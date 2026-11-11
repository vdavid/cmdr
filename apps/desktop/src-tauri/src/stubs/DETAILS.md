# Stubs module details

Per-stub behavior and rationale. `CLAUDE.md` holds the invariants; the catalog below is reference.

## Per-stub behavior (non-macOS, non-Linux unless noted)

- No stub for anything with a platform seam: network, MTP, permissions, the system look (accent color, display
  options, Liquid Glass, text size), the clipboard, drag, Quick Look, and the file actions all compile everywhere, and
  each seam's `windows.rs` holds the answer a stub here used to give.
- **`volumes.rs`**: returns root `/`, Home, and existing Desktop/Documents/Downloads; `get_volume_space` uses
  `libc::statvfs`; `start_volume_watcher` is a no-op.

## Decisions

### Hardcoded success values rather than errors

The frontend doesn't branch on platform; it calls the same commands everywhere. Returning errors would trigger
error-handling UI (toasts, retry prompts) on unsupported platforms. Returning empty/success makes the feature silently
not appear, which is the correct UX for "not available on this platform."

### `#[allow(dead_code)]` on no-op functions

Some no-op functions (for example `start_volume_watcher`) exist only to keep the API surface symmetric with macOS for
internal callers, so they carry `#[allow(dead_code)]`.
