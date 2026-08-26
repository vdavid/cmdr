# Can Cmdr move itself to Applications? (measured 2026-08-25)

`MoveToApplicationsDialog` currently only instructs: quit, drag Cmdr to Applications, reopen. This note is the evidence
behind whether Cmdr could do the move itself, taken by experiment on **macOS 26.5.2 (25F84), Apple silicon**, with a
throwaway Developer-ID-signed `.app` and a borrowed notarized third-party app. Nothing here was reasoned from
recollection.

**The finding: the mechanism is safe, and the FDA grant is not at risk.** Nothing in TCC records a path. What stops this
from being a small change is the machinery (a detached helper, a relaunch, an already-installed-copy branch) and the
fact that the end-to-end path can only be exercised on a notarized build.

The current decision, and the failure-mode plan a revisit was asked to bring, live in
`apps/desktop/src/lib/updates/DETAILS.md` § When the bundle can't be written.

## 1. The original path is recoverable under translocation

`SecTranslocateCreateOriginalPathForURL` works and is not deprecated. Cmdr already links its sibling
`SecTranslocateIsTranslocatedURL` (`src-tauri/src/updater/bundle_location.rs`), so this adds no new category of risk.

Measured: all seven `SecTranslocate*` symbols resolve through `dlsym` on the live Security.framework, and all seven are
exported by the SDK's `Security.tbd`. There is **no public header in the SDK** for any of them, so the whole family is
SPI: declare the prototypes yourself, link `-framework Security`. No deprecation attribute exists to read, because no
header exists.

On a real translocated bundle (a quarantined copy of a notarized app, opened from `~/Downloads`, translocated by
Gatekeeper):

- `SecTranslocateIsTranslocatedURL` → `true`
- `SecTranslocateCreateOriginalPathForURL` → `/Users/…/Downloads/Tl2.app`, exactly right

**Fallback if the SPI ever goes away**: `mount` lists the translocation as
`/Users/…/Downloads/Tl2.app on /private/var/folders/…/AppTranslocation/<UUID> (nullfs, local, nodev, nosuid, read-only, nobrowse)`.
The mount source IS the original path, so `getmntinfo` answers the same question through a public API.

`SecTranslocateCreateSecureDirectoryForURL` fails with `EPERM` from an ordinary process: only the trusted translocation
service may create the mount. So a translocated state can't be manufactured for a test; it has to be induced by
launching a quarantined, notarized app.

## 2. Full Disk Access survives a move. Nothing in TCC records a path

This was the question worth answering, since the whole custom updater exists to keep FDA. Four measurements, converging:

- **No TCC table has a path column.** The system store's `access` table is keyed
  `PRIMARY KEY (service, client, client_type, indirect_object_identifier)`; the other six tables (`policies`,
  `active_policy`, `access_overrides`, `expired`, `admin`, `integrity_flag`) carry no path either. Same shape in the
  per-user store.
- **Cmdr's own FDA row contains no path.** Decoding the `csreq` blob out of `kTCCServiceSystemPolicyAllFiles` /
  `com.veszelovszki.cmdr` with `csreq -t` gives:

  ```
  identifier "com.veszelovszki.cmdr" and anchor apple generic
    and certificate 1[field.1.2.840.113635.100.6.2.6] and certificate leaf[field.1.2.840.113635.100.6.1.13]
    and certificate leaf[subject.OU] = "83H6YAQMNP"
  ```

  Bundle id plus Developer ID team. `codesign --verify -R=` confirms `/Applications/Cmdr.app` satisfies it in place, and
  a test bundle satisfies its own equivalent requirement identically from three different directories. The requirement
  language cannot express a path at all: `csreq` rejects a `path =` predicate with `unexpected token: path`.

- **A moved bundle reuses its row rather than growing a second one.** A signed test app was launched through
  LaunchServices, moved to an unrelated directory, and launched again. One row before, the same one row after, same
  `last_modified`. TCC identified it by bundle id, not by where it sat.
- **Real-world corroboration on this machine.** Cmdr's FDA grant was last written 2026-06-14. The directory now at
  `/Applications/Cmdr.app` has a birth time of 2026-08-05, so the bundle was replaced wholesale (new inode) seven weeks
  after the grant, and the grant is still `auth_value = 2` and working.

**Contrast that with a client TCC identifies by path.** `client_type = 1` rows carry an executable path as the client,
and those DO fragment per location: this machine holds two separate FDA rows for Total Commander's `wineskinlauncher`,
one under `/Applications` and one under `~/Applications`. That only happens for clients with no bundle identity. A
signed `.app` is `client_type = 0` and is immune.

**Not measured**: an `auth_value = 2` row specifically, surviving a move. Granting FDA needs a click in System Settings,
and the one autonomous route (signing a probe with Cmdr's own identifier so it inherits Cmdr's grant) is TCC
impersonation and was refused. The gap is small (same table, same key, same csreq evaluation, and the row-reuse
measurement above was on the same code path), but it is a gap. **To close it in ~30 seconds**: build a probe app, launch
it once so TCC lists it, toggle it on under Privacy & Security > Full Disk Access, then move the bundle and launch
again.

### The inode rationale in `src-tauri/src/updater/DETAILS.md` looks over-stated

