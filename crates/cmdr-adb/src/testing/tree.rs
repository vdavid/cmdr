//! The in-memory filesystem the fake ADB server serves: what a device would
//! have on disk, with no device involved.
//!
//! [`FakeTree::read_only`] is the one fault switch here; every write then
//! answers `EROFS`.

use std::collections::BTreeMap;

use crate::errors::{EEXIST, EISDIR, ENOENT, ENOTEMPTY_DEVICE, EROFS};

/// One node of the in-memory device filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeNode {
    /// A regular file.
    File {
        /// Its contents.
        data: Vec<u8>,
        /// Its full mode word (type bits included).
        mode: u32,
        /// Modification time, seconds since the epoch.
        mtime: i64,
    },
    /// A directory.
    Dir {
        /// Its full mode word.
        mode: u32,
        /// Modification time.
        mtime: i64,
    },
    /// A symlink. The sync service reports it as a link; `readlink -f`
    /// resolves it.
    Symlink {
        /// Where it points.
        target: String,
        /// Modification time.
        mtime: i64,
    },
}

impl FakeNode {
    /// The mode word the sync service reports.
    pub fn mode(&self) -> u32 {
        match self {
            Self::File { mode, .. } | Self::Dir { mode, .. } => *mode,
            Self::Symlink { .. } => 0o120777,
        }
    }

    /// The size the sync service reports.
    pub fn size(&self) -> u64 {
        match self {
            Self::File { data, .. } => data.len() as u64,
            Self::Dir { .. } => 4096,
            Self::Symlink { target, .. } => target.len() as u64,
        }
    }

    /// The mtime the sync service reports.
    pub fn mtime(&self) -> i64 {
        match self {
            Self::File { mtime, .. } | Self::Dir { mtime, .. } | Self::Symlink { mtime, .. } => *mtime,
        }
    }
}

/// The in-memory device filesystem, keyed by absolute path.
#[derive(Debug, Clone)]
pub struct FakeTree {
    nodes: BTreeMap<String, FakeNode>,
    /// The filesystems `df -k` reports on, see [`FakeTree::mount_for`].
    mounts: Vec<FakeMount>,
    /// When set, every write answers `EROFS`.
    pub read_only: bool,
}

impl Default for FakeTree {
    fn default() -> Self {
        Self::new()
    }
}

/// The mtime every node gets unless a test sets one: 2026-01-01T00:00:00Z.
pub const DEFAULT_MTIME: i64 = 1_767_225_600;

/// The header toybox's `df -k` prints above its rows.
pub const DF_K_HEADER: &str = "Filesystem      1K-blocks     Used Available Use% Mounted on";

/// One filesystem mounted in the fake device, as `df -k` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeMount {
    /// Where it's mounted.
    pub path: String,
    /// The row `df -k` prints for it, below [`DF_K_HEADER`].
    pub df_row: String,
}

impl FakeMount {
    /// A mount whose `df -k` row is `row` verbatim, for a real device's output.
    pub fn with_row(path: &str, row: &str) -> Self {
        Self {
            path: FakeTree::normalize(path),
            df_row: row.to_string(),
        }
    }

    /// A mount of `total_kib` with `available_kib` free, in toybox's layout.
    pub fn sized(path: &str, device: &str, total_kib: u64, available_kib: u64) -> Self {
        let path = FakeTree::normalize(path);
        let used = total_kib.saturating_sub(available_kib);
        let pct = (used * 100).checked_div(total_kib).unwrap_or(0);
        let df_row = format!("{device:<15} {total_kib:>9} {used:>8} {available_kib:>9} {pct:>3}% {path}");
        Self { path, df_row }
    }
}

