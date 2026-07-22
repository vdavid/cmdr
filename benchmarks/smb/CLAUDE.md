# SMB benchmark

Standalone throughput benchmark with its own `Cargo.toml`. It uses the old `smb` crate (smb-rs); the main app uses
`smb2`, so this is not affected by that migration. Setup and usage: `README.md`.

## Gotchas and version pins

- **`sspi` pinned to `=0.18.7`**: NTLM auth fails without the `kerberos` feature on `sspi` 0.18.x (`Negotiate` needs
  `target_name` when `USE_SESSION_KEY` is set, which `smb` only supplies with Kerberos). Fixed in `sspi` 0.19, but `smb`
  0.11.1 still depends on `sspi` 0.18.x. Remove the pin once `smb` ships against `sspi` 0.19+.
- **`smb-rpc` pinned to `=0.11.1`**: must match the `smb` crate version exactly, or you get conflicting-type errors.
- **Chunk size capped at 64 KB**: the Pi's Samba hangs on 1 MB despite negotiating 8 MB. `smb`'s `read_block` /
  `write_block` don't clip to negotiated sizes, so the caller must chunk.
- **Unique per-cycle dir names** (`n-0`, `d-0`, …): without them, `list` after delete + recreate returns stale entries
  from SMB cache.

Full details (`smb` crate API call flow, negotiation): `DETAILS.md`.
