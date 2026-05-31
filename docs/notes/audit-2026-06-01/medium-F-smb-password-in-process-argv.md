# SMB password leaks into the process argument list on the CLI fallback paths

**Severity:** medium
**Lens:** F — Security
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/network/smb_smbutil.rs:289` (macOS, via `build_smbutil_url` at `:216-229`)
`apps/desktop/src-tauri/src/network/smb_smbclient.rs:32` (Linux)

## What
When the pure-Rust `smb2` share enumeration fails (older Samba/NAS servers) and Cmdr falls back to the
`smbutil` (macOS) or `smbclient` (Linux) CLI with stored credentials, the user's SMB password is passed
as a command-line argument: `smb://user:password@host` for `smbutil`, and `-U user%password` for
`smbclient`. Command-line arguments are world-readable on both platforms via `ps aux` /
`/proc/<pid>/cmdline`, so any other local process (any local user on a shared box) can read the cleartext
password for the lifetime of the child process.

## Why it matters
A non-privileged local process polling `ps` during a share-listing operation captures the SMB credential
in cleartext. The keychain/secret-service storage that protects the password at rest is bypassed the
moment it's handed to the fallback CLI. On macOS the primary mount path (`NetFSMountURLSync`) deliberately
avoids this — `network/CLAUDE.md` calls out "credentials passed via secure API, not exposed in process
list" — so the fallback is an inconsistency that silently downgrades that guarantee. The window is short
(the CLI runs briefly) and only the fallback path is affected, which is why this is medium, not high.

## Evidence
```rust
// smb_smbutil.rs build_smbutil_url
Some((username, password)) => {
    let encoded_username = urlencoding::encode(username);
    let encoded_password = urlencoding::encode(password);
    let url = if port == 445 {
        format!("//{}:{}@{}", encoded_username, encoded_password, host)   // <-- cleartext pw in url
    ...
    let safe_url = ... format!("//{}:***@{}", encoded_username, host) ... // redacted form exists, only used for logs
    (url, safe_url)
}
// run_smbutil_view: the *real* url (with pw) goes into argv
cmd.arg(&url_owned).output()                                              // smb_smbutil.rs:289
```
```rust
// smb_smbclient.rs:30-33
Some((username, password)) => {
    cmd.arg("-U").arg(format!("{}%{}", username, password));             // cleartext pw in argv
}
```

## Suggested fix
Feed the password to both CLIs over a channel that doesn't land in argv. `smbclient` reads credentials
from an `--authentication-file` (or the `PASSWD`/`USER` env vars / `-A file`); write a 0o600 temp file (or
set the env var on the child only) and pass `-A <file>` instead of `-U user%pass`. `smbutil` reads the
password from the `SMB_PASSWORD`-style prompt / `~/.nsmbrc` or via stdin in some flows; if no argv-free
channel exists, prefer the env-var/authfile route where supported, or document the residual exposure
explicitly. Keep the existing `safe_url` for logging. Both fallbacks are alpha/rare, so this can be a
fast-follow rather than a launch blocker.

## Notes
Logging is already handled correctly: only `safe_url` (with `***`) is logged, and the redactor scrubs
`url_userinfo` shapes from any line that does reach a bundle. The leak here is the OS process table, not
Cmdr's own logs.
