//! How long a streamed write is and how far it got: [`StreamLength`] and the
//! [`StreamWriteProgress`] every `write_from_stream` reports.

/// Whether a stream's final byte length is known before writing starts.
///
/// Real files carry [`Known`](Self::Known). Generated streams may carry
/// [`Unknown`](Self::Unknown), so callers never disguise an absent length as
/// zero. A backend must explicitly opt into unknown-length writes before it can
/// receive one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamLength {
    /// The stream must contain exactly this many bytes.
    Known(u64),
    /// The final byte count is unavailable until the stream reaches EOF.
    Unknown,
}

impl StreamLength {
    /// Returns the exact byte count when one was declared.
    pub const fn known(self) -> Option<u64> {
        match self {
            Self::Known(bytes) => Some(bytes),
            Self::Unknown => None,
        }
    }
}

/// One progress update from a streaming write.
///
/// Build it with [`new`](Self::new) (a count that proves nothing about order)
/// or [`in_order`](Self::in_order) (a destination that writes every byte in
/// order), then [`continuing_from`](Self::continuing_from) for a destination
/// that proves its prefix separately from its live count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamWriteProgress {
    /// Bytes accepted by the destination so far: what the progress bar shows.
    pub bytes_written: u64,
    /// The declared final length, or unknown for generated output.
    pub expected_length: StreamLength,
    /// How far a write that fails now can be carried on from
    /// ([`Volume::continue_write_from_stream`](super::Volume::continue_write_from_stream)):
    /// every byte before it is on the target, in order, and stays there when
    /// the write fails. `0` when nothing is proven (a destination that doesn't
    /// continue writes, or one that hasn't reached its first checkpoint).
    /// Never past `bytes_written`.
    pub continues_from: u64,
}

impl StreamWriteProgress {
    /// A count that proves nothing about which bytes landed: pipelined or
    /// out-of-order writes, or a destination that never continues one.
    pub const fn new(bytes_written: u64, expected_length: StreamLength) -> Self {
        Self {
            bytes_written,
            expected_length,
            continues_from: 0,
        }
    }

    /// A count from a destination that writes every byte in order, so the
    /// count is itself the proven prefix (a local `write_all`).
    pub const fn in_order(bytes_written: u64, expected_length: StreamLength) -> Self {
        Self {
            bytes_written,
            expected_length,
            continues_from: bytes_written,
        }
    }

    /// The same update, with `proven` bytes known to be a contiguous prefix of
    /// the target (SMB's last flushed checkpoint). Clamped to `bytes_written`.
    #[must_use]
    pub const fn continuing_from(self, proven: u64) -> Self {
        Self {
            continues_from: if proven < self.bytes_written {
                proven
            } else {
                self.bytes_written
            },
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proven_prefix_never_runs_past_the_live_count() {
        let length = StreamLength::Known(100);
        assert_eq!(StreamWriteProgress::new(40, length).continues_from, 0);
        assert_eq!(StreamWriteProgress::in_order(40, length).continues_from, 40);
        assert_eq!(
            StreamWriteProgress::new(40, length).continuing_from(30).continues_from,
            30
        );
        assert_eq!(
            StreamWriteProgress::new(40, length).continuing_from(50).continues_from,
            40
        );
    }
}
