# Cherokee (chr) translation style guide

Working notes for translating Cmdr into Cherokee (ᏣᎳᎩ, Tsalagi). Read `../README.md` for how this fits the translation
process.

`chr` is the language base, written in the Cherokee syllabary (the script-tagged form in the reference pile is
`chr-Cher`). The pile has Microsoft terminology and the Microsoft style guide for `chr-Cher`; no macOS UI strings,
though Apple does ship a Cherokee font and keyboard. Cherokee is notable among low-resource locales for having real
commercial localization (Microsoft Windows LIP, Office, Gmail).

## Voice and tone

Friendly, concise, active, calm, never alarmist. The Microsoft Cherokee style guide steers toward "warm and relaxed",
"less formal, more grounded", "crisp and clear", and everyday words over technical ones (verified in
`chr-Cher/microsoft-style-guides/`, 2026-06-20), a register that fits Cmdr's voice well. Keep error and crash copy
reassuring and factual; never use the bare labels "error"/"failed".

## Formality

**Use a respectful, plain register, recommended, with native review.** The Microsoft Cherokee style guide's "warm and
relaxed, less formal" voice applies. Cherokee does not have a Romance/Slavic-style T/V pronoun split; the register
concern is tone (respectful and natural) rather than a formal-vs-informal pronoun choice. Confidence: high that the
Microsoft "warm, plain" register is right; a native Cherokee reviewer (the Cherokee Nation Language Department is the
authority) confirms the verb morphology.

**Imperatives for UI actions**: follow the Microsoft Cherokee terminology and style guide conventions for actions;
Cherokee verbs are highly polysynthetic, so a native reviewer handles the action forms.

## Decision points

The defining fact is the syllabary script. Cherokee has unusually good commercial precedent for a small language.

- **Script: Cherokee syllabary (`chr` base), settled.** Cherokee is written in Sequoyah's syllabary (85 characters,
  Unicode-encoded), not Latin. Microsoft localizes Cherokee in the syllabary (the MS terminology is syllabary, e.g.
  "folder" → "ᏗᏴᏈᏛᎥᏍᎩ", verified 2026-06-20); Apple, Microsoft, and Google all ship Cherokee syllabary fonts and
  keyboards (verified via web research, 2026-06-20). Recommendation: syllabary. Confidence: confirmed.
- **Commercial precedent exists and is the anchor.** Microsoft shipped a Cherokee Windows Language Interface Pack
  (2012), Office (2015), and Google added Cherokee to Gmail. This is the only Native North American language Microsoft
  localized its OS into. So unlike most low-resource locales, there's a real product-UI corpus and a coordinating
  authority (the Cherokee Nation). Treat the Microsoft Cherokee terminology as the primary term source. Confidence:
  confirmed.
- **No grammatical gender.** Cherokee does not grammatically gender nouns or the addressee, so gender-agreement traps
  don't apply. Confidence: high.
- **Regional/dialect variant: none worth splitting.** Cherokee has dialects (Eastern/Giduwa vs Western/Otali), and the
  Western dialect underlies most standardized written Cherokee. No product-level split; target the standard written form
  the Cherokee Nation uses. Confidence: high.
- **Syllabary rendering.** Verify the app renders the full Cherokee syllabary block correctly with a font that has
  complete coverage (some fonts miss characters), before shipping. Confidence: high; a rendering check.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Primary source for `chr`: Microsoft terminology (Tier 2,
syllabary), confirmed by the Cherokee Nation Language Department on native review.

- folder → ᏗᏴᏈᏛᎥᏍᎩ · MS terminology ("folder" → "ᏗᏴᏈᏛᎥᏍᎩ") · high
- (populate file, copy, delete, search, settings, trash, etc. from `chr-Cher/microsoft-terminology/CHEROKEE.tbx` during
  translation)

| English term | Cherokee | Notes                                                                                        |
| ------------ | -------- | -------------------------------------------------------------------------------------------- |
| folder       | ᏗᏴᏈᏛᎥᏍᎩ  | MS terminology; high                                                                         |
|              |          | populate the rest from `chr-Cher/microsoft-terminology/`, confirm via Cherokee Nation review |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`. Latin
brand words sit inside syllabary runs; verify they render cleanly.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('chr')`, 2026-06-20). Two branches. Cherokee's verb
morphology encodes number richly; a native reviewer confirms how counted strings agree. Write both branches.

## Notes and decisions

- **Numbers and dates come from the formatter layer.** Cherokee uses Western digits in modern usage; let the formatter
  decide.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Priority: low-resource by audience size, but with strong commercial precedent** (Microsoft Windows/Office, Google
  Gmail) and a coordinating authority (Cherokee Nation Language Department). Better-positioned than most small locales.
  Confirm whether it's worth attempting this round.
- **Native review by the Cherokee Nation Language Department** is the right path for confirming syllabary terms and verb
  forms; flag that this locale has an obvious, organized review authority.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/chr/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
