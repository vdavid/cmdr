# Window state: details

## Why this isn't a plugin anymore

`tauri-plugin-window-state` 2.4.1 could deadlock the entire UI. Its `save_window_state` IPC command took the
`WindowStateCache` mutex and then, still holding it, called `is_maximized()` / `is_minimized()` / `inner_size()` /
`outer_position()`. Each of those round-trips to the main thread. Meanwhile the plugin's own `Moved` / `Resized`
handlers take that same mutex *on* the main thread.

So: an IPC thread holding the lock and waiting for the main thread, and the main thread inside a resize drag waiting for
the lock. Neither can proceed. The backend stays healthy (MCP answers, the indexer keeps writing) while every pixel of
UI is frozen, which makes it look like a frontend hang.

The amplifier that made a rare race certain was a **display reconfiguration**: switching screen resolution fires a burst
of `Moved` + `Resized` events, so the window of vulnerability that's normally a few milliseconds became continuous.

Upstream can't easily fix the shape, only patch it: `save_window_state` is a command callable from any thread, so it
*must* fetch geometry via main-thread hops, so it stays one refactor away from the same inversion. The plugin repo
(`tauri-apps/plugins-workspace`) is alive but the window-state plugin is feature-frozen: every commit touching it since
2025-10 is a dependency bump or a clippy fix.

## The design that removes the bug class

Three rules, all enforced by structure rather than discipline:

1. **Geometry comes from the event payload.** `WindowEvent::Resized(size)` and `Moved(position)` carry exactly what the
   plugin was calling getters to obtain: inner size and outer position respectively. The substitution is exact rather
   than merely equivalent; see "Why the payload substitution is exact" below.
2. **The lock is never held across a window call.** Handlers query flags they can't get from the payload
   (`is_minimized`, `is_maximized`, `is_fullscreen`) *before* taking the lock, then lock only for field assignment.
3. **The writer only sees a snapshot.** `write_to_disk` clones under the lock, releases, then serializes and writes. It
   never touches a window, so it's safe from any thread.

With those, the mutex is only ever held for plain field access, and the inversion has nowhere to form.

### Why the payload substitution is exact, not approximate

Rule 1 is load-bearing, so it's worth recording why it can't drift under a Tauri bump. `tauri-runtime-wry`'s
`WindowEventWrapper::parse` **discards tao's `Resized` payload and recomputes it**, because tao's size is unreliable on
macOS once wry replaces the `NSView`:

```rust
// resized event from tao doesn't include a reliable size on macOS
TaoWindowEvent::Resized(_) => {
    let size = inner_size(w, &window.webviews, window.has_children.load(...));
    ...
}
```

`WindowMessage::InnerSize`, which is what `window.inner_size()` resolves to, calls that same free function with the
same arguments. So the payload isn't merely equivalent to the getter, it *is* the getter's result. `Moved` passes
tao's payload through, and tao computes it (`NSWindow.frame` origin → `bottom_left_to_top_left` →
`to_physical(backingScaleFactor)`) identically to `outer_position()`. Verified against `tauri-runtime-wry` 2.11.4 and
`tao` 0.35.3, 2026-07-28.

Two macOS behaviors that follow from the same reading: `Moved` is **deduped** against the previous position (a no-op
move emits nothing), and `windowDidResize` emits **both** `Resized` and `Moved`, so an ordinary edge-drag resize churns
`prev_x`/`prev_y` too. Upstream behaves the same way.

### The restoring flag, and its hole

`restoring` suppresses the events `restore()` causes. It only covers the synchronous window, which on macOS usually
isn't enough: tao dispatches `set_position` / `set_size` / `maximize` to the main queue, so they execute on a later
turn than the `restore()` call that already cleared the flag, and their events arrive unguarded. Upstream's
`RestoringWindowState` `try_lock` has the identical hole, so this is not a regression.

What it costs, traced:

- **Non-maximized restore**: the `Moved` sets `prev_* = x,y` and leaves `x,y` correct. `prev_*` is only read when
  maximized, so this is harmless.
- **Maximized restore**: the sequence self-heals, because `maximize()` then emits `Moved(corner)` which pushes the real
  pre-maximize position back into `prev_*`. But macOS animates `zoom:`, `windowDidResize` fires repeatedly, and each
  emits a `Moved`, so `prev_*` can end up holding an intermediate animation frame. The visible effect is a
  restored-maximized window landing slightly off its old spot when un-maximized.

Not worth a timing hack to close (any fix is a guess about main-queue ordering), but don't trust the flag for anything
stronger than it claims.

### Why main-thread getters are safe at all

`tauri-runtime-wry`'s `send_user_message` handles a message inline when the caller is already on the main thread, and
falls back to the event-loop proxy otherwise. That's why the plugin's event handlers could call `is_maximized()`
without hanging while the IPC-thread path blocked on a dispatch semaphore. We still keep rule 2 as an invariant: it
makes the code correct regardless of which thread a future caller arrives on.

### Debounced writing

`WRITE_DEBOUNCE` (750 ms) coalesces a drag into one write. The flusher subscribes on a `tokio::sync::Notify` rather
than polling, so an idle app does no work. `RunEvent::Exit` and `CloseRequested` write synchronously, so the last
adjustment isn't lost to the debounce window.

The write goes through `config::durable_write_json` (temp + fsync + rename + parent fsync), same as the favorites and
known-shares stores. A torn write here would lose the user's window position, which is small but annoying.

## Restore

Order matters, and it's the order upstream used:

1. **Position**, but only if the saved rect still overlaps a connected monitor. Position before size, because sizing a
   window that's about to move can bounce it between monitors with different scale factors.
