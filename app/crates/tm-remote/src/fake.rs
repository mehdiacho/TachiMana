//! A small in-memory WebDAV server for tests: PROPFIND (depth 0/1), MKCOL, PUT, GET, HEAD,
//! DELETE and MOVE, optional Basic auth, and a switch that stores short files to fake a broken
//! upload.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};

#[derive(Default)]
pub struct Dav {
    pub files: Mutex<BTreeMap<String, Vec<u8>>>,
    pub dirs: Mutex<BTreeSet<String>>,
    /// Expected `Authorization` header, when set.
    pub auth: Mutex<Option<String>>,
    /// Store one byte less than sent.
    pub short_puts: AtomicBool,
    pub log: Mutex<Vec<String>>,
}

pub struct Server {
    pub base: String,
    pub dav: Arc<Dav>,
}

fn decode(p: &str) -> String {
    let d = percent_decode_str(p).decode_utf8_lossy().into_owned();
    let t = d.trim_end_matches('/');
    if t.is_empty() {
        "/".into()
    } else {
        t.into()
    }
}

fn encode(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn parent(p: &str) -> String {
    match p.rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((a, _)) => a.into(),
    }
}

fn status(code: StatusCode) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

fn entry(path: &str, dir: bool, len: usize) -> String {
    let href = if dir {
        format!("{}/", encode(path))
    } else {
        encode(path)
    };
    let prop = if dir {
        "<D:resourcetype><D:collection/></D:resourcetype>".to_string()
    } else {
        format!("<D:resourcetype/><D:getcontentlength>{len}</D:getcontentlength>")
    };
    format!("<D:response><D:href>{href}</D:href><D:propstat><D:prop>{prop}</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>")
}

async fn handle(
    State(dav): State<Arc<Dav>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = decode(uri.path());
    dav.log.lock().unwrap().push(format!("{method} {path}"));
    if let Some(want) = dav.auth.lock().unwrap().clone() {
        if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(want.as_str()) {
            return status(StatusCode::UNAUTHORIZED);
        }
    }
    let mut files = dav.files.lock().unwrap();
    let mut dirs = dav.dirs.lock().unwrap();
    match method.as_str() {
        "PROPFIND" => {
            let depth = headers
                .get("depth")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("1")
                .to_owned();
            let mut out = String::from(
                r#"<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:">"#,
            );
            if let Some(f) = files.get(&path) {
                out.push_str(&entry(&path, false, f.len()));
            } else if dirs.contains(&path) {
                out.push_str(&entry(&path, true, 0));
                if depth != "0" {
                    let prefix = format!("{path}/");
                    for d in dirs
                        .iter()
                        .filter(|d| d.starts_with(&prefix) && !d[prefix.len()..].contains('/'))
                    {
                        out.push_str(&entry(d, true, 0));
                    }
                    for (f, b) in files
                        .iter()
                        .filter(|(f, _)| f.starts_with(&prefix) && !f[prefix.len()..].contains('/'))
                    {
                        out.push_str(&entry(f, false, b.len()));
                    }
                }
            } else {
                return status(StatusCode::NOT_FOUND);
            }
            out.push_str("</D:multistatus>");
            Response::builder()
                .status(207)
                .header("content-type", "application/xml; charset=utf-8")
                .body(Body::from(out))
                .unwrap()
        }
        "MKCOL" => {
            if dirs.contains(&path) || files.contains_key(&path) {
                status(StatusCode::METHOD_NOT_ALLOWED)
            } else if !dirs.contains(&parent(&path)) {
                status(StatusCode::CONFLICT)
            } else {
                dirs.insert(path);
                status(StatusCode::CREATED)
            }
        }
        "PUT" => {
            if !dirs.contains(&parent(&path)) {
                return status(StatusCode::CONFLICT);
            }
            let mut b = body.to_vec();
            if dav.short_puts.load(Ordering::Relaxed) {
                b.pop();
            }
            files.insert(path, b);
            status(StatusCode::CREATED)
        }
        "HEAD" | "GET" => match files.get(&path) {
            Some(b) => {
                let r = Response::builder()
                    .status(200)
                    .header("content-length", b.len());
                if method == Method::HEAD {
                    r.body(Body::empty()).unwrap()
                } else {
                    r.body(Body::from(b.clone())).unwrap()
                }
            }
            None => status(StatusCode::NOT_FOUND),
        },
        "DELETE" => {
            let prefix = format!("{path}/");
            let had = files.remove(&path).is_some() | dirs.remove(&path);
            files.retain(|k, _| !k.starts_with(&prefix));
            dirs.retain(|k| !k.starts_with(&prefix));
            status(if had {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::NOT_FOUND
            })
        }
        "MOVE" => {
            let dest = headers
                .get("destination")
                .and_then(|v| v.to_str().ok())
                .and_then(|d| d.parse::<Uri>().ok());
            let Some(dest) = dest else {
                return status(StatusCode::BAD_REQUEST);
            };
            let to = decode(dest.path());
            match files.remove(&path) {
                Some(b) => {
                    files.insert(to, b);
                    status(StatusCode::CREATED)
                }
                None => status(StatusCode::NOT_FOUND),
            }
        }
        _ => status(StatusCode::METHOD_NOT_ALLOWED),
    }
}

/// Starts the server on a free port with `/webdav` as an existing folder.
pub async fn start() -> Server {
    start_on(0).await
}

/// Starts the server on `port` (0 = any free port).
pub async fn start_on(port: u16) -> Server {
    let dav = Arc::new(Dav::default());
    dav.dirs
        .lock()
        .unwrap()
        .extend(["/".to_string(), "/webdav".to_string()]);
    let app = axum::Router::new().fallback(handle).with_state(dav.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Server {
        base: format!("http://{addr}/webdav"),
        dav,
    }
}

impl Server {
    pub fn put_file(&self, path: &str, bytes: &[u8]) {
        let mut dirs = self.dav.dirs.lock().unwrap();
        let mut p = parent(path);
        while p != "/" {
            dirs.insert(p.clone());
            p = parent(&p);
        }
        self.dav
            .files
            .lock()
            .unwrap()
            .insert(path.to_string(), bytes.to_vec());
    }
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.dav.files.lock().unwrap().get(path).cloned()
    }
}
