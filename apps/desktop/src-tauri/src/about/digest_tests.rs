//! The digest and app-data rows over a drive index written by the test, read the
//! way the card reads it: through the handle, from the database on disk, with no
//! arena loaded.

use cmdr_about::digest::FolderClass;
use cmdr_about::owner::OwnerPlace;
use cmdr_index::store::{DirStatsById, IndexStore, ROOT_ID};
use rusqlite::Connection;

use super::*;
use crate::test_support::TestDir;

/// A share-shaped volume the host doesn't serve, so its reads come from the
/// database on disk.
const VOLUME: &str = "smb-127-0-0-1-445-aboutfixture-test_user_1";
const ROOT: &str = "/Volumes/aboutfixture";
const NOW: u64 = 1_800_000_000;

struct Fixture {
    conn: Connection,
}

impl Fixture {
    fn new(data: &Path) -> Self {
        let db_path = data.join(format!("index-{VOLUME}.db"));
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(&conn, "volume_path", ROOT).expect("meta");
        Self { conn }
    }

    fn dir(&self, parent: i64, name: &str) -> i64 {
        IndexStore::insert_entry_v2(&self.conn, parent, name, true, false, None, None, Some(NOW), None)
            .expect("insert dir")
    }

    fn file(&self, parent: i64, name: &str, bytes: u64) {
        IndexStore::insert_entry_v2(
            &self.conn,
            parent,
            name,
            false,
            false,
            Some(bytes),
            Some(bytes),
            Some(NOW),
            None,
        )
        .expect("insert file");
    }

    fn listed(&self, ids: &[i64]) {
        IndexStore::mark_dirs_listed(&self.conn, ids, 1).expect("mark listed");
    }

    /// `complete: false` is a subtree with a folder nothing listed: a lower bound.
    fn size(&self, id: i64, bytes: u64, complete: bool) {
        IndexStore::upsert_dir_stats_by_id(
            &self.conn,
            &[DirStatsById {
                entry_id: id,
                recursive_logical_size: bytes,
                recursive_physical_size: bytes,
                recursive_file_count: 3,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: u64::from(complete),
            }],
        )
        .expect("stats");
    }
}

fn place(path: &str) -> IndexPlace {
    IndexPlace {
        index_id: VOLUME.to_string(),
        path: path.to_string(),
        importance: None,
        data_dir: None,
    }
}

fn names(children: &[SizedChild]) -> Vec<&str> {
    children.iter().map(|child| child.name.as_str()).collect()
}

#[test]
fn a_folder_names_its_biggest_children_with_the_indexs_coverage() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("about-digest-big");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    let fx = Fixture::new(&data);
    let project = fx.dir(ROOT_ID, "project");
    let media = fx.dir(project, "media");
    let half_walked = fx.dir(project, "half-walked");
    fx.dir(project, "never-sized");
    fx.file(project, "notes.md", 2_000);
    fx.file(project, "movie.mov", 9_000_000);
    fx.file(project, "package.json", 100);
    fx.listed(&[ROOT_ID, project, media, half_walked]);
    fx.size(project, 30_000_000, true);
    fx.size(media, 20_000_000, true);
    fx.size(half_walked, 1_000_000, false);

    let Fact::Known(digest) = folder_digest_fact(&place(&format!("{ROOT}/project")), NOW) else {
        panic!("the index lists the folder");
    };
    assert_eq!(digest.direct_children, 6);
    assert_eq!(
        names(&digest.biggest_children),
        [
            "media",
            "movie.mov",
            "half-walked",
            "notes.md",
            "package.json",
            "never-sized"
        ]
    );
    let half = &digest.biggest_children[2];
    assert!(half.is_folder);
    assert_eq!(half.path, format!("{ROOT}/project/half-walked"));
    assert_eq!(
        half.size.known().map(|size| size.coverage),
        Some(SizeCoverage::LowerBound),
        "a partly walked child's size is a lower bound, verbatim"
    );
    assert_eq!(digest.biggest_children[5].size, Fact::unknown(UnknownWhy::NotIndexed));
    // No arena in memory: the card says the index can't tell yet, never "no files".
    assert_eq!(digest.largest_files, Fact::unknown(UnknownWhy::NotIndexed));
    assert_eq!(digest.unchanged_for_a_year, Fact::unknown(UnknownWhy::NotIndexed));
    assert_eq!(digest.class, FolderClass::Plain);
}

