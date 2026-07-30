//! Package/bundle detection: is this directory presented as a single composite
//! icon rather than a folder glyph?
//!
//! A directory whose name ends in a known package extension (`.app`, `.bundle`,
//! …) shows the app/plugin icon in Finder, so it deviates from the generic `dir`
//! glyph and earns a real per-path icon fetch. The check is a pure suffix test
//! with no I/O, cheap enough to run for every entry during a listing, so
//! `entry::get_icon_id` routes packages straight to a `pkg:{path}` key.
//!
//! The other Tier C signal — a folder carrying `kHasCustomIcon` in its
//! `com.apple.FinderInfo` xattr — needs a `getxattr` syscall and therefore stays
//! in the app, where it runs only for the bounded set of *visible* directory
//! paths the frontend asks about.

/// Prefix marking package icon keys (`pkg:/Applications/Safari.app`, …). Shares
/// the `path:`-key lifecycle (LRU-capped, not persisted) — `.app` icons are
/// per-app (each different), so they're as unbounded as custom-icon folders. The
/// distinct prefix keeps the two candidate sources legible in logs and lets a
/// future eviction-tuning pass treat them separately if needed.
pub const PKG_KEY_PREFIX: &str = "pkg:";

/// Directory-name suffixes that mark a macOS package/bundle. A package presents a
/// single composite icon in Finder (the app/plugin icon), not a folder glyph, so
/// these deviate from `dir` and earn a real fetch. The list is intentionally
/// bounded to the common, user-visible bundle kinds; obscure private bundle types
/// (`.xpc`, `.appex`, …) stay generic rather than paying a fetch for an icon a
/// user almost never sees in a normal browse.
///
/// Compared case-insensitively against the directory name. `.app` is the dominant
/// case; the rest are rarer but still show a distinct composite icon.
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",         // applications
    "bundle",      // loadable bundles
    "framework",   // shared frameworks
    "plugin",      // plug-ins
    "kext",        // kernel extensions
    "prefpane",    // System Settings panes
    "qlgenerator", // Quick Look generators
    "wdgt",        // Dashboard widgets
    "mdimporter",  // Spotlight importers
];

/// Returns true when `name` ends in a known package extension (case-insensitive).
/// Pure, no I/O — safe to call for every directory entry during listing.
///
/// `name` is the directory's own file name (the last path component), not the
/// full path: we classify by how Finder presents the bundle, which is purely a
/// function of its extension.
pub fn is_package_dir(name: &str) -> bool {
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    // Reject a leading-dot dotfile with no real extension ("`.app`" the folder,
    // not "Safari.app"): `rfind('.')` at index 0 means the whole name is the
    // "extension", which is a dotfile, not a bundle.
    if dot == 0 {
        return false;
    }
    let ext = &name[dot + 1..];
    PACKAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known))
}

/// Builds the `pkg:{path}` icon key for a package directory, or `None` when the
/// directory name isn't a known package. The key carries the full path because
/// `.app` icons are per-app — each bundle's icon is distinct — so unlike
/// `special:*` they can't share a bounded key.
pub fn package_icon_id(name: &str, path: &str) -> Option<String> {
    if is_package_dir(name) {
        Some(format!("{PKG_KEY_PREFIX}{path}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundle_is_a_package() {
        assert!(is_package_dir("Safari.app"));
        assert!(is_package_dir("My Cool App.app"));
    }

    #[test]
    fn known_bundle_extensions_are_packages() {
        assert!(is_package_dir("Foo.bundle"));
        assert!(is_package_dir("Cocoa.framework"));
        assert!(is_package_dir("Some.plugin"));
        assert!(is_package_dir("Driver.kext"));
        assert!(is_package_dir("Sound.prefpane"));
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert!(is_package_dir("LOUD.APP"));
        assert!(is_package_dir("Mixed.App"));
    }

    #[test]
    fn plain_folders_are_not_packages() {
        assert!(!is_package_dir("Documents"));
        assert!(!is_package_dir("my-project"));
        assert!(!is_package_dir("folder.with.dots"));
        assert!(!is_package_dir("archive.zip")); // not a directory-package ext
    }

    #[test]
    fn a_dotfile_is_not_a_package() {
        // `.app` as the whole name is a dotfile, not a bundle.
        assert!(!is_package_dir(".app"));
        assert!(!is_package_dir(".config"));
    }

    #[test]
    fn package_icon_id_uses_the_pkg_prefix_and_full_path() {
        assert_eq!(
            package_icon_id("Safari.app", "/Applications/Safari.app").as_deref(),
            Some("pkg:/Applications/Safari.app")
        );
        assert_eq!(package_icon_id("Documents", "/Users/x/Documents"), None);
    }
}
