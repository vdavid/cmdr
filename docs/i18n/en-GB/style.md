# English (UK) (en-GB) style guide

`en-GB` is an **overlay**, not a translation. It carries only the keys that genuinely differ from US English; every
other key resolves through the inheritance chain to the base `en` catalog. Mechanics of that contract (what coverage,
parity, and stale each do to an overlay): `docs/guides/i18n.md` § Overlay catalogs.

Because this is an overlay of the source language, most of the template's sections don't apply: there's no formality
call, no register to establish, no plural categories to rework. What this file owns is **what forks, and why**.

## Inheritance

`en-GB` inherits from `en` only. There is no `en-AU` → `en-GB` chain: `inheritableAncestors`
(`apps/desktop/src/lib/intl/locale-inheritance.ts`) walks a tag's own ancestors by dropping subtags, and `en-GB` is not
an ancestor of `en-AU`. **So every shared British form has to be written into BOTH catalogs.** `en-AU` is not a thin
patch on this one; it's a sibling that happens to agree with it on 149 of the 160 keys either one forks.

Keep the two in step by hand when you edit either. `docs/i18n/en-AU/style.md` records only where AU diverges.

## What forks

150 keys of 3,153 (4.8%). Four groups:

- **Bin (37 keys)**, the loudest one: the Trash is a destination people navigate to, and their Mac calls it the Bin.
- **Spelling (65 keys)**: `-our`, `-ise`, `-ll-`, plus `grey`, `ageing`, `towards`, and `got` for `gotten`.
- **`licence` (45 keys)**: the whole licensing surface.
- **Three odds and ends**: `per cent` (2 keys) and `Go forwards` (1, `en-GB` only).

## Rulings

### `Bin` the NOUN is always capitalised, and it's a count noun

Apple's British Finder writes **`Bin`** with a capital B in every attestation, including running prose: `Move to Bin`,
`Empty Bin…`, `Go to the Bin`, `Emptying the Bin…`, `You can recover or remove from the Bin within 30 days`,
`The item “^1” can't be moved to the Bin because it's open.` (verified in the reference pile,
`_ignored/i18n/en-GB/macOS/Finder/*.json`, 2026-08-29).

Two things a mechanical `Trash` → `Bin` replace gets wrong:

- **`trash` is a mass noun, `bin` is a count noun.** `This volume doesn't support trash.` becomes
  `This volume doesn't have a Bin.`, not the ungrammatical `...support bin`. Add the article the grammar needs.
- **Apple's article convention**: no article when the string names the command (`Move to Bin`), an article in running
  prose (`moved to the Bin`, `stayed in the Bin`).
