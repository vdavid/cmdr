//! "Go to path" backend: pure path resolution plus the recents store.
//!
//! The dialog lets the user type a path (absolute, `~`-rooted, or relative to
//! the focused pane) and jump to it. All the path reasoning lives here so the
//! frontend stays a thin presenter (AGENTS.md principle 3, "smart backend, thin
//! frontend"). The IPC wrappers in `commands/go_to_path.rs` are pass-throughs.
//!
//! ## What `resolve` does
//!
//! 1. Expands a leading `~` via `crate::commands::file_system::expand_tilde`.
//! 2. Joins a relative `input` against `base_dir` (the focused pane's path).
//! 3. **Lexically** normalizes `.` / `..` components (see the no-canonicalize
//!    decision below) without touching the disk.
//! 4. Classifies the lexical path against the **local** filesystem into
//!    [`GoToPathResolution`].
//!
//! `resolve` is a pure function (no `AppHandle`) so it's unit-testable over a
//! `tempfile::tempdir()`. The async command wraps it in `blocking_with_timeout`.
//!
//! ## Decision: backend owns resolution
//!
//! A single resolution path serves three callers (live as-you-type warning, the
//! actual jump, the clipboard-prefill decision), so the preview and the action
//! can never drift. The frontend switches on the returned enum variant and acts.
//!
//! ## Decision: no `canonicalize()`
//!
//! We normalize `.`/`..` lexically, not via `Path::canonicalize`. `canonicalize`
//! requires the *whole* path to exist (it errors otherwise), which would break
//! the nearest-ancestor case where the tail doesn't exist. It also resolves
//! symlinks, silently rewriting the path we show and navigate to into something
//! the user didn't type. Lexical normalization keeps the displayed path faithful
//! to the input and lets nearest-ancestor work. `metadata()` (which *does* follow
//! symlinks) only classifies the existing target as file vs. directory, so a
//! symlinked directory navigates into the symlink path and the listing follows
//! it - correct and intended.
//!
//! ## v1 limitations
//!
//! - **Relative paths on a non-local pane.** `base_dir` is the focused pane's
//!   path; if that pane is on MTP/SMB, a relative input resolves against a
//!   non-local base and the local-fs walk falls back to nearest-ancestor (often
//!   `/`). Absolute and `~` paths always work. Accepted degraded behavior.
//! - **Case-insensitive dedupe.** The recents store (see [`history`]) dedupes by
//!   a raw resolved-path string compare, so on case-insensitive APFS
//!   `/Users/x/Foo` and `/Users/x/foo` show as two entries. Worst case: a
//!   duplicate-looking row. We don't `canonicalize()` (reasons above) to fix it.

pub mod history;

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::file_system::expand_tilde;

/// The outcome of resolving a typed "go to path" input against the local
/// filesystem. The variant is the contract the frontend branches on; never
/// classify by string-matching a message (AGENTS.md § no-error-string-match).
///
/// `rename_all_fields = "camelCase"` is REQUIRED: without it, struct-variant
/// fields ship snake_case through tauri-specta and read `undefined` on the TS
/// side. Enforced by the `ipc-enum-camelcase` check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GoToPathResolution {
    /// The resolved path is an existing directory. Navigate into it.
    Directory { path: String },
    /// The resolved path is an existing file. Navigate to its parent and select it.
    File { parent_dir: String, file_name: String },
    /// The resolved path doesn't exist. `ancestor_dir` is the nearest existing
    /// ancestor (worst case `/`). Navigate there and fire an INFO toast.
    NearestAncestor { requested: String, ancestor_dir: String },
    /// Defensive: the input was empty or couldn't be turned into a path.
    Invalid { reason: String },
}

/// Lexically normalizes `.` and `..` components without touching the disk.
///
/// `.` is dropped; `..` pops the previous normal component, or is kept verbatim
/// when there's nothing to pop (a leading `..` on a relative path, or a `..`
/// right after the root). The root prefix (`/`) is preserved.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop only if the last pushed component is a normal name. A
                // leading `..`, or `..` after the root, has nothing to pop and
                // is kept so the path stays faithful to the input.
                let popped = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if popped {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Walks up from `path` to the closest existing ancestor. Worst case returns
/// `/` (which always exists on the platforms we target). Returns `path` itself
/// when it exists.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return current.to_path_buf(),
        }
    }
}

