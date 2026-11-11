# uk decisions

The distilled rulings behind this language's term choices (`terms.json`) and per-feature translation choices, one
section per decision: "X over Y because Z", at most ~3 lines. When a ruling changes, edit or replace its section and
delete what it supersedes; never append a narrative or a dated story. A byte budget holds the file to that
(`i18n-termbase`); bump it when new decisions need room. Translators don't read it whole: `pnpm i18n:brief` pulls the
sections whose heading cites a key in the batch. So every heading cites the keys it's about, in backticks
(`` `servers.sheet.remember` ``, `` `servers.*` ``, `` `a.b.label`/`.description` ``), and a ruling links its section
through the term's `decision` field. Format: `docs/i18n/termbase.md` § `decisions.md` headings.

## Product and names

### David stays «David» (`onboarding.stepBeta.greeting`, `licensing.acknowledgements.signature`, `securityNotices.updateNotGenuine.body`)

Lead call: Latin «David», untransliterated (not «Давід» / «Давид»), as the `@key` notes ask. Dative and other cases stay
the bare name («напишіть про це David»).

### Selection is вибір (`*elect*`, `selection.recent.*`, `settings.selection.recentSelections.*`)

Lead call: вибір / вибрати over виділення, matching Finder («Скасувати вибір усього», «Інвертувати вибір», «Скасувати
вибір файлів…»). Remembered selections are «шаблони вибору», since plural «вибори» reads as elections. Cursor moves name
the file («Перейти до попереднього файлу»).

### About = «Про Cmdr» (`licensing.about.srTitle`, `menu.app.about`, `commands.appAbout.label`)

Over «Про програму Cmdr»: the same English must stay byte-identical across the menu, command, and dialog title.

## Grammar patterns

### `=1` arms for single-item wording (`fileOperations.transferDialog.policy*`, `conflictsSummary`, `mergeQuestion`, `suggestedOps.reversible.*`, `settings.mediaIndex.reclaim.button`, `titleReversal*`)

CLDR `one` also covers 21, 31…, so singular wording («Пропустити», «зайвий запис», «файлу») lives in `=1`; `one` takes
the plural/"all" wording. Count-only pronouns become a noun («елемент(и)», «назва(и)»).

### Counts without a plural param use a counted-label colon (`errors.write.*`, `errors.eject.unmountRefusedByProcesses`, `queue.chip.ariaLabel`, `menu.edit.undoTrashMany`)

«(кількість: {count})», «Файлів, …: {count}», «відсоток виконання: {percentText}» over a numeral + noun, which can't
agree (21, 22, 25 each need a different ending). With a plural param, decline the noun in it
(`indexing.enrich.progress`: genitive after «з», `few` too).

### Inserts that can't decline take a type noun, quotes, or a colon (`errors.mount.*`, `errors.shareList.*`, `errors.write.*`, `fileExplorer.navigation.forget*`, `driveIndex.*`, `queue.row.itemsInFolder`, `downloads.toast.inSubdir`)

«сервер «{server}»», «спільна папка «{share}»», «диск «{name}»», «у папці «{folder}»». Where the insert may be a phone,
drive, or server, the quoted name stands alone in an impersonal frame («{name}» зараз не підʼєднано»).
`{system_settings}` paths and other OS tokens follow a colon (`errors.listing.*`,
`fileExplorer.restrictedFolder.tooltip`).

### Fallback inserts stay nominative (`search.coverage.line.uncovered.*`, `walk.interrupted`, `errors.write.readOnlyDevice.*`)

`{drive}` can be «цей диск» and `{deviceName}` «Місце призначення», so they take an accusative object or subject slot,
or a colon frame («{deviceName}: тільки для читання»), never a locative.

### Accusative count parts (`fileOperations.delete.foldersPart`, `fileOperations.transferDialog.foldersPart`, `transfer.movedPhrase`)

They're the object of the title verb or a `-но` participle, so folders read «1 папку / 2 папки / 5 папок». The
term-consistency split against the nominative keys is grammar, not drift.

### Genitive count parts (`fileExplorer.summary.fileNoun`/`.dirNoun`, `queue.row.itemCount`/`.folderCount`/`.fileCount`, `askCmdr.decision.approved`)

They follow «з» or a verbal noun («Копіювання 2 папок»), so one/few/many/other =
«елемента/елементів/елементів/елемента».

### Ungendered user (`onboarding.stepBeta.terms.consent`, `servers.hostKey.changedDisclosure`, `servers.certificate.changedDisclosure`, `settings.askCmdr.toolGroupsOnAnswer.opt.keep`, `settings.mediaIndex.*.description`)

