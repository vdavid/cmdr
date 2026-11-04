use lz4_flex::frame::{BlockMode, BlockSize, FrameEncoder, FrameInfo};

use super::*;

/// Compressible, with matches reaching far back: every block of it leans on the
/// one before when the frame is linked.
fn sample(len: usize) -> Vec<u8> {
    let period: Vec<u8> = (0u32..1000)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    period.iter().copied().cycle().take(len).collect()
}

/// A xorshift stream: nothing for LZ4 to find.
fn incompressible(len: usize) -> Vec<u8> {
    let mut state = 0x1234_5678u32;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn encode(info: FrameInfo, data: &[u8]) -> Vec<u8> {
    let mut encoder = FrameEncoder::with_frame_info(info, Vec::new());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// Feeds `frame` in pieces of `packet` bytes, draining after each, as the sync
/// session does. Asserts the chunk cap on the way through.
fn decode(frame: &[u8], packet: usize) -> Result<Vec<u8>, AdbError> {
    let mut reader = Lz4Reader::new();
    let mut out = Vec::new();
    for piece in frame.chunks(packet) {
        reader.feed(piece);
        while let Some(chunk) = reader.next_chunk()? {
            assert!(chunk.len() <= MAX_DATA_CHUNK, "a chunk of {} bytes", chunk.len());
            out.extend_from_slice(&chunk);
        }
    }
    reader.finish()?;
    Ok(out)
}

/// A header for a frame with `flg`, `bd`, and whatever `extra` descriptor bytes it
/// declares, checksum included.
fn header(flg: u8, bd: u8, extra: &[u8]) -> Vec<u8> {
    let mut descriptor = vec![flg, bd];
    descriptor.extend_from_slice(extra);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&descriptor);
    out.push((xxh32(&descriptor) >> 8) as u8);
    out
}

/// `FLG` for a plain linked frame, `BD` for 64 KiB blocks: what adbd writes.
const ADBD_FLG: u8 = 0x40;
const BD_64K: u8 = 0x40;

fn assert_refused(result: Result<Vec<u8>, AdbError>) {
    assert!(matches!(result, Err(AdbError::Protocol(_))), "got {result:?}");
}

#[test]
fn a_linked_frame_decodes_whatever_the_packet_size() {
    let data = sample(300 * 1024);
    let info = FrameInfo::new()
        .block_size(BlockSize::Max64KB)
        .block_mode(BlockMode::Linked);
    let frame = encode(info, &data);
    for packet in [1, 2, 7, 13, 1000, MAX_DATA_CHUNK] {
        assert_eq!(decode(&frame, packet).unwrap(), data, "packet size {packet}");
    }
}

// The reference codec's own bytes, in the shape adbd writes: `sample(200 KiB)` through
// `lz4 -B4 -BD --no-frame-crc` (lz4 1.10.0). Its second block only decodes with the
// first as history, so a reader that ignored the link would fail here.
#[test]
fn a_frame_written_by_liblz4_decodes() {
    let frame = include_bytes!("testing/liblz4_frame.bin");
    assert_eq!(frame[4] & FLG_INDEPENDENT_BLOCKS, 0, "linked");
    assert_eq!(frame[5], BD_64K);
    let data = sample(200 * 1024);
    for packet in [1, 1000, MAX_DATA_CHUNK] {
        assert_eq!(decode(frame, packet).unwrap(), data, "packet size {packet}");
    }
}

#[test]
fn an_independent_frame_decodes() {
    let data = sample(200 * 1024);
    let info = FrameInfo::new()
        .block_size(BlockSize::Max64KB)
        .block_mode(BlockMode::Independent);
    assert_eq!(decode(&encode(info, &data), 4096).unwrap(), data);
}

#[test]
fn a_frame_with_big_blocks_still_hands_out_small_chunks() {
    let data = sample(5 * 1024 * 1024);
    let info = FrameInfo::new().block_size(BlockSize::Max4MB);
    assert_eq!(decode(&encode(info, &data), MAX_DATA_CHUNK).unwrap(), data);
}

#[test]
fn incompressible_data_travels_as_stored_blocks() {
    let data = incompressible(200 * 1024);
    let info = FrameInfo::new()
        .block_mode(BlockMode::Linked)
        .block_size(BlockSize::Max64KB);
    assert_eq!(decode(&encode(info, &data), MAX_DATA_CHUNK).unwrap(), data);
}

#[test]
fn checksums_a_frame_declares_are_verified() {
    let data = sample(100 * 1024);
    let info = FrameInfo::new()
        .block_size(BlockSize::Max64KB)
        .block_checksums(true)
        .content_checksum(true);
    let frame = encode(info, &data);
    assert_eq!(decode(&frame, 999).unwrap(), data);

    let mut corrupt_block = frame.clone();
    corrupt_block[20] ^= 0xFF;
    assert_refused(decode(&corrupt_block, 999));

    let mut corrupt_content = frame;
    let last = corrupt_content.len() - 1;
    corrupt_content[last] ^= 0xFF;
    assert_refused(decode(&corrupt_content, 999));
}

// Regression anchor for the `adb_lz4` fuzz finding: `twox-hash` before 2.1.3 added the
// input length to the converged accumulators with a plain `+`, so an overflow-checked
// build panicked on any input whose sum wraps. These 16 bytes are brute-forced to wrap
// (accumulators at `u32::MAX - 15` or above). `Cargo.toml` pins `twox-hash` past the bug.
#[test]
fn a_content_checksum_whose_length_add_wraps_still_verifies() {
    let data = [0x74, 0x1f, 0x97, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let independent_with_content_checksum = 0x40 | FLG_INDEPENDENT_BLOCKS | FLG_CONTENT_CHECKSUM;
    let mut frame = header(independent_with_content_checksum, BD_64K, &[]);
    frame.extend_from_slice(&(BLOCK_STORED | data.len() as u32).to_le_bytes());
    frame.extend_from_slice(&data);
    frame.extend_from_slice(&0u32.to_le_bytes());
    frame.extend_from_slice(&xxh32(&data).to_le_bytes());
    assert_eq!(decode(&frame, 7).unwrap(), data);
}

#[test]
fn a_declared_content_size_is_held_to() {
    let framed = |declared: u64| {
        let mut frame = header(ADBD_FLG | FLG_CONTENT_SIZE, BD_64K, &declared.to_le_bytes());
        frame.extend((BLOCK_STORED | 3).to_le_bytes());
        frame.extend(b"abc");
        frame.extend(0u32.to_le_bytes());
        frame
    };
    assert_eq!(decode(&framed(3), 64).unwrap(), b"abc");
    assert_refused(decode(&framed(4), 64));
}

#[test]
fn a_bad_header_is_refused() {
    let mut wrong_magic = header(ADBD_FLG, BD_64K, &[]);
    wrong_magic[0] ^= 1;
    assert_refused(decode(&wrong_magic, 64));

    let mut wrong_checksum = header(ADBD_FLG, BD_64K, &[]);
    wrong_checksum[6] ^= 1;
    assert_refused(decode(&wrong_checksum, 64));

    for (flg, bd) in [
        (0x80, BD_64K), // version 10
        (ADBD_FLG | FLG_RESERVED, BD_64K),
        (ADBD_FLG | FLG_DICT_ID, BD_64K),
        (ADBD_FLG, 0x30),          // a block size the format doesn't name
        (ADBD_FLG, BD_64K | 0x01), // a reserved `BD` bit
    ] {
        assert_refused(decode(&header(flg, bd, &[]), 64));
    }
}

// ❗ What keeps a hostile phone from growing the input buffer to 2 GiB: the word
// alone is enough to refuse, with no byte of the block in hand.
#[test]
fn a_block_larger_than_the_frame_allows_is_refused_off_its_size_word() {
    for word in [
        0x7FFF_FFFFu32,
        0xFFFF_FFFF,
        64 * 1024 + 1,
        BLOCK_STORED | (64 * 1024 + 1),
    ] {
        let mut frame = header(ADBD_FLG, BD_64K, &[]);
        frame.extend(word.to_le_bytes());
        let mut reader = Lz4Reader::new();
        reader.feed(&frame);
        assert!(
            matches!(reader.next_chunk(), Err(AdbError::Protocol(_))),
            "word {word:#x}"
        );
    }
}

// The bomb: a small block that decodes to far more than the frame's declared
// maximum. The codec stops at the end of the buffer.
#[test]
fn a_block_that_decodes_past_the_frames_maximum_is_refused() {
    let block = lz4_flex::block::compress(&vec![0u8; 8 * 1024 * 1024]);
    assert!(
        block.len() <= 64 * 1024,
        "this has to pass the size word to mean anything"
    );
    let mut frame = header(ADBD_FLG, BD_64K, &[]);
    frame.extend(u32::try_from(block.len()).unwrap().to_le_bytes());
    frame.extend(&block);
    assert_refused(decode(&frame, MAX_DATA_CHUNK));
}

#[test]
fn a_linked_block_cant_reach_back_past_what_was_decoded() {
    // A match with offset 0xFFFF as the very first sequence: no history to copy.
    let block = [0x04u8, 0xFF, 0xFF];
    let mut frame = header(ADBD_FLG, BD_64K, &[]);
    frame.extend(u32::try_from(block.len()).unwrap().to_le_bytes());
    frame.extend(block);
    assert_refused(decode(&frame, 64));
}

#[test]
fn a_frame_without_its_end_mark_is_a_truncated_transfer() {
    let info = FrameInfo::new().block_size(BlockSize::Max64KB);
    let frame = encode(info, &sample(100 * 1024));
    let without_end_mark = &frame[..frame.len() - 4];
    assert_refused(decode(without_end_mark, 4096));
    assert_refused(decode(&frame[..3], 4096));
    assert!(matches!(Lz4Reader::new().finish(), Err(AdbError::Protocol(_))));
}

#[test]
fn bytes_after_the_end_mark_are_refused() {
    let info = FrameInfo::new().block_size(BlockSize::Max64KB);
    let mut frame = encode(info, b"hello");
    frame.push(0);
    assert_refused(decode(&frame, 4096));
}

fn written(data: &[u8]) -> Vec<u8> {
    let mut writer = Lz4Writer::new();
    let mut wire = Vec::new();
    for piece in data.chunks(MAX_DATA_CHUNK) {
        let out = writer.write(piece).unwrap();
        assert_eq!(out.len() % MAX_DATA_CHUNK, 0, "whole packets only");
        wire.extend(out);
    }
    wire.extend(writer.finish().unwrap());
    wire
}

#[test]
fn the_writer_makes_a_linked_frame_that_reads_back() {
    let data = sample(MAX_DATA_CHUNK * 40);
    let wire = written(&data);
    assert_eq!(decode(&wire, MAX_DATA_CHUNK).unwrap(), data);
    assert_eq!(
        wire[4] & FLG_INDEPENDENT_BLOCKS,
        0,
        "linked, as adbd's own client writes"
    );
    assert_eq!(wire[5], BD_64K);
}

#[test]
fn the_writer_keeps_up_on_incompressible_data() {
    let data = incompressible(MAX_DATA_CHUNK * 3);
    assert_eq!(decode(&written(&data), MAX_DATA_CHUNK).unwrap(), data);
}

#[test]
fn an_empty_file_is_still_a_whole_frame() {
    let wire = Lz4Writer::new().finish().unwrap();
    assert!(!wire.is_empty(), "adbd spins on a DONE that no end mark preceded");
    assert_eq!(decode(&wire, 64).unwrap(), b"");
}
