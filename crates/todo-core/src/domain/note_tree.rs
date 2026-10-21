//! Reine Baum-Regeln für Notiz-Ordner (`notes.md` §3.2) – ohne eigene Ops, auf allen Geräten
//! gleich:
//!
//! 1. **Waisen:** Fehlt der Elternordner (gelöscht oder noch unbekannt), hängt das Element in der
//!    Wurzel.
//! 2. **Zyklen** (zwei Geräte verschieben über Kreuz): Im Zyklus wird der Ordner mit dem höchsten
//!    Rang (HLC seines Eltern-Registers) in die Wurzel gehängt.
//! 3. **Wirksamer Papierkorb:** Ein Element ist im Papierkorb, wenn es selbst oder ein Vorfahr dort
//!    liegt.

use std::collections::{BTreeMap, BTreeSet};

use super::NoteFolderId;
use crate::{CoreError, CoreResult};

/// Größte Verschachtelungstiefe von Ordnern (ein Ordner in der Wurzel hat Tiefe 1).
pub const NOTE_TREE_MAX_DEPTH: usize = 32;

/// Wirksamer Elternordner je Ordner (`None` = Wurzel), garantiert zyklenfrei.
pub type FolderParents = BTreeMap<NoteFolderId, Option<NoteFolderId>>;

/// Wendet Regel 1 und 2 an. `declared` = je Ordner der gespeicherte Elternordner und ein Rang;
/// bei einem Zyklus verliert der Ordner mit dem höchsten Rang seinen Elternordner. Laufzeit O(n).
#[must_use]
pub fn resolve_parents<K: Ord>(
    declared: &BTreeMap<NoteFolderId, (Option<NoteFolderId>, K)>,
) -> FolderParents {
    let mut parents: FolderParents = declared
        .iter()
        .map(|(id, (parent, _))| (*id, parent.filter(|p| declared.contains_key(p))))
        .collect();

    // Funktionaler Graph: jede Komponente hat höchstens einen Zyklus. Pfad hochlaufen, bis die
    // Wurzel, ein schon geprüfter Ordner oder ein Ordner des aktuellen Pfads erreicht ist.
    let mut done: BTreeSet<NoteFolderId> = BTreeSet::new();
    let ids: Vec<NoteFolderId> = parents.keys().copied().collect();
    for start in ids {
        let mut path: Vec<NoteFolderId> = Vec::new();
        let mut on_path: BTreeSet<NoteFolderId> = BTreeSet::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if done.contains(&id) {
                break;
            }
            if !on_path.insert(id) {
                // Zyklus = Pfad ab dem ersten Vorkommen von `id`.
                let from = path.iter().position(|p| *p == id).expect("on path");
                let breaker = path[from..]
                    .iter()
                    .max_by(|a, b| declared[*a].1.cmp(&declared[*b].1).then(a.cmp(b)))
                    .copied()
                    .expect("cycle is not empty");
                parents.insert(breaker, None);
                break;
            }
            path.push(id);
            current = parents[&id];
        }
        done.extend(path);
    }
    parents
}

/// Vorfahren von `folder` (ohne ihn selbst), vom Elternordner bis zur Wurzel.
#[must_use]
pub fn ancestors(parents: &FolderParents, folder: NoteFolderId) -> Vec<NoteFolderId> {
    let mut out = Vec::new();
    let mut current = parents.get(&folder).copied().flatten();
    while let Some(id) = current {
        if out.contains(&id) || id == folder {
            break; // nur Schutz; `parents` aus `resolve_parents` ist zyklenfrei
        }
        out.push(id);
        current = parents.get(&id).copied().flatten();
    }
    out
}

/// Tiefe eines Ortes: Wurzel = 0, ein Ordner in der Wurzel = 1.
#[must_use]
pub fn depth(parents: &FolderParents, folder: Option<NoteFolderId>) -> usize {
    folder.map_or(0, |id| ancestors(parents, id).len() + 1)
}

/// Kinder je Ordner (`None` = Wurzel).
fn children(parents: &FolderParents) -> BTreeMap<Option<NoteFolderId>, Vec<NoteFolderId>> {
    let mut out: BTreeMap<Option<NoteFolderId>, Vec<NoteFolderId>> = BTreeMap::new();
    for (id, parent) in parents {
        out.entry(*parent).or_default().push(*id);
    }
    out
}

