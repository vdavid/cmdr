# Security notices: details

Issue #457. Must-knows in `CLAUDE.md`.

## Flow

1. A source records through the security log: `record(SecurityEvent::UpdateSignatureInvalid { .. })`, or
   `note_mcp_rejection` / `note_smb_signature_rejection`, which coalesce into episodes for the file
   (`../security_log/DETAILS.md` § Episodes).
2. The log hands every event to its subscribers on the recording thread: a recorded event as `Heard::Recorded`, and
   every later rejection inside an open episode as that episode's started event with `Heard::Repeat` (the file never sees
   repeats; it gets the count when the episode ends).
3. `on_event` maps the event through `notice_for`. `None` ends it there.
4. `NoticeGate::note` answers a `Raise { notice, count }` or holds it. A held count schedules the flush thread, which
   sleeps until the earliest window ends, raises what's due, and exits once nothing is held (an idle app runs no thread
   for this).
5. `SecurityNoticeRaised { notice, count }` goes to every window; only the main window listens
   (`apps/desktop/src/lib/security-notices/`), and the same toast id per notice updates the toast in place.

## Per case

- **MCP, missing or wrong token**: no notice. The token is fresh on every start, so an AI tool holding the previous
  run's token is the usual cause, and any local process can read the token file anyway (David, 2026-10-16). The record
  has it.
- **MCP, refused web page `Origin`**: `McpForeignOrigin`, keyed by kind alone (a local process can't be identified
  reliably). No innocent setup story, so it's a persistent warning. The origin stays in the log.
- **SMB, reply refused for its signature**: `SmbSignatureRefused { volume_id }`, one per volume. The transfer that hit it
  also stops with its own typed reason (`WriteOperationError::ReplySignatureRefused`), and `cmdr-smb` drops the session
  so the next dial is a fresh, freshly signed one (`crates/cmdr-smb/DETAILS.md`).
- **SMB, guest session offered for a named login**: no notice. It happens at connect, so it's a pane state
  (`VolumeConnection::GuestSessionRefused`), also for a background reconnect.
- **Update, signature doesn't hold** (minisign on the archive, or the Developer ID on the bundle): `UpdateNotGenuine
  { version, check }`, one per offered version. The updater also stops fetching that version for the session
  (`../updater/DETAILS.md` § "A version that isn't genuine"). No report is sent on its own: the toast offers one.
- **Update, older signed release offered** (`installer::refuse_unless_newer`): no notice; it already stops the install.

## Decisions

- **The gate is backend-side** so a misbehaving client can't spam the IPC bridge or the screen; the frontend never sees
  more than one raise per notice per minute.
- **Trailing raise.** Without the flush, a burst that ends inside the first minute would leave the toast saying "once"
  forever. The flush makes the last number true within a minute.
- **Repeats ride the subscriber, not new `SecurityEvent` variants.** A variant per rejection would be a file-format token
  that the file must never write; `Heard::Repeat` keeps the vocabulary the record's.
