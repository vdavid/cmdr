# SMB benchmark

> **Note**: This benchmark uses the old `smb` crate (smb-rs). The main Cmdr app has migrated to `smb2`. This benchmark
> has its own `Cargo.toml` and is not affected by the migration.

See [README.md](README.md) for setup and usage.

## `smb` crate API patterns

- **Connection flow**: `Client::new` -> `connect_to_address` (TCP) -> `share_connect` (auth + tree) -> `get_tree` ->
  file ops.
- **File creation**: `tree.create_file(path, CreateDisposition, FileAccessMask)` returns a `Resource`. Call
  `.unwrap_file()` to get a `File`.
- **Opening existing**: `tree.open_existing(path, access)`.
- **Directory listing**: `Directory::query::<FileBothDirectoryInformation>(dir, pattern)` returns an async stream.
  Filter out `.` and `..` entries.
- **Deletion**: Open with `FileAccessMask::new().with_delete(true)`, then set `FileDispositionInformation` via
  `handle.set_info()`. The file/dir is deleted on close.

## Key decisions

- **Chunk size negotiation**: After `share_connect`, we query `Connection::conn_info().negotiation.max_read_size` and
  `max_write_size` for optimal transfer sizes. Capped at 64 KB because Pi's Samba hangs on 1 MB despite negotiating 8
  MB. The `smb` crate's `read_block`/`write_block` don't internally clip to negotiated sizes, so the caller must chunk
  correctly.
- **Unique cycle directories**: Each benchmark iteration uses a unique directory name (`n-0`, `d-0`, etc.) to avoid SMB
  cache staleness. Without this, `list` after `delete` + recreate can return stale entries from the previous cycle.

## Gotchas

- **`sspi` 0.18.9 auth bug**: NTLM auth fails without the `kerberos` feature because `Negotiate` requires `target_name`
  when `USE_SESSION_KEY` is set, but `smb` only provides it with Kerberos enabled. Fixed in `sspi` 0.19.0, but `smb`
  0.11.1 depends on `sspi` 0.18.x. Pinned to `=0.18.7` until `smb` updates its dep. Remove the pin when `smb` releases a
  version using `sspi` 0.19+.
- **`smb-rpc` version pin**: Must match the `smb` crate's version exactly (`=0.11.1`), otherwise you get conflicting
  type errors.
- **Server name for NTLM**: Using the IP as `server_name` in `connect_to_address` is fine for NTLM. SPN only matters for
  Kerberos.
