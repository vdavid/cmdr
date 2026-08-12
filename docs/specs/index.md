# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under `later/`.

## In progress

- [x] 2026-08-10 `quit-and-operation-lifetime.md` - **Done (Q1, Q2, Q3 all landed).** The backend now owns operation
      lifetime and the quit decision. The `beforeunload` handler that cancelled the GLOBAL registry (killing a
      backgrounded transfer on a dev reload, and racing un-awaited at quit) is gone, replaced by a Rust-owned quit gate:
      it prompts when anything non-instant is active, counts down 15 seconds on its own OS thread (so a wedged webview
      can't block the quit), then cancels with no rollback, keeping completed files and removing only the in-flight
      partial, inside a hard 2-second budget. Two enabling changes made that budget real: **local copies stage through
      temp+rename** (they used to write to the FINAL name, so a quit mid-copy left a truncated file looking complete — a
      crash- and power-loss hole too), and a **hard-abort tier** races the chunk await against a second token so an SMB
      chunk's 30-second deadline can't hold the quit, with the cooperative cancel path that lets backends clean their
      own partials still the default. Prerequisite (M0) of `operation-session-plan.md`, now satisfied. Ready to wipe
      once someone confirms the colocated docs carry everything.
- [ ] 2026-08-09 `operation-session-plan.md` (refreshed 2026-08-12, re-pinned to `5d75512ab`) - Make the progress
      dialogs looking glasses instead of the process, so a "Foreground" button (click a running row in the operation
      queue, get the rich progress dialog back) becomes buildable. `createTransferProgressState` (1,299 lines) OWNS its
      operation: it scans, dispatches, and only then learns its `operationId`, so "attach to operation X" doesn't exist.
      The fix is an **operation session** keyed by `operationId` plus **views** that bind to one, with zero views a
      legal state. **M0 is done** (its own spec, `quit-and-operation-lifetime.md`): the backend owns operation lifetime
      and the quit decision, so "the operation outlives the view" is finally true. **M1 is now a shipped-bug fix and
      goes first:** a confirmed-but-still-scanning transfer has NO operation record (`scan_preview.rs` never touches
      `manager.rs`), so `canPauseOrQueue` hides Pause and Queue, and `destroy()` kills the scan with the dialog — you
      cannot background a transfer while it scans, and ⌘Q walks past it. Registering the operation at CONFIRM (not at
      dialog-open) and moving the scan-wait into the operation's own task fixes all of it with no new IPC, no new
      `LifecycleStatus` (reuse `Running`; `phase: 'scanning'` already exists and already renders), and ~150 lines
      DELETED from the module M5 has to rewrite. The registry earns its place not because two smoothers disagree (the
      EMA is deterministic; the shipped ETA bug was smoothed-versus-raw) but because smoothers **started at different
      times** diverge, which is exactly what a late-attaching view creates, so M3 must DELETE `operations-store`'s
      per-id smoother map rather than stack a second layer. M6's hardest problem is birth context: `OperationSnapshot`
      carries no `sourcePaths`, counts, or `sourcePaneSide`, yet `handleTransferComplete` purges search snapshots,
      composes the toast, and clears selection against a pane captured at dispatch, so an adopted view must degrade
      honestly (no pane mutation, a toast saying only what the snapshot knows). Also settles the two in-dialog guards
      (`destroy()` and `handleCancel`'s `if (backgrounded) return`, both retiring because "the modal closed" must stop
      meaning "cancel"), cross-window conflict ownership (a single stored oneshot sender makes two windows resolving a
      real lost-take race), the event fan-out as a NEW module both the store and sessions consume, and the pre-identity
      frontend session, now DECLINED with reasons (it mints identity in the wrong place, its payoff is M1's, and it
      does NOT remove the buffer; the fan-out does). SPECCED, not started.
- [x] 2026-08-09 `background-conflict-prompt.md` (built) - A backgrounded transfer that hits a name clash deep inside a
      merging folder used to wedge invisibly: the upfront check is top-level only, folders always merge, and the app's
      only `write-conflict` listener is the progress dialog the Queue button just unmounted, so the operation parked on
      a oneshot nobody could answer. The fix is a main-window host that owns conflict prompts for operations no
      foreground dialog is showing: pause what's running (remembered by id, so a resume can't override a pause the user
      made by hand), raise the same `TransferConflictDialog` through the same `resolveWriteConflict` path, resume on the
      answer. Ownership is one pure function, and the pause width is another, because "pause only this operation" is
      where this is going. Carries the claim-race fix the naive version would have shipped with (a conflict can arrive
      before the start command's response names the operation, so the foreground slot grew a claim counter and the
      controller defers instead of guessing), a FIFO of one prompt at a time, and three exits for an operation that dies
      mid-prompt.
