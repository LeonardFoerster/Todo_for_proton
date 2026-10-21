//! Wiederkehrende Tasks (N4, ADR-028): nächstes Vorkommen berechnen, Verlaufseinträge anlegen,
//! Überspringen und Rückgängig. Uhrzeiten über den Port `TimeZones` (gleiche lokale Uhrzeit).

use chrono::{NaiveDate, NaiveTime};

use super::{load_task, Deps};
use crate::{
    domain::{Due, Recurrence, SortKey, Task, TaskId, TaskParts, TaskStatus, Timestamp},
    ports::{Change, ChangeSet, EntityRef, TaskRepository, TimeZones},
    sync::EntityState,
    CoreError, CoreResult,
};

/// Lokaler Tag (und Uhrzeit) einer Fälligkeit.
pub(crate) fn due_local(
    due: &Due,
    zones: Option<&dyn TimeZones>,
) -> CoreResult<(NaiveDate, Option<NaiveTime>)> {
    match due {
        Due::Date(day) => Ok((*day, None)),
        Due::DateTime { at, tz } => {
            let zones = zones.ok_or_else(no_zones)?;
            let local = zones
                .to_local(*at, tz)
                .ok_or_else(|| CoreError::Validation(format!("unknown time zone {tz:?}")))?;
            Ok((local.date(), Some(local.time())))
        }
    }
}

/// Fälligkeit am lokalen Tag `day` mit der Uhrzeit/Zone von `like`.
fn due_on(day: NaiveDate, like: &Due, zones: Option<&dyn TimeZones>) -> CoreResult<Due> {
    match like {
        Due::Date(_) => Ok(Due::Date(day)),
        Due::DateTime { at, tz } => {
            let zones = zones.ok_or_else(no_zones)?;
            let time = zones
                .to_local(*at, tz)
                .ok_or_else(|| CoreError::Validation(format!("unknown time zone {tz:?}")))?
                .time();
            let at = zones
                .to_utc(day.and_time(time), tz)
                .ok_or_else(|| CoreError::Validation(format!("unknown time zone {tz:?}")))?;
            Ok(Due::DateTime { at, tz: tz.clone() })
        }
    }
}

fn no_zones() -> CoreError {
    CoreError::Validation("recurring tasks with a time need time zones (not available here)".into())
}

/// Heute für den Use-Case: vom Aufrufer, sonst das UTC-Datum der Uhr (CLI, Tests).
pub(crate) fn today_or(today: Option<NaiveDate>, now: Timestamp) -> NaiveDate {
    today.unwrap_or_else(|| {
        chrono::DateTime::from_timestamp_millis(now.as_millis())
            .map_or(NaiveDate::MIN, |t| t.date_naive())
    })
}

/// Setzt eine Regel: ohne Fälligkeit zuerst das erste Vorkommen ab heute (ganztägig), dann die
/// Regel an der Fälligkeit verankern (Wochentag/Monatstag) und prüfen.
pub(crate) fn apply_rule(
    task: &mut Task,
    rule: &Recurrence,
    zones: Option<&dyn TimeZones>,
    clock: &dyn crate::ports::Clock,
) -> CoreResult<()> {
    if task.due().is_none() {
        let first = rule
            .first_on_or_after(today_or(None, clock.now()))
            .ok_or_else(|| CoreError::Validation("recurrence ends before today".into()))?;
        task.set_due(Some(Due::Date(first)), clock)?;
    }
    let due = task.due().cloned().expect("set above");
    let (day, _) = due_local(&due, zones)?;
    task.set_recurrence(Some(rule.anchored(day)), Some(day), clock)
}

/// Nächste Fälligkeit einer Serie beim Abhaken/Überspringen (`None` = Serie endet).
pub(crate) fn next_due(
    task: &Task,
    today: NaiveDate,
    zones: Option<&dyn TimeZones>,
) -> CoreResult<Option<Due>> {
    let (Some(rule), Some(due)) = (task.recurrence(), task.due()) else {
        return Ok(None);
    };
    let (day, _) = due_local(due, zones)?;
    rule.next_occurrence(day, today)
        .map(|next| due_on(next, due, zones))
        .transpose()
}

