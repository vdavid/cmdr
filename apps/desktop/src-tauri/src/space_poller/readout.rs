//! What a pane's disk-space readout shows, as numbers, so the poller emits only when it would change.
//!
//! The frontend writes free and total space with `formatDriveFigure` (the friendliest unit, as
//! precise as the drive's size makes worth reading: about one step per pixel of the usage bar, and
//! a tenth below ten), the status bar's free percent and the usage bar's used percent as whole
//! numbers, and the low-disk-space toast's free percent to one decimal. [`DisplayedSpace`] is those
//! figures at those resolutions, never the text: the webview owns locale, separators, and unit
//! labels. Two readings with equal `DisplayedSpace` draw the same pixels, so emitting the second
//! would repaint both status bars for nothing.
//!
//! ❗ Mirrors `src/lib/units/byte-size.ts` (`formatDriveFigure`, `DRIVE_STEPS`) and
//! `src/lib/file-explorer/disk-space-utils.ts` (the percentages). Both sides test against
//! `src/lib/units/drive-figure-cases.json`, so a precision change on one side fails the other's
//! test until it follows.

use serde::Deserialize;

use crate::file_system::volume::SpaceInfo;

/// The `appearance.fileSizeFormat` setting: which base the size units step by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum FileSizeFormat {
    /// Base 1024 (`KiB`, `MiB`, `GiB`).
    Binary,
    /// Base 1000 (`kB`, `MB`, `GB`), the setting's default.
    #[default]
    Si,
}

impl FileSizeFormat {
    /// The setting's stored value, or the default for anything it doesn't recognize.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("binary") => Self::Binary,
            _ => Self::Si,
        }
    }

    fn base(self) -> f64 {
        match self {
            Self::Binary => 1024.0,
            Self::Si => 1000.0,
        }
    }
}

/// How many steps a drive's figures resolve it into: about one per pixel of a pane's usage bar.
const DRIVE_STEPS: f64 = 1000.0;

/// One size as the readout draws it: the unit step, the fraction digits, and the value scaled by
/// `10^digits` and rounded (plain bytes below the first step, which render as a whole number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisplayedSize {
    unit: u8,
    digits: u8,
    scaled: u64,
}

/// The unit ladder's top rung (`PB`), past which values keep growing in that unit.
const LARGEST_UNIT: u8 = 5;

/// `bytes` as the readout draws it. On a drive (`drive_bytes`), the precision follows its size, as
/// `formatDriveFigure` does; with no drive size (storage with no ceiling), two decimals.
fn displayed_size(bytes: u64, drive_bytes: Option<u64>, format: FileSizeFormat) -> DisplayedSize {
    let base = format.base();
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= base && unit < LARGEST_UNIT {
        value /= base;
        unit += 1;
    }
    if unit == 0 {
        return DisplayedSize {
            unit,
            digits: 0,
            scaled: bytes,
        };
    }
    let digits = match drive_bytes {
        Some(drive_bytes) => drive_digits(value, base.powi(i32::from(unit)), drive_bytes),
        None => 2,
    };
    DisplayedSize {
        unit,
        digits,
        scaled: (value * 10f64.powi(i32::from(digits))).round() as u64,
    }
}

/// The fraction digits of a figure of `value` units of `unit_bytes` on a drive of `drive_bytes`:
/// the most (up to two) whose step is still at least one of [`DRIVE_STEPS`], and a tenth below ten.
fn drive_digits(value: f64, unit_bytes: f64, drive_bytes: u64) -> u8 {
    let pixel_bytes = drive_bytes as f64 / DRIVE_STEPS;
    let pixel_digits = [2u8, 1]
        .into_iter()
        .find(|digits| unit_bytes / 10f64.powi(i32::from(*digits)) >= pixel_bytes)
        .unwrap_or(0);
    // The live form's rule, decided on the value as shown.
    let live_digits = u8::from((value * 10.0).round() / 10.0 < 10.0);
    pixel_digits.max(live_digits)
}

