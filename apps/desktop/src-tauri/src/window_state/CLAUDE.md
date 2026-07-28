# Window state

Persists the main window's size and position across launches, in `.window-state.json` in the data dir. Replaces
`tauri-plugin-window-state`, which deadlocked the UI (see `DETAILS.md`).

## Module map

- `mod.rs`: Tauri wiring. Load + `init`, `restore` (apply saved geometry, then show), `track` (Moved/Resized/Close
  handlers), `save_on_exit`, and the debounced disk writer.
- `geometry.rs`: pure data + rules, no Tauri types. `WindowGeometry`, monitor-overlap, maximize bookkeeping. All the
  unit tests live here.

## Must-knows

- **Never hold the `geometries` lock across a window call.** Window getters and setters round-trip to the main thread,
  and the main thread takes that same lock in the event handlers, so holding across one deadlocks the whole UI. That is
  exactly the bug this module was written to remove; don't reintroduce it. Every handler queries first, then locks for
  plain field assignment.
- **Take geometry from the event payload, not from a getter.** `WindowEvent::Resized(size)` and `Moved(position)`
  already carry what you'd otherwise call `inner_size()` / `outer_position()` for. Fewer round-trips and no lock
  inversion available to get wrong.
- **❌ Never call `show()` from here.** `restore()` does placement only. The frontend owns showing the main window,
  gated on a confirmed first paint (`routes/(main)/show-main-when-painted.ts` → the `show_main_window` command). Showing
  before the compositor presents a frame can leave the window blank until something forces a repaint, and
  `show_main_window` orders the window to the *back* under E2E so test runs don't steal focus. A `show()` here defeats
  both. The saved `visible` flag is therefore recorded but never acted on.
- **Only `main` is tracked.** Settings, Debug, and viewer windows deliberately reset each launch; their in-session
  position lives in `child_window_state.rs`. `track()` no-ops for other labels.
- **The `restoring` flag is load-bearing.** `set_position` / `set_size` come back as `Moved` / `Resized` events; without
  the flag the handlers would write the restored values back over the state mid-read.

Design rationale, the deadlock post-mortem, the on-disk schema, and what we deliberately dropped from the plugin:
`DETAILS.md`.
