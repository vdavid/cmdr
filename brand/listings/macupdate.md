# MacUpdate listing

Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form creates
and modifies a listing (search the app name at the top to modify).

Status: submitted 2026-07-29 for v0.36.2. The fields below are refreshed for v0.38.0 and ready to paste; that refresh is
not submitted yet. Edit them here first when refreshing, then paste. Because the 0.37.0 refresh was prepared but never
submitted, "Version changes" below covers both 0.37.0 and 0.38.0.

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
    `https://github.com/vdavid/cmdr/releases/download/v0.38.0/Cmdr_0.38.0_universal.dmg` (version-pinned, so it would
    need bumping per release).
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.38.0`
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
  <li>Auto-organization is on the way.</li>
  <li>
    The model runs on your Mac by default, so your files and data stay 100% private. You can choose to bring your own
    OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint to use more powerful models.
  </li>
</ul>
<p>Cmdr is source-available under the Business Source License 1.1: https://github.com/vdavid/cmdr</p>
```

### Version changes

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers 0.37.0
and 0.38.0, since the 0.37.0 refresh was never submitted; the 0.36 line went in with the previous submission.

```html
<h5>New</h5>
<ul>
  <li>
    Search reaches folders Cmdr hasn't indexed yet: it walks them live and streams matches as it finds them, and offers
    the permission for a folder macOS refused.
  </li>
  <li>Scope a search to the current folder (the new default) or the whole volume, with ⌥C and ⌥V.</li>
  <li>
    A corner chip in the main window for a backgrounded operation, showing its progress and one click from the queue.
  </li>
  <li>
    The Operation queue, opened with ⌥⌘Q, renamed from "Transfer queue" since it holds deletes, renames, and archive
    edits too.
  </li>
  <li>A failed background operation keeps its reason until you dismiss it, in the queue and as a toast.</li>
  <li>Cmdr asks before quitting with a transfer in flight, and clears away whatever it left half-written.</li>
  <li>Resize dialogs from any edge, and hover any shortened path or label to see the whole thing.</li>
  <li>Copy the current folder's path with ⌃⌘C on the ".." row.</li>
  <li>An Acknowledgements dialog crediting all 775 open-source packages Cmdr ships.</li>
  <li>Right-click any text field for Cut, Copy, Paste, and Select all.</li>
  <li>Undo for an AI bulk rename, one batch at a time or a whole multi-batch run at once.</li>
  <li>
    Every rename review row now shows the file itself and the evidence behind its proposed name, and you can correct a
    name in place before approving it.
  </li>
  <li>Recent searches live in the query field as a dropdown, each row showing its age, result count, and filters.</li>
  <li>Transfer speed in the Transfers window, plus an honest readout when a transfer has stopped moving.</li>
  <li>
    A "Chat memory size" setting for the AI chat, from Automatic up to 200,000 tokens, with a bar showing how full the
    conversation is.
  </li>
</ul>
<h5>Improved</h5>
<ul>
  <li>
    A new terms of service: a third shorter, accurate about what Cmdr actually does, and each release now converts to
    open source three years after it ships rather than every version on one shared date.
  </li>
  <li>
    An idle Mac stays idle: a dotfile write in your home folder no longer rescores the whole drive (5.25 s per pass
    becomes 2.1 ms), 51,081 folder-ranking rows a minute stop being rewritten for an unchanged result, and roughly 9,400
    log lines an hour are gone.
  </li>
  <li>About 97 MB less peak memory during a search, and half the write cost of keeping the index live.</li>
  <li>
    The Transfers window shows the same honest readout as the copy dialog: both bars labelled, percentages, speed, and a
    time left that doesn't shift the layout.
  </li>
  <li>What's new opens on the headlines, with each release's details behind a Show more.</li>
  <li>The copy conflict question is now a card you can read at a glance.</li>
  <li>One line-height scale across the whole app, so text spacing stops varying screen by screen.</li>
  <li>
    A broad search returns in under half a second instead of twelve, and the dialog opens and takes typing without
    waiting on an index rebuild.
  </li>
  <li>
    The Search and Select dialog is redesigned to match the rest of the app: standard window chrome, a tidy 2x2 query
    block, and a Path column with its width back.
  </li>
  <li>
    Copying to a network drive is up to 3.8 times faster, and a small file now costs one round trip instead of two.
  </li>
  <li>
    A network drive that goes silent recovers in about 50 seconds instead of hanging forever, and a single file retries
    after a blip rather than ending the whole transfer.
  </li>
  <li>Every text field shares one look: 8px corners, an accent-colored caret, and a solid focus ring.</li>
  <li>The main window appears a second sooner at startup.</li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>
    A folder move to a phone or a NAS could destroy the files you chose to keep or skip, and a folder copy that failed
    could wipe the destination folder.
  </li>
  <li>Copying a local folder to a NAS or a phone could fail outright.</li>
  <li>
    A force-quit or a crash mid-copy could leave a truncated file wearing your real filename, on every kind of drive.
  </li>
  <li>Two disks could be handed one identity, which could route reads and file operations to the wrong drive.</li>
  <li>⌘- and ⌘+ could freeze the app for up to 46 seconds while fonts were measured.</li>
  <li>Copying to a NAS died on a filename containing a "?" or a quote.</li>
  <li>Quick Look and dragging files out did nothing on a direct-SMB pane.</li>
  <li>A burst of changes on a phone pegged a CPU core and froze the pane.</li>
  <li>A file with a newline in its name was unfindable, in search, selection, and excludes.</li>
  <li>A pane could strand itself on "Path not found" after its network drive disappeared.</li>
  <li>A saved cloud AI key could come back out of the OS secret store.</li>
  <li>⌘V pasted twice in dialogs, and ⌥⌘A opened the AI chat and selected every file at once.</li>
  <li>Resizing the window could freeze the entire interface.</li>
  <li>Folders on network drives, inside archives, and on phones showed no size or date.</li>
  <li>The AI could invent names for screenshots whose contents it was never actually shown.</li>
  <li>A maximized window came back in the wrong place after a restart.</li>
</ul>
```

### System requirements

```
macOS 12 Monterey or later, both Apple Silicon and Intel
```

## 3. Media

- **Icon**: `brand/logos/cmdr-512.png` (512x512, transparent)
- **Screenshots** (up to five, in this order):
  1. `brand/screenshots/app-main-light.webp`
  2. `brand/screenshots/app-main-dark.webp`
  3. `brand/screenshots/search-light.webp`, `chat-light.webp`, `settings-light.webp` if the listing takes more.
  - Five slots for eight masters, so the dark twins of search, chat, and Settings sit out: the main-view pair already
    shows both themes. The submitted listing still carries only that pair, from before the rest existed.
  - The masters are lossless WebP. MacUpdate's uploader may want PNG: `magick app-main-light.webp app-main-light.png`.
    Reshoot per `docs/guides/screenshots.md`.

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
