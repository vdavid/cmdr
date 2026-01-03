# Rusty Commander

![License](https://img.shields.io/github/license/vdavid/rusty-commander)

An extremely fast, keyboard-driven, two-pane file manager written in Rust for folks who miss the golden days of Norton
Commander and Total Commander.

![rusty-commander](https://github.com/user-attachments/assets/d50c1b19-f947-47b6-8b29-b800b0f1ce31)

## Overview

<img alt="Rusty Commander logo" src="./src-tauri/icons/128x128.png" width="128" height="128" style="display:block; margin:0 auto;" />

Rusty Commander is a desktop file manager that brings back the classic two-pane layout. It's built for speed and
keyboard navigation. If you've ever used Norton Commander, Midnight Commander, or Total Commander, you'll feel right at
home.

Core features:

- **Two-pane layout**: see two directories side by side
- **Keyboard-first navigation**: do everything without touching your mouse
- **Fast file operations**: copy, move, rename, and delete with a few keystrokes
- **Cross-platform**: runs on macOS, Windows, and Linux

## Installation

Download the latest release for your platform from the [Releases](https://github.com/vdavid/rusty-commander/releases)
page.

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
| ------- | -------------------- |
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

## License

Rusty Commander is available under a **dual license**:

### Open source (AGPL-3.0-or-later)

For open-source projects, personal use, and those who can comply with the
[GNU Affero General Public License v3](https://www.gnu.org/licenses/agpl-3.0.html), Rusty Commander is free to use,
modify, and distribute. The AGPL requires that if you modify Rusty Commander and make it available to users over a
network, you must also make your source code available under the same license.

### Commercial license

For companies and individuals who cannot comply with the AGPL (e.g., you want to use Rusty Commander in proprietary
software, or you don't want to disclose your source code), a commercial license is available.

**Contact**: [veszelovszki@gmail.com](mailto:veszelovszki@gmail.com) for pricing and terms.

---

## Contributing

Contributions are welcome! Report issues and feature requests in the
[issue tracker](https://github.com/vdavid/rusty-commander/issues).

By submitting a contribution, you agree to license your contribution under the AGPL-3.0-or-later license and grant the
project owner the right to use your contribution under both the AGPL and any commercial license offered for this
project.

Happy browsing!

David
