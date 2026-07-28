# MacUpdate listing

Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form creates
and modifies a listing (search the app name at the top to modify).

Status: draft, not submitted yet. Prepared for v0.36.2 (2026-07-28).

The Description and Version changes fields take **HTML**, not plain text: `<p>`, `<strong>`, `<h5>`, `<ul>`, `<li>`.
Their own hint says to keep pricing and promotional text out of the description, so the license and price story lives in
the Price field and the note to the review team instead.

## 1. App and developer info

- **App name**: `Cmdr`
- **Developer name**: `David Veszelovszki`
- **Download URL**: `https://getcmdr.com/download/latest/universal?ref=macupdate.com`
  - Always points at the current release, so it never needs a resubmission, and it attributes the download to MacUpdate
    in the dashboard. Plain fallback if they reject redirects:
    `https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_universal.dmg` (version-pinned, so it would
    need bumping per release).
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.36.2`
- **Price**: leave empty (their hint says empty means free). Cmdr is free for personal use; commercial licenses are sold
  on the purchase URL and explained to the review team below.

## 2. Details and description

### Short description

Their hint: a brief, compelling overview of the key value proposition, without the app name.

```
A blazing-fast, keyboard-driven two-pane file manager for macOS, with fully optional, privacy-first AI.
```

### Description

```html
<p>
  <strong>Cmdr</strong> is a keyboard-driven two-pane file manager for macOS, written in Rust. It's built for people who
  miss Total Commander since switching to a Mac: two folders side by side, familiar function keys (F5 to copy, F6 to
  move, F8 to delete), all remappable.
</p>
<p>
  Cmdr is in open beta. There might be sharp edges, but the core is stable software used every day by a small group of
  testers, and feedback goes straight to the developer.
</p>
<h5>Features</h5>
<ul>
  <li>Two panes with tabs and a command palette, so navigation never needs the mouse.</li>
  <li>Copy, move, rename, and delete with accurate progress bars, honest ETAs, cancellation, and rollback.</li>
  <li>
    Lists 50,000 files near-instantly, and the built-in viewer opens a 10 GB file near-instantly with fast search.
  </li>
  <li>Live folder sizes everywhere, not only for the file under the cursor.</li>
  <li>A full-disk index built in minutes that stays current across restarts, so search answers immediately.</li>
  <li>Network drives over SMB through custom code, roughly four times faster than the macOS client.</li>
  <li>Android phones, Kindles, and cameras over MTP and PTP, up to four times faster than Android File Transfer.</li>
  <li>Git history, branches, worktrees, and stashes browsable like normal folders.</li>
  <li>Real dark and light modes, native macOS behavior, and text colors verified against WCAG 2.2 AA and APCA.</li>
</ul>
<h5>AI, entirely optional</h5>
<ul>
  <li>Switched off, Cmdr is a complete Total Commander-style file manager.</li>
  <li>
    Switched on, it adds natural-language search ("that presentation about turtles from last week") and smart selection.
    Natural-language renaming and auto-organization are on the way.
  </li>
  <li>
    The model runs on your Mac by default, so files and data stay on it. You can bring your own OpenAI, Claude, or
    Gemini key instead, or point Cmdr at any OpenAI-compatible endpoint.
  </li>
</ul>
<p>Cmdr is source-available under the Business Source License 1.1: https://github.com/vdavid/cmdr</p>
```

### Version changes

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers the 0.36
line, since 0.36.0 carries the interesting work and 0.36.1 and 0.36.2 are patches on top.

```html
<h5>New</h5>
<ul>
  <li>Photo search stays fast past 50,000 images with an on-device index that switches on automatically at scale.</li>
  <li>Image-index status shows right on the icons: a per-file badge, per-folder coverage, and a per-drive dot.</li>
  <li>
    Three focused settings cards manage image indexing: switch semantic search on or off, set how many parallel workers
    index, and reclaim the model's disk space.
  </li>
</ul>
<h5>Improved</h5>
<ul>
  <li>
    A design facelift toward a native macOS feel: rounder dialogs, capsule buttons, inset file panes, redesigned Copy,
    Move, and Delete dialogs, and settings controls matched to their job.
  </li>
  <li>
    Checking a drive for changes is five times faster, and indexing a NAS is about 3.8 times faster by spreading the
    scan across multiple SMB connections.
  </li>
  <li>
    File transfers and browsing now take priority over background indexing, and uploads pause while you browse the same
    share.
  </li>
  <li>
    Much lighter on memory: search's folder ranking is down 85% on a NAS-sized drive, background folder scoring by two
    thirds, and the image-search model reclaims about 550 MB after it installs.
  </li>
  <li>Settings is reorganized into Indexing and Notifications, with a leaner AI, Behavior, and Advanced.</li>
  <li>Downloads show one notification per burst, always naming the newest file.</li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>Cmdr could balloon to tens of gigabytes of memory shortly after launch.</li>
  <li>
    The app could crash when you closed Settings or the file viewer, and could show a blank window on cold launch.
  </li>
  <li>Clicking or dragging inside the rename field cancelled the rename; clicking away now saves, like Finder.</li>
  <li>
    NAS image indexing could stall at zero images, and NAS snapshot folders inflated a share's file count and size.
  </li>
  <li>Every switch and checkbox now has a real name and role for screen readers.</li>
</ul>
```

### System requirements

```
macOS 12 Monterey or later, both Apple Silicon and Intel
```

## 3. Media

- **Icon**: `brand/logos/cmdr-512.png` (512x512, transparent)
- **Screenshots** (up to five, in this order):
  1. `brand/screenshots/app-main-light.png`
  2. `brand/screenshots/app-main-dark.png`
  - Only the main-view pair exists today. Search shots come once the search UI is presentable; settings and the file
    viewer are the other candidates. Reshoot per `docs/guides/screenshots.md`.

## Comments for the review team

```
Hi folks! Cmdr is a two-pane file manager for macOS, written in Rust, in open beta since 2026.

- The download URL redirects to the signed DMG on GitHub Releases. The app is Developer ID signed and notarized by Rymdskottkärra AB (my Swedish company), and ships as a universal binary.
- I left the price empty because Cmdr is free for personal use, with no trial timer and no nags. Work use needs a paid license ($59/year, or $199 one-time), sold at https://getcmdr.com/pricing.
- The source is available under BSL 1.1 at https://github.com/vdavid/cmdr.
- Anything you need from me, write to hello@getcmdr.com and I'll answer the same day.

Thanks for the review!
David
```
