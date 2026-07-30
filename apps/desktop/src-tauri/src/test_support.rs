//! Shared test-only helpers for the whole crate.
//!
//! Waiting for background work to land: [`wait_until`] serves sync `#[test]`s,
//! [`wait_until_async`] serves `#[tokio::test]`s. Both live in `cmdr_fs::testing` (every crate in
//! the workspace waits the same way) and are re-exported here so `crate::test_support::wait_until`
//! keeps resolving. Don't hand-roll a poll loop, and don't sleep a fixed span hoping the work
//! landed: the sleep inside those two helpers is the only sanctioned one in Rust test code.
//!
//! **The allocation-counting harness is not here.** `count_allocations` / `heap_bytes_held` and
//! the `#[global_allocator]` behind them live in `indexing::test_support`, because the index
//! subsystems are the ones asserting allocation shape and they're moving to their own crate. A
//! `#[global_allocator]` is per BINARY, so it has to sit in the crate whose test binary is doing
//! the measuring; it can't be shared. `indexing/test_support.rs` says what that costs.

pub(crate) use cmdr_fs::testing::{wait_until, wait_until_async};