impl FakeTree {
    /// A tree holding `/` and `/sdcard`, mounted the way a phone mounts them: a
    /// full read-only system image at `/`, and the shared storage at `/sdcard`.
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: BTreeMap::new(),
            mounts: Vec::new(),
            read_only: false,
        };
        tree.add_dir("/");
        tree.add_dir("/sdcard");
        tree.mount(FakeMount::sized("/", "/dev/block/dm-7", 1_046_868, 0))
            .mount(FakeMount::sized("/sdcard", "/dev/fuse", 118_120_468, 96_764_008));
        tree
    }

    /// Normalizes a device path: leading `/`, no trailing `/` (except `/`).
    pub fn normalize(path: &str) -> String {
        let mut out = String::from("/");
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            if !out.ends_with('/') {
                out.push('/');
            }
            out.push_str(part);
        }
        out
    }

    fn parent_of(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }
        let idx = path.rfind('/')?;
        Some(if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        })
    }

    /// Adds a directory, creating ancestors. Chainable.
    pub fn add_dir(&mut self, path: &str) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.entry(path).or_insert(FakeNode::Dir {
            mode: 0o040755,
            mtime: DEFAULT_MTIME,
        });
        self
    }

    /// Adds a file with `data`, creating ancestors. Chainable.
    pub fn add_file(&mut self, path: &str, data: &[u8]) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.insert(
            path,
            FakeNode::File {
                data: data.to_vec(),
                mode: 0o100644,
                mtime: DEFAULT_MTIME,
            },
        );
        self
    }

    /// Adds a symlink to `target`, creating ancestors. Chainable.
    pub fn add_symlink(&mut self, path: &str, target: &str) -> &mut Self {
        let path = Self::normalize(path);
        if let Some(parent) = Self::parent_of(&path) {
            self.add_dir(&parent);
        }
        self.nodes.insert(
            path,
            FakeNode::Symlink {
                target: target.to_string(),
                mtime: DEFAULT_MTIME,
            },
        );
        self
    }

    /// Mounts a filesystem, replacing one already at the same path. Chainable.
    pub fn mount(&mut self, mount: FakeMount) -> &mut Self {
        self.mounts.retain(|m| m.path != mount.path);
        self.mounts.push(mount);
        self
    }

    /// Every mounted filesystem, in mount order.
    pub fn mounts(&self) -> &[FakeMount] {
        &self.mounts
    }

    /// The filesystem `df -k <path>` reports on: the deepest mount holding
    /// `path` after following a link at it. `Err(ENOENT)` when nothing is there.
    pub fn mount_for(&self, path: &str) -> Result<&FakeMount, i32> {
        let resolved = self.resolve(path)?;
        self.mounts
            .iter()
            .filter(|m| m.path == "/" || resolved == m.path || resolved.starts_with(&format!("{}/", m.path)))
            .max_by_key(|m| m.path.len())
            .ok_or(ENOENT)
    }

    /// The node at `path`, if any.
    pub fn get(&self, path: &str) -> Option<&FakeNode> {
        self.nodes.get(&Self::normalize(path))
    }

    /// A file's bytes, if `path` is a file.
    pub fn file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        match self.get(path) {
            Some(FakeNode::File { data, .. }) => Some(data.clone()),
            _ => None,
        }
    }

    /// Every path in the tree, sorted.
    pub fn paths(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// The direct children of `dir`: `(name, node)`.
    pub fn children(&self, dir: &str) -> Vec<(String, FakeNode)> {
        let dir = Self::normalize(dir);
        let prefix = if dir == "/" { "/".to_string() } else { format!("{dir}/") };
        self.nodes
            .iter()
            .filter(|(p, _)| p.starts_with(&prefix) && !p[prefix.len()..].contains('/') && p.len() > prefix.len())
            .map(|(p, n)| (p[prefix.len()..].to_string(), n.clone()))
            .collect()
    }

    /// Stat as the sync service would: `Err(errno)` when missing.
    pub fn stat(&self, path: &str) -> Result<FakeNode, i32> {
        self.get(path).cloned().ok_or(ENOENT)
    }

    /// Creates `path` and every missing ancestor (`mkdir -p`).
    pub fn mkdir_p(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        match self.nodes.get(&path) {
            Some(FakeNode::Dir { .. }) => Ok(()),
            Some(_) => Err(EEXIST),
            None => {
                self.add_dir(&path);
                Ok(())
            }
        }
    }

    /// Writes a file (`SEND`). The parent must exist and be a directory.
    pub fn write_file(&mut self, path: &str, data: Vec<u8>, mode: u32, mtime: i64) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        match Self::parent_of(&path).and_then(|p| self.nodes.get(&p)) {
            Some(FakeNode::Dir { .. }) => {}
            _ => return Err(ENOENT),
        }
        if matches!(self.nodes.get(&path), Some(FakeNode::Dir { .. })) {
            return Err(EISDIR);
        }
        let mode = if mode & 0o170000 == 0 { 0o100000 | mode } else { mode };
        self.nodes.insert(path, FakeNode::File { data, mode, mtime });
        Ok(())
    }

    /// Removes `path` and everything under it (`rm -rf`). `Err(ENOENT)` when
    /// nothing was there.
    pub fn remove_tree(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        if !self.nodes.contains_key(&path) {
            return Err(ENOENT);
        }
        let prefix = format!("{path}/");
        self.nodes.retain(|p, _| p != &path && !p.starts_with(&prefix));
        Ok(())
    }

    /// Removes one node, refusing a directory that still holds something.
    pub fn remove_one(&mut self, path: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let path = Self::normalize(path);
        if !self.nodes.contains_key(&path) {
            return Err(ENOENT);
        }
        if !self.children(&path).is_empty() {
            return Err(ENOTEMPTY_DEVICE);
        }
        self.nodes.remove(&path);
        Ok(())
    }

    /// Renames `from` to `to` (`mv`), overwriting a file at `to`. Moves a
    /// whole subtree.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), i32> {
        if self.read_only {
            return Err(EROFS);
        }
        let from = Self::normalize(from);
        let to = Self::normalize(to);
        if !self.nodes.contains_key(&from) {
            return Err(ENOENT);
        }
        match Self::parent_of(&to).and_then(|p| self.nodes.get(&p)) {
            Some(FakeNode::Dir { .. }) => {}
            _ => return Err(ENOENT),
        }
        if matches!(self.nodes.get(&to), Some(FakeNode::Dir { .. })) && !self.children(&to).is_empty() {
            return Err(ENOTEMPTY_DEVICE);
        }
        let prefix = format!("{from}/");
        let moving: Vec<(String, FakeNode)> = self
            .nodes
            .iter()
            .filter(|(p, _)| *p == &from || p.starts_with(&prefix))
            .map(|(p, n)| (p.clone(), n.clone()))
            .collect();
        for (p, _) in &moving {
            self.nodes.remove(p);
        }
        for (p, n) in moving {
            let new_path = format!("{to}{}", &p[from.len()..]);
            self.nodes.insert(new_path, n);
        }
        Ok(())
    }

    /// `readlink -f`: follows a symlink at `path` (one level; relative targets
    /// resolve against its directory). A non-link answers itself.
    pub fn resolve(&self, path: &str) -> Result<String, i32> {
        let path = Self::normalize(path);
        match self.nodes.get(&path) {
            None => Err(ENOENT),
            Some(FakeNode::Symlink { target, .. }) => {
                if target.starts_with('/') {
                    Ok(Self::normalize(target))
                } else {
                    let parent = Self::parent_of(&path).unwrap_or_else(|| "/".to_string());
                    Ok(Self::normalize(&format!("{parent}/{target}")))
                }
            }
            Some(_) => Ok(path),
        }
    }
}
