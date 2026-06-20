# Western Frisian (fy) translation style guide

Working notes for translating Cmdr into Western Frisian (Frysk). Read [`README.md`](../README.md) for how this fits the
translation process.

This is the language base (`fy`), Western Frisian, the West Germanic language of Friesland (Fryslân) in the Netherlands.
A single standard orthography (the 2015 spelling); no region variant needed for UI.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Frisian tech
vocabulary is thin and Dutch-adjacent; keep phrasing plain. Keep error and crash copy reassuring and factual.

## Formality

**Use informal direct address ("do", singular), recommended, high confidence.** Western Frisian has a T/V distinction
(informal "do" vs polite "jo"), paralleling Dutch je/u. Dutch and Frisian open-source software convention is informal
direct address ("do" / Dutch "je"). Address one user with "do". Confidence: high (parallels the Dutch-UI informal norm;
GNOME Frisian where present is informal). Flag only if David prefers the polite "jo" for a more reserved register, but
the default is informal.

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME catalog where it has the
term: "Wiskje" (delete). The imperative agrees with the informal address above.

## Decision points

**This is a thin-coverage locale, and sparsely translated, which is the headline finding.** The reference pile has ONLY
a GNOME nautilus catalog (Tier 3) for Frisian, and it's only ~33% translated (475 of 1,444 strings, verified
2026-06-20): no macOS, no Microsoft terminology, no Microsoft style guide. Apple and Microsoft do not localize into
Western Frisian, so the user's OS chrome is in Dutch or English. Even the one GNOME source has large gaps. Treat Frisian
as a low-priority, weakly-anchored locale that leans heavily on Dutch convention. Confidence: confirmed (about the
coverage).

- **Script: Latin only, no decision.** Western Frisian uses the Latin alphabet (with â ê ô û and the diphthong digraphs,
  plus ú/ä in loanwords). Use the standard 2015 orthography. Confidence: high.
- **Lean on Dutch convention for gap terms.** Frisian computing vocabulary is thin and Frisian speakers are bilingual in
  Dutch; where the GNOME catalog has no term, the natural fallback is the Dutch term or an English passthrough, not an
  invented Frisian coinage. Confidence: tentative (per-term; native check ideal).
- **Don't translate from Dutch by reflex where Frisian has its own word.** Frisian is a distinct language, not a Dutch
  dialect ("wiskje" = delete, not Dutch "verwijderen"). Use the Frisian GNOME terms where they exist, Dutch fallback
  only for genuine gaps. Confidence: tentative.
- **Gender: Frisian has grammatical gender** (common/neuter, as in Dutch), but direct do-address doesn't gender the
  user. Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: comparable to Dutch** (slightly longer than English, compound nouns). Overflow-check tight buttons against
  the pseudolocale (`en-XA`). Confidence: tentative.

## Terminology and glossary

| English term | Western Frisian | Notes                                                               |
| ------------ | --------------- | ------------------------------------------------------------------- |
| Delete       | Wiskje          | GNOME ("Wiske"/"Wiskje")                                            |
| trash        | Jiskefet        | GNOME                                                               |
| Copy         | (confirm)       | not translated in the sampled GNOME entries; Dutch "Kopiearje" form |
| Move         | (confirm)       | gap in GNOME; check catalog / Dutch fallback                        |
| Cancel       | (confirm)       | gap in GNOME                                                        |
| file         | (confirm)       | gap; likely "triem" (Frisian) or Dutch "bestand"                    |
| folder       | (confirm)       | gap; check catalog                                                  |

(Many rows are gaps in the sparse GNOME catalog; these are the highest-value terms to settle first, likely via Dutch
reference + native review.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Western Frisian CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('fy')`, 2026-06-20; GNOME nautilus
uses `nplurals=2; plural=(n != 1)`). Simple two-form system like English. The `desktop-i18n-plural` check enforces
coverage. Confidence: confirmed.

## Notes and decisions

- **Diacritics**: keep â ê ô û (and ú/ä/ë in loans); they're meaningful in Frisian spelling.
- **Numbers and dates come from the formatter layer.** Never hardcode separators (Frisian follows Dutch: comma decimal,
  period thousands).
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Frisian is a thin, sparsely-translated locale** (GNOME only at ~33%, no macOS/Microsoft). Confirm it's
  worth doing, and how much Dutch/English fallback is acceptable for the many gap terms.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/fy/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
