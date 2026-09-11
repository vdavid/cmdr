# File system details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is everything else. Submodule depth lives in each
submodule's own `DETAILS.md` (`listing/`, `write_operations/`, `volume/`).

## Module map

Four submodules carry their own docs: `listing/`, `write_operations/`, `volume/`, and `sync_status/` (the cloud
badges). The leaves beside them:

- `watcher.rs`: FSEvents-driven listing updates.
- `staging.rs`: scratch visibility. The `StagingTemp` mint itself is `cmdr_fs::staging`.
- `index_provider.rs`: the app's `VolumeProvider`, so the index never has to import `VolumeManager`. Its
  `ensure_direct_smb` is `network::smb_connect_directly::connect_directly`, with every `UpgradeResult` but `Success`
  read as a refusal.
- `backend_settings.rs`: live per-backend knobs.
- `cloud_actions.rs`: iCloud download and eviction. `cloud_provider.rs`: who owns a path, and what they can do.
- `google_drive/`: Drive item links, with `mirror_db.rs` as the mirror-mode fallback, and `share_dialog.rs` for
  Drive's own share dialog (stream mode).
- `open_with.rs`: the "Open with" candidate apps. `share.rs`: the `Share` submenu's services.
- `tags.rs`: Finder tags. `terminal.rs`: "open terminal here". `text_editor.rs`: which app F4 opens a file in (wire
  types; the macOS half in `text_editor_macos.rs`, its tests in `text_editor_test.rs`).

## What `mod.rs` is for

`mod.rs` is a facade: it re-exports the listing, write-operation, volume, and watcher surfaces upward, and it owns
startup wiring that legitimately needs to know every backend (`init_volume_manager`, `register_discovered_volumes`,
`upgrade_existing_smb_mounts`, the direct-SMB and SMB-concurrency settings).

What it deliberately does NOT own is the volume registry itself. The singleton and `get_volume_manager()` live in
`volume/manager.rs` beside the type, and call sites import them from there. Reasons, and why a `pub use` shim back into
this module is the thing to resist: `volume/DETAILS.md` § "Key decisions".

## Hiding transient scratch (`staging.rs`)

Every write lands on a `.cmdr-tmp-*` sibling and takes its real name by a rename, so a copy makes files appear under
names the user didn't create for as long as it takes to write them. The 2026-07-31 incident's visible tail was one of
those left in the pane after an otherwise successful 768-file copy: the SMB watcher won the race against the rename, and
its batched add landed after the rename event that would have cleared it.

**Read-path, not watcher.** Filtering where the frontend asks for a range (`CachedListing::rows`) rather than where the
cache is filled is what makes the fix safe. The cache stays the truth; every accessor re-tests on every fetch, so an
entry the pane received can always be taken away again. Filtering the watcher instead inverts the bug into a worse one:
a full listing shows the temp, the watcher skips the removal that would clear it, and the pane keeps an entry pointing
at nothing. The `.sb-` filter lived in `crates/cmdr-smb/src/volume/watcher.rs` from 2026-04-10 to 2026-08-01 and had exactly that ghost — its
`continue` sat above the `match action`, so it skipped `Removed` too.

**Re-testing on every fetch is not the same as re-deriving the whole sequence on every fetch**, and conflating the two
is what wedged a big directory (`listing/DETAILS.md` § "Row numbers"). A listing materializes its row numbers once and
keeps the scratch-named entries — the only ones whose answer can still change — in a short side list it re-asks about
per read. `could_be_hidden_from_listings` is the pure name test that gates `is_hidden_from_listings`, so "this name is
settled" holds by construction rather than by reading three functions and hoping.

**Other apps' scratch hides by NAME, and that's a different rule on purpose.** macOS safe-save writes
`file.txt.sb-<uuid>` next to the original on every save (TextEdit, Preview, anything on `NSDocument`). There's no
ownership signal available for a file another process is writing and no way to tell a live one from an abandoned one, so
`advanced.showSafeSaveFiles` is a plain name filter over every drive. That coarseness is acceptable only because the
files aren't ours: an abandoned `.sb-` says something about TextEdit's day, not about a Cmdr bug, so nothing diagnostic
is lost by not surfacing it. It defaults to ON (shown), where Cmdr's own defaults to hidden — hiding another app's files
by name is a bigger claim to make on someone's behalf than hiding our own.

**Ownership, not name.** A scratch file a live operation owns is noise; one nobody owns is a leftover from an
interrupted transfer, and hiding that misreports what's on disk. The mint, the in-flight registry, and why the RAII
guard needs a liveness token behind it all live in `crates/cmdr-fs/src/staging.rs`, so a backend crate can stage a write
without reaching into the app; `staging.rs` re-exports them and adds only the two visibility settings.

