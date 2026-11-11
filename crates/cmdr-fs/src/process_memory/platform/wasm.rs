//! The browser build has no process memory to read.

pub(crate) fn footprint() -> Option<u64> {
    None
}
