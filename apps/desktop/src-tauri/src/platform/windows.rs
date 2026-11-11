//! The Windows arm of the platform-identity seam. Placeholder answers until Windows reads its
//! real version (`RtlGetVersion`, or the registry's `ProductName` and `DisplayVersion`).

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
