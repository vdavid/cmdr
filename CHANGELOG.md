# Changelog

All notable changes to Cmdr will be documented in this file.

The format is based on [keep a changelog](https://keepachangelog.com/en/1.1.0/), and we use
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Crash reports are now on by default. If Cmdr goes down, it sends the app version, your macOS version, and where the
  code stopped, never your files or anything about them. Crashes are invisible to me otherwise, so this one helps a lot.
  To turn it off, head to Settings > Updates & privacy. Sending your logs stays off unless you ask for it.
- Clicking "Later" on an update no longer ends the conversation: Cmdr keeps looking, picks up anything newer than what
  it already downloaded, and asks again about once a day until you restart

### Fixed

- Tell you to move Cmdr to your Applications folder when it's running from a spot it can't update itself from, like your
  Downloads folder or a mounted disk image, instead of quietly getting stuck on an old version
- Stop downloading a 63 MB update once an hour on an install that can never apply it

## [0.40.0] - 2026-08-24

The built-in agent now watches the file system and can suggest actions on its own, like "Hey, how about we move
`~/Downloads/puppy-eye-exam-result.pdf` to `~/papers/doggy/medical/`, looks like all similar papers live there". The
agent can only suggest, you approve/reject everything.

Also a bunch of performance and correctness fixes.

### Added

- Ask Cmdr watches your folders by default, waking on real disk activity and leaving no thread behind when it finds
  nothing (3c44e22f, ff25394b, 4a6a2650, d1e92c7f, 7c391ec9, 657e68c2, f1babc51, 432e438c, 05b69ec8, b026f762)
- Add three "On its own" settings for Ask Cmdr: whether it watches, how calm it is (5 seconds to 2 hours), and whether
  it toasts (297fbbc6, e863bd3b)
- Show a wake in the status corner while it thinks, with a way in and a way to stop it (2bd74f95, 1c873ee9)
- Mark a thread Ask Cmdr opened for itself in the chat list and in search, and open it in your own language (ffbb0a85,
  7d46af4c)
- Toast a wake that staged a change for you to review (df4a34e6)
- Let Ask Cmdr keep notes about you in a memory folder it can't write its way out of, which you can open and wipe
  (6269d96c, 271764ac, d8093f6a, e3e77760, 2da00041, 8f8328bb, ee0f3bf3, 8de87681)
- Tell Ask Cmdr what you decided about each suggestion, so it asks about the ones you turned down (7ed4e059, 81d77271,
  9d5cd27d)
- Add ⌘R to re-read the folder you're looking at, phones and network shares included (34c01dea, ca6aef1c, 0efa1e6a)
- Say when a share drops to the slow macOS mount, with a "Try connecting directly" button (187212e0, 0041834a, 0c5e2769)

### Changed

- Cut 2.8 MB off the app by leaving translator notes out of the bundle (183024dd)
- Keep an Ask Cmdr answer through a reload, and let a wake's thread read live (a0b44afd)
- Stop an idle Mac asking the cloud providers 43 times a minute about a folder that isn't a cloud folder (338963f8)
- Stop an idle indexing tick re-reading 90,308 folder scores and walking directories it can never use (56e60c4a)
- Keep the file index's page cache inside the 64 MB it always promised, instead of 132 connections each claiming 8 MB
  (4d071982)

### Fixed

- Fix a big folder stalling the pane: rows, watcher events, and Finder tags stop re-walking the whole listing (e39f05aa,
  290a23a5, af1b56a2, ca5c8ab4, b53a0115)
- Fix the cursor and selection landing on a neighbour when a listing reorders itself mid-delete (55cbb2d5, 8287e99f,
  9937293a)
- Fix a rename that blinked mid-copy deleting the file it was landing next to (6bc4fb86)
- Fix an errno sentence or a `diskutil` message reaching a toast in English, in every flow that can refuse (6397681c,
  147faf94, 2867fb52, 7358c3c6)
- Fix a missing file being reported as "No such file or directory (os error 2)" instead of its own name (f1a59724)
- Fix the crash dialog claiming Cmdr quit unexpectedly when it didn't, and saying it twice (3fce6eba, 8bf17df7,
  148d08ec, 141c0767, 9214251f, 77ec42fb, 91e1b8d7)
- Fix the Settings crash-report toggle promising something it no longer does (2d0aadf7)
- Fix a panic on a background indexing thread crashing the app at the next drive start (457ddb23)
- Fix turning a drive's indexing off and straight back on leaving it dark for the rest of the session (4b04e1ac)
- Fix a failed mount trapping the pane with every key dead (1426ae59, d7f03ecd)
- Fix shares named `café` or `公開` refusing to mount (07a9f1ca)
- Fix ⌘R in the network browser running two full share rounds per keypress (d36dae04)
- Fix the macOS app menu saying "cmdr" instead of "Cmdr" (ff7ca084)
- Fix the viewer, queue, Keyboard shortcuts, and Settings windows showing English chrome and the wrong number format
  (b559c1df, aa417b71, 1f54d8e9, 1b859da4)
- Fix the window title and the macOS panel names Cmdr quotes staying in the old language after a live switch (deebdb60,
  5c61e687)
- Fix the German status bar declining folders into the dative, and Hungarian and Vietnamese pointing at Finder panels
  that don't exist (714abfc4, 8e8e01c2)
- Fix the Brazilian catalog calling a disk two different things, and its last European-Portuguese sentence (dd4dfa50)
- Fix an overdue wake spinning a CPU core flat (8067f08f, 2f4a1f8f, 560ed47d)

### Non-app

- Add the SFTP backend: sign-in with a four-rung auth ladder, host-key trust, reads seven times faster than a sequential
  one on a high-latency link, clobber-free writes, and server-side copy, over eleven Docker servers (403b77b9, 6395f694,
  243e0c8c, 183aaad8, ec5887e5, ca85f7a2, 9afd9223, b911c800, a1911dbf, ceacd1e3, f19abb5b, d8cf82e3, e4f68e6c,
  c8f37d12)
- Split the SMB backend and its protocol layer into their own crates, so a check is `cargo check -p cmdr-smb` instead of
  332k lines (22edeeb4, 4f436ce4, f02febb3, da0d4b11, ba71876a, c9ee1a91)
- Measure what the app holds at rest, and find the CLIP text tower costing 251.5 MB that enrichment never calls
  (ecc78c83, 0f58a7e2, cd27791e, 0857b5f1, abedcd65)
- Report which release, OS, and CPU produced each event, and cover the routine gestures that were analytics blind spots
  (3301818c, e9c1402a, 9e191bab, 2510eaed, 57d06c01, 7b1482bb)
- Stop Cmdr's own test and tooling runs registering as real users in DAU, installs, and the version split (c2e8f791,
  346b1748, 7b2b90c9)
- Fail a check when an event fires undocumented or a documented event nothing sends (28ba4ac2)
- Re-run only the check whose file changed, instead of all 116 lanes (0caef223, 0b829639, aa011290, 588286f7)
- Catch a subsystem re-welding itself to another one, and ratchet the desktop bundle's size (9868eebd, 3cce961b)
- Cut 133 test files from the `svelte-tests` lane for the same a11y coverage (fbad0b46, d242f179, 3a275625, fde17ac5,
  638c3814, 98a2b43d, 233129c9, 1978e384, e6252d6a, 1fdb36f2, 72c02686, dbd0324a, ee9c2b91, c8df2962)
- Fix seven per-test nextest overrides silently selecting nothing, leaving eleven flake-prone tests unserialized
  (9d7fbaf6)
- Wipe 21 shipped specs, moving their durable intent beside the code (3cb15944, 2ff670aa, e15f55bd, 7e5f4377, 54c834cc,
  bdd7a739, 3d5cf997)

## [0.39.0] - 2026-08-19

Besides bug fixes, here are the three most important changes:

- The AI chat can now suggest file operations (then you approve/reject; agent can't do anything without your approval)
- Drive indexing now prioritizes your most important folders
- Localization improvements, and automatic switching if you prefer a non-English language

### Added

- Add Suggested ops: Ask Cmdr proposes file operations, and you review each one beside what Cmdr knows about the file
  before approving it or turning it down (c4b03d90, 8debc2fd, 3ded61cb, 43d74fee, 426fb8d3, 99cc049b, 862aeb7c,
  b94169e8, 0d3af79c, 47c56b4b)
- Ask Cmdr wakes itself when something worth noticing lands on your disk, and opens a thread you can read back
  (d574f4a5, e2e091fc, e435b473, be9027bb)
- Add the native menu bar, the right-click menus, and the startup alerts in all nine shipped languages (f8f3c935,
  42c4c20f, 09e6d676)
- Switch Cmdr's language the moment you switch your Mac's, no restart (0154a1a1)
- Follow the region you set for dates, times, and number grouping, separately from the language you read (35d55dd3,
  047ced55)
- Walk your whole macOS language preference list, so your second choice is reachable and a Traditional-Chinese reader
  never lands in a Simplified app (d61d1699, a6f90968, 18d892c3)
- Name the language it landed on in the picker's "System default" option (cce3a236)
- Add a language picker to the onboarding wizard, so a first launch in a language you can't read has a way out
  (abb9604d)
- Add Duplicate on ⌘D, in the command palette, the right-click menu, and the File menu, on phones and network shares too
  (42384c92, bda49c75, e067e6d2, a4fca37b, 2b4328bd)
- Duplicate a selection in place by dropping it on its own pane with ⌥ held (6b90b515)
- Land in the rename editor after duplicating one item with ⌘V or F5 (fea31cba)
- Index a drive folder by folder in the order you care about, starting where you last were, keeping every second of it
  across a quit (9fe0d3ec, 87fbb496, b2450af4, 9687d2fe, 1d3d7001)
- Index a folder you open next, instead of behind whatever big folder is in front of it (befd144b)
- Say which folder a first index is on, and which of its three stages it's in (24662e55, 950e2f8b, 9d7db4ce, 88562dd2,
  278039ff, 3be2e2ee)
- Admit the folders a finished index couldn't read, in the drive badge's tooltip (6b0b8db9)
- Search a mount that comes back without a rescan, because Cmdr retries the ground it gave up on (aab59ecd, cc430db2)
- Rename a run of files in one keyboard flow: ArrowUp and ArrowDown carry the editor to the next file (1ae0a978,
  10137090, d7ff3c75)
- Bring a backgrounded operation back to the progress dialog from the queue, with its bars, ETA, Pause, Cancel, and
  Rollback (6b9c9c15, 0ad26f6d, 1c4acfbd, cd3543f4, 267e1108, 24ab591c)
- Background, pause, cancel, and hold ⌘Q against a transfer that is still counting its files (5b7ca55f)
- Escalate an F8 trash to a permanent delete by holding Shift (fff2e576)
- Add "Show hidden files" to Settings > Appearance (4b0ff78c)
- Open a fresh install on home and Downloads, once ever (6980d0e7)

### Changed

- Hide dotfiles by default on a fresh install (57d6862d)
- Resume a half-covered drive 7× faster: 185 seconds down to 26 on the benchmark tree (cadd8b81)
- Settle a drive somebody keeps writing to in minutes, instead of waiting for the next launch (14dc2e6d, 1e0c1add)
- Cut what a search costs on a drive that is still being covered, and what tracking a walk costs while it runs
  (542718f2, 0a524e6b, e6d8c8fe, 12658b90, 7d1f910d, 898f5634)
- Copy a folder with the concurrency window it always advertised (4b9f2e1d)
- Grey out Copy, Move, and Delete in the File menu while a dialog is up or Ask Cmdr has focus (40514a26, fcc35395)
- Name which file a clash prompt is asking about, and both ends of a transfer opened from the queue (ab61144e, d6c9f9c0)
- Open the operation queue with one query instead of one per row (8cc38847)
- Tell a screen reader what the scanning chip does and which chip it is (8f5ed6ab)

### Fixed

- Fix copying, moving, or compressing into a subfolder of a network share failing outright (e518a456)
- Fix a destination Cmdr can't reach being reported as your own file going missing (ceb71910)
- Fix a crash mid bulk-rename leaving renamed files with no undo (7bde003e)
- Fix a stored search listing files that are gone, or hiding files that are still there (616184c9)
- Fix a share macOS mounted twice losing its watcher, and uploading an error report every time (272e9753)
- Fix a folder on a sleeping NAS costing every later search a two-minute listing (baa1f4f2, 07ddef4c, 25a1ffbe)
- Fix opening a folder mid-index stranding the first index for up to an hour on a wide directory (0bb0eb95)
- Fix a rescan blanking the index underneath a search that is still writing to it (cb9568d0, 9747c16a, 94b4c59a,
  e9036364, 56858da8)
- Fix "Rescan now" reporting success and doing nothing during a scan or a search walk (af133ef2, 6b69f14d, 4a3468a5,
  aa4a6b8c)
- Fix a drive you turned on being forgotten after an interrupted first index (57ae8157, 0e97d09d, ea8a3148, 5ed425bd)
- Fix a poisoned lock permanently killing the MTP watcher, the space poller, the verifier, and the volume registry
  (cdfdcacd, f1ea3cc5, 4130904d)
- Fix a transfer parking forever when the next clash arrives while you're answering the last one (56ebb8db)
- Fix a paused transfer claiming a speed and hiding its time left (df1aed7b, 496e3399, 83274f03, 3c19b678, 42a61d18)
- Fix two progress dialogs stacking over one transfer, and closing a dialog killing the transfer behind it (d0387471,
  018f4e4c, 5f61f6ea, ac675359, 2dc6e473)
- Fix a scan preview on a dead volume spinning forever, and a wedged archive holding the scan dialog open (ee996316,
  74c837b4, bb0173ac, 117cc65f, d0eeb4a9)
- Fix a copy started while another operation is running starting nothing and saying nothing (df42f7f0, 1cb9a554)
- Fix the queue reading "Running" over a frozen copy, Rollback deleting on one click, and a view that attaches late
  showing a scan that isn't happening (f40ac1ac, 1778b146, 2f3be6a1, 78b8a704, c463808e)
- Fix a folder scan over SMB reporting 0/0/0 until it finished (5e80b5e9)
- Fix a guest share listing and the share browser waiting forever on a server that stopped answering (15ff0999,
  00c1ad22)
- Fix duplicating `photo (1).jpg` nesting into `photo (1) (1).jpg`, and a folder with a dot in its name being numbered
  mid-name (f770d3a5, 5d9de4e3, b1abd601, 53a5c33c, ff21a4f1)
- Fix a move into a differently-cased folder on a case-sensitive drive being counted done without moving (8c43edf2)
- Fix holding an arrow during a chained rename skipping files, and the editor vanishing three rows in (368f9d97,
  25f9fc85, e8890e46, 135d2ea8)
- Fix a chained rename the volume refuses losing its name silently, and its toasts naming files from folders you've left
  (c173fd07, 3863158a, 7d9d49d8, 78659dc5)
- Fix the French and Swedish rollback buttons promising to restore files rollback can't bring back (0ae5d9c6)
- Fix ⌘C in the viewer copying the search query instead of the text you selected (6be268f7)
- Fix double-clicking a number in the viewer selecting the word before it (a4595023)
- Fix a stray green line under the title bar when a dialog opens at startup (ba1c9508)
- Fix rebinding a shortcut stripping that menu item's icon (13eb0e0b)
- Fix the first-run layout not surviving the session it was applied in (919840ee)
- Fix a search over ground another walk holds sitting silent instead of saying so (5d187522, 6d04b410, 92be35ee)
- Fix Escape not closing a search dialog whose run never answers (a29bc77c)
- Fix the onboarding wizard quoting a drive-indexing cost you don't actually pay (b7f4c581)
- Fix a favorite on a wedged share stalling the order the index walks in (92d11c66)
- Fix a trash waiting out a scan preview it never reads (a8d56440)
- Fix MCP reporting a pause that never happened, and a false "queue is idle" (aba3d0e2, 8cd18287, 23edcb9f, 5acc1569,
  13576b84, 65261dbf)
- Fix an error report losing everything a log line says after a volume path (c3d28f72)

### Security

- Update `h2` to close an advisory letting a peer keep a connection busy for free (e2187223)

### Non-app

- Add `lock-poison`, which sees the 203 places a poisoned lock is silently swallowed, the shape behind two shipped bugs
  (ce317da1, 22b27b7b, 52e96cd2)
- Name the copy-pasted file pairs in the duplication check instead of hiding them behind a percentage, and point it at
  the frontend too (144907e3)
- Count every ❌ rule a subsystem's docs carry, as a number that can only go down (1acf1555, bf56603c, 04be43ef)
- Add `discarded-outcome`, catching the next function that throws away an answer its caller needs (a8b9d6eb)
- Say when a check run reformatted an already-committed file, so a green local check stops shipping a red CI (85859d88)
- Cut 172 seconds per run off the Playwright lane, and 15% of the frontend lane's invalidations (7663c42f, 55360dc8)
- Pin the language of the E2E suite and the screenshot pipeline, so neither answers to whoever's machine they run on
  (858e5501)
- Drive a one-file-at-a-time conflict answer over MCP, the state that hid a months-old wedge (917abbed)
- Refresh the AlternativeTo listing, which had been describing a v0.24 app (42442a80, 95d25b59, 0cc77fee, 9bd60d10)
- Move the Go toolchain to 1.26.6, clearing four stdlib advisories the check runner reaches (ce1147c6)
- Report which language people run Cmdr in, and whether covering a drive in phases delivers what it promises (c280f33d,
  a25b5d20, 5d83a7bc)

## [0.38.1] - 2026-08-12

- Double-mounted SMB volumes used to break stuff. Now they are not.
- Added some convenience buttons to error screens, with keyboard shortcuts!
- Some stability and UI fixes.

### Added

- Add ways out of every error screen: "Go to home folder", "Go back", ⌘D for technical details, and a Home command (⌘⇧H)
  (d25245d9)
- Tell agents over MCP when a folder size is still a lower bound, and show on-disk size where it differs a lot
  (70295b62)

### Changed

- Stop calling the usage stats anonymous, in all 10 languages: they carry a random per-install id (412b7093)
- Rewrite the privacy policy to match what Cmdr actually collects, keeps, and shares (4c893e4a, 81107094)
- Expire personal data in telemetry on a schedule instead of keeping it forever (e9f3465d)

### Fixed

- Fix a share mounted twice breaking the panes and the volume switcher, and freezing the app on F6 (01f93aa4, 1a9208e1,
  f535a18d, 1e1e74d8, d97e3a7f)
- Fix ejecting one of a share's two mounts making the share vanish until the next launch (0b3a86ad, 35e42d89, b9678ceb)
- Fix a dead NAS mount hanging every operation instead of handing over to the mount that still answers (aff924b1,
  fd04e8ce, 140473b8)
- Fix the panes and the volume picker freezing during a big transfer, when image-index queries took the whole thread
  pool (6566f8cc, f89e8c1f, 4401fcfb, 9dde346b)
