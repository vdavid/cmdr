use super::*;

const HOME: &str = "/Users/jane";

fn app(name: &str, id: &str) -> KnownApp {
    KnownApp {
        name: name.to_string(),
        id: Some(id.to_string()),
        other_names: Vec::new(),
        path: Some(app_path(name)),
    }
}

/// The installed apps the tests match against: one per case the rules name.
fn apps() -> Vec<KnownApp> {
    vec![
        app("Commander One", "com.eltima.cmd1"),
        KnownApp {
            other_names: vec!["Slack".to_string()],
            ..app("Slack", "com.tinyspeck.slackmacgap")
        },
        app("Google Chrome", "com.google.Chrome"),
        app("Google Drive", "com.google.drivefs"),
        app("Perplexity", "ai.perplexity.mac"),
        app("Docker", "com.docker.docker"),
        app("Microsoft Word", "com.microsoft.Word"),
        app("Microsoft Excel", "com.microsoft.Excel"),
    ]
}

fn no_receipts() -> Result<Vec<String>, UnknownWhy> {
    Ok(Vec::new())
}

/// `question` answered against [`apps`], with nothing installed elsewhere.
fn answer(question: &OwnerQuestion) -> Fact<OwnerClaim> {
    answer_with(question, &|_| None, &no_receipts)
}

fn answer_with(
    question: &OwnerQuestion,
    elsewhere: &dyn Fn(&str) -> Option<KnownApp>,
    receipts: &dyn Fn() -> Result<Vec<String>, UnknownWhy>,
) -> Fact<OwnerClaim> {
    question.answer(&OwnerLookups {
        installed_apps: &apps,
        app_with_id: elsewhere,
        receipts,
    })
}

fn owner_of(path: &str) -> Fact<OwnerClaim> {
    answer(&OwnerQuestion::of(path, HOME))
}

/// Where the test apps sit.
fn app_path(name: &str) -> String {
    format!("/Applications/{name}.app")
}

fn by_id(id: &str, place: OwnerPlace, app: Option<&str>) -> Fact<OwnerClaim> {
    Fact::Known(OwnerClaim::ByBundleId {
        bundle_id: ItemText::new(id),
        place,
        app: match app {
            Some(name) => InstalledApp::Installed {
                name: ItemText::new(name),
                path: Some(app_path(name)),
            },
            None => InstalledApp::NotInstalled,
        },
    })
}

fn probably(app: &str, place: OwnerPlace) -> Fact<OwnerClaim> {
    Fact::Known(OwnerClaim::ByName {
        app: ItemText::new(app),
        place,
        app_path: Some(app_path(app)),
    })
}

// ── What the path says ──────────────────────────────────────────────────────

#[test]
fn each_library_folder_names_its_owner_by_id() {
    let cases = [
        ("Library/Containers/com.eltima.cmd1", OwnerPlace::Containers),
        (
            "Library/Application Support/com.eltima.cmd1",
            OwnerPlace::ApplicationSupport,
        ),
        (
            "Library/Application Scripts/com.eltima.cmd1",
            OwnerPlace::ApplicationScripts,
        ),
        ("Library/Caches/com.eltima.cmd1", OwnerPlace::Caches),
        ("Library/HTTPStorages/com.eltima.cmd1", OwnerPlace::WebData),
        (
            "Library/HTTPStorages/com.eltima.cmd1.binarycookies",
            OwnerPlace::WebData,
        ),
        ("Library/WebKit/com.eltima.cmd1", OwnerPlace::WebData),
        (
            "Library/Saved Application State/com.eltima.cmd1.savedState",
            OwnerPlace::SavedState,
        ),
        ("Library/Logs/com.eltima.cmd1", OwnerPlace::Logs),
        ("Library/Preferences/com.eltima.cmd1.plist", OwnerPlace::Preferences),
    ];
    for (relative, place) in cases {
        assert_eq!(
            OwnerQuestion::of(&format!("{HOME}/{relative}"), HOME),
            OwnerQuestion::BundleId {
                id: "com.eltima.cmd1".to_string(),
                place
            },
            "{relative}"
        );
    }
}

#[test]
fn anything_deeper_in_an_apps_folder_belongs_to_the_same_app() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Containers/com.eltima.cmd1/Data/Library/x.db")),
        by_id("com.eltima.cmd1", OwnerPlace::Containers, Some("Commander One"))
    );
}

