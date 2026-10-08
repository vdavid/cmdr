# ru review queue

Open questions only a native reviewer can settle. Each ships a reasoned value today; none blocks shipping. Remove an
item once settled, and record the outcome in `terms.json` or `decisions.md`.

- **AI provider** (`settings.ai.provider.label`, `settings.section.aiProvider`): is `поставщик ИИ` natural to Russian AI
  users, or has `провайдер` won in this niche (Yandex Cloud and most dev blogs say `провайдер`)? The ruling follows MS
  and macOS.
- **Advanced** (`settings.section.advanced`): Apple's tab name `Дополнения` can also read as add-ons. Does it mislead in
  Cmdr's Settings sidebar?
- **Network share** (`commands.shareSelectShare.label`, `settings.section.smbNetworkShares`): does `общая папка` read
  naturally to SMB users used to Windows's `общий ресурс`, or does it sound like an iCloud shared folder?
- **Mount folded into connect** (`errors.listing.authRequiredEauth.suggestion`): `подключить` covers both; is
  `Отключите и снова подключите том` clear where both senses meet?
- **Click verb** (`settings.behavior.doubleClickPaneNavigatesToParent.label`): `нажать` / `дважды нажать` follows macOS;
  do Russian Mac users expect `щелкнуть` / `двойной щелчок` (MS and every file manager) for a mouse click?
- **SI kilobyte** (`common.sizeUnit.kilobyte`): `кБ` follows SI and GOST 8.417 (uppercase `Кбайт` = 1024), but Finder
  writes `КБ` for its own decimal sizes. Does `кБ` read as a deliberate SI symbol to a Russian user, or as a typo?
- **Items unit** (`settings.control.unitItems`): `шт.` after a typed buffer size (`200 шт.`) fits every number but drops
  the word `объект`; is it clear in Settings > Дополнения?
- **F4 label** (`fileExplorer.functionKeyBar.editLabel`): `Редактировать` is long for the F-key bar; is TC's `Правка`
  acceptable? (`Regex` and `Hex` are settled in `decisions.md`.)
- **Download wording** (`ai.toast.downloadingTitle`, `ai.local.downloadModel`): macOS uses `загрузка` for both download
  and load; should model downloads say `Скачать` / `Скачивание` everywhere to keep them apart from loading into memory?
- **AI status line** (`askCmdr.thinking`): first-person `Думаю…` (as Russian AI chat apps write it) vs `Думает…` or the
  termbase's `Размышление…`.
- **Remembered selections** (`selection.recent.*`): `недавние выделения` sounds stiff; is `последние выделения` or
  `недавние запросы выделения` better for remembered Select-files queries?
- **Stalled transfer** (`fileOperations.transferProgress.stallUnknown`): `замерла` vs `не продвигается` vs
  `застопорилась`.
- **Tail toggle** (`viewer.toolbar.tail.label`): does `Следить` read as follow-the-end, or is `Следить за концом`
  clearer?
- **Signed-out status** (`servers.hub.status.signedOut`, `servers.paneState.signedOut`): `Сеанс завершен` is neutral but
  doesn't say "sign in again". Is `Требуется вход` better?
- **Untested coinages** (`settings.appearance.dateColors.label`, `settings.listing.stripedRows.label`,
  `settings.fileExplorer.git.showRepoChip.label`, `errors.git.orphanedWorktree.title`): `увядание` (date wilting),
  `чередование фона строк`, `плашка` for the repo chip, and `рабочее дерево` vs `рабочая копия` have no source. Better
  everyday words?
- **Git terms** (`fileExplorer.git.*`): `коммит` and `ветка` (developer norm) over MS's `фиксация` and `ветвь`; the
  GitHub `issue` kept English. Confirm with a Russian developer.
- **Enter key** (`selection.runHint`): the Latin keycap `Enter` vs Apple's spoken `Ввод`.
- **Watcher** (`settings.fileSystemWatching.*`): `наблюдение за файлами` vs TC's `слежение за каталогами`.
- **USB debugging** (`settings.fileOperations.adbEnabled.description`): confirm `Отладка по USB` on a Russian-language
  Android phone (the pile has no Android source).
- **Mixed gender for products** (`errors.provider.*`): Cmdr masculine (`Cmdr попытался`) and macOS feminine
  (`macOS заблокировала`) are each consistent; does either grate?
