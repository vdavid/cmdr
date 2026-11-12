//! The macOS arm: Finder's sidebar four.

use std::path::{Path, PathBuf};

pub(crate) fn default_seed(home: &Path) -> Vec<(PathBuf, &'static str)> {
    vec![
        (PathBuf::from("/Applications"), "Applications"),
        (home.join("Desktop"), "Desktop"),
        (home.join("Documents"), "Documents"),
        (home.join("Downloads"), "Downloads"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_the_finder_sidebar_four() {
        let names: Vec<&str> = default_seed(Path::new("/Users/me"))
            .iter()
            .map(|(_, name)| *name)
            .collect();
        assert_eq!(names, vec!["Applications", "Desktop", "Documents", "Downloads"]);
    }
}
