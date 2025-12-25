# Rusty Commander

![License](https://img.shields.io/github/license/vdavid/rusty-commander)

An extremely fast, keyboard-driven, two-pane file manager written in Rust for folks who miss the golden days of
Norton Commander and Total Commander.

## Overview

Rusty Commander is a desktop file manager that brings back the classic two-pane layout.
It's built for speed and keyboard navigation.
If you've ever used Norton Commander, Midnight Commander, or Total Commander, you'll feel right at home.

Core features:
- **Two-pane layout**: see two directories side by side
- **Keyboard-first navigation**: do everything without touching your mouse
- **Fast file operations**: copy, move, rename, and delete with a few keystrokes
- **Cross-platform**: runs on macOS, Windows, and Linux

## Installation

Download the latest release for your platform from the [Releases](https://github.com/vdavid/rusty-commander/releases) page.

### macOS

```bash
# Coming soon: Homebrew tap
brew install --cask rusty-commander
```

### Windows

(Coming soon)

Download the `.msi` installer from the releases page and run it.

### Linux

```bash
# Coming soon: Flatpak or AppImage
flatpak install rusty-commander
```

## Usage

Launch Rusty Commander and start navigating:

| Key     | Action               |
|---------|----------------------|
| `Tab`   | Switch between panes |
| `↑` `↓` | Navigate files       |
| `Enter` | Open file/folder     |
| `F5`    | Copy                 |
| `F6`    | Move                 |
| `F7`    | Create folder        |
| `F8`    | Delete               |

## Tech stack

Rusty Commander is built with **Rust** and **Tauri** for the backend, and **Svelte** with **TypeScript** for the
frontend. This gives it native performance with a modern, responsive UI.

## Contributing

Contributions are welcome!
Report issues and feature requests in the [issue tracker](https://github.com/vdavid/rusty-commander/issues).

Happy browsing!

David
