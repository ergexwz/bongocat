//! The plugin worker: one thread that owns the manager and drives everything.
//!
//! The loop is a fixed cadence with three jobs. It applies every command the
//! windows sent, it hands the running plugins their tick and collects what they
//! published, and it composes the panels into the one layer the model window
//! draws. Nothing here blocks on a window: a window that is closed is simply not
//! sending commands, and the plugins keep running.

use crate::command::{
    PluginCard, PluginCenterSnapshot, PluginCommand, PluginCommandError, PluginReply,
    PluginServiceClient, PluginServiceEndpoint,
};
use crate::{LOOP_INTERVAL, TICK_INTERVAL};
use bongocat_plugin_hud::{
    HudCompositor, HudConsumer, HudInteraction, HudLayer, HudProducer, HudViewport,
    HudViewportReader, HudViewportReporter, PluginHudFrame, latest_hud_channel,
    latest_viewport_channel, load_platform_font,
};
use bongocat_plugin_manager::{
    AvailablePlugin, PluginActor, PluginManager, PluginManagerError, PluginOperation, PluginSource,
    PluginState, RegistryError,
};
use bongocat_plugin_sdk::{
    ConfigValue, InputActivity, PetBounds, PetState, PluginBubble, PluginMenuNode, PluginOutput,
    UiNode,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// One plugin-owned entry in the host's companion menu.
///
/// A node with `children` is a submenu and has no action; a leaf carries the
/// action id the host sends back, and an optional check state for radio-style
/// choices such as the counter's scale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionMenuItem {
    pub plugin: String,
    pub action: Option<String>,
    pub label: String,
    pub checked: Option<bool>,
    pub children: Vec<CompanionMenuItem>,
}

/// Turn one plugin's menu publication into the tree the host renders.
///
/// The flat list stays the compatibility path; a plugin that declares
/// `plugin_menu_tree_v1` publishes the richer shape and this is where it
/// replaces the flat entries.
fn companion_menu(plugin: &str, output: &PluginOutput) -> Vec<CompanionMenuItem> {
    if output.menu_tree.is_empty() {
        output
            .menu
            .iter()
            .map(|item| CompanionMenuItem {
                plugin: plugin.to_owned(),
                action: Some(item.id.clone()),
                label: item.label.clone(),
                checked: None,
                children: Vec::new(),
            })
            .collect()
    } else {
        output
            .menu_tree
            .iter()
            .map(|node| companion_menu_node(plugin, node))
            .collect()
    }
}

fn companion_menu_node(plugin: &str, node: &PluginMenuNode) -> CompanionMenuItem {
    let children = node
        .children
        .iter()
        .map(|child| companion_menu_node(plugin, child))
        .collect::<Vec<_>>();
    CompanionMenuItem {
        plugin: plugin.to_owned(),
        action: children.is_empty().then(|| node.id.clone()),
        label: node.label.clone(),
        checked: node.checked,
        children,
    }
}

/// One bubble waiting for the host's shared notification window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPluginBubble {
    pub plugin: String,
    pub bubble: PluginBubble,
}

/// A cheap, cloneable writer for the model window's logical rectangle.
#[derive(Clone, Default)]
pub struct PetBoundsReporter {
    wake: bongocat_plugin_hud::WorkerWake,
    slot: Arc<RwLock<Option<PetBounds>>>,
}

impl PetBoundsReporter {
    pub fn set(&self, bounds: PetBounds) {
        let mut slot = self
            .slot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *slot != Some(bounds) {
            *slot = Some(bounds);
            drop(slot);
            self.wake.wake();
        }
    }