/// Every figure a disk-space readout draws, at the resolution it draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisplayedSpace {
    Bounded {
        free: DisplayedSize,
        total: DisplayedSize,
        /// The status bar's `(42%)`: whole percent free.
        free_percent: u64,
        /// The toast's `4.2%`: free percent in tenths, while the low-disk-space toast is up.
        toast_free_permille: Option<u64>,
        /// The usage bar's fill and colour band: whole percent used.
        used_percent: u64,
    },
    Unbounded {
        used: DisplayedSize,
    },
}

/// `space` as its readout draws it under `format`. `toast_up`: the low-disk-space toast is showing
/// this volume's free percent to a tenth, so that counts too.
pub(super) fn displayed_space(space: &SpaceInfo, format: FileSizeFormat, toast_up: bool) -> DisplayedSpace {
    match *space {
        SpaceInfo::Bounded {
            total_bytes,
            available_bytes,
            ..
        } => {
            // `getUsedPercent` counts used as what isn't free, not the volume's own used figure.
            let (free_fraction, used_fraction) = if total_bytes == 0 {
                (0.0, 0.0)
            } else {
                let free = available_bytes as f64 / total_bytes as f64;
                let used = total_bytes.saturating_sub(available_bytes) as f64 / total_bytes as f64;
                (free, used)
            };
            DisplayedSpace::Bounded {
                free: displayed_size(available_bytes, Some(total_bytes), format),
                total: displayed_size(total_bytes, Some(total_bytes), format),
                free_percent: rounded_percent(free_fraction, 1.0),
                toast_free_permille: toast_up.then(|| rounded_percent(free_fraction, 10.0)),
                used_percent: rounded_percent(used_fraction, 1.0),
            }
        }
        SpaceInfo::Unbounded { used_bytes } => DisplayedSpace::Unbounded {
            used: displayed_size(used_bytes, None, format),
        },
    }
}

