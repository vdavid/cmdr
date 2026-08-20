# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under `later/`.

## In progress

- [ ] 2026-08-18 `agent-suggested-ops-plan.md` - The agent proposes file operations (move, copy, trash, delete, rename,
      compress, extract), the user approves them in GROUPS from a review dialog, and approved groups become ordinary
      queued operations. Ships as one release. **The guiding principle, from David, resolves most design questions
      here**: we do not trust the agent, its suggestions can be formally valid and factually hallucinated, so we lay
      everything out for the user to decide; once they approve, it is exactly as if they started the action, which means
      ❌ NO agent-specific behaviour on the execution path (no auto-skip on collision, no refusing to create a
      destination folder, no refusing an overwrite) and all the effort going into disclosure instead. A GROUP is exactly
      one call to one EXECUTOR (per-verb table; trash binds no volume, rename binds a shared PARENT with per-op
      destinations). Freeze moves from creation to APPROVAL so the agent can re-propose what is pending, and the claim
      transaction compares a hash-plus-count of the live op set against a SERVER-OWNED acceptance record, since
      comparing `proposal_ops` to itself is a tautology. No cap and no expiry: 60k-op groups and rule SELECTORS resolved
      server-side against the drive index and frozen at creation, plus whole-folder ops as a single op. Corrections the
      reviews forced: extract is a COPY, not an archive-edit verb; per-source "done" already ships on
      `OperationEventSink` so the gap is only skip/fail. **IN PROGRESS, and further along than the spec's own build
      status says**: the dialog, the approval bridge, the status-corner indicator, and all nine locales landed, and
      `agent/wake/` is built (coalesce, interest, compact, the `Inbox` with its deadlines, `agent_inbox` migration v6,
      `WakeReadiness`, and `run_wake`, 54 tests). What genuinely remains is the **tap adapter** (mapping the crate-side
      rollup into an `EventBundle` and calling `Inbox::admit_if_permitted`) and the **scheduler** that fires at
      `Inbox::next_deadline`: verified 2026-08-20, `run_wake` and `admit_if_permitted` have no production caller
      anywhere in the tree, only `wake/tests/`. So the pipeline is built and nothing drives it.
