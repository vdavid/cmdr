# MacUpdate listing

Live: https://cmdr.macupdate.com/ (accepted from the 2026-09-18 v0.46.0 submission, the first of three that went
through). Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form
creates and modifies a listing: type `Cmdr` into "Modify an existing listing?" at the top to load this one.

Status: **v0.49.0 prepared 2026-10-01, not yet submitted** (the fields below are the update to paste). The live page
still shows v0.46.0; the v0.47.0 update prepared 2026-09-24 never went in, and this one supersedes it.

❗ **The Download URL must be a DIRECT link to the installer**, not a redirect. Their guidelines say "the direct URL to
the installer package (e.g., .pkg .dmg, .zip)", and `getcmdr.com/download/latest/universal` takes 3 redirects to a
signed `release-assets.githubusercontent.com` blob URL that doesn't end in `.dmg` (verified 2026-09-18). The two
submissions that used the redirect were never accepted and the one with the version-pinned GitHub Releases URL was, so
keep the pinned URL, at the cost of the `?ref=macupdate.com` attribution and a bump per release.

"Version changes" covers the release being submitted (the whole minor line, patches included, once patches start getting
skipped here); the Description carries everything older.

Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory listings".

The Description and Version changes fields take **HTML**, not plain text: `<p>`, `<strong>`, `<h5>`, `<ul>`, `<li>`.
Their own hint says to keep pricing and promotional text out of the description, so the license and price story lives in
the Price field and the note to the review team instead.

## 1. App and developer info

- **App name**: `Cmdr`
- **Developer name**: `David Veszelovszki`
- **Download URL**: `https://github.com/vdavid/cmdr/releases/download/v0.49.0/Cmdr_0.49.0_universal.dmg`
  - Version-pinned, so bump it with every listing update (see the Download URL note at the top). The redirecting
    `https://getcmdr.com/download/latest/universal?ref=macupdate.com` would never go stale and would attribute downloads
    to MacUpdate, but it's what the rejected submissions used.
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.49.0`
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
  Cmdr is in open beta. There might be sharp edges in the newer features (archives, the operation log, and AI), but the
  core is well-tested, stable software used every day by the author and a group of testers. Feedback goes straight to
  the developer!
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
    Queue multiple file operations, send any of them to the background, pause/resume file transfers, view a full,
    searchable log of past operations, with rollback for anything that didn't permanently delete data.
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
    <em>all</em> your folders, and for near-instant full-drive search. A folder that isn't indexed yet gets walked live,
    with matches arriving as they're found.
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
  <li>
    File organization: "Clean up my Downloads folder" and the agent proposes each move for you to approve or reject.
  </li>
  <li>
    It can also watch your folders and raise things on its own: "How about moving that vet invoice to your medical
    papers?" It only ever suggests, you decide, and it stops on one click.
  </li>
  <li>
    The model runs on your Mac by default, so your files and data stay 100% private. You can choose to bring your own
    OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint to use more powerful models.
  </li>
</ul>
<p>Cmdr is source-available under the Business Source License 1.1: https://github.com/vdavid/cmdr</p>
```

### Version changes

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers v0.48.0
and v0.49.0 (neither line had patches); everything older is the Description's job.

