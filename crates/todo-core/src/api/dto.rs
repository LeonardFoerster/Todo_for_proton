//! FFI-/IPC-freundliche Datentypen der Fassade: nur owned Typen, IDs als `String`,
//! Zeitpunkte als UTC-ISO-8601-String (`2026-10-17T12:00:00.000Z`), Daten als `YYYY-MM-DD`.
//! `serde` mit camelCase, damit Tauri-IPC und TypeScript sie direkt verwenden können.

use chrono::{DateTime, SecondsFormat};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        ChecklistItem, ChecklistItemId, Due, Frequency, LocalDay, NaiveDate, Priority, Recurrence,
        RecurrenceAnchor, SortKey, Tag, TagId, Task, TaskId, TaskList, TaskListId, TaskParts,
        TaskStatus, Timestamp, WeekdaySet,
    },
    ports::{Page, SmartView, TaskQuery, TaskSummary},
    CoreError, CoreResult,
};

// --- Zeit & IDs --------------------------------------------------------------------------------

/// Zeitpunkt → ISO-8601 in UTC mit Millisekunden.
#[must_use]
pub fn iso(ts: Timestamp) -> String {
    DateTime::from_timestamp_millis(ts.as_millis()).map_or_else(
        || ts.as_millis().to_string(),
        |dt| dt.to_rfc3339_opts(SecondsFormat::Millis, true),
    )
}

/// ISO-8601/RFC 3339 (beliebiger Offset) → Zeitpunkt.
///
/// # Errors
/// `CoreError::Validation` bei ungültigem Format.
pub fn parse_iso(s: &str) -> CoreResult<Timestamp> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| Timestamp::from_millis(dt.timestamp_millis()))
        .map_err(|e| CoreError::Validation(format!("invalid timestamp {s:?}: {e}")))
}

/// `YYYY-MM-DD` → Datum.
///
/// # Errors
/// `CoreError::Validation` bei ungültigem Datum.
pub fn parse_date(s: &str) -> CoreResult<NaiveDate> {
    s.parse()
        .map_err(|e| CoreError::Validation(format!("invalid date {s:?}: {e}")))
}

pub(crate) fn task_id(s: &str) -> CoreResult<TaskId> {
    Ok(TaskId::parse(s)?)
}

pub(crate) fn list_id(s: &str) -> CoreResult<TaskListId> {
    Ok(TaskListId::parse(s)?)
}

// --- Enums -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TaskStatusDto {
    Open,
    Done,
}

impl From<TaskStatus> for TaskStatusDto {
    fn from(s: TaskStatus) -> Self {
        match s {
            TaskStatus::Open => Self::Open,
            TaskStatus::Done => Self::Done,
        }
    }
}

