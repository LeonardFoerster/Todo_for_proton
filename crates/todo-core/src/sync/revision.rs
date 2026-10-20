//! Revisionen des Notiz-Inhalts: ein wachsender DAG unveränderlicher Revisionen (G-Set) und
//! seine deterministische Auflösung zu einem Text (`notes.md` §4, ADR-029).
//!
//! - **Vereinigung** ist kommutativ, assoziativ und idempotent → Konvergenz wie beim LWW-Merge.
//! - **Köpfe** = Revisionen ohne bekannten Nachfolger. Ein Kopf (bzw. mehrere mit gleichem Text):
//!   das ist der Inhalt.
//! - Mehrere Köpfe mit verschiedenem Text werden nach HLC aufsteigend gefaltet: Basis ist der
//!   gemeinsame Vorfahr (bei mehreren gleichrangigen Kandidaten deren eigene Auflösung, wie
//!   „recursive" in Git), dann [`merge3`]. Bei Konflikt oder fehlender Basis gewinnt der Kopf mit
//!   der höheren HLC, der unterlegene Stand wird Konfliktkopie.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    text_merge::{merge3, Merge3},
    Hlc,
};
use crate::domain::{NoteId, RevisionId};

/// Eine Revision des Inhalts (= eine `Revise`-Op).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    pub id: RevisionId,
    pub hlc: Hlc,
    /// Leer = erste Revision (Basis: leerer Text); ≥ 2 = Merge-Revision.
    pub parents: Vec<RevisionId>,
    pub text: String,
}

/// Ein unterlegener Stand, der als Konfliktkopie erhalten bleibt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictLoser {
    /// Neueste Revision des unterlegenen Stands (Grundlage der Kopie-ID).
    pub rev: RevisionId,
    pub hlc: Hlc,
    pub text: String,
}

/// Ergebnis von [`RevisionSet::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Der aufgelöste Text.
    pub text: String,
    /// Köpfe (nach HLC aufsteigend).
    pub heads: Vec<RevisionId>,
    /// Gibt es Köpfe mit verschiedenem Text (→ Merge-Revision nötig)?
    pub needs_merge: bool,
    /// Bei Konflikten die unterlegenen Stände.
    pub losers: Vec<ConflictLoser>,
}

impl Resolution {
    /// Revision, auf der ein Editor aufsetzt: der neueste Kopf (`None` ohne Revisionen).
    #[must_use]
    pub fn base(&self) -> Option<RevisionId> {
        self.heads.last().copied()
    }
}

/// Die bekannten Revisionen einer Notiz.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevisionSet {
    revs: BTreeMap<RevisionId, Revision>,
}

