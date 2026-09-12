//! The pure half of File Provider actions: what a provider declares, which of it Cmdr
//! hides, what label an action gets, and which domain a right-clicked path belongs to.
//!
//! No Objective-C here, so every rule is unit-tested without a provider installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Google Drive for desktop's File Provider extension, as its items' `providerID` names it.
pub const GOOGLE_DRIVE_PROVIDER_ID: &str = "com.google.drivefs.fpext";

/// Actions a provider declares that Cmdr already offers as items of its own.
///
/// Cmdr's "Open in Google Drive", "Copy Google Drive link", and "Ask Gemini" build their
/// web URLs themselves, so they also work in Drive's mirror mode, where Drive's own
/// actions can't reach. Showing both would put each twice in one menu. Drive's
/// `ACTION_SHARE` has no Cmdr twin and stays.
const HIDDEN_BY_CMDR: &[(&str, &[&str])] = &[(
    GOOGLE_DRIVE_PROVIDER_ID,
    &["ACTION_OPEN", "ACTION_COPY_LINK", "ACTION_OPEN_GEMINI_WEB"],
)];

/// How many path components below the home folder a symlink into a domain is followed.
/// `~/Dropbox` (Dropbox's own link into `~/Library/CloudStorage/Dropbox`) is one deep.
/// Bounded so the walk costs a handful of `readlink`s on the home volume, never a trip
/// deep into some mount a folder in home happens to hold.
const HOME_SYMLINK_DEPTH: usize = 2;

/// One entry of an extension's `NSExtensionFileProviderActions`.
///
/// Identifiers aren't unique: Drive declares `ACTION_SHARE` three times, with a different
/// rule and label each, so an action is addressed by its POSITION in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDeclaration {
    /// What `FPVendorDefinedActionOperation` is built with.
    pub identifier: String,
    /// The key into the extension's `Localizable.strings` the label comes from.
    pub name_key: String,
    /// The `NSPredicate` format string that decides whether the action shows.
    pub rule: String,
}

/// An extension's declared actions, plus the bundle version they were read at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declarations {
    pub version: String,
    pub actions: Vec<ActionDeclaration>,
}

/// Reads the actions out of an extension's `Info.plist` (XML or binary).
///
/// `None` when the bytes aren't a property list. An entry missing its identifier or its
/// rule is skipped: Finder can't show it either. A missing name falls back to the
/// identifier, which is what the label lookup would end on anyway.
pub fn parse_declarations(info_plist: &[u8]) -> Option<Declarations> {
    let root = plist::from_bytes::<plist::Value>(info_plist).ok()?;
    let root = root.as_dictionary()?;
    let version = root
        .get("CFBundleVersion")
        .and_then(plist::Value::as_string)
        .unwrap_or_default()
        .to_string();
    let actions = root
        .get("NSExtension")
        .and_then(plist::Value::as_dictionary)
        .and_then(|extension| extension.get("NSExtensionFileProviderActions"))
        .and_then(plist::Value::as_array)
        .map(|entries| entries.iter().filter_map(declaration).collect())
        .unwrap_or_default();
    Some(Declarations { version, actions })
}

/// One `NSExtensionFileProviderActions` entry, or `None` when Finder couldn't show it.
fn declaration(entry: &plist::Value) -> Option<ActionDeclaration> {
    let entry = entry.as_dictionary()?;
    let text = |key: &str| entry.get(key).and_then(plist::Value::as_string).map(str::to_string);
    let identifier = text("NSExtensionFileProviderActionIdentifier")?;
    let rule = text("NSExtensionFileProviderActionActivationRule")?;
    let name_key = text("NSExtensionFileProviderActionName").unwrap_or_else(|| identifier.clone());
    Some(ActionDeclaration {
        identifier,
        name_key,
        rule,
    })
}

/// Whether Cmdr leaves `identifier` out because it has its own item for the same job.
pub fn is_hidden_by_cmdr(provider_id: &str, identifier: &str) -> bool {
    HIDDEN_BY_CMDR
        .iter()
        .any(|(provider, identifiers)| *provider == provider_id && identifiers.contains(&identifier))
}

/// The label for `name_key`: the first table that has it, else the key itself.
///
/// `tables` runs from the most preferred localization to the last resort (the UI
/// language, then English), so a provider without the user's language still reads in
/// English rather than as `COPY_LINK_ACTION_NAME`.
pub fn resolve_label(name_key: &str, tables: &[&HashMap<String, String>]) -> String {
    tables
        .iter()
        .find_map(|table| table.get(name_key))
        .cloned()
        .unwrap_or_else(|| name_key.to_string())
}

