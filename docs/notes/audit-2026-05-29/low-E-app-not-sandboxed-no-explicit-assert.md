# App is unsandboxed but nothing asserts or documents the contract

**Severity:** low
**Lens:** E — macOS pitfalls
**Confidence:** high

## Location

- `apps/desktop/src-tauri/Entitlements.plist` (production)
- `apps/desktop/src-tauri/Info.plist`
- `apps/desktop/src-tauri/tauri.conf.json`

## What

Cmdr is signed with Developer ID (`signingIdentity: "Developer ID Application: Rymdskottkarra AB"`) and ships outside the Mac App Store. `Entitlements.plist` confirms this:

```xml
<key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
<key>com.apple.security.cs.disable-library-validation</key><true/>
```

These are hardened-runtime relaxations, **not** App Sandbox entitlements. There's no `com.apple.security.app-sandbox` key, no per-container entitlements (no `com.apple.security.network.client`, no `com.apple.security.files.user-selected.read-write`, etc.). This is correct for a Developer ID file manager — App Sandbox would break the entire point of the app (arbitrary FS access).

The code throughout assumes non-sandboxed behavior:

- `commands/eject.rs` shells out to `diskutil`.
- `network/mount.rs` calls `NetFSMountURLSync` against `/Volumes/`.
- `updater/installer.rs` writes into `/Applications/Cmdr.app` and escalates to admin.
- `restricted_paths/tcc_paths.rs` walks paths anywhere on disk.
- `mtp::macos_workaround::ensure_ptpcamerad_enabled` toggles a system daemon via `launchctl`.

Each of these would fail-closed in a sandboxed build, but none of them assert non-sandboxedness or document it as a precondition.

## Why it matters

If Cmdr ever needs to publish a sandboxed variant (Mac App Store, enterprise managed distribution, or a stripped-down "Cmdr Lite" with read-only browsing for the App Store), there's no audit point for catching what would break. The current entitlements file is the only signal that "this is the unsandboxed build," and it's editable without a code change.

This is genuinely low-impact today: there's no roadmap for a sandboxed build. But it's the kind of constraint that should be captured in `docs/architecture.md` or a top-level `CLAUDE.md` so a future agent doesn't accidentally try to ship a sandboxed variant and burn a week discovering the constraints by failure.

## Evidence

- `Entitlements.plist` (10 lines, two keys, no sandbox key).
- `tauri.conf.json` has no `bundle.macOS.appSandbox` setting; the Tauri schema's default is unsandboxed.
- No `cfg!(target_env = "sandboxed")` or similar checks anywhere.

## Suggested fix

Document in `docs/architecture.md` § "Platform constraints" (or wherever fits):

> Cmdr ships unsandboxed (Developer ID, hardened runtime only). Code may freely:
> - Shell out (`diskutil`, `launchctl`, `osascript`, `rsync`)
> - Read/write arbitrary FS paths the user can access (subject to TCC)
> - Call private `NSWorkspace` / LaunchServices APIs
> - Spawn child processes (llama-server)
>
> If a sandboxed build is ever needed, the following subsystems would need rework: updater, eject, MTP daemon-toggle, network mount, AI subprocess.

No code change required.

## Notes

Confidence high; this is a documentation gap, not a defect. Flagging it because future-self / future-agents will appreciate having the constraint named.
