# Saved servers

The servers and shares the user saved, below every caller: the servers hub's commands (`commands/servers.rs`, thin IPC
over this) and Ask Cmdr read, save, edit, and dial them through the same functions. The stores themselves live in
`network/` (`sftp_known_servers.rs`, `manual_servers.rs`, `known_shares.rs`, …); this module is what a caller does with
them.

## Module map

- `listing.rs` (+ `smb_hosts.rs`, `s3_accounts.rs`, `nfs_places.rs`): every saved server as the hub lists it, one row
  per account or host; `saved_now` adds each SMB host's address and rows, for a caller that checks against them.
  `saved.rs`: the saved entry a place's volume id names (`saved_by_id`).
- `saves.rs`: an add without a dial (`save_target`) or an edit (`save_edit`: in place, or a move through
  `server_move.rs`; `save_place_edit`: only the editable fields, the rest as stored). `smb_hosts::edit_host` is an SMB
  host's edit. `target_ids.rs`: the id a `ServerTarget` names.
- `dial.rs` + `outcome.rs`: one dial per saved place, and the outcome vocabulary every caller branches on.
- `wire.rs`: the wire types, re-exported by `commands::servers`. `test_fixtures.rs`: entries both suites share.

## Must-knows

- **❌ Nothing here names `commands` or `agent`**: both sit above it (`agent/` must not reach into `commands/`).
- **The listing reads cached state only, ❌ never the wire**: the hub re-reads it on every `volumes-changed`.
- **Find a saved entry by the listing's own id funnel** (`saved_by_id`, `target_ids::place_id`), ❌ never by comparing
  a typed address: the stores normalize (a WebDAV URL gains its trailing slash).
- **An SMB host is its address AND port**, ❌ never its name or its address alone (`DETAILS.md` § "SMB hosts").
- **A save refusal writes nothing** (`SavedServerOutcome`), and an address change MOVES the server and its password
  (`server_move.rs`).
- **An unchecked add (Ask Cmdr's, or "Add anyway") contacts nothing until the user connects**: `save_target` for an
  unconnected place, and SMB's `manual_servers::save_manual_server_without_contact` (no discovery injection, no GUID
  probe; `SavedServer.awaiting_first_connect`). `DETAILS.md` § "Saving without contact".
- **The wire types' doc comments ship in `bindings.ts`**: editing one is a bindings change (`pnpm bindings:regen`).

Architecture and decisions: `DETAILS.md`. Read it before any non-trivial work here.
