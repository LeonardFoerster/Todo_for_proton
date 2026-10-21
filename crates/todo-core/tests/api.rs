//! Integrationstest der Fassade: kompletter Ablauf nur über DTOs, wie ihn eine UI-Shell nutzt.

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use todo_core::{
    api::{
        dto::{
            CreateTaskDto, DueDto, LocalDayDto, MoveTargetDto, PageRequestDto, PriorityDto,
            TaskPatchDto, TaskQueryDto, TaskStatusDto, ViewDto,
        },
        CoreApi, CoreEvent, EventSink,
    },
    domain::Timestamp,
    ports::SteppingClock,
    testing::InMemoryTaskRepository,
    CoreError,
};

#[derive(Default)]
struct Recorder(Mutex<Vec<CoreEvent>>);

impl EventSink for Recorder {
    fn emit(&self, event: CoreEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    fn take(&self) -> Vec<CoreEvent> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

fn api() -> (CoreApi, Arc<Recorder>) {
    let api = CoreApi::new(
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(SteppingClock::new(
            Timestamp::from_millis(1_760_000_000_000),
            1,
        )),
    );
    let recorder = Arc::new(Recorder::default());
    api.subscribe(Box::new(recorder.clone()));
    (api, recorder)
}

fn view(api: &CoreApi, view: ViewDto) -> Vec<(String, TaskStatusDto)> {
    let query = TaskQueryDto {
        view,
        status: None,
        tag: None,
    };
    let page = block_on(api.list_tasks(
        query,
        PageRequestDto {
            cursor: None,
            limit: 100,
        },
    ))
    .unwrap();
    page.items
        .into_iter()
        .map(|s| (s.title, s.status))
        .collect()
}

fn titles(api: &CoreApi, v: ViewDto) -> Vec<String> {
    view(api, v).into_iter().map(|(t, _)| t).collect()
}

#[test]
fn full_flow_inbox_create_move_complete_list() {
    let (api, events) = api();

    // Start: Inbox sicherstellen.
    let inbox = block_on(api.ensure_inbox()).unwrap();
    assert!(inbox.is_inbox);
    assert_eq!(
        events.take(),
        [CoreEvent::Changed {
            tasks: vec![],
            lists: vec![inbox.id.clone()],
            notes: Vec::new(),
            note_folders: Vec::new(),
            tags: Vec::new(),
        }]
    );
    assert_eq!(block_on(api.ensure_inbox()).unwrap(), inbox, "idempotent");
    assert!(events.take().is_empty(), "kein Event ohne Änderung");

    // Tasks anlegen (ohne Liste → Inbox, ans Ende).
    let create = |title: &str| {
        block_on(api.create_task(CreateTaskDto {
            title: title.into(),
            list_id: None,
            ..CreateTaskDto::default()
        }))
        .unwrap()
    };
    let a = create("a");
    let b = create("b");
    let c = create("c");
    assert_eq!(a.list_id, inbox.id);
    assert_eq!(titles(&api, ViewDto::Inbox), ["a", "b", "c"]);
    assert_eq!(events.take().len(), 3);

    // c vor a schieben.
    block_on(api.move_task(
        c.id.clone(),
        MoveTargetDto {
            before: Some(a.id.clone()),
            ..MoveTargetDto::default()
        },
    ))
    .unwrap();
    assert_eq!(titles(&api, ViewDto::Inbox), ["c", "a", "b"]);
    assert_eq!(
        events.take(),
        [CoreEvent::Changed {
            tasks: vec![c.id.clone()],
            lists: vec![],
            notes: Vec::new(),
            note_folders: Vec::new(),
            tags: Vec::new(),
        }]
    );

    // b in eine neue Liste verschieben.
    let work = block_on(api.create_task_list("Arbeit".into())).unwrap();
    block_on(api.move_task(
        b.id.clone(),
        MoveTargetDto {
            list_id: Some(work.id.clone()),
            ..MoveTargetDto::default()
        },
    ))
    .unwrap();
    assert_eq!(
        titles(
            &api,
            ViewDto::List {
                list_id: work.id.clone()
            }
        ),
        ["b"]
    );

    // a abhaken.
    let done = block_on(api.complete_task(a.id.clone(), true)).unwrap();
    assert_eq!(done.status, TaskStatusDto::Done);
    assert!(done
        .completed_at
        .as_deref()
        .is_some_and(|s| s.ends_with('Z')));

    // Listen.
    assert_eq!(
        view(&api, ViewDto::Inbox),
        [
            ("c".into(), TaskStatusDto::Open),
            ("a".into(), TaskStatusDto::Done)
        ]
    );
    assert_eq!(titles(&api, ViewDto::Completed), ["a"]);
    let open = TaskQueryDto {
        view: ViewDto::All,
        status: Some(TaskStatusDto::Open),
        tag: None,
    };
    let page = block_on(api.list_tasks(
        open,
        PageRequestDto {
            cursor: None,
            limit: 1,
        },
    ))
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some(), "zweite Seite vorhanden");
    let names: Vec<String> = block_on(api.list_task_lists())
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, ["Inbox", "Arbeit"]);
}

