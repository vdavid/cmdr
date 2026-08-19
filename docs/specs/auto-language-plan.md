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
  - **Confirmed with a GLOBAL preference too** (2026-08-19): with `defaults write -g AppleLanguages -array hu-HU en-US`
    and no per-app override, a relaunched production Cmdr came up Hungarian. So this is not an artifact of the
    app-domain `-AppleLanguages` argument, and auto-detection genuinely reaches real users today.
  - **Which makes the English menu bar a live shipping defect, not a hypothetical.** In both runs the menu bar stayed
    `File / Edit / Select / View / Go / Tab / Window / Help` over a fully Hungarian app. Every Hungarian, German,
    Spanish, French, Dutch, Portuguese, Swedish, Vietnamese, and Chinese macOS user running Cmdr sees this right now.
    That's the strongest argument for M4 and it belongs in the release note.
- **Nine non-`en` locales ship**, all at 100% coverage: `de`, `es`, `fr`, `hu`, `nl`, `pt`, `sv`, `vi`, `zh`.
- **`desktop-i18n-coverage` is already an error-level check** that fails on any missing key or any value byte-identical
  to English without a `@key.sameAsSourceJustification`. "Never ship a partial locale" is therefore already an enforced
  invariant, not something this plan has to build. What the plan adds is that _auto-selection_ draws only from that
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
_Intent_: the preference list is an ordered list of the user's choices, and honoring only its first element throws away
the user's own fallback plan.

**2. UI language and formatting locale are two values, and only the first one follows the setting.** Today
`setLocale('hu')` rewrites the single `getLocale()` source, so picking Hungarian also switches dates and number grouping
to Hungarian conventions, and picking English on a Swedish Mac switches them to US conventions. macOS itself keeps
Language and Region as separate settings, and David's own machine is a live example (`AppleLocale = en_US@rg=sezzzz`: US
English, Swedish region). _Intent_: match the OS's mental model. The user chose their number and date conventions in
System Settings; the app's UI language is not a licence to overwrite them.

**3. Auto-selection never crosses a script boundary.** `zh-Hant-TW` currently falls back to the `zh` directory, which is
Simplified. For a Traditional reader that is worse than English, because English is at least a language they chose to
list. The guard compares the candidate's likely script against the catalog's (see M1 for where that data comes from, a
decision that follows from where the resolver lives). `docs/i18n/script-decisions.md` records nine languages with a
script split, so this is not a one-off for Chinese. _Intent_: a fallback is only a kindness when it lands somewhere the
reader can actually read. An explicit pick in the picker is the user's business and carries no such guard.

**3b. Regional variants DO fall back, deliberately.** The script guard is about legibility, not about dialect, so
`pt-PT` lands on the Brazilian `pt` catalog and `en-GB` lands on US `en` ("Trash", `-ize`). That's the documented roster
decision (`docs/i18n/language-selection-decisions.md` lists `pt-PT` and `en-GB` as wave-2 variants), and reading a
sibling dialect is a small friction next to reading a language you don't speak. _Intent_: don't confuse "wrong dialect"
with "unreadable" — the first is a papercut a fast-follow locale fixes, the second is a wall. Say this out loud in the
docs so nobody later "fixes" it by blocking regional fallback.

**4. Auto-selection draws only from shipped, complete catalogs, and never from the pseudolocale.** `availableLocales()`
is already the gated set (`desktop-i18n-coverage` guarantees completeness), but it includes `en-XA` in dev builds.
_Intent_: "we auto-enabled a language" is a promise that the app is fully in that language. One English string in a
Hungarian dialog is a bug report; a whole English menu bar is a broken promise (which is why M4 exists).

**5. An explicit choice is permanent and an implicit one is not.** `'system'` stays a sentinel and we never write a
resolved tag back into the setting. _Intent_: writing back would freeze the user out of following the OS, and would
silently convert "I didn't care" into "I decided".

