//! Small shared host bridge for the companion plugins.
//!
//! Each plugin owns its business state and its Tauri windows. This crate owns
//! only the framed stdin/stdout conversation, the data directory contract and
//! the event names the Vue views listen to. It deliberately contains no todo or
//! mail rules.

#![forbid(unsafe_code)]

use bongocat_plugin_sdk::{
    Configuration, HostMessage, InputActivity, PROTOCOL_VERSION, PetBounds, PetState, PluginBubble,
    PluginOutput, Request, Response, read_frame, write_frame,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, sync_channel},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

pub const EVENT_NAME: &str = "companion://event";

/// Events the protocol thread sends to the Tauri windows.
///
/// They are UI hints only. The authoritative state remains in the plugin's Rust
/// state and in the host protocol output.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Open { kind: String },
    Locale { locale: String },
    PetBounds { bounds: PetBounds },
    Bubble { bubble: PluginBubble },
    Notify { title: String, body: String },
    StateChanged,
    Stop,
}

/// The thread-safe bridge from the protocol thread to the Tauri event loop.
#[derive(Clone, Default)]
pub struct UiBridge {
    app: Arc<Mutex<Option<AppHandle>>>,
    pending: Arc<Mutex<Vec<UiEvent>>>,
    stop: Arc<AtomicBool>,
}

impl UiBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the Tauri handle and replay everything that arrived before it.
    ///
    /// Returns true when a Stop arrived while the WebView was still starting;
    /// the caller should exit instead of opening a window.
    pub fn attach(&self, app: AppHandle) -> bool {
        let pending = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *pending)
        };
        *self
            .app
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(app.clone());
        let mut stop = false;
        for event in pending {
            stop |= matches!(event, UiEvent::Stop);
            let _ = app.emit(EVENT_NAME, event);
        }
        stop
    }

    pub fn emit(&self, event: UiEvent) {
        if matches!(event, UiEvent::Stop) {
            self.stop.store(true, Ordering::Release);
        }
        let app = self
            .app
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(app) = app {
            let _ = app.emit(EVENT_NAME, event);
        } else {
            self.pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(event);
        }
    }

    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// The Tauri handle once the app has attached it.
    ///
    /// A companion that owns windows and needs to present them from the
    /// protocol thread uses this to schedule the actual window call on the
    /// platform UI thread with [`AppHandle::run_on_main_thread`].
    pub fn app_handle(&self) -> Option<AppHandle> {
        self.app
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The business surface one companion plugin exposes to the host protocol.
pub trait CompanionState: Send + 'static {
    fn id(&self) -> &'static str;
    fn start(&mut self, config: Configuration, state: Vec<u8>) -> Result<PluginOutput, String>;
    fn tick(&mut self, elapsed_ms: u64) -> Result<PluginOutput, String>;
    fn action(&mut self, action: &str) -> Result<PluginOutput, String>;
    fn configure(&mut self, config: Configuration) -> Result<PluginOutput, String>;
    fn locale(&mut self, locale: &str) -> Result<PluginOutput, String>;
    fn pet_bounds(&mut self, bounds: PetBounds) -> Result<PluginOutput, String>;
    fn reset(&mut self) -> Result<PluginOutput, String>;
    fn stop(&mut self) -> Result<PluginOutput, String>;

    /// Anonymous device totals changed.
    ///
    /// The counters are absolute host totals; a plugin that cares compares them
    /// with the values it saw last. The default keeps every existing companion
    /// working without declaring `input_activity_v1`.
    fn input_activity(
        &mut self,
        _elapsed_ms: u64,
        _activity: InputActivity,
    ) -> Result<PluginOutput, String> {
        self.output()
    }

    /// The active model or the model window's visibility changed.
    ///
    /// Only plugins that declare `pet_state_v1` receive this message; the
    /// default keeps the trait usable for companions that do not need it.
    fn pet_state(&mut self, _state: PetState) -> Result<PluginOutput, String> {
        self.output()
    }

    fn pause(&mut self) -> Result<PluginOutput, String> {
        self.output()
    }
    fn resume(&mut self) -> Result<PluginOutput, String> {
        self.output()
    }
    fn output(&self) -> Result<PluginOutput, String>;
}

