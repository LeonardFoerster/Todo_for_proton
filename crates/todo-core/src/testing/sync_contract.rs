//! Contract-Fälle für die Sync-Methoden des [`TaskRepository`] (Outbox, Register, Merge,
//! Cursor). Werden über [`crate::repository_contract_tests!`] mit ausgeführt.

use futures::executor::block_on;

use super::repository_contract::{setup, Fixture};
use crate::{
    domain::{DeviceId, Due, NaiveDate, SortKey, Task, TaskId, TaskList, TaskListId, Timestamp},
    ports::{ChangeSet, EntityRef, FixedClock, TaskRepository},
    sync::{
        list_fields, task_fields, EntityState, FieldName, FieldValue, HlcState, Op, OpClock,
        OpKind, RemoteBatch, MAX_CLOCK_DRIFT_MS,
    },
    CoreError,
};

/// Ein simuliertes zweites Gerät, das Ops erzeugt.
struct Remote {
    clock: OpClock,
}

/// Basiszeit der Fixture-Uhr (siehe `setup`).
const T0: i64 = 1_700_000_000_000;

impl Remote {
    fn new() -> Self {
        Self {
            clock: OpClock::new(HlcState::new(DeviceId::new()), 0),
        }
    }

    fn device(&self) -> DeviceId {
        self.clock.hlc().device()
    }

    fn op(&mut self, at: i64, entity: EntityRef, kind: OpKind) -> Op {
        self.clock
            .stamp(Timestamp::from_millis(T0 + at), entity, kind)
    }

    fn create_task(&mut self, at: i64, task: &Task) -> Op {
        let fields = task_fields(task);
        self.op(at, EntityRef::Task(task.id()), OpKind::Create { fields })
    }

    fn create_list(&mut self, at: i64, list: &TaskList) -> Op {
        let fields = list_fields(list);
        self.op(
            at,
            EntityRef::TaskList(list.id()),
            OpKind::Create { fields },
        )
    }

    fn set(&mut self, at: i64, id: TaskId, field: FieldName, value: FieldValue) -> Op {
        self.op(at, EntityRef::Task(id), OpKind::Set { field, value })
    }

    fn batch(&self, ops: Vec<Op>) -> RemoteBatch {
        let last = ops.iter().map(|op| op.id.seq).max().unwrap_or(0);
        RemoteBatch {
            ops,
            cursors: vec![(self.device(), last)],
            now: Timestamp::from_millis(T0 + 10_000),
        }
    }
}

impl<R: TaskRepository> Fixture<R> {
    fn pending(&self) -> Vec<Op> {
        block_on(self.repo.pending_ops(1_000)).expect("pending_ops")
    }

    fn state(&self, entity: EntityRef) -> Option<EntityState> {
        block_on(self.repo.entity_state(entity)).expect("entity_state")
    }

    fn apply(&self, batch: RemoteBatch) -> Vec<EntityRef> {
        block_on(self.repo.apply_remote(batch))
            .expect("apply_remote")
            .changed
    }
}

fn kinds(ops: &[Op]) -> Vec<(EntityRef, &'static str)> {
    ops.iter()
        .map(|op| {
            let kind = match op.kind {
                OpKind::Create { .. } => "create",
                OpKind::Set { .. } => "set",
                OpKind::Delete => "delete",
            };
            (op.entity, kind)
        })
        .collect()
}

fn title(s: &str) -> FieldValue {
    FieldValue::Text(s.into())
}

// --- Fälle -------------------------------------------------------------------------------------

/// Die Geräte-ID ist stabil und stempelt alle lokalen Ops.
pub fn sync_device_id_is_stable<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let device = block_on(f.repo.device_id()).expect("device_id");
    assert_eq!(block_on(f.repo.device_id()).expect("device_id"), device);
    let pending = f.pending();
    assert!(!pending.is_empty());
    assert!(pending
        .iter()
        .all(|op| op.id.device == device && op.hlc.device == device));
}