**Which token the app hands it.** An operation's temps carry a `Weak` to `WriteOperationState`'s liveness token, dropped
by `end_liveness` wherever the operation leaves `WRITE_OPERATION_STATE`. ❌ Not `Arc<WriteOperationState>` reachability —
a task the driver abandoned holds one of those too, and the whole point is that its leftovers stop being hidden. A temp
minted outside any operation (the local safe-overwrite's two files) passes `None` and hides only until its function
returns.

**Known gap.** A leftover only becomes visible on the next fetch, and nothing forces one at settle. In practice
`transfer/volume/cleanup.rs::clean_abandoned_staged_writes` deletes the leftovers and that delete fires a watcher event,
so the pane updates on its own; the gap is the narrow case where the DELETE also fails. An immediate reveal would need
the volume-path to display-path mapping the name-keyed registry deliberately avoids.

**The escape hatch.** `advanced.showStagingTempFiles` (Settings > Advanced, off) shows the in-flight ones too. It's a
separate axis from `showHiddenFiles`: turning on dotfiles isn't a request to watch Cmdr's scratch.

## Cloud actions (`cloud_actions.rs`)

Wraps `FileManager.evictUbiquitousItem(at:)` and `startDownloadingUbiquitousItem(at:)` so the file context menu can
offer "Make available offline" and "Remove download". **iCloud Drive only.**

`NSFileProviderManager`'s host-side methods looked like the cross-provider API but are reserved for the app that
*bundles* the File Provider extension (Dropbox.app for Dropbox, and so on); a third-party app gets
`NSFileProviderErrorProviderNotFound` ("The application cannot be used right now") on the enumerate / evict / download
calls. The `FileManager` ubiquity APIs route through iCloud's separate code path and accept any URL inside an iCloud
container, so the menu items are offered only for paths under `~/Library/Mobile Documents/com~apple~CloudDocs/`.
`is_in_icloud_drive` gates them, by asking `CloudProvider::supports_eviction` rather than re-deriving the path rule. The
module-doc comment in `cloud_actions.rs` has the full story.

## Cloud providers (`cloud_provider.rs`)

One source of truth for "is this path in a cloud folder, and whose?", shared by the volume switcher (which wants a
display name and a stable volume ID) and the file context menu (which wants to know what the provider can do).
`locate(home, path)` is pure, so it's cheap on every navigation and unit-testable without a real cloud folder;
`volumes/cloud.rs` is a thin adapter over it, and a test pins `CloudProvider::ICloudDrive.volume_id()` against
`ICLOUD_VOLUME_ID` so the two can't drift.

`supports_eviction()` is the point of the enum: it states once that the eviction pair is iCloud's alone, so a provider
added later can't silently inherit actions it can't perform.

**It sees Google Drive's stream mode only.** Drive for desktop runs in one of two modes. In *stream* mode it lives under
`~/Library/CloudStorage/GoogleDrive-<account>/`. In *mirror* mode the files are ordinary local files in a folder the
user picks (`~/My Drive` by default), indistinguishable from any other directory by path. So a `None` from `locate` does
not prove a path is outside Google Drive.

## Google Drive links (`google_drive/`)

Backs "Open in Google Drive", "Copy Google Drive link", and "Ask Gemini". Drive registers no URL scheme (no
`CFBundleURLTypes`, no `NSServices` in its `Info.plist`), so these build web URLs.

**"Share on Google Drive" is the one item that opens Drive itself, in stream mode only** (`share_dialog.rs`, whose
header carries the mechanism and its evidence). It runs Drive's own `ACTION_SHARE` File Provider custom action through
FileProvider.framework's private host API, the way Finder does, gated on Drive's own activation rule. What a reader
needs before touching it:

- **Mirror mode can't have it.** A mirrored file isn't a File Provider item, and Drive's Finder menu there comes from
  its Finder Sync extension, which only Finder can host. The fetch answers nil, so the menu stays as it was.
- **Private API, so it fails closed.** Runtime class and selector checks, `objc2::exception::catch` around every call,
  and a 250 ms wait at menu build (`commands/menu.rs`). Anything off means no menu item, never a crash.
- **No completion block on the operation.** Its signature is private, and a guessed argument we'd dereference could
  crash the app, so a click reports only whether it scheduled.
- **Every block handed to File Provider needs a type signature** (`RcBlock::with_encoding`, never `RcBlock::new`).
  File Provider wraps completion handlers through `_Block_signature`, and a block without one becomes a nil handler
  inside File Provider that crashes the app when the reply arrives. The ignored
  `file_provider_answers_a_real_stream_item_without_crashing` test reproduces it against a real streamed item.

**One resolution, every URL.** `item_links()` resolves the item ONCE into a private `ResolvedItem` (id + kind +
resource key) and formats each URL from it, so a context menu never pays two xattr reads or two SQLite round-trips for
the same file. The public answer is a `DriveItemLinks { view_url, gemini_url }`, and `None` still means "not a Drive
item, offer nothing".

- `view_url` is the item's own page, in the shape its kind takes (see `mod.rs` for the per-kind table). A resource key,
  which only items shared through a resource-key link carry, qualifies this URL alone.
- `gemini_url` is `https://drive.google.com/drive/ai?di=<id>`, captured from the `Ask Gemini` entry in Drive for
  desktop's own Finder menu (2026-09-09) beside its `Open with Google Drive` link, which resolved the same id. It's
  `None` for folders: `?di=` names a document, and Drive's own menu leaves the action out there too. No resource key
  rides along, because Drive's own link carries none.

**Gotcha: our `Ask Gemini` shows for every resolvable Drive FILE, Drive's own shows for fewer.** Drive gates its Finder
entry on an account flag (`domainUserInfo.CONTEXT_MENU_OPEN_GEMINI_WEB`) that lives inside its own process and no API
exposes. On an account without Gemini, ours opens a Drive page saying so. Accepted deliberately (2026-09-09): a link
that occasionally lands on an explanation beats hiding the action from everyone who does have it.

**Three ID sources, in this order** — cheapest and most certain first, and the first one to answer wins:

1. **Google-native stubs** (`.gdoc`, `.gsheet`, `.gslides`, `.gform`, …) are small JSON files carrying `doc_id`. They
   work in BOTH Drive modes, and they're first because a native doc's canonical URL is on `docs.google.com`, which the
   bare ID can't tell us. They carry `resource_key` (empty unless the item is shared through a resource-key link) and
   the account `email`; there is **no** `url` field, so the URL has to be built.
2. **The `com.google.drivefs.item-id#S` xattr.** The `#S` suffix is macOS's File Provider syncable-attribute marker and
   part of the name: a `getxattr` for the bare name finds nothing.
3. **Drive's mirror-mode metadata databases** (`google_drive/mirror_db.rs`), the only source that answers for an
   ordinary mirrored file. Its own module header carries the mechanism, the schema, and the fail-closed rules; the
   section below is what a reader needs before touching it.

**Gotcha: the xattr exists in stream mode only.** Verified 2026-09-07 with `xattr -r -l`: present throughout
`~/Library/CloudStorage/GoogleDrive-…` on files AND directories, absent on every file in a mirrored `~/My Drive`. A
stream-mode `.gdoc` carries both sources, holding the identical ID. That's why nothing here gates on a path prefix: the
menu offers its items when an ID resolves, which is self-validating in every Drive setup.

### Mirror mode: resolving through Drive's own databases

A mirrored file is an ordinary local file. No xattr, and `NSFileProviderManager.getIdentifierForUserVisibleFile` answers
"the file doesn't exist" for both `~/My Drive/…` and the `~/Library/CloudStorage/GoogleDrive-…/My Drive/…` symlink that
points at it, while correctly answering `dbitem:17720` for a Dropbox file (verified 2026-09-08). Finder shows Drive
actions there through a Finder Sync extension (`com.google.drivefs.finderhelper.findersync`), which only Finder can
host. So the databases are the only route, and Drive keeps a **pair** of them per account in
`~/Library/Application Support/Google/DriveFS/<account-id>/` (a numeric directory we enumerate, never hardcode):
`mirror_sqlite.db` maps the local tree, `mirror_metadata_sqlite.db` maps a `stable_id` to the Drive item.

**❗ The mirror pair's `stable_id` space is NOT the stream-mode one.** The same file was `330816` in
`mirror_metadata_sqlite.db` and `324388` in the `metadata_sqlite_db` sitting beside it (verified on Drive for desktop
130.0, macOS 26.6.2, 2026-09-09). Reading a mirror id out of the stream-mode database silently hands back a different
file.

**❌ Never resolve by name.** `local_filename` repeats (`_archive` three times in a real 3,268-row mirror) and carries no
index of its own. Resolution is: walk UP the path until an ancestor's inode is a mirror root's (inodes rather than a path
prefix, so both spellings of a mirrored path work), then walk back DOWN one indexed
`UNIQUE (parent_local_stable_id, local_filename)` lookup per component, then **prove it** — the row we land on must
carry the inode the file actually has. A stale row describing the previous file at that path fails the proof.

**Fail closed at every step.** A missing or locked database, a changed schema, a trashed or tombstoned row, an
unresolvable shortcut, an inode mismatch: all produce no menu item, which is exactly what these files got before. The
downside of every failure is bounded at "no worse than not trying"; a wrong link would not be.

**Read-only, forever.** `open_read_only` is the only door (`desktop-rust-sqlite-open-direct` enforces the factory). A
read-only WAL reader does touch the `-shm` sidecar to register a read mark — WAL index bookkeeping, not database
content, and it can't block Drive's writer. `?immutable=1` would avoid even that but ignores the WAL, and the WAL held
222 MB against a 220 MB database here, so it would answer from last month.

**Cost** (measured 2026-09-09 on the real 3,268-item mirror, debug build): 12 µs for a path outside Drive, 0.5 ms for
one inside it, 9.8 ms on the first call of a 30-second window, which is when the account scan runs. The account and root
scan is cached for that window precisely so an ordinary right-click anywhere doesn't open a multi-megabyte SQLite
database to learn nothing.

**Shared drives** need nothing special: a mirrored one is another root row and its files take the same
`drive.google.com/file/d/<id>/view` shape. **Shortcuts** resolve one hop through `shortcut_details` to their target, so
the link opens the file the pane shows rather than the pointer; a shortcut to a shortcut, or one whose target row is
gone, resolves to nothing.

**URL shapes** are each item's `viewUrl` as Google's own Drive API returns it, checked against real items (2026-09-07),
not copied from documentation:

- binary file → `https://drive.google.com/file/d/<id>/view`
- folder → `https://drive.google.com/drive/folders/<id>`
- Doc / Sheet / Slides / Form → `https://docs.google.com/{document,spreadsheets,presentation,forms}/d/<id>/edit`

A Google-native type with no verified editor path (`.gdraw` and friends) falls back to Drive's type-agnostic
`https://drive.google.com/open?id=<id>` resolver rather than an invented editor segment.

**What we deliberately don't offer.** Drive's pin-offline toggle and its Share sheet are File Provider custom actions
reserved for the app that bundles the extension, so the eviction pair stays iCloud-only. Sync status, by contrast, is
already provider-agnostic: a streamed Drive file carries `SF_DATALESS` like any other stub (verified 2026-09-07 with
`stat -f %Sf`), so `sync_status/` answers for Drive without changes.

## Open with (`open_with.rs`)

- `URLsForApplicationsToOpenURL:` produces candidate apps, with multi-selection intersection across the selected files.
  It's macOS 12+, above the bundle's 10.15 floor, so `fetch_candidates_for_path` gates on
  `crate::platform::macos_at_least(12, 0)` and drops to `URLForApplicationToOpenURL:` (macOS 10.10) below that. Catalina
  and Big Sur therefore see only the OS default app in the menu, which is a shorter list rather than a crash: an
  unrecognized selector would raise `NSInvalidArgumentException` and abort the process. The `allowed-newer-selector`
  marker on the call is what tells `desktop-rust-macos-availability` the gate exists (it reads lines, not control
  flow).
- A session cache keyed by lowercased extension avoids repeated lookups; it subscribes to
  `NSWorkspace.didLaunchApplicationNotification` / `didTerminateApplicationNotification` for invalidation (per the
  "Subscribe, don't poll" principle; the TTL is a fallback only).
- `open_paths_with` launches with a single multi-URL
  `openURLs:withApplicationAtURL:configuration:completionHandler:` call.
- `pick_app_via_open_panel` shows an `NSOpenPanel` filtered to `.app` bundles for the "Open with → Other…" entry.
- Worker threads use 8 MB stacks (FileProvider XPC depth), per the gotcha in `CLAUDE.md`.
- **App names are what Finder shows.** `read_app_display_name` asks `NSFileManager` `displayNameAtPath:` (macOS 10.0)
  and trims a trailing `.app`, which stays when Finder's "Show all filename extensions" is on. The plist's
  `CFBundleDisplayName` / `CFBundleName` are only the fallback for a path with no bundle to ask about, because they
  aren't what people read: VS Code's plist says `Code`, while Finder, Spotlight, and the Dock say "Visual Studio Code"
  (verified on macOS 26.6.2 with VS Code 1.137.0, compiled probe, 2026-09-11). The terminal row, the text editor row,
  and the file-viewer row in `reveal/` name apps through it too.

## The Share submenu (`share.rs`)

The file context menu's `Share` is an inline submenu, one item per service macOS offers for the right-clicked rows, the
way Finder and Nimble Commander draw it. `services_for` enumerates; `../menu/share_submenu.rs` draws; `perform_offered`
runs the pick. AirDrop, Mail, Messages, Notes, and every installed share extension come from the system; Cmdr
contributes the file URLs and the item order macOS gave it.

**Decision: hand-build it, and take the deprecation.** `NSSharingService.sharingServicesForItems:` is deprecated (macOS
13, in favour of `NSSharingServicePicker.standardShareMenuItem`) and it is still the only API that hands over the LIST.
`standardShareMenuItem` is a plain action item: `hasSubmenu=false`, `submenu=nil`, title `Share…`, action
`_performStandardShareMenuItem:`, and clicking it opens the same popover (measured on macOS 26.6.2 by interrogating the
item and popping it up for real, 2026-09-09). So there is no modern equivalent to reach for, and no availability gate to
write either: the enumeration is 10.8+. Nimble Commander, whose submenu this matches, references
`NSSharingServicePicker` nowhere and hand-builds from the same call.

**Why it stopped being a popover.** A picker can't say whether it has anything to offer before it's on screen, so a
selection macOS offers nothing for came up as a sheet holding only `Edit Extensions…`. An enumeration answers first, and
an empty answer leaves the whole `Share` item out (`menu_structure.rs`). ⚠️ That empty case is real and reachable: a
broken symlink and a path that has since vanished both enumerate to zero, while an unreadable file, a FIFO, a device
node, and a directory all still offer services (verified on macOS 26.6.2 across those shapes, 2026-09-09).

**What we gave up with the popover**: the system sheet's `Edit Extensions…` row, which opens the Sharing pane of System
Settings. Nothing in the submenu replaces it today; a `More…` item at the bottom would, at the cost of one catalog key
in every locale.

**Items are `NSURL` file URLs.** Both calls take `NSArray *` with no element type (anything conforming to
`NSPasteboardWriting`); a file URL is what makes every service send the FILE rather than a rendering of it, and it's what
Finder hands over for the same gesture. A path that isn't valid UTF-8 is dropped from the list rather than sinking the
whole share.

**A click is routed by INDEX, and the offer is kept alive in a thread-local.** Menu ids are `share-service:<index>` into
the offer that filled the menu, and `OFFERED` holds both the `Retained<NSSharingService>` list and the very `NSArray`
they were enumerated for, so the pick performs on the items macOS vetted rather than on a re-read of the selection. The
index (not the title) is the key for the house reason: a title is macOS copy in the system language, and two extensions
may well share one. Thread-local rather than a `static` because `Retained` is neither `Send` nor `Sync`; both ends take a
`MainThreadMarker`, which is also what pins fill and read to the same thread.

**Cost on the popup path**: about 11 ms warm, ~190 ms on the first enumeration of the process while LaunchServices wakes
up, plus under 1 ms to draw the icons (measured on macOS 26.6.2, 2026-09-09). Uncached on purpose: the enumeration is
per-selection and must be fresh, and the cold bill is the same one `compute_open_with_choices` already pays here.

**Each item's icon is the service's own `NSImage`, drawn to 16×16 RGBA** through `icons::render_ns_image`, then fed to
`IconMenuItem` — the same route "Open with" takes for app-bundle icons, and full-colour non-template pixels are the
shape a context menu renders correctly (`menu/DETAILS.md` § SF Symbol icons). macOS already reports 16×16 for every
service image, so nothing is resampled.

**Which rows may be shared at all is the FRONTEND's answer**, arriving as `PaneContextMenuFacts.canShare`. A service
needs a real file behind the URL, which rules out phones, the SMB host list, an archive's insides, and the virtual
`.git` portal, while the search-results snapshot (real files, no folder of its own) is fine. That mix isn't a single
volume-kind lookup, so it lives in `src/lib/file-explorer/pane/volume-capabilities.ts::rowIsOsVisible`. `Share` needs one
more yes on top of it: a non-empty enumeration.

## Open terminal here (`terminal.rs`)

macOS has no system-wide "default terminal" setting. Finder's own "New Terminal at Folder" service is hardcoded to
Terminal.app, and iTerm2 works around that by installing its own service. So Cmdr carries a table, the way VS Code,
Marta, and Raycast do.

The external paper trail behind the table (every bundle id's source and date, why each app gets the recipe it gets, and
the per-app window-vs-tab survey) is `docs/notes/terminal-launch-sources-2026-09-04.md`. Read it before adding an app.

- **`KNOWN_TERMINALS`** holds bundle id, display name, and a `LaunchRecipe` for Terminal, Alacritty, Ghostty, Hyper,
  iTerm2, kitty, Warp, and WezTerm. Every bundle id carries its verification source and date in the doc comment above
  the table; a new entry owes the same. Adding one is a single struct literal, nothing else.
- **Three recipes, because three shapes of app exist.** `FolderAsDocument` (`open -b <id> <dir>`) covers everything
  that registers as a folder handler. `WorkingDirectoryFlag` (`open -n -b <id> --args --working-directory <dir>`) is
  Alacritty, which won't take a folder as a document; the `-n` is load-bearing, since without it a running instance is
  merely activated and the args are dropped. `WarpUri` is Warp's documented scheme. A custom "Choose an app…" pick is
  launched with `open -a <app path> <dir>`.
- **No window-vs-tab control, deliberately.** No portable mechanism exists, and the six vendors answer the question six
  different ways. Each app is launched the way it natively takes a folder, and the app's own preferences decide. The
  per-app survey and its sources are in the note above; don't re-litigate the toggle without reading it.
- **`launch_argv` is pure** (choice + folder → argv), which is what makes every recipe unit-testable without launching
  anything. Nothing goes through a shell, so awkward folder names travel as ordinary arguments; Warp's URI is the one
  place that needs encoding, and its `WARP_PATH_SET` keeps `/` literal (legal in a query, and the shape Warp's docs
  show) while encoding `&`, `?`, `#`, `%`, `+`, space, quotes, and every non-ASCII byte. The launch-time verification
  that pins that set is in the note above.
- **Installed-ness is one `URLForApplicationWithBundleIdentifier:` call per app**, asked whenever the settings row
  renders and again at launch time. ❌ Never a `/Applications` scan, and no "Refresh" button, because there's nothing
  to refresh.
- **Icons come from the bundle's own `.icns`** (`open_with::app_icon_data_url`, shared with the text editor row), not from
  the OS icon provider: a plain file read needs no TCC permission and can't descend into a FileProvider XPC chain deep
  enough to overflow a blocking-pool thread's stack.
- **The path-less refusal keys on the volume**, not the path string: `paths_are_os_visible()` is the same reading Quick
  Look and the drag commands take, so an OS-mounted share says yes and MTP, ADB, and a share whose mount went away say
  no. An unknown volume id is an unmount race rather than a verdict, so it assumes yes.
- **Both commands answer with a variant, never a sentence.** `OpenTerminalOutcome` separates `opened` from
  `app_missing_opened_terminal_instead` (the chosen app was uninstalled, so Terminal opened instead) and
  `not_a_local_path`; `OpenTerminalError` is the "couldn't answer at all" half.
- **The chosen app is passed in, never read here.** The frontend owns the settings store; Rust's own loader is the
  startup-time read only (`settings/CLAUDE.md`). Both commands take the stored value as an argument.
- The `playwright-e2e` build records the folder into `crate::open_mock` instead of launching, alongside `open_path` and
  `open_in_editor`, so a suite run doesn't pile up terminal windows nothing can close. It records the FOLDER, never the
  argv: Warp's recipe ends in a URI rather than a path, so a spec reading the argv would assert something different for
  one app in the table than for the other seven.

## Text editor (`text_editor.rs`)

Which app F4 opens a file in: the regular pane, the search-results pane, and the ⇧F4 new-file auto-open all reach
`open_in_editor`. The stored choice (`behavior.textEditorApp`) arrives as an argument, like the terminal's.

- **Three shapes, told apart structurally.** `system` (or empty) runs `open -t <file>`, byte-identical to F4 before the
  setting existed. An absolute path runs `open -a <app> <file>`. Anything else is a bundle id and runs
  `open -b <id> <file>`, which lets LaunchServices pick the copy. `launch_argv` is pure.
- **The list is `LSCopyAllRoleHandlersForContentType("public.plain-text", kLSRolesEditor)`** through `core-services`.
  Why, from a compiled Swift probe (verified on macOS 26.6.2 with Sublime Text 4200 and VS Code 1.137.0, 2026-09-11):
  - The editor role lists seven for plain text: Xcode, TextEdit, Warp, Path Finder, LibreOffice, Sublime Text, and VS
    Code. The viewer role adds browsers, Notes, and Script Editor, which is what the role filter keeps out.
  - VS Code declares text by extension (`txt`) and OSType (`TEXT`, `utxt`) only, and LaunchServices files it under
    `public.plain-text` and none of `public.text`, `public.utf8-plain-text`, or `public.source-code`. Plain text is
    the one query that lists it, and a union with those types adds nothing.
  - ❌ Not `NSWorkspace` `URLsForApplicationsToOpenContentType:`: no role filter (a second copy of Warp listed twice),
    and it needs `UniformTypeIdentifiers.framework`, which dyld refuses on the 10.15 floor. ❌ Not
    `URLsForApplicationsToOpenURL:` on a made-up path: a path with nothing at it answers zero apps.
  - Both C functions are deprecated since macOS 12 and still in the macOS 26 SDK. `core-services` 1.0.0 marks neither
    `#[deprecated]`, and `desktop-rust-macos-availability` checks Objective-C selectors only. If a bump adds the
    attribute, allow it per call with a reason naming the 10.15 floor.
- **Both return +1 objects (the Create rule), or NULL when nothing claims plain text** (an empty list, not an error).
  Each wrapper null-checks and wraps with `wrap_under_create_rule` exactly once: a get-rule wrap leaks, and a second
  wrap over-releases and crashes somewhere unrelated later.
- **LaunchServices' order isn't stable** (Xcode moved to the front once it became the default), so `apps` comes in no
  particular order and the frontend sorts by name.
- **Bundle ids compare case-insensitively.** Finder's "Change All" stored the default as `com.apple.dt.xcode`, while
  the APIs answer `com.apple.dt.Xcode`.
- **The system default** is `LSCopyDefaultRoleHandlerForContentType("public.plain-text", kLSRolesAll)`. Its id is left
  out of `apps` (it's the "System default (…)" row), and a choice that pins it by id gets a row of its own. A listed id
  counts only while its bundle is on disk: LaunchServices keeps answering for a deleted bundle until it notices.
- **The pick rule: store the bundle id unless the user deliberately picked a different copy than the one
  LaunchServices would launch.** The frontend passes the picked path to `list_text_editors` and stores `chosen_id`. A
  pick becomes its bundle id when that id resolves to the very bundle picked, compared after `std::fs::canonicalize`
  (the dialog's spelling and LaunchServices' can differ by a symlink or a trailing slash), and stays a path otherwise.
  When either side runs through `AppTranslocation/` (a quarantined download macOS runs from a randomized read-only
  mirror), its path says nothing about where the original sits, so the `.app` folder name decides instead: the id
  already matched, and only a copy renamed on purpose is a different one. That's `is_same_bundle`. Translocation is
  real here: Sublime Text and VS Code, both downloaded through a browser, ran from `AppTranslocation/` while
  `URLForApplicationWithBundleIdentifier:` and `NSRunningApplication` reported `/Applications` (same machine and date).
- **A chosen app that's gone** (a bundle id that no longer resolves to a bundle on disk, or a path that isn't a
  directory) gets the file opened with `open -t` and the outcome `chosen_app_missing_opened_default_instead`. An app on
  a volume that's unmounted right now reads as gone too.
- **`opened` means `open` took the request, never that a window appeared.** The first launch of a freshly downloaded VS
  Code exited with no window while `open -b` still exited 0; on a retry, `open -b com.microsoft.VSCode <file>` and
  `open -a "/Applications/Visual Studio Code.app" <file>` both opened the file (same machine and date). Both forms
  work, so bundle ids keep `open -b`.
- **What's verified about `open -t`**: with Xcode set as the plain-text default through Finder's "Change All", `open -t`
  launched Xcode, matching the resolved default. "Change All" writes an all-roles handler (`LSHandlerRoleAll`), so this
  can't tell `kLSRolesAll` from `kLSRolesEditor`: a role-specific override (from `duti`, say) could name one default
  while `open -t` launches another. Unverified.
- **Everything in the report after the launch runs after it, and only when something reads it**: the app's name when
  the outcome is a fallback or the caller asked about other editors (`report_needs_app_name`), and whether other
  editors exist only when asked. That query uses ids and installed-ness only, ❌ no names or icons: all of it shares the
  launch's 5 s deadline, and a deadline expiring after `open` spawned would word an editor that did open as a timeout.
- **Cost** (debug build, same machine and date): the editor query is 0.3 ms warm and 7–14 ms as a process's first
  LaunchServices call. `list_text_editors` with six apps, the default, names, and icons took 371 ms in a cold test
  process and 57 ms warm, nearly all of it icon decoding.
- The `playwright-e2e` build records the FILE into `crate::open_mock` instead of launching, whatever the choice, so
  every `e2e_opened_paths` consumer is untouched. On macOS that's `text_editor_macos.rs`'s own `launch`; off macOS it's the
  command's E2E arm.

## Finder tags MCP consumer (`tags.rs`)

The MCP `tag` tool wraps `tags::toggle_color` / `set_tags` (and `system_color_name` for canonical names), resolving
target paths off the pane state and refreshing via `apply_tags_to_listing`. `cmdr://state` file entries also surface a
`[tags:…]` marker mirrored from `PaneFileEntry.tags`. See `mcp/DETAILS.md`.

## Threading

The 8 MB-stack OS thread pattern (instead of rayon) for macOS framework calls is in `sync_status/pool.rs` as the
reference. The reasoning: NSURL resource-value lookups and FileProvider queries make synchronous XPC round-trips that
can consume deep stack frames through FileProvider override chains (iCloud, Dropbox), exceeding rayon's 2 MB worker
stack; running them on rayon would also starve the pool, which should stay reserved for CPU-bound work.

A per-call `std::thread::scope` isn't good enough on its own: those calls can block forever, so the threads have to be
pooled and hard-capped or they accumulate (21-23 of them in the 2026-07-31 wedge). `src/icons/` (a separate top-level
module) follows the same rule for `fetch_path_icons`.

## Watcher threading

The notify-rs debouncer callback runs on notify-rs's internal thread, which has no Tokio runtime, so `tokio::spawn`
there panics with "there is no reactor running". It bit `watcher.rs`'s full-reread fallback path (`>500` events or
ambiguous event kinds), then again in v0.24.0 via `git::watcher::refresh_local_listings_under` →
`listing::caching::notify_directory_changed(FullRefresh)` (CRASH-26SBB) — which is why FullRefresh dispatch now funnels
through `caching::spawn_full_refresh`. Use `tauri::async_runtime::spawn` (same as `indexing::watch::watcher`), and
apply the rule to every watcher OS thread (git, SMB, MTP, archive), not just notify-rs.

