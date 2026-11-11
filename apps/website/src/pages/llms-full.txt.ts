import type { APIContext } from 'astro'
import { getCollection } from 'astro:content'
import { version, dmgUrls } from '../lib/release'
import { CHAPTER_IDS, cardsByChapter, chapterLabels } from '../lib/features-page'
import latestRelease from '../../public/latest.json'
import { shippedLanguageCount } from '../lib/shipped-languages'

export async function GET(context: APIContext) {
  const site = context.site!.origin
  const posts = await getCollection('blog')
  const sortedPosts = posts.sort((a, b) => b.data.date.valueOf() - a.data.date.valueOf())

  const blogLines = sortedPosts
    .map((post) => `- [${post.data.title}](${site}/blog/${post.id}/): ${post.data.description}`)
    .join('\n')

  // Generated from `feature-status.json`, grouped like the features page, so it can't go stale.
  const featureLines = CHAPTER_IDS.map((chapter) => {
    const items = cardsByChapter[chapter].map(
      (card) => `- **${card.title}** (${card.feature.status}): ${card.feature.note}`,
    )
    return `### ${chapterLabels[chapter]}\n\n${items.join('\n')}`
  }).join('\n\n')

  const releaseNotes = latestRelease.notes
    .replace(/\[([a-f0-9]{7})\]\(https:\/\/github\.com\/[^)]+\)/g, '$1')
    .replace(/, [a-f0-9]{7}(, [a-f0-9]{7})*/g, '')
    .replace(/\(([a-f0-9]{7})\)/g, '')
    .replace(/ +/g, ' ')
    .trim()

  const body = `# Cmdr

> The fastest two-pane file manager for macOS. Every folder sized. Every file found.

Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS (Linux in alpha), built with Rust, Tauri 2, and Svelte 5. It indexes your entire drive in minutes, shows directory sizes everywhere, and offers instant search and keyboard-driven everything. Optional AI features (natural-language search, Ask Cmdr chat, AI rename, and AI file organization) are off by default and can run fully on-device. Translated into ${shippedLanguageCount} languages. Free for personal use, source-available under BSL 1.1.

Current version: ${version}
Release date: ${latestRelease.pub_date.split('T')[0]}

## Key links

- [Download (Apple Silicon)](${dmgUrls.aarch64}): DMG installer for Apple Silicon Macs
- [Download (Intel)](${dmgUrls.x86_64}): DMG installer for Intel Macs
- [Download (Universal)](${dmgUrls.universal}): DMG installer that works on both architectures
- [Live demo](${site}/demo/): The real app running in the browser on a sample drive, nothing to install
- [Pricing](${site}/pricing/): Free for personal use, $59 once for commercial use
- [Blog](${site}/blog/): Updates and news
- [Changelog](${site}/changelog/): Release notes
- [Roadmap](${site}/roadmap/): What's coming next
- [GitHub](https://github.com/getcmdr/cmdr): Source code and issues
- [RSS feed](${site}/rss.xml): Subscribe to blog updates
- [Privacy policy](${site}/privacy-policy/): How we handle your data
- [Terms and conditions](${site}/terms-and-conditions/): Terms of use
- [Refund policy](${site}/refund/): 30-day, no-questions-asked refunds
- [llms.txt](${site}/llms.txt): Concise version of this document

## Features

Every feature with its status (stable, beta, alpha, or planned) and an honest one-line note, the same list the [features page](${site}/features/) shows beside its product clips. AI features are optional and off by default.

${featureLines}

## Tech stack

- **Rust**: Backend logic, file operations, IPC commands, and drive indexing
- **Tauri 2**: Native desktop framework bridging Rust and the web frontend
- **Svelte 5**: Reactive frontend with TypeScript strict mode
- **Tailwind v4**: Styling with CSS-first configuration
- **SQLite**: Drive indexing with integer-keyed schema for efficient storage
- **Ed25519**: License key signing and verification

## Pricing

### Personal (free)

- The whole file manager
- Cmdr AI, free during the beta, on your own provider key or the on-device model (a Pro plan with hosted models may come later; the file manager stays free for personal use)
- Unlimited machines
- Automatic updates
- No commercial use

### Commercial ($59, paid once)

- All features included, Cmdr AI included
- Commercial use allowed
- Per user, your own devices
- One year of updates included
- After that year, $39/year to keep receiving updates, and the last version you were entitled to stays yours forever

### Enterprise

For larger teams and organizations: your IT department deploys Cmdr, your security team reviews it, and you get one contract and one invoice against your PO. Quoted by hand, with no published price. Email sales@getcmdr.com.

## Frequently asked questions

### What counts as "commercial use"?

If you're using Cmdr as part of your job (employment, freelancing, consulting), that's commercial use. Side projects, open source work, and personal file management are all fine without a commercial license.

### Can I use it on multiple machines?

Yes! Use Cmdr on as many machines as you like. Laptop, desktop, remote debugging rig, whatever. Your license is per user, not per machine.

### What happens after my year of updates runs out?

Nothing breaks. The last version you were entitled to keeps working, for as long as you like, on as many of your machines as you like. Updates after the first year are $39 a year, and skipping a year and coming back later is fine.

### Do I pay extra for Cmdr AI?

Not during the beta: Cmdr AI is free for everyone right now. It runs on an API key you bring from a provider you pick, or fully on-device with a local model. A Pro plan that includes hosted models, so no API key is needed, may come later. The file manager itself stays free for personal use, and a Commercial license bought today keeps Cmdr AI with your own key or the on-device model for as long as your updates run.

### Can I see the source code?

Yes! Cmdr is source-available on GitHub at https://github.com/getcmdr/cmdr. You can view, learn from, and modify the code. The license (BSL 1.1) converts to AGPL-3.0 after three years.

### What if I regret the purchase?

We offer a 30-day, no-questions-asked refund. Send an email and we'll sort it out.

### Do you offer team and enterprise licenses?

Yes. For a handful of people, buying Commercial licenses one by one is the quickest route, and each person can use theirs on all of their machines. Once your IT department has to deploy Cmdr, your security team has to sign off on it, or finance wants one invoice against a PO, that's an enterprise deal: email sales@getcmdr.com with your headcount and what you need, and you'll get a quote back.

## System requirements

- **macOS**: 12 or newer, on Apple Silicon (M1 and later) and Intel. Separate DMG installers for each architecture, plus a universal build.
- **Older macOS**: 10.15 Catalina and 11 Big Sur install and run on a best-effort basis. Everything works, but a few colors and layout details can look off, and fixes for those systems are a lower priority.
- **Linux**: Alpha support. Volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec, inotify file watching, native file icons via freedesktop-icons.

## License

BSL 1.1 (Business Source License). Source-available: you can view, learn from, and modify the code. The license converts to AGPL-3.0 after three years. See the GitHub repository for full license text.

## Latest release notes (v${version})

${releaseNotes}

## Blog posts

Each post is also available as raw Markdown: append \`index.md\` to its URL.

${blogLines}
`

  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  })
}
