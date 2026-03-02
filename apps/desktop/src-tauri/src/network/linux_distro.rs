//! Smbclient install hint, delegating to the crate-level `LinuxDistro`.

/// Returns the distro-specific install command for smbclient, or `None` if unknown.
#[cfg(target_os = "linux")]
pub fn smbclient_install_command() -> Option<String> {
    crate::linux_distro::LinuxDistro::detect()?.install_command("smbclient")
}
