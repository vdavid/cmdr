# SMB benchmark details

Depth for the benchmark. `CLAUDE.md` holds the version pins and gotchas that prevent silent breakage; the `smb` crate
API call patterns live here.

## `smb` crate API patterns

- **Connection flow**: `Client::new` → `connect_to_address` (TCP) → `share_connect` (auth + tree) → `get_tree` → file
  ops.
- **File creation**: `tree.create_file(path, CreateDisposition, FileAccessMask)` returns a `Resource`; call
  `.unwrap_file()` for a `File`.
- **Opening existing**: `tree.open_existing(path, access)`.
- **Directory listing**: `Directory::query::<FileBothDirectoryInformation>(dir, pattern)` returns an async stream;
  filter out `.` and `..`.
- **Deletion**: open with `FileAccessMask::new().with_delete(true)`, then set `FileDispositionInformation` via
  `handle.set_info()`. The file or dir is deleted on close.

## Chunk size negotiation

After `share_connect`, query `Connection::conn_info().negotiation.max_read_size` and `max_write_size` for the optimal
transfer sizes, then cap at 64 KB (see the CLAUDE.md gotcha on the Pi's Samba hang).

## Server name for NTLM

Using the IP as `server_name` in `connect_to_address` is fine for NTLM. The SPN only matters for Kerberos.