- [ ] 2026-08-17 `ground-ownership-plan.md` - Grow `cover::live::Claim` into the single authority for "who may walk this
      ground right now", retiring the parallel mechanisms that answer the same question in their own vocabulary
      (`mgr.scanning`'s arbitration half, `rescan_request::OWED`, the `phases_have_work` guard). Comes from the finding
      that `cmdr-index` is the top bug source (27 of 125 `fix(...)` commits since 2026-06-01, 368 `❌` rules at 4.1 per
      kloc against 0.9-1.3 elsewhere) and that essentially every one of those fixes is a handoff between two of eleven
      actors. An earlier `GroundBroker` draft was killed in review: its headline milestone was impossible (a lease can't
      give the borrow checker permission to take `Box<IndexManager>` off the registry lock), it hung work off `Drop`
      that `cover/mod.rs:338-342` refuses by name, and its perf case cited a figure `branch-set-cost-2026-08-15.md`
      refutes. Adds `Mode::{Exclusive, Additive}` (Decision 13 needs it), fixes `live.rs:69`'s in-call O(n²), and closes
      a latent `start_volume_scan` truncate bug. **M0, M1, M2, M4, and M7 (the product win) are shipped**, M4 with
      product call 4 (a "Rescan now" during a running scan now QUEUES). **M5 was spiked and DROPPED on its merits**
      (`docs/notes/manager-custody-spike-2026-08-18.md`): `Arc<Mutex<IndexManager>>` keeps the same exclusion and pays a
      lock for it, converting a window where no lock is held into one held across blocking I/O. The spike's own finding
      was the valuable part, a fourth stranding hazard the plan never listed (a teardown landing in the extraction
      window is silently swallowed, and the `fail_index` case is a principle #1 exposure: a fatal storage error never
      reaches `Failed` and the volume runs on over a dead writer), and its four-item replacement has since landed too.
      M3 (a rename) and M6 (optional) are all that remain.
- [ ] 2026-08-13 `phased-indexing-plan.md` - Replace the first full drive scan with ordered coverage phases: the user's
      own folders (last session's tabs, favorites, standard home dirs, cloud roots), then whatever they open while the
      app runs, then `$HOME`, then the rest of the drive. Every phase is an `Index::cover` walk, so nothing is ever
      truncated, an interrupted first run survives, and branch-scoped watching becomes the default shape rather than a
      retrofit. The whole drive still gets indexed and every existing promise stays true; the one addition is a
      `home_covered_at` signal so photo search and importance start when home is done instead of waiting for `/`.
      Carries the hourglass fix (corner and per-folder, keyed to ground actually being walked, with a 1 s debounce).
      **M0 through M5 are built**, plus the `Abandoned` cause with its heal; only M6 (follow-ups) is left: the Spotlight
      `kMDItemLastUsedDate` recency signal, the verifier MARK, "watch only these folders" as a user setting, and Finder
      sidebar favorites. The benchmark gate this was written behind has been passed.
- [ ] 2026-08-12 `marketing-screenshot-pipeline-plan.md` - Turn the hand-driven marketing screenshot round (20–30 min of
      MCP calls) into one Playwright command. The shutter stays `screencapture -l` because the plugin's native capture
      has no macOS shadow; the run leaves `CMDR_E2E_MODE` unset so the window can become key and earn the focused
      shadow; and the shard opts out of the fixture guard, which would otherwise delete the real folders being
      photographed. Also seeds a fake Ask Cmdr thread so the chat shot needs no provider. **M1 and M2 are done and
      proven against a live window; M3 is partly done** (the `app-main` pair and `hero-cutouts.json` come out correct at
      2508x1634, with the measured rectangles matching the hand-measured ones), leaving the pinned-tab arrangement, the
      pane paths, hidden files, the index-freshness gate, and the `search` / `chat` / `settings` pairs to stage. **M4
      (seeding the shots data dir) and M5 (docs, retiring the manual path) are not started.**
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
      sync-status poll skipping folders with no cloud files, and the origin bound and demotion (`0271855aa`, which made
      an over-budget origin rescore alone off a single `dir_stats.recursive_dir_count` lookup taken BEFORE anything is
      read, leaving the running descent count as a backstop for a missing or stale row). **Open**: the M3 arrival-rate
      governor (unchanged in substance: denylists rejected, must engage `cost_budget.rs:37`, the external-volume blind
      window, the hourglass flicker, and Spike B's over-climb finding), log volume, and the 643 MB `MALLOC_LARGE`, which
      is the primary memory unknown now that page-cache overflow is shown to land only in `MALLOC_SMALL` and the search
      arena is shown to drop correctly.

- [ ] 2026-08-03 `backend-crates-plan.md` - Make "a filesystem backend is its own crate" the shape FTP(S), S3, and SFTP
      get written in, validated first against one mature backend. The `Volume` trait is already the API and already
      lives in `cmdr-fs`, so a crate boundary adds enforcement, not design: `SmbVolume` reaches into the app at 23 sites
      today and nothing stops the 24th. Every reach-through across all four backends clusters into seven host seams
      (listing cache, runtime handle, typed event emit, credentials, index notification, settings, priority and
      analytics), modelled on `crates/cmdr-index/src/indexing/host/`. Design the seams from SMB's 23 sites, then land
      `cmdr-archive` as the pilot (its whole coupling is three seams, no Tauri types, no `cfg(test)` gates, no Docker in
      its tests), ending at a real measurement gate that can cancel `cmdr-smb`. **The pilot SHIPPED** (`6d435cdf7`):
      `crates/cmdr-archive` exists and is the model a new backend crate copies. What remains is the seam design and the
      SMB extraction itself, where the reach-through has since grown from the 23 sites measured here to 32. Two honest
      limits recorded up front: **`pnpm check` will not get faster** (every Rust check shares one `rustInputs` set and
      runs `--workspace`; that needs separate per-crate check lanes), and full app builds get ~11% SLOWER after a
      backend edit, as measured for the index. `local_posix` is declared permanently app-resident (it's the git portal's
      host, 6,402 lines behind it) and MTP is out of scope (seven `tauri_specta` derives inside the transport layer, six
      `cfg(test)` behavior gates, and a `pub(in …)` visibility with no cross-crate equivalent).
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
      **M1, M2, M3 (plus a per-item skip-reason follow-up), M4, M5, M6, M7, M8, and M10 are all shipped**; M11, M12, and
      M13 are what remain, and M11 needs David's decision before M12 is worth planning. Execution also found two things
      the plan lacked: `files_per_batch` sized a batch from the prompt budget alone (a 60,000-token budget advertised
      145 files while one reply can emit about 101, and past that a plan is cut off mid-JSON and lost), and nothing
      carried the batch size to the model until the turn envelope started rendering it.
- [ ] 2026-07-28 `flaky-test-eradication.md` - Make a red `rust-tests` run mean a real regression again. MOSTLY SHIPPED
      (2026-07-29): retry-rescued runs now warn instead of passing silently, failures are sorted by which deadline blew
      (nextest cap vs in-test `wait_until`), and a red run re-runs its failures alone before believing them, so
      starvation is told apart from real slowness and from a real defect. Re-measurement under saturation refuted three
      of the spec's original premises (the offenders are CPU-bound pure-logic tests, not watchers; the headline test was
      already on a 20 s cap, not 8 s). All three Rust lanes plus both Playwright lanes now report retry-passes as warns,
      and `rust-integration-tests` gets the contention re-run too. Remaining, needing David's OK: a per-test duration
      budget for the Rust suites, mirroring the 2 s one E2E already enforces (only 16 of ~4,900 tests exceed it today).
- [ ] 2026-08-20 `later/i18n-screenshot-gaps.md` - Translator-screenshot coverage: which catalog families are still
      uncoupled, why each resists capture, and what closing it takes. Stands at **2,101 / 2,989 keys (70%)**: 1,200
      direct plus 901 representative, over 132 captured surfaces with none failed. The percentage fell from the shipped
      plan's 75% only because the catalog grew (the translated menu bar alone added 129 permanently-native `menu.*`
      keys); absolute coverage rose. Biggest gaps: `settings.mediaIndex` (80, the whole panel body behind the
      image-indexing master toggle), `askCmdr` (82, needs the scripted fake LLM to emit a tool call),
      `fileExplorer.navigation` (55, SMB connection and favorites failure states), and four cheap settings surfaces the
      capture never visits. Live per-area numbers always come from the generated
      `apps/desktop/src/lib/intl/messages/screenshots/coverage-report.md`, never this doc.
- [ ] 2026-07-22 `later/indexing/swap-scan-plan.md` - Build-and-swap rescan: run the fast parallel guarded walker into a
      separate `index-{vid}.building.db`, then swap it in atomically (~8.4× faster, 107 s vs 897 s), replacing the
      ~15-minute serial in-place reconcile of a completed LOCAL index. Durable `.swap` marker + idempotent open-time
      recovery guarantees exactly one complete index across any crash. NOT STARTED (only the plan + reviews exist;
      reconcile is still the sole rescan path). Foundation: `docs/notes/swap-scan-feasibility.md`,
      `docs/notes/indexing-benchmarks-2026-07-21.md`.
- [ ] 2026-07-22 `later/indexing/sealed-subtrees-plan.md` - Bound the cost of pathological high-churn directories
      without lying about folder sizes (motivated by a 7-minute, 1 GB cold-start stall from one 1.14M-file directory).
      M1 (two-teeth child-count guard in post-replay verification) SHIPPED. M2–M5 (seal a subtree to its `dir_stats`
      aggregate + a bounded head of large files, churn-rolled seal root, periodic re-anchoring, a distinct "approximate"
      size state) NOT STARTED and probably never needed: M1 alone may be the whole fix, so M2–M5 stay gated behind
      measured residual pain.
- [ ] 2026-07-21 `later/ai/natural-language-bulk-rename-hardening-handoff.md` - Hardening continuation for the shipped
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
- [ ] 2026-07-13 `later/indexing/media-ml-index-plan.md` - Searchable image index (OCR, tags, faces, text→image) as an
      ML enrichment layer on the drive index: macOS-native (Vision + Core ML + Foundation Models), vectors in SQLite,
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
- [ ] 2026-07-18 `later/indexing/out-of-process-indexing.md` - Deferred escalation: move drive and media indexing into a
      separate OS process for a hard "can't starve the UI" guarantee. Not needed now (thread QoS + bounded logging
      closed the levers; the resilience fix stopped the source); captures the seams, the clean per-volume-WAL
      data-safety split, the `ai/process.rs` sidecar prior art, and the effort/tradeoffs, with revisit triggers.
- [ ] 2026-06-04 `later/ai/agent-spec.md` - Persistent in-app agent proposing file operations. PARTIALLY SHIPPED; status
      reconciled against the tree 2026-08-18 in the spec's §0. The reactive half shipped as Ask Cmdr (`src/agent/`:
      `main.db` store, the `AgentLlm` seam, consumer-gated tools, the chat runtime, consent, cost meter, one `Propose`
      tool), plus the importance scorer as its own subsystem. The proactive half (durable proposal store, detectors,
      activity log, event pipeline, wake loop, summaries, memory) is not started. §17 is rewritten around what's left
      and inverts the original order: prove the proposal loop with a deterministic detector and measure acceptance
      before spending on the whole-drive summarization pass.
