# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped, and `DETAILS.md` § "Wiping a shipped spec" for how. Shipped specs get wiped once their
durable intent is captured in colocated `CLAUDE.md`/`DETAILS.md` (and git history); what remains here is unfinished work
plus deferred work under `later/`.

Each spec below states the problem it solves and what finishing it costs. ❌ None of them narrates what already shipped:
that lives beside the code, and git holds the history.

## In progress

- [ ] 2026-08-21 `agent-wake-loop.md` - **Ask Cmdr can suggest things, but never notices anything on its own.** The
      whole proactive half is built (store, executors, dialog, approval bridge, indicator, ten locales, and all of
      `agent/wake/` under 54 tests) and nothing drives it: `run_wake` and `Inbox::admit_if_permitted` have no production
      caller outside `wake/tests/`. Needs a tap adapter and a scheduler. **About a day and a half, fully design-settled,
      blocked on nothing.** The highest ratio of user-visible payoff to effort in the folder.
- [ ] 2026-08-21 `idle-cost.md` - **Cmdr costs too much while you're not using it.** An idle prod build burned 110
      minutes of CPU over 9.1 hours at a 1.78 GB footprint. Two items left, and both wait on a measurement rather than
      on effort: the CLIP towers (an enrichment pass holds 251.5 MB of text tower it never calls, gated on one
      confirming command on David's laptop), and the reconcile drain's unbounded arrival rate (demoted to "read the
      churn line for a week, then choose between four candidate shapes", one of which needs David's call on whether the
      rescan walk may read the shipped `SYSTEM_DIR_EXCLUDES`). **About a day of build work, plus a week of passive
      observation before the second item can be ranked.** Read `docs/notes/idle-cpu-attribution-2026-08-03.md` first:
      four hypotheses here were refuted by measurement.
- [ ] 2026-08-21 `backend-as-a-crate.md` - **S3, FTP(S), SFTP, WebDAV, and NFS are the top planned feature, and there's
      no boundary to write them behind.** The SMB extraction and the module-cycle ratchet both shipped:
      `crates/cmdr-smb` holds the backend and its protocol layer, and `cargo check -p cmdr-smb --all-targets` is a
      complete loop with none of the app in it. **What's left is FTP**, the milestone that proves the seams survive a
      backend that isn't SMB, and it's blocked on one product decision: FTP's concurrency knob (global versus
      per-server, its default, and whether it's exposed at all). Read `crates/cmdr-fs/src/volume/host/DETAILS.md` for
      the seam set and the nine-step recipe, then `crates/cmdr-smb/DETAILS.md` for what a finished extraction looks
      like.
- [ ] 2026-08-21 `indexing-loose-ends.md` - **The coverage machine works; these are the threads left hanging.** Phased
      indexing and claim-based ground ownership both shipped and both left a named tail nobody scheduled: a rename that
      closes one plan outright, the verifier mark with its abandoned-ground trigger, Spotlight recency for a true first
      run, a "watch only these folders" setting, Finder sidebar favorites, and one skipped end-to-end test. **Four days
      if every item is taken**, and they are genuinely independent.
- [ ] 2026-08-21 `open-decisions.md` - **Questions that gate work but aren't work.** Twelve calls waiting on David:
      unreviewed user-facing copy in three features, two product calls that have blocked dependent milestones since
      July, three questions that shipped code has already answered and that just need closing, and two maintenance
      calls. Most take a minute. A question with no answer looks exactly like a task nobody picked up, which is how a
      600-line spec stays alive for a year.

## Later

Deferred future work. Unchecked by default; the folder name is the status. Each entry notes what shipped and what's
left, so the durable intent survives the wipe.

- [ ] 2026-08-23 `later/sftp-follow-ups.md` - **The SFTP backend ships without a way to reach it.** The crate, its IPC
      surface, and the fixtures are done and documented in `crates/cmdr-sftp/DETAILS.md`; four things are open. The
      sidebar has no SFTP arm and path resolution doesn't answer for a remote path, so David's sign-in UI is the next
      build (its whole guide is one section of that `DETAILS.md`). Free space and non-UTF-8 filenames wait on the same
      vendoring of `openssh-sftp-protocol` + `ssh_format`, so they're one job. The auth rung a banner shows is decided
      per dial and nothing refreshes it, which the banner design has to settle first. And two backends still put their
      protocol's wording where `VolumeError::NotFound` promises a path.

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
