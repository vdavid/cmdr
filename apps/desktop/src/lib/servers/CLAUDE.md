# Servers

Everything the app does with a remote place it can dial: the path spelling, the connect flow, the one sign-in sheet, and
the words for a connect that stopped.

## Module map

- `server-path-utils.ts` reads and writes `<protocol>://<user>@<host>:<port>/<server path>`; `address-parser.ts` turns a
  pasted string into a protocol and an endpoint; `server-form.ts` is the add form's model.
- `connect-flow.ts` picks the move by the volume's standing (via `server-outcomes.ts`); `open-sign-in.ts` picks the
  command it needs and opens `SignInSheet.svelte`, over `sign-in-contract.ts` + `sign-in-sheet-state.svelte.ts`.
- `connect-refusals.ts`: a sentence per reason and the field it goes under. `server-command-target.ts`: which server a
  palette command acts on. SMB's side of the sheet is `../file-explorer/network/smb-sign-in.ts`, ❌ not here.

## Must-knows

- **`connect-flow.ts` is the ONE caller of `connectSavedPlace` and of the reconnect manager's lazy start**;
  `open-sign-in.ts` alone picks between mending a REGISTERED volume and dialing an absent one. Getting that wrong is
  silent: dialing a registered volume registers a SECOND one under a second id. DETAILS § The three arms.
- **The sheet never dials, and stays open across rounds**, calling the caller's `attempt` as often as the user retries:
  a first connect is three round-trips, and closing between them would lose what was typed.
- **Username editability is the SHAPE VARIANT's property, ❌ never the sheet's mode.** Read it as a mode rule and you
  break a protocol. DETAILS § The renderer table.
- **The attempt id is minted before the first dial**, so Cancel is armed from the first millisecond of a 30 s dial.
- **Spell a remote path only through `server-path-utils.ts`**, which mints the prefix exactly as
  `cmdr_fs::volume::ids::sftp_app_root` does. ❌ Never hand-build one, and ❌ never let a bare server-absolute path
  (`/srv/data`) leave a pane: Rust's mount table answers the LOCAL root for any path it doesn't know.
- **Every refusal kind carries its own sentence AND field** (two `Record`s in `connect-refusals.ts`, so a new kind can't
  compile wordless or homeless). ❌ `needs_credentials` is NOT `authentication_rejected`.
- **❌ No inert affordance.** Every button does the thing it says, which is why a changed host key offers Disconnect
  rather than "Trust it".
- **The pane shows the waiting, the sheet takes the typing.** In `../file-explorer/pane/`: `RemoteConnectView.svelte`
  renders the states, `place-connect.svelte.ts` owns the one-dial-per-landing `$effect`, and `device-connect.svelte.ts`
  dials a PHONE, ❌ never through `connect-flow.ts`: a phone has no credential, no backoff, and no sheet. DETAILS § The
  device dial.

The three arms, the sheet contract, the renderer table, the path grammar, and the refusal table: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