## Arming a listing watch is detached

`start_watching_detached` hands the arm to the blocking pool; the listing pipeline never waits for it. Arming is slow,
and slow by an amount that has nothing to do with the directory being listed:

- `FSEventStreamCreate` + `FSEventStreamStart` are a handshake with `fseventsd`, and `notify` blocks until the stream's
  new CFRunLoop thread has published its `CFRunLoopRef`.
- `coverage_for_watched_path` adds a `statfs`.
- Worst of all, it queues on `WATCHER_MANAGER` behind the PREVIOUS listing's teardown, because the frontend fires
  `listDirectoryEnd(old)` immediately before loading the new directory (`listing-loader.ts`).

That mattered because `read_directory_with_progress` armed the watch before it emitted `listing-complete`, and the pane
renders nothing until that event (`listing-loader.ts::handleListingComplete` is the only place a listing is committed).
So the whole arm was dead time the user saw as a stalled "Sorting your files, preparing view…".

Measured while navigating a warm `~/Downloads` (macOS 26.5.2, 2026-08-11, from the `stall_probe::listing` line): p50
88 ms, p75 288 ms, p90 653 ms, max 1,509 ms, against `read_dir` 0–8 ms and `sort` 0 ms. A release build on the same
machine was worse (p90 775 ms, max 5,081 ms), so this was never a debug-build artifact. **Cost is independent of
directory size** — a 3-entry folder hit 723 ms while a 265-entry folder hit 57 ms — which is what says "lock and
run-loop scheduling", not "I/O proportional to the work".

