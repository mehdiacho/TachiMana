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

#[derive(Clone, PartialEq, prost::Message)]
struct BoolValue {
    #[prost(bool, tag = "1")]
    value: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
struct IntValue {
    #[prost(int32, tag = "1")]
    value: i32,
}

const CLASS_PREFIX: &str = "eu.kanade.tachiyomi.data.backup.models.";
/// Longest NAS text value taken from a backup; a real host, folder or user name is far shorter.
const MAX_NAS_FIELD: usize = 1024;

/// One public preference's class name and body, `None` when absent or undecodable.
fn public_value(prefs: &[Vec<u8>], key: &str) -> Option<RawValue> {
    prefs.iter().find_map(|raw| {
        let p = <RawPref as prost::Message>::decode(raw.as_slice()).ok()?;
        (p.key == key).then_some(p.value).flatten()
    })
}

fn pref_string(prefs: &[Vec<u8>], key: &str) -> Option<String> {
    let v = public_value(prefs, key)?;
    if !v.class.ends_with("StringPreferenceValue") {
        return None;
    }
    let s = <StringValue as prost::Message>::decode(v.body.as_slice())
        .ok()?
        .value;
    let s = s.trim();
    (s.len() <= MAX_NAS_FIELD && !s.chars().any(char::is_control)).then(|| s.to_owned())
}

fn pref_bool(prefs: &[Vec<u8>], key: &str) -> Option<bool> {
    let v = public_value(prefs, key)?;
    v.class.ends_with("BooleanPreferenceValue").then_some(())?;
    <BoolValue as prost::Message>::decode(v.body.as_slice())
        .ok()
        .map(|b| b.value)
}

fn pref_int(prefs: &[Vec<u8>], key: &str) -> Option<i64> {
    let v = public_value(prefs, key)?;
    v.class.ends_with("IntPreferenceValue").then_some(())?;
    <IntValue as prost::Message>::decode(v.body.as_slice())
        .ok()
        .map(|b| i64::from(b.value))
}

fn raw_pref(key: &str, class: &str, body: Vec<u8>) -> Vec<u8> {
    <RawPref as prost::Message>::encode_to_vec(&RawPref {
        key: key.to_owned(),
        value: Some(RawValue {
            class: format!("{CLASS_PREFIX}{class}"),
            body,
        }),
    })
}

/// The phone's remote storage (NAS) settings, as Mehdi's Mihon fork writes them among the app
/// preferences (`remote_storage_*`). The password is not among them: it is never carried here.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct NasPrefs {
    pub enabled: Option<bool>,
    pub host: String,
    /// Kept as text, as both apps store it.
    pub port: Option<String>,
    pub use_https: Option<bool>,
    pub folder: Option<String>,
    pub username: Option<String>,
    pub cache_mb: Option<i64>,
}

/// Preference keys of [`NasPrefs`] in the fork's backups.
pub mod nas_keys {
    pub const ENABLED: &str = "remote_storage_enabled";
    pub const HOST: &str = "remote_storage_host";
    pub const PORT: &str = "remote_storage_port";
    pub const HTTPS: &str = "remote_storage_https";
    pub const FOLDER: &str = "remote_storage_folder";
    pub const USERNAME: &str = "remote_storage_username";
    pub const CACHE_MB: &str = "remote_storage_cache_mb";
    pub const ALL: [&str; 7] = [ENABLED, HOST, PORT, HTTPS, FOLDER, USERNAME, CACHE_MB];
}

/// The NAS settings in `prefs`, when they name a host.
pub fn nas_prefs(prefs: &[Vec<u8>]) -> Option<NasPrefs> {
    let host = pref_string(prefs, nas_keys::HOST).filter(|h| !h.is_empty())?;
    Some(NasPrefs {
        enabled: pref_bool(prefs, nas_keys::ENABLED),
        host,
        port: pref_string(prefs, nas_keys::PORT)
            .filter(|p| p.parse::<u32>().is_ok_and(|n| (1..=65_535).contains(&n))),
        use_https: pref_bool(prefs, nas_keys::HTTPS),
        folder: pref_string(prefs, nas_keys::FOLDER),
        username: pref_string(prefs, nas_keys::USERNAME),
        cache_mb: pref_int(prefs, nas_keys::CACHE_MB).filter(|mb| *mb > 0),
    })
}

