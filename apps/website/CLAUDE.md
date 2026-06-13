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

**Must-knows** (full detail below in this section):

- `window.__cmdrRReady` gates Umami, PostHog, AND the first-touch `ref` script: all three await it before recording a
  pageview or reading `utm_source`, so a `?r=` code expands first. Gate anything new that reads `utm_source` on it too.
- Inline analytics/expansion `<script is:inline>` bodies must be raw JS, NEVER wrapped in a literal Astro `{`...`}`
  expression: Astro ships that as inert dead text and analytics silently dies. The `website-analytics-injection` check
  guards this.
- The code/UTM charset is the cross-repo attribution contract (`docs/architecture.md` § Acquisition analytics): the
  client-side `?r=` sanitizer must normalize identically to the api-server, or stored and pass-through values diverge.

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
  aggregate channel attribution. If you add a new download link, give it `data-download-link` (main) or `data-arch`
  inside `[data-download-dropdown]` (option) so the ref script finds it.

  **`?r=<code>` tracking-link expansion (storage-free).** A short, inconspicuous `?r=` code on a link (for example
  `getcmdr.com/?r=rmc`) expands to `utm_source` (+ `utm_medium`) before analytics runs, so visitors see a clean URL and
  the channel is attributed. The code → meaning map lives in Cloudflare KV and is fetched from
  `https://api.getcmdr.com/r-codes.json` (edge-cached), so a new code needs no website deploy. A known code maps to its
  stored source/medium; an unknown code passes through as a sanitized `utm_source` verbatim. Storage-free: URL state
  only (`history.replaceState`).

  **CRITICAL ordering (don't break this).** The expansion is async (a fetch), and it MUST take effect before the
  first-touch `ref` script, Umami, AND PostHog — they each read/record `utm_source` / the URL. The early `<head>` inline
  script exposes `window.__cmdrRReady`, a Promise that resolves once the URL is final (or immediately when there's no
  `?r=`). Umami and PostHog are injected via JS, and the `ref` script's first `run()` is deferred, all gated on
  `.then(__cmdrRReady)` — that's how the order holds despite the fetch. If you add anything that reads `utm_source` or
  records the pageview at load, gate it on `__cmdrRReady` too. Don't revert Umami/PostHog to static `<script>` tags: a
  static tag fires before the fetch resolves and would record the raw `?r=`.

  **Inline-script gotcha (don't wrap the body in `{\`...\`}`).** The Umami and PostHog injectors are
  `<script is:inline define:vars={...}>` blocks. Inside an `is:inline` script, the body is raw JS — Astro does NOT
  evaluate `{...}` expressions there. Writing the JS as a
  `{\`...\`}`template-literal child ships the literal backtick wrapper into the page, so the injector becomes dead text and analytics silently never loads (no error, no script tag). Author the body as plain JS, leading with`;`(matching the`\__cmdrRReady`and theme blocks).`define:vars`supplies the consts at the top; reference them directly. The `website-analytics-injection` check (`pnpm
  check
  analytics-injection`) guards this class: it builds the site WITH the `PUBLIC_\*`analytics env (the default`website-build`deliberately omits it, so its dist never renders these branches) into a separate`dist-analytics/`dir, then asserts the built HTML has a real`createElement('script')`Umami injector with`data-website-id`, a `/scripts/posthog-init.js`PostHog injector,`\_\_cmdrRReady`gating, AND no`{`-then-backtick
  inert-wrapper signature. Still worth a real-browser check for behavior the built HTML can't show, but the inert-text
  regression now fails CI.

**Decision/Why — client-side storage policy**: The site must never need a cookie consent banner.

- Preference flags (theme, download arch, newsletter dismissed/subscribed) in localStorage are fine and settled — don't
  flag them as a compliance problem.
- Anything that identifies, follows, or attributes a visitor must NOT use cookies, localStorage, or sessionStorage.
  Track anonymously in aggregate instead (cookieless Umami, memory-persistence PostHog, URL-state `ref`, daily-hashed
  IPs server-side).
- Legal reasoning and the preference-vs-tracking test: [DETAILS.md](DETAILS.md) § Client-side storage policy. Apply that
  test to any new feature that wants client-side persistence.

See `docs/tooling/umami.md` and `docs/tooling/posthog.md` for API access and config details.

## Gotchas

- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind. It doesn't
  affect the build.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- Font files for OG image generation (`inter-400.ttf`, `inter-700.ttf`) live in `public/fonts/`.
- All pages support both light and dark mode. Don't hardcode colors; use CSS variables from `global.css`.
- **Deploy order**: Always `docker compose build` before `docker compose down`. Building first keeps the old container
  serving traffic. `down → build → up` causes ~15s downtime while the image builds.
