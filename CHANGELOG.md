# Changelog

All notable changes to Cmdr will be documented in this file.

The format is based on [keep a changelog](https://keepachangelog.com/en/1.1.0/), and we use
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Thanks again for all the feedback! This release is full of stuff delivered based on real human requests! Keep them
coming!

Some highlights:

1. SFTP and WebDAV support! You can now mount an SSH server or your Nextcloud/ownCloud/Synology as a volume. Try `⌘K`!
2. "Show in Finder" support for your downloads in Google Chrome. Look for
   `Settings > Navigation & file ops > Show in Finder`.
3. Context menu updates:
   - On files: "Share" and "Services" menus added. "Copy path" was _sometimes_ broken, now fixed.
   - Google Drive: You now have "Open in Google Drive", "Copy Google Drive link", and "Ask Gemini", like in Finder!
     (Btw, no other third-party file manager seems to have this; I've checked the 10 most popular ones!)
   - On Cmdr in your macOS Dock: Right-click to see your tabs and bookmarks, and to open folders and servers!

### Added

- Connect to SFTP and WebDAV servers (alpha): a Servers list, saved servers in the volume switcher, one sign-in sheet
  with SSH host-key checks, ⌘K to add a server, and server addresses in ⌘G (4bd5769cd, cb23070e3, 116633dd8, d76c28aab,
  42d7b6fff, 59c204ae7, e7f391a0c, 8fbfa65c0, 4d2892967, 0a7308b5a, f52e62b39, cc4155228, 65d7f9c3c, 9c49cd283,
  891239110, 0c22aff63, ddab97974, d546df1bc, 1cbf731a8, 979b07ac8, 181ebf6ee, 3ec450e83, b0381ae6d, 6e150d019,
  0ecc815cc, 3fd8ad4dc, 98682b45d, 4d2cf2e67, c11408ea0, 9d89c43e2, e9ac0d601, 4a7753494, 357f5371a, 8ab154201)
- Browse an Android phone's whole filesystem over ADB (alpha), with a pane that waits for your Allow tap and a Settings
  page for `adb` (5ee5ea452, e9bd05fbe, d52186d9a, 53ca26515, 4a2efc50f, 61fa41f02, 74770f238, a9fef5541, a9a790a9d,
  28964c941, 1701ad579, f0d1da5fc)
- Share files from the right-click menu to AirDrop, Mail, Messages, and your other share extensions (b49af2d49,
  1dcec600c, 017fc3c61, ed3b9e3cf, 2bf6909e0)
- Use every macOS Service Finder lists, from `Cmdr > Services` and the right-click menu (39befc24e, c6c9835e7)
- Right-click a Google Drive file to open it on the web, copy its link, or ask Gemini about it (68eb3a2f9, bb182363b,
  6de78ee23, 98672145f, e60aa02aa, a4abe9475, 65568f608)
- Let "Show in Finder" in other apps open a Cmdr pane with the file under the cursor, from Settings or a one-time offer
  (94faf0dd5, bb3a3ad69, ac0f31ea6, c8003533b, c95ea3c52, e911a430a, 536f16d15, 1656b287f)
- Right-click Cmdr's Dock icon to search, go to a folder, connect to a server, or jump to a favorite or open tab
  (3469eb1c4, 05a537349)
- Offer once, a few days in, to pin Cmdr to the Dock next to Finder (6138e29e9, 6376f84c4, e8ca0bc63, 6c49f312a,
  1822d7c84)
- Start every file right-click menu with what it acts on, like `photo.jpg · 2.1 MB` or `3 items · 3.2 MB` (28f4a43a3)
- Add "New file…" to the File menu and the right-click menu (e401e3946)
- Right-click the function key bar to hide it (85254509c)
- Walk through AI provider setup in Settings the way onboarding does, with steps that fit each provider (6a2ed70df)

### Changed

- Rework onboarding into shorter steps: a quick checklist, one-line summaries with details behind info tips, and the
  app's own dialog look (3bf238b6e, b0b5a6f5e, 72d82cdf5, fbe86c8c4, 77da94242, ed04861c7, 4b15a5203, cfe9d7ccf,
  c12656f38, 861a11cd2, 2d372a46d, 59ae9a03a)
- Hide what Finder hides (like ~/Library and /usr), and dim hidden items when you show them (4f502c2ac, c1aedbd78,
  da4239995)
- Sort file names the way people read them, with accents beside their base letter and your language's order, twice as
  fast (4c2bf067f)
- SMB shares sign in on the new sign-in sheet, and the switcher's Network row is now called Servers (c33a47512,
  5015246c1, e74b3a420, 2a7d24a68, 39b5ce2ec)
- Tooltips get out of the way the moment you press a key (44038a0a3)
- Refresh the app icon, and show it in the About window instead of a ⌘ placeholder (c16ed9869, b26646a5e, 8c87e3f8f)
- Say "Cmdr" or "the AI" in sentences that used to call every AI feature "Ask Cmdr" (356ff46a3, d7e29dbc8, d1f316184,
  c0faed042, 20d2cb84d, 7205922cc)
- The app is 1.4 MB smaller, having dropped an image encoder it never used (8c18c0b37)

### Fixed

- Fix Tab, Space, and other single keys reaching the panes behind onboarding and most other dialogs (65fb4f4ec,
  8db8de7af)
- Fix right-click menu items doing nothing while Settings or the viewer had focus (ae222ae2d)
- Fix the New folder and New file dialogs opening without the cursor in the name box (1d40218b1)
- Fix the function key bar printing F2 twice while Shift is held (2f5db8abf)
- Fix a folder whose size is still updating showing a `≥422 GB` floor while it shrinks to 56 KB (ca0619f0c, d87f4d4c1)
- Fix a DFS domain share falling back to the slow macOS mount, and an 8-second stall on a NAS Bonjour hasn't found yet
  (4b68dd979, 524500cf9, 1891d18a2, e0df67bfb, b907ce55c)
- Fix New Zealand, Irish, Indian, and other regional English Macs getting US English instead of British (fd32191ea)
- Fix choosing no AI in onboarding leaving Ask Cmdr on, with a corner badge nagging for a provider (7b2bb151c,
  bc731c1cc, 5a4324c42)
- Fix pressing "Start using Cmdr!" with the terms unticked sometimes leaving their checkbox out of reach (cbf755cd3)
- Fix Qwen's setup showing no sign-up or API key links, and LM Studio's guide link going nowhere (057e0c42e)
- Fix screen readers skipping the restricted-folder marker, and restricted rows dropping below readable contrast in dark
  mode (80a93d8b0, 83a6e7a3f, d2f9cc23c)
- Fix `brew install cmdr` failing on current Homebrew (a8f229b0f)
- Fix translations naming macOS labels that aren't on screen: "Local Network" in every language, and Hungarian's old
  Applications folder name (db1dc0858, d9c91ffa1, eb165cbba)

### Non-app

- Fail the build when a translator guide cites a message key that doesn't exist, after repointing all 91 dead citations
  (07be26551, aeda68739, 26b279d4d)
- Make the contrast checker report text dimmed with CSS opacity, which it couldn't see before (71cb254d7, e00f2133f)
- Log a failed file operation's real OS reason, and fail lint on a log field that never reaches the log (ee26f5b2c,
  8e24eb2a6)
- Verify the website's visual baselines inside the container that shot them (c1bb61a46, 2e26326f5, 58c6f9fbc, 8e2ddf6bd,
  eca41ccae)
- Fix every browser login to the analytics dashboard being refused (d4411cc4e)
- Split the oversized transfer, scan, and startup modules along their real seams (307a73a9f, 0260a19e3, e6b0b0600,
  5bc04657e, 56fcbd365, ec1ad44c5)
- Give agents a business docs home with the product facts that change commercial answers (a4606db51, 3831e2c55)

## [0.43.0] - 2026-09-07

Thanks for all the bug reports, folks! ❤️ And keep them coming!

Highlights:

- A bunch of important fixes based on feedback and an adversarial hunt: data-safety, file viewer selection, in-pane
  search results, and more.
- "Open terminal here" feature. Just right-click to try it!
- Cmdr now can browse Office documents like .docx files (because those are secretly just zip archives!) — disabled by
  default, enable in Settings.

### Added

- Add "Open terminal here" (⌥⌘T, File menu, right-click, command palette), pointed at Terminal, Alacritty, Ghostty,
  Hyper, iTerm2, kitty, Warp, WezTerm, or any app you pick (c6a8e3fc6, fe528c277, 840873bea, 2d51e7c98, 66792e2ce,
  2b2396f7a, 9249cb54f, cdb70d24a, db8d8a166, 588e04485, cce75b72a, 3878363bb)
- Press Enter on a `.docx`, `.xlsx`, `.pptx`, `.jar`, or `.apk` to browse inside it, read-only so the document can't be
  corrupted from in there (c01a33061, 88c0857e8, c656d18eb, 41f529d33, 0d54da0d9, 02c1141d6, 7b1df722b)
- Grow the F3 viewer's selection from the keyboard with Shift+Arrow, ⌥⇧/⌃⇧+Arrow, Shift+Home/End, and ⌘⇧+Up/Down
  (c1dc7b79e, 9a6abc662, e9fabd6ac, 6faa54f73)
- Turn on a blinking text cursor in the viewer (Settings > Viewer) to see where the next Shift+Arrow starts from
  (71b48296d, 00fce0801)
- Ask Cmdr can find a file by name now that search joined its tools, and it reads a search's coverage field by field so
  a walk still running never reports "no matches" (e5b18e190, d5001db9c, 1e1ff369d, 56ede48e2, bd5a39e7a, 95de45f0e)
- Sort a search-results pane by column, with a third click on the active column restoring best-match order (af7ec36a5,
  a6115b10c, 9bc5a5f9e)
- Open a file from a repo's history in the viewer, copy one out with F5, and let Ask Cmdr read it, the way you already
  can from a zip (be252615f, 920fb38ce, 449ce62a2, ba57b5b14)
- Find 12 more Settings actions by searching for them (Clear index, Open log file, Get a license, Reset all shortcuts,
  and more), and the License page stops coming up blank (dd369a1ed, 1d4f8d063, 03a27e7d1, 884e51125)
- A chat says when its memory size changed, so shrinking the window mid-conversation explains itself (de07399d1)
- Answer a search over MCP with typed fields instead of a text table, bounded so it can't crowd out the caller's turn
  (9ab59f212, a3af205ec, 6b76d2e8a, 97d18838b)

### Changed

- Image results in Search are off by default now, with a switch in Settings > Indexing to bring them back (fca200d8c,
  234a2cb25, 281a9f090)
- Raise the smallest local AI window Cmdr accepts to 32,768 tokens, so a local chat that's offered can actually answer
  (374787e2d, ba0fda354)
- Copying off a share stops spending a round trip per file asking for permissions the share doesn't have (c80a1a654)
- A burst of git writes reaches the panes as one refresh instead of two (ae2d06276)

### Fixed

- Fix Cmdr dying on launch on macOS 10.15 Catalina, before any of its own code ran (16e064d97)
- Fix "Overwrite", "Overwrite all smaller", and "Overwrite all older" deleting a destination folder they compared
  against a file, whether you picked them in Settings or answered them on the prompt (7ab0ea51b, 4914ac6be, 1194fbc2d,
  0ec50f108)
- Fix a folder replacing a file destroying that file before its contents had landed (7dc3f86cd, 23367c944)
- Fix a merge silently replacing a file whose name only differs in case or accents (49e618303, 478528d81)
- Fix a move between drives destroying files that landed in the source while it ran, and say what it left behind
  (a59f78deb, 212702bdd)
- Fix a move walking through a folder symlink and emptying a folder you never selected (1ad7e9b35, ecaf48419)
- Fix a copy that fails partway deleting the files it had already replaced (02405134b)
- Fix your only copy of a file being swept away an hour later when an overwrite couldn't finish (aaf8daca7, 78396d26d,
  92bddf241)
- Fix a same-volume Overwrite taking the destination with it when the replacing rename refuses (f0da503b2)
- Fix a flaky NAS or phone reading as "nothing is there" and replacing a file you asked to skip (3c7750fef)
- Fix an undo after a folder merge reporting success without putting the folder back (14ac49b08)
- Fix a file that appears at the destination mid-move being overwritten with no prompt (dad8671cb)
- Fix copying two same-named folders from different places fighting over one destination path (1938da258)
- Fix a cross-volume copy abandoning a half-built destination with no cleanup and no word (f054042cd)
- Fix an empty `file (1).ext` left in your folder when a merged file never lands (0faaa9130)
- Fix the copy summary counting a skipped file as copied, and naming a folder that never arrived (4b8b740eb, 724b918a0)
- Fix compressing onto an existing file Cmdr may not write destroying it first (c687007de)
- Fix double-clicking the title bar of the viewer, Settings, queue, or shortcuts window doing nothing and sending an
  error report (726ec6173)
- Fix every plain folder wearing the home folder's icon, and folders with a custom Finder icon showing the generic one
  (51dbb4f69, 0be67267c)
- Fix the back and forward buttons doing nothing on a Logitech mouse running Logi Options+ (7699015f9, d2182e63b)
- Fix F8 in a search-results pane deleting one row instead of everything you selected (566aa5c9a)
- Fix right-clicking a selected search result acting on one row (65d6c0a65)
- Fix a search-results pane's selection drifting onto the wrong rows after a delete (e056bf37f, ca96333a3)
- Fix a delete or move from a search-results pane running against the wrong drive (2bee1bb0a, c240bb669)
- Fix ⌘C on a search result from a phone putting a broken path on the clipboard (0767b6932, 2a0c066ed)
- Fix searching a folder's parent returning only the child's files and calling it complete (ea953b6d3, 0635b2c73,
  cc68ca2d8)
- Fix Quick Look refusing a `.zip`, and renaming one skipping its permission check (01277e0d5)
- Fix the AI provider dropdown's top two options being unclickable in Settings (a5c0578d0)
- Fix deleting a repo folder on an external drive, a share, or a phone stopping with `.git/` still on disk (6910f59d4)
- Fix a pane inside a repo's `.git` browser offering renames, new folders, and pastes it then refuses (ae10c6c91,
  3d0098f1d)
- Fix a `.git` pane losing its six folders on ⌘R or after any finished copy (9f8e77b45, 6ebbb3ecf, 9a814a8cc)
- Fix a repo's `.git` panes going stale: a lone branches pane, a repo under a symlinked path, and the portal toggle
  (a2656b48c, 77d03aa30, 6f17e9788, 1b428768e)
- Fix the git browser's Size column shipping English to every language, and reading "1 commits" (492e990a2, 3bdcf6963,
  06257945c, 6edbc7218)
- Fix browsing to a name that isn't in a snapshot claiming the repository is damaged (b460de2c4)
- Fix a script extracted from a zip or copied out of a repo's history arriving without its executable bit (ff501a235)
- Fix a move off a read-only source telling you to change a destination that was fine (ec22f8a08)
- Fix a bulk rename asking once per batch and destroying the review you were still reading (a075ed928, da250d43f)
- Fix Ask Cmdr claiming a rename plan was waiting for review when the plan had been refused (e7b41c5a0, 05dbc57cf,
  20aabbc09)
- Fix a wake digest listing the same folder up to nine times, and expanding one freezing the Ask Cmdr panel (4a57e47fd,
  afad8e03b)
- Fix double-clicking a word then dragging selecting that word alone in the viewer (07c7c5068)
- Fix ⌘A on a huge file with no line index asking about a size the file knew all along (c563ba3d1)
- Fix Eject staying available while a paste was still reading off the drive (bf0821a01)
- Fix an agent being told a navigation arrived at a folder the pane never reached (45ae307ce, dce5404a2)

### Non-app

- Extract the MTP backend into `crates/cmdr-mtp`, so the phone-talking code compiles and tests with no app in its
  dependency graph (da8bd5ec3, 516874e7c, 7707cdecd, 45fef4c38, 6904b9b07, 5e6532be5, 392df5dab, 7a34ec01f, 3a32c44f6,
  8fba1b511, 42b5b0839, fec5279a2, c194dae25, 78f1dc3fe)
- Extract the git browser into `crates/cmdr-git` and turn the virtual `.git` trees into a routed read-only volume, so no
  walker can meet a folder with no inode behind it (1d2c34bf0, 1ddc41ba8, 1850d618c, a7c0e28ad, e1c00fdc5, d63cd277a,
  6910f59d4, 1ea4416e1, 9fb7cea63)
- Fail the build on a system framework newer than the macOS version Cmdr promises, the bug class behind the Catalina
  crash (5dfabd94b, 1ed07b6c7)
- Fix the api-server's telemetry size check throwing a false daily alarm since March (f90f7be2a)
- Record an outside contributor's PR as merged on GitHub instead of leaving it open forever after the code lands
  (92ef49348, 6c783f8c4, a88c5b7ad)
- Turn the data-safety hunt's remaining nine follow-ups into a ranked spec, with all 15 original findings closed
  (9996183f5, 966760525, 4fd23e33c)
- Shrink the website's visual baselines from 28 PNGs to six by shooting machinery instead of marketing copy, so a copy
  edit stops costing a baseline (abdc336ac)

## [0.42.0] - 2026-09-04

Headline changes:

- Added "Rollback" feature: undo a copy/move/rename/trash action from the operation log, even days after the op.
- The AI agent can now look inside many types of files, not just images.
- A huge run of transfer data-safety fixes!

Also added British and Australian English, traditional Chinese, and macOS Catalina support.

### Added

- Connect to WebDAV servers (Nextcloud, ownCloud, Synology, Fastmail, and any generic NAS) as a volume, with the same
  reconnect and sign-in story as SFTP, and a space readout for the accounts that carry no quota; no sidebar or sign-in
  UI yet (60def8113, ad535c04e, 31520cc75, 205e3cda0, 5a00b290b, 470ca9324, 73c223e58)
- Roll back a finished copy, move, rename, or delete from the operation log, with honest progress, pause, and stop, and
  a question worded for what the reversal will actually do (3395ed6c8, 296b08ba9, 800084893, c93b2da5b, cead2d0cb,
  969ac5892)
- A row you can't roll back now says why on sight, instead of a bare "Can't roll back" badge (63dfb82a9, fb3b37661)
- Undo an accidental F8 from the trash toast, and jump to the trash the items actually landed in (37da9004f, 21dfe1002)
- Ask Cmdr can look inside a file: text the way the viewer reads it, a photo's EXIF, a PDF's text page by page, and an
  archive's contents, searchable across up to 200 files in one go (4de992b01, 95c11d43d, 4b2720791, 0058d3573,
  6d644e499, 5ceed12c2, eebafea40, 0776d5fcd, 141b600d0)
- Read Cmdr in Traditional Chinese, all 3,138 strings, with the menu bar and shortcuts named the way a Taiwanese Mac
  names them (ff72d0ce5, a90bc5c91, 5d83ac394, 1ea3f6079, d50048dcf, b3c356b85, 90ac66328, 859ef8daf, 7bf3d40d5,
  0069cb730, 88bf23922, b78b50c34, 0d1ec2ac8, 7ab466328, 52878b4a9, 60dfff528, 31282f2a8, 187d60ad2, dc59e3c54,
  4f4d40496, 65e7b3b2c, 689cbbc4c, af3c7db0c, 9b8264116, 1276938c2, 8242671bb, 151ca91d1, 869f1a1b5, 77a790b4c,
  519778ed4, 7831efa23, 7540931f6)
- Read Cmdr in British and Australian English: the Bin, licence, colour, favourite, and 60 more strings your own Mac
  already spells that way (99b0fa5e9, 6ceba6676, a8a8b92e1, 839147ac5, cac86711a)
- Invert the selection with ⇧8 or the numpad `*`, from the Select menu or the command palette (b418be7b0, 78c4d64ff)
- Run Cmdr on macOS 10.15 Catalina and 11 Big Sur. Those two are best effort: everything works, but a few colors and
  small layout details can look off, and fixes for them come last. A one-time notice on those Macs says so, in your own
  language. macOS 12 and up stays the supported range. (9b2092721, ed324a564)
- Tell a Mac whose Safari is too old to draw Cmdr's interface what to do about it, in its own language, instead of
  opening a blank window (7f2ac2df7, 5707df62a, df75f9de9)
- Say what a search is waiting for when another scan holds the folder, with an elapsed clock and a give-up when that
  scan stalls (3a5e13f73, 32a4619d7)
- Add a note to a report you already sent, and give it a reply address (dcae0db2b, fae5e2350, df03f5e30)

### Changed

- Moving thousands of files on one drive finishes in under a second instead of nine, now that the durability pass
  flushes the directories the renames touched rather than every file (f793bed82)
- An upload to a network share now stands aside the instant you browse that share, and resumes the instant you stop
  (c4a21820f, b43b50fab, ee6e4ecfc, 70425777e, a7eaf7d5c)
- Copy over SMB charges read credits per file rather than per read window, and never asks for more slots than that
  window can carry (46f51716b, 1df7c4efe)
- Your first drive index now walks the folders you actually work in, instead of the same static list on every machine
  (b057366bb)
- A search over a NAS that fell asleep gives up on the first dead answer, instead of re-asking once per folder
  (a6497c169)
- Cmdr holds 246 MB less memory in a session that never runs a semantic search (3adf2b563)
- The app is 280 kB smaller, having stopped shipping translator tooling to every user (1cc78e44c)
- The restart toast names both versions and says that doing nothing still gets you the update (f3f8bc6cf)
- "Copy path" moves to ⌘⌥C, the chord Finder uses for "Copy as Pathname" (04f9afda7)
- Cap what Ask Cmdr's own wakes may cost: 200,000 tokens a local day, a fifteen-minute floor between them, and four
  attempts a day on a dead key instead of 261 (ff21f998f, 3b8eca224, 4cc0a02b4)
- Ask Cmdr stops paying tokens to dismiss cache folders, and temp dirs and `$HOME` stop scoring like project roots
  (14e1fd307, 055f5ced2)
- Every hover hint now reaches keyboard users, appears after 400 ms, and paints in Cmdr's own colors (f27bcdcf5)
- A storm of one-shot rescan requests costs one full check a day instead of a subtree walk each (fe3914fc2)

### Fixed

- Fix Cmdr losing most of its colors on macOS 12 Monterey: the shipped bundle is now built for the oldest macOS it
  promises, so nothing lands there that Monterey's Safari can't read (2a4ea468a)
- Fix a narrow tab on macOS 12 keeping a close button it had no room for, which crowded out the file name (0cf38a405)
- Fix cancelling a transfer deleting files something else changed, and a move-back destroying what had taken their place
  (4f12441b2, febe44d17)
- Fix undoing a move carrying off files the move never touched, and a folder move's undo reporting success while moving
  nothing (1573b3a85, cc331cbe7)
- Fix rolling back an SMB or MTP folder copy leaving the files inside it behind, and a cancelled transfer leaving an
  orphan tree (c19efcd83)
- Fix Pause not actually stopping a local move, a folder merge, a trash batch, an archive extract, or a copy building
  its destination skeleton (1e4dc8a67, be0af804d, be87c2e4f, 3caede87b, 63647eac8, 14a9c3aa4)
- Fix Pause and Cancel doing nothing while an operation is still counting files, on local drives, network shares, and
  phones alike (548ccd048, 7a1c05230, 290464f92, e98f10c7a)
- Fix a cancelled transfer claiming a clean rollback while gigabytes of its own scratch sat on the drive, and clear away
  what a crash left behind (360c7179c, 3abf7edbd, d7d29de7e, b8c79935b, 927399f26)
- Fix cancelling a move between drives leaving a staging folder in the destination (8d4d5e104)
- Fix Rollback offering an undo the engine can't make on a cross-volume move, and describing a copy's reversal on a move
  (abaaa3cd4)
