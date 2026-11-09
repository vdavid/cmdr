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

Decision/Why: every write goes through `assertSameOriginWrite`, which refuses a foreign `Origin` and requires
`application/json` on any request carrying a body. The API is unauthenticated because it's your own machine editing your
own repo, but while `pnpm dev:website` runs, any page open in your browser can reach `localhost:4829`, and a `fetch`
with a simple content type isn't preflighted. Without the gate, a site you merely visited could publish a post into
`src/content/blog/` or write image files into the repo, and the first sign would be an unexplained `git status`. A
missing `Origin` stays allowed (curl, scripts); `DELETE` skips the content-type rule because it carries no body here.
DNS rebinding needs no handling of ours: Vite's `allowedHosts` check rejects a foreign `Host` before this middleware
runs (verified on vite 8.2.0 with `Host: attacker.example`, 2026-08-12).

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

### Content cache

Astro's content layer stores each post's rendered HTML in `.astro/data-store.json` (gitignored), keyed by the post's own
text. Editing a remark/rehype plugin in `src/plugins/` (a new inline icon, a changed transform) doesn't invalidate it,
so posts you didn't touch keep their old render in dev AND in a local `astro build`, and a dev server also holds the
plugin modules it loaded at start. Delete the file and restart dev. CI and deploys build from a clean checkout, so
production is unaffected. (Verified on Astro 7.3.8, 2026-10-23: a new `:partial:` icon stayed raw text until the file
was deleted.)

## Patterns

- **Layouts**: `Layout.astro` (base), `BlogLayout.astro` (posts), `LegalLayout.astro` (terms, privacy)
- **CSS variables**: defined in `src/styles/global.css` under `@theme`. Use them everywhere.
- **Markdown pipeline** (Astro 7): the remark/rehype plugins run inside `markdown.processor: unified({…})` in
  `astro.config.ts` (Astro 7 requires an explicit processor); `shikiConfig` stays top-level and is handed to the
  processor's renderer. `unified()`'s built-in gfm + SmartyPants stay on by default. The `@ts-expect-error` on
  `remarkSmartypants` covers its generic-`Node` types vs Astro's `Root` — genuinely needed (`astro check` flags it as
  unused if removed). `compressHTML: true` is deliberate: Astro 7's `'jsx'` default strips significant inter-element
  whitespace and collapses the home + pricing layouts (which rely on it), so `true` (the classic minifier) is what keeps
  the render correct. Astro's image service emits an invalid empty `srcset=""` on single-candidate markdown images; the
  `stripEmptySrcsetIntegration` (`src/plugins/strip-empty-srcset.ts`) removes it at `astro:build:done` (a rehype plugin
  can't — Astro injects the srcset after user plugins run).
- **External links**: `rehype-external-links` auto-adds `target="_blank" rel="noopener noreferrer"`
- **Changelog rendering**: `src/lib/changelog.ts` linkifies `CHANGELOG.md`'s bare commit hashes
  (`scripts/check/checks/DETAILS.md` § "CHANGELOG commit refs") and gives each release heading the anchor `v<version>`,
  like `#v0.58.0`. ❌ Don't change that form: the app's security notices link to it from already shipped builds
  (`apps/desktop/src-tauri/src/security_advisories/DETAILS.md`). The dots are why `.htmlvalidate.json` sets `valid-id`
  to `relaxed`: HTML allows them, the rule's stricter default doesn't. `scroll-margin-top` on the page's `h2` keeps a
  jumped-to heading clear of the fixed header.
- **Download dropdown**: the split-button + arch menu. `DownloadButton.astro` holds only markup (variants
  `hero`/`card`/`header`/`mobile`/`pricing`); its styling is global in `src/styles/download-button.css` and its
  open/close + keyboard JS is a global `astro:page-load` script in `Layout.astro`, both keyed off
  `[data-download-split-btn]`/`[data-download-chevron]`/`[data-download-dropdown]`. Arch auto-detection (the recommended
  ✓) is a separate inline script in `Layout.astro`. Because all three are global and attribute-driven, the dropdown also
  works when the `rehypeDownloadDropdown` plugin (`src/plugins/download-dropdown.ts`) emits an inline copy into a blog
  post from the `[download](cmdr:download)` marker. That plugin must run after `rehype-external-links` (so its mirror
  links don't get `target="_blank"`) and reads `public/latest.json` directly, mirroring `src/lib/release.ts`'s
  no-download-base URL/size logic (direct links to `download.getcmdr.com`).
- **`public/latest.json` is live only in production.** The committed copy is frozen (dev and the check lanes build with
  it, so they never touch the network for it). The Docker build overwrites it with the live update manifest, the
  `latest.json` of the GitHub release marked "Latest" (`scripts/fetch-live-manifest.ts`), and a failed fetch fails the
  build so the deploy keeps the old site. So every deploy serves, and bakes into the download buttons, whatever release
  is current; the release pipeline commits nothing here. ❌ Never read the current version from the committed copy. Why:
  `docs/guides/releasing.md` § What a release publishes.
- **Caching**: three tiers in `nginx.conf`. `/_astro/` (Vite's hashed bundles) and any `public/` file requested with
  `?v=` get a year, `immutable`; HTML and an unversioned `public/` file get `no-cache` (a cheap 304 when unchanged).
  `src/plugins/asset-versions.ts` adds `?v=<content hash>` at `astro:build:done` to every `public/` asset URL in the
  built HTML (attributes, `srcset`, and `url()` in the inlined CSS), so a changed hero or blog image is a new URL the
  moment it deploys. Before this, those files kept their names and Cloudflare handed browsers a 4-hour TTL, so an
  updated hero stayed stale until a hard reload. `website-dead-links` fails a page that references a `public/` asset
  without `?v=`. ❗ A URL built at runtime in JS isn't versioned; it's served `no-cache`, which is correct but uncached.
  Cloudflare's Browser Cache TTL is "Respect existing headers", so `nginx.conf` alone decides (zone setting
  `browser_cache_ttl: 0`).
- **404s**: any URL with no page or file behind it is a real 404 rendering `src/pages/404.astro`, never the home page as
  a 200 (search engines treat that as a soft 404, and a broken link looks fine).
- **Security headers**: every `nginx.conf` location that serves content includes `nginx-security-headers.conf` (copied
  to `/etc/nginx/snippets/` by the Dockerfile), because nginx drops server-level `add_header`s in any location that sets
  its own. It carries the site's only copy of the CSP (`/demo/` has its own, § Browser demo), and pulls the headers both
  share from `nginx-security-headers-base.conf`. `connect-src` has to list every origin the site's JS fetches
  (`api.getcmdr.com` for likes and `?r=` codes, remark42, Paddle): the browser blocks the rest before the request
  leaves, and nothing in dev shows it, since Astro's dev server and the Playwright E2E serve no CSP. The blog's like
  button shipped that way and never worked until the origin was added. Two guards: the `website-csp-connect-src` check
  matches every fetch in `src/` and `public/` against `connect-src` (`scripts/check/checks/DETAILS.md` § Website CSP
  connect-src), skipping `src/dev/` and `src/build/` (build-time-only modules, imported from component frontmatter and
  ❌ never from a client `<script>`), and the CSP's `report-uri` / `report-to` send production violations to the API
  server, which alerts Discord (`apps/api-server/src/website/DETAILS.md` § CSP reports).
- **Inline scripts by hash (report-only for now)**: the enforced CSP still has `'unsafe-inline'` in `script-src`. A
  second header, `Content-Security-Policy-Report-Only`, is the policy we're moving to: inline scripts allowed only by
  hash, images only from us, remark42, and Paddle, plus `object-src 'none'` and `base-uri 'self'`.
  `src/plugins/csp-hashes.ts` hashes every executable inline `<script>` in the built HTML into
  `nginx-generated/csp-script-hashes.conf` (gitignored, copied by the Dockerfile), which sets `$csp_script_hashes` for
  that header. It's one union across all pages because `<ClientRouter />` runs the next page's inline scripts under the
  current page's policy, and it runs after every other integration because a hash covers exact bytes. Astro's built-in
  `security.csp` can't do this: it doesn't support `<ClientRouter />` and only emits a `<meta>` policy, which can't
  report. Report-only violations land in the Workers log, not an alert. To enforce, move the strict policy into
  `Content-Security-Policy` once that log shows nothing of ours, with a Paddle sandbox checkout in between. Verified
  with zero violations across five pages and client-side navigation in Chrome (local `nginx:alpine`, 2026-10-23).
- **Edge caching**: nginx sends `Cloudflare-CDN-Cache-Control` (edge-only, browsers ignore it) on pages and markdown (an
  hour) and on `.json` like `latest.json` (a minute); browsers still get `no-cache`. Cloudflare's cache rule for
  getcmdr.com caches only responses that declare a lifetime, so Caddy's other routes on the host (Listmonk, Umami,
  PostHog, the deploy hook) and nginx's `/download/latest` redirects stay uncached. Every deploy purges the zone
  (`infra/deploy-webhook/deploy-website.sh`). Link-preview bots (`TelegramBot`, `Twitterbot`, and so on) skip the edge
  cache, so they always reach nginx's access log; human page views served from the edge don't, and Umami counts them.
  nginx doesn't compress: Cloudflare only compresses with Brotli or zstd when the origin sends plain bytes (the demo's
  precompressed bundle is the one exception, § Browser demo). The rule itself lives in the infra repo
  (`cloudflare/getcmdr.com-cache.tf`).
