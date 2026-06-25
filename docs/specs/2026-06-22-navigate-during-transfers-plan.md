# Navigation works during MTP transfers (auto-yield)

Created 2026-06-22. Status: planned.

Make the app fully responsive while an MTP→local transfer is running: navigating folders, listing, and metadata reads on
the phone work _during_ an active copy — not just while it's manually paused. The transfer briefly, automatically yields
the device to the foreground request, then resumes from where it stopped.

## Why

Today a long MTP copy makes the phone feel locked: the device has one PTP session, and an in-flight `GetObject`/
`GetPartialObject64` download owns it, so a foreground listing can't get through until the current file finishes. We
already shipped two of the three pieces this needs:

- **Manual pause releases the session** (`CheckpointStream`, `pause_releases_read_stream()`): on pause it
  `cancel_and_release`s the download (freeing the session) and reopens at the byte offset on resume. So the user _can_
  browse — but only by manually pausing first.
- **A foreground/background priority arbiter** already exists per device (`mtp/connection/scheduler.rs`,
  `DevicePriorityGate`): foreground ops (nav/list/delete/rename/move/upload/visible-pane resolve) take a
  `ForegroundGuard` that raises `foreground_pending`; the background index scan calls `background_yield_point()` between
  units and parks while anything is foreground-pending.

This feature is the **composition** of those two: make a running transfer behave like the index scan — a yielding
background user of the device — by wiring the existing release/reopen primitive to the existing priority gate. No new
protocol work, no new mtp-rs API.

## The core reframe

A transfer is currently _neither_ foreground nor a yielding background user during streaming. Its
`open_download_stream_at_offset` takes a `ForegroundGuard` only for **stream setup** (`mtp/connection/file_ops.rs:443`),
which drops before the chunk loop. So while streaming, the transfer holds no guard and consults the gate nowhere — it
just competes for the per-op device lock chunk by chunk, and a foreground listing can't preempt the open download data
phase.

The fix: **the transfer's per-chunk checkpoint becomes a `background_yield_point`** — exactly what the index scan does
at its unit boundary, but because a transfer holds an _open download transaction_ (not cheap per-unit ops), yielding
means **release the session** (`cancel_and_release`), wait for foreground to drain, then **reopen at the offset**. The
release/reopen machinery already exists in `CheckpointStream`; this adds a second trigger for it (foreground-pending)
alongside the existing one (user pause).

## Design

### Where it lives

`CheckpointStream::next_chunk` in `write_operations/transfer/volume_strategy.rs` already runs once per chunk and today
does: (1) park while user-paused (release-on-pause for MTP), (2) `yield_now()`. Add a third arm: **(3) if the source is
an MTP device with foreground work pending, release the session, `background_yield_point().await`, and reopen at
`bytes_yielded`** — the same close/reopen path the user-pause arm uses, just gated on `foreground_pending` instead of
the pause flag.

So the checkpoint's logic becomes, in order:

- Cancelled? → let the backend's `on_progress` `is_cancelled` own cancel+cleanup (unchanged).
- User-paused? → release-and-park until resume/cancel (existing behavior, unchanged).
- Foreground pending on this device (and we've debounced, see below)? → `cancel_and_release`,
  `await background_yield_point(device_id)`, reopen at `bytes_yielded`. The op stays **Running** — this is a transient
  device yield, not a user pause.
- Else → `yield_now()` and continue (the runtime-fairness yield we already have).

### Reaching the gate from the stream

`CheckpointStream` needs the source device's `DevicePriorityGate` (or a cheap `foreground_pending(device_id) -> bool`
probe + `background_yield_point(device_id)` future). Options, pick at implementation time:

- Add an optional `fn device_priority_probe(&self) -> Option<ForegroundProbe>` to the `Volume` trait (default `None`;
  `MtpVolume` returns a handle wrapping its `MtpConnectionManager` + `device_id`). Keeps `CheckpointStream` volume-
  agnostic and testable with a fake probe. **Preferred.**
- Or thread the probe in alongside the existing `CheckpointReopen` struct (which already carries the source volume +
  path for reopen), since release-on-pause MTP sources are exactly the ones that have a gate.

Either way the probe is two operations: "is foreground pending?" (sync, cheap — an atomic load) and "await until
foreground drains" (`background_yield_point`). Both already exist on `MtpConnectionManager` (`mod.rs:577`); this exposes
them through the volume/stream boundary.

### Debounce / hysteresis (load-bearing)

The index scan yields for free (it just doesn't issue the next cheap op). A transfer's yield costs a real session
teardown + `GetPartialObject64` re-setup on resume, so naive yielding thrashes under rapid navigation (each keystroke in
a folder tree = a release+reopen). Rules:

- **Suspend promptly** on the first foreground demand observed at a chunk boundary (responsiveness is the whole point).
- **Resume only after foreground has been idle for a debounce window** (start ~300–500 ms; tune on device). The
  `background_yield_point` already waits until `foreground_pending == 0`; add a short "stay parked until quiet for N ms"
  so a burst of listings is served as one suspension, not N.
- Consider a **minimum-progress floor**: after a resume, transfer at least one chunk (or ~M ms) before honoring the next
  yield, so a continuous stream of foreground ops can't starve the transfer to zero throughput. (Foreground priority is
  right, but the transfer must still make _some_ progress — the scan has the same tension; mirror its policy.)