**6. The escape hatch appears where the user already is, and nowhere else.** A first-launch user meets
`OnboardingWizard` before anything else, already in the detected language, so the language control belongs in the wizard
frame. Already-onboarded users get NO notice: the settings picker is the escape hatch, and it's the same one they'd
reach for to change any other preference. _Intent_: an app that switches to your own language and says nothing about it
is behaving normally, not doing something that needs announcing. Interrupting to explain would make an ordinary act feel
like an incident.

### Explicitly not wanted (David, 2026-08-19)

- ❌ No "this translation is machine-made" notice anywhere in the UI, not even once.
- ❌ No "coming soon" rows for untranslated languages in the picker.
- ❌ No partial locales, ever, auto-enabled or otherwise. The existing error-level coverage check is the enforcement.

## Milestones

Run them in order, one at a time. Each ends green, committed, and documented. ❌ No parallel subagents: a harness bug
bleeds the working directory across agents in parallel sessions, which in a worktree means an agent can write to the
wrong checkout.

---

### M1: Resolve the UI language from the OS preference list

**Intent**: make `'system'` mean "walk the user's ordered preferences and take the first language we fully ship",
instead of "take whatever single tag the webview happens to resolve".

**Where the resolver lives: RUST** (decided; the argument is kept because the reasoning constrains the design).

Three consumers need the answer, and two of them run before the webview exists: the native menu bar (built in `setup`),
the "Cmdr is already running" alert (fires before any window), and the webview UI. That plus the project's smart-backend
/ thin-frontend principle points at **Rust owning the resolution**, with the frontend consuming the answer. The cost is
that the script guard needs CLDR likely-subtags data, which the webview gets free from `Intl.Locale.maximize()` and Rust
does not.

Resolving in Rust means getting the likely-script data from a small build-time generator that asks Node's `Intl` for it
(the same generator M4 extends to carry menu strings). It only has to cover the languages we ship, so the generated
table is small and needs no hand-maintenance. The alternative — resolve in TS and have Rust ask the frontend — means a
second resolver in Rust anyway for the pre-webview alert, which is the drift risk this whole decision is trying to
avoid. `docs/i18n/script-decisions.md` records nine languages with a script split (`zh`, `sr`, `uz`, `kk`, `mn`, `az`,
`pa`, `bs`, `be`), so the guard is not a one-off for Chinese and a hand-written table would rot.

**No startup gate.** `routes/(main)/show-main-on-mount.ts` documents, at length, a paint gate that was removed because
it cost a fixed second of startup for no signal. ❌ Don't reintroduce one here: the resolved locale must ride the
startup IPC the frontend already makes, or be injected into the webview before app code runs. If neither is possible
without adding a serialized round-trip, measure the cost and say so rather than quietly paying it. The window is created
`visible: false` and shown by the frontend, so there is no _visual_ flash to fix; the risk is purely latency.

**Changes**

1. `apps/desktop/src-tauri/src/system_strings.rs` (or a new sibling module, if this outgrows "system strings"): the
   ordered `apple_languages()` list feeds a `resolve_ui_locale()` that walks each preference in order, trying the full
   tag then its base subtag, applying the script guard, and returning `None` when nothing matches (caller uses `en`).
   Non-macOS returns `None`, and the webview default stands, which is the right answer on Linux.
2. A generator emits the shipped-locale list and their likely scripts into Rust, from the catalog dirs. Wire it into the
   same regeneration path as `keys.gen.ts`; never hand-edited.
3. The resolved tag reaches the frontend on the existing startup path, and `'system'` in `settings-applier.ts` uses it.
   `getLocale()` keeps its SSR-safety contract (no DOM, never throws) and its uncached-by-design behavior.
