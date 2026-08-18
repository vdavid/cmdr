# Auto language: follow the OS preference, honestly

Cmdr picks the user's language from their macOS language preferences and switches to it when we ship that language
completely. When we don't, they stay in English. The user can always overrule us, and overruling is reachable from the
first screen they ever see.

**The guiding principle**: the OS already knows what language this person reads, and macOS already models language and
region as two separate settings. Our job is to read those preferences correctly and never ship a half-translated app on
the strength of them. Every design decision below falls out of one of those two sentences.

## What's already true (verified, 2026-08-19, on `main` at `d574f4a58`)

Do not re-derive these; they're the reason the plan starts where it does.

- **`appearance.language` already defaults to `'system'`**, `applyLanguage` already maps that to `setLocale(null)`, and
  `getLocale()` already returns the webview's `Intl` default. `resolveRaw()` already walks locale → base → `en`.
- **Auto-detection therefore already ships, crudely.** Launching the production app as
  `open -a Cmdr --args -AppleLanguages "(hu-HU)"` brought the UI up in Hungarian (`Név`, `Méret`, `Módosítva`,
  `Nincs kijelölés, 0 fájl és 16 mappa`). This was expected to fail (the bundle declares no `CFBundleLocalizations`, so
  `NSLocale.current` should have resolved to `en-HU`); it did not. WebKit's default locale follows the preference list,
  not the bundle's declared localizations.
  - **Caveat, and the first task of M1**: `-AppleLanguages` is an app-domain override, which is not identical to a
    global preference. Whether a user whose *global* `AppleLanguages` is `hu-HU` (with no per-app override) gets the same
    result is unconfirmed. M1's design makes the question moot, but confirm it anyway so we know what today's users see.
- **Nine non-`en` locales ship**, all at 100% coverage: `de`, `es`, `fr`, `hu`, `nl`, `pt`, `sv`, `vi`, `zh`.
- **`desktop-i18n-coverage` is already an error-level check** that fails on any missing key or any value byte-identical
  to English without a `@key.sameAsSourceJustification`. "Never ship a partial locale" is therefore already an enforced
  invariant, not something this plan has to build. What the plan adds is that *auto-selection* draws only from that
  gated set.
- **The native menu bar is not localized at all.** `apps/desktop/src-tauri/src/menu/` holds ~295 hardcoded English
  literals and has no i18n plumbing. A Hungarian user today gets a Hungarian app under an English menu bar.
- **`system_strings.rs` already reads `AppleLanguages`** (`apple_languages()`) and already localizes the macOS pane
  labels it quotes, independently of the app's UI language. That's deliberate and correct (`system-strings.svelte.ts`
  explains why), and it's also the Rust seam M1 reuses.

## Design decisions and the intent behind them

**1. The OS preference list is read in Rust, not inferred from the webview.** `getLocale()` returns exactly one tag, so
a user whose preferences are `[hu-HU, sv-SE]` can never reach Swedish when Hungarian isn't shipped: the second choice is
structurally unreachable. `apple_languages()` already returns the whole ordered list. Reading it there also removes any
dependence on what WebKit decides to do with bundle metadata, which is the part we can't control or test cheaply.
*Intent*: the preference list is an ordered list of the user's choices, and honoring only its first element throws away
the user's own fallback plan.

**2. UI language and formatting locale are two values, and only the first one follows the setting.** Today
`setLocale('hu')` rewrites the single `getLocale()` source, so picking Hungarian also switches dates and number grouping
to Hungarian conventions, and picking English on a Swedish Mac switches them to US conventions. macOS itself keeps
Language and Region as separate settings, and David's own machine is a live example (`AppleLocale = en_US@rg=sezzzz`:
US English, Swedish region). *Intent*: match the OS's mental model. The user chose their number and date conventions in
System Settings; the app's UI language is not a licence to overwrite them.

**3. Auto-selection never crosses a script boundary.** `zh-Hant-TW` currently falls back to the `zh` directory, which is
Simplified. For a Traditional reader that is worse than English, because English is at least a language they chose to
list. The guard uses `Intl.Locale(tag).maximize()` and compares the likely script of the candidate with that of the
catalog. *Intent*: a fallback is only a kindness when it lands somewhere the reader can actually read. An explicit pick
in the picker is the user's business and carries no such guard.

**4. Auto-selection draws only from shipped, complete catalogs, and never from the pseudolocale.** `availableLocales()`
is already the gated set (`desktop-i18n-coverage` guarantees completeness), but it includes `en-XA` in dev builds.
*Intent*: "we auto-enabled a language" is a promise that the app is fully in that language. One English string in a
Hungarian dialog is a bug report; a whole English menu bar is a broken promise (which is why M4 exists).

