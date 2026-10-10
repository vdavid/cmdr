# Servers: details

Depth for `CLAUDE.md`; read this before any non-trivial work here. Backend contracts: `crates/cmdr-sftp/DETAILS.md`,
`crates/cmdr-webdav/DETAILS.md`, and `apps/desktop/src-tauri/src/server_volumes.rs`.

## The model: account, place, pin

Three levels. Every rule in this file is a consequence of them.

1. **Account**: an endpoint plus an identity. An SMB host, an SFTP `user@host:port`, a WebDAV URL plus a username, an S3
   endpoint plus an access key ID, later a Google account. It owns the credential, the trust (host keys, certificates,
   refresh tokens), and the auto-reconnect switch. Saved, and never itself navigable.
2. **Place**: the mountable thing under an account. An SMB share, an SFTP or WebDAV root, an S3 bucket or the account
   root that lists the buckets, later a shared drive. A place becomes a `VolumeInfo`, and it is what tabs, favorites,
   and paths point at. SFTP and WebDAV have exactly one place per account; SMB and S3 have many, which is why the SMB
   host → shares step is the general shape rather than an oddity (`../file-explorer/network/DETAILS.md` § "The pure
   modules beside it" has the hub's account → places rows).
3. **Pin**: whether a place shows in the volume switcher.

❗ **Why three levels and not "a server is a volume".** A server is not plugged in, and nothing else reminds a person it
exists, so the second use of one has to cost a single keystroke or the feature reads as "connect" rather than "my NAS is
right there". That needs saved things on screen, and saved things on screen need a cap, or someone with twelve buckets
scrolls past their own disks. The pin IS the cap, and the user holds it.

### The four rules

1. **The switcher's Network group holds three things and nothing else**: the hub row, every place connected right now,
   and every pinned place (greyed, hollow dot). A place is pinned on its first successful connect, and Unpin lives in
   the row's context menu. ❌ No "Connect to server…" row in the switcher: adding lives in the hub, the palette, and ⌘K.
   Where the rule is applied, and why the LISTING hides nothing: `../file-explorer/navigation/DETAILS.md` § "The
   three-things rule, and which side enforces it".
2. **Every row in the switcher is a place, and opening one that isn't live brings it to life IN THE PANE, with a
   cancel.** MTP already worked this way; SFTP, WebDAV, SMB, and ADB inherit it (`../file-explorer/pane/DETAILS.md` § "A
   pane on a saved place").
3. **Dialogs are for entering data; panes are for waiting.** The sheet exists to type a new server, edit one, or answer
   a sign-in. Connecting, waiting for a phone's Allow tap, a refusal with a retry, a changed host key, and "signed out"
   all render in the pane. That is what reconciles "a device never gets a modal" with having a connect dialog at all:
   both are right once the two are split.
4. **The sheet opens only on user intent**: activating a row that needs a sign-in, pressing Add, pressing "Sign in…". ❌
   Never on its own when a session drops. A modal stealing focus during a lid-open wake is the wrong thing, and a
   reconnect stays silent (`../file-explorer/network/DETAILS.md` § "SMB live-reconnect flow"); the pane's banner is
   where the button lives.

## The path grammar

An SFTP place's app-facing paths are `sftp://<user>@<host>:<port>/<server path>`; a WebDAV place's are
`webdav://<user>@<host>:<port>/<remote path>`; an S3 place's are the ACCOUNT's,
`s3://<access key id>@<host>:<port>/ <bucket>/<key>`, with a bucket place rooted at `…/<bucket>` and the account root at
`…/`. `server-path-utils.ts` is the frontend's reader and writer; the Rust twin is
`cmdr_fs::volume::remote_paths::RemoteRoot` (translation both ways) over the prefix `sftp_app_root` / `webdav_app_root`
/ `s3_app_root` mints. ❗ The account root's trailing `/` is why containment goes through `isUnderServerRoot`, the twin
of Rust's `cmdr_fs::volume::app_paths::path_is_under` (root trimmed first): a bare `${root}/` prefix refused every
bucket under it.

**Why a scheme and not a hint.** `commands/volumes.rs::resolve_path_to_volume` falls through to the mount table, which
on both platforms walks up to `/` and answers the LOCAL root for any absolute path it doesn't recognize. So a
scheme-free `/srv/data/x` is wrong at every resolver site the app has (a restored tab, a favorite, a drag between panes,
go-to-path, MCP, the trash), and the next site added would forget again. The scheme makes the path self-describing, and
it is the convention `mtp://` and `adb://` already carry.

**The folding has to match Rust's exactly.** The host is lowercased (DNS is case-insensitive), the account is NOT (a
POSIX account may be case-sensitive, and `Ada` and `ada` can be two people), and the port is literal. A path folded
differently misses the volume its own id names, which is the same tuple `sftp_volume_id` hashes.

**The split has to match Rust's too.** `cmdr_fs::volume::ids::server_of_path` reads the account as everything before the
authority's LAST `@` and the port as everything after its last `:`, and `SERVER_PATH_RE` splits at the same two places.
An email login (`webdav://ada@example.com@cloud.example.com:443`, common on Nextcloud and Fastmail) and an IPv6 literal
host (`sftp://ada@::1:22`) both depend on it. ❌ Don't stop at the first `@`: such an account then reads as no place at
all, so a pasted path to it opens the add sheet, an SFTP sign-in gets no secret writer, and the pane's refusal names the
place instead of its host. The account can also hold a typed `user:password`, which is why the log lines on this path
name the host and ❌ never the path.

**Matching is by whole components, never a string prefix.** `/srv/data-1` is a legal sibling of `/srv/data`, and a
string-prefix containment test would strip the root off it and ask the server for `-1/photos`.
`../file-explorer/pane/navigate.ts`'s `isUnderServerRoot` appends the separator for exactly that reason, and
`RemoteRoot::to_remote_path` refuses the same three shapes (a bare server path, another server's prefix, a `..` escape)
on the Rust side.

**The three root aliases** (`''`, `'/'`, `'.'`) all mean the volume root, on both sides. A backend's own code passes a
bare `/` for its root, which is why the Rust side keeps them too.

## The three arms

`connectPlace({ volumeId, connectionState, onAttemptStarted, openSignIn })` picks by standing. Three production callers,
one per arm: `../file-explorer/pane/place-connect.svelte.ts` for a `saved` row, and
`../file-explorer/pane/smb-view-state.svelte.ts` for both the lazy-nav `disconnected` landing and the signed-out
banner's Sign in.

- **`direct`, `os_mount` → nothing** (`already_live`). There is a session serving right now.
- **`disconnected` → `smbReconnectManager.startCycle`**, answering `reconnecting`. The volume is REGISTERED, so a dial
  would register a second one under a second id, and the backoff loop already owns recovery. The manager is idempotent,
  so landing on the same place twice costs nothing. The cycle itself: `../file-explorer/network/DETAILS.md` § "SMB
  live-reconnect flow".
- **`needs_sign_in` → the sheet, as a REGISTERED place.** The backend stopped retrying because a credential is missing,
  so re-dialing can't help, and a re-dial of a registered volume is the second-volume bug again.
- **`saved`, or nothing → `connectSavedPlace`.** Nothing is registered, so this is the first dial. If THAT answers
  `needs_host_key_approval`, `needs_credentials`, or `authentication_rejected`, the place goes to the sheet as an ABSENT
  one, carrying the outcome so the sheet opens on the right step.

❗ **`auth_method_unsupported` never reaches the sheet.** The server challenged with a scheme Cmdr doesn't speak, the
secret never left, and no typing fixes it. A password box over that asks for something that cannot help.

**The sheet opens knowing WHY.** `open-sign-in.ts` reads the refusal off `firstOutcome` through
`server-outcomes.ts::readConnectOutcome` (one reader, so `needs_credentials` and `authentication_rejected` keep their
own sentences) and falls back to the seam request's own `refusal` for arm 2, where a REGISTERED volume sitting in
`needs_sign_in` had no dial to read one off. ❗ Without it a person lands on an empty password box with nothing saying
what happened, which is the exact failure the refusal-under-the-field design exists to prevent.

The `openSignIn` seam takes `{ volumeId, registered, firstOutcome?, refusal? }` and answers
`{ signedIn: true, volumeId } | { signedIn: false }`. `open-sign-in.ts` supplies it; without one the flow refuses with
the reason instead, and ❌ never renders an inert "Sign in…" button.

`cancelled` returns silently in every arm: the user pressed the button and telling them what they just did is noise.

**A `SavedPlaceRefusal` thrown by `connect_saved_place`** (`already_connected`, `no_such_server`) means the `saved` row
the arm was picked from went stale: `volumes-changed` is debounced (150 ms), so another pane's dial can register the
place, or a forget can take it away, between the read and the dial. It crosses the throw as a `SavedPlaceFailure`.
`already_connected` is a MOVE, ❌ never a sentence: `connect-flow.ts` answers `already_live` (the pane reloads onto the
live place), and `open-sign-in.ts`'s dial answers `connected` (the sheet closes onto it). `no_such_server` still reads
as `unreachable`, like an untyped throw (a broken bridge). A forget's row leaves with the next `volumes-changed`, but ❌
a silent cancel would leave the pane blank for good whenever the row outlives the refusal, since the pane dials once per
landing; `apps/desktop/test/e2e-playwright/servers.spec.ts`'s synthetic place is exactly such a row.

Two dials that both pass the registry check can't register one place twice: both mint the same id from
`(host, port, username)`, and `install_retiring_incumbent` retires whichever volume held it
(`apps/desktop/src-tauri/src/network/connect_wiring_test.rs` pins it).

## The sheet contract

**The sheet owns the form; the caller owns the protocol.** `SignInSheet.svelte` renders what a `SignInShape` says to
ask, puts the refusal under the field it is about, and calls a caller-supplied `attempt` as many times as the user
retries. It ❌ never dials. That is what lets SMB's three sites (a share listing, a share mount, a reconnect) reuse it
with their own commands, and what lets S3 plug in with one more renderer.

❗ **It stays open across rounds.** A first connect to a new SFTP server is three round-trips (the host key, then the
credentials, then connected), and a sheet that closed between them would lose what the user typed and put the refusal
somewhere other than under the field it belongs to. `connect-flow.ts` decides WHEN a human is needed; the sheet decides
how many times to ask.

**An attempt promises an outcome, and the sheet holds it to that.** One that throws anyway (a broken IPC bridge) is
logged and reads as `unreachable`, so the sheet never sticks on busy behind a spinner nothing will stop. A secret write
the Keychain refuses crosses the throw as a `KeychainFailure` (`keychain-failure.ts`), whose diagnostic keeps the
variant and the store's own words, ❌ never the secret.

**The three SMB sites, and what each `attempt` runs** (`../file-explorer/network/smb-sign-in.ts` builds all three
requests, so the endpoint header, the remembered username, and the refusal vocabulary can't drift between them):

- **A share listing** (`PlacesBrowser`) → `listSharesWithCredentials`. Server-level, so the header is `smb://<host>` and
  guest is offered where the host allows one. Cancelling goes back to the host list.
- **A share mount** (`../file-explorer/pane/NetworkMountView.svelte`) → `mountNetworkShare`, then `saveSmbCredentials`
  ❗ only once it went through. Cancelling goes back to the share list.
- **A "Connect directly" upgrade** (`../file-explorer/network/direct-connect.ts`) → `upgradeToSmbVolumeWithCredentials`,
  which stores the credential backend-side when the box is checked.

The last two pass `guestAllowed: false`: an unauthenticated attempt is what just came back refused, so offering it again
would be inert. All three answer `handed_off` on success (none of them connects a VOLUME), and hand anything that isn't
a credential refusal back to the pane, which has the words and the retry for it.

❗ **`open-sign-in.ts` is where the standing picks the command**: a REGISTERED volume is mended with
`reconnectVolumeWithCredentials`, an absent one is dialed with `connectSavedPlace`, and a typed server goes through
`connectServer`. SMB's add path has no target at all (its connect is a share MOUNT), so it answers `handed_off` and the
caller opens the host's places list.

**Remember, and who decides where it starts.** `add`: on, because someone typing a password into a new server means to
come back to it. `edit`: from `hasServerSecret`. `sign-in`: from the request's `remembered`, which the ❗ OPENER
decides, ❌ never the sheet: whether the answer is worth what it costs is the protocol's business. SFTP, WebDAV, and S3
ask `hasServerSecret`, because an attended sign-in REFRESHES a remembered secret and ❌ never seeds one, so a default-on
box there would seed one the user already declined; the sheet is a moment a person is already waiting through, so the
read is affordable there. SMB passes `true` unasked, because nothing is written until a sign-in works and its caller
writes it, so a checked box promises nothing that hasn't been shown.

❗ **The read is not cheaper for SFTP, and ❌ don't reason as if it were.** `has_sftp_credentials` is
`network::keychain::has_credentials`, which is literally `get_credentials(server, share).is_ok()` — the same call
`has_server_secret` makes for an SMB host. So the cost argument is about WHEN it is worth paying, ❌ never about which
protocol. On a right-click it is not worth paying: `../file-explorer/navigation/server-row-actions.ts` offers "Forget
saved password" on every server row and lets `forgetServerSecret`'s own answer word the empty case, rather than reading
the Keychain to decide whether to draw a menu item.

**A flip is WRITTEN, before the round it belongs to.** `open-sign-in.ts`'s `withRememberFlip` wraps whichever attempt
the standing picked, compares the box against what `hasServerSecret` answered, and writes once per flip: OFF →
`forgetServerSecret` NOW, because `refresh_remembered_secret` writes wherever the store already holds something, so a
mend over a live entry would put the just-declined password straight back; ON over an empty store →
`saveSftpCredentials` / `saveWebdavCredentials` / `saveS3Credentials` NOW, because an attended sign-in refreshes a
remembered secret and never seeds one, so a box flipped on with nothing written would promise a thing that never
happens. ❌ Neither ever happens as a side effect of a dial. ❗ A save the Keychain refuses (Deny on the prompt, a
locked keychain, no secret service) answers `secret_not_stored` under the password field and runs no round: nothing was
filed, so the local reading stays where it was and the next press writes again.

❗ **The writer takes the whole tuple the volume id is minted from** (`(host, port, username)` for SFTP, the base URL
and the account for WebDAV), read off the place's `appRoot` rather than rebuilt from a host plus a default port: an
entry written under a different key is one the dial never finds, and the box would be lying in the other direction. A
place no saved server claims has no key to write under, so it has no writer. S3's key is the ACCOUNT's (the provider
choice plus the access key id, shared by every bucket under the key), which the listing doesn't carry: the writer reads
the saved place (`knownS3PlaceOf`, matched by the `volumeId` the backend publishes, ❌ never a frontend hash).

In EDIT mode there is nothing typed to save, so the box only ever forgets (`SignInSheet.svelte`'s `writeRememberFlip`);
turning it on there rides the next successful sign-in's offer.

**An S3 edit is the ACCOUNT's or a PLACE's** (`SignInSheet.svelte`'s `s3EditScope`). ❗ The account carries the name and
the secret; a bucket reads as its own name and keeps only its "Reconnect automatically" switch
(`src-tauri/src/network/DETAILS.md` § "The S3 twin, a place per entry").

- **The account** (Edit on the account's hub row: `openEditServerSheet(server)`, no `placeVolumeId`): the Name field,
  the provider and the key locked (`servers.sheet.identityLockedS3Account`), ❌ no Bucket field and ❌ no Advanced. It
  reads the store and the Keychain through one of the account's places (`storeId`: they share the secret, and each
  `SavedS3Place` carries the account's raw name), and Save renames through `updateSavedS3Account` (blank unnames it), ❌
  never `updateSavedServer`, whose bucket-less target would save the account ROOT as a new place. ❗ On "Other
  S3-compatible" the endpoint, region, and path-style switch stay editable (`S3EndpointFields`' `endpointEditable`, with
  `servers.sheet.s3EndpointMoveHelp`), and Save sends the provider beside the name: a new endpoint MOVES the account and
  every bucket under its key, with the shared secret (`apps/desktop/src-tauri/src/server_move.rs`). A preset's field
  stays locked and sends nothing: another region or account ID is other storage, not a new road to the same one. A move
  leaves the account under a new id, which the sheet reads back (`accountSavedAs`) for any Save again.
- **A place** (Edit on a bucket or root row, the switcher's menu included:
  `openEditServerSheet(server, placeVolumeId)`): the provider, the key, and the bucket locked
  (`servers.sheet.identityLockedS3`), ❌ no Name field, Advanced with the place's own switch. Save sends the target with
  a blank name, which leaves the account's name alone. The title is the place's listing name.

A typed secret is the account's either way (`saveS3Credentials`).

**Edit mode MOVES an address, ❌ never an account.** The protocol toggle and the username are locked, and
`servers.sheet.accountLocked` sits under the username saying to Add instead: another account is another place, since two
accounts on one server see different files. An SFTP or WebDAV address stays editable (`addressEditable`, with
`servers.sheet.addressMoveHelp` under it), as does an "Other S3-compatible" account's endpoint (§ "An S3 edit is the
ACCOUNT's or a PLACE's"), because a server that moved (a NAS's new IP, `nas.local` → its Tailscale name) must keep its
favorites, tabs, pin, and password. ❗ The sheet never decides whether an edit is a move: Save passes the id it opened
on (`updateSavedServer(target, editing)`), and the backend saves in place or moves the server
(`apps/desktop/src-tauri/src/server_move.rs`), refusing an address another saved server holds (`address_taken`, under
the address, naming that server). A save that moved it leaves the place under a NEW id, which the sheet reads back
(`savedServerId`) for the Remember flip and any Save again (`savedAs`). Typing an address in edit mode never steers the
username or the root, as it does in add mode. An SMB host's address stays locked (`servers.sheet.addressLocked`): its
share ids come off the mount (`docs/notes/server-address-move.md` § "SMB, deferred"). ❗ This is `ServerFormFields`' own
rule and says nothing about sign-in mode, where username editability is the SHAPE VARIANT's property (§ "The renderer
table").

**Edit mode opens on the first field it lets a person change** (`focusFirstEditableField`), which for an SFTP or WebDAV
server and an "Other S3-compatible" account is the address. Decision (David, 2026-10-11): keep it there, since with the
address unlocked, fixing a server that moved is the likely reason to open Edit. Where the address is locked (an SMB
host, an S3 preset), focus falls through to the name. ❌ Never `addressInput.focus()`: focusing a disabled field is a
silent no-op, and the sheet then opened with nothing taking keys (QA 2026-09-25).

**Edit mode's password field writes what it shows.** A non-empty value on Save goes through `saveSftpCredentials` /
`saveWebdavCredentials` keyed on the target's tuple, and the Remember box then reports on, because the store holds one.
An EMPTY field means "I didn't come here to change the password", ❌ never "store an empty one": the field opens empty
every time, since a stored secret is never read back out of the Keychain to prefill it. ❗ So over a stored secret
(`hasServerSecret` at open) it carries `servers.sheet.secretKeptPlaceholder` ("Saved in Keychain. Leave empty to keep
it."), dropped while Remember is off, since Save then forgets it: a bare empty box read as "no password saved". One
field serves SFTP, WebDAV, and S3 (`ServerFormFields`); an SMB host's edit has no password field. The typed password is
written LAST, after the Remember flip, so it wins over a box the same visit turned off: a password field with text in it
and Save pressed stores that password. A flip or a write that breaks down after the edit saved answers
`saved_secret_not_updated` under the password field, ❌ never the dial's `unreachable`: the edit landed and no server
was contacted, and Save again re-saves the same edit and retries the write.

**Edit mode's name field holds what the user TYPED.** The store row's raw name opens it, empty for a server nobody named
(`nameSource: 'fallback'`), with the same "Leave empty to use …" placeholder add mode shows; the sheet's title uses the
listing's label. An SMB host's edit is a RENAME and nothing else (`update_saved_smb_host`, reached from the host row's
native menu, "Edit server…"): the address is its identity and stays locked, and there is no secret to write. Naming a
host only the share history knew saves it as a manual entry (`network/manual_servers.rs` § `name_server_entry_at_path`).
`serverTargetFrom` sends an empty name as empty and ❌ never falls back to the address. ❗ A name that repeated the
typed address left it as the sheet's only URL-shaped field, and a root got "widened" through the name
(`apps/desktop/src-tauri/src/network/DETAILS.md` § "An unnamed server's label, and names that only repeat the address").

**The root folder is a ceiling, the start folder is a landing.** Both sit in Advanced, which edit mode opens because
most of the settings someone came to change live there (the name sits above, under the address). The start folder must
be the root or under it, by whole components: `server-form.ts::isStartFolderUnderRoot` mirrors the backend's
`start_folder_under_root` (`.` and `..` resolved, a relative path read from `/`), so the sentence arrives before a
round-trip, once the field has lost focus and again on Save or Connect. The backend stays authoritative. A pasted
address's path fills the root folder.

**Saving answers a typed outcome** (`server-outcomes.ts::readSavedServerOutcome`). `saved` writes the Remember flip and
the typed password, then closes. ❗ Every refusal writes NOTHING, the password included, keeps the sheet open, and puts
its sentence under its own field with the caret there, opening Advanced first. The backend's `unreachable` (a connected
server that didn't confirm a folder within 5 s, or a server a Forget elsewhere took away meanwhile) reads as
`save_unconfirmed`, ❌ never the dial's `unreachable`: nothing was saved, which is the half of the story that sentence
has to tell. A saved edit republishes the volume list, which is how the hub and the switcher learn it
(`apps/desktop/src-tauri/src/commands/DETAILS.md` § `servers.rs`). Where panes go when a connected place's root moved:
`../file-explorer/pane/DETAILS.md` § "A place whose root moved under the pane".

**Edit mode's "this can't reconnect on its own" warning is the BACKEND's answer, ❌ never a derivation.**
`getSftpUnattendedReconnect` / `getWebdavUnattendedReconnect` / `getS3UnattendedReconnect` say whether an unattended
reconnect can work as things stand, and the sheet asks when it RENDERS. ❌ Don't rebuild it from "auto-reconnect is on
AND no secret is stored": the rung a remote volume comes back on is decided per dial, so a derivation goes stale the
moment one lands elsewhere, and the two backends spell the same answer differently (`needs_stored_secret` vs
`no_stored_secret`). The state worth warning about is the silent one: auto-reconnect on, nothing stored, so nothing can
ever happen.

**"Reconnect automatically" carries an `InfoTip` saying what it does NOT do** (`servers.sheet.autoReconnectHelp`). The
switch only redials a session that DROPPED (`crates/cmdr-sftp/DETAILS.md` § "The two switches"); a saved place is dialed
when a pane lands on it (`../file-explorer/pane/place-connect.svelte.ts`), and Cmdr holds no session for a server nobody
opened. The bare label reads as "connect at startup", which is the misreading the tip corrects. It sits BESIDE the
checkbox, ❌ never inside its `<label>`, where a click on the glyph would flip the box. A saved place's row menu carries
the same switch, with the same two keys as its label and tooltip, so the two doors can't drift in meaning
(`../file-explorer/navigation/DETAILS.md` § "Row actions: the → submenu").

**The host-key step replaces the body, ❌ never a second dialog.** First contact is routine and gets a plain primary
button; a changed key is the shape a man-in-the-middle takes, so it carries the red weight, says what else it can mean,
and puts its trust button behind a disclosure. A `superseded` approval starts the step over on the key the server REALLY
presents rather than silently trusting the one on screen; an `unreachable` one records nothing, because approving is a
live question and an unanswered one is not a yes.

❗ **A changed host key on a REGISTERED volume has no fingerprint to show.** No backend command hands the PENDING
host-key prompt back for one, so the banner's "Check the key" drops the session and redials, and the sheet shows that
dial's prompt (`../file-explorer/pane/DETAILS.md` § "The connect views").

## The renderer table

One renderer per `SignInShape` variant, and ❗ **username editability is the VARIANT's property, ❌ never the sheet's
mode**. One implementer reading "read-only" as a mode rule breaks SMB; one reading "editable" as a mode rule breaks
SFTP.

- `nothing`: the sheet never opens. There is no secret a person could type that would help, and
  `reconnect_with_credentials` answers `NotSupported` for a key-only or agent-only server every time. ❗ The guard is at
  the SEAM (`open-sign-in.ts` answers `{ signedIn: false }` without opening anything), so a new caller inherits it
  rather than having to remember it; `smb-view-state.svelte.ts`'s own reading of the shape is a separate decision, about
  whether the `signed_out` banner offers a BUTTON at all.
- `password`: the account as a read-only header, one password field. SFTP's and WebDAV's `reconnect_with_credentials`
  refuse a changed username, because the volume id IS the account.
- `key_passphrase`: the same, with the field labelled for a key file's passphrase and `autocomplete="off"`, because a
  passphrase is not the account's password and autofill must not offer one.
- `access_keys` (S3): the access key ID as the read-only header (labelled "Access key ID"), one "Secret access key"
  field with `autocomplete="off"`. S3's `reconnect_with_credentials` refuses another key id: another key is another
  account. No session token: temporary credentials are out of scope (`docs/specs/s3-support-plan.md`).
- `username_password { guestAllowed }`: username editable, plus a guest `RadioGroup` where the share allows one. SMB's
  reconnect accepts a new username and rewrites its params, which is how re-auth-as-someone-else works. The field
  carries the "Example: barry" placeholder, so an empty box says what kind of thing goes in it; the remembered username
  (`../file-explorer/network/smb-sign-in.ts`) fills the VALUE when there is one, and the placeholder steps aside.

**Reserved, ❌ not added until a producer exists** (`crates/cmdr-fs/src/volume/connection.rs` carries the same list):
`oauth { provider }` (a "Continue in your browser" button and a waiting state; "remember" is implicit, since the refresh
token is the only sane state, and a revoked token surfaces as `needs_sign_in` behind the same banner).

## The refusal table

- `authentication_rejected`: the credential was offered and refused. Names the account.
- `needs_credentials`: nothing was ever offered. ❌ Not a rejection. ❗ A sheet that OPENS on it opens clean
  (`refusalShownOnOpen`): it's only the reason the sheet is asking, which the title already says, and red is feedback on
  something the person did. The sentence shows only after a round the person sent comes back with it (cmdr-reports#10).
- `account_not_permitted`: the account signed in, and the PLACE turned it away (an SMB share whose TreeConnect answers
  access denied). Names the account and asks for a different one. ❗ Goes under `form`, ❌ never `secret`: the password
  worked, and the `secret` slot marks its field invalid. Produced by SMB's mount (`permission_denied`) and by "Connect
  directly" (`accountNotPermitted`).
- `auth_method_unsupported`: the server challenged with a scheme Cmdr doesn't speak, so the secret never left. ❌ Never
  name the scheme; "Digest" means nothing to the reader.
- `certificate_untrusted`: macOS doesn't trust the certificate, and the fix is Keychain Access. Trust-on-first-use is
  backend work (GitHub [#173](https://github.com/vdavid/cmdr/issues/173)).
- `not_a_webdav_server`: the address answers HTTP but not WebDAV. The one refusal with a remedy button.
- `invalid_url`: the saved address isn't a usable web address.
- `timed_out` and `unreachable`: about the SERVER, so they name the host rather than the account. An `unreachable` from
  the Add probe may carry a `RefusalHint` (`local_network_permission`: this Mac refused the route to a LAN address,
  which is also how a stuck Local Network permission shows, ERR-XGS9X). The sheet renders it as a softer second line
  (`#server-address-hint`, `wordRefusalHint`) under the same sentence, only while the refusal it came with is on screen,
  and Add anyway stays: a server that's off looks the same to the probe. Backend rule:
  `src-tauri/src/network/DETAILS.md` § "This Mac refusing the route".
- `host_key_untrusted` (from `needs_host_key_approval`): the sheet's key step is where the fingerprint is shown and
  approved.
- `host_key_revoked`: deliberately final. No button can safely undo a revocation the user's own `known_hosts` records.
- `start_folder_outside_root`: the start folder is neither the root folder nor inside it. A connect and a save both
  refuse it, and the sheet says it before either.
- `root_not_found` and `start_folder_not_found`: a save to a connected place whose server can't open that folder for
  this account (missing, a file, or refused). They name the host.
- `save_unconfirmed`: a save to a connected place that the server didn't confirm in time. Names the host and says
  nothing was saved.
- `secret_not_stored`: sign-in mode, Remember went on and the Keychain wouldn't store the password, so no round ran.
  Says how to sign in without storing it. ❌ Not `authentication_rejected`: no server was asked anything.
- `saved_secret_not_updated`: edit mode, the settings saved and the password write didn't. Says the changes are saved,
  so nobody re-saves an edit that landed.
- `address_taken`: edit mode, the new address is another saved server's. Under `address`, naming that server
  (`RefusalSubject.takenBy`), and nothing was saved: a merge would silently drop one server's settings and password.
- `secret_not_moved`: edit mode, a move couldn't take the saved password along, so NOTHING moved. Under `secret`. ❌ Not
  `saved_secret_not_updated`, whose edit landed.
- `account_changed`: edit mode, the save named another account or protocol. The sheet locks both, so only a broken
  caller meets it. Under `form`.
- `operation_running`: edit mode, a move while a copy, move, or delete on the server is queued, running, or paused.
  Nothing was saved: the move drops the old session, which would cut that work off. Under `form`, since no field fixes
  it; letting the work finish or canceling it does.
- `access_denied` (S3): the bucket refused the key, which a bodyless 403 can't split into a wrong secret and a key with
  no rights here (Garage answers a wrong secret this way too), so it asks about both. Under `secret`, and it opens the
  sheet (`needsAHuman`), since a wrong secret is one thing it means.
- `bucket_list_refused` (S3): the account root needs `ListBuckets` and this key may not (on some servers, a wrong secret
  answers the same). Under `bucket`: typing a bucket is the way in. Stays in the pane, ❌ no sheet: no secret typed
  there creates a bucket place.
- `bucket_not_found` (S3): under `bucket`, naming the endpoint host.
- `region_mismatch` (S3): under `region`, naming the bucket's region when the server did (the refused outcome's `region`
  rides through `connect-flow` and the sheet to `RefusalSubject.region`). Add mode then offers "Use <region>", which
  switches the field and retries; R2 and Hetzner have no region field, so they get the sentence only.
- `clock_skewed` (S3): this Mac's clock is more than 15 minutes off. Under `form`: no field fixes it.
- `not_an_s3_endpoint` (S3): under `address`, which for S3 is the field that makes the endpoint.
- `s3_field_malformed` and `endpoint_malformed`: add mode's own checks before any round trip
  (`s3-form.ts::s3FieldProblem`, the mirror of the backend's `invalid_url` for S3): a region, location, or account ID
  outside `a–z 0–9 -`, and an Other endpoint that isn't `http(s)://host[:port]`.
- **An S3 subject says "secret access key" where the others say "password"**: `RefusalSubject.protocol === 's3'` swaps
  the five password sentences for their `servers.refusal.s3*` twins (`S3_REFUSAL_KEYS`), since an S3 account has no
  password.

**Whose name a refusal says.** In sign-in mode the sentence names the account the refused ROUND sent
(`SignInSheet.svelte`'s `roundUsername`), and only the refusal the sheet opened with names `endpoint.username`. Where
the shape lets the username be edited, the account the sheet opened with may not be the one that was turned away, and
"ada doesn't have access here" after `bob` was refused blames an account nobody tried. Add mode has the same rule for
the host: an edit retires the refusal, since its sentence reads the live form.

**A pane names a saved place by the name the user gave it.** `place-connect.svelte.ts` words its refusal through
`wordPaneRefusal`, which says `unreachable` as `servers.paneState.unreachable` ("Cmdr couldn't reach Naspolya."),
falling back to the host when the name is empty. Every other kind reads as in the sheet. The sheet and the Add form keep
`servers.refusal.unreachable` with the host, since there the address is what the person typed and can fix.

Keys live in `$lib/intl/messages/en/servers.json` under `servers.refusal.*`, reached through a `Record` in
`connect-refusals.ts` rather than a built string, which is what keeps `desktop-message-keys-unused` honest without a
dynamic-prefix entry. `$lib/error-messages/friendly-error-style.test.ts` renders all of them and holds them to the same
writing rules the friendly-error copy obeys: they are error copy however they are filed.

**A second `Record` says WHICH FIELD each sentence goes under** (`refusalField`): the secret for the ones about a
credential or storing it, the address for the ones about the endpoint, the root folder or the start folder for the ones
about a folder, S3's `region` and `bucket` for theirs, and the form for the ones no field can fix. For S3, whose form
has no address, `address` is the field that makes the endpoint: Other's URL, else the preset's own field, which is also
where a preset's `region` sentence lands (`S3EndpointFields.svelte`). ❗ A refusal floating above a form reads as being
about the whole form: "That password didn't work" under the password field is an instruction, and the same words above
the address are a puzzle. ❗ Sign-in mode renders only the password field, so there every refusal that isn't `secret`
reads in the form slot, ❌ never under a field that isn't on screen: a sign-in round refused as `unreachable` once
showed no word at all.

## Add mode, protocol first

The form reads top to bottom: the SMB / SFTP / WebDAV / S3 toggle, the address (S3: § "S3 in add mode"), the name, then
the protocol's own fields and Advanced. ❗ **Only the person moves the toggle, and it alone decides what gets dialed**
(`serverTargetFrom` for SFTP, WebDAV, and S3, `smbAddressFrom` for SMB's hand-off). Typing into the address used to flip
it: `sven@192.168.0.153`, typed for an SMB NAS, read as SFTP and dialed SSH on port 22 without anyone clicking SFTP, and
the host key the Mac already trusted turned that into a quiet SSH session the user objected to (cmdr-reports#8).

What the address says still earns a sentence: `addressLooksLike` answers which protocol it looks like when that isn't
the selected one, and the sheet shows it under the field (`servers.sheet.addressLooksLike*`, `role="status"`). Two
things count as evidence, and nothing else: a scheme or a pasted `ssh` line (it wins over any port it names), and a
well-known port on an address with no scheme (22, 139, 445, 80, 443). ❌ An account, a path, or a bare host is not
evidence: `user@host` fits all three protocols. The warning never blocks Connect.

A port or path the address's scheme named for a DIFFERENT protocol is dropped (`sftp://nas:2222/srv` with WebDAV
selected dials `https://nas`), while one typed with no scheme belongs to whichever protocol is selected. The account the
address carried fills the username either way; the path fills the SFTP root only when SFTP is selected, and the sheet
re-applies the address when the toggle moves, so typing first and picking second lands in the same place.

The username the address filled follows the address (`usernameAfter` in `server-form.ts`): it changes with the address,
and goes once the address names nobody or stops parsing. One the person typed is stashed (`typedUsername`) while the
address fills the field, and comes back when it stops. Besides `smb://`, SMB reads Windows' `\\nas\share` and the macOS
mount table's `//nas/share`; `smbAddressFrom` hands both to the backend as `smb://`.

❗ **A prefill is the one place a scheme sets the toggle** (`formFromPrefill`). Go to path and ⌘K hand over a whole URL
the person asked to open, so its scheme is already their choice, and the sheet opens with the toggle in view before
anything is dialed. Go to path only hands over addresses WITH a scheme, so a bare `user@host` never takes this path.

❗ **SMB takes an optional username, and nothing else of the account.** It fills from a `user@host` address and stays
editable in edit mode, because for SMB the account is a preference, not the identity (the address is). It travels as
`add_smb.username` (`null` when empty, ❌ never `''`) and does two things backend-side: it prefills the first sign-in,
and the share listing stops answering as guest (`src-tauri/src/network/DETAILS.md` § "Guest-first auth flow"). The add
also drops the frontend's cached share lists for that machine (`forgetShareListsOfMachine`), since one fetched before
the add may be the guest list the account now refuses. No password here: the listing or the mount asks.

❗ **SMB's hand-off spells a scheme-less address as `smb://…`.** `connect_to_server`'s bare-host reader refuses an `@`
or a `/`, so `sven@192.168.0.153` has to travel as the SMB URL it means; its URL reader drops the account. An address
naming another scheme keeps only its host.

❗ **The name sits under the address, for every protocol.** It isn't an advanced setting. Its placeholder says what an
empty one falls back to ("Leave empty to use 192.168.0.153"), through `server-form.ts::nameFallbackOf`, which mirrors
the backend's stand-in labels: an SMB host's address (plus a port that isn't 445), an account's `username@host`. An SMB
address already saved under a name someone gave it reads as that name ("Leave empty to use My NAS"), since Go to path
and ⌘K open this sheet on any `smb://`; the sheet fetches the saved list on open for it. The account it was saved with
fills Username the same way (`withSavedAccount`), as an address-supplied one, so it goes if the address moves on. SMB's
add carries the name in `add_smb` to `connect_to_server`; SMB still shows no Advanced disclosure, since it keeps no
folders.

❗ **Two honest buttons, and both check before saving** (cmdr-reports#6: the sheet said "Add server" and its one button
connected). The title stays "Add server". The primary, and Enter, is **"Add and open"**; beside it is **"Add"**. Each
submission carries an `AddIntent`, and `open-sign-in.ts::attemptAdd` runs the same check for both: a TCP probe for SMB
(`connect_to_server`), the real connect for SFTP and WebDAV, because a typo saved silently is found only later,
somewhere else. Only "Add and open" moves a pane:

- **"Add and open" lands a pane on the new place**, through `openAddServerSheet`'s `onConnected` (the place's volume id
  and app root, read off `list_saved_servers` rather than the volume store, which can still hold the previous
  `volumes-changed`). ⌘K and Go to path land the focused pane; the hub lands its own. SMB's add is a share mount, so it
  hands off instead (`onSmbHandOff`): the saved host's places list, mounting the share the address named. ⌘K and Go to
  path both go through `ExplorerAPI.openSmbHandOffInFocusedPane` (one destination, ❌ never a bare Servers list); the
  hub does the same to its own pane.
- **"Add" closes as `added`** with the server's saved id, and `onAdded` shows it: the hub selects the new row
  (`ServersHub.selectServer`, which waits for the row to be listed), and a door with no list on screen raises a toast
  saying where it went. ❗ "Add" only saves: an SFTP or WebDAV "Add" drops the session its check opened
  (`disconnectPlace`), so the row reads Saved, and a password Remember filed stays filed. A place that was already live
  before the Add keeps its session, since a pane may be standing on it.
- **"Add anyway"** appears under the address only after `unreachable` or `timed_out`, beside a line saying nothing was
  checked. It saves as typed: `connect_to_server` with `check_reachability: false` for SMB, `update_saved_server` for
  SFTP and WebDAV (plus the typed password when Remember is on). ❌ Never for `invalid_url`: `AddServerError` keeps an
  address that doesn't parse apart from a server that didn't answer, and only the second can be added anyway. ❌ Never
  the default.

❗ **SMB is the default**, the one that costs nothing when it's wrong: SMB browses with no account, so it asks the user
for nothing, while defaulting to SFTP would put an account field in front of someone who typed a NAS name off a sticker.

❗ **A Nextcloud URL stays whole, path and all.** Nobody can tell where the base URL ends and the collection begins, and
the backend resolves the remote root relative to the base anyway. `not_a_webdav_server` is the one refusal with a remedy
button, because the fix is a collection path nobody knows; `certificate_untrusted` deliberately has none, since trusting
a certificate happens in Keychain Access.

There is no property-testing library on the frontend, so `address-parser.test.ts`'s example table IS the contract: a
shape that reaches the field and isn't in it is a shape nobody decided.

### S3 in add mode

❗ **S3 has no address: the preset decides the endpoint.** With S3 selected, `S3EndpointFields.svelte` stands where the
address was: a Provider select (Amazon S3, Cloudflare R2, Backblaze B2, Wasabi, Hetzner Object Storage, Other
S3-compatible), then the one field that preset takes (AWS, B2, and Wasabi a region; R2 an account ID; Hetzner a location
picked from fsn1 / nbg1 / hel1; Other an endpoint, an optional region, and "Use path-style addressing", on by default),
then an optional Bucket. Its model is `s3-form.ts`, held under `ServerForm.s3`; `serverTargetFrom` builds the
`S3ProviderChoice` from it, each preset carrying only its own field.

- **Empty bucket = the account root**, which lists every bucket the key may see. A key that can't list buckets (one
  scoped to a single bucket, common on R2) answers `bucket_list_refused` under the Bucket field, and typing a bucket is
  the way in. ❗ So the field is optional only for an account-wide key, which is why it carries ❌ no "Optional"
  placeholder: the help line says both cases, in every preset, and stays beside a refusal (hidden in edit mode, where
  the bucket is locked).
- **The access key ID is `username` and the secret access key is `secret`**, relabelled, both `autocomplete="off"`. So
  the identity lock, Remember, and the Keychain plumbing are the ones every account uses, and a username an address
  filled can't leak in: `applyParsedAddress` fills nothing while S3 is selected.
- **The order is endpoint, then name, then credentials**, like the other protocols: Provider, its field, Bucket, Name
  (the ACCOUNT's name, so its placeholder mirrors the backend's `<key>@<host>` stand-in through `s3HostOf`, whatever the
  bucket; a typed name renames the account, a blank one leaves an already-named account alone), Access key ID, Secret
  access key, Remember, and Advanced holding only "Reconnect automatically" (no root or start folder: the bucket is the
  place).
- **Checked before any round trip**: `s3FieldProblem` (§ "The refusal table"). Submit stays disabled until the key and
  the preset's own field are typed.
- **A pasted `s3://` app path is a prefill** (`s3FieldsFromAppPath`): Go to path on an S3 place nothing has saved opens
  the sheet on S3 with the key, the preset its host names (else Other on `https://host[:port]`), and the bucket filled.
- **Add anyway** saves through `updateSavedServer`, plus `saveS3Credentials` (the ACCOUNT's secret, shared by every
  bucket under the key) when Remember is on.

## Saved SMB shares

A share Cmdr mounts is saved as a place under its host, with the account it signed in as (cmdr-reports#7): a row under
the host in the hub, pinnable like an SFTP place, and brought back to life in the pane through the same `connect-flow`
arm 3. Model, store, writers, and what Forget does: `docs/specs/saved-smb-shares.md`. What differs from an SFTP place on
this side:

- **The sign-in sheet asks for the account too** (`SignInShape::UsernamePassword`, no guest), prefilled with the account
  the share was saved with: for SMB the share is the place and the account a field on it. The attempt sends the typed
  username beside the secret (`connectSavedPlace`'s `username`), and the row remembers the new account on success.
- **`remembered` starts on and the Keychain is ❌ never probed** (`open-sign-in.ts`), SMB's rule everywhere; the backend
  writes the password only once the mount went through, so there is no secret writer here.
- **A share's next mount may land elsewhere** (`/Volumes/naspi-1`), so `place-connect` asks `placeRootOf` for its
  landing after a connect and the pane enters the share there; after that it follows the live row's mount path (the
  backend's `remember_mount` rewrites the saved row's path on every Cmdr mount).
- **The header names the share on its host** (`smb://192.168.0.153/Container`), since that is the place.

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

- **An SMB host row stops the search** rather than falling through to reading 2. Its places are its saved shares, each a
  row of its own, so the host itself has nothing for `disconnectPlace` or `setPlacePinned` to act on, and quietly acting
  on the pane's volume instead would move a server the user isn't pointing at. A saved SHARE row has a `volumeId`, so it
  wins like a one-place row: the pin moves, and Disconnect answers `false` (a share's session is a mount, which
  `disconnectPlace` doesn't speak; Eject in the switcher is its detach).
- **A target resolved from the pane's volume reports `pinned: null`**, because a `VolumeInfo` carries no pin (the pin is
  the switcher's cap, decided in Rust, and deliberately off the wire). `servers.togglePin` reads `listSavedServers()`
  for that case; a store that doesn't answer reads as unpinned, which makes the command a pin rather than a no-op.

**Edit and Rename on the Servers volume are "Edit server…".** `file.edit` and `file.rename` (F4, F2 / ⇧F6 by default,
read through the shortcut system like every command) and `servers.edit` hand
`../file-explorer/network/servers-hub-actions.ts::editServerInView` what the pane shows: the hub row under the cursor
(`ExplorerAPI.getFocusedPaneHubRow()`, an SMB host's included, since the `host` arm of `NetworkCursorEntry` carries its
row), else the host whose share list is up (`getFocusedPaneNetworkHost()`), matched to its saved server by the hub's own
merge. Off that volume they keep their file-list meaning. ❗ Every case answers: a row with nothing saved to edit (a
share, a host only mDNS sees) and "Add server…" under the cursor each get a one-line info toast, ❌ never silence. That
is also why the F-key bar's F2 button is enabled there (`FunctionKeyBar.svelte`'s `canRename`).

The handlers themselves (`src/routes/(main)/command-handlers/servers-handlers.ts`) then route through
`../file-explorer/navigation/server-row-actions.ts::runServerRowAction`, the same function the native menu's answer
lands in, so a menu item and a palette command can't drift on a confirmation or a toast.

## Deliberately not built

Each of these was decided against with a reason, and a reason nobody can find gets re-derived. The pointer is to
whichever doc owns the item now; ❌ nothing here restates a mechanism.

- **`~/.ssh/config` host aliases as address-field completions**: GitHub
  [#194](https://github.com/vdavid/cmdr/issues/194). It is a backend parser with its own edge cases, and the add form is
  usable without it.
- **One switcher row per phone** rather than one per protocol: GitHub [#192](https://github.com/vdavid/cmdr/issues/192).
  The "(ADB)" name suffix is the stopgap.
- **Certificate trust-on-first-use**, which is why a self-signed NAS lands on the honest `certificate_untrusted` wording
  with no button that could work: GitHub [#173](https://github.com/vdavid/cmdr/issues/173), backend work.
- **A property-testing library on the frontend.** `proptest` stays Rust-only, and `address-parser.test.ts`'s example
  table is the contract instead (§ "Add mode, protocol first").
- **A separate pane tint per server protocol.** `appearance.tintSmb` covers all four ("Tint server panes (SMB, SFTP,
  WebDAV, S3)"). A separate setting would be three definition sites, a section row, and two parity tests for a color
  nobody asked to set apart.
- **OAuth**, whose contract shaped types that shipped and so is written beside those types: one more renderer (§ "The
  renderer table") and the reserved `SignInShape` variant (`crates/cmdr-fs/src/volume/connection.rs`).
