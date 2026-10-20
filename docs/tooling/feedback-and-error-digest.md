# Reading in-app feedback and error reports

How an agent reads the **in-app "Send feedback" messages** and the **error-report bundles** straight from the app's own
stores. This is the data behind the reports board and the three-hour digest email, read from the source: the presigned
bundle links the digest carries expire after 24 hours, while the bundles themselves live 90 days in R2.

## The triage board comes first

Every crash, error report (hand-written or auto-sent), and feedback message from a release build is filed as an issue in
the **private** `vdavid/cmdr-reports` repo, which is the board David prioritizes on. Reach for it before the stores
below when the question is "what's waiting on me" rather than "what exactly happened in this bundle":

```bash
gh issue list --repo vdavid/cmdr-reports --label needs-reply
gh issue view <n> --repo vdavid/cmdr-reports --comments
```

Each issue's BODY is technical only (report id, versions, R2 key). The note, the reply-to address, and the bundle link
are in a comment that is **deleted after 90 days** (two years for a feedback reply-to). So a card older than that still
names its `ERR-XXXXX` and R2 key, and the recipes below are how you get the rest, for as long as the bundle itself
lives. Cards are filed by the API server's three-hour cron, so a report can take up to three hours to appear; one that
predates 2026-10-14 may have no card at all (back then only hand-written reports got one).

How the filing works, and the invariants that keep private data off the public repo: `apps/api-server/DETAILS.md` § The
reports repo.

The `/feedback-and-error-digest-from-app` command (`.claude/commands/`) drives the whole flow; this doc is the access
recipe it points to. Read-only: never write or delete here without explicit approval (`no-external-actions`).

## Auth

Both stores are reached with the Cloudflare API token already on this machine:

- `wrangler` reads it from the `CLOUDFLARE_API_TOKEN` env var (already set; `npx wrangler whoami` confirms the account).
- For raw REST calls, load it from the sops secrets store: `TOKEN=$(secret CLOUDFLARE_API_TOKEN)`.

Cloudflare account id: `6a4433bf11c3cf86feda057f76f47991` (also printed by `wrangler whoami`; not a secret).

## Feedback (D1 table `feedback` in `cmdr-telemetry`)

One row per submission, written by `POST /feedback`. Columns (see `apps/api-server/migrations/0007_feedback.sql`):
`id, created_at, feedback, email, app_version, os_version, build_mode, error_report_id`. `email` is the optional
reply-to the sender chose to attach (nullable); no install id is stored, so feedback can't be joined to analytics.
`error_report_id` is the `ERR-XXXXX` of the logs the sender ticked "Include the past five hours of logs" for (nullable,
migration `0026`): an ordinary error report, uploaded right after the feedback, with the note
`Logs sent with in-app feedback`. Chase it like any other id (§ Chasing ONE id below); on the board it rides on the
feedback's card rather than getting its own. `created_at` is SQLite `datetime('now')`, i.e. `YYYY-MM-DD HH:MM:SS` UTC (a
space, no `T`, no offset) so a plain `YYYY-MM-DD` bound compares lexicographically.

```bash
cd apps/api-server
npx wrangler d1 execute cmdr-telemetry --remote --json \
  --command "SELECT id, created_at, feedback, email, app_version, os_version, build_mode, error_report_id \
             FROM feedback WHERE created_at >= '2026-05-30' ORDER BY created_at DESC"
```

`--json` returns `[{ "results": [ ...rows... ], "success": true, ... }]`. A row with a non-null `email` is someone
awaiting a reply, treat that as an action item, not just a data point.

The table also carries `notified_at`: the timestamp of the digest email that shipped that message, NULL while it's still
waiting for one. It's bookkeeping for the email channel, so ignore it when reading feedback, and leave it alone when
writing: clearing it re-sends the message, and setting it drops one.

### The digest email

Every three hours the API server's cron mails David ONE digest of every crash, error report, amendment, and feedback
message that arrived since the last one (none arrived, no email), each entry linking its card on the board. Recipient is
`FEEDBACK_NOTIFICATION_EMAIL`, falling back to `CRASH_NOTIFICATION_EMAIL`. Rendering, the reply-to rule, and why the job
stamps `notified_at` after the send rather than before: `apps/api-server/DETAILS.md` § Cron handler.

## Error reports (R2 bucket `cmdr-error-reports`)

Bundles are keyed `error-reports/{prod|dev}/YYYY-MM-DD/{ERR-XXXXX}-{uuid}.zip`. Default to the `prod` prefix only; `dev`
is mostly E2E/test noise. 90-day lifecycle TTL on the bucket.

