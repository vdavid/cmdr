# Locale-aware formatting + message runtime details

Depth behind `CLAUDE.md`. Two efforts live here: the locale-aware FORMATTING layer (below) and the message RUNTIME
(next). Together they make the app translation-ready: copy resolves from a JSON catalog through one typed `t()`
formatted with a real ICU engine, and numbers/sizes/dates format per the OS region the way a native macOS app does. The
earlier error-text-to-frontend work shipped before these.

## The message runtime

`messages.svelte.ts` is a thin (~180-line) runtime over `intl-messageformat` (ICU MessageFormat 1, BSD-3-Clause). It
resolves user-facing text from JSON catalogs under `messages/<locale>/`, reading the language from `getUiLocale()`.

### Two accessors, two pipelines

- **`t(key, params?)`** is the path for ordinary copy: resolve the catalog string, compile it with `intl-messageformat`
  (memoized per `(locale, key)`), `.format(params)`. ONE code path for everything: plain `{name}` interpolation,
  `{count, plural, …}`, `{kind, select, …}`, and rich-text `<tag>` sentences (via `Trans.svelte`). Plurals/selects are
  resolved by the engine's `Intl.PluralRules`; we never hand-roll category selection.
- **`getMessage(key)`** returns the RAW catalog string with NO ICU parsing, for callers that do their own composition
  and must not hit ICU's brace/apostrophe grammar, specifically the error pipeline (`$lib/error-messages` `compose.ts` +
  `expandSystemStrings` + snarkdown). Its `{system_settings}` tokens and `esc()` HTML entities would collide with ICU
  placeholders. Same fallback chain as `t()`, just no `format()`.

This is the **error-pipeline boundary**: error literals migrate INTO the catalog as `errors.*` keys (plain strings,
possibly markdown), but they keep rendering through the existing snarkdown + `{@html}` + param-escaper pipeline via
`getMessage()`, NOT `t()`/`<Trans>`. `<Trans>` is only for the handful of UI sentences with inline INTERACTIVE
components (a `<LinkButton>` mid-sentence). Don't conflate the two.

**Why a tag and its snippet must be renamed together.** The renderer is `{#if childSnippet}…{/if}` with no else, so a
tag the call site doesn't supply a snippet for renders NOTHING: its inner text vanishes from the UI, at runtime, with no
warning. The realistic way in is renaming a tag in the catalog (or the `snippets={{ … }}` key) and finishing only one
side, which looks complete in review and passes every other check — the catalog is valid ICU, `i18n-parity` compares
locales to each other rather than to the component, and nothing throws. `i18n-trans-snippets` closes that by comparing
each call site's snippet keys against the English message's tags in both directions, so a half-finished rename names its
own other half. It resolves what it can read statically and SKIPS the rest (a computed `key={…}`, or a snippets prop
naming a variable rather than an inline object, as `NetworkBrowser` does), reporting the skipped count rather than
guessing: a false positive would cost the check the only thing that makes it useful, that a failure means a real bug.

### The resolver: per-locale catalogs + fallback chain

