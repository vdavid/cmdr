# Updater AppleScript injection via bundle path

**Severity:** high **Lens:** F — Security **Confidence:** high

## Location

`apps/desktop/src-tauri/src/updater/installer.rs:256-275` (`sync_with_admin_privileges`)

## What

The admin-privilege update path builds an AppleScript by interpolating two filesystem paths into a single-quoted shell
command, then runs it via `osascript -e ... with administrator privileges`. The interpolated paths are wrapped in single
quotes only; nothing escapes embedded single quotes. If either path contains a `'` character, the shell escape breaks
and the remainder of the path becomes shell code running as root.

## Why it matters

The bundle path comes from `find_running_bundle()` which walks up from `current_exe()`. A user can drag `Cmdr.app` into
a folder whose name contains a single quote (for example `/Users/me/Don't Touch/`). The next update will prompt for the
admin password, the user types it (it's the standard macOS admin dialog), and the injected payload runs as root. Local
privilege escalation from "user can rename a folder" to "user has root."

In practice the trigger is benign (a folder name) and discovery is non-obvious, but the asymmetry — admin-only damage
from a path string an unprivileged user controls — is the textbook shape of a CVE-grade bug in a desktop app's
auto-updater.

## Evidence

```rust
fn sync_with_admin_privileges(staged_contents: &Path, bundle_contents: &Path) -> Result<(), String> {
    let src = format!("{}/", staged_contents.display()); // trailing slash for rsync
    let dest = format!("{}/", bundle_contents.display());
    let script = format!(
        "do shell script \"rsync -a --delete '{}' '{}'\" with administrator privileges",
        src, dest
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        ...
```

`staging` is the hardcoded `/tmp/cmdr-update-staging` so `src` is safe. `dest` derives from the running app's `.app`
parent, which is user-controlled (the user can move/rename Cmdr.app into any folder).

## Suggested fix

Don't build the AppleScript by string interpolation. Two layered defenses are appropriate:

1. Pass the paths in via `osascript`'s positional arguments rather than embedding them in the script string. The script
   becomes a constant
   `"do shell script \"rsync -a --delete '\" & quoted form of item 1 of arguments & \"' '\" & quoted form of item 2 of arguments & \"'\" with administrator privileges"`,
   and the paths arrive via `Command::new("osascript").arg("-e").arg(SCRIPT).arg(src).arg(dest)`. `quoted form of` is
   AppleScript's own shell-quoter and handles single quotes correctly.
2. Validate the bundle path before reaching this function: it must be canonicalized and contain only the characters
   macOS allows in `/Applications/<name>.app` (or its `~/Applications/` equivalent). Reject `'` outright and surface a
   friendly error.

Both together: defense in depth. (1) prevents the injection mechanism; (2) keeps the failure mode visible if something
else changes.

## Notes

The `osascript` call is the only privilege-escalating path in the updater. Everything else runs as the user. Related but
lower-priority: `is_permission_error` at `installer.rs:248-251` substring-matches on `"Permission denied"` /
`"Operation not permitted"`. On a localized macOS, the string match fails and the function returns `false`, so the
admin-escalation arm at line 49 never runs — the whole update silently falls through into the direct-write attempt which
then permanently fails on `/Applications`. This is a separate finding (`medium-C-string-match-permission-error.md`).
