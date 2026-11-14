# cmdr-about details

The fact card behind "Tell me about this" (#1104): where an item came from, what it really is, who owns it, how it's
used, and what removing it costs. The card works with AI off; a model, when there is one, adds a few sentences beside
these facts and decides none of them.

## Layering

Modules import downward only, which is what keeps `module-cycles` green:

- `text` (no imports) → `fact` → `payload`, `owner`, `provenance`, `sniff` → `digest` → `card` → `removal`, `headline`,
  `actions` → `model`.
- `gather` imports nothing from the crate: it's generic over the job key and the answer.
- `removal::consequences`, `headline::headline`, and `actions::actions` are free functions over `&AboutCard`, never
  methods in `card.rs`, because `card` must not import the modules that read it. `for_model` is a method, but declared
  in `model.rs` (an inherent `impl` in another module of the crate), for the same reason.

## Coverage: `Fact<T>`

- `Known(T)`, `Unknown(Unknown)`, `NotApplicable(NotApplicableWhy)`, `Pending`. On the wire:
  `{ "state": "unknown", "value": { "why": "noRecord", "prominence": "routine" } }`.
- **`Unknown` is built only from an `UnknownWhy`** (`Fact::unknown`), so the prominence on the wire is always the one
  the rule gives. Routine: `NoRecord`, `NotInStatus`, `NoInstalledAppMatches`, `VolumeCantTell` (the card's volume note
  says that one once), and `NotIndexed`. Everything else is Notable.
- **Decision: `NotIndexed` is Routine.** Indexing off or not there yet is nothing going wrong, and on a folder it's one
  cause behind its size, what's inside, and its importance: Notable, it drew three warning sentences on every folder of
  an unindexed drive (screenshots, 2026-11-13). `only_ordinary_absences_are_routine` pins the list.
- **Decision: a nested `Fact` inside a payload** where one part can be unknown while the rest is known:
  `GitContext::status` (the root and branch are cheap, the status can be a whole-worktree walk that times out),
  `Download::sources` (a quarantine record without a readable source list), `FolderDigest::children` and
  `largest_files`, and each child's size. Why: the alternative is a kind per sub-answer, which splits one job and one
  row in two.
- **`NoInstalledAppMatches` versus `LibraryNameMatchesNoApp`**: the same lookup result, Routine outside the Library
  (nobody expects an owner there) and Notable inside it (a folder in `Application Support` that no installed app answers
  to is what the person came to ask about). Two variants, so the rule needs no path.
- **Decision: `GitStatus` has no "not in status" variant.** `list_status` can't tell a clean file from an ignored one
  from one outside the repo, so that answer is `Unknown(NotInStatus)` in `GitContext::status`, ❌ never `Clean`. The
  vocabulary already holds `Tracked(Clean)` and `Ignored` for a per-path read.

## Text from the item: `ItemText`

`model_text` is the rule of the app's `tools/ask.rs::fact_text` (no control or bidirectional characters, the middle
dropped past 400 characters so an extension survives), plus U+061C and the line and paragraph separators, which would
start a row of their own in plain text. `ItemText::new` is the only constructor, so a payload field typed `ItemText`
can't hold an unsanitized string.

**A name that hid a direction override says so** (`Identity::name_hid_reordering`, from `hides_reordering` on the raw
name). Dropping the character shows the name's real ending everywhere Cmdr shows it, but other apps still show the
disguise (`invoice<U+202E>fdp.command` reads as a PDF in some), so the card says once, in a Name row, that it hid one.
Only reordering characters count: a dropped control character changes no ending.

Not `ItemText`, on purpose:

- A `path` on a digest child or a Library folder: the exact path, for navigating. A sanitized path can name another
  file.
- `WhereFrom::Web::url`: the parser's own serialization (percent-encoded, so already free of control characters), kept
  whole for the card's copy and open, or dropped (`None`) past 2,048 characters. A middle-dropped link opens somewhere
  else.
- A web host: it passed `is_plain_host` (lowercase ASCII letters, digits, `-`, `_`, `.`, at most 253), which is
  stricter.
- `Importance::reasons`: the scorer's own words, not the item's.

## Provenance

- **The quarantine value** is `flags;time;agent;event-id`: flags and time in hex, the time in Unix seconds (verified on
  macOS 27.0.1 with a Chrome download, 2026-11-13: `0081;6af6442f;Chrome;<UUID>`). A field that doesn't parse is absent;
  a zero time is no date.
