# Transfer queue + pause for file operations

Created 2026-06-21. Status: planned.

Add **pause/resume** and a **queue** to copy, move, and delete, working uniformly across all volume types (local, MTP,
SMB, and future FTP). Surface them with a **Pause** and a **Queue (F2)** control on every transfer progress dialog, and
a standalone **queue window** (a real macOS window) that lists every operation with per-row pause/resume/cancel,
multi-select + "Cancel selected", and global pause/resume.

This spec captures the _why_ behind each decision so the implementing agent can adapt. Read it alongside the colocated
docs it points to; do not duplicate mechanism already documented there.

## Why this, why now

The two most-missed features. Today a copy/move/delete spawns immediately and the only foreground view is a modal
progress dialog; there is no way to line operations up, no way to pause one, and no single place to manage several at
once. The machinery to build on is strong (typed per-operation state, granular cancellation, a single progress-emit
path, a clean `Volume` trait), so these features slot in as orthogonal layers rather than a rewrite.

## The reframe that shapes everything

**Cmdr already runs operations in parallel.** Operations spawn immediately and get their own `WriteOperationState` in
the global `WRITE_OPERATION_STATE` map. There is no coordinator and no serialization. **Crucially, there are FIVE
independent spawn paths, not one** (verified): `start_write_operation` (`mod.rs`) covers local copy/move/trash and local
delete, but volume-aware delete (`mod.rs` inline branch), `copy_between_volumes` (`transfer/volume_copy.rs`),
`move_between_volumes`, and `move_within_same_volume` (`transfer/volume_move.rs`) each hand-roll their own
`tokio::spawn`

- state-insert + status-register + `WriteSettledGuard`. The MTP/SMB/cross-volume paths — exactly the ones the lane model
  exists for — do **not** go through `start_write_operation`. So the manager must unify all five, not refactor one (see
  M1).

So **"queue" is not adding parallelism — it is adding the ability to _serialize_ what currently always runs
concurrently.** Both features need the same missing piece first: a central **Operation Manager** (scheduler + registry
with real lifecycle states). Build that once and queue + pause become small riders on it. This is the backbone, and the
reason the milestone order below leads with the manager.

## Design principles in play

(From [`docs/design-principles.md`](../design-principles.md) and `AGENTS.md` § Principles.)

- **Rock solid (3).** Never block the main thread; honest progress/ETA; everything cancelable, including queued and
  paused work. Handle the hostile case: pausing must never tear a destination, a dropped connection mid-pause must
  degrade gracefully.
- **Protect data (4).** Pause parks at a safe boundary (between files for v1); a paused op holds only its invisible
  `.cmdr-tmp-<uuid>`, never a torn target. Cancel keeps fully-copied files (no rollback in this effort, see below).
- **Respect resources (5).** Don't thrash a shared device. Two ops on the same physical resource serialize by default;
  independent resources run in parallel. This is the _lane_ model below, and it is also the correct answer to the MTP
  single-USB-pipe reality and the future FTP connection-limit case.
- **Delightful, macOS-native UX (1).** The queue window is a real vibrancy window built on the existing Settings-window
  pattern, not a bolted-on panel. Reuse existing components.
- **Elegance (2).** The manager is the single source of truth for operation lifecycle; the frontend subscribes, never
  polls (`subscribe, don't poll`).

## Scope decisions (read before building)

- **Cancel-only, no rollback in this effort.** Per David: the new pause/queue UI exposes only **Cancel** (keep
  fully-copied files, delete only the last partial — the existing `cancel_write_operation(id, rollback=false)` path). Do
  **not** remove the existing rollback machinery (the current `TransferProgressDialog` keeps its existing Cancel +
  Rollback buttons as they are); simply don't add any rollback affordance to the new queue window or new buttons.
  "Cancel selected" / per-row cancel all map to `rollback=false`.
- **"Queue (F2)" is dialog-scoped, not a global shortcut.** F2 is already globally bound to `file.rename`
  (`command-registry.ts`). Like Total Commander's copy-dialog-local F2, our "Queue" key is active **only while a
  transfer progress dialog is open/focused** — a component-level keydown handler, not a `command-registry` entry. This
  avoids the global conflict entirely. (If we ever want a global "open queue window" command, that gets its own free
  shortcut, separate from this.)
