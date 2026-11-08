//! Protocol v2: negotiated extensions, independently published regions and events.
use crate::{
    Configuration, HostMessage, PluginBubble, PluginError, PluginMenuItem, PluginMenuNode,
    PluginOutput, PluginView,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    io,
};

pub const PROTOCOL_VERSION: u32 = 2;
pub const CAPABILITIES: [&str; 5] = [
    "regions.v2",
    "events.v2",
    "schedule.v2",
    "checkpoint.v2",
    "subscriptions.v2",
];
pub const EVENT_LIMIT: usize = 256;
pub const EVENT_BYTES_LIMIT: usize = 1024 * 1024;
pub const EVENT_RETRY_CAPABILITY: &str = "event_retry.v2";

pub fn event_bytes(bubble: &PluginBubble) -> usize {
    serde_json::to_vec(bubble).map_or(usize::MAX, |bytes| bytes.len())
}

#[derive(Clone, Debug)]
pub struct CapabilityRequirements {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}
impl Default for CapabilityRequirements {
    fn default() -> Self {
        Self {
            required: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            optional: vec![EVENT_RETRY_CAPABILITY.into()],
        }
    }
}
pub fn negotiate(
    requirements: &CapabilityRequirements,
    offered: &[String],
) -> Result<Vec<String>, Failure> {
    let names: Vec<_> = requirements
        .required
        .iter()
        .chain(&requirements.optional)
        .collect();
    if names.len() > 32
        || names
            .iter()
            .any(|name| name.is_empty() || name.len() > 128 || name.chars().any(char::is_control))
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
    {
        return Err(Failure {
            code: ErrorCode::ProtocolViolation,
            message: "invalid capability requirements".into(),
        });
    }
    if requirements
        .required
        .iter()
        .any(|cap| !offered.contains(cap))
    {
        return Err(Failure {
            code: ErrorCode::UnsupportedCapability,
            message: "required capability is unavailable".into(),
        });
    }
    Ok(requirements
        .required
        .iter()
        .chain(&requirements.optional)
        .filter(|cap| offered.contains(cap))
        .cloned()
        .collect())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostFrame {
    Hello {
        sequence: u64,
        protocol_version: u32,
        id: String,
        supported: Vec<String>,
        max_message_bytes: usize,
    },
    Command {
        sequence: u64,
        budget_ms: u64,
        message: HostMessage,
    },
    Ack {
        id: u64,
    },
    CheckpointAck {
        revision: u64,
    },
    EventRejected {
        id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Busy,
    Timeout,
    UnsupportedCapability,
    InvalidConfiguration,
    StorageFailure,
    ProtocolViolation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginFrame {
    Welcome {
        sequence: u64,
        required: Vec<String>,
        optional: Vec<String>,
    },
    Reply {
        sequence: u64,
        result: Result<(), Failure>,
    },
    Snapshot {
        revision: u64,
        patch: OutputPatch,
    },
    Event {
        id: u64,
        bubble: PluginBubble,
    },
    Wake {
        after_ms: Option<u64>,
    },
    Subscriptions {
        subscriptions: Subscriptions,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscriptions {
    pub input_activity: bool,
    pub input_events: bool,
    pub pet_state: bool,
    pub pet_bounds: bool,
}
impl Subscriptions {
    pub fn all() -> Self {
        Self {
            input_activity: true,
            input_events: true,
            pet_state: true,
            pet_bounds: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<PluginView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<PluginView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Configuration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Checkpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<PluginError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu: Option<Vec<PluginMenuItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu_tree: Option<Vec<PluginMenuNode>>,
}
impl OutputPatch {
    pub fn between(previous: &PluginOutput, current: &PluginOutput) -> Self {
        Self {
            model: (previous.model != current.model).then(|| current.model.clone()),
            settings: (previous.settings != current.settings).then(|| current.settings.clone()),
            config: (previous.config != current.config).then(|| current.config.clone()),
            checkpoint: (previous.state != current.state
                || previous.state_schema_version != current.state_schema_version)
                .then(|| Checkpoint {
                    schema_version: current.state_schema_version,
                    data: current.state.clone(),
                }),
            errors: (previous.errors != current.errors).then(|| current.errors.clone()),
            menu: (previous.menu != current.menu).then(|| current.menu.clone()),
            menu_tree: (previous.menu_tree != current.menu_tree).then(|| current.menu_tree.clone()),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.settings.is_none()
            && self.config.is_none()
            && self.checkpoint.is_none()
            && self.errors.is_none()
            && self.menu.is_none()
            && self.menu_tree.is_none()
    }
    fn apply(self, output: &mut PluginOutput) {
        if let Some(value) = self.model {
            output.model = value;
        }
        if let Some(value) = self.settings {
            output.settings = value;
        }
        if let Some(value) = self.config {
            output.config = value;
        }
        if let Some(value) = self.checkpoint {
            output.state = value.data;
            output.state_schema_version = value.schema_version;
        }
        if let Some(value) = self.errors {
            output.errors = value;
        }
        if let Some(value) = self.menu {
            output.menu = value;
        }
        if let Some(value) = self.menu_tree {
            output.menu_tree = value;
        }
    }
}

pub enum HostEffect {
    Reply {
        sequence: u64,
        result: Result<std::sync::Arc<PluginOutput>, Failure>,
    },
    Snapshot(std::sync::Arc<PluginOutput>),
    Event {
        id: u64,
        bubble: PluginBubble,
    },
    Wake(Option<u64>),
    DuplicateEvent(u64),
    RetryEvent(u64),
    Subscriptions(Subscriptions),
}

#[derive(Default)]
pub struct HostSession {
    ready: bool,
    revision: u64,
    output: std::sync::Arc<PluginOutput>,
    last_event_id: u64,
    negotiated: BTreeSet<String>,
}
impl HostSession {
    pub fn event_retry_enabled(&self) -> bool {
        self.negotiated.contains(EVENT_RETRY_CAPABILITY)
    }
    pub fn forget_event(&mut self, id: u64) {
        // Admission is serialized by the reader; only the latest candidate can
        // be rejected. Advancing this frontier is the queue admission commit.
        if id != 0 && self.last_event_id == id {
            self.last_event_id -= 1;
        }
    }
    pub fn accept(&mut self, frame: PluginFrame) -> io::Result<HostEffect> {
        if let PluginFrame::Welcome {
            sequence,
            required,
            optional,
        } = frame
        {
            if self.ready || sequence == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "incompatible plugin capabilities",
                ));
            }
            self.negotiated = negotiate(
                &CapabilityRequirements { required, optional },
                &CAPABILITIES
                    .iter()
                    .copied()
                    .chain([EVENT_RETRY_CAPABILITY])
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
            .map_err(|failure| io::Error::new(io::ErrorKind::Unsupported, failure.message))?
            .into_iter()
            .collect();
            if !self.negotiated.contains("regions.v2") {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "regions.v2 is required",
                ));
            }
            self.ready = true;
            return Ok(HostEffect::Reply {
                sequence,
                result: Ok(self.output.clone()),
            });
        }
        if !self.ready {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "welcome required",
            ));
        }
        match frame {
            PluginFrame::Subscriptions { subscriptions } => {
                if !self.negotiated.contains("subscriptions.v2") {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "undeclared subscriptions",
                    ));
                }
                Ok(HostEffect::Subscriptions(subscriptions))
            }
            PluginFrame::Reply { sequence, result } => Ok(HostEffect::Reply {
                sequence,
                result: result.map(|()| self.output.clone()),
            }),
            PluginFrame::Snapshot { revision, patch } => {
                if revision <= self.revision || patch.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid snapshot revision",
                    ));
                }
                if patch.checkpoint.is_some() && !self.negotiated.contains("checkpoint.v2") {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "undeclared checkpoint",
                    ));
                }
                if patch
                    .checkpoint
                    .as_ref()
                    .is_some_and(|checkpoint| checkpoint.schema_version == 0)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "checkpoint version zero is reserved for legacy state",
                    ));
                }
                let mut candidate = self.output.as_ref().clone();
                if patch.checkpoint.is_some() {
                    candidate.checkpoint_revision = revision;
                }
                patch.apply(&mut candidate);
                if !candidate.validate() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid plugin snapshot",
                    ));
                }
                self.output = std::sync::Arc::new(candidate);
                self.revision = revision;
                Ok(HostEffect::Snapshot(self.output.clone()))
            }
            PluginFrame::Event { id, bubble } => {
                if !self.negotiated.contains("events.v2") || id == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid event"));
                }
                if id <= self.last_event_id {
                    return Ok(HostEffect::DuplicateEvent(id));
                }
                if Some(id) != self.last_event_id.checked_add(1) {
                    return if self.event_retry_enabled() {
                        Ok(HostEffect::RetryEvent(id))
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "event delivery gap",
                        ))
                    };
                }
                let output = PluginOutput {
                    bubbles: vec![bubble.clone()],
                    ..Default::default()
                };
                if !output.validate() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid event payload",
                    ));
                }
                self.last_event_id = id;
                Ok(HostEffect::Event { id, bubble })
            }
            PluginFrame::Wake { after_ms } => {
                if !self.negotiated.contains("schedule.v2")
                    || after_ms.is_some_and(|ms| !(34..=86_400_000).contains(&ms))
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid wake request",
                    ));
                }
                Ok(HostEffect::Wake(after_ms))
            }
            PluginFrame::Welcome { .. } => unreachable!(),
        }
    }
}

