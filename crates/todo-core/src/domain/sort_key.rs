//! Sortierschlüssel für die manuelle Reihenfolge (Fractional Index, siehe `sync-strategy.md`).
//!
//! Umsortieren ändert nur den Schlüssel der verschobenen Task. Verfahren nach David Greenspan,
//! „Implementing Fractional Indexing": Ein Schlüssel besteht aus einem variabel langen
//! Ganzzahlteil und einem Bruchteil, beide Base62 (`0-9A-Za-z`). Die ASCII-Reihenfolge des
//! Alphabets entspricht der Ziffernreihenfolge, daher genügt ein Byte-Vergleich der Strings
//! (in SQLite: `BINARY`-Collation).
//!
//! - Ganzzahlteil: Kopfzeichen `a`–`z` = positive Zahl mit 1–26 Ziffern, `A`–`Z` = negative
//!   Zahl mit 26–1 Ziffern. Einfügen am Anfang/Ende zählt nur diese Zahl herunter/hoch,
//!   die Länge wächst daher nur logarithmisch.
//! - Bruchteil: optional, ohne abschließende `0`, damit zwischen zwei Schlüsseln immer
//!   ein weiterer liegt.

use std::fmt;

use crate::{CoreError, CoreResult};

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: u8 = 62;
/// Ganzzahlteil von [`SortKey::first`].
const INTEGER_ZERO: &str = "a0";
/// Kleinster darstellbarer Ganzzahlteil; als Schlüssel ohne Bruchteil unzulässig, damit
/// davor immer noch Platz ist.
const SMALLEST_INTEGER: &str = "A00000000000000000000000000";

/// Lexikografisch vergleichbarer Sortierschlüssel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SortKey(String);

impl SortKey {
    /// Schlüssel für das erste Element einer leeren Liste.
    #[must_use]
    pub fn first() -> Self {
        Self(INTEGER_ZERO.to_owned())
    }

    /// Erzeugt einen Schlüssel strikt zwischen `a` und `b`. `None` steht für Listenanfang
    /// bzw. Listenende.
    ///
    /// # Errors
    /// `CoreError::Validation`, wenn nicht `a < b` gilt.
    pub fn between(a: Option<&SortKey>, b: Option<&SortKey>) -> CoreResult<Self> {
        let key = match (a, b) {
            (None, None) => INTEGER_ZERO.to_owned(),
            (None, Some(b)) => before(&b.0),
            (Some(a), None) => after(&a.0),
            (Some(a), Some(b)) if a < b => between(&a.0, &b.0),
            (Some(a), Some(b)) => {
                return Err(CoreError::Validation(format!(
                    "sort key {a} must be less than {b}"
                )))
            }
        };
        Ok(Self(key))
    }

