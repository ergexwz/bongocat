//! The Cyber Work companion: counters, persistence, menu and window placement.
//!
//! The plugin owns its data and its two Tauri windows. The host supplies the
//! anonymous input counters, the pet rectangle and the active model/visibility;
//! nothing here can observe a key identity, text or pointer position.

use bongocat_companion::{CompanionState, UiBridge, UiEvent, now_ms, read_json, write_json};
use bongocat_plugin_sdk::{
    Configuration, InputActivity, PetBounds, PetState, PluginMenuNode, PluginOutput,
};
use chrono::{Datelike, Days, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, State};

const PLUGIN_ID: &str = "com.bongocat.cyberwork";
const DATA_FILE: &str = "cyberwork.json";
const SCHEMA_VERSION: u32 = 1;
const DEFAULT_SCALE_PERCENT: u16 = 100;
const ALLOWED_SCALE_PERCENTS: [u16; 8] = [20, 40, 60, 80, 100, 120, 140, 160];
const RETENTION_DAYS: i64 = 400;
const SAVE_INTERVAL: Duration = Duration::from_secs(2);
/// A tick gap longer than this is a suspend or a stalled host, not play time.
const MAX_COUNTED_TICK_GAP_MS: u64 = 5_000;

const COUNTER_LABEL: &str = "counter";
const HISTORY_LABEL: &str = "history";
const COUNTER_WIDTH: f64 = 340.0;
const COUNTER_HEIGHT: f64 = 98.0;
const COUNTER_GAP: f64 = 4.0;
const HISTORY_WIDTH: f64 = 500.0;
const HISTORY_HEIGHT: f64 = 580.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DailyRecord {
    #[serde(default)]
    taps: u64,
    #[serde(default)]
    game_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CyberWorkFile {
    schema_version: u32,
    scale_percent: u16,
    taps: u64,
    game_ms: u64,
    #[serde(default)]
    daily: BTreeMap<String, DailyRecord>,
}

impl Default for CyberWorkFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            scale_percent: DEFAULT_SCALE_PERCENT,
            taps: 0,
            game_ms: 0,
            daily: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterMode {
    Hidden,
    Taps,
    Game,
}

impl CounterMode {
    fn name(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Taps => "standard",
            Self::Game => "gamepad",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodValues {
    pub day: u64,
    pub week: u64,
    pub month: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    pub taps: PeriodValues,
    pub game_seconds: PeriodValues,
    pub day_start: String,
    pub week_start: String,
    pub month_start: String,
    pub scale_percent: u16,
    pub locale: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterRender {
    pub mode: String,
    pub text: String,
    pub scale_percent: u16,
}

pub struct CyberWorkState {
    data_file: PathBuf,
    scale_percent: u16,
    taps: u64,
    game_ms: u64,
    daily: BTreeMap<String, DailyRecord>,
    locale: String,
    pet_bounds: Option<PetBounds>,
    pet_state: Option<PetState>,
    last_activity: Option<InputActivity>,
    last_tick_ms: Option<u64>,
    last_prune_day: Option<String>,
    dirty: bool,
    last_save: Instant,
    persist_allowed: bool,
    error: Option<String>,
    pending_history: bool,
    ui: UiBridge,
    counter_shown: bool,
    counter_geometry: Option<WindowGeometry>,
    last_counter_text: String,
    last_counter_scale: u16,
}

impl CyberWorkState {
    pub fn load(directory: &Path, ui: UiBridge) -> Arc<Mutex<Self>> {
        let data_file = directory.join(DATA_FILE);
        let today = today();
        let mut persist_allowed = true;
        let mut dirty = false;
        let (mut file, error) = match read_json::<CyberWorkFile>(&data_file) {
            Ok(Some(file)) if file.schema_version == SCHEMA_VERSION => {
                dirty = true;
                (file, None)
            }
            Ok(Some(_)) => {
                if backup_corrupt(&data_file) {
                    (
                        CyberWorkFile::default(),
                        Some("data_schema_unsupported".to_owned()),
                    )
                } else {
                    persist_allowed = false;
                    (
                        CyberWorkFile::default(),
                        Some("data_backup_failed".to_owned()),
                    )
                }
            }
            Ok(None) => (CyberWorkFile::default(), None),
            Err(_) => {
                if backup_corrupt(&data_file) {
                    (
                        CyberWorkFile::default(),
                        Some("data_could_not_be_read".to_owned()),
                    )
                } else {
                    persist_allowed = false;
                    (
                        CyberWorkFile::default(),
                        Some("data_backup_failed".to_owned()),
                    )
                }
            }
        };

        file.scale_percent = normalize_scale(file.scale_percent);
        file.daily = sanitize_daily(file.daily, today);
        let daily_taps = file.daily.values().map(|record| record.taps).sum();
        let daily_game = file.daily.values().map(|record| record.game_ms).sum();
        file.taps = file.taps.max(daily_taps);
        file.game_ms = file.game_ms.max(daily_game);

        Arc::new(Mutex::new(Self {
            data_file,
            scale_percent: file.scale_percent,
            taps: file.taps,
            game_ms: file.game_ms,
            daily: file.daily,
            locale: "en-US".into(),
            pet_bounds: None,
            pet_state: None,
            last_activity: None,
            last_tick_ms: None,
            last_prune_day: Some(date_key(today)),
            dirty,
            last_save: Instant::now(),
            persist_allowed,
            error,
            pending_history: false,
            ui,
            counter_shown: false,
            counter_geometry: None,
            last_counter_text: String::new(),
            last_counter_scale: DEFAULT_SCALE_PERCENT,
        }))
    }

    fn mode(&self) -> CounterMode {
        match self
            .pet_state
            .as_ref()
            .and_then(|state| state.model.as_ref())
        {
            Some(model) if model.is_standard() => CounterMode::Taps,
            Some(model) if model.is_gamepad() => CounterMode::Game,
            _ => CounterMode::Hidden,
        }
    }

    fn counter_text(&self) -> String {
        match self.mode() {
            CounterMode::Hidden => String::new(),
            CounterMode::Taps => self.taps.to_string(),
            CounterMode::Game => format_game_ms(self.game_ms),
        }
    }

    fn add_taps(&mut self, count: u64) {
        if count == 0 {
            return;
        }
        self.taps = self.taps.saturating_add(count);
        let key = date_key(today());
        let record = self.daily.entry(key).or_default();
        record.taps = record.taps.saturating_add(count);
        self.dirty = true;
    }

    fn add_game_ms(&mut self, elapsed_ms: u64) {
        if elapsed_ms == 0 {
            return;
        }
        self.game_ms = self.game_ms.saturating_add(elapsed_ms);
        let key = date_key(today());
        let record = self.daily.entry(key).or_default();
        record.game_ms = record.game_ms.saturating_add(elapsed_ms);
        self.dirty = true;
    }

    fn persist(&mut self) -> Result<(), String> {
        if !self.persist_allowed {
            return Err("data_backup_failed".into());
        }
        let file = CyberWorkFile {
            schema_version: SCHEMA_VERSION,
            scale_percent: self.scale_percent,
            taps: self.taps,
            game_ms: self.game_ms,
            daily: self.daily.clone(),
        };
        match write_json(&self.data_file, &file) {
            Ok(()) => {
                self.dirty = false;
                self.last_save = Instant::now();
                self.error = None;
                Ok(())
            }
            Err(_) => {
                self.error = Some("data_could_not_be_saved".into());
                Err("data_could_not_be_saved".into())
            }
        }
    }

    fn persist_if_due(&mut self) {
        if self.dirty && self.last_save.elapsed() >= SAVE_INTERVAL {
            let _ = self.persist();
        }
    }

    fn prune_for_today(&mut self) {
        let today = today();
        let key = date_key(today);
        if self.last_prune_day.as_deref() == Some(&key) {
            return;
        }
        self.last_prune_day = Some(key);
        let before = self.daily.len();
        self.daily = sanitize_daily(std::mem::take(&mut self.daily), today);
        if self.daily.len() != before {
            self.dirty = true;
        }
    }

    fn aggregate_periods(&self, today: NaiveDate) -> (PeriodValues, PeriodValues) {
        let (day_start, week_start, month_start) = period_starts(today);
        let today_start = date_key(day_start);
        let week_start = date_key(week_start);
        let month_start = date_key(month_start);
        let mut day_taps = 0u64;
        let mut week_taps = 0u64;
        let mut month_taps = 0u64;
        let mut day_game = 0u64;
        let mut week_game = 0u64;
        let mut month_game = 0u64;
        for (key, record) in &self.daily {
            let key = key.as_str();
            if key > today_start.as_str() {
                continue;
            }
            if key == today_start.as_str() {
                day_taps = day_taps.saturating_add(record.taps);
                day_game = day_game.saturating_add(record.game_ms);
            }
            if key >= week_start.as_str() {
                week_taps = week_taps.saturating_add(record.taps);
                week_game = week_game.saturating_add(record.game_ms);
            }
            if key >= month_start.as_str() {
                month_taps = month_taps.saturating_add(record.taps);
                month_game = month_game.saturating_add(record.game_ms);
            }
        }
        (
            PeriodValues {
                day: day_taps,
                week: week_taps,
                month: month_taps,
                total: self.taps,
            },
            PeriodValues {
                day: day_game / 1000,
                week: week_game / 1000,
                month: month_game / 1000,
                total: self.game_ms / 1000,
            },
        )
    }

    fn history_snapshot(&self) -> HistorySnapshot {
        let today = today();
        let (_, week, month) = period_starts(today);
        let (taps, game_seconds) = self.aggregate_periods(today);
        HistorySnapshot {
            taps,
            game_seconds,
            day_start: date_key(today),
            week_start: date_key(week),
            month_start: date_key(month),
            scale_percent: self.scale_percent,
            locale: self.locale.clone(),
            error: self.error.clone(),
        }
    }

    fn cursor_over_pet(&self) -> bool {
        let Some(bounds) = self.pet_bounds else {
            return false;
        };
        let Some(app) = self.ui.app_handle() else {
            return false;
        };
        let Ok(cursor) = app.cursor_position() else {
            return false;
        };
        cursor.x >= f64::from(bounds.x)
            && cursor.x < f64::from(bounds.x) + f64::from(bounds.width)
            && cursor.y >= f64::from(bounds.y)
            && cursor.y < f64::from(bounds.y) + f64::from(bounds.height)
    }

    fn invalidate_counter_geometry(&mut self) {
        self.counter_geometry = None;
    }

    fn present_counter(&mut self) {
        let history_visible = self
            .ui
            .app_handle()
            .and_then(|app| app.get_webview_window(HISTORY_LABEL))
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        let base_show = !history_visible
            && self.pet_state.as_ref().is_some_and(|state| state.visible)
            && self.mode() != CounterMode::Hidden
            && self.cursor_over_pet();
        let geometry = if base_show {
            self.counter_geometry.or_else(|| self.counter_geometry())
        } else {
            None
        };
        let show = base_show && geometry.is_some();
        if show == self.counter_shown && geometry == self.counter_geometry {
            return;
        }
        self.apply_counter(show, geometry);
    }

    fn apply_counter(&mut self, show: bool, geometry: Option<WindowGeometry>) {
        let Some(app) = self.ui.app_handle() else {
            self.counter_shown = false;
            self.counter_geometry = None;
            return;
        };
        let ui = self.ui.clone();
        let main_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(window) = main_app.get_webview_window(COUNTER_LABEL) else {
                return;
            };
            if let Some(geometry) = geometry {
                let _ = window.set_size(PhysicalSize::new(geometry.width, geometry.height));
                let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
            }
            if show {
                let _ = window.show();
                ui.emit(UiEvent::StateChanged);
            } else {
                let _ = window.hide();
            }
        });
        self.counter_shown = show;
        self.counter_geometry = if show { geometry } else { None };
        if show {
            self.last_counter_text = self.counter_text();
            self.last_counter_scale = self.scale_percent;
        }
    }

    fn emit_counter_if_changed(&mut self) {
        if !self.counter_shown {
            return;
        }
        let text = self.counter_text();
        if text != self.last_counter_text || self.scale_percent != self.last_counter_scale {
            self.ui.emit(UiEvent::StateChanged);
            self.last_counter_text = text;
            self.last_counter_scale = self.scale_percent;
        }
    }

    fn counter_geometry(&self) -> Option<WindowGeometry> {
        let bounds = self.pet_bounds?;
        let app = self.ui.app_handle()?;
        let monitor = monitor_for(&app, Some(bounds));
        let factor = monitor
            .as_ref()
            .map_or(f64::from(bounds.scale), tauri::Monitor::scale_factor)
            .max(0.5);
        let scale = f64::from(self.scale_percent) / 100.0;
        let width = (COUNTER_WIDTH * scale * factor).round().max(1.0) as u32;
        let height = (COUNTER_HEIGHT * scale * factor).round().max(1.0) as u32;
        let gap = COUNTER_GAP * factor;
        let mut x = f64::from(bounds.x) + f64::from(bounds.width) / 2.0 - f64::from(width) / 2.0;
        let mut y = f64::from(bounds.y) - f64::from(height) - gap;
        if let Some(area) = monitor.as_ref().map(tauri::Monitor::work_area) {
            let left = f64::from(area.position.x);
            let top = f64::from(area.position.y);
            let right = left + f64::from(area.size.width);
            let bottom = top + f64::from(area.size.height);
            if y < top {
                y = f64::from(bounds.y) + f64::from(bounds.height) + gap;
            }
            x = x.clamp(left, (right - f64::from(width)).max(left));
            y = y.clamp(top, (bottom - f64::from(height)).max(top));
        }
        Some(WindowGeometry {
            x: x.round() as i32,
            y: y.round() as i32,
            width,
            height,
        })
    }

    fn history_geometry(&self) -> Option<WindowGeometry> {
        let app = self.ui.app_handle()?;
        let monitor = monitor_for(&app, self.pet_bounds);
        let factor = monitor
            .as_ref()
            .map_or_else(
                || {
                    self.pet_bounds
                        .map_or(1.0, |bounds| f64::from(bounds.scale))
                },
                tauri::Monitor::scale_factor,
            )
            .max(0.5);
        let mut width = (HISTORY_WIDTH * factor).round().max(1.0) as u32;
        let mut height = (HISTORY_HEIGHT * factor).round().max(1.0) as u32;
        let mut x = 0.0;
        let mut y = 0.0;
        if let Some(area) = monitor.as_ref().map(tauri::Monitor::work_area) {
            width = width.min(area.size.width.max(1));
            height = height.min(area.size.height.max(1));
            let left = f64::from(area.position.x);
            let top = f64::from(area.position.y);
            x = left + (f64::from(area.size.width) - f64::from(width)) / 2.0;
            y = top + (f64::from(area.size.height) - f64::from(height)) / 2.0;
            x = x.max(left);
            y = y.max(top);
        }
        Some(WindowGeometry {
            x: x.round() as i32,
            y: y.round() as i32,
            width,
            height,
        })
    }

    fn open_history(&mut self) {
        self.pending_history = true;
        let Some(app) = self.ui.app_handle() else {
            return;
        };
        let geometry = self.history_geometry();
        let ui = self.ui.clone();
        let main_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = main_app.get_webview_window(HISTORY_LABEL) {
                if let Some(geometry) = geometry {
                    let _ = window.set_size(PhysicalSize::new(geometry.width, geometry.height));
                    let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
                }
                let _ = window.show();
                let _ = window.set_focus();
            }
            ui.emit(UiEvent::Open {
                kind: "history".into(),
            });
        });
        // The pointer is still over the pet while the native menu closes; the
        // next tick would otherwise show the counter behind the history panel.
        self.apply_counter(false, None);
    }

    fn set_scale(&mut self, value: u16) -> bool {
        if !ALLOWED_SCALE_PERCENTS.contains(&value) {
            return false;
        }
        if self.scale_percent == value {
            return true;
        }
        self.scale_percent = value;
        self.dirty = true;
        self.invalidate_counter_geometry();
        self.present_counter();
        self.emit_counter_if_changed();
        let _ = self.persist();
        true
    }

    fn menu(&self) -> Vec<PluginMenuNode> {
        let zh = self.locale == "zh-CN";
        let scale_label = if zh {
            "计数器缩放"
        } else {
            "Counter scale"
        };
        let history_label = if zh { "使用历史" } else { "Usage history" };
        vec![
            PluginMenuNode {
                id: "scale".into(),
                label: scale_label.into(),
                checked: None,
                children: ALLOWED_SCALE_PERCENTS
                    .iter()
                    .map(|value| PluginMenuNode {
                        id: format!("scale-{value}"),
                        label: format!("{value}%"),
                        checked: Some(*value == self.scale_percent),
                        children: Vec::new(),
                    })
                    .collect(),
            },
            PluginMenuNode {
                id: "open-history".into(),
                label: history_label.into(),
                checked: None,
                children: Vec::new(),
            },
        ]
    }

    fn plugin_output(&self) -> PluginOutput {
        PluginOutput {
            menu_tree: self.menu(),
            ..Default::default()
        }
    }

    pub fn render(&self) -> CounterRender {
        CounterRender {
            mode: self.mode().name().into(),
            text: self.counter_text(),
            scale_percent: self.scale_percent,
        }
    }

    pub fn history(&self) -> HistorySnapshot {
        self.history_snapshot()
    }

    fn take_pending_history(&mut self) -> bool {
        std::mem::take(&mut self.pending_history)
    }

    fn locale(&self) -> String {
        self.locale.clone()
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn date_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn period_starts(today: NaiveDate) -> (NaiveDate, NaiveDate, NaiveDate) {
    let week = today
        .checked_sub_days(Days::new(today.weekday().num_days_from_monday().into()))
        .unwrap_or(today);
    let month = today.with_day(1).unwrap_or(today);
    (today, week, month)
}

fn normalize_scale(value: u16) -> u16 {
    if ALLOWED_SCALE_PERCENTS.contains(&value) {
        value
    } else {
        DEFAULT_SCALE_PERCENT
    }
}

fn sanitize_daily(
    daily: BTreeMap<String, DailyRecord>,
    today: NaiveDate,
) -> BTreeMap<String, DailyRecord> {
    let cutoff = today
        .checked_sub_days(Days::new(RETENTION_DAYS as u64 - 1))
        .unwrap_or(today);
    let today_key = date_key(today);
    daily
        .into_iter()
        .filter(|(key, record)| {
            (record.taps > 0 || record.game_ms > 0)
                && NaiveDate::parse_from_str(key, "%Y-%m-%d")
                    .is_ok_and(|date| date >= cutoff && date <= today)
                && key.as_str() <= today_key.as_str()
        })
        .collect()
}

fn backup_corrupt(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let target = path.with_file_name(format!("{name}.corrupt-{}", now_ms()));
    std::fs::rename(path, target).is_ok()
}

fn format_game_ms(game_ms: u64) -> String {
    let total_seconds = game_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn monitor_for(app: &AppHandle, bounds: Option<PetBounds>) -> Option<tauri::Monitor> {
    let monitors = app.available_monitors().ok()?;
    if let Some(bounds) = bounds {
        let center_x = f64::from(bounds.x) + f64::from(bounds.width) / 2.0;
        let center_y = f64::from(bounds.y) + f64::from(bounds.height) / 2.0;
        if let Some(monitor) = monitors.iter().find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            center_x >= f64::from(position.x)
                && center_x < f64::from(position.x) + f64::from(size.width)
                && center_y >= f64::from(position.y)
                && center_y < f64::from(position.y) + f64::from(size.height)
        }) {
            return Some(monitor.clone());
        }
    }
    if let Ok(Some(primary)) = app.primary_monitor() {
        return Some(primary);
    }
    monitors.into_iter().next()
}

impl CompanionState for CyberWorkState {
    fn id(&self) -> &'static str {
        PLUGIN_ID
    }

    fn start(&mut self, _config: Configuration, _state: Vec<u8>) -> Result<PluginOutput, String> {
        Ok(self.plugin_output())
    }

    fn tick(&mut self, elapsed_ms: u64) -> Result<PluginOutput, String> {
        // The host sends its absolute uptime in this field, not a delta. A
        // first tick, a decreasing value and a suspension-sized gap are all
        // baselines rather than play time.
        let delta = match self.last_tick_ms {
            Some(last) if elapsed_ms >= last => elapsed_ms - last,
            _ => 0,
        };
        self.last_tick_ms = Some(elapsed_ms);
        if delta > 0 && delta <= MAX_COUNTED_TICK_GAP_MS && self.mode() == CounterMode::Game {
            self.add_game_ms(delta);
        }
        self.prune_for_today();
        self.present_counter();
        self.emit_counter_if_changed();
        self.persist_if_due();
        Ok(self.plugin_output())
    }

    fn action(&mut self, action: &str) -> Result<PluginOutput, String> {
        if action == "open-history" {
            self.open_history();
        } else if let Some(value) = action
            .strip_prefix("scale-")
            .and_then(|value| value.parse::<u16>().ok())
        {
            self.set_scale(value);
        }
        Ok(self.plugin_output())
    }

    fn configure(&mut self, config: Configuration) -> Result<PluginOutput, String> {
        if !config.is_empty() {
            return Err("cyberwork does not accept configuration".into());
        }
        Ok(self.plugin_output())
    }

    fn locale(&mut self, locale: &str) -> Result<PluginOutput, String> {
        self.locale = if locale == "zh-CN" {
            "zh-CN".into()
        } else {
            "en-US".into()
        };
        self.ui.emit(UiEvent::Locale {
            locale: self.locale.clone(),
        });
        Ok(self.plugin_output())
    }

    fn pet_bounds(&mut self, bounds: PetBounds) -> Result<PluginOutput, String> {
        self.pet_bounds = Some(bounds);
        self.invalidate_counter_geometry();
        self.present_counter();
        Ok(self.plugin_output())
    }

    fn pet_state(&mut self, state: PetState) -> Result<PluginOutput, String> {
        self.pet_state = Some(state);
        self.invalidate_counter_geometry();
        self.present_counter();
        self.emit_counter_if_changed();
        Ok(self.plugin_output())
    }

    fn input_activity(
        &mut self,
        _elapsed_ms: u64,
        activity: InputActivity,
    ) -> Result<PluginOutput, String> {
        if let Some(previous) = self.last_activity {
            let keys = activity
                .keyboard_down
                .saturating_sub(previous.keyboard_down);
            let mouse = activity.mouse_down.saturating_sub(previous.mouse_down);
            if self.mode() == CounterMode::Taps && keys + mouse > 0 {
                self.add_taps(keys + mouse);
                self.emit_counter_if_changed();
            }
        }
        self.last_activity = Some(activity);
        Ok(self.plugin_output())
    }

    fn reset(&mut self) -> Result<PluginOutput, String> {
        self.taps = 0;
        self.game_ms = 0;
        self.daily.clear();
        self.scale_percent = DEFAULT_SCALE_PERCENT;
        self.last_activity = None;
        self.last_tick_ms = None;
        self.dirty = true;
        self.error = None;
        self.invalidate_counter_geometry();
        self.apply_counter(false, None);
        let _ = self.persist();
        Ok(self.plugin_output())
    }

    fn stop(&mut self) -> Result<PluginOutput, String> {
        let _ = self.persist();
        Ok(self.plugin_output())
    }

    fn output(&self) -> Result<PluginOutput, String> {
        Ok(self.plugin_output())
    }
}

