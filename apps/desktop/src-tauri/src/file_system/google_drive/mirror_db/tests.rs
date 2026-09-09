//! Mirror-mode resolution, end to end against a real pair of SQLite databases.
//!
//! The fixture builds Drive's own schema (the `CREATE TABLE` statements copied verbatim
//! from a live install, so a column this code reads can't quietly stop existing here
//! while it exists there) over REAL files in a temp tree. Real files mean real inodes,
//! which is the whole proof step — a fake inode would test nothing.

use super::*;
use crate::test_support::TestDir;
use std::fs;

// ── Drive's schema, verbatim ─────────────────────────────────────────
//
// Read off Drive for desktop 130.0 with `sqlite3 … ".schema"`, macOS 26.6.2,
// 2026-09-09. Kept whole rather than trimmed to the columns we read: the `NOT NULL`
// constraints are what force the fixture to insert realistic rows, and a schema drift
// in the real thing should show up as a diff against this text.

const MIRROR_DDL: &str = "\
CREATE TABLE mirror_item (local_stable_id INTEGER PRIMARY KEY, stable_id INTEGER NOT NULL, \
inode INTEGER NOT NULL, volume TEXT NOT NULL, parent_local_stable_id INTEGER, local_filename TEXT, \
cloud_filename TEXT, local_mtime_ms INTEGER, cloud_mtime_ms INTEGER, local_md5_checksum TEXT, \
cloud_md5_checksum TEXT, local_size INTEGER, cloud_size INTEGER, local_type INTEGER NOT NULL, \
cloud_type INTEGER NOT NULL, local_version INTEGER NOT NULL, cloud_version INTEGER NOT NULL, \
storage_policy INTEGER NOT NULL, shared BOOLEAN, read_only BOOLEAN, target_version INTEGER, \
is_root BOOLEAN, UNIQUE (stable_id, parent_local_stable_id), UNIQUE (parent_local_stable_id, local_filename));
CREATE TABLE root_config(root_id INTEGER PRIMARY KEY, root_state INTEGER NOT NULL, local_stable_id INTEGER, \
item_id TEXT, is_my_drive BOOLEAN);
CREATE INDEX local_relations_parent_local_stable_id_idx ON mirror_item (parent_local_stable_id);";

const MIRROR_METADATA_DDL: &str = "\
CREATE TABLE \"items\" (\"stable_id\" INTEGER PRIMARY KEY NOT NULL, \"id\" TEXT UNIQUE NOT NULL, \
\"proto\" BLOB, \"trashed\" BOOLEAN NOT NULL, \"starred\" BOOLEAN NOT NULL, \"is_owner\" BOOLEAN NOT NULL, \
\"mime_type\" TEXT NOT NULL COLLATE NOCASE, \"is_folder\" BOOLEAN NOT NULL, \"modified_date\" INTEGER, \
\"shared_with_me_date\" INTEGER, \"viewed_by_me_date\" INTEGER, \"file_size\" INTEGER, \
\"is_tombstone\" BOOLEAN NOT NULL, \"local_title\" TEXT, \"subscribed\" BOOLEAN NOT NULL, \
\"team_drive_stable_id\" INTEGER, \"inaccessible_inheritance_broken\" BOOLEAN NOT NULL);
CREATE TABLE \"shortcut_details\" (\"shortcut_stable_id\" INTEGER PRIMARY KEY NOT NULL, \
\"target_stable_id\" INTEGER NOT NULL, \"target_mime_type\" TEXT NOT NULL);";

const PDF: &str = "application/pdf";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";

/// A signed-in account: one mirror root on disk plus the two databases describing it.
struct Fixture {
    /// Stands in for `~/Library/Application Support`.
    support: TestDir,
    /// The mirrored files, so their inodes are real.
    tree: TestDir,
    /// The account directory under `support`.
    account: PathBuf,
    /// `local_stable_id` for the next row.
    next_local_id: i64,
}

