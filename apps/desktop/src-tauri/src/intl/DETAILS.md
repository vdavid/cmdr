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

Matching one preference against the table is a single rule with two halves:

- **Same language.** The base subtag has to match, which is what lets `fr-CA` land on `fr`, `pt-PT` on `pt`, and `en-GB`
  on `en`.
- **Same script.** See below.

Among the catalogs that qualify, the most specific wins: `shared_subtags` counts the leading subtags a catalog tag
shares with the preference (0 unless it's a subtag-aligned prefix), and ties break toward the SHORTER tag. With both
`pt` and `pt-BR` shipped, `pt-BR` takes `pt-BR` and `pt-PT` takes plain `pt`. Nothing we ship today has a regional
sibling; the rule is pinned by tests so the day one arrives it behaves.

`None` means nothing matched, and the caller uses English. That is NOT the same as matching `en`, which stops the walk
deliberately: a user who listed English above Swedish wants English, not the next-best translation.

Tags are normalized before comparison (trimmed, `_` folded to `-`, lowercased): macOS reports BCP-47 (`hu-HU`) but the
same list reaches us through paths that use the POSIX `hu_HU` spelling, and neither casing is guaranteed.

## The script guard, and why regional fallback survives it

A fallback is only a kindness when it lands somewhere the reader can actually read. Our `zh` catalog is Simplified, so
handing it to a `zh-Hant-TW` reader is worse than handing them English: English is at least a language they chose to
list. `docs/i18n/script-decisions.md` records nine languages with a script split (`zh`, `sr`, `uz`, `kk`, `mn`, `az`,
`pa`, `bs`, `be`), so this is not a one-off for Chinese.

Regional variants are the opposite case and DO fall back, deliberately: `pt-PT` reads the Brazilian catalog and `en-GB`
reads "Trash" and `-ize`. Both are documented roster decisions (`docs/i18n/language-selection-decisions.md` lists them
as wave-2 variants). Reading a sibling dialect is a small friction next to reading a language you don't speak: the first
is a papercut a fast-follow catalog fixes, the second is a wall. ❌ Don't collapse the two by blocking regional
fallback.

The guard applies to AUTO-selection only. An explicit pick in the Settings picker is the user's business and carries no
such check.

`script_of(tag, entry)` reads the preference's script from three sources, most explicit first:

1. the tag's own script subtag (`zh-hant-tw` → `hant`),
2. its region, when that region's likely script differs from the language default (`zh-tw` → `hant`),
3. the language's default script (`zh` alone → `hans`).

In practice macOS emits an explicit script for Chinese (`zh-Hans-CN`, `zh-Hant-TW`), so branch 1 usually decides;
branches 2 and 3 cover hand-set and imported preference lists.

## The generated table

`apps/desktop/scripts/gen-shipped-locales.ts` (pure logic in `gen-shipped-locales-lib.ts`, run via
`pnpm intl:shipped-locales` from `apps/desktop/`) reads the catalog directories under `src/lib/intl/messages/` via
`listLocales()` and asks Node's `Intl.Locale(tag).maximize()` for the script facts, emitting
`shipped_locales.gen.rs`. Per catalog it records:

- `tag`: the directory name VERBATIM. The resolver hands it straight back to the frontend, which keys its catalog map on
  the directory name, so the spelling has to survive the round trip. Comparisons are case-insensitive.
- `script`: what a reader of this catalog reads (`zh` → `hans`).
- `default_script`: the bare language's likely script. Differs from `script` only for a catalog that names a script
  itself (`zh-Hant`), which is exactly the case where reading `script` as the language default would be wrong.
- `region_scripts`: the regions whose likely script differs from `default_script`. Empty for every Latin-script
  language; `zh` carries the Traditional set (TW, HK, MO, plus the overseas-community regions CLDR lists, and their UN
  M49 numeric equivalents).

Regions are enumerated (the 676 two-letter combinations plus the 1,000 three-digit M49 codes) rather than listed,
because CLDR's region set drifts with every ICU update and a hand-kept list would quietly stop covering new codes.
Unknown codes maximize to the language default and contribute nothing. Everything but `tag` is emitted lowercase,
matching the normalized tags the resolver compares.

The generator runs `rustfmt` on its output itself, rather than leaving it to the `package.json` script: the
`shipped-locales-fresh` check invokes the script directly, so formatting anywhere else would make its
regenerate-and-diff report permanent phantom drift.

Two guards keep the table honest, because a stale one leaves a new locale both unreachable AND unguarded:

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

The three parts come from `NSLocale::autoupdatingCurrentLocale`, whose `regionCode` is documented to return the `rg`
subtag's value when there is one (`SE` here, not `US`), and which this machine confirms (`languageCode` `en`,
`regionCode` `SE`, `scriptCode` nil; macOS 26.5.2, 2026-08-19). The AUTOUPDATING locale rather than `currentLocale`:
this is read from a live-change path, and the autoupdating one is the locale Foundation documents as tracking the
user's preferences, so there's no question of a cached snapshot outliving the change that triggered the read.

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

### Which locale, and who decides

`LANGUAGE_PREFERENCE` holds the raw `appearance.language` SETTING, not a resolved tag, so `'system'` keeps meaning "the
language the user reads now": every refresh re-reads the OS. A pinned tag carries NO script guard (that guard exists to
stop auto-selection landing a Traditional-Chinese reader on the Simplified catalog; a user who picked a language picked
it), and an unrecognized pin falls back to English rather than to the OS, because "I picked something we don't ship" is
a broken setting, not a request to follow the system.

Two writers, and they agree by construction: `lib.rs` seeds it from `settings.json` during `setup` (before the menu is
built), and the frontend pushes the same value through `set_ui_language` once it loads and on every change. The push is
idempotent, since a rebuild only happens when the resolved answer moves.

The `en-XA` pseudolocale is absent from the table on purpose. It's gitignored and regenerated, so including it would
make the generated file differ between a fresh clone and a machine that ran `pnpm i18n:pseudo` — permanent phantom
drift for the freshness check. A developer running in `en-XA` therefore sees an English menu bar over an accented app,
which is the honest and harmless outcome.
