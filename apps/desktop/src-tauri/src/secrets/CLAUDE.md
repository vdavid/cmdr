# Secrets

Generic key-value secret storage with pluggable backends.

## Architecture

- `mod.rs` -- `SecretStore` trait, `store()` global accessor, backend selection
- `keychain_macos.rs` -- macOS Keychain via `security-framework` (production macOS)
- `keyring_linux.rs` -- Linux Secret Service via `keyring` crate (production Linux with GNOME/KDE)
- `encrypted_file.rs` -- Cocoon-encrypted file fallback (production Linux without secret service)
- `plain_file.rs` -- Plain JSON file (dev mode, all platforms)

## Backend selection

`store()` picks the backend once at first access:

1. `CMDR_SECRET_STORE=file` env var -> `PlainFileStore` (set by `tauri-wrapper.js` in dev mode)
2. macOS -> `KeychainStore`
3. Linux + Secret Service available -> `KeyringStore`
4. Linux + no Secret Service -> `EncryptedFileStore`
5. Other platforms -> `PlainFileStore`

## Key decisions

### Generic byte storage, not typed credentials

The trait stores opaque `&[u8]` / `Vec<u8>`. Callers (like `network/keychain.rs` for SMB) handle their own
serialization format. This keeps the store reusable for any secret type.

### File-based stores use `CMDR_DATA_DIR`

Unlike the old `keychain_linux.rs` which hardcoded `~/.local/share/cmdr/`, file stores respect `CMDR_DATA_DIR` so dev
and prod credentials are properly isolated.

### Plain file for dev, encrypted for prod Linux

Dev mode doesn't encrypt -- it's the developer's machine, and the file is in the dev data dir. The Linux production
fallback encrypts with `cocoon` (ChaCha20-Poly1305) using `/etc/machine-id` as the key.

### Backend is chosen at init, not per-operation (Linux)

The old `keychain_linux.rs` tried Secret Service then file fallback on every read/write/delete. The new design picks one
backend at startup via `is_available()` (a real write-read-delete probe). This means credentials saved to Secret Service
before the refactor won't be found if the service becomes unavailable later. Acceptable because Linux SMB is alpha and
the scenario is rare (user would need to switch from a full DE to headless mid-session).

### Encrypted file path changed (Linux)

Old: `~/.local/share/cmdr/credentials.enc`. New: `~/.local/share/com.veszelovszki.cmdr/credentials.enc` (matches the
app's data dir convention). Existing Linux alpha users would need to re-enter SMB passwords after upgrade.

## Gotchas

- `keyring` crate works with strings, not bytes. Values are converted via `String::from_utf8` (all current callers pass
  valid UTF-8). Non-UTF-8 values would error.
- `EncryptedFileStore` and `PlainFileStore` have separate `Mutex` statics for file access serialization (they're
  compiled on different platforms).
- `is_file_backed()` is checked by the frontend to show a one-time info toast about credential storage. In dev mode it
  always returns true (PlainFileStore), but the toast isn't relevant since it's dev.
- `KeyringStore::is_available()` does a full write-read-delete probe to catch locked keyrings that silently accept writes
  without persisting. This runs once at startup.