Past tense genders the speaker, so: gerund «Прочитавши, я погоджуюся» (Apple Setup Assistant), «Ключ перевірено»,
«Залишати як є», «позначені користувачем». Cmdr takes masculine past («Cmdr зупинився»); ШІ is masculine too.

### Ask Cmdr tool lines (`askCmdr.tool.*`)

Doing = verbal noun («Перегляд папки»), done = impersonal `-но/-то` («Переглянуто папку»). Search keeps the noun plus
«завершено»; tried lines start «Спроба …». Its notes are «нотатки про вас». Folder-count arms: `=1` «у папці», the rest
«у папках».

### Decision verbs are accusative verbal nouns (`askCmdr.decision.verb*`)

«переміщення», «переміщення в Смітник», «додавання сервера», «зміну параметрів сервера», after «Ви схвалили /
відхилили».

## Errors

### Listing-error frames (`errors.listing.*.suggestion`)

«Що можна спробувати:», «Знову перейдіть сюди, щоб спробувати ще раз», «У Терміналі виконайте `…`, щоб…», «Перевірте
підʼєднання до мережі». Typed keys copy their errno twins byte for byte. First Aid is named, not inflected: «програму
**Дискова утиліта** … натисніть **Швидка допомога**».

### Provider frames (`errors.provider.*`)

«Цією папкою керує **{name}**» keeps the insert a nominative subject; `{app}` follows «програма», `{name}` in
needs-action/serious lines follows «сервіс».

### Git vocabulary from git's uk.po (`errors.git.*`, `fileExplorer.git.*`)

«неглибокий клон», «робоче дерево», «обʼєкт git», «файли пакунків», «індекс git». «Bare-репозиторій» over uk.po's
«порожнє сховище», which reads as "empty".

### Cold storage (`errors.listing.coldStorage.*`, `viewer.error.coldStorage`, `fileExplorer.archivedFile.tooltip`)

«в архівному сховищі», so "archived" never reads as a zip archive.

### Eject reasons start capitalized (`errors.eject.*`, `fileExplorer.rename.needsOwnMoveDialog`)

Over the usual uk lowercase after a colon: the reasons can be two sentences and MCP reads them standalone. Holders:
`X досі щось там використовує.`, `Закрийте цю програму` / `ці програми`. Eject all → `Вийняти всі` (Finder `N199.1`).

### Raw-error suggestion with technical details (`errors.write.writeError.suggestion`, `readError.suggestion`, `ioError.suggestion`)

«…перевірте розділ «Технічні деталі» нижче», quoting the disclosure label as `errors.listing.*` does.

## File list and operations

### Type-to-jump «Перехід», quick filter «Фільтр» (`fileExplorer.typeToJump.prefix`, `fileExplorer.quickFilter.*`, `settings.fileExplorer.typeToJump.*`, `downloads.toast.summary*`)

«Перехід» over the calque «Стрибок» and TC's «Швидкий пошук» (collides with Search). The download toasts reuse «Перехід:
{key} у програмі, {key} глобально».

### Column headers use Get Info's short forms (`fileExplorer.columns.modified`/`.created`/`.added`, `suggestedOps.columnModified`)

«Змінено / Створено / Додано» (Finder Get Info) over «Дата змінення», too wide for a column; "Last modified" is
«Востаннє змінено». `<DIR>` is «<папка>» / «ПАПКА» (`fileExplorer.dirSize.dirPlaceholder`,
`fileExplorer.selectionInfo.dir`).

### Favorite in prose (`fileExplorer.navigation.*avorite*`)

Headings say «Улюблені» (Finder); one favorite in a sentence is «улюблена папка» / «серед улюблених», since a bare
«улюблене» reads as a liked thing.

### Compress with counts (`fileOperations.transferDialog.titleWithCounts`, `suggestedOps.approve.compress`)

«Стиснути {phrase}» over «Створити архів», which can't take the counted files as object; verb-only titles and buttons
(`suggestedOps.action.compress`) keep Finder's «Створити архів».

### Put Back, move back, rollback (`fileOperations.trash.*`, `fileOperations.transferProgress.titleReversal*`, `rollback*`, `askCmdr.renameUndo.*`, `fileOperations.cancelRollback.reason.*`)

«Повернути назад» is the Trash's Put Back; moving back is «Повернення … на місце»; renaming back «Повернення старих
назв». Rollback: prose «відкат», title «Відкочування…», button «Відкотити». Skip reasons: «Залишено як є: {name}. …».
"No longer in the Trash" is «зник(ли) зі Смітника», which agrees after any count.

### Existing / incoming (`fileOperations.transferProgress.existing*Label`, `new*Label`)