#[test]
fn an_empty_folder_is_known_and_empty_and_an_unlisted_one_is_not_indexed() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("about-digest-empty");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    let fx = Fixture::new(&data);
    let empty = fx.dir(ROOT_ID, "empty");
    let unlisted = fx.dir(ROOT_ID, "unlisted");
    fx.file(unlisted, "seen-by-a-watcher.txt", 10);
    fx.listed(&[ROOT_ID, empty]);

    let Fact::Known(digest) = folder_digest_fact(&place(&format!("{ROOT}/empty")), NOW) else {
        panic!("an empty listed folder is a known answer");
    };
    assert_eq!(digest.direct_children, 0);
    assert!(digest.biggest_children.is_empty());

    assert_eq!(
        folder_digest_fact(&place(&format!("{ROOT}/unlisted")), NOW),
        Fact::unknown(UnknownWhy::NotIndexed),
        "rows under a folder nothing listed are a lower bound, not its contents"
    );
}

#[test]
fn node_modules_and_a_rust_target_are_classed_from_the_index() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("about-digest-class");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    let fx = Fixture::new(&data);
    let app = fx.dir(ROOT_ID, "app");
    let modules = fx.dir(app, "node_modules");
    let target = fx.dir(app, "target");
    let docs = fx.dir(ROOT_ID, "docs");
    let build = fx.dir(docs, "build");
    fx.file(app, "Cargo.toml", 100);
    fx.listed(&[ROOT_ID, app, modules, target, docs, build]);

    let class = |path: &str| {
        folder_digest_fact(&place(&format!("{ROOT}/{path}")), NOW)
            .known()
            .map(|d| d.class)
    };
    assert_eq!(class("app/node_modules"), Some(FolderClass::Dependencies));
    assert_eq!(class("app/target"), Some(FolderClass::BuildOutput));
    // A `build` folder with no project file beside it is the person's own.
    assert_eq!(class("docs/build"), Some(FolderClass::Plain));
}

#[test]
fn an_apps_library_folders_are_found_by_id_and_a_name_only_vendor_folder_is_not() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("about-digest-app-data");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    let fx = Fixture::new(&data);
    let home = fx.dir(ROOT_ID, "home");
    let library = fx.dir(home, "Library");
    let support = fx.dir(library, "Application Support");
    let caches = fx.dir(library, "Caches");
    let containers = fx.dir(library, "Containers");
    let by_id = fx.dir(support, "com.example.tessera");
    let by_name = fx.dir(support, "Tessera");
    let cache = fx.dir(caches, "com.example.tessera");
    let other_app = fx.dir(caches, "com.example.other");
    fx.dir(containers, "com.example.tessera.ShareExtension");
    fx.listed(&[
        ROOT_ID, home, library, support, caches, containers, by_id, by_name, cache, other_app,
    ]);
    fx.size(by_id, 2_000_000_000, true);
    fx.size(cache, 300_000_000, false);
    fx.size(by_name, 9_000_000_000, true);

    let home_path = format!("{ROOT}/home");
    let Fact::Known(data) = app_data_fact(Some("com.example.tessera"), &home_path, VOLUME) else {
        panic!("three places were looked in");
    };
    let found: Vec<(&str, OwnerPlace)> = data
        .folders
        .iter()
        .map(|folder| (folder.name.as_str(), folder.place))
        .collect();
    assert_eq!(
        found,
        [
            ("com.example.tessera", OwnerPlace::ApplicationSupport),
            ("com.example.tessera", OwnerPlace::Caches),
            ("com.example.tessera.ShareExtension", OwnerPlace::Containers),
        ],
        "found by id, biggest first; `Tessera` by name and another app's cache stay out"
    );
    assert_eq!(data.total_bytes, 2_300_000_000);
    assert!(
        data.total_is_lower_bound,
        "a lower-bound cache and unlisted places say \"at least\""
    );
    assert!(data.not_looked_in.contains(&OwnerPlace::GroupContainers));
    assert!(!data.not_looked_in.contains(&OwnerPlace::Caches));
}

#[test]
fn with_no_library_in_the_index_cmdr_cant_tell_what_an_app_leaves() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("about-digest-no-library");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    let fx = Fixture::new(&data);
    fx.listed(&[ROOT_ID]);
    assert_eq!(
        app_data_fact(Some("com.example.tessera"), &format!("{ROOT}/home"), VOLUME),
        Fact::unknown(UnknownWhy::NotIndexed)
    );
    assert_eq!(
        app_data_fact(None, &format!("{ROOT}/home"), VOLUME),
        Fact::unknown(UnknownWhy::NoRecord)
    );
}
