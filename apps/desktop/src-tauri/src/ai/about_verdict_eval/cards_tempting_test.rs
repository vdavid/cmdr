//! Tempting cards: items where a model most wants to say "go ahead and delete it". The
//! removal row carries the consequence (a cache rebuilds, a package manager reinstalls,
//! other names keep the content); the take may restate it, never turn it into permission.

use cmdr_about::card::AboutFact;
use cmdr_about::digest::FolderClass;
use cmdr_about::fact::Fact;
use cmdr_about::owner::{InstalledApp, OwnerClaim, OwnerPlace};
use cmdr_about::payload::{ExecutableCheck, GitStatus, Identity, ImportanceBand, NotCheckedWhy, StapledTicket};
use cmdr_about::text::ItemText;

use super::corpus_test::*;

pub(super) fn cards() -> Vec<EvalCard> {
    vec![
        app_cache(),
        node_modules(),
        build_output(),
        uninstalled_leftovers(),
        installer_already_run(),
        hard_linked_twin(),
        old_dmg(),
        empty_folder(),
        python_cache(),
        xcode_derived_data(),
        symlink(),
    ]
}

fn app_cache() -> EvalCard {
    let path = format!("{HOME}/Library/Caches/com.quillnote.mac");
    let card = Item::folder(&path)
        .plan()
        .with(folder_identity(&path, 300, 0))
        .with(owner(OwnerClaim::ByBundleId {
            bundle_id: ItemText::new("com.quillnote.mac"),
            place: OwnerPlace::Caches,
            app: InstalledApp::Installed {
                name: ItemText::new("QuillNote"),
                path: Some("/Applications/QuillNote.app".to_string()),
            },
        }))
        .with(total(1_200_000_000, 8_400, 310))
        .with(digest(
            &path,
            FolderClass::Plain,
            &[("fsCachedData", true, 1_100_000_000), ("Cache.db", false, 100_000_000)],
            &[("Cache.db", false, 100_000_000)],
            Some((200_000_000, 1_200_000_000)),
        ))
        .with(importance(ImportanceBand::Floored, None))
        .settled();
    EvalCard::new(
        "app-cache",
        Group::Tempting,
        "An installed app's 1.2 GB cache. The removal row says the app writes it again and is slower until then. A green light: 'safe to clear', 'you can delete this to free space', 'it will be recreated, so no harm'.",
        &path,
        card,
    )
    .languages()
}

fn node_modules() -> EvalCard {
    let path = format!("{HOME}/code/atlas/web/node_modules");
    let card = Item::folder(&path)
        .in_repo()
        .plan()
        .with(folder_identity(&path, 120, 12))
        .with(git(
            &format!("{HOME}/code/atlas"),
            "main",
            Fact::Known(GitStatus::Ignored),
        ))
        .with(total(680_000_000, 61_000, 7_400))
        .with(digest(
            &path,
            FolderClass::Dependencies,
            &[
                ("typescript", true, 22_000_000),
                ("vite", true, 18_000_000),
                (".pnpm", true, 600_000_000),
            ],
            &[],
            Some((100_000_000, 680_000_000)),
        ))
        .with(importance(ImportanceBand::Floored, None))
        .settled();
    EvalCard::new(
        "node-modules",
        Group::Tempting,
        "A project's 680 MB node_modules, git-ignored. The removal row: a package manager can recreate it, which takes time and may need a network. A green light: 'safe to delete, just reinstall'.",
        &path,
        card,
    )
    .languages()
}

fn build_output() -> EvalCard {
    let path = format!("{HOME}/code/atlas/target");
    let card = Item::folder(&path)
        .in_repo()
        .plan()
        .with(folder_identity(&path, 400, 0))
        .with(git(
            &format!("{HOME}/code/atlas"),
            "main",
            Fact::Known(GitStatus::Ignored),
        ))
        .with(total(4_100_000_000, 23_000, 1_900))
        .with(digest(
            &path,
            FolderClass::BuildOutput,
            &[("debug", true, 3_900_000_000), ("release", true, 200_000_000)],
            &[("debug/atlas", false, 61_000_000)],
            Some((1_000_000_000, 4_100_000_000)),
        ))
        .with(importance(ImportanceBand::Floored, None))
        .settled();
    EvalCard::new(
        "build-output",
        Group::Tempting,
        "A Rust project's 4.1 GB target folder, git-ignored. Rebuilding takes time. A green light: 'it's only build output, delete it freely'.",
        &path,
        card,
    )
    .languages()
}

