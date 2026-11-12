//! A leading `~` in a path a person typed, expanded to their home directory.
//!
//! Only for a path on THIS machine's filesystem: on a share or a phone, `~` is an
//! ordinary folder name, so every caller decides first whether the path is local.
//!
//! On Windows the result is an app path (`C:/Users/test_user_1/Documents`, the path grammar's
//! spelling), never the home folder's native `C:\Users\test_user_1` spliced onto a `/` rest (#1109),
//! and `~\Documents` expands too. On macOS and Linux it's a plain splice, as it always was.

use crate::volume::path_grammar::{AppPath, PathFlavor};

/// Expands a leading `~` (`~` alone or `~/…`, plus `~\…` on Windows) to the user's home
/// directory. Any other path, and any path when the home directory can't be found, comes back
/// unchanged.
pub fn expand_tilde(path: &str) -> String {
    let home = dirs::home_dir();
    expand_tilde_with(
        path,
        home.as_deref().map(|h| h.to_string_lossy()).as_deref(),
        PathFlavor::HOST,
    )
}

/// [`expand_tilde`] against an explicit `home` (as the OS reports it) and `flavor`, so every
/// flavor is tested on every OS.
pub fn expand_tilde_with(path: &str, home: Option<&str>, flavor: PathFlavor) -> String {
    let Some(home) = home else {
        return path.to_string();
    };
    match flavor {
        PathFlavor::Posix => {
            if path.starts_with("~/") || path == "~" {
                return path.replacen('~', home, 1);
            }
            path.to_string()
        }
        PathFlavor::Windows => {
            let rest = match path.strip_prefix('~') {
                Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => rest,
                _ => return path.to_string(),
            };
            let Some(home) = AppPath::from_native(home, flavor) else {
                return path.to_string();
            };
            home.join(&rest.replace('\\', "/")).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_posix_tilde_splices_the_home_folder_in() {
        let home = Some("/Users/test_user_1");
        assert_eq!(
            expand_tilde_with("~/Documents", home, PathFlavor::Posix),
            "/Users/test_user_1/Documents"
        );
        assert_eq!(expand_tilde_with("~", home, PathFlavor::Posix), "/Users/test_user_1");
        assert_eq!(expand_tilde_with("~/", home, PathFlavor::Posix), "/Users/test_user_1/");
    }

    #[test]
    fn a_posix_path_without_a_leading_tilde_comes_back_unchanged() {
        let home = Some("/Users/test_user_1");
        for path in ["/usr/local/bin", "~test_user_1/x", "a/~/b", "~\\Documents"] {
            assert_eq!(expand_tilde_with(path, home, PathFlavor::Posix), path);
        }
    }

    /// The home folder is native (`C:\Users\test_user_1`); the result is an app path (#1109).
    #[test]
    fn a_windows_tilde_expands_to_an_app_path() {
        let home = Some(r"C:\Users\test_user_1");
        assert_eq!(
            expand_tilde_with("~/Documents", home, PathFlavor::Windows),
            "C:/Users/test_user_1/Documents"
        );
        assert_eq!(
            expand_tilde_with(r"~\Documents\Work", home, PathFlavor::Windows),
            "C:/Users/test_user_1/Documents/Work"
        );
        assert_eq!(
            expand_tilde_with("~", home, PathFlavor::Windows),
            "C:/Users/test_user_1"
        );
    }

    #[test]
    fn a_windows_home_on_a_share_or_a_lowercase_drive_lands_on_its_root() {
        assert_eq!(
            expand_tilde_with("~/x", Some(r"\\nas\homes\test_user_1"), PathFlavor::Windows),
            "//nas/homes/test_user_1/x"
        );
        assert_eq!(
            expand_tilde_with("~/x", Some(r"d:\Users\test_user_1"), PathFlavor::Windows),
            "D:/Users/test_user_1/x"
        );
    }

    #[test]
    fn a_windows_path_without_a_leading_tilde_comes_back_unchanged() {
        let home = Some(r"C:\Users\test_user_1");
        for path in [r"C:\Windows", "~test_user_1", "x/~"] {
            assert_eq!(expand_tilde_with(path, home, PathFlavor::Windows), path);
        }
    }

    #[test]
    fn no_home_folder_leaves_the_path_alone() {
        assert_eq!(expand_tilde_with("~/x", None, PathFlavor::Posix), "~/x");
        assert_eq!(expand_tilde_with("~/x", None, PathFlavor::Windows), "~/x");
        assert_eq!(expand_tilde_with("~/x", Some("relative"), PathFlavor::Windows), "~/x");
    }

    /// The host's own expansion leaves no `~` behind and lands on an app path.
    #[test]
    fn the_host_expansion_is_an_app_path() {
        let expanded = expand_tilde("~/Documents");
        assert!(!expanded.contains('~'), "{expanded}");
        assert!(
            crate::volume::path_grammar::is_local_path(&expanded, PathFlavor::HOST),
            "{expanded}"
        );
        assert!(expanded.ends_with("/Documents"), "{expanded}");
    }
}