- [x] 2026-08-08 `operation-queue-visibility-plan.md` (built; awaiting David's copy review) - Background file operations
      are invisible, and so are their failures: press Queue, close the queue window, and a running transfer leaves no
      trace in the main window; if it then fails, the reason is gone with the progress modal that Queue unmounted. Four
      parts. **A** moves `queue.show` out of Help into View after "Command palette…" with a ⌥⌘Q default (written `'⌘⌥Q'`
      — the registry spells ⌘ before ⌥, and Apple's display order would be dead on the keyboard), shifting both
      platforms' hardcoded menu indices. **B** renames "Transfer queue" to "Operation queue" in user-facing copy only
      (the window lists deletes, renames, and archive edits too; "transfer" already means copy-or-move in the code; and
      it now pairs with "Operation log" as present versus past), keeping the concrete empty-state prose and every code
      identifier, plus nine locales and two re-captured screenshots. **C** adds a corner progress chip left of the
      indexing hourglass, inside a new `StatusCorner` wrapper, driven by a second `createOperationsStore()` instance in
      the main window (both streams are already app-wide, so no new event, IPC, or polling), with typed visibility gates
      and a count-bar fallback for the same-volume move that moves zero bytes. **D** is the part that needed design and
      the part the brief under-estimates: `LifecycleStatus::Done`/`Cancelled`/`Failed` are **never assigned** —
      `on_settled` deletes the record — so the queue page's `isTerminalStatus` filter is dead code and failures
      disappear because the BACKEND drops them, not the frontend. The fix is bounded out-of-band retention in the
      operation manager (20 rows, typed `error` on the snapshot, explicit dismissal only), which is the only place both
      webviews can read from; the queue window keeps the failed row with its reason through the existing `getMessage()`
      error pipeline, and the main window raises a persistent toast plus a chip failure state. Carries a nine-item
      findings list from reading the code (`write-error` fires twice per op and fires for non-failures; the failure and
      live rows briefly share an id, which would throw in the keyed `{#each}`; `ProgressBar`'s shimmer ignores
      `prefers-reduced-motion` despite a doc claiming otherwise; the OS window title is hardcoded outside the catalog),
      five copy drafts needing David's sign-off, and six flagged risks. BUILT, M1–M11 all landed, including all nine
      locales. What's left is David's pass over the five copy drafts, and one `pnpm i18n:shots` run on an idle machine
      (the new `queue-failed` / `operation-chip` / `operation-failure` capture surfaces are wired but never yet shot).
- [ ] 2026-08-08 `copy-move-safety-hardening-plan.md` - Generalize the three lessons of `7046e9dbb` + `bf6d896b3` (a
      cross-volume copy that streamed directories as files and could recursively delete the user's merged destination
      folder, latent for three months) into types, guards, and checks. **P0 is urgent and goes first**: a claim-by-claim
      review found `MtpVolume::delete` recursing into non-empty directories, which the `Volume::delete` trait contract
      forbids in bold and which four reachable guards depend on (a fifth, rollback's created-dirs prune, leans on it in
      writing but isn't reachable today). The one that loses data is the same-volume move's source cleanup
      (`rename_merge.rs:186-197`), empty-only BY DESIGN because that refusal is the ONLY thing keeping a Skipped child's
      single copy alive — so on a phone, merging a folder onto a same-named one and choosing Skip destroys exactly the
      file the user chose to keep, with no probe error and no race. The fix is nearly free (the MTP code already lists a
      folder's children before recursing, so refusing costs zero USB roundtrips, and every real tree delete already
      walks caller-side and deletes leaf-first) plus a shared conformance assertion every backend runs, rather than an
      opt-in `delete_tree` capability serving zero callers. **P1** makes the preview cache truthful: split `scan.rs`'s
      1,462 lines with no allowlist bump, bind a cached scan to the sources it was asked for (a `preview_id` currently
      authorizes deleting whatever the PREVIEW walked — the LOCAL delete never re-reads its own `sources`, while the
      volume one already does — which is the same unverified-fact shape on the one op with no rollback), make
      `SCAN_PREVIEW_RESULTS` private so the `files > 0 && per_path == 0` canary is load-bearing, name the two cache
      shapes with constructors, remove `Default` from `SourceHint`, and decide each of the eight
      `is_directory(...).unwrap_or(false)` sites — of which `conflict.rs:80` and `rename_merge.rs:333` are destructive,
      and `walker.rs:749`, contrary to an earlier draft, is an accounting-and-honesty defect rather than a loss. **P2**
      splits `cleanup.rs`'s recursive delete by INTENT (`delete_written_file` / `prune_created_dir_if_empty` /
      `remove_tree(why: TreeRemoval)`) so the cleanup path physically cannot recurse, with the prune checking emptiness
      itself rather than trusting the trait. **P3** extracts the no-byte-lost oracle the merge suites already share
      (assertion only: the two fixtures are different trees and unifying them would weaken a policy assertion), teaches
      `InMemoryVolume` to lie about metadata as a first-class fault class, and adds a 3-tier ~39-cell grid plus four
      real-SMB cells and three new Go checks. Findings that reshape the brief: compress, trash, and rename consume no
      preview cache at all (only copy, move, delete do); local copy and local move each re-read `sources` in their own
      way, so one test per pipeline; and the oracle already exists twice. Carries pushbacks with reasoning: the proposed
      `DirectoryCreation::Created` newtype guards the SAFE case and is itself a backend-supplied belief; the literal
      coverage grid is ~360 cells of which most are meaningless because `InMemoryVolume` can't distinguish local from
      SMB from MTP; and `scan_sources_internal` should NOT adopt the per-path helper. One reversal on review: the
      `unwrap_or_default` check IS worth building, method-scoped rather than variable-scoped, because the compiler
      catches none of the hand-written probe unwraps. Reviewed twice, claim by claim, against the code. SPECCED, not
      started.
- [x] 2026-08-06 `i18n-screenshot-coverage.md` - SHIPPED. Translators see a screenshot per string; coverage went from
      **1549 / 2743 keys (56%)** to **2046 / 2743 (75%)**, with direct (precise) captures up from 910 to 1178, and the
      run from 68 surfaces with three dead passes to **133 surfaces, 0 failed**. The lever was driving the capture from
      `DIALOG_GALLERY_ENTRIES` instead of hand-staging each dialog, which needed the gallery's gate widened from
      `import.meta.env.DEV` to `DEV || __CMDR_I18N_CAPTURE__` (the capture binary's frontend is a production Vite
      build). Also added: the transfer-queue window, four Ask Cmdr states, acknowledgements, the pane volume chooser,
      and representative mappings that took `queryUi`, `search`, `updates`, and `viewer` to 100%. The `shortcuts` window
      was never broken; its skip blamed a tauri-playwright eval hang when the window was simply missing from the
      generated `playwright.json` capability. Keep the doc for its gotchas and the remaining gaps (`settings.mediaIndex`
      first).
- [ ] 2026-08-03 `resource-use-plan.md` - PARTLY SHIPPED. Cut idle CPU and RAM: prod v0.37.0 burned 110 min of CPU over
      9.1 h (about 20% of a core, sustained) at a 1.78 GB footprint while idle, writing 141,072 log lines in six hours.
      **The plan's value is mostly in what it got WRONG and how**, so read § M0 before trusting any number in it: four
      successive hypotheses were refuted by measurement, and each refutation is recorded in place rather than edited
      out. The reconcile drain was named the headline cost from LOG VOLUME, then found absent from a CPU profile
      entirely. A 20 s sample then said `index-writer` 45% and `cmdr-sync-status` 42%; a 180 s three-bucket sample said
      sync-status is 3.4% of busy but **0.2% of userspace CPU** (nearly all `stat` wait), and the writer did not appear
      at all. The statement-cache fix was estimated at 10x from a stack profile and measured at **12%**. Raising
      `SCOPED_WALK_MAX_DIRS` was reasoned to be an obvious win and measured as a **regression** (6.02 s scoped versus
      4.9 s full walk for the origin that actually fires). **The real finding, and it is one path**: `origin_dir` is the
      PARENT of the changed file, so any write directly in `~` (in practice `~/.claude.json`, constantly) makes `$HOME`
      an origin, and `$HOME` covers 574,007 of the volume's 694,963 directories (83%). That blows the scoped-walk cap
      and falls back to a full-volume walk: 330 times in a 10.5-hour log, **17.6% of that log's wall clock**. Nothing
      diffed either, so each pass rewrote 51,081 rows of which 99.88% had a byte-identical signals blob. Full evidence:
      `docs/notes/importance-treadmill-2026-08-04.md`. **Shipped**: smb2 0.17.0 (self-healing directory watch, 5,911
      WARNs/6 h gone, plus a latent AES-GMAC CANCEL bug); writer statement caching with the capacity guard (rusqlite's
      cache is 16 entries and the store needs 35, so the obvious fix alone would have thrashed silently); implicit write
      batching (**2.06x**, real writer path, median of three runs each; the 70.2 -> 34.1 us absolutes are DEBUG-build
      numbers and the release path is ~4.6x cheaper, so quote the ratio, never the microseconds — see
      `docs/notes/size-only-subtrees-rejected-2026-08-06.md`); importance passes writing only what moved; and the
      sync-status poll skipping folders with no cloud files. **Open**: the origin bound and demotion, the M3
      arrival-rate governor (unchanged in substance: denylists rejected, must engage `cost_budget.rs:37`, the
      external-volume blind window, the hourglass flicker, and Spike B's over-climb finding), log volume, and the 643 MB
      `MALLOC_LARGE`, which is the primary memory unknown now that page-cache overflow is shown to land only in
      `MALLOC_SMALL` and the search arena is shown to drop correctly.

- [x] 2026-08-04 `unindexed-search-plan.md` - SHIPPED (all eleven milestones on local `main`); doc kept for its
      decisions and its register of accepted indexed-versus-not differences. A search returns the same files indexed or
      not, only slower, on every volume kind (local, SMB, MTP, and whatever comes next), by walking the uncovered part
      live and writing what it finds into the drive index. Made reachable by capping a search at ONE volume: fan-out was
      the only way a search could quietly omit a drive, so removing it deletes machinery (k-way merge, cold-volume
      deferral, re-run-on-ready) rather than adding any, and the MCP tools collapse to a thin wrapper on the same path.
      Three findings shape the mechanism. The descent rule needs BOTH epoch fields: `min_subtree_epoch` alone
      degenerates to "walk everything", since its zero-absorbing min forces zero on every ancestor of any gap.
      Exclusions are a live-walk concern only, because an excluded dir gets no `entries` row at all, so the walk must
      index what the scanner does with `excludeSystemDirs` staying a match-time filter. And the convergence it all rests
      on **does not exist today**: a cancelled `scan_subtree` stamps zero coverage and deletes descendants first, while
      the non-destructive alternative is measured 9-19× slower on the add-everything delta a frontier walk always is, so
      the primitive is chosen by measurement and a cold volume needs real bootstrap work. Walked branches get a watcher
      rather than an expiry, so they stay live like indexed ones. Also fixes why none of it is reachable today: search
      returns before running at all when root's arena isn't loaded. Carries a 13-item register of accepted
      indexed-versus-not differences and a record of everything David settled on 2026-08-04.
- [x] 2026-08-03 `jetbrains-plugin.md` - SHIPPED, and wiped. A private IntelliJ plugin at `tools/intellij-plugin/`
      carrying Cmdr-specific reading aids: commit hashes in a `CHANGELOG.md` entry render link-colored and ⌘-click to
      GitHub, and a message key folds to its English text and ⌘-clicks through to its catalog line. The changelog
      normalization landed with it, taking every commit ref to exactly 8 characters and teaching `changelog-links` to
      fail on any other length. Decisions, measured platform behavior, and the feedback loop live in
      `tools/intellij-plugin/DETAILS.md`.
- [ ] 2026-08-03 `backend-crates-plan.md` - Make "a filesystem backend is its own crate" the shape FTP(S), S3, and SFTP
      get written in, validated first against one mature backend. The `Volume` trait is already the API and already
      lives in `cmdr-fs`, so a crate boundary adds enforcement, not design: `SmbVolume` reaches into the app at 23 sites
      today and nothing stops the 24th. Every reach-through across all four backends clusters into seven host seams
      (listing cache, runtime handle, typed event emit, credentials, index notification, settings, priority and
      analytics), modelled on `crates/cmdr-index/src/indexing/host/`. Design the seams from SMB's 23 sites, then land
      `cmdr-archive` as the pilot (its whole coupling is three seams, no Tauri types, no `cfg(test)` gates, no Docker in
      its tests), ending at a real measurement gate that can cancel `cmdr-smb`. Two honest limits recorded up front:
      **`pnpm check` will not get faster** (every Rust check shares one `rustInputs` set and runs `--workspace`; that
      needs separate per-crate check lanes), and full app builds get ~11% SLOWER after a backend edit, as measured for
      the index. `local_posix` is declared permanently app-resident (it's the git portal's host, 6,402 lines behind it)
      and MTP is out of scope (seven `tauri_specta` derives inside the transport layer, six `cfg(test)` behavior gates,
      and a `pub(in …)` visibility with no cross-crate equivalent).
- [ ] 2026-08-02 `module-cycle-untangling.md` - Cut the two large module dependency cycles in the Rust crates (a
      23-module index-engine SCC and a 17-module `file_system` ↔ `mtp` SCC) plus three small cross-subsystem ones, then
      install a ratcheting `rust-module-cycles` check. The headline finding is that both large components are thin: the
      17 is one `LazyLock` singleton living in a facade that also re-exports downward (moving `get_volume_manager` to
      `volume/manager.rs` takes it 17 → 3), and six import statements take the 23 → a 7 and a 6. `M1` is a hard
      prerequisite for the per-filesystem backend crates, since a crate can't import the app facade. Records four
      `cargo-modules` traps that make raw output untrustworthy (`--acyclic` runs before the filters, `use super::*`
      fabricates edges with no symbol basis, `--no-traits` misses `From` impls, and an inherent-impl method is
      attributed to the module defining its TYPE). Four apparent "tangles" are glob artifacts, not work. Deliberately
      NOT chasing zero: ~16 of 44 groups are idiomatic parent/child. **M0–M6 shipped** (modules in some cycle 184 → 132;
      `cmdr` max component 17 → 10, `cmdr-index` 23 → 6); M7, the ratcheting check, is what's left.
- [ ] 2026-08-01 `smb-transfer-resilience.md` - The cause of the transfer wedge, found on the second repro and now known
      to be ours: `smb2` never spends the credits it charges (the counter only ever grows, nothing gates a send), so
      under concurrency we over-run the server's grant and the NAS stops answering while TCP stays up. Everything else
      was a symptom. M0 makes the over-spend visible (`dispatch:` logs only `ChangeNotify` today, so it has been
      inferred from code, never observed) and is M1's red step; M1 spends, gates, and makes the budget connection-wide;
      M2 adds a session deadline plus **ECHO keepalive**, because the deadline has to sit on "is the session alive" and
      not "has this write finished" or a slow NAS gets aborted; M3 implements the `auto_reconnect` flag that today is
      stored and does nothing, with durable handles; M4 lets Cmdr retry the FILE rather than kill the transfer, and
      revisits the concurrency guess, where measurement replaced it with a defect fix (a LOCAL cap must not bound a
      REMOTE peer) plus skipping the per-file destination probe, rather than the credit-budget replacement M4.3
      proposed. Carries the rule that correct credits are NOT a throughput compromise (lowering concurrency is the only
      option here that actually costs speed) and that a naive credit gate must not turn an over-spend hang into a
      starvation hang. Evidence: `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`. M0-M2, M4.1, M4.3, and M4.4
      shipped; M3 open.

- [x] 2026-07-31 `transfer-wedge-observability.md` - SHIPPED (M1-M6). A 764-file copy to an SMB share wedged after 12
      files, ignored Rollback, and had to be force-quit, leaving two byte-incomplete files at their FINAL names on the
      NAS. **The root cause is still unknown**, so M1 was observability first: a live in-flight table where every task
      records its phase and the driver records its own, a watchdog that dumps the table when bytes stop moving, request
      accounting in `smb2` (its own repo, reaches Cmdr on the next release), logging in `sync_status`, and every exit
      from the cancel path. M2 stages every cross-volume write on a `.cmdr-tmp-*` so an interrupted transfer can't leave
      a truncated file wearing a real name; M3 makes the driver observe intent on its `in_flight.next()` await and
      abandon what won't wind down, so force-quit stops being the only way out; M4 replaces the per-call thread fan-out
      that leaked 21-23 wedged threads with a bounded pool plus a cache (300 thread creations and 3 s of sitting on a
      Dropbox folder became zero and 455 µs); M5 makes a stalled transfer say so instead of showing a confident ETA,
      which needed the watchdog to re-emit the last event because a wedged transfer emits none; M6 unifies
      size/speed/ETA behind one formatter each with a lint. Two of the spec's own premises were wrong and are corrected
      in the doc: the size disagreement needed `get_restricted_window_settings` to carry the field, not just
      `initReactiveSettings()` in every window, and the file counter was honest all along (the gap was in-flight tasks,
      now surfaced rather than "fixed"). Evidence: `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`.
- [x] 2026-07-29 `scoped-incremental-walk.md` - SHIPPED. Make an importance incremental rescore cost O(touched) instead
      of O(dirs): read only the changed subtrees out of the index instead of walking the whole volume (~5.5 s over a
      611,699-folder root index, which is essentially the entire cost of every incremental pass). Rests on separating
      the two whole-tree propagations: `under_floored_ancestor` is exact pure path math over a folder's own prefixes,
      and `has_marker_below` is exact inside a walked subtree, leaving one cross-boundary signal that a stored-vs-fresh
      `has_project_marker` comparison detects exactly, falling back to the full walk when it flips. Carries the
      crossover rule for a wide batch, the accepted lossiness (strict ancestors stop getting their recency term
      refreshed every pass), and the differential-oracle property the test suite pins. Measured median 98–164 µs per
      origin over real 391k- and 611k-folder indexes with zero disagreements against the full walk. Also records a
      pre-existing case-folding gap in the clear that this change neither caused nor fixed.
- [ ] 2026-07-29 `agent-context-harness-plan.md` - Two problems: the human can't check the agent's rename work (review
      is text against text, which is how 12 real files got fabricated names), and the agent loses grounding on a job too
      big for one prompt. Phase A puts the file in front of the reviewer (thumbnail preview, the evidence quote shown in
      context with a coverage figure, editable names, undo after apply) because that's the failure that actually
      happened; later phases harden the prompt, make elision stubs re-fetchable, let the user set the chat's memory
      size, show when history gets set aside, and ground batch jobs in the folder rather than the transcript. Carries
      measured budgets (~3,100 fixed overhead per call, ~39,700 for 100 files), a 12-item invariants register, and both
      shipped weaknesses four review rounds found: the image-tag evidence bypass (FIXED 2026-07-29) and bulk-rename undo
      verifying identity by size alone (OPEN, prerequisite for M3). M9 cut as anti-safety; M11 (per-rule approval for
      large jobs) needs David's policy decision and is a write-engine change, not a spike; M12/M13 deferred behind it.
      SPECCED, not started.
- [ ] 2026-07-28 `flaky-test-eradication.md` - Make a red `rust-tests` run mean a real regression again. MOSTLY SHIPPED
      (2026-07-29): retry-rescued runs now warn instead of passing silently, failures are sorted by which deadline blew
      (nextest cap vs in-test `wait_until`), and a red run re-runs its failures alone before believing them, so
      starvation is told apart from real slowness and from a real defect. Re-measurement under saturation refuted three
      of the spec's original premises (the offenders are CPU-bound pure-logic tests, not watchers; the headline test was
      already on a 20 s cap, not 8 s). All three Rust lanes plus both Playwright lanes now report retry-passes as warns,
      and `rust-integration-tests` gets the contention re-run too. Remaining, needing David's OK: a per-test duration
      budget for the Rust suites, mirroring the 2 s one E2E already enforces (only 16 of ~4,900 tests exceed it today).
- [ ] 2026-07-28 `rename-chaining-arrow-keys.md` - ArrowDown/ArrowUp in the inline rename editor commits the current
      edit and instantly starts renaming the file below/above, so renaming a run of files is one keyboard flow. Fire and
      forget (fast on SMB/MTP), neighbour captured by path BEFORE the rename re-sorts the listing, unusable names and
      conflicts discarded, extension changes committed without a dialog. Carries the data-safety invariants (session
      ids, superseded-session effects, `pendingCursorName` suppression) that keep a save in flight for file N from
      corrupting the editor already on N+1. SPECCED, not started.
- [x] 2026-07-25 `index-crate-extraction-plan.md` - SHIPPED, and wiped. Extracted `indexing/` + `media_index/` +
      `importance/` (93k lines, 28% of `src-tauri/src`) into a Tauri-free `cmdr-index` crate over a `cmdr-fs`
      foundation, with a designed public API: an owned `Index` handle, five named host seams, typed errors, no
      user-facing strings, one cancellation primitive, structured progress, and a designed-not-implemented ingest side.
      The durable intent lives in `crates/cmdr-index/DETAILS.md` (why the crate exists, the eight-point contract it's
      held to, the gated surfaces, what stayed host-side), `crates/cmdr-index/src/indexing/handle/DETAILS.md` (the
      public-surface audit, item by item), `crates/cmdr-index/src/indexing/host/DETAILS.md` (the seams), and
      `crates/cmdr-fs/DETAILS.md` (the compiler-derived closure and the four cuts that made it finite). Two properties
      are machine-checked rather than remembered: `index-crate-isolation` (no `tauri` in either crate's tree, plus a
      ceiling on the public surface) and `desktop-rust-rustdoc` (no broken intra-doc links). Measurements, before and
      after: `docs/notes/index-extraction-baseline.md`.

## Later

Deferred future work. Unchecked by default; the folder name is the status. Each entry notes what shipped and what's
left, so the durable intent survives the wipe.

- [ ] 2026-07-22 `later/swap-scan-plan.md` - Build-and-swap rescan: run the fast parallel guarded walker into a separate
      `index-{vid}.building.db`, then swap it in atomically (~8.4× faster, 107 s vs 897 s), replacing the ~15-minute
      serial in-place reconcile of a completed LOCAL index. Durable `.swap` marker + idempotent open-time recovery
      guarantees exactly one complete index across any crash. NOT STARTED (only the plan + reviews exist; reconcile is
      still the sole rescan path). Foundation: `docs/notes/swap-scan-feasibility.md`,
      `docs/notes/indexing-benchmarks-2026-07-21.md`.
- [ ] 2026-07-22 `later/sealed-subtrees-plan.md` - Bound the cost of pathological high-churn directories without lying
      about folder sizes (motivated by a 7-minute, 1 GB cold-start stall from one 1.14M-file directory). M1 (two-teeth
      child-count guard in post-replay verification) SHIPPED. M2–M5 (seal a subtree to its `dir_stats` aggregate + a
      bounded head of large files, churn-rolled seal root, periodic re-anchoring, a distinct "approximate" size state)
      NOT STARTED and probably never needed: M1 alone may be the whole fix, so M2–M5 stay gated behind measured residual
      pain.
- [ ] 2026-07-21 `later/natural-language-bulk-rename-hardening-handoff.md` - Hardening continuation for the shipped
      natural-language bulk rename. All hardening landed (atomic no-overwrite, dependency-aware execution, live
      conflict/source detection, review warnings, truncation disclosure, plus a follow-up closing local rename/rollback
      safety gaps) EXCEPT finding 5: record both "agent proposed" and "user approved" provenance in the operation log.
      That's the only remaining work.
- [ ] 2026-07-07 `later/archive-browsing-polish.md` - Follow-ups to the shipped archive-browsing feature. SHIPPED:
      one-pass sequential extract, ZipCrypto + WinZip-AES + 7z-AES decrypt end to end, remote-source copy-into, remote
      temp reaping, move-out per-entry convergence, the archive folder split, and SMB push-refresh for remote archives.
      DEFERRED (each with a settled design or trigger): fast tail-add zip edits (clone+tail-rewrite design validated in
      `docs/notes/m-append-spike.md`; the SMB path needs an smb2 copychunk client API), open-with-external for inner
      files (design spiked), and MTP in-place editing (stretch).
- [ ] 2026-07-08 `later/importance-subsystem-plan.md` - Neutral, deterministic folder-importance subsystem (per-volume
      `importance.db`, a minimal lifecycle bus in `indexing/`, an explain call, offline-unmounted reads), exposed as a
      general read API. SHIPPED (M1–M4); durable intent lives in the `importance/` and `indexing/` `CLAUDE.md`/
      `DETAILS.md`. Open follow-ups: weight tuning, and the `kMDItemLastUsedDate` sampling cost.
- [ ] 2026-07-13 `later/media-ml-index-plan.md` - Searchable image index (OCR, tags, faces, text→image) as an ML
      enrichment layer on the drive index: macOS-native (Vision + Core ML + Foundation Models), vectors in SQLite,
      on-device by default. SHIPPED: M1/M1.5/M2 (backend + OCR foundation), M3 (natural-language CLIP semantic search;
      the CLIP path is gated dark until the model artifacts are uploaded), M6 (photo-search agent/MCP tool). PARKED:
      M4a/M4b faces (David wants to be closer in the loop), and M5 LLM captions (optional).
- [ ] 2026-06-28 `later/colorful-tags-plan.md` - macOS Finder tags: read + show colored dots, and context-menu assign.
      SHIPPED (M0–M3); durable intent lives in the colocated `CLAUDE.md`/`DETAILS.md`. Remaining is minor polish only:
      quiet backfill, in-place search-results refresh, a locale native-string pass, and David's visual QA.
- [ ] 2026-03-10 `later/db-first-listings-plan.md` - Serve directory listings from the SQLite index for sub-ms
      navigation.
- [ ] 2026-03-10 `later/dropbox-sync-status-linux.md` - Detect Dropbox sync status on Linux via command socket.
- [ ] 2026-03-10 `later/linux-builds-plan.md` - Add Linux release build target plus website download detection.
- [ ] 2026-05-10 `later/totalcmd-plugin-analysis.md` - Not a spec, but Total Commander packer-plugin research backing
      future archive/plugin work.
- [ ] 2026-05-29 `later/disk-cleanup-advice-process.md` - Not a spec, but reference notes for a future disk-cleanup
      advice feature.
- [ ] 2026-07-18 `later/out-of-process-indexing.md` - Deferred escalation: move drive and media indexing into a separate
      OS process for a hard "can't starve the UI" guarantee. Not needed now (thread QoS + bounded logging closed the
      levers; the resilience fix stopped the source); captures the seams, the clean per-volume-WAL data-safety split,
      the `ai/process.rs` sidecar prior art, and the effort/tradeoffs, with revisit triggers.
- [ ] 2026-06-04 `later/agent-spec.md` - Persistent in-app agent proposing file operations.
- [ ] 2026-06-04 `later/data-dir-rename-spec-draft.md` - Rename data directories from bundle-id to plain names.
- [ ] 2026-06-28 `later/index-vacuum-reader-pinning.md` - Reclaim residual index-DB freelist that long-lived root
      readers stop the incremental vacuum from returning to the OS (deferred: the big freelist sources are now fixed).
- [ ] 2026-06-21 `later/transfer-queue-v2-plan.md` - Transfer queue/pause v2: per-lane budgets (FTP conns),
      mid-large-file pause, concurrent-path pause, connection keep-alive, queue reorder/persist.
- [ ] 2026-06-13 `later/docs-single-source-sweep.md` - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home).
- [ ] 2026-06-28 `later/drive-index-overall-eta.md` - Overall indexing ETA across remaining steps, with the backend
      per-phase calibration it needs to stay honest (the step checklist ships per-step ETA only).
- [ ] 2026-07-14 `later/default-file-manager-spec.md` - Reveal-in-Cmdr (`NSFileViewer` redirect) + `public.folder`
      default handler: two opt-in toggles (default OFF, onboarding step 4 + Settings), `RunEvent::Opened` plumbing with
      cold-start buffering, sanctioned `NSWorkspace` registration, and a spike checklist to run before building.
