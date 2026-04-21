# Phase 4 RTT investigation — SMB small-file copy

Investigating whether the Phase 4.1 unification (removing `SmbVolume::export_to_local` / `import_from_local`, routing
everything through `open_read_stream` + `write_from_stream`) regressed per-file wire cost for small SMB reads. Trigger:
~260 ms/file on a Tailscale link with ~137 ms RTT, 100×10 KB copy at nominal 8–10 way concurrency.

## Actual measurement (data, not guess)

Ran the bench with `RUST_LOG='smb2::client::connection=debug,smb2::client::tree=debug,smb2::client::stream=debug'`
through the real `copy_volumes_with_progress` pipeline (QNAP → local temp dir). The bench's `FILE_COUNT` was temporarily
set to 1 and then 3 (edits left uncommitted); `env_logger::try_init()` was added inside the test so the macro output
reaches stderr.

### Environment

- Date: 2026-04-21
- Host: MacBook, off the home subnet, QNAP reached via Tailscale
- QNAP: `100.127.48.122:445`, share `naspi`
- Tailscale RTT (`ping -c 20`): min/avg/max = **49.2 / 59.0 / 218.2 ms** (one outlier pulled max high; typical ~50 ms)
- Commit: `33cb984e` (Docs: Investigate Phase 4.1 SMB small-file RTT hypothesis)
- Files: `_test/bench_100tiny/f_000.bin` through `f_002.bin`, 10 KB each

Note: the 137 ms RTT cited in the original investigation reflected an earlier network condition. The link today is ~2.3×
faster. All gap-analysis numbers below use the measured 59 ms average.

### Per-file wire ops — FILE_COUNT=1

Wire trace during the Copy wall-clock window (after SMB connect + session setup finishes). The destination is
`LocalPosixVolume`, so dest-side SMB ops are zero. Only one SMB session (the main one used by `SmbVolume`) is relevant
here; the watcher spawns its own connection in the background but is out-of-band.

1. `execute_compound: msg_ids=[4,5,6,7]` — `stat` (CREATE + QueryInfo FileBasicInformation + QueryInfo
   FileStandardInformation + CLOSE). 1 RTT.
2. `execute_compound: msg_ids=[8,9,10,11]` — another `stat`, same path, same 4-op compound. 1 RTT.
3. `execute: cmd=Create, msg_id=12, tree_id=…` — open the file for streaming read. 1 RTT.
4. `execute: cmd=Read, msg_id=13` — first and only READ (file fits in one READ). 1 RTT.
5. `execute: cmd=Close, msg_id=14` — close the handle. 1 RTT.

Total: **5 RTTs per file on the read side, 0 on the write side** (local destination).

Source of the two stat probes:

- The first compound `stat` comes from `scan_for_copy` (`smb.rs:246`) called by `copy_volumes_with_progress` at
  `volume_copy.rs:338` during the pre-flight scan phase — it totals file counts and bytes to seed the progress model.
- The second compound `stat` comes from `is_directory` (`smb.rs:691`) called by `copy_single_path` at
  `volume_strategy.rs:42`, which branches between the file-streaming and directory-recursion paths.

Both are 1 RTT each (the smb2 `Tree::stat` helper packs CREATE + two QueryInfo + CLOSE into a single compound request),
but they're redundant for the copy — the scan phase already knows per-file size and `is_directory`, and the streaming
reader re-learns the size anyway from the CREATE response.

Prediction vs. measurement:

- Original guess: 3 RTTs read + 4 RTTs write = 7 per file.
- This is SMB→Local, so write side is 0 anyway — the relevant number is the read side.
- Measured read side: **5 RTTs** (3 for the data path + 2 stat probes). Guess missed both stat probes.

Wall-clock for one file: **328 ms**. Predicted from wire ops: 5 × 59 ms = **295 ms**. The 11% over-spend is plausibly
TCP/TLS buffering and the synchronous tokio hops. **RTT count explains the wall-clock within measurement noise.**

