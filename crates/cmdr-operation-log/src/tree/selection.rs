//! Which rows a rollback takes: a base plus hand toggles, nearest wins, normalized by
//! the crate on every read (❌ never trusted from the wire), and counted by algebra
//! over the summary, never by a scan (`DETAILS.md` § "The tree view").
//!
//! THE resolver: the tree view's "N of M" and tri-state, and the engine's per-row
//! decision, all go through [`Selection`]. ❌ Never a second copy.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::read::OpReader;
use super::summary::RowFacts;
use super::{UnitRole, unit_role};
use crate::store::OperationLogStoreError;
use crate::types::{EntryType, OpKind, RowRole};

/// What a selection starts from, before its toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum SelectionBase {
    /// Every reversible row (⌘A, and the default).
    Everything,
    /// No row (⌘⇧A).
    Nothing,
}

/// One hand toggle: a folder (and everything under it) or one item, on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum SelectionToggle {
    /// A folder row's key. A folder unit (a trashed folder) is decided by its own
    /// folder's toggle.
    Folder { dir_id: i64, on: bool },
    /// A file row's key.
    Item { seq: i64, on: bool },
}

/// Which rows a rollback takes. A row's decision is its own `Item` toggle, else the
/// nearest `Folder` toggle up its folder chain, else `base`. Send back the normalized
/// one the tree answered with; `null` where a selection is optional means everything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct RollbackSelection {
    pub base: SelectionBase,
    /// In the order they were made: a later folder toggle overrides every earlier
    /// toggle inside it.
    pub toggles: Vec<SelectionToggle>,
}

impl RollbackSelection {
    /// Everything, no toggles: what `null` means.
    pub(crate) fn everything() -> Self {
        RollbackSelection {
            base: SelectionBase::Everything,
            toggles: Vec::new(),
        }
    }
}

/// The most toggles a selection holds, the transfer tree's hand-toggle cap. Past it
/// the newest toggles are refused (`toggles_capped`), and the dialog asks for a folder.
pub(crate) const MAX_TOGGLES: usize = 2_000;

/// Selectable units, their files, and those files' bytes: the three counts the
/// algebra carries ("N of M", and a reversal's progress totals).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Picked {
    pub(crate) units: i64,
    pub(crate) files: i64,
    pub(crate) bytes: i64,
}

impl Picked {
    fn plus(self, other: Picked) -> Picked {
        Picked {
            units: self.units + other.units,
            files: self.files + other.files,
            bytes: self.bytes + other.bytes,
        }
    }

    fn minus(self, other: Picked) -> Picked {
        Picked {
            units: self.units - other.units,
            files: self.files - other.files,
            bytes: self.bytes - other.bytes,
        }
    }

    fn when(self, on: bool) -> Picked {
        if on { self } else { Picked::default() }
    }
}

/// A DIR row that owns what's recorded inside it: a trashed or moved folder, never a
/// folder a copy or move created around its files. Its contents have no checkbox.
pub(crate) fn owns_contents(kind: OpKind, facts: &RowFacts) -> bool {
    let made_by_a_transfer = matches!(kind, OpKind::Copy | OpKind::Move) && facts.created_in_place();
    facts.entry_type == EntryType::Dir && facts.row_role == RowRole::RollbackUnit && !made_by_a_transfer
}

/// A folder the algebra visited: a toggle's dir or one of its ancestors.
#[derive(Debug, Clone)]
struct Node {
    parent: Option<i64>,
    depth: usize,
    all: Picked,
    chosen: Picked,
}

/// One normalized toggle, with where it sits.
#[derive(Debug, Clone, Copy)]
struct Placed {
    toggle: SelectionToggle,
    /// The folder it decides from: its own dir for a folder toggle, the item's axis
    /// folder for an item toggle.
    dir: i64,
    /// What an item toggle's row counts as.
    pick: Picked,
}

/// A selection resolved against one op: normalized, counted, and able to decide any
/// row. Built once per tree request or rollback run.
#[derive(Debug)]
pub(crate) struct Selection {
    base_on: bool,
    folders: HashMap<i64, bool>,
    items: HashMap<i64, bool>,
    normalized: RollbackSelection,
    capped: bool,
    nodes: HashMap<i64, Node>,
    decisions: HashMap<i64, bool>,
    total: Picked,
    chosen: Picked,
}

