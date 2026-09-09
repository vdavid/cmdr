# Dock tile menu: details

## The menu, as it renders

```
Open Cmdr                      macwindow
Search files…                  magnifyingglass
──────────────────────────
Go to folder…                  arrow.right.to.line
Connect to server…             externaldrive.connected.to.line.below
──────────────────────────
<up to 12 bookmarks>           star
──────────────────────────
<up to 12 tabs>                folder
──────────────────────────     ← macOS's own, from here down
Options                    ▸
Show All Windows
Quit
```

A group with no rows takes its separator with it, so a fresh install with no favorites
and no tabs shows exactly the four commands. `Show All Windows` stays on purpose: it is
how someone reaches one particular file-viewer window, which nothing in our half offers.

`magnifyingglass` and `arrow.right.to.line` are the glyphs the menu bar already gives
`Search files…` and `Go to path…`; the same concept gets the same glyph.

## Why the menu can't stall the Dock

`applicationDockMenu:` is called by AppKit on the main thread, synchronously, while the
Dock holds the user's mouse-down. Everything the build touches was picked for that.

- **Bookmarks**: `favorites::store::list_cached()`. `try_lock` on the store's
  in-memory cache, answering `None` rather than waiting. Safe because that cache mutex
  is never held across I/O: `mutate_and_persist` clones under it and drops the guard
  before touching disk, and `load_or_seed` takes the separate `disk_lock` with the cache
  guard already dropped. ❌ The ordinary `list()` is off limits: on a cold cache it
  reads the file, seeds it, and holds the disk lock while it does.
- **Tabs**: `mcp::pane_state::PaneStateStore::tabs_focused_first()`. Three `try_read`s
  (focused pane, then each pane), cloning only the `Vec<TabInfo>` and never the pane's
  `files`. The writers are the frontend's own pushes, which move a value in memory and
  nothing else, so contention is already vanishingly unlikely; `try_read` makes it
  impossible to matter. This is the backend's live mirror of the panes, so the menu
  shows what is open right now rather than what the last session persisted.
- **The home directory**: resolved ONCE in `install`, on the blocking pool, into a
  `OnceLock`. `dirs::home_dir` falls back to `getpwuid_r` when `$HOME` is unset, and on
  a directory-service-managed Mac that can reach the network.
- **The labels**: `menu_t`, whose fast path is a read of a cached locale plus a binary
  search of a static table. The menu bar has warmed it ~150 times before the Dock ever
  asks, and `menu_t` never panics by contract.
- **Nothing else.** ❌ No `stat`, no `exists()`, no `read_dir`, no `AppHandle` call that
  waits. `sources::tabs` calls `app.try_state`, which is a type-map lookup that no
  thread holds across I/O.

`install` also warms the favorites cache on the blocking pool, so the first right-click
of a session already has bookmarks rather than an empty group.

## Why a panic there is worse than a missing menu

Both callbacks are `extern "C-unwind"` functions Objective-C calls. A Rust panic
unwinding across that boundary is undefined behavior, not a clean crash: the process
can end up in a state where nothing is trustworthy, including the file operations Cmdr
exists to do safely. So the ENTIRE body of each sits in `catch_unwind`:

- `dock_menu` returns nil on the panic path. macOS then draws its own default Dock menu,
  which is exactly what shipped before this feature.
- `item_clicked` returns void, so a panic simply ends there and the click does nothing.

`objc2::exception::catch` wraps the `NSMenu` construction on top, because an ObjC
exception is a *foreign* exception `catch_unwind` cannot see. Same discipline, and the
same layering, as `menu/macos_appkit.rs::cleanup_macos_menus`.

Each failure path logs through `warn_once`, so a defect that recurs on every right-click
costs one line per session rather than one per click.

### What the tests actually prove

`rows/tests.rs` is where the panic story is tested, because `rows.rs` is where the
untrusted data goes: `absurd_names_and_paths_produce_a_menu_rather_than_a_panic` runs a
100,000-character label, an empty name, control characters, emoji with ZWJ sequences, a
replacement character, `/`, `//`, a 2,000-level path, and both `~` forms through
`menu_rows` and asserts a menu comes out. This module's own `tests.rs` covers the callback shape: a
panicking body becomes `None` (the nil the callback returns), a tag past the end of the
row list selects nothing, and a negative tag is refused before it can index anything.

The AppKit half is not unit-tested: it needs a real `NSApplication` on a real main
thread, which a `cargo test` binary hasn't got. That is the same limit
`menu/macos_appkit.rs` works under, and the same answer: keep everything worth
asserting on the pure side of the line.

## Memory: the returned menu is autoreleased

`applicationDockMenu:` doesn't begin with `alloc` / `new` / `copy` / `mutableCopy`, so
by Cocoa's naming convention the caller does NOT own what it returns. `dock_menu` hands
the menu over with `Retained::autorelease_return`, ❌ never `Retained::into_raw`, which
would leak one retained `NSMenu` per right-click.

## Dedup and disambiguation

