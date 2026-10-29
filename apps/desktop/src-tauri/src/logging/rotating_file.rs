//! `cmdr.log` with size-based rotation into `cmdr.log.1` … `cmdr.log.N`, newest first.
//!
//! Every step tolerates a directory it doesn't own alone. Another Cmdr process can log into the
//! same directory, and the error reporter and `eager_prune` delete files in it, so nothing here
//! remembers which siblings exist: rotation renames over whatever is there (`rename(2)` replaces
//! the target atomically) and treats a missing file as nothing to move. A failure comes back as an
//! `io::Error`, which fern reports on stderr, never as a panic. That matters because logging runs
//! on every thread: `file-rotate` cached the sibling list at startup and asserted it was still
//! true, and when it wasn't, the panic killed the drive index writer (CRASH-EC4QS, 0.51.0).
//!
//! Rotation happens on `flush`, after the record that reached the limit. fern ends every record
//! with exactly one flush (and `CoalescingWriter` forwards it), so a record is never split across
//! two files. A file ends at most one record past `max_bytes`.
//!
//! Before rotating, the writer checks that the file it holds open is still the one at `cmdr.log`
//! (same device and inode). If it isn't, or `cmdr.log` is gone, another process already rotated
//! it away: the writer reopens `cmdr.log` and renames nothing, so it never moves another
//! process's live file (#812).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(crate) struct RotatingFile {
    path: PathBuf,
    /// Rotated files kept beside the live one.
    rotated_keep: usize,
    max_bytes: u64,
    /// `None` until the first write, and again right after a rotation.
    file: Option<File>,
    /// Bytes in the live file, counted from its size at open.
    len: u64,
    /// The size that triggers the next rotation. Normally `max_bytes`; pushed one more
    /// `max_bytes` out after a failed rotation, so a directory that refuses renames costs one
    /// attempt per `max_bytes` written rather than one per line.
    rotate_at: u64,
}

impl RotatingFile {
    pub(crate) fn new(path: PathBuf, rotated_keep: usize, max_bytes: u64) -> Self {
        Self {
            path,
            rotated_keep,
            max_bytes,
            file: None,
            len: 0,
            rotate_at: max_bytes,
        }
    }

    fn live_file(&mut self) -> io::Result<&mut File> {
        let file = match self.file.take() {
            Some(file) => file,
            None => {
                let file = OpenOptions::new().create(true).append(true).open(&self.path)?;
                // Picks up where an earlier session (or another process) left the file.
                self.len = file.metadata().map(|m| m.len()).unwrap_or(0);
                file
            }
        };
        Ok(self.file.insert(file))
    }

    /// Whether our open file is still the one at `cmdr.log`. With nothing open, there's nothing
    /// to compare, so it answers yes and rotation goes ahead as before.
    fn holds_the_live_file(&self) -> bool {
        let Some(file) = self.file.as_ref() else {
            return true;
        };
        match (file.metadata(), std::fs::metadata(&self.path)) {
            (Ok(ours), Ok(at_path)) => same_file(&ours, &at_path),
            // `cmdr.log` is gone: rotated away (or deleted) by someone else.
            (_, Err(e)) if e.kind() == io::ErrorKind::NotFound => false,
            // Can't tell: keep the single-process behavior.
            _ => true,
        }
    }

    /// `cmdr.log.<n>`.
    fn sibling(&self, n: usize) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{n}"));
        PathBuf::from(name)
    }

    /// Shift every file one slot older, dropping the oldest, and leave the live slot empty for
    /// the next write to create.
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        if self.rotated_keep == 0 {
            return remove_if_present(&self.path);
        }
        remove_if_present(&self.sibling(self.rotated_keep))?;
        for n in (1..self.rotated_keep).rev() {
            rename_if_present(&self.sibling(n), &self.sibling(n + 1))?;
        }
        rename_if_present(&self.path, &self.sibling(1))
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.live_file()?.write(buf)?;
        self.len += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        if self.len < self.rotate_at {
            return Ok(());
        }
        if !self.holds_the_live_file() {
            // Someone else rotated our file away. Join whatever is at `cmdr.log` now; the next
            // write opens it and counts from its size.
            self.file = None;
            self.len = 0;
            self.rotate_at = self.max_bytes;
            return Ok(());
        }
        match self.rotate() {
            Ok(()) => {
                self.len = 0;
                self.rotate_at = self.max_bytes;
                Ok(())
            }
            Err(e) => {
                self.rotate_at = self.len.saturating_add(self.max_bytes);
                Err(e)
            }
        }
    }
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// No stable file identity in std off unix; assume ours, which is the single-process behavior.
#[cfg(not(unix))]
fn same_file(_a: &std::fs::Metadata, _b: &std::fs::Metadata) -> bool {
    true
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn rename_if_present(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "rotating_file_tests.rs"]
mod tests;
