//! The jail: the one function both memory tools call before either touches the disk.
//!
//! The agent picks the path, and the agent's idea of a good path can come from text it read —
//! a file name, or a sentence photographed in one of the user's images. So this is the place
//! that assumes the caller is hostile, and every rule here has its own test in `tests.rs`.

use std::path::{Component, Path, PathBuf};

use super::store::MemoryRefusal;

/// The only file extension memory holds. Markdown is what the hub is fed to the model as, and
/// a folder that can hold a `.sh` or a `.json` is a folder the agent can drop something
/// executable or config-shaped into.
pub const MEMORY_EXTENSION: &str = "md";

/// Where `requested` lands inside `root`, or the typed reason it may not land anywhere.
///
/// Four checks, in an order that matters: the lexical shape first (cheap, and it rejects the
/// two escapes that need no filesystem at all), then the extension, then symlinks along the
/// whole chain, and finally a `canonicalize` of the PARENT.
///
/// ⚠️ **The parent, never the file.** `canonicalize` fails on a path that doesn't exist yet,
/// so canonicalizing the target would refuse every first write. The parent is created (inside
/// the jail, only after the symlink walk clears it), canonicalized, and re-checked for
/// containment; the file name is a single validated `Normal` component joined onto it.
pub(super) fn resolve(root: &Path, requested: &str) -> Result<PathBuf, MemoryRefusal> {
    let relative = lexically_inside(requested)?;
    let Some(extension) = relative.extension() else {
        return Err(MemoryRefusal::NotMarkdown);
    };
    if !extension.eq_ignore_ascii_case(MEMORY_EXTENSION) {
        return Err(MemoryRefusal::NotMarkdown);
    }

    // A link anywhere along the chain, including the file itself, is an escape a lexical check
    // cannot see: `link.md` is a perfectly clean relative path.
    let mut walked = root.to_path_buf();
    for component in relative.components() {
        walked.push(component);
        if std::fs::symlink_metadata(&walked).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(MemoryRefusal::OutsideMemory);
        }
    }

    let parent = walked.parent().unwrap_or(root).to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|e| MemoryRefusal::Unwritable(e.to_string()))?;

    // Belt to the symlink walk's braces: whatever the components said, the parent that now
    // exists on disk has to sit under the root that now exists on disk.
    let (Ok(canonical_root), Ok(canonical_parent)) = (root.canonicalize(), parent.canonicalize()) else {
        return Err(MemoryRefusal::OutsideMemory);
    };
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(MemoryRefusal::OutsideMemory);
    }

    let name = relative.file_name().ok_or(MemoryRefusal::NoPath)?;
    Ok(canonical_parent.join(name))
}

/// The lexical half: a non-empty relative path made only of ordinary components.
///
/// Rejects an absolute path, a Windows prefix, a `.` or `..` component, and a path that
/// carries no real name at all. This is what catches `../../../etc/passwd.md` and
/// `/tmp/x.md` before any syscall runs.
fn lexically_inside(requested: &str) -> Result<PathBuf, MemoryRefusal> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(MemoryRefusal::NoPath);
    }
    let candidate = Path::new(trimmed);
    let mut relative = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(MemoryRefusal::OutsideMemory);
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(MemoryRefusal::NoPath);
    }
    Ok(relative)
}
