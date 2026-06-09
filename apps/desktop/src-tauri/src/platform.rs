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
