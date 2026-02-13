# ADR 012: Custom auth UI with Keychain integration

## Status

Accepted

## Context

SMB shares may require authentication. We considered two approaches for handling credentials:

1. **System dialog (let macOS handle)**: Call mount APIs without credentials and let macOS show its standard login sheet
2. **Custom UI with Security.framework**: Build our own login form and integrate with Keychain directly

Key requirements:

- Seamless UX that keeps the user in the app's context
- Support for guest access vs. authenticated access choice
- Credential storage that persists across sessions
- Smart behavior based on connection history (see known shares store)

## Decision

Build a custom login UI with direct Keychain integration via Security.framework.

## Consequences

### Positive

- **Full UX control**: Login form appears in-pane, matching our app's design
- **Smart defaults**: Can pre-fill username from connection history
- **Contextual messaging**: Can inform user when auth options change (was guest, now can auth)
- **Guest/credentials toggle**: Can offer choice when both are available
- **Consistent with NetFSMountURLAsync**: Custom UI complements our async mounting approach

### Negative

- **More implementation work**: Need to build login form component and Keychain integration
- **Security responsibility**: We handle credentials directly (though Keychain does the heavy lifting)
- **Testing complexity**: Need to test various auth scenarios

### Notes

- Use `security-framework` crate for Keychain access (simpler than raw Security.framework FFI)
- Credentials stored under "Cmdr" service name, visible in Keychain Access.app
- Passwords never stored in our settings file—only in Keychain
- See [authentication.md](../features/network-smb/authentication.md) for implementation details
