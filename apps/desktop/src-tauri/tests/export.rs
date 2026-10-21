//! Export und Import (N6-04, ADR-032): Rundlauf über echte Dateien – Export aus einem Profil,
//! Import in ein leeres, gleiche Listen und Tasks; Notizen als Dateien mit Ordnern.

use std::{fs, sync::Arc};

use todo_core::{
    api::{
        dto::{CreateTaskDto, PageRequestDto, TaskQueryDto, ViewDto},
        note_dto::NoteKindDto,
        CoreApi,
    },
    testing::InMemoryTaskRepository,
};
use todo_desktop::{export, state::SystemClock};

fn api() -> CoreApi {
    CoreApi::new(
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(SystemClock),
    )
}

async fn titles(api: &CoreApi, view: ViewDto) -> Vec<String> {
    let mut t: Vec<String> = api
        .list_tasks(
            TaskQueryDto {
                view,
                status: None,
                tag: None,
            },
            PageRequestDto {
                cursor: None,
                limit: 500,
            },
        )
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|t| t.title)
        .collect();
    t.sort();
    t
}

#[tokio::test]
async fn export_writes_files_and_import_restores_into_an_empty_profile() {
    let source = api();
    source.ensure_inbox().await.unwrap();
    let work = source.create_task_list("Arbeit".into()).await.unwrap();
    source
        .create_task(CreateTaskDto {
            title: "Bericht".into(),
            list_id: Some(work.id.clone()),
            ..CreateTaskDto::default()
        })
        .await
        .unwrap();
    let gone = source
        .create_task(CreateTaskDto {
            title: "Alt".into(),
            ..CreateTaskDto::default()
        })
        .await
        .unwrap();
    source.trash_tasks(vec![gone.id]).await.unwrap();
    let uni = source.create_note_folder(None, "Uni".into()).await.unwrap();
    let folder = source
        .create_note_folder(Some(uni.id), "Analysis".into())
        .await
        .unwrap();
    let note = source
        .create_note(Some(folder.id.clone()), NoteKindDto::Markdown)
        .await
        .unwrap();
    source
        .update_note_body(
            note.id.clone(),
            "Blatt 3\nAufgabe".into(),
            note.body_rev.clone(),
        )
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let result = export::write_export(&source, dir.path(), "2026-10-21")
        .await
        .unwrap();
    assert_eq!((result.lists, result.tasks, result.notes), (2, 2, 1));
    let out = dir.path().join("Tasks-Export-2026-10-21");
    assert_eq!(result.path, out.to_string_lossy());
    assert!(out.join("tasks.json").is_file());
    let note_file = out
        .join("notizen")
        .join("Uni")
        .join("Analysis")
        .join("Blatt 3.md");
    assert_eq!(fs::read_to_string(note_file).unwrap(), "Blatt 3\nAufgabe");
    // Zweiter Export am selben Tag → eigener Ordner.
    let second = export::write_export(&source, dir.path(), "2026-10-21")
        .await
        .unwrap();
    assert!(second.path.ends_with("Tasks-Export-2026-10-21 (2)"));
    assert!(export::write_export(&source, dir.path(), "21.10.2026")
        .await
        .is_err());

    let target = api();
    let report = export::import_file(&target, &out.join("tasks.json"))
        .await
        .unwrap();
    assert_eq!(
        (
            report.lists_created,
            report.tasks_created,
            report.tasks_skipped
        ),
        (1, 2, 0)
    );
    assert_eq!(titles(&target, ViewDto::All).await, ["Bericht"]);
    assert_eq!(titles(&target, ViewDto::Trash).await, ["Alt"]);
    let again = export::import_file(&target, &out.join("tasks.json"))
        .await
        .unwrap();
    assert_eq!((again.tasks_created, again.tasks_skipped), (0, 2));

    fs::write(dir.path().join("kaputt.json"), "{\"format\":\"anders\"}").unwrap();
    assert!(
        export::import_file(&target, &dir.path().join("kaputt.json"))
            .await
            .is_err()
    );
}
