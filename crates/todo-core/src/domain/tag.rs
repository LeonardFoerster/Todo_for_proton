//! Tags (N8-02, ADR-035): Schlagworte quer zu den Listen („#uni“, „#einkauf“). Eine Task trägt
//! beliebig viele (höchstens [`TASK_TAGS_MAX`]); ein Tag ist eine eigene synchronisierte Entity.

use super::{text::normalize_name, TagId, Timestamp};
use crate::{ports::Clock, CoreError, CoreResult};

/// Maximale Länge eines Tag-Namens in Unicode-Zeichen.
pub const TAG_NAME_MAX_CHARS: usize = 50;
/// Höchstens so viele Tags je Task.
pub const TASK_TAGS_MAX: usize = 20;

/// Ein Tag. Der Name ist ein Wort ohne Leerzeichen und ohne führendes `#` (die UI zeigt es an).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    id: TagId,
    name: String,
    created_at: Timestamp,
    updated_at: Timestamp,
}

/// Alle Felder eines gespeicherten Tags, zum Wiederherstellen per [`Tag::from_parts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagParts {
    pub id: TagId,
    pub name: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Prüft und normalisiert einen Tag-Namen: `#` vorn fällt weg, Leerzeichen werden zu `-`.
///
/// # Errors
/// `CoreError::Validation` bei leerem oder zu langem Namen.
pub fn normalize_tag_name(input: &str) -> CoreResult<String> {
    let name = normalize_name(
        input.trim().trim_start_matches('#'),
        TAG_NAME_MAX_CHARS,
        "tag name",
    )?;
    Ok(name.split_whitespace().collect::<Vec<_>>().join("-"))
}

/// Schlüssel für „gleicher Name“ (Groß/Klein egal) – Grundlage der abgeleiteten ID.
#[must_use]
pub fn tag_key(name: &str) -> String {
    name.to_lowercase()
}

impl Tag {
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    #[allow(clippy::needless_pass_by_value)] // gleiche Form wie die übrigen `from_parts`
    pub fn from_parts(parts: TagParts) -> CoreResult<Self> {
        Ok(Self {
            id: parts.id,
            name: normalize_tag_name(&parts.name)?,
            created_at: parts.created_at,
            updated_at: parts.updated_at,
        })
    }

    /// Neuer Tag mit vorgegebener ID (der Use-Case leitet sie aus dem Namen ab, ADR-035).
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    pub fn new(id: TagId, name: &str, clock: &(impl Clock + ?Sized)) -> CoreResult<Self> {
        let now = clock.now();
        Ok(Self {
            id,
            name: normalize_tag_name(name)?,
            created_at: now,
            updated_at: now,
        })
    }

    #[must_use]
    pub fn id(&self) -> TagId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    #[must_use]
    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen.
    pub fn rename(&mut self, name: &str, clock: &(impl Clock + ?Sized)) -> CoreResult<()> {
        let name = normalize_tag_name(name)?;
        if name != self.name {
            self.name = name;
            self.updated_at = clock.now();
        }
        Ok(())
    }
}

/// Prüft die Tags einer Task: höchstens [`TASK_TAGS_MAX`], keine doppelten.
///
/// # Errors
/// `CoreError::Validation`.
pub fn check_task_tags(tags: &[TagId]) -> CoreResult<()> {
    if tags.len() > TASK_TAGS_MAX {
        return Err(CoreError::Validation(format!(
            "a task can have at most {TASK_TAGS_MAX} tags, got {}",
            tags.len()
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(tags.len());
    if let Some(dup) = tags.iter().find(|t| !seen.insert(**t)) {
        return Err(CoreError::Validation(format!("duplicate tag {dup}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FixedClock;

    #[test]
    fn names_are_single_words_without_hash() {
        assert_eq!(normalize_tag_name("#Uni").unwrap(), "Uni");
        assert_eq!(normalize_tag_name("  zu Hause ").unwrap(), "zu-Hause");
        assert!(normalize_tag_name("#").is_err());
        assert!(normalize_tag_name(&"x".repeat(51)).is_err());
        assert_eq!(tag_key("Uni"), tag_key("uNI"));
    }

    #[test]
    fn rename_and_task_tag_rules() {
        let clock = FixedClock(Timestamp::from_millis(5));
        let mut tag = Tag::new(TagId::new(), "uni", &clock).unwrap();
        tag.rename("#studium", &clock).unwrap();
        assert_eq!(tag.name(), "studium");
        let id = TagId::new();
        assert!(check_task_tags(&[id, id]).is_err());
        assert!(check_task_tags(&(0..21).map(|_| TagId::new()).collect::<Vec<_>>()).is_err());
        assert!(check_task_tags(&[id]).is_ok());
    }
}
