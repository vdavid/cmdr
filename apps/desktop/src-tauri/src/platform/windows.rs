//! The Windows arm of the platform-identity seam. Placeholder answers until Windows reads its
//! real version (`RtlGetVersion`, or the registry's `ProductName` and `DisplayVersion`).

use super::PlatformCapabilities;

/// Nothing yet: every surface waits for its Windows backend, and the title bar stays native, so
/// Snap Layouts keep working. Image indexing doesn't ship on Windows.
pub(crate) fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        quick_look: false,
        get_info: false,
        open_terminal_here: false,
        text_editor_choice: false,
        cloud_file_actions: false,
        color_tags: false,
        // No app menu: Quit is Cmdr's own command (File > Exit Cmdr), and there's nothing to hide.
        app_menu: false,
        services_menu: false,
        full_disk_access: false,
        finder_extension: false,
        reveal_handler: false,
        dock_pin: false,
        cli_command: false,
        overlay_title_bar: false,
        image_indexing: false,
        // Windows asks nothing for the local network; its firewall prompt is its own thing.
        local_network_permission: false,
        // The bundled model runtime is the macOS ARM64 build only.
        local_ai: false,
        // Only NSDocument writes the `.sb-` files the filter knows.
        safe_save_files: false,
        // No LaunchServices: a folder named `X.app` is a folder.
        app_bundles: false,
        // `view.toggleFullScreen` (F11, View > Full screen).
        full_screen: true,
        // The cask is the macOS distribution.
        homebrew_cask: false,
        // `RegisterHotKey` works for every app on the desktop.
        global_shortcuts: true,
        // Both readings are Mach queries; nothing reads the Windows equivalents yet.
        process_diagnostics: false,
    }
}

/// The bare OS name, `windows`, until the real version is read.
pub(crate) fn os_version() -> String {
    std::env::consts::OS.to_string()
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
}
