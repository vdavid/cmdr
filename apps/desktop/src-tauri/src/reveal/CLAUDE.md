# Reveal in Cmdr

Another app's "Show in Finder" lands in a Cmdr pane instead. One OS switch turns it on: the undocumented
`NSFileViewer` key in the global preferences domain, holding a bundle id.

## Module map

- **`registration.rs`**: the `NSFileViewer` state machine (`RevealHandlerState`, `RevealRegistration`), the
  `ViewerPreference` seam over CFPreferences, and `own_bundle_id`, which decides whether this build may write at all.
- **`mod.rs`**: what arrives once it's on. `plan_reveal` turns the delivered paths into one pane move; `PendingReveals`
  parks anything the frontend isn't up for yet; `deliver` drives the pane.
- **`commands.rs`**: three IPC commands (read state, set state, drain the parked reveals).

## Must-knows

- **Cold launch does not work, and that isn't a bug in this code.** A reveal to an already-running Cmdr arrives as
  `RunEvent::Opened`; one that cold-launches Cmdr never arrives at all (measured on macOS 26.6, 2026-09-09). Declaring
  document types and catching `application:openURLs:` at the delegate were both tried and neither helped. ❌ Don't
  re-derive it: `DETAILS.md` § "The cold-launch gap" has the evidence and what's left to try.
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
