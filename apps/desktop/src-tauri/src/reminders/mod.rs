//! Erinnerungen und Hintergrundbetrieb (N7, ADR-033). Ein Task plant mit [`plan::plan`]: genau ein
//! Timer auf die nächste Erinnerung (höchstens 15 min, damit ein Ruhezustand ihn nicht verschiebt –
//! nur solange eine ansteht), neu geplant bei Task-Änderungen, Einstellungen und Fokus.

pub mod plan;
pub mod settings;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use jiff::tz::TimeZone;
use serde::Serialize;
use todo_core::api::{
    dto::{PageRequestDto, TaskQueryDto, TaskStatusDto, ViewDto},
    CoreApi, CoreEvent, EventSink,
};
use tokio::sync::mpsc;

use self::{
    plan::{notification_text, now_ms, plan, reminder_at, Reminder},
    settings::BackgroundSettingsDto,
};

/// Höchstens so lange schläft der Timer, solange eine Erinnerung ansteht (Ruhezustand, ADR-033).
const MAX_SLEEP: Duration = Duration::from_secs(15 * 60);
/// Viele Änderungen hintereinander (Sync, Sammelaktion) → einmal neu planen.
const DEBOUNCE: Duration = Duration::from_millis(500);
/// „Schon gemeldet“ wird so lange gemerkt.
const KEEP_NOTIFIED_MS: i64 = 2 * 24 * 60 * 60 * 1000;
const NOTIFIED_FILE: &str = "reminders.json";

/// Zeigt eine Benachrichtigung; `task_id` öffnet beim Klick die Task (bei mehreren die erste).
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str, task_id: &str);
}

/// Gemeldete Erinnerung (Debug-Selbsttest, `dev_reminders_fired`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiredDto {
    pub task_id: String,
    pub title: String,
    pub at: i64,
}

enum Msg {
    Changed,
    Settings(BackgroundSettingsDto),
}

/// Griff auf den Planer.
#[derive(Debug, Clone)]
pub struct Reminders {
    tx: mpsc::UnboundedSender<Msg>,
    fired: Arc<Mutex<Vec<FiredDto>>>,
}

impl Reminders {
    /// Neu planen (Fokus, Aufwachen, Änderung).
    pub fn changed(&self) {
        let _ = self.tx.send(Msg::Changed);
    }

    pub fn set_settings(&self, settings: BackgroundSettingsDto) {
        let _ = self.tx.send(Msg::Settings(settings));
    }

    /// Bisher gemeldete Erinnerungen dieses Laufs (höchstens 50).
    #[must_use]
    pub fn fired(&self) -> Vec<FiredDto> {
        self.fired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Core-Event-Sink: jede Task-Änderung plant neu.
    #[must_use]
    pub fn sink(&self) -> Box<dyn EventSink> {
        Box::new(ChangeSink(self.tx.clone()))
    }
}

struct ChangeSink(mpsc::UnboundedSender<Msg>);

impl std::fmt::Debug for ChangeSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChangeSink")
    }
}

impl EventSink for ChangeSink {
    fn emit(&self, event: CoreEvent) {
        if matches!(&event, CoreEvent::Changed { tasks, .. } if !tasks.is_empty()) {
            let _ = self.0.send(Msg::Changed);
        }
    }
}

/// Startet den Planer auf der Tauri-Laufzeit.
pub fn spawn(
    api: Arc<CoreApi>,
    dir: PathBuf,
    settings: BackgroundSettingsDto,
    notifier: Arc<dyn Notifier>,
) -> Reminders {
    let (tx, rx) = mpsc::unbounded_channel();
    let fired = Arc::new(Mutex::new(Vec::new()));
    let handle = Reminders {
        tx,
        fired: Arc::clone(&fired),
    };
    tauri::async_runtime::spawn(run(api, dir, settings, notifier, rx, fired));
    handle
}

async fn run(
    api: Arc<CoreApi>,
    dir: PathBuf,
    mut settings: BackgroundSettingsDto,
    notifier: Arc<dyn Notifier>,
    mut rx: mpsc::UnboundedReceiver<Msg>,
    fired: Arc<Mutex<Vec<FiredDto>>>,
) {
    let local = TimeZone::system();
    let mut notified = load_notified(&dir);
    loop {
        let next = if settings.reminders {
            match collect(&api, settings.all_day(), &local).await {
                Ok(items) => {
                    let now = now_ms();
                    let p = plan(&items, &notified, now);
                    if !p.fire.is_empty() {
                        let (title, body) = notification_text(&p.fire);
                        notifier.notify(&title, &body, &p.fire[0].task_id);
                        record(&fired, &p.fire);
                        notified.extend(p.fire.iter().map(Reminder::key));
                        notified.retain(|(_, at)| *at > now - KEEP_NOTIFIED_MS);
                        save_notified(&dir, &notified);
                    }
                    p.next.map(|at| u64::try_from(at - now).unwrap_or(0))
                }
                Err(err) => {
                    tracing::warn!(error = %err, "reminders: reading tasks failed");
                    None
                }
            }
        } else {
            None
        };
        let sleep = next.map(|ms| Duration::from_millis(ms).min(MAX_SLEEP));
        tokio::select! {
            msg = rx.recv() => match msg {
                None => return,
                Some(Msg::Settings(s)) => settings = s,
                Some(Msg::Changed) => {
                    // Serien von Änderungen zusammenfassen.
                    tokio::time::sleep(DEBOUNCE).await;
                    while let Ok(msg) = rx.try_recv() {
                        if let Msg::Settings(s) = msg {
                            settings = s;
                        }
                    }
                }
            },
            () = sleep_for(sleep) => {}
        }
    }
}

