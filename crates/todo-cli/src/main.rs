//! Entwickler-CLI: Core + SQLite ohne UI benutzen und debuggen. Nicht Teil der App-Auslieferung.
//!
//! Nutzt ausschließlich die Fassade [`CoreApi`] – genau wie später die UI-Shells. Aufruf und
//! Befehle: `todo-cli help`.

mod args;
#[cfg(target_os = "linux")]
mod proton;

use std::{
    collections::BTreeMap,
    env, fmt,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use args::{Cli, Command, UsageError, USAGE};
use todo_core::{
    api::{
        dto::{
            iso, CreateTaskDto, DueDto, LocalDayDto, MoveTargetDto, PageRequestDto, PriorityDto,
            TaskPatchDto, TaskQueryDto, TaskStatusDto, ViewDto,
        },
        CoreApi,
    },
    domain::{NaiveDate, Timestamp},
    ports::{Clock, KeyStore, RemoteStore, SecretBytes},
    CoreError, CoreResult,
};
use todo_crypto::XChaCha20Poly1305Cipher;
use todo_remote_fs::FsRemoteStore;
use todo_storage_sqlite::SqliteTaskRepository;
use todo_sync::{SyncConfig, SyncEngine, SyncReport};

const DEFAULT_DB: &str = "todo-dev.sqlite3";
const PAGE_SIZE: u32 = 200;
/// So viele Zeichen vom Ende der ID zeigt `ls` an.
const SHORT_ID: usize = 8;
/// Kürzere ID-Enden sind zu oft mehrdeutig.
const MIN_ID_SUFFIX: usize = 4;
const MS_PER_DAY: i64 = 86_400_000;
/// Tag 1 = 0001-01-01; 1970-01-01 ist Tag 719 163.
const UNIX_EPOCH_DAYS_FROM_CE: i64 = 719_163;

#[derive(Debug)]
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Timestamp::from_millis(i64::try_from(ms).unwrap_or(i64::MAX))
    }
}

/// Schlüssel nur für die Dauer eines Aufrufs: Die CLI lädt den Sync-Schlüssel bei jedem `sync`
/// aus dem Sync-Ordner (`keys/sync-key.enc`). Die App nutzt dafür den System-Schlüsselbund.
#[derive(Debug, Default)]
struct ProcessKeyStore(Mutex<BTreeMap<String, SecretBytes>>);