- **Always-latest download links**: `getcmdr.com/download/latest/<arch>` (and bare `/download/latest`, which hands out
  the universal build) is an nginx `return 302` to the API server's `/download/latest/<arch>`, query string preserved so
  `?ref=` and `?src=` survive. It lives in `nginx.conf`, not Astro, because a static build can only emit a meta-refresh
  page, which a directory's link checker may not follow, and because the target has to stay correct between deploys. The
  API server resolves `latest` to the current version and logs the download (`apps/api-server/src/telemetry/DETAILS.md`
  § Download tracking). Only the production container serves it: `pnpm dev` and the Playwright E2E run against Astro's
  dev server, which knows nothing about `nginx.conf`. Use it for links published where we can't edit the URL per release
  (app directories, the README, blog posts); the site's own download button keeps its version-pinned URL, which carries
  the `?src=website` tag and the file size.
- **Page content as data**: pages whose bulk is content, not logic, keep that content in a typed module under `src/lib/`
  and map over it. `src/lib/roadmap.ts` holds every roadmap milestone (sections → optional month groups → milestones,
  each with a `date`, `title`, `description`, optional `icon`, and `done`); `src/pages/roadmap.astro` is layout and
  styling only. Titles and descriptions are inline HTML rendered with `set:html`, so keep them plain text unless markup
  is genuinely needed, and note that markup inside those strings gets no `data-astro-cid-…` scope attribute: it can't be
  targeted by the page's scoped `<style>`. Style such elements globally or wrap them in a classed span the template
  owns.
- **RSS autodiscovery**: `<link>` tag in `Layout.astro`
- **Agent-facing endpoints**: `src/pages/llms.txt.ts` (concise) and `llms-full.txt.ts` (detailed) describe Cmdr for AI
  agents; each blog post also has a Markdown mirror at `/blog/{slug}/index.md`. Keep the llms files in sync when product
  facts (pricing, features, system requirements) change. nginx serves `.md` as `text/markdown` via a dedicated location
  block in `nginx.conf`.
- **Sample MDM files**: `public/mdm/` holds the sample `.mobileconfig` profile and the bare `.plist` linked from
  `/trust#mdm`. nginx's `/mdm/` location gives them `application/x-apple-aspen-config` and `application/x-plist` (the
  default mime table knows neither). The page's key list renders from `managedPreferenceKeys` in `src/lib/trust-mdm.ts`.
  A Rust test guards all three key sets against the app's (`apps/desktop/src-tauri/src/managed_policy/DETAILS.md` § The
  public mirror), so a key edit here needs the app's key list to match.

## Icons

All icons are monochrome gold line-art Lucide glyphs, inline-SVG, rendered through one `<Icon>` component. No per-site
`.svg` files, no `~icons/*` imports outside the registry, no emoji for decorative/marker use. This mirrors the desktop
app's `apps/desktop/src/lib/ui/icons/` system (an `<Icon>` plus a single `icon-map`), so the two apps stay aligned. The
site used to ship ~25 hand-authored `public/icons/*.svg` (Lucide glyphs with the accent stroke baked in) plus scattered
roadmap emoji; consolidating gives one look, one place to add a glyph, and real `currentColor` theming.

