//! Tauri-2-Shell (Linux). Composition Root: verdrahtet Core, Speicher, Sync und Proton und reicht
//! die Fassade `todo_core::api::CoreApi` als dünne `async`-Commands an die UI durch.

mod commands;
mod dev;
pub mod dto;
mod events;
pub mod import;
pub mod state;
mod sync;

use std::{
    env,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use tauri::{Emitter, Manager, Runtime, Window, WindowEvent};
use todo_sync::SchedulerEvent;

use crate::state::{AppState, Options};

/// Alle Commands; die Selbsttest-Commands nur im Debug-Build (im Release-Build wären sie
/// wirkungslos, gehören aber nicht zur Angriffsfläche der WebView).
#[cfg(debug_assertions)]
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            commands::ensure_inbox,
            commands::list_task_lists,
            commands::create_task_list,
            commands::rename_task_list,
            commands::move_task_list,
            commands::delete_task_list,
            commands::create_task,
            commands::get_task,
            commands::update_task,
            commands::complete_task,
            commands::move_task,
            commands::delete_task,
            commands::complete_tasks,
            commands::delete_tasks,
            commands::trash_tasks,
            commands::untrash_tasks,
            commands::purge_tasks,
            commands::empty_task_trash,
            commands::move_tasks,
            commands::restore_tasks,
            commands::set_dues,
            commands::list_tasks,
            commands::search_tasks,
            commands::list_note_folders,
            commands::list_note_items,
            commands::get_note,
            commands::create_note,
            commands::update_note_body,
            commands::discard_empty_note,
            commands::rename_note,
            commands::set_note_kind,
            commands::move_note,
            commands::create_note_folder,
            commands::rename_note_folder,
            commands::move_note_folder,
            commands::trash_note_items,
            commands::restore_note_items,
            commands::purge_note_items,
            commands::empty_note_trash,
            commands::list_note_trash,
            commands::search_notes,
            commands::auth_info,
            commands::auth_current,
            commands::auth_begin_login,
            commands::auth_open_browser,
            commands::auth_poll_login,
            commands::auth_cancel_login,
            commands::auth_logout,
            commands::sync_status,
            commands::sync_now,
            commands::sync_flush,
            commands::close_ready,
            commands::open_external,
            commands::import_notes_folder,
            commands::set_online,
            commands::app_ready,
            commands::dev_selftest,
            commands::dev_selftest_report,
        ]
    };
}

#[cfg(not(debug_assertions))]
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            commands::ensure_inbox,
            commands::list_task_lists,
            commands::create_task_list,
            commands::rename_task_list,
            commands::move_task_list,
            commands::delete_task_list,
            commands::create_task,
            commands::get_task,
            commands::update_task,
            commands::complete_task,
            commands::move_task,
            commands::delete_task,
            commands::complete_tasks,
            commands::delete_tasks,
            commands::trash_tasks,
            commands::untrash_tasks,
            commands::purge_tasks,
            commands::empty_task_trash,
            commands::move_tasks,
            commands::restore_tasks,
            commands::set_dues,
            commands::list_tasks,
            commands::search_tasks,
            commands::list_note_folders,
            commands::list_note_items,
            commands::get_note,
            commands::create_note,
            commands::update_note_body,
            commands::discard_empty_note,
            commands::rename_note,
            commands::set_note_kind,
            commands::move_note,
            commands::create_note_folder,
            commands::rename_note_folder,
            commands::move_note_folder,
            commands::trash_note_items,
            commands::restore_note_items,
            commands::purge_note_items,
            commands::empty_note_trash,
            commands::list_note_trash,
            commands::search_notes,
            commands::auth_info,
            commands::auth_current,
            commands::auth_begin_login,
            commands::auth_open_browser,
            commands::auth_poll_login,
            commands::auth_cancel_login,
            commands::auth_logout,
            commands::sync_status,
            commands::sync_now,
            commands::sync_flush,
            commands::close_ready,
            commands::open_external,
            commands::import_notes_folder,
            commands::set_online,
            commands::app_ready,
        ]
    };
}

/// So lange wartet das Schließen höchstens auf die UI (letztes Speichern einer Notiz).
const CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1_500);

/// Startzeitpunkt des Prozesses (so früh wie möglich in [`run`] gesetzt).
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Gesetzt → Startzeit auf stderr ausgeben (`tools/measure-desktop.sh`).
const MEASURE_ENV: &str = "TODO_MEASURE_STARTUP";

/// Log-Filter in `Targets`-Syntax, z. B. `debug` oder `info,proton_adapter::http=debug`;
/// Standard `info`.
const LOG_ENV: &str = "TODO_LOG";

