//! Tauri commands for network host discovery and SMB share listing.

use crate::file_system::volume::reconnect_error::ReconnectError;
use crate::network::smb_sign_in_diagnostics::{CredentialSource, log_sign_in_refusal};
use crate::network::{
    DiscoveryState, NetworkHost, ShareListError, ShareListResult, cached_discovered_hosts, get_discovery_state_value,
    get_host_for_resolution, resolve_host_ip, service_name_to_hostname, smb_client, update_host_resolution,
};

use crate::network::os_mount_notice::FallbackNotice;
use crate::network::smb_connect_directly::{self, UpgradeResult};
use crate::network::smb_direct_switch::{self, DirectConnectionSwitch};
use crate::network::smb_upgrade::register_smb_volume;

/// Gets every host discovery knows, including ones a stopped browse found: the
/// Servers view shows them straight away while a fresh browse runs.
#[tauri::command]
#[specta::specta]
pub fn list_network_hosts() -> Vec<NetworkHost> {
    cached_discovered_hosts()
}

/// Gets the current discovery state.
#[tauri::command]
#[specta::specta]
pub fn get_network_discovery_state() -> DiscoveryState {
    get_discovery_state_value()
}

/// Resolves a network host by ID, returning the host with hostname and IP address populated.
/// This is an async command that uses spawn_blocking for the DNS lookup to avoid blocking
/// the main thread pool. Multiple hosts can resolve in parallel.
#[tauri::command]
#[specta::specta]
pub async fn resolve_host(host_id: String) -> Option<NetworkHost> {
    // Get host info (brief mutex hold)
    let info = get_host_for_resolution(&host_id)?;

    // If already resolved, return current state quickly
    if info.ip_address.is_some() {
        return Some(NetworkHost {
            id: info.id,
            name: info.name,
            hostname: info.hostname,
            ip_address: info.ip_address,
            port: info.port,
            source: info.source,
        });
    }

    // Generate hostname
    let hostname = info.hostname.unwrap_or_else(|| service_name_to_hostname(&info.name));
    let hostname_clone = hostname.clone();

    // Do DNS resolution in a blocking task (this is the slow part - runs on separate thread)
    let ip_address = tokio::task::spawn_blocking(move || resolve_host_ip(&hostname_clone))
        .await
        .ok()
        .flatten();

    // Update host with results (brief mutex hold)
    update_host_resolution(&host_id, hostname, ip_address)
}

/// Lists shares available on a network host.
///
/// Returns cached results if available, otherwise queries the host.
/// Attempts guest access first; returns an error if authentication is required.
///
/// # Arguments
/// * `host_id` - Unique identifier for the host (used for caching)
/// * `hostname` - Hostname to connect to (for example, "TEST_SERVER.local")
/// * `ip_address` - Optional resolved IP address (preferred over hostname for reliability)
/// * `port` - SMB port (default 445, but Docker containers may use different ports)
/// * `timeout_ms` - Optional timeout in milliseconds (default: 15000)
/// * `cache_ttl_ms` - Optional cache TTL in milliseconds (default: 30000)
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn list_shares_on_host(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
    app_handle: tauri::AppHandle,
) -> Result<ShareListResult, ShareListError> {
    let (guest, account) = account_listing_for(&app_handle, &hostname, ip_address.as_deref(), port);
    let credentials = account.as_ref().map(|c| (c.username.as_str(), c.password.as_str()));
    let result = smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        credentials,
        guest,
        timeout_ms,
        cache_ttl_ms,
    )
    .await;
    // The account's password is this session's copy of a saved entry.
    log_sign_in_refusal(&hostname, port, credentials, CredentialSource::Saved, &result);
    result
}

