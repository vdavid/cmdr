# cmdr-about details

The fact card behind "Tell me about this" (#1104): where an item came from, what it really is, who owns it, how it's
used, and what removing it costs. The card works with AI off; a model, when there is one, adds a few sentences beside
these facts and decides none of them.

## Layering

Modules import downward only, which is what keeps `module-cycles` green:

- `text` (no imports) → `fact` → `payload`, `owner`, `provenance`, `sniff` → `digest` → `card` → `removal`, `headline`.
- `gather` imports nothing from the crate: it's generic over the job key and the answer.
- `removal::consequences` and `headline::headline` are free functions over `&AboutCard`, never methods on it, because
  `card` must not import the modules that read it.

## Coverage: `Fact<T>`

- `Known(T)`, `Unknown(Unknown)`, `NotApplicable(NotApplicableWhy)`, `Pending`. On the wire:
  `{ "state": "unknown", "value": { "why": "noRecord", "prominence": "routine" } }`.
- **`Unknown` is built only from an `UnknownWhy`** (`Fact::unknown`), so the prominence on the wire is always the one
  the rule gives. Routine: `NoRecord`, `NotInStatus`, `NoInstalledAppMatches`, `VolumeCantTell` (the card's volume note
  says that one once). Everything else is Notable. `only_ordinary_absences_are_routine` pins the list.
- **Decision: a nested `Fact` inside a payload** where one part can be unknown while the rest is known:
  `GitContext::status` (the root and branch are cheap, the status can be a whole-worktree walk that times out),
  `Download::sources` (a quarantine record without a readable source list), `FolderDigest::largest_files`, and each
  child's size. Why: the alternative is a kind per sub-answer, which splits one job and one row in two.
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
  WhereFroms shape (the sender's name as the one entry) is still unverified on a live transfer.
- **Linux**: `user.xdg.origin.url` and `user.xdg.referrer.url`, each one address as plain text (Chromium writes them;
  Firefox not verified).
- **Egress** (`redact_for_egress`): a web source leaves as `Site { host }`; a numeric address as `IpAddress` (the URL
  parser reads `http://2130706433/` as IPv4, so a bare number can't pose as a name); `data:` and `blob:` as
  `InlineData`; `file:` as `LocalFile`; Mail and AirDrop as their kind. An IDN leaves as punycode. Why host only: real
  download addresses carry attachment tokens and signed keys in the query, and document ids, private repo names, and
  share tokens in the path, and no rule can tell a token from a folder name.
- **Gotcha: parse before cutting.** `https://known.example:<10,000 characters>@other.example/` cut at any cap reads as
  `known.example`. The whole entry (bounded by the 64 KB xattr cap) goes to the parser; only the kept link is capped.

## Real type

`infer` (magic bytes, no `std` feature) plus the two formats it lacks: a UDIF disk image's `koly` trailer, which is the
file's last 512 bytes (so the host reads a tail too, `TAIL_BYTES`), and a xar installer package's `xar!` header. A
bundle is a folder and is never sniffed.

`TypeAgreement` is `Matches`, `Mismatch`, or `CantTell`. A mismatch needs both sides confident: recognized bytes and an
extension with a claim. `agrees` is looser than equality where one format legitimately wears another's bytes (an office
document or e-book is a zip; `m4a` is an MP4; a `.js` may start with an interpreter line; a `.so` may be Mach-O). Plain
text has no magic, so a `.jpg` full of text is `CantTell`, never a mismatch.

## The plan

`plan(PlanInput)` reads the target, the extension, the executable bit, whether the bytes are on the disk, `VolumeCaps`,
and `OsCaps`, all known before any job. It returns the sections in display order with their rows, and the volume note.

- **`VolumeCaps` is a set of capabilities, not a volume kind.** The host maps each volume kind (and the OS) to one, so a
  new backend changes no rule here. `everything` is a local disk; `identity_only` is a phone and the default for any
  volume whose capabilities aren't mapped yet.
- **`OsCaps` is apart from `VolumeCaps`** so an OS limit never reads as a volume one: Linux has no Spotlight, no code
  signing, and (for now) no default-app lookup, and its disk still gets no note. The host sets all three on macOS.
- **An online-only file** (`contents_local: false`, a cloud drive evicted it) plans no row that reads its bytes: the
  real type and the signature. Reading them would download it. Its xattrs are metadata and stay.
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
- **An archive entry** has no owner, repo, or operation of its own; its download row is the archive's.
- **The note**: none when the volume answers everything, `IdentityOnly` when it answers nothing more, `SomeFacts` in
  between. `Unreachable` is the host's to set when its health check runs out.

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
- **A download** with a site → `RecordedDownload { host }`. A numeric address names no place to look again.
- **A symlink** → `LinkOnly`; **more than one hard link** → `OtherNamesRemain`. Without these the fallback would call a
  link the person's own irreplaceable document.
- **The fallback**: `UserDocument` only when the evidence rows the plan has (owner, git, download) all settled with an
  answer or a Routine absence; `CantTell` while one is pending, failed for a Notable reason, answered `VolumeCantTell`,
  or when the plan has none of them, and always for an archive entry.

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
  `Application Support`, anything else. A curated tool table is a follow-up.

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

## Headline

`headline(&AboutCard)` returns `what` (by target; for a file, the sniffed family when the bytes disagree with the name,
else the extension's claim), `size` (a folder's from the index, `at_least` on a lower bound; a folder's own entry size
is never used), and `lead`, whose variant is the template key.

Lead priority: a recorded download site, a Cmdr operation, Mail or AirDrop (never naming the person), an owning app or
that the app is gone, a downloading app alone, last opened, `Plain`. It never carries a removal consequence. A plain
document's headline is kind, size, and last opened, from identity and use alone.

## The gathering engine

The shape of `apps/desktop/src-tauri/src/menu/context_menu_facts.rs`, generic: jobs are `(key, FnOnce() -> answer)`, the
host injects `submit` (its framework pool), `collect(deadline, late)` returns what answered in time plus the keys still
out, and each later answer goes to `late` on the thread that produced it. The switch from `ready` to `late` happens
under the lock jobs deliver under, so an answer lands in exactly one place
(`no_answer_is_lost_or_delivered_twice_around_the_deadline`). `CancelGathering` makes queued jobs skip their work and
drops late answers. The menu keeps its own copy; unifying the two is a follow-up, not a reason to touch the menu.

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

11 root promises (one module per concern), 11 public modules, 120 items in them
(`scripts/check/checks/index-crate-isolation-ceilings.go`). The items are the wire types the card's events and the
model's projection carry, and the handful of functions a host calls: `plan`, `AboutCard`, `download_fact`,
`xdg_download_fact`, `usage_fact`, `redact_for_egress`, `real_type`, `consequences`, `headline`, `start_with`,
`model_text`. The decoders' pieces (`parse_quarantine`, `decode_where_froms`, `classify_sources`, `sniff`) are
`pub(crate)`: a host gets whole facts, never half-decoded records.

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
