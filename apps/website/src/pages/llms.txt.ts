import type { APIContext } from 'astro'
import { getCollection } from 'astro:content'
import { version, dmgUrls } from '../lib/release'

export async function GET(context: APIContext) {
  const site = context.site!.origin
  const posts = await getCollection('blog')
  const sortedPosts = posts.sort((a, b) => b.data.date.valueOf() - a.data.date.valueOf())

  const blogLines = sortedPosts
    .map((post) => `- [${post.data.title}](${site}/blog/${post.id}/): ${post.data.description}`)
    .join('\n')

  const body = `# Cmdr

> The fastest two-pane file manager for macOS. Every folder sized. Every file found.

Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS, built with Rust, Tauri 2, and Svelte 5. It indexes your entire drive in minutes, shows directory sizes everywhere, and offers instant search. Free for personal use, source-available under BSL 1.1.

Current version: ${version}

## Key links

- [Download (Apple Silicon)](${dmgUrls.aarch64}): DMG installer for Apple Silicon Macs
- [Download (Intel)](${dmgUrls.x86_64}): DMG installer for Intel Macs
- [Pricing](${site}/pricing/): Free for personal use, $59 once for commercial use
- [Trust and security](${site}/trust/): What the app sends over the network, where data is stored, signing and updates, and known gaps, for security and IT reviewers
- [How Cmdr is built and kept up to date](${site}/trust/development/): The development process by stage (design, code, review, test, dependencies, release, updates) with measured numbers, for security questionnaires
- [Accessibility guide](${site}/trust/accessibility-guide/): Cmdr's accessibility features (keyboard-only use, VoiceOver, text size and zoom, light and dark mode, Reduce motion, Reduce transparency), how to turn them on, known gaps, and how to get help
- [Accessibility conformance report](${site}/trust/accessibility/): How Cmdr meets WCAG 2.2 A and AA, Revised Section 508, and EN 301 549, criterion by criterion, with known gaps (based on VPAT® 2.5Rev INT)
- [Blog](${site}/blog/): Updates and news
- [Changelog](${site}/changelog/): Release notes
- [Roadmap](${site}/roadmap/): What's coming next
- [GitHub](https://github.com/vdavid/cmdr): Source code and issues
- [RSS feed](${site}/rss.xml): Subscribe to blog updates

## Features

- **Live full-disk index**: Indexes your entire drive once in 2–4 minutes. Then stays current forever, even across restarts.
- **Blazing fast**: Built in Rust. Opens a 100k-file folder in 4 seconds with icons, sizes, and dates.
- **Keyboard-first**: Navigate, select, copy, move without touching your mouse. Two panes, tabs, command palette.
- **Smart search** (rough around the edges): Find files by describing them: "that PDF contract from last month" or "screenshots with error messages."
- **Natural language rename** (coming soon): Type "make these lowercase and add date prefix" and watch it happen. No regex, no scripts.
- **AI batch operations** (coming soon): Organize hundreds of files with a single command. "Sort these into folders by project name."

## Pricing

- **Personal**: Free. The whole file manager, your own devices, automatic updates. Cmdr AI is free during the beta and later moves to a Pro plan with hosted models included. No commercial use.
- **Commercial**: $59, paid once. All features including Cmdr AI, commercial use, per user, your own devices. One year of updates included, then $39/year to keep receiving them. The last version you were entitled to stays yours forever.
- **Enterprise**: for larger teams and organizations, where IT deploys Cmdr, security reviews it, and finance wants one invoice against a PO. Quoted by hand, no published price. Email sales@getcmdr.com.

## System requirements

- macOS 12 or newer (Apple Silicon and Intel)
- macOS 10.15 and 11 run on a best-effort basis: they work, but a few colors and layout details can look off, and fixes for them are a lower priority
- Linux support in alpha

## License

BSL 1.1 (source-available). Converts to AGPL-3.0 after three years.

## Blog posts

Each post is also available as raw Markdown: append \`index.md\` to its URL.

${blogLines}
`

  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  })
}
