# Servers

Everything the app does with a remote place it can dial: the path spelling, the connect flow, and the words for a
connect that stopped. Up: `../../CLAUDE.md`. Backend contracts: `crates/cmdr-sftp/DETAILS.md`,
`crates/cmdr-webdav/DETAILS.md`, `apps/desktop/src-tauri/src/server_volumes.rs`.

## Module map

- `server-path-utils.ts`: reads and writes `<protocol>://<user>@<host>:<port>/<server path>`, plus `isServerPath` and
  `isServerVolumeId`.
- `connect-flow.ts`: `connectPlace` picks the move by the volume's standing, and `cancelPlaceConnect` calls it off.
- `connect-refusals.ts`: one sentence per reason a connect stopped.
- `server-command-target.ts`: which server the palette's server commands act on.

## Must-knows

- **`connect-flow.ts` is the ONE caller of `connectSavedPlace` and of the reconnect manager's lazy start.** Its three
  arms are decided by `connectionState`, and picking one wrong is silent: re-dialing a REGISTERED volume registers a
  second one under a second id, and dialing one the backoff loop already owns races it. Everything that opens a place
  (the switcher row, the hub, the pane, the sheet) comes through here. DETAILS § "The three arms".
- **The attempt id is minted before the first dial** and handed out through `onAttemptStarted`, so Cancel is armed from
  the first millisecond. A dial runs up to 30 s and the promise doesn't settle until it's over.
- **Spell a remote path only through `server-path-utils.ts`**, which mints the prefix exactly as
  `cmdr_fs::volume::ids::sftp_app_root` does: host lowercased, account left alone, port literal. ❌ Never hand-build
  one, and ❌ never let a bare server-absolute path (`/srv/data`) out of a pane: Rust's mount table answers the LOCAL
  root for any absolute path it doesn't know, so a scheme-free remote path resolves to the boot disk at every resolver
  site.
- **Every refusal kind carries its own sentence** (`connect-refusals.ts`, a `Record`, so a new kind can't compile
  wordless). ❌ `needs_credentials` is NOT `authentication_rejected`: telling someone who has never entered a password
  that theirs is wrong is what collapsing them does. Same for `auth_method_unsupported`, where the secret was never
  sent.
- **❌ No inert affordance.** A refused connect offers Try again, which really re-dials. The "Sign in…" button lands
  with the sheet that can answer it (M2), not before; until then the `needs_sign_in` arm refuses with the reason.
- **A server command aims at the hub's CURSOR ROW first, the focused pane's volume second**
  (`server-command-target.ts`). ❗ The hub IS a pane, so reading "the focused pane's volume" alone answers the synthetic
  hub row rather than the server the user is looking at. It stops at an SMB host row instead of falling through: acting
  on something other than what someone is pointing at is worse than doing nothing.
- **The pane is where waiting is shown, the sheet is where data is typed.**
  `file-explorer/pane/RemoteConnectView.svelte` renders the states; `pane/place-connect.svelte.ts` owns the `$effect`
  and the one-dial-per-landing rule.

`DETAILS.md` holds the three arms in full, the path grammar and its Rust twin, the refusal table, and what M2 fills in.
