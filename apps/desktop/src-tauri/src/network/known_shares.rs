//! Known network shares store.
//!
//! Persists metadata about network shares the user has connected to.
//! Enables username pre-fill, auth change detection, and quick reconnect.

use crate::ignore_poison::IgnorePoison;
use crate::network::NetworkHost;
use crate::network::server_identity::SmbServer;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Connection mode used for the last successful connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    Guest,
    Credentials,
}

/// Authentication options available for a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthOptions {
    GuestOnly,
    CredentialsOnly,
    GuestOrCredentials,
}

/// Information about a known network share.
///
/// Two kinds of row share this type. An empty `share_name` is the HOST's sign-in
/// history (what the share list last signed in as). A non-empty one is a saved
/// SHARE place (`docs/specs/saved-smb-shares.md`): one row per share, keyed by
/// server identity + share name, never by account, since an SMB volume id carries
/// no username. The place fields below are only ever set on share rows.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownNetworkShare {
    /// The name the person knows the server by (the discovery list's `name`):
    /// what the hub groups a share under, and what the Keychain keys it by.
    pub server_name: String,
    pub share_name: String,
    /// Currently only "smb".
    pub protocol: String,
    /// ISO 8601.
    pub last_connected_at: String,
    pub last_connection_mode: ConnectionMode,
    pub last_known_auth_options: AuthOptions,
    /// None for guest.
    pub username: Option<String>,
    /// Share rows: what the mount dialed (an IP or a hostname), which may not be
    /// `server_name`. `None` for a share no mount went through yet.
    #[serde(default)]
    pub address: Option<String>,
    /// Share rows: the SMB port, `None` for 445.
    #[serde(default)]
    pub port: Option<u16>,
    /// Share rows: the volume id the last mount had, read off `statfs` like every
    /// SMB id. ❗ Never re-derived from `server_name`: only the mount knows which
    /// spelling of the server it got. `None` until a mount through Cmdr went
    /// through.
    #[serde(default)]
    pub volume_id: Option<String>,
    /// Share rows: where the last mount sat, the path a `saved` row lands on.
    #[serde(default)]
    pub mount_path: Option<String>,
    /// Share rows: whether the share's place shows in the volume switcher. Set on
    /// its first mount through Cmdr, moved by `set_share_pinned`.
    #[serde(default)]
    pub pinned: bool,
}

impl KnownNetworkShare {
    /// Whether this row is a saved share rather than the host's sign-in history.
    pub fn is_share(&self) -> bool {
        !self.share_name.is_empty()
    }
}

/// One share on one server, named the way the mount reported it. Private, like the
/// list it lives in: the switch is read and set only through
/// [`direct_connection_enabled`] and [`set_direct_connection_enabled`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareRef {
    server_name: String,
    share_name: String,
}

/// The known shares store, persisted to disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownSharesStore {
    #[serde(default)]
    pub known_network_shares: Vec<KnownNetworkShare>,
    /// The shares the user turned "Use Cmdr's fast direct connection" off for.
    ///
    /// An opt-out list rather than a flag on [`KnownNetworkShare`], because that type
    /// records Cmdr's OWN connects (server-level history, and the shares Cmdr itself
    /// mounted): a share macOS mounted has no row, and minting one would invent a connection history
    /// that then shows up in the servers hub. Absence means on, so a store saved
    /// before the setting existed keeps every share on the fast connection. See
    /// [`direct_connection_enabled`].
    #[serde(default)]
    direct_connection_opt_outs: Vec<ShareRef>,
    /// Saved shares whose server moved to a new address since their last mount, by the
    /// volume id their row still carries (`server_move::smb`).
    ///
    /// ❗ The id stays the OLD one until a mount at the new address reports the real one,
    /// since only the mount knows which spelling of the server it got. Meanwhile favorites
    /// and tabs keep naming the old id, which now reaches the share at its new address;
    /// the first mount there completes the move ([`remember_share`] answers it). On disk,
    /// so a move survives a restart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_moves: Vec<String>,
}

