# ADR 011: Use NetFSMountURLAsync for SMB mounting

## Status

Accepted

## Context

When a user selects an SMB share to browse, the app needs to mount it to the local filesystem. We considered two
approaches:

1. **`mount_smbfs` CLI**: Shell out to the built-in macOS command
2. **`NetFSMountURLAsync` API**: Use the native NetFS.framework async mounting API

Key requirements:

- Non-blocking operation (user can cancel or navigate away)
- Secure credential handling (no passwords in command-line args)
- Integration with Keychain for stored credentials
- Good error handling with user-friendly messages

## Decision

Use `NetFSMountURLAsync` from macOS's NetFS.framework.

## Consequences

### Positive

- **Non-blocking**: Async API allows the UI to remain responsive during mount
- **Secure**: Credentials passed via secure API, not exposed in process list
- **Native integration**: Works seamlessly with macOS Keychain and credential management
- **Better error handling**: Structured error codes instead of parsing stderr

### Negative

- **More complex implementation**: Need to create Rust bindings for NetFS.framework
- **NetFS not in objc2**: Manual FFI declarations required or use bindgen
- **macOS-specific**: Will need different implementation for Windows/Linux later

### Notes

- The `core-foundation` crate (already a dependency) provides `CFURLRef`, `CFStringRef` types needed for the API
- Completion callback will be bridged to Rust async via channels or similar mechanism
- See [mounting.md](../features/network-smb/mounting.md) for implementation details
