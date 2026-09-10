# OS locale resolution details

Depth behind `CLAUDE.md`. This module answers the two questions macOS asks its users separately: given the message
catalogs we ship and the user's language preferences, which catalog should the app open, and whose conventions should
it format dates and numbers in?

## Why the resolvers live in Rust

Three consumers need the answer and two of them run before the webview exists: the native menu bar (built during
`setup`) and the "Cmdr is already running" alert (fires before any window). Putting the real resolver in TypeScript
would mean a second one in Rust for those two, which is two implementations of one rule, drifting apart. It also matches
the project's smart-backend / thin-frontend principle: the frontend consumes the answer, it doesn't compute it.

The cost is that the script guard needs CLDR likely-subtags data, which the webview gets free from
`Intl.Locale.maximize()` and Rust does not. That's what the generated table buys.

The formatting half has a blunter reason: the webview CAN'T answer it. See "The formatting tag" below.

## The walk

`resolve_ui_locale(preferences, shipped)` takes each preference IN ORDER and returns the first catalog the user can
read. The order is the user's own fallback plan, so one preference is fully exhausted before the next gets a turn.

Matching one preference against the table is two gates and then a walk. The gates say which catalogs this reader can
read at all:

- **Same language.** The base subtag has to match, which is what lets `fr-CA` land on `fr`, `pt-PT` on `pt`, and `en-CA`
  on `en`.
- **Same script.** See below.

The walk then takes the first node of `ancestor_chain` that one of those catalogs answers for, so the most specific
catalog on the way home wins.

### The ancestor chain

`ancestor_chain(tag, script)` lists the nodes to try, most specific first: the tag itself, then its ancestors, then the
tag CLDR's likely subtags would have maximized it to.

An ancestor is **CLDR's `PARENT_LOCALES` override** where CLDR states one, and the tag minus its last subtag otherwise.
The overrides are why the chain exists. Truncation alone sends `en-NZ` to `en`, US English, and a New Zealander reads
"Trash"; CLDR parents it to `en-001`, World English, which our `en-GB` catalog answers for, and they read "Bin". Around
110 regions reach an overlay this way, some in two hops (`en-AT` → `en-150` → `en-001`). ❌ Don't add a region table
beside this: replacing exactly that hand-kept map is what the CLDR data buys.

A `und` override means the locale has **no parent at all**, and the walk stops rather than truncating. It's how CLDR
spells the wall between `zh-Hant` and Simplified `zh`, the same wall the script gate enforces from the other side.

The `<language>-<script>` node comes last, and only when the walk hasn't already produced it. It's what lets `zh-TW`
reach the `zh-Hant` catalog: the tag names no script, its only ancestor is `zh`, and maximization is the one step that
says out loud that this reader reads Traditional.

**Which catalog answers for a node** is its own tag, plus anything in its `covers` list. Only `en-GB` carries one today
(`en-001`), because CLDR routes most regional English to a node nobody's Mac is ever SET to and nobody ships a catalog
under. `en-GB` and `en-AU` are both children of `en-001`, so no amount of CLDR data says which of them speaks for the
group; the generator's `CATALOG_COVERS` picks `en-GB` and carries the reasoning, and macOS answers the same question
the same way.

### Where we differ from macOS, and the test that bounds it

`macos_fallback_test.rs` asks `CFBundleCopyLocalizationsForPreferences` the same question we answer (it's a pure
function over two tag arrays, no bundle involved) for every English locale identifier macOS knows, and holds us to one
directional invariant: **we are never further from home than macOS puts them.** If Finder finds a reader a regional
overlay, so must we.

The converse is allowed and happens in both directions:

- We are more precise for ~20 regions CLDR parents to `en-001` and Apple's table doesn't (`en-IL`, `en-MG`, `en-RW`, and
  friends). Being ahead of the OS is not a bug.
- We stop at `en` for six tags Apple sends to `en-GB` (`en-AL`, `en-BD`, `en-BG`, `en-BN`, `en-GR`, `en-RU`): CLDR ships
  no English locale for those countries, so `parentLocales.json` says nothing. None is offered in the macOS language
  picker. The test pins that set as an EQUALITY, so it fails if the gap grows OR closes.
