# Priority — details

The transport-generic, per-volume priority mechanism for background work. Keyed by volume id; nothing SMB- or
MTP-specific lives here, so any future backend adopts it by reading the same two signals.

## Decision: signals + composed decisions, not a scheduler

**The mechanism is two process-global signals and a handful of pure decision functions — deliberately NOT a scheduler,
token broker, or queue.** Every background consumer already has a natural between-units boundary (between listings,
between images, between chunks) where it polls cheap state; a central scheduler would add ownership, fairness, and
cancellation machinery for no behavior we need. The priority order is enforced by three pairwise edges:

1. **Transfers yield to interactive** — `SmbVolume`'s foreground-yield methods
   (`file_system/volume/backends/smb/foreground_yield.rs`) park `CheckpointStream` between chunks while the share's
   per-volume foreground timestamp is fresh. MTP answers the same question from its own per-device gate (a PTP session
   has an explicit holder; time-based signals aren't needed there).
2. **Drive indexing yields to both** — `indexing/network_scanner/scan_pace.rs` drops the walk's listing budget to ONE
   while the share is browsed OR a transfer touches it. Throttle, never stop: the budget is never zero, so forward
   progress is structural (no starvation quota to get wrong).
3. **Image enrichment yields to both** — the network pass's between-images gate
   (`media_index/network/policy.rs::volume_clear_for_enrichment`) pauses the pass (`PauseReason::NotIdle` →
   `PassOutcome::RetryWhenIdle`) while the app is foreground-busy or a transfer touches the volume, and the resume wait
   (`wait_until_idle_to_resume`) polls the SAME composed condition, so "clear enough to resume" is exactly "clear
   enough to have kept going".

## The signals

- **`foreground`** — "when did the user last do foreground work", app-wide (one atomic) and per volume (a tiny map).
  Stamped by the hot listing IPC (`commands/file_system/listing.rs`), which knows the volume; a scoped note also stamps
  the app-wide slot so an app-wide reader never misses activity. The decision is the pure `is_idle(now, last,
  threshold)` over millis from a monotonic base.
- **`transfers`** — a per-volume COUNT of in-flight user-initiated write operations. Fed from the one write-op
  lifecycle choke point (`write_operations::state::register_operation_status` / `unregister_operation_status`, the
  same pair that maintains the eject busy set, so the two can't drift and the finish rides the manager's panic-safe
  guard). A count, not a flag: overlapping ops keep the volume busy until the LAST ends. Deletes, trash, and drag-out
  promises count too — they all contend on the same device connection a copy does.

## Scope choices (why each consumer reads what it reads)

- **Enrichment: app-wide foreground.** Heavy on-device ML with no deadline; foreground work anywhere is reason enough
  to wait. Its transfer check is per-volume, though — a copy on another device says nothing about this NAS.
- **Scan pacing + transfer-yield: per-volume.** Their contention is one share's session; browsing a local folder must
  not slow a NAS scan or park a NAS copy.
- **Local enrichment reads neither.** It contends on CPU/ANE (governed by the parallelism setting, thermal backoff,
  and the memory watchdog), not on a connection; wiring it to foreground would pause local indexing for no resource
  the user is waiting on.

## Yield shapes (why not one shape)

Drive indexing throttles (budget 64 → 1) because a walk holds cheap resumable state and one listing at a time is
harmless. Enrichment pauses whole passes because a pass holds a Vision backend and prefetch buffers, and its
resume-from-store machinery already exists (staleness skips done rows). Transfers park in place between chunks because
they hold open handles a full stop would invalidate. Same signals, per-consumer shape.

## MTP status

MTP drive indexing adopts the transfer edge automatically: it paces through the same `ScanPacer`, and MTP write ops
register their volume ids in the same status cache. MTP media enrichment never background-sweeps (`media_index`
policy), so there is nothing further to wire; the interactive edge for MTP transfers stays its per-device gate (above).
