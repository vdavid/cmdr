//! Linux: `sysinfo`'s breakdown.

use super::SystemMemoryInfo;

pub(crate) fn system_memory_info() -> SystemMemoryInfo {
    super::super::sysinfo_breakdown()
}
