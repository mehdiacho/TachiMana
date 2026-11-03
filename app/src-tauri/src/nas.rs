//! NAS storage in the app (PLAN §P6.3/P6.5): settings and test connection, reading chapters from the
//! NAS mirror, and uploading finished downloads. The heavy lifting is in `tm-remote`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tm_core::{settings::keys, AppPaths, Settings};
use tm_db::{Chapter, Manga};
use tm_remote::mirror::ChapterRef;
use tm_remote::{names, Lookup, Mirror, RemoteIndex, Role, UploadQueue, WebDav, WebDavConfig};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// NAS state that lives as long as the app.
pub struct Nas {
    pub index: RemoteIndex,
    pub uploads: UploadQueue,
    /// Source id → Mihon's folder label (`Name (LANG)`), remembered from the extension host so the
    /// NAS works without starting it.
    source_names: Mutex<HashMap<i64, String>>,
    names_file: PathBuf,
    /// Set while the upload worker runs.
    uploading: AtomicBool,
}

impl Nas {
    pub fn open(paths: &AppPaths) -> Self {
        let dir = paths.data_dir();
        let names_file = dir.join("source_names.json");
        let names = std::fs::read(&names_file)
            .ok()
            .and_then(|b| serde_json::from_slice::<HashMap<String, String>>(&b).ok())
            .map(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            index: RemoteIndex::open(dir.join("remote_index.json")),
            uploads: UploadQueue::open(dir.join("uploads.json"), paths.downloads_root()),
            source_names: Mutex::new(names),
            names_file,
            uploading: AtomicBool::new(false),
        }
    }

    fn source_name(&self, id: i64) -> Option<String> {
        self.source_names.lock().ok()?.get(&id).cloned()
    }

    /// Known source names without the language suffix, for a backup's sources section.
    pub fn source_names(&self) -> HashMap<i64, String> {
        let Ok(m) = self.source_names.lock() else {
            return HashMap::new();
        };
        m.iter()
            .map(|(id, label)| {
                let name = label
                    .rsplit_once(" (")
                    .filter(|(_, rest)| rest.ends_with(')'))
                    .map_or(label.as_str(), |(n, _)| n);
                (*id, name.to_owned())
            })
            .collect()
    }

    /// Remembers source labels (`Name (LANG)`, as Mihon's `toString`) for NAS folder names.
    pub fn remember_sources(&self, sources: impl IntoIterator<Item = (i64, String)>) {
        let Ok(mut m) = self.source_names.lock() else {
            return;
        };
        let mut changed = false;
        for (id, label) in sources {
            if m.get(&id) != Some(&label) {
                m.insert(id, label);
                changed = true;
            }
        }
        if changed {
            let out: HashMap<String, &String> = m.iter().map(|(k, v)| (k.to_string(), v)).collect();
            if let Ok(body) = serde_json::to_vec(&out) {
                if let Err(e) = std::fs::write(&self.names_file, body) {
                    tracing::warn!("source names save failed: {e}");
                }
            }
        }
    }
}

/// Mihon's `HttpSource.toString()`.
pub fn source_label(name: &str, lang: &str) -> String {
    format!("{name} ({})", lang.to_uppercase())
}

fn config_of(s: &Settings) -> WebDavConfig {
    WebDavConfig {
        url: s.remote_collection_url(),
        username: s.string(keys::REMOTE_USERNAME),
        password: s.string(keys::REMOTE_PASSWORD),
    }
}

/// The client, when NAS storage is on and configured.
pub fn dav(state: &AppState) -> Option<WebDav> {
    let s = state.settings.lock().ok()?;
    if !s.remote_enabled() {
        return None;
    }
    match WebDav::new(&config_of(&s)) {
        Ok(d) => Some(d),
        Err(e) => {
            tracing::warn!("NAS settings are not usable: {e}");
            None
        }
    }
}

fn mirror(state: &AppState) -> Mirror {
    let mb = state
        .settings
        .lock()
        .map(|s| s.i64(keys::REMOTE_CACHE_MB))
        .unwrap_or(2048);
    Mirror::new(
        state.paths.remote_cache_dir(),
        u64::try_from(mb.max(64)).unwrap_or(2048),
    )
}

fn role(state: &AppState) -> Role {
    match state
        .settings
        .lock()
        .map(|s| s.string(keys::REMOTE_ROLE))
        .as_deref()
    {
        Ok("archive") => Role::Archive,
        _ => Role::Mirror,
    }
}

