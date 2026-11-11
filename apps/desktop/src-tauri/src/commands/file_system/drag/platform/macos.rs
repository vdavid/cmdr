//! macOS: an `NSDraggingSession` built on the main thread (`crate::native_drag`), a file-url or a
//! file-promise layout by the session's locality, and the self-drag image swap
//! (`crate::drag_image_swap`).

use crate::file_system::volume::manager::path_routes_over_its_parent;
use crate::native_drag::{self, DragSessionLocality};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use tauri::Manager;

/// Resolves a source volume id to its drag-session locality.
///
/// Keyed on `Volume::paths_are_os_visible()`: the question a drop target asks is
/// whether a `file://` URL built from the path opens in ANOTHER app, not whether
/// Cmdr reads it through `std::fs`. Local disks and OS-mounted shares (direct
/// SMB included, for as long as the share stays mounted alongside the smb2
/// session) keep the file-url layout; protocol-only volumes (MTP,
/// search-results), and a direct SMB share whose mount has gone away under it,
/// advertise nothing materializable and drag as promises. An unknown or absent id (the
/// back-compatible default for callers that don't know their volume) resolves to
/// `Local` — the conservative choice that preserves today's layout.
///
/// ❌ Don't key this on `supports_local_fs_access()`: that asks a different
/// question, and direct SMB answers `false` to it while handing out perfectly
/// openable `/Volumes/…` paths. Promise-only drags are rejected by every target
/// except Finder, so the mistake looks like "drag to Mail/browser silently does
/// nothing" while a Finder drop keeps working.
fn locality_for_volume(volume_id: Option<&str>) -> DragSessionLocality {
    let Some(volume_id) = volume_id else {
        return DragSessionLocality::Local;
    };
    match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(volume) if !volume.paths_are_os_visible() => DragSessionLocality::Virtual,
        _ => DragSessionLocality::Local,
    }
}

/// Whether this drag session can hand out real `file://` URLs, or has to promise
/// the bytes instead.
///
/// A path a ROUTE serves isn't materializable as a local file URL — it lives
/// inside the `.zip`, or inside a repo's virtual `.git` trees — so it forces a
/// VIRTUAL session, a file-promise provider that streams the bytes on the drop,
/// even though the source volume id is the local parent drive. The `.zip` file
/// itself IS a real local file, so it stays a normal Local session, and so does a
/// real file under `.git`.
///
/// ❗ One locality for the whole session, decided here: a mixed session is
/// impossible by construction (one pane, one volume), and the pasteboard plan is
/// keyed on a single value.
fn session_locality(paths: &[PathBuf], source_volume_id: Option<&str>) -> DragSessionLocality {
    if paths.iter().any(|p| path_routes_over_its_parent(p)) {
        return DragSessionLocality::Virtual;
    }
    locality_for_volume(source_volume_id)
}

/// Builds the session for `paths` on the main thread.
pub(crate) fn start_drag(
    app: tauri::AppHandle,
    paths: Vec<String>,
    icon_path: String,
    source_volume_id: Option<String>,
) -> Result<(), String> {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    if path_bufs.is_empty() {
        return Err("No valid files to drag".to_string());
    }
    let locality = session_locality(&path_bufs, source_volume_id.as_deref());
    run_drag_on_main_thread(&app, path_bufs, PathBuf::from(icon_path), locality, source_volume_id)
}

