//! Checkliste einer Task (N8, ADR-034): kleine Punkte mit Text und Haken, in fester Reihenfolge.
//! Synchronisiert als **ein** LWW-Feld (wie die Tags in v1) – parallele Änderungen an derselben
//! Liste: die spätere gewinnt ganz.

use super::{text::normalize_name, ChecklistItemId};
use crate::{CoreError, CoreResult};

/// Höchstens so viele Punkte je Task.
pub const CHECKLIST_MAX_ITEMS: usize = 100;
/// Maximale Länge eines Punkts in Unicode-Zeichen.
pub const CHECKLIST_ITEM_MAX_CHARS: usize = 500;

/// Ein Punkt der Checkliste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    id: ChecklistItemId,
    text: String,
    done: bool,
}

impl ChecklistItem {
    /// # Errors
    /// `CoreError::Validation` bei leerem oder zu langem Text (einzeilig wie ein Titel).
    pub fn new(id: ChecklistItemId, text: &str, done: bool) -> CoreResult<Self> {
        Ok(Self {
            id,
            text: normalize_name(text, CHECKLIST_ITEM_MAX_CHARS, "checklist item")?,
            done,
        })
    }

    #[must_use]
    pub fn id(&self) -> ChecklistItemId {
        self.id
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    pub(crate) fn set_done(&mut self, done: bool) {
        self.done = done;
    }
}

/// Prüft eine ganze Liste: höchstens [`CHECKLIST_MAX_ITEMS`], jede ID nur einmal.
///
/// # Errors
/// `CoreError::Validation`.
pub fn check_checklist(items: &[ChecklistItem]) -> CoreResult<()> {
    if items.len() > CHECKLIST_MAX_ITEMS {
        return Err(CoreError::Validation(format!(
            "checklist must have at most {CHECKLIST_MAX_ITEMS} items, got {}",
            items.len()
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(items.len());
    if let Some(dup) = items.iter().find(|i| !seen.insert(i.id)) {
        return Err(CoreError::Validation(format!(
            "duplicate checklist item {}",
            dup.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_single_line_and_bounded() {
        let item = ChecklistItem::new(ChecklistItemId::new(), "  Milch\nholen ", false).unwrap();
        assert_eq!(item.text(), "Milch holen");
        assert!(ChecklistItem::new(ChecklistItemId::new(), " ", false).is_err());
        assert!(ChecklistItem::new(ChecklistItemId::new(), &"x".repeat(501), false).is_err());
    }

    #[test]
    fn lists_are_bounded_and_ids_unique() {
        let id = ChecklistItemId::new();
        let a = ChecklistItem::new(id, "a", false).unwrap();
        assert!(check_checklist(&[a.clone(), a.clone()]).is_err());
        let many: Vec<_> = (0..=CHECKLIST_MAX_ITEMS)
            .map(|i| ChecklistItem::new(ChecklistItemId::new(), &i.to_string(), false).unwrap())
            .collect();
        assert!(check_checklist(&many).is_err());
        assert!(check_checklist(&many[..CHECKLIST_MAX_ITEMS]).is_ok());
    }
}