    /// Liest einen gespeicherten Schlüssel und prüft sein Format.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungültigem Format.
    pub fn parse(s: &str) -> CoreResult<Self> {
        if is_valid(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(CoreError::Validation(format!("invalid sort key: {s:?}")))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SortKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn digit_value(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'Z' => c - b'A' + 10,
        b'a'..=b'z' => c - b'a' + 36,
        _ => unreachable!("validated base62 digit"),
    }
}

/// Länge des Ganzzahlteils inkl. Kopfzeichen.
fn integer_len(head: u8) -> Option<usize> {
    match head {
        b'a'..=b'z' => Some(usize::from(head - b'a') + 2),
        b'A'..=b'Z' => Some(usize::from(b'Z' - head) + 2),
        _ => None,
    }
}

/// Teilt einen gültigen Schlüssel in Ganzzahl- und Bruchteil.
fn split(key: &str) -> (&str, &str) {
    key.split_at(integer_len(key.as_bytes()[0]).expect("validated sort key"))
}

fn is_valid(key: &str) -> bool {
    let bytes = key.as_bytes();
    let Some(len) = bytes.first().copied().and_then(integer_len) else {
        return false;
    };
    bytes.len() >= len
        && bytes[1..].iter().all(u8::is_ascii_alphanumeric)
        && key != SMALLEST_INTEGER
        && bytes[len..].last() != Some(&b'0')
}

fn before(b: &str) -> String {
    let (int_b, frac_b) = split(b);
    if int_b == SMALLEST_INTEGER {
        return format!("{int_b}{}", midpoint("", Some(frac_b)));
    }
    if !frac_b.is_empty() {
        // Der reine Ganzzahlteil ist kleiner als `b` und so kurz wie möglich.
        return int_b.to_owned();
    }
    let int = decrement_integer(int_b).expect("only the smallest integer cannot be decremented");
    if int == SMALLEST_INTEGER {
        // Nackt unzulässig (siehe `is_valid`); mit Bruchteil liegt er weiterhin vor `b`.
        return format!("{int}{}", midpoint("", None));
    }
    int
}

fn after(a: &str) -> String {
    let (int_a, frac_a) = split(a);
    increment_integer(int_a).unwrap_or_else(|| format!("{int_a}{}", midpoint(frac_a, None)))
}

fn between(a: &str, b: &str) -> String {
    let (int_a, frac_a) = split(a);
    let (int_b, frac_b) = split(b);
    if int_a == int_b {
        return format!("{int_a}{}", midpoint(frac_a, Some(frac_b)));
    }
    match increment_integer(int_a) {
        Some(next) if next.as_str() < b => next,
        _ => format!("{int_a}{}", midpoint(frac_a, None)),
    }
}

/// Bruchteil strikt zwischen `a` und `b` (`None` = 1). Vorbedingung: `a < b`, keine
/// abschließenden Nullen.
fn midpoint(a: &str, b: Option<&str>) -> String {
    let (a, b) = (a.as_bytes(), b.map(str::as_bytes));
    let mut out = Vec::new();
    let mut a = a;
    let mut b = b;
    loop {
        if let Some(bb) = b {
            // Gemeinsames Präfix übernehmen (`a` gedanklich mit Nullen aufgefüllt).
            let n = bb
                .iter()
                .enumerate()
                .take_while(|&(i, &c)| a.get(i).copied().unwrap_or(b'0') == c)
                .count();
            if n > 0 {
                out.extend_from_slice(&bb[..n]);
                a = a.get(n..).unwrap_or_default();
                b = Some(&bb[n..]);
                continue;
            }
        }
        let digit_a = a.first().map_or(0, |&c| digit_value(c));
        let digit_b = b.map_or(BASE, |bb| digit_value(bb[0]));
        if digit_b - digit_a > 1 {
            out.push(DIGITS[usize::from((digit_a + digit_b).div_ceil(2))]);
            break;
        }
        // Benachbarte Ziffern: wenn `b` länger ist, liegt seine erste Ziffer allein dazwischen.
        if let Some(bb) = b.filter(|bb| bb.len() > 1) {
            out.push(bb[0]);
            break;
        }
        out.push(DIGITS[usize::from(digit_a)]);
        a = a.get(1..).unwrap_or_default();
        b = None;
    }
    String::from_utf8(out).expect("base62 is ascii")
}

/// Nächstgrößerer Ganzzahlteil; `None` am oberen Ende des Wertebereichs.
fn increment_integer(int: &str) -> Option<String> {
    let (head, digits) = (int.as_bytes()[0], &int.as_bytes()[1..]);
    let mut digits = digits.to_vec();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        let v = digit_value(*d) + 1;
        if v == BASE {
            *d = b'0';
        } else {
            *d = DIGITS[usize::from(v)];
            carry = false;
            break;
        }
    }
    if carry {
        match head {
            b'Z' => return Some(INTEGER_ZERO.to_owned()),
            b'z' => return None,
            _ => {}
        }
        let head = head + 1;
        if head > b'a' {
            digits.push(b'0');
        } else {
            digits.pop();
        }
        return Some(assemble(head, &digits));
    }
    Some(assemble(head, &digits))
}

/// Nächstkleinerer Ganzzahlteil; `None` am unteren Ende des Wertebereichs.
fn decrement_integer(int: &str) -> Option<String> {
    let (head, digits) = (int.as_bytes()[0], &int.as_bytes()[1..]);
    let mut digits = digits.to_vec();
    let mut borrow = true;
    for d in digits.iter_mut().rev() {
        let v = digit_value(*d);
        if v == 0 {
            *d = b'z';
        } else {
            *d = DIGITS[usize::from(v - 1)];
            borrow = false;
            break;
        }
    }
    if borrow {
        match head {
            b'a' => return Some("Zz".to_owned()),
            b'A' => return None,
            _ => {}
        }
        let head = head - 1;
        if head < b'Z' {
            digits.push(b'z');
        } else {
            digits.pop();
        }
        return Some(assemble(head, &digits));
    }
    Some(assemble(head, &digits))
}

fn assemble(head: u8, digits: &[u8]) -> String {
    let mut s = String::with_capacity(digits.len() + 1);
    s.push(char::from(head));
    s.extend(digits.iter().copied().map(char::from));
    s
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn key(s: &str) -> SortKey {
        SortKey::parse(s).unwrap()
    }

    fn between(a: Option<&str>, b: Option<&str>) -> String {
        let (a, b) = (a.map(key), b.map(key));
        SortKey::between(a.as_ref(), b.as_ref()).unwrap().0
    }

    #[test]
    fn first_key() {
        assert_eq!(SortKey::first().as_str(), "a0");
        assert_eq!(between(None, None), "a0");
    }

    #[test]
    fn known_values() {
        // Referenzwerte des Original-Verfahrens.
        assert_eq!(between(None, Some("a0")), "Zz");
        assert_eq!(between(Some("a0"), None), "a1");
        assert_eq!(between(Some("a0"), Some("a1")), "a0V");
        assert_eq!(between(Some("a1"), Some("a2")), "a1V");
        assert_eq!(between(Some("a0V"), Some("a1")), "a0l");
        assert_eq!(between(Some("Zz"), Some("a0")), "ZzV");
        assert_eq!(between(Some("Zz"), Some("a01")), "a0");
        assert_eq!(between(None, Some("a0V")), "a0");
        assert_eq!(between(None, Some("b999")), "b99");
        assert_eq!(between(Some("az"), None), "b00");
        assert_eq!(
            between(Some("zzzzzzzzzzzzzzzzzzzzzzzzzzz"), None),
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzV"
        );
        let almost_smallest = format!("{}1", &SMALLEST_INTEGER[..SMALLEST_INTEGER.len() - 1]);
        assert_eq!(
            between(None, Some(&almost_smallest)),
            format!("{SMALLEST_INTEGER}V")
        );
        assert_eq!(
            between(None, Some(&format!("{SMALLEST_INTEGER}V"))),
            format!("{SMALLEST_INTEGER}G")
        );
    }

    #[test]
    fn between_is_strictly_between() {
        let keys = ["Zz", "a0", "a0V", "a1", "a1V", "a2", "b00", "b01"];
        for pair in keys.windows(2) {
            let (a, b) = (key(pair[0]), key(pair[1]));
            let mid = SortKey::between(Some(&a), Some(&b)).unwrap();
            assert!(a < mid && mid < b, "{a} < {mid} < {b}");
        }
    }

    #[test]
    fn open_ends() {
        let x = key("a5");
        assert!(SortKey::between(None, Some(&x)).unwrap() < x);
        assert!(SortKey::between(Some(&x), None).unwrap() > x);
    }

    #[test]
    fn rejects_unordered_bounds() {
        let (a, b) = (key("a1"), key("a2"));
        assert!(matches!(
            SortKey::between(Some(&b), Some(&a)),
            Err(CoreError::Validation(_))
        ));
        assert!(matches!(
            SortKey::between(Some(&a), Some(&a)),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn thousand_inserts_at_start_stay_sorted_and_short() {
        let mut keys = vec![SortKey::first()];
        for _ in 0..1_000 {
            let k = SortKey::between(None, Some(&keys[0])).unwrap();
            keys.insert(0, k);
        }
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        let max = keys.iter().map(|k| k.as_str().len()).max().unwrap();
        assert!(max <= 3, "max key length {max}");
    }

    #[test]
    fn thousand_inserts_at_end_stay_sorted_and_short() {
        let mut keys = vec![SortKey::first()];
        for _ in 0..1_000 {
            let k = SortKey::between(keys.last(), None).unwrap();
            keys.push(k);
        }
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        let max = keys.iter().map(|k| k.as_str().len()).max().unwrap();
        assert!(max <= 3, "max key length {max}");
    }

    #[test]
    fn parse_validates_format() {
        for ok in [
            "a0",
            "a0V",
            "Zz",
            "b00",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzz",
            "A000000000000000000000000000V",
        ] {
            assert!(SortKey::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "a",
            "b0",
            "a00",
            "a0V0",
            "a0-",
            "0a",
            "ä0",
            SMALLEST_INTEGER,
        ] {
            assert!(SortKey::parse(bad).is_err(), "{bad:?}");
        }
    }

    proptest! {
        /// Zufällige Einfügepositionen: Die Liste bleibt streng sortiert, alle Keys sind gültig.
        #[test]
        fn random_inserts_keep_list_sorted(positions in prop::collection::vec(any::<prop::sample::Index>(), 1..300)) {
            let mut keys: Vec<SortKey> = Vec::new();
            for pos in positions {
                let i = pos.index(keys.len() + 1);
                let k = SortKey::between(i.checked_sub(1).map(|j| &keys[j]), keys.get(i)).unwrap();
                prop_assert!(SortKey::parse(k.as_str()).is_ok(), "invalid key {}", k);
                keys.insert(i, k);
            }
            prop_assert!(keys.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
