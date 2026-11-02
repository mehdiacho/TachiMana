//! WebDAV client, ported from `WebDavClient.kt`: PROPFIND (depth 0 and 1), HEAD, GET with
//! progress, PUT, MKCOL, DELETE and MOVE. A 404 means "absent"; any other failure is an error.

use std::path::Path;
use std::time::Duration;

use quick_xml::events::Event;
use reqwest::{Method, StatusCode};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::{Error, Result};

/// Longest PROPFIND reply read, so a broken server cannot exhaust memory.
const MAX_LISTING_BYTES: usize = 16 * 1024 * 1024;
/// Most entries kept from one listing.
const MAX_ENTRIES: usize = 100_000;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop><D:resourcetype/><D:getcontentlength/></D:prop>
</D:propfind>"#;

/// One child of a listed folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Where the NAS is and how to sign in.
#[derive(Debug, Clone, Default)]
pub struct WebDavConfig {
    /// Collection URL, e.g. `https://nas.local:5006/manga` (`remote_collection_url`).
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Clone)]
pub struct WebDav {
    http: reqwest::Client,
    base: Url,
    auth: Option<(String, String)>,
}

impl std::fmt::Debug for WebDav {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the password.
        f.debug_struct("WebDav")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

/// `normalize`: trims, drops a trailing slash and adds `http://` when no scheme is given.
pub fn normalize(raw: &str) -> Option<Url> {
    let t = raw.trim().trim_end_matches('/');
    if t.is_empty() {
        return None;
    }
    let with_scheme = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_owned()
    } else {
        format!("http://{t}")
    };
    let u = Url::parse(&with_scheme).ok()?;
    (u.has_host() && matches!(u.scheme(), "http" | "https")).then_some(u)
}

