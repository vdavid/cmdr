# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped, and `DETAILS.md` § "Wiping a shipped spec" for how. Shipped specs get wiped once their
durable intent is captured in colocated `CLAUDE.md`/`DETAILS.md` (and git history); what remains here is unfinished work
plus deferred work under `later/`.

Each spec below states the problem it solves and what finishing it costs. ❌ None of them narrates what already shipped:
that lives beside the code, and git holds the history.

## In progress

- [ ] 2026-08-21 `indexing-loose-ends.md` - **The coverage machine works; these are the threads left hanging.** Phased
      indexing and claim-based ground ownership both shipped and both left a named tail nobody scheduled: a rename that
      closes one plan outright, the verifier mark with its abandoned-ground trigger, Spotlight recency for a true first
      run, a "watch only these folders" setting, Finder sidebar favorites, and one skipped end-to-end test. **Four days
      if every item is taken**, and they are genuinely independent.
- [ ] 2026-08-21 `open-decisions.md` - **Questions that gate work but aren't work.** Ten calls waiting on David:
      unreviewed user-facing copy in four places, two product calls (one of which has blocked its dependent milestones
      since July), three questions that shipped code has already answered and that just need closing, and one
      maintenance call. Most take a minute. A question with no answer looks exactly like a task nobody picked up, which
      is how a 600-line spec stays alive for a year.

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
- [ ] 2026-08-20 `later/i18n-screenshot-gaps.md` - Translator-screenshot coverage: which catalog families are still
      uncoupled, why each resists capture, and what closing it takes. Stands at **2,186 / 3,112 keys (70%)**: 1,248
      direct plus 938 representative, over 137 captured surfaces. The percentage fell from the shipped plan's 75% only
      because the catalog grew (the translated menu bar alone added 129 permanently-native `menu.*` keys); absolute
      coverage rose. Biggest gaps: `settings.mediaIndex` (the whole panel body behind the image-indexing master toggle),
      `askCmdr` (needs the scripted fake LLM to emit a tool call), `fileExplorer.navigation` (SMB connection and
      favorites failure states), and four cheap settings surfaces the capture never visits. Live per-area numbers always
      come from the generated `apps/desktop/src/lib/intl/messages/screenshots/coverage-report.md`, never this doc.
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
      its own: the `bundle.linux` `.desktop` template. ⚠️ **Building for Linux is not supporting Linux**: the app on
      Linux has a file watcher that never starts, Super-bound menu accelerators, and 504 macOS-specific strings
      (`docs/notes/linux-gaps-2026-08-10.md`), and none of that is in these milestones. The roadmap puts real Linux
      support at "(winter?)".
- [ ] 2026-08-27 `later/totalcmd-plugin-analysis.md` - **The only design artifact for the roadmap's "Add plugins"** (at
      "(fall?)", not started). Not a spec: research plus an argument. Covers all four Total Commander plugin types, not
      just packers, categorizing every plugin in the catalogs A–F. The bottom third is the actionable part and the doc
      now says so up front: which abstraction should own each job, the TC patterns worth inheriting against the
      historical accidents, and 10 questions that shape a plugin API more than format support does. Its recommendations:
      subprocess plus JSON-RPC as primary with WASM as a fast lane, **one capability manifest instead of four plugin
      types**, and a Column-first vertical slice as the first build. The two calls it flags as expensive to get wrong
      are MCP-shaped against bespoke, and manifest against types. ⚠️ **84 KB**, by far the largest doc here; most of it
      is the survey tables that back the investment-priority stats.
- [ ] 2026-08-27 `later/disk-cleanup-advice-process.md` - Not a spec and not unfinished work: **reference notes on how
      to give disk-cleanup advice without losing the user's trust**, from a session where an agent got it wrong three
      times. The heuristic (delete only what is BOTH filesystem-idle by mtime and process-idle by `pgrep`; present
      candidates with signals, never a "safe to delete" bucket) doubles as the judgment model for the roadmap's
      disk-space visualizer. Also carries the Cmdr-against-`du` tooling numbers (~25–30× on a directory level, mtimes
      included). Corrected 2026-08-27: the parameter is `sizeMin`, not `min_size`, and the note's "you can't ask for
      every dir over N GB" limit is refuted by `sortBy: "size"` with `excludeSystemDirs: false`, which is now the
      default path. 💭 Belongs in `docs/notes/` rather than a folder whose contract is unfinished work; left here
      pending David's call.
- [ ] 2026-07-18 `later/indexing/out-of-process-indexing.md` - Deferred escalation: move drive and media indexing into a
      separate OS process for a hard "can't starve the UI" guarantee. Not needed now (thread QoS + bounded logging
      closed the levers; the resilience fix stopped the source); captures the seams, the clean per-volume-WAL
      data-safety split, the `ai/process.rs` sidecar prior art, and the effort/tradeoffs, with revisit triggers.
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
- [ ] 2026-06-28 `later/indexing/index-vacuum-reader-pinning.md` - Reclaim residual index-DB freelist that long-lived
      root readers stop the incremental vacuum from returning to the OS (deferred: the big freelist sources are now
      fixed).
- [ ] 2026-06-21 `later/transfer-queue-v2-plan.md` - Transfer queue/pause v2: per-lane budgets (FTP conns),
      mid-large-file pause, concurrent-path pause, connection keep-alive, queue reorder/persist.
- [ ] 2026-06-28 `later/indexing/drive-index-overall-eta.md` - Overall indexing ETA across remaining steps, with the
      backend per-phase calibration it needs to stay honest (the step checklist ships per-step ETA only).
- [ ] 2026-07-14 `later/default-file-manager-spec.md` - Reveal-in-Cmdr (`NSFileViewer` redirect) + `public.folder`
      default handler: two opt-in toggles (default OFF, onboarding step 4 + Settings), `RunEvent::Opened` plumbing with
      cold-start buffering, sanctioned `NSWorkspace` registration, and a spike checklist to run before building.