#[async_trait::async_trait]
impl KeyStore for ProcessKeyStore {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>> {
        Ok(self.entries().get(name).cloned())
    }

    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()> {
        self.entries().insert(name.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, name: &str) -> CoreResult<()> {
        self.entries().remove(name);
        Ok(())
    }
}

impl ProcessKeyStore {
    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, SecretBytes>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
enum Error {
    Usage(UsageError),
    Core(CoreError),
    Input(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(e) => e.fmt(f),
            Self::Core(e) => e.fmt(f),
            Self::Input(msg) => f.write_str(msg),
        }
    }
}

impl From<CoreError> for Error {
    fn from(e: CoreError) -> Self {
        Self::Core(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

fn main() -> ExitCode {
    match args::parse(env::args().skip(1))
        .map_err(Error::Usage)
        .and_then(run)
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {}", safe(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    if cli.command == Command::Help {
        println!("{USAGE}");
        return Ok(());
    }
    let path = cli
        .db
        .or_else(|| env::var_os("TODO_DB").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DB));
    let remote = cli
        .remote
        .or_else(|| env::var_os("TODO_REMOTE").map(PathBuf::from));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Input(format!("tokio runtime: {e}")))?;
    match cli.command {
        Command::Login => return runtime.block_on(login()),
        Command::Logout => return runtime.block_on(logout()),
        _ => {}
    }
    let repo = SqliteTaskRepository::open(&path)?;
    let api = CoreApi::new(Arc::new(repo), Arc::new(SystemClock));
    runtime.block_on(async {
        // Wie beim App-Start: Die Inbox existiert immer.
        api.ensure_inbox().await?;
        execute(&api, cli.command, remote).await
    })
}

#[cfg(target_os = "linux")]
async fn login() -> Result<()> {
    proton::login().await
}

#[cfg(target_os = "linux")]
async fn logout() -> Result<()> {
    proton::logout().await
}

#[cfg(not(target_os = "linux"))]
async fn login() -> Result<()> {
    Err(Error::Input(
        "Proton-Login gibt es in der Dev-CLI nur unter Linux".into(),
    ))
}

#[cfg(not(target_os = "linux"))]
async fn logout() -> Result<()> {
    login().await
}

/// Backend + KeyStore für `sync`: `proton` oder ein lokales Verzeichnis.
async fn backend(remote: PathBuf) -> Result<(Arc<dyn RemoteStore>, Arc<dyn KeyStore>)> {
    if remote.as_os_str() == "proton" {
        #[cfg(target_os = "linux")]
        {
            let proton = proton::Proton::connect().await?;
            return Ok((proton.remote().await?, Arc::clone(&proton.key_store)));
        }
        #[cfg(not(target_os = "linux"))]
        return Err(Error::Input(
            "Proton-Sync gibt es in der Dev-CLI nur unter Linux".into(),
        ));
    }
    Ok((
        Arc::new(FsRemoteStore::new(remote)?),
        Arc::new(ProcessKeyStore::default()),
    ))
}

/// Ein Sync-Durchlauf gegen das gewählte Backend.
async fn sync(api: &CoreApi, remote: PathBuf) -> Result<()> {
    let device_name = env::var("HOSTNAME").unwrap_or_else(|_| "todo-cli".into());
    let (remote, key_store) = backend(remote).await?;
    let engine = SyncEngine::new(
        api.repository(),
        remote,
        Arc::new(XChaCha20Poly1305Cipher::new()),
        key_store,
        Arc::new(SystemClock),
        SyncConfig {
            device_name,
            platform: "todo-cli".into(),
            ..SyncConfig::default()
        },
    );
    let started = Instant::now();
    let report = engine.sync_once().await?;
    println!(
        "{} ({} ms)",
        report_text(&report),
        started.elapsed().as_millis()
    );
    for warning in &report.warnings {
        println!("Warnung: {}", safe(warning));
    }
    let pending = engine.pending().await?;
    if pending > 0 {
        println!("{pending} Op(s) noch nicht hochgeladen.");
    }
    Ok(())
}

fn report_text(report: &SyncReport) -> String {
    let mut text = format!(
        "Sync: {} Op(s) hochgeladen, {} empfangen, {} Änderung(en) übernommen.",
        report.pushed, report.received, report.changed
    );
    if report.snapshot_loaded {
        text.push_str(" Snapshot geladen.");
    }
    if report.snapshot_written {
        text.push_str(" Snapshot geschrieben.");
    }
    if report.skipped_segments > 0 {
        text.push_str(&format!(
            " {} Segment(e) übersprungen.",
            report.skipped_segments
        ));
    }
    if report.newer_format {
        text.push_str(" Sync-Ordner stammt von einer neueren Version – bitte aktualisieren.");
    }
    text
}

async fn execute(api: &CoreApi, command: Command, remote: Option<PathBuf>) -> Result<()> {
    match command {
        Command::Help => println!("{USAGE}"),
        Command::Lists => {
            for list in api.list_task_lists().await? {
                let inbox = if list.is_inbox { "  (Inbox)" } else { "" };
                println!("{}  {}{inbox}", short(&list.id), safe(&list.name));
            }
        }
        Command::ListAdd { name } => {
            let list = api.create_task_list(name).await?;
            println!("Liste angelegt: {}  {}", short(&list.id), safe(&list.name));
        }
        Command::ListRm { list } => {
            let id = resolve_list(api, &list).await?;
            let moved = api.delete_task_list(id).await?;
            println!("Liste gelöscht; {moved} Task(s) in die Inbox verschoben.");
        }
        Command::Add { title, list } => {
            let list_id = match list {
                Some(list) => Some(resolve_list(api, &list).await?),
                None => None,
            };
            let task = api
                .create_task(CreateTaskDto {
                    title,
                    list_id,
                    ..CreateTaskDto::default()
                })
                .await?;
            println!(
                "{}",
                line(
                    &task.id,
                    task.status,
                    &task.title,
                    task.priority,
                    task.due.as_ref(),
                    !task.notes.is_empty()
                )
            );
        }
        Command::Ls { view, open_only } => {
            let query = TaskQueryDto {
                view: parse_view(api, &view).await?,
                status: open_only.then_some(TaskStatusDto::Open),
                tag: None,
                priority: None,
                sort: None,
            };
            let mut cursor = None;
            let mut count = 0;
            loop {
                let page = api
                    .list_tasks(
                        query.clone(),
                        PageRequestDto {
                            cursor,
                            limit: PAGE_SIZE,
                        },
                    )
                    .await?;
                for s in &page.items {
                    count += 1;
                    let due = s.due.as_ref();
                    println!(
                        "{}",
                        line(&s.id, s.status, &s.title, s.priority, due, s.has_notes)
                    );
                }
                match page.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => break,
                }
            }
            if count == 0 {
                println!("(keine Tasks)");
            }
        }
        Command::Show { id } => {
            let id = resolve_task(api, &id).await?;
            let task = api.get_task(id).await?;
            let list = api
                .list_task_lists()
                .await?
                .into_iter()
                .find(|l| l.id == task.list_id)
                .map_or_else(|| task.list_id.clone(), |l| l.name);
            println!("ID:        {}", task.id);
            println!("Titel:     {}", safe(&task.title));
            println!("Liste:     {}", safe(&list));
            println!("Status:    {}", status_text(task.status));
            println!("Priorität: {}", prio_text(task.priority));
            println!(
                "Fällig:    {}",
                task.due.as_ref().map_or("–".into(), due_text)
            );
            println!("Angelegt:  {}", task.created_at);
            println!("Geändert:  {}", task.updated_at);
            if let Some(at) = &task.completed_at {
                println!("Erledigt:  {at}");
            }
            if !task.notes.is_empty() {
                println!("Notizen:\n{}", safe(&task.notes));
            }
        }
        Command::Done { id, done } => {
            let id = resolve_task(api, &id).await?;
            let task = api.complete_task(id, done).await?;
            println!("{}: {}", status_text(task.status), safe(&task.title));
        }
        Command::Edit {
            id,
            title,
            notes,
            prio,
            due,
        } => {
            let patch = TaskPatchDto {
                title,
                notes,
                priority: prio.as_deref().map(parse_prio).transpose()?,
                due: due.as_deref().map(parse_due),
                ..TaskPatchDto::default()
            };
            let id = resolve_task(api, &id).await?;
            let task = api.update_task(id, patch).await?;
            println!("Geändert: {}", safe(&task.title));
        }
        Command::Mv {
            id,
            list,
            before,
            after,
        } => {
            let id = resolve_task(api, &id).await?;
            let target = MoveTargetDto {
                list_id: match list {
                    Some(list) => Some(resolve_list(api, &list).await?),
                    None => None,
                },
                before: match before {
                    Some(b) => Some(resolve_task(api, &b).await?),
                    None => None,
                },
                after: match after {
                    Some(a) => Some(resolve_task(api, &a).await?),
                    None => None,
                },
            };
            let task = api.move_task(id, target).await?;
            println!("Verschoben: {}", safe(&task.title));
        }
        Command::Rm { id } => {
            let id = resolve_task(api, &id).await?;
            api.delete_task(id).await?;
            println!("Gelöscht.");
        }
        Command::Sync => {
            let remote = remote.ok_or_else(|| {
                Error::Usage(UsageError(
                    "sync braucht --remote ORDNER oder $TODO_REMOTE".into(),
                ))
            })?;
            sync(api, remote).await?;
        }
        // Laufen ohne Datenbank (siehe `run`).
        Command::Login | Command::Logout => unreachable!("handled before opening the database"),
    }
    Ok(())
}

