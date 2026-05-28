# Downloads watcher + reveal-latest-download

Plan for a feature pair that closes the "I just downloaded something, take me there" loop without trying to hijack
Chrome's "Show in Finder" button (we can't — macOS routes `NSWorkspace.activateFileViewerSelecting` to Finder
unconditionally). Instead we own both ends inside Cmdr: a hotkey that jumps to the latest download, and an opt-in
toast/notification when a download lands. Both gated on Full Disk Access (or the per-folder Downloads consent) because
`~/Downloads` is TCC-protected.

This plan captures the **intention** behind each decision. The implementing agent should adapt details when reality
pushes back, as long as the intentions stay intact.

## Why

- Friend reported: "I keep ending up in Finder because Chrome's 'Show in folder' goes there." That's the headline.
- We can't override the OS-level Finder routing. What we _can_ do: be the better destination once the user knows to
  choose us.
- Two surfaces matter: (a) a deliberate "reveal latest" muscle-memory action (the hotkey), (b) a passive nudge when a
  download appears so the user doesn't have to remember (the toast).

## Out of scope

- Hijacking Chrome's "Show in folder" button. Not possible without an extension + native messaging host; explicitly
  deferred.
- Browser extension, URL scheme (`cmdr://reveal?path=...`), and Services menu entry. These are separate, larger pieces;
  revisit later.
- Watching `~/Desktop`, `~/Documents`, etc. Downloads only for v1. The architecture must allow extending later without
  rework, but no new folders ship in this plan.

## High-level shape

