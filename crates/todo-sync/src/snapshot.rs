//! Snapshot-Format: der komplette Sync-Zustand eines Geräts plus der Vektor der enthaltenen Ops
//! (`sync-strategy.md` §3–§4). Neue Geräte starten damit, statt alle Segmente zu lesen; danach
//! dürfen alte Segmente gelöscht werden.
//!
//! ```text
//! Header (52 Bytes, Klartext, zugleich AAD):
//!   magic "TSNP" (4) ‖ format_version (1) ‖ codec (1, 1 = zstd) ‖ reserviert (2)
//!   ‖ device_id (16) ‖ hlc (28)
//! Body: Cipher::seal(key, zstd(postcard(SnapshotBody)), aad = Header)
//! Datei: snapshots/<hlc>-<device>.snap   (Name sortiert nach Alter, Geräte kollidieren nie)
//! ```
//!
//! Importiert wird über denselben Merge wie Ops: Jedes Register wird zu einem `Set` mit seiner
//! HLC, jeder Tombstone zu einem `Delete` ([`SnapshotBody::to_ops`]). Das ist ein State-Join –
//! er schadet auch bei vorhandenen lokalen Daten nicht.

use serde::{Deserialize, Serialize};
use todo_core::{
    domain::DeviceId,
    ports::{Cipher, EntityRef, SecretBytes},
    sync::{EntityState, Hlc, Op, OpId, OpKind},
    CoreError, CoreResult,
};

use crate::{compression, segment::SegmentError};

pub const MAGIC: [u8; 4] = *b"TSNP";
pub const FORMAT_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 52;
pub const EXTENSION: &str = "snap";

const CODEC_ZSTD: u8 = 1;

/// Kopf eines Snapshots: wer, wann.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotHeader {
    pub device: DeviceId,
    pub hlc: Hlc,
}

/// Inhalt eines Snapshots. Teil des Formats: Felder nur anhängen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotBody {
    /// Pro Gerät: Nummer, bis zu der dessen Ops sicher enthalten sind.
    pub vector: Vec<(DeviceId, u64)>,
    /// Register bzw. Tombstone jeder Entity.
    pub entities: Vec<(EntityRef, EntityState)>,
}

impl SnapshotHeader {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0; HEADER_LEN];
        out[..4].copy_from_slice(&MAGIC);
        out[4] = FORMAT_VERSION;
        out[5] = CODEC_ZSTD;
        out[8..24].copy_from_slice(&self.device.to_bytes());
        out[24..52].copy_from_slice(&self.hlc.to_bytes());
        out
    }

    /// # Errors
    /// [`SegmentError`] bei fremdem Format oder neuerer Version.
    pub fn parse(bytes: &[u8]) -> Result<Self, SegmentError> {
        let corrupt = |why: &str| SegmentError::Corrupt(why.to_owned());
        if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC {
            return Err(corrupt("not a snapshot"));
        }
        if bytes[4] > FORMAT_VERSION {
            return Err(SegmentError::NewerFormat(bytes[4]));
        }
        if bytes[4] != FORMAT_VERSION || bytes[5] != CODEC_ZSTD || bytes[6..8] != [0, 0] {
            return Err(corrupt("unsupported header fields"));
        }
        let header = Self {
            device: DeviceId::from_bytes(bytes[8..24].try_into().expect("16 bytes")),
            hlc: Hlc::from_bytes(&bytes[24..52]).map_err(|e| corrupt(&e.to_string()))?,
        };
        if header.hlc.device != header.device {
            return Err(corrupt("hlc of another device"));
        }
        Ok(header)
    }

    /// `<hlc>-<device>.snap`; der String-Vergleich der Namen entspricht dem Alter.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}-{}.{EXTENSION}", self.hlc, self.device)
    }

    /// HLC aus einem Dateinamen (zum Sortieren); `None` für fremde Dateien.
    #[must_use]
    pub fn parse_file_name(name: &str) -> Option<(Hlc, DeviceId)> {
        let stem = name.strip_suffix(EXTENSION)?.strip_suffix('.')?;
        let (hlc, device) = stem.split_once('-')?;
        let hlc = Hlc::parse(hlc).ok()?;
        let device = DeviceId::parse(device).ok()?;
        (hlc.device == device).then_some((hlc, device))
    }
}

impl SnapshotBody {
    /// Übersetzt den Zustand in Ops für `apply_remote` (siehe Moduldoku). Die Op-Nummer ist 0:
    /// Diese Ops landen nie in einer Outbox oder einem Segment.
    #[must_use]
    pub fn to_ops(&self) -> Vec<Op> {
        let mut ops = Vec::new();
        for (entity, state) in &self.entities {
            let op = |hlc: Hlc, kind| Op {
                id: OpId {
                    device: hlc.device,
                    seq: 0,
                },
                hlc,
                entity: *entity,
                kind,
            };
            match state {
                EntityState::Deleted(hlc) => ops.push(op(*hlc, OpKind::Delete)),
                EntityState::Live(registers) => {
                    ops.extend(registers.iter().map(|(field, register)| {
                        op(
                            register.hlc,
                            OpKind::Set {
                                field: *field,
                                value: register.value.clone(),
                            },
                        )
                    }));
                }
            }
        }
        ops
    }
}

