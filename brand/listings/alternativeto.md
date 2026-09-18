# AlternativeTo listing

Live: https://alternativeto.net/software/cmdr/about/. Edit it while signed in; changes go through an admin approval
queue, and the "Note about your changes" field at the bottom speeds that up.

Status: submitted 2026-09-18 for v0.46.0, together with the note to the admins below (the same BSL wording request went
in with the v0.45.0 refresh). The fields below are what was submitted. Edit them here first when refreshing, then paste.
Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory listings".

## Main info

- **Name**: `Cmdr`
- **Website**: `https://getcmdr.com`
- **Short description** (their hint: one concise sentence on the main purpose):

```
A very fast two-pane file manager for macOS with Total Commander-compatible keyboard shortcuts, SMB, SFTP, WebDAV, and MTP/Android support. Optional+privacy-first AI features like natural language search and bulk rename.
```

### Full description

Plain text, their own line breaks, **max 3,000 characters** (the form rejects longer). Same substance as the MacUpdate
description (`macupdate.md`), minus the HTML, plus the licensing and pricing lines that MacUpdate's form bans. Measure
it after every edit; there's little room left under the cap.

```
Cmdr is on macOS what Total Commander is on Windows: familiar shortcuts, two panes, fast and transparent. Built with Rust, it's extremely fast and respectful of your CPU, RAM, and disk.

Plus it has two more cool things:

- It indexes your drive for near-instant search and shows you live folder sizes (optional, can be disabled at onboarding)
- AI features like bulk renames and search (optional, off by default, can run 100% locally, and all destructive operations need human approval)

Cmdr is in open beta: the core is well-tested and used daily by the author and a group of testers, while the newer features (archives, operation log, AI, SFTP, WebDAV, ADB) are marked as "alpha". Feedback goes straight to the developer!

Cmdr is source-available under the Business Source License 1.1, and the source is on GitHub. Free for personal use, commercial license for work use.

Core features:

- Basics: Two panes, tabs, command palette, keyboard-first. F5 to copy, F6 to move, F8 to delete, all remappable.
- Operations: Copy, move, rename, delete, with accurate progress bars, ETAs, and cancellation. Built for data safety, speed, and transparency.
- Advanced ops: Queue operations, background them, pause/resume, browse op log, roll back ops, even days later.
- Browse zip, tar, and 7z files like folders, and create or extract them.
- Speed: Lists 50,000 files near-instantly; the built-in viewer opens a 10 GB file in 1 sec, with search. (!)
- Accessibility: Real dark and light modes, native macOS behavior, WCAG 2.2 AA and APCA verified contrasts.
- Localization: Translated into 13 languages.

Extra features:

- USB: Reads/writes Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer, with any USB cable. Also ADB support for Android!
- SMB: Accesses network drives 4-200x faster than the macOS client. For small files and listing dirs, it's waay faster than Finder!
- SFTP: Use your server like a local drive
- WebDAV: Same
- Git: Browse Git history, branches, worktrees, and stashes like normal folders.
- Supports Dropbox's and Google Drive's own actions, like "Copy Dropbox link" and "Share with Google Drive".
- Drive index: Keeps a fully local index of your disk, for live folder sizes and fast search.

AI features (optional, off by default, and can stay fully local with a built-in LLM):

- Natural-language search: "Find my tax report from last year"
- Smart selection: "Select all screenshots in this folder"
- Chat: "Why is my Downloads folder so big?"
- Local image indexing: "Find photos where a dog looks into the camera"
- Natural-language renaming: "Rename these screenshots based on their content." The agent only suggests; you review, apply, and can roll back.
- File organization: "Clean up my Downloads folder". You approve each move.
- The model runs on your Mac by default; bring your own OpenAI, Claude, or Gemini key (or ollama, etc.) for better models.
- With AI off, Cmdr is a complete Total Commander-style file manager!
```

## More info

- **Supported languages**: English, German, Spanish, French, Hungarian, Dutch, Portuguese, Swedish, Vietnamese, Chinese.
  (The 13 locales the app ships, with British and Australian English folded into English:
  `apps/desktop/src/lib/intl/messages`. Traditional/Simplified Chinese split is not supported in their list.)
- **Pricing**: `Free for personal use`. Model `Purchase`, min `$59`, max `$59`. One figure in both fields, so the page
  renders a single price rather than a range: v0.46.0 retired the $59/year subscription and the $199 perpetual, leaving
  Commercial at $59 paid once (a year of updates included, $39/year after that to keep getting them). Enterprise
  publishes no number, so it contributes none here.
- **Is Opensource?**: checked. License `Other`. Source URL `https://github.com/vdavid/cmdr`

## Tags

Current tags: `total-commander`, `ai`, `finder-alternative`, `built-in-file-manager`, `macos`, `offline-access`,
`svelte`, `finder`, `dual-pane`, `ad-free`, `no-registration`, `lightweight`, `rust`, `privacy-focused`,
`built-in-viewer`, `tauri`, `two-pane`, `portable`, `file-management`, `file-manager`, `live-preview`, `rust-based`,
`rust-lang`, `night-mode`, `smb`, `mtp`, `git`, `file-search`, `batch-rename`, `batch-renamer`, `archive-manager`,
`keyboard-driven`, `keyboard-shortcuts-support`, `semantic-search`, `sftp`, `sftp-clients`, `webdav`, `nextcloud`,
`adb`, `android`

