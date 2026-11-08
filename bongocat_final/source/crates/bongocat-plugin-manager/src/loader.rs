//! One owned process with bounded request/response exchanges.
#[cfg(target_os = "windows")]
use bongocat_platform::PluginProcess;
#[cfg(all(test, unix))]
use bongocat_plugin_sdk::PROTOCOL_VERSION;
use bongocat_plugin_sdk::{
    Capability, HostMessage, PluginMetadata, PluginOutput, Request, Response, UiNode, read_frame,
    write_frame,
};
#[cfg(not(target_os = "windows"))]
use std::process::Child as PluginProcess;
use std::{
    collections::VecDeque,
    io,
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct WireStats {
    pub(crate) starts: std::sync::atomic::AtomicU64,
    pub(crate) read_bytes: std::sync::atomic::AtomicU64,
    pub(crate) written_bytes: std::sync::atomic::AtomicU64,
    pub(crate) requests: std::sync::atomic::AtomicU64,
    pub(crate) timeouts: std::sync::atomic::AtomicU64,
}
struct Counted<T> {
    stream: T,
    stats: Arc<WireStats>,
}
impl<T: io::Read> io::Read for Counted<T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let bytes = self.stream.read(buffer)?;
        self.stats
            .read_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(bytes)
    }
}
impl<T: io::Write> io::Write for Counted<T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let bytes = self.stream.write(buffer)?;
        self.stats
            .written_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

struct WireReply {
    sequence: u64,
    result: io::Result<Arc<PluginOutput>>,
}
impl From<Response> for WireReply {
    fn from(response: Response) -> Self {
        Self {
            sequence: response.sequence,
            result: response
                .result
                .map(Arc::new)
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message)),
        }
    }
}
fn plugin_failure(failure: bongocat_plugin_sdk::v2::Failure) -> io::Error {
    use bongocat_plugin_sdk::v2::ErrorCode;
    let kind = match failure.code {
        ErrorCode::Busy => io::ErrorKind::WouldBlock,
        ErrorCode::Timeout => io::ErrorKind::TimedOut,
        ErrorCode::UnsupportedCapability => io::ErrorKind::Unsupported,
        ErrorCode::InvalidConfiguration => io::ErrorKind::InvalidInput,
        ErrorCode::StorageFailure => io::ErrorKind::StorageFull,
        ErrorCode::ProtocolViolation => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, failure.message)
}

