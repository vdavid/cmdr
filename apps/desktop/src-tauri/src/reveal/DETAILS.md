# Reveal in Cmdr: architecture and decisions

Up: `CLAUDE.md`. The product-level plan, including the parts still deferred:
`docs/specs/later/default-file-manager-spec.md`.

## The mechanism

macOS keeps one per-user preference that decides who gets a reveal: `NSFileViewer` in the global preferences domain
(`Apple Global Domain`, which `defaults write -g` writes). It holds a bundle identifier. When it names an app, AppKit
routes the reveal there instead of to Finder.

Measured on macOS 26.6 (Darwin 25.6) with a throwaway probe app and a bundled Cmdr debug build, 2026-09-09:

- **`NSFileViewer` alone is sufficient.** A probe that declared no `CFBundleDocumentTypes` and claimed no UTI still
  received the reveal. Cmdr does NOT need to be a declared `public.folder` handler, which settles the open spike
  question. That's why no `CFBundleDocumentTypes` claim was added to `Info.plist`.
- **What arrives** is a plain open-documents Apple Event (`aevt`/`odoc`) whose direct object is a list of `bmrk`
  descriptors coercing to file URLs. The URLs are the items to reveal themselves, not their parents; a reveal of a
  folder arrives as that folder's own URL.
- **Honoured by**: `-[NSWorkspace activateFileViewerSelectingURLs:]`, `-[NSWorkspace
  selectFile:inFileViewerRootedAtPath:]` (the two Chromium and Electron call), and `/usr/bin/open -R`. AppleScript's
  `tell application "Finder" to reveal` does not redirect, by design.
- No TCC prompt, no entitlement, no helper app, no login item.

❗ **The key is undocumented and unofficial.** Apple has never published it; ForkLift and Path Finder ship exactly this
and nothing else. It can stop working in any macOS release, and nothing will tell us but a user report. That's a known,
accepted cost, not something to design around.

## How a cold-launch reveal arrives

**A reveal reaches Cmdr whether or not it is already running**, both times as
`RunEvent::Opened`. The two cases differ only in timing, and that timing is the whole
trap:

- **Already running**: the event arrives mid-session. Everything exists, and the pane moves
  immediately.
- **Cold launch**: LaunchServices starts Cmdr and the event arrives **before Tauri's `setup`
  closure runs** — measured at 513 ms before it, on macOS 26.6, from a breadcrumb trail in a
  bundled debug build driven by `open -R`, 2026-09-09.

Two things are therefore not yet true when `on_urls_opened` runs on a cold launch, and both
used to swallow the reveal without a trace:

- **`app.manage`d state does not exist.** `setup` is where it is registered, so a
  `try_state` lookup comes back empty. That is why [`PENDING`] is a process-global
  `LazyLock` and ❌ never Tauri-managed state.
- **The logger is not initialised.** `logging::startup::init()` also runs in `setup`, so
  every `log::` call from this path goes nowhere. ❌ Don't debug this path by adding log
  lines — they will not appear, and their absence is not evidence the code didn't run. The
  first honest line is the drain's "Frontend is up; delivering N parked path(s)".

The main window also doesn't exist yet, so `raise_main_window` is a no-op there. Nothing
needs to special-case the cold path: the buffer holds the paths and the frontend's drain
delivers them once its listeners are up.

**Ruled out, so nobody re-tries them.** Before the timing was measured, this looked like
"the event never arrives", and two fixes were built and reverted:

- An `NSAppleEventManager` / `AEInstallEventHandler` handler for `kAEOpenDocuments`,
  installed as the first statement in `run()`. It never fired once: AppKit claims the
  `odoc` handler while the app is being built and gets there first.
- A swizzle of `application:openURLs:` on the live app delegate. It fired for the
  already-running case and never for the cold one — installed in `setup`, it was already on
  the wrong side of the line.

Neither is needed. `RunEvent::Opened` is the whole delivery mechanism.

## Delivery