/// Jeder Commit schreibt seine Ops in dieselbe Outbox – lückenlos nummeriert, HLC steigend.
pub fn commit_writes_ops_to_outbox<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let pending = f.pending();
    assert_eq!(
        kinds(&pending),
        [
            (EntityRef::TaskList(f.inbox.id()), "create"),
            (EntityRef::TaskList(f.work.id()), "create"),
            (EntityRef::Task(task.id()), "create"),
        ]
    );
    assert_eq!(
        pending.iter().map(|op| op.id.seq).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(pending.windows(2).all(|w| w[0].hlc < w[1].hlc));
    assert_eq!(block_on(f.repo.pending_op_count()).expect("count"), 3);
    let OpKind::Create { fields } = &pending[2].kind else {
        panic!("{:?}", pending[2].kind)
    };
    assert_eq!(fields, &task_fields(&task));
}

/// Geänderte Felder → je ein `Set`; unveränderte Upserts → keine Op.
pub fn update_writes_only_changed_fields<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let before = f.pending().len();
    f.save(std::slice::from_ref(&task));
    assert_eq!(f.pending().len(), before, "unverändert → keine Op");

    task.rename("Neuer Titel", &f.clock).expect("rename");
    f.save(std::slice::from_ref(&task));
    let pending = f.pending();
    assert_eq!(pending.len(), before + 1);
    assert_eq!(
        pending[before].kind,
        OpKind::Set {
            field: FieldName::Title,
            value: title("Neuer Titel")
        }
    );
}

/// Löschen schreibt eine `Delete`-Op und einen Tombstone; spätere Upserts der gelöschten
/// Entity entfallen (Delete gewinnt).
pub fn delete_writes_tombstone_and_op<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let entity = EntityRef::Task(task.id());
    block_on(f.repo.commit(ChangeSet::new().delete(entity))).expect("delete");
    let last = f.pending().pop().expect("op");
    assert_eq!((last.entity, last.kind), (entity, OpKind::Delete));
    assert_eq!(f.state(entity), Some(EntityState::Deleted(last.hlc)));

    let count = f.pending().len();
    f.save(std::slice::from_ref(&task));
    assert_eq!(f.get(task.id()), None, "Tombstone gewinnt");
    assert_eq!(f.pending().len(), count, "keine Op für gelöschte Entity");
}

/// Abgebrochener Commit: weder Zustand noch Ops, Register oder Op-Nummern.
pub fn failed_commit_writes_neither_state_nor_ops<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let before = f.pending();
    let ok = f.task(f.inbox.id(), "ok", SortKey::first());
    let orphan = f.task(TaskListId::new(), "Waise", SortKey::first());
    let result = block_on(
        f.repo
            .commit(ChangeSet::new().upsert_task(ok.clone()).upsert_task(orphan)),
    );
    assert!(
        matches!(result, Err(CoreError::Validation(_))),
        "{result:?}"
    );
    assert_eq!(f.get(ok.id()), None);
    assert_eq!(f.pending(), before, "keine Ops");
    assert_eq!(f.state(EntityRef::Task(ok.id())), None, "keine Register");

    f.add_tasks(f.inbox.id(), 1);
    let next = f.pending().pop().expect("op");
    assert_eq!(
        next.id.seq,
        before.len() as u64 + 1,
        "Nummer nicht verbraucht"
    );
}

/// `mark_ops_pushed` entfernt den Anfang der Outbox; Nummern werden nie wiederverwendet.
pub fn mark_ops_pushed_removes_prefix<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    f.add_tasks(f.inbox.id(), 3);
    let all = f.pending();
    assert_eq!(all.len(), 5);
    let limited = block_on(f.repo.pending_ops(2)).expect("pending");
    assert_eq!(limited, all[..2]);

    block_on(f.repo.mark_ops_pushed(2)).expect("mark");
    block_on(f.repo.mark_ops_pushed(2)).expect("idempotent");
    assert_eq!(f.pending(), all[2..]);
    block_on(f.repo.mark_ops_pushed(5)).expect("mark");
    assert!(f.pending().is_empty());
    assert_eq!(block_on(f.repo.pending_op_count()).expect("count"), 0);

    f.add_tasks(f.work.id(), 1);
    assert_eq!(f.pending()[0].id.seq, 6, "Nummer läuft weiter");
}

