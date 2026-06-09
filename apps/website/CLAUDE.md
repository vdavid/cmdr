# Website (getcmdr.com)

Marketing site and blog for Cmdr. Astro + Tailwind v4, statically built, deployed via Docker + Caddy.

## Stack

- **Astro**: static site generator with content collections
- **Tailwind v4**: CSS-first config in `src/styles/global.css`
- **Playwright**: E2E tests in `e2e/`
- **Docker + Caddy**: production hosting (see `docs/guides/deploy-website.md`)

## Deployment

**The website auto-deploys on push to `main`** when `apps/website/**` changes. The `deploy-website` job in
`.github/workflows/ci.yml` (gated on `needs: website`, so it only fires after the website eslint/typecheck/build checks
pass) sends a signed `POST https://getcmdr.com/hooks/deploy-website` (HMAC-SHA256 with `DEPLOY_WEBHOOK_SECRET`). A
webhook listener on the Hetzner VPS verifies the signature, pulls `main`, and rebuilds the Docker image (`docker compose
build` before `down`, per the deploy-order gotcha below). This is the ONLY deploy path: a standalone `deploy-website.yml`
was removed because it double-deployed and ran even when checks failed. `release.yml` also hits the same webhook after a
desktop release (to publish the refreshed `latest.json`). The manual `docker compose` steps in
`docs/guides/deploy-website.md` are what the webhook runs server-side, and the manual fallback if the hook is down;
`workflow_dispatch` on `main` (run_all) is the manual re-deploy lever.

## Blog

Blog posts live in `src/content/blog/{slug}/index.md` with colocated images.

### Key files

| File                                    | Purpose                                                   |
| --------------------------------------- | --------------------------------------------------------- |
| `src/content.config.ts`                 | Blog collection schema (title, date, description, cover)  |
| `src/layouts/BlogLayout.astro`          | Post page layout (date, title, description, prose styles) |
| `src/styles/blog-prose.css`             | Shared prose styles for blog content                      |
| `src/pages/blog/index.astro`            | Blog index: excerpts with "Read more" links, newest first |
| `src/pages/blog/[slug].astro`           | Individual post page with comments                        |
| `src/pages/og/[slug].png.ts`            | OG image generation (Satori + resvg)                      |
| `src/pages/rss.xml.ts`                  | RSS feed                                                  |
| `src/components/Remark42Comments.astro` | Comment widget (disabled in dev)                          |
| `src/components/BlogImageClick.astro`   | Click-to-fullsize for blog images                         |

### OG images

Generated at build time with Satori (JSX to SVG) and resvg (SVG to PNG). Colors are hardcoded because Satori doesn't
support CSS variables; keep them in sync with `global.css` theme values.

### Comments (Remark42)

Self-hosted at `comments.getcmdr.com`. Disabled in dev mode (shows a placeholder instead). See
`docs/guides/deploying-remark42.md` for setup.

### Adding a new post

See `docs/guides/writing-blog-posts.md`.

## Patterns

- **Layouts**: `Layout.astro` (base), `BlogLayout.astro` (posts), `LegalLayout.astro` (terms, privacy)
- **CSS variables**: defined in `src/styles/global.css` under `@theme`. Use them everywhere.
- **External links**: `rehype-external-links` auto-adds `target="_blank" rel="noopener noreferrer"`
- **RSS autodiscovery**: `<link>` tag in `Layout.astro`

## Color scheme (light/dark mode)

All pages support both light and dark mode. Users can override their system preference via a theme toggle in the header.

**How it works:**

- `global.css` defines dark tokens in `@theme` (the default) and light tokens in two places: a
  `@media (prefers-color-scheme: light)` block scoped to `:root:not([data-theme='dark'])` (system preference fallback),
  and a `:root[data-theme='light']` block (explicit toggle override). Both contain the same token values.
- `Layout.astro` includes an inline `<script>` in `<head>` that reads `localStorage('theme')` and sets `data-theme` on
  `<html>` before first paint, preventing FOUC. A matching inline `<style>` sets the background color for all
  combinations (system pref + explicit override).
- `ThemeToggle.astro` is an animated sun/moon toggle (based on theme-toggles by Alfred Jones). It sets `data-theme` on
  `<html>` and persists the choice to `localStorage`. On first visit (no localStorage entry), the site follows the
  system preference.
- On accent-colored buttons, use `--color-accent-contrast` for text (not `--color-background`), because the background
  color changes between modes but button text on accent should always be dark.
- Blog code blocks stay dark in light mode: `blog-prose.css` forces Shiki's `--shiki-dark` variables on `pre` elements,
  and uses hardcoded dark surface/border colors.
- Shiki is configured with dual themes (`github-dark` + `github-light`) and `defaultColor: false` in `astro.config.mjs`.
  The theme switching CSS is in `global.css`.
- Remark42 comments detect `data-theme` attribute, falling back to `prefers-color-scheme`.
- Hero images have dark and light variants; `Hero.astro` switches between them using the same CSS selector pattern.

**Decision/Why**: The dual-selector pattern (media query + `data-theme` attribute) ensures the site works correctly
without JS (falls back to system preference) while supporting explicit overrides via the toggle.

## Analytics

The website uses three analytics layers. The desktop app also sends anonymous beta usage analytics: an `anal_`-keyed
hourly heartbeat (true DAU) plus curated, PII-free PostHog feature events, all behind a tri-state opt-out and stripped
of file names, paths, queries, and prompts by allowlist. See `apps/desktop/src-tauri/src/analytics/CLAUDE.md`.

- **Umami** (`Layout.astro`): Cookieless page analytics (pageviews, referrers, geo, UTM). Self-hosted. Script served at
  `/u/mami` (proxied through Caddy to avoid adblockers).
- **PostHog** (`public/scripts/posthog-init.js`): Session replay, heatmaps, click tracking. Configured with
  `persistence: 'memory'` (no cookies, no localStorage) and `person_profiles: 'identified_only'` (no anonymous person
  profiles). This keeps PostHog fully cookieless. The same EU project also receives the desktop app's
  `source: "desktop"` feature events.
- **D1** (API server): Download redirect endpoint logs version, arch, and country. The `heartbeat` table holds the
  desktop DAU beats.

**Decision/Why**: We avoid cookies to not need a cookie consent banner. All three analytics tools are configured to work
without cookies. If you add or change analytics tooling, preserve this property: no cookies unless absolutely
inevitable. See `docs/tooling/umami.md` and `docs/tooling/posthog.md` for API access and config details.

## Gotchas

- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind. It doesn't
  affect the build.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- Font files for OG image generation (`inter-400.ttf`, `inter-700.ttf`) live in `public/fonts/`.
- All pages support both light and dark mode. Don't hardcode colors; use CSS variables from `global.css`.
- **Deploy order**: Always `docker compose build` before `docker compose down`. Building first keeps the old container
  serving traffic. `down → build → up` causes ~15s downtime while the image builds.
