# Locale-aware formatting + message runtime details

Depth behind `CLAUDE.md`. Two efforts live here: the locale-aware FORMATTING layer (below) and the message RUNTIME
(next). Together they make the app translation-ready: copy resolves from a JSON catalog through one typed `t()`
formatted with a real ICU engine, numbers and sizes format in the app language, and dates per the OS region. The earlier
error-text-to-frontend work shipped before these.

## The message runtime

`messages.svelte.ts` is a thin (~180-line) runtime over `intl-messageformat` (ICU MessageFormat 1, BSD-3-Clause). It
resolves user-facing text from JSON catalogs under `messages/<locale>/`, reading the language from `getUiLocale()`.

### Two accessors, two pipelines

- **`t(key, params?)`** is the path for ordinary copy: resolve the catalog string, compile it with `intl-messageformat`
  (memoized per `(locale, key)`), `.format(params)`. ONE code path for everything: plain `{name}` interpolation,
  `{count, plural, …}`, `{kind, select, …}`, and rich-text `<tag>` sentences (via `Trans.svelte`). Plurals/selects are
  resolved by the engine's `Intl.PluralRules`; we never hand-roll category selection.
- **`getMessage(key)`** returns the RAW catalog string with NO ICU parsing, for callers that do their own composition
  and must not hit ICU's brace/apostrophe grammar, specifically the error pipeline (`#lib/error-messages` `compose.ts` +
  `expandSystemStrings` + snarkdown). Its `{system_settings}` tokens and `esc()` HTML entities would collide with ICU
  placeholders. Same fallback chain as `t()`, just no `format()`.
- **`tStringIn(locale, key, params?)`** is `tString` in a given locale, down that locale's own chain. A non-reactive
  snapshot for the rare reader that needs text the user isn't reading: Settings search indexes English as a fallback
  (`../settings/DETAILS.md` § Search). Only for a language the window holds (§ Loading catalogs on demand). ❌ Never for
  markup.
- **`manifestMessage(locale, key)`** reads one of the few keys the manifest carries from EVERY catalog, loaded or not:
  the Language label in every language, for Settings search.
- **`uiCatalogLocale()`** is `resolvedCatalogLocale(getUiLocale())` read REACTIVELY (it reads the version rune, like
  `t()`). For a cache keyed by the language on screen: Settings search's index reads it on every search, so a `$derived`
  over a search re-runs on a switch even when it hits the cache and resolves no message.

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
naming a variable rather than an inline object, as `ServersHub` does), reporting the skipped count rather than guessing:
a false positive would cost the check the only thing that makes it useful, that a failure means a real bug.

### The resolver: per-locale catalogs + fallback chain

The catalogs come from a Vite plugin, `scripts/vite-locale-catalogs.ts` (Vitest loads it too), as two virtual modules:

- `virtual:cmdr-catalog/<tag>`: every `messages/<tag>/*.json` merged into one object, `@key` metadata stripped at build
  time (so it never ships or reaches the heap; `messages/DETAILS.md` § `@key` metadata schema). The runtime imports `en`
  statically; every other tag becomes its own chunk.
- `virtual:cmdr-locale-manifest`: `shippedLocales`, `manifestMessages` (the `MANIFEST_KEYS` from EVERY catalog; today
  only the Language label, for Settings search), and one `loaders[tag]` per non-English tag.

`catalogs` in the runtime is `localeTag → Catalog` for what THIS window has loaded. The SHIPPED set (the manifest) is
what everything else derives from: `availableLocales()`, the fallback chain, and `resolvedCatalogLocale()`. Whether a
catalog happens to be loaded never shapes a chain, or `zh-Hant` could resolve differently depending on what else is in
memory.

**Which dirs are locales is `listLocales()`**, the same answer every i18n check uses, narrowed to BCP 47-shaped names.
That's what keeps `screenshots/` (a sibling holding translator tooling; `capture-report.json` alone is ~280 kB, and it
was 4.5% of the whole frontend riding in every silent update before it was kept out, verified 2026-09-02) from becoming
a language or reaching the bundle: a chunk exists only for a tag the manifest names. `vite-locale-catalogs.test.ts` and
`messages.svelte.test.ts` pin it. The `en-XA/` pseudolocale (gitignored) ships exactly when it's on disk at build time.
The plugin was chosen over a lazy `import.meta.glob` because a glob makes one chunk per FILE (39 areas × 14 locales), so
a switch would be dozens of requests and the per-tag merge would move into the runtime.

