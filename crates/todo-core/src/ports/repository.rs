//! `TaskRepository`: lokaler Speicher für Tasks und Listen (siehe `development/interfaces.md` §2).
//!
//! Neben dem Zustand führt jedes Repository die Sync-Buchhaltung (Outbox, Register, HLC, Cursor)
//! in denselben Transaktionen – über [`crate::sync::record_local`] und
//! [`crate::sync::apply_remote`] (ADR-015). Volltextsuche (`search`) ist nicht Teil des
//! Prototyps und fehlt deshalb bewusst.

use serde::{Deserialize, Serialize};

use super::{MaybeSend, MaybeSync};
use crate::{
    domain::{
        DeviceId, Due, LocalDay, Priority, SortKey, TagId, Task, TaskId, TaskList, TaskListId,
        TaskStatus, Timestamp,
    },
    sync::{EntityState, Op, RemoteBatch, RemoteReceipt},
    CoreResult,
};

/// Größte erlaubte Seitengröße für [`PageRequest::limit`].
pub const PAGE_LIMIT_MAX: u32 = 500;

/// Verweis auf eine gespeicherte Entity. Teil des Op-Formats: Varianten nur anhängen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntityRef {
    Task(TaskId),
    TaskList(TaskListId),
}

/// Eine einzelne Domain-Änderung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Task anlegen oder vollständig ersetzen.
    UpsertTask(Task),
    /// Liste anlegen oder vollständig ersetzen.
    UpsertList(TaskList),
    /// Entity entfernen. Unbekannte IDs sind kein Fehler (idempotent, wichtig für den Sync).
    Delete(EntityRef),
}

impl Change {
    /// Die betroffene Entity.
    #[must_use]
    pub fn entity(&self) -> EntityRef {
        match self {
            Self::UpsertTask(task) => EntityRef::Task(task.id()),
            Self::UpsertList(list) => EntityRef::TaskList(list.id()),
            Self::Delete(entity) => *entity,
        }
    }
}

/// Zusammengehörige Änderungen, die [`TaskRepository::commit`] atomar anwendet – in der
/// angegebenen Reihenfolge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    changes: Vec<Change>,
    at: Option<Timestamp>,
}

impl ChangeSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn upsert_task(mut self, task: Task) -> Self {
        self.changes.push(Change::UpsertTask(task));
        self
    }

    #[must_use]
    pub fn upsert_list(mut self, list: TaskList) -> Self {
        self.changes.push(Change::UpsertList(list));
        self
    }

    #[must_use]
    pub fn delete(mut self, entity: EntityRef) -> Self {
        self.changes.push(Change::Delete(entity));
        self
    }

    pub fn push(&mut self, change: Change) {
        self.changes.push(change);
    }

    /// Zeitpunkt der Änderung (Basis für die HLC der erzeugten Ops). Setzt die `CoreApi`
    /// automatisch; ohne Angabe nimmt das Repository das späteste `updated_at` der Entities.
    #[must_use]
    pub fn at(mut self, now: Timestamp) -> Self {
        self.at = Some(now);
        self
    }

    /// Siehe [`ChangeSet::at`].
    #[must_use]
    pub fn time(&self) -> Option<Timestamp> {
        self.at.or_else(|| {
            self.changes
                .iter()
                .filter_map(|change| match change {
                    Change::UpsertTask(task) => Some(task.updated_at()),
                    Change::UpsertList(list) => Some(list.updated_at()),
                    Change::Delete(_) => None,
                })
                .max()
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    #[must_use]
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// Betroffene Entities ohne Duplikate, in Reihenfolge des ersten Auftretens.
    #[must_use]
    pub fn entities(&self) -> Vec<EntityRef> {
        let mut seen = Vec::with_capacity(self.changes.len());
        for entity in self.changes.iter().map(Change::entity) {
            if !seen.contains(&entity) {
                seen.push(entity);
            }
        }
        seen
    }
}

impl IntoIterator for ChangeSet {
    type Item = Change;
    type IntoIter = std::vec::IntoIter<Change>;

    fn into_iter(self) -> Self::IntoIter {
        self.changes.into_iter()
    }
}

/// Ergebnis eines erfolgreichen [`TaskRepository::commit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Geänderte Entities (Grundlage für `CoreEvent::Changed`).
    pub changed: Vec<EntityRef>,
}

/// Vordefinierte Ansichten. Bestimmen, *welche* Tasks in Frage kommen und wie sortiert wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartView {
    /// Tasks der Inbox-Liste.
    Inbox,
    /// Tasks einer bestimmten Liste.
    List(TaskListId),
    /// Tasks aller Listen mit Fälligkeit an diesem Tag oder früher (inkl. überfällig).
    Today(LocalDay),
    /// Tasks aller Listen mit Fälligkeit nach diesem Tag.
    Upcoming(LocalDay),
    /// Tasks aller Listen.
    All,
    /// Erledigte Tasks aller Listen, neueste zuerst.
    Completed,
}

