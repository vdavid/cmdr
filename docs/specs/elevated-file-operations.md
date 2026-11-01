# Elevated file operations

**The quest**: A user (ERR-4TEMD, v0.44.0, macOS 26.6.2) tried to move two `root:admin` files out of
`/Applications/PixInsight/src/scripts/Toolbox/` into a folder under `~/Downloads`. Both attempts stopped within 2 ms
with `Permission denied (os error 13)`: both folders sit on one volume, so the move is a rename, and a rename needs
write access to the folder the files leave. Copying worked because the files are world-readable, so the user copied in
Cmdr, deleted the originals with `sudo` in Terminal, and asked for Cmdr to ask for the admin password instead.

**What we're building**: when an operation hits a folder the macOS user can't change, Cmdr asks, gets the admin password
at most once a day, and finishes the operation with its normal progress, cancel, conflict handling, and rollback. A tiny
root helper does only the refused steps. macOS 13+ only.

**Status**: draft. The decisions below were taken with David on 2026-09-15. The open questions at the end need the M1
spike before M2 starts.

## What already exists

- **Where the move stops**: `crates/cmdr-operations/src/transfer/move_op/mod.rs` picks the same-volume engine by
  `st_dev`, and `crates/cmdr-operations/src/transfer/move_op/same_fs.rs` renames with `?`, so a refusal aborts the
  operation with no fallback. The pre-flight in `crates/cmdr-operations/src/validation.rs` checks only that the
  DESTINATION is writable.
