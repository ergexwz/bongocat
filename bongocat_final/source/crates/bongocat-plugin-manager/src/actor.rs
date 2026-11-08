//! One bounded mailbox and runtime owner per plugin. No plugin IO runs on the service.
use crate::{PluginManager, PluginManagerError, PluginProjection};
use bongocat_plugin_sdk::{ConfigValue, InputActivity, PetBounds, PetState, PluginInputEvent};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone)]
pub enum PluginOperation {
    Enable(bool),
    Action(String),
    Preview(String, String),
    Configure(String, ConfigValue),
    Locale(String),
    Reset,
    Uninstall,
    Install {
        bytes: Vec<u8>,
        digest: Option<String>,
        enable: bool,
    },
    Tick(u64),
    Activity(u64, InputActivity),
    Input(Vec<PluginInputEvent>, u64),
    Bounds(PetBounds),
    Pet(PetState),
}

impl PluginOperation {
    pub fn accepts(&self, metadata: &bongocat_plugin_sdk::PluginMetadata) -> bool {
        use bongocat_plugin_sdk::Capability;
        let capability = match self {
            Self::Activity(..) => Some(Capability::InputActivityV1),
            Self::Input(..) => Some(Capability::InputEventsV1),
            Self::Bounds(_) => Some(Capability::CompanionV1),
            Self::Pet(_) => Some(Capability::PetStateV1),
            Self::Locale(_) => Some(Capability::LocalizedViewsV1),
            _ => None,
        };
        if matches!(self, Self::Tick(_)) && metadata.protocol_version != 1 {
            return false;
        }
        capability.is_none_or(|capability| metadata.capabilities.contains(&capability))
    }
    fn coalesces(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Tick(_), Self::Tick(_))
                | (Self::Activity(..), Self::Activity(..))
                | (Self::Bounds(_), Self::Bounds(_))
                | (Self::Pet(_), Self::Pet(_))
                | (Self::Preview(..), Self::Preview(..))
        )
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Install { .. } => 0, // ZIPs have a separate package-size gate and one lifecycle job per plugin.
            Self::Action(s) | Self::Locale(s) => s.len(),
            Self::Preview(a, b) => a.len() + b.len(),
            Self::Configure(key, value) => {
                key.len()
                    + match value {
                        ConfigValue::Text(s) => s.len(),
                        _ => 16,
                    }
            }
            Self::Input(events, _) => events.len() * 32,
            _ => 128,
        }
    }
    fn barrier(&self) -> bool {
        matches!(
            self,
            Self::Enable(_)
                | Self::Configure(..)
                | Self::Reset
                | Self::Uninstall
                | Self::Install { .. }
                | Self::Action(_)
        )
    }
}

pub struct PluginCompletion {
    pub sequence: u64,
    pub result: Result<(), PluginManagerError>,
}

