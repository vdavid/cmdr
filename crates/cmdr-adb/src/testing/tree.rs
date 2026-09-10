//! The in-memory filesystem the fake ADB server serves: what a device would
//! have on disk, with no device involved.
//!
//! [`FakeTree::read_only`] is the one fault switch here; every write then
//! answers `EROFS`.

use std::collections::BTreeMap;

use super::pixel_captures::{DF_K_ROOT, DF_K_SHARED_STORAGE, data_row};
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

/// One filesystem mounted in the fake device, as `df -k` reports it. The shell
/// lays its row out per invocation, as toybox does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeMount {
    /// The fake tree's path it holds: `df -k` on it, or on anything under it,
    /// reports this mount.
    pub path: String,
    /// The `Filesystem` column.
    pub device: String,
    /// Size, in 1 KiB blocks.
    pub total_kib: u64,
    /// Used, in 1 KiB blocks. On a real filesystem used and available don't
    /// add up to the size: reserved blocks are neither.
    pub used_kib: u64,
    /// Available to the caller, in 1 KiB blocks.
    pub available_kib: u64,
    /// The `Mounted on` column, which needn't be `path`: the fake's `/sdcard`
    /// is a plain folder, where a phone links it into `/storage/emulated`.
    pub mounted_on: String,
}

impl FakeMount {
    /// A mount holding `path` with the columns of a real `df -k` row, such as
    /// one from [`pixel_captures`](super::pixel_captures). The row's own
    /// spacing isn't kept; the shell lays each table out afresh.
    pub fn with_row(path: &str, row: &str) -> Self {
        let cells: Vec<&str> = row.split_whitespace().collect();
        let [device, total, used, available, _use_percent, mounted_on @ ..] = &cells[..] else {
            panic!("a df -k row has six columns, got {row:?}");
        };
        assert!(
            !mounted_on.is_empty(),
            "a df -k row ends in its mount point, got {row:?}"
        );
        let kib = |cell: &str| {
            cell.parse::<u64>()
                .unwrap_or_else(|_| panic!("{cell:?} in {row:?} is not a block count"))
        };
        Self {
            path: FakeTree::normalize(path),
            device: (*device).to_string(),
            total_kib: kib(total),
            used_kib: kib(used),
            available_kib: kib(available),
            mounted_on: mounted_on.join(" "),
        }
    }

    /// A mount of `total_kib` with `available_kib` free, mounted on `path`.
    pub fn sized(path: &str, device: &str, total_kib: u64, available_kib: u64) -> Self {
        let path = FakeTree::normalize(path);
        Self {
            device: device.to_string(),
            total_kib,
            used_kib: total_kib.saturating_sub(available_kib),
            available_kib,
            mounted_on: path.clone(),
            path,
        }
    }

    /// The `Use%` column as toybox computes it: used over used plus
    /// available, rounded up.
    pub(super) fn use_percent(&self) -> u64 {
        match self.used_kib.saturating_add(self.available_kib) {
            0 => 0,
            denominator => self.used_kib.saturating_mul(100).div_ceil(denominator),
        }
    }
}

impl FakeTree {
    /// A tree holding `/` and `/sdcard`, mounted as the Pixel in
    /// [`pixel_captures`](super::pixel_captures) mounts them: its read-only
    /// system image at `/` (0 available), and its shared storage holding
    /// `/sdcard`, reported on `/storage/emulated`.
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: BTreeMap::new(),
            mounts: Vec::new(),
            read_only: false,
        };
        tree.add_dir("/");
        tree.add_dir("/sdcard");
        tree.mount(FakeMount::with_row("/", data_row(DF_K_ROOT, 0)))
            .mount(FakeMount::with_row("/sdcard", data_row(DF_K_SHARED_STORAGE, 0)));
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

    /// Unmounts everything, for a cell that declares its own mounts. Chainable.
    pub fn unmount_all(&mut self) -> &mut Self {
        self.mounts.clear();
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
