# English (Australia) (en-AU) glossary

Australian-only terms. Everything `en-AU` shares with British English (Bin, licence, colour, favourite, grey, cancelled,
per cent, and the rest) is in `docs/i18n/en-GB/glossary.md` and is not repeated here, but it IS written into the `en-AU`
catalog: `en-AU` inherits from `en`, never from `en-GB`.

Sources are the reference pile (`_ignored/i18n/en-AU/`, recipes in `docs/i18n/reference-pile/how-to-mine.md`), read on
macOS 26.6.2 (build 25G83), 2026-08-29.

## In Cmdr's catalog today

| en / en-GB          | en-AU          | Source                                                                                    |
| ------------------- | -------------- | ----------------------------------------------------------------------------------------- |
| Deselect all        | Unselect all   | `Finder/MenuBar.json:300488.title` (`Deselect All` → `Unselect All`)                      |
| Deselect files      | Unselect files | Same source; the Select-menu item, the command, and the dialog title all carry it         |
| deselect (verb)     | unselect       | `Finder/LocalizableMerged.json:NE18`, `:NE43`, `:PE14`, `:BN43`                           |
| Select / Select all | (no fork)      | `Finder/MenuBar.json` reads `Select` and `Select All` identically in `en-GB` and `en-AU`  |
| go forwards         | go forward     | `AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate` (AU keeps the `en` form) |

## Attested, but no matching Cmdr string yet

| en / en-GB    | en-AU          | Source                                                                   |
| ------------- | -------------- | ------------------------------------------------------------------------ |
| Slideshow     | Slide show     | `Finder/LocalizableMerged.json:N169.23_V1..V3`, `:TL_HELP_QUIK`          |
| Recents       | Recent         | `Finder/Localizable.json:Recents`, `Finder/MenuBar.json:300636.title`    |
| Newest        | Latest         | `Finder/Localizable.json:Newest`                                         |
| strikethrough | strike-through | `AppKit/NSFontOptionsPanel.json:100069.title`                            |
| drop down     | drop-down      | `AppKit/AccessibilityImageDescriptions.json:NSDropDownIndicatorTemplate` |
| blockquote    | block quote    | `AppKit/FontManager.json:ax_blockquote`                                  |
| Uppercase     | Upper case     | `AppKit/Services.json:Uppercase`                                         |
| To Do         | To-Do          | `Finder/LocalizableMerged.json:GROUP_EVENT_TODO`                         |
| Macs          | Mac computers  | `Finder/LocalizableMerged.json:MR20`, `:MR8.2`, `:MR8.3`                 |
| tickbox       | tick box       | `Finder/LocalizableMerged.json:BN43`                                     |
| pop-over      | popover        | `AppKit/TouchBar.json:Dismiss Popover` (AU keeps the `en` form)          |

## Judgment calls

Every ruling in `docs/i18n/en-GB/glossary.md` § Judgment calls applies here unchanged: the `licence` (noun) / `license`
(verb) split, capital `Bin` as a count noun, ignoring `AppKit/Common.json:Trash` (`Bin` in `en-AU`, `Delete` in `en-GB`,
and generic-destructive-button in both), and keeping the Oxford comma. Two calls land differently here:

- **`go forward` stays** (so `commands.navForward.label` is absent from this catalog and inherits), while `en-GB` forks
  the verb phrase to `go forwards`. Apple leaves `AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate` and
  `:NSTouchBarGoForwardTemplate` as `go forward` in `en-AU`. The Go-MENU noun stays `Forward` in both variants
  (`Finder/MenuBar.json:249.title`, identical in `en-GB` and `en-AU`; the base-`en` pile ships no `MenuBar.json`), so
  `menu.go.forward` is forked nowhere.
- **Only the removing verb forks, and it forks everywhere the verb appears.** `Select` is identical across `en`,
  `en-GB`, and `en-AU`, so a Select/Deselect pair forks on one side only. Fork BOTH halves of a label + tooltip pair
  together (`selection.action.deselect.label` and `.tooltip`): the tooltip repeats the button's wording, and forking one
  alone leaves a button reading `Unselect these files` under a tooltip reading
  `Deselect these files in the focused pane`.
- **`Show less` stays**, and more plainly than in `en-GB`: `en-AU` doesn't even take the countable-noun correction, so
  `Finder/LocalizableMerged.json:FI8.1` still reads `show less options` where `en-GB` says `show fewer options`. The
  bare button is `Show Less` in every locale, so `whatsNew.dialog.showLess` inherits.

`recognize` → `recognise` is shared, not Australian-only: `Finder/LocalizableMerged.json:BN6` reads `recognised` in both
`en-GB` and `en-AU`.

## Traps

- **`Finder/LocalizableMerged.json:KIND_FORMATTER_9_1` is `Postcode` in `en-AU`** where `en` and `en-GB` say `ZIP`. It's
  a Spotlight address-field label, not the archive format; `Finder/CompressWithOptions.json` says `Zip archive` in every
  locale.
- **`en-AU/xfce-thunar/` is an empty pass-through catalog** (`msgid "Trash"` → `msgstr "Trash"`, most entries blank). It
  is worthless as evidence; don't cite it.
- **Apple's own `en-AU` typography is inconsistent** with `en-GB` in both directions (ellipsis vs three dots, curly vs
  straight apostrophes). Those are Apple bugs. Follow Cmdr's house style, which inherits base `en`'s ASCII-ellipsis
  convention (`docs/i18n/en/style.md`).
