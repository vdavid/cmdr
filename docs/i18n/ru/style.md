# Russian (ru) translation style guide

Working notes for translating Cmdr into Russian. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Russian. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`), their rationale in `decisions.md`, typography in `mechanics.json`, and open
questions in `review-queue.md`.

Russian is fully resourced in the reference pile: live macOS 27 (Finder, AppKit, System Settings), Microsoft terminology
and style guide, Total Commander, Double Commander, GNOME Nautilus, KDE Dolphin, and Xfce Thunar. macOS wins for
anything a Mac user sees in their own system; Total Commander and Double Commander win for two-pane vocabulary Finder
doesn't have.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: polite `вы`, always lowercase (`вы`, `вас`, `ваш`), even though Apple's Russian capitalizes `Вы`. Never
  `ты`. Keep direct address light: Russian UI often phrases neutrally (`Можно подождать`, `Нужно разрешение`).
  Onboarding, About, and a few notes may speak as David in the first person (`мне будет интересно узнать`).
- **Voice**: everyday, warm, calm; ❌ never bureaucratic. Prefer a verb clause to a noun chain
  (`Пока Cmdr читал папку, соединение прервалось`, not `При чтении папки Cmdr соединение сбросилось`), `чтобы` to
  `для + noun` (`Нажмите ESC, чтобы очистить`, not `для очистки`), `нужно` to `требуется`. Keep the English's warmth,
  apologies, and jokes when the `@key` description asks for them.
- **No «ошибка» in error copy**: "Couldn't X" → `Не удалось X`; "Cmdr couldn't X" → `Cmdr не удалось X`; "Something went
  wrong" → `Что-то пошло не так`; "ran into a problem" → `возникла проблема`; a failed count → `не выполнено: {n}`.
  Compounds are fine (`отчет об ошибке`, `опечатка`). "Try again?" beside a Retry button → `Попробовать еще раз?`; with
  no button → `Попробуйте еще раз (чуть позже).` (❌ not the stock `Повторите попытку через некоторое время`).
- **Register by UI slot**:
  - buttons, menu items, commands: perfective infinitive, Finder-style (`Скопировать`, `Переименовать`, `Отменить`);
  - toggle and checkbox labels: a noun phrase or an imperfective infinitive that describes a standing behavior
    (`Показывать скрытые файлы`); a switch named after a command keeps that command's exact name;
  - option labels in a picker: imperfective (`Просматривать`, `Открывать`, `Спрашивать`);
  - progress lines: a verbal noun with `…` (`Копирование…`, `Подключение: {name}…`);
  - status chips: a short participle or adjective agreeing with what it describes (`Подключен` for a server, `Завершено`
    for an operation), or a noun (`Ожидание`);
  - dialog titles: a question in the infinitive (`Удалить «{name}»?`) or a noun phrase.
- **Native names follow Russian macOS**: `Корзина` (capitalized), `Свойства` (Get Info), `Быстрый просмотр`,
  `Системные настройки`, `Конфиденциальность и безопасность`, `Связка ключей`, `Программы` (the folder), `Загрузки`,
  `Личное` (Go > Home), `Переход к папке…`, `Подключение к серверу…`, `Убрать в Dock`, `Справка`, `Защита` (Locked),
  `Мониторинг системы`, `Дисковая утилита` / `Первая помощь`, `Защита целостности системы`, `Терминал`, `Просмотр`
  (Preview), `Эмодзи и символы`, `Mac с чипом Apple`. Kept Latin: Finder, Dock, Spotlight, Mission Control, Spaces,
  iCloud Drive, TextEdit. Full Disk Access: the Privacy row is `Доступ к диску`, prose says `полный доступ к диску`.
- **Product vocabulary**: file marking is `выделение` / `выделить` / `Снять выделение` / `Инвертировать выделение`
  (Total Commander), while picking an option or ticking a queue row stays `выбрать`. `Ask Cmdr` stays verbatim, takes
  masculine agreement (`Ask Cmdr выключен`), and puts any case on a head noun (`в настройках Ask Cmdr`). click →
  `нажать`, double-click → `дважды нажать` / `двойное нажатие`. Full view → `Подробный режим`, brief view →
  `Краткий режим`. More top traps below.
- **Placeholders**: a `{name}`, `{path}`, `{server}`, `{app}` can't decline and has no known gender. Keep it nominative:
  a type noun in front (`папка {name}`, `сервер «{server}»`), a quoted name like Finder (`Переименовать «{name}»`), a
  label colon (`Пропущено: {name}`), or the sentence subject in present tense. ❌ Never a bare insert after a
  preposition (`в {ancestor}`), ❌ never a colon straight after a preposition or a transitive verb (`к: {path}`,
  `для: {name}`), ❌ never a past-tense verb or pronoun that guesses its gender.
- **Fragments**: a value that lands inside another sentence (an ETA, a fallback name, a duration, a unit word) must read
  correctly in its host. Find the host key and read the assembled sentence. Mid-line fragments start lowercase.
- **Plurals**: CLDR `one` / `few` / `many` / `other`, all four required. `one` covers 21, 31, 101 (`21 файл`); use `=1`
  only for wording that means exactly one (`этот файл`). Put every word that agrees with the count INSIDE the branches,
  the verb and participle too (`скопирован 21 файл` / `скопировано 5 файлов`). Without a plural param, use a counted
  label colon (`Файлов: {countText}`, `Выполнено: {percentText} %`) or Finder's parenthesis (`объекты ({countText})`).
- **Gender**: never gender the user (no `прочитал(а)`, no `готов` / `уверен` addressed to them); use the present, the
  infinitive, an impersonal (`Удалено`), or plural `вы` forms (`вы отменили`). Cmdr is masculine when a past tense is
  unavoidable (`Cmdr сохранил`), but `Cmdr не удалось` (impersonal) is better. macOS is feminine
  (`macOS заблокировала`).
- **Typography** (`mechanics.json`): quotes `«…»`, nested `„…“`; ellipsis is the single `…`, hugging its word
  (`Копирование…`); a no-break space before `%` (`42 %`) and before a dash `—` (`Cmdr — файловый менеджер`); `е`
  everywhere, never `ё`; en dash for ranges (`2–3`); decimal comma and space-grouped thousands come from the formatter.
  Size units are Cyrillic, as Finder writes them (`100 МБ`, `512 байт`), in the file list and prose alike; binary ones
  are IEC (`4 ГиБ`).
- **No hedged grammar**: ❌ no `файл(ы)`, `удалил(а)`, `объект/а`, no guessed ending glued to an insert (`{name}а`). Use
  ICU `plural` / `select`, or restructure.
- **Top traps** (details in `terms.json`):
  - host → `хост` (never `узел`); network share → `общая папка` (never `общий ресурс`); AI provider → `поставщик` (never
    `провайдер`); Advanced → `Дополнения`; delete permanently → `безвозвратно`; custom → `Пользовательский` /
    `Настроить…` (never `Свой`).
  - dismiss → `Закрыть` for a toast, dialog, or notice, `Скрыть` for a finished queue row or a one-time hint; remove
    from a list → `Удалить из <списка>` or `Убрать`, always naming the list, so it never reads as deleting files.
  - item → `объект` (never `элемент`); operation → `операция`; transfer → `передача`; volume → `том`; drive → `диск`;
    pane → `панель`; tab → `вкладка`; viewer → `окно просмотра` (never `просмотрщик`).
  - Undo and Cancel are both `Отменить`, like Russian macOS; Locked → `Защита` / `защищен` (never `заблокирован`);
    extract an archive → `распаковать` (`извлечь` is Eject); log → `журнал`; status → `состояние`.
  - home folder → `папка пользователя`, the Go menu item `Личное`; incoming file in a conflict → `Входящий файл`
    (`Новый файл` is the command); stalled → `замерла` (alive, not stopped); toggle → `Переключить` or both states.

## Voice and tone

Cmdr's English is friendly, concise, active, and calm. In Russian the danger runs one way: careful translation drifts
into канцелярит, the officialese of forms and bank apps. Nominal chains, `для + verbal noun`, `является`, `осуществить`,
`данный`, and `в течение некоторого времени` make a correct sentence sound like a notice from a ministry. Pick the
everyday form a Russian speaker would say to a colleague.

Worked examples from the shipped catalog (before → after):

- `При чтении папки Cmdr сетевое соединение неожиданно сбросилось.` →
  `Пока Cmdr читал папку, сетевое соединение неожиданно прервалось.` A genitive chain with `Cmdr` in the middle reads as
  "Cmdr's folder".
- `Том {volumeName} отключился после копирования Cmdr файлов: {done} из {total}.` →
  `Том {volumeName} отключился: Cmdr успел скопировать на него {done} из {total} файлов.`
- `Эта папка управляется сервисом **{name}**.` → `Этой папкой управляет **{name}**.` The passive is an English calque,
  and the active keeps `{name}` nominative.
- `Меньшие значения отзывчивее, но используют больше процессора.` →
  `При меньших значениях прогресс обновляется плавнее, но нагрузка на процессор выше.`
- `Результаты поиска не являются путем для перехода.` → `К результатам поиска нельзя перейти по пути.`
- `Нажмите ESC для очистки` → `Нажмите ESC, чтобы очистить`.
- `Повторите попытку через некоторое время.` → `Попробуйте еще раз чуть позже.`
- `Ожидание отсутствия активности` → `Ожидание простоя`.
- `Начинается новое сканирование для сохранения точности` → `Cmdr сканирует заново, чтобы индекс оставался точным`.
- `Перейдите в папку вместо открытия как файла.` → `Перейдите в папку, а не открывайте ее как файл.`
- `Само это не исправится` → `Повторная попытка не поможет`.

Warmth and first person:

- Where the `@key` description asks for a personal close, keep it. `main.oldMacos.body` ends
  `Если что-то сломается, мне все равно будет интересно об этом узнать.`, not the cold
  `Если что-то не работает, сообщите об этом.`
- An apology the English makes stays (`Извините, текст слишком длинный.`). No apology where Cmdr made a deliberate
  choice.
- Upbeat tips stay upbeat: `Кстати, к загрузкам можно перейти одним нажатием`, not
  `Полезный совет о переходе к загрузкам`.
- Where the description asks for direct address, use `вы`: `Вы отменили открытие телефона.`, not
  `Открытие телефона отменено.` (plural `вы` forms carry no gender).
- `Можно …` beats the permissive filler `Вы можете …`: `Его можно в любой момент перетащить в Dock`.

Error copy states what happened and what to do next, calmly. It never says `ошибка` or `сбой` as a label, and it blames
the thing, not the person: `Пароль не подошел`, not `Вы ввели неверный пароль`. The mapping:

- "Couldn't X" → `Не удалось X`; "Cmdr couldn't X" → `Cmdr не удалось X` (impersonal, no gender).
- "Something went wrong" → `Что-то пошло не так`; "X ran into a problem" → `При X возникла проблема`.
- "Failed: 3" in a summary → `не выполнено: 3`; a network status cell "Error" → `Проблема`.
- "Try again?" beside a Retry button → `Попробовать еще раз?` (the infinitive question points at the button); with no
  button → `Попробуйте еще раз.`

## Formality

**`вы`, lowercase, throughout.** Russian has no informal register that suits a product talking to strangers, and `ты`
reads as either an ad for teenagers or rude. The only call is the capital:

- Apple's Russian capitalizes the polite pronoun everywhere (129 `Вы` and 112 `Вам` / `Вас`, zero lowercase, across the
  live Finder, AppKit, and System Settings strings in the pile). That's letter-writing etiquette.
- Microsoft's Russian style guide prescribes lowercase `вы` for UI, and so do Google, Yandex, and the open-source file
  managers. Lowercase reads modern and lighter, which fits Cmdr's voice. Rationale: `decisions.md` § Lowercase вы.

Action labels address nobody: an infinitive, never the imperative (`Скопировать`, not `Скопируйте` or `Скопируй`). Body
text that asks the user to act uses the polite imperative (`Нажмите`, `Откройте`, `Убедитесь, что вы подключены…`; keep
the `вы` there, a bare `Убедитесь, что подключены` drops the subject).

## Register by UI slot

- **Buttons, menu items, commands**: the perfective infinitive, as Finder writes them (`Скопировать`, `Переместить`,
  `Переименовать`, `Извлечь`, `Отменить`). The F-key bar's short labels may be nouns where the infinitive won't fit
  (`Просмотр`).
- **Command descriptions** (palette): an infinitive phrase that completes "this command will…"
  (`Выделить все файлы в текущей папке`).
- **Toggles and checkboxes**: a switch names a state, so use a noun phrase or an imperfective infinitive of a standing
  behavior (`Показывать скрытые файлы`, `Автоматически подключаться снова`). A perfective command makes the switch look
  like a button, with one exception: a switch named after a command or shortcut keeps that name byte for byte
  (`Перейти к последней загрузке`, as in `commands.downloadsGoToLatest.label`).
- **Options in a segmented control or picker**: imperfective for a behavior (`Просматривать` / `Открывать` /
  `Спрашивать`, not `Спросить`); a noun or adjective that agrees with the implied noun for a value (`любой` размер,
  `любая` дата). A bare neuter adjective with no noun (`Умное`) dangles; use an adverb like its siblings (`Оптимально`,
  `Автоматически`) and refer to it as `В режиме «Оптимально» …`.
- **Progress lines**: a verbal noun plus `…` (`Копирование…`, `Удаление {countText} объекта…`). With an insert, use a
  colon (`Подключение: {target}…`).
- **Status chips and cells**: short and agreeing. A mixed column of states may use neuter impersonals (`Подключено`,
  `Недоступно`, `Не проверено`); whether a server column should go all masculine is open (`review-queue.md`). For an app
  server, `Работает`; for an operation, `Выполняется`.
- **AI status lines**: the present tense, first person for the assistant as Russian chat apps do (`Думаю…`), or a noun;
  ❌ never past (`Думал…`), which genders it. Tool lines stay nominal (`Поиск файлов`).
- **Dialog titles**: a question in the infinitive (`Удалить «{name}»?`, `Заменить объект?`) or a short noun phrase.
- **Hints and tooltips**: a full sentence with `Нажмите, чтобы …`; for very short ones `Нажмите для …` is fine.
- **Short labels and chips**: obey the description's length limit. A chip that holds "Regex" can't take 20 characters,
  so chips say `Regex` and `Hex` while prose says `регулярное выражение`; for anything else, check `terms.json` for an
  accepted short form and flag it in `review-queue.md` if there isn't one. Don't abbreviate with a period-cut unless
  Russian UI does (`макс.`, `мин`, `с`).

## Grammar

### Placeholders and case: the neutral-frame toolbox

Russian declines nouns in six cases, and an insert can't. A `{name}` that lands after `в`, `на`, `для`, `к`, `из`, or a
transitive verb shows in the nominative and breaks (`в Загрузки` instead of `в Загрузках`, `Извлечь Флешка`). Cmdr can't
know the value, so build the sentence so the nominative is right. The tools, best first:

1. **A type noun takes the case, the name stays nominative as an appositive**: `в папке {folder}`,
   `на сервере «{server}»`, `приложение {app}`, `Том {volumeName} отключился`, `Убрать папку {folder} из индексации`.
   Use the real type when Cmdr knows it (a server is `сервер`, not `объект`); `объект` only when it could be a file or
   folder.
2. **Quote a user's name and let it stay nominative**, as Finder does (`Переименовать «^0»`,
   `Для перемещения «^0» нажмите…`, `Копирование «^0» приостановлено`). Quotes go around names the user owns or typed
   (files, folders, servers, search queries); never around brands or app names.
3. **A label colon**: a noun, participle, or label, then a colon, then the insert: `Файл: {name}`, `Пропущено: {name}`,
   `Доступ: {localNetwork}`, `Выделено: {countText}`, `Получено: {doneText}`. The word before the colon must not need an
   object: ❌ `Перейти к: {path}`, `Копирование в: {destination}`, `Сочетание клавиш для: {name}`,
   `Нажмите для перехода к: {path}`. Fix those with a different head: `Переход: {buffer}`, `Нет доступа: {path}`,
   `Нажмите, чтобы перейти: {path}`, `Копирование в папку {destination}` (tool 1).
4. **The insert as the sentence subject, in the present tense or with a type noun**: `«{name}» уже существует`,
   `Сервер «{server}» не ответил вовремя` (the type noun carries the masculine; ❌ `«{server}» не ответил`).
5. **A parenthesis**: `(внутри: {ancestor})`, `Порт ({port}) занят`, `в приложении «{systemSettings}»`.
6. **Reorder** so the insert ends the sentence after a neutral word: `По запросу «{label}» найдено # совпадение`, not
   `Найдено # совпадение для {label}`.

