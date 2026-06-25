# Secrets

Generic key-value secret storage with pluggable backends. The trait stores opaque `&[u8]` / `Vec<u8>`; callers own
their serialization. Consumers: `network/keychain.rs` (SMB creds, `username\0password` under `smb://server/share` keys)
and `ai/api_keys.rs` (cloud AI API keys, raw UTF-8 under `ai.apiKey.<provider>` keys).

## Module map

- **`mod.rs`**: `SecretStore` trait, `store()` global accessor, backend selection.
- **`keychain_macos.rs`**: macOS Keychain via `security-framework` (production macOS).
- **`keyring_linux.rs`**: Linux Secret Service via `keyring-core` + `zbus-secret-service-keyring-store`.
- **`encrypted_file.rs`**: Cocoon-encrypted (ChaCha20-Poly1305, `/etc/machine-id` key) fallback for prod Linux without
  Secret Service.
- **`plain_file.rs`**: Plain JSON file (dev mode, all platforms).
- **`system_keychain_smb.rs`** (macOS only): reads SMB passwords *other* apps (Finder/macOS) saved in the login
  keychain, so an os-mount volume can borrow it and go green without retyping. See the guardrail below.

## Backend selection

`store()` picks once at first access: `CMDR_SECRET_STORE=file` env (set by `tauri-wrapper.ts` in dev) → `PlainFileStore`;
else macOS → Keychain; else Linux + Secret Service available → Keyring; else Linux → `EncryptedFileStore`; else
`PlainFileStore`. Chosen at init via an `is_available()` write-read-delete probe, not per-operation.

## Must-knows

- **Never call `system_keychain_smb::read_password` at startup**: it reads `kSecClassInternetPassword` data and triggers
  the macOS SecurityAgent consent dialog (one per share = popup storm, the FDA-popup lesson). It is NOT a `SecretStore`
  backend. Consume it only from user-initiated commands (`upgrade_to_smb_volume_using_saved_password` /
  `system_has_saved_smb_password`). `account_for_any` only probes attributes (no dialog); `read_password` reads data (a
  dialog; "Always Allow" adds Cmdr to the item ACL for silence after).
- **`keyring-core` needs a process-wide default store**: installed lazily via a `Once` in `ensure_default_store()`,
  called from every public `KeyringStore` method. Use `set_secret`/`get_secret` (binary) over `set_password`/
  `get_password` (forces UTF-8).
- **macOS `SERVICE_NAME` is instance-suffixed**: prod (env unset/empty) keeps `"Cmdr"`; any non-empty
  `CMDR_INSTANCE_ID` maps to `"Cmdr-<instance>"`. Belt-and-suspenders so a stray non-prod launch on the Keychain backend
  can't mix into prod credentials.
- **File stores respect `CMDR_DATA_DIR`** so dev and prod credentials are isolated. Dev mode doesn't encrypt (developer's
  own machine, dev data dir).
- **`EncryptedFileStore` and `PlainFileStore` have separate `Mutex` statics** (compiled on different platforms).
- **`is_file_backed()`** drives a one-time frontend info toast about credential storage. It returns true in dev mode
  (PlainFileStore), but the toast isn't shown in dev.
- **`KeyringStore::is_available()` does a full write-read-delete probe** once at startup, to catch locked keyrings that
  silently accept writes without persisting.

Full details (Linux path-change history, migration notes, the `keyring` v4 ecosystem-split rationale):
[DETAILS.md](DETAILS.md).
