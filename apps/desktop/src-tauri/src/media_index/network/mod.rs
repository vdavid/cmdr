//! Network-volume (SMB) enrichment: the conservative byte-fetch pipeline + policy
//! that make an opted-in NAS's images searchable by content (plan M1.5). This is the
//! one part of the plan with no `importance/` sibling to copy — `importance` never
//! reads bytes off the wire; media enrichment must.
//!
//! - [`fetch`] — the byte-fetch decision (reuse the viewer's OS-mount `std::fs` read,
//!   bounded against a hung mount) and the [`fetch::ByteFetcher`] seam.
//! - [`policy`] — the conservative-fetch knobs + the pure idle / bandwidth / override
//!   decisions.
//! - [`config`] — the durable-enough opt-in + "always index" override state (settings-
//!   seeded, live-applied) and the runtime paused-volume set.
//! - [`enrich`] — the pass core: fetch + OCR + GC, resumable and disconnect-paused.
//!
//! The scheduler wiring (route by volume kind; MTP never background-sweeps) lives in
//! [`super::scheduler`]; the read API answers OFFLINE from `media.db` after unmount
//! (plan Decision 8), so a network volume's photos stay searchable with the NAS gone.

pub mod config;
pub mod enrich;
pub mod fetch;
pub mod policy;

#[cfg(test)]
mod tests;
