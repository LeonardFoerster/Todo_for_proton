//! Sync-Buchhaltung eines Replikats: lokale Commits in Ops übersetzen, empfangene Ops mergen und
//! materialisieren (ADR-015).
//!
//! Einmal implementiert, für alle Speicher gleich: Repository-Implementierungen rufen
//! [`record_local`] bzw. [`apply_remote`] **innerhalb ihrer Transaktion** auf und stellen über
//! [`ReplicaStore`] nur Lesen und Schreiben bereit.
//!
//! Notizen (ADR-029): Der Inhalt läuft über Revisionen ([`RevisionSet`]). Bleiben nach einem
//! Commit oder Pull mehrere Köpfe mit verschiedenem Text, legt **derselbe** Commit sie zu einer
//! Merge-Revision zusammen und legt bei Konflikten eine Konfliktkopie an (`ResolveNoteHeads`).

use std::collections::BTreeMap;

use super::{
    conflict_copy_id, materialize_list, materialize_note, materialize_note_folder, materialize_tag,
    materialize_task, merge, merge::folder, merge::wall, ops_for, ConflictLoser, EntityState,
    FieldName, Op, OpClock, OpKind, Registers, Resolution, Revision, RevisionSet,
};
use crate::{
    domain::{
        note_tree, DeviceId, Note, NoteFolder, NoteFolderId, NoteId, NoteName, RevisionId, SortKey,
        Tag, TagId, Task, TaskId, TaskList, TaskListId, Timestamp, NOTE_NAME_MAX_CHARS,
    },
    ports::{Change, ChangeSet, Clock, EntityRef},
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

/// Eine beim Zusammenlegen entstandene Konfliktkopie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteConflict {
    pub note: NoteId,
    pub copy: NoteId,
}

/// Ergebnis von [`apply_remote`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteReceipt {
    /// Entities, deren materialisierter Zustand sich geändert hat (für `CoreEvent::Changed`).
    pub changed: Vec<EntityRef>,
    /// Ops, deren HLC mehr als 24 h vor der eigenen Uhr lag (gemergt, aber nicht beobachtet).
    pub clock_skews: u32,
    /// Neu angelegte Konfliktkopien (für den Hinweis in der UI).
    pub conflicts: Vec<NoteConflict>,
}

/// Ergebnis von [`record_local`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recorded {
    /// Die Änderungen, die der Speicher auf seine Tabellen anwenden soll (ohne solche an
    /// gelöschten Entities, Notizen mit aufgelöstem Inhalt, plus Konfliktkopien).
    pub changes: ChangeSet,
    pub conflicts: Vec<NoteConflict>,
}

impl Recorded {
    /// Geänderte Entities für den `CommitReceipt`: die angefragten plus zusätzlich entstandene.
    #[must_use]
    pub fn changed(&self, requested: Vec<EntityRef>) -> Vec<EntityRef> {
        let mut changed = requested;
        for entity in self.changes.entities() {
            if !changed.contains(&entity) {
                changed.push(entity);
            }
        }
        changed
    }
}