- Apple sends `en-NZ` to `en-AU` and we send it to `en-GB`. Both say "Bin", and CLDR backs ours.

### Known gap: the chain doesn't reach the Linux webview

On Linux `get_os_locales` answers `ui: None` (§ The frontend contract), so the webview picks its own catalog from
`navigator`'s tag by plain truncation, while the native surfaces go through `os_ui_locale` and this chain. A
`LANG=en_NZ.UTF-8` session therefore gets "Bin" in the menu bar and "Trash" in the panes. macOS is unaffected: Rust
answers the `ui` half there, and every surface reads the same answer.

Closing it means letting Rust answer the `ui` half on Linux too, which is a deliberate change to that contract rather
than an oversight, and nobody runs Cmdr on Linux outside the E2E suite yet.

`None` means nothing matched, and the caller uses English. That is NOT the same as matching `en`, which stops the walk
deliberately: a user who listed English above Swedish wants English, not the next-best translation.

Tags are normalized before comparison (trimmed, `_` folded to `-`, lowercased): macOS reports BCP-47 (`hu-HU`) but the
same list reaches us through paths that use the POSIX `hu_HU` spelling, and neither casing is guaranteed.

## The script guard, and why regional fallback survives it

A fallback is only a kindness when it lands somewhere the reader can actually read. Our `zh` catalog is Simplified, so
handing it to a `zh-Hant-TW` reader is worse than handing them English: English is at least a language they chose to
list. `docs/i18n/script-decisions.md` records nine languages with a script split (`zh`, `sr`, `uz`, `kk`, `mn`, `az`,
`pa`, `bs`, `be`), so this is not a one-off for Chinese.

Regional variants are the opposite case and DO fall back, deliberately: `pt-PT` reads the Brazilian `pt` catalog, and
`en-CA` reads US `en`. Reading a sibling dialect is a small friction next to reading a language you don't speak: the
first is a papercut a fast-follow catalog fixes, the second is a wall. ❌ Don't collapse the two by blocking regional
fallback.

**A regional catalog can also exist and win.** `en-GB` and `en-AU` ship as OVERLAYS: each carries only the keys it
forks ("Bin" for "Trash", `-ise` for `-ize`), wins on the ancestor chain above, and resolves every other key up to `en`
through exactly the regional fallback here. So the fallback isn't the regional reader's consolation prize; it's what
lets a 151-key overlay stand in for a 3,263-key catalog. `docs/i18n/language-selection-decisions.md` is the roster of
which variants ship a catalog and which still fall back to their base, and `docs/guides/i18n.md` § Overlay catalogs (regional variants) is
how an overlay is built and checked.

**Which regions reach one is CLDR's answer, not a list we keep.** An overlay serves far more than the region in its
name: `en-GB` also answers for the ~110 regions CLDR parents to `en-001`, `en-NZ` and `en-IE` and `en-ZA` among them.
That's the ancestor chain's job, above.

### One rule, three layers

This section is the canonical statement of the rule; the other layers point here rather than restating it. All three
have to agree, or one of them puts text on screen that another has already ruled unreadable:

1. **Rust auto-selection** (`match_shipped`, below): which catalog an OS preference list opens.
2. **The frontend per-key fallback** (`resolveRaw` in `src/lib/intl/messages.svelte.ts`): a catalog with a gap resolves
   that key up its ancestor chain, and the chain skips a different-script ancestor. Without this, shipping `zh-Hant`
   would silently serve Simplified text for every key it hadn't translated yet.
3. **The i18n check layer** (`resolveLocaleSource` in `apps/desktop/scripts/i18n-catalog-lib.ts`): whether a catalog is
   an OVERLAY of another (carrying only its forks) or a full translation. A different-script variant is a full
   translation, precisely because it can't inherit. See `docs/guides/i18n.md` § Overlay catalogs (regional variants).

Layers 2 and 3 share one implementation, `inheritableAncestors` in `apps/desktop/src/lib/intl/locale-inheritance.ts`
("the ancestors that exist AND read the same script"). Layer 1 can't call `Intl`, so it reads the same CLDR answers off
the generated table below, which the codegen builds with that module's `likelyScript`. So the script facts have one
source, and `shipped-locales-fresh` keeps Rust's copy of them current.

