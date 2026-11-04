//! `InMemoryVolume`'s symlinks: the knobs that turn them on, and how one is stored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{InMemoryEntry, InMemoryVolume};
use crate::entry::FileEntry;
use crate::ignore_poison::RwLockIgnorePoison;

impl InMemoryVolume {
    /// Makes this volume hold symlinks: [`Volume::can_hold_links`](crate::volume::Volume::can_hold_links)
    /// says so and [`Volume::create_symlink`](crate::volume::Volume::create_symlink) makes them.
    pub fn with_links(mut self) -> Self {
        self.holds_links = true;
        self
    }

    /// Claims links through `can_hold_links` but refuses every `create_symlink`:
    /// a link-capable backend on a filesystem that has none.
    pub fn with_links_refused(mut self) -> Self {
        self.holds_links = true;
        self.refuses_links = true;
        self
    }

    /// Stores a symlink at `path` directly, for a source a test reads from. The
    /// `create_symlink` route needs [`Self::with_links`].
    pub fn plant_symlink(&self, path: &Path, target: &Path) {
        let mut entries = self.entries.write_ignore_poison();
        Self::insert_link(&mut entries, self.normalize(path), target);
    }

    /// A symlink row holding `target`, stored as given: a leaf with no bytes,
    /// reported as the link whatever it points at.
    pub(super) fn insert_link(entries: &mut HashMap<PathBuf, InMemoryEntry>, normalized: PathBuf, target: &Path) {
        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let metadata = FileEntry {
            size: Some(0),
            modified_at: Some(Self::now_secs()),
            ..FileEntry::new(name, normalized.display().to_string(), false, true)
        };
        Self::touch_parent_of(entries, &normalized);
        let mut entry = InMemoryEntry::new(metadata, Some(super::InMemoryContent::bytes(Vec::new())));
        entry.link_target = Some(target.to_path_buf());
        entries.insert(normalized, entry);
    }
}
