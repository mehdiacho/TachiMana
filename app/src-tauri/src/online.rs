//! Online sources through the Kotlin extension host (plan §P5): repos, extensions, browsing,
//! manga detail and chapters, online reading, source settings.
//!
//! The host starts on first use. Manga opened from Browse are stored as non-favourite rows
//! (`Db::upsert_online`) so the detail screen and the reader work on real ids; the heart only
//! flips `favorite`.

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::State;
use tm_core::settings::keys;
use tm_db::{Chapter, ExtensionRepo, Manga, SourceChapter, SourceManga};
use tm_exthost::{model, Client, ExtHost, HostConfig};

use crate::commands::detail_of;
use crate::dto::*;
use crate::state::{AppState, OnlineChapter};

type R<T> = Result<T, CmdError>;

/// Most pages accepted for one chapter.
const MAX_PAGES: usize = 5000;
/// Online chapters remembered for the `tm://page` handler.
const MAX_OPEN_CHAPTERS: usize = 64;

// ---- the host ----------------------------------------------------------------------------

/// Where `ext-host.jar` lives, and why: `TM_EXTHOST_DIR` (`custom`), the helper installed in the
/// data folder from Settings (`installed`), the installer's resource folder (`bundled`), or in
/// debug builds the Gradle output in the repo (`dev`). `missing` when none has the jar.
pub fn host_location(state: &AppState) -> (&'static str, PathBuf) {
    if let Some(dir) = std::env::var_os("TM_EXTHOST_DIR") {
        return ("custom", PathBuf::from(dir));
    }
    let installed = state.paths.ext_host_dir();
    if installed.join("ext-host.jar").is_file() {
        return ("installed", installed);
    }
    if let Some(dir) = state.host_dir.get() {
        if dir.join("ext-host.jar").is_file() {
            return ("bundled", dir.clone());
        }
    }
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ext-host/build/ext-host");
        if dev.join("ext-host.jar").is_file() {
            return ("dev", dev);
        }
    }
    ("missing", installed)
}

pub fn host_dir(state: &AppState) -> PathBuf {
    host_location(state).1
}

fn host_config(state: &AppState) -> HostConfig {
    HostConfig {
        host_dir: host_dir(state),
        data_dir: state.paths.data_dir().to_path_buf(),
        java: std::env::var_os("TM_JAVA").map(PathBuf::from),
    }
}

/// Runs blocking work without stalling the async runtime's worker when called from it.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

/// A client for the running host, starting (or restarting) it when needed.
pub fn client(state: &AppState) -> R<Client> {
    let mut slot = state
        .exthost
        .lock()
        .map_err(|_| CmdError::internal("extension host lock poisoned"))?;
    state.touch_host();
    if crate::helper::INSTALLING.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CmdError::new(
            "busy",
            "The extension helper is being installed. Try again in a moment.",
        ));
    }
    if let Some(host) = slot.as_mut() {
        if host.is_alive() {
            return Ok(host.client().clone());
        }
        tracing::warn!("ext-host exited; starting it again");
    }
    *slot = None;
    let cfg = host_config(state);
    let host = blocking(|| ExtHost::spawn(&cfg))?;
    let client = host.client().clone();
    *slot = Some(host);
    drop(slot);
    // The host starts with the Cloudflare solver off; tell it the user's choice.
    let enabled = state.settings()?.bool(keys::CLOUDFLARE_BYPASS_ENABLED);
    let c = client.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = c.set_cloudflare_bypass(enabled).await {
            tracing::warn!("could not set the Cloudflare bypass: {e}");
        }
    });
    Ok(client)
}

/// How long the host may sit unused before it is stopped (PLAN §P5).
pub const HOST_IDLE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Stops the host when nothing has used it for `idle` and no library update is running. The next
/// call starts it again. Returns whether it stopped one.
pub fn stop_if_idle(state: &AppState, idle: std::time::Duration) -> bool {
    use std::sync::atomic::Ordering;
    if state.updating.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(mut slot) = state.exthost.lock() else {
        return false;
    };
    let last = state.host_used.load(Ordering::Relaxed);
    let quiet = crate::state::now_secs().saturating_sub(last) >= idle.as_secs();
    if slot.is_some() && quiet {
        tracing::info!("ext-host idle for {} s; stopping it", idle.as_secs());
        // Dropping closes its stdin, waits briefly, then kills it.
        *slot = None;
        return true;
    }
    false
}

