//! Process-backed plugin registry. A failed child only removes that entry.
use crate::loader::Process;
use bongocat_plugin_sdk::{
    Configuration, HostMessage, PetBounds, PetState, PluginMetadata, PluginOutput, UiNode,
};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin is already running: {0}")]
    AlreadyRunning(String),
    #[error("plugin is not running: {0}")]
    NotRunning(String),
    #[error("plugin did not publish this preview option")]
    InvalidPreview,
    #[error("plugin process failed: {0}")]
    Process(#[from] std::io::Error),
    #[error("plugin state could not be saved: {0}")]
    Storage(std::io::Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginState {
    Discovered,
    Enabled,
    Disabled,
    Failed,
}
struct Entry {
    stats: Arc<crate::loader::WireStats>,
    events: Arc<Mutex<VecDeque<bongocat_plugin_sdk::PluginBubble>>>,
    metadata: PluginMetadata,
    root: PathBuf,
    state: PluginState,
    process: Option<Process>,
    last_output: Arc<PluginOutput>,
    data_file: PathBuf,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginProjection {
    pub metadata: PluginMetadata,
    pub state: PluginState,
    pub enabled: bool,
    pub output: Arc<PluginOutput>,
}
/// The session envelope stored beside the packages: a plugin's configuration,
/// its own state bytes and the user's enable request.
///
/// It is a complete v1 document, and a session is either absent — a plugin that
/// has never run — or exactly this shape. A file missing its schema, or claiming
/// another one, is refused by the reader rather than completed here.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedSession {
    schema_version: u32,
    config: Configuration,
    state: Vec<u8>,
    #[serde(default, skip_serializing_if = "zero_schema")]
    state_schema_version: u32,
    enabled: bool,
}

fn zero_schema(version: &u32) -> bool {
    *version == 0
}

impl SavedSession {
    fn empty() -> Self {
        Self {
            schema_version: 1,
            config: Configuration::new(),
            state: Vec::new(),
            state_schema_version: 0,
            enabled: false,
        }
    }
}
pub struct PluginRegistry {
    entries: BTreeMap<String, Entry>,
    /// The catalog tag the host is showing, delivered to running plugins that
    /// declare [`bongocat_plugin_sdk::Capability::LocalizedViewsV1`].
    ///
    /// The registry owns it rather than the caller because a plugin started
    /// later — enable, upgrade, resume at startup — has to receive the tag the
    /// host was already showing, and a value that lived in the command layer
    /// would have to be replayed into every start path.
    locale: Option<String>,
}
impl PluginRegistry {
    pub(crate) fn statistics(&self, id: &str) -> Option<Arc<crate::loader::WireStats>> {
        self.entries.get(id).map(|entry| Arc::clone(&entry.stats))
    }

    pub(crate) fn next_wake(&self) -> Option<std::time::Instant> {
        self.entries
            .values()
            .filter_map(|entry| entry.process.as_ref().and_then(Process::next_wake))
            .min()
    }
    pub(crate) fn due_wakes(&mut self, elapsed_ms: u64) -> u64 {
        let mut count = 0;
        let now = std::time::Instant::now();
        for entry in self.entries.values_mut() {
            if entry.metadata.protocol_version == 2
                && entry
                    .process
                    .as_ref()
                    .is_some_and(|process| process.take_due_wake(now))
            {
                count += 1;
                let _ = entry.required_exchange(HostMessage::Tick { elapsed_ms });
            }
        }
        count
    }
    pub(crate) fn drain_events(
        &mut self,
        limit: usize,
    ) -> Vec<(String, bongocat_plugin_sdk::PluginBubble)> {
        let mut result = Vec::new();
        for (id, entry) in &mut self.entries {
            let mut events = entry
                .events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let count = limit.saturating_sub(result.len()).min(events.len());
            result.extend(events.drain(..count).map(|event| (id.clone(), event)));
        }
        result
    }
    pub(crate) fn rebind_thread(&mut self) {
        for entry in self.entries.values_mut() {
            if let Some(process) = &mut entry.process {
                process.rebind_thread();
            }
        }
    }
    pub(crate) fn detach(&mut self, id: &str) -> Self {
        let mut detached = Self::new();
        detached.locale = self.locale.clone();
        if let Some(entry) = self.entries.remove(id) {
            self.entries.insert(
                id.to_owned(),
                Entry {
                    stats: Arc::clone(&entry.stats),
                    events: Arc::new(Mutex::new(VecDeque::new())),
                    metadata: entry.metadata.clone(),
                    root: entry.root.clone(),
                    state: entry.state,
                    process: None,
                    last_output: entry.last_output.clone(),
                    data_file: entry.data_file.clone(),
                    enabled: entry.enabled,
                },
            );
            detached.entries.insert(id.to_owned(), entry);
        }
        detached
    }

    pub(crate) fn projection(&self, id: &str) -> Option<PluginProjection> {
        self.entries.get(id).map(|entry| PluginProjection {
            metadata: entry.metadata.clone(),
            state: entry.state,
            enabled: entry.enabled,
            output: entry.last_output.clone(),
        })
    }

    pub(crate) fn apply_projection(
        &mut self,
        id: &str,
        projection: Option<PluginProjection>,
        store: &std::path::Path,
    ) {
        match projection {
            Some(p) => {
                self.entries.insert(
                    id.into(),
                    Entry {
                        stats: Arc::default(),
                        events: Arc::new(Mutex::new(VecDeque::new())),
                        root: store
                            .join(crate::PACKAGES_DIRECTORY)
                            .join(id)
                            .join(&p.metadata.version),
                        data_file: store
                            .join(crate::DATA_DIRECTORY)
                            .join(id)
                            .join("session.json"),
                        metadata: p.metadata,
                        state: p.state,
                        enabled: p.enabled,
                        last_output: p.output,
                        process: None,
                    },
                );
            }
            None => {
                self.entries.remove(id);
            }
        }
    }
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            locale: None,
        }
    }
    pub fn register(
        &mut self,
        metadata: PluginMetadata,
        root: PathBuf,
    ) -> Result<(), RegistryError> {
        if let Some(entry) = self.entries.get_mut(&metadata.id) {
            if entry.root != root || entry.metadata != metadata {
                if entry.process.is_some() {
                    return Err(RegistryError::AlreadyRunning(metadata.id));
                }
                entry.metadata = metadata;
                entry.root = root;
            }
            return Ok(());
        }
        let data_file = root
            .ancestors()
            .nth(3)
            .ok_or_else(|| std::io::Error::other("invalid package root"))?
            .join(crate::DATA_DIRECTORY)
            .join(&metadata.id)
            .join("session.json");
        let bytes = crate::install::read_bounded(&data_file, MAX_SESSION_BYTES);
        let saved = match bytes {
            Ok(bytes) if bytes.len() <= MAX_SESSION_BYTES => {
                let saved: SavedSession =
                    serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
                if saved.schema_version != 1 {
                    return Err(std::io::Error::other("unsupported plugin session schema").into());
                }
                if !(PluginOutput {
                    config: saved.config.clone(),
                    state: saved.state.clone(),
                    ..Default::default()
                })
                .validate()
                {
                    return Err(std::io::Error::other("invalid saved plugin data").into());
                }
                saved
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SavedSession::empty(),
            Err(error) => return Err(error.into()),
            _ => return Err(std::io::Error::other("saved plugin data exceeds limit").into()),
        };
        self.entries.insert(
            metadata.id.clone(),
            Entry {
                stats: Arc::default(),
                metadata,
                events: Arc::new(Mutex::new(VecDeque::new())),
                root,
                state: PluginState::Discovered,
                process: None,
                last_output: Arc::new(PluginOutput {
                    config: saved.config,
                    state: saved.state,
                    state_schema_version: saved.state_schema_version,
                    ..Default::default()
                }),
                data_file,
                enabled: saved.enabled,
            },
        );
        Ok(())
    }
    pub fn list(&self) -> Vec<(PluginMetadata, PluginState)> {
        self.entries
            .values()
            .map(|e| (e.metadata.clone(), e.state))
            .collect()
    }
    /// Whether a plugin currently has a running process.
    pub fn is_running(&self, id: &str) -> bool {
        self.entries
            .get(id)
            .is_some_and(|entry| entry.process.is_some())
    }
    /// The durable user choice, independent of the child process state.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.enabled)
    }
    /// Drop a plugin from the registry entirely, stopping its process first.
    ///
    /// Used by uninstall, where the package the entry pointed at no longer
    /// exists: keeping the entry would leave a card for a plugin that is gone.
    pub fn forget(&mut self, id: &str) {
        if let Some(mut entry) = self.entries.remove(id)
            && let Some(mut process) = entry.process.take()
        {
            let _ = process.stop();
        }
    }
    /// Drop every registered plugin whose id is not in `present`.
    ///
    /// Discovery runs this after a scan so a package removed behind the product's
    /// back does not keep a stale entry, while a plugin that is still on disk
    /// keeps its state and configuration across rescans.
    pub fn forget_missing(&mut self, present: &[String]) {
        let missing = self
            .entries
            .keys()
            .filter(|id| !present.iter().any(|kept| kept == *id))
            .cloned()
            .collect::<Vec<_>>();
        for id in missing {
            self.forget(&id);
        }
    }
    pub fn enable(&mut self, id: &str) -> Result<Arc<PluginOutput>, RegistryError> {
        let locale = self.locale.clone();
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        if entry.process.is_some() {
            return Ok(Arc::clone(&entry.last_output));
        }
        // Remember an explicit enable even if process creation or handshake fails.
        if let Some(directory) = entry.data_file.parent() {
            std::fs::create_dir_all(directory)?;
        }
        if let Err(error) = persist_output(entry, &entry.last_output, true) {
            entry.state = PluginState::Failed;
            return Err(RegistryError::Storage(error));
        }
        entry.enabled = true;
        let started = (|| -> Result<_, RegistryError> {
            let mut process = Process::start_with_events(
                &entry.root,
                &entry.metadata,
                Arc::clone(&entry.events),
                Arc::clone(&entry.stats),
            )?;
            let message = if entry.metadata.protocol_version == 1 {
                HostMessage::Start {
                    config: entry.last_output.config.clone(),
                    state: entry.last_output.state.clone(),
                }
            } else {
                HostMessage::Restore {
                    config: entry.last_output.config.clone(),
                    state: entry.last_output.state.clone(),
                    schema_version: entry.last_output.state_schema_version,
                }
            };
            let mut output = process.exchange(message)?;
            // A plugin started while the host already shows a language has to
            // publish in it, because the host only announces changes from here
            // on. The message goes through the same capability-gated delivery
            // every running plugin uses.
            show_locale(
                &mut process,
                &entry.metadata,
                &mut output,
                locale.as_deref(),
            )?;
            if output.state_schema_version != entry.last_output.state_schema_version {
                let previous = crate::install::read_bounded(&entry.data_file, MAX_SESSION_BYTES)?;
                let backup = entry
                    .data_file
                    .with_file_name("checkpoint-before-migration.json");
                use std::io::Write;
                let mut file = atomic_write_file::AtomicWriteFile::options().open(&backup)?;
                file.write_all(&previous)?;
                file.flush()?;
                file.commit()?;
            }
            persist_output(entry, &output, true).map_err(RegistryError::Storage)?;
            process.acknowledge_checkpoint(output.checkpoint_revision)?;
            Ok((process, output))
        })();
        match started {
            Ok((process, output)) => {
                entry.set_output(output.clone());
                entry.process = Some(process);
                entry.state = PluginState::Enabled;
                entry.enabled = true;
                Ok(output)
            }
            Err(error) => {
                entry.state = PluginState::Failed;
                Err(error)
            }
        }
    }

    pub fn disable(&mut self, id: &str) -> Result<(), RegistryError> {
        self.stop(id, false)
    }

    /// Stop for replacement without persisting a user-requested disable.
    pub(crate) fn suspend(&mut self, id: &str) -> Result<(), RegistryError> {
        let enabled = self
            .entries
            .get(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?
            .enabled;
        self.stop(id, enabled)
    }

    fn stop(&mut self, id: &str, enabled: bool) -> Result<(), RegistryError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        stop_entry(entry, enabled)
    }
    pub fn configure(
        &mut self,
        id: &str,
        config: Configuration,
    ) -> Result<Arc<PluginOutput>, RegistryError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        let previous = entry.last_output.clone();
        let output = entry.exchange(HostMessage::Configure { config })?;
        if let Err(error) = persist(entry) {
            // A process that applied an uncommitted config must not keep running
            // with settings different from disk. Retry starts the last good state.
            entry.fail();
            entry.last_output = previous;
            return Err(RegistryError::Storage(error));
        }
        Ok(output)
    }
    pub fn action(&mut self, id: &str, action: &str) -> Result<Arc<PluginOutput>, RegistryError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        entry.exchange(HostMessage::Action { id: action.into() })
    }
    pub fn preview(
        &mut self,
        id: &str,
        key: &str,
        value: &str,
    ) -> Result<Arc<PluginOutput>, RegistryError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        if entry.state != PluginState::Enabled {
            return Err(RegistryError::NotRunning(id.into()));
        }
        if !entry.last_output.settings.nodes.iter().any(|node| {
            matches!(node, UiNode::Select { key: published_key, options, preview: true, .. }
                if published_key == key && options.iter().any(|option| option.value == value))
        }) {
            return Err(RegistryError::InvalidPreview);
        }
        entry.exchange(HostMessage::Preview {
            key: key.into(),
            value: value.into(),
        })
    }
    pub fn collect_updates(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.metadata.protocol_version == 1
                && !entry
                    .metadata
                    .capabilities
                    .contains(&bongocat_plugin_sdk::Capability::PushOutputV1)
            {
                continue;
            }
            let Some(process) = entry.process.as_mut() else {
                continue;
            };
            match process.update() {
                Ok(Some(output)) => {
                    let checkpoint_changed =
                        output.checkpoint_revision > entry.last_output.checkpoint_revision;
                    if output.state_schema_version != entry.last_output.state_schema_version
                        || (checkpoint_changed
                            && persist_output(entry, &output, entry.enabled).is_err())
                        || (checkpoint_changed
                            && entry.process.as_ref().is_some_and(|p| {
                                p.acknowledge_checkpoint(output.checkpoint_revision)
                                    .is_err()
                            }))
                    {
                        entry.fail();
                    } else {
                        entry.set_output(output);
                    }
                }
                Ok(None) => {}
                Err(_) => entry.fail(),
            }
        }
    }
    pub fn tick(&mut self, elapsed_ms: u64) -> Vec<(String, Arc<PluginOutput>)> {
        let mut outputs = Vec::new();
        for (id, entry) in &mut self.entries {
            if entry.process.is_none() || entry.metadata.protocol_version != 1 {
                continue;
            }
            if let Ok(output) = entry.required_exchange(HostMessage::Tick { elapsed_ms }) {
                outputs.push((id.clone(), output));
            }
        }
        outputs
    }
    pub fn output(&self, id: &str) -> Option<&PluginOutput> {
        self.entries.get(id).map(|e| e.last_output.as_ref())
    }

    /// Show a newly resolved catalog tag to every plugin that declares it can
    /// localize its own views.
    ///
    /// Plugins that never declared `localized_views_v1` are skipped rather than
    /// sent a message they cannot parse: the capability is the contract.
    ///
    /// The tag is remembered before it is delivered, so a plugin started later
    /// receives it when its process starts. A plugin that advertises the
    /// capability and still refuses the exchange turns its own card into a
    /// failure; the tag itself is applied and the caller is told so, which is
    /// what lets the application stop re-announcing a language it believes it
    /// already sent.
    pub fn set_locale(&mut self, locale: &str) -> Result<(), RegistryError> {
        let locale = bongocat_plugin_sdk::plugin_locale(locale);
        if self.locale.as_deref() == Some(locale) {
            return Ok(());
        }
        self.locale = Some(locale.to_owned());
        let mut failure = None;
        for (id, entry) in &mut self.entries {
            if !localizes(entry) {
                continue;
            }
            let Some(mut process) = entry.process.take() else {
                continue;
            };
            let mut output = Arc::clone(&entry.last_output);
            match show_locale(&mut process, &entry.metadata, &mut output, Some(locale)) {
                Ok(()) => {
                    entry.set_output(output);
                    entry.process = Some(process);
                }
                Err(error) => {
                    entry.state = PluginState::Failed;
                    failure.get_or_insert(RegistryError::Process(io::Error::new(
                        error.kind(),
                        format!("{} refused its localized views", id),
                    )));
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub fn shutdown(&mut self) -> Result<(), RegistryError> {
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .entries
                .values_mut()
                .filter(|entry| entry.process.is_some())
                .map(|entry| scope.spawn(move || stop_entry(entry, entry.enabled)))
                .collect();
            let mut failure = None;
            for worker in workers {
                let result = worker.join().unwrap_or_else(|_| {
                    Err(io::Error::other("plugin shutdown worker panicked").into())
                });
                if let Err(error) = result {
                    failure.get_or_insert(error);
                }
            }
            failure.map_or(Ok(()), Err)
        })
    }

    pub fn resume_enabled(&mut self) -> Result<(), RegistryError> {
        let ids: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.enabled && entry.process.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        let mut failure = None;
        for id in ids {
            if let Err(error) = self.enable(&id) {
                if let Some(entry) = self.entries.get_mut(&id) {
                    entry.state = PluginState::Failed;
                }
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Hand every running plugin the device edges counted since the last poll.
    ///
    /// The capability a plugin declared is what entitles it to the message, not
    /// the protocol version: a plugin that never asked for input activity is not
    /// running one at all, and is left alone.
    pub fn input_events(
        &mut self,
        events: Vec<bongocat_plugin_sdk::PluginInputEvent>,
        overflow: u64,
    ) {
        for entry in self.entries.values_mut() {
            if entry.process.is_some()
                && entry
                    .metadata
                    .capabilities
                    .contains(&bongocat_plugin_sdk::Capability::InputEventsV1)
            {
                if !entry.process.as_ref().is_some_and(|process| {
                    process.subscribed(bongocat_plugin_sdk::Capability::InputEventsV1)
                }) {
                    continue;
                }
                let _ = entry.required_exchange(HostMessage::InputEvents {
                    events: events.clone(),
                    overflow,
                });
            }
        }
    }
    /// Hand the model window's logical rectangle to every running companion
    /// plugin. Plugins that did not ask for it are never sent a frame they
    /// cannot parse.
    pub fn pet_bounds(&mut self, bounds: PetBounds) {
        for entry in self.entries.values_mut() {
            if entry.process.is_some()
                && entry
                    .metadata
                    .capabilities
                    .contains(&bongocat_plugin_sdk::Capability::CompanionV1)
            {
                if !entry.process.as_ref().is_some_and(|process| {
                    process.subscribed(bongocat_plugin_sdk::Capability::CompanionV1)
                }) {
                    continue;
                }
                let _ = entry.required_exchange(HostMessage::PetBounds { bounds });
            }
        }
    }

    /// Hand the active model and model-window visibility to every running
    /// companion that asked for it.
    ///
    /// A plugin from the flat companion contract never receives this frame, and
    /// therefore keeps parsing every message it can still receive.
    pub fn pet_state(&mut self, state: PetState) {
        for entry in self.entries.values_mut() {
            if entry.process.is_some() && receives_pet_state(&entry.metadata.capabilities) {
                if !entry.process.as_ref().is_some_and(|process| {
                    process.subscribed(bongocat_plugin_sdk::Capability::PetStateV1)
                }) {
                    continue;
                }
                let _ = entry.required_exchange(HostMessage::PetState {
                    state: state.clone(),
                });
            }
        }
    }
    pub fn input_activity(
        &mut self,
        elapsed_ms: u64,
        activity: bongocat_plugin_sdk::InputActivity,
    ) {
        for entry in self.entries.values_mut() {
            let Some(message) =
                activity_message(&entry.metadata.capabilities, elapsed_ms, activity)
            else {
                continue;
            };
            if entry.process.as_ref().is_some_and(|process| {
                process.subscribed(bongocat_plugin_sdk::Capability::InputActivityV1)
            }) {
                let _ = entry.required_exchange(message);
            }
        }
    }

    /// Establish the cumulative activity baseline for one newly started plugin.
    ///
    /// This is intentionally separate from [`Self::input_activity`]: sending a
    /// baseline to every plugin while enabling one plugin could make an already
    /// running sound plugin replay edges that occurred during the enable action.
    pub fn initialize_input_activity(
        &mut self,
        id: &str,
        elapsed_ms: u64,
        activity: bongocat_plugin_sdk::InputActivity,
    ) {
        let Some(entry) = self.entries.get_mut(id) else {
            return;
        };
        if !entry
            .metadata
            .capabilities
            .contains(&bongocat_plugin_sdk::Capability::InputActivityV1)
        {
            return;
        }
        if entry.process.as_ref().is_some_and(|process| {
            process.subscribed(bongocat_plugin_sdk::Capability::InputActivityV1)
        }) {
            let _ = entry.required_exchange(HostMessage::InputActivity {
                elapsed_ms,
                activity,
            });
        }
    }
}

fn stop_entry(entry: &mut Entry, enabled: bool) -> Result<(), RegistryError> {
    let stopped = entry
        .process
        .take()
        .map(|mut process| process.stop())
        .transpose();
    if let Ok(Some(output)) = &stopped {
        entry.set_output(output.clone());
    }
    entry.enabled = enabled;
    entry.state = if stopped.is_err() {
        PluginState::Failed
    } else {
        PluginState::Disabled
    };
    // Persist explicit disable even if Stop fails; shutdown retains the user's choice.
    persist(entry).map_err(RegistryError::Storage)?;
    stopped.map(|_| ()).map_err(Into::into)
}

/// Whether one plugin's manifest opted into the companion state message.
///
/// The rectangle is still gated by `CompanionV1`; the model/visibility message
/// has its own capability so a companion from the older flat contract is never
/// sent a frame it cannot parse.
fn receives_pet_state(capabilities: &[bongocat_plugin_sdk::Capability]) -> bool {
    capabilities.contains(&bongocat_plugin_sdk::Capability::PetStateV1)
}

/// The activity message one plugin's declared capabilities entitle it to.
///
/// The capability is the gate, not the protocol version: a plugin that declared
/// none is not running an activity plugin at all, so there is nothing to send it.
fn activity_message(
    capabilities: &[bongocat_plugin_sdk::Capability],
    elapsed_ms: u64,
    activity: bongocat_plugin_sdk::InputActivity,
) -> Option<HostMessage> {
    capabilities
        .contains(&bongocat_plugin_sdk::Capability::InputActivityV1)
        .then_some(HostMessage::InputActivity {
            elapsed_ms,
            activity,
        })
}

/// Hand the host's current catalog tag to one running plugin.
///
/// A plugin that never declared [`Capability::LocalizedViewsV1`] is skipped: the
/// capability is the contract that says the plugin understands the message, so a
/// plugin that does not localize its views is never sent a frame it would fail to
/// parse.
///
/// A plugin that declared the capability and then refuses the exchange is a
/// fault like any other failed tick — the process is dropped and the card shows
/// as failed — because a plugin that cannot answer a message it advertised is
/// not one the host can keep driving.
/// Whether a plugin declared that it localizes the views it publishes.
fn localizes(entry: &Entry) -> bool {
    entry
        .metadata
        .capabilities
        .contains(&bongocat_plugin_sdk::Capability::LocalizedViewsV1)
}

/// Send the tag the host is showing to one process, if its manifest declares that
/// it localizes its views.
fn show_locale(
    process: &mut Process,
    metadata: &PluginMetadata,
    output: &mut Arc<PluginOutput>,
    locale: Option<&str>,
) -> std::io::Result<()> {
    let Some(locale) = locale else {
        return Ok(());
    };
    if !metadata
        .capabilities
        .contains(&bongocat_plugin_sdk::Capability::LocalizedViewsV1)
    {
        return Ok(());
    }
    *output = process.exchange(HostMessage::Locale {
        locale: metadata.view_locale(locale).to_owned(),
    })?;
    Ok(())
}

impl Entry {
    fn set_output(&mut self, output: Arc<PluginOutput>) {
        if !Arc::ptr_eq(&self.last_output, &output) && self.last_output != output {
            self.last_output = output;
        }
    }

    fn fail(&mut self) {
        self.process.take();
        self.state = PluginState::Failed;
    }

    fn exchange(&mut self, message: HostMessage) -> Result<Arc<PluginOutput>, RegistryError> {
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| RegistryError::NotRunning(self.metadata.id.clone()))?;
        match process.exchange(message) {
            Ok(output) => {
                if output.state_schema_version != self.last_output.state_schema_version {
                    self.fail();
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "checkpoint schema changed after startup",
                    )
                    .into());
                }
                if output.checkpoint_revision > self.last_output.checkpoint_revision {
                    if let Err(error) = persist_output(self, &output, self.enabled) {
                        self.fail();
                        return Err(RegistryError::Storage(error));
                    }
                    if let Some(process) = &self.process {
                        process.acknowledge_checkpoint(output.checkpoint_revision)?;
                    }
                }
                self.set_output(output.clone());
                Ok(output)
            }
            Err(error) => {
                if !matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput | std::io::ErrorKind::WouldBlock
                ) {
                    self.fail();
                }
                Err(error.into())
            }
        }
    }

    fn required_exchange(
        &mut self,
        message: HostMessage,
    ) -> Result<Arc<PluginOutput>, RegistryError> {
        let result = self.exchange(message);
        if result.is_err() {
            self.fail();
        }
        result
    }
}

