use super::*;
use crate::card::AboutFact;
use crate::owner::OwnerPlace;
use crate::payload::AppRef;
use crate::test_support::{card, chrome_download, land, land_no_records, local, text};

use AboutAction as A;

fn everywhere() -> ActionCaps {
    ActionCaps {
        file_manager: true,
        removal: Some(RemoveBy::Trash),
        app_folder: None,
        ask_cmdr: false,
    }
}

fn default_app(name: &str) -> AboutFact {
    AboutFact::DefaultApp(Fact::Known(AppRef {
        name: text(name),
        id: None,
    }))
}

fn owned_by_installed_app(path: &str) -> AboutFact {
    AboutFact::Owner(Fact::Known(OwnerClaim::ByBundleId {
        bundle_id: text("com.tinyspeck.slackmacgap"),
        place: OwnerPlace::ApplicationSupport,
        app: InstalledApp::Installed {
            name: text("Slack"),
            path: Some(path.to_string()),
        },
    }))
}

fn file_with(facts: Vec<AboutFact>) -> AboutCard {
    let mut card = card(AboutTarget::File, Some("dmg"), &local());
    for fact in facts {
        land(&mut card, fact);
    }
    land_no_records(&mut card);
    card
}

fn all(actions: &Actions) -> Vec<AboutAction> {
    actions.shown.iter().chain(&actions.more).cloned().collect()
}

#[test]
fn a_downloaded_file_offers_open_reveal_the_page_and_trash_in_that_order() {
    let card = file_with(vec![
        default_app("DiskImageMounter"),
        chrome_download("https://releases.example/tool/Tool.dmg?token=abc"),
    ]);
    let actions = actions(&card, &everywhere());
    assert_eq!(
        actions.shown,
        [
            A::OpenWithDefaultApp {
                app: text("DiskImageMounter")
            },
            A::ShowInFileManager,
            A::OpenDownloadPage {
                host: "releases.example".to_string(),
                url: "https://releases.example/tool/Tool.dmg?token=abc".to_string(),
            },
        ]
    );
    assert_eq!(actions.more, [A::MoveToTrash]);
}

#[test]
fn a_plain_document_shows_its_three_actions_with_trash_last() {
    let actions = actions(&file_with(vec![default_app("TextEdit")]), &everywhere());
    assert_eq!(
        actions.shown,
        [
            A::OpenWithDefaultApp { app: text("TextEdit") },
            A::ShowInFileManager,
            A::MoveToTrash
        ]
    );
    assert!(actions.more.is_empty());
}

#[test]
fn past_the_shown_ones_the_rest_sit_behind_more_in_the_same_fixed_order() {
    let card = file_with(vec![
        default_app("Preview"),
        chrome_download("https://example.org/a.pdf"),
        owned_by_installed_app("/Applications/Slack.app"),
    ]);
    let caps = ActionCaps {
        app_folder: Some("/Users/jane/Library/Application Support/Slack".to_string()),
        ..everywhere()
    };
    let actions = actions(&card, &caps);
    assert_eq!(actions.shown.len(), SHOWN_ACTIONS);
    assert_eq!(
        actions.more,
        [
            A::RevealOwner {
                app: text("Slack"),
                path: "/Applications/Slack.app".to_string(),
            },
            A::GoToAppFolder {
                app: Some(text("Slack")),
                path: "/Users/jane/Library/Application Support/Slack".to_string(),
            },
            A::MoveToTrash,
        ]
    );
}

#[test]
fn removal_is_never_first_and_alone_it_waits_behind_more() {
    // A phone's file: no file manager, no default app, nothing but delete.
    let card = file_with(Vec::new());
    let caps = ActionCaps {
        file_manager: false,
        removal: Some(RemoveBy::Delete),
        app_folder: None,
        ask_cmdr: false,
    };
    let alone = actions(&card, &caps);
    assert!(alone.shown.is_empty());
    assert_eq!(alone.more, [A::Delete]);

    for caps in [everywhere(), caps] {
        for facts in [vec![], vec![default_app("TextEdit")]] {
            let offered = actions(&file_with(facts), &caps);
            assert!(
                !matches!(offered.shown.first(), Some(A::MoveToTrash | A::Delete)),
                "{offered:?}"
            );
        }
    }
}

#[test]
fn a_volume_that_cant_remove_or_show_offers_neither() {
    let actions = actions(&file_with(Vec::new()), &ActionCaps::none());
    assert!(all(&actions).is_empty());
}

