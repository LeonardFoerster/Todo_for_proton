//! Segment-Format: eine unveränderliche, komprimierte und verschlüsselte Op-Liste eines Geräts
//! (`sync-strategy.md` §3).
//!
//! ```text
//! Header (40 Bytes, Klartext, zugleich AAD):
//!   magic "TSEG" (4) ‖ format_version (1) ‖ codec (1, 1 = zstd) ‖ reserviert (2, = 0)
//!   ‖ device_id (16) ‖ seq_from (u64 BE) ‖ seq_to (u64 BE)
//! Body: Cipher::seal(key, zstd(postcard(Vec<Op>)), aad = Header)
//! ```
//!
//! Der Header als AAD bindet den Inhalt an Gerät und Op-Bereich: Ein Segment lässt sich weder
//! einem anderen Gerät unterschieben noch unter anderem Bereich wiederverwenden.

use std::fmt;

use todo_core::{
    domain::DeviceId,
    ports::{Cipher, SecretBytes},
    sync::{decode_ops, encode_ops, Op},
    CoreError, CoreResult,
};

use crate::compression;

/// Kennung am Dateianfang.
pub const MAGIC: [u8; 4] = *b"TSEG";
/// Aktuelle Formatversion. Neuere Versionen werden nicht gelesen ([`SegmentError::NewerFormat`]).
pub const FORMAT_VERSION: u8 = 1;
/// Länge des Headers in Bytes.
pub const HEADER_LEN: usize = 40;
/// Höchstens so viele Ops pro Segment (`sync-strategy.md` §4).
pub const MAX_OPS: usize = 500;
/// Dateiendung im Backend.
pub const EXTENSION: &str = "seg";

const CODEC_ZSTD: u8 = 1;

/// Fehler beim Lesen eines Segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentError {
    /// Von einer neueren App-Version geschrieben → überspringen, Hinweis „App aktualisieren".
    NewerFormat(u8),
    /// Beschädigt, manipuliert oder mit anderem Schlüssel verschlüsselt → überspringen.
    Corrupt(String),
}

impl fmt::Display for SegmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewerFormat(v) => write!(f, "segment format {v} is newer than supported"),
            Self::Corrupt(why) => write!(f, "corrupt segment: {why}"),
        }
    }
}

impl std::error::Error for SegmentError {}

impl From<SegmentError> for CoreError {
    fn from(err: SegmentError) -> Self {
        CoreError::Validation(err.to_string())
    }
}

/// Kopf eines Segments: wessen Ops, welcher Bereich.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentHeader {
    pub device: DeviceId,
    pub seq_from: u64,
    pub seq_to: u64,
}

impl SegmentHeader {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0; HEADER_LEN];
        out[..4].copy_from_slice(&MAGIC);
        out[4] = FORMAT_VERSION;
        out[5] = CODEC_ZSTD;
        out[8..24].copy_from_slice(&self.device.to_bytes());
        out[24..32].copy_from_slice(&self.seq_from.to_be_bytes());
        out[32..40].copy_from_slice(&self.seq_to.to_be_bytes());
        out
    }

    /// Liest und prüft den Header am Anfang von `bytes`.
    ///
    /// # Errors
    /// [`SegmentError`] bei fremdem Format, neuerer Version oder ungültigem Bereich.
    pub fn parse(bytes: &[u8]) -> Result<Self, SegmentError> {
        let corrupt = |why: &str| SegmentError::Corrupt(why.to_owned());
        if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC {
            return Err(corrupt("not a segment"));
        }
        if bytes[4] > FORMAT_VERSION {
            return Err(SegmentError::NewerFormat(bytes[4]));
        }
        if bytes[4] != FORMAT_VERSION || bytes[5] != CODEC_ZSTD || bytes[6..8] != [0, 0] {
            return Err(corrupt("unsupported header fields"));
        }
        let u64_at = |at: usize| u64::from_be_bytes(bytes[at..at + 8].try_into().expect("8 bytes"));
        let header = Self {
            device: DeviceId::from_bytes(bytes[8..24].try_into().expect("16 bytes")),
            seq_from: u64_at(24),
            seq_to: u64_at(32),
        };
        if header.seq_from == 0 || header.seq_from > header.seq_to {
            return Err(corrupt("invalid seq range"));
        }
        Ok(header)
    }

    /// Dateiname im Ordner des Geräts: `<seq_from>-<seq_to>.seg`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}-{}.{EXTENSION}", self.seq_from, self.seq_to)
    }

    /// Op-Bereich aus einem Dateinamen; `None` für fremde Dateien.
    #[must_use]
    pub fn parse_file_name(name: &str) -> Option<(u64, u64)> {
        let range = name.strip_suffix(EXTENSION)?.strip_suffix('.')?;
        let (from, to) = range.split_once('-')?;
        let (from, to) = (from.parse().ok()?, to.parse().ok()?);
        (from >= 1 && from <= to).then_some((from, to))
    }

    fn count(&self) -> u64 {
        self.seq_to - self.seq_from + 1
    }
}

