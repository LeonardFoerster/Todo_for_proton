//! Merge: Ops deterministisch auf den Zustand einer Entity anwenden (`sync-strategy.md` §2).
//!
//! Der Zustand ist pro Feld ein LWW-Register (Wert + HLC) bzw. ein Tombstone:
//! - **Feldweise Last-Writer-Wins:** Ein Wert gewinnt, wenn seine HLC größer ist. Gleichstand
//!   bei Zeit und Zähler entscheidet die höhere Geräte-ID (Teil der [`Hlc`]-Ordnung).
//! - **Delete gewinnt:** Nach einem Delete werden alle Ops ignoriert, auch spätere.
//! - **Idempotent:** Gleiche HLC = dieselbe Op → keine Änderung. Duplikate sind harmlos.
//!
//! Damit ist das Ergebnis unabhängig von Reihenfolge, Gruppierung und Wiederholung der Ops
//! (Konvergenz; Property-Test unten).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{FieldMap, FieldName, FieldValue, Hlc, Op, OpKind};
use crate::domain::{
    Due, Note, NoteFolder, NoteFolderId, NoteFolderParts, NoteId, NoteParts, Priority, SortKey,
    TagId, Task, TaskId, TaskList, TaskListId, TaskListParts, TaskParts, TaskStatus, Timestamp,
};

/// Ein Feldwert mit der HLC der Op, die ihn gesetzt hat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Register {
    pub hlc: Hlc,
    pub value: FieldValue,
}

/// Register pro Feld (zugleich die „Field-Clocks").
pub type Registers = BTreeMap<FieldName, Register>;

/// Sync-Zustand einer Entity. Teil des Snapshot-Formats: Varianten nur anhängen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityState {
    /// Bekannte Feldwerte. Kann unvollständig sein, wenn `Set`s vor dem `Create` eintreffen.
    Live(Registers),
    /// Gelöscht (Tombstone) mit der größten gesehenen Delete-HLC.
    Deleted(Hlc),
}

/// Ergebnis von [`merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    /// Neuer Zustand; `None` nur, wenn vorher keiner existierte und keine Op etwas bewirkt hat.
    pub state: Option<EntityState>,
    /// Hat sich gegenüber dem Ausgangszustand etwas geändert?
    pub changed: bool,
}