/// How a listing that got no credentials signs in: guest first, unless the person set
/// an account for this host, in which case as that account with the password this
/// session already read (`keychain::cached_credentials`, ❌ never a Keychain read), or
/// not at all (`AuthRequired`, which the frontend takes to the Keychain and the sheet).
///
/// ❗ Without the cached leg, "Sign in as…" didn't stick: the next background listing
/// had no password to offer and came back as nothing, where it used to come back guest.
fn account_listing_for(
    app: &tauri::AppHandle,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> (smb_client::GuestAttempt, Option<SmbCredentials>) {
    use crate::network::server_identity::{SmbServer, smb_server};

    let typed = manual_servers::typed_username(app, &SmbServer::new(hostname, port))
        .or_else(|| ip_address.and_then(|ip| manual_servers::typed_username(app, &SmbServer::new(ip, port))));
    let Some(username) = typed else {
        return (smb_client::GuestAttempt::Try, None);
    };
    let cached = std::iter::once(hostname)
        .chain(ip_address)
        .find_map(|name| keychain::cached_credentials(&smb_server(name, port)))
        .filter(|creds| creds.username == username);
    (smb_client::GuestAttempt::Skip, cached)
}

/// Whether a listing of this host may try guest: not when the person typed an
/// account for it (`manual_servers::typed_username`), by either name it goes by.
fn guest_attempt_for(
    app: &tauri::AppHandle,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> smb_client::GuestAttempt {
    use crate::network::server_identity::SmbServer;

    let typed = manual_servers::typed_username(app, &SmbServer::new(hostname, port))
        .or_else(|| ip_address.and_then(|ip| manual_servers::typed_username(app, &SmbServer::new(ip, port))));
    if typed.is_some() {
        smb_client::GuestAttempt::Skip
    } else {
        smb_client::GuestAttempt::Try
    }
}

/// Prefetches shares for a host, so its share list is cached by the time someone opens it.
/// Same as list_shares_on_host but designed for prefetching - errors are silently ignored.
/// Returns immediately if shares are already cached.
///
/// Listing signs in to the host (as a guest, where it lets one in), so the frontend calls
/// this for servers the user saved, only while a Servers view is on screen, and never for
/// one that was only discovered.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn prefetch_shares(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
    app_handle: tauri::AppHandle,
) {
    // Fire and forget - we don't care about the result for prefetching
    let (guest, account) = account_listing_for(&app_handle, &hostname, ip_address.as_deref(), port);
    let _ = smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        account.as_ref().map(|c| (c.username.as_str(), c.password.as_str())),
        guest,
        timeout_ms,
        cache_ttl_ms,
    )
    .await;
}

// --- Known Shares Commands ---

use crate::network::known_shares::{
    self, AuthOptions, ConnectionMode, KnownNetworkShare, get_known_share as get_known_share_inner,
};

/// Gets a specific known share by server and share name.
#[tauri::command]
#[specta::specta]
pub fn get_known_share_by_name(server_name: String, share_name: String) -> Option<KnownNetworkShare> {
    get_known_share_inner(&server_name, &share_name)
}

/// Updates or adds a known network share after successful connection.
#[tauri::command]
#[specta::specta]
pub fn update_known_share(
    server_name: String,
    share_name: String,
    last_connection_mode: ConnectionMode,
    last_known_auth_options: AuthOptions,
    username: Option<String>,
) {
    let share = KnownNetworkShare {
        server_name,
        share_name,
        protocol: "smb".to_string(),
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        last_connection_mode,
        last_known_auth_options,
        username,
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
    };

    known_shares::update_known_share(share);
}

/// The username to pre-fill for `server_name`, or `None` if it has never been signed in to.
///
/// Takes the server by name and answers for that one server, rather than handing back a
/// map the caller has to key into: the identity rule lives in `known_shares`, not in the
/// IPC contract. See `known_shares::get_username_hint`.
///
/// ❗ The account the person TYPED for this host (the add or edit sheet) wins over the
/// share history: it is a stated preference, where the history is only who signed in
/// last, and Edit is where it changes.
#[tauri::command]
#[specta::specta]
pub fn get_username_hint(server_name: String, app_handle: tauri::AppHandle) -> Option<String> {
    // `server_name` is the discovery name, which spells the port off 445 (`localhost:11482`).
    let server = crate::network::server_identity::SmbServer::from_name(&server_name);
    manual_servers::typed_username(&app_handle, &server).or_else(|| known_shares::get_username_hint(&server_name))
}

// --- Keychain Commands ---

use crate::network::keychain::{self, KeychainError, SmbCredentials};

/// Saves SMB credentials to the Keychain.
/// Credentials are stored under "Cmdr" service name.
#[tauri::command]
#[specta::specta]
pub fn save_smb_credentials(
    server: String,
    share: Option<String>,
    username: String,
    password: String,
) -> Result<(), KeychainError> {
    keychain::save_credentials(&server, share.as_deref(), &username, &password)
}

/// Retrieves SMB credentials from the Keychain.
/// Returns the stored username and password if found.
#[tauri::command]
#[specta::specta]
pub fn get_smb_credentials(server: String, share: Option<String>) -> Result<SmbCredentials, KeychainError> {
    keychain::get_credentials(&server, share.as_deref())
}

