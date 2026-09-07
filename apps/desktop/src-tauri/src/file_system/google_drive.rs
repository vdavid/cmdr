//! Google Drive item links: turning a local path inside Google Drive into the
//! `drive.google.com` / `docs.google.com` URL for the same item.
//!
//! Drive for desktop never registers a URL scheme (its `Info.plist` carries no
//! `CFBundleURLTypes` and no `NSServices`, verified on Drive for desktop
//! 2025-08 / macOS 15), and its Finder items are File Provider custom actions
//! only Finder can render. So there is no way to pop Drive's own Share sheet
//! from another app. Opening the item on the web is the reachable equivalent:
//! Share is one click away there.
//!
//! ## Where the item ID comes from
//!
//! Two sources, in this order, because they cover different Drive setups:
//!
//! 1. **Google-native shortcut files** (`.gdoc`, `.gsheet`, `.gslides`, `.gform`,
//!    …) are small JSON stubs carrying `doc_id`. They work in BOTH of Drive's
//!    modes, and they're checked first because a native doc's canonical URL is
//!    on `docs.google.com`, which the ID alone can't tell us.
//! 2. **The `com.google.drivefs.item-id#S` extended attribute**, which Drive
//!    stamps on every file and folder it streams.
//!
//! ## Gotcha: the xattr only exists in "stream" mode
//!
//! Drive for desktop has two modes. In **stream** mode the files live under
//! `~/Library/CloudStorage/GoogleDrive-<account>/` and every file AND folder
//! carries the item-id xattr. In **mirror** mode the files are ordinary local
//! files (`~/My Drive` by default, but the user picks the folder) and they
//! carry NO xattr at all, so only the `.gdoc`-family stubs resolve there.
//!
//! Verified on this machine 2026-09-07 with `xattr -r -l`: present throughout
//! `~/Library/CloudStorage/GoogleDrive-…` (including on directories), and
//! absent on every file in the mirrored `~/My Drive`.
//!
//! That's why nothing here gates on a path prefix. We offer the menu items when
//! an ID actually resolves, which is self-validating and works in mirror mode
//! for native docs.

use std::path::Path;

/// The extended attribute Drive for desktop stamps on streamed files and folders.
/// The `#S` suffix is macOS's File Provider "syncable attribute" marker and is
/// part of the name: `xattr::get` with the bare name finds nothing.
pub const DRIVE_ITEM_ID_XATTR: &str = "com.google.drivefs.item-id#S";

/// The canonical web URL for a Drive item, by item kind.
///
/// Every shape here is what Google's own Drive API returns as the item's
/// `viewUrl` (checked against real items in a live account, 2026-09-07), rather
/// than a shape copied from a blog post:
///
/// - binary file → `https://drive.google.com/file/d/<id>/view`
/// - folder → `https://drive.google.com/drive/folders/<id>`
/// - Doc → `https://docs.google.com/document/d/<id>/edit`
/// - Sheet → `https://docs.google.com/spreadsheets/d/<id>/edit`
/// - Slides → `https://docs.google.com/presentation/d/<id>/edit`
/// - Form → `https://docs.google.com/forms/d/<id>/edit`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriveItemKind {
    /// A Google-native document, edited on `docs.google.com` under this path
    /// segment (`document`, `spreadsheets`, `presentation`, `forms`).
    Native(&'static str),
    /// A Google-native type we have no verified editor path for. Drive's
    /// type-agnostic opener resolves it server-side.
    NativeUnknown,
    /// A folder.
    Folder,
    /// Anything else Drive stores byte-for-byte.
    Binary,
}

impl DriveItemKind {
    fn url_for(self, id: &str) -> String {
        match self {
            Self::Native(editor) => format!("https://docs.google.com/{editor}/d/{id}/edit"),
            Self::NativeUnknown => format!("https://drive.google.com/open?id={id}"),
            Self::Folder => format!("https://drive.google.com/drive/folders/{id}"),
            Self::Binary => format!("https://drive.google.com/file/d/{id}/view"),
        }
    }
}

/// Maps a Google-native shortcut extension to its `docs.google.com` editor
/// segment. `None` means "we know it's native but not where it's edited".
fn native_editor_segment(extension: &str) -> Option<&'static str> {
    match extension {
        "gdoc" => Some("document"),
        "gsheet" => Some("spreadsheets"),
        "gslides" => Some("presentation"),
        "gform" => Some("forms"),
        _ => None,
    }
}

