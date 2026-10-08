//! The chain of destination levels a copied file lands under, and the one
//! question the copy asks of it: is every level a directory in its own right?

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::file_system::write_operations::error_classification::IoResultExt;
use crate::file_system::write_operations::types::WriteOperationError;

/// The topmost entry on the way from `root` down to `dir` (inclusive) that
/// exists and ISN'T a directory in its own right: a file, or a link whatever it
/// points at. `None` when every level that exists is a real directory.
///
/// This is `validation::is_real_directory` for a whole chain. The copy walks
/// FILES and creates their parents as it goes, so "is the parent a directory?"
/// is its merge test, and `Path::is_dir()` follows links: a link standing where
/// an incoming folder lands read as that folder, and the copy wrote into the
/// link's target, a folder the user never picked. Topmost first, because
/// everything below a link lives in its target and proves nothing.
///
/// `root` itself and everything above it are never looked at: that is the
/// folder the person picked, links and all (`/tmp` is one).
///
/// `known_dirs` is the operation's set of directories already proven real or
/// created by it. Each level proven here joins it, so a tree costs one `lstat`
/// per directory; the root is the caller's to add (`copy_single_item` does,
/// once its first file lands there). An `lstat` that can't answer fails the item; ❌ never read it
/// as "nothing there".
pub(in crate::file_system::write_operations::transfer) fn leaf_in_the_way(
    root: &Path,
    dir: &Path,
    known_dirs: &mut HashSet<PathBuf>,
) -> Result<Option<PathBuf>, WriteOperationError> {
    let Ok(below) = dir.strip_prefix(root) else {
        // Can't happen: every destination is `root` joined with a relative path.
        log::warn!(
            "leaf_in_the_way: {} isn't under {}, nothing checked",
            dir.display(),
            root.display()
        );
        return Ok(None);
    };
    let mut level = root.to_path_buf();
    for component in below.components() {
        level.push(component);
        if known_dirs.contains(&level) {
            continue;
        }
        match fs::symlink_metadata(&level) {
            Ok(meta) if meta.is_dir() => {
                known_dirs.insert(level.clone());
            }
            Ok(_) => return Ok(Some(level)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_path(&level),
        }
    }
    Ok(None)
}
