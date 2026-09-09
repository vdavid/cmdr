//! Resolving a MIRRORED Google Drive file to its Drive item ID, out of Drive's own
//! local metadata databases.
//!
//! The third and last ID source (`mod.rs` has the other two, both cheaper). It exists
//! because mirror mode leaves no trace on the file: the bytes are an ordinary local
//! file with no xattr, and only a Google-native item carries a stub. Finder shows Drive
//! actions on those files through a Finder Sync extension, which is Finder-only.
//!
//! ## The chain
//!
//! Mirror mode keeps its own PAIR of SQLite databases, beside the stream-mode ones, in
//! `~/Library/Application Support/Google/DriveFS/<account-id>/`:
//!
//! 1. `mirror_sqlite.db` maps the LOCAL tree: `root_config` names each mirror root,
//!    and `mirror_item` holds one row per mirrored item carrying its `inode`, its
//!    parent, its on-disk `local_filename`, and a `stable_id`.
//! 2. `mirror_metadata_sqlite.db` maps a `stable_id` to the Drive item: `items.id` is
//!    the Drive file ID, plus `mime_type` and the `trashed` / `is_tombstone` flags.
//!
//! ❗ The two `stable_id` spaces are NOT the same. `mirror_sqlite.db`'s ids index
//! `mirror_metadata_sqlite.db`, never the stream-mode `metadata_sqlite_db` beside it —
//! the same file was `330816` in the mirror pair and `324388` in the stream-mode
//! database (verified on Drive for desktop 130.0, macOS 26.6.2, 2026-09-09).
//!
//! ## How a path becomes a row, and why not by name
//!
//! ❌ Never by `local_filename` alone. Names repeat (`_archive` appears three times in
//! a real 3,268-row mirror), so a name match can hand back SOMEONE ELSE'S file, and the
//! column carries no index of its own either.
//!
//! Instead: **walk up the path to a mirror root by inode, then walk back down the
//! database by parent + name, and prove the answer with the leaf's inode.**
//!
//! - Up: `stat` each ancestor until one's inode matches a root's, stopping at a device
//!   change (a mirror root can't sit across a mount from its contents). Inode identity
//!   rather than a path prefix, so the same file resolves through
//!   `~/My Drive/…` and through the `~/Library/CloudStorage/GoogleDrive-…/My Drive/…`
//!   symlink that points at it.
//! - Down: one indexed lookup per component on `UNIQUE (parent_local_stable_id,
//!   local_filename)`, which is exact by construction: no candidate set, no ambiguity.
//! - Proof: the row we land on must carry the inode the file actually has. A stale
//!   database (the file was replaced, renamed, or moved since Drive last looked) fails
//!   the check and we offer nothing.
//!
//! ## Fail closed, everywhere
//!
//! Every step answers `Option` and every `None` means "no menu item". That is exactly
//! what these files got before this module existed, so the worst case of a missing
//! database, a schema change, a lock, a trashed row, or a mismatch is no worse than
//! not trying. The upside case is a link; the downside case must never be a link to the
//! WRONG item.
//!
//! ## Read-only, forever
//!
//! ❌ Never open these databases for writing. `open_read_only` is the only door.
//! SQLite's read-only WAL reader does touch the `-shm` sidecar (that is how it
//! registers a read mark), which is WAL index bookkeeping and not database content; it
//! cannot change what Drive stores and cannot block Drive's writer. `?immutable=1`
//! would avoid even that, at the price of ignoring the WAL — and the WAL held 222 MB
//! against a 220 MB database here, so an immutable read would be answering from
//! last month.

use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;
use rusqlite::{Connection, OptionalExtension};

use super::DriveItemKind;

/// Where Drive for desktop keeps one directory per signed-in account, under the
/// user's Application Support.
const DRIVE_SUPPORT_SUBDIR: &str = "Google/DriveFS";

/// The mirror pair's filenames inside an account directory.
const MIRROR_DB: &str = "mirror_sqlite.db";
const MIRROR_METADATA_DB: &str = "mirror_metadata_sqlite.db";

/// Drive's mime type for a shortcut: an item whose content is a pointer to another
/// item. A machine-readable media type, not a message, so matching it is not the
/// string-matching the house rule forbids.
const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";