- Fix stopping a move between drives saying nothing at all, when the originals it had already cleared were gone for good
  (540cd479e, 775d80645)
- Fix the progress dialog reading "2 GB / 2 GB (80%)" mid-copy, and a move between drives reading 100% with every
  original still to clear (7954bacb0, 613e31b6a)
- Fix the operation log misreading itself: a finished move says what it moved, a reversal's row adds up, and same-second
  entries stop shuffling (890c57d42)
- Fix a cancel toast showing a green "removed everything" when files stayed behind (a9eb6dd7c, e999a53b6, 9988950ed,
  26f1e7ccb)
- Fix a burst of external writes leaving a pane permanently missing files that are on disk (544845a9f)
- Fix shares with accented names falling back to the slow macOS mount instead of connecting directly (08e9adbb4,
  b789700db)
- Fix an SMB folder indexing as empty because one file id had its high bit set (167a1b70a)
- Fix pasting into a folder the transfer is about to create being refused on SMB and MTP (daa4ae63f)
- Fix a copy from a huge folder blaming the destination for a check it never ran, and a merge into an existing folder
  looking wedged (88a6649ae, e50c59920)
- Fix a header-encrypted archive bouncing you out of the pane instead of asking for its password (61ffc60fc)
- Fix transparent image pixels reading as solid white in the viewer, and a text drag or triple-click not selecting
  (2af74e538, 79068b902, 698175aa2, 206011e78)
- Fix holding Shift over the F8 dialog not escalating to a permanent delete (f7fdccdbc)
- Fix Cmdr starting up behind whatever you were doing instead of coming to the front (b734f77f6)
- Fix a toast disappearing while your cursor was still on it (705fbc8cc)
- Fix an abandoned listing leaking a cache entry and an armed file watch for six hours (05da59ecf)
- Fix a reconnecting SMB session stalling for minutes on a cancel it should never have sent (6d2239070)
- Fix an Ask Cmdr thread wedging for good on a malformed tool call from the provider (e0d467690)
- Fix Ask Cmdr's folder and rename suggestions failing on every call with an Anthropic key (a579be031)
- Fix Ask Cmdr reporting searches it never ran, and burning a whole turn re-sending one broken tool call (ff63eda36,
  a81fd646a)
- Fix an agent quitting Cmdr over MCP ending a running transfer with no warning (2babd38c3)
- Fix nine languages naming one action two different ways, so the menu bar, command palette, and dialogs agree
  (30b0fbd24, 438868c25, 5e1108cd1, 82625f751, 7d363d1bf, 4ef3d9a87, 86cd0150d, cbee111c2, 8f4679271, cbef38ae1,
  91adaa472, 93adf4991, 9025cd362, 8f099472d, 594f31957)
- Fix Simplified Chinese contradicting itself, so a Dismiss button no longer reads as "ignore this" and the licence
  screens address you like the rest of the app (688180914, 31d9f99ca, deba0e9d0)
- Fix voice-control users being unable to press the pause, case-sensitivity, queue, and remove buttons in five languages
  (a600b1532, 934492f92, b4302d809, 9dc78895b)
- Fix every window failing to announce the language it's speaking, a WCAG 3.1.1 failure across all 13 catalogs
  (cc8ef9013, 8534a96a8)
- Fix the Select and Deselect dialog showing English, and the files-per-second rate shipping English to twelve locales
  (95fac93e5, 14eb7c045)
- Fix a rate of one file per second reading "1.0 file/s", and one screen showing two decimal conventions at once
  (5d900c630, 9ce4d4a13)
- Fix a Hungarian reader seeing "a iCloud", and Dutch describing one failure two ways (6fea99129, a8d86da44)

### Security

- Fix the MCP bearer token being written to the log file, where it could be read back without one (08f631a47, f5909eba8)
- Withhold a contained panic's message from the log, so bytes of a file you opened can't ride into an error report
  (6626c1e9a)

### Non-app

- Add `crates/cmdr-adb`, an Android-over-ADB backend that browses a phone's real filesystem, plus a registry so any
  device backend can put itself in the volume list; not reachable by a user yet, with no connect flow and no device
  picker (733a25b6f, 49153fbac, b4ad39cca, 68eca5c6f)
- Ledger 15 verified data-safety defects in the transfer engines from an adversarial hunt, ranked, each with the
  mechanism and the guard that was ruled out (e8406f4af)
- Let a regional variant ship as a 60-key overlay instead of a 3,138-key clone, which is what makes `en-GB` and `en-AU`
  affordable (edcc82acc)
- Fail the build on a WCAG 2.5.3 accessible-name regression, and on a locale that gives one thing two names (c17e0c3c0,
  44bdbe3a4, 614bd364f)
- Fail the build on a Vite bump raising the browser floor above the macOS version Cmdr promises, and let a
  newer-than-floor selector ship behind a proven runtime gate (2286d86a1, f4b7dcacb)
- Stop a tag push pointing the whole install base at an older build, and prove the updater still works after a
  `tauri-action` bump before shipping (c87d202a2, 5813d7ca9)
- Give every AI provider we hold a key for a real-API lane, so a model decommission surfaces wherever it happens
  (ec1a817a7, 4157df165, d36aa5d83, c6866439b)
- Alarm on a broken api-server cron job and a dead Resend key within a day, instead of failing into silence (d80bc3ce5,
  34e098baf)
- Close a forged-header bypass of comment voting and rate limits on the blog's comment system (510fe70a2, bdec4167d)
- Stop the test suite writing into the developer's real credential store, and stop it starting again (45c420770)
- Move the newsletter database to Postgres 18, with a cutover rehearsed end to end (d40ac68fe, 227810b9b)
- Kill a stuck CI job in minutes instead of burning six hours of runner, and a wedged E2E app in seconds instead of two
  hours (4f33cd527, 874eb61ad)
- Collapse the frontend and Rust duplication the sweep found: shared toast bodies, name checks, entry-row mappers, tar
  walks, and rename engines, with the split suites named after what they pin (038deea7d, 03ff3dede, e2816e9b8,
  b75a52cb4, c1eb73de6, 47de9f576, 731ce716b, ba2e7cca6, 9b48c70bd, 5cc2cafc3)
- Split the concurrent copy driver into three files named after a source's three stages, and cover the four data-safety
  rules nothing watched (8f77be3f7, a5c829470, f71f3a016, 75b6083bc, 5a72a6856)
- Move CI onto the Node 24 runtimes and the current lint and test majors, so five versions of upgrade debt clear
  (73b4cfa79, a1469497d, 2024d3c86, 05f95e136)
- Keep Rust and TypeScript code intelligence working for agents and editors across toolchain bumps (65f19fc66,
  29ed8e242)
- Enforce canonical Tailwind classes and typed lint rules on the website and the analytics dashboard (2f212c2f8,
  9562ae369, 611326f90, 4a3f8efd7)
- Say which macOS versions Cmdr is actually for, on the download page and in the directory listings (e7dd3d7a1,
  4f79c23b8)

## [0.41.0] - 2026-08-26

Cmdr was borked on v0.39.0 and v0.40.0 on macOS 12 Monterey and 13 Ventura. This update fixes it. Also some update flow
improvements, crash reports are on by default, and a copy via SMB now shows progress better.

### Added

- Ask for a reply address right in any report dialog, even with no address on file (1a2cf06c2)
- Name the address a report will carry, with a link into Settings that the label then follows live (4a54c9150)

### Changed

- Turn crash reports on by default, sending the app version, your macOS version, and where the code stopped. Never your
  files! Switch it off in Settings > Updates & privacy if you'd like, but crash reporting really helps. (82bd918e4)
- Clicking "Later" on an update no longer ends the conversation: Cmdr keeps looking, picks up anything newer than what
  it already downloaded, and asks again about once a day (8594de051)
- Bound what a copy to a NAS queues up, so cancelling stops in seconds (eb367cb0f)

### Fixed

- Fix Cmdr aborting on launch on macOS 12 and 13 (c004a1fea)
- Fix a NAS copy's progress racing ahead and then freezing: it counts bytes the server confirmed now (c9b841349)
- Say when Cmdr runs from a spot it can't update itself from, like Downloads or a mounted disk image, instead of
  re-downloading a 63 MB update every hour (eb6025651)
- Fix an update server outage reading as "Couldn't parse update manifest" (b795b921e)
- Fix the Settings email field showing `analytics.email` instead of your address (5c24351d6)
- Fix a `code` word in What's new stacking one letter per line (2bed1bfb2)
- Fix a link in a What's new entry doing nothing when clicked (30180107b)

### Non-app

- Fail the checks on any Objective-C selector newer than the macOS version Cmdr ships for, the bug class behind the
  launch crash (1d2cc9035)
- Add `cmdr/no-confusable-callback-params` and turn it on in all four apps, so two same-typed positional callback
  parameters are a lint error (40a058e55, f5bedc9fd, d2f06c799)
- Convert 105 confusable callback signatures to named payload objects, production and tests alike (9c289d6ee, 9a9a8a64d,
  b060586af, da0a1dc6f, 2f6e0a60f, cc961f9b2, a576cd248, faec43095, f48534f3a, b09a0cc63, 83b2263a8, 74c436fb9,
  0139b44ab, 60f56cd02, 567116bf9, ab9a532be, 8cbdfb8ca, debf4dc58, b3788f649, b5badd076, 89a869640)
- Answer "how many people use X" on the dashboard per app version, resolving an absent setting against what each release
  defaulted to (c8743ba52, b7d6a5d51, 347aa26cf)
- Report active installs as a range, since an install with analytics off sends nothing at all (f55a045d4)
- Make the update path visible with a PII-free `update_check` event naming its outcome and what set it going (09a6b4889,
  01acc678f)
- Stop Cmdr's own dev, E2E, and tooling runs registering as active installs in the update-check table (529da1a92)
- Mail hand-written error reports and in-app feedback to David's inbox instead of leaving them in Discord and D1
  (485f8f501, 9d8b58db1)
- Close the SFTP connect leak at both ends: a cancelled or abandoned dial ends the server's session at once instead of
  holding it for the life of the process (b1bf0a313, 04b4cbc31, d6630f6a4, 9274002b2, 39d9b1b16)
- Name the wedged file inside a folder move in the transfer probe, instead of hiding it behind the folder's row
  (96ebf91f3)
- Make `.mise.toml` the only place the repo names a Go toolchain (4aeecd4d3, c04fc03f7, 8e353dded, 37fcd9919)
- Unblock the Go 1.27 bump: staticcheck and three more analyzers can read its export data (13c3f2e65, 7bb1e581a,
  f8a545d37)
- Stop the SFTP test fixtures publishing twelve sshd servers to the LAN and the tailnet (c89100ef1)
- Stop an E2E run watching the developer's real Downloads folder (6bed8e6ec)
- Fix two E2E flakes: a search overlay leaking into the next spec, and a wake spec racing the turn that writes its
  digest (7fb02fd6a, 077667298)
- Measure the self-move to Applications, and drop the Full Disk Access worry that decision rested on (7d8bb8a63,
  35c835d41)
- Show translators the real move-to-Applications dialog, plus three other surfaces that had none (75c52f28c)

## [0.40.0] - 2026-08-24

The built-in agent now watches the file system and can suggest actions on its own, like "Hey, how about we move
`~/Downloads/puppy-eye-exam-result.pdf` to `~/papers/doggy/medical/`, looks like all similar papers live there". The
agent can only suggest, you approve/reject everything.

Also a bunch of performance and correctness fixes.

### Added

- Ask Cmdr watches your folders by default, waking on real disk activity and leaving no thread behind when it finds
  nothing (3c44e22fb, ff25394bf, 4a6a26509, d1e92c7fd, 7c391ec9c, 657e68c28, f1babc51f, 432e438ce, 05b69ec82, b026f7627)
- Add three "On its own" settings for Ask Cmdr: whether it watches, how calm it is (5 seconds to 2 hours), and whether
  it toasts (297fbbc66, e863bd3b7)
- Show a wake in the status corner while it thinks, with a way in and a way to stop it (2bd74f952, 1c873ee9f)
- Mark a thread Ask Cmdr opened for itself in the chat list and in search, and open it in your own language (ffbb0a85f,
  7d46af4cb)
- Toast a wake that staged a change for you to review (df4a34e63)
- Let Ask Cmdr keep notes about you in a memory folder it can't write its way out of, which you can open and wipe
  (6269d96cd, 271764ac2, d8093f6a8, e3e77760d, 2da00041e, 8f8328bb1, ee0f3bf30, 8de876816)
- Tell Ask Cmdr what you decided about each suggestion, so it asks about the ones you turned down (7ed4e0594, 81d77271b,
  9d5cd27db)
- Add ⌘R to re-read the folder you're looking at, phones and network shares included (34c01dea0, ca6aef1c5, 0efa1e6a5)
- Say when a share drops to the slow macOS mount, with a "Try connecting directly" button (187212e09, 0041834ab,
  0c5e2769a)

### Changed

- Cut 2.8 MB off the app by leaving translator notes out of the bundle (183024ddd)
- Keep an Ask Cmdr answer through a reload, and let a wake's thread read live (a0b44afdd)
- Stop an idle Mac asking the cloud providers 43 times a minute about a folder that isn't a cloud folder (338963f8a)
- Stop an idle indexing tick re-reading 90,308 folder scores and walking directories it can never use (56e60c4aa)
- Keep the file index's page cache inside the 64 MB it always promised, instead of 132 connections each claiming 8 MB
  (4d0719823)

### Fixed

- Fix a big folder stalling the pane: rows, watcher events, and Finder tags stop re-walking the whole listing
  (e39f05aa8, 290a23a58, af1b56a22, ca5c8ab4b, b53a01152)
- Fix the cursor and selection landing on a neighbour when a listing reorders itself mid-delete (55cbb2d5b, 8287e99f3,
  9937293a7)
- Fix a rename that blinked mid-copy deleting the file it was landing next to (6bc4fb86a)
- Fix an errno sentence or a `diskutil` message reaching a toast in English, in every flow that can refuse (6397681cd,
  147faf943, 2867fb525, 7358c3c6d)
- Fix a missing file being reported as "No such file or directory (os error 2)" instead of its own name (f1a59724c)
- Fix the crash dialog claiming Cmdr quit unexpectedly when it didn't, and saying it twice (3fce6eba0, 8bf17df70,
  148d08eca, 141c07678, 9214251fd, 77ec42fb7, 91e1b8d75)
- Fix the Settings crash-report toggle promising something it no longer does (2d0aadf7e)
- Fix a panic on a background indexing thread crashing the app at the next drive start (457ddb23f)
- Fix turning a drive's indexing off and straight back on leaving it dark for the rest of the session (4b04e1ac1)
- Fix a failed mount trapping the pane with every key dead (1426ae596, d7f03ecd5)
- Fix shares named `café` or `公開` refusing to mount (07a9f1ca9)
- Fix ⌘R in the network browser running two full share rounds per keypress (d36dae04f)
- Fix the macOS app menu saying "cmdr" instead of "Cmdr" (ff7ca0842)
- Fix the viewer, queue, Keyboard shortcuts, and Settings windows showing English chrome and the wrong number format
  (b559c1dfa, aa417b717, 1f54d8e98, 1b859da4f)
- Fix the window title and the macOS panel names Cmdr quotes staying in the old language after a live switch (deebdb60e,
  5c61e6877)
- Fix the German status bar declining folders into the dative, and Hungarian and Vietnamese pointing at Finder panels
  that don't exist (714abfc40, 8e8e01c21)
- Fix the Brazilian catalog calling a disk two different things, and its last European-Portuguese sentence (dd4dfa506)
- Fix an overdue wake spinning a CPU core flat (8067f08f4, 2f4a1f8f8, 560ed47db)

### Non-app

- Add the SFTP backend: sign-in with a four-rung auth ladder, host-key trust, reads seven times faster than a sequential
  one on a high-latency link, clobber-free writes, and server-side copy, over eleven Docker servers (403b77b9f,
  6395f6943, 243e0c8ce, 183aaad81, ec5887e50, ca85f7a20, 9afd9223f, b911c8004, a1911dbf8, ceacd1e3a, f19abb5b9,
  d8cf82e33, e4f68e6cd, c8f37d12c)
- Split the SMB backend and its protocol layer into their own crates, so a check is `cargo check -p cmdr-smb` instead of
  332k lines (22edeeb49, 4f436ce45, f02febb35, da0d4b11d, ba71876a0, c9ee1a916)
- Measure what the app holds at rest, and find the CLIP text tower costing 251.5 MB that enrichment never calls
  (ecc78c83c, 0f58a7e23, cd27791e7, 0857b5f19, abedcd652)
- Report which release, OS, and CPU produced each event, and cover the routine gestures that were analytics blind spots
  (3301818c9, e9c1402a8, 9e191baba, 2510eaed2, 57d06c017, 7b1482bbb)
- Stop Cmdr's own test and tooling runs registering as real users in DAU, installs, and the version split (c2e8f791e,
  346b1748f, 7b2b90c9c)
- Fail a check when an event fires undocumented or a documented event nothing sends (28ba4ac22)
- Re-run only the check whose file changed, instead of all 116 lanes (0caef2237, 0b829639c, aa0112903, 588286f7d)
- Catch a subsystem re-welding itself to another one, and ratchet the desktop bundle's size (9868eebdc, 3cce961bd)
- Cut 133 test files from the `svelte-tests` lane for the same a11y coverage (fbad0b46c, d242f179b, 3a275625a,
  fde17ac58, 638c3814d, 98a2b43d8, 233129c95, 1978e384d, e6252d6a7, 1fdb36f27, 72c02686b, dbd0324ac, ee9c2b91e,
  c8df29625)
- Fix seven per-test nextest overrides silently selecting nothing, leaving eleven flake-prone tests unserialized
  (9d7fbaf6f)
- Wipe 21 shipped specs, moving their durable intent beside the code (3cb159446, 2ff670aa0, e15f55bdc, 7e5f43774,
  54c834ccf, bdd7a7395, 3d5cf9974)

## [0.39.0] - 2026-08-19

Besides bug fixes, here are the three most important changes:

- The AI chat can now suggest file operations (then you approve/reject; agent can't do anything without your approval)
- Drive indexing now prioritizes your most important folders
- Localization improvements, and automatic switching if you prefer a non-English language

### Added

- Add Suggested ops: Ask Cmdr proposes file operations, and you review each one beside what Cmdr knows about the file
  before approving it or turning it down (c4b03d903, 8debc2fd3, 3ded61cb5, 43d74fee0, 426fb8d34, 99cc049b2, 862aeb7c9,
  b94169e86, 0d3af79cc, 47c56b4b2)
- Ask Cmdr wakes itself when something worth noticing lands on your disk, and opens a thread you can read back
  (d574f4a58, e2e091fc0, e435b4734, be9027bb0)
- Add the native menu bar, the right-click menus, and the startup alerts in all nine shipped languages (f8f3c935f,
  42c4c20f9, 09e6d6769)
- Switch Cmdr's language the moment you switch your Mac's, no restart (0154a1a14)
- Follow the region you set for dates, times, and number grouping, separately from the language you read (35d55dd3c,
  047ced55a)
- Walk your whole macOS language preference list, so your second choice is reachable and a Traditional-Chinese reader
  never lands in a Simplified app (d61d16996, a6f909688, 18d892c31)
- Name the language it landed on in the picker's "System default" option (cce3a236c)
- Add a language picker to the onboarding wizard, so a first launch in a language you can't read has a way out
  (abb9604d6)
- Add Duplicate on ⌘D, in the command palette, the right-click menu, and the File menu, on phones and network shares too
  (42384c92f, bda49c758, e067e6d24, a4fca37b3, 2b4328bd2)
- Duplicate a selection in place by dropping it on its own pane with ⌥ held (6b90b5155)
- Land in the rename editor after duplicating one item with ⌘V or F5 (fea31cba4)
- Index a drive folder by folder in the order you care about, starting where you last were, keeping every second of it
  across a quit (9fe0d3ec8, 87fbb4963, b2450af4c, 9687d2fe6, 1d3d70011)
- Index a folder you open next, instead of behind whatever big folder is in front of it (befd144b3)
- Say which folder a first index is on, and which of its three stages it's in (24662e558, 950e2f8bf, 9d7db4ce8,
  88562dd27, 278039ff9, 3be2e2ee0)
- Admit the folders a finished index couldn't read, in the drive badge's tooltip (6b0b8db9b)
- Search a mount that comes back without a rescan, because Cmdr retries the ground it gave up on (aab59ecdc, cc430db23)
- Rename a run of files in one keyboard flow: ArrowUp and ArrowDown carry the editor to the next file (1ae0a9789,
  101370904, d7ff3c758)
- Bring a backgrounded operation back to the progress dialog from the queue, with its bars, ETA, Pause, Cancel, and
  Rollback (6b9c9c15b, 0ad26f6d3, 1c4acfbdf, cd3543f4f, 267e1108d, 24ab591c5)
- Background, pause, cancel, and hold ⌘Q against a transfer that is still counting its files (5b7ca55ff)
- Escalate an F8 trash to a permanent delete by holding Shift (fff2e576b)
- Add "Show hidden files" to Settings > Appearance (4b0ff78c6)
- Open a fresh install on home and Downloads, once ever (6980d0e76, 919840ee8)

### Changed

- Hide dotfiles by default on a fresh install (57d6862dd)
- Resume a half-covered drive 7× faster: 185 seconds down to 26 on the benchmark tree (cadd8b81e)
- Settle a drive somebody keeps writing to in minutes, instead of waiting for the next launch (14dc2e6d9, 1e0c1add4)
- Cut what a search costs on a drive that is still being covered, and what tracking a walk costs while it runs
  (542718f2f, 0a524e6b9, e6d8c8fea, 12658b909, 7d1f910d9, 898f56347)
- Copy a folder with the concurrency window it always advertised (4b9f2e1dc)
- Grey out Copy, Move, and Delete in the File menu while a dialog is up or Ask Cmdr has focus (40514a268, fcc35395e)
- Name which file a clash prompt is asking about, and both ends of a transfer opened from the queue (ab61144e4,
  d6c9f9c02)
- Open the operation queue with one query instead of one per row (8cc38847f)
- Tell a screen reader what the scanning chip does and which chip it is (8f5ed6abc)

### Fixed

- Fix copying, moving, or compressing into a subfolder of a network share failing outright (e518a456d)
- Fix a destination Cmdr can't reach being reported as your own file going missing (ceb71910f)
- Fix a crash mid bulk-rename leaving renamed files with no undo (7bde003e7)
- Fix a stored search listing files that are gone, or hiding files that are still there (616184c93)
- Fix a share macOS mounted twice losing its watcher, and uploading an error report every time (272e9753b)
- Fix a folder on a sleeping NAS costing every later search a two-minute listing (baa1f4f2b, 07ddef4c7, 25a1ffbe0)
- Fix opening a folder mid-index stranding the first index for up to an hour on a wide directory (0bb0eb950)
- Fix a rescan blanking the index underneath a search that is still writing to it (cb9568d0f, 9747c16ad, 94b4c59a9,
  e90363640, 56858da8a)
- Fix "Rescan now" reporting success and doing nothing during a scan or a search walk (af133ef21, 6b69f14dc, 4a3468a52,
  aa4a6b8c2)
- Fix a drive you turned on being forgotten after an interrupted first index (57ae81573, 0e97d09d4, ea8a31480,
  5ed425bd7)
- Fix a poisoned lock permanently killing the MTP watcher, the space poller, the verifier, and the volume registry
  (cdfdcacd7, f1ea3cc54, 4130904db)