- Fix a slow volume listing emptying the volume picker with no way back (7405b534)
- Fix clicking Move on a slow share looking dead for minutes (cce94565)
- Fix a big local folder sitting on "Opening folder…" instead of counting files as it loads (b9563bce)
- Fix the Brief-mode cursor vanishing and every column filling the pane after a measurement blip (53e5c42a, 85a09bc4)
- Fix six error reasons missing their "Try again" button (f1226f02)
- Fix a browser download's final rename producing no toast, and Cmdr's own copies toasting behind a symlinked Downloads
  (6556e538)
- Fix the Full-view scrollbar running up alongside the column headings (a62fefbd)
- Fix screen readers announcing a file row that isn't on screen (b8486f70)
- Fix one purchase minting more than one set of license keys (9826d59f)
- Fix a rejected license email passing for a sent one (b1649dc9)

### Security

- Stop SMB account names riding along in error reports (14c6221d)
- Make stored IP hashes one-way, and drop the IP from crash reports entirely (825e7c0b)
- Stop blog likes storing recoverable IPs, and rate-limit the endpoint (92fc5e40)
- Sign dev and production licenses with separate keys, moving the production signer out of a dev config file (21785cc2)
- Stop a page you merely visited publishing to the local dev blog editor (203c1ea1)

### Non-app

- Add `pnpm marketing:shots`: all eight brand masters, staged and frame-verified, in 22 seconds instead of half an
  evening (66a6eeb9, 3350faf0, 109537fc, 56c5adab, a55384f6, 13a4f5e7, 84ba857a, 245627e4, b1f8f330, 7ec23078, 45253476)
- Regenerate the website hero with one command, from rectangles measured off the live DOM (dbfe84f1, aa6087da, 71183212)
- Ship the brand masters as lossless WebP, a fifth of the bytes for identical pixels (7d38351d, 36b5297f)
- Give back ~30,000 CPU-seconds a month in the local check suite, and stop a docs-only pass re-running the Rust tests
  (8f3f5a76, cf839c39, a41573c3, 4b71a6f6, 6d8a287f)
- Make `pnpm check` quiet by default, collapsing ~50 passing lines into one (ced50200)
- Log every failed and slow test individually, so flake and slowness rankings become a query (45161522, 739d980f)
- Fix 107 broken Rust doc links, and deny every doc lint the project holds itself to (d10960dc, 85faa8be, ac38e32c,
  aa706dae, 60c92c58)
- Fail `pnpm check` on a RUSTSEC vulnerability in a crate we actually ship (5df72a53)
- Split the Worker's 40 flat files into four self-describing areas, each with its own docs (5c12e78b, 8cc6122b)
- Bring 13 oversized `CLAUDE.md`s back under the resident-doc budget, and move the project's hard rules into `AGENTS.md`
  (1245ca05, 2429c1ad, 3be1b799, 39b2f240, f16e5a5d, 6b0df9ed, 15288e46, 571f2c97)
- Mirror the Linux volume module and the MTP backend on their macOS and SMB siblings, so cross-platform drift is visible
  (0d836f84, eb3e073e)
- Take the operation-session plan through four review rounds before any code (cf8ca51e, 63cf6b52, 4280431f, 84c21d70,
  2e1eff51)
- Prepare the AlternativeTo listing to paste, describing today's app with today's screenshots (039e17c6)

## [0.38.0] - 2026-08-11

The three main advancements:

- Search now works regardless of indexing.
- Backgrounded operations look and work better now.
- A ton of stability fixes and resource use improvements around indexing and otherwise.

### Added

- Search a folder Cmdr hasn't indexed yet: it walks the drive live, streams matches as they're found, says which wait
  you're in, offers the permission for a folder macOS refused, and keeps walking when you send the results to a pane
  (b3fd1f9a, 11baa340, 1ade70f4, d4c2abaf, 78bcbd41, 45096d95, 75d011fc, 5bfb8c2f, b49f8f75, e846c0ce, dfdd7ab0,
  e72322ff, efdeef06, bf0c546b, 4c241576)
- Scope a search to the current folder (the new default) or the whole volume, with ⌥C and ⌥V (b7ccec1a)
- Add a corner chip in the main window for a backgrounded operation, with its progress and one click to the queue
  (a447baa7, 221c84dd)
- Open the operation queue with ⌥⌘Q from View, renamed from "Transfer queue" since it holds deletes, renames, and
  archive edits too (2a52e4f3, 10110bf3)
- Keep a failed background operation and its reason until you dismiss it, in the queue and as a toast in the main window
  (8b97ed94, c53a9961, 0eb03c89, d592d047)
- Ask before quitting with a transfer in flight, and clear away whatever it left half-written (fe2cb825, f5c5e2b5,
  20a1022e, 6819a066)
- Roll back a copy from the Transfers window (7a1e1c3c)
- Resize dialogs from any edge, and grow every dialog that shows a path (d9de60c0, e7b4871a)
- Hover any shortened path or label to see the whole thing (3270e3fb)
- Copy the current folder's path with ⌃⌘C on the `..` row, with a toast showing what landed on the clipboard (e883b528)
- Ask agents where your disk space is going: one indexed listing tool with size ranking, paging, and honest coverage
  (45365c16)
- Agree to the terms during onboarding, instead of consent being assumed from the download (7a166919)

### Changed

- Rewrite the terms of service: 3,423 words down to 2,203, accurate about what Cmdr actually does, and each release now
  converts to AGPL three years after it ships rather than every version on one shared date (248cfc63, 57249e41,
  8af24705, 69fa0f52, 1311d8a0, 3ba6b4bc)
- Cut about 97 MB off peak memory during a search (d75453b8)
- Stop a dotfile write in your home folder rescoring the whole drive: 5.25 s per pass becomes 2.1 ms (0271855a)
- Stop an idle machine rewriting 51,081 folder-ranking rows a minute for a result that didn't change (234bd2ae)
- Halve the live index write cost by committing a burst of changes once instead of once per file (3313aabf)
- Stop asking iCloud and Dropbox about folders that hold no cloud files (74574516)
- Cut roughly 9,400 log lines an hour about the things that are always fine (5431df6b, 6834c4de, 1354ab0b, b67ff311)
- Stop an idle NAS connection filling the log with packet traces (e24382ab)
- Show the same honest readout in the Transfers window as in the copy dialog: both bars labelled, percentages, speed,
  and a time left that doesn't shift the layout (b4884f2b, 7a1e1c3c, 442dc733)
- Open What's new on the headlines, with each release's details behind a Show more (d6d14da9)
- Ask the copy conflict question in a card you can read at a glance (ec81db88)
- Give the whole app one line-height scale, so text spacing stops varying screen by screen (22fb4bbf)
- Say "Background" rather than "Queue" on the progress dialog when there's no queue to join (8441b4e2)
- Stop the write-error dialogs saying "failed" at you (19ecb6aa)

### Fixed

- Fix a folder move on a phone destroying the child you chose to keep (56047e43, 3971e86e)
- Fix a folder move to a NAS deleting the local files you chose to skip, and the conflict dialog reporting a real file
  as 0 bytes (b84a6f86)
- Fix copying a local folder to a NAS or phone failing outright, and a failed folder copy wiping the destination folder
  (7046e9db, be819a3e)
- Stop a force-quit or a crash mid-copy leaving a truncated file wearing your real filename, on every drive (a19325c9,
  06837bc6)
- Fix a scan preview authorizing an operation on a different selection (1e75af28)
- Fix a delete guessing a folder was a file when its details couldn't be read (0cabe9f0)
- Stop two disks being handed one identity, which could route reads and file operations to the wrong drive (181c2b71,
  a3c6684e)
- Fix folder navigation stalling up to a second while the file watcher armed (0141b744)
- Fix ⌘- and ⌘+ freezing the app for up to 46 seconds while fonts were measured, and non-Latin names staying at an
  estimated width in Brief mode (62e69c3e, bb781c12)
- Fix a folder replaced outside Cmdr leaving ghost files in the pane (4b633dfc)
- Fix copying to a NAS dying on a filename with a `?` or a quote in it (9536ea44)
- Name the file that actually failed in a transfer, instead of the folder you selected, and say which file the server
  refused the name of (7f4b50ec, 4062288b, 119cdb8d)
- Stop blaming macOS for a folder your file server refused, and drop the Full Disk Access prompt that couldn't help
  (c912c8d3, f37fe124)
- Fix a pane stranding itself on "Path not found" after its network drive disappeared (b314f814)
- Fix Quick Look and dragging files out doing nothing on a direct-SMB pane (230ff586, fea26283)
- Fix a burst of changes on a phone pegging a core and freezing the pane (63d7e0e2, 1b4e667f)
- Fix a backgrounded transfer's conflict question never reaching you (d744a61a)
- Fix a search from your home folder reporting that Cmdr doesn't cover it (6d3d7abb)
- Fix a folder-size filter answering from a ranked sample, so a 1.7 TB folder stops going missing, and add sorting by
  size or date (7ee6b639)
- Stop search presenting a filtered count as the whole truth (37e9c931)
- Fix a file with a newline in its name being unfindable, in search, selection, and excludes (d0b63a13, b71cef17,
  b73fa8e3, 15e444c7)
- Fix dialog text sitting further in than its own title, and long paths escaping the panel (555be094, 63be2dc3)
- Fix a listing on a network share passing as fresh when another machine had already changed it (f0b139af, 23f53281,
  28431ec1)
- Stop a NAS getting blamed for the seconds Cmdr itself spent frozen (608b1dd1)
- Fix network and external drives skipping the per-navigation self-heal that local browsing gets (76863f0f)
- Fix a dead mount stalling launch while Cmdr swept last session's leftovers (e063f793)

### Security

- Stop a saved cloud AI key ever coming back out of the OS secret store (4f32fe91)
- Close a bypass that served the private analytics dashboard without authentication (79969dff)
- Rate-limit every public ingest endpoint, with a global ceiling so an error-report flood can't drown the channel or
  delete real reports (037186a2, 763b8475, f8169606, 67c125f2, 7a023642)
- Clear the RUSTSEC-2026-0221 unsoundness advisory (63a0858f)

### Non-app

- Extract the archive backend into its own crate behind named seams, so a future filesystem backend can be written
  without reaching into the app (6d435cdf, e2be3721, d5ab81b0, 4f3360d8, fe33825a, 3f11fea4, 2cb09848, 057cc9e6)
- Add a Cmdr IntelliJ plugin: message keys fold to their English text, and a changelog hash ⌘-clicks to its commit
  (5eeff67d, 8fc5ffc8, bd337760, 64483b36, c623e589, 875f6d54)
- Regenerate the translator screenshot set at 131 surfaces, framed on their subject, with blank captures now detectable
  instead of shipping silently (5d48b702, b03fb4c7, aaf7eeb2, 8d03bc65, d1e95c2c, 2c117eba, cbefd514)
- Store changelog commit refs as bare hashes, dropping 89 KB of URL boilerplate (82694836, c9e70ebd, 35584706, 4adbe77f,
  96620f4d, ab322339)
- Cut debug builds 31% smaller and 35% faster by dropping variable-level debug info (ba87ee1b)
- Give test fixtures a scratch directory that can't collide with another run, across 106 fixtures in 51 files (9ac788e6,
  af28731a, 25a29f4f, 8f619690)
- Split 20 oversized files along seams the code already had, shrinking the length allowlist instead of growing it
  (feb065de, e5ea10d0, 7a45a777, 1a7514a8, b98e3484, fd17b768)
- Add ESLint, Stylelint, and knip to the analytics dashboard, and wire them into `pnpm check dashboard` (21627bb6,
  030db66d, 1da89169, dc758cd5, 026b7c70)
- Make every E2E spec put the shared fixture tree back, so one spec can't fail the one behind it (48c0a0f4, 2cd24c70,
  b1dbd074, f98ede59)
- Add two reusable instruments for resource questions: a churn baseline harness and an index size probe (38d391d8,
  5dc39c44, d74b558a)

## [0.37.0] - 2026-08-03

Highlights:

- Search dialog redesign and major speedup: looks a lot nicer, and even broad queries went down from taking 12 sec to
  0.5 sec.
- A bunch of fixes to AI bulk rename. Now it works with hundreds of files. But still alpha.
- A lot of SMB copy/move improvements incl. a 3x speedup for small files!

### Added

- Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a4, 2d41cc14, 18add0b0,
  42f76971, ede1a7d6, 84e5f3a5)
- Add right-click Cut / Copy / Paste / Select all in every text field (fd6fc293)
- Add a "Chat memory size" setting: Automatic, or 16,000 up to 200,000 tokens (75121419, 14aacf89)
- Show how full the chat is in the Ask Cmdr rail, with a fill bar and the real token counts (0b6efe95)
- Add Undo for a batch rename, per batch and across a whole multi-batch run (dcc14c13, 032722e1, c528ea8f, dada4bea,
  e301c1e4, 0b25450b)
- Show every file and the evidence behind its proposed name in the rename review, and let you fix a name in place
  (56788bdc, 766c3ebb, 64b8022e, b456a365, 0423a797, 7fc00aad)
- Refuse a rename plan whose content-derived names Cmdr can't verify the model actually read (285af99f, 52eeb308,
  0b619828, fb60f108, 6b0f066e)
- Move recent searches into the query field as a dropdown, each row showing its age, result count, and filters
  (503e8443)
- Show transfer speed in the Transfers window (821307e0)
- Say when a transfer has stopped moving and what it's waiting on, instead of a confident ETA that isn't true (066796c7,
  43c106cf, a2070fa7, a77bf832)

### Changed

- Return a broad search in under half a second instead of twelve (777c32c3, f3591790)
- Open the search dialog and start typing without waiting on a full index rebuild (1b8557a7)
- Stop a cold NAS index freezing the search dialog, and search one NAS once rather than twice (2890fe33, 5b3b6da2,
  9c55b0de)
- Redesign the Search and Select dialog into a real Cmdr dialog: house chrome, a 2×2 query block, one surface per zone,
  and a Path column with its width back (2643f746, 4a1ae52b, 6c036b1f, af837985, d97f6f6d, bf6354f2, b4437721, c0c6f227,
  24693d49, 66412c26, a4478708, 44295a78, acdaa945, 0a5170e1)
- Give every text field one look: 8px corners, an accent caret, and a solid focus ring (0394c062, d7a7179f, a40d2c26)
- Copy a small file to a network drive in one round trip instead of two (17d8a6b8)
- Skip the per-file destination check when copying into a folder Cmdr just made, 2.1–3.8x faster on a NAS (20d6fead,
  c4bc6cec)
- Make the SMB concurrency setting do what it promises, worth 25% on an 8-core Mac (9f3d5a7f)
- Recover from a network drive that goes silent in 50 seconds instead of hanging forever (560721b8, b995f699, 80bc07bf,
  aa5e7b26, 0c5f6cd2)
- Retry one file after a transport blip rather than ending the whole transfer (6a799377, 9b8ef8a0, 98f818e7)
- Survive a network-drive upgrade landing mid-copy, and stop re-connecting a share that's already connected (402e9b73,
  7de5961b, 1685795f)
- Cap the app's whole database page cache at 64 MiB, whatever the connection count (a780954e, 2d8b6bb2)
- Stop reopening a database connection on every pane switch (928709f4)
- Cut cloud sync badges from 300 thread creations a minute to none (852e7991)
- Stop a two-folder change rescoring 90,000 folders every minute, and rescore only what changed (914004f1, 555d75fe,
  04e9ee40, 4c3a794b)
- Show the main window a second sooner at startup (5ae724a9)
- Hide Cmdr's and other apps' temporary save files from the pane, on every drive (66e60c3b, ca2a6371)
- Render every size, speed, and ETA from one implementation, so two windows can't disagree (9cab4e03)
- Settle one rule for the `…` menu suffix, and give Compress its icon (499ab149, b04ac721)

### Fixed

- Fix ⌘V pasting twice in dialogs (9b352123)
- Fix ⌥⌘A opening Ask Cmdr and selecting every file at once, plus four default shortcuts that never fired (0919a6e1,
  f69cca28, 4beb9159, 4ead5e04)
- Fix the whole UI freezing when you resize the window (ceeddc43)
- Keep a maximized window's position through a restart (2b7bec84)
- Fix six windows rendering every setting at its default, including binary sizes in the Transfers window (0d64c84c,
  a434418a)
- Stop a killed transfer leaving a truncated file wearing your filename (b889e065)
- Make Cancel and Rollback work on a stalled transfer, instead of leaving force-quit as the only way out (78ef454a)
- Show size and date again for folders on network drives, in archives, and on phones (a0ab6ff8)
- Keep the Size column readable on a reconnected drive (bdd3ccbc)
- Retry a first network-drive connect that never reached the server (07b01142)
- Stop Ask Cmdr inventing names for screenshots whose contents it never received (01b3f2dc, 5d95d588, 9496fcc6)
- Fix the agent reading the wrong files on a scrolled pane (b2f066e3, 551795b2)
- Let a local-model user get an answer on default settings (02ece3ae)
- Stop a batch-rename undo restoring an impostor file over your original (d300f598)
- Keep folder importance and image indexing following the drive index after a full scan (88e07ff6)

### Non-app

- Extract the drive, media, and importance indexes into `cmdr-index` and the filesystem vocabulary into `cmdr-fs`: 93k
  lines behind a compiler-enforced boundary, with the index's inner build loop 6–9x faster (944b0bc2, 4323505f,
  913e43fb, de9ff9f8, a647efa3, 42f13a4f, e1081220)
- Run every Rust check over the whole workspace, so a crate can't hide from the tests, the linter, or the license policy
  (cba34d77, beb2dd50, bdc3849a, 8115e928, 65fb5603)
- Re-run a red Rust suite's failures alone before believing them, so machine contention stops reading as a defect
  (9e14ae57, 32aa6322, c92e0571, 736df70b)
- Break `FilePane.svelte` into tested controller modules, 2,733 lines down to 1,971 (0a29a457, 89493b33, bae466f9,
  1dfb350f, c1dc27b7, 766a80a2, d33ce6ea, 36d10e42, 0e1fb67e)
- Add a rustdoc lane that catches broken doc links, and repair the 32 it found (4092256c)
- Serve the current release from one download URL that never goes stale (8b9947c2)
- Keep app-directory listings in the repo, one file per directory (fbfea9a6, 68e6d4d0, 9daa96de)
- Move roadmap items into one typed data file (2a669f4a)

## [0.36.2] - 2026-07-28

The highlights:

- A 5x drive reindexing speedup!
- Very significant RAM use optimizations, incl. a 85% decrease for search.
- Also some convenience fixes in the renaming flow.

### Changed

- Make checking a drive for changes dramatically faster on macOS (it used to take ~20 minutes on a boot disk) (cbde9d89)
- Cut search's resident folder-ranking memory by 85% on a NAS-sized drive, 58 MB down to 9 MB (68237a9b)
- Cut background folder scoring's memory by two thirds on a NAS-sized drive, 256 MB down to 84 MB (0c5296dd, 5b91dadc)
- Say "Checking for changes" when Cmdr is checking a drive rather than rebuilding its index, and time each kind of scan
  separately so the estimate is right (022a6160)
