//! Fixtures for the Docker-backed NFS suites, on both sides of the crate
//! boundary. Gated behind the `testing` feature, so it exists in dev targets
//! and in no shipped build. The stack itself, its seeded tree, and why
//! `/secure` is reachable only from a Linux host:
//! `apps/desktop/test/nfs-servers/README.md`.
//!
//! ❗ The portmap port comes from `NFS_FIXTURE_PORTMAP_PORT` HERE, in test
//! code, ❌ never in the crate proper: a real server's portmapper is on 111.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use tokio_util::sync::CancellationToken;

use super::{NfsVolume, connect_nfs_volume};
use crate::errors::NfsConnectError;
use crate::params::{NfsConnectionParams, NfsIdentity, NfsServer};

/// The export any port may reach.
pub const FIXTURE_INSECURE_EXPORT: &str = "/insecure";
/// The export that refuses a source port past 1023 (knfsd's default `secure`).
pub const FIXTURE_SECURE_EXPORT: &str = "/secure";

/// Who owns the seeded tree, and who a cell acts as: root-squash leaves the
/// files to uid 1000.
pub const FIXTURE_IDENTITY: NfsIdentity = NfsIdentity::new(1000, 1000);

/// The file every fixture carries for the byte path to read.
pub const FIXTURE_LARGE_FILE: &str = "large.bin";
/// Its length: 200 MiB (`LARGE_MB` in `seed.sh`).
pub const FIXTURE_LARGE_LEN: u64 = 200 * 1024 * 1024;

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// The fixture server through its published loopback ports.
pub fn fixture_server() -> NfsServer {
    NfsServer {
        host: "127.0.0.1".to_string(),
        port: None,
        portmap_port: Some(env_port("NFS_FIXTURE_PORTMAP_PORT", 15480)),
    }
}

/// The fixture server dialed at the container's own IP, where the source port
/// survives: set by the integration lane on Linux only. `None` everywhere else,
/// which is every macOS run.
pub fn fixture_direct_server() -> Option<NfsServer> {
    let host = std::env::var("NFS_FIXTURE_DIRECT_HOST").ok()?;
    Some(NfsServer {
        host,
        port: None,
        portmap_port: None,
    })
}

/// Params for `export` (or a path under it) on the fixture, as the seeded
/// tree's owner.
pub fn fixture_params(export: &str) -> NfsConnectionParams {
    NfsConnectionParams::new(fixture_server(), export, FIXTURE_IDENTITY)
}

/// A host whose background work runs on the caller's runtime.
pub fn fixture_host() -> VolumeHost {
    VolumeHost::builder().runtime(tokio::runtime::Handle::current()).build()
}

/// Connects to `params` the way the app does.
pub async fn connect(params: NfsConnectionParams) -> Result<NfsVolume, NfsConnectError> {
    connect_nfs_volume("nfs-fixture", params, fixture_host(), CancellationToken::new()).await
}

/// Connects to the fixture's `/insecure` export.
pub async fn connect_insecure() -> NfsVolume {
    connect(fixture_params(FIXTURE_INSECURE_EXPORT))
        .await
        .unwrap_or_else(|err| panic!("the NFS fixture must be up (nfs-servers/start.sh): {err:?}"))
}

/// `remote` (`/photos`) as the app addresses it on `volume`.
pub fn app_path(volume: &NfsVolume, remote: &str) -> PathBuf {
    let remote = remote.trim_start_matches('/');
    if remote.is_empty() {
        return volume.root().to_path_buf();
    }
    Path::new(volume.root()).join(remote)
}

/// What every scratch folder's name starts with, so a cell that reads the
/// export's root can tell the seeded tree from other cells' working space.
///
/// ❗ The one prefix both sides use: [`scratch_dir`] names folders with it, and
/// the root-listing cell leaves them out. The stack is shared by every worktree
/// on the machine, so another run's cell can be mid-flight at the root at any
/// moment, and a cell that panics or hits the nextest cap never reaches its
/// cleanup. A root listing that counted them failed on whichever run happened to
/// overlap.
pub(crate) const SCRATCH_PREFIX: &str = "scratch-";