**5. An explicit choice is permanent and an implicit one is not.** `'system'` stays a sentinel and we never write a
resolved tag back into the setting. *Intent*: writing back would freeze the user out of following the OS, and would
silently convert "I didn't care" into "I decided".

**6. The escape hatch appears where the user already is.** A first-launch user meets `OnboardingWizard` before anything
else, already in the detected language, so the language control belongs in the wizard. An already-onboarded user whose
resolved language changes because of *this work* gets a one-time inline bar instead. *Intent*: don't build a nag for
new users who have a perfectly good place to decide, and don't silently flip an existing user's app without an
immediate, obvious undo.

### Explicitly not wanted (David, 2026-08-19)

- ❌ No "this translation is machine-made" notice anywhere in the UI, not even once.
- ❌ No "coming soon" rows for untranslated languages in the picker.
- ❌ No partial locales, ever, auto-enabled or otherwise. The existing error-level coverage check is the enforcement.

## Milestones

Sequential unless stated. Each ends green, committed, and documented.

---

### M1: Resolve the UI language from the OS preference list

**Intent**: make `'system'` mean "walk the user's ordered preferences and take the first language we fully ship",
instead of "take whatever single tag the webview happens to resolve".

**Changes**

1. `apps/desktop/src-tauri/src/system_strings.rs`: promote `apple_languages()` from private to a shape the frontend can
   read. Prefer extending the existing `get_localized_system_strings` command's payload over adding a second command,
   so the frontend keeps one startup round-trip; if that muddies the type, add `get_preferred_languages`. Non-macOS
   returns an empty list (the resolver then falls back to the webview default, which is right on Linux).
2. New `apps/desktop/src/lib/intl/detect-locale.ts`: `pickUiLocale(preferences, available)`, pure and testable. Walks
   each preference in order, trying the full tag then its base subtag, skipping any candidate that fails the script
   guard, skipping `en-XA`, returning `null` when nothing matches (caller then uses `en`). No `window`, no Tauri, no
   side effects: this is the piece that has to be right, so it must be trivially testable.
3. `messages.svelte.ts` / `locale.ts`: `'system'` resolves through `pickUiLocale`. Keep `getLocale()`'s SSR-safety
   contract (no DOM, never throws) — the preference list arrives asynchronously, so the resolver must degrade to the
   webview default until it lands, and re-resolve when it does.
4. `settings-applier.ts`: `applyLanguage('system')` drives the new resolution.

**Tests** (unit, Vitest; `pickUiLocale` is pure logic with sharp edges, so **test-first, real red → green**)

- `[hu-HU, sv-SE]` with `hu` absent → `sv`. This is the case today's code cannot express; write it first and watch it
  fail.
- `[hu-HU]` with `hu` present → `hu`; `[hu]` → `hu`; casing variants (`HU-hu`) normalize.
- `[fr-CA]` with only `fr` present → `fr` (base fallback within one preference, before advancing).
- `[zh-Hant-TW]` with only `zh` (Simplified) present → **not** `zh`; falls through to the next preference, then English.
- `[zh-CN]` with `zh` present → `zh` (same script, so the guard must not over-block).
- `[en-US, sv-SE]` → English, and the walk stops at `en` rather than reaching Swedish.
- `[de-DE]` in a dev build where `en-XA` is loaded → `de`, never the pseudolocale.
- Empty / missing preference list → `null`, caller lands on `en`, no throw.
- Rust: `apple_languages()` already has `apple_languages_returns_at_least_one_entry`; add one asserting the command
  surfaces the list in order.