`resolveRaw(locale, key)` walks a fallback CHAIN, first hit wins: the locale's own catalog, then each ancestor tag it
may inherit from, then `en` (the base, always present), then the key string itself, so a missing key renders as its own
key (visible, never a crash). The active locale is `getUiLocale()`. The chain is built by `fallbackChain()` and cached
per locale, because deriving it asks `Intl` for CLDR data and `t()` is a hot path; `_setCatalogForTests` clears that
cache along with the compiled-message one, since the chain depends on which catalogs exist.

"May inherit from" is `inheritableAncestors()` in `locale-inheritance.ts`: an ancestor that EXISTS and reads the SAME
SCRIPT. So `pt-PT` reads `pt` before English, `zh-Hant-TW` reads `zh-Hant` when we ship it, and no Chinese variant ever
reads across the Simplified/Traditional line. That one function is shared with the i18n check layer, and the Rust
resolver applies the same rule to auto-selection from its generated table; the canonical statement of the rule and why
all three must agree is `apps/desktop/src-tauri/src/intl/DETAILS.md` § The script guard, and why regional fallback
survives it.

The inheritable rung is what makes an OVERLAY catalog possible: a regional variant (`en-GB`, `pt-PT`) ships only the
keys that genuinely differ and inherits the rest per key, so it costs ~60 strings rather than a full catalog. A
different-SCRIPT variant inherits nothing, so it isn't an overlay at all; definition, rule table, and how to add one:
`docs/guides/i18n.md` § Overlay catalogs. The exact-locale rung is also exercised by the `_setCatalogForTests` seam (a
synthetic test-only locale).

`availableLocales()` returns the shipped tags (sorted, `en` first), whichever are loaded, and drives the Settings >
Appearance > Language picker, so a newly-added locale dir auto-appears with no code edit.

### Loading catalogs on demand

A window holds English (the per-key fallback, and Settings search's second language) plus the catalogs its language's
chain reads. Every other catalog is a chunk it loads when asked. Measured in a WKWebView on an M3 Max (2026-10-23,
`docs/notes/2026-10-23-lazy-catalogs-spike.md`): about 70 ms less module evaluation per window open and about 24 MiB
less WebContent footprint once settled, against loading all 15; one more catalog on demand takes 7–8 ms.

- **`setLocale()` loads first, switches second.** It loads the new locale's chain (an overlay brings its base: `es-419`
  loads `es`), then flips the locale, bumps the rune, and writes `<html lang>` in ONE synchronous step (`applyLocale`).
  Until then the window shows the old language in full: never a frame of English, never half-switched. A switch whose
  chain is already loaded (English, the current language) is synchronous. **Why**: the alternative (switch first,
  re-render when the catalog lands) shows English for a frame on every switch.
- **The latest call wins.** Each call takes a number; a load that lands after a newer call started applies nothing. The
  picker previews the highlighted row by applying it, so arrowing through it fires a switch per row; this keeps the last
  row the one that sticks, however the loads land. A chunk that fails to load keeps the old language (logged).
- **Eviction.** After a switch, shipped catalogs the window no longer reads (English aside) are dropped, so browsing the
  picker doesn't end with every catalog in memory. Switching back costs one more load.
- **Startup.** At module load the runtime starts loading the catalog the window will most likely speak, as
  `startupCatalogsReady`, and the root `routes/+layout.ts` `load` awaits it, which gates every window's first render. ❌
  Not a top-level await: one here made WebKitGTK 2.52 resolve a secondary window's route-node `import()` before that
  module had finished evaluating, so the page component ran with its own module-level templates still `undefined`, and
  every Settings, viewer, and queue window crashed while the main window worked (CI run 39989092928, reproduced in the
  Linux E2E container). `no-restricted-syntax` in `eslint.config.js` bans top-level await in app code. The catalog it
  loads: the one the last switch in ANY window landed on (`startup-catalog.ts`, in localStorage; every window shares the
  `tauri://localhost` origin), else the webview's own. The remembered tag also becomes the language (and `<html lang>`)
  until `appearance.language` applies, so a language picked against the macOS one shows from the first frame. A stale
  hint (the macOS language changed while Cmdr was closed) costs one more load when the setting applies, and the window
  shows the hinted language meanwhile, never English unless that's the hint.
- **Callers.** The main window's `initSettingsApplier()` awaits the startup switch before `settingsReady`, so the panes
  mount in the right language. Secondary windows' `initWindowLanguageSync()` and every live change fire and forget. The
  native menu bar (`setUiLanguage`, Rust) moves a few ms before the webview on a switch to an unloaded language.
- **Text from another language.** `tStringIn(locale)` is only for a language the window holds (English or the UI chain)
  and throws outside production builds otherwise, since resolving through an unloaded catalog would quietly answer in
  English. Text needed from EVERY language goes in the manifest: add its key to `MANIFEST_KEYS` and read it with
  `manifestMessage()`, which walks the chain like `t()`. `searchAllLocales` labels are pinned to the manifest by
  `settings-search.i18n.test.ts`.
