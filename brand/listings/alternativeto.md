# AlternativeTo listing

Live: https://alternativeto.net/software/cmdr/about/. Edit it while signed in; changes go through an admin approval
queue, and the "Note about your changes" field at the bottom speeds that up.

Status: live. The fields below are what the listing holds today (captured 2026-07-29), followed by the parts that have
gone stale. Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory
listings".

## Main info

- **Name**: `Cmdr`
- **Website**: `https://getcmdr.com`
- **Short description** (their hint: one concise sentence on the main purpose):

```
Extremely fast, keyboard-focused two-pane file manager for macOS with live folder sizes, high-speed SMB/MTP transfers, optional on-device privacy-first AI features, and built-in git folder browsing.
```

- **Supported languages**: English
- **Pricing**: `Free for personal use`. Model `Purchase`, min `$59`, max `$199`
- **Is Opensource?**: checked. License `Other`. Source URL `https://github.com/vdavid/cmdr`

### Full description

```
Cmdr is an extremely fast, keyboard-driven two-pane file manager for macOS, written in Rust.
Built for people who miss Total Commander since switching to a Mac.

Cmdr is in open beta! It means there might be sharp edges, but for its core, it's already stable software used every day by a small group of enthusiastic testers. You're welcome to be one of them! Feedback goes straight to the developer.

Cmdr is free forever for personal use with its source code available, but a commercial license is needed for work use.

Basic features:

- Full, keyboard-first list/copy/move/rename/delete capabilities
- All the familiar shortcuts (F5, F6, F8, etc.), but fully customizable
- Accurate progress bars, ETAs, cancellation, and even rollback
- Honest and transparent UI and UX, with even the full source code available (at https://github.com/vdavid/cmdr), no bs, no opaque error messages.
- Optimized for speed: lists 50k files near-instantly, the built-in viewer opens 10+ GB files also near-instantly with fast search.

Distinguishing features:

- Shows live sizes for all your folders (!)
- Network drives (SMB) use custom code for ~4x faster access than macOS.
- Can read/write Android phones, Kindles, cameras over MTP/PTP, at up to 4x speed compared to Android File Transfer and the like.
- Browse git history, branches, worktrees, stash like normal folders

AI:

- AI is controversial these days, so it is FULLY OPTIONAL to enable it in the product. Without it, you still have a really nice Total Commander-like experience.
- With AI enabled, you also have natural-language search ("that presentation about turtles from last week"), smart selection. Smart renaming and auto-organization are on the way.
- AI is on-device, privacy-first, so your files and data never leave your Mac. But if you want a smarter model, you can bring your own OpenAI/Claude/Gemini/etc. key to use any cloud model, or plug in any OpenAI-compatible API and bring your own LLM.
- Cmdr does not charge for AI features. You either use the built-in model for free, or pay your own API bills (Cmdr uses well under $1/month for normal use in occasional searches, etc.)
```

## Tags

`total-commander`, `ai`, `finder-alternative`, `built-in-file-manager`, `macos`, `offline-access`, `svelte`, `finder`,
`dual-pane`, `ad-free`, `no-registration`, `lightweight`, `rust`, `privacy-focused`, `built-in-viewer`, `tauri`,
`two-pane`, `portable`, `file-management`, `file-manager`, `live-preview`, `rust-based`, `rust-lang`, `night-mode`

## Application type

Checked: **File Manager**. Available and unchecked: File Archiver, File Compressor, File Search Utility, File Sync Tool,
FTP Client. (They ask for two to three types per app.)

## Features

Checked: No registration required, Portable, Privacy focused, Lightweight, Ad-free, Works offline, Dark mode, Built-in
viewer, Live preview.

Unchecked: Batch rename files, File tagging, No tracking, Windows Explorer extension, Support for themes, Extensible by
plugins/extensions, Supports zip files, Full-text search, Support for scripting.

## Platforms

`Mac`. No Mac App Store link, no platform note. (Linux is alpha and deliberately not listed.)

## Author and social

- **Company / author**: `vdavid`, country of origin `Sweden`, website `https://getcmdr.com`
- **X username**: `vdavid`. No Facebook URL.

## Icon and screenshots

- **Icon**: the current app icon.
- **Screenshots**, in order: "Cmdr v0.24 (light)", "Cmdr v0.24 (dark)", "Settings (light)". Limit is 3 MB each; JPEG,
  PNG, or WebP. YouTube videos can be added by URL.

## Pending updates

Concrete replacement values, ready to paste on the next edit pass.

- **Supported languages**: add German, Spanish, French, Hungarian, Dutch, Portuguese, Swedish, Vietnamese, and Chinese.
  The app ships all 10 (`apps/desktop/src/lib/intl/messages`).
- **Screenshots**: replace all three. The v0.24 pair predates the 0.36 facelift (rounder dialogs, capsule buttons, inset
  panes), so the listing shows an app that no longer exists. Upload `brand/screenshots/app-main-light.webp`,
  `app-main-dark.webp`, and `settings-light.webp` (all current). Caption them without a version number ("Two-pane main
  view (light)"), so they don't advertise their own age. If the uploader rejects WebP: `magick x.webp x.png`.
- **Full description**: "Smart renaming and auto-organization are on the way" is half stale, natural-language bulk
  rename shipped in 0.35. Also missing, all shipped and all headline-worthy: the full-disk index that makes search
  instant, browsing zip/tar/7z archives as folders (zip is writable), and photo search by content.
- **Features**: check **Batch rename files** (natural-language bulk rename), **File tagging** (`file_system/tags.rs`,
  Finder tags), and **Supports zip files** (browse, extract, and write). Leave **Full-text search** unchecked, the index
  covers names and metadata, not document contents. Leave **No tracking** unchecked, Cmdr sends anonymous analytics.
- **Application type**: add **File Search Utility**, and **File Archiver** if you want the third slot used.
- **Pricing model**: `Purchase` with a $59–$199 range renders on the page as "One time purchase (perpetual license)
  ranging between $59 and $199", which misstates the $59/year subscription. This needs revisiting when the new pricing
  ships anyway.

Two settled calls, so they don't get re-litigated at the next edit pass:

- **Keep "Is Opensource?" checked.** Unchecking it hides the Source URL field, and a public link to the source is worth
  more than a precise badge. BSL 1.1 is source-available, not OSI open source, so the description carries the correction
  instead, early rather than as fine print. Draft replacement for the third paragraph, for David to review when pasting:

  > Cmdr is source-available under the Business Source License 1.1: the full source is on GitHub, though BSL isn't an
  > OSI-approved open-source license (AlternativeTo's "open source" label here is auto-generated). Free forever for
  > personal use, commercial license needed for work use.

  Say the same in one sentence in the "Note about your changes" box: it's the only channel to the admins who control the
  page summary, which currently opens with "Ultra-fast, open-source two-pane file manager" and isn't editable.

- **Keep the `portable` tag and the Portable feature.** True in the only sense the tag can mean on macOS: a `.app` runs
  from wherever you put it, a USB stick included. Settings and the index stay in `~/Library` and Full Disk Access is per
  machine, but that's every Mac app, and nobody reads the tag as a promise about state.
