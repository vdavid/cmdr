//! Entry points for the fuzz targets in `fuzz/` (repo root): the readers that
//! take bytes from the device, fed from an in-memory script instead of a socket.
//!
//! The device (through the server, which relays it verbatim) controls every
//! byte after `sync:` or `shell,v2,raw:`, so these readers are the attack
//! surface a hostile phone gets. The fuzzer is looking for panics, runaway
//! allocations, and hangs, plus the properties each entry point asserts: what
//! the readers promise their callers whatever the device sends. Every assertion
//! has a failing mutant on record in `fuzz/fuzz_targets/<target>.mutants.json`.

use crate::features::DeviceFeatures;
use crate::lz4::{Lz4Reader, Lz4Writer};
use crate::shell::{SpaceParts, parse_df_k, read_frames};
use crate::sync::{MAX_DATA_CHUNK, SyncSession};
use crate::transport::AdbConnection;

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime with no drivers always builds")
        .block_on(future)
}

/// Drives one sync session against `data`: a stat, a listing, and a pull, in
/// that order, each reading on from where the last one stopped. The first byte
/// picks the feature set (v1 or v2 verbs, compressed pull or not); the rest is
/// what the device says.
pub fn sync_session(data: &[u8]) {
    let Some((&flags, device_bytes)) = data.split_first() else {
        return;
    };
    let features = DeviceFeatures {
        shell_v2: true,
        stat_v2: flags & 1 != 0,
        ls_v2: flags & 2 != 0,
        sendrecv_v2: flags & 4 != 0,
        sendrecv_v2_lz4: flags & 8 != 0,
    };
    let mut session = SyncSession::from_connection(AdbConnection::scripted(device_bytes), features);
    block_on(async {
        let _ = session.stat("/sdcard/a").await;
        // The volume joins each name onto the listed folder, so anything but one
        // path segment would name some other file.
        let _ = session
            .list("/sdcard", &mut |entry| {
                let name = entry.name;
                assert!(
                    !name.is_empty() && name != "." && name != ".." && !name.contains('/'),
                    "the listing handed on a name that isn't one path segment: {name:?}"
                );
            })
            .await;
        if session.recv_start("/sdcard/a").await.is_ok() {
            while let Ok(Some(chunk)) = session.recv_chunk().await {
                assert!(
                    chunk.len() <= MAX_DATA_CHUNK,
                    "a pull handed on a chunk past MAX_DATA_CHUNK"
                );
            }
        }
    });
}

/// Feeds one LZ4 frame to the reader a compressed pull uses, the way the sync
/// session does: a packet, then every chunk it released. The first byte picks the
/// packet size, so a packet can end anywhere in the frame. The chunk cap is what a
/// compression bomb has to get past, so breaking it is a finding.
///
/// The same bytes also go through the round trip: what our writer encodes, our
/// reader decodes back unchanged.
pub fn lz4_frame(data: &[u8]) {
    let Some((&size, frame)) = data.split_first() else {
        return;
    };
    // Up to about 64 KiB, the size a packet can actually be.
    let packet = usize::from(size).saturating_mul(256).clamp(1, MAX_DATA_CHUNK);
    lz4_round_trip(frame, packet);
    let mut reader = Lz4Reader::new();
    for piece in frame.chunks(packet) {
        reader.feed(piece);
        loop {
            match reader.next_chunk() {
                Ok(Some(chunk)) => assert!(chunk.len() <= MAX_DATA_CHUNK),
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }
    let _ = reader.finish();
}

/// Compresses `plain` the way a push does and decodes it the way a pull does, in
/// `packet`-sized pieces. A frame past 64 KiB has linked blocks, so the reader's
/// window is what a large input reaches.
fn lz4_round_trip(plain: &[u8], packet: usize) {
    let mut writer = Lz4Writer::new();
    let mut wire = writer.write(plain).expect("the writer takes any bytes");
    wire.extend(writer.finish().expect("the writer ends any frame"));
    let mut reader = Lz4Reader::new();
    let mut decoded = Vec::with_capacity(plain.len());
    for piece in wire.chunks(packet) {
        reader.feed(piece);
        while let Some(chunk) = reader
            .next_chunk()
            .expect("the reader refused a frame the writer wrote")
        {
            decoded.extend(chunk);
        }
    }
    reader.finish().expect("the reader refused a frame the writer wrote");
    assert!(decoded == plain, "an LZ4 round trip changed the bytes");
}

/// The stdout bound the bounded read runs under here.
const STDOUT_BOUND: usize = 4096;

/// Reads one shell v2 command's frames from `device_bytes`, bounded and not,
/// and parses the stdout as `df -k` output, as the volume's space query does.
/// Then reads back a `df -k` answer written from the input's first 24 bytes.
pub fn shell_v2(device_bytes: &[u8]) {
    block_on(async {
        if let Ok(outcome) = read_frames(&mut AdbConnection::scripted(device_bytes), None).await {
            let _ = parse_df_k(&outcome.stdout_text());
        }
        if let Ok(outcome) = read_frames(&mut AdbConnection::scripted(device_bytes), Some(STDOUT_BOUND)).await {
            assert!(
                outcome.stdout.len() <= STDOUT_BOUND,
                "a bounded read returned stdout past its bound"
            );
        }
    });
    df_round_trip(device_bytes);
}

/// Three figures from `bytes` (zero-padded), written as toybox's or busybox's
/// `df -k` prints them (busybox wraps a long device name onto its own line), must
/// read back as themselves, in bytes.
fn df_round_trip(bytes: &[u8]) {
    let mut padded = [0u8; 25];
    let taken = bytes.len().min(padded.len());
    padded[..taken].copy_from_slice(&bytes[..taken]);
    // Up to 2^40 KiB (a PiB), so `* 1024` never saturates two figures to the
    // same value and a swap between them still shows.
    let figure = |at: usize| u64::from_le_bytes(padded[at..at + 8].try_into().expect("eight bytes")) & ((1 << 40) - 1);
    let (total, used, available) = (figure(0), figure(8), figure(16));
    let device = if padded[24] & 1 == 0 {
        "/dev/fuse "
    } else {
        "/dev/block/by-name/userdata_with_a_long_name\n "
    };
    let stdout = format!(
        "Filesystem 1K-blocks Used Available Use% Mounted on\n{device}{total} {used} {available} 50% /storage/emulated\n"
    );
    assert_eq!(
        parse_df_k(&stdout),
        Some(SpaceParts {
            total_bytes: total.saturating_mul(1024),
            available_bytes: available.saturating_mul(1024),
        }),
        "df -k read back figures it wasn't given"
    );
}
