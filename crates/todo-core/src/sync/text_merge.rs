//! Zeilenbasierter 3-Wege-Merge (diff3) für Notiz-Inhalte (`notes.md` §4.3, ADR-029).
//!
//! Beide Seiten werden gegen die Basis gedifft (Myers); die Änderungen („Hunks") beider Seiten
//! werden zusammengelegt. **Konflikt** nur, wenn Hunks dieselbe Basiszeile betreffen bzw. eine
//! Einfügung mitten in einem geänderten Bereich der anderen Seite liegt – Änderungen an
//! **benachbarten** Zeilen sind kein Konflikt. Gleiche Änderungen auf beiden Seiten zählen einmal.
//!
//! Deterministisch: Das Ergebnis hängt nur von den drei Texten ab.

/// Ergebnis von [`merge3`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge3 {
    /// Ohne Überschneidung zusammengeführt.
    Clean(String),
    /// Überschneidende Änderungen; der Aufrufer entscheidet (höhere HLC gewinnt).
    Conflict,
}

/// Mehr Bearbeitungsschritte sucht der Diff nicht; darüber gilt der ganze (nach Präfix und
/// Suffix verbleibende) Bereich als eine Änderung. Begrenzt die Laufzeit bei völlig neuen Texten.
const MAX_EDIT_DISTANCE: usize = 1_000;

/// Eine Änderung einer Seite: Basiszeilen `start..end` werden durch `lines` ersetzt
/// (`start == end` = reine Einfügung vor Basiszeile `start`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

/// Teilt in Zeilen; `join("\n")` ergibt wieder exakt den Text.
fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// 3-Wege-Merge von `ours` und `theirs` gegen `base`.
#[must_use]
pub fn merge3(base: &str, ours: &str, theirs: &str) -> Merge3 {
    if ours == theirs || theirs == base {
        return Merge3::Clean(ours.to_owned());
    }
    if ours == base {
        return Merge3::Clean(theirs.to_owned());
    }
    let base_lines = lines(base);
    let ours_lines = lines(ours);
    let theirs_lines = lines(theirs);
    let a = hunks(&base_lines, &ours_lines);
    let b = hunks(&base_lines, &theirs_lines);
    let Some(merged) = combine(a, b) else {
        return Merge3::Conflict;
    };
    let mut out: Vec<&str> = Vec::with_capacity(base_lines.len());
    let mut pos = 0;
    for hunk in &merged {
        out.extend_from_slice(&base_lines[pos..hunk.start]);
        out.extend_from_slice(&hunk.lines);
        pos = hunk.end;
    }
    out.extend_from_slice(&base_lines[pos..]);
    Merge3::Clean(out.join("\n"))
}