/// The domain every one of `paths` belongs to, and each path as File Provider sees it.
///
/// `domain_roots[i]` is domain `i`'s storage roots. Answers `None` unless ALL paths land
/// in the SAME domain: Finder offers a provider's actions only for a selection that's
/// wholly that provider's. A path reached through a symlink in the home folder (like
/// `~/Dropbox`) comes back resolved, since that's the spelling File Provider knows.
///
/// `read_link` is `std::fs::read_link` in production, a table in tests.
pub fn domain_for_paths(
    domain_roots: &[Vec<PathBuf>],
    paths: &[PathBuf],
    home: Option<&Path>,
    read_link: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<(usize, Vec<PathBuf>)> {
    let mut domain = None;
    let mut resolved = Vec::with_capacity(paths.len());
    for path in paths {
        let (index, visible) = domain_for_path(domain_roots, path, home, &read_link)?;
        if *domain.get_or_insert(index) != index {
            return None;
        }
        resolved.push(visible);
    }
    Some((domain?, resolved))
}

/// The domain one path belongs to, directly or through the first symlink in the top
/// [`HOME_SYMLINK_DEPTH`] levels of home, plus the spelling File Provider knows it by.
fn domain_for_path(
    domain_roots: &[Vec<PathBuf>],
    path: &Path,
    home: Option<&Path>,
    read_link: &impl Fn(&Path) -> Option<PathBuf>,
) -> Option<(usize, PathBuf)> {
    if let Some(index) = domain_under(domain_roots, path) {
        return Some((index, path.to_path_buf()));
    }
    let home = home?;
    let components: Vec<_> = path.strip_prefix(home).ok()?.components().collect();
    let mut ancestor = home.to_path_buf();
    for (depth, component) in components.iter().enumerate().take(HOME_SYMLINK_DEPTH) {
        ancestor.push(component);
        let Some(target) = read_link(&ancestor) else {
            continue;
        };
        // Only the first link counts: whatever it points at either is a domain or isn't,
        // and nothing past it gets read.
        let target = match ancestor.parent() {
            Some(parent) if target.is_relative() => parent.join(target),
            _ => target,
        };
        let visible = components[depth + 1..]
            .iter()
            .fold(target, |joined, component| joined.join(component));
        return domain_under(domain_roots, &visible).map(|index| (index, visible));
    }
    None
}

/// The first domain with a root `path` sits under, compared component-wise.
fn domain_under(domain_roots: &[Vec<PathBuf>], path: &Path) -> Option<usize> {
    domain_roots
        .iter()
        .position(|roots| roots.iter().any(|root| path.starts_with(root)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plist(actions: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleVersion</key><string>130.0.2</string>
  <key>NSExtension</key><dict>
    <key>NSExtensionFileProviderActions</key><array>{actions}</array>
  </dict>
</dict></plist>"#
        )
        .into_bytes()
    }

    fn action(identifier: &str, name: &str, rule: &str) -> String {
        format!(
            "<dict><key>NSExtensionFileProviderActionIdentifier</key><string>{identifier}</string>\
             <key>NSExtensionFileProviderActionName</key><string>{name}</string>\
             <key>NSExtensionFileProviderActionActivationRule</key><string>{rule}</string></dict>"
        )
    }

    /// An excerpt of Drive for desktop 130.0's declarations: `ACTION_SHARE` appears three
    /// times, and each one is its own entry.
    #[test]
    fn drives_repeated_share_declarations_stay_separate_entries() {
        let bytes = plist(
            &[
                action("ACTION_OPEN", "ACTION_OPEN", "fileproviderItems.@count == 1"),
                action("ACTION_SHARE", "SHARE_WITH_DRIVE", "SHARED_DRIVE_ROOT == NO"),
                action(
                    "ACTION_SHARE",
                    "SHARE_ADD_OR_MANAGE_MEMBERS",
                    "CAN_MANAGE_MEMBERS == YES",
                ),
                action("ACTION_SHARE", "SHARE_VIEW_MEMBERS", "CAN_MANAGE_MEMBERS == NO"),
            ]
            .concat(),
        );

        let parsed = parse_declarations(&bytes).expect("a valid Info.plist parses");
        assert_eq!(parsed.version, "130.0.2");
        let keys: Vec<(&str, &str)> = parsed
            .actions
            .iter()
            .map(|a| (a.identifier.as_str(), a.name_key.as_str()))
            .collect();
        assert_eq!(
            keys,
            [
                ("ACTION_OPEN", "ACTION_OPEN"),
                ("ACTION_SHARE", "SHARE_WITH_DRIVE"),
                ("ACTION_SHARE", "SHARE_ADD_OR_MANAGE_MEMBERS"),
                ("ACTION_SHARE", "SHARE_VIEW_MEMBERS"),
            ]
        );
        assert_eq!(parsed.actions[1].rule, "SHARED_DRIVE_ROOT == NO");
    }

    /// An excerpt of Dropbox 270.3's declarations, including the settings entry, which is
    /// an action like any other.
    #[test]
    fn dropbox_declarations_parse_in_order() {
        let bytes = plist(
            &[
                action(
                    "com.getdropbox.dropbox.fileprovider.action.copy_link",
                    "COPY_LINK_ACTION_NAME",
                    "domainUserInfo.show_copy_link == 1",
                ),
                action(
                    "com.getdropbox.dropbox.fileprovider.action.context_menu_settings",
                    "CONTEXT_MENU_SETTINGS_ACTION_NAME",
                    "TRUEPREDICATE",
                ),
            ]
            .concat(),
        );

        let parsed = parse_declarations(&bytes).expect("a valid Info.plist parses");
        let identifiers: Vec<&str> = parsed.actions.iter().map(|a| a.identifier.as_str()).collect();
        assert_eq!(
            identifiers,
            [
                "com.getdropbox.dropbox.fileprovider.action.copy_link",
                "com.getdropbox.dropbox.fileprovider.action.context_menu_settings",
            ]
        );
    }

    #[test]
    fn an_entry_without_an_identifier_or_a_rule_is_skipped_and_a_missing_name_falls_back() {
        let bytes = plist(
            "<dict><key>NSExtensionFileProviderActionName</key><string>NO_ID</string>\
               <key>NSExtensionFileProviderActionActivationRule</key><string>TRUEPREDICATE</string></dict>\
             <dict><key>NSExtensionFileProviderActionIdentifier</key><string>NO_RULE</string></dict>\
             <dict><key>NSExtensionFileProviderActionIdentifier</key><string>NAMELESS</string>\
               <key>NSExtensionFileProviderActionActivationRule</key><string>TRUEPREDICATE</string></dict>",
        );

        let parsed = parse_declarations(&bytes).expect("a valid Info.plist parses");
        assert_eq!(
            parsed.actions,
            [ActionDeclaration {
                identifier: "NAMELESS".to_string(),
                name_key: "NAMELESS".to_string(),
                rule: "TRUEPREDICATE".to_string(),
            }]
        );
    }

    #[test]
    fn an_extension_without_actions_declares_none_and_garbage_is_no_plist() {
        let no_actions = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleVersion</key><string>1</string></dict></plist>"#;
        assert_eq!(parse_declarations(no_actions).map(|d| d.actions.len()), Some(0));
        assert_eq!(parse_declarations(b"not a plist"), None);
    }

    #[test]
    fn cmdr_hides_exactly_drives_three_duplicates() {
        for identifier in ["ACTION_OPEN", "ACTION_COPY_LINK", "ACTION_OPEN_GEMINI_WEB"] {
            assert!(is_hidden_by_cmdr(GOOGLE_DRIVE_PROVIDER_ID, identifier), "{identifier}");
        }
        for identifier in ["ACTION_SHARE", "ACTION_PIN", "ACTION_OPEN_REVISION_FOLDER"] {
            assert!(!is_hidden_by_cmdr(GOOGLE_DRIVE_PROVIDER_ID, identifier), "{identifier}");
        }
        // The same identifier from another provider is that provider's business.
        assert!(!is_hidden_by_cmdr("com.getdropbox.dropbox.fileprovider", "ACTION_OPEN"));
    }

    fn table(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn a_label_comes_from_the_first_table_that_has_it() {
        let swedish = table(&[("COPY_LINK_ACTION_NAME", "Kopiera Dropbox-länk")]);
        let english = table(&[
            ("COPY_LINK_ACTION_NAME", "Copy Dropbox link"),
            ("SHARE_ACTION_NAME", "Share..."),
        ]);

        assert_eq!(
            resolve_label("COPY_LINK_ACTION_NAME", &[&swedish, &english]),
            "Kopiera Dropbox-länk"
        );
        // Missing in the UI language: English.
        assert_eq!(resolve_label("SHARE_ACTION_NAME", &[&swedish, &english]), "Share...");
        // Missing everywhere: the key itself.
        assert_eq!(resolve_label("UNKNOWN_KEY", &[&swedish, &english]), "UNKNOWN_KEY");
        assert_eq!(resolve_label("UNKNOWN_KEY", &[]), "UNKNOWN_KEY");
    }

    fn roots() -> Vec<Vec<PathBuf>> {
        vec![
            vec![PathBuf::from("/Users/me/Library/CloudStorage/Dropbox")],
            vec![PathBuf::from(
                "/Users/me/Library/CloudStorage/GoogleDrive-me@example.com",
            )],
        ]
    }

    fn no_links(_: &Path) -> Option<PathBuf> {
        None
    }

    fn home() -> Option<&'static Path> {
        Some(Path::new("/Users/me"))
    }

    #[test]
    fn paths_under_one_domain_root_resolve_to_it_unchanged() {
        let paths = vec![
            PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/a.pdf"),
            PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/folder"),
        ];
        assert_eq!(
            domain_for_paths(&roots(), &paths, home(), no_links),
            Some((0, paths.clone()))
        );
    }

    #[test]
    fn a_selection_spanning_two_domains_or_leaving_one_resolves_to_none() {
        let across = vec![
            PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/a.pdf"),
            PathBuf::from("/Users/me/Library/CloudStorage/GoogleDrive-me@example.com/b.pdf"),
        ];
        assert_eq!(domain_for_paths(&roots(), &across, home(), no_links), None);

        let half_local = vec![
            PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/a.pdf"),
            PathBuf::from("/Users/me/Documents/b.pdf"),
        ];
        assert_eq!(domain_for_paths(&roots(), &half_local, home(), no_links), None);
        assert_eq!(domain_for_paths(&roots(), &[], home(), no_links), None);
    }

    /// Component-wise, so a sibling that merely shares the root's name prefix isn't in it.
    #[test]
    fn a_root_matches_by_component_not_by_string_prefix() {
        let sibling = vec![PathBuf::from("/Users/me/Library/CloudStorage/Dropbox-Old/a.pdf")];
        assert_eq!(domain_for_paths(&roots(), &sibling, home(), no_links), None);
    }

    /// `~/Dropbox` is Dropbox's own symlink into its domain; File Provider knows the
    /// item by the resolved spelling.
    #[test]
    fn a_home_symlink_into_a_domain_resolves_through_it() {
        let links = |path: &Path| {
            (path == Path::new("/Users/me/Dropbox")).then(|| PathBuf::from("/Users/me/Library/CloudStorage/Dropbox"))
        };
        let paths = vec![
            PathBuf::from("/Users/me/Dropbox/sub/a.pdf"),
            PathBuf::from("/Users/me/Dropbox"),
        ];
        assert_eq!(
            domain_for_paths(&roots(), &paths, home(), links),
            Some((
                0,
                vec![
                    PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/sub/a.pdf"),
                    PathBuf::from("/Users/me/Library/CloudStorage/Dropbox"),
                ]
            ))
        );
    }

    #[test]
    fn a_relative_home_symlink_resolves_against_its_folder() {
        let links = |path: &Path| {
            (path == Path::new("/Users/me/Dropbox")).then(|| PathBuf::from("Library/CloudStorage/Dropbox"))
        };
        let paths = vec![PathBuf::from("/Users/me/Dropbox/a.pdf")];
        assert_eq!(
            domain_for_paths(&roots(), &paths, home(), links),
            Some((0, vec![PathBuf::from("/Users/me/Library/CloudStorage/Dropbox/a.pdf")]))
        );
    }

    /// A symlink pointing anywhere else ends the walk: no domain, and nothing past it is
    /// read. A link deeper than the bounded depth isn't followed at all.
    #[test]
    fn links_elsewhere_or_too_deep_resolve_to_none() {
        let asked = std::cell::RefCell::new(Vec::new());
        let links = |path: &Path| {
            asked.borrow_mut().push(path.to_path_buf());
            match path.to_str() {
                Some("/Users/me/nas") => Some(PathBuf::from("/Volumes/nas")),
                Some("/Users/me/a/b/Dropbox") => Some(PathBuf::from("/Users/me/Library/CloudStorage/Dropbox")),
                _ => None,
            }
        };
        let elsewhere = vec![PathBuf::from("/Users/me/nas/deep/file")];
        assert_eq!(domain_for_paths(&roots(), &elsewhere, home(), links), None);
        assert_eq!(asked.borrow().as_slice(), [PathBuf::from("/Users/me/nas")]);

        let too_deep = vec![PathBuf::from("/Users/me/a/b/Dropbox/file")];
        assert_eq!(domain_for_paths(&roots(), &too_deep, home(), links), None);
    }

    /// Outside home there's no symlink walk at all, so a path on a mounted share costs no
    /// `readlink` on that share.
    #[test]
    fn a_path_outside_home_never_asks_for_links() {
        let links = |path: &Path| -> Option<PathBuf> { panic!("asked for a link at {path:?}") };
        let paths = vec![PathBuf::from("/Volumes/nas/share/file")];
        assert_eq!(domain_for_paths(&roots(), &paths, home(), links), None);
    }
}