async fn sleep_for(duration: Option<Duration>) {
    match duration {
        Some(d) => tokio::time::sleep(d).await,
        None => std::future::pending().await,
    }
}

/// Offene Tasks mit Fälligkeit (ohne Papierkorb – die Ansicht „Alle“ blendet ihn aus).
async fn collect(
    api: &CoreApi,
    all_day: (i8, i8),
    local: &TimeZone,
) -> todo_core::CoreResult<Vec<Reminder>> {
    let mut items = Vec::new();
    let mut cursor = None;
    loop {
        let page = api
            .list_tasks(
                TaskQueryDto {
                    view: ViewDto::All,
                    status: Some(TaskStatusDto::Open),
                    tag: None,
                },
                PageRequestDto { cursor, limit: 500 },
            )
            .await?;
        items.extend(page.items.into_iter().filter_map(|t| {
            let at = reminder_at(t.due.as_ref()?, all_day, local)?;
            Some(Reminder {
                task_id: t.id,
                title: t.title,
                at,
            })
        }));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(items),
        }
    }
}

fn record(fired: &Mutex<Vec<FiredDto>>, items: &[Reminder]) {
    let mut fired = fired.lock().unwrap_or_else(PoisonError::into_inner);
    fired.extend(items.iter().map(|r| FiredDto {
        task_id: r.task_id.clone(),
        title: r.title.clone(),
        at: r.at,
    }));
    let excess = fired.len().saturating_sub(50);
    fired.drain(..excess);
}

fn load_notified(dir: &Path) -> HashSet<(String, i64)> {
    std::fs::read(dir.join(NOTIFIED_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<(String, i64)>>(&bytes).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

fn save_notified(dir: &Path, notified: &HashSet<(String, i64)>) {
    let mut list: Vec<&(String, i64)> = notified.iter().collect();
    list.sort();
    if let Err(err) = serde_json::to_vec(&list)
        .map_err(|e| e.to_string())
        .and_then(|json| std::fs::write(dir.join(NOTIFIED_FILE), json).map_err(|e| e.to_string()))
    {
        tracing::warn!(error = %err, "reminders: saving state failed");
    }
}

/// Benachrichtigung über das XDG-Desktop-Portal; ein Klick ruft `on_open(task_id)` auf.
#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct PortalNotifier {
    inner: Arc<PortalInner>,
}

#[cfg(target_os = "linux")]
impl std::fmt::Debug for PortalNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortalNotifier").finish_non_exhaustive()
    }
}

#[cfg(target_os = "linux")]
struct PortalInner {
    proxy: tokio::sync::OnceCell<Option<ashpd::desktop::notification::NotificationProxy>>,
    on_open: Arc<dyn Fn(String) + Send + Sync>,
}

#[cfg(target_os = "linux")]
impl PortalNotifier {
    #[must_use]
    pub fn new(on_open: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(PortalInner {
                proxy: tokio::sync::OnceCell::new(),
                on_open,
            }),
        }
    }
}

#[cfg(target_os = "linux")]
impl PortalInner {
    /// Verbindet beim ersten Gebrauch und lauscht ab dann auf Klicks.
    async fn proxy(&self) -> Option<&ashpd::desktop::notification::NotificationProxy> {
        self.proxy
            .get_or_init(|| async {
                use futures::StreamExt;
                let proxy = match ashpd::desktop::notification::NotificationProxy::new().await {
                    Ok(p) => p,
                    Err(err) => {
                        tracing::warn!(error = %err, "reminders: no notification portal");
                        return None;
                    }
                };
                // Klicks hört ein eigener Task mit eigener Verbindung (der Stream leiht seinen Proxy).
                let on_open = Arc::clone(&self.on_open);
                tauri::async_runtime::spawn(async move {
                    let Ok(listener) = ashpd::desktop::notification::NotificationProxy::new().await
                    else {
                        return;
                    };
                    let actions = match listener.receive_action_invoked().await {
                        Ok(actions) => actions,
                        Err(err) => {
                            tracing::warn!(error = %err, "reminders: no action signal");
                            return;
                        }
                    };
                    let mut actions = Box::pin(actions);
                    while let Some(action) = actions.next().await {
                        let id = action
                            .parameter()
                            .first()
                            .and_then(|v| String::try_from(v.clone()).ok());
                        if let Some(id) = id {
                            on_open(id);
                        }
                    }
                });
                Some(proxy)
            })
            .await
            .as_ref()
    }
}

#[cfg(target_os = "linux")]
impl Notifier for PortalNotifier {
    fn notify(&self, title: &str, body: &str, task_id: &str) {
        use ashpd::desktop::notification::{Notification, Priority};
        let inner = Arc::clone(&self.inner);
        let (title, body, task_id) = (title.to_owned(), body.to_owned(), task_id.to_owned());
        tauri::async_runtime::spawn(async move {
            let Some(proxy) = inner.proxy().await else {
                return;
            };
            let notification = Notification::new(&title)
                .body(body.as_str())
                .priority(Priority::Normal)
                .default_action("open-task")
                .default_action_target(task_id.as_str());
            if let Err(err) = proxy
                .add_notification(&format!("reminder-{task_id}"), notification)
                .await
            {
                tracing::warn!(error = %err, "reminders: notification failed");
            }
        });
    }
}

/// Nur protokollieren (Selbsttest im Debug-Build mit `TODO_NOTIFY=log`, andere Systeme).
#[derive(Debug)]
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, title: &str, _body: &str, _task_id: &str) {
        tracing::info!(title, "reminder");
    }
}