- **Semantics of the two dialog controls:**
  - **Pause**: pause _this_ operation in place (resumable). Toggles to Resume.
  - **Queue (F2)**: "manage in the queue / send to background" — close the modal, the op keeps its place (running or
    queued) and is now managed in the queue window, which comes forward. This is the "stop blocking me, I want to keep
    working" action. It does **not** pause. **This is frontend-only state** (a store flag deciding whether to show the
    modal vs only the window) — no backend behavior changes, so it needs no Tauri command. The backend op runs
    identically whether or not its modal is showing.
- **Lane model (the parallel-ops answer).** An operation acquires a slot in **every lane it touches** (its source
  volume's lane _and_ its destination volume's lane) and runs only when all those lanes have a free slot. Default lane
  budget = **1** (serialize within a lane). Effects, all of which are the behavior we want:
  - Two MTP ops → serialize (shared device / single pipe).
  - Two ops on the same local disk → serialize (no seek thrash).
  - MTP→local while local→local(other disk) → may run in parallel (disjoint lanes).
  - An MTP→local op holds _both_ the MTP lane and the local lane while it runs.
- **Lane keys come from a new `Volume::lane_key()` trait method, NOT from parsing `volume_id` strings.** The `Volume`
  trait today exposes only `name()`, `root()`, `as_any()`, and capability flags — there is no stable device/server id
  accessor, and the `volume_id` is a separate `String` threaded alongside. Parsing it would violate
  `no-string-matching`. So M1 adds `fn lane_key(&self) -> LaneKey` to the trait (default = volume root):
  `LocalPosixVolume` → mount root (or a single `"local"` lane as the documented v1 fallback if mount-root detection is
  awkward); `MtpVolume` → device serial; `SmbVolume` → server (or server+share); `InMemoryVolume` → a chained
  `with_lane_key(self, key)` builder (matching the existing `with_space_info` / `with_entries` pattern), defaulting to
  the root lane so the ~169 existing `new(...)` call sites stay untouched, and tests opt in to force same-lane
  (serialize) vs different-lane (parallel). This is a real trait addition, budgeted as its own M1 sub-step.
- **Admission order is a single global FIFO with atomic multi-lane reservation, NOT per-lane queues.** Per-lane FIFO
  with two-lane ops invites head-of-line starvation (a cross-volume op stuck behind churn on either lane). Instead: keep
  one ordered queue of pending ops; on each admission pass, walk oldest-first and admit the first op whose _every_ lane
  has a free slot, reserving all its slots atomically. This sidesteps the two-resource ordering problem.
- **v1 vs v2 is explicit** (see the staging section). v1 ships a correct, useful queue+pause; v2 refines budgets,
  mid-file pause, persistence, and reordering.

## v1 vs v2 staging

**v1 (this effort):**

- Operation Manager: registry + lifecycle states (Queued, Running, Paused, Done, Cancelled, Failed) + lane-based
  admission (budget 1 per lane; an op holds a slot in each lane it touches).
- Pause/resume, parking **between files** (and at delete-walker boundaries). Cancel-while-paused works.
- Queue window (real Tauri window, Settings pattern, macOS vibrancy): lists all ops, inline progress reusing existing
  components, per-row Pause/Resume/Cancel, multi-select + "Cancel selected", global Pause all / Resume all.
- Progress dialog gains **Pause** + **Queue (F2)** controls.
- Auto-queue: starting an op whose lane is busy enqueues it (status Queued) and surfaces the queue window; no second
  modal stacks.
- Lightweight toasts for queued/started/completed (reuse the toast system; keep it quiet).

**v2 (deferred — capture as a `later/` follow-up at the end, do not build now):**

- Per-lane budgets > 1 and configurable (e.g. FTP = min(5, server limit)); finer local-device detection.
- Mid-large-file pause (park at a chunk boundary, keep the stream + temp + connection alive), with keep-alive or
  reconnect-on-resume for SMB/MTP idle timeouts.
