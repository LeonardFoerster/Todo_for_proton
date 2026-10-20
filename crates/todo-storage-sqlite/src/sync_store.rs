//! Sync-Buchhaltung auf SQLite (`migrations/0002_sync.sql`): [`ReplicaStore`] über eine
//! Verbindung bzw. offene Transaktion, Geräte-Identität und Übernahme alter Daten.

use std::collections::BTreeMap;

use rusqlite::{named_params, Connection, OptionalExtension, TransactionBehavior};
use todo_core::{
    domain::{
        DeviceId, Note, NoteFolder, NoteFolderId, NoteId, RevisionId, SortKey, Task, TaskId,
        TaskList, TaskListId,
    },
    ports::{Change, ChangeSet, EntityRef},
    sync::{
        record_local, EntityState, FieldName, FieldValue, Hlc, HlcState, Op, OpClock, OpId, OpKind,
        Register, Registers, ReplicaStore, Revision, RevisionSet,
    },
    CoreResult,
};

use crate::{
    error::{corrupt, storage},
    mapping::{
        folder_id, note_id, read_folder, read_list, read_note, read_sort_key, read_task,
        write_folder, write_list, write_note, write_task, FOLDER_COLUMNS, LIST_COLUMNS,
        NOTE_COLUMNS, TASK_COLUMNS,
    },
};

const DEVICE_ID: &str = "device_id";
const HLC: &str = "hlc";
const LAST_SEQ: &str = "last_seq";
/// Präfix für Werte der Sync-Engine in `sync_meta` (getrennt von den eigenen Schlüsseln).
const ENGINE_PREFIX: &str = "engine.";

/// Bis zu dieser Op-Nummer ist die Outbox zum Upload herausgegeben (`freeze_outbox`).
const OUTBOX_FROZEN: &str = "outbox_frozen";

const KIND_TASK: i64 = 0;
const KIND_LIST: i64 = 1;
const KIND_NOTE: i64 = 2;
const KIND_FOLDER: i64 = 3;

/// `outbox_ops.slot` für Revise- und Create-Ops (Set: Feld-Code).
const SLOT_REVISE: i64 = -1;
const SLOT_CREATE: i64 = 0;

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

pub(crate) fn freeze_outbox(conn: &Connection, up_to: u64) -> CoreResult<()> {
    let current: i64 = meta(conn, OUTBOX_FROZEN)?.unwrap_or(0);
    if sql_seq(up_to) > current {
        set_meta(conn, OUTBOX_FROZEN, &sql_seq(up_to))?;
    }
    Ok(())
}

/// Alle Revisionen nicht gelöschter Notizen als `Revise`-Ops, nach Notiz und ID sortiert.
pub(crate) fn revision_ops(conn: &Connection) -> CoreResult<Vec<Op>> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT note_id, rev_device, rev_seq, hlc, parents, text FROM note_revisions r
             WHERE NOT EXISTS (SELECT 1 FROM tombstones t WHERE t.entity_id = r.note_id)
             ORDER BY note_id, rev_device, rev_seq",
        )
        .map_err(storage)?;
    let mut rows = stmt.query([]).map_err(storage)?;
    let mut ops = Vec::new();
    while let Some(row) = rows.next().map_err(storage)? {
        let note = note_id(&row.get::<_, String>(0).map_err(storage)?)?;
        let rev = read_revision(row, 1)?;
        ops.push(todo_core::sync::revise_op(note, &rev));
    }
    Ok(ops)
}

/// Revision aus den Spalten `rev_device, rev_seq, hlc, parents, text` ab `at`.
fn read_revision(row: &rusqlite::Row<'_>, at: usize) -> CoreResult<Revision> {
    let device: String = row.get(at).map_err(storage)?;
    let seq: i64 = row.get(at + 1).map_err(storage)?;
    let hlc: Vec<u8> = row.get(at + 2).map_err(storage)?;
    let parents: Vec<u8> = row.get(at + 3).map_err(storage)?;
    let parents: Vec<OpId> =
        postcard::from_bytes(&parents).map_err(|e| corrupt("revision parents", e))?;
    Ok(Revision {
        id: RevisionId {
            device: DeviceId::parse(&device).map_err(|e| corrupt("revision device", e))?,
            seq: u64::try_from(seq).map_err(|e| corrupt("revision seq", e))?,
        },
        hlc: Hlc::from_bytes(&hlc).map_err(|e| corrupt("hlc", e))?,
        parents: parents.into_iter().map(Into::into).collect(),
        text: row.get(at + 4).map_err(storage)?,
    })
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
        KIND_NOTE => Ok(EntityRef::Note(note_id(&id)?)),
        KIND_FOLDER => Ok(EntityRef::NoteFolder(folder_id(&id)?)),
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
        EntityRef::Note(id) => (id.to_string(), KIND_NOTE),
        EntityRef::NoteFolder(id) => (id.to_string(), KIND_FOLDER),
    }
}

