//! The macOS arm of the platform-identity seam.

use super::PlatformCapabilities;

/// Everything: each surface was built on a macOS API (Quick Look, `NSWorkspace`, File Provider,
/// TCC, the Finder Sync extension, LaunchServices, the overlay title bar of
/// `tauri.macos.conf.json`, Vision and Core ML for images).
pub(crate) fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        quick_look: true,
        get_info: true,
        open_terminal_here: true,
        text_editor_choice: true,
        cloud_file_actions: true,
        color_tags: true,
        services_menu: true,
        full_disk_access: true,
        finder_extension: true,
        reveal_handler: true,
        dock_pin: true,
        cli_command: true,
        overlay_title_bar: true,
        image_indexing: true,
        local_network_permission: true,
        local_ai: true,
        safe_save_files: true,
        app_bundles: true,
        full_screen: true,
        homebrew_cask: true,
        global_shortcuts: true,
        // The Mach queries behind both readings (`task_info`, `mach_vm_region`, `thread_info`).
        process_diagnostics: true,
    }
}

/// `macOS <version>` from `sw_vers -productVersion`, or `macOS (unknown version)`.
pub(crate) fn os_version() -> String {
    if let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            return format!("macOS {version}");
        }
    }
    "macOS (unknown version)".to_string()
}

/// Reads `NSProcessInfo.operatingSystemVersion` (macOS 10.10), cached: the answer can't change
/// while the process runs, and callers sit on menu-build paths.
pub(crate) fn macos_at_least(major: i64, minor: i64) -> bool {
    use objc2_foundation::NSProcessInfo;
    use std::sync::OnceLock;

    static RUNNING: OnceLock<(i64, i64)> = OnceLock::new();
    let (running_major, running_minor) = *RUNNING.get_or_init(|| {
        let version = NSProcessInfo::processInfo().operatingSystemVersion();
        (version.majorVersion as i64, version.minorVersion as i64)
    });
    (running_major, running_minor) >= (major, minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_at_least_brackets_the_running_version() {
        // Every Mac that can build this is past the bundle's floor, and none is
        // running macOS 99. Anything else would mean the probe read garbage.
        assert!(macos_at_least(10, 15));
        assert!(macos_at_least(11, 0));
        assert!(!macos_at_least(99, 0));
    }
}