**Validation task (do this first, it's five minutes)**: `defaults write -g AppleLanguages -array hu-HU en-US`, relaunch
only Cmdr, observe, then `defaults write -g AppleLanguages -array en-US sv-SE` to restore David's exact prior value
(recorded here so it can't be lost). This settles whether today's users already get auto-detection from a *global*
preference. It changes nothing about the design; it changes what we say in the release notes.

**Docs**: `apps/desktop/src/lib/intl/CLAUDE.md` (one guardrail line: the OS list is the source, not the webview tag),
`DETAILS.md` (the walk, the script guard, and why the second preference matters). `apps/desktop/src-tauri/src/menu/`
untouched here.

**Checks**: `pnpm check --fast`, then `pnpm check desktop` and `pnpm check rust`.

---

### M2: Split UI language from formatting locale

**Intent**: stop a UI-language choice from overwriting the number, size, and date conventions the user set in System
Settings.

**Changes**

1. `locale.ts` grows two named readers: `getUiLocale()` (catalog resolution) and `getFormatLocale()` (everything
   `Intl`-numeric or `Intl`-temporal). `getFormatLocale()` always reads the OS, never the setting.
2. Repoint consumers: `messages.svelte.ts` → `getUiLocale()`; `number-format.ts`, `settings/format-utils.ts`
   (`getSystemLocaleFormatter`), and `views/measure-column-widths.ts`'s path → `getFormatLocale()`.
3. `setLocale()` writes only the UI half. The compiled-message cache clear and rune bump stay as they are.
4. **Investigate and record**: what `Intl.DateTimeFormat().resolvedOptions()` reports on a machine with a region
   override (`en_US@rg=sezzzz`), i.e. whether the `-u-rg-` extension survives into the webview. If it doesn't, note it
   in `DETAILS.md` with the evidence-anchor format; don't build around it in this milestone.

**Tests** (unit; written after the split, since this is a mechanical repoint with one behavioral assertion)

- With UI language pinned to `hu` and the OS format locale `sv-SE`, a formatted size and a `'system'`-mode date come out
  Swedish while the copy comes out Hungarian. This is the whole point of the milestone; it's the one test that must
  exist.
- `en-us-parity.test.ts` must stay green untouched: an en-US machine sees no change whatsoever.
- The `no-raw-locale-format` ESLint rule must still fire on a raw `toLocaleString` after the rename.

**Docs**: `apps/desktop/src/lib/intl/CLAUDE.md` — the "read the locale ONLY via `getLocale()`" guardrail becomes
"UI copy reads `getUiLocale()`, formatters read `getFormatLocale()`, and nothing else resolves a locale". `DETAILS.md`
gets the decision and the macOS language-vs-region rationale. Check whether `cmdr/no-raw-locale-format`'s message needs
the new names.

**Checks**: `pnpm check desktop`, plus `pnpm check` (the lint rule and the parity test both live under it).

---

### M3: Follow a live OS language change

**Intent**: `'system'` should mean *currently* system, not "system as of app launch". macOS nudges users to restart
apps after a language change, but an app that just does the right thing is better than one that asks.

**Changes**

1. Rust: observe `NSCurrentLocaleDidChangeNotification` (`kCFLocaleCurrentLocaleDidChangeNotification`) via `objc2`,
   registered on the main thread alongside the other system observers. On fire, re-read `apple_languages()` and emit a
   Tauri event carrying the fresh ordered list. Debounce (the notification arrives in bursts when several preferences
   change at once) and skip the emit when the list is unchanged.
2. Frontend: on that event, re-run `pickUiLocale` and call `setLocale(...)` when the answer moved. The rune bump gives
   the live re-render for free; the formatters re-key on the new format locale.
3. The event also drives the menu rebuild once M4 lands (wire the call then, not now).
4. Linux: no equivalent signal. Skip explicitly and say so in a comment, so nobody goes looking.

**Tests**

- Rust unit: the debounce collapses a burst into one emit; an unchanged list emits nothing.
- Frontend unit: an incoming event with a different list re-resolves and re-renders (the existing
  `messages.svelte.test.ts` reactivity fixture is the model); an event with the same list does not bump the rune.
- Manual: change the macOS language while Cmdr runs and watch it follow. Record the result in the milestone's commit
  message, since this is the one part no automated test really proves.

**Docs**: `apps/desktop/src-tauri/src/system_strings.rs` module doc (or the observer's home) plus a line in the intl
`DETAILS.md` on what `'system'` now tracks.

---

### M4: Localize the native menu bar

**Intent**: this is the prerequisite that makes auto-detection honest. It's also the largest milestone by far, and it's
the one to cut first if the effort has to shrink — but cutting it means accepting an English menu bar over a translated
app for every non-English user, which we know is already happening today.

**Two landmines found while planning; neither is optional**:

- `set_macos_menu_icons` (`macos.rs:630`) maps SF Symbols to items **by their English title string**
  (`"File" => [("Open", "arrow.up.forward"), …]`). Localize the titles and every icon silently disappears.
- `cleanup_macos_menus` (`macos.rs:559`) finds the Edit and Help menus **by title** to strip AppKit's injected items.
  Localize and the Writing Tools / AutoFill / Dictation items come back.

Both are also project-hard-rule violations already (❌ never classify control flow by string-matching), so fixing them
is owed regardless. **Fix them before touching a single title literal**, as their own commit with the existing menu
tests green, so that a later icon regression can't be blamed on the translation.

**Changes**

1. **Refactor first**: re-key icon assignment and AppKit cleanup off menu item ids (`command_map.rs` already holds the
   id constants) instead of titles. Existing tests stay green; `register_item_positions_match_submenu_order` is
   unaffected (it parses binding names, not titles — confirmed).
2. Add `menu.*` keys to the `en` catalog with full `@key` metadata (surface, trigger, do-not-translate tokens), per
   `apps/desktop/src/lib/intl/messages/DETAILS.md`. Roughly 150–200 keys once duplicates across `macos.rs`/`linux.rs`
   collapse.
3. A generator emits a Rust-side lookup for the `menu.*` subset of each catalog (mirroring how `keys.gen.ts` is
   generated; never hand-edited, and wired into the same regeneration path). Rust gets a `menu_t(key)` reading a
   static map plus the active UI locale. Static labels only: no ICU in Rust, no plurals, no parameters. If a menu label
   ever needs a parameter, that's a signal to reshape the label, not to import ICU.
4. Menu construction calls `menu_t`. Rebuild the whole menu bar on locale change (rare; correctness beats cleverness).
   Re-run `cleanup_macos_menus` and `set_macos_menu_icons` after the rebuild, exactly as the focus-swap path already
   does.
5. Check `PredefinedMenuItem::quit`/`about` and the other muda predefined items: they carry their own titles. Confirm
   whether they localize themselves; if not, pass explicit text. (David has open muda PRs, so he'll know the shape of
   that code faster than a fresh read will.)
6. **Then translate**: run the documented translator process (`docs/guides/i18n-translation.md`) for all nine locales.
   The reference pile is mandatory and lives ONLY in the main clone at
   `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/` — a worktree-relative path does not exist, and concluding "no
   reference pile" from a worktree is the documented trap. Menu labels are exactly the strings the macOS Finder
   reference is authoritative for, so this should be a high-confidence pass.

**Tests**

- Rust unit, **test-first** for the re-key refactor (it's a behavior-preserving change to code with no current guard,
  which is precisely when TDD pays): every icon mapping resolves to a live item id; every id in the cleanup path
  exists. Watch these fail against a deliberately-wrong id before fixing.
- Rust unit: `menu_t` returns the locale's string, falls back to English on a missing key, and never panics on an
  unknown key.
- `desktop-i18n-coverage` extends to the new keys automatically — that's the point of putting them in the same catalog.
  Confirm it fails before the nine translations land, and passes after.
- E2E: the existing menu specs assert English labels. Pin the locale (see M8) so they keep asserting English rather
  than whatever the machine speaks.

**Docs**: `apps/desktop/src-tauri/src/menu/CLAUDE.md` (a guardrail: labels come from `menu_t`, icons and cleanup key off
ids, ❌ never off titles) and its `DETAILS.md` (the generator, the rebuild-on-locale-change path). Note in
`invariant-density` terms: the two `❌ never string-match a title` rules this adds are paid for by two rule *removals*
elsewhere, since the string-matching they forbid is what we deleted.

**Checks**: `pnpm check rust`, `pnpm check desktop`, then a full `pnpm check` before the milestone closes.

---

### M5: The escape hatch

**Intent**: a user who lands in a language they don't read must be able to get out without navigating a settings tree
whose every label is in that language.

**Changes**

1. **First-launch (new users)**: a compact language control in `OnboardingWizard`'s shell, visible from step 1. Not a
   new step (the wizard's step contract is deliberate and step 3 is non-skippable — don't disturb it); a small control
   in the frame. It writes `appearance.language` like the settings picker does, reusing `setSetting()` per the module's
   "don't fork the existing wiring" rule.
2. **Already-onboarded users whose language moves because of M1**: a one-time dismissible inline bar. Sentence in the
   detected language, plus a plain `English` button that writes `appearance.language = 'en'`. Shown once; dismissal or
   any explicit language pick retires it forever, stored as a `hidden: true` setting alongside the other internal flags.
3. The bar's own copy is a catalog key like everything else, so it renders in the detected language. The `English`
   button label stays the literal string `English` in every locale (that's the point of it) — mark it
   `@key.sameAsSourceJustification` so the coverage check accepts an identical value.

**Tests**

- Unit: the bar shows exactly once for a user whose resolved language changed; never for an `en` resolution; never
  again after dismissal; never for a user with an explicit `appearance.language`.
- Unit: the `English` button writes the setting and retires the bar in one action.
- a11y test alongside the component, per the module convention (`*.a11y.test.ts` sits next to every UI component here).
- E2E: worth one spec, since "can the user escape" is the safety property of this whole feature.

**Docs**: `apps/desktop/src/lib/onboarding/CLAUDE.md` (the control's existence and why it isn't a step), and the intl
`DETAILS.md` for the one-time-bar trigger condition.

---

### M6: Name what "System default" resolved to

**Intent**: "System default" tells the user nothing about what they'll get. "System default (Svenska)" is
self-describing, and it costs one string.

**Changes**: `languageOptions()` in `definitions/appearance.ts` composes the resolved endonym into the `'system'`
option's label using the existing `localeDisplayName()`. The label must re-derive when the resolution changes (M3), so
it can't be computed once at module load the way the current options array is — that's the only real subtlety here.

**Tests**: unit, extending `settings-registry.test.ts`'s existing `appearance.language` block: the system option's
label names the resolved language; it updates when the resolution changes; it degrades to a bare "System default" when
resolution yields English or fails.

**Docs**: one line in the intl `DETAILS.md` picker section.

---

### M7: Learn what people actually use

**Intent**: David wants to see which languages are in use, and whether auto-selection is landing well. The undo click
is the honest quality signal we're deliberately not asking for in the UI.

**Changes**: two events through the existing `track_event` IPC (`commands/analytics.rs` → `posthog::capture`), riding
the existing consent gate:

- `language_resolved` on startup: props `detected` (the first shipped-language match, or `none`), `active` (what the app
  is actually running in), and `source` (`auto` / `explicit` / `fallback`).
- `language_reverted` when the escape hatch is used: props `from` (the language they left).

Send the **base language subtag only** (`hu`, not `hu-HU`). A rare language plus a region narrows a population more
than we need to; the base subtag answers David's question completely. Add `appearance.language` to
`CATEGORICAL_STRING_KEYS` in `config_shape.rs` so the heartbeat config shape carries the setting's value too — it's a
categorical string by construction, which is exactly what that allowlist is for.

**Tests**: Rust unit for the props being categorical (the existing `excludes_pii_shaped_strings` invariant covers the
config shape; add a case for the new key). Frontend unit that the events fire once per startup and once per revert, not
per re-render.

**Docs**: `apps/desktop/src-tauri/src/analytics/DETAILS.md` event set (the doc that already catalogs every event and
where it fires).

---

### M8: Pin the locale in the test harness

**Intent**: the Playwright coupling pass and the marketing/i18n screenshot pipeline assume English. Once resolution
follows the machine, a non-English dev machine or CI runner silently rewrites every asserted string and every captured
screenshot.

**Changes**: force `appearance.language = 'en'` in the E2E harness setup and the screenshot capture build, leaving the
pseudolocale override path (`setLocale('en-XA')` for the overflow pass) exactly as it is. Check the Linux Docker E2E
path too.

**Tests**: run the suites on a machine with a non-English preference (or with `-AppleLanguages` injected) and confirm
they still assert English. This is the milestone's whole proof, so don't skip it for a green run on an en-US machine.

**Docs**: `apps/desktop/test/e2e-playwright/CLAUDE.md` — a guardrail that the harness pins the locale and why.

**Checks**: `pnpm check --include-slow` once at the end, since this milestone is the one that can break the slow lanes.

---

## Risks and how they're handled

- **M4 is most of the work.** Everything else is small and mechanical. If the effort has to shrink, M4 is the cut, and
  the consequence is explicit: non-English users keep an English menu bar (as they do today), so cutting it means
  choosing not to fix a known gap rather than deferring a new one.
- **The `-AppleLanguages` caveat.** The M1 validation task settles what today's users experience. If a global
  preference does *not* trigger detection today, then M1 turns auto-language on for real users for the first time, which
  raises M4 from "prerequisite" to "hard blocker" and changes the release note. Do that validation before writing code.
- **The icon and cleanup re-key is invisible when it breaks.** SF Symbols quietly vanishing from menus is not something
  a test suite notices by default, which is why M4 puts the re-key first, with its own tests, before any title changes.
- **Region-override behavior in the webview is unknown.** M2 records it rather than guessing at it.

## Parallelism

Mostly sequential, which is fine. Two genuinely safe overlaps:

- **M4's translation pass** (step 6) can run while M5–M7 proceed, once the `en` menu keys exist and are frozen. It
  touches only `messages/<tag>/*.json` for the nine non-`en` locales.
- **M8** touches only test harness files and can land any time after M1.

Everything else shares `locale.ts` / `messages.svelte.ts` and should stay in order.

## Open questions for David

1. **M4 in or out?** It's the difference between "auto-language, done properly" and "auto-language, with an English
   menu bar". Recommendation: in — it's already a live gap, and it's the one thing that makes the feature honest.
2. **The one-time bar for existing users (M5.2)**: worth it, or is the onboarding control plus the settings picker
   enough? It only fires for people whose language actually moves because of M1's ordered walk, which may be a very
   small group.
