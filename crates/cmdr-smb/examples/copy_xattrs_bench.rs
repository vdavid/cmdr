//! What keeping xattrs costs a copy over SMB: a plain file's read with and without
//! the stream listing a copy now asks for, a tagged file's extra read and write,
//! and big-file throughput both ways. The numbers it produced:
//! `docs/notes/smb-copy-xattrs-bench-2026-11-03.md`.
//!
//! ```text
//! cargo run --release -p cmdr-smb --example copy_xattrs_bench -- <host> <port> <share> <scratch dir in the share>
//! ```
//!
//! Guest login (the Docker fixtures). It makes the scratch folder, fills it, and
//! removes it; it touches nothing else.

use cmdr_fs::metadata::{CarriedMetadata, MetadataAsk, MetadataClasses, Xattr};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, connect_smb_volume};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};

const SMALL_FILES: usize = 1_000;
const TAGGED_FILES: usize = 100;
const SMALL_BYTES: usize = 4 * 1024;
const LARGE_BYTES: usize = 256 * 1024 * 1024;
const RUNS: usize = 9;
const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";

/// A copy's source: `bytes`, carrying a tag when `tagged`.
struct Source {
    bytes: Option<Vec<u8>>,
    size: u64,
    tagged: bool,
}

impl Source {
    fn new(size: usize, tagged: bool) -> Box<Self> {
        Box::new(Self {
            bytes: Some(vec![b'b'; size]),
            size: size as u64,
            tagged,
        })
    }
}

impl VolumeReadStream for Source {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move { self.bytes.take().map(Ok) })
    }
    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.size)
    }
    fn bytes_read(&self) -> u64 {
        if self.bytes.is_some() { 0 } else { self.size }
    }
    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let xattrs = match self.tagged {
            true => vec![Xattr {
                name: TAGS.into(),
                value: b"bplist00 a red tag, about this long".to_vec(),
            }],
            false => Vec::new(),
        };
        CarriedMetadata {
            xattrs,
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}

fn say(line: &str) {
    std::io::Write::write_all(&mut std::io::stdout(), format!("{line}\n").as_bytes()).expect("stdout");
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

async fn write(volume: &dyn Volume, path: &Path, size: usize, tagged: bool) {
    volume
        .write_from_stream(
            path,
            WriteMode::CreateOrReplace,
            StreamLength::Known(size as u64),
            Source::new(size, tagged),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .expect("write");
}

async fn read(volume: &dyn Volume, path: &Path, size: usize, ask: Option<MetadataAsk>) -> usize {
    let mut stream = match ask {
        Some(ask) => volume.open_read_stream_for_copy(path, Some(size as u64), ask).await,
        None => volume.open_read_stream_with_hint(path, Some(size as u64)).await,
    }
    .expect("open");
    let xattrs = stream.metadata(MetadataClasses::XATTRS).xattrs.len();
    while let Some(chunk) = stream.next_chunk().await {
        chunk.expect("chunk");
    }
    xattrs
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, share, scratch] = args.as_slice() else {
        panic!("usage: <host> <port> <share> <scratch dir>");
    };
    let port: u16 = port.parse().expect("a port");
    let params = SmbConnectionParams::new(host, share, port, None, None);
    let id = cmdr_fs::volume::smb_volume_id(host, port, share);
    let volume = connect_smb_volume(
        share,
        MountAnchor::at_share_root("/Volumes/bench"),
        &id,
        params,
        VolumeHost::detached(),
    )
    .await
    .expect("connect");
    let scratch = PathBuf::from(scratch);
    volume.create_directory(&scratch).await.expect("a new scratch folder");
    let keeps = MetadataAsk {
        values: MetadataClasses::XATTRS,
        outline: MetadataClasses::NONE,
    };
    let plain: Vec<PathBuf> = (0..SMALL_FILES).map(|i| scratch.join(format!("p{i:04}.bin"))).collect();
    let tagged: Vec<PathBuf> = (0..TAGGED_FILES)
        .map(|i| scratch.join(format!("t{i:04}.bin")))
        .collect();
    let large = scratch.join("large.bin");
    say(&format!("share supports xattrs: {}", volume.supports_xattrs()));

    // Writes, one at a time: the per-file cost shows; the copy engine overlaps 10.
    let mut runs = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..RUNS {
        let start = Instant::now();
        for path in &plain {
            write(&volume, path, SMALL_BYTES, false).await;
        }
        runs.0.push(start.elapsed());
        let start = Instant::now();
        for path in &tagged {
            write(&volume, path, SMALL_BYTES, true).await;
        }
        runs.1.push(start.elapsed());
        let start = Instant::now();
        write(&volume, &large, LARGE_BYTES, false).await;
        runs.2.push(start.elapsed());
    }
    let (plain_write, tagged_write, large_write) = (median(runs.0), median(runs.1), median(runs.2));
    say(&format!(
        "write {SMALL_FILES} plain small: {:.0} files/s ({:.2} ms each)",
        SMALL_FILES as f64 / plain_write.as_secs_f64(),
        plain_write.as_secs_f64() * 1000.0 / SMALL_FILES as f64
    ));
    say(&format!(
        "write {TAGGED_FILES} tagged small: {:.2} ms each",
        tagged_write.as_secs_f64() * 1000.0 / TAGGED_FILES as f64
    ));
    say(&format!(
        "write large plain: {:.0} MB/s",
        LARGE_BYTES as f64 / 1e6 / large_write.as_secs_f64()
    ));

    // Reads: today's hinted read against the copy's, which lists the streams.
    // Interleaved file by file (and the order flipped every run), so a drift in
    // the server or the machine lands on both sides alike.
    let mut runs: [Vec<Duration>; 6] = Default::default();
    for run in 0..RUNS {
        let asks = match run % 2 {
            0 => [(0, None), (1, Some(keeps))],
            _ => [(1, Some(keeps)), (0, None)],
        };
        for (files, size, base) in [
            (&plain, SMALL_BYTES, 0),
            (&tagged, SMALL_BYTES, 2),
            (&vec![large.clone()], LARGE_BYTES, 4),
        ] {
            let mut took = [Duration::ZERO; 2];
            for path in files {
                for (slot, ask) in asks {
                    let start = Instant::now();
                    read(&volume, path, size, ask).await;
                    took[slot] += start.elapsed();
                }
            }
            runs[base].push(took[0]);
            runs[base + 1].push(took[1]);
        }
    }
    let runs = runs.map(median);
    let per = |took: Duration, count: usize| took.as_secs_f64() * 1000.0 / count as f64;
    say(&format!(
        "read {SMALL_FILES} plain small: {:.0} files/s before, {:.0} files/s with the listing",
        SMALL_FILES as f64 / runs[0].as_secs_f64(),
        SMALL_FILES as f64 / runs[1].as_secs_f64()
    ));
    say(&format!(
        "read {TAGGED_FILES} tagged small: {:.2} ms each without its tags, {:.2} ms with them",
        per(runs[2], TAGGED_FILES),
        per(runs[3], TAGGED_FILES)
    ));
    say(&format!(
        "read large plain: {:.0} MB/s before, {:.0} MB/s with the listing",
        LARGE_BYTES as f64 / 1e6 / runs[4].as_secs_f64(),
        LARGE_BYTES as f64 / 1e6 / runs[5].as_secs_f64()
    ));

    for path in plain.iter().chain(&tagged).chain([&large]) {
        let _ = volume.delete(path).await;
    }
    let _ = volume.delete(&scratch).await;
}
