# Navigate during MTP transfers, take 2: bounded-window reads

Created 2026-06-25. Status: planned. **Supersedes the auto-yield mechanism in**
[`2026-06-22-navigate-during-transfers-plan.md`](2026-06-22-navigate-during-transfers-plan.md) (M1 shipped in
`06d1874d` but is broken on real hardware — see below). The *goal* is unchanged: the phone stays navigable while an
MTP→local copy runs. Only the *mechanism* changes.

## Why the shipped version is broken

The shipped auto-yield, on a pending foreground listing, calls `CheckpointStream::cancel_and_release()` →
`FileDownload::cancel()` to free the PTP session, then reopens at the offset. The problem: the copy reads the file as
**one unbounded `GetObject`/`download_stream_from_offset` transaction that holds the session for the entire file**.
Aborting that mid-flight isn't cheap — the USB CLASS_CANCEL must drain the device's in-flight data backlog until the
pipe goes idle. For a multi-GB file mid-transfer that drain takes **~35 s**. So:

- Real-device repro (Pixel 9 Pro XL, copying a 2.4 GB video): navigate mid-copy → the listing blocks waiting for the
  session, hits the 30 s MTP timeout, the frontend treats it as fatal and **kicks the user back to Macintosh HD**. The
  copy froze at ~808 MB and only reopened the instant the listing timed out.

The root flaw: **"yield" was implemented as "abort a giant in-flight read," which is inherently slow.** The fix removes
the giant in-flight read entirely.

## Spike: bounded reads work (validated on the real device)

Run on the connected Pixel 9 Pro XL (throwaway `download_partial_64`-loop example, since removed):

- **One 8 MiB `download_partial_64(offset, 8 MiB)`: ~80 ms.**
- A 641-entry folder listing, with nothing else in flight (baseline): ~8.5 s (the device does one `GetObjectInfo` USB
  roundtrip per entry — pre-existing, unrelated to us, and under the 30 s timeout).
- **A listing issued BETWEEN bounded 8 MiB windows: ~8.5 s — identical to baseline.** No blocking either way; the read
  and the listing interleaved cleanly across 8 windows.

So a bounded `GetPartialObject64` releases the single PTP session every ~80 ms, and a foreground listing slips in
between windows at its natural cost. The hypothesis holds.

Implication for the design: **the "yield" cost drops from ~35 s (abort a 2 GB read) to ≤ ~one window (~80 ms, finish
the current window and don't start the next).** That's the whole fix.

## The mechanism

Read a long MTP file as a **sequence of small bounded `GetPartialObject64(offset, WINDOW)` transactions** (WINDOW ~8
MiB), advancing the offset, instead of one unbounded `GetObject` held open for the whole file. Between windows **nothing
is in flight and the PTP session is free**, so:

1. **Foreground yield is cheap and needs no cancel/drain.** To yield, the copy simply **doesn't start the next window**
   while foreground work is pending (then resumes from the kept offset). No `cancel_and_release` of a giant transfer.
2. **The device lock is the natural arbiter.** Each window acquires Cmdr's per-device lock (the one the
   foreground-priority scheduler arbitrates) for only ~80 ms. Between windows the copy holds nothing, so a foreground
   listing's `foreground_guard` + lock contention lets it through. The explicit `foreground_pending` check + debounce
   still gate the *next* window (lock fairness alone could let the copy immediately re-grab and starve foreground), but
   the heavy abort is gone.
3. **Pause becomes instant too**, via the same "don't start the next window" path — no session drain on pause either.
4. **Cancellation becomes cheap**: between windows there's nothing to drain; mid-window is a ≤~80 ms read, so even an
   abort mid-window is fast (vs aborting a multi-GB GetObject).

### KEY DECISION: this needs NO mtp-rs change (recommended)

The spike used the **already-published** `Storage::download_partial_64` (in `mtp-rs` 0.21.0). Cmdr's MTP read path can
loop it directly. Reasons this is the right end state, not just the cheap one:

- **Correct separation of concerns.** mtp-rs already provides the bounded-read primitive (`download_partial_64`, a
  single `GetPartialObject64` transaction that releases the session on return). The *windowing + yield policy* (window
  size, when to pause/yield, debounce, the foreground gate) is a Cmdr concern that lives next to the scheduler — it
  cannot live in mtp-rs, which knows nothing about Cmdr's foreground gate. A windowed-stream type *inside* mtp-rs would
  still have to hand control back to Cmdr between windows for the gate check.
