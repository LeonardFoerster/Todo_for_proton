//! Sync-Buchhaltung eines Replikats: lokale Commits in Ops übersetzen, empfangene Ops mergen und
//! materialisieren (ADR-015).
//!
//! Einmal implementiert, für alle Speicher gleich: Repository-Implementierungen rufen
//! [`record_local`] bzw. [`apply_remote`] **innerhalb ihrer Transaktion** auf und stellen über
//! [`ReplicaStore`] nur Lesen und Schreiben bereit.

use std::collections::BTreeMap;

use super::{
    materialize_list, materialize_task, merge, ops_for, EntityState, Op, OpClock, Registers,
};
use crate::{
    domain::{DeviceId, Task, TaskId, TaskList, TaskListId, Timestamp},
    ports::{ChangeSet, Clock, EntityRef},
    CoreResult,
};

/// Empfangene Ops (beliebige Geräte und Reihenfolge, Duplikate erlaubt) plus die Cursor, die
/// nach dem Anwenden gelten – beides in einer Transaktion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBatch {
    pub ops: Vec<Op>,
    /// Pro Gerät: Nummer der letzten verarbeiteten Op.
    pub cursors: Vec<(DeviceId, u64)>,
    /// Aktuelle Zeit (für den Schutz vor falschen Uhren in der HLC).
    pub now: Timestamp,
}

/// Ergebnis von [`apply_remote`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteReceipt {
    /// Entities, deren materialisierter Zustand sich geändert hat (für `CoreEvent::Changed`).
    pub changed: Vec<EntityRef>,
    /// Ops, deren HLC mehr als 24 h vor der eigenen Uhr lag (gemergt, aber nicht beobachtet).
    pub clock_skews: u32,
}

/// Lese-/Schreibzugriff eines Speichers innerhalb **einer** Transaktion.
///
/// „Register" sind der Sync-Zustand pro Entity ([`EntityState`]), die Tabellen für Tasks und
/// Listen die daraus materialisierte Sicht.
pub trait ReplicaStore {
    /// HLC-Zustand und letzte Op-Nummer dieses Geräts.
    ///
    /// # Errors
    /// Speicherfehler.
    fn op_clock(&mut self) -> CoreResult<OpClock>;
    /// # Errors
    /// Speicherfehler.
    fn set_op_clock(&mut self, clock: &OpClock) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn entity_state(&mut self, entity: EntityRef) -> CoreResult<Option<EntityState>>;
    /// Ersetzt den Zustand. `before` ist der zuvor per [`ReplicaStore::entity_state`] gelesene
    /// Zustand – Speicher können damit nur geänderte Register schreiben (Sammel-Commits, P0-01).
    ///
    /// # Errors
    /// Speicherfehler.
    fn set_entity_state(
        &mut self,
        entity: EntityRef,
        before: Option<&EntityState>,
        state: &EntityState,
    ) -> CoreResult<()>;
    /// Hängt eine lokal erzeugte Op an die Outbox an.
    ///
    /// # Errors
    /// Speicherfehler.
    fn push_outbox(&mut self, op: &Op) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn set_cursor(&mut self, device: DeviceId, seq: u64) -> CoreResult<()>;
    /// Alle nicht gelöschten Tasks mit ihren Registern (für die Neuzuordnung nach
    /// Listenänderungen).
    ///
    /// # Errors
    /// Speicherfehler.
    fn live_tasks(&mut self) -> CoreResult<Vec<(TaskId, Registers)>>;

    // --- materialisierte Sicht ---

    /// # Errors
    /// Speicherfehler.
    fn task(&mut self, id: TaskId) -> CoreResult<Option<Task>>;
    /// # Errors
    /// Speicherfehler.
    fn list(&mut self, id: TaskListId) -> CoreResult<Option<TaskList>>;
    /// # Errors
    /// Speicherfehler.
    fn put_task(&mut self, task: &Task) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn put_list(&mut self, list: &TaskList) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn remove(&mut self, entity: EntityRef) -> CoreResult<()>;
}

/// Erzeugt für einen lokalen Commit die Ops (Outbox) und schreibt die Register fort. Liefert
/// die Änderungen, die der Speicher danach auf die Tabellen anwenden soll: Änderungen an
/// gelöschten Entities (Tombstone) entfallen – „Delete gewinnt" gilt auch lokal.
///
/// # Errors
/// Speicherfehler.
pub fn record_local<S: ReplicaStore + ?Sized>(
    store: &mut S,
    changes: ChangeSet,
) -> CoreResult<ChangeSet> {
    let now = changes.time().unwrap_or(Timestamp::EPOCH);
    let mut clock = store.op_clock()?;
    let start = clock;
    let mut effective = ChangeSet::new().at(now);
    for change in changes {
        let entity = change.entity();
        let before = store.entity_state(entity)?;
        if before.as_ref().is_some_and(EntityState::is_deleted) {
            continue;
        }
        let fields = before.as_ref().and_then(EntityState::fields);
        let ops = ops_for(&change, fields.as_ref(), &mut clock, now);
        if let Some(state) = merge(before.as_ref(), &ops).state {
            if !ops.is_empty() {
                store.set_entity_state(entity, before.as_ref(), &state)?;
            }
        }
        for op in &ops {
            store.push_outbox(op)?;
        }
        effective.push(change);
    }
    if clock != start {
        store.set_op_clock(&clock)?;
    }
    Ok(effective)
}