/// How far up from a path we're willing to look for a mirror root.
///
/// A guard against a pathological path rather than a real limit: mirror roots live a
/// couple of levels under the home directory, and the walk already stops at the first
/// device change.
const MAX_ANCESTOR_WALK: usize = 64;

/// How long a scan of the account directories and their mirror roots stays good.
///
/// The point is the NEGATIVE case: without it, every right-click on an ordinary file
/// anywhere would open a multi-megabyte SQLite database to learn nothing. Roots change
/// only when someone reconfigures Drive, so a stale window costs at most half a minute
/// of the menu items not appearing (or appearing for a root that moved, where the
/// inode proof then declines) — never a wrong link.
const ROOTS_TTL: Duration = Duration::from_secs(30);

/// One mirror root: the row in `mirror_item` that `root_config` points at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorRoot {
    /// `mirror_item.local_stable_id`, where a downward walk starts.
    local_stable_id: i64,
    /// The inode of the root directory on disk, which is how we recognize it.
    inode: u64,
    /// The root's own Drive ID, from `root_config.item_id`. It's the one ID in this
    /// chain that needs no walk, so right-clicking the mirror folder itself resolves.
    item_id: String,
}

/// One signed-in account's mirror databases and the roots they describe.
#[derive(Debug, Clone)]
struct MirrorAccount {
    mirror_db: PathBuf,
    metadata_db: PathBuf,
    roots: Vec<MirrorRoot>,
}

/// The Drive item a path resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirrorItem {
    /// The Drive file ID.
    pub id: String,
    /// Which URL shape the item takes, from its Drive mime type.
    pub kind: DriveItemKind,
}

/// The Drive item mirrored at `path`, or `None` when nothing resolves it beyond doubt.
pub(super) fn resolve(path: &Path) -> Option<MirrorItem> {
    let accounts = cached_accounts()?;
    resolve_in(&accounts, path)
}

/// The resolution proper, over an already-scanned account list. Split out so the tests
/// drive it against a fixture instead of the user's real Drive.
fn resolve_in(accounts: &[MirrorAccount], path: &Path) -> Option<MirrorItem> {
    let target = std::fs::metadata(path).ok()?;
    for account in accounts {
        // The mirror folder itself, which no downward walk can reach: its Drive ID is
        // sitting in `root_config`.
        if let Some(root) = account.roots.iter().find(|root| root.inode == target.ino()) {
            return Some(MirrorItem {
                id: root.item_id.clone(),
                kind: DriveItemKind::Folder,
            });
        }
        let Some((root, components)) = walk_up_to_root(path, &target, &account.roots) else {
            continue;
        };
        if let Some(item) = resolve_within_account(account, root, &components, target.ino()) {
            return Some(item);
        }
    }
    None
}

/// Walks up from `path` until an ancestor IS one of `roots`, and hands back that root
/// plus the path components from it down to `path` (root-first).
///
/// Stops at a device change: a mirror root and its contents live on one volume, so an
/// ancestor across a mount boundary can't be the root and neither can anything above
/// it. That also keeps an ordinary right-click from walking the whole way to `/` on a
/// path that reached us through a mount.
fn walk_up_to_root<'a>(
    path: &Path,
    target: &std::fs::Metadata,
    roots: &'a [MirrorRoot],
) -> Option<(&'a MirrorRoot, Vec<OsString>)> {
    if roots.is_empty() {
        return None;
    }
    let mut components: Vec<OsString> = Vec::new();
    let mut cursor = path;
    for _ in 0..MAX_ANCESTOR_WALK {
        components.push(cursor.file_name()?.to_os_string());
        let parent = cursor.parent()?;
        // Following symlinks on purpose: it's what lets the CloudStorage spelling of a
        // mirrored path land on the same root as the folder it points at.
        let meta = std::fs::metadata(parent).ok()?;
        if meta.dev() != target.dev() {
            return None;
        }
        if let Some(root) = roots.iter().find(|root| root.inode == meta.ino()) {
            components.reverse();
            return Some((root, components));
        }
        cursor = parent;
    }
    None
}