### FILE_COUNT=3 run — concurrency check

Wire trace order (msg_ids in the main session, filtering out the background watcher):

- Scan phase: three sequential `execute_compound` stats, msg_ids `[4–7]`, `[8–11]`, `[12–15]` — one per file.
- `copy_single_path` `is_directory` probes: three sequential `execute_compound` stats, msg_ids `[16–19]`, `[20–23]`,
  `[24–27]`.
- Streaming downloads: file 0 `Create/Read/Close` msg_ids `28, 29, 30`; file 1 `31, 32, 33`; file 2 `34, 35, 36`.
  Strictly sequential — no interleaving between files.

Wall-clock: **838 ms** for 3 files, vs. 328 ms for 1 file. Ratio 2.55× — not the ~1× we'd see if 3-way concurrency were
working. Per-file amortized at FILE_COUNT=3: **279 ms**, which is close to the 1-file number (328 ms).

**Concurrency is not kicking in on the SMB read side.** Root cause: `SmbVolume::open_smb_download_stream` (`smb.rs:354`)
acquires `smb_arc.lock_owned()` and **holds the session mutex for the entire download** (CREATE + READ loop + CLOSE).
With a single SMB session, all downloads serialize on the lock. The `copy_between_volumes` batch picker sets concurrency
to `min(src=10, dst=4..16, 32)` — a nominal 10 — but the session lock flattens that to 1 for the read path. The
scan-phase stats and `is_directory` probes also share the same lock, so they serialize too, even though each individual
op is short.

### Gap analysis

Rewriting the back-of-envelope with measured numbers:

- Measured RTT: 59 ms.
- Measured wire ops per file (SMB→Local): 5 read + 0 write = 5.
- Effective concurrency: 1 (session-mutex bottleneck), not the nominal 10.
- Predicted serial per-file wall clock: 5 × 59 = 295 ms.
- Measured 1-file wall clock: 328 ms. 3-file per-file: 279 ms.
- The original 100×10 KB run observed ~260 ms/file. That lands in the same zone.

The original investigation's "2.7× gap that RTTs alone don't explain" dissolves once we (a) count the two stat probes we
missed, and (b) recognize that concurrency is effectively 1, not 10. 5 RTTs × 59 ms = 295 ms/file, matching the observed
~260–328 ms range directly.

### What the measurement asked for

Before building the compound read/write fast-path, the highest-leverage wins were elsewhere:

1. **Remove the two redundant stat probes in the copy pipeline.** Landed in commit `4683a8d8`: `scan_for_copy` now
   returns `top_level_is_directory`, the copy engine caches it per source in a `HashMap<PathBuf, SourceHint>`, and
   `copy_single_path` takes it as a parameter instead of re-statting. Net: one fewer compound `stat` per file.

2. **Unblock concurrency on the SMB read path.** The session mutex is still held end-to-end per download — this fix is
   blocked pending an smb2 API addition (see "Phase 4 speedup results" below).

3. **Compound fast-path** — `Tree::read_file_compound` / `Tree::write_file_compound`. Landed in commit `7097966f` for
   reads on files ≤ `max_read_size` and writes on files ≤ `max_write_size`. Cuts the 3-RTT CREATE+READ+CLOSE down to 1
   RTT (compound read) and 4-RTT CREATE+WRITE+FLUSH+CLOSE down to 1 RTT (compound write).

### Reproducing

The exact steps used:

```sh
# 1. Add env_logger as a dev-dep (once).
cd apps/desktop/src-tauri && cargo add --dev env_logger

# 2. In the bench, set FILE_COUNT=1 (or 3) and add `let _ = env_logger::try_init();` at the top of the test body.
#    (Leave these edits uncommitted.)

# 3. Run:
export SMB2_TEST_NAS_PASSWORD=$(grep '^SMB2_TEST_NAS_PASSWORD=' ~/projects-git/vdavid/smb2/.env | cut -d= -f2- | tr -d '"')
SMB2_TEST_NAS_PASSWORD="$SMB2_TEST_NAS_PASSWORD" \
  SMB2_TEST_NAS_HOST=100.127.48.122 \
  RUST_LOG='smb2::client::connection=debug,smb2::client::tree=debug,smb2::client::stream=debug' \
  cargo test --release --lib phase4_bench -- --ignored --nocapture --test-threads=1
```