A volume name in a Finder-style label is quoted (`Извлечь «{name}»`), so a Cyrillic name stays nominative.

Over-applying the toolbox is its own failure: `Объект {name}` for a known server, `Хост {host}`, and stacked colons read
robotic. A type noun the reader already expects is the lightest fix.

A **closed-set token** Cmdr supplies (`{system_settings}`, `{fullDiskAccess}`, `{localNetwork}`) is known text, but it
still arrives in the nominative. Wrap it in quotes behind a noun (`в приложении «{system_settings}»`,
`разрешение «{localNetwork}»`) rather than inflecting around it.

A **fallback value** is a placeholder too. When a host says `диск {drive}` and the fallback for an unnamed drive is
`этот диск`, the user sees `диск этот диск`. Read every `*.fallbackName` / `*Fallback` / `unnamed*` value against each
host frame: either the host drops its type noun, or the fallback becomes a bare name (`без названия`) that fits behind
one.

### Fragments inside host sentences

Many keys are pieces: an ETA, a unit word, a duration, a fallback, a verb chosen by `select`. The translator sees one
piece; the user sees the assembled line. Before translating a fragment, find where it lands (`@key` description, sibling
keys, a grep for its key in `apps/desktop/src/`) and read the result with real values.

Bugs that shipped this way:

