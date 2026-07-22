//! SMB share indexing: enable (the direct-smb2 gate) plus the live
//! `CHANGE_NOTIFY` watch that keeps a Fresh index correct under mutation.

pub(crate) mod watch;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod index;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "integration_test.rs"]
mod integration_test;
