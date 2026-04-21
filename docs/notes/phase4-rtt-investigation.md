# Phase 4 RTT investigation — SMB small-file copy

Investigating whether the Phase 4.1 unification (removing `SmbVolume::export_to_local` /
`import_from_local`, routing everything through `open_read_stream` + `write_from_stream`) regressed
per-file wire cost for small SMB reads. Trigger: ~260 ms/file on a Tailscale link with ~137 ms RTT,
100×10 KB copy at nominal 8–10 way concurrency.

## The starting hypothesis is wrong

The hypothesis assumed pre-P4.1 used `smb2::Tree::read_file_compound` (CREATE+READ+CLOSE in one
compound frame = **1 RTT**), and P4.1 replaced it with a 3-RTT streaming path. That's not what
the source says.

- `smb2::Tree::read_file_compound` exists (`smb2/src/client/tree.rs:270`) and is genuinely 1 RTT.
- **cmdr has never called it.** `git log -S read_file_compound` on cmdr returns zero hits. It is a
  public API in smb2 but no caller wired it up.

## What SmbVolume actually does on the wire, per file

### Post-P4.1 (current) — streaming read via `open_read_stream`

`SmbVolume::open_read_stream` → `open_smb_download_stream` → `client.download(tree, path)` which
calls `tree.open_file(conn, path)` (single CREATE, `mod.rs:763`) and returns a `FileDownload`. Then
`FileDownload::next_chunk` (`stream.rs:137`) sends one standalone READ per call and one CLOSE once
bytes_received ≥ file_size. For a ≤MaxReadSize file the consumer loop runs once:

- CREATE — 1 RTT (inside `client.download`, before the stream is returned)
- READ — 1 RTT (first `next_chunk`)
- CLOSE — 1 RTT (auto-triggered after the last READ inside the same `next_chunk`)

**Total: 3 sequential RTTs.** The consumer side (`SmbReadStream`) adds no extra SMB ops — just an
mpsc channel with capacity 4.

Write side (`write_from_stream`) uses `FileWriter`: CREATE → N×WRITE → FLUSH → CLOSE. For a 10 KB
file that fits in one WRITE: **4 RTTs** (CREATE + WRITE + FLUSH + CLOSE).

End-to-end read-and-write pipe through `volume_strategy.rs::stream_pipe_file` for a 10 KB
Local→SMB or SMB→Local copy: source 3 RTTs + dest 4 RTTs on independent connections. With 137 ms
RTT: 3×137 + 4×137 = 959 ms/file serial. At 10-way concurrency that's ~96 ms/file effective, but
the measurement shows ~260 ms/file. The RTT count alone doesn't fully explain the number — see
"needs measurement" section below.

### Pre-P4.1 — also streaming, via `export_single_file_with_progress`

Before commit `eb99c37c`, SMB→Local went through `export_single_file_with_progress`, which **also**
called `open_smb_download_stream` (commit `a8270909`, Apr 17). Before `a8270909` it called
`client.read_file_with_progress`, which under the hood is `read_file_pipelined_with_progress`
(`smb2/src/client/mod.rs:887`): `open_file` (CREATE, 1 RTT) + pipelined READs + `close_handle`
(CLOSE, 1 RTT). For a ≤MaxReadSize file the pipelined loop runs one iteration — same 3 RTTs.

So **pre-P4.1 was also 3 RTTs per small file** for the SMB→Local path. P4.1 did not add RTTs; it
swapped one 3-RTT pipelined reader for another 3-RTT streaming reader.

### Actual measurement — not done

Not run. The source is unambiguous on the RTT count (one CREATE, one READ, one CLOSE; no compound).
Running a test would only confirm what `execute` vs `execute_compound` call sites already tell us.

## If we want to make small-file copies faster, the fix isn't what P4.1 took away

P4.1 didn't cost us a compound path we used to have — there was none. To *gain* one, we'd add it.
Options, ranked:

### Option 1 — compound fast-path inside `open_read_stream` (recommended)

When `open_smb_download_stream` is about to issue `client.download`, peek at the file size first
(or skip the peek — see below) and, for `file_size ≤ max_read_size`, call
`tree.read_file_compound` instead. Buffer the returned `Vec<u8>` as the stream's first and only
chunk. Caller sees the normal `VolumeReadStream` API; wire traffic is 1 RTT.

Two sub-problems:

1. **We don't know file size before CREATE.** `open_file` returns size alongside file_id. To
   decide compound vs pipelined before CREATE, we'd need a prior stat (another RTT — defeats the
   point) or a size hint from the caller (the copy path has one — `copy_single_path` scan phase
   already computed per-file sizes). Alternatively: always try compound first; if the READ in the
   compound returns short, close the (non-existent — compound already closed it) handle and open
   for a pipelined read. Compound inherently handles "file fits in one READ"; if it doesn't, the
   data is truncated, not errored, so we'd need a length check.
