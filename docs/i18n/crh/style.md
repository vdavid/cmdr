# Crimean Tatar (crh) translation style guide

Working notes for translating Cmdr into Crimean Tatar (Qırımtatar tili). Read [`README.md`](../README.md) for how this
fits the translation process.

`crh` is the language base, targeted in the Latin script (see Decision points). The reference pile has only GNOME
nautilus for `crh` (Tier 3); no macOS, no Microsoft. This is a low-resource locale with a real but small localization
tradition.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Match the English register where the language allows. Keep error and
crash copy reassuring and factual; never use the bare labels "error"/"failed". Lean on the GNOME Crimean Tatar catalog
for established UI phrasing.

## Formality

**Use the polite/standard register, recommended, with native review.** Crimean Tatar is a Turkic language and, like
Turkish, distinguishes an informal `sen` from a polite/plural `siz`. Turkic software UI conventionally addresses the
user with the polite `siz` form. Recommendation: `siz` register. Confidence: high that polite is correct; the exact verb
forms need a native reviewer. Apply consistently.

**Imperatives for UI actions**: use the polite imperative for buttons and menu items, following the GNOME Crimean Tatar
catalog's conventions.

## Decision points

The defining decision is script, and it has recently been settled in Latin's favor by official policy.

- **Script: Latin (`crh` base), now the official standard.** Crimean Tatar has been historically biscriptal (Latin and
  Cyrillic), but:
  - Ukraine began the transition to Latin in 2021, and the Cabinet of Ministers finalized detailed Latin spelling rules
    on 2025-04-04, standardizing Latin for official literature, textbooks, and digital platforms (verified via web
    research, 2026-06-20).
  - Google Translate added a Latin-script Crimean Tatar option, alongside its older Cyrillic transliteration.
  - Cyrillic remains in use in Russian-occupied Crimea, but the forward-looking, Ukraine-aligned standard is Latin.
  - Recommendation: target Latin for the `crh` base. A `crh-Cyrl` variant is not worth shipping for a macOS app.
    Confidence: high. No David call needed; the policy has settled this.
- **Regional/dialect variant: none worth splitting.** Crimean Tatar has dialect variation (Northern/steppe, Central,
  Southern/coastal), and the literary standard blends them. There's no product-level region split. Target the literary
  standard. Confidence: high.
- **No grammatical gender.** Like other Turkic languages, Crimean Tatar has no grammatical gender, so the
  gender-agreement traps of Romance/Slavic languages don't apply. The polite-register choice is the live distinction.
  Confidence: high.
- **Agglutination and vowel harmony.** Crimean Tatar is agglutinative with vowel harmony: case and possessive endings
  attach as suffixes and their vowels harmonize with the stem. As with Basque and Turkish, avoid bolting a fixed case
  suffix directly onto an uncontrolled `{placeholder}` (a path or name whose final sound is unknown at runtime); frame
  the sentence so the placeholder sits in a position that doesn't force a harmonized suffix. Confidence: high;
  translator-craft concern.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Only source for `crh` is GNOME nautilus (Tier 3); pull terms
from there and mark them `tentative` until a native reviewer or a second source confirms.

- Glossary: populate from `crh/gnome-nautilus/nautilus.po`; mark tentative pending native review (populate via the cited
  sources and native review; nothing guessed yet).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. As with
other agglutinative languages, suffixes may attach to brand words in running text; keep the brand stem verbatim.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('crh')`, 2026-06-20). Two branches. As in Turkish,
the noun after a numeral typically stays singular in form, so don't force an English-style plural noun; phrase counted
strings to read correctly with the language's own number-agreement rules.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Length**: agglutinated suffix stacking can lengthen words; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics**: double every apostrophe in ICU values (Crimean Tatar Latin uses the apostrophe-like `'` in some
  orthographies, so this matters); keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: low-resource locale with only a GNOME catalog and no commercial precedent.** Confirm whether it's worth
  attempting this round. The Latin-script and register calls are settled enough to proceed if so, but every term needs
  native review.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/crh/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
