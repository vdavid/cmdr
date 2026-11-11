//! Linux: the volume's space from `statfs`, and nothing yet for the machine identity fields.

pub(crate) use super::unix::volume_space;

pub(crate) fn os_build() -> Option<String> {
    None
}

pub(crate) fn hardware_model() -> Option<String> {
    None
}

pub(crate) fn cpu_physical() -> Option<u32> {
    None
}

pub(crate) fn cpu_logical() -> Option<u32> {
    None
}

/// The kernel has no counterpart to macOS's thermal pressure level.
pub(crate) fn thermal_state() -> Option<String> {
    None
}
