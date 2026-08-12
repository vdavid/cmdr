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
syscall. `note_pending_write_for_cmdr` resolves the watcher via `runtime::with_watcher` and calls
`IgnoreSet::note_pending`, whose root check silently no-ops for paths outside the watched Downloads root, so call
sites invoke unconditionally; don't move the filter to the call sites.

That check accepts BOTH spellings of the root, the declared one and the symlink-resolved one, and stores every key
against the resolved spelling. It has to: the watcher's events always name canonical paths while a call site names the
path it's about to write, and those differ whenever the root is reached through a symlink (a relocated Downloads
folder, or macOS's `/var` → `/private/var` under `$TMPDIR`). Matching only one spelling means no registration ever
matches an event and Cmdr toasts its own writes. A prefix swap settles it without a `realpath` per registration, which
matters because a large copy registers every file it writes; a symlink DEEPER than the root is not covered, and paying
a syscall per write to cover it isn't worth it. The end-to-end safety net is
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

## Reading a rename on macOS

FSEvents cannot associate a rename's two halves (`fsevent.rs` in `notify` says so outright), so it reports each half as
`Modify(Name(RenameMode::Any))` naming one path. `notify-debouncer-full` tries to pair them by tracker or cached file
ID, and when it succeeds we get the tidy `RenameMode::Both`. When it misses we get the two halves separately, and the
miss is common, not exotic: it happens for a move IN from outside the watched tree (there is no `from` half to pair
with), and whenever the kernel coalesced the partial file's create into its rename, which leaves the old path's file ID
never cached. Measured on macOS 15 (raw-event dump from the partial→final rename test, 2026-08-12): the pairing missed
on every observed run.

So `translate_debounced` maps `RenameMode::Any` to its own `EventSummary::RenameAny`, and `classify_event` treats it
exactly like a `To` half. Nothing probes which half it is, because eligibility already does: `is_eligible` stats the
path, so the vanished `from` side of a move-out drops on its own while the surviving `to` side emits. **Don't "fix" that
by adding an `exists()` check** — it would duplicate the stat and give the two halves two different answers to the same
question.

Dropping `Any` (which is what the code did before) means a download whose rename the debouncer failed to pair produces
no toast at all.

## Browser-style rename scope

v1 scopes to browser-style downloads: a direct create of a final-name file, or a rename from a partial-suffix file
(`.crdownload`, `.part`, `.download`) to a final-name file. CLI tools that write directly to the final name with no
rename signal (curl/wget, `cp` from Terminal, 7-Zip extracting) are out of scope. No settle delay (re-stat after N ms to
confirm the size stabilized) is added: the rename signal is reliable for the headline use case, and a settle delay would
add visible toast latency. Revisit if real-world feedback says CLI downloads matter.

## Latest-download resolution

`go_to_latest_download` resolves the target in two tiers:

- **Primary — the event-driven `LatestRing`** (`watcher.latest_download()`): the most-recent eligible final-form file the
  watcher observed this session. The ring is process-lifetime — it survives across hotkey presses and clears only on
  restart.
- **Fallback — a bounded recursive scan** (`scan_latest`, capped at `SCAN_MAX_DEPTH`, picking the max-mtime eligible
  file), used ONLY when the ring is empty: a fresh launch where the hotkey fires before any download arrived this
  session. Run via `spawn_blocking` so the directory walk never blocks the IPC thread. Both tiers empty → the
  empty-state error (the FE offers to open Downloads anyway).

Both tiers share the same `is_eligible` filter (hidden and partial-suffix files excluded; a regular file or a symlink to
one accepted), so an event-detected "latest" and a scanned "latest" can't disagree.

## Real-FSEvents test determinism

`watcher.rs`'s five integration tests drive a real `notify` watch on a temp dir, so they inherit FSEvents' two
under-load failure modes: a mutation landing in the just-registered-watch arming window is dropped outright (not
delayed), and even a live watch can coalesce or drop a lone create/rename. Both are unrecoverable by waiting, so the
tests **self-heal** rather than wait-and-hope:

