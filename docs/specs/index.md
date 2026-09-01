# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped, and `DETAILS.md` § "Wiping a shipped spec" for how. Shipped specs get wiped once their
durable intent is captured in colocated `CLAUDE.md`/`DETAILS.md` (and git history); what remains here is unfinished work
plus deferred work under `later/`.

Each spec below states the problem it solves and what finishing it costs. ❌ None of them narrates what already shipped:
that lives beside the code, and git holds the history.

## In progress

- [ ] 2026-09-01 `android-adb-backend.md` - **MTP shows the tree a phone chooses to expose; developers want the real
      one.** `crates/cmdr-adb` is a device-anchored `Volume` over the ADB server's sync service and `shell,v2`, beside
      MTP rather than replacing it, and the development adds the seam MTP never had: `device_volumes.rs`, a provider
      registry the volume list folds over, with `host:track-devices` as the first push-channel hotplug. The crate,
      the seam, and the app wiring are documented beside the code (`crates/cmdr-adb/DETAILS.md`, `adb/DETAILS.md`).
      What finishing costs: a real-device pass (authorize prompt, `unauthorized` → `device` mid-session, a 2 GB
      transfer, a `/data` listing on a non-rooted phone), then three deliberate deferrals (`sendrecv_v2` compression
      off until measured, wireless pairing left to the server, a settings switch for the `adb` binary path).

- [ ] 2026-08-29 `unify-rollback-plan.md` - **Cmdr still carries three rollback implementations, and nobody has decided
      whether that stays.** The journal-driven engine (the safest, since it rechecks every item against its recorded
      snapshot before touching it) now runs behind a history-dialog button with progress, pause, and mid-file cancel;
      the two in-memory ones (`CopyTransaction` / `MoveTransaction` and `volume_rollback_with_progress`) still handle
      the in-flight case with no snapshot recheck. What's left is M5, collapsing the three into one, gated on three
      named blockers rather than on cost: the cancel-predicate design (already answered by `StopMeans`), the
      `MoveTransaction` refactor across three call paths, and the `archive_edit/move_out.rs` caller. M4's fourth gap
      (pre-finalize eligibility) is open too, and only serves M5. Both need a go/no-go from David before anyone starts.

- [ ] 2026-08-28 `rename-review-grouping.md` - **One review for one job, not one dialog per batch.** A 500-file bulk
      rename opens five review dialogs at a 60,000-token budget and twenty at the default, because the model can emit
      only ~101 plan rows per reply and each reply is staged and reviewed on its own. The fix is presentational:
      accumulate a job's proposals into one review, apply them as the operations they already are, and leave every
      guardrail per row. ❌ Not the per-rule approval question in `open-decisions.md`, which was answered no. Depends on
      two properties shipped code already has (a proposal never expires, and the dialog renders every row without
      paging), so what remains is frontend and store-shape work with one design choice: open the review on turn end.
- [ ] 2026-08-21 `open-decisions.md` - **Questions that gate work but aren't work.** Seven calls waiting on David:
      unreviewed user-facing copy in four places, two product calls (one of which has blocked its dependent milestones
      since July), and one maintenance call. Most take a minute. A question with no answer looks exactly like a task
      nobody picked up, which is how a 600-line spec stays alive for a year.

## Later

Deferred future work. Unchecked by default; the folder name is the status. Each entry notes what shipped and what's
left, so the durable intent survives the wipe.

- [ ] 2026-08-27 `later/idle-cost-follow-ups.md` - **What the idle-cost effort deliberately left.** Two structural fixes
      shipped (each CLIP tower loads on demand, and an anchor storm costs one visible sweep a day instead of a subtree
      walk each), both documented beside the code. What's open starts with a measurement rather than a fix: every number
      this effort was ranked against came from one v0.37.0 reading on a machine running six cargo builds, and five
      things have changed since, so **a fresh idle baseline on David's laptop comes before ranking anything else**.
      Then: three CLIP calls that memory alone can't settle (the idle unload, now with its 677 ms cold-query price
      measured; the ~400 MB compute-unit trade, blocked on unmeasured enrichment throughput; an fp16 spike), the rescan
      threshold's week of churn data, one question for David about `SYSTEM_DIR_EXCLUDES` that gates nothing, and two
      smaller calls.

- [ ] 2026-08-23 `later/sftp-follow-ups.md` - **The SFTP backend ships without a way to reach it.** The crate, its IPC
      surface, and the fixtures are done and documented in `crates/cmdr-sftp/DETAILS.md`; three things are open. The
      sidebar has no SFTP arm and path resolution doesn't answer for a remote path, so David's sign-in UI is the next
      build (its whole guide is one section of that `DETAILS.md`, and `get_volume_sign_in_state` already answers live
      what a banner should ask for). Free space and non-UTF-8 filenames wait on the same vendoring of
      `openssh-sftp-protocol` + `ssh_format`, so they're one job. And two backends still put their protocol's wording
      where `VolumeError::NotFound` promises a path.

