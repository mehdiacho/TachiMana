//! This install's stable identity and pairing code, in `device.json` (`DeviceIdentity.kt`).
//! The Kotlin app reads and writes the same file in the same shape.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    pub id: String,
    pub name: String,
    pub pairing_code: String,
}

impl DeviceIdentity {
    /// Load `file`, or create, save and return a new identity. A file that does not parse is
    /// replaced, like the Kotlin app does. Saving can fail (read-only data dir); the identity is
    /// still returned so sync works for this session.
    pub fn load_or_create(file: &Path) -> Self {
        if let Some(id) = std::fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str::<DeviceIdentity>(&t).ok())
            .filter(|d| !d.id.is_empty() && is_pairing_code(&d.pairing_code))
        {
            return id;
        }
        let me = Self {
            id: uuid_v4(),
            name: host_name(),
            pairing_code: new_pairing_code(),
        };
        match serde_json::to_string_pretty(&me) {
            Ok(text) => {
                if let Err(e) = std::fs::write(file, text) {
                    tracing::warn!("could not save {}: {e}", file.display());
                }
            }
            Err(e) => tracing::warn!("could not encode device identity: {e}"),
        }
        me
    }
}

/// Six ASCII digits.
pub fn is_pairing_code(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit())
}

fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    if getrandom::fill(&mut b).is_err() {
        // No OS randomness: fall back to the clock. Weak, but this is a LAN pairing code
        // the user can read off the screen, not a long-term secret.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        return u64::try_from(t & u128::from(u64::MAX)).unwrap_or(0);
    }
    u64::from_le_bytes(b)
}

pub fn new_pairing_code() -> String {
    format!("{:06}", random_u64() % 1_000_000)
}

fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    let hi = random_u64().to_le_bytes();
    let lo = random_u64().to_le_bytes();
    for (dst, src) in b.iter_mut().zip(hi.iter().chain(lo.iter())) {
        *dst = *src;
    }
    if let Some(x) = b.get_mut(6) {
        *x = (*x & 0x0f) | 0x40;
    }
    if let Some(x) = b.get_mut(8) {
        *x = (*x & 0x3f) | 0x80;
    }
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let part = |a: usize, z: usize| h.get(a..z).unwrap_or("");
    format!(
        "{}-{}-{}-{}-{}",
        part(0, 8),
        part(8, 12),
        part(12, 16),
        part(16, 20),
        part(20, 32)
    )
}

fn host_name() -> String {
    let from_env = ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok());
    let from_file = || {
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_owned())
    };
    from_env
        .or_else(from_file)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Desktop".to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn creates_once_then_reloads_the_same_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("device.json");
        let a = DeviceIdentity::load_or_create(&f);
        assert!(is_pairing_code(&a.pairing_code));
        assert_eq!(a.id.len(), 36);
        assert_eq!(DeviceIdentity::load_or_create(&f), a);
    }

    #[test]
    fn reads_the_kotlin_file() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("device.json");
        std::fs::write(
            &f,
            "{\n    \"id\": \"abc\",\n    \"name\": \"PC\",\n    \"pairingCode\": \"012345\"\n}",
        )
        .unwrap();
        let d = DeviceIdentity::load_or_create(&f);
        assert_eq!((d.id.as_str(), d.pairing_code.as_str()), ("abc", "012345"));
        // And writes the same keys.
        let text = serde_json::to_string(&d).unwrap();
        assert!(text.contains("\"pairingCode\":\"012345\""));
    }

    #[test]
    fn a_broken_file_is_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("device.json");
        std::fs::write(&f, "{not json").unwrap();
        let d = DeviceIdentity::load_or_create(&f);
        assert!(is_pairing_code(&d.pairing_code));
        assert_eq!(DeviceIdentity::load_or_create(&f), d);
    }
}
