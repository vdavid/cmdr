# English (Australia) (en-AU) style guide

`en-AU` is an **overlay** of base `en`, and it agrees with `en-GB` about almost everything.

**Read `docs/i18n/en-GB/style.md` first.** Every ruling there (Bin capitalisation and its count-noun grammar, `licence`
vs `license`, the deliberate non-fork of the Oxford comma, the fork test, the considered-and-skipped list, and the
catalog mechanics) applies here unchanged and is NOT repeated. This file records only where Australian English diverges
from British.

## Inheritance: `en-AU` does not read `en-GB`

`inheritableAncestors` (`apps/desktop/src/lib/intl/locale-inheritance.ts`) resolves a tag's ancestors by dropping
subtags, so `en-AU` inherits from `en` and nothing else. `en-GB` is a sibling, not an ancestor.

**So every British form this locale shares has to be written into this catalog too.** 149 of its 159 keys are
byte-identical to `en-GB`. That duplication is the price of the inheritance rule, and there's no chain that removes it:
adding one would mean telling a checker that `en-AU` may fall back to a catalog the runtime never consults. When you
edit one catalog, edit the other.

## Where AU diverges from GB

Only two places, out of 159 keys.

### `Deselect` → `Unselect` (10 keys, AU-only)

Australian Finder's Edit menu reads `Unselect All` where the British one reads `Deselect All`
(`Finder/MenuBar.json:300488.title`), and the swap runs consistently through the prose:
`Finder/LocalizableMerged.json:NE18` (`deselect “Locked”` → `unselect ‘Locked’`), `:NE43` (`is deselected` →
`is unselected`), `:PE14`, `:BN43`. Verified in the reference pile, 2026-08-29.

So Cmdr's whole selection surface forks: the two commands, their descriptions, the two native menu items, the settings
description that names the dialog, and the dialog itself (`selection.dialog.title.remove`,
`selection.action.deselect.label`, `selection.action.deselect.tooltip`). `menu.select.deselectAll` and
`menu.select.deselectFiles` reach the real macOS menu bar through `native_strings.gen.rs`, so this is the visible proof
the overlay hits native surfaces.

**The dialog three are why the fork has to be complete.** The menu item and the dialog it opens are the same sentence to
a user: while the dialog's title and button were hardcoded English, `en-AU` read `Unselect files…` in the menu bar and
`Deselect files` in the window that opened. The positive verb doesn't fork: `Select` and `Select All` are identical in
`en-GB` and `en-AU` (`Finder/MenuBar.json`), so only the removing side of each pair is here.

### `Go forward` stays (1 key NOT forked)

`en-GB` forks `go forward` → `go forwards`. `en-AU` keeps the American form: Apple leaves
`AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate` and `:NSTouchBarGoForwardTemplate` as `go forward` in
`en-AU`. So `commands.navForward.label` is deliberately absent from this catalog and inherits base `en`.

## Australian forks with nothing to fork in Cmdr today

All verified in the pile, all absent from Cmdr's catalog. Fork them if the copy ever grows one:

- `Slideshow` → `Slide show` (`Finder/LocalizableMerged.json:N169.23_V1..V3`, `:TL_HELP_QUIK`).
- `Recents` → `Recent` (7 attestations, including `Finder/MenuBar.json:300636.title` and `Finder/Toolbar.json`).
- `Newest` → `Latest` (`Finder/Localizable.json:Newest`).
- `strikethrough` → `strike-through`, `drop down` → `drop-down`, `blockquote` → `block quote`, `Uppercase` →
  `Upper case`, `To Do` → `To-Do`: AU hyphenates and splits compounds that GB fuses.
- `Macs` → `Mac computers` (`Finder/LocalizableMerged.json:MR20`, `:MR8.2`, `:MR8.3`).
- `checkbox` → `tick box` (two words; `en-GB` writes `tickbox`, one word). `Finder/LocalizableMerged.json:BN43`.
- `popover` stays `popover` (`en-GB` writes `pop-over`). Cmdr forks neither; see `docs/i18n/en-GB/style.md` § The fork
  test.
- The cancelled-rollback toast (`fileOperations.cancelRollback.*`) forks nothing here either; its vocabulary is
  dialect-neutral, `stagedLeftover.*` included. Same reading as `docs/i18n/en-GB/style.md` § The fork test, reached
  independently.

## Claims about `en-AU` that are WRONG

Recorded because each looked plausible enough to chase, and cost time to disprove:

- **"`en-AU` lowercases `bin` where `en-GB` capitalises `Bin`."** No. Both write capital `Bin` as the proper noun.
  `en-AU` mirrors base `en`'s sentence case exactly (100% over 104 trash-bearing strings), so it writes lowercase `bin`
  only in the 12 AppKit error strings where `en` itself wrote lowercase `trash`. `en-GB` is the deviant one there,
  up-casing to `Bin` in 16 of 20 such sentences. Finder writes capital `Bin` in both locales, and Cmdr follows Finder.
- **"`en-AU` uses single quotes where `en-GB` uses double."** Not a style fork. Whole-pile curly-quote counts are
  comparable (`en-GB` 24 `‘`, `en-AU` 27). Five isolated Finder strings apply single quotes, and only in the "quotes
  defining a term" case that Microsoft's style guide carves out.
- **"`en-AU` lowercases title-case verbs."** True only for the `By` / `With` / `Up` particle family (`Open With` →
  `Open with`, `Sort By:` → `Sort by:`, `Group By` → `Group by`). Counter-examples exist in the other direction:
  `Don't ask again` → `Don't Ask Again`, and `Connect to Server` → `Connect To Server`. Cmdr is sentence case everywhere
  already, so there's nothing to fork either way.
- **"`en-AU` drops the Oxford comma less than `en-GB`."** The reverse: `en-AU` drops 42 to `en-GB`'s 21, and strips it
  between clauses where `en-GB` keeps it. Cmdr forks neither; see `docs/i18n/en-GB/style.md` § The Oxford comma.

## New Zealand

macOS ships no `en-NZ` UI localization, so New Zealand users land on this catalog or on `en-GB`. Full evidence:
`docs/i18n/en-GB/style.md` § New Zealand.

## Glossary

AU-specific terms: `glossary.md`. Everything shared with British English: `docs/i18n/en-GB/glossary.md`.