- **Tests.** `test-setup.ts` loads every catalog and disables eviction, so `setLocale('de')` then `t()` on the next line
  works (the calls are `void setLocale(…)` for the floating-promise rule). It also forgets the startup hint before the
  runtime loads and before every test, since Vitest keeps localStorage across files. A test using `vi.resetModules()`
  gets a fresh runtime holding only English, like a new window, and must await (or `vi.waitFor`) a switch.
  `lazy-catalogs.svelte.test.ts` covers this section, with `_setCatalogLoaderForTests` to control when a catalog lands.

### Adding a regional overlay: the plumbing checklist

There's no locale registry: the catalog DIR is the registration, and every layer derives from it. For an overlay like
`es-419` over `es` (or `pt-PT` over `pt`), the translation process is `docs/guides/i18n.md` § Adding an overlay; the
code side is:

1. Create `messages/es-419/` holding only the forked keys. The runtime picks it up with no edit: `availableLocales()`
   lists it, `fallbackChain('es-419')` is `['es-419', 'es', 'en']`, and the picker labels it "Español latinoamericano"
   beside "Español" (both pinned in `messages.svelte.test.ts` and `locale-display-names.test.ts`).
2. Regenerate the two Rust tables from `apps/desktop/`: `pnpm intl:shipped-locales` and `pnpm intl:native-strings` (or
   let `shipped-locales-fresh` / `native-strings-fresh` rewrite them on a local `pnpm check`). CLDR's `es-MX` → `es-419`
   parents are already in `shipped_locales.gen.rs`, so every Latin American and US Spanish Mac starts opening the
   overlay. Don't add a `CATALOG_COVERS` entry: that's only for a catalog standing in for a CLDR node it isn't named
   after (`en-GB` for `en-001`).
3. Update the one hardcoded expectation: `the_overlays_are_the_only_locales_held_to_the_overlay_contract` in
   `src-tauri/src/intl/native_strings.rs` lists the overlays by name. `every_shipped_locale_speaks_its_own_menu_bar`
   then holds the overlay's `menu.*` forks against `es`, and `menu_t` inherits the rest from `es`.
4. Refresh the docs that count catalogs (`apps/desktop/CLAUDE.md`'s catalog count, `docs/guides/i18n.md` § Overlay
   catalogs) and the `SHIPPED` list in `locale-display-names.test.ts`.

Nothing outside the desktop app changes: the analytics language events carry `availableLocales()` tags, so the new tag
reaches the api-server and dashboard as data. The one path that still truncates is the Linux webview
(`src-tauri/src/intl/DETAILS.md` § Known gap), where `es-MX` reads `es`.

**A catalog chunk is wholesale, and the native strings ride along.** `menu.json` holds the labels only Rust ever draws
(the menu bar, the window title, the already-running alert), so each loaded catalog carries a few KB of message values
the frontend never renders (roughly 55 KB across ten catalogs, measured over `messages/*/menu.json`, metadata excluded,
2026-08-20; a window now holds two catalogs, not all of them). That's an accepted cost: one catalog, one `@key` schema,
and one set of coverage checks for every string in the app is worth more than the bytes, and a translator working one
pile beats a translator working two.

### Reactivity (load-bearing)

A module-level `localeVersion = $state(0)` rune (hence `.svelte.ts`) is a re-render SIGNAL, not a second locale source:
`getUiLocale()` stays the single source of truth for the VALUE. Every `t()`/`getMessage()` call reads the rune
UNCONDITIONALLY and FIRST, before any compiled-message cache lookup; otherwise Svelte doesn't track the dependency and a
markup `{t('key')}` won't re-run on a locale switch. `setLocale(locale)`, once its catalogs are loaded, writes the value
into `locale.ts`'s UI-language override (leaving the formatters' locale alone) AND bumps the rune AND clears the
compiled cache, in one step. `_setLocaleForTests` writes the value only: use it for non-reactive snapshot tests; use
`setLocale()` for reactivity tests. The pattern mirrors `system-strings.svelte.ts`. Reactivity holds only inside a
reactive context (markup / `$derived`); a `t()` in a plain `.ts` computation is a snapshot, which is the right semantics
for transient strings (toasts, error copy).

No SSR/prerender concern: the app is a pure SPA (`+layout.ts` has `ssr = false`), so route components are never
server/build-rendered; the catalog modules and both locale readers touch no `window`, and the startup hint's
localStorage read is wrapped for an environment without it.

### The ICU-vs-`#lib/intl` formatting split