- `indexing.eta.*` started with a capital `Осталось`, but hosts embed them mid-line: `примерно Осталось 2 мин`,
  `95 %, Осталось 8 с`. A fragment that can land mid-line starts lowercase.
- The byte unit word (now `common.sizeUnit.byte`, which carries a plural) was `байты`, shown after every number except
  1: `5 байты`. A unit word after a free number must fit every number: the abbreviation `Б`, or the genitive-plural-safe
  `байт`.
- `settings.network.customTimeoutUnit` was `секунд` after a user-typed number: `1 секунд`. Use `сек.`.
- `servers.paneState.retryTotal*` returned nominative durations into `в течение {duration}`, which needs the genitive.
  Recast the host so the duration fits (`Всего попытки продлятся {duration}`, with accusative branches
  `минуту / минуты / минут`).
- `settings.behavior.textEditorApp.label` `Редактировать файлы в` was followed by the dropdown value
  `Как в системе (TextEdit)`. A label that reads into a value must work with every value; a noun label (`Редактор`) is
  safer (`Редактор файлов`).

### Gender

Russian past tense, short adjectives, and participles agree in gender. Three sources of trouble:

- **The user.** Never let a form agree with the user's gender. Plural `вы` forms are safe (`Вы отменили`); singular
  short forms and hedges are not (`прочитал(а)`, `готов`, `уверен`). Use present (`Вы принимаете условия`), infinitive,
  impersonal (`Удалено`, `Можно`), or first-person plural phrasing the description allows. The terms checkbox is a
  model: `Я принимаю условия… и подтверждаю ознакомление с ними` avoids `прочитал(а)`.
