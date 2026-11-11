use std::fs::File;

/// One `pread`: no cursor, so concurrent readers never disturb each other.
pub(crate) fn positioned_read(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}