- **The one thing such a type WOULD save, Cmdr does itself.** A windowed type retaining the `Storage` handle + object
  handle across windows would avoid re-resolving them per window — but `MtpReadStream` already holds equivalent state
  today, so Cmdr just caches `Storage` + handle + `total_size` on open (see M1) and pays only `acquire_device_lock` +
  `download_partial_64` per window. So the honest framing is "Cmdr caches the handle/Storage itself; the windowing+gate
  stays in Cmdr next to the scheduler," not "a wrapper is impossible" — it's that the wrapper adds a publish dance for
  encapsulation Cmdr doesn't need.
- **No publish dance.** No new mtp-rs version, no temporary `[patch.crates-io]`, no revert step. Faster and lower-risk —
  and it keeps `pnpm check`'s cargo-audit/Renovate surface unchanged.
- **Bounded memory, fine throughput.** `download_partial_64` returns the window as a `Vec<u8>` (8 MiB buffered per
  window — bounded, trivial). Spike throughput was ~100 MB/s (healthy USB2-MTP). Progress updates every ~80 ms — smooth.

**Alternative (documented, NOT chosen unless the reviewer/real-device work surfaces a reason):** add a
`Storage::download_windowed(handle, window)` streaming type to mtp-rs that loops bounded `GetPartialObject64` internally
and yields one window per `next_window()`. Only worth it if we discover a second consumer or want the offset/EOF/u32-cap
bookkeeping unit-tested in the protocol crate. It does NOT remove Cmdr's need to interpose the gate between windows, so
it adds a publish dance for marginal encapsulation. If we ever do this, the existing `download_stream_from_offset` doc
already covers the u32 `max_bytes` cap to mirror.

The plan below assumes the recommended no-mtp-rs-change path. If the reviewer disagrees, M1 becomes "mtp-rs windowed
type" and M2 consumes it; the Cmdr-side yield rework (the bulk of the value) is identical either way.

## Design principles in play

- **Protect data (4):** byte-exactness across windows is the same invariant as the release-on-pause work —
  `bytes_yielded` == destination temp length, the next window reads `[offset, …)`, safe-replace temp+rename untouched.
  The window loop must never drop, double-read, or reorder bytes.
- **Rock solid (3):** everything stays cancelable; a yield/pause must not be able to wedge the copy or the device.
- **Respect resources (5):** bounded 8 MiB working set regardless of file size; the single USB pipe is shared fairly.
- **Delightful (1):** navigating the phone mid-copy "just works" within ~a window; the copy resumes seamlessly.

## Milestones

### M1 — Cmdr MTP read path: bounded-window reads (replaces the held-open stream)

Rework the MTP read so a download is a loop of bounded `download_partial_64(offset, WINDOW)` transactions, each
acquiring + releasing the per-device lock, instead of one held-open `FileDownload`.

- **Connection layer** (`mtp/connection/file_ops.rs`): two methods, NOT one-per-window-resolves-everything (resolving
  the handle + `Storage` per window costs an extra `GetStorageInfo`/resolve USB roundtrip per 8 MiB — hundreds over a
  multi-GB file; the spike never paid this because it held one `Storage`):
  - an **open** that resolves the object handle + obtains the `Storage` + reads `total_size` ONCE (under the device
    lock), returning them for the stream to cache;
  - a **per-window read** that takes only `acquire_device_lock` + `storage.download_partial_64(handle, offset, window)`
    and returns the `Vec<u8>`.
  This replaces `open_download_stream_at_offset`'s role for the read path.
- **The copy's window reads take NO `foreground_guard`.** A running transfer is a *background* user of the device gate
  (it yields TO foreground), so it must not raise `foreground_pending` — if a window read held a `foreground_guard`, the
  copy's own checkpoint would see pending foreground and **yield to itself forever** (livelock, frozen copy). Only the
  per-window `acquire_device_lock` is taken (held ~80 ms/window); the **open** method likewise takes no
  `foreground_guard` (consistency — a transfer is wholly a background gate user). Consequence to name in docs: the index
  scan now interleaves with a copy at window granularity (both background, contending fairly on the lock) instead of
  effectively waiting out the whole transfer. (In the held-open model the scan was starved not by a gate guard — the
  download's setup-time guard dropped on return — but because the single streaming `GetObject` held mtp-rs's
  `operation_lock` continuously for the whole file; bounded windows release that lock between windows.)
- **`MtpReadStream`** (`file_system/volume/backends/mtp.rs`): no longer wraps a `FileDownload`. It caches the resolved
  `Storage` + object handle + `total_size`, plus `offset` and `window`. `next_chunk()` issues the next bounded window via
  the per-window read, advances `offset` by the bytes ACTUALLY returned, returns the window's bytes (or `None` at EOF).
  `cancel_and_release()` becomes a near-noop (nothing held between windows). `bytes_read()` = `offset`.