impl From<TaskStatusDto> for TaskStatus {
    fn from(s: TaskStatusDto) -> Self {
        match s {
            TaskStatusDto::Open => Self::Open,
            TaskStatusDto::Done => Self::Done,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PriorityDto {
    None,
    Low,
    Medium,
    High,
}

impl From<Priority> for PriorityDto {
    fn from(p: Priority) -> Self {
        match p {
            Priority::None => Self::None,
            Priority::Low => Self::Low,
            Priority::Medium => Self::Medium,
            Priority::High => Self::High,
        }
    }
}

impl From<PriorityDto> for Priority {
    fn from(p: PriorityDto) -> Self {
        match p {
            PriorityDto::None => Self::None,
            PriorityDto::Low => Self::Low,
            PriorityDto::Medium => Self::Medium,
            PriorityDto::High => Self::High,
        }
    }
}

/// Fälligkeit: `{"kind":"date","date":"2026-10-20"}` oder
/// `{"kind":"dateTime","at":"2026-10-20T12:00:00.000Z","tz":"Europe/Berlin"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DueDto {
    Date { date: String },
    DateTime { at: String, tz: String },
}

impl From<&Due> for DueDto {
    fn from(due: &Due) -> Self {
        match due {
            Due::Date(date) => Self::Date {
                date: date.to_string(),
            },
            Due::DateTime { at, tz } => Self::DateTime {
                at: iso(*at),
                tz: tz.clone(),
            },
        }
    }
}

impl TryFrom<DueDto> for Due {
    type Error = CoreError;

    fn try_from(dto: DueDto) -> CoreResult<Self> {
        Ok(match dto {
            DueDto::Date { date } => Self::Date(parse_date(&date)?),
            DueDto::DateTime { at, tz } => Self::DateTime {
                at: parse_iso(&at)?,
                tz,
            },
        })
    }
}

// --- Ausgaben ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskListDto {
    pub id: String,
    pub name: String,
    pub is_inbox: bool,
    pub sort_key: String,
}

impl From<&TaskList> for TaskListDto {
    fn from(list: &TaskList) -> Self {
        Self {
            id: list.id().to_string(),
            name: list.name().to_owned(),
            is_inbox: list.is_inbox(),
            sort_key: list.sort_key().to_string(),
        }
    }
}

/// Vollständige Task (Detailansicht).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub notes: String,
    pub status: TaskStatusDto,
    pub priority: PriorityDto,
    pub due: Option<DueDto>,
    pub sort_key: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    /// Im Papierkorb seit (ADR-031).
    pub trashed_at: Option<String>,
    /// Wiederholung (N4, ADR-028).
    pub recurrence: Option<RecurrenceDto>,
    /// Checkliste (N8, ADR-034), in Anzeigereihenfolge.
    pub checklist: Vec<ChecklistItemDto>,
    /// Tag-IDs (ADR-035); Namen über `list_tags`.
    pub tags: Vec<String>,
}

/// Ein Tag (ADR-035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TagDto {
    pub id: String,
    /// Ohne `#`.
    pub name: String,
}

impl From<&Tag> for TagDto {
    fn from(tag: &Tag) -> Self {
        Self {
            id: tag.id().to_string(),
            name: tag.name().to_owned(),
        }
    }
}

/// Tag-IDs von außen.
///
/// # Errors
/// `CoreError::Validation` bei einer ungültigen ID.
pub fn tag_ids(ids: &[String]) -> CoreResult<Vec<TagId>> {
    ids.iter()
        .map(|id| TagId::parse(id).map_err(|e| CoreError::Validation(e.to_string())))
        .collect()
}

/// Punkt einer Checkliste. Beim Schreiben bekommt ein Punkt mit leerer `id` eine neue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItemDto {
    pub id: String,
    pub text: String,
    pub done: bool,
}

impl From<&ChecklistItem> for ChecklistItemDto {
    fn from(item: &ChecklistItem) -> Self {
        Self {
            id: item.id().to_string(),
            text: item.text().to_owned(),
            done: item.is_done(),
        }
    }
}

impl TryFrom<&ChecklistItemDto> for ChecklistItem {
    type Error = CoreError;

    fn try_from(dto: &ChecklistItemDto) -> CoreResult<Self> {
        let id = if dto.id.is_empty() {
            ChecklistItemId::new()
        } else {
            ChecklistItemId::parse(&dto.id).map_err(|e| CoreError::Validation(e.to_string()))?
        };
        ChecklistItem::new(id, &dto.text, dto.done)
    }
}

/// Liste von Punkten in Domain-Form (neue IDs für leere).
///
/// # Errors
/// `CoreError::Validation` bei ungültigem Punkt.
pub fn checklist_from_dto(items: &[ChecklistItemDto]) -> CoreResult<Vec<ChecklistItem>> {
    items.iter().map(TryInto::try_into).collect()
}

