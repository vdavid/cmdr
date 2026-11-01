# Reveal in Cmdr

Another app's "Show in Finder" lands in a Cmdr pane instead. One OS switch turns it on: the undocumented
`NSFileViewer` key in the global preferences domain, holding a bundle id. The `cmdr` terminal command (`../cli/`)
comes through the same door as a `cmdr-cli://` URL, sharing the buffer and the pane move.

## Module map

- **`registration.rs`**: the `NSFileViewer` state machine, the `ViewerPreference` seam over CFPreferences, and
  `own_bundle_id`, which decides whether this build may write at all.
- **`delivery.rs`**: what arrives. `on_urls_opened` is the one door; `requests_from_urls` sorts Finder `file://` from
  terminal `cmdr-cli://` into `RevealRequest`s; `plan_reveal` makes one pane move; `PendingReveals` parks what the
  frontend isn't up for yet; `deliver` moves the pane.
- **`mod.rs`**: the `RevealDelivered` event type and nothing else.
- **`commands.rs`**: three IPC commands (read state, set state, drain the parked reveals).

## Must-knows

- **`mod.rs` is NOT `#[cfg(target_os = "macos")]`; all three submodules are.** `RevealDelivered` has to resolve on
  every platform for `ipc.rs`'s `collect_events!`, which can't cfg-gate inline. ❌ Don't move anything else up into
  `mod.rs`: on Linux it would have no callers, and `-D unused` breaks the build. ❌ `mod.rs`'s docs name the submodules
  in plain backticks, never as intra-doc links: those break the Linux `rustdoc` job, which local checks can't see.
- **`RevealDelivered` is emitted only after the pane moved, and only for `RevealOrigin::Finder`.** The frontend turns
  the FIRST one into a once-ever "Show in Finder lands here" notice (`apps/desktop/src/lib/reveal/CLAUDE.md`), so a
  reveal that went nowhere, or a terminal open, must not spend it.
- **A cold-launch reveal arrives ~500 ms BEFORE Tauri's `setup` runs**, as the same `RunEvent::Opened` (measured on
  macOS 26.6, 2026-09-09). So at that moment there is no `app.manage`d state, no logger, and no main window. That's why
  `PENDING` is a process-global `LazyLock` and ❌ never Tauri-managed state, and why a log line added to this path won't
  appear. `DETAILS.md` § "How a cold-launch reveal arrives" also lists the two fixes already tried and reverted (an
  early `AEInstallEventHandler`, and an `application:openURLs:` swizzle) — ❌ don't re-derive them.
- **Turning off clears the key only when it still names us.** Another app can take it between the Settings row being
  drawn and the click; clearing then would unregister somebody else's file manager. And clearing means REMOVING the
  key, ❌ never writing `com.apple.finder`: absence is the true default.
- **A dangling key breaks reveal machine-wide**: macOS does NOT fall back to Finder when `NSFileViewer` names an app
  that's gone (macOS 26.6, 2026-09-09), and nothing of ours runs at uninstall. So a non-production build never writes
  it (`own_bundle_id` → `None` → `NotProductionBuild`), `set_enabled(true)` refuses from a copy outside an Applications
  folder (`crate::install_location`), and the cask clears it on uninstall. ❗ The gate blocks TAKING the key, ❌ never
  giving it up, or a moved copy is stranded holding it. `DETAILS.md` § "The uninstall problem".
- **This is not a stored setting.** The state reads through to the OS every time: the user can change the handler
  outside Cmdr, and a cached flag would lie.
- **The pane move is `crate::tools::go_to_in_pane`**, shared with `go_to_latest_download`; a terminal request carries its
  own `GoToTarget`. ❌ Don't grow a second navigate-then-cursor path; `DETAILS.md` says why it lives over there.
- **The drain call has to run AFTER the frontend's `mcp-*` listeners are up** (`routes/(main)/window-services.ts`,
  phase 2). Draining earlier emits `mcp-nav-to-path` into a window with nothing listening, and the reveal is lost.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