- **Rework the SHARED stream, so drag-out benefits too — don't fork a copy-only path.** `MtpVolume::open_read_stream`
  (used by native drag-out in `native_drag/fulfillment.rs`) and `open_read_stream_at_offset` (the copy path) both build
  `MtpReadStream`. Rework that one shared type; both consumers get bounded windows for free. Then
  `open_download_stream_at_offset` has zero callers (confirmed: its only caller is `MtpVolume::open_read_stream_at_offset`)
  — retire it and the `FileDownload`-wrapping `MtpReadStream` internals.
- **Also fix the single-file download command (same chokepoint).** `download_mtp_file` (FE-wired: `downloadMtpFile` →
  `commands/mtp.rs` → `file_ops.rs::download_file_with_progress`) still uses held-open `storage.download_stream` and
  holds the device lock + a `foreground_guard` for the WHOLE file — so "navigate during a single-file download" hits the
  identical wedge. It shares the connection layer, so route it through the same bounded-window read (drop its
  whole-file `foreground_guard`). If a quick check shows the current FE never actually invokes it (the real MTP→local
  copy is the streaming-copy path), note it out of scope instead — but don't silently leave a second held-open reader.
- **Window size**: a named const (`MTP_READ_WINDOW`, default 8 MiB — the spike's value), with a one-line rationale
  (throughput vs. yield-latency knob; M3 tunes it). Make it overridable for tests.
- **Data safety / liveness**: the offset accounting is the load-bearing invariant — each window reads exactly
  `[offset, offset+len)` and advances by the bytes actually returned (a short read mid-file is legal — advance by what
  came back; a short read at EOF ends the stream). **A 0-byte read while `offset < total_size` is an ERROR, not loop
  continuation** (else a device hiccup spins a frozen-progress livelock) — surface it as a transient `VolumeError`
  (optionally after one retry). Empty file / `offset == total_size` ⇒ first `next_chunk` returns `None` immediately. Map
  mtp-rs errors as today.
- **Drop-safety (why mid-window abort is safe — record in DETAILS):** a window-read future dropped mid-flight (task
  abort, disconnect) does NOT permanently desync the session — mtp-rs's `TransactionScope` flags the pipe and the next
  op drains under the operation lock before running (one ~300 ms drain, self-healing). This is the property that makes
  the buffered-window model safe to abort at any point; a future reader will worry about exactly this.

- **Tests (TDD, real red→green — this is the copy read loop):**
  - A virtual-MTP read of a multi-window file returns the full byte sequence identical to the file (assemble windows →
    equals source). Drive via the existing virtual-device harness; ensure the virtual transport serves repeated
    `GetPartialObject64` at arbitrary offsets (the buffered `download_partial` opcode is already handled — verify the
    looped/offset path; extend the virtual device if a repeated-partial gap shows up). Red first by asserting the read
    issues bounded windows (offset advances per `next_chunk`), which fails against the held-open stream.
  - EOF: a file whose size isn't a window multiple ends cleanly with a short final window, exact total bytes.
  - Empty file / `offset == total_size`: first `next_chunk` returns `None` immediately (no window read issued).
  - **Zero-length read before EOF → error, not a spin** (drive the virtual device to return an empty window mid-file —
    `read_partial` uses `file.read()`, so a short/empty read is serveable; assert a `VolumeError`, not an infinite loop).
  - Short mid-file read advances by the returned length and still assembles byte-exact.
  - Cancel mid-read returns promptly and keeps-partials correctly (no held transaction to drain).
- **Docs**: `mtp/connection/CLAUDE.md` + `DETAILS.md` (the read is now bounded windows; the session is free between
  windows; per-window lock acquisition is what lets foreground in). `file_system/volume/backends/` notes on
  `MtpReadStream`.
- **Checks**: `pnpm check rust` (note: cargo-audit is pre-existing-red on quinn-proto/memmap2 — verify via
  clippy + rust-tests + integration + bindings + rustfmt green).

### M2 — Rework the auto-yield + pause to "don't start the next window"

With M1's bounded reads, simplify `CheckpointStream` (`write_operations/transfer/volume_strategy.rs`): the checkpoint no
longer aborts a giant transfer. Between windows it: checks cancel (unchanged), then if **paused** OR **foreground
pending** on the source device, parks (foreground: debounce + min-progress floor as today) before letting the next
`next_chunk` window proceed. Because the read no longer holds the session, **`cancel_and_release` is no longer the yield
mechanism** — delete or neuter that path. The reopen-at-offset (`open_read_stream_at_offset`) collapses into "the next
window just reads from the current offset," so the explicit release/reopen dance can largely go away (the stream never
released anything to reopen).

