//! The Dock's `persistent-apps` array as plain data: reading an app out of a tile, deciding
//! whether an app is already there, and building the tile we'd add.
//!
//! Everything here is a pure function over `plist::Value`, so the interesting decisions are
//! testable without a preferences domain anywhere near them. The CFPreferences boundary that turns
//! the real domain into these values is `prefs.rs`.

use std::path::{Path, PathBuf};

/// The Dock preference holding the pinned application tiles, left to right. Finder's tile isn't in
/// it (the Dock draws that one itself), so index 0 is the leftmost slot we can reach.
pub(super) const PERSISTENT_APPS_KEY: &str = "persistent-apps";

/// What `tile-type` an application tile carries.
const FILE_TILE: &str = "file-tile";

/// The `file-type` every application tile in a live Dock carries.
const APP_FILE_TYPE: i64 = 41;

/// `_CFURLStringType` 0: `_CFURLString` is a plain absolute POSIX path, not a URL.
const POSIX_PATH_URL_TYPE: i64 = 0;

/// `_CFURLStringType` 15: `_CFURLString` is an absolute URL string, so its path is
/// percent-encoded.
const ABSOLUTE_URL_TYPE: i64 = 15;

/// Builds the application tile for the bundle at `bundle_path`.
///
/// `guid` is the tile's identity within the Dock; the caller draws a fresh random one so two tiles
/// never collide. `bundle_id` is written when we know it, which lets [`holds_app`] recognize our
/// own tile by its strongest key before the Dock rewrites the entry.
///
/// The POSIX-path form of `file-data` is deliberate: it needs no percent-encoding, so nothing here
/// can mis-escape a path. The Dock normalizes the entry (URL form, `book` bookmark, mod dates) on
/// its next write. Shape rationale and evidence: `DETAILS.md`.
pub(super) fn app_tile(bundle_path: &Path, bundle_id: Option<&str>, guid: i64) -> plist::Value {
    let mut file_data = plist::Dictionary::new();
    file_data.insert(
        "_CFURLString".to_string(),
        plist::Value::String(bundle_path.to_string_lossy().into_owned()),
    );
    file_data.insert(
        "_CFURLStringType".to_string(),
        plist::Value::Integer(POSIX_PATH_URL_TYPE.into()),
    );

    let mut tile_data = plist::Dictionary::new();
    if let Some(id) = bundle_id {
        tile_data.insert("bundle-identifier".to_string(), plist::Value::String(id.to_string()));
    }
    tile_data.insert("file-data".to_string(), plist::Value::Dictionary(file_data));
    tile_data.insert("file-label".to_string(), plist::Value::String(tile_label(bundle_path)));
    tile_data.insert("file-type".to_string(), plist::Value::Integer(APP_FILE_TYPE.into()));

    let mut entry = plist::Dictionary::new();
    entry.insert("GUID".to_string(), plist::Value::Integer(guid.into()));
    entry.insert("tile-data".to_string(), plist::Value::Dictionary(tile_data));
    entry.insert("tile-type".to_string(), plist::Value::String(FILE_TILE.to_string()));
    plist::Value::Dictionary(entry)
}

