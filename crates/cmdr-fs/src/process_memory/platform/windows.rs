//! No reader on Windows yet. Its footprint is `PROCESS_MEMORY_COUNTERS_EX::PrivateUsage`
//! from `K32GetProcessMemoryInfo`; until then the watchdog doesn't start.

pub(crate) fn footprint() -> Option<u64> {
    None
}
