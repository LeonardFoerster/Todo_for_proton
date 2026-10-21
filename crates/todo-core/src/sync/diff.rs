//! Diff: aus einer Domain-Änderung ([`Change`]) die passenden Ops erzeugen.
//!
//! Neu → ein `Create` mit allen Feldern; geändert → je geändertem Feld ein `Set`; gelöscht →
//! `Delete`. Unveränderte Felder erzeugen nichts (keine leeren Sync-Ops).

use super::{FieldMap, FieldName, FieldValue, HlcState, Op, OpId, OpKind};
use crate::{
    domain::{Note, NoteFolder, Task, TaskList, Timestamp},
    ports::{Change, EntityRef},
};

/// Vergibt HLC und fortlaufende Op-Nummer für lokal erzeugte Ops eines Geräts.
///
/// Wird zusammen mit den Ops persistiert (`sync_meta`), damit nach einem Neustart weder HLC noch
/// Nummer doppelt vergeben werden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpClock {
    hlc: HlcState,
    last_seq: u64,
}

impl OpClock {
    /// `last_seq` = Nummer der zuletzt erzeugten Op (0 = noch keine).
    #[must_use]
    pub fn new(hlc: HlcState, last_seq: u64) -> Self {
        Self { hlc, last_seq }
    }

    #[must_use]
    pub fn hlc(&self) -> &HlcState {
        &self.hlc
    }

    /// Für empfangene Ops ([`HlcState::observe`]).
    pub fn hlc_mut(&mut self) -> &mut HlcState {
        &mut self.hlc
    }

    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Gibt die letzten `n` Nummern zurück (Ops, die doch nicht entstehen). Die HLC bleibt
    /// vorgerückt – sie muss nur streng monoton sein, nicht lückenlos.
    pub fn rewind(&mut self, n: u64) {
        self.last_seq = self.last_seq.saturating_sub(n);
    }

    /// Erzeugt eine Op mit neuer HLC und der nächsten Nummer.
    pub fn stamp(&mut self, now: Timestamp, entity: EntityRef, kind: OpKind) -> Op {
        let hlc = self.hlc.tick(now);
        self.last_seq += 1;
        Op {
            id: OpId {
                device: hlc.device,
                seq: self.last_seq,
            },
            hlc,
            entity,
            kind,
        }
    }
}

