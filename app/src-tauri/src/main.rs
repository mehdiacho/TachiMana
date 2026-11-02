//! TachiMana desktop: Tauri shell over the `tm-*` crates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod downloads;
mod dto;
mod online;
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
        .setup(|app| {
            use tauri::Manager;
            if let Ok(res) = app.path().resource_dir() {
                let _ = app.state::<AppState>().host_dir.set(res.join("ext-host"));
            }
            downloads::init(app.handle());
            // Stop the extension host when it has been idle for a while; it restarts on demand.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                online::stop_if_idle(&handle.state::<AppState>(), online::HOST_IDLE);
            });
            Ok(())
        })
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
            online::browse_status,
            online::ext_repos,
            online::ext_repo_add,
            online::ext_repo_remove,
            online::ext_installed,
            online::ext_catalog,
            online::ext_install,
            online::ext_uninstall,
            online::browse_sources,
            online::browse_list,
            online::online_open,
            online::manga_refresh,
            online::library_update,
            online::library_set_favorite,
            online::source_preferences,
            online::source_preference_set,
            downloads::downloads_list,
            downloads::download_chapters,
            downloads::download_unread,
            downloads::download_delete,
            downloads::download_dismiss,
            downloads::downloads_clear_finished,
            downloads::downloads_on_disk,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("TachiMana: {e}");
        std::process::exit(1);
    }
}
