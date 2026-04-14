# SMB2 migration plan

Replace the `smb` + `smb-rpc` crates with `smb2` (David's own pure-Rust SMB2/3 client) for share listing. This is phase
1 — a drop-in replacement for the current share enumeration. Phase 2 (direct SMB I/O bypassing macOS mount) is out of
scope but this migration unblocks it.

## Why

- **Single dependency** instead of two (`smb` + `smb-rpc`), both of which have pinning headaches (`sspi` 0.18.9 auth
  bug, `smb-rpc` exact version pin).
- **Clean share data** — `smb2::ShareInfo` has proper `String` fields. The current code parses NDR debug-format strings
  via `{:?}` hacks (`clean_ndr_string`, `extract_share_name`). That all goes away.
- **Typed errors** — `smb2::Error` has `Auth`, `Timeout`, `Disconnected`, `Protocol { status }` variants. Current code
  does string pattern matching on error messages ("logon failure", "access denied", "0xc000006d"). We can replace that
  with proper match arms.
- **Unblocks phase 2** — `smb2` has `read_file_pipelined`, `write_file_pipelined`, streaming I/O, directory watching —
  everything needed to bypass macOS mount for ~4x faster file ops.

## Scope

**In scope**: Replace `smb`/`smb-rpc` with `smb2` for share listing only. Keep the same auth flow, caching, smbutil
fallback, and UI behavior.

**Out of scope**: Direct SMB I/O (replacing macOS mount), benchmarks migration, any UI changes.

## Key interface differences

| Concept          | Current (`smb` 0.11.1 + `smb-rpc`)                                               | New (`smb2`)                                                                                         |
| ---------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Client creation  | `Client::new(ClientConfig)` — shared client, connections by server name          | `SmbClient::connect(ClientConfig)` — one owned client per connection                                 |
| Connect          | `client.connect_to_address(name, addr)` + `client.ipc_connect(name, user, pass)` | `SmbClient::connect(config)` — single call, addr + creds in config                                   |
| List shares      | `client.list_shares(server_name)` → `Vec<ShareInfo1>` (NDR types)                | `client.list_shares()` → `Vec<smb2::ShareInfo>` (**already filtered** to disk shares, clean strings) |
| Share type check | `format!("{:?}", share.share_type).contains("Disk")`                             | Not needed — `list_shares()` pre-filters                                                             |
| Error handling   | `err.to_string()` → string pattern matching                                      | `match err { smb2::Error::Auth { .. } => ... }`                                                      |
| Config           | `config.connection.allow_unsigned_guest_access = true`                           | `ClientConfig { username: "Guest".into(), password: "".into(), addr, timeout, .. }`                  |
| Mutability       | `&Client` (shared/immutable reference)                                           | `&mut SmbClient` (owned, mutable — one connection per client)                                        |

**Important**: `smb2::SmbClient::list_shares(&mut self)` requires mutable access. Since smb2 uses one client per
connection (no shared state), functions should own or mutably borrow the `SmbClient`.

**Important**: `smb2::ShareInfo.comment` is `String`, not `Option<String>`. Map empty string to `None` when converting
to Cmdr's `ShareInfo`.

## Implementation

### Milestone 1: Swap the dependency and rewrite share listing

These files need changes — listed in dependency order so each builds on the previous.

#### 1. `apps/desktop/src-tauri/Cargo.toml`

- In the `[target.'cfg(any(target_os = "macos", target_os = "linux"))'.dependencies]` section, remove `smb = "0.11.1"`
  and `smb-rpc = "=0.11.1"`.
- In the **same target-gated section**, add `smb2 = { git = "https://github.com/vdavid/smb2", branch = "main" }`.
  - **Why target-gated**: The current `smb`/`smb-rpc` deps are under this gate. smb2 is only used for network/SMB on
    macOS and Linux. Adding it to general `[dependencies]` would compile it on all platforms.
  - **Why git, not path**: Path deps break CI and other contributors. Git dep works everywhere. Switch to crates.io
    version when approved.

#### 2. `apps/desktop/src-tauri/src/lib.rs`

- Remove `use smb as _;` (line 27) and `use smb_rpc as _;` (line 30) — these just kept the old crates linked.
- Update log level filter: change `.level_for("smb", log::LevelFilter::Warn)` (line 260) to
  `.level_for("smb2", log::LevelFilter::Warn)`.
- Remove `.level_for("sspi", log::LevelFilter::Warn)` (line 261) — smb2 doesn't use sspi.

#### 3. `apps/desktop/src-tauri/src/network/smb_util.rs` — simplify drastically

- **Replace `filter_disk_shares`** with a **`convert_shares`** function: accept `Vec<smb2::ShareInfo>`, return
  `Vec<ShareInfo>` (Cmdr's type). `smb2::SmbClient::list_shares()` already filters to disk shares and strips `$` shares
  internally, so no filtering needed on our side. Just map fields:
  - `name` → `name`
  - `comment` → `if comment.is_empty() { None } else { Some(comment) }` (smb2 uses `String`, Cmdr uses `Option<String>`)
  - `is_disk` → `true` (already filtered by smb2)
- **Delete entirely**: `extract_share_name`, `extract_share_comment`, `clean_ndr_string`, and their tests. These exist
  solely to work around `smb-rpc`'s opaque NDR types.
- **Rewrite `is_auth_error`**: Accept `&smb2::Error` instead of `&str`. Match on:
  - `Error::Auth { .. }` → true
  - `Error::Protocol { status, .. }` where status is `STATUS_LOGON_FAILURE` or `STATUS_ACCESS_DENIED` → true
  - Everything else → false
- **Rewrite `classify_error`**: Accept `&smb2::Error` instead of `&str`. Use **exhaustive match** (no wildcard) so the
  compiler catches new variants:
  - `Error::Timeout` → `ShareListError::Timeout`
  - `Error::Auth { message }` → `ShareListError::AuthRequired`
  - `Error::Disconnected` → `ShareListError::HostUnreachable`
  - `Error::Io(e)` → check `e.kind()`: `ConnectionRefused | AddrNotAvailable` → `HostUnreachable`, `TimedOut` →
    `Timeout`, else → `ProtocolError`
  - `Error::Protocol { status, .. }` → inspect `status`: `STATUS_LOGON_FAILURE` → `AuthFailed`, `STATUS_ACCESS_DENIED` →
    `AuthRequired`/`SigningRequired` (context-dependent), else → `ProtocolError`
  - `Error::InvalidData { .. }` → `ProtocolError`
  - `Error::DfsReferralRequired { .. }` → `ProtocolError`
  - `Error::Cancelled` → `ProtocolError` (shouldn't happen during share listing)
  - `Error::SessionExpired` → `ProtocolError`

  **Why exhaustive match**: The plan's main argument for smb2 is typed errors. Using a `_` catch-all would negate that
  benefit.

  **Note**: `smb2::Error` doesn't implement `Clone` (because `Error::Io` wraps `std::io::Error`). The mapping consumes
  the error or borrows it — either works since we extract strings/status codes into Cmdr's `ShareListError`.

#### 4. `apps/desktop/src-tauri/src/network/smb_connection.rs` — rewrite or inline into smb_client.rs

This is the biggest change. The current code uses smb-rs's shared `Client` model. smb2 uses owned `SmbClient`.

With smb2, the multi-step "establish connection → IPC connect → list shares" flow collapses to "connect → list_shares".
Each function becomes ~5-10 lines. **Consider inlining these into `smb_client.rs` and deleting `smb_connection.rs`
entirely** to reduce file sprawl. If we keep the file, the changes are:

- **Delete** `establish_smb_connection` — smb2 handles connection in `connect()`.
- **Rewrite** `try_list_shares_as_guest`:
  - Build `addr` string as `"{ip_or_hostname}:{port}"`. **Critical**: when using hostname (no IP available), strip the
    `.local` suffix from the addr itself — smb2's `Connection::connect()` extracts the server name from the addr string
    and uses it in the UNC path `\\server\IPC$`. Passing `"foo.local:445"` creates `\\foo.local\IPC$`, which some
    servers reject. Pass `"foo:445"` instead.
  - Create `smb2::ClientConfig { addr, username: "Guest".into(), password: String::new(), timeout, .. }`.
  - Call `SmbClient::connect(config).await?`.
  - Call `client.list_shares().await?`.
  - Return type changes from `Result<Vec<ShareInfo1>, String>` to `Result<Vec<smb2::ShareInfo>, smb2::Error>`.
  - Remove the `tokio::time::timeout` wrapper — smb2's `ClientConfig.timeout` handles the TCP connect timeout. Keep an
    outer timeout around the full `connect + list_shares` sequence as a safety net (set smb2's config timeout to
    `timeout - 2s`, outer to `timeout`, so smb2's fires first and gives typed `Error::Timeout`).
- **Rewrite** `try_list_shares_authenticated`:
  - Same pattern but with real username/password in `ClientConfig`.
  - No need for a "fresh client" workaround — smb2 creates one connection per `SmbClient`, no shared state to leak
    between attempts.
  - `domain` field in `ClientConfig`: leave empty (default). Current smb-rs code doesn't set domain either. AD
    environments may need this later.

#### 5. `apps/desktop/src-tauri/src/network/smb_client.rs` — adjust orchestration

- Remove `use smb::{Client, ClientConfig}`.
- The `list_shares_smb_rs` function: update to use the rewritten connection functions. The function's structure stays
  the same (guest → auth → smbutil fallback), but:
  - Error types change: the functions now return `smb2::Error`, not `String`.
  - `is_auth_error(&e)` now takes `&smb2::Error`.
  - `classify_error(&e)` now takes `&smb2::Error`.
  - `filter_disk_shares(shares)` becomes `convert_shares(shares)` — just type mapping, no filtering.

#### 6. `apps/desktop/src-tauri/examples/docker_smb_test.rs` — update or delete

This example uses `smb::{Client, ClientConfig}` (line 10). After removing the `smb` dependency, it won't compile. Either
rewrite it to use smb2, or delete it if the SMB test containers have better test coverage elsewhere.

#### 7. Files that don't change

- `smb_types.rs` — Cmdr's own types, no smb-rs references.
- `smb_smbutil.rs` — shells out to CLI tools, no smb-rs references.
- `smb_smbclient.rs` — shells out to smbclient, no smb-rs references.
- `smb_cache.rs` — caches `ShareListResult` (Cmdr's own type).
- `smb_util.rs` helper functions (`service_name_to_hostname`, etc.) — these are in `mod.rs`, not `smb_util.rs`.

### Milestone 2: Verify and clean up

#### 8. Run checks

- `./scripts/check.sh --check clippy --check rustfmt` — must pass.
- `cd apps/desktop/src-tauri && cargo nextest run` — all existing network tests must pass.
- Specifically run: `cargo nextest run smb` to target SMB-related tests.

#### 9. Manual testing against a real SMB server

- Start SMB test containers: `apps/desktop/test/smb-servers/start.sh` (containers from smb2's consumer test harness)
- Run `pnpm dev` with `--features smb-e2e`, browse Network sidebar, verify share listing works (guest and
  authenticated).
- Verify smbutil fallback works (connect to a Samba server that triggers protocol errors — the Pi container if
  available).

#### 10. E2E tests

- Run SMB E2E tests: `apps/desktop/test/e2e-playwright/smb.spec.ts` (if they cover share listing).
- These use the `smb-e2e` Cargo feature and Docker containers.

#### 11. Update docs

- `apps/desktop/src-tauri/src/network/CLAUDE.md`: Update "Share listing" section — mention `smb2` instead of
  `smb-rs`/`smb-rpc`. Remove references to NDR parsing hacks. Update the platform strategy table. Remove the `smb-rpc`
  version pin gotcha. Remove the `sspi` 0.18.9 gotcha.
- `benchmarks/smb/CLAUDE.md`: Add a note that the benchmarks still use the old `smb` crate and are separate from the
  main app's migration.

#### 12. Cargo.lock cleanup

The lock file updates automatically when building after `Cargo.toml` changes. Verify the old `smb`, `smb-rpc`, and
`sspi` deps are gone from `Cargo.lock` (grep for them). Don't run a blanket `cargo update` — that would also update
unrelated dependencies.

## Risks and mitigations

- **smb2 guest auth behavior differs from smb-rs**: smb-rs had an `allow_unsigned_guest_access` config toggle. smb2
  handles signing negotiation automatically. Test guest access against servers that require signing — if smb2 rejects
  guests where smb-rs allowed them, we need to handle that in the classify_error mapping (map to `SigningRequired`).
- **smb2's `list_shares` uses RPC internally**: Same as smb-rs. If RPC fails on old Samba servers, the smbutil fallback
  still catches it — no change in behavior.
- **Timeout handling**: smb2's `ClientConfig.timeout` covers the TCP connect phase. The `list_shares()` RPC exchange has
  no per-call timeout. Keep the outer `tokio::time::timeout` around the full `connect + list_shares` sequence as a
  safety net. Set smb2's config timeout to `duration - 2s` so it fires first for connect timeouts and gives a typed
  `Error::Timeout`.
- **CI needs access to smb2 repo**: The git dependency requires the smb2 repo to be public (or use a deploy key). If
  it's private, CI will fail. Ensure the repo is accessible before merging.
- **`.local` hostname in UNC path**: smb2 uses the server name from `ClientConfig.addr` to build `\\server\IPC$` for
  share enumeration. If we pass `foo.local` as the addr, the UNC path becomes `\\foo.local\IPC$`, which some servers
  reject. Strip `.local` suffix when building the addr string, consistent with what the current code does (line 121 of
  `smb_client.rs`).

## Not changing

- **Mount logic** (`mount.rs`, `mount_linux.rs`): Still uses macOS `NetFSMountURLSync` / Linux `gio mount`. Phase 2.
- **Keychain/credential storage**: Untouched.
- **mDNS discovery**: Untouched.
- **Frontend**: No UI changes. Same Tauri commands, same event shapes.
- **Benchmarks** (`benchmarks/smb/`): Separate Cargo project with its own `smb` dependency. Out of scope.