- Show one download toast and one macOS notification per burst, always naming the newest file (6a8bf70e, 81d69c64)
- Rewrite the drive-indexing settings copy in all nine languages so it reads natively (b3ab11f8)

### Fixed

- Fix clicking or dragging inside the rename field cancelling the rename, and make clicking away save, like Finder
  (d8dcb89c)
- Show the server's own reason when an error report fails to send (08183074)

### Non-app

- Add doc checks that catch pointers at headings and source files that no longer exist, then repair the 75 they found
  (c66b02e1, 8cb06924, d2e8b0f2, 06b3e504, 5944cb36)
- Split the folder-importance and image-index docs so each area carries its own guardrails instead of one shared ceiling
  (0b5f15fc, 5d6ca822, 79720e7d)
- Replace rotting file inventories across a dozen module docs with the layout facts codegraph can't answer (b2e7abeb,
  1f5eef34, 28b61d5e, fd2d6ab6)
- Plan the index-crate extraction, giving the hardest 28% of the Rust a real boundary (dd98eb57, f76cae0d, 5ef7c4e3)
- Catalog the 19 flaky tests that make a red `rust-tests` run meaningless (fd479988)
- Fix the Linux build breaking on a macOS-only dependency that wasn't declared as one (aabc4cb1)

## [0.36.1] - 2026-07-25

Cmdr could balloon to tens of gigabytes of memory within minutes of launching; that's fixed. Network drives stop
hoarding NAS snapshot folders in the index, and switching drive indexing off now really stops every drive.

### Fixed

- Fix Cmdr ballooning to tens of gigabytes of memory shortly after launch (28967ce8, fb874666, 89d2befb)
- Fix a network drive re-indexing itself at every launch while drive indexing is off (27227669)
- Keep NAS snapshot folders out of the drive index, so a share's file count and size are real (1046a5d2, 6d260eba,
  c802ed91, 85c3777e)
- Fix error reports failing to send when you leave the note empty (326ccf12, b3fd57d3)
- Drop "Send error report…" from the toast that reports a failed send (22001eed)

### Changed

- Cut memory for a big network drive's folder tree by two thirds, and load it three times faster (0f6663b8)
- Grey out the per-drive indexing controls while the main drive-indexing switch is off (50b0cb5d)
- Make the memory safeguard measure Cmdr's real memory use and keep watching after it steps in (7230e86f, f7631999)

### Non-app

- Document that `vmmap` reports Cmdr's Rust heap under a GPU name, and how to measure memory properly (b4b2c175,
  76bafde1)
- Add a test helper that proves a hot path doesn't allocate per row (f3e76419)

## [0.36.0] - 2026-07-24

The three main advancements:

1. A very nice design facelift
2. Photo search stays fast even with 2M+ images, reports status+progress nicely. SMB connections stay nice and
   responsive during indexing.
3. More RAM+CPU+disk use streamlining

### Added

- Keep photo search fast past 50,000 images with an on-device index that activates automatically at scale (c4f65cbf)
- Show image-index status right on the icons: a per-file badge, per-folder coverage, and a per-drive dot (8a8ceb76,
  fab1f8ac, 66fe72e6, c101c196)
- Manage image indexing from three focused settings cards, switch semantic search on or off, and reclaim the model's
  disk with Delete model (8c31e9c6, 52ea7bf8)
- Speed up image indexing with a Parallel workers slider, from one to your machine's CPU count (46bd40d5, 23c10d6e)

### Changed

- Give file transfers and browsing priority over background indexing, and pause background uploads while you browse the
  same share (e00321e6, 2d83ea8d)
- Index a NAS about 3.8x faster by spreading the scan across multiple SMB connections (f482daa9, 2a803d5e)
- Read NAS image bytes in parallel over multiple connections while indexing (fdf5832a, fc0c79fd)
- Shrink the image-search model download by about a third, reclaim ~550 MB after it installs, and roughly halve
  per-image disk and search memory (d91d92bb, eb5b629c)
- Refresh the app toward a native macOS feel: rounder dialogs, capsule buttons, redesigned Copy/Move and Delete dialogs,
  squared-off tabs, inset file panes, and right-aligned Size and Modified columns (cae582c7)
- Match each settings control to its job: segmented toggles for quick switches, drag-only sliders for coarse values, and
  framed number fields elsewhere (99ce8537, 088fdc38, 95c8fd34)
- Reorganize Settings into Indexing and Notifications, with a leaner AI, Behavior, and Advanced (f03119c1, 8f1e470b)
- Redesign the Ask Cmdr rename review to read as a calm, native rename list (6dcc6f6e, c5774727)
- Update the low-disk-space warning live and clear it once you free space (69b0146b)
- Stop a single churning folder or an app updater's throwaway files from dominating indexing CPU (d33f8ea6, 7b7c7b28)
- Keep the search-index journal files small instead of letting them grow to the size of the database (c1e167d0)

### Fixed

- Stop the app crashing when you close Settings or the file viewer (72dfeb58, 8713f797)
- Fix NAS image indexing stalling at zero images, and skip an unreadable file instead of pausing the whole pass
  (bb714a85, a8b063dd)
- Stop Ask Cmdr reporting a fully-indexed folder as not indexed (cb0b2685)
- Fix a rare blank window on cold launch (d24cc367)
- Fix three messages that rendered a code fragment or dropped their text: the license verify banner, the file-viewer
  type warning, and the MTP conflict dialog (c1267c5a, 5a60f21d, 67dba4e1)
- Keep per-folder image counts accurate right after you delete files (7e9837ef)
- Keep the Settings and Debug sidebars from sliding under the traffic-light buttons (7d9651bc)
- Stop deduplicated hardlinks from being rewritten on every index pass (0932ea3e)
- Stop a resting folder from holding the whole disk under a "size updating" indicator (04108314)
- Give every switch and checkbox a real name and role for screen readers (53a0e8f8, c3fbc4e1, 1a71baea)
- Restore the missing apostrophes in the French drive-index tooltip (47e9c79e)

### Non-app

- Kill the Rust test-synchronization flake class: one canonical wait helper, RAII isolation guards for shared globals,
  real completion counters, and a lint that fails on fixed sleeps (14d1a5a4, cd437885, 7013648b, 97819d46, ae57706d,
  814629a9)
- Reorganize the indexing subsystem into 13 stage-based areas, each with colocated docs (ff63e6df, 47624e9b, 840b31ad,
  e67ef280)
- Add Debug > Soft dialogs, a gallery that opens every dialog on demand for design review (43f1f51f, 656f24bf, 996aef5d,
  bcc8bd27)
- Enforce house UI primitives and add a building-UI guide so new work stops reinventing controls (67e2f03d, 4c2cf75b)
- Reference docs by bare path with dead-link checks, plus per-doc read/write usage stats (df663786, ee010a2b)
- Overhaul indexing logs: report reconcile churn by trend, attribute writer waits, and truncate the WAL during long
  replays (96297bc4, e5b6abed, 08e45f10, 65e7beff)
- Make signal-crash reports group across installs and become symbolicatable by recording the image base (cca5011c)
- Stop dev builds writing ~1 GB of dead WebKit cache (eee12e07)
- Benchmark the NAS scan and add a reliable CPU/RSS sampler (e740933a, c4baeb26)

## [0.35.0] - 2026-07-21

The three major things:

1. A lot of CPU+RAM use improvements
2. Bulk rename with natural language, with an approve/reject UI
3. Manually configurable image-indexed instead of a slider

### Added

- Rename a batch of files by describing what you want: Ask Cmdr proposes each new name, you allow or deny row by row,
  and it applies as one undoable operation (df87ec51, d6c4cff6, 7e63b4c6, c1654a3c)
- Pick which folders get image-indexed in Settings › AI › Image search, or add one from its right-click menu; each pane
  says whether the folder you're in is covered (51995f4a, f6168b22)

### Changed

- Browse a NAS at full speed while it's being indexed or copied from: opening a folder mid-scan dropped from 10.7 s to
  instant (6d9df62d)
- Read MTP archives about 35% faster: a ranged read now costs one USB round trip instead of three (8e9efebe)
- Stop rescanning the boot disk roughly ten times a day when macOS loses track; it sweeps once daily and counts what it
  coalesced (49da9914)
- Name the photo tools in Ask Cmdr's tool line instead of showing "Working" (f629af77)

### Fixed

- Fix a fresh scan silently dropping your biggest folders: one run lost 661,411 entries across five directories whose
  only fault was size (44cf3b74)
- Fix a rescan refusing a large healthy folder while walking browser caches instead (596390b5, c67e1f2c)
- Stop one pathological subtree eating a whole rescan, and cap the cost of a cold start behind a huge temp folder
  (90e4784c, 8034df2e)
- Fix a rename replacing a file created in the moment between the safety check and the rename itself (8cb4e073)
- Make two Cmdr processes on one data dir impossible, which was corrupting the index (af9bed4d, 1f96465e)
- Stop a slow file-event replay cascading into a forced full scan (e9b10c32, 6c35074c)
- Repair folder sizes after a failed write instead of dropping them, and heal a drifted entry counter (765d49d8,
  f6fe1db2, 1abad37b)
- Fix an interrupted rescan leaving folders claiming exact sizes they can't know (bed0e936)
- Keep the size of a folder named `dev`; only real Unix roots are skipped now (79b58b21)
- Recover on your own from a dead phone session instead of needing a replug (cb612afe, e5f9e023)
- Fix a phone's index drifting after an upload, rename, move, or delete (b47fee96)
- Stop a slow phone wedging mid-operation (f69c930c)
- Wake the phone watcher only for actual phones, not every USB event (e65980a7)
- Hard-wrap unbreakable lines in the file viewer and fix word-wrap scroll drift (7822523b, 2602a29c)
- Keep the app up when a git portal refreshes, and group crashes by the code that broke (8f4e6411)
- Explain skipped rescans in the drive tooltip, in all ten languages (46809911)
- Stop a superseded listing reporting progress against an id the pane already retired (b6ede325)

### Non-app

- Re-translate the newest three features across all nine non-English locales, fixing a European Portuguese leak and term
  drift in every language (0582089a)
- Restore the Linux build, which `#![deny(unused)]` had been failing since the instance lock landed (40ed4946)
- Cut recurring secret-scanning and Renovate noise at the source (4b4c051d, c129fba5)
- Convert `apps/desktop/scripts/` to TypeScript and run every script through one runner (a5890bf7, 7f7d3109, 2b36ddd2)
- Measure the indexing hot paths and record what three spikes actually found, including the rule they refuted (aa7656d0,
  ec948e23, 5665a049, 403a35e5)

## [0.34.0] - 2026-07-18

1. Ask Cmdr: a built-in AI assistant that has access to your files to answer questions (alpha, read-only).
2. Image index: Find your photos via natural language, by describing the stuff / text (OCR) inside them, fully
   on-device.
3. Operation log: Review and roll back (undo) any non-destructive file change.

Plus MCP improvements and tons of bug fixes incl. CPU use improvements.

### Added

- Add Ask Cmdr, a built-in AI assistant that answers questions about your files (alpha): saved sessions, cross-thread
  search, file attachments, and visible cost (46f5719e, 19618827, c067693b, 6a2ec7b9)
- Find your photos by description, by the text inside them, or by tag, and find visually similar images, all on-device
  (0b6e164b, 8efd2bf2, 3a43ff32)
- Control image indexing under AI › Image search: a depth slider, live progress and ETA, opt-in for other drives, and
  reclaimed disk space when you narrow it (6b56d195, bf2ffe5d, e293d63f, 47113476, 5bb09aab, ed1c660f)
- Review and roll back any file change from the new Operation log (View menu, ⌘⌥L); alpha (39282ade, 4cfde6e5)
- Compress files into a new zip from the Transfer dialog, with a compression-level slider, a live size estimate, and
  SMB/MTP destinations (cea1631b, 0dddc0e8, 50aeda6c)
- Open password-protected 7z and WinZip-AES encrypted zips (506d07f3)
- Index local external drives (USB sticks, SD cards, extra disks) so search covers them too (53e52e8b, 03497f8c)
- Search every indexed volume at once, not just the current drive, and answer "how many" with a count instead of a full
  list (a141cc24, 4426bffc)
- Let external AI agents rename, create, tag, favorite, eject, pick trash-vs-delete, drive the operation queue, and read
  indexing status and folder importance over MCP (e59f95af, a723a623, d8c6dc3c)
- See what's new after an update in a redesigned popup, and open the full changelog from Cmdr › Changelog… (bacc1d9e)

### Changed

- Scan local drives up to 2.8x faster with a bulk-stat walker (b2b30ac4, d6185b65)
- Use less CPU and memory while indexing: lower-priority background threads, throttled live writes, and a lighter file
  list (2671c00d, 846cf21a, 6032386d)
- Auto-reconnect an SMB share and resume its index after a disconnect or restart (499027b5)

### Fixed

- Keep folder sizes accurate after big deletes and creates; existing drives repair drifted sizes automatically on the
  next launch (3d13ab7d, acefb9a6)
- Show an honest "indexing stopped" badge when a drive's index fails, instead of spinning silently (1cc60856)
- Refresh Google Drive panes live after rename, create, and delete (b0ccff5e)
- Never freeze at launch or stall state reads on a hung network mount (cb669ab2)
- Fix scoped search silently returning nothing for `/tmp` and `/var` paths (1a448f17)
- Fix scoped filename search returning no results on a NAS (bdcdb7ab)
- Match image tag search regardless of case (092696aa)
- Stop MCP-created or moved files landing in the wrong pane (2125b522)
- Prompt for SMB credentials instead of falling back to the CLI when a share rejects guest access (9cbd0277)

### Non-app

- Split the largest source files into cohesive modules for maintainability (0cf01dc0)
- Add Ask Cmdr and the Operation log to the website features page (14c3d230, 6735cc6d)
- Bump `spin` off the yanked 0.9.8 (5eb645bf)

## [0.33.0] - 2026-07-09

**Archives open like folders, the clipboard pastes straight into files, and search shows your best files first.**

1. Browse and edit `.zip`, `.tar`, `.tar.gz`, and `.7z` archives like folders, even ones on a phone or SMB share.
2. Paste text or an image straight into a new file with ⌘V.
3. Search ranks your most interesting files first.

### Added

- Browse, extract, edit archives: `.zip`, `.tar`, `.tar.gz`, and `.7z`. Fully edit zips (create, rename, delete, and
  move files inside), unlock password-protected zips, even on SMB and MTP (179466f8, 8e15d86b, f4fa09a4, 8d80f012,
  2103b2fa, 778dddfd, 8e001cb9, e85cc448, f5c97511, 5efe4ba1, 82f39461, 54d20851)
- Paste clipboard text, an image, or a PDF as a file into the current folder with ⌘V (b0de3824)
- Add a folder-importance subsystem: a tunable scoring API any expensive feature can consume, with a measured eval suite
  for weight tuning (08d1d6dc, a435fb39, 513ff76b, 60fd27df, 02d156c3)

### Changed

- Rank your more interesting files higher in search results (1a998e47)
- Extract large folders from compressed `.tar` and solid `.7z` archives much faster (be11894e)

### Fixed

- Fix a moved or deleted file lingering in the source pane on MTP devices until manual refresh (bd8dc8de)
- Keep the inline rename box on the right file when the folder reorders underneath it (5e5ee92d)
- Stop a background folder load in one pane from clearing the other pane's messages (d22ab4c9)
- Say "not supported" instead of "damaged archive" when a `.7z` uses encryption Cmdr can't read (b7ae624e)

### Non-app

- Upgrade the website to Astro 7 for even more Rust! (81d0f575)
- Consume smb2 0.12.0 from crates.io, dropping the local FileReader patch (481d6834)

## [0.32.0] - 2026-07-01

**Design polish across the app, plus filesystem-aware copies.**

1. Refreshed colors, icons, dropdowns, and text alignment across the app.
2. Every volume shows its filesystem: APFS, exFAT, FAT32, and more.
3. Copying a file over 4 GB to a FAT32 drive is blocked before it fails.

### Added

- Block copying or moving a file too large for the destination drive (f177b604, e0450ca8)
- Show each real volume's filesystem (APFS, exFAT, FAT32, ext4, etc.) in the volume picker (c34d10de)

### Changed

- Redesign every modal dialog to the macOS layout: left-aligned titles and text, right-aligned buttons, labeled Action
  and Route rows, folder and file icons, and tooltips on the scan status icons (95191e2e, b19ccb45)
- Make the Copy/Move destination box forgiving: accept `~` and `~/…`, show the home folder in full, and create a missing
  destination folder on confirm on every drive (b19ccb45)
- Replace UI emoji with themeable Lucide icons across dialogs, menus, settings, and the network browser for sharper
  contrast in light and dark (ffd03c90, 5baba851, 48f3561f)
- Redesign the Select dropdown as a native macOS pop-up button with a frosted-glass menu that opens over the trigger
  (643f4200)
- Brighten dark-mode secondary text and the selection red for clearer readability (bbe29581)

### Fixed

- Fix the Homebrew install silently failing for new users on Homebrew 6 (now runs the required tap-trust step), and stop
  onboarding text showing literal `&gt;` and `&amp;` (dbceea71)

### Non-app

- Enforce an APCA Lc-45 contrast floor alongside WCAG 2.2 AA, clearing the last low-contrast spots so every text pair
  passes both (f6ccc188, e28b9a0e, cf33ac82)
- Add the full Tailwind v4 OKLCH color scale as reusable design tokens (3a2809b2)
- Upgrade the Node toolchain to 26 and relax the dependency cooldown to 3 days (4c5ef483, 714caeae)

## [0.31.0] - 2026-06-30

**Finder color tags, a nicer indexing UI, and much faster network and phone scans.**

1. See and set macOS Finder color tags right from Cmdr.
2. A refreshed drive-indexing UI with live folder sizes during scans.
3. Network and phone scans finish several times faster than before.

### Added

- Add macOS Finder colored tags (86a9ca38, 6039e1a6, 4d87d4ec)
- Show a per-drive indexing checklist (find files, save list, compute sizes, catch up) with live counts and a per-step
  ETA (4a74312f, a92f9cdb, 138bdfa8, 519a27ea)

### Changed

- Show folder sizes growing live during a network drive or phone scan, not only on the local disk (5a86abaf, ee9ee757)
- Speed up network and phone drive scans by listing directories concurrently, dropping long scans from minutes to
  seconds (a003f004, 6518b565)

### Fixed

- Fix the indexing progress counter freezing mid-scan, making a healthy scan look stuck (7568931c)
- Fix one drive's scan lighting the size-updating hourglass on folders of every drive (d4105d98)
- Fix a failed local scan sticking on a spinner instead of offering a rescan (61c66a0c)
- Fix a network first scan stalling for hours on NAS snapshot folders (bb64ad38)
- Fix a reindex wedging on a large set of changes (12e98e52, e4e13ed9)
- Fix folder size totals double-counting hardlinked files during a rescan (ca4151e6)
- Fix search, Go to path, and AI navigation sometimes opening a path on the wrong drive (3024839e, b029c435, ab44a722,
  f6e93c23)
