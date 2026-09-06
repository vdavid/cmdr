# Servers: details

Depth for `CLAUDE.md`. The model (account → place → pin) and every decision behind it: `docs/specs/servers-hub-plan.md`.

## The path grammar

An SFTP place's app-facing paths are `sftp://<user>@<host>:<port>/<server path>`; a WebDAV place's are
`webdav://<user>@<host>:<port>/<remote path>`. `server-path-utils.ts` is the frontend's reader and writer; the Rust twin
is `cmdr_fs::volume::remote_paths::RemoteRoot` (translation both ways) over the prefix
`cmdr_fs::volume::ids::sftp_app_root` / `webdav_app_root` mints.

**Why a scheme and not a hint.** `commands/volumes.rs::resolve_path_to_volume` falls through to the mount table, which
on both platforms walks up to `/` and answers the LOCAL root for any absolute path it doesn't recognize. So a
scheme-free `/srv/data/x` is wrong at every resolver site the app has (a restored tab, a favorite, a drag between panes,
go-to-path, MCP, the trash), and the next site added would forget again. The scheme makes the path self-describing, and
it is the convention `mtp://` and `adb://` already carry.

**The folding has to match Rust's exactly.** The host is lowercased (DNS is case-insensitive), the account is NOT (a
POSIX account may be case-sensitive, and `Ada` and `ada` can be two people), and the port is literal. A path folded
differently misses the volume its own id names, which is the same tuple `sftp_volume_id` hashes.

