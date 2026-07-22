# Inuktitut (iu) translation style guide

Working notes for translating Cmdr into Inuktitut. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice.

## Voice and tone

Friendly, concise, active, calm. Inuktitut is highly polysynthetic (whole phrases pack into single inflected words), so
"concise" reads differently here: a UI action is often one long word. Error messages stay calm and actionable.

## Formality

**Inuktitut has no European-style T-V formality distinction.** No formal/informal pronoun choice to make. Politeness is
conveyed by mood/affixes, not pronoun selection.

- Buttons and menu items: use the imperative/verbal form as the Microsoft terminology gives it (these are single
  inflected words, e.g. copy → "ijjuarli", open → "matuirli", search → "qinirli", cancel → "qujanaarli").

## Decision points

### Script: Latin vs syllabics (the defining call)

- Inuktitut is written in TWO scripts: Latin (Roman orthography) and Inuktitut syllabics (ᐃᓄᒃᑎᑐᑦ, the Unified Canadian
  Aboriginal Syllabics block). These are not regional accents, they are full alternate writing systems for the same
  language.
- The BCP-47 tags encode the choice: `iu-Latn` (Latin) vs `iu-Cans` (syllabics). The reference pile is `iu-Latn`
  (Microsoft terminology in Latin script).
- Majors: Microsoft localizes Inuktitut and ships terminology in Latin (`iu-Latn`). Syllabics is the more culturally
  prominent script in Nunavut official use, but the available localization data here is Latin.
- Recommendation: target `iu-Latn` (the tag the reference pile uses and the form with authoritative MS data). Adding
  `iu-Cans` syllabics would be a separate, harder effort needing a syllabics-literate reviewer and a font check. Flag
  the script choice to David: it is the single biggest decision for Inuktitut. Confidence: high (for which data exists);
  the Latn-vs-Cans product decision is David's.

### Polysynthesis: actions are single words, placeholders are hard

- Inuktitut builds meaning by affixing onto a root, so "move the file to trash" is not a word-order assembly but one
  inflected verb complex. A `{name}` or `{count}` placeholder cannot be incorporated into the word the way Inuktitut
  grammar would; it has to sit beside it.
- Implication: fragment-key assembly and placeholder insertion are much harder than in analytic languages. Structure
  sentences so placeholders stand apart from the inflected verb; expect the native reviewer to reshape many strings.
- Recommendation: keep placeholders separate from the action word; flag heavily for review. Confidence: high.

### Dual number (the plural surprise)

- Inuktitut grammatically distinguishes singular, DUAL (exactly two), and plural (three or more). This is why its CLDR
  plural set has a `two` category most European languages lack. See Plurals.
- Recommendation: write `one`, `two`, and `other` branches for every count. Confidence: high.

### No grammatical gender

- Inuktitut has no grammatical gender. Inclusive language is a non-issue. Confidence: high.

## Terminology and glossary

Source: Microsoft terminology (`iu-Latn/microsoft-terminology/INUKTITUT (LATIN).tbx`) is the only authoritative source
here (no macOS, no GNOME/Xfce Inuktitut). All terms `tentative` until a native reviewer confirms, given the single
source and the polysynthetic complexity. From the TBX:

- file → ini · MS · tentative
- folder → puurvik · MS · tentative
- directory → tukimuagutiit · MS · tentative
- drive → tuqquumavik · MS · tentative
- delete → nungutirlugu · MS · tentative
- copy → ijjuarli · MS · tentative
- move → nuulli · MS · tentative
- open → matuirli · MS · tentative
- search → qinirli · MS · tentative
- cancel → qujanaarli · MS · tentative
- server → ikiaqqitittivik · MS · tentative
- volume → nipiqquqtusigiarut · MS; check this is not the audio-loudness sense · tentative

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `two`, `other` (verified `new Intl.PluralRules('iu')` and `'iu-Latn'`, 2026-06-20). The `two`
category reflects Inuktitut's grammatical DUAL. Every plural message MUST cover all three; `desktop-i18n-plural` will
flag a missing `two`. Each branch inflects the counted noun for its number.

## Notes and decisions

- **Script integrity:** for `iu-Latn`, preserve the Roman orthography exactly. If `iu-Cans` is ever added, that is a
  separate catalog with the syllabics block.
- **Length:** Inuktitut words are long (polysynthesis). High overflow risk; overflow-check hard against the pseudolocale
  (`en-XA`).
- **Numbers/dates** come from the formatter layer. Never hardcode.

## Decisions to confirm with David

- **Script: `iu-Latn` (Latin) vs `iu-Cans` (syllabics)**, the defining Inuktitut decision. The reference data is Latin;
  syllabics would be a separate effort. David's call.
- **All glossary terms** are tentative (single source, polysynthetic grammar): need a native reviewer.
- **Priority:** very small user base, single thin source, hard grammar. Lowest-tier priority; flag whether to ship at
  all.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/iu/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
