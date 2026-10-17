//! Die zentrale Entity `Task`.

use super::{text::normalize_name, Due, SortKey, TagId, TaskId, TaskListId, Timestamp};
use crate::{ports::Clock, CoreResult};

/// Maximale Titellänge in Unicode-Zeichen.
pub const TASK_TITLE_MAX_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TaskStatus {
    #[default]
    Open,
    Done,
}

/// Priorität; die Ordnung entspricht der Wichtigkeit (`None` < `Low` < … < `High`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Priority {
    #[default]
    None,
    Low,
    Medium,
    High,
}

/// Eine Aufgabe. Felder sind privat, damit Invarianten nur über Methoden geändert werden.
///
/// Mutierende Methoden setzen `updated_at` nur, wenn sich tatsächlich etwas ändert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    id: TaskId,
    list_id: TaskListId,
    parent_id: Option<TaskId>,
    title: String,
    notes: String,
    status: TaskStatus,
    priority: Priority,
    due: Option<Due>,
    tags: Vec<TagId>,
    sort_key: SortKey,
    created_at: Timestamp,
    updated_at: Timestamp,
    completed_at: Option<Timestamp>,
}

impl Task {
    /// Legt eine neue, offene Task an.
    ///
    /// # Errors
    /// `CoreError::Validation`, wenn der getrimmte Titel leer oder länger als
    /// [`TASK_TITLE_MAX_CHARS`] Zeichen ist.
    pub fn new(
        list_id: TaskListId,
        title: &str,
        sort_key: SortKey,
        clock: &(impl Clock + ?Sized),
    ) -> CoreResult<Self> {
        let title = normalize_title(title)?;
        let now = clock.now();
        Ok(Self {
            id: TaskId::new(),
            list_id,
            parent_id: None,
            title,
            notes: String::new(),
            status: TaskStatus::Open,
            priority: Priority::None,
            due: None,
            tags: Vec::new(),
            sort_key,
            created_at: now,
            updated_at: now,
            completed_at: None,
        })
    }

    #[must_use]
    pub fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub fn list_id(&self) -> TaskListId {
        self.list_id
    }