That doc says replacing the `.app` directory "changes the inode, which makes macOS TCC lose FDA grants". The FDA row has
no inode in it, and the 2026-08-05 bundle replacement above did not cost the grant. What IS measured and does hold is
the guardrail in `updater/CLAUDE.md`: per-file writes need a NEW inode, or the kernel's code-signing cache validates the
new binary against the old code directory and `SIGKILL`s the app. `/Applications/Cmdr.app` also carries a
`com.apple.macl` xattr, which is per-file and is lost when the directory is recreated, so there is still a reason not to
replace the bundle in the updater. Only the FDA-specific half of the claim was wrong, and `updater/CLAUDE.md` and
`DETAILS.md` now say so.

## 3. A reliable self-move is implementable. The verified recipe

Every step below was run. The order is chosen so that no step destroys anything until the one after it has succeeded.

1. **Recover the original path** with `SecTranslocateCreateOriginalPathForURL` (§1). Needed only for step 6; the copy
   itself doesn't need it, see the next step.
2. **Copy the bundle out of the translocated mount.** The mount is a read-only nullfs view of the real bundle, so a
   `ditto` out of it is byte-identical to the original, `codesign --verify --deep --strict` passes, and `spctl --assess`
   still answers `accepted / Notarized Developer ID`. Measured. So the running app can copy ITSELF; it never has to
   reach for the original.
3. **Strip quarantine at the destination.** ⚠️ Skipping this silently wastes the whole exercise. The copy inherits
   `com.apple.quarantine` (as `0283;…`, already Gatekeeper-approved), and a quarantined bundle is translocated again
   **even from `/Applications`**: `SecTranslocateURLShouldRunTranslocated` answered `true` on the fresh copy sitting in
   `/Applications`. After `xattr -dr com.apple.quarantine` it answers `false`, the signature still verifies, `spctl`
   still accepts, and the bundle is writable in place, which is what puts the normal TCC-preserving updater back in
   business. This is the same step LetsMove does in its relaunch script, and it is why "just `mv` it" is a known
   non-fix.
4. **Verify before trusting.** `codesign --verify --deep --strict` on the destination, then re-read the version out of
   `Info.plist`. Only past this point has anything been gained.
5. **Relaunch.** A running app cannot overwrite itself in place, but it never has to here: the destination is a new
   path. A small detached helper waits for the old pid to exit (`kill -0` poll) and `open`s the destination. A helper
   that dies at ANY point leaves both copies on disk and nothing deleted, so the worst case is a confused user with two
   Cmdrs, never a user with none.
6. **Only then delete the original**, at the path from step 1. Trash rather than `rm`: recoverable, and it matches what
   dragging in Finder leaves behind.

`/Applications` is `drwxrwxr-x root:admin` and an admin user can write it with no prompt (measured), so the common case
needs no privilege escalation.

**The one branch that needs deciding: `/Applications/Cmdr.app` already exists.** Overwriting it would recreate a bundle
that has its own `com.apple.macl` and may be a NEWER build than the translocated one. The non-destructive answer, and
what LetsMove does, is to not move at all: say Cmdr is already installed and offer to switch to that copy.

**Gatekeeper on relaunch**: no re-check bites. Measured after step 3, the destination is un-quarantined, signature-valid
and `spctl`-accepted, so it opens with no prompt and no translocation.

## 4. What comparable apps do

LetsMove (`PFMoveApplication`) is the established approach, and most "move to Applications?" prompts are it.

- It **copies** (`CopyBundle` → `copyItemAtPath:`), then trashes the original, which is the non-destructive order.
- If `/Applications` already holds the app it trashes the duplicate, and if that copy is running it focuses it and quits
  itself.
- It relaunches through an `NSTask` shell script that polls for the old pid to exit, **runs
  `xattr -d com.apple.quarantine`**, then `open`s.
- It does **not** use `SecTranslocateCreateOriginalPathForURL`. It handles translocation only indirectly, via an
  AppleScript fallback for trashing an app running out of a translocation image.
- Its NSFileManager move alone does not disable translocation; this is a
  [known issue](https://github.com/potionfactory/LetsMove/issues/56), and matches step 3 above. Only Finder's own move
  clears the state implicitly.

So: the established approach handles translocation worse than the recipe in §3 would, and the piece it gets right is the
copy-then-trash ordering.

Sources: [potionfactory/LetsMove](https://github.com/potionfactory/LetsMove),
[LetsMove issue 56](https://github.com/potionfactory/LetsMove/issues/56),
[App Translocation, Lapcat Software](https://lapcatsoftware.com/articles/app-translocation.html),
[Untranslocating apps, Synack](https://synack.com/blog/untranslocating-apps),
[AppMover, Christian Tietze](https://christiantietze.de/posts/2020/01/appmover-swift/).

## 5. What it would cost to build

Not a small change, which is the real argument against doing it casually rather than any risk in the mechanism:

- Rust: recover the path, copy, verify, dequarantine, the already-installed branch, a detached relaunch helper, trash
  the original. New machinery in the subsystem whose entire job is not losing the user's TCC grants.
- Frontend: a button, a new dialog state, and its failure state.
- Copy in `updates.*` across ten locales, through the translator process (`docs/guides/i18n-translation.md`).
- A screenshot capture run for the new dialog state.
- **The end-to-end path can only be exercised on a notarized build.** Every piece above was measured individually, but
  the assembled flow (translocated Cmdr moves itself and comes back up in `/Applications` with FDA intact) can't be run
  against a dev build, because Gatekeeper won't translocate an un-notarized app in the first place. Whatever ships has
  to be verified against a real release artifact.
