//! The AniList GraphQL client (`AniListApi.kt`). Search works without signing in; anything that
//! touches the user's list needs the token from [`crate::auth`].

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Result, TrackError};

pub const ENDPOINT: &str = "https://graphql.anilist.co";

/// Largest GraphQL answer we read. A search page is a few KB.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// The account that owns a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub name: String,
}

/// A manga on AniList.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub id: i64,
    pub title: String,
    pub chapters: Option<i64>,
    pub format: Option<String>,
    pub status: Option<String>,
    pub year: Option<i64>,
}

impl Media {
    pub fn url(&self) -> String {
        format!("https://anilist.co/manga/{}", self.id)
    }
}

/// The user's list entry for a manga: progress in chapters, score out of 10.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: i64,
    pub status: String,
    pub progress: i64,
    pub score: f64,
}

/// Lineage track-status values (what `manga_sync.status` and `.tachibk` `BackupTracking.status`
/// hold, Yōkai's `Anilist.kt`) and AniList's enum names.
pub mod status {
    pub const READING: i64 = 1;
    pub const COMPLETED: i64 = 2;
    pub const PAUSED: i64 = 3;
    pub const DROPPED: i64 = 4;
    pub const PLANNING: i64 = 5;
    pub const REPEATING: i64 = 6;

    pub const ALL: [i64; 6] = [READING, COMPLETED, PAUSED, DROPPED, PLANNING, REPEATING];

    pub fn to_api(status: i64) -> &'static str {
        match status {
            COMPLETED => "COMPLETED",
            PAUSED => "PAUSED",
            DROPPED => "DROPPED",
            PLANNING => "PLANNING",
            REPEATING => "REPEATING",
            _ => "CURRENT",
        }
    }

    pub fn from_api(status: &str) -> i64 {
        match status {
            "COMPLETED" => COMPLETED,
            "PAUSED" => PAUSED,
            "DROPPED" => DROPPED,
            "PLANNING" => PLANNING,
            "REPEATING" => REPEATING,
            _ => READING,
        }
    }

    pub fn label(status: i64) -> &'static str {
        match status {
            COMPLETED => "Completed",
            PAUSED => "Paused",
            DROPPED => "Dropped",
            PLANNING => "Planning",
            REPEATING => "Rereading",
            _ => "Reading",
        }
    }
}

/// Changes to a list entry; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntryChange {
    pub status: Option<&'static str>,
    pub progress: Option<i64>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AniListApi {
    http: reqwest::Client,
    endpoint: String,
}

impl AniListApi {
    pub fn new() -> Result<Self> {
        Self::with_endpoint(ENDPOINT)
    }

    /// A client for another GraphQL endpoint (tests run a fake one).
    pub fn with_endpoint(endpoint: &str) -> Result<Self> {
        // reqwest is built without a default crypto provider; give rustls one. A second install
        // returns an error that only means one is already there.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("TachiMana/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| TrackError::Http(e.to_string()))?;
        Ok(Self {
            http,
            endpoint: endpoint.to_owned(),
        })
    }

    /// Who owns `token`. Also how a token is checked before it is saved.
    pub async fn viewer(&self, token: &str) -> Result<User> {
        let data = self
            .graphql("query { Viewer { id name } }", json!({}), Some(token))
            .await?;
        let v = data
            .get("Viewer")
            .ok_or_else(|| TrackError::Protocol("no Viewer".into()))?;
        Ok(User {
            id: int(v, "id")?,
            name: text(v, "name")?,
        })
    }

