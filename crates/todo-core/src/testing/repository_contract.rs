//! Wiederverwendbare Contract-Tests für jede [`TaskRepository`]-Implementierung.
//!
//! Verwendung in einem Adapter-Crate (dev-dependency auf `todo-core` mit Feature `test-support`):
//!
//! ```ignore
//! mod contract {
//!     todo_core::repository_contract_tests!(my_crate::MyRepository::open_in_memory().unwrap());
//! }
//! ```
//!
//! Das Makro erzeugt je Fall einen `#[test]`; der Ausdruck wird pro Fall neu ausgewertet, jeder
//! Fall bekommt also ein frisches, leeres Repository. Alternativ [`run_all`].

use futures::executor::block_on;

pub use super::{note_contract::*, note_sync_contract::*, sync_contract::*};

use crate::{
    domain::{Due, LocalDay, NaiveDate, SortKey, Task, TaskId, TaskList, TaskListId, Timestamp},
    ports::{
        ChangeSet, EntityRef, Page, PageRequest, SearchQuery, SmartView, SteppingClock, TaskQuery,
        TaskRepository, TaskSummary, PAGE_LIMIT_MAX, SEARCH_TEXT_MAX_CHARS,
    },
    CoreError,
};

/// Erzeugt je Contract-Fall einen `#[test]`, der `$make` als frisches Repository nutzt.
#[macro_export]
macro_rules! repository_contract_tests {
    ($make:expr) => {
        $crate::repository_contract_tests!(@cases $make;
            upsert_and_get,
            upsert_replaces_entity,
            get_unknown_returns_none,
            get_tasks_in_order_skipping_unknown,
            delete_task,
            delete_unknown_is_noop,
            commit_is_atomic,
            delete_list_with_tasks_is_rejected,
            commit_receipt_lists_changed_entities,
            lists_sorted_by_sort_key,
            empty_page,
            page_of_exactly_limit,
            page_of_limit_plus_one,
            paging_visits_all_in_sort_order,
            equal_sort_keys_ordered_by_id,
            last_sort_key_of_list,
            rejects_invalid_limit_and_cursor,
            filter_inbox_and_list,
            filter_today_and_upcoming,
            due_views_sorted_by_due,
            search_matches_words_in_title_or_notes,
            search_ranks_open_and_title_hits_first,
            search_rejects_invalid_input,
            filter_completed_newest_first,
            filter_status_and_tag,
            trash_view_hides_trashed_everywhere_else,
            summary_matches_task,
            sync_device_id_is_stable,
            commit_writes_ops_to_outbox,
            update_writes_only_changed_fields,
            delete_writes_tombstone_and_op,
            failed_commit_writes_neither_state_nor_ops,
            mark_ops_pushed_removes_prefix,
            entity_state_tracks_field_clocks,
            apply_remote_creates_entities_and_sets_cursor,
            apply_remote_is_idempotent,
            apply_remote_ignores_implausible_future_ops,
            apply_remote_merges_field_wise_with_local,
            apply_remote_delete_wins,
            apply_remote_task_without_list_goes_to_inbox,
            apply_remote_incomplete_entity_is_not_materialized,
            apply_remote_advances_local_hlc,
            inboxes_of_two_devices_merge,
            sync_cursors_and_values_roundtrip,
            entity_states_export_everything,
            notes_and_folders_roundtrip,
            folder_items_are_mixed_paged_and_filter_trash,
            search_notes_by_name_and_body,
            search_follows_edits_and_deletes,
        );
        $crate::repository_contract_tests!(@pairs $make;
            note_body_edits_coalesce_until_frozen,
            concurrent_note_edits_merge_cleanly,
            conflicting_note_edits_create_one_copy,
            purged_folder_rescues_offline_note,
            folder_cycle_resolves_identically,
            trash_and_edit_both_survive,
            purge_wins_over_offline_edit,
            acked_revisions_are_pruned,
            note_state_survives_snapshot_roundtrip,
        );
    };
    (@pairs $make:expr; $($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                $crate::testing::repository_contract::$case(|| $make);
            }
        )*
    };
    (@cases $make:expr; $($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                $crate::testing::repository_contract::$case($make);
            }
        )*
    };
}