#[test]
fn group_containers_are_named_by_a_team_or_a_group_prefix() {
    let group = |name: &str| OwnerQuestion::of(&format!("{HOME}/Library/Group Containers/{name}"), HOME);
    let id = |id: &str| OwnerQuestion::BundleId {
        id: id.to_string(),
        place: OwnerPlace::GroupContainers,
    };
    assert_eq!(group("group.com.docker"), id("com.docker"));
    assert_eq!(group("UBF8T346G9.Office"), id("Office"));
    assert_eq!(group("2BUA8C4S2C.com.agilebits"), id("com.agilebits"));
}

#[test]
fn a_library_folder_itself_or_a_plain_name_outside_application_support_claims_nothing() {
    for relative in [
        "Library",
        "Library/Containers",
        "Library/Application Support",
        "Library/Caches/Homebrew",
        "Library/Preferences/loginwindow.plist",
        "Library/Preferences/ByHost",
        "Library/Mail",
        "Documents/notes.txt",
        ".npm",
        ".cargo/bin/rg",
    ] {
        assert_eq!(
            OwnerQuestion::of(&format!("{HOME}/{relative}"), HOME),
            OwnerQuestion::Nothing,
            "{relative}"
        );
    }
}

#[test]
fn an_application_support_folder_with_a_plain_name_is_asked_by_name() {
    assert_eq!(
        OwnerQuestion::of(&format!("{HOME}/Library/Application Support/Slack/Cache"), HOME),
        OwnerQuestion::Name {
            name: "Slack".to_string(),
            place: OwnerPlace::ApplicationSupport,
            in_library: true
        }
    );
}

#[test]
fn outside_the_home_folder_installers_receipts_answer() {
    for path in [
        "/Library/Application Support/Apple",
        "/usr/local/bin/tool",
        "/opt/thing",
    ] {
        assert_eq!(OwnerQuestion::of(path, HOME), OwnerQuestion::Receipt, "{path}");
    }
    for path in ["/Users/Shared/x", "/Volumes/Naspolya/x", "/tmp/x", "/Libraryish/x"] {
        assert_eq!(OwnerQuestion::of(path, HOME), OwnerQuestion::Nothing, "{path}");
    }
}

#[test]
fn a_homebrew_cellar_or_caskroom_answers_by_its_path() {
    let formula = Fact::Known(OwnerClaim::Homebrew {
        name: ItemText::new("ripgrep"),
        kind: HomebrewKind::Formula,
    });
    assert_eq!(owner_of("/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg"), formula);
    assert_eq!(owner_of("/usr/local/Cellar/ripgrep"), formula);
    assert_eq!(owner_of("/home/linuxbrew/.linuxbrew/Cellar/ripgrep/14.1.1"), formula);
    assert_eq!(
        owner_of("/opt/homebrew/Caskroom/firefox/133.0"),
        Fact::Known(OwnerClaim::Homebrew {
            name: ItemText::new("firefox"),
            kind: HomebrewKind::Cask,
        })
    );
    // The cellar itself is no one formula's, so the receipts are asked.
    assert_eq!(OwnerQuestion::of("/opt/homebrew/Cellar", HOME), OwnerQuestion::Receipt);
}

#[test]
fn anything_inside_an_app_bundle_is_that_apps_and_the_bundle_itself_isnt_inside() {
    assert_eq!(
        owner_of("/Applications/Slack.app/Contents/Resources/app.asar"),
        Fact::Known(OwnerClaim::InsideApp {
            app: ItemText::new("Slack"),
            app_path: "/Applications/Slack.app".to_string(),
        })
    );
    assert_eq!(
        owner_of("/Users/jane/Applications/Tools/Big Tool.APP/Contents/MacOS/tool"),
        Fact::Known(OwnerClaim::InsideApp {
            app: ItemText::new("Big Tool"),
            app_path: "/Users/jane/Applications/Tools/Big Tool.APP".to_string(),
        })
    );
    assert_eq!(
        OwnerQuestion::of("/Applications/Slack.app", HOME),
        OwnerQuestion::Nothing
    );
}

#[test]
fn linux_names_its_app_folders_by_name_and_its_sandboxes_by_id() {
    let home = "/home/jane";
    assert_eq!(
        OwnerQuestion::of("/home/jane/.config/google-chrome/Default", home),
        OwnerQuestion::Name {
            name: "google-chrome".to_string(),
            place: OwnerPlace::Preferences,
            in_library: false
        }
    );
    assert_eq!(
        OwnerQuestion::of("/home/jane/.local/share/Steam", home),
        OwnerQuestion::Name {
            name: "Steam".to_string(),
            place: OwnerPlace::ApplicationSupport,
            in_library: false
        }
    );
    assert_eq!(
        OwnerQuestion::of("/home/jane/.var/app/org.mozilla.firefox/cache", home),
        OwnerQuestion::BundleId {
            id: "org.mozilla.firefox".to_string(),
            place: OwnerPlace::SandboxHome
        }
    );
    assert_eq!(
        OwnerQuestion::of("/home/jane/snap/firefox/common", home),
        OwnerQuestion::BundleId {
            id: "firefox".to_string(),
            place: OwnerPlace::SandboxHome
        }
    );
}

