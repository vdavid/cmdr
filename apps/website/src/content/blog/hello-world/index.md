---
title: Hello, world
date: 2026-02-18
description:
    Welcome to the Cmdr blog. We'll share product updates, behind-the-scenes stories, and tips for getting the most out
    of your file manager.
cover: ./cover.svg
---

We're excited to launch the Cmdr blog. This is where we'll share what we're building, why we're building it, and how you
can get the most out of Cmdr.

## What to expect

We plan to write about:

- **Product updates** — new features, improvements, and the reasoning behind them
- **Behind the scenes** — the technical decisions that shape Cmdr
- **Tips and workflows** — getting more done with keyboard-driven file management

<!-- more -->

## A quick look at Cmdr

Cmdr is a two-pane file manager built for people who prefer the keyboard. Here's a taste of navigating with it:

```bash
# Jump to a directory
cd ~/projects

# Or use Cmdr's built-in shortcuts
Cmd+G  → Go to path
Cmd+F  → Quick search
Tab    → Switch panes
```

> "The best file manager is the one that gets out of your way." — Every power user, probably

## Built with modern tools

Under the hood, Cmdr uses [Rust](https://www.rust-lang.org/) for the backend and [Svelte](https://svelte.dev/) for the
frontend, all wrapped in [Tauri](https://tauri.app/). This gives us native performance with a modern UI.

![Hello world cover graphic](./cover.svg)

### Why Rust?

Rust gives us memory safety without a garbage collector, which means Cmdr stays fast and lightweight even when handling
thousands of files. No Electron, no bloat.

### Why Svelte?

Svelte compiles to vanilla JavaScript at build time, so there's no virtual DOM overhead. The result is a snappy UI that
feels native.

## Stay in the loop

Follow along on [GitHub](https://github.com/vdavid/cmdr) or subscribe to our newsletter on
[getcmdr.com](https://getcmdr.com). We'd love to hear what you think.