impl From<&Task> for TaskDto {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().to_string(),
            list_id: task.list_id().to_string(),
            title: task.title().to_owned(),
            notes: task.notes().to_owned(),
            status: task.status().into(),
            priority: task.priority().into(),
            due: task.due().map(DueDto::from),
            sort_key: task.sort_key().to_string(),
            created_at: iso(task.created_at()),
            updated_at: iso(task.updated_at()),
            completed_at: task.completed_at().map(iso),
            trashed_at: task.trashed_at().map(iso),
            recurrence: task.recurrence().map(RecurrenceDto::from),
            checklist: task
                .checklist()
                .iter()
                .map(ChecklistItemDto::from)
                .collect(),
            tags: task.tags().iter().map(ToString::to_string).collect(),
        }
    }
}

/// Für `restore_tasks`: alle Felder zurück in Domain-Form (Prüfung der Invarianten erst in
/// [`Task::from_parts`]). Unterteilungen kennt das DTO nicht – leer.
impl TryFrom<&TaskDto> for TaskParts {
    type Error = CoreError;

    fn try_from(dto: &TaskDto) -> CoreResult<Self> {
        Ok(Self {
            id: task_id(&dto.id)?,
            list_id: list_id(&dto.list_id)?,
            parent_id: None,
            title: dto.title.clone(),
            notes: dto.notes.clone(),
            status: dto.status.into(),
            priority: dto.priority.into(),
            due: dto.due.clone().map(TryInto::try_into).transpose()?,
            tags: tag_ids(&dto.tags)?,
            sort_key: SortKey::parse(&dto.sort_key)?,
            created_at: parse_iso(&dto.created_at)?,
            updated_at: parse_iso(&dto.updated_at)?,
            completed_at: dto.completed_at.as_deref().map(parse_iso).transpose()?,
            // Wiederhergestellt wird nie in den Papierkorb.
            trashed_at: None,
            recurrence: dto.recurrence.as_ref().map(TryInto::try_into).transpose()?,
            checklist: checklist_from_dto(&dto.checklist)?,
        })
    }
}

/// Zeile einer Listenansicht (ohne Notiztext).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryDto {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub status: TaskStatusDto,
    pub priority: PriorityDto,
    pub due: Option<DueDto>,
    pub has_notes: bool,
    pub sort_key: String,
    pub completed_at: Option<String>,
    /// Im Papierkorb seit (ADR-031; nur in der Ansicht „Papierkorb“ gesetzt).
    pub trashed_at: Option<String>,
    /// Wirksame Serie (N4) – Symbol in der Zeile.
    pub recurring: bool,
    /// Checkliste (N8): erledigte / alle Punkte („2/5“; 0 Punkte = keine Anzeige).
    pub checklist_done: u16,
    pub checklist_total: u16,
    /// Tag-IDs (ADR-035).
    pub tags: Vec<String>,
    /// Mit Notizen verknüpft (ADR-036).
    pub has_links: bool,
}