pub(crate) struct Process {
    child: PluginProcess,
    stats: Arc<WireStats>,
    writer: Option<SyncSender<Outbound>>,
    wake_deadline: Arc<Mutex<Option<Instant>>>,
    writer_failed: Arc<std::sync::atomic::AtomicBool>,
    subscriptions: Arc<Mutex<bongocat_plugin_sdk::v2::Subscriptions>>,
    written: Receiver<io::Result<()>>,
    writer_thread: Option<JoinHandle<()>>,
    responses: Receiver<io::Result<(u64, WireReply)>>,
    reader: Option<JoinHandle<()>>,
    sequence: u64,
    icon_text: bool,
    pending_push: Arc<Mutex<Option<(u64, WireReply)>>>,
    companion_menu: bool,
    menu_tree: bool,
    host_bubble: bool,
    wake_target: Arc<Mutex<thread::Thread>>,
}
impl Process {
    #[cfg(all(test, unix))]
    pub(crate) fn start(root: &Path, metadata: &PluginMetadata) -> io::Result<Self> {
        Self::start_with_events(
            root,
            metadata,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::default(),
        )
    }
    pub(crate) fn start_with_events(
        root: &Path,
        metadata: &PluginMetadata,
        events: Arc<Mutex<VecDeque<bongocat_plugin_sdk::PluginBubble>>>,
        stats: Arc<WireStats>,
    ) -> io::Result<Self> {
        // The package tree is `<plugin_dir>/packages/<id>/<version>`, so the
        // plugin-owned data directory is its fourth ancestor plus `data/<id>`.
        // Creating it here keeps the path contract in one place instead of
        // making every plugin reverse-engineer the store layout.
        let data_dir = root
            .ancestors()
            .nth(3)
            .map(|plugin_dir| plugin_dir.join(crate::DATA_DIRECTORY).join(&metadata.id))
            .unwrap_or_else(|| root.join("data"));
        std::fs::create_dir_all(&data_dir)?;
        #[cfg(target_os = "windows")]
        let mut child = PluginProcess::spawn(
            &entry_path(root, &metadata.entry),
            root,
            &data_dir,
            &std::env::current_exe()?.with_file_name(crate::WINDOWS_PLUGIN_BROKER_FILE),
        )?;
        #[cfg(target_os = "windows")]
        let input = child.stdin.take();
        #[cfg(not(target_os = "windows"))]
        let (mut child, input) = {
            let mut child = std::process::Command::new(entry_path(root, &metadata.entry))
                .current_dir(root)
                .env("BONGOCAT_PLUGIN_DATA_DIR", &data_dir)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            let input = child
                .stdin
                .take()
                .map(|input| Box::new(input) as Box<dyn io::Write + Send>);
            (child, input)
        };
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("plugin stdout unavailable"))?;
        stats
            .starts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut output = Counted {
            stream: output,
            stats: Arc::clone(&stats),
        };
        let (sender, responses) = mpsc::sync_channel(2);
        let wake_target = Arc::new(Mutex::new(thread::current()));
        let owner = Arc::clone(&wake_target);
        let protocol = metadata.protocol_version;
        let allows_events = metadata.capabilities.contains(&Capability::HostBubbleV1);
        let push = protocol == 2 || metadata.capabilities.contains(&Capability::PushOutputV1);
        let reader_events = Arc::clone(&events);
        let wake_deadline = Arc::new(Mutex::new(None));
        let reader_wake = Arc::clone(&wake_deadline);
        let subscriptions = Arc::new(Mutex::new(if protocol == 1 {
            bongocat_plugin_sdk::v2::Subscriptions::all()
        } else {
            bongocat_plugin_sdk::v2::Subscriptions::default()
        }));
        let reader_subscriptions = Arc::clone(&subscriptions);
        let writer_failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let input = input.ok_or_else(|| io::Error::other("plugin stdin unavailable"))?;
        let (writer, completions, writer_thread) = start_protocol_writer(
            Box::new(Counted {
                stream: input,
                stats: Arc::clone(&stats),
            }),
            protocol,
            Arc::clone(&writer_failed),
            Arc::clone(&wake_target),
        );
        let ack_writer = writer.clone();
        let pending_push = Arc::new(Mutex::new(None));
        let reader_push = Arc::clone(&pending_push);
        let reader = thread::spawn(move || {
            let mut order = 0u64;
            let mut session = bongocat_plugin_sdk::v2::HostSession::default();
            loop {
                let response = if protocol == 1 {
                    read_frame::<Response>(&mut output).map(WireReply::from)
                } else {
                    use bongocat_plugin_sdk::v2::{HostEffect, PluginFrame};
                    match read_frame::<PluginFrame>(&mut output)
                        .and_then(|frame| session.accept(frame))
                    {
                        Ok(HostEffect::Reply { sequence, result }) => Ok(WireReply {
                            sequence,
                            result: result.map_err(plugin_failure),
                        }),
                        Ok(HostEffect::Snapshot(output)) => Ok(WireReply {
                            sequence: 0,
                            result: Ok(output),
                        }),
                        Ok(HostEffect::Event { id, bubble }) => {
                            let mut events = reader_events
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if !allows_events {
                                Err(io::Error::other("undeclared host bubble capability"))
                            } else if events.len() >= 256
                                || events
                                    .iter()
                                    .map(bongocat_plugin_sdk::v2::event_bytes)
                                    .sum::<usize>()
                                    .saturating_add(bongocat_plugin_sdk::v2::event_bytes(&bubble))
                                    > bongocat_plugin_sdk::v2::EVENT_BYTES_LIMIT
                            {
                                if session.event_retry_enabled() {
                                    session.forget_event(id);
                                    if ack_writer.try_send(Outbound::EventRejected(id)).is_err() {
                                        break;
                                    }
                                    continue;
                                }
                                Err(io::Error::new(
                                    io::ErrorKind::WouldBlock,
                                    "plugin event queue is full",
                                ))
                            } else {
                                events.push_back(bubble);
                                drop(events);
                                if ack_writer.try_send(Outbound::Ack(id)).is_err() {
                                    Err(io::Error::other("event acknowledgement queue is full"))
                                } else {
                                    owner
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                                        .unpark();
                                    continue;
                                }
                            }
                        }
                        Ok(HostEffect::DuplicateEvent(id)) => {
                            if ack_writer.try_send(Outbound::Ack(id)).is_err() {
                                Err(io::Error::other("event acknowledgement queue is full"))
                            } else {
                                continue;
                            }
                        }
                        Ok(HostEffect::RetryEvent(id)) => {
                            if ack_writer.try_send(Outbound::EventRejected(id)).is_err() {
                                Err(io::Error::other("event rejection queue is full"))
                            } else {
                                continue;
                            }
                        }
                        Ok(HostEffect::Subscriptions(subscriptions)) => {
                            *reader_subscriptions
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) = subscriptions;
                            continue;
                        }
                        Ok(HostEffect::Wake(delay)) => {
                            *reader_wake
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                delay.map(|ms| Instant::now() + Duration::from_millis(ms));
                            owner
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .unpark();
                            continue;
                        }
                        Err(error) => Err(error),
                    }
                };
                order = order.saturating_add(1);
                let failed = response.is_err();
                let update = push
                    && response
                        .as_ref()
                        .is_ok_and(|response| response.sequence == 0);
                // Pushes are latest-value state; keep request replies in their own
                // bounded queue so a burst cannot displace a requested reply.
                if update {
                    *reader_push
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        response.ok().map(|response| (order, response));
                    owner
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .unpark();
                    continue;
                }
                // Unsolicited output cannot block process cleanup or grow memory.
                let full = sender
                    .try_send(response.map(|response| (order, response)))
                    .is_err();
                owner
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .unpark();
                if full || failed {
                    break;
                }
            }
        });
        let mut process = Self {
            child,
            stats,
            wake_deadline,
            writer_failed,
            subscriptions,
            writer: Some(writer),
            written: completions,
            writer_thread: Some(writer_thread),
            responses,
            reader: Some(reader),
            sequence: 0,
            pending_push,
            icon_text: metadata.capabilities.contains(&Capability::IconTextV1),
            companion_menu: metadata.capabilities.contains(&Capability::CompanionV1),
            menu_tree: metadata
                .capabilities
                .contains(&Capability::PluginMenuTreeV1),
            host_bubble: metadata.capabilities.contains(&Capability::HostBubbleV1),
            wake_target,
        };
        process.exchange(HostMessage::Handshake {
            protocol_version: metadata.protocol_version,
            id: metadata.id.clone(),
        })?;
        Ok(process)
    }
    pub(crate) fn rebind_thread(&mut self) {
        *self
            .wake_target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = thread::current();
    }
    pub(crate) fn exchange(&mut self, message: HostMessage) -> io::Result<Arc<PluginOutput>> {
        self.stats
            .requests
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("sequence exhausted"))?;
        // A dedicated writer is reused across requests. Killing a non-reading
        // child on timeout unblocks the writer before process cleanup joins it.
        // Starting native UI runtimes under load can take longer than an
        // ordinary tick. Keep every exchange bounded, with separate budgets.
        let budget = match &message {
            HostMessage::Handshake { .. }
            | HostMessage::Start { .. }
            | HostMessage::Restore { .. } => Duration::from_secs(5),
            HostMessage::Stop => Duration::from_secs(3),
            _ => Duration::from_secs(2),
        };
        let request = Request {
            sequence: self.sequence,
            message,
        };
        let deadline = Instant::now() + budget;
        self.writer
            .as_ref()
            .ok_or_else(|| io::Error::other("plugin stopped"))?
            .try_send(Outbound::Request(Box::new(request), deadline))
            .map_err(|_| io::Error::other("plugin writer unavailable"))?;
        let completed = self
            .written
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
        if matches!(completed, Err(mpsc::RecvTimeoutError::Timeout)) {
            self.stats
                .timeouts
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if completed.is_err() {
            let _ = self.child.kill();
        }
        let result = completed.map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                io::Error::new(io::ErrorKind::TimedOut, "plugin write deadline")
            }
            mpsc::RecvTimeoutError::Disconnected => io::Error::other("plugin writer stopped"),
        })?;
        result?;
        let (reply_order, response) = self
            .responses
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| {
                self.stats
                    .timeouts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                io::Error::new(io::ErrorKind::TimedOut, "plugin response deadline")
            })??;
        // A push sent before this reply cannot replace the newer reply. A push
        // sent after it stays queued for the next service update.
        let stale = take_push_before_reply(&self.pending_push, reply_order);
        if let Some(response) = stale {
            self.validate_update(response)?;
        }
        if response.sequence != self.sequence {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "plugin response sequence mismatch",
            ));
        }
        // A valid refusal consumed its sequence and leaves the protocol usable.
        // Transport/protocol errors, unlike invalid user configuration, are fatal.
        let output = response.result?;
        if !output.validate() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "plugin output exceeds limits",
            ));
        }
        self.validate_capabilities(&output)?;
        Ok(output)
    }
    fn validate_update(&self, response: WireReply) -> io::Result<Arc<PluginOutput>> {
        let output = response.result.map_err(io::Error::other)?;
        if response.sequence != 0 || !output.validate() {
            return Err(io::Error::other("invalid plugin update"));
        }
        self.validate_capabilities(&output)?;
        Ok(output)
    }
    fn validate_capabilities(&self, output: &PluginOutput) -> io::Result<()> {
        if !self.icon_text
            && output
                .model
                .nodes
                .iter()
                .chain(&output.settings.nodes)
                .any(|node| matches!(node, UiNode::IconText { .. }))
        {
            return Err(io::Error::other(
                "plugin published undeclared icon_text_v1 nodes",
            ));
        }
        if !self.companion_menu && (!output.menu.is_empty() || !output.menu_tree.is_empty()) {
            return Err(io::Error::other(
                "plugin published undeclared companion menu entries",
            ));
        }
        if !self.menu_tree && !output.menu_tree.is_empty() {
            return Err(io::Error::other(
                "plugin published an undeclared plugin_menu_tree_v1",
            ));
        }
        if !self.host_bubble && !output.bubbles.is_empty() {
            return Err(io::Error::other("plugin published undeclared host bubbles"));
        }
        Ok(())
    }
    pub(crate) fn acknowledge_checkpoint(&self, revision: u64) -> io::Result<()> {
        if revision == 0 {
            return Ok(());
        }
        self.writer
            .as_ref()
            .ok_or_else(|| io::Error::other("plugin stopped"))?
            .try_send(Outbound::CheckpointAck(revision))
            .map_err(|_| io::Error::other("checkpoint acknowledgement queue is full"))
    }
    pub(crate) fn subscribed(&self, capability: Capability) -> bool {
        let subscriptions = self
            .subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match capability {
            Capability::InputActivityV1 => subscriptions.input_activity,
            Capability::InputEventsV1 => subscriptions.input_events,
            Capability::PetStateV1 => subscriptions.pet_state,
            Capability::CompanionV1 => subscriptions.pet_bounds,
            _ => true,
        }
    }
    pub(crate) fn next_wake(&self) -> Option<Instant> {
        *self
            .wake_deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub(crate) fn take_due_wake(&self, now: Instant) -> bool {
        let mut deadline = self
            .wake_deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if deadline.is_some_and(|deadline| deadline <= now) {
            deadline.take();
            true
        } else {
            false
        }
    }
    pub(crate) fn update(&mut self) -> io::Result<Option<Arc<PluginOutput>>> {
        if self
            .writer_failed
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(io::Error::other("plugin writer stopped"));
        }
        let push = self
            .pending_push
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let mut latest = push
            .map(|(_, response)| self.validate_update(response))
            .transpose()?;
        for _ in 0..2 {
            match self.responses.try_recv() {
                Ok(response) => {
                    latest = Some(self.validate_update(response?.1)?);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(io::Error::other("plugin reader stopped"));
                }
            }
        }
        Ok(latest)
    }
    pub(crate) fn stop(&mut self) -> io::Result<Arc<PluginOutput>> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let output = self.exchange(HostMessage::Stop);
        self.writer.take();
        while self.child.try_wait()?.is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let status = self.child.try_wait()?;
        if status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "plugin shutdown deadline",
            ));
        }
        if !status.is_some_and(|status| status.success()) {
            return Err(io::Error::other("plugin exited unsuccessfully"));
        }
        output
    }
}

