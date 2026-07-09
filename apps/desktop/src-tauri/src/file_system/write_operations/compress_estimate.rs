//! Compressed-size estimation for the Compress dialog.
//!
//! A cheap deflate-sampling estimator that predicts the compressed output size
//! of a Compress operation while the deep byte-scan runs. Local-FS scans only:
//! it reads a small head window from each file and extrapolates the deflate
//! ratio to the whole file, under a hard byte budget so the cost is bounded
//! regardless of tree size. Remote (SMB/MTP) scans never sample — the estimate
//! is simply absent there (sampling would do real network reads and defeat the
//! scan oracle's zero-I/O short-circuit; an extension-only guess is unbounded).
//!
//! The estimate accumulates as three per-compressibility-class subtotals of
//! estimated **level-6** deflate bytes, so the frontend can re-scale to the
//! user's selected level via a baked per-class curve without re-sampling (the
//! curve lives in `compress-estimate-scaling.ts`). At level 6 the shown value
//! is the plain sum of the three subtotals.
//!
//! Parameters below (window, budget, tiny threshold, extension table) and their
//! measured accuracy/cost are recorded in
//! `docs/notes/compress-size-estimate-spike.md`.

use std::collections::HashMap;
use std::io::{Read, Write as _};
use std::path::Path;

use flate2::Compression;
use flate2::write::DeflateEncoder;

use super::types::CompressedSizeEstimate;

/// Head window sampled per file. 32 KiB matched the 16 MB-budget config's
/// realistic accuracy at half the cost (spike § decision table).
pub(super) const HEAD_WINDOW: usize = 32 * 1024;
/// Hard cap on total bytes read + deflated across one scan. Bounds worst-case
/// added CPU to ~105 ms regardless of tree size; media-heavy trees cost near
/// zero (the incompressible-extension shortcut skips the read).
pub(super) const BYTE_BUDGET: u64 = 8 * 1024 * 1024;
/// Files under this size skip sampling and take the running-average ratio: a
/// head window of a tiny file is unrepresentative, and the 1.0-ratio variant
/// measured 34% worse on source trees.
pub(super) const TINY_THRESHOLD: u64 = 4 * 1024;
/// Reference deflate level the sample is measured at. The frontend re-scales
/// this to the user's selected level via the baked per-class curve (never
/// re-sampled per slider tick).
const REFERENCE_LEVEL: u32 = 6;
/// Ratio assumed for known-incompressible extensions (no read).
const INCOMPRESSIBLE_RATIO: f64 = 0.98;
/// Ratio used before any sample has landed (running-average seed). 0.5 sits in
/// the "medium" class so an unsampled folder reads as a middling guess.
const GLOBAL_DEFAULT_RATIO: f64 = 0.5;

/// Maps a level-6 deflate ratio to its compressibility class index (0 =
/// compressible, 1 = medium, 2 = incompressible). The boundaries match the
/// spike's per-class level-scaling curve, so a file's estimated bytes land in
/// the bucket whose curve the frontend will scale it by.
fn class_index(ratio: f64) -> usize {
    if ratio < 0.35 {
        0
    } else if ratio < 0.8 {
        1
    } else {
        2
    }
}

/// Extensions whose contents are already compressed, so deflate barely shrinks
/// them. Shortcut to `INCOMPRESSIBLE_RATIO` with no file read. Mirrors the
/// spike harness table.
fn is_incompressible_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "heic"
            | "heif"
            | "avif"
            | "bmp"
            | "mp4"
            | "mov"
            | "mkv"
            | "avi"
            | "webm"
            | "m4v"
            | "wmv"
            | "flv"
            | "mp3"
            | "aac"
            | "flac"
            | "ogg"
            | "opus"
            | "m4a"
            | "wav"
            | "zip"
            | "gz"
            | "xz"
            | "7z"
            | "bz2"
            | "zst"
            | "rar"
            | "lz4"
            | "br"
            | "woff"
            | "woff2"
            | "wasm"
            | "jar"
            | "apk"
            | "dmg"
            | "pak"
    )
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Deflated length of `data` at the reference level. Writing to a `Vec` can't
/// fail; on the impossible error path we fall back to the input length (ratio
/// 1.0), which is the safe no-shrink assumption.
fn deflate_len(data: &[u8]) -> usize {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(REFERENCE_LEVEL));
    if encoder.write_all(data).is_err() {
        return data.len();
    }
    encoder.finish().map(|v| v.len()).unwrap_or(data.len())
}

