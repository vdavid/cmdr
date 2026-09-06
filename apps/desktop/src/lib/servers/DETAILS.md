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
- **`needs_sign_in` → the `openSignIn` seam**, else refuse `needs_credentials`. The backend stopped retrying because a
  credential is missing, so re-dialing can't help — and a re-dial of a registered volume is the second-volume bug again.
- **`saved`, or nothing → `connectSavedPlace`.** Nothing is registered, so this is the first dial.

The `openSignIn` seam is M2's: it hands the sheet a volume id and awaits
`{ signedIn: true, volumeId } | { signedIn: false }`. Until M2 supplies one, the arm refuses with `needs_credentials`,
which is the honest thing to show — the server is asking for something and nothing here can collect it yet. ❌ An inert
"Sign in…" button would be worse than the sentence.

`cancelled` returns silently in every arm: the user pressed the button and telling them what they just did is noise.

**A `SavedPlaceRefusal` thrown by `connect_saved_place`** (`no_such_server`, `already_connected`) means the CALLER
picked the wrong arm. It is logged and shown as `unreachable`, because there is no sentence for it that helps a person:
a user should never see one, and the log line is what a maintainer needs.

## The refusal table

- `authentication_rejected`: the credential was offered and refused. Names the account.
- `needs_credentials`: nothing was ever offered. ❌ Not a rejection.
- `auth_method_unsupported`: the server challenged with a scheme Cmdr doesn't speak, so the secret never left. ❌ Never
  name the scheme; "Digest" means nothing to the reader.
- `certificate_untrusted`: macOS doesn't trust the certificate, and the fix is Keychain Access. Trust-on-first-use is
  backend work (`docs/specs/webdav-backend-follow-ups.md` § 2).
- `not_a_webdav_server`: the address answers HTTP but not WebDAV. M2 adds the "Try the Nextcloud address" remedy.
- `invalid_url`: the saved address isn't a usable web address.
- `timed_out` and `unreachable`: about the SERVER, so they name the host rather than the account.
- `host_key_untrusted` (from `needs_host_key_approval`): M1 states it; M2's sheet is where the fingerprint is shown and
  approved.
- `host_key_revoked`: deliberately final. No button can safely undo a revocation the user's own `known_hosts` records.

Keys live in `messages/en/servers.json` under `servers.refusal.*`, reached through a `Record` in `connect-refusals.ts`
rather than a built string, which is what keeps `desktop-message-keys-unused` honest without a dynamic-prefix entry.

## What M2 fills in

- `SignInSheet.svelte` and `sign-in-sheet-state.svelte.ts`, and with them the `openSignIn` seam above.
- `address-parser.ts` for add mode.
- `RemoteConnectView`'s remaining states (`waiting_for_device`, `signed_out`, `host_key_changed`, `gave_up`), each
  landing with the milestone that can act on it. ❗ A state added before its handler puts a button on screen that does
  nothing.
- The go-to-path scheme intercept, ⌘K, and the palette entries.