/// Baut ein Segment aus lückenlos aufeinanderfolgenden Ops **eines** Geräts.
///
/// # Errors
/// `CoreError::Validation` bei leerer, gemischter oder lückenhafter Op-Liste bzw. mehr als
/// [`MAX_OPS`] Ops; Fehler von Kompression oder Cipher.
pub fn seal_segment(
    cipher: &dyn Cipher,
    key: &SecretBytes,
    ops: &[Op],
) -> CoreResult<(SegmentHeader, Vec<u8>)> {
    let (Some(first), Some(last)) = (ops.first(), ops.last()) else {
        return Err(CoreError::Validation(
            "segment needs at least one op".into(),
        ));
    };
    if ops.len() > MAX_OPS {
        return Err(CoreError::Validation(format!(
            "segment holds at most {MAX_OPS} ops, got {}",
            ops.len()
        )));
    }
    let header = SegmentHeader {
        device: first.id.device,
        seq_from: first.id.seq,
        seq_to: last.id.seq,
    };
    check_ops(&header, ops).map_err(CoreError::from)?;
    let packed = compression::compress(&encode_ops(ops))?;
    let aad = header.to_bytes();
    let sealed = cipher.seal(key, &packed, &aad)?;
    let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
    out.extend_from_slice(&aad);
    out.extend_from_slice(&sealed);
    Ok((header, out))
}

/// Liest, entschlüsselt und prüft ein Segment.
///
/// # Errors
/// [`SegmentError::NewerFormat`] bzw. [`SegmentError::Corrupt`] (auch bei falschem Schlüssel).
pub fn open_segment(
    cipher: &dyn Cipher,
    key: &SecretBytes,
    bytes: &[u8],
) -> Result<(SegmentHeader, Vec<Op>), SegmentError> {
    let header = SegmentHeader::parse(bytes)?;
    let corrupt = |e: CoreError| SegmentError::Corrupt(e.to_string());
    let packed = cipher
        .open(key, &bytes[HEADER_LEN..], &bytes[..HEADER_LEN])
        .map_err(corrupt)?;
    let ops = decode_ops(&compression::decompress(&packed).map_err(corrupt)?).map_err(corrupt)?;
    check_ops(&header, &ops)?;
    Ok((header, ops))
}

