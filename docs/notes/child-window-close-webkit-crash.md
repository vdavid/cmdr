# Child-window close crashes the app (macOS WebKit teardown race)

Investigation of a `SIGSEGV` that kills the whole app when a content-heavy child window (Settings, file viewer) is
closed. Mitigated, not root-caused. Verified on macOS 26.5.2 (25F84), aarch64, app 0.35.0 dev build, 2026-07-23.

## Symptom

Pressing Escape to close the Settings window occasionally takes the entire app down instantly. No Rust panic, no error
dialog: the process is simply gone. It is rare enough to look random (roughly one crash per few dozen closes), which is
why it survived an earlier fix attempt and stayed unexplained for months.

## Crash signature

`EXC_BAD_ACCESS (SIGSEGV)`, null-pointer dereference (`KERN_INVALID_ADDRESS` at a small offset such as `0x8` / `0x10`),
always on thread 0, the main thread. The whole stack is inside WebKit; our only frames are the outer tao/wry event loop
pumping the run loop:

```
WebKit::RemoteLayerTreeDrawingAreaProxy::commitLayerTree(...)          <- faults here
  (sometimes one frame deeper: RemoteScrollingCoordinatorProxy::commitScrollingTreeState)
WebKit::RemoteLayerTreeDrawingAreaProxy::didReceiveMessage(...)
IPC::MessageReceiverMap::dispatchMessage / WebProcessProxy::didReceiveMessage
IPC::Connection::dispatchIncomingMessages()
WTF::RunLoop::performWork() -> __CFRunLoopRun -> -[NSApplication run]
tao/tauri_runtime_wry event loop -> cmdr_lib::run -> main
```

Read it as: the **UI process** (ours) is applying a layer-tree commit that a **web content process** sent over IPC, and
the state that commit refers to has already been torn down. Destroying the webview while a commit is in flight is the
race.

## What is and is not the cause

Findings from driving the live app and reading the backend log (`tauri_runtime_wry` logs
`web content process terminated` whenever a renderer dies):

- **It is the close, not the open.** On the original crash the app opened Settings at 21:25:16, kept logging, the user
  navigated to `Indexing > Image indexing` at 21:25:18.777, and only died at ~21:25:20. An open-crash would have died at
  :16.
- **Opening any content-heavy child window always churns a renderer, and that is benign.** Every Settings open logs
  exactly one `web content process terminated` (a WebKit process swap on navigation); wry respawns silently. The file
  viewer does the same. The About dialog (small, non-scrolling) does not.
- **Transparency and vibrancy are NOT involved.** The Settings window is the only `transparent: true` +
  `NSVisualEffectView` window, which made it the obvious suspect. It is wrong: the **file viewer is opaque with no
  vibrancy and churns renderers identically**. Do not spend time here again.
- **Closing is normally clean.** A close logs no renderer termination at all. The crash is a rare race on top of an
  otherwise quiet operation, not a per-close event.
- **Live-updating content raises the odds.** The section in view when it crashed (`Image indexing`) updates on a ~1 s
  media-index tick, so a layer-tree commit is more likely to be in flight at the instant of teardown.

The underlying null dereference is an upstream WebKit bug on macOS 26. We can only remove our trigger.

## Repro harness

The race needs many closes to surface, so drive it. The recipe that worked:

1. Run the app (`pnpm dev -m`) and read the MCP port from `<data dir>/mcp.port`.
2. **Open** via Cmdr's own MCP over HTTP (`POST /mcp`, `initialize` for a session id, then a `tools/call` of `dialog`
   with `action: open, type: settings, section: "Image indexing"`). Do not open via keystroke: it depends on focus and
   is flaky.
3. **Close with a real Escape keypress**, not a programmatic close. A genuine OS key event exercises the actual keydown
   handler and its timing. Programmatic closes (the MCP `dialog close`) go through the same `win.close()` but did not
   reproduce the crash in testing.

   ```bash
   osascript <<'AS'
   tell application "System Events" to tell process "Cmdr"
     set frontmost to true
     try
       perform action "AXRaise" of (first window whose title contains "Settings")
     end try
   end tell
   delay 0.15
   tell application "System Events" to key code 53
   AS
   ```

   `AXRaise` on the Settings window is required. Just making the process frontmost leaves the **main** window key, so
   Escape never reaches the settings keydown handler.

4. Randomise the dwell between open and close (0.2 to 1.5 s) so some closes land mid-commit, and poll
   `pgrep -f 'target/debug/Cmdr'` after each cycle to detect the crash.

macOS writes a fully symbolicated report to `~/Library/Logs/DiagnosticReports/Cmdr-*.ips`. Reach for it first for this
class of crash: it names the **system** frames (WebKit, AppKit) where the fault actually is, which no amount of our own
symbols would give. Our crash reports now carry an `imageBase` alongside the raw addresses, so their frames can be
resolved too (`apps/desktop/src-tauri/src/crash_reporter/DETAILS.md` § Image base), but that only covers frames in our
own binary.

## Measurements

Same harness, same machine, only the close delay changed:

- `setTimeout(close, 0)` (the previous next-tick defer): **crashed on cycle 36**, after 35 clean cycles. The crash
  landed on a 1.40 s dwell, and other 1.40 s dwells passed, confirming it is probabilistic rather than a timing cliff.
- `setTimeout(close, 100)`: **80 consecutive cycles clean, no crash.**

## What shipped

`closeSelfWindow()` (`apps/desktop/src/lib/child-window-close.ts`) asks the backend to hide the window and destroy it
`WINDOW_CLOSE_DEFER_MS` (100 ms) later, in `commands/child_window_state.rs`. Every self-closing child window uses it:
`routes/settings/+page.svelte`, `lib/settings/sections/LicenseSection.svelte`, `routes/viewer/+page.svelte`, and
`routes/debug/+page.svelte`.

The hide is option 1 below, taken after the delay alone proved insufficient: `CRASH-SK5RW` (0.39.0) and `CRASH-V2SCH`
(0.45.1) are both post-mitigation `SIGSEGV`s from one install, sharing a 17-frame outer path through the main run loop
into WebKit's IPC dispatch. Their fault sites differ from the `commitLayerTree` one above, so they are the same family
rather than provably this crash.

The wait runs in RUST rather than in the closing webview, and that is not incidental: WebKit throttles timers in a
hidden page to roughly 1 Hz, so hiding a window and then relying on its own `setTimeout` to close it can leave an
invisible window alive for a second or more, still holding a web content process. Keeping the timer out of the page also
means no `core:window:allow-hide` capability is needed, since nothing calls `hide` from JavaScript.

This is still a mitigation. Pulling the view out of the compositor stops NEW layer-tree commits, but commits already in
flight still land, so the race is narrowed rather than removed.

## If it comes back

Options considered, in rough order of expected effectiveness:

1. ~~**Hide before closing.**~~ Shipped, see above.
2. **Quiesce live sections on close**, so nothing is committing during teardown.
3. **Reuse the window instead of destroying it** (hide on close, show on open). This removes the teardown entirely, so
   it should remove the crash, but it keeps a webview resident for the session, turns "close" into "hide" (the red
   button has to be intercepted too), and retains page state across opens. Deliberately not taken.
4. **Report upstream** to WebKit / Tauri. The null dereference is theirs; we only control when we destroy the view.
