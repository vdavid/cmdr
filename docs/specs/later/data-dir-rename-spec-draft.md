# Data directory rename: rough spec draft

Status: **not started**, still a draft. Nothing has moved: `com.veszelovszki.cmdr` is still the directory name in
`install_id.rs`, `favorites/store.rs`, `analytics/mod.rs`, `priority/roots.rs`, `icons/disk_cache.rs`,
`logging/startup.rs`, `secrets/mod.rs`, and `settings/loader.rs`.

**Provenance, read this first.** This draft was written by an AI agent at the end of a design session mostly about a
different feature (the in-app agent, `docs/specs/later/ai/agent-spec.md`), without a fresh look at the code. A
2026-08-27 audit checked every claim in it against the tree: §3's four mechanisms all hold, and three of the open
questions are answered below from the code rather than guessed. **The two go/no-go questions (§6.1 and §6.2) are still
open**, and they are the whole risk. Treat this as an agenda, not a plan.

## 1. Goal

Rename the user-visible data directories from bundle-id names to plain names:

Each maps a current path to its target:

- **`~/Library/Application Support/com.veszelovszki.cmdr/`**: `~/Library/Application Support/cmdr/`
- **`~/Library/Application Support/com.veszelovszki.cmdr-dev/`**: `.../cmdr-dev/`
- **`~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>/` (per-worktree)**: `.../cmdr-dev-<slug>/`
- **`~/Library/Logs/com.veszelovszki.cmdr/`**: `~/Library/Logs/cmdr/`
- **`~/Library/Caches/com.veszelovszki.cmdr/`**: `~/Library/Caches/cmdr/`. ⚠️ This path doesn't exist yet. The move of
  the drive index into it is owned by `docs/specs/later/ai/agent-spec.md` § 4.1 (which also renames the files to
  `drive-index-{volume_id}.db`), and that item is not started either. **This doc owns the directory NAME, that one owns
  what goes in it**; neither should restate the other.

Motivation: the `com.veszelovszki` prefix adds no value to the user or the developer; plain `cmdr` is friendlier. This
is an aesthetic and ergonomics change.

Decided in the session: this work is **decoupled** from the agent feature and must not block it. The agent's `main.db`
and the relocated drive index land under the CURRENT names; this rename, if it happens, later moves them along with
everything else (one more migration step to account for).

## 2. The one hard constraint

**The bundle identifier `com.veszelovszki.cmdr` must never change.** macOS keys TCC/Full Disk Access grants to the
bundle identifier plus code signature, and the identifier is baked into the updater's designated requirement (see
`apps/desktop/src-tauri/src/updater/DETAILS.md`, which is the canonical statement). Changing it would reset FDA for
every user and break the update path. Only the data _paths_ move; the app's identity does not. The rename is therefore a
"stop deriving paths from the identifier" change, not an identifier change.

(TCC records no path, only the identifier plus signature, so moving the BUNDLE is a separate and already-answered
question: `docs/notes/self-move-to-applications-2026-08-25.md`.)

## 3. What makes this non-trivial

1. **Tauri derives `app_data_dir()` from the identifier.** The app already bypasses this broadly: the dev wrapper
   (`apps/desktop/scripts/tauri-wrapper.ts`) resolves `CMDR_INSTANCE_ID`, writes a per-instance `tauri.instance.json` so
   Tauri's own `app_data_dir()` lands on the right path, and exports `CMDR_DATA_DIR` so direct file I/O agrees (see
   `docs/tooling/instance-isolation.md`). Whether this mechanism (or another) can cleanly repoint a PROD build's data
   dir without touching the identifier is **the core go/no-go investigation**. HOLE: the author does not know Tauri's
   current capabilities here.
