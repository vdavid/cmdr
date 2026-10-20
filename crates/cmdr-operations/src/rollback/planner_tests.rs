//! The operation log's rollback planner, driven end to end by this module's
//! production executor (`Reversal`) over a real writer and volume registry.
//!
//! The planner lives in `cmdr-operation-log`, which can't see the executor, so
//! these cells live here, beside it. Its pure decision helpers are unit-tested in
//! that crate.

#[cfg(target_os = "macos")]
mod blocked_items_tests;
mod case_twin_tests;
mod control_tests;
mod lifecycle_tests;
mod root_reach_tests;
mod test_support;
mod tests;
mod undo_tests;
