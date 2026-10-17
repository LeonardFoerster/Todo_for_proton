//! Mock-Server-Tests für jeden Login-Übergang, Refresh und Logout.

use std::{sync::Arc, time::Duration};

use serde_json::{json, Value};
use todo_core::{
    ports::{AuthError, AuthProvider, AuthStep, KeyStore, SecretBytes, SessionInfo},
    testing::InMemoryKeyStore,
    CoreError,
};
use wiremock::{
    matchers::{body_partial_json, header, header_exists, method, path},
    Mock, MockServer, Request as WireRequest, ResponseTemplate,
};

use super::{fork, session::STORE_KEY, ProtonAuthProvider};
use crate::{
    api_client::{
        tests::{fixture, APP_VERSION},
        ApiConfig, Request,
    },
    error::code,
};

struct Harness {
    server: MockServer,
    store: Arc<InMemoryKeyStore>,
    auth: ProtonAuthProvider,
}

async fn harness() -> Harness {
    let server = MockServer::start().await;
    let store = Arc::new(InMemoryKeyStore::new());
    let auth = provider(&server, &store);
    Harness {
        server,
        store,
        auth,
    }
}

fn provider(server: &MockServer, store: &Arc<InMemoryKeyStore>) -> ProtonAuthProvider {
    let mut config = ApiConfig::new(APP_VERSION);
    config.base_url = server.uri();
    config.timeout = Duration::from_secs(2);
    ProtonAuthProvider::new(config, store.clone()).expect("provider")
}

fn ok(body: Value) -> ResponseTemplate {
    let mut body = body;
    body["Code"] = json!(code::OK);
    ResponseTemplate::new(200).set_body_json(body)
}

async fn mount_fork_init(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/auth/v4/sessions/forks"))
        .respond_with(ok(json!({ "Selector": "sel-1", "UserCode": "UC12" })))
        .mount(server)
        .await;
}

async fn mount_users(server: &MockServer, token: &str) {
    Mock::given(method("GET"))
        .and(path("/core/v4/users"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("users")))
        .mount(server)
        .await;
}

