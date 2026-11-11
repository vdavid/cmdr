//! Platform identity: which OS this is, and which version. Shared by the diagnostics and
//! analytics pipelines (crash reports, error reports, the heartbeat). Each OS's startup
//! installers sit beside it in `startup/`.
//!
//! A platform seam (`docs/guides/platform-seams.md`): one arm per OS, picked below, and every
//! signature the arms owe pinned, so an arm that drifts fails to compile at the pin.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

pub(crate) mod startup;

const _: fn() -> String = os_version;
const _: fn(i64, i64) -> bool = macos_at_least;

/// A human-readable OS version string, for example `macOS 26.0` or `Ubuntu 24.04 LTS`.
///
/// Never empty: each arm falls back to a generic label rather than failing (the heartbeat
/// contract requires a value).
pub(crate) use imp::os_version;

/// True when the running macOS is at least `major.minor`; always false off macOS.
///
/// This is the runtime half of the deployment-floor story. `objc2` carries no availability
/// information, so a selector the running OS doesn't have raises `NSInvalidArgumentException`,
/// which aborts the process instead of returning an error
/// (`scripts/check/checks/desktop-rust-macos-availability.go` has the full account). The bundle's
/// floor is macOS 10.15, so anything newer than that has to ask here first. Off macOS every caller
/// is itself macOS-only today; the question stays askable everywhere so none needs its own `cfg`.
pub(crate) use imp::macos_at_least;

/// The Linux distribution, for package-manager install hints. Linux-only by nature, so only Linux
/// code reaches it.
#[cfg(target_os = "linux")]
pub(crate) use imp::distro::LinuxDistro;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_version_is_never_empty() {
        assert!(!os_version().is_empty());
    }
}