- [ ] 2026-06-04 `later/data-dir-rename-spec-draft.md` - Rename data directories from bundle-id to plain names.
- [ ] 2026-06-28 `later/indexing/index-vacuum-reader-pinning.md` - Reclaim residual index-DB freelist that long-lived
      root readers stop the incremental vacuum from returning to the OS (deferred: the big freelist sources are now
      fixed).
- [ ] 2026-06-21 `later/transfer-queue-v2-plan.md` - Transfer queue/pause v2: per-lane budgets (FTP conns),
      mid-large-file pause, concurrent-path pause, connection keep-alive, queue reorder/persist.
- [ ] 2026-06-13 `later/docs-single-source-sweep.md` - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home).
- [ ] 2026-06-28 `later/indexing/drive-index-overall-eta.md` - Overall indexing ETA across remaining steps, with the
      backend per-phase calibration it needs to stay honest (the step checklist ships per-step ETA only).
- [ ] 2026-07-14 `later/default-file-manager-spec.md` - Reveal-in-Cmdr (`NSFileViewer` redirect) + `public.folder`
      default handler: two opt-in toggles (default OFF, onboarding step 4 + Settings), `RunEvent::Opened` plumbing with
      cold-start buffering, sanctioned `NSWorkspace` registration, and a spike checklist to run before building.