impl Fixture {
    /// An account with no roots yet.
    fn new(account_id: &str) -> Self {
        let support = TestDir::new("drive-support");
        let tree = TestDir::new("drive-tree");
        let account = support.join(DRIVE_SUPPORT_SUBDIR).join(account_id);
        fs::create_dir_all(&account).expect("account dir");
        exec(&account.join(MIRROR_DB), MIRROR_DDL);
        exec(&account.join(MIRROR_METADATA_DB), MIRROR_METADATA_DDL);
        Self {
            support,
            tree,
            account,
            next_local_id: 1,
        }
    }

    /// Creates the mirror root directory `name` and the rows that make it a root.
    /// Answers its `local_stable_id`.
    fn root(&mut self, name: &str, drive_id: &str) -> i64 {
        let dir = self.tree.join(name);
        fs::create_dir_all(&dir).expect("root dir");
        let local_id = self.insert_item(&dir, -1, name, 0, 1);
        self.mirror(|conn| {
            conn.execute(
                "INSERT INTO root_config (root_id, root_state, local_stable_id, item_id, is_my_drive) \
                 VALUES (1, 1, ?1, ?2, 1)",
                rusqlite::params![local_id, drive_id],
            )
            .expect("insert root_config");
        });
        local_id
    }

    /// Creates a mirrored directory under `parent`, and its row. Answers its
    /// `local_stable_id`.
    fn dir(&mut self, parent: i64, relative: &str, stable_id: i64, drive_id: &str) -> i64 {
        let dir = self.tree.join(relative);
        fs::create_dir_all(&dir).expect("mirrored dir");
        let name = last_component(relative);
        let local_id = self.insert_item(&dir, parent, name, stable_id, 0);
        self.item(stable_id, drive_id, FOLDER_MIME, true);
        local_id
    }

    /// Creates a mirrored file under `parent`, its `mirror_item` row, and its `items`
    /// row. Answers the file's path on disk.
    fn file(&mut self, parent: i64, relative: &str, stable_id: i64, drive_id: &str, mime: &str) -> PathBuf {
        let path = self.tree.join(relative);
        fs::write(&path, b"mirrored bytes").expect("mirrored file");
        let name = last_component(relative);
        self.insert_item(&path, parent, name, stable_id, 0);
        self.item(stable_id, drive_id, mime, false);
        path
    }

    /// One `mirror_item` row, carrying the real inode of `on_disk`.
    fn insert_item(&mut self, on_disk: &Path, parent: i64, name: &str, stable_id: i64, is_root: i64) -> i64 {
        let inode = fs::metadata(on_disk).expect("stat fixture path").ino() as i64;
        let local_id = self.next_local_id;
        self.next_local_id += 1;
        self.mirror(|conn| {
            conn.execute(
                "INSERT INTO mirror_item (local_stable_id, stable_id, inode, volume, \
                 parent_local_stable_id, local_filename, cloud_filename, local_type, cloud_type, \
                 local_version, cloud_version, storage_policy, is_root) \
                 VALUES (?1, ?2, ?3, 'TEST-VOLUME-UUID', ?4, ?5, ?5, 1, 1, 1, 1, 1, ?6)",
                rusqlite::params![local_id, stable_id, inode, parent, name, is_root],
            )
            .expect("insert mirror_item");
        });
        local_id
    }

    /// One `mirror_metadata_sqlite.db` `items` row.
    fn item(&self, stable_id: i64, drive_id: &str, mime: &str, is_folder: bool) {
        self.metadata(|conn| {
            conn.execute(
                "INSERT INTO items (stable_id, id, trashed, starred, is_owner, mime_type, is_folder, \
                 is_tombstone, local_title, subscribed, inaccessible_inheritance_broken) \
                 VALUES (?1, ?2, 0, 0, 1, ?3, ?4, 0, 'x', 0, 0)",
                rusqlite::params![stable_id, drive_id, mime, is_folder],
            )
            .expect("insert items");
        });
    }

    /// The accounts this fixture's support directory yields, as production scans them.
    fn accounts(&self) -> Vec<MirrorAccount> {
        scan_accounts(&self.support.join(DRIVE_SUPPORT_SUBDIR))
    }