- Fix a transfer parking forever when the next clash arrives while you're answering the last one (56ebb8db5)
- Fix a paused transfer claiming a speed and hiding its time left (df1aed7b1, 496e33998, 83274f034, 3c19b6784,
  42a61d189)
- Fix two progress dialogs stacking over one transfer, and closing a dialog killing the transfer behind it (d03874711,
  018f4e4c0, 5f61f6eac, ac675359c, 2dc6e4735)
- Fix a scan preview on a dead volume spinning forever, and a wedged archive holding the scan dialog open (ee9963162,
  74c837b45, bb0173ace, 117cc65f4, d0eeb4a9d)
- Fix a copy started while another operation is running starting nothing and saying nothing (df42f7f02, 1cb9a554c)
- Fix the queue reading "Running" over a frozen copy, Rollback deleting on one click, and a view that attaches late
  showing a scan that isn't happening (f40ac1acc, 1778b1467, 2f3be6a17, 78b8a7048, c463808e3)
- Fix a folder scan over SMB reporting 0/0/0 until it finished (5e80b5e9f)
- Fix a guest share listing and the share browser waiting forever on a server that stopped answering (15ff0999f,
  00c1ad228)
- Fix duplicating `photo (1).jpg` nesting into `photo (1) (1).jpg`, and a folder with a dot in its name being numbered
  mid-name (f770d3a5d, 5d9de4e35, b1abd601c, 53a5c33cd, ff21a4f17)
- Fix a move into a differently-cased folder on a case-sensitive drive being counted done without moving (8c43edf29)
- Fix holding an arrow during a chained rename skipping files, and the editor vanishing three rows in (368f9d97b,
  25f9fc856, e8890e466, 135d2ea86)
- Fix a chained rename the volume refuses losing its name silently, and its toasts naming files from folders you've left
  (c173fd07f, 3863158aa, 7d9d49d80, 78659dc52)
- Fix the French and Swedish rollback buttons promising to restore files rollback can't bring back (0ae5d9c67)
- Fix ⌘C in the viewer copying the search query instead of the text you selected (6be268f72)
- Fix double-clicking a number in the viewer selecting the word before it (a45950231)
- Fix a stray green line under the title bar when a dialog opens at startup (ba1c9508b)
- Fix rebinding a shortcut stripping that menu item's icon (13eb0e0b7)
- Fix a search over ground another walk holds sitting silent instead of saying so (5d1875222, 6d04b410b, 92be35ee6)
- Fix Escape not closing a search dialog whose run never answers (a29bc77c2)
- Fix the onboarding wizard quoting a drive-indexing cost you don't actually pay (b7f4c5811)
- Fix a favorite on a wedged share stalling the order the index walks in (92d11c66f)
- Fix a trash waiting out a scan preview it never reads (a8d56440e)
- Fix MCP reporting a pause that never happened, and a false "queue is idle" (aba3d0e2e, 8cd18287a, 23edcb9f0,
  5acc15697, 13576b844, 65261dbf4)
- Fix an error report losing everything a log line says after a volume path (c3d28f72b)

### Security

- Update `h2` to close an advisory letting a peer keep a connection busy for free (e21872235)

### Non-app

- Add `lock-poison`, which sees the 203 places a poisoned lock is silently swallowed, the shape behind two shipped bugs
  (ce317da14, 22b27b7ba, 52e96cd20)
- Name the copy-pasted file pairs in the duplication check instead of hiding them behind a percentage, and point it at
  the frontend too (144907e35)
- Count every ❌ rule a subsystem's docs carry, as a number that can only go down (1acf1555c, bf56603c1, 04be43ef7)
- Add `discarded-outcome`, catching the next function that throws away an answer its caller needs (a8b9d6eb4)
- Say when a check run reformatted an already-committed file, so a green local check stops shipping a red CI (85859d889)
- Cut 172 seconds per run off the Playwright lane, and 15% of the frontend lane's invalidations (7663c42f3, 55360dc89)
- Pin the language of the E2E suite and the screenshot pipeline, so neither answers to whoever's machine they run on
  (858e5501a)
- Drive a one-file-at-a-time conflict answer over MCP, the state that hid a months-old wedge (917abbed1)
- Refresh the AlternativeTo listing, which had been describing a v0.24 app (42442a806, 95d25b59e, 0cc77feef, 9bd60d103)
- Move the Go toolchain to 1.26.6, clearing four stdlib advisories the check runner reaches (ce1147c6f)
- Report which language people run Cmdr in, and whether covering a drive in phases delivers what it promises (c280f33d3,
  a25b5d207, 5d83a7bc4)

## [0.38.1] - 2026-08-12

- Double-mounted SMB volumes used to break stuff. Now they are not.
- Added some convenience buttons to error screens, with keyboard shortcuts!
- Some stability and UI fixes.

### Added

- Add ways out of every error screen: "Go to home folder", "Go back", ⌘D for technical details, and a Home command (⌘⇧H)
  (d25245d92)
- Tell agents over MCP when a folder size is still a lower bound, and show on-disk size where it differs a lot
  (70295b62e)

### Changed

- Stop calling the usage stats anonymous, in all 10 languages: they carry a random per-install id (412b70933)
- Rewrite the privacy policy to match what Cmdr actually collects, keeps, and shares (4c893e4a9, 811070948)
- Expire personal data in telemetry on a schedule instead of keeping it forever (e9f3465d2)

### Fixed

- Fix a share mounted twice breaking the panes and the volume switcher, and freezing the app on F6 (01f93aa49,
  1a9208e19, f535a18d9, 1e1e74d8f, d97e3a7fa)
- Fix ejecting one of a share's two mounts making the share vanish until the next launch (0b3a86adf, 35e42d893,
  b9678ceb4)
- Fix a dead NAS mount hanging every operation instead of handing over to the mount that still answers (aff924b12,
  fd04e8ce2, 140473b8e)
- Fix the panes and the volume picker freezing during a big transfer, when image-index queries took the whole thread
  pool (6566f8ccb, f89e8c1fe, 4401fcfb5, 9dde346b5)
- Fix a slow volume listing emptying the volume picker with no way back (7405b5341)
- Fix clicking Move on a slow share looking dead for minutes (cce94565d)
- Fix a big local folder sitting on "Opening folder…" instead of counting files as it loads (b9563bce8)
- Fix the Brief-mode cursor vanishing and every column filling the pane after a measurement blip (53e5c42ae, 85a09bc4b)
- Fix six error reasons missing their "Try again" button (f1226f020)
- Fix a browser download's final rename producing no toast, and Cmdr's own copies toasting behind a symlinked Downloads
  (6556e538b)
- Fix the Full-view scrollbar running up alongside the column headings (a62fefbd2)
- Fix screen readers announcing a file row that isn't on screen (b8486f709)
- Fix one purchase minting more than one set of license keys (9826d59f2)
- Fix a rejected license email passing for a sent one (b1649dc9d)

### Security

- Stop SMB account names riding along in error reports (14c6221d4)
- Make stored IP hashes one-way, and drop the IP from crash reports entirely (825e7c0bf)
- Stop blog likes storing recoverable IPs, and rate-limit the endpoint (92fc5e40d)
- Sign dev and production licenses with separate keys, moving the production signer out of a dev config file (21785cc27)
- Stop a page you merely visited publishing to the local dev blog editor (203c1ea18)

### Non-app

- Add `pnpm marketing:shots`: all eight brand masters, staged and frame-verified, in 22 seconds instead of half an
  evening (66a6eeb9f, 3350faf09, 109537fc5, 56c5adabd, a55384f6d, 13a4f5e75, 84ba857a3, 245627e49, b1f8f3305,
  7ec230784, 452534766)
- Regenerate the website hero with one command, from rectangles measured off the live DOM (dbfe84f10, aa6087dae,
  71183212a)
- Ship the brand masters as lossless WebP, a fifth of the bytes for identical pixels (7d38351dc, 36b5297f1)
- Give back ~30,000 CPU-seconds a month in the local check suite, and stop a docs-only pass re-running the Rust tests
  (8f3f5a765, cf839c390, a41573c35, 4b71a6f64, 6d8a287f0)
- Make `pnpm check` quiet by default, collapsing ~50 passing lines into one (ced502001)
- Log every failed and slow test individually, so flake and slowness rankings become a query (451615223, 739d980f9)
- Fix 107 broken Rust doc links, and deny every doc lint the project holds itself to (d10960dc0, 85faa8bef, ac38e32c9,
  aa706daee, 60c92c58c)
- Fail `pnpm check` on a RUSTSEC vulnerability in a crate we actually ship (5df72a533)
- Split the Worker's 40 flat files into four self-describing areas, each with its own docs (5c12e78b0, 8cc6122b1)
- Bring 13 oversized `CLAUDE.md`s back under the resident-doc budget, and move the project's hard rules into `AGENTS.md`
  (1245ca052, 2429c1ad9, 3be1b799c, 39b2f240a, f16e5a5d9, 6b0df9edb, 15288e463, 571f2c97c)
- Mirror the Linux volume module and the MTP backend on their macOS and SMB siblings, so cross-platform drift is visible
  (0d836f840, eb3e073ec)
- Take the operation-session plan through four review rounds before any code (cf8ca51e3, 63cf6b523, 4280431f3,
  84c21d709, 2e1eff515)
- Prepare the AlternativeTo listing to paste, describing today's app with today's screenshots (039e17c67)

## [0.38.0] - 2026-08-11

The three main advancements:

- Search now works regardless of indexing.
- Backgrounded operations look and work better now.
- A ton of stability fixes and resource use improvements around indexing and otherwise.

### Added

- Search a folder Cmdr hasn't indexed yet: it walks the drive live, streams matches as they're found, says which wait
  you're in, offers the permission for a folder macOS refused, and keeps walking when you send the results to a pane
  (b3fd1f9ac, 11baa340f, 1ade70f40, d4c2abaf7, 78bcbd41b, 45096d959, 75d011fc7, 5bfb8c2f4, b49f8f75f, e846c0cec,
  dfdd7ab0e, e72322ff2, efdeef061, bf0c546b7, 4c2415763)
- Scope a search to the current folder (the new default) or the whole volume, with ⌥C and ⌥V (b7ccec1ae)
- Add a corner chip in the main window for a backgrounded operation, with its progress and one click to the queue
  (a447baa7f, 221c84dd0)
- Open the operation queue with ⌥⌘Q from View, renamed from "Transfer queue" since it holds deletes, renames, and
  archive edits too (2a52e4f3a, 10110bf32)
- Keep a failed background operation and its reason until you dismiss it, in the queue and as a toast in the main window
  (8b97ed94b, c53a9961a, 0eb03c895, d592d0475)
- Ask before quitting with a transfer in flight, and clear away whatever it left half-written (fe2cb8250, f5c5e2b52,
  20a1022ee, 6819a0661)
- Roll back a copy from the Transfers window (7a1e1c3c5)
- Resize dialogs from any edge, and grow every dialog that shows a path (d9de60c00, e7b4871a8)
- Hover any shortened path or label to see the whole thing (3270e3fb1)
- Copy the current folder's path with ⌃⌘C on the `..` row, with a toast showing what landed on the clipboard (e883b5283)
- Ask agents where your disk space is going: one indexed listing tool with size ranking, paging, and honest coverage
  (45365c160)
- Agree to the terms during onboarding, instead of consent being assumed from the download (7a166919a)

### Changed

- Rewrite the terms of service: 3,423 words down to 2,203, accurate about what Cmdr actually does, and each release now
  converts to AGPL three years after it ships rather than every version on one shared date (248cfc630, 57249e41f,
  8af247059, 69fa0f52b, 1311d8a0c, 3ba6b4bcf)
- Cut about 97 MB off peak memory during a search (d75453b86)
- Stop a dotfile write in your home folder rescoring the whole drive: 5.25 s per pass becomes 2.1 ms (0271855aa)
- Stop an idle machine rewriting 51,081 folder-ranking rows a minute for a result that didn't change (234bd2aec)
- Halve the live index write cost by committing a burst of changes once instead of once per file (3313aabfd)
- Stop asking iCloud and Dropbox about folders that hold no cloud files (745745166)
- Cut roughly 9,400 log lines an hour about the things that are always fine (5431df6bd, 6834c4de3, 1354ab0b5, b67ff311d)
- Stop an idle NAS connection filling the log with packet traces (e24382ab8)
- Show the same honest readout in the Transfers window as in the copy dialog: both bars labelled, percentages, speed,
  and a time left that doesn't shift the layout (b4884f2ba, 7a1e1c3c5, 442dc733f)
- Open What's new on the headlines, with each release's details behind a Show more (d6d14da9d)
- Ask the copy conflict question in a card you can read at a glance (ec81db88a)
- Give the whole app one line-height scale, so text spacing stops varying screen by screen (22fb4bbfe)
- Say "Background" rather than "Queue" on the progress dialog when there's no queue to join (8441b4e2e)
- Stop the write-error dialogs saying "failed" at you (19ecb6aab)

### Fixed

- Fix a folder move on a phone destroying the child you chose to keep (56047e435, 3971e86ef)
- Fix a folder move to a NAS deleting the local files you chose to skip, and the conflict dialog reporting a real file
  as 0 bytes (b84a6f861)
- Fix copying a local folder to a NAS or phone failing outright, and a failed folder copy wiping the destination folder
  (7046e9dbb, be819a3ee)
- Stop a force-quit or a crash mid-copy leaving a truncated file wearing your real filename, on every drive (a19325c9c,
  06837bc63)
- Fix a scan preview authorizing an operation on a different selection (1e75af281)
- Fix a delete guessing a folder was a file when its details couldn't be read (0cabe9f0c)
- Stop two disks being handed one identity, which could route reads and file operations to the wrong drive (181c2b71c,
  a3c6684ef)
- Fix folder navigation stalling up to a second while the file watcher armed (0141b744d)
- Fix ⌘- and ⌘+ freezing the app for up to 46 seconds while fonts were measured, and non-Latin names staying at an
  estimated width in Brief mode (62e69c3ea, bb781c127)
- Fix a folder replaced outside Cmdr leaving ghost files in the pane (4b633dfc0)
- Fix copying to a NAS dying on a filename with a `?` or a quote in it (9536ea44d)
- Name the file that actually failed in a transfer, instead of the folder you selected, and say which file the server
  refused the name of (7f4b50eca, 4062288bc, 119cdb8df)
- Stop blaming macOS for a folder your file server refused, and drop the Full Disk Access prompt that couldn't help
  (c912c8d30, f37fe1245)
- Fix a pane stranding itself on "Path not found" after its network drive disappeared (b314f8140)
- Fix Quick Look and dragging files out doing nothing on a direct-SMB pane (230ff5866, fea262838)
- Fix a burst of changes on a phone pegging a core and freezing the pane (63d7e0e20, 1b4e667f3)
- Fix a backgrounded transfer's conflict question never reaching you (d744a61a4)
- Fix a search from your home folder reporting that Cmdr doesn't cover it (6d3d7abbf)
- Fix a folder-size filter answering from a ranked sample, so a 1.7 TB folder stops going missing, and add sorting by
  size or date (7ee6b639d)
- Stop search presenting a filtered count as the whole truth (37e9c9312)
- Fix a file with a newline in its name being unfindable, in search, selection, and excludes (d0b63a13d, b71cef17a,
  b73fa8e3d, 15e444c7c)
- Fix dialog text sitting further in than its own title, and long paths escaping the panel (555be0946, 63be2dc3d)
- Fix a listing on a network share passing as fresh when another machine had already changed it (f0b139af7, 23f532816,
  28431ec1f)
- Stop a NAS getting blamed for the seconds Cmdr itself spent frozen (608b1dd1b)
- Fix network and external drives skipping the per-navigation self-heal that local browsing gets (76863f0f4)
- Fix a dead mount stalling launch while Cmdr swept last session's leftovers (e063f793d)

### Security

- Stop a saved cloud AI key ever coming back out of the OS secret store (4f32fe913)
- Close a bypass that served the private analytics dashboard without authentication (79969dffc)
- Rate-limit every public ingest endpoint, with a global ceiling so an error-report flood can't drown the channel or
  delete real reports (037186a20, 763b84757, f81696061, 67c125f28, 7a0236425)
- Clear the RUSTSEC-2026-0221 unsoundness advisory (63a0858f3)

### Non-app

- Extract the archive backend into its own crate behind named seams, so a future filesystem backend can be written
  without reaching into the app (6d435cdf7, e2be3721b, d5ab81b01, 4f3360d85, fe33825a8, 3f11fea44, 2cb09848d, 057cc9e64)
- Add a Cmdr IntelliJ plugin: message keys fold to their English text, and a changelog hash ⌘-clicks to its commit
  (5eeff67db, 8fc5ffc8d, bd3377606, 64483b366, c623e589c, 875f6d545)
- Regenerate the translator screenshot set at 131 surfaces, framed on their subject, with blank captures now detectable
  instead of shipping silently (5d48b702b, b03fb4c7d, aaf7eeb20, 8d03bc65f, d1e95c2c7, 2c117eba0, cbefd5147)
- Store changelog commit refs as bare hashes, dropping 89 KB of URL boilerplate (826948365, c9e70ebdd, 35584706f,
  4adbe77fc, 96620f4d4, ab3223399)
- Cut debug builds 31% smaller and 35% faster by dropping variable-level debug info (ba87ee1b9)
- Give test fixtures a scratch directory that can't collide with another run, across 106 fixtures in 51 files
  (9ac788e6a, af28731a9, 25a29f4f2, 8f6196907)
- Split 20 oversized files along seams the code already had, shrinking the length allowlist instead of growing it
  (feb065de1, e5ea10d02, 7a45a777d, 1a7514a87, b98e3484f, fd17b768a)
- Add ESLint, Stylelint, and knip to the analytics dashboard, and wire them into `pnpm check dashboard` (21627bb61,
  030db66dc, 1da89169a, dc758cd55, 026b7c70c)
- Make every E2E spec put the shared fixture tree back, so one spec can't fail the one behind it (48c0a0f4b, 2cd24c704,
  b1dbd074c, f98ede590)
- Add two reusable instruments for resource questions: a churn baseline harness and an index size probe (38d391d8d,
  5dc39c44e, d74b558a4)

## [0.37.0] - 2026-08-03

Highlights:

- Search dialog redesign and major speedup: looks a lot nicer, and even broad queries went down from taking 12 sec to
  0.5 sec.
- A bunch of fixes to AI bulk rename. Now it works with hundreds of files. But still alpha.
- A lot of SMB copy/move improvements incl. a 3x speedup for small files!

### Added

- Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a43, 2d41cc148, 18add0b0d,
  42f76971a, ede1a7d6b, 84e5f3a58)
- Add right-click Cut / Copy / Paste / Select all in every text field (fd6fc2933)
- Add a "Chat memory size" setting: Automatic, or 16,000 up to 200,000 tokens (751214194, 14aacf89f)
- Show how full the chat is in the Ask Cmdr rail, with a fill bar and the real token counts (0b6efe952)
- Add Undo for a batch rename, per batch and across a whole multi-batch run (dcc14c131, 032722e14, c528ea8fb, dada4beaf,
  e301c1e49, 0b25450b2)
- Show every file and the evidence behind its proposed name in the rename review, and let you fix a name in place
  (56788bdc0, 766c3ebb2, 64b8022e0, b456a3651, 0423a7977, 7fc00aad7)
- Refuse a rename plan whose content-derived names Cmdr can't verify the model actually read (285af99f5, 52eeb3089,
  0b6198287, fb60f1083, 6b0f066eb)
- Move recent searches into the query field as a dropdown, each row showing its age, result count, and filters
  (503e8443f)
- Show transfer speed in the Transfers window (821307e0b)
- Say when a transfer has stopped moving and what it's waiting on, instead of a confident ETA that isn't true
  (066796c75, 43c106cfd, a2070fa7f, a77bf8326)

### Changed

- Return a broad search in under half a second instead of twelve (777c32c33, f35917902)
- Open the search dialog and start typing without waiting on a full index rebuild (1b8557a78)
- Stop a cold NAS index freezing the search dialog, and search one NAS once rather than twice (2890fe331, 5b3b6da23,
  9c55b0de9)
- Redesign the Search and Select dialog into a real Cmdr dialog: house chrome, a 2×2 query block, one surface per zone,
  and a Path column with its width back (2643f746f, 4a1ae52b8, 6c036b1ff, af8379856, d97f6f6df, bf6354f23, b44377215,
  c0c6f2277, 24693d49e, 66412c269, a44787089, 44295a786, acdaa9454, 0a5170e1d)
- Give every text field one look: 8px corners, an accent caret, and a solid focus ring (0394c062f, d7a7179f2, a40d2c26d)
- Copy a small file to a network drive in one round trip instead of two (17d8a6b85)
- Skip the per-file destination check when copying into a folder Cmdr just made, 2.1–3.8x faster on a NAS (20d6fead0,
  c4bc6cec4)
- Make the SMB concurrency setting do what it promises, worth 25% on an 8-core Mac (9f3d5a7f4)
- Recover from a network drive that goes silent in 50 seconds instead of hanging forever (560721b82, b995f699f,
  80bc07bfc, aa5e7b26f, 0c5f6cd21)
- Retry one file after a transport blip rather than ending the whole transfer (6a799377f, 9b8ef8a00, 98f818e71)
- Survive a network-drive upgrade landing mid-copy, and stop re-connecting a share that's already connected (402e9b73b,
  7de5961b7, 1685795fa)
- Cap the app's whole database page cache at 64 MiB, whatever the connection count (a780954ea, 2d8b6bb22)
- Stop reopening a database connection on every pane switch (928709f4f)
- Cut cloud sync badges from 300 thread creations a minute to none (852e79915)
- Stop a two-folder change rescoring 90,000 folders every minute, and rescore only what changed (914004f14, 555d75fe6,
  04e9ee406, 4c3a794b1)
- Show the main window a second sooner at startup (5ae724a99)
- Hide Cmdr's and other apps' temporary save files from the pane, on every drive (66e60c3b2, ca2a63718)
- Render every size, speed, and ETA from one implementation, so two windows can't disagree (9cab4e039)
- Settle one rule for the `…` menu suffix, and give Compress its icon (499ab1497, b04ac721b)

### Fixed

- Fix ⌘V pasting twice in dialogs (9b3521235)
- Fix ⌥⌘A opening Ask Cmdr and selecting every file at once, plus four default shortcuts that never fired (0919a6e1e,
  f69cca281, 4beb91595, 4ead5e040)
- Fix the whole UI freezing when you resize the window (ceeddc43a)
- Keep a maximized window's position through a restart (2b7bec842)
- Fix six windows rendering every setting at its default, including binary sizes in the Transfers window (0d64c84c3,
  a434418aa)
- Stop a killed transfer leaving a truncated file wearing your filename (b889e065c)
- Make Cancel and Rollback work on a stalled transfer, instead of leaving force-quit as the only way out (78ef454a4)
- Show size and date again for folders on network drives, in archives, and on phones (a0ab6ff8c)
- Keep the Size column readable on a reconnected drive (bdd3ccbc9)
- Retry a first network-drive connect that never reached the server (07b011428)
- Stop Ask Cmdr inventing names for screenshots whose contents it never received (01b3f2dc3, 5d95d588d, 9496fcc61)
- Fix the agent reading the wrong files on a scrolled pane (b2f066e37, 551795b24)
- Let a local-model user get an answer on default settings (02ece3aee)
- Stop a batch-rename undo restoring an impostor file over your original (d300f598a)
- Keep folder importance and image indexing following the drive index after a full scan (88e07ff68)

### Non-app

- Extract the drive, media, and importance indexes into `cmdr-index` and the filesystem vocabulary into `cmdr-fs`: 93k
  lines behind a compiler-enforced boundary, with the index's inner build loop 6–9x faster (944b0bc25, 4323505fb,
  913e43fb0, de9ff9f88, a647efa39, 42f13a4f1, e10812200)