How it's wired:

- `unplugin-icons` + `@iconify-json/lucide` (devDeps) resolve `~icons/lucide/<name>` to inline-SVG Astro components. The
  Vite plugin is `Icons({ compiler: 'astro' })` in `astro.config.ts`; the `~icons/*` module type is declared in
  `src/env.d.ts` so `astro check` resolves the imports.
- `src/components/icons/icon-map.ts` is the ONE place `~icons/lucide/*` is imported. It exports `ICONS` (a
  `name → glyph` registry, keyed by the Lucide kebab name) and `IconName` (the union of registered names).
- `src/components/icons/Icon.astro` looks the name up and renders the glyph. Props: `name` (`IconName`), `size` (px,
  default 24), `class` (passthrough). It throws on an unregistered name, so a typo fails the build.

Using it:

- `<Icon name="rocket" size={40} />`. Default gold comes from `.icon { color: var(--color-accent) }` plus Lucide's
  `currentColor` stroke; recolor by passing a `class` (e.g. a Tailwind `text-text-secondary`).
- When a data array feeds `<Icon name={…}>` (the `features.astro` grid, the `Showcase.astro` reach strip), type its
  `icon` field as `IconName` via `satisfies { …; icon: IconName }[]` so a bad name is a compile error at the data site.
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
- Shiki is configured with dual themes (`github-dark` + `github-light`) and `defaultColor: false` in `astro.config.ts`.
  The theme switching CSS is in `global.css`.
- Remark42 comments detect `data-theme` attribute, falling back to `prefers-color-scheme`.
- Hero images have dark and light variants; `Hero.astro` switches between them using the same CSS selector pattern.

**Decision/Why**: The dual-selector pattern (media query + `data-theme` attribute) ensures the site works correctly
without JS (falls back to system preference) while supporting explicit overrides via the toggle.

## Product clips

