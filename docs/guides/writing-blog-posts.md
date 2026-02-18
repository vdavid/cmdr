# Writing blog posts

Blog posts live in `apps/website/src/content/blog/`. Each post is a folder with an `index.md` file and any colocated
images.

## Creating a new post

1. Create a folder: `src/content/blog/{slug}/index.md`
2. Add frontmatter:

```yaml
---
title: Your title here
date: 2026-02-18
description: A one- or two-sentence summary for SEO and social sharing.
cover: ./cover.jpg  # Optional. Relative path to a colocated image.
---
```

3. Write the post body in Markdown. All standard Markdown features work: headings, lists, code blocks, blockquotes,
   images, and links.

## Description vs. excerpt

- **`description`** (frontmatter): a concise 1–2 sentence summary. Used for meta tags (`og:description`, `<meta name="description">`), the subtitle on the post page header, and the OG image.
- **Excerpt** (`<!-- more -->` marker): controls what readers see on the blog index. Can be multiple paragraphs with full markdown formatting.

## Excerpts

Place `<!-- more -->` in your post to control what appears on the blog index. Content above the marker is shown as the
excerpt, followed by a "Read more" link. Content below the marker only appears on the individual post page.

```markdown
Here's the intro paragraph that shows on the blog index.

<!-- more -->

This part only shows on the full post page.
```

If you omit the marker, the full post is shown on the index.

## Images

- Colocate images next to `index.md` in the post folder
- Store source images at ~1500px wide (for 2x retina at the 720px content width)
- Reference them with relative paths: `![Alt text](./my-image.png)`
- Images are click-to-open-fullsize automatically
- CSS handles responsive sizing (`max-width: 100%`)

## Previewing locally

```bash
cd apps/website
pnpm dev
# Open http://localhost:4321/blog
```

## What happens automatically

- **OG images**: generated at build time at `/og/{slug}.png` using the post title, description, and date
- **RSS feed**: updated at `/rss.xml` with all posts sorted by date
- **External links**: get `target="_blank"` and `rel="noopener noreferrer"` via `rehype-external-links`
- **Syntax highlighting**: code blocks use the `github-dark` Shiki theme

## Style reminders

- Use sentence case for headings
- Use em dashes (—) for combining thoughts, en dashes (--) for ranges
- Use the Oxford comma
- Follow the full [style guide](/docs/style-guide.md)