- Run every Rust check over the whole workspace, so a crate can't hide from the tests, the linter, or the license policy
  (cba34d778, beb2dd509, bdc3849a6, 8115e928c, 65fb5603a)
- Re-run a red Rust suite's failures alone before believing them, so machine contention stops reading as a defect
  (9e14ae574, 32aa63222, c92e05719, 736df70b4)
- Break `FilePane.svelte` into tested controller modules, 2,733 lines down to 1,971 (0a29a457f, 89493b331, bae466f92,
  1dfb350f7, c1dc27b72, 766a80a22, d33ce6eae, 36d10e42e, 0e1fb67e2)
- Add a rustdoc lane that catches broken doc links, and repair the 32 it found (4092256c0)
- Serve the current release from one download URL that never goes stale (8b9947c28)
- Keep app-directory listings in the repo, one file per directory (fbfea9a6b, 68e6d4d0d, 9daa96de8)
- Move roadmap items into one typed data file (2a669f4af)

## [0.36.2] - 2026-07-28

The highlights:

- A 5x drive reindexing speedup!
- Very significant RAM use optimizations, incl. a 85% decrease for search.
- Also some convenience fixes in the renaming flow.

### Changed

- Make checking a drive for changes dramatically faster on macOS (it used to take ~20 minutes on a boot disk)
  (cbde9d89e)
- Cut search's resident folder-ranking memory by 85% on a NAS-sized drive, 58 MB down to 9 MB (68237a9bb)
- Cut background folder scoring's memory by two thirds on a NAS-sized drive, 256 MB down to 84 MB (0c5296dd0, 5b91dadc6)
- Say "Checking for changes" when Cmdr is checking a drive rather than rebuilding its index, and time each kind of scan
  separately so the estimate is right (022a6160b)
- Show one download toast and one macOS notification per burst, always naming the newest file (6a8bf70e0, 81d69c64a)
- Rewrite the drive-indexing settings copy in all nine languages so it reads natively (b3ab11f8e)

### Fixed

- Fix clicking or dragging inside the rename field cancelling the rename, and make clicking away save, like Finder
  (d8dcb89c1)
- Show the server's own reason when an error report fails to send (081830744)

### Non-app

- Add doc checks that catch pointers at headings and source files that no longer exist, then repair the 75 they found
  (c66b02e1a, 8cb06924b, d2e8b0f23, 06b3e5045, 5944cb364)
- Split the folder-importance and image-index docs so each area carries its own guardrails instead of one shared ceiling
  (0b5f15fc4, 5d6ca8224, 79720e7df)
- Replace rotting file inventories across a dozen module docs with the layout facts codegraph can't answer (b2e7abebd,
  1f5eef348, 28b61d5e4, fd2d6ab61)
- Plan the index-crate extraction, giving the hardest 28% of the Rust a real boundary (dd98eb573, f76cae0dd, 5ef7c4e31)
- Catalog the 19 flaky tests that make a red `rust-tests` run meaningless (fd4799888)
- Fix the Linux build breaking on a macOS-only dependency that wasn't declared as one (aabc4cb11)

## [0.36.1] - 2026-07-25

Cmdr could balloon to tens of gigabytes of memory within minutes of launching; that's fixed. Network drives stop
hoarding NAS snapshot folders in the index, and switching drive indexing off now really stops every drive.

### Fixed

- Fix Cmdr ballooning to tens of gigabytes of memory shortly after launch (28967ce87, fb874666f, 89d2befbb)
- Fix a network drive re-indexing itself at every launch while drive indexing is off (272276699)
- Keep NAS snapshot folders out of the drive index, so a share's file count and size are real (1046a5d25, 6d260ebaa,
  c802ed913, 85c3777ee)
- Fix error reports failing to send when you leave the note empty (326ccf12e, b3fd57d30)
- Drop "Send error report…" from the toast that reports a failed send (22001eed9)

### Changed

- Cut memory for a big network drive's folder tree by two thirds, and load it three times faster (0f6663b87)
- Grey out the per-drive indexing controls while the main drive-indexing switch is off (50b0cb5d4)
- Make the memory safeguard measure Cmdr's real memory use and keep watching after it steps in (7230e86f3, f76319997)

### Non-app

- Document that `vmmap` reports Cmdr's Rust heap under a GPU name, and how to measure memory properly (b4b2c1757,
  76bafde16)
- Add a test helper that proves a hot path doesn't allocate per row (f3e764194)

## [0.36.0] - 2026-07-24

The three main advancements:

1. A very nice design facelift
2. Photo search stays fast even with 2M+ images, reports status+progress nicely. SMB connections stay nice and
   responsive during indexing.
3. More RAM+CPU+disk use streamlining

### Added

- Keep photo search fast past 50,000 images with an on-device index that activates automatically at scale (c4f65cbff)
- Show image-index status right on the icons: a per-file badge, per-folder coverage, and a per-drive dot (8a8ceb761,
  fab1f8ac0, 66fe72e6a, c101c1962)
- Manage image indexing from three focused settings cards, switch semantic search on or off, and reclaim the model's
  disk with Delete model (8c31e9c64, 52ea7bf8b)
- Speed up image indexing with a Parallel workers slider, from one to your machine's CPU count (46bd40d56, 23c10d6ef)

### Changed

- Give file transfers and browsing priority over background indexing, and pause background uploads while you browse the
  same share (e00321e6a, 2d83ea8dc)
- Index a NAS about 3.8x faster by spreading the scan across multiple SMB connections (f482daa97, 2a803d5e5)
- Read NAS image bytes in parallel over multiple connections while indexing (fdf5832aa, fc0c79fd3)
- Shrink the image-search model download by about a third, reclaim ~550 MB after it installs, and roughly halve
  per-image disk and search memory (d91d92bb8, eb5b629c4)
- Refresh the app toward a native macOS feel: rounder dialogs, capsule buttons, redesigned Copy/Move and Delete dialogs,
  squared-off tabs, inset file panes, and right-aligned Size and Modified columns (cae582c77)
- Match each settings control to its job: segmented toggles for quick switches, drag-only sliders for coarse values, and
  framed number fields elsewhere (99ce8537c, 088fdc386, 95c8fd348)
- Reorganize Settings into Indexing and Notifications, with a leaner AI, Behavior, and Advanced (f03119c1c, 8f1e470b3)
- Redesign the Ask Cmdr rename review to read as a calm, native rename list (6dcc6f6e6, c57747272)
- Update the low-disk-space warning live and clear it once you free space (69b0146b2)
- Stop a single churning folder or an app updater's throwaway files from dominating indexing CPU (d33f8ea69, 7b7c7b28c)
- Keep the search-index journal files small instead of letting them grow to the size of the database (c1e167d0a)

### Fixed

- Stop the app crashing when you close Settings or the file viewer (72dfeb58d, 8713f797d)
- Fix NAS image indexing stalling at zero images, and skip an unreadable file instead of pausing the whole pass
  (bb714a851, a8b063dde)
- Stop Ask Cmdr reporting a fully-indexed folder as not indexed (cb0b26856)
- Fix a rare blank window on cold launch (d24cc367f)
- Fix three messages that rendered a code fragment or dropped their text: the license verify banner, the file-viewer
  type warning, and the MTP conflict dialog (c1267c5a2, 5a60f21dd, 67dba4e17)
- Keep per-folder image counts accurate right after you delete files (7e9837ef1)
- Keep the Settings and Debug sidebars from sliding under the traffic-light buttons (7d9651bc2)
- Stop deduplicated hardlinks from being rewritten on every index pass (0932ea3e9)
- Stop a resting folder from holding the whole disk under a "size updating" indicator (041083149)
- Give every switch and checkbox a real name and role for screen readers (53a0e8f8d, c3fbc4e11, 1a71baeaa)
- Restore the missing apostrophes in the French drive-index tooltip (47e9c79ee)

### Non-app

- Kill the Rust test-synchronization flake class: one canonical wait helper, RAII isolation guards for shared globals,
  real completion counters, and a lint that fails on fixed sleeps (14d1a5a47, cd4378852, 7013648b9, 97819d466,
  ae57706d4, 814629a97)
- Reorganize the indexing subsystem into 13 stage-based areas, each with colocated docs (ff63e6df3, 47624e9be,
  840b31ad6, e67ef2809)
- Add Debug > Soft dialogs, a gallery that opens every dialog on demand for design review (43f1f51f3, 656f24bfd,
  996aef5df, bcc8bd277)
- Enforce house UI primitives and add a building-UI guide so new work stops reinventing controls (67e2f03d2, 4c2cf75b7)
- Reference docs by bare path with dead-link checks, plus per-doc read/write usage stats (df6637869, ee010a2b4)
- Overhaul indexing logs: report reconcile churn by trend, attribute writer waits, and truncate the WAL during long
  replays (96297bc4c, e5b6abed4, 08e45f103, 65e7beffe)
- Make signal-crash reports group across installs and become symbolicatable by recording the image base (cca5011cf)
- Stop dev builds writing ~1 GB of dead WebKit cache (eee12e07e)
- Benchmark the NAS scan and add a reliable CPU/RSS sampler (e740933a2, c4baeb266)

## [0.35.0] - 2026-07-21

The three major things:

1. A lot of CPU+RAM use improvements
2. Bulk rename with natural language, with an approve/reject UI
3. Manually configurable image-indexed instead of a slider

### Added

- Rename a batch of files by describing what you want: Ask Cmdr proposes each new name, you allow or deny row by row,
  and it applies as one undoable operation (df87ec519, d6c4cff62, 7e63b4c68, c1654a3c6)
- Pick which folders get image-indexed in Settings › AI › Image search, or add one from its right-click menu; each pane
  says whether the folder you're in is covered (51995f4a6, f6168b22b)

### Changed

- Browse a NAS at full speed while it's being indexed or copied from: opening a folder mid-scan dropped from 10.7 s to
  instant (6d9df62d7)
- Read MTP archives about 35% faster: a ranged read now costs one USB round trip instead of three (8e9efebea)
- Stop rescanning the boot disk roughly ten times a day when macOS loses track; it sweeps once daily and counts what it
  coalesced (49da9914e)
- Name the photo tools in Ask Cmdr's tool line instead of showing "Working" (f629af773)

### Fixed

- Fix a fresh scan silently dropping your biggest folders: one run lost 661,411 entries across five directories whose
  only fault was size (44cf3b746)
- Fix a rescan refusing a large healthy folder while walking browser caches instead (596390b53, c67e1f2c0)
- Stop one pathological subtree eating a whole rescan, and cap the cost of a cold start behind a huge temp folder
  (90e4784c2, 8034df2e1)
- Fix a rename replacing a file created in the moment between the safety check and the rename itself (8cb4e0738)
- Make two Cmdr processes on one data dir impossible, which was corrupting the index (af9bed4d6, 1f96465ec)
- Stop a slow file-event replay cascading into a forced full scan (e9b10c32c, 6c35074cc)
- Repair folder sizes after a failed write instead of dropping them, and heal a drifted entry counter (765d49d8d,
  f6fe1db2e, 1abad37bd)
- Fix an interrupted rescan leaving folders claiming exact sizes they can't know (bed0e9368)
- Keep the size of a folder named `dev`; only real Unix roots are skipped now (79b58b21c)
- Recover on your own from a dead phone session instead of needing a replug (cb612afe9, e5f9e0235)
- Fix a phone's index drifting after an upload, rename, move, or delete (b47fee96e)
- Stop a slow phone wedging mid-operation (f69c930c4)
- Wake the phone watcher only for actual phones, not every USB event (e65980a7c)
- Hard-wrap unbreakable lines in the file viewer and fix word-wrap scroll drift (7822523b7, 2602a29c3)
- Keep the app up when a git portal refreshes, and group crashes by the code that broke (8f4e64115)
- Explain skipped rescans in the drive tooltip, in all ten languages (46809911b)
- Stop a superseded listing reporting progress against an id the pane already retired (b6ede325b)

### Non-app

- Re-translate the newest three features across all nine non-English locales, fixing a European Portuguese leak and term
  drift in every language (0582089aa)
- Restore the Linux build, which `#![deny(unused)]` had been failing since the instance lock landed (40ed49463)
- Cut recurring secret-scanning and Renovate noise at the source (4b4c051d7, c129fba56)
- Convert `apps/desktop/scripts/` to TypeScript and run every script through one runner (a5890bf7a, 7f7d3109d,
  2b36ddd28)
- Measure the indexing hot paths and record what three spikes actually found, including the rule they refuted
  (aa7656d01, ec948e23b, 5665a049b, 403a35e5b)

## [0.34.0] - 2026-07-18

1. Ask Cmdr: a built-in AI assistant that has access to your files to answer questions (alpha, read-only).
2. Image index: Find your photos via natural language, by describing the stuff / text (OCR) inside them, fully
   on-device.
3. Operation log: Review and roll back (undo) any non-destructive file change.

Plus MCP improvements and tons of bug fixes incl. CPU use improvements.

### Added

- Add Ask Cmdr, a built-in AI assistant that answers questions about your files (alpha): saved sessions, cross-thread
  search, file attachments, and visible cost (46f5719e0, 196188279, c067693b4, 6a2ec7b9d)
- Find your photos by description, by the text inside them, or by tag, and find visually similar images, all on-device
  (0b6e164bd, 8efd2bf24, 3a43ff32e)
- Control image indexing under AI › Image search: a depth slider, live progress and ETA, opt-in for other drives, and
  reclaimed disk space when you narrow it (6b56d1951, bf2ffe5d2, e293d63f8, 471134761, 5bb09aab3, ed1c660fa)
- Review and roll back any file change from the new Operation log (View menu, ⌘⌥L); alpha (39282ade1, 4cfde6e55)
- Compress files into a new zip from the Transfer dialog, with a compression-level slider, a live size estimate, and
  SMB/MTP destinations (cea1631b4, 0dddc0e81, 50aeda6cd)
- Open password-protected 7z and WinZip-AES encrypted zips (506d07f3e)
- Index local external drives (USB sticks, SD cards, extra disks) so search covers them too (53e52e8bb, 03497f8cb)
- Search every indexed volume at once, not just the current drive, and answer "how many" with a count instead of a full
  list (a141cc24d, 4426bffc1)
- Let external AI agents rename, create, tag, favorite, eject, pick trash-vs-delete, drive the operation queue, and read
  indexing status and folder importance over MCP (e59f95af0, a723a623b, d8c6dc3c4)
- See what's new after an update in a redesigned popup, and open the full changelog from Cmdr › Changelog… (bacc1d9e3)

### Changed

- Scan local drives up to 2.8x faster with a bulk-stat walker (b2b30ac45, d6185b65c)
- Use less CPU and memory while indexing: lower-priority background threads, throttled live writes, and a lighter file
  list (2671c00d7, 846cf21a9, 6032386da)
- Auto-reconnect an SMB share and resume its index after a disconnect or restart (499027b55)

### Fixed

- Keep folder sizes accurate after big deletes and creates; existing drives repair drifted sizes automatically on the
  next launch (3d13ab7de, acefb9a6d)
- Show an honest "indexing stopped" badge when a drive's index fails, instead of spinning silently (1cc608562)
- Refresh Google Drive panes live after rename, create, and delete (b0ccff5e0)
- Never freeze at launch or stall state reads on a hung network mount (cb669ab29)
- Fix scoped search silently returning nothing for `/tmp` and `/var` paths (1a448f175)
- Fix scoped filename search returning no results on a NAS (bdcdb7ab5)
- Match image tag search regardless of case (092696aa2)
- Stop MCP-created or moved files landing in the wrong pane (2125b5225)
- Prompt for SMB credentials instead of falling back to the CLI when a share rejects guest access (9cbd0277e)

### Non-app

- Split the largest source files into cohesive modules for maintainability (0cf01dc05)
- Add Ask Cmdr and the Operation log to the website features page (14c3d2300, 6735cc6d2)
- Bump `spin` off the yanked 0.9.8 (5eb645bfc)

## [0.33.0] - 2026-07-09

**Archives open like folders, the clipboard pastes straight into files, and search shows your best files first.**

1. Browse and edit `.zip`, `.tar`, `.tar.gz`, and `.7z` archives like folders, even ones on a phone or SMB share.
2. Paste text or an image straight into a new file with ⌘V.
3. Search ranks your most interesting files first.

### Added

- Browse, extract, edit archives: `.zip`, `.tar`, `.tar.gz`, and `.7z`. Fully edit zips (create, rename, delete, and
  move files inside), unlock password-protected zips, even on SMB and MTP (179466f8b, 8e15d86b8, f4fa09a4b, 8d80f0122,
  2103b2fa9, 778dddfde, 8e001cb93, e85cc448b, f5c975115, 5efe4ba1e, 82f394618, 54d208511)
- Paste clipboard text, an image, or a PDF as a file into the current folder with ⌘V (b0de3824f)
- Add a folder-importance subsystem: a tunable scoring API any expensive feature can consume, with a measured eval suite
  for weight tuning (08d1d6dcf, a435fb395, 513ff76bb, 60fd27df0, 02d156c33)

### Changed

- Rank your more interesting files higher in search results (1a998e478)
- Extract large folders from compressed `.tar` and solid `.7z` archives much faster (be11894e4)

### Fixed

- Fix a moved or deleted file lingering in the source pane on MTP devices until manual refresh (bd8dc8de8)
- Keep the inline rename box on the right file when the folder reorders underneath it (5e5ee92d5)
- Stop a background folder load in one pane from clearing the other pane's messages (d22ab4c92)
- Say "not supported" instead of "damaged archive" when a `.7z` uses encryption Cmdr can't read (b7ae624ed)

### Non-app

- Upgrade the website to Astro 7 for even more Rust! (81d0f5758)
- Consume smb2 0.12.0 from crates.io, dropping the local FileReader patch (481d68345)

## [0.32.0] - 2026-07-01

**Design polish across the app, plus filesystem-aware copies.**

1. Refreshed colors, icons, dropdowns, and text alignment across the app.
2. Every volume shows its filesystem: APFS, exFAT, FAT32, and more.
3. Copying a file over 4 GB to a FAT32 drive is blocked before it fails.

### Added

- Block copying or moving a file too large for the destination drive (f177b6048, e0450ca8b)
- Show each real volume's filesystem (APFS, exFAT, FAT32, ext4, etc.) in the volume picker (c34d10de7)

### Changed

- Redesign every modal dialog to the macOS layout: left-aligned titles and text, right-aligned buttons, labeled Action
  and Route rows, folder and file icons, and tooltips on the scan status icons (95191e2e5, b19ccb459)
- Make the Copy/Move destination box forgiving: accept `~` and `~/…`, show the home folder in full, and create a missing
  destination folder on confirm on every drive (b19ccb459)
- Replace UI emoji with themeable Lucide icons across dialogs, menus, settings, and the network browser for sharper
  contrast in light and dark (ffd03c908, 5baba851c, 48f3561f5)
- Redesign the Select dropdown as a native macOS pop-up button with a frosted-glass menu that opens over the trigger
  (643f4200e)
- Brighten dark-mode secondary text and the selection red for clearer readability (bbe295818)

### Fixed

- Fix the Homebrew install silently failing for new users on Homebrew 6 (now runs the required tap-trust step), and stop
  onboarding text showing literal `&gt;` and `&amp;` (dbceea714)

### Non-app

- Enforce an APCA Lc-45 contrast floor alongside WCAG 2.2 AA, clearing the last low-contrast spots so every text pair
  passes both (f6ccc1889, e28b9a0ed, cf33ac826)
- Add the full Tailwind v4 OKLCH color scale as reusable design tokens (3a2809b29)
- Upgrade the Node toolchain to 26 and relax the dependency cooldown to 3 days (4c5ef4831, 714caeaea)

## [0.31.0] - 2026-06-30

**Finder color tags, a nicer indexing UI, and much faster network and phone scans.**

1. See and set macOS Finder color tags right from Cmdr.
2. A refreshed drive-indexing UI with live folder sizes during scans.
3. Network and phone scans finish several times faster than before.

### Added

- Add macOS Finder colored tags (86a9ca38b, 6039e1a65, 4d87d4ec5)
- Show a per-drive indexing checklist (find files, save list, compute sizes, catch up) with live counts and a per-step
  ETA (4a74312f0, a92f9cdb3, 138bdfa88, 519a27eac)

### Changed

- Show folder sizes growing live during a network drive or phone scan, not only on the local disk (5a86abaf6, ee9ee757e)
- Speed up network and phone drive scans by listing directories concurrently, dropping long scans from minutes to
  seconds (a003f004e, 6518b5656)

### Fixed

- Fix the indexing progress counter freezing mid-scan, making a healthy scan look stuck (7568931c9)
- Fix one drive's scan lighting the size-updating hourglass on folders of every drive (d4105d98c)
- Fix a failed local scan sticking on a spinner instead of offering a rescan (61c66a0c8)
- Fix a network first scan stalling for hours on NAS snapshot folders (bb64ad38b)
- Fix a reindex wedging on a large set of changes (12e98e520, e4e13ed95)
- Fix folder size totals double-counting hardlinked files during a rescan (ca4151e67)
- Fix search, Go to path, and AI navigation sometimes opening a path on the wrong drive (3024839e4, b029c435c,
  ab44a7226, f6e93c239)
- Stop the file index wasting disk space after a version upgrade (1536d3079)
- Fix the file viewer failing to load lines when a file's line count is still unknown (83ad3cebb)

### Non-app

- Refactor navigation onto a first-class (volume, path) Location type, deleting bare-path navigation (bb6ef69c7,
  3eabcec58, e2f4e6017, 0d189b230)
- Bump smb2 to 0.11.4, demoting per-frame SMB protocol logs to TRACE (676e24b91, 5e6d163e0)

## [0.30.0] - 2026-06-28

**Live folder sizes, browse while transfers run, and smoother mouse navigation.**

1. Watch folder sizes fill in while indexing runs.
2. Browse your phone while a copy, move, or delete is underway.
3. Smoother mouse-driven navigation, plus faster network-drive rescans.

### Added

- MTP: Browse a phone while a transfer runs (06d1874d5, 4a01ad7f6, f002606db, edc89aa28)
- Navigate pane history with mouse's back/forward side buttons (fcf34143e)
- Click breadcrumb segments to jump to any ancestor folder, double-click empty pane spce to go to parent (dcc5b2e7d)
- Explain why a phone's folders add up to less than its used space (caedb6558)

### Changed

- Show folder sizes while indexing: ≥lower-bound when partially scanned, also unknown and stale (494849a93, d9dbf0769,
  c4b20c962, fdadfc8f4, 9f318e74a)
- Speed up SMB and MTP rescans: update in place and keep last-known sizes visible while scanning (a6a2f5869)
- Stop showing indexing notif and free space for DMGs, and show the read-only lock for read-only mounts (889859c40,
  1ea486345)

### Fixed

- Fix progress bars for cross-volume folder copy and move (38c405ec5)
- Fix a UI freeze when starting a manual rescan (880688c92)
- Fix enabling or rescanning an SMB share or MTP device indexing nothing (d45275753, a8007894a)
- Show the indexing indicator for SMB and MTP drives, not just the local disk (ef6005d42)
- Keep an honest stale index when a drive disconnects mid-scan instead of marking it complete (4d66beb0f)
- Rebuild falsely-complete network indexes from earlier builds on upgrade, no manual action (3109ab698)
- Detect and explain Linux MTP permission denials from missing udev rules (51eee35db)

### Security

- Patch quinn-proto (remote memory exhaustion) and memmap2 advisories (584aa27fb)

### Non-app

