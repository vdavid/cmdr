# Website (getcmdr.com)

Marketing site and blog for Cmdr. Astro + Tailwind v4 (CSS-first config in `src/styles/global.css`), Playwright E2E in
`e2e/`, statically built, deployed via Docker + Caddy. Full details: [DETAILS.md](DETAILS.md). This app is human-facing,
so its markdown may use tables freely.

## Module map

- `src/pages/`, `src/layouts/`, `src/components/`: pages, layouts (`Layout.astro` base, `BlogLayout.astro`,
  `LegalLayout.astro`), components.
- `src/components/icons/`: shared `<Icon name size>` glyph system (Lucide line-art). Every icon goes through it; no
  `<img>`/raw `~icons`/decorative emoji. [DETAILS.md](DETAILS.md) § Icons.
- `src/content/blog/{slug}/index.md`: blog posts with colocated images (schema in `src/content.config.ts`). Add a post:
  `docs/guides/writing-blog-posts.md`.
- `src/dev/blog-editor/`: dev-only Markdown editor at `/dev/blog` (Vite middleware, not an Astro page, absent from prod
  builds). `pnpm dev:website`, then open it.
- `src/pages/llms.txt.ts` / `llms-full.txt.ts`: agent-facing product descriptions. Keep in sync when product facts
  (pricing, features, requirements) change.

## Deployment

The website auto-deploys on push to `main` when `apps/website/**` changes, via the `deploy-website` job in `ci.yml`
(gated on `needs: website`): a signed (HMAC-SHA256, `DEPLOY_WEBHOOK_SECRET`) hook to the Hetzner VPS, which pulls and
rebuilds the Docker image. This is the ONLY deploy path. `release.yml` hits the same hook after a desktop release (to
publish the refreshed `latest.json`); `workflow_dispatch` on `main` (run_all) is the manual lever. Server-side steps and
fallback: `docs/guides/deploy-website.md`.

- **Deploy order (gotcha)**: always `docker compose build` before `docker compose down`. Building first keeps the old
  container serving traffic; `down → build → up` causes ~15s downtime.

## Analytics (must-knows)

Full narrative (three analytics layers, first-touch `ref`, `?r=` expansion mechanics) in [DETAILS.md](DETAILS.md) §
Analytics.

- **`window.__cmdrRReady` gates Umami, PostHog, AND the first-touch `ref` script.** All three await this Promise before
  recording a pageview or reading `utm_source`, so the async `?r=` code expansion (a fetch to
  `api.getcmdr.com/r-codes.json`) takes effect first. Gate anything new that reads `utm_source` or records a pageview on
  it too. Don't revert Umami/PostHog to static `<script>` tags: a static tag fires before the fetch resolves and records
  the raw `?r=`.
- **Inline `<script is:inline>` analytics bodies must be raw JS, NEVER wrapped in a literal Astro ``{`...`}``
  template-literal expression.** Astro ships that wrapper as inert dead text, so analytics silently never loads. Author
  the body as plain JS leading with `;` (`define:vars` supplies the consts). The `website-analytics-injection` check
  guards this.
- **Charset is the cross-repo attribution contract.** The client-side `?r=` sanitizer must normalize identically to the
  api-server's, or stored and pass-through values diverge (`docs/architecture.md` § Acquisition analytics).
- **The site must never need a cookie consent banner.** Preference flags (theme, download arch, newsletter
  dismissed/subscribed) in localStorage are fine and settled (don't flag as a compliance problem). Anything that
  identifies, follows, or attributes a visitor must NOT use cookies/localStorage/sessionStorage: track anonymously in
  aggregate. Apply the preference-vs-tracking test to new client-side persistence ([DETAILS.md](DETAILS.md) §
  Client-side storage policy).
- A new download link needs `data-download-link` (main) or `data-arch` inside `[data-download-dropdown]` so the ref
  script finds it.

API access: `docs/tooling/umami.md`, `docs/tooling/posthog.md`.

## Color scheme (light/dark)

All pages support both modes; a header toggle (`ThemeToggle.astro`) overrides system preference. Mechanism
(dual-selector token setup, FOUC-free inline head script) in [DETAILS.md](DETAILS.md) § Color scheme.

- Don't hardcode colors; use CSS variables from `global.css`. (OG images are the exception: Satori can't read CSS
  variables, so keep their hardcoded colors in sync with `global.css` theme values.)
- On accent-colored buttons, use `--color-accent-contrast` for text (not `--color-background`): accent button text must
  stay dark across modes.

## Gotchas

- **Keep TS generic calls single-line in `.astro` `<script>` blocks.** The astro-eslint parser chokes on a multi-line
  generic call, failing `website-eslint` and cascade-blocking typecheck/build/deploy. Single-line `foo<T>(...)` parses
  fine.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind; harmless.
- Remark42 comments (`comments.getcmdr.com`) are disabled in dev. Setup: `docs/guides/deploying-remark42.md`.
