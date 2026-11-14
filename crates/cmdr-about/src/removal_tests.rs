use super::*;
use crate::card::AboutFact;
use crate::digest::LibraryFolder;
use crate::payload::{Identity, SizeCoverage};
use crate::provenance::Download;
use crate::test_support::{
    card, chrome_download, digest, git, identity, land, land_no_records, local, mounted_share, owned_by_id, phone,
    text, total,
};

use AboutTarget::{AppBundle, ArchiveEntry, File, Folder, Symlink};
use RemovalConsequence as R;

/// The whole row, most painful first.
fn all(card: &AboutCard) -> Vec<RemovalConsequence> {
    let removal = consequences(card);
    assert!(!removal.shown.is_empty(), "the row always says something");
    assert!(removal.shown.len() <= SHOWN);
    removal.shown.into_iter().chain(removal.more).collect()
}

fn settled_file(facts: Vec<AboutFact>) -> AboutCard {
    let mut card = card(File, Some("txt"), &local());
    for fact in facts {
        land(&mut card, fact);
    }
    land_no_records(&mut card);
    card
}

fn settled_folder(facts: Vec<AboutFact>) -> AboutCard {
    let mut card = card(Folder, None, &local());
    for fact in facts {
        land(&mut card, fact);
    }
    land_no_records(&mut card);
    card
}

// ── Honesty about not knowing ───────────────────────────────────────────────

#[test]
fn before_any_fact_lands_cmdr_cant_tell() {
    assert_eq!(all(&card(File, Some("txt"), &local())), [R::CantTell]);
}

#[test]
fn a_plain_document_is_the_persons_own_once_the_evidence_is_in() {
    let card = settled_file(vec![identity("Notes.txt", 2_000)]);
    assert_eq!(all(&card), [R::UserDocument]);
}

#[test]
fn evidence_that_didnt_arrive_is_no_evidence() {
    for why in [
        UnknownWhy::TimedOut,
        UnknownWhy::Unreachable,
        UnknownWhy::PermissionDenied,
        UnknownWhy::Protected,
    ] {
        let mut card = card(File, Some("txt"), &local());
        land(&mut card, identity("Notes.txt", 2_000));
        land(&mut card, AboutFact::Owner(Fact::unknown(why)));
        land_no_records(&mut card);
        assert_eq!(all(&card), [R::CantTell], "{why:?}");
    }
}

#[test]
fn a_volume_that_answers_identity_alone_cant_tell() {
    let mut on_phone = card(File, Some("jpg"), &phone());
    land(&mut on_phone, identity("IMG_0001.jpg", 3_000_000));
    assert_eq!(all(&on_phone), [R::CantTell]);

    // A share reads the download record, and "no record" there still leaves the
    // owner and the repo unasked.
    let mut on_share = card(File, Some("txt"), &mounted_share());
    land(&mut on_share, identity("Notes.txt", 2_000));
    land_no_records(&mut on_share);
    assert_eq!(all(&on_share), [R::UserDocument]);
    land(
        &mut on_share,
        AboutFact::Download(Fact::unknown(UnknownWhy::VolumeCantTell)),
    );
    assert_eq!(all(&on_share), [R::CantTell]);
}

#[test]
fn an_archive_entry_cant_tell() {
    let archive = crate::card::VolumeCaps {
        download_record: true,
        ..crate::card::VolumeCaps::identity_only(text("Photos.zip"))
    };
    let mut entry = card(ArchiveEntry, Some("jpg"), &archive);
    land(&mut entry, identity("a.jpg", 1_000));
    land_no_records(&mut entry);
    assert_eq!(all(&entry), [R::CantTell]);
}

/// The zip was downloaded from a site; its entry wasn't, and nothing says the
/// site still offers that one file: the entry's row never names the site.
#[test]
fn an_archive_entry_never_claims_the_archives_download_as_its_own() {
    let archive = crate::card::VolumeCaps {
        download_record: true,
        ..crate::card::VolumeCaps::identity_only(text("Photos.zip"))
    };
    let mut entry = card(ArchiveEntry, Some("jpg"), &archive);
    land(&mut entry, identity("a.jpg", 1_000));
    let AboutFact::Download(record) = chrome_download("https://example.com/Photos.zip") else {
        unreachable!("chrome_download is a download")
    };
    land(
        &mut entry,
        AboutFact::Download(Download::of_archive(text("Photos.zip"), record)),
    );
    assert_eq!(all(&entry), [R::CantTell]);
}