/// `fraction` as a percent in steps of `1 / per_point`, clamped to 0–100 like the frontend's.
fn rounded_percent(fraction: f64, per_point: f64) -> u64 {
    (fraction.clamp(0.0, 1.0) * 100.0 * per_point).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    fn bounded(total: u64, available: u64) -> SpaceInfo {
        SpaceInfo::bounded(total, available)
    }

    fn same(a: &SpaceInfo, b: &SpaceInfo, format: FileSizeFormat, toast_up: bool) -> bool {
        displayed_space(a, format, toast_up) == displayed_space(b, format, toast_up)
    }

    #[test]
    fn a_gigabyte_move_on_a_1_tb_drive_draws_differently() {
        let before = bounded(926 * GIB, 261 * GIB + 205 * MIB);
        let after = bounded(926 * GIB, 260 * GIB + 100 * MIB);
        assert!(!same(&before, &after, FileSizeFormat::Binary, false));
    }

    #[test]
    fn the_base_decides_where_the_digits_roll_over() {
        // 999.6 MB (SI) rounds to 1000 MB and 999.4 MB to 999 MB, while in binary both read 953 MB.
        let a = bounded(10 * GIB, 999_600_000);
        let b = bounded(10 * GIB, 999_400_000);
        assert!(!same(&a, &b, FileSizeFormat::Si, false));
        assert!(same(&a, &b, FileSizeFormat::Binary, false));
    }

    #[test]
    fn an_unbounded_volume_draws_its_used_figure() {
        let a = SpaceInfo::Unbounded { used_bytes: 64 * MIB };
        let b = SpaceInfo::Unbounded {
            used_bytes: 64 * MIB + 1024,
        };
        let c = SpaceInfo::Unbounded { used_bytes: 70 * MIB };
        assert!(same(&a, &b, FileSizeFormat::Binary, false));
        assert!(!same(&a, &c, FileSizeFormat::Binary, false));
    }

    #[test]
    fn bytes_below_the_first_step_are_exact() {
        let a = SpaceInfo::Unbounded { used_bytes: 1000 };
        let b = SpaceInfo::Unbounded { used_bytes: 1001 };
        assert!(!same(&a, &b, FileSizeFormat::Binary, false));
    }

    #[test]
    fn an_unknown_total_reads_as_nothing_free() {
        // Guarded like the frontend, which never divides by a zero total.
        assert!(matches!(
            displayed_space(&bounded(0, 0), FileSizeFormat::Binary, false),
            DisplayedSpace::Bounded {
                free_percent: 0,
                used_percent: 0,
                ..
            }
        ));
    }

    /// `displayed_size` rendered the way the webview's `formatDriveFigure` writes it in en-US.
    fn en_text(size: DisplayedSize, format: FileSizeFormat) -> String {
        let labels = match format {
            FileSizeFormat::Binary => ["bytes", "KiB", "MiB", "GiB", "TiB", "PiB"],
            FileSizeFormat::Si => ["bytes", "kB", "MB", "GB", "TB", "PB"],
        };
        let digits = usize::from(size.digits);
        let value = size.scaled as f64 / 10f64.powi(i32::from(size.digits));
        format!("{value:.digits$} {}", labels[usize::from(size.unit)])
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        bytes: u64,
        drive_bytes: u64,
        format: FileSizeFormat,
        text: String,
    }

    #[derive(serde::Deserialize)]
    struct Cases {
        cases: Vec<Case>,
    }

    #[test]
    fn rounds_every_drive_figure_to_what_the_webview_writes() {
        // The same table `formatDriveFigure` is tested against, so the emit gate and the text on
        // screen can't disagree about when the figure changed.
        let table: Cases = serde_json::from_str(include_str!("../../../src/lib/units/drive-figure-cases.json"))
            .expect("the shared drive-figure table parses");
        for case in table.cases {
            let size = displayed_size(case.bytes, Some(case.drive_bytes), case.format);
            assert_eq!(en_text(size, case.format), case.text, "{}", case.name);
        }
    }

    const TIB: u64 = 1024 * GIB;

    #[test]
    fn a_1_tb_drive_moving_by_hundreds_of_megabytes_draws_the_same() {
        // 261 GB of 926 GB: the readout moves in whole gigabytes, about one pixel of the bar.
        let before = bounded(926 * GIB, 261 * GIB + 100 * MIB);
        let after = bounded(926 * GIB, 261 * GIB + 400 * MIB);
        assert!(same(&before, &after, FileSizeFormat::Binary, false));
    }

    #[test]
    fn a_nearly_full_drive_still_shows_a_tenth() {
        let before = bounded(926 * GIB, 2 * GIB + 300 * MIB);
        let after = bounded(926 * GIB, 2 * GIB + 420 * MIB);
        assert!(!same(&before, &after, FileSizeFormat::Binary, false));
    }

    #[test]
    fn an_8_mb_card_shows_ten_kilobyte_moves() {
        let total = 7 * MIB + 512 * 1024;
        let before = bounded(total, 3 * MIB + 205 * 1024);
        let after = bounded(total, 3 * MIB + 215 * 1024);
        assert!(!same(&before, &after, FileSizeFormat::Binary, false));
    }

    #[test]
    fn the_toast_percent_counts_only_while_the_toast_is_up() {
        // 1.23 TB of 3.64 TB either way, and 34% free either way, but the toast's 33.7% turns 33.9%.
        let total = 4_000_000_000_000;
        let before = bounded(total, TIB + 231 * GIB);
        let after = bounded(total, TIB + 239 * GIB);
        assert!(same(&before, &after, FileSizeFormat::Binary, false));
        assert!(!same(&before, &after, FileSizeFormat::Binary, true));
    }

    #[test]
    fn the_setting_parses_with_si_as_the_default() {
        assert_eq!(FileSizeFormat::from_setting(Some("si")), FileSizeFormat::Si);
        assert_eq!(FileSizeFormat::from_setting(Some("binary")), FileSizeFormat::Binary);
        assert_eq!(FileSizeFormat::from_setting(None), FileSizeFormat::Si);
        assert_eq!(FileSizeFormat::from_setting(Some("bogus")), FileSizeFormat::Si);
        assert_eq!(FileSizeFormat::default(), FileSizeFormat::Si);
    }
}