- Add a Total Commander vs Cmdr blog post (f4ce564d4, d744a3800, 8190a0907, c0fdfd76a)
- Surface the Homebrew install (`brew install --cask cmdr`) on the website and README (c2e4ed549)
- Attach a PII-free machine snapshot (model, RAM, disk headroom, index size) to error and crash reports (d148af1b4)
- Migrate the MTP backend to the backend-neutral mtp-rs API for future Windows support (03f142794, 08a5059a4, 71b3d5809)
- Split the transfer and indexing modules into focused submodules (2597038a2, fe8b414d7, 4d65dcd07, e5005ca94,
  194190fa1)

## [0.29.0] - 2026-06-22

**Four big ones: pause/resume, a transfer queue, indexing everywhere, and nine languages.**

1. Copy, move, and delete operations can pause and resume.
2. Operations can be queued to run one after another.
3. Drive indexing now covers every volume type, including SMB shares and MTP devices.
4. Cmdr is now translated into nine languages.

### Added

- Translate Cmdr into nine languages: German, Spanish, French, Hungarian, Dutch, Brazilian Portuguese, Swedish,
  Vietnamese, and Simplified Chinese (5af98feaf, 43b7f4c29, 042c7b018, a34ef72f5)
- Pause/resume any operation (eeef1e2f8)
- Add a Queue window for ops, with pause/resume/cancel plus multi-select (c06b485d2, e279945b0, 49c7b126f)
- Add Pause/Resume and Queue (F2) controls to transfer progress dialog (07dd837c1)
- Index SMB shares and MTP devices so folder sizes and search work, with scanning/fresh/stale statuses (384bffe2b,
  7b084cdfd, 049e9f49c, e4cdbb8f6, 386e9c135, fbacdbd06)
- Add a per-drive index status badge and menu in the volume switcher (a36e7033f, eaa2eea0d)
- Add drive-indexing controls in Settings, a "index this drive?" prompt, and a one-time "drive stale" notice (bcd433ae9,
  0dddb45c6)
- Show a live file count while a drive index scans, instead of a frozen label (eca50e217)
- Add ⌘↓ to open the item under the cursor, ⌘⌫ to move it to the trash, and ⇧- to deselect files (54e8bdebb)

### Changed

- Keep MTP devices responsive during a background index scan: navigation, copy, and delete no longer stall behind it
  (0fa3faf97)
- Refresh only the affected folder on MTP changes, instead of every open pane on the device (7a08831af)
- Honor macOS Reduce transparency app-wide: every translucent surface goes opaque when the setting is on (298bdeded)
- Go back to the SMB host list with ⌘↑ in the share list, matching Backspace (1115440a6)

### Fixed

- Fix the error reporter crashing on log lines with accented characters or emoji (72a800eef)

### Non-app

- Speed up releases by reusing a persistent cargo build dirs and mise cache across architectures and releases
  (bc2b37794)
- Build the translation methodology: per-lang style guides, glossaries, and a reference-pile across 139 languages
  (45b6a7ddb, 0759d7205, ece168ea3, fbddd1650)

## [0.28.0] - 2026-06-19

The file viewer now renders images and PDFs inline, local and custom AI endpoints like Ollama and LM Studio work, and
counts and file sizes follow your Mac's region. The volume selector also gets a frosted-glass look.

### Added

- Show images and PDFs inline in the file viewer, with a Text/Image/PDF mode switch and a view-as-text fallback
  (ccfb536c5, c03c07156, e46cc1be5)
- Give the file viewer its own working menu bar on macOS: File, Edit, and View (with Word wrap) when it's focused
  (60b7b5687)

### Changed

- Give the volume selector a frosted-glass material and honor macOS Reduce transparency across the app (a10d7deff)
- Keep the volume selector open while ejecting, and make its row menu native (84fe8c66d)
- Major Settings revamp: Group Settings pages into cards and make Advanced settings findable from the main search
  (3f9168ce9, 43fb5ad1f, 027a89ed5)
- Format counts and file sizes by your Mac's region instead of always US formatting (0324047bd, 83906c5a2)
- Show real macOS default icons while the icon cache loads, replacing the emoji placeholders (8ea3a54aa, 7272df9d0,
  9b41bcc34)

### Fixed

- Fix local and custom AI endpoints (Ollama, LM Studio): the model picker now selects, and keyless endpoints register as
  configured (e83890030)
- MTP: Heal a stale destination folder on MTP upload and retry instead of failing the copy (010d8b45a)
- File viewer: Scroll search matches into view, and enable cut and paste in the search box (0496700ac)
- Show the running app version in copied diagnostic info, not a stale hardcoded one (32bce781f)
- Harden the backend against silent crashes and unsafe-code mistakes, and clear out dead code (d1e4f76f2, 6d2acfb0e,
  ab34d853b)
- Update the MTP library to 0.20.0 for transaction-ID self-heal and stale-handle recovery (7fedfadcc)

### Non-app

- Lay the full groundwork for translating Cmdr into other languages (English-only for now): a message catalog of ~2,070
  strings, region-aware number and date formatting, tooling that finds clipped text and screenshots every screen for
  translators, and a Language picker (56acb6c17, 17e05af8f, 2b085afc4, 375600ceb, 8af5a0bba, a3a9ef3cc)
- Move all error wording to the frontend so it's ready to translate, with the logic staying in Rust (1e918e06d,
  77a851b8e)
- Cut the docs that load into every AI coding session by two-thirds, and add checks to keep them lean (b84ca26ae,
  1ce6e7bbe, 3dad7e035)
- Route every in-app icon and spinner through shared components, and split several oversized files into focused ones
  (94b6218a4, 751e9bc4f, d3c50a874)
- Keep automated test runs from disturbing the developer's real apps, data, and keychain (28b6bcaf0, 2476aba4c,
  3a56d765b)

## [0.27.0] - 2026-06-14

You can now add/rename/reorder/remove Favorite folders in the Volume selector, hide the bottom F5/F6/F8 bar, and can set
Full mode to display filenames+extensions in one column. Also added `Help > Keyboard shortcuts`, a What's new popup that
shows up after Cmdr updates, and improved the Full Disk Access part of the onboarding.

### Added

- Show a What's new popup after Cmdr updates, with the changelog since the version you last saw and an opt-out in
  Settings (4e5ccbbad, cc2229194, 04f75ddb3, 9ca6c524b)
- Curate your favorites in the volume switcher: add (command palette, Go menu, or right-click a folder), rename, reorder
  by drag or ⌥↑/⌥↓, and remove (c660d6f47, 685fcac53, 335331ef4, 608b8c817, d3db386fc, 9dc2e9683, e3acd2a4e)
- Add a Help > Keyboard shortcuts window: a scannable reference of every command's shortcuts, live-synced to your
  customizations (3bcbc2855)
- Add a setting to show full filenames in the Name column instead of splitting off the extension (27060493f)
- Add a setting to hide the bottom function key bar (950a213ca)

### Changed

- Onboarding now detects Full Disk Access the instant you grant it, and gets Cmdr into the macOS 13+ Full Disk Access
  list (dbf4d70b0, 19e992dcc)
- The AI cloud model picker now loads its list on open and keeps it when you reopen (f8aa514d7)
- In Search and Select, ⌥←/⌥→ now move by word in the query field instead of navigating folders (dd8573b23)
- Search now remembers your query when you open a single result, not only "Open in pane" (5eae21397)

### Fixed

- Fix dragging a file from Cmdr into a browser upload field doing nothing (7c338b51c)
- Fix the file viewer misreading some binaries as UTF-16, which slowed the open by about a second (8f069f286)
- Fix the downloads jump re-opening a folder already shown in the other pane (9eee53959)
- Fix Search abbreviating paths that fit the column (3e558c7fb)
- Fix a rare drive-indexer race that could lose a folder's size (439d7fcbc)
- Stop local AI logging an error when you turn it off while it's still starting (1c8363b4a)

### Non-app

- Add a KV-backed `?r=` short-code system so tracking links expand to UTM params without a website deploy (f2b2c4659,
  7a5324067)
- Fix silently-broken Umami and PostHog injection (website analytics had stopped loading), and add a check guarding the
  regression (9cb620e8a, 36d859743)
- Add a per-day acquisition funnel with first-touch channel attribution to the analytics dashboard (8cae79064,
  a1dd804ec, a011de50e)
- Split the analytics dashboard into Acquisition, Product, and Link codes pages (83eb55be5)
- Converge the app's dropdowns onto two reusable Ark primitives for a consistent macOS-native look (d282fdba3,
  6ac9016ea, a705696d1, 69130e27e, 5f3556706)
- Add a `docs-reachable` check keeping every doc linked from the repo root, and connect the orphaned docs (69e91dbea,
  185afddb0, 74ef31ee8, 36b7075b9)
- Quiet the drive indexer's UNIQUE-conflict warning to fire only when two writers are racing the database (ba5a538ca)
- Ban two-column tables in agent-facing docs and convert all 130 existing ones, with a check enforcing it (a909679b4)

## [0.26.0] - 2026-06-11

This release sharpens the Search and Select dialogs: a Files or Folders filter, folders matched by size, an AI strip
that shows what the agent did, and your last query waiting for you on reopen. File-list dates and sizes now line up into
clean columns, and you can install Cmdr from Homebrew.

### Added

- Add a Files or Folders filter to Search and Select, matching folders by their recursive size (600b23cad)
- Add an AI strip to Search and Select that shows the pattern and filters the agent set, with a spinner while it
  translates (2328f4699)
- Install and upgrade Cmdr from Homebrew with `brew tap vdavid/tap && brew install --cask cmdr` (65729ee86, 6490cb167)
- Add a Discord community link to the About window and website footer (f65050b53)

### Changed

- Search and Select now remember your mode, text, and filters, and show your last results the moment you reopen
  (a5c60359d, df8195095)
- Keep your typed text when switching filename, regex, and AI modes, and land the cursor on the first file after a
  Select (8c90428e7)
- Line up the Modified and Size columns with tabular figures, and default to ISO 8601 dates (b84d68779)
- Enlarge the Search and Select dialog text a step and clear it to AA contrast (9effb0e5f)

### Fixed

- Fix Select doing nothing when you set only a size or date filter and leave the name empty (89204c287)
- Fix the size filter ignoring a `0` bound, and add a one-click `=` comparator (0071a009c)
- Fix an AI search keeping a stale size or date filter from the previous run (69ca52e59)
- Fix the onboarding AI step's provider list overflowing the options below it, and two stale provider links (44c905c1b)
- Fix commercial purchases not issuing a license (5e053ee60)

### Non-app

- Dashboard download count now means new installs, with a new-vs-update chart (7ff2b6f3e)
- Add a feedback and error-report section to the private dashboard (e449b007f)
- Add a `/feedback-and-error-digest-from-app` command for agents (77b49d09a)
- Cap each dashboard data source at 20s so one hung upstream can't 524 the page (8b2909a09)
- Capture real app screenshots and add a tracked `brand/` asset home (1b38da544, 8fa736330)
- Promote the Search and Select chip and popover primitives to `lib/ui` (14abab0d2)
- Quiet noisy dev-run logs (812e0bb5e)
- Fail the website build if a sandbox Paddle token would ship to production (7d227942c)
- Show a Discord invite modal after a website download (46829c22d)
- Use david@getcmdr.com as the public contact address (a667f7ce2)
- Clone `target/` to skip the full Rust rebuild on a fresh worktree (a2cbfce29)

## [0.25.0] - 2026-06-11

Cmdr is now an open beta: stability badges, a Send feedback channel, and anonymous usage stats you can opt out of. SMB
sign-in got smoother, and keyboard shortcut customization got a deep round of fixes.

### Added

- Mark Cmdr as an open beta in onboarding and the About window, with a personal intro from David (7ce2c5e41, b2b27d8f2)
- Add a Send feedback dialog (Help menu, command palette); notes go straight to David (79c4a6c9e, 6bdb188a1)
- Add stability badges (ALPHA, BETA) in the app and a feature status page on the website (219549db8)
- Add anonymous beta usage analytics (daily-active count, PII-free feature events), disclosed during onboarding, opt-out
  under Settings > Updates & privacy (d1c481f00, c328bb130, b2b27d8f2)
- Group crash and error reports per install, with an optional reply-to email so David can follow up (71da738c2)
- Add a progress bar, percent, and ETA to drive indexing, now a calm hourglass with details on hover (bc824f18e,
  6defbf74d, b03387e2f, f8694ce8c)
- Add a low-disk-space warning (in-app toast or macOS notification), configurable under Settings > Behavior (15ad9cf95)
- Drag files from your phone or NAS straight to Finder or the Desktop, with a toast tracking the download (c97a032f0,
  9e54719dd)
- Teach the go-to-latest shortcuts (⌘J in-app, ⌃⌥⌘J from anywhere) in the downloads toast, now collapsible (1da0b8357,
  9ab2cf4fd, 15fc93950)
- Shortcut hints across the app (F-key bar, toasts, onboarding) now follow your custom bindings live (123e76b73,
  e756a3795, 18acf50f5)
- Click any shortcut hint to jump to its row in Settings (b38f6cf88)
- Offer Finder's saved SMB password on "Connect directly", so a Finder-known share connects without retyping (2ccb45deb,
  3b07b0f21)
- Prompt for a fresh sign-in when a NAS password changes, instead of a misleading "unreachable" banner (7c654e70e)
- Add drag auto-scroll near a pane's top or bottom edge (6d1ca01b5)
- Prepare `brew install --cask cmdr` for installing Cmdr via Homebrew (9348f8884)

### Changed

- Reuse a saved SMB password instead of re-prompting on every connect (d12f8d3d8, 7c654e70e)

### Fixed

- Fix connecting to a password-protected NAS dead-ending in macOS's cryptic "error code -6600"; Cmdr now shows its own
  login form right where you are (0e1bc77d6)
- Picking an already-mounted share now goes straight there, even under a different name (Bonjour vs IP) (0e1bc77d6)
- Fix the wrong-password message and a stale connection dot after an SMB sign-in fails (5846d3513)
- Fix cloud AI for Groq, OpenRouter, DeepSeek, and Mistral (they were routed to the wrong API) (08aa31e10)
- AI search applies its translation again and reports failures (out of quota, bad key, timeout) instead of silently
  doing nothing (11f59ea1a)
- Move a stranded plaintext AI key from `settings.json` into the OS secret store (c9d45e096)
- Fix copying or moving an empty folder silently doing nothing, and across drives deleting the source (5053ea0b1)
- Fix the file viewer cutting off the file after about 60 lines with word wrap on (0655dc0b9)
- Fix dialogs leaking focus into the background and locking out the keyboard after two Tabs (f2e049736)
- Closing Search or Select files with Esc no longer kills pane keyboard navigation (040d424e1)
- Fix ⌘A doing nothing in the Settings and viewer windows (d99fafc1b)
- Fix drag-out from a phone or network pane dropping a junk `.textClipping` file or pasting a meaningless path
  (6e8ac5aea)
- Fix index rename failing when the destination name is already taken (dea074272)
- Harden shortcut capture: bare keys don't fire mid-typing, and macOS-owned combos (⌘Space, ⌃↑) warn instead of saving
  silently (a412e599c, 92c5ad4bf, 2b7abf3f3)
- Fix custom shortcut rebinds and removals not sticking, not reaching other windows, or missing conflict detection
  (6c21fd1b5, da5705667, 2247dac15, a1dae8896, add4db81d)
- The command palette and the Keyboard shortcuts editor now show your real bindings and list every command (87df2ed95,
  73766c9ee, 762b3951b, 396097ffd)
- Focus the textarea when the feedback or error report dialog opens (6f295fc64)
- Show "/" instead of a raw storage id (like "65537") in the tab title at a phone or camera storage root (582cfbaff)

### Non-app

- Rewrite the website around one honest feature list (a bento grid by capability), in a product-first voice (272d177eb,
  e975bd0c0, 6ccb8aeb7)
- Cut the landing page from ~2.3 MB to ~0.4 MB and remove render-blocking CSS (5fc6729a0, fbacb4e95)
- Replace stringly-typed backend event emits with a typed event bus across volumes, write ops, indexing, MTP, network,
  git, and AI (f2d3febf1, 57e9c87da, 5f510bd2b)
- Split colocated docs into `CLAUDE.md` and `DETAILS.md`) across ~30 areas, add `claude-md-length` check (9bf1a6534,
  bb26f2dff)

## [0.24.0] - 2026-06-06

Go to path (⌘G) lands, folders merge on copy and move, and same-volume moves are instant.

### Added

- Go to path (⌘G): jump anywhere by typing or pasting a path, with `~` expansion, recent paths on digit keys, clipboard
  prefill, and a nearest-existing-ancestor fallback when the path doesn't exist (2a87c01b5, afa2fe18c, 6b3e941b6,
  3a768fcc1, 078777929)
- Block ejecting a volume while a copy, move, or delete is touching it (fe2a09879)

### Changed

- Folders always merge on copy and move: your conflict choice (skip, overwrite, or rename) applies to the clashing files
  inside, and dest-only files survive (89cd978c3, 6e305a472)
- Same-volume moves are instant: moving within one drive, share, or phone is a rename, no more 30–40 s "Verifying before
  move…" on a big NAS folder (a9743ecc3, 114e5d2d8)
- Completion toasts now report what you selected, split by type: "Moved 1 file and 3 folders" (ae6296095, f977ed953)
- Disable Rollback for same-volume moves (a rename has nothing to roll back); Cancel stays available (f069e37e7)
- Rename "Reveal latest download" to "Go to latest download" in the menu, palette, and settings (49ddaf0aa)

### Fixed

- Resolve conflicts file by file inside folder merges on network and phone drives; a newer file deep in the tree no
  longer loses behind a single folder-level OK (6e305a472)
- Fix dropping files from the Desktop, Documents, or Downloads failing with "Source volume not found" (c30212436)
- Fix drags from phone and network panes reading 0 bytes / 0 files in the transfer dialog (c30212436)
- Dropping onto a read-only volume now shows the "Read-only device" alert instead of a copy dialog that can't succeed
  (62bbc09a5)
- Fix the Copy→Move toggle zeroing the transfer dialog counters on local moves (f4a8b1cbe)
- Show the volume name instead of a raw storage id (like "65538") in the transfer dialog header (f4a8b1cbe)
- Fix file viewer settings (word wrap, text size, binary warning) silently resetting every session (51e127aaa)
- Make the title bar draggable while a dialog is open, and in the file viewer window (016abbdfb, e28e8905c)
- Highlight cloud drives (iCloud, Dropbox, Google Drive) in the volume switcher instead of Macintosh HD (28e72ac03)
- Fix tooltips jumping to the window corner in big folders (2b45ec085)
- Fix a rare hang when answering a copy/move conflict prompt (070b8d15e, 99271478a)

### Non-app

- Rebuild the explorer frontend architecture: a module state store, a typed command bus across every entry path
  (keyboard, palette, menu, F-bar, MCP), one transactional `navigate()`, a per-kind volume capability table, and a flat
  command handler record replacing an 89-case switch (062ebbb72, 5709b50ac, ef52db455, 6270612cd, 6aaf82d08, c7c0f5d6c)
- Add a virtual MTP device for dev: `CMDR_VIRTUAL_MTP=1 pnpm dev` plugs in a fake "Virtual Pixel 9", no hardware needed
  (9b9a4cadb)
- Make the SMB test containers safe to share across concurrent agent sessions: lease-refcounted teardown, auto-restart,
  and resource caps (b4307236a, 7905a4ea4, 7ae14a75d)
- Stop E2E builds from uploading error reports to the live channel (293853b02)

## [0.23.0] - 2026-06-01

A guided onboarding wizard, a Downloads watcher with a jump-to-latest shortcut, and AI-powered file selection. Under the
hood, copy and move became durable and crash-safe.

### Added

- Onboarding wizard: a multi-step soft sheet (Full Disk Access, AI provider, optional setup) replacing the single
  permission modal, reopenable from the menu, command palette, and MCP (alpha version!) (5a21bdba0, 742ff625d,
  963b4bf17, 88ecdfaa1, 7d081d2c4, a09631c98)
- Downloads watcher: a toast or native notification when a download lands, and jump to the latest download with ⌘J or a
  global ⌃⌥⌘J hotkey (092203db1, a9466e5a4, 853a28a09, d378f42f6, 1484c4f0a, 2c3e36c33)
- Select or deselect files by query: a new Select menu plus a Select files… dialog with filters and AI-powered
  natural-language selection ("select all error logs from last week") (alpha version!) (1fd163c4d, 7ce90bb35, 8d5bd3dc9,
  dcb4b3a91, 6d68def3a, ac68709e8)
- File viewer tail mode (F): follow a file live as it grows (8a6671de2, ed479d2b9, a7eb8d876, 29a25ffca)
- File viewer char encoding picker: switch text encoding instantly, with strict ISO-8859-1 and UTF-16 BOM detection
  (a2270782d, 0c0b8716a, b12779067, 3978ed4c6)
- File viewer regex and case-sensitive search toggles (⌘⌥R, ⌘⌥C) (7d424d976, 48b5de063)
- Real folder icons in the list: system folders (Downloads, Desktop), packages (.app), and custom-icon folders, cached
  across restarts (1dd439d0c, 389829bfe, e50004ab2, 418a86a95)
- Per-directory "size updating" hourglass and progressive folder-size reveal as the index fills in, instead of waiting
  up to 5 minutes (0afc10b4c, f37401520, 66712c2d2)

### Changed

- Redesign the type-mismatch conflict dialog: one consistent layout across all clash types, with a clear warning when
  overwriting a whole folder with a file (a3faa3d8a, d2b8f153d, 790249320, 66df65702)

### Fixed

- Make copy and move durable before reporting "complete". Ejecting a USB stick right after a copy no longer loses files
  (bdb3b61aa)
- Make cross-volume Overwrite crash-safe: stream to a temp file and swap in place, so a mid-transfer disconnect keeps
  the original (6e99640e8)
- Stop concurrent indexing from corrupting the index (fixes inflated folder size display) and keep the index WAL bounded
  (0236723d9, eb6922870, b849ee018)
- Make config and secret-store writes survive power loss, protecting saved SMB servers, passwords, and AI keys
  (aea4aa0b1, 57a47b63b)
- Stream MTP uploads instead of buffering the whole file in RAM, and make Cancel stop in-flight USB writes (a01401502)
- Fix cross-volume moves showing "Moving... 0 bytes / 0 files" for the whole transfer (now real scan and per-file
  progress) (067b96db6)
- Open file viewer instantly even under heavy FS activity (was up to 730 ms and could time out) (aa9905f1a)
- Keep live indexing alive under database lock contention (9e8089147)
- Fix a git-repo watcher leak during fast navigation (a0bac502d)
- Stop losing Full Disk Access and onboarding state on a save failure (which re-ran onboarding) (5c46d8872)
- Error instead of silently overwriting when creating a file that already exists (25ce82f40)
- Fix Enter or Backspace on ".." from "~" landing at "/" instead of "/Users" (a8096a251)
- Fix SMB share listing on servers with many shares (native enumeration handles fragmented replies) (fe5569cff)

### Security

- Require bearer token for destructive MCP ops (68e337efa, 18cd4c35f)
- Redact PII from MCP logs and state (8ea092ba5)
- Close SMB password leak through process arg list in an edge case (a190f19c9, 0a154f21c)
- Reject plaintext-HTTP AI endpoints that carry an API key (3dd10609a)
- Fix an updater AppleScript injection via the app bundle path (5875fb4c0)
- Narrow down the FS capability to actually needed files, restrict Debug window's capabilities (6cabc94c5)
- Redact SMB credential URLs from debug logs (d7edb8a46)

### Non-app

A big push on dev tooling: the check suite is roughly twice as fast overall, with some checks 30–40x.