fn fork_done(url: &str, access: &str, refresh: &str) -> ResponseTemplate {
    let key = fork::key_from_url(url);
    ok(json!({
        "UID": "uid-1",
        "AccessToken": access,
        "RefreshToken": refresh,
        "Payload": fork::encrypt_payload(&key, r#"{"type":"default","keyPassword":"kp-1"}"#),
        "UserID": "user-id-1",
        "LocalID": 0,
        "Scopes": ["full", "drive"],
        "TokenType": "Bearer",
    }))
}

/// Login bis `Done` mit Access-Token `acc-1`, Refresh-Token `ref-1`.
async fn login(h: &Harness) -> SessionInfo {
    mount_fork_init(&h.server).await;
    let AuthStep::OpenBrowser { flow, url, .. } = h.auth.begin_login().await.unwrap() else {
        panic!("expected OpenBrowser");
    };
    Mock::given(path("/auth/v4/sessions/forks/sel-1"))
        .respond_with(fork_done(&url, "acc-1", "ref-1"))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    mount_users(&h.server, "acc-1").await;
    match h.auth.poll_login(&flow).await.unwrap() {
        AuthStep::Done(info) => info,
        other => panic!("expected Done, got {other:?}"),
    }
}

async fn stored(store: &InMemoryKeyStore) -> Option<Value> {
    store
        .get(STORE_KEY)
        .await
        .unwrap()
        .map(|bytes| serde_json::from_slice(bytes.expose()).unwrap())
}

fn refresh_ok(access: &str, refresh: &str) -> ResponseTemplate {
    ok(
        json!({ "UID": "uid-1", "AccessToken": access, "RefreshToken": refresh, "RefreshCounter": 1 }),
    )
}

#[tokio::test]
async fn begin_login_returns_browser_step_with_fork_url() {
    let h = harness().await;
    mount_fork_init(&h.server).await;
    let step = h.auth.begin_login().await.unwrap();
    let AuthStep::OpenBrowser {
        flow,
        url,
        poll_interval_s,
        expires_in_s,
    } = step
    else {
        panic!("{step:?}");
    };
    assert!(
        url.starts_with("https://account.proton.me/desktop/login?"),
        "{url}"
    );
    assert!(url.contains("UC12"));
    assert!(!url.contains("sel-1"), "Selector bleibt im Adapter");
    assert!(!flow.0.contains("sel-1"));
    assert_eq!((poll_interval_s, expires_in_s), (5, 600));
    assert_eq!(h.auth.current_session().await.unwrap(), None);
}

#[tokio::test]
async fn pending_then_done_stores_session_without_access_token() {
    let h = harness().await;
    mount_fork_init(&h.server).await;
    let AuthStep::OpenBrowser { flow, url, .. } = h.auth.begin_login().await.unwrap() else {
        panic!();
    };
    Mock::given(path("/auth/v4/sessions/forks/sel-1"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({ "Code": 9001, "Error": "not confirmed" })),
        )
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    assert_eq!(
        h.auth.poll_login(&flow).await.unwrap(),
        AuthStep::Pending { flow: flow.clone() }
    );

    Mock::given(path("/auth/v4/sessions/forks/sel-1"))
        .respond_with(fork_done(&url, "acc-1", "ref-1"))
        .mount(&h.server)
        .await;
    mount_users(&h.server, "acc-1").await;
    let expected = SessionInfo {
        account_label: "testuser@example.org".into(),
        account_id: "user-id-1".into(),
    };
    assert_eq!(
        h.auth.poll_login(&flow).await.unwrap(),
        AuthStep::Done(expected.clone())
    );
    assert_eq!(h.auth.current_session().await.unwrap(), Some(expected));

    let saved = stored(&h.store).await.expect("persisted");
    assert_eq!(saved["uid"], "uid-1");
    assert_eq!(saved["refresh_token"], "ref-1");
    assert_eq!(saved["key_password"], "kp-1");
    assert!(
        !saved.to_string().contains("acc-1"),
        "Access-Token nur im Speicher"
    );

    // Der Vorgang ist abgeschlossen.
    assert_eq!(
        h.auth.poll_login(&flow).await,
        Err(CoreError::Auth(AuthError::Cancelled))
    );
}

#[tokio::test]
async fn login_window_expires() {
    let mut h = harness().await;
    h.auth.set_login_expiry(Duration::ZERO);
    mount_fork_init(&h.server).await;
    let AuthStep::OpenBrowser { flow, .. } = h.auth.begin_login().await.unwrap() else {
        panic!();
    };
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(
        h.auth.poll_login(&flow).await,
        Err(CoreError::Auth(AuthError::Expired))
    );
}

#[tokio::test]
async fn cancel_discards_selector_and_flow() {
    let h = harness().await;
    mount_fork_init(&h.server).await;
    Mock::given(method("DELETE"))
        .and(path("/auth/v4/sessions/forks/sel-1"))
        .respond_with(ok(json!({})))
        .expect(1)
        .mount(&h.server)
        .await;
    let AuthStep::OpenBrowser { flow, .. } = h.auth.begin_login().await.unwrap() else {
        panic!();
    };
    h.auth.cancel_login(&flow).await.unwrap();
    h.auth.cancel_login(&flow).await.unwrap();
    assert_eq!(
        h.auth.poll_login(&flow).await,
        Err(CoreError::Auth(AuthError::Cancelled))
    );
}

#[tokio::test]
async fn broken_payload_fails_login_and_stores_nothing() {
    let h = harness().await;
    mount_fork_init(&h.server).await;
    let AuthStep::OpenBrowser { flow, .. } = h.auth.begin_login().await.unwrap() else {
        panic!();
    };
    // Mit falschem Schlüssel verschlüsselt.
    let payload = fork::encrypt_payload(&[9; 32], r#"{"keyPassword":"x"}"#);
    Mock::given(path("/auth/v4/sessions/forks/sel-1"))
        .respond_with(ok(json!({
            "UID": "u", "AccessToken": "a", "RefreshToken": "r", "Payload": payload,
        })))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.auth.poll_login(&flow).await,
        Err(CoreError::RemoteRejected(_))
    ));
    assert_eq!(stored(&h.store).await, None);
}

#[tokio::test]
async fn rejected_app_version_is_client_rejected() {
    let h = harness().await;
    Mock::given(path("/auth/v4/sessions/forks"))
        .respond_with(ResponseTemplate::new(400).set_body_json(fixture("missing_app_version")))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.auth.begin_login().await,
        Err(CoreError::Auth(AuthError::ClientRejected(_)))
    ));
}

#[tokio::test]
async fn resume_refreshes_and_persists_rotated_token() {
    let h = harness().await;
    login(&h).await;

    // Neuer Prozess: gleicher KeyStore, kein Access-Token im Speicher.
    let restarted = provider(&h.server, &h.store);
    Mock::given(method("POST"))
        .and(path("/auth/v4/refresh"))
        .and(header("x-pm-uid", "uid-1"))
        .and(body_partial_json(json!({
            "RefreshToken": "ref-1", "GrantType": "refresh_token", "ResponseType": "token",
        })))
        .respond_with(refresh_ok("acc-2", "ref-2"))
        .expect(1)
        .mount(&h.server)
        .await;
    let info = restarted.resume().await.unwrap().expect("session");
    assert_eq!(info.account_label, "testuser@example.org");
    assert_eq!(stored(&h.store).await.unwrap()["refresh_token"], "ref-2");
    // Bereits erneuert: kein zweiter Refresh.
    restarted.resume().await.unwrap();
}

#[tokio::test]
async fn resume_without_stored_session_is_none() {
    let h = harness().await;
    assert_eq!(h.auth.resume().await.unwrap(), None);
}

