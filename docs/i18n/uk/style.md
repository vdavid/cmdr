# Ukrainian (uk) translation style guide

Working notes for translating Cmdr into Ukrainian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Ukrainian.

Evidence is triangulated against the reference pile (`_ignored/i18n/uk/`: macOS strongest, then Microsoft terminology
and style guide, then GNOME Nautilus and Xfce Thunar) on 2026-06-20. Sources are read to decide a term, never copied
verbatim (Apple and Microsoft are copyrighted, GNOME and Xfce are GPL): Cmdr writes its own value.

## Decisions to confirm with David

These are the calls a translator can't make alone. The rest of this guide assumes them. Both resolved confidently from
the reference pile, listed here only so they aren't relitigated:

- **Formal `Ви` address, resolved (high).** Microsoft's Ukrainian style guide directs avoiding the informal «ти» in
  favor of the formal «Ви» for software (paraphrased; the substance is supported, but the exact quoted wording wasn't
  verifiable from the PDF). macOS Finder addresses the user formally too. Settled, not pending.
- **Cyrillic only, target the single `uk` base (high).** Ukrainian has one script (Cyrillic) and one mainstream written
  standard. No regional split to manage. The one live risk is Russian-convention leakage, called out below.

## Voice and tone

Friendly, concise, active, calm. Microsoft's Ukrainian voice guidance ("clear, friendly, and concise," everyday
conversational words, avoid "old-fashioned, too formal or archaic words") matches Cmdr's English voice, so it carries
over cleanly. Keep sentences short and natural-spoken, not bureaucratic or officialese.

Error messages stay calm and actionable, and never label themselves with "помилка" (error) or "не вдалося" as a status
the way English avoids "error"/"failed". State the problem and the next step. macOS Finder's pattern is the model here:
"Не вдалося створити папку." ("Couldn't create the folder.") reads as a calm statement, not an alarm; pair it with a
recovery action.

## Formality

- **`Ви` (formal), lowercase mid-sentence.** Never `ти`. Capitalized `Ви` only where a sentence genuinely addresses one
  specific person directly and respectfully (rare in UI); default to lowercase per Microsoft.
- **Buttons and menu actions: perfective infinitive, not imperative.** This is the macOS norm and the single most
  important register choice. macOS Finder/AppKit: "Скопіювати" (Copy), "Відкрити" (Open), "Закрити" (Close), "Видалити"
  (Delete), "Скасувати" (Cancel), "Стерти" (Erase). Use the infinitive form for action labels, NOT the imperative
  "Скопіюй"/"Скопіюйте".
- **In-progress status: verbal noun.** "Копіювання файлів…" (Copying files…), "Дублювання" (Duplicating), the way Thunar
  and macOS phrase operations in flight.
- **Explanatory/instructional sentences to the user: imperative plural or impersonal.** When telling the user to do
  something inside a sentence, Microsoft prefers the polite imperative ("Спершу видаліть файл" = Delete the file first)
  or an impersonal construction ("Потрібно додати текст" = Text needs to be added), specifically to dodge the
  second-person past-tense gender trap (see Decision points).

## Decision points

### Avoid gendered past-tense and adjective agreement (high)

Ukrainian past-tense verbs and adjectives agree with the subject's gender and number. A sentence that puts the user in
the past tense forces a gender the app can't know ("Ви видалив" masculine vs "Ви видалила" feminine), which is wrong for
half of users and reads as a defect.

- **Recommendation:** structure copy so no message about the user uses a gendered past-tense verb or adjective referring
  to them. Microsoft's own neutralization strategy: prefer present tense, impersonal forms, or imperatives. Concrete:
  "Файл переміщено" (the file was moved, impersonal `-но/-то` passive, no gender) over "Ви перемістили файл"; "Готово"
  over a gendered "completed"; present "{0} відкриває файл" over past "{0} відкрив файл". The impersonal `-но/-то` form
  ("видалено", "скопійовано", "переміщено") is the workhorse: it states the result with zero gender.
- Same care for adjectives describing the user. Describing a FILE is fine because the file's grammatical gender is fixed
  by the noun (файл is masculine, папка feminine, etc.) and known.
- Confidence: high. This is the dominant real-world Ukrainian UI gender concern and Microsoft documents the exact
  workaround.

### Don't let Russian conventions leak in (high)

Ukrainian and Russian share a Cyrillic base but are distinct languages; mixing them is the most visible localization
failure, and politically charged. Guard specifically:

- **Ukrainian-only letters must appear where the word needs them:** і (not Russian и as the default "i"), ї, є, and ґ.
  Ukrainian also has и and е but with different values than Russian. Don't transliterate from a Russian term.
- **The apostrophe is a LETTER, not punctuation.** It separates a hard consonant from an iotated vowel: комп'ютер,
  об'єкт, прем'єр. macOS renders it with the modifier-letter apostrophe ʼ (U+02BC), e.g. "Компʼютер". A plain ASCII `'`
  is the common practical choice and acceptable; be consistent. This collides with the ICU apostrophe-doubling rule, so
  see Plurals/Notes: every literal apostrophe in an ICU value must be doubled (`''`), or ICU silently eats text.
- Use Ukrainian terms, not Russian-derived ones (e.g. "тека"/"папка" for folder, not a Russian calque). Pull every term
  from the Ukrainian reference pile, never guess from Russian.
- Confidence: high. Recommendation: native review is the real backstop here; flag anything you're unsure reads as
  Ukrainian rather than Russian-influenced.

### Number, date, and quotation formatting (high)

- **Numbers come from the formatter layer, never hardcode separators.** Ukrainian uses a space (non-breaking) thousands
  separator and a comma decimal: `1 234 567,89`, `1,5` (verified with `Intl.NumberFormat('uk')`, 2026-06-20). Let
  `formatNumber()`/`formatBytes()` produce these from the locale.