/// Lese-/Schreibzugriff eines Speichers innerhalb **einer** Transaktion.
///
/// „Register" sind der Sync-Zustand pro Entity ([`EntityState`]), die Tabellen für Tasks,
/// Listen, Notizen und Ordner die daraus materialisierte Sicht.
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

    // --- Notizen (ADR-029) ---

    /// Alle gespeicherten Revisionen einer Notiz (mit Texten – nur für das Zusammenlegen).
    ///
    /// # Errors
    /// Speicherfehler.
    fn revisions(&mut self, note: NoteId) -> CoreResult<RevisionSet>;
    /// Nur die Köpfe (Revisionen, die keine andere gespeicherte als Elternteil nennt). Der
    /// häufige Fall – ein Kopf – kommt so ohne die Texte älterer Revisionen aus.
    ///
    /// # Errors
    /// Speicherfehler.
    fn revision_heads(&mut self, note: NoteId) -> CoreResult<Vec<Revision>>;
    /// Eine einzelne Revision.
    ///
    /// # Errors
    /// Speicherfehler.
    fn revision(&mut self, note: NoteId, id: RevisionId) -> CoreResult<Option<Revision>>;
    /// Legt eine Revision ab oder ersetzt die mit gleicher ID.
    ///
    /// # Errors
    /// Speicherfehler.
    fn put_revision(&mut self, note: NoteId, rev: &Revision) -> CoreResult<()>;
    /// Entfernt diese Revisionen (`None` = alle der Notiz).
    ///
    /// # Errors
    /// Speicherfehler.
    fn remove_revisions(&mut self, note: NoteId, ids: Option<&[RevisionId]>) -> CoreResult<()>;
    /// Notizen, zu denen Revisionen gespeichert sind (Aufräumen).
    ///
    /// # Errors
    /// Speicherfehler.
    fn notes_with_revisions(&mut self) -> CoreResult<Vec<NoteId>>;
    /// Eine Outbox-Op, die eine neue Bearbeitung ersetzen darf (§4.6): `field = None` → die
    /// neueste `Revise`-Op der Entity, `Some(f)` → die neueste Op, die Feld `f` setzt, wenn sie
    /// ein `Set` ist (kein `Create`). Nur wenn sie noch **nicht** zum Upload herausgegeben wurde
    /// (Nummer über der Grenze von `freeze_outbox`); sonst `None`.
    ///
    /// # Errors
    /// Speicherfehler.
    fn replaceable_op(
        &mut self,
        entity: EntityRef,
        field: Option<FieldName>,
    ) -> CoreResult<Option<Op>>;
    /// Ersetzt die Outbox-Op mit derselben Nummer (Zusammenfassen von Revisionen, §4.6).
    ///
    /// # Errors
    /// Speicherfehler.
    fn replace_outbox(&mut self, op: &Op) -> CoreResult<()>;
    /// Alle nicht gelöschten Ordner mit ihren Registern (Baum neu berechnen).
    ///
    /// # Errors
    /// Speicherfehler.
    fn live_note_folders(&mut self) -> CoreResult<Vec<(NoteFolderId, Registers)>>;
    /// Alle nicht gelöschten Notizen mit ihren Registern (Neuzuordnung nach Ordneränderungen).
    ///
    /// # Errors
    /// Speicherfehler.
    fn live_notes(&mut self) -> CoreResult<Vec<(NoteId, Registers)>>;
    /// Kleinster `sort_key` unter `folder` (Notizen und Ordner, ohne Papierkorb-Filter), der
    /// größer als `after` ist – für „direkt dahinter einfügen".
    ///
    /// # Errors
    /// Speicherfehler.
    fn next_sort_key(
        &mut self,
        folder: Option<NoteFolderId>,
        after: &SortKey,
    ) -> CoreResult<Option<SortKey>>;

    // --- materialisierte Sicht ---

    /// # Errors
    /// Speicherfehler.
    fn task(&mut self, id: TaskId) -> CoreResult<Option<Task>>;
    /// # Errors
    /// Speicherfehler.
    fn list(&mut self, id: TaskListId) -> CoreResult<Option<TaskList>>;
    /// # Errors
    /// Speicherfehler.
    fn note(&mut self, id: NoteId) -> CoreResult<Option<Note>>;
    /// # Errors
    /// Speicherfehler.
    fn note_folder(&mut self, id: NoteFolderId) -> CoreResult<Option<NoteFolder>>;
    /// # Errors
    /// Speicherfehler.
    fn tag(&mut self, id: TagId) -> CoreResult<Option<Tag>>;
    /// # Errors
    /// Speicherfehler.
    fn put_task(&mut self, task: &Task) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn put_list(&mut self, list: &TaskList) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn put_note(&mut self, note: &Note) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn put_note_folder(&mut self, folder: &NoteFolder) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn put_tag(&mut self, tag: &Tag) -> CoreResult<()>;
    /// # Errors
    /// Speicherfehler.
    fn remove(&mut self, entity: EntityRef) -> CoreResult<()>;
}

