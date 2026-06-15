# Secrets

Generic key-value secret storage with pluggable backends.

## Architecture

- `mod.rs` -- `SecretStore` trait, `store()` global accessor, backend selection
- `keychain_macos.rs` -- macOS Keychain via `security-framework` (production macOS)
- `system_keychain_smb.rs` (macOS only) -- reads SMB passwords *other* apps (Finder/macOS) saved in the login keychain, so a yellow os-mount volume can borrow the password and go green without the user retyping it. NOT a `SecretStore` backend (it reads `kSecClassInternetPassword` items keyed by `srvr`/`acct`/`smb`, which the generic-password store can't): raw `SecItemCopyMatching` via `security-framework-sys` + `core-foundation`. `account_for_any` probes attributes (no consent dialog); `read_password` reads the data (triggers the macOS consent dialog — drawn by SecurityAgent, text not customizable; "Always Allow" adds Cmdr to the item's ACL for silence thereafter). `server_query_candidates` tries the name forms Finder might key by (mDNS service name first); `is_real_account` skips the "No user account" guest sentinel. **Consumed only by user-initiated commands** (`upgrade_to_smb_volume_using_saved_password` / `system_has_saved_smb_password`); never call `read_password` at startup (would pop a system dialog per share — the FDA-popup-storm lesson).
- `keyring_linux.rs` -- Linux Secret Service via `keyring-core` + `zbus-secret-service-keyring-store` (production Linux with GNOME/KDE)
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

The trait stores opaque `&[u8]` / `Vec<u8>`. Callers handle their own serialization format. Current consumers:

- `network/keychain.rs`: SMB credentials, stored as `username\0password` under keys like `smb://server/share`.
- `ai/api_keys.rs`: cloud AI provider API keys, stored as raw UTF-8 under keys like `ai.apiKey.openai`.

This keeps the store reusable for any future secret type.

### File-based stores use `CMDR_DATA_DIR`

Unlike the old `keychain_linux.rs` which hardcoded `~/.local/share/cmdr/`, file stores respect `CMDR_DATA_DIR` so dev
and prod credentials are properly isolated.

### macOS Keychain `SERVICE_NAME` is instance-suffixed

`keychain_macos.rs` resolves `SERVICE_NAME` once at first use from `CMDR_INSTANCE_ID`: prod (env unset or empty) keeps
`"Cmdr"`; any non-empty instance ID maps to `"Cmdr-<instance>"` (for example, `"Cmdr-dev"`, `"Cmdr-dev-my-feature"`,
`"Cmdr-e2e-nonmtp1-12345"`). The wrapper already short-circuits non-prod to the file backend via `CMDR_SECRET_STORE=file`
and E2E forces the file backend via `is_e2e_mode()`, so the Keychain path is rarely hit outside prod. The suffix is the
belt-and-suspenders defense: if a stray manual launch ever lands on the Keychain backend under a non-prod identifier,
it writes to its own service namespace instead of mixing into prod credentials. Prod runs unchanged.

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

- `keyring-core` requires a process-wide default store. We install ours lazily via a `Once` in `keyring_linux.rs`'s
  `ensure_default_store()`, called from every public method on `KeyringStore`. Replaced the legacy `keyring = "3"`
  crate during the v4 ecosystem split (the canonical `keyring` crate became a sample/example crate; cross-platform
  API moved to `keyring-core` and each backend ships separately). We picked the zbus-based backend
  (`zbus-secret-service-keyring-store` with `rt-tokio-crypto-rust`) since we already use zbus + tokio + RustCrypto.
  No system libdbus needed.
- We use the `set_secret(&[u8])` / `get_secret() -> Vec<u8>` API on `keyring_core::Entry` so we can pass through
  arbitrary binary values instead of forcing UTF-8 strings via `set_password` / `get_password`.
- `EncryptedFileStore` and `PlainFileStore` have separate `Mutex` statics for file access serialization (they're
  compiled on different platforms).
- `is_file_backed()` is checked by the frontend to show a one-time info toast about credential storage. In dev mode it
  always returns true (PlainFileStore), but the toast isn't relevant since it's dev.
- `KeyringStore::is_available()` does a full write-read-delete probe to catch locked keyrings that silently accept writes
  without persisting. This runs once at startup.

Full details: [DETAILS.md](DETAILS.md).
