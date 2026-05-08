//! Integration tests for the streaming-suggestion cancel registry.
//!
//! Targets `manager::register_stream / unregister_stream / cancel_stream` since the full
//! `stream_folder_suggestions` Tauri command depends on the global `MANAGER` state.
//! Lower-level coverage of the registry's concurrency + idempotency contract is what
//! we actually want to lock in here — `client_streaming_test.rs` exercises the
//! end-to-end stream pipeline already.

use super::manager::{cancel_stream, register_stream, unregister_stream};

#[test]
fn cancel_one_does_not_cancel_other() {
    let token_a = register_stream("req-a");
    let token_b = register_stream("req-b");

    cancel_stream("req-a");

    assert!(token_a.is_cancelled(), "req-a should be cancelled");
    assert!(!token_b.is_cancelled(), "req-b should NOT be cancelled");

    // Cleanup so we don't leak entries to other tests.
    unregister_stream("req-b");
}

#[test]
fn cancel_unknown_id_is_noop() {
    // Should not panic, should not error. Just a no-op.
    cancel_stream("does-not-exist");
}

#[test]
fn double_cancel_is_idempotent() {
    let token = register_stream("req-double");
    cancel_stream("req-double");
    cancel_stream("req-double"); // second call: registry empty, no-op
    assert!(token.is_cancelled());
}

#[test]
fn unregister_after_cancel_is_safe() {
    register_stream("req-unreg");
    cancel_stream("req-unreg"); // removes the entry
    unregister_stream("req-unreg"); // already removed; should not panic
}

#[test]
fn unregister_without_cancel_does_not_cancel() {
    let token = register_stream("req-unreg-only");
    unregister_stream("req-unreg-only");
    assert!(!token.is_cancelled(), "unregister alone should NOT cancel the token");
    // The orphaned token reference still exists for the test, but no future cancel can
    // find it. This matches the natural-completion path where the task removes its own
    // entry via the RAII guard.
}