### Status / UX

- The op stays **`Running`** in the manager + queue window during an auto-yield (it still holds its lane; it's a device-
  level yield, not user intent). Do NOT flip to `Paused` — that would misreport user-initiated pause and is what the
  queue window keys its Pause/Resume button on.
- Optional, low priority: a subtle transient hint (e.g. the progress row showing "yielding to the phone…" or just the
  ETA naturally stretching). Default to no new UI; the win is that nav simply works. Decide during build; don't gold-
  plate.

### What stays unchanged (and must)

- **Byte exactness**: identical to release-on-pause — `bytes_yielded` == destination temp length, reopen appends
  `[bytes_yielded, size)`, safe-replace temp+rename untouched. The auto-yield uses the _same_ release/reopen code, so it
  inherits the same guarantees (and tests).
- **Cancellation** wins over an auto-yield exactly as it wins over pause (cancel observed → reopen, let the next chunk
  flow to `on_progress`, backend cleans up).
- **Non-MTP sources** (local, SMB, in-memory) have no `device_priority_probe` → the new arm is a no-op; they behave
  exactly as today.
- **Lane budget 1 on the MTP device** guarantees the only foreground contender is a listing/nav/metadata op, never a
  second transfer — so there's no transfer-vs-transfer yield to reason about.

## Risks / open questions

- **Reopen thrash** — mitigated by the debounce + min-progress floor; the real tuning happens on the phone. The biggest
  unknown is the right debounce window; make it a named constant, not magic.
- **Foreground starving the transfer to 0** — the min-progress floor addresses it; confirm a continuous nav session
  still lets a copy finish (slowly) rather than stalling forever.
- **Session re-acquire latency** — each reopen pays a `GetPartialObject64` setup. Acceptable for responsiveness, but if
  it's visibly janky, a longer debounce trades a touch of nav latency for fewer reopens.
- **Interaction with the device scheduler's existing scan-yield** — a transfer, the index scan, AND foreground ops can
  all want the device. Confirm the transfer's new yield composes with the scan's existing yield without a
  priority-inversion (foreground > transfer ≈ scan? or foreground > transfer > scan?). Likely fine since both yield to
  the same `foreground_pending` signal, but verify the scan doesn't starve a transfer or vice-versa.
- **Two-layer fairness**: the runtime-level `yield_now()` (tokio worker fairness) and this PTP-session-level yield are
  different things; keep both. This spec is only the session layer.

## Milestones

### M1 — Probe surface + checkpoint auto-yield (backend)

Expose the device foreground probe through the volume/stream boundary; add the foreground-pending arm to
`CheckpointStream` (release → `background_yield_point` → reopen) reusing the release-on-pause path. Debounce + min-
progress floor as named constants. Op stays `Running`.

- **Tests (real red→green, this is the data-safety copy loop):**
  - With a fake priority probe reporting "foreground pending", a multi-chunk MTP-style copy releases the source, waits,
    reopens at the kept offset, and the assembled bytes equal a non-yielded copy (byte exactness across auto-yield).
  - Foreground-pending then idle → exactly one release/reopen for a burst (debounce), not one per chunk.
  - Min-progress floor: continuous foreground-pending still advances the transfer by at least the floor each cycle (no
    zero-throughput starvation).
  - Cancel during an auto-yield → keeps-partials correctly, session usable after.
  - Non-MTP source (no probe) → never releases (regression guard).
- **Docs:** `transfer/CLAUDE.md` + `DETAILS.md` (the auto-yield arm, debounce, Running-not-Paused); `mtp/connection`
  docs (the transfer is now a yielding background user of the gate, like the scan).
- **Checks:** `pnpm check rust`.

### M2 — Real-device verification + tuning

Run on the phone: start a large copy, navigate folders / switch directories on the device pane mid-copy, confirm it
responds within ~the debounce window and the copy still completes byte-correct. Tune the debounce + floor constants from
observed feel. Confirm no thrash (count release/reopen cycles in logs during a nav burst) and no starvation (copy
finishes during continuous browsing).

- **Verification:** manual on real MTP hardware (virtual MTP can't reproduce the contention — no USB latency, single-
  chunk files), plus log assertions on release/reopen counts. Capture before/after responsiveness.
- **Docs:** record the tuned constants + rationale.

## Pointers (read, don't transcribe)

- `write_operations/transfer/volume_strategy.rs` — `CheckpointStream` (the checkpoint, release/reopen, `bytes_yielded`).
- `mtp/connection/scheduler.rs` — `DevicePriorityGate`, `foreground_guard`, `background_yield_point`,
  `foreground_pending`; `mtp/connection/{mod,file_ops,mutation_ops}.rs` for the guard call sites and the manager-level
  `background_yield_point(device_id)`.
- `mtp/connection/CLAUDE.md` + `DETAILS.md` — the foreground/background priority model and "every foreground device op
  takes a guard" rule.
- `docs/specs/mtp-device-scheduler-plan.md` — the original scheduler design this builds on.
- `file_system/volume/mod.rs` — `Volume` / `VolumeReadStream` traits (`pause_releases_read_stream`,
  `open_read_stream_at_offset`, `cancel_and_release`) the auto-yield reuses.
