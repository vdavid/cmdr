//! Filename and path validation utilities.
//!
//! Reusable validation for rename, mkdir, and file transfer operations.
//! Contains platform-aware checks for disallowed characters, name length,
//! and path length limits.

use serde::{Deserialize, Serialize};

/// Maximum file name length in bytes (APFS/HFS+ limit).
pub const MAX_NAME_BYTES: usize = 255;
/// Maximum path length in bytes (macOS PATH_MAX).
pub const MAX_PATH_BYTES: usize = 1024;

/// Validation error types for filename and path checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ValidationError {
    /// Name is empty or whitespace-only after trimming.
    Empty,
    /// Name contains a disallowed character.
    DisallowedCharacter { character: String },
    /// Name exceeds the maximum byte length for a single filename component.
    NameTooLong { bytes: usize, max: usize },
    /// Full path exceeds the maximum byte length.
    PathTooLong { bytes: usize, max: usize },
}

impl std::fmt::Display for ValidationError {
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
/// - No disallowed characters (`/` and `\0` on macOS)
/// - Byte length < 255 (APFS/HFS+ limit)
///
/// The input is checked as-is; callers are responsible for trimming if desired.
// TODO: Add per-OS logic for Windows (backslash, reserved names like CON, NUL, etc.)
pub fn validate_filename(name: &str) -> Result<(), ValidationError> {
    if name.trim().is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check disallowed characters (macOS: / and \0)
    // TODO: Per-OS disallowed character sets
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
    fn rejects_path_at_1024_bytes() {
        let long_path = "/".to_string() + &"a".repeat(1023);
        let result = validate_path_length(Path::new(&long_path));
        assert!(matches!(result, Err(ValidationError::PathTooLong { .. })));
    }

    #[test]
    fn accepts_path_at_1023_bytes() {
        let path = "/".to_string() + &"a".repeat(1022);
        assert!(validate_path_length(Path::new(&path)).is_ok());
    }
}