/// Register (Field-Clocks): jedes Feld mit der HLC der Op, die es zuletzt gesetzt hat.
pub fn entity_state_tracks_field_clocks<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let entity = EntityRef::Task(task.id());
    let created = f.pending().pop().expect("create");
    let Some(EntityState::Live(registers)) = f.state(entity) else {
        panic!("live state expected")
    };
    assert_eq!(registers.len(), FieldName::TASK.len());
    assert!(registers.values().all(|r| r.hlc == created.hlc));

    task.rename("anders", &f.clock).expect("rename");
    f.save(std::slice::from_ref(&task));
    let renamed = f.pending().pop().expect("set");
    let Some(EntityState::Live(registers)) = f.state(entity) else {
        panic!("live state expected")
    };
    assert_eq!(registers[&FieldName::Title].hlc, renamed.hlc);
    assert_eq!(registers[&FieldName::Title].value, title("anders"));
    assert_eq!(registers[&FieldName::Notes].hlc, created.hlc);
}

/// Empfangene Ops werden materialisiert, setzen den Cursor und landen nicht in der Outbox.
pub fn apply_remote_creates_entities_and_sets_cursor<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let list = TaskList::new("Remote", SortKey::first(), &f.clock).expect("list");
    let task = Task::new(list.id(), "von B", SortKey::first(), &f.clock).expect("task");
    let ops = vec![remote.create_task(2, &task), remote.create_list(1, &list)];
    let outbox = f.pending();

    let changed = f.apply(remote.batch(ops));
    assert_eq!(changed.len(), 2);
    assert!(changed.contains(&EntityRef::Task(task.id())));
    assert!(changed.contains(&EntityRef::TaskList(list.id())));
    let stored = f.get(task.id()).expect("task");
    assert_eq!(task_fields(&stored), task_fields(&task));
    assert_eq!(
        block_on(f.repo.get_task_list(list.id()))
            .expect("get")
            .map(|l| l.name().to_owned()),
        Some("Remote".to_owned())
    );
    assert_eq!(
        block_on(f.repo.sync_cursor(remote.device())).expect("cursor"),
        Some(2)
    );
    assert_eq!(f.pending(), outbox, "nichts in der Outbox");
}

/// Dieselben Ops nochmal: keine Änderung.
pub fn apply_remote_is_idempotent<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let task = f.task(f.inbox.id(), "x", SortKey::first());
    let ops = vec![
        remote.create_task(1, &task),
        remote.set(2, task.id(), FieldName::Title, title("y")),
    ];
    assert_eq!(f.apply(remote.batch(ops.clone())).len(), 1);
    assert!(f.apply(remote.batch(ops.clone())).is_empty());
    let mut reversed = ops;
    reversed.reverse();
    assert!(f.apply(remote.batch(reversed)).is_empty());
    assert_eq!(f.get(task.id()).expect("task").title(), "y");
}

/// Eine Op mit absurder Zukunfts-HLC (auch aus Snapshots, die `Op::validate` nicht durchlaufen)
/// wird verworfen und friert das Feld nicht ein.
pub fn apply_remote_ignores_implausible_future_ops<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let task = f.task(f.inbox.id(), "x", SortKey::first());
    let create = remote.create_task(1, &task);
    let mut frozen = remote.set(2, task.id(), FieldName::Title, title("eingefroren"));
    frozen.hlc.wall_ms = u64::MAX;
    let receipt =
        block_on(f.repo.apply_remote(remote.batch(vec![create, frozen]))).expect("apply_remote");
    assert_eq!(receipt.clock_skews, 1);
    assert_eq!(f.get(task.id()).expect("task").title(), "x");
    let later = remote.set(3, task.id(), FieldName::Title, title("später"));
    f.apply(remote.batch(vec![later]));
    assert_eq!(f.get(task.id()).expect("task").title(), "später");
}

