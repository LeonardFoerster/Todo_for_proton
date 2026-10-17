//! Commands über die echte IPC-Schicht (MockRuntime): Namen, Argumente (camelCase) und
//! Fehlerform so, wie die UI sie per `invoke` sieht.

use serde_json::{json, Value};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    App, WebviewWindow, WebviewWindowBuilder,
};
use todo_desktop::state::Options;

struct Harness {
    _app: App<MockRuntime>,
    window: WebviewWindow<MockRuntime>,
    _dir: tempfile::TempDir,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let options = Options {
        data_dir: Some(dir.path().to_owned()),
        accounts: false,
    };
    let app = todo_desktop::builder(mock_builder(), options.clone())
        .build(mock_context(noop_assets()))
        .unwrap();
    // `setup` läuft erst mit der Event-Loop; hier direkt.
    todo_desktop::manage_state(app.handle(), &options).unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Harness {
        _app: app,
        window,
        _dir: dir,
    }
}

impl Harness {
    fn invoke(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        get_ipc_response(
            &self.window,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.invoke(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }
}

#[test]
fn task_lifecycle_through_ipc() {
    let h = harness();
    let inbox = h.ok("ensure_inbox", json!({}));
    assert_eq!(inbox["isInbox"], true);

    let a = h.ok(
        "create_task",
        json!({ "input": { "title": "Milch kaufen" } }),
    );
    let b = h.ok(
        "create_task",
        json!({ "input": { "title": "Zahnarzt", "listId": inbox["id"] } }),
    );
    assert_eq!(a["listId"], inbox["id"]);

    let patched = h.ok(
        "update_task",
        json!({ "id": a["id"], "patch": { "notes": "2 l", "due": { "kind": "date", "date": "2026-10-20" } } }),
    );
    assert_eq!(patched["notes"], "2 l");
    assert_eq!(patched["due"]["date"], "2026-10-20");

    let done = h.ok("complete_task", json!({ "id": a["id"], "done": true }));
    assert_eq!(done["status"], "done");

    h.ok(
        "move_task",
        json!({ "id": b["id"], "target": { "before": a["id"] } }),
    );
    let page = h.ok(
        "list_tasks",
        json!({ "query": { "view": { "kind": "inbox" } }, "page": { "limit": 50 } }),
    );
    let titles: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Zahnarzt", "Milch kaufen"]);

    h.ok("delete_task", json!({ "id": b["id"] }));
    let err = h.invoke("get_task", json!({ "id": b["id"] })).unwrap_err();
    assert_eq!(err["kind"], "notFound");
}

#[test]
fn lists_and_validation_errors() {
    let h = harness();
    h.ok("ensure_inbox", json!({}));
    let list = h.ok("create_task_list", json!({ "name": "Arbeit" }));
    let renamed = h.ok(
        "rename_task_list",
        json!({ "id": list["id"], "name": "Büro" }),
    );
    assert_eq!(renamed["name"], "Büro");
    assert_eq!(
        h.ok("list_task_lists", json!({})).as_array().unwrap().len(),
        2
    );
    assert_eq!(h.ok("delete_task_list", json!({ "id": list["id"] })), 0);

    let err = h
        .invoke("create_task", json!({ "input": { "title": "   " } }))
        .unwrap_err();
    assert_eq!(err["kind"], "validation");
}

#[test]
fn sync_and_auth_without_keyring() {
    let h = harness();
    let status = h.ok("sync_status", json!({}));
    assert_eq!(status["state"], "disabled");
    h.ok("sync_now", json!({}));
    h.ok("set_online", json!({ "online": false }));
    h.ok("app_ready", json!({}));
    let info = h.ok("auth_info", json!({}));
    assert_eq!(info["available"], false);
    assert!(info["notice"].as_str().unwrap().contains("third-party"));
    assert_eq!(h.ok("auth_current", json!({})), Value::Null);
    let err = h.invoke("auth_begin_login", json!({})).unwrap_err();
    assert_eq!(err["kind"], "unavailable");
}