    /// What production would answer for `path`, over this fixture's accounts.
    fn resolve(&self, path: &Path) -> Option<MirrorItem> {
        resolve_in(&self.accounts(), path)
    }

    fn mirror(&self, f: impl FnOnce(&Connection)) {
        let conn = crate::sqlite_util::open(&self.account.join(MIRROR_DB)).expect("open mirror db");
        f(&conn);
    }

    fn metadata(&self, f: impl FnOnce(&Connection)) {
        let conn = crate::sqlite_util::open(&self.account.join(MIRROR_METADATA_DB)).expect("open metadata db");
        f(&conn);
    }
}

fn exec(db_path: &Path, ddl: &str) {
    let conn = crate::sqlite_util::open(db_path).expect("create fixture db");
    conn.execute_batch(ddl).expect("apply ddl");
}

fn last_component(relative: &str) -> &str {
    relative.rsplit('/').next().expect("a last component")
}

// ── The headline case ────────────────────────────────────────────────

/// A mirrored PDF: no xattr, no stub, nothing on the file itself. This is the whole
/// point of the module.
#[test]
fn a_mirrored_file_resolves_to_its_drive_id() {
    let mut fixture = Fixture::new("103986087393247194397");
    let root = fixture.root("My Drive", "0AFZXf6SvEaTnUk9PVA");
    let pdf = fixture.file(root, "My Drive/117243020.pdf", 330816, "1BtmyCwXjCCV", PDF);

    assert_eq!(
        fixture.resolve(&pdf),
        Some(MirrorItem {
            id: "1BtmyCwXjCCV".to_string(),
            kind: DriveItemKind::Binary,
        })
    );
}

/// The safety property, stated as a test: two files that share a name in different
/// folders must each resolve to their OWN id. A `WHERE local_title = ?` query would
/// hand back whichever row it hit first, which is an invitation to open someone else's
/// file.
#[test]
fn same_named_files_in_different_folders_resolve_to_their_own_ids() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let taxes = fixture.dir(root, "My Drive/Taxes", 200, "TAXES");
    let medical = fixture.dir(root, "My Drive/Medical", 201, "MEDICAL");
    let a = fixture.file(taxes, "My Drive/Taxes/scan.pdf", 300, "TAXES-SCAN", PDF);
    let b = fixture.file(medical, "My Drive/Medical/scan.pdf", 301, "MEDICAL-SCAN", PDF);

    assert_eq!(fixture.resolve(&a).expect("taxes scan resolves").id, "TAXES-SCAN");
    assert_eq!(fixture.resolve(&b).expect("medical scan resolves").id, "MEDICAL-SCAN");
}

/// Deep nesting is the same walk, one indexed lookup per level.
#[test]
fn a_deeply_nested_file_resolves() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let a = fixture.dir(root, "My Drive/a", 10, "A");
    let b = fixture.dir(a, "My Drive/a/b", 11, "B");
    let c = fixture.dir(b, "My Drive/a/b/c", 12, "C");
    let deep = fixture.file(c, "My Drive/a/b/c/deep.pdf", 13, "DEEP", PDF);

    assert_eq!(fixture.resolve(&deep).expect("deep file resolves").id, "DEEP");
}

// ── Failing closed ───────────────────────────────────────────────────

/// The inode proof. A row found by the right path but describing a DIFFERENT file (the
/// file was replaced since Drive last looked, and Drive hasn't caught up) must produce
/// nothing. Without this check the walk would hand back the previous file's id.
#[test]
fn a_row_whose_inode_isnt_the_files_yields_nothing() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let pdf = fixture.file(root, "My Drive/report.pdf", 300, "STALE", PDF);

    // Replace the file: same path, new inode.
    fs::remove_file(&pdf).expect("remove");
    fs::write(&pdf, b"different bytes entirely").expect("rewrite");

    assert_eq!(fixture.resolve(&pdf), None);
}

