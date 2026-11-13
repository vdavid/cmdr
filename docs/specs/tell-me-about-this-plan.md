# Tell me about this – execution plan

Just-in-time execution plan for [#1104](https://github.com/getcmdr/cmdr/issues/1104): a **fact card** for any file or
folder (where it came from, what it really is, what owns it, how it's used, what removing it costs), with a **short AI
verdict** streamed in on top of the facts. David's direction (2026-11-11): the thick deterministic part works with AI
off; the LLM adds two or three sentences. Built by sequential agents, one per milestone, in the single worktree
`.claude/worktrees/tell-me-about-this`; the lead lands the whole feature as one piece at the end.

Read the issue with its comments first. Then this plan in full. Every milestone agent also reads the `CLAUDE.md` (and
the `DETAILS.md` where the plan points) of each directory it touches.

## Loud rules (read before touching anything)

- **The card is AI-free at its core.** Every fact row comes from deterministic code and works with `ai.provider = off`,
  on Intel, on Linux, offline. The verdict is an optional block below the facts; ❌ no fact may depend on an LLM, and no
  fact row waits for the verdict.
- **Every fact voices its coverage, typed.** A fact is `Known(T)`, `Unknown(why)`, `NotApplicable(why)`, or still
  `Pending`. `why` is a typed enum (`NotIndexed`, `NoRecord`, `VolumeCantTell`, `TimedOut`, `Unreachable`,
  `PermissionDenied`, `Protected`, …), ❌ never an empty string, a zero, or `false` standing in for "don't know". "macOS
  has no record of opening this" (`NoRecord`) is NOT "never opened". This mirrors `tools/CLAUDE.md`'s "Every result
  voices its coverage" and `tools/DETAILS.md` § The honesty (coverage) contract.
- **Nothing slow on the main thread or an IPC handler thread.** Every slow fact is a job on a bounded pool of 8 MB-stack
  OS threads (`crate::file_system::framework_pool`, the `menu/context_menu_facts.rs` model), ❌ never rayon for
  framework calls (`src-tauri/CLAUDE.md`). Each job has its own budget; the card's request has a cancel that closing the
  card fires; a late answer after cancel is dropped. Each fact lands by itself, in any order.
- **Degrade, never block.** A dead mount, a slow SMB share, an MTP phone, an archive entry, and Linux each answer the
  facts they can; the rest are `Unknown`/`NotApplicable` with a typed reason. ❌ No syscall against an SMB or MTP mount
  beyond what its volume answers through Cmdr's own deadline-wrapped paths; index, importance, and op-log facts read
  their databases only.
- **The removal line speaks in consequences and never gives a green light.** It's derived deterministically
  (`RemovalConsequence`, M1) and shown with AI off. ❌ No surface, deterministic or AI, says "safe to delete", "you can
  delete this", "not needed", or any equivalent. The verdict may restate consequences the facts carry; it may not invent
  reassurance. "Cmdr can't tell what depends on this" is a valid, honest line.
- **One egress projection.** Whatever leaves the machine (the verdict prompt AND the agent tool's result) goes through
  ONE function (`AboutCard::for_model`, M7). It drops URL query strings, fragments, and userinfo (spike: real
  `kMDItemWhereFroms` values carry Gmail attachment tokens and signed CDN keys in the query), and contents ride ONLY
  through the `inspect_file` core and its sensitive gate (`tools/sensitive.rs`). ❌ No second serializer of facts for a
  model.
- **The egress line widens exactly once, with a copy change and a version bump in the same commit.** Download sources,
  owning-app and code-signature data, and git status are new kinds on the line. The milestone that first sends them (M7)
  adds the disclosure item and bumps `CLOUD_AI_CONSENT_VERSION` (today `2`, in
  `apps/desktop/src-tauri/src/ai/cloud_consent.rs`) to `3`, and updates `docs/security.md` § Cloud AI egress and the
  egress must-know in `agent/CLAUDE.md`.
- **Every LLM call resolves through the existing chokepoint.** The verdict uses
  `agent::chat::session::resolve_agent_llm` (managed policy, cloud consent, the E2E fake) and `AgentLlm::respond` with
  no tools. ❌ Never build an `AiBackend` around it, never call `genai` directly, never skip the cost meter.
- **Never open anything inside another app's container.** `~/Library/Containers/<id>` and
  `~/Library/Group Containers/<id>` are classified by their NAME only. Reading inside them can raise macOS's "would like
  to access data from other apps" prompt (macOS 14+). The owner mapping reads names, LaunchServices, and receipts,
  nothing more.
- **The fact core is tauri-free.** It lives in a new `crates/cmdr-about` crate (vocabulary, pure derivations, the
  gathering engine); the app implements the jobs. ❌ No `tauri`, `tauri-specta`, or `cmdr` in its tree
  (`index-crate-isolation`).
- **Human-facing copy is a draft.** Every label, row, empty state, and the consent item is English with a good `@key`
  description, ❌ never translated (translators come later). David reviews layout and copy before release: list each new
  string in the milestone's report.
- **Strip milestone tags** (`M1`, "Milestone 3") from code, comments, test names, `reason` strings, and docs before
  reporting. The plan keeps its own.

## The current code (fresh look, 2026-11-13)

What already exists and what each milestone reuses. Symbols named here are for `codegraph_explore`; don't re-derive
them.

- **Slow-fact model**: `menu/context_menu_facts.rs` (`FactKind`, `Fact`, `Gathering`, `start_with`, `GRACE` = 100 ms,
  `CancelGathering`, a `LateSink`) plus `context_menu_live.rs`. Its core is already AppKit-free and unit-tested with
  plain threads. The new crate's engine copies its shape, generic over the fact type; ❌ don't refactor the menu onto it
  in this effort (a follow-up issue may unify them).
- **Default app / installed apps**: `file_system/open_with.rs` (`installed_app_path(bundle_id)`,
  `read_app_display_name`, `read_bundle_identifier`, `app_icon_data_url`, an invalidation observer). `default_app.rs`
  launches; the card only reads.
- **Packages and bundles**: `file_system/packages.rs` (`NSURLIsPackageKey` beyond the suffix list in
  `cmdr_fs::file_extension`).
- **Content sniff**: `crates/cmdr-viewer/src/content_kind.rs` (`classify_viewer_content`, `looks_binary`, `media_mime`)
  is coarse (text / image / PDF / binary). No magic-byte library is in the tree; `mime_guess` is extension-based. M1
  adds `infer` (checked 2026-11-13: 0.22.0, 2026-07-15, ~37M recent downloads, repo live, not archived; re-check at add
  time per `AGENTS.md` § Dependencies).
- **Spotlight**: `crates/cmdr-index/src/importance/last_used.rs` samples `kMDItemLastUsedDate` on a dedicated thread
  with an autoreleasepool. Spike: `mdls` on `/System/Applications/Calculator.app` gives a date; on a just-downloaded
  file and on a repo file it's `(null)`. So `NoRecord`, never "never".
- **Provenance xattrs** (spike, macOS 27.0.1): a Chrome download carries `com.apple.metadata:kMDItemWhereFroms` (binary
  plist array: [file URL, page URL]) and `com.apple.quarantine` = `0081;6af6442f;Chrome;<UUID>` (flags hex; Unix seconds
  hex = the download time; agent name = the downloading app). `kMDItemDownloadedDate` was `(null)`. Read the xattrs
  directly (works without Spotlight and on external disks); `cmdr-fs` already has `xattr` and `plist`. Linux:
  `user.xdg.origin.url` / `user.xdg.referrer.url` (Chromium writes them; verify Firefox before claiming it).
- **Code signature** (spike): `codesign -dv` on `/Applications/Slack.app` answers in about 30 ms without validation
  (identifier, Developer ID authority chain, team id, stapled notarization); an MP4 is "not signed at all". In-process:
  `SecStaticCodeCreateWithPath` + `SecCodeCopySigningInformation` via the `security-framework(-sys)` deps the app
  already has. ❌ No deep validation (`SecStaticCodeCheckValidity` hashes the whole bundle: slow and not what the card
  claims). A `.pkg`'s signature is a xar signature (`pkgutil --check-signature`, a subprocess through
  `crate::subprocess` under a deadline).
- **Installer receipts** (spike): `pkgutil --file-info /Library/Application Support/Apple` →
  `pkgid: com.apple.files.data-template` in ~20 ms; a file no receipt owns answers volume and path only.
- **Owning app** (spike over a real `~/Library/Application Support`): a mix of bundle-ID folders (`com.eltima.cmd1`,
  `com.mitchellh.ghostty`, `com.openai.chat`) and display-name folders (`Slack`, `Arc`, `Firefox`, `Anki2`, `Google`,
  `Microsoft`). `~/Library/Containers` names are bundle ids (`ai.perplexity.mac`); Group Containers are `TEAMID.id` or
  `group.id`. `mdfind kMDItemCFBundleIdentifier == 'com.eltima.cmd1'` → `/Applications/Commander One.app`. See M3 §
  Owner mapping for what's honest.
- **Index**: `Index::list_children`, `dir_stats`, `dir_stats_batch` (`crates/cmdr-index/src/indexing/handle/mod.rs`).
  `entries` stores name, sizes, mtime, inode (indexed `idx_inode`), NO content hash and no size index. The app's
  coverage builder is `tools/read/listing/source.rs::coverage` (with `IndexStatusResponse::walk_affects` for
  `sizeIsUpdating`).
- **Importance**: `ImportanceIndex` (`importance/read/`), `classify.rs` (`PathClass`, `floors_by_path`: caches, build
  output, `node_modules`, hidden/system), visits in `visits-{id}.db`. The agent's reads are `tools/read/importance.rs`
  over `tools::resources::importance::snapshot_*`, with store-path translation via `crate::location::store_path`.
- **Operation log**: `crates/cmdr-operation-log/src/query.rs` searches by SOURCE name (`source_name_folded`), not by
  where an item landed. `operation_items` has `dest_dir_id`, `dest_name_folded` (indexed), and since v4 `dest_inode` +
  `dest_birth_ns`, "which node an item landed as". That's what makes an honest provenance query possible (M3).
- **Git**: `crates/cmdr-git` (`RepoCache`, the cached `status.rs` walk) via the app's `file_system/git/wiring.rs`. Every
  `gix` call on `spawn_blocking`; the surface is capped (11 root promises, no headroom): ask before a new `pub`.
- **Locations**: `crate::location::resolve` (`LocationInput`, `LocationMiss`); a miss is a typed answer.
- **`inspect_file` core**: `tools/read/inspect/mod.rs::inspect_path_with(path, &TextAsk, cancel, extract)`, with
  `path_gate` first and the private-key prescan, archives, PDFs, materialized remote files, and the runner's
  `PATH_TIMEOUT`.
- **LLM seam**: `AgentLlm::respond(system, tools, messages, CallSettings, cancel)` is one cold streaming call
  (`agent/llm/mod.rs`); `resolve_agent_llm(app, AgentSlot)` (`agent/chat/session.rs`) returns the fake under
  `CMDR_E2E_ASK_CMDR_FAKE=1`, else a genai backend through `ai::manager::resolve_backend_with_model`. It does NOT check
  `askCmdr.enabled` (`admit_send` does that for the rail).
- **Cost meter**: `agent/store/query.rs::record_cost`, keyed `(day, conversation_id, provider, model)` with
  `conversation_id NOT NULL`; `quiet_wakes_conversation` is the reserved-row precedent (migration v8,
  `ConversationOrigin::QuietWakes`). The wake loop's daily ceiling filters by origin, so a new origin stays out of it.
  Folder suggestions and translation are NOT metered today; the verdict will be.
- **Commands**: `file.getInfo` (⌘I on macOS, `Alt+Enter` elsewhere) opens the OS panel. Registry data in
  `apps/desktop/src/lib/commands/sources/file-list.ts`; native rows in `crates/cmdr-menu/src/context/file.rs` and
  `bar.rs`, ids in `ids.rs`; `shortcuts-store.ts` `menuCommands`. `⌘⇧I` and `Ctrl+Shift+I` are free in the default
  keymap (`crates/cmdr-menu/src/registry_default_keys.tsv`); `⌘⌥I` is the multi-rename dialog's Match case.
- **Ask Cmdr frontend**: `ask-cmdr-trigger.svelte.ts` exports `openRail`, `newChat`, `addAttachments(refs)`,
  `requestComposerFocus`. Attachments cross as path + kind only.
- **Consent copy**: `messages/en/ai.json` `ai.cloudConsent.*`, the Ask Cmdr items under
  `ai.cloudConsent.askCmdr.item.*`.
- **Feature status**: `feature-status.json` id `ai-tell-me` (`planned`, "Right-click any file for a quick AI
  explanation"), `apps/website/src/lib/roadmap.ts` ("AI-powered "Tell me about this"", not done),
  `apps/website/src/lib/features-page.ts`. Schema and consumers: `docs/feature-status.md`.

## Target shape

```
crates/cmdr-about/                 tauri-free: vocabulary, pure derivations, the gathering engine
  fact.rs        Fact<T>, UnknownWhy, NotApplicableWhy, FactKind, AboutTarget
  card.rs        AboutCard (one Option<Fact<…>> slot per kind), merge(), for_model() (M7)
  gather.rs      Gathering<F>, start_with(jobs, submit), collect(grace), late sink, cancel
  provenance.rs  quarantine parse, WhereFroms bplist decode, xdg origin, URL egress redaction
  sniff.rs       magic-byte kind over `infer`, extension-vs-content mismatch
  owner.rs       Library-location + bundle-id-name classifier (pure), Homebrew paths
  removal.rs     RemovalConsequence from the facts (pure)
  digest.rs      folder digest shaping from child rows (pure)
apps/desktop/src-tauri/src/about/  the host: jobs per fact, the IPC commands, the verdict
apps/desktop/src/lib/about-this/   the card dialog, its state slice, the verdict block
```

`AboutRequest { path, volume_id, is_directory }` → `start` returns a request id and streams `AboutEvent`s over a
`tauri::ipc::Channel`: one `Fact { kind, value }` per landing, then `FactsSettled`, then verdict deltas (`VerdictDelta`,
`VerdictEnd { usage, priced }`, `VerdictUnavailable(why)`). An explicit `about_cancel(requestId)` stops both the facts
and the verdict; ❌ never rely on a dropped `Channel` (`ai/CLAUDE.md`: `send` succeeds after the JS handler is gone).
The card renders what's there on its first frame (the 100 ms grace, as the menu does), then fills in.

## Milestones

Sizing: each fits one agent's context and leaves the tree green and committable (`AGENTS.md` § Checker script cadence:
`pnpm check --fast && pnpm check svelte-tests-related`, plus `pnpm check clippy` and the touched `cargo nextest` tests
when Rust changed). Model: Opus throughout; ★ marks the ones where a stronger model pays off, with why.

---

### M1 ★ – The fact vocabulary and gathering engine (`crates/cmdr-about`)

**Why ★**: this is the seam every later milestone, the UI, the tool, and the verdict speak through. A wrong shape here
(an untyped unknown, a bool for a tri-state, a fact that can't say "pending") leaks into IPC, the bindings, and the
model's input.

**Scope**

1. New crate per `docs/guides/adding-a-crate.md`, all seven steps: workspace member, `guardedIndexCrates` plus a
   `surfaceGuardedCrates` ceiling at exactly what it exposes, `module-cycles`, `cargo deny`, `C.md` + `D.md`, the
   `AGENTS.md` crate list and `docs/architecture.md` § Workspace crates. Not linked by `cmdr-listing-wasm`.
2. `fact.rs`: `Fact<T>` (`Known`, `Unknown(UnknownWhy)`, `NotApplicable(NotApplicableWhy)`, `Pending`), the typed reason
   enums, `FactKind` (one per row below), `AboutTarget` (`File`, `Folder`, `AppBundle`, `Package`, `Symlink`,
   `ArchiveEntry`). `serde` + `specta::Type` (pin `specta` identical to the app's, as `cmdr-index` does).
3. `card.rs`: `AboutCard` with one slot per kind, `apply(Fact)`; the per-kind payload types:
   - identity: name, kind label inputs, logical/physical size, created/modified, hard-link count (`nlink > 1` is an
     honest "other names point here"), symlink target.
   - real type: `SniffedKind` + `ExtensionClaim` + `mismatch: bool` (a `.jpg` that's a ZIP shows).
   - bundle/package: bundle id, display name, version (from `Info.plist`), package-ness.
   - signature: `Unsigned`, `AdHoc`, `DeveloperId { team, org }`, `AppStore`, `Apple`, `Unknown`; notarization stapled
     or not. Wording says "signed by", never "verified" (no deep validation).
   - provenance: download source (file URL + page URL, raw), downloaded-at and downloading app (quarantine),
     `CmdrOperation { kind, at, source_path, source_volume }` (M3).
   - use: last opened (Spotlight), use count, default app (id + name), Cmdr visits (folders).
   - context: git (`Tracked { clean | modified }`, `Untracked`, `Ignored`, repo root, branch), owner (M3's
     `OwnerClaim`), importance (score band + top explain reasons), recursive size with coverage flags verbatim from
     `DirStats` (exact vs lower bound, stale, updating).
   - folder digest (M5): biggest children, largest files, kind mix, staleness.
4. `gather.rs`: the `context_menu_facts.rs` engine made generic: jobs `(FactKind, Box<dyn FnOnce() -> F>)`, injected
   `submit`, `collect(grace)` → ready + pending + cancel, late sink, cancelled jobs skip work and late answers drop.
5. `provenance.rs` (pure): quarantine string parse (flags, time, agent, uuid; a malformed field is absent, never a
   guess), WhereFroms binary-plist decode (array of strings; anything else → `Unknown(Unparseable)`), Linux `user.xdg.*`
   decode, and `redact_for_egress(url)` → scheme + host + path, path cut at a sane length, no query, fragment, or
   userinfo. Unit-test with the spike's real shapes (synthesized, ❌ no real tokens in the repo).
6. `sniff.rs`: `infer` over the head bytes plus our own checks for what it lacks (verify: DMG's `koly` trailer, xar
   `xar!`, Mach-O fat/thin, `.app` is a folder so it's never sniffed). `mismatch` only when both sides are confident.
7. `removal.rs`: `RemovalConsequence` from facts alone: `RegeneratedCache { app }`, `AppData { app, installed }`,
   `LeftoverOfUninstalledApp { app }`, `InstalledBy { pkg_id }`, `RemovesApp { leaves_data_in_library: true }`,
   `GitCanRestore`, `UncommittedWork`, `UntrackedNotInGit`, `UserDocument`, `CantTell`. Several can apply; order them by
   how much they'd hurt. ❌ No "safe" variant exists, by construction.
8. Tests for all of it in the crate. No app wiring yet.

**Intentions**: the crate owns every rule a human could argue about (what a quarantine field means, what counts as a
mismatch, what deletion costs) as pure, tested code. The app only gathers raw inputs. Names follow the UI ("Tell me
about this", "About this" in code: `about`).

**Landmines**: `deny(missing_docs)`-style discipline like `cmdr-index` (types cross IPC; doc comments land in
`bindings.ts`). Two `specta` versions in one graph break bindings. A `bool` field for git "dirty" or "signed" can't say
"don't know": use the enums. `infer`'s defaults may pull optional deps; take the minimal feature set.

**Test plan**: unit tests per pure module, including the engine's grace-then-late routing with plain threads (copy the
menu's test style). `pnpm check index-crate-isolation`, `module-cycles`, `desktop-rust-cargo-deny`, `clippy`.

**DONE**: crate builds on macOS and Linux (`cargo check -p cmdr-about` both targets via the check runner), every pure
rule has a test, docs pair written, no app change.

---

### M2 – The host, IPC, and the cheap local facts

**Scope**

1. `apps/desktop/src-tauri/src/about/` with `C.md` + `D.md`. Commands (thin, registered in the `ipc.rs` manifest,
   `async`, off the IPC thread): `about_start(request, channel) -> AboutRequestId` and `about_cancel(id)`. A per-request
   registry like `ai/stream_registry.rs` (cancel one, cancel all on app quit).
2. Location first: `crate::location::resolve`; a miss is a typed `AboutEvent::Unavailable(LocationMiss…)`.
3. Jobs for a local, OS-visible path (macOS first, Linux arms where cheap): identity (`stat`/`lstat`, birth time,
   `nlink`), real type (64 KB head read, `cmdr-about::sniff`), bundle/package (`packages.rs`, `Info.plist`), signature
   (Security framework, framework pool, budget 1 s), default app (`open_with.rs`), Spotlight (`MDItemCopyAttributes` for
   last-used, use count, creator, on the framework pool with an autoreleasepool; `NoRecord` on null), provenance xattrs
   (macOS quarantine + WhereFroms; Linux `user.xdg.*`).
4. A new pool config beside the menu's (`name: "about_facts"`), so a wedged card can't starve the context menu.
5. `pnpm bindings:regen`; a typed `tauri-commands/about.ts` wrapper.

**Intentions**: get the streaming pipe right end to end before the expensive facts arrive: first frame by the grace,
each fact by itself, cancel that really stops work. The commands are pass-throughs; every decision is in the crate.

**Landmines**: `src-tauri/CLAUDE.md` rules: sync commands block the IPC thread; network-mount syscalls can hang 30–120 s
(wrap every one; a local path can still be a symlink into `/Volumes`); FDA/TCC: check the FDA gate before touching
`~/Downloads`-type paths only if this could run during onboarding (it can't: the user opens it). `MDItem` and Security
calls need the 8 MB-stack pool, never rayon. Don't touch `menu/` code.

**Test plan**: Rust unit tests for job selection per target and volume kind; an integration test over a `TestDir` with
xattrs written by the test (`xattr` crate) asserting each fact's shape, including `Unknown(NoRecord)` and a quarantine
on a fresh file; a test that `about_cancel` stops queued jobs and drops late answers. `cargo nextest run` the new tests,
`clippy`.

**DONE**: calling the command from a test harness on a downloaded file streams identity, type, provenance, signature,
default app, and last-used in any order, then `FactsSettled`; a cancel mid-way leaves no late events.

---

### M3 ★ – Cross-store facts: Cmdr's own history, git, importance, size, and the owner

**Why ★**: two honesty-critical designs. The op-log provenance query must never claim the wrong node ("you copied this
from Naspolya" about a file that was since replaced), and the owner mapping must say what it knows and how, never a
confident guess.

**Scope**

1. **Op-log provenance** (`crates/cmdr-operation-log`): a new read, `landed_at(conn, volume_id, path, identity)` → the
   newest op whose item LANDED at that path (dest dir resolved through `dirs` + `dest_name_folded`, index-served: add an
   `EXPLAIN QUERY PLAN` test as `build_search_query`'s tests do), and only when `dest_inode` + `dest_birth_ns` match the
   current node's identity (`cmdr_fs::volume::NodeIdentity`). No identity recorded, or a mismatch, is
   `Unknown(NodeChangedSince)` / `Unknown(NoRecord)`, ❌ never the op. Return kind (copy, move, extract, …), time,
   initiator, source path and volume. Its own migration only if an index is truly missing (DURABLE ladder rules). Host
   side through `operation_log_host/`.
2. **Git**: status for the path and repo/branch through the existing `RepoCache` wiring; `Ignored` is a first-class
   answer (it's a strong "this is build output" hint). Ask David before any new `cmdr-git` `pub`.
3. **Importance**: reuse `tools::resources::importance::snapshot_path_on` and the store-path translation; staleness as
   `tools/read/importance.rs` reports it. Cmdr visit count if `ImportanceIndex` already exposes it; else skip the row
   (no ceiling raise for it).
4. **Recursive size** for folders: `dir_stats` with the app's `coverage` builder and `walk_affects`, flags verbatim. A
   file's own size is identity, not this.
5. **Owner mapping** (§ below), pure parts in `cmdr-about::owner`, lookups in the host.

**Owner mapping: what's honest** (the spike's answer to the issue's open question)

- **Certain, by bundle id** (`OwnerClaim::ByBundleId { app, installed }`): a path under
  `~/Library/{Containers, Application Scripts, Caches, HTTPStorages, WebKit, Saved Application State, Logs, Preferences}`
  whose next component is (or starts with, for `.plist` / `.savedState` / `.binarycookies`) a reverse-DNS id;
  `Group Containers/<TEAM>.<id>` and `group.<id>`; `Application Support/<id>`. The id resolves through
  `installed_app_path` → app name, or `installed: false`, which is itself the most useful fact ("left over from an app
  that isn't installed").
- **Probable, by name** (`OwnerClaim::ByName { app }`): `Application Support/<Name>` where exactly one installed app's
  display name or bundle name matches, case-folded (`Slack` → Slack.app). Shown as "probably". Several matches
  (`Google`, `Microsoft`, `Adobe` are vendors) → `Unknown(Ambiguous)`; no match → `Unknown(NoInstalledAppMatches)`.
  Installed apps come from a cached scan of `/Applications`, `/Applications/Utilities`, `/System/Applications`,
  `~/Applications` (top level), invalidated by the same observer `open_with.rs` uses.
- **Certain, by receipt** (`OwnerClaim::InstalledBy { pkg_id }`): outside the home folder (`/Library`, `/usr/local`,
  `/opt`), `pkgutil --file-info` under a deadline. `com.apple.*` package ids read as "part of macOS".
- **Certain, by path** (`OwnerClaim::Homebrew { formula | cask }`): `/opt/homebrew/{Cellar,Caskroom}/<name>`,
  `/usr/local/{Cellar,Caskroom}/<name>`. Inside an `.app` bundle → that app.
- **Not claimed**: dot-folders in home (`~/.npm`, `~/.cargo`), arbitrary vendor folders, anything else: `Unknown`. A
  curated tool table is a follow-up issue, not this effort.
- ❌ Never read inside another app's container (loud rule); the name is the whole answer.
- Linux: `~/.config/<name>`, `~/.local/share/<name>`, `~/.cache/<name>`, `~/.var/app/<id>` (flatpak, certain by id),
  `~/snap/<name>` (certain); a `.desktop` file match is "probable".

**Intentions**: each claim carries HOW it knows (`ByBundleId`, `ByName`, `InstalledBy`, `Homebrew`), so the UI and the
model can phrase certainty honestly and the removal line can lean on it.

**Landmines**: the op log's NO-collation rule (`name_folded` b-tree equality, `fold_name` exactly as insert). Retention
prunes whole ops, so an old copy simply has no record: `NoRecord`, fine. The `dirs` table identity became the EXACT name
in v3; read its DETAILS before querying. `pkgutil` is a subprocess: `crate::subprocess` with a deadline, and its exit
code is the answer, never its stderr wording (`error-string-match`). Importance: never a syscall on an SMB/MTP mount;
MTP has no importance at all.

**Test plan**: op-log query tests in the crate (copy landed → found; file replaced since → not found; renamed after
landing → not found at the new name; index-served plan). Owner classifier table tests (pure), including vendor ambiguity
and `group.` prefixes. Host tests with a fake installed-apps list. `cargo nextest run` for both crates, `clippy`.

**DONE**: a file Cmdr copied in shows "You copied this from <place> on <date>";
`~/Library/Application Support/ com.eltima.cmd1` resolves to Commander One with its install state; `Slack` resolves as
probable; `Google` is ambiguous.

---

### M4 – The card: command, entry points, dialog, streaming rows (local files and folders, no AI)

**Scope**

1. Command `file.tellMeAbout` ("Tell me about this"): `COMMAND_IDS`, a `sources/file-list.ts` entry
   (`showInPalette: true`, `BLOCKED_BY_DIALOGS`, default keys
   `{ macos: ['⌘⇧I'], windows: ['Ctrl+Shift+I'], linux: ['Ctrl+Shift+I'] }`, pending decision D2), a handler in
   `routes/(main)/command-handlers/file-handlers.ts` (on the entry under the cursor, as `file.getInfo` does), the
   four-places menu wiring (`cmdr-menu` `ids.rs` + `bar.rs` File menu next to Get Info, the context row in
   `context/file.rs` right below Get Info, `shortcuts-store.ts` `menuCommands`), and `vitest -u` for the keymap
   snapshot. Read `lib/commands/DETAILS.md` § Adding a command.
2. `apps/desktop/src/lib/about-this/` with `C.md` + `D.md`: a state slice (`about-this-state.svelte.ts`: request id,
   facts by kind, settled flag, verdict state) fed by the channel; `AboutThisDialog.svelte` on `ModalDialog` (registered
   in `SOFT_DIALOG_REGISTRY`, `resizable`, tooltips on every shortened path, a `DialogBoundary`), a dialog-gallery
   fixture row (every fact state: known, unknown with each reason, pending, not applicable). Close fires `about_cancel`.
3. Rows render in a fixed order with a skeleton for `Pending` (no layout jump when a fact lands: reserve each row's
   line), sizes via `<Size>`, dates via `<DateLabel>`, glyphs via `<Icon>`. Unknown rows show their reason in plain
   words. A section groups: Where it came from, What it is, How it's used, Where it sits, What removing it means.
4. Keyboard: opens with focus on the dialog, Esc closes and returns focus to the pane, Tab reaches each action, every
   path row offers reveal/copy. a11y: rows are a description list (`<dl>`), pending rows `aria-busy`, ❌ no live region
   per fact (it would chatter); one polite announcement when facts settle.
5. Analytics hook point only (the event lands in M10).
6. Playwright spec `about-this.spec.ts`: a fixture file the harness writes with quarantine + WhereFroms xattrs; open by
   shortcut, palette, and context menu path; assert the provenance row (host shown), the type row, a folder's size row,
   Esc closes and focus returns. Read `test/e2e-playwright/CLAUDE.md` first.

**Intentions**: keyboard-first, instant, calm. The draft layout is David's to review (see § Review items); make the
defaults good rather than final. The dialog must be MCP-observable through the soft-dialog registry (whatever a user can
reach, an agent must observe).

**Landmines**: `cmdr/no-raw-user-facing-string`, `cmdr/no-raw-key-match`, `cmdr/no-raw-shortcut-text` (a shortcut in
copy is `<ShortcutChip>`), `cmdr/prefer-ui-primitive`, `dialog-inside-boundary`, `dialog-gallery-coverage`,
`command-reachability.test.ts`, `rust-command-id-drift.test.ts`, `menu-claims.test.ts`. Menu labels come from
`menu_t("menu.…")` and context rows live in `cmdr-menu` (❌ never a row in `menu/`). `Ctrl+Shift+I` may open the
WebKitGTK inspector in a debug build on Linux: verify in the Linux E2E container and pick another key if it does.

**Test plan**: vitest for the state slice (out-of-order facts, cancel on close, late event after close ignored), a11y
test for the dialog, the Playwright spec via `pnpm check desktop-e2e-playwright`, `svelte-tests-related`.

**DONE**: ⌘⇧I on a downloaded file shows the card within one frame of the grace, rows fill in, Esc closes, all three
entry points work, gallery shows every state.

---

### M5 – Folders: the digest

**Scope**

1. Host jobs for a folder: biggest children (`list_children` + `dir_stats_batch`, top 8 by size, with coverage), the
   largest files under it (the search core's scope + size sort, the path `search`'s `sortBy: "size"` takes, only when
   the arena is loaded; else `Unknown(NotIndexed)`), the kind mix of its direct children by sniffed-free extension class
   (cheap, from the listing), staleness (share of bytes not modified in 12 months, from the index's mtimes, with
   coverage), and the folder's own `PathClass` (cache / build output / `node_modules` / VCS) from `classify.rs`.
2. `cmdr-about::digest` (pure) shapes those inputs and feeds `RemovalConsequence` (a cache folder → regenerated; a repo
   → git status of its contents).
3. Card section "What's inside" for folders; children rows are actionable (Enter navigates there, closing the card).

**Intentions**: folders are the star. Answer "what lives here, what takes the space, what's stale" from the index in
milliseconds, honestly flagged when the index is partial, never by walking the disk from the card.

**Landmines**: ❌ no live recursive walk from the card (that's the index's job; a 2 TB folder must stay instant). A
full-subtree kind histogram would need a new `Index` method: out of scope (it would raise the `index-crate-isolation`
ceiling; file a follow-up issue instead). Size flags ride inside the spoken string, not only beside it
(`tools/DETAILS.md` § The honesty contract: "a caveat that lives only in a sibling flag is a caveat the model can
shed").

**Test plan**: pure digest tests; host tests against an in-memory index fixture (lower-bound and stale flags carried);
extend the Playwright spec with a folder.

**DONE**: a large folder's card names its biggest children and largest files with honest coverage; a `node_modules` or
`~/Library/Caches/<id>` folder's removal row says the app regenerates it.

---

### M6 ★ – Degradation: remote volumes, dead mounts, archives, MTP, Linux

**Why ★**: a wrong move here hangs the app on a dead NAS, which is the failure the whole design exists to prevent.

**Scope**

1. Per volume kind, which jobs run (a typed table in the host, tested): OS-visible SMB mount (xattrs through the
   volume's own `metadata` path, under budget; no Spotlight, no codesign by default: reading a bundle over SMB is slow),
   direct SMB / SFTP / WebDAV / S3 / NFS (what `Volume` metadata answers; provenance `NotApplicable(VolumeCantTell)`),
   MTP and ADB (name, size, date only), archive entries (from the archive's index; provenance of the ARCHIVE itself as a
   "comes from <archive>" row), `.git` portal paths (commit, author, date via `cmdr-git`), search-results snapshot rows
   (resolve to the real path first).
2. Dead or hung mount: a volume-health precheck through `cmdr_fs::deadline`; past budget every pending fact flips to
   `Unknown(Unreachable)` and the card says so once, at the top.
3. Linux: provenance from `user.xdg.*`, default app from the existing Linux path in `open_with.rs`/`default_app.rs`
   (verify what it answers), signature `NotApplicable(NoCodeSigningOnThisOs)`, owner mapping's Linux arms (M3). Windows
   paths compile but aren't a target.
4. Linux E2E: add an `about-this` case to `test/e2e-linux` (read its `C.md`): provenance via `setfattr`, no signature
   row, Esc closes.

**Landmines**: `src-tauri/CLAUDE.md` two-layer timeouts; `importance` and index reads never touch the mount; `inspect`'s
materialize path is NOT used for facts (it pulls up to 256 MiB). A Finder-mounted SMB path still looks local: route by
`VolumeManager::mount_id_for_path`, never by the path's prefix.

**Test plan**: host tests with an `InMemoryVolume` and a scripted hung volume (millisecond budgets, `wait_until`); the
Linux E2E via `pnpm check desktop-e2e-linux` (flake policy in `docs/guides/multi-agent-refactors.md`).

**DONE**: the card opens instantly on a dead SMB mount and says what it can't reach; an MTP file shows identity only; an
entry inside a zip shows the zip's provenance.

---

### M7 ★ – The egress line and the agent tool (`about_item`)

**Why ★**: the first time any of these facts leave the machine. Consent copy, version bump, redaction, and the tool's
honesty contract all land together, and a mistake is a privacy bug.

**Scope**

1. `AboutCard::for_model()` in `cmdr-about`: the ONE projection: facts as compact JSON with spoken twins (sizes, dates
   already worded, per `tools/DETAILS.md` § Numbers arrive already spoken), URLs through `redact_for_egress`, owner
   claims with their how, coverage reasons kept. A golden test pins the shape and proves no URL query survives.
2. Tool `about_item` per `tools/DETAILS.md` § Adding a tool: table entry (`access: Access::Read`,
   `load: Load::Deferred`, `ctx_params` for cancel), schema closed with `additionalProperties: false` (`path` + optional
   `volumeId`), handler over the same host jobs with a collect deadline (it answers settled facts plus `pending` kinds
   named, never waits forever), rail label (`ask-cmdr-tool-label-keys.ts` + `messages/en/askCmdr.json`), the wire
   fixture (`UPDATE_SNAPSHOT=1` over `tests/tool_snapshot_tests.rs`, read the diff). No contents: the tool never peeks.
3. `tools/CLAUDE.md` + `DETAILS.md`: the read-handler rule gains its third live-filesystem exception (`about_item`'s
   bounded metadata reads), said in the must-know and in the catalog.
4. Consent: a new item `ai.cloudConsent.askCmdr.item.aboutThis` (draft: "When you ask Cmdr about a file or folder: where
   it came from (the download site and page, without the link's private parts), the app that made or owns it, its code
   signature, and its git status"), wired into both disclosure surfaces (settings and onboarding) the way the other
   items are; `CLOUD_AI_CONSENT_VERSION` → `3`; `docs/security.md` § Cloud AI egress; `agent/CLAUDE.md`'s egress
   must-know; the `ai/DETAILS.md` § Cloud AI consent list. Read `ai/DETAILS.md` § Cloud AI consent first.

**Landmines**: `every_known_tool_has_an_ask_cmdr_rail_label`; deferred `brief` is in the cached prefix (keep it short);
every schema property needs a `type` (llama-server refuses otherwise). A tool reaching the live FS is new: the handler
MUST run its jobs off the async worker with the same budgets. Bumping the version makes every cloud user re-consent on
next use: correct, and the reason D7 exists.

**Test plan**: golden test for `for_model` (no query, fragment, or userinfo; protected-path facts still name-only); tool
tests through the real gate (`tools/tests/`); the wire fixture; consent tests in `cloud_consent.rs` keep passing at v3;
the onboarding/settings copy renders the new item (vitest).

**DONE**: Ask Cmdr answers "what is this folder?" by calling `about_item`; a fresh build asks cloud users to re-consent
and the disclosure names the new kinds.

---

### M8 ★ – The AI verdict

**Why ★**: prompt design under safety constraints (no green light, grounded only in facts, honest about uncertainty),
cost metering, cancellation, and the local-model quality call.

**Scope**

1. Gate: verdict runs only when `ai.provider` is `local` or `cloud`, the slot resolves (`resolve_agent_llm`; a refusal
   maps to a typed `VerdictUnavailable(Off | NoCloudConsent | NotConfigured | Managed | …)` the card words), and the
   `aboutThis.aiVerdict` setting is on (new registry setting per `docs/guides/adding-a-new-setting.md`, default per D3).
   AI Off shows no verdict block at all, only a quiet one-line hint where to turn AI on (review item).
2. Start after `FactsSettled` or a 1.5 s cap, whichever first (grounded, not blocked forever by a slow fact).
3. The peek: for a file, one bounded `inspect_path_with` call (first window, small: text head, PDF first page plus
   title, archive's first entries, EXIF minus GPS unless D8 says otherwise); for a folder, none (the digest is the
   peek). The sensitive gate is part of that call: a `protected` answer means the verdict says it didn't look inside.
4. The call:
   `AgentLlm::respond(system, &[], [user: for_model() + peek], CallSettings { modest max output, model_reasons per slot }, cancel)`.
   The prompt lives in `agent/prompts/` as Markdown (`prompts/CLAUDE.md`). It asks for two or three sentences: what it
   is, who owns it, what removing it would cost, using only the facts; says "can't tell" when facts don't say; ❌ never
   a green light. Stream deltas to the card; `about_cancel` fires the token. The local model's context is small
   (`ai.localContextSize`): keep the user message under ~1,500 tokens.
5. Cost: a migration in `agent/store/migrations.rs` adds one reserved conversation row with a new
   `ConversationOrigin::AboutThis` (the `quiet_wakes` precedent: hidden from the session list, outside the wake
   ceiling's origin filter), and the verdict meters through `agent::pricing::price_call` + `record_cost` against it.
   Unknown price → `priced = false`, shown "unknown", never $0. The card shows nothing about cost (review item: a small
   cost line?).
6. The E2E fake: `e2e_fake.rs` gains a verdict script (`by_prompt` on a marker in the verdict system prompt), so the
   Playwright spec asserts a streamed verdict with zero network.
7. Revoking cloud consent cancels in-flight verdicts too (register them where `revoke_cloud_ai_consent` cancels).

**Real-LLM testing (worth it here, about $3 of the $10)**: an `#[ignore]` eval test beside
`client_real_anthropic_test.rs` over ~12 synthesized cards (a Chrome-downloaded `.dmg`, an uninstalled app's container,
a `node_modules`, a modified git file, an unsigned binary with a lying extension, a `.pkg` receipt file, a dead-mount
card with mostly unknowns, a protected `~/.ssh` file, …), each run against one Haiku-class and one Sonnet-class model id
from `smoke_providers.rs` (the one home of model ids), key from `ANTHROPIC_API_KEY_CMDR_DEV` via `secret`. Assertions
are structural and reviewed by eye: ≤ 3 sentences, no green-light phrasing (a reviewed list checked in the TEST only,
never in product code), mentions "can't tell" on the unknown-heavy card, never claims to have read contents on the
protected card. Run the same corpus on the local model if one is installed (free); write the results to `docs/notes/`
and feed D4.

**Landmines**: `ai/CLAUDE.md`: classify provider failures by status, never sentence; cancel needs the explicit command.
`agent/llm/CLAUDE.md`: a non-reasoning model must say `model_reasons = Some(false)` or explicit effort 400s. Assistant
prose renders ONLY through the house Markdown renderer (`renderAssistantMarkdown`, the XSS boundary: a filename the
model echoes can inject); ❌ never `{@html}` it raw. ❌ No string-matching of model output in product code.

**Test plan**: Rust tests with `FakeAgentLlm` (verdict streams, cancel stops it, consent revoke cancels it, cost row
lands on the reserved conversation, unpriced model reads unknown), the migration's tests (ladder rules), vitest for the
verdict block states, the Playwright spec with the fake, the eval above.

**DONE**: with Cloud on and consented, the verdict streams under the facts and stops on Esc; with Off it never calls
anything; cost shows up in the daily summary; the eval notes are written.

---

### M9 – Next actions and "Continue in Ask Cmdr"

**Scope**

1. Actions derived from facts (deterministic, shown with AI off): Reveal the owning app (`OwnerClaim` with an installed
   app), Open with the default app, Show in Finder / file manager (existing `file.showInFinder`), Go to the download
   page (opens the page URL in the browser: full URL, local), Move to Trash (dispatches the existing delete flow with
   its confirmation; ❌ the card never deletes on its own), Go to where similar files live (only when M5's digest or the
   owner says where; otherwise hidden).
2. "Continue in Ask Cmdr" (shown when Ask Cmdr is on, per D5): `newChat()`, `addAttachments([{ path, kind }])`, a
   prefilled, unsent composer draft ("Tell me more about this.", plus the verdict quoted when there is one), `openRail`,
   `requestComposerFocus`. The agent reads the same facts by calling `about_item`. ❌ No auto-send: the user's send is
   the egress moment, and the rail's gates stay the only door. Add a draft setter to the trigger slice if none exists.
3. Every action reachable by keyboard (Tab order, plus single-key accelerators shown on the buttons only if David wants
   them: review item).

**Landmines**: Ask Cmdr's rail is a THIRD focus region and non-modal: close the card first, then open the rail, so focus
lands in the composer (`ask-cmdr/CLAUDE.md`). "Opening the rail GROWS the main window", once. The trash path must go
through the existing delete dialog so the op log records it.

**Test plan**: vitest for action derivation; the Playwright spec continues into Ask Cmdr with the fake and asserts the
attachment and the draft.

**DONE**: from a card, one keystroke path each to reveal the owner, open, trash (with confirmation), and continue in Ask
Cmdr with the item attached.

---

### M10 – Analytics, feature status, docs, and the close-out sweep

**Scope**

1. Analytics: one frontend event `tell_me_about_used` at card close (`track_event`): `trigger` (`shortcut`, `palette`,
   `context_menu`, `menu_bar`), `target` (`file`, `folder`, `app`, `package`, `archive_entry`), `volume_kind`, `verdict`
   (`none`, `shown`, `stopped`, `unavailable`), `action` (the action taken, or `none`), `continued` bool. ❌ No path,
   name, URL, or app name. Document it in `analytics/DETAILS.md` § Starter event set.
2. `feature-status.json` `ai-tell-me` → `alpha` with an honest one-line note (draft for David), `issueUrl` #1104;
   `roadmap.ts` milestone and `features-page.ts` stay as they are for David to flip (website copy is his review).
3. Docs: `docs/architecture.md` entries for `about/`, `lib/about-this/`, and the crate; every touched `C.md`/`D.md`
   current; the plan's durable decisions moved beside the code; `docs/specs/DETAILS.md` entry kept until the wipe.
4. Sweep: `rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'` over touched paths; then `pnpm check --only-slow`
   (this is a 5+ agent effort).
5. File follow-up issues (labels per `docs/tooling/backlog-board.md`, `claude-created`): the menu engine unification, a
   curated dot-folder tool table, a subtree kind histogram, an inspector that follows the cursor (if D1 picks the
   dialog), and whatever the reviews defer.

**DONE**: green `--only-slow`, docs audited, follow-ups filed, the lead runs the close-out reviews.

## Open decisions for David (each has a default so execution isn't blocked)

- **D1 – Surface.** (a) A resizable modal dialog over the panes, like Get Info but ours. (b) A non-modal inspector panel
  that follows the cursor. **Recommendation and default: (a)**: it has every house primitive (focus trap, registry, MCP
  observability, gallery), and (b) can grow from the same state slice later.
- **D2 – Shortcut.** **Default: `⌘⇧I` on macOS, `Ctrl+Shift+I` on Windows and Linux** (beside Get Info's `⌘I` /
  `Alt+Enter`; `Alt+Shift+Enter` is taken by Calculate folder sizes). Changes if the Linux inspector claims it.
- **D3 – Verdict start.** (a) Streams in automatically when AI is on, with a setting to turn it off. (b) A "Get a short
  AI take" button. **Recommendation and default: (a), setting default on**: the issue's design, and a verdict costs well
  under a cent on a small cloud model.
- **D4 – Local model.** **Default: verdict off on Local until M8's eval says it's good**; facts always. David decides
  from the eval notes.
- **D5 – Which switch gates the verdict.** **Recommendation and default: `ai.provider` plus cloud consent, like folder
  suggestions, NOT Ask Cmdr's switch**; "Continue in Ask Cmdr" shows only when Ask Cmdr is on.
- **D6 – Twins.** The index has no content hash and no size index, so "same content elsewhere" can't be answered
  honestly or fast. **Recommendation and default: cut content twins**; show the hard-link count and "Cmdr copied this
  from/to …" from the op log instead.
- **D7 – Consent bump.** **Default: one new disclosure item plus `CLOUD_AI_CONSENT_VERSION` 2 → 3**, so every cloud user
  re-consents once.
- **D8 – What the peek may send.** **Default: the `inspect_file` window as Ask Cmdr already sends it, minus a photo's
  GPS** (location adds nothing to "what is this file", and leaving it out costs nothing).
- **D9 – Probable owners.** **Default: show name-matched owners as "probably <App>"**; the alternative is bundle-id
  claims only, which leaves `Slack` and `Firefox` unexplained.
- **D10 – Download URL on the card.** **Default: the site's host on the card, the full URL behind copy and open** (it's
  the user's own data locally; only the model gets the redacted form).

## Review items for David (layout and copy, before release)

Dialog title ("About “name”" draft), section names and order, each row's wording per state (especially every unknown
reason), the removal-consequence sentences (one per `RemovalConsequence` variant), the AI-off hint, the verdict block's
look while streaming and when stopped, the cost line (shown or not), action labels and order, the consent item text, the
`feature-status.json` note, the context-menu and menu-bar label, and the analytics event's props.

## Invariants register (the close-out review checks each)

1. The card renders and every fact gathers with AI off (`ai.provider = off`), no LLM call made.
2. Every fact is `Known`, `Unknown(why)`, `NotApplicable(why)`, or `Pending`; no untyped unknown, no wrong zero, no
   "never" for `NoRecord`.
3. No fact gathering on the main thread, an IPC handler thread, or rayon; every job budgeted; cancel stops queued jobs
   and drops late answers.
4. No syscall against an SMB/MTP mount outside Cmdr's deadline-wrapped volume paths; index, importance, and op-log facts
   are DB-only; a dead mount answers within budget.
5. No surface says or implies a deletion is safe; `RemovalConsequence` has no "safe" variant.
6. Exactly one model projection (`for_model`); no URL query, fragment, or userinfo leaves the machine; contents only
   through `inspect_path_with` and its sensitive gate.
7. `CLOUD_AI_CONSENT_VERSION` was bumped in the same commit that first sent the new kinds, with the disclosure item.
8. Every LLM call goes through `resolve_agent_llm`; every completed verdict is metered on the reserved row; unpriced is
   never $0; cancel and consent revoke both stop it.
9. Nothing reads inside another app's container.
10. `cmdr-about` has no `tauri`, `tauri-specta`, or `cmdr` in its tree and sits at its surface ceiling.
11. The op-log provenance claim requires a matching node identity.
12. `about_item` and the card read the same host jobs; the tool never peeks at contents.
13. Every new string is English with an `@key` description, untranslated; every action has a keyboard path.
14. No milestone tags outside this plan.
