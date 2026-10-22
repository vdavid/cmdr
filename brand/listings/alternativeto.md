# AlternativeTo listing

Live: https://alternativeto.net/software/cmdr/about/. Edit it while signed in; changes go through an admin approval
queue, and the "Note about your changes" field at the bottom speeds that up.

Status: the 2026-09-18 v0.46.0 edit is live (checked 2026-09-24: description, tags, and the single $59 price all match
this file). The fields below are what went in with the v0.53.0 edit, **submitted 2026-10-15, awaiting admin approval**.
The form first said "Cmdr has pending changes" (likely an earlier edit still in their queue), then reported success, so
check the live page in a week or two; the last change before it went live 27 days earlier. Edit the fields here first
when refreshing, then paste. Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing
the app-directory listings".

v0.47.0 changed nothing here (checked 2026-09-24): AlternativeTo shows no version or changelog, and nothing in the
description, tags, or features became false. Its new features (the viewer's long lines, favorite letter shortcuts, the
"Allow cloud AI" switch) are too small to buy room under the 3,000-character cap.

v0.48.0 and v0.49.0 changed one line (checked 2026-10-01): Latin American Spanish made it 14 languages, a same-length
edit. Their other additions (hex and binary viewer modes, tab drag and drop, streaming compression, SFTP free space)
don't make anything false and are too small to buy room under the cap.

v0.53.0 rewrote the description (2026-10-15) in the voice of David's MacUpdate rewrite: labeled bullets, plainer claims,
and the newer features (multi-rename, VoiceOver, S3, on-server SMB copies). It also fixed the language count: Russian
shipped in v0.50.0, so it's 15, not 14.

v0.54.0 and v0.55.0 changed nothing here (checked 2026-10-22), so there's nothing to submit for them. No line became
false: still 15 languages, and the alpha features the description names are still alpha (`feature-status.json`). Restic
backups (new, alpha), Markdown in the viewer, WebDAV trusting a NAS's self-signed certificate, and the data-safety sweep
add to what's there without contradicting it, and with 8 characters left under the cap, adding any of them means cutting
something else. The Features box "Command line interface" stays unchecked: the new `cmdr <path>` command only opens a
folder in the app, which isn't what that box promises.

## Main info

- **Name**: `Cmdr`
- **Website**: `https://getcmdr.com`
- **Short description** (their hint: one concise sentence on the main purpose):

```
A very fast two-pane file manager for macOS with Total Commander-compatible keyboard shortcuts, SMB, SFTP, WebDAV, and MTP+ADB/Android support. Optional+privacy-first AI features like natural language search and bulk rename.
```

### Full description

Plain text, their own line breaks, **max 3,000 characters** (the form rejects longer). Same substance as the MacUpdate
description (`macupdate.md`), minus the HTML, plus the licensing and pricing lines that MacUpdate's form bans. Measure
it after every edit; there's little room left under the cap.

```
Cmdr brings the Total Commander experience to macOS, and (optionally, only if you enable it) adds AI features that genuinely help. Built with Rust, it's extremely fast and respectful toward your CPU, RAM, and disk.

Cmdr is in open beta. It's well-tested, stable software used every day by the author and hundreds of early adopters. Feedback goes straight to the developer!

Source-available (BSL 1.1) on GitHub. Free for personal use, paid license for work.

Core features:

- Basics: Two panes, tabs, command palette, keyboard-first. F5 to copy, F6 to move, F8 to delete, all remappable.
- Operations: Copy, move, rename, delete, compress with accurate progress bars, ETAs, cancellation. Optimized for data safety, speed, and transparency.
- Advanced operations: Queue ops, background them, pause/resume, browse op log, roll back ops, even days later.
- Multi-rename: With patterns, counters, search and replace, live preview, Undo.
- Archives: Browse zip, tar, and 7z files like folders, and create or extract them.
- Speed: Lists 50,000 files near-instantly, the built-in viewer opens a 10 GB file in 1 sec, with search!
- Accessibility: Follows macOS accessibility settings. VoiceOver, WCAG 2.2 AA and APCA-verified contrasts in dark+light modes, and everything works with the keyboard.
- Localization: Translated into 15 languages.

Extra features:

- Phones: Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer. Also ADB over USB, and iPhones natively.
- SMB: Network drives over a custom SMB implementation, 4–200x faster than the macOS client. Copies within a share happen on the server. For small files and listing dirs, it's waay faster than Finder!
- More servers: SFTP, WebDAV (Nextcloud and others), and S3 (AWS, R2, B2, and more), with cost estimates for S3 transfers.
- Git: Browse Git history, branches, worktrees, and stashes like normal folders.
- Cloud: Supports "Copy Dropbox link", "Share with Google Drive", etc.
- Indexing: (Optional) Keeps a fully local index of your disk for live folder sizes and near-instant search.

AI features (optional, off by default, can stay 100% local with a built-in LLM):

- Models: The default model runs on your Mac. Or bring your own OpenAI, Claude, Gemini, etc. key, or any OpenAI-compatible endpoint (like your ollama server).
- Chat: "Why is my Downloads folder so big?", "Summarize these three docs"
- Natural-language search: "Find my tax report from last year" just works.
- Smart selection: "Select all screenshots in this folder"
- Image indexing: "Find photos where a dog looks into the camera"
- Renaming and organizing: "Rename these screenshots based on their content", "Clean up my Downloads folder". The agent never actually changes stuff: it only suggests, you approve or reject, and you can roll back later.
- Proactive suggestions: "How about moving that vet invoice to your donkey's medical papers?"
- With AI off, Cmdr is a complete Total Commander-style file manager!
```