/// Abfrage für [`TaskRepository::list_tasks`]: eine Ansicht plus optionale UND-Filter.
///
/// Sortierung: [`SmartView::Completed`] nach `completed_at` absteigend, alle anderen nach
/// `sort_key` aufsteigend; bei Gleichstand jeweils nach `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskQuery {
    pub view: SmartView,
    /// Nur Tasks mit diesem Status; `None` = offene und erledigte.
    /// [`SmartView::Completed`] liefert unabhängig davon nur erledigte Tasks.
    pub status: Option<TaskStatus>,
    pub tag: Option<TagId>,
}

impl TaskQuery {
    /// Ansicht ohne weitere Filter.
    #[must_use]
    pub fn view(view: SmartView) -> Self {
        Self {
            view,
            status: None,
            tag: None,
        }
    }

    /// Nur offene Tasks dieser Ansicht.
    #[must_use]
    pub fn open(view: SmartView) -> Self {
        Self {
            view,
            status: Some(TaskStatus::Open),
            tag: None,
        }
    }

    /// Prüft, ob `task` (aus der Liste `list`) zur Abfrage gehört – unabhängig von Sortierung
    /// und Paginierung. Repository-Implementierungen ohne eigene Abfragesprache nutzen das direkt.
    #[must_use]
    pub fn matches(&self, task: &Task, list: &TaskList) -> bool {
        let in_view = match self.view {
            SmartView::Inbox => list.is_inbox(),
            SmartView::List(id) => task.list_id() == id,
            SmartView::Today(day) => task.due().is_some_and(|d| d.is_due_by(&day)),
            SmartView::Upcoming(day) => task.due().is_some_and(|d| !d.is_due_by(&day)),
            SmartView::All => true,
            SmartView::Completed => task.is_done(),
        };
        in_view
            && self.status.is_none_or(|s| task.status() == s)
            && self.tag.is_none_or(|t| task.tags().contains(&t))
    }
}

/// Seitenanfrage. `cursor` stammt aus [`Page::next_cursor`] einer vorherigen Antwort und ist für
/// Aufrufer opak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRequest {
    pub cursor: Option<String>,
    /// 1 bis [`PAGE_LIMIT_MAX`].
    pub limit: u32,
}

impl PageRequest {
    #[must_use]
    pub fn first(limit: u32) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }

    #[must_use]
    pub fn after(cursor: String, limit: u32) -> Self {
        Self {
            cursor: Some(cursor),
            limit,
        }
    }
}

/// Eine Seite von Ergebnissen. `next_cursor` ist `None`, wenn es keine weiteren gibt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Schlanke Projektion einer Task für Listenansichten (ohne Notiztext).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    pub id: TaskId,
    pub list_id: TaskListId,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub due: Option<Due>,
    pub has_notes: bool,
    pub sort_key: SortKey,
    pub completed_at: Option<Timestamp>,
}