#[test]
fn update_due_today_view_and_deletes() {
    let (api, events) = api();
    block_on(api.ensure_inbox()).unwrap();
    let t = block_on(api.create_task(CreateTaskDto {
        title: "Bericht".into(),
        list_id: None,
        ..CreateTaskDto::default()
    }))
    .unwrap();

    let patch = TaskPatchDto {
        notes: Some("Kapitel 3".into()),
        priority: Some(PriorityDto::High),
        due: Some(Some(DueDto::Date {
            date: "2026-10-17".into(),
        })),
        ..TaskPatchDto::default()
    };
    let updated = block_on(api.update_task(t.id.clone(), patch)).unwrap();
    assert_eq!(updated.priority, PriorityDto::High);
    assert_eq!(
        block_on(api.get_task(t.id.clone())).unwrap().notes,
        "Kapitel 3"
    );

    let day = LocalDayDto {
        date: "2026-10-17".into(),
        start: "2026-10-16T22:00:00Z".into(),
        end: "2026-10-17T22:00:00Z".into(),
    };
    assert_eq!(
        titles(&api, ViewDto::Today { day: day.clone() }),
        ["Bericht"]
    );
    assert!(titles(&api, ViewDto::Upcoming { day }).is_empty());

    // Liste mit Task löschen → Task wandert in die Inbox.
    let work = block_on(api.create_task_list("Arbeit".into())).unwrap();
    block_on(api.move_task(
        t.id.clone(),
        MoveTargetDto {
            list_id: Some(work.id.clone()),
            ..MoveTargetDto::default()
        },
    ))
    .unwrap();
    events.take();
    assert_eq!(block_on(api.delete_task_list(work.id.clone())).unwrap(), 1);
    assert_eq!(
        events.take(),
        [CoreEvent::Changed {
            tasks: vec![t.id.clone()],
            lists: vec![work.id.clone()],
            notes: Vec::new(),
            note_folders: Vec::new(),
            tags: Vec::new(),
        }]
    );
    assert_eq!(titles(&api, ViewDto::Inbox), ["Bericht"]);

    block_on(api.delete_task(t.id.clone())).unwrap();
    assert!(matches!(
        block_on(api.get_task(t.id)),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn invalid_input_maps_to_validation() {
    let (api, events) = api();
    assert!(matches!(
        block_on(api.get_task("keine-id".into())),
        Err(CoreError::Validation(_))
    ));
    assert!(matches!(
        block_on(api.create_task(CreateTaskDto {
            title: " ".into(),
            list_id: None,
            ..CreateTaskDto::default()
        })),
        Err(CoreError::Validation(_))
    ));
    let bad_due = TaskPatchDto {
        due: Some(Some(DueDto::Date {
            date: "morgen".into(),
        })),
        ..TaskPatchDto::default()
    };
    let task = block_on(api.create_task(CreateTaskDto {
        title: "x".into(),
        list_id: None,
        ..CreateTaskDto::default()
    }))
    .unwrap();
    events.take();
    assert!(matches!(
        block_on(api.update_task(task.id, bad_due)),
        Err(CoreError::Validation(_))
    ));
    assert!(events.take().is_empty(), "kein Event bei Fehler");
}

/// Tags (ADR-035) über die Fassade: anlegen per Name, zuweisen, im Export mitnehmen und in einem
/// leeren Profil über den Namen wieder zuordnen.
#[test]
fn tags_assign_filter_and_survive_export_import() {
    let (api, events) = api();
    block_on(api.ensure_inbox()).unwrap();
    let uni = block_on(api.ensure_tag("#Uni".into())).unwrap();
    assert_eq!(uni.name, "Uni");
    let task = block_on(api.create_task(CreateTaskDto {
        title: "Blatt 3".into(),
        tags: Some(vec![uni.id.clone()]),
        ..CreateTaskDto::default()
    }))
    .unwrap();
    assert_eq!(task.tags, std::slice::from_ref(&uni.id));
    let other = block_on(api.create_task(CreateTaskDto {
        title: "Milch".into(),
        ..CreateTaskDto::default()
    }))
    .unwrap();
    let tagged = block_on(api.list_tasks(
        TaskQueryDto {
            view: ViewDto::All,
            status: None,
            tag: Some(uni.id.clone()),
        },
        PageRequestDto {
            cursor: None,
            limit: 10,
        },
    ))
    .unwrap();
    assert_eq!(
        tagged
            .items
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&task.id)
    );
    assert_eq!(tagged.items[0].tags, std::slice::from_ref(&uni.id));
    assert!(events.take().iter().any(
        |e| matches!(e, CoreEvent::Changed { tags, .. } if tags == std::slice::from_ref(&uni.id))
    ));
    block_on(api.update_task(
        other.id.clone(),
        TaskPatchDto {
            tags: Some(vec![uni.id.clone()]),
            ..TaskPatchDto::default()
        },
    ))
    .unwrap();

    let export = block_on(api.export_data()).unwrap();
    assert_eq!(export.tags.len(), 1);
    let (fresh, _) = self::api();
    let report = block_on(fresh.import_data(export)).unwrap();
    assert_eq!(report.tasks_created, 2);
    let tags = block_on(fresh.list_tags()).unwrap();
    assert_eq!(
        tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["Uni"]
    );
    let imported = block_on(fresh.list_tasks(
        TaskQueryDto {
            view: ViewDto::All,
            status: None,
            tag: Some(tags[0].id.clone()),
        },
        PageRequestDto {
            cursor: None,
            limit: 10,
        },
    ))
    .unwrap();
    assert_eq!(imported.items.len(), 2);
}
