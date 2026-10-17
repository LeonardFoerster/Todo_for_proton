//! Tauri-2-Shell (Linux). Composition Root: verdrahtet Core, Speicher, Sync und Proton und reicht
//! die Fassade `todo_core::api::CoreApi` als dünne `async`-Commands an die UI durch.

use std::{
    env,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Startzeitpunkt des Prozesses (so früh wie möglich in [`run`] gesetzt).
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Gesetzt → Startzeit auf stderr ausgeben (`tools/measure-desktop.sh`).
const MEASURE_ENV: &str = "TODO_MEASURE_STARTUP";

/// Die UI meldet: erste Liste gerendert (nach dem ersten Frame).
#[tauri::command]
fn app_ready() {
    let since_start = STARTED.get().map_or(0, |t| t.elapsed().as_millis());
    if env::var_os(MEASURE_ENV).is_some() {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        eprintln!("TODO_MEASURE first_paint since_main_ms={since_start} unix_ms={unix_ms}");
    }
}

/// Startet die App und blockiert bis zum Beenden.
///
/// # Panics
/// Wenn Tauri nicht starten kann (z. B. keine WebView) – ohne Fenster gibt es nichts zu tun.
pub fn run() {
    STARTED.get_or_init(Instant::now);
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_ready])
        .run(tauri::generate_context!())
        .expect("Tauri-App konnte nicht gestartet werden");
}
