# Settings (Rust)

Thin read-only settings loader used during Rust startup. The frontend owns all settings via `tauri-plugin-store`; this
module reads what was persisted so the backend can configure itself at launch.

Frontend counterpart: `apps/desktop/src/lib/settings/CLAUDE.md` owns the settings
store, the typed `getSetting` / `setSetting` wrapper, the Settings window UI, and the `settings-applier.ts` IPC pump that
satisfies the live-apply rule below.

## Module map

- `mod.rs`: re-exports `load_settings` from `loader`.
- `loader.rs`: `Settings` struct + `load_settings` (reads `settings.json`, falls back to `Default`); the `RestrictedWindowSettings`
  snapshot; the early-load helpers.
- `ai_gates.rs`: switches the AI gates read fresh (`askCmdr.enabled` via the policy `overlay`, held-"no" markers).
- `loader_tests.rs`: `loader`'s own tests, in a sibling file wired in with `#[path]` so they still reach its private
  parse helpers.

## Must-knows

- **Live-apply rule (MUST, no exceptions).** Every setting applies immediately without restart. When adding a setting the
  backend reads, also add (a) a Tauri command in `commands/settings.rs` that updates the relevant atomic/global
  (delegating to the owning subsystem's setter), and (b) a call from `settings-applier.ts` on `onSettingChange`.
  Startup-time seeding from `load_settings` stays (a sane initial value before any window opens), but every subsequent
  change is pushed via IPC. Restart-required is a bug. If a setting touches a TCP connection, thread pool, watcher, or
  server: reconnect / rebind / restart the thread / swap the pool, whatever it takes.
- **One-way read only.** This module never writes; all writes go through the frontend's settings store. The restricted
  window's write path (`persist_restricted_window_setting`) forwards to the main window's store rather than writing from
  Rust, keeping this invariant intact.
- **Dot-notation keys are literal, parsed manually** (`"listing.showHiddenFiles"` is one flat key). Serde can't express
  them as struct fields, so `parse_settings` reads them by hand. Don't switch to serde auto-derivation.
- **Direct file reading is intentional**: the MCP server, hidden-files filter, indexing, and crash reporter need their
  config before any frontend window loads.
- **`full_disk_access_choice` gates every launch-time job that could stack a TCC popup** (the recursive `/` scan, the
  Downloads watcher, `NSWorkspace` icon calls): `crate::fda_gate::is_fda_pending` holds the gate closed unless the OS
  grants FDA or the choice is `Deny`. ❌ Don't read the raw key; `read_fda_choice` also reports the
  present-but-unparseable case, which falls back to `Unanswered` and so defers all of that. See
  `apps/desktop/src/lib/onboarding/DETAILS.md` § "FDA gate".
- **`developer.mcpPort = 0` means "kernel picks an ephemeral port"** (the post-instance-isolation default); non-zero
  pins. See `mcp/DETAILS.md` § Server lifecycle and `docs/tooling/instance-isolation.md`.
- **`Settings` holds STORED values, not the organization's effective ones.** A gate a managed lock can close reads
  `settings.json` through `managed_policy::overlay` (`ai_gates.rs`, `analytics::send_permission`), ❌ never through
  `load_settings`. Key catalog and gate list: `managed_policy/DETAILS.md`.
- **The restricted-window snapshot is a typed allowlist**: a setting missing from `RestrictedWindowSettings` silently
  reads as its registry default in the viewer and the Transfers queue. `DETAILS.md` § Restricted-window snapshot.

The full `Settings` field list, the restricted-window snapshot, the early-load helpers, and the file format:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
