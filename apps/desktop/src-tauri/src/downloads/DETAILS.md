# Downloads watcher details

Depth and rationale. `CLAUDE.md` holds the must-knows; the lifecycle wiring, scope rationale, and v1 limits live here.

## FDA gating lifecycle

The watcher is alive iff `fda_gate::is_fda_pending_runtime() == false`. `lib.rs` calls `runtime::refresh_runtime(&app)`
at:

1. **Startup**, after `set_fda_pending(...)` runs.
2. **Every main-window `Focused(true)` event**: covers the "I just toggled FDA in System Settings, came back to Cmdr"
   path.
3. **Settings pane mount**: `FileSystemWatchingSection.svelte`'s `onMount` calls `recheck_downloads_watcher_gate` so
   opening the section recovers from a stale focus-event read (granted FDA, came straight to Settings without
   window-focus firing).

`refresh_runtime` aligns the handle with `desired_running(fda_pending)` and is idempotent. Dropping the handle releases
the OS watch; the watcher holds no FDA-protected state beyond that, so the closed-gate side is a pure no-op.

## Cmdr-own-write hook contract

Write operations call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before each filesystem
syscall (and `note_pending_writes_for_cmdr(paths)` for transfer-driver paths that know their destination set up front,
saving N-1 mutex acquires). `note_pending_write_for_cmdr` resolves the watcher via `runtime::with_watcher` and calls
`IgnoreSet::note_pending`, whose prefix check silently no-ops for paths outside the watched Downloads root, so call
sites invoke unconditionally; don't move the filter to the call sites. The end-to-end safety net is
`downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`. Call sites live across
`file_system/write_operations/` (copy, move, delete walker, trash, volume strategy); renames register BOTH halves. See
`file_system/write_operations/DETAILS.md` for the write-side contract.

## Global go-to-latest hotkey

The default global combo is `⌃⌥⌘J` (settings key `behavior.fileSystemWatching.globalGoToLatestShortcut.binding`).
`lib.rs` calls `downloads::refresh_global_go_to_latest_shortcut(app)` at startup (after `set_fda_pending`, alongside the
watcher refresh), every main-window `Focused(true)` event, and on the Settings flip via the
`set_global_go_to_latest_shortcut(enabled, binding)` IPC.

The trigger handler (`global_shortcut::plugin_builder`) raises the main window (`unminimize` + `show` + `set_focus`)
then emits a `global-shortcut-fired` Tauri event on every key-down. The window-raise is load-bearing: the user fires the
hotkey from another app (the whole point is "I'm in Chrome, take me to my download"), so jumping to the file without
foregrounding Cmdr would leave the result hidden. The FE bridge in `lib/downloads/global-shortcut-bridge.svelte.ts`
subscribes and routes through `goToLatestDownload`. The first-trigger warn toast logic lives FE-side, keyed on the
`acknowledged` settings flag.

The plugin uses Carbon's `RegisterEventHotKey` on macOS, so no Accessibility / Input Monitoring TCC grant is needed; the
user sees no extra prompt.

**Gotcha: ⌘ maps to `Super`, not `Meta`, in the accelerator string.** `binding_to_accelerator` (and its FE mirror
`global-shortcut-binding.ts`) translate `⌘` to `Super`. The `global-hotkey` crate's parser accepts `COMMAND` / `CMD` /
`SUPER` for the Cmd key but rejects `META` (it falls through to the key-code parser and errors with `UnsupportedKey`); a
`Meta` mapping makes the default `⌃⌥⌘J` fail to register at startup. Keep both adapters on `Super`.

The `register`/`unregister` state machine in `GlobalShortcutManager` is idempotent: re-registering the same binding is a
no-op, swapping to a new binding unregisters the previous first, and a `Conflict` error stays remembered until the next
successful register so the Settings row can surface "Couldn't register: in use by another app." without re-attempting.
`global_shortcut.rs` carries typed `RegistrationError` (`Conflict | InvalidBinding | PluginError`) and
`RegistrationStatus` (`Registered | NotRegistered | Conflict`); production uses `TauriRegistrar` (owned `AppHandle`),
tests use an in-memory `FakeRegistrar`.

## Browser-style rename scope

v1 scopes to browser-style downloads: a direct create of a final-name file, or a rename from a partial-suffix file
(`.crdownload`, `.part`, `.download`) to a final-name file. CLI tools that write directly to the final name with no
rename signal (curl/wget, `cp` from Terminal, 7-Zip extracting) are out of scope. See
`docs/specs/downloads-watcher-plan.md` § "Latest download definition": no settle delay is added because the rename
signal is reliable for the headline use case and a settle delay adds visible toast latency.