2. **Plugins write to `app_data_dir()` on their own.** `tauri-plugin-store` (settings) still does; window state no
   longer does (it's ours, and honors `CMDR_DATA_DIR`). If the remaining ones can't be redirected cleanly (config, API,
   or acceptable fork), the choice is between a split-brain layout (some files in the old dir, ugly, defeats the point)
   and abandoning the rename. This is the second half of the go/no-go investigation. HOLE: not verified.
3. **Migration for existing installs.** Rename-on-startup (same volume, near-atomic), with partial-failure handling, and
   possibly a transitional symlink old → new kept for a release or two for external readers. Edge cases to design for: a
   crash mid-migration, and Time Machine restores of the old path. **A concurrent second instance is not one of them**:
   `apps/desktop/src-tauri/src/instance_lock.rs` holds an advisory whole-file lock on `<data dir>/.instance.lock`, so
   exactly one process owns a data dir. Two consequences for the migration: it runs after the lock is taken, and the
   lock file itself sits INSIDE the directory being moved, so the ordering (acquire, migrate, or migrate, acquire) is a
   real design decision rather than a detail.
4. **Every external reader of the paths.** Verified present, so complete this list by grepping rather than rebuilding
   it: `scripts/mcp-call.sh` and the agent helpers (read `<data dir>/mcp.port`, `<data dir>/mcp.token`, and
   `<data dir>/tauri-mcp.port`), `apps/desktop/test/e2e-shared/port-file.ts` and the Linux E2E pipeline, the crash
   reporter's `crash-report.json`, the error reporter's log-tail bundling, the file-backed secret store fallback
   (`~/.local/share/com.veszelovszki.cmdr/credentials.enc` on Linux), `logging/startup.rs`'s dir resolver, and
   `docs/tooling/instance-isolation.md` and `docs/tooling/mcp.md`, which both print the paths for humans. ⚠️ `mcp.token`
   is a secret written 0o600, so a migration that copies rather than renames has to carry the mode.
5. **Per-worktree dev instances** (`pnpm dev --worktree <slug>`) must keep their isolation guarantees through the rename
   (ports, data dir, Dock label).
6. **Linux.** Answered: Linux derives from the identifier too, via XDG. `dirs::data_dir()/com.veszelovszki.cmdr` for app
   data (`icons/disk_cache.rs`), `~/.local/share/com.veszelovszki.cmdr/credentials.enc` for the secret fallback, and
   `dirs::data_local_dir()/com.veszelovszki.cmdr/logs` for logs. Whether the rename should apply there is still a call
   to make, and it's cheap to defer: Linux isn't advertised (`docs/notes/linux-gaps-2026-08-10.md`), so there's no
   installed base to migrate.

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
2. Plugin redirection for the store to the new dir (window state already follows `CMDR_DATA_DIR`).
3. Migration-on-startup module with tests: detect old dir, move, leave breadcrumb or symlink, handle partial failure
   idempotently.
4. Update external readers and docs (the §3.4 list, completed by grep).
5. Dev wrapper: new instance naming (`cmdr-dev`, `cmdr-dev-<slug>`); keep `CMDR_INSTANCE_ID` semantics.
6. Release note + a support-facing line about where data now lives.

## 6. Open questions (all of them, since this is a draft)

Still open, in the order that matters:

1. Can a prod Tauri build's `app_data_dir()` be repointed without changing the identifier, and how? **Core go/no-go**,
   and the one to timebox first.
2. Can `tauri-plugin-store` be redirected? **Core go/no-go.** Still registered in `lib.rs`
   (`tauri_plugin_store::Builder`), and settings persist through it. Window state is no longer part of this question:
   it's ours (`apps/desktop/src-tauri/src/window_state/`) and resolves through `config::resolved_app_data_dir`.
3. Does the rename apply on Linux at all? (What Linux uses today is answered in §3.6.)
4. Symlink compatibility window: needed at all, and for how long?
5. Does anything outside the repo (user scripts, third-party tools, support docs) reference the old path in ways worth a
   release-note warning?
6. Sequencing with the index relocation (`agent-spec.md` § 4.1): one combined migration or two separate ones? Both are
   unstarted, so either order is still available.

Answered by the 2026-08-27 audit, kept so nobody re-asks: single-instance enforcement exists (§3.3), Linux derives from
the identifier via XDG (§3.6), the external-reader inventory is in §3.4, and window state is already redirected.
