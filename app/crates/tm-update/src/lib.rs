//! Updates (`update/UpdateChecker.kt`, `AppVersion.kt`): ask GitHub for the newest release of this
//! app, compare versions numerically, and download the Windows installer when the user asks.
//!
//! Like the Kotlin app it stays conservative: the check is anonymous and read-only, nothing is
//! downloaded until the user clicks, nothing runs without a second click, and any failure to
//! check only means "no update". The Rust app's releases are tagged `app-v<version>` so they
//! never mix with the Kotlin app's `v<version>` releases while both exist.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub const DEFAULT_REPO: &str = "mehdiacho/TachiMana";
pub const API_BASE: &str = "https://api.github.com";
/// Tag prefix of the Rust app's releases.
pub const TAG_PREFIX: &str = "app-v";

/// Largest installer we download.
pub const MAX_INSTALLER: u64 = 512 * 1024 * 1024;
const MAX_LISTING: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("couldn't reach GitHub: {0}")]
    Http(String),
    #[error("GitHub answered {0}")]
    Status(u16),
    #[error("unexpected answer from GitHub: {0}")]
    Protocol(String),
    #[error("the download doesn't match its checksum")]
    Checksum,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, UpdateError>;

/// Compares dotted numeric versions, ignoring a leading `v` (or `app-v`). Segments that are not
/// numbers count as 0 and missing ones too, so a malformed tag is never "newer" and `1.1`
/// equals `1.1.0`. `1.0.10` is newer than `1.0.9`.
pub fn compare(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<u64> {
        let s = s.trim();
        let s = s.strip_prefix(TAG_PREFIX).unwrap_or(s);
        let s = s.trim_start_matches(['v', 'V']);
        s.split(['.', '-'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

/// What the running build can install by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// The per-user NSIS installer, `*-setup.exe`.
    Windows,
    /// No self-install: the user gets the release page.
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// The `<name>.sha256` published next to it, if any.
    pub sha256_url: Option<String>,
}

/// A release newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    /// The installer for this platform, when there is one to download.
    pub installer: Option<Asset>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Picks the newest published `app-v` release newer than `current` from a GitHub releases
/// listing (JSON). `None` when up to date.
pub fn pick(listing: &[u8], current: &str, platform: Platform) -> Result<Option<Update>> {
    let releases: Vec<GhRelease> =
        serde_json::from_slice(listing).map_err(|e| UpdateError::Protocol(e.to_string()))?;
    let best = releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && r.tag_name.starts_with(TAG_PREFIX))
        .filter(|r| compare(&r.tag_name, current) == Ordering::Greater)
        .max_by(|a, b| compare(&a.tag_name, &b.tag_name));
    let Some(r) = best else { return Ok(None) };
    let installer = match platform {
        Platform::Windows => r
            .assets
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with("-setup.exe"))
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size,
                sha256_url: r
                    .assets
                    .iter()
                    .find(|s| s.name == format!("{}.sha256", a.name))
                    .map(|s| s.browser_download_url.clone()),
            }),
        Platform::Other => None,
    };
    Ok(Some(Update {
        version: r
            .tag_name
            .strip_prefix(TAG_PREFIX)
            .unwrap_or(&r.tag_name)
            .to_owned(),
        notes: r.body.unwrap_or_default(),
        page_url: r.html_url,
        installer,
    }))
}

#[derive(Debug, Clone)]
pub struct Updater {
    http: reqwest::Client,
    api_base: String,
    repo: String,
}

impl Updater {
    pub fn new() -> Result<Self> {
        Self::with(API_BASE, DEFAULT_REPO)
    }

    /// An updater for another API host or repository (tests run a fake one).
    pub fn with(api_base: &str, repo: &str) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("TachiMana/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| UpdateError::Http(e.to_string()))?;
        Ok(Self {
            http,
            api_base: api_base.trim_end_matches('/').to_owned(),
            repo: repo.to_owned(),
        })
    }

