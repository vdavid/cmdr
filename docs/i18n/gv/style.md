# Manx (gv) translation style guide

Working notes for translating Cmdr into Manx (Gaelg). Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`gv`), Manx Gaelic, the Goidelic Celtic language of the Isle of Man (revived from near-
extinction; a small but active speaker community). A single written standard; no region variant needed.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Manx is a revived
minority language with a small but committed localization community; lean on the GNOME catalog's register. Keep error
and crash copy reassuring and factual.

## Formality

**Address the user with the singular "oo" (informal), recommended, high confidence.** Like its Goidelic siblings Irish
and Scottish Gaelic, Manx distinguishes singular "oo" from plural/polite "shiu", and software convention addresses one
user with the singular imperative (the Gaelic-software norm). Confidence: high (parallels Irish/Scottish Gaelic, which
the Microsoft style guides confirm; GNOME Manx is informal).

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME catalog: "Doll magh"
(delete), "Cur ass" (cancel). Manx often uses verbal-noun constructions, as the other Goidelic languages do.

## Decision points

**This is a thin-coverage minority locale, and that's the headline finding (though GNOME coverage is surprisingly
good).** The reference pile has ONLY a GNOME nautilus catalog (Tier 3) for Manx, but it's well-translated (~82%, 1,187
of 1,444 strings, verified 2026-06-20): no macOS, no Microsoft terminology, no Microsoft style guide. Apple and
Microsoft do not localize into Manx, so the user's OS chrome is in English. The single GNOME source is solid for
file-manager terms. Treat Manx as a low-priority, single-anchor locale (its small speaker base is the real priority
signal), but the GNOME catalog makes a decent draft feasible. Confidence: confirmed (about the coverage).

- **Script: Latin only, no decision.** Manx uses the Latin alphabet with an English-influenced orthography (unlike
  Irish/Scottish Gaelic, Manx spelling is based on English conventions, e.g. "ch", "ee", "oo"). No special diacritics
  beyond the occasional ç/ï. Confidence: high.
- **Initial mutation (lenition / eclipsis) is the defining Manx difficulty, as in its Goidelic siblings.** Manx mutates
  word-initial consonants by grammatical context ("cabbyl" → "y chabbyl"). A `{filename}`/`{name}` placeholder after a
  mutating word should mutate, but the catalog can't mutate runtime text. Structure sentences so a placeholder sits
  where no mutation is required, or where leaving it unmutated reads acceptably. Never glue fragments without checking
  the mutation at each join. Confidence: confirmed (Goidelic grammar): the biggest blind-translation risk here.
- **Plurals: THREE CLDR categories.** See Plurals below; flagged because it's a genuine difficulty (more than English's
  two, fewer than Irish's five).
- **Gender: Manx has grammatical gender** (masculine/feminine), which triggers mutation, but direct oo-address doesn't
  gender the user. Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: Manx runs longer than English** (verbal-noun constructions). Overflow-check tight buttons against the
  pseudolocale (`en-XA`). Confidence: tentative.

## Terminology and glossary

| English term | Manx      | Notes                                                   |
| ------------ | --------- | ------------------------------------------------------- |
| Delete       | Doll magh | GNOME                                                   |
| Cancel       | Cur ass   | GNOME                                                   |
| trash        | Trustyr   | GNOME                                                   |
| Copy         | (confirm) | GNOME has it; verify the exact form against the catalog |
| Move         | (confirm) | check the catalog                                       |
| file         | (confirm) | likely "coadan"; verify against GNOME                   |
| folder       | (confirm) | likely "coodagh"; verify against GNOME                  |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Manx CLDR categories: `one`, `two`, `few`, `many`, `other` per CLDR (verified with `new Intl.PluralRules('gv')`,
2026-06-20), but the GNOME nautilus catalog uses a simpler 3-form rule `nplurals=3; plural=n==1?0:(n==2?1:2)`. This is a
genuine source conflict worth noting: CLDR assigns Manx five categories, while the real GNOME catalog phrases counts in
three. Write the branches the `desktop-i18n-plural` check requires for the `gv` CLDR set, and get the noun form right in
each (Manx counted nouns interact with mutation). Confidence: confirmed (categories); flag the CLDR-vs-GNOME mismatch
for the translator.

## Notes and decisions

- **Orthography is English-based**, unlike Irish/Scottish Gaelic; don't import Irish spelling conventions or acute/grave
  accents by reflex.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Manx is a small revived-minority locale** (GNOME only, no macOS/Microsoft, ~1,700 daily speakers). The
  GNOME anchor makes a draft feasible, but confirm it's worth doing before higher-reach languages.
- **Plurals: CLDR (five) vs GNOME (three) disagree** for Manx; the check enforces the CLDR set. Worth a native eye.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/gv/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
