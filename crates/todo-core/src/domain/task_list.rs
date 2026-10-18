//! Listen („Privat", „Arbeit") plus die feste Inbox.

use super::{text::normalize_name, SortKey, TaskListId, Timestamp};
use crate::{ports::Clock, CoreResult};

/// Maximale Länge eines Listennamens in Unicode-Zeichen.
pub const TASK_LIST_NAME_MAX_CHARS: usize = 100;

/// Interner Name der Inbox; die UI zeigt eine übersetzte Bezeichnung an.
pub const INBOX_NAME: &str = "Inbox";

/// Eine Liste von Tasks. Es gibt genau eine Inbox; sie darf weder gelöscht noch umbenannt
/// werden. Diese Regeln setzen die Use-Cases durch, die Entity stellt nur Flag und
/// Abfragen bereit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskList {
    id: TaskListId,
    name: String,
    sort_key: SortKey,
    created_at: Timestamp,
    updated_at: Timestamp,
    is_inbox: bool,
}

/// Alle Felder einer gespeicherten Liste, zum Wiederherstellen per [`TaskList::from_parts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskListParts {
    pub id: TaskListId,
    pub name: String,
    pub sort_key: SortKey,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub is_inbox: bool,
}

impl TaskList {
    /// Stellt eine Liste aus gespeicherten Feldern wieder her; der Name wird wie bei der Eingabe
    /// geprüft und normalisiert.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    pub fn from_parts(parts: TaskListParts) -> CoreResult<Self> {
        Ok(Self {
            id: parts.id,
            name: normalize_list_name(&parts.name)?,
            sort_key: parts.sort_key,
            created_at: parts.created_at,
            updated_at: parts.updated_at,
            is_inbox: parts.is_inbox,
        })
    }

    /// Legt eine normale Liste an.
    ///
    /// # Errors
    /// `CoreError::Validation`, wenn der getrimmte Name leer oder länger als
    /// [`TASK_LIST_NAME_MAX_CHARS`] Zeichen ist.
    pub fn new(name: &str, sort_key: SortKey, clock: &(impl Clock + ?Sized)) -> CoreResult<Self> {
        let name = normalize_list_name(name)?;
        Ok(Self::build(TaskListId::new(), name, sort_key, clock, false))
    }

    /// Legt die Inbox an (feste ID [`TaskListId::INBOX`]). Nur beim Einrichten eines neuen
    /// Datenbestands aufrufen.
    #[must_use]
    pub fn inbox(sort_key: SortKey, clock: &(impl Clock + ?Sized)) -> Self {
        Self::build(
            TaskListId::INBOX,
            INBOX_NAME.to_owned(),
            sort_key,
            clock,
            true,
        )
    }

    fn build(
        id: TaskListId,
        name: String,
        sort_key: SortKey,
        clock: &(impl Clock + ?Sized),
        is_inbox: bool,
    ) -> Self {
        let now = clock.now();
        Self {
            id,
            name,
            sort_key,
            created_at: now,
            updated_at: now,
            is_inbox,
        }
    }

    #[must_use]
    pub fn id(&self) -> TaskListId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
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
    pub fn is_inbox(&self) -> bool {
        self.is_inbox
    }

    /// Darf diese Liste gelöscht werden? Die Inbox nie.
    #[must_use]
    pub fn can_delete(&self) -> bool {
        !self.is_inbox
    }

    /// Darf diese Liste umbenannt werden? Die Inbox nie.
    #[must_use]
    pub fn can_rename(&self) -> bool {
        !self.is_inbox
    }

    /// Die Inbox steht immer zuerst und lässt sich nicht umsortieren (ADR-023).
    #[must_use]
    pub fn can_move(&self) -> bool {
        !self.is_inbox
    }

    /// Setzt die Position (`sort_key`) unter den Listen; `updated_at` nur bei Änderung.
    pub fn move_to(&mut self, sort_key: SortKey, clock: &(impl Clock + ?Sized)) {
        if sort_key != self.sort_key {
            self.sort_key = sort_key;
            self.updated_at = clock.now();
        }
    }

    /// Ändert den Namen (gleiche Regeln wie in [`TaskList::new`]). Ob umbenannt werden
    /// darf ([`TaskList::can_rename`]), prüft der Use-Case.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen; die Liste bleibt dann unverändert.
    pub fn rename(&mut self, name: &str, clock: &(impl Clock + ?Sized)) -> CoreResult<()> {
        let name = normalize_list_name(name)?;
        if name != self.name {
            self.name = name;
            self.updated_at = clock.now();
        }
        Ok(())
    }
}