- **Inserts.** A `{name}`, `{server}`, `{app}` has no gender. Put a type noun before it and let the verb agree with the
  noun (`Сервер «{server}» не ответил`, `Приложение {app} еще держит файлы открытыми`), or use the present tense.
- **Products and features.** Cmdr is masculine (`он`, `Cmdr сохранил`, `Cmdr успел`), but prefer the impersonal
  `Cmdr не удалось` or the present tense where they read as well. macOS is feminine (`macOS заблокировала`), following
  `система`. Other Latin product names, `Ask Cmdr` included, take masculine agreement (`Ask Cmdr выключен`) and never
  inflect.

Statuses in one column agree with the one thing they describe, or all use neuter impersonals. A menu item's busy marker
is `(занято)`, appended to the unchanged base label.

### Numerals and ICU plurals

CLDR categories for `ru` (verified with `new Intl.PluralRules('ru')`):

- `one`: 1, 21, 31, 101, 1001… → `файл`
- `few`: 2–4, 22–24, 102–104… → `файла`
- `many`: 0, 5–20, 25–30, 100, 111… → `файлов`
- `other`: fractions (1,5) → `файла` (genitive singular)

Rules:

- Write all four branches every time; `desktop-i18n-plural` fails otherwise.
- `one` is not "exactly one". `{count, plural, one {этот файл} …}` says "этот файл" for 21. For wording that only fits
  1, add `=1` and keep a real `one` branch: `=1 {Пропустить} one {Пропустить все} few {…}`.