/// Führt alle Fälle nacheinander aus; `make` liefert für jeden Fall ein frisches Repository.
pub fn run_all<R: TaskRepository>(make: impl Fn() -> R) {
    upsert_and_get(make());
    upsert_replaces_entity(make());
    get_unknown_returns_none(make());
    get_tasks_in_order_skipping_unknown(make());
    delete_task(make());
    delete_unknown_is_noop(make());
    commit_is_atomic(make());
    delete_list_with_tasks_is_rejected(make());
    commit_receipt_lists_changed_entities(make());
    lists_sorted_by_sort_key(make());
    empty_page(make());
    page_of_exactly_limit(make());
    page_of_limit_plus_one(make());
    paging_visits_all_in_sort_order(make());
    equal_sort_keys_ordered_by_id(make());
    last_sort_key_of_list(make());
    rejects_invalid_limit_and_cursor(make());
    filter_inbox_and_list(make());
    filter_today_and_upcoming(make());
    due_views_sorted_by_due(make());
    search_matches_words_in_title_or_notes(make());
    search_ranks_open_and_title_hits_first(make());
    search_rejects_invalid_input(make());
    filter_completed_newest_first(make());
    filter_status_and_tag(make());
    trash_view_hides_trashed_everywhere_else(make());
    summary_matches_task(make());
    sync_device_id_is_stable(make());
    commit_writes_ops_to_outbox(make());
    update_writes_only_changed_fields(make());
    delete_writes_tombstone_and_op(make());
    failed_commit_writes_neither_state_nor_ops(make());
    mark_ops_pushed_removes_prefix(make());
    entity_state_tracks_field_clocks(make());
    apply_remote_creates_entities_and_sets_cursor(make());
    apply_remote_is_idempotent(make());
    apply_remote_ignores_implausible_future_ops(make());
    apply_remote_merges_field_wise_with_local(make());
    apply_remote_delete_wins(make());
    apply_remote_task_without_list_goes_to_inbox(make());
    apply_remote_incomplete_entity_is_not_materialized(make());
    apply_remote_advances_local_hlc(make());
    inboxes_of_two_devices_merge(make());
    sync_cursors_and_values_roundtrip(make());
    entity_states_export_everything(make());
    notes_and_folders_roundtrip(make());
    folder_items_are_mixed_paged_and_filter_trash(make());
    search_notes_by_name_and_body(make());
    search_follows_edits_and_deletes(make());
    note_body_edits_coalesce_until_frozen(&make);
    concurrent_note_edits_merge_cleanly(&make);
    conflicting_note_edits_create_one_copy(&make);
    purged_folder_rescues_offline_note(&make);
    folder_cycle_resolves_identically(&make);
    trash_and_edit_both_survive(&make);
    purge_wins_over_offline_edit(&make);
    acked_revisions_are_pruned(&make);
    note_state_survives_snapshot_roundtrip(&make);
}

// --- Hilfen ------------------------------------------------------------------------------------

pub(super) struct Fixture<R> {
    pub(super) repo: R,
    pub(super) clock: SteppingClock,
    pub(super) inbox: TaskList,
    pub(super) work: TaskList,
}

pub(super) fn setup<R: TaskRepository>(repo: R) -> Fixture<R> {
    let clock = SteppingClock::new(Timestamp::from_millis(1_700_000_000_000), 1);
    let inbox = TaskList::inbox(SortKey::first(), &clock);
    let work = TaskList::new("Arbeit", after(&SortKey::first()), &clock).expect("list");
    block_on(
        repo.commit(
            ChangeSet::new()
                .upsert_list(inbox.clone())
                .upsert_list(work.clone()),
        ),
    )
    .expect("setup commit");
    Fixture {
        repo,
        clock,
        inbox,
        work,
    }
}