Key gotchas hit along the way (for anyone reproducing):

- The bench's `nas_password_from_env` fallback path assumes the cmdr repo lives at `~/projects-git/vdavid/cmdr`. In a
  worktree under `.claude/worktrees/`, the `smb2/.env` fallback doesn't resolve, so pass `SMB2_TEST_NAS_PASSWORD`
  explicitly.
- The worktree needs `apps/desktop/src-tauri/resources/ai/` to exist; the easiest fix is a symlink to the main repo's
  prebuilt copy (`ln -s ~/projects-git/vdavid/cmdr/apps/desktop/src-tauri/resources/ai <worktree>/…/resources/ai`).
- `RUST_LOG` alone doesn't produce output in `cargo test`; cmdr's production logging is routed via `tauri-plugin-log` at
  runtime, which isn't loaded during unit tests. `env_logger::try_init()` inside the test is the simplest fix.

## The starting hypothesis is wrong

The hypothesis assumed pre-P4.1 used `smb2::Tree::read_file_compound` (CREATE+READ+CLOSE in one compound frame = **1
RTT**), and P4.1 replaced it with a 3-RTT streaming path. That's not what the source says.

- `smb2::Tree::read_file_compound` exists (`smb2/src/client/tree.rs:270`) and is genuinely 1 RTT.
- **cmdr has never called it.** `git log -S read_file_compound` on cmdr returns zero hits. It is a public API in smb2
  but no caller wired it up.

## What SmbVolume actually does on the wire, per file

### Post-P4.1 (current) — streaming read via `open_read_stream`

`SmbVolume::open_read_stream` → `open_smb_download_stream` → `client.download(tree, path)` which calls
`tree.open_file(conn, path)` (single CREATE, `mod.rs:763`) and returns a `FileDownload`. Then `FileDownload::next_chunk`
(`stream.rs:137`) sends one standalone READ per call and one CLOSE once bytes_received ≥ file_size. For a ≤MaxReadSize
file the consumer loop runs once:

- CREATE — 1 RTT (inside `client.download`, before the stream is returned)
- READ — 1 RTT (first `next_chunk`)
- CLOSE — 1 RTT (auto-triggered after the last READ inside the same `next_chunk`)

**Total: 3 sequential RTTs.** The consumer side (`SmbReadStream`) adds no extra SMB ops — just an mpsc channel with
capacity 4.

Write side (`write_from_stream`) uses `FileWriter`: CREATE → N×WRITE → FLUSH → CLOSE. For a 10 KB file that fits in one
WRITE: **4 RTTs** (CREATE + WRITE + FLUSH + CLOSE).

End-to-end read-and-write pipe through `volume_strategy.rs::stream_pipe_file` for a 10 KB Local→SMB or SMB→Local copy:
source 3 RTTs + dest 4 RTTs on independent connections. With 137 ms RTT: 3×137 + 4×137 = 959 ms/file serial. At 10-way
concurrency that's ~96 ms/file effective, but the measurement shows ~260 ms/file. The RTT count alone doesn't fully
explain the number — see "needs measurement" section below.

### Pre-P4.1 — also streaming, via `export_single_file_with_progress`

Before commit `eb99c37c`, SMB→Local went through `export_single_file_with_progress`, which **also** called
`open_smb_download_stream` (commit `a8270909`, Apr 17). Before `a8270909` it called `client.read_file_with_progress`,
which under the hood is `read_file_pipelined_with_progress` (`smb2/src/client/mod.rs:887`): `open_file` (CREATE, 1
RTT) + pipelined READs + `close_handle` (CLOSE, 1 RTT). For a ≤MaxReadSize file the pipelined loop runs one iteration —
same 3 RTTs.