/// Pushes the Cloudflare toggle to a running host (a stopped host gets it when it starts).
pub fn push_cloudflare(state: &AppState, enabled: bool) {
    let running = state.exthost.lock().ok().and_then(|mut slot| {
        let host = slot.as_mut()?;
        host.is_alive().then(|| host.client().clone())
    });
    if let Some(c) = running {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = c.set_cloudflare_bypass(enabled).await {
                tracing::warn!("could not set the Cloudflare bypass: {e}");
            }
        });
    }
}

fn parse_source(id: &str) -> R<i64> {
    id.trim()
        .parse()
        .map_err(|_| CmdError::new("invalid", "That source id is not valid."))
}

// ---- status, repos, extensions -----------------------------------------------------------

/// Whether the extension helper is installed (starting it is then only a matter of time).
pub fn host_available(state: &AppState) -> bool {
    host_config(state).jar().is_file()
}

#[tauri::command]
pub fn browse_status(state: State<'_, AppState>) -> BrowseStatusDto {
    let cfg = host_config(&state);
    if cfg.jar().is_file() {
        BrowseStatusDto {
            available: true,
            message: None,
        }
    } else {
        BrowseStatusDto {
            available: false,
            message: Some(format!(
                "The extension host was not found at {}. Online sources need it.",
                cfg.host_dir.display()
            )),
        }
    }
}

#[tauri::command]
pub fn ext_repos(state: State<'_, AppState>) -> R<Vec<RepoDto>> {
    Ok(state
        .db
        .extension_repos()?
        .iter()
        .map(RepoDto::from)
        .collect())
}

/// Adds a repo from any URL the user pastes; the host resolves it (index v2, repo.json, legacy).
#[tauri::command]
pub async fn ext_repo_add(state: State<'_, AppState>, url: String) -> R<RepoDto> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(CmdError::new(
            "invalid",
            "Enter the repo's address, starting with https://",
        ));
    }
    let repo = client(&state)?.inspect_repo(url).await?;
    let row = ExtensionRepo {
        base_url: repo.index_url,
        name: repo.name,
        short_name: repo.short_name,
        website: repo.website,
        signing_key_fingerprint: repo.signing_key_fingerprint,
    };
    state.db.upsert_extension_repo(&row)?;
    Ok(RepoDto::from(&row))
}

#[tauri::command]
pub fn ext_repo_remove(state: State<'_, AppState>, base_url: String) -> R<()> {
    Ok(state.db.delete_extension_repo(&base_url)?)
}

#[tauri::command]
pub async fn ext_installed(state: State<'_, AppState>) -> R<Vec<InstalledExtensionDto>> {
    let exts = client(&state)?.extensions().await?;
    state.sync.set_installed(&exts);
    Ok(exts.iter().map(InstalledExtensionDto::from).collect())
}

/// Every extension the user's repos offer. One repo failing does not hide the others.
#[tauri::command]
pub async fn ext_catalog(state: State<'_, AppState>) -> R<CatalogDto> {
    let repos = state.db.extension_repos()?;
    let c = client(&state)?;
    let exts = c.extensions().await?;
    state.sync.set_installed(&exts);
    let installed: HashMap<String, model::Extension> =
        exts.into_iter().map(|e| (e.pkg_name.clone(), e)).collect();
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    for repo in &repos {
        match c.catalog(&repo.base_url).await {
            Ok(list) => entries.extend(list.into_iter().map(|e| {
                let have = installed.get(&e.pkg_name);
                CatalogEntryDto {
                    repo_url: repo.base_url.clone(),
                    repo_name: repo.short_name.clone().unwrap_or_else(|| repo.name.clone()),
                    update_available: have.is_some_and(|h| e.version_code > h.version_code),
                    installed_version: have.map(|h| h.version_name.clone()),
                    pkg_name: e.pkg_name,
                    name: e.name,
                    version_name: e.version_name,
                    lang: e.lang,
                    nsfw: e.nsfw,
                    icon_url: e.icon_url,
                }
            })),
            Err(e) => failures.push(RepoFailureDto {
                repo_name: repo.name.clone(),
                message: e.to_string(),
            }),
        }
    }
    entries.sort_by_key(|e| e.name.to_lowercase());
    Ok(CatalogDto { entries, failures })
}

