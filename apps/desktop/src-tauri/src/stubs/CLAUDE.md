# Stubs module

Compilation stubs for what has no platform seam yet, on platforms that aren't macOS and aren't Linux. Never compiled
on macOS. One sub-module is left, `volumes.rs`, gated `#[cfg(not(target_os = "linux"))]` since Linux has a real
implementation; it goes when the volumes seam lands. Everything else answers per OS from its own `platform/` seam
(`docs/guides/platform-seams.md`), whose Windows arm holds what a stub here used to.

## Module map

- **`mod.rs`**: declares `volumes` with the gate above.
- **`volumes.rs`**: the non-macOS/non-Linux volume list and space reads; types mirror the macOS shapes for JSON
  compatibility.

Per-stub behavior is cataloged in `DETAILS.md`.

## Invariants

- **JSON shape must match macOS.** The frontend doesn't branch on platform; it calls the same commands everywhere. If a
  macOS type gains or loses a field, align the corresponding stub type by hand.
- **Stubs import nothing from the real modules**: that keeps them dependency-free
  and fast to compile on targets where the real module's platform-specific deps may not build. The duplicated types are
  intentional.
- **Stubs return hardcoded success** (empty vecs, `true` for permissions), never errors: the frontend doesn't branch on
  platform, so an error would trigger error UI; empty/success makes the feature silently not appear, the correct UX for
  "not available here."
- **❌ Don't add logic here.** Stubs stay trivial; real functionality belongs in the platform-specific subsystem
  modules. `libc` (`volumes.rs` `statvfs`) and `dirs` (`volumes.rs` `home_dir`) are the only non-Tauri deps.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