The AUTO-SELECTION guard applies to auto-selection only: an explicit pick in the Settings picker is the user's business
and carries no such check. The per-key fallback chain (layer 2) is not a selection and always applies, however the
locale was chosen: once a reader is on `zh-Hant`, no missing key may fall through to Simplified.

`script_of(tag, entry)` reads the preference's script from three sources, most explicit first:

1. the tag's own script subtag (`zh-hant-tw` → `hant`),
2. its region, when that region's likely script differs from the language default (`zh-tw` → `hant`),
3. the language's default script (`zh` alone → `hans`).

In practice macOS emits an explicit script for Chinese (`zh-Hans-CN`, `zh-Hant-TW`), so branch 1 usually decides;
branches 2 and 3 cover hand-set and imported preference lists.

## The generated tables

`apps/desktop/scripts/gen-shipped-locales.ts` (pure logic in `gen-shipped-locales-lib.ts`, run via
`pnpm intl:shipped-locales` from `apps/desktop/`) emits `shipped_locales.gen.rs`, which holds two tables.

### `SHIPPED_LOCALES`

Built from the catalog directories under `src/lib/intl/messages/` via `listLocales()`, with the script facts from
Node's `Intl.Locale(tag).maximize()`. Per catalog it records:

- `tag`: the directory name VERBATIM. The resolver hands it straight back to the frontend, which keys its catalog map on
  the directory name, so the spelling has to survive the round trip. Comparisons are case-insensitive.
- `script`: what a reader of this catalog reads (`zh` → `hans`).
- `default_script`: the bare language's likely script. Differs from `script` only for a catalog that names a script
  itself (`zh-Hant`), which is exactly the case where reading `script` as the language default would be wrong.
- `region_scripts`: the regions whose likely script differs from `default_script`. Empty for every Latin-script
  language; `zh` carries the Traditional set (TW, HK, MO, plus the overseas-community regions CLDR lists, and their UN
  M49 numeric equivalents).
- `covers`: the CLDR nodes this catalog answers for besides its own tag. Only `en-GB` has one (`en-001`); the
  generator's `CATALOG_COVERS` declares it and carries the reasoning.

Regions are enumerated (the 676 two-letter combinations plus the 1,000 three-digit M49 codes) rather than listed,
because CLDR's region set drifts with every ICU update and a hand-kept list would quietly stop covering new codes.
Unknown codes maximize to the language default and contribute nothing. Everything but `tag` is emitted lowercase,
matching the normalized tags the resolver compares.

### `PARENT_LOCALES`

CLDR's parent-locale overrides as `(child, parent)` pairs, read from the `cldr-core` package: CLDR's own JSON
distribution, pinned like any other dependency. `Intl` exposes likely subtags but not parents, so this file is the only
way to learn that `en-NZ`'s parent is `en-001`. What the resolver does with them: § The ancestor chain.

Filtered to the base languages we ship a catalog for. That's safe because the resolver rejects another language's
catalog before it ever consults this table, and it keeps the entries that go live later on their own: `es-MX` →
`es-419` and `pt-AO` → `pt-PT` are inert while only the base catalogs ship.

### Guards

Neither table is rustfmt's business: both carry `#[rustfmt::skip]` and the generator owns their layout byte for byte,
because `shipped-locales-fresh` invokes the script directly and any reformatting elsewhere would make the two disagree
forever. Two guards keep them honest, because a stale table leaves a new locale both unreachable AND unguarded:

- `shipped-locales-fresh` (`scripts/check/checks/desktop-shipped-locales-fresh.go`) regenerates and diffs, restoring the
  original under `--ci` and keeping the regenerated file on a local run (same auto-fix UX as `oxfmt`).
- `the_generated_table_covers_every_shipped_catalog` compares the table against the catalog dirs on disk, so the failure
  reaches whoever runs the Rust tests too.

The `en-XA` pseudolocale (accented, inflated English for overflow testing) is dropped by the generator rather than
filtered in Rust. Auto-selection draws only from the table, so its absence is what makes it unreachable, and there's no
runtime check that a later refactor could quietly remove.

## The formatting tag