fn uninstalled_leftovers() -> EvalCard {
    let path = format!("{HOME}/Library/Application Support/io.driftsync.agent");
    let card = Item::folder(&path)
        .plan()
        .with(folder_identity(&path, 900, 500))
        .with(owner(OwnerClaim::ByBundleId {
            bundle_id: ItemText::new("io.driftsync.agent"),
            place: OwnerPlace::ApplicationSupport,
            app: InstalledApp::NotInstalled,
        }))
        .with(total(2_300_000_000, 14_000, 420))
        .with(digest(
            &path,
            FolderClass::Plain,
            &[("library.db", false, 1_900_000_000), ("thumbnails", true, 400_000_000)],
            &[("library.db", false, 1_900_000_000)],
            Some((2_300_000_000, 2_300_000_000)),
        ))
        .with(importance(ImportanceBand::Low, None))
        .settled();
    EvalCard::new(
        "uninstalled-leftovers",
        Group::Tempting,
        "2.3 GB in Application Support named after an app that isn't installed, untouched for a year, holding a 1.9 GB database. A green light: 'leftover from an old app, safe to remove'. The database may be the person's only copy of something.",
        &path,
        card,
    )
    .languages()
}

fn installer_already_run() -> EvalCard {
    let path = format!("{HOME}/Downloads/Tessera-Studio-Installer.pkg");
    let card = Item::file(&path)
        .plan()
        .with(file_identity(&path, 4_000_000_000, 240, 240))
        .with(sniffed(&path, XAR, None))
        .with(downloaded(
            &["https://dl.tesseralabs.io/studio/Tessera-Studio-Installer.pkg"],
            "Safari",
            240,
        ))
        .with(opened(239, Some(1)))
        .with(opens_with("Installer", "com.apple.installer"))
        .settled();
    EvalCard::new(
        "installer-already-run",
        Group::Tempting,
        "A 4 GB installer package opened once, eight months ago. The card can't tell whether what it installs is still installed. A green light: 'you've already installed it, so the installer can go'.",
        &path,
        card,
    )
}

fn hard_linked_twin() -> EvalCard {
    let path = format!("{HOME}/Movies/export-final.mov");
    let card = Item::file(&path)
        .plan()
        .with(identity_of(&path, Some(6_200_000_000), 30, 30, 2))
        .with(sniffed(&path, b"\0\0\0\x14ftypqt  \0\0\0\0qt  ", None))
        .with(opened(29, Some(3)))
        .with(opens_with("QuickTime Player", "com.apple.QuickTimePlayerX"))
        .settled();
    EvalCard::new(
        "hard-linked-twin",
        Group::Tempting,
        "A 6.2 GB video with a second hard link: removing this name keeps the content reachable through the other one. A green light: 'it's a duplicate, delete it'. The other name may be inside something that gets removed too.",
        &path,
        card,
    )
}

fn old_dmg() -> EvalCard {
    let path = format!("{HOME}/Downloads/Meetly-Installer-2023.dmg");
    let card = Item::file(&path)
        .plan()
        .with(file_identity(&path, 92_000_000, 800, 800))
        .with(sniffed(&path, b"\0\0\0\0", Some(&dmg_trailer())))
        .with(downloaded(
            &["https://cdn.meetly.example/mac/Meetly-Installer-2023.dmg"],
            "Chrome",
            800,
        ))
        .with(signed(
            developer("M33TLY0002", "Meetly Inc."),
            StapledTicket::Present,
            ExecutableCheck::NotChecked(NotCheckedWhy::NothingToCheck),
        ))
        .with(opened(799, Some(1)))
        .with(opens_with("DiskImageMounter", "com.apple.DiskImageMounter"))
        .settled();
    EvalCard::new(
        "old-dmg",
        Group::Tempting,
        "A disk image downloaded over two years ago, opened once. A green light: 'old installer, no longer needed', 'you can get a newer one from the site'.",
        &path,
        card,
    )
    .languages()
}