/// Alle Ordner unterhalb von `folder` (ohne ihn selbst), Eltern vor Kindern.
#[must_use]
pub fn descendants(parents: &FolderParents, folder: NoteFolderId) -> Vec<NoteFolderId> {
    let children = children(parents);
    let mut out = Vec::new();
    let mut queue = vec![folder];
    while let Some(id) = queue.pop() {
        for child in children.get(&Some(id)).into_iter().flatten() {
            if *child != folder && !out.contains(child) {
                out.push(*child);
                queue.push(*child);
            }
        }
    }
    out
}

/// Höhe des Teilbaums: 1 für einen Ordner ohne Unterordner.
fn subtree_height(parents: &FolderParents, folder: NoteFolderId) -> usize {
    descendants(parents, folder)
        .into_iter()
        .map(|d| {
            ancestors(parents, d)
                .iter()
                .take_while(|a| **a != folder)
                .count()
                + 1
        })
        .max()
        .unwrap_or(0)
        + 1
}

/// Darf `folder` nach `new_parent` verschoben werden? Nicht in sich selbst oder einen eigenen
/// Unterordner, und die Tiefe bleibt ≤ [`NOTE_TREE_MAX_DEPTH`].
///
/// # Errors
/// `CoreError::Validation` mit dem Grund.
pub fn check_folder_move(
    parents: &FolderParents,
    folder: NoteFolderId,
    new_parent: Option<NoteFolderId>,
) -> CoreResult<()> {
    if let Some(target) = new_parent {
        if target == folder || ancestors(parents, target).contains(&folder) {
            return Err(CoreError::Validation(
                "a folder cannot be moved into itself or one of its subfolders".into(),
            ));
        }
    }
    check_depth(depth(parents, new_parent) + subtree_height(parents, folder))
}

/// Darf unter `parent` ein neuer Ordner entstehen (Tiefe)?
///
/// # Errors
/// `CoreError::Validation`, wenn die Tiefe überschritten würde.
pub fn check_new_folder(parents: &FolderParents, parent: Option<NoteFolderId>) -> CoreResult<()> {
    check_depth(depth(parents, parent) + 1)
}

fn check_depth(depth: usize) -> CoreResult<()> {
    if depth > NOTE_TREE_MAX_DEPTH {
        return Err(CoreError::Validation(format!(
            "folders can be nested at most {NOTE_TREE_MAX_DEPTH} levels deep"
        )));
    }
    Ok(())
}

