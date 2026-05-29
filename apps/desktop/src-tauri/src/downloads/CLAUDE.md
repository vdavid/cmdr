# Downloads watcher

Recursive `notify` watch on `~/Downloads` that emits a `download-detected` Tauri event when an
eligible final-form file appears (browser-style: direct create of a final-name file, or rename from
a partial-suffix file to a final-name file). FDA-gated.

## Architecture

- `filter.rs` — `is_eligible(path)`: not hidden, not a partial-download suffix, regular file or
  symlink-to-file.
- `ignore_set.rs` — `IgnoreSet`: `HashMap<PathBuf, Instant>` + paired `VecDeque` (FIFO order),
  5 s TTL by default, 1000-entry cap. Suppresses events for paths Cmdr just wrote.
- `latest_ring.rs` — `LatestRing`: capacity-10 ring of `(PathBuf, Instant)`. Most-recent insert
  wins; re-pushing an existing path moves it to the back without duplication.
- `watcher.rs` — `DownloadsWatcher`: holds a `notify-debouncer-full` handle plus the ignore set
  and ring. Pure `classify_event()` + `translate_debounced()` keep the decision logic testable
  without `notify::Event` fixtures. Production `AppHandleSink` emits over Tauri; tests use an
  mpsc-backed `ChannelSink` so they don't need a running Tauri app.
- `runtime.rs` — `Mutex<Option<DownloadsWatcher>>`. `refresh_runtime(&app)` aligns the handle
  with `desired_running(fda_pending)`. Idempotent.
- `commands.rs` — IPC surface: `reveal_latest_download`, `downloads_watcher_status`,
  `recheck_downloads_watcher_gate`, `set_global_reveal_shortcut`.
- `global_shortcut.rs` — Wrapper around `tauri-plugin-global-shortcut`. Typed `RegistrationError`
  (`Conflict | InvalidBinding | PluginError`) + `RegistrationStatus` (`Registered | NotRegistered |
  Conflict`). The state machine sits in `GlobalShortcutManager<R: Registrar>`; production uses
  `TauriRegistrar` (owned `AppHandle`), tests use an in-memory `FakeRegistrar`.

## FDA gating contract

The watcher is alive iff `fda_gate::is_fda_pending_runtime() == false`. `lib.rs` calls
`runtime::refresh_runtime(&app)` at:

1. **Startup**, after `set_fda_pending(...)` runs.
2. **Every main-window `Focused(true)` event** — covers the "I just toggled FDA in System
   Settings, came back to Cmdr" path.
3. **Settings pane mount** — `FileSystemWatchingSection.svelte`'s `onMount` calls
   `recheck_downloads_watcher_gate` so opening the section recovers from a stale focus-event read
   (the user granted FDA and came straight to Settings without window-focus firing).

Dropping the handle releases the OS watch. The watcher holds no FDA-protected state beyond
that, so the closed-gate side is a pure no-op.

## Cmdr-own-write hook contract

Write operations land in `file_system/write_operations/`. Just before each filesystem syscall,
the operation calls:

```rust
crate::downloads::runtime::with_watcher(|w| {
    w.note_pending_write(dest_path, crate::downloads::DEFAULT_IGNORE_TTL);
});
```

`note_pending_write` silently no-ops for paths outside the watched Downloads root, so call
sites invoke unconditionally — locked in by `IgnoreSet::note_pending`'s prefix check. Don't
move the filter to the call sites.

Key on the **final** path, not the partial: browser rename `foo.zip.crdownload` → `foo.zip`
arrives as `RenameMode::Both` carrying both paths, and the watcher checks both halves against
the ignore set. Cmdr never writes `.crdownload` files; always register the final destination.

The bulk variant `note_pending_writes(paths, ttl)` exists for transfer-driver paths that know
their destination set up front.

## Global reveal hotkey

The default global combo is `⌃⌥⌘J`. `apps/desktop/src-tauri/src/lib.rs` calls
`downloads::refresh_global_reveal_shortcut(app)` at:

