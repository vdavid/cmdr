# Macedonian (mk) translation style guide

Working notes for translating Cmdr into Macedonian. Read [`README.md`](../README.md) for how this fits the translation
process. Macedonian is written in **Macedonian Cyrillic only**; there is no Latin UI variant to target (see Decision
points).

## Voice and tone

Cmdr's Macedonian voice mirrors its English one: friendly, concise, active, and never alarmist. Macedonian software copy
from the majors (Microsoft, GNOME, Google) is plain and direct, so this register is the native default.

- Address the user with the polite second-person plural (вие / ви / ваш), lowercase. This is the universal software
  register; do NOT use informal singular ти (see Formality).
- Keep verbs in instructional sentences in the polite plural (кликнете, користете, изберете), matching the вие address.
- Stay calm and actionable in error messages, and keep the English rule of avoiding "error" and "failed". Macedonian has
  no neutral one-word "failed"; rewrite around what happened and what to do (for example "Папката не е најдена" = the
  folder wasn't found, rather than a literal "the operation failed"). Microsoft's own Macedonian guidance is to keep
  error messages natural and empathetic, drop the exclamation marks English loves, and never just translate them
  literally.
- Drop English filler that carries no meaning: don't render "successfully" (a Macedonian sentence states the outcome
  without it), and avoid "ве молам" ("please") in terse UI actions, where it reads stiff.
- Macedonian capitalizes very sparingly: sentence case, only the first word and proper nouns. Days, months, and language
  names are lowercase. This aligns with Cmdr's sentence-case rule and goes a bit further.

## Formality