- Queue reordering (drag) and priorities.
- Persisting the queue across app restarts.

## Architecture

### Backend: the Operation Manager

New module under `write_operations/` (e.g. `manager.rs`, name to be decided by the implementer; keep it beside
`state.rs` since it owns lifecycle). Responsibilities:

1. **One managed-spawn seam that all five spawn paths call.** The core M1 work is introducing a single
   `spawn_managed(descriptor, deferred_start)` (name TBD) that replaces the five hand-rolled `tokio::spawn` +
   state-insert + status-register + `WriteSettledGuard` blocks in `mod.rs` (`start_write_operation` + the volume-delete
   branch), `volume_copy.rs`, and `volume_move.rs` (two paths). Every entry point hands the manager (a) a descriptor
   (id, type, source/dest summary, the lane keys it needs) and (b) a deferred start (the data/closure describing how to
   begin the actual work). This unification is the ideal end state (`ideal-over-cheap`) and is the only way MTP/SMB/
   cross-volume ops get queued at all. Treat "refactor `start_write_operation`" as shorthand for "build this seam and
   route all five through it."
2. **Single registry — one source of truth.** The manager owns ONE registry of operation records: id, type, source/dest
   summary, lane keys, and lifecycle status (Queued/Running/Paused/Done/Cancelled/Failed). Today the same facts are
   split across `WRITE_OPERATION_STATE` (per-op state) and `OPERATION_STATUS_CACHE` (`OperationStatusInternal`, which
   has `phase` but no lifecycle status). Fold lifecycle ownership into the manager registry; let the existing
   `recompute_and_emit_busy_volumes` / `volumes-busy-changed` path derive from the manager's membership rather than
   double-maintaining it. Don't duplicate per-op progress state — the manager references it. **Preserve the external
   busy seam:** `register_external_volume_op` (macOS drag-out file promises, called from `native_drag::fulfillment`)
   marks volumes busy with **no** `WRITE_OPERATION_STATE` entry and no manager record. When the manager takes over the
   busy-set derivation, that external seam must still feed it, or the eject-during-drag-out guard regresses. Keep the
   busy set = (manager membership) ∪ (external registrations), and keep its membership-debounced emit
   (`LAST_EMITTED_BUSY`).
3. **Lane model.** Lane keys come from `Volume::lane_key()` (see Scope decisions). An op touches the lanes of its source
   and destination volumes (same-volume ops touch one). Budget = 1 per lane in v1.
4. **Admission — global FIFO, atomic multi-lane reservation.** When an op is requested, register it and return its
   `operationId` to the frontend immediately (the UI shows the queued/running row at once). Then run an admission pass:
   walk the pending queue oldest-first and admit the first op whose every lane has a free slot, reserving all its slots
   atomically, marking it Running and invoking its deferred start (spawning the real work). Else it stays Queued.
5. **Dequeue on settle — explicit happy-path call, NOT inside `Drop`.** Do not free lanes + admit-next inside
   `WriteSettledGuard::Drop`: that would re-enter the manager (and possibly spawn the next op) during the previous op's
   unwind, risking panic-in-Drop (abort) or deadlock if a manager lock is held up-stack. Instead the spawn task calls
   `manager.on_settled(id)` explicitly on normal exit (sequenced like the existing terminal-event emit), which frees the
   op's lane slots and runs an admission pass. The `Drop` path stays a pure safety net that only **frees slots** (never
   spawns), so a panicking op still releases its lanes. Order: terminal event → `on_settled` (free + admit) on happy
   path; Drop frees slots if the task panicked before `on_settled`.
