# Norwegian Bokmål (nb) translation style guide

Working notes for translating Cmdr into Norwegian Bokmål. Read [`README.md`](../README.md) for how this fits the
translation process. `nb` targets **Bokmål only** (see Decision points); Nynorsk would be a separate `nn` locale.

## Voice and tone

Cmdr's Norwegian voice mirrors its English one: friendly, concise, active, and never alarmist. Norwegian UI copy from
the majors (Apple, Microsoft, GNOME) is naturally plain and direct, so this register is the native default, not a
stretch.

- Address the user informally as **du** (lowercase). This is universal in modern Norwegian software (see Formality).
- Stay calm and actionable in error messages, and keep the English rule of avoiding "error" and "failed". Norwegian has
  no single neutral word for "failed" that isn't either alarmist (`mislyktes`) or clunky; rewrite around what happened
  and what to do, for example "Fant ikke mappen" (Couldn't find the folder) rather than "Operasjonen mislyktes".
- Drop English filler that doesn't carry meaning: don't translate "successfully" (a Norwegian sentence states the
  outcome without it), and avoid `vennligst` ("please") in terse UI actions, where it reads stiff and
  machine-translated.
- Be concise: Norwegian compounds run long (see særskriving), so prefer the shortest natural phrasing.

## Formality

- **Second person: informal `du`, always.** The formal `De`/`Dem` paradigm is effectively dead in modern Norwegian and
  is explicitly not recommended for software. Apple, Microsoft, Google, Spotify, and Netflix all use `du` in their
  Norwegian products and on their Norwegian sites. There is no register decision to make here.
- Verbs don't change between `du` and `De`, so this choice carries no grammatical ripple.
- **UI actions use the imperative**, matching Apple Finder, GNOME, and Microsoft's official Bokmål style guide:
  "Kopier", "Flytt", "Slett", "Åpne", "Lim inn", "Endre navn", "Avbryt". Do NOT use the infinitive ("Kopiere", "Flytte")
  for buttons and menu items. (One major localization house, Proton, uses infinitive as a house style, but it's the
  outlier; the file-manager majors Cmdr competes with are uniformly imperative.)

## Decision points

The genuinely tricky calls, with how the majors handle each, a recommended default, and a confidence level.

- **Bokmål only; Nynorsk is out of scope.** Bokmål is the written form for ~85-90% of Norway and the universal software
  default. Microsoft, Apple, and Google all ship Bokmål as the Norwegian default; Nynorsk (`nn`) ships only as a
  separate extra in a few large products (Windows, Office) and almost never elsewhere. Recommendation: ship `nb`
  (Bokmål), do not attempt Nynorsk. Confidence: high.

- **Formality: informal `du`, no formal register.** Covered above. Every major uses `du`. Recommendation: `du`
  everywhere, lowercase. Confidence: high.

- **Imperative, not infinitive, for buttons and menu items.** Apple Finder ("Kopier", "Flytt", "Vis info", "Endre
  navn"), GNOME Nautilus ("Kopier", "Lim inn", "Klipp ut"), Thunar, and Microsoft's official Bokmål style guide all use
  the imperative for commands. The lone counterexample (Proton's infinitive house style) doesn't match the file-manager
  category. Recommendation: imperative throughout. Confidence: high.

- **Anglicism handling: translate the file-manager vocabulary, keep only entrenched acronyms.** Norwegian has solid,
  universally-used native words for the core domain, and the majors use them, so Cmdr should too: `fil` (file), `mappe`
  (folder), `papirkurv` (trash; Apple, GNOME, and Microsoft all agree), `disk`/`stasjon` (drive), `volum` (volume),
  `fane` (tab), `mappe`/`vindu`/`rute` (window/pane). Keep verbatim only the brand and platform names in the
  do-not-translate list plus established acronyms (SMB, MTP, URL, VPN, DNS). Acronyms take Norwegian gender with a
  hyphen when inflected ("URL-en", "VPN-et"). Recommendation: translate the domain vocabulary; keep only the
  do-not-translate list and standard acronyms. Confidence: high. Flag for David: "pane" has no single dominant Norwegian
  term (`rute` and `panel` both occur); pick one and lock it in the glossary.

- **Særskriving (compound spacing) is the top mechanical risk.** Norwegian writes compounds as ONE word where English
  uses two: "filnavn" (file name), "målmappe" (destination folder), "søkefelt" (search field), "hurtigtast" (keyboard
  shortcut). Splitting them (the English-influenced error "fil navn") is the single most common and most visible
  Norwegian localization mistake, and it can change meaning. Recommendation: compound by default; when a compound gets
  unwieldy, rephrase rather than split. Confidence: high. This deserves a human review pass dedicated to it.

- **Capitalization: sentence case, lighter than English.** Norwegian sentence case is stricter than English: only the
  first word and proper nouns are capitalized. Days, months, and languages are lowercase ("mandag", "januar", "norsk").
  This aligns with Cmdr's existing sentence-case rule but goes further (don't carry over English mid-title capitals).
  Recommendation: sentence case, lowercase days/months/languages. Confidence: high.

- **Special characters and quotation marks.** Bokmål uses `æ`, `ø`, and `å` (and capitals `Æ`, `Ø`, `Å`) as full
  letters, not decorations; never substitute `ae`/`oe`/`aa`. Quotation marks are guillemets «like this», matching Apple
  and Microsoft Norwegian. Numbers use a space (or non-breaking space) as the thousands separator and a comma as the
  decimal mark (1 234,5); `Intl` handles this at runtime, so this matters only for any hand-written numeral in copy.
  Recommendation: native letters, «guillemets», rely on `Intl` for numbers. Confidence: high.

- **Inclusive/gendered language.** Norwegian has grammatical gender on nouns but no he/she issue in generic UI copy (the
  user is addressed as `du`). No special handling needed beyond avoiding gendered role nouns where a neutral one exists.
  Recommendation: no special measures. Confidence: medium (low-stakes; revisit only if a string addresses a person by
  role).

## Terminology and glossary

A few core terms confirmed against Apple Finder, GNOME Nautilus, and Microsoft terminology. Extend as strings come up.

| English term       | Norwegian Bokmål | Notes                                                         |
| ------------------ | ---------------- | ------------------------------------------------------------- |
| file               | fil              |                                                               |
| folder             | mappe            |                                                               |
| copy               | kopier           | imperative                                                    |
| move               | flytt            | imperative                                                    |
| delete             | slett            | imperative                                                    |
| trash              | papirkurv        | the noun (the location); "legg i papirkurven" = move to trash |
| rename             | endre navn       | Apple/GNOME both use this two-word verb phrase                |
| paste              | lim inn          |                                                               |
| cut                | klipp ut         |                                                               |
| open               | åpne             |                                                               |
| cancel             | avbryt           |                                                               |
| tab                | fane             | UI tab, not the key                                           |
| volume             | volum            |                                                               |
| settings           | innstillinger    |                                                               |
| destination folder | målmappe         | one word (særskriving)                                        |
| file name          | filnavn          | one word (særskriving)                                        |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `nb`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('nb').resolvedOptions().pluralCategories`). Same two-category shape as English, so every plural
message needs both branches. Noun gender interacts with the count word and any adjective, so write each branch as a full
natural phrase rather than swapping only the numeral. The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- Quotation marks: guillemets «…».
- Punctuation and capitalization: sentence case; lowercase days, months, and language names.
- Letters: `æ`, `ø`, `å` are full letters; never transliterate to `ae`/`oe`/`aa`.
- Numbers: comma decimal mark, space thousands separator; `Intl` handles formatting at runtime.
- Dedicate one human review pass to særskriving (compound spelling), the highest-frequency Norwegian UI error.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/nb/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
