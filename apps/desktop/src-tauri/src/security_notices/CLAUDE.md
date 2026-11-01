# Security notices

Tells the person, in the moment, that Cmdr turned an access attempt away (#457, the CRA's Annex I Part I (2)(d)
"report on possible unauthorised access"). The security log (`../security_log/CLAUDE.md`) is the record; this is the
notice.

## Module map

- `mod.rs`: `AccessNotice` (what the person hears about, carrying its source), the `security-notice-raised` event,
  `notice_for` (which events notify), the `security_log` subscriber, and the flush thread.
- `gate.rs`: `NoticeGate`, the per-notice rate limit over an injected clock.
- Frontend: `apps/desktop/src/lib/security-notices/CLAUDE.md`.

## Must-knows

- **One event, one emitter.** This is a `security_log::subscribe`r: sources keep calling `security_log::record` /
  `note_*_rejection`. ❌ Never a second event enum or a call site that emits here directly.
- **`notice_for` decides who hears what, exhaustively.** Today: a refused web page `Origin`, an SMB reply refused for
  its signature, an update that isn't genuine, a launch in MCP direct mode. ❌ A missing or wrong MCP token stays record-only (David, 2026-10-16).
- **The gate rate-limits per source**: first raise at once, then at most one re-raise per minute, and a count that grew
  inside the window is raised when it ends. Repeats inside a log episode reach it as `Heard::Repeat`.
- **Dismissal is frontend-side** and only gates display; the backend keeps counting.
- ❌ Never put an origin, a path, or free text in an `AccessNotice`: it's toast content.

Flow, the per-case decisions, and why: `DETAILS.md`.