enum Outbound {
    // Keep the acknowledgement ring compact. An inline HostMessage would make
    // every reserved ACK slot as large as the biggest input/config request.
    Request(Box<Request>, Instant),
    Ack(u64),
    EventRejected(u64),
    CheckpointAck(u64),
}

fn start_protocol_writer(
    mut input: Box<dyn io::Write + Send>,
    protocol: u32,
    failed: Arc<std::sync::atomic::AtomicBool>,
    wake_target: Arc<Mutex<thread::Thread>>,
) -> (
    SyncSender<Outbound>,
    Receiver<io::Result<()>>,
    JoinHandle<()>,
) {
    // Control admission remains 64. Reserve space for 256 event and 256
    // checkpoint acknowledgements while the peer is publishing a bounded burst.
    let (writer, requests) = mpsc::sync_channel::<Outbound>(576);
    let (written, completions) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        while let Ok(job) = requests.recv() {
            let is_request = matches!(job, Outbound::Request(..));
            let result = match job {
                Outbound::Request(_, deadline) if Instant::now() >= deadline => Err(
                    io::Error::new(io::ErrorKind::TimedOut, "request expired before writing"),
                ),
                Outbound::Request(request, _) if protocol == 1 => write_frame(&mut input, &request),
                Outbound::Request(request, deadline) => {
                    use bongocat_plugin_sdk::v2::HostFrame;
                    let frame = match request.message {
                        HostMessage::Handshake {
                            protocol_version,
                            id,
                        } => HostFrame::Hello {
                            sequence: request.sequence,
                            protocol_version,
                            id,
                            supported: bongocat_plugin_sdk::v2::CAPABILITIES
                                .iter()
                                .copied()
                                .chain([bongocat_plugin_sdk::v2::EVENT_RETRY_CAPABILITY])
                                .map(str::to_owned)
                                .collect(),
                            max_message_bytes: bongocat_plugin_sdk::MAX_MESSAGE_BYTES,
                        },
                        message => HostFrame::Command {
                            sequence: request.sequence,
                            budget_ms: deadline
                                .saturating_duration_since(Instant::now())
                                .as_millis()
                                .clamp(1, 5000) as u64,
                            message,
                        },
                    };
                    write_frame(&mut input, &frame)
                }
                Outbound::EventRejected(id) => write_frame(
                    &mut input,
                    &bongocat_plugin_sdk::v2::HostFrame::EventRejected { id },
                ),
                Outbound::Ack(id) => {
                    write_frame(&mut input, &bongocat_plugin_sdk::v2::HostFrame::Ack { id })
                }
                Outbound::CheckpointAck(revision) => write_frame(
                    &mut input,
                    &bongocat_plugin_sdk::v2::HostFrame::CheckpointAck { revision },
                ),
            };
            let error = result.is_err();
            if error {
                failed.store(true, std::sync::atomic::Ordering::Release);
                wake_target
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .unpark();
            }
            if (is_request && written.send(result).is_err()) || error {
                break;
            }
        }
    });
    (writer, completions, worker)
}

