//! Ops: feldgranulare, unveränderliche Änderungen – die Einheit des Syncs (`sync-strategy.md` §2).
//!
//! # Format
//!
//! Kanonisches Binärformat ist `postcard` (ADR-016): kompakt, schnell, aber nicht selbst-
//! beschreibend. Deshalb gilt für alle Typen hier – und für [`EntityRef`] –:
//! **Enum-Varianten und Struct-Felder nie umsortieren oder entfernen, nur hinten anhängen.**
//! [`FieldName`] wird über feste numerische Codes kodiert, nicht über die Variantenposition.
//! Ein Golden-File-Test (`tests/op_format.rs`) bricht, sobald sich Bytes ändern.

use std::collections::BTreeMap;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::Hlc;
use crate::{
    domain::{DeviceId, Due, Priority, SortKey, TagId, TaskId, TaskListId, Timestamp},
    ports::EntityRef,
    CoreError, CoreResult,
};

/// Eindeutige Op-Kennung: Gerät plus dessen fortlaufende Nummer (ab 1, lückenlos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OpId {
    pub device: DeviceId,
    pub seq: u64,
}

/// Eine Änderung an genau einer Entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    /// Bestimmt beim Mergen, welcher Wert gewinnt (feldweise Last-Writer-Wins).
    pub hlc: Hlc,
    pub entity: EntityRef,
    pub kind: OpKind,
}

/// Was eine Op tut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    /// Legt die Entity mit diesen Feldwerten an (alle mit der HLC der Op).
    Create { fields: FieldMap },
    /// Setzt ein einzelnes Feld.
    Set { field: FieldName, value: FieldValue },
    /// Löscht die Entity (Tombstone; gewinnt gegen alle Sets, auch spätere).
    Delete,
    // Später (OR-Set für Tags): AddToSet, RemoveFromSet – nur anhängen.
}

/// Feldwerte einer Entity, nach [`FieldName`] sortiert (deterministische Kodierung).
pub type FieldMap = BTreeMap<FieldName, FieldValue>;

/// Ein synchronisiertes Feld. Kodiert als fester Code ([`FieldName::code`]), nie als
/// Variantenposition. `updated_at` ist bewusst kein Feld (gerätelokal, ADR-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldName {
    // Task
    ListId,
    ParentId,
    Title,
    Notes,
    /// Erledigt-Status inkl. `completed_at` (ein Feld, damit beides zusammen gewinnt).
    Status,
    Priority,
    Due,
    Tags,
    // Task und TaskList
    SortKey,
    CreatedAt,
    // TaskList
    Name,
    IsInbox,
}

impl FieldName {
    /// Alle Felder (für Tests und Validierung).
    pub const ALL: [Self; 12] = [
        Self::ListId,
        Self::ParentId,
        Self::Title,
        Self::Notes,
        Self::Status,
        Self::Priority,
        Self::Due,
        Self::Tags,
        Self::SortKey,
        Self::CreatedAt,
        Self::Name,
        Self::IsInbox,
    ];

    /// Felder einer Task.
    pub const TASK: [Self; 10] = [
        Self::ListId,
        Self::ParentId,
        Self::Title,
        Self::Notes,
        Self::Status,
        Self::Priority,
        Self::Due,
        Self::Tags,
        Self::SortKey,
        Self::CreatedAt,
    ];

    /// Felder einer Liste.
    pub const TASK_LIST: [Self; 4] = [Self::Name, Self::SortKey, Self::CreatedAt, Self::IsInbox];

    /// Stabiler Code im Binärformat. **Nie ändern oder wiederverwenden.**
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::ListId => 1,
            Self::ParentId => 2,
            Self::Title => 3,
            Self::Notes => 4,
            Self::Status => 5,
            Self::Priority => 6,
            Self::Due => 7,
            Self::Tags => 8,
            Self::SortKey => 9,
            Self::CreatedAt => 10,
            Self::Name => 11,
            Self::IsInbox => 12,
        }
    }

    /// Gegenstück zu [`FieldName::code`]; `None` für unbekannte Codes (neuere App-Version).
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.code() == code)
    }

    /// Gehört das Feld zu dieser Art Entity?
    #[must_use]
    pub fn applies_to(self, entity: &EntityRef) -> bool {
        match entity {
            EntityRef::Task(_) => Self::TASK.contains(&self),
            EntityRef::TaskList(_) => Self::TASK_LIST.contains(&self),
        }
    }

    /// Passt der Wert vom Typ her zu diesem Feld?
    #[must_use]
    pub fn accepts(self, value: &FieldValue) -> bool {
        matches!(
            (self, value),
            (Self::ListId, FieldValue::ListId(_))
                | (Self::ParentId, FieldValue::ParentId(_))
                | (Self::Title | Self::Notes | Self::Name, FieldValue::Text(_))
                | (Self::Status, FieldValue::Status(_))
                | (Self::Priority, FieldValue::Priority(_))
                | (Self::Due, FieldValue::Due(_))
                | (Self::Tags, FieldValue::Tags(_))
                | (Self::SortKey, FieldValue::SortKey(_))
                | (Self::CreatedAt, FieldValue::Timestamp(_))
                | (Self::IsInbox, FieldValue::Bool(_))
        )
    }
}