4. Exclude `en-XA` explicitly. It's dev-only and never appears in a real preference list, but the exclusion is one line
   and the failure mode (a tester's app in pseudolocale) is confusing enough to be worth it.

**Tests** (the walk is pure logic with sharp edges and no current guard, so **test-first, real red → green**; they live
wherever the resolver lands)

- `[hu-HU, sv-SE]` with `hu` absent → `sv`. This is the case today's code structurally cannot express; write it first
  and watch it fail for the right reason.
- `[hu-HU]` with `hu` present → `hu`; `[hu]` → `hu`; casing variants (`HU-hu`) normalize.
- `[fr-CA]` with only `fr` present → `fr` (base fallback within one preference, before advancing to the next).
- `[zh-Hant-TW]` with only Simplified `zh` present → **not** `zh`; falls through to the next preference, then English.
- `[zh-CN]` with `zh` present → `zh`, so the guard doesn't over-block the common case.
- `[pt-PT]` → `pt` and `[en-GB]` → `en`: regional fallback is allowed, so the guard must not block a same-script
  dialect. This is the mirror of the `zh-Hant` case and pins decision 3b against a later over-correction.
- `[en-US, sv-SE]` → English, and the walk stops at `en` rather than reaching Swedish.
- `[de-DE]` in a dev build where `en-XA` exists → `de`, never the pseudolocale.
- Empty / missing preference list → no match, caller lands on `en`, nothing throws.
- The generated script table covers every shipped locale (a locale added without regenerating must fail, not silently
  lose its guard).
- `apple_languages()` already has `apple_languages_returns_at_least_one_entry`; add one asserting order is preserved end
  to end, since the whole feature rests on the list being ordered.

**Validation task: DONE** (2026-08-19). Flipping the global `AppleLanguages` to `[hu-HU, en-US]` and relaunching brought
production Cmdr up in Hungarian with an English menu bar; `[en-US, sv-SE]` restored afterwards. Recorded at the top of
this plan. The upshot for M1: this milestone is a correctness fix to a feature that already ships, ❌ not the switch
that turns it on, so there's no "first exposure" risk to manage and no staged rollout to design.

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
   (`getSystemLocaleFormatter`), and `views/measure-column-widths.ts`'s path → `getFormatLocale()`. The fourth consumer
   is easy to miss: `query-ui/filter-chips/filter-popover-helpers.ts:188` feeds `getLocale()` into
   `resolveFirstDayOfWeek`, which is a calendar convention and therefore **format** locale — the preset _labels_ around
   it are catalog strings and stay on the UI language. That one line is the whole reason this milestone needs a
   deliberate per-consumer classification rather than a find-and-replace.
3. `setLocale()` writes only the UI half. The compiled-message cache clear and rune bump stay as they are.
4. **Investigate and record**: what `Intl.DateTimeFormat().resolvedOptions()` reports on a machine with a region
   override (`en_US@rg=sezzzz`), i.e. whether the `-u-rg-` extension survives into the webview. If it doesn't, note it
   in `DETAILS.md` with the evidence-anchor format; don't build around it in this milestone.
   - ❌ **Don't infer the answer from Cmdr's own date column.** It looks like free evidence (ISO dates on a US-English
     machine would suggest the Swedish region override is getting through) and it is not: David's settings carry
     `appearance.dateTimeFormat: 'iso'`, so the column proves nothing about the locale. Read `resolvedOptions()`
     directly in the webview.

**Tests** (unit; written after the split, since this is a mechanical repoint with one behavioral assertion)

- With UI language pinned to `hu` and the OS format locale `sv-SE`, a formatted size and a `'system'`-mode date come out
  Swedish while the copy comes out Hungarian. This is the whole point of the milestone; it's the one test that must
  exist.
- `en-us-parity.test.ts` must stay green untouched: an en-US machine sees no change whatsoever.
- The `no-raw-locale-format` ESLint rule must still fire on a raw `toLocaleString` after the rename.

**Docs**: `apps/desktop/src/lib/intl/CLAUDE.md` — the "read the locale ONLY via `getLocale()`" guardrail becomes "UI
copy reads `getUiLocale()`, formatters read `getFormatLocale()`, and nothing else resolves a locale". `DETAILS.md` gets
the decision and the macOS language-vs-region rationale. Check whether `cmdr/no-raw-locale-format`'s message needs the
new names.

**Checks**: `pnpm check desktop`, plus `pnpm check` (the lint rule and the parity test both live under it).

---

### M2b: Follow the user's region, not just their language

**Intent**: finish what M2 started. M2 stopped the UI language from overwriting formatting; this makes formatting
actually match the OS, which today it does not.

**The measured problem** (found during M2, recorded in `apps/desktop/src/lib/intl/DETAILS.md` with its evidence anchor):
WebKit does not hand the whole OS locale answer to the webview. On a Mac set to US English with a Swedish region
(`AppleLocale = en_US@rg=sezzzz`), Finder writes `2026-08-19` and `1 234 567,89` while our webview writes `08/19/2026`
and `1,234,567.89`. The `-u-rg-` extension is dropped, and passing it explicitly is not a workaround:
`en-US-u-rg-sezzzz` resolves straight back to `en-US`. A real region subtag DOES work, and `en-SE` reproduces
Foundation's output exactly.

So the fix is to stop asking the webview and compose the tag ourselves, on the same seam M1 already built for the
language. Elegance is the argument: one more small Rust-side answer, delivered the same way, rather than a special case
somewhere in the formatting layer.

**Changes**

1. Rust: read the current region (`Locale.current.region`, which honors the `rg=` override) and compose the format
   locale as `<language>-<REGION>`. Expose it through the same path `get_ui_locale` uses.
2. `getFormatLocale()` prefers the composed tag and falls back to the webview default when there isn't one. Everything
   downstream (`number-format.ts`, `format-utils.ts`, the first-day-of-week resolver) already reads that one function,
   so no call site changes.
3. The composed tag follows the OS, ❌ never the `appearance.language` setting — that's design decision 2 and M2b must
   not quietly undo it. A Hungarian UI on a US-English/Swedish-region Mac still formats `en-SE`.
4. The live-change path from M3 already re-emits on `NSCurrentLocaleDidChangeNotification`, which is exactly the
   notification that tracks `AppleLocale`. That's the right signal for a region change, so wire the region answer into
   the emit M3 built rather than adding a second observer.

**Tests**

- The case that motivated it: language `en`, region override `SE` → format locale `en-SE`, and a formatted date and
  grouped number match Foundation's output rather than US conventions.
- No region override (`AppleLocale = en_US`) → `en-US`, i.e. **nothing changes for the common case**.
  `en-us-parity.test.ts` must stay green untouched; if it moves, the composition is wrong.
- An unresolvable or missing region → fall back to the webview default rather than composing a malformed tag.
- An explicit UI language does not change the format locale (decision 2 held).

**Docs**: update the `DETAILS.md` finding from "here's why this user still sees US formats" to the current behavior plus
the reason the composition exists. Keep the evidence anchor and the ❌ note about Cmdr's own date column not being able
to answer this (`appearance.dateTimeFormat: 'iso'` hides it).

---

### M3: Follow a live OS language change

**Intent**: `'system'` should mean _currently_ system, not "system as of app launch". macOS nudges users to restart apps
after a language change, but an app that just does the right thing is better than one that asks.

**Changes**

1. Rust: observe `NSCurrentLocaleDidChangeNotification` (`kCFLocaleCurrentLocaleDidChangeNotification`) via `objc2`,
   registered on the main thread alongside the other system observers. On fire, re-read `apple_languages()` and emit a
   Tauri event carrying the fresh ordered list. Debounce (the notification arrives in bursts when several preferences
   change at once) and skip the emit when the list is unchanged.
   - **Copy an existing observer rather than inventing one.** `accent_color.rs:99`, `reduce_transparency.rs:74`, and
     `text_size.rs:105` each register a `block2::RcBlock` observer against a system notification centre and already
     carry the main-thread and SAFETY comments this needs. Follow whichever is closest in shape; don't hand-roll a
     fourth idiom.
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
2. Add `menu.*` keys to the `en` catalog with full `@key` metadata, per `apps/desktop/src/lib/intl/messages/DETAILS.md`.
   Roughly 150–200 keys once duplicates across `macos.rs`/`linux.rs` collapse.

   **This metadata is the milestone's real deliverable, not paperwork** (David's explicit rider on approving M4: set the
   translators up for success). A menu label is one or two words with no sentence around it, so the description carries
   all the context there is. Every key needs: which menu it sits in, what activating it does, whether the word is a VERB
   or a NOUN in English (`Open`, `View`, `Copy`, `Move`, `Search` are all ambiguous, and they resolve differently in
   most target languages), any do-not-translate token, and a width note where the item sits in a tight submenu. Where
   the reference pile has an obvious counterpart, name it: "Finder's File > Open" tells a translator more than three
   sentences of prose. A bare `"Open"` with no description is a coin flip.