```html
<h5>New</h5>
<ul>
  <li>Drag a tab to reorder it, or drop it on the other pane's tab bar to move it there.</li>
  <li>
    The viewer has Text, Binary, Hex, and Media modes, one keypress apart (0–3). Contributed by Gábor Gyebnár. Thank
    you!
  </li>
  <li>
    The Servers view remembers SMB shares, lets you name servers and switch accounts, and works fully from the keyboard.
  </li>
  <li>SFTP servers show their free space, and a copy onto one checks it first.</li>
  <li>Reveal any search result in its folder, from the keyboard or the right-click menu.</li>
  <li>Latin American Spanish.</li>
  <li>
    When macOS blocks Cmdr from reaching a server, Cmdr points you to the Local Network permission, with a button to
    open it.
  </li>
  <li>A "Check the key" button for an SFTP server whose host key changed.</li>
  <li>The AI chat can look inside files on connected phones, servers, and direct network shares.</li>
</ul>
<h5>Improved</h5>
<ul>
  <li>
    Compression, rebuilt: zips stream straight to local disks, network shares, SFTP servers, and phones, with
    step-by-step progress and a Cancel that stops promptly.
  </li>
  <li>Much faster broad searches on large drives.</li>
  <li>Lower memory use, settling around 240 MB after heavy searches instead of 400 MB, and less CPU while idle.</li>
  <li>Right-click menus open promptly on slow network shares, and busy NAS drives stay responsive.</li>
  <li>Cmdr no longer signs in to every SMB machine on the network at launch.</li>
  <li>A server that wants an account asks for one calmly, with no red message before you've typed anything.</li>
  <li>A copy, move, or new folder blocked by a same-named file now names the file in the way.</li>
  <li>Menus, toasts, and tooltips look closer to native macOS.</li>
  <li>Every translation reads more naturally, and elapsed times and ETAs speak your language.</li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>Copying or moving a folder onto a folder link could write into the link's target, replacing files elsewhere.</li>
  <li>A move between drives could delete originals, or files that changed while the move was running.</li>
  <li>A canceled or unsuccessful replacing copy or move could lose the original.</li>
  <li>Rollback on phones and servers skipped every item.</li>
  <li>Zips Cmdr wrote showed times off by your time zone in other apps.</li>
  <li>
    Indexing and search crawled pCloud, rclone, sshfs, and NFS mounts over the network as if they were local disks.
  </li>
  <li>Picking a slow cloud drive like pCloud dropped you in your home folder.</li>
  <li>Thunderbolt SSDs and other fixed external disks had no eject button.</li>
  <li>Forgotten drives left tens of MB of index data behind.</li>
  <li>Folders you granted access to in System Settings stayed grayed out until a restart.</li>
  <li>Settings text fields lost your edits when you closed the window.</li>
  <li>Rebuilt or cleared search indexes kept showing old results.</li>
  <li>Quick Look opened slowly or ignored Escape.</li>
  <li>A flicker when entering a folder. Contributed by Gábor Gyebnár.</li>
</ul>
<h5>Security</h5>
<ul>
  <li>Error and crash reports leave out file names, paths, server, share, and account names, and search words.</li>
  <li>Cmdr's MCP server requires its token for every request.</li>
  <li>Cmdr ignores files a server or phone lists outside the folder you connected to.</li>
</ul>
```

### System requirements

```
macOS 12 Monterey or later, both Apple Silicon and Intel. macOS 10.15 Catalina and 11 Big Sur run on a best-effort basis.
```

## 3. Media

- **Icon**: `brand/logos/cmdr-512.png` (512x512, transparent)
- **Screenshots** (up to five, in this order):
  1. `brand/screenshots/app-main-light.webp`
  2. `brand/screenshots/app-main-dark.webp`
  3. `brand/screenshots/search-light.webp`, `chat-light.webp`, `settings-light.webp` if the listing takes more.
  - Five slots for eight masters, so the dark twins of search, chat, and Settings sit out: the main-view pair already
    shows both themes. The submitted listing still carries only that pair, from before the rest existed.
  - The masters are lossless WebP. MacUpdate's uploader want PNG, so use:
    `magick app-main-light.webp app-main-light.png`. Reshoot per `docs/guides/screenshots.md`.

## Comments for the review team

For an update, keep it short:

```
Hi folks! Thanks for listing Cmdr! This is the v0.49.0 update: new version number, download link, and version changes. The description is unchanged.

David
```

The note that went with the first (accepted) submission, kept for reference:

```
Hi folks!

(Sorry, but it's 2026, so I must note that this is 100% human-written content here. ↓ )

Cmdr is a two-pane file manager for macOS, written in Rust, in open beta. I'd love to get some early users to test-run it. It's a free app for individuals. Thanks for your work at MacUpdate!

I've submitted Cmdr twice before (most recently on 2026-07-29 I think) and didn't hear back either time, and I can't find a listing for Cmdr. If something in those submissions was missing, unclear, or disqualifying, I'd appreciate a line about what to fix. Happy to correct anything or provide whatever else you need.

My notes:
- The download URL points straight at the signed DMG on GitHub Releases. The app is dev ID signed and notarized by Rymdskottkärra AB (my Swedish company), and ships as a universal binary.
- I left the price empty because, as I said above, Cmdr is free for personal use. Work use needs a license ($59 once, 1 year of updates), see https://getcmdr.com/pricing.
- Source is available under BSL 1.1 at https://github.com/vdavid/cmdr.
- Anything you need from me, write to me at hello@getcmdr.com and I'll answer usually the same day.

Thanks for the review!
David
```
