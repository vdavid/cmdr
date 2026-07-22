# Stubs module

Compilation stubs for platform-specific modules on platforms that aren't macOS and aren't Linux. Never compiled on
macOS. Most sub-modules are gated `#[cfg(not(target_os = "linux"))]` since Linux has real implementations (volumes,
MTP, network, accent color, permissions); `text_size.rs` is gated `#[cfg(not(target_os = "macos"))]` because the
Accessibility text-size signal is macOS-only.

## Module map

- **`mod.rs`**: declares the sub-modules with the cfg gates above.
- **`mtp.rs`**, **`network.rs`**, **`permissions.rs`**, **`accent_color.rs`**, **`volumes.rs`**: non-macOS/non-Linux
  stubs returning empty/success values; types mirror the macOS shapes for JSON compatibility.
- **`text_size.rs`**: non-macOS `get_system_text_size_multiplier` returns `1.0`. The in-app `appearance.textSize`
  slider still works on every platform.

Per-stub behavior is cataloged in `DETAILS.md`.

## Invariants

- **JSON shape must match macOS.** The frontend doesn't branch on platform; it calls the same commands everywhere. If a
  macOS type gains or loses a field, align the corresponding stub type by hand. Most fragile in `mtp.rs`, which keeps a
  local `FileEntry` duplicating `crate::file_system::FileEntry`.
- **`mtp.rs` deliberately doesn't import `crate::file_system`**: that keeps the stub dependency-free and fast to compile
  on targets where the real module's platform-specific deps may not build. The duplicated `FileEntry` is intentional.
- **Stubs return hardcoded success** (empty vecs, `true` for permissions), never errors: the frontend doesn't branch on
  platform, so an error would trigger error UI; empty/success makes the feature silently not appear, the correct UX for
  "not available here."
- **❌ Don't add logic here.** Stubs stay trivial; real functionality belongs in the platform-specific subsystem
  modules. `libc` (`volumes.rs` `statvfs`) and `dirs` (`volumes.rs` `home_dir`) are the only non-Tauri deps.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
