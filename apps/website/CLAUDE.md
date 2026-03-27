# Website (getcmdr.com)

Marketing site and blog for Cmdr. Astro + Tailwind v4, statically built, deployed via Docker + Caddy.

## Stack

- **Astro** — static site generator with content collections
- **Tailwind v4** — CSS-first config in `src/styles/global.css`
- **Playwright** — E2E tests in `e2e/`
- **Docker + Caddy** — production deployment (see `docs/guides/deploy-website.md`)

## Blog

Blog posts live in `src/content/blog/{slug}/index.md` with colocated images.

### Key files

| File                                    | Purpose                                                    |
| --------------------------------------- | ---------------------------------------------------------- |
| `src/content.config.ts`                 | Blog collection schema (title, date, description, cover)   |
| `src/layouts/BlogLayout.astro`          | Post page layout (date, title, description, prose styles)  |
| `src/styles/blog-prose.css`             | Shared prose styles for blog content                       |
| `src/pages/blog/index.astro`            | Blog index — excerpts with "Read more" links, newest first |
| `src/pages/blog/[slug].astro`           | Individual post page with comments                         |
| `src/pages/og/[slug].png.ts`            | OG image generation (Satori + resvg)                       |
| `src/pages/rss.xml.ts`                  | RSS feed                                                   |
| `src/components/Remark42Comments.astro` | Comment widget (disabled in dev)                           |
| `src/components/BlogImageClick.astro`   | Click-to-fullsize for blog images                          |

### OG images

Generated at build time with Satori (JSX to SVG) and resvg (SVG to PNG). Colors are hardcoded because Satori doesn't
support CSS variables — keep them in sync with `global.css` theme values.

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

The website uses three analytics layers. The desktop app has **no telemetry**.

- **Umami** (`Layout.astro`): Cookieless page analytics (pageviews, referrers, geo, UTM). Self-hosted. Script served at
  `/u/mami` (proxied through Caddy to avoid adblockers).
- **PostHog** (`public/scripts/posthog-init.js`): Session replay, heatmaps, click tracking. Configured with
  `persistence: 'memory'` (no cookies, no localStorage) and `person_profiles: 'identified_only'` (no anonymous person
  profiles). This keeps PostHog fully cookieless.
- **D1** (API server): Download redirect endpoint logs version, arch, and country.

**Decision/Why**: We avoid cookies to not need a cookie consent banner. All three analytics tools are configured to work
without cookies. If you add or change analytics tooling, preserve this property — no cookies unless absolutely
inevitable. See `docs/tooling/umami.md` and `docs/tooling/posthog.md` for API access and config details.

## Gotchas

- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind. It doesn't
  affect the build.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- Font files for OG image generation (`inter-400.ttf`, `inter-700.ttf`) live in `public/fonts/`.
- All pages support both light and dark mode. Don't hardcode colors — use CSS variables from `global.css`.
- **Deploy order**: Always `docker compose build` before `docker compose down`. Building first keeps the old container
  serving traffic. `down → build → up` causes ~15s downtime while the image builds.
