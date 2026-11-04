//! What a test runs the viewer on: one shared [`Viewer`] over a detached host whose volumes
//! a test registers itself ([`test_volumes`]), on `cmdr-fs`'s fallback runtime. Nothing
//! routes, pulls, fetches, or opens a link; a local path opens through `std::fs`.
//!
//! Gated on `any(test, feature = "testing")`, so a host's own tests reach the same viewer.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;

use crate::Viewer;
use crate::host::ViewerHost;
use crate::host::volumes::ViewerVolumes;

/// The volumes a test registered, by id. Nothing routes over its parent.
#[derive(Default)]
pub struct TestVolumes {
    volumes: Mutex<HashMap<String, Arc<dyn Volume>>>,
}

impl TestVolumes {
    /// Serves `volume` under `id`, replacing what was there.
    pub fn register(&self, id: &str, volume: Arc<dyn Volume>) {
        self.volumes.lock_ignore_poison().insert(id.to_string(), volume);
    }

    /// Stops serving `id`.
    pub fn unregister(&self, id: &str) {
        self.volumes.lock_ignore_poison().remove(id);
    }
}

impl ViewerVolumes for TestVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes.lock_ignore_poison().get(volume_id).cloned()
    }

    fn routes_over_its_parent(&self, _path: &Path) -> bool {
        false
    }
}

static VOLUMES: LazyLock<Arc<TestVolumes>> = LazyLock::new(|| Arc::new(TestVolumes::default()));

static VIEWER: LazyLock<Viewer> = LazyLock::new(|| {
    let host = ViewerHost {
        volumes: Arc::clone(&*VOLUMES) as Arc<dyn ViewerVolumes>,
        ..ViewerHost::detached()
    };
    Viewer::new(host, VolumeHost::detached().runtime())
});

/// The viewer tests open sessions on.
pub fn test_viewer() -> &'static Viewer {
    &VIEWER
}

/// The volumes [`test_viewer`] reads through.
pub fn test_volumes() -> &'static TestVolumes {
    &VOLUMES
}
