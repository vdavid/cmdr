# What the servers branch's review left open

The pre-merge review of the servers hub raised findings across four areas; most were fixed on the branch and live in git
history. What stayed open is here, verified against the code as it stands, so each item is schedulable on its own rather
than lost in a transcript. Nothing below restates a mechanism: every item points at the file that owns it.

## Product calls for David

Four items where the answer is a decision rather than a fix.

- [ ] **The SMB sign-in sheet seeds "Remember in Keychain" ON.** `apps/desktop/src/lib/file-explorer/network/smb-sign-in.ts:82`
      passes `remembered: true` unconditionally. **Problem**: the sheet's rule is that sign-in mode seeds the box from
      what the Keychain already holds, so a default-on box re-checks itself for a user who unchecked it last time.
      **Impact**: a successful share listing writes the credential (`PlacesBrowser.svelte`), so an SMB password can be
      stored by someone who declined it once. **Solution**: either seed SMB from `has_smb_credentials` like every other
      protocol, or ratify the deviation, on the reading that for SMB a share listing's sign-in IS a first connect and a
      first connect defaults to remembering. **Size**: an hour either way, plus a cell.
      **Tradeoff**: the two readings disagree about what an SMB sign-in is, and only David can pick.

- [ ] **A bare server-absolute path gets joined onto the app root instead of refused.**
      `crates/cmdr-fs/src/volume/mod.rs:1445`. **Problem**: `root_anchored` runs before the containment check at every
      app site that turns a user-typed path into an absolute one, so `/srv/data/photos` on a volume rooted at
      `sftp://ada@nas:22/srv/data` becomes `sftp://…/srv/data/srv/data/photos`, which then passes containment as a
      legal-but-wrong server path. The typed refusal in `crates/cmdr-fs/src/volume/remote_paths.rs` is correct and never
      gets the chance to fire. **Impact**: the whole guarantee rests on "the app never spells a remote path bare". The
      one free-text entry point is the transfer dialog's destination box
      (`apps/desktop/src-tauri/src/file_system/write_operations/routing.rs:54`), whose dialect is a leading-slash
      volume-relative path, indistinguishable from a server-absolute one. **Solution**: either accept the premise
      explicitly and say so beside `root_anchored`, or give the remote volumes their own anchoring that refuses a bare
      path rather than joining it. **Size**: an afternoon to document, two days to change the seam.
      **Tradeoff**: refusing at the seam costs the transfer box its current dialect for remote destinations.

- [ ] **Launch dials a phone, and nothing can call it off.** `apps/desktop/src/lib/file-explorer/pane/initialization.ts:81`
      resolves every restored tab through `resolvePathVolume`, and the resolver's `adb://` arm connects the device as a
      side effect (`apps/desktop/src-tauri/src/commands/volumes.rs:127`). **Problem**: the `sftp://` / `webdav://` arm
      one branch below deliberately never dials, for the reason its own comment gives. ADB does, under the backend's own
      attempt id, before any pane exists. **Impact**: startup blocks on a phone dial with no cancel affordance, which is
      the wait the pane's connect view was built to own. **Solution**: either make the `adb://` arm resolve without
      dialing the way the server arm does, or accept that a restored phone tab connects at launch and write that down.
      **Size**: half a day for the non-dialing arm, and it needs a look at what the pane then shows.
      **Tradeoff**: not dialing means a restored phone tab lands on a connect view rather than the user's files.

- [ ] **`VolumeBreadcrumb.svelte` is 1,845 lines against a 1,639 allowlist entry.**
      `apps/desktop/src/lib/file-explorer/navigation/VolumeBreadcrumb.svelte`. **Problem**: the servers work grew the
      file past its recorded size, and the allowlist can't be raised without consent. **Impact**: `file-length` warns on
      every run until it is split or the number moves. **Solution**: the switcher's server rows, their menus, and the
      connection dot are the natural seam for a child component; failing that, David bumps the entry.
      **Size**: a day for the split.
      **Tradeoff**: a split is the right end state, but it touches the busiest file in the navigation area.

## Major