/// Installs from one of the user's own repos only: a repo is never trusted implicitly.
#[tauri::command]
pub async fn ext_install(
    state: State<'_, AppState>,
    repo_url: String,
    pkg_name: String,
) -> R<InstalledExtensionDto> {
    let known = state
        .db
        .extension_repos()?
        .iter()
        .any(|r| r.base_url == repo_url);
    if !known {
        return Err(CmdError::new(
            "invalid",
            "Add this repo before installing from it.",
        ));
    }
    let ext = client(&state)?.install(&repo_url, &pkg_name).await?;
    Ok(InstalledExtensionDto::from(&ext))
}

#[tauri::command]
pub async fn ext_uninstall(state: State<'_, AppState>, pkg_name: String) -> R<()> {
    Ok(client(&state)?.uninstall(&pkg_name).await?)
}

// ---- browsing ------------------------------------------------------------------------------

#[tauri::command]
pub async fn browse_sources(state: State<'_, AppState>) -> R<Vec<SourceDto>> {
    let c = client(&state)?;
    let exts: HashMap<String, model::Extension> = c
        .extensions()
        .await?
        .into_iter()
        .map(|e| (e.pkg_name.clone(), e))
        .collect();
    let sources = c.sources().await?;
    let settings = state.settings()?.clone();
    let mut out: Vec<SourceDto> = sources
        .into_iter()
        .map(|s| {
            let ext = exts.get(&s.extension_pkg);
            let by_extension = ext.is_some_and(|e| e.nsfw);
            SourceDto {
                nsfw: settings.source_nsfw(s.id).unwrap_or(by_extension),
                nsfw_by_extension: by_extension,
                id: s.id,
                extension_name: ext.map(|e| e.name.clone()).unwrap_or_default(),
                icon_url: ext.and_then(|e| e.icon_url.clone()),
                name: s.name,
                lang: s.lang,
                extension_pkg: s.extension_pkg,
                supports_latest: s.supports_latest,
                is_configurable: s.is_configurable,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.lang.cmp(&b.lang))
    });
    state.nas.remember_sources(
        out.iter()
            .map(|s| (s.id, crate::nas::source_label(&s.name, &s.lang))),
    );
    Ok(out)
}

/// The person's own NSFW mark for a source; `None` goes back to what its extension says.
#[tauri::command]
pub fn source_set_nsfw(state: State<'_, AppState>, source_id: String, nsfw: Option<bool>) -> R<()> {
    let id = source_id
        .parse::<i64>()
        .map_err(|_| CmdError::new("invalid", "That is not a source."))?;
    let mut s = state.settings()?;
    s.set_source_nsfw(id, nsfw);
    s.save()?;
    Ok(())
}

/// One page of a source's popular, latest or search list.
#[tauri::command]
pub async fn browse_list(
    state: State<'_, AppState>,
    source_id: String,
    mode: String,
    query: Option<String>,
    page: u32,
) -> R<BrowsePageDto> {
    let source = parse_source(&source_id)?;
    let c = client(&state)?;
    let page = page.max(1);
    let result = match mode.as_str() {
        "popular" => c.popular(source, page).await?,
        "latest" => c.latest(source, page).await?,
        "search" => {
            c.search(source, query.as_deref().unwrap_or("").trim(), page)
                .await?
        }
        _ => return Err(CmdError::new("invalid", "Unknown list.")),
    };
    let mut mangas = Vec::with_capacity(result.mangas.len());
    for m in result.mangas {
        let library_id = state
            .db
            .manga_by_url(&m.url, source)?
            .filter(|row| row.favorite)
            .map(|row| row.id);
        mangas.push(OnlineMangaDto {
            url: m.url,
            title: m.title,
            thumbnail_url: m.thumbnail_url.filter(|u| !u.is_empty()),
            library_id,
        });
    }
    Ok(BrowsePageDto {
        mangas,
        has_next_page: result.has_next_page,
    })
}

