//! The NAS password sealed with a passphrase, as Mehdi's Mihon fork writes it into a backup's
//! "NAS connection" section (tag 901, field 6; agreed in TachiMana#35):
//!
//! `"TMP1"` ‖ salt (16) ‖ nonce (12) ‖ AES-256-GCM ciphertext and tag, with the key from
//! Argon2id(passphrase, salt, m = 64 MiB, t = 3, p = 1, 32 bytes).
//!
//! The passphrase is never stored; it unlocks the password once, on restore.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"TMP1";
const SALT: usize = 16;
const NONCE: usize = 12;
const TAG: usize = 16;
/// A sealed password larger than this is not a password.
const MAX_SEALED: usize = 64 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SealError {
    #[error("this is not a sealed NAS password")]
    NotSealed,
    #[error("that passphrase doesn't unlock the NAS password")]
    WrongPassphrase,
    #[error("couldn't derive the key: {0}")]
    Key(String),
}

fn key(passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, SealError> {
    let params = argon2::Params::new(64 * 1024, 3, 1, Some(32))
        .map_err(|e| SealError::Key(e.to_string()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    a.hash_password_into(passphrase.as_bytes(), salt, out.as_mut_slice())
        .map_err(|e| SealError::Key(e.to_string()))?;
    Ok(out)
}

/// Whether `blob` has the sealed-password layout (it may still not open).
pub fn is_sealed(blob: &[u8]) -> bool {
    blob.len() >= MAGIC.len() + SALT + NONCE + TAG
        && blob.len() <= MAX_SEALED
        && blob.starts_with(MAGIC)
}

/// Opens a sealed password with the person's passphrase. Slow on purpose (Argon2id, 64 MiB):
/// call it off the UI thread.
pub fn unseal(blob: &[u8], passphrase: &str) -> Result<Zeroizing<String>, SealError> {
    if !is_sealed(blob) {
        return Err(SealError::NotSealed);
    }
    let rest = blob.get(MAGIC.len()..).ok_or(SealError::NotSealed)?;
    let (salt, rest) = rest.split_at_checked(SALT).ok_or(SealError::NotSealed)?;
    let (nonce, sealed) = rest.split_at_checked(NONCE).ok_or(SealError::NotSealed)?;
    let k = key(passphrase, salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(k.as_slice()).map_err(|e| SealError::Key(e.to_string()))?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(Nonce::from_slice(nonce), sealed)
            .map_err(|_| SealError::WrongPassphrase)?,
    );
    String::from_utf8(plain.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| SealError::WrongPassphrase)
}

/// Seals `password` the way the phone does, with the given salt and nonce (tests and tooling;
/// the computer never writes a sealed password).
pub fn seal(
    password: &str,
    passphrase: &str,
    salt: [u8; SALT],
    nonce: [u8; NONCE],
) -> Result<Vec<u8>, SealError> {
    let k = key(passphrase, &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(k.as_slice()).map_err(|e| SealError::Key(e.to_string()))?;
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), password.as_bytes())
        .map_err(|e| SealError::Key(e.to_string()))?;
    let mut out = Vec::with_capacity(MAGIC.len() + SALT + NONCE + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_password_opens_only_with_its_passphrase() {
        let blob = seal("hunter2", "correct horse", [7; 16], [9; 12]).unwrap();
        assert!(blob.starts_with(b"TMP1"));
        assert_eq!(blob.len(), 4 + 16 + 12 + "hunter2".len() + 16);
        assert_eq!(unseal(&blob, "correct horse").unwrap().as_str(), "hunter2");
        assert_eq!(
            unseal(&blob, "wrong").unwrap_err(),
            SealError::WrongPassphrase
        );
    }

    #[test]
    fn damaged_or_foreign_blobs_are_refused_without_panicking() {
        let blob = seal("pw", "p", [1; 16], [2; 12]).unwrap();
        for cut in 0..blob.len() {
            assert!(unseal(blob.get(..cut).unwrap(), "p").is_err());
        }
        let mut flipped = blob.clone();
        if let Some(b) = flipped.last_mut() {
            *b ^= 1;
        }
        assert_eq!(
            unseal(&flipped, "p").unwrap_err(),
            SealError::WrongPassphrase
        );
        assert_eq!(unseal(b"TMP2 nope", "p").unwrap_err(), SealError::NotSealed);
    }
}