/// Gemeinsamer Zustand einer Buchung (lokaler Commit oder Pull).
struct Booking {
    clock: OpClock,
    now: Timestamp,
    changes: Vec<Change>,
    conflicts: Vec<NoteConflict>,
}

/// Erzeugt für einen lokalen Commit die Ops (Outbox) und schreibt die Register fort. Liefert
/// die Änderungen, die der Speicher danach auf die Tabellen anwenden soll: Änderungen an
/// gelöschten Entities (Tombstone) entfallen – „Delete gewinnt" gilt auch lokal. Notizen kommen
/// mit aufgelöstem Inhalt zurück, dazu eventuell entstandene Konfliktkopien.
///
/// # Errors
/// Speicherfehler; `Validation`, wenn ein zusammengeführter Inhalt ungültig wäre.
pub fn record_local<S: ReplicaStore + ?Sized>(
    store: &mut S,
    changes: ChangeSet,
) -> CoreResult<Recorded> {
    let now = changes.time().unwrap_or(Timestamp::EPOCH);
    let start = store.op_clock()?;
    let mut booking = Booking {
        clock: start,
        now,
        changes: Vec::new(),
        conflicts: Vec::new(),
    };
    for change in changes {
        record_change(store, &mut booking, change)?;
    }
    if booking.clock != start {
        store.set_op_clock(&booking.clock)?;
    }
    let mut effective = ChangeSet::new().at(now);
    for change in booking.changes {
        effective.push(change);
    }
    Ok(Recorded {
        changes: effective,
        conflicts: booking.conflicts,
    })
}

fn record_change<S: ReplicaStore + ?Sized>(
    store: &mut S,
    booking: &mut Booking,
    change: Change,
) -> CoreResult<()> {
    let entity = change.entity();
    let before = store.entity_state(entity)?;
    if before.as_ref().is_some_and(EntityState::is_deleted) {
        return Ok(());
    }
    let change = match change {
        Change::UpsertNote(note) => Change::UpsertNote(record_body(store, booking, note)?),
        other => other,
    };
    let fields = before.as_ref().and_then(EntityState::fields);
    let mut ops = ops_for(&change, fields.as_ref(), &mut booking.clock, booking.now);
    let mut replaced = Vec::new();
    if matches!(change, Change::UpsertNote(_)) && before.is_some() {
        // Feld-Änderungen einer Notiz (z. B. Auto-Name beim Tippen der ersten Zeile) ersetzen
        // ein noch nicht herausgegebenes `Set` desselben Felds. Die Nummern der neuen Ops sind
        // dann zu viel vergeben: neu stempeln, damit die Folge lückenlos bleibt.
        let mut clock = booking.clock;
        clock.rewind(u64::try_from(ops.len()).unwrap_or(0));
        let mut fresh = Vec::with_capacity(ops.len());
        for op in ops {
            let OpKind::Set { field, value } = op.kind else {
                unreachable!("ops_for liefert für bekannte Entities nur Sets")
            };
            match store.replaceable_op(entity, Some(field))? {
                Some(pending) => replaced.push(Op {
                    id: pending.id,
                    hlc: clock.hlc_mut().tick(booking.now),
                    entity,
                    kind: OpKind::Set { field, value },
                }),
                None => fresh.push(clock.stamp(booking.now, entity, OpKind::Set { field, value })),
            }
        }
        booking.clock = clock;
        ops = fresh;
    }
    let all: Vec<Op> = ops.iter().chain(&replaced).cloned().collect();
    if let Some(state) = merge(before.as_ref(), &all).state {
        if !all.is_empty() {
            store.set_entity_state(entity, before.as_ref(), &state)?;
        }
    }
    for op in &ops {
        store.push_outbox(op)?;
    }
    for op in &replaced {
        store.replace_outbox(op)?;
    }
    if let Change::Delete(EntityRef::Note(id)) = change {
        store.remove_revisions(id, None)?;
    }
    booking.changes.push(change);
    Ok(())
}

