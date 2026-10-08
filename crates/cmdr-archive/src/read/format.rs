//! The pure-Rust decoders that unwrap a tar's outer compression.
//!
//! Which format a name denotes lives in `cmdr_fs::archive_format` (so
//! `FileEntry.is_archive` can reach it) and is re-exported here, so this module
//! stays the one import site for "format + decoder".
//!
//! Every tar codec is a pull-model [`Read`] decoder driven on the producer's
//! `spawn_blocking` thread, so the whole-file decompress streams in bounded
//! chunks and never whole-buffers (principle 5). All decoders are pure-Rust
//! (`flate2`/`miniz_oxide`, `bzip2`/`libbz2-rs-sys`, `lzma-rust2`, `ruzstd`).

use std::io::Read;

use super::error::ArchiveError;

pub use cmdr_fs::archive_format::{ArchiveFormat, TarCodec, format_for_name, format_for_path};

/// The most memory one xz block's LZMA2 decoder may ask for, in KiB: a 256 MiB
/// dictionary plus 1 MiB for the decoder's own buffers. The block header names its
/// dictionary and the decoder allocates AND zeroes it up front, so without a cap
/// 40 hostile bytes cost 4 GiB of real memory (and seconds of CPU) per browse.
///
/// 256 MiB is four times the largest preset (`xz -9` / `-9e` and 7-Zip's Ultra all
/// use 64 MiB), so everything written with presets opens. Refused: only a stream
/// made with a hand-set dictionary past 256 MiB (`--lzma2=dict=1GiB`; liblzma allows
/// up to 1.5 GiB), which costs a user that much RAM to open anyway. Over the cap the
/// decoder refuses before allocating with `io::ErrorKind::OutOfMemory`, which
/// `ArchiveError` types as `TooLarge`.
pub(super) const XZ_MEMORY_LIMIT_KIB: u32 = 256 * 1024 + 1024;

/// Wraps `reader` (the raw archive bytes) in the streaming decoder for `codec`,
/// yielding the decompressed tar byte stream. `Plain` passes through. Every codec
/// handles concatenated members (`gzip -c a b`), matching GNU tar.
pub(super) fn open_tar_decoder<'a>(
    codec: TarCodec,
    reader: Box<dyn Read + Send + 'a>,
) -> Result<Box<dyn Read + Send + 'a>, ArchiveError> {
    Ok(match codec {
        TarCodec::Plain => reader,
        TarCodec::Gzip => Box::new(flate2::read::MultiGzDecoder::new(reader)),
        TarCodec::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(reader)),
        TarCodec::Xz => Box::new(lzma_rust2::XzReader::new_mem_limit(reader, true, XZ_MEMORY_LIMIT_KIB)),
        TarCodec::Zstd => {
            let decoder = ruzstd::decoding::StreamingDecoder::new(reader)
                .map_err(|e| ArchiveError::Corrupt(format!("zstd: {e}")))?;
            Box::new(decoder)
        }
    })
}