/// A path Drive doesn't know, inside a mirror root. The walk runs out of rows.
#[test]
fn a_path_the_database_doesnt_know_yields_nothing() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    fixture.file(root, "My Drive/known.pdf", 300, "KNOWN", PDF);
    let unknown = fixture.tree.join("My Drive/unknown.pdf");
    fs::write(&unknown, b"not in the db").expect("write");

    assert_eq!(fixture.resolve(&unknown), None);
}

/// An ordinary file outside every mirror root. The walk up never finds a root inode.
#[test]
fn a_path_outside_every_mirror_root_yields_nothing() {
    let mut fixture = Fixture::new("acct");
    fixture.root("My Drive", "ROOT");
    let elsewhere = fixture.tree.join("notes.txt");
    fs::write(&elsewhere, b"mine").expect("write");

    assert_eq!(fixture.resolve(&elsewhere), None);
}

/// A trashed item still has a live Drive URL, so nothing but this check stops us
/// offering a link to a file on its way out.
#[test]
fn a_trashed_or_tombstoned_row_yields_nothing() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let trashed = fixture.file(root, "My Drive/gone.pdf", 300, "TRASHED", PDF);
    let tombstoned = fixture.file(root, "My Drive/ghost.pdf", 301, "TOMBSTONED", PDF);
    fixture.metadata(|conn| {
        conn.execute("UPDATE items SET trashed = 1 WHERE stable_id = 300", [])
            .expect("trash");
        conn.execute("UPDATE items SET is_tombstone = 1 WHERE stable_id = 301", [])
            .expect("tombstone");
    });

    assert_eq!(fixture.resolve(&trashed), None);
    assert_eq!(fixture.resolve(&tombstoned), None);
}

/// No mirror pair at all (Drive not installed, or stream mode only): no accounts, no
/// resolution, and no error.
#[test]
fn an_account_without_a_mirror_pair_is_skipped() {
    let support = TestDir::new("drive-support");
    let account = support.join(DRIVE_SUPPORT_SUBDIR).join("acct");
    fs::create_dir_all(&account).expect("account dir");
    // Only the stream-mode database, which this module must never read.
    exec(&account.join("metadata_sqlite_db"), MIRROR_METADATA_DDL);

    assert!(scan_accounts(&support.join(DRIVE_SUPPORT_SUBDIR)).is_empty());
}

/// A support directory that doesn't exist is the common case for anyone without Drive.
#[test]
fn a_missing_support_directory_yields_no_accounts() {
    let support = TestDir::new("drive-support");
    assert!(scan_accounts(&support.join("no/such/place")).is_empty());
}

/// Drive's schema is undocumented and can change with any update. A mirror database
/// whose `mirror_item` has lost a column we read must cost the menu items, not panic.
#[test]
fn a_changed_schema_yields_nothing() {
    let support = TestDir::new("drive-support");
    let account = support.join(DRIVE_SUPPORT_SUBDIR).join("acct");
    fs::create_dir_all(&account).expect("account dir");
    exec(
        &account.join(MIRROR_DB),
        "CREATE TABLE mirror_item (local_stable_id INTEGER PRIMARY KEY); \
         CREATE TABLE root_config (root_id INTEGER PRIMARY KEY);",
    );
    exec(&account.join(MIRROR_METADATA_DB), MIRROR_METADATA_DDL);

    assert!(scan_accounts(&support.join(DRIVE_SUPPORT_SUBDIR)).is_empty());
}

/// A root row `root_config` points at that `mirror_item` doesn't mark as a root means
/// we've misread the schema, so the account contributes nothing.
#[test]
fn a_root_row_that_isnt_marked_a_root_is_ignored() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    fixture.file(root, "My Drive/report.pdf", 300, "REPORT", PDF);
    fixture.mirror(|conn| {
        conn.execute("UPDATE mirror_item SET is_root = 0 WHERE local_stable_id = ?1", [root])
            .expect("unmark root");
    });

    assert!(fixture.accounts().is_empty());
}