/// Google-native shortcut file extensions. `.gdraw`, `.gsite`, `.gjam`, and
/// friends are in here without an editor segment: we can still open them, via
/// Drive's type-agnostic `open?id=` resolver, we just don't invent an editor
/// path we haven't verified.
fn is_native_shortcut_extension(extension: &str) -> bool {
    native_editor_segment(extension).is_some()
        || matches!(
            extension,
            "gdraw" | "gsite" | "gjam" | "gmap" | "gtable" | "gscript" | "glink" | "gnote"
        )
}

/// The JSON body Drive writes into a `.gdoc`-family shortcut file.
///
/// Only `doc_id` is load-bearing. The stubs also carry a `resource_key` (empty
/// for everything not shared via a resource-key link) and the account `email`;
/// there is no `url` field, so the URL has to be built from the ID and the
/// extension.
#[derive(serde::Deserialize)]
struct NativeShortcut {
    doc_id: String,
    #[serde(default)]
    resource_key: String,
}

/// Reads the Drive item ID out of a Google-native shortcut file.
fn read_native_shortcut(path: &Path) -> Option<NativeShortcut> {
    // These stubs are a couple of hundred bytes; a malformed or oversized one
    // just means no menu item.
    let bytes = std::fs::read(path).ok()?;
    let parsed: NativeShortcut = serde_json::from_slice(&bytes).ok()?;
    if parsed.doc_id.is_empty() {
        return None;
    }
    Some(parsed)
}

