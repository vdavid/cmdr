# English (UK) (en-GB) glossary

Every term this overlay forks from base `en`, with its source. Rulings and the reasoning behind them live in `style.md`;
this is the lookup table. `en-AU` shares all of it except where `docs/i18n/en-AU/style.md` says otherwise.

Sources are the reference pile (`_ignored/i18n/en-GB/`, recipes in `docs/i18n/reference-pile/how-to-mine.md`) and the
live OS, both read on macOS 26.6.2 (build 25G83), 2026-08-29.

## Terminology

| en (source)          | en-GB           | Source                                                                            |
| -------------------- | --------------- | --------------------------------------------------------------------------------- |
| Trash (the location) | Bin             | `Finder/Localizable.json:Trash`, `LocalizableMerged.json:N39`, `:PW30`            |
| Move to Trash        | Move to Bin     | `Finder/LocalizableMerged.json:AL13`, `:N153`, `Finder/MenuBar.json:300787.title` |
| Empty Trash…         | Empty Bin…      | `Finder/LocalizableMerged.json:A3`, `:AL10`, `:N157`                              |
| in / to the Trash    | in / to the Bin | `Finder/LocalizableMerged.json:NE53`, `:MT43`, `AppKit/Document.json`             |
| Go to Trash          | Go to the Bin   | `Finder/LocalizableMerged.json:TL_HELP_TCAN`                                      |
| to trash (verb)      | to bin          | Standard BrE; no Apple attestation (Apple has no verb form)                       |
| license (noun)       | licence         | `Software Update.app/…/{en,en_GB}.lproj/LegalText*.rtf`, 6 minimal pairs          |
| licensing            | licensing       | Verb-derived, unchanged in BrE                                                    |
| color                | colour          | `AppKit/Accessibility.json:color`, `NSColorPanelExtras.json:Colors`               |
| favorite(s)          | favourite(s)    | `AppKit/FontManager.json:Favorites`, `Finder/LocalizableMerged.json:FI10`, `:TG4` |
| behavior             | behaviour       | Standard BrE `-our`; no catalog attestation                                       |
| gray                 | grey            | `AppKit/NSColorPanelExtras.json:Gray`, `Finder/LocalizableMerged.json:TG_COLOR_1` |
| recognize            | recognise       | `Finder/LocalizableMerged.json:BN6`                                               |
| organize             | organise        | `SystemSettings/MainMenu.json:8C9-qM-axf.title` ("Organise by Categories")        |
| customize            | customise       | `AppKit/Toolbar.json:Customize Toolbar…`, `Finder/LocalizableMerged.json:N274`    |
| synchronize          | synchronise     | `AppKit/AccessibilityImageDescriptions.json:NSSynchronize`                        |
| minimize             | minimise        | Standard BrE `-ise`; no catalog attestation                                       |
| virtualization       | virtualisation  | Standard BrE `-ise`; no catalog attestation                                       |
| prioritize           | prioritise      | Standard BrE `-ise`; no catalog attestation                                       |
| organization         | organisation    | Standard BrE `-ise`; no catalog attestation                                       |
| canceled             | cancelled       | `Finder/LocalizableMerged.json:MR16`, `:MR5`                                      |
| canceling            | cancelling      | `AppKit/Printing.json:Canceling…`                                                 |
| Cancel (the button)  | Cancel          | Unchanged; `AppKit/Common.json:Cancel` identical in all three                     |
| aging                | ageing          | Standard BrE; no catalog attestation                                              |
| toward               | towards         | Standard BrE                                                                      |
| gotten               | got             | Standard BrE (`gotten` is US-only)                                                |
| percent              | per cent        | `Finder/LocalizableMerged.json:PW13.2`                                            |
| go forward           | go forwards     | `AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate`                  |

## Verified unchanged

Terms a file manager reaches for that Apple does NOT fork in `en-GB`. Don't invent a difference for these: folder,
directory, disk (storage), disc (optical media), drive, eject, unmount, mount, archive, Zip archive, compress, Get Info,
rename, duplicate, Move To, Copy To, find, search, tag, sidebar, preview, Quick Look, permissions, Full Disk Access,
Storage, Available, Used, Capacity, network, server, share, Connect to Server, Sign In, Settings, Command / Option /
Control / Shift, KB / MB / GB / bytes, Put Back, dialog, Cancel, set up.

## Not attested anywhere

Words with no `en-GB` evidence in the pile at all, so nothing to copy if one enters the catalog later: `defence`,
`offence`, `catalogue`, `analogue`, `analyse`, `travelling`, `labelled`, `programme`, `practise`, `judgement`,
`acknowledgement`, `metre` / `litre` (only `centimetre` is attested), `amongst`, `whilst`, `learnt`. Reach for a
dictionary and record the call here rather than guessing silently.

## Traps

- **`Finder/LocalizableMerged.json:KIND_FORMATTER_9_1`** is `ZIP` in `en`/`en-GB` and `Postcode` in `en-AU`. It's a
  Spotlight address field, not the archive format. `Finder/CompressWithOptions.json` says `Zip archive` in every locale.
  Don't rename any zip UI from a naive value-grep.
- **`AppKit/Common.json:Trash`** is `Delete` in `en-GB`. That's AppKit's generic destructive button, not the Bin. See
  `style.md` § Bin.
- **`en-GB/xfce-thunar/`, `gnome-nautilus/`, `kde-dolphin/`** call the Trash `Wastebasket` / `Wastebin`. Linux desktops
  are Tier 3 evidence; for a macOS app, Finder wins.
