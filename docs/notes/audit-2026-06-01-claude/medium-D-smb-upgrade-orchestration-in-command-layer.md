# SMB-upgrade orchestration lives in the command layer, contradicting the thin-pass-through contract

**Severity:** medium **Lens:** D — IPC boundary **Confidence:** high

## Location

`apps/desktop/src-tauri/src/commands/network.rs:374-462` and `apps/desktop/src-tauri/src/commands/network.rs:467-537`

## What

`upgrade_to_smb_volume_inner` (~89 lines) and `upgrade_to_smb_volume_with_credentials` (~70 lines) carry the full
SMB-upgrade orchestration in the command file: volume lookup, `statfs` parsing, mDNS hostname resolution, Keychain
lookup, the connection attempt, the "remember credentials" branch, and the three-way mapping of `UpgradeError` to
`UpgradeResult` variants (`Success` / `CredentialsNeeded` / `NetworkError`). The building-block helpers
(`try_smb_upgrade`, `resolve_ip_to_hostname_with_wait`, `get_keychain_password`) already live in `network::smb_upgrade`,
but the decision logic that strings them together does not.

## Why it matters

`commands/CLAUDE.md` and `network/CLAUDE.md` both state the upgrade business logic lives in `network::smb_upgrade` and
the commands are "thin wrappers." They aren't: the part that decides which `UpgradeResult` the user sees — and the
credential-save side effect — sits in the command. Because it's a free fn adjacent to a `#[tauri::command]`, the
orchestration can't be unit-tested without a running app (the exact failure mode the thin-command rule exists to
prevent), and the two functions duplicate the same lookup-and-map sequence, so a fix to one (say, a new error class or a
hostname-resolution timeout change) silently skips the other.

## Evidence

```rust
// commands/network.rs:435-461
let result = try_smb_upgrade(&info.server, &info.share, &mount_path, username, password, info.port, &volume_id).await;
match result {
    Ok(()) => Ok(UpgradeResult::Success),
    Err(UpgradeError::Auth) => {
        log::info!("Stored credentials didn't work, requesting new credentials");
        Ok(UpgradeResult::CredentialsNeeded { server: info.server, share: info.share, port: info.port,
            display_name, username_hint: username.map(|s| s.to_string()),
            message: Some("Stored credentials didn't work".to_string()) })
    }
    Err(UpgradeError::Network(msg)) => Ok(UpgradeResult::NetworkError { message: msg }),
}
```

The same volume-lookup + `get_smb_mount_info` + `resolve_ip_to_hostname_with_wait` + `try_smb_upgrade` + 3-way match
block reappears at `network.rs:482-536`, the only deltas being the credential source and the `remember_in_keychain` save
branch.

## Suggested fix

Move the orchestration into `network::smb_upgrade` as two plain async functions, for example
`upgrade_with_stored_creds(volume_id) -> UpgradeResult` and
`upgrade_with_explicit_creds(volume_id, username, password, remember) -> UpgradeResult`, factoring the shared "resolve
volume + statfs + hostname, then map `UpgradeError`" core into a single private helper. The `#[tauri::command]` wrappers
then keep only the `ensure_mdns_started(app_handle)` kick (which legitimately needs a concrete `AppHandle`) and
delegate. This restores the contract, removes the duplicated branch, and makes the result-mapping unit-testable in the
module.

## Notes

`upgrade_to_smb_volume` (the outer wrapper at `network.rs:354-368`) is already correctly thin — it does the mDNS kick
and delegates. The violation is specifically the `_inner` and `_with_credentials` bodies. The MCP
`upgrade_smb_to_direct` tool also calls `_inner`, so keeping the moved function `AppHandle`-free preserves that path.