So **pre-P4.1 was also 3 RTTs per small file** for the SMB→Local path. P4.1 did not add RTTs; it swapped one 3-RTT
pipelined reader for another 3-RTT streaming reader.

### Actual measurement — not done

Not run. The source is unambiguous on the RTT count (one CREATE, one READ, one CLOSE; no compound). Running a test would
only confirm what `execute` vs `execute_compound` call sites already tell us.

## If we want to make small-file copies faster, the fix isn't what P4.1 took away

P4.1 didn't cost us a compound path we used to have — there was none. To _gain_ one, we'd add it. Options, ranked:

### Option 1 — compound fast-path inside `open_read_stream` (recommended)

When `open_smb_download_stream` is about to issue `client.download`, peek at the file size first (or skip the peek — see
below) and, for `file_size ≤ max_read_size`, call `tree.read_file_compound` instead. Buffer the returned `Vec<u8>` as
the stream's first and only chunk. Caller sees the normal `VolumeReadStream` API; wire traffic is 1 RTT.

Two sub-problems:

1. **We don't know file size before CREATE.** `open_file` returns size alongside file_id. To decide compound vs
   pipelined before CREATE, we'd need a prior stat (another RTT — defeats the point) or a size hint from the caller (the
   copy path has one — `copy_single_path` scan phase already computed per-file sizes). Alternatively: always try
   compound first; if the READ in the compound returns short, close the (non-existent — compound already closed it)
   handle and open for a pipelined read. Compound inherently handles "file fits in one READ"; if it doesn't, the data is
   truncated, not errored, so we'd need a length check.
2. **The writer side is separate** — we pay 4 RTTs on write regardless. `smb2::Tree` has `write_file_compound`
   (CREATE+WRITE+FLUSH+CLOSE in one compound = 1 RTT) — see `write_file_pipelined`'s fallback at `tree.rs:1757`. For
   SMB-dest small writes we can do the same fast-path in `write_from_stream`.

LOC: ~40 in `smb.rs` for each direction (read + write fast-paths); plus maybe 10 in smb2 to expose a stream-shaped
small-file wrapper if we want to keep `SmbVolume` decoupled from the compound API. Risk: low (compound path is already
unit-tested in smb2). Buys us: **3 RTTs → 1 RTT read, 4 RTTs → 1 RTT write**. For a 137 ms link, per-file wall clock
drops from ~959 ms serial to ~274 ms serial. At 10-way concurrency: from ~96 to ~27 ms/file effective. A roughly 3–4×
improvement on the small-file copy scenario.

### Option 2 — add a "compound stream" to smb2

Generalize: a stream-shaped API backed by a single compound frame when the file fits, and falling back to pipelined
otherwise. Cleaner, but scope creep — `smb2` would grow a new public type. LOC: ~100+. Risk: medium. Buys the same as
Option 1 but with a nicer cmdr-side API. Not worth it just for this; revisit if more consumers show up.

### Option 3 — widen chunk size

Irrelevant. For a 10 KB file the pipelined reader already issues one READ. Chunk size affects large-file throughput, not
small-file RTT count.

## Small-file threshold

If we go with Option 1: the natural ceiling is `max_read_size` (QNAP negotiates ~1 MB; SMB2 spec max is 8 MB). Above
that, the compound can't return all the bytes in one READ, so we lose the benefit anyway.

Recommended cutoff: use compound whenever the caller-provided size hint is `≤ max_read_size`. No reason to pick a
smaller number like 64 KB — the per-file saving grows with file size (1 RTT vs 3 RTTs stays constant, wall-clock saving
grows with the file's READ count). Only reason to be conservative would be memory (compound returns a `Vec<u8>`; stream
yields chunks). At 1 MB that's trivial.

## Is this the whole story?

