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
- **The test backend refuses to guess a directory**: `init_store()` returns `TestStore` under `cfg(test)`, and
  `TestStore` resolves `CMDR_DATA_DIR` fresh on every operation rather than sampling it once, so a test's
  `crate::test_support::isolate_secrets()` lands whatever order the process-global `LazyLock` initialized in. When that
  var is unset it panics instead of falling back to the platform data dir. The fallback is the developer's real
  `secrets.json`: the suite wrote five live fixtures into one, and the entries it left behind then broke
  `commands::sftp` and `commands::webdav`'s "nothing is stored before anything is saved" opener on every subsequent
  run, on every branch. `TestStore` is `#[cfg(test)]`-only and the crate has no integration-test targets, so the panic
  is unreachable from any shipped build. Which tests need the helper, and why the reach is wider than it looks:
  `docs/testing.md` § "Secret-store isolation (Rust)".

## Linux migration notes

- **Encrypted-file path**: `~/.local/share/com.veszelovszki.cmdr/credentials.enc`, matching the app data dir
  convention. (Linux alpha users from before this convention would re-enter SMB passwords after upgrade.)
- **A new machine id makes `credentials.enc` unreadable for good** (a home folder restored onto a new install, a cloned
  VM). `get` and `keys` answer the error and touch nothing. The first `set` or `delete` sets the file aside through
  `state_recovery::set_aside` (`StateFile::Credentials`, one notice) and starts an empty store. Before that, every save
  read the file first and failed, so no password could ever be saved again (#537). The module also compiles for tests on
  any Unix (with `cocoon` as a dev-dependency), so these cells run on a Mac.
- ❗ **Set-aside needs a key this machine REALLY has.** The machine id is read at every operation (`machine_key`): a
  machine id that exists but can't be read right now (EIO, EACCES) fails the operation, ❌ never the fixed fallback key,
  which would fail to open this machine's own file and set it aside on the next save. The fallback keys only a machine
  with no id at all (absent or blank `/etc/machine-id`), where it is that machine's real key for good, so a foreign file
  there is still set aside.
- ⚠️ **Known limitation: two OSes sharing one home folder** (a dual boot, each with its own machine id) can't share
  `credentials.enc`: each one's first save sets the other's file aside. A keyring (Secret Service) avoids it; per-id file
  names would trade it for a restored home starting empty without a notice.
- **`keyring-core` over `keyring = "3"`**: adopted during the keyring v4 ecosystem split, when the canonical `keyring`
  crate became a sample/example crate and the cross-platform API moved to `keyring-core` with each backend shipping
  separately. We picked the zbus-based backend (`zbus-secret-service-keyring-store` with `rt-tokio-crypto-rust`) since
  we already use zbus + tokio + RustCrypto, so no system libdbus is needed.

## `system_keychain_smb.rs` mechanics (macOS)

Reads `kSecClassInternetPassword` items keyed by `srvr`/`acct`/`smb` (which the generic-password store can't), via raw
`SecItemCopyMatching` through `security-framework-sys` + `core-foundation`. `server_query_candidates` tries the name
forms Finder might key by (mDNS service name first); `is_real_account` skips the "No user account" guest sentinel. The
consent dialog on `read_password` is drawn by SecurityAgent and its text isn't customizable.