/// Whether a server-level password was already read this session, from the
/// in-memory cache only. ❗ Never touches the Keychain (`keychain::has_cached_credentials`):
/// the share list uses it to offer "Forget saved password" without costing a prompt.
#[tauri::command]
#[specta::specta]
pub fn has_cached_smb_credentials(server: String) -> bool {
    keychain::has_cached_credentials(&server)
}

/// Deletes SMB credentials from the Keychain.
#[tauri::command]
#[specta::specta]
pub fn delete_smb_credentials(server: String, share: Option<String>) -> Result<(), KeychainError> {
    keychain::delete_credentials(&server, share.as_deref())
}

/// Returns whether credential storage is using an encrypted file fallback
/// instead of the system keyring. The frontend can use this to show a one-time
/// info toast when the user first saves credentials without a system keyring.
#[tauri::command]
#[specta::specta]
pub fn is_using_credential_file_fallback() -> bool {
    crate::secrets::is_file_backed()
}

/// Lists shares on a host using stored or provided credentials.
/// This is the main command for authenticated share listing.
///
/// # Arguments
/// * `host_id` - Unique identifier for the host (used for caching)
/// * `hostname` - Hostname to connect to
/// * `ip_address` - Optional resolved IP address
/// * `port` - SMB port
/// * `username` - Username for authentication (or None for guest)
/// * `password` - Password for authentication (or None for guest)
/// * `credential_source` - Whether the credentials were typed for this attempt or read from a
///   saved entry, for the refusal log line
/// * `timeout_ms` - Optional timeout in milliseconds (default: 15000)
/// * `cache_ttl_ms` - Optional cache TTL in milliseconds (default: 30000)
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn list_shares_with_credentials(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    credential_source: CredentialSource,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
    app_handle: tauri::AppHandle,
) -> Result<ShareListResult, ShareListError> {
    let credentials = match (username, password) {
        (Some(u), Some(p)) => Some((u, p)),
        _ => None,
    };
    let credentials = credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));

    let result = smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        credentials,
        guest_attempt_for(&app_handle, &hostname, ip_address.as_deref(), port),
        timeout_ms,
        cache_ttl_ms,
    )
    .await;
    log_sign_in_refusal(&hostname, port, credentials, credential_source, &result);
    result
}

// --- Mount Commands ---

use crate::network::mount::{self, MountError, MountResult};

/// Mounts an SMB share to the local filesystem.
///
/// Attempts to mount the specified share on the server. If credentials are
/// provided, they are used for authentication. If the share is already mounted,
/// returns the existing mount path without re-mounting.
///
/// After a successful OS mount, also establishes a direct smb2 connection and
/// registers the share as an `SmbVolume` in the `VolumeManager`. This means
/// Cmdr's own file operations go through smb2 (fast), while Finder/Terminal
/// use the OS mount (compatible). If smb2 connection fails, the volume falls
/// through to a regular `LocalPosixVolume` (registered by the watcher).
///
/// # Arguments
/// * `server` - Server hostname or IP address
/// * `share` - Name of the share to mount
/// * `username` - Optional username for authentication
/// * `password` - Optional password for authentication
/// * `port` - SMB port (default 445)
/// * `timeout_ms` - Optional timeout in milliseconds (default: 20000)
/// * `host_name` - The name the person knows the server by (the discovery list's
///   `name`), which a saved share row is filed under. `None` files it under
///   `server`.
///
/// ❗ A mount that went through is SAVED as a share place, with the account it
/// signed in as (`network::smb_saved_shares`, `docs/specs/saved-smb-shares.md`):
/// this is Cmdr's own mount, which is someone's intent, unlike the mounts the
/// watcher and the upgrade paths see.
///
/// # Returns
/// * `Ok(MountResult)` - Mount successful, with path to mount point
/// * `Err(MountError)` - Mount failed with specific error type
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn mount_network_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
    host_name: Option<String>,
) -> Result<MountResult, MountError> {
    let actual_port = port.unwrap_or(445);
    let result = crate::network::mount_share(
        server.clone(),
        share.clone(),
        username.clone(),
        password.clone(),
        actual_port,
        timeout_ms,
    )
    .await?;

    // Try to establish a direct smb2 connection and register as SmbVolume.
    // If this fails, the FSEvents watcher will register a LocalPosixVolume
    // as fallback (slower but still functional). Someone just asked for this
    // mount, so a fallback is worth telling them about.
    register_smb_volume(
        &server,
        &share,
        &result.mount_path,
        username.as_deref(),
        password.as_deref(),
        actual_port,
        FallbackNotice::Announce,
    )
    .await;

    crate::network::smb_saved_shares::remember_mount(crate::network::smb_saved_shares::MountedShare {
        host_name: host_name.as_deref().unwrap_or(&server),
        address: &server,
        port: actual_port,
        share: &share,
        username: username.as_deref(),
        mount_path: &result.mount_path,
    })
    .await;

    Ok(result)
}