Medium-high confidence that RTTs are the primary cost, but not 100%. Back-of-envelope: 3 RTTs read + 4 RTTs write =
7×137 ms = 959 ms/file serial. With 10-way concurrency that's ~96 ms/file amortized. Measured: ~260 ms/file. Factor of
~2.7× gap that RTTs alone don't explain. Possible other sources:

- **Concurrency is not actually 10.** `max_concurrent_ops` on SMB is 10 (hardcoded, per `volume/CLAUDE.md`), but
  `copy_between_volumes` takes `min(src, dst, 32)`. If the source is `LocalPosixVolume` returning 4–16 and the dest is
  SMB returning 10, the batch picks the minimum. Also, the SMB session mutex serializes per-connection, and the smb2
  connection pool (see commit `008e9627`) may or may not have multi-connection scaling on the read path.
- **Session mutex contention.** Both read (`open_smb_download_stream` holds `OwnedMutexGuard` for the whole download)
  and write (`write_from_stream` holds `acquire_smb` guard end-to-end) take the session lock. If both source and dest
  are the same SMB volume, concurrency drops to 1 on that volume. For 100×10 KB Local→SMB or SMB→Local it's two
  different volumes, so less of a concern.
- **Per-chunk tokio overhead and mpsc hop.** The channel adds a task-switch per chunk. Negligible per-hop (µs), but at
  260 ms/file it'd take thousands of hops to matter — not plausible.
- **Progress event throttling.** 200 ms throttle is per-operation, not per-file, so it doesn't stall per-file work.
- **Volume scan phase.** `copy_single_path` calls `source_volume.is_directory(source_path)` per file
  (`volume_strategy.rs:42`). For SMB that's a `get_metadata` → CREATE+QueryInfo+CLOSE — on the current code, likely
  another 1–3 RTTs per file that isn't in the "transfer" budget. Worth double-checking the pre-flight cost separately.

What I'd measure to be sure: run one small SMB copy with `RUST_LOG=smb2::client::connection=debug` and count `execute` /
`execute_compound` calls per file. Separately instrument the `volume_strategy::copy_single_path` entry/exit and
source-volume stat calls. If the real wire count per file is 7 (3 read + 4 write), Option 1 is the right fix. If it's
higher (say 10+, including a pre-flight stat or is_directory probe), also look at removing those probes from the hot
path.

## Summary

- Premise was wrong: there was never a compound SMB fast-path in cmdr. Pre- and post-P4.1 both use a 3-RTT streaming
  read (+ 4-RTT streaming write).
- P4.1 didn't regress RTT count. It swapped one 3-RTT reader for another.
- The real win available is **adding** a compound fast-path: `read_file_compound` for small reads, `write_file_compound`
  for small writes, both gated on `size ≤ max_read_size`. Estimated 3–4× wall-clock improvement on high-RTT small-file
  copy workloads.
- Before implementing: measure actual per-file RTT count (expected ~7, may be higher if `is_directory` / stat probes
  also hit the wire) to confirm the RTT diagnosis.

## Phase 4 speedup results (2026-04-21)

Measured after landing commits `4683a8d8` (drop redundant `is_directory` probe) and `7097966f` (compound read/write
fast-path). Tailscale RTT at measurement time: ~50–60 ms average.

### Wall-clock per configuration

| Files | Before (wall-clock / per file) | After (wall-clock / per file) | Speedup |
| ----- | ------------------------------ | ----------------------------- | ------- |
| 1     | 328 ms / 328 ms                | 144 ms / 144 ms               | 2.3×    |
| 3     | 838 ms / 279 ms                | 374 ms / 125 ms               | 2.2×    |
| 100   | ~28 s / ~280 ms                | 10.71 s / 107 ms              | 2.6×    |

(Sub-linear scaling past 3 files is expected — each file still serializes on the SMB session mutex. See concurrency note
below.)

### Per-file wire ops after the fix

