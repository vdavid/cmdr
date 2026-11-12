//! The Linux arm of the platform-identity seam.

pub(crate) mod distro;

use super::PlatformCapabilities;

/// None of the macOS-built surfaces has a Linux backend yet, and the OS draws the title bar. Global
/// shortcuts work on an X11 session only (`x11_session`). Image indexing runs only in the
/// never-shipped Playwright E2E build, on `cmdr-index`'s fake engine
/// (`media_index/scheduler/lifecycle.rs::platform_backends`), so the image-indexing specs run here.
pub(crate) fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        quick_look: false,
        get_info: false,
        open_terminal_here: false,
        text_editor_choice: false,
        cloud_file_actions: false,
        color_tags: false,
        services_menu: false,
        full_disk_access: false,
        finder_extension: false,
        reveal_handler: false,
        dock_pin: false,
        cli_command: false,
        overlay_title_bar: false,
        image_indexing: cfg!(feature = "playwright-e2e"),
        // Linux asks nothing for the local network.
        local_network_permission: false,
        // The bundled model runtime is the macOS ARM64 build only.
        local_ai: false,
        // Only NSDocument writes the `.sb-` files the filter knows.
        safe_save_files: false,
        // No LaunchServices: a folder named `X.app` is a folder.
        app_bundles: false,
        // No way into full screen yet; flips on once a native F11 entry lands.
        full_screen: false,
        // The cask is the macOS distribution.
        homebrew_cask: false,
        global_shortcuts: x11_session(),
        // Both readings are Mach queries; a `/proc` reader would flip this on.
        process_diagnostics: false,
    }
}

/// Whether this desktop session is X11, read once: `global-hotkey` grabs keys only through X11,
/// and on Wayland that grab lands on XWayland and fires only while an XWayland window has focus.
fn x11_session() -> bool {
    use std::sync::OnceLock;
    static X11: OnceLock<bool> = OnceLock::new();
    *X11.get_or_init(|| {
        is_x11_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
    })
}

/// `XDG_SESSION_TYPE` decides when it names `x11` or `wayland`; otherwise (unset, `tty`, junk) a
/// session without `WAYLAND_DISPLAY` is X11.
fn is_x11_session(session_type: Option<&str>, has_wayland_display: bool) -> bool {
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("x11") => true,
        Some("wayland") => false,
        _ => !has_wayland_display,
    }
}

/// `PRETTY_NAME` from `/etc/os-release`, or `Linux (unknown distro)`.
pub(crate) fn os_version() -> String {
    if let Ok(release) = std::fs::read_to_string("/etc/os-release") {
        for line in release.lines() {
            if let Some(name) = line.strip_prefix("PRETTY_NAME=") {
                return name.trim_matches('"').to_string();
            }
        }
    }
    "Linux (unknown distro)".to_string()
}

/// Not macOS, so never at least any macOS version.
pub(crate) fn macos_at_least(_major: i64, _minor: i64) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_at_least_is_false() {
        assert!(!macos_at_least(10, 15));
    }

    #[test]
    fn image_indexing_is_offered_only_by_the_e2e_build_with_its_fake_engine() {
        let offered = capabilities();
        assert_eq!(offered.image_indexing, cfg!(feature = "playwright-e2e"));
        assert!(!offered.full_disk_access);
        assert!(!offered.overlay_title_bar);
    }

    #[test]
    fn the_session_type_decides_x11_and_wayland_display_breaks_the_tie() {
        assert!(is_x11_session(Some("x11"), true));
        assert!(!is_x11_session(Some("wayland"), false));
        assert!(!is_x11_session(Some("Wayland"), false));
        assert!(is_x11_session(None, false));
        assert!(!is_x11_session(None, true));
        assert!(!is_x11_session(Some("tty"), true));
    }
}
