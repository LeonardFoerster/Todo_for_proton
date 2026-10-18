//! Frühwarnung für langsame Speicherzugriffe (P4-04). Ausführen im Release-Build:
//!
//! ```sh
//! cargo run --release -p todo-storage-sqlite --example bench_storage
//! ```
//!
//! Ziele (`architecture.md` §6): erste Seite (50) der Inbox < 5 ms, einzelner Commit < 2 ms,
//! Sammelaktionen mit 1 000 IDs < 50 ms (P0-01).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::executor::block_on;
use todo_core::{
    api::{
        dto::{CreateTaskDto, MoveTargetDto},
        CoreApi,
    },
    domain::{SortKey, Task, TaskList, Timestamp},
    ports::{ChangeSet, Clock, PageRequest, SmartView, TaskQuery, TaskRepository},
};
use todo_storage_sqlite::SqliteTaskRepository;

const TASKS: usize = 10_000;
const RUNS: usize = 200;

#[derive(Debug)]
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after 1970")
            .as_millis();
        Timestamp::from_millis(i64::try_from(ms).expect("fits"))
    }
}

/// Median und 95. Perzentil von `runs` Durchläufen.
fn measure(runs: usize, mut f: impl FnMut()) -> (Duration, Duration) {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    times.sort();
    (times[runs / 2], times[runs * 95 / 100])
}

fn report(name: &str, (median, p95): (Duration, Duration), target: Option<Duration>) {
    let verdict = match target {
        Some(t) if p95 <= t => format!("ok (Ziel < {t:?})"),
        Some(t) => format!("ZU LANGSAM (Ziel < {t:?})"),
        None => String::new(),
    };
    println!("{name:<44} median {median:>10.3?}   p95 {p95:>10.3?}   {verdict}");
}

fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bench.sqlite3");
    let repo = Arc::new(SqliteTaskRepository::open(&path).expect("open"));
    let clock = SystemClock;

    // --- Daten: Inbox mit 10 000 Tasks, jede 10. erledigt -------------------------------------
    let inbox = TaskList::inbox(SortKey::first(), &clock);
    block_on(repo.commit(ChangeSet::new().upsert_list(inbox.clone()))).expect("inbox");
    let start = Instant::now();
    let mut key = SortKey::first();
    let mut tasks = Vec::with_capacity(TASKS);
    for chunk in 0..TASKS / 1_000 {
        let mut set = ChangeSet::new();
        for i in 0..1_000 {
            let mut task = Task::new(
                inbox.id(),
                &format!("Task {chunk}-{i}"),
                key.clone(),
                &clock,
            )
            .expect("task");
            if i % 10 == 0 {
                task.complete(&clock);
            }
            key = SortKey::between(Some(&key), None).expect("key");
            tasks.push(task.clone());
            set = set.upsert_task(task);
        }
        block_on(repo.commit(set)).expect("insert");
    }
    println!(
        "{TASKS} Tasks eingefügt (10 Commits à 1 000) in {:.1?}\n",
        start.elapsed()
    );

    // --- Lesen ---------------------------------------------------------------------------------
    let page = |query: TaskQuery, cursor: Option<String>| {
        block_on(repo.list_tasks(&query, PageRequest { cursor, limit: 50 })).expect("page")
    };
    let ms = Duration::from_millis;
    report(
        "Inbox, erste Seite (50)",
        measure(RUNS, || drop(page(TaskQuery::view(SmartView::Inbox), None))),
        Some(ms(5)),
    );
    report(
        "Inbox offen, erste Seite (50)",
        measure(RUNS, || drop(page(TaskQuery::open(SmartView::Inbox), None))),
        Some(ms(5)),
    );
    let middle = format!(
        "s:{}:{}",
        tasks[TASKS / 2].sort_key(),
        tasks[TASKS / 2].id()
    );
    report(
        "Inbox, Seite ab Mitte (Cursor)",
        measure(RUNS, || {
            drop(page(
                TaskQuery::view(SmartView::Inbox),
                Some(middle.clone()),
            ));
        }),
        Some(ms(5)),
    );
    report(
        "Alle offen, erste Seite (50)",
        measure(RUNS, || drop(page(TaskQuery::open(SmartView::All), None))),
        Some(ms(5)),
    );
    report(
        "Erledigt, erste Seite (50)",
        measure(RUNS, || {
            drop(page(TaskQuery::view(SmartView::Completed), None));
        }),
        Some(ms(5)),
    );
    report(
        "get_task",
        measure(RUNS, || {
            drop(block_on(repo.get_task(tasks[1234].id())).expect("get"));
        }),
        None,
    );

    // --- Schreiben -----------------------------------------------------------------------------
    let mut moving = tasks[TASKS / 2].clone();
    let (a, b) = (tasks[10].sort_key().clone(), tasks[11].sort_key().clone());
    let mut toggle = false;
    report(
        "Commit: Task verschieben (1 Upsert)",
        measure(RUNS, || {
            toggle = !toggle;
            let target = if toggle {
                SortKey::between(Some(&a), Some(&b)).expect("key")
            } else {
                tasks[TASKS / 2].sort_key().clone()
            };
            moving.move_to(inbox.id(), target, &clock);
            block_on(repo.commit(ChangeSet::new().upsert_task(moving.clone()))).expect("move");
        }),
        Some(ms(2)),
    );

    // --- Use-Cases über CoreApi (create: nur letzter Key; move mit Nachbar: ganze Liste, FIX-11) -
    let api = CoreApi::new(repo.clone(), Arc::new(SystemClock));
    let first = tasks[0].id().to_string();
    report(
        "CoreApi::create_task (Inbox mit 10k)",
        measure(20, || {
            drop(
                block_on(api.create_task(CreateTaskDto {
                    title: "neu".into(),
                    list_id: None,
                    ..CreateTaskDto::default()
                }))
                .expect("create"),
            );
        }),
        None,
    );
    report(
        "CoreApi::move_task nach oben (Inbox mit 10k)",
        measure(20, || {
            drop(
                block_on(api.move_task(
                    tasks[TASKS - 1].id().to_string(),
                    MoveTargetDto {
                        list_id: None,
                        after: None,
                        before: Some(first.clone()),
                    },
                ))
                .expect("move"),
            );
        }),
        None,
    );

    // --- Sammelaktionen über CoreApi: 1 000 IDs in der 10k-Inbox (P0-01) -----------------------
    let block = |i: usize| -> Vec<String> {
        tasks[i * 1_000..(i + 1) * 1_000]
            .iter()
            .map(|t| t.id().to_string())
            .collect()
    };
    let mut to_start = false;
    report(
        "CoreApi::move_tasks 1 000 IDs (Inbox mit 10k)",
        measure(10, || {
            to_start = !to_start;
            let target = MoveTargetDto {
                list_id: None,
                after: None,
                before: to_start.then(|| tasks[9_999].id().to_string()),
            };
            let moved = block_on(api.move_tasks(block(4), target)).expect("move");
            assert_eq!(moved.len(), 1_000);
        }),
        Some(ms(50)),
    );
    let mut round = 0;
    report(
        "CoreApi::complete_tasks 1 000 IDs",
        measure(10, || {
            round += 1;
            let done = round % 2 == 1;
            drop(block_on(api.complete_tasks(block(5), done)).expect("complete"));
        }),
        Some(ms(50)),
    );
    let mut chunk = 0;
    let mut snapshots = Vec::new();
    report(
        "CoreApi::delete_tasks 1 000 IDs",
        measure(4, || {
            chunk += 1;
            snapshots = block_on(api.delete_tasks(block(chunk))).expect("delete");
            assert_eq!(snapshots.len(), 1_000);
        }),
        Some(ms(50)),
    );
    report(
        "CoreApi::restore_tasks 1 000 Tasks",
        measure(1, || {
            let restored = block_on(api.restore_tasks(snapshots.clone())).expect("restore");
            assert_eq!(restored.len(), 1_000);
        }),
        Some(ms(50)),
    );
}