/// ID des Verlaufseintrags: deterministisch aus Serie und Tag; ist sie schon gelöscht (nach
/// „Rückgängig“), die nächste Variante – sonst verschwände ein erneutes Abhaken sofort wieder.
pub(crate) async fn occurrence_id(
    repo: &dyn TaskRepository,
    series: TaskId,
    day: NaiveDate,
) -> CoreResult<TaskId> {
    let series_bytes = series.to_string();
    let day_text = day.to_string();
    for attempt in 0u8..16 {
        let id = TaskId::derived(&[
            series_bytes.as_bytes(),
            b"|",
            day_text.as_bytes(),
            b"|",
            &[attempt],
        ]);
        match repo.entity_state(EntityRef::Task(id)).await? {
            Some(EntityState::Deleted(_)) => {}
            _ => return Ok(id),
        }
    }
    Err(CoreError::Conflict(format!(
        "no free occurrence id for {series} on {day}"
    )))
}

/// Verlaufseintrag: erledigte Kopie des Vorkommens ohne Regel.
pub(crate) fn occurrence_of(
    series: &Task,
    id: TaskId,
    sort_key: SortKey,
    now: Timestamp,
) -> CoreResult<Task> {
    Task::from_parts(TaskParts {
        id,
        list_id: series.list_id(),
        parent_id: None,
        title: series.title().to_owned(),
        notes: series.notes().to_owned(),
        status: TaskStatus::Done,
        priority: series.priority(),
        due: series.due().cloned(),
        tags: series.tags().to_vec(),
        sort_key,
        created_at: now,
        updated_at: now,
        completed_at: Some(now),
        trashed_at: None,
        recurrence: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipOccurrenceInput {
    pub id: TaskId,
    /// Lokales Datum des Nutzers (`None` = UTC-Datum der Uhr).
    pub today: Option<NaiveDate>,
}

/// „Dieses Mal überspringen“: nur fortschreiben, kein Verlaufseintrag. Endet die Serie damit,
/// wird sie erledigt.
#[derive(Debug, Clone)]
pub struct SkipOccurrence {
    deps: Deps,
}

impl SkipOccurrence {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `NotFound`, `Validation` (keine Serie, Zeitzonen fehlen); Fehler des Repositories.
    pub async fn execute(&self, input: SkipOccurrenceInput) -> CoreResult<Task> {
        let repo = self.deps.repo.as_ref();
        let mut task = load_task(repo, input.id).await?;
        if !task.is_recurring() || task.is_done() {
            return Err(CoreError::Validation(format!(
                "task {} is not an open recurring task",
                input.id
            )));
        }
        let clock = self.deps.clock.as_ref();
        let today = today_or(input.today, clock.now());
        match next_due(&task, today, self.deps.zones.as_deref())? {
            Some(next) => task.set_due(Some(next), clock)?,
            None => task.complete(clock),
        }
        repo.commit(ChangeSet::new().upsert_task(task.clone()))
            .await?;
        Ok(task)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOccurrenceInput {
    pub series: TaskId,
    /// Verlaufseintrag aus dem Abhaken (`None`, wenn die Serie dabei geendet hat).
    pub occurrence: Option<TaskId>,
    /// Fälligkeit der Serie vor dem Abhaken.
    pub previous_due: Option<Due>,
}

/// Rückgängig nach dem Abhaken einer Serie: Verlaufseintrag löschen, Serie wieder auf ihre alte
/// Fälligkeit (und offen) setzen – ein Commit.
#[derive(Debug, Clone)]
pub struct UndoOccurrence {
    deps: Deps,
}

impl UndoOccurrence {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `NotFound` (Serie), `Validation` (Fälligkeit); Fehler des Repositories.
    pub async fn execute(&self, input: UndoOccurrenceInput) -> CoreResult<Task> {
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let mut series = load_task(repo, input.series).await?;
        series.reopen(clock);
        series.set_due(input.previous_due, clock)?;
        let mut changes = ChangeSet::new().upsert_task(series.clone());
        if let Some(occ) = input.occurrence {
            if repo.get_task(occ).await?.is_some() {
                changes.push(Change::Delete(EntityRef::Task(occ)));
            }
        }
        repo.commit(changes).await?;
        Ok(series)
    }
}