Numbers/sizes/dates format through `#lib/intl` + `format-utils` (the formatting layer, below), NOT through ICU
`number`/`date` skeletons. `t()` embeds ALREADY-formatted count STRINGS as `*Text` params (e.g. `transfer.movedPhrase`'s
`filesText`), keeping formatting single-sourced. The raw integer is passed alongside ONLY to drive ICU `plural`
selection (noun + was/were agreement), never for display. Don't reformat inside messages with ICU `{n, number}`.
`IntlMessageFormat` is constructed with the RESOLVED catalog tag (`resolvedCatalogLocale`, the same answer as
`getCatalogLocale()`), so a `#` or a stray `{n, number}` still writes the same digits as the layer, and plural rules
match the text actually on screen (a `ja-JP` Mac reading English gets English plurals).

### Generated keys, codegen, checks

`scripts/gen-message-keys.ts` (pure logic in `gen-message-keys-lib.ts`, run via `pnpm intl:keys`) reads
`messages/en/*.json`, strips `@key` metadata, and emits the `keys.gen.ts` `MessageKey` union, so a wrong/missing key is
a typecheck error. It also reports keys used-in-code-but-missing (exit 1, a build failure) and catalog-keys-never-used
(a warning; the scan only sees STATIC keys, so a dynamically-built key reads as dead, so verify before deleting). Two Go
checks guard the rest: `desktop-message-keys-fresh` (regenerate-and-diff `keys.gen.ts`, fail if stale) and
`desktop-message-key-naming` (the `area.feature.leaf` shape + a known first-segment area).
`cmdr/no-raw-user-facing-string` (ESLint) stops new hardcoded copy in migrated areas (a closed sink set: `addToast`
content, `title`/`label`/`placeholder`/`aria-label`/`ariaLabel` props, `use:tooltip`, `.svelte` text; an area allowlist
widened per migrated area). In a markup sink it follows the expression: a raw literal, template literal, either branch
of a ternary or `||`/`??`, or a `const` (`$derived`, concise `$derived.by`, or plain) holding one. It can't see through
a call, a block-bodied `$derived.by`, `let`/`$state`, props, or object args (`use:tooltip={{ text }}`). The rule file's
header is canonical.

Ask Cmdr tool labels are literal-keyed in `ask-cmdr-tool-label-keys.ts`, so proposal status remains localized without
dynamic message-key construction. New English keys require translated catalog entries and a regenerated `keys.gen.ts`.

## The locale-aware formatting layer

## What this layer owns vs. doesn't