- Stop the file index wasting disk space after a version upgrade (1536d307)
- Fix the file viewer failing to load lines when a file's line count is still unknown (83ad3ceb)

### Non-app

- Refactor navigation onto a first-class (volume, path) Location type, deleting bare-path navigation (bb6ef69c,
  3eabcec5, e2f4e601, 0d189b23)
- Bump smb2 to 0.11.4, demoting per-frame SMB protocol logs to TRACE (676e24b9, 5e6d163e)

## [0.30.0] - 2026-06-28

**Live folder sizes, browse while transfers run, and smoother mouse navigation.**

1. Watch folder sizes fill in while indexing runs.
2. Browse your phone while a copy, move, or delete is underway.
3. Smoother mouse-driven navigation, plus faster network-drive rescans.

### Added

- MTP: Browse a phone while a transfer runs (06d1874d, 4a01ad7f, f002606d, edc89aa2)
- Navigate pane history with mouse's back/forward side buttons (fcf34143)
- Click breadcrumb segments to jump to any ancestor folder, double-click empty pane spce to go to parent (dcc5b2e7)
- Explain why a phone's folders add up to less than its used space (caedb655)

### Changed

- Show folder sizes while indexing: ≥lower-bound when partially scanned, also unknown and stale (494849a9, d9dbf076,
  c4b20c96, fdadfc8f, 9f318e74)
- Speed up SMB and MTP rescans: update in place and keep last-known sizes visible while scanning (a6a2f586)
- Stop showing indexing notif and free space for DMGs, and show the read-only lock for read-only mounts (889859c4,
  1ea48634)

### Fixed

- Fix progress bars for cross-volume folder copy and move (38c405ec)
- Fix a UI freeze when starting a manual rescan (880688c9)
- Fix enabling or rescanning an SMB share or MTP device indexing nothing (d4527575, a8007894)
- Show the indexing indicator for SMB and MTP drives, not just the local disk (ef6005d4)
- Keep an honest stale index when a drive disconnects mid-scan instead of marking it complete (4d66beb0)
- Rebuild falsely-complete network indexes from earlier builds on upgrade, no manual action (3109ab69)
- Detect and explain Linux MTP permission denials from missing udev rules (51eee35d)

### Security

- Patch quinn-proto (remote memory exhaustion) and memmap2 advisories (584aa27f)

### Non-app

- Add a Total Commander vs Cmdr blog post (f4ce564d, d744a380, 8190a090, c0fdfd76)
- Surface the Homebrew install (`brew install --cask cmdr`) on the website and README (c2e4ed54)
- Attach a PII-free machine snapshot (model, RAM, disk headroom, index size) to error and crash reports (d148af1b)
- Migrate the MTP backend to the backend-neutral mtp-rs API for future Windows support (03f14279, 08a5059a, 71b3d580)
- Split the transfer and indexing modules into focused submodules (2597038a, fe8b414d, 4d65dcd0, e5005ca9, 194190fa)

## [0.29.0] - 2026-06-22

**Four big ones: pause/resume, a transfer queue, indexing everywhere, and nine languages.**

1. Copy, move, and delete operations can pause and resume.
2. Operations can be queued to run one after another.
3. Drive indexing now covers every volume type, including SMB shares and MTP devices.
4. Cmdr is now translated into nine languages.

### Added

- Translate Cmdr into nine languages: German, Spanish, French, Hungarian, Dutch, Brazilian Portuguese, Swedish,
  Vietnamese, and Simplified Chinese (5af98fea, 43b7f4c2, 042c7b01, a34ef72f)
- Pause/resume any operation (eeef1e2f)
- Add a Queue window for ops, with pause/resume/cancel plus multi-select (c06b485d, e279945b, 49c7b126)
- Add Pause/Resume and Queue (F2) controls to transfer progress dialog (07dd837c)
- Index SMB shares and MTP devices so folder sizes and search work, with scanning/fresh/stale statuses (384bffe2,
  7b084cdf, 049e9f49, e4cdbb8f, 386e9c13, fbacdbd0)
- Add a per-drive index status badge and menu in the volume switcher (a36e7033, eaa2eea0)
- Add drive-indexing controls in Settings, a "index this drive?" prompt, and a one-time "drive stale" notice (bcd433ae,
  0dddb45c)
- Show a live file count while a drive index scans, instead of a frozen label (eca50e21)
- Add ⌘↓ to open the item under the cursor, ⌘⌫ to move it to the trash, and ⇧- to deselect files (54e8bdeb)

### Changed

- Keep MTP devices responsive during a background index scan: navigation, copy, and delete no longer stall behind it
  (0fa3faf9)
- Refresh only the affected folder on MTP changes, instead of every open pane on the device (7a08831a)
- Honor macOS Reduce transparency app-wide: every translucent surface goes opaque when the setting is on (298bdede)
- Go back to the SMB host list with ⌘↑ in the share list, matching Backspace (1115440a)

### Fixed

- Fix the error reporter crashing on log lines with accented characters or emoji (72a800ee)

### Non-app

- Speed up releases by reusing a persistent cargo build dirs and mise cache across architectures and releases (bc2b3779)
- Build the translation methodology: per-lang style guides, glossaries, and a reference-pile across 139 languages
  (45b6a7dd, 0759d720, ece168ea, fbddd165)

## [0.28.0] - 2026-06-19

The file viewer now renders images and PDFs inline, local and custom AI endpoints like Ollama and LM Studio work, and
counts and file sizes follow your Mac's region. The volume selector also gets a frosted-glass look.

### Added

- Show images and PDFs inline in the file viewer, with a Text/Image/PDF mode switch and a view-as-text fallback
  (ccfb536c, c03c0715, e46cc1be)
- Give the file viewer its own working menu bar on macOS: File, Edit, and View (with Word wrap) when it's focused
  (60b7b568)

### Changed

- Give the volume selector a frosted-glass material and honor macOS Reduce transparency across the app (a10d7def)
- Keep the volume selector open while ejecting, and make its row menu native (84fe8c66)
- Major Settings revamp: Group Settings pages into cards and make Advanced settings findable from the main search
  (3f9168ce, 43fb5ad1, 027a89ed)
- Format counts and file sizes by your Mac's region instead of always US formatting (0324047b, 83906c5a)
- Show real macOS default icons while the icon cache loads, replacing the emoji placeholders (8ea3a54a, 7272df9d,
  9b41bcc3)

### Fixed

- Fix local and custom AI endpoints (Ollama, LM Studio): the model picker now selects, and keyless endpoints register as
  configured (e8389003)
- MTP: Heal a stale destination folder on MTP upload and retry instead of failing the copy (010d8b45)
- File viewer: Scroll search matches into view, and enable cut and paste in the search box (0496700a)
- Show the running app version in copied diagnostic info, not a stale hardcoded one (32bce781)
- Harden the backend against silent crashes and unsafe-code mistakes, and clear out dead code (d1e4f76f, 6d2acfb0,
  ab34d853)
- Update the MTP library to 0.20.0 for transaction-ID self-heal and stale-handle recovery (7fedfadc)

### Non-app

- Lay the full groundwork for translating Cmdr into other languages (English-only for now): a message catalog of ~2,070
  strings, region-aware number and date formatting, tooling that finds clipped text and screenshots every screen for
  translators, and a Language picker (56acb6c1, 17e05af8, 2b085afc, 375600ce, 8af5a0bb, a3a9ef3c)
- Move all error wording to the frontend so it's ready to translate, with the logic staying in Rust (1e918e06, 77a851b8)
- Cut the docs that load into every AI coding session by two-thirds, and add checks to keep them lean (b84ca26a,
  1ce6e7bb, 3dad7e03)
- Route every in-app icon and spinner through shared components, and split several oversized files into focused ones
  (94b6218a, 751e9bc4, d3c50a87)
- Keep automated test runs from disturbing the developer's real apps, data, and keychain (28b6bcaf, 2476aba4, 3a56d765)

## [0.27.0] - 2026-06-14

You can now add/rename/reorder/remove Favorite folders in the Volume selector, hide the bottom F5/F6/F8 bar, and can set
Full mode to display filenames+extensions in one column. Also added `Help > Keyboard shortcuts`, a What's new popup that
shows up after Cmdr updates, and improved the Full Disk Access part of the onboarding.

### Added

- Show a What's new popup after Cmdr updates, with the changelog since the version you last saw and an opt-out in
  Settings (4e5ccbba, cc222919, 04f75ddb, 9ca6c524)
- Curate your favorites in the volume switcher: add (command palette, Go menu, or right-click a folder), rename, reorder
  by drag or ⌥↑/⌥↓, and remove (c660d6f4, 685fcac5, 335331ef, 608b8c81, d3db386f, 9dc2e968, e3acd2a4)
- Add a Help > Keyboard shortcuts window: a scannable reference of every command's shortcuts, live-synced to your
  customizations (3bcbc285)
- Add a setting to show full filenames in the Name column instead of splitting off the extension (27060493)
- Add a setting to hide the bottom function key bar (950a213c)

### Changed

- Onboarding now detects Full Disk Access the instant you grant it, and gets Cmdr into the macOS 13+ Full Disk Access
  list (dbf4d70b, 19e992dc)
- The AI cloud model picker now loads its list on open and keeps it when you reopen (f8aa514d)
- In Search and Select, ⌥←/⌥→ now move by word in the query field instead of navigating folders (dd8573b2)
- Search now remembers your query when you open a single result, not only "Open in pane" (5eae2139)

### Fixed

- Fix dragging a file from Cmdr into a browser upload field doing nothing (7c338b51)
- Fix the file viewer misreading some binaries as UTF-16, which slowed the open by about a second (8f069f28)
- Fix the downloads jump re-opening a folder already shown in the other pane (9eee5395)
- Fix Search abbreviating paths that fit the column (3e558c7f)
- Fix a rare drive-indexer race that could lose a folder's size (439d7fcb)
- Stop local AI logging an error when you turn it off while it's still starting (1c8363b4)

### Non-app

- Add a KV-backed `?r=` short-code system so tracking links expand to UTM params without a website deploy (f2b2c465,
  7a532406)
- Fix silently-broken Umami and PostHog injection (website analytics had stopped loading), and add a check guarding the
  regression (9cb620e8, 36d85974)
- Add a per-day acquisition funnel with first-touch channel attribution to the analytics dashboard (8cae7906, a1dd804e,
  a011de50)
- Split the analytics dashboard into Acquisition, Product, and Link codes pages (83eb55be)
- Converge the app's dropdowns onto two reusable Ark primitives for a consistent macOS-native look (d282fdba, 6ac9016e,
  a705696d, 69130e27, 5f355670)
- Add a `docs-reachable` check keeping every doc linked from the repo root, and connect the orphaned docs (69e91dbe,
  185afddb, 74ef31ee, 36b7075b)
- Quiet the drive indexer's UNIQUE-conflict warning to fire only when two writers are racing the database (ba5a538c)
- Ban two-column tables in agent-facing docs and convert all 130 existing ones, with a check enforcing it (a909679b)

## [0.26.0] - 2026-06-11

This release sharpens the Search and Select dialogs: a Files or Folders filter, folders matched by size, an AI strip
that shows what the agent did, and your last query waiting for you on reopen. File-list dates and sizes now line up into
clean columns, and you can install Cmdr from Homebrew.

### Added

- Add a Files or Folders filter to Search and Select, matching folders by their recursive size (600b23ca)
- Add an AI strip to Search and Select that shows the pattern and filters the agent set, with a spinner while it
  translates (2328f469)
- Install and upgrade Cmdr from Homebrew with `brew tap vdavid/tap && brew install --cask cmdr` (65729ee8, 6490cb16)
- Add a Discord community link to the About window and website footer (f65050b5)

### Changed

- Search and Select now remember your mode, text, and filters, and show your last results the moment you reopen
  (a5c60359, df819509)
- Keep your typed text when switching filename, regex, and AI modes, and land the cursor on the first file after a
  Select (8c90428e)
- Line up the Modified and Size columns with tabular figures, and default to ISO 8601 dates (b84d6877)
- Enlarge the Search and Select dialog text a step and clear it to AA contrast (9effb0e5)

### Fixed

- Fix Select doing nothing when you set only a size or date filter and leave the name empty (89204c28)
- Fix the size filter ignoring a `0` bound, and add a one-click `=` comparator (0071a009)
- Fix an AI search keeping a stale size or date filter from the previous run (69ca52e5)
- Fix the onboarding AI step's provider list overflowing the options below it, and two stale provider links (44c905c1)
- Fix commercial purchases not issuing a license (5e053ee6)

### Non-app

- Dashboard download count now means new installs, with a new-vs-update chart (7ff2b6f3)
- Add a feedback and error-report section to the private dashboard (e449b007)
- Add a `/feedback-and-error-digest-from-app` command for agents (77b49d09)
- Cap each dashboard data source at 20s so one hung upstream can't 524 the page (8b2909a0)
- Capture real app screenshots and add a tracked `brand/` asset home (1b38da54, 8fa73633)
- Promote the Search and Select chip and popover primitives to `lib/ui` (14abab0d)
- Quiet noisy dev-run logs (812e0bb5)
- Fail the website build if a sandbox Paddle token would ship to production (7d227942)
- Show a Discord invite modal after a website download (46829c22)
- Use david@getcmdr.com as the public contact address (a667f7ce)
- Clone `target/` to skip the full Rust rebuild on a fresh worktree (a2cbfce2)

## [0.25.0] - 2026-06-11

Cmdr is now an open beta: stability badges, a Send feedback channel, and anonymous usage stats you can opt out of. SMB
sign-in got smoother, and keyboard shortcut customization got a deep round of fixes.

### Added

- Mark Cmdr as an open beta in onboarding and the About window, with a personal intro from David (7ce2c5e4, b2b27d8f)
- Add a Send feedback dialog (Help menu, command palette); notes go straight to David (79c4a6c9, 6bdb188a)
- Add stability badges (ALPHA, BETA) in the app and a feature status page on the website (219549db)
- Add anonymous beta usage analytics (daily-active count, PII-free feature events), disclosed during onboarding, opt-out
  under Settings > Updates & privacy (d1c481f0, c328bb13, b2b27d8f)
- Group crash and error reports per install, with an optional reply-to email so David can follow up (71da738c)
- Add a progress bar, percent, and ETA to drive indexing, now a calm hourglass with details on hover (bc824f18,
  6defbf74, b03387e2, f8694ce8)
- Add a low-disk-space warning (in-app toast or macOS notification), configurable under Settings > Behavior (15ad9cf9)
- Drag files from your phone or NAS straight to Finder or the Desktop, with a toast tracking the download (c97a032f,
  9e54719d)
- Teach the go-to-latest shortcuts (⌘J in-app, ⌃⌥⌘J from anywhere) in the downloads toast, now collapsible (1da0b835,
  9ab2cf4f, 15fc9395)
- Shortcut hints across the app (F-key bar, toasts, onboarding) now follow your custom bindings live (123e76b7,
  e756a379, 18acf50f)
- Click any shortcut hint to jump to its row in Settings (b38f6cf8)
- Offer Finder's saved SMB password on "Connect directly", so a Finder-known share connects without retyping (2ccb45de,
  3b07b0f2)
- Prompt for a fresh sign-in when a NAS password changes, instead of a misleading "unreachable" banner (7c654e70)
- Add drag auto-scroll near a pane's top or bottom edge (6d1ca01b)
- Prepare `brew install --cask cmdr` for installing Cmdr via Homebrew (9348f888)

### Changed

- Reuse a saved SMB password instead of re-prompting on every connect (d12f8d3d, 7c654e70)

### Fixed

- Fix connecting to a password-protected NAS dead-ending in macOS's cryptic "error code -6600"; Cmdr now shows its own
  login form right where you are (0e1bc77d)
- Picking an already-mounted share now goes straight there, even under a different name (Bonjour vs IP) (0e1bc77d)
- Fix the wrong-password message and a stale connection dot after an SMB sign-in fails (5846d351)
- Fix cloud AI for Groq, OpenRouter, DeepSeek, and Mistral (they were routed to the wrong API) (08aa31e1)
- AI search applies its translation again and reports failures (out of quota, bad key, timeout) instead of silently
  doing nothing (11f59ea1)
- Move a stranded plaintext AI key from `settings.json` into the OS secret store (c9d45e09)
- Fix copying or moving an empty folder silently doing nothing, and across drives deleting the source (5053ea0b)
- Fix the file viewer cutting off the file after about 60 lines with word wrap on (0655dc0b)
- Fix dialogs leaking focus into the background and locking out the keyboard after two Tabs (f2e04973)
- Closing Search or Select files with Esc no longer kills pane keyboard navigation (040d424e)
- Fix ⌘A doing nothing in the Settings and viewer windows (d99fafc1)
- Fix drag-out from a phone or network pane dropping a junk `.textClipping` file or pasting a meaningless path
  (6e8ac5ae)
- Fix index rename failing when the destination name is already taken (dea07427)
- Harden shortcut capture: bare keys don't fire mid-typing, and macOS-owned combos (⌘Space, ⌃↑) warn instead of saving
  silently (a412e599, 92c5ad4b, 2b7abf3f)
- Fix custom shortcut rebinds and removals not sticking, not reaching other windows, or missing conflict detection
  (6c21fd1b, da570566, 2247dac1, a1dae889, add4db81)
- The command palette and the Keyboard shortcuts editor now show your real bindings and list every command (87df2ed9,
  73766c9e, 762b3951, 396097ff)
- Focus the textarea when the feedback or error report dialog opens (6f295fc6)
- Show "/" instead of a raw storage id (like "65537") in the tab title at a phone or camera storage root (582cfbaf)

### Non-app

- Rewrite the website around one honest feature list (a bento grid by capability), in a product-first voice (272d177e,
  e975bd0c, 6ccb8aeb)
- Cut the landing page from ~2.3 MB to ~0.4 MB and remove render-blocking CSS (5fc6729a, fbacb4e9)
- Replace stringly-typed backend event emits with a typed event bus across volumes, write ops, indexing, MTP, network,
  git, and AI (f2d3febf, 57e9c87d, 5f510bd2)
- Split colocated docs into `CLAUDE.md` and `DETAILS.md`) across ~30 areas, add `claude-md-length` check (9bf1a653,
  bb26f2df)

## [0.24.0] - 2026-06-06

Go to path (⌘G) lands, folders merge on copy and move, and same-volume moves are instant.

### Added

- Go to path (⌘G): jump anywhere by typing or pasting a path, with `~` expansion, recent paths on digit keys, clipboard
  prefill, and a nearest-existing-ancestor fallback when the path doesn't exist (2a87c01b, afa2fe18, 6b3e941b,
  3a768fcc, 07877792)
- Block ejecting a volume while a copy, move, or delete is touching it (fe2a0987)

### Changed

- Folders always merge on copy and move: your conflict choice (skip, overwrite, or rename) applies to the clashing files
  inside, and dest-only files survive (89cd978c, 6e305a47)
