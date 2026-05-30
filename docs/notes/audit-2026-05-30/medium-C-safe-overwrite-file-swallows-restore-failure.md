# `safe_overwrite_file` silently swallows a failed restore-of-original

**Severity:** medium
**Lens:** C — Error handling discipline (data-protection path)
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:877-885` (vs. its sibling `safe_overwrite_dir` at `:983-990`).

## What
In `safe_overwrite_file`, after the original destination is renamed aside (`dest` → `aside_path`) and the temp→dest finalize rename fails, the recovery renames the aside back to `dest` via `let _ = fs::rename(&aside_path, dest);` — the error is discarded with no log. The error returned to the user is the *finalize* error, which says nothing about whether the original was restored. The byte-identical situation in `safe_overwrite_dir` is handled correctly: it logs the restore failure via `crate::log_error!`.

## Why it matters
This is the data-protection path (AGENTS.md principle #4). If the restore rename also fails — parent went read-only mid-operation, the aside path now collides, the disk filled — the user's original file is left orphaned under a `.cmdr-temp-<uuid>` name and `dest` is gone, with zero diagnostic trail in the logs or error-report bundle. The user sees a generic "couldn't finalize" error and has no idea their original survives under a hidden temp name they could recover by hand. The two safe-overwrite helpers should behave consistently on the recovery path.

## Evidence
```rust
// safe_overwrite_file (helpers.rs:877-885)
if let Err(e) = fs::rename(&temp_path, dest) {
    let _ = fs::rename(&aside_path, dest);   // ← restore failure swallowed; no log
    let _ = fs::remove_file(&temp_path);
    return Err(WriteOperationError::IoError { /* finalize error, not restore */ });
}

// safe_overwrite_dir (helpers.rs:983-990) does it right:
if let Err(restore_err) = fs::rename(&aside_path, dest) {
    crate::log_error!("safe_overwrite_dir: failed to restore aside {} -> {}: {}", ...);
}
```

## Suggested fix
Mirror `safe_overwrite_dir`: replace the bare `let _ =` with an `if let Err(restore_err) = ...` that logs the restore failure under a `target: "write_durability"`-style scope, and ideally surface "your original is saved as `<aside_path>`" in the error so the user can recover. This is a logging/diagnostics change, not a behavior change — flagging for the maintainer's decision per the no-proactive-fixes rule.

## Notes
The C-lens sweep found Cmdr's panic discipline otherwise strong (no `panic!`/`unwrap` crash-class bugs in live code; banned `eprintln!`/error-string-matching rules respected). This is the one swallowed error on a path where the swallowed value genuinely matters for the user's data.