Two supporting changes came with it:

- **`stop_watching` drops the `WatchedDirectory` OUTSIDE the manager's write lock.** Dropping it tears an FSEvents run
  loop down, and notify's teardown busy-spins on `CFRunLoopIsWaiting` before joining the stream's thread. Holding the
  write lock across that is what made an arm queue behind the previous teardown. ❌ Don't fold the removal and the drop
  back into one `if let Ok(mut manager) = …` block.
- **The debouncer uses `NoCache`, not the platform-default `RecommendedCache`** (a `FileIdMap` on macOS). The map exists
  to pair a rename's `From` with its `To` by file id, and pays for it by walking the watched directory and `stat`ing
  every entry at arm time, then re-`stat`ing on every create, rename, and remove. Cmdr gets nothing for that:
  `handle_directory_change_incremental` collects the unique paths out of a batch and re-stats each one, so a rename
  classifies identically whether it arrives as one paired event carrying both paths or as a separate `From` and `To`.
  Root-rename detection is unaffected too, since `watch_root_identity_changed` matches on `Modify(Name(_))`, which the
  debouncer emits either way. Linux already ran this path with `NoCache`.

**The reconcile half of a detached arm is not optional.** `list_directory_end` removes the listing from `LISTING_CACHE`
and then removes a watch the arm may not have inserted yet, so an arm can land on a listing nobody will ever close
again. `arm_and_reconcile` re-checks `LISTING_CACHE` membership after arming and tears the watch down if the listing
went away; without that, each such navigation strands an FSEvents stream, its CFRunLoop thread, and a manager entry for
the life of the process, each one still costing `fseventsd` fan-out. Pinned by
`watcher_test::a_detached_arm_that_lost_the_race_leaves_no_watch_behind`, which drives `arm_and_reconcile` directly so
the losing interleaving is the only one under test.