6. **Pause and the lane.** A **paused Running** op still **holds its lane slots** (it occupies the resource; we don't
   want a queued op to start and then fight it on resume). A **Queued** op that is "paused/held" simply isn't admitted.
   The **paused** bit lives on the manager's record (a `paused: bool` or a lifecycle `status`), NOT on `OperationIntent`
   (which stays the cancel/rollback machine) and NOT as a `WriteOperationPhase` (which is the progress phase:
   Scanning/Copying/Flushing/RollingBack). Audit `get_operation_status` / `list_active_operations` consumers so none
   assume `phase` covers "not running". Specifically: `get_operation_status().is_running` derives from
   `WRITE_OPERATION_STATE` membership, and a paused op stays in the map — so it reports `is_running: true` while paused
   ("running but not progressing"). Any consumer reading `is_running` to mean "the bar is moving" must read the new
   paused bit instead.
7. **Events.** Emit a typed `operations-changed` event carrying the registry snapshot for **membership + lifecycle
   status** (including paused), derived from the single registry (point 2). The window subscribes to it for the row set,
   and to the existing per-file `write-progress` stream for live per-row bars/ETA — so the snapshot stays thin (no 200
   ms progress fattening it).
8. **IPC commands** (typed, via `commands.*`): `list_operations`, `pause_operation(id)`, `resume_operation(id)`,
   `cancel_operation(id)` (→ existing cancel, rollback=false), `cancel_operations(ids)`, `pause_all`, `resume_all`. (No
   `set_operation_background` — that's frontend-only, see Scope.) Register in `ipc.rs` + `ipc_collectors.rs`; regenerate
   bindings. App `#[tauri::command]`s go through the `tauri_specta` invoke handler, not the capability ACL, so the queue
   window needs no per-command `core:` grant for these.

**Why a deferred start and not "spawn then block on a lane semaphore at the top":** blocking a spawned op on a semaphore
would hold a `spawn_blocking` thread (a finite pool) idle for every queued op — a resource leak that violates principle
5 and can deadlock the pool under many queued ops. The manager must hold _data_ describing how to start, and only spawn
on admission. (Note the related, accepted asymmetry under Pause: a paused Running op _does_ park its `spawn_blocking`
thread — see S-note there.)

### Backend: pause/resume mechanism

Pause gates at exactly the points cancellation is already checked, so the data-safety ordering (cancel/skip checks
before any destructive call) is preserved. Those points (see `transfer/transfer_driver.rs`):

- `drive_transfer_serial_sync` and `drive_transfer_serial_async`: the per-source loop top (currently
  `if is_cancelled(&state.intent) { … }`), plus the async driver's post-loop intent check.
- The delete walker (`delete/walker.rs`): gate the **delete-phase** loops (files, then dirs), NOT the scan-recursion
  `is_cancelled` site. Pausing mid-scan (e.g. a multi-second MTP USB enumeration) freezes a half-counted "Scanning…" for
  no clear reason; let the scan finish, then park before the first destructive call. This also keeps pause at a
  between-files boundary in the destructive phase, exactly where it's data-safe.
- The per-file chunk callbacks `make_serial_per_file_progress` / `make_concurrent_per_file_progress` (currently return
  `ControlFlow::Break` on cancel) stay **cancel-only** in v1 — mid-file parking is v2. They must keep breaking on cancel
  as today.

**v1 pause covers serial transfers only; the concurrent copy path is documented as not honoring mid-batch pause.** The
loop-top gate lives in the serial drivers. `copy_volumes_with_progress` has a `FuturesUnordered` concurrent path where
several files are in flight with no single "between files" boundary. For v1, state plainly (in `transfer/DETAILS.md`)
that a pause on a concurrent-path op takes effect only once the in-flight batch drains to the next admission point (or,
simpler and acceptable: pause is a no-op visual on that path until v2). Don't half-gate it in a way that risks data
safety. Serial paths (local copy/move, cross-volume serial) honor pause between files.

**Conflict-prompt interaction (corrected framing).** The conflict-dispatch mutex is **per-`WriteOperationState`** (one
per op), acquired and released per dispatch, never held across a file write — so a paused op does NOT block "the one
human" across other ops. The real, narrower fact to document: an op blocked awaiting a conflict resolution (on the
oneshot) won't observe pause until the human answers. That's acceptable for v1; just state that pause parks between
files and a pending conflict prompt is a separate wait the gate doesn't preempt.

