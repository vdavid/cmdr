# Migration from bincode v1 to bincode2 v2

**Date**: 2025-12-31

## Context

The original `bincode` crate became unmaintained in late 2024/early 2025 due to a doxxing and harassment incident. The
maintainer ceased all development and published v3.0.0 as a "tombstone" release that intentionally fails to compile.

## Decision

Migrated to `bincode2` v2, a maintained fork by Pravega, which provides:

- Drop-in replacement with minimal code changes
- Ongoing maintenance and security updates
- Compatible API with bincode v1
- Active development

## Alternatives considered

1. **Stay on bincode v1**: No work needed, but no security updates or bug fixes
2. **postcard**: Different API, would require more code changes
3. **rkyv**: Zero-copy deserialization, but more complex API and higher migration effort
4. **bincode v2.0.1**: Original but unmaintained version

## Changes made

1. **Cargo.toml**: Changed `bincode = "1"` to `bincode2 = "2"`
2. **src-tauri/src/font_metrics/mod.rs**: Updated two function calls:
    - `bincode::deserialize()` → `bincode2::deserialize()`
    - `bincode::serialize()` → `bincode2::serialize()`
3. **docs/features/font-metrics.md**: Updated documentation to mention bincode2

## Impact

- **Breaking change for cached data**: Existing font metrics cache files may need to be regenerated
- **Location**: `~/Library/Application Support/com.cmdr.app/font-metrics/`
- **Mitigation**: The app will automatically regenerate metrics if deserialization fails

## Testing

- ✅ All Rust tests pass (26/26)
- ✅ Build succeeds
- ✅ All checks pass (rustfmt, clippy, cargo-audit, cargo-deny, cargo-udeps)

## References

- bincode2 crate: https://crates.io/crates/bincode2
- Original bincode situation: https://crates.io/crates/bincode