fn after(key: &SortKey) -> SortKey {
    SortKey::between(Some(key), None).expect("sort key")
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).expect("date")
}

impl<R: TaskRepository> Fixture<R> {
    pub(super) fn task(&self, list: TaskListId, title: &str, key: SortKey) -> Task {
        Task::new(list, title, key, &self.clock).expect("task")
    }

    /// `n` Tasks mit aufsteigenden Sort-Keys, in dieser Reihenfolge gespeichert und zurückgegeben.
    pub(super) fn add_tasks(&self, list: TaskListId, n: usize) -> Vec<Task> {
        let mut key = SortKey::first();
        let mut tasks = Vec::with_capacity(n);
        for i in 0..n {
            tasks.push(self.task(list, &format!("Task {i}"), key.clone()));
            key = after(&key);
        }
        self.save(&tasks);
        tasks
    }

    pub(super) fn save(&self, tasks: &[Task]) {
        let set = tasks
            .iter()
            .cloned()
            .fold(ChangeSet::new(), ChangeSet::upsert_task);
        block_on(self.repo.commit(set)).expect("commit");
    }

    fn page(
        &self,
        query: TaskQuery,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<Page<TaskSummary>, CoreError> {
        block_on(self.repo.list_tasks(&query, PageRequest { cursor, limit }))
    }

    fn ids(&self, query: TaskQuery, limit: u32) -> Vec<TaskId> {
        let mut ids = Vec::new();
        let mut cursor = None;
        loop {
            let page = self.page(query, cursor, limit).expect("page");
            assert!(page.items.len() <= limit as usize, "Seite größer als limit");
            ids.extend(page.items.iter().map(|s| s.id));
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return ids,
            }
        }
    }

    pub(super) fn get(&self, id: TaskId) -> Option<Task> {
        block_on(self.repo.get_task(id)).expect("get_task")
    }
}

fn all() -> TaskQuery {
    TaskQuery::view(SmartView::All)
}

fn ids_of(tasks: &[Task]) -> Vec<TaskId> {
    tasks.iter().map(Task::id).collect()
}

// --- Fälle -------------------------------------------------------------------------------------

/// Gespeicherte Tasks und Listen kommen unverändert zurück.
pub fn upsert_and_get<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.task(f.work.id(), "Bericht", SortKey::first());
    task.set_notes("Kapitel 3", &f.clock);
    task.set_due(
        Some(Due::DateTime {
            at: Timestamp::from_millis(5),
            tz: "Europe/Berlin".into(),
        }),
        &f.clock,
    )
    .expect("due");
    f.save(std::slice::from_ref(&task));
    assert_eq!(f.get(task.id()), Some(task));
    assert_eq!(
        block_on(f.repo.get_task_list(f.work.id())).expect("get list"),
        Some(f.work.clone())
    );
}

/// Upsert ersetzt die Entity vollständig.
pub fn upsert_replaces_entity<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.add_tasks(f.inbox.id(), 1).remove(0);
    task.rename("neu", &f.clock).expect("rename");
    task.complete(&f.clock);
    f.save(std::slice::from_ref(&task));
    assert_eq!(f.get(task.id()), Some(task));

    let mut list = f.work.clone();
    list.rename("Büro", &f.clock).expect("rename list");
    block_on(f.repo.commit(ChangeSet::new().upsert_list(list.clone()))).expect("commit");
    assert_eq!(
        block_on(f.repo.get_task_list(list.id())).expect("get"),
        Some(list)
    );
}

pub fn get_unknown_returns_none<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    assert_eq!(f.get(TaskId::new()), None);
    assert_eq!(
        block_on(f.repo.get_task_list(TaskListId::new())).expect("get"),
        None
    );
}