/// Installiert den Log-Subscriber (stderr). Niemand loggt Task-Inhalte oder Tokens
/// (`development/architecture.md` §5); der Proton-Adapter loggt nur Routen-Vorlagen (ADR-018).
fn init_logging() {
    use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

    let default = || Targets::new().with_default(tracing::Level::INFO);
    let filter = match env::var(LOG_ENV) {
        Ok(spec) => spec.parse::<Targets>().unwrap_or_else(|err| {
            eprintln!("{LOG_ENV}={spec:?} ignored: {err}");
            default()
        }),
        Err(_) => default(),
    };
    // Schlägt nur fehl, wenn schon ein Subscriber läuft – dann bleibt der.
    drop(
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(filter)
            .try_init(),
    );
}

fn report_startup(started: Option<Instant>) {
    if env::var_os(MEASURE_ENV).is_some() {
        let since_start = started.map_or(0, |t| t.elapsed().as_millis());
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        eprintln!("TODO_MEASURE first_paint since_main_ms={since_start} unix_ms={unix_ms}");
    }
}

/// Baut den [`AppState`] und übergibt ihn Tauri (aus `setup`; Tests rufen es direkt auf).
///
/// # Errors
/// Wenn Datenverzeichnis oder Datenbank nicht geöffnet werden können.
pub fn manage_state<R: Runtime>(
    app: &tauri::AppHandle<R>,
    options: &Options,
) -> Result<(), String> {
    app.manage(state::init(app, options)?);
    Ok(())
}

/// Ergänzt `base` um State, Fenster-Ereignisse und Commands. Ohne Laufzeit-Bindung, damit Tests
/// `tauri::test::mock_builder()` übergeben können.
pub fn builder<R: Runtime>(base: tauri::Builder<R>, options: Options) -> tauri::Builder<R> {
    base.setup(move |app| {
        let state = state::init(app.handle(), &options)?;
        app.manage(state);
        Ok(())
    })
    .on_window_event(window_event)
    .invoke_handler(handlers!())
}

/// Fenster sichtbar/versteckt → Scheduler (Pull bei Fokus, keine Timer im Hintergrund).
fn window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let Some(state) = window.try_state::<AppState>() else {
        return;
    };
    let hidden = || window.is_minimized().unwrap_or(false) || !window.is_visible().unwrap_or(true);
    match event {
        WindowEvent::Focused(true) => {
            state.sync.event(SchedulerEvent::AppFocused);
            state.purge_expired_notes_daily();
        }
        WindowEvent::Focused(false) | WindowEvent::Resized(_) if hidden() => {
            state.sync.event(SchedulerEvent::AppHidden);
        }
        // Vor dem Schließen noch ausstehende Änderungen hochladen (best effort) und der UI Zeit
        // geben, eine offene Notiz zu speichern (`notes.md` §6.2): Sie meldet `close_ready`,
        // spätestens nach `CLOSE_TIMEOUT` schließt das Fenster trotzdem.
        WindowEvent::CloseRequested { api, .. } => {
            state.sync.event(SchedulerEvent::AppHidden);
            if !state
                .closing
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                api.prevent_close();
                let _ = window.emit(events::CLOSE_REQUESTED_EVENT, ());
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(CLOSE_TIMEOUT).await;
                    let _ = window.destroy();
                });
            }
        }
        _ => {}
    }
}

/// WebKitGTK ohne beschleunigten Compositing-Modus: spart im WebProcess ~20 MB PSS und macht den
/// Wert stabil, Scrollen/Abhaken messbar unverändert (P8-03, ADR-001). Rückgängig mit
/// `TODO_WEBKIT_COMPOSITING=1`; eine explizit gesetzte WebKit-Variable bleibt unangetastet.
const WEBKIT_COMPOSITING_ENV: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";
const KEEP_COMPOSITING_ENV: &str = "TODO_WEBKIT_COMPOSITING";

fn configure_webkit() {
    if env::var_os(WEBKIT_COMPOSITING_ENV).is_none() && env::var_os(KEEP_COMPOSITING_ENV).is_none()
    {
        // Noch vor jedem weiteren Thread und vor dem Start von WebKit.
        env::set_var(WEBKIT_COMPOSITING_ENV, "1");
    }
}

/// Startet die App und blockiert bis zum Beenden.
///
/// # Panics
/// Wenn Tauri nicht starten kann (z. B. keine WebView) – ohne Fenster gibt es nichts zu tun.
pub fn run() {
    STARTED.get_or_init(Instant::now);
    init_logging();
    configure_webkit();
    builder(tauri::Builder::default(), Options::from_env())
        .run(tauri::generate_context!())
        .expect("Tauri-App konnte nicht gestartet werden");
}
