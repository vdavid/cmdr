# Platform-variant copy: implementation plan (Windows port task 27)

Issue: [#1103](https://github.com/getcmdr/cmdr/issues/1103). Plan entry: `docs/specs/windows-port-plan.md` task 27.
Written against `f0e0d8639b` (2026-11-11).

**Goal**: every user-visible string reads in its own OS's words on macOS, Windows, and Linux. On macOS, every catalog
renders byte-identical text to today, in every language, except the two accepted changes. Translating the new Windows
and Linux branches is task 28 and comes after.

## Loud rules (read before every milestone)

1. **macOS text doesn't move.** Every locale's macOS rendering of every key stays byte-identical, except exactly two
   English values (`ai.secretError.genericBody`: "keyring" → "Keychain"; `goToPath.dialog.inputPlaceholder`: the "e.g."
   change) and the keys collapsed from a `.mac`/`.other` pair (whose macOS branch equals the old `.mac` value).
   `macos-copy-unchanged.test.ts` (milestone 1) is the proof; never edit its baseline to make it pass.
2. **❌ Never translate.** Task 27 writes English. Non-English locale files change only by script (milestones 6–11), and
   only in the shape § "How English lands before translations" pins. A translator agent is task 28. The English overlays
   `en-GB`/`en-AU` are English: task 27 spells and restamps their new branches (same §).
3. **❌ Nothing hand-loosened.** No allowlist, baseline, golden, or check severity is raised to get green. If a check
   goes red, the design is wrong or the check needs to learn platform selects; fix one of those. Allowlists only shrink
   (`AGENTS.md` § Hard rules).
4. **One platform-select shape, everywhere.** A platform-variant value is ENTIRELY one block:
   `{platform, select, macos {…} windows {…} other {…}}`. Never nested inside a sentence, never interpolated as
   `{platform}`. `other` is Linux and every context without a backend. Raw and ICU families share the shape.
5. **Decisions are settled.** `docs/notes/windows-port/copy-decisions.md` wins over `copy-proposal.json`; the two
   sibling docs (below) correct the proposal further. Don't re-litigate a glossary choice; where the code doesn't settle
   a row, keep the macOS text for that platform and list the row as open in `copy-decisions.md`.
6. **Show or hide with a capability, word with a select.** `hasCapability()` decides whether a surface exists;
   `isMacOS()` never chooses copy (lint lands in milestone 7). Platform-specific UI stays per
   `docs/design-principles.md` § "Platform-native, not generic".
7. **The proposal's raw-family notes are stale.** Its `icu` field says raw rows "ship as `<key>.windows` / `<key>.linux`
   siblings" and `implementationNote` repeats it. David chose the one top-level block (decision file § Beyond wording).
   Build the block; ignore those notes.
8. **Strip milestone tags** from code, comments, test names, and docs before each commit
   (`docs/guides/multi-agent-refactors.md`). This plan keeps its own.

## Inputs

- `docs/notes/windows-port/copy-decisions.md`: David's answers. Wins over everything below.
- `docs/notes/windows-port/copy-glossary-evidence.md` (sibling agent, may land after this plan): vocabulary corrections
  from the reference pile. Where it corrects a glossary term, it wins over the proposal.
- `docs/notes/windows-port/copy-row-verdicts.md` (sibling agent, may land after this plan): per-row verdicts and the
  exact capability gates, read from the code. Where it gives a row a verdict, it wins over the proposal row.
- `docs/notes/windows-port/copy-proposal.json`: 478 rows (`strings`), 349 description-only rows (`descriptionOnly`), 109
  reviewed-no-change rows. Row fields that matter: `key`, `keys` (a collapsed pair lists both), `file`, `source`
  (`catalog` or `rust`), `mechanism` (`select` 269, `none` 174, `split-feature` 18, `neutral` 10, `code` 7), `macos`,
  `windows`, `linux`, `linuxDesktops` (ignored: task 59), `macosChanged`, `rawFamily`, `needsDavid`, `question`.
- `docs/notes/windows-port/copy-survey.md` § Mechanism: the design this plan refines. Where the code contradicts it, §
  "What the code says" below wins.

If a sibling doc isn't on disk when a milestone that consumes it starts, stop and report; don't guess its content.

## What the code says (fresh look at `f0e0d8639b`)

### Runtime (`apps/desktop/src/lib/intl/messages.svelte.ts`)

- `t()` and `tStringIn()` both end in `getCompiled(locale, key).format(params)`, wrapped by `asShown()`. `Trans.svelte`
  calls `t(key, handlers)`, so injecting `platform` at the one shared format call covers `Trans` with no change there.
- `getCompiled` caches by `(locale, key)`. `platform` as a format ARG keeps that cache valid: no platform in the key.
- `getMessage()` is `resolveRaw(locale, key)` with no parsing. Its callers include raw `errors.*` factories
  (`error-messages/*.ts`, `file-operations/transfer/transfer-error-messages.ts` via `w(…)`) and a few ICU-family keys.
- `TranslationParams = Record<string, PrimitiveType | Date | FormatXMLElementFn<unknown>>`: excluding a key needs an
  intersection (`& { platform?: never }`); a plain `Omit` on a `Record<string, …>` does nothing.
- `#lib/platform` (`getPlatform()`) reads `globalThis.__CMDR_PLATFORM__` per call and imports only a type from
  `bindings.ts`, so the runtime may import it without breaking `test-setup-closure.test.ts` (verify it stays green).
- `FALLBACK_PLATFORM = 'linux'`: Vitest renders `other` branches. Every test asserting macOS text needs
  `withPlatform('macos')` (`lib/platform/test-platform.ts`). That includes the ~20 per-area English goldens
  (`find apps/desktop/src -name '*i18n-parity.test.ts' -o -name '*.parity.test.ts'`: file-explorer, settings,
  onboarding, command-registry, transfer-error-messages, handler-toasts, …), which pin today's macOS English through
  `tString`. `withPlatform` also moves key-glyph formatting, so wrap per assertion where a golden renders a shortcut.
- `manifestMessage()` returns the RAW manifest value with no format call (`MANIFEST_KEYS` in
  `scripts/vite-locale-catalogs.ts`, today only `settings.appearance.language.label`). A manifest key must never hold a
  block.

### Raw families

- `isRawKey()` (`apps/desktop/scripts/i18n-catalog-lib.ts`) is `errors.*` plus `isNativeKey()`, which is `menu.`,
  `licensing.windowTitle.`, and `main.instanceLock.` (`NATIVE_KEY_PREFIXES` in `gen-native-strings-lib.ts`). The survey
  names only `errors.*` and `menu.*`.
- Native keys are ALSO read by the frontend through ICU: `row-menu.ts` calls `tString('menu.volume.eject', { name })`,
  and the demo's `traffic-lights.ts` reads `menu.window.minimize`. So a native block must parse as ICU and split as raw.
  It does, given rule 4, single apostrophes (the raw rule already forbids `''`), AND no apostrophe directly before a
  brace in a raw body: ICU reads `'{` as the start of a quoted run, raw reads it as text, so a French `l'{name}` would
  split one way and render another. No raw value has `'{` or `'}` today (grep at `f0e0d8639b`); the shape check keeps it
  that way, which lets one ICU-quoting splitter serve both families. (The demo's `menu-surfaces.ts` also reads `menu.*`
  through `getMessage`.)
- `rawTokens()` is a `\{([^{}]*)\}` regex. On a block it would read each brace-free branch body as a "token". Every raw
  consumer (parity, brief `i18n-brief-lib.ts`, pseudolocale `pseudoRaw`, and the `visibleTextOf` helpers in
  `scripts/i18n-en-overlays.test.ts` and `scripts/i18n-es-overlay.test.ts`, which strip `{…}` spans and so would go
  BLIND to a raw block's text) must split the block first.
- `expandSystemStrings()` replaces `{system_settings}` and friends after the catalog lookup; those tokens belong in the
  `macos` branch only, and per-branch token parity must allow branches to differ from each other.
- The old-WebKit boot guard (`gen-boot-guard-lib.ts`, spliced in by `vite.config.js`) runs in ES5 before the runtime
  exists. A key it reads can't hold a block unless the guard resolves it itself from `window.__CMDR_PLATFORM__`.

### Rust

- `menu_t(key)` (`src-tauri/src/intl/native_strings.rs`) is a binary-search lookup in the generated
  `native_strings.gen.rs` table, then `.to_string()`. `menu_t_with` does literal `{token}` replacement.
- `cmdr-menu`'s `Platform` is `MacOs | Linux` only, and `Platform::current()` returns `Linux` on Windows ("every
  platform that isn't macOS gets the Linux one"). `PerPlatform<T>` has `macos` and `linux` fields. So text can't be
  chosen through `cmdr-menu`'s platform without making Windows read the Linux words.
- An OS `cfg` in `native_strings.gen.rs` (`src-tauri/src/intl/`, not a `platform/` dir) counts as outside a seam for
  `desktop-rust-platform-cfg-placement`, whose allowlist only shrinks without David's OK. `src-tauri/src/intl/platform/`
  is an existing seam (`linux.rs`, `macos.rs`, `windows.rs`, `mod.rs`): the OS answer comes from there.
- `SHOW_IN_FILE_MANAGER_KEY: PerPlatform<&str>` in `crates/cmdr-menu/src/bar.rs` is the one pair that lives in Rust.
- `menu.file.getInfo` and `menu.file.quickLook` are plain `item`s in `bar.rs` today (shown on Linux).

### Existing pairs and copy branches

The 14 pair rows and their `isMacOS()` call sites:

- `commands.fileShowInFinder.{mac,other}.label`, `commands.fileGetInfo.{mac,other}.label`,
  `commands.fileQuickLook.{mac,other}.label`: `lib/commands/sources/file-list.ts` (three ternaries on `nameKey`).
- `errors.write.fileLocked.suggestion.{mac,other}`,
  `errors.write.permissionDenied.suggestion.{deleteMac,deleteOther, needsAdminMac,needsAdminOther,systemProtectedMac,systemProtectedOther}`:
  `lib/file-operations/transfer/transfer-error-messages.ts` (line ~181 and the `mac ?` helper near line 436).
- `settings.appearance.appColorHintLink{Mac,Other}`: `lib/settings/sections/AppearanceSection.svelte`.
- `ai.secretError.{keychainTitle,keyringTitle}`, `{keychainBody,keyringBody}`, `{removeKeychainBody,keyringBody}`:
  `lib/settings/sections/ai-secret-error.ts` (`keyringBody` serves two pairs).
- `mtp.connectedToast.body{Mac,Other}`: `lib/mtp/MtpConnectedToastContent.svelte`.
- `main.upgradeNudge.{mac,other}`: `routes/(main)/startup-gates.ts` line ~196.
- `menu.file.showInFinder` + `menu.file.showInFileManager`: `SHOW_IN_FILE_MANAGER_KEY` in `bar.rs`.

### Checks (`apps/desktop/scripts/i18n-check-*.ts`, wrapped by `scripts/check/checks/desktop-i18n-*.go`)

- **Parity** (ERROR): ICU keys compare `parseMessage` placeholder and tag sets. `platform` already counts as a
  placeholder (it's a select arg), so a locale left as a plain value beside a migrated English value FAILS parity. That
  is why every locale file must be restructured in the same commit as its English (§ How English lands). Branch SETS
  aren't compared today.
- **Stale** (WARN; ERROR under `CMDR_I18N_STALE_STRICT=1`, which `scripts/release.sh` sets): whole-value
  `@key.sourceHash` against `sourceHash(current source value)`.
- **Coverage** (ERROR for missing, identical, and source-text-only; WARN for `englishBranches`): `englishBranches` flags
  a branch that repeats a DIFFERENT English category's text. A locale branch equal to English's SAME branch is never
  flagged, so the survey's "it already warns per branch" doesn't hold for platform branches.
- **Dont-translate, quoted labels, aria containment, term consistency, termbase, mechanics**: read whole values.
  Termbase and mechanics hold per-locale baselines whose raise needs David's OK.
- **ICU** (ERROR): skips ICU parsing for raw keys and refuses `''` there.
- **Plural** (ERROR): `parseMessage` merges each arg's categories across EVERY branch (`walkAst`), so a block's
  `{count, plural}` categories are a union over its platform branches. Today that masks a translated branch missing
  `few` whenever another branch has it. Going per branch naively is worse: a `ru`/`cs`/`uk` branch carrying English text
  carries English's `one`/`other` and would ERROR for `few`/`many`. Per-branch plural must hold an English-carryover
  branch to English's categories (the runtime falls back to `other` for a missing category, so it renders).
- **Go checks read catalogs too**: `i18n-tag-param-collision` and `i18n-trans-snippets` regex the whole English value,
  so they already see the union of every branch's args and tags (a `<Trans>` call site must supply snippets for every
  branch's tags, which is right). `i18n-citations` reads `docs/i18n/**/*.md` key citations, so a deleted pair key
  orphans every citation of it.
- **Tests that read catalogs from disk, not through imports**: `scripts/i18n-en-overlays.test.ts` (ERROR, vitest):
  asserts every `en-GB`/`en-AU` key's `@key.sourceHash` equals `sourceHash(en value)` ("a wrong hash silently disarms
  the staleness net"), sweeps `FORKED_TERMS` (a base-`en` value using "color", "favorite", "canceled", "trash", … must
  be forked by both overlays, and no fork may keep the American form), pins "no `trash` in overlay copy" to an exact key
  list, and pins the Bin rulings. `scripts/i18n-es-overlay.test.ts` sweeps Spain-only forms. `svelte-tests-related`
  never selects these for a JSON-only change (no import edge), so a catalog milestone runs them by name.
- **Other key-name readers**: `messages/screenshots/capture-report.json` (read by `message-screenshots-fresh`),
  `docs/i18n/*/terms.json` `exceptions` (an exception naming a key English lacks is a termbase SCHEMA error),
  `docs/i18n/*/decisions.md` and `review-queue.md` citations, and `scripts/i18n-term-consistency-allowlist.json` all
  name pair keys today (`git grep fileShowInFinder.mac` and friends). A collapse renames them all.
- `sync-locale-keys.ts` drops locale keys that left `en` and adds new ones as English skeletons. Run naively after a
  pair collapse, it would throw away every locale's pair translations. The apply script (milestone 6) does the collapse
  itself; `sync-locale-keys.ts` afterwards must be a no-op.

## Design

### The block

```
{platform, select, macos {…} windows {…} other {…}}
```

- Branch names are `macos`, `windows`, `other`. `other` is required and covers Linux plus every backend-less context. No
  `linux` branch: it would be a second name for `other` today. (Task 59's `desktop` value nests inside `other` later.)
- At least two branches; no two branches byte-identical (the apply script folds a duplicate into `other`, or drops the
  `windows` branch when Windows reads like Linux). So `{macos, other}`, `{windows, other}`, and
  `{macos, windows, other}` are the legal sets. In `{windows, other}`, macOS reads `other`: every rule below that says
  "the `macos` branch" means "the branch macOS reads" (`pickPlatformBranch(value, 'macos')`), never `branches.macos`.
- Canonical spelling: the value starts with exactly `{platform, select, ` (one space after each comma) and branches come
  in the order `macos`, `windows`, `other`. The shape check enforces it, so `getMessage`'s prefix test is sound.
- Whole value only (rule 4). The same block is valid ICU and is what the raw splitter reads.
- Raw branch bodies keep raw rules: single apostrophes, literal `{token}`s, markdown, literal `<…>`. Braces inside a raw
  body must balance (the shape check enforces it).

### One splitter, three readers

`apps/desktop/src/lib/intl/platform-select.ts` (new, dependency-free, ES2020):

- `parsePlatformSelect(value): { macos?: string; windows?: string; other: string } | null`: `null` for a value that
  isn't a block; brace-matched, respecting ICU apostrophe quoting so an ICU branch containing `'{'` still splits. Raw
  bodies have no quoting; the shape check's "no `'` directly before a brace in a raw body" rule (§ Raw families) is what
  makes one quoting rule split both families the same.
- `pickPlatformBranch(value, platform): string`: the branch's source text (`linux` → `other`, missing branch → `other`),
  or `value` unchanged when it isn't a block.

Readers:

1. **`getMessage()`**: returns `pickPlatformBranch(resolveRaw(locale, key), getPlatform())`. Covers `errors.*` and any
   ICU-family key read raw. Cheap prefix test first (`value.startsWith('{platform, select,')`).
2. **`gen-native-strings`**: imports the splitter (scripts already import `src/lib/intl/locale-inheritance.ts`, the
   precedent) and emits each native entry as a per-OS value (below).
3. **The checks**: `i18n-catalog-lib.ts` gains `platformBranches(key, value)` over the splitter, and every check that
   reads text iterates branches through it.

### ICU family: `platform` injected at the one format call

`t()` and `tStringIn()` both call a private `format(compiled, params)` that passes `{ ...params, platform }` (platform
last, so even an untyped caller can't override it). When `params` is undefined, reuse one frozen `{ platform }` object
per platform: `t()` runs per visible row in some lists. `TranslationParams` becomes
`Record<string, …> & { platform?: never }`, so `t(key, { platform: 'x' })` is a type error. ICU does the select, so
nested `plural`s inside a branch work unchanged. `Trans.svelte` needs no change; `i18n-tag-param-collision` learns that
`platform` is reserved (a `<platform>` tag would collide with the injected arg).

Why not pre-split ICU values too: ICU owns ICU grammar (quoting, nested selects); one engine per family keeps each
correct. The splitter exists only because the raw families have no engine.

### Rust: per-OS values, OS answer from the seam

`gen-native-strings` emits entries as `(key, NativeText)` where `NativeText` is `All("…")` or
`PerOs { macos: "…", windows: "…", other: "…" }` (hand-written type beside `LocaleStrings`, like today). `menu_t` picks
with `super::platform::NATIVE_TEXT_OS` (or a fn), a new answer in the `intl/platform/` seam, one arm per OS. No `cfg` in
the generated file, no allowlist change. `menu_t`'s signature and every caller stay as they are. The binary carries the
other OSes' variants of a dozen menu labels per locale; that's a few KB, accepted for a cfg-free generated file.

The seam's shape: each arm (`intl/platform/{macos,linux,windows}.rs`) defines the answer, `mod.rs` re-exports it from
`imp` and pins its type with a `const _:` line like the two existing answers. The pick itself is a pure
`fn pick(&NativeText, NativeOs) -> &'static str` in `native_strings.rs`, so one test covers all three OSes on any host;
only "this arm answers its own OS" is per-host.

`SHOW_IN_FILE_MANAGER_KEY` retires (one key, one block); `PerPlatform` stays for accelerators and ids. It is
`Label::PerPlatformKey`'s only user (`bar.rs`), so that variant goes too, and `cmdr-menu`'s text goldens (`bar_test.rs`
lines naming `menu.file.showInFinder` / `menu.file.showInFileManager`) move to the survivor key.

### `system_strings` tokens

`{system_settings}`, `{privacy_and_security}`, `{local_network}`, `{appearance}` stay in `macos` branches. Windows and
Linux branches write their own path in words (`Settings > System > Storage`). Giving `LocalizedSystemStrings` per-OS
arms is out of scope (survey: low priority once the Local Network block is gated).

## How English lands before translations

The pipeline's answer to "English changed, translations lag" is `desktop-i18n-stale`: whole-value `sourceHash`, WARN in
`pnpm check` and CI, ERROR only in `scripts/release.sh`. Task 27 uses it as designed, so no check is loosened.

**What every one of the 13 full locales (`cs de es fr hu nl pt ru sv uk vi zh zh-Hant`) looks like after task 27:**

- **A migrated `select` key**: the value becomes the same block English has, same branch set. Each branch holds:
  - the locale's existing translation, byte-identical, when that branch's English text equals the English the
    translation was made from (always true for `macos`; also for any `windows`/`other` branch the row left unchanged);
  - otherwise that branch's English text.
- `@key.sourceHash` stays as it is: the hash of the pre-migration English, which the `macos` branch was translated from.
  So `desktop-i18n-stale` lists exactly these keys per locale: task 28's worklist. `reviewed` and
  `sameAsSourceJustification` stay untouched; the stale check already reports both as no longer applying.
- **A collapsed pair**: the survivor key's value is a block; `macos` ← the locale's `.mac` value, `other` ← the locale's
  `.other` value when English `other` equals the old English `.other` (else English), `windows` likewise. Its `@key`
  block comes from the `.mac` key (so its hash is the old `.mac` English hash: stale). The old keys are gone.
- **A key already stale before task 27**: its translation goes in the branch macOS reads only; the apply script lists
  these.
- **`none`, gated, `code`-mechanism, and description-only rows**: locale files untouched.
- **A collapsed pair, outside the catalogs**: the script renames the old keys to the survivor in every
  `docs/i18n/*/terms.json` `exceptions`, `docs/i18n/*/{decisions,review-queue}.md` citation,
  `scripts/i18n-term-consistency-allowlist.json`, and `messages/screenshots/capture-report.json` (the survivor renders
  on the surface the `.mac` key was captured on). Otherwise termbase (schema ERROR), `i18n-citations` (ERROR), and
  `message-screenshots-fresh` go red.
- **`es-419`** (a translated overlay of `es`): a forked key gets the block shape with the full-locale rule per branch:
  `es-419`'s own fork text in each branch whose English equals the English the fork was made from, else `es`'s migrated
  branch (English text there). ❌ Never copy `es`'s carried TRANSLATION into an `es-419` branch: that puts Spain-only
  forms back and fails `i18n-es-overlay.test.ts`. Hash untouched, so stale (that test asserts no hash).
- **`en-GB` and `en-AU`** (hand-written English overlays): they are English, so task 27 writes them, and they can't stay
  stale. `i18n-en-overlays.test.ts` hard-fails on any fork whose `@key.sourceHash` isn't `sourceHash(current en)`, on
  any base-`en` value using a `FORKED_TERMS` word the overlay doesn't fork (a new Windows or Linux branch saying
  "favorite", "color", or "canceled" needs a NEW fork), on any American form inside a fork, and on "trash" anywhere in
  overlay copy. So per migrated key: the script builds the block (each branch: the overlay's fork text where that
  branch's English is unchanged, else `en`'s branch), then the milestone agent applies the overlay's spelling rulings
  (`docs/i18n/en-GB/glossary.md`, `docs/i18n/en-AU/glossary.md`) to the new branches, adds the forks the sweep demands,
  and restamps with `sync-locale-keys.ts --restamp <key>` (its `restampOverlay`). This is the one sanctioned restamp
  (invariant 9's exception): an English fork of English isn't a translation.

**What a reader sees**: German on a Mac reads today's German everywhere. German on Windows or Linux reads today's German
wherever the row didn't change that branch, and the English Windows or Linux sentence where it did (instead of German
text naming Finder or the Trash). English on every OS reads its own OS's words.

**Which checks see what**: parity green (same args and branch sets); ICU green; coverage green (no value is identical or
source-text-only, since the `macos` branch is translated; a key whose translation was already English-identical keeps
its `sameAsSourceJustification`, which coverage honors; the new branch rule below fires only on fresh keys); plural
green (per branch, carryover branches held to English's categories); the judging checks skip English-carryover branches;
stale WARNs on every migrated key in every full locale and `es-419`, none in `en-GB`/`en-AU` (restamped); the overlay
vitest files green. CI goes green.

**The consequence to plan around**: `CMDR_I18N_STALE_STRICT=1` makes `scripts/release.sh` refuse to release until task
28 restamps those keys. That's the gate working as designed. David's call (open question 1 below): (a) start task 28
right after task 27 lands and pause releases until it's done (recommended; translator agents, ~3 in flight), or (b)
teach the stale check a "macOS branch current, other branches pending" class that strict mode lets through while Windows
and Linux have no public build. (b) is a release-gate change, so it needs David's go.

## Milestones

Each runs as one agent, sequentially, in this worktree, and ends in one commit (or a short series) with these green:
`pnpm check --fast && pnpm check svelte-tests-related`; if Rust changed, `pnpm check clippy` plus the touched tests
(`cargo nextest run <name>`); if catalogs changed, the i18n lane:
`pnpm check i18n-parity i18n-icu i18n-plural i18n-stale i18n-coverage i18n-dont-translate i18n-quoted-labels i18n-aria i18n-terms i18n-termbase i18n-mechanics i18n-citations`
plus
`i18n-tag-param-collision i18n-trans-snippets message-keys-fresh message-keys-unused message-key-naming message-screenshots-fresh native-strings-fresh`
and the new `desktop-i18n-platform-select` once it exists, PLUS the disk-reading catalog tests by name, which
`svelte-tests-related` can't select from a JSON change:
`cd apps/desktop && pnpm vitest run scripts/i18n-en-overlays.test.ts scripts/i18n-es-overlay.test.ts scripts/gen-pseudolocale.test.ts scripts/gen-boot-guard-lib.test.ts`
and the touched area's `*i18n-parity.test.ts`. Stale WARNs from migrated keys are expected from milestone 7 on; no other
new warning is. A milestone whose copy is asserted by a Playwright spec runs `desktop-e2e-linux` for that spec
(`./scripts/e2e-linux.sh --grep …`), since the shared specs run on Linux and see `other` branches. Report in ≤350 words.

### 1. Characterize macOS text

- **Scope**: a frozen baseline of today's macOS-visible values and a test that holds every later milestone to it; the
  friendly-error golden asserted under macOS.
- **Intentions**: make "byte-identical on macOS" a proof (characterize-then-convert). The baseline covers every key in
  the proposal's `strings[].keys` across `en`, the 13 full locales, and the three overlays (about 8,000 values), at
  `apps/desktop/src/lib/intl/__fixtures__/macos-copy-baseline.json`, written by a one-shot generator committed beside
  it. `macos-copy-unchanged.test.ts` asserts, per (locale, key): the `macos` branch of the current value (or the value
  itself when it isn't a block) equals the baseline. ICU values compare ASTs (`intl-messageformat-parser`), raw values
  compare text. The "macos branch" is `pickPlatformBranch(value, 'macos')`, so a `{windows, other}` block compares its
  `other`. Allowed differences: the two accepted keys (English only; their locale values stay), and a pair map (survivor
  key's `macos` branch ↔ old `.mac` baseline entry; the menu pair's "mac" side is `menu.file.showInFinder`). The test
  can't import `platform-select.ts` before milestone 2: until then a non-block value compares as itself, and milestone 2
  wires the splitter in. `friendly-error-parity.test.ts`, `friendly-error-style.test.ts`, and every per-area English
  golden (`*i18n-parity.test.ts`, `*.parity.test.ts`, § Runtime) run their macOS-English assertions under
  `withPlatform('macos')`, per assertion where a shortcut glyph would move.
- **Landmines**: the test must read catalogs from disk (all locales), not the runtime (only `en` + one chain is loaded
  outside `test-setup.ts`'s preload). Don't snapshot pseudolocale `en-XA` (gitignored).
- **Test plan**: green on today's tree; then red-check it by temporarily editing one German value and one raw `errors.*`
  value, see it fail naming the key, revert.
- **DONE**: baseline + test committed, golden tests run under macOS (each one re-run green after wrapping), red-check
  noted in the commit body.

### 2. Runtime: the splitter and the injected `platform`

- **Scope**: `platform-select.ts`, `getMessage()`, the shared format call in `t()`/`tStringIn()`, `TranslationParams`,
  `i18n-tag-param-collision`'s reserved name (a Go check), and `macos-copy-unchanged.test.ts` switched to
  `pickPlatformBranch`. Frontend only.
- **Intentions**: § Design. One place injects, one place splits; no call site changes.
- **Landmines**: the locale-version rune read stays FIRST in `t()`/`getMessage()` (`intl/CLAUDE.md`). Don't add
  `platform` to `getCompiled`'s cache key. The runtime must not grow imports beyond `#lib/platform`
  (`test-setup-closure.test.ts`). Apostrophe-quoted braces in ICU branches (`'{'`) must not confuse the splitter.
- **Test plan** (red first): `platform-select.test.ts` (block vs plain, each platform, `linux` → `other`, missing branch
  → `other`, quoted braces, nested ICU in a branch, unbalanced raw body rejected). `messages.svelte.test.ts` with
  `_setCatalogForTests`: an ICU key and a raw key holding blocks, read under `withPlatform` × 3 with no cache reset
  between flips; `tStringIn` picks the branch; a `<Trans>` key with a block renders the right branch's snippet;
  `// @ts-expect-error` on `t(key, { platform: 'macos' })`.
- **DONE**: tests green, `intl/DETAILS.md` gains § "Platform variants" (canonical mechanism doc), `intl/CLAUDE.md` one
  must-know line, `messages/CLAUDE.md`'s raw-family line mentions the block.

### 3. Rust native strings learn the block

- **Scope**: `gen-native-strings-lib.ts` / `gen-native-strings.ts`, `native_strings.rs` (`NativeText`, `menu_t`), the
  `intl/platform/` seam's OS answer, `native_strings.gen.rs` regenerated.
- **Intentions**: § Design, Rust. The generated file stays cfg-free and `menu_t` callers unchanged.
- **Landmines**: never use `cmdr-menu`'s `Platform` for text (Windows reads Linux there). `menu_t` runs inside AppKit
  callbacks and must never panic. The Finder menu-item titles (`finderMenuTitles`, `settings.finderExtension.card`) are
  macOS-only and must not hold a block; assert it in the generator. The existing tests iterate `entries` as
  `(&str, &str)`: `menu_labels_end_with_the_ellipsis_character` must check every `PerOs` variant, and the overlay
  contract in `assert_speaks_its_menu_bar` (`assert_ne!(inherited, Some(*value))`) must compare whole `NativeText`s
  (derive `PartialEq`), never one OS's pick, or an overlay forking only its Windows text reads as dead weight. Adapt
  them; ❌ don't weaken them.
- **Test plan**: `gen-native-strings.test.ts` (a block emits `PerOs`, a plain value `All`, branch text escaped; a
  `{windows, other}` block emits `macos` = `other`); a Rust unit test of the pure `pick` over all three OSes, plus one
  asserting the host's arm answers the host's OS (runs on each CI OS lane, Windows included). Existing
  `every_shipped_locale_speaks_its_own_menu_bar` and the overlay test stay green.
- **DONE**: regenerated table identical in content for today's catalogs (no block exists yet), tests green, clippy
  green, `src-tauri/src/intl/DETAILS.md` § native strings updated, `scripts/CLAUDE.md` line touched if needed.

### 4. Checks: shape, parity, coverage, structure-aware tooling

- **Scope**: `platformBranches()` in `i18n-catalog-lib.ts`; new `i18n-check-platform-select.ts` +
  `scripts/check/checks/desktop-i18n-platform-select.go` (ERROR, plus its `registry.go` entry); parity, coverage, ICU,
  plural, the pseudolocale generator, the brief (`i18n-brief-lib.ts`'s token and placeholder listing), and the two
  overlay vitest files' `visibleTextOf` made branch-aware. Split into 4a (splitter-backed lib + platform-select +
  parity + ICU) and 4b (coverage, plural, pseudolocale, brief, overlay tests) if the context runs low.
- **Intentions**:
  - `platform-select` (ERROR): rule 4's shape and canonical spelling; legal branch sets; no duplicate branches; balanced
    raw bodies; no `'` directly before `{` or `}` in a raw body; no `{platform}` outside a top-level select (so no OS
    name leaks as text); no block in a boot-guard key, a `MANIFEST_KEYS` key, or the Finder menu key; every locale's
    branch set equals its source's.
  - Parity: per branch, ICU placeholder and tag sets (raw: `rawTokens` per branch) equal the source's same branch.
  - Coverage: new ERROR "untranslated platform branch": a FRESH locale key (hash matches) whose branch equals English's
    same branch while that English branch differs from the English branch macOS reads, unless
    `sameAsSourceJustification` is set. A stale key is never judged here: stale owns it. This is the net that keeps task
    28 honest after restamping. Overlays are exempt (an `en-GB` Windows branch equal to `en`'s is correct).
  - ICU: a raw block is shape-checked, not ICU-parsed per branch.
  - Plural: per branch (§ Checks: today's per-arg union masks a missing category). A branch byte-identical to the
    source's same branch is held to the SOURCE's categories; a translated branch to the locale's CLDR set.
  - Pseudolocale: `pseudoRaw` and the ICU path keep the block structure (branch names unaccented).
  - Overlay tests: `visibleTextOf` reads a raw block's branches through `platformBranches` (today's `{…}` strip would
    delete every branch body and pass silently).
- **Landmines**: overlays resolve their source through `resolveLocaleSource`; branch pairing must use that source, not
  `en`. Don't make coverage's new rule fire on today's catalogs (no blocks yet) or on stale keys.
- **Test plan**: fixtures under each check's existing `--messages-root` tests: a migrated key in the § How English lands
  shape is clean for parity, coverage, ICU, and platform-select, and stale for stale; a missing branch, an extra branch,
  a nested block, a `{platform}` interpolation, and a per-branch token mismatch each fail; the fresh-key English branch
  fails coverage and passes with a justification; a `ru` block whose `windows` branch carries English `one`/`other`
  passes plural while a translated branch missing `few` fails; `'{` in a raw body and a non-canonical
  `{platform,select,` each fail platform-select. `gen-pseudolocale.test.ts` covers a raw and an ICU block.
- **DONE**: all green on today's tree, `i18n.md` § Raw families and the check catalog (`scripts/check/CLAUDE.md` or
  wherever checks are listed) mention the new check.

### 5. Judging checks skip English carryover, and the lint

- **Scope**: dont-translate, quoted-labels, aria containment, term consistency, termbase, and mechanics made
  branch-aware (`i18n-trans-snippets` is Go and already regexes the whole value, so it sees the branch union: verify,
  likely no change); the ESLint rule `cmdr/no-platform-copy-branch` written and tested but not yet switched on (it would
  fail on the pair sites until milestone 7).
- **Intentions**: each check pairs a locale branch with the source's same branch. A locale branch byte-identical to the
  English same branch is pending translation (stale's job), so the judging checks skip it; otherwise "File Explorer" in
  an English windows branch would read as a missing German ruling. Dont-translate expects "Finder" only where the
  English branch has it. Aria containment pairs two KEYS, either of which may be a block while the other isn't, so it
  compares per rendered platform (`pickPlatformBranch(label, p)` inside `pickPlatformBranch(aria, p)` for each `p`),
  never per branch name; the same goes for the aria part of `i18n-en-overlays.test.ts`. The lint flags a conditional
  (ternary, `if`, `{#if}`) on `isMacOS()`/`getPlatform()` whose arms resolve copy (`t`, `tString`, `getMessage`,
  `<Trans>`, or a string literal shaped like a message key).
- **Landmines**: termbase and mechanics baselines must not move up. If a branch-aware check finds MORE on today's
  catalogs than before, the pairing is wrong. `eslint-plugins/` holds the house rules; copy the structure of a sibling
  (`no-error-string-match.js`) and its tests.
- **Test plan**: each check's fixture tests gain a block case (carryover branch skipped, translated branch judged); the
  ESLint rule's own tests (flags the `file-list.ts` and `ai-secret-error.ts` shapes, passes `hasCapability` gates and
  glyph formatting).
- **DONE**: all checks report the same counts on today's tree as before (say so in the commit body with numbers).

### 6. Gates, the resolved-copy file, the glossary, and the apply script

This is the biggest non-copy milestone; split it into 6a (gates) and 6b (the rest) if the agent's context runs low.

- **Scope (6a, gates)**: every "gate, don't translate" row from `copy-decisions.md` § Beyond wording, with the exact
  gate from `copy-row-verdicts.md`: the Quick Look hint toast and its setting, the Linux menu bar's Get info and Quick
  look rows (`macos_only` in `bar.rs`), the macOS Local Network block in Settings, `listing.showTags`, the Local AI
  option, and `showSafeSaveFiles`, the bundle row, and the full-screen rows where the verdicts confirm they mean nothing
  off macOS. New capability fields go in `src-tauri/src/platform/capabilities.rs` with an answer in every arm,
  `NO_CAPABILITIES`, the demo's `lib/demo/platform.ts`, and `pnpm bindings:regen`.
- **Scope (6b)**:
  - `docs/notes/windows-port/copy-final.json`: the resolved rows the script reads. Same keys as the proposal plus
    `branches: { macos, windows?, other }` (final English), `collapseFrom` for pairs, `status` (`apply`, `gated`,
    `none`, `open`), `sources` (which doc decided it). The milestone writes the schema, a generator that seeds it from
    the proposal, and resolves nothing beyond the pairs and the two `macosChanged` rows; milestones 7–11 resolve their
    own catalogs into it.
  - `docs/style-guide.md` § "Platform vocabulary": the canonical per-OS English glossary (decision file's glossary as
    corrected by `copy-glossary-evidence.md`): each term with its macOS, Windows, and Linux word, both registers for
    Trash and Quit, and the Linux actor rule. Every other doc points here. `copy-decisions.md` keeps the why.
  - `apps/desktop/scripts/apply-platform-copy.ts` (+ `.test.ts`): reads `copy-final.json` rows for named catalogs,
    builds the English block (dedupes branches, keeps `’`, raw rules for raw keys), rewrites `en` values, applies § How
    English lands to every locale and overlay (reading `@key.sourceHash` to decide which branches carry a translation),
    collapses pairs (survivor key, `@key` block carried from `.mac`, old keys removed in every locale), renames
    collapsed keys outside the catalogs (§ How English lands: termbase `exceptions`, i18n docs citations, the
    term-consistency allowlist, the capture report), writes `en-GB`/`en-AU` blocks for the agent to spell and restamp,
    preserves each file's key order (`i18n-catalog-lib.ts` has no writer: write like `sync-locale-keys.ts` does,
    `JSON.stringify(file, null, 2)` + newline, then `oxfmt` on anything that isn't a catalog), and prints a per-locale
    report (branches carried, branches English, pre-stale keys, overlay keys needing a spelling pass). Runs
    `pnpm intl:keys` and `pnpm intl:native-strings` at the end.
- **Intentions**: 478 rows by hand would drift; a script makes every locale edit mechanical and the review a diff of
  `copy-final.json`. TypeScript, not Go, deliberately: the catalog reader/writer, `sourceHash`, the overlay source
  resolution, and the ICU parser all live in `apps/desktop/scripts/` TS already, and a Go copy of them would be a second
  implementation of the parity-critical parts.
- **Landmines**: gated surfaces that E2E specs open on Linux (Quick Look hint, menu rows) need those specs adjusted, not
  skipped. A `split-feature` row's strings stay macOS-only; don't add branches to them.
- **Test plan**: 6a: component and unit tests under `withCapabilities()`; `desktop-e2e-linux` for touched specs. 6b:
  script fixture tests (plain row, Windows-only change, Linux-only change, raw row with `{system_settings}` in `macos`
  only, pair collapse with a locale missing `.other`, a pair key named in a fixture `terms.json` and `decisions.md`,
  `es-419` fork whose `es` branch carries a Spain-only form, `en-GB` fork, pre-stale key, `{windows, other}` row); then
  a dry run over every catalog reporting what it would do, without writing.
- **DONE**: gates landed and tested, glossary written, script tested, `copy-final.json` seeded.

### 7. Collapse the 14 pairs, switch on the lint

- **Scope**: the pair rows through the script; their call sites (§ Existing pairs) lose the ternary;
  `SHOW_IN_FILE_MANAGER_KEY` retires; `cmdr/no-platform-copy-branch` turns on.
- **Intentions**: the survivor name is the pair's stem without the suffix (`commands.fileShowInFinder.label`,
  `errors.write.fileLocked.suggestion`, `ai.secretError.keyringBody` folded into its keychain twin's neutral name), so
  the key names the UI concept, not an OS. `ai.secretError.keyringBody` serves two pairs today: resolve it into two
  survivors. The menu pair has no suffix to drop (`menu.file.showInFinder` + `menu.file.showInFileManager`): its `.mac`
  side is `showInFinder`; pick the survivor name in `copy-final.json` (review note 3) and name it in the commit.
- **Landmines**: `transfer-error-messages.ts` builds keys through `w(…)` and a `mac ?` helper; the dynamic-prefix
  allowlist of `desktop-message-keys-unused` may name these: it shrinks, never grows. Run `sync-locale-keys.ts` after
  and confirm it's a no-op. The `ai.secretError.genericBody` accepted change lands here or with its catalog in milestone
  11; pick one and record it. This is the first milestone where `i18n-en-overlays.test.ts`, termbase schema,
  `i18n-citations`, and `message-screenshots-fresh` can see a renamed or migrated key: run all four before committing.
  `Label::PerPlatformKey` loses its only user with `SHOW_IN_FILE_MANAGER_KEY`; remove it, and move `cmdr-menu`'s
  `bar_test.rs` text goldens to the survivor key (a rename, not a loosening).
- **Test plan**: the call sites' tests under `withPlatform` × 3; `macos-copy-unchanged.test.ts` via the pair map; the
  Rust menu test for the Show-in row on each seam arm.
- **DONE**: zero `isMacOS()` copy branches, lint on, stale lists exactly the survivors.

### 8. Errors, listing family

- **Scope**: `errors.listing.*` (44), `errors.provider.*` (16), `errors.git`, `errors.eject`, `errors.mount`,
  `errors.mutation.*` (about 68 rows, about 50 `select`): resolve into `copy-final.json`, apply, fix tests.
- **Intentions**: the decision file's "errors only macOS produces today" rule: write Windows and Linux variants where
  the verdicts say what they should be, keep macOS text (no block) for the rest. Add sibling goldens
  `__fixtures__/friendly_error_golden.windows.json` and `.linux.json`, generated ONCE from the factories after the
  migration, checked against `copy-final.json` by the agent, then frozen with the same "never regenerate" rule; the
  parity test asserts each under its platform.
- **Landmines**: `escapeMarkdown` stays the XSS boundary in every branch; tokens are per branch. The frozen macOS golden
  must stay green untouched.
- **Test plan**: golden parity × 3 platforms; `friendly-error-style.test.ts` over every branch (the style rules: no
  "error"/"failed", no "just/simple/easy"); `macos-copy-unchanged.test.ts`.
- **DONE**: rows resolved and applied, goldens green, `error-messages/CLAUDE.md` + `DETAILS.md` say how a platform
  branch is added and how the sibling goldens are maintained.

### 9. Errors, write family, and file operations

- **Scope**: `errors.write.*` (40, minus the pairs from milestone 7), `fileOperations.*` (26), `transfer.*` (4),
  `queue.*`, `operationLog.*`, `suggestedOps.*`, `lowDiskSpace.*`.
- **Intentions**: the Trash rows are the bulk: labels say "Delete" / "Delete permanently" on Windows, sentences name
  "the Recycle Bin" (glossary). Toasts with plurals keep their `plural` inside each branch.
- **Landmines**: Playwright `file-operations.spec.ts` and friends assert trash copy on both OSes; adjust per platform.
  F8 / Shift+F8 labels show in the function-key bar, palette, and menus: one key each, check every surface. A Linux
  (`other`) branch that says "Trash" in a key `en-GB`/`en-AU` fork fails `i18n-en-overlays.test.ts` ("no American trash
  in overlay copy", pinned key list) unless the overlay's `other` branch gets a British word (review note 1); its
  Windows branch reads "Recycle Bin", which the lowercase-` bin` ruling test leaves alone.
- **Test plan**: transfer dialog and toast tests × 3 platforms; `desktop-e2e-linux` for the touched specs.
- **DONE**: as milestone 8, plus `file-operations/` docs touched.

### 10. Settings

- **Scope**: `settings.*` (92 rows: 41 `select`, 11 `split-feature`, 4 `neutral`, 2 `code`, the rest `none`).
- **Intentions**: settings descriptions follow the glossary; `split-feature` rows were gated in milestone 6a, so only
  their descriptions might move. Settings search indexes English through `tStringIn`: a Windows user searching "Recycle
  Bin" must find the row (test it).
- **Landmines**: `settings-search.i18n.test.ts` pins manifest labels; the search index caches by locale, and platform is
  constant per window, so no cache change.
- **Test plan**: section component tests × 3; the search test above.
- **DONE**: as milestone 8, `settings/` docs touched.

### 11. Everything else

- **Scope**: `fileExplorer` (38), `servers` (24), `commands` (26), `menu` (13, Rust-drawn), `mtp`, `adb`, `onboarding`
  (34), `main` (31), `ai` (9), `askCmdr` (12), `updates`, `shortcuts`, `crashReporter`, `errorReporter`, `downloads`,
  `search`, `queryUi`, `viewer`, `ui`, `goToPath`, `multiRename`, `notifications`, `stateRecovery`, `common`. Split into
  11a (`fileExplorer`, `servers`, `commands`, `menu`, `mtp`, `adb`) and 11b (the rest) if needed.
- **Intentions**: `menu.*` rows regenerate `native_strings.gen.rs`; Quit rows follow the two registers (decision file:
  "Exit Cmdr" as the action, "closed" in narration; Linux "Quit" in both). `code` rows (runtime data) follow the
  verdicts. Any `main.oldWebkit.*` change must respect the boot-guard landmine.
- **Landmines**: `commands.*` labels feed the palette's fuzzy search and the shortcut settings: test both. The home
  folder command gets "Go to user folder" on Windows with "home" kept as a palette search alias (decision file).
- **Test plan**: as milestone 10; Rust menu tests per seam arm for changed menu rows.
- **DONE**: every `apply` row in `copy-final.json` applied; every `open` row listed in `copy-decisions.md`.

### 12. `@key` descriptions and the translator rule

- **Scope**: the 349 `descriptionOnly` rows plus every migrated key's description;
  `docs/i18n/translator-instructions.md` and `docs/guides/i18n-translation.md`.
- **Intentions**: a key shown off macOS no longer tells translators "use your language's macOS word"; a key with a block
  says which branch is which surface. The translator rule, canonical in `translator-instructions.md` (embedded in every
  brief): "In a platform select, each branch uses that OS's own localized term (Windows' own word for the Recycle Bin,
  File Explorer, Settings), sourced from that OS's localization, never the macOS term carried over."
  `i18n-translation.md` § New feature points there and says how a branch-level stale key is worked. `@key.screenshot`
  shows the macOS branch only: the description says so where it matters. Add the English half of the new termbase
  concepts (`recycle-bin`, `file-explorer`, `this-pc`, `credential-manager`, `user-folder`, `task-manager`, `exit`, …)
  to `docs/i18n/concepts.json`; per-locale rulings are task 28.
- **Landmines**: an `en` description edit doesn't change the value hash, so it never marks a translation stale; don't
  restamp anything. Use a script for the 349 snippet rewrites, then read the diff.
- **Test plan**: `i18n-citations` and doc checks green.
- **DONE**: no `@key` description of a key shown off macOS steers to macOS terms (grep the 349 snippets).

### 13. Close out

- **Scope**: docs, the plan's bookkeeping, the review page for David.
- **Intentions**: `docs/specs/windows-port-plan.md` task 27 marked done with a two-line summary; `docs/specs/DETAILS.md`
  § The specs lists this plan (added with it) and later loses it on wipe; `docs/design-principles.md` §
  "Platform-native, not generic" points at the glossary; `docs/guides/i18n.md` § "Add or change a user-facing string"
  gets a line on platform blocks; every touched `CLAUDE.md`/`DETAILS.md` re-read. The lead publishes a review page
  rendering `copy-final.json` (key, surface, macOS / Windows / Linux English) for David: the plan entry's "David reviews
  the English". `apply-platform-copy.ts` is deleted (git keeps it); `macos-copy-unchanged.test.ts` and its baseline STAY
  until task 28 finishes, since they keep translator agents off the `macos` branches, and task 28's entry says to delete
  them at its end.
- **Test plan**: `pnpm check --only-slow` (backgrounded by the lead), then the adversarial conformance review and the
  docs audit (`docs/guides/multi-agent-refactors.md` § Close out).
- **DONE**: landed via `pnpm land`, task 28 ready to start.

## Out of scope

- Translating anything (task 28).
- Key glyphs in prose, the eight values with literal ⌘/⌥/⌃/⇧ (wait for #1102, task 26).
- The Linux `desktop` value and GNOME/KDE wording (`linuxDesktops`, task 59).
- Per-OS `LocalizedSystemStrings` arms, a Windows "Pin to taskbar" nudge, new Linux/Windows Quit/Exit menu rows, a "Run
  this program?" ask for `.exe`: new features, not variants.

## Open questions for David

1. Releases between tasks 27 and 28: (a) pause releases until task 28 restamps (recommended), or (b) a stale-check class
   for "macOS branch current" that strict mode lets through while Windows and Linux have no public build? (b) changes a
   release gate.

## Invariants (the close-out review checks each)

1. Every locale renders byte-identical macOS text, except the two accepted English keys
   (`macos-copy-unchanged.test.ts`).
2. Every platform-variant value is one whole-value block with branches ⊆ `{macos, windows, other}`, `other` present, no
   duplicates, and the same branch set in every locale (`desktop-i18n-platform-select`).
3. `platform` reaches ICU only through the runtime's one format call; no caller can pass it (type-level).
4. Raw values are split by the one splitter in `platform-select.ts`, in the runtime, the generator, and the checks.
5. The generated Rust table carries no OS `cfg`; the OS answer comes from `intl/platform/`.
6. No `isMacOS()` or `getPlatform()` chooses copy (`cmdr/no-platform-copy-branch`).
7. No allowlist, baseline, golden, or severity loosened; every migrated key in a full locale and `es-419` is stale until
   task 28; `en-GB`/`en-AU` forks are fresh (restamped English).
8. Gated surfaces ask `hasCapability()`, with an answer in every capability arm.
9. No string translated; no `sourceHash` restamped outside `en-GB`/`en-AU` (§ How English lands).

## Review notes

Mechanism review against `f0e0d8639b`. The plan text above already carries the clear fixes; these are the judgment calls
left open.

1. **English overlays on Windows and Linux.** `i18n-en-overlays.test.ts` forces `en-GB`/`en-AU` to fork every new branch
   that uses an American form and to restamp, so the plan now treats them as English that task 27 writes. That is a
   narrow exception to "no restamp"; David may prefer it the other way round (overlays stay stale, the hash assertion
   learns platform blocks), but that loosens a test, so the plan doesn't do it. Open wording: what a British or
   Australian Linux branch calls the Trash (the test bans "trash" in overlay copy; GNOME's and KDE's en_GB words are
   unverified here). Route it to `copy-glossary-evidence.md` before milestone 9.
2. **The demo reads Linux words for Windows visitors.** `lib/demo/platform.ts` answers `macos` for a Mac user agent and
   `linux` otherwise, so getcmdr.com's demo shows a Windows visitor the Linux branch. Fine while no Windows build ships;
   a one-line UA check would make the demo speak Windows once one does.
3. **The menu pair's survivor name.** Rule "stem without the suffix" doesn't apply to `menu.file.showInFinder` /
   `menu.file.showInFileManager`. Keeping `showInFinder` names an OS in the key; a neutral `menu.file.reveal` (or
   `showInFileManager`) names the concept. Either works mechanically; it's a naming call.
4. **Plural strictness after task 28.** The per-branch plural rule makes `i18n-plural` stricter than today on real
   catalogs only where a translated branch already lacks a category another branch has; on today's tree there are no
   blocks, so nothing changes until milestone 7. If milestone 7+ surfaces real misses in carried translations, they are
   pre-existing translation bugs exposed, not new ones; listing them for task 28 (not baselining them) keeps rule 3.
5. **Milestone size.** Milestones 4 and 6 are each near one agent's limit now (4 grew the brief and the overlay tests);
   both carry an a/b split line. Milestone 7 is small in rows but touches Rust (`cmdr-menu`), TS call sites, catalogs,
   four docs trees, and the capture report in one commit, because a half-collapsed pair leaves `message-keys-fresh` or
   the termbase red; keep it one commit.
6. **Not found, so not planned for**: no catalog value has an arg or tag named `platform` today; no Rust reads catalog
   values outside `native_strings.gen.rs` (`resolve.rs` only lists catalog dirs); the website and api-server don't read
   desktop catalog values (`apps/website/src/lib/trust-sources.json` lists `messages/en/{settings,updates}.json` and
   `messages/*/settings.json` as trust-page sources, so milestones 10 and 11 queue the `/trust` MDM and accessibility
   sections for the next release's trust review, which is expected, not a CI failure);
   `TranslationParams & { platform?: never }` accepts `Record<string, string>`-typed params built elsewhere and rejects
   a literal `platform` (checked with the repo's `tsc` 7.0.2).

## Lead rulings on the review notes

1. The overlay exception stands: `en-GB` and `en-AU` are English, so task 27 writes and restamps them. The British and
   Australian Linux word for the Trash is mined from the pile (`en-GB/gnome-nautilus`, `en-GB/kde-dolphin`, and the
   `en-AU` siblings in the main clone's `_ignored/i18n/`) by the milestone 9 agent and recorded in
   `copy-glossary-evidence.md`; Windows overlays say "Recycle Bin".
2. The demo answers `windows` for a Windows user agent. It goes in with milestone 2: a Windows visitor reading Linux
   words is the "leftover platform" treatment the design principle rules out.
3. The menu pair's survivor is `menu.file.showInFileManager`: the key names the concept.
4. Plural gaps that the stricter rule exposes in carried translations are listed for task 28, never baselined.
5. Milestones 4 and 6 run as their a/b halves, one agent each. Milestone 7 stays one commit.