impl RevisionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_revisions(revs: impl IntoIterator<Item = Revision>) -> Self {
        let mut set = Self::new();
        for rev in revs {
            set.insert(rev);
        }
        set
    }

    /// Fügt eine Revision hinzu (Vereinigung). Eine bekannte ID bleibt unverändert – außer bei
    /// gleicher ID mit neuerer HLC: Das ist eine noch nicht hochgeladene eigene Revision, die in
    /// der Outbox ersetzt wurde (§4.6). `true` bei Änderung.
    pub fn insert(&mut self, rev: Revision) -> bool {
        match self.revs.get(&rev.id) {
            Some(known) if known.hlc >= rev.hlc => false,
            _ => {
                self.revs.insert(rev.id, rev);
                true
            }
        }
    }

    #[must_use]
    pub fn get(&self, id: RevisionId) -> Option<&Revision> {
        self.revs.get(&id)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.revs.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.revs.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Revision> {
        self.revs.values()
    }

    /// Entfernt Revisionen (Aufräumen, endgültiges Löschen).
    pub fn remove(&mut self, id: RevisionId) -> Option<Revision> {
        self.revs.remove(&id)
    }

    /// Köpfe, nach HLC aufsteigend.
    #[must_use]
    pub fn heads(&self) -> Vec<&Revision> {
        let parents: BTreeSet<RevisionId> = self
            .revs
            .values()
            .flat_map(|r| r.parents.iter().copied())
            .collect();
        let mut heads: Vec<&Revision> = self
            .revs
            .values()
            .filter(|r| !parents.contains(&r.id))
            .collect();
        heads.sort_by_key(|r| r.hlc);
        heads
    }

    /// Eltern einer neuen lokalen Revision, die auf `base` aufsetzt: `base` selbst und – wenn
    /// `base` ein Kopf ist – alle Köpfe mit gleichem Text (so legt die nächste Bearbeitung
    /// gleichlautende Köpfe zusammen, §4.4).
    #[must_use]
    pub fn parents_for_edit(&self, base: Option<RevisionId>) -> Vec<RevisionId> {
        let Some(base) = base.filter(|b| self.revs.contains_key(b)) else {
            return Vec::new();
        };
        let heads = self.heads();
        if !heads.iter().any(|h| h.id == base) {
            return vec![base];
        }
        let text = &self.revs[&base].text;
        let mut parents: Vec<RevisionId> = heads
            .iter()
            .filter(|h| h.text == *text)
            .map(|h| h.id)
            .collect();
        parents.sort();
        parents
    }

    /// Vorfahren von `ids` (einschließlich) und ob die Kette vollständig bekannt ist (kein
    /// Elternverweis zeigt auf eine unbekannte, z. B. aufgeräumte Revision).
    fn ancestors(&self, ids: &[RevisionId]) -> (BTreeSet<RevisionId>, bool) {
        let mut seen = BTreeSet::new();
        let mut complete = true;
        let mut stack: Vec<RevisionId> = ids.to_vec();
        while let Some(id) = stack.pop() {
            let Some(rev) = self.revs.get(&id) else {
                complete = false;
                continue;
            };
            if seen.insert(id) {
                stack.extend(rev.parents.iter().copied());
            }
        }
        (seen, complete)
    }

    /// Text der Merge-Basis von `ours` (mehrere bereits gefaltete Köpfe) und `theirs`:
    /// `None`, wenn keine Basis bekannt ist.
    fn merge_base(&self, ours: &[RevisionId], theirs: RevisionId, depth: usize) -> Option<String> {
        let (a, a_complete) = self.ancestors(ours);
        let (b, b_complete) = self.ancestors(&[theirs]);
        let common: BTreeSet<RevisionId> = a.intersection(&b).copied().collect();
        if common.is_empty() {
            // Zwei unabhängige erste Revisionen setzen beide auf dem leeren Text auf.
            return (a_complete && b_complete).then(String::new);
        }
        // Gleichrangige gemeinsame Vorfahren: keiner ist Vorfahr eines anderen.
        let maximal: Vec<RevisionId> = common
            .iter()
            .copied()
            .filter(|c| {
                !common
                    .iter()
                    .any(|o| self.is_proper_ancestor(*c, *o, &common))
            })
            .collect();
        match maximal.as_slice() {
            [single] => Some(self.revs[single].text.clone()),
            many if depth < 8 => Some(self.fold(many, depth + 1).0),
            many => many
                .iter()
                .max_by_key(|id| self.revs[*id].hlc)
                .map(|id| self.revs[id].text.clone()),
        }
    }

    /// Ist `ancestor` echter Vorfahr von `of` (innerhalb von `within`)?
    fn is_proper_ancestor(
        &self,
        ancestor: RevisionId,
        of: RevisionId,
        within: &BTreeSet<RevisionId>,
    ) -> bool {
        if ancestor == of {
            return false;
        }
        let mut stack = vec![of];
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            let Some(rev) = self.revs.get(&id) else {
                continue;
            };
            for parent in &rev.parents {
                if *parent == ancestor {
                    return true;
                }
                if within.contains(parent) && seen.insert(*parent) {
                    stack.push(*parent);
                }
            }
        }
        false
    }

    /// Faltet `heads` (beliebige Reihenfolge) nach HLC zu einem Text; liefert ihn mit den
    /// unterlegenen Ständen.
    fn fold(&self, heads: &[RevisionId], depth: usize) -> (String, Vec<ConflictLoser>) {
        let mut sorted: Vec<&Revision> = heads.iter().filter_map(|id| self.revs.get(id)).collect();
        sorted.sort_by_key(|r| r.hlc);
        let Some((first, rest)) = sorted.split_first() else {
            return (String::new(), Vec::new());
        };
        let mut folded = vec![first.id];
        let mut text = first.text.clone();
        // Neueste Revision des bisher gefalteten Stands (für eine Konfliktkopie).
        let mut newest = *first;
        let mut losers = Vec::new();
        for head in rest {
            if head.text == text {
                folded.push(head.id);
                newest = head;
                continue;
            }
            let merged = self
                .merge_base(&folded, head.id, depth)
                .map(|base| merge3(&base, &text, &head.text));
            match merged {
                Some(Merge3::Clean(merged)) => text = merged,
                Some(Merge3::Conflict) | None => {
                    losers.push(ConflictLoser {
                        rev: newest.id,
                        hlc: newest.hlc,
                        text: std::mem::take(&mut text),
                    });
                    text.clone_from(&head.text);
                }
            }
            folded.push(head.id);
            newest = head;
        }
        (text, losers)
    }

    /// Löst die Köpfe zu einem Text auf (reine Funktion der Revisionsmenge).
    #[must_use]
    pub fn resolve(&self) -> Resolution {
        let heads: Vec<RevisionId> = self.heads().iter().map(|r| r.id).collect();
        let distinct: BTreeSet<&str> = heads.iter().map(|id| self.revs[id].text.as_str()).collect();
        let (text, losers) = self.fold(&heads, 0);
        Resolution {
            text,
            needs_merge: distinct.len() > 1,
            heads,
            losers,
        }
    }

    /// Revisionen, die weg dürfen: kein Kopf und mit einem Nachfahren, den `acked` als von allen
    /// aktiven Geräten gesehen meldet (§4.5). Jede künftige Bearbeitung dieser Geräte stammt dann
    /// von diesem Nachfahren ab.
    #[must_use]
    pub fn prunable(&self, acked: impl Fn(RevisionId) -> bool) -> Vec<RevisionId> {
        let parents: Vec<RevisionId> = self
            .revs
            .values()
            .filter(|r| acked(r.id))
            .flat_map(|r| r.parents.iter().copied())
            .collect();
        // Köpfe sind nie Vorfahren, bleiben also automatisch.
        self.ancestors(&parents).0.into_iter().collect()
    }
}

