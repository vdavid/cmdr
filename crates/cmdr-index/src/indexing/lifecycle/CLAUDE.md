# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant here holds PER volume id.

`state.rs` the registry + `IndexPhase` machine (a job per file under `state/`, re-exported so `state::*` is the one
path); `manager.rs` the coordinator (`launch_route.rs` the launch table); `cover/` the search-driven walk, which carries
its own `CLAUDE.md`; `phases/` the first index. The other leaves are one job each; `DETAILS.md` § Module structure names
them.

## Must-knows

- **`INDEX_REGISTRY` guards lifecycle ONLY, and disabled = the ABSENCE of a key.** Present ≠ indexed: an enable asks
  `awaits_its_first_scan`.
- **`start_indexing` is lock-first** (reserve the slot before building `IndexManager`), and ❌ never hold the registry
  across a blocking or re-entrant manager call.
- **Handles are PUSHED down, never pulled up** (`../read/handles.rs`): ❌ nothing below `lifecycle` imports
  `lifecycle::state`. Withdraw them BEFORE the drain and before any DB file goes; withdrawal IS the read-skip.
- **The phase MACHINE is here; the phase EVENT is not**: fire it via `events::set_phase_for`.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`). ❌ Never `start_scan` a trait-scanned
  volume: it walks nothing and falsely completes.
- **No `scan_completed_at` ⇒ COVERED in phases, ❌ never bulk-scanned**; every "walk it whole" door goes through
  `cover_or_scan`, ❌ never straight to `start_scan`. ⚠️ A COMPLETED volume takes the replay/reconcile arm FIRST, and an
  EMPTY persisted branch set means a legacy interrupted bulk scan ⇒ rebuild. Start the machine via
  `state::start_pending_phases`, OUTSIDE a registry-held window.
- **Every scan entry asks TWO single-flight questions** (`mgr.scanning` AND `cover::ground_being_walked`), plus the
  machine having WORK, ❌ not merely walking. ❌ Don't collapse them or classify them by text (both are
  `ScanStartError`). A MANUAL rescan they refuse on a COMPLETED volume is REMEMBERED (`rescan_request`) and fired from
  `cover::release_ground`, claim first.
- **A machine that stops with a non-empty frontier gets another PASS, ❌ never a rescan** (`completion_retry.rs`, an
  in-memory 1/5/15-minute per-volume backoff). It goes through `state::resume_the_phases`, ❌ never `force_scan`: on a
  volume that completed meanwhile that one truncates, fired by a timer. ❌ Never runs against a machine that has work.
- **`IndexVolumeKind` is a capability model**: branch on the axis, not the variant. `has_event_journal()` gates journal
  replay, ❌ not `last_event_id.is_some()`.
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ Stale on launch. `..._on` vs
  `apply_freshness_event` is LOCK DISCIPLINE, not style.
- **A fatal storage error STOPS + FAILS the index, ❌ never retries**; recovery is a rebuild.
- **TWO switches, master wins, and both gate BACKGROUND work only.** `indexing.enabled` hard-gates
  `Activation::IndexTheVolume`; master-off goes through `stop_indexing`, which ❌ must never write per-drive intent. ⚠️
  A search walk is carved out of both switches AND `user_disabled`; ❌ don't "fix" that into a refusal.
- **Per-drive intent is the `user_enabled`/`user_disabled` pair, written when the user ASKS** (`start_volume`, BEFORE
  dispatching). ❌ Never infer it from `scan_completed_at`: absent through a first index AND every rescan.
- **Defer `root` auto-start**: scanning `/` stacks TCC popups, so FDA gates ONLY `root`, ❌ never `set_master_enabled`.
- **`publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure** (consumers expand DOWNWARD: one ancestor
  cost ~90 k folders a minute). The bus is neutral and one-way.

Depth on every bullet: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
