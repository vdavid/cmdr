# MacUpdate listing

Live: https://cmdr.macupdate.com/ (accepted from the 2026-09-18 v0.46.0 submission, the first of three that went
through). Submission form: https://member.macupdate.com/content/submit (needs a MacUpdate member account). The same form
creates and modifies a listing: type `Cmdr` into "Modify an existing listing?" at the top to load this one.

Status: **v0.55.0 prepared 2026-10-22, not submitted yet** (the fields below are what goes in). Before it, v0.53.0 was
submitted 2026-10-15 with the reviewer pinged the same day; the live page showed v0.46.0 then, and the v0.47.0 and
v0.49.0 updates were prepared but never submitted. v0.54.0 got no submission of its own, so the v0.55.0 Version changes
cover both. Neither release made the Description false, so it's unchanged since v0.53.0.

❗ **A submission sits unreviewed until David pings a MacUpdate reviewer directly.** The v0.46.0 one only went through
after a ping, and the earlier ones never did. The contact is in the vault's directory blitz plan (MacUpdate entry), so
every handoff ends with "submit, then ping". The v0.53.0 submission skipped v0.47–v0.52, so its Description picked up
what shipped in between (multi-rename, compare folders, folder sizes on demand, S3).

**The Download URL is the dynamic `getcmdr.com/download/latest/universal` link** (since the v0.53.0 submission), so it
never goes stale and downloads count toward MacUpdate. Their guidelines ask for "the direct URL to the installer package
(e.g., .pkg .dmg, .zip)", and this one takes 3 redirects to a signed `release-assets.githubusercontent.com` blob URL
that doesn't end in `.dmg` (verified 2026-09-18). The two submissions that used it went unreviewed, but so did
everything until a reviewer got pinged, so the missing ping, not the redirect, is the likely cause. If MacUpdate ever
refuses it, fall back to the version-pinned GitHub Releases URL
(`https://github.com/vdavid/cmdr/releases/download/vX.Y.Z/Cmdr_X.Y.Z_universal.dmg`), bumped per release.

"Version changes" covers the release being submitted (the whole minor line, patches included, once patches start getting
skipped here); the Description carries everything older.

Refresh cadence and what to update per release: `docs/guides/releasing.md` § "Refreshing the app-directory listings".

The Description and Version changes fields take **HTML**, not plain text: `<p>`, `<strong>`, `<h5>`, `<ul>`, `<li>`.
Their own hint says to keep pricing and promotional text out of the description, so the license and price story lives in
the Price field and the note to the review team instead.

## 1. App and developer info

- **App name**: `Cmdr`
- **Developer name**: `David Veszelovszki`
- **Download URL**: `https://getcmdr.com/download/latest/universal?ref=macupdate.com` (dynamic, no bump per release; see
  the Download URL note at the top)
- **Product page URL**: `https://getcmdr.com`
- **Purchase URL**: `https://getcmdr.com/pricing`
- **Developer support URL**: `https://github.com/vdavid/cmdr/issues`
- **Version number**: `0.55.0`
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
  Cmdr is in open beta. It is well-tested, stable software used every day by the author and hundreds of early adopters.
  Feedback goes straight to the developer!
</p>
<h5>Core features</h5>
<ul>
  <li>
    <strong>Basics:</strong> Two panes, tabs, command palette, keyboard-first. Common shortcuts like F5 to copy, F6 to
    move, F8 to delete all work, and are remappable.
  </li>
  <li>
    <strong>Operations:</strong> Browse, copy, move, rename, delete, compress/decompress with accurate progress bars,
    honest ETAs, cancellation. Optimized for data safety, speed, and transparency.
  </li>
  <li>
    <strong>Advanced operations:</strong> Queue multiple file ops, send any of them to the background, pause/resume file
    transfers, view a full, searchable log of past operations, with rollback for anything that didn't permanently delete
    data.
  </li>
  <li><strong>Multi-rename:</strong> With patterns, counters, and search and replace, live preview, Undo.</li>
  <li>
    <strong>Speed:</strong> Lists 50,000 files near-instantly, the built-in viewer opens a 10 GB file in a second,
    near-instant search.
  </li>
  <li>
    <strong>Accessibility:</strong> Follows macOS accessibility settings, works with VoiceOver, has WCAG 2.2 AA and
    APCA-verified contrasts, and everything works with the keyboard.
  </li>
  <li><strong>Design:</strong> Looks good, has real dark and light modes, follows native macOS look and behavior.</li>
