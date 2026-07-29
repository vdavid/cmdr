# MacUpdate listing

Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form creates
and modifies a listing (search the app name at the top to modify).

Status: draft, not submitted yet. Prepared for v0.36.2 (2026-07-28).

Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory listings".

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
A blazing-fast, keyboard-driven two-pane file manager for macOS, with fully optional, privacy-first AI built in.
```

### Description

```html
<p>
  <strong>Cmdr</strong> brings the Total Commander experience to macOS, and (optionally, only if you enable it) adds AI
  features that genuinely help. Built with Rust, it's extremely fast and respectful toward your CPU, RAM, and disk.
</p>
<p>
  Cmdr is in open beta. There might be sharp edges in the newer features (search, archives, the operation log, and AI),
  but the core is well-tested, stable software used every day by the author and a group of testers. Feedback goes
  straight to the developer!
</p>
<h5>Core features</h5>
<ul>
  <li>
    Two panes, tabs, command palette, keyboard-first. Common shortcuts like F5 to copy, F6 to move, F8 to delete all
    work, and are remappable.
  </li>
  <li>
    Browse, copy, move, rename, delete, compress/decompress with accurate progress bars, honest ETAs, cancellation.
    Optimized for data safety, speed, and transparency.
  </li>
  <li>
    Queue multiple file operations, pause/resume file transfers, view a full, searchable log of past operations, with
    rollback for anything that didn't permanently delete data.
  </li>
  <li>
    Very fast: Lists 50,000 files near-instantly, and the built-in viewer opens a 10 GB file near-instantly with fast
    search.
  </li>
  <li>
    Real dark and light modes, native macOS behavior, and all text color / background contrasts verified against WCAG
    2.2 AA and APCA.
  </li>
</ul>
<h5>Extra features</h5>
<ul>
  <li>
    Full access to Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than Android File Transfer, no
    hacks needed, works out of the box with any USB cable.
  </li>
  <li>Full access to network drives over a custom SMB implementation, roughly 4x faster than the macOS client.</li>
  <li>
    Keeps a full index of your disk (fully local and private) and uses it to display live folder sizes for
    <em>all</em> your folders, and for near-instant full-drive search.
  </li>
  <li>For Git repositories, it shows a Git history, branches, worktrees, and stashes browsable like normal folders.</li>
</ul>
<h5>AI features (entirely optional, can be fully local and private with a built-in LLM)</h5>
<ul>
  <li>
    With AI features switched off, Cmdr is a complete Total Commander-style file manager. Many people don't like AI
    features, so they are off by default.
  </li>
  <li>Switched on, it adds natural-language search: "Find my tax report from last year"</li>
  <li>Smart selection: "Select all screenshots in this folder"</li>
  <li>Chat: "Why is my Downloads folder so big?"</li>
  <li>
    Image indexing (fully local and private!): "Find me all photos in this folder where a dog looks into the camera."
  </li>
  <li>
    Natural-language renaming: "Rename all these screenshots based on their content." → The agent can only
    <em>suggest</em> write operations like renames, you are in charge of reviewing and applying them. If you change your
    mind, you can always roll back any past operations.
  </li>
  <li>Auto-organization is on the way.</li>
  <li>
    The model runs on your Mac by default, so your files and data stay 100% private. You can choose to bring your own
    OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint to use more powerful models.
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
Hi folks! Cmdr is a two-pane file manager for macOS, written in Rust, in open beta.

- The download URL redirects to the signed DMG on GitHub Releases. The app is Developer ID signed and notarized by Rymdskottkärra AB (my Swedish company), and ships as a universal binary.
- I left the price empty because Cmdr is free for personal use, with no trial timer, no nags. Work use needs a paid license ($59/year, or $199 one-time), sold at https://getcmdr.com/pricing.
- The source is available under BSL 1.1 at https://github.com/vdavid/cmdr.
- Anything you need from me, write to me at hello@getcmdr.com and I'll answer the same day.

Thanks for the review!
David
```