/// Upgrades an existing OS-mounted SMB volume to use a direct smb2 connection, with
/// the credentials Cmdr stored for its share.
///
/// Called from the "Connect directly for faster access" UI action. Every answer,
/// a volume that's gone or isn't an SMB mount included:
/// `network::smb_connect_directly::UpgradeResult`.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume(volume_id: String) -> UpgradeResult {
    smb_connect_directly::connect_directly(&volume_id).await
}

/// Whether the SMB share behind `volume_id` may use Cmdr's fast direct connection
/// (the per-share switch), or `None` when there's no SMB share behind it to ask
/// about. See `network::smb_direct_switch`.
#[tauri::command]
#[specta::specta]
pub async fn get_smb_direct_connection_enabled(volume_id: String) -> Option<bool> {
    smb_direct_switch::direct_connection_enabled_for(&volume_id).await
}

/// Switches the SMB share behind `volume_id` onto or off Cmdr's fast direct
/// connection. Off on a direct share hands it back to the macOS mount now; on only
/// saves, and the caller runs "Connect directly". See `network::smb_direct_switch`.
#[tauri::command]
#[specta::specta]
pub async fn set_smb_direct_connection_enabled(volume_id: String, enabled: bool) -> DirectConnectionSwitch {
    smb_direct_switch::set_direct_connection_for(&volume_id, enabled).await
}

// Per-drive indexing enable/disable/rescan lives in `commands/indexing.rs` as a
// single drive-type-agnostic surface (`enable_drive_index` / `disable_drive_index`
// / `rescan_drive_index`), so the freshness UX drives any drive (local or SMB)
// through one set of commands. The SMB-specific gate + typed `SmbIndexGateReason`
// it surfaces still live in `indexing::start_indexing_for_smb`.

/// Upgrades an existing OS-mounted SMB volume using explicit credentials.
///
/// Called with what the user typed into the sign-in sheet `upgrade_to_smb_volume`
/// led to.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_with_credentials(
    volume_id: String,
    username: Option<String>,
    password: Option<String>,
    remember_in_keychain: bool,
) -> UpgradeResult {
    smb_connect_directly::connect_directly_with_credentials(&volume_id, username, password, remember_in_keychain).await
}

/// Does the system (login) keychain hold an SMB password another app (Finder) saved for
/// this volume's server? Attributes-only probe — **never triggers the consent dialog** —
/// so the frontend can decide whether to offer the "Use the password macOS saved"
/// affordance. macOS-only; returns `false` everywhere else.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(volume_id: String) -> Result<bool, String> {
    Ok(smb_connect_directly::system_has_saved_password(&volume_id).await)
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(_volume_id: String) -> Result<bool, String> {
    Ok(false)
}

/// Upgrades an OS-mounted SMB volume to a direct smb2 connection using the password
/// another app (Finder) already saved in the login keychain, so the user doesn't retype
/// it. Reading it raises the macOS consent dialog, so this is **user-initiated only**:
/// never call it at startup. Where there's no system keychain, it asks for the password.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_using_saved_password(volume_id: String) -> UpgradeResult {
    smb_connect_directly::connect_directly_with_system_saved_password(&volume_id).await
}

// --- Disconnect Command ---

/// Unmounts all SMB shares mounted from `host`, answering the mount paths that went.
/// Uses a 15s timeout because `statfs` on hung mounts can block indefinitely
/// and `diskutil unmount` may wait for the OS to release the mount.
///
/// ❗ Takes the whole host, because what identifies its mounts is where it dials:
/// every name it goes by (hostname, IP, the discovery name's host half), each on
/// ITS port. A display label matched nothing `statfs` says, and a name without the
/// port would take another server's mounts on the same machine.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_network_host(host: NetworkHost) -> Result<Vec<String>, String> {
    use crate::deadline::blocking_with_timeout;
    use std::time::Duration;

    // Drop the cached share list so a later browse re-fetches fresh shares and
    // auth mode rather than serving a stale (up to 30 s TTL) entry for a host
    // the user just disconnected from.
    smb_client::invalidate_cache(&host.id);

    let targets = crate::network::server_identity::smb_servers_of(&host);
    let result = blocking_with_timeout(Duration::from_secs(15), vec![], move || {
        mount::unmount_smb_shares_from_host(&targets)
    })
    .await;

    Ok(result)
}

