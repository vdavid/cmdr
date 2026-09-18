# MacUpdate listing

Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form creates
and modifies a listing (search the app name at the top to modify).

Status: **submitted 2026-09-18 for v0.46.0** (the fields below are what went in). Their confirmation said they review
and reply by email within 10 days, so silence past 2026-09-28 is itself a data point.

Two earlier submissions (2026-07-29 for v0.36.2, and one before it) were **never accepted** and drew no reply, so
MacUpdate has published no listing and every attempt is a fresh CREATE. ❌ Don't use the form's "Modify an existing
listing?" search; it will find nothing until they accept one.

❗ **The Download URL must be a DIRECT link to the installer**, not a redirect. Their guidelines say "the direct URL to
the installer package (e.g., .pkg .dmg, .zip)", and `getcmdr.com/download/latest/universal` takes 3 redirects to a
signed `release-assets.githubusercontent.com` blob URL that doesn't end in `.dmg` (verified 2026-09-18). That is the
best guess for why the first two submissions went nowhere, so this one used the version-pinned GitHub Releases URL
instead, at the cost of the `?ref=macupdate.com` attribution and a bump per release.

Because no version of this listing was ever published, "Version changes" covers v0.46.0 alone rather than accumulating
every unsubmitted release: for a listing with no history, it's the changes in the version being submitted, and a reader
meeting Cmdr for the first time gets the Description for everything else.

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
    `https://github.com/vdavid/cmdr/releases/download/v0.46.0/Cmdr_0.46.0_universal.dmg` (version-pinned, so it would
    need bumping per release).
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.46.0`
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

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. No listing was ever
published, so this covers v0.46.0 alone; everything older is the Description's job.

```html
<h5>New</h5>
<ul>
  <li>
    Press Ctrl-D for your favorites, then a number key to jump straight to one. Drag to reorder them, press 0 to add the
    folder you're standing in.
  </li>
  <li>
    Option-Shift-= selects the rest of the files like the one you're on: every PDF, every folder, every file with no
    extension. The menu item says what it would pick before you press it.
  </li>
  <li>Finder's tag colors as one row of circles in the right-click menu, instead of seven stacked items.</li>
  <li>Ctrl-Return opens the right-click menu on the row you're on, so 40-odd file actions stop needing a mouse.</li>
  <li>A refused eject now names the app holding the drive, so there's something to go and close.</li>
  <li>A search counts its hits in the footer, and results open in a tab of their own.</li>
  <li>
    A badge in the title bar when Full Disk Access is missing, and a refused trash that explains itself instead of
    saying "try again".
  </li>
  <li>
    Unplug a drive mid-index and Cmdr says why its folder sizes get recomputed, rather than rescanning in silence.
  </li>
</ul>
<h5>Improved</h5>
<ul>
  <li>
    Ejecting takes the drive's whole physical disk down, or says why it couldn't, instead of reporting success over a
    partition that's still mounted.
  </li>
  <li>
    Every unmount waits for Cmdr's index to let go first, whoever started it: Finder, Disk Utility, or another app.
  </li>
  <li>A drive pulled from its port stops its index instead of leaving one reading a filesystem that isn't there.</li>
  <li>A cloud drive mounted in your home folder, like pCloud's, gets its own row in the drive list beside Dropbox.</li>
  <li>Cmdr is now $59 bought once, with a year of updates. The yearly subscription is retired.</li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>
    A drive pulled mid-transfer kept every original, and the message names the drive and how far it got. A move to a USB
    stick no longer deletes the sources before the files are really on it.
  </li>
  <li>A crash or force-quit mid-overwrite could lose the file being replaced.</li>
  <li>
    A drive that left mid-scan could blank its own index, and an eject could unmount a drive Cmdr was still reading.
  </li>
  <li>Renaming anything on a mounted network share failed every time.</li>
  <li>Copying and moving did nothing on an SFTP, WebDAV, or Android server rooted at "/".</li>
  <li>
    A company share published as a DFS namespace root crawled on the slow macOS mount instead of connecting directly.
  </li>
  <li>F8 in a Dropbox or Google Drive folder failed with a message about the wrong disk.</li>
  <li>A batch trash that left items behind reported a clean success.</li>
  <li>A drive mounted outside /Volumes dead-ended on "Volume not found".</li>
  <li>Photos in a folder you excluded still showed up in photo search, find similar, and the AI's photo tools.</li>
  <li>A search handed back at most 1,000 rows, and "Show all in main window" opened with only 30.</li>
  <li>
    The right-click menu showed the default keys after you rebound a command, and Option shortcuts showed no key at all.
  </li>
  <li>The viewer's Edit menu acted on the status bar instead of the file.</li>
  <li>
    "Add to favorites" could write a favorite nobody could ever see, on phones, archives, and protocol-only servers.
  </li>
  <li>Activating a license left "Personal use only" in the Dock until a relaunch.</li>
  <li>Onboarding could stack macOS permission popups when Full Disk Access was revoked or half-granted.</li>
  <li>An offline update check showed raw request text, and a crash report that didn't go out closed as if it had.</li>
  <li>Turning the AI off didn't stick when the database refused the write.</li>
  <li>Saved SFTP and WebDAV places with an email address as the username wouldn't open or sign in.</li>
</ul>
<h5>Security</h5>
<ul>
  <li>An AI client can no longer change your consent answers over Cmdr's automation interface.</li>
  <li>A multi-word filename could leak part of itself into an uploaded error report.</li>
  <li>A password typed into a network address or server path could reach the log file.</li>
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
