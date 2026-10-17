//! Gemeinsame Validierung für Titel und Namen.

use crate::{CoreError, CoreResult};

/// Trimmt `input` und prüft: nicht leer, höchstens `max_chars` Unicode-Zeichen (nicht Bytes).
pub(crate) fn normalize_name(input: &str, max_chars: usize, what: &str) -> CoreResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Validation(format!("{what} must not be empty")));
    }
    let len = trimmed.chars().count();
    if len > max_chars {
        return Err(CoreError::Validation(format!(
            "{what} must be at most {max_chars} characters, got {len}"
        )));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts() {
        assert_eq!(normalize_name("  Milch  ", 10, "title").unwrap(), "Milch");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        for input in ["", "   ", "\t\n"] {
            let err = normalize_name(input, 10, "title").unwrap_err();
            assert_eq!(err, CoreError::Validation("title must not be empty".into()));
        }
    }

    #[test]
    fn limit_counts_chars_not_bytes() {
        let umlauts = "ä".repeat(10); // 20 Bytes, 10 Zeichen
        assert!(normalize_name(&umlauts, 10, "title").is_ok());
        assert!(normalize_name(&"ä".repeat(11), 10, "title").is_err());
    }
}
