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

## The cold-launch gap

**A reveal that has to launch Cmdr is lost.** LaunchServices cold-launches the app, and then nothing is delivered:
neither `RunEvent::Opened` nor `application:openURLs:` on the app delegate ever fires. The window opens on its
remembered location, as if the app had been launched from the Dock. A reveal to an ALREADY-RUNNING Cmdr works, every
time.

Measured 2026-09-09 on macOS 26.6 with a Developer ID-signed bundled debug build
(`CMDR_INSTANCE_ID=dev-reveal-probe`), driven by `open -R`, read back from the log. Two candidate causes were tested
and **ruled out**:

- *Tauri or tao dropping the event before its callback exists.* A swizzle of `application:openURLs:` on the live app
  delegate class, installed during `setup` (before the event loop starts), fired for the already-running case and never
  for the cold-launch case. So the event isn't reaching AppKit's delegate at all. The swizzle was removed again: it
  bought nothing that `RunEvent::Opened` doesn't already do.
- *The missing document-type declaration.* Adding `CFBundleDocumentTypes` with `LSItemContentTypes = [public.item]`,
  `Viewer` / `Alternate`, to the built bundle and re-registering with `lsregister` changed nothing.

What's left to try, for whoever picks this up: register an `NSAppleEventManager` handler for `kAEOpenDocuments`
directly and see whether the event is queued but unhandled, or genuinely never sent; and compare against the throwaway
probe app that DID get a cold-launch reveal, since the difference between it and Cmdr is the lead. Cmdr's startup is
seconds long and pumps nested run loops (`instance_lock`'s `CFUserNotification`) before Tauri builds its event loop,
which is the most likely place a queued Apple Event goes missing.

The buffering below stays regardless: it's what a fix would deliver into, and it already covers the smaller race of a
reveal landing while an already-running app's window is still booting.

## Delivery

`app_lifecycle.rs`'s `RunEvent::Opened` arm hands the URLs to `on_urls_opened`. From there:

1. Keep the `file://` URLs, drop everything else.
2. Raise the main window (`unminimize` → `show` → `set_focus`), the same three calls the go-to-latest-download hotkey
   makes. The reveal was fired from another app, so without the raise the result stays hidden behind it.
3. Park the paths in `PendingReveals`, and deliver them right away if the frontend has drained at least once.
4. `plan_reveal` decides the pane move, then `crate::mcp::go_to_in_focused_pane` makes it.

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

**Decision: the drain fires from phase 2 of `window-services.ts`, not phase 1.** Phase 1 runs at the top of `onMount`,
before `setupMcpListeners`, so a drain there would race the very listener it depends on. Phase 2 runs after every `mcp-*`
listener is registered and after the explorer handle exists. There is no frontend-ready handshake in this repo to reuse;
this call is it.

The command returns as soon as it has taken the parked paths and spawns the pane move on the async runtime. It must
not await the move: the frontend that has to answer `mcp-nav-to-path` is the same frontend waiting on the call.

## Where the pane primitive lives

`go_to_in_focused_pane` sits in `mcp/executor/nav.rs` and is re-exported from `mcp` as a named seam. Two callers: the
`go_to_latest_download` MCP tool and this module.

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

## What's not built

Mechanism B (the `public.folder` LaunchServices handler), the onboarding step, the first-activation notice, and the
Settings UI itself. See the spec.