pub fn get_tasks_in_order_skipping_unknown<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let tasks = f.add_tasks(f.inbox.id(), 3);
    let ids = [tasks[2].id(), TaskId::new(), tasks[0].id()];
    assert_eq!(
        block_on(f.repo.get_tasks(&ids)).expect("get_tasks"),
        [tasks[2].clone(), tasks[0].clone()]
    );
    assert!(block_on(f.repo.get_tasks(&[])).expect("empty").is_empty());
}

pub fn delete_task<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let tasks = f.add_tasks(f.inbox.id(), 2);
    block_on(
        f.repo
            .commit(ChangeSet::new().delete(EntityRef::Task(tasks[0].id()))),
    )
    .expect("delete");
    assert_eq!(f.get(tasks[0].id()), None);
    assert!(f.get(tasks[1].id()).is_some());
    assert_eq!(f.ids(all(), 10), [tasks[1].id()]);
}

/// Löschen unbekannter oder bereits gelöschter Entities ist kein Fehler.
pub fn delete_unknown_is_noop<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let task = f.add_tasks(f.inbox.id(), 1).remove(0);
    let delete = || ChangeSet::new().delete(EntityRef::Task(task.id()));
    block_on(f.repo.commit(delete())).expect("first delete");
    block_on(f.repo.commit(delete())).expect("second delete");
    block_on(
        f.repo
            .commit(ChangeSet::new().delete(EntityRef::TaskList(TaskListId::new()))),
    )
    .expect("unknown list");
}

/// Verletzt ein Commit die Integrität, wird nichts davon übernommen.
pub fn commit_is_atomic<R: TaskRepository>(repo: R) {
    let f = setup(repo);
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
    assert_eq!(f.get(ok.id()), None, "Teil-Commit übernommen");
}

/// Eine Liste mit Tasks kann nicht gelöscht werden (Use-Case verschiebt die Tasks vorher).
pub fn delete_list_with_tasks_is_rejected<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let task = f.add_tasks(f.work.id(), 1).remove(0);
    let result = block_on(
        f.repo
            .commit(ChangeSet::new().delete(EntityRef::TaskList(f.work.id()))),
    );
    assert!(
        matches!(result, Err(CoreError::Validation(_))),
        "{result:?}"
    );
    assert!(block_on(f.repo.get_task_list(f.work.id()))
        .expect("get")
        .is_some());

    // Verschieben + Löschen in einem Commit geht.
    let mut moved = task.clone();
    moved.move_to(f.inbox.id(), SortKey::first(), &f.clock);
    block_on(
        f.repo.commit(
            ChangeSet::new()
                .upsert_task(moved)
                .delete(EntityRef::TaskList(f.work.id())),
        ),
    )
    .expect("move + delete");
    assert_eq!(
        block_on(f.repo.get_task_list(f.work.id())).expect("get"),
        None
    );
    assert_eq!(f.get(task.id()).expect("task").list_id(), f.inbox.id());
}

pub fn commit_receipt_lists_changed_entities<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let task = f.task(f.inbox.id(), "x", SortKey::first());
    let receipt = block_on(
        f.repo.commit(
            ChangeSet::new()
                .upsert_task(task.clone())
                .upsert_task(task.clone())
                .upsert_list(f.work.clone()),
        ),
    )
    .expect("commit");
    assert_eq!(
        receipt.changed,
        [EntityRef::Task(task.id()), EntityRef::TaskList(f.work.id())]
    );
}

pub fn lists_sorted_by_sort_key<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let before = SortKey::between(None, Some(&SortKey::first())).expect("key");
    let first = TaskList::new("Zuerst", before, &f.clock).expect("list");
    block_on(f.repo.commit(ChangeSet::new().upsert_list(first))).expect("commit");
    let names: Vec<String> = block_on(f.repo.list_task_lists())
        .expect("lists")
        .iter()
        .map(|l| l.name().to_owned())
        .collect();
    assert_eq!(names, ["Zuerst", "Inbox", "Arbeit"]);
}

pub fn empty_page<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let page = f.page(all(), None, 10).expect("page");
    assert!(page.items.is_empty());
    assert_eq!(page.next_cursor, None);
}

