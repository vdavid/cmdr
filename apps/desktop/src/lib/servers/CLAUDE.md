# Servers

Everything the app does with a remote place it can dial: the path spelling, the connect flow, the one sign-in sheet, and
the words for a connect that stopped. Up: `../../CLAUDE.md`. Backend contracts: `crates/cmdr-sftp/DETAILS.md`,
`crates/cmdr-webdav/DETAILS.md`, `apps/desktop/src-tauri/src/server_volumes.rs`.

## Module map

- `server-path-utils.ts`: reads and writes `<protocol>://<user>@<host>:<port>/<server path>`.
- `address-parser.ts`: one pasted string into a protocol and an endpoint. `server-form.ts`: the add form's model, and a
  `ServerTarget` out of it.
- `connect-flow.ts`: `connectPlace` picks the move by the volume's standing. `open-sign-in.ts`: the three ways the sheet
  opens, each with the command that standing needs.
- `SignInSheet.svelte` (+ `ServerFormFields`, `SignInCredentialFields`, `HostKeyStep`), behind
  `sign-in-sheet-state.svelte.ts`. `sign-in-contract.ts`: what they hand each other.
- `connect-refusals.ts`: one sentence per reason, and which field it goes under. `server-command-target.ts`: which
  server a palette command acts on.
- SMB's side of the sheet is a sibling, ❌ not here: `../file-explorer/network/smb-sign-in.ts`.

## Must-knows

- **`connect-flow.ts` is the ONE caller of `connectSavedPlace` and the reconnect manager's lazy start**, and
  `open-sign-in.ts` is the one that picks between mending a REGISTERED volume and dialing an absent one. Getting that
  wrong is silent: a dial on a registered volume registers a SECOND one under a second id. DETAILS § "The three arms".
- **The sheet never dials, and stays open across rounds.** It calls the caller's `attempt` as many times as the user
  retries; a first connect is three round-trips, and a sheet that closed between them would lose what was typed.
- **Username editability is the SHAPE VARIANT's property, ❌ never the sheet's mode.** Read either as a mode rule and
  you break the other protocol. DETAILS § "The renderer table".
- **The attempt id is minted before the first dial**, so Cancel is armed from the first millisecond. A dial runs up to
  30 s.
- **Spell a remote path only through `server-path-utils.ts`**, which mints the prefix exactly as
  `cmdr_fs::volume::ids::sftp_app_root` does. ❌ Never hand-build one, and ❌ never let a bare server-absolute path
  (`/srv/data`) out of a pane: Rust's mount table answers the LOCAL root for any absolute path it doesn't know.
- **Every refusal kind carries its own sentence AND its own field** (`connect-refusals.ts`, two `Record`s, so a new kind
  can't compile wordless or homeless). ❌ `needs_credentials` is NOT `authentication_rejected`.
- **❌ No inert affordance.** Every button does the thing it says. That is why a changed host key offers Disconnect
  rather than "Trust it": nobody can answer for a fingerprint they haven't been shown.
- **The pane is where waiting is shown, the sheet is where data is typed.**
  `file-explorer/pane/RemoteConnectView.svelte` renders the states; `pane/place-connect.svelte.ts` owns the `$effect`
  and the one-dial-per-landing rule. A PHONE rides the same view through `pane/device-connect.svelte.ts` and ❌ never
  through `connect-flow.ts`: it has no credential, no backoff, and no sheet. DETAILS § The device dial.

`DETAILS.md` holds the three arms, the sheet contract, the renderer table, the path grammar, the refusal table, the
reserved S3 and OAuth shapes, and what later milestones fill in.