/// Hops to the AppKit main thread, builds the drag session, and returns synchronously.
/// `NSDraggingItem`s and the source class are not `Send`, so everything happens inside
/// the closure; the result travels back via a one-shot channel.
fn run_drag_on_main_thread(
    app: &tauri::AppHandle,
    paths: Vec<PathBuf>,
    icon_path: PathBuf,
    locality: DragSessionLocality,
    source_volume_id: Option<String>,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    let (tx, rx) = channel();

    app.run_on_main_thread(move || {
        let result = native_drag::start_drag(&window, paths, &icon_path, locality, source_volume_id.as_deref());
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    rx.recv().map_err(|_| "Failed to receive drag result")?
}

/// Marks the self-drag active with its rich image.
pub(crate) fn prepare_self_drag_overlay(rich_image_path: String) {
    crate::drag_image_swap::set_self_drag_active(rich_image_path);
}

/// Clears the self-drag state.
pub(crate) fn clear_self_drag_overlay() {
    crate::drag_image_swap::clear_self_drag_state();
}

/// Pushes the resolved operation to the swizzle; unknown values are ignored.
pub(crate) fn set_self_drag_resolved_op(operation: String) {
    use crate::drag_image_swap::SelfDragOp;
    let op = match operation.as_str() {
        "copy" => SelfDragOp::Copy,
        "move" => SelfDragOp::Move,
        _ => return,
    };
    crate::drag_image_swap::set_self_drag_resolved_op(op);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::Volume;
    use crate::test_support::CapabilityStub;
    use std::sync::Arc;

    /// Registers a stub under a unique id and returns the locality the drag path
    /// derives for it. Ids are per-test so the process-wide manager can't make
    /// two tests collide.
    fn locality_of(id: &str, supports_local_fs_access: bool, paths_are_os_visible: bool) -> DragSessionLocality {
        let manager = crate::file_system::volume::manager::get_volume_manager();
        manager.register(
            id,
            Arc::new(CapabilityStub {
                supports_local_fs_access,
                paths_are_os_visible,
            }) as Arc<dyn Volume>,
        );
        let locality = locality_for_volume(Some(id));
        manager.unregister(id);
        locality
    }

    /// The direct-SMB shape: Cmdr's own I/O goes over smb2 (no `std::fs`), but
    /// the share stays OS-mounted, so other apps CAN open the paths. This must
    /// drag as a local session; a virtual one publishes file promises, which
    /// every drop target except Finder rejects.
    #[test]
    fn os_visible_volume_drags_as_local_even_without_local_fs_access() {
        assert_eq!(locality_of("stub-smb-shaped", false, true), DragSessionLocality::Local);
    }

    /// The MTP shape: no local paths at all, so promises are the only honest
    /// offer.
    #[test]
    fn volume_with_no_os_visible_paths_drags_as_virtual() {
        assert_eq!(
            locality_of("stub-mtp-shaped", false, false),
            DragSessionLocality::Virtual
        );
    }

    /// A plain local disk keeps the file-url layout.
    #[test]
    fn local_volume_drags_as_local() {
        assert_eq!(locality_of("stub-local", true, true), DragSessionLocality::Local);
    }

    /// An id nobody registered resolves to `Local`, the back-compatible default
    /// for callers that don't know their volume.
    #[test]
    fn unknown_volume_id_drags_as_local() {
        assert_eq!(
            locality_for_volume(Some("stub-never-registered")),
            DragSessionLocality::Local
        );
        assert_eq!(locality_for_volume(None), DragSessionLocality::Local);
    }

    /// A dragged item that a ROUTE serves has no `file://` URL to hand out, so
    /// the session has to promise the bytes whatever the pane's volume says.
    /// Both namespaces, and neither of their real-file neighbours.
    #[test]
    fn a_routed_path_forces_a_promise_session_and_a_real_one_does_not() {
        use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

        let dir = temp_dir("drag_locality", "snapshot");
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");
        crate::file_system::git::wiring::set_virtual_portal_enabled(true);

        // Inside a repo's history: no inode, so the promise provider streams it.
        assert_eq!(
            session_locality(&[dir.join(".git/branches/main/README.md")], Some("root")),
            DragSessionLocality::Virtual
        );
        // Inside a `.zip`: the case this predicate already covered.
        assert_eq!(
            session_locality(&[dir.join("bundle.zip/inner.txt")], Some("root")),
            DragSessionLocality::Virtual
        );

        // The neighbours that ARE real files on disk keep the local session: the
        // `.zip` itself, a real entry under `.git`, and an ordinary file.
        for real in [dir.join("bundle.zip"), dir.join(".git/config"), dir.join("README.md")] {
            assert_eq!(
                session_locality(std::slice::from_ref(&real), None),
                DragSessionLocality::Local,
                "{} is a real file on disk",
                real.display()
            );
        }

        // And with the portal off, a snapshot path is just a missing local path.
        crate::file_system::git::wiring::set_virtual_portal_enabled(false);
        assert_eq!(
            session_locality(&[dir.join(".git/branches/main/README.md")], None),
            DragSessionLocality::Local
        );
        crate::file_system::git::wiring::set_virtual_portal_enabled(true);

        cleanup(&dir);
    }
}