`watcher_start_ms` in the `stall_probe::listing` line now measures only the dispatch, so it should read ~0. A
regression that puts arming back on the critical path shows up there first.

## Watcher path rebasing

On macOS, FSEvents reports canonical paths (`/private/tmp/…`) while `LISTING_CACHE` holds the user-navigated form
(`/tmp/…`). `watcher.rs::rebase_event_path` compares the firmlink-normalized forms
(`indexing::paths::firmlinks::normalize_path`) and rebases matching event paths onto the listing's directory. A raw
`path.parent() == dir_path` comparison silently dropped every event for listings under `/tmp`, `/var`, and `/etc`, so
the pane never updated until the user re-navigated.

FSEvents also resolves a **symlinked watch root** and reports events under the real target, so the handler additionally
matches against the `canonicalize`d watch dir. This bit Google Drive, whose `My Drive` is a symlink to `~/My Drive`, so
rename/create/delete never refreshed the pane; iCloud and Dropbox mount real directories and hit the firmlink path
instead.

## Reordered rows

A row whose own sort key changes (an mtime bump under sort-by-date, a size change under sort-by-size) jumps to a new
position while every other row merely slides over. `DiffChangeType::Move` reports that jump with `previous_index` and
`index`, and it carries the fresh entry, so it replaces the `Modify` rather than accompanying it.