- [ ] 2026-08-23 `later/ai/wake-loop-follow-ups.md` - What the shipped proactive agent deliberately left. Two interest
      tuning knobs and three cadence constants that want a week of real wakes before anyone moves them (the per-outcome
      log line and analytics event exist for exactly that), reading file contents, a thread-timeline event for a
      chat-memory-size change (half a day, unblocked), the rail not refetching on a decision, and one chore needing a
      machine with a foreground: the consent screenshots.
- [ ] 2026-08-27 `later/i18n-screenshot-gaps.md` - **Which catalog families a translator still gets no picture of, and
      why each resists capture.** Structural only: the doc now carries NO absolute numbers, because the ones it used to
      carry went stale twice while the analysis around them stayed true. Every count, percentage, and per-area ranking
      comes from the generated `apps/desktop/src/lib/intl/messages/screenshots/coverage-report.md`. Biggest gaps:
      `settings.mediaIndex` (the whole panel body behind the image-indexing master toggle), `askCmdr` (the rail's fake
      LLM says one thing and calls no tool, so no tool row ever renders), `fileExplorer.navigation` (per-drive index
      status, SMB connection, favorites failures, disk-space retries), and four cheap settings surfaces the capture
      never visits.
- [ ] 2026-08-27 `later/indexing/swap-scan-plan.md` - **A rescan of a completed local index takes ~15 minutes; a fresh
      parallel scan of the same volume takes two.** Build-and-swap closes that gap: run the guarded walker into a
      separate `index-{vid}.building.db`, then promote it atomically (8.4× measured, 107 s vs 897 s), keeping the
      in-place reconcile as the fallback when disk is tight or the flag is off. A durable `.swap` marker plus idempotent
      open-time recovery is what guarantees exactly one complete index across any crash. **NOT STARTED**, re-derived
      from the tree 2026-08-27: no `.building.db`, no marker, no route. Its custody window was rebuilt underneath it
      since it was written (`IndexPhase::Detached` now CLAIMS a teardown rather than refusing it), so § 2.3 step 5 is
      corrected and `docs/notes/manager-custody-spike-2026-08-18.md` § 5 is required reading. Foundation:
      `docs/notes/swap-scan-feasibility.md`, `docs/notes/indexing-benchmarks-2026-07-21.md`.
- [ ] 2026-08-27 `later/indexing/sealed-subtrees-plan.md` - **Bound the cost of pathological high-churn directories
      without lying about folder sizes**, motivated by one 1.14M-file directory causing a 7-minute, 1 GB cold-start
      stall. **M1 (the two-teeth verify guard) SHIPPED**, and its account now lives beside the code, so the milestone
      here is a decision record rather than work. **M2–M5 NOT STARTED and possibly never needed** (seal a subtree to its
      `dir_stats` aggregate plus a bounded head of large files, a churn-rolled seal root, periodic re-anchoring, a
      distinct "approximate" size state): they stay gated behind measured residual pain, and gate 1 is answerable TODAY
      from a SQL census plus two shipped counters, without building an instrument first. All three spikes have run;
      Spike B's result CHANGES Phase B's seal-root rule rather than confirming it (churn share alone selects
      `~/Library/Containers`, which would seal every app's container).
- [ ] 2026-08-27 `later/ai/bulk-rename-follow-ups.md` - **What the reviewed bulk rename deliberately left.** The whole
      hardening wave shipped, provenance included: the operation log records `agent_edited` when the user retyped a name
      in review, and the durable proposal spine binds an approval to the exact ops it covered. Two items are open, both
      waiting on a real report rather than on effort. One invented remote filename still refuses a whole plan (a local
      one reaches review as a blocked row), because proposal construction is synchronous and must never touch a live
      mount. And nothing compares a reply's coverage claim against what the tool actually returned; the prompt contract
      and its test are all there is.
- [ ] 2026-08-27 `later/importance-follow-ups.md` - **What the shipped folder-importance subsystem still owes.** It grew
      well past its plan: five documented area pairs under `crates/cmdr-index/src/importance/`, a scoped incremental
      rescore costing O(touched), a kind-aware multi-volume scheduler, an anonymized real-index eval corpus, and three
      dev bins. Three things are open. The weights are still untuned defaults even though the whole tuning loop is
      built, because real corpus dumps are gitignored and the run needs David's own home directory. The Spotlight
      sampler's cap has never been measured (and is NOT the same work as the shipped first-run recency signal in
      `apps/desktop/src-tauri/src/priority/DETAILS.md`, which seeds a first run before any index exists). And a
      recompute runs under no cancellation token, so `stop_all_indexing` doesn't reach it, which is survivable only
      while a full pass stays seconds.