    /// Manga search, no sign-in needed. At most 12 hits, like the Kotlin app.
    pub async fn search(&self, query: &str) -> Result<Vec<Media>> {
        let q = "query ($q: String) { Page(perPage: 12) { media(search: $q, type: MANGA) { \
                 id title { userPreferred } chapters format status startDate { year } } } }";
        let data = self.graphql(q, json!({ "q": query }), None).await?;
        let list = data
            .get("Page")
            .and_then(|p| p.get("media"))
            .and_then(Value::as_array)
            .ok_or_else(|| TrackError::Protocol("no Page.media".into()))?;
        list.iter()
            .map(|m| {
                Ok(Media {
                    id: int(m, "id")?,
                    title: m
                        .get("title")
                        .and_then(|t| t.get("userPreferred"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    chapters: m.get("chapters").and_then(Value::as_i64),
                    format: m.get("format").and_then(Value::as_str).map(str::to_owned),
                    status: m.get("status").and_then(Value::as_str).map(str::to_owned),
                    year: m
                        .get("startDate")
                        .and_then(|d| d.get("year"))
                        .and_then(Value::as_i64),
                })
            })
            .collect()
    }

    /// The user's entry for `media_id`, or `None` when it isn't on their list.
    pub async fn find_entry(
        &self,
        token: &str,
        user_id: i64,
        media_id: i64,
    ) -> Result<Option<Entry>> {
        let q =
            "query ($userId: Int, $mediaId: Int) { MediaList(userId: $userId, mediaId: $mediaId, \
                 type: MANGA) { id status progress score(format: POINT_10) } }";
        match self
            .graphql(
                q,
                json!({ "userId": user_id, "mediaId": media_id }),
                Some(token),
            )
            .await
        {
            // AniList answers "not on your list" with a 404 GraphQL error.
            Err(TrackError::Api(m)) if m.to_ascii_lowercase().starts_with("not found") => Ok(None),
            Err(e) => Err(e),
            Ok(data) => match data.get("MediaList") {
                None | Some(Value::Null) => Ok(None),
                Some(e) => Ok(Some(entry(e)?)),
            },
        }
    }

    /// Create or change the user's entry for `media_id`.
    pub async fn save_entry(
        &self,
        token: &str,
        media_id: i64,
        change: &EntryChange,
    ) -> Result<Entry> {
        let mut args = vec!["mediaId: $mediaId"];
        let mut vars = Map::new();
        vars.insert("mediaId".into(), json!(media_id));
        if let Some(s) = change.status {
            args.push("status: $status");
            vars.insert("status".into(), json!(s));
        }
        if let Some(p) = change.progress {
            args.push("progress: $progress");
            vars.insert("progress".into(), json!(p));
        }
        if let Some(s) = change.score {
            if !s.is_finite() {
                return Err(TrackError::Invalid("Score must be a number.".into()));
            }
            args.push("score: $score");
            vars.insert("score".into(), json!(s));
        }
        let q = format!(
            "mutation ($mediaId: Int, $status: MediaListStatus, $progress: Int, $score: Float) {{ \
             SaveMediaListEntry({}) {{ id status progress score(format: POINT_10) }} }}",
            args.join(", ")
        );
        let data = self.graphql(&q, Value::Object(vars), Some(token)).await?;
        entry(
            data.get("SaveMediaListEntry")
                .ok_or_else(|| TrackError::Protocol("no SaveMediaListEntry".into()))?,
        )
    }

    async fn graphql(&self, query: &str, variables: Value, token: Option<&str>) -> Result<Value> {
        let mut req = self
            .http
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(json!({ "query": query, "variables": variables }).to_string());
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let mut resp = req
            .send()
            .await
            .map_err(|e| TrackError::Http(e.without_url().to_string()))?;
        let code = resp.status().as_u16();
        let mut body = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| TrackError::Http(e.without_url().to_string()))?
        {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(TrackError::Protocol("answer too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&body)
            .map_err(|_| TrackError::Protocol(format!("not JSON (HTTP {code})")))?;
        if let Some(err) = v
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(TrackError::Api(msg.to_owned()));
        }
        match v.get("data") {
            Some(d @ Value::Object(_)) => Ok(d.clone()),
            _ => Err(TrackError::Protocol(format!("empty answer (HTTP {code})"))),
        }
    }
}

fn int(v: &Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| TrackError::Protocol(format!("no {key}")))
}

fn text(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| TrackError::Protocol(format!("no {key}")))
}

fn entry(v: &Value) -> Result<Entry> {
    Ok(Entry {
        id: int(v, "id")?,
        status: v
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("CURRENT")
            .to_owned(),
        progress: v.get("progress").and_then(Value::as_i64).unwrap_or(0),
        score: v
            .get("score")
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite())
            .unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::status;

    #[test]
    fn status_mapping_round_trips() {
        for s in status::ALL {
            assert_eq!(status::from_api(status::to_api(s)), s);
        }
        assert_eq!(status::from_api("SOMETHING_NEW"), status::READING);
        assert_eq!(status::to_api(99), "CURRENT");
    }
}