- Same-volume moves are instant: moving within one drive, share, or phone is a rename, no more 30–40 s "Verifying before
  move…" on a big NAS folder (a9743ecc, 114e5d2d)
- Completion toasts now report what you selected, split by type: "Moved 1 file and 3 folders" (ae629609, f977ed95)
- Disable Rollback for same-volume moves (a rename has nothing to roll back); Cancel stays available (f069e37e)
- Rename "Reveal latest download" to "Go to latest download" in the menu, palette, and settings (49ddaf0a)

### Fixed

- Resolve conflicts file by file inside folder merges on network and phone drives; a newer file deep in the tree no
  longer loses behind a single folder-level OK (6e305a47)
- Fix dropping files from the Desktop, Documents, or Downloads failing with "Source volume not found" (c3021243)
- Fix drags from phone and network panes reading 0 bytes / 0 files in the transfer dialog (c3021243)
- Dropping onto a read-only volume now shows the "Read-only device" alert instead of a copy dialog that can't succeed
  (62bbc09a)
- Fix the Copy→Move toggle zeroing the transfer dialog counters on local moves (f4a8b1cb)
- Show the volume name instead of a raw storage id (like "65538") in the transfer dialog header (f4a8b1cb)
- Fix file viewer settings (word wrap, text size, binary warning) silently resetting every session (51e127aa)
- Make the title bar draggable while a dialog is open, and in the file viewer window (016abbdf, e28e8905)
- Highlight cloud drives (iCloud, Dropbox, Google Drive) in the volume switcher instead of Macintosh HD (28e72ac0)
- Fix tooltips jumping to the window corner in big folders (2b45ec08)
- Fix a rare hang when answering a copy/move conflict prompt (070b8d15, 99271478)

### Non-app

- Rebuild the explorer frontend architecture: a module state store, a typed command bus across every entry path
  (keyboard, palette, menu, F-bar, MCP), one transactional `navigate()`, a per-kind volume capability table, and a flat
  command handler record replacing an 89-case switch (062ebbb7, 5709b50a, ef52db45, 6270612c, 6aaf82d0, c7c0f5d6)
- Add a virtual MTP device for dev: `CMDR_VIRTUAL_MTP=1 pnpm dev` plugs in a fake "Virtual Pixel 9", no hardware needed
  (9b9a4cad)
- Make the SMB test containers safe to share across concurrent agent sessions: lease-refcounted teardown, auto-restart,
  and resource caps (b4307236, 7905a4ea, 7ae14a75)
- Stop E2E builds from uploading error reports to the live channel (293853b0)

## [0.23.0] - 2026-06-01

A guided onboarding wizard, a Downloads watcher with a jump-to-latest shortcut, and AI-powered file selection. Under the
hood, copy and move became durable and crash-safe.

### Added

- Onboarding wizard: a multi-step soft sheet (Full Disk Access, AI provider, optional setup) replacing the single
  permission modal, reopenable from the menu, command palette, and MCP (alpha version!) (5a21bdba, 742ff625, 963b4bf1,
  88ecdfaa, 7d081d2c, a09631c9)
- Downloads watcher: a toast or native notification when a download lands, and jump to the latest download with ⌘J or a
  global ⌃⌥⌘J hotkey (092203db, a9466e5a, 853a28a0, d378f42f, 1484c4f0, 2c3e36c3)
- Select or deselect files by query: a new Select menu plus a Select files… dialog with filters and AI-powered
  natural-language selection ("select all error logs from last week") (alpha version!) (1fd163c4, 7ce90bb3, 8d5bd3dc,
  dcb4b3a9, 6d68def3, ac68709e)
- File viewer tail mode (F): follow a file live as it grows (8a6671de, ed479d2b, a7eb8d87, 29a25ffc)
- File viewer char encoding picker: switch text encoding instantly, with strict ISO-8859-1 and UTF-16 BOM detection
  (a2270782, 0c0b8716, b1277906, 3978ed4c)
- File viewer regex and case-sensitive search toggles (⌘⌥R, ⌘⌥C) (7d424d97, 48b5de06)
- Real folder icons in the list: system folders (Downloads, Desktop), packages (.app), and custom-icon folders, cached
  across restarts (1dd439d0, 389829bf, e50004ab, 418a86a9)
- Per-directory "size updating" hourglass and progressive folder-size reveal as the index fills in, instead of waiting
  up to 5 minutes (0afc10b4, f3740152, 66712c2d)

### Changed

- Redesign the type-mismatch conflict dialog: one consistent layout across all clash types, with a clear warning when
  overwriting a whole folder with a file (a3faa3d8, d2b8f153, 79024932, 66df6570)

### Fixed

- Make copy and move durable before reporting "complete". Ejecting a USB stick right after a copy no longer loses files
  (bdb3b61a)
- Make cross-volume Overwrite crash-safe: stream to a temp file and swap in place, so a mid-transfer disconnect keeps
  the original (6e99640e)
- Stop concurrent indexing from corrupting the index (fixes inflated folder size display) and keep the index WAL bounded
  (0236723d, eb692287, b849ee01)
- Make config and secret-store writes survive power loss, protecting saved SMB servers, passwords, and AI keys
  (aea4aa0b, 57a47b63)
- Stream MTP uploads instead of buffering the whole file in RAM, and make Cancel stop in-flight USB writes (a0140150)
- Fix cross-volume moves showing "Moving... 0 bytes / 0 files" for the whole transfer (now real scan and per-file
  progress) (067b96db)
- Open file viewer instantly even under heavy FS activity (was up to 730 ms and could time out) (aa9905f1)
- Keep live indexing alive under database lock contention (9e808914)
- Fix a git-repo watcher leak during fast navigation (a0bac502)
- Stop losing Full Disk Access and onboarding state on a save failure (which re-ran onboarding) (5c46d887)
- Error instead of silently overwriting when creating a file that already exists (25ce82f4)
- Fix Enter or Backspace on ".." from "~" landing at "/" instead of "/Users" (a8096a25)
- Fix SMB share listing on servers with many shares (native enumeration handles fragmented replies) (fe5569cf)

### Security

- Require bearer token for destructive MCP ops (68e337ef, 18cd4c35)
- Redact PII from MCP logs and state (8ea092ba)
- Close SMB password leak through process arg list in an edge case (a190f19c, 0a154f21)
- Reject plaintext-HTTP AI endpoints that carry an API key (3dd10609)
- Fix an updater AppleScript injection via the app bundle path (5875fb4c)
- Narrow down the FS capability to actually needed files, restrict Debug window's capabilities (6cabc94c)
- Redact SMB credential URLs from debug logs (d7edb8a4)

### Non-app

A big push on dev tooling: the check suite is roughly twice as fast overall, with some checks 30–40x.

- Add CPU-weight-aware scheduling (46bfae99)
- Add `--graph` arg to checker script to view the dep graph (46bfae99)
- Split `eslint-typecheck` into TS / Svelte: 616s to ~15s (~40x speed-up!) (10632789)
- Stop clippy forcing full crate rebuild every run: ~32s to ~1–2s warm, also sped up other Rust checks (3318f29c)
- Switch Svelte tests to happy-dom (22% faster) (ca6b13d9)
- Add per-instance isolation (`CMDR_INSTANCE_ID`). Parallel dev sessions now get own ports, data dir, and Keychain
  (3bcd2ed4)
- Add a `lock-poison` static check and pnpm install-side supply-chain guardrails (14-day cooldown, trust no-downgrade)
  (038c5ec2, d568789f)

## [0.22.0] - 2026-05-23

The Search dialog got a full redesign, and the file viewer learned text selection and copy.

### Added

- Redesign Search around one unified bar with mode chips for AI, filename, and regex, each remembering its own typed
  query, and keep the dialog's state when you close and reopen it (62aef440, ac4c6340, b9ca1e6f, 3ea1b45e, 5c35d9ea,
  9b8f9dd7, 71c9485b)
- Filter searches with size and modified-date chips that open quick popovers, and see the AI's interpreted prompt and
  caveats right in the dialog (2c10bba7, 807e456e)
- Replay past searches from a recent-searches history with quick-pick chips, and auto-apply filename and regex queries
  as you type (1f03ff49, f4eea79d)
- Act on search results in place: clickable path pills, per-row menus, "Show all in main window", and copy, move, or
  delete files straight from the results (e52c6dec, d94187bd, c79c1112, 4770a93f, e7afc8b3, 1b1fc5ab, f3f45084)
- Select and copy text in the file viewer (files up to 100 MB): double/triple-click for word/line, right-click menu
  (6f717829, 1e061820, 8d6f85c0, 46f278bb, e329bb39, 1445c2d7)
- Eject ejectable volumes (USB, SMB, DMG) from the picker and the breadcrumb right-click menu (2a7e256f)
- Replace the human-friendly size units toggle with a 5-way size unit picker (dynamic / bytes / kB / MB / GB) (78a7f367)
- Show climbing bytes and dirs during MTP/SMB scan previews (was "0 / N / 0" until done) (c2b5a040)
- Reuse scan-preview cache for local delete and cross-FS move so the dialog skips straight to the active phase
  (9445e61a)
- Center child windows on the main window; file viewers cascade (8cd06bf4)
- Tint zebra stripes with the pane bg so per-volume tinting actually shows through (b84e761e)
- Toast confirms zoom changes and points at ⌘0 to reset (37f94410)
- Show a Space hint on "Toggle selection" in the right-click menu (a24613d9)
- Blue info toasts, new colorless `default` level for low-importance feedback, and reclassify routine confirmations and
  soft refusals (dabf0e3a, 51e30112)

### Fixed

- Fix SMB share mis-loading local paths after a volume switch (3e613ca6)
- Fix volume copy dialog wedging open after SMB/MTP cancel (0fbafebb)
- Process selected files in pane sort order, not Cmd+click order (39fc8d2e)
- Cursor lands on the new folder, not the row below (38ebdc87)
- Fix Full view ".." row hiding behind the header after PageDown/PageUp (6ddb4273)
- Fix viewer ⌘A freezing on huge unindexed files (e29312bd)
- Cancel viewer reads within ~64 KB instead of 16 MB (0e758b46)
- Fix Escape on viewer context menu closing the whole window (4464f766)
- Honor `prefers-reduced-motion` in viewer drag autoscroll (aec327b8)
- Surface silent-band viewer copy failures as a warn toast (41398aca)
- Polish viewer copy dialogs: ⌘A routes to the right size tier, Enter triggers the primary action, Tab skips ×
  (b6542e7b)

### Non-app

- Build git test fixtures via `gix` instead of CLI shell-outs; 91 tests went from tens of seconds to ~1.7 s (532722c8)
- Trim slow tests under the 8 s nextest cap, drop the 30 s `cap_bundle_*` exception, make
  `index_mtime_change_invalidates_cache` deterministic (f4c0b5ad, 9e23ff2a, 44429405)
- Re-enable three previously-skipped E2E tests; the culprit was a Node fixture-helper dangling-symlink bug (915c5f33)
- Settings-style chrome and a live SMB diagnostics dashboard in the dev Debug window (6bd0f15c, e7660b3a)

## [0.21.0] - 2026-05-21

Quick Look (⇧Space) arrives, and Settings plus the main window now look properly macOS-native.

### Added

- Add Quick Look (⇧Space) (6778494b)
- Add ⌘← / ⌘→ to copy the cursor path between panes (a3e15f45)
- Add red binary-file warning in the file viewer (74e7b0cd)
- Redesign Settings window to look like System Settings (69480931, 76be4f8a, 9668a078, 91c31f35)
- Redesign tab bar, flatten panes for a more native-macOS look, fix UI glitches (dc7d6500, 9668a078, 3771570a, 79ed3b6c)

### Fixed

- Fix transfer dialog showing "✓ 0 files" when pre-flight scan beat the FE listeners (8525835c)
- Fix stale path events corrupting the breadcrumb after switching a pane to Network (a3e15f45)
- Fix Quick Look toast/content import cycle (b3d67fe6)

### Non-app

- Move `rust-toolchain.toml` to the workspace root so every crate pins one toolchain (fixes v0.20.0's
  `rustup target add` drift) (41e999ab)
- Add `workflows-rustup` check forbidding `rustup target/component add` in workflows (c68630ee)

## [0.20.0] - 2026-05-20

Snappier and safer transfers: MTP cancels land instantly, SMB writes pipeline over one session, and selection switched
to a high-contrast red. Cmdr now runs on macOS 12 Monterey, too.

### Added

- Cmd+click toggles selection (c6adee74)
- Bind `Insert` to toggle selection in Total Commander style (719e4f9b)
- Modify Shift+Arrow/Page/Home/End behavior to align more with other file managers (47932132)
- Switch selection to red. Clears WCAG AA across all backgrounds! (9028722c, 02b295da, 069bc400, 14a36dd8)
- Tint each pane's background by volume type (local/SMB/MTP) (3f5629d3)
- Improve MCP: replace fire-and-forgets with round-trips (48a9701c, 3c1b0dc9, e12285d1, df11caef)
- New MCP resources: `cmdr://logs` + filters, `cmdr://state` filters, `recentErrors`, `upgrade_smb_to_direct`, SMB
  connection state (e597d24d, 640c3330)
- SMB volumes auto-upgrade from OS-mount to direct smb2 sessions (640c3330)
- Copy/move/delete pre-flight scans reuse watcher-backed listings. Skip a 17s MTP re-list when the folder is already
  open in another pane! (9d434638, ba20ca3e, 49187230, fdebd329, b90b9003)
- SMB streaming writes no longer hold the client mutex (smb2 0.9). Concurrent writes pipeline over one session
  (3d0d5db7, 06bc5da7, ed4b6886)
- Bump SMB watcher to smb2 0.10 to stop losing events between polls (432d13ff)
- Localize macOS pane names in onboarding and error dialogs (points at what System Settings actually shows) (bad5d926)
- Honest transfer-complete toasts: report copied vs skipped separately (5cdf989e)
- Polish the license nudge: clearer copy and layout (95007952)
- Add fallback UI colors on macOS Monterey, achieving macOS 12.x compat! (5792b10e)
- Improve accent-fg to match WCAG AA+ against all colors, and add cursor outline (d00ba5b4)

### Fixed

- Propagate MTP cancel all the way to the USB layer; no more 30-second "Cancelling…" wedges (0de4c6b7, 1696355d,
  f894e60e, b4018891)
- No more empty-pane flicker on bulk ops (coalesced refresh events) (54674854, 13b486a8)
- Friendly message for SMB `STATUS_DELETE_PENDING` (was misleading "disk needs attention") (a560243b)
- Properly pluralize all words ("1 file"/"10 files") everywhere (eb360370)
- Fix MTP destination pane staying stale after cross-volume writes (873f1102)
- Fix SMB/MTP listing cache going stale when the watcher misses an event (1dea24e1, ab98ee88)
- Fix MTP delete not emitting `write-cancelled` when cancel landed mid-iteration (e21ca6d3)
- Fix transfer dialog wedging at "Cancelling…" when Cancel raced ahead of the `operationId` IPC (2b2a5ec6)
- Fix MCP `open_under_cursor` on the Network view (0aec8fbd)
- Fix Linux startup hanging on a half-configured D-Bus (probes now bounded by a 500 ms timeout) (91afacbf, 85580df9)
- Fix `refresh_listing` short-circuiting on local volumes during the FSEvents symlink race (57ef1034)
- Fix two SMB shares with the same case-folded name on different servers colliding on the same volume ID (f2414556)
- Fix opening a guest SMB share popping the kernel `smbfs` credential dialog (92119464)
- Fix `TransferErrorDialog` being see-through in the transient branch (f01af359)
- Fix error dialogs rendering OS strings with markdown bleed-through (`STATUS<em>DELETE</em>PENDING`) (dbd7a2ac)
- Fix Brief mode cursor stripe briefly spanning the entire pane while column widths load (d676efa5)
- Fix Move dialog hiding the Size progress bar (`bytes_total` was 0) (8856e012)
- Fix conflict-resolution radios reading "Skip all" / "Ask for each" when only one conflict exists (4eac76b4)
- Fix focused-button Enter firing the dialog's default action instead of the focused button (079a0ce1)
- Fix free-space numbers tier-coloring as red on healthy disks (8219a06c)
- Fix the AI offer prompting Intel Macs for a local-model download they can't run (52f3cd81)
- Fix every tokio task crashing when stderr becomes a broken pipe (31d97e06)
- Fix Linux compile errors in `errno.rs` and the MCP resources module (90b0afee)
- Fix Linux compile errors in `system_strings.rs` (macOS-only loctable items) (e852f04a)
- Fix `clippy::unnecessary_sort_by` on Linux volume sorting (1.95 picked it up) (03faf480)

### Non-app

- Cap every Rust test at 8 s (matches the Playwright convention), with documented exceptions (eb67f389)
- Stop gating `desktop-e2e-linux` on `desktop-rust` in CI (66a2e501)
- Harden the checker against supply-chain attacks: `--locked` everywhere, pinned tool versions, new
  `workflows-hardening` + `govulncheck` checks (7d771ca8)
- Declare `rustfmt` and `clippy` as required `rust-toolchain.toml` components (a23222eb)
- Trigger Rust CI on `rust-toolchain.toml` changes (0f8c9ffb)
- Dev override `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` to test the old-WebKit fallback on modern Macs (17537510)
- 14-day release-age gate via Renovate (3-day override for security advisories) (8bd5af1e)
- Shared `pluralize` helper for log/error/UI strings, plus a `pluralize-noun` check (0ae2ee92, ec277ba8, e070fc34)
- Force file-backed secret store under `CMDR_E2E_MODE=1` (no more Keychain prompts in unattended E2E) (ecb495fc)
- New `btn-restyle` check (forbids `.btn-*` overrides); accent-matrix in the contrast check (51f31939, 0e885f5d)
- Codify 100-char Rust comment width; reflow existing comments (b76b9277, 610f66f6)
- Vendor `smb-consumer-maxreadsize` and pin the SMB streaming-write no-deadlock invariant (200 × 1 MB at concurrency 8)
  (1ae6eec7, e8259eef, e750920b)
- Ticketed acquire/release logs on the `SmbVolume` client mutex (2e4aeb9d)
- E2E focus hygiene: viewer/settings windows skip OS focus, Escape-binding tests use synthetic dispatch (be21bebe,
  0dfdcb2a)
- Defensive disk-poll + refresh in the MTP→local copy E2E (9693b283)
- Stamp the running E2E test name into the main window's OS title (1181e0c1)
- Document the UTM Ubuntu VM loop for iterating Linux-only tests (917938ee)
- Switch `mtp-rs` to crates.io 0.15.0 (off the path dep) (f98313f0)

## [0.19.0] - 2026-05-16

Settings got reorganized into clear sections, the command palette remembers your recent commands, and you can type to
jump to a file.

### Added