impl Selection {
    /// Normalize `selection` against the op `reader` reads, and count it. The rules,
    /// in order: a toggle naming no summary row or no `Selectable` row of THIS op goes
    /// (a foreign dir id, a `search_only`, housekeeping, settled, or unknown seq); an
    /// `Item` toggle on a folder unit becomes its folder's toggle; a folder toggle
    /// strictly inside a folder unit goes (its contents have no checkbox); a toggle a
    /// LATER folder toggle covers goes, as does an earlier toggle of the same item; a
    /// toggle equal to what it would inherit goes, as does a folder toggle over no
    /// selectable unit; past [`MAX_TOGGLES`], the newest go.
    pub(crate) fn resolve(
        reader: &mut OpReader<'_>,
        selection: &RollbackSelection,
    ) -> Result<Self, OperationLogStoreError> {
        let kind = reader.context.kind;
        let base_on = selection.base == SelectionBase::Everything;

        // Validate each toggle and find where it sits.
        let mut placed: Vec<Placed> = Vec::new();
        for &toggle in &selection.toggles {
            if let Some(found) = place(reader, kind, toggle)? {
                placed.push(found);
            }
        }

        // Each toggle's folder chain, its own dir first, up to the volume root.
        let mut chains: HashMap<i64, Vec<i64>> = HashMap::new();
        for p in &placed {
            if let std::collections::hash_map::Entry::Vacant(e) = chains.entry(p.dir) {
                let chain = chain_of(reader, p.dir)?;
                e.insert(chain);
            }
        }

        // Folder toggles strictly inside a folder unit go.
        let mut unit_folders: HashMap<i64, bool> = HashMap::new();
        let mut kept: Vec<Placed> = Vec::new();
        for p in placed {
            if let SelectionToggle::Folder { .. } = p.toggle {
                let mut inside_a_unit = false;
                for &ancestor in chains[&p.dir].iter().skip(1) {
                    if is_unit_folder(reader, kind, ancestor, &mut unit_folders)? {
                        inside_a_unit = true;
                        break;
                    }
                }
                if inside_a_unit {
                    continue;
                }
            }
            kept.push(p);
        }

        // A later folder toggle covers every earlier toggle inside it (or on it); a
        // later toggle of an item replaces an earlier one. Walk newest first.
        let mut covering: HashSet<i64> = HashSet::new();
        let mut seen_items: HashSet<i64> = HashSet::new();
        let mut survivors: Vec<Placed> = Vec::new();
        for p in kept.into_iter().rev() {
            let covered = chains[&p.dir].iter().any(|dir| covering.contains(dir));
            let repeated = match p.toggle {
                SelectionToggle::Item { seq, .. } => !seen_items.insert(seq),
                SelectionToggle::Folder { .. } => false,
            };
            if covered || repeated {
                continue;
            }
            if let SelectionToggle::Folder { dir_id, .. } = p.toggle {
                covering.insert(dir_id);
            }
            survivors.push(p);
        }
        survivors.reverse();

        // A toggle equal to what it inherits is a no-op, and so is a folder toggle over
        // no selectable unit. Neither changes any row's decision, so dropping one never
        // changes another's inheritance.
        let survivor_folders: HashMap<i64, bool> = survivors
            .iter()
            .filter_map(|p| match p.toggle {
                SelectionToggle::Folder { dir_id, on } => Some((dir_id, on)),
                SelectionToggle::Item { .. } => None,
            })
            .collect();
        let inherited = |dir: i64, skip_own: bool| -> bool {
            chains[&dir]
                .iter()
                .skip(usize::from(skip_own))
                .find_map(|d| survivor_folders.get(d).copied())
                .unwrap_or(base_on)
        };
        let mut normalized: Vec<Placed> = Vec::new();
        for p in survivors {
            let keep = match p.toggle {
                SelectionToggle::Folder { dir_id, on } => {
                    let all = reader.known_dir(dir_id)?.totals.units_selectable;
                    all > 0 && on != inherited(dir_id, true)
                }
                SelectionToggle::Item { on, .. } => on != inherited(p.dir, false),
            };
            if keep {
                normalized.push(p);
            }
        }

        let capped = normalized.len() > MAX_TOGGLES;
        normalized.truncate(MAX_TOGGLES);

        let mut resolved = Selection {
            base_on,
            folders: HashMap::new(),
            items: HashMap::new(),
            normalized: RollbackSelection {
                base: selection.base,
                toggles: normalized.iter().map(|p| p.toggle).collect(),
            },
            capped,
            nodes: HashMap::new(),
            decisions: HashMap::new(),
            total: Picked::default(),
            chosen: Picked::default(),
        };
        for p in &normalized {
            match p.toggle {
                SelectionToggle::Folder { dir_id, on } => {
                    resolved.folders.insert(dir_id, on);
                }
                SelectionToggle::Item { seq, on } => {
                    resolved.items.insert(seq, on);
                }
            }
        }
        resolved.count(reader, &normalized, &chains)?;
        Ok(resolved)
    }

