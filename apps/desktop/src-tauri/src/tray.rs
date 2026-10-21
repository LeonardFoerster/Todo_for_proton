//! Tray-Symbol für den Hintergrundbetrieb (ADR-033): Öffnen, Neue Task, Beenden. Nur da, solange
//! „Im Hintergrund weiterlaufen“ an ist.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};

use crate::{events::NEW_TASK_EVENT, state::AppState};

const TRAY_ID: &str = "tasks";

/// Fenster zeigen und nach vorn holen (Tray, Benachrichtigung, zweiter Start).
pub fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Beenden über das Tray: wie Schließen, aber am Ende wirklich zu (Notizen werden vorher gespeichert).
pub fn quit<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .quitting
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    match app.get_webview_window("main") {
        Some(window) => {
            let _ = window.close();
        }
        None => app.exit(0),
    }
}

/// Zeigt das Tray-Symbol (idempotent). Fehler (kein StatusNotifier-Host) nur im Log.
pub fn create<R: Runtime>(app: &AppHandle<R>) {
    if app.tray_by_id(TRAY_ID).is_some() {
        return;
    }
    let build = || -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "open", "Tasks öffnen", true, None::<&str>)?;
        let new = MenuItem::with_id(app, "new", "Neue Task", true, None::<&str>)?;
        let quit_item = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
        let menu = Menu::with_items(
            app,
            &[
                &open,
                &new,
                &PredefinedMenuItem::separator(app)?,
                &quit_item,
            ],
        )?;
        let mut builder = TrayIconBuilder::with_id(TRAY_ID)
            .tooltip("Tasks")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "open" => show_window(app),
                "new" => {
                    show_window(app);
                    let _ = app.emit(NEW_TASK_EVENT, ());
                }
                "quit" => quit(app),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_window(tray.app_handle());
                }
            });
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        builder.build(app)?;
        Ok(())
    };
    if let Err(err) = build() {
        tracing::warn!(error = %err, "tray not available");
    }
}

/// Entfernt das Tray-Symbol.
pub fn remove<R: Runtime>(app: &AppHandle<R>) {
    drop(app.remove_tray_by_id(TRAY_ID));
}