/// A share whose server moved, at the first mount that reported its id at the new
/// address: what a pending move re-keys from and to (`server_move::smb::complete`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedMove {
    /// The id the share had before its server moved, which favorites and tabs still name.
    pub old_volume_id: String,
    /// Where the share's last mount before the move sat, which tabs on the old id spell.
    pub old_mount_path: Option<String>,
    /// The id the mount at the new address reported.
    pub new_volume_id: String,
    /// Where that mount sits.
    pub new_mount_path: String,
    pub share_name: String,
    /// The name the row files the server under.
    pub server_name: String,
}

/// In-memory cache of known shares, synchronized with disk.
static KNOWN_SHARES: std::sync::OnceLock<Mutex<KnownSharesStore>> = std::sync::OnceLock::new();

fn get_known_shares_mutex() -> &'static Mutex<KnownSharesStore> {
    KNOWN_SHARES.get_or_init(|| Mutex::new(KnownSharesStore::default()))
}

/// Durably writes content to a file using write-to-temp + fsync + rename + parent-dir fsync.
/// On failure, the original file (if any) remains intact. The fsyncs make the write survive a
/// power loss, not just process death. See `crate::config::durable_write_json` for the rationale.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    crate::config::durable_write_json(path, &tmp, content)
}

/// Removes a stale `.tmp` file left over from a crash during atomic write.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Where the store lives on disk, set once by [`load_known_shares`]. Kept here so a
/// write needs no `AppHandle`: "Connect directly" records its consent from code the
/// MCP executor also calls, and that holds none. Unset (tests, no data dir) means the
/// store lives in memory only.
static STORE_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Returns the path to the known shares store file.
fn get_store_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join("known-shares.json"))
}

/// Loads known shares from disk into memory.
pub fn load_known_shares<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(path) = get_store_path(app) else {
        return;
    };
    let _ = STORE_PATH.set(path.clone());

    cleanup_tmp_file(&path);

    let store = if let Ok(contents) = fs::read_to_string(&path) {
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        KnownSharesStore::default()
    };

    *get_known_shares_mutex().lock_ignore_poison() = store;
}

/// Saves known shares from memory to disk.
fn save_known_shares() {
    let Some(path) = STORE_PATH.get() else {
        return;
    };

    let store = get_known_shares_mutex().lock_ignore_poison().clone();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(&store)
        && let Err(e) = atomic_write_json(path, &json)
    {
        log::warn!("Couldn't write known shares store: {}", e);
    }
}

/// Creates a unique key for a share.
///
/// Case- and NFC-folded, because one share reaches this store spelled more than one
/// way: the frontend saves the name from the server's share list (composed), while
/// the mount paths look it up from `statfs` (decomposed on macOS). A byte key
/// remembers one share as two, and the auth mode saved under one spelling is missing
/// under the other.
fn share_key(server_name: &str, share_name: &str) -> String {
    format!("{}/{}", fold_name(server_name), fold_name(share_name))
}

/// Gets all known network shares.
pub fn get_all_known_shares() -> Vec<KnownNetworkShare> {
    get_known_shares_mutex()
        .lock_ignore_poison()
        .known_network_shares
        .clone()
}

/// Gets a specific known share by server and share name.
pub fn get_known_share(server_name: &str, share_name: &str) -> Option<KnownNetworkShare> {
    let key = share_key(server_name, share_name);
    get_known_shares_mutex()
        .lock_ignore_poison()
        .known_network_shares
        .iter()
        .find(|s| share_key(&s.server_name, &s.share_name) == key)
        .cloned()
}

/// Updates or adds a known network share.
/// Called after a successful connection.
pub fn update_known_share(share: KnownNetworkShare) {
    let key = share_key(&share.server_name, &share.share_name);

    {
        let mut cache = get_known_shares_mutex().lock_ignore_poison();
        // Find and update, or add new
        if let Some(existing) = cache
            .known_network_shares
            .iter_mut()
            .find(|s| share_key(&s.server_name, &s.share_name) == key)
        {
            *existing = share;
        } else {
            cache.known_network_shares.push(share);
        }
    }

    save_known_shares();
}