fn chapter_ref<'a>(name: Option<&'a str>, manga: &'a Manga, ch: &'a Chapter) -> ChapterRef<'a> {
    ChapterRef {
        source_name: name,
        source_id: manga.source,
        manga_title: &manga.title,
        chapter_name: &ch.name,
        scanlator: ch.scanlator.as_deref(),
        chapter_url: &ch.url,
    }
}

fn enabled(state: &AppState) -> bool {
    state.settings.lock().is_ok_and(|s| s.remote_enabled())
}

/// Whether the NAS holds this chapter, from the saved listing only (no network). Local manga and
/// NAS storage switched off are never "on the NAS".
pub fn holds(state: &AppState, manga: &Manga, ch: &Chapter) -> bool {
    if manga.is_local() || !enabled(state) {
        return false;
    }
    let name = state.nas.source_name(manga.source);
    mirror(state).holds(&state.nas.index, &chapter_ref(name.as_deref(), manga, ch))
}

/// Lists the manga's NAS folder when it has not been listed yet, so the chapter list can mark what
/// the NAS holds (`RemoteMirror.holds` in the Kotlin app). Returns whether a listing was made.
#[tauri::command]
pub async fn nas_list_manga(state: State<'_, AppState>, id: i64) -> R<bool> {
    list_manga(&state, id).await
}

async fn list_manga(state: &AppState, id: i64) -> R<bool> {
    let Some(manga) = state.db.manga(id)? else {
        return Ok(false);
    };
    if manga.is_local() {
        return Ok(false);
    }
    let (Some(dav), Some(src)) = (dav(state), state.nas.source_name(manga.source)) else {
        return Ok(false);
    };
    let (sd, md) = (
        names::source_dir_name(&src),
        names::manga_dir_name(&manga.title),
    );
    if state.nas.index.count_for(&sd, &md).is_some() {
        return Ok(false);
    }
    match state.nas.index.refresh(&dav, &sd, &md).await {
        Ok(_) => Ok(true),
        Err(e) => {
            // Not there yet, or the NAS is unreachable: no marks, and reading falls back as usual.
            tracing::info!("NAS listing for {} failed: {e}", manga.title);
            Ok(false)
        }
    }
}

/// The chapter's archive from the NAS, fetched into the mirror cache when needed. `None` when NAS
/// storage is off, the NAS does not have it, or it can't be reached (logged).
pub async fn fetch_chapter(state: &AppState, manga: &Manga, ch: &Chapter) -> Option<PathBuf> {
    if manga.is_local() {
        return None;
    }
    let dav = dav(state)?;
    let name = state.nas.source_name(manga.source);
    let c = chapter_ref(name.as_deref(), manga, ch);
    let m = mirror(state);
    // List the manga's folder once so the index can answer.
    if let Some(src) = &name {
        let (sd, md) = (
            names::source_dir_name(src),
            names::manga_dir_name(&manga.title),
        );
        let cands = names::chapter_file_names(&ch.name, ch.scanlator.as_deref(), &ch.url);
        if state.nas.index.resolve(&sd, &md, &cands) == Lookup::Unknown {
            if let Err(e) = state.nas.index.refresh(&dav, &sd, &md).await {
                tracing::info!("NAS listing failed: {e}");
                return None;
            }
        }
    }
    let segments = m.segments_for(&state.nas.index, &c)?;
    match m.fetch(&dav, &segments, |_, _| ()).await {
        Ok(found) => found,
        Err(e) => {
            tracing::info!("NAS fetch failed: {e}");
            None
        }
    }
}

/// Queue the upload of a finished download, unless NAS storage is off or the NAS has it already.
pub fn upload_finished(
    app: &AppHandle,
    source: i64,
    manga_url: &str,
    chapter_url: &str,
    dir: PathBuf,
) {
    let state = app.state::<AppState>();
    if !enabled(&state) {
        return;
    }
    let Some(name) = state.nas.source_name(source) else {
        tracing::info!("not uploading: source {source} has no known name yet");
        return;
    };
    let Ok(Some(manga)) = state.db.manga_by_url(manga_url, source) else {
        return;
    };
    let Ok(chapters) = state.db.chapters(manga.id) else {
        return;
    };
    let Some(ch) = chapters.iter().find(|c| c.url == chapter_url) else {
        return;
    };
    let (sd, md) = (
        names::source_dir_name(&name),
        names::manga_dir_name(&manga.title),
    );
    let cands = names::chapter_file_names(&ch.name, ch.scanlator.as_deref(), &ch.url);
    if matches!(state.nas.index.resolve(&sd, &md, &cands), Lookup::Found(_)) {
        return;
    }
    let Some(file) = cands.into_iter().next() else {
        return;
    };
    let label = format!("{} · {}", manga.title, ch.name);
    if state
        .nas
        .uploads
        .enqueue(dir, vec![sd, md, file], role(&state), label)
    {
        emit(app);
        kick(app);
    }
}

