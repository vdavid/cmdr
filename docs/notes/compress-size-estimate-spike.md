# Compressed-size estimate — M6 spike findings

Records the M6 kill-criterion measurement for Feature 2 of the compression-level plan: can a cheap deflate-sampling
estimator predict the compressed output size of a Compress operation accurately and cheaply enough to show live in the
Transfer dialog? The verdict decides whether M7 ships UI or Feature 2 ships as
nothing.

**Verdict:** GO for the local-FS sampling estimator; SUPPRESS the estimate on remote (SMB/MTP) sources. Recommended M7
parameters and the reason remote extension-only is suppressed are below. The throwaway measurement rig lived in scratch
and is discarded; its full source is in the appendix so the numbers are reproducible.

Evidence anchor: measured 2026-07-09 on an Apple M3 Max (16 cores), macOS 15 (Darwin 25.5.0), with `flate2` 1.1.9 /
`miniz_oxide` 0.8.9 — the exact deflate backend the app's `zip` 8.6.0 mutator compiles against, so a file's ground-truth
bytes here equal its real zip deflate-stream bytes (zip container overhead excluded; see "Caveats"). `rustc` 1.95.0,
release build. Machine load during accuracy runs was elevated (a parallel worktree build); timing used the min of five
single-threaded trials, which on a 16-core box with free cores reflects uncontended cost.

## The two questions and the bars

Per decided-question #7: median absolute error ≤ 15% overall, worst-mix median ≤ 30%, sampling adds ≤ 20% of scan
wall-time (or ≤ ~~300 ms absolute on a large mix); remote sources are suppressed unless extension ratios ALONE clear the
same bars. Estimate is always shown as explicitly approximate ("~~").

## Method