- [ ] **Twenty pointers into `docs/specs/servers-hub-plan.md` from code and colocated docs, across 19 files.**
      For example `apps/desktop/src/lib/servers/connect-flow.ts`,
      `apps/desktop/src/lib/file-explorer/navigation/server-row-actions.ts`,
      `apps/desktop/src/lib/file-explorer/navigation/DETAILS.md`, and
      `apps/desktop/src-tauri/src/menu/menu_structure.rs`. **Problem**: `docs/specs/` is wiped periodically by design, so
      each of these rots at a scheduled event rather than a random one. **Impact**: twenty comments and doc lines start
      citing a file that isn't there, and the rationale they carry is gone with it. **Solution**: move each cited
      decision into the nearest `DETAILS.md` and repoint the comment at that, which is the wipe procedure anyway.
      **Size**: half a day, and it is most of what the plan's own wipe would cost.
      **Clear win**: the wipe has to do this work regardless.

- [ ] **A saved SMB host on a non-445 port is listed twice in the hub.**
      `apps/desktop/src-tauri/src/commands/servers.rs:347`. **Problem**: the dedupe compares `address`, and the two
      sources spell it differently: a share row carries `share.server_name` and never a port, while a manually-typed
      entry carries `host` or `host:port`. **Impact**: `nas.local` typed on port 4450 never matches its own share row,
      so the hub shows two rows for one machine. **Solution**: dedupe on the host alone, or on the id
      `generate_server_id` already derives from `(host, port)`. **Size**: an hour with a cell.
      **Clear win.**

- [ ] **Nothing asserts that disconnecting a place tells the panes.**
      `apps/desktop/src-tauri/src/commands/servers_test.rs:390`. **Problem**: the cell's own heading is
      "Disconnecting a place tells the panes too", and its body asserts only the negative: no session means nothing is
      announced. It would pass with the `emit_volume_gone` call deleted. **Impact**: the guarantee a pane depends on
      (drop the session, emit `VolumeUnmounted` with the volume id, keep the row as `saved`) has no test. **Solution**:
      a second cell that registers a session, disconnects, and asserts both halves. **Size**: an hour.
      **Clear win.**

## Minor

- [ ] **`OneShotCredentials::save_credentials` refuses forever, including after `forget()`.**
      `apps/desktop/src-tauri/src/network/one_shot_credentials.rs:129`. **Problem**: the refusal is unconditional rather
      than tied to the live offer, and the volume keeps the wrapper for its whole life. **Impact**: harmless today,
      because the only writer never seeds. A volume dialed once with `remember: false` can never persist a secret again,
      and a future "sign in and remember on this volume" writer would fail on those volumes only.
      **Solution**: gate the refusal on `self.offer.lock().is_some()`, so the type says what it means. **Size**: an hour.
      **Clear win.**

- [ ] **The adb path field commits on blur only.** `apps/desktop/src/lib/settings/sections/AdbSection.svelte:175`.
      **Problem**: no Enter commit and no commit on unmount. **Impact**: typing a path and closing the Settings window,
      or pressing Enter and assuming it took, silently drops the edit, and this field decides which binary the tracker
      restarts under. **Solution**: commit on Enter and on unmount. **Size**: an hour.
      **Tradeoff**: `UpdatesSection` is blur-only too, so fixing one field alone makes the house shape inconsistent.

- [ ] **The host-key-changed pane has no button, where the plan promised one.**
      `apps/desktop/src/lib/intl/messages/en/servers.json:362`. **Problem**: `paneState.hostKeyChangedHint` tells the
      user to "Disconnect, then open it again to check the fingerprint", which is two manual steps.
      **Impact**: the one moment the app most wants the user to look at a fingerprint is the one it makes hardest.
      **Solution**: a "Look at the key" button on that pane state that opens the host-key sheet directly.
      **Size**: half a day, sheet plumbing plus copy in 10 locales.
      **Clear win.**

- [ ] **An unreachable server is named by its hostname, not the name the user gave it.**
      `apps/desktop/src/lib/intl/messages/en/servers.json:151`. **Problem**: there is no `paneState.unreachable` key, so
      the pane falls back to `servers.refusal.unreachable` ("Cmdr couldn't reach {host}.") plus
      `fileExplorer.unreachable.title`. **Impact**: the user named the server and gets a hostname back.
      **Solution**: a `paneState` key taking `{name}`, the way every other pane state does. **Size**: two hours with the
      translations. **Clear win.**