fn to_source_manga(m: &model::Manga) -> SourceManga {
    SourceManga {
        url: m.url.clone(),
        title: m.title.clone(),
        artist: m.artist.clone(),
        author: m.author.clone(),
        description: m.description.clone(),
        genre: m.genre.clone(),
        status: i64::from(m.status),
        thumbnail_url: m.thumbnail_url.clone(),
        initialized: m.initialized,
    }
}

fn to_source_chapters(list: &[model::Chapter]) -> Vec<SourceChapter> {
    list.iter()
        .map(|c| SourceChapter {
            url: c.url.clone(),
            name: c.name.clone(),
            date_upload: c.date_upload,
            chapter_number: f64::from(c.chapter_number),
            scanlator: c.scanlator.clone(),
        })
        .collect()
}

/// Fetches details and chapters and stores them. On a source failure the stored detail (if
/// any) comes back with the source's own error message, so the screen can say what went wrong.
async fn refresh(state: &AppState, source: i64, url: &str, title: &str) -> R<RefreshDto> {
    let c = client(state)?;
    let req = model::Manga {
        url: url.to_owned(),
        title: title.to_owned(),
        ..Default::default()
    };
    match c.manga(source, &req, true, true).await {
        Ok(update) => {
            let mut manga = update.manga;
            if manga.url.is_empty() {
                manga.url = url.to_owned();
            }
            if manga.title.is_empty() {
                manga.title = title.to_owned();
            }
            let (id, added) = state.db.upsert_online(
                source,
                &to_source_manga(&manga),
                &to_source_chapters(&update.chapters),
            )?;
            Ok(RefreshDto {
                detail: detail_of(state, id)?,
                chapters_added: u32::try_from(added).unwrap_or(u32::MAX),
                error: None,
            })
        }
        Err(e) => match state.db.manga_by_url(url, source)? {
            Some(row) => Ok(RefreshDto {
                detail: detail_of(state, row.id)?,
                chapters_added: 0,
                error: Some(format!("Could not load chapters: {e}")),
            }),
            None => Err(e.into()),
        },
    }
}

/// Opens a manga from Browse: fetch, store (not favourite) and return its detail.
#[tauri::command]
pub async fn online_open(
    state: State<'_, AppState>,
    source_id: String,
    url: String,
    title: String,
) -> R<RefreshDto> {
    let source = parse_source(&source_id)?;
    refresh(&state, source, &url, &title).await
}

/// Refreshes a stored online manga from its source (pull to refresh on the detail screen).
#[tauri::command]
pub async fn manga_refresh(state: State<'_, AppState>, id: i64) -> R<RefreshDto> {
    let m = manga_or_404(&state, id)?;
    if m.is_local() {
        return Err(CmdError::new("invalid", "Local manga refresh with Rescan."));
    }
    refresh(&state, m.source, &m.url, &m.title).await
}

#[tauri::command]
pub fn library_set_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> R<MangaDto> {
    manga_or_404(&state, id)?;
    state.db.set_favorite(id, favorite)?;
    crate::changes::library(&state, id);
    Ok(MangaDto::from(&manga_or_404(&state, id)?))
}

