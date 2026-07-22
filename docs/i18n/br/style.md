# Breton (br) translation style guide

Working notes for translating Cmdr into Breton (Brezhoneg). Read `../README.md` for how this fits the translation
process.

`br` is the language base, written in the Latin script. The reference pile has only GNOME nautilus for `br` (Tier 3); no
macOS, no Microsoft. Breton is a minority Celtic language of Brittany (France) with an active but small free-software
localization community.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Match the English register where the language allows. Keep error and
crash copy reassuring and factual; never use the bare labels "error"/"failed". Lean on the GNOME Breton catalog for
established UI phrasing; the Breton open-source localization community (e.g. the Drouizig project) is the de facto
terminology authority.

## Formality

**Use the polite `c'hwi` (vous-equivalent), recommended, with native review.** Breton distinguishes informal `te`
(singular intimate) from polite/plural `c'hwi`. Software and formal address conventionally use `c'hwi`. Recommendation:
`c'hwi` register. Confidence: medium-high; a native reviewer confirms, and may note that Breton's revitalization
community sometimes favors a warmer register. Apply consistently.

**Imperatives for UI actions**: use the imperative consistent with the address choice, following the GNOME Breton
catalog's conventions for file-manager actions.

## Decision points

Breton's defining difficulties are its initial-consonant mutations and a plural-category mismatch between CLDR and real
catalogs, not script or variant.

- **Script: Latin only. No decision.** Breton uses the Latin script with some digraphs (notably `c'h`, written with an
  apostrophe). No alternate script. Confidence: confirmed. Note the `c'h` apostrophe is orthographic, not an ICU escape;
  see the apostrophe note under Plurals/Notes.
- **Plural categories: CLDR says five, real catalogs use two. This is the key technical decision.** `Intl.PluralRules`
  reports `one, two, few, many, other` for `br` (verified 2026-06-20), reflecting Breton's genuine grammatical number
  system (it has dual and other distinctions). But the GNOME Breton nautilus catalog declares `nplurals=2; plural=n>1`
  (verified 2026-06-20), i.e. real-world Breton software localization uses a simple singular/plural split. Cmdr's
  `desktop-i18n-plural` check requires covering the CLDR categories the language needs. Recommendation: follow CLDR and
  write the categories ICU asks for, but a native reviewer should confirm which categories actually carry distinct
  Breton forms for counted UI strings (several CLDR branches may collapse to identical text). Flag for David/translator:
  this language is one where the CLDR category set and the practical catalog convention diverge. Confidence: high that
  the divergence exists; native review settles how many branches truly differ.
- **Initial consonant mutations.** Breton mutates the initial consonant of a word based on the preceding word (article,
  possessive, number, etc.). This bites with `{placeholder}` inserts and assembled \*Join fragments: a fixed-form word
  before or after a placeholder may need a mutated form depending on context that isn't known until runtime. Structure
  sentences to avoid forcing a mutation across a placeholder boundary; a native reviewer handles the mutation rules.
  Confidence: high; the subtlest translator-craft concern for Breton.
- **No grammatical gender trap for the address pronoun**, though nouns are gendered (masculine/feminine) and trigger
  mutations. Don't gender the user; the mutation system is the real grammar concern. Confidence: high.
- **Regional variant: none worth splitting.** Breton has dialects (KLT vs Gwenedeg) and competing orthographies
  historically, but modern localization uses the unified `peurunvan` orthography. No product-level split; target
  peurunvan. Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Only source for `br` is GNOME nautilus (Tier 3); the Breton
open-source community (Drouizig) is the practical terminology authority. Mark single-source terms `tentative`.

- trash → Lastez · GNOME nautilus ("Trash" → "Lastez") · high
- (populate copy, delete, search, settings, file, folder, etc. from `br/gnome-nautilus/nautilus.po`)

| English term | Breton | Notes                                                                             |
| ------------ | ------ | --------------------------------------------------------------------------------- |
| trash        | Lastez | GNOME nautilus; high                                                              |
|              |        | populate the rest from `br/gnome-nautilus/`, mark tentative pending native review |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR categories: `one`, `two`, `few`, `many`, `other` (verified with `new Intl.PluralRules('br')`, 2026-06-20), the most
complex plural system in this batch. But note the divergence from real catalogs under Decision points: GNOME Breton uses
only two forms. Cover the CLDR categories the check requires; a native reviewer confirms which branches carry truly
distinct text vs which can repeat the same form.

## Notes and decisions

- **The `c'h` digraph and apostrophe.** Breton's `c'h` contains a literal apostrophe that is part of the orthography. In
  ICU values, ALL apostrophes must be doubled (`'` → `''`) per ICU's escape rule, including the one in `c'h`. This is a
  high-risk Breton-specific gotcha: a single un-doubled `c'h` apostrophe silently swallows following text. Double every
  one.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **ICU mechanics**: keep every `{placeholder}` and `<tag>` verbatim, and double every apostrophe (see above). Full
  rules: `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Priority: minority language with only a GNOME catalog and no commercial precedent.** Confirm whether it's worth
  attempting this round.
- **Plural categories diverge (CLDR five vs catalog two).** A native reviewer settles how many branches truly differ;
  Cmdr's check requires the CLDR set.
- **Initial-consonant mutations and `c'h`-apostrophe doubling** are Breton-specific traps a native reviewer must own.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/br/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
