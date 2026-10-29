use bongocat_companion::{CompanionState, UiBridge, UiEvent, now_ms, read_json, write_json};
use bongocat_plugin_sdk::{
    BubbleKind, Configuration, PetBounds, PluginBubble, PluginMenuItem, PluginOutput,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;

pub const MAX_TASKS: usize = 500;
pub const MAX_TITLE_CHARS: usize = 200;
pub const MIN_UI_SCALE: f64 = 0.8;
pub const MAX_UI_SCALE: f64 = 1.5;
const DUE_CHECK_MS: i64 = 10_000;

fn default_ui_scale() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoUiSettings {
    #[serde(default = "default_ui_scale")]
    pub panel_scale: f64,
    #[serde(default = "default_ui_scale")]
    pub mini_scale: f64,
}

impl Default for TodoUiSettings {
    fn default() -> Self {
        Self {
            panel_scale: default_ui_scale(),
            mini_scale: default_ui_scale(),
        }
    }
}

fn normalize_ui_scale(scale: f64) -> Result<f64, String> {
    if !scale.is_finite() {
        return Err("todo ui scale is invalid".into());
    }
    Ok(scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE))
}

fn sanitize_ui_settings(mut ui: TodoUiSettings) -> TodoUiSettings {
    ui.panel_scale = normalize_ui_scale(ui.panel_scale).unwrap_or(1.0);
    ui.mini_scale = normalize_ui_scale(ui.mini_scale).unwrap_or(1.0);
    ui
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub priority: String,
    pub due_date: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub notified_due_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoSnapshot {
    pub tasks: Vec<Todo>,
    pub error: Option<String>,
    pub ui: TodoUiSettings,
}

#[derive(Serialize, Deserialize)]
struct TodoFile {
    schema_version: u32,
    tasks: Vec<Todo>,
    #[serde(default)]
    ui: TodoUiSettings,
}

pub struct TodoState {
    data_file: PathBuf,
    tasks: Vec<Todo>,
    ui_settings: TodoUiSettings,
    ui: UiBridge,
    locale: String,
    pet_bounds: Option<PetBounds>,
    last_due_check: i64,
    pending_open: Vec<String>,
    pending_bubbles: Vec<PluginBubble>,
    error: Option<String>,
}

impl TodoState {
    pub fn load(data_dir: &Path, ui: UiBridge) -> Arc<Mutex<Self>> {
        let data_file = data_dir.join("todo.json");
        let (tasks, ui_settings, error) = match read_json::<TodoFile>(&data_file) {
            Ok(Some(file)) if file.schema_version == 1 => {
                (file.tasks, sanitize_ui_settings(file.ui), None)
            }
            Ok(Some(_)) => (
                Vec::new(),
                TodoUiSettings::default(),
                Some("unsupported todo schema".to_owned()),
            ),
            Ok(None) => (Vec::new(), TodoUiSettings::default(), None),
            Err(_) => (
                Vec::new(),
                TodoUiSettings::default(),
                Some("todo data could not be read".to_owned()),
            ),
        };
        Arc::new(Mutex::new(Self {
            data_file,
            tasks,
            ui_settings,
            ui,
            locale: "en-US".into(),
            pet_bounds: None,
            last_due_check: 0,
            pending_open: Vec::new(),
            pending_bubbles: Vec::new(),
            error,
        }))
    }

    fn persist(&self) -> Result<(), String> {
        write_json(
            &self.data_file,
            &TodoFile {
                schema_version: 1,
                tasks: self.tasks.clone(),
                ui: self.ui_settings.clone(),
            },
        )
        .map_err(|_| "todo data could not be saved".to_owned())
    }

    fn copy(&self) -> (&'static str, &'static str) {
        if self.locale == "zh-CN" {
            ("待办列表", "快速新建")
        } else {
            ("Todo List", "Quick Add")
        }
    }

    fn menu(&self) -> Vec<PluginMenuItem> {
        let (panel, quick) = self.copy();
        vec![
            PluginMenuItem {
                id: "open-panel".into(),
                label: panel.into(),
            },
            PluginMenuItem {
                id: "open-quick-add".into(),
                label: quick.into(),
            },
        ]
    }

    fn sorted(&self) -> Vec<Todo> {
        let mut tasks = self.tasks.clone();
        tasks.sort_by(|left, right| {
            left.completed
                .cmp(&right.completed)
                .then_with(|| {
                    priority_weight(&right.priority).cmp(&priority_weight(&left.priority))
                })
                .then_with(|| match (left.due_date, right.due_date) {
                    (Some(left), Some(right)) => left.cmp(&right),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        tasks
    }

    fn snapshot(&self) -> TodoSnapshot {
        TodoSnapshot {
            tasks: self.sorted(),
            error: self.error.clone(),
            ui: self.ui_settings.clone(),
        }
    }

    fn open(&mut self, kind: &str) {
        if !self.pending_open.iter().any(|pending| pending == kind) {
            self.pending_open.push(kind.to_owned());
        }
        self.ui.emit(UiEvent::Open {
            kind: kind.to_owned(),
        });
    }

    fn emit_state(&self) {
        self.ui.emit(UiEvent::StateChanged);
    }

    fn due_bubble(&self, task: &Todo) -> PluginBubble {
        let zh = self.locale == "zh-CN";
        PluginBubble {
            id: format!("todo-{}-{}", task.id, task.due_date.unwrap_or_default()),
            kind: BubbleKind::Todo,
            source: if zh { "待办到期" } else { "Todo due" }.into(),
            title: task.title.clone(),
            subtitle: if zh { "已到期" } else { "is due now" }.into(),
            urgent: true,
            action: format!("open-todo-{}", task.id),
            auto_dismiss_ms: None,
        }
    }

    pub fn create(
        &mut self,
        title: &str,
        due_date: Option<i64>,
        priority: &str,
    ) -> Result<TodoSnapshot, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("todo title is empty".into());
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return Err("todo title is too long".into());
        }
        if self.tasks.len() >= MAX_TASKS {
            return Err("todo list is full".into());
        }
        if !matches!(priority, "low" | "medium" | "high") {
            return Err("todo priority is invalid".into());
        }
        let now = now_ms();
        self.tasks.push(Todo {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_owned(),
            completed: false,
            priority: priority.to_owned(),
            due_date,
            created_at: now,
            updated_at: now,
            notified_due_at: None,
        });
        self.error = None;
        self.persist()?;
        self.emit_state();
        Ok(self.snapshot())
    }

    pub fn toggle(&mut self, id: &str) -> Result<TodoSnapshot, String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(|| "todo was not found".to_owned())?;
        task.completed = !task.completed;
        task.updated_at = now_ms();
        self.persist()?;
        self.emit_state();
        Ok(self.snapshot())
    }

    pub fn delete(&mut self, id: &str) -> Result<TodoSnapshot, String> {
        let before = self.tasks.len();
        self.tasks.retain(|task| task.id != id);
        if before == self.tasks.len() {
            return Err("todo was not found".to_owned());
        }
        self.persist()?;
        self.emit_state();
        Ok(self.snapshot())
    }

    pub fn set_priority(&mut self, id: &str, priority: &str) -> Result<TodoSnapshot, String> {
        if !matches!(priority, "low" | "medium" | "high") {
            return Err("todo priority is invalid".into());
        }
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(|| "todo was not found".to_owned())?;
        task.priority = priority.to_owned();
        task.updated_at = now_ms();
        self.persist()?;
        self.emit_state();
        Ok(self.snapshot())
    }

    pub fn snooze(&mut self, id: &str, until: i64) -> Result<TodoSnapshot, String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(|| "todo was not found".to_owned())?;
        task.due_date = Some(until);
        task.notified_due_at = None;
        task.updated_at = now_ms();
        self.persist()?;
        self.emit_state();
        Ok(self.snapshot())
    }
}

fn priority_weight(priority: &str) -> u8 {
    match priority {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

impl CompanionState for TodoState {
    fn id(&self) -> &'static str {
        "com.bongocat.todo"
    }

    fn start(&mut self, _config: Configuration, _state: Vec<u8>) -> Result<PluginOutput, String> {
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn tick(&mut self, _elapsed_ms: u64) -> Result<PluginOutput, String> {
        let now = now_ms();
        if now - self.last_due_check >= DUE_CHECK_MS {
            self.last_due_check = now;
            let due = self
                .tasks
                .iter()
                .filter(|task| {
                    !task.completed
                        && task
                            .due_date
                            .is_some_and(|due| due <= now && task.notified_due_at != Some(due))
                })
                .cloned()
                .collect::<Vec<_>>();
            if !due.is_empty() {
                for task in &due {
                    if let Some(entry) = self.tasks.iter_mut().find(|entry| entry.id == task.id) {
                        entry.notified_due_at = entry.due_date;
                    }
                }
                let _ = self.persist();
                let zh = self.locale == "zh-CN";
                for task in due {
                    let title = if zh {
                        "待办到期提醒"
                    } else {
                        "Todo due reminder"
                    };
                    let body = if zh {
                        format!("「{}」到点啦", task.title)
                    } else {
                        format!("\"{}\" is due now", task.title)
                    };
                    self.ui.emit(UiEvent::Notify {
                        title: title.into(),
                        body,
                    });
                    let bubble = self.due_bubble(&task);
                    self.pending_bubbles.push(bubble.clone());
                    self.ui.emit(UiEvent::Bubble { bubble });
                }
            }
        }
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn action(&mut self, action: &str) -> Result<PluginOutput, String> {
        match action {
            "open-panel" => self.open("todo"),
            "open-quick-add" => self.open("todo-mini"),
            _ if action.starts_with("open-todo-") => self.open("todo"),
            _ => {}
        }
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn configure(&mut self, config: Configuration) -> Result<PluginOutput, String> {
        if !config.is_empty() {
            return Err("todo does not accept configuration".into());
        }
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn locale(&mut self, locale: &str) -> Result<PluginOutput, String> {
        self.locale = if locale == "zh-CN" { "zh-CN" } else { "en-US" }.into();
        self.ui.emit(UiEvent::Locale {
            locale: self.locale.clone(),
        });
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn pet_bounds(&mut self, bounds: PetBounds) -> Result<PluginOutput, String> {
        self.pet_bounds = Some(bounds);
        self.ui.emit(UiEvent::PetBounds { bounds });
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn reset(&mut self) -> Result<PluginOutput, String> {
        self.tasks.clear();
        self.error = None;
        self.persist()?;
        self.emit_state();
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn stop(&mut self) -> Result<PluginOutput, String> {
        self.persist()?;
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn output(&self) -> Result<PluginOutput, String> {
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }
}

pub struct AppState {
    pub todo: Arc<Mutex<TodoState>>,
}

#[tauri::command]
pub fn todo_snapshot(state: State<'_, AppState>) -> Result<TodoSnapshot, String> {
    let todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(todo.snapshot())
}

#[tauri::command]
pub fn todo_create(
    state: State<'_, AppState>,
    title: String,
    due_date: Option<i64>,
    priority: String,
) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    todo.create(&title, due_date, &priority)
}

#[tauri::command]
pub fn todo_toggle(state: State<'_, AppState>, id: String) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    todo.toggle(&id)
}

#[tauri::command]
pub fn todo_delete(state: State<'_, AppState>, id: String) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    todo.delete(&id)
}

#[tauri::command]
pub fn todo_set_priority(
    state: State<'_, AppState>,
    id: String,
    priority: String,
) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    todo.set_priority(&id, &priority)
}

#[tauri::command]
pub fn todo_snooze(
    state: State<'_, AppState>,
    id: String,
    until: i64,
) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    todo.snooze(&id, until)
}

#[tauri::command]
pub fn todo_set_ui_scale(
    state: State<'_, AppState>,
    view: String,
    scale: f64,
) -> Result<TodoSnapshot, String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let scale = normalize_ui_scale(scale)?;
    match view.as_str() {
        "todo" => todo.ui_settings.panel_scale = scale,
        "todo-mini" => todo.ui_settings.mini_scale = scale,
        _ => return Err("todo ui view is invalid".into()),
    }
    todo.persist()?;
    todo.emit_state();
    Ok(todo.snapshot())
}

#[tauri::command]
pub fn take_pending_open(state: State<'_, AppState>, view: String) -> Option<String> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let position = todo.pending_open.iter().position(|kind| kind == &view)?;
    Some(todo.pending_open.remove(position))
}

#[tauri::command]
pub fn take_pending_bubbles(state: State<'_, AppState>) -> Vec<PluginBubble> {
    let mut todo = state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::take(&mut todo.pending_bubbles)
}

#[tauri::command]
pub fn get_pet_bounds(state: State<'_, AppState>) -> Option<PetBounds> {
    state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pet_bounds
}

#[tauri::command]
pub fn open_panel(state: State<'_, AppState>) {
    state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .open("todo");
}

#[tauri::command]
pub fn open_quick_add(state: State<'_, AppState>) {
    state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .open("todo-mini");
}

#[tauri::command]
pub fn get_locale(state: State<'_, AppState>) -> String {
    state
        .todo
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .locale
        .clone()
}