- **Everything that agrees goes inside the branch**: the noun, its adjectives, and the verb or participle.
  `Скопирован {countText} файл` / `Скопированы {countText} файла` / `Скопировано {countText} файлов`. ❌
  `скопировано {transferredText} {файл/файла/файлов}` outside the branch gives `скопировано 1 файл`.
- **Case after a preposition flows into the numeral and noun**: `из 21 изображения`, `из 5 изображений`, `в 3 пакетах`,
  `около 2 изображений`. A noun after `из N` takes the genitive in every branch, so the `one` branch is `файла`
  (`fileExplorer.summary.fileNoun` is right to read `one {файла}`).
- **No adjective in front of the numeral**: ❌ `по лишней 21 записи`. Put the adjective after (`21 лишняя запись`), or
  recast.
- **Tense and aspect stay the same across branches**: ❌ `one {остается} … many {осталось}`.
- **No plural param, no agreement**: if the message passes only a formatted `{somethingText}` (a percent, a token count,
  a total) with no integer to select on, don't write a noun after it. Use the counted label colon, where the noun sits
  in the genitive plural BEFORE the number and fits every count (`Файлов: {countText}`,
  `Токенов в памяти чата: {tokens}`, `Выполнено: {percentText} %`), Finder's parenthesis (`объекты ({countText})`), or
  ask for an integer partner in the English source. Shipped bugs: `{percentText} процентов` (`1 процентов`),
  `{tokens} токенов` (`16 384 токенов`).
