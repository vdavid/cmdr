# UI-language resolution details

Depth behind `CLAUDE.md`. This module answers one question: given the message catalogs we ship and the user's macOS
language preferences, which catalog should the app open?

## Why the resolver lives in Rust

Three consumers need the answer and two of them run before the webview exists: the native menu bar (built during
`setup`) and the "Cmdr is already running" alert (fires before any window). Putting the real resolver in TypeScript
would mean a second one in Rust for those two, which is two implementations of one rule, drifting apart. It also matches
the project's smart-backend / thin-frontend principle: the frontend consumes the answer, it doesn't compute it.

The cost is that the script guard needs CLDR likely-subtags data, which the webview gets free from
`Intl.Locale.maximize()` and Rust does not. That's what the generated table buys.

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

## The frontend contract

`get_ui_locale` is the only way out of this module. It returns `Option<String>`:

- **macOS**: always `Some`, falling back to `"en"` when the walk finds nothing, because English IS the answer when the
  user reads no language we ship.
- **anything else**: `None`, meaning "no OS preference list here". The frontend reads that as "no override" and lets the
  webview default stand, which is the right behavior on Linux.

The frontend half (`src/lib/intl/ui-locale.ts`) fetches this once per window and resolves the `'system'` setting through
it. Startup pays no serialized round-trip: the main window fires the fetch before awaiting the settings store, so the
two overlap. The backend side of the call measures ~65 µs (debug build, 1,000 iterations, dominated by the
`NSUserDefaults` read), so there's nothing to cache here; keeping it uncached is also what will let a live OS
language change be re-read later.
