# File system › git

The app's side of the git browser: the two seams that reach `crates/cmdr-git` and the decisions only the app can make.
Everything that talks to a repository lives in that crate, whose `CLAUDE.md` is the one to read before touching a
listing, the status walk, or the watcher.

Frontend counterpart: `apps/desktop/src/lib/file-explorer/git/CLAUDE.md`.

## Module map

- `overlay.rs`: the `ListingOverlay` contributor that puts the six category rows into a repo's `.git/` listing. It asks
  `GitPortal::category_rows` and does nothing else.
- `wiring.rs`: the parked portal, the toggle both seams consult, `volume_holds_real_repos`, the `git-state-changed`
  payload and the sink that emits it, and the listing re-reads a repo change or a toggle drives.
- The route itself is `file_system/volume/manager/git_routing.rs`, with the registry that owns it. The IPC commands are
  `commands/file_system/git.rs`.

## Must-knows

- **Two seams, no hooks: `LocalPosixVolume` names git nowhere, and ❌ must never again.** Below `.git/` is a ROUTE
  (`resolve` → `cmdr_git::GitPortalVolume`); `.git/` itself is a listing OVERLAY reaching a PANE and nothing else.
  `DETAILS.md` § "Two seams, no hooks".
- **❌ Never widen either seam.** The volume serves the six categories and nothing under them; the overlay claims only
  a DIRECTORY called `.git` on a volume `gix` can open. So a linked worktree's gitlink FILE has no landing listing, and
  a `.git` on a direct-SMB share isn't the portal's.
- **Real files under `.git` are ordinary local files**, portal on or off, which is what lets a repo-folder delete walk
  `.git/` to the end. ❌ Never add a guard back: the last one refused `.git/config` too and half-deleted repos.
- **Flipping the toggle must refresh open listings.** `set_show_virtual_git_portal` flips the atomic AND calls
  `wiring::refresh_all_virtual_listings_after_toggle`, across every volume; the atomic alone leaves stale children on
  screen. That set comes from the LISTING CACHE, ❌ never the watcher registry. `DETAILS.md` § Decisions.
- **The portal is parked, ❌ never rebuilt.** `wiring::portal()` is the app's one `GitPortal`; a second would open every
  repository twice and watch it twice.
- **A virtual listing is unwatchable by TYPE** (`can_watch_listings()` false), and `.git/` itself IS watched. So a
  `FullRefresh` there must re-run the overlays, and the fresh-listing oracle must keep declining a decorated listing.
  `DETAILS.md` § "A virtual listing is unwatchable by type".
- **`walker_exposure_tests.rs` pins the portal's blast radius**: which non-pane walkers can reach a virtual entry, and
  the answer is none. `DETAILS.md` § "What each walker sees".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
