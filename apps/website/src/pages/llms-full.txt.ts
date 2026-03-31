import type { APIContext } from 'astro'
import { getCollection } from 'astro:content'
import { version, dmgUrls } from '../lib/release'
import latestRelease from '../../public/latest.json'

export async function GET(context: APIContext) {
  const site = context.site!.origin
  const posts = await getCollection('blog')
  const sortedPosts = posts.sort((a, b) => b.data.date.valueOf() - a.data.date.valueOf())

  const blogLines = sortedPosts
    .map((post) => `- [${post.data.title}](${site}/blog/${post.id}/): ${post.data.description}`)
    .join('\n')

  const releaseNotes = latestRelease.notes
    .replace(/\[([a-f0-9]{7})\]\(https:\/\/github\.com\/[^)]+\)/g, '$1')
    .replace(/, [a-f0-9]{7}(, [a-f0-9]{7})*/g, '')
    .replace(/\(([a-f0-9]{7})\)/g, '')
    .replace(/ +/g, ' ')
    .trim()

  const body = `# Cmdr

> The fastest two-pane file manager for macOS. Every folder sized. Every file found.

Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS (Linux in alpha), built with Rust, Tauri 2, and Svelte 5. It indexes your entire drive in minutes, shows directory sizes everywhere, and offers instant search and keyboard-driven everything. AI features (smart search, natural language rename, batch operations) are in active development. Free forever for personal use, source-available under BSL 1.1.

Current version: ${version}
Release date: ${latestRelease.pub_date.split('T')[0]}

## Key links

- [Download (Apple Silicon)](${dmgUrls.aarch64}): DMG installer for Apple Silicon Macs
- [Download (Intel)](${dmgUrls.x86_64}): DMG installer for Intel Macs
- [Download (Universal)](${dmgUrls.universal}): DMG installer that works on both architectures
- [Pricing](${site}/pricing/): Free for personal use, commercial from $59/year
- [Blog](${site}/blog/): Updates and news
- [Changelog](${site}/changelog/): Release notes
- [Roadmap](${site}/roadmap/): What's coming next
- [GitHub](https://github.com/vdavid/cmdr): Source code and issues
- [RSS feed](${site}/rss.xml): Subscribe to blog updates
- [Privacy policy](${site}/privacy-policy/): How we handle your data
- [Terms and conditions](${site}/terms-and-conditions/): Terms of use
- [Refund policy](${site}/refund/): 30-day, no-questions-asked refunds
- [llms.txt](${site}/llms.txt): Concise version of this document

## Features

### Core features

- **Live full-disk index**: Indexes your entire drive once in about 4 minutes. Then stays current forever, even across restarts. Directory sizes everywhere.
- **Blazing fast**: Built in Rust. Opens a 100k-file folder in 4 seconds with icons, sizes, and dates. Startup is near-instant.
- **Keyboard-first**: Navigate, select, copy, move without touching your mouse. Two panes, tabs, command palette. Every action has a keyboard shortcut, and you can customize them all.

### AI features (in active development)

- **Smart search** (rough around the edges): Find files by describing them in plain English: "that PDF contract from last month" or "screenshots with error messages." No need to remember exact file names.
- **Natural language rename** (coming soon): Type "make these lowercase and add date prefix" and watch it happen. No regex, no scripts, just words. Cmdr understands your intent and renames files accordingly.
- **AI batch operations** (coming soon): Organize hundreds of files with a single command. Tell Cmdr to "sort these into folders by project name" and it figures out the rest.
- **Tabs**: Multiple tabs per pane with pinning, persistence, and per-tab sorting.
- **File viewer**: Built-in viewer for text files with search, syntax highlighting, and support for very large files.
- **Drive indexing**: Index your drives for fast search with an efficient integer-keyed database schema.
- **Clipboard**: Full clipboard support with Finder interop. Copy, cut, and paste files between Cmdr and Finder.
- **Delete and trash**: Trash by default, permanent delete with confirmation. Batch operations with progress and cancellation.
- **Network shares**: Connect to SMB network shares with saved credentials, mDNS discovery, and timeout protection.
- **Disk space display**: See free space per volume in the status bar and volume dropdown.
- **Custom tooltips**: Glass-effect tooltips with shortcut badges and smart positioning.
- **Accent color**: Choose between macOS system accent or Cmdr gold, with optional gold folder icons.
- **Command palette**: Quick access to every action in the app.

## Tech stack

- **Rust**: Backend logic, file operations, IPC commands, and drive indexing
- **Tauri 2**: Native desktop framework bridging Rust and the web frontend
- **Svelte 5**: Reactive frontend with TypeScript strict mode
- **Tailwind v4**: Styling with CSS-first configuration
- **SQLite**: Drive indexing with integer-keyed schema for efficient storage
- **Ed25519**: License key signing and verification

## Pricing

### Personal (free forever)

- All features included
- Unlimited machines
- Automatic updates
- No commercial use

### Commercial ($59/year)

- All features included
- Commercial use allowed
- Per user, your own devices
- Auto-renews annually
- Discounted from $79 for the first 1,000 licenses

### Perpetual ($199 one-time)

- All features included
- Commercial use allowed
- Per user, your own devices
- Three years of updates included
- Keep using your version forever after updates expire
- Renew updates at a reduced rate

## Frequently asked questions

### What counts as "commercial use"?

If you're using Cmdr as part of your job (employment, freelancing, consulting), that's commercial use. Side projects, open source work, and personal file management are all fine without a commercial license.

### Can I use it on multiple machines?

Yes! Use Cmdr on as many machines as you like. Laptop, desktop, remote debugging rig, whatever. Your license is per user, not per machine.

### What's the difference between subscription and perpetual?

Subscription ($59/year) auto-renews annually and always includes the latest updates. Perpetual ($199) is a one-time purchase that includes three years of updates. After that, you can keep using your current version forever or renew updates at a reduced rate.

### Can I see the source code?

Yes! Cmdr is source-available on GitHub at https://github.com/vdavid/cmdr. You can view, learn from, and modify the code. The license (BSL 1.1) converts to AGPL-3.0 after three years.

### What if I regret the purchase?

We offer a 30-day, no-questions-asked refund. Send an email and we'll sort it out.

### Do you offer team licenses?

Each person needs their own license. For teams of five or more, email legal@getcmdr.com for volume pricing.

## System requirements

- **macOS**: Apple Silicon (M1 and later) and Intel. Separate DMG installers for each architecture, plus a universal build.
- **Linux**: Alpha support. Volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec, inotify file watching, native file icons via freedesktop-icons.

## License

BSL 1.1 (Business Source License). Source-available: you can view, learn from, and modify the code. The license converts to AGPL-3.0 after three years. See the GitHub repository for full license text.

## Latest release notes (v${version})

${releaseNotes}

## Blog posts

${blogLines}
`

  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  })
}
