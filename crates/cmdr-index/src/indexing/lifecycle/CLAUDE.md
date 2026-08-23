# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant holds PER volume id.

`state.rs` the registry + `IndexPhase` machine (a job per file under `state/`, re-exported so `state::*` is the one
path); `manager.rs` the coordinator (`launch_route.rs` the launch table); `cover/` the search-driven walk and `phases/`
the first index, each with its own `CLAUDE.md`. Other leaves are one job each (`DETAILS.md` § Module structure).

## Must-knows

- **`INDEX_REGISTRY` guards lifecycle ONLY; disabled is the ABSENCE of a key.** Present ≠ indexed: ask
  `awaits_its_first_scan`. Read handles are PUSHED down into `../read/handles.rs` and withdrawn before any drain or DB
  removal, since withdrawal IS the read-skip. ❌ Nothing below `lifecycle` imports `lifecycle::state`. ONE line inserts
  a key and every `remove` is final, which is what lets every acquisition RECOVER from poison (`DETAILS.md` § Poisoning
  the registry lock).
- **Never hold the registry across a blocking or re-entrant manager call.** `start_indexing` reserves lock-first,
  teardown drops the guard before the drain, and a scan start hands the manager out under `IndexPhase::Detached` through
  the one door `state::off_the_registry`; a teardown meeting that window CLAIMS it rather than bouncing.
- **Route by the TYPED `IndexVolumeKind` capability, never the variant or the volume id.** `has_event_journal()` gates
  journal replay; `rescan_scanner_for_kind` picks the scanner, so a trait-scanned volume never reaches `start_scan`,
  which walks nothing and falsely completes.
- **`cover_or_scan` is the ONE door for "walk this volume whole"**: no `scan_completed_at` means the phase machine
  covers it, never a bulk scan. Start the machine via `state::start_pending_phases`, OUTSIDE a registry-held window; one
  that stopped with a non-empty frontier earns another PASS via `state::resume_the_phases`, ❌ never `force_scan`, which
  truncates a volume that completed meanwhile.
- **Both scan entries ask `phases_have_work`, then `claim_the_volume`**, and a whole-volume claim OUTLIVES its call: it
  drops where its run clears `mgr.scanning`. ❌ Never release one early, and run the walk it owes where its holder stops
  WRITING (`rescan_request::run_if_owed`).
- **Fire a pipeline-phase transition through `events::set_phase_for`**; the `IndexPhase` swaps here are a different
  machine. Freshness has ONE total table (`Freshness::on`), and `..._on` vs `apply_freshness_event` is LOCK DISCIPLINE.
- **A fatal storage error STOPS and FAILS the index.** Recovery is a rebuild, never a retry.
- **Two switches gate BACKGROUND work only, master first.** `indexing.enabled` hard-gates `Activation::IndexTheVolume`;
  per-drive intent is the `user_enabled` / `user_disabled` pair, written when the user ASKS, ❌ never inferred from
  `scan_completed_at`. A search walk is carved out of both.
- **FDA gates ONLY `root`'s auto-start**, because scanning `/` stacks TCC popups. It is not a master-switch concern.
- **`publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure**: consumers expand DOWNWARD, and one
  ancestor cost ~90 k folders a minute.

Depth on every bullet: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
