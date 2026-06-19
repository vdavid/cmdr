# Dari (prs) translation style guide

Working notes for translating Cmdr into Dari (Afghan Persian). Read [`README.md`](README.md) for how this fits the
translation process. Dari is written in the Perso-Arabic script and is **right-to-left (RTL)**, which is the dominant
concern for this locale (see Decision points). Dari is the Afghan variety of Persian; it is close to Iranian Persian
([`fa`](fa-style.md)) but has its own lexical preferences. References: Microsoft terminology (`prs-AF`, Tier 2); no
GNOME/Xfce catalog and no macOS.

## Voice and tone

Cmdr's Dari voice mirrors its English one: friendly, concise, active, and never alarmist. Persian-family UI copy is
naturally polite; keep it warm without becoming florid.

- Stay calm and actionable in error messages; keep the English rule of avoiding "error" and "failed".
- Drop English filler ("successfully"); Persian states the outcome without it.
- Cross-reference Iranian Persian (`fa`) for terminology, but prefer Dari/Afghan usage where it differs (e.g. Dari
  often keeps `پرونده` for file and some terms diverge from Tehran usage). Don't ship Iranian Persian verbatim as Dari.

## Formality

- **Second person: respectful `شما` (shomā).** Persian distinguishes familiar `تو` (to) from respectful `شما` (shomā);
  software universally uses `شما`, which reads as ordinary courtesy. Microsoft uses `شما`-level address. Recommendation:
  `شما` throughout. Confidence: high.
- **UI actions use the polite imperative or verbal-noun form**, matching Microsoft's Dari terminology (which lists
  verbs like `باز کردن` "to open", `حذف کردن` "to delete", `لغو` "cancel"). Persian UI commonly uses the infinitive/
  verbal-noun (`کردن`-form) for actions rather than a bare imperative. Recommendation: follow Microsoft's
  verbal-noun/infinitive style for buttons (e.g. `باز کردن`, `کپی کردن`, `لغو`); be consistent. Confidence: medium-high.

## Decision points

- **RTL is the dominant concern.** Dari runs right-to-left. The app must handle RTL layout (mirrored panes, icon and
  caret direction, alignment) exactly as for Arabic ([`ar`](ar-style.md)) and Persian ([`fa`](fa-style.md)). Numbers
  and Latin tokens (paths, brand names) stay LTR inside the RTL flow, which needs correct bidi handling so a path or
  `Cmdr` doesn't visually scramble. Recommendation: treat `prs` as a full RTL locale; verify with the pseudolocale and a
  real RTL pass. Confidence: high. Flag for David: if `ar`/`fa` RTL support isn't yet in the app, Dari can't ship until
  it is; this is shared RTL infrastructure, not a per-language string job.
- **Dari vs Iranian Persian (`fa`).** Same script and grammar, but lexical and stylistic differences (some computing
  terms, some everyday words). Microsoft ships Dari (`prs`) and Persian (`fa`) as separate locales. Recommendation:
  translate Dari fresh using `fa` as a cross-reference, not a source of truth; prefer Afghan usage where it differs.
  Confidence: high.
- **Formality: respectful `شما` + verbal-noun imperatives.** Covered above. Confidence: high.
- **Numerals: Eastern Arabic-Indic vs Western digits.** Dari traditionally uses Eastern Arabic-Indic digits
  (`۰۱۲۳۴`); Western digits also appear. `Intl` formats numbers per locale at runtime (and will pick the locale's
  digit set). Recommendation: rely on `Intl`; don't hand-type digits in copy. Confidence: medium. Flag for David if a
  specific digit set is wanted.
- **Anglicism handling.** Persian generally translates computing vocabulary (file = `پرونده`/`فایل`, folder = `دوسیه`/
  `پوشه`). Microsoft Dari uses `دوسیه` (folder) and `پرونده` (file). Recommendation: follow Microsoft Dari terms; keep
  brand/platform names verbatim (they sit LTR in the RTL flow). Confidence: medium-high.
- **Inclusive/gendered language.** Persian has NO grammatical gender and a single third-person pronoun (`او` =
  he/she); generic UI copy is naturally gender-neutral. No special handling. Confidence: high.

## Terminology and glossary

From Microsoft terminology Dari (Tier 2). Cross-reference `fa` but prefer these Afghan forms. Extend as strings come up.

| English term | Dari | Notes |
| ------------ | ---- | ----- |
| folder | دوسیه | Microsoft Dari (Iranian `fa` often uses پوشه) |
| file | پرونده | Microsoft Dari |
| copy | رونویسی کردن | Microsoft Dari (also کپی کردن) |
| open | باز کردن | Microsoft Dari |
| delete | حذف کردن | Microsoft Dari |
| cancel | لغو | Microsoft Dari |
| move | انتقال | Microsoft listed حرکت; انتقال is the better "move file" sense |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. They render LTR inside RTL text, so bidi handling must keep them intact. The same list (plus the
system placeholder tokens) is enforced by the `desktop-i18n-dont-translate` check; see the curated list in
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `prs`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('prs').resolvedOptions().pluralCategories`). Same two-category shape as English. Persian commonly
uses the singular noun form after a numeral (the counted noun isn't pluralized), so write each branch as a full natural
phrase rather than swapping only the numeral. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- Direction: RTL. Shares RTL infrastructure with `ar`/`fa`; Dari can't ship until that exists. Flag to David.
- Bidi: numbers, paths, and brand/platform names stay LTR within RTL text; verify they don't scramble.
- Numerals: rely on `Intl` for digit set; flag if a specific set is wanted.
- Gender: Persian is genderless; copy is naturally neutral.
- Translate Dari fresh; use Iranian Persian (`fa`) as a cross-reference only.
- Strongest source is Microsoft (Tier 2); no GNOME catalog and no macOS for Dari.