    #[must_use]
    pub fn parent_id(&self) -> Option<TaskId> {
        self.parent_id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn notes(&self) -> &str {
        &self.notes
    }

    #[must_use]
    pub fn status(&self) -> TaskStatus {
        self.status
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.status == TaskStatus::Done
    }

    #[must_use]
    pub fn priority(&self) -> Priority {
        self.priority
    }

    #[must_use]
    pub fn due(&self) -> Option<&Due> {
        self.due.as_ref()
    }

    #[must_use]
    pub fn tags(&self) -> &[TagId] {
        &self.tags
    }

    #[must_use]
    pub fn sort_key(&self) -> &SortKey {
        &self.sort_key
    }

    #[must_use]
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    #[must_use]
    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    #[must_use]
    pub fn completed_at(&self) -> Option<Timestamp> {
        self.completed_at
    }

    /// Ändert den Titel (gleiche Regeln wie in [`Task::new`]).
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Titel; die Task bleibt dann unverändert.
    pub fn rename(&mut self, title: &str, clock: &(impl Clock + ?Sized)) -> CoreResult<()> {
        let title = normalize_title(title)?;
        if title != self.title {
            self.title = title;
            self.touch(clock);
        }
        Ok(())
    }

    pub fn set_notes(&mut self, notes: &str, clock: &(impl Clock + ?Sized)) {
        if notes != self.notes {
            notes.clone_into(&mut self.notes);
            self.touch(clock);
        }
    }

    pub fn set_priority(&mut self, priority: Priority, clock: &(impl Clock + ?Sized)) {
        if priority != self.priority {
            self.priority = priority;
            self.touch(clock);
        }
    }

    /// Setzt oder entfernt die Fälligkeit.
    ///
    /// # Errors
    /// `CoreError::Validation` bei leerem oder zu langem Zeitzonen-Namen.
    pub fn set_due(&mut self, due: Option<Due>, clock: &(impl Clock + ?Sized)) -> CoreResult<()> {
        let due = due.map(Due::normalized).transpose()?;
        if due != self.due {
            self.due = due;
            self.touch(clock);
        }
        Ok(())
    }

    /// Markiert die Task als erledigt. Auf einer erledigten Task ohne Wirkung
    /// (der ursprüngliche `completed_at` bleibt erhalten).
    pub fn complete(&mut self, clock: &(impl Clock + ?Sized)) {
        if self.status == TaskStatus::Done {
            return;
        }
        let now = clock.now();
        self.status = TaskStatus::Done;
        self.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Öffnet eine erledigte Task wieder und löscht `completed_at`.
    pub fn reopen(&mut self, clock: &(impl Clock + ?Sized)) {
        if self.status == TaskStatus::Open {
            return;
        }
        self.status = TaskStatus::Open;
        self.completed_at = None;
        self.touch(clock);
    }

    fn touch(&mut self, clock: &(impl Clock + ?Sized)) {
        self.updated_at = clock.now();
    }
}

fn normalize_title(title: &str) -> CoreResult<String> {
    normalize_name(title, TASK_TITLE_MAX_CHARS, "task title")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::{FixedClock, SteppingClock},
        CoreError,
    };

    fn clock() -> SteppingClock {
        SteppingClock::new(Timestamp::from_millis(1_000), 1)
    }

    fn task(clock: &SteppingClock) -> Task {
        Task::new(TaskListId::new(), "Milch kaufen", SortKey::first(), clock).unwrap()
    }

    #[test]
    fn new_task_has_sane_defaults() {
        let list = TaskListId::new();
        let clock = FixedClock(Timestamp::from_millis(7));
        let t = Task::new(list, "  Milch kaufen \n", SortKey::first(), &clock).unwrap();
        assert_eq!(t.title(), "Milch kaufen");
        assert_eq!(t.list_id(), list);
        assert_eq!(t.status(), TaskStatus::Open);
        assert!(!t.is_done());
        assert_eq!(t.priority(), Priority::None);
        assert_eq!(t.notes(), "");
        assert_eq!(t.due(), None);
        assert!(t.tags().is_empty());
        assert_eq!(t.parent_id(), None);
        assert_eq!(t.sort_key(), &SortKey::first());
        assert_eq!(t.created_at(), Timestamp::from_millis(7));
        assert_eq!(t.updated_at(), t.created_at());
        assert_eq!(t.completed_at(), None);
    }

    #[test]
    fn new_tasks_get_distinct_ids() {
        let clock = clock();
        assert_ne!(task(&clock).id(), task(&clock).id());
    }

    #[test]
    fn new_rejects_invalid_titles() {
        let clock = clock();
        let list = TaskListId::new();
        let too_long = "x".repeat(TASK_TITLE_MAX_CHARS + 1);
        for title in ["", "   ", "\t\n", too_long.as_str()] {
            let err = Task::new(list, title, SortKey::first(), &clock).unwrap_err();
            assert!(
                matches!(err, CoreError::Validation(_)),
                "{title:?} → {err:?}"
            );
        }
    }

    #[test]
    fn title_limit_counts_chars_not_bytes() {
        let clock = clock();
        let list = TaskListId::new();
        let max = "ü".repeat(TASK_TITLE_MAX_CHARS); // 1000 Bytes, 500 Zeichen
        assert!(Task::new(list, &max, SortKey::first(), &clock).is_ok());
        let emoji = "🦀".repeat(TASK_TITLE_MAX_CHARS + 1);
        assert!(Task::new(list, &emoji, SortKey::first(), &clock).is_err());
    }

    #[test]
    fn rename_trims_and_touches() {
        let clock = clock();
        let mut t = task(&clock);
        let before = t.updated_at();
        t.rename("  Hafermilch  ", &clock).unwrap();
        assert_eq!(t.title(), "Hafermilch");
        assert!(t.updated_at() > before);
    }

    #[test]
    fn rename_rejects_invalid_and_keeps_state() {
        let clock = clock();
        let mut t = task(&clock);
        let snapshot = t.clone();
        assert!(t.rename("   ", &clock).is_err());
        assert!(t.rename(&"a".repeat(501), &clock).is_err());
        assert_eq!(t, snapshot);
    }

    #[test]
    fn rename_to_same_title_is_noop() {
        let clock = clock();
        let mut t = task(&clock);
        let snapshot = t.clone();
        t.rename(" Milch kaufen ", &clock).unwrap();
        assert_eq!(t, snapshot);
    }

    #[test]
    fn complete_sets_completed_at() {
        let clock = clock();
        let mut t = task(&clock);
        t.complete(&clock);
        assert!(t.is_done());
        assert_eq!(t.completed_at(), Some(t.updated_at()));
        assert!(t.updated_at() > t.created_at());
    }

    #[test]
    fn complete_twice_keeps_first_completion() {
        let clock = clock();
        let mut t = task(&clock);
        t.complete(&clock);
        let snapshot = t.clone();
        t.complete(&clock);
        assert_eq!(t, snapshot);
    }

    #[test]
    fn reopen_clears_completed_at() {
        let clock = clock();
        let mut t = task(&clock);
        t.complete(&clock);
        let completed = t.updated_at();
        t.reopen(&clock);
        assert_eq!(t.status(), TaskStatus::Open);
        assert_eq!(t.completed_at(), None);
        assert!(t.updated_at() > completed);
    }

    #[test]
    fn reopen_open_task_is_noop() {
        let clock = clock();
        let mut t = task(&clock);
        let snapshot = t.clone();
        t.reopen(&clock);
        assert_eq!(t, snapshot);
    }

    #[test]
    fn set_notes_touches_only_on_change() {
        let clock = clock();
        let mut t = task(&clock);
        t.set_notes("2 Liter, **bio**", &clock);
        assert_eq!(t.notes(), "2 Liter, **bio**");
        let snapshot = t.clone();
        t.set_notes("2 Liter, **bio**", &clock);
        assert_eq!(t, snapshot);
        assert!(t.updated_at() > t.created_at());
    }

    #[test]
    fn set_priority_touches_only_on_change() {
        let clock = clock();
        let mut t = task(&clock);
        t.set_priority(Priority::High, &clock);
        assert_eq!(t.priority(), Priority::High);
        let snapshot = t.clone();
        t.set_priority(Priority::High, &clock);
        assert_eq!(t, snapshot);
    }

    #[test]
    fn set_due_sets_clears_and_touches_only_on_change() {
        let clock = clock();
        let mut t = task(&clock);
        let date = Due::Date(crate::domain::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        t.set_due(Some(date.clone()), &clock).unwrap();
        assert_eq!(t.due(), Some(&date));
        let snapshot = t.clone();
        t.set_due(Some(date), &clock).unwrap();
        assert_eq!(t, snapshot);
        t.set_due(None, &clock).unwrap();
        assert_eq!(t.due(), None);
        assert!(t.updated_at() > snapshot.updated_at());
    }

    #[test]
    fn set_due_rejects_invalid_time_zone_and_keeps_state() {
        let clock = clock();
        let mut t = task(&clock);
        let snapshot = t.clone();
        let due = Due::DateTime {
            at: Timestamp::EPOCH,
            tz: String::new(),
        };
        assert!(t.set_due(Some(due), &clock).is_err());
        assert_eq!(t, snapshot);
    }

    #[test]
    fn priority_orders_by_importance() {
        assert!(Priority::None < Priority::Low);
        assert!(Priority::Low < Priority::Medium);
        assert!(Priority::Medium < Priority::High);
    }

    #[test]
    fn accepts_dyn_clock() {
        let clock: Box<dyn Clock> = Box::new(FixedClock(Timestamp::EPOCH));
        let t = Task::new(TaskListId::new(), "x", SortKey::first(), clock.as_ref());
        assert!(t.is_ok());
    }
}