/// Walks `components` down from `root` in this account's databases, and turns the row
/// it lands on into a Drive item.
///
/// `target_inode` is the inode the file on disk actually has; the row has to agree or
/// this answers `None`.
fn resolve_within_account(
    account: &MirrorAccount,
    root: &MirrorRoot,
    components: &[OsString],
    target_inode: u64,
) -> Option<MirrorItem> {
    let mirror = open_read_only(&account.mirror_db)?;
    let stable_id = walk_down(&mirror, root.local_stable_id, components, target_inode)?;
    drop(mirror);

    let metadata = open_read_only(&account.metadata_db)?;
    drive_item(&metadata, stable_id)
}

/// The `mirror_item.stable_id` of the row `components` names under `local_stable_id`,
/// once its inode matches the file on disk.
fn walk_down(conn: &Connection, root_id: i64, components: &[OsString], target_inode: u64) -> Option<i64> {
    let mut stmt = conn
        .prepare(
            "SELECT local_stable_id, stable_id, inode FROM mirror_item \
             WHERE parent_local_stable_id = ?1 AND local_filename = ?2",
        )
        .ok()?;
    let mut cursor = (root_id, 0_i64, 0_u64);
    for component in components {
        // `local_filename` is a TEXT column, so a name that isn't UTF-8 can't be in it.
        // Refusing rather than lossily converting: a lossy name would match a row that
        // describes a different file.
        let name = component.to_str()?;
        cursor = stmt
            .query_row(rusqlite::params![cursor.0, name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? as u64))
            })
            .optional()
            .ok()??;
    }
    let (_, stable_id, inode) = cursor;
    // The proof. Everything above found A row; this is what says it's THIS file.
    if inode != target_inode {
        log::debug!(
            target: "google_drive",
            "a mirrored path matched `mirror_item` {stable_id}, whose inode {inode} isn't the file's {target_inode}; offering no Drive link"
        );
        return None;
    }
    Some(stable_id)
}

/// The Drive item one `stable_id` names, following a shortcut to what it points at.
fn drive_item(conn: &Connection, stable_id: i64) -> Option<MirrorItem> {
    let row = item_row(conn, stable_id)?;
    // A shortcut's own ID opens the shortcut, not the thing the user sees in the pane,
    // so resolve one hop. Exactly one: a shortcut to a shortcut isn't something Drive
    // makes, and a loop must not become a walk.
    let row = if row.mime_type == SHORTCUT_MIME {
        let target_id: i64 = conn
            .query_row(
                "SELECT target_stable_id FROM shortcut_details WHERE shortcut_stable_id = ?1",
                [stable_id],
                |row| row.get(0),
            )
            .optional()
            .ok()??;
        let target = item_row(conn, target_id)?;
        if target.mime_type == SHORTCUT_MIME {
            return None;
        }
        target
    } else {
        row
    };
    Some(MirrorItem {
        kind: kind_for_mime(&row.mime_type, row.is_folder),
        id: row.id,
    })
}

/// One `items` row, already filtered on the states that must not produce a link.
struct ItemRow {
    id: String,
    mime_type: String,
    is_folder: bool,
}