2. **The writer side is separate** — we pay 4 RTTs on write regardless. `smb2::Tree` has
   `write_file_compound` (CREATE+WRITE+FLUSH+CLOSE in one compound = 1 RTT) — see
   `write_file_pipelined`'s fallback at `tree.rs:1757`. For SMB-dest small writes we can do the
   same fast-path in `write_from_stream`.

LOC: ~40 in `smb.rs` for each direction (read + write fast-paths); plus maybe 10 in smb2 to expose
a stream-shaped small-file wrapper if we want to keep `SmbVolume` decoupled from the compound API.
Risk: low (compound path is already unit-tested in smb2). Buys us: **3 RTTs → 1 RTT read, 4 RTTs
→ 1 RTT write**. For a 137 ms link, per-file wall clock drops from ~959 ms serial to ~274 ms
serial. At 10-way concurrency: from ~96 to ~27 ms/file effective. A roughly 3–4× improvement on
the small-file copy scenario.

### Option 2 — add a "compound stream" to smb2

Generalize: a stream-shaped API backed by a single compound frame when the file fits, and falling
back to pipelined otherwise. Cleaner, but scope creep — `smb2` would grow a new public type.
LOC: ~100+. Risk: medium. Buys the same as Option 1 but with a nicer cmdr-side API. Not worth it
just for this; revisit if more consumers show up.

### Option 3 — widen chunk size

Irrelevant. For a 10 KB file the pipelined reader already issues one READ. Chunk size affects
large-file throughput, not small-file RTT count.

## Small-file threshold

If we go with Option 1: the natural ceiling is `max_read_size` (QNAP negotiates ~1 MB; SMB2 spec
max is 8 MB). Above that, the compound can't return all the bytes in one READ, so we lose the
benefit anyway.

Recommended cutoff: use compound whenever the caller-provided size hint is `≤ max_read_size`.
No reason to pick a smaller number like 64 KB — the per-file saving grows with file size (1 RTT
vs 3 RTTs stays constant, wall-clock saving grows with the file's READ count). Only reason to
be conservative would be memory (compound returns a `Vec<u8>`; stream yields chunks). At
1 MB that's trivial.

## Is this the whole story?

Medium-high confidence that RTTs are the primary cost, but not 100%. Back-of-envelope: 3 RTTs
read + 4 RTTs write = 7×137 ms = 959 ms/file serial. With 10-way concurrency that's ~96 ms/file
amortized. Measured: ~260 ms/file. Factor of ~2.7× gap that RTTs alone don't explain. Possible
other sources:

- **Concurrency is not actually 10.** `max_concurrent_ops` on SMB is 10 (hardcoded, per
  `volume/CLAUDE.md`), but `copy_between_volumes` takes `min(src, dst, 32)`. If the source is
  `LocalPosixVolume` returning 4–16 and the dest is SMB returning 10, the batch picks the
  minimum. Also, the SMB session mutex serializes per-connection, and the smb2 connection pool
  (see commit `008e9627`) may or may not have multi-connection scaling on the read path.
- **Session mutex contention.** Both read (`open_smb_download_stream` holds `OwnedMutexGuard`
  for the whole download) and write (`write_from_stream` holds `acquire_smb` guard end-to-end)
  take the session lock. If both source and dest are the same SMB volume, concurrency drops to
  1 on that volume. For 100×10 KB Local→SMB or SMB→Local it's two different volumes, so less of
  a concern.
- **Per-chunk tokio overhead and mpsc hop.** The channel adds a task-switch per chunk.
  Negligible per-hop (µs), but at 260 ms/file it'd take thousands of hops to matter — not
  plausible.
- **Progress event throttling.** 200 ms throttle is per-operation, not per-file, so it doesn't
  stall per-file work.
- **Volume scan phase.** `copy_single_path` calls `source_volume.is_directory(source_path)` per
  file (`volume_strategy.rs:42`). For SMB that's a `get_metadata` → CREATE+QueryInfo+CLOSE — on
  the current code, likely another 1–3 RTTs per file that isn't in the "transfer" budget. Worth
  double-checking the pre-flight cost separately.

What I'd measure to be sure: run one small SMB copy with `RUST_LOG=smb2::client::connection=debug`
and count `execute` / `execute_compound` calls per file. Separately instrument the
`volume_strategy::copy_single_path` entry/exit and source-volume stat calls. If the real wire count
per file is 7 (3 read + 4 write), Option 1 is the right fix. If it's higher (say 10+, including
a pre-flight stat or is_directory probe), also look at removing those probes from the hot path.

## Summary

- Premise was wrong: there was never a compound SMB fast-path in cmdr. Pre- and post-P4.1 both
  use a 3-RTT streaming read (+ 4-RTT streaming write).
- P4.1 didn't regress RTT count. It swapped one 3-RTT reader for another.
- The real win available is **adding** a compound fast-path: `read_file_compound` for small
  reads, `write_file_compound` for small writes, both gated on `size ≤ max_read_size`. Estimated
  3–4× wall-clock improvement on high-RTT small-file copy workloads.
- Before implementing: measure actual per-file RTT count (expected ~7, may be higher if
  `is_directory` / stat probes also hit the wire) to confirm the RTT diagnosis.
