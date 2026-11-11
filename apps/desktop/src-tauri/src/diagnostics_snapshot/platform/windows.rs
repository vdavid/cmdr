//! Windows: nothing yet. Every field reads as unknown until Windows grows its readers
//! (`GetDiskFreeSpaceExW` for the volume, `GetLogicalProcessorInformationEx` for the cores).

use std::path::Path;

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

pub(crate) fn thermal_state() -> Option<String> {
    None
}

pub(crate) fn volume_space(_path: &Path) -> (Option<u64>, Option<u64>) {
    (None, None)
}