**Mechanism (avoid polling).** Add a `PauseGate` to `WriteOperationState`: a paused flag plus a `std::sync::Condvar`
(for the sync driver, which runs inside `spawn_blocking`) and a `tokio::sync::Notify` (for the async volume drivers).

- `pause()`: set paused = true.
- `resume()`: clear paused, `notify_all()` (condvar) + `notify_waiters()` (Notify).
- `wait_while_paused_sync(&intent)`: while paused and not cancelled, `condvar.wait`. Returns immediately if cancelled
  (so cancel-while-paused unblocks and the existing cancel path takes over).
- `wait_while_paused_async(&intent).await`: loop { if !paused or cancelled break; `notify.notified().await` }.

Call `wait_while_paused_*` immediately **after** the existing `is_cancelled` check at each loop boundary. Keep `Paused`
**out** of the `OperationIntent` enum (which stays `Running → RollingBack/Stopped`, `Stopped` terminal): pause is
orthogonal to the cancel/rollback machine and must not perturb the validated transitions documented in
`write_operations/CLAUDE.md`. Pause is a separate gate; cancellation still wins over a paused state.

**Accepted resource asymmetry (principle 5 note).** A `wait_while_paused_sync` on a `std::sync::Condvar` parks the op's
`spawn_blocking` pool thread for the whole pause — the same thing the deferred-start design avoids for _queued_ ops. A
paused Running op legitimately holds its lane and is rarer than queued ops, so v1 accepts this, but document it: many
simultaneously-paused local ops could pressure the blocking pool. If this proves real, v2 bounds concurrent
paused-and-parked ops. (Tokio's `spawn_blocking` pool is large by default, so this is a noted edge, not a v1 blocker.)

**Connection-idle risk (document, don't fully solve in v1):** a long pause holds SMB/MTP connections idle and may hit
server/USB timeouts. v1 accepts that resume may surface a normal transient error (SMB already reconnects; MTP
stale-handle has a one-shot retry). v2 adds keep-alive / explicit reconnect-on-resume. Note this in the
`write_operations` DETAILS.

### Frontend: the queue window

Clone the Settings-window pattern: `apps/desktop/src/lib/file-operations/queue/queue-window.ts` (opener +
focus-if-open + macOS vibrancy with reduce-transparency fallback), a `routes/queue/+page.svelte` route, and a new
capability file `apps/desktop/src-tauri/capabilities/queue.json`.

- **Capabilities.** Mirror `settings.json`'s window perms
  (`core:window:allow-close/-set-focus/-set-min-size/ -set-max-size/-set-effects/-start-dragging/-outer-position/-outer-size/-scale-factor`,
  `core:event:default`, `core:app:allow-set-app-theme`, `core:webview:allow-internal-toggle-devtools`) **plus only what
  the window actually calls** — and **drop `store:default` and `dialog:allow-ask`** unless the window genuinely needs
  them (v1 has no persistence and no confirm-dialog need; keep-partials cancel needs no prompt). App
  `#[tauri::command]`s (`pause_operation`, etc.) go through the `tauri_specta` invoke handler, NOT the capability ACL —
  Settings grants zero app-command perms — so the queue window needs no per-command grant. The opener calls
  (`getByLabel` + `readMonitors()`) run on the **calling** (main) window, which **already has** everything they need:
  `default.json`'s `core:default` expands to `core:window:default` (which includes `allow-get-all-windows` and
  `allow-available-monitors`) plus an explicit `core:webview:allow-create-webview-window`. So there is **nothing to add,
  and nothing to "verify" by grepping `default.json`** for those literals (they're transitive — an agent grepping will
  find nothing and might wrongly add a redundant line); `adding-a-window.md` confirms main has all of these. Follow that
  guide exactly (route + opener + capability recipe). Perms fail **silently**: `await` every Tauri call in try/catch
  with `log.warn`.
- **macOS-native look.** Vibrancy via `setEffects` (Sidebar or HUD material — pick what reads best for a transfer
  manager; HUD/`Effect.UnderWindowBackground` is common for utility windows), overlay title bar, traffic-light inset,
  reduce-transparency opaque fallback keyed off `html.reduce-transparency`. Honor `prefers-reduced-motion`. Use design
  tokens only (stylelint enforces).
- **Ops store.** New store (`.svelte.ts`) subscribing to `operations-changed` (membership/status) and `write-progress`
  (per-row live bars). No polling. This is also what the progress dialog and any status indicator can read.
- **List UI — reuse components.** Build rows from existing primitives: the progress bar / phase body used in
  `TransferProgressDialog` / `ScanPhaseBody`, `Button`, `Icon`, `Spinner`, the existing multi-select list interaction if
  one exists (check `query-dialogs` / Select dialog work). Each row: type+direction (reuse `DirectionIndicator`),
  source→dest summary, live progress + ETA, status, and per-row **Pause/Resume** and **Cancel**. Window chrome:
  multi-select + **Cancel selected**, and global **Pause all** / **Resume all**. Cancel of a Queued row drops it
  silently; Cancel of a Running/Paused row uses the normal keep-partials cancel (no confirm needed for keep-partials,
  but match the app's existing cancel affordance).
- **All copy via i18n catalog** (`messages/en/<area>.json`, `t()` / `<Trans>`); no raw user-facing strings
  (`cmdr/no-raw-user-facing-string`). Sentence case, active voice, no "error/failed" in messages.

### Frontend: progress dialog controls

In `TransferProgressDialog.svelte`, add a **Pause/Resume** toggle button and a **Queue** button (reuse `Button`), plus a
dialog-scoped **F2** keydown → Queue. Pause → `pause_operation`/`resume_operation`. Queue → `set_operation_background` +
open the queue window + close the modal (defer `close()` past the tick per the self-closing-webview gotcha if the dialog
ever lives in its own webview — here it's a soft dialog in main, so just unmount). Keep existing Cancel/Rollback buttons
untouched.

## Milestones

Each milestone ends green (`pnpm check` for the relevant scope). Run `--fast` while iterating, plain per milestone,
`--include-slow` before wrapping (`no-tail-checker`: never truncate check output). TDD red→green where marked.

### M1 — Operation Manager + lanes + admission (backend, no pause yet)

This is materially bigger than "refactor one function". Sub-steps:

- **Add `Volume::lane_key()`** to the trait (+ each backend impl + `InMemoryVolume` constructor-supplied key for tests).
  Reject any `volume_id`-string parsing (`no-string-matching`).
- **Build the managed-spawn seam** (`spawn_managed`) and route **all five** spawn paths through it
  (`start_write_operation`
  - the volume-delete branch in `mod.rs`; `copy_between_volumes`; `move_between_volumes`; `move_within_same_volume`),
    replacing each hand-rolled `tokio::spawn` + state-insert + status-register + `WriteSettledGuard` block.
- **Single registry** owning lifecycle status; `recompute_and_emit_busy_volumes` derives from it (no double-maintain).
- **Admission**: global FIFO, atomic multi-lane reservation; return `operationId` immediately, admit on a pass.
- **Dequeue**: explicit `manager.on_settled(id)` on happy-path exit (free + admit); `Drop` frees slots only, never
  spawns.
- **`operations-changed`** typed event (membership + status, thin) + `list_operations` / `cancel_operation` /
  `cancel_operations` IPC (cancel → existing `rollback=false`). Register in `ipc.rs` + `ipc_collectors.rs`; regen
  bindings.

Existing single-op behavior must be unchanged when nothing else is running (the common case still spawns immediately).

- **Tests (TDD red→green for the scheduler — risky core logic):**
  - Admission: op admitted immediately when lanes free; enqueued (Queued) when a needed lane is busy.
  - Lane occupancy: an op holds a slot in each lane it touches; an op needing two lanes waits until both free.
  - FIFO dequeue on settle; multi-lane op only admitted when all its lanes free.
  - Two same-lane ops serialize; two disjoint-lane ops both run (use `InMemoryVolume` with an artificial throttle).
  - Cancel of a Queued op removes it without spawning; cancel of a Running op uses the existing path.
- **Docs:** `write_operations/CLAUDE.md` (manager must-knows: lanes, deferred spawn, settle→dequeue), its DETAILS (full
  model + the deferred-thunk rationale). `docs/architecture.md` map entry pointer.
- **Checks:** `pnpm check rust` (clippy, tests, lock-poison, unwrap, error-string-match).

### M2 — Pause / resume (backend)

Add `PauseGate` to `WriteOperationState`; wire `wait_while_paused_{sync,async}` after the cancel checks in both drivers
and the delete walker. Add `pause_operation` / `resume_operation` / `pause_all` / `resume_all` IPC. Manager: paused
Running op keeps its lane slots; paused Queued op is held.

- **Tests (TDD red→green — risky logic touching the transfer loop):**
  - While paused, no further progress events fire and no further sources transfer (drive an `InMemoryVolume` op, pause,
    assert the files-done counter stops advancing).
  - Resume continues to completion.
  - Cancel-while-paused unblocks the gate and cancels (keeps already-copied files; deletes only the last partial).
  - Pause is orthogonal to `OperationIntent` (intent stays `Running`; the validated transitions are untouched); the
    paused bit surfaces in the manager record / `operations-changed` snapshot, not in `WriteOperationPhase`.
  - Serial path honors between-files pause; assert the documented concurrent-path behavior (no mid-batch pause in v1) so
    a future change to gate it is a deliberate decision, not an accident.
- **Docs:** pause mechanism + connection-idle caveat in `write_operations` CLAUDE/DETAILS and `transfer/DETAILS.md`
  (where the loop boundaries are).
- **Checks:** `pnpm check rust`.

### M3 — Queue window + ops store + bindings (frontend)

Regenerate bindings (`pnpm bindings:regen`). Build `queue-window.ts`, `routes/queue/+page.svelte`,
`capabilities/ queue.json`, register the window, and the ops store. Render the list reusing existing components with
per-row Pause/Resume/Cancel, multi-select + Cancel selected, global Pause all/Resume all. Add i18n keys.

- **Tests:**
  - Unit/component: ops store reduces `operations-changed` + `write-progress` correctly; row renders each status;
    Cancel-selected calls `cancel_operations` with the selected ids; Pause toggles to Resume.
  - a11y test for the window (AA contrast, screen-reader labels) matching the pattern of existing `.a11y.test.ts`.
  - E2E (Playwright, `pnpm check desktop-e2e-playwright`): start two same-lane ops, open the queue window, see one
    Running + one Queued, cancel the queued one, pause+resume the running one. (Read `test/e2e-playwright/CLAUDE.md`
    first.)
- **Docs:** new `src/lib/file-operations/queue/CLAUDE.md` + `DETAILS.md` (window pattern, store, why a hard window);
  update `capabilities/CLAUDE.md` (new window) and `capabilities/DETAILS.md`; `docs/architecture.md` map entry; if the
  window recipe gained any nuance, update `docs/guides/adding-a-window.md`.
- **Checks:** `pnpm check svelte ts` + bindings drift + stylelint; then `desktop-e2e-playwright`.

### M4 — Progress-dialog controls + auto-queue + F2 (frontend)

Add Pause/Resume + Queue buttons to `TransferProgressDialog`, the dialog-scoped F2 → Queue handler, the
`set_operation_background` wiring, and the auto-queue surfacing (starting an op on a busy lane opens/raises the queue
window; no stacked modal). Quiet toasts for queued/started/completed.

- **Tests:**
  - Component: Pause button calls pause then flips to Resume; Queue button backgrounds (sets the frontend-only flag) +
    opens the window + closes the modal; F2 triggers Queue only while the dialog is open and intercepts before the
    global key handler. **Negative test:** F2 with the dialog _closed_ still dispatches `file.rename` (the dialog
    handler must not leak the binding).
  - E2E: start an op, hit Queue/F2, assert the modal closes and the window shows the op still running; start a second
    same-lane op and assert it appears Queued without a second modal.
  - Update existing `TransferProgressDialog.*.test.ts` for the new buttons (don't break cancel-settle / conflict /
    flushing / rollback specs).
- **Docs:** `src/lib/file-operations/transfer/CLAUDE.md` + DETAILS (the new controls, the dialog-scoped F2, the
  background→window flow).
- **Checks:** `pnpm check svelte ts`; `desktop-e2e-playwright`.

### M5 — Docs, polish, full verification

Reconcile all colocated docs, add the v2 `later/` follow-up spec, tick this spec in `docs/specs/index.md`. Run the app
on macOS (`pnpm dev --worktree transfer-queue-pause`), exercise the full flow, screenshot the queue window, and do a
visual polish pass (vibrancy, spacing tokens, dark/light, reduce-transparency, reduced-motion). Final
`pnpm check --include-slow -q`.

- **Docs:** everything above reconciled; `later/transfer-queue-v2-plan.md` capturing per-lane budgets / mid-file pause /
  persistence / reorder; `docs/specs/index.md` entries.
- **Checks:** `pnpm check --include-slow` (full, untruncated).

## Parallelism notes (for execution)

Mostly sequential; the milestones build on each other. The only safe parallelism:

- Within **M1**, the lane-key derivation helper + its unit tests can be written alongside the registry/admission code.
- The **M3** i18n message keys and the `capabilities/queue.json` + route scaffolding can be prepared while the ops store
  is being written. Do **not** parallelize M2 against M1 (M2 edits the same drivers M1's manager spawns) or M4 against
  M3 (M4 consumes M3's store and window). When in doubt, run sequentially — we are not in a hurry.

## Risks / open questions

- **The managed-spawn seam touches all five spawn paths** — the riskiest structural change; it owns the lifecycle every
  op flows through. Keep the "register + return id immediately" contract intact (the dialog opens before any stalled
  mount can block). Heavy unit coverage in M1.
- **Dequeue must not happen in `Drop`** (reentrancy / panic-in-Drop / deadlock). `on_settled` is the happy-path call;
  Drop only frees slots. Pinned by an M1 test that a panicking op still releases its lanes without spawning the next.
- **Multi-lane admission**: global FIFO + atomic all-lanes reservation avoids the two-resource starvation that per-lane
  queues invite. Covered by M1 tests (a two-lane op admits only when both free; doesn't starve behind one-lane churn).
- **`Volume::lane_key` is a new trait method**, not derivable from today's `Volume` surface; `InMemoryVolume` must take
  a constructor key so M1/M3/E2E tests can force same-lane vs different-lane. Tests can't be written before this lands.
- **Paused state has no home in `OperationIntent`/`WriteOperationPhase`** — it lives on the manager record. Audit
  `get_operation_status` / `list_active_operations` consumers for "phase == not-running" assumptions.
- **Window perms fail silently.** Smoke-test the window with `pnpm dev` after M3; every Tauri call `await`+try/catch.
  Verify the main window's opener perms (`create-webview-window`, `get-all-windows`, `available-monitors`).
- **Progress event volume**: per-row `write-progress` subscription + thin `operations-changed` for membership, not a fat
  snapshot every 200 ms.
- **Concurrent copy path** (`copy_volumes_with_progress` `FuturesUnordered`) doesn't honor mid-batch pause in v1 —
  documented and asserted, not silently half-implemented.
- **Existing `TransferProgressDialog` tests** are extensive; budget time to update them for the new buttons without
  regressing cancel-settle / conflict / flushing behavior.

## Pointers (do not transcribe — read these)

- Backend lifecycle + invariants: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (+ DETAILS).
- Transfer drivers (where pause gates): `…/write_operations/transfer/CLAUDE.md` + `transfer_driver.rs`.
- Window pattern + perms: `apps/desktop/src/lib/settings/settings-window.ts`, `capabilities/CLAUDE.md`,
  `docs/guides/adding-a-window.md`.
- Commands + shortcuts: `apps/desktop/src/lib/commands/CLAUDE.md`, `command-registry.ts`.
- Frontend transfer UI: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.
- Product values: `docs/design-principles.md`, `docs/style-guide.md`.
