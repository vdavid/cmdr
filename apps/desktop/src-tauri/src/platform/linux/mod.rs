//! The Linux arm of the platform-identity seam.

pub(crate) mod distro;

use super::PlatformCapabilities;

/// None of the macOS-built surfaces has a Linux backend yet, and the OS draws the title bar. Image
/// indexing runs only in the never-shipped Playwright E2E build, on `cmdr-index`'s fake engine
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
}