/// Resolves a typed input against `base_dir` into a [`GoToPathResolution`].
///
/// Pure: no `AppHandle`, no IPC. The disk is only read via `exists` / `metadata`
/// for classification; the async command wraps this in `blocking_with_timeout`
/// so a hung mount can't freeze IPC.
pub fn resolve(input: &str, base_dir: &str) -> GoToPathResolution {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return GoToPathResolution::Invalid {
            reason: "Type a path to go to.".to_string(),
        };
    }

    // Expand `~` first, then join relative inputs against the focused pane's dir.
    let expanded = expand_tilde(trimmed);
    let expanded_path = Path::new(&expanded);
    let joined = if expanded_path.is_absolute() {
        expanded_path.to_path_buf()
    } else {
        Path::new(base_dir).join(expanded_path)
    };

    let normalized = lexical_normalize(&joined);
    let normalized_str = normalized.to_string_lossy().to_string();

    // `metadata` follows symlinks, so a symlinked dir/file classifies as its target.
    match std::fs::metadata(&normalized) {
        Ok(meta) if meta.is_dir() => GoToPathResolution::Directory { path: normalized_str },
        Ok(_) => {
            // Existing non-directory (file, symlink-to-file, etc.). Navigate to
            // the parent and select the entry. A path that exists always has a
            // parent except for the root, which `is_dir()` already caught above.
            let parent_dir = normalized
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string());
            let file_name = normalized
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| normalized_str.clone());
            GoToPathResolution::File { parent_dir, file_name }
        }
        Err(_) => {
            // Doesn't exist (or unreadable): fall back to the nearest ancestor.
            let ancestor = nearest_existing_ancestor(&normalized);
            GoToPathResolution::NearestAncestor {
                requested: normalized_str,
                ancestor_dir: ancestor.to_string_lossy().to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn existing_dir_resolves_to_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("create sub");

        let res = resolve(&sub.to_string_lossy(), dir.path().to_str().unwrap());
        assert_eq!(
            res,
            GoToPathResolution::Directory {
                path: sub.to_string_lossy().to_string()
            }
        );
    }

    #[test]
    fn existing_file_resolves_to_file_with_parent_and_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("notes.txt");
        fs::write(&file, b"hi").expect("write file");

        let res = resolve(&file.to_string_lossy(), dir.path().to_str().unwrap());
        assert_eq!(
            res,
            GoToPathResolution::File {
                parent_dir: dir.path().to_string_lossy().to_string(),
                file_name: "notes.txt".to_string(),
            }
        );
    }

    #[test]
    fn tilde_expands_to_home() {
        // `~` alone expands to the home dir, which exists, so it resolves to a Directory.
        let home = dirs::home_dir().expect("home dir");
        let res = resolve("~", "/tmp");
        assert_eq!(
            res,
            GoToPathResolution::Directory {
                path: home.to_string_lossy().to_string()
            }
        );
    }

    #[test]
    fn relative_input_joins_against_base_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sub = dir.path().join("child");
        fs::create_dir(&sub).expect("create child");

        let res = resolve("child", dir.path().to_str().unwrap());
        assert_eq!(
            res,
            GoToPathResolution::Directory {
                path: sub.to_string_lossy().to_string()
            }
        );
    }

    #[test]
    fn dot_and_dotdot_are_lexically_normalized() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a");
        let b = a.join("b");
        fs::create_dir_all(&b).expect("create a/b");

        // From base `a/b`, `./../.` should land back on `a`.
        let res = resolve("./../.", b.to_str().unwrap());
        assert_eq!(
            res,
            GoToPathResolution::Directory {
                path: a.to_string_lossy().to_string()
            }
        );
    }

    #[test]
    fn dotdot_past_a_nonexistent_middle_segment_normalizes_lexically() {
        let dir = tempfile::tempdir().expect("tempdir");
        // `<dir>/nope/../` lexically normalizes to `<dir>`, which exists, even
        // though `nope` never did. Canonicalize would have errored here; lexical
        // normalization is exactly why we don't use it.
        let input = dir.path().join("nope").join("..");
        let res = resolve(&input.to_string_lossy(), "/tmp");
        assert_eq!(
            res,
            GoToPathResolution::Directory {
                path: dir.path().to_string_lossy().to_string()
            }
        );
    }

    #[test]
    fn deep_nonexistent_path_falls_back_to_nearest_ancestor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope").join("a.txt");
        let res = resolve(&missing.to_string_lossy(), "/tmp");
        assert_eq!(
            res,
            GoToPathResolution::NearestAncestor {
                requested: missing.to_string_lossy().to_string(),
                ancestor_dir: dir.path().to_string_lossy().to_string(),
            }
        );
    }

    #[test]
    fn totally_nonexistent_absolute_path_falls_back_to_root() {
        let res = resolve("/totally-nonexistent-xyz-123", "/tmp");
        assert_eq!(
            res,
            GoToPathResolution::NearestAncestor {
                requested: "/totally-nonexistent-xyz-123".to_string(),
                ancestor_dir: "/".to_string(),
            }
        );
    }

    #[test]
    fn empty_input_is_invalid() {
        let res = resolve("", "/tmp");
        assert!(matches!(res, GoToPathResolution::Invalid { .. }));
    }

    #[test]
    fn whitespace_only_input_is_invalid() {
        let res = resolve("   ", "/tmp");
        assert!(matches!(res, GoToPathResolution::Invalid { .. }));
    }

    #[test]
    fn resolution_serializes_with_camelcase_kind_and_fields() {
        let res = GoToPathResolution::File {
            parent_dir: "/Users/x".to_string(),
            file_name: "a.txt".to_string(),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"kind\":\"file\""), "got {json}");
        assert!(json.contains("\"parentDir\""), "got {json}");
        assert!(json.contains("\"fileName\""), "got {json}");

        let ancestor = GoToPathResolution::NearestAncestor {
            requested: "/tmp/nope".to_string(),
            ancestor_dir: "/tmp".to_string(),
        };
        let json = serde_json::to_string(&ancestor).unwrap();
        assert!(json.contains("\"kind\":\"nearestAncestor\""), "got {json}");
        assert!(json.contains("\"ancestorDir\""), "got {json}");
    }
}