- [ ] **An overdue migration is still running.** `apps/desktop/src/lib/settings/ai-config.ts:54` says "Remove this
      migration after 2026-09-01". **Problem**: the date has passed. **Impact**: dead code carrying a promise nobody
      kept; pre-existing, unrelated to the servers work. **Solution**: delete the migration and its TODO.
      **Size**: half an hour. **Clear win.**

## Nits and copy

Each of these is under an hour.

- [ ] **`to_app_path` doesn't check containment.** `crates/cmdr-fs/src/volume/remote_paths.rs:118` prefixes whatever the
      server answered, so an href above the collection becomes an app path outside the volume, which the way back then
      refuses. Only reachable through a misbehaving server. **Clear win.**
- [ ] **WebDAV keeps its own copy of `normalize`.** `crates/cmdr-webdav/src/volume/paths.rs:48` is verbatim
      `crates/cmdr-fs/src/volume/remote_paths.rs:141`, kept because `root_remote_path` needs it before a volume exists.
      Exporting the `cmdr-fs` one removes the copy that can drift. **Clear win.**
- [ ] **A test that passes for the wrong reason.**
      `apps/desktop/src/lib/file-explorer/navigation/connection-tooltips.test.ts:22` claims a new state fails to
      typecheck, but `STATES` is an array literal annotated `ConnectionState[]`, so a seventh state compiles and the test
      keeps passing. A `Record<ConnectionState, true>` key list would make the claim true. **Clear win.**
- [ ] **The ADB hint's phone twin is matched by display name.**
      `apps/desktop/src/lib/adb/should-show-adb-hint.ts:38` counts rows sharing a name, so two Pixel 7s on MTP alone
      suppress the hint for both, though neither has USB debugging on. Pairing by serial through the volume path is
      exact. **Clear win.**
- [ ] **A tautological cell.** `apps/desktop/src-tauri/src/network/one_shot_credentials_test.rs:99` constructs
      `SecretOffer { remember: false }` and asserts `!offer.remember`. It would pass with the field's meaning inverted
      everywhere else. Either assert the serde shape the IPC boundary needs or drop it. **Clear win.**
- [ ] **Straight quotes around a command name.** `apps/desktop/src/lib/intl/messages/en/servers.json:10` uses `"` where
      `servers.sheet.needsStoredSecret` uses typographic quotes for the same job, and Hungarian already uses its own.
      **Clear win.**
- [ ] **A placeholder that reads as an instruction.**
      `apps/desktop/src/lib/intl/messages/en/settings.json:2955` is "Look for adb the usual way" in an empty path field,
      which sounds like a command to the user. The description one line up
      (`apps/desktop/src/lib/intl/messages/en/settings.json:1312`) already says the right thing: "Cmdr looks for adb the
      usual way". Hungarian renders it as a noun phrase and is correct. **Clear win.**
- [ ] **"Press" for an on-screen button.** `apps/desktop/src/lib/intl/messages/en/settings.json:2951` says "then press
      Re-check:". The house verb is choose or select. **Clear win.**
- [ ] **Two snags in one description.** `apps/desktop/src/lib/intl/messages/en/settings.json:1304` puts straight quotes
      around "adb", and "costs nothing when you have no Android tooling installed" should be "if". **Clear win.**
- [ ] **"where {name} shows" isn't idiomatic.**
      `apps/desktop/src/lib/intl/messages/en/fileExplorer.json:1848` should say "where {name} appears"; the Hungarian
      translation already does. **Clear win.**
- [ ] **The two busy tooltips read as backend register.**
      `apps/desktop/src/lib/intl/messages/en/fileExplorer.json:1830` and
      `apps/desktop/src/lib/intl/messages/en/adb.json:73` say "Can't disconnect while operations are in progress on this
      server / device", with no terminal period where every other new tooltip has one. The pre-existing eject twin at
      `apps/desktop/src/lib/intl/messages/en/fileExplorer.json:1913` has the same shape, so this is a three-line pass or
      none. **Tradeoff**: rewording one means rewording all three.
- [ ] **A missing contraction.** `apps/desktop/src/lib/intl/messages/en/settings.json:2307` says "The SSH host keys you
      have trusted."; the house voice asks for "you've". **Clear win.**
- [ ] **A hidden setting nobody can parse.** `apps/desktop/src/lib/intl/messages/en/settings.json:1886` is "Long Network
      group hint shown". It never renders in the UI, but it does surface in settings search. **Clear win.**