impl From<&TaskSummary> for TaskSummaryDto {
    fn from(s: &TaskSummary) -> Self {
        Self {
            id: s.id.to_string(),
            list_id: s.list_id.to_string(),
            title: s.title.clone(),
            status: s.status.into(),
            priority: s.priority.into(),
            due: s.due.as_ref().map(DueDto::from),
            has_notes: s.has_notes,
            sort_key: s.sort_key.to_string(),
            completed_at: s.completed_at.map(iso),
            trashed_at: s.trashed_at.map(iso),
            recurring: s.recurring,
            checklist_done: s.checklist_done,
            checklist_total: s.checklist_total,
            tags: s.tags.iter().map(ToString::to_string).collect(),
            has_links: s.has_links,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl From<Page<TaskSummary>> for PageDto<TaskSummaryDto> {
    fn from(page: Page<TaskSummary>) -> Self {
        Self {
            items: page.items.iter().map(TaskSummaryDto::from).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

// --- Eingaben ----------------------------------------------------------------------------------

/// Neue Task; fehlende Felder = Standard (leere Notiz, keine Priorität, nicht fällig).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskDto {
    pub title: String,
    /// `None` = Inbox.
    #[cfg_attr(test, ts(optional = nullable))]
    pub list_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub priority: Option<PriorityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub due: Option<DueDto>,
    /// Wiederholung; ohne `due` wird das erste Vorkommen ab heute die Fälligkeit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub recurrence: Option<RecurrenceDto>,
    /// Tag-IDs (ADR-035); Namen vorher mit `ensure_tag` anlegen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub tags: Option<Vec<String>>,
}

/// Zu ändernde Felder; fehlend = unverändert. `due: null` entfernt die Fälligkeit,
/// ein fehlendes `due` lässt sie unverändert.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskPatchDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub priority: Option<PriorityDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[cfg_attr(test, ts(as = "Option<DueDto>", optional = nullable))]
    pub due: Option<Option<DueDto>>,
    /// `null` beendet die Wiederholung.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[cfg_attr(test, ts(as = "Option<RecurrenceDto>", optional = nullable))]
    pub recurrence: Option<Option<RecurrenceDto>>,
    /// Ersetzt die ganze Checkliste (ADR-034); leere `id` = neuer Punkt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub checklist: Option<Vec<ChecklistItemDto>>,
    /// Ersetzt die Tags (IDs, ADR-035).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub tags: Option<Vec<String>>,
}

/// Unterscheidet bei `Option<Option<T>>` „Feld fehlt" (`None`) von „Feld ist null" (`Some(None)`).
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Option<T>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

/// Häufigkeit einer Wiederholung (N4, ADR-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum FrequencyDto {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum RecurrenceAnchorDto {
    /// Fester Rhythmus ab der Fälligkeit.
    Schedule,
    /// Ab dem Tag des Abhakens.
    Completion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum WeekdayDto {
    Mo,
    Tu,
    We,
    Th,
    Fr,
    Sa,
    Su,
}

const WEEKDAY_ORDER: [WeekdayDto; 7] = [
    WeekdayDto::Mo,
    WeekdayDto::Tu,
    WeekdayDto::We,
    WeekdayDto::Th,
    WeekdayDto::Fr,
    WeekdayDto::Sa,
    WeekdayDto::Su,
];

/// Wiederholungsregel (ADR-028). `weekdays` nur bei `weekly` (leer = Wochentag der Fälligkeit),
/// `monthDay` nur bei `monthly`/`yearly` (31 = letzter Tag), `until` = `YYYY-MM-DD`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecurrenceDto {
    pub freq: FrequencyDto,
    pub interval: u16,
    #[serde(default)]
    pub weekdays: Vec<WeekdayDto>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub month_day: Option<u8>,
    pub anchor: RecurrenceAnchorDto,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub until: Option<String>,
}

impl From<&Recurrence> for RecurrenceDto {
    fn from(r: &Recurrence) -> Self {
        Self {
            freq: match r.freq() {
                Frequency::Daily => FrequencyDto::Daily,
                Frequency::Weekly => FrequencyDto::Weekly,
                Frequency::Monthly => FrequencyDto::Monthly,
                Frequency::Yearly => FrequencyDto::Yearly,
            },
            interval: r.interval(),
            weekdays: r
                .weekdays()
                .iter()
                .map(|d| WEEKDAY_ORDER[d.num_days_from_monday() as usize])
                .collect(),
            month_day: r.month_day(),
            anchor: match r.anchor() {
                RecurrenceAnchor::Schedule => RecurrenceAnchorDto::Schedule,
                RecurrenceAnchor::Completion => RecurrenceAnchorDto::Completion,
            },
            until: r.until().map(|d| d.to_string()),
        }
    }
}

impl TryFrom<&RecurrenceDto> for Recurrence {
    type Error = CoreError;

    fn try_from(dto: &RecurrenceDto) -> CoreResult<Self> {
        let mut weekdays = WeekdaySet::EMPTY;
        for d in &dto.weekdays {
            let i = WEEKDAY_ORDER.iter().position(|w| w == d).unwrap_or(0);
            weekdays = weekdays.with(
                chrono::Weekday::try_from(u8::try_from(i).unwrap_or(0))
                    .unwrap_or(chrono::Weekday::Mon),
            );
        }
        let until = dto
            .until
            .as_deref()
            .map(|s| {
                s.parse::<NaiveDate>()
                    .map_err(|_| CoreError::Validation(format!("invalid date {s:?}")))
            })
            .transpose()?;
        Recurrence::new(
            match dto.freq {
                FrequencyDto::Daily => Frequency::Daily,
                FrequencyDto::Weekly => Frequency::Weekly,
                FrequencyDto::Monthly => Frequency::Monthly,
                FrequencyDto::Yearly => Frequency::Yearly,
            },
            dto.interval,
            weekdays,
            dto.month_day,
            match dto.anchor {
                RecurrenceAnchorDto::Schedule => RecurrenceAnchor::Schedule,
                RecurrenceAnchorDto::Completion => RecurrenceAnchor::Completion,
            },
            until,
        )
    }
}

/// Abgehaktes Vorkommen einer Serie (für „Rückgängig“ per `undo_occurrence`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct OccurrenceDto {
    pub series_id: String,
    /// `null`, wenn die Serie damit geendet hat.
    pub occurrence_id: Option<String>,
    pub previous_due: DueDto,
}

/// Ergebnis von `complete_tasks`: geänderte Tasks (Verlaufseinträge dahinter) und die Vorkommen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CompleteResultDto {
    pub tasks: Vec<TaskDto>,
    pub occurrences: Vec<OccurrenceDto>,
}

/// Ziel einer Verschiebung (Semantik wie `MoveTaskInput`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MoveTargetDto {
    /// `None` = bisherige Liste.
    #[cfg_attr(test, ts(optional = nullable))]
    pub list_id: Option<String>,
    #[cfg_attr(test, ts(optional = nullable))]
    pub after: Option<String>,
    #[cfg_attr(test, ts(optional = nullable))]
    pub before: Option<String>,
}

