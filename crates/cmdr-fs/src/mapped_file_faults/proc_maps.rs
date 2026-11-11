//! Reading Linux's `/proc/self/maps` with no allocation, for a signal handler.
//!
//! Pure byte work, compiled everywhere so both halves are tested on every host: [`Lines`] splits
//! a stream fed in arbitrary chunks into lines held in a fixed buffer, and [`parse`] reads the
//! fields the fault recovery needs from one line.

#![cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    allow(dead_code, reason = "only Linux reads /proc/self/maps")
)]

/// The fields of one `/proc/self/maps` line before the path:
/// `start-end perms offset major:minor inode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MapsLine {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) readable: bool,
    pub(super) writable: bool,
    pub(super) executable: bool,
    /// The backing file's device, `(major << 32) | minor`.
    pub(super) device: u64,
    /// The backing file's inode, 0 for anonymous memory, the stack, the heap, and `[vdso]`.
    pub(super) inode: u64,
}

impl MapsLine {
    /// Whether a file backs the mapping.
    pub(super) fn is_file_backed(&self) -> bool {
        self.inode != 0
    }

    /// Whether `other` maps the same file.
    pub(super) fn same_file(&self, other: &MapsLine) -> bool {
        self.is_file_backed() && self.device == other.device && self.inode == other.inode
    }
}

/// Parses the leading fields of a maps line, `None` when they aren't all there.
pub(super) fn parse(line: &[u8]) -> Option<MapsLine> {
    let mut fields = line.split(|byte| *byte == b' ').filter(|field| !field.is_empty());
    let (start, end) = split_once(fields.next()?, b'-')?;
    let perms = fields.next()?;
    let _offset = fields.next()?;
    let (major, minor) = split_once(fields.next()?, b':')?;
    let inode = fields.next()?;
    if perms.len() < 3 {
        return None;
    }
    Some(MapsLine {
        start: hex(start)?,
        end: hex(end)?,
        readable: perms[0] == b'r',
        writable: perms[1] == b'w',
        executable: perms[2] == b'x',
        device: (hex(major)? << 32) | hex(minor)?,
        inode: decimal(inode)?,
    })
}

fn split_once(field: &[u8], separator: u8) -> Option<(&[u8], &[u8])> {
    let at = field.iter().position(|byte| *byte == separator)?;
    Some((&field[..at], &field[at + 1..]))
}

fn hex(digits: &[u8]) -> Option<u64> {
    number(digits, 16)
}

fn decimal(digits: &[u8]) -> Option<u64> {
    number(digits, 10)
}

fn number(digits: &[u8], radix: u32) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u64, |value, byte| {
        let digit = char::from(*byte).to_digit(radix)?;
        value.checked_mul(u64::from(radix))?.checked_add(u64::from(digit))
    })
}

/// How much of a line [`Lines`] keeps. The fields [`parse`] reads take under 100 bytes; the rest
/// is the path, which nothing here reads, so a longer line is cut rather than refused.
pub(super) const LINE_CAPACITY: usize = 256;

/// Splits a byte stream into lines, holding at most [`LINE_CAPACITY`] bytes of each.
pub(super) struct Lines {
    line: [u8; LINE_CAPACITY],
    len: usize,
}

impl Lines {
    pub(super) const fn new() -> Self {
        Self {
            line: [0; LINE_CAPACITY],
            len: 0,
        }
    }

    /// Feeds the next chunk, calling `on_line` for each line it completes. `on_line` answers
    /// whether to keep going; `feed` answers `false` once it said stop.
    pub(super) fn feed(&mut self, chunk: &[u8], on_line: &mut impl FnMut(&[u8]) -> bool) -> bool {
        for byte in chunk {
            if *byte == b'\n' {
                let keep_going = on_line(&self.line[..self.len]);
                self.len = 0;
                if !keep_going {
                    return false;
                }
            } else if self.len < LINE_CAPACITY {
                self.line[self.len] = *byte;
                self.len += 1;
            }
        }
        true
    }

    /// Hands over a last line that had no newline.
    pub(super) fn finish(&mut self, on_line: &mut impl FnMut(&[u8]) -> bool) {
        if self.len > 0 {
            on_line(&self.line[..self.len]);
            self.len = 0;
        }
    }
}
