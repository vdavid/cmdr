# Reveal in Cmdr

Another app's "Show in Finder" lands in a Cmdr pane instead. One OS switch turns it on: the undocumented
`NSFileViewer` key in the global preferences domain, holding a bundle id.

## Module map

- **`registration.rs`**: the `NSFileViewer` state machine (`RevealHandlerState`, `RevealRegistration`), the
  `ViewerPreference` seam over CFPreferences, and `own_bundle_id`, which decides whether this build may write at all.
- **`mod.rs`**: what arrives once it's on. `on_urls_opened` is the one door; `plan_reveal` turns the delivered paths
  into one pane move; `PendingReveals` parks anything the frontend isn't up for yet; `deliver` drives the pane and then
  announces `RevealDelivered`.
- **`commands.rs`**: three IPC commands (read state, set state, drain the parked reveals).

## Must-knows

- **This module is NOT `#[cfg(target_os = "macos")]`; its two macOS halves are.** `RevealDelivered` has to resolve on
  every platform for `ipc.rs`'s `collect_events!`, which can't cfg-gate inline (same reason `SmbFellBackToOsMount` sits
  in `network/mod.rs`). So `commands` and `registration` carry the gate, and the planning tests run on the Linux lane
  for free.
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
- **A non-production build may never write the key** (`own_bundle_id` returns `None` → state `Unavailable`). A dev build
  that grabs it and is then deleted leaves a dangling `NSFileViewer` that breaks reveal machine-wide, and the E2E suite
  must be structurally incapable of rewiring the user's Mac. Three independent gates: debug build, any
  `prod_instance` harness env, and a bundle id that isn't the production one.
- **This is not a stored setting.** No `settings.json` key, no registry entry: the state reads through to the OS every
  time, because the user can change the handler outside Cmdr and a cached flag would lie.
- **The pane move is `crate::mcp::go_to_in_focused_pane`**, shared with `go_to_latest_download`. ❌ Don't grow a second
  navigate-then-cursor path; `DETAILS.md` says why it lives over there.
- **The drain call has to run AFTER the frontend's `mcp-*` listeners are up** (`routes/(main)/window-services.ts`,
  phase 2). Draining earlier emits `mcp-nav-to-path` into a window with nothing listening, and the reveal is lost.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
