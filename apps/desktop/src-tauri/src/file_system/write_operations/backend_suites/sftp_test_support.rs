//! What the app-side SFTP suites reach the fixture stack through.
//!
//! The dial itself is `cmdr_sftp::volume::testing`, shared with the backend's own
//! suites. This adds the one thing every app cell repeats: a live volume on a
//! named fixture server plus a scratch directory nothing else in the run will
//! pick, since every cell shares one export and `nextest` runs them in parallel.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_sftp::volume::SftpVolume;
use cmdr_sftp::volume::testing::{FIXTURE_PASSWORD, connect_fixture, fixture_host, fixture_params, scratch_dir};

/// A fixture server the app cells dial, named the way the compose file names it.
#[derive(Clone, Copy)]
pub(super) enum SftpFixture {
    /// Stock OpenSSH: has `posix-rename@openssh.com`, `copy-data`, and
    /// `statvfs@openssh.com`.
    Stock,
    /// None of the three, so a same-server copy streams, a forced rename takes
    /// the non-atomic path, and free space is unknown.
    NoPosixRename,
}

impl SftpFixture {
    fn service_and_port(self) -> (&'static str, u16) {
        match self {
            Self::Stock => ("OPENSSH", 12480),
            Self::NoPosixRename => ("NOPOSIXRENAME", 12486),
        }
    }
}

/// A live volume on `server`, still concrete, for a cell that needs a
/// backend-specific knob or a registration by id.
pub(super) async fn connect(server: SftpFixture) -> SftpVolume {
    let (service, port) = server.service_and_port();
    let params = fixture_params(service, port);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    connect_fixture(&host, params).await
}

/// A live volume on `server` and a scratch directory of its own on the export,
/// already created. `what` says which cell to look at when one leaves a mess.
pub(super) async fn fixture_on(server: SftpFixture, what: &str) -> (Arc<dyn Volume>, PathBuf) {
    let volume = connect(server).await;
    let dir = PathBuf::from(scratch_dir(what));
    volume.create_directory(&dir).await.expect("scratch dir");
    (Arc::new(volume), dir)
}

/// The stock server, which is what most cells want.
pub(super) async fn fixture(what: &str) -> (Arc<dyn Volume>, PathBuf) {
    fixture_on(SftpFixture::Stock, what).await
}
