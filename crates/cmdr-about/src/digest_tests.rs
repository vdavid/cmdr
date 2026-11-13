use super::*;
use crate::test_support::{text, total};

const HOME: &str = "/Users/jane";

fn child(name: &str, is_folder: bool, size: Option<u64>) -> SizedChild {
    SizedChild {
        name: text(name),
        path: format!("/Users/jane/project/{name}"),
        is_folder,
        size: size.map_or(Fact::unknown(UnknownWhy::NotIndexed), |bytes| {
            Fact::Known(total(bytes, SizeCoverage::Exact))
        }),
    }
}

fn names(children: &[SizedChild]) -> Vec<&str> {
    children.iter().map(|child| child.name.as_str()).collect()
}

fn no_files() -> Fact<Vec<SizedChild>> {
    Fact::unknown(UnknownWhy::NotIndexed)
}

fn no_share() -> Fact<ByteShare> {
    Fact::unknown(UnknownWhy::NotIndexed)
}

// ── The digest ──────────────────────────────────────────────────────────────

#[test]
fn the_biggest_children_lead_and_one_with_no_size_comes_last() {
    let mut children: Vec<SizedChild> = (0..10)
        .map(|n| child(&format!("file-{n}.txt"), false, Some(n * 1_000)))
        .collect();
    children.push(child("unsized", true, None));
    children.push(child("huge", true, Some(5_000_000)));
    let digest = FolderDigest::of(children, no_files(), no_share(), FolderClass::Plain);

    assert_eq!(digest.direct_children, 12);
    assert_eq!(digest.biggest_children.len(), SHOWN_CHILDREN);
    assert_eq!(
        names(&digest.biggest_children)[..3],
        ["huge", "file-9.txt", "file-8.txt"]
    );
    assert!(!names(&digest.biggest_children).contains(&"unsized"));
}

#[test]
fn an_unsized_child_still_shows_when_there_is_room() {
    let digest = FolderDigest::of(
        vec![child("a", true, None), child("b.txt", false, Some(10))],
        no_files(),
        no_share(),
        FolderClass::Plain,
    );
    assert_eq!(names(&digest.biggest_children), ["b.txt", "a"]);
}

#[test]
fn an_empty_folder_says_so_with_zero_children() {
    let digest = FolderDigest::of(Vec::new(), Fact::Known(Vec::new()), no_share(), FolderClass::Plain);
    assert_eq!(digest.direct_children, 0);
    assert!(digest.biggest_children.is_empty());
    assert!(digest.kind_mix.is_empty());
}

#[test]
fn the_largest_files_keep_their_coverage_and_are_cut_to_the_shown_few() {
    let files: Vec<SizedChild> = (0..12)
        .map(|n| child(&format!("deep-{n}.mov"), false, Some(n * 10)))
        .collect();
    let digest = FolderDigest::of(Vec::new(), Fact::Known(files), no_share(), FolderClass::Plain);
    let largest = digest.largest_files.known().expect("known");
    assert_eq!(largest.len(), SHOWN_CHILDREN);
    assert_eq!(largest[0].name.as_str(), "deep-11.mov");

    let unloaded = FolderDigest::of(Vec::new(), no_files(), no_share(), FolderClass::Plain);
    assert_eq!(unloaded.largest_files, Fact::unknown(UnknownWhy::NotIndexed));
}

#[test]
fn the_kind_mix_counts_every_child_most_common_first() {
    let children = vec![
        child("a.jpg", false, Some(1)),
        child("b.HEIC", false, Some(1)),
        child("c.jpg", false, Some(1)),
        child("notes.md", false, Some(1)),
        child("src", true, None),
        child("main.rs", false, Some(1)),
    ];
    let digest = FolderDigest::of(children, no_files(), no_share(), FolderClass::Plain);
    assert_eq!(
        digest.kind_mix,
        [
            KindCount {
                kind: ChildKind::Image,
                count: 3
            },
            KindCount {
                kind: ChildKind::Folder,
                count: 1
            },
            KindCount {
                kind: ChildKind::Document,
                count: 1
            },
            KindCount {
                kind: ChildKind::Code,
                count: 1
            },
        ]
    );
}

