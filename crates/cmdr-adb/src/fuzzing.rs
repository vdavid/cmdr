//! Entry points for the fuzz targets in `fuzz/` (repo root): the readers that
//! take bytes from the device, fed from an in-memory script instead of a socket.
//!
//! The device (through the server, which relays it verbatim) controls every
//! byte after `sync:` or `shell,v2,raw:`, so these readers are the attack
//! surface a hostile phone gets. Each entry point returns nothing: the fuzzer
//! is looking for panics, runaway allocations, and hangs, never for answers.

use crate::features::DeviceFeatures;
use crate::shell::{parse_df_k, read_frames};
use crate::sync::SyncSession;
use crate::transport::AdbConnection;

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime with no drivers always builds")
        .block_on(future)
}

/// Drives one sync session against `data`: a stat, a listing, and a pull, in
/// that order, each reading on from where the last one stopped. The first byte
/// picks the feature set (v1 or v2 verbs); the rest is what the device says.
pub fn sync_session(data: &[u8]) {
    let Some((&flags, device_bytes)) = data.split_first() else {
        return;
    };
    let features = DeviceFeatures {
        shell_v2: true,
        stat_v2: flags & 1 != 0,
        ls_v2: flags & 2 != 0,
        sendrecv_v2: flags & 4 != 0,
    };
    let mut session = SyncSession::from_connection(AdbConnection::scripted(device_bytes), features);
    block_on(async {
        let _ = session.stat("/sdcard/a").await;
        let _ = session.list("/sdcard", &mut |_| {}).await;
        if session.recv_start("/sdcard/a").await.is_ok() {
            while let Ok(Some(_)) = session.recv_chunk().await {}
        }
    });
}

/// Reads one shell v2 command's frames from `device_bytes`, bounded and not,
/// and parses the stdout as `df -k` output, as the volume's space query does.
pub fn shell_v2(device_bytes: &[u8]) {
    block_on(async {
        if let Ok(outcome) = read_frames(&mut AdbConnection::scripted(device_bytes), None).await {
            let _ = parse_df_k(&outcome.stdout_text());
        }
        let _ = read_frames(&mut AdbConnection::scripted(device_bytes), Some(4096)).await;
    });
}
