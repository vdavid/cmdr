# Network SMB support

Discover, browse, and mount SMB network shares on macOS. Integrates mDNS discovery, SMB client, and macOS Keychain.

## Architecture

- **Discovery**: `mdns_discovery.rs` — Pure Rust mDNS using `mdns-sd` crate. Replaced deprecated NSNetServiceBrowser.
- **Share listing**: Split across multiple files:
  - `smb_client.rs` — Top-level share-listing entry point; orchestrates guest -> keychain -> prompt auth flow; tries smb-rs first, falls back to smbutil
  - `smb_connection.rs` — TCP connection establishment and IPC-level share listing calls
  - `smb_cache.rs` — 30-second in-memory cache for share lists, keyed by server address
  - `smb_smbutil.rs` — `smbutil view -G` fallback for older Samba/NAS servers
  - `smb_types.rs` — Shared types (`SmbShare`, `AuthMode`, `SmbError`, etc.)
  - `smb_util.rs` — Helpers: hostname derivation, IP resolution, account-name normalization
- **Mounting**: `mount.rs` — macOS `NetFSMountURLSync` for native `/Volumes/` mounts.
- **Auth**: `keychain.rs` — macOS Keychain via `security-framework`. Credentials cached in-memory after first access.
- **State**: `known_shares.rs` — Connection history in `known-shares.json` (usernames, last auth mode, timestamps).

## Key decisions

### Always use IP when available

smb-rs doesn't resolve `.local` hostnames reliably (std lib DNS doesn't handle mDNS). Always pass resolved IP from Bonjour discovery. If IP unavailable, use derived hostname (`service_name_to_hostname`).

### Guest-first auth flow

1. Try anonymous/guest access first
2. On auth error → check Keychain
3. If no stored creds → prompt user
4. Never assume "guest only" — always offer "Sign in for more access" when guest succeeds (can't distinguish guest-only from guest-or-creds at probe time)

### smbutil fallback (macOS only)

`smb` crate fails on older Samba servers with RPC incompatibility. Classify error as `ProtocolError`, then try `smbutil view -G` as fallback. This handles Linux Samba and old NAS devices gracefully.

### No persistent connection pool

Task 2.9 explicitly marked "not needed." smb-rs connections are lightweight and created on-demand. Caching is at the share list level (30s TTL), not TCP connection level.

### In-memory credential cache

After first Keychain fetch, credentials cached in `CREDENTIAL_CACHE` (LazyLock + RwLock). Prevents repeated Keychain dialogs during session. Cache keyed by `"smb://{server}/{share}"`.

## Gotchas

- **Don't hold mutex during DNS resolution**: `resolve_network_host_sync` extracts host info, releases mutex, does blocking DNS, re-acquires mutex to update. Old code held mutex across network call (deadlock risk).
- **Auth mode is a guess**: `GuestAllowed` means "guest worked, creds might also work." `CredsRequired` means "guest failed, must have creds." Can't detect guest-only vs guest-or-creds without trying both.
- **NetFS error 17 (EEXIST) is success**: Share already mounted. Return existing mount path, set `already_mounted: true`. Not an error.
- **mDNS service type must include `.local.`**: `mdns-sd` requires full form `"_smb._tcp.local."` (trailing dot). Without it, browse() fails silently.
- **Keychain account name is lowercase**: `make_account_name` lowercases server name for consistency. Prevents duplicate entries for "SERVER" vs "server".