/// Feldweise LWW gegen lokale Änderungen: neuere lokale Werte bleiben, andere Felder kommen an.
pub fn apply_remote_merges_field_wise_with_local<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let mut remote = Remote::new();
    // Reihenfolge: lokales Create (T0 + wenige ms) < Remote-Edits < lokale Umbenennung.
    let due = FieldValue::Due(Some(Due::Date(
        NaiveDate::from_ymd_opt(2026, 10, 20).expect("date"),
    )));
    let old_title = remote.set(500, task.id(), FieldName::Title, title("alt von B"));
    let due_op = remote.set(600, task.id(), FieldName::Due, due);
    task.rename("lokal neu", &FixedClock(Timestamp::from_millis(T0 + 1_000)))
        .expect("rename");
    f.save(std::slice::from_ref(&task));

    f.apply(remote.batch(vec![old_title, due_op]));
    let merged = f.get(task.id()).expect("task");
    assert_eq!(merged.title(), "lokal neu", "lokal ist neuer");
    assert!(merged.due().is_some(), "Fälligkeit von B kommt an");

    let newer = remote.set(5_000, task.id(), FieldName::Title, title("neuer von B"));
    f.apply(remote.batch(vec![newer]));
    assert_eq!(f.get(task.id()).expect("task").title(), "neuer von B");
}

/// Remote-Delete gewinnt gegen lokale (auch spätere) Änderungen.
pub fn apply_remote_delete_wins<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let mut remote = Remote::new();
    let delete = remote.op(-1_000, EntityRef::Task(task.id()), OpKind::Delete);
    task.rename("lokal", &f.clock).expect("rename");
    f.save(std::slice::from_ref(&task));

    let changed = f.apply(remote.batch(vec![delete]));
    assert_eq!(changed, [EntityRef::Task(task.id())]);
    assert_eq!(f.get(task.id()), None);
    assert!(matches!(
        f.state(EntityRef::Task(task.id())),
        Some(EntityState::Deleted(_))
    ));
}

/// Eine Task, deren Liste (noch) fehlt, erscheint in der Inbox und zieht um, sobald die Liste
/// eintrifft; wird die Liste gelöscht, fällt sie zurück in die Inbox.
pub fn apply_remote_task_without_list_goes_to_inbox<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let list = TaskList::new("Später", SortKey::first(), &f.clock).expect("list");
    let task = Task::new(list.id(), "früh da", SortKey::first(), &f.clock).expect("task");
    let list_entity = EntityRef::TaskList(list.id());

    let create_task = remote.create_task(1, &task);
    f.apply(remote.batch(vec![create_task]));
    assert_eq!(f.get(task.id()).expect("task").list_id(), f.inbox.id());

    let create_list = remote.create_list(2, &list);
    let changed = f.apply(remote.batch(vec![create_list]));
    assert!(changed.contains(&EntityRef::Task(task.id())), "{changed:?}");
    assert_eq!(f.get(task.id()).expect("task").list_id(), list.id());

    let delete_list = remote.op(3, list_entity, OpKind::Delete);
    f.apply(remote.batch(vec![delete_list]));
    assert_eq!(
        block_on(f.repo.get_task_list(list.id())).expect("get"),
        None
    );
    assert_eq!(f.get(task.id()).expect("task").list_id(), f.inbox.id());
}

/// `Set`s vor dem `Create` werden gemerkt, aber erst mit dem `Create` sichtbar.
pub fn apply_remote_incomplete_entity_is_not_materialized<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let task = f.task(f.inbox.id(), "x", SortKey::first());
    let create = remote.create_task(1, &task);
    let rename = remote.set(2, task.id(), FieldName::Title, title("später"));

    assert!(f.apply(remote.batch(vec![rename])).is_empty());
    assert_eq!(f.get(task.id()), None);
    assert!(f.state(EntityRef::Task(task.id())).is_some());

    f.apply(remote.batch(vec![create]));
    assert_eq!(f.get(task.id()).expect("task").title(), "später");
}