/// Runs queued uploads one after another until none are left.
pub fn kick(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.nas.uploads.has_queued() || state.nas.uploading.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        while let Some(dav) = dav(&state) {
            if state
                .nas
                .uploads
                .run_next(&dav, &state.nas.index)
                .await
                .is_none()
            {
                break;
            }
            emit(&app);
            // An archived chapter left the disk; let the UI drop its downloaded mark.
            let _ = app.emit("downloads-changed", ());
        }
        state.nas.uploading.store(false, Ordering::Release);
    });
}

fn emit(app: &AppHandle) {
    let jobs: Vec<UploadJobDto> = app
        .state::<AppState>()
        .nas
        .uploads
        .jobs()
        .iter()
        .map(UploadJobDto::from)
        .collect();
    let _ = app.emit("uploads", jobs);
}

// ---- DTOs and commands ----

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NasSettingsDto {
    pub enabled: bool,
    pub host: String,
    pub port: String,
    pub use_https: bool,
    pub folder: String,
    pub username: String,
    /// The password is never sent to the UI; only whether one is saved.
    pub password_set: bool,
    /// `mirror` or `archive`.
    pub role: String,
    #[ts(type = "number")]
    pub cache_mb: i64,
    pub redownload_from_remote: bool,
    /// The folder address the settings make, for display.
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NasSettingsPatch {
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[ts(optional)]
    pub host: Option<String>,
    #[ts(optional)]
    pub port: Option<String>,
    #[ts(optional)]
    pub use_https: Option<bool>,
    #[ts(optional)]
    pub folder: Option<String>,
    #[ts(optional)]
    pub username: Option<String>,
    /// A new password; an empty string clears it.
    #[ts(optional)]
    pub password: Option<String>,
    #[ts(optional)]
    pub role: Option<String>,
    #[ts(optional, as = "Option<f64>")]
    pub cache_mb: Option<i64>,
    #[ts(optional)]
    pub redownload_from_remote: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadJobDto {
    #[ts(type = "number")]
    pub id: u64,
    pub label: String,
    /// Path on the NAS.
    pub path: String,
    /// `QUEUED`, `UPLOADING`, `DONE` or `FAILED`.
    pub status: String,
    pub error: Option<String>,
    pub archive: bool,
}

impl From<&tm_remote::UploadJob> for UploadJobDto {
    fn from(j: &tm_remote::UploadJob) -> Self {
        let status = match j.status {
            tm_remote::UploadStatus::Queued => "QUEUED",
            tm_remote::UploadStatus::Uploading => "UPLOADING",
            tm_remote::UploadStatus::Done => "DONE",
            tm_remote::UploadStatus::Failed => "FAILED",
        };
        Self {
            id: j.id,
            label: j.label.clone(),
            path: j.segments.join("/"),
            status: status.into(),
            error: j.error.clone(),
            archive: j.role == Role::Archive,
        }
    }
}

fn settings_dto(s: &Settings) -> NasSettingsDto {
    NasSettingsDto {
        enabled: s.bool(keys::REMOTE_ENABLED),
        host: s.string(keys::REMOTE_HOST),
        port: s.string(keys::REMOTE_PORT),
        use_https: s.bool(keys::REMOTE_USE_HTTPS),
        folder: s.string(keys::REMOTE_FOLDER),
        username: s.string(keys::REMOTE_USERNAME),
        password_set: !s.string(keys::REMOTE_PASSWORD).is_empty(),
        role: s.string(keys::REMOTE_ROLE),
        cache_mb: s.i64(keys::REMOTE_CACHE_MB),
        redownload_from_remote: s.bool(keys::REMOTE_REDOWNLOAD),
        url: s.remote_collection_url(),
    }
}

/// Longest text field accepted.
const MAX_FIELD: usize = 1024;

fn apply(s: &mut Settings, p: &NasSettingsPatch) -> R<()> {
    let texts = [
        (&p.host, keys::REMOTE_HOST),
        (&p.port, keys::REMOTE_PORT),
        (&p.folder, keys::REMOTE_FOLDER),
        (&p.username, keys::REMOTE_USERNAME),
        (&p.password, keys::REMOTE_PASSWORD),
    ];
    for (v, key) in texts {
        if let Some(v) = v {
            if v.len() > MAX_FIELD {
                return Err(CmdError::new("invalid", "That value is too long."));
            }
            s.set(key, v.as_str());
        }
    }
    if let Some(port) = p.port.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        if !port.parse::<u32>().is_ok_and(|n| (1..=65535).contains(&n)) {
            return Err(CmdError::new(
                "invalid",
                "The port must be a number from 1 to 65535.",
            ));
        }
    }
    if let Some(r) = &p.role {
        if !matches!(r.as_str(), "mirror" | "archive") {
            return Err(CmdError::new("invalid", "Unknown storage role."));
        }
        s.set(keys::REMOTE_ROLE, r.as_str());
    }
    for (v, key) in [
        (p.enabled, keys::REMOTE_ENABLED),
        (p.use_https, keys::REMOTE_USE_HTTPS),
        (p.redownload_from_remote, keys::REMOTE_REDOWNLOAD),
    ] {
        if let Some(v) = v {
            s.set(key, v);
        }
    }
    if let Some(mb) = p.cache_mb {
        s.set(keys::REMOTE_CACHE_MB, mb.clamp(256, 1024 * 1024));
    }
    Ok(())
}

#[tauri::command]
pub fn nas_settings_get(state: State<'_, AppState>) -> R<NasSettingsDto> {
    Ok(settings_dto(&*state.settings()?))
}

#[tauri::command]
pub fn nas_settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: NasSettingsPatch,
) -> R<NasSettingsDto> {
    let (dto, moved) = {
        let mut s = state.settings()?;
        let before = s.remote_collection_url();
        apply(&mut s, &patch)?;
        s.save()?;
        (settings_dto(&s), before != s.remote_collection_url())
    };
    if moved {
        // A different server or folder: what we knew about the old one no longer holds.
        state.nas.index.invalidate();
    }
    kick(&app);
    Ok(dto)
}