#[cfg(test)]
fn start_writer(
    input: Box<dyn io::Write + Send>,
) -> (
    SyncSender<Outbound>,
    Receiver<io::Result<()>>,
    JoinHandle<()>,
) {
    start_protocol_writer(
        input,
        1,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(Mutex::new(thread::current())),
    )
}
impl Drop for Process {
    fn drop(&mut self) {
        self.writer.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(writer) = self.writer_thread.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn take_push_before_reply(
    pending: &Mutex<Option<(u64, WireReply)>>,
    reply_order: u64,
) -> Option<WireReply> {
    let mut pending = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if pending
        .as_ref()
        .is_some_and(|(order, _)| *order < reply_order)
    {
        pending.take().map(|(_, response)| response)
    } else {
        None
    }
}

/// The file a manifest's `entry` names inside its package.
///
/// The manifest names the executable without a platform suffix, and a Windows
/// package always carries the `.exe` the toolchain writes, so the suffix is the
/// platform's fact rather than something a package may omit.
pub(crate) fn entry_path(root: &Path, entry: &str) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    return root.join(format!("{entry}.exe"));
    #[cfg(not(target_os = "windows"))]
    root.join(entry)
}

#[cfg(test)]
mod ordered_push_tests {
    use super::*;

    #[test]
    fn writer_reuses_one_stream_for_multiple_requests_and_closes() {
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl io::Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let (writer, written, worker) = start_writer(Box::new(Capture(Arc::clone(&bytes))));
        for sequence in 1..=2 {
            writer
                .send(Outbound::Request(
                    Box::new(Request {
                        sequence,
                        message: HostMessage::Tick {
                            elapsed_ms: sequence,
                        },
                    }),
                    Instant::now() + Duration::from_secs(2),
                ))
                .unwrap();
            written
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap();
        }
        drop(writer);
        worker.join().unwrap();
        let bytes = bytes.lock().unwrap();
        let mut cursor = io::Cursor::new(bytes.as_slice());
        let first: Request = read_frame(&mut cursor).unwrap();
        let second: Request = read_frame(&mut cursor).unwrap();
        assert_eq!((first.sequence, second.sequence), (1, 2));
        assert_eq!(cursor.position() as usize, bytes.len());
    }