### Chasing ONE id (`ERR-ABXW4`)

The most common way you get here: David pastes a report id. **Ask KV first.** Every upload writes `report:{ERR-XXXXX}` →
`{ env, date, key, amendKeyHash }` into the `ERROR_REPORT_META` namespace, so one call turns an id into its full R2 key:

```bash
cd apps/api-server
npx wrangler kv key get "report:ERR-ABXW4" --binding ERROR_REPORT_META --remote --text
# {"env":"prod","date":"2026-08-27","key":"error-reports/prod/2026-08-27/ERR-ABXW4-<uuid>.zip","amendKeyHash":"..."}
```

`amendKeyHash` is only there for the amend route's credential check; it tells you nothing useful about the report.

The entry has a 90-day TTL, matching the bucket lifecycle, so a miss means one of two things: the bundle is gone too, or
the report predates the index. **For a report that predates the index, fall back to the walk below.**

Walk BACKWARDS FROM TODAY, one day per request, and stop at the first hit. Reports are usually read within a day or two
of arriving, so this is normally one or two calls; a 90-day sweep to find one bundle is the slow way round. ❌ Don't
reach for the range recipe below for a single id.

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

### Amendments (`{key}.amend.json`)

A reporter can add a note or a reply-to address after their bundle is already stored, and it lands in a sidecar object
beside the bundle: the same key with `.zip` swapped for `.amend.json`, holding
`{ id, amendments: [{ note, email, amendedAt }, ...] }`. Fetch it with the same URL-encoded-key `curl` as a bundle; it's
plain JSON, no unzip. Each amendment is also mailed when it arrives, so an inbox thread usually beats going to R2 for
it. Most reports have no sidecar; a 404 there is the normal case, not a problem.

### The `error_reports` row

Every upload, hand-written or auto-sent, also writes a row to the D1 `error_reports` table (short id, kind, versions, R2
key, the note and reply-to, `issue_number`, `notified_at`), deleted with the bundle at 90 days. For a recent report it's
the quickest way to its note and card:

```bash
npx wrangler d1 execute cmdr-telemetry --remote --json \
  --command "SELECT short_id, created_at, kind, user_note, email, issue_number FROM error_reports WHERE short_id = 'ERR-ABXW4'"
```

Each zip holds `manifest.json` plus `logs/cmdr.log` (and any rotated logs), all PII-redacted. The manifest includes
`kind` (`user`/`auto`), `appVersion`, `osVersion`, `arch`, `generatedAt`, `buildMode`, `userNote` (Flow A only), the
`diagId`, `activeSettings`, `logLevels` (`stdoutCurrent` + `stdoutModuleOverrides`), and a `breadcrumbs` ring (`kind`,
`at`, `message`, `ctx`) that's the best triage trail. For field semantics and the redaction rules, read
`apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` and its `DETAILS.md`.

**Check `activeSettings.fullDiskAccessChoice` and `.osFullDiskAccess` early on any macOS permission-shaped report.** A
missing grant changes what a whole class of failures means, and the two fields answer different questions: the choice is
what the USER clicked (`allow` / `deny` / `unanswered`), the boolean is what the OS says right now. `allow` with
`osFullDiskAccess: false` is someone who clicked Allow and never finished in System Settings. ❗ `unanswered` means the
question is still open, NOT that onboarding never ran: the wizard opens on that step every launch until it's answered,
so a long-time user can sit there. Reports from before these fields exist need the old route: grep `fda_probe` and
`FDA choice:` in `logs/cmdr.log`.

⚠️ **A `FDA choice:` line is written ONCE per launch, at boot, so it can be hours stale.** It says what the setting held
when that launch started, never what the grant is now. Before concluding anything from one, find the LAST `FDA probe:`
line at or after the `Logger initialized` line of the launch the failure actually happened in; a single log file holds
several launches. `ERR-2YGHG` (app 0.45.1) is the example: its first launch logged
`Drive indexing auto-start skipped (indexing enabled: None, FDA choice: NotAskedYet, OS-granted: false)`, 30 seconds
later the same log has `FDA probe: read OK → FDA granted` and `Downloads watcher started (FDA gate open)`, and all three
later launches in the file were granted, the reported failure among them. Reading the boot line alone sends you
diagnosing a permission problem that was fixed before the report was written.

A useful clustering signal: bundles that arrived in pairs minutes apart with overlapping log timestamps are usually the
same incident; compare `breadcrumbs` and the `logs/cmdr.log` ERROR/WARN lines across bundles to group them.
