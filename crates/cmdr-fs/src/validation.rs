//! Filename and path validation utilities.
//!
//! Reusable validation for rename, mkdir, and file transfer operations.
//! Contains platform-aware checks for disallowed characters, name length,
//! and path length limits.

use serde::{Deserialize, Serialize};

/// Maximum file name length in bytes (APFS/HFS+/ext4 limit).
pub const MAX_NAME_BYTES: usize = 255;

/// Maximum path length in bytes.
/// macOS PATH_MAX is 1024; Linux PATH_MAX is 4096.
#[cfg(target_os = "linux")]
pub const MAX_PATH_BYTES: usize = 4096;
/// Maximum path length in bytes (macOS `PATH_MAX`, which the browser build borrows).
#[cfg(any(target_os = "macos", target_family = "wasm"))]
pub const MAX_PATH_BYTES: usize = 1024;

/// Validation error types for filename and path checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum ValidationError {
    /// Name is empty or whitespace-only after trimming.
    Empty,
    /// Name contains a disallowed character.
    #[allow(
        missing_docs,
        reason = "a field doc would change `bindings.ts`; the variant doc says what it holds"
    )]
    DisallowedCharacter { character: String },
    /// Name exceeds the maximum byte length for a single filename component.
    #[allow(
        missing_docs,
        reason = "a field doc would change `bindings.ts`; the variant doc says what it holds"
    )]
    NameTooLong { bytes: usize, max: usize },
    /// Full path exceeds the maximum byte length.
    #[allow(
        missing_docs,
        reason = "a field doc would change `bindings.ts`; the variant doc says what it holds"
    )]
    PathTooLong { bytes: usize, max: usize },
}

