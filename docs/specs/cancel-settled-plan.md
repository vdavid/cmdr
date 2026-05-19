# Cancel + settled-state hardening

## Why this exists

After the fresh-listing-reuse work merged, a real-device test surfaced an MTP wedge:

1. User starts deleting 92 photos from a watched `/DCIM/Camera` (980-entry folder on Pixel).
2. Cancels after 29 files. FE shows "Delete cancelled."
3. User immediately presses F8 again on the survivors.
4. ~30 s later, every MTP operation times out. Volume falls back to default.

Root cause, from the dev log (`req#3` and `req#4` in `mtp::connection::directory_ops`):

- The FE's `refreshPanesAfterTransfer` (`dialog-state.svelte.ts:161`) eagerly fires `refreshListing` on both panes after
  every transfer outcome — including cancel.
- That calls the backend `refresh_listing` Tauri command, which calls `handle_directory_change(listing_id)`, which
  re-reads the directory via `volume.list_directory`.
- On MTP, the re-read of `/DCIM/Camera` (950 entries after the partial delete) takes 17+ s and holds the USB session.
- `notify_mutation(Removed(name))` already patched the cache per file — the eager refresh is pure redundancy on
  watcher-backed volumes.
- The user's second F8 dispatches a new MTP delete which queues behind the refresh, hits the 30 s op timeout, and tears
  down its in-flight USB transfer mid-stream — leaving the device confused. All subsequent ops then time out too.

Three fixes, in order of leverage:

- **#1** — Stop the redundant `refresh_listing` on watcher-backed volumes. Prevents the collision entirely.
- **#2** — Propagate cancel into in-flight mtp-rs ops via PTP `CancelTransaction (0x4000)`. Makes "cancel" mean "stopped
  on the wire," not "stopped issuing more."
- **#4** — Keep the FE "Cancelling…" state visible until the BE confirms the volume is fully settled. Stops the user
  from acting before the system is ready.

(#3 — a blind 500 ms guard — was rejected as race-prone.)

## Scope

- BE-only for #1.
- #2 requires changes to `mtp-rs` (sibling repo at `~/projects-git/vdavid/mtp-rs`).
- #4 spans BE (per-volume in-flight counter + event) and FE (dialog state).
- Applies to copy, move, delete, trash on MTP. SMB inherits #1 for free (it's the oracle's same `listing_is_watched`
  gate). #2 for SMB is a follow-up — `smb2` has its own cancel surface that's not in scope here.
- Local FS keeps today's behavior: `refresh_listing` still runs because `LocalPosixVolume::listing_is_watched` only
  returns true when an FSEvents watcher is attached, which is the case we explicitly want to short-circuit when it's
  safe.

## Milestones

### M1 — Backend `refresh_listing` short-circuit on watcher-backed paths

The smallest, highest-leverage fix.

**Change**: `refresh_listing` (`apps/desktop/src-tauri/src/commands/file_system/listing.rs:327`) checks the volume's
`listing_is_watched(path)` (M1 of the fresh-listing-reuse work added this). If true, log at debug level and return
`TimedOut { data: (), timed_out: false }` without calling `handle_directory_change`.

**Why this is safe**: `listing_is_watched` returning true means the same volume's `notify_mutation` is keeping the cache
live and granular. Every successful `Volume::delete`, `Volume::rename`, etc. already patches `LISTING_CACHE` and emits a
`directory-diff`. A redundant `list_directory` adds nothing and costs a lot on slow volumes.

**Why we don't push it to the FE**: the FE has multiple call sites (`refreshPaneListing` from `dialog-state.svelte.ts`,
plus rename flow, mkdir, etc.). Centralizing the policy on the BE means every caller benefits without code duplication,
and there's no "did this call site get the new check?" risk.

**Tests** (in `apps/desktop/src-tauri/src/commands/file_system/listing_test.rs` or wherever the command tests live):

