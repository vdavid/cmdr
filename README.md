# Cmdr

![License](https://img.shields.io/badge/license-BSL--1.1-blue)

An extremely fast AI-native file manager written in Rust, free forever for personal use on macOS.

Cmdr is for folks who love a rock-solid, keyboard-driven, two-pane file manager with a modern UI in 2026.

Give it a try: [Download for macOS](https://getcmdr.com) on the website, or do `brew install cmdr`.

![cmdr](https://github.com/user-attachments/assets/7827b88d-e0a9-447e-b195-af7216c0fa35)

## Overview

Cmdr is the first AI-native file manager, written by modern standards with built-in AI to support natural language
search, smart renaming, and auto-organization. Built on the spiritual foundations of `mc` and Total Commander.

Core features:

- **Two-pane layout**: see two dirs side by side.
- **Keyboard-first**: do anything without touching your mouse, using your familiar shortcuts.
- **Blazing fast file operations**: copy, move, rename, and delete with a few keystrokes
- **AI native**: search, rename, organize like you're in 2026.

## Installation

Download from [getcmdr.com](https://getcmdr.com) or just do `brew install cmdr`.

Windows and Linux users: sorry, you'll need to wait. The Rust+Tauri stack allows for cross-platform deployment, but the app
uses OS-specific features by nature, so I've only had time to write and test it on macOS for now.

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

- **$59/year** — subscription, auto-renews
- **$149 one-time** — perpetual license

Purchase at [getcmdr.com/pricing](https://getcmdr.com/pricing).

### Source code

The source becomes [AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html) after 3 years (rolling per release). Until
then, you can view, modify, and learn from the code — just not use it commercially without a license.

---

## Contributing

Contributions are welcome! Report issues and feature requests in the
[issue tracker](https://github.com/vdavid/cmdr/issues).

By submitting a contribution, you agree to license your contribution under the same terms as the project (BSL 1.1,
converting to AGPL-3.0) and grant the project owner the right to use your contribution under any commercial license
offered for this project.

Happy browsing!

David
