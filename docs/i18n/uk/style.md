# Ukrainian (uk) translation style guide

Working notes for translating Cmdr into Ukrainian. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Ukrainian. Term rulings live in
`terms.json` (keyed by the shared `../concepts.json`), their rationale in `decisions.md`, typography in
`mechanics.json`, and open questions in `review-queue.md`.

Ukrainian is well resourced in the reference pile (`_ignored/i18n/uk/` in the main clone): macOS (Finder, AppKit, System
Settings), Microsoft terminology and style guide, Total Commander, Double Commander, GNOME Nautilus, KDE Dolphin, and
Xfce Thunar. The live macOS bundles on any Mac (`uk.lproj`, `.loctable`) fill what the pile's macOS extract lacks (the
Privacy & Security rows, for example). macOS wins for anything a Mac user sees in their own system; Total Commander and
Double Commander win for two-pane vocabulary Finder doesn't have. Sources are read to decide a term, never copied
verbatim: Cmdr writes its own value.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Ukrainian, not Russian**: the most visible failure is Russian leaking in. Write modern standard Ukrainian (2019
  orthography) the way Ukrainian macOS and current Ukrainian apps speak. ❌ Never calque a Russian construction or term
  (`являтися`, `слідуючий`, `на протязі`, `приймати участь`, `співпадати`, `вірний` for "correct", `згідно з чим`
  piles); use `бути`, `наступний`, `протягом`, `брати участь`, `збігатися`, `правильний`. Ukrainian letters `і`, `ї`,
  `є`, `ґ` wherever the word needs them. If a phrase only sounds right because Russian says it that way, rewrite it.
- **Address**: polite `ви`, always lowercase (`ви`, `вас`, `ваш`), as Ukrainian macOS writes it (140 lowercase, zero
  capitalized mid-sentence). Never `ти`. Keep direct address light: Ukrainian UI often phrases impersonally
  (`Можна зачекати`, `Потрібен дозвіл`). Onboarding, About, and a few notes may speak as David in the first person.
- **Voice**: everyday, warm, calm; ❌ never officialese (канцелярит). Prefer a verb clause to a noun chain, `щоб` to
  `для + verbal noun` (`Натисніть Esc, щоб очистити`, not `для очищення`), `потрібно` / `треба` to `необхідно`, and
  `цей` to `даний`. Keep the English's warmth, apologies, and jokes when the `@key` description asks for them.
- **No «помилка» in error copy**: "Couldn't X" → `Не вдалося X` (a one-time result, macOS: `Не вдалося створити папку`);
  "Can't X" (a standing state) → `Не вдається X` or `Неможливо X`; "Cmdr couldn't X" → `Cmdr не вдалося X`; "Something
  went wrong" → `Щось пішло не так`; "ran into a problem" → `виникла проблема`. Compounds are fine (`звіт про помилку`,
  `друкарська помилка`). "Try again" as a button → `Спробувати ще раз`; in prose → `Спробуйте ще раз`.
- **Register by UI slot**:
  - buttons, menu items, commands: perfective infinitive, macOS-style (`Скопіювати`, `Перейменувати`, `Скасувати`,
    `Вийняти`); ❌ never an imperative (`Скопіюйте`) on a button;
  - toggle and checkbox labels: an imperfective infinitive or a noun phrase describing a standing behavior
    (`Показувати приховані файли`); a switch named after a command keeps that command's name;
  - progress lines: a verbal noun with `…` (`Копіювання…`, `Підʼєднання: {name}…`);
  - finished results: the impersonal `-но/-то` form (`Скопійовано`, `Переміщено`, `Видалено`): no gender, no person;
  - status chips: a short adjective or participle agreeing with its noun (`Підʼєднано`, `Призупинено`), or a noun;
  - dialog titles: an infinitive question (`Видалити «{name}»?`) or a noun phrase;
  - instructions inside a sentence: the polite imperative (`Відкрийте {systemSettings}`) or an impersonal
    (`Потрібно дозволити доступ`).
