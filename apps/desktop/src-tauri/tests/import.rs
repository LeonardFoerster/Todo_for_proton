//! Ordner importieren (N5-11): echter Testordner mit allen Sonderfällen aus `notes.md` §8.

use std::{fs, sync::Arc};

use todo_core::{
    api::{
        dto::PageRequestDto,
        note_dto::{NoteKindDto, NoteTreeItemDto},
        CoreApi,
    },
    testing::InMemoryTaskRepository,
};
use todo_desktop::{import, state::SystemClock};

fn api() -> CoreApi {
    CoreApi::new(
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(SystemClock),
    )
}

async fn items(api: &CoreApi, folder: Option<String>) -> Vec<NoteTreeItemDto> {
    api.list_note_items(
        folder,
        PageRequestDto {
            cursor: None,
            limit: 500,
        },
    )
    .await
    .unwrap()
    .items
}

fn names(items: &[NoteTreeItemDto]) -> Vec<String> {
    items
        .iter()
        .map(|i| match i {
            NoteTreeItemDto::Folder(f) => format!("F:{}", f.name),
            NoteTreeItemDto::Note(n) => format!("N:{}", n.name),
        })
        .collect()
}

#[tokio::test]
async fn imports_a_vault_and_reports_what_was_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Vault");
    fs::create_dir_all(root.join("Uni/Analysis")).unwrap();
    fs::create_dir_all(root.join(".obsidian")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join("Index.md"), "\u{feff}# Start\r\nZeile 2\r\n").unwrap();
    fs::write(root.join("Uni/Plan.txt"), "Montag\rDienstag").unwrap();
    fs::write(root.join("Uni/Analysis/Blatt 3.markdown"), "Aufgabe 1").unwrap();
    fs::write(root.join(".obsidian/app.json"), "{}").unwrap();
    fs::write(root.join(".notiz.md"), "versteckt").unwrap();
    fs::write(root.join("bild.png"), [0x89, 0x50]).unwrap();
    fs::write(root.join("latin1.txt"), [0x66, 0xfc, 0x72]).unwrap();
    fs::write(root.join("riesig.md"), "x".repeat(1024 * 1024 + 1)).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("Index.md"), root.join("link.md")).unwrap();

    let api = api();
    let report = import::import_path(&api, root.clone()).await.unwrap();
    assert_eq!((report.notes, report.folders), (3, 2));
    let mut skipped: Vec<(String, String)> = report
        .skipped
        .iter()
        .map(|s| (s.path.clone(), s.reason.clone()))
        .collect();
    skipped.sort();
    let mut expected = vec![
        (".git".to_owned(), "versteckt".to_owned()),
        (".notiz.md".to_owned(), "versteckt".to_owned()),
        (".obsidian".to_owned(), "versteckt".to_owned()),
        ("bild.png".to_owned(), "andere Endung".to_owned()),
        ("latin1.txt".to_owned(), "kein UTF-8".to_owned()),
        ("riesig.md".to_owned(), "größer als 1 MiB".to_owned()),
    ];
    if cfg!(unix) {
        expected.push((
            "link.md".to_owned(),
            "Verknüpfung (nicht gefolgt)".to_owned(),
        ));
    }
    expected.sort();
    assert_eq!(skipped, expected);

    // Neuer oberster Ordner „Vault" mit dem Inhalt in Dateimanager-Reihenfolge.
    let top = items(&api, None).await;
    assert_eq!(names(&top), ["F:Vault"]);
    let NoteTreeItemDto::Folder(vault) = &top[0] else {
        panic!()
    };
    assert_eq!(report.folder_id, vault.id);
    let inside = items(&api, Some(vault.id.clone())).await;
    assert_eq!(names(&inside), ["N:Index", "F:Uni"]);
    let NoteTreeItemDto::Note(index) = &inside[0] else {
        panic!()
    };
    let index = api.get_note(index.id.clone()).await.unwrap();
    assert_eq!(index.body, "# Start\nZeile 2\n", "BOM weg, LF");
    assert_eq!(index.kind, NoteKindDto::Markdown);
    assert!(!index.auto_name, "Name = Dateiname");
    let NoteTreeItemDto::Folder(uni) = &inside[1] else {
        panic!()
    };
    let uni_items = items(&api, Some(uni.id.clone())).await;
    assert_eq!(names(&uni_items), ["F:Analysis", "N:Plan"]);
    let NoteTreeItemDto::Note(plan) = &uni_items[1] else {
        panic!()
    };
    let plan = api.get_note(plan.id.clone()).await.unwrap();
    assert_eq!(
        (plan.body.as_str(), plan.kind),
        ("Montag\nDienstag", NoteKindDto::Text)
    );

    // Der Quellordner bleibt unverändert.
    assert_eq!(
        fs::read(root.join("Index.md")).unwrap()[..3],
        [0xef, 0xbb, 0xbf]
    );
}

#[test]
fn deep_folders_stop_at_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let mut path = dir.path().join("Tief");
    for i in 0..33 {
        path = path.join(format!("e{i}"));
    }
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("ganz unten.md"), "x").unwrap();
    let scan = import::scan(&dir.path().join("Tief"));
    assert!(scan.files.is_empty());
    assert_eq!(scan.skipped.len(), 1);
    assert_eq!(scan.skipped[0].reason, "zu tief verschachtelt");
}
