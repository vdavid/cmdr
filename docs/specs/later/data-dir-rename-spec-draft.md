# Data directory rename: rough spec draft

Status: draft, deliberately rough. 2026-06-04.

**Provenance, read this first.** This draft was written by an AI agent at the end of a design session that was mostly
about a different feature (the in-app agent, see [agent-spec.md](agent-spec.md)). The author's knowledge of Cmdr comes
from `AGENTS.md`, `docs/architecture.md`, `docs/architecture-patterns.md`, and that session; it did NOT freshly
investigate the codebase for this draft. Every claim about the code below should be verified by the agent that picks
this up. Holes are marked explicitly. Treat this as a starting point and an agenda, not a plan.

## 1. Goal

Rename the user-visible data directories from bundle-id names to plain names:

| Today                                                                                       | Target                                |
| ------------------------------------------------------------------------------------------- | ------------------------------------- |
| `~/Library/Application Support/com.veszelovszki.cmdr/`                                      | `~/Library/Application Support/cmdr/` |
| `~/Library/Application Support/com.veszelovszki.cmdr-dev/`                                  | `.../cmdr-dev/`                       |
| `~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>/` (per-worktree)            | `.../cmdr-dev-<slug>/`                |
| `~/Library/Logs/com.veszelovszki.cmdr/`                                                     | `~/Library/Logs/cmdr/`                |
| `~/Library/Caches/com.veszelovszki.cmdr/` (once the drive index moves there per agent-spec) | `~/Library/Caches/cmdr/`              |

Motivation: the `com.veszelovszki` prefix adds no value to the user or the developer; plain `cmdr` is friendlier. This
is an aesthetic and ergonomics change.

Decided in the session: this work is **decoupled** from the agent feature and must not block it. The agent's `main.db`
and the relocated drive index land under the CURRENT names; this rename, if it happens, later moves them along with
everything else (one more migration step to account for).

## 2. The one hard constraint

**The bundle identifier `com.veszelovszki.cmdr` must never change.** macOS keys TCC/Full Disk Access grants to the
bundle identifier plus code signature; the custom in-place updater exists precisely to preserve TCC across updates.
Changing the identifier would reset FDA for every user, and likely disturb Keychain access and signing/notarization
identity. Only the data _paths_ move; the app's identity does not. The rename is therefore a "stop deriving paths from
the identifier" change, not an identifier change.

## 3. What makes this non-trivial

1. **Tauri derives `app_data_dir()` from the identifier.** The app already bypasses this broadly: the dev wrapper
   (`apps/desktop/scripts/tauri-wrapper.js`) resolves `CMDR_INSTANCE_ID`, writes a per-instance `tauri.instance.json` so
   Tauri's own `app_data_dir()` lands on the right path, and exports `CMDR_DATA_DIR` so direct file I/O agrees (see
   `docs/tooling/instance-isolation.md`). Whether this mechanism (or another) can cleanly repoint a PROD build's data
   dir without touching the identifier is **the core go/no-go investigation**. HOLE: the author does not know Tauri's
   current capabilities here.
2. **Plugins write to `app_data_dir()` on their own.** At least `tauri-plugin-store` (settings) and the window-state
   plugin. If they can't be redirected cleanly (config, API, or acceptable fork), the choice is between a split-brain
   layout (some files in the old dir, ugly, defeats the point) and abandoning the rename. This is the second half of the
   go/no-go investigation. HOLE: not verified.
3. **Migration for existing installs.** Rename-on-startup (same volume, near-atomic), with partial-failure handling, and
   possibly a transitional symlink old → new kept for a release or two for external readers. Edge cases to design for: a
   second instance running during migration (HOLE: single-instance enforcement status unknown to the author), a crash
   mid-migration, and Time Machine restores of the old path.
4. **Every external reader of the paths.** Known from docs (verify and complete by grepping): `scripts/mcp-call.sh` and
   agent helpers (read `mcp.port` / `tauri-mcp.port` from the data dir), E2E fixtures and the Linux E2E pipeline, the
   crash reporter (writes `crash-report.json` to the data dir), the error reporter's log-tail bundling, the file-backed
   secret store fallback, the logging dir resolver, and `docs/tooling/instance-isolation.md` itself.
5. **Per-worktree dev instances** (`pnpm dev --worktree <slug>`) must keep their isolation guarantees through the rename
   (ports, data dir, Dock label).
6. **Linux.** The author does not know what Linux currently uses (presumably an XDG path derived from the identifier) or
   whether the rename should apply there at all. HOLE.

## 4. Honest value assessment (from the session)

- The value is real but small: nicer paths for the developer and for power users who look.
- Counterpoint raised and accepted: bundle-id-named dirs are the macOS convention, so Cmdr's own platform-native
  principle argues mildly against the rename. David wants it anyway; fine, but it means the bar for accepting migration
  risk should be low-risk-only.
- Recommendation carried over from the session: **timebox the investigation (items 3.1 and 3.2) first.** If plugins or
  Tauri fight back, drop the rename rather than fight them; the cost is permanent migration code and support burden for
  a cosmetic win.

## 5. Suggested shape of the work (if the investigation says go)

1. Central path resolution: one Rust module owns every app path (data, logs, caches), with the plain-name targets;
   nothing derives paths from the identifier directly anymore. (Much of this may already exist via `CMDR_DATA_DIR`;
   verify.)
2. Plugin redirection for store and window-state to the new dir.
3. Migration-on-startup module with tests: detect old dir, move, leave breadcrumb or symlink, handle partial failure
   idempotently.
4. Update external readers and docs (the §3.4 list, completed by grep).
5. Dev wrapper: new instance naming (`cmdr-dev`, `cmdr-dev-<slug>`); keep `CMDR_INSTANCE_ID` semantics.
6. Release note + a support-facing line about where data now lives.

## 6. Open questions (all of them, since this is a draft)

1. Can a prod Tauri build's `app_data_dir()` be repointed without changing the identifier, and how? (Core go/no-go.)
2. Can `tauri-plugin-store` and window-state be redirected? (Core go/no-go.)
3. What does Linux use today, and does the rename apply there?
4. Is there single-instance enforcement that makes migration-on-startup safe?
5. Full inventory of path references (grep for the bundle id across the repo, scripts, docs, CI).
6. Symlink compatibility window: needed at all, and for how long?
7. Does anything outside the repo (user scripts, third-party tools, support docs) reference the old path in ways worth a
   release-note warning?
8. Sequencing with the agent feature's storage work (agent-spec §4): one combined migration or two separate ones?