/// ID der Konfliktkopie für den unterlegenen Stand `loser` der Notiz `note` – auf allen Geräten
/// gleich (UUIDv5), damit gleichzeitige Auflösungen dieselbe Kopie anlegen.
#[must_use]
pub fn conflict_copy_id(note: NoteId, loser: RevisionId) -> NoteId {
    NoteId::derived(&[
        &note.to_bytes(),
        &loser.device.to_bytes(),
        &loser.seq.to_be_bytes(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DeviceId;

    struct Dev {
        id: DeviceId,
        seq: u64,
    }

    impl Dev {
        fn new() -> Self {
            Self {
                id: DeviceId::new(),
                seq: 0,
            }
        }

        fn rev(&mut self, wall: u64, parents: &[RevisionId], text: &str) -> Revision {
            self.seq += 1;
            Revision {
                id: RevisionId {
                    device: self.id,
                    seq: self.seq,
                },
                hlc: Hlc {
                    wall_ms: wall,
                    counter: 0,
                    device: self.id,
                },
                parents: parents.to_vec(),
                text: text.into(),
            }
        }
    }

    #[test]
    fn single_head_is_the_text() {
        let mut a = Dev::new();
        let r1 = a.rev(1, &[], "eins");
        let r2 = a.rev(2, &[r1.id], "zwei");
        let set = RevisionSet::from_revisions([r2.clone(), r1]);
        let res = set.resolve();
        assert_eq!(res.text, "zwei");
        assert_eq!(res.heads, [r2.id]);
        assert!(!res.needs_merge);
        assert_eq!(res.base(), Some(r2.id));
        assert!(RevisionSet::new().resolve().text.is_empty());
    }

    #[test]
    fn concurrent_disjoint_edits_merge_cleanly() {
        let (mut a, mut b) = (Dev::new(), Dev::new());
        let base = a.rev(1, &[], "A\n\nB\n\nC");
        let ea = a.rev(5, &[base.id], "A!\n\nB\n\nC");
        let eb = b.rev(4, &[base.id], "A\n\nB\n\nC!");
        let set = RevisionSet::from_revisions([base, ea, eb]);
        let res = set.resolve();
        assert_eq!(res.text, "A!\n\nB\n\nC!");
        assert!(res.needs_merge);
        assert!(res.losers.is_empty());
    }

    #[test]
    fn conflict_keeps_the_newer_head_and_reports_the_loser() {
        let (mut a, mut b) = (Dev::new(), Dev::new());
        let base = a.rev(1, &[], "x");
        let ea = a.rev(5, &[base.id], "von A");
        let eb = b.rev(7, &[base.id], "von B");
        let res = RevisionSet::from_revisions([base, ea.clone(), eb]).resolve();
        assert_eq!(res.text, "von B");
        assert_eq!(
            res.losers,
            [ConflictLoser {
                rev: ea.id,
                hlc: ea.hlc,
                text: "von A".into()
            }]
        );
    }

    #[test]
    fn missing_base_is_a_conflict_but_independent_roots_merge_from_empty() {
        let (mut a, mut b) = (Dev::new(), Dev::new());
        let pruned = a.rev(1, &[], "alt");
        let ea = a.rev(2, &[pruned.id], "alt\nA");
        let eb = b.rev(3, &[pruned.id], "B\nalt");
        let res = RevisionSet::from_revisions([ea.clone(), eb.clone()]).resolve();
        assert_eq!(res.text, eb.text);
        assert_eq!(res.losers.len(), 1);

        let ra = a.rev(4, &[], "nur A");
        let rb = b.rev(5, &[], "nur B");
        let res = RevisionSet::from_revisions([ra, rb]).resolve();
        assert_eq!(res.losers.len(), 1, "beide fügen an derselben Stelle ein");
        let ra = a.rev(6, &[], "A\n");
        let rb = b.rev(7, &[], "");
        assert_eq!(RevisionSet::from_revisions([ra, rb]).resolve().text, "A\n");
    }

    #[test]
    fn identical_heads_need_no_merge_and_are_joined_by_the_next_edit() {
        let (mut a, mut b) = (Dev::new(), Dev::new());
        let base = a.rev(1, &[], "x");
        let ma = a.rev(5, &[base.id], "gleich");
        let mb = b.rev(6, &[base.id], "gleich");
        let set = RevisionSet::from_revisions([base, ma.clone(), mb.clone()]);
        let res = set.resolve();
        assert!(!res.needs_merge);
        assert!(res.losers.is_empty());
        assert_eq!(res.base(), Some(mb.id));
        let mut parents = vec![ma.id, mb.id];
        parents.sort();
        assert_eq!(set.parents_for_edit(Some(mb.id)), parents);
        assert_eq!(set.parents_for_edit(None), Vec::<RevisionId>::new());
    }

    #[test]
    fn criss_cross_merges_use_a_virtual_base() {
        // h1, h2 nebenläufig; A legt sie zu ma zusammen, B zusätzlich mit h3 zu mb.
        let (mut a, mut b, mut c) = (Dev::new(), Dev::new(), Dev::new());
        let base = a.rev(1, &[], "1\n2\n3\n4\n5\n6");
        let h1 = a.rev(2, &[base.id], "1!\n2\n3\n4\n5\n6");
        let h2 = b.rev(3, &[base.id], "1\n2\n3!\n4\n5\n6");
        let h3 = c.rev(4, &[base.id], "1\n2\n3\n4!\n5\n6");
        let ma = a.rev(10, &[h1.id, h2.id], "1!\n2\n3!\n4\n5\n6");
        let mb = b.rev(11, &[h1.id, h2.id, h3.id], "1!\n2\n3!\n4!\n5\n6");
        let set = RevisionSet::from_revisions([base, h1, h2, h3, ma, mb]);
        let res = set.resolve();
        assert!(res.losers.is_empty(), "{res:?}");
        assert_eq!(res.text, "1!\n2\n3!\n4!\n5\n6");
    }

    #[test]
    fn insert_is_idempotent_but_takes_a_replaced_own_revision() {
        let mut a = Dev::new();
        let r = a.rev(1, &[], "a");
        let mut set = RevisionSet::new();
        assert!(set.insert(r.clone()));
        assert!(!set.insert(r.clone()));
        let replaced = Revision {
            hlc: Hlc {
                wall_ms: 9,
                ..r.hlc
            },
            text: "ab".into(),
            ..r.clone()
        };
        assert!(set.insert(replaced.clone()));
        assert!(!set.insert(r), "älterer Stand verdrängt den neueren nicht");
        assert_eq!(set.resolve().text, "ab");
    }

    #[test]
    fn prunable_keeps_heads_and_unacked_chains() {
        let mut a = Dev::new();
        let r1 = a.rev(1, &[], "1");
        let r2 = a.rev(2, &[r1.id], "2");
        let r3 = a.rev(3, &[r2.id], "3");
        let set = RevisionSet::from_revisions([r1.clone(), r2.clone(), r3.clone()]);
        assert!(set.prunable(|_| false).is_empty());
        assert_eq!(set.prunable(|id| id == r2.id), [r1.id]);
        let mut all = set.prunable(|_| true);
        all.sort();
        assert_eq!(all, [r1.id, r2.id], "Kopf bleibt");
    }

    #[test]
    fn conflict_copy_ids_are_deterministic() {
        let note = NoteId::new();
        let rev = RevisionId {
            device: DeviceId::new(),
            seq: 3,
        };
        assert_eq!(conflict_copy_id(note, rev), conflict_copy_id(note, rev));
        assert_ne!(
            conflict_copy_id(note, rev),
            conflict_copy_id(note, RevisionId { seq: 4, ..rev })
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod prop_tests {
    //! Konvergenz (`notes.md` §4.8): Geräte bearbeiten zufällig, tauschen Revisionen in
    //! zufälliger Reihenfolge (auch doppelt) und legen Köpfe per Folge-Commit zusammen. Nach
    //! vollständigem Austausch haben alle denselben Text und dieselben Konfliktkopien.
    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::*;
    use crate::domain::DeviceId;

    #[derive(Debug, Clone)]
    enum Action {
        Edit { dev: usize, line: usize, text: u8 },
        Send { from: usize, to: usize },
    }

    fn action(devices: usize) -> impl Strategy<Value = Action> {
        prop_oneof![
            (0..devices, 0usize..6, 0u8..4).prop_map(|(dev, line, text)| Action::Edit {
                dev,
                line,
                text
            }),
            (0..devices, 0..devices).prop_map(|(from, to)| Action::Send { from, to }),
        ]
    }

    struct Device {
        id: DeviceId,
        seq: u64,
        revs: RevisionSet,
        copies: BTreeSet<NoteId>,
    }

    struct World {
        note: NoteId,
        wall: u64,
        devices: Vec<Device>,
    }

    impl World {
        fn new(n: usize) -> Self {
            let mut ids: Vec<DeviceId> = (0..n).map(|_| DeviceId::new()).collect();
            ids.sort();
            Self {
                note: NoteId::new(),
                wall: 0,
                devices: ids
                    .into_iter()
                    .map(|id| Device {
                        id,
                        seq: 0,
                        revs: RevisionSet::new(),
                        copies: BTreeSet::new(),
                    })
                    .collect(),
            }
        }

        fn write(&mut self, dev: usize, parents: Vec<RevisionId>, text: String) {
            self.wall += 1;
            let d = &mut self.devices[dev];
            d.seq += 1;
            d.revs.insert(Revision {
                id: RevisionId {
                    device: d.id,
                    seq: d.seq,
                },
                hlc: Hlc {
                    wall_ms: self.wall,
                    counter: 0,
                    device: d.id,
                },
                parents,
                text,
            });
        }

        /// Folge-Commit: Köpfe mit verschiedenem Text zu einer Merge-Revision zusammenlegen.
        fn settle(&mut self, dev: usize) {
            let res = self.devices[dev].revs.resolve();
            for loser in &res.losers {
                let id = conflict_copy_id(self.note, loser.rev);
                self.devices[dev].copies.insert(id);
            }
            if res.needs_merge {
                self.write(dev, res.heads.clone(), res.text);
            }
        }

        fn edit(&mut self, dev: usize, line: usize, text: u8) {
            let res = self.devices[dev].revs.resolve();
            let mut lines: Vec<String> = res.text.split('\n').map(str::to_owned).collect();
            let new = format!(
                "{}{text}",
                self.devices[dev].id.to_string().get(..2).unwrap()
            );
            if line < lines.len() {
                lines[line] = new;
            } else {
                lines.push(new);
            }
            let parents = self.devices[dev].revs.parents_for_edit(res.base());
            self.write(dev, parents, lines.join("\n"));
            self.settle(dev);
        }

        fn send(&mut self, from: usize, to: usize) {
            let revs: Vec<Revision> = self.devices[from].revs.iter().cloned().collect();
            let copies = self.devices[from].copies.clone();
            for rev in revs {
                self.devices[to].revs.insert(rev);
            }
            self.devices[to].copies.extend(copies);
            self.settle(to);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn devices_converge(
            n in 2usize..=4,
            actions in prop::collection::vec(action(4), 1..40),
        ) {
            let mut world = World::new(n);
            for action in actions {
                match action {
                    Action::Edit { dev, line, text } => world.edit(dev % n, line, text),
                    Action::Send { from, to } => world.send(from % n, to % n),
                }
            }
            // Vollständiger Austausch, bis sich nichts mehr ändert.
            for _ in 0..4 {
                for from in 0..n {
                    for to in 0..n {
                        world.send(from, to);
                    }
                }
            }
            let texts: BTreeSet<String> =
                world.devices.iter().map(|d| d.revs.resolve().text).collect();
            prop_assert_eq!(texts.len(), 1, "gleicher Text überall");
            for d in &world.devices {
                prop_assert!(!d.revs.resolve().needs_merge, "keine offenen Köpfe");
                prop_assert_eq!(&d.copies, &world.devices[0].copies);
            }
        }
    }
}