</ul>
<h5>Extra features</h5>
<ul>
  <li>
    <strong>Phones:</strong> Full access to Android phones, Kindles, and cameras over MTP and PTP, up to 4x faster than
    Android File Transfer, no hacks needed, works out of the box with any USB cable. Also supports ADB over USB, and
    handles iPhones natively.
  </li>
  <li>
    <strong>SMB:</strong> Full access to network drives over a custom SMB implementation, which is up to 100x faster
    than the macOS client. Copies within a share happen in-place on the server.
  </li>
  <li>
    <strong>More servers:</strong> Full support for SFTP, WebDAV (Nextcloud and others), and S3 servers (AWS, R2, B2,
    and more), including cost estimates for S3 transfers.
  </li>
  <li>
    <strong>Indexing:</strong> (Optional) Keeps a (local&amp;private) full index of your disk to display live folder
    sizes and for near-instant search.
  </li>
  <li>
    <strong>Git:</strong> In Git repos, the commits, branches, worktrees, and stashes are browsable like normal folders.
  </li>
</ul>
<h5>AI features (entirely optional, can be fully local and private with a built-in LLM)</h5>
<ul>
  <li>
    <strong>Fully optional!</strong> With AI features switched off, Cmdr is a complete Total Commander-style file
    manager. Many people don't like AI features, and they are off by default.
  </li>
  <li>
    <strong>Model support:</strong> The default model runs on your Mac, so your data stays 100% private. You can choose
    to bring your own OpenAI, Claude, Gemini, etc. key, or point Cmdr at any OpenAI-compatible endpoint (like your
    ollama server) to use better models.
  </li>
  <li>
    <strong>Built-in chat:</strong> You can ask "Why is my Downloads folder so big?", "Summarize these three docs", etc.
    (alpha version)
  </li>
  <li><strong>Natural-language search:</strong> "Find my tax report from last year" just works.</li>
  <li><strong>Smart selection:</strong> "Select all screenshots in this folder" selects your screenshots.</li>
  <li>
    <strong>Image indexing:</strong> (fully local and private!) "Find me all photos in this folder where a dog looks
    into the camera."
  </li>
  <li>
    <strong>Natural-language renaming:</strong> "Rename all these screenshots based on their content." The agent
    <strong>never</strong> actually renames stuff. It can only <em>suggest</em> write operations like renames! You are
    in charge of reviewing and applying them. And if you change your mind later, you can always roll back any past
    operations.
  </li>
  <li>
    <strong>File organization:</strong> "Clean up my Downloads folder" and the agent proposes each move for you to
    approve or reject.
  </li>
  <li>
    <strong>Proactive suggestions:</strong> The agent can also watch your folders and raise things on its own: "How
    about moving that vet invoice to your donkey's medical papers?" The agent can only <em>suggest</em> actions, you
    need to approve/reject.
  </li>
</ul>
<p>Cmdr is source-available under the Business Source License 1.1: https://github.com/vdavid/cmdr</p>
```

### Version changes

Their hint asks for the changes in the current version, with `<h5>` section heads and `<ul>` lists. This covers v0.54.0
and v0.55.0 (no patches in either), since v0.54.0 never got a submission of its own. MacUpdate takes `<p>` in this field
too, so the data-safety sweep gets one line of context above its Fixed list.

```html
<h5>New</h5>
<ul>
  <li>
    Browse restic backups on any drive or server like folders: unlock a repo once, open any snapshot read-only, and copy
    files out to restore them. (alpha)
  </li>
  <li>Markdown files open formatted in the viewer, with pictures and links you can follow with Back and Forward.</li>
  <li>
    Type "cmdr ." in a terminal to open that folder in Cmdr. Homebrew installs the command, or add it from the menu.
  </li>
  <li>Remember your choice for files that already exist in the copy and move dialog, contributed by Jiri Slovacek.</li>
  <li>Escape cancels a running transfer, and Space pauses it.</li>
  <li>Drag waiting operations in the queue to reorder them.</li>
  <li>WebDAV trusts a NAS's self-signed certificate on first use, and you manage it in Settings.</li>
  <li>Settings &gt; Updates &amp; privacy &gt; Delete all Cmdr data removes everything Cmdr keeps on your Mac.</li>
  <li>An optional box sends the past five hours of redacted logs with your feedback.</li>
