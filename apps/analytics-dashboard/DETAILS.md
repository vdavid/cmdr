# Analytics dashboard — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` is the
always-loaded summary; this is the depth.

## Multi-page structure

The dashboard is three routes under one shared layout. Each section is its own component; the route files just compose
sections and pass their loaded data down.

### Routes and sections

- `/` — Acquisition (`routes/+page.svelte`), in render order:
  - Daily funnel + Channels (`components/FunnelTable.svelte`) — always the last 30 UTC days, independent of the picker.
  - Awareness (`components/sections/AwarenessSection.svelte`).
  - Interest (`components/sections/InterestSection.svelte`).
  - Download (`components/sections/DownloadSection.svelte`), with `CountryTable.svelte` and the per-day pie tooltip.
- `/product` — Product (`routes/product/+page.svelte`):
  - Active use (`ActiveUseSection.svelte`), Payment (`PaymentSection.svelte`), Retention (`RetentionSection.svelte`),
    Feedback & errors (`FeedbackErrorsSection.svelte`).
- `/links` — Link codes (`routes/links/+page.{svelte,server.ts}`): CRUD for the `?r=` short codes. See § "Link codes
  CRUD" below. The layout hides the range/day picker here.

### Shared layout

`routes/+layout.svelte` is the only shell: a sticky header with the brand, the page nav (Acquisition / Product / Link
codes, active page marked with `aria-current="page"` and the accent background), and the range/day picker. The picker
writes `?range=` / `?day=` and keeps the current pathname (`${pathname}?range=...`), so switching range on `/product`
stays on `/product`. It's hidden on `/links`. The "Updated HH:MM" stamp reads `page.data.updatedAt` (set by whichever
data page is active; absent on `/links`).