- **The VERB stays lowercase**, because it's an ordinary verb rather than the name of a place: `Counting items to
  bin…`, `Overwrite and bin old file`, `binning`. Those three are correct as they stand. ❌ Don't "fix" them up to
  `Bin` to match the noun.

**Gotcha: AppKit and Finder disagree, and Finder wins here.** `AppKit/Common.json:Trash` is `Delete` in `en-GB` (and
`Bin` in `en-AU`), and the Touch Bar accessibility labels `NSTouchBarTrashTemplate` / `NSTrashEmpty` say `delete` in
`en-GB`. That's AppKit's generic destructive-action button, not the location. Cmdr is a file manager, so the Finder
register is the right one: **`Bin` for the place, and Cmdr's separate `menu.file.delete` stays `Delete`.** Don't let a
naive value-grep across the pile talk you into renaming the Bin to `Delete`.

Base `en` is inconsistent about capitalising `Trash` (`errors.mutation.trashNotSupported` says `Trash`,
`errors.write.trashNotSupported.message` says `trash`, for the same concept). The overlay does NOT mirror that
inconsistency; it capitalises `Bin` throughout, because a destination's name shouldn't shift case by sentence. If base
`en` ever gets tidied up, these keys go stale and get re-checked, which is the system working.

### `licence` is the noun, `license` is the verb

`Licence key`, `Licence type`, `Get a licence`, `the licence server`. The verb-derived `licensing` keeps its `s`, and so
does the `licensing.*` key prefix.

Apple's `.strings` catalogs contain no `licen*` string at all, and the Microsoft en-GB style guide doesn't rule on it
either, so this looked like an unsourced style call. It isn't. Apple's shipped legal text carries matched pairs:
`Software Update.app/Contents/Resources/{en,en_GB,en_AU}.lproj/LegalText{Update,Generic,MacOSX}.rtf` is the same
sentence three times over, reading `Software License Agreement` in `en` and `Software Licence Agreement` in both `en_GB`
and `en_AU` (verified on macOS 26.6.2, build 25G83, `textutil -convert txt`, 2026-08-29). Six minimal pairs, zero
counterexamples, and Apple applies it inside the _name_ of a legal instrument.

Counter-evidence worth knowing: Apple ships exactly ONE English macOS SLA
(`/Library/Documentation/License.lpdf/Contents/Resources/English.lproj/`, no `en_GB.lproj`), in full US register. So
Apple's position is "don't localise the contract body, do localise the UI chrome that refers to it". Cmdr's licensing
copy is entirely UI chrome, which is the localised side of that line.

The product-tier names go with it (`Personal licence`, `Commercial licence`). They're descriptive category names in
running prose, not trademarks, and a British reader parses `license` there as a spelling mistake. getcmdr.com keeps US
spelling; it's a different surface with a different audience.

### The Oxford comma: Apple reverses Cmdr's house style, and we keep ours anyway

**This is a deliberate non-fork. Don't "discover" it later and apply it.**

Apple and Microsoft both drop the serial comma in British English, and the rule is grammatical rather than blanket:

- **Noun-phrase lists lose it, 100% of the time.** `macOS, iOS, and iPadOS` → `macOS, iOS and iPadOS`;
  `CDs, DVDs, and iPods` → `CDs, DVDs and iPods`; `contacts, mail, events, webpages, and more` →
  `contacts, mail, events, web pages and more`.
- **Clause lists keep it.** `Ask the other user to quit the “%@” application, and then try again.` is untouched in
  `en-GB`.

Microsoft's en-GB style guide states exactly that split (§ 4.1.12 Punctuation → Comma). Of 63 `en` strings with a serial
comma in the pile, `en-GB` changes 32, all noun lists.

Cmdr's `AGENTS.md` mandates the Oxford comma globally. **We do not reverse it here**, for three reasons:

1. **It isn't wrong in British English.** The comma is named after Oxford University Press, which mandates it; Cambridge
   and much British technical writing use it. Apple and the Guardian drop it. It's a house-style preference on both
   sides of the Atlantic, unlike `colour` or `Bin`, which read as a mistake or the wrong word.
2. **Nobody misreads it.** A regional overlay exists to stop the app sounding foreign, not to re-house-style it.
3. **The cost is brutal.** Roughly 35–50 of our keys carry a noun-phrase serial list, and they're the longest,
   most-edited descriptive strings we have (settings summaries, onboarding, error suggestions). Each fork freezes a copy
   that goes stale on any English copy edit to that string, forever, in two catalogs. That would grow the overlay by
   half again to change punctuation nobody notices.

If David ever wants this reversed, it's a mechanical pass over the noun-list subset, and this section is the spec for
it.

### The fork test

**Fork what a British reader would misread or read as foreign. Skip what merely differs in house style.** Applying that:

- `go forward` → **`go forwards`** forks: the adverbial `-s` is a real BrE/AmE grammatical difference, and Apple forks
  it on the identical navigation control (`AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate`). The bare
  NOUN doesn't: `menu.go.forward` stays `Forward`, matching `Finder/MenuBar.json:249.title`.
- `percent` → **`per cent`** forks: one word reads American. Apple writes two in both `en-GB` and `en-AU`
  (`Finder/LocalizableMerged.json:PW13.2`).

Considered and deliberately NOT forked, so nobody re-litigates them:

- **`popover` → `pop-over`.** Attested, but only in one Touch Bar accessibility file, and `en-AU` doesn't follow. A
  hyphenation quirk in a term users don't know either way; the fork adds oddness, not clarity.
- **`%@ - %@` → `%@ – %@` (en dash).** Claimed as systematic; it's 1 flip out of 9 in the pile, and base `en` already
  uses an en dash where it matters. Not a rule. Our catalog already uses en dashes.
- **Quote punctuation moving outside (`“%@.”` → `“%@”.`).** Real in `en-GB`, but moot: no user-visible Cmdr string puts
  a full stop inside a closing quote (checked, zero matches).
- **`dialog` → `dialogue`.** Zero attestations. Apple never forks it. `dialog` stays.
- **`Deselect` → `Unselect`.** An `en-AU` fork only. British Finder keeps the American `Deselect`, in the menu
  (`Finder/MenuBar.json` `Deselect All`) and in all four prose attestations (`Finder/LocalizableMerged.json`:
  `deselect “Locked”`, `is deselected` ×2, `deselect the Locked tickbox`), where `en-AU` writes `unselect` throughout.
  Verified in the reference pile, 2026-08-29. So the whole `selection.*` area, the two Select menu items, and the
  selection commands inherit base `en` here. Don't mirror `docs/i18n/en-AU/glossary.md` on this one.
- **Date and time formats.** `en-GB` day-first, 24-hour is real (`Finder/LocalizableMerged.json:DATE_FORMATTER1`), but
  Cmdr formats dates through `$lib/intl/number-format.ts` and the OS region, not through catalog strings. Nothing to
  fork.
- **`checkbox` → `tickbox`, `webpages` → `web pages`, `signed into` → `signed in to`, `period` → `full stop`.** All
  genuine `en-GB` forks with no matching string in Cmdr's catalog today. If copy ever introduces one of these words,
  fork it then.
- **`Show less` → `Show fewer`.** Not a fork at all. `en-GB` Finder keeps `Show Less` on the button in all four
  attestations (`LocalizableMerged.json:GV6`, `:PV2`, `ColumnPreview.json:4jc-Hy-JNJ.alternateTitle`,
  `IconCollectionGroupHeaderView.json:Y6z-b0-II4.title`); the single flip, `show less options` → `show fewer options`
  (`:FI8.1`), is `fewer` in front of a countable plural noun, which is a grammar rule both dialects share. Cmdr's
  `whatsNew.dialog.showLess` is a bare button, so it inherits base `en`.
- **`Forward` → `Forwards` on the Go menu.** The adverbial `-s` rides the verb phrase only. `en-GB` writes `Forward` on
  the menu item (`Finder/MenuBar.json:249.title`, `SystemSettings/MainMenu.json:448.title`) while writing `go forwards`
  in the accessibility description, so `menu.go.forward` inherits and `commands.navForward.label` forks.

## New Zealand

**macOS ships no `en-NZ` user-interface localization**, so a New Zealand user runs English (UK) or English (Australia)
and is served by one of these two catalogs. Don't build an `en-NZ` overlay.

Verified on macOS 26.6.2, build 25G83, 2026-08-29: AppKit, Finder, System Settings, and Setup Assistant each ship
exactly three English lprojs (`en`, `en_AU`, `en_GB`), and `/System/Library` holds 2,329 of each of `en_GB.lproj` and
`en_AU.lproj` and zero `en_NZ.lproj` UI bundles. The one `en_NZ.lproj` on the system,
`ProofReader.framework/.../en_NZ.lproj/bindict.dat`, is a spell-check dictionary, not a localization; `locale -a` also
lists `en_NZ` as a formatting region. So en-NZ exists to macOS as a _region_ and a _dictionary_, never as a UI language.

## Catalog mechanics

- **Stamp `@key.sourceHash` from the base `en` value you override**, so a later English copy edit marks the fork stale.
- **A value identical to base `en` is a coverage finding**, and the fix is always to delete the key. `@key.sourceHash`
  and `reviewed` apply; `sameAsSourceJustification` does not.
- `gen-locale-skeleton.ts` refuses overlay tags and `sync-locale-keys.ts` skips them, both on purpose: they'd mirror all
  3,153 English keys. These catalogs are written by hand, which is what keeps them small.
- ICU rules are inherited from the source: double every apostrophe (`''`) in ICU families, keep them single in the raw
  `errors.*` and native `menu.*` families, and match every `{placeholder}` and `<tag>` name to base `en` exactly.
  `settings.appearance.tintTriggerAria` is the trap: its only "color" is the `{colorName}` placeholder NAME, so it must
  not be forked.
- **A forked visible label needs its `*Aria` sibling forked too** (WCAG 2.5.3 wants the accessible name to contain the
  visible one).

## Glossary

Term-by-term forks with citations: `glossary.md`.