3. A generator emits a Rust-side lookup for the `menu.*` subset of each catalog (mirroring how `keys.gen.ts` is
   generated; never hand-edited, and wired into the same regeneration path). Rust gets a `menu_t(key)` reading a static
   map plus the active UI locale. Static labels only: no ICU in Rust, no plurals, no parameters. If a menu label ever
   needs a parameter, that's a signal to reshape the label, not to import ICU.
4. Menu construction calls `menu_t`. Rebuild the whole menu bar on locale change (rare; correctness beats cleverness).
   Re-run `cleanup_macos_menus` and `set_macos_menu_icons` after the rebuild, exactly as the focus-swap path already
   does.
5. Check `PredefinedMenuItem::quit`/`about` and the other muda predefined items: they carry their own titles. Confirm
   whether they localize themselves; if not, pass explicit text. (David has open muda PRs, so he'll know the shape of
   that code faster than a fresh read will.)
6. **Two non-menu Rust strings ride along**, because they need the same Rust-side lookup and nothing else will ever give
   them one:
   - `licensing/app_status.rs:448` `get_window_title` → `"Cmdr – Personal use only"`, set on the main window at
     `lib.rs:795`. Highly visible, and it sits right next to a translated app.
   - `instance_lock.rs:48` `ALERT_TITLE` / `ALERT_BODY` → the native "Cmdr is already running" alert. This one is the
     strongest argument for a Rust-side catalog rather than an IPC hand-off: the alert fires _before_ the webview
     exists, so a frontend-supplied string could never reach it.
