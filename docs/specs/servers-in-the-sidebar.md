# Servers in the sidebar: one front end for SFTP and WebDAV

**The problem**: Cmdr can connect to an SFTP server and a WebDAV server, list them, read them, write them, and reconnect
after a drop, and a user has no way to reach any of it. Both backends are done and both are invisible: nothing puts a
server on screen, nothing takes an address, and nothing signs in. This is the last thing between two finished backends
and two shipped features.

❗ **One design, both backends, ❌ never two.** They differ in exactly two places (a host-key approval step SFTP has and
WebDAV doesn't; an untrusted-certificate refusal WebDAV has and SFTP doesn't), and every other surface is the same
shape: a saved-server list, a connect form, a typed outcome to branch on, a sign-in prompt, and a reconnect banner.
Building WebDAV's first and SFTP's later is how they end up as two half-matching dialogs.

❌ Nothing here restates a mechanism. The commands, every connect outcome, and the reconnect model live in
`crates/cmdr-webdav/DETAILS.md` § "Connecting from the frontend" and `crates/cmdr-sftp/DETAILS.md` § the same. Read
those first; this file is the plan, not the contract.

## What already exists

More than it looks like. The backends went in with the frontend in mind, so the UI is mostly rendering:

- **Every command is there and typed.** `connectWebdavVolume` / `connectSftpVolume` answer a tagged union
  (`connected | authentication_rejected | needs_credentials | auth_method_unsupported | certificate_untrusted | not_a_webdav_server | timed_out | unreachable | invalid_url | cancelled`),
  ❌ never a message to parse. The saved-server trio (`get` / `update` / `forget`) and the credential trio (`save` /
  `has` / `delete`) are wired for both.
- **Cancel is already possible.** The attempt id is the CALLER's and made before the call, so a dialog can arm its
  cancel button while a dial hangs (`newWebdavAttemptId()`, then `cancelWebdavConnect(id)`).
- **Reconnect and sign-in are backend-neutral already.** `reconnectSmbVolume`, `reconnectSmbVolumeWithCredentials`, and
  `getVolumeSignInState` serve all three backends despite the SMB names, and `smb-reconnect-manager.svelte.ts` already
  drives its backoff off the backend-neutral `volume-connection-changed` event.
- **The "can this even come back?" question is answered by the backend.** `getWebdavUnattendedReconnect(volumeId)`
  returns `possible | switch_off | no_stored_secret`, so ❌ no UI derives it from a credential check.

## What's missing, in the order it wants building

### M1. A server is a thing on screen

A connected volume is registered and navigable by `volumeId`, but `volume_listing::complete` has no SFTP or WebDAV arm,
so nothing reaches the sidebar. Add one, the way `append_mtp_volumes` does: a registered remote volume becomes a
`LocationInfo` under a Servers section, enriched from the registry like everything else (❗ appended BEFORE
`enrich_from_volume_registry`, or it ships with `capabilities: None` and the pane falls back to per-kind defaults).

Design calls for David, ❗ not derivable from the code:

- Where the section sits, what it's called, and what a server's icon is (one icon, or one per protocol).
- Whether a SAVED-but-not-connected server shows there too, greyed, or only lives in the connect dialog. This is the
  biggest call in the whole effort: it decides whether the sidebar is "what's mounted" or "what you have".
- What Eject means on a server. `disconnectWebdavVolume` drops the client and unregisters; forgetting the server and
  forgetting its password are two further, separate acts.

### M2. Path resolution agrees with the sidebar

`resolve_path_volume` / `resolve_location` don't answer for a remote path. ❗ Both backends spell paths as plain
absolute paths with no scheme (`/Photos/2024`), exactly like a local one, so whatever M1 decides about identity,
resolution has to agree or a saved tab reopens on the wrong volume. Do this WITH M1, ❌ not after: a sidebar entry whose
path doesn't resolve is a tab that can't be restored.

### M3. One connect dialog

`ConnectToServerDialog.svelte` is SMB's; this is its sibling, or its widening. The form is protocol-first: pick SFTP or
WebDAV, then address, username, password, and a "remember" checkbox. WebDAV also takes a base URL path (a Nextcloud
address is `https://host/remote.php/dav/files/ada/`), SFTP also takes a key file.

- ❗ **Arm the cancel button before the call.** The promise doesn't settle until the dial is over, which can be half a
  minute against a hung server.
- ❗ **Branch on the outcome, ❌ never on a message.** Two pairs are easy to collapse and must not be:
  `needs_credentials` is not `authentication_rejected` (telling someone who has never typed a password that theirs is
  wrong), and `auth_method_unsupported` is not either (a Digest-only server never saw the secret, so "check your
  password" is the wrong fix). `certificate_untrusted` has no in-app remedy yet: say the certificate isn't trusted and
  point at the OS keychain rather than offering a button that can't work.
- **Copy is David's**, per principle 4. The outcomes above are the full list a person can meet.

### M4. The sign-in banner on a volume that dropped

The reconnect manager already fires; what's missing is what the banner says and offers. `getVolumeSignInState(volumeId)`
answers live what a sign-in would ask for, and `getWebdavUnattendedReconnect` says whether waiting is even worth it. ❗
`no_stored_secret` is the state to warn about (auto-reconnect is on and nothing can happen), and it wants a "remember
the password" affordance rather than a spinner.

### M5. SFTP's host-key approval, the one step WebDAV has no equivalent of

The two-phase approval and the per-rung banner table are already written down in `crates/cmdr-sftp/DETAILS.md`. It is a
step in the SFTP arm of M3, ❌ not a second dialog.

## Deliberately not in this effort

- **Trust-on-first-use for a self-signed certificate.** Most NAS boxes present one, so this is the biggest gap after the
  UI, but it is backend work with its own design (`docs/specs/webdav-backend-follow-ups.md` § 2). Until it lands,
  `certificate_untrusted` is a dead end the dialog has to word honestly.
- **Digest auth**, **Nextcloud chunked uploads**, and **quota confirmation**: same file, § 3-5.
- **A file-manager-wide "recent servers" concept.** The saved list per backend is enough for a first version.

## Cost

Roughly a week for both backends together, most of it M1 and M3. M2 is small but must ride with M1. David designs and
builds it; the backend side has nothing left to add.
