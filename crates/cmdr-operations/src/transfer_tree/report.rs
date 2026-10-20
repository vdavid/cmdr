//! How a driver reports into its operation's tree: one [`TreeReporter`] per
//! operation, one [`TreeLeaf`] per file it transfers, one [`TreeLevel`] per
//! folder a walker lists.
//!
//! ❗ Every call here returns nothing a driver could act on, and every one is a
//! no-op when there is no tree (an operation that never scanned, a test that
//! installs none) or no row (a file the scan never recorded). That's the
//! data-safety half of the tree: it observes a transfer, and nothing a driver
//! does may depend on it. A tree bug is caught in `TransferTree::guarded`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::TransferTree;
use super::model::{NodeId, NodeKind};
use crate::types::WriteOperationError;

/// An operation's tree as its drivers see it: cheap to clone, and silent when
/// the operation has none.
#[derive(Clone)]
pub(crate) struct TreeReporter(Option<Arc<TransferTree>>);

impl TreeReporter {
    pub(super) fn new(tree: Option<Arc<TransferTree>>) -> Self {
        Self(tree)
    }

    /// A reporter with no tree behind it, for a transfer nobody watches.
    pub(crate) fn none() -> Self {
        Self(None)
    }

    /// The top-level row of a selected source.
    pub(crate) fn source(&self, path: &Path) -> Option<NodeId> {
        self.0.as_ref()?.read(|model| model.source_node(path))?
    }

    /// Opens one file's row for its whole transfer (`TreeLeaf`).
    pub(crate) fn leaf(&self, node: Option<NodeId>) -> TreeLeaf {
        TreeLeaf::begin(self, node)
    }

    /// The level `folder` a walker has listed, indexed by name once. `listed`
    /// is the listing, `(name, is_folder, size)` per child: every child the scan
    /// never saw joins the tree right here, before any child lands, so the
    /// folder's counts hold the whole listing and it can't read done while a
    /// newcomer (a file that arrived since the scan, last in tree order, say)
    /// is still to come. One lock for the level.
    pub(crate) fn level<'a>(
        &self,
        folder: Option<NodeId>,
        listed: impl IntoIterator<Item = (&'a str, bool, u64)>,
    ) -> TreeLevel {
        let by_name = match (&self.0, folder) {
            (Some(tree), Some(folder)) => tree
                .mutate(|model| {
                    if !model.kind(folder).is_folder() {
                        return None;
                    }
                    let mut by_name: HashMap<String, (NodeId, bool)> = model
                        .children_by_name(folder)
                        .into_iter()
                        .map(|(name, child)| (name, (child, model.kind(child).is_folder())))
                        .collect();
                    for (name, is_folder, size) in listed {
                        if by_name.contains_key(name) {
                            continue;
                        }
                        let kind = if is_folder { NodeKind::Folder } else { NodeKind::File };
                        let node = model.discover(folder, name, kind, size);
                        by_name.insert(name.to_owned(), (node, is_folder));
                    }
                    Some(by_name)
                })
                .flatten(),
            _ => None,
        };
        TreeLevel {
            reporter: self.clone(),
            folder: by_name.as_ref().and(folder),
            by_name: by_name.unwrap_or_default(),
        }
    }

    /// `node` landed whole at once (a rename, or a whole level of empty
    /// folders): every file below it is done.
    pub(crate) fn complete_whole(&self, node: Option<NodeId>) {
        self.on(node, |model, node| model.complete_subtree(node));
    }

    /// `node` was skipped whole: a file, or a folder and everything below it.
    pub(crate) fn skip_whole(&self, node: Option<NodeId>) {
        self.on(node, |model, node| model.skip_subtree(node));
    }

    /// A folder the driver created or merged into. Only an empty folder's row
    /// reads it.
    pub(crate) fn folder_in_place(&self, node: Option<NodeId>) {
        self.on(node, |model, node| model.folder_created(node));
    }

    /// Every empty folder is in place (the local engine's scanned-dirs pass).
    pub(crate) fn empty_folders_in_place(&self) {
        if let Some(tree) = &self.0 {
            tree.mutate(|model| model.empty_folders_created());
        }
    }

