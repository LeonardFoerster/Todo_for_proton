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