The landing page leads with short screen recordings of the app (`Showcase.astro`, order and pitch from issue #693):
trust (copy and rollback, the queue), reach (two clips plus a card strip), multi-rename, the viewer, optional AI, and
craft. The markup and styles are `ProductClip.astro`; its browser runtime is `src/client/product-clip-player.ts`, which
the component's `<script>` imports. `src/client/` is where a component's script goes once it outgrows inline; eslint
gives it browser globals.

- **Files**: `pnpm marketing:clips` (apps/desktop) writes `public/clips/{slug}-{light,dark}-{av1,h264}.mp4`, a
  `{slug}-{theme}.webp` poster per theme, and `clips.json` (`{ clips: [{ slug, width, height, duration, themes }] }`).
  The contract and the "available" rule (manifest entry, both H.264 files, and both posters; AV1 optional) live in
  `src/build/product-clips.ts`, read at build time.
- **Player** (`ProductClip.astro`): no `src` until the clip scrolls near the viewport, then AV1 with H.264 as the
  fallback (Safari decodes AV1 only in hardware), for the active theme. A theme toggle swaps the files and keeps the
  position. Offscreen pauses. A play/pause button is always reachable (WCAG 2.2.2); with `prefers-reduced-motion` the
  clip waits on its poster until clicked. Posters are lazy `<img>` pairs hidden per theme by CSS, so only the visible
  one downloads, the same reasoning as the hero's background images.
- **Progress and scrubbing**: a 2 px accent line along the bottom shows the position, redrawn per animation frame only
  while playing (with reduced motion it steps on `timeupdate` instead of gliding). The whole frame is a `role="slider"`
  overlay with `touch-action: pan-y`: a press without travel toggles play, a horizontal drag seeks relative to where it
  started (full width spans the clip) and pauses only for the drag's length, and when focused, arrows (1 s), Page
  Up/Down (5 s), Home, and End seek while Space and Enter toggle. A seek before the clip has loaded loads it first.
  **Decision/Why**: the slider carries the clip's description through `aria-describedby`, so the `<video>` stays
  `aria-hidden` (html-validate rejects naming a video) and the slider and the play/pause button are the honest controls.
- **Key captions** (KeyCastr-style): an optional `public/clips/{slug}-{theme}.keys.json` (`{ keys: [{ at, label }] }`,
  `at` in seconds from the encoded clip's start) is read at build time by `parseClipKeys`, which drops invalid entries,
  and lands on the clip as `data-keys-{theme}`. The player shows the presses from the last 1.6 s as a row of caps in the
  bottom tenth (up to five, newest last), sized in container units so phone and desktop match, with a short pop-in and a
  fade over the last 0.4 s. It's computed from `currentTime`, never from events, so scrubbing, seeking, looping, and a
  theme swap (each theme has its own timings) need nothing special. Reduced motion drops the pop and the fade. The
  overlay is `aria-hidden` (the description covers it) and waits for `data-ready`. `/clip-fixture`
  (`src/pages/clip-fixture.astro`, keys from `src/fixtures/clip-keys.json`, unlinked, `noindex`, out of the sitemap)
  lets the E2E test timing without depending on recorded keys.
- **Third-party media in a recording** gets a `credit` prop on its `ProductClip` (kind, linked title, author, linked
  license), rendered as a small tertiary line under that clip only. No clip carries one right now.
- **Window chrome is CSS**: the recordings have no alpha, so the rounded corners, hairline rim, and shadow come from
  `.clip-window`.
- **A missing clip** (no manifest entry, or a missing H.264 file or poster) renders a placeholder card labeled as one,
  so a half-recorded set never breaks the build. The homepage E2E asserts every landing clip renders as a video, which
  catches a `clips.json` shape drift that would silently turn them all into placeholders; only `reach-network` and
  `reach-devices` may be placeholders while they wait to be recorded.

## Browser demo

`/demo/` serves the real desktop frontend on a fake backend, with the listing engine built to WASM (issue #302; the app
side is `apps/desktop/src/lib/demo/CLAUDE.md`, the engine `crates/cmdr-listing-wasm/CLAUDE.md`). The home page's hero
embeds it (below), and `/demo/` is a page of its own too, in the sitemap and `llms.txt`.

- **Indexing**: search engines list `/demo/` and nothing else under it. The app's own routes (`/demo/settings` and the
  like, its secondary windows) get the same page from the SPA fallback, so the page's canonical link names `/demo/`, and
  nginx sends `X-Robots-Tag: noindex, nofollow` for every path under `/demo/` except the page itself (`$demo_robots` in
  `nginx.conf`, read off `$request_uri`, since `try_files` rewrites `$uri`). The sitemap lists `/demo/` through
  `customPages` in `astro.config.ts`, only on a `CMDR_WEBSITE_DEMO=1` build, and its `filter` drops any other `/demo/`
  URL. `website-docker-build` probes the header both ways when a local demo is built.

- **The hero embed** (`src/components/HeroLiveDemo.astro`): a same-origin iframe of `/demo/` at a fixed desktop size,
  scaled to fit a tilted window. The iframe gets its `src` only after the page's `load` and an idle callback, so the
  demo's bundle and WASM engine never count against the home page's own load or its Lighthouse scores. Same-origin is
  load-bearing: the script reads pointer moves inside the frame (for the glow and the lean) and catches the first
  `pointerdown` there. The app focuses its pane on boot, so the script hands focus back to the page until the visitor
  clicks into the frame or tabs to it; otherwise Space and the arrows on the home page would drive the app. It's live on
  phones too, with a narrower layout (`--demo-w` / `--demo-h`) so its text stays legible. The site CSP's
  `frame-src 'self'` (`nginx-security-headers.conf`) is what lets it load in production.
- **Dev**: `pnpm dev` serves `/demo/` from `apps/desktop/build-demo/` (a dev-server middleware in `src/plugins/demo.ts`,
  file else app page, like nginx), so the hero is live locally once `pnpm build:demo` has run; without it, the window
  shows a one-line "no demo build" note.

- **Build**: `pnpm build:demo` at the repo root (the WASM engine, then `apps/desktop`'s `build:demo` into
  `apps/desktop/build-demo/`). Then `CMDR_WEBSITE_DEMO=1 pnpm build` here: `src/plugins/demo.ts` copies it into
  `dist/demo/`, gives the page its own `<title>`, description, canonical link, and link-preview tags (`demoPageHead`;
  the copy is a draft David reviews), and writes `nginx-generated/csp-demo-script-hashes.conf` (the page's inline-script
  hashes). With the flag set, a missing demo build fails the site build; without it, the site builds as always and the
  snippet is written empty.
- **Decision: opt-in, not part of every `pnpm build`.** The demo needs Rust, the wasm32 target, `wasm-bindgen`, and Go,
  so building it on every website build would put that toolchain under the website checks, local dev, and Lighthouse.
  The places that ship or test the site set the flag: the Dockerfile (on by default) and CI's `website` job.
- **Decision: CI builds it, the server only copies it.** The Hetzner box also runs Caddy, Umami, Photato, and the
  grocery apps, and the demo's Vite build alone peaks around 1.7 GB RSS (with 2.6 GB free and no swap, 2026-11-04), so a
  Rust or Vite build there risks OOM-killing them. The release pipeline pushes the bundle as a `FROM scratch` image
  (`demo-image.Dockerfile`) to `ghcr.io/getcmdr/cmdr-demo`, with `GITHUB_TOKEN` and `packages: write` (no new secret).
  The deploy keeps `--no-cache`.
- **Decision: the demo ships at release cadence.** The site serves the LAST RELEASE's demo, not `main`'s. Building it
  per land put ~4 min of wasm builds in the Rust lane and a demo build in the website lane on every land run, for a page
  that shows what users can download, which is the release. `release-pipeline.yml`'s `demo` job builds both wasm engines
  with the listing engine's smoke run and golden traces (`desktop-rust-listing-wasm`, `desktop-rust-viewer-wasm`), runs
  `pnpm build:demo` on the tagged sources, runs the full demo suite against it, pushes `cmdr-demo:<tag>`, and attests it
  (SLSA provenance naming `release-pipeline.yml`, pushed beside the image). `publish` needs it, so a release whose demo
  doesn't build or pass doesn't ship. The nightly CI run also builds both engines (`rust-lane.yml`'s `wasm` job) and the
  demo with its full suite (`website-lane.yml`'s `demo-suite` job), so a host-only dep reaching the engine or a change
  that breaks the demo surfaces the next morning, not on release day. A change to the desktop frontend or the engine
  reaches `/demo/` with the next release; a change to the site itself still deploys on land.
- **Decision: the source of truth is GitHub's "Latest" release, resolved at use.** `scripts/released-demo.sh` reads the
  Latest release's tag, resolves `cmdr-demo:<tag>` to its digest, and runs `gh attestation verify` against
  `release-pipeline.yml` as signer, so a tag moved by anything but the release pipeline is refused. Why this over a file
  the release commits: "Latest" is already the one pointer to the live release (the site's `latest.json` and the release
  guard read it, `docs/guides/releasing.md`), `publish` flips it only after the demo exists, a rollback
  (`RELEASE_REPUBLISH_TAG`) moves the demo with it, and `main` only takes commits CI passed, so a release commit there
  would need a land of its own. Releases up to 0.59.0 published no image: the script maps them to the demo `main`
  deployed at the cutover, pinned by digest in the script.
- **Decision: the build pins the image by digest.** The Dockerfile's `demo-bundle` stage is
  `FROM ghcr.io/getcmdr/cmdr-demo@sha256:${DEMO_IMAGE_DIGEST}` (a tag can be moved, and Scorecard's Pinned-Dependencies
  accepts this `@sha256:${ARG}` form). The deploy sends the digest, with the release tag for the log, in the HMAC-signed
  webhook payload (the signature covers the whole body); `hooks.json` passes them to `deploy-website.sh` as
  `DEMO_IMAGE_DIGEST` and `DEMO_VERSION`. Two senders: `ci.yml`'s `deploy-website` (a land that touched the site; it
  runs `released-demo.sh`) and the release pipeline's `publish` (the new release's `latest.json` and demo, by the digest
  its `demo` job pushed).
- **The image must be public**: the server pulls anonymously. GHCR may create the `cmdr-demo` package private on its
  first push; then David sets it public once at github.com/orgs/getcmdr/packages/container/cmdr-demo/settings. The
  release's `demo` job checks an anonymous pull before the release can publish and names that page when it fails.
- **Local builds**: `CMDR_WEBSITE_DEMO=1 pnpm build` reads `apps/desktop/build-demo/` directly. A local Docker build
  passes that dir as a named build context for the placeholder ref the Dockerfile names without a digest:
  `--build-context ghcr.io/getcmdr/cmdr-demo@sha256:<64 zeros>=apps/desktop/build-demo` (what `website-docker-build`
  does; without a local demo it uses an empty dir and `CMDR_WEBSITE_DEMO=0`).
- **Changing the webhook**: the listener reads `hooks.json` only at start (no `-hotreload`), so a change to it takes a
  `sudo systemctl restart deploy-webhook` on the server after it lands, then a redeploy (`workflow_dispatch`).
- **CI**: the website lane's `demo` job (`.github/workflows/website-lane.yml`) runs `released-demo.sh`, unpacks that
  image on the bare runner (`docker buildx build --output type=local`; its `site` job runs inside the Playwright image,
  which has no Docker), and hands the files over as an artifact, so the site's build, the demo smoke, and Lighthouse see
  exactly the `/demo/` the deploy ships (the full demo suite runs elsewhere: the tests bullet below). The `website`
  filter carries no demo source: a frontend or engine change doesn't run the lane. The lane's verdict therefore also
  rides on which release is Latest, which its marker doesn't hash; the nightly full run (no markers) re-tests the site
  against the current one.
- **Serving** (`nginx.conf`): `/demo` redirects to `/demo/`; `/demo/_app/immutable/` is content-hashed (a year,
  immutable, and a bare 404 when missing: no body, the demo's headers, never the site's HTML 404 page, so the demo's
  loader can tell a stale chunk name in an old tab from a real failure); anything else under `/demo/` is the file or,
  failing that, the app page, so a deep link or reload boots the app. `website-docker-build` starts the built image and
  probes the bare 404 (`websiteServingProbes` in `scripts/check/checks/website-docker.go`). nginx's own mime table
  serves `.wasm` as `application/wasm` (verified in `nginx:alpine` 1.31.6, `curl -I`, 2026-11-04).
- **Precompressed Brotli**: `pnpm build:demo` ends with `apps/desktop/scripts/precompress-demo.ts`, which writes a
  quality-11, 16 MiB-window `<file>.br` next to every `.wasm`, `.js`, and `.css` of 10 KiB or more under
  `_app/immutable/` (57 files, 21,571,685 → 5,034,366 bytes, about 20–40 s on a Mac, 2026-11-04). The engine `.wasm`
  goes out as 2,571,369 bytes instead of 9,358,817. It runs in CI with the rest of the demo build, so the `.br` files
  ship inside the demo image and the server only copies them.
  - nginx serves them with no third-party module (`nginx-demo-precompressed.conf`, included by one location per type
    under `/demo/_app/immutable/`): a `map` turns `Accept-Encoding: br` into a `.br` suffix, `try_files $uri.br $uri`
    falls back to the plain file, a per-location `types` gives the twin its original's Content-Type, and
    `Content-Encoding: br` follows the file actually served, with `Vary: Accept-Encoding` on both. Verified in
    `nginx:alpine` 1.31.6 (`curl -D -` with `br`, `gzip`, and `identity`; the `br` body decoded byte-identical; a file
    with no twin went out plain and unlabeled; 2026-11-04).
  - **Cloudflare must pass the origin's `br` through.** Its docs say it asks the origin with `accept-encoding: br, gzip`
    and keeps an origin's `br` for visitors that accept it, as long as no content-rewriting feature touches the response
    (developers.cloudflare.com/speed/optimization/content/compression/, read 2026-11-04). If it ever stops, nginx
    answers plain and the edge compresses as before, so nothing breaks; the page only downloads more.
  - **The post-deploy check**: `content-encoding: br` alone proves nothing, since the edge's own Brotli says the same.
    The tell is the ETag: when Cloudflare compresses, it weakens the origin's ETag to `W/"…"`; a passed-through `.br`
    keeps nginx's strong `"<mtime>-<size in hex>"`, and that size is the `.br` file's. Run
    `curl -s https://getcmdr.com/demo/ | grep -o '/demo/_app/immutable/[^"]*\.js' | sort -u | while read -r p; do echo "$(curl -sI -H 'Accept-Encoding: br' "https://getcmdr.com$p" | tr -d '\r' | grep -i '^etag:') $p"; done`
    and every chunk of 10 KiB or more should print a strong `etag: "…"`; a `W/` there means the edge recompressed it.
    Before this shipped, `chunks/IoygyzeY.js` (646,867 bytes) went out as 166,052 bytes of edge Brotli with
    `etag: W/"6aebaa25-9ded3"`; its quality-11 twin is 137,492 bytes, 17% smaller (2026-11-04). For the engine, find
    `cmdr_listing_wasm_bg.<hash>.wasm` in the browser's network tab on `/demo/` (it loads lazily, so the page's HTML
    doesn't name it) and `curl -sI -H 'Accept-Encoding: br'` it the same way.
- **Headers**: `nginx-demo-security-headers.conf` replaces the site's CSP for `/demo/` only, sharing the rest via
  `nginx-security-headers-base.conf`. The policy and why each part is there are in that file's header. Verified in the
  built image with Chromium (2026-11-04): no CSP violation on boot, `WebAssembly.instantiate` allowed under `/demo/` and
  refused under the site's own policy. It also sends the `X-Robots-Tag` (the Indexing bullet above).
  `frame-src 'self' blob:` because the demo's secondary windows (Settings) are same-origin iframes of `/demo/<route>`,
  and Chrome's PDF viewer checks `frame-src` for the viewer's `blob:` PDF; `frame-ancestors 'self'` lets our own pages
  embed the demo. `connect-src 'self'` refuses a `data:` URL, so the demo's media imports are never inlined
  (`apps/desktop/src/lib/demo/DETAILS.md` § Viewer media).
- **Tests: a smoke on every land, the full suite against a demo of the same sources.** Both run against `dist/` like
  every website E2E (so without nginx's headers; the CSP is verified by hand as above), skip locally when `dist/demo/`
  isn't built, and fail in CI when it's missing. Locally they need Playwright's Chromium
  (`pnpm exec playwright install chromium` here, once). Two Playwright projects split them (`playwright.config.ts`):
  - `e2e/demo-smoke.spec.ts` (boots without a page error, is indexable with its description and canonical link, is the
    sitemap's only `/demo/` URL, lists the home folder) is in the `chromium` project with every other spec, so
    `website-e2e` runs it in the website lane against the RELEASED demo. It asks only what every release's demo answers;
    the page's `<head>` and the sitemap come from the site's own build (`demoPageHead`), never the image, so those
    asserts track `main`. The deep routes' `noindex` is an nginx header, so `website-docker-build` probes it instead.
  - `e2e/demo.spec.ts` is everything else (⌘F, file operations, the queue and the operation log, Multi-Rename, drops, a
    full disk via `/demo/?free=1000`, touch), the `demo` project with the smoke, run by `website-demo-e2e` (CI-only)
    against a demo built from the same sources: in the release pipeline's `demo` job before it publishes (a failure
    blocks the release), and in the nightly full run's website `demo-suite` job (a failure reaches the CI health
    digest). **Why**: the land path tests the released image, which predates `main`'s frontend, so a test of a new demo
    feature failed every land run until the next release (land run 42134174874, 2026-11-06, two Multi-Rename cells).
    Both jobs prepare through `.github/actions/website-demo-suite`. Locally: `pnpm build:demo`,
    `CMDR_WEBSITE_DEMO=1 pnpm build` here, then `pnpm check website-demo-e2e`.
- **Other dist checks skip `dist/demo/`**: `.htmlvalidateignore` (the desktop's `app.html` conventions differ) and
  `website-dead-links` (Vite hashes its own asset names, so the `?v=` rule doesn't apply). A build without the demo (no
  `CMDR_WEBSITE_DEMO=1`) can't judge links INTO `/demo/` either, like the hero's, so `website-dead-links` skips those
  until `dist/demo/index.html` exists, and checks them once it does.

## Typed linting

`eslint.config.js` runs typescript-eslint's type-aware rules in two scopes, and they are deliberately different.

**`**/*.ts` carries the full house set** (`no-unsafe-assignment` / `-call` / `-member-access` / `-return`,
`no-floating-promises`, `await-thenable`, `no-misused-promises`, `require-await`, `no-explicit-any`), the same nine
rules `api-server` and `analytics-dashboard` run. `projectService: true` was already configured here, so the type
program was being built and read by nothing.

**`**/*.astro` carries only the promise family plus `no-explicit-any`.** The `no-unsafe-*` four are excluded on
evidence: turning them on reported 23 findings across eight files, every one false, while `astro check` found 0 errors
in the same files. Astro's `Astro.props` inference and its template JSX are Volar features, and plain TypeScript
resolves neither, so the findings read as "type that cannot be resolved" or "type error" rather than `any`. No source
change can fix them. The promise rules, by contrast, were verified live by planting a floating promise and an unawaited
async function in frontmatter and watching both fire. (verified on astro-eslint-parser 1.x + typescript-eslint 8.64,
2026-09-02)

The `.astro` block asks for `project: true`, not `projectService`: `astro-eslint-parser` doesn't support the latter and
silently rewrites it to the former, printing a warning on every run. Naming it directly drops the warning. The block
costs about eight seconds of the roughly 27-second warm lint run.

**`astro sync` must run first.** Astro's generated `types.d.ts` is gitignored, and without it `getCollection('blog')` is
`any`, so the `no-unsafe-*` rules on the `.ts` side report ~90 false positives across five files. The
`website-astro-sync` check is a dependency of `website-eslint`, which also refuses to start when the file is missing;
CI's website job runs `pnpm exec astro sync` as a setup step. This mirrors what `desktop-svelte-kit-sync` does for
SvelteKit's generated `$app/tsconfig`.

**`PUBLIC_*` environment variables are typed** in `src/env.d.ts` (`ImportMetaEnv`), mirroring `.env.example`. Without
those declarations `import.meta.env.PUBLIC_ANYTHING` is `any`, so a typo reads as `undefined` and the feature quietly
turns itself off. Add a variable to both places together.

## Tailwind class hygiene

`eslint-plugin-better-tailwindcss` runs on `src/**/*.astro` and `src/**/*.ts` from `eslint.config.js`, with
`src/styles/global.css` as the Tailwind entry point so it knows every `@theme` token. Nothing else enforced class
hygiene before it: IntelliSense's editor nudges never reached `pnpm check` or CI, and the site had drifted to some 255
`text-[var(--color-…)]`-style utilities.

- **The canonical form is the theme name.** `text-text-secondary`, `bg-accent/10`, `border-border`,
  `hover:text-accent-hover`, `from-surface`, `outline-accent`. Tailwind's own canonicalizer sometimes stops at the
  variable shorthand (`border-(--color-border)`, `bg-(--color-accent)/10`) and accepts both, so the sweep rewrote those
  to the theme name by hand; new code should go straight to the theme name. Both forms compile to the same declaration
  (`color: var(--color-…)` or the same `color-mix(…)`), verified by building the site before and after the sweep and
  diffing the emitted CSS rule bodies: every color declaration was identical, and the only differences were the
  collapsed utilities below (2026-09-01). Light-mode overrides in `global.css` keep applying because the utility still
  references the variable.
- **Rules on** (all as errors, all but two autofixable, so `pnpm lint:fix` does the work): `enforce-canonical-classes`
  (the ESLint twin of IntelliSense's `suggestCanonicalClasses`; also collapses `h-6 w-6` into `size-6` and
  `[grid-auto-flow:dense]` into `grid-flow-dense`), `enforce-consistent-class-order` (Tailwind's official order),
  `no-conflicting-classes`, `no-deprecated-classes`, `no-duplicate-classes`, `no-unnecessary-whitespace`.
- **Rules deliberately off.** `enforce-shorthand-classes`, `enforce-consistent-important-position`, and
  `enforce-consistent-variable-syntax` are subsumed by `enforce-canonical-classes` and would double-report.
  `no-unknown-classes` can't work here: the templates mix Tailwind with BEM classes (`split-btn__label`, `hero-frame`,
  `link-muted`) styled in scoped `<style>` blocks or plain CSS in `global.css`, which the plugin can't see (it only
  detects `@utility` and `@layer components` classes in the entry CSS). `enforce-consistent-line-wrapping` would fight
  oxfmt.
- **Where it looks.** The plugin's default selectors cover `class` and Astro's `class:list` (its string literals and
  object keys, so the arrays and conditionals in `src/pages/features.astro` and `StatusPill.astro` are linted; verified
  on 4.7.0 by reading `getDefaultSelectors()`). Class strings held in a plain `const` are only linted when the variable
  is named `className`, `classNames`, or `classes`, so name them that way. HTML inside data strings (the `answerHtml`
  fields in `src/pages/pricing.astro`) is invisible to it; keep those canonical by hand.
- **Editor**: set `"tailwindCSS.lint.suggestCanonicalClasses": "ignore"` in your `.vscode/settings.json` (gitignored) so
  IntelliSense stops reporting what ESLint now owns.
- **Cost**: the canonical rule boots Tailwind, about one second per lint run.
- **`rootFontSize: 16`** in the plugin settings lets the rule turn pixel values that land on the spacing scale into
  their rem forms (`p-[2px]` to `p-0.5`). It doesn't catch every one (`min-w-[320px]` stayed put; it's `min-w-80` by
  hand), so prefer the scale name when writing new classes.
