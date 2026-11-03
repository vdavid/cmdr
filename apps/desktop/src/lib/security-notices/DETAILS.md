# Security notices (frontend): details

Must-knows in `CLAUDE.md`; the backend half (which events notify, the rate limit) is
`apps/desktop/src-tauri/src/security_notices/DETAILS.md`.

## Per kind

- **`mcpForeignOrigin`**: warn, persistent. "MCP settings" opens Settings › AI › MCP server
  (`openSettingsWindow('security-notice-toast', …)`) and closes the toast; Dismiss closes it for the session. A web page
  has no innocent setup story, so it stays up until the person answers it.
- **`smbSignatureRefused`**: warn, persistent, one per volume. The title names the share from the volume list
  (`getVolumes`), with a nameless sentence once the share is gone from it. Reconnect calls `reconnectVolume` (the
  backend already dropped the refused session, so this builds a fresh signed one); Disconnect calls
  `disconnectSmbVolume`. Both close the toast without marking it dismissed. The transfer that hit it shows its own
  stopped reason (`reply_signature_refused` in `#lib/file-operations/transfer/transfer-error-messages.ts`).
- **`updateNotGenuine`**: error, persistent, one per offered version, with the report offer on: a typed
  `updateNotGenuine` incident (`#lib/error-reporter/report-incident.ts`) carrying the version and which signature didn't
  hold, never the tarball or a path. "Open getcmdr.com" opens the website. Nothing is sent unless the person sends it.

## Count line

Shown from the second refusal on: "{count} times since you opened Cmdr". The backend's count is per session and per
notice, so it reads the same however many raises it took.

## Copy

English only, in `securityNotices.*`. The drafts are #457's, including "please tell David" in the update body; David
reviews them in the usual pass.
