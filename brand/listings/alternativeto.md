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
A very fast two-pane file manager for macOS with Total Commander-compatible keyboard shortcuts, SMB and MTP/Android support. Optional+privacy-first AI features like natural language search and bulk rename.
```

### Full description

Plain text, their own line breaks, **max 3,000 characters** (the form rejects longer). Same substance as the MacUpdate
description (`macupdate.md`), minus the HTML, plus the licensing and pricing lines that MacUpdate's form bans. The
version below is 2,914 characters, so a new bullet or two needs a trim elsewhere.

```
Cmdr is on macOS what Total Commander is on Windows: familiar shortcuts, two panes, fast and transparent in telling you what's going on with your files. Built with Rust, it's extremely fast and respectful to your CPU, RAM, and disk.

Plus it has two more cool things:
- It indexes your drive for near-instant search and to show you live folder sizes (optional, enabled/disabled at onboarding!)
- AI features like bulk renames and search. (optional, off by default, can run 100% locally, and all destructive operations need human approval.)

Cmdr is in open beta: the core is well-tested software used daily by the author and a group of testers, while the newer features (search, archives, operation log, AI) are marked as "alpha". Your feedback is very much appreciated and goes straight to the developer!

Cmdr is source-available under the Business Source License 1.1, and the source is on GitHub. Free for personal use, commercial license for work use.

Core features:

- Two panes, tabs, command palette, keyboard-first. F5 to copy, F6 to move, F8 to delete, all remappable.
- Copy, move, rename, delete, compress and decompress, with accurate progress bars, ETAs, and cancellation. Built for data safety, speed, and transparency.
- Queue operations, background them, pause and resume transfers, and browse a searchable log of past operations, with rollback where nothing was permanently deleted.
- Lists 50,000 files near-instantly; the built-in viewer opens a 10 GB file near-instantly, too, with search (!)
- Browse zip, tar, and 7z archives like normal folders, and compress/uncompress archives.
- Real dark and light modes, native macOS behavior, WCAG 2.2 AA and APCA verified contrasts, so you can read what's on the screen.
- Translated in good quality to 10 languages.

Extra features:

- Reads/writes Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer, with any USB cable. Just plug it in and it works.
- Uses network drives 4x faster than the macOS client, but for small files it's sometimes 100x.
- Keeps a fully local index of your disk, for live folder sizes and fast search.
- Browse Git history, branches, worktrees, and stashes like normal folders.

AI features (optional, off by default, and can stay fully local with a built-in LLM):

- Natural-language search: "Find my tax report from last year"
- Smart selection: "Select all screenshots in this folder"
- Chat: "Why is my Downloads folder so big?"
- Local image indexing: "Find photos where a dog looks into the camera"
- Natural-language renaming: "Rename these screenshots based on their content." The agent only suggests; you review, apply, and can roll back.
- Auto-organization is coming soon.
- The model runs on your Mac by default; bring your own OpenAI, Claude, or Gemini key (or ollama, etc.) for better models.
- With AI off, Cmdr is a complete Total Commander-style file manager!
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
I added my app like 2 months ago. I've made a bunch of updates since then, so now I've:
- refreshed the desc,
- added supported languages,
- ticked three new features that shipped since the last edit (batch rename, file tagging, zip support),
- and updated the screenshots. They were very obsolete.

I have one ask: Cmdr is source-available under BSL 1.1, not OSI open source. I've kept the "Is Opensource?" box ticked because unticking it hides the Source URL field and the code really _is_ public and I want the GitHub link there, but the (auto-generated-looking) page summary at https://alternativeto.net/software/cmdr/ calls Cmdr "open-source", which is not strictly true in the OSI sense. If you can manually adjust/overwrite that wording to "source available", I'd appreciate it, otherwise never mind, most people don't know the diff anyway.
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