/// Tries the settings in `patch` (over the saved ones) without saving them.
#[tauri::command]
pub async fn nas_test(state: State<'_, AppState>, patch: NasSettingsPatch) -> R<String> {
    let cfg = {
        let mut s = state.settings()?.clone();
        apply(&mut s, &patch)?;
        if s.remote_collection_url().is_empty() {
            return Err(CmdError::new("invalid", "Enter the NAS address first."));
        }
        config_of(&s)
    };
    let dav = WebDav::new(&cfg).map_err(|e| CmdError::new("invalid", e.to_string()))?;
    dav.test_connection()
        .await
        .map_err(|e| CmdError::new("remote", e.to_string()))?;
    Ok(format!("Connected to {}", dav.base()))
}

#[tauri::command]
pub fn nas_uploads(state: State<'_, AppState>) -> R<Vec<UploadJobDto>> {
    Ok(state
        .nas
        .uploads
        .jobs()
        .iter()
        .map(UploadJobDto::from)
        .collect())
}

#[tauri::command]
pub fn nas_uploads_retry(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    state.nas.uploads.retry_failed();
    emit(&app);
    kick(&app);
    Ok(())
}

#[tauri::command]
pub fn nas_upload_dismiss(app: AppHandle, state: State<'_, AppState>, id: u64) -> R<()> {
    state.nas.uploads.dismiss(id);
    emit(&app);
    Ok(())
}

#[tauri::command]
pub fn nas_uploads_clear_finished(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    state.nas.uploads.clear_finished();
    emit(&app);
    Ok(())
}

/// Bytes in the NAS read cache.
#[tauri::command]
pub fn nas_cache_usage(state: State<'_, AppState>) -> R<f64> {
    Ok(dir_bytes(&state.paths.remote_cache_dir()) as f64)
}

#[tauri::command]
pub fn nas_cache_clear(state: State<'_, AppState>) -> R<f64> {
    mirror(&state).clear();
    Ok(dir_bytes(&state.paths.remote_cache_dir()) as f64)
}

