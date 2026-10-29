#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod todo;

use bongocat_companion::{UiBridge, data_dir, ensure_data_dir, run_host};
use std::sync::Arc;

fn main() {
    let directory = data_dir("com.bongocat.todo");
    let _ = ensure_data_dir(&directory);
    let ui = UiBridge::new();
    let state = todo::TodoState::load(&directory, ui.clone());
    run_host(Arc::clone(&state), ui.clone());

    let app_state = todo::AppState {
        todo: Arc::clone(&state),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            todo::todo_snapshot,
            todo::todo_create,
            todo::todo_toggle,
            todo::todo_delete,
            todo::todo_set_priority,
            todo::todo_snooze,
            todo::todo_set_ui_scale,
            todo::take_pending_open,
            todo::take_pending_bubbles,
            todo::open_panel,
            todo::open_quick_add,
            todo::get_pet_bounds,
            todo::get_locale,
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
                .name("bongocat-todo-stop".to_owned())
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
        .expect("bongocat todo plugin");
}
