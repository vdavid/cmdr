# MacUpdate listing

Live: https://cmdr.macupdate.com/ (accepted from the 2026-09-18 v0.46.0 submission, the first of three that went
through). Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form
creates and modifies a listing: type `Cmdr` into "Modify an existing listing?" at the top to load this one.

Status: **v0.47.0 prepared 2026-09-24, not yet submitted** (the fields below are the update to paste). The live page
still shows v0.46.0.

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
- **Download URL**: `https://github.com/vdavid/cmdr/releases/download/v0.47.0/Cmdr_0.47.0_universal.dmg`
  - Version-pinned, so bump it with every listing update (see the Download URL note at the top). The redirecting
    `https://getcmdr.com/download/latest/universal?ref=macupdate.com` would never go stale and would attribute downloads
    to MacUpdate, but it's what the rejected submissions used.
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.47.0`
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

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers v0.47.0;
everything older is the Description's job.

```html
<h5>New</h5>
<ul>
  <li>
    The file viewer opens files with one huge line, like minified JSON, without hanging. Reading, searching, copying,
    and saving all stay fast.
  </li>
  <li>Give any favorite a letter shortcut from its right-click menu. Contributed by Gábor Gyebnár. Thank you!</li>
  <li>A new "Allow cloud AI" switch: no AI feature sends anything to a cloud service until you turn it on.</li>
  <li>
    A per-share switch for Cmdr's fast direct network connection, so you can keep a share on the regular macOS mount,
    and a one-click "Connect directly now".
  </li>
  <li>Command-Option-Left and Right switch tabs too, like in browsers.</li>
  <li>A refused move or delete now names the folder that said no, and whether admin rights would help.</li>
  <li>A search says when it's waiting for the drive's index to load.</li>
</ul>
<h5>Improved</h5>
<ul>
  <li>An SMB share stays browsable while you copy to it.</li>
  <li>
    Cmdr no longer talks to PostHog: usage stats and update checks go through Cmdr's own server, every three hours.
  </li>
  <li>The search dialog opens much faster on large drives.</li>
  <li>Less CPU and memory use while idle.</li>
  <li>Faster uploads to WebDAV servers.</li>
  <li>
    Each drive's actions (eject, disconnect, rename, pin, forget) live in its own submenu in the drive list, and
    "Connect to server" moved to the Go menu, like in Finder.
  </li>
</ul>
<h5>Fixed</h5>
<ul>
  <li>
    Accented file names on a NAS's network share showed up but couldn't be opened, copied, renamed, or deleted. (Even
    Finder gets this one wrong.)
  </li>
  <li>Copies to some NAS and Samba setups failed or hung.</li>
  <li>Downloading a 1–8 MB file from a network share showed no progress and blocked browsing that share.</li>
  <li>A failed copy could delete a file someone else had just saved under the same name.</li>
  <li>In rare error cases on a network share, a rename or overwrite could replace the wrong file.</li>
  <li>A refused overwrite could leave a temporary file in your folder for up to an hour.</li>
  <li>
    In the viewer, Command-A then copy could drop a file's last line, and a copy could come back empty or with a line
    twice.
  </li>
  <li>Enter could open a file twice, and Page Up, Page Down, Home, and End jumped twice as far.</li>
  <li>Every menu icon went blank on macOS 27.</li>
  <li>Escape on a dialog also took the window out of full screen.</li>
  <li>An SFTP or WebDAV server that went silent hung operations instead of showing the disconnect.</li>
  <li>A transfer's progress bar could jump backward, or freeze without saying it was waiting on the source.</li>
  <li>A false "can't reach your server" notice could pop up at launch.</li>
  <li>The drive list could stall on a network share that stopped responding.</li>
</ul>
<h5>Security</h5>
<ul>
  <li>Dropped a code-signing exception that could have let another program load code into Cmdr.</li>
  <li>Release builds always keep server passwords and AI keys in the macOS Keychain.</li>
  <li>Natural-language search queries stay out of the log and error reports.</li>
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
Hi folks! Thanks for listing Cmdr! This is the v0.47.0 update: new version number, download link, and version changes. The description is unchanged.

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
