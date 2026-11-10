//! The tree's flatten, over SQL: the transfer tree's `Layout` (`crates/cmdr-operations/
//! src/transfer_tree/view.rs`), with listings read off the summary and item indexes
//! instead of an in-memory model.
//!
//! - **The top**: each volume root descends while a folder holds no files and one
//!   child folder, stopping ABOVE any folder an item row names (`self_seq`), so a
//!   trashed folder is a top row with its own checkbox. One volume root: its descended
//!   folder is the root path and its listing is the top. Several: a forest, one top
//!   row per volume, labelled with its full path.
//! - **A listing**: child folders first, then files, each by folded name (byte order),
//!   ties by id. A folder with no files and one child folder shares that child's row
//!   (keyed by the deepest folder), unless either is named by an item row.
//! - **The window**: rows under a folder = its listing plus its open children's rows,
//!   so the first row is found by descending through the open folders (O(expansion)),
//!   then rows are read forward from the listings.
//!
//! This module is where rows ARE; what each row SAYS is `layout/rows.rs`.

mod rows;

use std::collections::{HashMap, HashSet, VecDeque};

use super::TreeAxis;
use super::read::{FileRow, OpReader};
use super::selection::Selection;
use super::summary::TreeDirRow;
use super::wire::{OperationTreeRow, TreeCheckbox, TreeRowKey, TreeTally};
use crate::store::{OperationLogStoreError, OperationRow, fold_name};
use crate::types::{EntryType, ItemOutcome, OpKind, SearchCoverage, SearchCoverageReason, SkipReason};

/// The top of the tree.
enum Top {
    /// One volume root, descended: its listing is the top.
    Single(TreeDirRow),
    /// Several: each descended root is a top row.
    Forest(Vec<TreeDirRow>),
}

/// A listing's owner: `None` is the top.
type Container = Option<i64>;

/// What a folder unit hands its contents: they show its state.
#[derive(Debug, Clone, Copy)]
struct Governor {
    outcome: ItemOutcome,
    skip_reason: Option<SkipReason>,
    checkbox: TreeCheckbox,
}

/// One entry of a listing.
enum Entry {
    /// A child folder: the head of its (maybe compacted) chain.
    Folder(TreeDirRow),
    File(FileRow),
}

/// One listing being read forward.
struct Frame {
    container: Container,
    next: u64,
    size: u64,
    level: u16,
    path: String,
    governor: Option<Governor>,
    buffer: VecDeque<Entry>,
}

pub(super) struct Layout {
    kind: OpKind,
    axis: TreeAxis,
    read_only: bool,
    is_trash: bool,
    coverage: SearchCoverage,
    coverage_reason: Option<SearchCoverageReason>,
    top: Top,
    /// Open, visible folders: their open, visible children by listing position.
    kids: HashMap<Container, Vec<(u64, i64)>>,
    /// Rows under each open, visible folder.
    rows_under: HashMap<i64, u64>,
    places: HashMap<i64, Option<(Container, i64)>>,
    root_path: Option<String>,
    other_root: Option<String>,
}

fn compactable(parent: &TreeDirRow, child: &TreeDirRow) -> bool {
    parent.self_seq.is_none() && child.self_seq.is_none() && parent.direct_files == 0 && parent.child_dirs == 1
}

fn size_of(row: &TreeDirRow) -> u64 {
    (row.child_dirs + row.direct_files).max(0) as u64
}

fn count(n: i64) -> u64 {
    n.max(0) as u64
}

impl Layout {
    pub(super) fn new(
        reader: &mut OpReader<'_>,
        op: &OperationRow,
        expanded: &[i64],
        read_only: bool,
    ) -> Result<Self, OperationLogStoreError> {
        let context = reader.context;
        let mut tops = Vec::new();
        for root in reader.roots()? {
            tops.push(descend(reader, root)?);
        }
        let top = if tops.len() == 1 {
            Top::Single(tops.remove(0))
        } else {
            Top::Forest(tops)
        };
        let root_path = match &top {
            Top::Single(root) => Some(reader.path(root.dir_id)?),
            Top::Forest(_) => None,
        };
        let mut layout = Layout {
            kind: context.kind,
            axis: context.axis,
            read_only,
            is_trash: context.kind == OpKind::Trash,
            coverage: op.search_coverage,
            coverage_reason: op.search_coverage_reason,
            top,
            kids: HashMap::new(),
            rows_under: HashMap::new(),
            places: HashMap::new(),
            root_path,
            other_root: None,
        };
        layout.other_root = layout.find_other_root(reader)?;
        layout.open(reader, expanded)?;
        Ok(layout)
    }

