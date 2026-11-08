//! What a row SAYS: one listing entry turned into an `OperationTreeRow` (its label and
//! path, role, outcome, checkbox, tally, and the other end). Where rows ARE (the
//! flatten, the window, the cursor) is the parent module's; these builders share
//! nothing with the walk beyond `&self`.

use super::super::read::{FileRow, OpReader, join_path};
use super::super::selection::{Selection, owns_contents};
use super::super::summary::{RowFacts, TreeDirRow};
use super::super::view::{self, OperationTreeRow, TreeCheckbox, TreeRowKey, TreeTally, UnexpandableReason};
use super::super::{TreeAxis, UnitRole};
use super::{Governor, Layout, Top, count, only_child_compacts, size_of};
use crate::store::OperationLogStoreError;
use crate::types::{EntryType, SearchCoverage, SearchCoverageReason};

impl Layout {
    #[allow(
        clippy::too_many_arguments,
        reason = "one row's place in its listing; a struct for it would be built and torn down per row"
    )]
    pub(super) fn folder_row(
        &self,
        reader: &mut OpReader<'_>,
        selection: &mut Selection,
        head: TreeDirRow,
        index: u64,
        level: u16,
        pos: u64,
        set_size: u64,
        governor: Option<Governor>,
        parent_path: &str,
    ) -> Result<(OperationTreeRow, Option<Governor>), OperationLogStoreError> {
        let is_top_of_forest = matches!(&self.top, Top::Forest(_)) && level == 1;
        // Follow the compacted chain to its deepest folder.
        let mut names = vec![reader.dir_name(head.dir_id)?.0];
        let mut deepest = head;
        while only_child_compacts(reader, &deepest)? {
            let child = reader.child_dirs(deepest.dir_id, 0, 1, 1)?.remove(0);
            names.push(reader.dir_name(child.dir_id)?.0);
            deepest = child;
        }
        let dir = deepest.dir_id;
        let full_path = if is_top_of_forest {
            reader.path(dir)?
        } else {
            names
                .iter()
                .fold(parent_path.to_string(), |path, name| join_path(&path, name))
        };
        let label = if is_top_of_forest {
            full_path.clone()
        } else {
            names.join("/")
        };

        let own = match deepest.self_seq {
            Some(seq) => reader.item(seq)?.map(|row| row.facts.clone()),
            None => None,
        };
        let role = own.as_ref().map(|facts| facts.role(self.kind));
        let (chosen, all) = selection.under(reader, dir)?;
        let tri_state = if all.units == 0 {
            TreeCheckbox::None
        } else if chosen == all {
            TreeCheckbox::On
        } else if chosen.units == 0 {
            TreeCheckbox::Off
        } else {
            TreeCheckbox::Mixed
        };
        let (mut outcome, mut skip_reason) = (
            own.as_ref().map(|f| f.outcome),
            own.as_ref().and_then(|f| f.skip_reason),
        );
        let checkbox = match (role, &own) {
            _ if self.read_only => TreeCheckbox::None,
            (Some(UnitRole::Selectable), Some(facts)) => on_off(selection.includes(reader, facts)?),
            (Some(UnitRole::Contents), _) => governor.map_or(TreeCheckbox::None, |g| g.checkbox),
            (Some(UnitRole::Settled | UnitRole::NotReversible), _) => TreeCheckbox::None,
            _ => tri_state,
        };
        if role == Some(UnitRole::Contents)
            && let Some(g) = governor
        {
            outcome = Some(g.outcome);
            skip_reason = g.skip_reason;
        }
        let child_governor = match &own {
            Some(facts) if owns_contents(self.kind, facts) => Some(Governor {
                outcome: facts.outcome,
                skip_reason: facts.skip_reason,
                checkbox,
            }),
            _ => governor,
        };

        let listing = size_of(&deepest);
        let unexpandable = (listing == 0).then(|| match &own {
            Some(facts) if owns_contents(self.kind, facts) && self.coverage == SearchCoverage::TopLevelOnly => {
                if self.coverage_reason == Some(SearchCoverageReason::Capped) {
                    UnexpandableReason::Capped
                } else {
                    UnexpandableReason::NotRecorded
                }
            }
            _ => UnexpandableReason::Empty,
        });
        let t = &deepest.totals;
        let tally = TreeTally {
            files: count(t.files),
            bytes: count(t.bytes),
            selected: count(chosen.units),
            selectable: count(all.units),
            rolled_back: count(t.units_rolled_back),
            held_back: count(t.units_held_back),
            unfinished: count(t.units_unfinished),
            housekeeping_waiting: count(t.housekeeping_waiting),
        };
        let other_end = match &own {
            Some(facts) => self.other_end(reader, facts, &full_path)?,
            None => None,
        };
        let row = OperationTreeRow {
            index: view::clamp_u32(index),
            key: TreeRowKey::Folder { dir_id: dir },
            label,
            level,
            entry_type: EntryType::Dir,
            expandable: listing > 0,
            expanded: self.rows_under.contains_key(&dir),
            pos_in_set: view::clamp_u32(pos + 1),
            set_size: view::clamp_u32(set_size),
            role: role.map(Into::into),
            outcome,
            skip_reason,
            checkbox,
            size: None,
            tally: Some(tally),
            full_path,
            other_end,
            unexpandable,
        };
        Ok((row, child_governor))
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one row's place in its listing; a struct for it would be built and torn down per row"
    )]
    pub(super) fn file_row(
        &self,
        reader: &mut OpReader<'_>,
        selection: &mut Selection,
        file: &FileRow,
        index: u64,
        level: u16,
        pos: u64,
        set_size: u64,
        governor: Option<Governor>,
        parent_path: &str,
    ) -> Result<OperationTreeRow, OperationLogStoreError> {
        let facts = &file.facts;
        let name = facts.axis_side(self.axis).1.to_string();
        let full_path = join_path(parent_path, &name);
        let role = facts.role(self.kind);
        let (mut outcome, mut skip_reason) = (facts.outcome, facts.skip_reason);
        let checkbox = match role {
            _ if self.read_only => TreeCheckbox::None,
            UnitRole::Selectable => on_off(selection.includes(reader, facts)?),
            UnitRole::Contents => governor.map_or(TreeCheckbox::None, |g| g.checkbox),
            UnitRole::Housekeeping | UnitRole::Settled | UnitRole::NotReversible => TreeCheckbox::None,
        };
        if role == UnitRole::Contents
            && let Some(g) = governor
        {
            outcome = g.outcome;
            skip_reason = g.skip_reason;
        }
        Ok(OperationTreeRow {
            index: view::clamp_u32(index),
            key: TreeRowKey::Item { seq: facts.seq },
            label: name,
            level,
            entry_type: EntryType::File,
            expandable: false,
            expanded: false,
            pos_in_set: view::clamp_u32(pos + 1),
            set_size: view::clamp_u32(set_size),
            role: Some(role.into()),
            outcome: Some(outcome),
            skip_reason,
            checkbox,
            size: facts.size,
            tally: None,
            other_end: self.other_end(reader, facts, &full_path)?,
            full_path,
            unexpandable: None,
        })
    }

    /// The other side of a row with both, unless the other root plus this row's
    /// relative path already says it. Never for a trash: the Trash's names mean
    /// nothing to a person.
    fn other_end(
        &self,
        reader: &mut OpReader<'_>,
        facts: &RowFacts,
        axis_path: &str,
    ) -> Result<Option<String>, OperationLogStoreError> {
        if self.is_trash {
            return Ok(None);
        }
        let Some(other) = self.other_path(reader, facts)? else {
            return Ok(None);
        };
        if other == axis_path {
            return Ok(None);
        }
        let expected = match (&self.other_root, self.relative(axis_path)) {
            (Some(root), Some(rel)) => Some(format!("{root}{rel}")),
            _ => None,
        };
        Ok((expected.as_deref() != Some(other.as_str())).then_some(other))
    }

    /// The full path of a row's side off the axis, when it has one.
    fn other_path(
        &self,
        reader: &mut OpReader<'_>,
        facts: &RowFacts,
    ) -> Result<Option<String>, OperationLogStoreError> {
        let (dir, name) = match (self.axis, &facts.dest) {
            (TreeAxis::Dest, Some(_)) => (facts.source_dir_id, facts.source_name.as_str()),
            (TreeAxis::Source, Some((dir, name))) => (*dir, name.as_str()),
            (_, None) => return Ok(None),
        };
        Ok(Some(join_path(&reader.path(dir)?, name)))
    }

    /// `path` relative to the root path, with its leading `/`.
    fn relative<'p>(&self, path: &'p str) -> Option<&'p str> {
        let root = self.root_path.as_deref()?;
        if root == "/" {
            return Some(path);
        }
        path.strip_prefix(root).filter(|rel| rel.starts_with('/'))
    }

    /// Where the other side's rows sit: read off the first of the op's first rows that
    /// has both sides and whose other path ends with its relative path.
    pub(super) fn find_other_root(&self, reader: &mut OpReader<'_>) -> Result<Option<String>, OperationLogStoreError> {
        const PROBE: i64 = 64;
        if self.is_trash || self.root_path.is_none() {
            return Ok(None);
        }
        for row in reader.first_rows(PROBE)? {
            let facts = &row.facts;
            if facts.dest.is_none() {
                continue;
            }
            let (dir, name) = facts.axis_side(self.axis);
            let axis_path = join_path(&reader.path(dir)?, name);
            let Some(other) = self.other_path(reader, facts)? else {
                continue;
            };
            let Some(rel) = self.relative(&axis_path) else { continue };
            return Ok(other.strip_suffix(rel).map(str::to_string));
        }
        Ok(None)
    }
}

fn on_off(on: bool) -> TreeCheckbox {
    if on { TreeCheckbox::On } else { TreeCheckbox::Off }
}
