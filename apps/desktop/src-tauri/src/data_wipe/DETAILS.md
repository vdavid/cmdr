# Data wipe: details

Must-knows are in `CLAUDE.md`. This file holds the flow, the decisions, and what the wipe can't reach.

## Flow

1. The dialog opens and calls `data_wipe_summary`: total bytes of every target, the downloaded models' bytes (`*.gguf`
   in `<data>/ai/` plus `<data>/clip-model/`), and whether a license key is activated (`licensing::get_license_info`).
   30 s deadline (the recursive-scan tier); on `TimedOut` the dialog shows no sizes and the license line in its
   "if you activated one" form.
2. Confirm calls `delete_all_app_data`:
   1. Refuse with `FileOperationsRunning` when the quit gate would hold a quit (`quit::blocks_quit`), and with
      `AlreadyRunning` on a second concurrent call.
   2. `quiesce`: cancel live search walks; `index().forget_all_volumes()` (drains each drive-index writer, then deletes
      its database: the ~1 GB part, removed the index's own clean way); `search::drop_all_indices`; stop the local
      LLM server (it holds the model file and `llama-server.log`); stop the MCP server.
   3. `execute::wipe`: delete every secret-store key; `defaults delete <identifier>` (so `cfprefsd` drops its
      in-memory copy rather than writing the plist back); each target through its tombstone; re-list the secret store.
   4. Keep the plan in `WIPED` for the exit pass and return the `WipeReport`.
3. The dialog shows the outcome and offers only "Quit Cmdr", which calls `quit_after_data_wipe`: the quit gate, then
   `app.exit(0)`. On `RunEvent::Exit` the window-state flush is skipped and `on_exit` removes every target again.

## Targets

From `WipeRoots` (`plan::build_plan`), in order, minus unsafe ones and ones nested in another whole target:

- The data dir (settings, histories, `main.db` with Ask Cmdr chats, `ai/` with memory and the local model, the
  importance, media, and operation-log databases, `license.json`, the telemetry queue, crash and error reports, the
  image-search model, MCP token and port files, server lists, SFTP host keys, the file-backed secret store in dev).
- The cache dir (the drive index; bulk-rename Results files in `multi-rename/`; WebKit's and Core ML's caches).
- The log dir (`cmdr.log`), when it isn't inside the data dir.
- macOS `~/Library`: `Preferences/<id>.plist`, `Saved Application State/<id>.savedState`, `WebKit/<id>`,
  `HTTPStorages/<id>`, and `Caches/<id>` / `Logs/<id>` when the cache or log dir lives elsewhere (an E2E run), since
  WebKit still writes there.

## Decisions

- **Decision: rename to a tombstone, then remove.** Why: `remove_dir_all` on a live data dir is a long, non-atomic walk.
  A crash halfway would leave a data dir the next launch reads: a SQLite database without its `-wal` (or a `-wal`
  without its database, which SQLite would try to replay into a fresh file), a settings file pointing at a deleted
  store. The rename is atomic within the parent, so the app sees all or nothing, and the tombstone is inert. Open
  SQLite handles follow the inode into the tombstone and keep working until exit; their data goes with it.
- **Decision: quiesce only the writers that matter, and sweep at exit for the rest.** Why: stopping every subsystem in
  order (media and importance writers, the operation log, the agent store, analytics, watchers) is a shutdown sequence
  the app doesn't have yet, and building one for this path alone would rot. What isn't stopped holds handles into the
  tombstone (harmless) or writes a fresh file by path (removed by `remove_target`'s second look or by `on_exit`).
  Stopped: what holds big files or would rebuild one (the drive index, search walks, the LLM server) and what lets
  something outside drive the app mid-wipe (MCP).
- **Decision: secrets first.** Why: a Keychain item has no folder to fall back on; deleting them before anything else
  means a later crash still leaves no password behind. The file-backed store's `secrets.json` lives in the data dir and
  goes with it either way.
- **Decision: `keys` reads attributes only** (`load_attributes(true)`, no `load_data`), so listing the Keychain never
  raises the consent dialog that reading secret data can.
- **Decision: no cancel once started.** Why: a half-wiped state is worse than either end, and the work takes seconds.
  The confirmation is the cancel point; the dialog can't be closed while it runs.
- **Decision: the app quits after, and doesn't relaunch.** Why: a relaunch would recreate a fresh state at once, which
  reads like the delete didn't happen; quitting makes the outcome obvious.

## What it can't reach

- `cfprefsd` or AppKit may write a few keys back to the plist after `on_exit` (spelling-panel frames, the open panel's
  last folder) if something touches `NSUserDefaults` during teardown. Nothing personal beyond the last folder an open
  panel showed. Unverified in practice: David's QA checks for a plist after a wipe.
- `$TMPDIR/cmdr-update` (the last update's tarball) holds no personal data, and macOS empties `$TMPDIR` on its own.
- Anything on Cmdr's servers (telemetry, crash reports, license records): the privacy policy's erasure process
  (`docs/guides/privacy-requests.md`).