## More info

- **Supported languages**: English, German, Spanish, French, Hungarian, Dutch, Portuguese, Russian, Swedish, Vietnamese,
  Chinese. (The 15 locales the app ships, with British and Australian English folded into English and Latin American
  Spanish into Spanish: `apps/desktop/src/lib/intl/messages`. Traditional/Simplified Chinese split is not supported in
  their list.)
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

Their optional box for the reviewing admin. For the v0.53.0 edit:

```
Hi! I've refreshed the description for v0.53.0: Cmdr now works with VoiceOver, got multi-rename and S3 support, and speaks 15 languages (Russian added to the list). Thanks!
```

The note that went with the v0.46.0 edit, kept for reference (it's where the licensing mislabel ask went in):

```
Hi! Cmdr got a bunch of new stuff since my last edit (2026-08-13), so I've:
- refreshed the desc (SFTP, WebDAV, and ADB support, cloud provider right-click menus, AI file organization),
- added Traditional Chinese to the supported languages,
- added four tags: sftp, webdav, adb, android,
- and corrected the price: it's one $59 purchase now, not a range. I retired the old $59/year subscription and the $199 perpetual, so "between $59 and $199" no longer describes anything I sell.

Also, my ask from last time (if it's not done already): Cmdr is source-available under BSL 1.1, not OSI open source. I keep the "Is Opensource?" box ticked because unticking it hides the Source URL field, and the code really _is_ public, but the page summary at https://alternativeto.net/software/cmdr/ calls Cmdr "open-source", which isn't strictly true in the OSI sense. If you can change that wording to "source available", I'd appreciate it. Otherwise never mind, most people don't know the diff anyway. :)
```

## Still pending after this pass

- **The page still labels Cmdr "Open Source"** under Licensing (checked 2026-09-24), after the BSL wording request went
  in with both the v0.45.0 and v0.46.0 edits. Admins approved everything else in those edits and ignored this, so a
  third ask through the same note field is unlikely to land. If it's worth pursuing, their contact form is the next
  step.

## Settled calls

So they don't get re-litigated at the next edit pass:

- **The full description sits at 2,992 of their 3,000-character cap** (measured 2026-10-22). Every addition has to be
  paid for by a cut, so measure before and after; the form rejects an over-long one outright. Nothing in v0.46.0 made
  the description false (no feature graduated out of alpha, no claim broke, and the description names no prices), so
  that release changed nothing here, per `docs/guides/releasing.md` § "Refreshing the app-directory listings".

- **Keep "Is Opensource?" checked.** Unchecking it hides the Source URL field, and a public link to the source is worth
  more than a precise badge. The description carries the BSL correction instead, in its third paragraph rather than as
  fine print, and the note above asks the admins to fix the page summary they generate.
- **Keep the `portable` tag and the Portable feature.** True in the only sense the tag can mean on macOS: a `.app` runs
  from wherever you put it, a USB stick included. Settings and the index stay in `~/Library` and Full Disk Access is per
  machine, but that's every Mac app, and nobody reads the tag as a promise about state.
