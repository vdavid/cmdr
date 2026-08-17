# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant holds PER volume id.

`state.rs` the registry + `IndexPhase` machine (a job per file under `state/`, re-exported so `state::*` is the one
path); `manager.rs` the coordinator (`launch_route.rs` the launch table); `cover/` the search-driven walk, with its own
`CLAUDE.md`; `phases/` the first index. Other leaves are one job each (`DETAILS.md` § Module structure).

## Must-knows

- **`INDEX_REGISTRY` guards lifecycle ONLY; disabled = the ABSENCE of a key.** Present ≠ indexed: an enable asks
  `awaits_its_first_scan`.
- **`start_indexing` is lock-first** (reserve the slot before building `IndexManager`); ❌ never hold the registry
  across a blocking or re-entrant manager call.
- **Handles are PUSHED down, never pulled up** (`../read/handles.rs`): ❌ nothing below `lifecycle` imports
  `lifecycle::state`. Withdraw them BEFORE the drain and any DB file removal; withdrawal IS the read-skip.
- **The phase MACHINE is here, the phase EVENT is not**: fire it via `events::set_phase_for`.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`): ❌ never `start_scan` a trait-scanned volume
  — it walks nothing and falsely completes.
- **No `scan_completed_at` ⇒ COVERED in phases, ❌ never bulk-scanned**; every "walk it whole" door goes through
  `cover_or_scan`, ❌ never straight to `start_scan`. ⚠️ A COMPLETED volume takes the replay/reconcile arm FIRST; an
  EMPTY persisted branch set means a legacy interrupted bulk scan ⇒ rebuild. Start it via `state::start_pending_phases`,
  OUTSIDE a registry-held window.
- **Both scan entries ask TWO single-flight questions**: `phases_have_work` (❌ not merely walking; ⚠️ refuses nothing a
  trait-scanned kind reaches today and ❌ isn't dead), then `claim_the_volume` — one `Exclusive` claim answering for
  every other holder, replay included. A MANUAL refusal is REMEMBERED and deferred (the refusing MODE picks which of the
  two); only `phases_have_work` answers `Started`.
- **A whole-volume claim OUTLIVES its call**: it rides into whatever ends the run and drops where that clears
  `mgr.scanning`. ❌ Never release one early — `stop_scan`/`shutdown` only cancel, and the replay TASK outlives the
  replay. **The deferred walk it owes runs later still**, where its holder stops WRITING (`rescan_request::run_if_owed`,
  ❌ never from `Drop`; for both scans the END of the completion task, ❌ not the release), anchored by
  `rescan_request::tests::every_whole_volume_holder_runs_the_rescan_it_owes`.
- **A machine that stops with a non-empty frontier gets another PASS, ❌ never a rescan** (`completion_retry.rs`, an
  in-memory 1/5/15-minute per-volume backoff) through `state::resume_the_phases`, ❌ never `force_scan`, which truncates
  a volume that completed meanwhile — fired by a timer. ❌ Never against a machine that has work.
- **`IndexVolumeKind` is a capability model**: branch on the axis, not the variant. `has_event_journal()` gates journal
  replay, ❌ not `last_event_id.is_some()`.
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ Stale on launch. `..._on` vs
  `apply_freshness_event` is LOCK DISCIPLINE, not style.
- **A fatal storage error STOPS + FAILS the index, ❌ never retries**; recovery is a rebuild.
- **TWO switches, master wins, both gating BACKGROUND work only.** `indexing.enabled` hard-gates
  `Activation::IndexTheVolume`; master-off goes through `stop_indexing`, which ❌ must never write per-drive intent. ⚠️
  A search walk is carved out of both AND `user_disabled`; ❌ don't "fix" that into a refusal.
- **Per-drive intent is the `user_enabled`/`user_disabled` pair, written when the user ASKS** (`start_volume`, BEFORE
  dispatching). ❌ Never infer it from `scan_completed_at`, absent through a first index AND every rescan.
- **Defer `root` auto-start**: scanning `/` stacks TCC popups, so FDA gates ONLY `root`, ❌ never `set_master_enabled`.
- **`publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure** (consumers expand DOWNWARD: one ancestor
  cost ~90 k folders a minute). The bus is neutral, one-way.

Depth on every bullet: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
