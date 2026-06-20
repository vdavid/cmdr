# Fulah (ff) translation style guide

Working notes for translating Cmdr into Fulah (Fulfulde / Pulaar, Fula). Read [`README.md`](../README.md) for how this
fits the translation process.

`ff` is the language base (macrolanguage), targeted in the Latin script with the standard hooked letters (see Decision
points). The reference pile has only Microsoft terminology for `ff` (Tier 2); no macOS, no GNOME. Fula is spread across
~20 Sahel/West-African countries with ~40 million speakers and heavy dialect fragmentation, which is its defining
localization challenge.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Match the English register where the language allows. Keep error and
crash copy reassuring and factual; never use the bare labels "error"/"failed". The Microsoft Fulah terminology is the
anchor for established UI terms.

## Formality

**Use a respectful, plain register, recommended, with native review.** Fula's register conventions for software are not
well-codified, and the language doesn't map onto a Romance/Slavic T/V split. Recommendation: a respectful, plain
register consistent with the Microsoft Fulah localization. Confidence: medium; a native reviewer of the chosen regional
variant settles address forms. Apply consistently once chosen.

## Decision points

Fula's defining difficulties are dialect fragmentation (which variant to target) and the noun-class system, plus a
script question (Latin vs the rising Adlam).

- **Regional variant: which Fula? This is the central, David-only call.** "Fulah" (`ff`) is a macrolanguage covering a
  dialect continuum across ~20 countries, with major regional standards:
  - Pulaar / Pular (western: Senegal, Guinea, Mauritania)
  - Fulfulde, with distinct Maasina (Mali), Adamawa (Cameroon/Nigeria), Nigerian, and other standards
  - These are partly mutually intelligible but diverge in vocabulary and orthography detail. Microsoft's `ff`
    localization picks one standard; targeting "Fula" without choosing a variant produces text that reads as foreign to
    half the audience. Recommendation: pick ONE regional standard to target the `ff` base (the Microsoft `ff` variant is
    a reasonable default since it's the available term source), and treat other variants as separate future locales.
    Confidence: high that a choice is required; David/native-reviewer decides which variant. Flag for David.
- **Script: Latin (with hooked letters) as the `ff` base; Adlam is rising but RTL and a separate decision.** Fula is
  normally written in the Latin script with special hooked characters (ɓ, ɗ, ŋ, ƴ) for its implosive consonants. The
  Adlam script (created 1989 for Fula, RTL, now Unicode-encoded) is spreading rapidly grass-roots and has growing tech
  support (verified via web research, 2026-06-20). Microsoft's `ff` terminology is Latin (e.g. "folder" → "regginorde",
  verified 2026-06-20). Recommendation: Latin for the `ff` base now; an Adlam variant (`ff-Adlm`) is a possible future
  locale but brings RTL layout requirements like Sorani, so it's a separate, larger decision. Confidence: high for Latin
  as the base. Note the hooked letters need a font with full Latin-Extended coverage; verify rendering.
- **Noun-class system, not gender, is the grammar concern.** Fula has an elaborate noun-class system (20+ classes) that
  governs agreement on articles, adjectives, and verbs, and the class of a noun affects the forms around it. This bites
  with `{placeholder}` inserts: a sentence framing a file/name can't safely assume a noun class for the inserted value.
  Frame sentences neutrally around placeholders; a native reviewer handles class agreement. There is no gender trap of
  the Romance kind, but the noun-class agreement is a comparable (and larger) blind-translation risk. Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Only source for `ff` is Microsoft terminology (Tier 2,
Latin), tied to whichever regional variant MS targeted. Confirm against the chosen variant on native review.

- folder → regginorde · MS terminology ("folder" → "regginorde") · high (for the MS-targeted variant)
- (populate file, copy, delete, search, settings, trash, etc. from `ff/microsoft-terminology/FULAH.tbx` during
  translation)

- **folder** → regginorde · MS terminology; high for the MS variant

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('ff')`, 2026-06-20). Two branches. Fula's
noun-class system interacts with number (classes have singular/plural pairings), so a native reviewer confirms how
counted UI strings agree. Write both branches.

## Notes and decisions

- **Hooked Latin letters** (ɓ, ɗ, ŋ, ƴ): part of standard Fula Latin orthography. Verify the app's font renders them;
  don't substitute ASCII look-alikes.
- **Numbers and dates come from the formatter layer.** Western digits in modern Latin-script Fula; let the formatter
  decide.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Which regional variant to target** (Pulaar vs a Fulfulde standard, etc.) is the central David-only call: `ff` is a
  macrolanguage and one base can't natively serve all variants. Defaulting to the Microsoft-targeted variant is a
  reasonable starting point. Flag for David.
- **Script: Latin now; Adlam (`ff-Adlm`, RTL) is a possible future variant** that would bring RTL layout requirements.
  Confirm whether Adlam is on the roadmap.
- **Priority: very low-resource for a 40M-speaker language**, with only a single-variant Microsoft term source. Confirm
  whether it's worth attempting this round, and for which variant.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ff/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