pub fn page_of_exactly_limit<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    f.add_tasks(f.inbox.id(), 3);
    let page = f.page(all(), None, 3).expect("page");
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.next_cursor, None);
}

pub fn page_of_limit_plus_one<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let tasks = f.add_tasks(f.inbox.id(), 4);
    let first = f.page(all(), None, 3).expect("page 1");
    assert_eq!(
        first.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        ids_of(&tasks[..3])
    );
    let cursor = first.next_cursor.expect("next cursor");
    let second = f.page(all(), Some(cursor), 3).expect("page 2");
    assert_eq!(
        second.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [tasks[3].id()]
    );
    assert_eq!(second.next_cursor, None);
}

/// Jede Seitengröße liefert alle Tasks genau einmal, sortiert nach `sort_key` –
/// auch wenn die Tasks in anderer Reihenfolge gespeichert wurden.
pub fn paging_visits_all_in_sort_order<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut key = SortKey::first();
    let mut tasks = Vec::new();
    for i in 0..25 {
        tasks.push(f.task(f.work.id(), &format!("T{i}"), key.clone()));
        key = after(&key);
    }
    let mut shuffled = tasks.clone();
    shuffled.reverse();
    shuffled.swap(3, 17);
    f.save(&shuffled);
    for limit in [1, 2, 7, 25, 100] {
        assert_eq!(f.ids(all(), limit), ids_of(&tasks), "limit {limit}");
    }
}

pub fn equal_sort_keys_ordered_by_id<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let a = f.task(f.inbox.id(), "a", SortKey::first());
    let b = f.task(f.inbox.id(), "b", SortKey::first());
    f.save(&[b.clone(), a.clone()]);
    let mut expected = [a.id(), b.id()];
    expected.sort();
    assert_eq!(f.ids(all(), 1), expected);
}

pub fn last_sort_key_of_list<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    assert_eq!(block_on(f.repo.last_sort_key(f.work.id())).unwrap(), None);
    assert_eq!(
        block_on(f.repo.last_sort_key(TaskListId::new())).unwrap(),
        None
    );

    // Mehr als eine Seite, gespeichert in gemischter Reihenfolge; die letzte ist erledigt.
    let mut tasks = f.add_tasks(f.work.id(), PAGE_LIMIT_MAX as usize + 3);
    tasks.reverse();
    let mut last = tasks[0].clone();
    last.complete(&f.clock);
    f.save(&[last.clone()]);
    f.add_tasks(f.inbox.id(), 1);
    assert_eq!(
        block_on(f.repo.last_sort_key(f.work.id()))
            .unwrap()
            .as_ref(),
        Some(last.sort_key())
    );
    assert_eq!(
        block_on(f.repo.last_sort_key(f.inbox.id())).unwrap(),
        Some(SortKey::first())
    );
}

pub fn rejects_invalid_limit_and_cursor<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    for limit in [0, PAGE_LIMIT_MAX + 1] {
        assert!(
            matches!(f.page(all(), None, limit), Err(CoreError::Validation(_))),
            "limit {limit}"
        );
    }
    assert!(f.page(all(), None, PAGE_LIMIT_MAX).is_ok());
    let result = f.page(all(), Some("garbage".into()), 10);
    assert!(
        matches!(result, Err(CoreError::Validation(_))),
        "{result:?}"
    );
}

pub fn filter_inbox_and_list<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let inbox_tasks = f.add_tasks(f.inbox.id(), 2);
    let work_tasks = f.add_tasks(f.work.id(), 3);
    assert_eq!(
        f.ids(TaskQuery::view(SmartView::Inbox), 10),
        ids_of(&inbox_tasks)
    );
    assert_eq!(
        f.ids(TaskQuery::view(SmartView::List(f.work.id())), 2),
        ids_of(&work_tasks)
    );
    assert_eq!(
        f.ids(TaskQuery::view(SmartView::List(f.inbox.id())), 10),
        ids_of(&inbox_tasks)
    );
    assert!(f
        .ids(TaskQuery::view(SmartView::List(TaskListId::new())), 10)
        .is_empty());
}