- **The errno already travels, typed**: `WriteOperationError::PermissionDenied` carries `errno`, a `PermissionRefusal`
  derived from it (`FolderPermissions` for `EACCES`, where root helps; `SystemProtected` for `EPERM`, where it doesn't),
  and a `refused_folder` proved with `access(W_OK)` at the refusal. Built only through
  `WriteOperationError::permission_denied`. See `crates/cmdr-operations/DETAILS.md` § "Naming the folder that refused a
  write"; decision 12 below is what branches on it.
- **Elevation today**: only the updater, which runs `rsync` through `do shell script ... with administrator privileges`
  (`apps/desktop/src-tauri/src/updater/installer.rs`).
- **An out-of-process native alert**: `apps/desktop/src-tauri/src/instance_lock.rs` (`show_already_running_alert`) uses
  `CFUserNotificationDisplayAlert`, drawn by macOS's own UserNotificationCenter agent and localized through
  `crate::intl::menu_t`. Its doc comment explains why `NSAlert::runModal` and `tauri-plugin-dialog` don't fit.
- **Parking on a person**: the write-operations engine already parks a running operation on a human answer for name
  clashes (`crates/cmdr-operations/src/conflict_slot.rs`, `crates/cmdr-operations/src/human_wait.rs`). The elevation
  question parks the same way.
- **MCP writes without a dialog**: `copy` / `move` / `delete` with `autoConfirm: true` plus the token from
  `<data_dir>/mcp.token`, and `dialog` with `action: "confirm"`, apply an operation with no human click
  (`apps/desktop/src-tauri/src/mcp/auth.rs`, `apps/desktop/src-tauri/src/tools/actions/file_ops.rs`). Seven Playwright
  specs use `autoConfirm` (34 call sites) and two more use `dialog confirm`. The in-app agent can only propose.
- **Signing**: `apps/desktop/src-tauri/Entitlements.plist` is an empty dict, so the hardened runtime has no holes and
  nothing can inject code into Cmdr. That's what makes decision 8's "trust whatever runs inside Cmdr" sound.
  `docs/security.md` § Entitlements holds the invariant.
- **Platform floor**: `apps/desktop/src-tauri/tauri.conf.json` says 10.15; the supported floor is macOS 12, with 10.15
  and 11 best effort (`apps/website/src/pages/llms.txt.ts`).

## Rejected alternatives

- **One `do shell script ... with administrator privileges` per operation** (the updater's pattern): one password prompt
  per batch works, since macOS caches the authentication for five minutes per script, but the elevated step is a black
  box until it returns. No live progress, no clean cancel, and conflicts must be decided up front. Progress is required.
- **Per-file password prompts**: every file makes a different script, so a 20-file move means 20 prompts.
- **A helper that stays unlocked with no password ("Always allow")**: it turns "needs the admin password" into "anything
  that can drive Cmdr gets root writes".
- **Routing every operation through the helper**: every write would run as root and pay an XPC hop per syscall.
- **`SMJobBless` for macOS 12 and below**: deprecated, needs a hand-rolled caller check, more attack surface for few
  users.
- **Removing `autoConfirm` from MCP**: it wouldn't close the gap (`dialog confirm` does the same thing), it breaks nine
  E2E specs and David's own agent workflows, and it isn't needed once the elevation question lives outside every MCP
  tool (decision 13).

## Threat model

- **Attacker**: code already running as the logged-in user (malware, a compromised dependency, a hostile MCP client), an
  app the user granted Accessibility permission, or a person at an unlocked Mac.
- **Goal**: root file writes, which equal full root (a plist in `/Library/LaunchDaemons`, a file in
  `/private/etc/sudoers.d`), or reading files the user can't read.
- **Invariant**: every root action traces back to a human who typed the admin password within the window AND confirmed
  that operation in a UI the attacker can't drive.

## Design decisions, taken

1. **macOS 13+ only.** `SMAppService` daemons and XPC listener code-signing requirements need macOS 13. Below that, Cmdr
   keeps today's error and offers nothing.
2. **An `SMAppService` launch daemon, started on demand.** The helper binary and its launchd plist ship inside
   `Cmdr.app` and stay inert until registered; nothing is copied to `/Library`. No `RunAtLoad` and no `KeepAlive`:
   launchd starts the helper when Cmdr connects over XPC, and it exits when Cmdr disconnects or after a short idle
   period. It never runs at boot.
3. **Nothing is registered until the first refusal.** The first `[Allow]` registers the helper and walks the user
   through approval (§ First-use flow).
4. **The question is an out-of-process native alert: `[Cancel] [Skip] [Allow]`.** It's a `CFUserNotification`, the API
   `instance_lock.rs` already uses: macOS draws it in a separate process, so a webview bug, a prompt-injected agent, or
   an MCP call can't click it, and faking a click needs Accessibility permission. It supports up to three buttons plus
   checkboxes and blocks the operation's worker thread (❌ never the main thread) while the operation parks via
   `human_wait.rs`. There's no separate in-app toast: an in-webview `[Allow]` would be forgeable. Copy comes from the
   Rust catalog (`crate::intl::menu_t`); drafts in § Copy drafts.
   - **Allow**: registers and approves the helper on first use, asks for the password if the proof has expired, then
     resumes. It covers the rest of this operation.
   - **Skip**: skips this item. A checkbox extends it to every refused item in this operation.
   - **Cancel**: cancels the operation, same as the queue's cancel.
5. **Every elevated operation gets its own alert**, even while the password is still valid. Operations never elevate on
   their own.
6. **The admin password unlocks a Cmdr-specific right for 24 hours**, revoked early on screen lock, sleep, logout, fast
   user switching, and Cmdr quit. It's an Authorization Services right (for example
   `com.veszelovszki.cmdr.elevated-file-operations`) defined with `shared: false` and `timeout: 86400`. Cmdr holds the
   `AuthorizationRef` and sends its external form with each request; the helper verifies it with
   `AuthorizationCopyRights`, no interaction allowed. Revoking means Cmdr destroys the rights. (Rights carry `shared`
   and `timeout` attributes: `security authorizationdb read system.privilege.admin` shows `shared false`, `timeout 300`
   on macOS 26, 2026-09-15.)
7. **A registered helper does nothing without a valid proof.** That makes the persisted approval harmless: registration
   is a door, and the password is its key.
8. **Only Cmdr can connect.** The helper's listener sets `setConnectionCodeSigningRequirement` (macOS 13+) to our Team
   ID plus `com.veszelovszki.cmdr`. It's audit-token based, so PID reuse doesn't fool it, and non-matching callers are
   dropped before the helper sees them. Cmdr pins the helper's signature the same way (`setCodeSigningRequirement`).
9. **Nothing can inject code into Cmdr.** Decision 8 trusts whatever runs inside Cmdr, and the bundle now ships no
   entitlements at all, so that trust has something behind it (`docs/security.md` § Entitlements). Should Tauri ever
   need library validation off, the XPC client and the alert move into a tiny separate signed binary that keeps it on.
10. **The helper touches only the folder the user can't change; Cmdr does everything else as the user.** Its XPC API is
    a handful of primitives, all through directory file descriptors with `O_NOFOLLOW`, ❌ never a path string resolved
    as root:
    - `unlink_at(dir, name)`, `rmdir_at(dir, name)`, `mkdir_at(dir, name)`.
    - `rename_at(from_dir, from_name, to_dir, to_name)` via `renameatx_np` with `RENAME_EXCL`.
    - `create_temp_at(dir)`, which returns a writable descriptor for a new temp file. Cmdr streams the bytes itself
      (progress, cancel, fsync), then lands it with `rename_at`, keeping the engine's temp+rename landing.
    - ❌ No read, no chmod or chown, no shell, no "run this".
11. **Reads never escalate.** There's no read primitive. Cmdr opens every source as the user and passes that descriptor
    over XPC; before moving an entry, the helper checks that the descriptor's device and inode match it and refuses
    otherwise. So a root-only file inside a `700` folder can't be moved or copied somewhere readable. Deletes need no
    descriptor, since no content travels. The viewer, listings, search, indexing, and MCP resources always read as the
    user; a request to view root-only files in the viewer gets a no. Why this matters even though MCP can't write: MCP
    reads, the agent, the index, and any user-level process would all see whatever lands in a readable place, and a
    confirmed alert only guards the moment of the click; it can't control what happens to the data afterwards.
12. **Detect by refusal, typed by errno.** The engine runs as the user and elevates only on
    `PermissionRefusal::FolderPermissions` (`EACCES`), which `PermissionDenied` already carries. `SystemProtected`
    (`EPERM`) doesn't elevate (SIP, TCC, and immutable flags stop root too). Detection is free, since it's the refusal
    the engine already gets. As a hint, the scan preview may call `access(W_OK)` once per distinct source parent and
    destination so the transfer dialog can warn "needs administrator rights" before starting; the refusal stays the only
    trigger.
13. **MCP and the agent can see the wait but never answer it.** `cmdr://state` shows the parked operation as waiting for
    administrator approval, and no tool answers the alert. `autoConfirm` and `dialog confirm` stay: they reach only the
    normal confirmation. This is a deliberate exception to `apps/desktop/src-tauri/src/mcp/CLAUDE.md`'s "whatever a user
    can reach, an agent must reach, answer, and observe", to be written there in M4.
14. **A denylist as a backstop.** The helper refuses writes under `/Library/LaunchDaemons`, `/Library/LaunchAgents`,
    `/Library/PrivilegedHelperTools`, `/private/etc`, and `/var/root`, checked on the resolved descriptor
    (`fcntl(F_GETPATH)`), ❌ never on a caller's string. It can't list every root-run file, so it only backs up
    decisions 4–11.
15. **The setting**: Settings > Behavior > Navigation & file ops > "Allow administrator actions". The toggle reads the
    live `SMAppService` status, because users can switch the helper off in System Settings. Off: `unregister()` and
    destroy the proof. On: register, which may send the user to System Settings.
16. **Updates re-register.** On connect, Cmdr and the helper compare versions; on a mismatch, Cmdr unregisters and
    registers again so launchd runs the new binary (a running daemon keeps the old binary after an app update). The
    updater replaces the whole bundle, so this runs after every update.
17. **Visible.** Elevated operations get a badge in the queue and a line in the operation log. macOS lists the helper in
    Login Items & Extensions.
18. **Rollback goes through the same gate.** A rollback that needs an elevated step shows the alert again.

## First-use flow

1. An operation hits `EACCES`, parks, and shows the first-use alert. The user clicks `[Allow]`.
2. Cmdr registers the daemon. Its status becomes "requires approval", and macOS shows its "Background Items Added"
   notification.
3. Cmdr opens System Settings > General > Login Items & Extensions. The user turns Cmdr on and authenticates as an
   admin.
4. Cmdr re-reads the status when it becomes the active app again (an activation event, no polling). If it's still not
   enabled, the operation stays parked, the queue says why, and Cancel works.
5. Cmdr requests the right, and macOS shows its password dialog.
6. The operation resumes, refused steps go through the helper, and progress continues.

Later: alert, then the password only if the 24-hour proof expired or was revoked, then resume.

## Why 24 hours is acceptable

While the proof is valid, a root write still needs a click on the out-of-process alert. What gets through without the
password:

- **Code running inside Cmdr** skips the alert and uses the cached proof. With library validation on (decision 9), that
  takes a real exploit in Cmdr itself. A five-minute window narrows this but doesn't close it.
- **An app with Accessibility permission** (Keyboard Maestro, Raycast, BetterTouchTool) can click the alert. The window
  length matters most here; accepted.
- **A person at an unlocked Mac** can click `[Allow]`. Revoking on lock, sleep, and user switch closes most of it.

## Copy drafts

For David's review; not final.

- **Alert header**: "Moving this needs administrator rights" (one variant per operation).
- **Alert body, first use**: "Your macOS user can't change {folder}. Cmdr can finish this as an administrator. It
  installs a small helper that runs with full access to your Mac while Cmdr uses it, and it asks for your password once
  a day. Only allow this for files you trust."
- **Alert body, later**: "Your macOS user can't change {folder}. Cmdr can finish this as an administrator."
- **Buttons**: "Allow", "Skip", "Cancel". **Checkbox**: "Skip every item like this in this operation".
- **Queue status**: "Waiting for administrator approval".
- **Setting**: "Allow administrator actions". **Description**: "Cmdr asks before each operation, then moves, copies, or
  deletes files your macOS user can't change. A small helper with administrator rights runs in the background while Cmdr
  uses it."

## Milestones

The groundwork that needed no helper already shipped: the empty entitlements plist (`docs/security.md` § Entitlements)
and the typed errno plus proved folder that decision 12 branches on (`crates/cmdr-operations/DETAILS.md` § "Naming the
folder that refused a write").

- [ ] **M1: spike on macOS 13 and 26** (throwaway code). Answers every open question below, plus: daemon on-demand start
      and idle exit, descriptor passing with `renameatx_np`, and a `CFUserNotification` with three buttons and a
      checkbox from a worker thread. Questions 3 and 4 have a free head start: `PermissionDenied` already carries the
      errno, so a sticky-bit or TCC refusal can be read off a real report or a hand-made case before the spike builds
      anything.
- [ ] **M2: the helper.** Binary and plist in the bundle; signing (same Team ID, hardened runtime, no weakening
      entitlements); the XPC protocol with decision 10's primitives; the caller requirement, proof check, descriptor
      checks, denylist, idle exit, and version handshake. Unit tests for everything that runs unprivileged in temp dirs.
- [ ] **M3: engine integration.** Typed `EACCES` parks the operation, shows the alert, and routes only the refused steps
      through the helper, keeping temp+rename landing, cancel, rollback, and the journal. `cmdr://state` exposes the
      wait.
- [ ] **M4: settings, revocation, visibility.** The settings row with live status; revoke on lock, sleep, user switch,
      and quit; the queue badge and operation-log line; the MCP exception in `mcp/CLAUDE.md`.
- [ ] **M5: testing and review.** E2E through a compile-time-only mock of the alert and helper (❌ never in release
      builds); manual QA of the real flow on macOS 13 and 26; an outside security review of the helper before it ships;
      durable docs in the colocated `CLAUDE.md` / `DETAILS.md`.

## Open questions

1. **Re-registering**: after `unregister()`, does `register()` skip approval? One researcher reports no
   re-authentication; Apple hasn't confirmed. Either answer is fine for the UX, since the user toggled it.
2. **The 24-hour right**: does `AuthorizationCopyRights` honor `timeout: 86400` on a custom right, and does destroying
   the rights in Cmdr make the helper's check fail right away?
3. **TCC and the helper**: is a root launchd daemon refused when it writes into `~/Downloads` or `~/Documents`, even
   through a descriptor Cmdr opened? If yes, the helper can't rename across into those folders, and a move out of a
   refused folder becomes clone-then-delete: Cmdr clones as the user, and the helper unlinks the source, reusing the
   cross-volume move's keep-sources-until-fsynced path.
4. **Sticky-bit refusals**: does macOS return `EPERM` for them? If yes, recognize that case explicitly (parent has
   `S_ISVTX`, entry owned by someone else) so it can elevate too.
5. **Ownership**: who owns files the helper creates in a refused folder: root, the folder's owner, or the user?
6. **Helper language**: Rust (one toolchain, raw `xpc_*` plus objc2 bindings) or Swift (first-class `NSXPCConnection`
   and `SMAppService`)? Check any crate's health before depending on it.
7. **Uninstall**: the approval state reportedly outlives deleting the app. If that still holds on macOS 26, document
   that uninstalling Cmdr leaves a Login Items entry.

## Sources

Checked 2026-09-15.

- [TN2065](https://developer.apple.com/library/archive/technotes/tn2065/_index.html):
  `do shell script ... with administrator privileges` caches authentication for five minutes, per script.
- [SMAppService quick notes (theevilbit)](https://theevilbit.github.io/posts/smappservice/): daemon approval needs admin
  authentication; re-registering after approval doesn't ask again.
- [Apple forum 707482](https://developer.apple.com/forums/thread/707482): the approval state persists across reboots and
  app removal "to preserve user intent".
- [Apple forum 768592](https://developer.apple.com/forums/thread/768592): a running daemon keeps its old binary after an
  app upgrade until it's unregistered and registered again.
- [setConnectionCodeSigningRequirement](<https://developer.apple.com/documentation/foundation/nsxpclistener/setconnectioncodesigningrequirement(_:)>):
  macOS 13+.
- [CFUserNotification](https://developer.apple.com/documentation/corefoundation/cfusernotification): up to three
  buttons, plus checkboxes.
- [Learn XPC exploitation, part 3 (Wojciech Reguła)](https://wojciechregula.blog/post/learn-xpc-exploitation-part-3-code-injections/):
  library validation off as the standard way to inject into a trusted XPC client.
- Shipped privileged-helper escalations in this class:
  [AWS Client VPN, CVE-2024-30165](https://labs.reversec.com/posts/2024/04/exploiting-the-aws-client-vpn-on-macos-for-local-privilege-escalation-cve-2024-30165),
  [Plugin Alliance, CVE-2025-55076](https://almightysec.com/plugin-alliance-helpertool-xpc-service-local-privilege-escalation/).