// State bytes expand to JSON integers; a valid 64 KiB state can exceed the
// protocol's frame limit once wrapped in a durable session.
const MAX_SESSION_BYTES: usize = 512 * 1024;

fn persist(entry: &Entry) -> std::io::Result<()> {
    persist_output(entry, &entry.last_output, entry.enabled)
}

fn persist_output(entry: &Entry, output: &PluginOutput, enabled: bool) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(
        entry
            .data_file
            .parent()
            .ok_or_else(|| std::io::Error::other("invalid data root"))?,
    )?;
    let mut file = atomic_write_file::AtomicWriteFile::options().open(&entry.data_file)?;
    serde_json::to_writer(
        &mut file,
        &SavedSession {
            schema_version: 1,
            config: output.config.clone(),
            state: output.state.clone(),
            state_schema_version: output.state_schema_version,
            enabled,
        },
    )?;
    file.flush()?;
    file.commit()
}
impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bongocat_plugin_sdk::{Capability, InputActivity};

    fn metadata() -> PluginMetadata {
        PluginMetadata {
            id: "com.bongocat.test".into(),
            name: "Test plugin".into(),
            description: "A test fixture plugin.".into(),
            version: "1.0.0".into(),
            app_versions: ">=2.3.1".into(),
            protocol_version: bongocat_plugin_sdk::PROTOCOL_VERSION,
            entry: "plugin".into(),
            icon: bongocat_plugin_sdk::PluginIcon::Emoji("A".into()),
            capabilities: Vec::new(),
            localizations: Default::default(),
        }
    }

    #[test]
    fn failed_start_keeps_user_enable_choice_until_explicit_disable() {
        let store = tempfile::tempdir().unwrap();
        let metadata = metadata();
        let root = store
            .path()
            .join("packages")
            .join(&metadata.id)
            .join(&metadata.version);
        std::fs::create_dir_all(&root).unwrap();
        let mut registry = PluginRegistry::new();
        registry.register(metadata.clone(), root.clone()).unwrap();
        assert!(registry.enable(&metadata.id).is_err());
        assert!(registry.is_enabled(&metadata.id));
        let mut reopened = PluginRegistry::new();
        reopened.register(metadata.clone(), root.clone()).unwrap();
        assert!(reopened.is_enabled(&metadata.id));
        reopened.disable(&metadata.id).unwrap();
        let mut reopened = PluginRegistry::new();
        reopened.register(metadata.clone(), root).unwrap();
        assert!(!reopened.is_enabled(&metadata.id));
    }

    #[test]
    fn durable_sessions_round_trip_full_state_and_reject_invalid_config() {
        let store = tempfile::tempdir().unwrap();
        let metadata = metadata();
        let root = store
            .path()
            .join("packages")
            .join(&metadata.id)
            .join(&metadata.version);
        let data = store.path().join("data").join(&metadata.id);
        std::fs::create_dir_all(&data).unwrap();
        let file = data.join("session.json");
        let mut saved = SavedSession {
            state: vec![255; 64 * 1024],
            config: [(
                "minutes".into(),
                bongocat_plugin_sdk::ConfigValue::Integer(5),
            )]
            .into(),
            enabled: true,
            ..SavedSession::empty()
        };
        let bytes = serde_json::to_vec(&saved).unwrap();
        assert!(bytes.len() > bongocat_plugin_sdk::MAX_MESSAGE_BYTES);
        std::fs::write(&file, bytes).unwrap();
        let mut registry = PluginRegistry::new();
        registry.register(metadata.clone(), root.clone()).unwrap();
        assert_eq!(registry.output(&metadata.id).unwrap().state, saved.state);
        assert_eq!(registry.output(&metadata.id).unwrap().config, saved.config);
        assert!(registry.entries[&metadata.id].enabled);
        persist(&registry.entries[&metadata.id]).unwrap();
        let mut reopened = PluginRegistry::new();
        reopened.register(metadata.clone(), root.clone()).unwrap();
        assert_eq!(reopened.output(&metadata.id).unwrap().state, saved.state);

        saved.config.insert(
            "../escape".into(),
            bongocat_plugin_sdk::ConfigValue::Boolean(true),
        );
        std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(
            PluginRegistry::new()
                .register(metadata.clone(), root.clone())
                .is_err()
        );

        // A session is a complete v1 document: one that omits its schema, names
        // another one, or carries a field the envelope does not define is
        // refused instead of being completed with defaults.
        let value = serde_json::to_value(SavedSession::empty()).unwrap();
        for edit in [
            |value: &mut serde_json::Value| {
                value.as_object_mut().unwrap().remove("schema_version");
            },
            |value: &mut serde_json::Value| value["schema_version"] = 2.into(),
            |value: &mut serde_json::Value| {
                value["enabled_by"] = "unknown field".into();
            },
        ] {
            let mut document = value.clone();
            edit(&mut document);
            std::fs::write(&file, serde_json::to_vec(&document).unwrap()).unwrap();
            assert!(
                PluginRegistry::new()
                    .register(metadata.clone(), root.clone())
                    .is_err(),
                "{document} was accepted"
            );
        }
    }

    #[test]
    fn configuring_a_stopped_plugin_is_refused_without_changing_its_output() {
        let store = tempfile::tempdir().unwrap();
        let metadata = metadata();
        let mut registry = PluginRegistry::new();
        registry
            .register(
                metadata.clone(),
                store
                    .path()
                    .join("packages")
                    .join(&metadata.id)
                    .join(&metadata.version),
            )
            .unwrap();
        assert!(matches!(
            registry.configure(
                &metadata.id,
                [(
                    "minutes".into(),
                    bongocat_plugin_sdk::ConfigValue::Integer(5)
                )]
                .into()
            ),
            Err(RegistryError::NotRunning(_))
        ));
        assert!(registry.output(&metadata.id).unwrap().config.is_empty());
    }

    #[test]
    fn only_running_plugins_with_opted_in_published_options_receive_previews() {
        let store = tempfile::tempdir().unwrap();
        let metadata = metadata();
        let mut registry = PluginRegistry::new();
        registry
            .register(
                metadata.clone(),
                store
                    .path()
                    .join("packages")
                    .join(&metadata.id)
                    .join(&metadata.version),
            )
            .unwrap();
        assert!(matches!(
            registry.preview(&metadata.id, "pack", "one"),
            Err(RegistryError::NotRunning(_))
        ));
        let entry = registry.entries.get_mut(&metadata.id).unwrap();
        entry.state = PluginState::Enabled;
        let select = |preview| UiNode::Select {
            key: "pack".into(),
            label: "Sound".into(),
            value: "one".into(),
            options: vec![bongocat_plugin_sdk::SelectOption {
                value: "one".into(),
                label: "One".into(),
                group: None,
            }],
            preview,
        };
        Arc::make_mut(&mut entry.last_output).settings.nodes = vec![select(false)];
        assert!(matches!(
            registry.preview(&metadata.id, "pack", "one"),
            Err(RegistryError::InvalidPreview)
        ));
        Arc::make_mut(&mut registry.entries.get_mut(&metadata.id).unwrap().last_output)
            .settings
            .nodes = vec![select(true)];
        for (key, value) in [("other", "one"), ("pack", "other")] {
            assert!(matches!(
                registry.preview(&metadata.id, key, value),
                Err(RegistryError::InvalidPreview)
            ));
        }
        // A valid request reaches exchange (there is deliberately no child).
        assert!(matches!(
            registry.preview(&metadata.id, "pack", "one"),
            Err(RegistryError::NotRunning(_))
        ));
        assert_eq!(registry.entries[&metadata.id].state, PluginState::Enabled);
    }

    const ACTIVITY: InputActivity = InputActivity {
        keyboard_down: 3,
        keyboard_up: 2,
        mouse_down: 1,
        mouse_up: 1,
        space_down: 1,
        space_up: 1,
        enter_down: 1,
        enter_up: 1,
        backspace_down: 1,
        backspace_up: 1,
        tab_down: 1,
        tab_up: 1,
        shift_left_down: 1,
        shift_left_up: 1,
        shift_right_down: 1,
        shift_right_up: 1,
        ctrl_left_down: 1,
        ctrl_left_up: 1,
    };

    /// Only a plugin that declared the activity capability is fed activity.
    #[test]
    fn the_declared_capability_picks_the_activity_message() {
        let message = |capabilities: &[Capability]| activity_message(capabilities, 500, ACTIVITY);

        assert!(matches!(
            message(&[Capability::InputActivityV1]),
            Some(HostMessage::InputActivity {
                elapsed_ms: 500,
                activity,
            }) if activity == ACTIVITY
        ));
        // A plugin that did not ask for activity is not an activity plugin at all.
        assert!(message(&[]).is_none());
        assert!(message(&[Capability::ModelUi]).is_none());
    }

    /// The companion geometry contract does not imply the newer state message.
    #[test]
    fn only_the_declared_capability_receives_pet_state() {
        assert!(receives_pet_state(&[Capability::PetStateV1]));
        assert!(!receives_pet_state(&[Capability::CompanionV1]));
        assert!(!receives_pet_state(&[]));
    }
}
