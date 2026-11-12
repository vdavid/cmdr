//! Pure type mapping: a PROPFIND entry into `FileEntry`.
use cmdr_fs::entry::FileEntry;

use crate::propfind::PropfindEntry;

/// Builds a [`FileEntry`] from one PROPFIND response.
///
/// ❗ `app_path` is the APP spelling, prefix and all
/// (`webdav://test_user_1@testnas.local:443/Photos/a.jpg`), ❌ never the bare remote path.
/// A pane holds what a listing hands it and passes it straight back, and the app
/// anchors it against the volume root on the way (`root_anchored`), so a bare
/// remote path would come back doubled. `RemoteRoot::to_app_path` is where it is
/// made. WebDAV has no symlinks, so the flag is always off.
pub(super) fn propfind_to_file_entry(name: &str, app_path: &str, prop: &PropfindEntry) -> FileEntry {
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), prop.is_collection, false);
    entry.size = if prop.is_collection { None } else { prop.size };
    entry.modified_at = prop.modified_at;
    entry.created_at = prop.created_at;
    entry
}

/// The last path segment of a decoded `href`, trailing slash ignored.
pub(super) fn href_name(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}

/// Which of a `Depth: 1` answer's decoded `hrefs` is the listed collection
/// itself, where `own` is the path Cmdr asked for (base path included).
///
/// ❗ An exact match on the slash-normalized path wins, ❌ never position: RFC
/// 4918 promises no order. Without one, a proxy has rewritten the links (a
/// Nextcloud served under a path prefix it doesn't know about answers without
/// the prefix), so it falls back to what the rewrite keeps: the self entry is
/// the one every other entry sits directly inside, and it carries the
/// collection's own name. A lone entry is the collection itself (RFC 4918 always
/// includes it), so an empty folder never shows itself either.
pub(super) fn self_entry_index(own: &str, hrefs: &[&str]) -> Option<usize> {
    let own = own.trim_end_matches('/');
    let trimmed: Vec<&str> = hrefs.iter().map(|href| href.trim_end_matches('/')).collect();
    if let Some(exact) = trimmed.iter().position(|href| *href == own) {
        return Some(exact);
    }
    let own_name = href_name(own);
    fn parent_of(href: &str) -> Option<&str> {
        href.rsplit_once('/').map(|(parent, _)| parent)
    }
    let mut candidates = trimmed.iter().enumerate().filter(|(index, candidate)| {
        !own_name.is_empty()
            && href_name(candidate) == own_name
            && trimmed
                .iter()
                .enumerate()
                .all(|(other, href)| other == *index || parent_of(href) == Some(**candidate))
    });
    let found = candidates.next().map(|(index, _)| index);
    // Two candidates means the answer isn't one consistent rewrite: list it all.
    if candidates.next().is_some() { None } else { found }
}

#[cfg(test)]
mod tests {
    use super::self_entry_index;

    const OWN: &str = "/remote.php/dav/files/test_user_1/Photos";

    #[test]
    fn the_exact_path_wins_in_any_position() {
        let hrefs = [
            "/remote.php/dav/files/test_user_1/Photos/a.jpg",
            "/remote.php/dav/files/test_user_1/Photos/",
        ];
        assert_eq!(self_entry_index(OWN, &hrefs), Some(1));
    }

    #[test]
    fn a_child_named_like_its_folder_is_never_the_self_entry() {
        let hrefs = [
            "/remote.php/dav/files/test_user_1/Photos/Photos/",
            "/remote.php/dav/files/test_user_1/Photos/",
        ];
        assert_eq!(self_entry_index(OWN, &hrefs), Some(1));
    }

    #[test]
    fn a_proxy_that_strips_a_path_prefix_still_finds_the_self_entry() {
        // Cmdr asked for `/cloud/remote.php/...`; the Nextcloud behind the proxy
        // answers as if it lived at the root.
        let own = "/cloud/remote.php/dav/files/test_user_1/Photos";
        let hrefs = [
            "/remote.php/dav/files/test_user_1/Photos/a.jpg",
            "/remote.php/dav/files/test_user_1/Photos/",
            "/remote.php/dav/files/test_user_1/Photos/Trips/",
        ];
        assert_eq!(self_entry_index(own, &hrefs), Some(1));
    }

    #[test]
    fn a_proxy_that_adds_a_path_prefix_still_finds_the_self_entry() {
        let hrefs = [
            "/cloud/remote.php/dav/files/test_user_1/Photos/",
            "/cloud/remote.php/dav/files/test_user_1/Photos/Photos/",
        ];
        assert_eq!(self_entry_index(OWN, &hrefs), Some(0));
    }

    #[test]
    fn a_rewritten_empty_folder_hides_its_lone_self_entry() {
        assert_eq!(
            self_entry_index(
                "/cloud/remote.php/dav/files/test_user_1/Photos",
                &["/remote.php/dav/files/test_user_1/Photos/"]
            ),
            Some(0)
        );
    }

    #[test]
    fn a_rewritten_answer_without_a_common_parent_drops_nothing() {
        // The entries don't share one folder, so none of them can be its self
        // entry: listing a stray row beats hiding a real one.
        let hrefs = ["/x/Photos/", "/y/Photos/a.jpg"];
        assert_eq!(self_entry_index("/cloud/Photos", &hrefs), None);
    }

    #[test]
    fn a_lone_child_with_another_name_is_kept() {
        assert_eq!(self_entry_index(OWN, &["/elsewhere/Other/"]), None);
    }
}
