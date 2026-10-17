//! Sortierschlüssel für die manuelle Reihenfolge (Fractional Index, siehe `sync-strategy.md`).

/// Lexikografisch vergleichbarer Sortierschlüssel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SortKey(String);

impl SortKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl SortKey {
    /// Nur für Tests, bis `SortKey::first()`/`between()` existieren (P1-06).
    pub(crate) fn for_test(raw: &str) -> Self {
        Self(raw.to_owned())
    }
}