</ul>
<h5>Improved</h5>
<ul>
  <li>
    Copies, moves, deletes, compressions, and zip edits wait out a dropped server, then finish and clean up their temp
    files once it's back.
  </li>
  <li>Search Settings, the command palette, and shortcuts in any language, accents optional.</li>
  <li>Zips holding password-protected entries can be edited, and those entries keep their password.</li>
  <li>Cmdr asks once before moving Locked items.</li>
  <li>A wrong archive or backup password shows right under the field.</li>
</ul>
<h5>Fixed</h5>
<p>A big data-safety sweep fixed dozens of rare ways an operation could lose or damage files. The main ones:</p>
<ul>
  <li>
    Deleting a folder on an external drive, phone, server, or Windows share could also delete what its symlinks pointed
    to.
  </li>
  <li>Deleting, overwriting, or moving a folder could empty a drive mounted inside it.</li>
  <li>An Overwrite cut off by a crash or a dropped share could lose both the original and the new file.</li>
  <li>A file that changed size mid-copy, or an upload cut off by a network drop, could land truncated.</li>
  <li>Actions right after a folder change, a quick filter, or a sort sometimes hit the neighboring file.</li>
  <li>Undo could delete the only copy left of a file, or a file edited since.</li>
  <li>Settings, shortcuts, favorites, and saved servers got wiped when Cmdr couldn't read their file at launch.</li>
  <li>Copying small files to older Samba NASes dropped the connection.</li>
  <li>Cmdr crashed when a drive or share disappeared while it read a file there.</li>
</ul>
<h5>Security</h5>
<ul>
  <li>Error reports hide every path, name, server, and account.</li>
  <li>The license key lives in the Keychain.</li>
  <li>Cmdr warns before sending a WebDAV password to an http address, and tries https first.</li>
  <li>SFTP refuses a host key marked revoked in known_hosts.</li>
  <li>A local security event log, and an alert when Cmdr turns away a fake update.</li>
</ul>
```

### System requirements

```
macOS 12 Monterey or later, both Apple Silicon and Intel. macOS 10.15 Catalina and 11 Big Sur run on a best-effort basis.
```

## 3. Media

- **Icon**: `brand/logos/cmdr-512.png` (512x512, transparent)
- **Screenshots** (five, in this order): PNG conversions of the lossless WebP masters, since MacUpdate's uploader wants
  PNG. Run from the repo root; the output folder is gitignored, so the PNGs never get committed:

  ```bash
  magick brand/screenshots/app-main-light.webp brand/listings/macupdate-media/1-app-main-light.png
  magick brand/screenshots/app-main-dark.webp brand/listings/macupdate-media/2-app-main-dark.png
  magick brand/screenshots/search-light.webp brand/listings/macupdate-media/3-search-light.png
  magick brand/screenshots/chat-light.webp brand/listings/macupdate-media/4-chat-light.png
  magick brand/screenshots/settings-light.webp brand/listings/macupdate-media/5-settings-light.png
  ```

  - Five slots for eight masters, so the dark twins of search, chat, and Settings sit out: the main-view pair already
    shows both themes. The submitted listing still carries only that pair, from before the rest existed.
  - Re-run the commands for every listing update, since the masters change with each reshoot
    (`docs/guides/screenshots.md`).

## Comments for the review team

For an update, keep it short:

```
Hi folks! Thanks for listing Cmdr! This is the v0.55.0 update: new version number, version changes (covering v0.54.0 and v0.55.0), and fresh screenshots. Thanks!

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
