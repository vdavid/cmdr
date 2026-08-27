# Reading in-app feedback and error reports

How an agent reads the **in-app "Send feedback" messages** and the **error-report bundles** straight from the app's own
stores. This is the data behind the `#feedback` and `#error-reports` Discord channels, but read from the source, not
Discord: those channels are private and denied to the community bot (see `discord.md`), and the presigned bundle links
Discord posts expire after 7 days while the bundles themselves live 90 days in R2.

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

The table also carries `notified_at`: the timestamp of the digest email that shipped that message, NULL while it's still
waiting for one. It's bookkeeping for the email channel, so ignore it when reading feedback, and leave it alone when
writing: clearing it re-sends the message, and setting it drops one.

### The digest email

Every three hours the API server's cron mails David whatever is still un-notified, one card per message, with a
`mailto:` link per card and a `Reply-To` header when the batch holds exactly one address. That's the channel that
reaches him without being asked; D1 and Discord both need someone to go looking. Recipient is
`FEEDBACK_NOTIFICATION_EMAIL`, falling back to `CRASH_NOTIFICATION_EMAIL`. Rendering, the reply-to rule, and why the job
stamps `notified_at` after the send rather than before: `apps/api-server/DETAILS.md` § Cron handler.

## Error reports (R2 bucket `cmdr-error-reports`)

Bundles are keyed `error-reports/{prod|dev}/YYYY-MM-DD/{ERR-XXXXX}-{uuid}.zip`. Default to the `prod` prefix only; `dev`
is mostly E2E/test noise. 90-day lifecycle TTL on the bucket.

### Chasing ONE id (`ERR-ABXW4`)

The most common way you get here: David pastes a report id. **An id alone is not addressable.** The date is part of the
key and nothing maps id to date, so you cannot `get` a bundle knowing only its id, and there is no id index to query.

Walk BACKWARDS FROM TODAY, one day per request, and stop at the first hit. Reports are usually read within a day or two
of arriving, so this is normally one or two calls; a 90-day sweep to find one bundle is the slow way round and the whole
reason this section exists. ❌ Don't reach for the range recipe below for a single id.

```bash
ACC=6a4433bf11c3cf86feda057f76f47991
TOKEN=$(secret CLOUDFLARE_API_TOKEN)
for i in $(seq 0 90); do
  d=$(date -v-${i}d +%Y-%m-%d)   # GNU date: date -d "-$i day" +%Y-%m-%d
  hit=$(timeout 15 curl -s --max-time 12 </dev/null \
    "https://api.cloudflare.com/client/v4/accounts/$ACC/r2/buckets/cmdr-error-reports/objects?prefix=error-reports/prod/$d&per_page=100" \
    -H "Authorization: Bearer $TOKEN" | jq -r '.result[]?.key' | grep ERR-ABXW4)
  [ -n "$hit" ] && { echo "$hit"; break; }
done
```

❌ **Keep the loop SEQUENTIAL.** `xargs -P` (and backgrounded `curl`s) hang indefinitely against this API in a sandboxed
shell, with no output and no error: a 92-day parallel sweep produced zero rows and had to be killed twice before the
sequential version ran the same range in under a minute. Redirect stdin from `/dev/null` too, so a child can't inherit
and block on the terminal.

Once you have the key, download and unzip it per **Download a bundle** below, then read `manifest.json` (`userNote`,
`kind`, `appVersion`, `breadcrumbs`) and `logs/cmdr.log`. The breadcrumb ring holds only the last 50 events, so for
anything that happened earlier in the session the LOG is the record, not the breadcrumbs.

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
in a per-day loop will hang the whole run indefinitely. Keep the loop sequential and feed each `curl` `</dev/null` for
the reason in "Chasing ONE id" above: parallelizing it with `xargs -P` hangs with no output at all. An empty day is a
normal `{"success":true,"result":[]}`, so use `.result[]?` to tolerate it.

`.custom_metadata` holds `id, kind, appVersion, osVersion, arch, generatedAt`. If a day has more than `per_page`
results, `.result_info` carries `is_truncated` + `cursor`; pass `&cursor=<cursor>` to page on.

Download a bundle by URL-encoding the slashes in its key:

```bash
enc=$(printf '%s' "$key" | sed 's|/|%2F|g')
curl -s --max-time 60 -o "$id.zip" "https://api.cloudflare.com/client/v4/accounts/$ACC/r2/buckets/cmdr-error-reports/objects/$enc" \
  -H "Authorization: Bearer $TOKEN"
```

### The notification email

A report someone wrote by hand (`kind: 'user'`) is also mailed to David the moment it lands, with the note, the machine
facts, and the presigned download link. Auto-sent reports aren't: they go to Discord only. Recipient, cap, and why it
sends at intake instead of on a cron: `apps/api-server/src/telemetry/DETAILS.md` § Notification email.

Each zip holds `manifest.json` plus `logs/cmdr.log` (and any rotated logs), all PII-redacted. The manifest includes
`kind` (`user`/`auto`), `appVersion`, `osVersion`, `arch`, `generatedAt`, `buildMode`, `userNote` (Flow A only), the
`diagId`, `activeSettings`, `logLevels` (`stdoutCurrent` + `stdoutModuleOverrides`), and a `breadcrumbs` ring (`kind`,
`at`, `message`, `ctx`) that's the best triage trail. For field semantics and the redaction rules, read
`apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` and its `DETAILS.md`.

A useful clustering signal: bundles that arrived in pairs minutes apart with overlapping log timestamps are usually the
same incident; compare `breadcrumbs` and the `logs/cmdr.log` ERROR/WARN lines across bundles to group them.
