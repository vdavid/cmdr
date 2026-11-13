//! Windows: File Explorer has no bundles, so there's nothing beyond the list to ask (`cmdr-fs`'s
//! Windows arm: `FOLDER_NAMES_MAKE_PACKAGES` is `false`).

/// No package concept beyond the list.
pub(crate) async fn look_up(_candidates: Vec<String>) -> Option<Vec<(String, bool)>> {
    Some(Vec::new())
}

/// No package concept beyond the list.
pub(crate) fn is_package_to_the_os(_path: &str) -> bool {
    false
}