/// Stateful publisher shared by the SDK runner and companion bridge.
pub struct Publisher {
    previous: PluginOutput,
    revision: u64,
    seen: VecDeque<String>,
    unacknowledged: std::collections::BTreeMap<u64, PluginBubble>,
    last_event_id: u64,
    retries: VecDeque<u64>,
    next_retry: Option<std::time::Instant>,
    pending_checkpoints: VecDeque<u64>,
    pub committed_checkpoint_revision: u64,
    last_wake: Option<Option<u64>>,
    subscriptions: Subscriptions,
    last_subscriptions: Option<Subscriptions>,
    capabilities: BTreeSet<String>,
}
impl Default for Publisher {
    fn default() -> Self {
        Self {
            previous: PluginOutput::default(),
            revision: 0,
            seen: VecDeque::new(),
            unacknowledged: Default::default(),
            last_event_id: 0,
            retries: VecDeque::new(),
            next_retry: None,
            pending_checkpoints: VecDeque::new(),
            committed_checkpoint_revision: 0,
            last_wake: None,
            subscriptions: Subscriptions::default(),
            last_subscriptions: None,
            capabilities: CAPABILITIES
                .iter()
                .copied()
                .chain([EVENT_RETRY_CAPABILITY])
                .map(str::to_owned)
                .collect(),
        }
    }
}
impl Publisher {
    /// Set the Hello/Welcome intersection before the first publication.
    pub fn set_capabilities(&mut self, capabilities: &[String]) {
        self.capabilities = capabilities.iter().cloned().collect();
    }
    pub fn set_subscriptions(&mut self, subscriptions: Subscriptions) {
        self.subscriptions = subscriptions;
    }
    pub fn reset_wake(&mut self) {
        if matches!(self.last_wake, Some(Some(_))) {
            self.last_wake = None;
        }
    }
    pub fn acknowledge(&mut self, id: u64) -> Option<String> {
        let bubble = self.unacknowledged.remove(&id)?;
        self.retries.retain(|pending| *pending != id);
        if self.retries.is_empty() {
            self.next_retry = None;
        }
        Some(bubble.id)
    }
    pub fn reject_event(&mut self, id: u64) -> io::Result<()> {
        if !self.capabilities.contains(EVENT_RETRY_CAPABILITY)
            || !self.unacknowledged.contains_key(&id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid event rejection",
            ));
        }
        if !self.retries.contains(&id) {
            self.retries.push_back(id);
            self.next_retry.get_or_insert_with(|| {
                std::time::Instant::now() + std::time::Duration::from_millis(34)
            });
        }
        Ok(())
    }
    pub fn retry_wait(&self) -> Option<std::time::Duration> {
        self.next_retry
            .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()))
    }
    pub fn retry_event(&mut self) -> Option<PluginFrame> {
        if self.retry_wait()? > std::time::Duration::ZERO {
            return None;
        }
        let id = self.retries.pop_front()?;
        self.next_retry = if self.retries.is_empty() {
            None
        } else {
            Some(std::time::Instant::now() + std::time::Duration::from_millis(34))
        };
        self.unacknowledged
            .get(&id)
            .cloned()
            .map(|bubble| PluginFrame::Event { id, bubble })
    }
    pub fn acknowledge_checkpoint(&mut self, revision: u64) -> bool {
        if !self.pending_checkpoints.contains(&revision) {
            return false;
        }
        self.pending_checkpoints
            .retain(|pending| *pending > revision);
        self.committed_checkpoint_revision = self.committed_checkpoint_revision.max(revision);
        true
    }
    pub fn publication(
        &mut self,
        output: PluginOutput,
        wake: Option<u64>,
    ) -> io::Result<Vec<PluginFrame>> {
        if !output.validate() || wake.is_some_and(|ms| !(34..=86_400_000).contains(&ms)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid publication",
            ));
        }
        if (!self.capabilities.contains("events.v2") && !output.bubbles.is_empty())
            || (!self.capabilities.contains("checkpoint.v2") && !output.state.is_empty())
            || (!self.capabilities.contains("schedule.v2") && wake.is_some())
            || (!self.capabilities.contains("subscriptions.v2")
                && self.subscriptions != Subscriptions::default())
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "publication requires an unnegotiated capability",
            ));
        }
        let new_events: Vec<_> = output
            .bubbles
            .iter()
            .filter(|bubble| {
                !self.seen.contains(&bubble.id)
                    && !self
                        .unacknowledged
                        .values()
                        .any(|pending| pending.id == bubble.id)
            })
            .cloned()
            .collect();
        if self.unacknowledged.len() + new_events.len() > EVENT_LIMIT
            || self
                .unacknowledged
                .values()
                .chain(new_events.iter())
                .map(event_bytes)
                .sum::<usize>()
                > EVENT_BYTES_LIMIT
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "event acknowledgements are pending",
            ));
        }
        self.last_event_id
            .checked_add(new_events.len() as u64)
            .ok_or_else(|| io::Error::other("event sequence exhausted"))?;
        let mut patch = OutputPatch::between(&self.previous, &output);
        if !self.capabilities.contains("checkpoint.v2") {
            patch.checkpoint = None;
        }
        if patch
            .checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.schema_version == 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "checkpoint version zero is reserved for legacy state",
            ));
        }
        if patch.checkpoint.is_some() && self.pending_checkpoints.len() >= EVENT_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "checkpoint acknowledgements are pending",
            ));
        }
        let mut frames = Vec::new();
        if self.capabilities.contains("subscriptions.v2")
            && self.last_subscriptions != Some(self.subscriptions)
        {
            frames.push(PluginFrame::Subscriptions {
                subscriptions: self.subscriptions,
            });
            self.last_subscriptions = Some(self.subscriptions);
        }
        if !patch.is_empty() {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or_else(|| io::Error::other("revision exhausted"))?;
            if patch.checkpoint.is_some() {
                self.pending_checkpoints.push_back(self.revision);
            }
            frames.push(PluginFrame::Snapshot {
                revision: self.revision,
                patch,
            });
        }
        for bubble in new_events {
            self.last_event_id += 1;
            let id = self.last_event_id;
            self.seen.push_back(bubble.id.clone());
            self.unacknowledged.insert(id, bubble.clone());
            frames.push(PluginFrame::Event { id, bubble });
        }
        while self.seen.len() > 1024 {
            self.seen.pop_front();
        }
        self.previous = output;
        if self.capabilities.contains("schedule.v2") && self.last_wake != Some(wake) {
            frames.push(PluginFrame::Wake { after_ms: wake });
            self.last_wake = Some(wake);
        }
        Ok(frames)
    }
}