/// How old a scratch folder has to be before [`scratch_dir`] treats it as left
/// behind and removes it. Far past the slowest cell's nextest cap, so a live
/// cell on another worktree never loses its folder mid-run.
const STALE_SCRATCH_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// A fresh folder of the cell's own at the export's root, already created:
/// every cell shares the one export, and `nextest` runs them side by side.
/// `what` names the cell, so a mess left behind says whose it is.
///
/// Also sweeps scratch folders older than [`STALE_SCRATCH_AFTER`] first: a cell
/// cleans up only when it passes, so a failed or killed one leaves its folder
/// forever otherwise.
pub async fn scratch_dir(volume: &NfsVolume, what: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    sweep_stale_scratch(volume, now).await;
    let nanos = now.as_nanos();
    let dir = app_path(
        volume,
        &format!("/{SCRATCH_PREFIX}{what}-{}-{nanos}", std::process::id()),
    );
    volume
        .create_directory(&dir)
        .await
        .unwrap_or_else(|err| panic!("the scratch folder for {what} must be creatable: {err:?}"));
    dir
}

/// Removes every scratch folder at the export's root that was created more than
/// [`STALE_SCRATCH_AFTER`] before `now`, best effort. Age comes from the name's
/// own timestamp, which `scratch_dir` writes, so a clock on the server doesn't
/// enter into it.
async fn sweep_stale_scratch(volume: &NfsVolume, now: std::time::Duration) {
    let root = app_path(volume, "/");
    let Ok(entries) = volume.list_directory(&root, None).await else {
        return;
    };
    for entry in entries {
        let Some(created) = scratch_created_at(&entry.name) else {
            continue;
        };
        if entry.is_directory && now.saturating_sub(created) > STALE_SCRATCH_AFTER {
            clean_deep(volume, &root.join(&entry.name)).await;
        }
    }
}

/// When a scratch folder named `name` was made, read off its trailing
/// timestamp. `None` for anything [`scratch_dir`] didn't name.
fn scratch_created_at(name: &str) -> Option<std::time::Duration> {
    let rest = name.strip_prefix(SCRATCH_PREFIX)?;
    let nanos: u64 = rest.rsplit('-').next()?.parse().ok()?;
    Some(std::time::Duration::from_nanos(nanos))
}

/// Removes `path` and everything under it, deepest first, best effort.
pub fn clean_deep<'a>(
    volume: &'a NfsVolume,
    path: &'a Path,
) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if let Ok(entries) = volume.list_directory(path, None).await {
            for entry in entries {
                let child = path.join(&entry.name);
                if entry.is_directory && !entry.is_symlink {
                    clean_deep(volume, &child).await;
                } else {
                    let _ = volume.delete(&child).await;
                }
            }
        }
        let _ = volume.delete(path).await;
    })
}

/// What `large.bin` holds at `[offset, offset + len)`: every 16-byte line its
/// own zero-padded number.
pub fn fixture_large_bytes_at(offset: u64, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 16);
    let mut line = offset / 16;
    let skip = (offset % 16) as usize;
    while out.len() < len + skip {
        out.extend_from_slice(format!("{line:015}\n").as_bytes());
        line += 1;
    }
    out.drain(..skip);
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep deletes by this answer, so anything it didn't name has to read
    /// as `None`, a dashed cell name included.
    #[test]
    fn a_scratch_name_says_when_it_was_made_and_nothing_else_does() {
        assert_eq!(
            scratch_created_at("scratch-same-move-68917-1793805685163787000"),
            Some(std::time::Duration::from_nanos(1_793_805_685_163_787_000))
        );
        assert_eq!(scratch_created_at("photos"), None);
        assert_eq!(scratch_created_at("scratch-notes"), None);
        assert_eq!(scratch_created_at("scratchpad-1793805685163787000"), None);
    }
}
