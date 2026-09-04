//! Small cross-cutting platform-identity helpers shared by the diagnostics and analytics
//! pipelines (crash reports, error reports, the heartbeat).

/// A human-readable OS version string, for example `macOS 26.0` or `Ubuntu 24.04 LTS`.
///
/// macOS reads `sw_vers -productVersion`; Linux reads `PRETTY_NAME` from `/etc/os-release`. Both
/// fall back to a generic label rather than failing, so callers always get a non-empty string
/// (the heartbeat contract requires it).
pub(crate) fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return format!("macOS {version}");
            }
        }
        "macOS (unknown version)".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(release) = std::fs::read_to_string("/etc/os-release") {
            for line in release.lines() {
                if let Some(name) = line.strip_prefix("PRETTY_NAME=") {
                    return name.trim_matches('"').to_string();
                }
            }
        }
        "Linux (unknown distro)".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}

/// True when the running macOS is at least `major.minor`. Always false off macOS.
///
/// This is the runtime half of the deployment-floor story. `objc2` carries no
/// availability information, so a selector the running OS doesn't have raises
/// `NSInvalidArgumentException`, which aborts the process instead of returning an
/// error (`scripts/check/checks/desktop-rust-macos-availability.go` has the full
/// account). The bundle's floor is macOS 10.15, so anything newer than that has to
/// ask here first.
///
/// Reads `NSProcessInfo.operatingSystemVersion` (macOS 10.10), cached: the answer
/// can't change while the process runs, and callers sit on menu-build paths.
#[cfg(target_os = "macos")]
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

/// Off macOS every caller is itself `cfg`-gated to macOS, so nothing but the test below reaches
/// this stub. It stays anyway: callers get to ask the question unconditionally instead of each
/// growing its own `cfg`. `allow` rather than `expect`, because the test build DOES use it and an
/// `expect` would go unfulfilled there.
#[allow(
    dead_code,
    reason = "callers are all macOS-gated; the stub spares them their own `cfg`"
)]
#[cfg(not(target_os = "macos"))]
pub(crate) fn macos_at_least(_major: i64, _minor: i64) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_version_is_never_empty() {
        assert!(!os_version().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_at_least_brackets_the_running_version() {
        // Every Mac that can build this is past the bundle's floor, and none is
        // running macOS 99. Anything else would mean the probe read garbage.
        assert!(macos_at_least(10, 15));
        assert!(macos_at_least(11, 0));
        assert!(!macos_at_least(99, 0));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn macos_at_least_is_false_off_macos() {
        assert!(!macos_at_least(10, 15));
    }
}