**Why a dedicated variant rather than a remove plus an add**: the frontend rides the pane cursor and the selection along
a move by identity (`listing-diff-sync.svelte.ts::reconcileCursorAndSelection`). Reported as a remove plus an add, the
cursor instead stays on the vacated index, which now holds a neighbour. Watch a big folder being deleted in a
date-sorted pane and that reads as if the wrong folder were disappearing, which is exactly how it was found.

Both watcher paths produce it: the incremental path from `ModifyResult::Moved` (the cache re-inserts the entry at its
new sorted position and knows both indices), and the full re-read path from `compute_diff`.

`compute_diff` has to separate a real jump from the index shift every row below an add or a remove takes, or an ordinary
delete would make the pane chase the cursor around. It reads the surviving rows' old positions in new order and keeps
the longest increasing subsequence of them (patience sorting, O(n log n)); those rows held their relative order, and the
rest are the minimal set that genuinely moved.

**One event's removals are one call**, `listing::remove_entries_by_paths`, not a loop: the batch resolves every doomed
row against the pre-removal listing in a single pass under one write lock, which is what keeps the indices the
`directory-diff` carries in one index space and stops a 500-path event walking the listing 1,000 times. Why, and what
the loop cost: `listing/DETAILS.md` § "Entries by path".

## Replacing a watch root