- **`kMDItemWhereFroms`** is a binary plist array of strings (Chrome: the file's address, then the page's). The decoder
  refuses anything that doesn't start with `bplist00`, so the XML plist reader is never reached with these bytes.
- **Mail** writes the sender, the subject, and a `message:` address. The first two are free text, so they're read by
  position: `Re: hello` parses as a URL with scheme `re`, which is why nothing here asks the URL parser what's text.
- **AirDrop** is told by the quarantine agent `sharingd`, with the sender's name as the one entry. The agent is
  corroborated by forensics write-ups that query `LSQuarantineAgentName = 'sharingd'` (0xmachos "Quarantine
  Introduction", 2019; DFIR.REPORT "Revisiting macOS Forensics: Quarantine Data"), checked 2026-11-13; no AirDrop record
  exists on the dev Mac (its `QuarantineEventsV2` holds Chrome, Firefox, qBittorrent, and Homebrew only). ⚠️ The
  WhereFroms shape (the sender's name as the one entry) is still unverified on a live transfer (#1185).
- **Linux**: `user.xdg.origin.url` and `user.xdg.referrer.url`, each one address as plain text (Chromium writes them;
  Firefox not verified, #1185).
- **Egress** (`redact_for_egress`): a web source leaves as `Site { host }`; a numeric address as `IpAddress` (the URL
  parser reads `http://2130706433/` as IPv4, so a bare number can't pose as a name); `data:` and `blob:` as
  `InlineData`; `file:` as `LocalFile`; Mail and AirDrop as their kind. An IDN leaves as punycode. Why host only: real
  download addresses carry attachment tokens and signed keys in the query, and document ids, private repo names, and
  share tokens in the path, and no rule can tell a token from a folder name.
- **Gotcha: parse before cutting.** `https://known.example:<10,000 characters>@other.example/` cut at any cap reads as
  `known.example`. The whole entry (bounded by the 64 KB xattr cap) goes to the parser; only the kept link is capped.

## Real type

`infer` (magic bytes, no `std` feature) plus what it lacks: a UDIF disk image's `koly` trailer, which is the file's last
512 bytes (so the host reads a tail too, `TAIL_BYTES`), a xar installer package's `xar!` header, and plain text, which
has no magic and is told by being text (`looks_like_text`: UTF-8, a character cut at the head's end allowed, or UTF-16
behind its byte-order mark; no NUL; at most 1% control bytes besides tab, line ends, form feed, and escape). A bundle is
a folder and is never sniffed.

`TypeAgreement` is `Matches`, `Mismatch`, or `CantTell`. A mismatch needs both sides confident: recognized bytes and an
extension with a claim. `agrees` is looser than equality where one format legitimately wears another's bytes (an office
document or e-book is a zip; `m4a` is an MP4; a `.js` may start with an interpreter line and a `.sh` needn't have one; a
`.so` may be Mach-O). So a `.txt`, `.md`, `.json`, or source file whose bytes are text `Matches` (the Kind row reads
"Text, as its name says"), and a `.jpg` full of text is a `Mismatch`.

- **Gotcha/Why**: without the text check, every plain text file was `CantTell`, the Kind row hedged "Text, going by its
  name", and the verdict eval's models read the projection's `bytesAre: unrecognized, agreement: cantTell` as "its
  contents don't match the extension" (`docs/notes/about-verdict-eval.md`). Regression test:
  `plain_text_is_told_by_being_text`.

## The plan

`plan(PlanInput)` reads the target, the extension, the executable bit, whether the bytes are on the disk, `VolumeCaps`,
and `OsCaps`, all known before any job. It returns the sections in display order with their rows, and the volume note.

- **`VolumeCaps` is a set of capabilities, not a volume kind.** The host maps each volume kind (and the OS) to one, so a
  new backend changes no rule here. `everything` is a local disk; `identity_only` is a phone and the default for any
  volume whose capabilities aren't mapped yet.
- **`OsCaps` is apart from `VolumeCaps`** so an OS limit never reads as a volume one: Linux has no Spotlight, no code
  signing, and no default-app lookup yet (#1186), and its disk still gets no note. The host sets all three on macOS.
  `download_records` is off on Windows, where Cmdr reads no download record: a row that could only answer `NoRecord`
  would claim there's none.
- **An online-only file** (`contents_local: false`, a cloud drive evicted it) plans no row that reads its bytes: the
  real type and the signature, and for an app or package the bundle and its app data (both open its `Info.plist`).
  Reading them would download it. Its xattrs are metadata and stay.
- **Each kind sits in exactly one section** (`rows_of`): Origin (download, Cmdr operation), Nature (identity, real type,
  bundle, signature, recursive size), Use (last opened, default app, importance), Context (owner, git, app data),
  Contents (folder digest), Removal (derived, no rows, always last).
- **Order per target**: a folder leads with Contents, an app or package with Nature, everything else with Origin.
- **Signature** is planned for bundles, packages, `dmg` / `dylib`, and files with the executable bit. A program behind
  another extension shows through the type row's mismatch. Not `pkg` / `mpkg`: an installer's signature sits in its xar
  table of contents, which the code-signing API doesn't read, and `pkgutil` answers "unsigned" and "invalid" with one
  exit code, so the row could only say Cmdr can't tell.
- **`retain_kinds`** trims a plan to the kinds a host gathers, under the same rule (an emptied section goes, Removal
  stays). For a host that answers a subset; the note is the volume's and stays.
- **The owner row** is planned only when the path can name an owner (`PlanInput::owner`, from `OwnerQuestion::of`), and
  a receipt question only on an OS that keeps receipts (`OsCaps::install_receipts`). **The git row** only when the
  host's look before the plan found a repo around the item (`PlanInput::in_a_repo`). Why: a document in `~/Documents`
  outside every repo would otherwise draw two rows that can only say "nothing", which is the wall of unknowns the card
  must not be.
- **An archive entry** has no owner, repo, or operation of its own; its download row is the archive's
  (`Download::of_archive`: always `Known`, naming the archive, with the archive's record or why there's none).
- **A `.git` portal entry** (`PlanInput::git_snapshot`) is git's record, not a file on the disk: its `GitSnapshot` row
  leads Origin, and no row of the disk's (download, Cmdr operation, owner, git status) is planned, whatever the caps.
- **A view has no note**: an archive's entry and a portal entry are views into one file or repo, so their own row says
  where they come from and no "On <volume>, Cmdr can see…" line appears.
- **The note**: none when the volume answers everything, `IdentityOnly` when it answers nothing more, `SomeFacts` in
  between. `Unreachable` is the host's to set when its health check runs out; every row that was waiting then lands
  `VolumeStoppedAnswering`, which is Routine for the same reason `VolumeCantTell` is: the note says it once.

## Removal

`consequences(&AboutCard)` is callable at any moment and never empty. The enum is declared from most to least painful
and `rank` follows it; the card shows the top two.

- **Git**: `Tracked(Modified)` → `UncommittedWork`; `Untracked` → `UntrackedNotInGit`; `Tracked(Clean)` →
  `GitCanRestore`; `Ignored` → `IgnoredByGit`, file or folder alike, which says only that git can't bring it back.
  **Decision: an ignored item is never "rebuildable" on that alone.** A project ignores its `data/` and its secrets as
  readily as its build output, so "a tool can recreate this" would be a guess dressed as a fact.
- **Folder class**: `Dependencies` → `RebuildableOutput { PackageManager }`, `BuildOutput` → `{ BuildTool }`, `Cache` →
  `RegeneratedCache`. Any of the three says more than "git ignores it", so it replaces the `IgnoredByGit` line.
- **Owner**: an uninstalled app's folder is `LeftoverOfUninstalledApp` even under `Caches` (nothing regenerates it); an
  installed app's `Caches` folder is `RegeneratedCache { app }`; its other data is `AppData` with a `Certainty`; a
  receipt or Homebrew is `InstalledBy`.
- **An app bundle** always answers `RemovesApp { data_left }`, carrying the app-data fact as it stands (pending, known,
  or why Cmdr can't tell).
- **A download** with a site → `RecordedDownload { host }`. A numeric address names no place to look again. ❌ Never for
  an archive's entry: the record is the archive's, and nothing says the site still offers that one file.
- **A repository's own data** (`FolderClass::VersionControl`, `.git`) → `HoldsRepoHistory`, ranked right after
  `UncommittedWork`: removing it leaves the files but drops every commit not pushed elsewhere, and Cmdr can't tell
  what's pushed. Said of `.svn` too, where it overstates the loss (svn's history is on the server): the line never
  understates one.
- **A `.git` portal entry** → `InGitHistory` alone: it's git's record, so no other consequence applies.
- **A symlink** → `LinkOnly`; **more than one hard link** → `OtherNamesRemain`. Without these the fallback would call a
  link the person's own irreplaceable document.
- **The fallback**: `UserDocument` only when the evidence rows the plan has (owner, git, download) all settled with an
  answer or a Routine absence; `CantTell` while one is pending, failed for a Notable reason, answered `VolumeCantTell`,
  or when the plan has none of them, and always for an archive entry. `VolumeStoppedAnswering` is no evidence either.

## The owner

`OwnerQuestion::of(path, home)` reads the path alone; `answer(&OwnerLookups)` settles it with what the host looked up.
Each claim keeps how Cmdr knows (`ByBundleId`, `ByName`, `InstalledBy`, `Homebrew`, `InsideApp`) so the card and a model
word certainty honestly. `home` is a parameter, so a fixture home never touches the real `~/Library`.

- **Certain by id**:
  `~/Library/{Containers, Application Scripts, Caches, HTTPStorages, WebKit, Saved Application State, Logs, Preferences, Application Support}/<reverse-DNS id>`
  (a `.plist`, `.savedState`, or `.binarycookies` suffix dropped), `Group Containers/{group.<id>, <TEAM>.<id>}`, and on
  Linux `~/.var/app/<id>` and `~/snap/<name>`. Anything deeper belongs to the same app. ❌ Nothing reads inside a
  container: the name is the whole answer.
- **Probable by name**: `Application Support/<Name>` and Linux's `~/.config`, `~/.local/share`, `~/.cache` children. An
  app's own name (display name, `CFBundleName`, bundle folder, desktop id; case-folded) wins; failing that a vendor's
  name in front of an app's (`Google` → Google Chrome). Several matches are `Ambiguous`, ❌ never a pick. No match is
  `LibraryNameMatchesNoApp` (Notable) in the Library and `NoInstalledAppMatches` (Routine) elsewhere.
- **By receipt**: outside home under `/Library`, `/usr/local`, `/opt`. One package id is the claim (`com.apple.*` is
  part of macOS); several all Apple's is part of macOS (Apple's own packages share folders like
  `/Library/Application Support/Apple`, verified on macOS 27.0.1 with `pkgutil --file-info`, 2026-11-13); two vendors'
  is `Ambiguous`; none is `NoRecord`.
- **By path**: a Homebrew `Cellar/<name>` or `Caskroom/<name>` under `/opt/homebrew`, `/usr/local`, or
  `/home/linuxbrew/.linuxbrew`; anything inside an `.app` bundle is that app's (the bundle itself isn't inside one).
- **Not claimed**: dot-folders in home (`~/.npm`, `~/.cargo`), plain names in Library folders other than
  `Application Support`, anything else. A curated tool table is #1179.

Resolving an id, in order:

1. An installed app whose id is the folder's or a dot-prefix of it (`ai.perplexity.mac` owns
   `ai.perplexity.mac.PerplexityRichNotification`, an extension's container), the longest; ids compare ignoring ASCII
   case, as LaunchServices does.
2. A group container's id names a team's shared data, not an app, so it also matches apps whose ids extend it; several
   are `Ambiguous`.
3. The app with that id elsewhere on this computer's own disk (`OwnerLookups::app_with_id`: a helper under `/Library`).
4. Else `NotInstalled` ("left over from an app that isn't installed"), **except** for a group id or Apple's own
   (`com.apple.*`), which are `LibraryNameMatchesNoApp`. **Decision/Why**: macOS keeps many of its services outside the
   app folders and a group id is no app's id, so calling those "left over" would invite deleting data something live
   uses. A wrong "left over" is the worst answer this row can give.

**Decision: "installed" never counts an app on another volume**, and step 3 exists because the app-folder scan alone
calls a helper app under `/Library/Application Support` "not installed". The host keeps the scan to the app folders and
filters `app_with_id` to this disk outside the Trash (`apps/desktop/src-tauri/src/about/DETAILS.md` § The owner).

## A folder's digest and an app's Library data

`digest.rs`. The host hands over index rows; every rule about them is here, and nothing here reads anything.

- **`FolderDigest::of`**: the class always, then `children` (`Children`: count, biggest, kind mix) as a nested `Fact`.
  **Decision: the class never waits on the index.** It comes from the name, so an unindexed `node_modules` still reads
  as installed packages and its removal row says a package manager can fetch it again; with the class inside an unknown
  digest it said "Nothing Cmdr knows of made this" (seen on a real `node_modules`, 2026-11-13). The `SHOWN_CHILDREN` (8)
  biggest direct children, a known size before an unknown one, ties by name so two reads never swap rows; the same cut
  for the largest files; the kind mix over every child, by name alone (`ChildKind::of_name`, no byte read across
  thousands of children), most common first.
- **`FolderClass::of(name, beside)`**: a telling name (`node_modules`, `.git`, `__pycache__`, `.next`, `DerivedData`)
  classes a folder wherever it sits. **Decision: a name a person also gives their own folders (`build`, `target`,
  `dist`, `out`, `venv`, `Pods`) counts only beside its tool's project file** (`Cargo.toml` beside `target`,
  `package.json` beside `dist`). Why: `RebuildableOutput` says "a build tool makes it again", and said about someone's
  own `build` folder of drafts it invites deleting their work. `beside` is asked only for those names.
- **`ByteShare::unchanged`**: the share of the folder's bytes in files untouched for `UNCHANGED_FOR_SECS` (a year). Over
  a lower-bound total it's `Unknown(NotIndexed)`: the untouched bytes may sit in the part nobody listed. It's `at_least`
  when the host counted only the largest untouched files, and it never exceeds the total.
- **`app_data_place`**: the owner rule run the other way. A Library child counts as the app's only when its NAME names
  the app's id or one of its own (`<id>.<extension>`), in `APP_DATA_PLACES` (Application Support, Containers, Group
  Containers, Caches, HTTPStorages, Saved Application State, Logs). ❌ Never a folder named by name alone
  (`Application Support/Slack`): what removing the app leaves is counted, not guessed. A vendor's shared group
  (`<TEAM>.com.vendor`) isn't counted for one of its apps.
