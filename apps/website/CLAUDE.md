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
webhook listener on the Hetzner VPS verifies the signature, pulls `main`, and rebuilds the Docker image
(`docker compose build` before `down`, per the deploy-order gotcha below). This is the ONLY deploy path: a standalone
`deploy-website.yml` was removed because it double-deployed and ran even when checks failed. `release.yml` also hits the
same webhook after a desktop release (to publish the refreshed `latest.json`). The manual `docker compose` steps in
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
| `src/pages/blog/[slug]/index.md.ts`     | Markdown mirror of each post for AI agents                |
| `src/pages/og/[slug].png.ts`            | OG image generation (Satori + resvg)                      |
| `src/pages/rss.xml.ts`                  | RSS feed                                                  |
| `src/components/Remark42Comments.astro` | Comment widget (disabled in dev)                          |
| `src/components/BlogImageClick.astro`   | Click-to-fullsize for blog images                         |

### Dev blog editor

The website has a local-only Markdown editor at `/dev/blog` while the Astro dev server is running. It is served by a
Vite dev middleware, not by an Astro page, so it is unavailable in production builds.

Start it with:

```bash
pnpm dev:website
# Open http://localhost:4829/dev/blog
```

Drafts autosave atomically to `apps/website/.blog-drafts/{draft-id}/index.md`, which is gitignored. Each draft has a
stable internal ID, so editing the publish slug does not create duplicate drafts. The draft frontmatter stores `slug`;
publishing writes the final plain blog post to `src/content/blog/{slug}/index.md`.

Images can be added with the **Add image** button, pasted into the Markdown textarea, or dropped onto it. Uploaded
images are processed through `sharp`, resized to fit within 1500x1500 without enlargement, converted to WebP, and stored
under `apps/website/.blog-drafts/{draft-id}/assets/`. The editor inserts final-form Markdown like
`![Alt text](./image.webp)` and rewrites those relative image URLs only for draft preview. Publishing copies referenced
draft images to `src/content/blog/{slug}/`, matching the production colocated-image model.

Editor files:

| File                                 | Purpose                                                               |
| ------------------------------------ | --------------------------------------------------------------------- |
| `src/dev/blog-editor/dev-server.mjs` | Dev-only Vite middleware for draft/post file operations               |
| `src/dev/blog-editor/index.html`     | Editor shell                                                          |
| `src/dev/blog-editor/entry.ts`       | Autosave, image upload, preview, publish, delete, and backup behavior |
| `src/dev/blog-editor/styles.css`     | Editor-specific CSS                                                   |

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
- **Agent-facing endpoints**: `src/pages/llms.txt.ts` (concise) and `llms-full.txt.ts` (detailed) describe Cmdr for AI
  agents; each blog post also has a Markdown mirror at `/blog/{slug}/index.md`. Keep the llms files in sync when product
  facts (pricing, features, system requirements) change. nginx serves `.md` as `text/markdown` via a dedicated location
  block in `nginx.conf`.

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
- **D1** (API server): Download redirect endpoint logs version, arch, country, source, a first-touch `ref` channel, and
  a daily-hashed IP (bot hits dropped). The download button carries `?src=website` so the endpoint can tag it as a
  website download (vs Homebrew or direct links). The `heartbeat` table holds the desktop DAU beats.

  **First-touch attribution (`ref`), storage-free.** An inline script in `Layout.astro` attributes downloads to the
  channel a visitor first arrived from (a UTM source/campaign, or an external referrer hostname). It uses NO
  localStorage/sessionStorage/cookie on purpose — device storage would count as ePrivacy storage and force a consent
  banner, which this site avoids everywhere (see the cookieless Umami/PostHog setup above). So attribution is pure URL
  state: the script computes the channel from the current page (a `?ref=` already on the URL wins, then UTM params, then
  the external referrer; same-origin and getcmdr.com count as no referrer), copies it onto same-origin links so it
  survives internal navigation, and appends it as `?ref=` to the download endpoint URLs plus a `data-umami-event-ref`
  prop. The server (`api-server` `/download` handler) re-sanitizes `ref` before storing — never trust the client value.
  Trade-off: a return visit in a later session has no URL ref and shows as direct/NULL; that's fine for anonymous
  aggregate channel attribution. If you add a new download link, give it `data-download-link` (main) or
  `data-arch` inside `[data-download-dropdown]` (option) so the ref script finds it.

**Decision/Why — client-side storage policy (no consent banner, ever)**: The site must never need a cookie consent
banner. The legal line (ePrivacy Article 5(3)) covers all device storage, not only cookies, but with an exemption for
storage that's strictly necessary / UI customization. So the rule here is preference vs tracking:

- **Preference flags are fine, no banner needed**: the theme choice, download-arch choice, and the newsletter form's
  dismissed/subscribed flags live in localStorage. They fall under the ePrivacy "UI customization" exemption (WP29
  Opinion 04/2012): no identifier, never sent anywhere, exist for the user's benefit. They're not personal data, so
  GDPR isn't triggered either. Don't flag these as a compliance problem; David has settled this.
- **Tracking storage is never okay**: anything that identifies, follows, or attributes a visitor (analytics IDs,
  attribution state, session cookies) must NOT use cookies, localStorage, or sessionStorage — that's consent-requiring
  storage and would force a banner. Track anonymously in aggregate instead, like the existing setup does: cookieless
  Umami, memory-persistence PostHog, URL-state `ref` attribution, daily-hashed IPs server-side. Anonymous aggregate
  insight is a win; individual tracking at the cost of a banner is not.

If you add analytics tooling or any feature that wants client-side persistence, preserve this split. See
`docs/tooling/umami.md` and `docs/tooling/posthog.md` for API access and config details.

## Gotchas

- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind. It doesn't
  affect the build.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- Font files for OG image generation (`inter-400.ttf`, `inter-700.ttf`) live in `public/fonts/`.
- All pages support both light and dark mode. Don't hardcode colors; use CSS variables from `global.css`.
- **Deploy order**: Always `docker compose build` before `docker compose down`. Building first keeps the old container
  serving traffic. `down → build → up` causes ~15s downtime while the image builds.
