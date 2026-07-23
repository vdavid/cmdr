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

macOS writes a fully symbolicated report to `~/Library/Logs/DiagnosticReports/Cmdr-*.ips`. That is far more useful than
our own crash report, whose raw addresses cannot be symbolicated (see
`apps/desktop/src-tauri/src/crash_reporter/CLAUDE.md` on the missing binary image base).

## Measurements

Same harness, same machine, only the close delay changed:

- `setTimeout(close, 0)` (the previous next-tick defer): **crashed on cycle 36**, after 35 clean cycles. The crash
  landed on a 1.40 s dwell, and other 1.40 s dwells passed, confirming it is probabilistic rather than a timing cliff.
- `setTimeout(close, 100)`: **80 consecutive cycles clean, no crash.**

## What shipped

`deferWindowClose()` / `WINDOW_CLOSE_DEFER_MS` (`apps/desktop/src/lib/window-close-defer.ts`), used by both self-closing
child windows: `routes/settings/+page.svelte` and `routes/viewer/+page.svelte`.

This is a mitigation: it widens the drain window for in-flight commits and makes the race very unlikely, but it does not
prove the race is gone. 80 clean cycles against a baseline that failed at 36 is decent evidence, not a guarantee.

## If it comes back

Options considered, in rough order of expected effectiveness:

1. **Hide before closing.** `win.hide()` first, then `close()`. Pulling the webview out of the compositor should stop
   new layer-tree commits outright rather than merely waiting them out. Needs `core:window:allow-hide` in
   `apps/desktop/src-tauri/capabilities/settings.json` (only `allow-close` is granted today), so it costs a Rust
   rebuild. This was the next candidate and is untested.
2. **Quiesce live sections on close**, so nothing is committing during teardown.
3. **Reuse the window instead of destroying it** (hide on close, show on open). This removes the teardown entirely, so
   it should remove the crash, but it keeps a webview resident for the session, turns "close" into "hide" (the red
   button has to be intercepted too), and retains page state across opens. Deliberately not taken.
4. **Report upstream** to WebKit / Tauri. The null dereference is theirs; we only control when we destroy the view.
