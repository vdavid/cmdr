# Nepali (ne) translation style guide

Working notes for translating Cmdr into Nepali. Read `../README.md` for how this fits the translation process. Nepali is
written in the Devanagari script; there is no script decision. References: Microsoft terminology and style guide
(`ne-NP`, Tier 2) plus GNOME Nautilus (`ne`, Tier 3).

## Voice and tone

Cmdr's Nepali voice mirrors its English one: friendly, concise, active, and never alarmist. Microsoft's Nepali style
guide explicitly targets a "modern voice": warm, relaxed, less formal, crisp, and conversational, using everyday words
and well-established short forms. That register matches Cmdr exactly.

- Address the user directly with second-person pronouns; Microsoft's guide says to avoid third-person "user" framing as
  formal and impersonal.
- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Use established everyday words and accepted short forms for a friendly, scannable tone; avoid words that are too
  informal or coined.

## Formality

- **Second person: respectful-but-modern `तपाईं` (tapāī̃).** Nepali has a three-way honorific scale (`तँ` low / `तिमी`
  mid / `तपाईं` high-respect). Software universally uses `तपाईं`, the polite-standard form, which reads as ordinary
  courtesy rather than stiff formality. Microsoft, GNOME, and Google all use `तपाईं`-level address. Recommendation:
  `तपाईं` throughout. Confidence: high.
- **UI actions use the polite imperative**, matching GNOME Nepali, which suffixes verbs with `-नुहोस्` ("कपी गर्नुहोस्",
  "खोल्नुहोस्", "रद्द गर्नुहोस्"). This is the standard polite-command form. Recommendation: `-नुहोस्` imperatives for
  buttons and menu items. Confidence: high.

## Decision points

- **Script: Devanagari only.** No alternative script in use for Nepali; nothing to decide. Recommendation: Devanagari.
  Confidence: high.
- **Formality: polite-standard `तपाईं` and `-नुहोस्` imperatives.** Covered above. Confidence: high.
- **Register: modern, not Sanskritized.** The risk for Nepali (as for Hindi) is over-Sanskritized officialese.
  Microsoft's modern-voice guidance and GNOME both favor everyday vocabulary and accepted loanwords over coined Sanskrit
  equivalents. Recommendation: everyday register; prefer a common loanword (e.g. "फोल्डर" for folder, as GNOME does)
  over a coined Sanskrit term when the loanword is what users say. Confidence: high.
- **Anglicism handling.** Common computing terms are routinely kept as Devanagari-spelled loanwords ("फोल्डर" folder,
  "फाइल" file). GNOME and Microsoft both do this. Recommendation: keep entrenched loanwords in Devanagari spelling;
  translate where a native word is genuinely common (e.g. "प्रतिलिपि" for copy). Confidence: high.
- **Numerals: Western (ASCII) digits.** Modern Nepali UI commonly uses Western digits, though Devanagari digits (`०१२३`)
  exist. `Intl` formats numbers per locale at runtime. Recommendation: rely on `Intl`; don't hand-type Devanagari digits
  in copy. Confidence: medium. Flag for David if Devanagari numerals are wanted for a more localized feel.
- **Inclusive/gendered language.** Nepali verbs and adjectives carry gender agreement, but UI copy addressed via `तपाईं`
  and `-नुहोस्` imperatives avoids subject-gender in most strings. Where a string would need agreement, prefer
  gender-neutral phrasing. Confidence: medium.

## Terminology and glossary

Confirmed against GNOME Nautilus Nepali (Tier 3) and Microsoft terminology (Tier 2). Extend as strings come up.

| English term | Nepali                | Notes                        |
| ------------ | --------------------- | ---------------------------- |
| file         | फाइल                  | loanword, Devanagari-spelled |
| folder       | फोल्डर                | GNOME; loanword              |
| copy         | प्रतिलिपि गर्नुहोस्   | GNOME; polite imperative     |
| trash        | रद्दीटोकरी            | GNOME; the location noun     |
| rename       | पुन: नामकरण गर्नुहोस् | GNOME                        |
| paste        | टाँस्नुहोस्           | GNOME                        |
| open         | खोल्नुहोस्            | GNOME                        |
| cancel       | रद्द गर्नुहोस्        | GNOME                        |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR plural categories for `ne`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('ne').resolvedOptions().pluralCategories`; GNOME Nepali uses `nplurals=2; plural=n != 1`). Same
two-category shape as English; every plural message needs both branches. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Script: Devanagari.
- Register: modern everyday Nepali, not Sanskritized officialese.
- Numerals: rely on `Intl` (Western digits by default); flag if Devanagari digits wanted.
- Address: `तपाईं` + `-नुहोस्` polite imperatives throughout.
- No macOS reference (Apple ships no Nepali macOS); strongest sources are Microsoft (Tier 2) and GNOME (Tier 3).

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ne/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