7. **Then translate**: run the documented translator process (`docs/guides/i18n-translation.md`) for all nine locales.
   The reference pile is mandatory and lives ONLY in the main clone at `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/`
   — a worktree-relative path does not exist, and concluding "no reference pile" from a worktree is the documented trap.
   Menu labels are exactly the strings the macOS Finder reference is authoritative for, so this should be a
   high-confidence pass.

**The checks that break the moment `menu.*` keys exist** (found while planning; budget for them, they're not
incidental):

- **`message-keys-unused`** scans the frontend for literal `t('…')` / `getMessage('…')` usage. Keys consumed only from
  Rust look like orphans. Its own doc says the `unusedKeyDynamicPrefixes` allowlist is closed and ❌ must not be widened
  to silence an orphan — and it's right, because these aren't orphans, they're used from a language the scanner doesn't
  read. **Teach the scanner to also scan Rust for `menu_t("…")` literals.** Allowlisting `menu.` would blind us to a
  genuinely dead menu key forever.
- **`message-screenshots-fresh`** and the `@key.screenshot` coupling harness drive the webview surface by surface.
  Native menu strings never render in the webview, so they can't be coupled. They need a documented exemption in the
  coupling pipeline, not a fake screenshot.
- **`message-key-naming`**: confirm the `menu.*` shape satisfies it before writing 200 keys under it.
- **Bundle weight**: the frontend loads `messages/*/*.json` wholesale, so menu strings it never uses would ride along in
  the JS bundle (~60 KB across ten locales). Accept that for now: one catalog and one set of checks is worth more than
  the bytes. Splitting `menu.json` out and excluding it from the glob is the fallback, and it walks straight into the
  gotcha `intl/CLAUDE.md` already documents about the `screenshots/` sibling, so only do it with a real reason.

