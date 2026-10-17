//! Manueller Test gegen den laufenden Secret Service des Nutzers (ändert nur eigene Testeinträge):
//! `cargo test -p todo-keystore-secret-service --test live -- --ignored`
#![cfg(target_os = "linux")]

use todo_core::ports::{KeyStore, SecretBytes};
use todo_keystore_secret_service::SecretServiceKeyStore;

const APP_ID: &str = "dev.tasks.keystore-test";

#[tokio::test]
#[ignore = "braucht einen laufenden Secret Service (GNOME Keyring/KWallet)"]
async fn put_get_overwrite_delete() {
    let store = SecretServiceKeyStore::connect(APP_ID, "Tasks (Test)")
        .await
        .expect("connect");
    let name = format!("probe-{}", std::process::id());
    assert_eq!(store.get(&name).await.expect("get missing"), None);

    let first = SecretBytes::new(vec![0, 1, 2, 255]);
    store.put(&name, first.clone()).await.expect("put");
    assert_eq!(store.get(&name).await.expect("get"), Some(first));

    let second = SecretBytes::new(b"zweiter Wert".to_vec());
    store.put(&name, second.clone()).await.expect("overwrite");
    assert_eq!(store.get(&name).await.expect("get"), Some(second));

    // Getrennte Namensräume je App-Kennung.
    let other = SecretServiceKeyStore::connect("dev.tasks.keystore-test-other", "Tasks (Test)")
        .await
        .expect("connect other");
    assert_eq!(other.get(&name).await.expect("get other"), None);

    store.delete(&name).await.expect("delete");
    store.delete(&name).await.expect("delete again");
    assert_eq!(store.get(&name).await.expect("get deleted"), None);
}
