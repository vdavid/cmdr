# Show HN capacity check (2026-11-04)

Will getcmdr.com, the browser demo at `/demo/`, and the api-server survive a Show HN front-page day? Research only:
nothing in production changed. Every number carries how and when it was measured. "Estimate" marks anything derived
rather than measured.

## Verdict

The static site and the demo will hold comfortably: Cloudflare serves them from cache, the origin is tiny and fast, and
the demo is well packaged. The real risk sits on the dynamic side: **D1's Free-plan write cap of 100,000 rows a day is
already about half used on a normal day**, and a successful launch will very likely blow through it, silently stopping
telemetry, crash reports, feedback, and download attribution for the rest of that UTC day. The Workers Free cap of
100,000 requests a day is the second risk, and it fails closed, so the download button would show a Cloudflare error
page. Both go away with Workers Paid ($5 a month), which needs David's go.

**Update 2026-11-05**: the account is on Workers Paid now (vdavid/cmdr#947), so fix 1 is done and failure modes 1, 2,
and 4 are gone. The D1 size alarm measures against Paid's 10 GB (`D1_MAX_DB_MB`), and `wrangler.toml` caps CPU at 1,000
ms per invocation (`apps/api-server/DETAILS.md` § Deployment). The rest of this note is the 2026-11-04 snapshot.

## 1. Traffic to plan for

Public reports (each fetched 2026-11-04 via Firecrawl):

- marcotm.com, "Stats of being on the Hacker News front page": 35–40 unique visitors a minute at rank 17, about 10 a
  minute once it fell to page two.
- nicklafferty.com, "What happens when you're on the front page": about 18,000 visitors in the day, peak rank 5, 270
  comments.
- blog.royalsloth.eu, "How much traffic comes from the front page of Hacker News": 93,275 requests on day one, about
  20,000 unique requests per front-page post, about 6 GB out.
- ma.ttias.be, "The Frontpage of Hacker News" (2015): stuck at the very top for hours, "around 600 HTTP req/s" at peak
  (assets included), 1 → 60 Mbps within 30 minutes, 37,955 hits to the page.
- thehftguy.com (2017): "10k to 100k visitors in the days after", 2,000+ in one hour.
- A Show HN that hit number one (Josh Howarth, Trennd, Medium): peaked at about 400 concurrent users.

Our own baseline, for scale (Umami DB, `website_event`, queried 2026-11-04): about 76 page views a day over the last 30
days; the largest day in 45 days was 2026-09-30 (the iPhones.ru spike) at 331 page views and 232 sessions, with a peak
minute of eight page views. **A front-page day is 100–300x anything this stack has seen.**

Planning numbers (estimate, from the sources above):

- **Visitors**: 20,000–50,000 on the day, plan for 50,000; a top-three run can reach 100,000 over two days.
- **Peak**: 300 visitors a minute in the first one to three hours, with bursts of 2x: plan for **10 page views a
  second**.
- **Shape**: a steep ramp within 20–30 minutes of hitting the front page, a plateau of a few hours, then a long tail;
  the second day is roughly half the first.
- **Demo share**: unknown; assume 30% of visitors open `/demo/`, so about three demo loads a second at peak.
- **Downloads**: assume 5–10% of visitors, so 2,500–5,000 DMG downloads; assume 1,000–2,000 first-day installs.

## 2. The path of a page view and a demo load

### DNS and Cloudflare

DNS and zone settings (CF API, read 2026-11-04):

- `getcmdr.com` and `www.getcmdr.com` are proxied (orange). `api.getcmdr.com` and `license.getcmdr.com` are proxied
  CNAMEs to the Worker. `comments.getcmdr.com` (remark42) is **DNS-only**, straight to the Hetzner IP.
- The zone is on the **Free Website** plan. Smart Tiered Cache is on (set 2026-10-23), so a miss at a lower-tier data
  center goes to an upper tier before it reaches the origin. Brotli on, browser cache TTL 0 ("respect existing
  headers"), security level medium, Always Online off.
- Cache rules (infra repo `cloudflare/getcmdr.com-cache.tf`): "bypass by default", so Cloudflare caches only what the
  origin gives a lifetime. nginx sends `Cloudflare-CDN-Cache-Control: max-age=3600` on pages and the demo shell,
  `max-age=60` on `latest.json`, and a year (`immutable`) on hashed or versioned assets. Link-preview bots always skip
  the cache. Every deploy purges the whole zone (`infra/deploy-webhook/deploy-website.sh`).

Cache behavior (curl with `Accept-Encoding: br, gzip` from Stockholm, Cloudflare ARN, 2026-11-04):

- `/`, `/demo/`, `/features/`: first request MISS or REVALIDATED, every repeat HIT. Pages stay `Cache-Control: no-cache`
  for browsers, so a returning visitor revalidates.
- Zone-wide daily cache ratio today is low: 5–11% of requests and 19–30% of bytes over 2026-10-28–2026-11-04 (GraphQL
  `httpRequests1dGroups`). That's the low-traffic shape, not a misconfiguration: the 24-hour breakdown
  (`httpRequestsAdaptiveGroups`, 2026-11-03T12:00Z to 2026-11-04T12:00Z) shows the uncached share is
  first-in-data-center misses (each of Cloudflare's hundreds of data centers fills its own cache), plus the Worker hosts
  (`api.`, `license.`, `analdash.`, about 3,000 requests) and the dynamic analytics paths. Under HN load each data
  center warms after its first visitor, so the hit ratio climbs toward 95%+ (estimate).

### What the browser downloads

Measured with headless Chromium (Playwright 1.64.1, CDP `encodedDataLength`, browser cache disabled, 2026-11-04, script
kept in the session scratchpad):

- **Home page**: 21 requests, **0.80 MB on the wire** (1.49 MB decoded). All HIT on a warm run except Umami's beacon
  `/u/api/send` (DYNAMIC). Biggest items: PostHog's `array.js` (120 KB) and recorder (71 KB), four product clips (60–112
  KB WebP each), the font (50 KB), the HTML (41 KB zstd).
- **Demo**: 109 requests, **1.94 MB on the wire** (5.71 MB decoded), all HIT on the second run, load event at 133 ms
  from Stockholm. The WASM engine is the biggest single file: `cmdr_listing_wasm_bg.CRMMqomx.wasm`, 2,148,017 bytes raw,
  served as `application/wasm` with `Cache-Control: public, max-age=31536000, immutable` and `Content-Encoding` zstd
  (801,697 B), br (765,706 B), or gzip (763,359 B) depending on what the browser accepts (curl per encoding). Next
  largest: `nodes/5` JS at 276 KB and two chunks at 171 KB and 117 KB.
- Every `/demo/_app/immutable/` file is content-hashed and immutable. The demo loads no analytics and calls no
  api-server endpoint (no `/u/`, `/ph/`, or `api.getcmdr.com` request in the run; its CSP is `connect-src 'self'`). So
  **demo usage is invisible to Umami and PostHog**; only nginx's access log sees it, and only on cache misses.
- Small nit: `/demo/icons/default-folder.png` and `default-file.png` come back `Cache-Control: no-cache`, so the browser
  revalidates them on each visit. Harmless.

Edge bandwidth at the planned peak (estimate): 10 home views a second × 0.8 MB plus three demo loads a second × 1.94 MB
is about 14 MB/s, roughly 110 Mbps. Cloudflare Free doesn't meter it.

### The Hetzner box

Read over SSH, 2026-11-04 12:08 UTC:

- `ubuntu-4gb-hel1-1`: four vCPU (AMD EPYC-Rome), 7.6 GiB RAM, **no swap**, 2.7 GiB available, load average 0.33 / 0.47
  / 0.43, root disk 65% used (13 GB free).
- 46 containers. The big tenants are mailcow (clamd alone 1 GiB, rspamd, MariaDB, SOGo), Umami (229 MiB plus Postgres),
  MiroTalk, Monica, and listmonk. `getcmdr-static` (nginx) uses 5 MiB.
- Path: Cloudflare → Caddy (TLS plus per-hostname Authenticated Origin Pulls, so nobody can reach the site around
  Cloudflare) → `getcmdr-static` nginx on `proxy-net`. Caddy also proxies `/u/*` to Umami, `/ph/*` to PostHog EU, the
  listmonk public routes, and `/hooks/*` to the deploy webhook.
- nginx sends plain bytes on purpose (Cloudflare compresses), so a cache miss on the WASM costs the origin 2.1 MB.
- Network: `eth0` sent 7.2 GB in 20 days of uptime (`/proc/net/dev`), against the 20 TB a month a Hetzner Cloud server
  includes.
- Origin traffic today is tiny: `access-2026-11-03.log` holds 5,887 nginx requests, of which about 2,900 are the
  container's own `wget` health check (twice a minute) and 912 are `latest.json` refreshes. Only one request touched
  `/demo/` that day.

### Load test of the origin

Run on 2026-11-04 12:10 UTC from the box itself against the nginx container on the Docker bridge (`172.18.0.11`),
bypassing Caddy and Cloudflare, with `oha` 1.16.0 (static Linux binary from GitHub releases, run under `nice -n 10`,
removed afterward):

- `GET /` (205 KB uncompressed HTML), 200 requests a second, 20 connections, 15 s: 3,000 × 200, 100% success, mean 0.55
  ms, p99 1.08 ms, slowest 2.75 ms. Whole-box CPU during the run: 84% idle.
- `GET` the 2.1 MB WASM, 25 requests a second, five connections, 10 s: 250 × 200, mean 1.66 ms, p99 3.17 ms, about 52
  MB/s; whole-box CPU 91% idle.
- Load average stayed under 0.4 throughout, so other tenants didn't notice.

What this means: nginx could serve every HN request uncached and still be bored. With Smart Tiered Cache, the origin
sees roughly one request per file per upper-tier data center per hour (estimate), so static serving isn't a concern. I
didn't load-test Caddy's TLS path, Umami, PostHog, or the api-server (all write somewhere or belong to a third party).

### The dynamic parts on the box

Per page view, the origin sees (Caddy access log, 80 minutes on 2026-11-04, and the CF 24-hour breakdown):

- One Umami beacon `/u/api/send` (mean 36 ms, max 59 ms through Caddy), which writes a Postgres row.
- About five PostHog requests (`/ph/i/v0/e/`, `/ph/s/` session recording, `/ph/flags/`, `/ph/e/`), each proxied to
  `eu.i.posthog.com` in 77–117 ms on average. PostHog's static files under `/ph/static/` are edge-cached.
- At the planned peak that's about 10 Umami writes and about 50 PostHog proxy requests a second (estimate). Caddy and
  Umami should absorb that; it's the busiest the box will get, but nowhere near CPU limits given today's 84–91% idle
  under test load.
- Blog posts also embed remark42 (`comments.getcmdr.com`), which is DNS-only, uncached, and served by the box. It only
  matters if the HN link points at a blog post.

## 3. The api-server and other dynamic parts

### Plan and limits

- The account was on **Workers Free** at the time (`apps/api-server/DETAILS.md`, verified there 2026-10-23 against $0
  invoices); it moved to Workers Paid on 2026-11-05. Limits (developers.cloudflare.com, fetched 2026-11-04 with
  `maxAge: 0`): 100,000 requests a day per account, reset at midnight UTC, then Error 1027; 10 ms CPU per request; 50
  subrequests per request.
- Both Worker routes (`api.getcmdr.com/*`, `license.getcmdr.com/*`) are `request_limit_fail_open: false` (CF API,
  2026-11-04), so **past the daily cap, every api request gets a Cloudflare 1027 error page**. Fail-open wouldn't help:
  the hostnames are CNAMEs to `workers.dev`, there's no origin behind them.
- D1 Free (fetched 2026-11-04): 5 million rows read and **100,000 rows written a day**; past either, "you will not be
  able to run queries against D1" until the reset. 500 MB per database.
- KV Free: 1,000 writes and 100,000 reads a day (Cloudflare's published Free limits; not re-fetched).
- IP rate limits (`apps/api-server/wrangler.toml`) count per data center and only cap one client: heartbeat 12/min,
  signups 5/min, feedback 5/min, error reports 3/min, crash reports 10/min, likes 20/min, activate 10/min, validate
  60/min. Download and update-check have none. None of them will bite a real HN visitor.

### Today's usage against those limits

Cloudflare GraphQL, queried 2026-11-04:

- **Worker requests** (`workersInvocationsAdaptive`, 2026-10-05–2026-11-04): 1,610–4,040 a day across the account
  (`cmdr-license-server` plus `git-strata-cors-proxy` and `slither-duo-relay`, which share the cap). 63,120 requests in
  30 days, all `success`, zero errors. CPU p50 is about 5 ms, but **p99 is 13–28 ms and p99.9 29 ms, above the Free
  plan's 10 ms**, with no CPU errors so far, so Cloudflare isn't enforcing it strictly today. Don't count on that under
  load.
- **D1 rows written** (`d1AnalyticsAdaptiveGroups`, `cmdr-telemetry`): **36,438–58,940 a day** over 2026-10-20 to
  2026-11-03, about 2,000 write queries a day. Rows read: 120,000–1,030,000 a day (the dashboard's aggregations), far
  under five million.
- **Active installs** (`/admin/active-users?range=7d`): 277–366 a day. So each active install costs about 140 D1 rows
  written a day (estimate: 45,000 / 320). Heartbeats batch up to 500 events, and `analytics_event` carries four indexes
  plus its unique key, so every event is several row writes (`migrations/0020_heartbeat_uptime_and_events.sql`).
- **KV**: 13–65 reads and 1–49 writes a day.
- **Downloads** (`/admin/downloads?range=7d`): 35–121 a day.

### Under the spike (estimate)

- **Worker requests**: downloads (2,500–5,000) plus new installs (1,000–2,000 × about 15 calls on day one: heartbeats,
  update checks, settings) plus the usual 2,000–4,000 is about 25,000–40,000. Under the cap, unless something multiplies
  it: an `?r=` code in the HN link adds one Worker request per visitor (the expansion fetches
  `api.getcmdr.com/r-codes.json`, `apps/website/src/layouts/Layout.astro`), which at 50,000 visitors alone is half the
  cap; heartbeat retries after a D1 outage add more; a 100,000-visitor day roughly doubles the rest.
- **D1 rows written**: today's 45,000–59,000 plus 1,000–2,000 installs × 140 rows plus 2,500–5,000 download rows (each a
  row plus its index entries) is **190,000–350,000, two to three and a half times the cap**. Even 400 new installs would
  use up the headroom. The cap would hit hours into the launch.
- **What breaks when D1 is out**: download redirects still work (the D1 write is fire-and-forget and swallowed) but go
  uncounted, so launch-day attribution is lost; heartbeats, update-check rows, and crash reports are lost; in-app
  feedback gets a soft 502 and the app offers a retry; error-report cards and the dashboard stop until 00:00 UTC.
  Visitors on the website don't see any of it. New users who send feedback do.
- **Downloads themselves**: the Worker resolves `latest` from `getcmdr.com/latest.json` (edge-cached five minutes) and
  302s to GitHub Releases, so GitHub serves the DMG: 116 MB universal, 62 MB Apple Silicon, 67 MB Intel (v0.59.0,
  `gh release view`). 5,000 universal downloads is about 580 GB, all on GitHub, which charges nothing for release
  downloads. No egress on our side.
- **Update feed**: the app's `/update-check/:version` writes one row and 302s to `getcmdr.com/latest.json`, cached 60 s
  at the edge. Fine.
- **Licensing**: `/activate` uses KV and `/validate` calls Paddle, each rate-limited per IP. Purchases on launch day are
  a handful; no capacity concern.

## 4. Failure modes, ranked by likelihood times impact

1. **D1 write cap hit mid-launch.** Likelihood: high if the post reaches the front page. Impact: medium-high. Visitors
   see nothing, but the launch-day funnel, crash reports, and feedback from brand-new users vanish until 00:00 UTC, and
   new users sending feedback see a retry prompt.
2. **Workers daily request cap, failing closed.** Likelihood: low-medium (needs a 100,000-visitor day, an `?r=` link, or
   a retry storm on top of failure 1). Impact: high. Every download button and `getcmdr.com/download/latest` shows a
   Cloudflare 1027 error page until 00:00 UTC, and the app's licensing and update checks fail.
3. **A deploy during the spike.** Likelihood: medium (agents land things all day, and the website auto-deploys from
   `main`). Impact: medium. The deploy builds on the box with `--no-cache` (pnpm install plus the Astro build) while 2.7
   GiB is free and there's no swap, next to mailcow; then `docker compose down && up` leaves a few seconds of 502s (and
   restarts remark42 too); then `purge_everything` empties every data center's cache at the busiest moment.
4. **Worker CPU over 10 ms.** Likelihood: low-medium. Impact: medium. The p99 is already 13–28 ms; if Cloudflare starts
   enforcing under load, some downloads and heartbeats get Error 1102.
5. **The box itself goes down** (OOM from point 3, a mailcow spike, a Hetzner incident). Likelihood: low. Impact: high.
   Pages are cached for an hour but nothing is served stale on errors, and Always Online is off, so within the hour
   visitors get Cloudflare's 52x page.
6. **Monitoring doesn't see an origin outage.** Likelihood: medium. Impact: low-medium. UptimeRobot checks only
   `https://getcmdr.com` every five minutes (`docs/tooling/monitoring.md`), which is an edge-cache HIT, so an origin
   outage or a 1027 on the api-server goes unnoticed until someone complains on HN.
7. **PostHog quotas.** Likelihood: medium. Impact: low. Free PostHog includes a limited number of session recordings a
   month (5,000 on its published pricing; not checked against this account), so recordings stop partway through the day.
   Invisible to visitors.
8. **KV writes past 1,000 a day.** Likelihood: low (only if the HN link is a blog post and many people click like).
   Impact: low. Likes fail.

The static site, the demo, nginx, bandwidth, GitHub-hosted DMGs, and the update feed aren't on this list: they have 50x
or more headroom.

## 5. Fixes

1. **Workers Paid for the launch** (done 2026-11-05).
   - Problem: Free caps D1 writes at 100,000 rows a day (half used today), Workers at 100,000 requests (fail closed),
     and CPU at 10 ms (p99 already above it).
   - Impact: fixes failure modes 1, 2, and 4 at once.
   - Solution: upgrade the account to Workers Paid before launch day: 50 million D1 rows written a month included, no
     daily request cap, and up to five minutes of CPU per request. D1's pricing page says the limits lift "typically
     within minutes" of the upgrade, so it could even be a same-day reaction, but going in early is safer.
   - Size: 10 minutes in the dashboard, $5 a month.
   - Clear win. The only cost is the $5.
2. **Deploy freeze on launch day** (David's call; no account changes).
   - Problem: a deploy builds on the box, restarts the site and remark42, and purges every cache.
   - Impact: removes failure mode 3 and most of failure mode 5.
   - Solution: hold website-affecting landings (`apps/website/**`, demo sources, releases) from about an hour before
     posting until the evening. Tell the agents.
   - Size: none.
   - Clear win.
3. **Post the bare URL, without `?r=`.**
   - Problem: an `?r=` code costs one Worker request (and maybe a KV read) per visitor.
   - Impact: keeps the Worker count low; HN's `Referer` already attributes the traffic.
   - Solution: submit `https://getcmdr.com/` (apex, so no extra `www` redirect through the origin), or the demo URL.
   - Size: none.
   - Clear win.
4. **Serve stale pages if the origin errors.**
   - Problem: Cloudflare has nothing to fall back on when the box is down (failure mode 5).
   - Impact: visitors keep getting the site during a short origin outage.
   - Solution: add `stale-if-error=86400` (and maybe `stale-while-revalidate=60`) to the `Cloudflare-CDN-Cache-Control`
     lines in `apps/website/nginx.conf`. Cloudflare's docs say it honors `stale-if-error` except when Always Online is
     on or an explicit `no-cache` applies; our browser-facing `Cache-Control` is `no-cache`, so verify on a test path
     that the CDN header wins before relying on it. A deploy's purge still drops the stale copies, which is another
     reason for fix 2.
   - Size: small (two-line nginx change plus a verification with the origin container stopped on a test path).
   - Tradeoff, small: a stale page for up to a day during an outage, which beats an error page.
5. **Watch what can actually break.**
   - Problem: the only monitor checks a cached page.
   - Impact: failure modes 2, 5, and 6 get noticed in minutes.
   - Solution: add UptimeRobot monitors for `https://api.getcmdr.com/download/latest/universal` (expects a 302; use a UA
     the bot filter catches, so it doesn't pollute D1) and for an origin-only path, for example `/u/mami`. During the
     launch, keep the Cloudflare dashboard's D1 and Workers usage open; the numbers in § 3 came from its GraphQL API.
   - Size: small (15 minutes in UptimeRobot).
   - Clear win.
6. **Trim analytics row writes** (only if David declines fix 1).
   - Problem: about 140 D1 rows per active install a day, mostly `analytics_event` plus its indexes.
   - Impact: more headroom on Free.
   - Solution: drop indexes no query uses, or sample heartbeat events server-side for the launch week.
   - Size: medium, and it touches what the privacy policy and dashboard promise.
   - Tradeoff: real engineering and data loss to save $5 a month. Fix 1 is better.
7. **PostHog recording budget.**
   - Problem: free recordings may run out mid-day; every recording also rides through the box.
   - Impact: low either way.
   - Solution: check the account's recording quota and billing limit in PostHog before launch (needs David's login), or
     set a session-recording sample rate in `public/scripts/posthog-init.js` for the week.
   - Size: small.
   - Tradeoff: fewer recordings of exactly the day you'd want to watch them.
8. **Count demo usage** (optional, not a capacity fix).
   - Problem: the demo has no analytics, so you won't know how many HN visitors tried it.
   - Solution: count `/demo/` page loads at the edge (Cloudflare's zone analytics by path already shows them), or add a
     Umami beacon to the demo page, which needs a CSP `connect-src` change.
   - Size: small.
   - Tradeoff: the zone-analytics route is free and good enough for launch day.

## Needs David

- Fix 1 (Workers Paid, $5 a month): done 2026-11-05.
- Fix 2 (deploy freeze): a workflow call for launch day.
- Fix 7: checking PostHog's quota needs his PostHog login.
- `/demo/` is still `noindex, nofollow` with the title "Cmdr demo" and isn't linked from the home page (curl,
  2026-11-04; `apps/website/DETAILS.md` § Browser demo calls it unlaunched on purpose). If the Show HN post links it,
  decide whether to launch it properly first.