    /// The normalized selection, which the frontend sends straight back.
    pub(crate) fn normalized(&self) -> &RollbackSelection {
        &self.normalized
    }

    /// Whether normalizing refused toggles past [`MAX_TOGGLES`].
    pub(crate) fn capped(&self) -> bool {
        self.capped
    }

    /// The op's selected units, files, and bytes: the "N" of "N of M", and a selective
    /// reversal's progress totals.
    pub(crate) fn chosen(&self) -> Picked {
        self.chosen
    }

    /// Every selectable unit of the op: the "M".
    pub(crate) fn total(&self) -> Picked {
        self.total
    }

    /// The decision a folder hands down: its own folder toggle, else its nearest
    /// ancestor's, else the base. One summary read per distinct folder, cached.
    pub(crate) fn decision_at(&mut self, reader: &mut OpReader<'_>, dir: i64) -> Result<bool, OperationLogStoreError> {
        let mut walked = Vec::new();
        let mut current = Some(dir);
        let decision = loop {
            let Some(id) = current else { break self.base_on };
            if let Some(&known) = self.decisions.get(&id) {
                break known;
            }
            if let Some(&on) = self.folders.get(&id) {
                break on;
            }
            walked.push(id);
            current = match reader.dir(id)? {
                Some(row) => row.parent_dir_id,
                None => None,
            };
        };
        for id in walked {
            self.decisions.insert(id, decision);
        }
        if self.folders.contains_key(&dir) {
            self.decisions.insert(dir, decision);
        }
        Ok(decision)
    }

    /// Selected and selectable counts at or below `dir`.
    pub(crate) fn under(
        &mut self,
        reader: &mut OpReader<'_>,
        dir: i64,
    ) -> Result<(Picked, Picked), OperationLogStoreError> {
        if let Some(node) = self.nodes.get(&dir) {
            return Ok((node.chosen, node.all));
        }
        let all = match reader.dir(dir)? {
            Some(row) => picked_of(row),
            None => return Ok((Picked::default(), Picked::default())),
        };
        let on = self.decision_at(reader, dir)?;
        Ok((all.when(on), all))
    }

    /// The engine's per-row decision: does a rollback with this selection act on this
    /// row? A row's own item toggle, else its folder's decision; a folder a copy or
    /// move made goes along only when its folder's decision is on AND every
    /// selectable unit under it is selected, since a folder still holding a file
    /// can't be emptied. No DB read beyond one summary row per distinct folder.
    pub(crate) fn includes(
        &mut self,
        reader: &mut OpReader<'_>,
        facts: &RowFacts,
    ) -> Result<bool, OperationLogStoreError> {
        if let Some(&on) = self.items.get(&facts.seq) {
            return Ok(on);
        }
        let context = reader.context;
        let dir = facts.placement(context.axis);
        let on = self.decision_at(reader, dir)?;
        if facts.role(context.kind) != UnitRole::Housekeeping || !on {
            return Ok(on);
        }
        Ok(self.nodes.get(&dir).is_none_or(|node| node.chosen == node.all))
    }

