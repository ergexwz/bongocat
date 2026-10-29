//! Mail account, notification and companion-window state.

pub mod logic;
pub mod manager;

use bongocat_companion::{CompanionState, UiBridge, UiEvent, now_ms, read_json, write_json};
use bongocat_plugin_sdk::{Configuration, PetBounds, PluginMenuItem, PluginOutput};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, State};

use manager::{
    ConnectionManager, ConnectionStatus, ConnectionStatusPayload, LastSeenUidPayload,
    NewMailPayload,
};

const KEYRING_PREFIX: &str = "com.bongocat.mail";
const MAX_ACCOUNTS: usize = 16;
const MAX_NOTIFICATIONS: usize = 500;
const UNREAD_ARCHIVE_MS: i64 = 24 * 60 * 60 * 1000;
const READ_ARCHIVE_MS: i64 = 5 * 60 * 1000;
const ARCHIVE_PURGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailAccount {
    pub id: String,
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub provider_domain: String,
    pub enabled: bool,
    pub status: String,
    pub last_seen_uid: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailNotification {
    pub id: String,
    pub account_id: String,
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub arrived_at: i64,
    pub status: String,
    pub read_at: Option<i64>,
    pub archived_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailSettings {
    pub proxy: String,
    pub bubble_enabled: bool,
    pub bubble_auto_dismiss: bool,
    pub unread_only: bool,
}

impl Default for MailSettings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            bubble_enabled: true,
            bubble_auto_dismiss: false,
            unread_only: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MailFile {
    schema_version: u32,
    accounts: Vec<MailAccount>,
    notifications: Vec<MailNotification>,
    #[serde(default)]
    settings: MailSettings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailSnapshot {
    pub accounts: Vec<MailAccount>,
    pub notifications: Vec<MailNotification>,
    pub settings: MailSettings,
}

pub struct MailStore {
    path: PathBuf,
    accounts: Vec<MailAccount>,
    notifications: Vec<MailNotification>,
    settings: MailSettings,
}

impl MailStore {
    pub fn load(data_dir: &Path) -> Arc<Mutex<Self>> {
        let path = data_dir.join("mail.json");
        let file = read_json::<MailFile>(&path).ok().flatten();
        let (mut accounts, notifications, settings) = match file {
            Some(file) if file.schema_version == 1 => {
                (file.accounts, file.notifications, file.settings)
            }
            _ => (Vec::new(), Vec::new(), MailSettings::default()),
        };
        for account in &mut accounts {
            account.status = "idle".into();
        }
        Arc::new(Mutex::new(Self {
            path,
            accounts,
            notifications,
            settings,
        }))
    }

    fn persist(&self) {
        let _ = write_json(
            &self.path,
            &MailFile {
                schema_version: 1,
                accounts: self.accounts.clone(),
                notifications: self.notifications.clone(),
                settings: self.settings.clone(),
            },
        );
    }

    fn snapshot(&self) -> MailSnapshot {
        MailSnapshot {
            accounts: self.accounts.clone(),
            notifications: self.notifications.clone(),
            settings: self.settings.clone(),
        }
    }

    fn record_new(&mut self, payload: &NewMailPayload) -> bool {
        if payload.uid > 0
            && self
                .notifications
                .iter()
                .any(|item| item.account_id == payload.account_id && item.uid == payload.uid)
        {
            return false;
        }
        self.notifications.push(MailNotification {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: payload.account_id.clone(),
            uid: payload.uid,
            from: payload.from.clone(),
            subject: payload.subject.clone(),
            arrived_at: payload.arrived_at,
            status: "unread".into(),
            read_at: None,
            archived_at: None,
        });
        if self.notifications.len() > MAX_NOTIFICATIONS {
            let excess = self.notifications.len() - MAX_NOTIFICATIONS;
            self.notifications.drain(0..excess);
        }
        self.persist();
        true
    }

    fn mark_read(&mut self, id: &str) -> bool {
        let Some(item) = self.notifications.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        if item.status != "unread" {
            return false;
        }
        item.status = "read".into();
        item.read_at = Some(now_ms());
        self.persist();
        true
    }

    fn archive(&mut self, id: &str) -> bool {
        let Some(item) = self.notifications.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        if item.status == "archived" {
            return false;
        }
        item.status = "archived".into();
        item.archived_at = Some(now_ms());
        self.persist();
        true
    }

    fn delete(&mut self, id: &str) -> bool {
        let before = self.notifications.len();
        self.notifications.retain(|item| item.id != id);
        let changed = before != self.notifications.len();
        if changed {
            self.persist();
        }
        changed
    }

    fn retention(&mut self) -> bool {
        let now = now_ms();
        let mut changed = false;
        for item in &mut self.notifications {
            let archive = (item.status == "unread" && now - item.arrived_at >= UNREAD_ARCHIVE_MS)
                || (item.status == "read"
                    && item
                        .read_at
                        .is_some_and(|read| now - read >= READ_ARCHIVE_MS));
            if archive {
                item.status = "archived".into();
                item.archived_at = Some(now);
                changed = true;
            }
        }
        let before = self.notifications.len();
        self.notifications.retain(|item| {
            !(item.status == "archived"
                && item
                    .archived_at
                    .is_some_and(|at| now - at >= ARCHIVE_PURGE_MS))
        });
        changed |= before != self.notifications.len();
        if changed {
            self.persist();
        }
        changed
    }

    fn set_status(&mut self, account_id: &str, status: &str) -> bool {
        let Some(account) = self.accounts.iter_mut().find(|item| item.id == account_id) else {
            return false;
        };
        if account.status == status {
            return false;
        }
        account.status = status.into();
        self.persist();
        true
    }

    fn set_last_seen(&mut self, account_id: &str, uid: u32) -> bool {
        let Some(account) = self.accounts.iter_mut().find(|item| item.id == account_id) else {
            return false;
        };
        if uid <= account.last_seen_uid {
            return false;
        }
        account.last_seen_uid = uid;
        self.persist();
        true
    }
}

pub struct MailState {
    store: Arc<Mutex<MailStore>>,
    ui: UiBridge,
    locale: String,
    pet_bounds: Option<PetBounds>,
    pending_open: Vec<String>,
    last_retention: i64,
}

impl MailState {
    pub fn new(store: Arc<Mutex<MailStore>>, ui: UiBridge) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            store,
            ui,
            locale: "en-US".into(),
            pet_bounds: None,
            pending_open: Vec::new(),
            last_retention: 0,
        }))
    }

    fn copy(&self) -> (&'static str, &'static str, &'static str) {
        if self.locale == "zh-CN" {
            ("邮件列表", "归档邮件", "邮件设置")
        } else {
            ("Mail List", "Archived", "Mail Settings")
        }
    }

    fn menu(&self) -> Vec<PluginMenuItem> {
        let (list, archive, settings) = self.copy();
        vec![
            PluginMenuItem {
                id: "open-list".into(),
                label: list.into(),
            },
            PluginMenuItem {
                id: "open-archive".into(),
                label: archive.into(),
            },
            PluginMenuItem {
                id: "open-settings".into(),
                label: settings.into(),
            },
        ]
    }

    fn open(&mut self, kind: &str) {
        if !self.pending_open.iter().any(|pending| pending == kind) {
            self.pending_open.push(kind.to_owned());
        }
        self.ui.emit(UiEvent::Open {
            kind: kind.to_owned(),
        });
    }
}