- **`AppData::of`**: biggest first, the total, and `total_is_lower_bound` when a place went unlooked or a folder's size
  is partial or unknown (an unknown size never counts as zero). No place looked in at all is `Unknown(NotIndexed)`, ❌
  never "no data".

## Headline

`headline(&AboutCard)` returns `what` (by target; for a file, the sniffed family when the bytes disagree with the name,
else the extension's claim), `size` (a folder's from the index, `at_least` on a lower bound; a folder's own entry size
is never used), and `lead`, whose variant is the template key.

Lead priority: a portal entry's snapshot (`FromGitHistory`), an archive's entry's archive (`InsideArchive`: the
archive's download site is the row's to tell, never the entry's lead), a recorded download site, a Cmdr operation, Mail
or AirDrop (never naming the person), an owning app (its cache as `CacheOfApp`, so it agrees with the removal row) or
that the app is gone, a downloading app alone, last opened, `Plain`. It never carries a removal consequence. A plain
document's headline is kind, size, and last opened, from identity and use alone.

## Next actions

`actions(&AboutCard, &ActionCaps)` returns the footer: `shown` (at most `SHOWN_ACTIONS`, 3) and `more`, each in one
fixed order per target, recomputed with every landing like the headline. The facts say where there's somewhere to go;
`ActionCaps` (the host's, per way of reaching the item) says what the volume lets an action do. Nothing a model writes
reaches it.

- **Order**: a file or package: open, show in the file manager, the download page, the owning app, the app's data
  folder. A folder: show, owner, app folder. An app: show, the download page. A symlink: show, owner, app folder. An
  archive's entry: the archive's download page only. Then `ContinueInAskCmdr`, every target, only while the host says
  Ask Cmdr is on (`ActionCaps::ask_cmdr`). Removal (`MoveToTrash`, or `Delete` on a volume with no Trash) always last.
- **Decision: a file whose bytes and name disagree never leads with Open** (`TypeAgreement::Mismatch`, a Mac program
  named `.jpg`): Open heads "More", and while the type row is still pending Open isn't offered at all, so it never shows
  first and then moves. Why: the disguise is what the type row exists to catch, and opening it shouldn't be the leading
  suggestion (lead's review of the fixture cards, 2026-11-13).
- **Decision: removal is never first, and alone it sits behind "More"** (a phone's file: nothing but Delete). Why: the
  first button is where a hurried press lands, and the deterministic removal row above it is what the person should read
  first. It goes through the app's delete dialog, which asks.
- **Decision: three shown, not four.** At the card's default width, three labels ("Open", "Show in Finder", "Download
  page") plus "More" and Close fill the footer; a fourth wrapped it (QA screenshots, 2026-11-13). On a typical download,
  Move to trash is the one behind "More", which also keeps it furthest from the hand.
- **The download page is the record's LAST web address**: Chrome and Safari write the file's own address first and the
  page after it, and opening the file's address only downloads it again.
- **`web_page` is the only way an address becomes an action**: re-parsed by `url`, `http` or `https` only, a plain
  lowercase DNS host (punycode for an IDN, so a look-alike shows as `xn--…` on the button), no user info (a link with
  `apple.com@evil.example` is a disguise), no IP literal (it names no site the person could recognize). Written back out
  by the parser. ❌ Never a `file:`, `smb:` (it mounts a stranger's share), `javascript:`, or an app's own scheme (it
  launches whatever registered it). The card names the site before the press: in the button's tooltip (hover and
  keyboard focus), and in the download row above it.
- **The owning app is revealed only where Cmdr knows its bundle's path** (`InstalledApp::Installed.path`,
  `OwnerClaim::ByName.app_path`, `InsideApp.app_path`: raw paths, never shown). A Linux desktop entry has none, so a
  Linux owner gets no reveal.
- **The app's data folder** is `OwnerQuestion::app_folder_above(path, home)`: the owned Library or XDG folder the item
  sits strictly inside, so a file deep in `Application Support/Slack` offers "Go to Slack's data". The folder itself
  gets none (it's already there).

## The model's projection

`model::for_model(card, path, protected, speak)` (`model.rs`) is everything a model may read about an item: the
verdict's prompt and the agent tool's result both go through it, and the app's one caller is `about::model::for_model`,
which answers the sensitive gate and the words. Each row keeps its coverage
(`{ "state": "unknown", "value": "timedOut" }`); the card's prominence stays home. Numbers carry `*Human` twins with any
uncertainty inside the string (`≥ 2.3 GB` over a lower bound), as every tool result does
(`apps/desktop/src-tauri/src/tools/DETAILS.md` § Numbers arrive already spoken). The removal row comes along, derived
from the projected card, so a model restates Cmdr's consequences instead of inventing its own. **Decision: it's the
first field after the target and path**, before the rows: on a real-model try (2026-11-13) "what is this folder?" came
back without what removing it would cost, with `removal` last, behind a folder's long child lists. A field's place in
the JSON is the cheapest nudge there is; the agent tool's description asks for it too.

- **Kept**: the target, the path (through `model_text`) and every `ItemText` as is: name, sizes, dates, hard links, a
  link's target; the real type (Cmdr's tokens plus the extension); bundle id, name, version; signer with team, ticket,
  and whether the executable was checked; download time and app, and the archive a record is from; Cmdr's operation with
  its source path and volume name; last opened and use count; the default app; importance band, staleness, the scorer's
  reasons, and visits; the owner claim and its how; git root, branch, status; a folder's size, class, children and
  largest files by name and size, kind mix, unchanged share; an app's Library folders by name, place, and size; a portal
  entry's snapshot.
- **Redacted**: each download source to `redact_for_egress` (a host, or a kind with no text).
- **Dropped**: every raw `path` kept for navigating (an owner's app bundle, a digest child, a largest file, a Library
  folder: the largest files' paths would map a folder's whole subtree), and the prominence.
- **A protected path** (`protected: Some`): identity, the owner claim (read from the path), and a folder's whole size
  stay; every other planned row answers `Unknown(Protected)`, and the removal row is derived from that trimmed card, so
  it can't carry the download host the row dropped. `contentsStayHome` names the reason; `unresolved` is the host's gate
  failing closed (it couldn't follow the path's links in time).

How a silent widening is made impossible (each one a compile error, not a review):

- Every payload is destructured with no `..`, so a new field on a fact must be kept, redacted, or dropped here.
- The output types are this module's own; payload types ride along only when they're tokens, and `tokens_are_fieldless`
  casts each to `u8`, which compiles only for an enum with no data. `EgressSource` is the one exception: it IS the
  redaction.
- `every_field_that_leaves_names_its_disclosure_item` destructures `ModelCard` with no `..` and maps every field to an
  `AboutDisclosure`, and every token to some field. The consent copy is a `Record<AboutDisclosure, MessageKey>`
  (`apps/desktop/src/lib/ai/about-this-disclosure.ts`), so a new token fails the frontend's typecheck until it has a
  line, and a new line is a `CLOUD_AI_CONSENT_VERSION` bump (`apps/desktop/src-tauri/src/ai/DETAILS.md` § Cloud AI
  consent).

## The gathering engine

The shape of `apps/desktop/src-tauri/src/menu/context_menu_facts.rs`, generic: jobs are `(key, FnOnce() -> answer)`, the
host injects `submit` (its framework pool), `collect(deadline, late)` returns what answered in time plus the keys still
out, and each later answer goes to `late` on the thread that produced it. The switch from `ready` to `late` happens
under the lock jobs deliver under, so an answer lands in exactly one place
(`no_answer_is_lost_or_delivered_twice_around_the_deadline`). `CancelGathering` makes queued jobs skip their work and
drops late answers. The menu keeps its own copy; unifying the two is #1178, not a reason to touch the menu.

A worker stuck in a framework call is lost, never interrupted: a deadline bounds the collector's wait, and the host
pool's ceiling bounds the threads a dead mount can hold.

## Fuzzing

`fuzzing::provenance` runs every provenance decoder over the input, then over a well-formed binary plist written from
the input's lines. Seven assertions, each with a mutant on record. `the_properties_hold_for_the_shapes_the_seeds_carry`
(a unit test) runs the seam over the same shapes inline, so a broken property is red in `cargo nextest` before any fuzz
run. It builds them in code and never reads `fuzz/seeds/`: an out-of-crate read would need a scope rule, an input glob,
and two workflow filters to stay honest.

## Dependencies

- `infer` 0.22.0 (checked 2026-11-13: released 2026-07-15, repo live), `default-features = false`: `std` adds file
  helpers and the `cfb` reader, neither needed for a buffer.
- `plist` 1.10.3, `default-features = false` (no `serde` bridge needed): the app and `cmdr-fs` already resolve it.
- `url` 2: the app already resolves it. Its host parsing is what makes the egress rule cheap to state.
- `specta` pinned to the app's exact version.

## The public surface is capped

One root promise per concern, each a public module, and the items in them, all capped
(`scripts/check/checks/index-crate-isolation-ceilings.go`, which holds the current numbers). The items are the wire
types the card's events carry and the handful of functions a host calls: `plan`, `AboutCard`, `download_fact`,
`xdg_download_fact`, `usage_fact`, `redact_for_egress`, `real_type`, `consequences`, `headline`, `start_with`,
`model_text`, `for_model`. The decoders' pieces (`parse_quarantine`, `decode_where_froms`, `classify_sources`, `sniff`)
are `pub(crate)`: a host gets whole facts, never half-decoded records.

A new public item is a new thing every surface of the card can say (the dialog, the agent tool, the model's projection,
the consent copy that lists what leaves). Argue it here before raising the ceiling.

Raised 110 → 113 by the host's first milestone, none of them wire types:

- **`usage_fact`**: Spotlight stamps `kMDItemLastUsedDate` on a file an app WRITES (a file `echo` just wrote has one
  equal to its creation, macOS 27.0.1, `mdls`, 2026-11-13), so a date on the last change with no use count is the save,
  read as `NoRecord`. Which Spotlight record counts as "last opened" is a rule a person could argue, so it lives here.

- **`OsCaps`**: what the OS keeps about any item. Folding it into `VolumeCaps` would make a Linux disk's card open with
  "On this disk, Cmdr can see…", a volume limit that isn't one; a host-side filter would move a rule (which rows an OS
  can answer) out of the crate. No constructor: the host spells its three fields out.
- **`AboutPlan::retain_kinds`**: a host that gathers a subset of the kinds trims the plan under the plan's own rule
  (emptied sections go, Removal stays), instead of rebuilding sections and restating that rule.

Raised 113 → 119 by the owner and importance rows:

- **`OwnerQuestion` with `of` and `answer`, `KnownApp`, `OwnerLookups`**: which Library folder names an app by id, which
  by name, when a vendor's name is ambiguous, and when an unmatched id may be called "left over" are rules a person
  could argue (§ The owner), so they live here as two pure steps; the host only answers the lookups it's asked for. The
  lookups are closures so a Homebrew path never costs an app scan, and the question is a type of its own so the plan can
  leave out an owner row nothing could answer.
- **`ImportanceBand::of_score`**: where High and Medium start is a rule a person could argue; the host passes the stored
  score.

Raised 119 → 120:

- **`Signature::read`**: which signatures a notarization ticket is news on (Developer ID and unrecognized ones: Apple's
  own software and the Mac App Store's aren't notarized, unsigned or ad hoc code can't be) is a rule a person could
  argue, so the host builds every signature through it and the card words `DoesNotApply` as nothing.

Raised 120 → 129 by a folder's digest and an app's Library data, each a rule a person could argue that would otherwise
live in the host, plus one wire type:

- **`Children`**: what the index listed directly inside, as its own nested `Fact`, so the class stands when the listing
  doesn't (`node_modules` the index never reached is still installed packages, and its removal row says so).

- **`FolderDigest::of`**: which children lead (a known size before an unknown one, then by name) and how many show.
- **`FolderClass::of`**: which names make a folder dependencies, build output, a cache, or version control, and which
  count only beside their tool's project file.
- **`ByteShare::unchanged`**: when an untouched share is honest (never over a lower-bound total) and when it's "at
  least".
- **`AppData::of`**: when an app's data is "at least" and when Cmdr can't tell at all.
- **`app_data_place` and `APP_DATA_PLACES`**: which Library folders are an app's, the owner rule run the other way.
- **`SHOWN_CHILDREN` and `UNCHANGED_FOR_SECS`**: how many the digest names and what "untouched" means, so the host asks
  the search core for exactly that. `ChildKind::of_name` stays inside the crate: only the digest reads it.

Raised 129 → 132 by items Cmdr reaches through their volume:

- **`GitSnapshot` and `SnapshotKind`**: a wire type, the row of a `.git` portal entry (which branch, tag, commit, or
  stash entry the path names, and when a commit last changed it). Without it a file in `.git/commits/<id>/` read as a
  plain file on the disk, and its removal row as the person's own document.
- **`Download::of_archive`**: an archive's entry always names its archive, with the archive's record or why there's
  none. Whether an entry's row may say "nothing recorded" or must name the archive is a rule a person could argue.

Raised 132 → 139 (and 11 → 12 modules and root promises) by the card's next actions (§ Next actions):

- **The `actions` module: `AboutAction` and `Actions`** (wire types: the footer the card draws), **`ActionCaps` with
  `none`, and `RemoveBy`** (what the host knows about the volume before any job), and **`actions`**. Which actions show,
  in what order, which wait behind "More", and which addresses may be opened are rules a person could argue, and the
  footer must never depend on anything a model writes, so they live here, beside the removal row they sit under. A
  module of its own: `card` must not import the modules that read it, as with `removal` and `headline`.
- **`OwnerQuestion::app_folder_above`**: which folder is "the app's data" is the owner rule's own geometry.
- `web_page` and `SHOWN_ACTIONS` stay `pub(crate)`: a host gets whole actions, never a half-checked address.

Raised 139 → 140:

- **`RecursiveSize::into_fact`**: when a folder's size is worth stating. A lower bound with nothing counted (no file,
  folder, or byte) is the index not having listed it yet, so it's `NotIndexed`; stated, it read "at least 0 bytes in 0
  files and 0 folders", which on an empty folder is wrong (QA screenshots, 2026-11-13).

Raised 12 → 13 modules and root promises, 140 → 146 items, by the egress line (§ The model's projection):

- **The `model` module: `ModelCard` and `model::for_model`**: what may leave the machine about an item is the rule a
  person would argue hardest, and both senders must share it, so it's one function here, beside the facts it reads.
  `ModelCard` has private fields: a host can serialize it and do nothing else with it.
- **`ProtectedReason`**: the app's sensitive-gate answer in this crate's words (the gate itself is the app's), so the
  name-level-only rule lives here with the projection it trims.
- **`Speak` and `SpokenSize`**: the host's size and date words, so a number reads the same as in every other tool result
  and the crate needs no zone or unit table. A struct, not two positional flags that mean opposite things.
- **`AboutDisclosure`**: a wire token per consent line, exported to `bindings.ts` so the frontend's disclosure is a
  `Record` over it.

Raised 146 → 147 items:

- **`text::hides_reordering`**: whether a raw name holds a direction override (the characters `model_text` drops for
  reordering text), which the host sets on `Identity::name_hid_reordering`. Which characters reorder text is the
  sanitizer's own rule; a host copy of the set would drift from what `model_text` drops, and the card would stop saying
  why a name's ending changed.
