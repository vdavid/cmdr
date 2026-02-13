# ADR 013: Use smb-rs for SMB share enumeration

## Status

Accepted

## Context

When a user browses to a network host, we need to enumerate available SMB shares. We considered several approaches:

1. **`smbutil view`**: Shell out to macOS's built-in command
2. **`pavao` crate**: Rust bindings to libsmbclient (C library)
3. **`smb` crate (smb-rs)**: Pure Rust SMB2/3 implementation

Key requirements:

- License compatible with AGPL and future dual-licensing for enterprise sales
- No C dependency complications for distribution
- Async-friendly for good UX
- Cross-platform potential (Windows, Linux later)

## Decision

Use the `smb` crate (smb-rs) for SMB share enumeration.

## Consequences

### Positive

- **MIT license**: Fully compatible with AGPL, allows dual-licensing for enterprise
- **Pure Rust**: No C dependencies, no distribution complexity, compiles cleanly
- **Async-native**: Built on tokio, fits perfectly with our async architecture
- **Modern protocol**: Supports SMB 2.X and 3.X with encryption/compression
- **Cross-platform**: Works on Windows, Linux, macOS without platform-specific code

### Negative

- **Newer crate**: Less battle-tested than libsmbclient (Samba's C library)
- **Potential edge cases**: Some quirky servers might not work; may need fallback
- **API learning curve**: Need to understand smb-rs's specific API patterns

### Alternatives rejected

**`smbutil view` (CLI)**:

- ✅ Simple, uses macOS built-in
- ❌ Spawns process, fragile text parsing, no streaming

**`pavao` (libsmbclient wrapper)**:

- ✅ Battle-tested SMB implementation
- ❌ **GPLv3 license** - incompatible with dual-licensing, prevents enterprise sales
- ❌ Requires bundling C library

### Notes

- Fallback to `smbutil` available for edge cases where smb-rs fails
- Validation spike recommended before full implementation to test with real servers
- See [share-listing.md](../features/network-smb/share-listing.md) for implementation details
