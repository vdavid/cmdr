# Website (getcmdr.com)

Marketing site and blog for Cmdr. Astro + Tailwind v4 (CSS-first config in `src/styles/global.css`), Playwright E2E in
`e2e/`, statically built. Human-facing; its markdown may use tables freely.

## Module map

- `src/pages/`, `src/layouts/`, `src/components/`: pages, layouts, components.
- `src/components/icons/`: shared `<Icon name size>` glyph system (Lucide line-art). Every icon goes through it; no
  `<img>`/raw `~icons`/decorative emoji. [DETAILS.md](DETAILS.md) § Icons.
- `src/content/blog/{slug}/index.md`: blog posts, colocated images (schema in `src/content.config.ts`). Add one:
  `docs/guides/writing-blog-posts.md`.
- `src/lib/`: typed page content, edited there, never in the page: `roadmap.ts`, `feature-status.ts`, `trust.ts`,
  `trust-development.ts`. `changelog.ts` linkifies `CHANGELOG.md`'s bare commit hashes
  (`scripts/check/checks/DETAILS.md` § "CHANGELOG commit refs").
- `src/components/DevTodo.astro`: loud ⚠️ callout for David, rendered only in dev. [DETAILS.md](DETAILS.md) § Dev-only
  task callouts.
- `src/dev/blog-editor/`: dev-only Markdown editor at `/dev/blog` (Vite middleware, absent from prod).
- `src/pages/llms.txt.ts` / `llms-full.txt.ts`: agent-facing product descriptions; keep synced with product facts.

## Deployment

Auto-deploys on push to `main` touching `apps/website/**` (the `deploy-website` job in `ci.yml`, a signed webhook to the
Hetzner VPS). This is the ONLY deploy path; `release-pipeline.yml` hits the same hook after a desktop release. Steps,
fallback, and the build-before-`down` order: `docs/guides/deploy-website.md`.

## Analytics (must-knows)

Full narrative: [DETAILS.md](DETAILS.md) § Analytics.

- **`window.__cmdrRReady` gates Umami, PostHog, and the first-touch `ref` script**, so the async `?r=` expansion runs
  first. Gate anything new that reads `utm_source` or records a pageview on it too, and never revert Umami/PostHog to
  static `<script>` tags (they fire before the fetch and record the raw `?r=`).
- **Inline `<script is:inline>` analytics bodies must be raw JS, never a literal Astro ``{`...`}`` template-literal**
  (Astro ships the wrapper as inert dead text, so analytics silently never loads). Lead with `;`; guarded by the
  `website-analytics-injection` check.
- **Charset is the cross-repo attribution contract:** the client `?r=` sanitizer must normalize identically to the
  api-server's (`docs/architecture.md` § Acquisition analytics).
- **Never need a cookie consent banner.** Preference flags in localStorage are fine; anything that identifies, follows,
  or attributes a visitor must not use cookies/storage (track anonymously). Test: [DETAILS.md](DETAILS.md) § Client-side
  storage policy.
- A new download link needs `data-download-link` (main) or `data-arch` inside `[data-download-dropdown]` so the ref
  script finds it.

API access: `docs/tooling/umami.md`, `docs/tooling/posthog.md`.

## Color scheme (light/dark)

All pages support both; a header toggle (`ThemeToggle.astro`) overrides system preference. [DETAILS.md](DETAILS.md) §
Color scheme.

- Don't hardcode colors; use `global.css` CSS variables. (Except OG images: Satori can't read them, so sync by hand.)
- Accent buttons: text uses `--color-accent-contrast` (not `--color-background`) so it stays dark across modes.
- Color utilities use the `@theme` names (`text-text-secondary`), never `text-[var(--color-…)]`; eslint enforces it and
  `pnpm lint:fix` rewrites them. [DETAILS.md](DETAILS.md) § Tailwind class hygiene.

## Gotchas

- **Visual baselines (`e2e/visual.spec.ts`) shoot machinery, never content.** Six Linux-only region shots on
  `/visual-fixture`; ❌ never shoot a marketing page or post full-page (they churn on copy edits). New markdown
  transform → add a block to `src/fixtures/visual-fixture.md`. Refresh with
  `apps/website/scripts/update-visual-baselines.sh`; CI verifies inside that same container, so ❌ never move that job
  to the bare runner or install browsers in it. [DETAILS.md](DETAILS.md) § Visual baselines.
- **Keep TS generic calls single-line in `.astro` `<script>` blocks** (multi-line breaks astro-eslint).
- **Typed lint needs `astro sync` first**; the `.astro` block omits the `no-unsafe-*` rules (only false positives
  there). [DETAILS.md](DETAILS.md) § Typed linting.
- `site` must be set in `astro.config.ts` for RSS and OG image URLs.
- Keep `compressHTML: true`: Astro 7's `'jsx'` default breaks home + pricing.
- **A new fetch origin goes in `connect-src`** (`nginx-security-headers.conf`), or prod silently blocks it.
  [DETAILS.md](DETAILS.md) § Security headers.
- Remark42 comments disabled in dev. Setup: `docs/guides/deploying-remark42.md`.

Patterns, analytics, and baselines: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
