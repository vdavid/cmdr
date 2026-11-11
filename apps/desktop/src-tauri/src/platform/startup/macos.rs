//! macOS: the system-look observers, the mouse's back and forward gestures, the Services menu,
//! the volume watcher and unmount approver, and the custom updater's state.

use tauri::{AppHandle, Manager};

/// Starts every macOS installer. A step whose position matters says so in its own comment (the
/// Services menu goes in before the menu bar is built).
pub(crate) fn install(app: &AppHandle) {
    // Read which File Provider domains exist and what their extensions declare, on a thread of
    // its own, so the first right-click in Dropbox or Google Drive already finds its actions.
    crate::file_system::file_provider_actions::warm();

    // Stash the AppHandle so the drag-out file-promise machinery can dispatch session cleanup
    // (freeing the retained promise delegates) back to the AppKit main thread once a
    // fulfillment drains.
    crate::native_drag::set_app_handle(app.clone());

    // Start the volume mount/unmount watcher.
    crate::volumes::start_volume_watcher(app);
    // Holds every DA-mediated unmount while the drive's index lets go (`volumes/platform/macos/unmount_approver/`).
    crate::volumes::unmount_approver::install_for_app();

    // Ensure ptpcamerad is re-enabled in case a previous session crashed while it was
    // suppressed. No-op if it was already enabled.
    crate::mtp::macos_workaround::ensure_ptpcamerad_enabled();

    // Follow the accent color, Accessibility > Display (Reduce transparency, Increase contrast,
    // Differentiate without color), the macOS 27 Liquid Glass slider, and Text Size, emitting
    // events to the frontend.
    crate::system_look::observe_changes(app.clone());

    // Watch the mouse's back / forward navigation. macOS only: the mouse's own driver decides
    // what the press becomes, and a Logi Options+ mouse posts a swipe gesture with no button
    // behind it, so AppKit is the only layer that sees both shapes. On Linux the frontend reads
    // the buttons off the DOM.
    crate::mouse_nav::install(app.clone());

    // Before the menu bar is built: AppKit decides what the Services submenu may contain from
    // the send types the app has registered.
    crate::services_menu::install(app);

    // Initialize custom updater state (shared between download and install commands).
    app.manage(crate::updater::UpdateState::new());
}
