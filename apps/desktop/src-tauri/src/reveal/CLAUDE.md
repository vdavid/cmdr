# Reveal in Cmdr

Another app's "Show in Finder" lands in a Cmdr pane instead. One OS switch turns it on: the undocumented
`NSFileViewer` key in the global preferences domain, holding a bundle id.

## Module map

- **`registration.rs`**: the `NSFileViewer` state machine (`RevealHandlerState`, `RevealHandlerStatus`,
  `RevealRegistration`), the `ViewerPreference` seam over CFPreferences, and `own_bundle_id`, which decides whether this
  build may write at all.
- **`delivery.rs`**: what arrives once it's on. `on_urls_opened` is the one door; `plan_reveal` turns the delivered
  paths into one pane move; `PendingReveals` parks anything the frontend isn't up for yet; `deliver` drives the pane
  and then announces `RevealDelivered`.
- **`mod.rs`**: the `RevealDelivered` event type and nothing else.
- **`commands.rs`**: three IPC commands (read state, set state, drain the parked reveals).

## Must-knows

- **`mod.rs` is NOT `#[cfg(target_os = "macos")]`; all three submodules are.** `RevealDelivered` has to resolve on
  every platform for `ipc.rs`'s `collect_events!`, which can't cfg-gate inline (same reason `SmbFellBackToOsMount` sits
  in `network/mod.rs`). ❌ Don't move anything else up into `mod.rs`: on Linux it would have no callers, and `-D unused`
  turns that into a broken build rather than a warning. ❌ `mod.rs`'s docs name the three submodules in plain backticks,
  never as `[`delivery`]` intra-doc links: those resolve on macOS and break the Linux `rustdoc` job, which is a red CI
  your local `pnpm check rustdoc` can't see.
- **`RevealDelivered` is emitted only after the pane actually moved**, so it means "the feature just did its thing".
  The frontend turns the FIRST one into a once-ever notice (`apps/desktop/src/lib/reveal/CLAUDE.md`), so announcing a
  reveal that went nowhere would spend it.
- **A cold-launch reveal arrives ~500 ms BEFORE Tauri's `setup` runs**, as the same `RunEvent::Opened` (measured on
  macOS 26.6, 2026-09-09). So at that moment there is no `app.manage`d state, no logger, and no main window. That's why
  `PENDING` is a process-global `LazyLock` and ❌ never Tauri-managed state, and why a log line added to this path won't
  appear. `DETAILS.md` § "How a cold-launch reveal arrives" also lists the two fixes already tried and reverted (an
  early `AEInstallEventHandler`, and an `application:openURLs:` swizzle) — ❌ don't re-derive them.
- **Turning off clears the key only when it still names us.** Another app can take it between the Settings row being
  drawn and the click; clearing then would unregister somebody else's file manager. And clearing means REMOVING the
  key, ❌ never writing `com.apple.finder`: absence is the true default.
- **A dangling key breaks reveal machine-wide**: macOS does NOT fall back to Finder when `NSFileViewer` names an app
  that's gone (macOS 26.6, 2026-09-09), and nothing of ours runs at uninstall. So a non-production build may never write
  it at all (`own_bundle_id` → `None` → `Unavailable`, on any of: debug build, a `prod_instance` harness env, a
  non-production bundle id), `set_enabled(true)` refuses from a copy outside an Applications folder
  (`crate::install_location`), and the cask clears it on uninstall. ❗ The gate blocks TAKING the key, ❌ never giving it
  up, or a copy that registered and then moved is stranded holding it. `DETAILS.md` § "The uninstall problem".
- **This is not a stored setting.** No `settings.json` key, no registry entry: the state reads through to the OS every
  time, because the user can change the handler outside Cmdr and a cached flag would lie.
- **The pane move is `crate::mcp::go_to_in_focused_pane`**, shared with `go_to_latest_download`. ❌ Don't grow a second
  navigate-then-cursor path; `DETAILS.md` says why it lives over there.
- **The drain call has to run AFTER the frontend's `mcp-*` listeners are up** (`routes/(main)/window-services.ts`,
  phase 2). Draining earlier emits `mcp-nav-to-path` into a window with nothing listening, and the reveal is lost.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