- **Dates:** `dd.MM.yyyy` (e.g. `20.06.2026`), verified with `Intl.DateTimeFormat('uk')` 2026-06-20. From the formatter
  layer, not strings.
- **Quotation marks: «…» (guillemets)** as the primary pair, the way macOS Finder writes them ("папки «Робочий стіл»").
  For a nested quote, the German-style „…" can be used inside. Avoid English "…".
- **Sentence case is native.** Ukrainian doesn't capitalize common nouns, weekdays, or months, so the app's
  sentence-case rule applies cleanly. Don't title-case.
- Confidence: high.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. macOS wins when sources differ, since Cmdr is a macOS app. Verified
against `_ignored/i18n/uk/` on 2026-06-20.

- **file: `файл`** · macOS, MS, GNOME, Thunar all agree. Definite forms by case; plural "файли". `high`.
- **folder: `папка`** · macOS Finder ("Не вдалося створити папку", "вкладеної папки"). Thunar/GNOME also use "тека";
  prefer macOS's "папка" for consistency with the platform, reserve "тека" only if mirroring a GNOME-style surface.
  `high`.
- **copy: `Скопіювати` (action) / `Копіювання` (in progress)** · macOS AppKit, Thunar. `high`.
- **move: `Перемістити`** · macOS Finder ("перемістити до {destination}", "Переміщує елементи у Смітник"). `high`.
- **delete: `Видалити`** · macOS Finder/AppKit. The move-to-trash safe action is separate (below). `high`.
- **erase (permanent, format/wipe): `Стерти`** · macOS Finder ("Erase" = "Стерти", "Стартовий диск"). Use for the
  destructive wipe sense, not routine delete. `high`.
- **rename: `Перейменувати`** · Thunar, MS terminology. `high`.
- **open: `Відкрити`** · macOS. `high`.
- **cancel: `Скасувати`** · macOS Finder. Imperative-infinitive on buttons. `high`.
- **trash (the location): `Смітник`** · macOS Finder ("у Смітник", "Елементи в Смітнику"). Capitalized as a place name,
  the way Finder shows it. `high`.
- **move to trash: `Перемістити у Смітник`** (or Finder's "Викинути … у Смітник") · macOS Finder. Prefer "Перемістити у
  Смітник" for the neutral action label. `high`.
- **eject: `Вийняти`** · macOS AppKit ("NSNavEjectButton" = "вийняти"). `high`.
- **duplicate: `Дублювати` / `Дублювання`** · macOS Finder ("Дублює елементи", "для дублювання"). `high`.
- **settings: `Параметри`** · MS terminology and Microsoft's standard; note macOS may show "Налаштування" in places.
  `tentative` (macOS-vs-MS split possible): confirm which the user-facing macOS surface shows.
- **search: `Пошук` (noun) / `Шукати` (action)** · MS terminology, common Ukrainian IT usage. `high`.
- **overwrite: `Перезаписати`** · macOS Finder ("Залишати чи перезаписувати … розширення"). `high`.
- **volume / disk: `том` / `диск`** · macOS Finder ("Зовнішні диски", "Стартовий диск"); "том" for a mounted volume per
  MS terminology. `high`.
- **pane / tab: `панель` / `вкладка`** · MS terminology ("вкладка" for tab). The two file lists are "панелі".
  `tentative` for pane (no direct macOS Finder source); low risk.

Add terms as they come up in this same `chosen · sources · confidence` shape.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. macOS UI names Cmdr opens into (System Settings panes, "Смітник") should
match what a Ukrainian macOS actually shows.

## Plurals

CLDR categories for Ukrainian: **`one`, `few`, `many`, `other`** (4 forms; verified with
`new Intl.PluralRules('uk').resolvedOptions().pluralCategories`, 2026-06-20). The `desktop-i18n-plural` check requires
every plural message to cover all four. Real catalogs (Nautilus) use `nplurals=4`, so this is well-trodden.

The categories map to noun-ending agreement after the count (this is WHY there are four, and the branch text must use
the matching case form):

- **`one`**: numbers ending in 1 but not 11 (1, 21, 101). Noun in nominative singular: "1 файл", "21 файл".
- **`few`**: ending in 2-4 but not 12-14 (2, 3, 4, 22, 23). Noun in genitive singular: "2 файли", "3 папки".
- **`many`**: ending in 0, 5-9, or 11-14 (5, 11, 12, 25, 100). Noun in genitive plural: "5 файлів", "11 папок".
- **`other`**: fractional or decimal values: "1,5 файлу".

Write all four branches with the correctly-cased noun in each; don't reuse one noun form across branches. The number
itself comes from the formatter, but the noun and any agreeing adjective live in the branch text. Combine with the
gender-avoidance rule above: if a counted-items message also references an action on the user, keep that part impersonal
(`-но/-то`) so no branch carries a gendered past-tense verb.

## Notes and decisions

- **ICU apostrophe doubling is extra-sharp here.** Because the apostrophe is a real Ukrainian letter (комп'ютер,
  об'єкт), it appears in ordinary words inside ICU values, where a lone `'` is an ICU escape that silently swallows
  following text. Double EVERY literal apostrophe (`'` → `''`) in ICU-formatted values. (Keys under `errors.*` are raw,
  not ICU: there use a single normal apostrophe. See `i18n.md` § Error pipeline and the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).)
- **Length.** Ukrainian runs somewhat longer than English (often 10-20% more), and case endings lengthen nouns.
  Overflow-check the layout against the pseudolocale (`en-XA`).
- **Numbers/dates from the formatter layer**, never hardcoded in a string (see Decision points).
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/uk/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
