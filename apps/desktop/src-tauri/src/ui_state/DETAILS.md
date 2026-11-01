# UI state mirror details

Must-knows: `CLAUDE.md`.

## Why this is its own module

- **Decision**: these stores live in `crate::ui_state`, outside both `tools/` and `mcp/`.
- **Why**: 22 files outside the AI surface read or write them (IPC commands, the listing host, the error reporter, and
  others). They describe the UI, not a transport or a tool, so naming them after either one would mislead every reader.

## The stores

- `pane_state.rs`: updated over IPC by the frontend. Tools read it for pane context, and `cmdr://state` renders it.
- `dialog_state.rs`: soft dialogs (overlays with no Tauri window) register here so `cmdr://state` and `dialog` can see
  them. `register_known_dialogs` lists every dialog id an external caller may name.
- `archive_password.rs`: the encrypted-archive prompt writes what it's asking. `cmdr://state` names the archive, and
  `unlock` answers it.
- `listing_errors.rs`: `cmdr_listing`'s streaming read records every `listing-error`. Ring-buffered to 20 entries, so a
  `cmdr://state` read pays a bounded YAML cost, and an E2E test can assert "no failure since X" without reading log files.
- `terminal_ops.rs`: the operation manager removes an op from its registry when it settles, so its outcome lives only in
  the terminal events. This ring keeps them, so an `await operation_complete` arriving just after the end still resolves.
  The module's own doc comment has the full reasoning.
