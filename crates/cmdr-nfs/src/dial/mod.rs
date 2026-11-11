//! The dial: from an [`NfsServer`] and a typed export path to a mounted export
//! with a live NFS connection, every refusal a typed [`NfsConnectError`].
//!
//! resolve → portmap `GETPORT` for MOUNT v3 and NFS v3 over TCP → `EXPORT` and
//! `MNT` as `AUTH_SYS` (the export/folder split) → `FSINFO` and `GETATTR` on the
//! root. The portmapper is asked from an ephemeral port; MOUNT and NFS from a
//! reserved one.
//!
//! - ❗ **A `secure` export's refusal names the port when we had none.** knfsd
//!   answers a non-reserved source port with `MNT3ERR_ACCES` on `MNT` and
//!   `NFS3ERR_PERM` on every later call (measured against the fixture,
//!   `apps/desktop/test/nfs-servers/README.md` § "Reaching /secure"), others
//!   with `AUTH_TOOWEAK`. Each of those on a connection whose reserved port was
//!   [`ReservedPort::Unavailable`] is [`NfsConnectError::ReservedPortRequired`];
//!   from a reserved port it's [`NfsConnectError::ExportRefused`].
//! - ❗ **The split goes by `EXPORT` first.** knfsd mounts a subfolder of an
//!   export too, so `MNT /share/Public/photos` would "succeed" and mint a place
//!   per folder. The longest listed export the typed path is at or under wins,
//!   and a listing with none that fits is `ExportNotFound`; only when the server
//!   won't list does `MNT` walk ever-shorter prefixes.

mod platform;

use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use log::debug;
use tokio_util::sync::CancellationToken;

use crate::errors::NfsConnectError;
use crate::params::{NfsIdentity, NfsServer};
use crate::proto::mount::{self, MOUNT_V3, MountStat, Mounted};
use crate::proto::nfs3::{Fattr3, FileHandle, FsInfo, NFS_V3, Nfs3Client, NfsError, NfsStat3};
use crate::proto::portmap::{self, PORTMAP, PORTMAP_PORT};
use crate::proto::rpc::{AUTH_SYS, AcceptError, AuthStat, AuthSys, Credential, Program, Rejection};
use crate::transport::{CallError, ReservedPort, RpcConnection, SourcePort, TransportOptions, random_u32};

/// The budgets a dial runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DialOptions {
    /// One TCP connect, per address.
    pub(crate) connect_timeout: Duration,
    /// The whole dial, from resolving to the root's attributes.
    pub(crate) budget: Duration,
    /// Every metadata and mutation call on the connection the dial hands back.
    pub(crate) call_budget: Duration,
    /// The transport's cap and timings.
    pub(crate) transport: TransportOptions,
}

impl DialOptions {
    /// 5 s per connect, 15 s for the whole dial, 60 s per metadata call.
    pub(crate) const PRODUCTION: Self = Self {
        connect_timeout: Duration::from_secs(5),
        budget: Duration::from_secs(15),
        call_budget: Duration::from_secs(60),
        transport: TransportOptions::PRODUCTION,
    };
}

/// Where a server's MOUNT and NFS programs listen, as portmap answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServerAddresses {
    /// MOUNT v3 over TCP.
    pub(crate) mount: SocketAddr,
    /// NFS v3 over TCP.
    pub(crate) nfs: SocketAddr,
}

/// A mounted export: the live NFS client, and everything a redial needs.
pub(crate) struct MountedExport {
    /// Typed NFSv3 calls on the live connection.
    pub(crate) nfs: Nfs3Client,
    /// Where to dial again. ❗ An NFS redial needs no new `MNT`: v3 is
    /// stateless and the root handle outlives the connection.
    pub(crate) addresses: ServerAddresses,
    /// The `AUTH_SYS` credential every call carries.
    pub(crate) credential: Credential,
    /// The export, canonical, as the split resolved it.
    pub(crate) export: String,
    /// The rest of the typed path (`/photos`), or `None` when it named the
    /// export itself.
    pub(crate) folder: Option<String>,
    /// The export's root handle.
    pub(crate) root: FileHandle,
    /// The server's transfer sizes.
    pub(crate) fsinfo: FsInfo,
    /// The root's attributes, as the dial saw them.
    pub(crate) root_attributes: Fattr3,
}

/// The `AUTH_SYS` credential for `identity`: this Mac's name, the uid, and the
/// gid, and the place's groups (`NfsIdentity::credential_gids`). A file readable
/// only through a group not listed answers `PermissionDenied`.
pub(crate) fn credential_for(identity: &NfsIdentity) -> Credential {
    Credential::sys(&AuthSys {
        stamp: random_u32(),
        machine_name: platform::machine_name(),
        uid: identity.uid,
        gid: identity.gid,
        gids: identity.credential_gids(),
    })
}