/// Today = fällig an diesem Tag oder früher; Upcoming = danach; ohne Fälligkeit in keiner.
pub fn filter_today_and_upcoming<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let start = Timestamp::from_millis(1_000);
    let end = Timestamp::from_millis(2_000);
    let today = LocalDay::new(date(17), start, end).expect("day");
    let mut t = f.add_tasks(f.inbox.id(), 6);
    let dues = [
        Some(Due::Date(date(16))),
        Some(Due::Date(date(17))),
        Some(Due::Date(date(18))),
        Some(Due::DateTime {
            at: Timestamp::from_millis(1_999),
            tz: "UTC".into(),
        }),
        Some(Due::DateTime {
            at: end,
            tz: "UTC".into(),
        }),
        None,
    ];
    for (task, due) in t.iter_mut().zip(dues) {
        task.set_due(due, &f.clock).expect("due");
    }
    f.save(&t);
    assert_eq!(
        f.ids(TaskQuery::view(SmartView::Today(today)), 2),
        [t[0].id(), t[1].id(), t[3].id()]
    );
    assert_eq!(
        f.ids(TaskQuery::view(SmartView::Upcoming(today)), 2),
        [t[4].id(), t[2].id()],
        "nach Fälligkeit: 2 000 ms vor dem 18. (≈ 86 401 000 ms)"
    );
}

/// Heute/Geplant sortieren nach Fälligkeit (Ganztägiges als Tagesbeginn, vor Uhrzeiten desselben
/// Tages), bei Gleichstand nach `sort_key`, dann `id` – auch über Seitengrenzen (ADR-026).
pub fn due_views_sorted_by_due<R: TaskRepository>(repo: R) {
    const DAY: i64 = 86_400_000;
    let f = setup(repo);
    let start = Timestamp::from_millis(100 * DAY);
    let today = LocalDay::new(date(17), start, start.plus_millis(DAY)).expect("day");
    let at = |ms: i64| Due::DateTime {
        at: start.plus_millis(ms),
        tz: "UTC".into(),
    };
    // Anlegereihenfolge = sort_key-Reihenfolge, absichtlich quer zur Fälligkeit.
    let mut t = f.add_tasks(f.inbox.id(), 9);
    let dues = [
        at(DAY + 3_600_000), // 18., 01:00
        Due::Date(date(20)),
        Due::Date(date(18)), // ganztägig vor 01:00
        at(-DAY),            // überfällig, 16. 00:00
        Due::Date(date(17)),
        Due::Date(date(15)), // ältestes
        at(5_000),           // heute 00:00:05
        Due::Date(date(18)), // gleicher Tag wie t[2]: danach (sort_key)
        Due::Date(date(17)),
    ];
    for (task, due) in t.iter_mut().zip(dues) {
        task.set_due(Some(due), &f.clock).expect("due");
    }
    f.save(&t);
    for limit in [1, 2, 3, 10] {
        assert_eq!(
            f.ids(TaskQuery::view(SmartView::Today(today)), limit),
            [t[5].id(), t[3].id(), t[4].id(), t[8].id(), t[6].id()],
            "Heute, limit {limit}"
        );
        assert_eq!(
            f.ids(TaskQuery::view(SmartView::Upcoming(today)), limit),
            [t[2].id(), t[7].id(), t[0].id(), t[1].id()],
            "Geplant, limit {limit}"
        );
    }
}

impl<R: TaskRepository> Fixture<R> {
    fn search(&self, text: &str, limit: u32) -> Vec<String> {
        let query = SearchQuery {
            text: text.into(),
            limit,
        };
        block_on(self.repo.search_tasks(&query))
            .expect("search")
            .into_iter()
            .map(|t| t.title)
            .collect()
    }

