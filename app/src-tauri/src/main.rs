//! TachiMana desktop: Tauri shell over the `tm-*` crates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod protocol;
mod state;

use state::AppState;

fn main() {
    tracing_subscriber::fmt().with_target(false).init();
    let paths = match tm_core::AppPaths::from_env() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("TachiMana: can't find a data folder: {e}");
            std::process::exit(1);
        }
    };
    let state = match AppState::open(paths) {
        Ok(s) => s,
        Err(e) => {
            // Most often a second copy already holds app.lock.
            eprintln!("TachiMana: can't start: {e}");
            std::process::exit(1);
        }
    };
    let result = tauri::Builder::default()
        .manage(state)
        .register_asynchronous_uri_scheme_protocol("tm", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::library,
            commands::categories,
            commands::category_create,
            commands::category_rename,
            commands::category_delete,
            commands::category_move,
            commands::manga_set_categories,
            commands::manga_detail,
            commands::rescan_local,
            commands::set_viewer,
            commands::chapter_pages,
            commands::save_progress,
            commands::set_chapter_read,
            commands::settings_get,
            commands::settings_set,
            commands::cache_usage,
            commands::cache_clear,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("TachiMana: {e}");
        std::process::exit(1);
    }
}