/// Schreibt bei geändertem Inhalt eine Revision (bzw. ersetzt die noch nicht hochgeladene
/// eigene, §4.6), legt Köpfe zusammen und liefert die Notiz mit aufgelöstem Inhalt.
fn record_body<S: ReplicaStore + ?Sized>(
    store: &mut S,
    booking: &mut Booking,
    mut note: Note,
) -> CoreResult<Note> {
    let id = note.id();
    let entity = EntityRef::Note(id);
    let mut revs = load_revisions(store, id)?;
    let current = revs.resolve();
    // Unverändert gegenüber der eigenen Basis (z. B. Umbenennen, während eine Remote-Revision
    // eintraf): keine Revision, sonst würde der alte Stand neu geschrieben.
    let base_text = match note.body_rev() {
        Some(base) => match revs.get(base) {
            Some(rev) => Some(rev.text.clone()),
            None => store.revision(id, base)?.map(|rev| rev.text),
        },
        None => None,
    };
    let unedited = base_text.is_some_and(|text| text == note.body());
    if note.body() != current.text && !unedited {
        let parents = revs.parents_for_edit(note.body_rev());
        let replaceable = store.replaceable_op(entity, None)?.filter(|op| {
            parents == [RevisionId::from(op.id)] && current.heads.contains(&op.id.into())
        });
        let op = match replaceable {
            Some(pending) => {
                let OpKind::Revise { parents, .. } = pending.kind else {
                    unreachable!("replaceable_op(None) liefert nur Revise-Ops")
                };
                let op = Op {
                    id: pending.id,
                    hlc: booking.clock.hlc_mut().tick(booking.now),
                    entity,
                    kind: OpKind::Revise {
                        parents,
                        text: note.body().to_owned(),
                    },
                };
                store.replace_outbox(&op)?;
                op
            }
            None => {
                let op = booking.clock.stamp(
                    booking.now,
                    entity,
                    OpKind::Revise {
                        parents: parents.iter().map(|p| (*p).into()).collect(),
                        text: note.body().to_owned(),
                    },
                );
                store.push_outbox(&op)?;
                op
            }
        };
        let rev = revision_of(&op).expect("Revise-Op");
        store.put_revision(id, &rev)?;
        revs.insert(rev);
    }
    let resolution = settle(store, booking, &note, &mut revs)?;
    note.set_body(&resolution.text, resolution.base(), &At(booking.now))?;
    Ok(note)
}

/// Legt Köpfe mit verschiedenem Text zu einer Merge-Revision zusammen (`ResolveNoteHeads`,
/// §4.4) und legt für unterlegene Stände Konfliktkopien an. `note` liefert Name, Ordner, Typ und
/// Position für die Kopie. Liefert die Auflösung danach.
fn settle<S: ReplicaStore + ?Sized>(
    store: &mut S,
    booking: &mut Booking,
    note: &Note,
    revs: &mut RevisionSet,
) -> CoreResult<Resolution> {
    let mut resolution = revs.resolve();
    if !resolution.needs_merge {
        return Ok(resolution);
    }
    // Verschiedene Köpfe: für die Merge-Basis alle Revisionen laden (selten).
    *revs = store.revisions(note.id())?;
    resolution = revs.resolve();
    let op = booking.clock.stamp(
        booking.now,
        EntityRef::Note(note.id()),
        OpKind::Revise {
            parents: resolution.heads.iter().map(|h| (*h).into()).collect(),
            text: resolution.text.clone(),
        },
    );
    store.push_outbox(&op)?;
    let rev = revision_of(&op).expect("Revise-Op");
    store.put_revision(note.id(), &rev)?;
    revs.insert(rev);
    // Mit Auto-Name heißt das Original nach dem aufgelösten Inhalt.
    let name = if note.auto_name() {
        NoteName::from_first_line(&resolution.text)
    } else {
        note.name().clone()
    };
    for loser in &resolution.losers {
        create_conflict_copy(store, booking, note, &name, loser)?;
    }
    Ok(revs.resolve())
}