/// A root with no Drive id of its own is not a root we can anchor to.
#[test]
fn a_root_without_a_drive_id_is_ignored() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    fixture.file(root, "My Drive/report.pdf", 300, "REPORT", PDF);
    fixture.mirror(|conn| {
        conn.execute("UPDATE root_config SET item_id = NULL", [])
            .expect("clear item_id");
    });

    assert!(fixture.accounts().is_empty());
}

// ── Shapes and kinds ─────────────────────────────────────────────────

/// The mirror folder itself, which no downward walk reaches: its id comes straight from
/// `root_config`.
#[test]
fn the_mirror_root_itself_resolves_to_the_drive_root_folder() {
    let mut fixture = Fixture::new("acct");
    fixture.root("My Drive", "0AFZXf6SvEaTnUk9PVA");
    let root_dir = fixture.tree.join("My Drive");

    assert_eq!(
        fixture.resolve(&root_dir),
        Some(MirrorItem {
            id: "0AFZXf6SvEaTnUk9PVA".to_string(),
            kind: DriveItemKind::Folder,
        })
    );
}

/// A mirrored subfolder takes the folder URL shape, from Drive's own mime type.
#[test]
fn a_mirrored_folder_takes_the_folder_shape() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    fixture.dir(root, "My Drive/Taxes", 200, "TAXES");

    assert_eq!(
        fixture.resolve(&fixture.tree.join("My Drive/Taxes")),
        Some(MirrorItem {
            id: "TAXES".to_string(),
            kind: DriveItemKind::Folder,
        })
    );
}

/// The mime type is better information than the on-disk extension the stub path has to
/// guess from, so a native doc gets its editor URL even here.
#[test]
fn a_native_doc_takes_its_editor_shape_from_the_mime_type() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let sheet = fixture.file(root, "My Drive/stats.gsheet", 300, "SHEET1", SHEET_MIME);

    assert_eq!(
        fixture.resolve(&sheet),
        Some(MirrorItem {
            id: "SHEET1".to_string(),
            kind: DriveItemKind::Native("spreadsheets"),
        })
    );
}

#[test]
fn mime_types_map_to_the_url_shapes() {
    assert_eq!(kind_for_mime(FOLDER_MIME, true), DriveItemKind::Folder);
    assert_eq!(
        kind_for_mime("application/vnd.google-apps.document", false),
        DriveItemKind::Native("document")
    );
    assert_eq!(kind_for_mime(SHEET_MIME, false), DriveItemKind::Native("spreadsheets"));
    assert_eq!(
        kind_for_mime("application/vnd.google-apps.presentation", false),
        DriveItemKind::Native("presentation")
    );
    assert_eq!(
        kind_for_mime("application/vnd.google-apps.form", false),
        DriveItemKind::Native("forms")
    );
    // Native, but no editor path we've verified: Drive's own resolver sorts it out.
    assert_eq!(
        kind_for_mime("application/vnd.google-apps.script", false),
        DriveItemKind::NativeUnknown
    );
    assert_eq!(kind_for_mime(PDF, false), DriveItemKind::Binary);
    // Drive disagreeing with itself: trust the folder flag over the mime type.
    assert_eq!(kind_for_mime(PDF, true), DriveItemKind::Folder);
}

// ── Shortcuts ────────────────────────────────────────────────────────

/// A shortcut's own id opens the shortcut, not the file the pane shows, so it resolves
/// one hop to its target — id and URL shape both.
#[test]
fn a_shortcut_resolves_to_what_it_points_at() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let link = fixture.file(root, "My Drive/budget.gsheet", 300, "SHORTCUT-ID", SHORTCUT_MIME);
    fixture.item(400, "TARGET-ID", SHEET_MIME, false);
    fixture.metadata(|conn| {
        conn.execute(
            "INSERT INTO shortcut_details (shortcut_stable_id, target_stable_id, target_mime_type) \
             VALUES (300, 400, ?1)",
            [SHEET_MIME],
        )
        .expect("insert shortcut_details");
    });

    assert_eq!(
        fixture.resolve(&link),
        Some(MirrorItem {
            id: "TARGET-ID".to_string(),
            kind: DriveItemKind::Native("spreadsheets"),
        })
    );
}

