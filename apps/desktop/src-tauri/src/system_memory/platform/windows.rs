//! Windows: `sysinfo`'s breakdown, until a finer reader (`GlobalMemoryStatusEx`) earns its place.

use super::SystemMemoryInfo;

pub(crate) fn system_memory_info() -> SystemMemoryInfo {
    super::super::sysinfo_breakdown()
}