/// Mergt empfangene Ops, materialisiert betroffene Tasks/Listen und setzt die Cursor.
///
/// Tasks, deren Liste (noch) nicht existiert oder gelöscht wurde, landen in der Inbox; ohne
/// Inbox werden sie erst sichtbar, sobald eine der beiden existiert.
///
/// # Errors
/// Speicherfehler.
pub fn apply_remote<S: ReplicaStore + ?Sized>(
    store: &mut S,
    batch: &RemoteBatch,
) -> CoreResult<RemoteReceipt> {
    let mut clock = store.op_clock()?;
    let mut receipt = RemoteReceipt::default();
    let mut groups: BTreeMap<EntityRef, Vec<Op>> = BTreeMap::new();
    for op in &batch.ops {
        // Auch Snapshot-Ops (ohne `Op::validate`): absurde Zeitstempel überall gleich verwerfen.
        if op.hlc.wall_ms > super::hlc::MAX_WALL_MS {
            receipt.clock_skews += 1;
            continue;
        }
        if clock.hlc_mut().observe(&op.hlc, batch.now).is_err() {
            receipt.clock_skews += 1;
        }
        groups.entry(op.entity).or_default().push(op.clone());
    }

    // Erst Listen, damit Tasks ihre Zielliste vorfinden.
    let mut list_set_changed = false;
    for (entity, ops) in groups.iter().filter(|(e, _)| is_list(e)) {
        let EntityRef::TaskList(id) = *entity else {
            unreachable!("filtered")
        };
        let Some(state) = merged(store, *entity, ops)? else {
            continue;
        };
        let target = match &state {
            EntityState::Live(registers) => materialize_list(id, registers),
            EntityState::Deleted(_) => None,
        };
        let current = store.list(id)?;
        if current.as_ref().map(crate::sync::list_fields)
            == target.as_ref().map(crate::sync::list_fields)
        {
            continue;
        }
        list_set_changed |= current.is_some() != target.is_some();
        match &target {
            Some(list) => store.put_list(list)?,
            None => store.remove(*entity)?,
        }
        receipt.changed.push(*entity);
    }
    for (entity, ops) in groups.iter().filter(|(e, _)| !is_list(e)) {
        let EntityRef::Task(id) = *entity else {
            unreachable!("filtered")
        };
        if let Some(state) = merged(store, *entity, ops)? {
            if rematerialize_task(store, id, &state)? {
                receipt.changed.push(*entity);
            }
        }
    }
    if list_set_changed {
        for (id, registers) in store.live_tasks()? {
            let entity = EntityRef::Task(id);
            if !groups.contains_key(&entity)
                && rematerialize_task(store, id, &EntityState::Live(registers))?
            {
                receipt.changed.push(entity);
            }
        }
    }

    for (device, seq) in &batch.cursors {
        store.set_cursor(*device, *seq)?;
    }
    store.set_op_clock(&clock)?;
    Ok(receipt)
}

fn is_list(entity: &EntityRef) -> bool {
    matches!(entity, EntityRef::TaskList(_))
}

/// Mergt und speichert; `Some(neuer Zustand)` nur, wenn sich etwas geändert hat.
fn merged<S: ReplicaStore + ?Sized>(
    store: &mut S,
    entity: EntityRef,
    ops: &[Op],
) -> CoreResult<Option<EntityState>> {
    let before = store.entity_state(entity)?;
    let result = merge(before.as_ref(), ops);
    match result.state {
        Some(state) if result.changed => {
            store.set_entity_state(entity, before.as_ref(), &state)?;
            Ok(Some(state))
        }
        _ => Ok(None),
    }
}

/// Bringt die Tabellenzeile einer Task auf den Stand ihrer Register. `true` bei Änderung.
fn rematerialize_task<S: ReplicaStore + ?Sized>(
    store: &mut S,
    id: TaskId,
    state: &EntityState,
) -> CoreResult<bool> {
    let target = match state {
        EntityState::Live(registers) => match materialize_task(id, registers) {
            Some(task) => place(store, task)?,
            None => None,
        },
        EntityState::Deleted(_) => None,
    };
    let current = store.task(id)?;
    let fields = |t: &Option<Task>| t.as_ref().map(crate::sync::task_fields);
    if fields(&current) == fields(&target) {
        return Ok(false);
    }
    match &target {
        Some(task) => store.put_task(task)?,
        None => store.remove(EntityRef::Task(id))?,
    }
    Ok(true)
}

/// Task in ihrer Liste, ersatzweise in der Inbox; `None`, wenn es keine von beiden gibt.
fn place<S: ReplicaStore + ?Sized>(store: &mut S, mut task: Task) -> CoreResult<Option<Task>> {
    if store.list(task.list_id())?.is_some() {
        return Ok(Some(task));
    }
    if store.list(TaskListId::INBOX)?.is_none() {
        return Ok(None);
    }
    let key = task.sort_key().clone();
    let at = Frozen(task.updated_at());
    task.move_to(TaskListId::INBOX, key, &at);
    Ok(Some(task))
}

/// Uhr, die `updated_at` beim Umhängen unverändert lässt.
struct Frozen(Timestamp);

impl Clock for Frozen {
    fn now(&self) -> Timestamp {
        self.0
    }
}