Owns: the locale decision (`getUiLocale` / `getCatalogLocale` / `getFormatLocale`), and number/size grouping + decimals
(`number-format.ts`). The DATE formatter lives in `#lib/settings/format-utils.ts` (`formatDateForDisplay` + the cached
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

Constructing an `Intl` formatter costs roughly 10× a format call, and these run per-visible-entry both in render and in
the column-measurement fold, so a per-call construction is what regresses scroll and measure on big directories. Hence
the caches below.

`getNumberFormatter(options)` caches by `${locale} ${JSON.stringify(options)}` and rebuilds only when
`getCatalogLocale()` changes. Words around a number (a narrow unit, a currency symbol) come from the same formatter, so
they and the digits always agree. `getCatalogLocale()` itself is per-entry hot, so `resolvedCatalogLocale` memoizes its
answer per tag (cleared with the chain cache). `getGroupSeparator()` caches the group character per locale (derived from
`Intl.NumberFormat(locale).formatToParts(11111)`). Both mirror the lazy-singleton `getSystemLocaleFormatter()` in
`format-utils.ts`, which keys its single cached `Intl.DateTimeFormat` on `getFormatLocale()`.

## The en-US triad change (Decision 4, reviewable)

Raw-byte triads (`formatSizeTriads`) now group with the locale's separator instead of the hardcoded U+2009 thin space,
so byte sizes agree with the localized counts from `formatNumber`. en-US's `Intl` group separator is the comma, so for
an en-US user the byte readout changes from `1 234` (thin space) to `1,234`. This is the one place en-US output is NOT
byte-identical to the pre-change code. The alternative was to keep the thin space always (locale-independent); that
would have preserved the en-US look but left counts (comma) and byte sizes (thin space) incoherent within a locale, the
same incoherence German users would get. The commit is isolated and revertible on its own if the always-thin-space look
is preferred (in which case `formatNumber` arguably should match it).

Human-friendly sizes (`formatFileSizeWithFormat`, in `#lib/units`) use `useGrouping: false`, so en-US stays
byte-identical there: the old `toFixed(2)`/`String(value)` never grouped, and a forced-unit `10000.00 MB` must not
become `10,000.00 MB`. Only the decimal separator localizes (`1.02 MB` → `1,02 MB`). `en-us-parity.test.ts` pins both
halves of this split, and is the net if a refactor ever reaches for a shared `useGrouping`.

## Value↔unit spacing invariant

Human-friendly sizes compose as `` `${value} ${unitLabel}` `` with an explicit ASCII space; we never adopt `Intl`'s
`style: 'unit'`, which injects a narrow no-break space and has no IEC binary units (`KiB`, `MiB`) at all. The unit word
itself is catalog copy in the UI language (`common.sizeUnit.*`, so French reads `Mo`), and tier coloring takes the tier
from `formatTieredSize`, never from the text.

## Chinese list joins get Han–Latin spacing

CLDR's zh and zh-Hant conjunction patterns join tight (`Warp和其他 App`), but both Chinese style guides space Han
against Latin words and digits. For a `zh*` UI locale, `formatConjunctionList` (and `formatDisjunctionList`, the same
path) formats to parts and pads a Han connective (`和`) wherever it meets a Latin or digit edge. The full-width `、`
stays tight (punctuation takes no space), and Japanese is excluded (it runs tight), so the rule keys off the language,
not the script. The per-locale memo carries the flag, so the common path stays one cached `format()`.

## Catalina has no Intl.ListFormat

On macOS 10.15 `Intl.ListFormat` is `undefined`, whatever Safari the Mac has, so `new Intl.ListFormat(...)` throws
`undefined is not a constructor`. Every file row's accessible name joins through `formatUnitList`, so a bare constructor
call crashed the main window at startup there (cmdr-reports#80, ERR-5DKYE, Cmdr 0.55.0 on 10.15.8).

- **Why, when Safari 14.1 "has" it**: JavaScriptCore installs `Intl.ListFormat` only when built against ICU 67+ (66 with
  Apple's internal SDK): `HAVE_ICU_U_LIST_FORMATTER` in `IntlListFormat.h`, read by `IntlObject::finishCreation`. A
  Safari update staged onto Catalina brings its own JavaScriptCore but links the OS's `libicucore`, and Catalina ships
  ICU 64 (`ICU-64260.0.1` in Apple's `distribution-macOS` tag `macos-10156`); Big Sur 11.0.1 ships `ICU-66108`, so it
  has the constructor. MDN and caniuse don't model this, so a Safari-version linter can't see it (verified from WebKit's
  `safari-613-branch` source and `apple-oss-distributions/distribution-macOS` `release.json`, 2026-10-22).
- **The same ICU gate hides, on Catalina**: `Intl.NumberFormat.prototype.formatRange` / `formatRangeToParts` and
  `Intl.PluralRules.prototype.selectRange` (ICU 68+, so Big Sur lacks them too). Nothing in the app calls them; the one
  bundled `formatRange` (Ark/Zag) feature-detects. `style: 'unit'` number formatting needs ICU 64, which Catalina meets.
- **The answer is degrade, not block**: `createListFormat()` is the one door to the constructor and answers `undefined`
  without it. The "and"/"or" joins and `formatUnitList` then join with the enumeration comma (`、` for zh/ja, `, `
  otherwise), and narrow durations with a space. A guessed connective would be English in every language, and a comma
  list keeps every name in order, which is what the eject message, the transfer toast, and the accessible names need.
- **Guardrail**: `no-restricted-properties` in `apps/desktop/eslint.config.js` bans `Intl.ListFormat` member access
  outside that door (off in tests). `list-format-fallback.test.ts` deletes the constructor and pins the fallback output.

## Column measurement

`views/measure-column-widths.ts` shrink-wraps the Size/Modified columns and calls `formatSizeForDisplay` per visible
entry (render path) AND in `foldEntries` over the prefetch buffer. Because render and measure share that one function,
they read the same locale, so a localized separator is produced identically in both. `tabularize` substitutes only
digits (modeling `font-variant-numeric: tabular-nums`), so a localized separator is measured at its real width, which is
correct. Never add a second formatting path for measurement.

## `<html lang>`: which language the DOM is told it holds

`app.html` ships a static `lang="en"` and one template serves every window, so nothing in `src/` used to correct it and
all 13 catalogs served English's language tag. That's a WCAG 3.1.1 (Level A) failure, and the practical cost is larger
than the letter of it: a screen reader picks voice, pronunciation, and prosody from this attribute, so a Swedish UI
announced as English gets an English voice spelling out Swedish words.

**Decision**: the write hangs off `setLocale()`, in `document-language.ts`. **Why**: Cmdr is multi-window and each
webview runs its own i18n runtime, so "set it at mount" is six call sites that drift the moment one window is added
without the line. `setLocale()` is already the one seam every window's init AND every live change funnels through (the
main window through `settings-applier.ts::applyLanguage`, the others through
`window-settings.ts::initWindowLanguageSync`, and an OS-locale move through both of those via `watchSystemLocales`). One
write there covers startup, the picker, and the OS moving underneath us, for every window, for free.
`routes/window-route-coverage.test.ts` therefore needs no new assertion: its existing "a window that gates on
`initWindowSettings()` must also call `initWindowLanguageSync()`" rule already guarantees every window reaches
`setLocale()`.

**Decision**: it carries the RESOLVED catalog tag (`resolvedCatalogLocale()`), not the requested locale. **Why**: the
two differ exactly when we don't ship what was asked for. A Japanese Mac with no `ja` catalog reads English text, and
announcing `lang="ja-JP"` over it hands a screen reader a Japanese voice for English words — measurably worse than the
blunt-but-true `en`. The resolver is the head of the fallback chain that has a catalog, so it answers `zh-Hant` →
`zh-Hant` (never Simplified `zh`, the script guard holds), `pt-PT` → `pt`, `en-GB` → `en-GB` (an overlay is still a
language), `ja-JP` → `en`. Per-KEY resolution can still walk further down the chain for a key an overlay doesn't fork;
the chain head is the honest answer for the window as a whole.

**`'system'` never reaches it.** `setLocale()` is fed `pickUiLocale(setting)`, which has already mapped `'system'` to
the OS's tag or to `null`; on `null` the write reads back through `getUiLocale()`, so it announces the webview's own
resolved tag rather than going stale on the previous language.

Layout DIRECTION is deliberately not set: Cmdr ships no RTL catalog, and a `dir` derived from a language tag would be an
untested guess. `document-language.ts` is where it lands when an RTL locale does.

## The locale source seam + the Language picker

`getUiLocale()` is intentionally a single function, not a locale-management system: `setLocale()` (in
`messages.svelte.ts`) writes its override and bumps the message rune, so a language switch is observable everywhere
without a store. Keep the seam minimal.

The in-app picker rides this seam. **Settings > Appearance > Language** is the `appearance.language` enum setting
(`settings-registry.ts`), default `'system'`. Its options are built by `languageOptions()` from `availableLocales()`:
`'system'` (the only translatable option label) plus one option per loaded locale, each labeled with the locale's own
endonym via `Intl.DisplayNames` (`de` → "Deutsch"), so the list is self-describing and no language names are hardcoded.
That endonym is resolved in the option's OWN locale, so the `en` row reads "English" whatever the app currently speaks:
the way out for a user who can't read the current language.

### Why a label is computed against the whole list

`locale-display-names.ts` takes the shipped set, not just one tag, because a label's only job is answering "which of
these rows is mine?" and that question is about the list in front of the user.

Each locale option also carries `lang: <tag>` (`settings/definitions/appearance.ts`), so a screen reader voices
"Deutsch" as German rather than as an English word (WCAG 3.1.2). The System default option has none: its label is UI
copy, which the page's own `lang` already covers.

CLDR names a locale by DIALECT, which settles the REGION axis unaided: `en`, `en-GB`, and `en-AU` come back as
"English", "British English", and "Australian English", three distinct strings with nothing added. It does not settle
the SCRIPT axis. CLDR's endonym for bare `zh` is "中文", so next to "繁體中文" a Traditional reader had no way to tell
which row was Simplified.

**Decision**: name the script only when a SIBLING catalog of the same language is written in a different one, and take
the script from `likelyScript()`. **Why**: it's general (a future `sr` beside `sr-Latn` decorates itself with no edit; a
`pt-PT` beside `pt` correctly doesn't, since the dialect names already differ), and it reuses the very CLDR answer that
decides catalog inheritance, so the picker and the fallback chain can't disagree about what `zh` is.

**Decision**: ❌ don't decorate unconditionally. **Why**: maximizing every tag yields "Deutsch (Lateinisch)", "English
(Latin)", "magyar (Latin)". A qualifier that distinguishes nothing is noise, and macOS agrees — System Settings >
Language & Region writes "English", never "English (Latin)". macOS can't settle the always-vs-sometimes question on its
own, though: it ships every variant of every language, so its list is always the both-present case and both policies
would look identical there.

**The consequence, accepted**: the label is list-dependent, so `zh` would read "中文" if we ever dropped `zh-Hant`.
That's the honest direction — the qualifier appears exactly when it starts distinguishing something, and dropping a
shipped catalog is not a thing that happens.

❌ No two rows may share a label. `locale-display-names.test.ts` asserts distinctness across the whole shipped set, so a
colliding new catalog fails loudly and a human picks the label, rather than the picker quietly printing one word twice.

The `'system'` row names what it resolves to — "System default (Svenska)"
(`settings.appearance.language.opt.systemWithLanguage`, with `…opt.system` as the bare fallback for English and for no
answer at all). It's a `get label()` on a literal-label option, which `resolveOption` passes through untouched, so it
re-reads on every access and needs nothing from the registry; both branches go through `tString()`, so the locale rune
makes it reactive to a live OS change as well as to a language switch. `settings-applier.ts`'s `applyLanguage` maps the
value to the seam through `os-locales.ts`'s `pickUiLocale`: a tag → `setLocale(tag)`, `'system'` → the OS answer
(below). It runs in `applyAllSettings` at startup (so a persisted choice survives restart) and on every
`appearance.language` change (live, no Apply button, no restart, no Tauri command: locale is frontend-only). A persisted
tag with no shipped catalog (e.g. `en-XA` chosen in a dev build, then opened in prod) fails enum validation in the store
and degrades to the `'system'` default with a warn.

### Three things the picker deliberately doesn't do

Product decisions, so a later "we should also…" pass has something to read before it builds one of them (David,
2026-08-19):

- **No "this translation is machine-made" notice**, anywhere, not even once. A translation the app offers is a
  translation the app stands behind; hedging it in the UI would make every locale feel provisional. The consequence is
  that the app never asks how a translation reads, which is what makes the `language_changed` event the only quality
  signal there is (`src-tauri/src/analytics/DETAILS.md` § The language events).
- **No "coming soon" rows for languages we don't ship yet.** The picker lists what a user can pick right now; a disabled
  row is a promise with a date attached, and the roster (`docs/i18n/language-selection-decisions.md`) is a plan, not a
  commitment.
- **No partial locales, auto-enabled or otherwise.** `desktop-i18n-coverage` is the enforcement, at error level: a key
  missing from a locale, or byte-identical to English without a `@key.sameAsSourceJustification`, fails the build
  (`docs/guides/i18n.md`).

That last one is why auto-selection draws from `availableLocales()` and the resolver from the generated catalog table,
both of which are the coverage-gated set: switching a user's app to their own language without being asked is a promise
that the app IS in that language. One English string in a Hungarian dialog is a bug report; a whole English surface is a
broken promise. An explicit pick in the picker carries no such promise, which is also why it carries no script guard.

## What `'system'` resolves to

`'system'` is a sentinel we never write a resolved tag back into: writing one back would freeze the user out of
following the OS, and would silently turn "I didn't care" into "I decided". So it's resolved on every read, by
`pickUiLocale()` in `os-locales.ts`, against an answer the Rust resolver computed (`src-tauri/src/intl/`, which owns the
walk and the script guard).

**The OS preference LIST is the source, not the webview's tag.** macOS hands out an ORDERED list of languages the user
reads; the webview exposes exactly one tag, so a user whose preferences are `[hu-HU, sv-SE]` could never reach Swedish
while Hungarian isn't shipped. Their own second choice was structurally unreachable. Reading the list in Rust also
removes any dependence on what WebKit decides to do with bundle metadata, which we can't control or test cheaply.

**Regional fallback is deliberate; a script boundary is not.** `pt-PT` lands on the Brazilian `pt` catalog and `en-CA`
on US `en`, while `zh-Hant-TW` does NOT land on the Simplified `zh` catalog. ❌ Don't "fix" the guard by blocking
regional fallback: the two cases pull in opposite directions on purpose, and the `en-GB` / `en-AU` overlays ride on the
regional half for every key they don't fork. This applies BOTH to Rust's auto-selection and to the per-key chain above,
which is why `locale-inheritance.ts` exists. Why each half is the way it is:
`apps/desktop/src-tauri/src/intl/DETAILS.md` § The script guard, and why regional fallback survives it (canonical).

**Every window resolves for itself.** Each Cmdr window is its own webview with its own i18n runtime instance, so the
main window's answer doesn't reach the Settings or Queue window. The main window awaits the answer inside
`initSettingsApplier` (the fetch is fired BEFORE the settings store is awaited, so the two IPC round-trips overlap
rather than stack: ❌ no serialized round-trip, and no paint gate — `routes/(main)/show-main-on-mount.ts` records why we
removed the last one); it then waits for that switch's catalog before the panes mount. Secondary windows use
`initWindowLanguageSync()` from `#lib/settings/window-settings`, which applies the persisted value as soon as the
settings load (synchronously when its catalog is the remembered startup one, else a few ms later) and re-applies when
the OS answer lands, so nothing gates a first paint.

**A localized window title follows a switch too.** The OS title (title bar, Window menu, what VoiceOver reads for the
window) is a native `setTitle` write that no re-render reaches, so the opener's constructor title would stay in the
launch language. **Decision**: `initWindowLanguageSync({ title })` hands the getter to `followWindowTitle()`
(`window-title.svelte.ts`), an `$effect.root` that re-writes the title whenever the copy it reads moves. **Why**: the
language sync is the one call every secondary window already makes, so opting a window in is one argument, not a
per-window listener. It awaits `initAppMode()` first so the E2E and capture title markers survive the write, skips a
write when a region-only bump leaves the copy unchanged, and treats a `null` title as "leave it" (the viewer, until its
file loads). Every window passing `title` needs `core:window:allow-set-title` in its capability file; the Settings,
Keyboard shortcuts, queue, and viewer windows do. The dev-only Debug window stays English and passes none.

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

"Reasonable" is measured, not assumed: WebKit's default locale follows the user's macOS preference LIST, and pays no
attention to what the bundle declares. Cmdr's bundle names no `CFBundleLocalizations` at all, so `NSLocale.current`
should have resolved a Hungarian-preference machine to `en-HU` and the webview to English; instead the whole app came up
Hungarian, from a per-app `-AppleLanguages "(hu-HU)"` and from a global `defaults write -g AppleLanguages` alike
(verified on macOS 26.5.2, production build relaunched under both, 2026-08-19). So the fallback lands on the user's
first preference rather than on English, which is the right shape for a degraded answer. It's only ever the first one,
though, which is the whole reason the real resolver reads the list in Rust.

## Language and region are two settings

macOS keeps them apart (System Settings > General > Language & Region), and so does `locale.ts`. A person can read
Hungarian and live in Sweden. **Decision** (David, 2026-10-28): numbers follow the app language; dates and times follow
the region.

- **`getUiLocale()`** follows `appearance.language`. Catalog text reads it: `t()`, `getMessage()`, `<Trans>`, and the
  weekday/month NAMES in `query-ui/filter-chips/filter-popover-helpers.ts` (they're words inside translated sentences).
- **`getCatalogLocale()`** is that language's RESOLVED catalog tag (`resolvedCatalogLocale(getUiLocale())`, so `en-GB`
  stays `en-GB` and a `ja-JP` Mac with no `ja` catalog gets `en`). Every number reads it: counts, sizes, rates,
  durations, money, the Ask Cmdr cost and token counts (CLDR compact notation, so zh counts in 万), the low-space
  percent, and ICU `#` (the engine gets the same tag). The list joins (`list-format.ts`) read it too, so a list in an
  English sentence on a Mac with no catalog for its language joins with "and". `messages.svelte.ts` installs it through
  `setCatalogLocaleResolver()` at module load, reading the version rune, so a markup or `$derived` number re-renders on
  a language switch with no message involved; before that (or in a test that mocks the runtime) it falls back to
  `getUiLocale()`.
- **`getFormatLocale()`** always follows the OS. The `'system'` date (`format-utils.ts::getSystemLocaleFormatter`), the
  time of day, the worded license dates (`formatLongDate`), and calendar facts read it: `resolveFirstDayOfWeek` is in
  the SAME function as those month names and takes the other locale, because which day the week starts on is a region
  decision. No setting can move it: the only production writer is `setOsFormatLocale()`, which carries the OS's own
  answer, and `_setFormatLocaleForTests` is the test seam. ❌ No `toLocaleDateString` / `toLocaleTimeString`: they read
  the webview's tag, which drops the region (`cmdr/no-raw-locale-format`).

**Why numbers follow the language.** Digits sit inside translated sentences, and a number in another locale's convention
reads as a typo there: an English UI on a Swedish Mac showed `612,00 MB`, `0,83 US$`-style money, and `54,5s`, where an
English sentence wants `612.00 MB`, `$0.83`, `54.5s`. **Why dates keep the region.** Those are the conventions people
actually pick and notice (ISO-like dates, a 24-hour clock, Monday week start); a language pick must not take them away.
`language-vs-region.test.ts` pins both halves, `en-us-parity.test.ts` pins that an English UI writes US digits on any
region.

`setLocale()` therefore moves copy and numbers, never dates. A macOS region change still re-renders dates the way it
always did: `watchSystemLocales()` adopts the new tag and re-applies `appearance.language`, whose rune bump re-runs open
markup.

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

So this user now sees Swedish dates under English copy (and English numbers, per the decision above), and a machine with
no region override composes the tag it already had (`en-US` stays `en-US`; `en-us-parity.test.ts` is the net). ❌ Don't
try to re-derive any of this from Cmdr's own date column: `appearance.dateTimeFormat: 'iso'` makes it prove nothing
about the locale. Read `resolvedOptions()` directly, or the Rust side's tests. The Rust half (which `NSLocale` fields,
why the autoupdating locale, what makes a part unusable) is canonical in `apps/desktop/src-tauri/src/intl/DETAILS.md`.
