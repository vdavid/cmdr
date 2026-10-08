//! The prelude this crate's suites share.
//!
//! ❌ Not a `use super::*` glob out of `mod.rs`: what a glob pulls in isn't
//! determinable without building, which is what made the SMB extraction's suites
//! impossible to size in advance.

use cmdr_fs::volume::host::VolumeHost;

use super::{AuthRungUsed, SftpConnectionParams, SftpVolume};

/// The remote directory the path suites pretend a volume is rooted at.
pub const TEST_ROOT: &str = "/srv/data";

/// A port in the fixture stack's own range that nothing ever listens on.
///
/// ❗ `127.0.0.1` and a closed port, ❌ never a hostname: a dial that has to
/// resolve a name is at the mercy of whatever the developer's DNS does with it,
/// and one that resolves to a real machine would have a unit cell reaching a
/// stranger's server. A refused connection is instant and means the same thing.
pub const CLOSED_PORT: u16 = 12599;

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
    make_test_volume_with(root, AuthRungUsed::Agent, VolumeHost::detached())
}

/// The same, on a named rung and a host of the caller's choosing.
///
/// The reconnect suites live on this one: the rung is what the policy keys off,
/// and the host is where the connection events and the secret store come from.
pub fn make_test_volume_with(root: &str, rung: AuthRungUsed, host: VolumeHost) -> SftpVolume {
    super::testing::offline_volume(
        "data",
        SftpConnectionParams::new("127.0.0.1", CLOSED_PORT, "ada", root).without_agent(),
        rung,
        host,
    )
}
