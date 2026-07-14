# Reveal in Cmdr + default folder handler

Status: deferred. Research done 2026-07-14 (web sources + codebase recon, Fable 5 session "Open with Cmdr - Finder alt
via LSHandler"); NOT yet spike-verified on a real machine. Run the spike below before building.

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
  `CFPreferences` on the global domain, not by shelling out). No sanctioned API, could break in any macOS release;
  evidence: ForkLift/Path Finder document it, and community recipes confirm it works through recent macOS (researched
  2026-07, community sources: ForkLift docs, MPU forum, yazi/fman GitHub threads).
- **B. `public.folder` default handler**: the sanctioned LaunchServices route. Set via
  `NSWorkspace.setDefaultApplication(at:toOpenContentType:completionHandler:)` (macOS 12+, exactly our
  `minimumSystemVersion` floor, so no fallback path needed). Takes the app URL, async completion, surfaces errors. Never
  poke `com.apple.launchservices.secure` with `defaults write` (the recipe most blogs show): it's fragile and LS db
  rebuilds can drop it.

Prerequisite for B (and possibly for A; spike item 1): Cmdr must declare itself a folder viewer via
`CFBundleDocumentTypes` with `LSItemContentTypes = [public.folder]`, `CFBundleTypeRole = Viewer`,
`LSHandlerRank = Alternate` (Alternate so we never auto-become default; the toggle is the only path). Tauri's
`bundle.fileAssociations` is extension-based and can't express a UTI-only type, so this goes directly into our
`Info.plist` (Tauri merges it). Side effect worth knowing: Cmdr will appear in Finder's "Open With" menu for folders
even with both toggles off. That's arguably a free feature, not a bug.

## Event delivery and app plumbing

- macOS delivers the open/reveal as an Apple Event (`odoc`) → AppKit `application:openURLs:` → Tauri
  `RunEvent::Opened { urls }` (macOS/iOS). No argv involved, ever: cold launch and already-running both arrive as this
  event. macOS single-instances GUI apps through LaunchServices, so no second process and no need for a single-instance
  plugin on macOS.
- **Dispatch rule**: URL is a file → reveal (navigate focused pane to parent, cursor on the file). URL is a folder →
  open it. Known ambiguity: a _reveal_ of a folder becomes an _open_ of it; acceptable, Finder-parity is not required.
- **Cold-start race**: `RunEvent::Opened` can fire before the webview/frontend is mounted; a naive forward drops the
  event and the launch silently lands on the default dirs. Buffer URLs in Rust and replay on the frontend-ready signal
  (find the existing FE-ready handshake; the MCP session init path has one).
- **Window state**: on delivery, show + unminimize + focus the main window.
- **The reveal primitive already exists**: `mcp/executor/downloads.rs` (`go_to_latest_download`) does exactly
  navigate-parent-then-`mcp-move-cursor`, with the disappeared-file race handled. Reuse that internal path; don't build
  a second one.
- Multi-URL events (user opened several folders at once): iterate; see open question 3.
- Revealed paths may hit TCC prompts like any navigation; existing flows handle it, nothing new needed.

## Registration module (Rust, objc2 – we already ship objc2-app-kit 0.3)

- **Read current state**: `NSWorkspace.urlForApplication(toOpen:)` for `public.folder`, and read the `NSFileViewer`
  default. The Settings toggles MUST mirror actual OS state on every Settings open, not a stored flag: the user can
  change handlers externally, and another file manager can take the handler at any time.
- **Set B**: `setDefaultApplication(at: <our bundle URL>, toOpenContentType: UTType("public.folder"))`.
- **Set A**: write `NSFileViewer` = our bundle id to the global preferences domain.
- **Revert**: B → Finder's app URL (`/System/Library/CoreServices/Finder.app`); A → delete the key (not "set to Finder";
  absence is the true default state).
- **Courtesy to existing handlers**: if the current `public.folder` handler is neither Finder nor Cmdr (ForkLift, Path
  Finder), the toggle copy names it ("Currently: ForkLift") and turning us on is still one click, but never clobber
  silently from onboarding bulk-apply.
- Dev/E2E builds: only offer registration in production bundles (a debug bundle id grabbing the user's handler is a
  footgun).

## UX