- Durations, ETAs, and sizes arrive pre-formatted and nominative. Put them where nominative works: after a colon, after
  `осталось`, after `еще`, at the end of a clause.
- `{count} г назад` is wrong after 5+ (`лет`). A short time-ago chip that can't pluralize uses a form that fits all
  counts, or a real plural.

### Verb aspect

- **Buttons and menu items**: perfective infinitive, one completed act (`Скопировать`, `Удалить`, `Подключиться`).
- **Settings, toggles, option labels**: imperfective, a standing behavior (`Спрашивать`, `Показывать`,
  `Подключаться снова автоматически`).
- **Statuses**: a perfective participle for a finished state (`Скопировано`, `Подключен`), imperfective present for an
  ongoing one (`Выполняется`, `Ждет ответа`).
- **Repeated events** are imperfective: `Списки изменений больше не будут показываться`, not `показаны`.
- **Promises about later**: don't overclaim. "A later transfer" is `позже, при одной из следующих передач`, ❌ never
  `при следующей передаче` when the code skips recent files.

### Nominal chains and the genitive pile-up

Russian allows long genitive chains, and English source text invites them. Three genitives in a row, or a name wedged
between a verbal noun and its object, is a sign to recast with a verb clause:

- ❌ `до завершения чтения Cmdr` → `раньше, чем Cmdr закончил чтение`
- ❌ `по мере их обработки Cmdr` → `по мере того, как Cmdr до них доходит`
- ❌ `Открыть шаг первоначальной настройки с полным доступом к диску` →
  `Открыть шаг «Полный доступ к диску» в первоначальной настройке`
- ❌ `предоставление Cmdr этого разрешения, скорее всего, решит проблему` →
  `скорее всего, поможет выдать Cmdr это разрешение`

### Toggle and two-state commands

`Изменить X` loses the two-state meaning ("change your selection" for Toggle selection). Name both states the way macOS
does (`Включить или выключить`, `Выделить или снять выделение`) or use `Переключить` (`Переключить порядок сортировки`).

## Typography

The machine-checked rules live in `mechanics.json` (`pnpm i18n:check-mechanics`); this is the human version.

- **Quotes**: `«…»` primary, `„…“` nested (`«Открыть „{name}“»`). Never ASCII `"…"` or `'…'` in prose. Quote UI labels
  exactly as displayed (`нажмите «Разрешить»`, `в разделе «Настройки > Дополнения»`).
- **Ellipsis**: the single character `…`, hugging the word, no space before (`Копирование…`, `Обзор…`), as macOS ru
  writes it (198 tight label-ending ellipses, zero spaced).
- **Dash**: Russian uses the em dash `—` (тире) where grammar asks for it: an omitted copula
  (`Cmdr — файловый менеджер`), a summarizing dash, a window-title separator (`Системные настройки — %@`). It takes a
  no-break space before and a normal space after, as all 72 macOS ru dashes do. For an English-style aside, prefer a
  colon or parentheses: they read lighter in UI. The en dash `–` is for ranges, tight (`2–3 минуты`); ❌ not a spaced en
  dash as a sentence dash (`Cmdr – Только для личного использования` → `Cmdr — только для личного использования`).
- **Percent**: a no-break space before `%` (`42 %`, `Масштаб 100 %`), as macOS ru writes it (11 no-break, 3 plain, 0
  tight).
- **Numbers**: decimal comma, space-grouped thousands (`16 384`). They come from the formatter; never hand-format a
  number inside a message.
