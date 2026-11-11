# Child window lifecycle

How every child window (Settings, file viewer, Debug, Keyboard shortcuts, operation queue; every window but `main`)
appears and goes away. On macOS it fades in while sliding down 24 pt over 0.22 s, and reverses that on close, unless
Reduce motion is on.

## Module map

- `mod.rs`: `show`, `close`, `cancel_close`, `accepts_rect_write`, the timings. `ledger.rs`: per-window state (slid in,
  closing with a ticket), pure and unit-tested. `appkit.rs`: the `NSWindow` animator calls and the Reduce motion read.
- Frontend halves: `#lib/child-window-show` (page shows itself, opener's 2 s fallback) and `#lib/child-window-close`.

## Must-knows

- **Every close lands in `close`.** Escape and in-page buttons via `close_child_window`; the red button, ⌘W, the Window
  menu, and any `close()` (Rust, JS, MCP) via `app_lifecycle.rs`'s `CloseRequested`, which `prevent_close()`s. The
  last step is `destroy()`, which emits no `CloseRequested`, so it can't loop. ❌ Never `close()` from inside `close`.
- **Order: slide out, hide, wait `HIDE_TO_DESTROY` (100 ms), destroy.** The slide runs BEFORE the hide, ❌ never
  inside the 100 ms: that wait is the WebKit teardown-crash floor (`docs/notes/child-window-close-webkit-crash.md`).
  ❌ Never lower it.
- **Only a reopen may stop a scheduled destroy** (`raise_child_window` → `cancel_close`, which shows the window). Any
  other early return leaks an invisible, still-alive webview.
- **A window slides out only if it slid in** (the ledger says so): an E2E window (created visible) or one shown under
  Reduce motion closes without a slide.
- **Mid-slide rect writes are dropped** (`accepts_rect_write`): `trackOwnRect` saves on every move, and saving the
  slide's offset made each reopen land 24 pt higher.
- **Openers create the window `visible: false` outside E2E**, so AppKit's own `animationBehavior` can be switched off
  before the first order-front. A new child window needs that plus a `showSelfWindow()` call in its page.
- **AppKit only on the main thread**: every `appkit.rs` fn checks a `MainThreadMarker`; callers hop via
  `run_on_main_thread`.

Flows, timings, races, and decisions: `DETAILS.md`.
