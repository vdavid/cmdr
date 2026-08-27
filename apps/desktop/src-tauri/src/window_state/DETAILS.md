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

### No "currently restoring" flag, by design

Upstream guards its handlers with a `RestoringWindowState` mutex so the events `restore_state` causes don't overwrite
the state being restored. **That guard cannot work**, and we deliberately don't have an equivalent: tao dispatches
`set_position` / `set_size` / `maximize` to the main queue, so they execute on a later turn than the synchronous
`restore()` call that would have cleared the flag, and their `Moved` / `Resized` events arrive unguarded. Any fix
phrased in terms of *when* to clear the flag is a guess about main-queue ordering.

Instead the guard is unnecessary, because `apply_move` is a no-op for exactly the values restore applied:

- **Non-maximized restore**: `set_position(x, y)` comes back as `Moved(x, y)`; recording it writes the same numbers.
- **Maximized restore**: `restore_position` aims at `prev_*` and our state already says `maximized`, so `prev_*` is
  frozen. The subsequent `maximize()` events, including every intermediate frame of the macOS `zoom:` animation, are
  frozen out too.

Upstream's one-step-of-history shuffle is what made this fragile: each animation frame shifted `prev_*` along, so a
restored-maximized window could land on an arbitrary intermediate spot when un-maximized. Tracking "last non-maximized
position" instead is both simpler and immune to how many events arrive. Covered by
`the_zoom_animations_intermediate_moves_cannot_poison_prev` and the two `restore_feedback_*` tests.

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

The plugin had a fourth step, **show + focus**, and we deliberately dropped it. Cmdr shows the main window from the
frontend's `onMount` (`routes/(main)/show-main-on-mount.ts` → `show_main_window`), and that path must stay the only
one: `show_main_window` orders the window to the *back* in E2E mode so test runs don't pop in front of the developer,
so an unconditional `show()` from Rust would make every E2E run steal focus. The focus half isn't lost: a `launch`
show activates the app from inside `show_main_window` (`commands/DETAILS.md`).

Because nothing here acts on it, the saved `visible` flag is recorded but never applied. It stays in the schema so
plugin-written files round-trip unchanged.

### Consequence for startup: the frontend now owns when the window appears

The plugin used to show the window at window-ready, before the frontend ran, which made the frontend's own paint-gated
show a no-op re-show. With the plugin gone the window really is hidden until `showMainOnMount()` shows it, so the
frontend's startup path became load-bearing for the first time.

That path was reshaped to match (see `src/routes/(main)/DETAILS.md` § Startup): the pre-show paint gate is gone, the
window is shown straight from `onMount`, and the paint check moved after the show where it can actually observe
something. Don't reintroduce a pre-show gate here or there.

If no monitor overlaps (the display was unplugged, or the resolution shrank), we leave placement to the OS rather than
restoring a position the user can't reach.

With nothing usable saved, `seed_from_live_window` records wherever the OS put the window, so the first move or resize
edits a sane base rather than `(0, 0)`.

### `prev_x` / `prev_y`

Maximizing parks the window at the monitor corner, and that `Moved` event would otherwise destroy the only record of
where the user actually had it. So `prev_x` / `prev_y` track **the last position while not maximized**: an ordinary
move updates them alongside `x`/`y`, and they freeze for the whole maximized period. A window saved while maximized
restores to them. Without this, un-maximizing after a restart strands the window in the corner.

This differs from upstream, which keeps one step of history (every move shifts the old `x`/`y` into `prev_*`, whether
maximized or not). See "No 'currently restoring' flag" above for why the difference matters.

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
  `onResized` listener in a since-deleted `window-state.ts`. Both are gone: persistence is entirely backend-side now, which is
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
