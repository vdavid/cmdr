//! Path arithmetic over the index-space paths this subsystem stores and counts.
//!
//! A leaf: nothing here reads a cache, a DB, or a config, so every part of the
//! subsystem (the walk, the writer, both coverage caches) can call it without
//! importing the module that happens to walk the index.

/// The parent directory of an absolute path (the folder importance keys on). `"/"`
/// for a top-level file. A pure slice, no allocation.
pub(crate) fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

#[cfg(test)]
mod tests {
    use super::parent_dir;

    #[test]
    fn parent_dir_slices_the_folder_off_an_absolute_path() {
        assert_eq!(parent_dir("/photos/trip/a.jpg"), "/photos/trip");
        assert_eq!(parent_dir("/a.jpg"), "/", "a top-level file's parent is the root");
        assert_eq!(
            parent_dir("relative.jpg"),
            "/",
            "a path with no separator answers the root"
        );
    }
}