impl Serialize for FieldName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for FieldName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        Self::from_code(code).ok_or_else(|| de::Error::custom(format!("unknown field code {code}")))
    }
}

/// Ein Feldwert. Welche Variante zu welchem Feld gehört, sagt [`FieldName::accepts`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldValue {
    ListId(TaskListId),
    ParentId(Option<TaskId>),
    /// Titel, Notizen, Listenname.
    Text(String),
    /// `None` = offen, `Some(completed_at)` = erledigt.
    Status(Option<Timestamp>),
    Priority(#[serde(with = "priority_code")] Priority),
    Due(#[serde(with = "due_wire")] Option<Due>),
    Tags(Vec<TagId>),
    SortKey(#[serde(with = "sort_key_str")] SortKey),
    Timestamp(Timestamp),
    Bool(bool),
}

impl Op {
    /// Kodiert eine einzelne Op (Outbox-Zeile).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("serializing to a Vec cannot fail")
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigen oder unbekannten Daten.
    pub fn decode(bytes: &[u8]) -> CoreResult<Self> {
        let op: Self = postcard::from_bytes(bytes).map_err(invalid_op)?;
        op.validate()?;
        Ok(op)
    }

    /// Prüft, dass Felder zur Entity und Werte zu Feldern passen und die HLC vom Gerät der Op
    /// stammt.
    ///
    /// # Errors
    /// `CoreError::Validation` mit Beschreibung des ersten Verstoßes.
    pub fn validate(&self) -> CoreResult<()> {
        if self.hlc.device != self.id.device {
            return Err(CoreError::Validation(format!(
                "op {}/{}: hlc belongs to another device",
                self.id.device, self.id.seq
            )));
        }
        if self.hlc.wall_ms > super::hlc::MAX_WALL_MS {
            return Err(CoreError::Validation(format!(
                "op {}/{}: hlc is implausibly far in the future",
                self.id.device, self.id.seq
            )));
        }
        let check = |field: FieldName, value: &FieldValue| {
            if field.applies_to(&self.entity) && field.accepts(value) {
                Ok(())
            } else {
                Err(CoreError::Validation(format!(
                    "op {}/{}: invalid value for field {field:?} of {:?}",
                    self.id.device, self.id.seq, self.entity
                )))
            }
        };
        match &self.kind {
            OpKind::Create { fields } => fields.iter().try_for_each(|(f, v)| check(*f, v)),
            OpKind::Set { field, value } => check(*field, value),
            OpKind::Delete => Ok(()),
        }
    }
}

impl FieldValue {
    /// Kodiert einen einzelnen Wert (Register im Speicher).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("serializing to a Vec cannot fail")
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigen Daten.
    pub fn decode(bytes: &[u8]) -> CoreResult<Self> {
        postcard::from_bytes(bytes).map_err(invalid_op)
    }
}

/// Kodiert eine Op-Liste (Inhalt eines Segments, vor Kompression und Verschlüsselung).
#[must_use]
pub fn encode_ops(ops: &[Op]) -> Vec<u8> {
    postcard::to_allocvec(ops).expect("serializing to a Vec cannot fail")
}

/// Gegenstück zu [`encode_ops`]; prüft jede Op ([`Op::validate`]).
///
/// # Errors
/// `CoreError::Validation` bei ungültigen Daten.
pub fn decode_ops(bytes: &[u8]) -> CoreResult<Vec<Op>> {
    let ops: Vec<Op> = postcard::from_bytes(bytes).map_err(invalid_op)?;
    ops.iter().try_for_each(Op::validate)?;
    Ok(ops)
}

#[allow(clippy::needless_pass_by_value)] // passend für `map_err`
fn invalid_op(err: postcard::Error) -> CoreError {
    CoreError::Validation(format!("invalid op encoding: {err}"))
}

/// Priorität als fester Code (unabhängig von der Variantenreihenfolge im Domain-Typ).
mod priority_code {
    use serde::{de, Deserialize, Deserializer, Serializer};

    use crate::domain::Priority;

    #[allow(clippy::trivially_copy_pass_by_ref)] // Signatur von serde vorgegeben
    pub(super) fn serialize<S: Serializer>(p: &Priority, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(match p {
            Priority::None => 0,
            Priority::Low => 1,
            Priority::Medium => 2,
            Priority::High => 3,
        })
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Priority, D::Error> {
        match u8::deserialize(d)? {
            0 => Ok(Priority::None),
            1 => Ok(Priority::Low),
            2 => Ok(Priority::Medium),
            3 => Ok(Priority::High),
            other => Err(de::Error::custom(format!("unknown priority code {other}"))),
        }
    }
}

/// Fälligkeit: Datum als Tageszahl (seit 0001-01-01), Zeitpunkt mit Zonenname.
mod due_wire {
    use chrono::Datelike;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use crate::domain::{Due, NaiveDate, Timestamp};

    #[derive(Serialize, Deserialize)]
    enum Wire {
        Date(i32),
        DateTime { at: Timestamp, tz: String },
    }

    #[allow(clippy::ref_option)] // Signatur von serde vorgegeben
    pub(super) fn serialize<S: Serializer>(due: &Option<Due>, s: S) -> Result<S::Ok, S::Error> {
        due.as_ref()
            .map(|due| match due {
                Due::Date(date) => Wire::Date(date.num_days_from_ce()),
                Due::DateTime { at, tz } => Wire::DateTime {
                    at: *at,
                    tz: tz.clone(),
                },
            })
            .serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Due>, D::Error> {
        Option::<Wire>::deserialize(d)?
            .map(|wire| match wire {
                Wire::Date(days) => NaiveDate::from_num_days_from_ce_opt(days)
                    .map(Due::Date)
                    .ok_or_else(|| de::Error::custom(format!("invalid day number {days}"))),
                Wire::DateTime { at, tz } => Ok(Due::DateTime { at, tz }),
            })
            .transpose()
    }
}

/// Sortierschlüssel als String, beim Lesen mit Formatprüfung.
mod sort_key_str {
    use serde::{de, Deserialize, Deserializer, Serializer};

    use crate::domain::SortKey;

    pub(super) fn serialize<S: Serializer>(key: &SortKey, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(key.as_str())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SortKey, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        SortKey::parse(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NaiveDate;

    fn device() -> DeviceId {
        DeviceId::new()
    }

    fn op(device: DeviceId, seq: u64, entity: EntityRef, kind: OpKind) -> Op {
        Op {
            id: OpId { device, seq },
            hlc: Hlc {
                wall_ms: 1_000 + seq,
                counter: 0,
                device,
            },
            entity,
            kind,
        }
    }

    fn sample_values() -> Vec<(FieldName, FieldValue)> {
        vec![
            (FieldName::ListId, FieldValue::ListId(TaskListId::new())),
            (
                FieldName::ParentId,
                FieldValue::ParentId(Some(TaskId::new())),
            ),
            (FieldName::Title, FieldValue::Text("Milch".into())),
            (FieldName::Notes, FieldValue::Text(String::new())),
            (
                FieldName::Status,
                FieldValue::Status(Some(Timestamp::from_millis(5))),
            ),
            (FieldName::Priority, FieldValue::Priority(Priority::High)),
            (
                FieldName::Due,
                FieldValue::Due(Some(Due::Date(
                    NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
                ))),
            ),
            (FieldName::Tags, FieldValue::Tags(vec![TagId::new()])),
            (FieldName::SortKey, FieldValue::SortKey(SortKey::first())),
            (
                FieldName::CreatedAt,
                FieldValue::Timestamp(Timestamp::from_millis(-3)),
            ),
        ]
    }

    #[test]
    fn implausibly_far_future_hlc_is_rejected() {
        let d = device();
        let mut far = op(d, 1, EntityRef::Task(TaskId::new()), OpKind::Delete);
        far.hlc.wall_ms = u64::MAX;
        assert!(far.validate().is_err());
        far.hlc.wall_ms = super::super::MAX_WALL_MS;
        assert!(far.validate().is_ok(), "Grenze selbst ist erlaubt");
    }

    #[test]
    fn field_codes_are_unique_and_roundtrip() {
        let mut codes: Vec<u8> = FieldName::ALL.iter().map(|f| f.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), FieldName::ALL.len());
        for field in FieldName::ALL {
            assert_eq!(FieldName::from_code(field.code()), Some(field));
        }
        assert_eq!(FieldName::from_code(0), None);
        assert_eq!(FieldName::from_code(200), None);
    }

    #[test]
    fn task_and_list_fields_cover_all() {
        for field in FieldName::ALL {
            assert!(
                FieldName::TASK.contains(&field) || FieldName::TASK_LIST.contains(&field),
                "{field:?}"
            );
        }
    }

    #[test]
    fn roundtrip_every_kind_and_value() {
        let d = device();
        let task = EntityRef::Task(TaskId::new());
        let list = EntityRef::TaskList(TaskListId::new());
        let mut ops = vec![
            op(
                d,
                1,
                task,
                OpKind::Create {
                    fields: sample_values().into_iter().collect(),
                },
            ),
            op(d, 2, list, OpKind::Delete),
            op(
                d,
                3,
                list,
                OpKind::Set {
                    field: FieldName::IsInbox,
                    value: FieldValue::Bool(true),
                },
            ),
            op(
                d,
                4,
                task,
                OpKind::Set {
                    field: FieldName::Due,
                    value: FieldValue::Due(Some(Due::DateTime {
                        at: Timestamp::from_millis(9),
                        tz: "Europe/Berlin".into(),
                    })),
                },
            ),
        ];
        for (seq, (field, value)) in (5..).zip(sample_values()) {
            ops.push(op(d, seq, task, OpKind::Set { field, value }));
        }
        for op in &ops {
            assert_eq!(&Op::decode(&op.encode()).unwrap(), op);
        }
        assert_eq!(decode_ops(&encode_ops(&ops)).unwrap(), ops);
        for (_, value) in sample_values() {
            assert_eq!(FieldValue::decode(&value.encode()).unwrap(), value);
        }
        assert!(FieldValue::decode(&[0xff]).is_err());
    }

    #[test]
    fn ids_are_encoded_as_raw_bytes() {
        let d = device();
        let encoded = op(d, 1, EntityRef::Task(TaskId::new()), OpKind::Delete).encode();
        // OpId: 1 Längenbyte + 16 Bytes Gerät + Seq; HLC: 1 + 28; Entity: Variante + 1 + 16; Kind: 1.
        assert_eq!(encoded.len(), 17 + 1 + 29 + 18 + 1);
    }

    #[test]
    fn rejects_mismatched_field_value_or_entity() {
        let d = device();
        let task = EntityRef::Task(TaskId::new());
        let wrong_type = op(
            d,
            1,
            task,
            OpKind::Set {
                field: FieldName::Title,
                value: FieldValue::Bool(true),
            },
        );
        assert!(Op::decode(&wrong_type.encode()).is_err());
        let wrong_entity = op(
            d,
            1,
            task,
            OpKind::Set {
                field: FieldName::Name,
                value: FieldValue::Text("x".into()),
            },
        );
        assert!(wrong_entity.validate().is_err());
        let mut foreign_hlc = op(d, 1, task, OpKind::Delete);
        foreign_hlc.hlc.device = device();
        assert!(foreign_hlc.validate().is_err());
    }

    #[test]
    fn rejects_garbage_and_unknown_codes() {
        assert!(Op::decode(&[]).is_err());
        assert!(decode_ops(&[0xff, 0xff]).is_err());
        let d = device();
        let mut bytes = op(
            d,
            1,
            EntityRef::Task(TaskId::new()),
            OpKind::Set {
                field: FieldName::Title,
                value: FieldValue::Text("x".into()),
            },
        )
        .encode();
        // Kind-Tag (Set = 1) gefolgt vom Feld-Code; den Code auf einen unbekannten setzen.
        let pos = bytes.len() - 4; // [kind=1, field, value-tag=2, len=1, 'x'] → field bei len-4
        assert_eq!(bytes[pos], FieldName::Title.code());
        bytes[pos] = 99;
        // postcard verwirft eigene Fehlertexte; es genügt, dass abgelehnt wird.
        let err = Op::decode(&bytes).unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)), "{err}");
    }

    #[test]
    fn rejects_invalid_sort_key_and_priority() {
        let d = device();
        let encoded = op(
            d,
            1,
            EntityRef::Task(TaskId::new()),
            OpKind::Set {
                field: FieldName::Priority,
                value: FieldValue::Priority(Priority::Low),
            },
        )
        .encode();
        let mut bad_priority = encoded.clone();
        *bad_priority.last_mut().unwrap() = 9;
        assert!(Op::decode(&bad_priority).is_err());

        let with_key = op(
            d,
            1,
            EntityRef::Task(TaskId::new()),
            OpKind::Set {
                field: FieldName::SortKey,
                value: FieldValue::SortKey(SortKey::first()),
            },
        )
        .encode();
        let mut bad_key = with_key.clone();
        *bad_key.last_mut().unwrap() = b'!';
        assert!(Op::decode(&bad_key).is_err());
    }
}
