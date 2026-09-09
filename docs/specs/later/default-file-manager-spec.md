# Reveal in Cmdr + default folder handler

Status: **mechanism A is built and works end to end** (`apps/desktop/src-tauri/src/reveal/`, 2026-09-09), cold launch
included. Mechanism B, onboarding, the first-activation moment, and the Settings UI are still deferred. The spike ran on
2026-09-09 (macOS 26.6 / Darwin 25.6, probe app plus a bundled Cmdr debug build driven by `open -R`); its answers are
folded into the sections below and supersede the 2026-07-14 web research.

What exists now: the `NSFileViewer` registration state machine plus its three IPC commands, the `RunEvent::Opened` arm,
the cold-start buffer and its drain, and the shared navigate-then-cursor pane primitive. Depth and decisions live in
`apps/desktop/src-tauri/src/reveal/DETAILS.md`; ❌ don't restate them here.

## Goal

Two related, separately toggleable capabilities:

- **A. Reveal redirect**: when another app does "Show in Finder" / "Reveal in Finder"
  (`NSWorkspace.activateFileViewerSelectingURLs` and `selectFile:inFileViewerRootedAtPath:`), Cmdr opens with the file
  selected instead of Finder.
- **B. Folder-open handler**: folder opens dispatched through LaunchServices (`open .` in a terminal, Spotlight folder
  hits, other apps opening a folder URL) land in Cmdr.

Explicitly NOT the goal, and the UI copy must not overpromise: replacing Finder wholesale. Finder keeps the desktop,
Trash, menu bar, and mount UI. Double-clicking a folder inside a Finder window stays in Finder; Finder opens folders
internally without consulting LaunchServices (expectation from research; spike item 4 confirms the exact surface).

## The two mechanisms

They're independent OS toggles with different blast radius and different fragility:

- **A. `NSFileViewer` global default** (`defaults write -g NSFileViewer -string <bundle-id>`): an UNOFFICIAL,
  undocumented AppKit key consulted when apps call the reveal APIs. Per-user, plain user default (write via
  `CFPreferences` on the global domain, not by shelling out). No sanctioned API, could break in any macOS release.
  **Built.** Verified working on macOS 26.6 for `open -R` and the two `NSWorkspace` reveal calls; AppleScript's
  `tell Finder to reveal` does not redirect, by design (verified 2026-09-09).
- **B. `public.folder` default handler**: the sanctioned LaunchServices route. Set via
  `NSWorkspace.setDefaultApplication(at:toOpenContentType:completionHandler:)` (macOS 12+, above the 10.15
  `minimumSystemVersion` floor in `tauri.conf.json`, so it needs a `macos_at_least(12, 0)` gate plus the
  `allowed-newer-selector` marker, and the feature stays hidden below macOS 12). Takes the app URL, async completion,
  surfaces errors. Never poke `com.apple.launchservices.secure` with `defaults write` (the recipe most blogs show): it's
  fragile and LS db rebuilds can drop it.

**Not a prerequisite for A** (spike item 1, answered 2026-09-09): a probe app declaring no `CFBundleDocumentTypes` and
claiming no UTI still received the reveal, so `NSFileViewer` alone is sufficient and Cmdr ships no document-type claim.
Prerequisite for B only: Cmdr must declare itself a folder viewer via `CFBundleDocumentTypes` with
`LSItemContentTypes = [public.folder]`, `CFBundleTypeRole = Viewer`, `LSHandlerRank = Alternate` (Alternate so we never
auto-become default; the toggle is the only path). Tauri's `bundle.fileAssociations` is extension-based and can't
express a UTI-only type, so this goes directly into `apps/desktop/src-tauri/Info.plist`, which already exists and which
Tauri merges. Side effect worth knowing: Cmdr will appear in Finder's "Open With" menu for folders even with both
toggles off. That's arguably a free feature, not a bug.

## Event delivery and app plumbing

**Built**, in `apps/desktop/src-tauri/src/reveal/` plus the `RunEvent::Opened` arm in `app_lifecycle.rs`: the dispatch
rule (file → parent + cursor; folder → open it), the window raise, the cold-start buffer and its frontend drain, and
multi-URL accumulation. It reuses `go_to_latest_download`'s pane primitive, now shared as `mcp::go_to_in_focused_pane`.
Mechanics and rationale: `apps/desktop/src-tauri/src/reveal/DETAILS.md`.

Verified end to end (macOS 26.6, `open -R`, 2026-09-09): cold launch with a file, already running with a file in another
directory, already running with a folder, and two siblings at once.

❗ **The trap this cost a day to find**: on a cold launch the event fires ~500 ms BEFORE Tauri's `setup` closure, so at
that moment there is no `app.manage`d state, no logger, and no main window. Buffering is not a nicety here, it is the
delivery mechanism, and it has to be process-global. A log line added to that path never appears, which is what made the
event look like it was never sent at all. `DETAILS.md` § "How a cold-launch reveal arrives" also records the two fixes
built and reverted before the timing was measured, so nobody spends that day again.

## Registration module