/// Konfliktkopie direkt hinter dem Original: „<Name> (Konflikt <TT.MM.JJJJ>)", Inhalt = der
/// unterlegene Stand. Die ID ist deterministisch; existiert die Kopie schon (anderes Gerät war
/// schneller) oder wurde sie gelöscht, passiert nichts.
fn create_conflict_copy<S: ReplicaStore + ?Sized>(
    store: &mut S,
    booking: &mut Booking,
    original: &Note,
    name: &NoteName,
    loser: &ConflictLoser,
) -> CoreResult<()> {
    let id = conflict_copy_id(original.id(), loser.rev);
    if store.entity_state(EntityRef::Note(id))?.is_some() {
        return Ok(());
    }
    let next = store.next_sort_key(original.folder(), original.sort_key())?;
    let key = SortKey::between(Some(original.sort_key()), next.as_ref())
        .or_else(|_| SortKey::between(Some(original.sort_key()), None))?;
    let clock = At(booking.now);
    let mut copy = Note::with_id(id, original.folder(), original.kind(), key, &clock);
    copy.rename(&conflict_copy_name(name, loser), &clock)?;
    copy.set_body(&loser.text, None, &clock)?;
    booking.conflicts.push(NoteConflict {
        note: original.id(),
        copy: id,
    });
    record_change(store, booking, Change::UpsertNote(copy))
}

/// „<Name> (Konflikt TT.MM.JJJJ)" mit dem Datum (UTC) des unterlegenen Stands – auf allen Geräten
/// gleich. Lange Namen werden gekürzt.
fn conflict_copy_name(name: &NoteName, loser: &ConflictLoser) -> String {
    let date = chrono::DateTime::from_timestamp_millis(wall(loser.hlc.wall_ms).as_millis())
        .map(|at| at.format("%d.%m.%Y").to_string())
        .unwrap_or_default();
    let suffix = format!(" (Konflikt {date})");
    let keep = NOTE_NAME_MAX_CHARS.saturating_sub(suffix.chars().count());
    let base: String = name.as_str().chars().take(keep).collect();
    format!("{}{suffix}", base.trim_end())
}

/// Revisionen für die Auflösung: nur die Köpfe, solange sie denselben Text haben (ein Kopf ist
/// der Normalfall); sonst alle.
fn load_revisions<S: ReplicaStore + ?Sized>(store: &mut S, id: NoteId) -> CoreResult<RevisionSet> {
    let heads = store.revision_heads(id)?;
    if heads.windows(2).all(|w| w[0].text == w[1].text) {
        Ok(RevisionSet::from_revisions(heads))
    } else {
        store.revisions(id)
    }
}

/// Die `Revise`-Op einer gespeicherten Revision (Snapshot-Export).
#[must_use]
pub fn revise_op(note: NoteId, rev: &Revision) -> Op {
    Op {
        id: rev.id.into(),
        hlc: rev.hlc,
        entity: EntityRef::Note(note),
        kind: OpKind::Revise {
            parents: rev.parents.iter().map(|p| (*p).into()).collect(),
            text: rev.text.clone(),
        },
    }
}

/// Die Revision einer `Revise`-Op.
#[must_use]
pub fn revision_of(op: &Op) -> Option<Revision> {
    match &op.kind {
        OpKind::Revise { parents, text } => Some(Revision {
            id: op.id.into(),
            hlc: op.hlc,
            parents: parents.iter().map(|p| (*p).into()).collect(),
            text: text.clone(),
        }),
        _ => None,
    }
}

