# English (Australia) (en-AU) glossary

Australian-only terms. Everything `en-AU` shares with British English (Bin, licence, colour, favourite, grey, cancelled,
per cent, and the rest) is in `docs/i18n/en-GB/glossary.md` and is not repeated here, but it IS written into the `en-AU`
catalog: `en-AU` inherits from `en`, never from `en-GB`.

Sources are the reference pile (`_ignored/i18n/en-AU/`, recipes in `docs/i18n/reference-pile/how-to-mine.md`), read on
macOS 26.6.2 (build 25G83), 2026-08-29.

## In Cmdr's catalog today

| en / en-GB      | en-AU        | Source                                                                                    |
| --------------- | ------------ | ----------------------------------------------------------------------------------------- |
| Deselect all    | Unselect all | `Finder/MenuBar.json:300488.title` (`Deselect All` → `Unselect All`)                      |
| deselect (verb) | unselect     | `Finder/LocalizableMerged.json:NE18`, `:NE43`, `:PE14`, `:BN43`                           |
| go forwards     | go forward   | `AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate` (AU keeps the `en` form) |

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

## Traps

- **`Finder/LocalizableMerged.json:KIND_FORMATTER_9_1` is `Postcode` in `en-AU`** where `en` and `en-GB` say `ZIP`. It's
  a Spotlight address-field label, not the archive format; `Finder/CompressWithOptions.json` says `Zip archive` in every
  locale.
- **`en-AU/xfce-thunar/` is an empty pass-through catalog** (`msgid "Trash"` → `msgstr "Trash"`, most entries blank). It
  is worthless as evidence; don't cite it.
- **Apple's own `en-AU` typography is inconsistent** with `en-GB` in both directions (ellipsis vs three dots, curly vs
  straight apostrophes). Those are Apple bugs. Follow Cmdr's house style, which inherits base `en`'s ASCII-ellipsis
  convention (`docs/i18n/en/style.md`).
