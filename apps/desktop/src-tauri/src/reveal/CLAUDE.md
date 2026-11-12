# Reveal in Cmdr

Everything that lands in a pane from outside Cmdr: another app's "Show in Finder" (the undocumented `NSFileViewer` key
in the global preferences domain), the `cmdr` terminal command (`../cli/`, a `cmdr-cli://` URL), and "Open in Cmdr" in
Finder's right-click menu (a macOS service). All share the buffer and the pane move.

## Module map

- **`registration.rs`**: the `NSFileViewer` state machine, its `ViewerPreference` seam, and `own_bundle_id`, which
  decides whether this build may write at all.
- **`delivery.rs`**: `take_requests` is the one door; `requests_from_urls` sorts the URLs; `plan_moves` makes the pane
  moves; `PendingReveals` parks what the frontend isn't up for yet; `deliver` moves the pane.
- **`service.rs`**: the provider behind "Open in Cmdr". **`finder_handoff.rs`**: Cmdr's own "Show in Finder".
- **`mod.rs`**: the two event types and nothing else. **`commands.rs`**: the IPC commands.

## Must-knows

- **`mod.rs` is NOT `#[cfg(target_os = "macos")]`; all its submodules are.** The event types must resolve on every
  platform for `ipc.rs`'s `collect_events!`. ❌ Nothing else goes up there (`-D unused` on Linux). ❌ Its docs name
  submodules in plain backticks, never intra-doc links: those break the Linux `rustdoc` job.
- **`RevealDelivered` fires only after a pane moved, only for `RevealOrigin::Finder`, only while the key is ours.** The
  frontend spends a once-ever "Show in Finder lands here" notice on it.
- **The window is raised BEFORE the move, so every move that doesn't land must emit `RevealNotShown`** (one toast per
  batch, typed reason), ❌ never a warn alone (#1020). `DETAILS.md` § Delivery.
- **❌ At a pane's tab limit, only the FIRST move may use the current tab**, or every extra folder overwrites it. The
  rest drop with `RevealTabsDropped`. The frontend owns the limit. `DETAILS.md` § "The dispatch rule".
- **A cold-launch reveal arrives ~500 ms BEFORE Tauri's `setup` runs** (macOS 26.6, 2026-09-09): no `app.manage`d state,
  no logger, no main window. So `PENDING` is a process-global `LazyLock`, ❌ never managed state, and a log line here
  won't appear. Two reverted fixes are listed in `DETAILS.md` § "How a cold-launch reveal arrives"; ❌ don't re-derive
  them.
- **❌ Don't drop or re-rank `Info.plist`'s `public.item` claim (`LSHandlerRank = None`).** Without it every sandboxed
  app's reveal (Archive Utility after a browser download) only activates Cmdr (#1158). `DETAILS.md` § "Sandboxed
  senders".
- **Turning off clears the key only when it still names us**, by REMOVING it, ❌ never writing `com.apple.finder`.
- **Cmdr's own "Show in Finder" must beat the key.** ❌ `open -a Finder -R` is redirected, and `NSFileViewer =
  com.apple.finder` opens the FILE. `DETAILS.md` § "Cmdr's own Show in Finder".
- **A dangling key breaks reveal machine-wide** (macOS 26.6, 2026-09-09), and nothing of ours runs at uninstall. So a
  non-production build never writes it, a copy outside an Applications folder can't take it, and the cask clears it.
  ❗ The gate blocks TAKING, ❌ never giving up. `DETAILS.md` § "The uninstall problem".
- **Not a stored setting.** The state reads through to the OS every time.
- **The pane move is `crate::tools::go_to_in_pane`**, after `open_tab_in_pane` when a move wants a tab. ❌ Don't grow a
  second navigate-then-cursor path.
- **The drain runs AFTER the frontend's `mcp-*` listeners are up** (`routes/(main)/window-services.ts`, phase 2), or the
  reveal is lost.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