/// Ops müssen genau den Bereich des Headers lückenlos und in Reihenfolge abdecken.
fn check_ops(header: &SegmentHeader, ops: &[Op]) -> Result<(), SegmentError> {
    let in_order = ops
        .iter()
        .zip(header.seq_from..=header.seq_to)
        .all(|(op, seq)| {
            op.id.device == header.device && op.hlc.device == header.device && op.id.seq == seq
        });
    if in_order && ops.len() as u64 == header.count() {
        Ok(())
    } else {
        Err(SegmentError::Corrupt(format!(
            "ops do not match range {}-{} of device {}",
            header.seq_from, header.seq_to, header.device
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use todo_core::{
        domain::{SortKey, Task, TaskListId, Timestamp},
        ports::{EntityRef, FixedClock},
        sync::{task_fields, HlcState, OpClock, OpKind},
    };
    use todo_crypto::XChaCha20Poly1305Cipher;

    use super::*;

    fn ops(device: DeviceId, n: usize) -> Vec<Op> {
        let mut clock = OpClock::new(HlcState::new(device), 0);
        let fixed = FixedClock(Timestamp::from_millis(1));
        (0..n)
            .map(|i| {
                let task = Task::new(
                    TaskListId::INBOX,
                    &format!("Task {i}"),
                    SortKey::first(),
                    &fixed,
                )
                .unwrap();
                clock.stamp(
                    Timestamp::from_millis(1_000),
                    EntityRef::Task(task.id()),
                    OpKind::Create {
                        fields: task_fields(&task),
                    },
                )
            })
            .collect()
    }

    fn setup() -> (XChaCha20Poly1305Cipher, SecretBytes) {
        let cipher = XChaCha20Poly1305Cipher::new();
        let key = cipher.generate_key().unwrap();
        (cipher, key)
    }

    #[test]
    fn roundtrip() {
        let (cipher, key) = setup();
        let device = DeviceId::new();
        let ops = ops(device, 3);
        let (header, bytes) = seal_segment(&cipher, &key, &ops).unwrap();
        assert_eq!(
            header,
            SegmentHeader {
                device,
                seq_from: 1,
                seq_to: 3
            }
        );
        assert_eq!(header.file_name(), "1-3.seg");
        assert_eq!(open_segment(&cipher, &key, &bytes).unwrap(), (header, ops));
    }

    #[test]
    fn many_ops_compress_well() {
        let (cipher, key) = setup();
        let ops = ops(DeviceId::new(), MAX_OPS);
        let raw = encode_ops(&ops).len();
        let (_, bytes) = seal_segment(&cipher, &key, &ops).unwrap();
        assert!(bytes.len() < raw / 2, "{} von {raw} Bytes", bytes.len());
        assert!(seal_segment(&cipher, &key, &self::ops(DeviceId::new(), MAX_OPS + 1)).is_err());
    }

    #[test]
    fn any_tampered_byte_is_rejected() {
        let (cipher, key) = setup();
        let (_, bytes) = seal_segment(&cipher, &key, &ops(DeviceId::new(), 2)).unwrap();
        for i in 0..bytes.len() {
            let mut tampered = bytes.clone();
            tampered[i] ^= 0x01;
            assert!(open_segment(&cipher, &key, &tampered).is_err(), "Byte {i}");
        }
    }

    #[test]
    fn header_of_another_device_or_range_fails_as_aad() {
        let (cipher, key) = setup();
        let (header, bytes) = seal_segment(&cipher, &key, &ops(DeviceId::new(), 2)).unwrap();
        let swap = |other: SegmentHeader| {
            let mut forged = other.to_bytes().to_vec();
            forged.extend_from_slice(&bytes[HEADER_LEN..]);
            open_segment(&cipher, &key, &forged)
        };
        let other_device = swap(SegmentHeader {
            device: DeviceId::new(),
            ..header
        });
        assert!(matches!(other_device, Err(SegmentError::Corrupt(_))));
        let other_range = swap(SegmentHeader {
            seq_from: 3,
            seq_to: 4,
            ..header
        });
        assert!(matches!(other_range, Err(SegmentError::Corrupt(_))));
    }

    #[test]
    fn wrong_key_is_corrupt() {
        let (cipher, key) = setup();
        let (_, bytes) = seal_segment(&cipher, &key, &ops(DeviceId::new(), 1)).unwrap();
        let other = cipher.generate_key().unwrap();
        assert!(matches!(
            open_segment(&cipher, &other, &bytes),
            Err(SegmentError::Corrupt(_))
        ));
    }

    #[test]
    fn newer_format_is_reported_separately() {
        let (cipher, key) = setup();
        let (_, mut bytes) = seal_segment(&cipher, &key, &ops(DeviceId::new(), 1)).unwrap();
        bytes[4] = FORMAT_VERSION + 1;
        assert_eq!(
            open_segment(&cipher, &key, &bytes),
            Err(SegmentError::NewerFormat(FORMAT_VERSION + 1))
        );
        assert!(SegmentHeader::parse(b"PK\x03\x04").is_err());
    }

    #[test]
    fn rejects_mixed_or_gappy_ops() {
        let (cipher, key) = setup();
        let mut mixed = ops(DeviceId::new(), 1);
        mixed.extend(ops(DeviceId::new(), 1));
        assert!(seal_segment(&cipher, &key, &mixed).is_err());
        let mut gappy = ops(DeviceId::new(), 3);
        gappy.remove(1);
        assert!(seal_segment(&cipher, &key, &gappy).is_err());
        assert!(seal_segment(&cipher, &key, &[]).is_err());
    }

    #[test]
    fn file_names() {
        assert_eq!(SegmentHeader::parse_file_name("1-500.seg"), Some((1, 500)));
        for bad in [
            "1-500.snap",
            "0-3.seg",
            "5-3.seg",
            "x-3.seg",
            "1-3",
            "1.seg",
            ".tmp-1-3.seg",
        ] {
            assert_eq!(SegmentHeader::parse_file_name(bad), None, "{bad}");
        }
    }

    /// Golden File: Das Header-Layout darf sich nicht unbemerkt ändern.
    /// Neu erzeugen: `UPDATE_GOLDEN=1 cargo test -p todo-sync segment`.
    #[test]
    fn header_matches_golden_file() {
        let header = SegmentHeader {
            device: DeviceId::parse("0192f0c4-7a3b-7c2d-8e4f-0123456789ab").unwrap(),
            seq_from: 1,
            seq_to: 500,
        };
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/segment_header_v1.bin");
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, header.to_bytes()).unwrap();
        }
        let golden = std::fs::read(&path).expect("golden file");
        assert_eq!(header.to_bytes().as_slice(), golden.as_slice());
        assert_eq!(SegmentHeader::parse(&golden).unwrap(), header);
    }
}
