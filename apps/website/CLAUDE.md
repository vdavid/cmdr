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

## Gotchas

- The `@ts-expect-error` in `astro.config.mjs` is for a Vite version mismatch between Astro and Tailwind. It doesn't
  affect the build.
- `site` must be set in `astro.config.mjs` for RSS and OG image URLs to work.
- Font files for OG image generation (`inter-400.ttf`, `inter-700.ttf`) live in `public/fonts/`.
