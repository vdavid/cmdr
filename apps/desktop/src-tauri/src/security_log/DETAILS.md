# Security event log: details

Issue #456. Must-knows in `CLAUDE.md`; this is the why and the catalog.

## The file

- `security-events.jsonl` in `config::resolved_app_data_dir`, one JSON object per line:
  `{"v":1,"at":"<UTC ISO 8601, ms>","event":{"kind":"<token>", …typed fields}}`. The stamp is taken at `record` time,
  so lines stay in order however late the writer gets to them.
- Rotates past 1 MiB into `security-events.jsonl.1`, keeping one rotated file: at most about 2 MiB, which at a handful
  of events a day is years. No age limit (decided 2026-10-16): the size cap and the opt-out cover it.
- Plain text on purpose: "Show in Finder" (`reveal_target`: the file, or the data dir while there's no file yet) hands
  it to whatever the person reads text with.
- Not in the log dir, so error reports (`error_reporter/bundle_builder.rs` collects `logging::log_dir()`) never carry
  it. In the data dir, so the wipe removes it with the rest (`data_wipe/plan.rs` names it in a test).

## Event catalog and hook points

- `cloud_ai_consent_given` / `cloud_ai_consent_withdrawn`: `ai/cloud_consent.rs`, `accept_on` / `revoke_on`, only on a
  real change (a repeat accept, or a held "no" retried against a store that already cleared it, notes nothing). A held
  "no" is noted when the store finally takes it.
- `mcp_server_turned_on` / `mcp_server_turned_off` (`by`: `user` | `launch`): `commands/mcp.rs` `toggle_event` (only a
  real start or stop) and `mcp/server.rs` `start_mcp_server_background`, so a log that starts mid-session still says
  the server runs.
- `mcp_rejections_started` / `mcp_rejections_ended` (`reason`: `missing_token` | `wrong_token` |
  `no_token_configured` | `foreign_origin`): `mcp/auth.rs` `validate_token` and `validate_origin`. Never the token,
  the origin, or the request.
- `managed_policy_applied` (`trigger`: `launch` | `change`, `keys_before`, `keys_after`): `managed_policy/cache.rs`
  `init` (when a policy is present) and `apply_change`. Key names from `managed_policy::keys::restricting`, never
  values (an allowlist value can name an internal host).
- `update_installed`, `update_refused_by_policy` (`reason`: `updates_disabled` | `above_ceiling`),
  `update_signature_invalid` (`check`: `minisign` | `code_signature`): `../updater/security_events.rs`, one-line hooks in
  `updater/mod.rs`. The installed version is the one the bundle's own `Info.plist` names (`installer::install` answers
  it), not the one the unsigned manifest offered.
- `smb_signature_rejections_started` / `_ended` (`volume_id`): `cmdr-smb` reports each refused reply
  (`smb2::ErrorKind::SignatureInvalid`) through the volume host's `security_refusal` seam;
  `events/volume_mapping.rs` hands it to `note_smb_signature_rejection`, one episode per volume. A guest session offered
  for a named login (`GuestSessionRefused`) isn't recorded: it reads as a wrong password, and the pane names it
  (`VolumeConnection::GuestSessionRefused`).
- `recording_turned_on` / `recording_turned_off` (`by`: `user` | `organization`): the opt-out itself.
- `state_file_recovered` (`file`: `settings` | `shortcuts` | `license` | `app_status` | `operation_log` |
  `chat_history` | `favorites` | `go_to_path_history` | `search_history` | `selection_history` |
  `multi_rename_presets` | `multi_rename_last_settings` | `multi_rename_field_history` | `launch_days` |
  `credentials`):
  `../state_recovery/mod.rs` `report`, once per file Cmdr couldn't read and started fresh. Never the path. Its notice
  rides `state_recovery`'s own queue, since it names where the copy went.

- `content_refusals_started` / `content_refusals_ended` (`tool`: `inspect_file` | `image_facts` | `search_photos` |
  `multi_rename`, `reason`: `credentials` | `browser_data` | `message_store` | `cmdr_data` | `private_key`):
  `../tools/sensitive.rs` `note_refusal`, one episode per tool and reason. Never the path, or the log would list
  what an injected prompt went looking for.

Not recorded: reads of file data that went ahead. The operation log covers modifications; journaling every read would cost privacy and
performance for little security value. Also not yet: `installer::refuse_unless_newer` (an older signed release offered,
which a rollback attempt looks like), because its refusal is still a `String` inside `UpdateInstallError::Failed`.

## Opt-out

- Recording is on when the person's switch is on OR the organization requires it (`DisableSecurityLogOptOut`).
  `Recorder` holds both as atomics: the switch from `settings::load_security_log_enabled` at `init` (anything but a
  real JSON `false` reads as on), then live from `settings-applier.ts` through `set_security_log_enabled`; the policy
  from `managed_policy::current()` at `init`, then through `on_policy_applied`.
- Transitions: off writes "turned off" as its last line; on writes "turned on" as its first. A policy that starts
  requiring the log turns it on before recording itself; one that stops requiring it records itself first and only
  then turns off (when the switch is off). Same-value pushes (the applier re-pushes at startup) are no change.
- `mcpSettable: false` in the registry: an MCP client must not turn off the record of what MCP clients do, in either
  direction. The policy lock is `Fixed { true }` in `managed_policy/locked.rs`, so the switch shows as managed.
- Turning off keeps the file (decided 2026-10-16). "Delete all Cmdr data" removes it.

## Episodes

`episodes.rs` holds a generic `Episodes<K>` over an injected clock. An episode for a key starts with the first
rejection, counts every later one, and ends after 10 minutes without one (`QUIET`). A rejection after a quiet spell the
closer hasn't swept yet ends the old episode before starting the new one, so two never merge.

- **Why not end on a successful request**, as the issue suggested: a working client and a stale one polling side by
  side would open and close an episode on every alternation, which is the flood this exists to stop.
- The closer is a thread that starts with the first open episode, ticks every minute, and exits once nothing is open,
  so an idle app runs no thread for it. `on_exit` ends every open episode so its count lands.
- MCP keys by reason (a local process can't be told apart reliably), SMB by volume id, content refusals by tool and
  reason.

## Consumers

`subscribe` adds a consumer that hears every event, on the recording thread, before the file does, as
`Heard::Recorded`. Every rejection after an episode's first also reaches subscribers as that episode's started event,
`Heard::Repeat`, so a consumer can count them; the file never sees those. The one consumer is the in-app notice gate
(`../security_notices/CLAUDE.md`), which decides per variant whether to tell the person. Subscribers ignore the opt-out:
the switch is about the record, not about being told.

## Testing

`testing::capture_events()` captures what THIS thread records (and runs its own episodes; `repeats()` lists the
repeats subscribers would have heard), so hook tests elsewhere
assert the exact events without touching the file or racing parallel tests. A `#[tokio::test]` runs its tasks on the
test thread, so it reaches them too; a hook that records from a `spawn_blocking` thread is tested through the pure
helper it calls (`accept_on`, `toggle_event`, `../updater/security_events.rs`). The opt-out logic is tested on a private
`Recorder` with a list for a file.
