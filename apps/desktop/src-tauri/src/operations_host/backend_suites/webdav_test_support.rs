//! What the app-side WebDAV suites reach the fixture stack through.
//!
//! The dial itself is `cmdr_webdav::volume::testing`, shared with the backend's
//! own suites. This adds the one thing every app cell repeats: a live volume on
//! a named fixture server plus a scratch directory nothing else in the run will
//! pick, since every cell shares one export and `nextest` runs them in parallel.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_webdav::WebdavVolume;
use cmdr_webdav::volume::testing::{connect_fixture, scratch_dir};

/// A fixture server the app cells dial, named the way the compose file names it.
#[derive(Clone, Copy)]
pub(super) enum WebdavFixture {
    /// Stock Apache `mod_dav` behind Basic auth.
    Stock,
    /// The same export under `MaxRanges none`: every ranged GET comes back 200
    /// with the whole file, so a remote zip's windows are skipped to locally.
    IgnoresRange,
    /// The same export behind a server that drops a request body after 3 s of
    /// silence, so a paused upload outlasts it in seconds.
    Impatient,
}

impl WebdavFixture {
    fn service_and_port(self) -> (&'static str, u16) {
        match self {
            Self::Stock => ("APACHE", 13480),
            Self::IgnoresRange => ("NORANGE", 13483),
            Self::Impatient => ("IMPATIENT", 13485),
        }
    }
}

/// A live volume on `server`, still concrete, for a cell that needs a
/// backend-specific knob.
pub(super) async fn connect(server: WebdavFixture) -> WebdavVolume {
    let (service, port) = server.service_and_port();
    connect_fixture(service, port).await
}

/// A live volume on `server` and a scratch directory of its own on the export,
/// already created.
pub(super) async fn fixture_on(server: WebdavFixture) -> (Arc<dyn Volume>, PathBuf) {
    let volume = connect(server).await;
    let dir = scratch_dir(&volume).await;
    (Arc::new(volume), dir)
}

/// The stock server, which is what most cells want.
pub(super) async fn fixture() -> (Arc<dyn Volume>, PathBuf) {
    fixture_on(WebdavFixture::Stock).await
}
