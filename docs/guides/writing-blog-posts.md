# Writing blog posts

Blog posts live in `apps/website/src/content/blog/`. Each post is a folder with an `index.md` file and any colocated
images.

## Creating a new post

For low-friction drafting, run the website dev server and use the dev-only editor:

```bash
pnpm dev:website
# Open http://localhost:4829/dev/blog
```

The editor has fields for title, slug, date, description, and excerpt (see "Blog-index blurb" below), plus a **Copy
markdown** button that copies the whole post (frontmatter + body) to the clipboard, handy for pasting a draft to an
agent for review. It autosaves drafts to `apps/website/.blog-drafts/` and only writes to the published blog collection
when you click **Publish**. The draft directory is gitignored. Use **Add image**, paste, or drag/drop in the Markdown
editor to add images; the editor stores draft images separately, inserts relative Markdown paths, and copies referenced
images next to the post when publishing.

The **Formatting help** button opens a cheat sheet covering everything below. In the body, description, and excerpt
fields, <kbd>⌘</kbd><kbd>B</kbd> / <kbd>⌘</kbd><kbd>I</kbd> / <kbd>⌘</kbd><kbd>K</kbd> wrap the selection (or insert at
the cursor) as bold, italic, and a link. The excerpt field renders as markdown in the preview, so a link there works
(it's the blurb under the title on `/blog`).

For manual authoring:

1. Create a folder: `src/content/blog/{slug}/index.md`
2. Add frontmatter:

```yaml
---
title: Your title here
date: 2026-02-18
description: A one- or two-sentence summary for SEO and social sharing.
---
```

3. Write the post body in Markdown. All standard Markdown features work: headings, lists, code blocks, blockquotes,
   images, and links.

## Description vs. excerpt

- **`description`** (frontmatter): a concise 1–2 sentence summary. Used for meta tags (`og:description`,
  `<meta name="description">`), the subtitle on the post page header, and the OG image. It's also the last-resort blurb
  on the blog index (see below).
- **Excerpt**: what shows under the title on the blog index, followed by a "Read more" link.

## Blog-index blurb

The blog index picks each post's blurb from the first of these that's set, so you control it without dumping the top of
the article into the list:

1. **`excerpt`** frontmatter (markdown): an explicit list-only blurb. Use this when the opening of the post (a heading,
   a long first paragraph) wouldn't read well in the list.

   ```yaml
   excerpt: A one-liner written just for the blog index.
   ```

2. **`<!-- more -->` marker** in the body: everything above it becomes the blurb. Good for a multi-paragraph teaser that
   doubles as the article's intro. Content below the marker only appears on the full post page.

   ```markdown
   Here's the intro that shows on the blog index.

   <!-- more -->

   This part only shows on the full post page.
   ```

3. **`description`**: if neither of the above is set, the index falls back to the `description`. So a short post that
   opens straight into a heading can just rely on its `description` and skip both.

## Download dropdown

To drop an inline download link into a post, use the marker `[download](cmdr:download)`. The `rehypeDownloadDropdown`
plugin replaces it with the same arch-aware dropdown the rest of the site uses (Apple Silicon / Intel / Universal, with
the visitor's arch auto-marked): the link text reads as a normal prose link with a download glyph, and clicking it opens
the menu. The link text is whatever you write between the brackets. The dev editor's `marked` preview shows it as a
plain link (the plugin only runs in the real Astro build).

## Images

- Colocate images next to `index.md` in the post folder. The dev editor does this automatically when publishing.
- Store source images at ~1500px wide (for 2x retina at the 720px content width)
- Reference them with relative paths: `![Alt text](./my-image.webp)`
- Images are click-to-open-fullsize automatically
- CSS handles responsive sizing (`max-width: 100%`)

## Theme-aware images and comparison rows

Two conveniences plain markdown can't express, handled by the `rehypeBlogMedia` plugin (`src/plugins/blog-media.mjs`):

- **Theme-aware image**: put the literal token `{theme}` in an image path and the post renders `…-light.webp` or
  `…-dark.webp` to match the visitor's theme (the header toggle, falling back to `prefers-color-scheme`):

  ```markdown
  ![Cmdr on macOS](/blog/my-post/cmdr-{theme}.webp 'Caption')
  ```

  Provide both files (`cmdr-light.webp` and `cmdr-dark.webp`). Theme images **must live in `public/`** (e.g.
  `public/blog/my-post/`), not colocated — Astro's per-file image optimizer can't resolve the `{theme}` token, the same
  reason the hero screenshots live in `public/`.

- **Side-by-side comparison**: two (or more) image lines in a single paragraph (no blank line between them) become a
  responsive row that stacks on mobile. Each image's `"title"` becomes its caption:

  ```markdown
  ![Total Commander on Windows](/blog/my-post/totalcmd.webp 'Total Commander · Windows')
  ![Cmdr on macOS](/blog/my-post/cmdr-{theme}.webp 'Cmdr · macOS')
  ```

The dev editor's preview mirrors both transforms in JS so you can see them while writing. The exact theme switch only
happens on the built site.

## Tables

GitHub-style tables render (Astro enables GFM by default); wide tables scroll horizontally. The dev editor preview
styles them too.

```markdown
| Feature  | Total Commander | Cmdr  |
| -------- | --------------- | ----- |
| Platform | Windows         | macOS |
```

## Previewing locally

```bash
pnpm dev:website
# Open http://localhost:4829/blog
```

## What happens automatically

- **OG images**: generated at build time at `/og/{slug}.png` using the post title, description, and date
- **RSS feed**: updated at `/rss.xml` with all posts sorted by date
- **External links**: get `target="_blank"` and `rel="noopener noreferrer"` via `rehype-external-links`
- **Syntax highlighting**: code blocks use the `github-dark` Shiki theme

## Style reminders

- Use sentence case for headings
- Use en dashes (--) for ranges. Avoid em dashes.
- Use the Oxford comma
- Follow the full [style guide](/docs/style-guide.md)