- Add CPU-weight-aware scheduling (46bfae993)
- Add `--graph` arg to checker script to view the dep graph (46bfae993)
- Split `eslint-typecheck` into TS / Svelte: 616s to ~15s (~40x speed-up!) (106327893)
- Stop clippy forcing full crate rebuild every run: ~32s to ~1–2s warm, also sped up other Rust checks (3318f29ca)
- Switch Svelte tests to happy-dom (22% faster) (ca6b13d94)
- Add per-instance isolation (`CMDR_INSTANCE_ID`). Parallel dev sessions now get own ports, data dir, and Keychain
  (3bcd2ed45)
- Add a `lock-poison` static check and pnpm install-side supply-chain guardrails (14-day cooldown, trust no-downgrade)
  (038c5ec21, d568789fd)

## [0.22.0] - 2026-05-23

The Search dialog got a full redesign, and the file viewer learned text selection and copy.

### Added

- Redesign Search around one unified bar with mode chips for AI, filename, and regex, each remembering its own typed
  query, and keep the dialog's state when you close and reopen it (62aef4404, ac4c63405, b9ca1e6fb, 3ea1b45e8,
  5c35d9ea3, 9b8f9dd7c, 71c9485b7)
- Filter searches with size and modified-date chips that open quick popovers, and see the AI's interpreted prompt and
  caveats right in the dialog (2c10bba7d, 807e456ea)
- Replay past searches from a recent-searches history with quick-pick chips, and auto-apply filename and regex queries
  as you type (1f03ff49e, f4eea79dd)
- Act on search results in place: clickable path pills, per-row menus, "Show all in main window", and copy, move, or
  delete files straight from the results (e52c6dec1, d94187bdf, c79c11124, 4770a93f8, e7afc8b3a, 1b1fc5aba, f3f450848)
- Select and copy text in the file viewer (files up to 100 MB): double/triple-click for word/line, right-click menu
  (6f7178293, 1e061820e, 8d6f85c01, 46f278bb1, e329bb399, 1445c2d75)
- Eject ejectable volumes (USB, SMB, DMG) from the picker and the breadcrumb right-click menu (2a7e256f8)
- Replace the human-friendly size units toggle with a 5-way size unit picker (dynamic / bytes / kB / MB / GB)
  (78a7f3676)
- Show climbing bytes and dirs during MTP/SMB scan previews (was "0 / N / 0" until done) (c2b5a0404)
- Reuse scan-preview cache for local delete and cross-FS move so the dialog skips straight to the active phase
  (9445e61a6)
- Center child windows on the main window; file viewers cascade (8cd06bf4e)
- Tint zebra stripes with the pane bg so per-volume tinting actually shows through (b84e761e6)
- Toast confirms zoom changes and points at ⌘0 to reset (37f944100)
- Show a Space hint on "Toggle selection" in the right-click menu (a24613d9b)
- Blue info toasts, new colorless `default` level for low-importance feedback, and reclassify routine confirmations and
  soft refusals (dabf0e3a6, 51e301128)

### Fixed

- Fix SMB share mis-loading local paths after a volume switch (3e613ca6d)
- Fix volume copy dialog wedging open after SMB/MTP cancel (0fbafebb8)
- Process selected files in pane sort order, not Cmd+click order (39fc8d2eb)
- Cursor lands on the new folder, not the row below (38ebdc876)
- Fix Full view ".." row hiding behind the header after PageDown/PageUp (6ddb4273c)
- Fix viewer ⌘A freezing on huge unindexed files (e29312bd9)
- Cancel viewer reads within ~64 KB instead of 16 MB (0e758b468)
- Fix Escape on viewer context menu closing the whole window (4464f7666)
- Honor `prefers-reduced-motion` in viewer drag autoscroll (aec327b80)
- Surface silent-band viewer copy failures as a warn toast (41398aca8)
- Polish viewer copy dialogs: ⌘A routes to the right size tier, Enter triggers the primary action, Tab skips ×
  (b6542e7b2)

### Non-app

- Build git test fixtures via `gix` instead of CLI shell-outs; 91 tests went from tens of seconds to ~1.7 s (532722c8a)
- Trim slow tests under the 8 s nextest cap, drop the 30 s `cap_bundle_*` exception, make
  `index_mtime_change_invalidates_cache` deterministic (f4c0b5ad7, 9e23ff2ac, 444294051)
- Re-enable three previously-skipped E2E tests; the culprit was a Node fixture-helper dangling-symlink bug (915c5f33c)
- Settings-style chrome and a live SMB diagnostics dashboard in the dev Debug window (6bd0f15c1, e7660b3a4)

## [0.21.0] - 2026-05-21

Quick Look (⇧Space) arrives, and Settings plus the main window now look properly macOS-native.

### Added

- Add Quick Look (⇧Space) (6778494b1)
- Add ⌘← / ⌘→ to copy the cursor path between panes (a3e15f450)
- Add red binary-file warning in the file viewer (74e7b0cd0)
- Redesign Settings window to look like System Settings (694809319, 76be4f8aa, 9668a0788, 91c31f35c)
- Redesign tab bar, flatten panes for a more native-macOS look, fix UI glitches (dc7d6500c, 9668a0788, 3771570af,
  79ed3b6ca)

### Fixed

- Fix transfer dialog showing "✓ 0 files" when pre-flight scan beat the FE listeners (8525835c9)
- Fix stale path events corrupting the breadcrumb after switching a pane to Network (a3e15f450)
- Fix Quick Look toast/content import cycle (b3d67fe66)

### Non-app

- Move `rust-toolchain.toml` to the workspace root so every crate pins one toolchain (fixes v0.20.0's
  `rustup target add` drift) (41e999ab0)
- Add `workflows-rustup` check forbidding `rustup target/component add` in workflows (c68630ee8)

## [0.20.0] - 2026-05-20

Snappier and safer transfers: MTP cancels land instantly, SMB writes pipeline over one session, and selection switched
to a high-contrast red. Cmdr now runs on macOS 12 Monterey, too.

### Added

- Cmd+click toggles selection (c6adee741)
- Bind `Insert` to toggle selection in Total Commander style (719e4f9bf)
- Modify Shift+Arrow/Page/Home/End behavior to align more with other file managers (47932132b)
- Switch selection to red. Clears WCAG AA across all backgrounds! (9028722c6, 02b295da7, 069bc400d, 14a36dd8b)
- Tint each pane's background by volume type (local/SMB/MTP) (3f5629d39)
- Improve MCP: replace fire-and-forgets with round-trips (48a9701c0, 3c1b0dc9b, e12285d15, df11caef8)
- New MCP resources: `cmdr://logs` + filters, `cmdr://state` filters, `recentErrors`, `upgrade_smb_to_direct`, SMB
  connection state (e597d24de, 640c3330e)
- SMB volumes auto-upgrade from OS-mount to direct smb2 sessions (640c3330e)
- Copy/move/delete pre-flight scans reuse watcher-backed listings. Skip a 17s MTP re-list when the folder is already
  open in another pane! (9d4346384, ba20ca3ea, 49187230b, fdebd3297, b90b90037)
- SMB streaming writes no longer hold the client mutex (smb2 0.9). Concurrent writes pipeline over one session
  (3d0d5db7c, 06bc5da75, ed4b6886c)
- Bump SMB watcher to smb2 0.10 to stop losing events between polls (432d13ffe)
- Localize macOS pane names in onboarding and error dialogs (points at what System Settings actually shows) (bad5d9263)
- Honest transfer-complete toasts: report copied vs skipped separately (5cdf989e0)
- Polish the license nudge: clearer copy and layout (95007952b)
- Add fallback UI colors on macOS Monterey, achieving macOS 12.x compat! (5792b10ee)
- Improve accent-fg to match WCAG AA+ against all colors, and add cursor outline (d00ba5b41)

### Fixed

- Propagate MTP cancel all the way to the USB layer; no more 30-second "Cancelling…" wedges (0de4c6b70, 1696355d5,
  f894e60e5, b40188919)
- No more empty-pane flicker on bulk ops (coalesced refresh events) (546748549, 13b486a81)
- Friendly message for SMB `STATUS_DELETE_PENDING` (was misleading "disk needs attention") (a560243bb)
- Properly pluralize all words ("1 file"/"10 files") everywhere (eb3603709)
- Fix MTP destination pane staying stale after cross-volume writes (873f1102d)
- Fix SMB/MTP listing cache going stale when the watcher misses an event (1dea24e17, ab98ee880)
- Fix MTP delete not emitting `write-cancelled` when cancel landed mid-iteration (e21ca6d3c)
- Fix transfer dialog wedging at "Cancelling…" when Cancel raced ahead of the `operationId` IPC (2b2a5ec60)
- Fix MCP `open_under_cursor` on the Network view (0aec8fbdf)
- Fix Linux startup hanging on a half-configured D-Bus (probes now bounded by a 500 ms timeout) (91afacbfd, 85580df9a)
- Fix `refresh_listing` short-circuiting on local volumes during the FSEvents symlink race (57ef10345)
- Fix two SMB shares with the same case-folded name on different servers colliding on the same volume ID (f2414556f)
- Fix opening a guest SMB share popping the kernel `smbfs` credential dialog (92119464d)
- Fix `TransferErrorDialog` being see-through in the transient branch (f01af3591)
- Fix error dialogs rendering OS strings with markdown bleed-through (`STATUS<em>DELETE</em>PENDING`) (dbd7a2ac1)
- Fix Brief mode cursor stripe briefly spanning the entire pane while column widths load (d676efa57)
- Fix Move dialog hiding the Size progress bar (`bytes_total` was 0) (8856e0127)
- Fix conflict-resolution radios reading "Skip all" / "Ask for each" when only one conflict exists (4eac76b45)
- Fix focused-button Enter firing the dialog's default action instead of the focused button (079a0ce1a)
- Fix free-space numbers tier-coloring as red on healthy disks (8219a06c7)
- Fix the AI offer prompting Intel Macs for a local-model download they can't run (52f3cd812)
- Fix every tokio task crashing when stderr becomes a broken pipe (31d97e061)
- Fix Linux compile errors in `errno.rs` and the MCP resources module (90b0afee1)
- Fix Linux compile errors in `system_strings.rs` (macOS-only loctable items) (e852f04a5)
- Fix `clippy::unnecessary_sort_by` on Linux volume sorting (1.95 picked it up) (03faf4809)

### Non-app

- Cap every Rust test at 8 s (matches the Playwright convention), with documented exceptions (eb67f3895)
- Stop gating `desktop-e2e-linux` on `desktop-rust` in CI (66a2e501f)
- Harden the checker against supply-chain attacks: `--locked` everywhere, pinned tool versions, new
  `workflows-hardening` + `govulncheck` checks (7d771ca83)
- Declare `rustfmt` and `clippy` as required `rust-toolchain.toml` components (a23222ebf)
- Trigger Rust CI on `rust-toolchain.toml` changes (0f8c9ffb5)
- Dev override `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` to test the old-WebKit fallback on modern Macs (175375100)
- 14-day release-age gate via Renovate (3-day override for security advisories) (8bd5af1ec)
- Shared `pluralize` helper for log/error/UI strings, plus a `pluralize-noun` check (0ae2ee927, ec277ba80, e070fc34a)
- Force file-backed secret store under `CMDR_E2E_MODE=1` (no more Keychain prompts in unattended E2E) (ecb495fc3)
- New `btn-restyle` check (forbids `.btn-*` overrides); accent-matrix in the contrast check (51f31939a, 0e885f5db)
- Codify 100-char Rust comment width; reflow existing comments (b76b9277f, 610f66f6a)
- Vendor `smb-consumer-maxreadsize` and pin the SMB streaming-write no-deadlock invariant (200 × 1 MB at concurrency 8)
  (1ae6eec7a, e8259eef6, e750920bd)
- Ticketed acquire/release logs on the `SmbVolume` client mutex (2e4aeb9df)
- E2E focus hygiene: viewer/settings windows skip OS focus, Escape-binding tests use synthetic dispatch (be21bebe7,
  0dfdcb2af)
- Defensive disk-poll + refresh in the MTP→local copy E2E (9693b283d)
- Stamp the running E2E test name into the main window's OS title (1181e0c16)
- Document the UTM Ubuntu VM loop for iterating Linux-only tests (917938ee3)
- Switch `mtp-rs` to crates.io 0.15.0 (off the path dep) (f98313f09)

## [0.19.0] - 2026-05-16

Settings got reorganized into clear sections, the command palette remembers your recent commands, and you can type to
jump to a file.

### Added

- Reorganize Settings into Appearance, Behavior, File systems, Updates, AI, Network, Privacy, and Advanced (c3003a057)
- Add "Overwrite all smaller" and "Overwrite all older" conflict actions (2dfd17b8a)
- ⌘⇧T reopens closed tabs; double-click the tab bar opens a new tab (65417fbe8, d7a85a332)
- Move AI API keys to the OS keychain, with 300 ms debounced save (42bc5eaf7, 10f8525b5)
- Command palette recents on empty query (last 10, LRU, grouped, self-heals stale IDs) (d34062992, a2971abab)
- Type to jump to a file in the explorer (0b9f943f7)
- Sort-column shortcuts ⌘3–6 (Brief) and ⌘F3–F6 (Full) (74e827e51)
- Brief mode: backend-computed per-column widths, plus a max-column-width slider (d84d5c2a9, f79071071, f9e40fc4b,
  e18bdbf43)
- Volume picker wraps cursor at top and bottom (206ec7d90)
- USB link-speed indicator in the volume switcher (637b152e5)
- Stream MTP source-scan progress in the copy dialog (no more 0/0/0 freeze) (fef1aafd1)
- Bulk-skip pre-known conflicts under Skip-all for copy and move (b365076d9)
- MTP→SMB copy: kill the 2-min stall, faster source scan (1ae5c1980)
- Honest copy ETA on long single-file streams: stop decaying files_rate (4737acbc9)
- Format sub-1 files/s readouts instead of rounding to 0 (ff7a72f94)
- Strip em-dashes from user copy and docs; rephrase microcopy to sound more human (971e35c4d, c39ecdc7f, a16afb0c5,
  adab08fa2)

### Fixed

- Fix MTP delete freezing instead of showing live scan progress (4e005f95c)
- Fix Cancel-copy losing the rollback on the APFS clonefile fast path (9c2e62440)
- SMB upgrade no longer races mDNS in dev (be1350d79)
- "Connect directly" SMB login dialog now shows the actual server name (0d84e4e7f)
- Bulk-skip no longer pollutes the throughput estimator, and only fires for top-level file conflicts (55d3ca461,
  c3be95c15)
- Per-iter Skip on volume copy credits byte progress (e7f657dfa)
- Show duration settings in their declared unit (66571349b)
- Brief column-width slider enables inside the Settings window (591e090b6)
- Brief mode horizontal scrollbar drag no longer vibrates at 60 Hz (b80789e14)
- Restore focus when a ModalDialog closes and when the command palette closes (35413fa38, 6c45e12d3)
- File viewer surfaces `SearchStatus::Cancelled` to the FE on cancel (14ba27357)
- Separate MCP ports for prod (19224) and dev (19225) so dev no longer collides with the installed app (f05246587)
- `setSetting` is idempotent on unchanged values (c49636d86)
- Pane state: clear network host on leaving the network volume; skip FilePane MCP sync on Network (602fcb94b, a1d19947f)

### Non-app

- Refactor write ops behind a shared transfer driver: per-source loop for copy/move, sink-based inner functions across
  local and volume code paths, drop one unsafe transmute (b6833e269, 1d9f2ca42, 63b6728ef, 0218a6458, 01c8614ec,
  101e8385c, 118ac6b1e, bc957471e, 9d7c69e8c, a056eb58f, 5cf1173ac, 1280056bc, 0a7c257c0, 643e7cb27, afb709015)
- Parallel-shard the E2E suite across three Tauri instances (MTP isolated, two non-MTP shards balanced); wall-clock 5m
  49s → 2m 48s (7802fca3f, 1841e0c5d, 6e8971a0e)
- Cut Playwright wall-clock 10m 12s → 5m 6s via condition polling, MCP-driven cursor moves, beforeEach short-circuits,
  and a per-keystroke → menu-dispatch migration (507afb0ed, 3b04806e0, f907adc2d, df89b2172)
- Add proptest-based property tests for `platform_case_compare`, search scope parsing, `glob_to_regex`, and
  `topological_sort_bottom_up` (2e747bf88, ffd799c86, 1813e3dc4, 2cf586d17, e69e45aa0)
- Add state-transition tests for `IndexPhase`, `ActivityPhase`, `DiscoveryState`, and `SearchStatus` (c0aed6511,
  9a9899e9b, 9dd325046, 4ae151201)
- Add vitest mockIPC harness plus IPC contract tests for SMB connection, file viewer, and write operations (04c26e4d4,
  3a538b443, baa977ed2, 967d93be5)
- Add mutation-testing-driven unit tests across `indexing/store`, `chunked_copy`, `watcher`, `copy_strategy`, and
  `state` (ef91cfb83, a812cd9a3, e9a3a9fd4, b026f43d3, 4f04d03c4, 41a3a8313)
- Codify the testing playbook and tools inventory (9515adde8)
- Add ESLint rule `cmdr/no-arbitrary-sleep-in-e2e` (a9aea301e)
- File-length check: 10% growth buffer with growth % shown; split long files into focused modules (1c1bdeb0c, 2d7c27a37)
- Pre-commit `--fast` lane in the check runner (33f77ca58)
- E2E windows get a blue title stripe and `E2E -` prefix so they can't be mistaken for the installed app (b1f707b78)

## [0.18.0] - 2026-05-12

First launch stopped stacking permission popups, copy and delete dialogs show real scan progress, and cloud AI grew to
cover many more providers. Dates and sizes are now color-coded across the app.

### Added

- Suppress the 5–10 macOS permission popups that stacked behind the Full Disk Access prompt, and deep-link straight to
  the right System Settings pane (3c708d356, 169182183, f32dfc55d, 791edff0a)
- Flag TCC-restricted folders live in the sidebar and file list: italic + (i) icon, `<no perms>` Size, generic folder
  icon for FDA-gated favorites, failed listings stay in nav history (7baa93175, 6581f5ad0, df6cd794d, 762d7b9ad)
- Defer the AI offer toast until onboarding ends so it stops piling on the FDA prompt (265c72d9e)
- Color modified dates by age with per-segment tiers (year, month, day, time each get their own color); App palette is
  now the default (c73fcf54d, d98459b6a, be2333c20)
- Color sizes at every previously-plain site (tooltips, breadcrumb, transfer/delete dialogs, viewer footer, AI progress,
  search results); light-mode palette retuned to clear WCAG AA against every background (265c5a0e6, 31128012b)
- Show real scan progress in copy/delete dialogs with running tallies, throughput, current directory, and a real
  progress bar; hardlinks deduped by inode so totals match the indexer (03215d259)
- Honest ETA when files outnumber bytes: tracks both axes, picks the slower; no more "~0 s remaining" while the
  small-file tail drains (16b49a04d)
- Stream folder-name suggestions in the New folder dialog: first option in under 500 ms instead of after the full reply
  (d681c8ded)
- Add multi-provider AI via the `genai` crate (GPT-5, o-series, Anthropic, Gemini, xAI, Groq, DeepSeek, OpenRouter,
  Ollama); fixes GPT-5 400 on `temperature` and `*-pro`/`*-codex` 404 on chat completions (0c45a4695)
- Cap updater hangs at 30 s and surface the real cause (DNS error, TCP deadline) instead of generic "error sending
  request" (e5be14677)
- Per-row crash email with build mode and short ID, schema migrations, newest-first sort (e89a63a3a)
- One stable short ID per error report, shown the same in the dialog, the toast, and on David's side (772608278,
  e18103612)
- Guard read-only volumes up front for F7/F8/F2 so MTP read-only SD cards warn before you type anything (d9212b835)
- Friendlier write errors that name the provider (like "Managed by **MacDroid**…") and offer Retry only when it helps
  (e94520327, 51dff4c1b, 5bcacfef2)
- Make Stop/Skip/Overwrite/Rename work for folder conflicts on cross-volume copies too (7ecf9d370, 2f4e377d0)
- Fix merging into an existing SMB folder after a partial copy (smb2 0.8.0) (7dd9cfc87, 623f8c17e)
- Move MCP defaults to ports 19224 (prod) and 19225 (dev) so a dev build no longer collides with the installed app
  (c9fad17ec)
- Polish getcmdr.com hero: "Download for macOS" button, viewport-responsive illustration mask, muted link style,
  tightened copy (606c724ed)

### Fixed

- Fix F8 and other dialogs dying after a volume switch (f2019aff5, 46bd6d0e8, eef042d36)
- Fix the Modified column ellipsizing on some rows under non-100% text size (a7a7915ec)
- Fix light/dark theme briefly flipping at startup when the persisted choice differed from the system preference
  (f689da019)
- Stop the dev runtime silently overwriting committed `bindings.ts` on every `pnpm dev` launch (6e39d68db)
- Silence the `get_file_at` FE/BE drift warning that fired legitimately during async listing refreshes (0b51a331d)
- Accept `null` for optional crash-report fields so reports written by older app versions still upload after upgrade
  (3c12ff2f2)
- Fix dropped keystrokes during fast multi-select sequences (6074cd211)

### Non-app

- Migrate the full IPC surface to typed bindings via tauri-specta; an ESLint rule and a Go check block raw `invoke()`
  and lockfile drift (f1e580110, dc5f0b47d)
- Ban classifying errors by string-matching `message`/`stderr`/`title` with a Go check and ESLint rule; sweep across
  SMB, git, friendly errors, and updater (c764962af)
- Pin pnpm 11.0.9 in `mise.toml` and move overrides to `pnpm-workspace.yaml`; unblocks CI's E2E-Linux (cee0aa08c,
  c41d2e0d7)
- Track recurring upkeep in `docs/maintenance.md` with a log going back to 2025-12-25 (49a119bd8)

## [0.17.0] - 2026-05-06

Dynamic text size lands, along with "Open with", system Services, and Finder-matching drag and drop.

### Added

- Add dynamic text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts) (a326bca69, ca78382dc, e207effb6)
- Add "Open with" and system Services to menus (71e6061ba)
- Add iCloud Drive cloud actions to context menu (01bc0daeb)
- Split Brief/Full menu items to per-pane View > Left/Right submenus (7f4d123d4)
- Add networking toggle, lazy mDNS, no more local-network prompt at launch (d2ae51703)
- Faster external drive detection, fixes USB-C dock invisibility (6527d850a)
- Drag & drop matches Finder (same-volume Move, cross-volume Copy, modifier overrides) (64db140f9)
- Drag & drop "+" badge tracks the actual op, no flicker (cf8e3818a, dcfe439ec)
- Drag files into terminals (Warp etc.) (97d106753)
- Add Trash/Delete toggle to delete dialog (778296dde)
- Always show Copy/Move toggle in transfer dialog (450363e62)
- Default to Full mode on fresh installs (57ba47c17)
- File list typography polish: aligned dates, aligned headers, fade selection, clamped Ext (474f74140, e9aec7bda,
  88f56367e, c56989986)
- Add size-color palette setting (Rainbow / Accent / None) (5fe0d77ec)
- Restore double-click-to-zoom on macOS title bar (f95441dc0)
- Focus search when Settings opens (cb88685da)
- Hand cursor on License dialog support and Buy links (554b38012)
- Show real .git/\* files alongside virtual categories in git portal (33219321a)
- Per-file Modified dates inside git portal snapshots (3cead8788)
- Cache git status per index change, near-instant repeat navs (19f0e98ed)
- Error-report preview now lands under 200 ms on big log dirs (was 30+ s) (f24f255c2)
- Send error reports in dev too, tagged \[DEV\] (63ebabf65)
- Persistent "Save bundle to disk" toast with Reveal in Finder (0debff1cd)
- getcmdr.com comments follow live theme changes (7333b13ce)

