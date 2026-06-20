# Russian (ru) translation style guide

Working notes for translating Cmdr into Russian. Read [`README.md`](../README.md) for how this fits the translation
process.

Russian is fully resourced in the pile: macOS Finder/AppKit, Microsoft terminology + full style guide, GNOME Nautilus +
Xfce Thunar. Lean on macOS Finder first.

## Voice and tone

Friendly, concise, active, never alarmist. Russian tech UI is somewhat more formal and impersonal than English by
default; keep Cmdr's warmth but don't force colloquialisms. Microsoft's Russian style guide explicitly favors a neutral,
respectful register. Error messages stay calm and actionable; avoid alarmist words.

## Formality

Russian UI overwhelmingly **avoids addressing the user with a verb form at all**, using verbal nouns for actions instead.
This is the dominant macOS + Microsoft convention and the single most important register rule:

- Use the **verbal noun (nominalization)** for menu/button actions: "Копировать" (copy), "Переместить" (move),
  "Переименовать" (rename), "Удалить" (delete) are infinitives used as commands, this is the standard, NOT the
  imperative "Скопируй". Apple and Microsoft both use the infinitive-as-command throughout.
- When running text must address the user, use the polite **вы** (lowercase in modern tech UI; uppercase "Вы" is older
  correspondence style). Microsoft's style guide prescribes lowercase "вы". Never the familiar **ты** in product UI.
- Prefer impersonal/passive constructions for system messages ("Файл удалён", "the file was deleted") where English
  uses active; this reads natural in Russian even though Cmdr's English prefers active voice. Don't over-apply: keep it
  concise.

## Decision points

### Script: Cyrillic only (no decision, but lock it)

Russian is Cyrillic, full stop. No Latin transliteration in UI. The only trap is mixing visually identical
Latin/Cyrillic letters (e.g. Latin "c"/"a"/"o" inside a Cyrillic word), keep all-Cyrillic in Russian words.
Recommendation: pure Cyrillic. Confidence: high. Not a real decision, just a correctness guard.

### "ё" vs "е"

Russian optionally writes **ё** but it's very often replaced by **е** in print and UI. Apple and Microsoft generally use
**е** (without dots) in UI except where ё disambiguates. Recommendation: follow the macOS Finder convention (mostly **е**,
ё only where needed for clarity); be consistent across the catalog. Confidence: high.

### Grammatical case agreement with counts and inserted values

The biggest Russian translation hazard. Nouns take different case/number forms after numbers (1 файл, 2 файла, 5
файлов), and any `{placeholder}` carrying a count or a noun phrase can land in the wrong case. This is handled by the
plural categories (see below), but ALSO affects non-count inserts: a `{path}` or `{name}` dropped into a sentence keeps
nominative form, so structure sentences so the insert sits in a position that reads correctly regardless of its
grammatical gender/number. Recommendation: write count messages with full one/few/many/other branches, and phrase
sentences with raw inserts so the insert is in an isolated nominative slot (e.g. "Файл: {name}" not "Перемещение {name}").
Confidence: high. This is the #1 source of clumsy Russian translations.

### Gender and inclusive language

Russian is heavily gendered, including in **past-tense verbs** that agree with the subject's gender ("удалил" masc. vs
"удалила" fem.). If a message ever has the USER as the past-tense subject ("you deleted"), it would force a gender. Avoid
entirely: use impersonal/passive ("Файл удалён", neuter, agrees with "файл", not the user) or the infinitive. There is
NO accepted gender-neutral morphology in Russian product UI; Apple/Microsoft/Google/Spotify/Netflix all avoid the
problem structurally rather than inventing neutral forms. Recommendation: phrase around user-gendered past tense; never
invent neutral endings. Confidence: high.

## Terminology and glossary

Defer the full glossary; triangulate macOS Finder (highest) + Microsoft terminology + Nautilus/Thunar.

| English term | Russian | Notes |
| ------------ | ------- | ----- |
| file | файл | |
| folder | папка | |
| trash | Корзина | Finder term |
| copy | Копировать | infinitive-as-command |
| pane | панель | confirm vs Finder |
| tab | вкладка | |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `ru`: `one`, `few`, `many`, `other`. All four are required and grammatically real:
- `one`: 1, 21, 31… (файл)
- `few`: 2-4, 22-24… (файла)
- `many`: 0, 5-20, 25-30… (файлов)
- `other`: fractionals (файла)
Every count message MUST write all four branches with the correctly cased noun form; this is non-optional and the most
common Russian plural bug is omitting `many`.

## Notes and decisions

- Quotation marks: Russian uses guillemets «...» for primary quotes and „..." (low-9/high-9) for nested. Use «...», not
  English "...".
- Decimal comma, space thousands separator (non-breaking). Let `Intl` format.
- No serial/Oxford comma in Russian (English Cmdr style uses it; Russian punctuation rules differ, follow Russian
  norms, not the English style guide, for in-language punctuation).

## Decisions to confirm with David

- None blocking. The impersonal/passive-by-default register (vs Cmdr's English active-voice preference) is a deliberate
  call worth confirming once with a native reviewer, but it matches every major.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/ru/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