All of this is `delivery.rs`, which is macOS-only like the rest of the mechanism; `mod.rs` carries only
`RevealDelivered`, which `ipc.rs`'s `collect_events!` needs to resolve on every platform.

`app_lifecycle.rs`'s `RunEvent::Opened` arm hands the URLs to `on_urls_opened`. From there:

1. Keep the `file://` URLs, drop everything else.
2. Raise the main window (`unminimize` → `show` → `set_focus`), the same three calls the go-to-latest-download hotkey
   makes. The reveal was fired from another app, so without the raise the result stays hidden behind it. On a cold
   launch there is no window yet and this does nothing, which is correct: the window is on its way up anyway.
3. Park the paths in `PendingReveals`, and deliver them right away if the frontend has drained at least once.
4. `plan_reveal` decides the pane move, then `crate::mcp::go_to_in_focused_pane` makes it.
5. On success, `RevealDelivered` goes out. **Decision: after the move, ❌ never before it.** The frontend spends a
   once-ever notice on the first one it sees (`apps/desktop/src/lib/reveal/DETAILS.md`), and a reveal onto a dead mount
   or an unreadable path must not be what spends it. It carries no payload: the paths are already crossing over
   `mcp-nav-to-path`, and a second copy would be a second thing that can disagree.

Verified end to end on macOS 26.6 (bundled debug build, `open -R`, 2026-09-09): cold launch with a file, already
running with a file in another directory, already running with a folder, and two siblings at once. `open -R` with
several paths sends one event per path, so the multi-URL branch below is exercised by its unit tests rather than by
that command.

### The dispatch rule

A file navigates the focused pane to its parent and puts the cursor on it. A folder is navigated INTO. So a reveal of a
folder becomes an open of it; Finder parity isn't the goal, landing where the user asked about is.

Multiple URLs accumulate rather than overwrite. `plan_reveal` takes the first path's directory and honours every other
path that sits in it — cursor on the first, all of them selected via `mcp-select-names`. Paths in other directories are
dropped with a log line saying how many: one pane shows one directory, and picking a different one for the rest would
be a guess.

The folder probe runs under a 2 s deadline (`blocking_with_timeout`). A revealed path can sit on a dead network mount
where `is_dir` blocks for minutes, and a reveal that quietly does nothing beats a task wedged on a syscall.

### Buffering

`PendingReveals` is a `Vec` plus a `frontend_ready` flag, not an `Option`: two reveals can land back to back and the
second must not erase the first. Nothing is delivered until the frontend calls `drain_pending_reveals`, because the
delivery rides `mcp-nav-to-path` and an emit into a window with no listener is simply lost.

This is what carries a cold-launch reveal across the ~1 s between the event arriving and the frontend being able to act
on it, so it is load-bearing for the feature's main selling point, not a safety net. § "How a cold-launch reveal
arrives".

**Decision: the drain fires from phase 2 of `window-services.ts`, not phase 1.** Phase 1 runs at the top of `onMount`,
before `setupMcpListeners`, so a drain there would race the very listener it depends on. Phase 2 runs after every `mcp-*`
listener is registered and after the explorer handle exists. There is no frontend-ready handshake in this repo to reuse;
this call is it.

The command returns as soon as it has taken the parked paths and spawns the pane move on the async runtime. It must
not await the move: the frontend that has to answer `mcp-nav-to-path` is the same frontend waiting on the call.

## Where the pane primitive lives

`go_to_in_focused_pane` sits in `mcp/executor/nav.rs` and is re-exported from `mcp` as a named seam. Two callers: the
`go_to_latest_download` MCP tool, which reaches `executor::nav` directly from inside the module, and this one, which is
the seam's only user. That's why the re-export itself carries a `#[cfg(target_os = "macos")]`: without it the export is
dead on Linux and `-D unused` fails the build.

**Decision: it lives with the MCP executor rather than here or in a neutral module.** "Take the focused pane there and
point at that" is spoken entirely in the `mcp-*` frontend protocol (`mcp-nav-to-path`, `mcp-move-cursor`,
`mcp-select-names`) and needs the round-trip machinery that only exists inside `mcp::executor` — the request-id
correlation, the typed `NavAck` landing check, the ack budgets. Moving it out would mean either widening `executor`'s
surface or duplicating that machinery. The name `mcp` is about the protocol these events are named for, not about who
is allowed to speak it; an OS reveal makes the same move a user or an agent does.