// ── Settling it ─────────────────────────────────────────────────────────────

#[test]
fn a_bundle_id_resolves_to_its_installed_app() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Application Support/com.eltima.cmd1")),
        by_id("com.eltima.cmd1", OwnerPlace::ApplicationSupport, Some("Commander One"))
    );
}

#[test]
fn ids_match_ignoring_case_as_launchservices_does() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Caches/com.google.chrome")),
        by_id("com.google.chrome", OwnerPlace::Caches, Some("Google Chrome"))
    );
}

#[test]
fn an_apps_extension_or_helper_belongs_to_the_app_and_a_lookalike_id_doesnt() {
    assert_eq!(
        owner_of(&format!(
            "{HOME}/Library/Containers/ai.perplexity.mac.PerplexityRichNotification"
        )),
        by_id(
            "ai.perplexity.mac.PerplexityRichNotification",
            OwnerPlace::Containers,
            Some("Perplexity")
        )
    );
    // `ai.perplexity.macintosh` isn't `ai.perplexity.mac`'s: a prefix counts only at a dot.
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Containers/ai.perplexity.macintosh")),
        by_id("ai.perplexity.macintosh", OwnerPlace::Containers, None)
    );
}

#[test]
fn an_id_no_installed_app_has_is_left_over() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Containers/com.example.gone")),
        by_id("com.example.gone", OwnerPlace::Containers, None)
    );
}

#[test]
fn an_app_installed_outside_the_app_folders_is_installed() {
    let helper = |id: &str| {
        (id == "com.adobe.acc.helper").then(|| KnownApp {
            name: "Creative Cloud".to_string(),
            id: Some(id.to_string()),
            other_names: Vec::new(),
            path: Some(app_path("Creative Cloud")),
        })
    };
    assert_eq!(
        answer_with(
            &OwnerQuestion::of(&format!("{HOME}/Library/Containers/com.adobe.acc.helper"), HOME),
            &helper,
            &no_receipts
        ),
        by_id("com.adobe.acc.helper", OwnerPlace::Containers, Some("Creative Cloud"))
    );
}

/// macOS keeps its own services outside the app folders, so an Apple id nothing
/// resolves is never called left over.
#[test]
fn apples_own_unresolved_id_is_never_left_over() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Containers/com.apple.AppleMediaServicesUI")),
        Fact::unknown(UnknownWhy::LibraryNameMatchesNoApp)
    );
}

#[test]
fn a_group_id_matches_the_apps_that_share_it_and_is_never_left_over() {
    // `com.docker` is extended by exactly one installed app.
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Group Containers/group.com.docker")),
        by_id("com.docker", OwnerPlace::GroupContainers, Some("Docker"))
    );
    // `com.microsoft` by two: no pick.
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Group Containers/UBF8T346G9.com.microsoft")),
        Fact::unknown(UnknownWhy::Ambiguous)
    );
    // A team's group nothing installed extends says nothing about an uninstall.
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Group Containers/UBF8T346G9.Office")),
        Fact::unknown(UnknownWhy::LibraryNameMatchesNoApp)
    );
}

#[test]
fn a_name_one_app_answers_to_is_probable() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Application Support/Slack")),
        probably("Slack", OwnerPlace::ApplicationSupport)
    );
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Application Support/commander one")),
        probably("Commander One", OwnerPlace::ApplicationSupport)
    );
}

#[test]
fn a_vendors_name_is_ambiguous_with_several_of_its_apps_and_probable_with_one() {
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Application Support/Google")),
        Fact::unknown(UnknownWhy::Ambiguous)
    );
    assert_eq!(
        owner_of(&format!("{HOME}/Library/Application Support/Microsoft")),
        Fact::unknown(UnknownWhy::Ambiguous)
    );
    let one_google = |_: &str| None;
    let question = OwnerQuestion::of(&format!("{HOME}/Library/Application Support/Google"), HOME);
    let only_chrome = || vec![app("Google Chrome", "com.google.Chrome")];
    assert_eq!(
        question.answer(&OwnerLookups {
            installed_apps: &only_chrome,
            app_with_id: &one_google,
            receipts: &no_receipts,
        }),
        probably("Google Chrome", OwnerPlace::ApplicationSupport)
    );
}

