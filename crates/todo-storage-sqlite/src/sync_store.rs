//! Sync-Buchhaltung auf SQLite (`migrations/0002_sync.sql`): [`ReplicaStore`] über eine
//! Verbindung bzw. offene Transaktion, Geräte-Identität und Übernahme alter Daten.

use std::collections::BTreeMap;

use rusqlite::{named_params, Connection, OptionalExtension, TransactionBehavior};
use todo_core::{
    domain::{DeviceId, Task, TaskId, TaskList, TaskListId},
    ports::{Change, ChangeSet, EntityRef},
    sync::{
        record_local, EntityState, FieldName, FieldValue, Hlc, HlcState, Op, OpClock, Register,
        Registers, ReplicaStore,
    },
    CoreResult,
};

use crate::{
    error::{corrupt, storage},
    mapping::{read_list, read_task, write_list, write_task, LIST_COLUMNS, TASK_COLUMNS},
};

const DEVICE_ID: &str = "device_id";
const HLC: &str = "hlc";
const LAST_SEQ: &str = "last_seq";
/// Präfix für Werte der Sync-Engine in `sync_meta` (getrennt von den eigenen Schlüsseln).
const ENGINE_PREFIX: &str = "engine.";

const KIND_TASK: i64 = 0;
const KIND_LIST: i64 = 1;

/// [`ReplicaStore`] auf einer Verbindung; für Schreibzugriffe innerhalb einer Transaktion
/// verwenden (eine `rusqlite::Transaction` dereferenziert auf `Connection`).
pub(crate) struct SqliteReplica<'c> {
    conn: &'c Connection,
}

impl<'c> SqliteReplica<'c> {
    pub(crate) fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }
}

/// Erzeugt beim ersten Öffnen die Geräte-ID und übernimmt vorhandene Daten (Datenbanken von
/// vor dem Sync) als `Create`-Ops in die Outbox, damit sie beim ersten Sync hochgeladen werden.
pub(crate) fn init(conn: &mut Connection) -> CoreResult<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    if meta::<String>(&tx, DEVICE_ID)?.is_some() {
        return Ok(());
    }
    let device = DeviceId::new();
    set_meta(&tx, DEVICE_ID, &device.to_string())?;
    SqliteReplica::new(&tx).set_op_clock(&OpClock::new(HlcState::new(device), 0))?;

    let mut existing = ChangeSet::new();
    for list in read_all(
        &tx,
        &format!("SELECT {LIST_COLUMNS} FROM task_lists"),
        read_list,
    )? {
        existing.push(Change::UpsertList(list));
    }
    for task in read_all(&tx, &format!("SELECT {TASK_COLUMNS} FROM tasks"), read_task)? {
        existing.push(Change::UpsertTask(task));
    }
    record_local(&mut SqliteReplica::new(&tx), existing)?;
    tx.commit().map_err(storage)
}