/// Bounded metadata only: no request bodies, keys, paths, or plugin user data.
#[derive(Clone, Debug, Default)]
pub struct PluginDiagnostics {
    pub completed: u64,
    pub requests: u64,
    pub starts: u64,
    pub restarts: u64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub timeouts: u64,
    pub queue_peak: usize,
    pub payload_peak_bytes: usize,
    pub scheduled_wakes: u64,
    pub failed: u64,
    pub rejected: u64,
    pub coalesced: u64,
    pub input_overflow: u64,
    pub recent_request_micros: VecDeque<u64>,
}
struct Mailbox {
    queue: VecDeque<(u64, PluginOperation)>,
    bytes: usize,
    input_overflow: u64,
}
pub struct PluginActor {
    events: Arc<Mutex<VecDeque<bongocat_plugin_sdk::PluginBubble>>>,
    diagnostics: Arc<Mutex<PluginDiagnostics>>,
    mailbox: Arc<Mutex<Mailbox>>,
    projection: Arc<Mutex<Option<Option<PluginProjection>>>>,
    completions: Option<Receiver<PluginCompletion>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), PluginManagerError>>>,
}
impl PluginActor {
    pub(crate) fn start(id: String, mut manager: PluginManager) -> io::Result<Self> {
        let mailbox = Arc::new(Mutex::new(Mailbox {
            queue: VecDeque::new(),
            bytes: 0,
            input_overflow: 0,
        }));
        let projection = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (completed, completions) = mpsc::sync_channel(64);
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let worker_events = Arc::clone(&events);
        let owner = thread::current();
        let diagnostics = Arc::new(Mutex::new(PluginDiagnostics::default()));
        let worker_diagnostics = Arc::clone(&diagnostics);
        let work_mailbox = Arc::clone(&mailbox);
        let work_projection = Arc::clone(&projection);
        let work_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name(format!("plugin-{id}"))
            .spawn(move || {
                manager.registry.rebind_thread();
                let started = Instant::now();
                let mut previous = manager.registry.projection(&id);
                while !work_stop.load(Ordering::Acquire) {
                    let task = {
                        let mut mailbox = work_mailbox
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let mut task = mailbox.queue.pop_front();
                        if let Some((_, operation)) = &task {
                            mailbox.bytes = mailbox.bytes.saturating_sub(operation.bytes());
                        }
                        if let Some((_, PluginOperation::Input(_, overflow))) = &mut task {
                            *overflow = overflow
                                .saturating_add(std::mem::take(&mut mailbox.input_overflow));
                        } else if task.is_none() && mailbox.input_overflow > 0 {
                            task = Some((
                                0,
                                PluginOperation::Input(
                                    Vec::new(),
                                    std::mem::take(&mut mailbox.input_overflow),
                                ),
                            ));
                        }
                        task
                    };
                    let completion = task.map(|(sequence, operation)| {
                        let started = Instant::now();
                        let result = execute(&mut manager, &id, operation);
                        let mut stats = worker_diagnostics
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        stats.completed = stats.completed.saturating_add(1);
                        if result.is_err() {
                            stats.failed = stats.failed.saturating_add(1);
                        }
                        if stats.recent_request_micros.len() == 1024 {
                            stats.recent_request_micros.pop_front();
                        }
                        stats
                            .recent_request_micros
                            .push_back(started.elapsed().as_micros().min(u64::MAX as u128) as u64);
                        PluginCompletion { sequence, result }
                    });
                    {
                        let mut events = worker_events
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        for (_, event) in manager
                            .registry
                            .drain_events(256usize.saturating_sub(events.len()))
                        {
                            events.push_back(event);
                            owner.unpark();
                        }
                    }
                    let wakes = manager
                        .registry
                        .due_wakes(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
                    {
                        let mut stats = worker_diagnostics
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        stats.scheduled_wakes = stats.scheduled_wakes.saturating_add(wakes);
                    }
                    manager.collect_updates();
                    if let Some(wire) = manager.registry.statistics(&id) {
                        let mut stats = worker_diagnostics
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        stats.requests = wire.requests.load(Ordering::Relaxed);
                        stats.starts = wire.starts.load(Ordering::Relaxed);
                        stats.restarts = stats.starts.saturating_sub(1);
                        stats.received_bytes = wire.read_bytes.load(Ordering::Relaxed);
                        stats.sent_bytes = wire.written_bytes.load(Ordering::Relaxed);
                        stats.timeouts = wire.timeouts.load(Ordering::Relaxed);
                    }
                    let current = manager.registry.projection(&id);
                    if current != previous {
                        *work_projection
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(current.clone());
                        previous = current;
                        owner.unpark();
                    }
                    if let Some(completion) = completion {
                        if completion.sequence != 0 && completed.send(completion).is_err() {
                            break;
                        }
                        owner.unpark();
                    }
                    let idle = {
                        let mailbox = work_mailbox
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        mailbox.queue.is_empty() && mailbox.input_overflow == 0
                    };
                    if work_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if idle {
                        if let Some(deadline) = manager.registry.next_wake() {
                            thread::park_timeout(
                                deadline.saturating_duration_since(Instant::now()),
                            );
                        } else {
                            thread::park();
                        }
                    }
                }
                let result = manager.shutdown();
                owner.unpark();
                result
            })?;
        Ok(Self {
            events,
            diagnostics,
            mailbox,
            projection,
            completions: Some(completions),
            stop,
            worker: Some(worker),
        })
    }
    pub fn submit(&self, sequence: u64, operation: PluginOperation) -> io::Result<()> {
        if self.stop.load(Ordering::Acquire) || self.worker.as_ref().is_none_or(|w| w.is_finished())
        {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let mut mailbox = self
            .mailbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if sequence == 0 {
            let total_bytes = mailbox.bytes;
            for (queued_sequence, queued) in mailbox.queue.iter_mut().rev() {
                if queued.barrier() {
                    break;
                }
                if *queued_sequence == 0 && operation.coalesces(queued) {
                    let old = queued.bytes();
                    let new = operation.bytes();
                    if total_bytes - old + new > 1024 * 1024 {
                        let mut stats = self
                            .diagnostics
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        stats.rejected = stats.rejected.saturating_add(1);
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    *queued = operation;
                    mailbox.bytes = mailbox.bytes - old + new;
                    let mut stats = self
                        .diagnostics
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    stats.coalesced = stats.coalesced.saturating_add(1);
                    return Ok(());
                }
            }
        }
        let bytes = operation.bytes();
        if mailbox.queue.len() >= 64 || mailbox.bytes + bytes > 1024 * 1024 {
            let mut stats = self
                .diagnostics
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            stats.rejected = stats.rejected.saturating_add(1);
            if let PluginOperation::Input(events, overflow) = &operation {
                stats.input_overflow = stats
                    .input_overflow
                    .saturating_add(*overflow)
                    .saturating_add(events.len() as u64);
                mailbox.input_overflow = mailbox
                    .input_overflow
                    .saturating_add(*overflow)
                    .saturating_add(events.len() as u64);
            }
            return Err(io::ErrorKind::WouldBlock.into());
        }
        mailbox.bytes += bytes;
        mailbox.queue.push_back((sequence, operation));
        {
            let mut stats = self
                .diagnostics
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            stats.queue_peak = stats.queue_peak.max(mailbox.queue.len());
            stats.payload_peak_bytes = stats.payload_peak_bytes.max(mailbox.bytes);
        }
        drop(mailbox);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(())
    }
    pub fn take_projection(&self) -> Option<Option<PluginProjection>> {
        self.projection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
    pub fn try_completion(&self) -> Result<PluginCompletion, mpsc::TryRecvError> {
        self.completions.as_ref().expect("actor is open").try_recv()
    }
    pub fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
    pub fn take_events(&self, limit: usize) -> Vec<bongocat_plugin_sdk::PluginBubble> {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = limit.min(events.len());
        let result = events.drain(..count).collect();
        if count > 0
            && let Some(worker) = &self.worker
        {
            worker.thread().unpark();
        }
        result
    }
    pub fn diagnostics(&self) -> PluginDiagnostics {
        self.diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
    pub fn join(mut self) -> Result<(), PluginManagerError> {
        self.request_stop();
        self.completions.take();
        self.worker
            .take()
            .expect("worker exists")
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("plugin worker panicked").into()))
    }
}
impl Drop for PluginActor {
    fn drop(&mut self) {
        self.request_stop();
        self.completions.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn execute(
    manager: &mut PluginManager,
    id: &str,
    op: PluginOperation,
) -> Result<(), PluginManagerError> {
    match op {
        PluginOperation::Enable(true) => manager.enable_plugin(id),
        PluginOperation::Enable(false) => manager.disable_plugin(id),
        PluginOperation::Action(action) => manager.action_plugin(id, &action),
        PluginOperation::Preview(key, value) => manager.preview_plugin(id, &key, &value),
        PluginOperation::Configure(key, value) => {
            let mut config = manager
                .plugin_output(id)
                .ok_or_else(|| io::Error::other("plugin is not installed"))?
                .config
                .clone();
            config.insert(key, value);
            manager.configure_plugin(id, config)
        }
        PluginOperation::Locale(locale) => manager.set_locale(&locale),
        PluginOperation::Reset => manager.reset(id).map(|_| ()),
        PluginOperation::Uninstall => manager.uninstall(id).map(|_| ()),
        PluginOperation::Install {
            bytes,
            digest,
            enable,
        } => {
            if let Some(digest) = digest {
                manager.install_verified(&bytes, &digest)?;
            } else {
                manager.install_local(&bytes)?;
            }
            if enable {
                manager.enable_plugin(id)?;
            }
            Ok(())
        }
        PluginOperation::Tick(ms) => {
            manager.registry.tick(ms);
            Ok(())
        }
        PluginOperation::Activity(ms, a) => {
            manager.input_activity(ms, a);
            Ok(())
        }
        PluginOperation::Input(events, overflow) => {
            manager.input_events(events, overflow);
            Ok(())
        }
        PluginOperation::Bounds(bounds) => {
            manager.pet_bounds(bounds);
            Ok(())
        }
        PluginOperation::Pet(state) => {
            manager.pet_state(state);
            Ok(())
        }
    }
}
