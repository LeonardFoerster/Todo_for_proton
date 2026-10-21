//! Frühwarnung für langsame Speicherzugriffe (P4-04). Ausführen im Release-Build:
//!
//! ```sh
//! cargo run --release -p todo-storage-sqlite --example bench_storage
//! ```
//!
//! Ziele (`architecture.md` §6): erste Seite (50) der Inbox < 5 ms, einzelner Commit < 2 ms,
//! Sammelaktionen mit 1 000 IDs < 50 ms (P0-01), Suche (50 Treffer) < 16 ms (P4-05).
//! Notizen (`notes.md` §9, 2 000 Notizen à ~20 KB): Ordner mit 500 Einträgen < 30 ms, Notiz
//! öffnen < 50 ms, Autosave-Commit < 10 ms, Suche p95 < 16 ms.

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
    domain::{Note, NoteFolder, NoteKind, SortKey, Task, TaskList, Timestamp},
    ports::{
        ChangeSet, Clock, FolderItemsQuery, NoteRepository, PageRequest, SearchQuery, SmartView,
        TaskQuery, TaskRepository, PAGE_LIMIT_MAX,
    },
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

    // --- Suche (P4-05, ADR-027) ------------------------------------------------------------------
    let search = |text: &str| {
        let query = SearchQuery {
            text: text.into(),
            limit: 50,
        };
        block_on(repo.search_tasks(&query)).expect("search")
    };
    for (name, text) in [
        ("Suche: Wort in allen 10 000 („task“)", "task"),
        ("Suche: seltenes Teilwort („9-99“)", "9-99"),
        ("Suche: zwei Wörter („task 7-12“)", "task 7-12"),
        ("Suche: kurzes Wort, LIKE („9-“)", "9-"),
        ("Suche: kein Treffer („xyz“)", "xyz"),
    ] {
        report(name, measure(RUNS, || drop(search(text))), Some(ms(16)));
    }

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
            drop(block_on(api.complete_tasks(block(5), done, None)).expect("complete"));
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

    bench_notes();
}

const NOTES: usize = 2_000;
const NOTE_BYTES: usize = 20 * 1024;

/// Wortschatz für [`note_text`]: häufige Füllwörter vorn, dahinter 5 000 Kunstwörter aus Silben.
fn vocabulary() -> Vec<String> {
    const STOP: [&str; 12] = [
        "und", "die", "der", "ist", "von", "mit", "das", "für", "auf", "nicht", "eine", "sich",
    ];
    const SYLLABLES: [&str; 20] = [
        "ra", "ko", "ver", "gen", "zen", "bas", "tor", "lem", "ma", "sa", "funk", "ti", "on", "ab",
        "lei", "tung", "inte", "gral", "ei", "wert",
    ];
    let mut words: Vec<String> = STOP.iter().map(|w| (*w).to_owned()).collect();
    for i in 0..5_000usize {
        let mut w = String::new();
        let mut x = i;
        for _ in 0..3 {
            w.push_str(SYLLABLES[x % SYLLABLES.len()]);
            x /= SYLLABLES.len();
        }
        words.push(format!("{w}{}", i % 7));
    }
    words
}

/// Pseudozufälliger Text (gleich bei jedem Lauf) mit Zipf-ähnlicher Verteilung: Füllwörter in
/// jeder Notiz, Fachwörter selten – damit der Trigram-Index realistisch gefüllt ist.
fn note_text(seed: usize, words: &[String]) -> String {
    let mut out = format!("# Notiz {seed}\n\n");
    let mut x = seed.wrapping_mul(2_654_435_761) | 1;
    while out.len() < NOTE_BYTES {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let index = {
            let u = (x % 1_000_000) as f64 / 1_000_000.0;
            ((u * u * u) * words.len() as f64) as usize
        };
        out.push_str(&words[index.min(words.len() - 1)]);
        out.push(if x.is_multiple_of(11) { '\n' } else { ' ' });
    }
    out
}

/// Wie [`note_text`], aber nur 24 Wörter: jedes steht in jeder Notiz dutzendfach. Schlimmster
/// Fall für den Trigram-Index (nur zur Information).
fn uniform_text(seed: usize) -> String {
    const WORDS: [&str; 24] = [
        "Grenzwert",
        "Folge",
        "Reihe",
        "Konvergenz",
        "Integral",
        "Ableitung",
        "Matrix",
        "Vektor",
        "Eigenwert",
        "Basis",
        "Raum",
        "Funktion",
        "stetig",
        "beschränkt",
        "monoton",
        "Beweis",
        "Satz",
        "Lemma",
        "Definition",
        "Beispiel",
        "Aufgabe",
        "Lösung",
        "und",
        "die",
    ];
    let mut out = String::new();
    let mut x = seed.wrapping_mul(2_654_435_761) | 1;
    while out.len() < NOTE_BYTES {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push_str(WORDS[x % WORDS.len()]);
        out.push(' ');
    }
    out
}

fn bench_notes() {
    bench_note_store(false);
    bench_note_store(true);
}