/// A drive that stopped answering is no evidence the item is the person's own.
#[test]
fn rows_the_volume_never_answered_are_no_evidence() {
    let mut on_share = card(File, Some("txt"), &mounted_share());
    land(&mut on_share, identity("Notes.txt", 2_000));
    on_share.settle_pending(UnknownWhy::VolumeStoppedAnswering);
    assert_eq!(all(&on_share), [R::CantTell]);
}

// ── Git ─────────────────────────────────────────────────────────────────────

/// A repository's `.git` is its history: removing it drops every commit not
/// pushed elsewhere, which outranks anything else the folder says.
#[test]
fn a_repos_own_data_says_it_holds_the_history() {
    let card = settled_folder(vec![digest(FolderClass::VersionControl)]);
    assert_eq!(all(&card)[0], R::HoldsRepoHistory);
    assert!(!all(&card).contains(&R::UserDocument));
}

/// A file inside `.git/commits/<id>/` is git's record of it: that's the one line.
#[test]
fn a_snapshot_from_git_history_says_only_that() {
    let mut snapshot = AboutCard::new(&crate::card::plan(crate::card::PlanInput {
        target: File,
        extension: Some("rs"),
        is_executable: false,
        contents_local: false,
        caps: &crate::card::VolumeCaps::identity_only(text("cmdr")),
        os: &crate::test_support::macos(),
        owner: &crate::owner::OwnerQuestion::Nothing,
        in_a_repo: false,
        git_snapshot: true,
        image_format: false,
    }));
    assert_eq!(all(&snapshot), [R::InGitHistory]);
    land(&mut snapshot, identity("main.rs", 1_000));
    assert_eq!(all(&snapshot), [R::InGitHistory]);
}

#[test]
fn git_says_what_it_can_and_cant_bring_back() {
    let modified = settled_file(vec![git(Fact::Known(GitStatus::Tracked(TrackedState::Modified)))]);
    assert_eq!(all(&modified), [R::UncommittedWork]);

    let untracked = settled_file(vec![git(Fact::Known(GitStatus::Untracked))]);
    assert_eq!(all(&untracked), [R::UntrackedNotInGit]);

    let clean = settled_file(vec![git(Fact::Known(GitStatus::Tracked(TrackedState::Clean)))]);
    assert_eq!(all(&clean), [R::GitCanRestore]);
}

#[test]
fn a_status_git_doesnt_list_is_never_read_as_clean() {
    let unlisted = settled_file(vec![git(Fact::unknown(UnknownWhy::NotInStatus))]);
    assert_eq!(all(&unlisted), [R::UserDocument]);
}

/// Git ignoring something says only that git can't bring it back. A project
/// ignores its `data/` and its secrets as readily as its build output, so an
/// ignored item is never called rebuildable on that alone.
#[test]
fn an_ignored_item_is_one_git_cant_restore_and_never_rebuildable_on_that_alone() {
    let folder = settled_folder(vec![git(Fact::Known(GitStatus::Ignored))]);
    assert_eq!(all(&folder), [R::IgnoredByGit]);
    let file = settled_file(vec![git(Fact::Known(GitStatus::Ignored))]);
    assert_eq!(all(&file), [R::IgnoredByGit]);
}

#[test]
fn an_ignored_cache_says_only_that_it_is_regenerated() {
    let cache = settled_folder(vec![digest(FolderClass::Cache), git(Fact::Known(GitStatus::Ignored))]);
    assert_eq!(all(&cache), [R::RegeneratedCache { app: None }]);
}

// ── Folder classes ──────────────────────────────────────────────────────────

#[test]
fn dependencies_and_build_output_name_what_recreates_them() {
    let node_modules = settled_folder(vec![
        digest(FolderClass::Dependencies),
        git(Fact::Known(GitStatus::Ignored)),
    ]);
    assert_eq!(
        all(&node_modules),
        [R::RebuildableOutput {
            by: Rebuilder::PackageManager
        }],
        "one rebuildable line, by the more specific evidence"
    );
    let target = settled_folder(vec![digest(FolderClass::BuildOutput)]);
    assert_eq!(
        all(&target),
        [R::RebuildableOutput {
            by: Rebuilder::BuildTool
        }]
    );
    let plain = settled_folder(vec![digest(FolderClass::Plain)]);
    assert_eq!(all(&plain), [R::UserDocument]);
}

// ── Owners ──────────────────────────────────────────────────────────────────