- [ ] 2026-08-27 `later/indexing/media-ml-index-plan.md` - **Mostly shipped; kept for its decision log and two parked
      milestones.** Searchable image index (OCR, tags, faces, text→image) as an ML enrichment layer on the drive index:
      macOS-native Vision + Core ML, vectors in SQLite, on-device by default. SHIPPED and in users' hands: M1/M1.5/M2
      (backend, OCR, tags, SMB enrichment), M3 (CLIP semantic search, live since v0.36.0 on 2026-07-24 — an earlier
      entry here said it was gated dark pending a model upload, which was false), M6 (photo-search agent/MCP tool).
      PARKED on purpose: M4a/M4b faces (David wants to be closer in the loop) and M5 LLM captions. **The doc survives a
      wipe because ~40 `media_index` code sites cite its § "Key decisions" by bare number.** ⚠️ A bare `plan M<n>` in
      that same code means the WIPED `resource-use-plan.md`, not this plan's milestones.
- [ ] 2026-03-10 `later/db-first-listings-plan.md` - Serve directory listings from the SQLite index instead of
      `readdir` + `stat`. Status re-derived 2026-08-27: the per-navigation verifier SHIPPED (it grew into all of
      `indexing/reconcile/`), as did the logical/physical size split; the DB-first read path is NOT built. The 2026-03
      design was rewritten against the current index, which is id-keyed and per-volume rather than path-keyed, and two
      blockers are now named: the motivating latency number predates release-build measurement, and `created` became a
      sort column the index can't answer.
- [ ] 2026-08-27 `later/dropbox-sync-status-linux.md` - **Cloud badges on Linux, which today are simply absent**: the
      IPC command has a non-macOS arm returning an empty map, and the whole `file_system::sync_status` module is
      macOS-gated. NOT STARTED. The doc holds the research that took the work (Dropbox's `~/.dropbox/command_socket`
      protocol, the four status strings, the `dropbox filestatus` fallback, and why only `Synced` / `Uploading` /
      `Unknown` are reachable without Smart Sync), plus what the module's current six-file shape demands of a Linux arm:
      reuse the cache and the one-batch service, skip the macOS-only thread pool, and build a cheap ancestor gate,
      because Linux has no equivalent of the xattr tier that lets macOS skip nearly every file for 13.9 µs. **A Linux
      arm is a Dropbox arm**; no other provider has a common layer there.