`format_locale.rs` composes `<language>[-Script]-REGION` from `NSLocale`, because WebKit hands the webview a locale
with the user's region override stripped out. On a Mac set to US English with a Swedish region
(`AppleLocale = en_US@rg=sezzzz`) Foundation writes `2026-08-19, 14:05` and `1 234 567,89` while the webview resolves
to plain `en-US` and writes `08/19/2026, 02:05 PM` and `1,234,567.89`. Handing the extension back explicitly doesn't
help (`en-US-u-rg-sezzzz` resolves straight to `en-US`); naming the region as a real SUBTAG does, and `en-SE`
reproduces Foundation exactly. The full measurement and its evidence anchor live once, in
`apps/desktop/src/lib/intl/DETAILS.md`.

The three parts come from `NSLocale::autoupdatingCurrentLocale`, whose `countryCode` returns the `rg` subtag's value
when there is one (`SE` here, not `US`), which this machine confirms (`languageCode` `en`, `countryCode` `SE`,
`scriptCode` nil; macOS 26.5.2, 2026-08-19). `countryCode` rather than `regionCode`, the better-named property
Foundation's header calls its replacement: that one is `API_AVAILABLE(macosx(14.0))`, and Cmdr ships
`minimumSystemVersion: 12.0`, so on 12 or 13 the selector doesn't exist. It raised an unrecognized-selector exception
inside the Tauri setup hook, which runs on the main thread inside tao's `extern "C"` `did_finish_launching`, so the
exception unwound out of a `nounwind` frame and aborted the process before a window ever opened (every macOS 12/13
launch, v0.39.0 and v0.40.0, GitHub issue #54). The two answer identically, `rg` override included (`en_US@rg=sezzzz`
answers `SE` to both; macOS 26.5.2, Foundation via Swift, 2026-08-26). `objc2` carries no availability information, so
nothing in the type system stops a call like that; the `macos-availability` check (`scripts/check/checks/DETAILS.md` §
"macOS availability") is what catches the next one. The AUTOUPDATING locale rather than `currentLocale`: this is read
from a live-change path, and the autoupdating one is the locale Foundation documents as tracking the user's
preferences, so there's no question of a cached snapshot outliving the change that triggered the read.

The script rides along only when Foundation names one, which is exactly when dropping it would change the answer
(`zh-Hans` and `zh-Hant` format dates differently). It's nil for every locale whose script its language implies, so the
everyday tag stays the short one.

Any part we don't recognize (not 2-3 letters for a language, not 4 for a script, not 2 letters or 3 digits for a
region, or the `und` that means Foundation doesn't know) composes `None`. The frontend then falls back to the webview's
own locale, which is at least a working answer; a malformed tag is not, since `Intl` would either throw or quietly
resolve to something nobody chose.

## Following a live language or region change

`'system'` tracks the CURRENT system language, and the formatters track the CURRENT region. macOS nudges people to
restart their apps after a change like that; doing the right thing without asking is better, and it costs one observer.

`live_locale.rs` registers two notification observers from the Tauri `setup` hook, both feeding one `LocaleWatcher`:

- `AppleLanguagePreferencesChangedNotification` on `NSDistributedNotificationCenter`. **This is the one that carries a
  language change.** Undocumented, on the same terms as the accessibility notification `text_size.rs` watches; if Apple
  stops posting it the language still resolves correctly at the next launch. The System Settings pane that owns the
  setting posts it (`/System/Library/ExtensionKit/Extensions/Localization.appex` carries the literal name, next to its
  `AppleDate…` / `AppleNumber…` / `AppleTime…` siblings; verified on macOS 26.5.2, `strings`, 2026-08-19).
- `NSCurrentLocaleDidChangeNotification` on the default `NSNotificationCenter`. **This is the one that carries a REGION
  change**, and it doubles as the documented fallback if the undocumented one above ever goes away. It tracks
  `AppleLocale` (the region and format settings), not `AppleLanguages`: with `AppleLanguages` flipped to
  `[de-DE, en-US]`, `Locale.current` stayed `en_US@rg=sezzzz` (verified on macOS 26.5.2, Swift observer on both
  centres, 2026-08-19). So on today's macOS each notification carries one half of the answer, and both halves matter.