/// The uid and gid this process runs as: who `EXPORT` asks as.
fn this_process() -> NfsIdentity {
    let (uid, gid) = platform::process_ids();
    NfsIdentity::new(uid, gid)
}

/// Runs `work` inside the dial's budget, ending at once on `cancel`.
async fn bounded<T>(
    options: &DialOptions,
    cancel: &CancellationToken,
    work: impl Future<Output = Result<T, NfsConnectError>>,
) -> Result<T, NfsConnectError> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(NfsConnectError::Cancelled),
        outcome = tokio::time::timeout(options.budget, work) => outcome.unwrap_or(Err(NfsConnectError::TimedOut)),
    }
}

/// The export paths `server` lists, canonical, the non-UTF-8 ones left out.
/// A refusal is [`NfsConnectError::ListingRefused`].
pub(crate) async fn list_exports(
    server: &NfsServer,
    options: &DialOptions,
    cancel: &CancellationToken,
) -> Result<Vec<String>, NfsConnectError> {
    bounded(options, cancel, async {
        let addresses = locate(server, options).await?;
        let conn = connect(
            addresses.mount,
            MOUNT_V3,
            credential_for(&this_process()),
            SourcePort::Reserved,
            options,
        )
        .await?;
        let listed = mount::export(&conn, Some(options.call_budget)).await;
        conn.close();
        match listed {
            Ok(exports) => Ok(canonical_exports(exports)),
            Err(err) if is_transport(&err) => Err(call_failure(&err, conn.reserved_port())),
            Err(err) => {
                debug!(target: "nfs", "the server won't list its exports: {err:?}");
                Err(NfsConnectError::ListingRefused)
            }
        }
    })
    .await
}

/// Mounts the export `typed` is at or under, as `identity`, and opens the NFS
/// connection on it.
pub(crate) async fn mount(
    server: &NfsServer,
    typed: &str,
    identity: &NfsIdentity,
    options: &DialOptions,
    cancel: &CancellationToken,
) -> Result<MountedExport, NfsConnectError> {
    bounded(options, cancel, async {
        let addresses = locate(server, options).await?;
        let credential = credential_for(identity);
        let mount_conn = connect(
            addresses.mount,
            MOUNT_V3,
            credential.clone(),
            SourcePort::Reserved,
            options,
        )
        .await?;
        let typed = cmdr_fs::volume::canonical_nfs_export(typed);
        let resolved = resolve_and_mount(&mount_conn, &typed, options).await;
        mount_conn.close();
        let (export, mounted) = resolved?;
        if !accepts_auth_sys(&mounted.auth_flavors) {
            debug!(target: "nfs", "the export takes only flavors {:?}", mounted.auth_flavors);
            return Err(NfsConnectError::ExportRefused);
        }

        let nfs = connect_nfs(addresses.nfs, credential.clone(), options).await?;
        let reserved = nfs.connection().reserved_port();
        let fsinfo = nfs
            .fsinfo(&mounted.root)
            .await
            .map_err(|err| nfs_failure(&err, reserved))?;
        let root_attributes = nfs
            .getattr(&mounted.root)
            .await
            .map_err(|err| nfs_failure(&err, reserved))?;
        Ok(MountedExport {
            nfs,
            addresses,
            credential,
            folder: folder_under(&export, &typed),
            export,
            root: mounted.root,
            fsinfo,
            root_attributes,
        })
    })
    .await
}

/// A fresh NFS connection to an export already mounted: the reconnect path.
pub(crate) async fn connect_nfs(
    nfs: SocketAddr,
    credential: Credential,
    options: &DialOptions,
) -> Result<Nfs3Client, NfsConnectError> {
    let conn = connect(nfs, NFS_V3, credential, SourcePort::Reserved, options).await?;
    Ok(Nfs3Client::new(conn, Some(options.call_budget)))
}

/// `UMNT` of `export`, best effort: NFSv3 is stateless, and the server's mount
/// table is only bookkeeping.
pub(crate) async fn unmount(mount_at: SocketAddr, export: &str, credential: Credential, options: &DialOptions) {
    let outcome = async {
        let conn = connect(mount_at, MOUNT_V3, credential, SourcePort::Reserved, options).await?;
        let answered = mount::umnt(&conn, export.as_bytes(), Some(options.call_budget)).await;
        conn.close();
        answered.map_err(|err| call_failure(&err, ReservedPort::NotAsked))
    };
    if let Err(err) = tokio::time::timeout(options.budget, outcome)
        .await
        .unwrap_or(Err(NfsConnectError::TimedOut))
    {
        debug!(target: "nfs", "UMNT didn't go through: {err:?}");
    }
}