- **Units**: size unit symbols are Cyrillic, as Russian Finder writes them (`МБ`, `ГБ`, `байт`). The file list takes
  them from `common.sizeUnit.*`, so prose and option labels write the same symbols (`100 МБ`, `(МБ)`). Binary (IEC) is
  `КиБ`, `МиБ`, `ГиБ`, `ТиБ`, `ПиБ`; SI (decimal) is `кБ`, `МБ`, `ГБ`, `ТБ`, `ПБ`. English GB/MB stay `ГБ`/`МБ`. Time
  abbreviations are Russian too (`с`, `мин`, `ч`).
- **Letters**: `е` everywhere, ❌ never `ё` (zero `ё` in macOS ru). Where `все` could read as `всё`, recast
  (`Все изображения проиндексированы`, not `Проиндексировано все`). Pure Cyrillic in Russian words: no Latin `a`, `c`,
  `e`, `o`, `p`, `x` look-alikes.
- **After a colon**: lowercase, unless a proper name or a full new sentence follows.
- **Commas**: Russian punctuation, not English. No Oxford comma (`копирование, перемещение и удаление`); subordinate
  clauses always take commas (`Узнать, почему`, `Нажмите, чтобы …`).
- **Capitalization**: sentence case. Proper names keep their capital (`Корзина`, `Системные настройки`, `Терминал`); a
  section name quoted mid-sentence keeps its own case.
- **Apostrophe**: `’`, for the rare foreign name (`O’Reilly`).
- **Keys**: Latin keycaps stay Latin (`Enter`, `Escape`, `Tab`, `⌘F`); the space bar is `Пробел` (`⇧Пробел`), as macOS
  ru names it. Don't mix `Space` and `Пробел` in one catalog.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Finder, Dock, Spotlight, Mission Control, Spaces,
iCloud Drive, TextEdit, Ollama, LM Studio, AlternativeTo, git, `worktree`, plus the `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

- **Latin brands don't inflect.** `в Cmdr`, `для Cmdr`, `у Cmdr`, `с GitHub`, never `Cmdr'а` or `Cmdrа`. When the case
  matters for the reader, put a head noun in front and inflect that (`в приложении Cmdr`, `на сайте GitHub`,
  `в панели Ask Cmdr`). A declined brand also trips `desktop-i18n-dont-translate`.
- **No transliteration** of brands in UI (`Finder`, not `Файндер`). A person's name in running prose may be
  transliterated (`Дэвид`) where the description allows; keep `David` when it's a signature.
- **Apple localizes some names, and Cmdr follows**: `Быстрый просмотр` (Quick Look), `Терминал`, `Просмотр` (Preview),
  `Связка ключей`, `Мониторинг системы`, `Дисковая утилита`, `Программы`, `Корзина`, `Mac с чипом Apple` (Apple
  silicon), `Эмодзи и символы`, `Аккаунт Apple`. Inflect a localized Russian name like any noun (`в Терминале`,
  `в Корзину`). The macOS names live in `terms.json`; check there before keeping something English.
- **Commands to type stay English** in a code span, with the sentence around it Russian: «В Терминале выполните
  `df -h`».
