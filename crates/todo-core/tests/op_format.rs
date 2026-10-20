//! Golden-File-Test: Das Binärformat der Ops darf sich nie unbemerkt ändern – Segmente anderer
//! Geräte und alte Outbox-Zeilen müssen lesbar bleiben.
//!
//! Neu erzeugen (nur bei bewusster, versionierter Formatänderung):
//! `UPDATE_GOLDEN=1 cargo test -p todo-core --test op_format`

use std::path::PathBuf;

use todo_core::{
    domain::{DeviceId, Due, NaiveDate, Priority, SortKey, TagId, TaskId, TaskListId, Timestamp},
    ports::EntityRef,
    sync::{decode_ops, encode_ops, FieldMap, FieldName, FieldValue, Hlc, Op, OpId, OpKind},
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

/// Feste Beispiel-Ops, die jede Op-Art und jeden Werttyp abdecken.
fn sample() -> Vec<Op> {
    let device: DeviceId = id("0192f0c4-7a3b-7c2d-8e4f-0123456789ab");
    let task = EntityRef::Task(id::<TaskId>("0192f0c4-7a3b-7c2d-8e4f-000000000001"));
    let list = EntityRef::TaskList(id::<TaskListId>("0192f0c4-7a3b-7c2d-8e4f-000000000002"));
    let tag: TagId = id("0192f0c4-7a3b-7c2d-8e4f-000000000003");
    let stamp = |seq: u64, kind| Op {
        id: OpId { device, seq },
        hlc: Hlc {
            wall_ms: 1_760_000_000_000 + seq,
            counter: u32::try_from(seq).unwrap(),
            device,
        },
        entity: if seq == 1 { list } else { task },
        kind,
    };
    let list_fields: FieldMap = [
        (FieldName::Name, FieldValue::Text("Arbeit".into())),
        (FieldName::SortKey, FieldValue::SortKey(SortKey::first())),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(Timestamp::from_millis(1_760_000_000_000)),
        ),
        (FieldName::IsInbox, FieldValue::Bool(false)),
    ]
    .into_iter()
    .collect();
    let task_fields: FieldMap = [
        (
            FieldName::ListId,
            FieldValue::ListId(id("0192f0c4-7a3b-7c2d-8e4f-000000000002")),
        ),
        (FieldName::ParentId, FieldValue::ParentId(None)),
        (FieldName::Title, FieldValue::Text("Milch kaufen".into())),
        (FieldName::Notes, FieldValue::Text("bio, 1 l".into())),
        (FieldName::Status, FieldValue::Status(None)),
        (FieldName::Priority, FieldValue::Priority(Priority::Medium)),
        (
            FieldName::Due,
            FieldValue::Due(Some(Due::Date(
                NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
            ))),
        ),
        (FieldName::Tags, FieldValue::Tags(vec![tag])),
        (
            FieldName::SortKey,
            FieldValue::SortKey(SortKey::parse("a0V").unwrap()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(Timestamp::from_millis(1_760_000_000_001)),
        ),
    ]
    .into_iter()
    .collect();
    vec![
        stamp(
            1,
            OpKind::Create {
                fields: list_fields,
            },
        ),
        stamp(
            2,
            OpKind::Create {
                fields: task_fields,
            },
        ),
        stamp(
            3,
            OpKind::Set {
                field: FieldName::Status,
                value: FieldValue::Status(Some(Timestamp::from_millis(1_760_000_100_000))),
            },
        ),
        stamp(
            4,
            OpKind::Set {
                field: FieldName::Due,
                value: FieldValue::Due(Some(Due::DateTime {
                    at: Timestamp::from_millis(1_760_100_000_000),
                    tz: "Europe/Berlin".into(),
                })),
            },
        ),
        stamp(5, OpKind::Delete),
    ]
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/ops_v1.bin")
}

#[test]
fn op_encoding_matches_golden_file() {
    let encoded = encode_ops(&sample());
    let path = golden_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &encoded).unwrap();
    }
    let golden = std::fs::read(&path).expect("golden file missing – see module docs");
    assert_eq!(encoded, golden, "Op-Binärformat hat sich geändert");
    assert_eq!(decode_ops(&golden).unwrap(), sample());
}