- **Colors inside arbitrary values are invisible to every rule.** A gradient stop takes
  `color-mix(in_srgb,var(--color-accent)_15%,transparent)`, never a literal `rgba(…)`, so the light theme's accent
  applies (see `Hero.astro`).
- **Only utility-worthy colors go in `@theme`.** A token there becomes a utility the canonicalizer may suggest, so an
  alias like `--color-icon-no: var(--color-warning)` made it propose `text-icon-no` for `text-warning`. CSS-only
  variables (the inline markdown icon colors) live in a plain `:root` block.

The analytics dashboard runs the same rule set from its `eslint.config.js` with `src/app.css` as the entry point; its
sweep was class order plus a few collapses (`h-2 w-2` to `size-2`, `text-xs leading-relaxed` to `text-xs/relaxed`). The
desktop app writes plain CSS and has no Tailwind.

## Analytics

The website uses three analytics layers. The desktop app separately sends anonymous beta usage analytics: an
`anal_`-keyed hourly heartbeat (true DAU) plus curated PII-free feature events stored in D1, all behind a tri-state
opt-out and stripped of file names, paths, queries, and prompts by allowlist. See
`apps/desktop/src-tauri/src/analytics/CLAUDE.md`.

- **Umami** (`Layout.astro`): Cookieless page analytics (pageviews, referrers, geo, UTM). Self-hosted. Script served at
  `/u/mami` (proxied through Caddy to avoid adblockers).