/// Die synchronisierten Felder einer Task.
#[must_use]
pub fn task_fields(task: &Task) -> FieldMap {
    FieldMap::from([
        (FieldName::ListId, FieldValue::ListId(task.list_id())),
        (FieldName::ParentId, FieldValue::ParentId(task.parent_id())),
        (FieldName::Title, FieldValue::Text(task.title().to_owned())),
        (FieldName::Notes, FieldValue::Text(task.notes().to_owned())),
        (FieldName::Status, FieldValue::Status(task.completed_at())),
        (FieldName::Priority, FieldValue::Priority(task.priority())),
        (FieldName::Due, FieldValue::Due(task.due().cloned())),
        (FieldName::Tags, FieldValue::Tags(task.tags().to_vec())),
        (
            FieldName::SortKey,
            FieldValue::SortKey(task.sort_key().clone()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(task.created_at()),
        ),
        (FieldName::Trashed, FieldValue::Trashed(task.trashed_at())),
    ])
}

/// Die synchronisierten Felder einer Liste.
#[must_use]
pub fn list_fields(list: &TaskList) -> FieldMap {
    FieldMap::from([
        (FieldName::Name, FieldValue::Text(list.name().to_owned())),
        (
            FieldName::SortKey,
            FieldValue::SortKey(list.sort_key().clone()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(list.created_at()),
        ),
        (FieldName::IsInbox, FieldValue::Bool(list.is_inbox())),
    ])
}

/// Die synchronisierten Felder einer Notiz (ohne Inhalt – der läuft über Revisionen). Mit
/// Auto-Name ist der Name kein Feld: Jedes Gerät leitet ihn aus dem aufgelösten Inhalt ab, so
/// passen Name und Inhalt nach einem Merge zusammen.
#[must_use]
pub fn note_fields(note: &Note) -> FieldMap {
    let mut fields = FieldMap::from([
        (FieldName::ParentFolder, FieldValue::Folder(note.folder())),
        (FieldName::AutoName, FieldValue::Bool(note.auto_name())),
        (FieldName::NoteKind, FieldValue::NoteKind(note.kind())),
        (
            FieldName::SortKey,
            FieldValue::SortKey(note.sort_key().clone()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(note.created_at()),
        ),
        (FieldName::Trashed, FieldValue::Trashed(note.trashed_at())),
    ]);
    if !note.auto_name() {
        fields.insert(FieldName::Name, FieldValue::Text(note.name().to_string()));
    }
    fields
}

/// Die synchronisierten Felder eines Notiz-Ordners.
#[must_use]
pub fn note_folder_fields(folder: &NoteFolder) -> FieldMap {
    FieldMap::from([
        (FieldName::ParentFolder, FieldValue::Folder(folder.parent())),
        (FieldName::Name, FieldValue::Text(folder.name().to_string())),
        (
            FieldName::SortKey,
            FieldValue::SortKey(folder.sort_key().clone()),
        ),
        (
            FieldName::CreatedAt,
            FieldValue::Timestamp(folder.created_at()),
        ),
        (FieldName::Trashed, FieldValue::Trashed(folder.trashed_at())),
    ])
}

/// Ops für eine Änderung. `before` sind die bisher bekannten Feldwerte der Entity (`None` =
/// unbekannt); Felder, die dort fehlen, gelten als geändert.
///
/// Löschen einer unbekannten Entity erzeugt nichts (idempotent wie
/// [`TaskRepository::commit`](crate::ports::TaskRepository::commit)).
pub fn ops_for(
    change: &Change,
    before: Option<&FieldMap>,
    clock: &mut OpClock,
    now: Timestamp,
) -> Vec<Op> {
    let entity = change.entity();
    let after = match change {
        Change::UpsertTask(task) => task_fields(task),
        Change::UpsertList(list) => list_fields(list),
        Change::UpsertNote(note) => note_fields(note),
        Change::UpsertNoteFolder(folder) => note_folder_fields(folder),
        Change::Delete(_) => {
            return before
                .map(|_| clock.stamp(now, entity, OpKind::Delete))
                .into_iter()
                .collect();
        }
    };
    let Some(before) = before else {
        return vec![clock.stamp(now, entity, OpKind::Create { fields: after })];
    };
    after
        .into_iter()
        .filter(|(field, value)| before.get(field) != Some(value))
        .map(|(field, value)| clock.stamp(now, entity, OpKind::Set { field, value }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{DeviceId, Due, NaiveDate, Priority, SortKey, TaskListId},
        ports::FixedClock,
    };

    struct Fixture {
        clock: OpClock,
        domain_clock: FixedClock,
        now: Timestamp,
    }

    fn fixture() -> Fixture {
        Fixture {
            clock: OpClock::new(HlcState::new(DeviceId::new()), 0),
            domain_clock: FixedClock(Timestamp::from_millis(50)),
            now: Timestamp::from_millis(1_000),
        }
    }

    impl Fixture {
        fn task(&self) -> Task {
            Task::new(
                TaskListId::new(),
                "Milch",
                SortKey::first(),
                &self.domain_clock,
            )
            .unwrap()
        }

        fn ops(&mut self, change: &Change, before: Option<&FieldMap>) -> Vec<Op> {
            ops_for(change, before, &mut self.clock, self.now)
        }
    }

    fn set(op: &Op) -> (FieldName, &FieldValue) {
        match &op.kind {
            OpKind::Set { field, value } => (*field, value),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn new_task_becomes_one_create_with_all_fields() {
        let mut f = fixture();
        let task = f.task();
        let ops = f.ops(&Change::UpsertTask(task.clone()), None);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].entity, EntityRef::Task(task.id()));
        let OpKind::Create { fields } = &ops[0].kind else {
            panic!("{:?}", ops[0].kind)
        };
        assert_eq!(fields, &task_fields(&task));
        let mut all_task_fields = FieldName::TASK.to_vec();
        all_task_fields.sort();
        assert_eq!(fields.keys().copied().collect::<Vec<_>>(), all_task_fields);
        ops[0].validate().unwrap();
    }

    #[test]
    fn title_change_creates_exactly_one_set() {
        let mut f = fixture();
        let mut task = f.task();
        let before = task_fields(&task);
        task.rename("Hafermilch", &f.domain_clock).unwrap();
        let ops = f.ops(&Change::UpsertTask(task), Some(&before));
        assert_eq!(ops.len(), 1);
        assert_eq!(
            set(&ops[0]),
            (FieldName::Title, &FieldValue::Text("Hafermilch".into()))
        );
    }

    #[test]
    fn unchanged_fields_create_nothing() {
        let mut f = fixture();
        let task = f.task();
        let before = task_fields(&task);
        assert!(f.ops(&Change::UpsertTask(task), Some(&before)).is_empty());
        assert_eq!(f.clock.last_seq(), 0, "keine Nummer verbraucht");
    }

    #[test]
    fn updated_at_alone_is_not_synced() {
        let mut f = fixture();
        let task = f.task();
        let before = task_fields(&task);
        let mut parts = crate::domain::TaskParts {
            id: task.id(),
            list_id: task.list_id(),
            parent_id: None,
            title: task.title().into(),
            notes: String::new(),
            status: task.status(),
            priority: task.priority(),
            due: None,
            tags: Vec::new(),
            sort_key: task.sort_key().clone(),
            created_at: task.created_at(),
            updated_at: task.updated_at(),
            completed_at: None,
            trashed_at: None,
        };
        parts.updated_at = Timestamp::from_millis(999_999);
        let touched = Task::from_parts(parts).unwrap();
        assert!(f
            .ops(&Change::UpsertTask(touched), Some(&before))
            .is_empty());
    }

    #[test]
    fn several_changes_create_one_set_each_in_field_order() {
        let mut f = fixture();
        let mut task = f.task();
        let before = task_fields(&task);
        let c = &f.domain_clock;
        task.set_priority(Priority::High, c);
        task.set_due(
            Some(Due::Date(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())),
            c,
        )
        .unwrap();
        task.complete(c);
        let ops = f.ops(&Change::UpsertTask(task.clone()), Some(&before));
        let fields: Vec<FieldName> = ops.iter().map(|op| set(op).0).collect();
        assert_eq!(
            fields,
            [FieldName::Status, FieldName::Priority, FieldName::Due]
        );
        assert_eq!(
            set(&ops[0]).1,
            &FieldValue::Status(task.completed_at()),
            "Status trägt completed_at mit"
        );
    }

    #[test]
    fn move_sets_list_and_sort_key() {
        let mut f = fixture();
        let mut task = f.task();
        let before = task_fields(&task);
        let target = TaskListId::new();
        let key = SortKey::between(Some(&SortKey::first()), None).unwrap();
        task.move_to(target, key.clone(), &f.domain_clock);
        let ops = f.ops(&Change::UpsertTask(task), Some(&before));
        let sets: Vec<_> = ops.iter().map(set).collect();
        assert_eq!(
            sets,
            [
                (FieldName::ListId, &FieldValue::ListId(target)),
                (FieldName::SortKey, &FieldValue::SortKey(key)),
            ]
        );
    }

    #[test]
    fn missing_known_fields_count_as_changed() {
        let mut f = fixture();
        let task = f.task();
        let mut partial = task_fields(&task);
        partial.remove(&FieldName::Notes);
        let ops = f.ops(&Change::UpsertTask(task), Some(&partial));
        assert_eq!(ops.len(), 1);
        assert_eq!(set(&ops[0]).0, FieldName::Notes);
    }

    #[test]
    fn list_create_and_rename() {
        let mut f = fixture();
        let mut list = TaskList::new("Privat", SortKey::first(), &f.domain_clock).unwrap();
        let created = f.ops(&Change::UpsertList(list.clone()), None);
        assert!(
            matches!(&created[0].kind, OpKind::Create { fields } if fields == &list_fields(&list))
        );
        let before = list_fields(&list);
        list.rename("Familie", &f.domain_clock).unwrap();
        let ops = f.ops(&Change::UpsertList(list), Some(&before));
        assert_eq!(ops.len(), 1);
        assert_eq!(
            set(&ops[0]),
            (FieldName::Name, &FieldValue::Text("Familie".into()))
        );
        ops[0].validate().unwrap();
    }

    #[test]
    fn delete_known_entity_creates_delete_unknown_creates_nothing() {
        let mut f = fixture();
        let task = f.task();
        let entity = EntityRef::Task(task.id());
        let before = task_fields(&task);
        let ops = f.ops(&Change::Delete(entity), Some(&before));
        assert_eq!(ops.len(), 1);
        assert_eq!((ops[0].entity, &ops[0].kind), (entity, &OpKind::Delete));
        assert!(f.ops(&Change::Delete(entity), None).is_empty());
    }

    #[test]
    fn ops_get_increasing_seq_and_hlc_from_the_device() {
        let mut f = fixture();
        let device = f.clock.hlc().device();
        let mut task = f.task();
        let before = task_fields(&task);
        task.set_notes("bio", &f.domain_clock);
        task.set_priority(Priority::Low, &f.domain_clock);
        let mut ops = f.ops(&Change::UpsertTask(task.clone()), None);
        ops.extend(f.ops(&Change::UpsertTask(task), Some(&before)));
        assert_eq!(
            ops.iter().map(|op| op.id.seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(ops.windows(2).all(|w| w[0].hlc < w[1].hlc));
        assert!(ops
            .iter()
            .all(|op| op.id.device == device && op.hlc.device == device));
        assert_eq!(f.clock.last_seq(), 3);
        assert_eq!(f.clock.hlc().last(), ops[2].hlc);
    }
}