### Fixed

- Fix Intel DMG download 404 (19f797da8)
- Fix crash on virtual git portal toggle; empty git roots no longer render as 1970-01-01 (b266737ec)
- Fix folder size column losing value after rename (b1d032c1e, d7e08e16c)

### Non-app

- Big dead-code cleanup, 355 lines across 22 files (a6b461312)
- Bump GitHub Actions to Node 24 (2f02fa7e6)
- Replace claude-md-staleness with claude-md-reminder (fires in-loop, not weeks later) (60e30be54)
- Big CHANGELOG cleanup: shorten long items and document style guidelines. (8f3daa0ad)

## [0.16.0] - 2026-05-01

Network shares now reconnect on their own, and you can check for updates from inside the app.

### Added

- Add SMB live reconnect, 5-attempt backoff right in the pane, no re-auth (d96bc4b4f, 0c1d36808)
- Disconnect button now actually unmounts (toast if Finder's holding the volume) (c5a410aa8)
- Add Check for updates from inside app (00470b96d)
- Add human-friendly size units toggle (c8cc1008b)
- Add symlink-aware size hint, info icon explains exclusion (matches du and Finder) (0d83a7b27)
- AI download toast X stays closed for the rest of the download (97f1cee37)
- Skip rename warning for equivalent extensions (jpg/jpeg, htm/html, yml/yaml, tif/tiff, etc.) (55592ba48)

### Fixed

- Fix temp network issues kicking users out of folders (48ac9bf81)
- Suppress "Restart to update" toast during first-launch onboarding (ffeb7d96f)
- Fix indexer triggering macOS perm popups while onboarding: now waits for FDA (59aca7177)
- Fix SMB reconnect runaway subscribe loop after hot reload (91bc2e46a)
- Fix SMB reconnect double-triggering loadDirectory (3f6b1b0d3)

## [0.15.0] - 2026-04-29

The git browser lands: browse branches, commits, stashes, and worktrees like folders, and copy a file out of any
version.

### Added

- Add git browser: live branch/dirty pill in breadcrumb, browse `.git/branches/`, `tags/`, `commits/`, `stash/`,
  `worktrees/`, `submodules/` as folders, drag any file out of any branch or commit into working tree (preserves bytes
  and exec bit, no `git checkout`), optional per-file status column with M/A/D/?/! glyphs (314e9ae24, 897df2c72,
  1ebcfa1c1)
- Meaningful Modified and Size columns in git portal (`+12 / -3` for branches, `5 files` for commits, `on main` for
  stashes, short SHAs for tags) (31aec35ca)
- Add friendly errors for git browser (19d5b0754, af64689f9)
- Add Git toggles in Settings (repo chip, status column, virtual portal) (19d5b0754, af64689f9)

### Fixed

- Fix virtual `.git/<category>/...` paths kicking pane back to parent (bfcbfa483)

## [0.14.0] - 2026-04-26

### Added

- Add error reports: one-click redacted diagnostic bundle via Help menu or error toast, with optional auto-send and a
  short ERR-XXXXX correlation ID (6d904aa6b, 51b6102a2)
- Add log storage cap setting (default 200 MB, 0 disables log storage and error reports) (f3dbf514f)
- Add per-output log filtering, with a verbose-logging toggle in Settings (319d5d373)

### Fixed

- Fix auto-sent error reports dropping when fired before the Tauri handle exists (f069a712b)
- Align Size column icons flush right (1d5f661a3)

### Non-app

- Add error-report endpoint on api server with R2 presigned-URL handoff (1a2ea1c04, f78f76aff)
- Add shared PII redactor for crash files and error-report bundles (1d719f36e, b64e2c2c7)

## [0.13.0] - 2026-04-22

### Added

- SMB copies ~30× faster on high-latency links (100×10 KB over ~60 ms RTT: ~28 s to ~1 s) (940905552, 9d6df0e9f,
  4009b9baa, 77ea6e81c)
- Add SMB concurrency setting (default 10, range 1–32, live) (7fdd85e3e, aa331c4e8, f46d45e4a)
- `..` row shows current folder's totals, not parent's (36212edee)
- Full mode shrink-wraps Ext/Size/Modified to give Name every spare pixel (7325c8f8f)
- Brief mode shrink-wraps each column to its widest filename (c336dbbae)
- Filename tooltip on truncation in Brief and Full (f37d7e51f)
- Volume tooltip on tabs (b66639886)

### Fixed

- Security: bump smb2 to 0.7.2, fixes a crafted DFS referral crashing Cmdr (7e7eaf76f)
- Fix small SMB uploads ignoring cancel (f948731c7)
- Fix click-on-cursor eating the next drag (cccf00958)
- ⌘C now copies selected text when there's a text selection (47f03b209)
- Block dropping a folder onto itself or its descendants ("Can't drop here" feedback); `..` accepts drops (b7c3d9609)
- Fix frontend hot reload (swap UnoCSS for unplugin-icons) (00906566a)

### Changed

- Internal: cross-volume copies flow through stream API (plus APFS clonefile fast path); batch copies run in parallel
  per-backend (eb99c37c5, 508a0fe1f, 50b7221ef, 39c71eede)
- Move smb2 from git to crates.io, bump through 0.7.1 and 0.7.2 (96f4bbd3a, 0ec95a79c, 7e7eaf76f)

### Non-app

- Run Docker SMB integration tests on every push (26 tests against real servers) (257269bbd)
- Byte-level blake3 hash verification on every SMB copy test (fd5a2d843)
- SMB copy soak harness: 30-min Docker run, 41,984 iterations, zero drift (3a9b58f2f, 6a9e046de)
- Add changelog-commit-links check (surfaced and fixed 8 bad links) (4e2813034)

## [0.12.0] - 2026-04-18

### Added

- Add friendly error pane for listing failures (provider-aware suggestions for Dropbox, Drive, OneDrive, iCloud,
  MacDroid, VeraCrypt, etc.) (eec50ff5e, cc7bb319a)
- Live disk-space updates in status bar (configurable threshold, 3 s timeout) (d67dd3824)
- Add "Copy path" to breadcrumb context menu (eb4d3c921)
- Add SMB streaming reads/writes (MTP↔SMB and SMB↔SMB copies skip temp files, ~1 MiB peak RAM) (ac71bd0af, a82709091,
  35120da0a, 043597f86)
- Disambiguate same-named SMB shares per server (76671bf5c)
- Inline SMB login form on direct-connection upgrade (b315b4211)
- Instant dialog open for large selections (50k-file Copy/Move: ~10 s to ~1 ms) (48ea60303)
- Add MTP Samsung support (phones reporting 0 storages at connect time now appear) (14b3ac3fe)
- Batch MTP scan for copy (one USB call per parent dir, not per file) (70978c84c)
- Skip rename extension warning on case-only changes (photo.JPG to photo.jpg) (1401017d5)
- Split filename + extension in Full view (275d0918a)
- Volume selector polish (clickable spacebar area, no clipping over F-key bar) (700eac4a1)
- File-op dialog polish (thousand separators, mid-text truncation, fixed 500 px width) (d67dd3824)
- Add debug-window error-pane preview with all 47 error states (cc7bb319a)

### Fixed

- User cancels no longer log as ERROR (6f793929c)
- Fix copy/move crash from a reactivity race (0cdd7d7eb)
- Fix stuck "Scanning 0 files" transfer dialog (dd06d680c)
- Fix double-dispatched MCP autoConfirm copies (4af22ab53)
- Fix file watcher panic on 500+ external changes (4087e30e7)
- Match Finder for copy space checks (count APFS purgeable space) (34546567a)
- Fix SMB paths with spaces, serialize concurrent manual-server writes, fix viewer search after emoji/CJK (97c048185)
- Fix SMB port handling and human host display for mDNS names (c26f7e873, 017b7043b)
- Fix "Connect directly" on QNAP (2666db8a0)
- Hide Clear-index button when there's no index (fixes AA contrast) (b1915d9b7)
- Network pane no longer sticks on old host after mount (41c186091)
- Fix llama-server startup on Linux with locked keyring (encrypted-file fallback) (55ccde30b)
- Fix nested-runtime panics on MTP/SMB (async Volume trait end-to-end, runtime-safe MTP reads) (531bb9b91, 9d4982a85,
  694ddc126, 1598f8cf9)

### Changed

- Cancelled SMB uploads skip the server flush (~100 ms to 1 s saved per cancel) (6fa07801a)

### Non-app

- Add design-time WCAG contrast checker (resolves CSS vars and color-mix chains, replaces flaky axe rule) (db25f0d33,
  55af25814)
- Fix 18 real WCAG AA contrast failures (747507f11, 67d42ba39, 4a15a53d5)
- Add tier-3 component-level a11y tests (61 files, 146 tests, ~6.3 s) and a11y-coverage check (33300a4f9, d56c1dfe2,
  398bf7a5f)
- Switch Lucide to UnoCSS pure-CSS icons (93548fa62)
- Add file-length check; split 20+ long files into sub-800-line modules (7514cb4ec, 2939bfee4, 4514a8322, 315609a32)
- Run Linux E2E in Docker (8803c3c67, f39177c2f)
- Drop CrabNebula/WebDriverIO macOS E2E suite (Playwright covers all 15) (4cecfb917)
- Upgrade rustls-webpki 0.103.12 (RUSTSEC-2026-0098/0099) and bitstream-io 4.10.0 (3734502a9)
- Add docs/error-handling.md contributor guide (a4a5fdb51)

## [0.11.1] - 2026-04-10

### Added

- Add striped-rows setting (alternating row shading in Full and Brief) (faa253490)
- Add MTP per-file copy progress and instant mid-file cancel (~300 ms via USB SIC abort) (ac5ec4df5, a66adf673)

### Fixed

- Sync View menu Full/Brief checkmarks across panes (6e36a49be)
- Stop MTP `ObjectNotFound` log spam on every copy (0cc675a6b)
- Fix MTP mid-stream cancel corrupting USB session (mtp-rs 0.11.0) (a66adf673)
- A11y: darken accent-text for WCAG AA, fix search placeholder opacity (b7744dd9a)
- Fix Linux compilation (cross-platform SMB types, get_smb_mount_info) (00c5f1849)

## [0.11.0] - 2026-04-10

### Added

- Add SMB direct connections via smb2 (~4× faster, OS mount stays for Finder/Terminal) (dea46ecce)
- Auto-upgrade existing and new SMB mounts to direct connections in the background (a6ab2ca17)
- Add "Connect to server" for SMB by hostname, IP, or `smb://` URL (persisted, context-menu Disconnect/Forget)
  (2df24aca3)
- Add SMB connection status indicators with one-click upgrade (04732500d)
- Real-time SMB transfer progress with end-to-end cancel (f5303551b)
- All SMB write ops (create, delete, rename, copy, move) through direct connections with full conflict handling
  (e72c08284, 4f030d7fa)
- Unified SMB/MTP change notifications with incremental cache patches (2d0bc9865)
- Warn in transfer dialog when using slower OS mount (d25de484a)
- Auto-suppress ptpcamerad on macOS for MTP (d161f9b1f)
- Add MTP settings (disable toggle, "Don't show again" toast, dedicated section) (2467ecef4, 70d8d40a5)
- Brief mode shows real recursive directory sizes in selection info (53ee5efb3)
- Cursor jumps to newly created directories (eff84d175)

### Fixed

- Fix per-file copy progress (counts files, not top-level items) (d10d9cc0e)
- Faster SMB deletes (skip stat round-trip) (0e7f07276)
- Copy cancellation checks between every file in tree copies (a7d401ac4)
- Fix cross-volume copy misclassifying SmbVolume as local (4a86a85c7)
- Fix SMB paths with accented characters (NFC normalization) (baaccc842)
- Resolve SMB IPs to hostnames via mDNS so Keychain finds saved credentials (b1addfd2f)
- Show login form on stale Keychain credentials instead of empty share list (46609f165)
- Block navigating above SMB mount root, fall back to home when unreachable (d25de484a)
- Fix stale cursor index after file ops (945093bc3)
- Fix drag & drop after wry upgrade (a816c77c2)
- Fix stale dir sizes after copy/create (1479108e5)
- Fix scan-preview race in progress dialog (5d9b91bd1)
- Fix dir_stats count drift on file/dir type changes (364ddf15e)
- Fix index entry ID race via shared atomic counter (6e173e45c)
- Fix MTP move not refreshing UI on Linux (mtp-rs 0.9.1) (5b27ead1f)

### Non-app

- Replace smb/smb-rpc crates with our own smb2 (2d7904f00)

## [0.10.0] - 2026-04-08

### Added

- Visible copy rollback (progress bars count back, Cancel stops the rollback) (0ac5d05f4)
- Dual progress bars in transfer dialogs (size + file count) (ced9d253a)
- MCP: cmdr://settings resource and set_setting tool (c71115823)
- MCP: move_cursor awaits frontend confirmation (6341c2552)

### Fixed

- Fix MTP move conflicts silently overwriting (27f2ff0bc)
- Fix MTP watcher missing external file changes (266026d5a)
- Fix MTP event debouncer dropping suppressed events (21b3bc5f1)
- Fix MTP pane falling back to local root after copy (9deba727a)
- Fix MTP volumes missing from copy/move dialog (cd66031ed)
- Fix MTP event-loop lock contention blocking copy/move/scan (0461e33a3, 547a41315)
- Fix MTP scan preview showing 0/0/0 in confirmation dialog (4e1efab7e)
- Fix MTP rename conflicts not showing dialog on non-local volumes (25f2b263e)
- Fix copy "Cancel" (keep partial files) triggering unintended rollback (3042f2349)
- Fix copy cancel hanging 30+ s on network mounts (816e9e12b)
- Fix UI blocking on network filesystem ops (bed59dbee)
- Fix indexing replay progress showing "Scanning..." instead of replay overlay (32c05393d)
- Push-based volume selector, fixes mount/unmount races (b09665927)
- Fix volume path resolution to <1 ms regardless of mount health, handle APFS firmlinks (5a1f78cb1)
- Harden unsafe Rust (main-thread markers, scoped Send impls, SAFETY comments) (541804c3e)

### Changed

- Typed write-op errors (9 variants) replace string parsing (c10e0614c)
- Typed MTP volume errors (8f2296a42)

### Non-app

- Backend owns MTP move strategy, frontend no longer orchestrates (547a41315)
- Demote noisy per-file copy/move/MTP logs from INFO to DEBUG (357feff0d)
- Fix all WCAG violations found by axe-core (d29a7cddd, 4380469ef, 6e6230837)
- Port E2E tests from WebDriverIO to Playwright; add 80+ tests (MTP, SMB, conflicts, a11y, indexing) (77d059373,
  7d58bd6c1, 4f83aeb81)
- Replace Prettier with oxfmt (10–20× faster) (995f8c8e5)
- Split indexing module (1951 lines) into focused files (39086418a)
- Add light/dark website theme, features page, OG images, blog Like buttons (49dbe7828, 98bdcc356, 56a9e764a, 5cff7c356)
- Dashboard: color-coded charts, GitHub star tracking, error reporting (4b7c9e1ef, 67efc4ae6, 2e26b956b)

## [0.9.1] - 2026-03-24

### Fixed

- Fix orphaned llama-server processes after rapid AI provider switching (b3382efe9)
- Fix vendor-specific MTP detection (Kindle, USB class 0xFF) via mtp-rs 0.4.1 (1a170dbd9)

### Non-app

- API server: migrate telemetry to D1, add crash email notifications via Resend, rename license-server to api-server
  (7dc0da232)
- Split search.rs (2361 lines) and SearchDialog.svelte (1552 lines) into focused modules (c17c210c2)
- Deduplicate repeated patterns across Rust, Svelte, TS, and Go (52afe37ab)
- Bump 9 Rust deps (reqwest 0.13, rusqlite 0.39, notify-debouncer-full 0.7, etc.) (929556f22)
- Skip pnpm install when lockfile unchanged (~20 s saved per run) (8d2b39b84)
- Blog: add Kindle support article (5c9d5b168)

## [0.9.0] - 2026-03-23

### Added

- Add whole-drive file search (⌘F): glob/regex, size/date filters, scope, AI mode, MCP search and ai_search tools
  (058136398, 15110c0df, 8c3546dcc, cf5827b1d, 415db3f16, 21d32ef15, 26d682cd4)
- Add opt-in crash reporting (panic hook + signal handler, inspect-and-send dialog, no PII) (016ee3a53, be29affc9)
- Add Shift+F4 (Total Commander style): create new file, open in default editor (da8ca9343)
- Add smart size display (min logical/physical, dual-size tooltips, hardlink dedup, mismatch icons) (1d666a70d,
  b302d0eb0, 065820014, 1d588f841, a93a8bb21, 9c450cdc3)
- Add sortable Ext column in Full mode (e834b4cb1)
- Add replay progress overlay during cold-start (f166b063e)
- Show live MTP disk space in volume dropdown and status bar (b155f1f8a, c4cc26f27)
- Show MTP loading progress on large folders (77ebaa00c)
- Add focus indicators on search and command palette inputs (1792216b9)
- Selection summary includes directory sizes (3928c1c98)
- MCP: show directory sizes in state resource (9cb77509a)

### Fixed

- Fix multi-GB macOS memory leak (ObjC calls on background threads now run inside autoreleasepool) (777f9ec37)
- Fix stack overflow in sync status (8 MB OS threads instead of rayon for NSURL/XPC calls) (fa28cd43e)
- Fix size overcounting (hardlink dedup, exclude cloud-only files, smart-size for dataless) (fe5eff72b)
- Fix file watcher: instant updates in large dirs via incremental diffs (df558e8ba)
- Fix selection clearing after file ops; gradual deselection per source item (538ec5ac1)
- Fix selection indices drifting after external file changes (453ec02b1)
- Fix cursor lost after deleting all files (17808d4b6)
- Fix stale dir sizes on rename (10213d845)
- Fix indexing not starting on fresh DB (a61376d69)
- Fix "Scanning..." stuck after replay (4a44d7dfb, fb796e725)
- Fix verifier + replay transaction conflict via named savepoints (72ca9fbb9)
- Fix MTP browsing panic; show device name on single-storage devices (d37b8a5f5)
- Fix MTP duplicate directory listing on connect (17efe8be3)
- Fix MCP stale state after server crash; auto-probe port when configured port is in use (0369d219b, d69f87617)
- Fix OpenAI compatibility (795a6775b)
- Hide misleading rollback button for move ops (fbdba5b42)
- Raise replay/journal gap thresholds to reduce unnecessary full rescans (377919863, af2bf7a7a)

### Non-app

- Add full-stack analytics dashboard (6 data sources, agent-readable report) (b4f740a18, 0766c4b7a, b97028f61)
- Enforce CSS design tokens via Stylelint (50f2b4220, e3259b0a1, 36b3408cd)
- Drop desktop smoke tests, speed up store tests by ~20 s (c6210ae4e, dab071f5f)
- Reduce code duplication across write ops, listing, events, search dialog (33ec2f279)
- Website: story + testimonials sections, landing page polish, Docker healthcheck, Remark42 CSP (d5a7f430d, 51acd88cc,
  424a8075d, dd5e34038)
- Bump mtp-rs to 0.2.0 (63425523b)

## [0.8.2] - 2026-03-15

### Fixed

- Fix crash on launch after auto-update (kernel code-signing cache SIGKILL: temp + rename for a fresh inode) (d2923aff2)
- Fix indexing drift: per-navigation verifier with 30 s debounce; skip /System and /dev as empty stubs (0f28b51ec,
  b0b173055)
- Fix dir size display during indexing (refresh on aggregation-complete, not scan-complete) (d0746fbbd)
- Fix navigation latency: fire-and-forget verification, parallelize 6 listen() calls (a4e87f1a4)
- Fix indexing perf (integer-only index: 25 min to seconds on 5.1M entries; 99% replay-event dedup) (a5b5beb74,
  44fecd666, d9877c147)

### Non-app

- Separate dev and prod log dirs, fix Linux test output capture, fix smoke test timeout (e8762be4b, 83d23655a,
  88901f918)
- Improve agent instructions (dec19cf4d)

## [0.8.1] - 2026-03-14

### Fixed

- Fix indexing (lock-free dir-stats reads, drop stale PathResolver cache, fix "DB is locked", fix overlay race, lost
  scan metadata, dir→file replacement orphans) (50bd4faa3, 44abfd10b, 7319c5c41, 26785fcd7, 795e48b70, 424eedb3f,
  dbccec1b9, 8f87a4f58)
- Fix traffic light position in production builds (7551df2f7)

### Non-app

- Add indexing concurrency stress tests, event loop tests, reconciler tests (3ad3adc99, 8a084cdaf, dbccec1b9)

## [0.8.0] - 2026-03-13

### Added

- Add custom macOS updater that preserves Full Disk Access (syncs into existing .app bundle, privilege escalation)
  (190a63776)
- Add MTP delete, rename, move (full progress, cancel, dry-run) (812ad0733)
- Add breadcrumb polish ("/" prefix, "~" for home) (44b710561)
- Add auto-rescan on FSEvents channel overflow (ca7cece36)
- Add index debug dashboard (DB stats, watcher status, event-rate sparkline) (7510ec392)

### Fixed

- Fix indexing (interrupt-safe reconciler, stop micro-scans, faster bulk inserts, false FSEvents deletes, missing dir
  sizes after replay, periodic DB vacuum) (31df59e6c, 981b31134, da742904d, f0c225f49, bf0b47f22, d125a2415, 67684bbb8)
- Fix drag swizzle failing on wry 0.54+ (2680bae89)
- Fix MCP live start/stop UX (backend state as ground truth, port auto-check) (f4c107aa1)
- Fix MCP server not stopping on app quit (61fe290ae)
- Fix traffic light position in production builds (b74ed395e)
- Fix scan overlay showing stale state (218bcb984)

### Non-app

- Vendor cmdr-fsevent-stream fork as workspace crate (8b937a6bc)
- Fix two FOUC flickers on website page load (8c21ac78b)
- Set up self-hosted macOS GitHub Actions runner; add index DB query tool, website deploy workflow extracted (665f63a9a,
  37f106292, 5744636f3)
- Pink title bar in dev to distinguish from prod (d2c9ae412)

## [0.7.1] - 2026-03-12

### Fixed

- Fix scan overlay stuck at 100% after directory size aggregation (424eedb3f)

## [0.7.0] - 2026-03-12

### Added

- Add AI settings: three providers (off / cloud / local LLM), 15 cloud presets, per-provider keys, model combobox, RAM
  gauge, context size (b41365b3d, abfc24818, 423e669fe)
- Live MCP server start/stop in Settings (no app restart) (e0c55e731)
- Add stale index detection with toast + auto-rescan (b590a54e6)
- Add device tracking for license abuse, fair-use terms in ToS (cf4f91387)
- Add license section to Settings (status display, action buttons, dynamic labels) (39cf7b4b3)
- Improve app icon for macOS Sequoia (cc80d2807)

### Changed

- Drop supporter license tier (legacy keys map to Personal) (c0a63f57a)
- Split Settings UI horizontally 50/50 (9493f880f)
- Rename settings-v2.json to settings.json (d987cc8fe)

### Fixed

- Fix startup panic from blocking_lock in async context (f9855ca01)
- Fix SQLite write pragmas on read-only connections (panic in subtree scans) (a53a2753c)
- Fix llama-server not stopping on quit, stale PIDs, excess memory (256k to 4k default context) (eae70f108, ffcbc8185,
  e45c742ac)
- Fix Settings UI freezing ~5 s when stopping AI server (instant SIGKILL for stateless llama-server) (2af7ee822)
- Separate dev/prod data dir and MCP port (b8b058a20)
- Fix fallback path resolution falling to / instead of ~ (8d7c64417)
- Fix indexing (100× faster aggregation, DB auto-vacuum, truncate before full scan) (47a2e8ef7, cad1af562, aff2046e0,
  96323e976)
- Fix FSEvents storms causing memory pressure (mimalloc, 1 s dedup window) (207ddee1d)

### Non-app

- Replace 19 ADRs with colocated Decision/Why entries in 11 CLAUDE.md files; slim AGENTS.md from 245 to 93 lines
  (ccf5cc7ff, d297a1a82, 059579614)
- Website: version + file size on download buttons, fix Intel/Apple detection flicker (bd170563c, ec35b1f75)
- Add html-validate and circular-dep checks (3dbd5af55, 4bead2b97)
- Eliminate all circular deps via refactor (volume grouping, menu platform code, viewer scroll/search) (7740fbc41,
  8522e71ff, e16bd9181, 7ed1cea1f)

## [0.6.1] - 2026-03-10

### Added

- Add top menu icons (1a2621af6)
- Add View, Copy, Move, New folder, and Delete actions to context menu (a966f1745)

### Fixed

- Fix OOM crash from unbounded indexing buffers; toggling Full Disk Access could replay millions of FSEvents with zero
  backpressure, consuming 500+ GB RAM. All buffers are now bounded (~350 MB peak), with a memory watchdog that stops
  indexing at 16 GB (f1501ece7)

### Non-app

- Website: add llms.txt, Schema.org JSON-LD, and auto-generated sitemap for agent accessibility (ba64c362f)
- Website: update roadmap (51971200a)
- CI: simplify release pipeline, download sigs directly from release, generate `latest.json` with `jq`, validate all 3
  sigs before proceeding (d3095cbca, 5b82cd012)
- CI: fix Backspace E2E test on WebKitGTK, fix CI failures, fix 3 flaky tests (7c22951a3, 79f593cbd, 8f4ea825e)
- Docs: add troubleshooting section to releasing guide (1768b29a6)

## [0.6.0] - 2026-03-08

### Added

- Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
  inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native file
  icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no system
  keyring, distro-specific install hints, USB permission handling (b6e80f6b6, 20be0c383, 9c51fa9b6, 64e41f9d4,
  40cc1a982, c3ad1ed50, d40ea2568, 60063ece0, e65d993c1, 22e2ea794, afe260907, 4bbcbb090, 48af543be)
- Add per-pane tab support: ⌘T/⌘W, ⌃Tab cycling, pin/unpin, context menu, persistence with migration, per-tab sort
  (791a29a90)
- Add delete/trash feature (F8): trash by default, ⇧F8 for permanent delete, confirmation dialog with scan preview,
  batch progress with cancellation, volume trash support detection (e3560a36e)
- Add clipboard for files: ⌘C/⌘V/⌘X with Finder interop, ⌥⌘V for "Move here", cut state tracking, text clipboard in all
  windows via NSPasteboard (0dc295367, 60baebad0)
- Add toast notification system with centralized store, dedup, stacking, three levels, transient/persistent modes
  (6c5c4525e, 2329f2f53)
- Add per-pane disk space display: 2px usage bar, free-space text in status bar, mini bars in volume dropdown
  (9b6d0579f)
- Add custom tooltips with glass material effect, shortcut badges, smart positioning, accessibility support, replacing
  all native tooltips (3c7f9654e)
- Add drive indexing with integer-keyed DB schema (7.4x size reduction, 3.8 GB → 0.54 GB), LRU path cache,
  platform-aware collation, recursive CTE aggregation (7c5d3ce15, daee97b09, 5e10fa9fb, 68be3abb1)
- Add IPC hardening: timeout-protect all filesystem commands, transparent timeout UI with retry/fallback for volumes,
  tabs, file ops, and viewer (6a5827882, 71de96e14)
- Add accent color option in Settings: macOS theme or Cmdr gold, "Recolor to gold" for folder icons (330e8245d,
  ef9de79df)
- Add directory sorting by size with toggle in Settings (a7dd8cae1)
- Add "Forget saved password" UI for SMB network shares (7d751d533)
- Add path validation in copy/move and mkdir dialogs with platform-correct limits (6b295ec4b)
- Add centralized keyboard shortcut dispatch with runtime custom bindings (e40bcc2f0)
- Add macOS entitlements and TCC usage descriptions for proper permission prompts (ff0c27ee4)
- Add Apple code signing, notarization, and arch-specific downloads (aarch64, x86_64, universal) (b03f91eca, 944085fbf)
- Add licensing UI improvements: verify/commit split, typed errors, short code in signed payload, Paddle live setup
  (0abc7049d, 1f2308bee)

### Fixed

- Fix file viewer: search progress bar with spinner and stop button, incremental match delivery, 10k match cap,
  byte-seek navigation, loading very long files (9c0a3c390, a3b9d0eec, 31cf5fdc3, d15ecded1, 86ef2a5e7, 0fcdb13c4,
  8b57bbe6f)
- Fix 3–10s startup block from index enrichment holding the mutex (267e02b8b)
- Fix mDNS host resolution arriving before discovery, causing SMB auth failures (2dda99b68)
- Fix focus escaping panes with focus guard, removing ~50 redundant refocus calls (4c9aadc9e)
- Fix clipboard shortcuts in text fields on macOS (20f3de028)
- Fix non-blocking navigation on slow/dead SMB shares with timeouts and optimistic UI (c85c8c269)
- Fix copy feature: auto-rollback on panic, deadlock prevention, cancel race condition (2b17ab557)
- Fix status bar not refreshing after file watcher diffs (e880f9f7b)
- Fix pinned tab volume change now opens new tab instead of navigating in-place (ff4c8f2ff)
- Fix cancel-loading to return to previous folder instead of home (8ff237983)
- Fix ⌘, to refocus Settings window if already open (71b3e612d)
- Fix Settings: ⌥+key shortcuts showing "Dead" on macOS, key filter subset matching, ESC clears filter (1fd540a0a,
  5056bb6bb, 47050e02f)
- Fix settings not initialized warning at startup (b540fcc50)
- Fix SMB share showing 0 bytes free on network filesystems (f791153be)
- Fix volumes cached to prevent timeout at startup (024e48f2f)
- Fix top menu items staying enabled on non-main windows (7572d1309)
- Fix live file count during large folder loading (7815d0fb2)
- Fix window content height for production builds (0cbd0fd2a)
- Fix folder icons updating on OS theme change (6b0244535)
- Fix focus lost after rename cancellation (edace1893)
- Fix file viewer not loading settings (acfef93bc)
- Fix drive indexing: orphaned entries, missing dir sizes, background scan failures, DB transaction issues (323ae8668,
  004f30260, c331143d4)
- Fix MCP protocol version mismatch warnings at startup (2af0b901e)
- Fix arrow up/down performance in large folders (e6f268c3d)
- Fix PostHog CSP and make it cookieless (1700d9999, 9cea85aae)
- Fix app loading slowly due to startup optimizations: license cache, async validation (3835866cd, 87de13691)

### Non-app

- Overhaul native menus on macOS and Linux: build from scratch, strip macOS system-injected items, unify dispatch via
  single event, context-aware graying, full accelerator sync (b38c552b1)
- Unify frontend + backend logging via tauri-plugin-log, demote noisy log levels, suppress smb/sspi noise (22f4ab5b1,
  dbbcc551c, 1e59a5646)
- Design system: unified button styles, consistent loading states, improved text readability, redesigned network screens
  (8dc2e33c7, 4d07ad0be, 71dbe0be4, b5d8b2807, a018a3ec1, 90e20104f)
- Docs overhaul: CLAUDE.md staleness checker in CI, enriched 25 CLAUDE.md files with Decision/Why entries, cross-cutting
  patterns in architecture.md, split infrastructure.md into per-service files (ff8b3be25, 347ae9bd3, f961f1950,
  2f7bff1a6)
- Website: add blog with first post, PostHog and Umami analytics, arch-specific download buttons, Docker build check,
  newsletter improvements (01681c195, 75d52283f, 78de57315, ae8f6cb99, 34ecc703e)
- Check runner: CSV stats logging, cfg-gate enclosing block scope detection, file length check, flag combining fix
  (9ac4b54b8, 539db62fb, 4a2456235, 6fe48a96e)
- Refactors: split DualPaneExplorer and FilePane, extract dialog state, deduplicate templates and Settings CSS, split
  tauri-commands (337f6207f, cfae0db41, dad8790c7, 35a42394f, ba86d8742)
- License server: download tracking via Cloudflare Analytics Engine (ef0f04944)
- Add Renovate for automated dependency updates (00880a0cd)
- Add macOS Playwright E2E tests and CrabNebula E2E tests (ec900eec4, a768c030a)
- Infra: uptime monitoring with UptimeRobot + Pushover, hardened deploy script (19baefd10)
- Add cfg-gate lint check for macOS-only Rust crates (075c1d4a7)

## [0.5.0] - 2026-02-15

### Added

- Add file viewer (F3) with three-backend architecture for files of any size, virtual scrolling, search with multibyte
  support, word wrap, horizontal scrolling, and keyboard shortcuts (79268a4c5, 9f91bce04, b10002a98, 2ad2521b2,
  b65c422f4, 43adc86ca)
- Add drag-and-drop into Cmdr: pane and folder-level targeting, canvas overlay with file names and icons, Alt to switch
  copy/move, smart overlay suppression for large source images (1ad149325, 6207d8e2c, a89f18fb6, 371746bb1, a3eae1cf3,
  c776eed91, e97d3db74)
- Add settings window (⌘,) with declarative registry, fuzzy search, persistence, keyboard shortcut customization with
  conflict detection, and cross-window sync (db121f6d3, 418f79029, 8f78596cf, 218b79b7f, 9c39db32a, 4e90137df)
- Add MTP (Android device) support: browsing, file operations (copy, delete, rename, new folder), USB hotplug,
  multi-storage, MTP-to-MTP transfers (938e87c4e, 672fa6e55, d1e9f8027, 7ac1528b7, b08af36f5, ea845a661, fd8dad669)
- Add move feature (F6) reusing the copy UI as a unified transfer abstraction (682d33a24, cb9e04710)
- Add rename feature with edge-case handling (62799c6a9)
- Add swap panes feature with ⌘U shortcut (2a1b32964)
- Add local AI for folder name suggestions in New Folder dialog, optional download (b9a112ed2, 3dc19c092)
- Add chunked copy with cancellation and pause support on network drives (ba5409ef6)
- Add 6 copy/move safety checks: path canonicalization, writability, disk space, inode identity, name length, special
  file filtering (954802281)
- Add sync status polling so iCloud/Dropbox icons update in real time (ed3615826, 629641257)
- Add CSP to Tauri webview for XSS protection (68bd510bc)
- Add copy/move folder-into-subfolder warning with clear error message (521ab5e84)

### Fixed

- Fix panes getting stale when current directory or its parents are deleted (1b5ad52aa)
- Fix multi-window race conditions that could crash the app (9a33e24bf)
- Fix recovering from poisoned mutexes instead of crashing (56 lock sites) (62fd6852a)
- Fix wrong cursor position after show/hide hidden files (223b041e5)
- Fix selection and cursor position breaking on sort change (36d61d086)
- Fix panel unresponsive after Brief/Full view change (2b6d51318)
- Fix copy operationId capture race condition (9b5c57c13)
- Fix $effect listener cleanup race in FilePane (e2c6ee12a)
- Fix condvar hang on unresolved conflict dialog (2975c4501)
- Fix first click on main window not changing file focus (59c5da485)
- Fix AppleScript injection in get_info command (e3378c355)
- Fix URL-encoding of SMB username in smbutil URLs (f908a74b8)
- Fix mouse/keyboard interaction bug for volume picker (8afd0de65)
- Fix drop coordinates when DevTools is docked (a9a041f17)
- Fix MCP server always returning left pane as selected (2f9160a51)
- Redact PII from production log statements (fe31316f9)

### Non-app

- Migrate network discovery from NSNetServiceBrowser to mdns-sd: 68% code reduction, no unsafe code (3d44cf170)
- Rewrite MCP server with fewer tools but more capabilities, auto-reconnect, and instructions field (1061fad78,
  ede6463a2, 82345d18d)
- Introduce ModalDialog component for all soft modals with drag support (ffbf14a79)
- Major refactors: split DualPaneExplorer, FilePane, volume_copy, listing/operations, connection modules (04dc3debd,
  e14c28930, 2da8e6dd7, c0bd500b8, 707a96a92)
- Security: pin GitHub Actions to commit SHAs, fix Paddle webhook timing attack, use crypto.getRandomValues for license
  codes, HTML-escape license emails, add webhook idempotency, constant-time admin auth (c0d8cc31e, 70bc59481, 51cd0b57b,
  bea3b2a96, 9db450b7a, b82f857a4)
- Docs overhaul: add colocated CLAUDE.md files throughout repo, architecture.md, branding guide (eac9e6187, dd91c7883)
- Website: add changelog, roadmap, newsletter signup with Listmonk + AWS SES, mobile responsiveness fixes, 512px logo
  (643de6ad9, 07936d1d2, ba4812d54, aa661cff2)
- Add dead code check, manual CI trigger, pnpm security audit, LoC counter, summary job for branch protection
  (9876600d5, 3b20e6602, ad22eba93)
- Tooling: extract shared Go check helpers, add VNC mode for Linux testing, fix Linux E2E environment (550c35368,
  6aa5ff7ce, fa907b6b1)
- License server: add input validation, webhook idempotency, and security hardening (4363a3203, 9db450b7a, 7398965b5)

## [0.4.0] - 2026-01-27

### Added

- Add file selection: Space toggles, Shift+arrows for range, Cmd+A for select all, selection info in status bar
  (4d44cda03, 1cac4b310)
- Add copy feature with F5: copy dialog, destination picker with free space display, conflict handling (281f45ee4,
  fb5f0275f, a6d148d83, 6c661f296)
- Add new folder feature with F7 shortcut and conflict handling (80ec297d1)
- Add "Open in editor" feature with F4 shortcut (7eb66aca7)
- Add function key bar at bottom of UI for mouse-initiated actions (537e0405b)
- Add pane resizing: drag to resize between 25–75%, double-click to reset to 50% (542b49108)
- Add multifile external drag and drop (742633446)
- Add keyboard navigation to network panes: PgUp/PgDn, Home/End, arrow keys (70aa3410e)
- Add "Opening folder..." loading phase for network folders with distinct status messages (9eb1185e1)
- Add license key entry dialog with organization address and tax ID collection (52480cefa, 29eb6fe1f)

### Fixed

- Fix UI not updating on external file renames (5de93465f)
- Fix light mode colors (42888c707)
- Fix cursor going out of Full view bounds (7edcac89e)
- Fix ESC during loading navigating to wrong location (b8c12e780)
- Fix focus after dragging window (8488de6a3)
- Fix multiple volume selectors opening at once (f4c4c2145)
- Fix frontend race condition from refactor (646c7af3b)

### Non-app

- Add E2E tests with tauri-driver on Linux using WebDriverIO in Docker (1b0cbac5f)
- Revamp checker script: parallel execution, dependency graph, aligned output, colored durations (7835b4cb2)
- Add type drift detection between Rust and Svelte types (b3ae1c3ff)
- Add jscpd for Rust code duplication detection, CSS health checks, Go checks (67e6c15af, d177eb368, 254075236)
- Add Claude hooks for pre-session context and post-edit autoformat (3d59ddea1, 122182d6c)
- Add LogTape logging for Svelte and debug pane for dev mode (affa5482d, f494e15f0)
- Require reasoning in clippy lint exceptions (d327cf49e)
- Website: fix hero image animation and sizing, fix broken Paddle references (40faeeef3, 278ad4c8d, 5eb5a523a)
- License server: wire up Paddle checkout, fix webhook email fetching, support quantity > 1 (3c40929cc)

## [0.3.2] - 2026-01-14

### Fixed

- Fix auto-updater to download updates and restart the app after updating (c0bff9a67)

### Non-app

- Website: redesign with mustard yellow theme, view transitions, hero animation, and reduced motion support (0296379a0,
  18b729fda, 689a15131)
- Website: avoid aggressive caching, rearrange T&C (8ca05395d, c92dff8c8)
- Tooling: turn off MCP stdio sidecar, fix Rust-Linux check, reduce CI frequency, fix latest.json formatting (5dda608a5,
  2ec3f7e1d, 42d81ab95, 52980aecd)
- Docs: release process and auto-updater documentation (c7c36f602, 765f5ad05, f3785da7b, 10e43de7e)

## [0.3.1] - 2026-01-14

### Added

- Add custom title bar, 4 px narrower for more content space (33e90c8b2)

### Changed

- Replace rusty icon with yellow one (79777e34b)

### Fixed

- Fix app name in task switcher: shows "Cmdr" instead of "cmdr" (8117300e8)

## [0.3.0] - 2026-01-13

### Added

- Add MCP server with file exploring tools (f6dcf2738)
- Add stdio MCP interface for broader client compatibility (3b193f7c1)
- Add Streamable HTTP support to MCP server (1d0549b0d)
- Stream folder contents for blazing fast experience (1d82ec9f0)
- Add "listing complete" state showing file count (5059e00bb)
- Add Linux checks to checker script (02ab0ab7d)

### Fixed

- Fix MCP server port and tool naming (c2ae7de7a)
- Fix race condition when loading files (38865e623)

## [0.2.0] - 2026-01-10

Initial public release. Free forever for personal use (BSL license).

### Added

- Dual-pane file explorer with keyboard and mouse navigation (c945f18c7)
- Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column), switchable via ⌘1/⌘2
  (c779a6de9)