    /// The newest release newer than `current`, or `None`.
    pub async fn check(&self, current: &str, platform: Platform) -> Result<Option<Update>> {
        let url = format!("{}/repos/{}/releases?per_page=30", self.api_base, self.repo);
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| UpdateError::Http(e.without_url().to_string()))?;
        if !resp.status().is_success() {
            return Err(UpdateError::Status(resp.status().as_u16()));
        }
        let body = read_capped(resp, MAX_LISTING).await?;
        pick(&body, current, platform)
    }

    /// Downloads `asset` into `dir` and returns the file. Checks the size GitHub announced and,
    /// when published, the SHA-256; a file that fails either is deleted.
    pub async fn download(
        &self,
        asset: &Asset,
        dir: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf> {
        let name = Path::new(&asset.name)
            .file_name()
            .filter(|n| *n == asset.name.as_str())
            .ok_or_else(|| UpdateError::Protocol("odd installer name".into()))?;
        let expected_sha = match &asset.sha256_url {
            Some(u) => Some(self.fetch_sha(u).await?),
            None => None,
        };
        tokio::fs::create_dir_all(dir).await?;
        let dest = dir.join(name);
        let part = dir.join(format!("{}.part", asset.name));
        let result = self
            .stream_to(&asset.url, &part, asset.size, &mut progress)
            .await
            .and_then(|digest| match expected_sha {
                Some(want) if want != digest => Err(UpdateError::Checksum),
                _ => Ok(()),
            });
        match result {
            Ok(()) => {
                tokio::fs::rename(&part, &dest).await?;
                Ok(dest)
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(e)
            }
        }
    }

    async fn fetch_sha(&self, url: &str) -> Result<String> {
        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| UpdateError::Http(e.without_url().to_string()))?;
        if !resp.status().is_success() {
            return Err(UpdateError::Status(resp.status().as_u16()));
        }
        let text = read_capped(resp, 4096).await?;
        // `sha256sum` format: the hex digest, then optionally the file name.
        let hex = String::from_utf8_lossy(&text)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(hex)
        } else {
            Err(UpdateError::Protocol("bad checksum file".into()))
        }
    }

    /// Streams `url` to `dest`, returning the lowercase hex SHA-256 of what was written.
    async fn stream_to(
        &self,
        url: &str,
        dest: &Path,
        announced: u64,
        progress: &mut impl FnMut(u64, u64),
    ) -> Result<String> {
        let mut resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| UpdateError::Http(e.without_url().to_string()))?;
        if !resp.status().is_success() {
            return Err(UpdateError::Status(resp.status().as_u16()));
        }
        let total = if announced > 0 {
            announced
        } else {
            resp.content_length().unwrap_or(0)
        };
        if total > MAX_INSTALLER {
            return Err(UpdateError::Protocol("installer too large".into()));
        }
        let mut f = tokio::fs::File::create(dest).await?;
        let mut hasher = Sha256::new();
        let mut n: u64 = 0;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| UpdateError::Http(e.without_url().to_string()))?
        {
            n = n.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            if n > MAX_INSTALLER || (total > 0 && n > total) {
                return Err(UpdateError::Protocol(
                    "installer larger than announced".into(),
                ));
            }
            hasher.update(&chunk);
            f.write_all(&chunk).await?;
            progress(n, total);
        }
        f.flush().await?;
        f.sync_all().await?;
        if total > 0 && n != total {
            return Err(UpdateError::Protocol(format!(
                "got {n} bytes, expected {total}"
            )));
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}

async fn read_capped(mut resp: reqwest::Response, cap: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| UpdateError::Http(e.without_url().to_string()))?
    {
        if out.len().saturating_add(chunk.len()) > cap {
            return Err(UpdateError::Protocol("answer too large".into()));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    /// The checks of `UpdateSelfTest.kt`.
    #[test]
    fn versions_compare_numerically() {
        use Ordering::*;
        assert_eq!(compare("1.0.43", "1.0.42"), Greater);
        assert_eq!(compare("1.0.42", "1.0.42"), Equal);
        assert_eq!(compare("1.0.41", "1.0.42"), Less);
        assert_eq!(
            compare("1.0.10", "1.0.9"),
            Greater,
            "10 beats 9 numerically, not lexically"
        );
        assert_eq!(compare("1.1.0", "1.0.99"), Greater);
        assert_eq!(compare("v1.0.43", "1.0.42"), Greater);
        assert_eq!(compare("app-v1.0.43", "1.0.42"), Greater);
        assert_eq!(compare("1.1", "1.1.0"), Equal);
        assert_eq!(
            compare("garbage", "1.0.0"),
            Less,
            "a malformed tag is never newer"
        );
        assert_eq!(
            compare("99999999999999999999999.0", "1.0"),
            Less,
            "overflow is not newer"
        );
    }
}