    #[test]
    fn writer_reports_pipe_failure_and_exits() {
        struct BrokenPipe;
        impl io::Write for BrokenPipe {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (writer, written, worker) = start_writer(Box::new(BrokenPipe));
        writer
            .send(Outbound::Request(
                Box::new(Request {
                    sequence: 1,
                    message: HostMessage::Stop,
                }),
                Instant::now() + Duration::from_secs(2),
            ))
            .unwrap();
        assert_eq!(
            written
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        worker.join().unwrap();
        assert!(
            writer
                .send(Outbound::Request(
                    Box::new(Request {
                        sequence: 2,
                        message: HostMessage::Stop
                    }),
                    Instant::now() + Duration::from_secs(2)
                ))
                .is_err()
        );
    }

    #[test]
    fn response_supersedes_older_push_but_preserves_later_push() {
        let update = || WireReply {
            sequence: 0,
            result: Ok(Arc::new(PluginOutput::default())),
        };
        let pending = Mutex::new(Some((2, update())));
        assert!(take_push_before_reply(&pending, 3).is_some());
        assert!(pending.lock().unwrap().is_none());
        *pending.lock().unwrap() = Some((4, update()));
        assert!(take_push_before_reply(&pending, 3).is_none());
        assert_eq!(pending.lock().unwrap().as_ref().unwrap().0, 4);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn metadata() -> PluginMetadata {
        let mut metadata = PluginMetadata {
            id: "com.bongocat.test".into(),
            name: "Test plugin".into(),
            description: "A test fixture plugin.".into(),
            version: "1.0.0".into(),
            app_versions: ">=2.3.1".into(),
            protocol_version: PROTOCOL_VERSION,
            entry: "plugin".into(),
            icon: bongocat_plugin_sdk::PluginIcon::Emoji("A".into()),
            capabilities: Vec::new(),
            localizations: Default::default(),
        };
        metadata.entry = "plugin".into();
        metadata
    }

    fn script(body: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plugin");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        directory
    }

    fn frame(sequence: u64) -> String {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &Response {
                sequence,
                result: Ok(Arc::new(PluginOutput::default())),
            },
        )
        .unwrap();
        bytes.iter().map(|byte| format!("\\{byte:03o}")).collect()
    }

    #[test]
    fn push_updates_do_not_consume_request_sequences() {
        let fixture = script(&format!(
            "printf '{}'\nsleep 0.05\nprintf '{}{}'\nexec sleep 10",
            frame(1),
            frame(0),
            frame(2)
        ));
        let mut metadata = metadata();
        metadata.capabilities.push(Capability::PushOutputV1);
        let mut process = Process::start(fixture.path(), &metadata).unwrap();
        process
            .exchange(HostMessage::Tick { elapsed_ms: 1 })
            .unwrap();
        assert_eq!(process.sequence, 2);
    }

    #[test]
    fn unsolicited_updates_require_the_capability() {
        let fixture = script(&format!("printf '{}{}'\nexec sleep 10", frame(1), frame(0)));
        let mut process = Process::start(fixture.path(), &metadata()).unwrap();
        assert!(
            process
                .exchange(HostMessage::Tick { elapsed_ms: 1 })
                .is_err()
        );
    }

    #[test]
    fn inline_artwork_requires_the_manifest_capability() {
        let output = PluginOutput {
            model: bongocat_plugin_sdk::PluginView {
                nodes: vec![UiNode::IconText {
                    icon: bongocat_plugin_sdk::StatusIcon::Badge("A".into()),
                    text: "ABC".into(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &Response {
                sequence: 2,
                result: Ok(output),
            },
        )
        .unwrap();
        let response: String = bytes.iter().map(|byte| format!("\\{byte:03o}")).collect();
        let fixture = script(&format!("printf '{}{response}'\nexec sleep 10", frame(1)));
        for declared in [false, true] {
            let mut metadata = metadata();
            if declared {
                metadata.capabilities.push(Capability::IconTextV1);
            }
            let mut process = Process::start(fixture.path(), &metadata).unwrap();
            let result = process.exchange(HostMessage::Start {
                config: Default::default(),
                state: vec![],
            });
            assert_eq!(result.is_ok(), declared);
        }
    }

    #[test]
    fn malformed_sequences_and_nonresponsive_children_fail_closed() {
        let metadata = metadata();
        let wrong_sequence = script(&format!("printf '{}'\nexec sleep 10", frame(99)));
        assert!(Process::start(wrong_sequence.path(), &metadata).is_err());
        let unresponsive = script("exec sleep 10");
        let error = Process::start(unresponsive.path(), &metadata)
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn stop_requires_a_successful_exit_not_only_a_valid_response() {
        let metadata = metadata();
        let request_size = |sequence, message| {
            let mut bytes = Vec::new();
            write_frame(&mut bytes, &Request { sequence, message }).unwrap();
            bytes.len()
        };
        let handshake = request_size(
            1,
            HostMessage::Handshake {
                protocol_version: PROTOCOL_VERSION,
                id: metadata.id.clone(),
            },
        );
        let stop = request_size(2, HostMessage::Stop);
        let directory = script(&format!(
            "dd bs=1 count={handshake} >/dev/null 2>/dev/null\nprintf '{}'\ndd bs=1 count={stop} >/dev/null 2>/dev/null\nprintf '{}'\nexit 7",
            frame(1),
            frame(2)
        ));
        let mut process = Process::start(directory.path(), &metadata).unwrap();
        let error = process.stop().unwrap_err();
        assert_eq!(error.to_string(), "plugin exited unsuccessfully");
    }
}
