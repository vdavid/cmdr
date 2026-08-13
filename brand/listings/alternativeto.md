# AlternativeTo listing

Live: https://alternativeto.net/software/cmdr/about/. Edit it while signed in; changes go through an admin approval
queue, and the "Note about your changes" field at the bottom speeds that up.

Status: live, with a refresh pending. The fields below are the values to paste; the live listing still shows the
previous ones (English-only languages, a description that predates bulk rename and the disk index, and three unchecked
features that are now true). Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing
the app-directory listings".

## Main info

- **Name**: `Cmdr`
- **Website**: `https://getcmdr.com`
- **Short description** (their hint: one concise sentence on the main purpose):

```
An extremely fast Total Commander alternative for macOS: a two-pane file manager with SMB and MTP/Android support, with optional and privacy-first AI features like natural language search and bulk rename.
```

### Full description

Plain text, their own line breaks. Same substance as the MacUpdate description (`macupdate.md`), minus the HTML, plus
the licensing and pricing lines that MacUpdate's form bans.

```
Cmdr brings the Total Commander experience to macOS, and (optionally, only if you enable it) adds AI features that genuinely help. Built with Rust, it's extremely fast and respectful toward your CPU, RAM, and disk.

Cmdr is in open beta. There might be sharp edges in the newer features (search, archives, the operation log, and AI), but the core is well-tested, stable software used every day by the author and a group of testers. Feedback goes straight to the developer!

Cmdr is source-available under the Business Source License 1.1: the full source is on GitHub (https://github.com/vdavid/cmdr), though BSL isn't an OSI-approved open-source license, so the "open source" label on this page is auto-generated rather than mine. Free forever for personal use, and a commercial license is needed for work use.

Core features:

- Two panes, tabs, command palette, keyboard-first. Common shortcuts like F5 to copy, F6 to move, F8 to delete all work, and are remappable.
- Browse, copy, move, rename, delete, compress/decompress with accurate progress bars, honest ETAs, cancellation. Optimized for data safety, speed, and transparency.
- Queue multiple file operations, send any of them to the background, pause/resume file transfers, view a full, searchable log of past operations, with rollback for anything that didn't permanently delete data.
- Very fast: lists 50,000 files near-instantly, and the built-in viewer opens a 10 GB file near-instantly with fast search.
- Browse zip, tar, and 7z archives like normal folders, and write into zip.
- Real dark and light modes, native macOS behavior, and all text color / background contrasts verified against WCAG 2.2 AA and APCA.
- Speaks 10 languages.

Extra features:

- Full access to Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer, no hacks needed, works out of the box with any USB cable.
- Full access to network drives over a custom SMB implementation, roughly 4x faster than the macOS client.
- Keeps a full index of your disk (fully local and private) and uses it to display live folder sizes for all your folders, and for near-instant full-drive search. A folder that isn't indexed yet gets walked live, with matches arriving as they're found.
- For Git repositories, it shows a Git history, branches, worktrees, and stashes browsable like normal folders.

AI features (entirely optional, can be fully local and private with a built-in LLM):

- With AI features switched off, Cmdr is a complete Total Commander-style file manager. Many people don't like AI features, so they are off by default.
- Switched on, it adds natural-language search: "Find my tax report from last year"
- Smart selection: "Select all screenshots in this folder"
- Chat: "Why is my Downloads folder so big?"
- Image indexing (fully local and private!): "Find me all photos in this folder where a dog looks into the camera."
- Natural-language renaming: "Rename all these screenshots based on their content." The agent can only suggest write operations like renames, you are in charge of reviewing and applying them. If you change your mind, you can always roll back any past operations.
- Auto-organization is on the way.
- The model runs on your Mac by default, so your files and data stay 100% private. You can choose to bring your own OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint to use more powerful models.
```

## More info

- **Supported languages**: English, German, Spanish, French, Hungarian, Dutch, Portuguese, Swedish, Vietnamese, Chinese.
  (The 10 the app ships: `apps/desktop/src/lib/intl/messages`.)
- **Pricing**: `Free for personal use`. Model `Purchase`, min `$59`, max `$199`
- **Is Opensource?**: checked. License `Other`. Source URL `https://github.com/vdavid/cmdr`

## Tags

Current tags: `total-commander`, `ai`, `finder-alternative`, `built-in-file-manager`, `macos`, `offline-access`,
`svelte`, `finder`, `dual-pane`, `ad-free`, `no-registration`, `lightweight`, `rust`, `privacy-focused`,
`built-in-viewer`, `tauri`, `two-pane`, `portable`, `file-management`, `file-manager`, `live-preview`, `rust-based`,
`rust-lang`, `night-mode`, `smb`, `mtp`, `git`, `file-search`, `batch-rename`, `batch-renamer`, `archive-manager`,
`keyboard-driven`, `keyboard-shortcuts-support`, `semantic-search`

## Application type

Check **File Manager**, **File Search Utility** (the full-disk index and instant search), and **File Archiver** (zip,
tar, and 7z browsing and extraction, zip writing). They ask for two to three per app, so those three fill it.

Leave unchecked: File Compressor, File Sync Tool, FTP Client.

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
Refreshed the description and languages for the current version, and ticked three features that shipped since the last edit (batch rename, file tagging, zip support).

One correction I can't make myself: Cmdr is source-available under BSL 1.1, not OSI open source. I've kept the "Is Opensource?" box ticked because unticking it hides the Source URL field and the code really is public, but the page summary calls Cmdr "open-source", which overstates it. If you can adjust that wording, I'd appreciate it.
```

## Still pending after this pass

- **Pricing model**: `Purchase` with a $59–$199 range renders on the page as "One time purchase (perpetual license)
  ranging between $59 and $199", which misstates the $59/year subscription. Revisit when the new pricing ships.

## Settled calls

So they don't get re-litigated at the next edit pass:

- **Keep "Is Opensource?" checked.** Unchecking it hides the Source URL field, and a public link to the source is worth
  more than a precise badge. The description carries the BSL correction instead, in its third paragraph rather than as
  fine print, and the note above asks the admins to fix the page summary they generate.
- **Keep the `portable` tag and the Portable feature.** True in the only sense the tag can mean on macOS: a `.app` runs
  from wherever you put it, a USB stick included. Settings and the index stay in `~/Library` and Full Disk Access is per
  machine, but that's every Mac app, and nobody reads the tag as a promise about state.