`rangeButtons` (the picker's `today/24h/7d/30d` button list) lives as a local const in the layout, NOT in
`$lib/server/types.ts` — see the boundary gotcha below.

## Data-loading split

`fetch-all.ts` is structured as:

1. **Per-source loaders** (`fetchFunnelSource`, `fetchUmamiSource`, `fetchCloudflareSource`, …): each wraps one source
   with its env-var guard (`guardedFetch`) and the 20s `withTimeout` cap. One place per source.
2. **Per-page composers**: `fetchAcquisitionData` runs funnel + Umami + Cloudflare + GitHub + GitHub-stars + PostHog in
   parallel; `fetchProductData` runs Cloudflare + Paddle + license + feedback-and-errors + settings-adoption. Each
   page's `+page.server.ts` re-resolves the selection from the URL and calls its composer, so a page fetches only what
   it renders.
3. **`fetchDashboardData`**: all ten sources, used by the report endpoint (`api/report/+server.ts`) which dumps every
   section at once. Unchanged contract.

The funnel is always 30 days (`fetchFunnelData` ignores the selection); it's still gated on the worker admin token, with
Umami and Paddle degrading to dashes inside. Each source returns `SourceResult<T>` (ok+data, or an error string the UI
shows as "Couldn't load this data").

`AcquisitionData` also carries `umamiSiteUrls`, which is **config, not a source**: the per-site Umami dashboard deep
links, built from the `UMAMI_*_WEBSITE_ID` env vars so the IDs live in exactly one place instead of being hardcoded in a
component. It has no `SourceResult` wrapper because it has no failure mode; a site whose ID isn't configured is `null`
and the UI drops that link rather than pointing at a broken page. `fetch-all.subset.test.ts` pins both that shape and
the fact that it's excluded from the "which sources does this page load" contract.

The 20s cap is load-bearing, not belt-and-braces: Workers `fetch` has no built-in timeout, so without it a single hung
upstream stalls the whole `Promise.all` until Cloudflare gives up with a 524 at 100 seconds, taking the entire page down
over one slow third party. Results cache via `cache.ts`: 5 minutes for the 24h and 7d ranges, 1 hour for 30d.

### The Cloudflare source is shared by both pages

`fetchCloudflareData` returns `downloads` (Download section, on `/`) AND `heartbeatDau` + `updateActivity` (Active use,
on `/product`) in one fetch. Both pages call `fetchCloudflareSource`. That's intentional, not duplication: the source is
cached per selection in `cache.ts`, so the two page loads share one cache entry rather than re-fetching. Don't split the
worker endpoints apart to "load less" — it would fragment the cache key and the 401/timeout degradation, for no gain.

## Link codes CRUD

The `/links` page manages the `?r=` short codes that the blogs and the website expand into UTM params. It's the admin
front end for the api-server's `link-codes.ts` (`/admin/r-codes` CRUD over the `LINK_CODES` KV namespace).

- **Token stays server-side.** The page never holds the admin token. `+page.server.ts` resolves it via `resolveEnv` (the
  same `LICENSE_SERVER_ADMIN_TOKEN` the worker-backed sources use) and is the only code that touches it. The `load`
  lists the map; the `save` and `delete` form actions proxy writes. The browser bundle gets only the rows and a load
  error string — `pnpm build` confirms nothing token-bearing leaks past the client/server boundary.
- **Two modules, by boundary.** `$lib/link-codes.ts` is client-safe: validation (`validateLinkCode`, mirroring the
  api-server's `isValidCode`/`sanitizeUtmValue` so the form rejects bad input pre-round-trip), `toRows` (flatten +
  sort), and `exampleLink`. `$lib/server/sources/link-codes.ts` is server-only: `fetchLinkCodes` / `upsertLinkCode` /
  `deleteLinkCode`, each attaching the bearer token. The server action re-validates before proxying; the api-server is
  the final source of truth and re-validates again.
- **No caching here.** Unlike the metric sources, the admin list isn't cached: David edits it interactively, so the view
  must reflect live KV. The public `/r-codes.json` is the edge-cached path (≈5 min), so the page's description warns
  that edits take up to ~5 min to reach visitors.
- **Form UX.** One add/edit form (Edit copies a row's values in and locks the code field, since the code is the KV key);
  per-row Delete; inline validation/proxy errors via `fail(...)`; a live example-link preview. All `use:enhance` with
  `reset: false` so a failed save repopulates.

## Selection state

The shared `DashboardSelection` (`{ range, day }`) is resolved from `?range=` / `?day=` in `+layout.server.ts` (for the
picker UI) and again in each `+page.server.ts` (for that page's fetch); both call the same `resolveSelection`, so they
agree. A valid `?day=YYYY-MM-DD` forces `range: 'day'`. The funnel row click on `/` navigates to `/?day=<date>`, which
filters the Acquisition sections to that day and highlights the row. Cache keys include the day (`selectionCacheKey`) so
two picked days never collide. The whole rationale (why a `DashboardSelection` not a bare `TimeRange`, the coarse-window
mapping for worker/PostHog sources) is in § "Key decisions" below.

## Componentization

- Section components take their `SourceResult<…>` props plus `selection` (so `ErrorState` can build the "Try again"
  link). They own their own local interaction state — e.g. `DownloadSection` owns the timeline zoom window and the
  per-day pie tooltip; `CountryTable` owns its hover tooltip and shares the parent's zoom window via props.
- Shared presentational pieces: `MetricRow`, `MetricTable`, `SectionDescription` (the "what + how reliable" blurb),
  `Methodology` (the "how this is measured" note), `ErrorState` / `EmptyState` / `BetaEmptyState`, `ExternalLinks`.
- Pure data shaping lives in `$lib/chart-helpers.ts` (stacking, `aggregateBy`, `buildTimeline`, semver compare, etc.),
  formatting in `$lib/format.ts`, and color tokens in `$lib/colors.ts`. These are client-safe modules.

## Client/server boundary gotcha

SvelteKit forbids importing `$lib/server/*` as a runtime value into browser-bundled code (components, route `.svelte`
files). Type-only imports (`import type { DashboardSelection, SourceResult } from '$lib/server/types.js'`) are fine. A
runtime value import (e.g. a `const`) trips `vite-plugin-sveltekit-guard` at BUILD time — and `svelte-check` does NOT
catch it. So always run `pnpm build`, not just `pnpm check`, after touching imports across that boundary. Client-shared
runtime values live outside `$lib/server`: `$lib/funnel.ts`, `$lib/feedback-and-errors.ts`, `$lib/format.ts`,
`$lib/colors.ts`, `$lib/chart-helpers.ts`.

## Key files

- `src/app.css`: Tailwind v4 theme (dark palette matching getcmdr.com). `src/app.d.ts`: CF Pages platform env types.
- `src/routes/+layout.svelte`: shared shell (header, page nav, range/day picker). `src/routes/+layout.server.ts`:
  resolves the shared `DashboardSelection` from `?range=` / `?day=` once for the layout.
- `src/routes/+page.{svelte,server.ts}`: Acquisition page (funnel/Umami/Cloudflare/GitHub/PostHog subset).
  `src/routes/product/+page.{svelte,server.ts}`: Product page (Cloudflare/Paddle/license/feedback subset).
  `src/routes/links/+page.{svelte,server.ts}`: Link codes CRUD (`load` lists, `save`/`delete` form actions proxy).
- `src/routes/api/report/+server.ts`: the agent-readable plain-text report endpoint. It only fetches (via
  `fetchDashboardData`) and responds; all the formatting is in `src/routes/api/report/format-report.ts`, which imports
  no SvelteKit-only modules so a plain unit test can reach it. `format-report.test.ts` pins the whole output as golden
  strings across four fixtures, so any wording, ordering, or spacing change to the report is a deliberate act.
- `src/lib/server/fetch-all.ts`: per-source loaders, per-page composers (`fetchAcquisitionData`, `fetchProductData`),
  and the all-sources `fetchDashboardData`.
- `src/lib/server/types.ts`: `TimeRange`, `DashboardSelection`, `SourceResult`, time window + selection helpers.
- `src/lib/server/cache.ts`: CF Cache API wrapper with in-memory Map fallback for local dev.
- `src/lib/server/sources/`: data source modules (one per external API).
- `src/lib/components/sections/`: one component per dashboard section. `src/lib/components/`: shared UI (FunnelTable,
  CountryTable, MetricRow/MetricTable, ErrorState/EmptyState/BetaEmptyState, SectionDescription, Methodology,
  ExternalLinks, Chart, StackedBarChart, MiniTimeline, PieChart).
- `src/lib/{format,colors,chart-helpers}.ts`: client-safe formatters, color tokens, chart data-shaping helpers.
- `src/lib/link-codes.ts`: client-safe `?r=` helpers (validation mirroring the api-server, row flattening, example
  link), shared by the `/links` page, its server action, and tests.
- `StackedBarChart.svelte`: discrete per-day stacked bars (plain elements, not uPlot) with an exact-numbers hover/focus
  tooltip; used for by-source new-installs and by-version update charts.
- `svelte.config.js` (adapter-cloudflare), `vitest.config.ts`.

## Data sources

Each source under `src/lib/server/sources/` exports a typed fetch returning `SourceResult<T>`:

- `umami.ts` (JWT login): page views, visitors, referrers, countries, download events for veszelovszki.com, getcmdr.com,
  getprvw.com.
- `cloudflare.ts` (Bearer via `LICENSE_SERVER_ADMIN_TOKEN`): downloads (by version/arch/country/source, raw +
  same-day-deduped), true per-day DAU + beats from the heartbeat, per-day update-check activity by version, from worker
  endpoints (`/admin/downloads`, `/admin/heartbeat-dau`, `/admin/update-activity`).
- `paddle.ts` (Bearer, cursor pagination): completed transactions, subscriptions by status.
- `github.ts` (optional Bearer): release download counts per asset; star history (daily + cumulative) for cmdr and
  mtp-rs.
- `posthog.ts` (Bearer personal API key): pageview trends via the HogQL query API (EU endpoint).
- `license.ts` (Bearer admin token): activation count + active devices from `/admin/stats`.
- `feedback-and-errors.ts` (Bearer via `LICENSE_SERVER_ADMIN_TOKEN`): in-app feedback + error-report bundle metadata
  from `/admin/feedback` and `/admin/error-reports`. Pure helpers + row types in client-safe
  `$lib/feedback-and-errors.ts`.
- `funnel.ts` (Bearer + Umami + Paddle): feeds the top "Daily funnel" table (per-UTC-day, last 30 days, always 30 days
  independent of the picker), joining `/admin/funnel` with Umami per-day visitors/clicks and Paddle purchases. Per-day
  `downloadsByRef` feeds the "Channels" breakdown (rolled up by `aggregateChannels` in `$lib/funnel.ts`).
- `settings-adoption.ts` (Bearer via `LICENSE_SERVER_ADMIN_TOKEN`): per-setting adoption from `/admin/config-shape`,
  resolved against the per-version defaults manifest before it leaves the server (§ Settings adoption).
- `worker-endpoint.ts` (shared helper): `fetchWorkerEndpoint(token, path)`, used by the worker-backed sources.

## Settings adoption

The heartbeat's config shape carries only what a user CHANGED (`settings.json` is sparse by design), so on its own it
measures deviation and never adoption: an absent key means EITHER "still on the default" OR "the setting didn't exist in
that build". Sending every default with every beat would answer that and was rejected as wasteful, so the answer is
resolved here instead, joined on the `app_version` every heartbeat already carries:

    effective = override ?? defaults[newest manifest entry at or below the install's app version][key]

- **The manifest** is `src/lib/server/settings-defaults.gen.json`, generated from the desktop settings registry by
  `apps/desktop/scripts/gen-analytics-defaults.ts` and pinned to it by the `settings-defaults` check. ❌ Never hand-edit
  it. Entries are sparse along the version axis (written only where a default moved) but COMPLETE within an entry, which
  is what lets an absent key mean something.
- **A key missing from the resolved entry means the setting did not exist in that build**, so that install drops OUT of
  that setting's denominator instead of counting as a default. Get this wrong and every newly added setting silently
  corrupts its own adoption number for every older install still running. `settings-defaults.test.ts` pins it.
- **An install on a version older than the manifest's first entry can't be resolved at all.** Those are counted in
  `unresolvedInstalls` and surfaced in the UI, ❌ never folded into a denominator.
- **Resolution runs server-side** (`src/lib/server/settings-defaults.ts`), so the manifest never reaches the browser
  bundle; the client gets the aggregate plus the client-safe views in `$lib/settings-adoption.ts`.
- **A value someone set back to the default counts as being on the default.** Persistence is structural, not a value
  compare, so a deliberate choice equal to the default is persisted explicitly; what adoption asks about is what the app
  runs with.

## Local QA against a local worker

The download/update-activity/feedback/error charts come from the api-server worker (default `https://api.getcmdr.com`).
To QA against seeded data with zero production impact, run the worker locally and point the dashboard at it via
`WORKER_BASE_URL`:

1. From `apps/api-server/`, start the worker on a local D1 with seeded rows:
   ```bash
   pnpm exec wrangler d1 migrations apply cmdr-telemetry --local
   pnpm exec wrangler d1 execute cmdr-telemetry --local --file=scripts/seed-local-telemetry.sql
   pnpm exec wrangler dev --port 18900 --var ADMIN_API_TOKEN:local-qa-token
   ```
   The seed uses dates relative to `now`, includes a same-day duplicate IP (dedup), a pre-migration NULL-source row, and
   a 0.24→0.25 update rollout.
2. In `.env`, set `WORKER_BASE_URL=http://127.0.0.1:18900` and `LICENSE_SERVER_ADMIN_TOKEN=local-qa-token`, then
   `pnpm dev:dashboard`.

`WORKER_BASE_URL` is unset in production (sources fall back to `api.getcmdr.com`). Sources without local creds (Umami,
Paddle, PostHog) show "Couldn't load" and don't block the worker-backed charts.

## Deployment and env vars

Auto-deploys to CF Pages on push to `main` when `apps/analytics-dashboard/` changes
(`.github/workflows/deploy-dashboard.yml`: checkout, install, build, wrangler deploy).

Manual setup (not in code): create CF Pages project `cmdr-analytics-dashboard`; add `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` to GitHub repo secrets; set all env vars below as CF Pages secrets; configure the custom domain
`analdash.getcmdr.com` and the Cloudflare Access policy.

If the Access application is ever recreated, its audience tag changes and the constant in `src/lib/server/access-jwt.ts`
must change with it, or every request starts 403ing. Read the current values with the CF API (needs
`Access: Apps and Policies → Read` and `Access: Organizations → Read`):

```bash
curl -s -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/access/apps"
```

`resolveEnv` reads from `platform.env` when deployed and falls back to SvelteKit's `$env/dynamic/private` otherwise: CF
Pages only populates `platform.env` on a real deployment, so local dev would see nothing without the fallback. Copy
`.env.example` to `.env` and escape a literal `$` as `\$`.

All env vars are CF Pages secrets, never in code:

- `UMAMI_API_URL`: `https://anal.veszelovszki.com`. `UMAMI_USERNAME`: `admin`. `UMAMI_PASSWORD`: read it with
  `secret UMAMI_PASSWORD`; sops is the source of truth, and the copies here and in the Pages secrets are exactly that,
  copies. Rotating means updating all three together (the recipe lives in the maintainer's Umami tooling note).
- `UMAMI_WEBSITE_ID`: getcmdr.com. `UMAMI_BLOG_WEBSITE_ID`: veszelovszki.com (name kept for CF secret compatibility).
  `UMAMI_PRVW_WEBSITE_ID`: getprvw.com (add when deploying).
- `PADDLE_API_KEY_LIVE`: live key (not sandbox).
- `POSTHOG_API_KEY`: personal `phx_...` key (not the public `phc_...`). `POSTHOG_PROJECT_ID`: `136072`.
  `POSTHOG_API_URL`: `https://eu.posthog.com` (must be EU).
- `GITHUB_TOKEN`: optional, avoids public-repo API rate limits.
- `LICENSE_SERVER_ADMIN_TOKEN`: dedicated admin secret, also set on the API server.

## Color coding

One color means one property everywhere it appears (metric dots, chart strokes and fills), so a reader can carry the
association from one section to the next:

- Gold (`#ffc206`): getcmdr.com / vdavid/cmdr, the primary product.
- Purple (`#a78bfa`): vdavid/mtp-rs, the library repo.
- Autumn green (`#8faa3b`): veszelovszki.com, the personal site.
- Cyan (`#22d3ee`): getprvw.com, the Prvw product site.

## Write `{#if source.ok}`, never `{#if !source.ok}`

Every section renders a `SourceResult<T>`, which is a discriminated union on `ok`. Put the happy path in the `{#if}`
branch and the error state in `{:else}`:

```svelte
{#if source.ok}
  {@const data = source.data}
  ...
{:else}
  <ErrorState message={source.error} />
{/if}
```

Inverted, `svelte-eslint-parser` narrows the union correctly inside `{#if !source.ok}` but loses the negated narrowing
in the `{:else}`, so `source.data` resolves to an error type and every read off it cascades into
`@typescript-eslint/no-unsafe-member-access`. The two forms are behavior-identical, so this costs nothing. It's worth
knowing because the failure looks like "the props are untyped" and invites a bogus fix (an `any`, or a type assertion);
flipping the branch is the actual fix. Six sections were written inverted, and flipping them cleared about 200 of the
270 lint errors the app had when ESLint was introduced.

## Tooling

The dashboard is a first-class app in the Go check runner (`pnpm check dashboard`); the per-check rationale and what it
deliberately omits live in `scripts/check/checks/DETAILS.md` § "Apps and check counts".

What's specific to this app's configs:

- **ESLint** (`eslint.config.js`) mirrors the api-server's `strictTypeChecked` base plus the desktop app's Svelte parser
  blocks. Type-aware rules need `.svelte-kit/tsconfig.json`, which `tsconfig.json` extends, so `svelte-kit sync` has to
  run first; the `dashboard-eslint` check hard-fails when it's missing rather than let the rules quietly no-op.
- Config files (`svelte.config.js`, `vite.config.ts`, `vitest.config.ts`) lint with `disableTypeChecked`. They sit
  outside the generated tsconfig's include list, and adding them to the TS project purely to satisfy the linter would be
  the tail wagging the dog.
- **Component props are typed, not inferred.** Route pages annotate with `PageProps` / `LayoutProps` from `./$types`;
  reusable components declare an explicit `interface Props`. Cross-boundary type imports (`import type` from
  `$lib/server/...`) are safe and expected: only a _runtime_ value import would pull server code into the browser
  bundle.
- **Stylelint** (`.stylelintrc.mjs`) is deliberately lighter than desktop's. Tailwind v4 supplies the utilities, so
  there's no design-token ladder to police here; the config's real job is allowing Tailwind's CSS-first at-rules
  (`@theme` and friends, which `stylelint-config-standard` would reject as unknown) while keeping the actual-mistake
  rules.
- **knip** (`knip.json`) treats SvelteKit's `+page` / `+layout` / `+server` files and `hooks.server.ts` as entry points.
  Without that it reports every route as unused, since nothing imports them by name.

## Auth

Two layers, and both are load-bearing.

1. **Cloudflare Access** on `analdash.getcmdr.com` runs the login flow (team domain `getcmdr.cloudflareaccess.com`, app
   "Cmdr analytics dashboard"). It's what makes the dashboard usable without any login UI in the app.
2. **`src/hooks.server.ts` verifies the Access JWT** on every server-handled request, via
   `src/lib/server/access-jwt.ts`, and answers 403 when it doesn't check out.

**Why the second layer.** Access binds to a _hostname_, not to a Pages project. The project's default
`cmdr-analytics-dashboard.pages.dev` alias serves the identical deployment and never passes through Access, so an
Access-only setup leaves every page, form action, and `/api/report` open to anyone who guesses the alias. Edge config
can't be the only gate; an in-app check covers all hostnames and all routes at once.

**How the verification works.** Read the token from the `cf-access-jwt-assertion` header, or the `CF_Authorization`
cookie on browser navigations. Pin `alg` to RS256 _before_ touching the signature (this is what defeats `alg: none` and
HS256-over-the-public-key confusion), resolve the `kid` against Cloudflare's published JWKS at
`https://getcmdr.cloudflareaccess.com/cdn-cgi/access/certs`, verify the RSASSA-PKCS1-v1_5 signature with WebCrypto, then
check `iss`, `aud`, `exp`, and `nbf` (60s clock skew). Keys are cached for an hour; an unrecognized `kid` triggers
exactly one refetch, so a Cloudflare key rotation doesn't lock everyone out and a bogus `kid` can't drive unbounded
fetches at the certs endpoint.

**Two kinds of caller, one gate.** Access mints a user token on a browser login and a `type: 'app'` token for a service
token (the `CF-Access-Client-Id` / `CF-Access-Client-Secret` pair the agent-readable `/api/report` recipe uses, see
`docs/tooling/analytics-dashboard.md`). Both are signed by the same keys and go through the identical checks; only the
last step differs, mapping a verified payload to an identity. A machine token has no `email` claim at all and an empty
`sub`, and is named by `common_name`, so `verifyAccessJwt` returns a discriminated union:
`{ kind: 'user', email, sub } | { kind: 'service', commonName }`, landing on `event.locals.identity`. The union is the
point: a bare email string would have to be empty or synthetic for a machine, and a route wanting a real person could
then read it as one by accident. `type` is checked first, so a machine token can never come back as a user whatever else
it carries; each branch then needs its own identifying claim, and a payload with neither is refused. That mapping step
is the last place a payload can still be rejected, so it fails closed both ways: a `type: 'app'` token with no
`common_name` is refused, as is a user token with no `email`.

**Constants, not env vars.** The team domain and audience tag live in `access-jwt.ts` as constants. Both are public (the
audience tag appears in Access's own login-redirect URL), and a missing env var would fail _open_ on a deploy slip,
which is the one direction an auth gate must never fail. Reading the audience from config would trade a real risk for no
benefit.

**Local dev.** The gate is skipped under `import.meta.env.DEV`, which Vite inlines at build time, so the deployed bundle
contains no bypass branch at all (verifiable: the compiled `handle` in
`.svelte-kit/output/server/entries/hooks.server.js` has a bare block where the condition was). `pnpm dev:dashboard`
works with no token. `wrangler pages dev` serves a built bundle, so it enforces the gate and needs a real Access token.

**No hand-rolled crypto beyond this.** Verification is ~60 lines of WebCrypto against a pinned algorithm, which is what
Cloudflare's own guidance describes. `src/lib/server/access-jwt.test.ts` covers the attack cases directly: `alg: none`,
HS256 confusion, foreign signing key, tampered payload, wrong audience, wrong issuer, expired, not-yet-valid, malformed,
and certs-endpoint failure (must reject, never fail open). The service-token path carries the same set: a self-minted
`type: 'app'` payload spliced onto a real signature, an unsigned one, and one minted for another Access application all
have to be refused, or the machine path would be a way around the gate rather than a way through it.

## Key decisions

- **Metrics organized by acquisition stage, not as a cohort funnel.** Tracking is cookieless and anonymous, so there's
  no way to follow an individual from blog visit to download to payment. Stages show independent aggregate numbers.
- **A top "Daily funnel" table lines stages up per UTC day** (last 30 days, newest first, today partial), independent of
  the picker, to answer "what happened on day Y across the whole path". It's NOT a true cohort funnel across columns
  (still no cross-site identity); each column is its own per-day aggregate. Clicks won't equal server downloads (clicks
  are in-browser; server downloads also include Homebrew, direct links, GitHub-page traffic, bots filtered imperfectly).
  D7 needs a cohort ≥8 days old that had installs, so recent/empty days show a dash. The api-server owns the funnel
  contract and the D7 definition (`apps/api-server/src/admin/DETAILS.md` § "Per-day funnel").
- **Time selection is a `DashboardSelection` (`{ range, day }`) carried in the URL,** not a bare `TimeRange`: David
  needs "today" and any single specific day, not just rolling windows. A valid `?day=YYYY-MM-DD` forces `range: 'day'`
  (so a single-day link is shareable/stable). Worker-backed and PostHog sources can't isolate one day cheaply, so they
  map `today`/`day` to their nearest coarse window (`24h`) via `selectionToWorkerRange`; Umami and Paddle honor the
  exact day. Cache keys include the day (`selectionCacheKey`) so two picked days never collide.
- **Every section carries a "what insight + how reliable" blurb** (`sectionDescription`) plus per-chart `methodology`
  notes, because an opaque analytics number is worse than none. They state real caveats (Umami under/double-counts;
  clicks vs server downloads differ; new installs miss opt-outs and debug builds; heartbeat DAU is the trustworthy one;
  D7 needs old cohorts; Paddle has minor webhook lag; all days UTC).
- **"Active use" daily-active count comes from the heartbeat** (`/admin/heartbeat-dau`, `COUNT(DISTINCT anal_id)` per
  day), not from summed update checks (which multiplied a ~10/day figure into a wildly inflated total). Charted gold
  over the range, with `beats/day` as an engagement signal. Starts empty at release and fills as beta testers update.
- **"New installs" (Download) and "Got the latest release" (Active use) are two distinct charts off two distinct tables,
  never merged.** A DMG download (`downloads` table) is a fresh acquisition; an update check (`update_checks` table) is
  an existing install updating in place; in-app auto-updates fetch from GitHub and never hit the download endpoint, so
  the populations don't overlap. New-installs bars stack by source using the deduped same-day-distinct count
  (`uniqueDownloads`); update bars stack by the version each install was on when it checked.
- **"Feedback & errors" reads the app's own stores via two worker admin endpoints** (`/admin/feedback` from D1,
  `/admin/error-reports` from R2 `list` with `customMetadata`), not Discord (the `#feedback`/`#error-reports` channels
  are private and denied to the community bot). The row types and pure aggregation helpers live in
  `$lib/feedback-and-errors.ts` (client-safe) so the page, the report endpoint, and tests share one copy. The
  agent-facing local digest (`/feedback-and-error-digest-from-app`) reads the same stores; see
  `docs/tooling/feedback-and-error-digest.md`.
- **PostHog uses the HogQL query API** (`/api/projects/{id}/query/`), not the legacy Trends API (`/insights/trend/`),
  which returns "Legacy insight endpoints are not available" for newer accounts.
- **Umami metrics use `type=path`** (not `type=url`): `url` and `page` return 400, but `path`, `referrer`, `event`, and
  `country` work.
- **uPlot for charts** (~45 KB, fast canvas, simple API, no wrapper). **Dark mode only** (internal tool, always on a
  laptop).
- **Split across pages under a shared layout, not one long scroll:** the single page grew too dense; grouping by stage
  keeps each route readable and lets each page fetch only what it renders. Selection stays shared (resolved in the
  layout, carried in the URL). See § "Multi-page structure".