- **Native names follow Ukrainian macOS**: `Смітник` (capitalized, `у Смітник`, `зі Смітника`), `Досьє` (Get Info),
  `Швидкий перегляд` (Quick Look), `Системні параметри`, `Параметри…` (the app Settings menu item),
  `Приватність і безпека`, `Повний доступ до диска`, `Файли та папки`, `Локальна мережа`, `Автоматизація`, `Програми`
  (the folder), `Викачане` (Downloads), `Документи`, `Робочий стіл`, `Домівка` (Go > Home), `Перехід до папки…` (Go to
  Folder), `Підʼєднатися до сервера…`, `Вийняти` (Eject), `Замкнено` (Locked), `Довідка` (Help), `Теги`, `Бічна панель`,
  `Недавні`, `Улюблені`, `Мережа`, `Показати у Finder`, `Відкрити програмою` (Open With), `Показати вміст пакета`,
  `Емодзі та символи`, `Повноекранний режим`, `Доступність` (Accessibility), `Експертні` (Finder's Advanced tab),
  `Спільний доступ і привілеї`, `Сховати решту`, `Повернути назад` (Put Back), `Стартовий диск`, `Панель стану`. Apple
  app names localized in Ukrainian: `Термінал`, `Мініредактор` (TextEdit), `Ключар` (Keychain Access),
  `Монітор активності`, `Дискова утиліта` / `Швидка допомога` (First Aid), `Пошта`, `Повідомлення`, `Нотатки` (Mail,
  Messages, Notes; live bundles). Kept Latin: Finder, Dock, Spotlight, Mission Control, AirDrop, iCloud Drive, Time
  Machine. Verify any other macOS label in the pile or the live bundle before quoting it.
- **Product vocabulary** (rulings in `terms.json`): item → `елемент` (macOS, ❌ not `обʼєкт`); delete → `Видалити`,
  remove from a list → `Вилучити` (macOS keeps the two apart); Undo → `Відмінити`, Cancel → `Скасувати` (macOS keeps
  these apart too); Settings → `Параметри`; folder → `папка` (❌ not `тека`, the GNOME word); pane → `панель`; tab →
  `вкладка`; volume → `том`; drive / disk → `диск`; connect / disconnect → `підʼєднати(ся)` / `відʼєднати(ся)`; download
  / upload → `викачати` / `закачати` (macOS); name (a file's) → `назва`, a person's or account name → `імʼя`; Resume →
  `Поновити`; Pause → `Призупинити`; Merge → `Обʼєднати`; app → `програма` (❌ not `застосунок` / `додаток`, macOS and
  Microsoft agree); default → `типовий` (❌ not `за замовчуванням`, a Russian calque); select / choose → `вибрати`,
  selected → `вибрано`; mount → `змонтувати`; permissions → `привілеї`; private key → `приватний ключ` (live Keychain
  Access, ❌ not Microsoft's `закритий ключ`). Ask Cmdr stays verbatim and takes no case ending: put case on a head noun
  (`у вікні Ask Cmdr`). click → `натиснути` / `клацнути`; double-click → `двічі клацнути`.
- **Placeholders**: a `{name}`, `{path}`, `{server}`, `{app}` can't decline and has no known gender. Keep it nominative:
  a type noun in front (`папка {name}`, `сервер «{server}»`), a quoted name like Finder (`Перейменувати «{name}»`), a
  label colon (`Пропущено: {name}`), or the sentence subject in the present tense. ❌ Never a bare insert where
  Ukrainian needs a case ending (`у {folder}`), ❌ never a past-tense verb or adjective that guesses its gender.
- **Fragments**: a value that lands inside another sentence (an ETA, a fallback name, a duration, a unit word) must read
  correctly in its host. Find the host key and read the assembled sentence. Mid-line fragments start lowercase.
- **Plurals**: CLDR `one` / `few` / `many` / `other`, all four required. `one` covers 21, 31, 101 (`21 файл`); use `=1`
  only for wording that means exactly one (`цей файл`). `few` takes nominative plural (`2 файли`), `many` genitive
  plural (`5 файлів`), `other` (fractions) genitive singular (`1,5 файлу`). Put every word that agrees with the count
  INSIDE the branches, the verb and participle too (`скопійовано 21 файл` works for every branch; `вибрано` is the safe
  participle). Without a plural param, use a counted label colon (`Файлів: {countText}`) or Finder's parenthesis.
- **Gender**: never gender the user (no `ви видалив(ла)`, no `готовий` addressed to them); use the present tense, the
  infinitive, the impersonal `-но/-то`, or plural `ви` forms (`ви скасували`). Cmdr is masculine when a past tense is
  unavoidable (`Cmdr зберіг`), but `Cmdr не вдалося` (impersonal) is better. macOS is feminine (`macOS заблокувала`).
- **Typography** (`mechanics.json`): quotes `«…»`, nested `„…“`; the apostrophe is the letter `ʼ` (U+02BC), as Ukrainian
  macOS writes it (`підʼєднати`, `обʼєкт`, `імʼя`), ❌ never ASCII `'` or `’` (and the ICU doubling rule then never
  bites); ellipsis is the single `…` hugging its word; a no-break space before `%` (`42 %`) and before a dash `—`
  (`Cmdr — файловий менеджер`); en dash for ranges (`2–3`); decimal comma and space-grouped thousands come from the
  formatter. Size units are Cyrillic, as Finder writes them (`100 МБ`, `512 байтів`).
- **No hedged grammar**: ❌ no `файл(и)`, `видалив(ла)`, `елемент/и`, no guessed ending glued to an insert (`{name}а`).
  Use ICU `plural` / `select`, or restructure.

## Voice and tone

Friendly, concise, active, calm. Microsoft's Ukrainian voice guidance (clear, friendly, concise, everyday words, no
old-fashioned or overly formal vocabulary) matches Cmdr's English voice, so it carries over cleanly. Keep sentences
short and natural-spoken.

The danger in Ukrainian runs two ways at once: careful translation drifts into канцелярит (nominal chains, `здійснити`,
`даний`, `з метою`, `у разі необхідності`), and it drifts toward Russian syntax, since many translation habits were
formed on Russian software. Pick the everyday form a Ukrainian speaker would say to a colleague in 2026:

- `Здійснюється копіювання файлів` → `Копіювання файлів…`
- `Для продовження роботи необхідно надати дозвіл` → `Щоб продовжити, дайте дозвіл`
- `У даній папці` → `У цій папці`
- `Співпадає з` → `Збігається з`; `слідуючий` → `наступний`; `на протязі` → `протягом`; `являється` → `є`.

Error messages stay calm and actionable. State the problem and the next step, with a recovery action where there is one.

## Formality

- **`ви` (formal), lowercase mid-sentence.** Never `ти`. Ukrainian macOS writes lowercase throughout, and so does
  Microsoft.
- **Buttons and menu actions: perfective infinitive.** This is the macOS norm and the single most important register
  choice: `Скопіювати`, `Відкрити`, `Закрити`, `Видалити`, `Скасувати`, `Стерти`.
- **In-progress status: verbal noun** (`Копіювання файлів…`, `Дублювання`), the way macOS and Thunar phrase operations
  in flight.
- **Instructions inside a sentence: polite imperative or impersonal** (`Спершу видаліть файл`, `Потрібно додати текст`),
  which also dodges the past-tense gender trap.

## Decision points

### Avoid gendered past-tense and adjective agreement (high)

Ukrainian past-tense verbs and adjectives agree with the subject's gender and number. A sentence that puts the user in
the past tense forces a gender the app can't know (`ви видалив` vs `ви видалила` in the singular), which is wrong for
half of users. Prefer the present tense, the impersonal `-но/-то` form (`Файл переміщено`), the infinitive, or the
plural `ви` past (`ви скасували`, grammatically plural so gender-free). Describing a FILE is fine: its noun fixes the
gender (`файл` masculine, `папка` feminine, `елемент` masculine).

### Russian-convention leakage (high)

Guard the letters (`і` / `ї` / `є` / `ґ`, the apostrophe letter), the vocabulary (pull every term from the Ukrainian
pile, never guess from Russian), and the syntax (`щоб` clauses, `-но/-то` impersonals, and verbs over noun chains).
Russian is shipped in Cmdr; ❌ don't read the Russian catalog as a template. A native Ukrainian reviewer will proofread;
anything you're unsure reads as Ukrainian goes in `review-queue.md`.

### Regional variant and script (high)

Ukrainian has one script (Cyrillic) and one mainstream written standard, so Cmdr ships the single `uk` base with no
overlay.

## Terminology

The agreed translation for recurring terms lives in `terms.json`, one ruling per concept from `../concepts.json`.
Cross-term notes:

- **Short labels** start with a capital and use sentence case (`Показати приховані файли`); Ukrainian never title-cases.
- **Compounds**: Ukrainian builds noun phrases with genitive or an adjective (`журнал операцій`, `черга передач`), not a
  glued compound. Keep those phrases short; more than two genitives in a row reads like officialese.
- **Foreign technical terms** Ukrainian IT uses natively stay as words (`сервер`, `хост`, `токен`, `кеш`); a protocol or
  format name stays Latin (`SMB`, `SFTP`, `ZIP`).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, plus the `{system_settings}`-style tokens (the full
list: BRAND_WORDS + SYSTEM_TOKENS in `apps/desktop/scripts/i18n-catalog-lib.ts`). Quick Look is NOT kept: Ukrainian
macOS calls it `Швидкий перегляд`. A brand takes no case ending (`у Cmdr`, `для Cmdr`, `Cmdr зберіг`): Ukrainian leaves
an indeclinable foreign name bare, like `у Finder`.

## Plurals

CLDR categories for Ukrainian: `one`, `few`, `many`, `other` (verified with
`new Intl.PluralRules('uk').resolvedOptions().pluralCategories`, 2026-06-20). The `desktop-i18n-plural` check requires
all four.

- `one`: numbers ending in 1 but not 11 (1, 21, 101). Nominative singular: `1 файл`, `21 папка`.
- `few`: ending in 2–4 but not 12–14 (2, 3, 22). Nominative plural: `2 файли`, `3 папки`.
- `many`: ending in 0, 5–9, or 11–14 (5, 11, 25, 100). Genitive plural: `5 файлів`, `11 папок`.
- `other`: fractions (1,5). Genitive singular: `1,5 файлу`, `2,5 гігабайта`.

Adjectives and participles agree too (`1 вибраний файл`, `2 вибрані файли`, `5 вибраних файлів`), so put them inside the
branches. The neuter impersonal (`вибрано 5 файлів`, `скопійовано 21 файл`) is the easiest frame: the same verb form
works in every branch.

## Typography

As Ukrainian macOS writes it (the pile's Finder, AppKit, and System Settings strings, counted 2026-10-29):

- Quotes `«…»` (763 in the pile, 16 curly `“`); nested `„…“`.
- The apostrophe is `ʼ` (U+02BC MODIFIER LETTER APOSTROPHE): 212 in the pile, zero ASCII `'`, zero `’` inside a word.
  It's a letter, so ICU never treats it as an escape.
- Ellipsis `…` (262 vs 10 three-dot), hugging its word.
- `%` takes a no-break space (`100 %`, `^0 % завершено`); the dash `—` takes a no-break space before it and a plain
  space after.
- Ranges take an en dash with no spaces (`2–3`).

## Notes and decisions

- **Length.** Ukrainian runs 10–20% longer than English, and case endings lengthen nouns. Overflow-check the layout
  against the pseudolocale (`en-XA`).
- **Numbers and dates come from the formatter layer** (`1 234 567,89`, `20.06.2026`), never hardcoded in a string.
- **Search keywords** (`settings.*.keywords`): Ukrainian users often type with a Ukrainian keyboard whose apostrophe key
  yields ASCII `'`, so prefer keywords without an apostrophe where a synonym exists.
- Case-by-case rulings go in `decisions.md` under a heading citing their keys.

## Termbase files

`terms.json` holds one ruling per concept (`chosen`, `sources`, `confidence`, plus `accept` / `forms` / `avoid`), seeded
from the reference pile on the first translation pass (2026-10-29). Schema: `../termbase.md`.
