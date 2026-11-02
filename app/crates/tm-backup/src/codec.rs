//! The `.tachibk` / `.proto.gz` container: gzip around the [`Backup`] protobuf
//! (`BackupCodec.kt`). This is also what moves over the wire during a LAN sync.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use prost::Message;

use crate::{Backup, BackupError, Result};

/// Largest decompressed backup accepted. A library of tens of thousands of chapters is a few
/// megabytes; this only stops a hostile file from inflating without bound.
pub const MAX_DECODED: u64 = 256 * 1024 * 1024;

/// Encode to gzip + protobuf container bytes.
pub fn encode(backup: &Backup) -> Result<Vec<u8>> {
    let proto = backup.encode_to_vec();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&proto).map_err(BackupError::Io)?;
    gz.finish().map_err(BackupError::Io)
}

/// Decode container bytes. A raw (not gzipped) protobuf is accepted too, since some tools
/// write one.
pub fn decode(bytes: &[u8]) -> Result<Backup> {
    let proto = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_DECODED.saturating_add(1))
            .read_to_end(&mut out)
            .map_err(|e| BackupError::Format(format!("not a valid gzip backup: {e}")))?;
        if u64::try_from(out.len()).unwrap_or(u64::MAX) > MAX_DECODED {
            return Err(BackupError::Format("backup is too large".into()));
        }
        out
    } else {
        bytes.to_vec()
    };
    Backup::decode(proto.as_slice())
        .map_err(|e| BackupError::Format(format!("not a TachiMana/Mihon backup: {e}")))
}
