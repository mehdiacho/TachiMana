//! Typed async client for every ext-host endpoint.

use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::model::{
    AvailableExtension, Chapter, ErrorBody, Extension, Filter, Health, Image, Manga, MangaUpdate,
    MangasPage, Page, Preference, Repo, Source,
};
use crate::Error;

/// Largest response body accepted (a long chapter page can be several MB; catalogs are ~1 MB).
const MAX_BODY: usize = 64 * 1024 * 1024;

/// A little above the host's own timeouts (60 s per source call, 120 s per install), so the host
/// answers with its JSON timeout error before the client gives up.
const SOURCE_TIMEOUT: Duration = Duration::from_secs(70);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(130);

#[derive(Clone)]
pub struct Client {
    base: String,
    auth: String,
    http: reqwest::Client,
}

impl Client {
    /// `base` is `http://127.0.0.1:<port>`.
    pub fn new(base: impl Into<String>, token: &str) -> Result<Self, Error> {
        // When another crate in the build turns on reqwest's rustls feature, building a client
        // without a crypto provider panics. The host is plain http on 127.0.0.1, but install one
        // anyway; a second install (someone got there first) fails harmlessly.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| Error::Transport(e.to_string()))?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_string(),
            auth: format!("Bearer {token}"),
            http,
        })
    }

    pub async fn health(&self) -> Result<Health, Error> {
        self.get("/v1/health").await
    }

    pub async fn extensions(&self) -> Result<Vec<Extension>, Error> {
        self.get("/v1/extensions").await
    }

    pub async fn inspect_repo(&self, url: &str) -> Result<Repo, Error> {
        self.post("/v1/repos/inspect", &json!({ "url": url })).await
    }

    pub async fn catalog(&self, repo_url: &str) -> Result<Vec<AvailableExtension>, Error> {
        self.post("/v1/repos/catalog", &json!({ "repoUrl": repo_url }))
            .await
    }

    pub async fn install(&self, repo_url: &str, pkg_name: &str) -> Result<Extension, Error> {
        let body = json!({ "repoUrl": repo_url, "pkgName": pkg_name });
        let resp = self
            .send(
                Method::POST,
                "/v1/extensions/install",
                Some(&body),
                INSTALL_TIMEOUT,
            )
            .await?;
        decode(resp).await
    }

    pub async fn uninstall(&self, pkg_name: &str) -> Result<(), Error> {
        let path = format!("/v1/extensions/{}", encode(pkg_name));
        let resp = self
            .send::<Value>(Method::DELETE, &path, None, SOURCE_TIMEOUT)
            .await?;
        decode::<Value>(resp).await.map(|_| ())
    }

    pub async fn sources(&self) -> Result<Vec<Source>, Error> {
        self.get("/v1/sources").await
    }

    pub async fn popular(&self, source: i64, page: u32) -> Result<MangasPage, Error> {
        self.get(&format!(
            "/v1/sources/{source}/popular?page={}",
            page.max(1)
        ))
        .await
    }

    pub async fn latest(&self, source: i64, page: u32) -> Result<MangasPage, Error> {
        self.get(&format!("/v1/sources/{source}/latest?page={}", page.max(1)))
            .await
    }

    pub async fn search(&self, source: i64, query: &str, page: u32) -> Result<MangasPage, Error> {
        self.get(&format!(
            "/v1/sources/{source}/search?q={}&page={}",
            encode(query),
            page.max(1)
        ))
        .await
    }

    /// The source's search filters, as it sets them to start with. Empty when the source has
    /// none, and when the installed helper is from before filters (it answers 404).
    pub async fn filters(&self, source: i64) -> Result<Vec<Filter>, Error> {
        match self.get(&format!("/v1/sources/{source}/filters")).await {
            Err(Error::Host { status: 404, .. }) => Ok(Vec::new()),
            other => other,
        }
    }

    /// A search with the person's filter choices: `states` is the array of filter states in the
    /// order [`filters`](Self::filters) listed them (a group's state is its children's array).
    pub async fn search_filtered(
        &self,
        source: i64,
        query: &str,
        page: u32,
        states: &Value,
    ) -> Result<MangasPage, Error> {
        self.post(
            &format!("/v1/sources/{source}/search"),
            &json!({"q": query, "page": page.max(1), "filters": states}),
        )
        .await
    }

    pub async fn manga(
        &self,
        source: i64,
        manga: &Manga,
        fetch_details: bool,
        fetch_chapters: bool,
    ) -> Result<MangaUpdate, Error> {
        let body = json!({ "manga": manga, "fetchDetails": fetch_details, "fetchChapters": fetch_chapters });
        self.post(&format!("/v1/sources/{source}/manga"), &body)
            .await
    }

    pub async fn pages(&self, source: i64, chapter: &Chapter) -> Result<Vec<Page>, Error> {
        self.post(
            &format!("/v1/sources/{source}/pages"),
            &json!({ "chapter": chapter }),
        )
        .await
    }

    pub async fn page_image(&self, source: i64, page: &Page) -> Result<Image, Error> {
        self.image(source, &json!({ "page": page })).await
    }

    pub async fn image_url(&self, source: i64, url: &str) -> Result<Image, Error> {
        self.image(source, &json!({ "url": url })).await
    }

    pub async fn preferences(&self, source: i64) -> Result<Vec<Preference>, Error> {
        self.get(&format!("/v1/sources/{source}/preferences")).await
    }

    pub async fn set_preference(
        &self,
        source: i64,
        key: &str,
        value: Value,
    ) -> Result<Preference, Error> {
        let path = format!("/v1/sources/{source}/preferences/{}", encode(key));
        let resp = self
            .send(
                Method::PUT,
                &path,
                Some(&json!({ "value": value })),
                SOURCE_TIMEOUT,
            )
            .await?;
        decode(resp).await
    }

    pub async fn set_cloudflare_bypass(&self, enabled: bool) -> Result<(), Error> {
        let body = json!({ "cloudflareBypassEnabled": enabled });
        let resp = self
            .send(Method::PUT, "/v1/config", Some(&body), SOURCE_TIMEOUT)
            .await?;
        decode::<Value>(resp).await.map(|_| ())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let resp = self
            .send::<Value>(Method::POST, "/v1/shutdown", None, SOURCE_TIMEOUT)
            .await?;
        decode::<Value>(resp).await.map(|_| ())
    }

    async fn image(&self, source: i64, body: &Value) -> Result<Image, Error> {
        let path = format!("/v1/sources/{source}/image");
        let resp = self
            .send(Method::POST, &path, Some(body), SOURCE_TIMEOUT)
            .await?;
        let resp = check(resp).await?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = read_capped(resp).await?;
        Ok(Image {
            bytes,
            content_type,
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let resp = self
            .send::<Value>(Method::GET, path, None, SOURCE_TIMEOUT)
            .await?;
        decode(resp).await
    }

    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Error> {
        let resp = self
            .send(Method::POST, path, Some(body), SOURCE_TIMEOUT)
            .await?;
        decode(resp).await
    }

    async fn send<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        timeout: Duration,
    ) -> Result<reqwest::Response, Error> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header(reqwest::header::AUTHORIZATION, &self.auth)
            .timeout(timeout);
        if let Some(body) = body {
            req = req.json(body);
        }
        req.send().await.map_err(|e| {
            if e.is_timeout() {
                Error::Host {
                    status: 504,
                    kind: "timeout".into(),
                    message: "The extension host did not answer in time".into(),
                }
            } else {
                Error::Transport(e.to_string())
            }
        })
    }
}