/// Mergt empfangene Ops, materialisiert betroffene Tasks, Listen, Ordner und Notizen und setzt
/// die Cursor.
///
/// Tasks, deren Liste (noch) nicht existiert oder gelöscht wurde, landen in der Inbox; ohne
/// Inbox werden sie erst sichtbar, sobald eine der beiden existiert. Notizen und Ordner, deren
/// Ordner fehlt, landen in der Wurzel. Notizen mit mehreren Köpfen werden in derselben
/// Transaktion zusammengelegt (lokale Ops in der Outbox).
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
    for (entity, ops) in &groups {
        let EntityRef::TaskList(id) = *entity else {
            continue;
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
    // Tags (ADR-035): eigenständig; Tasks verweisen nur per ID.
    for (entity, ops) in &groups {
        let EntityRef::Tag(id) = *entity else {
            continue;
        };
        let Some(state) = merged(store, *entity, ops)? else {
            continue;
        };
        let target = match &state {
            EntityState::Live(registers) => materialize_tag(id, registers),
            EntityState::Deleted(_) => None,
        };
        if store.tag(id)? == target {
            continue;
        }
        match &target {
            Some(tag) => store.put_tag(tag)?,
            None => store.remove(*entity)?,
        }
        receipt.changed.push(*entity);
    }
    for (entity, ops) in &groups {
        let EntityRef::Task(id) = *entity else {
            continue;
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

    // Ordner: bei jeder Änderung den ganzen Baum neu berechnen (wenige Einträge).
    let mut folders_touched = false;
    let mut folder_set_changed = false;
    for (entity, ops) in &groups {
        let EntityRef::NoteFolder(id) = *entity else {
            continue;
        };
        if let Some(state) = merged(store, *entity, ops)? {
            folders_touched = true;
            if state.is_deleted() && store.note_folder(id)?.is_some() {
                store.remove(*entity)?;
                receipt.changed.push(*entity);
                folder_set_changed = true;
            }
        }
    }
    if folders_touched {
        folder_set_changed |= rematerialize_folders(store, &mut receipt)?;
    }

    let mut booking = Booking {
        clock,
        now: batch.now,
        changes: Vec::new(),
        conflicts: Vec::new(),
    };
    for (entity, ops) in &groups {
        let EntityRef::Note(id) = *entity else {
            continue;
        };
        let state = merged(store, *entity, ops)?;
        let deleted = store
            .entity_state(*entity)?
            .as_ref()
            .is_some_and(EntityState::is_deleted);
        let mut revs_added = false;
        if !deleted
            && ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Revise { .. }))
        {
            for rev in ops.iter().filter_map(revision_of) {
                let known = store.revision(id, rev.id)?;
                if known.is_none_or(|k| k.hlc < rev.hlc) {
                    store.put_revision(id, &rev)?;
                    revs_added = true;
                }
            }
        }
        if (state.is_some() || revs_added) && rematerialize_note(store, &mut booking, id)? {
            receipt.changed.push(*entity);
        }
    }
    if folder_set_changed {
        for (id, registers) in store.live_notes()? {
            let entity = EntityRef::Note(id);
            if groups.contains_key(&entity) {
                continue;
            }
            if let Some(mut note) = store.note(id)? {
                let target = placed_folder(store, folder(&registers).flatten())?;
                if note.folder() != target {
                    let key = note.sort_key().clone();
                    note.move_to(target, key);
                    store.put_note(&note)?;
                    receipt.changed.push(entity);
                }
            }
        }
    }
    // Beim Zusammenlegen entstandene Kopien sind lokale Änderungen: materialisieren.
    for change in std::mem::take(&mut booking.changes) {
        if let Change::UpsertNote(note) = change {
            store.put_note(&note)?;
            receipt.changed.push(EntityRef::Note(note.id()));
        }
    }
    receipt.conflicts = booking.conflicts;

    for (device, seq) in &batch.cursors {
        store.set_cursor(*device, *seq)?;
    }
    store.set_op_clock(&booking.clock)?;
    Ok(receipt)
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
    let at = At(task.updated_at());
    task.move_to(TaskListId::INBOX, key, &at);
    Ok(Some(task))
}

/// Berechnet den Ordnerbaum neu (Waisen → Wurzel, Zyklen brechen) und schreibt geänderte
/// Ordner. `true`, wenn Ordner dazukamen (dann müssen Notizen neu zugeordnet werden).
fn rematerialize_folders<S: ReplicaStore + ?Sized>(
    store: &mut S,
    receipt: &mut RemoteReceipt,
) -> CoreResult<bool> {
    let live: Vec<(NoteFolderId, Registers)> = store
        .live_note_folders()?
        .into_iter()
        .filter(|(id, registers)| materialize_note_folder(*id, registers, None).is_some())
        .collect();
    let declared: BTreeMap<NoteFolderId, (Option<NoteFolderId>, super::Hlc)> = live
        .iter()
        .map(|(id, registers)| {
            let parent = &registers[&FieldName::ParentFolder];
            (*id, (folder(registers).flatten(), parent.hlc))
        })
        .collect();
    let parents = note_tree::resolve_parents(&declared);
    let mut set_changed = false;
    for (id, registers) in &live {
        let target = materialize_note_folder(*id, registers, parents[id]);
        let current = store.note_folder(*id)?;
        if current == target {
            continue;
        }
        set_changed |= current.is_none();
        if let Some(folder) = &target {
            store.put_note_folder(folder)?;
        }
        let entity = EntityRef::NoteFolder(*id);
        if !receipt.changed.contains(&entity) {
            receipt.changed.push(entity);
        }
    }
    Ok(set_changed)
}

/// Wirksamer Ordner: der gewünschte, wenn er existiert, sonst die Wurzel („Notiz retten").
fn placed_folder<S: ReplicaStore + ?Sized>(
    store: &mut S,
    wanted: Option<NoteFolderId>,
) -> CoreResult<Option<NoteFolderId>> {
    match wanted {
        Some(id) if store.note_folder(id)?.is_some() => Ok(Some(id)),
        _ => Ok(None),
    }
}

/// Bringt die Tabellenzeile einer Notiz auf den Stand von Registern und Revisionen (legt
/// mehrere Köpfe dabei zusammen). `true` bei Änderung.
fn rematerialize_note<S: ReplicaStore + ?Sized>(
    store: &mut S,
    booking: &mut Booking,
    id: NoteId,
) -> CoreResult<bool> {
    let entity = EntityRef::Note(id);
    let state = store.entity_state(entity)?;
    let target = match &state {
        Some(EntityState::Live(registers)) => match materialize_note(id, registers) {
            Some(mut parts) => {
                parts.folder = placed_folder(store, parts.folder)?;
                let mut revs = load_revisions(store, id)?;
                let skeleton = Note::from_parts(parts.clone())?;
                let resolution = settle(store, booking, &skeleton, &mut revs)?;
                let newest_head = resolution
                    .heads
                    .iter()
                    .filter_map(|h| revs.get(*h))
                    .map(|r| wall(r.hlc.wall_ms))
                    .max();
                parts.modified_at = parts
                    .modified_at
                    .max(newest_head.unwrap_or(Timestamp::EPOCH));
                if parts.auto_name {
                    parts.name = NoteName::from_first_line(&resolution.text).to_string();
                }
                parts.body = resolution.text.clone();
                parts.body_rev = resolution.base();
                Some(Note::from_parts(parts)?)
            }
            None => None,
        },
        Some(EntityState::Deleted(_)) => {
            store.remove_revisions(id, None)?;
            None
        }
        None => None,
    };
    let current = store.note(id)?;
    if current == target {
        return Ok(false);
    }
    match &target {
        Some(note) => store.put_note(note)?,
        None => store.remove(entity)?,
    }
    Ok(true)
}

/// Entfernt Revisionen, die kein aktives Gerät mehr als Basis braucht (§4.5). `acked` = je
/// Gerät die höchste Op-Nummer, die **alle** aktiven Geräte gesehen haben. Liefert die Zahl
/// entfernter Revisionen.
///
/// # Errors
/// Speicherfehler.
pub fn prune_revisions<S: ReplicaStore + ?Sized>(
    store: &mut S,
    acked: &BTreeMap<DeviceId, u64>,
) -> CoreResult<u32> {
    let mut removed = 0u32;
    for note in store.notes_with_revisions()? {
        let revs = store.revisions(note)?;
        let prunable = revs.prunable(|id| acked.get(&id.device).is_some_and(|seq| id.seq <= *seq));
        if !prunable.is_empty() {
            removed += u32::try_from(prunable.len()).unwrap_or(u32::MAX);
            store.remove_revisions(note, Some(&prunable))?;
        }
    }
    Ok(removed)
}

/// Uhr mit festem Zeitpunkt (z. B. `updated_at` beim Umhängen unverändert lassen).
struct At(Timestamp);

impl Clock for At {
    fn now(&self) -> Timestamp {
        self.0
    }
}