1. **Startup**, after `set_fda_pending(...)`, alongside the watcher refresh.
2. **Every main-window `Focused(true)` event** — covers the FDA flip path.
3. **Settings UI flip** via the `set_global_reveal_shortcut(enabled, binding)` IPC command,
   which the FE calls from the Settings row's change handlers.

The trigger handler (`global_shortcut::plugin_builder`) **raises the main window**
(`unminimize` + `show` + `set_focus`) and THEN emits a `global-shortcut-fired` Tauri event on
every key-down. The window-raise is load-bearing: the user fires this hotkey from another app
(the whole point is "I'm in Chrome, take me to my download"), so revealing the file without
foregrounding Cmdr would leave the result hidden behind the active app. Don't drop the raise.
The FE bridge in `lib/downloads/global-shortcut-bridge.svelte.ts` subscribes and routes through
`revealLatestDownload`. The first-trigger warn toast logic lives FE-side, keyed on the
`acknowledged` settings flag.

The plugin uses Carbon's `RegisterEventHotKey` on macOS, so no Accessibility / Input
Monitoring TCC grant is needed; the user sees no extra prompt for the hotkey.

**Gotcha: ⌘ maps to `Super`, not `Meta`, in the accelerator string.** `binding_to_accelerator`
(and its FE mirror `global-shortcut-binding.ts`) translate `⌘` to `Super`. The underlying
`global-hotkey` crate's accelerator parser accepts `COMMAND` / `CMD` / `SUPER` for the Cmd key
but rejects `META` (it falls through to the key-code parser and errors with `UnsupportedKey`).
An earlier `Meta` mapping made the default `⌃⌥⌘J` fail to register at startup with "Invalid
global shortcut binding: Control+Alt+Meta+J". Keep both adapters on `Super`.

The `register/unregister` state machine in `GlobalShortcutManager` is idempotent: re-registering
the same binding is a no-op, swapping to a new binding unregisters the previous one first, and
a `Conflict` error stays remembered until the next successful register so the Settings row can
surface "Couldn't register: in use by another app." without re-attempting.

## Browser-style rename target

v1 scopes to browser-style downloads: a direct create of a final-name file, or a rename from a
partial-suffix file (`.crdownload`, `.part`, `.download`) to a final-name file. CLI tools that
write directly to the final name with no rename signal (curl/wget, `cp` from Terminal, 7-Zip
extracting) are out of scope. See `docs/specs/downloads-watcher-plan.md` § "Latest download
definition" for the rationale (we don't add a settle delay; the rename signal is reliable for
the headline use case and a settle delay adds visible toast latency).

## Gotchas

**Don't `tokio::spawn` from the notify callback.** The `notify-debouncer-full` callback runs on
`notify-rs`'s internal thread with no Tokio runtime context. `tokio::spawn` panics with "there
is no reactor running." All the work (`is_eligible`, ring push, ignore-set check, `app.emit`)
is synchronous and cheap, so we keep it inline. If async work is ever needed here, use
`tauri::async_runtime::spawn` (same pattern the listing watcher uses for its fallback path).

**No `println!` / `eprintln!` / `dbg!`.** Clippy denies these crate-wide (see
`logging/CLAUDE.md`). Use `log::debug!(target: "downloads::watcher", ...)` for per-event lines
and `log::info!` / `log::warn!` for lifecycle / errors. The `target:` prefix lets
`RUST_LOG=cmdr_lib::downloads=debug` filter just this subsystem.

**Tests run against a tempdir, not `~/Downloads`.** `DownloadsWatcher::start_at(path, sink)` is
the test entry point; production code uses `DownloadsWatcher::start(&app)` which resolves
`dirs::download_dir()`. Tests use `unhidden_tempdir()` (a `cmdr-downloads-test-` prefix) so the
`is_eligible` hidden-component check doesn't shadow positive-path assertions on macOS.