/// Minimal v2 plugin contract. Business code owns its state and migration.
pub trait Plugin: crate::Plugin {
    /// Clear temporary pressed/input state after a gap, retaining business data.
    fn reset_input(&mut self, _overflow: u64) {}
    /// Override when business failures need a specific v2 error code.
    fn handle_v2(&mut self, message: HostMessage) -> Result<PluginOutput, Failure> {
        self.handle(message).map_err(|message| Failure {
            code: ErrorCode::InvalidConfiguration,
            message,
        })
    }
    fn event_acknowledged(&mut self, _id: &str) {}
    fn checkpoint_committed(&mut self, _revision: u64) {}
    fn capabilities(&self) -> CapabilityRequirements {
        CapabilityRequirements::default()
    }
    fn negotiated(&mut self, _capabilities: &[String]) {}
    fn next_wake_ms(&self) -> Option<u64> {
        None
    }
    fn subscriptions(&self) -> Subscriptions {
        Subscriptions::default()
    }
}

/// Run a plugin without a UI framework or host-internal dependencies.
pub fn run(plugin: &mut impl Plugin) -> io::Result<()> {
    let (sender, incoming) = std::sync::mpsc::sync_channel(64);
    std::thread::Builder::new()
        .name("sdk-v2-reader".into())
        .spawn(move || {
            let mut input = io::stdin().lock();
            loop {
                let frame = crate::read_frame::<HostFrame>(&mut input);
                let failed = frame.is_err();
                if sender.send((std::time::Instant::now(), frame)).is_err() || failed {
                    break;
                }
            }
        })?;
    let mut output = io::stdout().lock();
    let mut publisher = Publisher::default();
    let mut ready = false;
    let mut last_sequence = 0;
    loop {
        if let Some(frame) = publisher.retry_event() {
            crate::write_frame(&mut output, &frame)?;
        }
        let (received, frame) = match publisher.retry_wait() {
            Some(wait) => match incoming.recv_timeout(wait) {
                Ok((received, frame)) => (received, frame?),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err(io::ErrorKind::UnexpectedEof.into()),
            },
            None => {
                let (received, frame) = incoming
                    .recv()
                    .map_err(|_| io::Error::from(io::ErrorKind::UnexpectedEof))?;
                (received, frame?)
            }
        };
        match frame {
            HostFrame::Hello {
                sequence,
                protocol_version,
                id,
                supported,
                max_message_bytes,
            } => {
                if ready
                    || sequence == 0
                    || protocol_version != 2
                    || id != plugin.id()
                    || max_message_bytes < crate::MAX_MESSAGE_BYTES
                {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "incompatible host",
                    ));
                }
                let requirements = plugin.capabilities();
                let selected = negotiate(&requirements, &supported).map_err(|failure| {
                    io::Error::new(io::ErrorKind::Unsupported, failure.message)
                })?;
                plugin.negotiated(&selected);
                publisher.set_capabilities(&selected);
                ready = true;
                last_sequence = sequence;
                crate::write_frame(
                    &mut output,
                    &PluginFrame::Welcome {
                        sequence,
                        required: requirements.required,
                        optional: requirements
                            .optional
                            .into_iter()
                            .filter(|cap| selected.contains(cap))
                            .collect(),
                    },
                )?;
            }
            HostFrame::EventRejected { id } if ready => publisher.reject_event(id)?,
            HostFrame::Ack { id } if ready => {
                if let Some(business_id) = publisher.acknowledge(id) {
                    plugin.event_acknowledged(&business_id);
                }
            }
            HostFrame::CheckpointAck { revision } if ready => {
                if publisher.acknowledge_checkpoint(revision) {
                    plugin.checkpoint_committed(revision);
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
                        "invalid request",
                    ));
                }
                last_sequence = sequence;
                if let HostMessage::InputEvents { overflow, .. } = &message
                    && *overflow > 0
                {
                    plugin.reset_input(*overflow);
                }
                let stop = matches!(message, HostMessage::Stop);
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
                let handled = if received.elapsed() >= std::time::Duration::from_millis(budget_ms) {
                    Err(Failure {
                        code: ErrorCode::Timeout,
                        message: "request expired in queue".into(),
                    })
                } else {
                    plugin.handle_v2(message)
                };
                let result = handled.and_then(|mut snapshot| {
                    if snapshot.state_schema_version == 0 {
                        snapshot.state_schema_version = 1;
                    }
                    publisher.set_subscriptions(plugin.subscriptions());
                    let frames = publisher
                        .publication(snapshot, plugin.next_wake_ms())
                        .map_err(|error| Failure {
                            code: if error.kind() == io::ErrorKind::WouldBlock {
                                ErrorCode::Busy
                            } else if error.kind() == io::ErrorKind::Unsupported {
                                ErrorCode::UnsupportedCapability
                            } else {
                                ErrorCode::ProtocolViolation
                            },
                            message: error.to_string(),
                        })?;
                    for frame in frames {
                        crate::write_frame(&mut output, &frame).map_err(|error| Failure {
                            code: ErrorCode::ProtocolViolation,
                            message: error.to_string(),
                        })?;
                    }
                    Ok(())
                });
                crate::write_frame(&mut output, &PluginFrame::Reply { sequence, result })?;
                if stop {
                    return Ok(());
                }
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "hello required")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_timer_requests_do_not_postpone_the_existing_deadline() {
        let mut publisher = Publisher::default();
        assert!(
            publisher
                .publication(PluginOutput::default(), Some(200))
                .unwrap()
                .iter()
                .any(|frame| matches!(frame, PluginFrame::Wake { .. }))
        );
        assert!(
            publisher
                .publication(PluginOutput::default(), Some(200))
                .unwrap()
                .iter()
                .all(|frame| !matches!(frame, PluginFrame::Wake { .. }))
        );
        publisher.reset_wake();
        assert!(
            publisher
                .publication(PluginOutput::default(), Some(200))
                .unwrap()
                .iter()
                .any(|frame| matches!(frame, PluginFrame::Wake { .. }))
        );
    }
    #[test]
    fn event_backpressure_is_released_only_by_acknowledgement() {
        let bubble = |n| PluginBubble {
            id: format!("event-{n}"),
            kind: crate::BubbleKind::Todo,
            source: "Fixture".into(),
            title: "Event".into(),
            subtitle: "Details".into(),
            urgent: false,
            action: "open".into(),
            auto_dismiss_ms: None,
        };
        let mut publisher = Publisher::default();
        for n in 0..EVENT_LIMIT {
            publisher
                .publication(
                    PluginOutput {
                        bubbles: vec![bubble(n)],
                        ..Default::default()
                    },
                    None,
                )
                .unwrap();
        }
        let next = PluginOutput {
            bubbles: vec![bubble(EVENT_LIMIT)],
            ..Default::default()
        };
        assert_eq!(
            publisher
                .publication(next.clone(), None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        publisher.acknowledge(1);
        assert!(
            publisher
                .publication(next, None)
                .unwrap()
                .iter()
                .any(|frame| matches!(frame, PluginFrame::Event { .. }))
        );
    }

    #[test]
    fn rejected_events_retain_their_id_and_payload_until_acknowledged() {
        let mut publisher = Publisher::default();
        let bubble = PluginBubble {
            id: "retry-1".into(),
            kind: crate::BubbleKind::Todo,
            source: "Fixture".into(),
            title: "Event".into(),
            subtitle: "Details".into(),
            urgent: false,
            action: "open".into(),
            auto_dismiss_ms: None,
        };
        publisher
            .publication(
                PluginOutput {
                    bubbles: vec![bubble.clone()],
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        publisher.reject_event(1).unwrap();
        assert!(publisher.retry_event().is_none());
        publisher.next_retry = Some(std::time::Instant::now());
        assert!(
            matches!(publisher.retry_event(), Some(PluginFrame::Event { id, bubble: retried }) if id == 1 && retried == bubble)
        );
        assert_eq!(publisher.acknowledge(1), Some(bubble.id.clone()));
        assert!(publisher.reject_event(1).is_err());
    }

    #[test]
    fn replies_share_the_current_snapshot_without_cloning_its_contents() {
        let mut session = HostSession::default();
        session
            .accept(PluginFrame::Welcome {
                sequence: 1,
                required: CAPABILITIES.iter().map(|cap| cap.to_string()).collect(),
                optional: Vec::new(),
            })
            .unwrap();
        let HostEffect::Reply {
            result: Ok(first), ..
        } = session
            .accept(PluginFrame::Reply {
                sequence: 2,
                result: Ok(()),
            })
            .unwrap()
        else {
            panic!("reply expected");
        };
        let HostEffect::Reply {
            result: Ok(second), ..
        } = session
            .accept(PluginFrame::Reply {
                sequence: 3,
                result: Ok(()),
            })
            .unwrap()
        else {
            panic!("reply expected");
        };
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }
    #[test]
    fn optional_capabilities_can_be_disabled_without_refusing_the_plugin() {
        let requirements = CapabilityRequirements {
            required: vec!["regions.v2".into()],
            optional: vec!["events.v2".into()],
        };
        assert_eq!(
            negotiate(&requirements, &["regions.v2".into()]).unwrap(),
            vec!["regions.v2"]
        );
        assert_eq!(
            negotiate(&requirements, &[]).unwrap_err().code,
            ErrorCode::UnsupportedCapability
        );
        let selected = negotiate(&requirements, &["regions.v2".into()]).unwrap();
        let mut publisher = Publisher::default();
        publisher.set_capabilities(&selected);
        let output = PluginOutput {
            state_schema_version: 1,
            model: PluginView {
                nodes: vec![crate::UiNode::Text {
                    text: "hello".into(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut host = HostSession::default();
        host.accept(PluginFrame::Welcome {
            sequence: 1,
            required: selected,
            optional: vec![],
        })
        .unwrap();
        for frame in publisher.publication(output.clone(), None).unwrap() {
            assert!(matches!(frame, PluginFrame::Snapshot { .. }));
            host.accept(frame).unwrap();
        }
        assert_eq!(
            publisher.publication(output, Some(100)).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }
    #[test]
    fn checkpoints_are_committed_only_by_an_ack_for_a_published_revision() {
        let mut publisher = Publisher::default();
        for n in 1..=2 {
            publisher
                .publication(
                    PluginOutput {
                        state: vec![n],
                        state_schema_version: 1,
                        ..Default::default()
                    },
                    None,
                )
                .unwrap();
        }
        assert!(!publisher.acknowledge_checkpoint(3));
        assert_eq!(publisher.committed_checkpoint_revision, 0);
        assert!(publisher.acknowledge_checkpoint(2));
        assert!(!publisher.acknowledge_checkpoint(1));
        assert!(!publisher.acknowledge_checkpoint(2));
        assert_eq!(publisher.committed_checkpoint_revision, 2);
    }
    #[test]
    fn duplicate_capability_names_are_refused_by_both_peers() {
        let requirements = CapabilityRequirements {
            required: vec!["regions.v2".into()],
            optional: vec!["regions.v2".into()],
        };
        assert!(negotiate(&requirements, &["regions.v2".into()]).is_err());
        assert!(
            HostSession::default()
                .accept(PluginFrame::Welcome {
                    sequence: 1,
                    required: requirements.required,
                    optional: requirements.optional,
                })
                .is_err()
        );
    }
    #[test]
    fn duplicate_events_are_acknowledged_without_second_delivery() {
        let mut host = HostSession::default();
        host.accept(PluginFrame::Welcome {
            sequence: 1,
            required: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            optional: vec![],
        })
        .unwrap();
        let frame = PluginFrame::Event {
            id: 1,
            bubble: PluginBubble {
                id: "event-1".into(),
                kind: crate::BubbleKind::Todo,
                source: "Fixture".into(),
                title: "Event".into(),
                subtitle: "Details".into(),
                urgent: false,
                action: "open".into(),
                auto_dismiss_ms: None,
            },
        };
        assert!(matches!(
            host.accept(frame.clone()).unwrap(),
            HostEffect::Event { .. }
        ));
        assert!(matches!(
            host.accept(frame).unwrap(),
            HostEffect::DuplicateEvent(_)
        ));
    }
    #[test]
    fn event_replays_remain_duplicates_after_more_than_1024_publications() {
        let mut host = HostSession::default();
        host.accept(PluginFrame::Welcome {
            sequence: 1,
            required: CAPABILITIES.iter().map(|cap| cap.to_string()).collect(),
            optional: vec![],
        })
        .unwrap();
        let event = |n| PluginFrame::Event {
            id: n,
            bubble: PluginBubble {
                id: format!("event-{n}"),
                kind: crate::BubbleKind::Todo,
                source: "Fixture".into(),
                title: "Event".into(),
                subtitle: "Details".into(),
                urgent: false,
                action: "open".into(),
                auto_dismiss_ms: None,
            },
        };
        for n in 1..=2048 {
            assert!(matches!(
                host.accept(event(n)).unwrap(),
                HostEffect::Event { .. }
            ));
        }
        assert!(matches!(
            host.accept(event(1)).unwrap(),
            HostEffect::DuplicateEvent(_)
        ));
    }
    fn test_event(id: u64) -> PluginFrame {
        PluginFrame::Event {
            id,
            bubble: PluginBubble {
                id: format!("business-{id}"),
                kind: crate::BubbleKind::Todo,
                source: "Fixture".into(),
                title: "Event".into(),
                subtitle: "Details".into(),
                urgent: false,
                action: "open".into(),
                auto_dismiss_ms: None,
            },
        }
    }
    fn event_host(retry: bool) -> HostSession {
        let mut host = HostSession::default();
        host.accept(PluginFrame::Welcome {
            sequence: 1,
            required: vec!["regions.v2".into(), "events.v2".into()],
            optional: if retry {
                vec![EVENT_RETRY_CAPABILITY.into()]
            } else {
                vec![]
            },
        })
        .unwrap();
        host
    }
    #[test]
    fn rejected_admission_does_not_allow_later_events_to_overtake() {
        let mut host = event_host(true);
        assert!(matches!(
            host.accept(test_event(1)).unwrap(),
            HostEffect::Event { id: 1, .. }
        ));
        host.forget_event(1);
        assert!(matches!(
            host.accept(test_event(2)).unwrap(),
            HostEffect::RetryEvent(2)
        ));
        assert!(matches!(
            host.accept(test_event(1)).unwrap(),
            HostEffect::Event { id: 1, .. }
        ));
        assert!(matches!(
            host.accept(test_event(1)).unwrap(),
            HostEffect::DuplicateEvent(1)
        ));
        assert!(matches!(
            host.accept(test_event(2)).unwrap(),
            HostEffect::Event { id: 2, .. }
        ));
        host.forget_event(1);
        assert!(matches!(
            host.accept(test_event(2)).unwrap(),
            HostEffect::DuplicateEvent(2)
        ));
    }
    #[test]
    fn event_zero_and_gaps_without_retry_are_protocol_errors() {
        let mut host = event_host(false);
        for id in [0, 2] {
            assert_eq!(
                host.accept(test_event(id)).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
        assert!(matches!(
            host.accept(test_event(1)).unwrap(),
            HostEffect::Event { id: 1, .. }
        ));
    }
    #[test]
    fn exhausted_event_sequence_never_wraps_or_mutates_publication() {
        let mut publisher = Publisher {
            last_event_id: u64::MAX,
            ..Default::default()
        };
        let PluginFrame::Event { bubble, .. } = test_event(1) else {
            unreachable!()
        };
        assert!(
            publisher
                .publication(
                    PluginOutput {
                        bubbles: vec![bubble],
                        ..Default::default()
                    },
                    None
                )
                .is_err()
        );
        assert_eq!(publisher.last_event_id, u64::MAX);
        assert!(publisher.unacknowledged.is_empty());
        assert!(publisher.seen.is_empty());
        assert_eq!(publisher.revision, 0);
        let mut host = event_host(true);
        host.last_event_id = u64::MAX;
        assert!(matches!(
            host.accept(test_event(u64::MAX)).unwrap(),
            HostEffect::DuplicateEvent(u64::MAX)
        ));
        assert!(host.accept(test_event(0)).is_err());
    }
    #[test]
    fn unknown_optional_capability_is_ignored_but_required_is_refused() {
        let welcome = |required| PluginFrame::Welcome {
            sequence: 1,
            required,
            optional: vec!["future.v3".into()],
        };
        assert!(
            HostSession::default()
                .accept(welcome(vec!["regions.v2".into()]))
                .is_ok()
        );
        assert!(
            HostSession::default()
                .accept(welcome(vec!["future.v3".into()]))
                .is_err()
        );
    }
    #[test]
    fn unchanged_output_has_no_snapshot_and_revisions_cannot_go_backwards() {
        let mut publisher = Publisher::default();
        assert!(
            publisher
                .publication(PluginOutput::default(), None)
                .unwrap()
                .iter()
                .all(|frame| !matches!(frame, PluginFrame::Snapshot { .. }))
        );
        let mut host = HostSession::default();
        host.accept(PluginFrame::Welcome {
            sequence: 1,
            required: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            optional: vec![],
        })
        .unwrap();
        let changed = PluginOutput {
            model: PluginView {
                nodes: vec![crate::UiNode::Text {
                    text: "hello".into(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let frames = publisher.publication(changed, None).unwrap();
        let snapshot = frames
            .into_iter()
            .find(|frame| matches!(frame, PluginFrame::Snapshot { .. }))
            .unwrap();
        assert!(host.accept(snapshot.clone()).is_ok());
        assert!(host.accept(snapshot).is_err());
    }
}
