# ADR 019: Keep exacl for ACL copy

## Status

Accepted

## Summary

We evaluated whether to replace the `exacl` crate with custom FFI bindings for copying ACLs during chunked file copies to network filesystems. We decided to keep `exacl` because it adds zero new transitive dependencies, provides cross-platform support (macOS/Linux/FreeBSD), and solves the problem well despite being unmaintained since early 2024.

## Context, problem, solution

### Context

The chunked copy implementation (`chunked_copy.rs`) exists because macOS's `copyfile()` ignores cancellation on network filesystems. Since chunked copy bypasses `copyfile()`, it must manually copy all metadata including ACLs.

The primary copy path (`macos_copy.rs`) uses `copyfile()` with `COPYFILE_ALL` which already preserves ACLs. The `exacl` dependency is only needed for the chunked copy fallback path.

### Problem

The `exacl` crate hasn't had commits for ~2 years (last release Feb 2024). We needed to decide whether to:
1. Keep the dependency despite apparent abandonment
2. Replace it with minimal custom FFI bindings (~80 lines for opaque ACL copy)

### Possible solutions considered

**Custom FFI bindings**: Write ~80 lines of Rust wrapping `acl_get_file()`, `acl_set_file()`, and `acl_free()`. This would give us full control and remove the external dependency.

Tradeoffs:
- Pro: We own the code, no external dependency risk
- Pro: Simpler (only what we need)
- Con: macOS-only (Linux/FreeBSD would need separate implementation)
- Con: If we ever want ACL UI features (view/edit ACLs), we'd need to rewrite or bring back exacl

### Solution

Keep `exacl` because:

1. **Zero new transitive dependencies**: All of exacl's dependencies (`bitflags`, `log`, `scopeguard`, `uuid`) are already in our dependency tree from other crates.

2. **Cross-platform support**: Works on macOS, Linux, and FreeBSD out of the box. While chunked copy is currently macOS-only, this keeps options open.

3. **Future flexibility**: If we ever want to display or edit ACLs in the UI, exacl provides full ACL parsing/manipulation, not just opaque copy.

4. **Healthy adoption**: 273K downloads/month indicates real-world usage and implicit validation.

5. **Stable, feature-complete**: The lack of commits may simply mean the library is "done" - ACL APIs are stable and don't change.

6. **Low risk**: Our usage is best-effort with graceful fallback (debug logging on failure, no hard errors). If exacl ever breaks, files still copy - they just lose ACLs.

## Consequences

### Positive

- No code to write or maintain for ACL handling
- Cross-platform ACL support if we extend chunked copy to Linux
- Option to add ACL UI features later without new dependencies

### Negative

- Dependency on an externally maintained crate that may not receive security updates
- If a critical bug is found, we'd need to fork or replace

### Notes

- exacl is MIT licensed (compatible with our BSL)
- 3 open issues, none security-related; the O(n^2) issue doesn't affect our use case (we just read/write, not build entry-by-entry)
- Alternative `posix-acl` crate is Linux-only, not viable for macOS
