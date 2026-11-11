//! Linux: `RssAnon` + `VmSwap` from `/proc/self/status`, the anonymous memory this process
//! holds in RAM and in swap. File-backed resident pages (`RssFile`, `RssShmem`) stay out: the
//! kernel drops or shares them under pressure, the way `phys_footprint` leaves them out on macOS.

pub(crate) fn footprint() -> Option<u64> {
    footprint_from_status(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// The footprint a `/proc/<pid>/status` text names, or `None` without an `RssAnon` line (a
/// kernel before 4.5). `VmSwap` absent counts as zero swapped.
pub(crate) fn footprint_from_status(status: &str) -> Option<u64> {
    let kib = |key: &str| {
        status.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?.strip_prefix(':')?;
            rest.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()
        })
    };
    let anon = kib("RssAnon")?;
    let swap = kib("VmSwap").unwrap_or(0);
    Some((anon + swap) * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_resident_plus_swap_in_bytes() {
        let status = "Name:\tcmdr\nVmRSS:\t  900 kB\nRssAnon:\t  600 kB\nRssFile:\t  300 kB\nVmSwap:\t   40 kB\n";
        assert_eq!(footprint_from_status(status), Some(640 * 1024));
    }

    #[test]
    fn no_swap_line_counts_as_none_swapped_and_no_anon_line_is_unknown() {
        assert_eq!(footprint_from_status("RssAnon:\t10 kB\n"), Some(10 * 1024));
        assert_eq!(footprint_from_status("VmRSS:\t10 kB\n"), None);
    }

    /// The real reading on this machine grows by what a live, touched allocation holds.
    #[test]
    fn the_real_reading_sees_a_touched_allocation() {
        const SIZE: usize = 256 * 1024 * 1024;
        let before = footprint().expect("this kernel reports RssAnon");
        let block = vec![1u8; SIZE];
        let after = footprint().expect("this kernel reports RssAnon");
        assert!(std::hint::black_box(&block).iter().step_by(4096).all(|&b| b == 1));
        assert!(
            after >= before + (SIZE as u64) / 2,
            "a touched 256 MiB block shows in the footprint (before {before}, after {after})"
        );
    }
}
