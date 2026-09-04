# Website (getcmdr.com)

Marketing site and blog for Cmdr. Astro + Tailwind v4 (CSS-first config in `src/styles/global.css`), Playwright E2E in
`e2e/`, statically built. Full details: [DETAILS.md](DETAILS.md). Human-facing; its markdown may use tables freely.

## Module map

- `src/pages/`, `src/layouts/`, `src/components/`: pages, layouts, components.
- `src/components/icons/`: shared `<Icon name size>` glyph system (Lucide line-art). Every icon goes through it; no
  `<img>`/raw `~icons`/decorative emoji. [DETAILS.md](DETAILS.md) § Icons.
- `src/content/blog/{slug}/index.md`: blog posts, colocated images (schema in `src/content.config.ts`). Add one:
  `docs/guides/writing-blog-posts.md`.
- `src/lib/`: typed page content kept out of the templates. Roadmap milestones live in `src/lib/roadmap.ts` (edit items
  there, never in `src/pages/roadmap.astro`); per-feature stability in `src/lib/feature-status.ts`;
  `src/lib/changelog.ts` linkifies the bare commit hashes `CHANGELOG.md` stores (rule: `scripts/check/checks/DETAILS.md`
  § "CHANGELOG commit refs").
- `src/dev/blog-editor/`: dev-only Markdown editor at `/dev/blog` (Vite middleware, not an Astro page, absent from
  prod).
- `src/pages/llms.txt.ts` / `llms-full.txt.ts`: agent-facing product descriptions; keep synced with product facts.

## Deployment

Auto-deploys on push to `main` touching `apps/website/**` (the `deploy-website` job in `ci.yml`, a signed webhook to the
Hetzner VPS). This is the ONLY deploy path; `release.yml` hits the same hook after a desktop release. Steps and
fallback: `docs/guides/deploy-website.md`.

- **Deploy order:** always `docker compose build` before `down`; building first avoids ~15s downtime.

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

- Don't hardcode colors; use `global.css` CSS variables. (OG images excepted: Satori can't read them, so keep their
  hardcoded colors in sync.)
- Accent buttons: text uses `--color-accent-contrast` (not `--color-background`) so it stays dark across modes.
- Color utilities use the `@theme` names (`text-text-secondary`, `bg-accent/10`, `border-border`), never
  `text-[var(--color-…)]` or `text-(--color-…)`. `pnpm check eslint` enforces it (`better-tailwindcss`), and
  `pnpm lint:fix` rewrites the long forms. [DETAILS.md](DETAILS.md) § Tailwind class hygiene.

## Gotchas

- **Visual baselines (`e2e/visual.spec.ts`) are per-OS; refresh BOTH.** Updating one platform strands the other
  (`-linux` is what CI checks) and reddens CI later. Use `apps/website/scripts/update-visual-baselines.sh` (needs
  Docker; `scripts/release.sh` auto-runs it). [DETAILS.md](DETAILS.md) § Visual baselines.
- **Keep TS generic calls single-line in `.astro` `<script>` blocks** — the astro-eslint parser chokes on multi-line,
  cascade-blocking build/deploy.
- **Typed lint needs `astro sync` first**, and the `.astro` block deliberately omits the `no-unsafe-*` rules (they
  report only false positives there). [DETAILS.md](DETAILS.md) § Typed linting.
- `site` must be set in `astro.config.ts` for RSS and OG image URLs.
- `compressHTML: true` is deliberate: Astro 7's `'jsx'` default breaks home + pricing; don't drop it.
- Markdown pipeline details: [DETAILS.md](DETAILS.md) § Patterns.
- Remark42 comments disabled in dev. Setup: `docs/guides/deploying-remark42.md`.
