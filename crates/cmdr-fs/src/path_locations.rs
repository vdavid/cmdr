//! How many PLACES a set of directories amounts to.
//!
//! Both surfaces that report ground Cmdr couldn't read — the search coverage note
//! and the per-drive index badge — have to answer "how much of the drive is this
//! about?", and the honest unit is a place, not a folder. A mount that stops
//! answering marks every directory a walk had reached inside it: 1,497 of them on
//! one real machine, which the coverage descent already cuts to 76 shallowest
//! ancestors. Telling somebody "1,497 folders" is technically true and tells them
//! nothing they can picture or act on; "one place" is what they'd recognize.
//!
//! It lives in the shared vocabulary crate because the two callers are in
//! different crates and the rule has to be ONE rule: two copies would drift, and
//! the two surfaces would then disagree about the same drive.

/// How many places `paths` amounts to: directories that share a parent are one.
///
/// The input is expected to be the shallowest marked ancestors (what a coverage
/// answer already cuts to), so grouping by parent is the one step between that
/// and a number a person can picture. The root's own parent is the root, which
/// keeps a marked `/Volumes` and a marked `/opt` two places rather than folding
/// them into one.
#[must_use]
pub fn location_count(paths: &[String]) -> u32 {
    let mut parents: Vec<&str> = paths.iter().map(|path| parent_of(path)).collect();
    parents.sort_unstable();
    parents.dedup();
    parents.len() as u32
}

/// The parent of an absolute path, with a trailing separator ignored. A path
/// directly under the root reports the root.
fn parent_of(path: &str) -> &str {
    let trimmed = if path.len() > 1 {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    match trimmed.rfind('/') {
        Some(0) | None => "/",
        Some(cut) => &trimmed[..cut],
    }
}

#[cfg(test)]
mod tests {
    use super::location_count;

    fn count(paths: &[&str]) -> u32 {
        location_count(&paths.iter().map(|p| (*p).to_string()).collect::<Vec<_>>())
    }

    /// The case this exists for. A drive that went to sleep leaves a long list of
    /// directories that are all the same place to the person reading about it.
    #[test]
    fn folders_under_one_parent_are_one_place() {
        assert_eq!(
            count(&[
                "/Volumes/nas/photos/2019",
                "/Volumes/nas/photos/2020",
                "/Volumes/nas/photos/2021",
            ]),
            1
        );
    }

    #[test]
    fn folders_in_genuinely_different_places_stay_apart() {
        assert_eq!(count(&["/Volumes/nas/photos/2019", "/opt/homebrew/cellar"]), 2);
    }

    #[test]
    fn a_trailing_separator_is_the_same_folder() {
        assert_eq!(count(&["/Volumes/nas/photos/2019/", "/Volumes/nas/photos/2019"]), 1);
    }

    /// Two folders directly under the root share it as a parent; two folders one
    /// level deeper don't.
    #[test]
    fn the_root_is_a_parent_like_any_other() {
        assert_eq!(count(&["/Volumes", "/opt"]), 1);
        assert_eq!(count(&["/Volumes/nas", "/opt/tools"]), 2);
    }

    #[test]
    fn nothing_unread_is_no_places() {
        assert_eq!(count(&[]), 0);
    }
}
