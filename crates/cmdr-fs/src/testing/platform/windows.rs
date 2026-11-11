//! Windows links need a privilege a plain user only has with Developer Mode, and their
//! kind (file or folder) is fixed at creation.

use std::io;
use std::path::Path;
use std::sync::OnceLock;

/// See the re-export in `mod.rs`.
pub fn symlink<T: AsRef<Path>, L: AsRef<Path>>(target: T, link: L) -> io::Result<()> {
    let (target, link) = (target.as_ref(), link.as_ref());
    let resolved = match link.parent() {
        Some(folder) if target.is_relative() => folder.join(target),
        _ => target.to_path_buf(),
    };
    if resolved.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// An elevated Windows process still meets ACL refusals the way a test means it to, so
/// nothing here needs skipping.
pub fn is_root() -> bool {
    false
}

/// Probed once per process: one dangling file link in a fresh temp folder.
pub fn can_create_symlinks() -> bool {
    static CAN: OnceLock<bool> = OnceLock::new();
    *CAN.get_or_init(|| {
        let Ok(dir) = tempfile::tempdir() else { return false };
        std::os::windows::fs::symlink_file("missing", dir.path().join("probe")).is_ok()
    })
}