/// Notiz-Ops (ADR-029): neue Entity-Arten, Felder, Werte und `Revise`. Eigene Datei, damit
/// `ops_v1.bin` unverändert bleibt (alte Segmente sind weiter lesbar).
fn note_sample() -> Vec<Op> {
    use todo_core::domain::{NoteFolderId, NoteId, NoteKind};
    let device: DeviceId = id("0192f0c4-7a3b-7c2d-8e4f-0123456789ab");
    let note = EntityRef::Note(id::<NoteId>("0192f0c4-7a3b-7c2d-8e4f-000000000011"));
    let folder_id: NoteFolderId = id("0192f0c4-7a3b-7c2d-8e4f-000000000012");
    let folder = EntityRef::NoteFolder(folder_id);
    let stamp = |seq: u64, entity, kind| Op {
        id: OpId { device, seq },
        hlc: Hlc {
            wall_ms: 1_760_000_000_000 + seq,
            counter: 0,
            device,
        },
        entity,
        kind,
    };
    let folder_fields: FieldMap = [
        (FieldName::ParentFolder, FieldValue::Folder(None)),
        (FieldName::Name, FieldValue::Text("Uni".into())),
        (FieldName::SortKey, FieldValue::SortKey(SortKey::first())),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(Timestamp::from_millis(1_760_000_000_000)),
        ),
        (FieldName::Trashed, FieldValue::Trashed(None)),
    ]
    .into_iter()
    .collect();
    let note_fields: FieldMap = [
        (FieldName::ParentFolder, FieldValue::Folder(Some(folder_id))),
        (FieldName::AutoName, FieldValue::Bool(true)),
        (
            FieldName::NoteKind,
            FieldValue::NoteKind(NoteKind::Markdown),
        ),
        (
            FieldName::SortKey,
            FieldValue::SortKey(SortKey::parse("a0V").unwrap()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(Timestamp::from_millis(1_760_000_000_001)),
        ),
        (FieldName::Trashed, FieldValue::Trashed(None)),
    ]
    .into_iter()
    .collect();
    let first = OpId { device, seq: 3 };
    vec![
        stamp(
            1,
            folder,
            OpKind::Create {
                fields: folder_fields,
            },
        ),
        stamp(
            2,
            note,
            OpKind::Create {
                fields: note_fields,
            },
        ),
        stamp(
            3,
            note,
            OpKind::Revise {
                parents: Vec::new(),
                text: "# Blatt 3\nAufgabe 1".into(),
            },
        ),
        stamp(
            4,
            note,
            OpKind::Revise {
                parents: vec![first],
                text: "# Blatt 3\nAufgabe 1\nAufgabe 2".into(),
            },
        ),
        stamp(
            5,
            note,
            OpKind::Set {
                field: FieldName::NoteKind,
                value: FieldValue::NoteKind(NoteKind::Text),
            },
        ),
        stamp(
            6,
            folder,
            OpKind::Set {
                field: FieldName::Trashed,
                value: FieldValue::Trashed(Some(Timestamp::from_millis(1_760_000_200_000))),
            },
        ),
        stamp(7, note, OpKind::Delete),
    ]
}

#[test]
fn note_op_encoding_matches_golden_file() {
    let encoded = encode_ops(&note_sample());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/ops_v1_notes.bin");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &encoded).unwrap();
    }
    let golden = std::fs::read(&path).expect("golden file missing – see module docs");
    assert_eq!(encoded, golden, "Op-Binärformat hat sich geändert");
    assert_eq!(decode_ops(&golden).unwrap(), note_sample());
}

/// Eine App ohne Notizen kennt die neuen Varianten nicht: Dekodieren scheitert sauber (das
/// Segment wird übersprungen), es gibt keinen Absturz. Nachgestellt durch eine Variante, die
/// auch diese Version nicht kennt.
#[test]
fn unknown_variants_are_rejected_without_panic() {
    let mut bytes = encode_ops(&note_sample()[..1]);
    // Vec-Länge (1) + OpId (18) + HLC (29) → Varianten-Byte der Entity (NoteFolder = 3).
    assert_eq!(bytes[48], 3);
    bytes[48] = 200;
    assert!(decode_ops(&bytes).is_err());
}