/// Turns a non-2xx response into [`Error::Host`] with the host's own message.
async fn check(resp: reqwest::Response) -> Result<reqwest::Response, Error> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let bytes = read_capped(resp).await.unwrap_or_default();
    let (kind, message) = match serde_json::from_slice::<ErrorBody>(&bytes) {
        Ok(body) => (body.kind, body.error),
        Err(_) => (
            fallback_kind(status).into(),
            format!("HTTP {}", status.as_u16()),
        ),
    };
    Err(Error::Host {
        status: status.as_u16(),
        kind,
        message,
    })
}

fn fallback_kind(status: StatusCode) -> &'static str {
    match status.as_u16() {
        401 => "unauthorized",
        404 => "notFound",
        400..=499 => "badRequest",
        _ => "source",
    }
}

async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, Error> {
    let resp = check(resp).await?;
    let bytes = read_capped(resp).await?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))
}

async fn read_capped(mut resp: reqwest::Response) -> Result<Vec<u8>, Error> {
    if resp.content_length().is_some_and(|n| n > MAX_BODY as u64) {
        return Err(Error::TooLarge);
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| Error::Transport(e.to_string()))?
    {
        if out.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(Error::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Percent-encodes everything outside the RFC 3986 unreserved set.
pub(crate) fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_queries_and_keys() {
        assert_eq!(encode("one piece"), "one%20piece");
        assert_eq!(encode("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
        assert_eq!(encode("é"), "%C3%A9");
        assert_eq!(encode("eu.kanade-x_y~"), "eu.kanade-x_y~");
    }
}
