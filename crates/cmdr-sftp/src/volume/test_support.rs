//! The prelude this crate's suites share.
//!
//! ❌ Not a `use super::*` glob out of `mod.rs`: what a glob pulls in isn't
//! determinable without building, which is what made the SMB extraction's suites
//! impossible to size in advance.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;

use super::{AuthRungUsed, SftpConnectionParams, SftpVolume, SftpVolumeInner};

/// The remote directory the path suites pretend a volume is rooted at.
pub const TEST_ROOT: &str = "/srv/data";

/// A volume with NO session behind it, for everything that is a pure function
/// over the root.
///
/// Every operation that would touch the wire answers `DeviceDisconnected`, which
/// is exactly right: these cells assert on translation, never on I/O.
pub fn make_test_volume() -> SftpVolume {
    make_test_volume_at(TEST_ROOT)
}

/// The same, rooted wherever the caller needs.
pub fn make_test_volume_at(root: &str) -> SftpVolume {
    SftpVolume {
        name: "data".to_string(),
        root: PathBuf::from(root),
        inner: Arc::new(SftpVolumeInner {
            volume_id: cmdr_fs::volume::sftp_volume_id("naspolya", 22, "ada"),
            params: SftpConnectionParams::new("naspolya", 22, "ada", root),
            rung: AuthRungUsed::Agent,
            session: tokio::sync::RwLock::new(None),
            host: VolumeHost::detached(),
        }),
    }
}