- `refresh_listing_short_circuits_on_watched_volume`: register a volume reporting `listing_is_watched(path) == true`,
  populate `LISTING_CACHE`, call `refresh_listing`, assert `list_directory` was not called on the volume.
  Counter-wrapping volume pattern from the M2a tests in the previous worktree.
- `refresh_listing_falls_through_on_unwatched`: same volume reporting false, assert `list_directory` is called.
- `refresh_listing_falls_through_on_missing_listing`: no cache entry, assert today's behavior (calls through to
  `handle_directory_change`).

**Docs**:

- Update doc comment on `refresh_listing` to note the short-circuit.
- `commands/CLAUDE.md`: brief mention next to `refresh_listing`.
- `volume/CLAUDE.md`: note this is the third consumer of `listing_is_watched` (oracle, scan walker, now
  `refresh_listing`).

**Checks**: `./scripts/check.sh` end of milestone.

### M2 — Propagate cancel into mtp-rs in-flight ops

Make `OperationIntent::Stopped` actually stop the wire activity, not just the loop above it.

**The PTP angle**: PTP defines `OperationCode::CancelTransaction = 0x4000`. It's an interrupt-endpoint command that
tells the device to abort its current data phase. mtp-rs needs to expose this.

**Phased approach**:

1. **mtp-rs side**: add a cancel token type (`CancelToken { cancelled: Arc<AtomicBool> }` or a `tokio::sync::watch` if
   we want notification). Thread it through `list_objects`, `delete_object`, and any other long-running call. When
   `cancelled` flips true, the call:
   - Aborts before issuing the next per-object request, if it's between cycles.
   - Sends `CancelTransaction` via the interrupt-out endpoint if it's mid-data-phase.
   - Drains any remaining response data so the bulk endpoints stay in a clean state.
   - Returns a typed `Cancelled` error.
2. **cmdr side**: extend `Volume::list_directory`, `Volume::delete`, etc. signatures to accept a cancel token. Or —
   simpler — add the token to `MtpVolume` itself, set on construction from a clone of the relevant `OperationIntent`.
   When intent flips to `Stopped`/`RollingBack`, the token flips. mtp-rs polls it.
3. Default impl: trait method takes an `Option<&CancelToken>`, defaults to `None` (cancel-free). Backends opt in.

**Tests**:

- `mtp_list_directory_cancels_promptly`: virtual MTP device with a slow `list_objects` (intercept the mock to
  artificially delay); fire cancel mid-call; assert call returns `Cancelled` within ~500 ms (not 30 s timeout) and the
  next listing on the same device works immediately.
- `mtp_delete_cancels_between_files`: same idea for `delete_object` batched.
- `smb2_unchanged`: assert SMB is untouched by this milestone — its volume calls work exactly as before.

**Docs**:

- `mtp/CLAUDE.md`: document the cancel propagation, including the `CancelTransaction` opcode and the fact that some
  Android devices may still leave the session in a degraded state after a cancel (unfixable in software).
- `volume/CLAUDE.md`: trait method signature change.
- `mtp-rs` README/docs: cancel token API.

**Checks**: `./scripts/check.sh` end of milestone, plus the virtual-mtp test.

**Open questions** (resolve during implementation):

- Does mtp-rs's current architecture allow injecting a cancel token without breaking existing public API? If yes,
  additive change. If no, a minor version bump in mtp-rs.
- Some Android devices don't honor `CancelTransaction` cleanly. Test against the user's Pixel.

### M4 — Dialog "Cancelling…" stays until the volume is settled

Make the FE honest about state: don't clear the cancelling indicator until the BE has actually quieted down.

**Concept**: each volume tracks an in-flight write-op count. When `start_write_operation` spawns work on a volume,
increment. When the spawned task returns (success, error, or cancelled — and only after any tear-down completes),
decrement. Emit a `volume-settled { volume_id }` Tauri event when the count hits zero.

