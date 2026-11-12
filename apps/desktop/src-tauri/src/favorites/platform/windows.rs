//! The Windows arm: the user folder first, then the three known folders under it.

use std::path::{Path, PathBuf};

/// The first row is "User folder", ❌ never "Home": on Windows 11 "Home" is File Explorer's
/// landing page, a different place (`docs/notes/windows-port/copy-decisions.md` § Glossary).
pub(crate) fn default_seed(home: &Path) -> Vec<(PathBuf, &'static str)> {
    vec![
        (home.to_path_buf(), "User folder"),
        (home.join("Desktop"), "Desktop"),
        (home.join("Documents"), "Documents"),
        (home.join("Downloads"), "Downloads"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_user_folder_in_windows_own_words() {
        let seed = default_seed(Path::new(r"C:\Users\me"));
        assert_eq!(seed[0], (PathBuf::from(r"C:\Users\me"), "User folder"));
        let names: Vec<&str> = seed.iter().map(|(_, name)| *name).collect();
        assert_eq!(names, vec!["User folder", "Desktop", "Documents", "Downloads"]);
    }
}
