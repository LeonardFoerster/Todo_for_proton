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
    app: App<MockRuntime>,
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
        app,
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
fn reorders_lists_with_the_inbox_first() {
    let h = harness();
    let inbox = h.ok("ensure_inbox", json!({}));
    let a = h.ok("create_task_list", json!({ "name": "A" }));
    let b = h.ok("create_task_list", json!({ "name": "B" }));
    let moved = h.ok(
        "move_task_list",
        json!({ "id": b["id"], "target": { "before": a["id"] } }),
    );
    assert_eq!(moved["name"], "B");
    let names: Vec<String> = h
        .ok("list_task_lists", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Inbox", "B", "A"]);
    let err = h
        .invoke("move_task_list", json!({ "id": inbox["id"], "target": {} }))
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

#[test]
fn bulk_commands_through_ipc() {
    let h = harness();
    h.ok("ensure_inbox", json!({}));
    let list = h.ok("create_task_list", json!({ "name": "Arbeit" }));
    let ids: Vec<Value> = ["a", "b", "c"]
        .iter()
        .map(|t| h.ok("create_task", json!({ "input": { "title": t } }))["id"].clone())
        .collect();
    let titles = |view: Value| -> Vec<String> {
        h.ok(
            "list_tasks",
            json!({ "query": { "view": view }, "page": { "limit": 50 } }),
        )["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_owned())
            .collect()
    };

    let done = h.ok(
        "complete_tasks",
        json!({ "ids": [ids[0], ids[1], "0190f5c4-0000-7000-8000-000000000000"], "done": true }),
    );
    assert_eq!(
        done.as_array().unwrap().len(),
        2,
        "unbekannte ID übersprungen"
    );

    let moved = h.ok(
        "move_tasks",
        json!({ "ids": [ids[2], ids[0]], "target": { "listId": list["id"] } }),
    );
    assert_eq!(moved.as_array().unwrap().len(), 2);
    assert_eq!(
        titles(json!({ "kind": "list", "listId": list["id"] })),
        ["a", "c"]
    );

    let deleted = h.ok("delete_tasks", json!({ "ids": [ids[0], ids[2]] }));
    assert_eq!(deleted[0]["title"], "a");
    assert!(titles(json!({ "kind": "list", "listId": list["id"] })).is_empty());

    let restored = h.ok("restore_tasks", json!({ "tasks": deleted }));
    assert_ne!(restored[0]["id"], ids[0], "neue ID");
    assert_eq!(restored[0]["status"], "done");
    assert_eq!(
        titles(json!({ "kind": "list", "listId": list["id"] })),
        ["a", "c"]
    );

    let err = h
        .invoke("delete_tasks", json!({ "ids": ["kaputt"] }))
        .unwrap_err();
    assert_eq!(err["kind"], "validation");
}

/// ADR-022: Jedes schreibende Command meldet genau eine lokale Änderung (→ Upload), lesende keine.
#[test]
fn only_writing_commands_trigger_a_sync() {
    use tauri::Manager;
    let h = harness();
    let changes = || {
        h.app
            .state::<todo_desktop::state::AppState>()
            .sync
            .local_changes()
    };
    let expect = |cmd: &str, args: Value, delta: u64| -> Value {
        let before = changes();
        let out = h.ok(cmd, args);
        assert_eq!(changes() - before, delta, "{cmd}");
        out
    };
    let inbox = expect("ensure_inbox", json!({}), 0);
    let list = expect("create_task_list", json!({ "name": "Arbeit" }), 1);
    expect(
        "rename_task_list",
        json!({ "id": list["id"], "name": "Büro" }),
        1,
    );
    let a = expect("create_task", json!({ "input": { "title": "a" } }), 1);
    let b = expect("create_task", json!({ "input": { "title": "b" } }), 1);
    expect(
        "update_task",
        json!({ "id": a["id"], "patch": { "notes": "n" } }),
        1,
    );
    expect("complete_task", json!({ "id": a["id"], "done": true }), 1);
    expect("complete_task", json!({ "id": a["id"], "done": false }), 1);
    expect(
        "move_task",
        json!({ "id": b["id"], "target": { "before": a["id"] } }),
        1,
    );
    expect(
        "complete_tasks",
        json!({ "ids": [a["id"], b["id"]], "done": true }),
        1,
    );
    expect(
        "move_tasks",
        json!({ "ids": [a["id"], b["id"]], "target": { "listId": list["id"] } }),
        1,
    );
    expect(
        "set_dues",
        json!({ "dues": [{ "id": a["id"], "due": { "kind": "date", "date": "2026-10-20" } }, { "id": b["id"], "due": null }] }),
        1,
    );
    let deleted = expect("delete_tasks", json!({ "ids": [a["id"]] }), 1);
    expect("restore_tasks", json!({ "tasks": deleted }), 1);
    expect("delete_task", json!({ "id": b["id"] }), 1);
    expect("delete_task_list", json!({ "id": list["id"] }), 1);

    expect("list_task_lists", json!({}), 0);
    expect(
        "list_tasks",
        json!({ "query": { "view": { "kind": "list", "listId": inbox["id"] } }, "page": { "limit": 10 } }),
        0,
    );
    expect("search_tasks", json!({ "text": "a", "limit": 10 }), 0);
    expect("sync_status", json!({}), 0);
    expect("auth_info", json!({}), 0);
    expect("app_ready", json!({}), 0);
}
