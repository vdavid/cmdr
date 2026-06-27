# Website details

Deep docs for `apps/website`. The always-loaded counterpart is [CLAUDE.md](CLAUDE.md).

## Client-side storage policy (no consent banner, ever)

The site must never need a cookie consent banner. This section holds the legal reasoning; the imperatives live in
`CLAUDE.md` § Analytics.

The banner requirement comes from the ePrivacy directive (Article 5(3)), not GDPR itself. It covers ALL storage on the
visitor's device (cookies, localStorage, sessionStorage alike), but exempts storage that's strictly necessary for a
service the user requested, including "UI customization" (WP29 Opinion 04/2012, still the reference guidance). So the
line that matters is **preference vs tracking**:

- **Preference flags need no banner.** The theme choice, download-arch choice, and the newsletter form's
  dismissed/subscribed flags live in localStorage. They carry no identifier, never leave the device, and exist for the
  user's benefit — squarely inside the UI-customization exemption. They're not personal data, so GDPR isn't triggered
  either. David has settled this; don't re-raise it or flag it as a compliance problem.
- **Tracking storage is never okay.** Anything that identifies, follows, or attributes a visitor (analytics IDs,
  attribution state, session cookies) is consent-requiring storage and would force a banner. The trade-off is settled:
  anonymous aggregate insight is a win; individual tracking at the cost of a banner is not.

How the site tracks anonymously instead:

- Umami: cookieless by design, proxied to dodge adblockers.
- PostHog: `persistence: 'memory'` and `person_profiles: 'identified_only'`, so no device storage.
- Download attribution: pure URL state (`?ref=`, see `CLAUDE.md` § Analytics), nothing persisted client-side.
- Server side: daily-hashed IPs, no raw IP storage.

When adding analytics tooling or any feature that wants client-side persistence, classify it with the preference vs
tracking test above. Preference: localStorage is fine. Tracking: find a storage-free design (URL state, server-side
aggregation) or drop the feature.

## Blog

Blog posts live in `src/content/blog/{slug}/index.md` with colocated images. Add a post: see
`docs/guides/writing-blog-posts.md`.

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
Vite dev middleware, not by an Astro page, so it is unavailable in production builds. Start it with `pnpm dev:website`,
then open `http://localhost:4829/dev/blog`.

The editor distinguishes two kinds of entry, and which one you have determines where autosave writes:

- **Drafts** are unpublished work in progress, keyed by a stable internal ID, autosaving atomically to
  `apps/website/.blog-drafts/{draft-id}/index.md` (gitignored). The draft frontmatter stores `slug`; editing the slug
  does not fork a new draft. **Publish** writes the final plain blog post to `src/content/blog/{slug}/index.md`, then
  retires the source draft (so you never accumulate a draft + post pair for the same article) and switches the editor to
  editing the now-live post.
- **Published posts** are the source of truth, so they are edited in place: selecting one from the dropdown loads it
  with no draft fork, and autosave writes straight back to `src/content/blog/{slug}/index.md` (a `PUT /posts/{slug}`,
  which only ever overwrites an existing post, never creates one). Because the slug is the post's folder and URL, the
  slug field and the publish controls are disabled while editing a post. These edits churn the git-tracked working tree
  by design; commit them when the post is ready.

Decision/Why: posts are editable in place rather than always round-tripping through a draft, because the older
draft-only model minted a fresh random draft on every reload-and-edit of a published post, piling up duplicate drafts of
one article with no way to edit the post itself.

Images can be added with the **Add image** button, pasted into the Markdown textarea, or dropped onto it. Uploaded
images are processed through `sharp`, resized to fit within 1500x1500 without enlargement, and converted to WebP. For a
draft they go under `apps/website/.blog-drafts/{draft-id}/assets/`; for an in-place post edit they go colocated directly
in `src/content/blog/{slug}/` (matching the production layout). The editor inserts final-form Markdown like
`![Alt text](./image.webp)` and rewrites those relative URLs for preview only (to the draft- or post-asset endpoint per
kind). Publishing a draft copies its referenced images next to the post.

| File                                 | Purpose                                                               |
| ------------------------------------ | --------------------------------------------------------------------- |
| `src/dev/blog-editor/dev-server.mjs` | Dev-only Vite middleware for draft/post file operations               |
| `src/dev/blog-editor/index.html`     | Editor shell                                                          |
| `src/dev/blog-editor/entry.ts`       | Autosave, image upload, preview, publish, delete, and backup behavior |
| `src/dev/blog-editor/styles.css`     | Editor-specific CSS                                                   |

### OG images

Generated at build time with Satori (JSX to SVG) and resvg (SVG to PNG). Colors are hardcoded because Satori doesn't
support CSS variables; keep them in sync with `global.css` theme values. Fonts (`inter-400.ttf`, `inter-700.ttf`) live
in `public/fonts/`.

