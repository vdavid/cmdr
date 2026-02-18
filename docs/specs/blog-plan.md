# Blog for getcmdr.com

## Context

Adding a lightweight blog to the Astro website. The goal is: write markdown, preview locally, publish as static pages.
No blog engine — just Astro content collections, a few npm packages, and Remark42 for comments.

## Image sizing

- Blog content: `max-w-3xl` (768px) with `px-6` (24px each side) = **720px effective content width**
- For 2x retina at 720px = 1440px needed
- Store source images at ~1500px wide. CSS `max-width: 100%; height: auto;` handles responsive display. Astro optimizes format (WebP) at build time.
- Click an image to open full-size in a new tab (tiny `<script>`, no library)

## New dependencies

| Package | Purpose |
|---|---|
| `@astrojs/rss` | RSS feed |
| `rehype-external-links` | Auto `target="_blank"` on external links |
| `satori` | OG image generation (SVG) |
| `@resvg/resvg-js` | SVG to PNG rasterization |

## Files created

| File | Purpose |
|---|---|
| `src/content.config.ts` | Blog collection schema (title, date, description, cover) |
| `src/content/blog/hello-world/index.md` | Example post with a colocated image |
| `src/layouts/BlogLayout.astro` | Post layout (mirrors `LegalLayout.astro` pattern) |
| `src/styles/blog-prose.css` | Shared prose styles (imported by both layout and index) |
| `src/pages/blog/index.astro` | Blog index — full articles, newest first |
| `src/pages/blog/[slug].astro` | Individual post page with comments |
| `src/pages/og/[slug].png.ts` | Static OG image generation at build time |
| `src/pages/rss.xml.ts` | RSS feed |
| `src/components/Remark42Comments.astro` | Comment widget |
| `e2e/blog.spec.ts` | E2E tests |

## Files modified

| File | Change |
|---|---|
| `astro.config.mjs` | Added `site`, `markdown.shikiConfig`, `markdown.rehypePlugins` |
| `src/layouts/Layout.astro` | Added `ogImage` prop, `og:image`/`twitter:image` meta, RSS `<link>` |
| `src/components/Header.astro` | Added Blog to `navLinks` |
| `src/components/Footer.astro` | Added Blog link to "Resources" column |
| `docker-compose.yml` | Added Remark42 service |
| `package.json` | New dependencies |

---

## Milestones

### Milestone 1: Content collection + blog pages + navigation

1. Install deps (`@astrojs/rss`, `rehype-external-links`, `satori`, `@resvg/resvg-js`)
2. Update `astro.config.mjs` with `site` and markdown config
3. Create content config with blog collection schema
4. Create example "Hello, world" post
5. Update Layout.astro with ogImage prop, meta tags, RSS autodiscovery
6. Create BlogLayout.astro following LegalLayout pattern
7. Create blog index page (full articles, newest first)
8. Create individual post page with getStaticPaths
9. Add Blog to Header and Footer navigation

### Milestone 2: Blog prose styles

CSS scoped under `.blog-content :global(...)` covering headings, paragraphs, lists, links, external link indicator, inline code, code blocks, blockquotes, images, horizontal rules, and strong text.

### Milestone 3: OG images with Satori

Static OG image generation at `/og/[slug].png` using Satori + resvg. 1200x630 dark template with title, description, date, and Cmdr branding.

### Milestone 4: RSS feed

RSS feed at `/rss.xml` using `@astrojs/rss`. Includes title, description, pubDate, and link for each post.

### Milestone 5: Remark42 comments

- `Remark42Comments.astro` component (disabled in dev mode, shows placeholder)
- Remark42 Docker service in `docker-compose.yml`
- Caddy route: `comments.getcmdr.com` to `remark42:8080` (configured manually)

### Milestone 6: Testing and polish

E2E tests for blog index, individual posts, RSS feed, OG images, and navigation. Full check suite pass.