- **PostHog** (`public/scripts/posthog-init.js`): Session replay, heatmaps, click tracking. Configured with
  `persistence: 'memory'` (no cookies, no localStorage) and `person_profiles: 'identified_only'` (no anonymous person
  profiles). This keeps PostHog fully cookieless. It gets website data only; the desktop app's events stay in D1.
- **D1** (API server): Download redirect endpoint logs version, arch, country, source, a first-touch `ref` channel, and
  a daily-hashed IP (bot hits dropped). The download button carries `?src=website` so the endpoint can tag it as a
  website download (vs Homebrew or direct links). The `heartbeat` table holds the desktop DAU beats.

### Post-download popup

`DownloadStartedPopup.astro` (mounted in `Layout.astro`) opens once per visitor, on their first click of any
`[data-umami-event="download"]` link, and asks for a GitHub star plus a release-email signup, with Discord as a footer
link. Decision/why: the Discord-first version converted at about 5% (21 joins out of about 430 people who interacted
with it, PostHog autocapture, 2026-07-03 to 2026-09-30), and this audience mostly isn't on Discord. A star moves Cmdr
toward the 225 that a self-submitted cask needs for Homebrew's main catalog, and an email is the one way back to someone
who tries Cmdr once and drifts off.

- The star count comes from `src/build/github-stars.ts` at build time, so visitors never call GitHub. When the fetch
  fails, the popup drops the number and the progress bar instead of showing a stale one.