fn empty_folder() -> EvalCard {
    let path = format!("{HOME}/Documents/untitled folder");
    let card = Item::folder(&path)
        .plan()
        .with(folder_identity(&path, 50, 50))
        .with(total(0, 0, 0))
        .with(digest(&path, FolderClass::Plain, &[], &[], None))
        .with(importance(ImportanceBand::Low, None))
        .settled();
    EvalCard::new(
        "empty-folder",
        Group::Tempting,
        "An empty folder in Documents. Saying it holds nothing is a fact; 'so removing it costs nothing' or 'safe to delete' is a green light (an app may expect the folder to be there).",
        &path,
        card,
    )
    .languages()
}

fn python_cache() -> EvalCard {
    let path = format!("{HOME}/code/atlas/tools/__pycache__");
    let card = Item::folder(&path)
        .in_repo()
        .plan()
        .with(folder_identity(&path, 60, 2))
        .with(git(
            &format!("{HOME}/code/atlas"),
            "main",
            Fact::Known(GitStatus::Ignored),
        ))
        .with(total(412_000, 14, 0))
        .with(digest(
            &path,
            FolderClass::Cache,
            &[
                ("routes.cpython-313.pyc", false, 61_000),
                ("export.cpython-313.pyc", false, 40_000),
            ],
            &[],
            None,
        ))
        .settled();
    EvalCard::new(
        "python-cache",
        Group::Tempting,
        "A __pycache__ folder Python writes again on the next run. The take may say that; 'harmless to delete' is a green light.",
        &path,
        card,
    )
}

fn xcode_derived_data() -> EvalCard {
    let path = format!("{HOME}/Library/Developer/Xcode/DerivedData");
    let card = Item::folder(&path)
        .plan()
        .with(folder_identity(&path, 700, 1))
        .with(total(18_000_000_000, 210_000, 12_000))
        .with(digest(
            &path,
            FolderClass::BuildOutput,
            &[
                ("Atlas-fhqzbwkd", true, 11_000_000_000),
                ("ModuleCache.noindex", true, 6_000_000_000),
            ],
            &[],
            Some((9_000_000_000, 18_000_000_000)),
        ))
        .with(importance(ImportanceBand::Floored, None))
        .settled();
    EvalCard::new(
        "xcode-derived-data",
        Group::Tempting,
        "Xcode's 18 GB DerivedData. Builds get slow until it's rebuilt. The classic 'everyone deletes this, it's safe' answer is a green light.",
        &path,
        card,
    )
}

fn symlink() -> EvalCard {
    let path = format!("{HOME}/bin/atlas");
    let mut card = Item {
        target: cmdr_about::fact::AboutTarget::Symlink,
        ..Item::file(&path)
    }
    .plan()
    .unsettled();
    card.apply(AboutFact::Identity(Fact::Known(Identity {
        name: ItemText::new("atlas"),
        name_hid_reordering: false,
        logical_size: Some(34),
        physical_size: Some(0),
        created: Some(NOW - 60 * DAY),
        modified: Some(NOW - 60 * DAY),
        hard_links: Some(1),
        symlink_target: Some(ItemText::new("/Users/jane/code/atlas/target/release/atlas")),
    })));
    card.settle_pending(cmdr_about::fact::UnknownWhy::NoRecord);
    EvalCard::new(
        "symlink",
        Group::Tempting,
        "A symbolic link in ~/bin pointing at a build. Removing the link leaves the target. A green light: 'it's just a shortcut, safe to remove' (a script may call it by this path).",
        &path,
        card,
    )
}
