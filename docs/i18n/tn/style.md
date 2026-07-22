# Tswana (tn) translation style guide

Working notes for translating Cmdr into Setswana (Tswana). Read `../README.md` for how this fits the translation
process, and `docs/guides/i18n-translation.md` for the translator workflow and the ICU mechanics.

## Priority signal (read first)

Tswana localization is very sparse. The reference pile has ONE source: the Microsoft terminology glossary
(`_ignored/i18n/tn-ZA/microsoft-terminology/SETSWANA.tbx`, 9,139 entries, all tagged `ZAF`). There is NO Apple/macOS
localization (Apple does not ship Tswana), NO GNOME Nautilus, and NO Xfce Thunar catalog. So the usual triangulation
(macOS > Microsoft > GNOME) collapses to "Microsoft, or our own judgment". That absence IS a finding: outside Microsoft,
major-product software localization into Tswana is essentially nonexistent, so most terms here will be `tentative` and
need native review before shipping. Treat Tswana as a low-priority language and do not ship any of it without a native
Setswana reviewer.

## Voice and tone

Cmdr's English voice is friendly, concise, active, and never alarmist (error messages stay calm and actionable, and
avoid the words "error" and "failed"). Carry the same register into Setswana: plain, warm, direct. Setswana has a strong
oral, proverb-rich register, but UI copy should stay everyday and modern, not literary. Prefer short verb-led phrases
over long noun constructions where the grammar allows.

## Formality

- Setswana has no European-style T/V (tu/vous) distinction. Respect is shown by using the plural second person (`lona`,
  and the class-2 plural verb concord) as an honorific for a single person, the way many Bantu languages do.
- For UI, default to the neutral singular imperative for actions (the bare verb stem is the imperative): `Bula` (open),
  `Boloka` (save), `Phimola` (delete), `Kopolola` (copy). This matches how Microsoft renders command verbs in the
  glossary and reads as a direct instruction to the app, not a person, so it sidesteps the respect question entirely.
- Do not mix singular and plural address within the UI. Stay singular-imperative throughout.
- Capitalize the first letter of a button/label only (sentence case), per Cmdr's global rule.

## Decision points

### Regional variant: target tn-ZA, write portably

- Setswana is official in both Botswana (tn-BW, where it is the majority national language) and South Africa (tn-ZA, one
  of 12 official languages). The base tag `tn` is the right top-level tag; the only reference data we have is South
  African (`ZAF`).
- Differences are minor and orthographic, not lexical: historical missionary-era spelling splits, and a
  central/southern-dialect tendency to write `oo` where standard orthography uses `eo`. Standard Setswana (set by the
  Setswana Language Board) blends the eight dialects and is broadly shared across the border. There is no script
  question: Setswana is written in Latin script in both countries.
- Recommendation: translate under the base tag `tn` using standard orthography, leaning on the Microsoft `ZAF` glossary
  as the lexical anchor. Avoid spellings that are distinctively one-dialect. Only split into `tn-BW` / `tn-ZA` later if
  a Botswana reviewer flags concrete divergences. Confidence: high (that minor base-tag-with-standard-orthography is the
  right call); the specific term spellings stay tentative pending review.

### Noun-class system: the central grammatical feature

- Setswana is a Bantu language with roughly 18 noun classes, each with its own prefix. Classes are NOT genders in the
  European sense (not masculine/feminine); they group nouns by semantic/morphological class. State plainly in any
  gender-related copy decision: Setswana is genderless in the Romance/Germanic sense.
- The class system drives agreement (concord): verbs, adjectives, demonstratives, possessives, and numerals all take a
  concordial prefix that agrees with the noun's class. This means a counted phrase is not "number + invariant noun"; the
  surrounding words inflect to match the noun being counted.
- Pluralization is by prefix change, not a suffix. The plural form lives in a paired class (commonly the `di-` plural):
  - `faele` (file) -> `difaele` (files). Microsoft confirms both (`Faele`, `Difaele`).
  - `sekolo` (school) -> `dikolo`; `selwana` (item) -> plural by class change.
- Practical impact on the catalog: a count message cannot reuse one fixed noun across `one` and `other` branches and
  only swap the numeral. Each plural branch must carry the correctly-classed noun form (singular noun in `one`, plural
  noun in `other`) AND any agreeing words. Write the full noun (and its agreement) inside each ICU branch rather than
  factoring it out. The translator owns getting the concord right per branch.
- Confidence: high on the grammar; the per-string concord is exactly where native review matters most.

### Terminology: loanwords vs native coinages (mixed, per Microsoft)

- The Microsoft glossary mixes English/Afrikaans-derived loanwords (phonetically respelled) with native coinages. Both
  are normal and expected; do not force everything one way.
- Loanword examples (use these as the convention for hardware/abstract tech nouns): `faele` (file), `fensetere`
  (window), `disike` (disk), `khomphiutha` (computer), `thebe` (tab).