/// Reads one `items` row, refusing a trashed, tombstoned, or ID-less one.
///
/// A trashed row still has a working Drive URL, but offering it would send the user to
/// a copy of the file that is on its way out while the pane shows the live one. A
/// tombstone is a deletion marker whose ID names nothing at all.
fn item_row(conn: &Connection, stable_id: i64) -> Option<ItemRow> {
    let (id, mime_type, is_folder, trashed, tombstone): (String, String, bool, bool, bool) = conn
        .query_row(
            "SELECT id, mime_type, is_folder, trashed, is_tombstone FROM items WHERE stable_id = ?1",
            [stable_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()
        .ok()??;
    if id.is_empty() || trashed || tombstone {
        return None;
    }
    Some(ItemRow {
        id,
        mime_type,
        is_folder,
    })
}

/// Which URL shape a Drive mime type takes.
///
/// Better information than the on-disk extension the stub path has to use: this is
/// Drive's own type for the item. The editor segments are the ones `mod.rs` verified
/// against Google's API; every other `application/vnd.google-apps.*` type is native
/// but has no editor path we've checked, so it goes to Drive's own resolver.
fn kind_for_mime(mime_type: &str, is_folder: bool) -> DriveItemKind {
    match mime_type {
        "application/vnd.google-apps.folder" => DriveItemKind::Folder,
        "application/vnd.google-apps.document" => DriveItemKind::Native("document"),
        "application/vnd.google-apps.spreadsheet" => DriveItemKind::Native("spreadsheets"),
        "application/vnd.google-apps.presentation" => DriveItemKind::Native("presentation"),
        "application/vnd.google-apps.form" => DriveItemKind::Native("forms"),
        other if other.starts_with("application/vnd.google-apps.") => DriveItemKind::NativeUnknown,
        // `is_folder` is Drive's own flag and the folder mime above should be the only
        // thing carrying it, but if the two ever disagree the folder URL is the honest
        // answer for something Drive calls a folder.
        _ if is_folder => DriveItemKind::Folder,
        _ => DriveItemKind::Binary,
    }
}

/// Opens one of Drive's databases read-only, logging at debug and answering `None` on
/// anything that goes wrong (missing, locked, mid-recovery, or replaced under us).
fn open_read_only(db_path: &Path) -> Option<Connection> {
    match crate::sqlite_util::open_read_only(db_path) {
        Ok(conn) => Some(conn),
        Err(e) => {
            log::debug!(target: "google_drive", "can't read {}: {e}", db_path.display());
            None
        }
    }
}

// ── Account discovery, cached ────────────────────────────────────────

/// One scan of the account directories, and when it happened.
struct AccountScan {
    at: Instant,
    accounts: Vec<MirrorAccount>,
}

/// The last scan. See [`ROOTS_TTL`] for why this is cached rather than re-read.
static ACCOUNTS: LazyLock<Mutex<Option<AccountScan>>> = LazyLock::new(|| Mutex::new(None));

/// The signed-in accounts' mirror databases and roots, re-scanned at most every
/// [`ROOTS_TTL`]. `None` means no account mirrors anything, so there is nothing to
/// resolve against.
fn cached_accounts() -> Option<Vec<MirrorAccount>> {
    let mut slot = ACCOUNTS.lock_ignore_poison();
    if let Some(scan) = slot.as_ref()
        && scan.at.elapsed() < ROOTS_TTL
    {
        return (!scan.accounts.is_empty()).then(|| scan.accounts.clone());
    }
    let accounts = scan_accounts(&dirs::data_dir()?.join(DRIVE_SUPPORT_SUBDIR));
    *slot = Some(AccountScan {
        at: Instant::now(),
        accounts: accounts.clone(),
    });
    (!accounts.is_empty()).then_some(accounts)
}

/// Every account directory under `drive_support` that holds a mirror pair with at
/// least one root. Enumerated rather than assumed: the directory name is an opaque
/// numeric account ID, and someone can be signed into several accounts at once.
fn scan_accounts(drive_support: &Path) -> Vec<MirrorAccount> {
    let Ok(entries) = std::fs::read_dir(drive_support) else {
        return Vec::new();
    };
    let mut accounts = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let mirror_db = dir.join(MIRROR_DB);
        let metadata_db = dir.join(MIRROR_METADATA_DB);
        if !mirror_db.is_file() || !metadata_db.is_file() {
            continue;
        }
        let Some(conn) = open_read_only(&mirror_db) else {
            continue;
        };
        let roots = read_roots(&conn);
        if roots.is_empty() {
            continue;
        }
        accounts.push(MirrorAccount {
            mirror_db,
            metadata_db,
            roots,
        });
    }
    accounts
}

/// The mirror roots one account's `mirror_sqlite.db` describes.
///
/// Through `root_config` (a handful of rows) into `mirror_item` by primary key, rather
/// than `WHERE is_root = 1` — that column carries no index, so asking it directly is a
/// full scan of every mirrored item. `is_root` still has to agree on the row we land
/// on: it costs nothing and a disagreement means we've misread the schema.
fn read_roots(conn: &Connection) -> Vec<MirrorRoot> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT item.local_stable_id, item.inode, root.item_id FROM root_config AS root \
         JOIN mirror_item AS item ON item.local_stable_id = root.local_stable_id \
         WHERE item.is_root = 1 AND root.item_id IS NOT NULL AND root.item_id <> ''",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok(MirrorRoot {
            local_stable_id: row.get(0)?,
            inode: row.get::<_, i64>(1)? as u64,
            item_id: row.get(2)?,
        })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

#[cfg(test)]
mod tests;