«Наявний» over «Існуючий» (macOS FinderKit, Font Book); same-type labels name the noun («Наявний елемент:»).

### ETA fragments take «ще» (`indexing.eta.*`)

«ще 3 хв» over «залишилося 3 хв»: it fits after «приблизно», «Загалом:», or «95 %,» and stands alone.

## Menus, settings, and dialogs

### Menu bar copies Finder (`menu.bar.*`, `menu.view.sortBy`, `menu.window.zoom`, `menu.app.services`, `menu.file.view`)

Файл, Редагування, Перегляд, Перейти, Вікно, Довідка, Сортування, Оптимізувати, Сервіси. F3 View is the verb
«Переглянути», since «Перегляд» is the menu title. Go to folder is «Перейти до папки» in the dialog title too
(`goToPath.dialog.title`, per its `@key`).

### Finder tag colors are quoted names (`commands.tagsToggle*.label`)

«Перемкнути тег «Бузковий»» over an agreeing adjective; colors from AppKit: Червоний, Оранжевий, Жовтий, Зелений, Синій,
Бузковий, Сірий.

### Options agree with their label (`settings.appearance.*.opt.system`, `settings.appearance.uiDensity.opt.*`, `settings.theme.mode.opt.*`, `settings.behavior.textEditorApp.systemDefault*`, `queryUi.*.comparator.any`, `queryUi.*.customCell`, `settings.managed.summary.*Off`)

«типовий» needs a noun: «Типова мова системи», «Типовий системний», «Типова програма». Density is adverbs (Компактно /
Зручно / Просторо); theme «Світлий / Темний / Системний»; size vs date cells «будь-який / власний…» vs «будь-коли /
власна…»; switched-off rows «Вимкнено». Splits are allowlisted.

### Size display "Smart" = «Оптимальний» (`settings.listing.sizeDisplay.opt.smart`)

Over «Динамічний» (macOS's Smart, collides with sizeUnit «Динамічна») and «Розумний» (not macOS's word).

### Dropdown labels are nouns (`settings.behavior.textEditorApp.label`, `settings.behavior.openTerminalHereApp.label`)

«Програма для редагування файлів», «Термінал для «Відкрити термінал тут»» over "Edit files in [X]", which would need a
locative on the value.

### Settings vocabulary (`settings.section.listing`, `settings.operationLog.enabled.label`, `settings.mediaIndex.parallelism.*`, `settings.mcp.portInUse*`, `settings.network.customTimeoutUnit`, `shortcuts.system.appWindows`)

«Список файлів» (display options, not reading a folder); «операції з файлами» over «файлові операції» (catalog 7:0);
«Паралельні потоки»; «уже використовується»; unit «с» after a free number; «Вікна програми» (KeyboardSettings).

### Placeholders and wizard (`common.attachEmailPlaceholder`, `settings.updates.emailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `onboarding.wizard.finish`, `onboarding.stepAi.table.row*`)

Email samples are «name@example.com» everywhere. Wizard Finish «Готово» (Microsoft) avoids quit's «Завершити». Table
rows are nouns («Пошук», «Вибір») under «Функція».

### Org fallback is genitive (`licensing.about.fallbackOrg`, `licensing.expiration.orgName`)

«вашої організації», since every host has «для {org}»; `orgName` uses «Власник ліцензії: {org}» so real names stay
nominative.

### Rename tool (`multiRename.counterEditor.start`, `multiRename.rangeEditor.*`)

«Початок» for both Start labels (nouns, per the descriptions). Ordinals agree with «символ»: «3-й», «3-го», «з кінця»;
`=1` «останній».

### Viewer modes (`viewer.toolbar.viewMode.*`, `viewer.toolbar.tail.label`, `commands.viewerViewAs*.label`, `commands.viewerToggleTail.label`)

«Двійкові дані» over «Бінарний» (catalog already says «двійкові дані»; mode matches its command); «Шістнадцятковий
вигляд»; tail is «Стеження» / «режим стеження».

### Queue window and Servers list are quoted (`queue.failureToast.action`, `queue.handoffToast.action`, `queue.chip.*`, `servers.pinHint.*`, `servers.sheet.addedToast`)

«у вікні «Черга операцій»», «у списку «Сервери»», «група «Мережа»», so the names never inflect.

### Date presets (`queryUi.date.preset.*`)

Intl month and weekday names stay nominative; a `{date}` arrives declined («1 травня»), so «{date}, {time}»; `yearStart`
writes «1 січня».

### Drop hint (`askCmdr.composer.dropHint`)

«Відпустіть» over «Перетягніть»: the hint shows mid-drag.