    pub(super) fn root_path(&self) -> Option<String> {
        self.root_path.clone()
    }

    pub(super) fn other_root(&self) -> Option<String> {
        self.other_root.clone()
    }

    pub(super) fn total_rows(&self) -> u64 {
        self.rows_in(None)
    }

    /// Rows under a container: its listing plus its open children's rows.
    fn rows_in(&self, container: Container) -> u64 {
        match container {
            None => {
                let listing = match &self.top {
                    Top::Single(root) => size_of(root),
                    Top::Forest(roots) => roots.len() as u64,
                };
                listing + self.kids_rows(None)
            }
            Some(dir) => self.rows_under.get(&dir).copied().unwrap_or(0),
        }
    }

    fn kids_rows(&self, container: Container) -> u64 {
        self.kids
            .get(&container)
            .map_or(0, |kids| kids.iter().map(|(_, k)| self.rows_under[k]).sum())
    }

    /// The listing size of a container.
    fn size(&self, reader: &mut OpReader<'_>, container: Container) -> Result<u64, OperationLogStoreError> {
        Ok(match (container, &self.top) {
            (None, Top::Single(root)) => size_of(root),
            (None, Top::Forest(roots)) => roots.len() as u64,
            (Some(dir), _) => size_of(&reader.known_dir(dir)?),
        })
    }

    /// The dir whose child folders and files a container lists.
    fn listing_dir(&self, container: Container) -> Option<i64> {
        match (container, &self.top) {
            (Some(dir), _) => Some(dir),
            (None, Top::Single(root)) => Some(root.dir_id),
            (None, Top::Forest(_)) => None,
        }
    }

    /// Validate the expanded keys and lay out the visible open folders.
    fn open(&mut self, reader: &mut OpReader<'_>, expanded: &[i64]) -> Result<(), OperationLogStoreError> {
        let mut placed: HashMap<i64, (Container, i64)> = HashMap::new();
        for &dir in expanded {
            if let Some(place) = self.place(reader, dir)? {
                placed.insert(dir, place);
            }
        }
        // Visible: every open ancestor up to the top is open too.
        let mut visible: HashSet<i64> = HashSet::new();
        for &dir in placed.keys() {
            let mut current = dir;
            let shown = loop {
                match placed.get(&current) {
                    Some((None, _)) => break true,
                    Some((Some(parent), _)) => current = *parent,
                    None => break false,
                }
            };
            if shown {
                visible.insert(dir);
            }
        }
        for &dir in &visible {
            let (parent, head) = placed[&dir];
            let pos = self.position_in(reader, parent, head)?;
            self.kids.entry(parent).or_default().push((pos, dir));
        }
        for kids in self.kids.values_mut() {
            kids.sort_unstable();
        }
        // Rows under each open folder, deepest first.
        let mut order: Vec<(usize, i64)> = visible
            .iter()
            .map(|&dir| {
                let mut depth = 0;
                let mut current = placed[&dir].0;
                while let Some(parent) = current {
                    depth += 1;
                    current = placed[&parent].0;
                }
                (depth, dir)
            })
            .collect();
        order.sort_unstable_by(|a, b| b.cmp(a));
        for (_, dir) in order {
            let rows = size_of(&reader.known_dir(dir)?) + self.kids_rows(Some(dir));
            self.rows_under.insert(dir, rows);
        }
        Ok(())
    }