At module load, `import.meta.glob('./messages/*/*.json', { eager })` pulls EVERY locale dir's catalog files, not just
`en`. The dir segment of each glob path is the locale tag (`messages/pt-BR/foo.json` → `pt-BR`), and a `BCP47_DIR` regex
gate keeps only directories that look like a BCP-47 tag: the `screenshots/` capture-artifact dir sits alongside the
locale dirs under `messages/` and is globbed too, so it MUST be filtered out (it's not a locale). The dev-only `en-XA/`
pseudolocale is globbed when present and simply absent in prod (gitignored). The result is `catalogs`: a
`localeTag → merged metadata-stripped Catalog` map.

`resolveRaw(locale, key)` resolves with BCP-47 fallback: `catalog[locale]` → `catalog[baseLanguage]` (`de-DE` → `de`) →
`catalog.en` (the base, always present) → the key string itself. A missing key renders as its own key (visible, never a
crash). The active locale is `getUiLocale()`. `en` is the only TRANSLATED catalog that ships today; the base-language
and exact-locale rungs are real (any added locale dir resolves through them) and also exercised by the
`_setCatalogForTests` seam (a synthetic test-only locale).

`availableLocales()` returns the loaded catalog tags (sorted, `en` first) and drives the Settings > Appearance >
Language picker, so a newly-added locale dir auto-appears with no code edit. The non-locale `screenshots/` dir never
shows up there (the same `BCP47_DIR` gate).

### Reactivity (load-bearing)

A module-level `localeVersion = $state(0)` rune (hence `.svelte.ts`) is a re-render SIGNAL, not a second locale source:
`getUiLocale()` stays the single source of truth for the VALUE. Every `t()`/`getMessage()` call reads the rune
UNCONDITIONALLY and FIRST, before any compiled-message cache lookup; otherwise Svelte doesn't track the dependency and a
markup `{t('key')}` won't re-run on a locale switch. `setLocale(locale)` writes the value into `locale.ts`'s UI-language
override (leaving the formatters' locale alone) AND bumps the rune AND clears the compiled cache. `_setLocaleForTests`
writes the value only: use it for non-reactive snapshot tests; use `setLocale()` for reactivity tests. The pattern
mirrors `system-strings.svelte.ts`. Reactivity holds only inside a reactive context (markup / `$derived`); a `t()` in a
plain `.ts` computation is a snapshot, which is the right semantics for transient strings (toasts, error copy).

No SSR/prerender concern: the app is a pure SPA (`+layout.ts` has `ssr = false`), so route components are never
server/build-rendered; the catalog merge (a `import.meta.glob` over `messages/*/*.json`) and both locale readers touch
no `window`.

### The ICU-vs-`$lib/intl` formatting split

Numbers/sizes/dates format through `$lib/intl` + `format-utils` (the formatting layer, below), NOT through ICU
`number`/`date` skeletons. `t()` embeds ALREADY-formatted count STRINGS as `*Text` params (e.g. `transfer.movedPhrase`'s
`filesText`), keeping formatting single-sourced. The raw integer is passed alongside ONLY to drive ICU `plural`
selection (noun + was/were agreement), never for display. Don't reformat inside messages with ICU `{n, number}`. A
second reason to keep that line: `IntlMessageFormat` is constructed with the UI language (plural rules have to match the
words around them), so a number formatted INSIDE a message would group by language while every number outside it groups
by region.

### Generated keys, codegen, checks

`scripts/gen-message-keys.ts` (pure logic in `gen-message-keys-lib.ts`, run via `pnpm intl:keys`) reads
`messages/en/*.json`, strips `@key` metadata, and emits the `keys.gen.ts` `MessageKey` union, so a wrong/missing key is
a typecheck error. It also reports keys used-in-code-but-missing (exit 1, a build failure) and catalog-keys-never-used
(a warning; the scan only sees STATIC keys, so a dynamically-built key reads as dead, so verify before deleting). Two Go
checks guard the rest: `desktop-message-keys-fresh` (regenerate-and-diff `keys.gen.ts`, fail if stale) and
`desktop-message-key-naming` (the `area.feature.leaf` shape + a known first-segment area).
`cmdr/no-raw-user-facing-string` (ESLint) stops new hardcoded copy in migrated areas (a closed sink set: `addToast`
content, `title`/`label`/`placeholder`/ `aria-label` props, `.svelte` text nodes; an area allowlist widened per migrated
area).

Ask Cmdr tool labels are literal-keyed in `ask-cmdr-labels.ts`, so proposal status remains localized without dynamic
message-key construction. New English keys require translated catalog entries and a regenerated `keys.gen.ts`.

## The locale-aware formatting layer

## What this layer owns vs. doesn't

Owns: the locale decision (`getUiLocale` / `getFormatLocale`), and number/size grouping + decimals (`number-format.ts`).
The DATE formatter lives in `$lib/settings/format-utils.ts` (`formatDateForDisplay` + the cached
`getSystemLocaleFormatter`) because dates carry per-component age-tier coloring that belongs with the date-color
settings; it reads `getFormatLocale()` from here, so the locale source is still single per job.

Doesn't own (deliberately out of scope for the formatting layer):

- Pluralization and sentence assembly (`pluralize.ts`, `${n} ${pluralize(...)}` sites, the fragment-concatenated
  transfer toasts). Locale-correct plurals (`Intl.PluralRules`, 6 categories) and whole-template messages belong to step
  2, where a catalog can hold the variants.
- A reactive locale STORE. Live locale switching IS supported (the Settings > Appearance > Language picker, below), but
  it rides the `setLocale()` seam + the message rune, not a `$store`. Don't add a reactive locale store; the
  `getUiLocale()` source plus the rune is the whole mechanism.
- The deliberately-fixed date formats: the `iso`/`short`/`custom` modes (`format-utils.ts::applyTokens`) and the ISO
  `formatDate` helper in `selection-info-utils.ts` (`YYYY-MM-DD hh:mm:ss`). These are user-chosen fixed formats,
  locale-independent by design. Only the `'system'` date mode is locale-driven.
- The backend. Rust emits raw numbers, byte counts, and Unix timestamps; formatting is and stays a frontend concern.

## Why the locale readers are uncached

A plain function call returning the live runtime default keeps the locale-switching seam (`setLocale()`) able to change
the answer observably. Caching a resolved locale here would freeze it for the page's life and make a switch invisible.
The cost is one cheap `Intl.NumberFormat().resolvedOptions().locale` resolve per formatter construction, and the
formatters themselves are memoized (keyed on the returned locale), so the hot paths don't pay it per format call.

## Memoization shape

`getNumberFormatter(options)` caches by `${locale} ${JSON.stringify(options)}` and rebuilds only when
`getFormatLocale()` changes. `getGroupSeparator()` caches the group character per locale (derived from
`Intl.NumberFormat(locale).formatToParts(11111)`). Both mirror the lazy-singleton `getSystemLocaleFormatter()` in
`format-utils.ts`, which now also keys its single cached `Intl.DateTimeFormat` on the active locale.

## The en-US triad change (Decision 4, reviewable)

Raw-byte triads (`formatSizeTriads`) now group with the locale's separator instead of the hardcoded U+2009 thin space,
so byte sizes agree with the localized counts from `formatNumber`. en-US's `Intl` group separator is the comma, so for
an en-US user the byte readout changes from `1 234` (thin space) to `1,234`. This is the one place en-US output is NOT
byte-identical to the pre-change code. The alternative was to keep the thin space always (locale-independent); that
would have preserved the en-US look but left counts (comma) and byte sizes (thin space) incoherent within a locale, the
same incoherence German users would get. The commit is isolated and revertible on its own if the always-thin-space look
is preferred (in which case `formatNumber` arguably should match it).

Human-friendly sizes (`formatFileSizeWithFormat`, in `$lib/units`) use `useGrouping: false`, so en-US stays
byte-identical there: the old `toFixed(2)`/`String(value)` never grouped, and a forced-unit `10000.00 MB` must not
become `10,000.00 MB`. Only the decimal separator localizes (`1.02 MB` → `1,02 MB`).

## Value↔unit spacing invariant

Human-friendly sizes compose as `` `${value} ${unitLabel}` `` with an explicit ASCII space; we never adopt `Intl`'s
`style: 'unit'`, which injects a narrow no-break space. `colorizeSizeString`/`tierClassForUnit` recover the unit via
`lastIndexOf(' ')`, so a non-ASCII space there would break tier coloring.

## Column measurement

`views/measure-column-widths.ts` shrink-wraps the Size/Modified columns and calls `formatSizeForDisplay` per visible
entry (render path) AND in `foldEntries` over the prefetch buffer. Because render and measure share that one function,
they read the same locale, so a localized separator is produced identically in both. `tabularize` substitutes only
digits (modeling `font-variant-numeric: tabular-nums`), so a localized separator is measured at its real width, which is
correct. Never add a second formatting path for measurement.

## The locale source seam + the Language picker

`getUiLocale()` is intentionally a single function, not a locale-management system: `setLocale()` (in
`messages.svelte.ts`) writes its override and bumps the message rune, so a language switch is observable everywhere
without a store. Keep the seam minimal.

The in-app picker rides this seam. **Settings > Appearance > Language** is the `appearance.language` enum setting
(`settings-registry.ts`), default `'system'`. Its options are built by `languageOptions()` from `availableLocales()`:
`'system'` (the only translatable option label, `settings.appearance.language.opt.system`) plus one option per loaded
locale, each labeled with the locale's own endonym via `Intl.DisplayNames` (`de` → "Deutsch"), so the list is
self-describing and no language names are hardcoded. `settings-applier.ts`'s `applyLanguage` maps the value to the seam
through `os-locales.ts`'s `pickUiLocale`: a tag → `setLocale(tag)`, `'system'` → the OS answer (below). It runs in
`applyAllSettings` at startup (so a persisted choice survives restart) and on every `appearance.language` change (live,
no Apply button, no restart, no Tauri command: locale is frontend-only). A persisted tag with no loaded catalog (e.g.
`en-XA` chosen in a dev build, then opened in prod) fails enum validation in the store and degrades to the `'system'`
default with a warn.

## What `'system'` resolves to

`'system'` is a sentinel we never write a resolved tag back into: writing one back would freeze the user out of
following the OS, and would silently turn "I didn't care" into "I decided". So it's resolved on every read, by
`pickUiLocale()` in `os-locales.ts`, against an answer the Rust resolver computed (`src-tauri/src/intl/`, which owns the
walk and the script guard).

**The OS preference LIST is the source, not the webview's tag.** macOS hands out an ORDERED list of languages the user
reads; the webview exposes exactly one tag, so a user whose preferences are `[hu-HU, sv-SE]` could never reach Swedish
while Hungarian isn't shipped. Their own second choice was structurally unreachable. Reading the list in Rust also
removes any dependence on what WebKit decides to do with bundle metadata, which we can't control or test cheaply.

**Regional fallback is deliberate; a script boundary is not.** `pt-PT` lands on the Brazilian `pt` catalog and `en-GB`
on US `en` ("Trash", `-ize`): reading a sibling dialect is a small friction next to reading a language you don't speak,
and a fast-follow catalog fixes it. `zh-Hant-TW` does NOT land on the Simplified `zh` catalog, because that's not a
dialect difference, it's a wall — and English is at least a language the user chose to list. ❌ Don't "fix" the guard by
blocking regional fallback: the two cases pull in opposite directions on purpose. The rule and its data live in Rust;
`apps/desktop/src-tauri/src/intl/DETAILS.md` is the canonical description.

**Every window resolves for itself.** Each Cmdr window is its own webview with its own i18n runtime instance, so the
main window's answer doesn't reach the Settings or Queue window. The main window awaits the answer inside
`initSettingsApplier` (the fetch is fired BEFORE the settings store is awaited, so the two IPC round-trips overlap
rather than stack: ❌ no serialized round-trip, and no paint gate — `routes/(main)/show-main-on-mount.ts` records why we
removed the last one). Secondary windows use `initWindowLanguageSync()` from `$lib/settings/window-settings`, which
applies the persisted value synchronously and re-applies when the OS answer lands, so nothing gates a first paint.

**`'system'` means the language the user reads NOW, not the one they read at launch.** Rust observes the macOS language
preferences and pushes `os-locales-changed` when the resolved catalog moves (`src-tauri/src/intl/DETAILS.md` has the
observer, the burst collapsing, and why Linux gets nothing). Every window subscribes through `watchSystemLocales()`,
which adopts the fresh answer and re-applies `appearance.language`, so the `setLocale()` rune bump re-renders the window
in place. It drops an event whose locale matches the answer it already has, a second guard behind the backend's, because
a needless bump re-runs every open `t()` for nothing. Under an EXPLICIT language the re-apply still happens and is still
right: the copy doesn't move, and the bump is what re-renders the formatters against the OS's new locale.

`null` from `pickUiLocale` means "no override": the webview default stands. That's the answer before the fetch settles,
on any platform with no preference list (Linux), and when the read fails, so a broken read degrades to a reasonable
language rather than a broken app.

## Language and region are two settings

macOS keeps them apart (System Settings > General > Language & Region), and so does `locale.ts`. A person can read
Hungarian and live in Sweden, and picking a UI language is not permission to overwrite the conventions they chose:

- **`getUiLocale()`** follows `appearance.language`. Catalog text reads it: `t()`, `getMessage()`, `<Trans>`, and the
  weekday/month NAMES in `query-ui/filter-chips/filter-popover-helpers.ts` (they're words inside translated sentences).
- **`getFormatLocale()`** always follows the OS. Numbers and sizes (`number-format.ts`), the `'system'` date
  (`format-utils.ts::getSystemLocaleFormatter`), and calendar facts read it: `resolveFirstDayOfWeek` is in the SAME
  function as those month names and takes the other locale, because which day the week starts on is a region decision.
  No setting can move it: the only production writer is `setOsFormatLocale()`, which carries the OS's own answer, and
  `_setFormatLocaleForTests` is the test seam.

`setLocale()` therefore reaches the UI half only. Without the split, a Hungarian pick would rewrite Swedish dates and
number grouping, and an English pick on a Swedish Mac would impose US conventions.

**The UI answer is a language, never a region.** The tag the catalog resolver picks is a catalog's, so `'system'` mode
never carries the user's regional variant into the copy. The region lives entirely in the formatting half, below.

**The formatting tag is composed in Rust, because WebKit can't answer the question.** `getFormatLocale()` prefers the
tag Rust hands over (`<language>[-Script]-REGION`, e.g. `en-SE`) and falls back to the webview's own locale when there
isn't one: before the fetch settles, off macOS, and when the region is missing or unreadable. `os-locales.ts` adopts it
through `setOsFormatLocale()` the moment it arrives, so no call site had to change.

The composition exists because the webview's locale silently loses the user's region. On a machine set to US English
with a Swedish region (`AppleLocale = en_US@rg=sezzzz`), the two sides disagreed flatly:

- Foundation: `Locale.current.identifier` = `en_US@rg=sezzzz`, region `SE`, short date `2026-08-19, 14:05`, number
  `1 234 567,89`, `firstWeekday` 2 (Monday).
- The webview: `Intl.DateTimeFormat().resolvedOptions().locale` = plain `en-US`, date `08/19/2026, 02:05 PM`, number
  `1,234,567.89`, `weekInfo.firstDay` 7 (Sunday). Only the time zone (`Europe/Stockholm`) reflected the region.
- Passing the extension explicitly doesn't help: `en-US-u-rg-sezzzz` resolves back to `en-US` with US output. Naming the
  region as a real tag DOES work: `en-SE` yields `2026-08-19, 14:05` and `1 234 567,89`, matching Foundation exactly.

(Verified on macOS 26.5.2 / Safari 26.5.2 (21624.2.5.11.8), 2026-08-19, by reading `Intl` in a bare `WKWebView` and
Foundation's `Locale` / `DateFormatter` in the same process.)

So this user now sees Swedish conventions under English copy, which is what System Settings says they asked for, and a
machine with no region override composes the tag it already had (`en-US` stays `en-US`; `en-us-parity.test.ts` is the
net). ❌ Don't try to re-derive any of this from Cmdr's own date column: `appearance.dateTimeFormat: 'iso'` makes it
prove nothing about the locale. Read `resolvedOptions()` directly, or the Rust side's tests. The Rust half (which
`NSLocale` fields, why the autoupdating locale, what makes a part unusable) is canonical in
`apps/desktop/src-tauri/src/intl/DETAILS.md`.