- Reorganize Settings into Appearance, Behavior, File systems, Updates, AI, Network, Privacy, and Advanced (c3003a05)
- Add "Overwrite all smaller" and "Overwrite all older" conflict actions (2dfd17b8)
- ⌘⇧T reopens closed tabs; double-click the tab bar opens a new tab (65417fbe, d7a85a33)
- Move AI API keys to the OS keychain, with 300 ms debounced save (42bc5eaf, 10f8525b)
- Command palette recents on empty query (last 10, LRU, grouped, self-heals stale IDs) (d3406299, a2971aba)
- Type to jump to a file in the explorer (0b9f943f)
- Sort-column shortcuts ⌘3–6 (Brief) and ⌘F3–F6 (Full) (74e827e5)
- Brief mode: backend-computed per-column widths, plus a max-column-width slider (d84d5c2a, f7907107, f9e40fc4,
  e18bdbf4)
- Volume picker wraps cursor at top and bottom (206ec7d9)
- USB link-speed indicator in the volume switcher (637b152e)
- Stream MTP source-scan progress in the copy dialog (no more 0/0/0 freeze) (fef1aafd)
- Bulk-skip pre-known conflicts under Skip-all for copy and move (b365076d)
- MTP→SMB copy: kill the 2-min stall, faster source scan (1ae5c198)
- Honest copy ETA on long single-file streams: stop decaying files_rate (4737acbc)
- Format sub-1 files/s readouts instead of rounding to 0 (ff7a72f9)
- Strip em-dashes from user copy and docs; rephrase microcopy to sound more human (971e35c4, c39ecdc7, a16afb0c,
  adab08fa)

### Fixed

- Fix MTP delete freezing instead of showing live scan progress (4e005f95)
- Fix Cancel-copy losing the rollback on the APFS clonefile fast path (9c2e6244)
- SMB upgrade no longer races mDNS in dev (be1350d7)
- "Connect directly" SMB login dialog now shows the actual server name (0d84e4e7)
- Bulk-skip no longer pollutes the throughput estimator, and only fires for top-level file conflicts (55d3ca46,
  c3be95c1)
- Per-iter Skip on volume copy credits byte progress (e7f657df)
- Show duration settings in their declared unit (66571349)
- Brief column-width slider enables inside the Settings window (591e090b)
- Brief mode horizontal scrollbar drag no longer vibrates at 60 Hz (b80789e1)
- Restore focus when a ModalDialog closes and when the command palette closes (35413fa3, 6c45e12d)
- File viewer surfaces `SearchStatus::Cancelled` to the FE on cancel (14ba2735)
- Separate MCP ports for prod (19224) and dev (19225) so dev no longer collides with the installed app (f0524658)
- `setSetting` is idempotent on unchanged values (c49636d8)
- Pane state: clear network host on leaving the network volume; skip FilePane MCP sync on Network (602fcb94, a1d19947)

### Non-app

- Refactor write ops behind a shared transfer driver: per-source loop for copy/move, sink-based inner functions across
  local and volume code paths, drop one unsafe transmute (b6833e26, 1d9f2ca4, 63b6728e, 0218a645, 01c8614e, 101e8385,
  118ac6b1, bc957471, 9d7c69e8, a056eb58, 5cf1173a, 1280056b, 0a7c257c, 643e7cb2, afb70901)
- Parallel-shard the E2E suite across three Tauri instances (MTP isolated, two non-MTP shards balanced); wall-clock 5m
  49s → 2m 48s (7802fca3, 1841e0c5, 6e8971a0)
- Cut Playwright wall-clock 10m 12s → 5m 6s via condition polling, MCP-driven cursor moves, beforeEach short-circuits,
  and a per-keystroke → menu-dispatch migration (507afb0e, 3b04806e, f907adc2, df89b217)
- Add proptest-based property tests for `platform_case_compare`, search scope parsing, `glob_to_regex`, and
  `topological_sort_bottom_up` (2e747bf8, ffd799c8, 1813e3dc, 2cf586d1, e69e45aa)
- Add state-transition tests for `IndexPhase`, `ActivityPhase`, `DiscoveryState`, and `SearchStatus` (c0aed651,
  9a9899e9, 9dd32504, 4ae15120)
- Add vitest mockIPC harness plus IPC contract tests for SMB connection, file viewer, and write operations (04c26e4d,
  3a538b44, baa977ed, 967d93be)
- Add mutation-testing-driven unit tests across `indexing/store`, `chunked_copy`, `watcher`, `copy_strategy`, and
  `state` (ef91cfb8, a812cd9a, e9a3a9fd, b026f43d, 4f04d03c, 41a3a831)
- Codify the testing playbook and tools inventory (9515adde)
- Add ESLint rule `cmdr/no-arbitrary-sleep-in-e2e` (a9aea301)
- File-length check: 10% growth buffer with growth % shown; split long files into focused modules (1c1bdeb0, 2d7c27a3)
- Pre-commit `--fast` lane in the check runner (33f77ca5)
- E2E windows get a blue title stripe and `E2E -` prefix so they can't be mistaken for the installed app (b1f707b7)

## [0.18.0] - 2026-05-12

First launch stopped stacking permission popups, copy and delete dialogs show real scan progress, and cloud AI grew to
cover many more providers. Dates and sizes are now color-coded across the app.

### Added

- Suppress the 5–10 macOS permission popups that stacked behind the Full Disk Access prompt, and deep-link straight to
  the right System Settings pane (3c708d35, 16918218, f32dfc55, 791edff0)
- Flag TCC-restricted folders live in the sidebar and file list: italic + (i) icon, `<no perms>` Size, generic folder
  icon for FDA-gated favorites, failed listings stay in nav history (7baa9317, 6581f5ad, df6cd794, 762d7b9a)
- Defer the AI offer toast until onboarding ends so it stops piling on the FDA prompt (265c72d9)
- Color modified dates by age with per-segment tiers (year, month, day, time each get their own color); App palette is
  now the default (c73fcf54, d98459b6, be2333c2)
- Color sizes at every previously-plain site (tooltips, breadcrumb, transfer/delete dialogs, viewer footer, AI progress,
  search results); light-mode palette retuned to clear WCAG AA against every background (265c5a0e, 31128012)
- Show real scan progress in copy/delete dialogs with running tallies, throughput, current directory, and a real
  progress bar; hardlinks deduped by inode so totals match the indexer (03215d25)
- Honest ETA when files outnumber bytes: tracks both axes, picks the slower; no more "~0 s remaining" while the
  small-file tail drains (16b49a04)
- Stream folder-name suggestions in the New folder dialog: first option in under 500 ms instead of after the full reply
  (d681c8de)
- Add multi-provider AI via the `genai` crate (GPT-5, o-series, Anthropic, Gemini, xAI, Groq, DeepSeek, OpenRouter,
  Ollama); fixes GPT-5 400 on `temperature` and `*-pro`/`*-codex` 404 on chat completions (0c45a469)
- Cap updater hangs at 30 s and surface the real cause (DNS error, TCP deadline) instead of generic "error sending
  request" (e5be1467)
- Per-row crash email with build mode and short ID, schema migrations, newest-first sort (e89a63a3)
- One stable short ID per error report, shown the same in the dialog, the toast, and on David's side (77260827,
  e1810361)
- Guard read-only volumes up front for F7/F8/F2 so MTP read-only SD cards warn before you type anything (d9212b83)
- Friendlier write errors that name the provider (like "Managed by **MacDroid**…") and offer Retry only when it helps
  (e9452032, 51dff4c1, 5bcacfef)
- Make Stop/Skip/Overwrite/Rename work for folder conflicts on cross-volume copies too (7ecf9d37, 2f4e377d)
- Fix merging into an existing SMB folder after a partial copy (smb2 0.8.0) (7dd9cfc8, 623f8c17)
- Move MCP defaults to ports 19224 (prod) and 19225 (dev) so a dev build no longer collides with the installed app
  (c9fad17e)
- Polish getcmdr.com hero: "Download for macOS" button, viewport-responsive illustration mask, muted link style,
  tightened copy (606c724e)

### Fixed

- Fix F8 and other dialogs dying after a volume switch (f2019aff, 46bd6d0e, eef042d3)
- Fix the Modified column ellipsizing on some rows under non-100% text size (a7a7915e)
- Fix light/dark theme briefly flipping at startup when the persisted choice differed from the system preference
  (f689da01)
- Stop the dev runtime silently overwriting committed `bindings.ts` on every `pnpm dev` launch (6e39d68d)
- Silence the `get_file_at` FE/BE drift warning that fired legitimately during async listing refreshes (0b51a331)
- Accept `null` for optional crash-report fields so reports written by older app versions still upload after upgrade
  (3c12ff2f)
- Fix dropped keystrokes during fast multi-select sequences (6074cd21)

### Non-app

- Migrate the full IPC surface to typed bindings via tauri-specta; an ESLint rule and a Go check block raw `invoke()`
  and lockfile drift (f1e58011, dc5f0b47)
- Ban classifying errors by string-matching `message`/`stderr`/`title` with a Go check and ESLint rule; sweep across
  SMB, git, friendly errors, and updater (c764962a)
- Pin pnpm 11.0.9 in `mise.toml` and move overrides to `pnpm-workspace.yaml`; unblocks CI's E2E-Linux (cee0aa08,
  c41d2e0d)
- Track recurring upkeep in `docs/maintenance.md` with a log going back to 2025-12-25 (49a119bd)

## [0.17.0] - 2026-05-06

Dynamic text size lands, along with "Open with", system Services, and Finder-matching drag and drop.

### Added

- Add dynamic text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts) (a326bca6, ca78382d, e207effb)
- Add "Open with" and system Services to menus (71e6061b)
- Add iCloud Drive cloud actions to context menu (01bc0dae)
- Split Brief/Full menu items to per-pane View > Left/Right submenus (7f4d123d)
- Add networking toggle, lazy mDNS, no more local-network prompt at launch (d2ae5170)
- Faster external drive detection, fixes USB-C dock invisibility (6527d850)
- Drag & drop matches Finder (same-volume Move, cross-volume Copy, modifier overrides) (64db140f)
- Drag & drop "+" badge tracks the actual op, no flicker (cf8e3818, dcfe439e)
- Drag files into terminals (Warp etc.) (97d10675)
- Add Trash/Delete toggle to delete dialog (778296dd)
- Always show Copy/Move toggle in transfer dialog (450363e6)
- Default to Full mode on fresh installs (57ba47c1)
- File list typography polish: aligned dates, aligned headers, fade selection, clamped Ext (474f7414, e9aec7bd,
  88f56367, c5698998)
- Add size-color palette setting (Rainbow / Accent / None) (5fe0d77e)
- Restore double-click-to-zoom on macOS title bar (f95441dc)
- Focus search when Settings opens (cb88685d)
- Hand cursor on License dialog support and Buy links (554b3801)
- Show real .git/\* files alongside virtual categories in git portal (33219321)
- Per-file Modified dates inside git portal snapshots (3cead878)
- Cache git status per index change, near-instant repeat navs (19f0e98e)
- Error-report preview now lands under 200 ms on big log dirs (was 30+ s) (f24f255c)
- Send error reports in dev too, tagged \[DEV\] (63ebabf6)
- Persistent "Save bundle to disk" toast with Reveal in Finder (0debff1c)
- getcmdr.com comments follow live theme changes (7333b13c)

### Fixed

- Fix Intel DMG download 404 (19f797da)
- Fix crash on virtual git portal toggle; empty git roots no longer render as 1970-01-01 (b266737e)
- Fix folder size column losing value after rename (b1d032c1, d7e08e16)

### Non-app

- Big dead-code cleanup, 355 lines across 22 files (a6b46131)
- Bump GitHub Actions to Node 24 (2f02fa7e)
- Replace claude-md-staleness with claude-md-reminder (fires in-loop, not weeks later) (60e30be5)
- Big CHANGELOG cleanup: shorten long items and document style guidelines. (8f3daa0a)

## [0.16.0] - 2026-05-01

Network shares now reconnect on their own, and you can check for updates from inside the app.

### Added

