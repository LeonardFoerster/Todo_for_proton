//! Live-Test gegen das echte Proton-Testkonto (P6-03). Läuft nur manuell:
//!
//! ```text
//! PROTON_LIVE=1 cargo test -p proton-adapter --test live_auth -- --ignored --nocapture
//! ```
//!
//! Login im Browser (Session-Fork); die App sieht kein Passwort. Ausgabe ohne Geheimnisse und
//! ohne Kontodaten. Sitzung nur im Speicher (InMemoryKeyStore), am Ende abgemeldet.

use std::{process::Command, sync::Arc, time::Duration};

use proton_adapter::{ApiConfig, ProtonAuthProvider};
use todo_core::{
    ports::{AuthError, AuthProvider, AuthStep, KeyStore},
    testing::InMemoryKeyStore,
    CoreError,
};

const APP_VERSION: &str = "external-drive-tasks@0.1.0-alpha";
const STORE_KEY: &str = "proton-session";

fn enabled() -> bool {
    std::env::var("PROTON_LIVE").is_ok_and(|v| v == "1")
}

#[tokio::test]
#[ignore = "live: braucht PROTON_LIVE=1 und einen Browser-Login"]
async fn fork_login_resume_logout() {
    if !enabled() {
        eprintln!("PROTON_LIVE=1 nicht gesetzt – übersprungen");
        return;
    }
    println!("This is a third-party application not officially supported by Proton.");
    let store = Arc::new(InMemoryKeyStore::new());
    let auth = ProtonAuthProvider::new(ApiConfig::new(APP_VERSION), store.clone()).unwrap();

    let AuthStep::OpenBrowser {
        flow,
        url,
        poll_interval_s,
        expires_in_s,
    } = auth.begin_login().await.expect("begin_login")
    else {
        panic!("expected OpenBrowser");
    };
    println!("[1] Im Browser mit dem TESTKONTO bestätigen (öffnet sich gleich):\n    {url}");
    let _ = Command::new("xdg-open").arg(&url).spawn();

    let started = std::time::Instant::now();
    let mut pending = 0;
    let info = loop {
        tokio::time::sleep(Duration::from_secs(u64::from(poll_interval_s))).await;
        assert!(
            started.elapsed().as_secs() < u64::from(expires_in_s),
            "Zeitüberschreitung"
        );
        match auth.poll_login(&flow).await.expect("poll_login") {
            AuthStep::Pending { .. } => pending += 1,
            AuthStep::Done(info) => break info,
            AuthStep::OpenBrowser { .. } => panic!("unexpected OpenBrowser"),
        }
    };
    println!(
        "[2] Login OK nach {} s ({pending}× Pending); Label vorhanden: {}",
        started.elapsed().as_secs(),
        !info.account_label.is_empty()
    );

    // Neuer „Prozess" mit demselben KeyStore: resume = Refresh (Token-Rotation).
    let restarted = ProtonAuthProvider::new(ApiConfig::new(APP_VERSION), store.clone()).unwrap();
    let t = std::time::Instant::now();
    let resumed = restarted.resume().await.expect("resume");
    assert_eq!(resumed.as_ref(), Some(&info));
    println!("[3] resume (Refresh) OK in {} ms", t.elapsed().as_millis());
    let before_logout = store.get(STORE_KEY).await.unwrap().expect("stored");

    restarted.logout().await.expect("logout");
    assert_eq!(store.get(STORE_KEY).await.unwrap(), None);
    println!("[4] logout OK, lokale Sitzung gelöscht");

    // Alte (inzwischen widerrufene) Sitzungsdaten: Server muss ablehnen.
    let stale_store = Arc::new(InMemoryKeyStore::new());
    stale_store.put(STORE_KEY, before_logout).await.unwrap();
    let stale = ProtonAuthProvider::new(ApiConfig::new(APP_VERSION), stale_store).unwrap();
    assert_eq!(
        stale.resume().await,
        Err(CoreError::Auth(AuthError::SessionRevoked))
    );
    println!("[5] widerrufene Sitzung → SessionRevoked (gut)");
    println!("ERGEBNIS: OK");
}