- **Server status column** (`servers.hub.status.connected`, `servers.hub.status.saved`): `Подключено` (the connect
  ruling's state form, shared with `ai.cloud.connected`) sits beside masculine `Сохранен` / `Найден поблизости`. Go all
  masculine for server rows, or neuter everywhere?
- **Deselect all** (`menu.select.deselectAll`, `commands.selectionDeselectAll.label`): `Снять выделение со всех` follows
  the ruling, but TC and DC say plain `Снять выделение`. Shorter reads more natural in a menu.
- **Network switch** (`settings.network.enabled.label`): `Включить сетевые функции` keeps the `enable` ruling but reads
  like a button on a switch; `Сетевые функции` reads better.
- **Smart size display** (`settings.listing.sizeDisplay.opt.smart`): `Оптимально`, an adverb like its sibling; a better
  word for "Smart"?
- **Short badges and long labels** (`settings.indexing.overriddenBadge`, `settings.behavior.openTerminalHereApp.label`,
  `settings.control.customPrefix`): `Выкл. с индексацией дисков` is abbreviated to fit; the other two may overflow their
  rows.
- **Insufficient space** (`errors.write.insufficientSpace.title`): `Возможно, в целевом месте не хватит места` repeats
  `место… места`; is there a shorter form that keeps the hedge?
- **Counter frames** (`errors.write.trashRefused.message.*`, `errors.eject.unmountRefusedByProcesses`):
  `(объектов: {count})` with 1, and `процессы ({countText}): {processes}` after `Не удалось извлечь X:`. Do they read
  well?
- **worktree gender** (`fileExplorer.git.size.linkedWorktrees`, `errors.git.orphanedWorktree.*`): masculine
  `связанный worktree`, and checkout as Pro Git's `Извлечена ветка` (`fileExplorer.git.tooltip.worktreeOnBranch`) vs the
  everyday `переключена на ветку`.
- **Idle wait** (`indexing.enrich.pausedIdle`): `Ждет паузы в вашей активности` is stiff; `Ждет, пока вы освободитесь`
  is warmer.
- **Month presets and years ago** (`queryUi.date.preset.thisMonth`, `queryUi.age.years`): `{month}: 1-е число` and
  `{count} г. назад` are workarounds until the source passes better values (`source-queue.md`).
- **Onboarding checklist** (`onboarding.stepBeta.checklist.title`, `onboarding.stepBeta.checklist.alternativeTo`): the
  title satisfies two rulings but is stiffer than the English; `Поставить лайк` is colloquial over the clunky
  `отметку «Нравится»`. Warmer options?
- **Nested quotes in a title** (`main.revealActivation.title`): `Команда «Показать в Finder» открыла папку здесь` reads
  well?
- **Fixed shortcut badge** (`shortcuts.section.fixedBadge`): `Закреплено` may read as Dock pinning; `Встроено` would
  match `shortcuts.section.fixedTooltip`.
- **S3 terms** (`servers.sheet.s3Bucket`, `servers.sheet.accessKeyId`, `commands.fileCopyShareLink*`,
  `fileExplorer.archivedFile.label`): `бакет`, `идентификатор ключа доступа`, `публичная ссылка`, and
  `В архивном хранилище` are tentative, sourced from Yandex Cloud and AWS's Russian docs outside the pile. Do they match
  what Russian S3 users see?
- **Google's tab name** (`servers.sheet.s3GcsKeyHelp`): kept `«Interoperability»` in Latin; what does Google Cloud's
  Russian console call that Cloud Storage settings tab?
- **Multi-Rename labels** (`multiRename.counterStart`, `multiRename.counterDigits`, `multiRename.removeDiacritics`,
  `multiRename.insertPlaceholder`): `Начало счетчика`, `Число цифр` (TC writes `Начать с:` / `Цифр:` inside a «Параметры
  счетчика» group Cmdr lacks), the noun phrase `Удаление диакритических знаков` shared by the checkbox and the preset,
  and `обозначение` for the [N]/[C] tokens. Natural for TC users?
- **Hidden rows** (`multiRename.hiddenProblems`, `multiRename.moreRows`): «за пределами показанных строк» and «Еще #
  файл не показан» both mean past the 1,000 shown rows. Clear enough?