- **Third-party product names** stay as their vendor writes them in Russian docs: `Android SDK Platform Tools` (Google
  keeps it English), `VeraCrypt`, `macFUSE`.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json`: `chosen`, `accept`, `avoid`,
`forms`, a confidence, and sources. Tier order: macOS (Tier 1) → Microsoft (Tier 2) → the file managers (Tier 3), with
one deliberate inversion: two-pane concepts Finder doesn't have (selection, full and brief view, the F-key bar,
directory hotlist) follow Total Commander and Double Commander, the vocabulary Russian two-pane users already know.

Cross-cutting rules:

- **One word per thing, per catalog.** If the catalog already named a surface, reuse that name, and quote a label
  byte-for-byte when another string points at it. Grep the `ru` catalog before coining.
- **Name the list a removal leaves**: `Удалить из избранного`, `Убрать из списка`, `исключить из индексации`. A bare
  `Удалить` in a file manager reads as deleting files.
- **`Просмотр` is one word for three English ones** (the F3 viewer, View the command, Preview the app). Disambiguate in
  prose with `окно просмотра`; the View menu is Apple's `Вид`.
- **`загрузка` covers download and load** in Russian macOS. Scope it with its object (`загрузка обновления`,
  `загрузка модели в память`); prose may say `скачать` where both senses meet.
- **Names stay put, prose breathes.** A ruling locks names (menu items, settings, buttons); prose may use a
  `proseAccept` synonym (`сетевая папка` for a share).

## Pitfalls seen in this catalog

A six-reviewer audit graded the first full Russian catalog 7/10: clean mechanics, well-researched macOS names, and
near-flawless ICU plurals, held back by these recurring patterns. Check every batch against them.

1. **Composition blind spots**: a fragment correct alone and broken in its host (ETA capitals, `диск этот диск`,
   `Устройство Целевое устройство`, `Подключение тома общий ресурс…`, `5 байты`). See § Fragments.
2. **The colon hack after a preposition or verb** (`к: {path}`, `в: {destination}`, `для: {name}`). See the toolbox.
3. **Bare inserts in inflected slots** where the toolbox wasn't applied (`в {ancestor}`, `на {counterpart}`,
   `для {label}`). Fine for `Documents`, broken for `Загрузки`.
4. **Words outside the plural branch**: verbs, participles, and fixed nouns written once (`скопировано 1 файл`,
   `{percentText} процентов`).
5. **Канцелярит**: nominal chains, `для + noun`, `не являются`, `Повторите попытку через некоторое время`, robotic
   status nouns (`Работа`, `Нет продвижения в течение …`).
6. **Warmth dropped**: lost apologies, lost first-person lines, upbeat tips turned into notices.
7. **«ошибка» in error copy**: `Произошла ошибка`, `Ошибка провайдера ИИ`, `ошибок: {failedText}`.
8. **Contradictions**: `Объект не изменен: {name}. После записи Cmdr он был изменен.` A "left X alone" line says
   `Оставлено как есть`, never `Без изменений`, when the next sentence says the item changed.
9. **Meaning slips on small words**: `часть выбранных объектов` when the count can be all of them; `Недостаточно места`
   where the English hedges with "may"; `при следующей передаче` where the English says "a later one"; busy folders
   (`активные`, not `часто используемые`); git dirty (`незафиксированные`, not `несохраненные`).
10. **Ignored length limits**: `Регулярное выражение` in a chip, `(поврежденная символическая ссылка)` in the date
    column (`(битая ссылка)`), `Вернуться назад` (a pleonasm: `Назад`).
11. **A label that doesn't match its target**: suggestions that point at «Технические подробности» while the disclosure
    says something else; dialog names quoted differently from the dialog's own title; `Настройки > Обновления` where the
    section is `Обновления и конфиденциальность`.
12. **Untranslated keys**: a `<<MISSING>>` or English value in `ru` falls back silently. Grep for Latin-only values
    after every batch.

Good models from the same catalog: `errors.write.destinationNotAFolder.title` (`Мешает файл`),
`errors.volume.passwordRejected` (`Пароль не подошел`), `settings.network.localNetworkAccessLabel`
(`Доступ: {localNetwork}`), `goToPath.toast.landedOnAncestor` (type nouns before each path), `operationLog.summary.*`
(participle agreement in every branch), `viewer.selection.singleLine` (`В строке {line} выделено символов: {chars}`).

## Decision points

- **Script**: Cyrillic only, no transliteration in UI. Guard against Latin look-alike letters inside Russian words.
  Confidence: high.
- **Regional variant**: one `ru`. Russian has no regional UI split; Apple, Microsoft, and Google ship one Russian.
  Confidence: high.
- **`е` over `ё`**: macOS ru writes zero `ё`; Microsoft and the file managers mostly agree. Recast an ambiguous `все` /
  `всё` instead. Confidence: high. `decisions.md` § `е` over `ё`.
- **Gender**: Russian has no accepted gender-neutral morphology, and Apple, Microsoft, and Google all avoid the problem
  structurally. So do we: no hedges, no slashes, rephrase. Confidence: high.
- **Register**: lowercase `вы`, everyday wording, and Cmdr's warmth over the neutral impersonal Russian UI tends toward.
  Confidence: high; a native reviewer may tune individual lines.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry.
- `terms.json`: this locale's ruling per concept, with the keys that legitimately deviate under `exceptions`.
- `decisions.md`: distilled rulings ("X over Y because Z"), one section per topic, headings citing their keys.
- `mechanics.json`: quotes, ellipsis, spacing, and hedge patterns, checked by `pnpm i18n:check-mechanics`.
- `review-queue.md`: open questions for a native reviewer. Not translator input.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line.