- Add SMB live reconnect, 5-attempt backoff right in the pane, no re-auth (d96bc4b4, 0c1d3680)
- Disconnect button now actually unmounts (toast if Finder's holding the volume) (c5a410aa)
- Add Check for updates from inside app (00470b96)
- Add human-friendly size units toggle (c8cc1008)
- Add symlink-aware size hint, info icon explains exclusion (matches du and Finder) (0d83a7b2)
- AI download toast X stays closed for the rest of the download (97f1cee3)
- Skip rename warning for equivalent extensions (jpg/jpeg, htm/html, yml/yaml, tif/tiff, etc.) (55592ba4)

### Fixed

- Fix temp network issues kicking users out of folders (48ac9bf8)
- Suppress "Restart to update" toast during first-launch onboarding (ffeb7d96)
- Fix indexer triggering macOS perm popups while onboarding: now waits for FDA (59aca717)
- Fix SMB reconnect runaway subscribe loop after hot reload (91bc2e46)
- Fix SMB reconnect double-triggering loadDirectory (3f6b1b0d)

## [0.15.0] - 2026-04-29

The git browser lands: browse branches, commits, stashes, and worktrees like folders, and copy a file out of any
version.

### Added

- Add git browser: live branch/dirty pill in breadcrumb, browse `.git/branches/`, `tags/`, `commits/`, `stash/`,
  `worktrees/`, `submodules/` as folders, drag any file out of any branch or commit into working tree (preserves bytes
  and exec bit, no `git checkout`), optional per-file status column with M/A/D/?/! glyphs (314e9ae2, 897df2c7, 1ebcfa1c)
- Meaningful Modified and Size columns in git portal (`+12 / -3` for branches, `5 files` for commits, `on main` for
  stashes, short SHAs for tags) (31aec35c)
- Add friendly errors for git browser (19d5b075, af64689f)
- Add Git toggles in Settings (repo chip, status column, virtual portal) (19d5b075, af64689f)

### Fixed

- Fix virtual `.git/<category>/...` paths kicking pane back to parent (bfcbfa48)

## [0.14.0] - 2026-04-26

### Added

- Add error reports: one-click redacted diagnostic bundle via Help menu or error toast, with optional auto-send and a
  short ERR-XXXXX correlation ID (6d904aa6, 51b6102a)
- Add log storage cap setting (default 200 MB, 0 disables log storage and error reports) (f3dbf514)
- Add per-output log filtering, with a verbose-logging toggle in Settings (319d5d37)

### Fixed

- Fix auto-sent error reports dropping when fired before the Tauri handle exists (f069a712)
- Align Size column icons flush right (1d5f661a)

### Non-app

- Add error-report endpoint on api server with R2 presigned-URL handoff (1a2ea1c0, f78f76af)
- Add shared PII redactor for crash files and error-report bundles (1d719f36, b64e2c2c)

## [0.13.0] - 2026-04-22

### Added

- SMB copies ~30× faster on high-latency links (100×10 KB over ~60 ms RTT: ~28 s to ~1 s) (94090555, 9d6df0e9, 4009b9ba,
  77ea6e81)
- Add SMB concurrency setting (default 10, range 1–32, live) (7fdd85e3, aa331c4e, f46d45e4)
- `..` row shows current folder's totals, not parent's (36212ede)
- Full mode shrink-wraps Ext/Size/Modified to give Name every spare pixel (7325c8f8)
- Brief mode shrink-wraps each column to its widest filename (c336dbba)
- Filename tooltip on truncation in Brief and Full (f37d7e51)
- Volume tooltip on tabs (b6663988)

### Fixed

- Security: bump smb2 to 0.7.2, fixes a crafted DFS referral crashing Cmdr (7e7eaf76)
- Fix small SMB uploads ignoring cancel (f948731c)
- Fix click-on-cursor eating the next drag (cccf0095)
- ⌘C now copies selected text when there's a text selection (47f03b20)
- Block dropping a folder onto itself or its descendants ("Can't drop here" feedback); `..` accepts drops (b7c3d960)
- Fix frontend hot reload (swap UnoCSS for unplugin-icons) (00906566)

### Changed

- Internal: cross-volume copies flow through stream API (plus APFS clonefile fast path); batch copies run in parallel
  per-backend (eb99c37c, 508a0fe1, 50b7221e, 39c71eed)
- Move smb2 from git to crates.io, bump through 0.7.1 and 0.7.2 (96f4bbd3, 0ec95a79, 7e7eaf76)

### Non-app

- Run Docker SMB integration tests on every push (26 tests against real servers) (257269bb)
- Byte-level blake3 hash verification on every SMB copy test (fd5a2d84)
- SMB copy soak harness: 30-min Docker run, 41,984 iterations, zero drift (3a9b58f2, 6a9e046d)
- Add changelog-commit-links check (surfaced and fixed 8 bad links) (4e281303)

## [0.12.0] - 2026-04-18

### Added

- Add friendly error pane for listing failures (provider-aware suggestions for Dropbox, Drive, OneDrive, iCloud,
  MacDroid, VeraCrypt, etc.) (eec50ff5, cc7bb319)
- Live disk-space updates in status bar (configurable threshold, 3 s timeout) (d67dd382)
- Add "Copy path" to breadcrumb context menu (eb4d3c92)
- Add SMB streaming reads/writes (MTP↔SMB and SMB↔SMB copies skip temp files, ~1 MiB peak RAM) (ac71bd0a, a8270909,
  35120da0, 043597f8)
- Disambiguate same-named SMB shares per server (76671bf5)
- Inline SMB login form on direct-connection upgrade (b315b421)
- Instant dialog open for large selections (50k-file Copy/Move: ~10 s to ~1 ms) (48ea6030)
- Add MTP Samsung support (phones reporting 0 storages at connect time now appear) (14b3ac3f)
- Batch MTP scan for copy (one USB call per parent dir, not per file) (70978c84)
- Skip rename extension warning on case-only changes (photo.JPG to photo.jpg) (1401017d)
- Split filename + extension in Full view (275d0918)
- Volume selector polish (clickable spacebar area, no clipping over F-key bar) (700eac4a)
- File-op dialog polish (thousand separators, mid-text truncation, fixed 500 px width) (d67dd382)
- Add debug-window error-pane preview with all 47 error states (cc7bb319)

### Fixed

- User cancels no longer log as ERROR (6f793929)
- Fix copy/move crash from a reactivity race (0cdd7d7e)
- Fix stuck "Scanning 0 files" transfer dialog (dd06d680)
- Fix double-dispatched MCP autoConfirm copies (4af22ab5)
- Fix file watcher panic on 500+ external changes (4087e30e)
- Match Finder for copy space checks (count APFS purgeable space) (34546567)
- Fix SMB paths with spaces, serialize concurrent manual-server writes, fix viewer search after emoji/CJK (97c04818)
- Fix SMB port handling and human host display for mDNS names (c26f7e87, 017b7043)
- Fix "Connect directly" on QNAP (2666db8a)
- Hide Clear-index button when there's no index (fixes AA contrast) (b1915d9b)
- Network pane no longer sticks on old host after mount (41c18609)
- Fix llama-server startup on Linux with locked keyring (encrypted-file fallback) (55ccde30)
- Fix nested-runtime panics on MTP/SMB (async Volume trait end-to-end, runtime-safe MTP reads) (531bb9b9, 9d4982a8,
  694ddc12, 1598f8cf)

### Changed

- Cancelled SMB uploads skip the server flush (~100 ms to 1 s saved per cancel) (6fa07801)

### Non-app

- Add design-time WCAG contrast checker (resolves CSS vars and color-mix chains, replaces flaky axe rule) (db25f0d3,
  55af2581)
- Fix 18 real WCAG AA contrast failures (747507f1, 67d42ba3, 4a15a53d)
- Add tier-3 component-level a11y tests (61 files, 146 tests, ~6.3 s) and a11y-coverage check (33300a4f, d56c1dfe,
  398bf7a5)
- Switch Lucide to UnoCSS pure-CSS icons (93548fa6)
- Add file-length check; split 20+ long files into sub-800-line modules (7514cb4e, 2939bfee, 4514a832, 315609a3)
- Run Linux E2E in Docker (8803c3c6, f39177c2)
- Drop CrabNebula/WebDriverIO macOS E2E suite (Playwright covers all 15) (4cecfb91)
- Upgrade rustls-webpki 0.103.12 (RUSTSEC-2026-0098/0099) and bitstream-io 4.10.0 (3734502a)
- Add docs/error-handling.md contributor guide (a4a5fdb5)

## [0.11.1] - 2026-04-10

### Added

- Add striped-rows setting (alternating row shading in Full and Brief) (faa25349)
- Add MTP per-file copy progress and instant mid-file cancel (~300 ms via USB SIC abort) (ac5ec4df, a66adf67)

### Fixed

- Sync View menu Full/Brief checkmarks across panes (6e36a49b)
- Stop MTP `ObjectNotFound` log spam on every copy (0cc675a6)
- Fix MTP mid-stream cancel corrupting USB session (mtp-rs 0.11.0) (a66adf67)
- A11y: darken accent-text for WCAG AA, fix search placeholder opacity (b7744dd9)
- Fix Linux compilation (cross-platform SMB types, get_smb_mount_info) (00c5f184)

## [0.11.0] - 2026-04-10

### Added

- Add SMB direct connections via smb2 (~4× faster, OS mount stays for Finder/Terminal) (dea46ecc)
- Auto-upgrade existing and new SMB mounts to direct connections in the background (a6ab2ca1)
- Add "Connect to server" for SMB by hostname, IP, or `smb://` URL (persisted, context-menu Disconnect/Forget)
  (2df24aca)
- Add SMB connection status indicators with one-click upgrade (04732500)
- Real-time SMB transfer progress with end-to-end cancel (f5303551)
- All SMB write ops (create, delete, rename, copy, move) through direct connections with full conflict handling
  (e72c0828, 4f030d7f)
- Unified SMB/MTP change notifications with incremental cache patches (2d0bc986)
- Warn in transfer dialog when using slower OS mount (d25de484)
- Auto-suppress ptpcamerad on macOS for MTP (d161f9b1)
- Add MTP settings (disable toggle, "Don't show again" toast, dedicated section) (2467ecef, 70d8d40a)
- Brief mode shows real recursive directory sizes in selection info (53ee5efb)
- Cursor jumps to newly created directories (eff84d17)

### Fixed

- Fix per-file copy progress (counts files, not top-level items) (d10d9cc0)
- Faster SMB deletes (skip stat round-trip) (0e7f0727)
- Copy cancellation checks between every file in tree copies (a7d401ac)
- Fix cross-volume copy misclassifying SmbVolume as local (4a86a85c)
- Fix SMB paths with accented characters (NFC normalization) (baaccc84)
- Resolve SMB IPs to hostnames via mDNS so Keychain finds saved credentials (b1addfd2)
- Show login form on stale Keychain credentials instead of empty share list (46609f16)
- Block navigating above SMB mount root, fall back to home when unreachable (d25de484)
- Fix stale cursor index after file ops (945093bc)
- Fix drag & drop after wry upgrade (a816c77c)
- Fix stale dir sizes after copy/create (1479108e)
- Fix scan-preview race in progress dialog (5d9b91bd)
- Fix dir_stats count drift on file/dir type changes (364ddf15)
- Fix index entry ID race via shared atomic counter (6e173e45)
- Fix MTP move not refreshing UI on Linux (mtp-rs 0.9.1) (5b27ead1)

### Non-app

- Replace smb/smb-rpc crates with our own smb2 (2d7904f0)

## [0.10.0] - 2026-04-08

### Added

- Visible copy rollback (progress bars count back, Cancel stops the rollback) (0ac5d05f)
- Dual progress bars in transfer dialogs (size + file count) (ced9d253)
- MCP: cmdr://settings resource and set_setting tool (c7111582)
- MCP: move_cursor awaits frontend confirmation (6341c255)

### Fixed

- Fix MTP move conflicts silently overwriting (27f2ff0b)
- Fix MTP watcher missing external file changes (266026d5)
- Fix MTP event debouncer dropping suppressed events (21b3bc5f)
- Fix MTP pane falling back to local root after copy (9deba727)
- Fix MTP volumes missing from copy/move dialog (cd66031e)
- Fix MTP event-loop lock contention blocking copy/move/scan (0461e33a, 547a4131)
- Fix MTP scan preview showing 0/0/0 in confirmation dialog (4e1efab7)
- Fix MTP rename conflicts not showing dialog on non-local volumes (25f2b263)
- Fix copy "Cancel" (keep partial files) triggering unintended rollback (3042f234)
- Fix copy cancel hanging 30+ s on network mounts (816e9e12)
- Fix UI blocking on network filesystem ops (bed59dbe)
- Fix indexing replay progress showing "Scanning..." instead of replay overlay (32c05393)
- Push-based volume selector, fixes mount/unmount races (b0966592)
- Fix volume path resolution to <1 ms regardless of mount health, handle APFS firmlinks (5a1f78cb)
- Harden unsafe Rust (main-thread markers, scoped Send impls, SAFETY comments) (541804c3)

### Changed

- Typed write-op errors (9 variants) replace string parsing (c10e0614)
- Typed MTP volume errors (8f2296a4)

### Non-app

- Backend owns MTP move strategy, frontend no longer orchestrates (547a4131)
- Demote noisy per-file copy/move/MTP logs from INFO to DEBUG (357feff0)
- Fix all WCAG violations found by axe-core (d29a7cdd, 4380469e, 6e623083)
- Port E2E tests from WebDriverIO to Playwright; add 80+ tests (MTP, SMB, conflicts, a11y, indexing) (77d05937,
  7d58bd6c, 4f83aeb8)
- Replace Prettier with oxfmt (10–20× faster) (995f8c8e)
- Split indexing module (1951 lines) into focused files (39086418)
- Add light/dark website theme, features page, OG images, blog Like buttons (49dbe782, 98bdcc35, 56a9e764, 5cff7c35)
- Dashboard: color-coded charts, GitHub star tracking, error reporting (4b7c9e1e, 67efc4ae, 2e26b956)

## [0.9.1] - 2026-03-24

### Fixed

- Fix orphaned llama-server processes after rapid AI provider switching (b3382efe)
- Fix vendor-specific MTP detection (Kindle, USB class 0xFF) via mtp-rs 0.4.1 (1a170dbd)

### Non-app

- API server: migrate telemetry to D1, add crash email notifications via Resend, rename license-server to api-server
  (7dc0da23)
- Split search.rs (2361 lines) and SearchDialog.svelte (1552 lines) into focused modules (c17c210c)
- Deduplicate repeated patterns across Rust, Svelte, TS, and Go (52afe37a)
- Bump 9 Rust deps (reqwest 0.13, rusqlite 0.39, notify-debouncer-full 0.7, etc.) (929556f2)
- Skip pnpm install when lockfile unchanged (~20 s saved per run) (8d2b39b8)
- Blog: add Kindle support article (5c9d5b16)

## [0.9.0] - 2026-03-23

### Added

- Add whole-drive file search (⌘F): glob/regex, size/date filters, scope, AI mode, MCP search and ai_search tools
  (05813639, 15110c0d, 8c3546dc, cf5827b1, 415db3f1, 21d32ef1, 26d682cd)
- Add opt-in crash reporting (panic hook + signal handler, inspect-and-send dialog, no PII) (016ee3a5, be29affc)
- Add Shift+F4 (Total Commander style): create new file, open in default editor (da8ca934)
- Add smart size display (min logical/physical, dual-size tooltips, hardlink dedup, mismatch icons) (1d666a70, b302d0eb,
  06582001, 1d588f84, a93a8bb2, 9c450cdc)
- Add sortable Ext column in Full mode (e834b4cb)
- Add replay progress overlay during cold-start (f166b063)
- Show live MTP disk space in volume dropdown and status bar (b155f1f8, c4cc26f2)
- Show MTP loading progress on large folders (77ebaa00)
- Add focus indicators on search and command palette inputs (1792216b)
- Selection summary includes directory sizes (3928c1c9)
- MCP: show directory sizes in state resource (9cb77509)

### Fixed

- Fix multi-GB macOS memory leak (ObjC calls on background threads now run inside autoreleasepool) (777f9ec3)
- Fix stack overflow in sync status (8 MB OS threads instead of rayon for NSURL/XPC calls) (fa28cd43)
- Fix size overcounting (hardlink dedup, exclude cloud-only files, smart-size for dataless) (fe5eff72)
- Fix file watcher: instant updates in large dirs via incremental diffs (df558e8b)
- Fix selection clearing after file ops; gradual deselection per source item (538ec5ac)
- Fix selection indices drifting after external file changes (453ec02b)
- Fix cursor lost after deleting all files (17808d4b)
- Fix stale dir sizes on rename (10213d84)
- Fix indexing not starting on fresh DB (a61376d6)
- Fix "Scanning..." stuck after replay (4a44d7df, fb796e72)
- Fix verifier + replay transaction conflict via named savepoints (72ca9fbb)
- Fix MTP browsing panic; show device name on single-storage devices (d37b8a5f)
- Fix MTP duplicate directory listing on connect (17efe8be)
- Fix MCP stale state after server crash; auto-probe port when configured port is in use (0369d219, d69f8761)
- Fix OpenAI compatibility (795a6775)
- Hide misleading rollback button for move ops (fbdba5b4)
- Raise replay/journal gap thresholds to reduce unnecessary full rescans (37791986, af2bf7a7)

### Non-app

- Add full-stack analytics dashboard (6 data sources, agent-readable report) (b4f740a1, 0766c4b7, b97028f6)
- Enforce CSS design tokens via Stylelint (50f2b422, e3259b0a, 36b3408c)
- Drop desktop smoke tests, speed up store tests by ~20 s (c6210ae4, dab071f5)
- Reduce code duplication across write ops, listing, events, search dialog (33ec2f27)
- Website: story + testimonials sections, landing page polish, Docker healthcheck, Remark42 CSP (d5a7f430, 51acd88c,
  424a8075, dd5e3403)
- Bump mtp-rs to 0.2.0 (63425523)

## [0.8.2] - 2026-03-15

### Fixed

- Fix crash on launch after auto-update (kernel code-signing cache SIGKILL: temp + rename for a fresh inode) (d2923aff)
- Fix indexing drift: per-navigation verifier with 30 s debounce; skip /System and /dev as empty stubs (0f28b51e,
  b0b17305)
- Fix dir size display during indexing (refresh on aggregation-complete, not scan-complete) (d0746fbb)
- Fix navigation latency: fire-and-forget verification, parallelize 6 listen() calls (a4e87f1a)
- Fix indexing perf (integer-only index: 25 min to seconds on 5.1M entries; 99% replay-event dedup) (a5b5beb7, 44fecd66,
  d9877c14)

### Non-app

- Separate dev and prod log dirs, fix Linux test output capture, fix smoke test timeout (e8762be4, 83d23655, 88901f91)
- Improve agent instructions (dec19cf4)

## [0.8.1] - 2026-03-14

### Fixed

- Fix indexing (lock-free dir-stats reads, drop stale PathResolver cache, fix "DB is locked", fix overlay race, lost
  scan metadata, dir→file replacement orphans) (50bd4faa, 44abfd10, 7319c5c4, 26785fcd, 795e48b7, 424eedb3, dbccec1b,
  8f87a4f5)
- Fix traffic light position in production builds (7551df2f)

### Non-app

- Add indexing concurrency stress tests, event loop tests, reconciler tests (3ad3adc9, 8a084cda, dbccec1b)

## [0.8.0] - 2026-03-13

### Added

- Add custom macOS updater that preserves Full Disk Access (syncs into existing .app bundle, privilege escalation)
  (190a6377)
- Add MTP delete, rename, move (full progress, cancel, dry-run) (812ad073)
- Add breadcrumb polish ("/" prefix, "~" for home) (44b71056)
- Add auto-rescan on FSEvents channel overflow (ca7cece3)
- Add index debug dashboard (DB stats, watcher status, event-rate sparkline) (7510ec39)

### Fixed

- Fix indexing (interrupt-safe reconciler, stop micro-scans, faster bulk inserts, false FSEvents deletes, missing dir
  sizes after replay, periodic DB vacuum) (31df59e6, 981b3113, da742904, f0c225f4, bf0b47f2, d125a241, 67684bbb)
- Fix drag swizzle failing on wry 0.54+ (2680bae8)
- Fix MCP live start/stop UX (backend state as ground truth, port auto-check) (f4c107aa)
- Fix MCP server not stopping on app quit (61fe290a)
- Fix traffic light position in production builds (b74ed395)
- Fix scan overlay showing stale state (218bcb98)

### Non-app

- Vendor cmdr-fsevent-stream fork as workspace crate (8b937a6b)
- Fix two FOUC flickers on website page load (8c21ac78)
- Set up self-hosted macOS GitHub Actions runner; add index DB query tool, website deploy workflow extracted (665f63a9,
  37f10629, 5744636f)
- Pink title bar in dev to distinguish from prod (d2c9ae41)

## [0.7.1] - 2026-03-12

### Fixed

- Fix scan overlay stuck at 100% after directory size aggregation (424eedb3)

## [0.7.0] - 2026-03-12

### Added

- Add AI settings: three providers (off / cloud / local LLM), 15 cloud presets, per-provider keys, model combobox, RAM
  gauge, context size (b41365b3, abfc2481, 423e669f)
- Live MCP server start/stop in Settings (no app restart) (e0c55e73)
- Add stale index detection with toast + auto-rescan (b590a54e)
- Add device tracking for license abuse, fair-use terms in ToS (cf4f9138)
- Add license section to Settings (status display, action buttons, dynamic labels) (39cf7b4b)
- Improve app icon for macOS Sequoia (cc80d280)

### Changed

- Drop supporter license tier (legacy keys map to Personal) (c0a63f57)
- Split Settings UI horizontally 50/50 (9493f880)
- Rename settings-v2.json to settings.json (d987cc8f)

### Fixed

- Fix startup panic from blocking_lock in async context (f9855ca0)
- Fix SQLite write pragmas on read-only connections (panic in subtree scans) (a53a2753)
- Fix llama-server not stopping on quit, stale PIDs, excess memory (256k to 4k default context) (eae70f10, ffcbc818,
  e45c742a)
- Fix Settings UI freezing ~5 s when stopping AI server (instant SIGKILL for stateless llama-server) (2af7ee82)
- Separate dev/prod data dir and MCP port (b8b058a2)
- Fix fallback path resolution falling to / instead of ~ (8d7c6441)
- Fix indexing (100× faster aggregation, DB auto-vacuum, truncate before full scan) (47a2e8ef, cad1af56, aff2046e,
  96323e97)
- Fix FSEvents storms causing memory pressure (mimalloc, 1 s dedup window) (207ddee1)

### Non-app

- Replace 19 ADRs with colocated Decision/Why entries in 11 CLAUDE.md files; slim AGENTS.md from 245 to 93 lines
  (ccf5cc7f, d297a1a8, 05957961)
- Website: version + file size on download buttons, fix Intel/Apple detection flicker (bd170563, ec35b1f7)
- Add html-validate and circular-dep checks (3dbd5af5, 4bead2b9)
- Eliminate all circular deps via refactor (volume grouping, menu platform code, viewer scroll/search) (7740fbc4,
  8522e71f, e16bd918, 7ed1cea1)

## [0.6.1] - 2026-03-10

### Added

- Add top menu icons (1a2621af)
- Add View, Copy, Move, New folder, and Delete actions to context menu (a966f174)

### Fixed

- Fix OOM crash from unbounded indexing buffers; toggling Full Disk Access could replay millions of FSEvents with zero
  backpressure, consuming 500+ GB RAM. All buffers are now bounded (~350 MB peak), with a memory watchdog that stops
  indexing at 16 GB (f1501ece)

### Non-app

- Website: add llms.txt, Schema.org JSON-LD, and auto-generated sitemap for agent accessibility (ba64c362)
- Website: update roadmap (51971200)
- CI: simplify release pipeline, download sigs directly from release, generate `latest.json` with `jq`, validate all 3
  sigs before proceeding (d3095cbc, 5b82cd01)
- CI: fix Backspace E2E test on WebKitGTK, fix CI failures, fix 3 flaky tests (7c22951a, 79f593cb, 8f4ea825)
- Docs: add troubleshooting section to releasing guide (1768b29a)

## [0.6.0] - 2026-03-08

### Added

- Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
  inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native file
  icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no system
  keyring, distro-specific install hints, USB permission handling (b6e80f6b, 20be0c38, 9c51fa9b, 64e41f9d, 40cc1a98,
  c3ad1ed5, d40ea256, 60063ece, e65d993c, 22e2ea79, afe26090, 4bbcbb09, 48af543b)
- Add per-pane tab support: ⌘T/⌘W, ⌃Tab cycling, pin/unpin, context menu, persistence with migration, per-tab sort
  (791a29a9)
- Add delete/trash feature (F8): trash by default, ⇧F8 for permanent delete, confirmation dialog with scan preview,
  batch progress with cancellation, volume trash support detection (e3560a36)
- Add clipboard for files: ⌘C/⌘V/⌘X with Finder interop, ⌥⌘V for "Move here", cut state tracking, text clipboard in all
  windows via NSPasteboard (0dc29536, 60baebad)
- Add toast notification system with centralized store, dedup, stacking, three levels, transient/persistent modes
  (6c5c4525, 2329f2f5)
- Add per-pane disk space display: 2px usage bar, free-space text in status bar, mini bars in volume dropdown (9b6d0579)
- Add custom tooltips with glass material effect, shortcut badges, smart positioning, accessibility support, replacing
  all native tooltips (3c7f9654)
- Add drive indexing with integer-keyed DB schema (7.4x size reduction, 3.8 GB → 0.54 GB), LRU path cache,
  platform-aware collation, recursive CTE aggregation (7c5d3ce1, daee97b0, 5e10fa9f, 68be3abb)
- Add IPC hardening: timeout-protect all filesystem commands, transparent timeout UI with retry/fallback for volumes,
  tabs, file ops, and viewer (6a582788, 71de96e1)
- Add accent color option in Settings: macOS theme or Cmdr gold, "Recolor to gold" for folder icons (330e8245, ef9de79d)
- Add directory sorting by size with toggle in Settings (a7dd8cae)
- Add "Forget saved password" UI for SMB network shares (7d751d53)
- Add path validation in copy/move and mkdir dialogs with platform-correct limits (6b295ec4)
- Add centralized keyboard shortcut dispatch with runtime custom bindings (e40bcc2f)
- Add macOS entitlements and TCC usage descriptions for proper permission prompts (ff0c27ee)
- Add Apple code signing, notarization, and arch-specific downloads (aarch64, x86_64, universal) (b03f91ec, 944085fb)
- Add licensing UI improvements: verify/commit split, typed errors, short code in signed payload, Paddle live setup
  (0abc7049, 1f2308be)

### Fixed

- Fix file viewer: search progress bar with spinner and stop button, incremental match delivery, 10k match cap,
  byte-seek navigation, loading very long files (9c0a3c39, a3b9d0ee, 31cf5fdc, d15ecded, 86ef2a5e, 0fcdb13c, 8b57bbe6)
- Fix 3–10s startup block from index enrichment holding the mutex (267e02b8)
- Fix mDNS host resolution arriving before discovery, causing SMB auth failures (2dda99b6)
- Fix focus escaping panes with focus guard, removing ~50 redundant refocus calls (4c9aadc9)
- Fix clipboard shortcuts in text fields on macOS (20f3de02)
- Fix non-blocking navigation on slow/dead SMB shares with timeouts and optimistic UI (c85c8c26)
- Fix copy feature: auto-rollback on panic, deadlock prevention, cancel race condition (2b17ab55)
- Fix status bar not refreshing after file watcher diffs (e880f9f7)
- Fix pinned tab volume change now opens new tab instead of navigating in-place (ff4c8f2f)
- Fix cancel-loading to return to previous folder instead of home (8ff23798)
- Fix ⌘, to refocus Settings window if already open (71b3e612)
- Fix Settings: ⌥+key shortcuts showing "Dead" on macOS, key filter subset matching, ESC clears filter (1fd540a0,
  5056bb6b, 47050e02)
- Fix settings not initialized warning at startup (b540fcc5)
- Fix SMB share showing 0 bytes free on network filesystems (f791153b)
- Fix volumes cached to prevent timeout at startup (024e48f2)
- Fix top menu items staying enabled on non-main windows (7572d130)
- Fix live file count during large folder loading (7815d0fb)
- Fix window content height for production builds (0cbd0fd2)
- Fix folder icons updating on OS theme change (6b024453)
- Fix focus lost after rename cancellation (edace189)
- Fix file viewer not loading settings (acfef93b)
- Fix drive indexing: orphaned entries, missing dir sizes, background scan failures, DB transaction issues (323ae866,
  004f3026, c331143d)
- Fix MCP protocol version mismatch warnings at startup (2af0b901)
- Fix arrow up/down performance in large folders (e6f268c3)
- Fix PostHog CSP and make it cookieless (1700d999, 9cea85aa)
- Fix app loading slowly due to startup optimizations: license cache, async validation (3835866c, 87de1369)

### Non-app

- Overhaul native menus on macOS and Linux: build from scratch, strip macOS system-injected items, unify dispatch via
  single event, context-aware graying, full accelerator sync (b38c552b)
- Unify frontend + backend logging via tauri-plugin-log, demote noisy log levels, suppress smb/sspi noise (22f4ab5b,
  dbbcc551, 1e59a564)
- Design system: unified button styles, consistent loading states, improved text readability, redesigned network screens
  (8dc2e33c, 4d07ad0b, 71dbe0be, b5d8b280, a018a3ec, 90e20104)
- Docs overhaul: CLAUDE.md staleness checker in CI, enriched 25 CLAUDE.md files with Decision/Why entries, cross-cutting
  patterns in architecture.md, split infrastructure.md into per-service files (ff8b3be2, 347ae9bd, f961f195, 2f7bff1a)
- Website: add blog with first post, PostHog and Umami analytics, arch-specific download buttons, Docker build check,
  newsletter improvements (01681c19, 75d52283, 78de5731, ae8f6cb9, 34ecc703)
- Check runner: CSV stats logging, cfg-gate enclosing block scope detection, file length check, flag combining fix
  (9ac4b54b, 539db62f, 4a245623, 6fe48a96)
- Refactors: split DualPaneExplorer and FilePane, extract dialog state, deduplicate templates and Settings CSS, split
  tauri-commands (337f6207, cfae0db4, dad8790c, 35a42394, ba86d874)
- License server: download tracking via Cloudflare Analytics Engine (ef0f0494)
- Add Renovate for automated dependency updates (00880a0c)
- Add macOS Playwright E2E tests and CrabNebula E2E tests (ec900eec, a768c030)
- Infra: uptime monitoring with UptimeRobot + Pushover, hardened deploy script (19baefd1)
- Add cfg-gate lint check for macOS-only Rust crates (075c1d4a)

## [0.5.0] - 2026-02-15

### Added

- Add file viewer (F3) with three-backend architecture for files of any size, virtual scrolling, search with multibyte
  support, word wrap, horizontal scrolling, and keyboard shortcuts (79268a4c, 9f91bce0, b10002a9, 2ad2521b, b65c422f,
  43adc86c)
- Add drag-and-drop into Cmdr: pane and folder-level targeting, canvas overlay with file names and icons, Alt to switch
  copy/move, smart overlay suppression for large source images (1ad14932, 6207d8e2, a89f18fb, 371746bb, a3eae1cf,
  c776eed9, e97d3db7)
- Add settings window (⌘,) with declarative registry, fuzzy search, persistence, keyboard shortcut customization with
  conflict detection, and cross-window sync (db121f6d, 418f7902, 8f78596c, 218b79b7, 9c39db32, 4e90137d)
- Add MTP (Android device) support: browsing, file operations (copy, delete, rename, new folder), USB hotplug,
  multi-storage, MTP-to-MTP transfers (938e87c4, 672fa6e5, d1e9f802, 7ac1528b, b08af36f, ea845a66, fd8dad66)
- Add move feature (F6) reusing the copy UI as a unified transfer abstraction (682d33a2, cb9e0471)
- Add rename feature with edge-case handling (62799c6a)
- Add swap panes feature with ⌘U shortcut (2a1b3296)
- Add local AI for folder name suggestions in New Folder dialog, optional download (b9a112ed, 3dc19c09)
- Add chunked copy with cancellation and pause support on network drives (ba5409ef)
- Add 6 copy/move safety checks: path canonicalization, writability, disk space, inode identity, name length, special
  file filtering (95480228)
- Add sync status polling so iCloud/Dropbox icons update in real time (ed361582, 62964125)
- Add CSP to Tauri webview for XSS protection (68bd510b)
- Add copy/move folder-into-subfolder warning with clear error message (521ab5e8)

### Fixed

- Fix panes getting stale when current directory or its parents are deleted (1b5ad52a)
- Fix multi-window race conditions that could crash the app (9a33e24b)
- Fix recovering from poisoned mutexes instead of crashing (56 lock sites) (62fd6852)
- Fix wrong cursor position after show/hide hidden files (223b041e)
- Fix selection and cursor position breaking on sort change (36d61d08)
- Fix panel unresponsive after Brief/Full view change (2b6d5131)
- Fix copy operationId capture race condition (9b5c57c1)
- Fix $effect listener cleanup race in FilePane (e2c6ee12)
- Fix condvar hang on unresolved conflict dialog (2975c450)
- Fix first click on main window not changing file focus (59c5da48)
- Fix AppleScript injection in get_info command (e3378c35)
- Fix URL-encoding of SMB username in smbutil URLs (f908a74b)
- Fix mouse/keyboard interaction bug for volume picker (8afd0de6)
- Fix drop coordinates when DevTools is docked (a9a041f1)
- Fix MCP server always returning left pane as selected (2f9160a5)
- Redact PII from production log statements (fe31316f)

### Non-app

- Migrate network discovery from NSNetServiceBrowser to mdns-sd: 68% code reduction, no unsafe code (3d44cf17)
- Rewrite MCP server with fewer tools but more capabilities, auto-reconnect, and instructions field (1061fad7, ede6463a,
  82345d18)
- Introduce ModalDialog component for all soft modals with drag support (ffbf14a7)
- Major refactors: split DualPaneExplorer, FilePane, volume_copy, listing/operations, connection modules (04dc3deb,
  e14c2893, 2da8e6dd, c0bd500b, 707a96a9)
- Security: pin GitHub Actions to commit SHAs, fix Paddle webhook timing attack, use crypto.getRandomValues for license
  codes, HTML-escape license emails, add webhook idempotency, constant-time admin auth (c0d8cc31, 70bc5948, 51cd0b57,
  bea3b2a9, 9db450b7, b82f857a)
- Docs overhaul: add colocated CLAUDE.md files throughout repo, architecture.md, branding guide (eac9e618, dd91c788)
- Website: add changelog, roadmap, newsletter signup with Listmonk + AWS SES, mobile responsiveness fixes, 512px logo
  (643de6ad, 07936d1d, ba4812d5, aa661cff)
- Add dead code check, manual CI trigger, pnpm security audit, LoC counter, summary job for branch protection (9876600d,
  3b20e660, ad22eba9)
- Tooling: extract shared Go check helpers, add VNC mode for Linux testing, fix Linux E2E environment (550c3536,
  6aa5ff7c, fa907b6b)
- License server: add input validation, webhook idempotency, and security hardening (4363a320, 9db450b7, 7398965b)

## [0.4.0] - 2026-01-27

### Added

- Add file selection: Space toggles, Shift+arrows for range, Cmd+A for select all, selection info in status bar
  (4d44cda0, 1cac4b31)
- Add copy feature with F5: copy dialog, destination picker with free space display, conflict handling (281f45ee,
  fb5f0275, a6d148d8, 6c661f29)
- Add new folder feature with F7 shortcut and conflict handling (80ec297d)
- Add "Open in editor" feature with F4 shortcut (7eb66aca)
- Add function key bar at bottom of UI for mouse-initiated actions (537e0405)
- Add pane resizing: drag to resize between 25–75%, double-click to reset to 50% (542b4910)
- Add multifile external drag and drop (74263344)
- Add keyboard navigation to network panes: PgUp/PgDn, Home/End, arrow keys (70aa3410)
- Add "Opening folder..." loading phase for network folders with distinct status messages (9eb1185e)
- Add license key entry dialog with organization address and tax ID collection (52480cef, 29eb6fe1)

### Fixed

- Fix UI not updating on external file renames (5de93465)
- Fix light mode colors (42888c70)
- Fix cursor going out of Full view bounds (7edcac89)
- Fix ESC during loading navigating to wrong location (b8c12e78)
- Fix focus after dragging window (8488de6a)
- Fix multiple volume selectors opening at once (f4c4c214)
- Fix frontend race condition from refactor (646c7af3)

### Non-app

- Add E2E tests with tauri-driver on Linux using WebDriverIO in Docker (1b0cbac5)
- Revamp checker script: parallel execution, dependency graph, aligned output, colored durations (7835b4cb)
- Add type drift detection between Rust and Svelte types (b3ae1c3f)
- Add jscpd for Rust code duplication detection, CSS health checks, Go checks (67e6c15a, d177eb36, 25407523)
- Add Claude hooks for pre-session context and post-edit autoformat (3d59ddea, 122182d6)
- Add LogTape logging for Svelte and debug pane for dev mode (affa5482, f494e15f)
- Require reasoning in clippy lint exceptions (d327cf49)
- Website: fix hero image animation and sizing, fix broken Paddle references (40faeeef, 278ad4c8, 5eb5a523)
- License server: wire up Paddle checkout, fix webhook email fetching, support quantity > 1 (3c40929c)

## [0.3.2] - 2026-01-14

### Fixed

- Fix auto-updater to download updates and restart the app after updating (c0bff9a6)

### Non-app

- Website: redesign with mustard yellow theme, view transitions, hero animation, and reduced motion support (0296379a,
  18b729fd, 689a1513)
- Website: avoid aggressive caching, rearrange T&C (8ca05395, c92dff8c)
- Tooling: turn off MCP stdio sidecar, fix Rust-Linux check, reduce CI frequency, fix latest.json formatting (5dda608a,
  2ec3f7e1, 42d81ab9, 52980aec)
- Docs: release process and auto-updater documentation (c7c36f60, 765f5ad0, f3785da7, 10e43de7)

## [0.3.1] - 2026-01-14

### Added

- Add custom title bar, 4 px narrower for more content space (33e90c8b)

### Changed

- Replace rusty icon with yellow one (79777e34)

### Fixed

- Fix app name in task switcher: shows "Cmdr" instead of "cmdr" (8117300e)

## [0.3.0] - 2026-01-13

### Added

- Add MCP server with file exploring tools (f6dcf273)
- Add stdio MCP interface for broader client compatibility (3b193f7c)
- Add Streamable HTTP support to MCP server (1d0549b0)
- Stream folder contents for blazing fast experience (1d82ec9f)
- Add "listing complete" state showing file count (5059e00b)
- Add Linux checks to checker script (02ab0ab7)

### Fixed

- Fix MCP server port and tool naming (c2ae7de7)
- Fix race condition when loading files (38865e62)

## [0.2.0] - 2026-01-10

Initial public release. Free forever for personal use (BSL license).

### Added

- Dual-pane file explorer with keyboard and mouse navigation (c945f18c)
- Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column), switchable via ⌘1/⌘2
  (c779a6de)
