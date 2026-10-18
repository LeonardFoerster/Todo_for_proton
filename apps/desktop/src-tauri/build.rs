//! Erzeugt Tauri-Kontext und ACL. Jedes Command muss hier stehen, sonst lehnt die Capability es ab.

const COMMANDS: &[&str] = &[
    "ensure_inbox",
    "list_task_lists",
    "create_task_list",
    "rename_task_list",
    "delete_task_list",
    "create_task",
    "get_task",
    "update_task",
    "complete_task",
    "move_task",
    "delete_task",
    "complete_tasks",
    "delete_tasks",
    "move_tasks",
    "restore_tasks",
    "list_tasks",
    "auth_info",
    "auth_current",
    "auth_begin_login",
    "auth_open_browser",
    "auth_poll_login",
    "auth_cancel_login",
    "auth_logout",
    "sync_status",
    "sync_now",
    "set_online",
    "app_ready",
    "dev_selftest",
    "dev_selftest_report",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build fehlgeschlagen");
}
