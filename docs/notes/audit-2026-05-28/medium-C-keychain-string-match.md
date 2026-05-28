# Keychain error classifier substring-matches localizable text

**Severity:** medium **Lens:** C — Error handling **Confidence:** high

## Location

`apps/desktop/src-tauri/src/secrets/keychain_macos.rs:59-68`

## What

`classify_security_error` decides whether a Keychain failure is `NotFound` vs `AccessDenied` vs `Other` by
substring-matching the formatted `security_framework::base::Error`. The `security-framework` crate exposes the OSStatus
integer via `Error::code()` — the real, typed contract — but this code reaches for the `Display` string instead, in
direct violation of AGENTS.md's banned-pattern list. No `// allowed-error-string-match: ...` opt-out is present.

## Why it matters

- **Localization:** On a non-English macOS the Keychain error's formatted message localizes. `"not found"` doesn't match
  `"introuvable"`, so `errSecItemNotFound` reaches the frontend as `Other(...)` instead of `NotFound(...)`. The AI
  API-key flow at `ai/api_keys.rs:106` then surfaces an error toast to the user (`Err(e)` arm) instead of silently
  treating "no key stored" as the empty-string default. SMB credential flow at `network/keychain.rs` similarly
  miscategorizes "no creds stored" as a generic failure and prompts the user instead of falling back to guest auth as
  designed.
- **Upstream wording drift:** `security-framework` could reformat its `Display` impl in a minor version bump and break
  the classifier without anyone noticing. The OSStatus codes are stable Apple ABI; the Rust formatting isn't.

## Evidence

```rust
fn classify_security_error(key: &str, error: security_framework::base::Error) -> SecretStoreError {
    let msg = format!("{}", error);
    if msg.contains("not found") || msg.contains("No such") || msg.contains("errSecItemNotFound") {
        SecretStoreError::NotFound(format!("No secret found for key: {}", key))
    } else if msg.contains("denied") || msg.contains("cancelled") {
        SecretStoreError::AccessDenied(msg)
    } else {
        SecretStoreError::Other(msg)
    }
}
```

`security_framework::base::Error::code() -> OSStatus`. The standard codes are documented constants:
`errSecItemNotFound = -25300`, `errSecAuthFailed = -25293`, `errSecUserCanceled = -128`, etc.

## Suggested fix

Match on `error.code()`:

```rust
fn classify_security_error(key: &str, error: security_framework::base::Error) -> SecretStoreError {
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    const ERR_SEC_AUTH_FAILED: i32 = -25293;
    const ERR_SEC_USER_CANCELED: i32 = -128;
    const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
    match error.code() {
        ERR_SEC_ITEM_NOT_FOUND => SecretStoreError::NotFound(format!("No secret found for key: {}", key)),
        ERR_SEC_AUTH_FAILED | ERR_SEC_USER_CANCELED | ERR_SEC_INTERACTION_NOT_ALLOWED => {
            SecretStoreError::AccessDenied(format!("{}", error))
        }
        _ => SecretStoreError::Other(format!("{}", error)),
    }
}
```

If pulling in `security_framework::base::SecError` symbolic constants is preferred, do that instead — but the integer
form is fine and self-documenting.

## Notes

Worth checking whether `scripts/check/checks/error-string-match` actually scans this file and what its detection regex
is. Two `error-string-match` slip-throughs in one audit pass (this + `installer.rs::is_permission_error`) suggests the
check has a blind spot.