- Virtual scrolling for 100k+ files (cf6c35d0)
- Chunked directory loading (50k files: 350 ms to first files) (869cdfb3)
- File icons from OS with caching (b8c588ec)
- File metadata panel with size color coding and date tooltips (bc3dc85b)
- Native context menu (Open, Show in Finder, Copy path, Quick Look) (7d977a12)
- Live file watching with incremental diffs (cf123728)
- Dropbox and iCloud sync status icons (46f1770d)
- Volume switching with keyboard navigation (ba3e7704)
- Network drives (SMB): host discovery via Bonjour, share listing, authentication, and mounting (54ee04f5)
- Sorting by name, size, date, extension with alphanumeric sort (e7b72068)
- Back/Forward navigation (56a5bf61)
- Drag and drop from the app (8e1d53b5)
- Command palette with fuzzy search (7b0ea13c)
- Window state persistence (position and size remembered) (b8d93c58)
- Dark mode support (7deb986b)
- Show hidden files menu item (4af855d7)
- Full disk access permission handling (9f433d8b)
- Licensing features (validation, about screen, expiry modal) (dc68eeb9)
- Keyboard shortcuts: Backspace/⌘↑ (go up), ⌥↑/↓ (home/end), Fn arrows (page up/down) (fc899d4d)
- getcmdr.com website (0f9eb210)
- License server (Cloudflare Worker) with Ed25519-signed keys (bff3e8a2)

---

### Development history

<details>
<summary>Click to expand full development history</summary>

#### 2026-01-10

Initial public release.

- Add licensing features to app (validation, about screen, expiry modal) (dc68eeb9)
- Add command palette with fuzzy search (7b0ea13c)
- Switch to BSL license (free for individuals) (06c49cba)

#### 2026-01-09

License server improvements.

- Add checkout tester tool for license server (38774feb)
- Add sandbox/live environment duality for license tests (15b39576)
- Unify trial period to 14 days (7e68c276)

#### 2026-01-08

Cmdr, website, licensing.

- Rename to Cmdr (016a3e3c)
- Restructure as monorepo with desktop app in apps/desktop (c0e764a7)
- Add getcmdr.com website (0f9eb210)
- Add license server (Cloudflare Worker) with Ed25519-signed keys (bff3e8a2)
- Add legal pages (privacy policy, terms, refund policy, pricing) (4f32a298)
- Streamline CI (website-only PRs: 22 min → 2 min) (48940033)

#### 2026-01-07

Network fixes.

- Fix network share unnecessary login prompts (dbeebaf9)
- Fix Back/Forward navigation across network screens (bf462e95)
- Sort network hosts and shares alphabetically (9de5f2b6)

#### 2026-01-05-06

Network drives (SMB).

- Add network host discovery via Bonjour (54ee04f5)
- Add SMB share listing (693e9262)
- Add network share authentication (283e5fd0)
- Add network share mounting (308d55cc)
- Add volume mount/unmount watching (76bbf222)

#### 2026-01-04

Sorting.

- Add sorting feature (name, size, date, extension) with alphanumeric sort (e7b72068)
- Add Stylelint for CSS quality (a778dccd)

#### 2026-01-02-03

Navigation and permissions.

- Add ⌘↑ shortcut to go up a folder (848e2f1a)
- Add full disk access permission handling (9f433d8b)
- Add Back/Forward navigation with menu items (56a5bf61)
- Add keyboard navigation to volume selector (46c30239)
- Save last directory per volume (9886fcdc)
- Set minimum window size (237c5a92)
- Fix opening files (714dc5a2)

#### 2026-01-01

Drag and drop, volumes.

- Add drag and drop FROM the app (8e1d53b5)
- Add volume switching feature (ba3e7704)
- Remove Tailwind (was slowing down app startup) (5354a48b)

#### 2025-12-31

Polish.

- Add font width measuring for precise Brief mode layout (848f68fe)
- Abstract file system access for better testing (eb9dd726)
- Fix Dropbox sync icon false positives (64007f07)
- Fix file watching reliability (aefe3e72)

#### 2025-12-30

Speed optimizations.

- Add keyboard shortcuts: ⌥↑/↓ for home/end, Fn arrows for page up/down (62989901)
- Move file cache to backend for major speed improvements (a42eda53)
- Optimize directory loading (phase 1 and 2) (7efd61a3)

#### 2025-12-29

View modes and cloud sync.

- Add Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column) (c779a6de)
- Add Dropbox and iCloud sync status icons (46f1770d)
- Add loading screen animation (234f0a70)

#### 2025-12-28

Performance and file operations.

- Add chunked directory loading (50k files: 350 ms to first files) (869cdfb3)
- Add file metadata panel with size color coding and date tooltips (bc3dc85b)
- Add native context menu (Open, Show in Finder, Copy path, Quick Look) (7d977a12)
- Add live file watching with incremental diffs (cf123728)
- Add virtual scrolling for 100k+ files (cf6c35d0)
- Add Backspace shortcut to go up a folder (fc899d4d)
- Scroll to last folder when navigating up (8ccd8bd8)

#### 2025-12-27

File metadata and icons.

- Add file metadata display (owner, size, dates) (d9994bc9)
- Add file icons from OS with caching (b8c588ec)
- Add per-folder custom icons support (210f23be)
- Add Tauri MCP server for AI tooling integration (0a64eb3c)
- Fix symlinked directory handling (5a134ac4)

#### 2025-12-26

Dual-pane explorer.

- Add dual-pane file explorer with home directory listing (c945f18c)
- Add window state persistence (position and size remembered) (b8d93c58)
- Add file navigation with keyboard and mouse (20424e01)
- Add "Show hidden files" menu item (4af855d7)
- Add dark mode support (7deb986b)

#### 2025-12-25

Project init.

- Initialize Rust + Tauri 2 + Svelte 5 project (b410bd94)
- Add GitHub Actions workflow (6dbf2657)

</details>
