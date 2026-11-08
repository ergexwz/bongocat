use super::*;
use bongocat_plugin_sdk::v2::{self, ErrorCode, Failure, HostFrame, PluginFrame, Publisher};
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

pub fn run_host_v2<S: CompanionState>(state: Arc<Mutex<S>>, ui: UiBridge) {
    thread::Builder::new()
        .name("companion-v2".into())
        .spawn(move || {
            let result = host_loop(&state, &ui);
            if result.is_err() {
                ui.emit(UiEvent::Stop);
            }
        })
        .expect("companion protocol worker");
}

fn host_loop<S: CompanionState>(state: &Arc<Mutex<S>>, ui: &UiBridge) -> io::Result<()> {
    let owner = thread::current();
    *ui.protocol_waker
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(owner.clone());
    let (sender, incoming) = sync_channel(64);
    let reader = thread::Builder::new()
        .name("companion-v2-reader".into())
        .spawn(move || {
            let mut input = io::stdin().lock();
            loop {
                let frame = read_frame::<HostFrame>(&mut input);
                let failed = frame.is_err();
                if sender.send((Instant::now(), frame)).is_err() {
                    break;
                }
                owner.unpark();
                if failed {
                    break;
                }
            }
        })?;
    let mut output = io::stdout().lock();
    let mut publisher = Publisher::default();
    let mut ready = false;
    let mut last_sequence = 0;
    let mut stopped = false;
    while !stopped {
        if let Some(frame) = publisher.retry_event() {
            write_frame(&mut output, &frame)?;
        }
        match incoming.try_recv() {
            Ok((received, frame)) => match frame? {
                HostFrame::Hello {
                    sequence,
                    protocol_version,
                    id,
                    supported,
                    max_message_bytes,
                } => {
                    let expected = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .id();
                    if ready
                        || protocol_version != 2
                        || id != expected
                        || sequence == 0
                        || max_message_bytes < bongocat_plugin_sdk::MAX_MESSAGE_BYTES
                        || v2::CAPABILITIES
                            .iter()
                            .any(|cap| !supported.iter().any(|s| s == cap))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "incompatible host capabilities",
                        ));
                    }
                    ready = true;
                    last_sequence = sequence;
                    let optional: Vec<String> = supported
                        .into_iter()
                        .filter(|cap| cap == v2::EVENT_RETRY_CAPABILITY)
                        .collect();
                    let selected: Vec<String> = v2::CAPABILITIES
                        .iter()
                        .map(|cap| cap.to_string())
                        .chain(optional.iter().cloned())
                        .collect();
                    publisher.set_capabilities(&selected);
                    write_frame(
                        &mut output,
                        &PluginFrame::Welcome {
                            sequence,
                            required: v2::CAPABILITIES.iter().map(|s| s.to_string()).collect(),
                            optional,
                        },
                    )?;
                }
                HostFrame::EventRejected { id } if ready => publisher.reject_event(id)?,
                HostFrame::Ack { id } if ready => {
                    if let Some(business_id) = publisher.acknowledge(id) {
                        state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .event_acknowledged(&business_id);
                    }
                }
                HostFrame::CheckpointAck { revision } if ready => {
                    if publisher.acknowledge_checkpoint(revision) {
                        state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .checkpoint_committed(revision);
                    }
                }
                HostFrame::Command {
                    sequence,
                    budget_ms,
                    message,
                } if ready => {
                    if sequence <= last_sequence || !(1..=5000).contains(&budget_ms) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid request sequence or budget",
                        ));
                    }
                    last_sequence = sequence;
                    stopped = matches!(message, HostMessage::Stop);
                    if matches!(
                        message,
                        HostMessage::Tick { .. }
                            | HostMessage::Action { .. }
                            | HostMessage::Configure { .. }
                            | HostMessage::Restore { .. }
                            | HostMessage::Reset
                            | HostMessage::Pause
                            | HostMessage::Resume
                    ) {
                        publisher.reset_wake();
                    }
                    let result = if received.elapsed() > Duration::from_millis(budget_ms) {
                        Err(Failure {
                            code: ErrorCode::Timeout,
                            message: "request expired in queue".into(),
                        })
                    } else {
                        let handled = if stopped {
                            state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .stop()
                        } else {
                            super::handle_message(state, ui, &mut ready, message)
                        };
                        handled.map_err(|message| Failure {
                            code: ErrorCode::InvalidConfiguration,
                            message,
                        })
                    };
                    let result = result.and_then(|mut snapshot| {
                        if snapshot.state_schema_version == 0 {
                            snapshot.state_schema_version = 1;
                        }
                        let wake = state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .next_wake_ms();
                        publisher.set_subscriptions(
                            state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .subscriptions(),
                        );
                        publish(&mut output, &mut publisher, snapshot, wake).map_err(|error| {
                            Failure {
                                code: if error.kind() == io::ErrorKind::WouldBlock {
                                    ErrorCode::Busy
                                } else {
                                    ErrorCode::ProtocolViolation
                                },
                                message: error.to_string(),
                            }
                        })
                    });
                    write_frame(&mut output, &PluginFrame::Reply { sequence, result })?;
                    if stopped {
                        ui.emit(UiEvent::Stop);
                    }
                }
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "hello required")),
            },
            Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {
                if ready && ui.protocol_dirty.swap(false, Ordering::AcqRel) {
                    let (mut snapshot, subscriptions, wake) = {
                        let state = state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        (
                            state.output().map_err(io::Error::other)?,
                            state.subscriptions(),
                            state.next_wake_ms(),
                        )
                    };
                    if snapshot.state_schema_version == 0 {
                        snapshot.state_schema_version = 1;
                    }
                    publisher.set_subscriptions(subscriptions);
                    publish(&mut output, &mut publisher, snapshot, wake)?;
                } else {
                    match publisher.retry_wait() {
                        Some(wait) => thread::park_timeout(wait),
                        None => thread::park(),
                    }
                }
            }
        }
    }
    // Host closes stdin after the Stop reply, allowing the bounded reader to exit.
    drop(incoming);
    let _ = reader.join();
    Ok(())
}

fn publish(
    output: &mut impl io::Write,
    publisher: &mut Publisher,
    snapshot: PluginOutput,
    wake: Option<u64>,
) -> io::Result<()> {
    for frame in publisher.publication(snapshot, wake)? {
        write_frame(output, &frame)?;
    }
    Ok(())
}