/// Regel 3: Liegt ein Element in `folder` wegen eines Ordners im Papierkorb? (Ob das Element
/// selbst dort liegt, prüft der Aufrufer.)
#[must_use]
pub fn in_trashed_folder(
    parents: &FolderParents,
    trashed: &BTreeSet<NoteFolderId>,
    folder: Option<NoteFolderId>,
) -> bool {
    folder.is_some_and(|id| {
        trashed.contains(&id) || ancestors(parents, id).iter().any(|a| trashed.contains(a))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<NoteFolderId> {
        let mut ids: Vec<NoteFolderId> = (0..n).map(|_| NoteFolderId::new()).collect();
        ids.sort();
        ids
    }

    fn declared(
        entries: &[(NoteFolderId, Option<NoteFolderId>, u32)],
    ) -> BTreeMap<NoteFolderId, (Option<NoteFolderId>, u32)> {
        entries.iter().map(|(id, p, k)| (*id, (*p, *k))).collect()
    }

    #[test]
    fn orphans_go_to_the_root() {
        let f = ids(2);
        let unknown = NoteFolderId::new();
        let parents = resolve_parents(&declared(&[
            (f[0], Some(unknown), 1),
            (f[1], Some(f[0]), 1),
        ]));
        assert_eq!(parents[&f[0]], None);
        assert_eq!(parents[&f[1]], Some(f[0]));
    }

    #[test]
    fn cycle_is_broken_at_the_highest_rank() {
        let f = ids(4);
        // f0 → f1 → f2 → f0 (Zyklus), f3 → f2
        let d = declared(&[
            (f[0], Some(f[1]), 5),
            (f[1], Some(f[2]), 9),
            (f[2], Some(f[0]), 1),
            (f[3], Some(f[2]), 99),
        ]);
        let parents = resolve_parents(&d);
        assert_eq!(parents[&f[1]], None, "höchster Rang im Zyklus");
        assert_eq!(parents[&f[0]], Some(f[1]));
        assert_eq!(parents[&f[2]], Some(f[0]));
        assert_eq!(
            parents[&f[3]],
            Some(f[2]),
            "außerhalb des Zyklus unverändert"
        );
        let self_loop = resolve_parents(&declared(&[(f[0], Some(f[0]), 1)]));
        assert_eq!(self_loop[&f[0]], None);
    }

    #[test]
    fn moves_are_checked_for_cycles_and_depth() {
        let f = ids(3);
        let parents = resolve_parents(&declared(&[
            (f[0], None, 0),
            (f[1], Some(f[0]), 0),
            (f[2], Some(f[1]), 0),
        ]));
        assert!(check_folder_move(&parents, f[0], Some(f[2])).is_err());
        assert!(check_folder_move(&parents, f[0], Some(f[0])).is_err());
        assert!(check_folder_move(&parents, f[2], None).is_ok());
        assert!(check_folder_move(&parents, f[2], Some(f[0])).is_ok());
        assert_eq!(depth(&parents, Some(f[2])), 3);
        assert_eq!(depth(&parents, None), 0);
        assert_eq!(subtree_height(&parents, f[0]), 3);
        assert_eq!(descendants(&parents, f[0]), [f[1], f[2]]);
        assert_eq!(ancestors(&parents, f[2]), [f[1], f[0]]);

        // Kette mit 32 Ebenen: ein weiterer Ordner darunter geht nicht mehr.
        let chain = ids(NOTE_TREE_MAX_DEPTH);
        let mut entries = Vec::new();
        for (i, id) in chain.iter().enumerate() {
            entries.push((*id, i.checked_sub(1).map(|p| chain[p]), 0));
        }
        let deep = resolve_parents(&declared(&entries));
        assert!(check_new_folder(&deep, Some(chain[NOTE_TREE_MAX_DEPTH - 2])).is_ok());
        assert!(check_new_folder(&deep, chain.last().copied()).is_err());
        let mut with_extra = entries.clone();
        with_extra.push((f[0], None, 0));
        with_extra.push((f[1], Some(f[0]), 0));
        let tree = resolve_parents(&declared(&with_extra));
        assert!(
            check_folder_move(&tree, f[0], Some(chain[NOTE_TREE_MAX_DEPTH - 2])).is_err(),
            "Teilbaum mit 2 Ebenen passt nicht mehr"
        );
        assert!(check_folder_move(&tree, f[1], Some(chain[NOTE_TREE_MAX_DEPTH - 2])).is_ok());
    }

    #[test]
    fn trash_is_inherited() {
        let f = ids(3);
        let parents = resolve_parents(&declared(&[
            (f[0], None, 0),
            (f[1], Some(f[0]), 0),
            (f[2], None, 0),
        ]));
        let trashed = BTreeSet::from([f[0]]);
        assert!(in_trashed_folder(&parents, &trashed, Some(f[1])));
        assert!(in_trashed_folder(&parents, &trashed, Some(f[0])));
        assert!(!in_trashed_folder(&parents, &trashed, Some(f[2])));
        assert!(!in_trashed_folder(&parents, &trashed, None));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// Beliebige (auch zyklische) Eltern-Angaben: Ergebnis ist zyklenfrei, jeder Ordner
        /// erreicht die Wurzel, und abweichend vom gespeicherten Elternordner ist nur „Wurzel".
        #[test]
        fn tree_is_always_acyclic_and_reachable(
            spec in prop::collection::vec((prop::option::of(0usize..12), 0u32..5), 1..12)
        ) {
            let mut ids: Vec<NoteFolderId> = (0..spec.len()).map(|_| NoteFolderId::new()).collect();
            ids.sort();
            let extra = NoteFolderId::new(); // unbekannter Elternordner
            let declared: BTreeMap<_, _> = spec
                .iter()
                .enumerate()
                .map(|(i, (p, k))| {
                    let parent = p.map(|p| ids.get(p).copied().unwrap_or(extra));
                    (ids[i], (parent, *k))
                })
                .collect();
            let parents = resolve_parents(&declared);
            prop_assert_eq!(parents.len(), ids.len());
            for id in &ids {
                // Hochlaufen endet nach höchstens n Schritten in der Wurzel.
                let mut steps = 0;
                let mut current = parents[id];
                while let Some(p) = current {
                    steps += 1;
                    prop_assert!(steps <= ids.len(), "Zyklus über {id}");
                    current = parents[&p];
                }
                let resolved = parents[id];
                let wanted = declared[id].0;
                if resolved != wanted {
                    prop_assert!(resolved.is_none());
                }
            }
        }
    }
}
