#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mail;

use bongocat_companion::{UiBridge, data_dir, ensure_data_dir, run_host};
use mail::{AppState, MailState, MailStore};
use std::sync::Arc;

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = data_dir("com.bongocat.mail");
    let _ = ensure_data_dir(&directory);
    let ui = UiBridge::new();
    let store = MailStore::load(&directory);
    let mail = MailState::new(Arc::clone(&store), ui.clone());
    run_host(Arc::clone(&mail), ui.clone());

    let app_state = AppState { store, mail };

    tauri::Builder::default()
        .manage(app_state)
        .manage(mail::manager::ConnectionManager::new())
        .invoke_handler(tauri::generate_handler![
            mail::mail_snapshot,
            mail::mail_record_new,
            mail::mail_record_status,
            mail::mail_record_last_seen,
            mail::mail_mark_read,
            mail::mail_mark_read_uid,
            mail::mail_archive,
            mail::mail_delete_notification,
            mail::mail_set_settings,
            mail::mail_set_ui_scale,
            mail::mail_test_connection,
            mail::mail_add_account,
            mail::mail_set_enabled,
            mail::mail_remove_account,
            mail::mail_reconnect_all,
            mail::mail_open_url,
            mail::mail_password_exists,
            mail::take_pending_open,
            mail::get_pet_bounds,
            mail::get_locale,
        ])
        .setup(move |app| {
            let stop_pending = ui.attach(app.handle().clone());
            if stop_pending {
                app.handle().exit(0);
                return Ok(());
            }
            let watcher = ui.clone();
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("bongocat-mail-stop".to_owned())
                .spawn(move || {
                    while !watcher.stop_requested() {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    let exit = handle.clone();
                    let _ = handle.run_on_main_thread(move || exit.exit(0));
                })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("bongocat mail plugin");
}