/// Legt die Hunks beider Seiten in Basis-Reihenfolge zusammen; `None` bei Konflikt.
fn combine<'a>(a: Vec<Hunk<'a>>, b: Vec<Hunk<'a>>) -> Option<Vec<Hunk<'a>>> {
    let mut all: Vec<(Hunk<'a>, u8)> = a
        .into_iter()
        .map(|h| (h, 0))
        .chain(b.into_iter().map(|h| (h, 1)))
        .collect();
    // Einfügungen vor Ersetzungen am selben Punkt; bei Gleichstand Seite A zuerst.
    all.sort_by(|(x, sx), (y, sy)| {
        (x.start, x.end != x.start, x.end, *sx).cmp(&(y.start, y.end != y.start, y.end, *sy))
    });
    let mut out: Vec<Hunk<'a>> = Vec::with_capacity(all.len());
    let mut last_side: Option<u8> = None;
    for (hunk, side) in all {
        if let Some(prev) = out.last() {
            if *prev == hunk {
                continue; // gleiche Änderung auf beiden Seiten
            }
            if last_side != Some(side) && overlaps(prev, &hunk) {
                return None;
            }
        }
        last_side = Some(side);
        out.push(hunk);
    }
    Some(out)
}

/// Betreffen zwei Hunks (in Basis-Reihenfolge, `a` zuerst) dieselbe Stelle?
fn overlaps(a: &Hunk<'_>, b: &Hunk<'_>) -> bool {
    let a_insert = a.start == a.end;
    let b_insert = b.start == b.end;
    match (a_insert, b_insert) {
        // Zwei verschiedene Einfügungen an derselben Stelle: Reihenfolge wäre willkürlich.
        (true, true) => a.start == b.start,
        // Einfügung vor einem geänderten Bereich, der dort beginnt: benachbart, kein Konflikt.
        (true, false) => false,
        // Einfügung mitten im geänderten Bereich von `a`.
        (false, true) => b.start < a.end,
        (false, false) => b.start < a.end,
    }
}

/// Änderungen von `from` nach `to` als Hunks (Myers, mit Präfix-/Suffix-Abgleich).
fn hunks<'a>(from: &[&'a str], to: &[&'a str]) -> Vec<Hunk<'a>> {
    let prefix = from.iter().zip(to).take_while(|(x, y)| x == y).count();
    let suffix = from[prefix..]
        .iter()
        .rev()
        .zip(to[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let a = &from[prefix..from.len() - suffix];
    let b = &to[prefix..to.len() - suffix];
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    let Some(script) = myers(a, b) else {
        return vec![Hunk {
            start: prefix,
            end: prefix + a.len(),
            lines: b.to_vec(),
        }];
    };
    // `script`: Paare (i, j) gleicher Zeilen in aufsteigender Folge; dazwischen liegen Hunks.
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in script
        .into_iter()
        .chain(std::iter::once((a.len(), b.len())))
    {
        if mi > i || mj > j {
            out.push(Hunk {
                start: prefix + i,
                end: prefix + mi,
                lines: b[j..mj].to_vec(),
            });
        }
        i = mi + 1;
        j = mj + 1;
    }
    out
}

/// Myers-Diff: gleiche Zeilenpaare `(i, j)` einer kürzesten Bearbeitung von `a` nach `b`;
/// `None`, wenn sie mehr als [`MAX_EDIT_DISTANCE`] Schritte braucht.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // Indizes < isize::MAX
fn myers(a: &[&str], b: &[&str]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (a.len() + b.len()).min(MAX_EDIT_DISTANCE) as isize;
    let offset = max + 1;
    let mut v = vec![0isize; 2 * offset as usize + 1];
    // Je Schritt `d` die Front davor, nur der genutzte Bereich `k ∈ -d..=d` (Speicher O(D²)).
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = None;
    'search: for d in 0..=max {
        trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let idx = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n && y >= m {
                found = Some(d);
                break 'search;
            }
            k += 2;
        }
    }
    let d_end = found?;
    // Rückweg durch die gespeicherten Fronten.
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=d_end).rev() {
        let front = &trace[d as usize];
        let at = |k: isize| front[(k + d) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            pairs.push((x as usize, y as usize));
        }
        x = prev_x;
        y = prev_y;
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        pairs.push((x as usize, y as usize));
    }
    pairs.reverse();
    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(base: &str, a: &str, b: &str) -> String {
        match merge3(base, a, b) {
            Merge3::Clean(text) => text,
            Merge3::Conflict => panic!("conflict: {base:?} / {a:?} / {b:?}"),
        }
    }

    #[test]
    fn trivial_cases() {
        assert_eq!(clean("a", "a", "b"), "b");
        assert_eq!(clean("a", "b", "a"), "b");
        assert_eq!(clean("a", "c", "c"), "c");
        assert_eq!(clean("", "x", ""), "x");
    }

    #[test]
    fn disjoint_paragraphs_merge() {
        let base = "Absatz 1\n\nAbsatz 2\n\nAbsatz 3";
        let a = "Absatz 1 geändert\n\nAbsatz 2\n\nAbsatz 3";
        let b = "Absatz 1\n\nAbsatz 2\n\nAbsatz 3 auch";
        assert_eq!(
            clean(base, a, b),
            "Absatz 1 geändert\n\nAbsatz 2\n\nAbsatz 3 auch"
        );
    }

    #[test]
    fn adjacent_lines_are_not_a_conflict() {
        let base = "1\n2\n3\n4";
        assert_eq!(
            clean(base, "1\nzwei\n3\n4", "1\n2\ndrei\n4"),
            "1\nzwei\ndrei\n4"
        );
        // Löschen neben Ändern
        assert_eq!(clean(base, "1\n3\n4", "1\n2\ndrei\n4"), "1\ndrei\n4");
        // Einfügen direkt vor einer geänderten Zeile
        assert_eq!(
            clean(base, "1\nneu\n2\n3\n4", "1\nZWEI\n3\n4"),
            "1\nneu\nZWEI\n3\n4"
        );
    }

    #[test]
    fn same_line_is_a_conflict() {
        assert_eq!(merge3("1\n2\n3", "1\nA\n3", "1\nB\n3"), Merge3::Conflict);
        // Zwei verschiedene Einfügungen an derselben Stelle
        assert_eq!(merge3("1\n2", "1\nA\n2", "1\nB\n2"), Merge3::Conflict);
        // Einfügung mitten in einem ersetzten Bereich
        assert_eq!(
            merge3("1\n2\n3\n4", "1\nX\nY\n4", "1\n2\nneu\n3\n4"),
            Merge3::Conflict
        );
        // Löschen gegen Ändern derselben Zeile
        assert_eq!(merge3("1\n2\n3", "1\n3", "1\nB\n3"), Merge3::Conflict);
    }

    #[test]
    fn identical_changes_count_once() {
        let base = "1\n2\n3";
        assert_eq!(clean(base, "1\nX\n3\n4", "1\nX\n3"), "1\nX\n3\n4");
    }

    #[test]
    fn appending_on_both_sides_at_the_end_conflicts_but_different_ends_do_not() {
        assert_eq!(merge3("a", "a\nb", "a\nc"), Merge3::Conflict);
        assert_eq!(clean("a\nz", "x\na\nz", "a\nz\ny"), "x\na\nz\ny");
    }

    #[test]
    fn trailing_newlines_are_kept() {
        assert_eq!(clean("a\nb\n", "A\nb\n", "a\nb\nc\n"), "A\nb\nc\n");
    }

    #[test]
    fn huge_rewrites_fall_back_to_one_hunk() {
        let base: String = (0..5_000).map(|i| format!("{i}\n")).collect();
        let a: String = (0..5_000).map(|i| format!("a{i}\n")).collect();
        let b = format!("{base}ende");
        // A ersetzt alles bis auf die letzte (leere) Zeile als einen Hunk, B nur diese: benachbart.
        assert!(
            clean(&base, &a, &b) == format!("{a}ende"),
            "ohne Hängen gemergt"
        );
        let c = format!("x{base}");
        assert!(merge3(&base, &a, &c) == Merge3::Conflict);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    fn text() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-d]{0,2}", 0..12)
    }

    proptest! {
        /// Ist eine Seite gleich der Basis, ist das Ergebnis die andere Seite.
        #[test]
        fn unchanged_side_yields_the_other(base in text(), other in text()) {
            let (base, other) = (base.join("\n"), other.join("\n"));
            prop_assert_eq!(merge3(&base, &base, &other), Merge3::Clean(other.clone()));
            prop_assert_eq!(merge3(&base, &other, &base), Merge3::Clean(other));
        }

        /// Ändern beide Seiten disjunkte Zeilenbereiche (mit Abstand), enthält das Ergebnis beide.
        #[test]
        fn disjoint_edits_contain_both(
            base in prop::collection::vec("[a-z]{3}", 4..20),
            i_raw in 0usize..100,
            j_raw in 0usize..100,
            ea in "[A-Z]{1,3}",
            eb in "[0-9]{1,3}",
        ) {
            let n = base.len();
            let i = i_raw % (n - 2);
            let j = i + 2 + j_raw % (n - i - 2);
            let mut a = base.clone();
            a[i] = ea.clone();
            let mut b = base.clone();
            b[j] = eb.clone();
            let mut both = base.clone();
            both[i] = ea;
            both[j] = eb;
            prop_assert_eq!(
                merge3(&base.join("\n"), &a.join("\n"), &b.join("\n")),
                Merge3::Clean(both.join("\n"))
            );
        }

        /// Der Diff ist korrekt: Hunks auf die Basis angewendet ergeben das Ziel.
        #[test]
        fn hunks_reproduce_the_target(from in text(), to in text()) {
            let from: Vec<&str> = from.iter().map(String::as_str).collect();
            let to: Vec<&str> = to.iter().map(String::as_str).collect();
            let mut out: Vec<&str> = Vec::new();
            let mut pos = 0;
            for hunk in hunks(&from, &to) {
                prop_assert!(hunk.start >= pos && hunk.end >= hunk.start);
                out.extend_from_slice(&from[pos..hunk.start]);
                out.extend_from_slice(&hunk.lines);
                pos = hunk.end;
            }
            out.extend_from_slice(&from[pos..]);
            prop_assert_eq!(out, to);
        }
    }
}
