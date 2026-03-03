# Tauri capabilities

Each window has its own capability file controlling which Tauri APIs it can call.
When adding a Tauri API call to a window (setFocus, setTitle, show, etc.),
you must add the corresponding permission here or the call will be silently
rejected at runtime with a "not allowed" error.

- `default.json` — main window
- `desktop.json` — desktop-wide permissions
- `settings.json` — settings window
- `viewer.json` — file viewer windows

Check the [Tauri permissions reference](https://tauri.app/security/permissions/)
for available permission identifiers.

## Key decisions

**Decision**: One capability file per window type, not one global file.
**Why**: Tauri's capability system is the security boundary between webview code and native APIs. Each window type (main, settings, viewer) has a different trust level. The main window needs filesystem access, drag-and-drop, clipboard, and the updater. The settings window only needs event dispatch and theme control. The viewer only needs window management. Splitting by window prevents privilege escalation -- a compromised viewer webview can't invoke filesystem operations.

**Decision**: `viewer-*` wildcard pattern for viewer window capabilities.
**Why**: Viewer windows are created dynamically with labels like `viewer-0`, `viewer-1`, etc. A wildcard `"viewer-*"` in the `windows` array covers all of them without needing to pre-register each label. This is a Tauri feature specifically for dynamic window creation.

**Decision**: `desktop.json` is separate from `default.json` despite overlapping window targets.
**Why**: `desktop.json` holds desktop-platform-specific permissions (`window-state:default` for remembering window size/position). It's scoped to `["macOS", "windows", "linux"]` platforms. Keeping it separate from `default.json` makes the platform-specific vs. universal split clear and avoids accidentally applying desktop-only permissions to future non-desktop targets.

## Gotchas

**Gotcha**: Missing permissions fail silently at runtime.
**Why**: Tauri doesn't crash or warn visibly when a webview calls an API it lacks permission for. The call just returns a generic "not allowed" error. If a new Tauri API call (e.g., `setFocus`, `setTitle`) is added to a window's frontend code, the corresponding permission must be added here or it will silently fail. Check the browser console for "not allowed" errors.

**Gotcha**: `opener:allow-open-path` needs explicit glob patterns for hidden files.
**Why**: The default `opener:allow-open-path` permission doesn't match dotfiles. The `"**/*"` glob excludes files starting with `.`, so a separate `"**/.*"` pattern is required. Without it, opening hidden files from the file manager would silently fail.
