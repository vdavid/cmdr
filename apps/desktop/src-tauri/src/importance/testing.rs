//! What a test outside this subsystem needs to drive folder importance, behind
//! the same `testing` feature the index uses.
//!
//! ❌ Not part of the API. It exists because two app-side tests seed a scored
//! folder and then assert on what search and the MCP resource make of it, and
//! `#[cfg(test)]` can't reach across a crate boundary.

/// Writing weights straight into a volume's importance database, so a test can
/// stage a scored folder without running a scoring pass.
pub use crate::importance::store::importance_db_path;
pub use crate::importance::writer::{ImportanceWriter, WeightRow};

/// Announcing a completed recompute, which is what a subscriber waits on.
pub use crate::importance::read::notify_recompute_completed_for_test;