fn read_all<T>(
    conn: &Connection,
    sql: &str,
    read: impl Fn(&rusqlite::Row<'_>) -> CoreResult<T>,
) -> CoreResult<Vec<T>> {
    let mut stmt = conn.prepare(sql).map_err(storage)?;
    let mut rows = stmt.query([]).map_err(storage)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(storage)? {
        out.push(read(row)?);
    }
    Ok(out)
}

fn meta<T: rusqlite::types::FromSql>(conn: &Connection, key: &str) -> CoreResult<Option<T>> {
    conn.prepare_cached("SELECT value FROM sync_meta WHERE key = ?1")
        .and_then(|mut stmt| stmt.query_row([key], |row| row.get(0)).optional())
        .map_err(storage)
}

fn set_meta(conn: &Connection, key: &str, value: &dyn rusqlite::ToSql) -> CoreResult<()> {
    conn.prepare_cached(
        "INSERT INTO sync_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .and_then(|mut stmt| stmt.execute(rusqlite::params![key, value]))
    .map_err(storage)?;
    Ok(())
}

pub(crate) fn device_id(conn: &Connection) -> CoreResult<DeviceId> {
    let id: String = meta(conn, DEVICE_ID)?.ok_or_else(|| corrupt("sync_meta", "no device id"))?;
    DeviceId::parse(&id).map_err(|e| corrupt("device id", e))
}

pub(crate) fn engine_value(conn: &Connection, key: &str) -> CoreResult<Option<String>> {
    meta(conn, &format!("{ENGINE_PREFIX}{key}"))
}

pub(crate) fn set_engine_value(conn: &Connection, key: &str, value: &str) -> CoreResult<()> {
    set_meta(conn, &format!("{ENGINE_PREFIX}{key}"), &value)
}

pub(crate) fn pending_ops(conn: &Connection, limit: u32) -> CoreResult<Vec<Op>> {
    let mut stmt = conn
        .prepare_cached("SELECT op FROM outbox_ops ORDER BY seq LIMIT ?1")
        .map_err(storage)?;
    let mut rows = stmt.query([limit]).map_err(storage)?;
    let mut ops = Vec::new();
    while let Some(row) = rows.next().map_err(storage)? {
        let bytes: Vec<u8> = row.get(0).map_err(storage)?;
        ops.push(Op::decode(&bytes).map_err(|e| corrupt("outbox op", e))?);
    }
    Ok(ops)
}

pub(crate) fn pending_op_count(conn: &Connection) -> CoreResult<u64> {
    let n: i64 = conn
        .query_row("SELECT count(*) FROM outbox_ops", [], |row| row.get(0))
        .map_err(storage)?;
    Ok(u64::try_from(n).unwrap_or(0))
}

pub(crate) fn mark_ops_pushed(conn: &Connection, up_to: u64) -> CoreResult<()> {
    conn.prepare_cached("DELETE FROM outbox_ops WHERE seq <= ?1")
        .and_then(|mut stmt| stmt.execute([sql_seq(up_to)]))
        .map_err(storage)?;
    Ok(())
}

pub(crate) fn sync_cursor(conn: &Connection, device: DeviceId) -> CoreResult<Option<u64>> {
    let seq: Option<i64> = conn
        .prepare_cached("SELECT seq FROM remote_cursors WHERE device_id = ?1")
        .and_then(|mut stmt| {
            stmt.query_row([device.to_string()], |row| row.get(0))
                .optional()
        })
        .map_err(storage)?;
    seq.map(|s| u64::try_from(s).map_err(|e| corrupt("cursor", e)))
        .transpose()
}

pub(crate) fn sync_cursors(conn: &Connection) -> CoreResult<Vec<(DeviceId, u64)>> {
    let mut stmt = conn
        .prepare_cached("SELECT device_id, seq FROM remote_cursors ORDER BY device_id")
        .map_err(storage)?;
    let mut rows = stmt.query([]).map_err(storage)?;
    let mut cursors = Vec::new();
    while let Some(row) = rows.next().map_err(storage)? {
        let device: String = row.get(0).map_err(storage)?;
        let seq: i64 = row.get(1).map_err(storage)?;
        cursors.push((
            DeviceId::parse(&device).map_err(|e| corrupt("cursor device", e))?,
            u64::try_from(seq).map_err(|e| corrupt("cursor", e))?,
        ));
    }
    Ok(cursors)
}

/// Alle Register und Tombstones, nach Entity sortiert.
pub(crate) fn entity_states(conn: &Connection) -> CoreResult<Vec<(EntityRef, EntityState)>> {
    let mut states: BTreeMap<EntityRef, EntityState> = BTreeMap::new();
    let mut stmt = conn
        .prepare_cached("SELECT field, hlc, value, entity_id, entity_kind FROM field_clocks")
        .map_err(storage)?;
    let mut rows = stmt.query([]).map_err(storage)?;
    while let Some(row) = rows.next().map_err(storage)? {
        let entity = read_entity(row, 3)?;
        let (field, register) = read_register(row)?;
        if let EntityState::Live(registers) = states
            .entry(entity)
            .or_insert_with(|| EntityState::Live(Registers::new()))
        {
            registers.insert(field, register);
        }
    }
    let mut stmt = conn
        .prepare_cached("SELECT hlc, entity_id, entity_kind FROM tombstones")
        .map_err(storage)?;
    let mut rows = stmt.query([]).map_err(storage)?;
    while let Some(row) = rows.next().map_err(storage)? {
        let hlc: Vec<u8> = row.get(0).map_err(storage)?;
        let hlc = Hlc::from_bytes(&hlc).map_err(|e| corrupt("hlc", e))?;
        states.insert(read_entity(row, 1)?, EntityState::Deleted(hlc));
    }
    Ok(states.into_iter().collect())
}

/// Entity aus den Spalten `entity_id` (bei `at`) und `entity_kind` (bei `at + 1`).
fn read_entity(row: &rusqlite::Row<'_>, at: usize) -> CoreResult<EntityRef> {
    let id: String = row.get(at).map_err(storage)?;
    let kind: i64 = row.get(at + 1).map_err(storage)?;
    match kind {
        KIND_TASK => Ok(EntityRef::Task(
            TaskId::parse(&id).map_err(|e| corrupt("task id", e))?,
        )),
        KIND_LIST => Ok(EntityRef::TaskList(
            TaskListId::parse(&id).map_err(|e| corrupt("list id", e))?,
        )),
        other => Err(corrupt("entity kind", other)),
    }
}

/// SQLite-Ganzzahlen sind vorzeichenbehaftet; Op-Nummern erreichen 2^63 nie.
fn sql_seq(seq: u64) -> i64 {
    i64::try_from(seq).unwrap_or(i64::MAX)
}

fn entity_key(entity: EntityRef) -> (String, i64) {
    match entity {
        EntityRef::Task(id) => (id.to_string(), KIND_TASK),
        EntityRef::TaskList(id) => (id.to_string(), KIND_LIST),
    }
}

fn read_register(row: &rusqlite::Row<'_>) -> CoreResult<(FieldName, Register)> {
    let code: i64 = row.get(0).map_err(storage)?;
    let field = u8::try_from(code)
        .ok()
        .and_then(FieldName::from_code)
        .ok_or_else(|| corrupt("field code", code))?;
    let hlc: Vec<u8> = row.get(1).map_err(storage)?;
    let value: Vec<u8> = row.get(2).map_err(storage)?;
    Ok((
        field,
        Register {
            hlc: Hlc::from_bytes(&hlc).map_err(|e| corrupt("hlc", e))?,
            value: FieldValue::decode(&value).map_err(|e| corrupt("field value", e))?,
        },
    ))
}

impl ReplicaStore for SqliteReplica<'_> {
    fn op_clock(&mut self) -> CoreResult<OpClock> {
        let hlc: Vec<u8> = meta(self.conn, HLC)?.ok_or_else(|| corrupt("sync_meta", "no hlc"))?;
        let last_seq: i64 =
            meta(self.conn, LAST_SEQ)?.ok_or_else(|| corrupt("sync_meta", "no last_seq"))?;
        let last = Hlc::from_bytes(&hlc).map_err(|e| corrupt("hlc", e))?;
        Ok(OpClock::new(
            HlcState::restore(last),
            u64::try_from(last_seq).map_err(|e| corrupt("last_seq", e))?,
        ))
    }

    fn set_op_clock(&mut self, clock: &OpClock) -> CoreResult<()> {
        set_meta(self.conn, HLC, &clock.hlc().last().to_bytes().to_vec())?;
        set_meta(self.conn, LAST_SEQ, &sql_seq(clock.last_seq()))
    }

    fn entity_state(&mut self, entity: EntityRef) -> CoreResult<Option<EntityState>> {
        let (id, _) = entity_key(entity);
        let tombstone: Option<Vec<u8>> = self
            .conn
            .prepare_cached("SELECT hlc FROM tombstones WHERE entity_id = ?1")
            .and_then(|mut stmt| stmt.query_row([&id], |row| row.get(0)).optional())
            .map_err(storage)?;
        if let Some(hlc) = tombstone {
            return Ok(Some(EntityState::Deleted(
                Hlc::from_bytes(&hlc).map_err(|e| corrupt("hlc", e))?,
            )));
        }
        let mut stmt = self
            .conn
            .prepare_cached("SELECT field, hlc, value FROM field_clocks WHERE entity_id = ?1")
            .map_err(storage)?;
        let mut rows = stmt.query([&id]).map_err(storage)?;
        let mut registers = Registers::new();
        while let Some(row) = rows.next().map_err(storage)? {
            let (field, register) = read_register(row)?;
            registers.insert(field, register);
        }
        Ok((!registers.is_empty()).then_some(EntityState::Live(registers)))
    }

    fn set_entity_state(
        &mut self,
        entity: EntityRef,
        before: Option<&EntityState>,
        state: &EntityState,
    ) -> CoreResult<()> {
        let (id, kind) = entity_key(entity);
        match state {
            EntityState::Deleted(hlc) => {
                self.conn
                    .prepare_cached("DELETE FROM field_clocks WHERE entity_id = ?1")
                    .and_then(|mut stmt| stmt.execute([&id]))
                    .map_err(storage)?;
                self.conn
                    .prepare_cached(
                        "INSERT INTO tombstones (entity_id, entity_kind, hlc) VALUES (?1, ?2, ?3)
                         ON CONFLICT (entity_id) DO UPDATE SET hlc = excluded.hlc",
                    )
                    .and_then(|mut stmt| {
                        stmt.execute(rusqlite::params![id, kind, hlc.to_bytes().to_vec()])
                    })
                    .map_err(storage)?;
            }
            EntityState::Live(registers) => {
                // Nur geänderte Register schreiben: ein Feld-Update kostet so ein Statement statt
                // eines pro Feld (Sammel-Commits mit 1 000 Tasks, P0-01). Register verschwinden
                // nie aus einem lebenden Zustand, daher genügt ein Upsert.
                let old = match before {
                    Some(EntityState::Live(old)) => Some(old),
                    _ => None,
                };
                let mut stmt = self
                    .conn
                    .prepare_cached(
                        "INSERT INTO field_clocks (entity_id, field, entity_kind, hlc, value)
                         VALUES (:id, :field, :kind, :hlc, :value)
                         ON CONFLICT (entity_id, field) DO UPDATE
                         SET hlc = excluded.hlc, value = excluded.value",
                    )
                    .map_err(storage)?;
                for (field, register) in registers {
                    if old.and_then(|old| old.get(field)) == Some(register) {
                        continue;
                    }
                    stmt.execute(named_params! {
                        ":id": id,
                        ":field": field.code(),
                        ":kind": kind,
                        ":hlc": register.hlc.to_bytes().to_vec(),
                        ":value": register.value.encode(),
                    })
                    .map_err(storage)?;
                }
            }
        }
        Ok(())
    }

    fn push_outbox(&mut self, op: &Op) -> CoreResult<()> {
        self.conn
            .prepare_cached("INSERT INTO outbox_ops (seq, op) VALUES (?1, ?2)")
            .and_then(|mut stmt| stmt.execute(rusqlite::params![sql_seq(op.id.seq), op.encode()]))
            .map_err(storage)?;
        Ok(())
    }

    fn set_cursor(&mut self, device: DeviceId, seq: u64) -> CoreResult<()> {
        set_cursor(self.conn, device, seq)
    }

    fn live_tasks(&mut self) -> CoreResult<Vec<(TaskId, Registers)>> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT field, hlc, value, entity_id FROM field_clocks
                 WHERE entity_kind = ?1 ORDER BY entity_id",
            )
            .map_err(storage)?;
        let mut rows = stmt.query([KIND_TASK]).map_err(storage)?;
        let mut tasks: BTreeMap<TaskId, Registers> = BTreeMap::new();
        while let Some(row) = rows.next().map_err(storage)? {
            let id: String = row.get(3).map_err(storage)?;
            let id = TaskId::parse(&id).map_err(|e| corrupt("task id", e))?;
            let (field, register) = read_register(row)?;
            tasks.entry(id).or_default().insert(field, register);
        }
        Ok(tasks.into_iter().collect())
    }

    fn task(&mut self, id: TaskId) -> CoreResult<Option<Task>> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"))
            .map_err(storage)?;
        let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
        rows.next().map_err(storage)?.map(read_task).transpose()
    }

    fn list(&mut self, id: TaskListId) -> CoreResult<Option<TaskList>> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!(
                "SELECT {LIST_COLUMNS} FROM task_lists WHERE id = ?1"
            ))
            .map_err(storage)?;
        let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
        rows.next().map_err(storage)?.map(read_list).transpose()
    }

    fn put_task(&mut self, task: &Task) -> CoreResult<()> {
        write_task(self.conn, task)
    }

    fn put_list(&mut self, list: &TaskList) -> CoreResult<()> {
        write_list(self.conn, list)
    }

    fn remove(&mut self, entity: EntityRef) -> CoreResult<()> {
        let (table, id) = match entity {
            EntityRef::Task(id) => ("tasks", id.to_string()),
            EntityRef::TaskList(id) => ("task_lists", id.to_string()),
        };
        self.conn
            .prepare_cached(&format!("DELETE FROM {table} WHERE id = ?1"))
            .and_then(|mut stmt| stmt.execute([id]))
            .map_err(storage)?;
        Ok(())
    }
}

pub(crate) fn set_cursor(conn: &Connection, device: DeviceId, seq: u64) -> CoreResult<()> {
    conn.prepare_cached(
        "INSERT INTO remote_cursors (device_id, seq) VALUES (?1, ?2)
         ON CONFLICT (device_id) DO UPDATE SET seq = excluded.seq",
    )
    .and_then(|mut stmt| stmt.execute(rusqlite::params![device.to_string(), sql_seq(seq)]))
    .map_err(storage)?;
    Ok(())
}