- The email block reads the newsletter's localStorage flags when the popup opens: subscribers see a thank-you line,
  visitors who clicked "Not interested" anywhere see no email block, and everyone else sees the form.
- Clicks carry Umami events `download-popup-star` and `download-popup-discord`; the form's signups show in the
  dashboard's Listmonk column and in PostHog autocapture under `.download-popup`.

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

## Visual baselines

`e2e/visual.spec.ts` diffs six committed PNGs (`maxDiffPixelRatio: 0.01`), totalling ~550 KB:

- `fixture-light` / `fixture-dark`: `article.blog-content` on `/visual-fixture`, the only pair shot in both themes.
- `fixture-dropdown`: the in-prose download dropdown, opened, clipped to itself.
- `home-fold`: `/` at viewport size (nav, hero, split button, theme toggle).
- `pricing-tiers`: the `[data-visual="pricing-tiers"]` grid.
- `footer`: the `<footer>` on `/visual-fixture`. It's shared sitewide, so one shot covers every page; shot on `/`, it
  churned with every landing edit (its top lands on a fractional pixel below the copy).

**Two rules keep the set from regrowing.**

1. _Shoot machinery, never content._ `/visual-fixture` (`src/pages/visual-fixture.astro` rendering
   `src/fixtures/visual-fixture.md`) exercises every rehype transform, Shiki, and smartypants on frozen non-editorial
   copy, so a baseline moves only when the machinery moves. Adding a markdown transform means adding a block there.
2. _Shoot regions, not pages._ Structural regressions show in the first fold and in specific components; mid-page prose
   is most of the bytes and none of the signal, and it's what grows as the site gets written.