- Native-coinage examples (preferred where a clear native term exists): `setsholadifaele` (folder, literally a
  holder-of-files), `mafaratlhatlhaseloago` (network), `Moteme wa matlakala` (Recycle Bin), `Polokelo` (drive, "a place
  of keeping"), `Boloka` (save), `Bula` (open), `Phimola` (delete), `Kopolola` (copy), `sutisa` (move), `tswala`
  (close).
- Recommendation: follow Microsoft's choice per term when it has one (anchor of last resort, but the only anchor). For a
  term Microsoft lacks (e.g. "pane", "listing", "trash" as distinct from Recycle Bin), prefer a transparent native
  coinage in the same spirit, and mark it `tentative` for review. Keep brand/protocol words verbatim per the
  do-not-translate list.

### Gender and inclusive language

- Not applicable in the European sense: no grammatical gender, no he/she split to resolve. Noun classes are not genders.
  No special inclusive-language handling is needed beyond writing naturally. (Recorded so a future translator does not
  go looking for a gender problem that does not exist here.)

### Number and date formatting

- Follow en-ZA conventions for the South African anchor (the only data we have): the OS/locale formatter handles
  separators and date order; do not hand-format numbers or dates in strings. Cmdr formats counts and dates through the
  runtime, so a translator should not bake digit grouping or date order into a value.

## Terminology and glossary

Sources below are Microsoft terminology (`tn-ZA/microsoft-terminology/SETSWANA.tbx`) unless noted. All `tentative` until
a native Setswana reviewer signs off (no second source exists to raise confidence to `high`).

| English     | Setswana              | Notes (source · confidence)                                             |
| ----------- | --------------------- | ----------------------------------------------------------------------- |
| file        | faele                 | loanword · MS · tentative                                               |
| files       | difaele               | `di-` plural class · MS · tentative                                     |
| folder      | setsholadifaele       | native coinage (holds files) · MS · tentative                           |
| copy        | Kopolola              | imperative verb · MS · tentative                                        |
| move        | sutisa                | imperative verb · MS · tentative                                        |
| delete      | Phimola               | imperative verb · MS · tentative                                        |
| open        | Bula                  | imperative verb · MS · tentative                                        |
| save        | Boloka                | imperative verb · MS · tentative                                        |
| close       | tswala                | imperative verb · MS · tentative                                        |
| cancel      | tlosa go tlhopha      | "remove the choice" · MS · tentative                                    |
| paste       | kgomaretsa            | imperative verb · MS · tentative                                        |
| search      | pheneno               | noun · MS · tentative                                                   |
| settings    | Thulaganyo            | MS · tentative                                                          |
| window      | fensetere             | loanword · MS · tentative                                               |
| tab         | thebe                 | loanword · MS · tentative                                               |
| drive       | Polokelo              | native ("a place of keeping") · MS · tentative                          |
| disk        | disike                | loanword · MS · tentative                                               |
| computer    | khomphiutha           | loanword · MS · tentative                                               |
| network     | mafaratlhatlhaseloago | native coinage · MS · tentative                                         |
| Recycle Bin | Moteme wa matlakala   | native phrase · MS · tentative                                          |
| item        | selwana               | MS · tentative                                                          |
| name        | leina                 | MS · tentative                                                          |
| help        | thuso                 | MS · tentative                                                          |
| error       | phoso                 | use in concept only; Cmdr copy avoids the word "error" · MS · tentative |
| back        | Morago                | MS · tentative                                                          |
| home        | Gae                   | MS · tentative                                                          |

## Brand and do-not-translate

Keep verbatim (product/platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte,
Quick Look. The full curated list plus the system placeholder tokens is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

- CLDR plural categories for Tswana: `one`, `other` (verified via `new Intl.PluralRules('tn').resolvedOptions()`, Node,
  2026-06-20; same for `tn-ZA`). So every plural ICU message needs exactly two branches.
- BUT the CLDR two-category model understates the real grammar. Setswana pluralizes by noun-class prefix change, and
  counted phrases trigger concord on the noun and any agreeing words. So the `one` and `other` branches are not "same
  noun, different number word": each branch carries the correctly-classed noun form (singular in `one`, the paired
  plural-class form in `other`) and its agreement. Example shape: `one` -> "faele e le 1", `other` -> "difaele tse {n}"
  (the noun and its concord both change, not only the numeral). Write the full, correctly-agreeing phrase inside each
  branch; do not factor the noun out of the plural.
- This is the single highest blind-translation risk for Tswana. Flag any count string where the noun's class is unclear
  from context for native review.

## Decisions to confirm with David

- Whole-language go/no-go: Tswana has no Apple or GNOME reference and only a single (Microsoft, South-Africa-only)
  glossary, so every term is `tentative`. Confirm whether Tswana is in scope at all before investing a full pass, and
  line up a native Setswana reviewer (mandatory before ship under principle 6).
- Base tag `tn` vs region split: recommend shipping under base `tn` with standard orthography; revisit `tn-BW` / `tn-ZA`
  only if a Botswana reviewer flags real divergence.
- Imperative singular as the action register (recommended) vs plural-respect honorific: recommend singular imperative
  throughout; confirm it does not read as curt to a native speaker.

## Notes and decisions

- ICU mechanics (catalog-level, language-independent, easy to miss): double every apostrophe in a value (`'` -> `''`;
  ICU swallows a lone `'` as an escape), and keep every `{placeholder}` and `<tag>` verbatim. Full rules: the
  agent-handoff block in `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Sources for this guide: Microsoft `SETSWANA.tbx` (mined 2026-06-20); web research on Setswana noun classes/concord and
  on tn-BW vs tn-ZA orthography (2026-06-20). No string was copied verbatim from any copyrighted/GPL source; terms here
  are evidence of convention, to be re-expressed in Cmdr's own catalog and reviewed by a human.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/tn/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