    pub fn get(&self) -> Option<PetBounds> {
        *self
            .slot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A cheap, cloneable writer for the active model and model-window visibility.
///
/// It is separate from [`PetBoundsReporter`] because visibility changes without
/// the rectangle moving (the tray can hide the model window in place), and a
/// companion that only watched the rectangle would keep showing its window over
/// an invisible pet.
#[derive(Clone, Default)]
pub struct PetStateReporter {
    wake: bongocat_plugin_hud::WorkerWake,
    slot: Arc<RwLock<Option<PetState>>>,
}

impl PetStateReporter {
    pub fn set(&self, state: PetState) {
        let mut slot = self
            .slot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.as_ref() != Some(&state) {
            *slot = Some(state);
            drop(slot);
            self.wake.wake();
        }
    }

    pub fn get(&self) -> Option<PetState> {
        self.slot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The store root a build's plugins are installed under.
///
/// Development and Production never share one: the storage root already separates
/// them, and a Development build that scanned Production's plugins would run
/// packages the user installed in the other environment. The manager owns the
/// layout below this (`packages/`, `data/`), so this is only the root.
pub fn plugin_directory(storage_root: &Path) -> PathBuf {
    storage_root.join("plugins")
}

/// The channel halves the model window owns and the service writes into.
///
/// They are handed over once, at start, because the window is what knows its own
/// size and is what draws the layer. Until the window reports a size the service
/// publishes empty layers, which is what a product with no model window yet
/// looks like.
pub struct PluginOverlayHandles {
    /// The newest composed panel layer, taken once per overlay tick.
    pub hud_consumer: HudConsumer,
    /// Where the window reports its own size and backing scale.
    pub viewport_reporter: HudViewportReporter,
    /// Where the window reports a click that landed on a plugin's control.
    pub interaction_sender: bongocat_plugin_hud::WakingSender<HudInteraction>,
}

/// The service's own end of the overlay channels, plus its command client.
pub struct PluginService {
    client: PluginServiceClient,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), PluginManagerError>>>,
    menu_items: Arc<RwLock<Vec<CompanionMenuItem>>>,
    bubbles: Arc<Mutex<Vec<QueuedPluginBubble>>>,
    pet_bounds: Arc<RwLock<Option<PetBounds>>>,
    pet_state: Arc<RwLock<Option<PetState>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginServiceStartError {
    #[error("the plugin store could not be opened: {0}")]
    Store(PluginManagerError),
    #[error("the plugin worker could not be started: {0}")]
    Spawn(std::io::Error),
    #[error("the plugin worker panicked")]
    WorkerPanicked,
    #[error("the plugin service could not shut down cleanly: {0}")]
    Shutdown(PluginManagerError),
}

impl PluginService {
    /// Start the worker and its manager.
    ///
    /// The two channel pairs are created here rather than taken as arguments, so
    /// the layer a window draws and the viewport the service composes against are
    /// always the two ends of the same conversation. The manager is opened on a
    /// background thread, so a slow or unreadable store delays nothing the user
    /// can see.
    pub fn start(
        storage_root: &Path,
        capacity: usize,
    ) -> Result<(Self, PluginOverlayHandles), PluginServiceStartError> {
        Self::start_with_source(storage_root, capacity, PluginSource::InstalledOnly, None)
    }

    pub fn start_with_source(
        storage_root: &Path,
        capacity: usize,
        source: PluginSource,
        input_activity: Option<Box<dyn Fn() -> InputActivity + Send>>,
    ) -> Result<(Self, PluginOverlayHandles), PluginServiceStartError> {
        Self::start_with_inputs(storage_root, capacity, source, input_activity, None)
    }

    pub fn start_with_events(
        storage_root: &Path,
        capacity: usize,
        source: PluginSource,
        events: Box<dyn crate::PluginInputEvents>,
    ) -> Result<(Self, PluginOverlayHandles), PluginServiceStartError> {
        Self::start_with_inputs(storage_root, capacity, source, None, Some(events))
    }

    /// Start the service with both input sources.
    ///
    /// The product passes both an absolute counter source and the ordered event
    /// feed: existing plugins declare whichever capability they parse, and the
    /// service never sends a capability-gated frame to a plugin that did not ask
    /// for it. Companion bounds and state are published through their reporters
    /// after start.
    pub fn start_with_inputs(
        storage_root: &Path,
        capacity: usize,
        source: PluginSource,
        input_activity: Option<Box<dyn Fn() -> InputActivity + Send>>,
        events: Option<Box<dyn crate::PluginInputEvents>>,
    ) -> Result<(Self, PluginOverlayHandles), PluginServiceStartError> {
        let directory = plugin_directory(storage_root);
        let (client, endpoint) = PluginServiceClient::bounded(capacity);
        let (hud_producer, hud_consumer) = latest_hud_channel();
        let (viewport_reporter, viewport) = latest_viewport_channel();
        let (interaction_sender, interactions) = bongocat_plugin_hud::waking_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let menu_items = Arc::new(RwLock::new(Vec::new()));
        let bubbles = Arc::new(Mutex::new(Vec::new()));
        let pet_bounds = Arc::new(RwLock::new(None));
        let pet_state = Arc::new(RwLock::new(None));
        let worker_stop = Arc::clone(&stop);
        let worker_menu = Arc::clone(&menu_items);
        let worker_bubbles = Arc::clone(&bubbles);
        let worker_pet_bounds = Arc::clone(&pet_bounds);
        let worker_pet_state = Arc::clone(&pet_state);
        let worker = thread::Builder::new()
            .name("bongocat-plugin-service".to_owned())
            .spawn(move || {
                run(
                    directory,
                    endpoint,
                    OverlaySinks {
                        hud_producer,
                        viewport,
                        interactions,
                    },
                    worker_stop,
                    source,
                    input_activity,
                    events,
                    CompanionSinks {
                        menu_items: worker_menu,
                        bubbles: worker_bubbles,
                        pet_bounds: worker_pet_bounds,
                        pet_state: worker_pet_state,
                    },
                )
            })
            .map_err(PluginServiceStartError::Spawn)?;
        Ok((
            Self {
                client,
                stop,
                worker: Some(worker),
                menu_items,
                bubbles,
                pet_bounds,
                pet_state,
            },
            PluginOverlayHandles {
                hud_consumer,
                viewport_reporter,
                interaction_sender,
            },
        ))
    }

    /// The window end of the command channel.
    pub fn client(&self) -> PluginServiceClient {
        self.client.clone()
    }

    /// The companion menu entries published by currently running plugins.
    pub fn menu_items(&self) -> Vec<CompanionMenuItem> {
        self.menu_items
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Take the bubbles that arrived since the host last drained the queue.
    pub fn drain_bubbles(&self) -> Vec<QueuedPluginBubble> {
        let bubbles = std::mem::take(
            &mut *self
                .bubbles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        if !bubbles.is_empty() {
            self.client.wake_signal().wake();
        }
        bubbles
    }

    /// Drop queued bubbles owned by a plugin that is no longer running.
    pub fn clear_bubbles(&self, plugin: &str) {
        self.bubbles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|queued| queued.plugin != plugin);
    }

    /// The writer the product frame loop uses to publish the pet rectangle.
    pub fn pet_bounds_reporter(&self) -> PetBoundsReporter {
        PetBoundsReporter {
            wake: self.client.wake_signal(),
            slot: Arc::clone(&self.pet_bounds),
        }
    }

    /// The writer the product frame loop uses to publish the active model and
    /// model-window visibility.
    pub fn pet_state_reporter(&self) -> PetStateReporter {
        PetStateReporter {
            wake: self.client.wake_signal(),
            slot: Arc::clone(&self.pet_state),
        }
    }

    /// Ask the worker to stop and wait for it to have stopped.
    ///
    /// Every plugin process is joined before this returns, which is what makes
    /// the plugin service a step in the product's shutdown order rather than a
    /// thread that outlives the window it drew into.
    pub fn shutdown(mut self) -> Result<(), PluginServiceStartError> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker
                .join()
                .map_err(|_| PluginServiceStartError::WorkerPanicked)?
                .map_err(PluginServiceStartError::Shutdown)?;
        }
        Ok(())
    }
}

impl Drop for PluginService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

/// The service's end of the overlay channels.
struct OverlaySinks {
    hud_producer: HudProducer,
    viewport: HudViewportReader,
    interactions: bongocat_plugin_hud::WakingReceiver<HudInteraction>,
}

struct CompanionSinks {
    menu_items: Arc<RwLock<Vec<CompanionMenuItem>>>,
    bubbles: Arc<Mutex<Vec<QueuedPluginBubble>>>,
    pet_bounds: Arc<RwLock<Option<PetBounds>>>,
    pet_state: Arc<RwLock<Option<PetState>>>,
}

/// The worker's whole state.
struct Worker {
    hud_dirty: bool,
    companion_dirty: bool,
    actors: BTreeMap<String, PluginActor>,
    retiring: Vec<PluginActor>,
    hud_frames: BTreeMap<String, PluginHudFrame>,
    pending_operations: BTreeMap<u64, PendingOperation>,
    next_operation: u64,
    locale: Option<String>,
    manager: PluginManager,
    source: PluginSource,
    offers: Vec<AvailablePlugin>,
    catalog_job: Option<CatalogJob>,
    install_jobs: BTreeMap<String, InstallJob>,
    icon_job: Option<IconJob>,
    icon_prefetch_pending: bool,
    catalog_stop: Arc<AtomicBool>,
    input_activity: Option<Box<dyn Fn() -> InputActivity + Send>>,
    last_input_activity: InputActivity,
    last_input_overflow: u64,
    input_events: Option<Box<dyn crate::PluginInputEvents>>,
    endpoint: PluginServiceEndpoint,
    hud: OverlaySinks,
    compositor: Option<HudCompositor>,
    hud_last_used: Instant,
    hud_font_failed: bool,
    /// Store/catalog failures. Command refusals travel in their typed reply.
    error: Option<String>,
    interaction_failures: std::collections::BTreeMap<String, crate::PluginInteractionFailure>,
    /// The layer last published, kept only to decide whether a new one is needed.
    ///
    /// A panel that is not changing is the common case — a countdown changes once
    /// a second and a plugin with no model view never changes at all — so the
    /// comparison is what keeps an idle plugin from costing a rasterization per
    /// loop.
    last_signature: Option<LayerSignature>,
    started: Instant,
    last_tick: Duration,
    shutdown_reply: Option<PluginReply<()>>,
    companion: CompanionSinks,
    last_pet_bounds: Option<PetBounds>,
    last_pet_state: Option<PetState>,
    seen_bubbles: BTreeMap<String, BTreeSet<String>>,
    recovery: BTreeMap<String, Recovery>,
}

enum AfterOperation {
    None,
    Enabled,
    Disabled,
    Reset,
    Uninstalled,
    Installed,
}
struct PendingOperation {
    id: String,
    reply: Option<PluginReply<Result<PluginCenterSnapshot, PluginCommandError>>>,
    after: AfterOperation,
    interaction: Option<crate::PluginInteractionFailure>,
}

struct Recovery {
    failures: usize,
    next_attempt: Instant,
    running_since: Option<Instant>,
}

struct CatalogJob {
    result: Receiver<Result<Vec<AvailablePlugin>, String>>,
    worker: JoinHandle<()>,
}

struct InstallPayload {
    offer: Box<AvailablePlugin>,
    bytes: Vec<u8>,
}

struct InstallJob {
    result: Receiver<Result<InstallPayload, String>>,
    worker: JoinHandle<()>,
    reply: Option<PluginReply<Result<PluginCenterSnapshot, PluginCommandError>>>,
}

/// Catalog artwork fetched after the cards themselves are already readable.
///
/// The UI can paint a skeleton immediately and switch to the cached path when
/// this job finishes. Keeping it separate from `CatalogJob` means a slow image
/// proxy cannot delay package installation or the plugin list.
struct IconJob {
    result: Receiver<Vec<CachedIcon>>,
    worker: JoinHandle<()>,
}

struct CachedIcon {
    id: String,
    version: String,
    source_url: String,
    cached_url: String,
}

/// What a composed layer is a function of, for skipping an unchanged one.
#[derive(Clone, PartialEq)]
struct LayerSignature {
    viewport: HudViewport,
    frames: Vec<PluginHudFrame>,
}

#[allow(clippy::too_many_arguments)]
fn run(
    directory: PathBuf,
    endpoint: PluginServiceEndpoint,
    hud: OverlaySinks,
    stop: Arc<AtomicBool>,
    source: PluginSource,
    input_activity: Option<Box<dyn Fn() -> InputActivity + Send>>,
    input_events: Option<Box<dyn crate::PluginInputEvents>>,
    companion: CompanionSinks,
) -> Result<(), PluginManagerError> {
    endpoint.bind_waker();
    let Ok(mut manager) = PluginManager::new(directory) else {
        // The store could not be opened at all. The service still answers, so the
        // plugin center can say what happened instead of reporting a service that
        // never started.
        let mut unavailable = Unavailable {
            endpoint,
            error: Some(STORE_OPEN_FAILED.to_owned()),
        };
        while !stop.load(Ordering::Acquire) {
            if unavailable.drain() {
                break;
            }
            thread::park();
        }
        return Ok(());
    };
    if matches!(
        &source,
        PluginSource::Production {
            bundled_catalog_sha256: Some(_),
            ..
        }
    ) {
        manager.retire_legacy_plugins();
    }
    let mut error = None;
    if let Err(failure) = manager.discover_plugins() {
        error = Some(failure.to_string());
    }
    if let Err(failure) = source.install_bundled(&mut manager, env!("CARGO_PKG_VERSION"), &stop) {
        error = Some(failure);
    }
    // Resume failures are represented by each affected card's Failed state.
    // Resume is queued per plugin after the service owner is ready.
    let last_input_activity = input_activity
        .as_ref()
        .map_or(InputActivity::default(), |read| read());
    let mut worker = Worker {
        hud_dirty: true,
        companion_dirty: true,
        actors: BTreeMap::new(),
        retiring: Vec::new(),
        hud_frames: BTreeMap::new(),
        pending_operations: BTreeMap::new(),
        next_operation: 0,
        locale: None,
        manager,
        source,
        offers: Vec::new(),
        catalog_job: None,
        install_jobs: BTreeMap::new(),
        icon_job: None,
        icon_prefetch_pending: false,
        catalog_stop: Arc::clone(&stop),
        input_activity,
        last_input_activity,
        last_input_overflow: 0,
        input_events,
        endpoint,
        hud,
        compositor: None,
        hud_last_used: Instant::now(),
        hud_font_failed: false,
        error,
        interaction_failures: Default::default(),
        last_signature: None,
        started: Instant::now(),
        last_tick: Duration::ZERO,
        shutdown_reply: None,
        companion,
        last_pet_bounds: None,
        last_pet_state: None,
        seen_bubbles: BTreeMap::new(),
        recovery: BTreeMap::new(),
    };
    for (metadata, _) in worker.manager.list_plugins() {
        if worker.manager.plugin_is_enabled(&metadata.id) {
            worker.queue_operation(
                &metadata.id,
                PluginOperation::Enable(true),
                None,
                AfterOperation::Enabled,
            );
        }
    }
    worker.initialize_running_input_activity();
    worker.run(stop)
}

const STORE_OPEN_FAILED: &str = "the plugin store could not be opened";
const COMMANDS_PER_TURN: usize = 16;
const INTERACTIONS_PER_TURN: usize = 64;

/// The worker when the plugin store itself could not be opened.
struct Unavailable {
    endpoint: PluginServiceEndpoint,
    error: Option<String>,
}

impl Unavailable {
    fn drain(&mut self) -> bool {
        for _ in 0..COMMANDS_PER_TURN {
            let Ok(Some(command)) = self.endpoint.try_recv() else {
                break;
            };
            let center = PluginCenterSnapshot {
                cards: Vec::new(),
                error: self.error.clone(),
                loading: false,
            };
            match command {
                PluginCommand::ReadCenter { reply } | PluginCommand::Refresh { reply } => {
                    let _ = reply.respond(center);
                }
                PluginCommand::SetEnabled { reply, .. }
                | PluginCommand::InstallAvailable { reply, .. }
                | PluginCommand::InstallLocal { reply, .. }
                | PluginCommand::Uninstall { reply, .. }
                | PluginCommand::ResetData { reply, .. }
                | PluginCommand::Action { reply, .. }
                | PluginCommand::Preview { reply, .. }
                | PluginCommand::Configure { reply, .. }
                | PluginCommand::SetLocale { reply, .. } => {
                    let _ = reply.respond(Err::<PluginCenterSnapshot, PluginCommandError>(
                        PluginCommandError::ServiceUnavailable,
                    ));
                }
                PluginCommand::Shutdown { reply } => {
                    self.endpoint.close();
                    let _ = reply.respond(());
                    return true;
                }
            }
        }
        false
    }
}

impl Worker {
    fn next_wait(&self) -> Option<Duration> {
        if self.endpoint.has_pending() {
            return Some(Duration::ZERO);
        }
        let now = Instant::now();
        let mut waits = Vec::new();
        if self.input_activity.is_some() && self.input_events.is_none() {
            waits.push(LOOP_INTERVAL);
        }
        if self.manager.list_plugins().iter().any(|(metadata, state)| {
            *state == PluginState::Enabled && metadata.protocol_version == 1
        }) {
            waits.push((self.last_tick + TICK_INTERVAL).saturating_sub(self.started.elapsed()));
        }
        for (id, recovery) in &self.recovery {
            if self
                .pending_operations
                .values()
                .any(|pending| pending.id == *id)
                || self.is_installing(id)
            {
                continue;
            }
            if let Some(since) = recovery.running_since {
                waits.push((since + Duration::from_secs(300)).saturating_duration_since(now));
            } else if recovery.failures < 3 {
                waits.push(recovery.next_attempt.saturating_duration_since(now));
            }
        }
        if self.compositor.is_some()
            && self.last_signature.as_ref().is_some_and(|signature| {
                signature
                    .frames
                    .iter()
                    .all(|frame| frame.view.nodes.is_empty())
            })
        {
            waits.push(
                (self.hud_last_used + Duration::from_secs(30)).saturating_duration_since(now),
            );
        }
        waits.into_iter().min()
    }

    fn apply_projection(
        &mut self,
        id: &str,
        projection: Option<bongocat_plugin_manager::PluginProjection>,
    ) {
        let previous = self.manager.plugin_output(id);
        let state = self
            .manager
            .list_plugins()
            .into_iter()
            .find(|(metadata, _)| metadata.id == id)
            .map(|(_, state)| state);
        let state_changed = state != projection.as_ref().map(|p| p.state);
        let model_changed = state_changed
            || previous.map(|o| &o.model) != projection.as_ref().map(|p| &p.output.model);
        self.hud_dirty |= model_changed;
        if model_changed {
            match projection
                .as_ref()
                .filter(|p| p.state == PluginState::Enabled)
            {
                Some(p) => {
                    self.hud_frames.insert(
                        id.to_owned(),
                        PluginHudFrame::new(id, p.output.model.clone()),
                    );
                }
                None => {
                    self.hud_frames.remove(id);
                }
            }
        }
        self.companion_dirty |= state_changed
            || previous.map(|o| (&o.menu, &o.menu_tree, &o.bubbles))
                != projection
                    .as_ref()
                    .map(|p| (&p.output.menu, &p.output.menu_tree, &p.output.bubbles));
        self.manager.apply_projection(id, projection);
    }
    fn is_installing(&self, id: &str) -> bool {
        self.install_jobs.contains_key(id)
            || self.pending_operations.values().any(|pending| {
                pending.id == id && matches!(pending.after, AfterOperation::Installed)
            })
    }

    fn ensure_actor(&mut self, id: &str) -> bool {
        if self.actors.contains_key(id) {
            return true;
        }
        match self.manager.spawn_actor(id) {
            Ok(actor) => {
                if let Some(locale) = &self.locale {
                    let _ = actor.submit(0, PluginOperation::Locale(locale.clone()));
                }
                self.actors.insert(id.into(), actor);
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.manager.mark_actor_failed(id);
                self.hud_dirty = true;
                self.companion_dirty = true;
                false
            }
        }
    }

    fn queue_operation(
        &mut self,
        id: &str,
        operation: PluginOperation,
        reply: Option<PluginReply<Result<PluginCenterSnapshot, PluginCommandError>>>,
        after: AfterOperation,
    ) {
        if !self.is_installing(id)
            && !matches!(
                operation,
                PluginOperation::Install { .. }
                    | PluginOperation::Reset
                    | PluginOperation::Uninstall
            )
            && !self
                .manager
                .list_plugins()
                .iter()
                .any(|(metadata, _)| metadata.id == id)
        {
            if let Some(reply) = reply {
                self.respond(reply, Err(PluginCommandError::NotFound));
            }
            return;
        }
        if self.is_installing(id) || !self.ensure_actor(id) {
            if let Some(reply) = reply {
                self.respond(reply, Err(PluginCommandError::Operation));
            }
            return;
        }
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .expect("operation sequence exhausted");
        let sequence = self.next_operation;
        let interaction = match &operation {
            PluginOperation::Action(action) => Some(crate::PluginInteractionFailure::Action {
                action: action.clone(),
                code: PluginCommandError::Operation,
            }),
            PluginOperation::Configure(key, _) => {
                Some(crate::PluginInteractionFailure::Configure {
                    key: key.clone(),
                    code: PluginCommandError::Operation,
                })
            }
            _ => None,
        };
        match self.actors[id].submit(sequence, operation) {
            Ok(()) => {
                self.pending_operations.insert(
                    sequence,
                    PendingOperation {
                        id: id.into(),
                        reply,
                        after,
                        interaction,
                    },
                );
            }
            Err(error) => {
                if let Some(reply) = reply {
                    self.respond(
                        reply,
                        Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                            PluginCommandError::Busy
                        } else {
                            PluginCommandError::ServiceUnavailable
                        }),
                    );
                }
            }
        }
    }

    fn signal(&mut self, id: &str, operation: PluginOperation) {
        if self.ensure_actor(id) && self.actors[id].submit(0, operation).is_err() {
            self.error = Some(format!("plugin queue is full: {id}"));
        }
    }

    fn broadcast(&mut self, make: impl Fn() -> PluginOperation) {
        let operation = make();
        for (metadata, state) in self.manager.list_plugins() {
            if state == PluginState::Enabled && operation.accepts(&metadata) {
                self.signal(&metadata.id, operation.clone());
            }
        }
    }

    fn finish_actor_work(&mut self) {
        let mut index = 0;
        while index < self.retiring.len() {
            if self.retiring[index].is_finished() {
                if let Err(error) = self.retiring.swap_remove(index).join() {
                    self.error = Some(error.to_string());
                }
            } else {
                index += 1;
            }
        }
        let failed: Vec<_> = self
            .actors
            .iter()
            .filter(|(_, actor)| actor.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in failed {
            self.actors.remove(&id);
            self.hud_frames.remove(&id);
            self.manager.mark_actor_failed(&id);
            self.hud_dirty = true;
            self.companion_dirty = true;
            let sequences: Vec<_> = self
                .pending_operations
                .iter()
                .filter(|(_, pending)| pending.id == id)
                .map(|(sequence, _)| *sequence)
                .collect();
            for sequence in sequences {
                if let Some(pending) = self.pending_operations.remove(&sequence)
                    && let Some(reply) = pending.reply
                {
                    self.respond(reply, Err(PluginCommandError::ServiceUnavailable));
                }
            }
        }
        let mut completed = Vec::new();
        let mut projections = Vec::new();
        for (id, actor) in &self.actors {
            let mut bubbles = self
                .companion
                .bubbles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let capacity = 256usize.saturating_sub(bubbles.len());
            bubbles.extend(actor.take_events(capacity).into_iter().map(|bubble| {
                QueuedPluginBubble {
                    plugin: id.clone(),
                    bubble,
                }
            }));
            drop(bubbles);
            if let Some(projection) = actor.take_projection() {
                projections.push((id.clone(), projection));
            }
            while let Ok(completion) = actor.try_completion() {
                completed.push(completion);
            }
            // A completion is published after its projection. Re-read after
            // draining replies so the acknowledgement never exposes stale state.
            if let Some(projection) = actor.take_projection() {
                projections.push((id.clone(), projection));
            }
        }
        for (id, projection) in projections {
            self.apply_projection(&id, projection);
        }
        for completion in completed {
            let Some(pending) = self.pending_operations.remove(&completion.sequence) else {
                continue;
            };
            let result = completion.result.map_err(|error| {
                let code = operation(&error);
                self.error = Some(error.to_string());
                code
            });
            if result.is_ok() {
                self.interaction_failures.remove(&pending.id);
                match pending.after {
                    AfterOperation::Enabled | AfterOperation::Reset | AfterOperation::Installed => {
                        self.initialize_plugin_input_activity(&pending.id);
                        self.last_pet_bounds = None;
                        self.last_pet_state = None;
                    }
                    AfterOperation::Disabled | AfterOperation::Uninstalled => {
                        self.clear_plugin_bubbles(&pending.id)
                    }
                    AfterOperation::None => {}
                }
            } else if let (Err(code), Some(interaction)) = (&result, pending.interaction) {
                let failure = match interaction {
                    crate::PluginInteractionFailure::Action { action, .. } => {
                        crate::PluginInteractionFailure::Action {
                            action,
                            code: code.clone(),
                        }
                    }
                    crate::PluginInteractionFailure::Configure { key, .. } => {
                        crate::PluginInteractionFailure::Configure {
                            key,
                            code: code.clone(),
                        }
                    }
                };
                self.interaction_failures
                    .insert(pending.id.clone(), failure);
            }
            if let Some(reply) = pending.reply {
                self.respond(reply, result);
            }
        }
        let idle: Vec<_> = self
            .actors
            .keys()
            .filter(|id| {
                !self.manager.plugin_is_enabled(id)
                    && !self
                        .pending_operations
                        .values()
                        .any(|pending| &pending.id == *id)
            })
            .cloned()
            .collect();
        for id in idle {
            if let Some(actor) = self.actors.remove(&id) {
                actor.request_stop();
                self.retiring.push(actor);
            }
        }
    }

    fn run(&mut self, stop: Arc<AtomicBool>) -> Result<(), PluginManagerError> {
        self.endpoint.bind_waker();
        self.hud.viewport.set_waker(thread::current());
        self.hud.interactions.set_waker(thread::current());
        if let Some(events) = &self.input_events {
            events.set_waker(thread::current());
        }
        self.refresh_catalog();
        while !stop.load(Ordering::Acquire) {
            self.finish_actor_work();
            self.finish_catalog_job();
            self.finish_install_jobs();
            self.finish_icon_job();
            self.drain_commands();
            if stop.load(Ordering::Acquire) {
                break;
            }
            self.dispatch_input_events();
            self.dispatch_input_activity();
            self.drain_interactions();
            self.dispatch_tick();
            self.finish_actor_work();
            self.recover_plugins();
            self.publish_companion();
            self.compose();
            match self.next_wait() {
                Some(wait) => thread::park_timeout(wait),
                None => thread::park(),
            }
        }
        self.endpoint.close();
        if let Some(job) = self.catalog_job.take() {
            let _ = job.worker.join();
        }
        for (_, job) in std::mem::take(&mut self.install_jobs) {
            let _ = job.worker.join();
            if let Some(reply) = job.reply {
                let _ = reply.respond(Err(PluginCommandError::ServiceUnavailable));
            }
        }
        if let Some(job) = self.icon_job.take() {
            let _ = job.worker.join();
        }
        for actor in self.actors.values() {
            actor.request_stop();
        }
        let mut shutdown_error = None;
        for actor in std::mem::take(&mut self.retiring) {
            if let Err(error) = actor.join() {
                shutdown_error.get_or_insert(error);
            }
        }
        for (_, actor) in std::mem::take(&mut self.actors) {
            if let Err(error) = actor.join() {
                shutdown_error.get_or_insert(error);
            }
        }
        for (_, pending) in std::mem::take(&mut self.pending_operations) {
            if let Some(reply) = pending.reply {
                let _ = reply.respond(Err(PluginCommandError::ServiceUnavailable));
            }
        }
        if let Some(error) = shutdown_error {
            return Err(error);
        }
        self.manager.shutdown()?;
        if let Some(reply) = self.shutdown_reply.take() {
            let _ = reply.respond(());
        }
        Ok(())
    }

    /// Publish the newest companion menu, queue unseen bubbles and forward the
    /// model window rectangle, active model and visibility. Menu and bubble
    /// reads are cheap: a few plugins and bounded outputs, once per service
    /// loop.
    fn publish_companion(&mut self) {
        if self.companion_dirty {
            self.companion_dirty = false;
            let mut menu = Vec::new();
            let mut arrivals = Vec::new();
            for (metadata, state) in self.manager.list_plugins() {
                if state != PluginState::Enabled {
                    continue;
                }
                let Some(output) = self.manager.plugin_output(&metadata.id) else {
                    continue;
                };
                menu.extend(companion_menu(&metadata.id, output));
                let seen = self.seen_bubbles.entry(metadata.id.clone()).or_default();
                for bubble in &output.bubbles {
                    if seen.insert(bubble.id.clone()) {
                        arrivals.push(QueuedPluginBubble {
                            plugin: metadata.id.clone(),
                            bubble: bubble.clone(),
                        });
                    }
                }
                // A plugin publishes only its recent bubble ring, so a set that has
                // grown far past that ring cannot still be matching live entries.
                if seen.len() > 512 {
                    seen.clear();
                }
            }
            menu.truncate(16);
            let menu_changed = {
                let current = self
                    .companion
                    .menu_items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                current.as_slice() != menu.as_slice()
            };
            if menu_changed {
                *self
                    .companion
                    .menu_items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = menu;
            }
            if !arrivals.is_empty() {
                let mut bubbles = self
                    .companion
                    .bubbles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let capacity = 256usize.saturating_sub(bubbles.len());
                if arrivals.len() > capacity {
                    self.error = Some("legacy plugin bubble queue is full".into());
                }
                bubbles.extend(arrivals.into_iter().take(capacity));
            }
        }
        let bounds = *self
            .companion
            .pet_bounds
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(bounds) = bounds
            && self.last_pet_bounds != Some(bounds)
        {
            self.broadcast(|| PluginOperation::Bounds(bounds));
            self.last_pet_bounds = Some(bounds);
        }
        let pet_state = self
            .companion
            .pet_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(state) = pet_state
            && self.last_pet_state.as_ref() != Some(&state)
        {
            self.broadcast(|| PluginOperation::Pet(state.clone()));
            self.last_pet_state = Some(state);
        }
    }

    fn clear_plugin_bubbles(&self, plugin: &str) {
        self.companion
            .bubbles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|queued| queued.plugin != plugin);
    }

    fn start_catalog_job(
        &mut self,
        operation: impl FnOnce() -> Result<Vec<AvailablePlugin>, String> + Send + 'static,
    ) {
        let (sender, result) = sync_channel(1);
        let owner = thread::current();
        match thread::Builder::new()
            .name("bongocat-plugin-catalog".into())
            .spawn(move || {
                let _ = sender.send(operation());
                owner.unpark();
            }) {
            Ok(worker) => self.catalog_job = Some(CatalogJob { result, worker }),
            Err(_) => self.error = Some("catalog worker unavailable".into()),
        }
    }

    fn refresh_catalog(&mut self) {
        if self.catalog_job.is_some() {
            return;
        }
        if matches!(self.source, PluginSource::InstalledOnly) {
            return;
        }
        let source = self.source.clone();
        let stop = Arc::clone(&self.catalog_stop);
        self.start_catalog_job(move || source.discover(env!("CARGO_PKG_VERSION"), &stop));
    }

    fn finish_catalog_job(&mut self) {
        let Some(job) = &self.catalog_job else {
            return;
        };
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("catalog worker failed".into()),
        };
        if let Some(job) = self.catalog_job.take() {
            let _ = job.worker.join();
        }
        match result {
            Ok(offers) => {
                self.offers = offers;
                self.start_icon_prefetch();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn start_install_job(
        &mut self,
        id: String,
        operation: impl FnOnce() -> Result<InstallPayload, String> + Send + 'static,
    ) -> bool {
        let (sender, result) = sync_channel(1);
        let owner = thread::current();
        match thread::Builder::new()
            .name("bongocat-plugin-install".into())
            .spawn(move || {
                let _ = sender.send(operation());
                owner.unpark();
            }) {
            Ok(worker) => {
                self.install_jobs.insert(
                    id,
                    InstallJob {
                        result,
                        worker,
                        reply: None,
                    },
                );
                true
            }
            Err(_) => {
                self.error = Some("package worker unavailable".into());
                false
            }
        }
    }

    fn finish_install_jobs(&mut self) {
        let completed = self
            .install_jobs
            .iter()
            .filter_map(|(id, job)| match job.result.try_recv() {
                Ok(result) => Some((id.clone(), result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some((id.clone(), Err("package worker failed".into())))
                }
            })
            .collect::<Vec<_>>();
        for (id, result) in completed {
            let Some(job) = self.install_jobs.remove(&id) else {
                continue;
            };
            let _ = job.worker.join();
            let InstallPayload { offer, bytes } = match result {
                Ok(payload) => payload,
                Err(error) => {
                    self.error = Some(error);
                    if let Some(reply) = job.reply {
                        self.respond(reply, Err(PluginCommandError::Operation));
                    }
                    continue;
                }
            };
            self.dispatch_input_events();
            self.dispatch_input_activity();
            // Explicit installs, including reinstalls, auto-start; updates keep
            // the installed plugin's enable choice.
            let new_install = self.manager.installed_version(&offer.metadata.id).is_none();
            self.queue_operation(
                &offer.metadata.id,
                PluginOperation::Install {
                    bytes,
                    digest: Some(offer.digest),
                    enable: new_install,
                },
                job.reply,
                AfterOperation::Installed,
            );
        }
    }

    fn start_icon_prefetch(&mut self) {
        if self.icon_job.is_some() {
            self.icon_prefetch_pending = true;
            return;
        }
        let offers = self
            .offers
            .iter()
            .filter(|offer| {
                offer.icon_url.starts_with("https://") || offer.local_package_path().is_some()
            })
            .cloned()
            .collect::<Vec<_>>();
        if offers.is_empty() {
            return;
        }
        let plugin_dir = self.manager.plugin_dir().to_path_buf();
        let stop = Arc::clone(&self.catalog_stop);
        let (sender, result) = sync_channel(1);
        let owner = thread::current();
        match thread::Builder::new()
            .name("bongocat-plugin-icons".into())
            .spawn(move || {
                let mut cached = Vec::new();
                for mut offer in offers {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let original = offer.icon_url.clone();
                    if offer.cache_icon(&plugin_dir, &stop).is_ok() && offer.icon_url != original {
                        cached.push(CachedIcon {
                            id: offer.metadata.id,
                            version: offer.metadata.version,
                            source_url: original,
                            cached_url: offer.icon_url,
                        });
                    }
                }
                let _ = sender.send(cached);
                owner.unpark();
            }) {
            Ok(worker) => self.icon_job = Some(IconJob { result, worker }),
            Err(_) => self.icon_prefetch_pending = false,
        }
    }

    fn finish_icon_job(&mut self) {
        let Some(job) = &self.icon_job else {
            return;
        };
        let cached = match job.result.try_recv() {
            Ok(cached) => cached,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Vec::new(),
        };
        if let Some(job) = self.icon_job.take() {
            let _ = job.worker.join();
        }
        for cached in cached {
            if let Some(offer) = self.offers.iter_mut().find(|offer| {
                offer.metadata.id == cached.id
                    && offer.metadata.version == cached.version
                    && offer.icon_url == cached.source_url
            }) {
                offer.icon_url = cached.cached_url;
            }
        }
        if std::mem::take(&mut self.icon_prefetch_pending) {
            self.start_icon_prefetch();
        }
    }

    /// Hand the running plugins the device activity that happened since the last
    /// poll.
    ///
    /// The counters are absolute, so an unchanged read is what says nothing
    /// happened: an idle desktop costs one comparison per loop and no message at
    /// all, and a plugin that declared no activity capability is skipped inside
    /// the manager rather than here.
    fn dispatch_input_events(&mut self) {
        if let Some(source) = &mut self.input_events {
            let (events, overflow) = source.drain();
            let delta = overflow.saturating_sub(self.last_input_overflow);
            self.last_input_overflow = overflow;
            if !events.is_empty() || delta > 0 {
                self.broadcast(|| PluginOperation::Input(events.clone(), delta));
            }
        }
    }

    fn dispatch_input_activity(&mut self) {
        let Some(read) = &self.input_activity else {
            return;
        };
        let current = read();
        if current.advanced_since(self.last_input_activity) {
            self.last_input_activity = current;
            let now = self.now_ms();
            self.broadcast(|| PluginOperation::Activity(now, current));
        }
    }

    /// Give newly started counter-based plugins the current totals without
    /// replaying input that happened before their process existed.
    fn initialize_running_input_activity(&mut self) {
        let ids = self
            .manager
            .list_plugins()
            .into_iter()
            .filter(|(_, state)| *state == PluginState::Enabled)
            .map(|(metadata, _)| metadata.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.initialize_plugin_input_activity(&id);
        }
    }

    fn initialize_plugin_input_activity(&mut self, id: &str) {
        let Some(read) = &self.input_activity else {
            return;
        };
        let current = read();
        let now = self.now_ms();
        self.signal(id, PluginOperation::Activity(now, current));
    }

    fn recover_plugins(&mut self) {
        const DELAYS: [Duration; 3] = [
            Duration::from_secs(1),
            Duration::from_secs(3),
            Duration::from_secs(10),
        ];
        let now = Instant::now();
        let plugins = self.manager.list_plugins();
        self.recovery.retain(|id, _| {
            plugins.iter().any(|(metadata, _)| &metadata.id == id)
                && self.manager.plugin_is_enabled(id)
        });
        for (metadata, state) in plugins {
            let id = metadata.id;
            if !self.manager.plugin_is_enabled(&id) {
                continue;
            }
            if state == PluginState::Enabled {
                if let Some(recovery) = self.recovery.get_mut(&id) {
                    let since = recovery.running_since.get_or_insert(now);
                    if now.duration_since(*since) >= Duration::from_secs(300) {
                        self.recovery.remove(&id);
                    }
                }
                continue;
            }
            if state != PluginState::Failed
                || self.is_installing(&id)
                || self.pending_operations.values().any(|p| p.id == id)
            {
                continue;
            }
            let recovery = self.recovery.entry(id.clone()).or_insert(Recovery {
                failures: 0,
                next_attempt: now + DELAYS[0],
                running_since: None,
            });
            if recovery.running_since.take().is_some() && recovery.failures < DELAYS.len() {
                recovery.next_attempt = now + DELAYS[recovery.failures];
            }
            if recovery.failures >= DELAYS.len() || now < recovery.next_attempt {
                continue;
            }
            recovery.failures += 1;
            let attempt = recovery.failures;
            self.queue_operation(
                &id,
                PluginOperation::Enable(true),
                None,
                AfterOperation::Enabled,
            );
            if attempt < DELAYS.len() {
                self.recovery
                    .get_mut(&id)
                    .expect("recovery exists")
                    .next_attempt = Instant::now() + DELAYS[attempt];
            }
        }
    }

    /// The elapsed monotonic clock a plugin counts with.
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn center(&self) -> PluginCenterSnapshot {
        let icon_urls = self
            .offers
            .iter()
            .filter(|offer| !offer.icon_url.is_empty())
            .map(|offer| (offer.metadata.id.clone(), offer.icon_url.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut cards: Vec<PluginCard> = self
            .manager
            .list_plugins()
            .into_iter()
            .map(|(metadata, state)| {
                let output = self.manager.plugin_output(&metadata.id);
                let version = self.manager.installed_version(&metadata.id);
                let available_version = self
                    .offers
                    .iter()
                    .find(|offer| offer.metadata.id == metadata.id)
                    .map(|offer| offer.metadata.version.clone());
                let update_available = version
                    .as_ref()
                    .zip(available_version.as_ref())
                    .is_some_and(|(installed, available)| {
                        bongocat_plugin_manager::is_newer(available, installed)
                    });
                PluginCard {
                    id: metadata.id.clone(),
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                    localizations: metadata.localizations,
                    icon_url: icon_urls
                        .get(&metadata.id)
                        .cloned()
                        .or_else(|| self.manager.installed_icon_url(&metadata.id)),
                    enabled: self.manager.plugin_is_enabled(&metadata.id),
                    failed: state == PluginState::Failed,
                    version,
                    available_version,
                    update_available,
                    installing: self.is_installing(&metadata.id),
                    actions: nodes(output, |output| &output.model.nodes),
                    settings: nodes(output, |output| &output.settings.nodes),
                    errors: nodes(output, |output| &output.errors),
                    interaction_failure: self.interaction_failures.get(&metadata.id).cloned(),
                }
            })
            .collect();
        for offer in &self.offers {
            if cards.iter().any(|card| card.id == offer.metadata.id) {
                continue;
            }
            // A bad installed manifest/session may prevent registration. Keep
            // its reset/uninstall controls reachable using verified catalog copy.
            let version = self.manager.installed_version(&offer.metadata.id);
            let update_available = version.as_ref().is_some_and(|installed| {
                bongocat_plugin_manager::is_newer(&offer.metadata.version, installed)
            });
            cards.push(PluginCard {
                id: offer.metadata.id.clone(),
                name: offer.metadata.name.clone(),
                description: offer.metadata.description.clone(),
                localizations: offer.metadata.localizations.clone(),
                icon_url: Some(offer.icon_url.clone()),
                enabled: false,
                failed: version.is_some(),
                version,
                available_version: Some(offer.metadata.version.clone()),
                update_available,
                installing: self.is_installing(&offer.metadata.id),
                actions: Vec::new(),
                settings: Vec::new(),
                errors: Vec::new(),
                interaction_failure: self.interaction_failures.get(&offer.metadata.id).cloned(),
            });
        }
        PluginCenterSnapshot {
            cards,
            error: self.error.clone(),
            loading: self.catalog_job.is_some(),
        }
    }

    fn drain_commands(&mut self) {
        for _ in 0..COMMANDS_PER_TURN {
            match self.endpoint.try_recv() {
                Ok(Some(command)) => {
                    self.apply(command);
                    if self.catalog_stop.load(Ordering::Acquire) {
                        break;
                    }
                }
                Ok(None) => return,
                Err(_) => return,
            }
        }
    }

    fn apply(&mut self, command: PluginCommand) {
        let command = match command {
            PluginCommand::InstallLocal { bytes, reply } => {
                match bongocat_plugin_manager::install::inspect_package_with_limit(
                    &bytes,
                    bongocat_plugin_manager::install::MAX_LOCAL_INSTALL_BYTES,
                ) {
                    Ok(manifest) => {
                        // A local package can also be an update. Preserve the
                        // existing enable choice just as catalog updates do.
                        let new_install = self
                            .manager
                            .installed_version(&manifest.metadata.id)
                            .is_none();
                        self.queue_operation(
                            &manifest.metadata.id,
                            PluginOperation::Install {
                                bytes,
                                digest: None,
                                enable: new_install,
                            },
                            Some(reply),
                            AfterOperation::Installed,
                        );
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.respond(reply, Err(PluginCommandError::Operation));
                    }
                }
                return;
            }
            PluginCommand::SetEnabled { id, enabled, reply } => {
                self.recovery.remove(&id);
                self.queue_operation(
                    &id,
                    PluginOperation::Enable(enabled),
                    Some(reply),
                    if enabled {
                        AfterOperation::Enabled
                    } else {
                        AfterOperation::Disabled
                    },
                );
                return;
            }
            PluginCommand::Action {
                plugin,
                action,
                reply,
            } => {
                self.queue_operation(
                    &plugin,
                    PluginOperation::Action(action),
                    Some(reply),
                    AfterOperation::None,
                );
                return;
            }
            PluginCommand::Preview {
                plugin,
                key,
                value,
                reply,
            } => {
                self.queue_operation(
                    &plugin,
                    PluginOperation::Preview(key, value),
                    Some(reply),
                    AfterOperation::None,
                );
                return;
            }
            PluginCommand::Configure {
                plugin,
                key,
                value,
                reply,
            } => {
                self.queue_operation(
                    &plugin,
                    PluginOperation::Configure(key, value),
                    Some(reply),
                    AfterOperation::None,
                );
                return;
            }
            PluginCommand::ResetData { id, reply } => {
                self.queue_operation(
                    &id,
                    PluginOperation::Reset,
                    Some(reply),
                    AfterOperation::Reset,
                );
                return;
            }
            PluginCommand::Uninstall { id, reply } => {
                self.queue_operation(
                    &id,
                    PluginOperation::Uninstall,
                    Some(reply),
                    AfterOperation::Uninstalled,
                );
                return;
            }
            PluginCommand::SetLocale { locale, reply } => {
                self.locale = Some(locale.clone());
                self.broadcast(|| PluginOperation::Locale(locale.clone()));
                self.respond(reply, Ok(()));
                return;
            }
            command => command,
        };
        match command {
            PluginCommand::ReadCenter { reply } => {
                let _ = reply.respond(self.center());
            }
            PluginCommand::Refresh { reply } => {
                self.error = None;
                if let Err(error) = self.manager.discover_plugins() {
                    self.error = Some(error.to_string());
                }
                self.refresh_catalog();
                let _ = reply.respond(self.center());
            }
            PluginCommand::InstallAvailable { id, reply } => {
                if self.is_installing(&id) {
                    self.respond(reply, Err(PluginCommandError::Operation));
                    return;
                }
                let Some(offer) = self
                    .offers
                    .iter()
                    .find(|offer| offer.metadata.id == id)
                    .cloned()
                else {
                    self.respond(reply, Err(PluginCommandError::NotFound));
                    return;
                };
                if self
                    .manager
                    .installed_version(&id)
                    .is_some_and(|installed| {
                        !bongocat_plugin_manager::is_newer(&offer.metadata.version, &installed)
                    })
                {
                    self.respond(reply, Err(PluginCommandError::Operation));
                    return;
                }
                self.error = None;
                let stop = Arc::clone(&self.catalog_stop);
                let job_id = id.clone();
                if self.start_install_job(job_id, move || {
                    let bytes = offer.read(&stop)?;
                    Ok(InstallPayload {
                        offer: Box::new(offer),
                        bytes,
                    })
                }) {
                    self.install_jobs
                        .get_mut(&id)
                        .expect("install job started")
                        .reply = Some(reply);
                } else {
                    self.respond(reply, Err(PluginCommandError::Operation));
                }
            }
            PluginCommand::SetEnabled { .. }
            | PluginCommand::InstallLocal { .. }
            | PluginCommand::Uninstall { .. }
            | PluginCommand::ResetData { .. }
            | PluginCommand::Action { .. }
            | PluginCommand::Preview { .. }
            | PluginCommand::Configure { .. }
            | PluginCommand::SetLocale { .. } => unreachable!("plugin IO is routed to its actor"),
            PluginCommand::Shutdown { reply } => {
                self.catalog_stop.store(true, Ordering::Release);
                self.endpoint.close();
                // The loop acknowledges only after all workers stopped cleanly.
                self.shutdown_reply = Some(reply);
            }
        }
    }

    fn respond(
        &mut self,
        reply: PluginReply<Result<PluginCenterSnapshot, PluginCommandError>>,
        result: Result<(), PluginCommandError>,
    ) {
        let center = result.map(|()| self.center());
        let _ = reply.respond(center);
    }

    #[cfg(test)]
    fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), PluginCommandError> {
        if !self
            .manager
            .list_plugins()
            .iter()
            .any(|(metadata, _)| metadata.id == id)
        {
            return Err(PluginCommandError::NotFound);
        }
        self.recovery.remove(id);
        let result = if enabled {
            self.manager.enable_plugin(id)
        } else {
            self.manager.disable_plugin(id)
        };
        result.map_err(|error| operation(&error))
    }

    fn drain_interactions(&mut self) {
        for _ in 0..INTERACTIONS_PER_TURN {
            match self.hud.interactions.try_recv() {
                Ok(interaction) => {
                    if !self.manager.list_plugins().iter().any(|(metadata, state)| {
                        metadata.id == interaction.plugin && *state == PluginState::Enabled
                    }) {
                        continue;
                    }
                    let Some(output) = self.manager.plugin_output(&interaction.plugin) else {
                        continue;
                    };
                    let (result, failure) =
                        match output.model.nodes.iter().find(|node| match node {
                            UiNode::Button {
                                id, loading: false, ..
                            } => id == &interaction.id,
                            UiNode::Toggle { key, .. } => key == &interaction.id,
                            _ => false,
                        }) {
                            Some(UiNode::Button { .. }) => (
                                {
                                    self.queue_operation(
                                        &interaction.plugin,
                                        PluginOperation::Action(interaction.id.clone()),
                                        None,
                                        AfterOperation::None,
                                    );
                                    Ok(())
                                },
                                crate::PluginInteractionFailure::Action {
                                    action: interaction.id.clone(),
                                    code: PluginCommandError::Operation,
                                },
                            ),
                            Some(UiNode::Toggle { key, value, .. }) => {
                                let key = key.clone();
                                (
                                    {
                                        self.queue_operation(
                                            &interaction.plugin,
                                            PluginOperation::Configure(
                                                key.clone(),
                                                ConfigValue::Boolean(!value),
                                            ),
                                            None,
                                            AfterOperation::None,
                                        );
                                        Ok(())
                                    },
                                    crate::PluginInteractionFailure::Configure {
                                        key,
                                        code: PluginCommandError::Operation,
                                    },
                                )
                            }
                            _ => continue,
                        };
                    if let Err(error) = result {
                        let failure = match failure {
                            crate::PluginInteractionFailure::Action { action, .. } => {
                                crate::PluginInteractionFailure::Action {
                                    action,
                                    code: error,
                                }
                            }
                            crate::PluginInteractionFailure::Configure { key, .. } => {
                                crate::PluginInteractionFailure::Configure { key, code: error }
                            }
                        };
                        self.interaction_failures
                            .insert(interaction.plugin, failure);
                    } else {
                        self.interaction_failures.remove(&interaction.plugin);
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => return,
            }
        }
    }

    fn dispatch_tick(&mut self) {
        let elapsed = self.started.elapsed();
        if elapsed.saturating_sub(self.last_tick) < TICK_INTERVAL {
            return;
        }
        self.last_tick = elapsed;
        let now = self.now_ms();
        self.broadcast(|| PluginOperation::Tick(now));
    }

    /// Publish the plugins' current views only when their composed input changed.
    fn compose(&mut self) {
        let viewport = self.hud.viewport.latest();
        if !self.hud_dirty
            && self
                .last_signature
                .as_ref()
                .is_some_and(|signature| signature.viewport == viewport)
        {
            if self.hud_last_used.elapsed() >= Duration::from_secs(30)
                && self.last_signature.as_ref().is_some_and(|signature| {
                    signature
                        .frames
                        .iter()
                        .all(|frame| frame.view.nodes.is_empty())
                })
            {
                self.compositor = None;
            }
            return;
        }
        self.hud_dirty = false;
        let frames = if viewport.is_usable() {
            self.running_frames()
        } else {
            Vec::new()
        };
        let has_content = frames.iter().any(|frame| !frame.view.nodes.is_empty());
        if has_content {
            self.hud_last_used = Instant::now();
            if self.compositor.is_none() && !self.hud_font_failed {
                match load_platform_font() {
                    Ok(font) => self.compositor = Some(HudCompositor::new(font)),
                    Err(failure) => {
                        self.hud_font_failed = true;
                        self.error = Some(failure.to_string());
                    }
                }
            }
        } else if self.hud_last_used.elapsed() >= Duration::from_secs(30) {
            self.compositor = None;
        }
        let signature = LayerSignature {
            viewport,
            frames: frames.clone(),
        };
        if self.last_signature.as_ref() == Some(&signature) {
            return;
        }
        let layer = if has_content && viewport.is_usable() {
            self.compositor
                .as_ref()
                .and_then(|compositor| compositor.compose(viewport, &frames).ok())
                .unwrap_or_else(HudLayer::empty)
        } else {
            HudLayer::empty()
        };
        self.last_signature = Some(signature);
        self.hud.hud_producer.publish(layer);
    }

    /// The panel each enabled plugin currently publishes.
    ///
    /// A plugin that is enabled but has published nothing yet is skipped rather
    /// than drawn as an empty panel: its first frame arrives on the tick that
    /// follows its start.
    fn running_frames(&self) -> Vec<PluginHudFrame> {
        self.hud_frames.values().cloned().collect()
    }
}

fn nodes<T: Clone>(
    output: Option<&PluginOutput>,
    select: impl FnOnce(&PluginOutput) -> &Vec<T>,
) -> Vec<T> {
    output.map(select).cloned().unwrap_or_else(Vec::new)
}

fn operation(error: &PluginManagerError) -> PluginCommandError {
    match error {
        PluginManagerError::Registry(RegistryError::Storage(_)) => {
            PluginCommandError::StorageFailure
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::StorageFull =>
        {
            PluginCommandError::StorageFailure
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::Unsupported =>
        {
            PluginCommandError::UnsupportedCapability
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::WouldBlock =>
        {
            PluginCommandError::Busy
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::TimedOut =>
        {
            PluginCommandError::Timeout
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::InvalidInput =>
        {
            PluginCommandError::InvalidConfiguration
        }
        PluginManagerError::Registry(RegistryError::Process(error))
            if error.kind() == std::io::ErrorKind::InvalidData =>
        {
            PluginCommandError::ProtocolViolation
        }
        PluginManagerError::Io(_) => PluginCommandError::StorageFailure,
        PluginManagerError::Registry(RegistryError::NotFound(_)) => PluginCommandError::NotFound,
        PluginManagerError::Registry(RegistryError::NotRunning(_)) => {
            PluginCommandError::NotRunning
        }
        _ => PluginCommandError::Operation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_service_has_no_periodic_deadline() {
        let store = tempfile::tempdir().unwrap();
        let worker = test_worker(&store);
        assert_eq!(worker.next_wait(), None);
    }

    #[test]
    fn projection_changes_invalidate_only_the_affected_display() {
        let store = tempfile::tempdir().unwrap();
        let mut worker = test_worker(&store);
        let id = "com.example.regions";
        let mut projection = bongocat_plugin_manager::PluginProjection {
            metadata: bongocat_plugin_sdk::PluginMetadata {
                id: id.into(),
                name: "Regions".into(),
                description: "fixture".into(),
                version: "1.0.0".into(),
                app_versions: ">=2.3.1".into(),
                protocol_version: 1,
                entry: "fixture".into(),
                icon: bongocat_plugin_sdk::PluginIcon::Emoji("X".into()),
                capabilities: vec![],
                localizations: Default::default(),
            },
            state: PluginState::Enabled,
            enabled: true,
            output: Arc::new(PluginOutput::default()),
        };
        worker.apply_projection(id, Some(projection.clone()));
        worker.hud_dirty = false;
        worker.companion_dirty = false;
        Arc::make_mut(&mut projection.output)
            .config
            .insert("enabled".into(), ConfigValue::Boolean(true));
        worker.apply_projection(id, Some(projection.clone()));
        assert!(!worker.hud_dirty && !worker.companion_dirty);
        Arc::make_mut(&mut projection.output)
            .menu
            .push(bongocat_plugin_sdk::PluginMenuItem {
                id: "open".into(),
                label: "Open".into(),
            });
        worker.apply_projection(id, Some(projection.clone()));
        assert!(!worker.hud_dirty && worker.companion_dirty);
        worker.companion_dirty = false;
        Arc::make_mut(&mut projection.output)
            .model
            .nodes
            .push(UiNode::Text {
                text: "Changed".into(),
            });
        worker.apply_projection(id, Some(projection));
        assert!(worker.hud_dirty && !worker.companion_dirty);
    }

    #[test]
    fn failed_enabled_plugin_retries_at_most_three_times_and_can_be_disabled() {
        use std::io::Write;
        let store = tempfile::tempdir().unwrap();
        let mut worker = test_worker(&store);
        let id = "com.example.recovery";
        let metadata = bongocat_plugin_sdk::PluginMetadata {
            id: id.into(),
            name: "Recovery".into(),
            description: "fixture".into(),
            version: "1.0.0".into(),
            app_versions: ">=2.3.1".into(),
            protocol_version: bongocat_plugin_sdk::PROTOCOL_VERSION,
            entry: "plugin".into(),
            icon: bongocat_plugin_sdk::PluginIcon::Emoji("R".into()),
            capabilities: Vec::new(),
            localizations: Default::default(),
        };
        let manifest = bongocat_plugin_manager::PluginManifest { metadata };
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("plugin.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        let executable = if cfg!(target_os = "windows") {
            "plugin.exe"
        } else {
            "plugin"
        };
        zip.start_file(executable, options).unwrap();
        zip.write_all(b"not an executable").unwrap();
        let package = zip.finish().unwrap().into_inner();
        let installed = worker.manager.install_local(&package).unwrap();
        // A missing executable fails deterministically without asking Windows to
        // start invalid bytes, which may open an interactive system error dialog.
        std::fs::remove_file(installed.root.join(executable)).unwrap();
        assert!(worker.set_enabled(id, true).is_err());
        assert!(
            worker
                .center()
                .cards
                .iter()
                .find(|card| card.id == id)
                .unwrap()
                .enabled
        );
        worker.recover_plugins();
        for attempt in 1..=3 {
            worker.recovery.get_mut(id).unwrap().next_attempt = Instant::now();
            worker.recover_plugins();
            settle(&mut worker);
            assert_eq!(worker.recovery[id].failures, attempt);
        }
        worker.recover_plugins();
        assert_eq!(worker.recovery[id].failures, 3);
        worker.queue_operation(
            id,
            PluginOperation::Enable(false),
            None,
            AfterOperation::Disabled,
        );
        worker.recovery.remove(id);
        settle(&mut worker);
        assert!(!worker.manager.plugin_is_enabled(id));
        assert!(!worker.recovery.contains_key(id));
    }

    fn settle(worker: &mut Worker) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !worker.pending_operations.is_empty() {
            assert!(Instant::now() < deadline, "plugin jobs must complete");
            worker.finish_actor_work();
            thread::park_timeout(Duration::from_millis(5));
        }
    }

    fn test_worker(store: &tempfile::TempDir) -> Worker {
        let (_, endpoint) = PluginServiceClient::bounded(4);
        let (hud_producer, _) = latest_hud_channel();
        let (_, viewport) = latest_viewport_channel();
        let (_, interactions) = bongocat_plugin_hud::waking_channel(4);
        Worker {
            hud_dirty: true,
            companion_dirty: true,
            actors: BTreeMap::new(),
            retiring: Vec::new(),
            hud_frames: BTreeMap::new(),
            pending_operations: BTreeMap::new(),
            next_operation: 0,
            locale: None,
            manager: PluginManager::new(store.path().to_path_buf()).unwrap(),
            source: PluginSource::InstalledOnly,
            offers: Vec::new(),
            catalog_job: None,
            install_jobs: BTreeMap::new(),
            icon_job: None,
            icon_prefetch_pending: false,
            catalog_stop: Arc::default(),
            input_activity: None,
            last_input_activity: InputActivity::default(),
            last_input_overflow: 0,
            input_events: None,
            endpoint,
            hud: OverlaySinks {
                hud_producer,
                viewport,
                interactions,
            },
            compositor: None,
            hud_last_used: Instant::now(),
            hud_font_failed: false,
            error: None,
            interaction_failures: Default::default(),
            last_signature: None,
            started: Instant::now(),
            last_tick: Duration::ZERO,
            shutdown_reply: None,
            companion: CompanionSinks {
                menu_items: Arc::new(RwLock::new(Vec::new())),
                bubbles: Arc::new(Mutex::new(Vec::new())),
                pet_bounds: Arc::new(RwLock::new(None)),
                pet_state: Arc::new(RwLock::new(None)),
            },
            last_pet_bounds: None,
            last_pet_state: None,
            seen_bubbles: BTreeMap::new(),
            recovery: BTreeMap::new(),
        }
    }

    #[test]
    fn companion_menu_uses_the_tree_when_published_and_keeps_the_flat_fallback() {
        let flat = PluginOutput {
            menu: vec![bongocat_plugin_sdk::PluginMenuItem {
                id: "open".into(),
                label: "Open".into(),
            }],
            ..Default::default()
        };
        let items = companion_menu("com.bongocat.test", &flat);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].action.as_deref(), Some("open"));
        assert!(items[0].children.is_empty());
        assert_eq!(items[0].checked, None);

        let tree = PluginOutput {
            menu_tree: vec![PluginMenuNode {
                id: "scale".into(),
                label: "Scale".into(),
                checked: None,
                children: vec![PluginMenuNode {
                    id: "scale-40".into(),
                    label: "40%".into(),
                    checked: Some(true),
                    children: Vec::new(),
                }],
            }],
            ..Default::default()
        };
        let items = companion_menu("com.bongocat.test", &tree);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].action, None);
        assert_eq!(items[0].children.len(), 1);
        assert_eq!(items[0].children[0].action.as_deref(), Some("scale-40"));
        assert_eq!(items[0].children[0].checked, Some(true));
    }

    #[test]
    fn package_downloads_only_block_lifecycle_operations_for_the_same_plugin() {
        let store = tempfile::tempdir().unwrap();
        let mut worker = test_worker(&store);
        let id = "com.bongocat.pomodoro";
        let data = worker.manager.data_root(id, true).unwrap();
        std::fs::write(data.join("owned-data"), b"keep").unwrap();
        let (release, wait) = sync_channel(1);
        assert!(worker.start_install_job(id.into(), move || {
            wait.recv().unwrap();
            Err("released".into())
        }));
        let uninstall = || PluginCommand::Uninstall {
            id: id.to_owned(),
            reply: PluginReply(async_channel::bounded(1).0),
        };
        let commands = [
            PluginCommand::SetEnabled {
                id: id.to_owned(),
                enabled: false,
                reply: PluginReply(async_channel::bounded(1).0),
            },
            uninstall(),
            PluginCommand::ResetData {
                id: id.to_owned(),
                reply: PluginReply(async_channel::bounded(1).0),
            },
        ];
        for command in commands {
            let (reply, response) = async_channel::bounded(1);
            // Rebuild the command around this iteration's reply channel.
            let command = match command {
                PluginCommand::SetEnabled { id, enabled, .. } => PluginCommand::SetEnabled {
                    id,
                    enabled,
                    reply: PluginReply(reply),
                },
                PluginCommand::Uninstall { id, .. } => PluginCommand::Uninstall {
                    id,
                    reply: PluginReply(reply),
                },
                PluginCommand::ResetData { id, .. } => PluginCommand::ResetData {
                    id,
                    reply: PluginReply(reply),
                },
                _ => unreachable!("only lifecycle commands are exercised"),
            };
            worker.apply(command);
            assert_eq!(
                response.recv_blocking().unwrap(),
                Err(PluginCommandError::Operation)
            );
        }
        assert_eq!(std::fs::read(data.join("owned-data")).unwrap(), b"keep");
        // An unrelated plugin's lifecycle is not blocked by this download.
        let (reply, response) = async_channel::bounded(1);
        worker.apply(PluginCommand::Uninstall {
            id: "com.bongocat.other".into(),
            reply: PluginReply(reply),
        });
        settle(&mut worker);
        assert!(response.recv_blocking().unwrap().is_ok());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.install_jobs.is_empty() && Instant::now() < deadline {
            worker.finish_install_jobs();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(worker.install_jobs.is_empty());
    }

    #[test]
    fn separate_plugins_can_download_packages_at_the_same_time() {
        let store = tempfile::tempdir().unwrap();
        let mut worker = test_worker(&store);
        let (release_first, wait_first) = sync_channel(1);
        let (release_second, wait_second) = sync_channel(1);
        assert!(
            worker.start_install_job("com.example.first".into(), move || {
                wait_first.recv().unwrap();
                Err("released".into())
            })
        );
        assert!(
            worker.start_install_job("com.example.second".into(), move || {
                wait_second.recv().unwrap();
                Err("released".into())
            })
        );

        assert_eq!(worker.install_jobs.len(), 2);
        assert!(!worker.center().loading);
        assert!(worker.install_jobs.contains_key("com.example.first"));
        assert!(worker.install_jobs.contains_key("com.example.second"));

        release_first.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.install_jobs.contains_key("com.example.second")
            || worker.install_jobs.contains_key("com.example.first")
        {
            assert!(
                Instant::now() < deadline,
                "first package job must finish alone"
            );
            worker.finish_install_jobs();
            thread::sleep(Duration::from_millis(5));
        }
        release_second.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.install_jobs.is_empty() && Instant::now() < deadline {
            worker.finish_install_jobs();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(worker.install_jobs.is_empty());
    }
}
