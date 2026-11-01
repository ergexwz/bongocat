#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cyberwork;

use bongocat_companion::{UiBridge, data_dir, ensure_data_dir, run_host};
use cyberwork::{AppState, CyberWorkState};
use std::sync::Arc;

fn main() {
    let directory = data_dir("com.bongocat.cyberwork");
    let _ = ensure_data_dir(&directory);
    let ui = UiBridge::new();
    let state = CyberWorkState::load(&directory, ui.clone());
    run_host(Arc::clone(&state), ui.clone());

    let app_state = AppState { state };

    let window_presence = ui.clone();
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            cyberwork::cyberwork_render,
            cyberwork::cyberwork_history,
            cyberwork::take_pending_open,
            cyberwork::get_locale,
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
                .name("bongocat-cyberwork-stop".to_owned())
                .spawn(move || {
                    while !watcher.stop_requested() {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    let exit = handle.clone();
                    let _ = handle.run_on_main_thread(move || exit.exit(0));
                })?;
            Ok(())
        })
        .on_window_event(move |window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window_presence.set_window_open(window.label(), false);
            }
        })
        .build(tauri::generate_context!())
        .expect("bongocat cyberwork plugin")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