#[tokio::test]
async fn resume_offline_keeps_session() {
    let h = harness().await;
    login(&h).await;
    let mut config = ApiConfig::new(APP_VERSION);
    config.base_url = "http://127.0.0.1:9".into();
    let offline = ProtonAuthProvider::new(config, h.store.clone()).unwrap();
    let info = offline.resume().await.unwrap();
    assert_eq!(info.map(|i| i.account_id), Some("user-id-1".into()));
    assert!(stored(&h.store).await.is_some());
}

#[tokio::test]
async fn resume_with_revoked_session_deletes_it() {
    let h = harness().await;
    login(&h).await;
    let restarted = provider(&h.server, &h.store);
    Mock::given(path("/auth/v4/refresh"))
        .respond_with(ResponseTemplate::new(422).set_body_json(fixture("invalid_refresh_token")))
        .mount(&h.server)
        .await;
    assert_eq!(
        restarted.resume().await,
        Err(CoreError::Auth(AuthError::SessionRevoked))
    );
    assert_eq!(stored(&h.store).await, None);
    assert_eq!(restarted.current_session().await.unwrap(), None);
    assert_eq!(restarted.resume().await.unwrap(), None);
}

#[tokio::test]
async fn unauthorized_call_refreshes_once_and_retries() {
    let h = harness().await;
    login(&h).await;
    let session = h.auth.session();
    // acc-1 gilt nicht mehr, acc-2 schon.
    Mock::given(path("/drive/volumes"))
        .and(header("authorization", "Bearer acc-1"))
        .respond_with(ResponseTemplate::new(401).set_body_json(fixture("invalid_access_token")))
        .mount(&h.server)
        .await;
    Mock::given(path("/drive/volumes"))
        .and(header("authorization", "Bearer acc-2"))
        .respond_with(ok(json!({ "Volumes": [] })))
        .mount(&h.server)
        .await;
    Mock::given(path("/auth/v4/refresh"))
        .respond_with(refresh_ok("acc-2", "ref-2"))
        .expect(1)
        .mount(&h.server)
        .await;
    // Mehrere parallele Aufrufe mit abgelaufenem Token → genau ein Refresh.
    let calls = (0..4).map(|_| session.send(Request::get("/drive/volumes", "/drive/volumes")));
    for result in futures::future::join_all(calls).await {
        assert_eq!(result.unwrap()["Volumes"], json!([]));
    }
    assert_eq!(stored(&h.store).await.unwrap()["refresh_token"], "ref-2");
}

#[tokio::test]
async fn still_unauthorized_after_refresh_needs_login() {
    let h = harness().await;
    login(&h).await;
    Mock::given(path("/drive/volumes"))
        .respond_with(ResponseTemplate::new(401).set_body_json(fixture("invalid_access_token")))
        .mount(&h.server)
        .await;
    Mock::given(path("/auth/v4/refresh"))
        .respond_with(refresh_ok("acc-2", "ref-2"))
        .expect(1)
        .mount(&h.server)
        .await;
    let result = h
        .auth
        .session()
        .send(Request::get("/drive/volumes", "/drive/volumes"))
        .await;
    assert_eq!(result, Err(CoreError::AuthRequired));
}

#[tokio::test]
async fn call_without_session_needs_login() {
    let h = harness().await;
    let result = h
        .auth
        .session()
        .send(Request::get("/drive/volumes", "/drive/volumes"))
        .await;
    assert_eq!(result, Err(CoreError::AuthRequired));
}

#[tokio::test]
async fn logout_revokes_and_clears_even_if_server_fails() {
    for status in [200, 500] {
        let h = harness().await;
        login(&h).await;
        Mock::given(method("DELETE"))
            .and(path("/auth/v4"))
            .and(header("authorization", "Bearer acc-1"))
            .and(header_exists("x-pm-uid"))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({ "Code": 1000 })))
            .expect(1)
            .mount(&h.server)
            .await;
        h.auth.logout().await.unwrap();
        assert_eq!(h.auth.current_session().await.unwrap(), None);
        assert_eq!(stored(&h.store).await, None);
        h.auth.logout().await.unwrap();
    }
}

#[tokio::test]
async fn unreadable_stored_session_is_ignored() {
    let h = harness().await;
    h.store
        .put(STORE_KEY, SecretBytes::new(b"not json".to_vec()))
        .await
        .unwrap();
    assert_eq!(h.auth.current_session().await.unwrap(), None);
}

#[tokio::test]
async fn requests_carry_app_version() {
    let h = harness().await;
    Mock::given(path("/auth/v4/sessions/forks"))
        .and(header("x-pm-appversion", APP_VERSION))
        .respond_with(|_: &WireRequest| ok(json!({ "Selector": "s", "UserCode": "u" })))
        .expect(1)
        .mount(&h.server)
        .await;
    h.auth.begin_login().await.unwrap();
}
