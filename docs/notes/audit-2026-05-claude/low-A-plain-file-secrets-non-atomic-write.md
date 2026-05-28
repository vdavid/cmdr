# `PlainFileStore` writes secrets non-atomically, can drop all secrets on crash

**Severity:** low **Lens:** A — Data safety **Confidence:** high

## Location

`apps/desktop/src-tauri/src/secrets/plain_file.rs:49-67` (`write_store`) plus the matching call sites at lines 76, 99.

## What

`write_store` rewrites the entire `secrets.json` file via `std::fs::write(path, json)`. There's no temp+rename pattern,
no fsync, no atomic substitution. If the process crashes (panic, OOM, SIGKILL, power loss) between the file being
truncated and the new bytes landing, the file is left half-written. The next launch calls `read_store`, hits
`serde_json::from_slice` failure, logs `"Secret file has invalid format ({}), starting fresh"`, and returns
`StoreContents::default()` — every stored secret is silently dropped from the in-memory view, and the next `write_store`
overwrites the corrupt file with the empty (or new) state, permanently losing everything.

Permissions are also set _after_ the write (line 62), so there's a brief window where the file exists at default umask
(likely 644) holding plaintext secrets.

## Why it matters

This is the dev-mode and non-mac/non-linux secret backend. On those platforms it holds:

- SMB share passwords (`network/keychain.rs` writes through here)
- AI provider API keys (`ai/api_keys.rs` writes through here)

A crash during a `save_ai_api_key` call would corrupt the file, and the user would silently lose every SMB credential
and every AI API key on next launch. They'd be re-prompted for the SMB ones (annoying but recoverable), but BYOK AI keys
are typically pasted from a provider dashboard — re-fetching them is a friction the user wouldn't expect from a file
manager.

The dev-mode angle matters too: a dev iterating on the AI flow loses their test keys on every crash, which encourages
workarounds (committing keys to the repo, etc.).

The `EncryptedFileStore` (Linux fallback, prod) likely has the same shape and the same risk — worth checking; same fix
applies.

## Evidence

```rust
fn write_store(path: &PathBuf, contents: &StoreContents) -> Result<(), SecretStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(contents)?;
    std::fs::write(path, json.as_bytes())?;        // ← non-atomic; truncates then writes

    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;  // ← after write
    }
    Ok(())
}
```

And the read-side recovery:

```rust
match std::fs::read(path) {
    Ok(data) => serde_json::from_slice(&data).unwrap_or_else(|e| {
        warn!("Secret file has invalid format ({}), starting fresh", e);
        StoreContents::default()                   // ← silent data loss
    }),
    ...
}
```

## Suggested fix

Use the project's existing safe-overwrite shape (which `write_operations` already implements for user files):

1. Serialize to bytes.
2. Open `secrets.json.cmdr-tmp-<uuid>` with O_CREAT|O_EXCL and mode 0o600.
3. Write all bytes; `fsync` the file.
4. `rename` the temp into place (atomic on the same FS).
5. `fsync` the parent directory.

`std::fs` doesn't expose directory fsync directly; either pull in `tempfile::persist` (already a transitive dep) or call
`libc::fsync` on a `File::open(parent)?` handle. The temp file gets correct permissions from the start by passing mode
bits to `OpenOptions`, closing the umask window.

On the read side, if the parse fails, _don't_ silently start fresh: log at error, leave the file in place (don't write
over it), and return `SecretStoreError::Other` so callers see the failure. A corrupt secrets file shouldn't quietly
become an empty one.

## Notes

Dev-mode-only data loss is annoying but recoverable. The same module is the production fallback on every
non-mac/non-linux platform (`init_store` in `mod.rs:115-121`) — there it's load-bearing. The fix is the same code in
either case.
