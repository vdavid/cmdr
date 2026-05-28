# `validate_destination_not_inside_source` silently falls back to non-canonical paths

**Severity:** medium **Lens:** A — Data safety **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:67-87`

## What

The "is destination inside source?" guard canonicalizes both paths to defeat `..` segments and symlinks, but on
canonicalize failure it falls back to the **non-canonical** input path. Any error in canonicalization (broken symlink in
the chain, transient `EACCES` on a parent, network mount blip) silently drops the safety check down to a naive
`starts_with` against the raw inputs.

## Why it matters

The check exists to prevent recursive self-copy / self-move into a sub-directory of the source, which would normally
explode into infinite recursion or wedge the operation halfway through. With the fallback active, an attacker (or a
confused user with symlinks) can craft a `dest` that lexically doesn't start with `source` but canonically does — for
example, `dest = /src/sub` where `/src/sub` is a symlink elsewhere that the canonicalize call would have resolved. The
validator passes, the copy starts walking, the destination is inside the source after all, and the scan never terminates
/ the user's source dir gets duplicated into itself until disk is full.

The "real" failure modes (`EACCES` on a parent during the up-walk, network mount stall, dangling symlink in the
destination's chain) are exactly the cases the file manager is supposed to handle gracefully — they shouldn't silently
weaken a data-safety check.

## Evidence

```rust
pub(crate) fn validate_destination_not_inside_source(
    sources: &[PathBuf],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    // Canonicalize destination to resolve symlinks and ".." segments that could
    // bypass a naive starts_with check (like /foo/bar/../foo/sub → /foo/sub)
    let canonical_dest = destination.canonicalize().unwrap_or_else(|_| destination.to_path_buf());

    for source in sources {
        if source.is_dir() {
            let canonical_source = source.canonicalize().unwrap_or_else(|_| source.to_path_buf());
            if canonical_dest.starts_with(&canonical_source) {
                return Err(WriteOperationError::DestinationInsideSource { ... });
            }
        }
    }
    Ok(())
}
```

## Suggested fix

Don't silently swallow canonicalize errors here. Two paths, depending on which failed:

- If `destination.canonicalize()` fails: the destination doesn't exist yet (`ENOENT` is the common case) — canonicalize
  its parent instead, then re-append the trailing component. Only fall back to raw `destination` if the parent also
  can't be canonicalized.
- If `source.canonicalize()` fails: this is the source being moved; if we can't resolve it, we can't trust the check.
  Return an `IoError` so the validator fails closed instead of fails open. The user gets a clean error message; the
  alternative is an unbounded copy.

The "destination doesn't exist yet" path is the legitimate canonicalize-fails case; everything else should fail the
operation.

## Notes

The neighboring `validate_destination` at `helpers.rs:38-52` uses `destination.exists()` (which follows symlinks) and
`destination.is_dir()`. A dangling symlink at `destination` would fail `.exists()` and return `SourceNotFound` (the
right outcome here, by coincidence). But the symlink-traversal asymmetry between these two validators is worth a
consistency pass — the volume CLAUDE.md explicitly says `LocalPosixVolume::exists` uses `symlink_metadata`; the
write-operations helpers should match.