/// The username to pre-fill a login form for `server_name`, if this server has ever been
/// signed in to.
///
/// A LOOKUP, not a map of every server's hint, for the same reason
/// [`get_known_share`] is one: the key stays on this side. A command handing the
/// frontend a keyed map makes the key part of the IPC contract, and the caller has to
/// rebuild it to read its own answer, so the rule ends up written twice in two
/// languages and drifts. Ask by name and there is one rule, here.
///
/// Matching is [`credential_key`](crate::network::server_identity::credential_key), the
/// same identity the stored PASSWORD is keyed by, so every name form of one server
/// (`Naspolya`, `naspolya.local`, `Naspolya._smb._tcp.local`) finds the hint saved under
/// any other. The share list records whichever name the connect used, while the login
/// form opens on whatever discovery produced; a raw compare misses exactly the case the
/// hint exists for.
///
/// Last match wins: shares are appended in connect order, so the newest username on this
/// server is the one the person most recently signed in as.
pub fn get_username_hint(server_name: &str) -> Option<String> {
    use crate::network::server_identity::credential_key;

    let key = credential_key(server_name);
    get_known_shares_mutex()
        .lock_ignore_poison()
        .known_network_shares
        .iter()
        .filter(|s| credential_key(&s.server_name) == key)
        .filter_map(|s| s.username.clone())
        .next_back()
}

/// Whether `a` and `b` are the same share: the name folded, the server by
/// identity under either the name it's known by or the address a mount dialed.
///
/// ❗ When both rows know their port (a mount filed each), the ports must agree:
/// an address names a MACHINE, and one machine can run several servers with a
/// `public` share each. A row an Add filed knows no port; its server name is the
/// host's discovery name, which carries the port off 445, so it still pairs.
fn same_share_row(a: &KnownNetworkShare, b: &KnownNetworkShare, hosts: &[NetworkHost]) -> bool {
    use crate::network::server_identity::same_machine;

    if fold_name(&a.share_name) != fold_name(&b.share_name) {
        return false;
    }
    let port = |row: &KnownNetworkShare| row.address.as_ref().map(|_| row.port.unwrap_or(445));
    if let (Some(pa), Some(pb)) = (port(a), port(b))
        && pa != pb
    {
        return false;
    }
    let names = |row: &KnownNetworkShare| {
        std::iter::once(row.server_name.clone())
            .chain(row.address.clone())
            .collect::<Vec<_>>()
    };
    names(a)
        .iter()
        .any(|x| names(b).iter().any(|y| same_machine(x, y, hosts)))
}

/// Files `row` (a share row) in `rows`, replacing the one for the same share.
///
/// What a replace keeps: the pin (only `set_share_pinned` moves one, except that
/// a share's FIRST mount pins it, rule 1 of the servers model), and a volume id,
/// mount path, address, and port the new row doesn't bring (an add names none of
/// them). The account is the new row's: the row remembers who opened it last.
///
/// ❗ Except that an ADD naming no account keeps the one the share is saved with. An
/// add is the row with no address (every mount records the address it dialed), and
/// `smb://host/share` typed with no user says nothing about who opens it: filing it
/// "as nobody" made the next open ask for a password that was stored all along.
///
/// Answers the volume id and mount path the replaced row had, `None` for a new row or
/// one with no id.
fn upsert_share_row(
    rows: &mut Vec<KnownNetworkShare>,
    mut row: KnownNetworkShare,
    hosts: &[NetworkHost],
) -> Option<(String, Option<String>)> {
    match rows
        .iter_mut()
        .find(|existing| existing.is_share() && same_share_row(existing, &row, hosts))
    {
        Some(existing) => {
            let previous = existing.volume_id.clone().map(|id| (id, existing.mount_path.clone()));
            let first_mount = existing.volume_id.is_none() && row.volume_id.is_some();
            row.pinned = existing.pinned || first_mount;
            row.volume_id = row.volume_id.or(existing.volume_id.take());
            row.mount_path = row.mount_path.or(existing.mount_path.take());
            row.port = row.port.or(existing.port);
            let is_add = row.address.is_none();
            if is_add && row.username.is_none() && existing.username.is_some() {
                row.username = existing.username.take();
                row.last_connection_mode = existing.last_connection_mode;
            }
            row.address = row.address.or(existing.address.take());
            *existing = row;
            previous
        }
        None => {
            row.pinned = row.volume_id.is_some();
            rows.push(row);
            None
        }
    }
}