#[allow(clippy::cast_precision_loss)]
fn dir_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .fold(0, u64::saturating_add)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;
    use tm_db::{SourceChapter, SourceManga};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

    #[test]
    fn an_online_chapter_on_the_nas_is_read_from_it() {
        tauri::async_runtime::block_on(async {
            let srv = tm_remote::fake::start().await;
            let tmp = tempfile::tempdir().unwrap();
            let state = AppState::open(AppPaths::at(tmp.path().join("data")).unwrap()).unwrap();
            let port = srv
                .base
                .trim_start_matches("http://127.0.0.1:")
                .trim_end_matches("/webdav");
            {
                let mut s = state.settings.lock().unwrap();
                apply(
                    &mut s,
                    &NasSettingsPatch {
                        enabled: Some(true),
                        host: Some("127.0.0.1".into()),
                        port: Some(port.into()),
                        use_https: Some(false),
                        folder: Some("webdav".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            let source = 2_499_283_573_021_220_255_i64;
            state
                .nas
                .remember_sources([(source, source_label("MangaDex", "en"))]);
            let (manga_id, _) = state
                .db
                .upsert_online(
                    source,
                    &SourceManga {
                        url: "/m/1".into(),
                        title: "Chainsaw Man".into(),
                        ..Default::default()
                    },
                    &[SourceChapter {
                        url: "/c/1".into(),
                        name: "Chapter 1".into(),
                        ..Default::default()
                    }],
                )
                .unwrap();
            let manga = state.db.manga(manga_id).unwrap().unwrap();
            let ch = state.db.chapters(manga_id).unwrap().remove(0);

            assert!(
                fetch_chapter(&state, &manga, &ch).await.is_none(),
                "not on the NAS yet"
            );

            let cbz = tmp.path().join("x.cbz");
            tm_archive::pack_cbz(
                &[
                    ("001.png".into(), PNG.to_vec()),
                    ("002.png".into(), PNG.to_vec()),
                ],
                &cbz,
            )
            .unwrap();
            let file = names::chapter_file_names("Chapter 1", None, "/c/1").remove(0);
            srv.put_file(
                &format!("/webdav/MangaDex (EN)/Chainsaw Man/{file}"),
                &std::fs::read(&cbz).unwrap(),
            );
            // The folder was listed (empty) a moment ago; a new listing finds the file.
            state.nas.index.invalidate();
            let on_nas = |s: &AppState| {
                crate::commands::detail_of(s, manga_id).unwrap().chapters[0].on_nas
            };
            assert!(!on_nas(&state), "not marked before the folder is listed");
            assert!(list_manga(&state, manga_id).await.unwrap(), "listed");
            assert!(on_nas(&state), "marked once listed");
            assert!(
                !list_manga(&state, manga_id).await.unwrap(),
                "an already listed folder is not listed again"
            );

            let got = fetch_chapter(&state, &manga, &ch)
                .await
                .expect("fetched from the NAS");
            assert!(got.starts_with(state.paths.remote_cache_dir()));
            crate::commands::remember_archive(&state, ch.id, got).unwrap();
            let open = crate::commands::resolve_chapter(&state, ch.id).unwrap();
            assert_eq!(open.names, ["001.png", "002.png"]);

            // Off means off.
            state
                .settings
                .lock()
                .unwrap()
                .set(keys::REMOTE_ENABLED, false);
            assert!(fetch_chapter(&state, &manga, &ch).await.is_none());
            assert!(!on_nas(&state), "no marks while NAS storage is off");
        });
    }

    #[test]
    fn settings_are_checked_and_the_password_is_never_returned() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Settings::load(&tmp.path().join("settings.json"));
        assert!(apply(
            &mut s,
            &NasSettingsPatch {
                port: Some("70000".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(apply(
            &mut s,
            &NasSettingsPatch {
                role: Some("both".into()),
                ..Default::default()
            }
        )
        .is_err());
        apply(
            &mut s,
            &NasSettingsPatch {
                password: Some("secret".into()),
                host: Some("nas.local".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let dto = settings_dto(&s);
        assert!(dto.password_set);
        assert!(!serde_json::to_string(&dto).unwrap().contains("secret"));
        assert_eq!(dto.url, "https://nas.local");
        assert_eq!(source_label("MangaDex", "en"), "MangaDex (EN)");
    }
}