// --- Volume reconnect and sign-in ---
//
// Backend-neutral despite two of the three names: each one asks whatever volume
// is registered under the id, so an SFTP volume goes through them unchanged. The
// frontend's reconnect manager calls all three and never branches on backend.

/// What FORM a "Sign in" affordance on this volume takes, right now: which fields
/// the sheet renders, and whether the username among them is editable.
///
/// ❗ **Asked when the affordance renders, ❌ never carried on a connect result.**
/// A backend that authenticates per connection can prove itself with a different
/// credential each time it dials, so an answer captured when the volume was
/// opened describes a session that may already be gone. This reads the live
/// volume, which is the only moment the answer is true.
///
/// A volume with no sign-in story of its own, and an id nothing is registered
/// under, both answer `Password` (`Volume::sign_in_prompt`'s default): the one
/// safe way to be wrong here is a needless password box, because a wrong
/// `Nothing` is a volume the user can't sign in to at all.
///
/// ❗ The answer is a tagged union, so the frontend switches on `kind` and
/// ❌ never derives the form from the protocol, the mode the sheet is in, or the
/// rung a connect result mentioned.
#[tauri::command]
#[specta::specta]
// No timeout wrapper: this reads a lock and a map, and reaches no device.
pub async fn get_volume_sign_in_state(volume_id: String) -> cmdr_fs::volume::SignInShape {
    use crate::file_system::volume::manager::get_volume_manager;

    match get_volume_manager().get(&volume_id) {
        Some(volume) => volume.sign_in_prompt(),
        // ❗ A saved SMB share not mounted right now: the share is the place, and
        // the account is a field on it (`docs/specs/saved-smb-shares.md`). No
        // guest, since an unauthenticated mount is what asking follows from.
        None if known_shares::share_by_volume_id(&volume_id).is_some() => {
            cmdr_fs::volume::SignInShape::UsernamePassword { guest_allowed: false }
        }
        None => cmdr_fs::volume::SignInShape::Password,
    }
}

/// Tries to rebuild a Disconnected volume's session in place.
///
/// ❗ Backend-neutral: every remote backend implements `attempt_reconnect`, and
/// the frontend's reconnect manager drives SMB, SFTP, and WebDAV through this
/// one command. Called on each backoff tick (and on "Retry now" / lazy nav-time
/// retry). The backend single-flights concurrent calls, so the frontend is free
/// to fire on its own schedule. Returns `Ok(())` on success (the volume now
/// reports `Direct`), or a typed [`ReconnectError`] saying why the rebuild
/// didn't happen.
///
/// A volume whose backend can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported` (the trait default).
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume(volume_id: String) -> Result<(), ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume.attempt_reconnect().await.map_err(ReconnectError::from)
}

/// Reconnects a volume with freshly-entered credentials.
///
/// ❗ Backend-neutral, like [`reconnect_volume`]. Invoked by the "Sign in"
/// affordance shown when an in-place reconnect gave up on an auth failure (a
/// `needs_credentials` `volume-connection-changed` event). The volume refreshes
/// what it has stored (so future reconnects are silent) and runs the standard
/// reconnect; on success the backend emits
/// `volume-connection-changed { state: "connected" }`.
///
/// ❗ Whether the USERNAME may change is the backend's call, not this command's:
/// SMB accepts a new one and rewrites its params, SFTP and WebDAV refuse because
/// the volume id IS the account. `SignInShape` is what tells the sheet which it
/// is. A backend that can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported`.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume_with_credentials(
    volume_id: String,
    username: String,
    password: String,
) -> Result<(), ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume
        .reconnect_with_credentials(username, password)
        .await
        .map_err(ReconnectError::from)
}

/// Disconnects a single SMB volume by tearing down its OS mount.
///
/// Thin delegate to [`crate::file_system::volume::eject::disconnect_smb`]; the
/// typed `EjectError` IS the wire type, so it crosses unchanged. Called by the
/// "Disconnect" button in the pane's reconnect view and the gave-up
/// `VolumeUnreachableBanner`.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_smb_volume(volume_id: String) -> Result<(), crate::file_system::volume::eject::EjectError> {
    crate::file_system::volume::eject::disconnect_smb(&volume_id).await
}