/// Files `row` in `store` ([`upsert_share_row`]), and completes the pending move of the
/// share it replaced when the row brings the id a mount reported. Answers the move only
/// when the id changed: a mount that minted the same id just ends the wait.
fn remember_in(store: &mut KnownSharesStore, row: KnownNetworkShare, hosts: &[NetworkHost]) -> Option<CompletedMove> {
    let reported = row.volume_id.clone().zip(row.mount_path.clone());
    let (share_name, server_name) = (row.share_name.clone(), row.server_name.clone());
    let (previous, old_mount_path) = upsert_share_row(&mut store.known_network_shares, row, hosts)?;
    let (new_volume_id, new_mount_path) = reported?;
    let pending = store.pending_moves.iter().position(|id| *id == previous)?;
    store.pending_moves.remove(pending);
    (previous != new_volume_id).then_some(CompletedMove {
        old_volume_id: previous,
        old_mount_path,
        new_volume_id,
        new_mount_path,
        share_name,
        server_name,
    })
}

/// Records a saved share, or refreshes the one it already is. See
/// [`upsert_share_row`], and `docs/specs/saved-smb-shares.md` for who may call it.
///
/// Answers a pending move the row just completed: the share's server moved, and this
/// is its first mount at the new address (`server_move::smb::complete` follows it).
pub fn remember_share(row: KnownNetworkShare) -> Option<CompletedMove> {
    debug_assert!(row.is_share(), "a share row names its share");
    let hosts = crate::network::fresh_discovered_hosts();
    let completed = {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        remember_in(&mut store, row, &hosts)
    };
    save_known_shares();
    completed
}

/// Points `rows` (a host's, as the servers listing filed them) at the server `to`,
/// answering how many it found. See [`move_host_rows_in`].
pub fn move_host_rows(rows: &[KnownNetworkShare], from: &SmbServer, to: &SmbServer) -> usize {
    let hosts = crate::network::fresh_discovered_hosts();
    let moved = {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        move_host_rows_in(&mut store, rows, from, to, &hosts)
    };
    if moved > 0 {
        save_known_shares();
    }
    moved
}

/// Points `rows` at `to` in one pass under the lock: each row with an address dials the
/// new one, a name that spelled the old address spells the new one, and each saved share
/// with a volume id waits for its first mount there (`pending_moves`). A share's
/// direct-connection opt-out is copied to the new address.
///
/// ❗ The volume id and mount path STAY: only a mount knows the new id, and the old one
/// is what favorites, tabs, and the share's `saved` row name until then.
///
/// ❗ A name that ISN'T the old address (a Bonjour name, `Naspolya`) stays: it still names
/// the server, and the password filed under it stays reachable. A row with no address
/// (the host's sign-in history, or a share an Add named) has only its name to say which
/// host it is, so it always takes the new one.
fn move_host_rows_in(
    store: &mut KnownSharesStore,
    rows: &[KnownNetworkShare],
    from: &SmbServer,
    to: &SmbServer,
    hosts: &[NetworkHost],
) -> usize {
    use crate::network::server_identity::{credential_key, smb_server};

    let old_key = credential_key(&smb_server(from.host(), from.port()));
    let new_name = crate::network::manual_servers::discovery_name(to.host(), to.port());
    let mut moved = 0;
    let mut shares = Vec::new();
    for row in &mut store.known_network_shares {
        if !rows.iter().any(|other| is_same_row(row, other)) {
            continue;
        }
        if row.address.is_none() || credential_key(&row.server_name) == old_key {
            row.server_name = new_name.clone();
        }
        if row.address.is_some() {
            row.address = Some(to.host().to_string());
            row.port = (to.port() != 445).then_some(to.port());
        }
        if let Some(id) = row.volume_id.clone().filter(|_| row.is_share())
            && !store.pending_moves.contains(&id)
        {
            store.pending_moves.push(id);
        }
        if row.is_share() {
            shares.push(row.share_name.clone());
        }
        moved += 1;
    }
    for share in shares {
        if is_opted_out(&store.direct_connection_opt_outs, &[from.host()], &share, hosts)
            && !is_opted_out(&store.direct_connection_opt_outs, &[to.host()], &share, hosts)
        {
            store.direct_connection_opt_outs.push(ShareRef {
                server_name: to.host().to_string(),
                share_name: share,
            });
        }
    }
    moved
}