- [ ] 2026-08-27 `later/linux-builds-plan.md` - **A Linux release build and a website that offers it.** NOT STARTED:
      `release.yml` still builds three macOS targets only, and `release.ts` exports only `dmgUrls` / `dmgSizes`. The
      plan covers the CI matrix (x86_64 plus aarch64, native on GitHub's ARM runner), the `latest.json` platform keys
      and `appImageSizes`, and the website's OS detection and platform-aware download card. One item already landed on
      its own: the `bundle.linux` `.desktop` template. ⚠️ **Building for Linux is not supporting Linux**, so the three
      known gaps are now Milestone 0: a file watcher that never starts (the blocker; one unreadable directory aborts the
      whole recursive inotify watch), Super-bound menu accelerators, and 504 macOS-specific strings needing a
      re-translation pass (`docs/notes/linux-gaps-2026-08-10.md`). ❗ Milestone 0 gates the website's download button,
      not the CI build: publishing artifacts a self-builder can find is what the roadmap already promises. The roadmap
      puts real Linux support at "(winter?)".
- [ ] 2026-08-27 `later/indexing/out-of-process-indexing.md` - **The escalation we chose not to take, written down so
      the decision can be re-opened on evidence rather than re-derived.** Moving drive and media indexing into their own
      OS process is the only design that makes "a runaway indexer can never starve the UI" structural instead of
      defended. Not needed now: thread QoS and bounded logging closed the actual levers, and the resilience fix stopped
      the source incident. Captures the seams (the crate extraction turned most of the control-plane work from "design
      it" into "route it"), the clean per-volume-WAL data-safety split, the `ai/process.rs` sidecar prior art, the
      multi-week effort and its new failure modes, and three named revisit triggers.
- [ ] 2026-06-04 `later/ai/agent-spec.md` - Persistent in-app agent proposing file operations. PARTIALLY SHIPPED; status
      re-derived from the tree 2026-08-27 in the spec's §0, which is the first thing to read and wins over any later
      section. Shipped: Ask Cmdr's chat rail, plus the proactive agent in v0.40.0 (the wake pipeline with its coalescer,
      interest scorer, and inbox; the three-level durable proposal spine; agent memory; two `Propose` tools;
      acceptance-rate instrumentation), plus the importance scorer as its own subsystem. **Still designed and unbuilt**:
      the knowledge layer (folder summaries, the importance-gated walk, the preflight) as the largest block, the
      activity log, scoped rules, notification etiquette with the proactivity dial, the bulk model slot, the index
      relocation, prompts-as-assets, the evals harness, and auto-apply. §17 carries that order. The spec's §19 decision
      log is cited by bare number from about 25 code and doc sites, so it stays whether or not the rest does. The wake
      loop's own leftovers are `later/ai/wake-loop-follow-ups.md`, not this file.
- [ ] 2026-08-27 `later/data-dir-rename-spec-draft.md` - **Plain data-directory names instead of bundle-id ones**
      (`~/Library/Application Support/cmdr/`, not `.../com.veszelovszki.cmdr/`). NOT STARTED; still a draft, and the
      value is admittedly small and cosmetic. **The bundle identifier itself must never change** (TCC and the updater's
      designated requirement both key on it), so this is a "stop deriving paths from the identifier" change. Two
      go/no-go questions carry all the risk and want a timebox before any building: can a prod Tauri build's
      `app_data_dir()` be repointed, and can `tauri-plugin-store` follow it. If they fight back, drop the rename. The
      2026-08-27 audit closed three other holes from the code (single-instance enforcement exists, Linux derives via
      XDG, the external-reader inventory is written out). ❗ The index relocation into `~/Library/Caches/` belongs to
      `later/ai/agent-spec.md` § 4.1, not here: this doc owns the directory name, that one owns what goes in it.
- [ ] 2026-08-27 `later/indexing/index-vacuum-reader-pinning.md` - **A long-lived reader pins the index DB's freelist,
      so the incremental vacuum returns almost nothing to the OS during a session** (40 KB shed over five minutes
      against a 1.6 GB freelist: ~135 days at that rate). Deferred, because the shipped reclaim work removed both
      SOURCES of a large freelist, so the acute multi-GB bloat no longer forms; what's left is slow live-event churn.
      Carries the diagnosis, the two candidate fix shapes (release the pinning reader, or a quiesce barrier at an idle
      trigger), why a startup `VACUUM` is the wrong tool, and the diagnosis to run FIRST. **Revisit only with data**
      from a long session that actually bloats.
- [ ] 2026-08-27 `later/indexing/drive-index-overall-eta.md` - **A true overall "~Xm left" across all remaining indexing
      steps, deliberately not built, because an honest one needs per-phase priors that mostly don't exist yet.** Today
      the step checklist shows where you are plus the ACTIVE step's own ETA. Half the calibration has since landed: scan
      duration is persisted per volume and per walk kind and already seeds that ETA. Save, compute, and replay still
      have no priors at all (their timings live only in an app-wide 20-entry debug ring that a restart empties), so the
      remaining work is extending a pattern that exists rather than inventing one.
- [ ] 2026-07-14 `later/default-file-manager-spec.md` - Reveal-in-Cmdr (`NSFileViewer` redirect) + `public.folder`
      default handler: two opt-in toggles (default OFF, onboarding step 4 + Settings), `RunEvent::Opened` plumbing with
      cold-start buffering, sanctioned `NSWorkspace` registration, and a spike checklist to run before building.
      Confirmed 2026-08-27 that NONE of it is built and every codebase anchor it names still holds, except that there is
      no frontend-ready handshake to reuse for the cold-start replay.
- [ ] 2026-08-27 `later/archive-follow-ups.md` - The three things archive browsing still owes, re-derived from the tree:
      a fast tail-add zip edit (design settled in `docs/notes/m-append-spike.md`; the remote half waits on an `smb2`
      copychunk client API, the same one `Volume::copy_within` waits on), open-with-external-app for a file inside an
      archive (shape spiked, one LaunchServices seam unverified), and in-place MTP archive editing (stretch).
- [ ] 2026-08-27 `later/tags-follow-ups.md` - The two Finder-tag gaps still open, re-derived from the tree: the seven
      context-menu color circles show on backends that can't hold a tag, and a tag assigned from search results doesn't
      light up until the next navigation. Both are judgment calls, neither blocks anything, and every design decision
      behind the feature lives in the colocated `DETAILS.md` files.
- [ ] 2026-08-27 `later/transfer-queue-follow-ups.md` - The five transfer-queue extensions still open, re-derived from
      the tree: per-lane budgets above 1 (`LANE_BUDGET` is still a const 1, and the motivating case moved from parked
      FTP to SFTP/SMB), reconnect or keep-alive across a long pause (SMB and SFTP only now), bounding paused-and-parked
      blocking threads, queue reordering, and queue persistence across restarts.
