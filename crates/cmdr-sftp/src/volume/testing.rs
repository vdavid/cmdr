//! Fixtures for the Docker-backed SFTP suites, on both sides of the crate
//! boundary.
//!
//! The suites asserting on this backend live in this crate; the ones whose other
//! half is the app's own machinery (the transfer pipeline, the volume registry,
//! the listing cache) live in the app. Both talk to the same container stack, so
//! the plumbing is here rather than duplicated across the seam.
//!
//! Gated behind the `testing` feature, so it exists in dev targets and in no
//! shipped build. The stack itself: `apps/desktop/test/sftp-servers/README.md`.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::host_keys::InMemoryHostKeys;

use super::{SftpConnectOutcome, SftpVolume, connect_sftp_volume};
use crate::params::SftpConnectionParams;
use crate::transport::HostKeyPromptKind;

/// The remote directory every fixture server exports.
pub const FIXTURE_ROOT: &str = "/srv/data";

/// The file every export carries for the byte path to read.
///
/// Self-describing by construction: each 16-byte line holds its own line number,
/// so every position in it says where it belongs. A reader that holes or
/// duplicates a span lands bytes at offsets whose contents no longer match, which
/// is what lets a cell assert byte-exactness without shipping a copy of the file.
/// `LARGE_MB` in the compose file sets its size; 4 MiB everywhere but the bench
/// server.
pub const FIXTURE_LARGE_FILE: &str = "large.bin";

/// What `large.bin` holds, for its first `len` bytes.
pub fn fixture_large_bytes(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut line = 0u64;
    while out.len() < len {
        out.extend_from_slice(format!("{line:015}\n").as_bytes());
        line += 1;
    }
    out.truncate(len);
    out
}

/// Fails with the first offset where two byte runs differ, and what sits around
/// it.
///
/// ❗ Not `assert_eq!` on the two buffers: a 4 MiB mismatch would print 8 MiB of
/// escaped bytes and bury the one number that says what went wrong. A hole from a
/// misadvanced offset shows here as "the line at this offset says it belongs
/// somewhere else".
pub fn assert_same_bytes(read: &[u8], expected: &[u8], what: &str) {
    assert_eq!(
        read.len(),
        expected.len(),
        "{what}: the wrong number of bytes came back"
    );
    let Some(at) = read.iter().zip(expected).position(|(left, right)| left != right) else {
        return;
    };
    let from = at.saturating_sub(16);
    panic!(
        "{what}: the bytes differ from offset {at}\n  read:     {:?}\n  expected: {:?}",
        String::from_utf8_lossy(&read[from..(from + 48).min(read.len())]),
        String::from_utf8_lossy(&expected[from..(from + 48).min(expected.len())]),
    );
}

/// The account every fixture server runs as.
pub const FIXTURE_USER: &str = "ada";
/// Its password, for the rungs that use one.
pub const FIXTURE_PASSWORD: &str = "openthedoor";
/// The passphrase on `sftp-fixture-passphrase`'s private key.
pub const FIXTURE_KEY_PASSPHRASE: &str = "letmein";