#[test]
fn an_installed_apps_cache_is_regenerated_by_it() {
    let cache = settled_folder(vec![owned_by_id(OwnerPlace::Caches, Some("Tool"))]);
    assert_eq!(
        all(&cache),
        [R::RegeneratedCache {
            app: Some(text("Tool"))
        }]
    );

    let unowned = settled_folder(vec![digest(FolderClass::Cache)]);
    assert_eq!(all(&unowned), [R::RegeneratedCache { app: None }]);
}

#[test]
fn an_uninstalled_apps_folder_is_left_over_cache_or_not() {
    for place in [
        OwnerPlace::Caches,
        OwnerPlace::ApplicationSupport,
        OwnerPlace::Containers,
    ] {
        let leftover = settled_folder(vec![owned_by_id(place, None)]);
        assert_eq!(
            all(&leftover),
            [R::LeftoverOfUninstalledApp {
                bundle_id: text("com.example.tool")
            }],
            "nothing regenerates the cache of an app that's gone"
        );
    }
}

#[test]
fn an_installed_apps_data_says_how_sure_the_match_is() {
    let by_id = settled_folder(vec![owned_by_id(OwnerPlace::ApplicationSupport, Some("Tool"))]);
    assert_eq!(
        all(&by_id),
        [R::AppData {
            app: text("Tool"),
            certainty: Certainty::Certain
        }]
    );
    let by_name = settled_folder(vec![AboutFact::Owner(Fact::Known(OwnerClaim::ByName {
        app: text("Slack"),
        place: OwnerPlace::ApplicationSupport,
        app_path: None,
    }))]);
    assert_eq!(
        all(&by_name),
        [R::AppData {
            app: text("Slack"),
            certainty: Certainty::Probable
        }]
    );
}

#[test]
fn an_installers_file_names_its_installer() {
    let receipt = settled_file(vec![AboutFact::Owner(Fact::Known(OwnerClaim::InstalledBy {
        pkg_id: text("com.example.pkg.tool"),
        part_of_os: false,
    }))]);
    assert_eq!(
        all(&receipt),
        [R::InstalledBy {
            installer: Installer::Receipt {
                pkg_id: text("com.example.pkg.tool"),
                part_of_os: false
            }
        }]
    );
    let brewed = settled_folder(vec![AboutFact::Owner(Fact::Known(OwnerClaim::Homebrew {
        name: text("ripgrep"),
        kind: HomebrewKind::Formula,
    }))]);
    assert_eq!(
        all(&brewed),
        [R::InstalledBy {
            installer: Installer::Homebrew {
                name: text("ripgrep"),
                kind: HomebrewKind::Formula
            }
        }]
    );
}

// ── Apps ────────────────────────────────────────────────────────────────────

#[test]
fn removing_an_app_says_what_it_leaves_in_the_library_or_that_cmdr_cant_tell() {
    let mut app = card(AppBundle, Some("app"), &local());
    assert_eq!(
        all(&app),
        [R::RemovesApp {
            data_left: Fact::Pending
        }]
    );

    let data = AppData::of(
        vec![LibraryFolder {
            name: text("com.example.tool"),
            path: "/Users/jane/Library/Application Support/com.example.tool".to_owned(),
            place: OwnerPlace::ApplicationSupport,
            size: Fact::Known(total(2_300_000_000, SizeCoverage::Exact)),
        }],
        Vec::new(),
    );
    land(&mut app, AboutFact::AppData(data.clone()));
    assert_eq!(all(&app), [R::RemovesApp { data_left: data }]);

    land(&mut app, AboutFact::AppData(Fact::unknown(UnknownWhy::NotIndexed)));
    assert_eq!(
        all(&app),
        [R::RemovesApp {
            data_left: Fact::unknown(UnknownWhy::NotIndexed)
        }]
    );

    // On a share, where nothing looks for its data at all.
    let on_share = card(AppBundle, Some("app"), &mounted_share());
    assert_eq!(
        all(&on_share),
        [R::RemovesApp {
            data_left: Fact::unknown(UnknownWhy::VolumeCantTell)
        }]
    );
}

// ── Downloads, links, and names ─────────────────────────────────────────────

#[test]
fn a_recorded_download_names_the_site_and_nothing_more() {
    let downloaded = settled_file(vec![chrome_download(
        "https://downloads.example.com/private/tok3n/Tool.dmg?sig=abc",
    )]);
    assert_eq!(
        all(&downloaded),
        [R::RecordedDownload {
            host: "downloads.example.com".to_owned()
        }]
    );
    // A numeric address names no site, so it names no place to look again.
    let from_address = settled_file(vec![chrome_download("http://192.168.1.20/Tool.dmg")]);
    assert_eq!(all(&from_address), [R::UserDocument]);
}

