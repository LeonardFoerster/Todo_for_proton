//! Ruft das echte Binary auf: Jeder Befehl ist ein eigener Prozess, die Daten müssen also über
//! die Datenbankdatei erhalten bleiben.

use std::{path::Path, process::Command};

fn todo(db: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_todo-cli"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("TODO_DB")
        .output()
        .expect("todo-cli startet");
    let text = String::from_utf8(if out.status.success() {
        out.stdout
    } else {
        out.stderr
    })
    .expect("utf-8");
    (out.status.success(), text)
}

fn ok(db: &Path, args: &[&str]) -> String {
    let (success, text) = todo(db, args);
    assert!(success, "{args:?}: {text}");
    text
}

/// Kurz-ID (drittes Wort) der Zeile mit `title` aus `ls`.
fn short_id(listing: &str, title: &str) -> String {
    listing
        .lines()
        .find(|l| l.contains(title))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap_or_else(|| panic!("{title} nicht in {listing}"))
        .to_owned()
}

#[test]
fn add_list_complete_move_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cli.sqlite3");

    assert!(ok(&db, &["lists"]).contains("(Inbox)"));
    ok(&db, &["list-add", "Arbeit"]);
    ok(&db, &["add", "Milch", "kaufen"]);
    ok(&db, &["add", "Brot"]);
    ok(&db, &["add", "Bericht", "-l", "arbeit"]);

    let inbox = ok(&db, &["ls"]);
    let milk = short_id(&inbox, "Milch kaufen");
    let bread = short_id(&inbox, "Brot");
    ok(&db, &["done", &milk]);
    ok(&db, &["mv", &bread, "--before", &milk]);

    assert_eq!(
        ok(&db, &["ls"]),
        format!("[ ] {bread}  Brot\n[x] {milk}  Milch kaufen\n")
    );
    assert_eq!(ok(&db, &["ls", "--open"]), format!("[ ] {bread}  Brot\n"));
    assert!(ok(&db, &["ls", "Arbeit"]).contains("Bericht"));
    assert!(ok(&db, &["ls", "done"]).contains("Milch kaufen"));

    ok(&db, &["rm", &bread]);
    assert!(!ok(&db, &["ls", "all"]).contains("Brot"));
}

#[test]
fn errors_exit_non_zero() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cli.sqlite3");
    let (success, text) = todo(&db, &["done", "ffff"]);
    assert!(!success);
    assert!(text.contains("keine Task"), "{text}");
    let (success, text) = todo(&db, &["add", "   "]);
    assert!(!success);
    assert!(text.contains("validation"), "{text}");
    assert!(!todo(&db, &["frobnicate"]).0);
}
