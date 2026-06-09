# Cmdr

![License](https://img.shields.io/badge/license-BSL--1.1-blue)

An extremely fast, keyboard-driven two-pane file manager for macOS, written in Rust. Source-available, free forever for
personal use. With fully optional, privacy-first AI features.

Cmdr is for folks who love a rock-solid, keyboard-driven, two-pane file manager with a modern UI in 2026. Like Total
Commander, but on macOS.

Give it a try: [Download for macOS](https://getcmdr.com) on the website.

![cmdr](https://github.com/user-attachments/assets/7827b88d-e0a9-447e-b195-af7216c0fa35)

## Overview

I (David, the dev) loved Total Commander on Windows, used it for 20+ years. Then I switched to macOS, and my biggest
pain point about the OS was a fast, rock-solid, and pleasant file manager. Cmdr intends to fix this.

Then there is AI. With LLMs, some really cool features became possible that never were. I'm experimenting with adding
these features gradually. But the intention is that AI features remain fully **optional**. With AI features off, you've
got a Total Commander-like experience. With it, you get natural-language search and smart selection, with smart renaming
and (human-approved!) auto-organization coming soon. AI features are local-by-default, so no files or other data leave
your Mac.

Core features:

- **Two-pane layout**: see two folders side by side.
- **Keyboard-first**: do anything without touching your mouse, using your familiar shortcuts.
- **Blazing fast file operations**: copy, move, rename, and delete with a few keystrokes.
- **Optional, privacy-first AI**: search and select with natural language, all on your Mac.

## Installation

Download it from [getcmdr.com](https://getcmdr.com). (`brew install -- cask cmdr` will be available as soon as this repo
hits 50+ forks, 50+ watchers, and 100+ stars. These are
[Brew's constraints](https://docs.brew.sh/Acceptable-Casks#rejected-casks) plus some margin to make sure it's accepted.
You can help today by starring/forking/watching the repo.)

Windows and Linux users: sorry, you'll need to wait. The Rust+Tauri stack allows for cross-platform deployment, but the
app uses OS-specific features by nature, so I've only had time to write and test it on macOS for now.

## Usage

Launch Cmdr and start navigating:

| Key     | Action               |
| ------- | -------------------- |
| `Tab`   | Switch between panes |
| `↑` `↓` | Navigate files       |
| `Enter` | Open file/folder     |
| `F5`    | Copy                 |
| `F6`    | Move                 |
| `F7`    | Create folder        |
| `F8`    | Delete               |

## Tech stack

Cmdr is built with **Rust** and **Tauri** for the backend, and **Svelte** with **TypeScript** for the frontend. This
gives it native performance with a modern, responsive UI.

## License

Cmdr is **source-available** under the [Business Source License 1.1](LICENSE).

### Free for personal use

Use Cmdr for free on any number of machines for personal, non-commercial projects. No nags, no trial timers, no
restrictions.

### Commercial use

For work projects, you'll need a license:

- **$59/year**: subscription, auto-renews
- **$199 one-time**: perpetual license

Purchase at [getcmdr.com/pricing](https://getcmdr.com/pricing).

### Source code

The source becomes [AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html) after 3 years (rolling per release). Until
then, you can view, modify, and learn from the code, but not use it commercially without a license.

---

## Contributing

Contributions are welcome! Report issues and feature requests in the
[issue tracker](https://github.com/vdavid/cmdr/issues).

By submitting a contribution, you agree to license your contribution under the same terms as the project (BSL 1.1,
converting to AGPL-3.0) and grant the project owner the right to use your contribution under any commercial license
offered for this project.

Happy browsing!

David