    /// Count the selection: the toggles' folders and their ancestors form a small
    /// forest, each folder's selected counts follow from its children's and its own
    /// item toggles, and every folder outside it is all-or-nothing by its decision.
    fn count(
        &mut self,
        reader: &mut OpReader<'_>,
        placed: &[Placed],
        chains: &HashMap<i64, Vec<i64>>,
    ) -> Result<(), OperationLogStoreError> {
        for p in placed {
            let chain = &chains[&p.dir];
            for (i, &dir) in chain.iter().enumerate() {
                if self.nodes.contains_key(&dir) {
                    continue;
                }
                let row = reader.known_dir(dir)?;
                self.nodes.insert(
                    dir,
                    Node {
                        parent: row.parent_dir_id,
                        depth: chain.len() - i,
                        all: picked_of(&row),
                        chosen: Picked::default(),
                    },
                );
            }
        }
        let mut item_picks: HashMap<i64, (Picked, Picked)> = HashMap::new();
        for p in placed {
            if let SelectionToggle::Item { on, .. } = p.toggle {
                let entry = item_picks.entry(p.dir).or_default();
                if on {
                    entry.0 = entry.0.plus(p.pick);
                } else {
                    entry.1 = entry.1.plus(p.pick);
                }
            }
        }
        let mut order: Vec<(usize, i64)> = self.nodes.iter().map(|(&dir, node)| (node.depth, dir)).collect();
        order.sort_unstable_by(|a, b| b.cmp(a));
        let mut children: HashMap<i64, Picked> = HashMap::new();
        let mut children_all: HashMap<i64, Picked> = HashMap::new();
        for (_, dir) in order {
            let on = self.decision_at(reader, dir)?;
            let node = &self.nodes[&dir];
            let (all, parent) = (node.all, node.parent);
            let kids_chosen = children.get(&dir).copied().unwrap_or_default();
            let kids_all = children_all.get(&dir).copied().unwrap_or_default();
            let (items_on, items_off) = item_picks.get(&dir).copied().unwrap_or_default();
            // Everything here takes the folder's decision, except what a child in the
            // forest decided for itself and what an item toggle flipped.
            let mut chosen = all.minus(kids_all).when(on).plus(kids_chosen);
            chosen = if on {
                chosen.minus(items_off)
            } else {
                chosen.plus(items_on)
            };
            if let Some(node) = self.nodes.get_mut(&dir) {
                node.chosen = chosen;
            }
            if let Some(parent) = parent {
                let entry = children.entry(parent).or_default();
                *entry = entry.plus(chosen);
                let entry = children_all.entry(parent).or_default();
                *entry = entry.plus(all);
            }
        }
        for root in reader.roots()? {
            let all = picked_of(&root);
            self.total = self.total.plus(all);
            let chosen = match self.nodes.get(&root.dir_id) {
                Some(node) => node.chosen,
                None => all.when(self.base_on),
            };
            self.chosen = self.chosen.plus(chosen);
        }
        Ok(())
    }
}

fn picked_of(row: &super::summary::TreeDirRow) -> Picked {
    Picked {
        units: row.totals.units_selectable,
        files: row.totals.unit_files_selectable,
        bytes: row.totals.unit_bytes_selectable,
    }
}

/// Validate one wire toggle against the op and find its folder, `None` to drop it.
fn place(
    reader: &mut OpReader<'_>,
    kind: OpKind,
    toggle: SelectionToggle,
) -> Result<Option<Placed>, OperationLogStoreError> {
    match toggle {
        SelectionToggle::Folder { dir_id, .. } => Ok(reader.dir(dir_id)?.map(|_| Placed {
            toggle,
            dir: dir_id,
            pick: Picked::default(),
        })),
        SelectionToggle::Item { seq, on } => {
            let axis = reader.context.axis;
            let Some(row) = reader.item(seq)? else { return Ok(None) };
            let facts = row.facts.clone();
            let role = unit_role(
                kind,
                facts.row_role,
                facts.entry_type,
                facts.created_in_place(),
                facts.outcome,
                facts.skip_reason,
            );
            if role != UnitRole::Selectable {
                return Ok(None);
            }
            if facts.entry_type == EntryType::Dir {
                // A folder unit is decided by its own folder, so the item map and the
                // folder chain can't disagree about it.
                let Some(own) = facts.self_dir_id else { return Ok(None) };
                if reader.dir(own)?.is_none() {
                    return Ok(None);
                }
                return Ok(Some(Placed {
                    toggle: SelectionToggle::Folder { dir_id: own, on },
                    dir: own,
                    pick: Picked::default(),
                }));
            }
            let dir = facts.placement(axis);
            if reader.dir(dir)?.is_none() {
                return Ok(None);
            }
            let bytes = facts.size.unwrap_or(0).max(0);
            Ok(Some(Placed {
                toggle,
                dir,
                pick: Picked {
                    units: 1,
                    files: 1,
                    bytes,
                },
            }))
        }
    }
}

/// `dir` and its ancestors up to the volume root, by summary row.
fn chain_of(reader: &mut OpReader<'_>, dir: i64) -> Result<Vec<i64>, OperationLogStoreError> {
    let mut chain = Vec::new();
    let mut current = Some(dir);
    while let Some(id) = current {
        chain.push(id);
        current = reader.known_dir(id)?.parent_dir_id;
    }
    Ok(chain)
}

/// Whether `dir` is a folder unit's own folder (its contents have no checkbox).
fn is_unit_folder(
    reader: &mut OpReader<'_>,
    kind: OpKind,
    dir: i64,
    known: &mut HashMap<i64, bool>,
) -> Result<bool, OperationLogStoreError> {
    if let Some(&answer) = known.get(&dir) {
        return Ok(answer);
    }
    let answer = match reader.known_dir(dir)?.self_seq {
        Some(seq) => match reader.item(seq)? {
            Some(row) => owns_contents(kind, &row.facts),
            None => false,
        },
        None => false,
    };
    known.insert(dir, answer);
    Ok(answer)
}