- Virtual scrolling for 100k+ files (cf6c35d01)
- Chunked directory loading (50k files: 350 ms to first files) (869cdfb36)
- File icons from OS with caching (b8c588ec7)
- File metadata panel with size color coding and date tooltips (bc3dc85bb)
- Native context menu (Open, Show in Finder, Copy path, Quick Look) (7d977a12c)
- Live file watching with incremental diffs (cf1237280)
- Dropbox and iCloud sync status icons (46f1770d3)
- Volume switching with keyboard navigation (ba3e7704c)
- Network drives (SMB): host discovery via Bonjour, share listing, authentication, and mounting (54ee04f5f)
- Sorting by name, size, date, extension with alphanumeric sort (e7b720685)
- Back/Forward navigation (56a5bf61d)
- Drag and drop from the app (8e1d53b5c)
- Command palette with fuzzy search (7b0ea13c2)
- Window state persistence (position and size remembered) (b8d93c58f)
- Dark mode support (7deb986ba)
- Show hidden files menu item (4af855d7f)
- Full disk access permission handling (9f433d8b7)
- Licensing features (validation, about screen, expiry modal) (dc68eeb9a)
- Keyboard shortcuts: Backspace/⌘↑ (go up), ⌥↑/↓ (home/end), Fn arrows (page up/down) (fc899d4d7)
- getcmdr.com website (0f9eb2102)
- License server (Cloudflare Worker) with Ed25519-signed keys (bff3e8a2d)

---

### Development history

<details>
<summary>Click to expand full development history</summary>

#### 2026-01-10

Initial public release.

- Add licensing features to app (validation, about screen, expiry modal) (dc68eeb9a)
- Add command palette with fuzzy search (7b0ea13c2)
- Switch to BSL license (free for individuals) (06c49cba1)

#### 2026-01-09

License server improvements.

- Add checkout tester tool for license server (38774feb7)
- Add sandbox/live environment duality for license tests (15b395767)
- Unify trial period to 14 days (7e68c2763)

#### 2026-01-08

Cmdr, website, licensing.

- Rename to Cmdr (016a3e3c6)
- Restructure as monorepo with desktop app in apps/desktop (c0e764a72)
- Add getcmdr.com website (0f9eb2102)
- Add license server (Cloudflare Worker) with Ed25519-signed keys (bff3e8a2d)
- Add legal pages (privacy policy, terms, refund policy, pricing) (4f32a2987)
- Streamline CI (website-only PRs: 22 min → 2 min) (48940033f)

#### 2026-01-07

Network fixes.

- Fix network share unnecessary login prompts (dbeebaf90)
- Fix Back/Forward navigation across network screens (bf462e956)
- Sort network hosts and shares alphabetically (9de5f2b69)

#### 2026-01-05-06

Network drives (SMB).

- Add network host discovery via Bonjour (54ee04f5f)
- Add SMB share listing (693e92621)
- Add network share authentication (283e5fd0d)
- Add network share mounting (308d55ccc)
- Add volume mount/unmount watching (76bbf222b)

#### 2026-01-04

Sorting.

- Add sorting feature (name, size, date, extension) with alphanumeric sort (e7b720685)
- Add Stylelint for CSS quality (a778dccd8)

#### 2026-01-02-03

Navigation and permissions.

- Add ⌘↑ shortcut to go up a folder (848e2f1aa)
- Add full disk access permission handling (9f433d8b7)
- Add Back/Forward navigation with menu items (56a5bf61d)
- Add keyboard navigation to volume selector (46c302399)
- Save last directory per volume (9886fcdc8)
- Set minimum window size (237c5a922)
- Fix opening files (714dc5a2f)

#### 2026-01-01

Drag and drop, volumes.

- Add drag and drop FROM the app (8e1d53b5c)
- Add volume switching feature (ba3e7704c)
- Remove Tailwind (was slowing down app startup) (5354a48b1)

#### 2025-12-31

Polish.

- Add font width measuring for precise Brief mode layout (848f68fe9)
- Abstract file system access for better testing (eb9dd726e)
- Fix Dropbox sync icon false positives (64007f07a)
- Fix file watching reliability (aefe3e721)

#### 2025-12-30

Speed optimizations.

- Add keyboard shortcuts: ⌥↑/↓ for home/end, Fn arrows for page up/down (629899012)
- Move file cache to backend for major speed improvements (a42eda533)
- Optimize directory loading (phase 1 and 2) (7efd61a35)

#### 2025-12-29

View modes and cloud sync.

- Add Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column) (c779a6de9)
- Add Dropbox and iCloud sync status icons (46f1770d3)
- Add loading screen animation (234f0a703)

#### 2025-12-28

Performance and file operations.

- Add chunked directory loading (50k files: 350 ms to first files) (869cdfb36)
- Add file metadata panel with size color coding and date tooltips (bc3dc85bb)
- Add native context menu (Open, Show in Finder, Copy path, Quick Look) (7d977a12c)
- Add live file watching with incremental diffs (cf1237280)
- Add virtual scrolling for 100k+ files (cf6c35d01)
- Add Backspace shortcut to go up a folder (fc899d4d7)
- Scroll to last folder when navigating up (8ccd8bd81)

#### 2025-12-27

File metadata and icons.

- Add file metadata display (owner, size, dates) (d9994bc97)
- Add file icons from OS with caching (b8c588ec7)
- Add per-folder custom icons support (210f23beb)
- Add Tauri MCP server for AI tooling integration (0a64eb3c3)
- Fix symlinked directory handling (5a134ac44)

#### 2025-12-26

Dual-pane explorer.

- Add dual-pane file explorer with home directory listing (c945f18c7)
- Add window state persistence (position and size remembered) (b8d93c58f)
- Add file navigation with keyboard and mouse (20424e016)
- Add "Show hidden files" menu item (4af855d7f)
- Add dark mode support (7deb986ba)

#### 2025-12-25

Project init.

- Initialize Rust + Tauri 2 + Svelte 5 project (b410bd941)
- Add GitHub Actions workflow (6dbf26571)

</details>
