# Security notices (frontend)

The toast that tells the person Cmdr turned an access attempt away: a web page at the MCP server, a server reply refused
for its signature (on a session, or on a direct connection that never got in), an update that isn't a genuine Cmdr
release (#457). The backend decides what and how often: `apps/desktop/src-tauri/src/security_notices/CLAUDE.md`.

## Module map

- `security-notices.ts`: `startSecurityNotices` / `stopSecurityNotices` (main window only, wired in
  `routes/(main)/window-services.ts`), `announceSecurityNotice` (one switch over `AccessNotice['kind']` → level and
  report). `dismissed-notices.ts`: `securityNoticeId` and the session's dismissed set (its own module so the component
  reaches it without an import cycle).
- `SecurityNoticeToastContent.svelte`: the words and actions per kind, in the update toasts' look.
- Copy: `#lib/intl/messages/en/securityNotices.json`.

## Must-knows

- **One toast per source** (`securityNoticeId`): a re-raise with a grown count replaces it in place, ❌ never stacks.
  All share `SECURITY_NOTICE_GROUP`, at most three standing.
- **A closed notice stays quiet for the session**: the X (`onDismiss`) and the Dismiss button add to the set. An action
  that closes the toast (MCP settings, Reconnect, Disconnect) does NOT, so a repeat after a reconnect speaks again.
- **Main window only**, ❌ never subscribe from another window: the settings window would raise its own copy.
- ❌ No "Show security events" action, and ❌ never the origin or a path in the toast (David, 2026-10-16).
- **The update case's standing status says the same thing**: `#lib/updates` words `updateState.failure`
  `{ phase: 'notGenuine' }` like this toast's title.

Per-kind levels, actions, and the reasons: `DETAILS.md`.