2. **Size**, skipped for a zero dimension.
3. **Maximize**, then **fullscreen**.

The plugin had a fourth step, **show + focus**, and we deliberately dropped it. Cmdr already shows the main window from
the frontend, gated on a confirmed first paint (`routes/(main)/show-main-when-painted.ts` → `show_main_window`), and
that path must stay the only one:

- Showing before the compositor presents a frame can leave the window blank until something forces a repaint. That was
  observed on a cold prod launch during a heavy reindex, which is why the paint gate exists.
- `show_main_window` orders the window to the *back* in E2E mode so test runs don't pop in front of the developer. An
  unconditional `show()` from Rust would make every E2E run steal focus.

Because nothing here acts on it, the saved `visible` flag is recorded but never applied. It stays in the schema so
plugin-written files round-trip unchanged.

### Known consequence: the paint gate now always times out

Dropping the backend show changed which branch of `showMainWhenPainted()` runs, and it's worth knowing before someone
"fixes" the resulting log line.

The plugin used to show the window at window-ready, i.e. before the frontend ran. The window was therefore already
visible by the time `showMainWhenPainted()` executed, `requestAnimationFrame` fired normally, paint was confirmed in a
few ms, and its `show()` was a no-op re-show. The paint gate was, in practice, gating a call that no longer mattered.

Now the window really is hidden until the frontend asks for it, and **WebKit throttles `requestAnimationFrame` in a
hidden window**, so the paint can never be confirmed. Every launch takes the `FIRST_PAINT_TIMEOUT_MS` (1000 ms)
fallback and logs:

```
WARN FE:startup  First paint not confirmed within 1000ms; showing the main window anyway (it may briefly appear blank)
```

Verified on macOS 15, dev build, 2026-07-28. That warning was written to flag a rare event, so it's now misleading, and
the window appears ~1s later than it strictly needs to. The gate can't do its job while the window is hidden, so the
options are to show on mount without the rAF gate, or keep a much shorter timeout and demote the log to `debug`. That's
a startup-UX call, deliberately left to David rather than changed as a side effect of removing the plugin.

If no monitor overlaps (the display was unplugged, or the resolution shrank), we leave placement to the OS rather than
restoring a position the user can't reach.

With nothing usable saved, `seed_from_live_window` records wherever the OS put the window, so the first move or resize
edits a sane base rather than `(0, 0)`.

### `prev_x` / `prev_y`

Maximizing moves the window to the monitor corner, and that `Moved` event would otherwise destroy the only record of
where the user actually had it. So every move shifts the old position into `prev_x` / `prev_y`, and a window saved
while maximized restores to the *previous* position. Without this, un-maximizing after a restart strands the window in
the corner.

Note `is_on_any_monitor` checks the position the window will actually land on (via `restore_position`), not the raw
`x`/`y`, so a maximized window is validated against its pre-maximize spot.

## Deliberate differences from the plugin

- **Rectangle overlap instead of a corner test.** Upstream asked whether any of the window's four corners fell inside a
  monitor. A window *larger* than its monitor has no corner inside it, so upstream silently discarded the position. We
  test rectangle overlap, which is simpler and handles containment in either direction. Covered by
  `window_larger_than_its_monitor_still_overlaps`.
- **No `decorated` field.** Cmdr's main window is always decorated (`titleBarStyle: Overlay`), so restoring a saved
  value could only ever break the title bar. Serde ignores the field when reading a plugin-written file and drops it on
  the next write.
- **No IPC command, no frontend module.** The plugin exposed `save_window_state` to JS, and we drove it from a debounced
  `onResized` listener in `src/lib/window-state.ts`. Both are gone: persistence is entirely backend-side now, which is
  also what the "smart backend / thin frontend" principle asks for. That removed the `window-state:default` permission
  and with it the whole `capabilities/desktop.json` file, whose only content it was.
- **Path resolution via `config::resolved_app_data_dir`** (honors `CMDR_DATA_DIR`) rather than `app_config_dir()`. On
  macOS both resolve to `~/Library/Application Support/<identifier>/` in production, so **upgrading users keep their
  window position**; in dev it additionally isolates per worktree session. See `docs/tooling/instance-isolation.md`.
- **Multi-window machinery dropped**: `map_label`, denylist, `skip_initial_state`, and the state-flag bitfield. Only
  `main` is persisted, so they were dead configuration.

## On-disk schema

`.window-state.json`, a map keyed by window label (kept as a map purely for compatibility with plugin-written files,
even though only `main` is ever present). All values are **physical pixels**.

- `width`, `height` (`u32`): inner size.
- `x`, `y` (`i32`): outer position.
- `prev_x`, `prev_y` (`i32`): outer position before the last move. See above.
- `maximized`, `visible`, `fullscreen` (`bool`).

Missing fields fall back to `WindowGeometry::default()`, where `visible` is `true`: a window we've never seen must not
restore hidden, because nothing would ever show it.

## Attribution

Ported and modified from `tauri-plugin-window-state` v2.4.1, Copyright 2019-2023 Tauri Programme within The Commons
Conservancy. The upstream crate is dual-licensed `Apache-2.0 OR MIT`; **we take it under MIT**, whose only condition is
retaining the copyright and permission notice. The restore ordering, the `prev_x`/`prev_y` trick, and the on-disk
schema are theirs; the threading model and the overlap test are not. Upstream ships no `NOTICE` file, so there's
nothing to propagate.

Both source files carry the copyright line and say they're modified ports, per MIT and per Apache-2.0 §4(b) had we
elected that instead.