- `observe_mutation` primes the watch (`prime_watch` writes throwaway creates until one is observed, proving the stream
  armed), then redoes the real mutation on a fresh name until a matching emit — all inside one 15 s budget so priming
  and the redo never stack a second budget past the 20 s nextest cap. `matches` accepts ANY attempt's emit, so a
  merely-slow event is never discarded as a spurious failure. Used by the create and partial→final-rename tests.
- `note_pending_write_suppresses_matching_event` redoes a registered-write + unregistered-control pair until the control
  emits (proving the watch is live THIS round, so it can't pass vacuously on a watch that never armed), then asserts the
  registered sibling stayed silent. FSEvents' per-stream ordering plus writing the control after the registered file
  means a broken suppression surfaces the registered event first — caught, not masked.

**Two serialization layers, one per test runner.** Concurrent live watches multiply each other's starvation, so only one
runs at a time: under `cargo nextest` (process-per-test) the `real-notify` group in
`.config/nextest.toml` caps the group at one thread; under plain `cargo test` (the
whole `#[cfg(test)]` module shares one process) a `WATCH_SERIAL` mutex the five tests hold for their duration does the
equivalent. The self-heal above is what actually defeats the residual single-watch arming/coalescing; serialization just
removes the mutual interference.

**What the self-heal CANNOT cover: a stream that stops delivering outright.** The redo loops assume the watch is still
alive, so they beat a dropped or coalesced event but not a dead stream. That third failure mode is real on macOS:
`dropping_a_file_emits_one_event` fails at the full 15 s budget on 5 of 12 runs, and
`latest_download_returns_ring_value_after_event` on 6 of 10, both measured on `main` with NO local changes (2026-08-12,
M-series, moderate load). A raw-event dump of a failing run shows the stream delivering the tempdir's own create plus
two priming sentinels, then NOTHING for the remaining 15 s while the test writes a fresh file every second — the events
never reach the debouncer, so no in-test retry can rescue them. Under a full-suite run the same tests fail earlier
still, at `prime_watch`'s "arming never completed".

**Suspect the host before the code.** `fseventsd` is a shared, machine-wide service, so a big local file-churn event
starves every watch on the box: cloning a multi-GB `target/` into a new worktree, a Spotlight reindex, a Time Machine
pass. Both measurements above followed an APFS clone of a 78 GB build tree. Before digging into a watcher, check what
else has been writing, and re-measure on a quiet machine.

**And the daemon's bad state can be PERSISTENT, not just a passing storm.** Measured 2026-08-12 on the M3 Max, machine
fully idle: `fseventsd` at 100% CPU, **26.6 GB RSS on a 64 GB box**, 154 CPU-hours accumulated over 15 days of uptime.
Arming took 4.2 s, then 7.8 s, then 11.0 s across three consecutive runs of the same lone test — climbing, against a 15 s
budget, on a machine doing nothing else. That's the difference between "wait for the Time Machine pass to finish" and
"the daemon needs restarting": `sudo killall fseventsd` (launchd respawns it), or a reboot.

So the check is two questions, in order: is something churning the disk right now, and is `ps aux | grep fseventsd`
showing a daemon that has gone bad? `starvation_message` in `watcher.rs` prints the second one into the failure itself,
along with whether the budget went on ARMING (the stream never came up) or on the redo loop (it came up, then swallowed
everything) — the two have different causes and the old single line couldn't tell them apart.

❌ So don't read a starvation failure here as a watcher regression, and don't "fix" it by lengthening the budget or
adding redo attempts: the loop already gets ~14 tries and none of them are seen. A rescue would have to re-arm the
watch, and whether that belongs in the test or in `DownloadsWatcher` itself is an open question, not a cleanup.
Retries in `.config/nextest.toml` are the other option, and that file's own notes explain why adding them is a policy
decision (they downgrade a run to WARN and are meant to read as a flake-rate meter).