    /// Where a folder row sits: the container listing it and the head of its chain.
    /// `None` when `dir` isn't a row (inside a compacted chain, above the root, or not
    /// in this op).
    fn place(
        &mut self,
        reader: &mut OpReader<'_>,
        dir: i64,
    ) -> Result<Option<(Container, i64)>, OperationLogStoreError> {
        if let Some(known) = self.places.get(&dir) {
            return Ok(*known);
        }
        let found = self.find_place(reader, dir)?;
        self.places.insert(dir, found);
        Ok(found)
    }

    fn find_place(
        &self,
        reader: &mut OpReader<'_>,
        dir: i64,
    ) -> Result<Option<(Container, i64)>, OperationLogStoreError> {
        let Some(row) = reader.dir(dir)?.cloned() else {
            return Ok(None);
        };
        match &self.top {
            Top::Single(root) if root.dir_id == dir => return Ok(None),
            Top::Forest(roots) if roots.iter().any(|r| r.dir_id == dir) => return Ok(Some((None, dir))),
            _ => {}
        }
        if only_child_compacts(reader, &row)? {
            return Ok(None);
        }
        let mut current = row;
        loop {
            let Some(parent_id) = current.parent_dir_id else {
                return Ok(None);
            };
            match &self.top {
                Top::Single(root) if root.dir_id == parent_id => return Ok(Some((None, current.dir_id))),
                Top::Forest(roots) if roots.iter().any(|r| r.dir_id == parent_id) => {
                    return Ok(Some((Some(parent_id), current.dir_id)));
                }
                _ => {}
            }
            let parent = reader.known_dir(parent_id)?;
            if !compactable(&parent, &current) {
                return Ok(Some((Some(parent_id), current.dir_id)));
            }
            current = parent;
        }
    }

    /// Where the chain headed by `head` sits in `container`'s listing.
    fn position_in(
        &self,
        reader: &mut OpReader<'_>,
        container: Container,
        head: i64,
    ) -> Result<u64, OperationLogStoreError> {
        if let (None, Top::Forest(roots)) = (container, &self.top) {
            return Ok(roots.iter().position(|r| r.dir_id == head).unwrap_or(0) as u64);
        }
        let Some(dir) = self.listing_dir(container) else {
            return Ok(0);
        };
        let parent = reader.known_dir(dir)?;
        let head = reader.known_dir(head)?;
        reader.position_of_dir(dir, &head, count(parent.child_dirs))
    }

    /// Rows from flat index `offset`, at most `limit`, and the open rows above the first.
    pub(super) fn window(
        &self,
        reader: &mut OpReader<'_>,
        selection: &mut Selection,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<OperationTreeRow>, Vec<OperationTreeRow>), OperationLogStoreError> {
        let mut rows = Vec::new();
        let mut ancestors = Vec::new();
        if offset >= self.total_rows() || limit == 0 {
            return Ok((rows, ancestors));
        }
        let mut stack = vec![self.frame(reader, None, 1, None)?];
        // Descend to the first row through the open folders above it.
        let mut remaining = offset;
        let mut base = 0u64;
        loop {
            let container = stack.last().and_then(|f| f.container);
            let mut skipped = 0u64;
            let mut inside = None;
            for &(pos, kid) in self.kids.get(&container).map_or(&[][..], Vec::as_slice) {
                let index = pos + skipped;
                if remaining <= index {
                    break;
                }
                if remaining <= index + self.rows_under[&kid] {
                    inside = Some((pos, kid, index));
                    break;
                }
                skipped += self.rows_under[&kid];
            }
            let frame = stack.last_mut().expect("the top frame");
            match inside {
                None => {
                    frame.next = remaining - skipped;
                    break;
                }
                Some((pos, kid, index)) => {
                    frame.next = pos + 1;
                    let head = self.entry_at(reader, frame.container, pos)?;
                    let (level, governor, path) = (frame.level, frame.governor, frame.path.clone());
                    let parent_size = frame.size;
                    let (row, child_governor) = self.folder_row(
                        reader,
                        selection,
                        head,
                        base + index,
                        level,
                        pos,
                        parent_size,
                        governor,
                        &path,
                    )?;
                    ancestors.push(row);
                    stack.push(self.frame(reader, Some(kid), level + 1, child_governor)?);
                    remaining -= index + 1;
                    base += index + 1;
                }
            }
        }
        // Read forward.
        let mut index = offset;
        while (rows.len() as u64) < limit {
            let Some(frame) = stack.last_mut() else { break };
            if frame.next >= frame.size {
                stack.pop();
                continue;
            }
            if frame.buffer.is_empty() {
                let want = limit - rows.len() as u64;
                let entries = self.entries(reader, frame.container, frame.next, want)?;
                let frame = stack.last_mut().expect("the frame just read");
                frame.buffer.extend(entries);
            }
            let frame = stack.last_mut().expect("the frame just read");
            let Some(entry) = frame.buffer.pop_front() else { break };
            let pos = frame.next;
            frame.next += 1;
            let (level, governor, path, size) = (frame.level, frame.governor, frame.path.clone(), frame.size);
            match entry {
                Entry::File(file) => {
                    rows.push(self.file_row(reader, selection, &file, index, level, pos, size, governor, &path)?);
                }
                Entry::Folder(head) => {
                    let (row, child_governor) =
                        self.folder_row(reader, selection, head, index, level, pos, size, governor, &path)?;
                    let open = match row.key {
                        TreeRowKey::Folder { dir_id } if row.expanded => Some(dir_id),
                        _ => None,
                    };
                    rows.push(row);
                    if let Some(dir) = open {
                        stack.push(self.frame(reader, Some(dir), level + 1, child_governor)?);
                    }
                }
            }
            index += 1;
        }
        Ok((rows, ancestors))
    }