#[test]
fn a_childs_kind_comes_from_its_name_alone() {
    let cases = [
        ("Holiday.MOV", false, ChildKind::Video),
        ("song.flac", false, ChildKind::Audio),
        ("report.pdf", false, ChildKind::Document),
        ("budget.numbers", false, ChildKind::Document),
        ("backup.zip", false, ChildKind::Archive),
        ("Tool.dmg", false, ChildKind::Archive),
        ("index.ts", false, ChildKind::Code),
        ("run.sh", false, ChildKind::Code),
        ("libfoo.dylib", false, ChildKind::App),
        ("Makefile", false, ChildKind::Other),
        (".env", false, ChildKind::Other),
        ("Font.ttf", false, ChildKind::Other),
        ("Safari.app", true, ChildKind::App),
        ("photos.2024", true, ChildKind::Folder),
    ];
    for (name, is_folder, kind) in cases {
        assert_eq!(ChildKind::of_name(name, is_folder), kind, "{name}");
    }
}

// ── The folder's class ──────────────────────────────────────────────────────

#[test]
fn a_telling_name_is_enough_wherever_it_sits() {
    let nothing_beside = |_: &str| false;
    assert_eq!(
        FolderClass::of("node_modules", &nothing_beside),
        FolderClass::Dependencies
    );
    assert_eq!(FolderClass::of(".git", &nothing_beside), FolderClass::VersionControl);
    assert_eq!(FolderClass::of("__pycache__", &nothing_beside), FolderClass::Cache);
    assert_eq!(FolderClass::of(".next", &nothing_beside), FolderClass::BuildOutput);
    assert_eq!(FolderClass::of("Photos", &nothing_beside), FolderClass::Plain);
}

/// A person names their own folders `build` and `target` too, and "a build tool
/// makes it again" about one of those invites deleting the person's work.
#[test]
fn a_common_name_is_build_output_only_beside_its_tools_project_file() {
    let nothing_beside = |_: &str| false;
    assert_eq!(FolderClass::of("build", &nothing_beside), FolderClass::Plain);
    assert_eq!(FolderClass::of("target", &nothing_beside), FolderClass::Plain);
    assert_eq!(FolderClass::of("venv", &nothing_beside), FolderClass::Plain);

    let cargo = |name: &str| name == "Cargo.toml";
    assert_eq!(FolderClass::of("target", &cargo), FolderClass::BuildOutput);
    // A Rust project's `dist` is nobody's output Cmdr knows of.
    assert_eq!(FolderClass::of("dist", &cargo), FolderClass::Plain);

    let node = |name: &str| name == "package.json";
    assert_eq!(FolderClass::of("dist", &node), FolderClass::BuildOutput);
    assert_eq!(FolderClass::of("Build", &node), FolderClass::BuildOutput);

    let python = |name: &str| name == "requirements.txt";
    assert_eq!(FolderClass::of(".venv", &python), FolderClass::Dependencies);
}

#[test]
fn a_telling_name_never_asks_about_its_neighbors() {
    let asked = std::cell::Cell::new(false);
    let beside = |_: &str| {
        asked.set(true);
        false
    };
    FolderClass::of("node_modules", &beside);
    FolderClass::of("Photos", &beside);
    assert!(!asked.get(), "only a name that needs a marker looks for one");
}

// ── Untouched for a year ────────────────────────────────────────────────────

#[test]
fn the_untouched_share_is_of_the_folders_own_total() {
    let folder = Fact::Known(total(1_000, SizeCoverage::Exact));
    assert_eq!(
        ByteShare::unchanged(400, true, &folder),
        Fact::Known(ByteShare {
            bytes: 400,
            of: 1_000,
            at_least: false
        })
    );
    // Only the largest untouched files were counted: at least that share.
    assert_eq!(
        ByteShare::unchanged(400, false, &folder)
            .known()
            .map(|share| share.at_least),
        Some(true)
    );
    // The index read a moment apart never makes it more than the whole.
    assert_eq!(
        ByteShare::unchanged(5_000, true, &folder)
            .known()
            .map(|share| share.bytes),
        Some(1_000)
    );
    // A stale but complete total still holds one.
    let stale = Fact::Known(total(1_000, SizeCoverage::ExactButStale));
    assert!(ByteShare::unchanged(1, true, &stale).known().is_some());
}

#[test]
fn a_partly_indexed_folder_has_no_honest_share() {
    let partial = Fact::Known(total(1_000, SizeCoverage::LowerBound));
    assert_eq!(
        ByteShare::unchanged(400, true, &partial),
        Fact::unknown(UnknownWhy::NotIndexed)
    );
    assert_eq!(
        ByteShare::unchanged(400, true, &Fact::unknown(UnknownWhy::TimedOut)),
        Fact::unknown(UnknownWhy::TimedOut)
    );
}

// ── An app's data in the Library ────────────────────────────────────────────