The incremental watcher path classifies each event against the cached listing, so it can only learn about entries the OS
names. When the watched directory is itself replaced (a `git checkout` across branches, `rsync --delete`, unzipping over
a folder, a build regenerating its output dir), macOS names almost none of them.

Measured against a live pane by logging the raw debounced batch (macOS 26.5.2, `notify-debouncer-full` 0.7.0,
2026-08-08),
`rm -rf target && mkdir target && touch gamma.txt delta.txt` delivers exactly:

- `Remove(Folder)` on the watch root
- `Create(Folder)` on the watch root
- `Modify(Metadata(Extended))` on the watch root
- `Create(File)` for each NEW child

There is **no remove event for the old children at all**: the directory went away as a unit, and FSEvents reports the
unit. All three root-level events are correctly rejected by `rebase_event_path` (their parent isn't the watch root), so
a child-only classifier applied the adds and kept every entry the replacement took away. The pane then showed a union of
the old and new listing, indefinitely: no later event ever mentions a removed name, and the ghosts survive a `⌘R`-less
session until the user navigates away and back. Repeated replacements stack more ghosts. The FSEvents stream itself
survives the replacement, so this is not a dead watch: the pane keeps getting *some* updates, which is what made it look
like a refresh timing issue rather than a correctness one.

`watch_root_identity_changed` therefore escalates to the full re-read (`handle_directory_change`) whenever a
`Create`, `Remove`, or `Modify(Name)` event names the watch root itself (in either path form, via
`event_targets_watch_root`, for the same reason `rebase_event_path` needs two). The re-read diffs against disk, replaces
the listing, and, when the directory is genuinely gone, emits `directory-deleted` and stops the watch.

**Gotcha**: `Modify(Metadata(_))` on the root is deliberately not a trigger. Every ordinary child create or remove bumps
the directory's own mtime and produces one, so counting it would route every change through the full re-read and cost
the incremental path its entire reason to exist.

## Tag analytics

`toggle_color` reports `tag_toggled` at its single exit, so all three triggers (the seven keyboard commands, the
context-menu circles, and the MCP `tag` tool) are covered without any of them having to remember. The trade is that
the event can't say WHICH trigger fired; a `surface` parameter would have meant threading one through every caller and
every test for a question none of them asks yet. ❌ The `color` prop is the Finder palette's canonical name (a closed
set of seven), never a tag's own text, which is user-authored content. Props: `src-tauri/src/analytics/DETAILS.md` §
"Starter event set".