**Matching is by whole components, never a string prefix.** `/srv/data-1` is a legal sibling of `/srv/data`, and a
string-prefix containment test would strip the root off it and ask the server for `-1/photos`. `navigate.ts`'s
`isUnderServerRoot` appends the separator for exactly that reason, and `RemoteRoot::to_remote_path` refuses the same
three shapes (a bare server path, another server's prefix, a `..` escape) on the Rust side.

**The three root aliases** (`''`, `'/'`, `'.'`) all mean the volume root, on both sides. A backend's own code passes a
bare `/` for its root, which is why the Rust side keeps them too.

## The three arms

`connectPlace({ volumeId, connectionState, onAttemptStarted, openSignIn })` picks by standing:

- **`direct`, `os_mount` → nothing** (`already_live`). There is a session serving right now.
- **`disconnected` → `smbReconnectManager.startCycle`**, answering `reconnecting`. The volume is REGISTERED, so a dial
  would register a second one under a second id, and the backoff loop already owns recovery. The manager is idempotent,
  so landing on the same place twice costs nothing.
- **`needs_sign_in` → the sheet, as a REGISTERED place.** The backend stopped retrying because a credential is missing,
  so re-dialing can't help — and a re-dial of a registered volume is the second-volume bug again.
- **`saved`, or nothing → `connectSavedPlace`.** Nothing is registered, so this is the first dial. If THAT answers
  `needs_host_key_approval`, `needs_credentials`, or `authentication_rejected`, the place goes to the sheet as an ABSENT
  one, carrying the outcome so the sheet opens on the right step.

❗ **`auth_method_unsupported` never reaches the sheet.** The server challenged with a scheme Cmdr doesn't speak, the
secret never left, and no typing fixes it. A password box over that asks for something that cannot help.

The `openSignIn` seam takes `{ volumeId, registered, firstOutcome? }` and answers
`{ signedIn: true, volumeId } | { signedIn: false }`. `open-sign-in.ts` supplies it; without one the flow refuses with
the reason instead, and ❌ never renders an inert "Sign in…" button.

`cancelled` returns silently in every arm: the user pressed the button and telling them what they just did is noise.

**A `SavedPlaceRefusal` thrown by `connect_saved_place`** (`no_such_server`, `already_connected`) means the CALLER
picked the wrong arm. It is logged and shown as `unreachable`, because there is no sentence for it that helps a person:
a user should never see one, and the log line is what a maintainer needs.

## The sheet contract

**The sheet owns the form; the caller owns the protocol.** `SignInSheet.svelte` renders what a `SignInShape` says to
ask, puts the refusal under the field it is about, and calls a caller-supplied `attempt` as many times as the user
retries. It ❌ never dials. That is what lets SMB's three sites (a share listing, a share mount, a reconnect) reuse it
with their own commands, and what lets S3 plug in with one more renderer.

❗ **It stays open across rounds.** A first connect to a new SFTP server is three round-trips — the host key, then the
credentials, then connected — and a sheet that closed between them would lose what the user typed and put the refusal
somewhere other than under the field it belongs to. `connect-flow.ts` decides WHEN a human is needed; the sheet decides
how many times to ask.

❗ **`open-sign-in.ts` is where the standing picks the command**: a REGISTERED volume is mended with
`reconnectVolumeWithCredentials`, an absent one is dialed with `connectSavedPlace`, and a typed server goes through
`connectServer`. SMB's add path has no target at all (its connect is a share MOUNT), so it answers `handed_off` and the
caller opens the host's places list.

**Remember, per mode.** `add`: on, because someone typing a password into a new server means to come back to it.
`sign-in` and `edit`: seeded from `hasServerSecret`. ❗ An attended sign-in REFRESHES a remembered secret and ❌ never
seeds one, so a default-on box would seed one the user already declined. Turning it OFF forgets the stored secret
immediately; turning it on rides the next successful offer, because there is nothing typed to save yet. ❌ Neither ever
happens as a side effect of a dial.

**The host-key step replaces the body, ❌ never a second dialog.** First contact is routine and gets a plain primary
button; a changed key is the shape a man-in-the-middle takes, so it carries the red weight, says what else it can mean,
and puts its trust button behind a disclosure. A `superseded` approval starts the step over on the key the server REALLY
presents rather than silently trusting the one on screen; an `unreachable` one records nothing, because approving is a
live question and an unanswered one is not a yes.

## The renderer table

One renderer per `SignInShape` variant, and ❗ **username editability is the VARIANT's property, ❌ never the sheet's
mode**. One implementer reading "read-only" as a mode rule breaks SMB; one reading "editable" as a mode rule breaks
SFTP.

- `nothing`: the sheet never opens. There is no secret a person could type that would help.
- `password`: the account as a read-only header, one password field. SFTP's and WebDAV's `reconnect_with_credentials`
  refuse a changed username, because the volume id IS the account.
- `key_passphrase`: the same, with the field labelled for a key file's passphrase and `autocomplete="off"` — a
  passphrase is not the account's password, and autofill must not offer one.
- `username_password { guestAllowed }`: username editable, plus a guest `RadioGroup` where the share allows one. SMB's
  reconnect accepts a new username and rewrites its params, which is how re-auth-as-someone-else works.

**Reserved, ❌ not added until a producer exists** (`crates/cmdr-fs/src/volume/connection.rs` carries the same list):
`access_keys { sessionToken }` for S3 (access key id, secret access key, optional session token) and `oauth { provider }`
(a "Continue in your browser" button and a waiting state; "remember" is implicit, since the refresh token is the only
sane state, and a revoked token surfaces as `needs_sign_in` behind the same banner).

## The refusal table

- `authentication_rejected`: the credential was offered and refused. Names the account.
- `needs_credentials`: nothing was ever offered. ❌ Not a rejection.
- `auth_method_unsupported`: the server challenged with a scheme Cmdr doesn't speak, so the secret never left. ❌ Never
  name the scheme; "Digest" means nothing to the reader.
- `certificate_untrusted`: macOS doesn't trust the certificate, and the fix is Keychain Access. Trust-on-first-use is
  backend work (`docs/specs/webdav-backend-follow-ups.md` § 2).
- `not_a_webdav_server`: the address answers HTTP but not WebDAV. The one refusal with a remedy button.
- `invalid_url`: the saved address isn't a usable web address.
- `timed_out` and `unreachable`: about the SERVER, so they name the host rather than the account.
- `host_key_untrusted` (from `needs_host_key_approval`): the sheet's key step is where the fingerprint is shown and
  approved.
- `host_key_revoked`: deliberately final. No button can safely undo a revocation the user's own `known_hosts` records.

Keys live in `messages/en/servers.json` under `servers.refusal.*`, reached through a `Record` in `connect-refusals.ts`
rather than a built string, which is what keeps `desktop-message-keys-unused` honest without a dynamic-prefix entry.
`error-messages/friendly-error-style.test.ts` renders all of them and holds them to the same writing rules the
friendly-error copy obeys: they are error copy however they are filed.

**A second `Record` says WHICH FIELD each sentence goes under** (`refusalField`): the secret for the two that are about
a credential, the address for the four that are about the endpoint, and the form for the three no field can fix. ❗ A
refusal floating above a form reads as being about the whole form: "That password didn't work" under the password field
is an instruction, and the same words above the address are a puzzle.

## Add mode, address first

People have an address, not a protocol, so the field comes first and the toggle follows what `address-parser.ts` read.
The toggle stays editable, and ❗ **it is what decides which target `serverTargetFrom` builds**, not the address: someone
can type a bare host and say "that one is SFTP". A port the address named for a DIFFERENT protocol is dropped, because
`445` off a bare hostname is SMB's default and dialing it for HTTP opens a socket nothing answers on.

❗ **A bare hostname reads as SMB**, the one guess that costs nothing: SMB browses with no account, so a wrong guess asks
the user for nothing, while guessing SFTP would put an account field in front of someone who typed a NAS name off a
sticker. `user@host` reads as SFTP, because an account is what `user@` means.

❗ **A Nextcloud URL stays whole, path and all.** Nobody can tell where the base URL ends and the collection begins, and
the backend resolves the remote root relative to the base anyway. `not_a_webdav_server` is the one refusal with a remedy
button, because the fix is a collection path nobody knows; `certificate_untrusted` deliberately has none, since trusting
a certificate happens in Keychain Access.

There is no property-testing library on the frontend, so `address-parser.test.ts`'s example table IS the contract: a
shape that reaches the field and isn't in it is a shape nobody decided.

## What later milestones fill in

- SMB's three credential sites move onto the sheet, `NetworkLoginForm`'s in-pane rendering and `smb-login-hosts.ts` go,
  and `RemoteConnectView` gains `gave_up` when `VolumeUnreachableBanner`'s `smbGaveUp` variant retires with it. Two
  renderers for one state would be worse than one in the wrong file.
- `waiting_for_device`, with the ADB work that produces it. ❗ A state added before its handler puts a button on screen
  that does nothing.
- A backend command handing back the PENDING host-key prompt for a registered volume. Until then the changed-key banner
  offers Disconnect, and the fingerprint appears on the next open's dial (`pane/DETAILS.md` § the connect views).

## Which server a command acts on

`server-command-target.ts` is a pure resolver over two readings of "the server in view":

1. **The hub's cursor row** (`ExplorerAPI.getFocusedPaneServerRow()`, which reaches through `NetworkCursorEntry`'s
   `server` arm). A row with a `volumeId` wins outright.
2. **The focused pane's own volume**, for a pane standing INSIDE a server, filtered through `isServerPlaceRow` so a
   mounted SMB share, the synthetic hub row, and a local disk all answer `null`.

Two details are load-bearing:

- **An SMB host row stops the search** rather than falling through to reading 2. Its places are mounted shares whose ids
  `statfs` mints, so there is nothing for `disconnectPlace` or `setPlacePinned` to act on — and quietly acting on the
  pane's volume instead would move a server the user isn't pointing at.
- **A target resolved from the pane's volume reports `pinned: null`**, because a `VolumeInfo` carries no pin (the pin is
  the switcher's cap, decided in Rust, and deliberately off the wire). `servers.togglePin` reads `listSavedServers()`
  for that case; a store that doesn't answer reads as unpinned, which makes the command a pin rather than a no-op.

The handlers themselves (`routes/(main)/command-handlers/servers-handlers.ts`) then route through
`navigation/server-row-actions.ts::runServerRowAction`, the same function the native menu's answer lands in, so a menu
item and a palette command can't drift on a confirmation or a toast.
