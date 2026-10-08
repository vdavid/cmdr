//! Archive browsing and reading: `ArchiveIndex::parse` over an in-memory archive in
//! each format (zip via rc-zip, tar under every codec, 7z), the synthetic-tree walk,
//! and a bounded read of the first few files. Checks the index-level Zip Slip
//! guarantee on every node the tree lists.
#![no_main]

use std::sync::Arc;

use cmdr_archive::{ArchiveByteSource, ArchiveFormat, ArchiveIndex, BytesSource, TarCodec};

/// Files read per input, and bytes drained from each. Enough to drive every
/// decoder through its first blocks without letting a bomb dominate the run.
const FILES_READ: usize = 3;
const BYTES_PER_FILE: u64 = 256 * 1024;

const FORMATS: [ArchiveFormat; 7] = [
    ArchiveFormat::Zip,
    ArchiveFormat::Tar(TarCodec::Plain),
    ArchiveFormat::Tar(TarCodec::Gzip),
    ArchiveFormat::Tar(TarCodec::Bzip2),
    ArchiveFormat::Tar(TarCodec::Xz),
    ArchiveFormat::Tar(TarCodec::Zstd),
    ArchiveFormat::SevenZ,
];

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((&selector, archive)) = data.split_first() else {
        return;
    };
    let format = FORMATS[usize::from(selector) % FORMATS.len()];
    let source: Arc<dyn ArchiveByteSource> = Arc::new(BytesSource::new(archive.to_vec()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime with no drivers always builds");
    // The parse reads through the source synchronously, but the tar and 7z stores
    // hand decoding to `spawn_blocking`, so everything runs inside the runtime.
    runtime.block_on(async {
        let Ok(index) = ArchiveIndex::parse(Arc::clone(&source), format, None) else {
            return;
        };
        let mut files = Vec::new();
        let mut pending = vec![String::new()];
        while let Some(dir) = pending.pop() {
            for node in index.list(&dir).unwrap_or_default() {
                let path = node.path;
                assert!(
                    !path.split('/').any(|c| matches!(c, "" | "." | "..")),
                    "the tree lists an unsafe path: {path:?}"
                );
                let expected = if dir.is_empty() {
                    node.name
                } else {
                    format!("{dir}/{}", node.name)
                };
                assert_eq!(path, expected, "a node's path disagrees with where the tree lists it");
                if node.is_dir {
                    pending.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        for path in files.into_iter().take(FILES_READ) {
            let Ok(mut reader) = index.open_read(&path, Arc::clone(&source), None) else {
                continue;
            };
            while reader.bytes_read() < BYTES_PER_FILE {
                match reader.next_chunk().await {
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    });
});