/// Empfangene HLCs rücken die eigene HLC vor; Werte > 24 h in der Zukunft werden gemergt, aber
/// gezählt und nicht übernommen.
pub fn apply_remote_advances_local_hlc<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let task = f.task(f.inbox.id(), "x", SortKey::first());
    let ahead = remote.create_task(60 * 60 * 1000, &task);
    let receipt = block_on(f.repo.apply_remote(remote.batch(vec![ahead.clone()]))).expect("apply");
    assert_eq!(receipt.clock_skews, 0);
    f.add_tasks(f.work.id(), 1);
    assert!(f.pending().pop().expect("op").hlc > ahead.hlc);

    let far = MAX_CLOCK_DRIFT_MS as i64 + 3_600_000;
    let skewed = remote.set(far, task.id(), FieldName::Notes, title("Zukunft"));
    let receipt = block_on(f.repo.apply_remote(remote.batch(vec![skewed.clone()]))).expect("apply");
    assert_eq!(receipt.clock_skews, 1);
    assert_eq!(
        f.get(task.id()).expect("task").notes(),
        "Zukunft",
        "trotzdem gemergt"
    );
    f.add_tasks(f.work.id(), 1);
    assert!(
        f.pending().pop().expect("op").hlc < skewed.hlc,
        "HLC nicht übernommen"
    );
}

/// Inboxen zweier Geräte laufen zu einer zusammen (feste ID).
pub fn inboxes_of_two_devices_merge<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut remote = Remote::new();
    let other_inbox = TaskList::inbox(SortKey::first(), &f.clock);
    assert_eq!(other_inbox.id(), f.inbox.id());
    let create = remote.create_list(1, &other_inbox);
    f.apply(remote.batch(vec![create]));
    let lists = block_on(f.repo.list_task_lists()).expect("lists");
    assert_eq!(lists.iter().filter(|l| l.is_inbox()).count(), 1);
}

/// Der Export enthält Register aller lebenden und Tombstones aller gelöschten Entities.
pub fn entity_states_export_everything<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let tasks = f.add_tasks(f.inbox.id(), 2);
    let gone = EntityRef::Task(tasks[1].id());
    block_on(f.repo.commit(ChangeSet::new().delete(gone))).expect("delete");
    let states = block_on(f.repo.entity_states()).expect("export");
    let entities: Vec<EntityRef> = states.iter().map(|(e, _)| *e).collect();
    let mut expected = vec![
        EntityRef::Task(tasks[0].id()),
        gone,
        EntityRef::TaskList(f.inbox.id()),
        EntityRef::TaskList(f.work.id()),
    ];
    expected.sort();
    assert_eq!(entities, expected);
    for (entity, state) in &states {
        assert_eq!(Some(state), f.state(*entity).as_ref());
    }
    assert!(matches!(
        states.iter().find(|(e, _)| *e == gone),
        Some((_, EntityState::Deleted(_)))
    ));
}

/// Cursor und Werte der Engine bleiben gespeichert.
pub fn sync_cursors_and_values_roundtrip<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let device = DeviceId::new();
    assert_eq!(block_on(f.repo.sync_cursor(device)).expect("cursor"), None);
    block_on(f.repo.set_sync_cursor(device, 7)).expect("set");
    block_on(f.repo.set_sync_cursor(device, 9)).expect("set");
    assert_eq!(
        block_on(f.repo.sync_cursor(device)).expect("cursor"),
        Some(9)
    );

    assert_eq!(block_on(f.repo.sync_value("k")).expect("get"), None);
    block_on(f.repo.set_sync_value("k", "v1".into())).expect("set");
    block_on(f.repo.set_sync_value("k", "v2".into())).expect("set");
    assert_eq!(
        block_on(f.repo.sync_value("k")).expect("get"),
        Some("v2".to_owned())
    );
}