impl std::fmt::Display for ValidationError {
    /// ❗ For logs and debugging only: the frontend words each variant from its own catalog.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Name can't be empty"),
            Self::DisallowedCharacter { character } => {
                write!(f, "Name contains a disallowed character: {}", character)
            }
            Self::NameTooLong { bytes, max } => {
                write!(f, "Name is {} bytes, which exceeds the {} byte limit", bytes, max)
            }
            Self::PathTooLong { bytes, max } => {
                write!(f, "Path is {} bytes, which exceeds the {} byte limit", bytes, max)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validates a filename for use on the current platform.
///
/// Checks performed (on trimmed input):
/// - Not empty / whitespace-only
/// - No disallowed characters (`/` and `\0` on macOS and Linux)
/// - Byte length < 255 (APFS/HFS+/ext4 limit)
///
/// The input is checked as-is; callers are responsible for trimming if desired.
// TODO: Add per-OS logic for Windows (backslash, reserved names like CON, NUL, etc.)
pub fn validate_filename(name: &str) -> Result<(), ValidationError> {
    if name.trim().is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check disallowed characters (macOS + Linux: / and \0)
    // TODO: Per-OS disallowed character sets for Windows
    for ch in name.chars() {
        if ch == '/' {
            return Err(ValidationError::DisallowedCharacter {
                character: "/".to_string(),
            });
        }
        if ch == '\0' {
            return Err(ValidationError::DisallowedCharacter {
                character: "NUL".to_string(),
            });
        }
    }

    // Check byte length
    let byte_len = name.len();
    if byte_len >= MAX_NAME_BYTES {
        return Err(ValidationError::NameTooLong {
            bytes: byte_len,
            max: MAX_NAME_BYTES,
        });
    }

    Ok(())
}

/// Why a name isn't one plain, visible leaf, for [`validate_plain_leaf`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainLeafRefusal {
    /// Empty, or only whitespace.
    Empty,
    /// Starts or ends with whitespace, which reads exactly like the name without it.
    EdgeWhitespace,
    /// Holds a `/`, so it's a path, not one name.
    Separator,
    /// `.` or `..`, which name a folder, not an entry in one.
    DotName,
    /// Longer than [`MAX_NAME_BYTES`].
    TooLong {
        /// The name's length in bytes.
        bytes: usize,
    },
    /// Holds a control character (`char::is_control`, NUL included).
    Control(char),
    /// Holds an invisible formatting character: a bidi embedding, override,
    /// isolate, or mark, a zero-width character, a byte-order mark, or a soft
    /// hyphen.
    Invisible(char),
}

/// Checks that `name` is one plain, visible leaf: a name a reviewer reads as
/// exactly what lands.
///
/// Stricter than [`validate_filename`] on purpose: it's for a name somebody
/// else chose for the user (a per-item transfer name, a proposal), where a name
/// that renders as something else is a spoof the review can't show:
/// `invoice`, a right-to-left override, then `fdp.exe` reads as
/// `invoiceexe.pdf`. Refuses an empty name, edge whitespace, `/`, `.` and
/// `..`, more than [`MAX_NAME_BYTES`] bytes, any control character, and the
/// invisible format characters [`is_invisible_format`] lists.
pub fn validate_plain_leaf(name: &str) -> Result<(), PlainLeafRefusal> {
    if name.trim().is_empty() {
        return Err(PlainLeafRefusal::Empty);
    }
    if name.starts_with(char::is_whitespace) || name.ends_with(char::is_whitespace) {
        return Err(PlainLeafRefusal::EdgeWhitespace);
    }
    if name.contains('/') {
        return Err(PlainLeafRefusal::Separator);
    }
    if name == "." || name == ".." {
        return Err(PlainLeafRefusal::DotName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(PlainLeafRefusal::TooLong { bytes: name.len() });
    }
    for ch in name.chars() {
        if ch.is_control() {
            return Err(PlainLeafRefusal::Control(ch));
        }
        if is_invisible_format(ch) {
            return Err(PlainLeafRefusal::Invisible(ch));
        }
    }
    Ok(())
}

/// The invisible format characters a plain leaf refuses: they change how a
/// name renders, or hide inside it, without showing themselves. The bidi
/// embeddings and overrides (U+202A–U+202E), the isolates (U+2066–U+2069), the
/// marks (U+200E, U+200F, U+061C), the zero-width characters (U+200B–U+200D,
/// U+2060), the byte-order mark (U+FEFF), and the soft hyphen (U+00AD). An
/// explicit list rather than all of category `Cf`, so a legitimate name loses
/// nothing it could show.
pub fn is_invisible_format(ch: char) -> bool {
    matches!(
        ch,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{061C}'
            | '\u{200B}'..='\u{200D}'
            | '\u{2060}'
            | '\u{FEFF}'
            | '\u{00AD}'
    )
}

/// Validates that a full path doesn't exceed the filesystem path length limit.
pub fn validate_path_length(path: &std::path::Path) -> Result<(), ValidationError> {
    let byte_len = path.as_os_str().len();
    if byte_len >= MAX_PATH_BYTES {
        return Err(ValidationError::PathTooLong {
            bytes: byte_len,
            max: MAX_PATH_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_plain_leaf_test.rs"]
mod plain_leaf_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ========================================================================
    // validate_filename
    // ========================================================================

    #[test]
    fn valid_simple_name() {
        assert!(validate_filename("document.txt").is_ok());
    }

    #[test]
    fn valid_name_with_spaces() {
        assert!(validate_filename("my document.txt").is_ok());
    }

    #[test]
    fn valid_dotfile() {
        assert!(validate_filename(".gitignore").is_ok());
    }

    #[test]
    fn valid_unicode_name() {
        assert!(validate_filename("日本語ファイル.txt").is_ok());
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(validate_filename(""), Err(ValidationError::Empty));
    }

    #[test]
    fn rejects_whitespace_only() {
        assert_eq!(validate_filename("   "), Err(ValidationError::Empty));
    }

    #[test]
    fn rejects_slash() {
        assert_eq!(
            validate_filename("foo/bar"),
            Err(ValidationError::DisallowedCharacter {
                character: "/".to_string()
            })
        );
    }

    #[test]
    fn rejects_null_byte() {
        assert_eq!(
            validate_filename("foo\0bar"),
            Err(ValidationError::DisallowedCharacter {
                character: "NUL".to_string()
            })
        );
    }

    #[test]
    fn rejects_name_at_255_bytes() {
        let long_name = "a".repeat(255);
        let result = validate_filename(&long_name);
        assert!(matches!(result, Err(ValidationError::NameTooLong { .. })));
    }

    #[test]
    fn accepts_name_at_254_bytes() {
        let name = "a".repeat(254);
        assert!(validate_filename(&name).is_ok());
    }

    // ========================================================================
    // validate_path_length
    // ========================================================================

    #[test]
    fn valid_short_path() {
        assert!(validate_path_length(Path::new("/Users/test/file.txt")).is_ok());
    }

    #[test]
    fn rejects_path_at_max_bytes() {
        let long_path = "/".to_string() + &"a".repeat(MAX_PATH_BYTES - 1);
        let result = validate_path_length(Path::new(&long_path));
        assert!(matches!(result, Err(ValidationError::PathTooLong { .. })));
    }

    #[test]
    fn accepts_path_just_under_max() {
        let path = "/".to_string() + &"a".repeat(MAX_PATH_BYTES - 2);
        assert!(validate_path_length(Path::new(&path)).is_ok());
    }
}
