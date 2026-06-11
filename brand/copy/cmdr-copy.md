# Cmdr copy

Canonical marketing copy: the reusable blurbs that describe what Cmdr is, for AlternativeTo, stores, the newsletter,
social, and any new surface.

The `## Current copy from places` section below is **scaffolding to dissolve**: it collects, verbatim, every spot in the
repo that currently introduces what Cmdr is, so you can spot what's stale, reconcile the divergence, and write the final
versions into the empty sections above it. Once you've populated those, delete the scaffolding section.

---

## One-liner

<!-- One sentence. The elevator pitch. -->

## Tagline

<!-- A few words. The sticky phrase. -->

## Short description

<!-- ~1-2 sentences, for store/listing summaries. -->

## Standard description

<!-- One paragraph, for AlternativeTo and app listings. -->

## Feature list

<!-- The canonical bullet list. -->

## Boilerplate

<!-- Legal entity, license, pricing one-liner, system requirements. -->

---

## Current copy from places (dissolve me)

Collected verbatim on 2026-06-11. Several of these diverge (the tagline alone has three forms); reconcile into the
sections above.

### One-liner — `AGENTS.md`

> An extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
> Downloadable at the website.

### Intro — `README.md`

> An extremely fast, keyboard-driven two-pane file manager for macOS, written in Rust. Source-available, free forever
> for personal use. With fully optional, privacy-first AI features.
>
> Cmdr is for folks who love a rock-solid, keyboard-driven, two-pane file manager with a modern UI in 2026. Like Total
> Commander, but on macOS.

### Tagline + entity — `docs/guides/branding.md`

> The tagline is **"The AI-native file manager"**. The legal entity is **Rymdskottkärra AB**, based in Sweden.

### Website page title / hero — `apps/website/src/pages/index.astro`, `components/Hero.astro`

- Tagline (page title): `Finally, a file manager from {currentYear}!`
- Hero headline: "Finally, a file manager from {currentYear}!"
- Hero subhead: "Indexes your whole drive in minutes. Instant search, visible folder sizes, keyboard-driven everything.
  Built in Rust, free for personal use."
- Pricing hint: "Source-available · Free forever for personal use · Commercial from $59/year"

### Meta description — `apps/website/src/layouts/Layout.astro`

> The fastest two-pane file manager for macOS. Indexes your whole drive in minutes, shows folder sizes everywhere,
> instant search, keyboard-driven everything. Built in Rust, free for personal use.

### Agent-facing summary — `apps/website/src/pages/llms.txt.ts`

> The fastest two-pane file manager for macOS. Every folder sized. Every file found.
>
> Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS, built with Rust, Tauri 2, and Svelte 5.
> It indexes your entire drive in minutes, shows directory sizes everywhere, and offers instant search. Free forever for
> personal use, source-available under BSL 1.1.

### Agent-facing summary (full) — `apps/website/src/pages/llms-full.txt.ts`

> Cmdr is an extremely fast, keyboard-driven, two-pane file manager for macOS (Linux in alpha), built with Rust, Tauri
> 2, and Svelte 5. It indexes your entire drive in minutes, shows directory sizes everywhere, and offers instant search
> and keyboard-driven everything. AI features (smart search, natural language rename, batch operations) are in active
> development. Free forever for personal use, source-available under BSL 1.1.

### Feature list — `apps/website/src/pages/llms.txt.ts`

> - **Live full-disk index**: Indexes your entire drive once in about 4 minutes. Then stays current forever, even across
>   restarts.
> - **Blazing fast**: Built in Rust. Opens a 100k-file folder in 4 seconds with icons, sizes, and dates.
> - **Keyboard-first**: Navigate, select, copy, move without touching your mouse. Two panes, tabs, command palette.
> - **Smart search** (rough around the edges): Find files by describing them: "that PDF contract from last month" or
>   "screenshots with error messages."
> - **Natural language rename** (coming soon): Type "make these lowercase and add date prefix" and watch it happen. No
>   regex, no scripts.
> - **AI batch operations** (coming soon): Organize hundreds of files with a single command. "Sort these into folders by
>   project name."

### README core features — `README.md`

> - **Two-pane layout**: see two folders side by side.
> - **Keyboard-first**: do anything without touching your mouse, using your familiar shortcuts.
> - **Blazing fast file operations**: copy, move, rename, and delete with a few keystrokes.
> - **Optional, privacy-first AI**: search and select with natural language, all on your Mac.

### Footer tagline — `apps/website/src/components/Footer.astro`

> Fast, keyboard-driven file manager

### About window — `apps/desktop/src/lib/licensing/AboutWindow.svelte`

> Keyboard-driven file manager