/// The host port a fixture service publishes.
///
/// `service` is the suffix after `SFTP_FIXTURE_`, uppercased. The check runner
/// exports the whole range before bring-up; the fallback is the compose file's
/// own default, so a bare `start.sh` works too.
pub fn fixture_port(service: &str, fallback: u16) -> u16 {
    std::env::var(format!("SFTP_FIXTURE_{service}_PORT"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// Params for one fixture service, with the ssh-agent left out.
///
/// ❗ Agent off on purpose: a developer's own agent would answer for the rung a
/// cell is trying to exercise, and the suite would pass without ever testing the
/// rung it names.
pub fn fixture_params(service: &str, fallback_port: u16) -> SftpConnectionParams {
    SftpConnectionParams::new(
        "127.0.0.1",
        fixture_port(service, fallback_port),
        FIXTURE_USER,
        FIXTURE_ROOT,
    )
    .without_agent()
}

/// A host that remembers approvals and can answer for a secret.
///
/// ❗ `VolumeHost::detached()` trusts nothing AND remembers nothing, which is
/// right for it and useless here: a harness on it would loop forever on
/// "unknown → approve → still unknown".
pub fn fixture_host(params: &SftpConnectionParams, secret: Option<&str>) -> VolumeHost {
    let credentials = match secret {
        Some(secret) => InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some(&params.username),
            &params.username,
            secret,
        ),
        None => InMemoryCredentials::new(),
    };
    VolumeHost::builder()
        .host_keys(Arc::new(InMemoryHostKeys::new()))
        .credentials(Arc::new(credentials))
        .build()
}

/// The same host, plus the listing recorder the mutation cells assert on.
///
/// There is no watcher on this backend, so `notify_mutation` is the ONLY thing
/// that keeps a destination pane honest after a copy — which makes what it
/// reports worth pinning.
pub fn fixture_host_recording(
    params: &SftpConnectionParams,
    secret: Option<&str>,
) -> (VolumeHost, Arc<cmdr_fs::volume::host::listings::RecordingListings>) {
    let listings = Arc::new(cmdr_fs::volume::host::listings::RecordingListings::new());
    let credentials = match secret {
        Some(secret) => InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some(&params.username),
            &params.username,
            secret,
        ),
        None => InMemoryCredentials::new(),
    };
    let host = VolumeHost::builder()
        .host_keys(Arc::new(InMemoryHostKeys::new()))
        .credentials(Arc::new(credentials))
        .listings(Arc::clone(&listings) as Arc<dyn cmdr_fs::volume::host::listings::ListingHost>)
        .build();
    (host, listings)
}

/// Dials a fixture, approving its host key on first contact the way a user
/// would, and returns the live volume.
///
/// The whole two-phase flow in one helper, because every Docker cell needs it.
/// ❗ The first dial is DROPPED rather than held across the approval, which is
/// what production does too.
pub async fn connect_fixture(host: &VolumeHost, params: SftpConnectionParams) -> SftpVolume {
    match approve_and_connect(host, params).await {
        Ok(volume) => volume,
        Err(reason) => panic!("{reason}"),
    }
}

/// The same flow, as a `Result`, for a cell asserting on the refusal.
pub async fn approve_and_connect(host: &VolumeHost, params: SftpConnectionParams) -> Result<SftpVolume, String> {
    let volume_id = cmdr_fs::volume::sftp_volume_id(&params.host, params.port, &params.username);
    let first = connect_sftp_volume("fixture", &volume_id, params.clone(), host.clone())
        .await
        .map_err(|e| format!("dialing {}:{} failed: {e:?}", params.host, params.port))?;

    let prompt = match first {
        SftpConnectOutcome::Connected(volume) => return Ok(volume),
        SftpConnectOutcome::NeedsHostKeyApproval(prompt) => prompt,
    };
    if prompt.kind != HostKeyPromptKind::Unknown {
        return Err(format!(
            "a fresh fixture host must read as first contact, got {:?}",
            prompt.kind
        ));
    }
    super::approve_host_key(host, &prompt.host, prompt.port, &prompt.algorithm, &prompt.fingerprint);

    match connect_sftp_volume("fixture", &volume_id, params, host.clone()).await {
        Ok(SftpConnectOutcome::Connected(volume)) => Ok(volume),
        Ok(SftpConnectOutcome::NeedsHostKeyApproval(_)) => {
            Err("the fixture asked for approval again after its key was recorded".to_string())
        }
        Err(e) => Err(format!("the fixture refused a connection after approval: {e:?}")),
    }
}

/// A scratch directory name nothing else in this run will pick.
///
/// ❗ The write cells share one export with every other cell in the binary, and
/// `nextest` runs them in parallel: a fixed name would have two cells creating,
/// renaming, and deleting each other's files. The process id keeps two `cargo`
/// runs apart, the counter keeps two cells apart, and `what` says which cell to
/// look at when one leaves a mess behind.
pub fn scratch_dir(what: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "cmdr-test-{}-{}-{what}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Empties `dir` and takes it away, so a cell starts and ends on a clean export.
///
/// One level deep, which is all the write cells build. `Volume::delete` refuses
/// a directory that still holds something, so a cell that left a child behind
/// fails here loudly rather than leaving the next run a surprise.
pub async fn clean_scratch(volume: &SftpVolume, dir: &str) {
    use cmdr_fs::volume::Volume;
    use std::path::Path;

    let root = Path::new(dir);
    if let Ok(entries) = volume.list_directory(root, None).await {
        for entry in entries {
            let _ = volume.delete(&root.join(&entry.name)).await;
        }
    }
    let _ = volume.delete(root).await;
}
