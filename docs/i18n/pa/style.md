# Punjabi (pa) translation style guide

Working notes for translating Cmdr into Punjabi. Read [`README.md`](../README.md) for how this fits the translation
process. Punjabi has a real SCRIPT decision (see Decision points): Gurmukhi (India side) vs Shahmukhi/Perso-Arabic
(Pakistan side). This `pa` base targets **Gurmukhi**; a Perso-Arabic build would be a separate `pa-Arab` locale.
References: Microsoft terminology for both scripts (`pa-Guru`, `pa-Arab`), a Microsoft style guide, and GNOME Nautilus
(`pa`, in Gurmukhi).

## Voice and tone

Cmdr's Punjabi voice mirrors its English one: friendly, concise, active, and never alarmist. Microsoft's Punjabi style
guide follows the modern Microsoft voice: warm, relaxed, less formal, crisp, conversational, everyday vocabulary. That
register matches Cmdr.

- Address the user directly; avoid impersonal third-person "user" framing.
- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Prefer everyday Punjabi and accepted loanwords over coined officialese.

## Formality

- **Second person: respectful `ਤੁਸੀਂ` (tusī̃).** Punjabi distinguishes familiar `ਤੂੰ` (tū̃) from respectful `ਤੁਸੀਂ`
  (tusī̃); software uses respectful `ਤੁਸੀਂ`, which reads as ordinary courtesy. GNOME and Microsoft both use
  respectful-level address. Recommendation: `ਤੁਸੀਂ` throughout. Confidence: high.
- **UI actions use the polite imperative**, matching GNOME Punjabi ("ਕਾਪੀ ਕਰੋ" copy, "ਖੋਲ੍ਹੋ" open, "ਰੱਦ ਕਰੋ" cancel all
  use the `-ੋ` polite command form). Recommendation: polite imperatives for buttons and menu items. Confidence: high.

## Decision points

- **Script: Gurmukhi vs Shahmukhi (Perso-Arabic). RESOLVED to Gurmukhi (`pa`).** Punjabi is written in Gurmukhi in
  Indian Punjab (left-to-right, Brahmic) and in Shahmukhi (a Perso-Arabic script, right-to-left) in Pakistani Punjab.
  They are mutually unreadable in print. Microsoft ships terminology for BOTH (`pa-IN` Gurmukhi and a Perso-Arabic
  variant); GNOME ships Gurmukhi. Gurmukhi has by far the larger digital footprint, standardized Unicode usage, and
  free-software coverage. Ship `pa` as Gurmukhi (LTR). Shahmukhi `pa-Arab` is Perso-Arabic RTL and out of scope under
  the no-RTL decision (it would need full bidi handling like Arabic). Recorded in
  [`script-decisions.md`](../script-decisions.md).
- **Formality: respectful `ਤੁਸੀਂ` + polite imperatives.** Covered above. Confidence: high.
- **Register: modern, not over-Sanskritized/Persianized.** Keep everyday Punjabi; avoid heavy Sanskrit (Gurmukhi side)
  or heavy Persian/Urdu (Shahmukhi side) officialese. Microsoft modern voice and GNOME both favor everyday words.
  Recommendation: everyday register. Confidence: high.
- **Anglicism handling.** Computing terms are often kept as Gurmukhi-spelled loanwords ("ਫੋਲਡਰ" folder). GNOME does
  this. Recommendation: keep entrenched loanwords; translate where a native word is common ("ਕਾਪੀ ਕਰੋ" copy, "ਰੱਦੀ"
  trash). Confidence: high.
- **Numerals.** Gurmukhi has its own digits (`੦੧੨੩`) but modern UI commonly uses Western digits; `Intl` formats per
  locale at runtime. Recommendation: rely on `Intl`. Confidence: medium.
- **Inclusive/gendered language.** Punjabi verbs and adjectives carry gender agreement. UI copy via `ਤੁਸੀਂ` and polite
  imperatives avoids subject-gender in most strings; prefer neutral phrasing where agreement would surface. Confidence:
  medium.

## Terminology and glossary

Confirmed against GNOME Nautilus Punjabi (Gurmukhi, Tier 3) and Microsoft terminology (Tier 2). Gurmukhi forms. Extend
as strings come up.

| English term | Punjabi (Gurmukhi) | Notes                    |
| ------------ | ------------------ | ------------------------ |
| folder       | ਫੋਲਡਰ              | GNOME; loanword          |
| copy         | ਕਾਪੀ ਕਰੋ           | GNOME; polite imperative |
| trash        | ਰੱਦੀ               | GNOME; the location noun |
| rename       | ਨਾਂ ਨੂੰ ਬਦਲੋ       | GNOME                    |
| paste        | ਚੇਪੋ               | GNOME                    |
| open         | ਖੋਲ੍ਹੋ             | GNOME                    |
| cancel       | ਰੱਦ ਕਰੋ            | GNOME                    |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `pa`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('pa').resolvedOptions().pluralCategories`; GNOME Punjabi uses `nplurals=2; plural=n != 1`). Same
two-category shape as English; every plural message needs both branches. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Script: RESOLVED to Gurmukhi (`pa`, LTR); Shahmukhi/Perso-Arabic `pa-Arab` (RTL) is out of scope under the no-RTL
  decision. See [`script-decisions.md`](../script-decisions.md).
- Register: modern everyday Punjabi, not officialese.
- Numerals: rely on `Intl` (Western digits by default).
- Address: `ਤੁਸੀਂ` + polite imperatives throughout.
- No macOS reference (Apple ships no Punjabi macOS); strongest sources are Microsoft (Tier 2, both scripts) and GNOME
  (Tier 3, Gurmukhi).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/pa/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