    /// Tasks mit Titel und Notiz, aufsteigende Sort-Keys, gespeichert.
    fn add_titled(&self, items: &[(&str, &str)]) -> Vec<Task> {
        let mut key = SortKey::first();
        let tasks: Vec<Task> = items
            .iter()
            .map(|(title, notes)| {
                let mut task = self.task(self.inbox.id(), title, key.clone());
                task.set_notes(notes, &self.clock);
                key = after(&key);
                task
            })
            .collect();
        self.save(&tasks);
        tasks
    }
}

/// Suche (ADR-027): jedes Wort als Teilwort in Titel **oder** Notiz, ohne Groß/Klein – auch bei
/// Umlauten; gelöschte Tasks nie.
pub fn search_matches_words_in_title_or_notes<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let t = f.add_titled(&[
        ("Bericht schreiben", ""),
        ("Übungsblatt 3", "Fragen zur Aufgabe 2"),
        ("Einkaufen", "Milch und Brot"),
        ("Zahnarzt", "Über die Brücke"),
        ("Alt", "Bericht von gestern"),
    ]);
    assert_eq!(
        f.search("richt", 10),
        ["Bericht schreiben", "Alt"],
        "Teilwort, Titel vor Notiz"
    );
    assert_eq!(
        f.search("BERICHT", 10),
        ["Bericht schreiben", "Alt"],
        "ohne Groß/Klein"
    );
    assert_eq!(
        f.search("übung", 10),
        ["Übungsblatt 3"],
        "Umlaut, ohne Groß/Klein"
    );
    assert_eq!(f.search("über", 10), ["Zahnarzt"], "Umlaut in der Notiz");
    assert_eq!(
        f.search("milch brot", 10),
        ["Einkaufen"],
        "alle Wörter, auch über Titel/Notiz verteilt"
    );
    assert_eq!(f.search("blatt aufgabe", 10), ["Übungsblatt 3"]);
    assert!(f.search("milch zahnarzt", 10).is_empty(), "UND, nicht ODER");
    assert_eq!(
        f.search("3", 10),
        ["Übungsblatt 3"],
        "kurze Wörter (< 3 Zeichen)"
    );
    assert_eq!(f.search("KA", 10), ["Einkaufen"]);
    assert!(
        f.search("   ", 10).is_empty(),
        "leerer Text = keine Treffer"
    );

    block_on(
        f.repo
            .commit(ChangeSet::new().delete(EntityRef::Task(t[0].id()))),
    )
    .expect("delete");
    assert_eq!(f.search("richt", 10), ["Alt"], "gelöschte nie");
    let mut renamed = t[2].clone();
    renamed.rename("Wocheneinkauf", &f.clock).expect("title");
    renamed.set_notes("", &f.clock);
    f.save(&[renamed]);
    assert!(f.search("milch", 10).is_empty(), "Index folgt Änderungen");
    assert_eq!(f.search("woche", 10), ["Wocheneinkauf"]);
}

/// Offene vor erledigten, Titel- vor Notiztreffern, dann Sort-Key; `limit` schneidet ab.
pub fn search_ranks_open_and_title_hits_first<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut t = f.add_titled(&[
        ("Notiz a", "Projekt"),
        ("Projekt erledigt", ""),
        ("Projekt b", ""),
        ("Projekt c", ""),
    ]);
    t[1].complete(&f.clock);
    f.save(&t);
    assert_eq!(
        f.search("projekt", 10),
        ["Projekt b", "Projekt c", "Notiz a", "Projekt erledigt"]
    );
    assert_eq!(f.search("projekt", 2), ["Projekt b", "Projekt c"]);
}

