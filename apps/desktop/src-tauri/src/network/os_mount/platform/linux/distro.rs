//! Smbclient install hint, delegating to `crate::platform::LinuxDistro`.

/// Returns the distro-specific install command for smbclient, or `None` if unknown.
pub fn smbclient_install_command() -> Option<String> {
    crate::platform::LinuxDistro::detect()?.install_command("smbclient")
}
