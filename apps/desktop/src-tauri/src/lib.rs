//! Tauri-2-Shell (Linux). Composition Root: verdrahtet Core, Speicher, Sync und Proton und reicht
//! die Fassade `todo_core::api::CoreApi` als dünne `async`-Commands an die UI durch.

/// Startet die App und blockiert bis zum Beenden.
///
/// # Panics
/// Wenn Tauri nicht starten kann (z. B. keine WebView) – ohne Fenster gibt es nichts zu tun.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("Tauri-App konnte nicht gestartet werden");
}
