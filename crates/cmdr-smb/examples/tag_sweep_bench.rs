//! What a pane's Finder tag sweep costs against a real share, against what it cost through
//! the macOS mount. The numbers it produced: `docs/notes/smb-finder-tags-2026-10-31.md`.
//!
//! ```text
//! CMDR_NAS_PASSWORD=… cargo run --release -p cmdr-smb --example tag_sweep_bench -- \
//!     <host> <share> <user> <scratch dir in the share> <seed N | sweep [mount path] | clean>
//! ```
//!
//! `seed` makes the folder and N files, 5% of them tagged. `sweep` runs the same 500-path
//! chunks `pane/tag-sweep.ts` sends. `clean` removes the folder. It touches nothing else.

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, XattrReadMode};
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, connect_smb_volume};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
/// macOS's own plist for `["Red\n6"]`.
const RED: &[u8] =
    b"bplist00\xa1\x01URed\n6\x08\x0a\0\0\0\0\0\0\x01\x01\0\0\0\0\0\0\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x10";
/// `TAG_SWEEP_CHUNK` in `pane/tag-sweep.ts`.
const SWEEP_CHUNK: usize = 500;
const RUNS: usize = 3;
const MOUNT_ROOT: &str = "/Volumes/tag-sweep-bench";

fn usage() -> ! {
    panic!("usage: <host> <share> <user> <scratch dir> <seed N | sweep [mount path] | clean>");
}

/// Files a second, and the slowest chunk, for one run of the sweep.
fn report(label: &str, files: usize, took: Duration, slowest: Duration) {
    let rate = files as f64 / took.as_secs_f64();
    let note = format!("{label:<36} {rate:>7.0} files/s, {took:>9.2?} for {files}, slowest chunk {slowest:.2?}");
    std::io::Write::write_all(&mut std::io::stdout(), format!("{note}\n").as_bytes()).expect("stdout");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, share, user, scratch, command, rest @ ..] = args.as_slice() else {
        usage();
    };
    let password = std::env::var("CMDR_NAS_PASSWORD").expect("CMDR_NAS_PASSWORD");
    let params = SmbConnectionParams::new(host, share, 445, Some(user), Some(&password));
    let id = cmdr_fs::volume::smb_volume_id(host, 445, share);
    let anchor = MountAnchor::at_share_root(MOUNT_ROOT);
    let volume = connect_smb_volume(share, anchor, &id, params, VolumeHost::detached())
        .await
        .expect("connect");
    let scratch = Path::new(scratch);

    match command.as_str() {
        "seed" => {
            let count: usize = rest.first().unwrap_or_else(|| usage()).parse().expect("a count");
            volume
                .create_directory(scratch)
                .await
                .expect("the scratch folder is new");
            let volume = &volume;
            futures_util::stream::iter(0..count)
                .map(|i| async move {
                    let file = scratch.join(format!("file-{i:05}.txt"));
                    volume.create_file(&file, b"x").await.expect("create");
                    if i % 20 == 0 {
                        volume.write_xattr(&file, TAGS, RED).await.expect("tag");
                    }
                })
                .buffer_unordered(16)
                .collect::<Vec<_>>()
                .await;
        }
        "sweep" => {
            let entries = volume.list_directory(scratch, None).await.expect("list");
            let paths: Vec<PathBuf> = entries.iter().map(|entry| scratch.join(&entry.name)).collect();
            for run in 0..RUNS {
                for (label, mode) in [
                    ("smb2, Verified", XattrReadMode::Verified),
                    ("smb2, AssumePathsExist", XattrReadMode::AssumePathsExist),
                ] {
                    let started = Instant::now();
                    let mut slowest = Duration::ZERO;
                    let mut tagged = 0;
                    for chunk in paths.chunks(SWEEP_CHUNK) {
                        let chunk_started = Instant::now();
                        let answers = volume.read_xattr_many(chunk, TAGS, mode).await;
                        slowest = slowest.max(chunk_started.elapsed());
                        tagged += answers.iter().filter(|answer| matches!(answer, Ok(Some(_)))).count();
                        assert!(answers.iter().all(Result::is_ok), "a read failed");
                    }
                    assert_eq!(tagged, paths.len() / 20, "the seed's 5% read back");
                    report(&format!("run {run}: {label}"), paths.len(), started.elapsed(), slowest);
                }
                if let Some(mount) = rest.first() {
                    let started = Instant::now();
                    let mut slowest = Duration::ZERO;
                    for chunk in paths.chunks(SWEEP_CHUNK) {
                        let chunk_started = Instant::now();
                        for path in chunk {
                            let _ = xattr::get(Path::new(mount).join(path), TAGS);
                        }
                        slowest = slowest.max(chunk_started.elapsed());
                    }
                    report(
                        &format!("run {run}: mount, serial getxattr"),
                        paths.len(),
                        started.elapsed(),
                        slowest,
                    );
                }
            }
        }
        "clean" => {
            let entries = volume.list_directory(scratch, None).await.expect("list");
            let volume = &volume;
            futures_util::stream::iter(entries)
                .map(|entry| async move { volume.delete(&scratch.join(&entry.name)).await.expect("delete") })
                .buffer_unordered(16)
                .collect::<Vec<_>>()
                .await;
            volume.delete(scratch).await.expect("the scratch folder is empty");
        }
        _ => usage(),
    }
}
