# Analytics dashboard — details

Read this before structural changes. `CLAUDE.md` is the always-loaded summary; this is the depth.

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
- `/links` — Link codes (`routes/links/+page.svelte`): a stub. The `?r=` short-link CRUD lands here later; for now it's
  a heading + a "coming soon" note, no data load. The layout hides the range/day picker here.

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
   parallel; `fetchProductData` runs Cloudflare + Paddle + license + feedback-and-errors. Each page's `+page.server.ts`
   re-resolves the selection from the URL and calls its composer, so a page fetches only what it renders.
3. **`fetchDashboardData`**: all nine sources, used by the report endpoint (`api/report/+server.ts`) which dumps every
   section at once. Unchanged contract.

The funnel is always 30 days (`fetchFunnelData` ignores the selection); it's still gated on the worker admin token, with
Umami and Paddle degrading to dashes inside. Each source returns `SourceResult<T>` (ok+data, or an error string the UI
shows as "Couldn't load this data").

### The Cloudflare source is shared by both pages

`fetchCloudflareData` returns `downloads` (Download section, on `/`) AND `heartbeatDau` + `updateActivity` (Active use,
on `/product`) in one fetch. Both pages call `fetchCloudflareSource`. That's intentional, not duplication: the source is
cached per selection in `cache.ts`, so the two page loads share one cache entry rather than re-fetching. Don't split the
worker endpoints apart to "load less" — it would fragment the cache key and the 401/timeout degradation, for no gain.

## Selection state

The shared `DashboardSelection` (`{ range, day }`) is resolved from `?range=` / `?day=` in `+layout.server.ts` (for the
picker UI) and again in each `+page.server.ts` (for that page's fetch); both call the same `resolveSelection`, so they
agree. A valid `?day=YYYY-MM-DD` forces `range: 'day'`. The funnel row click on `/` navigates to `/?day=<date>`, which
filters the Acquisition sections to that day and highlights the row. Cache keys include the day (`selectionCacheKey`) so
two picked days never collide. The whole rationale (why a `DashboardSelection` not a bare `TimeRange`, the coarse-window
mapping for worker/PostHog sources) is in `CLAUDE.md` § "Key decisions".

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
