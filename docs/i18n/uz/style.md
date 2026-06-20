# Uzbek (uz) translation style guide

Working notes for translating Cmdr into Uzbek. Read [`README.md`](../README.md) for how this fits the translation
process. Uzbek has a real SCRIPT decision: Latin vs Cyrillic (see Decision points). This `uz` base targets **Latin**; a
Cyrillic build would be a separate `uz-Cyrl` locale. References: a Microsoft "Uzbek (Latin) Style Guide", Microsoft
terminology for both scripts (`uz-Latn`, `uz-Cyrl`), and GNOME Nautilus (`uz`, in Latin).

## Voice and tone

Cmdr's Uzbek voice mirrors its English one: friendly, concise, active, and never alarmist. The free-software and
Microsoft references are plain and direct, which suits Cmdr's register.

- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Drop English filler ("successfully"); Uzbek states the outcome without it.
- Uzbek often drops a possessive pronoun where a possessive affix on the noun already carries it (Microsoft's guide
  notes this); prefer the natural affixed form over a literal pronoun-heavy rendering.

## Formality

- **Second person: respectful `Siz` (plural-polite "you").** Uzbek distinguishes familiar `sen` from respectful `siz`;
  software and formal address use `siz`, which reads as ordinary courtesy. Microsoft and GNOME use `siz`-level address.
  Recommendation: `siz` throughout. Confidence: high.
- **UI actions use the imperative**, matching GNOME Uzbek (Latin): "Oching" (open), "Bekor qilish" (cancel), "Nusxalash"
  (copy). Recommendation: imperative for buttons and menu items. Confidence: high.

## Decision points

- **Script: Latin vs Cyrillic. RESOLVED to Latin (`uz`).** Uzbek officially transitioned from Cyrillic to a Latin
  alphabet (the transition has run for decades and is still ongoing in practice). Both scripts are in real use: Latin is
  the official, education, and younger-generation script and the modern software default; Cyrillic remains widespread
  among older users and in some media. Microsoft's primary style guide is the "Uzbek (Latin) Style Guide", and GNOME
  ships Latin ("Nusxalash", "Katalog", "Bekor qilish"). Both vendors keep a Cyrillic variant too. Ship `uz` as Latin;
  add `uz-Cyrl` only on real demand. Recorded in [`script-decisions.md`](../script-decisions.md).
- **Formality: respectful `siz` + imperatives.** Covered above. Confidence: high.
- **Latin orthography detail.** Modern Uzbek Latin uses the letters `oʻ` (o with turned comma) and `gʻ` (g with turned
  comma), and the apostrophe-like `ʼ` (modifier letter), NOT plain ASCII apostrophe `'`. The GNOME catalog uses a
  straight `'` ("O'chirish", "Qo'yish") as a practical substitute, but the correct Unicode is `ʻ`/`ʼ` (U+02BB / U+02BC).
  Recommendation: use the correct Unicode turned-comma letters; note this interacts with ICU apostrophe escaping (a
  literal `'` in a value must be doubled to `''`, but `ʻ`/`ʼ` are normal letters and are NOT doubled). Confidence: high.
  Flag for David only if a font-rendering issue with `ʻ`/`ʼ` shows up in the app.
- **Anglicism handling.** Computing terms are often kept as Uzbek-spelled loanwords or Russian-derived terms ("Katalog"
  for folder in GNOME). Recommendation: prefer the native/established term the references use; keep entrenched
  loanwords. Confidence: medium.
- **Inclusive/gendered language.** Uzbek has NO grammatical gender and no gendered third-person pronoun (`u` =
  he/she/it). Generic UI copy is naturally gender-neutral. No special handling needed. Confidence: high.

## Terminology and glossary

Confirmed against GNOME Nautilus Uzbek (Latin, Tier 3) and Microsoft (Tier 2). Latin forms; correct Unicode would use
`ʻ`/`ʼ` where GNOME shows a straight `'`. Extend as strings come up.

| English term | Uzbek (Latin)       | Notes                                                                   |
| ------------ | ------------------- | ----------------------------------------------------------------------- |
| folder       | Katalog             | GNOME (Russian-derived); confirm vs "jild"                              |
| copy         | Nusxalash           | GNOME                                                                   |
| open         | Oching              | GNOME; imperative                                                       |
| cancel       | Bekor qilish        | GNOME                                                                   |
| rename       | Nomini oʻzgartirish | GNOME ("Nomini o'zgartirish"); use `ʻ`                                  |
| paste        | Qoʻyish             | GNOME ("Qo'yish"); use `ʻ`                                              |
| trash        | Savat / Oʻchirish   | GNOME shows "O'chirish" (=delete); "Savat" is the better trash-bin noun |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `uz`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('uz').resolvedOptions().pluralCategories`; GNOME Uzbek uses `nplurals=2; plural=(n != 1)`). Same
two-category shape as English; every plural message needs both branches. Uzbek often uses the singular noun form after a
numeral (no plural suffix when counted), so write each branch as a full natural phrase. The `desktop-i18n-plural` check
requires both.

## Notes and decisions

- Script: RESOLVED to Latin (`uz`); Cyrillic would be a separate `uz-Cyrl` build. See
  [`script-decisions.md`](../script-decisions.md).
- Latin letters: use Unicode `oʻ`/`gʻ` (U+02BB) and modifier `ʼ` (U+02BC), not ASCII `'`. These are letters, NOT ICU
  apostrophes, so do NOT double them; a genuine literal `'` still doubles to `''`.
- Gender: Uzbek is genderless; copy is naturally neutral.
- Address: `siz` + imperatives throughout.
- No macOS reference (Apple ships no Uzbek macOS); strongest sources are Microsoft (Tier 2, both scripts) and GNOME
  (Tier 3, Latin).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/uz/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