pub fn search_rejects_invalid_input<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    for limit in [0, PAGE_LIMIT_MAX + 1] {
        let query = SearchQuery {
            text: "x".into(),
            limit,
        };
        assert!(matches!(
            block_on(f.repo.search_tasks(&query)),
            Err(CoreError::Validation(_))
        ));
    }
    let long = SearchQuery {
        text: "x".repeat(SEARCH_TEXT_MAX_CHARS + 1),
        limit: 10,
    };
    assert!(matches!(
        block_on(f.repo.search_tasks(&long)),
        Err(CoreError::Validation(_))
    ));
    // Sonderzeichen der Abfragesprache sind normaler Text.
    f.add_titled(&[("a \"b\" OR c* (d)", "")]);
    assert_eq!(f.search("\"b\"", 10), ["a \"b\" OR c* (d)"]);
    assert_eq!(f.search("c* (d)", 10), ["a \"b\" OR c* (d)"]);
    assert_eq!(f.search("100%", 10), Vec::<String>::new());
}

/// Completed: nur erledigte Tasks, zuletzt erledigte zuerst – auch über Seitengrenzen.
pub fn filter_completed_newest_first<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut t = f.add_tasks(f.work.id(), 4);
    t[2].complete(&f.clock);
    t[0].complete(&f.clock);
    t[3].complete(&f.clock);
    f.save(&t);
    let expected = [t[3].id(), t[0].id(), t[2].id()];
    for limit in [1, 2, 10] {
        assert_eq!(
            f.ids(TaskQuery::view(SmartView::Completed), limit),
            expected,
            "limit {limit}"
        );
    }
}

pub fn filter_status_and_tag<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut t = f.add_tasks(f.inbox.id(), 3);
    t[1].complete(&f.clock);
    f.save(&t);
    assert_eq!(
        f.ids(TaskQuery::open(SmartView::All), 10),
        [t[0].id(), t[2].id()]
    );
    let done = TaskQuery {
        status: Some(crate::domain::TaskStatus::Done),
        ..all()
    };
    assert_eq!(f.ids(done, 10), [t[1].id()]);
    let tagged = TaskQuery {
        tag: Some(crate::domain::TagId::new()),
        ..all()
    };
    assert!(f.ids(tagged, 10).is_empty());
}

/// Papierkorb (ADR-031): nur in [`SmartView::Trash`] (zuletzt gelöschte zuerst, seitenweise),
/// sonst nirgends – auch nicht in der Suche.
pub fn trash_view_hides_trashed_everywhere_else<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut t = f.add_tasks(f.inbox.id(), 4);
    for (task, at) in t.iter_mut().zip([0, 30, 20, 10]) {
        if at > 0 {
            task.set_trashed(Some(Timestamp::from_millis(at)), &f.clock);
        }
        task.set_notes("Papierkorbtest", &f.clock);
    }
    f.save(&t);
    assert_eq!(f.ids(all(), 10), [t[0].id()]);
    assert_eq!(f.ids(TaskQuery::view(SmartView::Inbox), 10), [t[0].id()]);
    for limit in [1, 2, 10] {
        assert_eq!(
            f.ids(TaskQuery::view(SmartView::Trash), limit),
            [t[1].id(), t[2].id(), t[3].id()],
            "Papierkorb, limit {limit}"
        );
    }
    assert_eq!(f.search("Papierkorbtest", 10), ["Task 0"]);
    let page = f
        .page(TaskQuery::view(SmartView::Trash), None, 1)
        .expect("page");
    assert_eq!(page.items[0].trashed_at, Some(Timestamp::from_millis(30)));
}

/// Die Zusammenfassung entspricht `TaskSummary::from(&task)`.
pub fn summary_matches_task<R: TaskRepository>(repo: R) {
    let f = setup(repo);
    let mut task = f.task(f.inbox.id(), "Zusammenfassung", SortKey::first());
    task.set_notes("mit Notiz", &f.clock);
    task.set_priority(crate::domain::Priority::High, &f.clock);
    task.set_due(Some(Due::Date(date(20))), &f.clock)
        .expect("due");
    f.save(std::slice::from_ref(&task));
    let page = f.page(all(), None, 10).expect("page");
    assert_eq!(page.items, [TaskSummary::from(&task)]);
}
