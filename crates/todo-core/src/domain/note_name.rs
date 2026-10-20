//! Name einer Notiz oder eines Ordners – wie ein Dateiname, aber ohne Endung (`notes.md` §3.1).

use std::fmt;

use super::text::normalize_name;
use crate::{CoreError, CoreResult};

/// Maximale Länge eines Notiz- oder Ordnernamens in Unicode-Zeichen.
pub const NOTE_NAME_MAX_CHARS: usize = 200;

/// Name einer neuen Notiz, solange ihr Inhalt keinen besseren hergibt.
pub const UNTITLED_NOTE_NAME: &str = "Unbenannt";

/// So viele Zeichen der ersten Zeile übernimmt der Auto-Name.
const AUTO_NAME_MAX_CHARS: usize = 60;

/// Ein geprüfter Name: getrimmt, 1–200 Zeichen, ohne `/`, `\`, Steuerzeichen und ohne
/// führenden Punkt. Namen sind **nicht** eindeutig (der Sync kann das nicht garantieren).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteName(String);

impl NoteName {
    /// Prüft eine Eingabe (Umbenennen, Anlegen, Sync).
    ///
    /// # Errors
    /// `CoreError::Validation` bei leerem, zu langem Namen, `/`, `\` oder führendem Punkt.
    pub fn parse(input: &str) -> CoreResult<Self> {
        let name = normalize_name(input, NOTE_NAME_MAX_CHARS, "note name")?;
        if name.contains(['/', '\\']) {
            return Err(CoreError::Validation(
                "note name must not contain / or \\".into(),
            ));
        }
        if name.starts_with('.') {
            return Err(CoreError::Validation(
                "note name must not start with a dot".into(),
            ));
        }
        Ok(Self(name))
    }

    /// „Unbenannt".
    #[must_use]
    pub fn untitled() -> Self {
        Self(UNTITLED_NOTE_NAME.to_owned())
    }

    /// Auto-Name aus dem Inhalt: erste nicht leere Zeile ohne führende Markdown-Zeichen
    /// (`#`, `-`, `*`, `>`, `+`, Checkbox), auf 60 Zeichen gekürzt; sonst „Unbenannt".
    /// Unzulässige Zeichen werden ersetzt statt abgelehnt. Deterministisch (alle Geräte gleich).
    #[must_use]
    pub fn from_first_line(text: &str) -> Self {
        let Some(line) = text
            .lines()
            .map(strip_markers)
            .find(|line| !line.trim().is_empty())
        else {
            return Self::untitled();
        };
        let cleaned: String = line
            .chars()
            .map(|c| if matches!(c, '/' | '\\') { '-' } else { c })
            .take(AUTO_NAME_MAX_CHARS)
            .collect();
        Self::parse(cleaned.trim_start_matches(['.', ' ', '\t']))
            .unwrap_or_else(|_| Self::untitled())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Entfernt führende Markdown-Zeichen einer Zeile (Überschrift, Liste, Zitat, Checkbox).
fn strip_markers(line: &str) -> &str {
    let mut rest = line.trim_start_matches(['#', '-', '*', '>', '+', ' ', '\t']);
    for checkbox in ["[ ]", "[x]", "[X]"] {
        if let Some(after) = rest.strip_prefix(checkbox) {
            rest = after.trim_start();
        }
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_and_validates() {
        assert_eq!(NoteName::parse("  Blatt 3 ").unwrap().as_str(), "Blatt 3");
        for bad in ["", "  ", "a/b", "a\\b", ".hidden", " .x"] {
            assert!(
                matches!(NoteName::parse(bad), Err(CoreError::Validation(_))),
                "{bad:?}"
            );
        }
        assert!(NoteName::parse(&"ä".repeat(NOTE_NAME_MAX_CHARS)).is_ok());
        assert!(NoteName::parse(&"ä".repeat(NOTE_NAME_MAX_CHARS + 1)).is_err());
        assert_eq!(
            NoteName::parse("a\u{1b}b\tc").unwrap().as_str(),
            "ab c",
            "Steuerzeichen raus"
        );
    }

    #[test]
    fn auto_name_from_first_non_empty_line() {
        let cases = [
            ("", "Unbenannt"),
            ("\n\n   \n", "Unbenannt"),
            ("# Blatt 3\nAufgabe 1", "Blatt 3"),
            ("\n\n## Grenzwerte  \n", "Grenzwerte"),
            ("- [ ] Milch kaufen", "Milch kaufen"),
            ("* [x] erledigt", "erledigt"),
            ("> Zitat", "Zitat"),
            ("a/b\\c", "a-b-c"),
            ("...geheim", "geheim"),
            ("#", "Unbenannt"),
            ("###\nzweite", "zweite"),
        ];
        for (text, expected) in cases {
            assert_eq!(
                NoteName::from_first_line(text).as_str(),
                expected,
                "{text:?}"
            );
        }
        let long = "x".repeat(100);
        assert_eq!(
            NoteName::from_first_line(&long).as_str().chars().count(),
            60
        );
    }
}
