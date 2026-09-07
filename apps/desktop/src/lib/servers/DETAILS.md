# Servers: details

Depth for `CLAUDE.md`; read this before any non-trivial work here. The model (account → place → pin) and every decision
behind it: `docs/specs/servers-hub-plan.md`. Backend contracts: `crates/cmdr-sftp/DETAILS.md`,
`crates/cmdr-webdav/DETAILS.md`, and `apps/desktop/src-tauri/src/server_volumes.rs`.

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
string-prefix containment test would strip the root off it and ask the server for `-1/photos`.
`../file-explorer/pane/navigate.ts`'s `isUnderServerRoot` appends the separator for exactly that reason, and
`RemoteRoot::to_remote_path` refuses the same
three shapes (a bare server path, another server's prefix, a `..` escape) on the Rust side.

**The three root aliases** (`''`, `'/'`, `'.'`) all mean the volume root, on both sides. A backend's own code passes a
bare `/` for its root, which is why the Rust side keeps them too.

## The three arms

`connectPlace({ volumeId, connectionState, onAttemptStarted, openSignIn })` picks by standing:

- **`direct`, `os_mount` → nothing** (`already_live`). There is a session serving right now.
- **`disconnected` → `smbReconnectManager.startCycle`**, answering `reconnecting`. The volume is REGISTERED, so a dial
  would register a second one under a second id, and the backoff loop already owns recovery. The manager is idempotent,
  so landing on the same place twice costs nothing. The cycle itself:
  `../file-explorer/network/DETAILS.md` § "SMB live-reconnect flow".
- **`needs_sign_in` → the sheet, as a REGISTERED place.** The backend stopped retrying because a credential is missing,
  so re-dialing can't help, and a re-dial of a registered volume is the second-volume bug again.
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

❗ **It stays open across rounds.** A first connect to a new SFTP server is three round-trips (the host key, then the
credentials, then connected), and a sheet that closed between them would lose what the user typed and put the refusal
somewhere other than under the field it belongs to. `connect-flow.ts` decides WHEN a human is needed; the sheet decides
how many times to ask.

**The three SMB sites, and what each `attempt` runs** (`../file-explorer/network/smb-sign-in.ts`
builds all three requests, so the endpoint header, the remembered username, and the refusal vocabulary can't drift
between them):

- **A share listing** (`PlacesBrowser`) → `listSharesWithCredentials`. Server-level, so the header is `smb://<host>` and
  guest is offered where the host allows one. Cancelling goes back to the host list.
- **A share mount** (`../file-explorer/pane/NetworkMountView.svelte`) → `mountNetworkShare`, then `saveSmbCredentials`
  ❗ only once it went through. Cancelling goes back to the share list.
- **A "Connect directly" upgrade** (`../file-explorer/network/direct-connect.ts`) →
  `upgradeToSmbVolumeWithCredentials`, which stores the credential backend-side when the box is checked.

The last two pass `guestAllowed: false`: an unauthenticated attempt is what just came back refused, so offering it again
would be inert. All three answer `handed_off` on success (none of them connects a VOLUME), and hand anything that isn't
a credential refusal back to the pane, which has the words and the retry for it.

❗ **`open-sign-in.ts` is where the standing picks the command**: a REGISTERED volume is mended with
`reconnectVolumeWithCredentials`, an absent one is dialed with `connectSavedPlace`, and a typed server goes through
`connectServer`. SMB's add path has no target at all (its connect is a share MOUNT), so it answers `handed_off` and the
caller opens the host's places list.

**Remember, and who decides where it starts.** `add`: on, because someone typing a password into a new server means to
come back to it. `edit`: from `hasServerSecret`. `sign-in`: from the request's `remembered`, which the ❗ OPENER
decides, ❌ never the sheet: what it costs to find out is the protocol's business. SFTP and WebDAV ask
`hasServerSecret`, because an attended sign-in REFRESHES a remembered secret and ❌ never seeds one, so a default-on box
there would seed one the user already declined. SMB passes `true` unasked, because `has_smb_credentials` is
`get_credentials(…).is_ok()` and every read of that Keychain entry can cost a system prompt; nothing is written until a
sign-in works, and its caller writes it, so a checked box promises nothing that hasn't been shown.

Turning it OFF forgets the stored secret immediately; turning it on rides the next successful offer, because there is
nothing typed to save yet. ❌ Neither ever happens as a side effect of a dial.

**Edit mode's "this can't reconnect on its own" warning is the BACKEND's answer, ❌ never a derivation.**
`getSftpUnattendedReconnect` / `getWebdavUnattendedReconnect` say whether an unattended reconnect can work as things
stand, and the sheet asks when it RENDERS. ❌ Don't rebuild it from "auto-reconnect is on AND no secret is stored": the
rung a remote volume comes back on is decided per dial, so a derivation goes stale the moment one lands elsewhere, and
the two backends spell the same answer differently (`needs_stored_secret` vs `no_stored_secret`). The state worth
warning about is the silent one: auto-reconnect on, nothing stored, so nothing can ever happen.

**The host-key step replaces the body, ❌ never a second dialog.** First contact is routine and gets a plain primary
button; a changed key is the shape a man-in-the-middle takes, so it carries the red weight, says what else it can mean,
and puts its trust button behind a disclosure. A `superseded` approval starts the step over on the key the server REALLY
presents rather than silently trusting the one on screen; an `unreachable` one records nothing, because approving is a
live question and an unanswered one is not a yes.

❗ **A changed host key on a REGISTERED volume shows no fingerprint.** No backend command hands the PENDING host-key
prompt back for one, so the banner offers Disconnect and the fingerprint appears on the next open's dial
(`../file-explorer/pane/DETAILS.md` § "The connect views").

## The renderer table

One renderer per `SignInShape` variant, and ❗ **username editability is the VARIANT's property, ❌ never the sheet's
mode**. One implementer reading "read-only" as a mode rule breaks SMB; one reading "editable" as a mode rule breaks
SFTP.

- `nothing`: the sheet never opens. There is no secret a person could type that would help.
- `password`: the account as a read-only header, one password field. SFTP's and WebDAV's `reconnect_with_credentials`
  refuse a changed username, because the volume id IS the account.
- `key_passphrase`: the same, with the field labelled for a key file's passphrase and `autocomplete="off"`, because a
  passphrase is not the account's password and autofill must not offer one.
- `username_password { guestAllowed }`: username editable, plus a guest `RadioGroup` where the share allows one. SMB's
  reconnect accepts a new username and rewrites its params, which is how re-auth-as-someone-else works. The field
  carries the "Example: barry" placeholder, so an empty box says what kind of thing goes in it; the remembered username
  (`../file-explorer/network/smb-sign-in.ts`) fills the VALUE when there is one, and the placeholder steps aside.

**Reserved, ❌ not added until a producer exists** (`crates/cmdr-fs/src/volume/connection.rs` carries the same list):
`access_keys { sessionToken }` for S3 (access key id, secret access key, optional session token) and
`oauth { provider }` (a "Continue in your browser" button and a waiting state; "remember" is implicit, since the refresh
token is the only sane state, and a revoked token surfaces as `needs_sign_in` behind the same banner).

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

Keys live in `$lib/intl/messages/en/servers.json` under `servers.refusal.*`, reached through a `Record` in
`connect-refusals.ts` rather than a built string, which is what keeps `desktop-message-keys-unused` honest without a
dynamic-prefix entry.
`$lib/error-messages/friendly-error-style.test.ts` renders all of them and holds them to the same writing rules the
friendly-error copy obeys: they are error copy however they are filed.

**A second `Record` says WHICH FIELD each sentence goes under** (`refusalField`): the secret for the two that are about
a credential, the address for the four that are about the endpoint, and the form for the three no field can fix. ❗ A
refusal floating above a form reads as being about the whole form: "That password didn't work" under the password field
is an instruction, and the same words above the address are a puzzle.

## Add mode, address first

People have an address, not a protocol, so the field comes first and the toggle follows what `address-parser.ts` read.
The toggle stays editable, and ❗ **it is what decides which target `serverTargetFrom` builds**, not the address:
someone can type a bare host and say "that one is SFTP". A port the address named for a DIFFERENT protocol is dropped,
because `445` off a bare hostname is SMB's default and dialing it for HTTP opens a socket nothing answers on.

❗ **A bare hostname reads as SMB**, the one guess that costs nothing: SMB browses with no account, so a wrong guess
asks the user for nothing, while guessing SFTP would put an account field in front of someone who typed a NAS name off a
sticker. `user@host` reads as SFTP, because an account is what `user@` means.

❗ **A Nextcloud URL stays whole, path and all.** Nobody can tell where the base URL ends and the collection begins, and
the backend resolves the remote root relative to the base anyway. `not_a_webdav_server` is the one refusal with a remedy
button, because the fix is a collection path nobody knows; `certificate_untrusted` deliberately has none, since trusting
a certificate happens in Keychain Access.

There is no property-testing library on the frontend, so `address-parser.test.ts`'s example table IS the contract: a
shape that reaches the field and isn't in it is a shape nobody decided.

## The device dial, beside the place dial

A phone is dialed by the same seam and rendered by the same `RemoteConnectView`, but it does NOT come through
`connect-flow.ts`, and that is deliberate. `connect-flow` exists to pick between three moves a SERVER can need
(subscribe to a running backoff, mend a registered volume's credentials, or dial an absent one) and to loop a sign-in
sheet through as many rounds as the user retries. A phone has none of that: there is no credential, no backoff loop, no
sheet, and no registered-versus-absent question, since `connect_adb_device` answers an already-dialed device without a
second dial. So `../file-explorer/pane/device-connect.svelte.ts` calls `connectAdbDevice` directly, and what the two
share is the typed `RemoteConnectState`, the pane-not-dialog rule, and the attempt-id-before-the-dial rule.

❗ The attempt-id prefixes are separate on purpose (`server-connect-…` vs `adb-connect-…`): ADB files attempts in its
own table (`src-tauri/src/adb/volume_wiring.rs`), so a cancel aimed across the two answers a silent `false`. The words
are separate too (`$lib/adb/adb-connect-errors.ts` vs `connect-refusals.ts`): a phone's reasons and a server's share
nothing but shape.

## Which server a command acts on

`server-command-target.ts` is a pure resolver over two readings of "the server in view":

1. **The hub's cursor row** (`ExplorerAPI.getFocusedPaneServerRow()`, which reaches through `NetworkCursorEntry`'s
   `server` arm). A row with a `volumeId` wins outright.
2. **The focused pane's own volume**, for a pane standing INSIDE a server, filtered through `isServerPlaceRow` so a
   mounted SMB share, the synthetic hub row, and a local disk all answer `null`.

Two details are load-bearing:

- **An SMB host row stops the search** rather than falling through to reading 2. Its places are mounted shares whose ids
  `statfs` mints, so there is nothing for `disconnectPlace` or `setPlacePinned` to act on, and quietly acting on the
  pane's volume instead would move a server the user isn't pointing at.
- **A target resolved from the pane's volume reports `pinned: null`**, because a `VolumeInfo` carries no pin (the pin is
  the switcher's cap, decided in Rust, and deliberately off the wire). `servers.togglePin` reads `listSavedServers()`
  for that case; a store that doesn't answer reads as unpinned, which makes the command a pin rather than a no-op.

The handlers themselves (`src/routes/(main)/command-handlers/servers-handlers.ts`) then route through
`../file-explorer/navigation/server-row-actions.ts::runServerRowAction`, the same function the native menu's answer
lands in, so a menu item and a palette command can't drift on a confirmation or a toast.