#[test]
fn only_a_parsed_http_or_https_address_becomes_an_open_action() {
    for refused in [
        "smb://nas.example/share",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,<b>hi</b>",
        "ssh://host.example",
        "x-hostile-app://run?cmd=1",
        "https://apple.com@evil.example/",
        "https://user:pass@evil.example/",
        "https://192.168.1.10/a.dmg",
        "https://[::1]/a.dmg",
        "not a url",
    ] {
        assert_eq!(web_page(refused), None, "{refused}");
    }
    // Written back out by the parser: lowercased host, an IDN as punycode, so a
    // look-alike can't pass for another site on the button.
    assert_eq!(
        web_page("HTTPS://Exämple.ORG/Path?q=1#frag"),
        Some(A::OpenDownloadPage {
            host: "xn--exmple-cua.org".to_string(),
            url: "https://xn--exmple-cua.org/Path?q=1#frag".to_string(),
        })
    );
}

#[test]
fn the_page_it_came_from_is_opened_never_the_files_own_address() {
    let mut record = Vec::new();
    plist::Value::Array(vec![
        plist::Value::String("https://cdn.example.net/f/Tool.dmg?sig=abc".to_owned()),
        plist::Value::String("https://tool.example/download".to_owned()),
    ])
    .to_writer_binary(&mut record)
    .expect("a plist value writes to memory");
    let download = AboutFact::Download(crate::provenance::download_fact(Some(&record), None));
    let offered = all(&actions(&file_with(vec![download]), &everywhere()));
    assert!(offered.contains(&A::OpenDownloadPage {
        host: "tool.example".to_string(),
        url: "https://tool.example/download".to_string(),
    }));
}

#[test]
fn a_download_record_that_isnt_a_web_address_has_no_page_to_open() {
    for source in [
        "smb://nas.example/share/a.dmg",
        "file:///Users/jane/a.dmg",
        "javascript:x",
    ] {
        let card = file_with(vec![chrome_download(source)]);
        assert!(
            !all(&actions(&card, &everywhere()))
                .iter()
                .any(|action| matches!(action, A::OpenDownloadPage { .. })),
            "{source}"
        );
    }
}

#[test]
fn the_owner_is_revealed_only_where_cmdr_knows_the_installed_apps_path() {
    let left_over = AboutFact::Owner(Fact::Known(OwnerClaim::ByBundleId {
        bundle_id: text("com.example.gone"),
        place: OwnerPlace::Containers,
        app: InstalledApp::NotInstalled,
    }));
    let pathless = AboutFact::Owner(Fact::Known(OwnerClaim::ByName {
        app: text("Slack"),
        place: OwnerPlace::ApplicationSupport,
        app_path: None,
    }));
    for owner in [left_over, pathless] {
        let card = file_with(vec![owner]);
        assert!(
            !all(&actions(&card, &everywhere()))
                .iter()
                .any(|action| matches!(action, A::RevealOwner { .. }))
        );
    }
}

#[test]
fn an_app_bundle_offers_no_open_and_a_folder_no_download_page() {
    let mut app = card(AboutTarget::AppBundle, None, &local());
    land_no_records(&mut app);
    assert_eq!(
        all(&actions(&app, &everywhere())),
        [A::ShowInFileManager, A::MoveToTrash]
    );

    let mut folder = card(AboutTarget::Folder, None, &local());
    land_no_records(&mut folder);
    assert_eq!(
        all(&actions(&folder, &everywhere())),
        [A::ShowInFileManager, A::MoveToTrash]
    );
}

#[test]
fn the_same_facts_give_the_same_actions() {
    let facts = || vec![default_app("Preview"), chrome_download("https://example.org/a.pdf")];
    assert_eq!(
        actions(&file_with(facts()), &everywhere()),
        actions(&file_with(facts()), &everywhere())
    );
}

#[test]
fn ask_cmdr_comes_after_the_facts_own_next_steps_and_before_removal_only_when_it_is_on() {
    let on = ActionCaps {
        ask_cmdr: true,
        ..everywhere()
    };
    let document = all(&actions(&file_with(vec![default_app("TextEdit")]), &on));
    assert_eq!(
        document,
        [
            A::OpenWithDefaultApp { app: text("TextEdit") },
            A::ShowInFileManager,
            A::ContinueInAskCmdr,
            A::MoveToTrash
        ]
    );
    let mut folder = card(AboutTarget::Folder, None, &local());
    land_no_records(&mut folder);
    assert_eq!(
        all(&actions(&folder, &on)),
        [A::ShowInFileManager, A::ContinueInAskCmdr, A::MoveToTrash]
    );

    // Off: the same facts, no Ask Cmdr.
    assert!(!all(&actions(&file_with(vec![default_app("TextEdit")]), &everywhere())).contains(&A::ContinueInAskCmdr));
}
