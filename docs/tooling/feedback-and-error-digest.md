# Reading in-app feedback and error reports

How an agent reads the **in-app "Send feedback" messages** and the **error-report bundles** straight from the app's own
stores. This is the data behind the `#feedback` and `#error-reports` Discord channels, but read from the source, not
Discord: those channels are private and denied to the community bot (see [discord.md](discord.md)), and the presigned
bundle links Discord posts expire after 7 days while the bundles themselves live 90 days in R2.

The `/feedback-and-error-digest-from-app` command (`.claude/commands/`) drives the whole flow; this doc is the access
recipe it points to. Read-only: never write or delete here without explicit approval (`no-external-actions`).

## Auth

Both stores are reached with the Cloudflare API token already on this machine:

- `wrangler` reads it from the `CLOUDFLARE_API_TOKEN` env var (already set; `npx wrangler whoami` confirms the account).
- For raw REST calls, load it from the sops secrets store: `TOKEN=$(secret CLOUDFLARE_API_TOKEN)`.

Cloudflare account id: `6a4433bf11c3cf86feda057f76f47991` (also printed by `wrangler whoami`; not a secret).

## Feedback (D1 table `feedback` in `cmdr-telemetry`)

One row per submission, written by `POST /feedback`. Columns (see `apps/api-server/migrations/0007_feedback.sql`):
`id, created_at, feedback, email, app_version, os_version, build_mode`. `email` is the optional reply-to the sender
chose to attach (nullable); no install id is stored, so feedback can't be joined to analytics. `created_at` is SQLite
`datetime('now')`, i.e. `YYYY-MM-DD HH:MM:SS` UTC (a space, no `T`, no offset) so a plain `YYYY-MM-DD` bound compares
lexicographically.

```bash
cd apps/api-server
npx wrangler d1 execute cmdr-telemetry --remote --json \
  --command "SELECT id, created_at, feedback, email, app_version, os_version, build_mode \
             FROM feedback WHERE created_at >= '2026-05-30' ORDER BY created_at DESC"
```

`--json` returns `[{ "results": [ ...rows... ], "success": true, ... }]`. A row with a non-null `email` is someone
awaiting a reply, treat that as an action item, not just a data point.

## Error reports (R2 bucket `cmdr-error-reports`)

Bundles are keyed `error-reports/{prod|dev}/YYYY-MM-DD/{ERR-XXXXX}-{uuid}.zip`. Default to the `prod` prefix only; `dev`
is mostly E2E/test noise. 90-day lifecycle TTL on the bucket.

`wrangler r2 object` can `get` a known key but has **no list subcommand**, so enumerate with the Cloudflare REST API
(per-day prefix; loop the dates in the range). The list response carries each bundle's metadata, so you get the shape of
the window without downloading anything:

```bash
ACC=6a4433bf11c3cf86feda057f76f47991
curl -s --max-time 25 "https://api.cloudflare.com/client/v4/accounts/$ACC/r2/buckets/cmdr-error-reports/objects?prefix=error-reports/prod/2026-05-30&per_page=100" \
  -H "Authorization: Bearer $TOKEN" \
  | jq -r '.result[]? | [.custom_metadata.id, .custom_metadata.kind, .custom_metadata.appVersion, .custom_metadata.osVersion, .last_modified] | @tsv'
```

Always pass `--max-time` (and ideally wrap in `timeout`): the CF API occasionally stalls a connection, and a bare `curl`
in a per-day loop will hang the whole run indefinitely. An empty day is a normal `{"success":true,"result":[]}`, so use
`.result[]?` to tolerate it.

`.custom_metadata` holds `id, kind, appVersion, osVersion, arch, generatedAt`. If a day has more than `per_page`
results, `.result_info` carries `is_truncated` + `cursor`; pass `&cursor=<cursor>` to page on.

Download a bundle by URL-encoding the slashes in its key:

```bash
enc=$(printf '%s' "$key" | sed 's|/|%2F|g')
curl -s --max-time 60 -o "$id.zip" "https://api.cloudflare.com/client/v4/accounts/$ACC/r2/buckets/cmdr-error-reports/objects/$enc" \
  -H "Authorization: Bearer $TOKEN"
```

Each zip holds `manifest.json` plus `logs/cmdr.log` (and any rotated logs), all PII-redacted. The manifest includes
`kind` (`user`/`auto`), `appVersion`, `osVersion`, `arch`, `generatedAt`, `buildMode`, `userNote` (Flow A only), the
`diagId`, `activeSettings`, `logLevels` (`stdoutCurrent` + `stdoutModuleOverrides`), and a `breadcrumbs` ring (`kind`,
`at`, `message`, `ctx`) that's the best triage trail. For field semantics and the redaction rules, read
`apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` and its `DETAILS.md`.

A useful clustering signal: bundles that arrived in pairs minutes apart with overlapping log timestamps are usually the
same incident; compare `breadcrumbs` and the `logs/cmdr.log` ERROR/WARN lines across bundles to group them.
