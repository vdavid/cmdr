# Faroese (fo) translation style guide

Working notes for translating Cmdr into Faroese (føroyskt). Read [`README.md`](README.md) for how this fits the
translation process.

This is the language base (`fo`), the universal Faroese set. Faroese is the North Germanic language of the Faroe
Islands; a single standard, no region variant needed.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Faroese is a small
North Germanic language; lean on the GNOME catalog's register and keep error and crash copy reassuring and factual.

## Formality

**Use informal direct address ("tú", singular), recommended, high confidence.** Faroese has a T/V-style distinction (tú
vs the polite plural "tit"/"tygum"), but the polite form is archaic/very formal and Scandinavian-software convention is
firmly informal singular (as in Danish, Norwegian, Swedish, Icelandic UI). Address one user with "tú". Confidence: high
(parallels the confirmed informal default across the Nordic languages; GNOME Faroese is informal).

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME catalog: "Avrita" (copy),
"Strika" (delete), "Angra" (cancel).

## Decision points

**This is a thin-coverage locale, and that's the headline finding.** The reference pile has ONLY a GNOME nautilus
catalog (Tier 3) for Faroese, ~85% translated (verified 2026-06-20): no macOS, no Microsoft terminology, no Microsoft
style guide. Apple does not localize macOS into Faroese; Microsoft's Faroese support is minimal. So a Faroese Cmdr
user's OS chrome is in Danish or English. The GNOME file-manager catalog is the one solid anchor. Treat Faroese as a
lower-priority, less-anchored locale. Confidence: confirmed (about the coverage).

- **Script: Latin only, no decision.** Faroese uses the Latin alphabet with ð, æ, ø, å, and accented vowels (á í ó ú ý).
  Keep these; they're meaningful letters. Confidence: high.
- **Lean on Danish/Nordic convention where Faroese sources run out.** Faroese tech vocabulary is thin; where the GNOME
  catalog has no term, the natural fallback for a Faroese reader is the Danish term or an English passthrough, not an
  invented coinage. Danish is widely understood in the Faroes. Confidence: tentative (per-term; a native check is
  ideal).
- **Gender: Faroese has three grammatical genders** (masculine/feminine/neuter), but direct tú-address doesn't gender
  the user. Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: comparable to other Nordic languages** (slightly longer than English, compound nouns can be long).
  Overflow-check tight buttons against the pseudolocale (`en-XA`). Confidence: tentative.

## Terminology and glossary

| English term | Faroese | Notes |
| ------------ | ------- | ----- |
| Copy | Avrita | GNOME ("_Avrita") |
| Delete | Strika | GNOME ("_Strika") |
| Cancel | Angra | GNOME ("A_ngra") |
| folder | Skjátta | GNOME |
| file | (confirm) | GNOME has it in compounds ("fílur"); confirm the bare term |
| trash | Ruskílat | GNOME |
| Move | (confirm) | not in the sampled GNOME entries; check the catalog |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Faroese CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('fo')`, 2026-06-20; GNOME nautilus uses
`nplurals=2; plural=(n != 1)`). Simple two-form system like English. The `desktop-i18n-plural` check enforces coverage.
Confidence: confirmed.

## Notes and decisions

- **Diacritics**: keep ð, æ, ø, å, á, í, ó, ú, ý; all are real Faroese letters.
- **Numbers and dates come from the formatter layer.** Never hardcode separators (Faroese uses comma decimal, like the
  Nordics).
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: [`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Faroese is a thin-coverage locale** (GNOME only, no macOS/Microsoft). Confirm it's worth doing, and how
  much Danish/English fallback is acceptable for gap terms.
