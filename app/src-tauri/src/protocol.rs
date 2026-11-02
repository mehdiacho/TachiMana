//! The `tm://` URI scheme (plan §2.1). Images reach the webview as plain URLs, never through IPC:
//! - `tm://cover/<mangaId>`
//! - `tm://page/<chapterId>/<index>`
//!
//! Windows' webview sees these as `http://tm.localhost/<kind>/...`; both forms are accepted.

use std::path::Path;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::commands::resolve_chapter;
use crate::state::AppState;

/// Covers are small; refuse anything larger than this as a cover file.
const MAX_COVER_BYTES: u64 = 20 * 1024 * 1024;

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let uri = request.uri().clone();
    // Archive reads are blocking file IO; keep them off the webview thread.
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let segments = segments(uri.host(), uri.path());
        let seg: Vec<&str> = segments.iter().map(String::as_str).collect();
        let res = match seg.as_slice() {
            ["cover", id] => id.parse().ok().map(|id| cover(&state, id)),
            ["page", ch, idx] => match (ch.parse(), idx.parse()) {
                (Ok(ch), Ok(idx)) => Some(page(&state, ch, idx)),
                _ => None,
            },
            _ => None,
        };
        responder.respond(res.unwrap_or_else(|| status(StatusCode::NOT_FOUND)));
    });
}

/// Path segments of a `tm://kind/a/b` or `http://tm.localhost/kind/a/b` URL.
pub fn segments(host: Option<&str>, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    match host {
        Some(h) if !h.is_empty() && h != "tm.localhost" && h != "localhost" => {
            out.push(h.to_owned())
        }
        _ => {}
    }
    out.extend(path.split('/').filter(|s| !s.is_empty()).map(|s| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    }));
    out
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = code;
    r
}

/// Content type from the magic bytes (as `DownloadManager.kt` does), not the file name.
pub fn sniff(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f', ..] => "image/avif",
        [b'B', b'M', ..] => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn image(bytes: Vec<u8>, immutable: bool) -> Response<Vec<u8>> {
    let cache = if immutable {
        "max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, sniff(&bytes))
        .header(header::CACHE_CONTROL, cache)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(bytes)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

fn cover(state: &AppState, manga_id: i64) -> Response<Vec<u8>> {
    let Ok(Some(manga)) = state.db.manga(manga_id) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(thumb) = manga.thumbnail_url.as_deref() else {
        return status(StatusCode::NOT_FOUND);
    };
    // Local covers are files under the local folder; online covers come through the ext-host (P5).
    if !manga.is_local() {
        return status(StatusCode::NOT_FOUND);
    }
    let root = state.paths.local_library_dir();
    let path = Path::new(thumb);
    let inside = match (path.canonicalize(), root.canonicalize()) {
        (Ok(p), Ok(r)) => p.starts_with(r),
        _ => false,
    };
    if !inside {
        return status(StatusCode::FORBIDDEN);
    }
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() <= MAX_COVER_BYTES => match std::fs::read(path) {
            Ok(bytes) => image(bytes, false),
            Err(_) => status(StatusCode::NOT_FOUND),
        },
        _ => status(StatusCode::NOT_FOUND),
    }
}

fn page(state: &AppState, chapter_id: i64, index: usize) -> Response<Vec<u8>> {
    let Ok(open) = resolve_chapter(state, chapter_id) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(name) = open.names.get(index) else {
        return status(StatusCode::NOT_FOUND);
    };
    match tm_archive::read_page(&open.path, name) {
        Ok(bytes) => image(bytes, true),
        Err(e) => {
            tracing::warn!("page {chapter_id}/{index}: {e}");
            status(StatusCode::NOT_FOUND)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_url_forms_parse_the_same() {
        assert_eq!(segments(Some("page"), "/12/3"), ["page", "12", "3"]);
        assert_eq!(
            segments(Some("tm.localhost"), "/page/12/3"),
            ["page", "12", "3"]
        );
        assert_eq!(segments(Some("cover"), "/7"), ["cover", "7"]);
        assert_eq!(segments(None, "/cover/7"), ["cover", "7"]);
    }

    #[test]
    fn sniffs_magic_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff(b"\x89PNG\r\n"), "image/png");
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(sniff(b""), "application/octet-stream");
    }
}
