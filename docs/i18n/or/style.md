# Odia (or) translation style guide

Working notes for translating Cmdr into Odia (Oriya). Read [`README.md`](../README.md) for how this fits the translation
process. Odia is written in its own Odia (Oriya) script; there is no script decision. References: Microsoft terminology
and style guide (`or-IN`, Tier 2) plus GNOME Nautilus (`or`, Tier 3).

## Voice and tone

Cmdr's Odia voice mirrors its English one: friendly, concise, active, and never alarmist. Microsoft's Odia style guide
follows the same modern Microsoft voice as the other Indic guides: warm, relaxed, less formal, crisp, conversational,
everyday vocabulary. That register matches Cmdr.

- Address the user directly; avoid impersonal third-person "user" framing.
- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Prefer everyday words and accepted loanwords over coined Sanskrit-heavy officialese.

## Formality

- **Second person: respectful `ଆପଣ` (āpaṇa).** Odia distinguishes familiar `ତୁମେ` (tume) from respectful `ଆପଣ` (āpaṇa);
  software uses the respectful `ଆପଣ`, which reads as ordinary courtesy. Microsoft and GNOME both use respectful-level
  address. Recommendation: `ଆପଣ` throughout. Confidence: medium-high (consistent across the two sources; no Tier-1 OS).
- **UI actions use the polite imperative**, matching GNOME Odia ("ବାତିଲ କରନ୍ତୁ" for Cancel uses the polite `-ନ୍ତୁ`
  command form). Recommendation: polite `-ନ୍ତୁ` imperatives for buttons and menu items. Confidence: high.

## Decision points

- **Script: Odia only.** No alternative script; nothing to decide. Recommendation: Odia script. Confidence: high.
- **Formality: respectful `ଆପଣ` + `-ନ୍ତୁ` imperatives.** Covered above. Confidence: high.
- **Register: modern, not over-Sanskritized.** As with the other Indic languages, the risk is dense Sanskrit officialese
  that ordinary users don't say. Microsoft's modern voice and GNOME both favor everyday words and accepted loanwords.
  Recommendation: everyday register; use the common loanword (e.g. "ଫୋଲଡର" for folder, as GNOME does) over a coined
  term when that's what users say. Confidence: high.
- **Anglicism handling.** Computing terms are often kept as Odia-spelled loanwords ("ଫୋଲଡର" folder). GNOME does this.
  Recommendation: keep entrenched loanwords in Odia spelling; translate where a native word is common. Confidence: high.
- **Numerals: Western (ASCII) digits.** Odia has its own digits (`୦୧୨୩`), but modern UI commonly uses Western digits;
  `Intl` formats per locale at runtime. Recommendation: rely on `Intl`; don't hand-type Odia digits in copy.
  Confidence: medium. Flag for David if Odia numerals are wanted.
- **Inclusive/gendered language.** Odia has relatively light grammatical-gender load in verb agreement compared to
  Hindi; UI copy via `ଆପଣ` and polite imperatives avoids subject-gender. Prefer neutral phrasing where agreement would
  surface. Confidence: medium.

## Terminology and glossary

Confirmed against GNOME Nautilus Odia (Tier 3) and Microsoft terminology (Tier 2). Extend as strings come up.

| English term | Odia | Notes |
| ------------ | ---- | ----- |
| folder | ଫୋଲଡର | GNOME; loanword |
| trash | ଆବର୍ଜନା ପାତ୍ର | GNOME; the location noun ("refuse vessel") |
| cancel | ବାତିଲ କରନ୍ତୁ | GNOME; polite imperative |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `or`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('or').resolvedOptions().pluralCategories`; GNOME Odia uses `nplurals=2; plural=(n!=1)`). Same
two-category shape as English; every plural message needs both branches. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Script: Odia (Oriya).
- Register: modern everyday Odia, not Sanskritized officialese.
- Numerals: rely on `Intl` (Western digits by default); flag if Odia digits wanted.
- Address: `ଆପଣ` + `-ନ୍ତୁ` polite imperatives throughout.
- No macOS reference (Apple ships no Odia macOS); strongest sources are Microsoft (Tier 2) and GNOME (Tier 3).
- Glossary is thin (GNOME Odia coverage is partial); extend and human-review as strings come up.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/or/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