**Tests**

- Rust unit, **test-first** for the re-key refactor (it's a behavior-preserving change to code with no current guard,
  which is precisely when TDD pays): every icon mapping resolves to a live item id; every id in the cleanup path exists.
  Watch these fail against a deliberately-wrong id before fixing.
- Rust unit: `menu_t` returns the locale's string, falls back to English on a missing key, and never panics on an
  unknown key.
- `desktop-i18n-coverage` extends to the new keys automatically — that's the point of putting them in the same catalog.
  Confirm it fails before the nine translations land, and passes after.
- E2E: the existing menu specs assert English labels. Pin the locale (see M8) so they keep asserting English rather than
  whatever the machine speaks.

**Docs**: `apps/desktop/src-tauri/src/menu/CLAUDE.md` (a guardrail: labels come from `menu_t`, icons and cleanup key off
ids, ❌ never off titles) and its `DETAILS.md` (the generator, the rebuild-on-locale-change path). Note in
`invariant-density` terms: the two `❌ never string-match a title` rules this adds are paid for by two rule _removals_
elsewhere, since the string-matching they forbid is what we deleted.

**Checks**: `pnpm check rust`, `pnpm check desktop`, then a full `pnpm check` before the milestone closes.

---

### M5: The escape hatch

**Intent**: a user who lands in a language they don't read must be able to get out without navigating a settings tree
whose every label is in that language.

**Scope**: the onboarding control only. The one-time notice for already-onboarded users is **cut** (David, 2026-08-19:
silent is fine). ❌ Don't build it speculatively.

**Changes**

1. A compact language control in `OnboardingWizard`'s shell, visible from step 1. ❌ Not a new step: the wizard's step
   contract is deliberate and step 3 is non-skippable, so don't disturb the sequence. A small control in the frame. It
   writes `appearance.language` through `setSetting()`, the same wiring the settings picker uses, per the module's
   "don't fork the existing wiring" rule.
2. The `English` option label stays the literal string `English` in every locale — that's the entire point of it, since
   someone who can't read the current language has to recognize the way out. Mark it `@key.sameAsSourceJustification` so
   the coverage check accepts a value identical to English.

**Tests**

- Unit: the control writes `appearance.language` and the UI switches in place, no restart.
- Unit: picking a language in onboarding retires `'system'` for good (decision 5: an explicit choice is permanent).
- a11y test alongside the component, per the module convention (`*.a11y.test.ts` sits next to every UI component here).
- E2E: worth one spec, since "can the user escape" is the safety property of this whole feature.

**Docs**: `apps/desktop/src/lib/onboarding/CLAUDE.md` (the control's existence and why it isn't a step).

---

### M6: Name what "System default" resolved to

**Intent**: "System default" tells the user nothing about what they'll get. "System default (Svenska)" is
self-describing, and it costs one string.

**Changes**: `languageOptions()` in `definitions/appearance.ts` composes the resolved endonym into the `'system'`
option's label via the existing `localeDisplayName()`.

The re-derivation worry turns out to be a non-issue, and the milestone is smaller than it looks: `resolveOption`
(`settings-registry.ts:71`) passes an option with a literal `label` through **unchanged**, and "unchanged" includes a
getter. So the `'system'` option can carry `get label() { return tString('…opt.system', { language: … }) }` and it
re-evaluates on every read, already reactive through the locale rune, with **zero changes to the settings registry**.
Give the message key a `{language}` placeholder rather than concatenating strings, so word order stays the translator's
decision.

