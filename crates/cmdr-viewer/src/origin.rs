//! Which way a session's file opens: through `std::fs` ([`super::source`]) or through its
//! volume's positioned reads ([`super::volume_source`]). Sits above both readers, which
//! know nothing of each other. `DETAILS.md` § "Byte sources".

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use log::debug;

use super::ViewerError;
use super::host::ViewerHost;
use super::source::{OpenedFile, open_local};
use super::volume_error::map_volume_error;
use super::volume_source::{PositionedRead, open_on_volume, size_on_volume};
use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::{Volume, VolumeError};

/// What a session asks its file's reader to do when the file turns out to have
/// changed under it (`VolumeError::SourceChanged`: SMB refusing to splice two versions
/// after a reconnect): reload, as a watcher seeing the file replaced does. Shared by
/// every source its origin opens, so a reload's fresh source answers the same way, and
/// set once the session exists (`Viewer::reload_on_replacement`).
#[derive(Clone, Default)]
pub struct ReplacementHook(Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>);

impl ReplacementHook {
    /// Runs `on_replaced` whenever a source this hook was handed to finds its file
    /// changed. The first call wins; a later one changes nothing.
    pub(super) fn set(&self, on_replaced: impl Fn() + Send + Sync + 'static) {
        let _ = self.0.set(Box::new(on_replaced));
    }

    /// Tells the session its file changed. Nothing before the session set the hook.
    pub(super) fn fire(&self) {
        if let Some(on_replaced) = self.0.get() {
            on_replaced();
        }
    }
}

/// Where a session's file lives, so the session can open it again: a reload, or a file
/// replaced under it, needs a fresh handle rather than the one it holds.
#[derive(Clone)]
pub enum SourceOrigin {
    /// A path `std::fs` opens.
    Local(PathBuf),
    /// A file read through its volume's positioned reads (`volume_source.rs`).
    Volume {
        volume: Arc<dyn Volume>,
        path: PathBuf,
        way: PositionedRead,
        runtime: Runtime,
        /// Told when the file changes under a reader this origin opened.
        replaced: ReplacementHook,
    },
}

impl SourceOrigin {
    pub fn open(&self) -> Result<OpenedFile, ViewerError> {
        match self {
            Self::Local(path) => open_local(path),
            Self::Volume { .. } => self.open_volume().map_err(|e| map_volume_error(e, None)),
        }
    }

    /// [`open`](Self::open) for a volume origin, keeping the volume's own error so a
    /// caller can tell `NotSupported` (fall back) from a real failure.
    pub fn open_volume(&self) -> Result<OpenedFile, VolumeError> {
        match self {
            Self::Local(_) => Err(VolumeError::NotSupported),
            Self::Volume {
                volume,
                path,
                way,
                runtime,
                replaced,
            } => open_on_volume(volume, path, *way, runtime, replaced),
        }
    }

    /// Has every reader this origin opens, now and on a reload, call `on_replaced` when
    /// its file changes under it. Nothing for a local file: its watcher says so.
    pub(super) fn on_replaced(&self, on_replaced: impl Fn() + Send + Sync + 'static) {
        if let Self::Volume { replaced, .. } = self {
            replaced.set(on_replaced);
        }
    }

    /// The file's size right now, for tail mode's catch-up on enable.
    pub fn current_size(&self) -> Result<u64, ViewerError> {
        match self {
            Self::Local(path) => Ok(std::fs::metadata(path)?.len()),
            Self::Volume {
                volume, path, runtime, ..
            } => size_on_volume(volume, path, runtime),
        }
    }
}

/// A file a session reads through its volume, and whether its path is also open to the
/// OS (a live SMB mount), which is what decides whether a watcher can follow it.
pub(super) struct VolumeOpen {
    pub origin: SourceOrigin,
    pub opened: OpenedFile,
    pub os_visible: bool,
}

/// Opens `requested` through `volume_id`'s random reader when the volume offers one,
/// or `None` to open it the way the viewer always has (`std::fs`, or a temp pull).
///
/// A routed path (inside a `.zip`, a `.git` portal tree) goes to the materializer:
/// its route mints the volume that serves it. Any open error falls back too, so a share
/// whose smb2 session is down still opens through a live mount, and a missing file gets
/// the same `NotFound` the old path gives. The exception is a dead DFS link: the session
/// is fine and already named the trouble, and a mount's answer could be a `NotFound`, which
/// reads as "this file was deleted".
pub(super) fn open_for_viewer(
    host: &ViewerHost,
    runtime: &Runtime,
    requested: &Path,
    volume_id: &str,
) -> Result<Option<VolumeOpen>, ViewerError> {
    match open_through_volume(host, runtime, requested, volume_id, PositionedRead::Held)? {
        // A host with no local files (the browser): a volume that holds no handle reads
        // through `read_range` (the demo's unpaced one), with nothing to fall back on.
        None if !host.local_files => open_through_volume(host, runtime, requested, volume_id, PositionedRead::Ranged),
        found => Ok(found),
    }
}

/// Opens `requested` through `read_range`, for a volume the OS can't open and that holds
/// no random reader: long enough to sniff it and, when it's media, to serve it with no
/// pull (`media_session.rs`). Text keeps its temp copy, so a caller that finds no media
/// drops this. `None` under the same rules as [`open_for_viewer`], plus any volume whose
/// paths `std::fs` already opens.
pub(super) fn open_ranged_for_media(
    host: &ViewerHost,
    runtime: &Runtime,
    requested: &Path,
    volume_id: &str,
) -> Result<Option<VolumeOpen>, ViewerError> {
    let os_visible = host
        .volumes
        .get(volume_id)
        .is_none_or(|volume| volume.paths_are_os_visible());
    if os_visible {
        return Ok(None);
    }
    open_through_volume(host, runtime, requested, volume_id, PositionedRead::Ranged)
}

fn open_through_volume(
    host: &ViewerHost,
    runtime: &Runtime,
    requested: &Path,
    volume_id: &str,
    way: PositionedRead,
) -> Result<Option<VolumeOpen>, ViewerError> {
    if host.volumes.routes_over_its_parent(requested) {
        return Ok(None);
    }
    let Some(volume) = host.volumes.get(volume_id) else {
        return Ok(None);
    };
    let os_visible = volume.paths_are_os_visible();
    let origin = SourceOrigin::Volume {
        volume,
        path: requested.to_path_buf(),
        way,
        runtime: runtime.clone(),
        replaced: ReplacementHook::default(),
    };
    match origin.open_volume() {
        Ok(opened) => Ok(Some(VolumeOpen {
            origin,
            opened,
            os_visible,
        })),
        Err(VolumeError::NotSupported) => Ok(None),
        Err(e @ (VolumeError::LinkTargetUnreachable(_) | VolumeError::LinkNeedsConsent(_))) => {
            Err(map_volume_error(e, None))
        }
        // A host with no local files has no usual way: its volume's answer is the answer, so a read
        // error never turns into the `NotFound` an absent local file would give.
        Err(e) if !host.local_files => Err(map_volume_error(e, None)),
        Err(e) => {
            debug!(
                "viewer: {} doesn't open through its volume ({e}); opening it the usual way",
                cmdr_fs::log_private::LogPrivate::file_path(requested),
                e = cmdr_fs::log_error::LogError(&e)
            );
            Ok(None)
        }
    }
}