/// Spalten `entity_id`, `slot` einer Outbox-Zeile (siehe `0004_notes.sql`).
fn outbox_slot(op: &Op) -> (String, Option<i64>) {
    let slot = match &op.kind {
        OpKind::Revise { .. } => Some(SLOT_REVISE),
        OpKind::Create { .. } => Some(SLOT_CREATE),
        OpKind::Set { field, .. } => Some(i64::from(field.code())),
        OpKind::Delete => None,
    };
    (entity_key(op.entity).0, slot)
}

/// Registers aller lebenden Entities einer Art.
fn live_registers(conn: &Connection, kind: i64) -> CoreResult<Vec<(String, Registers)>> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT field, hlc, value, entity_id FROM field_clocks
             WHERE entity_kind = ?1 ORDER BY entity_id",
        )
        .map_err(storage)?;
    let mut rows = stmt.query([kind]).map_err(storage)?;
    let mut out: Vec<(String, Registers)> = Vec::new();
    while let Some(row) = rows.next().map_err(storage)? {
        let id: String = row.get(3).map_err(storage)?;
        let (field, register) = read_register(row)?;
        match out.last_mut() {
            Some((last, registers)) if *last == id => {
                registers.insert(field, register);
            }
            _ => out.push((id, Registers::from([(field, register)]))),
        }
    }
    Ok(out)
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
        let (entity, slot) = outbox_slot(op);
        self.conn
            .prepare_cached(
                "INSERT INTO outbox_ops (seq, op, entity_id, slot) VALUES (?1, ?2, ?3, ?4)",
            )
            .and_then(|mut stmt| {
                stmt.execute(rusqlite::params![
                    sql_seq(op.id.seq),
                    op.encode(),
                    entity,
                    slot
                ])
            })
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

    fn revisions(&mut self, note: NoteId) -> CoreResult<RevisionSet> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT rev_device, rev_seq, hlc, parents, text FROM note_revisions
                 WHERE note_id = ?1",
            )
            .map_err(storage)?;
        let mut rows = stmt.query([note.to_string()]).map_err(storage)?;
        let mut revs = RevisionSet::new();
        while let Some(row) = rows.next().map_err(storage)? {
            revs.insert(read_revision(row, 0)?);
        }
        Ok(revs)
    }

    fn put_revision(&mut self, note: NoteId, rev: &Revision) -> CoreResult<()> {
        let parents: Vec<OpId> = rev.parents.iter().map(|p| (*p).into()).collect();
        let parents = postcard::to_allocvec(&parents).expect("serializing to a Vec cannot fail");
        self.conn
            .prepare_cached(
                "INSERT INTO note_revisions (note_id, rev_device, rev_seq, hlc, parents, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (note_id, rev_device, rev_seq) DO UPDATE
                 SET hlc = excluded.hlc, parents = excluded.parents, text = excluded.text",
            )
            .and_then(|mut stmt| {
                stmt.execute(rusqlite::params![
                    note.to_string(),
                    rev.id.device.to_string(),
                    sql_seq(rev.id.seq),
                    rev.hlc.to_bytes().to_vec(),
                    parents,
                    rev.text,
                ])
            })
            .map_err(storage)?;
        Ok(())
    }

    fn remove_revisions(&mut self, note: NoteId, ids: Option<&[RevisionId]>) -> CoreResult<()> {
        match ids {
            None => {
                self.conn
                    .prepare_cached("DELETE FROM note_revisions WHERE note_id = ?1")
                    .and_then(|mut stmt| stmt.execute([note.to_string()]))
                    .map_err(storage)?;
            }
            Some(ids) => {
                let mut stmt = self
                    .conn
                    .prepare_cached(
                        "DELETE FROM note_revisions
                         WHERE note_id = ?1 AND rev_device = ?2 AND rev_seq = ?3",
                    )
                    .map_err(storage)?;
                for id in ids {
                    stmt.execute(rusqlite::params![
                        note.to_string(),
                        id.device.to_string(),
                        sql_seq(id.seq)
                    ])
                    .map_err(storage)?;
                }
            }
        }
        Ok(())
    }

    fn notes_with_revisions(&mut self) -> CoreResult<Vec<NoteId>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT note_id FROM note_revisions ORDER BY note_id")
            .map_err(storage)?;
        let mut rows = stmt.query([]).map_err(storage)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(storage)? {
            out.push(note_id(&row.get::<_, String>(0).map_err(storage)?)?);
        }
        Ok(out)
    }

    fn replaceable_op(
        &mut self,
        entity: EntityRef,
        field: Option<FieldName>,
    ) -> CoreResult<Option<Op>> {
        let (id, _) = entity_key(entity);
        let slot = field.map_or(SLOT_REVISE, |f| i64::from(f.code()));
        let frozen: i64 = meta(self.conn, OUTBOX_FROZEN)?.unwrap_or(0);
        let found: Option<(i64, Vec<u8>)> = self
            .conn
            .prepare_cached(
                "SELECT seq, op FROM outbox_ops
                 WHERE entity_id = ?1 AND (slot = ?2 OR (slot = ?3 AND ?2 <> ?4))
                 ORDER BY seq DESC LIMIT 1",
            )
            .and_then(|mut stmt| {
                stmt.query_row(
                    rusqlite::params![id, slot, SLOT_CREATE, SLOT_REVISE],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
            })
            .map_err(storage)?;
        let Some((_, bytes)) = found.filter(|(seq, _)| *seq > frozen) else {
            return Ok(None);
        };
        let op = Op::decode(&bytes).map_err(|e| corrupt("outbox op", e))?;
        Ok((!matches!(op.kind, OpKind::Create { .. })).then_some(op))
    }

    fn replace_outbox(&mut self, op: &Op) -> CoreResult<()> {
        let (entity, slot) = outbox_slot(op);
        self.conn
            .prepare_cached(
                "UPDATE outbox_ops SET op = ?2, entity_id = ?3, slot = ?4 WHERE seq = ?1",
            )
            .and_then(|mut stmt| {
                stmt.execute(rusqlite::params![
                    sql_seq(op.id.seq),
                    op.encode(),
                    entity,
                    slot
                ])
            })
            .map_err(storage)?;
        Ok(())
    }

    fn live_note_folders(&mut self) -> CoreResult<Vec<(NoteFolderId, Registers)>> {
        live_registers(self.conn, KIND_FOLDER)?
            .into_iter()
            .map(|(id, registers)| Ok((folder_id(&id)?, registers)))
            .collect()
    }

    fn live_notes(&mut self) -> CoreResult<Vec<(NoteId, Registers)>> {
        live_registers(self.conn, KIND_NOTE)?
            .into_iter()
            .map(|(id, registers)| Ok((note_id(&id)?, registers)))
            .collect()
    }

    fn next_sort_key(
        &mut self,
        folder: Option<NoteFolderId>,
        after: &SortKey,
    ) -> CoreResult<Option<SortKey>> {
        let folder = folder.map(|f| f.to_string());
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT min(sort_key) FROM (
                     SELECT min(sort_key) AS sort_key FROM notes
                     WHERE folder_id IS ?1 AND sort_key > ?2
                     UNION ALL
                     SELECT min(sort_key) FROM note_folders
                     WHERE parent_id IS ?1 AND sort_key > ?2)",
            )
            .map_err(storage)?;
        let mut rows = stmt
            .query(rusqlite::params![folder, after.as_str()])
            .map_err(storage)?;
        match rows.next().map_err(storage)? {
            Some(row) if row.get_ref(0).map_err(storage)? != rusqlite::types::ValueRef::Null => {
                Ok(Some(read_sort_key(row, 0)?))
            }
            _ => Ok(None),
        }
    }

    fn note(&mut self, id: NoteId) -> CoreResult<Option<Note>> {
        read_note_row(self.conn, id)
    }

    fn note_folder(&mut self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>> {
        read_folder_row(self.conn, id)
    }

    fn put_note(&mut self, note: &Note) -> CoreResult<()> {
        write_note(self.conn, note)
    }

    fn put_note_folder(&mut self, folder: &NoteFolder) -> CoreResult<()> {
        write_folder(self.conn, folder)
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
            EntityRef::Note(id) => ("notes", id.to_string()),
            EntityRef::NoteFolder(id) => ("note_folders", id.to_string()),
        };
        self.conn
            .prepare_cached(&format!("DELETE FROM {table} WHERE id = ?1"))
            .and_then(|mut stmt| stmt.execute([id]))
            .map_err(storage)?;
        Ok(())
    }
}

pub(crate) fn read_note_row(conn: &Connection, id: NoteId) -> CoreResult<Option<Note>> {
    let mut stmt = conn
        .prepare_cached(&format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"))
        .map_err(storage)?;
    let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
    rows.next().map_err(storage)?.map(read_note).transpose()
}

pub(crate) fn read_folder_row(
    conn: &Connection,
    id: NoteFolderId,
) -> CoreResult<Option<NoteFolder>> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "SELECT {FOLDER_COLUMNS} FROM note_folders WHERE id = ?1"
        ))
        .map_err(storage)?;
    let mut rows = stmt.query([id.to_string()]).map_err(storage)?;
    rows.next().map_err(storage)?.map(read_folder).transpose()
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
