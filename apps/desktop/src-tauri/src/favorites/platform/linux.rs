//! The Linux arm: the home folder first, as GNOME Files and Dolphin list it.

use std::path::{Path, PathBuf};

pub(crate) fn default_seed(home: &Path) -> Vec<(PathBuf, &'static str)> {
    vec![
        (home.to_path_buf(), "Home"),
        (home.join("Desktop"), "Desktop"),
        (home.join("Documents"), "Documents"),
        (home.join("Downloads"), "Downloads"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_the_home_folder_first() {
        let seed = default_seed(Path::new("/home/me"));
        assert_eq!(seed[0], (PathBuf::from("/home/me"), "Home"));
        let names: Vec<&str> = seed.iter().map(|(_, name)| *name).collect();
        assert_eq!(names, vec!["Home", "Desktop", "Documents", "Downloads"]);
    }
}