/// # Errors
/// Fehler von Kompression oder Cipher.
pub fn seal_snapshot(
    cipher: &dyn Cipher,
    key: &SecretBytes,
    header: &SnapshotHeader,
    body: &SnapshotBody,
) -> CoreResult<Vec<u8>> {
    let encoded = postcard::to_allocvec(body)
        .map_err(|e| CoreError::Internal(format!("snapshot encoding: {e}")))?;
    let aad = header.to_bytes();
    let sealed = cipher.seal(key, &compression::compress(&encoded)?, &aad)?;
    let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
    out.extend_from_slice(&aad);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// # Errors
/// [`SegmentError::NewerFormat`] bzw. [`SegmentError::Corrupt`] (auch bei falschem Schlüssel).
pub fn open_snapshot(
    cipher: &dyn Cipher,
    key: &SecretBytes,
    bytes: &[u8],
) -> Result<(SnapshotHeader, SnapshotBody), SegmentError> {
    let header = SnapshotHeader::parse(bytes)?;
    let corrupt = |e: CoreError| SegmentError::Corrupt(e.to_string());
    let packed = cipher
        .open(key, &bytes[HEADER_LEN..], &bytes[..HEADER_LEN])
        .map_err(corrupt)?;
    let plain = compression::decompress(&packed).map_err(corrupt)?;
    let body: SnapshotBody = postcard::from_bytes(&plain)
        .map_err(|e| SegmentError::Corrupt(format!("snapshot encoding: {e}")))?;
    Ok((header, body))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use todo_core::{
        domain::{SortKey, Task, TaskListId, Timestamp},
        ports::FixedClock,
        sync::{merge, task_fields, HlcState, OpClock},
    };
    use todo_crypto::XChaCha20Poly1305Cipher;

    use super::*;

    fn header(device: DeviceId) -> SnapshotHeader {
        SnapshotHeader {
            device,
            hlc: Hlc {
                wall_ms: 1_792_000_000_000,
                counter: 3,
                device,
            },
        }
    }

    fn body() -> SnapshotBody {
        let device = DeviceId::new();
        let mut clock = OpClock::new(HlcState::new(device), 0);
        let task = Task::new(
            TaskListId::INBOX,
            "Milch",
            SortKey::first(),
            &FixedClock(Timestamp::from_millis(1)),
        )
        .unwrap();
        let entity = EntityRef::Task(task.id());
        let create = clock.stamp(
            Timestamp::from_millis(5),
            entity,
            OpKind::Create {
                fields: task_fields(&task),
            },
        );
        let live = merge(None, &[create]).state.unwrap();
        let gone = EntityRef::Task(todo_core::domain::TaskId::new());
        let deleted = EntityState::Deleted(
            clock
                .stamp(Timestamp::from_millis(6), gone, OpKind::Delete)
                .hlc,
        );
        SnapshotBody {
            vector: vec![(device, 2)],
            entities: vec![(entity, live), (gone, deleted)],
        }
    }

    #[test]
    fn roundtrip_and_tamper_detection() {
        let cipher = XChaCha20Poly1305Cipher::new();
        let key = cipher.generate_key().unwrap();
        let header = header(DeviceId::new());
        let body = body();
        let bytes = seal_snapshot(&cipher, &key, &header, &body).unwrap();
        assert_eq!(
            open_snapshot(&cipher, &key, &bytes).unwrap(),
            (header, body)
        );
        let mut tampered = bytes.clone();
        tampered[30] ^= 1;
        assert!(
            open_snapshot(&cipher, &key, &tampered).is_err(),
            "Header ist AAD"
        );
        let mut tampered = bytes;
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(open_snapshot(&cipher, &key, &tampered).is_err());
    }

    #[test]
    fn to_ops_rebuilds_the_same_state() {
        let body = body();
        for (entity, state) in &body.entities {
            let ops: Vec<Op> = body
                .to_ops()
                .into_iter()
                .filter(|op| op.entity == *entity)
                .collect();
            assert_eq!(merge(None, &ops).state.as_ref(), Some(state));
        }
    }

    #[test]
    fn file_names_sort_by_age() {
        let device = DeviceId::new();
        let old = header(device);
        let new = SnapshotHeader {
            hlc: Hlc {
                wall_ms: old.hlc.wall_ms + 1,
                ..old.hlc
            },
            ..old
        };
        assert!(old.file_name() < new.file_name());
        assert_eq!(
            SnapshotHeader::parse_file_name(&new.file_name()),
            Some((new.hlc, device))
        );
        assert_eq!(SnapshotHeader::parse_file_name("1-3.seg"), None);
        let foreign = format!("{}-{}.snap", old.hlc, DeviceId::new());
        assert_eq!(SnapshotHeader::parse_file_name(&foreign), None);
    }

    #[test]
    fn newer_format_is_reported() {
        let mut bytes = header(DeviceId::new()).to_bytes().to_vec();
        bytes[4] = FORMAT_VERSION + 1;
        assert_eq!(
            SnapshotHeader::parse(&bytes),
            Err(SegmentError::NewerFormat(FORMAT_VERSION + 1))
        );
    }

    /// Golden File für das Header-Layout. Neu erzeugen: `UPDATE_GOLDEN=1 cargo test -p todo-sync snapshot`.
    #[test]
    fn header_matches_golden_file() {
        let header = header(DeviceId::parse("0192f0c4-7a3b-7c2d-8e4f-0123456789ab").unwrap());
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/snapshot_header_v1.bin");
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, header.to_bytes()).unwrap();
        }
        let golden = std::fs::read(&path).expect("golden file");
        assert_eq!(header.to_bytes().as_slice(), golden.as_slice());
        assert_eq!(SnapshotHeader::parse(&golden).unwrap(), header);
    }
}
