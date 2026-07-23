//! M2 scaling spike (ignored, measurement-only): does parallel Vision enrichment
//! actually scale, and where does it plateau?
//!
//! The plan (`docs/specs/resource-use-plan.md` § M2) mandates measuring
//! decode-vs-inference scaling at N ∈ {1, 2, 4, 8} BEFORE building the worker pool, so
//! the milestone's success metric comes from a measurement rather than an asserted
//! multiplier. This test is the measurement, not a regression assertion, so it's
//! `#[ignore]`d — run it by hand against a real local image dir:
//!
//! ```sh
//! CMDR_SPIKE_DIR="$HOME/Downloads" cargo test -p cmdr --release --lib \
//!   media_index::backend::vision::spike -- --ignored --nocapture
//! ```
//!
//! It measures two curves over the SAME image set:
//! - **decode-only**: N threads each call [`super::decode_thumbnail`] (ImageIO decode,
//!   the CPU part) — the piece most likely to scale with cores.
//! - **full analyze**: N independent [`VisionOcrBackend`]s (each its own 8 MB-stack
//!   thread), N driver threads each running OCR + classify + feature-print (the ANE
//!   inference part, which serializes in the framework).
//!
//! The gap between the two curves is the decode-vs-inference split the plan asks for.
//! CLIP is intentionally excluded (it funnels through ONE global CLIP worker thread, so
//! it can't scale here regardless of N — noted in the plan).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use objc2::rc::autoreleasepool;

use crate::media_index::backend::{ImageInput, VisionBackend};
use crate::media_index::predicate::MediaKind;

use super::VisionOcrBackend;

/// The parallelism levels the plan names.
const LEVELS: [usize; 4] = [1, 2, 4, 8];

/// The most images to measure over (the plan asks for ~200).
const MAX_IMAGES: usize = 200;

/// Collect up to [`MAX_IMAGES`] real image paths from `CMDR_SPIKE_DIR` (default
/// `~/Downloads`), recursing a couple of levels. Returns absolute paths.
fn collect_images() -> Vec<String> {
    let dir = std::env::var("CMDR_SPIKE_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").expect("HOME set");
        format!("{home}/Downloads")
    });
    let exts = ["jpg", "jpeg", "png", "heic", "heif", "tiff", "tif", "webp", "gif"];
    let mut out = Vec::new();
    collect_into(PathBuf::from(&dir), 0, &exts, &mut out);
    out.truncate(MAX_IMAGES);
    out
}

fn collect_into(dir: PathBuf, depth: usize, exts: &[&str], out: &mut Vec<String>) {
    if out.len() >= MAX_IMAGES || depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_IMAGES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_into(path, depth + 1, exts, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && exts.iter().any(|w| w.eq_ignore_ascii_case(ext))
        {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

fn input(path: &str) -> ImageInput {
    ImageInput {
        path: path.to_string(),
        kind: MediaKind::Image,
        bytes: None,
    }
}

/// Run `work` over `paths` split across `n` threads pulling from a shared cursor (so a
/// slow image doesn't leave a thread's static chunk lagging), and return the wall-clock
/// duration. `make` builds the per-thread state (`VisionOcrBackend`, or `()` for the
/// decode-only path) once per thread.
fn timed<S, M, W>(paths: &[String], n: usize, make: M, work: W) -> std::time::Duration
where
    S: Send,
    M: Fn() -> S + Send + Sync,
    W: Fn(&S, &str) + Send + Sync,
{
    let cursor = AtomicUsize::new(0);
    let make = &make;
    let work = &work;
    let cursor = &cursor;
    let start = Instant::now();
    thread::scope(|scope| {
        for _ in 0..n {
            scope.spawn(move || {
                let state = make();
                loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    work(&state, path);
                }
            });
        }
    });
    start.elapsed()
}

#[test]
#[ignore = "measurement spike; run by hand against a real image dir (see module docs)"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement spike prints its scaling table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn parallelism_scaling_spike() {
    let paths = collect_images();
    assert!(
        paths.len() >= 20,
        "need a real image dir with >=20 files; set CMDR_SPIKE_DIR (found {})",
        paths.len()
    );
    let count = paths.len();
    eprintln!("\n=== M2 enrichment scaling spike ===");
    eprintln!("images: {count} (from CMDR_SPIKE_DIR or ~/Downloads)\n");

    // Warm up: load the Vision models / spin the ANE once so the first timed config
    // doesn't eat the one-time model-load cost.
    {
        let warm = VisionOcrBackend::new();
        for path in paths.iter().take(10) {
            let _ = warm.analyze_media(&input(path), true, false);
        }
    }

    let mut decode_rates = Vec::new();
    let mut analyze_rates = Vec::new();

    for &n in &LEVELS {
        // Decode-only (ImageIO decode, the CPU part), each thread on an 8 MB stack with a
        // per-image autoreleasepool (mirroring the real worker), no dedicated Vision thread.
        let d = timed(
            &paths,
            n,
            || (),
            |(), path| {
                autoreleasepool(|_| {
                    let _ = super::decode_thumbnail(path, None);
                });
            },
        );
        let d_rate = count as f64 / d.as_secs_f64();
        decode_rates.push(d_rate);

        // Full analyze: N independent backends (N dedicated Vision threads), N drivers.
        let backends: Vec<Arc<VisionOcrBackend>> = (0..n).map(|_| Arc::new(VisionOcrBackend::new())).collect();
        let next = AtomicUsize::new(0);
        let a = timed(
            &paths,
            n,
            || {
                let idx = next.fetch_add(1, Ordering::Relaxed);
                backends[idx].clone()
            },
            |backend, path| {
                let _ = backend.analyze_media(&input(path), true, false);
            },
        );
        let a_rate = count as f64 / a.as_secs_f64();
        analyze_rates.push(a_rate);

        eprintln!(
            "N={n}: decode-only {:.1} img/s ({:.2}x)  |  full analyze {:.1} img/s ({:.2}x)",
            d_rate,
            d_rate / decode_rates[0],
            a_rate,
            a_rate / analyze_rates[0],
        );
    }

    eprintln!("\nfull-analyze speedup vs N=1:");
    for (i, &n) in LEVELS.iter().enumerate() {
        eprintln!("  N={n}: {:.2}x", analyze_rates[i] / analyze_rates[0]);
    }
    eprintln!("=== end spike ===\n");
}
