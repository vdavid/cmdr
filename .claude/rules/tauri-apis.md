# Tauri command and capability rules

- ❌ Tauri APIs fail silently without permission. When you call a new Tauri API from a window (`setMinSize`, `setTitle`,
  plugin commands, anything new), add the matching permission to that window's capability file in
  `src-tauri/capabilities/{default,settings,viewer}.json`, and `await` the call in try/catch so failures surface instead
  of looking like a broken feature. See `src-tauri/capabilities/CLAUDE.md`.
- A new filesystem-touching Tauri command must be `async` with `blocking_with_timeout` (network mounts block syscalls
  for 30-120s), and you should check `docs/architecture.md` § Platform constraints.
- ❌ Don't read TCC-protected paths (`~/Downloads`, `~/Documents`, iCloud, etc.) or call `NSWorkspace` icon /
  LaunchServices APIs at launch without the FDA gate: they stack macOS TCC popups during onboarding (we hit 5-10 once).
  Use `crate::fda_gate::is_fda_pending_runtime()`. See `fda_gate.rs` and `lib/onboarding/CLAUDE.md`.
