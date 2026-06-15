# Secrets details

Depth for the pluggable secret store. `CLAUDE.md` holds the must-knows; this file holds rationale and Linux migration
notes.

## Decisions

- **Generic byte storage, not typed credentials**: the trait stores opaque `&[u8]` / `Vec<u8>` so it stays reusable for
  any future secret type. Callers (`network/keychain.rs`, `ai/api_keys.rs`) own their own serialization.
- **Backend chosen at init, not per-operation (Linux)**: the design picks one backend at startup via an `is_available()`
  write-read-delete probe. A consequence: credentials saved to Secret Service won't be found if the service later
  becomes unavailable (for example switching from a full desktop environment to headless mid-session). Acceptable
  because Linux SMB is alpha and the scenario is rare.

## Linux migration notes

- **Encrypted-file path**: `~/.local/share/com.veszelovszki.cmdr/credentials.enc`, matching the app data dir
  convention. (Linux alpha users from before this convention would re-enter SMB passwords after upgrade.)
- **`keyring-core` over `keyring = "3"`**: adopted during the keyring v4 ecosystem split, when the canonical `keyring`
  crate became a sample/example crate and the cross-platform API moved to `keyring-core` with each backend shipping
  separately. We picked the zbus-based backend (`zbus-secret-service-keyring-store` with `rt-tokio-crypto-rust`) since
  we already use zbus + tokio + RustCrypto, so no system libdbus is needed.

## `system_keychain_smb.rs` mechanics (macOS)

Reads `kSecClassInternetPassword` items keyed by `srvr`/`acct`/`smb` (which the generic-password store can't), via raw
`SecItemCopyMatching` through `security-framework-sys` + `core-foundation`. `server_query_candidates` tries the name
forms Finder might key by (mDNS service name first); `is_real_account` skips the "No user account" guest sentinel. The
consent dialog on `read_password` is drawn by SecurityAgent and its text isn't customizable.
