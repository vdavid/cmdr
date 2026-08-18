//! Linux registration surface for the volume commands.
//!
//! The commands themselves are cross-platform and live in `commands/volumes.rs`.
//! This module exists only because `ipc.rs` and `ipc_collectors.rs` register the
//! Linux set under this path; ❌ don't add anything here.

pub use super::volumes::*;