/// Asks portmap on each of the host's addresses, in order, for the MOUNT and
/// NFS ports.
async fn locate(server: &NfsServer, options: &DialOptions) -> Result<ServerAddresses, NfsConnectError> {
    let portmap_port = server.portmap_port.unwrap_or(PORTMAP_PORT);
    let host = server.host.trim_start_matches('[').trim_end_matches(']');
    let resolved = tokio::net::lookup_host((host, portmap_port))
        .await
        .map_err(|err| NfsConnectError::Unreachable(format!("the name didn't resolve: {}", err.kind())))?;
    let mut last = NfsConnectError::Unreachable("the name resolved to no address".to_string());
    for portmap_at in resolved {
        let conn = match connect(portmap_at, PORTMAP, Credential::None, SourcePort::Ephemeral, options).await {
            Ok(conn) => conn,
            Err(err) => {
                last = err;
                continue;
            }
        };
        let located = ask_portmap(&conn, portmap_at, server.port, options).await;
        conn.close();
        return located;
    }
    Err(last)
}

async fn ask_portmap(
    conn: &RpcConnection,
    portmap_at: SocketAddr,
    nfs_port: Option<u16>,
    options: &DialOptions,
) -> Result<ServerAddresses, NfsConnectError> {
    let budget = Some(options.call_budget);
    let getport = |program: Program| async move {
        match portmap::getport(conn, program, budget).await {
            Ok(Some(port)) => Ok(SocketAddr::new(portmap_at.ip(), port)),
            Ok(None) => Err(NfsConnectError::NoNfsV3),
            Err(err) => Err(call_failure(&err, ReservedPort::NotAsked)),
        }
    };
    let mount = getport(MOUNT_V3).await?;
    let nfs = match nfs_port {
        Some(port) => SocketAddr::new(portmap_at.ip(), port),
        None => getport(NFS_V3).await?,
    };
    Ok(ServerAddresses { mount, nfs })
}

/// One connection, inside the per-connect budget.
async fn connect(
    target: SocketAddr,
    program: Program,
    credential: Credential,
    source: SourcePort,
    options: &DialOptions,
) -> Result<RpcConnection, NfsConnectError> {
    let dialed = tokio::time::timeout(
        options.connect_timeout,
        RpcConnection::connect(target, program, credential, source, options.transport),
    )
    .await;
    match dialed {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(err)) => Err(NfsConnectError::Unreachable(format!(
            "the connect failed: {}",
            err.kind()
        ))),
        Err(_) => Err(NfsConnectError::Unreachable("the connect timed out".to_string())),
    }
}

/// Picks the export `typed` belongs to and mounts it: by `EXPORT`'s list when
/// the server gives one that fits, else by `MNT` on ever-shorter prefixes.
async fn resolve_and_mount(
    conn: &RpcConnection,
    typed: &str,
    options: &DialOptions,
) -> Result<(String, Mounted), NfsConnectError> {
    let reserved = conn.reserved_port();
    let budget = Some(options.call_budget);
    let listed = match mount::export(conn, budget).await {
        Ok(exports) => Some(canonical_exports(exports)),
        Err(err) if is_transport(&err) => return Err(call_failure(&err, reserved)),
        Err(err) => {
            debug!(target: "nfs", "the server won't list its exports, so the split walks MNT: {err:?}");
            None
        }
    };
    let candidates = match listed {
        // The server named its exports and none holds the typed path.
        Some(exports) => match longest_export(&exports, typed) {
            Some(export) => vec![export.to_string()],
            None => return Err(NfsConnectError::ExportNotFound),
        },
        None => prefixes(typed),
    };
    // ❗ knfsd answers `MNT3ERR_ACCES` for a path it doesn't export, so on the
    // walk a refusal moves on to the next prefix too, and only decides the
    // answer when no prefix mounts.
    let mut refused = false;
    for candidate in candidates {
        match mount::mnt(conn, candidate.as_bytes(), budget).await {
            Ok(Ok(mounted)) => return Ok((candidate, mounted)),
            Ok(Err(MountStat::NoEnt | MountStat::NotDir)) => {}
            Ok(Err(MountStat::Access | MountStat::Perm)) => refused = true,
            Ok(Err(other)) => return Err(NfsConnectError::Transport(format!("MNT answered {other:?}"))),
            Err(err) => return Err(call_failure(&err, reserved)),
        }
    }
    Err(if refused {
        refusal(reserved)
    } else {
        NfsConnectError::ExportNotFound
    })
}