// --- Auflösen von Eingaben ---------------------------------------------------------------------

/// Vollständige Task-ID oder eindeutiges Ende einer ID.
async fn resolve_task(api: &CoreApi, input: &str) -> Result<String> {
    let suffix = input.to_ascii_lowercase();
    if suffix.len() < MIN_ID_SUFFIX {
        return Err(Error::Input(format!(
            "ID {input:?} zu kurz (mindestens {MIN_ID_SUFFIX} Zeichen)"
        )));
    }
    let query = TaskQueryDto {
        view: ViewDto::All,
        status: None,
        tag: None,
        priority: None,
        sort: None,
    };
    let mut matches = Vec::new();
    let mut cursor = None;
    loop {
        let page = api
            .list_tasks(
                query.clone(),
                PageRequestDto {
                    cursor,
                    limit: PAGE_SIZE,
                },
            )
            .await?;
        matches.extend(
            page.items
                .into_iter()
                .filter(|s| s.id.ends_with(&suffix))
                .map(|s| s.id),
        );
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    unique(matches, "Task", input)
}

/// Listenname (ohne Groß-/Kleinschreibung) oder vollständige/Ende einer Listen-ID.
async fn resolve_list(api: &CoreApi, input: &str) -> Result<String> {
    let lists = api.list_task_lists().await?;
    let by_name: Vec<String> = lists
        .iter()
        .filter(|l| {
            l.name.eq_ignore_ascii_case(input) || l.name.to_lowercase() == input.to_lowercase()
        })
        .map(|l| l.id.clone())
        .collect();
    if !by_name.is_empty() {
        return unique(by_name, "Liste", input);
    }
    let suffix = input.to_ascii_lowercase();
    let by_id = if suffix.len() >= MIN_ID_SUFFIX {
        lists
            .into_iter()
            .filter(|l| l.id.ends_with(&suffix))
            .map(|l| l.id)
            .collect()
    } else {
        Vec::new()
    };
    unique(by_id, "Liste", input)
}

fn unique(mut ids: Vec<String>, what: &str, input: &str) -> Result<String> {
    match ids.len() {
        1 => Ok(ids.remove(0)),
        0 => Err(Error::Input(format!("keine {what} passt zu {input:?}"))),
        n => Err(Error::Input(format!(
            "{input:?} ist mehrdeutig ({n} Treffer) – mehr Zeichen angeben"
        ))),
    }
}

async fn parse_view(api: &CoreApi, view: &str) -> Result<ViewDto> {
    Ok(match view.to_lowercase().as_str() {
        "inbox" => ViewDto::Inbox,
        "all" => ViewDto::All,
        "done" => ViewDto::Completed,
        "today" => ViewDto::Today {
            day: utc_day(SystemClock.now()),
        },
        "upcoming" => ViewDto::Upcoming {
            day: utc_day(SystemClock.now()),
        },
        _ => ViewDto::List {
            list_id: resolve_list(api, view).await?,
        },
    })
}

/// Der UTC-Kalendertag, in dem `now` liegt. (Die App rechnet später in der lokalen Zone.)
fn utc_day(now: Timestamp) -> LocalDayDto {
    let days = now.as_millis().div_euclid(MS_PER_DAY);
    let date = i32::try_from(days + UNIX_EPOCH_DAYS_FROM_CE)
        .ok()
        .and_then(NaiveDate::from_num_days_from_ce_opt)
        .unwrap_or(NaiveDate::MAX);
    let start = Timestamp::from_millis(days * MS_PER_DAY);
    LocalDayDto {
        date: date.to_string(),
        start: iso(start),
        end: iso(start.plus_millis(MS_PER_DAY)),
    }
}

fn parse_prio(s: &str) -> Result<PriorityDto> {
    match s.to_lowercase().as_str() {
        "none" => Ok(PriorityDto::None),
        "low" => Ok(PriorityDto::Low),
        "medium" => Ok(PriorityDto::Medium),
        "high" => Ok(PriorityDto::High),
        _ => Err(Error::Input(format!(
            "Priorität {s:?}: erlaubt sind none, low, medium, high"
        ))),
    }
}

/// `none` löscht die Fälligkeit; sonst ein Datum (der Core prüft das Format).
fn parse_due(s: &str) -> Option<DueDto> {
    (!s.eq_ignore_ascii_case("none")).then(|| DueDto::Date { date: s.to_owned() })
}

// --- Ausgabe -----------------------------------------------------------------------------------

fn short(id: &str) -> &str {
    &id[id.len().saturating_sub(SHORT_ID)..]
}

fn line(
    id: &str,
    status: TaskStatusDto,
    title: &str,
    priority: PriorityDto,
    due: Option<&DueDto>,
    has_notes: bool,
) -> String {
    let check = match status {
        TaskStatusDto::Open => "[ ]",
        TaskStatusDto::Done => "[x]",
    };
    let mut out = format!("{check} {}  {}", short(id), safe(title));
    if let Some(due) = due {
        out.push_str(&format!("  · fällig {}", due_text(due)));
    }
    if priority != PriorityDto::None {
        out.push_str(&format!("  · !{}", prio_text(priority)));
    }
    if has_notes {
        out.push_str("  · +Notiz");
    }
    out
}

/// Text aus dem Sync (andere Geräte, Server) terminalsicher machen: Steuerzeichen außer
/// Zeilenumbruch und Tab sowie Bidi-Steuerzeichen werden als `\u{…}` ausgegeben, damit keine
/// Escape-Sequenzen (Fenstertitel, Zwischenablage, Zeilen überschreiben) ins Terminal gelangen.
pub(crate) fn safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
        if (c.is_control() && c != '\n' && c != '\t') || bidi {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

fn status_text(status: TaskStatusDto) -> &'static str {
    match status {
        TaskStatusDto::Open => "offen",
        TaskStatusDto::Done => "erledigt",
    }
}

fn prio_text(priority: PriorityDto) -> &'static str {
    match priority {
        PriorityDto::None => "keine",
        PriorityDto::Low => "niedrig",
        PriorityDto::Medium => "mittel",
        PriorityDto::High => "hoch",
    }
}