/// The name the Dock shows under the tile: the bundle's own name without `.app`.
fn tile_label(bundle_path: &Path) -> String {
    bundle_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `entries` with `tile` in front, so the new tile lands in the leftmost app slot.
pub(super) fn with_tile_first(entries: &[plist::Value], tile: plist::Value) -> Vec<plist::Value> {
    let mut next = Vec::with_capacity(entries.len() + 1);
    next.push(tile);
    next.extend_from_slice(entries);
    next
}

/// Whether `entries` already holds a tile for this app.
///
/// The bundle identifier is the key that decides it, and the path is only a fallback for a tile
/// that carries no identifier. Why: an identifier is an exact string with no encoding to get
/// wrong, whereas `_CFURLString` may be percent-encoded, may carry a trailing slash, and may be a
/// `/.file/id=…` file-reference URL that names the right app and matches no path we can build. The
/// question the caller is really asking is "is there already a Cmdr tile down there", which is
/// about the app rather than about which copy of it.
///
/// Both keys are checked, and either one matching answers yes: a false "already there" costs a
/// silent hint, while a false "not there" would put a second Cmdr tile in someone's Dock.
pub(super) fn holds_app(entries: &[plist::Value], bundle_id: Option<&str>, bundle_path: &Path) -> bool {
    entries.iter().any(|entry| {
        let id_matches = matches!(
            (bundle_id, entry_bundle_id(entry)),
            (Some(ours), Some(theirs)) if ours == theirs
        );
        id_matches || entry_path(entry).is_some_and(|path| path == bundle_path)
    })
}

/// The `tile-data` dictionary of an entry, if it has one.
fn tile_data(entry: &plist::Value) -> Option<&plist::Dictionary> {
    entry.as_dictionary()?.get("tile-data")?.as_dictionary()
}

/// The bundle identifier a tile names, if it carries one.
fn entry_bundle_id(entry: &plist::Value) -> Option<&str> {
    tile_data(entry)?.get("bundle-identifier")?.as_string()
}

/// The filesystem path a tile points at, if we can resolve one.
///
/// Both `_CFURLStringType` forms are handled: 0, a plain POSIX path (what we write), and 15, an
/// absolute `file://` URL with a percent-encoded path and a trailing slash (what macOS writes). A
/// file-reference URL (`file:///.file/id=…`) parses to a path that resolves to nothing, which is
/// exactly why [`holds_app`] doesn't lean on this.
fn entry_path(entry: &plist::Value) -> Option<PathBuf> {
    let file_data = tile_data(entry)?.get("file-data")?.as_dictionary()?;
    let raw = file_data.get("_CFURLString")?.as_string()?;
    let url_type = file_data
        .get("_CFURLStringType")
        .and_then(plist::Value::as_signed_integer)
        .unwrap_or(ABSOLUTE_URL_TYPE);

    let path = match url_type {
        POSIX_PATH_URL_TYPE => raw.to_string(),
        _ => {
            let encoded = raw.strip_prefix("file://")?;
            percent_encoding::percent_decode_str(encoded)
                .decode_utf8()
                .ok()?
                .into_owned()
        }
    };

    // The Dock writes `…/Cmdr.app/`; we write `…/Cmdr.app`. Trim so the two compare equal without
    // relying on `Path`'s own trailing-separator handling.
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile in the shape macOS itself writes: absolute URL, percent-encoded, trailing slash.
    fn macos_tile(bundle_id: &str, url: &str) -> plist::Value {
        let mut file_data = plist::Dictionary::new();
        file_data.insert("_CFURLString".to_string(), plist::Value::String(url.to_string()));
        file_data.insert(
            "_CFURLStringType".to_string(),
            plist::Value::Integer(ABSOLUTE_URL_TYPE.into()),
        );
        let mut tile_data = plist::Dictionary::new();
        tile_data.insert(
            "bundle-identifier".to_string(),
            plist::Value::String(bundle_id.to_string()),
        );
        tile_data.insert("file-data".to_string(), plist::Value::Dictionary(file_data));
        let mut entry = plist::Dictionary::new();
        entry.insert("tile-data".to_string(), plist::Value::Dictionary(tile_data));
        entry.insert("tile-type".to_string(), plist::Value::String(FILE_TILE.to_string()));
        plist::Value::Dictionary(entry)
    }

    fn string_at<'a>(entry: &'a plist::Value, path: &[&str]) -> Option<&'a str> {
        let mut current = entry;
        for key in path {
            current = current.as_dictionary()?.get(key)?;
        }
        current.as_string()
    }

    fn integer_at(entry: &plist::Value, path: &[&str]) -> Option<i64> {
        let mut current = entry;
        for key in path {
            current = current.as_dictionary()?.get(key)?;
        }
        current.as_signed_integer()
    }

    #[test]
    fn app_tile_carries_the_keys_a_file_tile_needs() {
        let tile = app_tile(Path::new("/Applications/Cmdr.app"), Some("com.veszelovszki.cmdr"), 42);

        assert_eq!(string_at(&tile, &["tile-type"]), Some(FILE_TILE));
        assert_eq!(integer_at(&tile, &["GUID"]), Some(42));
        assert_eq!(
            string_at(&tile, &["tile-data", "file-label"]),
            Some("Cmdr"),
            "the Dock shows the bundle name without its extension"
        );
        assert_eq!(integer_at(&tile, &["tile-data", "file-type"]), Some(APP_FILE_TYPE));
        assert_eq!(
            string_at(&tile, &["tile-data", "bundle-identifier"]),
            Some("com.veszelovszki.cmdr")
        );
        assert_eq!(
            string_at(&tile, &["tile-data", "file-data", "_CFURLString"]),
            Some("/Applications/Cmdr.app"),
            "the POSIX-path form needs no percent-encoding"
        );
        assert_eq!(
            integer_at(&tile, &["tile-data", "file-data", "_CFURLStringType"]),
            Some(POSIX_PATH_URL_TYPE)
        );
    }

    #[test]
    fn app_tile_omits_the_bundle_identifier_when_we_do_not_know_it() {
        let tile = app_tile(Path::new("/Applications/Cmdr.app"), None, 1);

        assert!(
            string_at(&tile, &["tile-data", "bundle-identifier"]).is_none(),
            "a guessed identifier would be worse than none: the Dock backfills it"
        );
    }

    #[test]
    fn app_tile_survives_a_bundle_path_with_a_space() {
        let tile = app_tile(Path::new("/Applications/Google Chrome.app"), None, 1);

        assert_eq!(
            string_at(&tile, &["tile-data", "file-data", "_CFURLString"]),
            Some("/Applications/Google Chrome.app")
        );
        assert_eq!(string_at(&tile, &["tile-data", "file-label"]), Some("Google Chrome"));
    }

    #[test]
    fn with_tile_first_puts_the_new_tile_in_the_leftmost_slot() {
        let existing = vec![
            macos_tile("md.obsidian", "file:///Applications/Obsidian.app/"),
            macos_tile("com.google.Chrome", "file:///Applications/Google%20Chrome.app/"),
        ];
        let tile = app_tile(Path::new("/Applications/Cmdr.app"), Some("com.x.cmdr"), 7);

        let next = with_tile_first(&existing, tile);

        assert_eq!(next.len(), 3);
        assert_eq!(
            string_at(&next[0], &["tile-data", "bundle-identifier"]),
            Some("com.x.cmdr")
        );
        assert_eq!(
            string_at(&next[1], &["tile-data", "bundle-identifier"]),
            Some("md.obsidian"),
            "existing tiles keep their order behind the new one"
        );
        assert_eq!(
            string_at(&next[2], &["tile-data", "bundle-identifier"]),
            Some("com.google.Chrome")
        );
    }

    #[test]
    fn holds_app_finds_the_app_by_bundle_identifier() {
        let entries = vec![macos_tile("com.veszelovszki.cmdr", "file:///Applications/Cmdr.app/")];

        assert!(holds_app(
            &entries,
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn holds_app_finds_the_app_by_identifier_even_from_another_copy() {
        // A tile pointing at an older copy still means "there's a Cmdr down there". Offering to add
        // a second one would put two Cmdr tiles in the Dock.
        let entries = vec![macos_tile(
            "com.veszelovszki.cmdr",
            "file:///Users/jane/Applications/Cmdr.app/",
        )];

        assert!(holds_app(
            &entries,
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn holds_app_falls_back_to_the_path_when_a_tile_has_no_identifier() {
        let mut file_data = plist::Dictionary::new();
        file_data.insert(
            "_CFURLString".to_string(),
            plist::Value::String("file:///Applications/Cmdr.app/".to_string()),
        );
        file_data.insert(
            "_CFURLStringType".to_string(),
            plist::Value::Integer(ABSOLUTE_URL_TYPE.into()),
        );
        let mut tile_data = plist::Dictionary::new();
        tile_data.insert("file-data".to_string(), plist::Value::Dictionary(file_data));
        let mut entry = plist::Dictionary::new();
        entry.insert("tile-data".to_string(), plist::Value::Dictionary(tile_data));
        let entries = vec![plist::Value::Dictionary(entry)];

        assert!(holds_app(
            &entries,
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn holds_app_matches_the_tile_we_write_ourselves() {
        // The POSIX-path form, before the Dock has had a chance to rewrite it as a URL.
        let entries = vec![app_tile(
            Path::new("/Applications/Cmdr.app"),
            Some("com.veszelovszki.cmdr"),
            1,
        )];

        assert!(holds_app(&entries, None, Path::new("/Applications/Cmdr.app")));
    }

    #[test]
    fn holds_app_says_no_for_a_dock_full_of_other_apps() {
        let entries = vec![
            macos_tile("md.obsidian", "file:///Applications/Obsidian.app/"),
            macos_tile("com.google.Chrome", "file:///Applications/Google%20Chrome.app/"),
            macos_tile(
                "com.apple.systempreferences",
                "file:///System/Applications/System%20Settings.app/",
            ),
        ];

        assert!(!holds_app(
            &entries,
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn holds_app_says_no_for_an_empty_dock() {
        assert!(!holds_app(
            &[],
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn holds_app_ignores_a_tile_that_is_not_a_dictionary() {
        let entries = vec![plist::Value::String("not a tile".to_string())];

        assert!(!holds_app(
            &entries,
            Some("com.veszelovszki.cmdr"),
            Path::new("/Applications/Cmdr.app")
        ));
    }

    #[test]
    fn entry_path_decodes_a_percent_encoded_url() {
        let tile = macos_tile("com.google.Chrome", "file:///Applications/Google%20Chrome.app/");

        assert_eq!(
            entry_path(&tile),
            Some(PathBuf::from("/Applications/Google Chrome.app"))
        );
    }

    #[test]
    fn entry_path_drops_the_trailing_slash_macos_writes() {
        let tile = macos_tile("md.obsidian", "file:///Applications/Obsidian.app/");

        assert_eq!(entry_path(&tile), Some(PathBuf::from("/Applications/Obsidian.app")));
    }

    #[test]
    fn entry_path_reads_the_posix_path_form_verbatim() {
        let tile = app_tile(Path::new("/Applications/Cmdr.app"), None, 1);

        assert_eq!(
            entry_path(&tile),
            Some(PathBuf::from("/Applications/Cmdr.app")),
            "type 0 is a plain path: percent-decoding it would corrupt a literal '%' in a name"
        );
    }

    #[test]
    fn entry_path_leaves_a_literal_percent_alone_in_the_posix_form() {
        let tile = app_tile(Path::new("/Applications/100%25 Orange.app"), None, 1);

        assert_eq!(
            entry_path(&tile),
            Some(PathBuf::from("/Applications/100%25 Orange.app"))
        );
    }

    #[test]
    fn entry_path_gives_up_on_a_url_scheme_it_does_not_know() {
        let tile = macos_tile("com.example.web", "https://example.com/");

        assert_eq!(entry_path(&tile), None);
    }

    #[test]
    fn entry_path_gives_up_on_a_tile_with_no_file_data() {
        let mut entry = plist::Dictionary::new();
        entry.insert(
            "tile-data".to_string(),
            plist::Value::Dictionary(plist::Dictionary::new()),
        );

        assert_eq!(entry_path(&plist::Value::Dictionary(entry)), None);
    }

    #[test]
    fn a_file_reference_url_does_not_masquerade_as_the_bundle_path() {
        // The reason `holds_app` leads with the bundle identifier: this tile names the right app
        // and yields a path that matches nothing.
        let tile = macos_tile("com.veszelovszki.cmdr", "file:///.file/id=6815814.9265358/");

        assert_eq!(entry_path(&tile), Some(PathBuf::from("/.file/id=6815814.9265358")));
        assert!(
            holds_app(
                &[tile],
                Some("com.veszelovszki.cmdr"),
                Path::new("/Applications/Cmdr.app")
            ),
            "the identifier still has to find it"
        );
    }
}
