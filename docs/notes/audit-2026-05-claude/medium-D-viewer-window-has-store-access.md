# Viewer windows can read/write the persistent store (license, settings)

**Severity:** medium **Lens:** D — IPC boundary **Confidence:** high

## Location

`apps/desktop/src-tauri/capabilities/viewer.json:14`

## What

Viewer windows are granted `store:default`, the full `tauri-plugin-store` permission set: read, write, save, get/set
arbitrary keys across every store the app uses. The viewer is a separate webview that opens files in read-only mode — it
has no legitimate reason to touch the persistent store. The capability split lives in CLAUDE.md as the security boundary
between webviews ("a compromised viewer webview can't invoke filesystem operations"); store access is exactly the kind
of escalation that boundary is supposed to prevent.

## Why it matters

A compromised viewer webview (loaded a malicious file, hypothetical XSS in the viewer's content renderer, etc.) can:

- Read `license.json` (license key, transaction id, organization name, short code) and exfiltrate it via opener URL or
  clipboard.
- Read `secrets.json` if file-backed (dev mode, Linux fallback, non-mac/non-linux platforms). That includes every SMB
  password and AI API key.
- Write to any store: tamper with the license cache to flip `Personal` → `Commercial`, flip `updates.errorReports` to
  opt the user into auto-send, change `developer.mcpEnabled` to expose the MCP server.

The MCP server caveat: the FE expects `developer.mcpEnabled` writes to go through the settings window (per
`mcp/CLAUDE.md` § "Live MCP control only works from the settings window"), but a viewer-side write succeeds silently and
persists across launches.

## Evidence

```json
{
  "identifier": "viewer",
  "description": "Capability for file viewer windows",
  "windows": ["viewer-*"],
  "permissions": [
    "core:window:allow-close",
    "core:window:allow-set-title",
    "core:window:allow-set-focus",
    "core:window:allow-get-all-windows",
    "core:event:default",
    "store:default", // ← grants full store access
    "clipboard-manager:allow-write-text",
    "dialog:allow-save"
  ]
}
```

Compared to settings or main windows, the viewer should be the most-restricted webview, not equally privileged.

## Suggested fix

Drop `store:default` entirely. If the viewer genuinely needs a specific store entry (font preferences, last-used view
mode), expose a typed Tauri command in the backend that reads only the allowed keys, and grant the viewer that command's
permission instead. Audit what the viewer actually reads from the store and either:

- Move those reads into a backend command surface (`get_viewer_preferences`, etc.), or
- Pass them in via the URL query string when the viewer window is opened (one-way snapshot, no write-back).

Either approach drops the `store:default` grant.

Same audit applies to `core:window:allow-get-all-windows` — viewer windows shouldn't need to enumerate the rest of the
app's windows. That permission lets a viewer probe whether the main window or settings window is open, which is
information it has no business with.

## Notes

The capabilities CLAUDE.md says the split is the "security boundary between webview code and native APIs" and exists
"specifically to prevent privilege escalation — a compromised viewer webview can't invoke filesystem operations." The
store access negates that promise for everything that lives in stores. The architecture intent is right; the file just
hasn't been narrowed down.