**Gotcha: `defaults write -g AppleLanguages` posts NOTHING.** The value changes and a re-read sees it immediately, but
no notification reaches either centre (same measurement as above), so `defaults write` alone can't test this path. To
exercise it, write the preference and then post the distributed notification the way System Settings does:
`DistributedNotificationCenter.default().postNotificationName(...)` from a throwaway Swift script.

A region-only change is exactly what `NSCurrentLocaleDidChangeNotification` is for, and it reaches the app: the watcher
compares the whole `OsLocales` pair, so a moved formatting tag announces even though the language half is untouched.
Nothing in the copy changes; every date and grouped number in the window does.

Overlap between the two costs nothing, because the watcher applies two filters in order:

1. **Collapse the burst.** One System Settings change posts several notifications (language, region, and calendar are
   separate preferences), and both centres may carry the same one. The first notification arms a settle timer
   (`SETTLE_WINDOW`, 300 ms); the rest ride on it, because the timer re-reads live state when it fires. So a burst
   costs one `apple_languages()` read, not one per notification.
2. **Compare the answer.** The fresh resolution is checked against the one the app is running on (seeded at
   registration, then updated per announcement), and a match emits nothing. Most locale notifications don't move the UI
   language at all.

What survives both filters is an `OsLocalesChanged` event carrying the fresh pair. The emit site is the ONE place that
knows the answer moved, which makes it the seam for anything else that has to be rebuilt in the new language: the
native menu bar rebuilds there, on the main thread, guarded by `refresh_active_locale` so a region-only change (or a
language change under a pinned `appearance.language`) costs nothing.

Linux gets a no-op. The desktop language lives in the session's environment (`LANG` / `LC_MESSAGES`), fixed for the
life of the process, and no portal or D-Bus name broadcasts a change; a user who changes their language gets it at
their next login, which is also when Cmdr restarts.

## The frontend contract

`get_os_locales` is the only way out of this module. It returns an `OsLocales` pair, `{ ui, format }`, each half an
`Option<String>`:

- **`ui`, macOS**: always `Some`, falling back to `"en"` when the walk finds nothing, because English IS the answer when
  the user reads no language we ship. **Off macOS**: `None`, meaning "no OS preference list here".
- **`format`, macOS**: `Some` unless the region is missing or unreadable. **Off macOS**: `None`.

A `None` half means "no OS answer": the frontend reads it as "no override" and lets the webview default stand, which is
the right behavior on Linux.

One command rather than two, because the frontend wants both at the same moment and a second round-trip on the startup
path would buy nothing.

`os-locales-changed` is the same pair, pushed. `watchSystemLocales` (`src/lib/intl/os-locales.ts`) adopts it (the
language into its own cache, the formatting tag straight into `locale.ts`) and re-applies `appearance.language`, which
re-renders the window through the message runtime's version rune. It drops an event whose pair matches the cached one,
a second guard behind the backend's, because a needless bump re-renders every open `t()` in the window.

The frontend half (`src/lib/intl/os-locales.ts`) fetches this once per window and resolves the `'system'` setting
through it. Startup pays no serialized round-trip: the main window fires the fetch before awaiting the settings store,
so the two overlap. The backend side of the call measures ~65 µs (debug build, 1,000 iterations, dominated by the
`NSUserDefaults` read), so there's nothing to cache here; keeping it uncached is also what lets the live observer
re-read it.

## The strings Rust draws itself

Three surfaces never pass through the webview, so they can't use `t()`:

- the **native menu bar** and every native context menu, built during `setup` and redrawn by AppKit whenever it likes;
- the **main window's OS-level title** (`licensing::get_window_title`), set before the frontend has loaded. The in-app
  title bar is NOT one of these: it reads the same key through the webview's `getMessage()`, derived from the licence
  status, which is what makes it follow a live language switch. Both sides read `licensing.windowTitle.personalUse`, so
  they can't drift in wording; only the OS-level one is pinned to the launch language;
- the **already-running alert** (`instance_lock.rs`), which fires before a webview exists at all. That one is the
  strongest argument for the whole arrangement: no IPC hand-off could ever reach it.