1. **Reveal action** ("Reveal latest download")
   - In-app shortcut: `⌘J` (in command registry, customizable, fires only while Cmdr has focus)
   - Optional global hotkey: `⌃⌥⌘J` (system-wide, registered while Cmdr runs; user can disable in Settings)
   - Command palette entry
   - MCP tool: `reveal_latest_download` (optional `index` arg: 0 = latest, 1 = previous, …)
   - Behavior: navigate the focused pane in the current tab to `~/Downloads` (the resolved Downloads dir; see "Downloads
     dir resolution" below), select the latest non-hidden, non-partial file. If no such file exists, INFO toast offering
     to navigate to `~/Downloads` anyway.

2. **Downloads watcher**
   - Recursive `notify` watch on `~/Downloads`
   - Filters out hidden files and partial-download suffixes (`.crdownload`, `.part`, `.download`)
   - Fires on rename-to-final or direct-create-of-final
   - Suppressed when Cmdr itself initiated the write (see "Cmdr-own-write ignore set")
   - Surfaces as in-app toasts, macOS native notifications, both, or neither — user choice via a 4-option ToggleGroup
   - In-app toasts use a new per-toast-group cap with FIFO-in-group eviction (capped at 5 of the same type)
   - All auto-dismissing toasts gain hover-pause + 2-sec re-arm-after-mouse-leave grace (global change to the toast
     component)

3. **Settings**
   - Rename **Settings > Behavior > Drive indexing** to **Settings > Behavior > File system watching** (broader
     umbrella; both indexing and the downloads watcher are file-system watchers).
   - Add **Notify on `~/Downloads` changes** ToggleGroup: `In-app | macOS notifications | Both | Neither`. Default
     `In-app`.
   - Add **Global "Reveal latest download" shortcut** row with the binding picker + on/off (default on, `⌃⌥⌘J`).
   - Both rows grey out with an "FDA required to watch Downloads" hint if the access gate is closed.

4. **Cross-cutting toast infra changes**
   - Hover-pause: while the pointer is over a transient toast, the auto-dismiss timer freezes.
   - 2-sec mouse-leave grace: when the pointer leaves a toast that has already passed its `timeoutMs`, restart a 2-sec
     timer instead of dismissing immediately. This catches accidental cursor exits.
   - `toastGroup?: string` option: when set, the new toast counts against a per-group cap (default 5) before falling
     back to global eviction.

## Design principles in play

- **Platform-native, not generic** ([design-principles.md](../design-principles.md)). All UI strings say "Downloads",
  "System Settings", "Allow", "Full Disk Access" — macOS terminology. The macOS notification copy and the Settings
  rename both follow.
- **Radical transparency.** The first-trigger warning toast on the global hotkey, the FDA-required hint on the Settings
  row, and the "Couldn't register, in use by another app" indicator all surface what's happening rather than failing
  silently.
- **Keyboard-first.** ⌘J and ⌃⌥⌘J both work from the keyboard. The toast itself is fully clickable so mouse-first users
  aren't punished.
- **Respect user resources.** One `notify` watcher on one directory. No polling. No background daemon when Cmdr isn't
  running.
- **Protect user data.** The watcher only _reads_. Cmdr-own-write suppression is a UX feature; we never mute file events
  for actions originating from outside Cmdr.
- **Subscribe, don't poll** ([AGENTS.md](../../AGENTS.md)). The watcher is event-driven. The FDA-state re-check is
  foreground-event-driven, not polling.

---

## Downloads dir resolution

Default: `~/Downloads`. Resolved via `dirs::download_dir()` (cross-platform); if that returns `None`, fall back to
`$HOME/Downloads`. macOS users virtually never customize this. We **don't** add a setting for it in v1; if a real user
asks, we add it later.

## "Latest download" definition

`notify`-driven primary signal:

- The watcher keeps a small in-memory ring (capacity ~10) of `(path, observed_at)` tuples in insertion order. Most
  recent wins.
- The ring survives across hotkey presses; it's cleared only on Cmdr restart.

Fallback when the ring is empty (fresh launch, hotkey pressed before any download arrives):

- Recursive scan of `~/Downloads` excluding hidden and partial-suffix files, pick max mtime. Synchronous; bounded by the
  ring fill check; tens of ms even for many-thousand-file Downloads folders.

Filtering rules (applied identically to event-driven and scan-based selection):

- Hidden files: any path component starting with `.` is excluded.
- Partial suffixes: filename ends in `.crdownload`, `.part`, or `.download` → excluded.
- Directories: excluded (we navigate to the file's parent and select the file).
- Symlinks: included if they resolve to a regular file inside `~/Downloads`.

**Target use case explicitly scoped:** browser-style downloads that finalize via a rename from `partial-suffix` →
`final-name`, or a direct create of a final-name file. Out of scope for v1: CLI tools that write directly to the final
name with no rename signal (curl/wget without `-O foo.part`, `cp` from Terminal, 7-Zip extracting to `~/Downloads`,
etc.). We DO NOT add a settle delay (re-stat after N ms to check the size has stabilized) in v1; the rename signal is
reliable enough for the headline use case, and a settle delay adds latency the user feels in the toast. Document this
limitation in `downloads/CLAUDE.md`; revisit if real-world feedback says CLI downloads matter.

## Cmdr-own-write ignore set

Why: David doesn't want a toast when he just used Cmdr to copy 100 files into `~/Downloads`.

Shape: `DownloadsWatcher` (held as Tauri state) exposes plain Rust functions
`note_pending_write(path: PathBuf, ttl: Duration)` and `note_pending_writes(paths: Vec<PathBuf>, ttl: Duration)`. These
are NOT `#[tauri::command]`s — the hook sites are Rust backend code, no need to round-trip through IPC. Default TTL: 5
seconds. The ignore set is a `HashMap<PathBuf, Instant>` behind a `Mutex`.

**Key on the final path, not the partial.** Browser rename `foo.zip.crdownload` → `foo.zip` arrives as a
`notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both))` event carrying both paths. We check the ignore set
against the `to` path (the final, non-partial name). For Cmdr's own writes, register the final destination — we never
write `.crdownload` files.

**Rename event handling.** When a rename event arrives, check the `to` path against the ignore set (Cmdr-own move
suppresses) AND check whether the `from` path was an ignored entry being moved out (skip both halves). Direct creates
check the path directly.

**Bounded map size.** Lazy TTL expiry on event arrival isn't enough if events never arrive in a session. Add a
1000-entry hard cap with FIFO eviction (oldest insertion drops first) as a safety valve. In normal use the map holds <10
entries; the cap is paranoia.

Hook sites (all must register their intent BEFORE issuing the write so the FS event always lands on a populated ignore
set):

- `file_system/write_operations/transfer/` — copy and move pipelines. The driver knows the destination path set;
  register each just before the per-file write.
- `file_system/write_operations/delete/` — irrelevant for _new_ file detection, but include for completeness so a future
  "deleted from Downloads" event source doesn't surprise us.
- `file_system/write_operations/mkdir/` and `mkfile/` — register on the new path.
- `file_system/inline-rename` and clipboard paste — register on the destination.
- MTP-to-local transfers landing in `~/Downloads` (uncommon but possible) — register on the destination.

**Scoping:** the prefix check lives inside `note_pending` itself, so every hook site can call unconditionally —
`note_pending` silently no-ops for paths outside the resolved Downloads dir. Locked in (don't move the filter to the
call sites).

**Why a hashmap with TTL instead of a counter / refcount:** simpler, no risk of leaking permanent entries if a write
fails mid-flight, and FS events arrive within a few hundred ms of the syscall, so 5 s is plenty of headroom. Lazy
expiry + a hard cap keep the map small.

## FDA gating

Existing infra: `fda_gate::is_fda_pending_runtime()` already gates the indexer. We reuse it.

Lifecycle:

1. **During onboarding's FDA step:** we don't touch the gate. Onboarding owns it; we piggyback on the existing
   `set_fda_pending(false)` call when the user decides.
2. **App startup (post-onboarding):** if `is_fda_pending_runtime()` is `false`, start the watcher and register the
   global hotkey (if enabled in settings). Otherwise leave both dormant.
3. **Window-focus event:** re-check the gate. If state changed (user granted FDA in System Settings → returned to Cmdr,
   or user revoked FDA → returned to Cmdr), start or stop the watcher and global hotkey accordingly. Use the existing
   `tauri::WindowEvent::Focused(true)` listener; if there's no such listener today, add one in
   `apps/desktop/src-tauri/src/lib.rs`.
4. **No periodic polling.** Foreground events cover every realistic transition.

UX when FDA is missing:

- Settings rows greyed with an in-line hint that links to System Settings (we already have helpers for this; see how the
  indexing row handles it today).
- Global hotkey is simply not registered, so pressing `⌃⌥⌘J` does nothing — no dead-key toast. This is intentional:
  registering a hotkey we can't act on is worse than no hotkey.
- In-app `⌘J` IS still registered (always), but on trigger it checks the gate; if closed, it shows a single INFO toast:
  "Cmdr needs Full Disk Access to watch your Downloads folder. [Open System Settings]" with a dedup id so spamming `⌘J`
  doesn't stack toasts.

**Belt-and-braces re-check:** in addition to window-focus events, also re-check `is_fda_pending_runtime()` whenever the
user opens any pane in **Settings > Behavior > File system watching** (mount-time hook in the section component). Cheap,
user-driven, covers the "I just granted FDA, came back to Cmdr, opened Settings to verify" path where the focus event
may have already fired on a stale gate read.

## In-app shortcut: ⌘J

- Command registry: add
  `{ id: 'downloads.revealLatest', name: 'Reveal latest download', scope: 'Main window', shortcuts: ['⌘J'], showInPalette: true, description: 'Open ~/Downloads and select the most recent file.' }`
  in `lib/commands/command-registry.ts`.
- Dispatch: add a case in `routes/(main)/command-dispatch.ts` that calls a `revealLatestDownload()` IPC.
- Reasoning for `⌘J`: short, easy, related to "Jump." Chrome's Downloads-tab shortcut is `⌘⇧J`, so we don't collide.
  Other apps use `⌘J` for "Jump to selection" etc.; that's in-app only and stays untouched because our `⌘J` is also
  in-app only and only fires when Cmdr has focus.
- **Known Finder-parity precedent:** in Finder, `⌘J` shows "View Options." Cmdr is a file manager, so users migrating
  from Finder may reach for `⌘J` expecting per-pane appearance controls. **Decision (user-confirmed):** we accept the
  deviation. Rationale: Cmdr's view-mode switching has dedicated single-key shortcuts (the inline view-mode toggle and
  the appearance settings under `⌘,`), so Finder migrants pick up new muscle memory quickly. We're not displacing an
  existing Cmdr action — we're choosing not to mirror Finder for this one binding. Document the decision in
  `lib/shortcuts/CLAUDE.md` so the next agent doesn't try to "fix" it.

## Global hotkey: ⌃⌥⌘J

- Crate: `tauri-plugin-global-shortcut` (Tauri 2 plugin). Pin to the latest version that's ≥14 days old at
  implementation time (check crates.io; don't trust training data).
- **macOS permission scope:** no Accessibility or Input Monitoring grant needed. `tauri-plugin-global-shortcut` on macOS
  uses Carbon's `RegisterEventHotKey` (a system-API event hook in-process), distinct from key-logging APIs that require
  TCC grants. The user sees no extra prompt.
- Capability: add `global-shortcut:default` to the relevant capability file in `src-tauri/capabilities/`.
- Default binding: `⌃⌥⌘J` (Ctrl + Option + Cmd + J). Yes, finger-breaker — that's the point. Three-modifier combos
  collide with almost nothing.
- Default state: ON. The first time it triggers, we surface it via a non-auto-dismissing WARN toast (see below).
- Customization: a row in **Settings > Behavior > File system watching** with an on/off toggle and a key-recorder.
  Reusing the existing shortcut UI bits where possible; if the existing recorder doesn't support global shortcuts
  (system-modifier capture quirks), we accept a constrained recorder for v1 and link to docs.
- Collision: if registration fails (another app holds the combo), surface a small persistent indicator in the Settings
  row: "Couldn't register: in use by another app." No noisy toast. The user can pick a different combo.
- First-trigger warn toast (only fires when (a) the hotkey was triggered AND (b) the user hasn't explicitly toggled the
  setting yet — i.e., the `acknowledged` bit is unset):
  - Level: `warn`
  - Dismissal: `persistent`
  - Copy: "The ⌃⌥⌘J shortcut jumps to your latest download from anywhere. Keep it on?"
  - Buttons: "Keep it on" (primary) and "Turn it off" (secondary). Both set the `acknowledged` bit so the toast never
    surfaces a second time. "Turn it off" also flips the `enabled` setting to false (in-app `⌘J` stays registered
    regardless).
  - Style-guide rationale: friendly, active voice, casual, no permissive language. Buttons are verb phrases.
    Screen-reader-friendly: copy starts with "The", not the symbol.
  - **Copy idiom note:** the global-hotkey warn-toast uses "Turn it off" (the hotkey is currently ON; the action turns
    it off). The downloads toast uses "Stop showing these" (a stream of notifications, not a switch). Two phrasings for
    two different mental models, intentional.

## Downloads toast

Component: `apps/desktop/src/lib/downloads/DownloadToastContent.svelte`. Wired into the toast store from a backend
`download-detected` Tauri event listener mounted once in the main layout.

Visual contract:

- Title row: "Downloaded `foo.zip`" (filename in monospace via existing toast styles or backticks rendered through
  `Size`-style colorization; if the file size is known cheaply via the FS event, append a colored size badge).
- Body: optional second line "in Downloads/Chrome/" if the file is in a subdirectory under `~/Downloads`.
- Shortcut hint: small tertiary line showing the `⌘J` binding **snapshotted at toast creation time**. Pass it as a prop.
  Rationale: a toast born showing `⌘J` shouldn't mutate mid-flight if the user remaps to `⌘K` while the toast is visible
  — the visible hint should match the binding the user could have pressed when this specific toast appeared. The next
  toast picks up the new value. Also makes the component pure-prop-driven and trivially testable.
- Two visible actions: "Jump to file" (primary) and "Stop showing these" (secondary, link-styled).
- Whole toast is clickable: click anywhere on the toast (outside the explicit buttons) triggers Jump.
- "Stop showing these" deep-links to **Settings > Behavior > File system watching > Notify on ~/Downloads changes** and
  sets the toggle to "Neither". Active voice, casual, friendly per style guide.
- `toastGroup: 'downloads'`, cap 5 visible.
- Dismissal: `transient`, default 10000 ms (longer than the global 4000 ms default; chosen because the user is in
  Chrome, not Cmdr, so a 4-sec window is too tight). Subject to the global hover-pause + 2-sec grace logic.

## macOS native notification

Crate: `tauri-plugin-notification` (Tauri 2 official plugin). Pin ≥14-day-old version. Capability:
`notification:default`.

Behavior:

- Title: "Downloaded `foo.zip`"
- Body: relative path under Downloads if it's in a subdir, else empty.
- Primary action: "Reveal" — triggers the same code path as the in-app `⌘J`.
- Clicking the notification body itself also triggers Reveal (default action).
- First-time the user picks "macOS notifications" or "Both" in Settings, we trigger the OS permission prompt by calling
  `requestPermission()`. The macOS dialog wording is fixed by Apple — we can't customize it; the user already knows what
  they signed up for because they just picked that setting.
- No proactive priming, no onboarding step.

## Toast infra changes (cross-cutting)

These are foundational for the downloads toast but apply to ALL toasts. Files touched:
`apps/desktop/src/lib/ui/toast/toast-store.svelte.ts`, `ToastItem.svelte`, `ToastContainer.svelte`.

1. **Hover-pause + 2-sec grace.** The pause behavior lives in `ToastItem.svelte` (it owns the timer today). On
   `pointerenter`, clear the timer and record `pausedAt`. On `pointerleave`, if `Date.now() - createdAt < timeoutMs`
   (i.e., haven't reached natural expiry yet), restart with the _remaining_ time. If already past expiry, start a 2000
   ms grace timer (a const, `HOVER_LEAVE_GRACE_MS`, exported from the toast module for any future tuning).
2. **`toastGroup` option.** Add `toastGroup?: string` to `ToastOptions` and a `toastGroup?: string` field to `Toast`.
   Also add `maxInGroup?: number` (default 5). Eviction order in `makeRoomForNewToast()`:
   - If the new toast has a `toastGroup`, first try to evict the oldest _transient_ toast in that same group when the
     group is at its `maxInGroup`. Group eviction can happen even if global cap isn't hit.
   - Then apply the existing global cap logic.
   - Persistent toasts in the group block group-level eviction (just like they block global eviction today).
3. **Component-catalog entry.** Update the dev-only `routes/dev/components/sections/Toast.svelte` to show the new
   behaviors (a hover-pause example, a grouped-toast burst).

## MCP tool

New tool: `reveal_latest_download`. No arguments in v1. Returns: the absolute path that was revealed, or an error if no
eligible file exists.

Ack: `GenerationAdvanced` (standard nav ack pattern). Wire through `mcp/executor/`. Register schema in
`mcp/protocol.rs`. Update `mcp/CLAUDE.md` if needed.

**Future:** an optional `index` arg (0 = latest, 1 = previous, …) could be added once the scan fallback returns a sorted
list (it currently returns max-mtime only, so `index > 0` would behave inconsistently between the ring and the
fallback). Deliberately deferred.

This lets agents (including David's own scripts) reveal downloads programmatically. Cheap to add given the IPC already
exists.

## Settings shape

`apps/desktop/src/lib/settings/` registry entries:

- `behavior.fileSystemWatching.downloadsNotifications`: enum `'in-app' | 'macos' | 'both' | 'neither'`, default
  `'in-app'`. Rendered via `SettingToggleGroup`.
- `behavior.fileSystemWatching.globalRevealShortcut.enabled`: bool, default `true`.
- `behavior.fileSystemWatching.globalRevealShortcut.binding`: string (key combo), default `'⌃⌥⌘J'`.
- `behavior.fileSystemWatching.globalRevealShortcut.acknowledged`: bool, default `false`. Internal; controls the
  first-trigger warn-toast suppression. NOT shown in the Settings UI. **Reset on binding change:** if the user changes
  `binding`, reset `acknowledged` to `false` — the new combo is effectively a fresh hotkey and deserves the
  first-trigger warning again.

Section file rename: `DriveIndexingSection.svelte` → `FileSystemWatchingSection.svelte`. The Drive-indexing controls
stay inside it as a top sub-group; new sub-groups: "Downloads notifications" and "Global reveal shortcut". Use
`SectionCard` to visually group within the page if appropriate.

Update settings registry, ToggleGroup labels, and any deep-link IDs accordingly.

## Documentation updates

- `apps/desktop/src/lib/ui/CLAUDE.md` — update the toast section to document hover-pause, 2-sec grace, `toastGroup`.
- `apps/desktop/src/lib/downloads/CLAUDE.md` — NEW. Architecture of the watcher + reveal command + toast wiring on the
  frontend.
- `apps/desktop/src-tauri/src/downloads/CLAUDE.md` — NEW. Backend watcher, ignore-set, FDA gating, event emission.
- `apps/desktop/src-tauri/src/mcp/CLAUDE.md` — add `reveal_latest_download` to the tool inventory.
- `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` — document the `note_pending_write` hook contract.
- `docs/architecture.md` — add `downloads/` to both frontend and backend tables.
- `apps/desktop/src/lib/settings/sections/CLAUDE.md` — note the rename + new sub-groups.

---

## Milestones

Sequential by default. Notes call out the few places where parallelism is genuinely safe.

### M0 — FSEvents-under-TCC spike (BLOCKER)

Pre-flight de-risking. The whole watcher pipeline hinges on `notify`/FSEvents delivering events for `~/Downloads`. We
don't know for sure whether the per-folder Downloads TCC grant (the popup macOS shows on first read attempt) is enough,
or whether full FDA is required.

**Scope:**

- Write a 20-line standalone Rust probe at `apps/desktop/src-tauri/src/downloads/probe.rs` (or a separate `cargo`
  example): `notify::recommended_watcher` on `~/Downloads`, print events, run for 30 seconds.
- David runs it manually under both TCC states (full FDA granted, per-folder Downloads granted) and reports findings.
- Update plan's "Risks and open questions" risk 1 with the result.

**Done when:** we know whether the watcher only works under full FDA, or also under per-folder Downloads consent. The
answer shapes the FDA-gating UX (the Settings hint copy changes if per-folder works).

### M1 — Toast infra updates (hover-pause + group cap)

Foundational. Touches `lib/ui/toast/`. No feature behavior depends on this YET, but the downloads toast does. Land it
first so the downstream milestones can use it. Can run in parallel with M2a (different languages, different files).

**Scope:**

- Add `toastGroup` and `maxInGroup` to `ToastOptions` / `Toast` / store.
- Implement group-aware eviction in `makeRoomForNewToast()`.
- Add hover-pause + 2-sec mouse-leave grace in `ToastItem.svelte`.
- Update `lib/ui/CLAUDE.md` toast section.
- Update component catalog (`routes/dev/components/sections/Toast.svelte`).

**Tests (TDD where possible):**

- Vitest unit tests for the store: existing tests should keep passing; add tests for `toastGroup` eviction (5 of group
  A, 1 of group B; new group-A toast evicts oldest A, not B), `maxInGroup` cap, group + persistent interactions.
- Vitest behavior test for `ToastItem`: hover pauses, leave resumes, grace timer fires after expiry-while-hovered.
- Tier-3 a11y test for the new states (axe-core).
- Existing `crash-reporter` and `error-reporter` toast tests must keep passing — they're the regression net.

**Done when:** `./scripts/check.sh --fast` green, all toast tests green, component catalog renders the new examples.

### M2a — Pure-Rust modules: filter, ignore_set, latest_ring

The pure-logic Rust modules. No Tauri integration, no `notify` — just data structures + filters + ring + ignore-set.
Independently testable with unit tests. Can run in parallel with M1 (different language, different files).

**Scope:**

- New module `apps/desktop/src-tauri/src/downloads/`:
  - `mod.rs` — public re-exports
  - `filter.rs` — `is_eligible(path: &Path) -> bool` for hidden / partial-suffix / directory / symlink checks
  - `ignore_set.rs` — `IgnoreSet { state: Mutex<IgnoreSetState>, max_entries: usize }` where
    `IgnoreSetState { map: HashMap<PathBuf, Instant>, order: VecDeque<PathBuf> }`. Plain Rust API:
    `note_pending(path, ttl)`, `note_pending_batch(paths, ttl)`, `is_pending(path) -> bool` (lazy expiry on check),
    `len()` for tests. The paired `VecDeque` preserves insertion order so the FIFO cap actually evicts the oldest entry;
    `HashMap` keeps the O(1) `is_pending` check. **Scoping:** `note_pending` ignores paths NOT under the resolved
    Downloads dir — filter inside the function so every hook site can call unconditionally. 1000-entry FIFO cap.
  - `latest_ring.rs` — capacity-10 ring of `(PathBuf, Instant)`; `push`, `latest() -> Option<&Path>`, `clear`.

**Tests (TDD: write tests FIRST, then implement):**

- `filter`: exhaustive table-driven tests on hidden / partial-suffix / dir / final-name / symlink combos.
- `ignore_set`: insert / expire / multi-path / TTL boundary; FIFO eviction at cap; concurrent insert+check under
  `Mutex`.
- `latest_ring`: fill / overflow drops oldest / `latest()` returns most-recent / `clear()`.

**Done when:** `./scripts/check.sh --check clippy` and `--check rust-tests-fast` (or equivalent fast lane) green for the
new files. No `notify` or Tauri dependency yet.

### M2b — Watcher + Tauri wiring + FDA lifecycle

The integration layer: `notify`-driven watcher consuming the M2a primitives, IPC command, FDA-gated start/stop.

**Scope:**

- `apps/desktop/src-tauri/src/downloads/watcher.rs` — `notify`-based recursive watch on `~/Downloads`, rename-event
  handling (check `from` AND `to` against ignore set; key on the `to` final path), debounce via `notify-debouncer-full`,
  scan-based fallback for empty ring at startup.
- `apps/desktop/src-tauri/src/downloads/commands.rs` — Tauri commands: ONLY `reveal_latest_download()` and
  `downloads_watcher_status()`. (`note_pending_write` is a plain Rust function on the watcher handle held in Tauri state
  — NOT an IPC command. Hook sites call it directly.)
- FDA lifecycle: read `is_fda_pending_runtime()` at startup. Add a `WindowEvent::Focused(true)` listener in `lib.rs` if
  not already present; on focus, re-check the gate and start/stop the watcher accordingly. Settings panel mount also
  triggers a re-check (belt-and-braces).
- Emit `download-detected` Tauri event with `{ path, observed_at_ms, in_subdir: bool, size_bytes?: number }`.
- Register IPC commands in `lib.rs` and `tauri-specta` bindings (regen with `pnpm bindings:regen`).
- Logging convention: `log::debug!(target: "downloads::watcher", ...)`, `log::info!(target: "downloads::watcher", ...)`.
  NO `eprintln!`/`println!`/`dbg!` (clippy denies these crate-wide — see `src-tauri/src/logging/CLAUDE.md`).
- New CLAUDE.md in `apps/desktop/src-tauri/src/downloads/`.

**Tests (TDD where possible):**

- Integration test against a tempdir: spawn the watcher, drop files in (including partial→final renames), assert correct
  events emitted with a captured event sink.
- Test: `note_pending_write` on the watcher handle suppresses the next event for the registered path within TTL, stops
  suppressing after expiry.
- Test: rename event `foo.zip.crdownload` → `foo.zip` produces exactly one `download-detected` event with
  `path = .../foo.zip`.
- Test: Cmdr-own move from inside Downloads to elsewhere does NOT fire (the `from` path is in the ignore set).
- Test: FDA-gate transition (mocked) starts/stops the watcher idempotently.

**Done when:** `./scripts/check.sh --rust` green, all integration tests pass, no clippy warnings, bindings regenerated.

### M3 — Hook Cmdr-own writes into the ignore set

Touch the write-op code paths to register their target paths.

**Scope:**

- `file_system/write_operations/transfer/` — register per-destination just before the write at the per-file level
  (granular masking matters; a copy of 100 files generates 100 events).
- `file_system/write_operations/delete/` — register on the deleted path (defensive; not used yet, but a future
  "deleted-from-Downloads" event would need it).
- `file_system/write_operations/mkdir/` and `mkfile/` — register on the new path.
- `file_system/inline-rename` and clipboard paste — register on destination.
- Filtering is enforced inside `note_pending_write` itself (locked in above): paths outside the resolved Downloads dir
  silently no-op. Call sites invoke unconditionally — no per-call-site `if path.starts_with(downloads_dir)` guard.
- Update `file_system/write_operations/CLAUDE.md`.

**Tests (TDD where possible):**

- Rust unit test on each write op: with the watcher's ignore-set mocked, confirm the op calls `note_pending_write`
  exactly once per destination path, exactly once before the syscall.
- One end-to-end test: in a tempdir simulating Downloads, run a copy via the real write-op driver, assert no
  `download-detected` event fires.

**Done when:** `./scripts/check.sh --rust` green, write-op tests green, no missed call sites (use a grep-based check or
an audit task to confirm every write-op entrypoint has the hook).

### M4 — Reveal-latest-download (in-app `⌘J` + command palette + MCP)

Note: numbering convention — M0 spike runs first, then M1 (toast) and M2a (pure Rust) can run in parallel, then M2b → M3
→ M4 → … sequentially.

Closes the deliberate-action loop. Doesn't yet depend on toasts or notifications — pure reveal.

**Scope:**

- Add `downloads.revealLatest` to `lib/commands/command-registry.ts` with default `⌘J`.
- Add the dispatch case in `routes/(main)/command-dispatch.ts` calling `revealLatestDownload({ index: 0 })`.
- Frontend handler: call the backend IPC, await the result, navigate the focused pane via existing nav helpers (the
  listing/pane API), select the returned file.
- Empty-Downloads INFO toast: "Your Downloads folder is empty. Go there anyway? [Go to Downloads]". Active, friendly,
  casual. Per the style guide.
- FDA-closed INFO toast: "Cmdr needs Full Disk Access to watch your Downloads folder. [Open System Settings]" with a
  dedup id so spamming doesn't stack.
- MCP tool `reveal_latest_download`: register in `mcp/protocol.rs` and `mcp/executor/`, ack on `GenerationAdvanced`.
  Update `mcp/CLAUDE.md`.

**Tests (TDD where possible):**

- Vitest test for the dispatch case: given a mocked IPC returning a path, verify the pane navigates and selects.
- Vitest test for the empty-Downloads INFO toast path.
- Vitest test for the FDA-closed INFO toast path.
- Rust unit test for the backend command: empty ring + empty Downloads, empty ring + populated Downloads (scan
  fallback), populated ring.
- Playwright E2E (optional, low-priority): scripted "create file in Downloads, fire `⌘J`, assert selection in focused
  pane." Only if it fits cleanly into the existing E2E harness.

**Done when:** ⌘J reveals from a fresh launch (scan fallback), from a watched event (ring), and from the empty case
(INFO toast). Both `--fast` and `--rust` green.

### M5 — Downloads notifications (toast + macOS native, one event bridge)

The passive-nudge UI. Depends on M1 (toast infra), M2b (backend events), M4 (the reveal action). Merges in-app toast and
macOS native notification surfaces into a single milestone: both subscribe to the same `download-detected` event and
gate on the same Settings enum, so a single milestone with one shared event bridge is the right cut.

**Scope:**

- Add `tauri-plugin-notification` (Tauri 2 plugin, ≥14-day-old version) to `Cargo.toml` and `package.json`. Verify
  version on crates.io / npmjs.com — don't trust training data.
- Add `notification:default` capability to the relevant window's capability file.
- New component `apps/desktop/src/lib/downloads/DownloadToastContent.svelte` per the visual contract above.
  Pure-prop-driven (the snapshotted shortcut binding is a prop).
- New `apps/desktop/src/lib/downloads/event-bridge.svelte.ts`: mounts ONE `download-detected` Tauri event listener in
  `routes/(main)/+layout.svelte`. Per event, reads the current Settings value and dispatches to:
  - `'in-app'` → `addToast(DownloadToastContent, { toastGroup: 'downloads', timeoutMs: 10000, level: 'info', props })`
    only
  - `'macos'` → `tauri-plugin-notification` post only
  - `'both'` → both
  - `'neither'` → nothing
- macOS notification: title "Downloaded `foo.zip`", body = relative-subdir if any, primary action "Reveal" wired to the
  same reveal code path as `⌘J`. Body-click (default action) does the same. **Snapshot scope:** the notification carries
  NO shortcut hint (intentional — system notifications shouldn't surface app-internal accelerators). The "Reveal" action
  invokes the command, not the binding, so user remaps between notification post and click don't matter.
- Permission flow: when the user picks `'macos'` or `'both'` in Settings, call `tauri-plugin-notification`'s
  `requestPermission()` synchronously. The macOS dialog appears with Apple's fixed wording (we can't customize). If
  denied, leave the setting unchanged but show a single INFO toast: "macOS notifications are off. [Open System
  Settings]". No retries.
- "Stop showing these" wires to a Settings deep-link that sets the toggle to "Neither".
- Defensive: gate on `is_fda_pending_runtime()` AND the notification-permission state for the macOS path.
- New `apps/desktop/src/lib/downloads/CLAUDE.md`.

**Tests (TDD where possible):**

- Vitest behavior test for `DownloadToastContent`: renders filename, shows the snapshotted shortcut, primary button
  triggers reveal, secondary deep-links, body click triggers reveal. Tier-3 a11y test. Mock the deep-link helper
  (`vi.mock('$lib/settings/deep-link', ...)`).
- Vitest test for `event-bridge`: each of the 4 settings values produces the expected calls (mock `addToast` and the
  notification plugin).
- Unit-test the notification response handler in isolation (mock the plugin).
- Manual smoke: drop a file into `~/Downloads` under each of the 4 settings; verify the right surfaces fire; confirm
  "Reveal" from the notification navigates.

**Done when:** `--fast` and `--rust` green. Manual smoke confirms all four settings paths.

### M6 — Global hotkey ⌃⌥⌘J

The escape hatch from Chrome. Default ON, with a first-trigger warning toast.

**Scope:**

- Add `tauri-plugin-global-shortcut` (Tauri 2, ≥14-day-old version). Verify version on crates.io / npmjs.com.
- Add the capability.
- Backend: `downloads::global_shortcut::register(binding) -> Result<(), RegistrationError>` and `unregister(binding)`.
  Status query for the Settings UI ("registered" / "couldn't register").
- Lifecycle: register at startup if
  `settings.behavior.fileSystemWatching.globalRevealShortcut.enabled && !is_fda_pending_runtime()`. On window-focus,
  re-evaluate (settings might have changed, FDA might have flipped). On settings change, register/unregister
  immediately.
- On trigger: same code path as `⌘J`. PLUS, if `acknowledged === false`, also fire the first-trigger warn toast.
- First-trigger warn toast (persistent, level `warn`): copy and buttons per the design above.
- Settings UI: add a row to `FileSystemWatchingSection.svelte` with the on/off toggle, the binding picker, and an inline
  status string (`registered` / `couldn't register: in use by another app`). Greyed when FDA is closed.

**Tests (TDD where possible):**

- Rust unit test for the register/unregister state machine (mock the plugin).
- Vitest test for the warn-toast logic: triggers when `acknowledged === false`, doesn't trigger when `true`; both
  buttons set `acknowledged = true`; "Disable" also flips the `enabled` setting.
- Manual smoke: hotkey from Chrome focused, returns to Cmdr with the latest download selected.

**Done when:** `--fast` and `--rust` green. Manual smoke confirms hotkey works system-wide, warn toast fires once, both
buttons behave correctly, Settings row reflects current state.

### M7 — Settings UI consolidation + rename

Final shape of **Settings > Behavior > File system watching**.

**Pre-flight grep audit** (do this before touching any file): run `rg -i "DriveIndexing|drive-indexing|driveIndexing"`
across the repo. List every hit in the executing agent's report. Expect call sites in: settings registry, deep-link IDs,
e2e selectors (Playwright + Linux), test snapshots, navigation entries, and possibly MCP resources (`cmdr://settings`).
The rename touches all of them; missing any one produces silent bugs (deep-links 404 silently, e2e fails on selector).

**Scope:**

- Rename `DriveIndexingSection.svelte` → `FileSystemWatchingSection.svelte` and update the Settings registry.
- Restructure into sub-groups: "Drive indexing" (existing controls), "Downloads notifications" (new ToggleGroup from
  M5), "Global reveal shortcut" (new row from M6).
- Update settings registry IDs and any deep-link targets ("Stop showing these" from M5, "Open System Settings" from
  M4/M5).
- Update every hit from the pre-flight grep.
- Verify all 4 settings (`downloadsNotifications`, `globalRevealShortcut.enabled`, `.binding`, `.acknowledged`) persist
  correctly.
- Verify FDA-closed grey-out state for both new sub-groups, with a single hint that links to System Settings.
- Update `lib/settings/sections/CLAUDE.md`.

**Tests (TDD where possible):**

- Vitest test for the section render with each of: FDA granted, FDA pending; each of the 4 ToggleGroup values; global
  shortcut enabled/disabled.
- Tier-3 a11y test (heading order, keyboard nav, ARIA on the ToggleGroup is already covered by the shared component).

**Done when:** `--fast` green, Settings UI looks right in light + dark mode, deep-links land in the right place.

### M8 — Docs, full checks, polish

Final pass.

**Scope:**

- Verify and update all CLAUDE.md files listed under "Documentation updates" above.
- `docs/architecture.md` updates (downloads on both frontend + backend tables; settings section rename).
- `AGENTS.md` — only update if a critical-rules change is needed; otherwise leave.
- Run `./scripts/check.sh` (full default suite). Then run `./scripts/check.sh --include-slow` once everything else is
  green.
- Fix any new file-length warnings by splitting modules. Don't bump the allowlist without justification.
- Manual smoke:
  - Drop a file from Chrome → see toast → click → land on file. ✓
  - `⌘J` from Cmdr-focused. ✓
  - `⌃⌥⌘J` from Chrome-focused. ✓
  - First-trigger warn toast on `⌃⌥⌘J`. ✓
  - "Stop showing these" → Settings opens to the right row. ✓
  - "macOS notifications" picked → OS prompt → notification fires. ✓
  - Bulk copy 100 files via Cmdr → no toasts (ignore-set works). ✓
  - Light mode, dark mode, OS appearance change while Cmdr is open. ✓

**Done when:** `./scripts/check.sh` green, `--include-slow` green, all manual smoke items pass, every doc updated.

---

## Parallelism notes

Most of this is sequential by necessity. The exceptions:

- **M1 (toast infra)** and **M2a (pure-Rust modules)** are fully independent (different languages, different files).
  They can run in parallel after M0 if we want speed.
- **M6 (global hotkey)** can start while M5 (notifications) is still wrapping up if the dispatch case from M4 is already
  in place — both depend on M4 but not on each other.

But David said "we're usually not in a hurry and sequential running is totally fine." So default to sequential. Only
parallelize if the executing leader sees a clear win and the worktree story is simple (no cross-modifying the same
files).

## Version pinning reminder

`tauri-plugin-notification` and `tauri-plugin-global-shortcut` are new dependencies. At implementation time:

- Look up the latest version on crates.io (Rust) and npmjs.com (JS half) — DO NOT trust training data.
- Verify the version is ≥14 days old (Cmdr's `minimumReleaseAge` policy from
  `~/.claude/rules/use-latest-dep-versions.md`). If it's not, pin to the previous version that is, then let Renovate
  catch us up.
- After `Cargo.toml` / `package.json` edits, run `pnpm dedupe`.

## Test cadence

- After each milestone: `./scripts/check.sh --fast` minimum, `./scripts/check.sh` if Rust files touched.
- After M8: `./scripts/check.sh --include-slow` once. Don't run the slow lane on intermediate milestones unless
  something downstream specifically requires it.

## Risks and open questions

1. **`notify` on macOS Downloads under TCC.** Promoted to **M0 spike** above. Resolves before M2b.
2. **Window-focus event firing reliably.** If the focus event doesn't fire on the relevant transitions (e.g., user
   grants FDA in System Settings, then Cmd-Tabs back), the Settings-pane mount re-check covers the most likely user
   path. Verify during M2b.
3. **Shortcut recorder for global combos.** The existing recorder is built for in-app shortcuts. System-modifier capture
   has quirks (Cmd-Tab interception, Option-key dead keys). If the recorder can't capture `⌃⌥⌘J` reliably, accept a
   constrained recorder for v1 (predefined options + custom-via-config) and file a follow-up.
4. **macOS notification permission revocation mid-session.** If the user revokes notification permission from System
   Settings while Cmdr is running, our next `requestPermission()` returns `denied`. We should detect this on
   window-focus and show a one-shot INFO toast. Low-priority polish.
5. **Settings deep-link sub-group targeting.** The "Stop showing these" toast button (M5) and the "Cmdr needs Full Disk
   Access..." toast (M4) both rely on deep-linking into a specific sub-group within `FileSystemWatchingSection`. If the
   existing Settings deep-link helper only supports section-level IDs (not sub-group anchors), the link lands at the top
   of the section, not on the right row. **M7 acceptance criterion:** "Stop showing these" lands focused on the
   `downloadsNotifications` ToggleGroup; if the helper doesn't support anchors, extend it or accept a section-level
   deep-link for v1 + scroll-into-view as a follow-up.
