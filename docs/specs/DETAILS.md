# specs/ details

Read this before adding, wiping, or reorganizing a spec.

- **What lives here**: design docs and plans for big planned work, each linked from the GitHub issue that tracks it and
  listed in § "The specs" below. They don't describe the current state of the codebase; they're working docs that aid a
  development while it's going, kept for reference like ADRs.
- **What doesn't**: open work, follow-ups, and decisions waiting on David. Those are GitHub issues on the "Cmdr backlog"
  project (labels such as `needs-decision`), where priorities get set, one self-contained issue per item. ❌ No
  follow-ups files.
- **Lifecycle**: a spec goes away once its work ships and its durable intent (the why behind decisions, guardrails,
  evidence) lives beside the code. What's still open then becomes an issue. Procedure: § "Wiping a shipped spec".
- **Discipline**: update § "The specs" whenever you add or remove one, so each stays discoverable.

## The specs

- `elevated-file-operations.md`: **A user couldn't move root-owned files out of a folder their macOS user can't change,
  and had to finish with `sudo`.** An out-of-process native alert, a 24-hour Cmdr admin right, and a tiny on-demand root
  helper. Issues: [#107](https://github.com/vdavid/cmdr/issues/107), [#280](https://github.com/vdavid/cmdr/issues/280).
- `error-report-triage-plan.md`: **Auto-sent error reports arrive one by one, and nothing says whether one is fixed,
  known, or new.** Stable signatures, a D1 registry, fix trailers, and one daily digest. Issue:
  [#108](https://github.com/vdavid/cmdr/issues/108).
- `search-arena-snapshot.md`: **Opening search waits ~1 s on every reopen past the idle window, and seconds on a
  session's first open.** Map a journaled columnar arena in place. Issue:
  [#114](https://github.com/vdavid/cmdr/issues/114).
- `swap-scan-plan.md`: **A rescan of a completed local index takes ~15 minutes; a fresh parallel scan takes two.** Build
  a fresh index beside the live one and swap it in atomically. Issues:
  [#242](https://github.com/vdavid/cmdr/issues/242), [#243](https://github.com/vdavid/cmdr/issues/243).
- `db-first-listings-plan.md`: **Serve directory listings from the SQLite index instead of `readdir` + `stat`**, so
  first paint is a query. Blocked on a measurement first. Issues: [#244](https://github.com/vdavid/cmdr/issues/244),
  [#245](https://github.com/vdavid/cmdr/issues/245).
- `data-dir-rename-spec-draft.md`: **Plain data-directory names** (`cmdr/`, not `com.veszelovszki.cmdr/`). Cosmetic and
  low value; a timeboxed go/no-go comes first. Issues: [#282](https://github.com/vdavid/cmdr/issues/282),
  [#283](https://github.com/vdavid/cmdr/issues/283).
- `linux-builds-plan.md`: **A Linux release build (AppImage + .deb) and a website that offers it.** Three known Linux
  gaps gate the download button. Issues: [#151](https://github.com/vdavid/cmdr/issues/151),
  [#284](https://github.com/vdavid/cmdr/issues/284)–[#287](https://github.com/vdavid/cmdr/issues/287).
- `dropbox-sync-status-linux.md`: **Cloud badges on Linux, which today are simply absent.** Holds the Dropbox socket
  protocol research and what a Linux arm needs. Issue: [#288](https://github.com/vdavid/cmdr/issues/288).

## Wiping a shipped spec

The wipe is a one-way door for the working tree, so it runs in this order, one spec at a time:

1. **Re-derive the status from the code and `git log`, never from the spec's own status line.** Statuses lag in BOTH
   directions: specs have read "SPECCED, not started" with every phase on `main`, and "the tap adapter is not built"
   after it was. Cited commit hashes are usually dangling, because a branch gets rebased before the FF merge, so verify
   against `file:line` instead.
2. **Move the durable intent into the colocated `CLAUDE.md` / `DETAILS.md` nearest the code**: design decisions and
   their why, guardrails that stop a regression, measured evidence, accepted tradeoffs and lossiness, edge-case
   registers, and gotchas that cost someone time. A recorded "we considered X and said no, because" counts: it stops the
   next agent re-deriving it.
3. **Let the process die**: milestone checklists, sequencing, parallelization notes, "what I checked", per-phase
   correction lists whose substance already landed in the code, and status narration.
4. **File what's still open as a GitHub issue** rather than keeping the whole spec alive for it: one self-contained
   issue per item, with any decision spelled out for David, and re-derive any numbers it quotes. ❌ Don't keep a
   follow-ups file.
5. **Repoint anything citing the spec for CONTENT** to the issue that now carries it. A bare backticked `docs/specs/…`
   path naming where a decision came from is deliberately exempt from `docs-dead-links` and stays.

⚠️ A spec that says "keep this doc for its gotchas" is describing work to do, not an exemption: rehome the gotchas and
wipe it anyway.