impl EntityState {
    /// Zustand nach einem lokalen oder empfangenen `Create`.
    #[must_use]
    pub fn created(fields: &FieldMap, hlc: Hlc) -> Self {
        Self::Live(
            fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        Register {
                            hlc,
                            value: value.clone(),
                        },
                    )
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn is_deleted(&self) -> bool {
        matches!(self, Self::Deleted(_))
    }

    /// Die aktuellen Feldwerte (Grundlage für den Diff); `None` bei Tombstone.
    #[must_use]
    pub fn fields(&self) -> Option<FieldMap> {
        match self {
            Self::Live(registers) => Some(
                registers
                    .iter()
                    .map(|(field, reg)| (*field, reg.value.clone()))
                    .collect(),
            ),
            Self::Deleted(_) => None,
        }
    }

    /// Größte HLC im Zustand (z. B. für das gerätelokale `updated_at`).
    #[must_use]
    pub fn max_hlc(&self) -> Option<Hlc> {
        match self {
            Self::Live(registers) => registers.values().map(|reg| reg.hlc).max(),
            Self::Deleted(hlc) => Some(*hlc),
        }
    }

    /// Wendet eine Op an. Liefert `true`, wenn sich der Zustand geändert hat.
    pub fn apply(&mut self, op: &Op) -> bool {
        match (&mut *self, &op.kind) {
            (Self::Deleted(hlc), OpKind::Delete) if op.hlc > *hlc => {
                *hlc = op.hlc;
                true
            }
            (Self::Deleted(_), _) => false,
            (Self::Live(_), OpKind::Delete) => {
                *self = Self::Deleted(op.hlc);
                true
            }
            (Self::Live(registers), OpKind::Create { fields }) => {
                let mut changed = false;
                for (field, value) in fields {
                    changed |= write(registers, *field, value, op.hlc);
                }
                changed
            }
            (Self::Live(registers), OpKind::Set { field, value }) => {
                write(registers, *field, value, op.hlc)
            }
            // Inhalte sind keine Register (Revisionen, `sync::revision`).
            (Self::Live(_), OpKind::Revise { .. }) => false,
        }
    }
}

/// LWW-Schreiben eines Registers.
fn write(registers: &mut Registers, field: FieldName, value: &FieldValue, hlc: Hlc) -> bool {
    match registers.get(&field) {
        Some(current) if current.hlc >= hlc => false,
        _ => {
            registers.insert(
                field,
                Register {
                    hlc,
                    value: value.clone(),
                },
            );
            true
        }
    }
}

/// Wendet `ops` (alle für dieselbe Entity, beliebige Reihenfolge, Duplikate erlaubt) auf
/// `state` an. Reine Funktion.
#[must_use]
pub fn merge(state: Option<&EntityState>, ops: &[Op]) -> MergeResult {
    let mut current = state.cloned();
    let mut changed = false;
    for op in ops {
        let entry = current.get_or_insert_with(|| EntityState::Live(Registers::new()));
        changed |= entry.apply(op);
    }
    // Eine Entity ohne Felder ist dasselbe wie keine (z. B. nur unwirksame Ops).
    if matches!(&current, Some(EntityState::Live(r)) if r.is_empty()) && state.is_none() {
        current = None;
    }
    MergeResult {
        state: current,
        changed,
    }
}

/// Baut die Task aus vollständigen, gültigen Registern; sonst `None` (z. B. `Create` noch nicht
/// empfangen oder Werte, die die Domain ablehnt). `updated_at` = größte Wall-Zeit der Register.
#[must_use]
pub fn materialize_task(id: TaskId, registers: &Registers) -> Option<Task> {
    let completed_at = match value(registers, FieldName::Status)? {
        FieldValue::Status(at) => *at,
        _ => return None,
    };
    let parts = TaskParts {
        id,
        list_id: list_id(registers)?,
        parent_id: match value(registers, FieldName::ParentId)? {
            FieldValue::ParentId(parent) => *parent,
            _ => return None,
        },
        title: text(registers, FieldName::Title)?,
        notes: text(registers, FieldName::Notes)?,
        status: if completed_at.is_some() {
            TaskStatus::Done
        } else {
            TaskStatus::Open
        },
        priority: priority(registers)?,
        due: due(registers)?,
        tags: tags(registers)?,
        sort_key: sort_key(registers)?,
        created_at: created_at(registers)?,
        updated_at: updated_at(registers),
        completed_at,
        // Seit ADR-031; ältere Tasks haben kein Register → nicht im Papierkorb (nie `?`, sonst
        // verschwände jede bestehende Task beim nächsten Materialisieren).
        trashed_at: trashed(registers).flatten(),
        // Seit ADR-028; fehlt bei älteren Tasks → keine Wiederholung (nie `?`, s. o.).
        recurrence: recurrence(registers).flatten(),
    };
    Task::from_parts(parts).ok()
}

/// Wie [`materialize_task`] für Listen.
#[must_use]
pub fn materialize_list(id: TaskListId, registers: &Registers) -> Option<TaskList> {
    let parts = TaskListParts {
        id,
        name: text(registers, FieldName::Name)?,
        sort_key: sort_key(registers)?,
        created_at: created_at(registers)?,
        updated_at: updated_at(registers),
        is_inbox: match value(registers, FieldName::IsInbox)? {
            FieldValue::Bool(b) => *b,
            _ => return None,
        },
    };
    TaskList::from_parts(parts).ok()
}

/// Wie [`materialize_task`] für Notiz-Ordner. `parent` = wirksamer Elternordner nach den
/// Baum-Regeln (`domain::note_tree`), nicht unbedingt der aus den Registern.
#[must_use]
pub fn materialize_note_folder(
    id: NoteFolderId,
    registers: &Registers,
    parent: Option<NoteFolderId>,
) -> Option<NoteFolder> {
    folder(registers)?;
    let parts = NoteFolderParts {
        id,
        parent,
        name: text(registers, FieldName::Name)?,
        sort_key: sort_key(registers)?,
        created_at: created_at(registers)?,
        trashed_at: trashed(registers)?,
    };
    NoteFolder::from_parts(parts).ok()
}

/// Felder einer Notiz aus den Registern; Ordner (wirksam), Inhalt, `modified_at` und – bei
/// Auto-Name oder fehlendem Namen – den Namen setzt der Aufrufer (`sync::replica`). `None` bei
/// unvollständigen oder ungültigen Registern.
#[must_use]
pub fn materialize_note(id: NoteId, registers: &Registers) -> Option<NoteParts> {
    let created_at = created_at(registers)?;
    let name = registers.get(&FieldName::Name);
    let auto_name = match value(registers, FieldName::AutoName)? {
        FieldValue::Bool(b) => *b,
        _ => return None,
    };
    let explicit = name
        .filter(|_| !auto_name)
        .and_then(|reg| match &reg.value {
            FieldValue::Text(s) => Some((s.clone(), reg.hlc.wall_ms)),
            _ => None,
        });
    Some(NoteParts {
        id,
        folder: folder(registers)?,
        name: explicit.as_ref().map_or_else(
            || crate::domain::UNTITLED_NOTE_NAME.to_owned(),
            |(s, _)| s.clone(),
        ),
        auto_name: auto_name || explicit.is_none(),
        kind: match value(registers, FieldName::NoteKind)? {
            FieldValue::NoteKind(kind) => *kind,
            _ => return None,
        },
        body: String::new(),
        body_rev: None,
        sort_key: sort_key(registers)?,
        created_at,
        modified_at: created_at.max(wall(explicit.map_or(0, |(_, at)| at))),
        trashed_at: trashed(registers)?,
    })
    .filter(|parts| Note::from_parts(parts.clone()).is_ok())
}

/// Gespeicherter Ordner (bzw. Elternordner) laut Registern.
#[must_use]
pub fn folder(registers: &Registers) -> Option<Option<NoteFolderId>> {
    match value(registers, FieldName::ParentFolder)? {
        FieldValue::Folder(folder) => Some(*folder),
        _ => None,
    }
}

fn recurrence(registers: &Registers) -> Option<Option<crate::domain::Recurrence>> {
    match value(registers, FieldName::Recurrence)? {
        FieldValue::Recurrence(r) => Some(r.clone()),
        _ => None,
    }
}

fn trashed(registers: &Registers) -> Option<Option<Timestamp>> {
    match value(registers, FieldName::Trashed)? {
        FieldValue::Trashed(at) => Some(*at),
        _ => None,
    }
}

/// HLC-Wandzeit als Zeitstempel.
#[must_use]
pub fn wall(wall_ms: u64) -> Timestamp {
    Timestamp::from_millis(i64::try_from(wall_ms).unwrap_or(i64::MAX))
}

/// Die Liste, in die eine Task laut Registern gehört (auch wenn die Task unvollständig ist).
#[must_use]
pub fn list_id(registers: &Registers) -> Option<TaskListId> {
    match value(registers, FieldName::ListId)? {
        FieldValue::ListId(id) => Some(*id),
        _ => None,
    }
}

fn value(registers: &Registers, field: FieldName) -> Option<&FieldValue> {
    registers.get(&field).map(|reg| &reg.value)
}

fn text(registers: &Registers, field: FieldName) -> Option<String> {
    match value(registers, field)? {
        FieldValue::Text(s) => Some(s.clone()),
        _ => None,
    }
}

fn priority(registers: &Registers) -> Option<Priority> {
    match value(registers, FieldName::Priority)? {
        FieldValue::Priority(p) => Some(*p),
        _ => None,
    }
}

fn due(registers: &Registers) -> Option<Option<Due>> {
    match value(registers, FieldName::Due)? {
        FieldValue::Due(d) => Some(d.clone()),
        _ => None,
    }
}

fn tags(registers: &Registers) -> Option<Vec<TagId>> {
    match value(registers, FieldName::Tags)? {
        FieldValue::Tags(t) => Some(t.clone()),
        _ => None,
    }
}

fn sort_key(registers: &Registers) -> Option<SortKey> {
    match value(registers, FieldName::SortKey)? {
        FieldValue::SortKey(k) => Some(k.clone()),
        _ => None,
    }
}

fn created_at(registers: &Registers) -> Option<Timestamp> {
    match value(registers, FieldName::CreatedAt)? {
        FieldValue::Timestamp(t) => Some(*t),
        _ => None,
    }
}

fn updated_at(registers: &Registers) -> Timestamp {
    wall(
        registers
            .values()
            .map(|reg| reg.hlc.wall_ms)
            .max()
            .unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{DeviceId, NaiveDate},
        ports::{EntityRef, FixedClock},
        sync::{task_fields, HlcState, OpClock},
    };

    /// Zwei Geräte mit fester Reihenfolge der IDs (`low` < `high`).
    struct Devices {
        low: OpClock,
        high: OpClock,
    }

    fn devices() -> Devices {
        let (a, b) = (DeviceId::new(), DeviceId::new());
        let (low, high) = if a < b { (a, b) } else { (b, a) };
        Devices {
            low: OpClock::new(HlcState::new(low), 0),
            high: OpClock::new(HlcState::new(high), 0),
        }
    }

    fn ts(ms: i64) -> Timestamp {
        Timestamp::from_millis(ms)
    }

    fn task() -> Task {
        Task::new(
            TaskListId::new(),
            "Milch",
            SortKey::first(),
            &FixedClock(ts(10)),
        )
        .unwrap()
    }

    fn create(clock: &mut OpClock, now: i64, task: &Task) -> Op {
        clock.stamp(
            ts(now),
            EntityRef::Task(task.id()),
            OpKind::Create {
                fields: task_fields(task),
            },
        )
    }

    fn set(clock: &mut OpClock, now: i64, task: &Task, field: FieldName, value: FieldValue) -> Op {
        clock.stamp(
            ts(now),
            EntityRef::Task(task.id()),
            OpKind::Set { field, value },
        )
    }

    fn delete(clock: &mut OpClock, now: i64, task: &Task) -> Op {
        clock.stamp(ts(now), EntityRef::Task(task.id()), OpKind::Delete)
    }

    fn title(s: &str) -> FieldValue {
        FieldValue::Text(s.into())
    }

    fn merged(ops: &[Op]) -> EntityState {
        merge(None, ops).state.expect("state")
    }

    fn field(state: &EntityState, field: FieldName) -> FieldValue {
        state.fields().expect("live")[&field].clone()
    }

    #[test]
    fn create_materializes_the_same_task() {
        let mut d = devices();
        let t = task();
        let state = merged(&[create(&mut d.low, 100, &t)]);
        let EntityState::Live(registers) = &state else {
            panic!()
        };
        let back = materialize_task(t.id(), registers).unwrap();
        assert_eq!(task_fields(&back), task_fields(&t));
        assert_eq!(back.updated_at(), ts(100), "updated_at = größte HLC-Zeit");
    }

    #[test]
    fn concurrent_title_and_due_edits_both_survive() {
        let mut d = devices();
        let t = task();
        let base = create(&mut d.low, 100, &t);
        let due = FieldValue::Due(Some(Due::Date(
            NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
        )));
        let a = set(&mut d.low, 200, &t, FieldName::Title, title("Hafermilch"));
        let b = set(&mut d.high, 150, &t, FieldName::Due, due.clone());
        for order in [[&base, &a, &b], [&b, &a, &base]] {
            let ops: Vec<Op> = order.into_iter().cloned().collect();
            let state = merged(&ops);
            assert_eq!(field(&state, FieldName::Title), title("Hafermilch"));
            assert_eq!(field(&state, FieldName::Due), due);
        }
    }

    #[test]
    fn later_hlc_wins_regardless_of_arrival_order() {
        let mut d = devices();
        let t = task();
        let early = set(&mut d.high, 100, &t, FieldName::Title, title("früh"));
        let late = set(&mut d.low, 200, &t, FieldName::Title, title("spät"));
        for ops in [[early.clone(), late.clone()], [late, early]] {
            assert_eq!(field(&merged(&ops), FieldName::Title), title("spät"));
        }
    }

    #[test]
    fn equal_time_and_counter_higher_device_wins() {
        let mut d = devices();
        let t = task();
        let from_low = set(&mut d.low, 100, &t, FieldName::Title, title("low"));
        let from_high = set(&mut d.high, 100, &t, FieldName::Title, title("high"));
        assert_eq!(
            (from_low.hlc.wall_ms, from_low.hlc.counter),
            (from_high.hlc.wall_ms, from_high.hlc.counter)
        );
        for ops in [[from_low.clone(), from_high.clone()], [from_high, from_low]] {
            assert_eq!(field(&merged(&ops), FieldName::Title), title("high"));
        }
    }

    #[test]
    fn delete_wins_over_later_set() {
        let mut d = devices();
        let t = task();
        let created = create(&mut d.low, 100, &t);
        let deleted = delete(&mut d.low, 200, &t);
        let edited = set(&mut d.high, 300, &t, FieldName::Title, title("neu"));
        for ops in [
            [created.clone(), deleted.clone(), edited.clone()],
            [edited, deleted.clone(), created],
        ] {
            assert_eq!(merged(&ops), EntityState::Deleted(deleted.hlc));
        }
    }

    #[test]
    fn create_after_delete_is_ignored() {
        let mut d = devices();
        let t = task();
        let deleted = delete(&mut d.low, 100, &t);
        let result = merge(
            Some(&EntityState::Deleted(deleted.hlc)),
            &[create(&mut d.high, 500, &t)],
        );
        assert!(!result.changed);
        assert_eq!(result.state, Some(EntityState::Deleted(deleted.hlc)));
    }

    #[test]
    fn concurrent_deletes_keep_the_largest_tombstone() {
        let mut d = devices();
        let t = task();
        let a = delete(&mut d.low, 100, &t);
        let b = delete(&mut d.high, 90, &t);
        for ops in [[a.clone(), b.clone()], [b, a.clone()]] {
            assert_eq!(merged(&ops), EntityState::Deleted(a.hlc));
        }
    }

    #[test]
    fn duplicate_ops_are_idempotent() {
        let mut d = devices();
        let t = task();
        let base = create(&mut d.low, 100, &t);
        let edit = set(&mut d.low, 200, &t, FieldName::Notes, title("bio"));
        let once = merge(None, &[base.clone(), edit.clone()]);
        let again = merge(once.state.as_ref(), &[edit.clone(), base.clone()]);
        assert!(!again.changed);
        assert_eq!(again.state, once.state);
        assert_eq!(
            merged(&[base.clone(), edit.clone(), base, edit]),
            once.state.unwrap()
        );
    }

    #[test]
    fn create_does_not_override_newer_set() {
        let mut d = devices();
        let t = task();
        let create_op = create(&mut d.low, 100, &t);
        let newer = set(&mut d.high, 200, &t, FieldName::Title, title("neu"));
        let state = merged(&[newer, create_op]);
        assert_eq!(field(&state, FieldName::Title), title("neu"));
        assert_eq!(field(&state, FieldName::Notes), title(""));
    }

    #[test]
    fn set_before_create_is_kept_but_not_materialized() {
        let mut d = devices();
        let t = task();
        let early = set(&mut d.high, 100, &t, FieldName::Title, title("x"));
        let state = merged(std::slice::from_ref(&early));
        let EntityState::Live(registers) = &state else {
            panic!()
        };
        assert_eq!(materialize_task(t.id(), registers), None, "unvollständig");
    }

    #[test]
    fn status_carries_completion_time() {
        let mut d = devices();
        let t = task();
        let base = create(&mut d.low, 100, &t);
        let done = set(
            &mut d.high,
            200,
            &t,
            FieldName::Status,
            FieldValue::Status(Some(ts(200))),
        );
        let EntityState::Live(registers) = merged(&[base, done]) else {
            panic!()
        };
        let task = materialize_task(t.id(), &registers).unwrap();
        assert!(task.is_done());
        assert_eq!(task.completed_at(), Some(ts(200)));
    }

    #[test]
    fn invalid_values_are_not_materialized() {
        let mut d = devices();
        let t = task();
        let base = create(&mut d.low, 100, &t);
        let empty_title = set(&mut d.low, 200, &t, FieldName::Title, title("   "));
        let EntityState::Live(registers) = merged(&[base, empty_title]) else {
            panic!()
        };
        assert_eq!(materialize_task(t.id(), &registers), None);
    }

    #[test]
    fn list_roundtrip() {
        let mut d = devices();
        let list = TaskList::new("Arbeit", SortKey::first(), &FixedClock(ts(5))).unwrap();
        let op = d.low.stamp(
            ts(100),
            EntityRef::TaskList(list.id()),
            OpKind::Create {
                fields: crate::sync::list_fields(&list),
            },
        );
        let EntityState::Live(registers) = merged(&[op]) else {
            panic!()
        };
        let back = materialize_list(list.id(), &registers).unwrap();
        assert_eq!(back.name(), "Arbeit");
        assert_eq!(back.created_at(), ts(5));
        assert!(!back.is_inbox());
    }

    #[test]
    fn no_ops_on_unknown_entity_gives_no_state() {
        let result = merge(None, &[]);
        assert_eq!(result.state, None);
        assert!(!result.changed);
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod convergence {
        use std::collections::BTreeMap;

        use proptest::prelude::*;

        use super::*;
        use crate::domain::TaskId;

        /// Was ein Gerät tut (Entity per Index aus einem kleinen Pool, damit Konflikte häufig sind).
        #[derive(Debug, Clone)]
        enum Action {
            Create(usize, String),
            Title(usize, String),
            Priority(usize, u8),
            Done(usize, bool),
            /// Papierkorb an/aus (ADR-031).
            Trash(usize, bool),
            /// Wiederholung täglich/keine (ADR-028).
            Rule(usize, bool),
            Delete(usize),
        }

        fn action() -> impl Strategy<Value = Action> {
            let entity = 0..3usize;
            prop_oneof![
                2 => (entity.clone(), "[a-c]{1,2}").prop_map(|(e, s)| Action::Create(e, s)),
                4 => (entity.clone(), "[a-c]{1,2}").prop_map(|(e, s)| Action::Title(e, s)),
                2 => (entity.clone(), 0u8..4).prop_map(|(e, p)| Action::Priority(e, p)),
                2 => (entity.clone(), any::<bool>()).prop_map(|(e, d)| Action::Done(e, d)),
                2 => (entity.clone(), any::<bool>()).prop_map(|(e, t)| Action::Trash(e, t)),
                2 => (entity.clone(), any::<bool>()).prop_map(|(e, r)| Action::Rule(e, r)),
                1 => entity.prop_map(Action::Delete),
            ]
        }

        /// Ops eines Geräts; die Uhr springt zufällig (auch rückwärts), Werte kollidieren oft.
        fn device_ops(device: DeviceId, ids: &[TaskId], actions: &[(Action, i64)]) -> Vec<Op> {
            let mut clock = OpClock::new(HlcState::new(device), 0);
            let list = TaskListId::parse("0192f0c4-7a3b-7c2d-8e4f-0123456789ab").unwrap();
            actions
                .iter()
                .map(|(action, now)| {
                    let now = Timestamp::from_millis(*now);
                    let (entity, kind) = match action {
                        Action::Create(e, title) => {
                            let t =
                                Task::new(list, title, SortKey::first(), &FixedClock(now)).unwrap();
                            let mut fields = task_fields(&t);
                            fields.remove(&FieldName::CreatedAt);
                            fields.insert(FieldName::CreatedAt, FieldValue::Timestamp(now));
                            (ids[*e], OpKind::Create { fields })
                        }
                        Action::Title(e, title) => (
                            ids[*e],
                            OpKind::Set {
                                field: FieldName::Title,
                                value: FieldValue::Text(title.clone()),
                            },
                        ),
                        Action::Priority(e, p) => (
                            ids[*e],
                            OpKind::Set {
                                field: FieldName::Priority,
                                value: FieldValue::Priority(
                                    [
                                        Priority::None,
                                        Priority::Low,
                                        Priority::Medium,
                                        Priority::High,
                                    ][usize::from(*p)],
                                ),
                            },
                        ),
                        Action::Done(e, done) => (
                            ids[*e],
                            OpKind::Set {
                                field: FieldName::Status,
                                value: FieldValue::Status(done.then_some(now)),
                            },
                        ),
                        Action::Trash(e, trashed) => (
                            ids[*e],
                            OpKind::Set {
                                field: FieldName::Trashed,
                                value: FieldValue::Trashed(trashed.then_some(now)),
                            },
                        ),
                        Action::Rule(e, on) => (
                            ids[*e],
                            OpKind::Set {
                                field: FieldName::Recurrence,
                                value: FieldValue::Recurrence(on.then(|| {
                                    crate::domain::Recurrence::every(
                                        crate::domain::Frequency::Daily,
                                    )
                                })),
                            },
                        ),
                        Action::Delete(e) => (ids[*e], OpKind::Delete),
                    };
                    clock.stamp(now, EntityRef::Task(entity), kind)
                })
                .collect()
        }

        /// Ein Replikat: wendet die Ops in Gruppen (Pulls) auf den Zustand aller Entities an.
        fn replay(ops: &[Op], chunk: usize) -> BTreeMap<EntityRef, EntityState> {
            let mut states: BTreeMap<EntityRef, EntityState> = BTreeMap::new();
            for batch in ops.chunks(chunk.max(1)) {
                let mut by_entity: BTreeMap<EntityRef, Vec<Op>> = BTreeMap::new();
                for op in batch {
                    by_entity.entry(op.entity).or_default().push(op.clone());
                }
                for (entity, ops) in by_entity {
                    if let Some(state) = merge(states.get(&entity), &ops).state {
                        states.insert(entity, state);
                    }
                }
            }
            states
        }

        fn scenario() -> impl Strategy<Value = Vec<Vec<(Action, i64)>>> {
            prop::collection::vec(prop::collection::vec((action(), 0i64..50), 0..12), 2..=4)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1_000))]

            /// 2–4 Geräte, zufällige Ops; jedes Replikat bekommt alle Ops in eigener Reihenfolge,
            /// eigener Gruppierung und mit Duplikaten → identische Endzustände.
            #[test]
            fn replicas_converge(
                devices in scenario(),
                orders in prop::collection::vec(
                    (any::<prop::sample::Index>(), any::<u64>(), 1usize..6, 0usize..10), 3),
            ) {
                let ids: Vec<TaskId> = (0..3).map(|_| TaskId::new()).collect();
                let all: Vec<Op> = devices
                    .iter()
                    .flat_map(|actions| device_ops(DeviceId::new(), &ids, actions))
                    .collect();

                let mut results = Vec::new();
                for (dup_index, seed, chunk, dups) in orders {
                    let mut ops = all.clone();
                    if !all.is_empty() {
                        for i in 0..dups {
                            ops.push(all[(dup_index.index(all.len()) + i) % all.len()].clone());
                        }
                    }
                    shuffle(&mut ops, seed);
                    results.push(replay(&ops, chunk));
                }
                // Referenz: alles auf einmal in Erzeugungsreihenfolge.
                let reference = replay(&all, all.len());
                for result in &results {
                    prop_assert_eq!(result, &reference);
                }
                // Materialisierte Tasks stimmen ebenfalls überein.
                let materialized = |states: &BTreeMap<EntityRef, EntityState>| -> Vec<Option<Task>> {
                    states.iter().map(|(entity, state)| match (entity, state) {
                        (EntityRef::Task(id), EntityState::Live(r)) => materialize_task(*id, r),
                        _ => None,
                    }).collect()
                };
                for result in &results {
                    prop_assert_eq!(materialized(result), materialized(&reference));
                }
            }
        }

        /// Deterministisches Mischen (Fisher-Yates mit einfachem LCG), damit proptest schrumpfen kann.
        fn shuffle(ops: &mut [Op], mut seed: u64) {
            for i in (1..ops.len()).rev() {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let j = usize::try_from((seed >> 33) % (i as u64 + 1)).unwrap();
                ops.swap(i, j);
            }
        }
    }
}
