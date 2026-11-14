//! TachiMana desktop: Tauri shell over the `tm-*` crates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod changes;
mod commands;
mod downloads;
mod dto;
mod helper;
mod links;
mod logging;
mod nas;
mod online;
mod phone_queue;
mod profiles;
mod protocol;
mod state;
mod sync;
mod track;
mod update;

use state::AppState;

fn main() {
    let root = match tm_core::AppPaths::from_env() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("TachiMana: can't find a data folder: {e}");
            std::process::exit(1);
        }
    };
    logging::init(&root.logs_dir());
    // Which profile to open: the one saved as active (the data folder itself on first start).
    let (open, list, paths) = match tm_core::Profiles::load_or_init(root.root()).and_then(|l| {
        let open = l.active().map(|p| p.id.clone()).unwrap_or_default();
        let paths = l.paths(root.root(), &open)?;
        Ok((open, l, paths))
    }) {
        Ok(v) => v,
        Err(e) => {
            // Never refuse to start over the profile list: open the data folder as before.
            tracing::error!("profiles unavailable, opening the data folder: {e}");
            (
                String::new(),
                tm_core::Profiles {
                    active: String::new(),
                    profiles: Vec::new(),
                },
                root,
            )
        }
    };
    tracing::info!(profile = %open, dir = %paths.data_dir().display(), "opening profile");
    let state = match AppState::open(paths) {
        Ok(s) => s,
        Err(e) => {
            // Most often a second copy already holds app.lock.
            eprintln!("TachiMana: can't start: {e}");
            std::process::exit(1);
        }
    };
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .manage(profiles::ProfileState::new(open, list))
        .setup(|app| {
            use tauri::Manager;
            if let Ok(res) = app.path().resource_dir() {
                let _ = app.state::<AppState>().host_dir.set(res.join("ext-host"));
            }
            downloads::init(app.handle());
            // Uploads left over from the last run carry on.
            nas::kick(app.handle());
            // Serve this library on the LAN and look for other devices, like the Kotlin app.
            sync::init(app.handle());
            // One quiet update check a little after start-up.
            update::init(app.handle());
            // After an app update, bring the extension helper to this version too.
            helper::update_if_stale(app.handle());
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
            changes::changes_sync_now,
            changes::changes_last,
            changes::link_set_mode,
            commands::app_info,
            commands::open_folder,
            profiles::profiles_list,
            profiles::profile_create,
            profiles::profile_edit,
            profiles::profile_switch,
            profiles::profile_delete,
            links::links_info,
            links::link_device,
            links::unlink_device,
            links::link_refused,
            phone_queue::phone_queue,
            phone_queue::phone_queue_resume,
            phone_queue::phone_queue_set_dest,
            phone_queue::phone_queue_clear_finished,
            links::dismiss_refused,
            commands::library,
            commands::recents,
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
            nas::nas_settings_get,
            nas::nas_settings_set,
            nas::nas_test,
            nas::nas_uploads,
            nas::nas_uploads_retry,
            nas::nas_upload_dismiss,
            nas::nas_uploads_clear_finished,
            nas::nas_cache_usage,
            nas::nas_cache_clear,
            nas::nas_list_manga,
            sync::sync_status,
            sync::sync_add_peer,
            sync::sync_pull,
            sync::backup_export,
            sync::backup_restore,
            sync::backup_restore_path,
            sync::restore_extensions,
            sync::restore_redownload,
            sync::restore_dismiss,
            track::anilist_account,
            track::anilist_connect,
            track::anilist_connect_token,
            track::anilist_disconnect,
            track::track_get,
            track::track_search,
            track::track_bind,
            track::track_unbind,
            track::track_set_status,
            track::track_set_progress,
            track::track_set_score,
            track::track_open,
            update::update_status,
            update::update_check,
            update::update_download,
            update::update_install,
            update::update_open_page,
            helper::helper_status,
            helper::helper_install_file,
            helper::helper_download,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("TachiMana: {e}");
        std::process::exit(1);
    }
}
