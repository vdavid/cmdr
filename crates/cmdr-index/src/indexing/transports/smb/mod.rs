//! SMB share indexing: enable (the direct-smb2 gate) plus the live
//! `CHANGE_NOTIFY` watch that keeps a Fresh index correct under mutation.

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod index;
pub(crate) mod watch;

// The live-share half of this scanner's coverage lives app-side, in
// `file_system/volume/smb_index_scan_test.rs`: it needs a real `SmbVolume`, which
// only the app can build. The backend-agnostic half is `network_scanner/tests`.