**Built for A** (`reveal/registration.rs`): read-through state, enable, disable, the production-bundle-only gate, and
the "clear only if it's ours" rule, each covered by a test against an in-memory fake. Decisions and their reasons:
`apps/desktop/src-tauri/src/reveal/DETAILS.md` § Registration.

Still to build, for B: `NSWorkspace.urlForApplication(toOpen:)` for `public.folder`,
`setDefaultApplication(at:toOpenContentType:)` to set it, and Finder's app URL to revert. The Settings toggle for B must
mirror actual OS state on every open, same as A's does, and must name a third-party incumbent rather than clobbering it
silently.

## UX

Decided (conversation, 2026-06/07):

- **Default OFF for both.** This mutates machine-wide OS state; installing a beta app must not rewire the Mac. Opt-in
  lives on onboarding step 4 (the existing "Optional" step: `OnboardingStep` is `1 | 2 | 3 | 4` and step 4 renders
  `StepOptional.svelte`, verified 2026-08-27) and in Settings.
- **First-activation moment**: the first time a redirect/open actually fires, show a one-time "this is the thing you
  turned on, here's the off switch" confirmation. Since the user opted in, the tone is friendly confirmation, not
  apology. David wants a notification here; recommendation on the table (undecided): an in-app sheet in the just-focused
  window instead of a Notification Center banner (banners auto-dismiss, can't host two styled buttons, and attention is
  already on our window).
- **No red opt-out button** (red = destructive in HIG and pressure-frames leaving it on). "Keep" is the primary, "turn
  off" a neutral secondary. On turn-off, swap the same surface's content to a short friendly confirmation; no second
  notification, no groveling.
- All copy through i18n; style guide applies (no "error"/"failed", active voice, sentence case).
- **A's row is not a stored setting**: no `settings.json` key and no registry entry. `getRevealHandlerState` reads
  through to the OS every time, and `setRevealHandlerEnabled` returns the state the OS was left in, which is what the
  row must render. The typed state carries `registered` / `notRegistered` / `heldByOtherApp { bundleId, displayName }` /
  `unavailable`; `unavailable` means this build may never write the key, so the row hides rather than disabling.

Open questions:

1. One toggle or two? Recommendation: two toggles under one Settings group ("Default file manager" or similar);
   reveal-redirect (A) is the gentle, high-value one, folder-open (B) the bold one. Onboarding step 4 can offer just A,
   or both.
2. In-app sheet vs Notification Center banner for the first-activation moment (see above).
3. Folder-open target when Cmdr is already running: navigate the focused pane, or open a new tab in it? Recommendation:
   new tab (doesn't clobber the user's current locations; matches protect-user-state).
4. Onboarding copy and Settings copy (write with the style guide; pitch honestly: "folder opens and reveals land in
   Cmdr", not "Cmdr becomes your default file manager").

## Spike checklist

Ran 2026-09-09 on macOS 26.6 (Darwin 25.6). Numbers below are the original item numbers. Answered:

1. **`NSFileViewer` alone is enough.** No `public.folder` claim, no `CFBundleDocumentTypes`, no UTI needed (probe app
   with none of them still received the reveal).
2. **`RunEvent::Opened` carries the revealed items themselves**, as `file://` URLs; a reveal of a folder arrives as that
   folder's URL. It fires for both a cold launch and an already-running app — but on a cold launch it lands before
   `setup`, which is what makes the buffering load-bearing. See § "Event delivery".
3. Coverage for A: `open -R` verified end to end. `activateFileViewerSelectingURLs:` and
   `selectFile:inFileViewerRootedAtPath:` (what Chromium and Electron call) redirect; AppleScript
   `tell Finder to reveal` does not, by design. A per-app matrix (Chrome, Safari, VS Code, Slack) is still worth running
   for honest toggle copy.
4. **No OS consent dialog, no TCC prompt, no entitlement** for A on macOS 26.6.

Still open, all for mechanism B: item 4 (B's actual surface), item 5 (deleting Cmdr.app while registered), and item 7
(side effects of the `Info.plist` declaration).

## Rough cost

Spent on A: event plumbing, buffering, registration module, and the shared pane primitive, with unit tests. Remaining:
mechanism B ~1 day; Settings rows + onboarding step 4 + first-activation surface + copy + i18n ~1 day. Cold-launch
reveal is hard to E2E; the dispatch rule and the buffering are covered by Rust unit tests, and the toggles get
Playwright coverage when they exist.

## Sources

- [ForkLift as default file viewer (MPU forum)](https://talk.macpowerusers.com/t/forklift-3-default-file-viewer-with-mac-os-catalina-10-15-7/33989)
- [yazi discussion #1828: default file viewer recipe](https://github.com/sxyazi/yazi/discussions/1828)
- [fman issue #555: NSFileViewer redirects but app must handle the open event](https://github.com/fman-users/fman/issues/555)
- [Apple: activateFileViewerSelectingURLs](https://developer.apple.com/documentation/appkit/nsworkspace/1524549-activatefileviewerselectingurls)
- [Tauri: RunEvent::Opened / file associations](https://v2.tauri.app/learn/mobile-file-associations/) and
  [tauri#13159 (fileAssociations lacks LSHandlerRank on macOS)](https://github.com/tauri-apps/tauri/issues/13159)