Wire trace for the 1-file run (`RUST_LOG='smb2::client::connection=debug,smb2::client::tree=debug'`, `FILE_COUNT=1`):

```
TreeConnect msg_id=3                                          ; one-time per session
execute_compound 4 ops, msg_ids=[4,5,6,7]                      ; scan_for_copy stat — 1 RTT
tree: read_file_compound path=_test\bench_100tiny\f_000.bin
execute_compound 3 ops, msg_ids=[8,9,137]                      ; CREATE+READ+CLOSE — 1 RTT
```

Down from **5 RTTs before** (2× stat + CREATE + READ + CLOSE) to **2 RTTs after** (1× stat + compound
CREATE+READ+CLOSE). Predicted from wire count: 2 × 59 ms = 118 ms. Measured: 144 ms. The ~26 ms over-spend is plausibly
TLS/TCP buffering and the first-file session warm-up.

### Interleaving — concurrency still bottlenecked

Wire trace for the 3-file run shows the three compound reads land strictly sequentially (no interleaving between files
0, 1, and 2):

```
tree: read_file_compound path=_test\bench_100tiny\f_000.bin
execute_compound 3 ops, msg_ids=[16, 17, 145]
tree: read_file_compound done, read 10240 bytes
tree: read_file_compound path=_test\bench_100tiny\f_001.bin
execute_compound 3 ops, msg_ids=[146, 147, 275]
tree: read_file_compound done, read 10240 bytes
tree: read_file_compound path=_test\bench_100tiny\f_002.bin
execute_compound 3 ops, msg_ids=[276, 277, 405]
tree: read_file_compound done, read 10240 bytes
```

Ratio 3-files / 1-file = 374 / 144 = 2.6×. If concurrency were working we'd see ~1×. The session-mutex bottleneck
identified in the original analysis is still in play — the compound fast-path reduces per-file RTTs but can't
parallelize across files while the mutex serializes them. See "Fix 2 blocker" below.

### Fix 2 blocker (unblocking SMB read-path concurrency)

Exposed during implementation: there's no public way to construct an `smb2::FileDownload` without holding
`&mut SmbClient`. `Tree::open_file` and `FileDownload::new` are both `pub(crate)`. Public `Tree::read_file_compound` and
`Tree::read_file_pipelined_with_progress` both buffer the whole file as `Vec<u8>`, which regresses peak memory for large
files from a bounded window (chunk size × channel capacity ≈ a few MB) to the full file size.

To keep the streaming shape and run multiple downloads in parallel on a single `smb2::Connection` (which is `Clone` and
multiplexes concurrent `execute` calls over the same session), we'd need either:

1. A public `Tree::download(&self, conn: &mut Connection, path: &str) -> Result<FileDownload<'_>>`, mirroring the
   existing `pub async fn SmbClient::download` but without requiring the full client. About 5 lines additive in smb2.
2. Or promote `Tree::open_file` + `FileDownload::new` from `pub(crate)` to `pub` so callers can assemble a download from
   the primitives. Larger surface change, same effect.

Either change is small but smb2 releases are batched, so the task directs to stop and report rather than forge ahead.
Once one of those APIs ships, the cmdr side is straightforward: clone the `Tree` + `Connection` under the session lock,
release the lock, drive the download on the clone. With 10-way concurrency unlocked, the 100-file wall-clock should drop
from 10.71 s to roughly 1–2 s (per-file 107 ms → ~15 ms, limited by the scan stat + compound read serialized on the
connection's pipeline window, not by the cmdr-side mutex).

### Stacking plan

- Fix 1 + Fix 3 (landed): **107 ms/file** at 100 files, 2.6× baseline.
- Fix 2 (blocked on smb2 API, see above): projected **~15 ms/file** at 100 files (10-way concurrency, ~2 RTTs / 10 ≈ 12
  ms effective, plus a few ms of tokio/mpsc overhead).

That final number hits the same order of magnitude as a local-to-local copy and matches what `smb2`'s pipelined bench
sees for the same workload.