/// A protobuf field as read leniently: a number or a length-delimited body.
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos)?;
        *pos = pos.checked_add(1)?;
        out |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

/// The fields of one message, in order; `None` when it is malformed. Fixed-width fields are
/// skipped, groups refuse the message.
fn fields(buf: &[u8]) -> Option<Vec<(u64, Field<'_>)>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        let key = varint(buf, &mut pos)?;
        let (tag, wire) = (key >> 3, key & 7);
        match wire {
            0 => out.push((tag, Field::Varint(varint(buf, &mut pos)?))),
            1 => pos = pos.checked_add(8)?,
            2 => {
                let len = usize::try_from(varint(buf, &mut pos)?).ok()?;
                let end = pos.checked_add(len)?;
                out.push((tag, Field::Bytes(buf.get(pos..end)?)));
                pos = end;
            }
            5 => pos = pos.checked_add(4)?,
            _ => return None,
        }
    }
    (pos == buf.len()).then_some(out)
}

/// The NAS connection section of a phone backup (tag 901, `remoteStorage`, TachiMana#35): the
/// settings, and the password sealed with a passphrase when the person chose to include it.
/// Read field by field and leniently, so a type the fork changes later drops that field only.
/// `None` when it names no host or is not a message.
pub fn nas_section(raw: &[u8]) -> Option<(NasPrefs, Option<Vec<u8>>)> {
    let mut nas = NasPrefs::default();
    let mut sealed = None;
    let text = |b: &[u8]| std::str::from_utf8(b).ok().map(|s| s.trim().to_owned());
    for (tag, f) in fields(raw)? {
        match (tag, f) {
            (1, Field::Bytes(b)) => nas.host = text(b).unwrap_or_default(),
            (2, Field::Varint(n)) => nas.port = Some(n.to_string()),
            (2, Field::Bytes(b)) => nas.port = text(b),
            (3, Field::Varint(n)) => nas.use_https = Some(n != 0),
            (4, Field::Bytes(b)) => nas.folder = text(b),
            (5, Field::Bytes(b)) => nas.username = text(b),
            (6, Field::Bytes(b)) if crate::seal::is_sealed(b) => sealed = Some(b.to_vec()),
            _ => {}
        }
    }
    nas.port = nas
        .port
        .filter(|p| p.parse::<u32>().is_ok_and(|n| (1..=65_535).contains(&n)));
    if nas.host.is_empty() {
        return None;
    }
    nas.enabled = Some(true);
    Some((nas, sealed))
}