/// Reads the `com.google.drivefs.item-id#S` xattr, if present and sane.
#[cfg(target_os = "macos")]
fn read_item_id_xattr(path: &Path) -> Option<String> {
    let raw = xattr::get(path, DRIVE_ITEM_ID_XATTR).ok().flatten()?;
    let id = String::from_utf8(raw).ok()?;
    let id = id.trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

#[cfg(not(target_os = "macos"))]
fn read_item_id_xattr(_path: &Path) -> Option<String> {
    None
}

/// The web URL for the Drive item at `path`, or `None` when the path isn't a
/// Drive item we can identify.
///
/// `is_directory` is passed in rather than stat'd here: the menu already knows
/// it, and this runs while the user waits for a context menu.
pub fn item_url(path: &Path, is_directory: bool) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    // Native shortcut stubs first: they resolve in both Drive modes, and only
    // the extension tells us the item is native rather than a plain file.
    if !is_directory
        && is_native_shortcut_extension(&extension)
        && let Some(shortcut) = read_native_shortcut(path)
    {
        let kind = match native_editor_segment(&extension) {
            Some(editor) => DriveItemKind::Native(editor),
            None => DriveItemKind::NativeUnknown,
        };
        let mut url = kind.url_for(&shortcut.doc_id);
        // A resource key is required to open items shared through a
        // resource-key link; it's empty on everything else.
        if !shortcut.resource_key.is_empty() {
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push_str(&format!("{separator}resourcekey={}", shortcut.resource_key));
        }
        return Some(url);
    }

    let id = read_item_id_xattr(path)?;
    let kind = if is_directory {
        DriveItemKind::Folder
    } else {
        DriveItemKind::Binary
    };
    Some(kind.url_for(&id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_shortcut(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, body).expect("write shortcut");
        path
    }

    /// The exact JSON Drive for desktop writes (captured from a real `.gdoc`).
    fn stub_json(doc_id: &str) -> String {
        format!(
            r#"{{"":"WARNING! DO NOT EDIT THIS FILE! ANY CHANGES MADE WILL BE LOST!","doc_id":"{doc_id}","resource_key":"","email":"someone@example.com"}}"#
        )
    }

    #[test]
    fn native_docs_open_on_the_docs_host() {
        let dir = TempDir::new().unwrap();
        let doc = write_shortcut(&dir, "notes.gdoc", &stub_json("DOC1"));
        let sheet = write_shortcut(&dir, "budget.gsheet", &stub_json("SHEET1"));
        let slides = write_shortcut(&dir, "deck.gslides", &stub_json("SLIDES1"));
        let form = write_shortcut(&dir, "survey.gform", &stub_json("FORM1"));

        assert_eq!(
            item_url(&doc, false).as_deref(),
            Some("https://docs.google.com/document/d/DOC1/edit")
        );
        assert_eq!(
            item_url(&sheet, false).as_deref(),
            Some("https://docs.google.com/spreadsheets/d/SHEET1/edit")
        );
        assert_eq!(
            item_url(&slides, false).as_deref(),
            Some("https://docs.google.com/presentation/d/SLIDES1/edit")
        );
        assert_eq!(
            item_url(&form, false).as_deref(),
            Some("https://docs.google.com/forms/d/FORM1/edit")
        );
    }

    #[test]
    fn native_type_without_a_verified_editor_path_uses_drives_resolver() {
        let dir = TempDir::new().unwrap();
        let drawing = write_shortcut(&dir, "sketch.gdraw", &stub_json("DRAW1"));
        assert_eq!(
            item_url(&drawing, false).as_deref(),
            Some("https://drive.google.com/open?id=DRAW1")
        );
    }

    #[test]
    fn a_resource_key_rides_along_when_the_stub_carries_one() {
        let dir = TempDir::new().unwrap();
        let body = r#"{"doc_id":"DOC2","resource_key":"KEY2","email":"x@example.com"}"#;
        let doc = write_shortcut(&dir, "shared.gdoc", body);
        assert_eq!(
            item_url(&doc, false).as_deref(),
            Some("https://docs.google.com/document/d/DOC2/edit?resourcekey=KEY2")
        );
    }

    #[test]
    fn a_malformed_or_empty_stub_yields_no_link() {
        let dir = TempDir::new().unwrap();
        let broken = write_shortcut(&dir, "broken.gdoc", "not json at all");
        let empty_id = write_shortcut(&dir, "empty.gdoc", r#"{"doc_id":""}"#);
        assert_eq!(item_url(&broken, false), None);
        assert_eq!(item_url(&empty_id, false), None);
    }

    #[test]
    fn a_path_outside_drive_yields_no_link() {
        let dir = TempDir::new().unwrap();
        let plain = write_shortcut(&dir, "readme.txt", "hello");
        assert_eq!(item_url(&plain, false), None);
        assert_eq!(item_url(dir.path(), true), None);
    }

    /// The stream-mode path: the xattr decides, and file vs folder picks the shape.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_item_id_xattr_drives_file_and_folder_urls() {
        let dir = TempDir::new().unwrap();
        let file = write_shortcut(&dir, "photo.jpg", "not really a jpeg");
        let folder = dir.path().join("subfolder");
        fs::create_dir(&folder).unwrap();

        xattr::set(&file, DRIVE_ITEM_ID_XATTR, b"FILEID").expect("set xattr on file");
        xattr::set(&folder, DRIVE_ITEM_ID_XATTR, b"FOLDERID").expect("set xattr on folder");

        assert_eq!(
            item_url(&file, false).as_deref(),
            Some("https://drive.google.com/file/d/FILEID/view")
        );
        assert_eq!(
            item_url(&folder, true).as_deref(),
            Some("https://drive.google.com/drive/folders/FOLDERID")
        );
    }

    /// A `.gdoc` in stream mode carries BOTH the stub and the xattr, holding the
    /// same ID (verified on a real Drive item). The stub has to win, or a native
    /// doc would open on the wrong host.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_stub_wins_over_the_xattr_for_native_docs() {
        let dir = TempDir::new().unwrap();
        let doc = write_shortcut(&dir, "notes.gdoc", &stub_json("SAMEID"));
        xattr::set(&doc, DRIVE_ITEM_ID_XATTR, b"SAMEID").expect("set xattr");

        assert_eq!(
            item_url(&doc, false).as_deref(),
            Some("https://docs.google.com/document/d/SAMEID/edit")
        );
    }

    /// An empty xattr is not an ID.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_blank_xattr_yields_no_link() {
        let dir = TempDir::new().unwrap();
        let file = write_shortcut(&dir, "photo.jpg", "x");
        xattr::set(&file, DRIVE_ITEM_ID_XATTR, b"   ").expect("set xattr");
        assert_eq!(item_url(&file, false), None);
    }
}
