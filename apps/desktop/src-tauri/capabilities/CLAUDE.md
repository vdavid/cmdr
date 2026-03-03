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