    fn frame(
        &self,
        reader: &mut OpReader<'_>,
        container: Container,
        level: u16,
        governor: Option<Governor>,
    ) -> Result<Frame, OperationLogStoreError> {
        let path = match self.listing_dir(container) {
            Some(dir) => reader.path(dir)?,
            None => String::new(),
        };
        Ok(Frame {
            container,
            next: 0,
            size: self.size(reader, container)?,
            level,
            path,
            governor,
            buffer: VecDeque::new(),
        })
    }

    /// Up to `want` entries of a container's listing from position `from`.
    fn entries(
        &self,
        reader: &mut OpReader<'_>,
        container: Container,
        from: u64,
        want: u64,
    ) -> Result<Vec<Entry>, OperationLogStoreError> {
        let Some(dir) = self.listing_dir(container) else {
            let Top::Forest(roots) = &self.top else {
                return Ok(Vec::new());
            };
            return Ok(roots
                .iter()
                .skip(from as usize)
                .take(want as usize)
                .cloned()
                .map(Entry::Folder)
                .collect());
        };
        let row = reader.known_dir(dir)?;
        let (folders, files) = (count(row.child_dirs), count(row.direct_files));
        let mut out = Vec::new();
        if from < folders {
            let take = want.min(folders - from);
            out.extend(
                reader
                    .child_dirs(dir, from, take, folders)?
                    .into_iter()
                    .map(Entry::Folder),
            );
        }
        let left = want - out.len() as u64;
        let file_from = from.max(folders) - folders;
        if left > 0 && file_from < files {
            let take = left.min(files - file_from);
            out.extend(reader.files(dir, file_from, take, files)?.into_iter().map(Entry::File));
        }
        Ok(out)
    }

    /// The single entry at `pos` of a container's listing, a folder (an open row).
    fn entry_at(
        &self,
        reader: &mut OpReader<'_>,
        container: Container,
        pos: u64,
    ) -> Result<TreeDirRow, OperationLogStoreError> {
        match self.entries(reader, container, pos, 1)?.pop() {
            Some(Entry::Folder(head)) => Ok(head),
            _ => Err(OperationLogStoreError::Decode {
                column: "operation_tree_dirs",
                value: format!("no folder at listing position {pos}"),
            }),
        }
    }

