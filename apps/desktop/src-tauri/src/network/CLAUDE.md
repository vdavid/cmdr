# Network SMB support

Discover, browse, and mount SMB network shares. Works on macOS and Linux.

## Architecture

- **Discovery**: `mdns_discovery.rs` — Pure Rust mDNS using `mdns-sd` crate. Cross-platform.
- **Share listing**: Split across multiple files:
  - `smb_client.rs` — Top-level share-listing entry point; orchestrates guest -> keychain -> prompt auth flow; tries smb-rs first, falls back to smbutil (macOS only)
  - `smb_connection.rs` — TCP connection establishment and IPC-level share listing calls
  - `smb_cache.rs` — 30-second in-memory cache for share lists, keyed by server address
  - `smb_smbutil.rs` — `smbutil view -G` fallback for older Samba/NAS servers (macOS); on Linux delegates to `smb_smbclient`
  - `smb_smbclient.rs` — `smbclient -L` fallback for Linux (requires `samba-client` package)
  - `smb_types.rs` — Shared types (`SmbShare`, `AuthMode`, `SmbError`, etc.)
  - `smb_util.rs` — Helpers: hostname derivation, IP resolution, account-name normalization
- **Mounting** (platform-specific via `#[path]` in `mod.rs`):
  - `mount.rs` — macOS `NetFSMountURLSync` for native `/Volumes/` mounts
  - `mount_linux.rs` — Linux `gio mount` for GVFS-based user-space mounts
- **Auth** (platform-specific via `#[path]` in `mod.rs`):
  - `keychain.rs` — macOS Keychain via `security-framework`
  - `keychain_linux.rs` — Two-tier: Secret Service via `keyring` crate → encrypted file via `cocoon` crate
- **State**: `known_shares.rs` — Connection history in `known-shares.json` (usernames, last auth mode, timestamps).

## Platform strategy

| Component | macOS | Linux |
|-----------|-------|-------|
| mDNS discovery | `mdns-sd` (pure Rust) | `mdns-sd` (same) |
| SMB share listing | `smb` + `smb-rpc` crates | `smb` + `smb-rpc` (same) |
| smbutil fallback | `smbutil view -G` | `smbclient -L` (from `samba-client` package) |
| Credential storage | `security-framework` (macOS Keychain) | `keyring` (Secret Service) → `cocoon` encrypted file fallback |
| Mounting | `NetFSMountURLSync` → `/Volumes/` | `gio mount` → `/run/user/<uid>/gvfs/` |

## Key decisions

### Always use IP when available

smb-rs doesn't resolve `.local` hostnames reliably (std lib DNS doesn't handle mDNS). Always pass resolved IP from mDNS discovery. If IP unavailable, use derived hostname (`service_name_to_hostname`).

### Guest-first auth flow

1. Try anonymous/guest access first
2. On auth error → check stored credentials
3. If no stored creds → prompt user
4. Never assume "guest only" — always offer "Sign in for more access" when guest succeeds (can't distinguish guest-only from guest-or-creds at probe time)

### smbutil / smbclient fallback

`smb` crate fails on older Samba servers (for example, Raspberry Pi) with RPC incompatibility. Classify error as `ProtocolError`, then try a platform-specific CLI fallback:
- **macOS:** `smbutil view -G` (built-in).
- **Linux:** `smbclient -L` (from `samba-client` package). If `smbclient` is not installed, returns a helpful error message. The `smb_smbutil.rs` Linux stubs delegate to `smb_smbclient.rs`.
- **Other platforms:** stubs return `ProtocolError`.

### No persistent connection pool

smb-rs connections are lightweight and created on-demand. Caching is at the share list level (30s TTL), not TCP connection level.

### In-memory credential cache

After first credential fetch, credentials cached in `CREDENTIAL_CACHE` (LazyLock + RwLock). Prevents repeated Keychain/secret-service round-trips during session. Cache keyed by `"smb://{server}/{share}"`.

### Linux credential storage fallback

On Linux, `keychain_linux.rs` tries Secret Service (GNOME Keyring / KDE Wallet) first. If unavailable (no D-Bus service, headless server, minimal DE), it falls back to an encrypted file at `~/.local/share/cmdr/credentials.enc`. The file is encrypted with `cocoon` (Chacha20-Poly1305) using `/etc/machine-id` as the password, with 0600 file permissions. A static `USING_FILE_FALLBACK` flag tracks whether the fallback is active for the frontend to show a one-time info toast. Corrupted credential files are handled gracefully (start fresh, log warning).

### Linux mounting via GVFS

`gio mount` is used for user-space SMB mounting on Linux. It requires the `gvfs-smb` package. If `gio` is not available, a helpful error message is returned. Mounts appear under `/run/user/<uid>/gvfs/`.

## Gotchas

- **Don't hold mutex during DNS resolution**: `resolve_network_host_sync` extracts host info, releases mutex, does blocking DNS, re-acquires mutex to update. Old code held mutex across network call (deadlock risk).
- **Auth mode is a guess**: `GuestAllowed` means "guest worked, creds might also work." `CredsRequired` means "guest failed, must have creds." Can't detect guest-only vs guest-or-creds without trying both.
- **NetFS error 17 (EEXIST) is success** (macOS): Share already mounted. Return existing mount path, set `already_mounted: true`. Not an error.
- **mDNS service type must include `.local.`**: `mdns-sd` requires full form `"_smb._tcp.local."` (trailing dot). Without it, browse() fails silently.
- **Account name is lowercase**: `make_account_name` lowercases server name for consistency. Prevents duplicate entries for "SERVER" vs "server".
- **Linux `gio mount` requires GVFS**: The `gvfs-smb` package must be installed. Standard on Ubuntu/Fedora GNOME desktops. KDE desktops may need it explicitly.
