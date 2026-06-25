# Abkhaz (ab) translation style guide

Working notes for translating Cmdr into this language. Read [`README.md`](../README.md) for how this fits the
translation process.

## Voice and tone

Friendly, concise, active, calm. Match Cmdr's English register. No established software-UI register exists for Abkhaz
(see Decision points), so the translator effectively sets the house style; keep it plain and modern.

## Formality

Abkhaz has a T/V-style distinction (familiar vs. respectful second person, the respectful form being morphologically
marked on the verb). With no UI precedent to anchor on, default to the respectful/polite form for app chrome, matching
how Apple and Microsoft pick the polite register for most major locales. Low confidence; flag for David.

## Terminology and glossary

| English term | This language | Notes |
| ------------ | ------------- | ----- |

(Fill as terms come up. Expect to coin most file-manager terms from scratch; there's no inherited Abkhaz computing
lexicon, so prefer descriptive native coinages over Russian loans where a clear native word exists, and record each
choice here so it stays consistent.)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Run `new Intl.PluralRules('ab').resolvedOptions().pluralCategories` and cover what it reports. CLDR data for Abkhaz is
thin; verify the categories the runtime actually returns before writing plural branches.

## Decision points

These are the calls that actually matter for Abkhaz; settle them before a translation pass.

- **Viability / priority (the headline finding).** Abkhaz is a very-low-resource language for software localization.
  GNOME lists it but there's essentially no Apple, Microsoft, Google, Spotify, or Netflix product localized into Abkhaz,
  and the reference pile has only `gnome-nautilus` (no macOS, no Microsoft style guide or terminology). Practical
  effect: there's no major-vendor precedent to copy, almost no inherited computing vocabulary, and a tiny user base.
  Recommendation: treat as low priority; only translate if there's a specific reason. Confidence: high.
- **Script: Cyrillic.** Abkhaz is written in an extended Cyrillic alphabet (with many additional letters for its large
  consonant inventory). Left-to-right, no RTL concerns. Use Cyrillic; no Latin variant is in real use. Confidence: high.
- **Loan-word policy.** Because there's no native computing lexicon, the real translation work is deciding per term
  whether to coin a native Abkhaz word or borrow (most often from Russian). Be consistent and record every call in the
  glossary. Flag for David that this is judgment-heavy and best done with a native reviewer. Confidence: medium that
  this is the main effort; the per-term calls are low confidence.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ab/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
