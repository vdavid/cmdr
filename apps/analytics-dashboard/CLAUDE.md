# Analytics dashboard

Private SvelteKit dashboard consolidating Cmdr business metrics, organized by acquisition stage. Deployed to Cloudflare
Pages at `analdash.getcmdr.com`, behind Cloudflare Access plus an in-app JWT gate (`src/hooks.server.ts`).

## Pages

Three routes share `routes/+layout.svelte` (sticky header: brand, page nav, range/day picker). The picker writes
`?range=` / `?day=` and is hidden on `/links`.

- `/` (Acquisition, `routes/+page.svelte`): daily funnel + channels, awareness, interest, download.
- `/product` (`routes/product/`): active use, settings adoption, payment, retention, feedback & errors.
- `/links` (`routes/links/`): CRUD for the `?r=` short codes, proxied to the api-server admin endpoints.

Each section is a component under `src/lib/components/sections/`; shared bits in `src/lib/components/`. Data sources
live in `src/lib/server/sources/` (one module per external API); `src/lib/server/fetch-all.ts` has the per-source
loaders and the per-page composers.

## Stack

SvelteKit + `@sveltejs/adapter-cloudflare`, Tailwind v4 (CSS-first in `src/app.css`, dark mode only), uPlot for charts.
All API keys stay server-side, proxied via `+server.ts` / `+page.server.ts`.

## Checks

`pnpm check dashboard` runs all eight (eslint, stylelint, svelte-check, import-cycles, knip, tests, build, plus
`svelte-kit sync`). Don't judge a change by raw `pnpm lint`: the runner owns ordering.

## Must-knows

- **Auth is the app's job, not just Cloudflare's.** Access binds to a hostname, so the default
  `cmdr-analytics-dashboard.pages.dev` alias reaches the same deployment around it. `src/hooks.server.ts` verifies the
  Access JWT on every server-handled request and 403s otherwise; every failure path must fail closed. Never gate on the
  spoofable `cf-access-authenticated-user-email` header. Both people and service tokens pass, so `locals.identity` is a
  `user | service` union. Details: `src/lib/server/access-jwt.ts`.
- **Write `{#if source.ok}` with the error state in `{:else}`, never the inverse.** Inverted, the Svelte ESLint parser
  loses the narrowing and every read off `source.data` becomes an unsafe-member-access error that looks like missing
  prop types. Flipping the branch is the fix, not an `any`. Why: `DETAILS.md`.
- **Don't import `$lib/server/*` as a runtime value into browser-bundled code** (components, route `.svelte` files).
  Type-only imports are fine. A runtime-value import trips `vite-plugin-sveltekit-guard` at BUILD time, and
  **`svelte-check` does NOT catch it**, so run `pnpm build` (not just `pnpm check`) after touching imports across the
  boundary. Client-shared runtime values live outside `$lib/server`; `DETAILS.md` lists them.
- **Every data source must go behind the 20s `withTimeout` cap in `fetch-all.ts`.** Workers `fetch` has no timeout, so
  one hung upstream otherwise stalls the whole `Promise.all` until Cloudflare's 524 at 100s. Sources run in parallel,
  return `SourceResult<T>`, and cache via `cache.ts`. Details: `DETAILS.md`.
- **The admin token (`LICENSE_SERVER_ADMIN_TOKEN`) stays server-side.** `/links` and the worker-backed sources resolve
  it in `+page.server.ts` only; the browser bundle gets only rows and a load-error string. `pnpm build` confirms nothing
  token-bearing leaks past the boundary.
- **`caches.default` / `caches.open()` aren't emulated in `wrangler pages dev`.** `cache.ts` falls back to an in-memory
  Map for local dev. Don't assume the CF Cache API works locally.
- **A `null` cell renders as a dash (`–`), kept distinct from a real 0** in the funnel and metric tables: "couldn't get
  this" and "this was zero" mean different things. Every source is best-effort and independent.
- **One color means one property across the whole dashboard** (metric dots, chart strokes and fills). Keep it when
  adding UI; the palette is in `DETAILS.md`.
- **The funnel needs no new env vars:** it reuses the worker admin token, Umami creds, and Paddle key already present
  (Listmonk signups come via the api-server, so no Listmonk secret reaches the dashboard).

Local dev: `pnpm dev:dashboard` serves on port 4830, reading `.env` (copy `.env.example`; escape a literal `$` as `\$`).
Env resolution and the full variable list: `DETAILS.md`.

Everything else (structure, data loading, sources, auth, decisions, env vars, local QA, deployment): `DETAILS.md`.