/// Writes `nas` into `prefs` the way the fork reads them, replacing any NAS keys already there.
/// The password is never written.
pub fn put_nas_prefs(prefs: &mut Vec<Vec<u8>>, nas: &NasPrefs) {
    prefs.retain(|raw| key_of(raw).is_none_or(|k| !nas_keys::ALL.contains(&k.as_str())));
    let string = |key: &str, v: &str| {
        raw_pref(
            key,
            "StringPreferenceValue",
            <StringValue as prost::Message>::encode_to_vec(&StringValue {
                value: v.to_owned(),
            }),
        )
    };
    let boolean = |key: &str, v: bool| {
        raw_pref(
            key,
            "BooleanPreferenceValue",
            <BoolValue as prost::Message>::encode_to_vec(&BoolValue { value: v }),
        )
    };
    prefs.push(string(nas_keys::HOST, &nas.host));
    if let Some(v) = nas.enabled {
        prefs.push(boolean(nas_keys::ENABLED, v));
    }
    if let Some(v) = &nas.port {
        prefs.push(string(nas_keys::PORT, v));
    }
    if let Some(v) = nas.use_https {
        prefs.push(boolean(nas_keys::HTTPS, v));
    }
    if let Some(v) = &nas.folder {
        prefs.push(string(nas_keys::FOLDER, v));
    }
    if let Some(v) = &nas.username {
        prefs.push(string(nas_keys::USERNAME, v));
    }
    if let Some(mb) = nas.cache_mb.and_then(|mb| i32::try_from(mb).ok()) {
        prefs.push(raw_pref(
            nas_keys::CACHE_MB,
            "IntPreferenceValue",
            <IntValue as prost::Message>::encode_to_vec(&IntValue { value: mb }),
        ));
    }
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

    fn pref(key: &str, class: &str, body: Vec<u8>) -> Vec<u8> {
        raw_pref(key, class, body)
    }

    #[test]
    fn reads_the_phones_nas_settings_and_writes_them_back() {
        let prefs = vec![
            string_pref("remote_storage_host", " nas.local "),
            string_pref("remote_storage_port", "25601"),
            string_pref("remote_storage_folder", "Mihon"),
            string_pref("remote_storage_username", "reader"),
            pref(
                "remote_storage_enabled",
                "BooleanPreferenceValue",
                BoolValue { value: true }.encode_to_vec(),
            ),
            pref(
                "remote_storage_https",
                "BooleanPreferenceValue",
                BoolValue { value: false }.encode_to_vec(),
            ),
            pref(
                "remote_storage_cache_mb",
                "IntPreferenceValue",
                IntValue { value: 4352 }.encode_to_vec(),
            ),
            string_pref("display_remote_badge", "x"),
        ];
        let nas = nas_prefs(&prefs).unwrap();
        assert_eq!(
            nas,
            NasPrefs {
                enabled: Some(true),
                host: "nas.local".into(),
                port: Some("25601".into()),
                use_https: Some(false),
                folder: Some("Mihon".into()),
                username: Some("reader".into()),
                cache_mb: Some(4352),
            }
        );
        let mut out = vec![
            string_pref("remote_storage_host", "old"),
            string_pref("other", "kept"),
        ];
        put_nas_prefs(&mut out, &nas);
        assert_eq!(nas_prefs(&out), Some(nas));
        assert_eq!(
            out.iter()
                .filter(|r| key_of(r).as_deref() == Some("remote_storage_host"))
                .count(),
            1
        );
        assert!(out.iter().any(|r| key_of(r).as_deref() == Some("other")));
    }

    #[test]
    fn no_nas_settings_without_a_host_or_with_junk() {
        assert_eq!(nas_prefs(&[]), None);
        assert_eq!(nas_prefs(&[string_pref("remote_storage_host", "  ")]), None);
        assert_eq!(nas_prefs(&[vec![0xff, 0x00, 0x12]]), None);
        // A wrong type or a bad port is left out, not misread.
        let nas = nas_prefs(&[
            string_pref("remote_storage_host", "nas"),
            string_pref("remote_storage_enabled", "true"),
            string_pref("remote_storage_port", "99999"),
            string_pref("remote_storage_folder", "a\nb"),
        ])
        .unwrap();
        assert_eq!(nas.enabled, None);
        assert_eq!(nas.port, None);
        assert_eq!(nas.folder, None);
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

    fn len_field(tag: u8, bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![(tag << 3) | 2, u8::try_from(bytes.len()).unwrap()];
        out.extend_from_slice(bytes);
        out
    }

    #[test]
    fn the_nas_connection_section_is_read_with_its_sealed_password() {
        let sealed = crate::seal::seal("pw", "phrase", [3; 16], [4; 12]).unwrap();
        let mut raw = len_field(1, b" nas.local ");
        raw.extend([2 << 3, 0xb9, 0xc8, 0x01]); // port 25657 as a number
        raw.extend([3 << 3, 1]);
        raw.extend(len_field(4, b"Mihon"));
        raw.extend(len_field(5, b"reader"));
        raw.extend(len_field(6, &sealed));
        raw.extend([7 << 3, 0x05]); // stateTakenAt
        raw.extend(len_field(9, b"MIRROR")); // role, whatever its type
        let (nas, got) = nas_section(&raw).unwrap();
        assert_eq!(nas.host, "nas.local");
        assert_eq!(nas.port.as_deref(), Some("25657"));
        assert_eq!(nas.use_https, Some(true));
        assert_eq!(nas.folder.as_deref(), Some("Mihon"));
        assert_eq!(nas.username.as_deref(), Some("reader"));
        assert_eq!(got.as_deref(), Some(sealed.as_slice()));
    }

    #[test]
    fn a_broken_nas_section_is_ignored_not_fatal() {
        let raw = len_field(1, b"nas");
        for cut in 0..raw.len() {
            assert!(nas_section(raw.get(..cut).unwrap()).is_none());
        }
        // A password that is not sealed is never taken.
        let mut plain = len_field(1, b"nas");
        plain.extend(len_field(6, b"hunter2"));
        assert_eq!(nas_section(&plain).unwrap().1, None);
        assert!(nas_section(&[0xff; 12]).is_none());
    }
}
