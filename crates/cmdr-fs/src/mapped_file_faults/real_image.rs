//! The recovery against a real dead mapping: a file on a disk image, mapped, then the image
//! force-detached under it.
//!
//! `#[ignore]`d: it attaches an APFS image. Hand-run with
//! `cargo nextest run -p cmdr-fs --run-ignored only -E 'test(mapped_file_faults::real_image::)'`,
//! or through `pnpm check disk-images`.

use std::os::fd::AsRawFd as _;

use crate::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};

const MAPPED_BYTES: usize = 8 * 1024 * 1024;

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn reading_a_mapping_whose_disk_was_force_detached_reads_zeros_and_moves_the_generation() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Apfs).expect("attach the image");
    let path = image.volumes()[0].mount_point.join("mapped.bin");
    std::fs::write(&path, vec![0xA5u8; MAPPED_BYTES]).expect("write the file");
    let file = std::fs::File::open(&path).expect("open it");
    // SAFETY: a read-only shared mapping of a file this test owns, unmapped below; the shape
    // `gix-pack`'s `memmap2::Mmap::map` makes.
    let mapped = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            MAPPED_BYTES,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            0,
        )
    };
    assert_ne!(mapped, libc::MAP_FAILED);
    drop(file);
    super::install_test_handler();
    let before = super::generation();

    image.force_detach().expect("force-detach the image");

    // Every page this read crosses is a dead vnode's: without the recovery, SIGBUS ends the
    // process here.
    // SAFETY: the mapping above, read within its length.
    let bytes = unsafe { std::slice::from_raw_parts(mapped.cast::<u8>(), MAPPED_BYTES) };
    let nonzero = bytes.iter().filter(|byte| **byte != 0).count();
    assert!(
        super::generation() > before,
        "a dead page was recovered, and the generation says so"
    );
    assert!(
        nonzero < MAPPED_BYTES / 2,
        "the pages that weren't resident read as zeros (of the file's bytes, {nonzero} kept their value)"
    );
    // SAFETY: the mapping above, with its own length; zero pages included, it's one range.
    assert_eq!(unsafe { libc::munmap(mapped, MAPPED_BYTES) }, 0);
}