Ordering inside it is load-bearing: navigate, move the cursor, THEN multi-select. The frontend's cursor move resets the
selection, so selecting first would lose it.

## Registration

`RevealRegistration<P: ViewerPreference>` is the state machine; `GlobalDomain` is the production `P` over
`CFPreferencesCopyValue` / `SetValue` / `Synchronize` against (`kCFPreferencesAnyApplication`,
`kCFPreferencesCurrentUser`, `kCFPreferencesAnyHost`) — the triple `defaults write -g` uses. CFPreferences and
LaunchServices are both thread-safe, so no `MainThreadMarker` is needed (`../../DETAILS.md` § "Which Apple APIs skip
the main-thread rule").

The `Synchronize` call is not optional: without it the change stays in this process's `cfprefsd` cache and an app
asked to reveal a second later still gets the old answer.

**Decision: disable clears the key only when it currently equals our bundle id.** Another app can take it at any moment
— that's the whole point of a shared global key — and a blind clear would silently unregister ForkLift or Path Finder
on a user who only meant to switch Cmdr off. When the key isn't ours, `set_enabled(false)` reports `HeldByOtherApp` and
touches nothing.

**Decision: clear means remove.** Absence is the OS default, and writing `com.apple.finder` instead would pin a choice
the user never made and would survive their later preference for something else.

**Decision: production bundles only.** `own_bundle_id` returns `None` — and every operation then reports `Unavailable`
and writes nothing — unless all three hold: not a debug build, no `prod_instance` harness env var set, and
`NSBundle.mainBundle().bundleIdentifier()` equal to the production id. Three gates because each catches a case the
others miss: `pnpm dev` off the plain config still carries the production identifier; a release-mode E2E lane is caught
by its env; and every `--worktree` / E2E instance carries a suffixed id (`apps/desktop/scripts/instance-id.ts`). The
bundle id is read from `NSBundle` at runtime, never hardcoded, because telling those builds apart is the whole job.

Why it matters: an uninstalled build that still holds the key leaves a dangling `NSFileViewer`, and reveal silently
stops working system-wide with nothing pointing at the cause.

**Decision: no stored setting.** The Settings row reads through to the OS on every open, and the write returns the
state the OS was left in rather than the state that was asked for. A `settings.json` mirror would disagree with the
machine the first time the user changed the handler elsewhere, and there is no event to keep it in sync.

Display names come from `NSWorkspace URLForApplicationWithBundleIdentifier:` plus the existing
`file_system::open_with::read_app_display_name` (a plist read), so the row can say "Currently: Path Finder". A holder
that isn't installed reports `None` and the UI falls back to the raw bundle id.

## The Settings row

`Settings > Behavior > Navigation & file ops > Show in Finder`, built as
`apps/desktop/src/lib/settings/sections/RevealHandlerCard.svelte`. It's the settings system's only OS-BACKED row (no
registry entry, no `settings.json` key), so what that pattern is and when to reach for it lives over there:
`apps/desktop/src/lib/settings/DETAILS.md` § OS-backed rows. Two things worth knowing from this side: the row renders
nothing on an `Unavailable` answer, so it never appears in a dev, worktree, or E2E build, and it renders the state
`set_reveal_handler_enabled` RETURNS rather than the one the click asked for, which is what makes the
"another app took the key first" case honest.

## How the feature is offered

The Settings row is not the only way in. `apps/desktop/src/lib/reveal/DETAILS.md` owns both moments a person meets this:
the once-ever offer a couple of launch days in (one of the two nudges, `apps/desktop/src/lib/nudges/DETAILS.md`), and
the once-ever notice the first time a reveal actually lands.

## What's not built

Mechanism B (the `public.folder` LaunchServices handler) and the onboarding step. See the spec.
