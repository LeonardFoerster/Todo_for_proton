//! Wiederverwendbare Contract-Tests für jede [`RemoteStore`]-Implementierung.
//!
//! ```ignore
//! mod contract {
//!     todo_core::remote_store_contract_tests!(my_crate::MyStore::new());
//! }
//! ```

use futures::executor::block_on;

use crate::{
    ports::{RemoteDir, RemotePath, RemoteStore, Revision},
    CoreError,
};

/// Erzeugt je Contract-Fall einen `#[test]` mit frischem Store aus `$make`.
#[macro_export]
macro_rules! remote_store_contract_tests {
    ($make:expr) => {
        $crate::remote_store_contract_tests!(@cases $make;
            open_app_root_is_idempotent,
            create_then_read,
            create_never_overwrites,
            read_missing_is_not_found,
            list_is_recursive_sorted_and_scoped,
            replace_checks_expected_revision,
            delete_is_idempotent,
            changes_feed_if_supported
        );
    };
    (@cases $make:expr; $($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() {
                $crate::testing::remote_store_contract::$case(&$make);
            }
        )*
    };
}

fn root<S: RemoteStore + ?Sized>(store: &S) -> RemoteDir {
    block_on(store.open_app_root("Apps/Tasks")).expect("open_app_root")
}

fn at(root: &RemoteDir, rel: &str) -> RemotePath {
    root.join(rel).expect("path")
}

pub fn open_app_root_is_idempotent<S: RemoteStore + ?Sized>(store: &S) {
    let a = root(store);
    block_on(store.create(&at(&a, "manifest.json"), b"{}".to_vec())).expect("create");
    let b = root(store);
    assert_eq!(a, b);
    assert_eq!(
        block_on(store.read(&at(&b, "manifest.json"))).expect("read"),
        b"{}"
    );
}

pub fn create_then_read<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let path = at(&root, "ops/dev/1-3.seg");
    let entry = block_on(store.create(&path, vec![1, 2, 3])).expect("create");
    assert_eq!(entry.path, path);
    assert_eq!(entry.size, 3);
    assert_eq!(block_on(store.read(&path)).expect("read"), [1, 2, 3]);
}

/// `create` ist die Grundlage für „jedes Gerät schreibt nur eigene, unveränderliche Dateien".
pub fn create_never_overwrites<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let path = at(&root, "ops/dev/1-1.seg");
    block_on(store.create(&path, b"first".to_vec())).expect("create");
    let second = block_on(store.create(&path, b"second".to_vec()));
    assert!(matches!(second, Err(CoreError::Conflict(_))), "{second:?}");
    assert_eq!(block_on(store.read(&path)).expect("read"), b"first");
}

pub fn read_missing_is_not_found<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let result = block_on(store.read(&at(&root, "nope")));
    assert!(matches!(result, Err(CoreError::NotFound(_))), "{result:?}");
}

pub fn list_is_recursive_sorted_and_scoped<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    for rel in [
        "ops/b/1-1.seg",
        "ops/a/2-2.seg",
        "ops/a/1-1.seg",
        "devices/a.json",
    ] {
        block_on(store.create(&at(&root, rel), vec![0])).expect("create");
    }
    let ops = at(&root, "ops");
    let listing = block_on(store.list(&ops, None)).expect("list");
    let rel: Vec<&str> = listing
        .entries
        .iter()
        .map(|e| e.path.relative_to(&ops).expect("inside"))
        .collect();
    assert_eq!(rel, ["a/1-1.seg", "a/2-2.seg", "b/1-1.seg"]);
    let empty = block_on(store.list(&at(&root, "snapshots"), None)).expect("list empty");
    assert!(empty.entries.is_empty());
}

pub fn replace_checks_expected_revision<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let path = at(&root, "devices/a.json");
    let first = block_on(store.replace(&path, b"1".to_vec(), None)).expect("create via replace");
    let second =
        block_on(store.replace(&path, b"2".to_vec(), Some(&first.revision))).expect("replace");
    assert_ne!(first.revision, second.revision);
    let stale = block_on(store.replace(&path, b"3".to_vec(), Some(&first.revision)));
    assert!(matches!(stale, Err(CoreError::Conflict(_))), "{stale:?}");
    let missing = block_on(store.replace(
        &at(&root, "devices/b.json"),
        b"x".to_vec(),
        Some(&Revision("1".into())),
    ));
    assert!(
        matches!(missing, Err(CoreError::Conflict(_))),
        "{missing:?}"
    );
    assert_eq!(block_on(store.read(&path)).expect("read"), b"2");
}

pub fn delete_is_idempotent<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let path = at(&root, "ops/dev/1-1.seg");
    block_on(store.create(&path, vec![1])).expect("create");
    block_on(store.delete(&path)).expect("delete");
    block_on(store.delete(&path)).expect("delete again");
    assert!(matches!(
        block_on(store.read(&path)),
        Err(CoreError::NotFound(_))
    ));
    block_on(store.create(&path, vec![2])).expect("create after delete");
}

/// Unterstützt der Store einen Feed, meldet er Anlegen und Löschen.
pub fn changes_feed_if_supported<S: RemoteStore + ?Sized>(store: &S) {
    let root = root(store);
    let Some(start) = block_on(store.changes_since(None)).expect("feed") else {
        return;
    };
    assert!(start.changed.is_empty() && start.deleted.is_empty());
    let a = at(&root, "ops/dev/1-1.seg");
    block_on(store.create(&a, vec![1])).expect("create");
    let changes = block_on(store.changes_since(Some(&start.cursor)))
        .expect("feed")
        .expect("still supported");
    assert!(changes.changed.iter().any(|e| e.path == a));
    block_on(store.delete(&a)).expect("delete");
    let after = block_on(store.changes_since(Some(&changes.cursor)))
        .expect("feed")
        .expect("still supported");
    assert!(after.deleted.contains(&a));
}