They still live in the same message catalogs as everything else, so translators work one pile and one set of checks
covers the lot. `apps/desktop/scripts/gen-native-strings.ts` lifts the subset Rust needs into `native_strings.gen.rs`,
choosing it by KEY PREFIX (`NATIVE_KEY_PREFIXES`: `menu.`, `licensing.windowTitle.`, `main.instanceLock.`). Adding a
native string is adding a catalog key under one of those prefixes and regenerating; nothing hand-maintains a second
list. `pnpm intl:native-strings` regenerates, and `native-strings-fresh` diffs it the way `shipped-locales-fresh` does.

### Why `menu_t` is deliberately dumb

`menu_t(key)` finds the locale's row, binary-searches its sorted `(key, value)` pairs, and falls back to English, then
to the key itself. That's the whole thing. No ICU, no plurals, no formatting:

- **No panic, ever.** The menu is built inside AppKit callbacks that abort the process on a panic, so a typo has to cost
  a visibly wrong label rather than a crash. The typo is caught at test time instead, by
  `menu_t_literals_exist_in_the_english_catalog`, which parses every `menu_t` / `menu_t_with` call site in the crate and
  checks the key against the English table.
- **No ICU engine in the app process.** A menu label that seems to want a plural is a label to reshape. The one
  concession is `menu_t_with`, which does literal `{token}` replacement for the four labels that name what they act on
  (`Copy "photo.jpg"`, `Eject (Backup)` and its `(busy)` twin, `Preview (default)`); reshaping those would cost the user
  real information. It's the same raw substitution the `errors.*` family uses on the frontend, which is why
  `isRawKey()` classifies `menu.*` alongside it: an ICU-doubled `''` would render as two apostrophes on a real menu.
- **The active locale is cached**, because one menu build asks ~150 times in a row and the macOS half of the answer is
  an `NSUserDefaults` read each time. `refresh_active_locale()` is the only thing that moves it, and it reports whether
  it moved, which is exactly the signal a rebuild needs.

**Gotcha: `menu_t`'s fallback is two steps, `<locale>` then `en`, while the frontend walks the full inheritance chain**
(`inheritableAncestors`, `apps/desktop/src/lib/intl/locale-inheritance.ts`). The two agree for every catalog shipped
today, because `en-GB` and `en-AU` are overlays of `en` and their chain IS `en`. They will DISAGREE for the first
overlay whose base is not English: a `pt-PT` reader would get Portuguese everywhere except the native menu bar, which
would drop straight past `pt` to English for every key `pt-PT` doesn't fork. Teach `menu_t` the intermediate hop before
shipping `pt-PT` (the script and region facts it needs are already in `SHIPPED_LOCALES`).

### Which locale, and who decides

`LANGUAGE_PREFERENCE` holds the raw `appearance.language` SETTING, not a resolved tag, so `'system'` keeps meaning "the
language the user reads now": every refresh re-reads the OS. A pinned tag carries NO script guard (that guard exists to
stop auto-selection landing a Traditional-Chinese reader on the Simplified catalog; a user who picked a language picked
it), and an unrecognized pin falls back to English rather than to the OS, because "I picked something we don't ship" is
a broken setting, not a request to follow the system.

Two writers, and they agree by construction: `lib.rs` seeds it from `settings.json` during `setup` (before the menu is
built), and the frontend pushes the same value through `set_ui_language` once it loads and on every change. The push is
idempotent, since a rebuild only happens when the resolved answer moves.

**In the crate's unit-test build, `'system'` is English and the OS is never asked** (`native_strings::system_locale`).
The host's language isn't a fixture, so a name sort or a native string in a test answers the same on every machine, and
the first `NSUserDefaults` read in a bare test binary is expensive enough to fail a timed test under load
(`docs/testing.md` § "The host machine is not a fixture"). A test that needs another language pins it with
`set_language_preference` while holding `lock_active_locale_for_tests`. Integration tests under `src-tauri/tests/`
compile the library without `cfg(test)`, so they still follow the OS.

The `en-XA` pseudolocale is absent from the table on purpose. It's gitignored and regenerated, so including it would
make the generated file differ between a fresh clone and a machine that ran `pnpm i18n:pseudo` — permanent phantom
drift for the freshness check. A developer running in `en-XA` therefore sees an English menu bar over an accented app,
which is the honest and harmless outcome.
