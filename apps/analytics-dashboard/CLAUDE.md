# Analytics dashboard

Private SvelteKit dashboard consolidating Cmdr business metrics, organized by acquisition stage. Deployed to Cloudflare
Pages at `analdash.getcmdr.com`, auth via Cloudflare Access (zero-trust, no application code).

## Pages

Three routes share `routes/+layout.svelte` (sticky header: brand, page nav, range/day picker). The picker writes
`?range=` / `?day=` and is hidden on `/links`.

- `/` (Acquisition, `routes/+page.svelte`): daily funnel + channels, awareness, interest, download.
- `/product` (`routes/product/`): active use, payment, retention, feedback & errors.
- `/links` (`routes/links/`): CRUD for the `?r=` short codes, proxied to the api-server admin endpoints.

Each section is a component under `src/lib/components/sections/`; shared bits in `src/lib/components/`. Data sources
live in `src/lib/server/sources/` (one module per external API); `src/lib/server/fetch-all.ts` has the per-source
loaders and the per-page composers.

## Stack

SvelteKit + `@sveltejs/adapter-cloudflare`, Tailwind v4 (CSS-first in `src/app.css`, dark mode only), uPlot for charts.
All API keys stay server-side, proxied via `+server.ts` / `+page.server.ts`.

## Must-knows

- **Don't import `$lib/server/*` as a runtime value into browser-bundled code** (components, route `.svelte` files).
  Type-only imports are fine. A runtime-value import trips `vite-plugin-sveltekit-guard` at BUILD time, and
  **`svelte-check` does NOT catch it**, so run `pnpm build` (not just `pnpm check`) after touching imports across the
  boundary. Client-shared runtime values live outside `$lib/server`: `$lib/funnel.ts`, `$lib/feedback-and-errors.ts`,
  `$lib/link-codes.ts`, `$lib/format.ts`, `$lib/colors.ts`, `$lib/chart-helpers.ts`.
- **Every data source must go behind the 20s `withTimeout` cap in `fetch-all.ts`.** Workers `fetch` has no built-in
  timeout, so without the cap one hung upstream stalls the whole `Promise.all` until Cloudflare returns a 524 at 100s.
  Each source returns `SourceResult<T>` (ok+data, or an error string the UI shows as "Couldn't load this data"); results
  are cached via `cache.ts` (5 min for 24h/7d, 1 hour for 30d). The page server calls all sources in parallel.
- **The admin token (`LICENSE_SERVER_ADMIN_TOKEN`) stays server-side.** `/links` and the worker-backed sources resolve
  it in `+page.server.ts` only; the browser bundle gets only rows and a load-error string. `pnpm build` confirms nothing
  token-bearing leaks past the boundary.
- **`caches.default` / `caches.open()` aren't emulated in `wrangler pages dev`.** `cache.ts` falls back to an in-memory
  Map for local dev. Don't assume the CF Cache API works locally.
- **A `null` cell renders as a dash (`–`), kept distinct from a real 0** in the funnel and metric tables: "couldn't get
  this" and "this was zero" mean different things. Every source is best-effort and independent.
- **Color coding is consistent across the dashboard** (metric dots, chart strokes/fills). Keep it when adding UI:
  - Gold (`#ffc206`): getcmdr.com / vdavid/cmdr (primary product).
  - Purple (`#a78bfa`): vdavid/mtp-rs (library repo).
  - Autumn green (`#8faa3b`): veszelovszki.com (personal site).
  - Cyan (`#22d3ee`): getprvw.com (Prvw product site).
- **No new dashboard env vars** for the funnel: it reuses the worker admin token, Umami creds, and Paddle key already
  present (Listmonk signups are sourced inside the api-server, so no Listmonk secret reaches the dashboard).

Local dev reads env vars via SvelteKit's `$env/dynamic/private` when `platform?.env` is undefined (CF Pages
`platform.env` only exists when deployed). Copy `.env.example` to `.env`; values containing `$` must use `\$`.
`pnpm dev:dashboard` serves on `http://localhost:4830`.

Full details (multi-page structure, data-loading split, link-codes CRUD, selection state, componentization, the data
source list, decision rationale, env-var list, local QA against a local worker, deployment): [DETAILS.md](DETAILS.md).