/// `uniform`: Korpus aus [`uniform_text`] – dann nur die Suche messen.
fn bench_note_store(uniform: bool) {
    let words = vocabulary();
    let dir = tempfile::tempdir().expect("tempdir");
    let repo =
        Arc::new(SqliteTaskRepository::open(dir.path().join("notes.sqlite3")).expect("open"));
    let clock = SystemClock;
    let ms = Duration::from_millis;

    // 40 Ordner; der erste mit 500 Notizen, die übrigen 1 500 verteilt.
    let start = Instant::now();
    let mut folders = Vec::new();
    let mut key = SortKey::first();
    let mut set = ChangeSet::new();
    for i in 0..40 {
        let folder =
            NoteFolder::new(None, &format!("Ordner {i}"), key.clone(), &clock).expect("folder");
        key = SortKey::between(Some(&key), None).expect("key");
        folders.push(folder.clone());
        set = set.upsert_note_folder(folder);
    }
    block_on(repo.commit(set)).expect("folders");
    let keys = SortKey::n_between(None, None, NOTES).expect("keys");
    let mut notes = Vec::with_capacity(NOTES);
    for chunk in 0..NOTES / 100 {
        let mut set = ChangeSet::new();
        for i in 0..100 {
            let n = chunk * 100 + i;
            let folder = if n < 500 { 0 } else { 1 + n % 39 };
            let mut note = Note::new(
                Some(folders[folder].id()),
                NoteKind::Markdown,
                keys[n].clone(),
                &clock,
            );
            let text = if uniform {
                uniform_text(n)
            } else {
                note_text(n, &words)
            };
            note.set_body(&text, None, &clock).expect("body");
            notes.push(note.id());
            set = set.upsert_note(note);
        }
        block_on(repo.commit(set)).expect("notes");
    }
    println!(
        "\n{NOTES} Notizen à ~{} KB{} in 40 Ordnern eingefügt in {:.1?}\n",
        NOTE_BYTES / 1024,
        if uniform { " (24-Wörter-Korpus)" } else { "" },
        start.elapsed()
    );

    if uniform {
        let search = |text: &str| {
            let query = SearchQuery {
                text: text.into(),
                limit: 50,
            };
            block_on(repo.search_notes(&query)).expect("search")
        };
        report(
            "Notes-Suche, 24-Wörter-Korpus („konvergenz“)",
            measure(RUNS / 4, || drop(search("konvergenz"))),
            None,
        );
        return;
    }
    let full_folder = folders[0].id();
    report(
        "Notes: Ordner mit 500 Einträgen aufklappen",
        measure(RUNS, || {
            let page = block_on(repo.list_folder_items(
                FolderItemsQuery {
                    folder: Some(full_folder),
                    include_trashed: false,
                },
                PageRequest::first(PAGE_LIMIT_MAX),
            ))
            .expect("items");
            assert_eq!(page.items.len(), 500);
        }),
        Some(ms(30)),
    );
    report(
        "Notes: Notiz öffnen (get_note, 20 KB)",
        measure(RUNS, || {
            drop(block_on(repo.get_note(notes[777])).expect("get"));
        }),
        Some(ms(50)),
    );

    let mut note = block_on(repo.get_note(notes[42]))
        .expect("get")
        .expect("note");
    let mut round = 0;
    report(
        "Notes: Autosave-Commit (20 KB, Outbox ersetzt)",
        measure(RUNS, || {
            round += 1;
            let base = note.body_rev();
            let text = format!("{}{round}", note.body());
            note.set_body(&text, base, &clock).expect("body");
            block_on(repo.commit(ChangeSet::new().upsert_note(note.clone()))).expect("save");
            note = block_on(repo.get_note(notes[42]))
                .expect("get")
                .expect("note");
        }),
        Some(ms(10)),
    );
    report(
        "Notes: Autosave-Commit (20 KB, neue Revision)",
        measure(RUNS, || {
            round += 1;
            // Wie nach einem Upload-Versuch: die vorige Revision ist herausgegeben.
            block_on(repo.freeze_outbox(u64::MAX)).expect("freeze");
            let base = note.body_rev();
            let text = format!("{}{round}", note.body());
            note.set_body(&text, base, &clock).expect("body");
            block_on(repo.commit(ChangeSet::new().upsert_note(note.clone()))).expect("save");
            note = block_on(repo.get_note(notes[42]))
                .expect("get")
                .expect("note");
        }),
        Some(ms(10)),
    );

    let search = |text: &str| {
        let query = SearchQuery {
            text: text.into(),
            limit: 50,
        };
        block_on(repo.search_notes(&query)).expect("search")
    };
    for (name, text) in [
        ("Notes-Suche: Füllwort in jeder Notiz („und“)", "und"),
        ("Notes-Suche: Teilwort, häufig („ver“)", "ver"),
        ("Notes-Suche: Fachwort („kogenra“)", "kogenra"),
        ("Notes-Suche: Name („notiz 1234“)", "notiz 1234"),
        ("Notes-Suche: zwei Wörter („und kogen“)", "und kogen"),
        ("Notes-Suche: kurzes Wort, nur Namen („ra“)", "ra"),
        ("Notes-Suche: kein Treffer („xyzzy“)", "xyzzy"),
    ] {
        report(name, measure(RUNS / 4, || drop(search(text))), Some(ms(16)));
    }
    // Wie die Palette (N5-10): über die Fassade, mit Ordnerpfad je Treffer.
    let api = CoreApi::new(repo.clone(), Arc::new(SystemClock));
    report(
        "CoreApi::search_notes mit Pfad („und kogen“)",
        measure(RUNS / 4, || {
            drop(block_on(api.search_notes("und kogen".into(), 50)).expect("search"));
        }),
        Some(ms(16)),
    );
}