**FE**: `TransferProgressDialog.svelte` listens for `volume-settled` filtered by the source volume ID. The dialog's
existing "Cancelling…" state stays until both `OperationIntent::Stopped` is set AND the `volume-settled` event has been
received for this volume. The existing complete/error flows are unchanged — only the cancel flow gains the gate.

**UX detail**: if settling takes more than 200 ms, show "Cancelling… (finishing 3 USB transfers)" with the live count.
Radical-transparency principle applied — user sees real work, not generic spinner.

**Tests**:

- `volume_settled_emits_after_cancel`: virtual MTP, start a delete, cancel, wait for `volume-settled`, assert it fires
  within a bounded time (depends on M2; before M2 lands this might be 30 s; after M2, ~500 ms).
- Playwright `mtp-cancel-volume-settled.spec.ts`: real-app test. Start delete on virtual MTP, cancel mid-flight, assert
  the dialog stays in "Cancelling…" state until settled, then clears. Next F8 dispatched immediately after the dialog
  clears succeeds.

**Docs**:

- `write_operations/CLAUDE.md`: new section on the settling contract. Note this is the BE half of the cancel UX promise.
- `file-operations/CLAUDE.md` (FE): document the dialog's two-condition cancel close.

**Checks**: `./scripts/check.sh` end of milestone, including the new Playwright spec.

### M5 — End-to-end verification

1. Run the user's original incident scenario manually with their Pixel:
   - Connect Pixel, navigate to `/DCIM/Camera`.
   - Select ~90 photos. F8 → confirm. Cancel after ~30 files.
   - **Expected**: dialog says "Cancelling…" for ~500 ms, then clears.
   - Immediately F8 again on survivors → second delete dispatches cleanly, no 30 s timeout, no wedge.
2. Same flow on a connected SMB share (only #1 applies; #2 and #4 are MTP-only this round).
3. `./scripts/check.sh --include-slow`.

**Docs**: update top-of-file purpose paragraph in `write_operations/CLAUDE.md` to note "cancel returns the volume to a
clean state immediately on supported backends (MTP today, SMB inherits #1)."

## Risks

- **mtp-rs cancel injection** could ripple through more code paths than expected. Mitigation: cancel token is
  `Option<&CancelToken>`, default `None`, additive only. Existing call sites compile unchanged.
- **PTP `CancelTransaction` on flaky Android devices**: if the device doesn't honor it, the cancel is best-effort and
  the test against the real Pixel becomes the bar. Documented in mtp/CLAUDE.md.
- **#4's "settled" event ordering**: the event must fire AFTER the FE's `write-cancelled` event has been processed,
  otherwise the FE might receive `volume-settled` before it knows the op was cancelled. Implementation note: settle
  decrement happens after `emit("write-cancelled")`, not before.
- **Volume disconnected during cancel**: the in-flight ops count must decrement even if the volume goes away. Use an
  `on_unmount` hook (already in `Volume` trait) to clear the count.
- **Local FS unaffected**: confirmed — local `notify_mutation` plus FSEvents already gives us what M1's check approves;
  nothing breaks.

## Out of scope

- SMB cancel propagation (M2 for SMB) — deferred to a follow-up.
- Any FE change to `refreshPanesAfterTransfer` itself. The BE short-circuit makes the existing FE call cheap.
- Cross-volume operations (copy from MTP to local with cancel): the source volume settles via this work; dest is local
  and already fine.

## Design principle alignment

- **Protect the user's data**: cancel means cancel, on the wire, every time.
- **Be respectful to the user's resources**: stops issuing a 17 s `list_directory` per transfer outcome on MTP.
- **Radical transparency**: the "Cancelling…" indicator shows real work happening; user sees system state, not a generic
  spinner.
- **Elegance over hacks**: same `listing_is_watched` signal does triple duty (oracle, scan walker, refresh_listing). No
  new ad-hoc state machines.
- **Smart backend, thin frontend**: #1 lives in the BE so every FE caller benefits without changes.