fn library(place: &str, name: &str) -> String {
    format!("{HOME}/Library/{place}/{name}")
}

#[test]
fn an_apps_library_folders_are_found_by_its_id() {
    let id = "com.tinyspeck.slackmacgap";
    let cases = [
        (library("Application Support", id), Some(OwnerPlace::ApplicationSupport)),
        (library("Containers", id), Some(OwnerPlace::Containers)),
        (
            library("Containers", "COM.TINYSPECK.SLACKMACGAP"),
            Some(OwnerPlace::Containers),
        ),
        // An extension's container is the app's own.
        (
            library("Containers", "com.tinyspeck.slackmacgap.Share"),
            Some(OwnerPlace::Containers),
        ),
        (library("Caches", id), Some(OwnerPlace::Caches)),
        (library("HTTPStorages", id), Some(OwnerPlace::WebData)),
        (
            library("Saved Application State", "com.tinyspeck.slackmacgap.savedState"),
            Some(OwnerPlace::SavedState),
        ),
        (library("Logs", id), Some(OwnerPlace::Logs)),
        (
            library("Group Containers", "BQR82RBBHL.com.tinyspeck.slackmacgap"),
            Some(OwnerPlace::GroupContainers),
        ),
    ];
    for (path, place) in cases {
        assert_eq!(app_data_place(&path, HOME, id), place, "{path}");
    }
}

#[test]
fn a_name_match_a_vendors_shared_group_or_another_app_is_never_counted() {
    let id = "com.tinyspeck.slackmacgap";
    let never = [
        // By name alone: a guess, not what removing the app leaves.
        library("Application Support", "Slack"),
        // The vendor's shared group: other apps may live off it.
        library("Group Containers", "BQR82RBBHL.com.tinyspeck"),
        // Another app whose id merely starts the same.
        library("Containers", "com.tinyspeck.slackmacgapx"),
        // Preferences are a file of settings, not a folder of data.
        library("Preferences", "com.tinyspeck.slackmacgap.plist"),
        // Outside the Library places, and outside home.
        format!("{HOME}/Documents/{id}"),
        format!("/Library/Application Support/{id}"),
    ];
    for path in never {
        assert_eq!(app_data_place(&path, HOME, id), None, "{path}");
    }
}

fn folder(name: &str, size: Fact<RecursiveSize>) -> LibraryFolder {
    LibraryFolder {
        name: text(name),
        path: library("Application Support", name),
        place: OwnerPlace::ApplicationSupport,
        size,
    }
}

#[test]
fn the_apps_data_adds_up_biggest_first() {
    let data = AppData::of(
        vec![
            folder("small", Fact::Known(total(100, SizeCoverage::Exact))),
            folder("big", Fact::Known(total(2_000, SizeCoverage::Exact))),
        ],
        Vec::new(),
    );
    let data = data.known().expect("every place was looked in");
    assert_eq!(data.folders[0].name.as_str(), "big");
    assert_eq!(data.total_bytes, 2_100);
    assert!(!data.total_is_lower_bound);
}

#[test]
fn a_place_not_looked_in_or_a_partial_size_makes_the_total_a_lower_bound() {
    let unlooked = AppData::of(
        vec![folder("a", Fact::Known(total(100, SizeCoverage::Exact)))],
        vec![OwnerPlace::Caches],
    );
    assert!(unlooked.known().expect("known").total_is_lower_bound);

    let partial = AppData::of(
        vec![folder("a", Fact::Known(total(100, SizeCoverage::LowerBound)))],
        Vec::new(),
    );
    assert!(partial.known().expect("known").total_is_lower_bound);

    let no_size = AppData::of(vec![folder("a", Fact::unknown(UnknownWhy::NotIndexed))], Vec::new());
    let no_size = no_size.known().expect("known");
    assert_eq!(no_size.total_bytes, 0);
    assert!(
        no_size.total_is_lower_bound,
        "a folder of unknown size is never counted as zero"
    );
}

#[test]
fn no_folders_where_every_place_was_looked_in_is_a_real_answer() {
    let none = AppData::of(Vec::new(), Vec::new());
    let none = none.known().expect("known");
    assert!(none.folders.is_empty());
    assert!(!none.total_is_lower_bound);
}

#[test]
fn with_no_place_looked_in_cmdr_cant_tell() {
    let places: Vec<OwnerPlace> = APP_DATA_PLACES.iter().map(|(_, place)| *place).collect();
    assert_eq!(AppData::of(Vec::new(), places), Fact::unknown(UnknownWhy::NotIndexed));
}