- **Intention**: the M1 release-on-pause + auto-yield code was built around "release the held session, reopen later."
  Bounded windows make the held session — and thus the release/reopen — obsolete. Keep the *policy* (pause parks;
  foreground parks with debounce + min-progress floor; op stays **Running** during a foreground yield, never flips to
  `Paused`; cancel wins) and drop the *mechanism* (cancel_and_release/reopen). Net: less code, and the ~35 s stall is
  structurally impossible.
- **CRITICAL re-gating (don't take the feature down with the reopen code).** Today BOTH the pause arm and
  `auto_yield_to_foreground` are switched on by `self.reopen.is_some()` (the MTP-detection proxy: `volume_strategy.rs`
  ~194 and the `None => return` at ~232). If you remove `CheckpointReopen`/`reopen`/`ensure_open` literally, you delete
  the auto-yield's own enable-switch and silently disable the feature being built. So when removing the reopen
  machinery: **(a) the release-on-pause arm is removed ENTIRELY** — once MTP flips to park-in-place (next bullet), every
  backend just parks via the unconditional `wait_while_paused_async` (`volume_strategy.rs` ~212), and the release block
  (~194–210) plus `CheckpointReopen`/`ensure_open` are deleted; **(b) the foreground auto-yield's enable-switch moves to
  `source_volume.supports_foreground_yield()`** (stays `true` for MTP), NOT `reopen.is_some()`. Don't conflate them:
  pause stops releasing for everyone; foreground auto-yield stays on for MTP. The `auto_yield_*` tests catch a
  regression, but call this out so it's a deliberate move, not a deletion.
- **Pause becomes park-in-place for MTP too.** With nothing scarce held between windows, `MtpVolume` should flip
  `pause_releases_read_stream()` to `false` — pause just stops starting the next window, no session release. That makes
  pause instant and removes the last reopen caller. **Update the now-stale doc comments** on
  `MtpVolume::pause_releases_read_stream()` (`mtp.rs` ~850, "in-flight download holds the PTP session") and
  `supports_foreground_yield()` (`mtp.rs` ~860, "holds the PTP session across the whole download") — these predicates are
  load-bearing and their rationale changes. `open_read_stream_at_offset` with a non-zero offset then has no caller (the
  copy reads from offset 0 forward in windows); keep the trait method (it's reached via `open_read_stream`'s offset 0)
  but note the non-zero path is now unused so it isn't later "cleaned up" as dead by mistake.
- **Keep**: byte-exactness (offset == temp length), cancellation-wins, debounce (`FOREGROUND_YIELD_DEBOUNCE`),
  min-progress floor (`MIN_PROGRESS_FLOOR_BYTES`), op-stays-Running, non-MTP no-op.
- **Watch**: the foreground `wait_until_foreground_idle` race-against-cancel (the `select!` that prevents a
  cancel-while-yielding hang) stays relevant — keep it. Lock fairness alone may not stop the copy from re-grabbing the
  lock ahead of a waiting listing, so the explicit `foreground_pending` gate + debounce before the next window is still
  required (don't rely on fairness).

- **Tests (TDD red→green):**
  - Paused multi-window copy stops advancing between windows and resumes byte-exact (reuse/adapt the existing
    `release_on_pause_*` tests to the no-release model — they should now assert "no next window while paused," not
    "stream released").
  - Foreground-pending: the copy parks before the next window and resumes; debounce collapses a burst; min-progress
    floor prevents starvation; op stays Running. (Adapt the existing `auto_yield_*` tests; the fake probe drives
    `foreground_pending`.)
  - Cancel-while-paused and cancel-while-yielding keep-partials correctly and don't hang.
  - **No self-yield livelock**: a copy with NO foreground listing pending runs to completion and never parks itself
    (guards against a window read accidentally raising `foreground_pending`). Pin it.
  - Non-MTP source: unchanged park-in-place behavior, never windows/yields differently.
- **Docs**: `write_operations/transfer/CLAUDE.md` + `DETAILS.md` — rewrite the "Release-on-pause" / "Foreground
  auto-yield" sections to the bounded-window model (no session release/reopen; "don't start the next window"). Update
  `mtp/connection` docs' "a running MTP transfer … releases + reopens the PTP session" line to "reads in bounded
  windows, yielding the session between windows." Keep both CLAUDE.md files ≤ 600 words (move depth to DETAILS).
- **Checks**: `pnpm check rust`.

### M3 — Real-device verification + tuning (David)

Virtual MTP can't reproduce USB contention, so this is on the Pixel. Start a large MTP→local copy, navigate the device
pane mid-copy, and confirm: the listing returns within ~its natural cost (no 30 s timeout, no kick to Macintosh HD), the
copy resumes and finishes byte-correct, and continuous browsing doesn't starve the copy (min-progress floor). Tune
`MTP_READ_WINDOW` (throughput vs. yield latency), `FOREGROUND_YIELD_DEBOUNCE`, and `MIN_PROGRESS_FLOOR_BYTES` from
observed feel; record the chosen values + rationale.

- **Verification**: manual on real hardware, plus log assertions (window count, no Timeout errors during nav, copy
  completes). Capture before/after responsiveness.
- **Docs**: record tuned constants. Tick this spec and mark the 2026-06-22 spec's mechanism superseded.

## Parallelism

Sequential. M2 depends on M1's read shape; M3 needs both. Within M1 the connection-layer method and its unit test can be
written together.

## Risks / open questions

- **Per-window throughput overhead**: each window is a fresh `GetPartialObject64` (command + data + response). Spike
  showed ~100 MB/s at 8 MiB windows, but full-file throughput vs. the old held-open stream is unmeasured at scale — M3
  confirms; the window size is the knob if it regresses.
- **`download_partial_64` at large offsets across a full multi-GB file**: spike only exercised 0–64 MiB. The per-call
  `max_bytes` is u32 (≤4 GiB) — irrelevant at 8 MiB windows. M3 must copy a full multi-GB file end to end and verify the
  bytes (checksum) — the load-bearing real-device check.
- **Listing latency is still ~8.5 s for huge folders** (pre-existing per-entry `GetObjectInfo`). Out of scope here, but
  note it: nav during copy will work yet feel slow on big folders. Candidate future optimization (batch metadata).
- **The spike validated the PRIMITIVE, not Cmdr's gate-parking.** It was a raw single-threaded `download_partial_64`
  loop with no foreground gate; it proves bounded reads free the session between windows. Cmdr's real behavior (the copy
  parks entirely for the ~8.5 s a foreground listing runs, then resumes) plus the debounce/floor/`select!` interaction
  under genuine concurrent USB contention is only exercised at M3. The outcome should match, but don't read the spike as
  validating the gate machinery.
- **Progress + pause latency coarsen to one window.** `on_progress` now fires per ~8 MiB (was per small USB chunk), and
  pause/yield takes effect at a window boundary (~80 ms healthy, more on slow USB2). Acceptable — note in DETAILS so
  it's not later mistaken for a regression.
- **`MIN_PROGRESS_FLOOR_BYTES` (4 MiB) is now smaller than the window (8 MiB)** ⇒ the floor is effectively "one window."
  Re-tune + re-doc both constants together in M3 (the old "a handful of chunks" rationale no longer fits — the chunk IS
  the window).
- **Retiring `open_download_stream_at_offset` / `FileDownload` use**: confirm no other consumer (search, drag-out) needs
  a held-open stream before removing it; if one does, keep it for that path and only swap the copy path.
- **mtp-rs unchanged** means the `download_stream_from_offset` API added in 0.21.0 becomes unused by the copy path. Leave
  it (it's a fine published primitive; the resume-a-paused-copy example still uses it) — don't churn mtp-rs to remove it.

## Pointers (read, don't transcribe)

- `mtp/connection/file_ops.rs` (`open_download_stream_at_offset`, `acquire_device_lock`), `mtp/connection/mod.rs`
  (`foreground_pending`, `background_yield_point`, the device lock), `mtp/connection/{CLAUDE,DETAILS}.md` (the
  scheduler).
- `file_system/volume/backends/mtp.rs` (`MtpReadStream`), `file_system/volume/mod.rs` (`VolumeReadStream`,
  `open_read_stream_at_offset`, `pause_releases_read_stream`).
- `write_operations/transfer/volume_strategy.rs` (`CheckpointStream`, `auto_yield_to_foreground`, the debounce/floor
  constants) + `transfer/{CLAUDE,DETAILS}.md`.
- mtp-rs: `crates/mtp-rs/src/mtp/storage.rs` (`download_partial_64`, `download_stream_from_offset`).
- Superseded mechanism: [`2026-06-22-navigate-during-transfers-plan.md`](2026-06-22-navigate-during-transfers-plan.md).