/// Neue Fälligkeit einer Task für `set_dues` (`due: null` = entfernen).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskDueDto {
    pub id: String,
    pub due: Option<DueDto>,
}

/// Ziel beim Umsortieren einer Liste (Semantik wie `MoveTaskListInput`; Nachbarn sind Listen-IDs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MoveListTargetDto {
    #[cfg_attr(test, ts(optional = nullable))]
    pub after: Option<String>,
    #[cfg_attr(test, ts(optional = nullable))]
    pub before: Option<String>,
}

/// Lokaler Tag, von der Shell in der Zeitzone des Nutzers berechnet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LocalDayDto {
    /// `YYYY-MM-DD`
    pub date: String,
    /// Beginn des Tages (ISO-8601).
    pub start: String,
    /// Beginn des Folgetags (ISO-8601).
    pub end: String,
}

impl TryFrom<&LocalDayDto> for LocalDay {
    type Error = CoreError;

    fn try_from(dto: &LocalDayDto) -> CoreResult<Self> {
        LocalDay::new(
            parse_date(&dto.date)?,
            parse_iso(&dto.start)?,
            parse_iso(&dto.end)?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ViewDto {
    Inbox,
    #[serde(rename_all = "camelCase")]
    List {
        list_id: String,
    },
    Today {
        day: LocalDayDto,
    },
    Upcoming {
        day: LocalDayDto,
    },
    All,
    Completed,
    /// Papierkorb (ADR-031).
    Trash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskQueryDto {
    pub view: ViewDto,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub status: Option<TaskStatusDto>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub tag: Option<String>,
}

impl TryFrom<&TaskQueryDto> for TaskQuery {
    type Error = CoreError;

    fn try_from(dto: &TaskQueryDto) -> CoreResult<Self> {
        let view = match &dto.view {
            ViewDto::Inbox => SmartView::Inbox,
            ViewDto::List { list_id: id } => SmartView::List(list_id(id)?),
            ViewDto::Today { day } => SmartView::Today(day.try_into()?),
            ViewDto::Upcoming { day } => SmartView::Upcoming(day.try_into()?),
            ViewDto::All => SmartView::All,
            ViewDto::Completed => SmartView::Completed,
            ViewDto::Trash => SmartView::Trash,
        };
        Ok(Self {
            view,
            status: dto.status.map(Into::into),
            tag: dto.tag.as_deref().map(TagId::parse).transpose()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageRequestDto {
    #[cfg_attr(test, ts(optional = nullable))]
    pub cursor: Option<String>,
    pub limit: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_roundtrip() {
        let ts = Timestamp::from_millis(1_760_710_496_123);
        assert_eq!(iso(ts), "2025-10-17T14:14:56.123Z");
        assert_eq!(parse_iso(&iso(ts)).unwrap(), ts);
        assert_eq!(parse_iso("2025-10-17T16:14:56.123+02:00").unwrap(), ts);
        assert!(parse_iso("gestern").is_err());
    }

    #[test]
    fn due_dto_roundtrip_and_json_shape() {
        let date = Due::Date(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        let dto = DueDto::from(&date);
        assert_eq!(
            serde_json::to_string(&dto).unwrap(),
            r#"{"kind":"date","date":"2026-10-20"}"#
        );
        assert_eq!(Due::try_from(dto).unwrap(), date);
        let dt = Due::DateTime {
            at: Timestamp::from_millis(0),
            tz: "UTC".into(),
        };
        assert_eq!(Due::try_from(DueDto::from(&dt)).unwrap(), dt);
        assert!(Due::try_from(DueDto::Date {
            date: "2026-13-01".into()
        })
        .is_err());
    }

    #[test]
    fn patch_distinguishes_missing_and_null_due() {
        let missing: TaskPatchDto = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(missing.due, None);
        let null: TaskPatchDto = serde_json::from_str(r#"{"due":null}"#).unwrap();
        assert_eq!(null.due, Some(None));
        let set: TaskPatchDto =
            serde_json::from_str(r#"{"due":{"kind":"date","date":"2026-10-20"}}"#).unwrap();
        assert_eq!(
            set.due,
            Some(Some(DueDto::Date {
                date: "2026-10-20".into()
            }))
        );
        assert_eq!(
            serde_json::to_string(&TaskPatchDto::default()).unwrap(),
            "{}"
        );
    }

    #[test]
    fn task_dto_roundtrips_to_parts() {
        let clock = crate::ports::FixedClock(Timestamp::from_millis(1_760_710_496_123));
        let mut task = Task::new(TaskListId::INBOX, "Milch", SortKey::first(), &clock).unwrap();
        task.set_notes("bio", &clock);
        task.set_due(
            Some(Due::Date(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())),
            &clock,
        )
        .unwrap();
        task.complete(&clock);
        let parts = TaskParts::try_from(&TaskDto::from(&task)).unwrap();
        assert_eq!(Task::from_parts(parts).unwrap(), task);
        let bad = TaskDto {
            sort_key: "?".into(),
            ..TaskDto::from(&task)
        };
        assert!(matches!(
            TaskParts::try_from(&bad),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn query_dto_parses_views() {
        let json = r#"{"view":{"kind":"today","day":{"date":"2026-10-17","start":"2026-10-16T22:00:00Z","end":"2026-10-17T22:00:00Z"}}}"#;
        let dto: TaskQueryDto = serde_json::from_str(json).unwrap();
        let query = TaskQuery::try_from(&dto).unwrap();
        assert!(matches!(query.view, SmartView::Today(_)));
        let list: TaskQueryDto =
            serde_json::from_str(r#"{"view":{"kind":"list","listId":"nope"}}"#).unwrap();
        assert!(matches!(
            TaskQuery::try_from(&list),
            Err(CoreError::Validation(_))
        ));
    }
}