impl From<&Task> for TaskSummary {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id(),
            list_id: task.list_id(),
            title: task.title().to_owned(),
            status: task.status(),
            priority: task.priority(),
            due: task.due().cloned(),
            has_notes: !task.notes().is_empty(),
            sort_key: task.sort_key().clone(),
            completed_at: task.completed_at(),
        }
    }
}

/// Lokaler Speicher. Implementierungen: `SqliteTaskRepository` (nativ), später ein Web-Speicher,
/// `InMemoryTaskRepository` (Tests).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait TaskRepository: MaybeSend + MaybeSync {
    /// Lädt eine Task; `None`, wenn sie nicht existiert.
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>>;

    /// Lädt eine Liste; `None`, wenn sie nicht existiert.
    async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>>;

    /// Alle Listen, sortiert nach `sort_key` (dann `id`). Wenige Einträge, daher ohne Paginierung.
    async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>>;

    /// Eine Seite Tasks gemäß `query` (Sortierung siehe [`TaskQuery`]).
    ///
    /// # Errors
    /// `CoreError::Validation` bei `limit` außerhalb `1..=PAGE_LIMIT_MAX` oder ungültigem Cursor.
    async fn list_tasks(
        &self,
        query: &TaskQuery,
        page: PageRequest,
    ) -> CoreResult<Page<TaskSummary>>;

    /// Wendet alle Änderungen atomar an (alles oder nichts), in der angegebenen Reihenfolge.
    /// Dieselbe Transaktion schreibt die zugehörigen Ops in die Outbox und schreibt Register und
    /// HLC fort ([`crate::sync::record_local`]). Änderungen an gelöschten Entities (Tombstone)
    /// entfallen.
    ///
    /// # Errors
    /// `CoreError::Validation`, wenn nach Anwendung eine Task auf eine nicht existierende Liste
    /// verweist; `CoreError::Storage` bei Speicherfehlern. In beiden Fällen bleibt der Bestand
    /// unverändert – auch Outbox und Register.
    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt>;

    // --- Sync ----------------------------------------------------------------------------------

    /// ID dieses Geräts; beim ersten Öffnen des Speichers erzeugt und danach unverändert.
    async fn device_id(&self) -> CoreResult<DeviceId>;

    /// Noch nicht hochgeladene lokale Ops, aufsteigend nach Nummer, höchstens `limit`.
    async fn pending_ops(&self, limit: u32) -> CoreResult<Vec<Op>>;

    /// Anzahl der Ops in der Outbox (für den Sync-Status).
    async fn pending_op_count(&self) -> CoreResult<u64>;

    /// Entfernt alle Ops mit Nummer `<= up_to` aus der Outbox. Idempotent.
    async fn mark_ops_pushed(&self, up_to: u64) -> CoreResult<()>;

    /// Wendet empfangene Ops an (Merge, Materialisierung) und setzt die Cursor des Batches –
    /// atomar und idempotent ([`crate::sync::apply_remote`]).
    async fn apply_remote(&self, batch: RemoteBatch) -> CoreResult<RemoteReceipt>;

    /// Nummer der letzten verarbeiteten Op von `device`; `None` = noch nichts empfangen.
    async fn sync_cursor(&self, device: DeviceId) -> CoreResult<Option<u64>>;

    async fn set_sync_cursor(&self, device: DeviceId, seq: u64) -> CoreResult<()>;

    /// Alle Cursor, nach Gerät sortiert (für `devices/<id>.json` und die Kompaktierung).
    async fn sync_cursors(&self) -> CoreResult<Vec<(DeviceId, u64)>>;

    /// Sync-Zustand einer Entity: Register (Wert + HLC je Feld, „Field-Clocks") oder Tombstone.
    async fn entity_state(&self, entity: EntityRef) -> CoreResult<Option<EntityState>>;

    /// Kleine Schlüssel/Wert-Ablage der Sync-Engine (z. B. Feed-Cursor, Snapshot-Stand).
    async fn sync_value(&self, key: &str) -> CoreResult<Option<String>>;

    async fn set_sync_value(&self, key: &str, value: String) -> CoreResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::NaiveDate,
        ports::{FixedClock, SteppingClock},
    };

    fn clock() -> SteppingClock {
        SteppingClock::new(Timestamp::from_millis(1_000), 1)
    }

    #[test]
    fn change_set_keeps_order_and_dedups_entities() {
        let c = clock();
        let list = TaskList::new("Privat", SortKey::first(), &c).unwrap();
        let task = Task::new(list.id(), "Milch", SortKey::first(), &c).unwrap();
        let set = ChangeSet::new()
            .upsert_list(list.clone())
            .upsert_task(task.clone())
            .upsert_task(task.clone())
            .delete(EntityRef::Task(task.id()));
        assert_eq!(set.len(), 4);
        assert_eq!(
            set.entities(),
            [EntityRef::TaskList(list.id()), EntityRef::Task(task.id())]
        );
        assert!(matches!(set.changes()[0], Change::UpsertList(_)));
        assert!(!set.is_empty());
        assert!(ChangeSet::new().is_empty());
    }

    #[test]
    fn summary_projects_task() {
        let c = FixedClock(Timestamp::from_millis(5));
        let mut task = Task::new(TaskListId::new(), "Milch", SortKey::first(), &c).unwrap();
        task.set_notes("bio", &c);
        task.complete(&c);
        let summary = TaskSummary::from(&task);
        assert_eq!(summary.title, "Milch");
        assert!(summary.has_notes);
        assert_eq!(summary.status, TaskStatus::Done);
        assert_eq!(summary.completed_at, Some(Timestamp::from_millis(5)));
    }

    #[test]
    fn query_matches_views_and_filters() {
        let c = clock();
        let inbox = TaskList::inbox(SortKey::first(), &c);
        let work = TaskList::new("Arbeit", SortKey::first(), &c).unwrap();
        let date = |d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        let today = LocalDay::new(
            date(17),
            Timestamp::from_millis(0),
            Timestamp::from_millis(10),
        )
        .unwrap();

        let mut due_today = Task::new(inbox.id(), "heute", SortKey::first(), &c).unwrap();
        due_today.set_due(Some(Due::Date(date(17))), &c).unwrap();
        let mut due_later = Task::new(work.id(), "später", SortKey::first(), &c).unwrap();
        due_later.set_due(Some(Due::Date(date(20))), &c).unwrap();
        let mut done = Task::new(work.id(), "fertig", SortKey::first(), &c).unwrap();
        done.complete(&c);

        let q = TaskQuery::view;
        assert!(q(SmartView::Inbox).matches(&due_today, &inbox));
        assert!(!q(SmartView::Inbox).matches(&due_later, &work));
        assert!(q(SmartView::List(work.id())).matches(&done, &work));
        assert!(q(SmartView::Today(today)).matches(&due_today, &inbox));
        assert!(!q(SmartView::Today(today)).matches(&due_later, &work));
        assert!(
            !q(SmartView::Today(today)).matches(&done, &work),
            "ohne Fälligkeit"
        );
        assert!(q(SmartView::Upcoming(today)).matches(&due_later, &work));
        assert!(!q(SmartView::Upcoming(today)).matches(&due_today, &inbox));
        assert!(q(SmartView::All).matches(&done, &work));
        assert!(q(SmartView::Completed).matches(&done, &work));
        assert!(!q(SmartView::Completed).matches(&due_today, &inbox));

        assert!(!TaskQuery::open(SmartView::All).matches(&done, &work));
        assert!(TaskQuery::open(SmartView::All).matches(&due_later, &work));
        let tagged = TaskQuery {
            tag: Some(TagId::new()),
            ..q(SmartView::All)
        };
        assert!(!tagged.matches(&due_later, &work));
    }
}