fn normalize_list_name(name: &str) -> CoreResult<String> {
    normalize_name(name, TASK_LIST_NAME_MAX_CHARS, "list name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::{FixedClock, SteppingClock},
        CoreError,
    };

    fn clock() -> SteppingClock {
        SteppingClock::new(Timestamp::from_millis(500), 1)
    }

    #[test]
    fn move_to_changes_key_and_touches_only_on_change() {
        let c = clock();
        let mut list = TaskList::new("A", SortKey::first(), &c).unwrap();
        let before = list.clone();
        list.move_to(SortKey::first(), &c);
        assert_eq!(list, before, "gleicher Key: unverändert");
        let key = SortKey::between(Some(&SortKey::first()), None).unwrap();
        list.move_to(key.clone(), &c);
        assert_eq!(list.sort_key(), &key);
        assert!(list.updated_at() > before.updated_at());
        assert!(list.can_move());
        assert!(!TaskList::inbox(SortKey::first(), &c).can_move());
    }

    #[test]
    fn new_list_trims_name_and_sets_timestamps() {
        let clock = FixedClock(Timestamp::from_millis(9));
        let list = TaskList::new("  Arbeit ", SortKey::first(), &clock).unwrap();
        assert_eq!(list.name(), "Arbeit");
        assert_eq!(list.sort_key(), &SortKey::first());
        assert_eq!(list.created_at(), Timestamp::from_millis(9));
        assert_eq!(list.updated_at(), list.created_at());
        assert!(!list.is_inbox());
        assert!(list.can_delete());
        assert!(list.can_rename());
    }

    #[test]
    fn new_rejects_invalid_names() {
        let clock = clock();
        let too_long = "x".repeat(TASK_LIST_NAME_MAX_CHARS + 1);
        for name in ["", "  ", too_long.as_str()] {
            let err = TaskList::new(name, SortKey::first(), &clock).unwrap_err();
            assert!(matches!(err, CoreError::Validation(_)), "{name:?}");
        }
    }

    #[test]
    fn name_limit_counts_chars_not_bytes() {
        let clock = clock();
        let max = "ß".repeat(TASK_LIST_NAME_MAX_CHARS);
        assert!(TaskList::new(&max, SortKey::first(), &clock).is_ok());
    }

    #[test]
    fn inbox_cannot_be_deleted_or_renamed() {
        let inbox = TaskList::inbox(SortKey::first(), &clock());
        assert!(inbox.is_inbox());
        assert_eq!(inbox.name(), INBOX_NAME);
        assert!(!inbox.can_delete());
        assert!(!inbox.can_rename());
    }

    #[test]
    fn inbox_has_the_same_id_everywhere() {
        let a = TaskList::inbox(SortKey::first(), &clock());
        let b = TaskList::inbox(SortKey::first(), &clock());
        assert_eq!(a.id(), TaskListId::INBOX);
        assert_eq!(a.id(), b.id());
        assert_eq!(
            TaskListId::INBOX.to_string(),
            "00000000-0000-7000-8000-000000000001"
        );
    }

    #[test]
    fn lists_get_distinct_ids() {
        let clock = clock();
        let a = TaskList::new("A", SortKey::first(), &clock).unwrap();
        let b = TaskList::new("A", SortKey::first(), &clock).unwrap();
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn rename_touches_only_on_change() {
        let clock = clock();
        let mut list = TaskList::new("Privat", SortKey::first(), &clock).unwrap();
        list.rename(" Privat ", &clock).unwrap();
        assert_eq!(list.updated_at(), list.created_at());
        list.rename("Zuhause", &clock).unwrap();
        assert_eq!(list.name(), "Zuhause");
        assert!(list.updated_at() > list.created_at());
    }

    #[test]
    fn from_parts_roundtrips_and_validates_name() {
        let clock = clock();
        for list in [
            TaskList::new("Arbeit", SortKey::first(), &clock).unwrap(),
            TaskList::inbox(SortKey::first(), &clock),
        ] {
            let parts = TaskListParts {
                id: list.id(),
                name: list.name().to_owned(),
                sort_key: list.sort_key().clone(),
                created_at: list.created_at(),
                updated_at: list.updated_at(),
                is_inbox: list.is_inbox(),
            };
            assert_eq!(TaskList::from_parts(parts.clone()).unwrap(), list);
            let err = TaskList::from_parts(TaskListParts {
                name: "  ".into(),
                ..parts
            })
            .unwrap_err();
            assert!(matches!(err, CoreError::Validation(_)));
        }
    }

    #[test]
    fn rename_rejects_invalid_and_keeps_state() {
        let clock = clock();
        let mut list = TaskList::new("Privat", SortKey::first(), &clock).unwrap();
        let snapshot = list.clone();
        assert!(list.rename(" ", &clock).is_err());
        assert_eq!(list, snapshot);
    }
}
