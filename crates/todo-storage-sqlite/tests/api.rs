//! `CoreApi` auf echter SQLite-Datei: Ablauf wie in einer UI-Shell, inkl. Neustart.

use std::{path::Path, sync::Arc};

use futures::executor::block_on;
use todo_core::{
    api::{
        dto::{CreateTaskDto, MoveTargetDto, PageRequestDto, TaskQueryDto, TaskStatusDto, ViewDto},
        CoreApi,
    },
    domain::Timestamp,
    ports::SteppingClock,
};
use todo_storage_sqlite::SqliteTaskRepository;

fn api(path: &Path) -> CoreApi {
    CoreApi::new(
        Arc::new(SqliteTaskRepository::open(path).expect("open")),
        Arc::new(SteppingClock::new(
            Timestamp::from_millis(1_760_000_000_000),
            1,
        )),
    )
}

fn titles(api: &CoreApi, view: ViewDto) -> Vec<(String, TaskStatusDto)> {
    let page = block_on(api.list_tasks(
        TaskQueryDto {
            view,
            status: None,
            tag: None,
            priority: None,
            sort: None,
        },
        PageRequestDto {
            cursor: None,
            limit: 50,
        },
    ))
    .expect("list");
    page.items
        .into_iter()
        .map(|s| (s.title, s.status))
        .collect()
}

#[test]
fn full_flow_persists_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.sqlite3");
    {
        let api = api(&path);
        let create = |title: &str| {
            block_on(api.create_task(CreateTaskDto {
                title: title.into(),
                list_id: None,
                ..CreateTaskDto::default()
            }))
            .expect("create")
        };
        let milk = create("Milch");
        let bread = create("Brot");
        let eggs = create("Eier");
        block_on(api.complete_task(bread.id.clone(), true)).expect("complete");
        block_on(api.move_task(
            eggs.id,
            MoveTargetDto {
                list_id: None,
                after: None,
                before: Some(milk.id),
            },
        ))
        .expect("move");
    }

    let api = api(&path);
    let open = TaskStatusDto::Open;
    assert_eq!(
        titles(&api, ViewDto::Inbox),
        [
            ("Eier".to_owned(), open),
            ("Milch".to_owned(), open),
            ("Brot".to_owned(), TaskStatusDto::Done),
        ]
    );
    assert_eq!(
        titles(&api, ViewDto::Completed),
        [("Brot".to_owned(), TaskStatusDto::Done)]
    );
    let lists = block_on(api.list_task_lists()).expect("lists");
    assert_eq!(
        lists.len(),
        1,
        "Inbox wird beim Neustart nicht doppelt angelegt"
    );
    assert!(lists[0].is_inbox);
}

/// FIX-10: Parallele `create_task` (wie `Promise.all` im Selbsttest) bekommen verschiedene
/// Sort-Keys. Der Datenbank-Thread lässt die Aufrufe tatsächlich ineinander laufen.
#[test]
fn parallel_creates_get_distinct_sort_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let api = api(&dir.path().join("todo.sqlite3"));
    let creates = (0..50).map(|i| {
        api.create_task(CreateTaskDto {
            title: format!("T{i}"),
            list_id: None,
            ..CreateTaskDto::default()
        })
    });
    let tasks = block_on(futures::future::join_all(creates));
    let mut keys: Vec<String> = tasks
        .into_iter()
        .map(|t| t.expect("create").sort_key)
        .collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 50, "doppelte Sort-Keys");
}