    /// Marks the row a failure names, by the typed path it carries.
    pub(crate) fn fail_at(&self, error: &WriteOperationError) {
        let Some(tree) = &self.0 else { return };
        // A failure that also set the user's originals aside, or kept new
        // files, failed on its cause's item.
        let mut on_item = error;
        while let WriteOperationError::OriginalsKeptAside { cause, .. }
        | WriteOperationError::NewFilesKept { cause, .. } = on_item
        {
            on_item = cause.as_ref();
        }
        let Some(path) = on_item.item_path() else { return };
        let failure = error.clone();
        tree.mutate(|model| {
            if let Some(node) = model.node_at_path(Path::new(path)) {
                model.fail(node, failure);
            }
        });
    }

    fn on(&self, node: Option<NodeId>, apply: impl FnOnce(&mut super::TreeModel, NodeId)) {
        if let (Some(tree), Some(node)) = (&self.0, node) {
            tree.mutate(|model| apply(model, node));
        }
    }
}

/// One file's row for the whole of its transfer, settled exactly once: by
/// [`complete`](Self::complete) when it landed, [`skip`](Self::skip) when the
/// transfer declined it, or `Drop` when it stopped without either (a failure,
/// a cancel, a retry that gave up): back to waiting.
///
/// Its byte counter is the one place the file's in-flight bytes live. The
/// driver advances it per chunk with a plain atomic store, never the tree's
/// lock; a view reads it.
pub(crate) struct TreeLeaf {
    tree: Option<Arc<TransferTree>>,
    node: Option<NodeId>,
    bytes: Arc<AtomicU64>,
    settled: AtomicBool,
}

impl TreeLeaf {
    fn begin(reporter: &TreeReporter, node: Option<NodeId>) -> Self {
        let bytes = Arc::new(AtomicU64::new(0));
        let tree = match (&reporter.0, node) {
            (Some(tree), Some(node)) => {
                let live = Arc::clone(&bytes);
                tree.mutate(|model| model.begin(node, live));
                Some(Arc::clone(tree))
            }
            _ => None,
        };
        Self {
            tree,
            node,
            bytes,
            settled: AtomicBool::new(false),
        }
    }

    /// A leaf with no row, for a test that drives a per-file copier directly.
    #[cfg(test)]
    pub(crate) fn detached() -> Self {
        TreeReporter::none().leaf(None)
    }

    /// The file's live byte count, which the driver advances per chunk.
    pub(crate) fn bytes(&self) -> &AtomicU64 {
        &self.bytes
    }

    /// The file landed.
    pub(crate) fn complete(&self) {
        self.settle(|model, node| model.complete(node));
    }

    /// The transfer declined the file.
    pub(crate) fn skip(&self) {
        self.settle(|model, node| model.skip(node));
    }

    fn settle(&self, apply: impl FnOnce(&mut super::TreeModel, NodeId)) {
        if self.settled.swap(true, Ordering::Relaxed) {
            return;
        }
        if let (Some(tree), Some(node)) = (&self.tree, self.node) {
            tree.mutate(|model| apply(model, node));
        }
    }
}

impl Drop for TreeLeaf {
    fn drop(&mut self) {
        self.settle(|model, node| model.withdraw(node));
    }
}

/// One folder's children by name, built when a walker lists that folder and
/// dropped with the level. ❌ No global path map: a walker only ever asks about
/// the level in its hand.
pub(crate) struct TreeLevel {
    reporter: TreeReporter,
    /// `None` when the tree has no folder row here: then every child is `None`.
    folder: Option<NodeId>,
    /// Each child's row and whether it's a folder.
    by_name: HashMap<String, (NodeId, bool)>,
}

impl TreeLevel {
    /// The row of the child called `name`, if the tree holds one of the same
    /// kind. The level's listing already brought in every child the scan never
    /// saw; a name outside that listing joins the tree here.
    pub(crate) fn child(&mut self, name: &str, is_folder: bool, size: u64) -> Option<NodeId> {
        let folder = self.folder?;
        if let Some(&(node, was_folder)) = self.by_name.get(name) {
            // The scan saw something else under this name (it changed since):
            // no row, rather than a folder walked as a file.
            return (was_folder == is_folder).then_some(node);
        }
        let kind = if is_folder { NodeKind::Folder } else { NodeKind::File };
        let node = self
            .reporter
            .0
            .as_ref()?
            .mutate(|model| model.discover(folder, name, kind, size))?;
        self.by_name.insert(name.to_owned(), (node, is_folder));
        Some(node)
    }

    /// This level's own row.
    pub(crate) fn folder(&self) -> Option<NodeId> {
        self.folder
    }
}