#[test]
fn a_name_no_app_answers_to_is_notable_in_the_library_and_routine_elsewhere() {
    let in_library = owner_of(&format!("{HOME}/Library/Application Support/Anki2"));
    assert_eq!(in_library, Fact::unknown(UnknownWhy::LibraryNameMatchesNoApp));
    let elsewhere = owner_of(&format!("{HOME}/.config/gtk-3.0"));
    assert_eq!(elsewhere, Fact::unknown(UnknownWhy::NoInstalledAppMatches));
}

#[test]
fn receipts_name_their_installer_and_apples_is_part_of_macos() {
    let receipt = OwnerQuestion::Receipt;
    let one = || Ok(vec!["com.example.driver".to_string()]);
    assert_eq!(
        answer_with(&receipt, &|_| None, &one),
        Fact::Known(OwnerClaim::InstalledBy {
            pkg_id: ItemText::new("com.example.driver"),
            part_of_os: false,
        })
    );
    let apples = || {
        Ok(vec![
            "com.apple.files.data-template".to_string(),
            "com.apple.pkg.Core".to_string(),
            "com.apple.files.data-template".to_string(),
        ])
    };
    assert_eq!(
        answer_with(&receipt, &|_| None, &apples),
        Fact::Known(OwnerClaim::InstalledBy {
            pkg_id: ItemText::new("com.apple.files.data-template"),
            part_of_os: true,
        })
    );
    let two_vendors = || Ok(vec!["com.example.a".to_string(), "com.other.b".to_string()]);
    assert_eq!(
        answer_with(&receipt, &|_| None, &two_vendors),
        Fact::unknown(UnknownWhy::Ambiguous)
    );
    assert_eq!(answer(&receipt), Fact::unknown(UnknownWhy::NoRecord));
    let timed_out = || Err(UnknownWhy::TimedOut);
    assert_eq!(
        answer_with(&receipt, &|_| None, &timed_out),
        Fact::unknown(UnknownWhy::TimedOut)
    );
}

#[test]
fn a_path_answered_alone_asks_no_lookup() {
    let never = || -> Vec<KnownApp> { panic!("no app scan for a Homebrew path") };
    let never_id = |_: &str| -> Option<KnownApp> { panic!("no id lookup for a Homebrew path") };
    let never_receipts = || -> Result<Vec<String>, UnknownWhy> { panic!("no receipts for a Homebrew path") };
    let question = OwnerQuestion::of("/opt/homebrew/Cellar/ripgrep", HOME);
    let fact = question.answer(&OwnerLookups {
        installed_apps: &never,
        app_with_id: &never_id,
        receipts: &never_receipts,
    });
    assert!(fact.known().is_some());
}

/// Each claim's wire words, which the card and a model word certainty from.
#[test]
fn a_claim_says_how_cmdr_knows_on_the_wire() {
    let claim = probably("Slack", OwnerPlace::ApplicationSupport);
    assert_eq!(
        serde_json::to_value(&claim).expect("a claim serializes"),
        serde_json::json!({
            "state": "known",
            "value": {
                "type": "byName",
                "app": "Slack",
                "place": "applicationSupport",
                "appPath": "/Applications/Slack.app"
            }
        })
    );
}

#[test]
fn the_app_folder_above_is_the_owned_folder_for_what_sits_inside_it() {
    let above = |path: &str| OwnerQuestion::app_folder_above(path, HOME);
    assert_eq!(
        above(&format!("{HOME}/Library/Application Support/Slack/Cache/data_1")).as_deref(),
        Some("/Users/jane/Library/Application Support/Slack")
    );
    assert_eq!(
        above(&format!("{HOME}/Library/Caches/com.example.tool/blob")).as_deref(),
        Some("/Users/jane/Library/Caches/com.example.tool")
    );
    assert_eq!(
        above(&format!("{HOME}/.config/tool/settings.toml")).as_deref(),
        Some("/Users/jane/.config/tool")
    );
    // The folder itself, the place above it, and anything elsewhere: no folder to go to.
    for path in [
        format!("{HOME}/Library/Application Support/Slack"),
        format!("{HOME}/Library/Application Support"),
        format!("{HOME}/Documents/report.pdf"),
        "/Library/Application Support/Slack/x".to_string(),
    ] {
        assert_eq!(above(&path), None, "{path}");
    }
}