/// Refreshes every online manga in the library from its source, one at a time, reporting progress
/// as `library-update` events (port of LibraryUpdater). Returns how many titles it will refresh.
#[tauri::command]
pub fn library_update(app: tauri::AppHandle, state: State<'_, AppState>) -> R<u32> {
    use std::sync::atomic::Ordering;
    let todo: Vec<Manga> = state
        .db
        .library()?
        .into_iter()
        .filter(|m| !m.is_local())
        .collect();
    let total = u32::try_from(todo.len()).unwrap_or(u32::MAX);
    if todo.is_empty() {
        return Ok(0);
    }
    if state.updating.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new("busy", "The library is already updating."));
    }
    tauri::async_runtime::spawn(async move {
        use tauri::{Emitter, Manager};
        let state = app.state::<AppState>();
        let mut p = LibraryUpdateDto {
            running: true,
            done: 0,
            total,
            current: None,
            chapters_added: 0,
            failures: Vec::new(),
        };
        for m in &todo {
            p.current = Some(m.title.clone());
            let _ = app.emit("library-update", &p);
            match refresh(&state, m.source, &m.url, &m.title).await {
                Ok(r) => {
                    p.chapters_added = p.chapters_added.saturating_add(r.chapters_added);
                    if let Some(e) = r.error {
                        p.failures.push(format!("{}: {e}", m.title));
                    }
                }
                Err(e) => p.failures.push(format!("{}: {}", m.title, e.message)),
            }
            p.done = p.done.saturating_add(1);
        }
        p.running = false;
        p.current = None;
        state.updating.store(false, Ordering::Release);
        let _ = app.emit("library-update", &p);
    });
    Ok(total)
}

fn manga_or_404(state: &AppState, id: i64) -> R<Manga> {
    state
        .db
        .manga(id)?
        .ok_or_else(|| CmdError::not_found("That manga"))
}

// ---- source settings -----------------------------------------------------------------------

#[tauri::command]
pub async fn source_preferences(
    state: State<'_, AppState>,
    source_id: String,
) -> R<Vec<PreferenceDto>> {
    let source = parse_source(&source_id)?;
    let prefs = client(&state)?.preferences(source).await?;
    Ok(prefs.into_iter().map(PreferenceDto::from).collect())
}

#[tauri::command]
pub async fn source_preference_set(
    state: State<'_, AppState>,
    source_id: String,
    key: String,
    value: serde_json::Value,
) -> R<PreferenceDto> {
    let source = parse_source(&source_id)?;
    let pref = client(&state)?.set_preference(source, &key, value).await?;
    Ok(PreferenceDto::from(pref))
}

// ---- reading -------------------------------------------------------------------------------

/// Lists a chapter's pages through its source and remembers them for `tm://page`.
pub async fn load_online_chapter(
    state: &AppState,
    manga: &Manga,
    ch: &Chapter,
) -> R<OnlineChapter> {
    let c = client(state)?;
    let req = model::Chapter {
        url: ch.url.clone(),
        name: ch.name.clone(),
        date_upload: ch.date_upload,
        chapter_number: {
            // Chapter numbers are small; the f32 the API uses holds them.
            #[allow(clippy::cast_possible_truncation)]
            let n = ch.chapter_number as f32;
            n
        },
        scanlator: ch.scanlator.clone(),
        memo: Default::default(),
    };
    let pages = c.pages(manga.source, &req).await?;
    if pages.len() > MAX_PAGES {
        return Err(CmdError::new(
            "source",
            "This chapter has too many pages to open.",
        ));
    }
    let open = OnlineChapter {
        source: manga.source,
        chapter_url: ch.url.clone(),
        pages,
    };
    let mut table = state.online()?;
    if table.len() >= MAX_OPEN_CHAPTERS {
        table.clear();
    }
    table.insert(ch.id, open.clone());
    Ok(open)
}

pub async fn chapter_pages(state: &AppState, manga: &Manga, ch: &Chapter) -> R<ChapterPagesDto> {
    let open = match load_online_chapter(state, manga, ch).await {
        Ok(open) => open,
        Err(first) => {
            // Sources move chapters (a new domain, a new url format), and some extensions then
            // refuse the old url with "Refresh Chapter List". Refresh the chapter list once,
            // which moves the stored chapter to its new url, and try again.
            tracing::info!(
                "pages of {} failed ({}); refreshing the chapter list",
                ch.url,
                first.message
            );
            let refreshed = matches!(
                refresh(state, manga.source, &manga.url, &manga.title).await,
                Ok(RefreshDto { error: None, .. })
            );
            if !refreshed {
                return Err(first);
            }
            match state.db.chapter(ch.id)? {
                Some(now) if now.url != ch.url => load_online_chapter(state, manga, &now).await?,
                Some(_) => return Err(first),
                None => {
                    return Err(CmdError::new(
                        "source",
                        "The source no longer lists this chapter. Go back and pick it from the refreshed list.",
                    ))
                }
            }
        }
    };
    Ok(ChapterPagesDto {
        chapter_id: ch.id,
        // Sizes are unknown until an image loads; the strip corrects heights as pages arrive.
        pages: open
            .pages
            .iter()
            .map(|_| PageDto { w: None, h: None })
            .collect(),
    })
}

