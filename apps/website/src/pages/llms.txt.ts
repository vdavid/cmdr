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

> The AI-native file manager for power users who want superpowers.

Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS, built with Rust, Tauri 2, and Svelte 5. It lets you rename files with natural language, search by describing what you're looking for, and organize hundreds of files with a single command. Free forever for personal use, source-available under BSL 1.1.

Current version: ${version}

## Key links

- [Download (Apple Silicon)](${dmgUrls.aarch64}): DMG installer for Apple Silicon Macs
- [Download (Intel)](${dmgUrls.x86_64}): DMG installer for Intel Macs
- [Pricing](${site}/pricing/): Free for personal use, commercial from $59/year
- [Blog](${site}/blog/): Updates and news
- [Changelog](${site}/changelog/): Release notes
- [Roadmap](${site}/roadmap/): What's coming next
- [GitHub](https://github.com/vdavid/cmdr): Source code and issues
- [RSS feed](${site}/rss.xml): Subscribe to blog updates

## Features

- **Natural language rename**: Type "make these lowercase and add date prefix" and watch it happen. No regex, no scripts.
- **Smart search**: Find files by describing them: "that PDF contract from last month" or "screenshots with error messages."
- **AI batch operations**: Organize hundreds of files with a single command. "Sort these into folders by project name."
- **Keyboard-first**: Navigate, select, copy, move without touching your mouse.
- **Blazing fast**: Built with Rust for native performance. Handles folders with 50,000+ files effortlessly.
- **Two-pane layout**: See source and destination side by side. The classic layout that works.

## Pricing

- **Personal**: Free forever. All features, your own devices, automatic updates. No commercial use.
- **Supporter**: $10 one-time. Everything in Personal plus a "Supporter" badge in the app.
- **Commercial**: $59/year (discounted from $79 for first 1,000 licenses). All features, commercial use, per user, your own devices.
- **Perpetual**: $199 one-time. All features, commercial use, per user, your own devices, three years of updates.

## System requirements

- macOS (Apple Silicon and Intel)
- Linux support in alpha

## License

BSL 1.1 (source-available). Converts to AGPL-3.0 after three years.

## Blog posts

${blogLines}
`

    return new Response(body, {
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
    })
}