// --- Manual Server Commands ---

use crate::network::manual_servers::{self, ManualConnectResult};

/// Connects to a manually-specified server: parses, checks reachability, persists, and injects.
///
/// `name` is the Add form's Name field; `None` or empty leaves the server unnamed, so
/// the UI calls it by its address. `username` is the account the person means to sign
/// in as (else the one an `smb://user@host` address names): it prefills the first
/// sign-in, and a host with one is never listed as guest.
///
/// `check_reachability: false` is the sheet's "Add anyway", offered only after a check
/// answered `Unreachable`: it saves without probing. An address that doesn't parse is
/// refused either way.
#[tauri::command]
#[specta::specta]
pub async fn connect_to_server(
    address: String,
    name: Option<String>,
    username: Option<String>,
    check_reachability: bool,
    app_handle: tauri::AppHandle,
) -> Result<ManualConnectResult, manual_servers::AddServerError> {
    let details = manual_servers::HostEdit {
        name: name.unwrap_or_default(),
        username,
    };
    let reachability = if check_reachability {
        manual_servers::Reachability::Check
    } else {
        manual_servers::Reachability::Skip
    };
    manual_servers::add_manual_server(&address, &details, reachability, &app_handle).await
}

/// Sets the account the SMB server `server_name` (its discovery name, `host` or
/// `host:port`) is used with: "Sign in as…" in its share list, or `None` for guest.
/// The same preference a typed username is, so the listing stops trying guest and
/// signs in as it. Answers whether the store could be written.
#[tauri::command]
#[specta::specta]
pub fn set_smb_account_preference(server_name: String, username: Option<String>, app_handle: tauri::AppHandle) -> bool {
    let server = crate::network::server_identity::SmbServer::from_name(&server_name);
    let set = manual_servers::set_account(&server, username.as_deref(), &app_handle);
    if set {
        crate::volume_broadcast::emit_volumes_changed();
    }
    set
}

/// The user took a network action: opened the Servers view, "Connect to server…", or
/// upgraded a mounted share to direct smb2. Brings back the manual servers (a
/// toggle-off cleared them) and runs the existing-SMB-mount upgrade pass: if macOS
/// auto-remounted shares at login, this is the first moment we can open direct smb2
/// connections to them (TCP to a private IP gates on the Local Network permission).
///
/// Starts no browse of its own: the Servers view holds one while it's on screen
/// (`set_servers_view_shown`), and the upgrade pass holds one while it resolves.
#[tauri::command]
#[specta::specta]
pub fn note_network_action(app_handle: tauri::AppHandle) {
    manual_servers::load_manual_servers(&app_handle);
    crate::file_system::upgrade_existing_smb_mounts();

    #[cfg(feature = "smb-e2e")]
    crate::network::virtual_smb_hosts::setup_virtual_smb_hosts(&app_handle);
}

/// The Servers view (a pane on the network volume) is on screen, or no longer is.
/// While it is, the mDNS browse runs, so hosts arriving and leaving show live;
/// `discovery_gate` stops it once nothing else needs it either.
///
/// The frontend sends its whole truth each time (any view shown at all), so a
/// repeat is harmless and a reloaded page corrects whatever the last one left.
#[tauri::command]
#[specta::specta]
pub fn set_servers_view_shown(shown: bool) {
    use crate::ignore_poison::IgnorePoison;
    use crate::network::discovery_gate::{self, DiscoveryLease};
    use std::sync::Mutex;

    static VIEW_LEASE: Mutex<Option<DiscoveryLease>> = Mutex::new(None);
    let mut lease = VIEW_LEASE.lock_ignore_poison();
    match (shown, lease.is_some()) {
        (true, false) => *lease = Some(discovery_gate::hold()),
        (false, true) => *lease = None,
        _ => {}
    }
}

/// Live-apply the `network.enabled` toggle. When `false`, stops mDNS and clears the discovered
/// host list (frontend store empties via emitted `network-host-lost` events). When `true`, the
/// browse resumes if the Servers view is holding it.
#[tauri::command]
#[specta::specta]
pub fn set_network_enabled(enabled: bool, app_handle: tauri::AppHandle) {
    crate::network::set_network_enabled_flag(enabled);
    if !enabled {
        crate::network::clear_discovered_hosts(&app_handle);
    }
}

#[cfg(test)]
#[path = "network_test.rs"]
mod network_test;
