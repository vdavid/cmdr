//! Thermal-aware worker-count backoff for parallel enrichment.
//!
//! N enrichment workers pounding the ANE + CPU heat the machine; on an M3 Max a full
//! NAS pass at the max worker count can push `NSProcessInfo.thermalState` up. macOS
//! then throttles the whole system, which hurts the foreground app more than it helps
//! enrichment. So the enrichment pool reads the LIVE thermal pressure between images
//! and backs its EFFECTIVE worker count down under heat, independently of the user's
//! chosen `mediaIndex.parallelism` (the slider is the ceiling; thermal only ever
//! lowers it, never raises it).
//!
//! The state is read as a TYPED enum, never a string label (`no-string-matching`): the
//! diagnostics snapshot stringifies the same `NSProcessInfo` property for its report,
//! but a control-flow decision must branch on the typed pressure, not on wording.

/// macOS thermal pressure, as the typed `NSProcessInfoThermalState` (0 nominal … 3
/// critical). Off macOS there is no such signal, so it reads as [`Nominal`]
/// (parallelism is a macOS-only feature anyway — the real backend is macOS-only).
///
/// [`Nominal`]: ThermalPressure::Nominal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalPressure {
    /// No thermal pressure: run the full chosen worker count.
    Nominal,
    /// Mild pressure: still fine to run the full count (the OS hasn't started throttling).
    Fair,
    /// The OS is actively shedding heat and will throttle: halve the effective workers.
    Serious,
    /// Severe: throttle hard: drop to a single worker.
    Critical,
}

impl ThermalPressure {
    /// Cap `desired` workers by this pressure level. Nominal/Fair pass `desired`
    /// through; Serious halves it (rounding down, floor 1); Critical drops to 1. Never
    /// raises the count, and never returns 0 — a pass always makes forward progress.
    pub fn cap(self, desired: usize) -> usize {
        let desired = desired.max(1);
        match self {
            ThermalPressure::Nominal | ThermalPressure::Fair => desired,
            ThermalPressure::Serious => (desired / 2).max(1),
            ThermalPressure::Critical => 1,
        }
    }
}

/// Read the current macOS thermal pressure (thread-safe Foundation property, no
/// main-thread requirement). Off macOS, always [`ThermalPressure::Nominal`].
#[cfg(target_os = "macos")]
pub fn current_pressure() -> ThermalPressure {
    use objc2_foundation::NSProcessInfo;
    let info = NSProcessInfo::processInfo();
    // The newtype wraps the raw `NSProcessInfoThermalState`: 0 nominal … 3 critical.
    // Branch on the typed discriminant, never on a stringified label.
    match info.thermalState().0 {
        1 => ThermalPressure::Fair,
        2 => ThermalPressure::Serious,
        3 => ThermalPressure::Critical,
        // 0 and any future/unknown value read as no-backoff: the user's chosen count
        // stands unless macOS positively signals heat.
        _ => ThermalPressure::Nominal,
    }
}

/// Off macOS there is no thermal signal.
#[cfg(not(target_os = "macos"))]
pub fn current_pressure() -> ThermalPressure {
    ThermalPressure::Nominal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_and_fair_do_not_back_off() {
        assert_eq!(ThermalPressure::Nominal.cap(8), 8);
        assert_eq!(ThermalPressure::Fair.cap(8), 8);
        assert_eq!(ThermalPressure::Nominal.cap(1), 1);
    }

    #[test]
    fn serious_halves_and_critical_drops_to_one() {
        assert_eq!(ThermalPressure::Serious.cap(8), 4);
        assert_eq!(ThermalPressure::Serious.cap(3), 1);
        assert_eq!(ThermalPressure::Serious.cap(2), 1);
        assert_eq!(ThermalPressure::Critical.cap(16), 1);
    }

    #[test]
    fn cap_never_returns_zero_or_raises() {
        // Floor at 1 even from a zero/one desired, and never above `desired`.
        assert_eq!(ThermalPressure::Serious.cap(0), 1);
        assert_eq!(ThermalPressure::Critical.cap(0), 1);
        for p in [
            ThermalPressure::Nominal,
            ThermalPressure::Fair,
            ThermalPressure::Serious,
            ThermalPressure::Critical,
        ] {
            for desired in 1..=16 {
                let capped = p.cap(desired);
                assert!((1..=desired).contains(&capped), "{p:?}.cap({desired}) = {capped}");
            }
        }
    }
}
