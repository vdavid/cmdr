//! What a test outside this subsystem needs to drive media enrichment, behind
//! the same `testing` feature the index uses.
//!
//! ❌ Not part of the API. One app-side integration test fetches image bytes off
//! a real SMB share through the enrichment path's own fetcher, to prove the two
//! agree; `#[cfg(test)]` can't reach across a crate boundary to let it.

/// Pulling image bytes for enrichment: the trait, the volume-backed
/// implementation the network pass uses, and what it fails with.
pub use crate::media_index::network::fetch::{ByteFetcher, FetchError, VolumeByteFetcher};