**Tests**: unit, extending `settings-registry.test.ts`'s existing `appearance.language` block: the system option's label
names the resolved language; it updates when the resolution changes; it degrades to a bare "System default" when
resolution yields English or fails.

**Docs**: one line in the intl `DETAILS.md` picker section.

---

### M7: Learn what people actually use

**Intent**: David wants to see which languages are in use, and whether auto-selection is landing well. The undo click is
the honest quality signal we're deliberately not asking for in the UI.

**Changes**: two events through the existing `track_event` IPC (`commands/analytics.rs` → `posthog::capture`), riding
the existing consent gate:

- `language_resolved` on startup: props `detected` (the first shipped-language match, or `none`), `active` (what the app
  is actually running in), and `source` (`auto` / `explicit` / `fallback`).
- `language_changed` when the user picks a language by hand, from onboarding or from settings: props `from` (what they
  left) and `surface` (`onboarding` / `settings`). This is the honest quality signal, and it's the only one we get:
  nothing in the UI asks how the translation reads, so a user walking away from their own language is the strongest
  evidence we have that a locale is bad.

Send the **base language subtag only** (`hu`, not `hu-HU`). A rare language plus a region narrows a population more than
we need to; the base subtag answers David's question completely. Add `appearance.language` to `CATEGORICAL_STRING_KEYS`
in `config_shape.rs` so the heartbeat config shape carries the setting's value too — it's a categorical string by
construction, which is exactly what that allowlist is for.

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
- **The `-AppleLanguages` caveat.** The M1 validation task settles what today's users experience. If a global preference
  does _not_ trigger detection today, then M1 turns auto-language on for real users for the first time, which raises M4
  from "prerequisite" to "hard blocker" and changes the release note. Do that validation before writing code.
- **The icon and cleanup re-key is invisible when it breaks.** SF Symbols quietly vanishing from menus is not something
  a test suite notices by default, which is why M4 puts the re-key first, with its own tests, before any title changes.
- **Region-override behavior in the webview is unknown.** M2 records it rather than guessing at it.

## Parallelism

Mostly sequential, which is fine. Two genuinely safe overlaps:

- **M4's translation pass** (step 6) can run while M5–M7 proceed, once the `en` menu keys exist and are frozen. It
  touches only `messages/<tag>/*.json` for the nine non-`en` locales.
- **M8** touches only test harness files and can land any time after M1.

Everything else shares `locale.ts` / `messages.svelte.ts` and should stay in order.

## Decisions (David, 2026-08-19)

All four questions the plan opened with are answered; nothing here is pending.

1. **M4 is IN.** The native menu bar gets localized. Explicit rider: **the new strings must carry enough `@key` context
   that a translator is set up for success** — surface, trigger, what the item does, constraints, do-not-translate
   tokens. A menu label is two words with no sentence around it, so the metadata IS the context; a bare `"Open"` with no
   description is a coin flip between a verb and an adjective in half the target languages.
2. **The resolver lives in Rust.** The likely-script data comes from the build-time generator, as argued in M1.
3. **No one-time notice for already-onboarded users.** Silent is fine. M5 keeps only the onboarding control; the
   persistent-toast half is cut. (Keep the toast idea out of the code entirely — don't build it "just in case".)
4. **The M1 validation may flip the global `AppleLanguages` to Hungarian**, then restore `[en-US, sv-SE]`.
5. **M2b is IN** (added 2026-08-19, after M2's investigation measured the problem): compose the format locale from the
   OS region so a region override actually reaches the formatters. David's call: "it sounds like the elegant solution".

**Execution constraint**: work SEQUENTIALLY, no parallel subagents. There's a harness bug where the current working
directory bleeds across agents in parallel sessions, which in a worktree means an agent can write to the wrong checkout.
The parallelism notes above stand as descriptions of what is logically independent, ❌ not as licence to run agents
concurrently.
