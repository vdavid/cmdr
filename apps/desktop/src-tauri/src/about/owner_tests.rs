use cmdr_about::owner::{InstalledApp, OwnerPlace};
use cmdr_about::text::ItemText;

use super::*;

const HOME: &str = "/Users/jane";

fn app(name: &str, id: &str) -> KnownApp {
    KnownApp {
        name: name.to_string(),
        id: Some(id.to_string()),
        other_names: Vec::new(),
        path: None,
    }
}

/// A Mac with Commander One, Slack, and two of Google's apps installed.
fn fake_apps() -> Vec<KnownApp> {
    vec![
        app("Commander One", "com.eltima.cmd1"),
        app("Slack", "com.tinyspeck.slackmacgap"),
        app("Google Chrome", "com.google.Chrome"),
        app("Google Drive", "com.google.drivefs"),
    ]
}

fn nothing_elsewhere(_id: &str) -> Option<KnownApp> {
    None
}

fn no_receipts(_path: &str) -> Result<Vec<String>, UnknownWhy> {
    Ok(Vec::new())
}

fn fake_source() -> AppSource {
    AppSource {
        installed: fake_apps,
        with_id: nothing_elsewhere,
        receipts: no_receipts,
    }
}

fn owner_of(relative: &str) -> Fact<OwnerClaim> {
    let path = format!("{HOME}/{relative}");
    owner_fact(&OwnerQuestion::of(&path, HOME), &path, fake_source())
}

#[test]
fn a_bundle_id_folder_resolves_to_its_app_and_says_it_is_installed() {
    assert_eq!(
        owner_of("Library/Application Support/com.eltima.cmd1"),
        Fact::Known(OwnerClaim::ByBundleId {
            bundle_id: ItemText::new("com.eltima.cmd1"),
            place: OwnerPlace::ApplicationSupport,
            app: InstalledApp::Installed {
                name: ItemText::new("Commander One"),
                path: None,
            },
        })
    );
}

#[test]
fn an_app_named_folder_is_probable_and_a_vendors_is_ambiguous() {
    assert_eq!(
        owner_of("Library/Application Support/Slack"),
        Fact::Known(OwnerClaim::ByName {
            app: ItemText::new("Slack"),
            place: OwnerPlace::ApplicationSupport,
            app_path: None,
        })
    );
    assert_eq!(
        owner_of("Library/Application Support/Google"),
        Fact::unknown(UnknownWhy::Ambiguous)
    );
}

#[test]
fn a_receipt_question_asks_the_receipts_for_the_items_own_path() {
    fn receipts_for_fonts(path: &str) -> Result<Vec<String>, UnknownWhy> {
        assert_eq!(path, "/Library/Fonts", "the receipts are asked about the item");
        Ok(vec!["com.apple.files.data-template".to_string()])
    }
    let source = AppSource {
        receipts: receipts_for_fonts,
        ..fake_source()
    };
    assert_eq!(
        owner_fact(&OwnerQuestion::of("/Library/Fonts", HOME), "/Library/Fonts", source),
        Fact::Known(OwnerClaim::InstalledBy {
            pkg_id: ItemText::new("com.apple.files.data-template"),
            part_of_os: true,
        })
    );
}