fn cache_key(kind: &str, source: i64, a: &str, b: &str) -> String {
    format!("online:{kind}:{source}:{a}:{b}")
}

/// Image bytes of an online page, from the page cache or the source. Called on the protocol
/// thread (no async runtime there).
pub fn page_bytes(state: &AppState, chapter_id: i64, index: usize) -> R<Vec<u8>> {
    let known = state.online()?.get(&chapter_id).cloned();
    let open = match known {
        Some(open) => open,
        None => {
            let ch = crate::commands::chapter_or_404(state, chapter_id)?;
            let manga = manga_or_404(state, ch.manga_id)?;
            tauri::async_runtime::block_on(load_online_chapter(state, &manga, &ch))?
        }
    };
    let page = open
        .pages
        .get(index)
        .ok_or_else(|| CmdError::not_found("That page"))?;
    let key = cache_key("page", open.source, &open.chapter_url, &index.to_string());
    if let Ok(Some(bytes)) = state.cache.get(&key) {
        return Ok(bytes);
    }
    let c = client(state)?;
    let img = tauri::async_runtime::block_on(c.page_image(open.source, page))?;
    if let Err(e) = state.cache.put(&key, &img.bytes) {
        tracing::warn!("page cache write failed: {e}");
    }
    Ok(img.bytes)
}

/// A cover (or any image a source lists) fetched with that source's client and headers.
pub fn cover_bytes(state: &AppState, source: i64, url: &str) -> R<Vec<u8>> {
    if url.is_empty() {
        return Err(CmdError::not_found("That cover"));
    }
    let key = cache_key("cover", source, url, "");
    if let Ok(Some(bytes)) = state.cache.get(&key) {
        return Ok(bytes);
    }
    let c = client(state)?;
    let img = tauri::async_runtime::block_on(c.image_url(source, url))?;
    if let Err(e) = state.cache.put(&key, &img.bytes) {
        tracing::warn!("cover cache write failed: {e}");
    }
    Ok(img.bytes)
}

#[cfg(all(test, unix))]
mod idle_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    /// A stand-in `java` that prints the ready line and waits for stdin to close.
    fn fake_host(dir: &std::path::Path) -> ExtHost {
        let java = dir.join("java");
        std::fs::write(
            &java,
            "#!/bin/sh\necho 'TM_EXTHOST_READY port=4100'\ncat > /dev/null\n",
        )
        .unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("ext-host.jar"), b"").unwrap();
        ExtHost::spawn(&HostConfig {
            host_dir: dir.into(),
            data_dir: dir.into(),
            java: Some(java),
        })
        .unwrap()
    }

    #[test]
    fn idle_host_is_stopped_but_not_during_an_update() {
        let tmp = tempfile::tempdir().unwrap();
        let state =
            AppState::open(tm_core::AppPaths::at(tmp.path().join("data")).unwrap()).unwrap();
        assert!(!stop_if_idle(&state, Duration::ZERO), "nothing running");

        *state.exthost.lock().unwrap() = Some(fake_host(tmp.path()));
        state.touch_host();
        assert!(
            !stop_if_idle(&state, Duration::from_secs(600)),
            "used just now"
        );

        state.updating.store(true, Ordering::Relaxed);
        assert!(!stop_if_idle(&state, Duration::ZERO), "an update keeps it");
        state.updating.store(false, Ordering::Relaxed);

        assert!(stop_if_idle(&state, Duration::ZERO));
        assert!(state.exthost.lock().unwrap().is_none());
    }
}