Decided (conversation, 2026-06/07):

- **Default OFF for both.** This mutates machine-wide OS state; installing a beta app must not rewire the Mac. Opt-in
  lives on onboarding step 4 (the existing "Optional" step, `OnboardingStep = 4`) and in Settings.
- **First-activation moment**: the first time a redirect/open actually fires, show a one-time "this is the thing you
  turned on, here's the off switch" confirmation. Since the user opted in, the tone is friendly confirmation, not
  apology. David wants a notification here; recommendation on the table (undecided): an in-app sheet in the just-focused
  window instead of a Notification Center banner (banners auto-dismiss, can't host two styled buttons, and attention is
  already on our window).
- **No red opt-out button** (red = destructive in HIG and pressure-frames leaving it on). "Keep" is the primary, "turn
  off" a neutral secondary. On turn-off, swap the same surface's content to a short friendly confirmation; no second
  notification, no groveling.
- All copy through i18n; style guide applies (no "error"/"failed", active voice, sentence case).

Open questions:

1. One toggle or two? Recommendation: two toggles under one Settings group ("Default file manager" or similar);
   reveal-redirect (A) is the gentle, high-value one, folder-open (B) the bold one. Onboarding step 4 can offer just A,
   or both.
2. In-app sheet vs Notification Center banner for the first-activation moment (see above).
3. Folder-open target when Cmdr is already running: navigate the focused pane, or open a new tab in it? Recommendation:
   new tab (doesn't clobber the user's current locations; matches protect-user-state).
4. Onboarding copy and Settings copy (write with the style guide; pitch honestly: "folder opens and reveals land in
   Cmdr", not "Cmdr becomes your default file manager").

## Spike checklist (~1 day, do first; each result gets an evidence anchor)

1. Does `NSFileViewer` alone redirect `activateFileViewerSelectingURLs`, or must the app also be the (or a declared)
   `public.folder` handler? Community recipes always set both; test the combos.
2. What exactly arrives in `RunEvent::Opened` for (a) reveal-of-file, (b) open-of-folder, (c) cold launch vs already
   running; and the cold-launch timing vs webview-ready (verify the buffering is needed and sufficient).
3. Coverage matrix for A: Chrome/Safari download "show in Finder", VS Code "Reveal in Finder", Slack, `open -R`,
   AppleScript `tell Finder to reveal`. Expect partial coverage (apps that script Finder directly won't redirect);
   record which, for honest toggle copy.
4. Actual surface of B: `open .`, Spotlight folder hit, Dock folder click, "open containing folder" from a few apps, and
   confirm within-Finder navigation is untouched.
5. Deleting Cmdr.app while registered: confirm LS falls back to Finder for B and AppKit falls back for A (uninstall runs
   no code, so graceful degradation is the only cleanup we get).
6. Confirm no OS consent dialog interferes (macOS 15+ added consent UI for default _browser_ changes; folders are
   believed prompt-free; verify on macOS 15 and 26).
7. Confirm the `Info.plist` declaration alone (toggles off) has no surprising side effects beyond the "Open With" menu
   entry.

## Rough cost (after the spike)

Event plumbing + buffering + reveal wiring ~1 day; registration module ~1 day; Settings + onboarding step 4 +
first-activation surface + copy + i18n ~1 day. E2E: cold-launch reveal is hard to E2E; cover the dispatch rule and
buffering with Rust unit tests, the toggles with Playwright.

## Sources

- [ForkLift as default file viewer (MPU forum)](https://talk.macpowerusers.com/t/forklift-3-default-file-viewer-with-mac-os-catalina-10-15-7/33989)
- [yazi discussion #1828: default file viewer recipe](https://github.com/sxyazi/yazi/discussions/1828)
- [fman issue #555: NSFileViewer redirects but app must handle the open event](https://github.com/fman-users/fman/issues/555)
- [Apple: activateFileViewerSelectingURLs](https://developer.apple.com/documentation/appkit/nsworkspace/1524549-activatefileviewerselectingurls)
- [Tauri: RunEvent::Opened / file associations](https://v2.tauri.app/learn/mobile-file-associations/) and
  [tauri#13159 (fileAssociations lacks LSHandlerRank on macOS)](https://github.com/tauri-apps/tauri/issues/13159)
