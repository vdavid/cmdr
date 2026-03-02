//! Linux distro detection via `/etc/os-release`.
//!
//! Provides distro family classification and package manager install commands.
//! Detected once and cached for the lifetime of the process.

use std::sync::OnceLock;

/// Parsed Linux distribution info from `/etc/os-release`.
#[derive(Debug)]
pub struct LinuxDistro {
    pub id: String,
    pub id_like: Vec<String>,
    #[allow(dead_code, reason = "Parsed for future UI use (e.g. about dialog)")]
    pub pretty_name: String,
}

/// High-level distro family, determines the package manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    Debian,
    Fedora,
    Arch,
    Suse,
    Unknown,
}

static DETECTED: OnceLock<Option<LinuxDistro>> = OnceLock::new();

impl LinuxDistro {
    /// Returns the detected distro, reading `/etc/os-release` once.
    /// Returns `None` if the file is missing or unparseable.
    #[cfg(target_os = "linux")]
    pub fn detect() -> Option<&'static Self> {
        DETECTED
            .get_or_init(|| {
                let content = std::fs::read_to_string("/etc/os-release").ok()?;
                Self::parse(&content)
            })
            .as_ref()
    }

    /// Parses the content of an os-release file.
    fn parse(content: &str) -> Option<Self> {
        let mut id = String::new();
        let mut id_like = String::new();
        let mut pretty_name = String::new();

        for line in content.lines() {
            if let Some(val) = line.strip_prefix("ID=") {
                id = val.trim_matches('"').to_lowercase();
            } else if let Some(val) = line.strip_prefix("ID_LIKE=") {
                id_like = val.trim_matches('"').to_lowercase();
            } else if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                pretty_name = val.trim_matches('"').to_string();
            }
        }

        if id.is_empty() {
            return None;
        }

        Some(Self {
            id,
            id_like: id_like.split_whitespace().map(String::from).collect(),
            pretty_name,
        })
    }

    /// Classifies this distro into a package-manager family.
    pub fn family(&self) -> DistroFamily {
        let tokens: Vec<&str> = std::iter::once(self.id.as_str())
            .chain(self.id_like.iter().map(String::as_str))
            .collect();

        for t in &tokens {
            if *t == "debian" || *t == "ubuntu" {
                return DistroFamily::Debian;
            }
            if *t == "fedora" || *t == "rhel" || *t == "centos" {
                return DistroFamily::Fedora;
            }
            if *t == "arch" {
                return DistroFamily::Arch;
            }
            if *t == "suse" || *t == "opensuse" || t.starts_with("opensuse") {
                return DistroFamily::Suse;
            }
        }

        DistroFamily::Unknown
    }

    /// Returns the distro-specific install command for the given package, or `None` if unknown.
    pub fn install_command(&self, package: &str) -> Option<String> {
        match self.family() {
            DistroFamily::Debian => Some(format!("sudo apt install {}", package)),
            DistroFamily::Fedora => Some(format!("sudo dnf install {}", package)),
            DistroFamily::Arch => Some(format!("sudo pacman -S {}", package)),
            DistroFamily::Suse => Some(format!("sudo zypper install {}", package)),
            DistroFamily::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distro(content: &str) -> Option<LinuxDistro> {
        LinuxDistro::parse(content)
    }

    #[test]
    fn test_ubuntu() {
        let d = distro("ID=ubuntu\nID_LIKE=debian\nPRETTY_NAME=\"Ubuntu 22.04 LTS\"\n").unwrap();
        assert_eq!(d.id, "ubuntu");
        assert_eq!(d.id_like, vec!["debian"]);
        assert_eq!(d.pretty_name, "Ubuntu 22.04 LTS");
        assert_eq!(d.family(), DistroFamily::Debian);
        assert_eq!(d.install_command("smbclient").unwrap(), "sudo apt install smbclient");
    }

    #[test]
    fn test_fedora() {
        let d = distro("ID=fedora\nVERSION_ID=39\nPRETTY_NAME=\"Fedora Linux 39\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Fedora);
        assert_eq!(
            d.install_command("samba-client").unwrap(),
            "sudo dnf install samba-client"
        );
    }

    #[test]
    fn test_arch() {
        let d = distro("ID=arch\nBUILD_ID=rolling\nPRETTY_NAME=\"Arch Linux\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Arch);
        assert_eq!(d.install_command("smbclient").unwrap(), "sudo pacman -S smbclient");
    }

    #[test]
    fn test_opensuse() {
        let d = distro("ID=opensuse-tumbleweed\nID_LIKE=\"suse\"\nPRETTY_NAME=\"openSUSE Tumbleweed\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Suse);
        assert_eq!(
            d.install_command("samba-client").unwrap(),
            "sudo zypper install samba-client"
        );
    }

    #[test]
    fn test_rhel_derivative() {
        let d = distro("ID=rocky\nID_LIKE=\"rhel centos fedora\"\nPRETTY_NAME=\"Rocky Linux 9\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Fedora);
        assert_eq!(d.id_like, vec!["rhel", "centos", "fedora"]);
    }

    #[test]
    fn test_linux_mint() {
        let d = distro("ID=linuxmint\nID_LIKE=ubuntu\nPRETTY_NAME=\"Linux Mint 21\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Debian);
    }

    #[test]
    fn test_unknown_distro() {
        let d = distro("ID=nixos\nPRETTY_NAME=\"NixOS 23.11\"\n").unwrap();
        assert_eq!(d.family(), DistroFamily::Unknown);
        assert!(d.install_command("smbclient").is_none());
    }

    #[test]
    fn test_empty_content() {
        assert!(distro("").is_none());
    }

    #[test]
    fn test_quoted_id() {
        let d = distro("ID=\"ubuntu\"\nPRETTY_NAME=\"Ubuntu\"\n").unwrap();
        assert_eq!(d.id, "ubuntu");
        assert_eq!(d.family(), DistroFamily::Debian);
    }

    #[test]
    fn test_different_packages() {
        let d = distro("ID=ubuntu\nID_LIKE=debian\n").unwrap();
        assert_eq!(d.install_command("gvfs-smb").unwrap(), "sudo apt install gvfs-smb");
        assert_eq!(d.install_command("gio").unwrap(), "sudo apt install gio");
    }
}