**Why the set looks like this.** It used to be 28 full-page PNGs of the marketing pages and two real posts, 16 MB in the
tree and 47 MB across history (about a fifth of `.git`). The churn was concentrated and avoidable: `features` alone had
13 revisions (it renders `feature-status.json`, so every release invalidated ~840 KB) against two or three for
everything else, and every one of those diffs was an intentional copy change rather than a caught regression. Two shots
also re-rendered the same 5,722 px page to record one open dropdown. The current set stays still when you publish a
post, edit marketing copy, or ship a release.

**Baselines are Linux-only.** CI runs Linux and macOS renders different font antialiasing, so a `-darwin` set would be a
second copy of every byte that CI never checks (it was half the old 16 MB). `visual.spec.ts` calls `test.skip()` on
other platforms, which is what stops the darwin set from reappearing the next time the suite runs on a Mac; the trade is
that `pnpm --filter @cmdr/website test:e2e` no longer covers visual regressions locally.

**One renderer shoots and verifies.** `scripts/update-visual-baselines.sh` captures in
`mcr.microsoft.com/playwright:v<version>-noble`, and ci.yml's Website job runs inside that same image. Both derive
`<version>` from the resolved `@playwright/test` rather than naming it (the script from the installed package, CI from
`pnpm-lock.yaml` in the `changes` job, whose `playwright-image` output feeds `container:` — that key is evaluated before
a job starts, so the tag can only reach it from an earlier job). A Renovate bump therefore moves capture and
verification together.

- **Gotcha:** a bare `ubuntu-latest` runner is NOT that renderer, even running the same Chromium build. On the host font
  stack the two densest region shots went over the threshold with nothing behind them (`fixture-dropdown`: 4,103 px, 10%
  of its 244×183 clip; `pricing-tiers`: 6,751 px, 2%), while the whitespace-heavy shots stayed under it. The ratio is
  per clip, so tight component shots feel a systematic text-render difference that a full-viewport shot dilutes. The CPU
  is NOT the variable: the same image renders the whole set within threshold on arm64 and on amd64 (verified on
  `mcr.microsoft.com/playwright:v1.62.1-noble`, both architectures, 2026-09-07), so shooting them from an Apple Silicon
  Mac is fine.
- ❌ Never add a `playwright install` step to that job. The image already carries the browsers at
  `PLAYWRIGHT_BROWSERS_PATH`; installing fetches a build for whatever npm resolved, which is how a render quietly stops
  matching the image the baselines were shot in. A genuine mismatch instead fails loudly ("Executable doesn't exist at
  /ms-playwright/chromium-…").
- Lighthouse runs in that container too, so it needs `CHROME_PATH` pointed at the image's Chromium and `--no-sandbox`
  (`lighthouserc.cjs`), because the container runs as root.
- A failed Website job uploads `apps/website/test-results/` as the `website-playwright-results` artifact: the `-actual`
  / `-expected` / `-diff` PNGs and the retry traces. Without it a red visual lane can only be guessed at, since the
  `github` reporter prints pixel counts and no images.

**How the refresh script behaves:**

- Compare-then-`--last-failed`: it runs a normal comparison and only re-shoots baselines that actually fail the
  threshold, so passing (sub-threshold-noisy) shots aren't churned. Idempotent (a no-op when everything already passes).
- `--force` sets `VISUAL_FORCE=1` and re-shoots everything with `--update-snapshots=all`, skipping the comparison. It
  exists because compare-then-`--last-failed` can't see a real change that stays under `maxDiffPixelRatio`: swapping the
  logo moved 0.08% of the 1280×533 footer shot against a 1% threshold, so all six baselines passed while two of them
  still showed the old mark (2026-09-07). **`=all` is load-bearing** — bare `--update-snapshots` presets to mode
  `changed`, which re-asks the same comparator and so skips exactly those diffs.
  - ❌ **Don't commit everything it rewrites.** These renders aren't byte-deterministic: two forced runs of the same
    commit in the same container differed by ~50 of 1,024,000 px in `home-fold`, and 5 of 6 baselines came back modified
    with nothing changed between them. Keep only the shots containing what you changed, `git checkout --` the rest, then
    run the script with no arguments; an empty "Changed baselines:" list is the proof CI will pass.
- The container installs and builds into anonymous volumes and chowns the written PNGs back, so it never overwrites the
  caller's macOS `node_modules`/`dist`. Requires Docker; a stopped Docker aborts before any change.
- `--full` sets `VISUAL_FULL=1`, which swaps in a `snapshotPathTemplate` (playwright.config.ts) pointing at the
  gitignored `e2e/visual-full-snapshots/` and shoots every real page full-page in both themes (~7 MB). It's the coverage
  the region set gives up: capture before an Astro major upgrade, upgrade, re-run to compare, then delete the dir.
  **Gotcha:** the redirect has to be a separate directory, not a `full/` prefix in the snapshot name, because Playwright
  flattens a `/` in a name into a `-`, which would drop the files beside the committed baselines and get them committed.
  `/changelog` is excluded from this set: at ~75,000 px, consecutive captures disagree on the height and Playwright can
  never stabilize it.

`scripts/release.sh` calls the script after the release copy is finalized and before the release commit, so `git add -u`
folds any refreshed baselines into the `chore(release)` commit. With `/features` no longer shot, a release should no
longer touch a baseline at all; the call stays as a cheap guard.

The fixture page ships in production as an unlinked, `noindex` page, excluded from the sitemap by the `filter` in
`astro.config.ts`. Keeping it out of `src/content/blog/` is what spares `/blog`, `rss.xml`, and the `/og/[slug].png`
route from having to learn to skip it.

## Dev-only task callouts

`src/components/DevTodo.astro` marks an open task for David right where it applies: a big ⚠️, a warning-colored border,
and a short task in the default slot (or `html=` for inline HTML from a `src/lib/` data module, like the `devTodo`
fields in `src/lib/trust.ts`).

- It renders only when `import.meta.env.DEV` is true. The site is statically built, so a production build emits nothing
  for it, and a callout merged by accident can't reach getcmdr.com. That is also why it may use a raw ⚠️ despite the
  emoji policy in § Icons.
- It's for copy that can't yet be both true and good: a claim that needs David's decision, or a contradiction with
  another page the agent wasn't allowed to edit. The public copy next to it must stay honest on its own; the callout
  never excuses an untrue sentence.
- Find every open one with `rg -n "DevTodo|devTodo" apps/website/src`.
