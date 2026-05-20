# Quick Look — implementation plan

## What we're building

A Finder-style Quick Look for Cmdr:

- **Shift+Space** opens a native macOS preview panel over Cmdr.
- The panel shows the file under the cursor in the focused pane.
- Focus stays on Cmdr's main window. Arrow keys keep navigating the file list, and the panel updates live to track the
  cursor.
- Switching the focused pane (or focused tab) also updates the panel — cross-pane "follows the cursor" out of the box.
- **Shift+Space again, or Esc**, closes the panel. Closing the panel via its own ✕ button also clears our open-state.
- The panel is a real `QLPreviewPanel` from our process — menu bar says "Cmdr", thumbnails/icons match what Finder
  shows, plugins handle whatever file types macOS knows about.

## Why we're doing it this way

The user's instinct ("can we just wire `qlmanage` up?") would work for a v0 demo but fails every UX requirement they
listed:

| Approach                      | "Cmdr" in menu bar?  | Focus stays on Cmdr? | Cursor-follow as we navigate?            | Plugin support?               |
| ----------------------------- | -------------------- | -------------------- | ---------------------------------------- | ----------------------------- |
| `qlmanage -p` (today)         | No — says "qlmanage" | No                   | No (separate process, we can't drive it) | Yes                           |
| Tauri window with custom HTML | Yes                  | Yes                  | Yes                                      | No (we'd reinvent every type) |
| **`QLPreviewPanel` (native)** | **Yes**              | **Yes** (see below)  | **Yes** (data source + `reloadData`)     | **Yes**                       |

`QLPreviewPanel` is the same AppKit API Finder uses. It lives in `QuickLookUI.framework`. The crate
`objc2-quick-look-ui` (v0.3.2) exposes it for the objc2 ecosystem we already use (`objc2 = "0.6"`, `objc2-app-kit`,
`objc2-foundation` are all in `Cargo.toml`). So this is a small, well-typed binding addition, not a hand-rolled FFI
adventure.

**Focus / keyboard caveat (read this — this is the structural decision).** `QLPreviewPanel` is the standard AppKit
preview panel, and it _always_ becomes the key window when shown — `makeKeyAndOrderFront:` is the documented entry
point. `orderFront:` alone isn't a supported "show without becoming key" mode: the panel will silently force itself key
on first interaction, and on top of that it won't respond to its own Esc or spacebar dismissal, because those are
processed via key-window event routing. **We commit upfront to `makeKeyAndOrderFront:` + key forwarding.** There is no
"v1 without forwarding" — we'd just rediscover the limitation and rewrite, and the forwarding plumbing is small.

The forwarding mechanism is what makes the UX feel identical to Finder. `QLPreviewPanel` routes key events to its
delegate via `previewPanel:handleEvent:` (BOOL return). The delegate **filters**:

- **Esc**: return NO. Let `QLPreviewPanel` handle it natively — the panel closes, our `windowWillClose:` hook fires, we
  emit `quick-look-closed`, frontend flips `isOpen = false`. Same path as ✕-button close. Forwarding Esc just to
  reimplement the close ourselves would be busywork.
- **Arrow keys, Page Up/Down, Home/End, type-to-jump letters, Shift+Space**: emit Tauri event `quick-look-key` with
  payload `{ key, code, shiftKey, metaKey, altKey, ctrlKey }`, return YES.
- **Other events** (mouse, modifier-only keydowns, etc.): return NO; let the panel handle natively.

On the frontend, `quick-look-state.svelte.ts` listens for `quick-look-key` and **calls the navigation API directly on
the focused pane** — `explorerRef.routePanelKey(payload)` — which dispatches to the same primitives FilePane's own
`keydown` handler uses (`applyNavigation`, type-to-jump's `feed`, `selection.toggleAt`, etc.). We **do not** re-dispatch
synthetic `KeyboardEvent`s on the pane element: `dispatchEvent(new KeyboardEvent(...))` produces `isTrusted: false`
events, which have suppressed default actions and can interact poorly with `beforeinput` / composition handlers and our
centralized Tier-1 dispatch. A trusted explicit API call is the cleaner cut.

**Shift+Space close** is a special case: rather than route through the navigation API, the listener checks for
`shiftKey && key === ' '` first and calls `quickLookClose()` + flips `isOpen` directly. The menu-accelerator route isn't
reliable when the panel is key (the panel may consume the keydown before AppKit's menu dispatcher sees it), so the
Tauri-event path is the authoritative close-via-shortcut mechanism while the panel is up.

**Implementation note for the navigation API**: `FilePane.svelte` already has the primitives (`applyNavigation`,
selection toggles, type-to-jump). We add a small `routePanelKey(payload)` wrapper on `explorerRef` that fans out to the
focused pane's existing handlers. Keep it narrow — don't leak internal handler state through `explorerRef`. The wrapper
exists so the listener doesn't have to know which pane is focused or which primitive to call for `ArrowDown` vs
`PageUp`; FilePane already encodes that logic.

We also don't forward via direct AppKit calls (`[contentView keyDown:]`) because WKWebView's key handling is sensitive
to window key state, and we'd be poking at Tauri internals.

**Volumes that don't have a real file path.** `QLPreviewPanel` wants an `NSURL` to a local file. Local + SMB-mounted
paths Just Work. MTP paths don't have a filesystem path at all. For v1 we **no-op on non-local-path volumes** and log
debug; future work can stream to a temp file for MTP. This matches our principle "platform-native, not generic" — we
shouldn't fake a worse preview for MTP just to feel uniform.

## What's already there

We have more than I expected. Big chunks of this exist as wiring for the `qlmanage` shell-out; the work is mostly
**replacing the engine, not rewiring the controls**.

- **Command registry entry** `file.quickLook` already exists (`command-registry.ts:385`). `showInPalette: true` on
  macOS, name is `'Quick look'`, shortcuts list is empty — we'll set it to `['⇧Space']`.
- **`handleCommandExecute`** dispatch case for `file.quickLook` already exists at
  `routes/(main)/command-dispatch.ts:399`. It reads the cursor entry and calls our `quickLook()` IPC wrapper.
- **Tauri command** `quick_look(path)` already exists at `commands/ui.rs:290`. **This is what we rewrite.**
- **Menu wiring** — there's already a "Quick look" menu item with `Some("Space")` as its macOS accelerator
  (`menu/macos.rs:100`). **We'll change this to `Some("Shift+Space")`.** The reverse-lookup tables in `menu/mod.rs` and
  the `menuCommands` array in `shortcuts-store.ts` already include `file.quickLook`, so the accelerator-sync mechanism
  picks up the customized shortcut for free.
- **`objc2` deps** are already in `Cargo.toml` (`objc2`, `objc2-foundation`, `objc2-app-kit`). We add
  `objc2-quick-look-ui`.

## What's broken / wrong right now

- **Shift+Space currently triggers `selection.toggleAt`.** The Space handler at
  `file-explorer/pane/FilePane.svelte:1604` checks `e.key === ' '` and ignores modifiers. So Shift+Space toggles
  selection. We have to gate this: `if (e.key === ' ' && !e.shiftKey)`.
- **Plain Space.** The menu accelerator `Some("Space")` on `quick_look_item` is dead today: the webview captures plain
  Space first (it's in our Tier 2 handlers in FilePane), so the AppKit menu accelerator never fires. Modifier-combo
  accelerators behave the opposite way — AppKit consumes them _before_ the webview sees the keydown, which is why `⌘⇧P`
  (Open command palette) works as a menu accelerator without double-dispatching against its registry shortcut. Changing
  to `Shift+Space` puts us in that regime: AppKit fires the menu item, `on_menu_event` emits
  `execute-command file.quickLook`, frontend dispatch handles it, JS shortcut path stays dormant. **No double-dispatch**
  — that's how every other modifier-shortcut menu item in this app already works.
- **`quick_look(path)` is fire-and-forget single-shot.** No notion of "the panel is open, here's a new path." The
  controller needs to be a singleton that holds the open state.

## Implementation strategy

**Two big shifts from today's code:**

1. **Backend gains a stateful `QuickLookController`** (singleton owned by Tauri's `AppHandle`) that implements
   `QLPreviewPanelDataSource` and `QLPreviewPanelDelegate`. The IPC surface goes from one command (`quick_look(path)`)
   to three: `quick_look_open(path)`, `quick_look_set_path(path)`, `quick_look_close()`. The frontend chooses which one
   to call based on whether the panel is open.
2. **Frontend gains a tiny `quickLookState`**
   ($state) plus a `$effect`that pushes path updates to the backend any time`isOpen && (focusedPane, cursorIndex,
   currentEntry)`change. Closing the panel from inside the panel itself (✕ button, Esc routed through AppKit) needs a Tauri event`quick-look-closed`so we can flip`isOpen`
   back to false.

### Why a singleton controller (and not "create a new panel each time")

`QLPreviewPanel` is a process-wide shared singleton in AppKit — there's literally `+[QLPreviewPanel sharedPreviewPanel]`
and you don't get to make your own. The "open it" call is really "set ourselves as the panel's dataSource + delegate,
then `makeKeyAndOrderFront`." The "close it" call is `orderOut:`. So our struct is just bookkeeping: "are we the current
controller? what path are we showing?". Living in `Mutex<QuickLookController>` behind a `tauri::State` is enough. We set
`dataSource` and `delegate` directly rather than going through the responder-chain `QLPreviewPanelController` discovery
path — Tauri's window-delegate ownership makes inserting ourselves into the chain awkward, and direct assignment is both
legal and what the Apple sample code calls out as the simple route when you own the panel's lifecycle.

### Why all AppKit calls hop to the main thread

Every `QLPreviewPanel`, `NSURL`, and panel-window call has main-thread affinity. Tauri commands run on a Tokio worker
thread. Doing the AppKit work on the worker is undefined behavior — sometimes "works," sometimes crashes inside objc2 on
the next pump. We use the same `app.run_on_main_thread()` + `mpsc` channel pattern as `clipboard/pasteboard.rs` and
`commands/file_system/drag.rs::run_drag_on_main_thread`. The `Mutex<QuickLookController>` guards data; the main-thread
hop guards AppKit affinity. Both layers are needed.

### Why a `quick-look-closed` event, not polling

Principle "subscribe, don't poll." The panel can be closed three ways: our IPC call, the user clicking ✕ on the panel,
or AppKit's Esc handling. The latter two happen entirely inside AppKit — we find out via the delegate callback
(`windowWillClose:` on the panel, or `endPreviewPanelControl:` if we ever migrate to the controller-protocol entry
point). We `app.emit("quick-look-closed", ())` (broadcast — there's only one panel and the listener lives in the main
window, but broadcasting is the simplest scope and doesn't cost us anything); the frontend listens once and flips
`isOpen = false`.

### Why we forward keys via a Tauri event, not direct AppKit forwarding

`previewPanel:handleEvent:` is the delegate hook for panel-to-controller key routing. Inside it, the "obvious" move is
to forward the `NSEvent` to our main window's content view via `[contentView keyDown:event]`. We don't, because
WKWebView's keydown handling depends on the window being key, and ours isn't (the panel is). Routing through a Tauri
event (payload: `{ key, shiftKey, metaKey, altKey, ctrlKey, code }`) and re-dispatching a synthetic `KeyboardEvent` on
the focused pane in the frontend gives us:

- a clean, testable IPC boundary (no AppKit poking on the frontend);
- compatibility with our existing FilePane / DualPaneExplorer key handlers (no special "is the panel open?" branch in
  any keydown handler — the cursor moves, the `$effect` notices, the preview follows);
- no risk of the WKWebView no-key-window edge case eating arrow keys silently.

## Milestones

### M1 — Native panel, single-shot

Goal: Shift+Space opens a real `QLPreviewPanel` for the file under the cursor. No cursor-follow yet. Closing works.

**Do this verification first, before writing any Rust.** Skipping it costs a day:

1. Open `objc2-quick-look-ui` on docs.rs and confirm it exposes _implementable_ protocol traits for both
   `QLPreviewPanelDataSource` and `QLPreviewPanelDelegate` (i.e. they can be implemented from Rust via `declare_class!`
   or equivalent), not just opaque marker traits. If they're marker-only, plan to declare the protocols manually with
   `extern_protocol!` — same pattern as the objc2 FFI in `menu/macos.rs::cleanup_macos_menus`. It's a one-day delta in
   either direction; flag it now.
2. Confirm `objc2-quick-look-ui` 0.3.2 resolves against `objc2-app-kit = "0.3"` and `objc2-foundation = "0.3"` already
   in our Cargo.toml. Mismatched minor versions across the objc2 family produce baffling trait-bound errors at the crate
   boundary.
3. Confirm `~/.claude/rules/use-latest-dep-versions.md` 14-day-old constraint is met (0.3.2 is well outside).

Then implement:

- **Cargo.toml** (`apps/desktop/src-tauri/Cargo.toml`): add
  `objc2-quick-look-ui = { version = "0.3.2", features = [...] }` with the `QLPreviewPanel`, `QLPreviewItem`, and any
  panel-window features needed. Gate with `[target.'cfg(target_os = "macos")'.dependencies]` so non-macOS builds aren't
  affected.
- **New module** `src-tauri/src/quick_look/mod.rs`:
  - `pub struct QuickLookController { current_url: Option<PathBuf>, is_open: bool, app: AppHandle<Wry> }`.
  - `QuickLookDelegate` NSObject implementing:
    - `QLPreviewPanelDataSource`: `numberOfPreviewItems` returns 1, `previewItemAtIndex` returns an `NSURL` from
      `current_url`.
    - `QLPreviewPanelDelegate`: `previewPanel:handleEvent:` filters NSEventTypeKeyDown, emits Tauri event
      `quick-look-key` with payload `{ key, code, shiftKey, metaKey, altKey, ctrlKey }`, returns YES. Returns NO for
      other event types (lets the panel handle clicks, etc.).
    - Panel-window close: hook `windowWillClose:` (or NSNotificationCenter on `QLPreviewPanelWillCloseNotification`,
      whichever the binding exposes cleanly) and emit `quick-look-closed`.
  - `open(path)`: sets `current_url`, sets panel `dataSource`/`delegate` to our singleton, calls
    `panel.makeKeyAndOrderFront(nil)`, sets `is_open = true`. **Wrap every AppKit call in `app.run_on_main_thread()` via
    an `mpsc` channel** — same pattern as `clipboard/pasteboard.rs` and `run_drag_on_main_thread`.
  - `set_path(path)`: sets `current_url`, calls `panel.reloadData()`. No-op if `!is_open`. Main-thread hop required.
  - `close()`: `panel.orderOut(nil)`, `is_open = false`. Main-thread hop required.
  - All log calls use `target: "quick_look"` (per AGENTS.md "Critical rules" and `logging/CLAUDE.md`).
- **Replace `commands/ui.rs:quick_look`** with three commands routed through `QuickLookController` behind
  `tauri::State<Mutex<QuickLookController>>`. Make them `async` and wrap in `blocking_with_timeout` (2s) so a wedged
  AppKit call never freezes the IPC thread — consistent with the platform constraint in `docs/architecture.md`.
  Stub-no-op variants for `#[cfg(not(target_os = "macos"))]`.
- **Volume gate**: `quick_look_open(path, volume_id)` and `quick_look_set_path(path, volume_id)` take a
  `volume_id: String` second argument. In the backend, look up the volume and call `Volume::supports_local_fs_access()`
  (defined at `file_system/volume/mod.rs:600`); if false, log debug and return early. **Do not** use `Path::exists()` —
  MTP paths look like real paths and `exists()` returning false is indistinguishable from "file genuinely deleted." The
  volume kind is the correct gate.
- **Register commands** in `ipc.rs` and `ipc_collectors.rs` (both list `quick_look` today; replace the one entry with
  three).
- **Capabilities**: `apps/desktop/src-tauri/capabilities/default.json` — verify whether the existing single `quick_look`
  command is listed there. If yes, replace with the three new names. If not, the new commands also don't need explicit
  caps. (Quick `rg quick_look src-tauri/capabilities/` confirms this in one step.)
- **Regenerate bindings**: `cd apps/desktop && pnpm bindings:regen`. CI's `bindings-fresh` check (at
  `scripts/check/checks/desktop-bindings-fresh.go`) fails otherwise.
- **Frontend IPC wrappers** in `tauri-commands/file-actions.ts` (`quickLookOpen`, `quickLookSetPath`, `quickLookClose`).
  Delete the old `quickLook` wrapper.
- **Frontend state**: `lib/file-explorer/quick-look/quick-look-state.svelte.ts` with `{ isOpen: $state(false) }`.
  Listens once for `quick-look-closed` (flips to false) and `quick-look-key` (re-dispatches a synthetic `KeyboardEvent`
  on the focused pane element). The key re-dispatch lives here rather than in a component because it has to keep working
  regardless of which pane has focus.
- **`command-dispatch.ts:399`** `file.quickLook` case: toggle. If `isOpen`, call `quickLookClose()` + set
  `isOpen = false`; else, `quickLookOpen(path, volumeId)` and set `isOpen = true`. Pass the volume id from the cursor
  entry's pane state.
- **`command-registry.ts:385`**: shortcuts: `['⇧Space']`. Verified format: `key-capture.ts:25` maps `' '` → `'Space'`,
  so the registry string is `⇧Space` (no separator, `⇧` modifier symbol followed by `Space` word).
- **`menu/macos.rs:100`**: change accelerator from `Some("Space")` to `Some("Shift+Space")`. AppKit will fire the menu
  accelerator for this combo; webview won't see the keydown (matches every other modifier accelerator in the app); no
  double-dispatch with the registry shortcut.
- **`file-explorer/pane/FilePane.svelte:1604`**: guard the Space selection toggle with `!e.shiftKey`. Necessary even
  though the menu accelerator consumes Shift+Space (defense-in-depth: AppKit can in edge cases let modifier keydowns
  through to the webview, e.g. when the menu is being rebuilt during shortcut customization).

**Why no extra Tier-2 branch.** The shortcut-dispatch CLAUDE.md is explicit: Tier 2 (Space, arrows, Enter…) stays
component-local because they have context-dependent meaning. But `file.quickLook` has `showInPalette: true` on macOS,
which means it's auto-included in the centralized Tier-1 dispatch map. Putting `⇧Space` on it makes it a Tier-1 command.
In practice the AppKit menu accelerator does the actual dispatch for modifier-key shortcuts (see the "Plain Space"
section above), and the registry entry's role is documentation + conflict-detection + driving the menu accelerator sync
when the user customizes the shortcut. Either path leads to the same `handleCommandExecute(file.quickLook)` call.

**Test**: manually `pnpm dev`, navigate to a folder with images/PDFs, Shift+Space, confirm:

- panel opens over Cmdr, menu bar says "Cmdr"
- main window's titlebar goes _inactive_-colored (the panel is key now, by design) — this is correct AppKit behavior and
  matches what Finder does; the window still "feels" usable because key forwarding routes arrows back
- Shift+Space again closes it; Esc on the panel closes it and frontend `isOpen` flips back to false
- closing via ✕ also flips `isOpen` back

### M2 — Cursor-follow

Goal: with the panel open, arrow keys / typing / pane switches update the preview reactively.

- **In `file-explorer/pane/DualPaneExplorer.svelte`** (where `focusedPane` lives): a `$effect` that reads
  `quickLookState.isOpen`, the focused pane's `cursorIndex`, the entry under it, and the pane's `volumeId`; calls
  `quickLookSetPath(path, volumeId)` with debounce. Use the existing reactive pattern; no new abstraction.
- **Debounce strategy**: trailing-edge, ~100 ms idle window. Holding ArrowDown shouldn't pelt `reloadData` 60×/s; on the
  other hand 30–50 ms is jumpy when navigating quickly through directories that need real thumbnail generation. 100 ms
  is the Finder ballpark — fast enough to feel reactive on slow stop, slow enough not to thrash AppKit during burst nav.
  We tune this in M3 if it feels off. Use the same generation-counter pattern as `type-to-jump`
  (`type-to-jump-state.svelte.ts`) to guarantee no out-of-order updates if the user nav-bursts faster than IPC
  round-trip.
- **Cursor over `..`** or a directory: directories DO get a Quick Look preview in Finder (folder icon + name). We pass
  the path through unchanged; AppKit handles directories natively. Heads-up that directory previews can be heavier (icon
  composite); the 100 ms debounce already mitigates.
- **Cursor over a non-local-path file** (MTP, virtual git portal entries): backend short-circuits via the
  `Volume::supports_local_fs_access()` check from M1. Log debug with `target: "quick_look"`. No UI feedback in v1; panel
  just keeps showing the previous item until the cursor lands on a local-accessible entry. This is a deliberate v1
  trade-off — Finder doesn't preview MTP either.
- **Cancelability**: AppKit's QL preview loading is internal; opening a preview on a multi-GB video can take >1s. The
  100 ms debounce naturally cancels prior loads (the new `reloadData` supersedes the old one). When the user closes the
  panel, AppKit aborts in-flight loads. So our "longer than 1s = cancelable" principle is satisfied without us writing
  cancellation code.
- **Pane focus switch** with the panel open: same `$effect` fires because the focused pane / cursor changed.
- **Directory navigation** (Enter on a dir, Backspace to parent): same. The cursor lands on `..` or the first child; the
  panel updates.

**Test**: manually verify each transition. Hold ArrowDown — preview updates smoothly. Tab to other pane — preview
follows. Cmd+Right to navigate into a dir — preview updates to the first entry.

### M3 — Edge cases and polish

- **File deleted while open**: AppKit handles missing-file gracefully (shows a generic icon). Verify, then move on.
- **Volume unmounted while open**: same as above. Add a panel-close in the frontend if the focused pane goes into an
  error state, so we don't sit on a stale path.
- **Cmdr loses focus** (user Cmd-Tabs away): panel stays open, attached to Cmdr's window. Standard AppKit behavior. No
  special handling.
- **Panel close on tab switch**: open question. Finder keeps the panel open across tab switches. We do the same — cheap
  to implement since `$effect` already follows the focused tab.
- **Multi-selection**: Finder shows a "carousel" of selected items. v1: just show the cursor item, ignore selection set.
  This is a quality-of-life upgrade that can come later; flagging here so we don't forget the gap.
- **Accessibility**: `QLPreviewPanel` is fully accessible via AppKit. Nothing for us to do.

### M4 — Tests, docs, checks

- **Rust unit tests** (`quick_look/mod.rs`): test the `QuickLookController` state machine in isolation — open → set_path
  → close → reopen, double-open is idempotent, set_path before open is a no-op. The objc2 calls live behind a small
  trait so tests can mock the panel.
- **Vitest** (`quick-look-state.test.ts`): event listener flips `isOpen` to false; toggling via dispatch calls the right
  IPC.
- **Playwright E2E**: skip. `QLPreviewPanel` is a separate native NSPanel, not in the webview; Playwright can't see it.
  Document the gap in the test plan.
- **MCP-driven smoke** (replaces the parts Playwright can't reach): one spec via the `cmdr` + `tauri` MCP servers that
  (a) triggers `file.quickLook` via the command palette or direct dispatch, (b) asserts `quickLookState.isOpen === true`
  via `tauri__webview_execute_js`, (c) calls `quickLookClose` IPC and asserts the close event fires and `isOpen` flips
  back, (d) re-opens and asserts `quickLookSetPath` updates the controller's `current_url`. This covers the parts most
  likely to silently break in refactors (event listener wiring, state flips) without depending on the native panel's
  rendering.
- **Docs**:
  - Add `apps/desktop/src-tauri/src/quick_look/CLAUDE.md` describing the controller, why singleton, the focus decision,
    the MTP no-op, and how to extend to multi-selection.
  - Update `docs/architecture.md`: new row under "Backend" pointing to `quick_look/`.
  - Update `apps/desktop/src/lib/file-explorer/CLAUDE.md` Selection section to mention Shift+Space → Quick Look.
- **Checks**:
  - `./scripts/check.sh` (full suite — runs clippy, Rust tests, svelte-check, etc.)
  - `./scripts/check.sh --include-slow` before declaring the worktree mergeable.
  - `pnpm bindings:regen` was already run in M1; CI's `bindings-fresh` will tell us if anything drifted.

## Parallelism notes

Sequential is fine; nothing in this plan benefits meaningfully from parallel execution. M1 → M2 → M3 → M4 in order. The
Rust binding work in M1 needs to land before the frontend can call the new IPC; everything else flows naturally.

## Risk register

| Risk                                                                                                                    | Mitigation                                                                                                                                                                                                                                                       |
| ----------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `objc2-quick-look-ui` exposes `QLPreviewPanel{DataSource,Delegate}` as marker-only traits, not implementable protocols  | M1's verification step #1 catches this upfront. Fallback: declare the protocols manually via `extern_protocol!` (same pattern as `menu/macos.rs::cleanup_macos_menus`). One-day delta either way.                                                                |
| `objc2-quick-look-ui` minor version mismatch with `objc2-app-kit`/`objc2-foundation`                                    | M1's verification step #2 catches this. Pin to the matching 0.3.x line.                                                                                                                                                                                          |
| Key-forwarding via Tauri event has noticeable latency vs native AppKit                                                  | Tauri events round-trip in <1 ms locally; perceived latency dominated by main-thread hop + `reloadData`. Verify in M1 manual test; if it feels laggy, consider buffering the key payload and dispatching synchronously from a more direct route.                 |
| Debounce in M2 too aggressive (preview lags arrow nav) or too loose (60 reloads/sec on Down-hold)                       | 100 ms trailing-edge is the Finder ballpark; tune in M3 by feel. Generation counter (`type-to-jump` pattern) guarantees no out-of-order reloads.                                                                                                                 |
| MTP user expects Quick Look                                                                                             | v1 just no-ops with a debug log. If users complain, follow-up streams to a temp file.                                                                                                                                                                            |
| `QLPreviewPanel` interacts with an already-open `NSOpenPanel` (Cmdr's "Open with… Other" picker, save dialogs)          | Both are modal-ish on the main thread. AppKit serializes; opening QL while NSOpenPanel is up either queues or fails silently. Manual test in M3 covers this; document the behavior in `quick_look/CLAUDE.md`. Probably "QL skipped while NSOpenPanel is active." |
| Menu still shows greyed-out "Quick look" when no entry is under the cursor (focus on directory crumb, empty list, etc.) | `set_menu_context` already disables file-scoped items when main loses focus or context is invalid. Verify the file-scoped flag is set correctly for `QUICK_LOOK_ID` in `menu/mod.rs:233` (it already is, per existing wiring).                                   |

## Test plan (manual)

1. **Open / close**
   - Folder with mixed types (image, pdf, text, video, dir, `..`)
   - Shift+Space on each → preview opens with right content, menu bar says "Cmdr", main window titlebar goes inactive
     (panel is key — that's correct), arrow keys still navigate the file list via the key-forwarding event
   - Shift+Space again → closes; Esc → closes; ✕ → closes; in all cases frontend `isOpen` flips back (verify by
     reopening — first press should open, not close)
2. **Cursor-follow**
   - Open panel, hold ArrowDown — preview updates smoothly without stutters
   - ArrowUp / Cmd+Down / PageDown / type-to-jump — preview updates
   - Tab to other pane — preview switches to other pane's cursor item
   - Cmd+Right into a directory — preview updates to new cursor position
3. **Edge cases**
   - Open on a file, delete the file from terminal → panel handles missing file gracefully
   - Open on a file in an SMB mount — should work (it's a real fs path)
   - Open on an MTP file — backend no-ops, no crash, debug log present
   - Open, then unmount volume → panel closes or shows error, doesn't leave a dangling preview
4. **Menu**
   - File → Quick look → fires the same action, shows ⇧Space as accelerator
5. **Shortcut customization**
   - Open Settings → Shortcuts, change Quick Look from ⇧Space to ⌥Q (or whatever) → menu accelerator updates, keystroke
     works, Shift+Space goes back to selection toggle

## When this plan is done

- Cmdr's Quick Look feels indistinguishable from Finder's, on local + SMB-mounted volumes.
- MTP and other non-fs volumes log a debug line and otherwise stay silent — non-goal for v1, documented for v2.
- All checks green, including `--include-slow`, including manual test plan.