/// Reads up to `window` bytes from the head of `path`. `None` on any open/read
/// error (an unreadable file falls back to the running average, never panics).
fn read_head(path: &Path, window: usize) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(window);
    file.take(window as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Running deflate-ratio average, keyed by extension with a global fallback.
/// Feeds tiny and budget-exhausted files a ratio learned from sampled peers of
/// the same type rather than a fixed guess.
struct RunningAvg {
    per_ext: HashMap<String, (f64, u32)>,
    global: (f64, u32),
}

impl RunningAvg {
    fn new() -> Self {
        Self {
            per_ext: HashMap::new(),
            global: (0.0, 0),
        }
    }

    fn update(&mut self, ext: &str, ratio: f64) {
        let entry = self.per_ext.entry(ext.to_string()).or_insert((0.0, 0));
        entry.0 += ratio;
        entry.1 += 1;
        self.global.0 += ratio;
        self.global.1 += 1;
    }

    fn get(&self, ext: &str) -> f64 {
        if let Some((sum, count)) = self.per_ext.get(ext)
            && *count > 0
        {
            return sum / f64::from(*count);
        }
        if self.global.1 > 0 {
            self.global.0 / f64::from(self.global.1)
        } else {
            GLOBAL_DEFAULT_RATIO
        }
    }
}

/// Accumulates a compressed-size estimate as files are observed. Drive it with
/// one `observe` per regular file, then `finish` for the per-class subtotals.
/// Cheap per-file (a bounded head read + deflate, or a table/average lookup),
/// so it runs on a worker thread fed by the scan walk without touching the
/// walk's critical path.
pub(super) struct CompressEstimator {
    budget_left: u64,
    running: RunningAvg,
    /// Estimated level-6 deflate bytes accumulated per compressibility class,
    /// indexed by `class_index`.
    class_bytes: [f64; 3],
}

impl CompressEstimator {
    pub(super) fn new() -> Self {
        Self::with_budget(BYTE_BUDGET)
    }

    fn with_budget(budget: u64) -> Self {
        Self {
            budget_left: budget,
            running: RunningAvg::new(),
            class_bytes: [0.0; 3],
        }
    }

    /// Feed one regular file (path + full size). Reads a head window only when
    /// the file is worth sampling and the budget allows; otherwise takes the
    /// running average or the incompressible-extension shortcut. Never panics.
    pub(super) fn observe(&mut self, path: &Path, size: u64) {
        let ext = ext_lower(path);
        let ratio = self.ratio_for(path, size, &ext);
        let estimated_level6 = size as f64 * ratio;
        self.class_bytes[class_index(ratio)] += estimated_level6;
    }

    fn ratio_for(&mut self, path: &Path, size: u64, ext: &str) -> f64 {
        if is_incompressible_ext(ext) {
            return INCOMPRESSIBLE_RATIO;
        }
        if size < TINY_THRESHOLD || self.budget_left == 0 {
            return self.running.get(ext);
        }
        let window = HEAD_WINDOW.min(size as usize);
        match read_head(path, window) {
            Some(buf) if !buf.is_empty() => {
                let ratio = deflate_len(&buf) as f64 / buf.len() as f64;
                self.budget_left = self.budget_left.saturating_sub(buf.len() as u64);
                self.running.update(ext, ratio);
                ratio
            }
            // Unreadable (or empty) file: fall back without spending budget.
            _ => self.running.get(ext),
        }
    }

    pub(super) fn finish(self) -> CompressedSizeEstimate {
        CompressedSizeEstimate {
            compressible_bytes: self.class_bytes[0].round() as u64,
            medium_bytes: self.class_bytes[1].round() as u64,
            incompressible_bytes: self.class_bytes[2].round() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn total(est: &CompressedSizeEstimate) -> u64 {
        est.compressible_bytes + est.medium_bytes + est.incompressible_bytes
    }

    #[test]
    fn class_index_boundaries() {
        // Kills off-by-boundary mutants: < 0.35 compressible, [0.35, 0.8) medium,
        // >= 0.8 incompressible.
        assert_eq!(class_index(0.0), 0);
        assert_eq!(class_index(0.349), 0);
        assert_eq!(class_index(0.35), 1);
        assert_eq!(class_index(0.5), 1);
        assert_eq!(class_index(0.799), 1);
        assert_eq!(class_index(0.8), 2);
        assert_eq!(class_index(1.0), 2);
    }

    #[test]
    fn incompressible_extension_shortcut_skips_the_read() {
        // A .jpg path that does not exist on disk: the extension shortcut must
        // return ~0.98 without ever opening the file (no panic, no read error).
        let mut est = CompressEstimator::new();
        est.observe(&PathBuf::from("/does/not/exist/photo.jpg"), 1_000_000);
        let out = est.finish();
        assert_eq!(out.compressible_bytes, 0);
        assert_eq!(out.medium_bytes, 0);
        // 1_000_000 * 0.98 = 980_000, landing in the incompressible bucket.
        assert_eq!(out.incompressible_bytes, 980_000);
    }

    #[test]
    fn highly_compressible_sample_lands_in_compressible_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zeros.bin");
        // 64 KiB of zeros deflates to almost nothing → ratio well under 0.35.
        std::fs::write(&path, vec![0u8; 64 * 1024]).unwrap();

        let mut est = CompressEstimator::new();
        est.observe(&path, 64 * 1024);
        let out = est.finish();

        assert!(out.compressible_bytes > 0, "expected a nonzero compressible estimate");
        assert_eq!(out.medium_bytes, 0);
        assert_eq!(out.incompressible_bytes, 0);
        // Zeros compress to a tiny fraction of the original.
        assert!(
            out.compressible_bytes < 64 * 1024 / 4,
            "zeros should estimate to far under a quarter of the input, got {}",
            out.compressible_bytes
        );
    }

    #[test]
    fn tiny_file_uses_running_average_seed_not_a_read() {
        // A sub-4-KiB file that does NOT exist: it must take the running-average
        // seed (0.5 → medium) without attempting a read.
        let mut est = CompressEstimator::new();
        est.observe(&PathBuf::from("/does/not/exist/note.txt"), 1_000);
        let out = est.finish();
        // 1_000 * 0.5 = 500, in the medium bucket.
        assert_eq!(out.medium_bytes, 500);
        assert_eq!(out.compressible_bytes, 0);
        assert_eq!(out.incompressible_bytes, 0);
    }

    #[test]
    fn budget_exhaustion_falls_back_to_running_average() {
        let dir = tempfile::tempdir().unwrap();
        let sampled = dir.path().join("a.bin");
        std::fs::write(&sampled, vec![0u8; 8 * 1024]).unwrap();

        // Budget = exactly one head window, so the first non-tiny file spends it
        // all and the second must ride the running average without a read.
        let mut est = CompressEstimator::with_budget(HEAD_WINDOW as u64);
        est.observe(&sampled, 8 * 1024);
        // Second file does not exist: proves no read is attempted once the budget
        // is gone (a read would fail, but the running-average path never reads).
        est.observe(&PathBuf::from("/does/not/exist/b.bin"), 8 * 1024);
        let out = est.finish();

        // Both files are highly compressible (`.bin` isn't in the incompressible
        // table; the sampled zeros drive the running average near zero), so both
        // land in the compressible bucket and the total is a small positive.
        assert!(total(&out) > 0);
        assert!(
            out.compressible_bytes > 0,
            "both files should have landed in the compressible bucket via the sample + its running average"
        );
    }

    #[test]
    fn incompressible_extension_table_covers_the_common_media_types() {
        for ext in ["jpg", "png", "mp4", "mov", "mp3", "zip", "gz", "woff2", "wasm"] {
            assert!(is_incompressible_ext(ext), "{ext} should be incompressible");
        }
        for ext in ["txt", "json", "rs", "svelte", "csv", "log", ""] {
            assert!(!is_incompressible_ext(ext), "{ext} should be compressible");
        }
    }

    #[test]
    fn ext_lower_normalizes_case() {
        assert_eq!(ext_lower(&PathBuf::from("/x/Photo.JPG")), "jpg");
        assert_eq!(ext_lower(&PathBuf::from("/x/README")), "");
    }

    #[test]
    fn empty_scan_finishes_at_zero() {
        let out = CompressEstimator::new().finish();
        assert_eq!(total(&out), 0);
    }

    /// Opt-in accuracy harness (`--ignored`): runs the production estimator over
    /// this crate's own `src` tree (a real, portable, highly-compressible source
    /// mix) and compares its summed level-6 prediction against the actual
    /// full-file level-6 deflate of the same tree. Prints the estimate, the
    /// ground truth, and the error, and asserts the error stays well within the
    /// spike's realistic-mix bar. Not part of the normal run (walks the tree and
    /// reads files); run with `cargo test ... estimator_accuracy -- --ignored`.
    #[test]
    #[ignore = "reads the crate src tree; opt-in accuracy check"]
    #[allow(
        clippy::print_stdout,
        reason = "opt-in (--ignored) diagnostic harness reports the measured estimate-vs-actual numbers"
    )]
    fn estimator_accuracy_vs_actual_deflate_on_source_tree() {
        fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    collect(&p, out);
                } else if p.is_file() {
                    out.push(p);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect(&root, &mut files);
        assert!(!files.is_empty(), "expected source files under {}", root.display());

        let mut est = CompressEstimator::new();
        let mut actual: u64 = 0;
        for path in &files {
            let Ok(bytes) = std::fs::read(path) else { continue };
            est.observe(path, bytes.len() as u64);
            actual += deflate_len(&bytes) as u64;
        }
        let out = est.finish();
        let predicted = total(&out);
        let err = (predicted as f64 - actual as f64).abs() / actual as f64 * 100.0;
        println!(
            "estimator accuracy: files={} actual_deflate={} predicted_level6={} (compressible={}, medium={}, incompressible={}) error={:.1}%",
            files.len(),
            actual,
            predicted,
            out.compressible_bytes,
            out.medium_bytes,
            out.incompressible_bytes,
            err
        );
        assert!(
            err < 20.0,
            "estimate error {err:.1}% exceeded the 20% realistic-mix bar"
        );
    }
}