fn due_text(due: &DueDto) -> String {
    match due {
        DueDto::Date { date } => date.clone(),
        DueDto::DateTime { at, tz } => format!("{at} ({})", safe(tz)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_text_is_terminal_safe() {
        let title = "Milch\u{1b}]0;SPOOF\u{7}\u{1b}[2K\rx\u{202e}";
        let shown = line(
            "0192aaaa-0000-7000-8000-000000000000",
            TaskStatusDto::Open,
            title,
            PriorityDto::None,
            None,
            false,
        );
        assert!(
            !shown.chars().any(|c| c.is_control() || c == '\u{202e}'),
            "{shown:?}"
        );
        assert!(shown.contains("Milch\\u{1b}]0;SPOOF\\u{7}"), "{shown:?}");
        assert_eq!(safe("a\nb\tc"), "a\nb\tc", "Umbruch und Tab bleiben");
    }

    #[test]
    fn utc_day_boundaries() {
        let day = utc_day(Timestamp::from_millis(0));
        assert_eq!(day.date, "1970-01-01");
        assert_eq!(day.start, "1970-01-01T00:00:00.000Z");
        assert_eq!(day.end, "1970-01-02T00:00:00.000Z");
        let day = utc_day(Timestamp::from_millis(1_760_710_496_123));
        assert_eq!(day.date, "2025-10-17");
        assert_eq!(day.start, "2025-10-17T00:00:00.000Z");
        assert_eq!(utc_day(Timestamp::from_millis(-1)).date, "1969-12-31");
    }

    #[test]
    fn short_id_is_the_end() {
        assert_eq!(short("0192f0c4-7a3b-7c2d-8e4f-0123456789ab"), "456789ab");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn due_none_clears() {
        assert_eq!(parse_due("none"), None);
        assert_eq!(
            parse_due("2026-10-20"),
            Some(DueDto::Date {
                date: "2026-10-20".into()
            })
        );
        assert!(parse_prio("urgent").is_err());
    }

    #[test]
    fn line_shows_markers() {
        let due = DueDto::Date {
            date: "2026-10-20".into(),
        };
        assert_eq!(
            line(
                "x-12345678",
                TaskStatusDto::Done,
                "Milch",
                PriorityDto::High,
                Some(&due),
                true
            ),
            "[x] 12345678  Milch  · fällig 2026-10-20  · !hoch  · +Notiz"
        );
    }
}