Both lists are deduplicated by normalized path (one trailing separator stripped, but
never the root's), against ONE shared `seen` list, so a tab pointing at a folder a
bookmark already offers is dropped rather than drawn twice. Finder's own Dock menu lists
"Applications" twice with nothing to tell the two apart; not copying that wart was the
explicit goal.

Names are then made unique in two passes, weakest change first:

1. A name more than one row carries gets qualified by the folder holding it, through the
   `menu.dock.locationInParent` key so a translator decides the wording (several
   languages don't put the qualifier in trailing parentheses).
2. Anything still sharing a `(name, parent)` pair falls back to its whole path. Paths
   are unique by this point, so this always settles it.

A row's name is the favorite's own label when it has one, else the path's last
component, else the path itself (which is what `/` gets). The user's label is what
collides, not the folder underneath it: two favorites both called "Work" get qualified
even though they point at differently-named folders.

`MAX_LOCATIONS_PER_GROUP` is 12 per group. Both lists are user-curated and rarely reach
double digits, but nothing stops someone keeping 200 favorites, and a Dock menu running
off the screen is worse than one that stops early. The cap applies to each group after
dedup, so 12 bookmarks and 12 tabs is the ceiling.

## Which paths are offered

Shape only. An absolute path is taken as it is; `~` and `~/…` are expanded against the
cached home. Everything else is dropped: `search-results`, `mtp://…`, a relative path,
an empty string. Those are the virtual locations no Dock click could navigate to anyway,
and testing them by shape keeps the "never a syscall" rule intact.

A consequence worth knowing: a bookmark or tab whose folder has since been deleted,
unmounted, or renamed still appears, and clicking it lands on the pane's ordinary
"couldn't open that" path. That is the deliberate trade: the alternative is a `stat`,
and a `stat` on a wedged mount is a beachballed Dock.

## How a click gets out

Each `NSMenuItem` carries its index in the built row list as its AppKit `tag`, and the
rows live in a `thread_local!` `RefCell` (`SHOWING`) written when the menu is handed to
AppKit. A thread-local rather than a lock because both the build and the click run on
the main thread, so there is nothing to contend with and nothing that could block.
It is the same shape `menu/context_menu_icons.rs`'s `ARMED` uses.

Every click raises the main window first (`unminimize` → `show` → `set_focus`, the
sequence `downloads/global_shortcut.rs` uses and for the same reason: a Dock right-click
happens with Cmdr in the background, so a dialog opened without raising would land
behind whatever the user is looking at). Then:

- `Open Cmdr`: raising the window WAS the command.
- The other three: `ExecuteCommand { command_id }` to `"main"`, with `search.open`,
  `nav.goToPath`, or `servers.connect`.
- A bookmark or a tab: `RevealPath { path }` to `"main"`, which
  `routes/(main)/listener-setup.ts` already listens for and turns into
  `revealFolderInFocusedPane`.

Both events already existed and both already had a main-window listener, so this
milestone adds **no IPC type, no binding, and no frontend code**.

### Decision: our own selector, not `handle_menu_event`

A muda-built menu would have routed through Tauri's global menu-event handler and into
`menu/menu_handlers.rs::handle_menu_event`, which drops every `CommandScope::FileScoped`
command unless the main window has focus. `nav.goToPath` and `search.open` are both
file-scoped, so every Dock click would have silently done nothing. Prefix-routing them
in their own branch ahead of the unified dispatch would have worked, but hand-building
the menu removes the problem rather than working around it, and it was already the
better answer for two other reasons: no muda pin to keep in sync with Tauri's, and
`macos_appkit::set_sf_symbol` works directly on the bare `NSMenuItem`s.

### Decision: the delegate is its own target

The two selectors go on tao's live `TaoAppDelegateParent`, which implements neither, so
this is a plain `class_addMethod` with no swizzle and no original to forward to.
`drag_image_detection.rs` is the house precedent for reaching into a foreign class, and
this is the easier half of it. Using the delegate as the click target too means no new
Objective-C class anywhere in the repo (there is none today), and the target outlives
every menu built from it by construction: `NSApp` retains its delegate for the life of
the process.

`add_method` refuses to replace an existing implementation and logs loudly instead. If a
tao upgrade ever grows `applicationDockMenu:`, the answer is a swizzle that forwards to
theirs; silently overwriting it would break whatever they added it for.

`setAutoenablesItems(false)` on our menu is load-bearing: with autoenabling on, AppKit
asks the target for `validateMenuItem:`, the app delegate doesn't implement it, and
every row comes up grey.

## Deliberately out of scope

- **A "Connected devices" submenu.** The full volume list
  (`volume_listing::list_with_timeout`) is `async`, and there is no awaiting inside a
  synchronous AppKit callback. Doing it properly means caching a projection off
  `volume_broadcast::do_emit` and reading that; the plan is in
  `docs/specs/dock-integration.md` § B.
- **Creating the main window when it's missing.** Closing the main window quits the
  whole app (`app_lifecycle::on_window_event`), so a running process without one is a
  state nothing produces. `raise_main_window` logs and returns rather than carrying a
  window builder nothing can reach.
- **Anything below `Show All Windows`.** macOS owns that half and we don't touch it.

## Still unverified

- **The menu has not been seen on a real Dock.** Everything up to the AppKit boundary is
  tested, and the boundary itself follows the shapes `drag_image_detection.rs` and
  `macos_appkit.rs` already use in production, but no run has right-clicked the tile.
  What to look for: the four commands in order, the two groups, icons that follow light
  and dark, and macOS's own three items still below ours.
- **Whether `set_sf_symbol` renders on a Dock menu the way it does on the menu bar.**
  The API is the same `NSMenuItem.setImage:`, and SF Symbols are template images, so it
  should; a Dock menu is drawn in its own context and that hasn't been checked.
