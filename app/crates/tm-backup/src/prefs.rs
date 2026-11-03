//! Mihon's app preferences (tag 104). The restore parks them raw for a re-export, except the
//! private ones, which this module reads instead.
//!
//! Each entry is `BackupPreference { 1: key, 2: value }`, and the value is kotlinx
//! serialization's polymorphic message `{ 1: class name, 2: the value's own message }`. A
//! `StringPreferenceValue` is `{ 1: string }`.
//!
//! Mihon writes private preferences (keys starting `__PRIVATE_`) only when the user ticks
//! "Include sensitive settings" on Create backup. For trackers they are
//! (`TrackPreferences.kt`, tracker id `N`):
//! - `__PRIVATE_track_token_N`: the OAuth token. For AniList (id 2) it is the JSON of `ALOAuth`,
//!   `{"access_token": "...", "expires": <unix ms>}`.
//! - `__PRIVATE_pref_mangasync_password_N`: for AniList, the access token again.

use crate::model::Backup;

pub const PRIVATE_PREFIX: &str = "__PRIVATE_";
/// Mihon's tracker id for AniList (`TrackerManager.ANILIST`).
pub const ANILIST: i64 = 2;
/// Longer than any real token; anything bigger is not one.
const MAX_TOKEN_LEN: usize = 8 * 1024;

#[derive(Clone, PartialEq, prost::Message)]
struct RawPref {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(message, optional, tag = "2")]
    value: Option<RawValue>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct RawValue {
    #[prost(string, tag = "1")]
    class: String,
    #[prost(bytes = "vec", tag = "2")]
    body: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct StringValue {
    #[prost(string, tag = "1")]
    value: String,
}

/// The key of one raw preference, `None` when it does not decode.
fn key_of(raw: &[u8]) -> Option<String> {
    <RawPref as prost::Message>::decode(raw).ok().map(|p| p.key)
}

/// Whether a raw preference is a private one (a token or password). One that does not decode
/// is not, so it is still parked as before.
pub fn is_private(raw: &[u8]) -> bool {
    key_of(raw).is_some_and(|k| k.starts_with(PRIVATE_PREFIX))
}

/// The string value of the private preference `__PRIVATE_<key>`, when the backup has it and it
/// is not empty.
pub fn private_string(prefs: &[Vec<u8>], key: &str) -> Option<String> {
    let want = format!("{PRIVATE_PREFIX}{key}");
    prefs.iter().find_map(|raw| {
        let p = <RawPref as prost::Message>::decode(raw.as_slice()).ok()?;
        if p.key != want {
            return None;
        }
        let v = p.value?;
        if !v.class.ends_with("StringPreferenceValue") {
            return None;
        }
        let s = <StringValue as prost::Message>::decode(v.body.as_slice())
            .ok()?
            .value;
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_owned())
    })
}

/// A tracker sign-in carried by a backup. Its `Debug` leaves the token out, so it can't reach a log.
#[derive(Clone, PartialEq, Eq)]
pub struct TrackerLogin {
    pub token: String,
    /// Unix milliseconds the token stops working, when the backup says.
    pub expires_at: Option<i64>,
}

impl std::fmt::Debug for TrackerLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackerLogin")
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(serde::Deserialize)]
struct AlOAuth {
    access_token: String,
    expires: Option<i64>,
}

/// The AniList sign-in in `backup`, when it was made with sensitive settings included.
pub fn anilist_login(backup: &Backup) -> Option<TrackerLogin> {
    let prefs = &backup.backup_preferences;
    let from_oauth = private_string(prefs, &format!("track_token_{ANILIST}")).and_then(|json| {
        let o: AlOAuth = serde_json::from_str(&json).ok()?;
        Some(TrackerLogin {
            token: o.access_token,
            expires_at: o.expires,
        })
    });
    let login = from_oauth.or_else(|| {
        private_string(prefs, &format!("pref_mangasync_password_{ANILIST}")).map(|token| {
            TrackerLogin {
                token,
                expires_at: None,
            }
        })
    })?;
    let ok = !login.token.trim().is_empty()
        && login.token.len() <= MAX_TOKEN_LEN
        && login.token.chars().all(|c| c.is_ascii_graphic());
    ok.then_some(login)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use prost::Message;

    /// A raw `BackupPreference` holding a string, as Mihon writes it.
    pub(crate) fn string_pref(key: &str, value: &str) -> Vec<u8> {
        RawPref {
            key: key.to_owned(),
            value: Some(RawValue {
                class: "eu.kanade.tachiyomi.data.backup.models.StringPreferenceValue".into(),
                body: StringValue {
                    value: value.to_owned(),
                }
                .encode_to_vec(),
            }),
        }
        .encode_to_vec()
    }

    fn backup(prefs: Vec<Vec<u8>>) -> Backup {
        Backup {
            backup_preferences: prefs,
            ..Default::default()
        }
    }

    #[test]
    fn reads_the_anilist_oauth_json() {
        let b = backup(vec![
            string_pref("anilist_score_type", "POINT_10"),
            string_pref(
                "__PRIVATE_track_token_2",
                r#"{"access_token":"abc.def","expires":1900000000000}"#,
            ),
        ]);
        assert_eq!(
            anilist_login(&b),
            Some(TrackerLogin {
                token: "abc.def".into(),
                expires_at: Some(1_900_000_000_000),
            })
        );
    }

    #[test]
    fn falls_back_to_the_password_and_ignores_other_trackers() {
        let b = backup(vec![
            string_pref("__PRIVATE_track_token_1", r#"{"access_token":"mal"}"#),
            string_pref("__PRIVATE_pref_mangasync_password_2", "tok"),
        ]);
        assert_eq!(anilist_login(&b).map(|l| l.token), Some("tok".into()));
    }

    #[test]
    fn no_login_without_sensitive_settings_or_with_junk() {
        assert_eq!(
            anilist_login(&backup(vec![string_pref("anilist_score_type", "POINT_10")])),
            None
        );
        assert_eq!(anilist_login(&backup(vec![vec![0xff, 0x00, 0x12]])), None);
        assert_eq!(
            anilist_login(&backup(vec![string_pref(
                "__PRIVATE_track_token_2",
                "not json"
            )])),
            None
        );
        assert_eq!(
            anilist_login(&backup(vec![string_pref(
                "__PRIVATE_pref_mangasync_password_2",
                "has space"
            )])),
            None
        );
        assert_eq!(
            anilist_login(&backup(vec![string_pref(
                "__PRIVATE_pref_mangasync_password_2",
                "  "
            )])),
            None
        );
    }

    #[test]
    fn debug_never_shows_the_token() {
        let l = TrackerLogin {
            token: "secret-token".into(),
            expires_at: None,
        };
        assert!(!format!("{l:?}").contains("secret-token"));
    }

    #[test]
    fn private_keys_are_recognised() {
        assert!(is_private(&string_pref("__PRIVATE_track_token_2", "x")));
        assert!(!is_private(&string_pref("anilist_score_type", "x")));
        assert!(!is_private(&[0xff, 0x00]));
    }
}