pub struct AppState {
    pub state: Arc<Mutex<CyberWorkState>>,
}

#[tauri::command]
pub fn cyberwork_render(state: State<'_, AppState>) -> Result<CounterRender, String> {
    let state = state
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(state.render())
}

#[tauri::command]
pub fn cyberwork_history(state: State<'_, AppState>) -> Result<HistorySnapshot, String> {
    let state = state
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(state.history())
}

#[tauri::command]
pub fn take_pending_open(state: State<'_, AppState>) -> bool {
    let mut state = state
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.take_pending_history()
}

#[tauri::command]
pub fn get_locale(state: State<'_, AppState>) -> String {
    let state = state
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.locale()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(store: &tempfile::TempDir) -> Arc<Mutex<CyberWorkState>> {
        CyberWorkState::load(store.path(), UiBridge::new())
    }

    fn standard_state() -> PetState {
        PetState {
            model: Some(bongocat_plugin_sdk::ModelInfo {
                id: "standard".into(),
                built_in: true,
            }),
            visible: true,
        }
    }

    fn gamepad_state() -> PetState {
        PetState {
            model: Some(bongocat_plugin_sdk::ModelInfo {
                id: "gamepad".into(),
                built_in: true,
            }),
            visible: true,
        }
    }

    fn activity(keyboard: u64, mouse: u64) -> InputActivity {
        InputActivity {
            keyboard_down: keyboard,
            mouse_down: mouse,
            ..Default::default()
        }
    }

    #[test]
    fn first_activity_is_a_baseline_and_standard_mode_counts_deltas() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        let mut state = state.lock().unwrap();
        state.pet_state(standard_state()).unwrap();
        state.input_activity(0, activity(10, 2)).unwrap();
        assert_eq!(state.taps, 0);
        state.input_activity(100, activity(15, 4)).unwrap();
        assert_eq!(state.taps, 7);
        state.input_activity(200, activity(15, 4)).unwrap();
        assert_eq!(state.taps, 7);
    }

    #[test]
    fn only_standard_mode_counts_taps() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        let mut state = state.lock().unwrap();
        state.pet_state(gamepad_state()).unwrap();
        state.input_activity(0, activity(10, 2)).unwrap();
        state.input_activity(100, activity(20, 5)).unwrap();
        assert_eq!(state.taps, 0);
        state.pet_state(standard_state()).unwrap();
        state.input_activity(200, activity(21, 5)).unwrap();
        assert_eq!(state.taps, 1);
    }

    #[test]
    fn game_time_uses_absolute_tick_deltas_and_ignores_suspension() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        let mut state = state.lock().unwrap();
        state.pet_state(gamepad_state()).unwrap();
        state.tick(1_000).unwrap();
        state.tick(3_000).unwrap();
        assert_eq!(state.game_ms, 2_000);
        state.tick(9_500).unwrap();
        assert_eq!(state.game_ms, 2_000);
        state.tick(9_700).unwrap();
        assert_eq!(state.game_ms, 2_200);
    }

    #[test]
    fn periods_start_at_midnight_monday_and_the_first() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        let mut state = state.lock().unwrap();
        for (key, taps) in [
            ("2026-10-30", 1u64),
            ("2026-10-29", 2),
            ("2026-10-26", 4),
            ("2026-10-25", 8),
            ("2026-10-01", 16),
            ("2026-09-30", 32),
        ] {
            state.daily.insert(
                key.into(),
                DailyRecord {
                    taps,
                    game_ms: taps * 1000,
                },
            );
        }
        state.taps = 63;
        state.game_ms = 63_000;
        let (taps, game) = state.aggregate_periods(NaiveDate::from_ymd_opt(2026, 10, 30).unwrap());
        assert_eq!(taps.day, 1);
        assert_eq!(taps.week, 7);
        assert_eq!(taps.month, 31);
        assert_eq!(taps.total, 63);
        assert_eq!(game.day, 1);
        assert_eq!(game.week, 7);
        assert_eq!(game.month, 31);
        assert_eq!(game.total, 63);
    }

    #[test]
    fn retention_keeps_more_than_two_months_and_drops_invalid_keys() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let mut daily = BTreeMap::new();
        for index in 0..500 {
            let date = today.checked_sub_days(Days::new(index)).unwrap();
            daily.insert(
                date_key(date),
                DailyRecord {
                    taps: 1,
                    game_ms: 0,
                },
            );
        }
        daily.insert("not-a-date".into(), DailyRecord::default());
        let kept = sanitize_daily(daily, today);
        assert_eq!(kept.len(), RETENTION_DAYS as usize);
        assert!(
            kept.contains_key(&date_key(
                today
                    .checked_sub_days(Days::new(RETENTION_DAYS as u64 - 1))
                    .unwrap()
            ))
        );
    }

    #[test]
    fn scale_actions_are_bounded_and_persisted() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        {
            let mut state = state.lock().unwrap();
            state.action("scale-40").unwrap();
            assert_eq!(state.scale_percent, 40);
            state.action("scale-45").unwrap();
            assert_eq!(state.scale_percent, 40);
            state.persist().unwrap();
        }
        let reopened = CyberWorkState::load(store.path(), UiBridge::new());
        assert_eq!(reopened.lock().unwrap().scale_percent, 40);
    }

    #[test]
    fn menu_is_a_two_level_tree_with_the_current_scale_checked() {
        let store = tempfile::tempdir().unwrap();
        let state = test_state(&store);
        let state = state.lock().unwrap();
        let menu = state.menu();
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].children.len(), ALLOWED_SCALE_PERCENTS.len());
        assert_eq!(menu[0].children[4].checked, Some(true));
        assert_eq!(menu[0].children[0].checked, Some(false));
        assert_eq!(menu[1].label, "Usage history");
        assert!(menu[1].children.is_empty());
    }

    #[test]
    fn corrupt_data_is_backed_up_and_starts_empty() {
        let store = tempfile::tempdir().unwrap();
        let path = store.path().join(DATA_FILE);
        std::fs::write(&path, b"{not json").unwrap();
        let state = CyberWorkState::load(store.path(), UiBridge::new());
        assert!(!path.is_file());
        assert!(state.lock().unwrap().error.is_some());
        assert!(
            std::fs::read_dir(store.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        );
    }
}