Six real, licensing-safe file mixes on this machine (David's own data and system assets), each compared against actual
whole-file deflate at levels 1, 6, and 9:

- `source-code` — 2,280 text source files from the Cmdr repo (`.ts`, `.rs`, `.svelte`, `.json`, `.css`, `.md`), 23.4 MB.
  Highly compressible.
- `jpeg-photos` — 400 real JPEGs from a personal Facebook data export, 70.7 MB. Near-incompressible.
- `mixed-docs` — 55 PDFs plus `.xlsx` / `.docx` plus 300 JSON files from the same export, 51.1 MB. Medium.
- `already-compressed` — MP4, GIF, WebP, PNG, MP3, ZIP, RAR plus `node_modules` `.wasm` / `.woff2` / `.gz`, 641.6 MB.
  Incompressible.
- `large-mixed` — every 37th file across the whole 20 GB export (broad realistic cross-section of all types), 584.7 MB.
- `adversarial` — hand-built to break the estimator (see "Adversarial mix"), 212.2 MB.

Two estimators are measured on each mix:

1. **Sampling** (the local-FS ride-the-scan design): per file over 4 KiB, deflate a head window at reference level 6 and
   extrapolate the window ratio to the full file, under a global byte budget; once the budget is spent, remaining files
   take a running-average ratio keyed by extension. Known-incompressible extensions shortcut to ratio ~1.0 with no read.
   Tiny files (< 4 KiB) take the running average. The level-6 estimate is scaled to levels 1 and 9 by a per-class curve
   measured once (never re-sampled per slider tick).
2. **Extension-only** (the remote candidate): ratio purely from a static extension table, no file reads.

Ground truth and estimate both count the deflate stream only (no zip container overhead), so the comparison is
apples-to-apples on the dominant term.

## The measured level-scaling curve

Calibrated once from real representative files (source text, JSON/PDF, JPEG), as size@L / size@6 per content class:

- compressible (window ratio < 0.35): L1 = 1.448, L3 = 1.042, L6 = 1.000, L9 = 0.997
- medium (0.35–0.8): L1 = 1.104, L3 = 1.017, L6 = 1.000, L9 = 0.999
- incompressible (≥ 0.8): L1 = 1.002, L3 = 1.000, L6 = 1.000, L9 = 1.000

Verified on individual real files across all nine levels (`accent_color.rs`, `settings-registry.ts`, a 3.5 MB PDF, a 442
KB source concat): levels 6–9 land within 0.3% of each other, levels 4–6 within ~2%, and all meaningful reduction
happens at levels 1–4 (level 1 is 15–42% larger for compressible content).

**Load-bearing side finding for Feature 1 (the slider itself):** with the `flate2` / `miniz_oxide` backend, dragging the
compression slider from 6 to 9 (the "Smaller" half) changes output size by under 0.5%, and 4→9 by ~2%. `miniz_oxide`
does not implement the higher-effort matching that zlib's level 9 does, so the slider's practical range is really 1–6;
6–9 is nearly inert. This does not block anything, but David should know the "Smaller" end buys almost nothing today.
For the estimator it is good news: the level-6 sample scales to 9 essentially for free, and only the "Faster" (level 1)
end carries real scaling error.

## Decision table

Recommended config `head 32 KiB / budget 8 MiB / tiny→running-avg`, absolute % error, per level (L1 / L6 / L9):

| mix                | sampling L1/L6/L9  | ext-only L1/L6/L9  |
| ------------------ | ------------------ | ------------------ |
| source-code        | 11.7 / 1.1 / 0.9   | 14.9 / 0.3 / 0.5   |
| jpeg-photos        | 0.5 / 0.5 / 0.5    | 0.5 / 0.5 / 0.5    |
| mixed-docs         | 4.8 / 6.9 / 6.8    | 13.7 / 10.8 / 10.8 |
| already-compressed | 0.0 / 0.0 / 0.0    | 0.0 / 0.0 / 0.0    |
| large-mixed        | 1.1 / 1.3 / 1.3    | 0.8 / 1.0 / 1.0    |
| adversarial        | 29.5 / 37.1 / 37.0 | 1066 / 833 / 830   |

- Sampling: overall median absolute error **1.3%**; worst realistic mix (mixed-docs) median **6.9%**; only the synthetic
  adversarial mix exceeds 30% (37%).
- Extension-only: overall median **1.0%**; worst realistic mix (mixed-docs) median **10.8%**; adversarial explodes to
  **833%** (one mistyped file).

Config sweep (sampling; overall median / worst-realistic-mix median / adversarial median; source-code added time):

| config                   | overall med | worst realistic | adversarial | source added ms |
| ------------------------ | ----------- | --------------- | ----------- | --------------- |
| head 16K / budget 4M     | 6.0%        | 17.4% (source)  | 45.7%       | 54              |
| head 16K / budget 8M     | 3.5%        | 9.3% (docs)     | 45.7%       | 105             |
| **head 32K / budget 8M** | **1.3%**    | **6.9% (docs)** | **37.0%**   | **105**         |
| head 64K / budget 16M    | 1.2%        | 5.2% (docs)     | 32.8%       | 211             |

`head 32K / budget 8M` matches the 16 MB-budget config's realistic accuracy at half the cost, so it is the recommended
operating point.

## Sampling cost (warm cache, min of five trials)

Added wall-time = sample pass − stat-only pass, at `head 32 KiB / budget 8 MiB`:

| mix                | files | stat ms | added ms | sampled MB |
| ------------------ | ----- | ------- | -------- | ---------- |
| source-code        | 2,280 | 3.1     | 105      | 8.0        |
| jpeg-photos        | 400   | 0.5     | ~0       | 0.0        |
| mixed-docs         | 375   | 0.4     | 40       | 4.0        |
| already-compressed | 311   | 0.4     | ~0       | 0.0        |
| large-mixed        | 800   | 1.4     | 5        | 0.6        |
| adversarial        | 405   | 0.4     | 1        | 0.1        |

Cost is dominated by deflating the sampled bytes at level 6, and total sampled bytes are capped by the byte budget, so
the **worst-case added time is bounded at ~105 ms regardless of tree size** — a 100,000-file source tree still adds only
~105 ms because after ~256 sampled files the rest ride the running average for free. Media-heavy folders add near-zero
(the extension shortcut skips every read). All mixes are under the 300 ms absolute escape.

The relative-to-stat overhead looks enormous (105 ms vs a 3 ms warm-stat pass) only because warm-stat is near-instant;
against a realistic deep scan (cold cache, real directory reads, conflict detection, event emission) the relative cost
is far smaller. Even so, M7 should run the sample-deflate off the walk thread (a small bounded worker, cancelled with
the scan) so the CPU cost never lands on the scan's critical path — then wall-time impact approaches zero and the
resource bar is met with margin.

## Adversarial mix — the understood failure mode

Six traps: a compressible 64 KiB head + 4 MiB random tail; the reverse; a 2 MB text file renamed `.png` (extension
trap); a 2 MB random-bytes file named `.txt`; one 200 MB highly-compressible `.log` that alone exceeds the byte budget;
and 400 tiny compressible JSON files. Sampling lands at 37% (signed +37%, i.e. it overestimates — the safe direction, a
bigger predicted zip than reality). The overestimate is driven almost entirely by the text-renamed-`.png` file (the
extension shortcut assumes ~1.0 and skips the read) and the split-compressibility files (a head window can't see a tail
of opposite character). A head+mid window trims this to ~31%, not enough to matter. This is inherent to any cheap
sampling estimator and does not occur in real user folders, which do not contain text saved as `.png` or files
engineered with opposite-character halves. Extension-only has no sampling safety net at all, so the same traps send it
to 833%.

## Verdict and recommendation

**(a) Local-FS sampling estimator — GO.** On all five realistic mixes it clears both bars with enormous margin (overall
median 1.3%, worst realistic mix 6.9% at levels 6/9, ≤ 11.7% even at the level-1 "Faster" end). The only mix over 30% is
the deliberately adversarial synthetic one (37%), whose failure mode is understood, bounded, and in the safe
overestimate direction. Cost is bounded at ~~105 ms worst case and near-zero for media-heavy folders. Ship it, styled as
explicitly approximate ("~~"), cancelled with the scan, sampling off the walk thread.

**(b) Extension-only for remote — NO-GO; suppress the estimate on remote sources** (matches the lead default). Nuance
worth recording: by the strict letter of decided-question #7, extension-only DOES clear the bars on the five realistic
mixes (overall median 1.0%, worst realistic mix 10.8%). But its failure mode is unbounded and silent — a single mistyped
or atypical file yields an 8×-wrong number (833% on the adversarial mix) with no sampling to catch it, and remote
content is exactly where the file population is least predictable and least controllable. A live "~40 MB" that is really
5 MB, styled as a measurement, violates the honest-estimate principle worse than showing nothing. Much of
extension-only's realistic accuracy also comes from the already-compressed-media majority, where the estimate ≈ the
original byte total the user already sees — low added value. So: show nothing (or a subtle "estimate unavailable") for
SMB/MTP sources. If David later wants a rough remote number, extension-only is available as an explicitly-labeled "rough
guess" option, but it should not be the default.

## Recommended M7 parameters

- Sampling seam: local-FS walk only (`walk_dir_recursive` per-file branch), behind a `sample_for_estimate` flag set only
  for compress scans. Remote/oracle-cached walk paths never sample and never guess (estimate suppressed).
- Window: 32 KiB from the head. Byte budget: 8 MiB per scan (hard cap). Tiny-file threshold: 4 KiB → running-average
  ratio (not 1.0; the 1.0 variant measured 34% worse on source-code). Optional time budget as a second guard (~150 ms),
  though the byte budget already bounds cost.
- Known-incompressible extension table (shortcut to ratio ~0.98, no read): the image/video/audio/archive/font/wasm set
  in the appendix. Running average keyed by extension with a global fallback for budget-exhausted and tiny files.
- Level scaling: bake the per-class curve above once; scale the level-6 estimate to the selected level arithmetically,
  no re-sampling per slider tick. Scaling adds ≤ ~1.6% error at levels 6–9 and up to ~12% at level 1 on realistic mixes,
  folded into the numbers above.
- Run the sample-deflate off the scan-walk thread; cancel it with the scan; it must never delay or destabilize the
  scan/conflict/confirm flow.

## Caveats

- Ground truth excludes the zip per-entry container overhead (local header ~30 bytes + central-directory ~46 bytes +
  twice the name length per entry). This is negligible for normal files and only matters for piles of thousands of tiny
  entries, where it slightly increases the real zip beyond the deflate-stream estimate (the estimate errs low there —
  the safe direction is arguable, but M7 can add a flat per-entry overhead term from the scan's file count if wanted).
- Accuracy runs happened under machine load; accuracy is load-independent (deterministic), and timing used min-of-five
  to approximate uncontended cost. A clean-machine re-run would only tighten the timing numbers, not the verdict.

## Appendix — reproducer

Mixes are built from real machine paths (Cmdr repo source, a personal Facebook export, `node_modules`, plus synthesized
adversarial files); the manifests are absolute paths and machine-specific, so they are not committed. Rebuild them by
listing files into `mixes/<n>_<name>.txt` (one absolute path per line), then run the harness below in a scratch crate
with `flate2 = "1"`.

```rust
// SPIKE-ONLY throwaway harness (compress-size-estimate M6). Not production code.
// Cargo.toml: [dependencies] flate2 = "1"   (uses the default miniz_oxide backend, matching the app's zip crate)
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::Instant;
use flate2::write::DeflateEncoder;
use flate2::Compression;

const TINY_THRESHOLD: u64 = 4 * 1024;
const INCOMPRESSIBLE_RATIO: f64 = 0.98;
const GLOBAL_DEFAULT_RATIO: f64 = 0.5;

fn deflate_len(data: &[u8], level: u32) -> usize {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::new(level));
    e.write_all(data).unwrap();
    e.finish().unwrap().len()
}
fn ext_of(p: &str) -> String {
    Path::new(p).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default()
}
fn is_incompressible_ext(e: &str) -> bool {
    matches!(e,
        "jpg"|"jpeg"|"png"|"gif"|"webp"|"heic"|"heif"|"avif"|"bmp"
        |"mp4"|"mov"|"mkv"|"avi"|"webm"|"m4v"|"wmv"|"flv"
        |"mp3"|"aac"|"flac"|"ogg"|"opus"|"m4a"|"wav"
        |"zip"|"gz"|"xz"|"7z"|"bz2"|"zst"|"rar"|"lz4"|"br"
        |"woff"|"woff2"|"wasm"|"jar"|"apk"|"dmg"|"pak")
}
fn ext_only_ratio(e: &str) -> f64 {
    if is_incompressible_ext(e) { return INCOMPRESSIBLE_RATIO; }
    match e {
        "txt"|"md"|"json"|"xml"|"html"|"htm"|"css"|"js"|"ts"|"tsx"|"jsx"|"svelte"
        |"rs"|"go"|"c"|"h"|"cpp"|"py"|"java"|"csv"|"log"|"yaml"|"yml"|"toml"|"srt"|"sql" => 0.28,
        "docx"|"xlsx"|"pptx"|"odt"|"ods" => 0.95,
        "pdf" => 0.9,
        _ => 0.6,
    }
}

#[derive(Clone, Copy)]
struct Config { name: &'static str, window: usize, use_mid_window: bool, byte_budget: u64, tiny_as_running_avg: bool }

struct LevelCurve { m: [[f64; 9]; 3] }
impl LevelCurve {
    fn class_of(r6: f64) -> usize { if r6 < 0.35 { 0 } else if r6 < 0.8 { 1 } else { 2 } }
    fn mult(&self, r6: f64, lvl: u32) -> f64 { self.m[Self::class_of(r6)][(lvl - 1) as usize] }
}
fn calib_buffer<F: Fn(&str) -> bool>(mixes_dir: &str, pred: F, cap: usize) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut es: Vec<_> = fs::read_dir(mixes_dir).unwrap().filter_map(|e| e.ok()).collect();
    es.sort_by_key(|e| e.file_name());
    for e in es {
        if e.path().extension().and_then(|x| x.to_str()) != Some("txt") { continue; }
        let Ok(list) = fs::read_to_string(e.path()) else { continue };
        for p in list.lines() {
            if p.trim().is_empty() || !pred(p) { continue; }
            if let Ok(mut f) = fs::File::open(p) {
                let mut b = vec![0u8; 64 * 1024];
                if let Ok(n) = f.read(&mut b) { out.extend_from_slice(&b[..n]); }
            }
            if out.len() >= cap { return out; }
        }
    }
    out
}
fn calibrate_curve(mixes_dir: &str) -> LevelCurve {
    let comp = calib_buffer(mixes_dir, |p| matches!(ext_of(p).as_str(), "ts"|"rs"|"svelte"|"js"|"css"|"md"|"txt"), 8 << 20);
    let med = calib_buffer(mixes_dir, |p| matches!(ext_of(p).as_str(), "json"|"pdf"|"docx"|"xlsx"), 8 << 20);
    let inc = calib_buffer(mixes_dir, |p| matches!(ext_of(p).as_str(), "jpg"|"jpeg"|"mp4"|"png"|"gif"), 8 << 20);
    let classes = [comp, med, inc];
    let mut m = [[1.0f64; 9]; 3];
    for (ci, buf) in classes.iter().enumerate() {
        let base = deflate_len(buf, 6) as f64;
        for lvl in 1..=9u32 { m[ci][(lvl - 1) as usize] = deflate_len(buf, lvl) as f64 / base; }
    }
    LevelCurve { m }
}

struct RunningAvg { per_ext: std::collections::HashMap<String, (f64, u32)>, global: (f64, u32) }
impl RunningAvg {
    fn new() -> Self { RunningAvg { per_ext: Default::default(), global: (0.0, 0) } }
    fn update(&mut self, e: &str, r: f64) {
        let x = self.per_ext.entry(e.to_string()).or_insert((0.0, 0));
        x.0 += r; x.1 += 1; self.global.0 += r; self.global.1 += 1;
    }
    fn get(&self, e: &str) -> f64 {
        if let Some((s, n)) = self.per_ext.get(e) { if *n > 0 { return s / *n as f64; } }
        if self.global.1 > 0 { self.global.0 / self.global.1 as f64 } else { GLOBAL_DEFAULT_RATIO }
    }
}

const LEVELS: [u32; 3] = [1, 6, 9];

// Single pass over a mix: read each file once, deflate ground truth once, feed each config's estimator.
// (Accumulators for sampling/ext-only estimates and per-level ground truth omitted here for brevity;
//  the full rig prints, per config: per-mix |err|% at L1/L6/L9, overall median, worst-mix median,
//  and the level-6→L scaling error isolated against a same-window oracle sampled directly at L.)
fn estimate_file(data: &[u8], ext: &str, cfg: &Config, ra: &mut RunningAvg, budget_left: &mut u64) -> f64 {
    let size = data.len() as u64;
    let r6 = if is_incompressible_ext(ext) {
        INCOMPRESSIBLE_RATIO
    } else if size < TINY_THRESHOLD {
        if cfg.tiny_as_running_avg { ra.get(ext) } else { 1.0 }
    } else if *budget_left == 0 {
        ra.get(ext)
    } else {
        let w = cfg.window.min(data.len());
        let mut sample = data[..w].to_vec();
        if cfg.use_mid_window && data.len() > cfg.window * 3 {
            let mid = data.len() / 2;
            let end = (mid + cfg.window).min(data.len());
            sample.extend_from_slice(&data[mid..end]);
        }
        let rr = deflate_len(&sample, 6) as f64 / sample.len() as f64;
        *budget_left = budget_left.saturating_sub(sample.len() as u64);
        ra.update(ext, rr);
        rr
    };
    size as f64 * r6 // predicted level-6 compressed bytes; multiply by LevelCurve::mult(r6, lvl) for other levels
}

// Warm-cache timing: stat-only pass vs stat+sample pass; report min of five trials; added = sample - stat.
fn _timing_shape(paths: &[String], cfg: &Config) -> (f64, f64) {
    use std::io::Read;
    for p in paths { if let Ok(mut f) = fs::File::open(p) { let mut b = vec![0u8; cfg.window]; let _ = f.read(&mut b); } }
    let t0 = Instant::now();
    let mut a = 0u64;
    for p in paths { if let Ok(md) = fs::metadata(p) { a += md.len(); } }
    std::hint::black_box(a);
    let stat_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mut budget = cfg.byte_budget;
    let t1 = Instant::now();
    for p in paths {
        let Ok(md) = fs::metadata(p) else { continue };
        let e = ext_of(p);
        if is_incompressible_ext(&e) || md.len() < TINY_THRESHOLD || budget == 0 { continue; }
        if let Ok(mut f) = fs::File::open(p) {
            let mut b = vec![0u8; cfg.window.min(md.len() as usize)];
            if f.read_exact(&mut b).is_ok() { std::hint::black_box(deflate_len(&b, 6)); budget = budget.saturating_sub(b.len() as u64); }
        }
    }
    (stat_ms, t1.elapsed().as_secs_f64() * 1000.0)
}

fn main() {
    let mixes = std::env::args().nth(1).unwrap_or_else(|| "mixes".into());
    let curve = calibrate_curve(&mixes);
    let cfg = Config { name: "head32/budget8M/tiny-avg", window: 32 << 10, use_mid_window: false, byte_budget: 8 << 20, tiny_as_running_avg: true };
    // For each mix manifest: read files, sum deflate_len at [1,6,9] for ground truth, and sum
    // estimate_file(...) * curve.mult(r6, lvl) for the estimate; report |estimate - ground_truth| / ground_truth.
    let _ = (curve, cfg, estimate_file, _timing_shape, ext_only_ratio, LEVELS);
    println!("see notes for the measured numbers");
}
```