/// Files `row` (a share row) in `rows` only when no row holds that share yet, answering whether it
/// did. ❗ A share that's already saved keeps everything about it: its pin, its account, the name
/// its server goes by. See [`remember_share_unless_saved`].
fn insert_unless_saved(rows: &mut Vec<KnownNetworkShare>, row: KnownNetworkShare, hosts: &[NetworkHost]) -> bool {
    let place = place_id(&row);
    if rows
        .iter()
        .any(|existing| existing.is_share() && (place_id(existing) == place || same_share_row(existing, &row, hosts)))
    {
        return false;
    }
    rows.push(row);
    true
}

/// Records a share only when nothing has saved it yet, as `row` says (pin included), answering
/// whether it did. The favorites writer (`smb_saved_shares::remember_favorited_share`): favoriting
/// a folder is a "remember this place", ❌ never a reason to move a pin or an account the person set.
pub fn remember_share_unless_saved(row: KnownNetworkShare) -> bool {
    debug_assert!(row.is_share(), "a share row names its share");
    let hosts = crate::network::fresh_discovered_hosts();
    let added = {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        insert_unless_saved(&mut store.known_network_shares, row, &hosts)
    };
    if added {
        save_known_shares();
    }
    added
}

/// Every saved share row.
pub fn saved_shares() -> Vec<KnownNetworkShare> {
    get_all_known_shares()
        .into_iter()
        .filter(KnownNetworkShare::is_share)
        .collect()
}

/// The id a share row's place goes by: the one its last mount had, else the id a
/// mount by the name the row knows would mint.
///
/// ❗ The ONE place that id is decided, for the servers listing and for every
/// lookup by it. A share an Add named has no mount yet, and a lookup that only
/// read the stored id couldn't find the row the hub was showing for it.
pub fn place_id(row: &KnownNetworkShare) -> String {
    row.volume_id.clone().unwrap_or_else(|| {
        cmdr_fs::volume::smb_volume_id(
            row.address.as_deref().unwrap_or(&row.server_name),
            row.port.unwrap_or(445),
            &row.share_name,
        )
    })
}

/// The saved share whose place the listing calls `volume_id`.
pub fn share_by_volume_id(volume_id: &str) -> Option<KnownNetworkShare> {
    saved_shares().into_iter().find(|row| place_id(row) == volume_id)
}

/// Moves a saved share's pin, in place under the lock, answering whether a share
/// row holds that volume id.
pub fn set_share_pinned(volume_id: &str, pinned: bool) -> bool {
    let moved = {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        match store
            .known_network_shares
            .iter_mut()
            .find(|row| row.is_share() && place_id(row) == volume_id)
        {
            Some(row) => {
                row.pinned = pinned;
                true
            }
            None => false,
        }
    };
    if moved {
        save_known_shares();
    }
    moved
}

/// Whether `a` and `b` are the same stored row, field for field on what makes a
/// row the one it is (never on when it was last used or how it's pinned).
fn is_same_row(a: &KnownNetworkShare, b: &KnownNetworkShare) -> bool {
    a.server_name == b.server_name && a.share_name == b.share_name && a.address == b.address && a.port == b.port
}

/// Drops `gone` from `rows`, answering how many went. See [`forget_rows`].
fn forget_rows_in(rows: &mut Vec<KnownNetworkShare>, gone: &[KnownNetworkShare]) -> usize {
    let before = rows.len();
    rows.retain(|row| !gone.iter().any(|other| is_same_row(row, other)));
    before - rows.len()
}

/// [`forget_rows_in`] on the store, plus the pending moves no row is left to complete.
fn forget_in(store: &mut KnownSharesStore, gone: &[KnownNetworkShare]) -> usize {
    let removed = forget_rows_in(&mut store.known_network_shares, gone);
    let rows = &store.known_network_shares;
    store
        .pending_moves
        .retain(|id| rows.iter().any(|row| row.volume_id.as_deref() == Some(id)));
    removed
}