## Application type

Check **File Manager**, **File Search Utility** (the full-disk index and instant search), and **File Archiver** (zip,
tar, and 7z browsing and extraction, zip writing). They ask for two to three per app, so those three fill it.

Leave unchecked: File Compressor, File Sync Tool, FTP Client (Cmdr speaks SFTP but no plain FTP, and SFTP is still
alpha; the three above describe it better).

## Features

Check: No registration required, Portable, Privacy focused, Lightweight, Ad-free, Works offline, Dark mode, Built-in
viewer, Live preview, plus the three the live listing is missing: **Batch rename files** (natural-language bulk rename),
**File tagging** (Finder tags, `file_system/tags.rs`), and **Supports zip files**.

Leave unchecked, deliberately:

- **Full-text search**: the index covers names and metadata, not the text inside documents.
- **No tracking**: Cmdr sends anonymous analytics.
- **Support for themes**: dark/light and accent colors aren't user-authored themes.
- **Extensible by plugins/extensions** and **Windows Explorer extension**: not a thing.
- **AES-256 encryption** (appears once File Archiver is ticked): Cmdr opens AES-256 and ZipCrypto archives, prompting
  for the password, but creates no encrypted archives and refuses edits that would retain an encrypted entry
  (`crates/cmdr-archive`). The checkbox reads as "can encrypt", so it stays off until that ships.
- **Command line interface**

## Platforms

`Mac`. No Mac App Store link, no platform note. (Linux is alpha and deliberately not listed.)

## Author and social

- **Company / author**: `vdavid`, country of origin `Sweden`, website `https://getcmdr.com`
- **X username**: `vdavid`. No Facebook URL.

## Icon and screenshots

- **Icon**: the current app icon.
- **Screenshots**: replace all three live ones ("Cmdr v0.24 (light)", "Cmdr v0.24 (dark)", "Settings (light)"), which
  predate the 0.36 facelift, with these, captioned without a version number so they don't advertise their own age:
  1. `brand/screenshots/app-main-light.webp` — "Two-pane main view (light)"
  2. `brand/screenshots/app-main-dark.webp` — "Two-pane main view (dark)"
  3. `brand/screenshots/search-light.webp` — "Search your files (light)"
  4. `brand/screenshots/search-dark.webp` — "Search your files (dark)"
  5. `brand/screenshots/chat-light.webp` — "Ask Cmdr about your files (light)"
  6. `brand/screenshots/chat-dark.webp` — "Ask Cmdr about your files (dark)"
  7. `brand/screenshots/settings-light.webp` — "Settings (light)"
  8. `brand/screenshots/settings-dark.webp` — "Settings (dark)"

  Limit is 3 MB each; JPEG, PNG, or WebP, so the lossless WebP masters upload as they are. If their uploader ever balks:
  `magick app-main-light.webp app-main-light.png`. Reshoot per `docs/guides/screenshots.md`. YouTube videos can be added
  by URL.

## Note about your changes

Their optional box for the reviewing admin. Use it to head off the licensing mislabel:

```
Hi! Cmdr got a bunch of new stuff since my last edit (2026-08-13), so I've:
- refreshed the desc (SFTP, WebDAV, and ADB support, cloud provider right-click menus, AI file organization),
- added Traditional Chinese to the supported languages,
- added four tags: sftp, webdav, adb, android,
- and corrected the price: it's one $59 purchase now, not a range. I retired the old $59/year subscription and the $199 perpetual, so "between $59 and $199" no longer describes anything I sell.

Also, my ask from last time (if it's not done already): Cmdr is source-available under BSL 1.1, not OSI open source. I keep the "Is Opensource?" box ticked because unticking it hides the Source URL field, and the code really _is_ public, but the page summary at https://alternativeto.net/software/cmdr/ calls Cmdr "open-source", which isn't strictly true in the OSI sense. If you can change that wording to "source available", I'd appreciate it. Otherwise never mind, most people don't know the diff anyway. :)
```

## Still pending after this pass

Nothing. The pricing mismatch that sat here through several passes is gone: `Purchase` was always the right model and
only the range was wrong, and v0.46.0's move to a single $59 one-time price makes the page's "One time purchase
(perpetual license)" wording true as written.

## Settled calls

So they don't get re-litigated at the next edit pass:

- **The full description sits at 2,999 of their 3,000-character cap.** Every addition has to be paid for by a cut, so
  measure before and after; the form rejects an over-long one outright. Nothing in v0.46.0 made the description false
  (no feature graduated out of alpha, no claim broke, and the description names no prices), so that release changed
  nothing here, per `docs/guides/releasing.md` § "Refreshing the app-directory listings".

- **Keep "Is Opensource?" checked.** Unchecking it hides the Source URL field, and a public link to the source is worth
  more than a precise badge. The description carries the BSL correction instead, in its third paragraph rather than as
  fine print, and the note above asks the admins to fix the page summary they generate.
- **Keep the `portable` tag and the Portable feature.** True in the only sense the tag can mean on macOS: a `.app` runs
  from wherever you put it, a USB stick included. Settings and the index stay in `~/Library` and Full Disk Access is per
  machine, but that's every Mac app, and nobody reads the tag as a promise about state.
