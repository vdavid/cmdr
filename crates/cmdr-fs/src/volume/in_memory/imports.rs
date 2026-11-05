//! Bytes from outside the store: a file or folder a visitor dropped into the
//! browser demo from their own computer (`crates/cmdr-listing-wasm/src/imports.rs`).
//! An imported file holds its real bytes ([`InMemoryContent::Bytes`]) beside the
//! seeded files' synthetic ones, so a read, a copy, a search, and a folder size
//! treat it like any other file.
//!
//! Both doors keep the no-clobber promise `create_file` makes: a taken name is
//! refused, never replaced. Putting it beside a name that's there is the
//! caller's choice (the demo stages each drop in a fresh folder and lets the
//! app's transfer dialog settle collisions).

use std::path::Path;

use super::{InMemoryContent, InMemoryEntry, InMemoryVolume};
use crate::entry::FileEntry;
use crate::ignore_poison::RwLockIgnorePoison;
use crate::volume::VolumeError;

impl InMemoryVolume {
    /// Puts a folder at `path`, dated `modified_at` (seconds; now when `None`).
    ///
    /// Refuses a taken name (`AlreadyExists`) and a parent the store doesn't
    /// hold as a folder (`NotFound`).
    pub async fn import_folder(&self, path: &Path, modified_at: Option<u64>) -> Result<(), VolumeError> {
        let normalized = self.normalize(path);
        {
            let mut entries = self.entries.write_ignore_poison();
            Self::check_import_target(&entries, &normalized)?;
            let metadata = FileEntry {
                modified_at: Some(modified_at.unwrap_or_else(Self::now_secs)),
                created_at: Some(Self::now_secs()),
                permissions: 0o755,
                ..FileEntry::new(
                    Self::name_of(&normalized),
                    normalized.display().to_string(),
                    true,
                    false,
                )
            };
            Self::touch_parent_of(&mut entries, &normalized);
            entries.insert(normalized.clone(), InMemoryEntry::new(metadata, None));
        }
        self.patch_created_at(&normalized).await;
        Ok(())
    }

    /// Puts a file holding `bytes` at `path`, dated `modified_at` (seconds; now
    /// when `None`), stored as real bytes.
    ///
    /// Refuses a taken name (`AlreadyExists`), a parent the store doesn't hold
    /// as a folder (`NotFound`), and bytes the disk has no room for
    /// (`StorageFull`, under [`with_capacity`](Self::with_capacity)), all
    /// before it stores anything.
    pub async fn import_file(&self, path: &Path, bytes: Vec<u8>, modified_at: Option<u64>) -> Result<(), VolumeError> {
        let normalized = self.normalize(path);
        let content = InMemoryContent::bytes(bytes);
        self.check_room_for(content.len())?;
        {
            let mut entries = self.entries.write_ignore_poison();
            Self::check_import_target(&entries, &normalized)?;
            let metadata = FileEntry {
                size: Some(content.len()),
                modified_at: Some(modified_at.unwrap_or_else(Self::now_secs)),
                created_at: Some(Self::now_secs()),
                permissions: 0o644,
                ..FileEntry::new(
                    Self::name_of(&normalized),
                    normalized.display().to_string(),
                    false,
                    false,
                )
            };
            Self::touch_parent_of(&mut entries, &normalized);
            entries.insert(normalized.clone(), InMemoryEntry::new(metadata, Some(content)));
        }
        self.patch_created_at(&normalized).await;
        Ok(())
    }

    /// The refusals both doors share: a taken name, and a parent that isn't a
    /// folder here. The volume root counts as held.
    fn check_import_target(
        entries: &std::collections::HashMap<std::path::PathBuf, InMemoryEntry>,
        normalized: &Path,
    ) -> Result<(), VolumeError> {
        if entries.contains_key(normalized) {
            return Err(VolumeError::AlreadyExists(normalized.display().to_string()));
        }
        let parent = Self::parent_of(normalized);
        let parent_held = parent == Path::new("/") || entries.get(&parent).is_some_and(|e| e.metadata.is_directory);
        if !parent_held {
            return Err(VolumeError::NotFound(parent.display().to_string()));
        }
        Ok(())
    }

    fn name_of(normalized: &Path) -> String {
        normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "imports_test.rs"]
mod tests;