/// Drops exactly the rows `gone` holds, as a caller read them from this store,
/// answering how many went. ❗ Rows only: mounts and passwords stay.
///
/// ❗ Exact rows, ❌ never "whatever this name matches": a Forget acts on what a
/// row SHOWED, so the caller hands back the rows it drew the row from (the servers
/// listing's host group, or the one share row a place id found), and a name match
/// here would reach rows that row never showed (another port of the same machine).
pub fn forget_rows(gone: &[KnownNetworkShare]) -> usize {
    let removed = {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        forget_in(&mut store, gone)
    };
    if removed > 0 {
        save_known_shares();
    }
    removed
}

/// Case- and NFC-folds one half of a share's name, the fold [`share_key`] applies.
fn fold_name(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    name.nfc().flat_map(char::to_lowercase).collect()
}

/// Whether `entry` names the share `share` on the server any of `server_names` names.
///
/// The server half asks [`same_machine`](crate::network::server_identity::same_machine)
/// rather than comparing keys, because `statfs` echoes whichever name form each mount
/// used: one NAS is `192.168.1.111` on one mount and `Naspolya._smb._tcp.local` on the
/// next, and a choice the other spelling can't see looks like the switch resetting
/// itself.
fn names_share(entry: &ShareRef, server_names: &[&str], share: &str, hosts: &[NetworkHost]) -> bool {
    use crate::network::server_identity::same_machine;

    fold_name(&entry.share_name) == fold_name(share)
        && server_names
            .iter()
            .any(|server| same_machine(&entry.server_name, server, hosts))
}

/// Whether `opt_outs` holds the share, under any name form of its server.
fn is_opted_out(opt_outs: &[ShareRef], server_names: &[&str], share: &str, hosts: &[NetworkHost]) -> bool {
    opt_outs
        .iter()
        .any(|entry| names_share(entry, server_names, share, hosts))
}

/// Records (`enabled: false`) or drops (`enabled: true`) the share's opt-out in `opt_outs`.
///
/// Either way every entry naming the share goes first, so a share chosen under two
/// spellings keeps one entry, filed under the newest.
fn apply_choice(opt_outs: &mut Vec<ShareRef>, server_name: &str, share: &str, enabled: bool, hosts: &[NetworkHost]) {
    opt_outs.retain(|entry| !names_share(entry, &[server_name], share, hosts));
    if !enabled {
        opt_outs.push(ShareRef {
            server_name: server_name.to_string(),
            share_name: share.to_string(),
        });
    }
}

/// Whether Cmdr may give this share its own direct smb2 session without being asked
/// right then: the per-share "Use Cmdr's fast direct connection" switch. `true` unless
/// the user turned it off.
///
/// ❗ **Every upgrade nobody clicked for must ask this**, under
/// `smb_upgrade::lock_volume_upgrade` and right before dialing. Today that's one place,
/// `smb_upgrade::register_smb_volume`, which the startup pass, the mount watcher, the
/// pane-open upgrade, and Cmdr's own mount all funnel through. A trigger that bypasses it makes the switch do
/// nothing.
///
/// `server_names` takes every spelling the caller has for the server (the `statfs`
/// name and the address it resolved to, say); any one of them matching counts.
pub fn direct_connection_enabled(server_names: &[&str], share: &str) -> bool {
    // Discovery is read BEFORE this store's lock is taken, so the two never nest.
    let hosts = crate::network::fresh_discovered_hosts();
    let store = get_known_shares_mutex().lock_ignore_poison();
    !is_opted_out(&store.direct_connection_opt_outs, server_names, share, &hosts)
}

/// Records the user's "Use Cmdr's fast direct connection" choice for a share, and
/// persists it. Every "Connect directly" calls this with `true`: asking for the direct
/// session is consent to it, so the switch can't strand someone in a state they can't
/// click their way out of.
///
/// ❗ Switching OFF goes through `smb_direct_switch::set_direct_connection_for`, which
/// also withdraws the share's slow-connection notice; calling this with `false`
/// directly would leave that notice offering what the user just opted out of.
pub fn set_direct_connection_enabled(server_name: &str, share: &str, enabled: bool) {
    let hosts = crate::network::fresh_discovered_hosts();
    {
        let mut store = get_known_shares_mutex().lock_ignore_poison();
        apply_choice(
            &mut store.direct_connection_opt_outs,
            server_name,
            share,
            enabled,
            &hosts,
        );
    }
    save_known_shares();
}

#[cfg(test)]
#[path = "known_shares_test.rs"]
mod tests;