fn install_crypto() {
    // A second install fails harmlessly when another part of the app installed one first.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl WebDav {
    pub fn new(cfg: &WebDavConfig) -> Result<Self> {
        install_crypto();
        let base = normalize(&cfg.url)
            .ok_or_else(|| Error::Config(format!("Not a valid http(s) address: {}", cfg.url)))?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            // WebDAV servers answer MKCOL/PROPFIND on the same URL; never follow a redirect blindly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::Config(e.to_string()))?;
        let auth = (!cfg.username.is_empty()).then(|| (cfg.username.clone(), cfg.password.clone()));
        Ok(Self { http, base, auth })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// The URL of `segments` under the base, each segment escaped. Blank segments are skipped;
    /// `.` and `..` are refused so nothing can climb out of the folder.
    pub fn url_for(&self, segments: &[String]) -> Result<Url> {
        let mut u = self.base.clone();
        {
            let mut p = u
                .path_segments_mut()
                .map_err(|()| Error::Config("The address cannot hold a path".into()))?;
            p.pop_if_empty();
            for s in segments.iter().filter(|s| !s.trim().is_empty()) {
                if s == "." || s == ".." {
                    return Err(Error::Config(format!("Bad path segment {s:?}")));
                }
                p.push(s);
            }
        }
        Ok(u)
    }

    fn req(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        let r = self.http.request(method, url);
        match &self.auth {
            Some((u, p)) => r.basic_auth(u, Some(p)),
            None => r,
        }
    }

    fn propfind(&self, url: Url, depth: &str) -> reqwest::RequestBuilder {
        // PROPFIND is a valid token, so this cannot fail.
        let m = Method::from_bytes(b"PROPFIND").unwrap_or(Method::GET);
        self.req(m, url)
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
    }

    /// PROPFIND depth 0 on the folder: is the server there and are the credentials right?
    pub async fn test_connection(&self) -> Result<()> {
        let r = self.propfind(self.base.clone(), "0").send().await?;
        let code = r.status();
        if code.is_success() {
            return Ok(());
        }
        Err(Error::Server(match code.as_u16() {
            401 | 403 => format!("Sign-in was refused ({code})"),
            404 => "The folder was not found on the server (404)".into(),
            300..=399 => format!(
                "The server redirected to {}",
                r.headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("another address")
            ),
            _ => format!("The server answered {code}"),
        }))
    }

    /// Size of a file: `None` when absent (404). A reply without a length gives `Some(None)`.
    pub async fn size_of(&self, segments: &[String]) -> Result<Option<Option<u64>>> {
        let r = self
            .req(Method::HEAD, self.url_for(segments)?)
            .send()
            .await?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        check(&r, "HEAD", segments)?;
        let len = r
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        Ok(Some(len))
    }

    /// Children of a folder (depth 1). A missing folder lists as empty.
    pub async fn list(&self, segments: &[String]) -> Result<Vec<RemoteEntry>> {
        let url = self.url_for(segments)?;
        let mut r = self.propfind(url.clone(), "1").send().await?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        check(&r, "PROPFIND", segments)?;
        let mut body = Vec::new();
        while let Some(chunk) = r.chunk().await? {
            if body.len().saturating_add(chunk.len()) > MAX_LISTING_BYTES {
                return Err(Error::Server("The folder listing is too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        parse_multistatus(&body, url.path())
    }

    /// MKCOL each folder of `segments`, parents first. "Already there" (405) is fine.
    pub async fn create_collections(&self, segments: &[String]) -> Result<()> {
        let m = Method::from_bytes(b"MKCOL").unwrap_or(Method::PUT);
        for depth in 1..=segments.len() {
            let path = segments.get(..depth).unwrap_or(segments);
            let r = self.req(m.clone(), self.url_for(path)?).send().await?;
            let code = r.status();
            if code.is_success() || code == StatusCode::METHOD_NOT_ALLOWED {
                continue;
            }
            // Some servers answer 409/403 for an existing folder; trust a listing over the code.
            if self.list(path).await.is_err() {
                return Err(Error::Server(format!("MKCOL {} -> {code}", path.join("/"))));
            }
        }
        Ok(())
    }

    /// Upload a file, creating its folders first.
    pub async fn put_file(&self, segments: &[String], file: &Path) -> Result<()> {
        if let Some((_, parents)) = segments.split_last() {
            if !parents.is_empty() {
                self.create_collections(parents).await?;
            }
        }
        let bytes = tokio::fs::read(file)
            .await
            .map_err(|e| Error::Io(file.display().to_string(), e))?;
        let r = self
            .req(Method::PUT, self.url_for(segments)?)
            .body(bytes)
            .send()
            .await?;
        check(&r, "PUT", segments)
    }

    /// Download into `target`, calling `progress(read, total)`. `max_bytes` caps the size.
    pub async fn get_to(
        &self,
        segments: &[String],
        target: &Path,
        max_bytes: u64,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<u64> {
        let mut r = self
            .req(Method::GET, self.url_for(segments)?)
            .send()
            .await?;
        check(&r, "GET", segments)?;
        let total = r.content_length();
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Io(parent.display().to_string(), e))?;
        }
        let io = |e| Error::Io(target.display().to_string(), e);
        let mut out = tokio::fs::File::create(target).await.map_err(io)?;
        let mut read = 0u64;
        while let Some(chunk) = r.chunk().await? {
            read = read.saturating_add(chunk.len() as u64);
            if read > max_bytes {
                return Err(Error::Server("The file is larger than allowed".into()));
            }
            out.write_all(&chunk).await.map_err(io)?;
            progress(read, total);
        }
        out.flush().await.map_err(io)?;
        Ok(read)
    }

    /// Delete a file or folder. Already gone is fine.
    pub async fn delete(&self, segments: &[String]) -> Result<()> {
        let r = self
            .req(Method::DELETE, self.url_for(segments)?)
            .send()
            .await?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        check(&r, "DELETE", segments)
    }

    /// Move (rename), overwriting the target and creating its folders.
    pub async fn move_to(&self, from: &[String], to: &[String]) -> Result<()> {
        if let Some((_, parents)) = to.split_last() {
            if !parents.is_empty() {
                self.create_collections(parents).await?;
            }
        }
        let m = Method::from_bytes(b"MOVE").unwrap_or(Method::POST);
        let r = self
            .req(m, self.url_for(from)?)
            .header("Destination", self.url_for(to)?.as_str())
            .header("Overwrite", "T")
            .send()
            .await?;
        if r.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        check(&r, "MOVE", from)
    }
}

fn check(r: &reqwest::Response, what: &str, segments: &[String]) -> Result<()> {
    if r.status().is_success() {
        Ok(())
    } else {
        Err(Error::Server(format!(
            "{what} {} -> {}",
            segments.join("/"),
            r.status()
        )))
    }
}

/// Path part of an `href`, which may be a full URL or an absolute path.
fn href_path(href: &str) -> String {
    let path = match Url::parse(href) {
        Ok(u) if u.has_host() => u.path().to_owned(),
        _ => href.to_owned(),
    };
    percent_encoding::percent_decode_str(&path)
        .decode_utf8_lossy()
        .into_owned()
}

fn local(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// RFC 4918 `<multistatus>`: one entry per `<response>` except the folder itself.
pub fn parse_multistatus(xml: &[u8], self_path: &str) -> Result<Vec<RemoteEntry>> {
    let self_decoded = href_path(self_path);
    let self_trim = self_decoded.trim_end_matches('/');
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut out = Vec::new();
    let mut buf = Vec::new();
    let (mut href, mut size, mut is_dir) = (None::<String>, 0u64, false);
    // Text of the element being read (`href` or `getcontentlength`), across text and entity events.
    let mut field: Option<&'static str> = None;
    let mut text = String::new();
    loop {
        let ev = reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Server(format!("Bad folder listing: {e}")))?;
        match ev {
            Event::Start(e) => match local(e.local_name().as_ref()).as_str() {
                "response" => {
                    href = None;
                    size = 0;
                    is_dir = false;
                }
                "href" => {
                    field = Some("href");
                    text.clear();
                }
                "getcontentlength" => {
                    field = Some("len");
                    text.clear();
                }
                "collection" => is_dir = true,
                _ => {}
            },
            Event::Empty(e) => {
                if local(e.local_name().as_ref()) == "collection" {
                    is_dir = true;
                }
            }
            Event::Text(t) if field.is_some() => {
                text.push_str(&t.xml10_content());
            }
            Event::GeneralRef(r) if field.is_some() => {
                let name: &str = &r;
                match name {
                    "amp" => text.push('&'),
                    "lt" => text.push('<'),
                    "gt" => text.push('>'),
                    "quot" => text.push('"'),
                    "apos" => text.push('\''),
                    _ => {
                        if let Ok(Some(c)) = r.resolve_char_ref() {
                            text.push(c);
                        }
                    }
                }
            }
            Event::End(e) => match local(e.local_name().as_ref()).as_str() {
                "href" if field == Some("href") => {
                    href = Some(std::mem::take(&mut text).trim().to_owned());
                    field = None;
                }
                "getcontentlength" if field == Some("len") => {
                    size = text.trim().parse().unwrap_or(0);
                    field = None;
                }
                "response" => {
                    if let Some(h) = href.take() {
                        let p = href_path(&h);
                        let p = p.trim_end_matches('/');
                        if p != self_trim {
                            let child = p.rsplit('/').next().unwrap_or_default();
                            if !child.is_empty() && out.len() < MAX_ENTRIES {
                                out.push(RemoteEntry {
                                    name: child.to_owned(),
                                    is_dir,
                                    size,
                                });
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}