/// The listed exports as canonical strings, a name that isn't UTF-8 left out.
fn canonical_exports(exports: Vec<Bytes>) -> Vec<String> {
    let mut skipped = 0;
    let canonical = exports
        .into_iter()
        .filter_map(|raw| match std::str::from_utf8(&raw) {
            Ok(path) => Some(cmdr_fs::volume::canonical_nfs_export(path)),
            Err(_) => {
                skipped += 1;
                None
            }
        })
        .collect();
    if skipped > 0 {
        debug!(target: "nfs", "left out {skipped} export(s) whose name isn't UTF-8");
    }
    canonical
}

/// Whether `typed` is `export` or under it. Both canonical.
pub(crate) fn is_at_or_under(typed: &str, export: &str) -> bool {
    export == "/" || typed == export || typed.strip_prefix(export).is_some_and(|rest| rest.starts_with('/'))
}

/// The longest of `exports` that `typed` is at or under.
pub(crate) fn longest_export<'a>(exports: &'a [String], typed: &str) -> Option<&'a str> {
    exports
        .iter()
        .filter(|export| is_at_or_under(typed, export))
        .max_by_key(|export| export.len())
        .map(String::as_str)
}

/// `typed` and each of its ancestors, longest first, down to `/`.
pub(crate) fn prefixes(typed: &str) -> Vec<String> {
    let mut out = vec![typed.to_string()];
    let mut rest = typed;
    while let Some(cut) = rest.rfind('/') {
        rest = &rest[..cut];
        out.push(if rest.is_empty() {
            "/".to_string()
        } else {
            rest.to_string()
        });
        if rest.is_empty() {
            break;
        }
    }
    out.dedup();
    out
}

/// What's left of `typed` under `export` (`/photos`), or `None` when it's the
/// export itself.
pub(crate) fn folder_under(export: &str, typed: &str) -> Option<String> {
    let rest = if export == "/" {
        typed
    } else {
        &typed[export.len().min(typed.len())..]
    };
    (!rest.is_empty() && rest != "/").then(|| rest.to_string())
}

/// An empty list is a server that names none, which the Linux client (and so
/// every NAS) reads as `AUTH_SYS`.
fn accepts_auth_sys(flavors: &[u32]) -> bool {
    flavors.is_empty() || flavors.contains(&AUTH_SYS)
}

/// The export refused us: for the port's sake when we had no reserved one.
fn refusal(reserved: ReservedPort) -> NfsConnectError {
    match reserved {
        ReservedPort::Bound(_) => NfsConnectError::ExportRefused,
        ReservedPort::Unavailable | ReservedPort::NotAsked => NfsConnectError::ReservedPortRequired,
    }
}

/// Whether a call failure is the connection's (so the dial stops) rather
/// than the server declining this one procedure.
fn is_transport(err: &CallError) -> bool {
    matches!(
        err,
        CallError::Disconnected(_) | CallError::TimedOut | CallError::Unanswered | CallError::Retired
    )
}

/// A failed call during the dial, by number.
pub(crate) fn call_failure(err: &CallError, reserved: ReservedPort) -> NfsConnectError {
    match err {
        CallError::Disconnected(why) => NfsConnectError::Unreachable(why.clone()),
        CallError::TimedOut => NfsConnectError::TimedOut,
        CallError::NotAccepted(AcceptError::ProgramUnavailable | AcceptError::ProgramMismatch { .. }) => {
            NfsConnectError::NoNfsV3
        }
        CallError::Denied(Rejection::Auth(AuthStat::TooWeak | AuthStat::BadCredential)) => refusal(reserved),
        CallError::Denied(Rejection::Auth(_)) => NfsConnectError::ExportRefused,
        CallError::Unanswered
        | CallError::Retired
        | CallError::NotAccepted(_)
        | CallError::Denied(Rejection::RpcMismatch { .. })
        | CallError::Decode(_) => NfsConnectError::Transport(format!("{err:?}")),
    }
}

/// A failed NFS call during the dial, by number.
pub(crate) fn nfs_failure(err: &NfsError, reserved: ReservedPort) -> NfsConnectError {
    match err {
        NfsError::Status(NfsStat3::Perm | NfsStat3::Access) => refusal(reserved),
        NfsError::Status(NfsStat3::NoEnt | NfsStat3::Stale | NfsStat3::BadHandle) => NfsConnectError::ExportNotFound,
        NfsError::Status(status) => NfsConnectError::Transport(format!("the server answered {status:?}")),
        NfsError::Call(err) => call_failure(err, reserved),
        NfsError::NoProgress => NfsConnectError::Transport("no progress".to_string()),
    }
}

#[cfg(test)]
#[path = "dial_test.rs"]
mod dial_test;

#[cfg(test)]
#[path = "dial_fixture_test.rs"]
mod dial_fixture_test;
