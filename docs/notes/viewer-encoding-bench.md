# Viewer newline scanner throughput

Captures the throughput of `NewlineScanner::feed` (see `apps/desktop/src-tauri/src/file_viewer/encoding.rs`) on
synthetic ASCII and UTF-16 LE buffers. The numbers anchor the perf budget for opening large logs: the line-index build
is one full pass through the file at scan speed, so a 1 GB log takes ~100 ms to index in UTF-8 and ~300 ms in UTF-16 LE
on the captured hardware.

## Measurement methodology

- **Harness**: `newline_scan_throughput_bench` in `apps/desktop/src-tauri/src/file_viewer/encoding_test.rs`. Marked
  `#[ignore]` so it doesn't run in CI; run manually before declaring perf-sensitive work done.
- **Input**: 64 MB synthetic buffer per encoding. ASCII uses one `\n` every 100 bytes (close to a realistic log-line
  cadence). UTF-16 LE uses one `U+000A` every 100 code units. Both buffers fit comfortably in L2/L3 cache, so the
  numbers reflect scan throughput in isolation from disk and main-memory latency.
- **Counters**: `Instant::elapsed()` around `find_newlines(buf, encoding)`. Each encoding is timed in a single shot; we
  take the first measurement (cache-cold reads are within ~5% on this hardware so the spread isn't worth quoting).
- **Hardware** (2026-05-28): MacBook Pro M-series, native build. Release profile (`cargo nextest run --release`).
  Background load: editor + browser.

## Run command

```bash
cargo nextest run --release -p cmdr --run-ignored only \
    -E 'test(newline_scan_throughput_bench)' --no-capture
```

## Captured throughput

| Encoding                   | Time (64 MB) | Throughput |
| -------------------------- | -----------: | ---------: |
| UTF-8 (`memchr` fast path) |      6.48 ms | 10.35 GB/s |
| UTF-16 LE (manual scanner) |     19.49 ms |  3.44 GB/s |

The UTF-8 path uses SIMD-accelerated `memchr_iter`. UTF-16 takes the manual byte-pair scanner so a `0x0A` byte inside a
non-newline code unit (such as `U+010A` or the low surrogate of an astral codepoint) doesn't trip the scan.

## Budget interpretation

- **1 GB log**:
  - UTF-8: ~100 ms scan. Comfortable inside the 5 s `INDEXING_TIMEOUT_SECS` on slow disks; the disk read dominates
    anyway.
  - UTF-16 LE: ~300 ms scan. Still comfortable; ~3× UTF-8, matches the plan's acceptance criterion ("UTF-16 scan ≤ 3×
    UTF-8").
- **5 GB log**:
  - UTF-8: ~500 ms.
  - UTF-16 LE: ~1.5 s.
- **The `find_newlines` short circuit** for ASCII-compatible encodings keeps the byte path identical to `memchr`. Any
  future encoding that gains `is_ascii_newline_compatible() == true` automatically rides the fast path.

## When to re-measure

- Touching `NewlineScanner::feed`'s hot loop.
- Touching the chunk size constant (`256 KB` today; smaller chunks would hurt UTF-16 by increasing carry-byte stitching
  frequency).
- Switching to a different `encoding_rs` major version or replacing the per- byte UTF-16 logic.
- On a meaningfully different host (Linux x86_64, Apple Silicon model bump).

If the UTF-16 path drops below ~1 GB/s, the line-index build on a 5 GB UTF-16 log breaks the "open feels instant" feel
and we need to revisit the scanner.