/// "Nothing Cmdr knows of made this" right above "macOS recorded this as
/// downloaded from…" contradicts itself: a fallback speaks only when no fact
/// names a consequence.
#[test]
fn a_fallback_never_sits_beside_a_real_consequence() {
    let cards = [
        settled_file(vec![chrome_download("https://example.com/Tool.dmg")]),
        settled_file(vec![git(Fact::Known(GitStatus::Untracked))]),
        settled_folder(vec![digest(FolderClass::Dependencies)]),
        settled_file(vec![owned_by_id(OwnerPlace::ApplicationSupport, Some("Tessera"))]),
    ];
    for card in cards {
        let row = all(&card);
        assert!(
            !row.iter().any(|line| matches!(line, R::UserDocument | R::CantTell)),
            "{row:?}"
        );
    }
}

#[test]
fn removing_a_link_leaves_what_it_points_at() {
    let mut link = card(Symlink, None, &local());
    land_no_records(&mut link);
    assert_eq!(all(&link), [R::LinkOnly]);
}

#[test]
fn other_names_keep_the_content_reachable() {
    let twin = settled_file(vec![AboutFact::Identity(Fact::Known(Identity {
        name: text("backup.bin"),
        name_hid: None,
        logical_size: Some(10),
        physical_size: Some(10),
        created: None,
        modified: None,
        hard_links: Some(3),
        symlink_target: None,
    }))]);
    assert_eq!(all(&twin), [R::OtherNamesRemain { others: 2 }]);
}

// ── Order ───────────────────────────────────────────────────────────────────

#[test]
fn consequences_come_most_painful_first_and_two_are_shown() {
    let mut busy = card(Folder, None, &local());
    land(&mut busy, git(Fact::Known(GitStatus::Tracked(TrackedState::Modified))));
    land(&mut busy, owned_by_id(OwnerPlace::ApplicationSupport, Some("Tool")));
    land(&mut busy, digest(FolderClass::BuildOutput));
    land(&mut busy, chrome_download("https://example.com/x.zip"));
    land_no_records(&mut busy);
    let removal = consequences(&busy);
    assert_eq!(
        removal.shown,
        [
            R::UncommittedWork,
            R::AppData {
                app: text("Tool"),
                certainty: Certainty::Certain
            }
        ]
    );
    assert_eq!(
        removal.more,
        [
            R::RecordedDownload {
                host: "example.com".to_owned()
            },
            R::RebuildableOutput {
                by: Rebuilder::BuildTool
            }
        ]
    );
}

/// The rule no surface may break: no consequence says or implies a deletion is
/// safe. A new variant has to be added here, where its wire word is read.
#[test]
fn no_consequence_is_a_green_light() {
    let every = [
        R::UncommittedWork,
        R::HoldsRepoHistory,
        R::UntrackedNotInGit,
        R::IgnoredByGit,
        R::UserDocument,
        R::RemovesApp {
            data_left: Fact::Pending,
        },
        R::AppData {
            app: text("Tool"),
            certainty: Certainty::Certain,
        },
        R::InstalledBy {
            installer: Installer::Homebrew {
                name: text("ripgrep"),
                kind: HomebrewKind::Formula,
            },
        },
        R::LeftoverOfUninstalledApp {
            bundle_id: text("com.example.tool"),
        },
        R::RecordedDownload {
            host: "example.com".to_owned(),
        },
        R::GitCanRestore,
        R::RebuildableOutput {
            by: Rebuilder::BuildTool,
        },
        R::RegeneratedCache { app: None },
        R::OtherNamesRemain { others: 1 },
        R::LinkOnly,
        R::InGitHistory,
        R::CantTell,
    ];
    let ranks: Vec<u8> = every.iter().map(R::rank).collect();
    assert_eq!(
        ranks,
        (0..every.len() as u8).collect::<Vec<_>>(),
        "declared in rank order"
    );
    for consequence in every {
        let wire = serde_json::to_value(&consequence).expect("a consequence serializes");
        let word = wire["type"].as_str().expect("a consequence is tagged").to_lowercase();
        for reassurance in ["safe", "delete", "unneeded", "notneeded", "fine", "ok"] {
            assert!(!word.contains(reassurance), "{word} reads as a green light");
        }
    }
}