- **Second person: polite plural вие, always; never informal ти.** This is the single most important register decision
  and it differs from the Germanic locales (which use informal singular). Microsoft's official Macedonian style guide
  addresses the user with the second-person plural (вие/ви/ваш), and GNOME Nautilus does the same ("Дали сте сигурни
  дека сакате..."). Informal ти would read as too casual for a software product. Confidence: high.
- **Lowercase the pronouns.** вие, ви, ваш, вашиот are NOT capitalized in software addressed to users in general
  (Microsoft capitalizes them only in personal letters to a named individual). Confidence: high.
- **Prefer neutral structures over spelling out the pronoun.** Macedonian uses "you/your" far less than English. Where
  English says "save your work", Macedonian drops the possessive or uses the reflexive/article ("зачувајте ја
  работата"). Don't transcribe every English "your". Confidence: high.
- **UI commands, buttons, and menu items use the imperative singular.** Microsoft's menu commands and GNOME Nautilus
  both use the short imperative for actions: Отвори (Open), Зачувај (Save), Копирај (Copy), Премести (Move), Избриши
  (Delete), Залепи (Paste), Откажи (Cancel), Преименувај (Rename). Note the split: short commands are imperative
  singular, while full instructional sentences use the polite plural (above). Confidence: high.

## Decision points

The genuinely tricky calls, with how the majors handle each, a recommended default, and a confidence level.

- **Script: Macedonian Cyrillic only.** Macedonian's sole standard script is Cyrillic; there is no Latin UI variant the
  way Serbian has (Serbian ships both `sr-Cyrl` and `sr-Latn`, Macedonian does not). Romanization exists only for
  transliteration of names, never for software UI. Recommendation: ship a single Cyrillic `mk` catalog, no Latin
  variant. Confidence: high.

- **Major-product availability is mixed; lean on Microsoft and GNOME.** Apple does NOT ship Macedonian at all (not a
  macOS/iOS system language), so there is no Apple Finder precedent to match. Microsoft DOES (full localization, plus a
  published style guide and terminology base). GNOME ships Macedonian (Nautilus is translated). Google is partial (core
  products like Search, Gmail, and Android UI are Macedonian-localized, but coverage is uneven and some features lag).
  Spotify added Macedonian in 2023. Netflix has Macedonian subtitles but not a Macedonian interface. Recommendation:
  treat Microsoft terminology + GNOME Nautilus as the primary references; there is no Apple anchor for this locale, so
  don't reach for Finder conventions. Confidence: high.

- **Anglicism handling: use native words, the majors do.** Both Microsoft and GNOME Nautilus consistently use native
  Macedonian terms over English loans for the core domain:
  - file: **датотека** (Microsoft and Nautilus both; the loan "фајл" exists colloquially but neither major uses it in
    UI).
  - folder: **папка** (Microsoft and Nautilus both; "именик" does not appear in Nautilus). "папка" is itself a Slavic
    loan but is the established, universal UI term.
  - Microsoft's noun guidance is explicit: prefer Macedonian words over borrowed ones, keeping loans only where they are
    long-entrenched (клиент, администратор). Recommendation: датотека for file, папка for folder; native verbs
    throughout; keep only the do-not-translate brand list and entrenched acronyms (SMB, MTP, URL). Confidence: high.

- **Quotation marks: Macedonian curly quotes „…".** Microsoft's Macedonian guide specifies the opening low curly quote „
  and the closing high curly quote ", and GNOME Nautilus uses them throughout („Откажи"). A following punctuation mark
  sits OUTSIDE the closing quote. Do not use English "straight" or «guillemet» quotes. Recommendation: „…" around any
  quoted name or title in copy. Confidence: high.

- **Gender, counts, and agreement.** Macedonian nouns carry grammatical gender (masculine/feminine/neuter) and the verb,
  adjective, and any past-tense form must agree with the noun's gender and number. This bites in two places: (1) plural
  count messages, where the branch must agree with the counted noun (датотека is feminine, so "1 датотека" / "5
  датотеки"); write each plural branch as a full natural phrase, never swap only the numeral. (2) Past-tense outcomes,
  where the verb's gender ending depends on the subject noun. Recommendation: write count and outcome strings as
  complete phrases per branch; a human review pass should check gender agreement. Confidence: high.

- **Inclusive / gendered language.** The user is addressed as вие (plural), which sidesteps the he/she problem in
  generic UI copy, so no special measures are needed for addressing the user. Avoid gendered role nouns where a neutral
  phrasing exists. Recommendation: no special handling beyond the вие address. Confidence: medium (low-stakes).

- **Sorting and encoding.** Macedonian Cyrillic has 31 letters including the distinctive Ѓ, Ѕ, Ј, Љ, Њ, Ќ, and Џ;
  collation follows the Macedonian alphabet order, which `Intl.Collator('mk')` handles at runtime. All text is UTF-8;
  never transliterate Cyrillic to Latin in stored or displayed values. Recommendation: rely on `Intl` for sorting and
  number/date formatting; this matters only for any hand-written numeral in copy. Confidence: high.

## Flag for David (owner calls)

- **"pane" term.** Microsoft uses **окно** for a navigation pane; GNOME has no single dominant term. Cmdr is
  pane-centric, so lock one term in the glossary. Recommendation: окно. Confidence: medium.
- **"trash" term.** Cmdr's concept is the macOS Trash. Microsoft uses "канта за отпадоци" (Recycle Bin); GNOME Nautilus
  uses the shorter **ѓубре**. Since there is no Apple Macedonian to match the macOS "Trash" name, pick one and lock it.
  Recommendation: ѓубре (shorter, matches the file-manager peer). Confidence: medium.

## Terminology and glossary

Core terms confirmed against Microsoft terminology and GNOME Nautilus. Extend as strings come up.

| English term       | Macedonian      | Notes                                          |
| ------------------ | --------------- | ---------------------------------------------- |
| file               | датотека        | feminine; not "фајл"                           |
| folder             | папка           | not "именик"                                   |
| copy               | копирај         | imperative                                     |
| move               | премести        | imperative                                     |
| delete             | избриши         | imperative                                     |
| paste              | залепи          | imperative                                     |
| cut                | отсечи          | imperative                                     |
| rename             | преименувај     | imperative                                     |
| open               | отвори          | imperative                                     |
| save               | зачувај         | imperative                                     |
| cancel             | откажи          | imperative                                     |
| search             | пребарување     | the noun; verb form "пребарај"                 |
| trash              | ѓубре           | owner call; Microsoft uses "канта за отпадоци" |
| pane               | окно            | owner call                                     |
| tab                | картичка        | UI tab, not the key                            |
| window             | прозорец        |                                                |
| volume             | волумен         | the storage volume sense                       |
| settings           | параметри       |                                                |
| destination folder | целна папка     |                                                |
| file name          | име на датотека |                                                |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`. Acronyms (SMB, MTP, URL) stay Latin and take
Macedonian inflection with a hyphen when needed.

## Plurals

CLDR plural categories for `mk`: **`one`** and **`other`** (confirmed via
`new Intl.PluralRules('mk').resolvedOptions().pluralCategories`). The `one` category covers numbers ending in 1 except
11 (so 1, 21, 31, 101...); everything else is `other`. This is NOT the same as English's one/other even though both have
two categories: the boundary differs, so don't assume "1 vs many". Write both branches as full natural phrases and mind
gender agreement with the counted noun (датотека is feminine). The `desktop-i18n-plural` check requires both categories.

## Notes and decisions

- Quotation marks: Macedonian curly quotes „…"; following punctuation sits outside the closing quote.
- Capitalization: sentence case, only first word and proper nouns; lowercase days, months, and language names; lowercase
  the вие/ваш pronouns.
- Pronoun economy: Macedonian uses "you/your" far less than English; prefer neutral or reflexive phrasing.
- Error messages: calm, natural, no exclamation marks, never the words "error" or "failed".
- Sorting and numbers: rely on `Intl` ('mk') at runtime; never transliterate Cyrillic.
- Dedicate one human review pass to gender agreement (verbs, adjectives, and past-tense endings agreeing with noun
  gender), the highest-frequency Macedonian UI correctness risk.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/mk/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
