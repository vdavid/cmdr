# Script-target decisions

The authoritative record of which SCRIPT each digraphic or transliteration-flexible language ships in, mirroring
[`formal-informal-decisions.md`](formal-informal-decisions.md) for register. These calls are David's, not a
translator's: each picks one shipped script as the catalog base, with any second script a later sibling locale.
Reconcile each `<tag>/style.md` script Decision point to this table. Decisions made 2026-06-20.

| id  | language    | script decision                           | notes                                                                                          |
| --- | ----------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------- |
| zh  | Chinese     | Simplified `zh-Hans` only for now         | Traditional `zh-Hant` (Taiwan norm) is a fast-follow; never auto-convert (vocabulary differs). |
| sr  | Serbian     | Latin `sr-Latn` first                     | Cyrillic `sr-Cyrl` an optional fast-follow; the two are 1:1 transliterable.                    |
| be  | Belarusian  | Cyrillic, official наркамаўка orthography | Not classical тарашкевіца; `be-Latn` (Łacinka) out of scope.                                   |
| uz  | Uzbek       | Latin (`uz`)                              | Cyrillic `uz-Cyrl` only on real demand; Latin is the official direction.                       |
| kk  | Kazakh      | Cyrillic (base tag `kk`)                  | `kk-Latn` a later fast-follow as the ~2031 Latin transition lands.                             |
| mn  | Mongolian   | Cyrillic (`mn`)                           | Traditional vertical `mn-Mong` out of scope (vertical layout, complex shaping).                |
| az  | Azerbaijani | Latin (`az-Latn`)                         | Perso-Arabic `az-Arab` (Iran) is RTL, out of scope under the no-RTL decision.                  |
| pa  | Punjabi     | Gurmukhi (`pa`)                           | Shahmukhi `pa-Arab` is Perso-Arabic RTL, out of scope under the no-RTL decision.               |
| bs  | Bosnian     | Latin (`bs`)                              | `bs-Cyrl` only if a Cyrillic audience surfaces; unlikely for a macOS app.                      |
