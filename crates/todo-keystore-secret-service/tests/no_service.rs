//! Ohne erreichbaren Secret Service: sofortiger, verständlicher Fehler statt Hängen.
//! Eigene Testdatei = eigener Prozess, daher darf der Test die Bus-Adresse verbiegen.
#![cfg(target_os = "linux")]

use todo_core::CoreError;
use todo_keystore_secret_service::SecretServiceKeyStore;

#[tokio::test]
async fn missing_service_gives_actionable_error() {
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/bus");
    let err = SecretServiceKeyStore::connect("dev.tasks.keystore-test", "Tasks (Test)")
        .await
        .expect_err("no bus");
    let CoreError::Storage(msg) = err else {
        panic!("expected Storage, got {err:?}");
    };
    assert!(msg.contains("system keyring not reachable"), "{msg}");
    assert!(msg.contains("org.freedesktop.secrets"), "{msg}");
}