## Patterns

- **Layouts**: `Layout.astro` (base), `BlogLayout.astro` (posts), `LegalLayout.astro` (terms, privacy)
- **CSS variables**: defined in `src/styles/global.css` under `@theme`. Use them everywhere.
- **External links**: `rehype-external-links` auto-adds `target="_blank" rel="noopener noreferrer"`
- **Download dropdown**: the split-button + arch menu. `DownloadButton.astro` holds only markup (variants
  `hero`/`card`/`header`/`mobile`/`pricing`); its styling is global in `src/styles/download-button.css` and its
  open/close + keyboard JS is a global `astro:page-load` script in `Layout.astro`, both keyed off
  `[data-download-split-btn]`/`[data-download-chevron]`/`[data-download-dropdown]`. Arch auto-detection (the recommended
  ✓) is a separate inline script in `Layout.astro`. Because all three are global and attribute-driven, the dropdown also
  works when the `rehypeDownloadDropdown` plugin (`src/plugins/download-dropdown.mjs`) emits an inline copy into a blog
  post from the `[download](cmdr:download)` marker. That plugin must run after `rehype-external-links` (so its GitHub
  links don't get `target="_blank"`) and reads `public/latest.json` directly, mirroring `src/lib/release.ts`'s
  GitHub-fallback URL/size logic.
- **RSS autodiscovery**: `<link>` tag in `Layout.astro`
- **Agent-facing endpoints**: `src/pages/llms.txt.ts` (concise) and `llms-full.txt.ts` (detailed) describe Cmdr for AI
  agents; each blog post also has a Markdown mirror at `/blog/{slug}/index.md`. Keep the llms files in sync when product
  facts (pricing, features, system requirements) change. nginx serves `.md` as `text/markdown` via a dedicated location
  block in `nginx.conf`.

## Icons

All icons are monochrome gold line-art Lucide glyphs, inline-SVG, rendered through one `<Icon>` component. No per-site
`.svg` files, no `~icons/*` imports outside the registry, no emoji for decorative/marker use. This mirrors the desktop
app's `apps/desktop/src/lib/ui/icons/` system (an `<Icon>` plus a single `icon-map`), so the two apps stay aligned. The
site used to ship ~25 hand-authored `public/icons/*.svg` (Lucide glyphs with the accent stroke baked in) plus scattered
roadmap emoji; consolidating gives one look, one place to add a glyph, and real `currentColor` theming.

How it's wired:

- `unplugin-icons` + `@iconify-json/lucide` (devDeps) resolve `~icons/lucide/<name>` to inline-SVG Astro components. The
  Vite plugin is `Icons({ compiler: 'astro' })` in `astro.config.mjs`; the `~icons/*` module type is declared in
  `src/env.d.ts` so `astro check` resolves the imports.
- `src/components/icons/icon-map.ts` is the ONE place `~icons/lucide/*` is imported. It exports `ICONS` (a
  `name → glyph` registry, keyed by the Lucide kebab name) and `IconName` (the union of registered names).
- `src/components/icons/Icon.astro` looks the name up and renders the glyph. Props: `name` (`IconName`), `size` (px,
  default 24), `class` (passthrough). It throws on an unregistered name, so a typo fails the build.

Using it:

- `<Icon name="rocket" size={40} />`. Default gold comes from `.icon { color: var(--color-accent) }` plus Lucide's
  `currentColor` stroke; recolor by passing a `class` (e.g. a Tailwind `text-[…]`).
- When a data array feeds `<Icon name={…}>` (the `Features.astro` / `features.astro` grids), type its `icon` field as
  `IconName` via `satisfies { …; icon: IconName }[]` so a bad name is a compile error at the data site.
- **Size is applied as CSS `width`/`height`, never as width/height attributes.** Lucide glyphs already ship
  `width="1.2em" height="1.2em"`; passing width/height props too emits duplicate attributes, which `html-validate`'s
  `no-dup-attr` rejects (it runs over `dist/**/*.html`). `Icon.astro` sizes via a scoped `.icon :global(svg)` rule.

Adding a glyph: find the name at [lucide.dev/icons](https://lucide.dev/icons) (it must exist in the installed
`@iconify-json/lucide`; some are recent renames, e.g. `chart-pie` not `pie-chart`), import it from
`~icons/lucide/<name>` in `icon-map.ts`, and register it under that kebab name. `IconName` updates automatically.

Emoji policy: no emoji for decorative or marker use. The one sanctioned exception is the roadmap's Linux milestone 🐧
(Lucide ships no penguin, and a penguin from another icon set would mean a second, off-style stroke). Genuinely tonal
in-prose emoji (the `❤️` in the newsletter CTA, the `😅` on a roadmap note) are a deliberate copy/voice choice owned by
a human (`AGENTS.md` principle 6), kept on purpose; don't bulk-swap those for glyphs.

## Color scheme (light/dark mode)

All pages support both light and dark mode. Users can override their system preference via a theme toggle in the header.

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

The website uses three analytics layers. The desktop app separately sends anonymous beta usage analytics: an
`anal_`-keyed hourly heartbeat (true DAU) plus curated PII-free PostHog feature events, all behind a tri-state opt-out
and stripped of file names, paths, queries, and prompts by allowlist. See
`apps/desktop/src-tauri/src/analytics/CLAUDE.md`.

- **Umami** (`Layout.astro`): Cookieless page analytics (pageviews, referrers, geo, UTM). Self-hosted. Script served at
  `/u/mami` (proxied through Caddy to avoid adblockers).
- **PostHog** (`public/scripts/posthog-init.js`): Session replay, heatmaps, click tracking. Configured with
  `persistence: 'memory'` (no cookies, no localStorage) and `person_profiles: 'identified_only'` (no anonymous person
  profiles). This keeps PostHog fully cookieless. The same EU project also receives the desktop app's
  `source: "desktop"` feature events.
- **D1** (API server): Download redirect endpoint logs version, arch, country, source, a first-touch `ref` channel, and
  a daily-hashed IP (bot hits dropped). The download button carries `?src=website` so the endpoint can tag it as a
  website download (vs Homebrew or direct links). The `heartbeat` table holds the desktop DAU beats.

### First-touch attribution (`ref`), storage-free

An inline script in `Layout.astro` attributes downloads to the channel a visitor first arrived from (a UTM
source/campaign, or an external referrer hostname). It uses NO localStorage/sessionStorage/cookie on purpose: device
storage would count as ePrivacy storage and force a consent banner, which this site avoids everywhere. So attribution is
pure URL state: the script computes the channel from the current page (a `?ref=` already on the URL wins, then UTM
params, then the external referrer; same-origin and getcmdr.com count as no referrer), copies it onto same-origin links
so it survives internal navigation, and appends it as `?ref=` to the download endpoint URLs plus a
`data-umami-event-ref` prop. The server (`api-server` `/download` handler) re-sanitizes `ref` before storing: never
trust the client value. Trade-off: a return visit in a later session has no URL ref and shows as direct/NULL; that's
fine for anonymous aggregate channel attribution. If you add a new download link, give it `data-download-link` (main) or
`data-arch` inside `[data-download-dropdown]` (option) so the ref script finds it.

### `?r=<code>` tracking-link expansion (storage-free)

A short, inconspicuous `?r=` code on a link (for example `getcmdr.com/?r=rmc`) expands to `utm_source` (+ `utm_medium`)
before analytics runs, so visitors see a clean URL and the channel is attributed. The code → meaning map lives in
Cloudflare KV and is fetched from `https://api.getcmdr.com/r-codes.json` (edge-cached), so a new code needs no website
deploy. A known code maps to its stored source/medium; an unknown code passes through as a sanitized `utm_source`
verbatim. Storage-free: URL state only (`history.replaceState`).

**Critical ordering (don't break this).** The expansion is async (a fetch), and it MUST take effect before the
first-touch `ref` script, Umami, AND PostHog: they each read/record `utm_source` / the URL. The early `<head>` inline
script exposes `window.__cmdrRReady`, a Promise that resolves once the URL is final (or immediately when there's no
`?r=`). Umami and PostHog are injected via JS, and the `ref` script's first `run()` is deferred, all gated on
`.then(__cmdrRReady)`: that's how the order holds despite the fetch. If you add anything that reads `utm_source` or
records the pageview at load, gate it on `__cmdrRReady` too. Don't revert Umami/PostHog to static `<script>` tags.

**Inline-script gotcha (don't wrap the body in ``{`...`}``).** The Umami and PostHog injectors are
`<script is:inline define:vars={...}>` blocks. Inside an `is:inline` script, the body is raw JS: Astro does NOT evaluate
`{...}` expressions there. Writing the JS as a ``{`...`}`` template-literal child ships the literal backtick wrapper
into the page, so the injector becomes dead text and analytics silently never loads (no error, no script tag). Author
the body as plain JS, leading with `;` (matching the `__cmdrRReady` and theme blocks); `define:vars` supplies the consts
at the top. The `website-analytics-injection` check (`pnpm check analytics-injection`) guards this class: it builds the
site WITH the `PUBLIC_*` analytics env into a separate `dist-analytics/` dir, then asserts the built HTML has a real
`createElement('script')` Umami injector with `data-website-id`, a `/scripts/posthog-init.js` PostHog injector,
`__cmdrRReady` gating, and no inert-wrapper signature. Still worth a real-browser check for behavior the built HTML
can't show.