impl CompanionState for MailState {
    fn id(&self) -> &'static str {
        "com.bongocat.mail"
    }

    fn start(&mut self, _config: Configuration, _state: Vec<u8>) -> Result<PluginOutput, String> {
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn tick(&mut self, _elapsed_ms: u64) -> Result<PluginOutput, String> {
        let now = now_ms();
        if now - self.last_retention >= 60_000 {
            self.last_retention = now;
            let changed = self
                .store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retention();
            if changed {
                self.ui.emit(UiEvent::StateChanged);
            }
        }
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn action(&mut self, action: &str) -> Result<PluginOutput, String> {
        match action {
            "open-list" => self.open("mail-list"),
            "open-archive" => self.open("mail-archive"),
            "open-settings" => self.open("mail-settings"),
            _ => {}
        }
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn configure(&mut self, config: Configuration) -> Result<PluginOutput, String> {
        if !config.is_empty() {
            return Err("mail does not accept configuration".into());
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
        let mut store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        store.accounts.clear();
        store.notifications.clear();
        store.settings = MailSettings::default();
        store.persist();
        drop(store);
        self.ui.emit(UiEvent::StateChanged);
        Ok(PluginOutput {
            menu: self.menu(),
            ..Default::default()
        })
    }

    fn stop(&mut self) -> Result<PluginOutput, String> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .persist();
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
    pub store: Arc<Mutex<MailStore>>,
    pub mail: Arc<Mutex<MailState>>,
}

fn keyring_key(account_id: &str) -> String {
    format!("{KEYRING_PREFIX}/{account_id}")
}

fn account_password(account_id: &str, username: &str) -> Result<String, String> {
    keyring::v1::Entry::new(&keyring_key(account_id), username)
        .map_err(|_| "keyring entry could not be created".to_owned())?
        .get_password()
        .map_err(|_| "mail password could not be read from the keyring".to_owned())
}

#[tauri::command]
pub fn mail_snapshot(state: State<'_, AppState>) -> MailSnapshot {
    state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot()
}

#[tauri::command]
pub fn mail_record_new(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: NewMailPayload,
) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .record_new(&payload);
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_record_status(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ConnectionStatusPayload,
) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .set_status(
            &payload.account_id,
            match payload.status {
                ConnectionStatus::Connected => "connected",
                ConnectionStatus::Connecting => "connecting",
                ConnectionStatus::Error => "error",
            },
        );
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_record_last_seen(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: LastSeenUidPayload,
) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .set_last_seen(&payload.account_id, payload.last_seen_uid);
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_mark_read(app: AppHandle, state: State<'_, AppState>, id: String) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .mark_read(&id);
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_mark_read_uid(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    uid: u32,
) -> bool {
    let id = {
        let store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        store
            .notifications
            .iter()
            .find(|item| item.account_id == account_id && item.uid == uid)
            .map(|item| item.id.clone())
    };
    let changed = id.is_some_and(|id| {
        state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_read(&id)
    });
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_archive(app: AppHandle, state: State<'_, AppState>, id: String) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .archive(&id);
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_delete_notification(app: AppHandle, state: State<'_, AppState>, id: String) -> bool {
    let changed = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .delete(&id);
    if changed {
        let _ = app.emit("mail://state", ());
    }
    changed
}

#[tauri::command]
pub fn mail_set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: MailSettings,
) -> MailSnapshot {
    let mut store = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    store.settings = settings;
    store.persist();
    let snapshot = store.snapshot();
    drop(store);
    let _ = app.emit("mail://state", ());
    snapshot
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
}

#[tauri::command]
pub async fn mail_test_connection(
    input: AccountInput,
    proxy: Option<String>,
) -> Result<(), String> {
    let mut session = manager::build_imap_session(
        &input.imap_host,
        input.imap_port,
        &input.username,
        &input.password,
        proxy.as_deref(),
    )
    .await?;
    let _ = session.logout().await;
    Ok(())
}

#[tauri::command]
pub async fn mail_add_account(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, ConnectionManager>,
    input: AccountInput,
    proxy: Option<String>,
) -> Result<MailSnapshot, String> {
    if input.address.trim().is_empty()
        || input.imap_host.trim().is_empty()
        || input.username.trim().is_empty()
        || input.password.is_empty()
    {
        return Err("please fill in all fields".into());
    }
    let account_id = uuid::Uuid::new_v4().to_string();
    let domain = input
        .address
        .rsplit_once('@')
        .map_or(String::new(), |(_, domain)| domain.to_lowercase());
    let account = MailAccount {
        id: account_id.clone(),
        address: input.address.trim().to_owned(),
        imap_host: input.imap_host.trim().to_owned(),
        imap_port: input.imap_port,
        username: input.username.trim().to_owned(),
        provider_domain: domain,
        enabled: true,
        status: "connecting".into(),
        last_seen_uid: 0,
    };
    keyring::v1::Entry::new(&keyring_key(&account.id), &account.username)
        .map_err(|_| "keyring entry could not be created".to_owned())?
        .set_password(&input.password)
        .map_err(|_| "mail password could not be stored".to_owned())?;
    let snapshot = {
        let mut store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if store.accounts.len() >= MAX_ACCOUNTS {
            return Err("too many mail accounts".into());
        }
        store.accounts.push(account.clone());
        store.persist();
        store.snapshot()
    };
    let _ = app.emit("mail://state", ());
    manager
        .connect(
            app.clone(),
            account.id.clone(),
            account.imap_host.clone(),
            account.imap_port,
            account.username.clone(),
            proxy,
            0,
        )
        .await?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn mail_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, ConnectionManager>,
    id: String,
    enabled: bool,
    proxy: Option<String>,
) -> Result<MailSnapshot, String> {
    let account = {
        let store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        store
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
            .ok_or_else(|| "mail account was not found".to_owned())?
    };
    {
        let mut store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = store.accounts.iter_mut().find(|entry| entry.id == id) {
            entry.enabled = enabled;
            entry.status = if enabled { "connecting" } else { "idle" }.into();
        }
        store.persist();
    }
    if enabled {
        manager
            .connect(
                app.clone(),
                account.id.clone(),
                account.imap_host,
                account.imap_port,
                account.username,
                proxy,
                account.last_seen_uid,
            )
            .await?;
    } else {
        manager.disconnect(&id).await;
    }
    let _ = app.emit("mail://state", ());
    Ok(state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot())
}

#[tauri::command]
pub async fn mail_remove_account(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, ConnectionManager>,
    id: String,
) -> Result<MailSnapshot, String> {
    let username = {
        let store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        store
            .accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| account.username.clone())
    };
    manager.disconnect(&id).await;
    if let Some(username) = username {
        let entry = keyring::v1::Entry::new(&keyring_key(&id), &username)
            .map_err(|_| "keyring entry could not be created".to_owned())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err("mail password could not be removed".to_owned()),
        }
    }
    let mut store = state
        .store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    store.accounts.retain(|account| account.id != id);
    store.notifications.retain(|item| item.account_id != id);
    store.persist();
    let snapshot = store.snapshot();
    drop(store);
    let _ = app.emit("mail://state", ());
    Ok(snapshot)
}

#[tauri::command]
pub async fn mail_reconnect_all(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, ConnectionManager>,
) -> Result<(), String> {
    let (accounts, proxy) = {
        let store = state
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            store.accounts.clone(),
            if store.settings.proxy.trim().is_empty() {
                None
            } else {
                Some(store.settings.proxy.trim().to_owned())
            },
        )
    };
    for account in accounts.into_iter().filter(|account| account.enabled) {
        let _ = manager
            .connect(
                app.clone(),
                account.id,
                account.imap_host,
                account.imap_port,
                account.username,
                proxy.clone(),
                account.last_seen_uid,
            )
            .await;
    }
    Ok(())
}

#[tauri::command]
pub fn mail_open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("unsupported URL".into());
    }
    opener::open(url).map_err(|_| "could not open the mail site".to_owned())
}

#[tauri::command]
pub fn take_pending_open(state: State<'_, AppState>, view: String) -> Option<String> {
    let mut mail = state
        .mail
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let position = mail.pending_open.iter().position(|kind| kind == &view)?;
    Some(mail.pending_open.remove(position))
}

#[tauri::command]
pub fn get_pet_bounds(state: State<'_, AppState>) -> Option<PetBounds> {
    state
        .mail
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pet_bounds
}

#[tauri::command]
pub fn get_locale(state: State<'_, AppState>) -> String {
    state
        .mail
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .locale
        .clone()
}

#[tauri::command]
pub fn mail_password_exists(account_id: String, username: String) -> bool {
    account_password(&account_id, &username).is_ok()
}