    /// The flat index of the row `key` names, or of its nearest visible ancestor when
    /// a closed folder hides it. `None` when it isn't in this tree.
    pub(super) fn cursor_index(
        &mut self,
        reader: &mut OpReader<'_>,
        key: TreeRowKey,
    ) -> Result<Option<u64>, OperationLogStoreError> {
        let folder = |dir| TreeRowKey::Folder { dir_id: dir };
        let key = match key {
            TreeRowKey::Item { seq } => match reader.item(seq)?.map(|row| row.facts.clone()) {
                Some(facts) if facts.entry_type == EntryType::Dir => match facts.self_dir_id {
                    Some(own) => folder(own),
                    None => return Ok(None),
                },
                Some(_) => key,
                None => return Ok(None),
            },
            TreeRowKey::Folder { .. } => key,
        };
        // (container, position) from the row up to the top.
        let mut steps: Vec<(Container, u64)> = Vec::new();
        let mut container = match key {
            TreeRowKey::Item { seq } => {
                let Some(file) = reader.item(seq)?.cloned() else {
                    return Ok(None);
                };
                let dir = file.facts.placement(self.axis);
                let container = match &self.top {
                    Top::Single(root) if root.dir_id == dir => None,
                    _ => match self.place(reader, dir)? {
                        Some(_) => Some(dir),
                        None => return Ok(None),
                    },
                };
                let row = reader.known_dir(dir)?;
                let name = fold_name(file.facts.axis_side(self.axis).1);
                let pos = count(row.child_dirs)
                    + reader.position_of_file(dir, &name, file.item_id, count(row.direct_files))?;
                steps.push((container, pos));
                container
            }
            // The folder's own row is the first step the walk up places.
            TreeRowKey::Folder { dir_id } => Some(dir_id),
        };
        while let Some(dir) = container {
            let Some((parent, head)) = self.place(reader, dir)? else {
                return Ok(None);
            };
            let pos = self.position_in(reader, parent, head)?;
            steps.push((parent, pos));
            container = parent;
        }
        steps.reverse();
        let mut first_child = 0u64;
        let mut row = None;
        for (container, pos) in steps {
            if let Some(dir) = container
                && !self.rows_under.contains_key(&dir)
            {
                return Ok(row);
            }
            let before: u64 = self.kids.get(&container).map_or(0, |kids| {
                kids.iter()
                    .filter(|(p, _)| *p < pos)
                    .map(|(_, k)| self.rows_under[k])
                    .sum()
            });
            let index = first_child + pos + before;
            row = Some(index);
            first_child = index + 1;
        }
        Ok(row)
    }

    /// The op's own counts: its volume roots' totals, and the selection's.
    pub(super) fn op_tally(
        &self,
        reader: &mut OpReader<'_>,
        selection: &Selection,
    ) -> Result<TreeTally, OperationLogStoreError> {
        let mut tally = TreeTally {
            selected: count(selection.chosen().units),
            selectable: count(selection.total().units),
            ..TreeTally::default()
        };
        for root in reader.roots()? {
            let t = &root.totals;
            tally.files += count(t.files);
            tally.bytes += count(t.bytes);
            tally.rolled_back += count(t.units_rolled_back);
            tally.held_back += count(t.units_held_back);
            tally.unfinished += count(t.units_unfinished);
            tally.housekeeping_waiting += count(t.housekeeping_waiting);
        }
        Ok(tally)
    }
}

/// Whether `row` shares its row with its only child folder.
fn only_child_compacts(reader: &mut OpReader<'_>, row: &TreeDirRow) -> Result<bool, OperationLogStoreError> {
    if row.self_seq.is_some() || row.direct_files != 0 || row.child_dirs != 1 {
        return Ok(false);
    }
    let child = reader.child_dirs(row.dir_id, 0, 1, 1)?;
    Ok(child.first().is_some_and(|child| compactable(row, child)))
}

/// A volume root descended to the deepest folder that holds the whole subtree.
fn descend(reader: &mut OpReader<'_>, root: TreeDirRow) -> Result<TreeDirRow, OperationLogStoreError> {
    let mut current = root;
    while only_child_compacts(reader, &current)? {
        current = reader.child_dirs(current.dir_id, 0, 1, 1)?.remove(0);
    }
    Ok(current)
}
