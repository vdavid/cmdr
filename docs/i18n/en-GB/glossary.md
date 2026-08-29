# English (UK) (en-GB) glossary

Every term this overlay forks from base `en`, with its source. This is the lookup table; the full argument behind each
ruling lives in `style.md`, and the picks that aren't a dictionary lookup carry a one-line reason under
[Judgment calls](#judgment-calls). `en-AU` shares all of it except where `docs/i18n/en-AU/style.md` says otherwise.

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
| license (verb)       | license         | Same split as `practice`/`practise`; no verb use in Cmdr's copy today             |
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
| go forward (verb)    | go forwards     | `AppKit/AccessibilityImageDescriptions.json:NSGoForwardTemplate`                  |
| Forward (menu item)  | Forward         | `Finder/MenuBar.json:249.title`; the noun doesn't take the adverbial `-s`         |

## Judgment calls

Six calls a dictionary can't settle, including two things we deliberately DON'T fork. Each bullet is the operative rule
plus its evidence; the full argument lives in the `style.md` section named in brackets.

- **`licence` is the noun, `license` is the verb** [§ `licence` is the noun]. So `Licence key`, `Licence type`,
  `Get a licence`, and the tier names `Personal licence` / `Commercial licence`; but `licensing` keeps its `s`, and so
  does the `licensing.*` key prefix. Apple's `.strings` files never say `licen*`, so the evidence is its shipped legal
  text: `Software Update.app/Contents/Resources/{en,en_GB,en_AU}.lproj/LegalText*.rtf` reads
  `Software License Agreement` in `en` and `Software Licence Agreement` in both variants, six minimal pairs, zero
  counterexamples. Apply it per OCCURRENCE, not per key: a value can hold both parts of speech. Cmdr's copy has no verb
  use today, so all 45 forked occurrences are nouns.
- **`Bin` is capitalised everywhere, and it's a count noun** [§ Bin]. Finder writes capital `Bin` in every attestation,
  including running prose, so the overlay does too, and it does NOT mirror base `en`'s own inconsistency between `Trash`
  and `trash`. Because `bin` counts and `trash` doesn't, `doesn't support trash` becomes `doesn't have a Bin`, and the
  article follows Apple: none when the string names the command (`Move to Bin`), one in prose (`moved to the Bin`).
- **`AppKit/Common.json:Trash` is `Delete` in `en-GB` (and `Bin` in `en-AU`), and we ignore it** [§ Bin, the gotcha].
  That key is AppKit's generic destructive button, not the location, and the Touch Bar labels `NSTouchBarTrashTemplate`
  / `NSTrashEmpty` agree with it. Finder's register is the right one for a file manager, so the place stays `Bin` and
  Cmdr's own `menu.file.delete` stays `Delete`. A naive value-grep across the pile argues the opposite; don't follow it.
- **The Oxford comma is a DELIBERATE non-fork** [§ The Oxford comma]. Apple and Microsoft both drop the serial comma
  from noun-phrase lists in British English (32 of 63 such strings in the pile), and Cmdr keeps it anyway: it isn't
  wrong in British English, nobody misreads it, and forking it would freeze roughly 35–50 of our longest, most-edited
  strings in two catalogs to change punctuation nobody notices. Don't "discover" this later and apply it.
- **`go forwards` forks, `Forward` doesn't** [§ The fork test]. The adverbial `-s` rides the VERB phrase, which is where
  Apple forks it (`NSGoForwardTemplate`, `NSTouchBarGoForwardTemplate`). The Go-menu noun stays `Forward` in `en-GB`
  (`Finder/MenuBar.json:249.title`, `SystemSettings/MainMenu.json:448.title`), so `menu.go.forward` is NOT forked while
  `commands.navForward.label` is. `Go back` needs no fork either way.
- **`Show less` stays `Show less`** [§ The fork test]. `en-GB` Finder keeps `Show Less` in all four button attestations
  (`LocalizableMerged.json:GV6`, `:PV2`, `ColumnPreview.json:4jc-Hy-JNJ.alternateTitle`,
  `IconCollectionGroupHeaderView.json:Y6z-b0-II4.title`). The one flip, `show less options` → `show fewer options`
  (`:FI8.1`), is a countable-noun correction in front of a plural noun, not a fork of the bare button.
  `whatsNew.dialog.showLess` is a bare button, so it inherits.

## Verified unchanged

Terms a file manager reaches for that Apple does NOT fork in `en-GB`. Don't invent a difference for these: folder,
directory, disk (storage), disc (optical media), drive, eject, unmount, mount, archive, Zip archive, compress, Get Info,
rename, duplicate, Move To, Copy To, find, search, tag, sidebar, preview, Quick Look, permissions, Full Disk Access,
Storage, Available, Used, Capacity, network, server, share, Connect to Server, Sign In, Settings, Command / Option /
Control / Shift, KB / MB / GB / bytes, Put Back, dialog, Cancel, set up.

## Not attested anywhere

Words with no `en-GB` evidence in the pile at all, so nothing to copy if one enters the catalog later: `defence`,
`offence`, `catalogue`, `analogue`, `analyse`, `travelling`, `labelled`, `programme`, `practise`, `judgement`,
`acknowledgement`, `metre` / `litre` (only `centimetre` is attested), `amongst`, `whilst`, `learnt`, `spelt` (base `en`
says `spelled` in two `errors.listing.*` suggestions; Apple has no `spelt` anywhere in the pile), `artefact`. Reach for
a dictionary and record the call here rather than guessing silently.

## Traps

- **`Finder/LocalizableMerged.json:KIND_FORMATTER_9_1`** is `ZIP` in `en`/`en-GB` and `Postcode` in `en-AU`. It's a
  Spotlight address field, not the archive format. `Finder/CompressWithOptions.json` says `Zip archive` in every locale.
  Don't rename any zip UI from a naive value-grep.
- **`AppKit/Common.json:Trash`** is `Delete` in `en-GB`. That's AppKit's generic destructive button, not the Bin. See
  `style.md` § Bin.
- **`en-GB/xfce-thunar/`, `gnome-nautilus/`, `kde-dolphin/`** call the Trash `Wastebasket` / `Wastebin`. Linux desktops
  are Tier 3 evidence; for a macOS app, Finder wins.