/// Start the framed host conversation and return immediately.
///
/// The caller runs the Tauri event loop on the main thread. Protocol frames are
/// answered by a separate thread, so a slow WebView start or a window animation
/// can never consume the host's 750 ms request deadline.
pub fn run_host<S: CompanionState>(state: Arc<Mutex<S>>, ui: UiBridge) {
    let state_for_reader = Arc::clone(&state);
    let ui_for_reader = ui.clone();
    thread::Builder::new()
        .name("bongocat-companion-host".to_owned())
        .spawn(move || {
            let _ = host_loop(state_for_reader, ui_for_reader);
        })
        .expect("companion host thread");
}

fn host_loop<S: CompanionState>(state: Arc<Mutex<S>>, ui: UiBridge) -> io::Result<()> {
    let (sender, responses) = sync_channel::<Response>(8);
    let writer = thread::Builder::new()
        .name("bongocat-companion-writer".to_owned())
        .spawn(move || writer_loop(responses))?;
    let mut input = io::stdin().lock();
    let mut ready = false;
    loop {
        let request: Request = match read_frame(&mut input) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                ui.emit(UiEvent::Stop);
                break;
            }
            Err(error) => return Err(error),
        };
        let stop = matches!(request.message, HostMessage::Stop);
        let result = handle_message(&state, &ui, &mut ready, request.message);
        if sender
            .send(Response {
                sequence: request.sequence,
                result,
            })
            .is_err()
        {
            break;
        }
        if stop {
            break;
        }
    }
    drop(sender);
    let _ = writer.join();
    Ok(())
}

fn handle_message<S: CompanionState>(
    state: &Arc<Mutex<S>>,
    ui: &UiBridge,
    ready: &mut bool,
    message: HostMessage,
) -> Result<PluginOutput, String> {
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match message {
        HostMessage::Handshake {
            protocol_version,
            id,
        } => {
            if *ready || protocol_version != PROTOCOL_VERSION || id != state.id() {
                Err("incompatible plugin handshake".into())
            } else {
                *ready = true;
                Ok(PluginOutput::default())
            }
        }
        message if *ready => match message {
            HostMessage::Start {
                config,
                state: bytes,
            } => state.start(config, bytes),
            HostMessage::Tick { elapsed_ms } => state.tick(elapsed_ms),
            HostMessage::Action { id } => state.action(&id),
            HostMessage::Preview { .. } => Err("preview is not supported".into()),
            HostMessage::Configure { config } => state.configure(config),
            HostMessage::Locale { locale } => {
                ui.emit(UiEvent::Locale {
                    locale: locale.clone(),
                });
                state.locale(&locale)
            }
            HostMessage::PetBounds { bounds } => {
                ui.emit(UiEvent::PetBounds { bounds });
                state.pet_bounds(bounds)
            }
            HostMessage::Reset => state.reset(),
            HostMessage::Pause => state.pause(),
            HostMessage::Resume => state.resume(),
            HostMessage::Stop => {
                let output = state.stop();
                ui.emit(UiEvent::Stop);
                output
            }
            HostMessage::Handshake { .. } => Err("incompatible plugin handshake".into()),
            HostMessage::InputActivity {
                elapsed_ms,
                activity,
            } => state.input_activity(elapsed_ms, activity),
            HostMessage::PetState { state: pet } => state.pet_state(pet),
            HostMessage::InputEvents { .. } => state.output(),
        },
        _ => Err("handshake required".into()),
    }
}

fn writer_loop(responses: Receiver<Response>) -> io::Result<()> {
    let mut output = io::stdout().lock();
    while let Ok(response) = responses.recv() {
        write_frame(&mut output, &response)?;
    }
    Ok(())
}

/// The host-owned data directory, with a stable fallback for manual runs.
pub fn data_dir(id: &str) -> PathBuf {
    if let Some(path) = std::env::var_os("BONGOCAT_PLUGIN_DATA_DIR") {
        return PathBuf::from(path);
    }
    std::env::temp_dir().join(format!("bongocat-plugin-{id}"))
}

pub fn ensure_data_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Read one JSON document, returning `None` when it does not exist.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Write one JSON document atomically beside its destination.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = atomic_write_file::AtomicWriteFile::options().open(path)?;
    serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
    use std::io::Write as _;
    file.flush()?;
    file.commit()
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

pub fn next_id(prefix: &str) -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{counter}", now_ms())
}