/// A shortcut whose target row is missing (or itself a shortcut) resolves to nothing
/// rather than to the shortcut's own id.
#[test]
fn a_shortcut_with_no_usable_target_yields_nothing() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let dangling = fixture.file(root, "My Drive/dangling.gsheet", 300, "SHORTCUT-A", SHORTCUT_MIME);
    let chained = fixture.file(root, "My Drive/chained.gsheet", 301, "SHORTCUT-B", SHORTCUT_MIME);
    fixture.item(401, "ANOTHER-SHORTCUT", SHORTCUT_MIME, false);
    fixture.metadata(|conn| {
        // 300's target row simply isn't there; 301 points at another shortcut.
        conn.execute(
            "INSERT INTO shortcut_details (shortcut_stable_id, target_stable_id, target_mime_type) \
             VALUES (300, 999, 'x'), (301, 401, 'x')",
            [],
        )
        .expect("insert shortcut_details");
    });

    assert_eq!(fixture.resolve(&dangling), None);
    assert_eq!(fixture.resolve(&chained), None);
}

// ── Several accounts ─────────────────────────────────────────────────

/// Someone can be signed into more than one account, each with its own mirror pair. A
/// file resolves against whichever account's root contains it.
#[test]
fn each_signed_in_account_is_searched() {
    let mut work = Fixture::new("111111111111111111111");
    let work_root = work.root("Work Drive", "WORK-ROOT");
    let work_file = work.file(work_root, "Work Drive/contract.pdf", 300, "WORK-FILE", PDF);

    // A second account inside the SAME support directory, with its own tree.
    let personal_account = work.support.join(DRIVE_SUPPORT_SUBDIR).join("222222222222222222222");
    fs::create_dir_all(&personal_account).expect("account dir");
    let mut personal = Fixture::new("222222222222222222222");
    let personal_root = personal.root("My Drive", "PERSONAL-ROOT");
    let personal_file = personal.file(personal_root, "My Drive/holiday.pdf", 300, "PERSONAL-FILE", PDF);
    fs::copy(personal.account.join(MIRROR_DB), personal_account.join(MIRROR_DB)).expect("copy mirror db");
    fs::copy(
        personal.account.join(MIRROR_METADATA_DB),
        personal_account.join(MIRROR_METADATA_DB),
    )
    .expect("copy metadata db");

    let accounts = scan_accounts(&work.support.join(DRIVE_SUPPORT_SUBDIR));
    assert_eq!(accounts.len(), 2, "both accounts contribute a mirror pair");
    assert_eq!(
        resolve_in(&accounts, &work_file).expect("work file resolves").id,
        "WORK-FILE"
    );
    assert_eq!(
        resolve_in(&accounts, &personal_file)
            .expect("personal file resolves")
            .id,
        "PERSONAL-FILE"
    );
}

// ── Reaching the same file two ways ──────────────────────────────────

/// Drive puts a symlink at `~/Library/CloudStorage/GoogleDrive-<account>/My Drive`
/// pointing at the mirror folder, so the same file has two spellings. The walk up
/// compares inodes rather than path prefixes, which is what makes both work.
#[test]
fn a_symlinked_spelling_of_the_same_path_resolves() {
    let mut fixture = Fixture::new("acct");
    let root = fixture.root("My Drive", "ROOT");
    let real = fixture.file(root, "My Drive/report.pdf", 300, "REPORT", PDF);

    let link = fixture.tree.join("CloudStorage-My Drive");
    std::os::unix::fs::symlink(fixture.tree.join("My Drive"), &link).expect("symlink");
    let through_link = link.join("report.pdf");

    assert_eq!(fixture.resolve(&real).expect("direct").id, "REPORT");
    assert_eq!(
        fixture.resolve(&through_link).expect("through the symlink").id,
        "REPORT"
    );
}
