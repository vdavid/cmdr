# ru decisions

Distilled rulings behind `terms.json`: "X over Y because Z", one entry per topic, edited in place. `pnpm i18n:brief`
pulls a section when its heading cites a batch key, so every heading cites its keys in backticks. Term rulings live in
`terms.json`, voice and typography in `style.md`, open questions in `review-queue.md`.

## Lowercase вы

Lowercase `вы` / `вас` / `ваш` over Apple's capitalized `Вы` (129 `Вы`, 112 `Вам` / `Вас`, zero lowercase in the macOS
ru pile): the capital is letter-writing etiquette; Microsoft's Russian style guide, Google, and Yandex write lowercase,
and it fits Cmdr's lighter voice.

## `е` over `ё`

`е` everywhere: live macOS 27 ru writes zero `ё`, and a mixed catalog looks careless. Where `все` / `всё` is ambiguous,
recast the sentence (`Все изображения проиндексированы`).

## Selection: выделение over выбор (`menu.bar.select`, `menu.select.*`, `menu.context.selection`, `menu.context.toggleSelection`, `commands.selection*`, `selection.*`, `fileExplorer.selectionInfo.*`, `settings.selection.*`)

Product-owner call: marking files is Total Commander's `выделение` / `Выделить` / `Снять выделение` /
`Инвертировать выделение` (TC/DC `Снять выделение` ×9 vs macOS `Отменить выбор` ×2 in the pile; Nautilus, Dolphin, MS
agree). Picking an option, volume, or server stays `выбрать`; cursor moves name the file (`commands.navDown.label`
`Следующий файл`).

## Ticking queue rows: выбрать (`queue.row.selectAria`, `queue.toolbar.cancelSelected`, `queue.toolbar.selectedCount`)

Checkbox rows in the operation queue are choosing list items, not marking files in a pane: `Выбрать эту операцию`,
`Выбрано: #`.

## Remembered selections (`selection.recent.*`, `settings.selection.recentSelections.maxCount.label`/`.description`)

`недавние выделения` over `недавние выборы` / `выборки`: `выборов` reads as "elections" and `выборка` as a database
sample. Quote the dialogs by their titles (`selection.dialog.title.add`/`.remove`).

## Terminal: Терминал over Terminal (`errors.listing.*.suggestion`, `mtp.ptpcameradDialog.*`, `commands.handler.openTerminalHere.*`, `errors.provider.macFuse.serious`, `errors.git.gitDirPermissionDenied.suggestion`)

Product-owner call: live macOS 27 Terminal.app is `Терминал` (Finder N67 `Открыть в Терминале`), inflected and unquoted
(`В Терминале выполните …`). The commands themselves stay English in code spans.

## Ask Cmdr stays verbatim (`askCmdr.title`, `menu.view.askCmdr`, `settings.section.askCmdr`, `commands.askCmdrToggle.label`, `askCmdr.gate.off.*`, `askCmdr.error.askCmdrOff`, `ai.cloudConsent.askCmdr.title`)

`Ask Cmdr` over `Спросить Cmdr`: a product name, verbatim in every locale, and a Russian verb blurs into the
ask-question verb. Like other Latin product names it takes masculine agreement (`Ask Cmdr выключен`) and never inflects;
a head noun carries the case where one is needed (`в настройках Ask Cmdr`, `панель Ask Cmdr`).

## Full Disk Access: row label vs prose (`onboarding.stepFda.*`, `downloads.fda.message`, `common.downloadsFdaHint`, `search.coverage.setUpFullDiskAccess`, `onboarding.fdaBadge.*`, `askCmdr.wake.needsFullDiskAccess`)

Where the user must find the Privacy row, quote it as macOS 27 shows it: `Доступ к диску` (SecurityPrivacyExtension
`ALL_FILES`, 26A428). Prose says `полный доступ к диску` (Apple's own ALL_FILES_QUIT_* prose and search terms). ❌ Not
`Доступ ко всему диску`: an older macOS label.

## Quit & Reopen: Перезапустить (`onboarding.stepFda.step3`)

Quote `«Перезапустить»`, the Russian TCC button (SecurityPrivacyExtension `QUIT_APP`, macOS 27 26A428); the
back-translated `Завершить и открыть заново` sends the user hunting for a button that doesn't exist.

## Click and double-click: нажать over щелкнуть (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`, `fileExplorer.network.browser.tooltip.*`, `viewer.binaryWarning.body`, `fileExplorer.breadcrumb.navigateTooltip`)

macOS says `нажать` (Finder MR20 `Нажмите «Отменить»`, AppKit `Нажать для выбора`; zero `Щелкните` in the pile), so
click → `нажмите`, double-click → `дважды нажмите` / `двойное нажатие`. MS and the file managers' `щелкнуть` /
`двойной щелчок` still pass the check but aren't the default; one verb per catalog.

## Full and brief view: Подробный / Краткий режим (`menu.view.fullView`, `menu.view.briefView`, `commands.viewFullMode.label`, `commands.viewBriefMode.label`, `shortcuts.scope.fullMode`, `settings.appearance.card.briefMode`, `settings.listing.stripedRows.description`)

`Подробный режим` over `Полный режим`: Total Commander pairs 301 `Краткий режим` with 302 `Подробный режим`, DC says
`Подробный`, and `полный` reads as "complete". Command labels name the mode (`Подробный режим`), not `Переключить на …`.

## No «ошибка» in error copy (`askCmdr.error.provider`, `ai.cloud.genericError`, `onboarding.cloudSetup.status.genericError`, `licensing.error.generic`, `ai.translateError.serverError.title`, `askCmdr.decision.result`, `fileExplorer.navigation.driveIndex.tooltipFailed`, `fileExplorer.network.browser.status.error`, `fileExplorer.navigation.driveIndex.refusedInternal`)

`Что-то пошло не так` (Nautilus) over `Произошла ошибка`; `возникла проблема`, `не выполнено: {failedText}`, a status
cell `Проблема`, "hit a snag" `Что-то помешало Cmdr…`. `Не удалось` stays where the English says "Couldn't" (macOS ru's
rendering). Compounds (`отчет об ошибке`) are fine.

## Cmdr couldn't: Cmdr не удалось (`fileExplorer.navigation.*RefusedToast`, `fileExplorer.navigation.driveIndex.refusedUpgradeFailed`, `fileExplorer.edit.launchRefused`, `errors.*`)

"Cmdr couldn't X" → `Cmdr не удалось X` (35 keys), over `Cmdr не смог X` (13): the impersonal reads as macOS's own
`Finder не удается` and genders nothing. Where the sentence already has Cmdr acting in the past, masculine is fine
(`Cmdr успел`).

## Try again: button question vs suggestion (`viewer.error.readFailed`, `viewer.copy.*`, `viewer.saveAs.*`, `operationLog.dialog.loadError`)

Beside a Retry button, "Try again?" → `Попробовать еще раз?` (the infinitive question points at the button); with no
button, `Попробуйте еще раз.` / `Попробуйте еще раз чуть позже.` ❌ Not the stock
`Повторите попытку через некоторое время`.

## AI provider: поставщик over провайдер (`settings.ai.provider.label`, `settings.section.aiProvider`, `settings.ai.providerAria`, `askCmdr.error.*`, `ai.translateError.*`, `onboarding.stepAi.cloud.pickerTitle`, `settings.askCmdr.provider.*`)

MS keeps `провайдер` for an internet provider and says `поставщик (услуг)` for a company providing a service; macOS Home
and Font Book agree. de, hu, and sv took MS's word too. A vendor's billing page is `личный кабинет`.

## Advanced: Дополнения (`settings.section.advanced`, `servers.sheet.advanced`, `settings.advanced.resetAllConfirm`/`.resetAllConfirmTitle`, `ai.cloudConsent.logsNote`, `main.escapeFullScreenHint.settingsLine`)

Every Apple settings tab and sheet group says `Дополнения` (Finder 261.label, Mail, VPN); `Расширенные` is the Windows
form, and `Дополнительно` is an adverb. Prose and confirm dialogs say `дополнительные настройки`.

## Delete permanently: безвозвратно (`menu.file.deletePermanently`, `commands.fileDeletePermanently.label`, `fileExplorer.functionKeyBar.permanentlyLabel`/`.deletePermanentlyAction`)

`Удалить безвозвратно` over Finder's `Удалить немедленно`: Finder's Delete Immediately stresses timing, Cmdr's English
says permanently, and the F-key bar's one word (`Безвозвратно`) must match the command.

## Custom: Пользовательский / Настроить… (`settings.control.customOption`, `settings.control.customPrefix`, `settings.network.timeoutMode.opt.custom`, `settings.appearance.dateTimeFormat.opt.custom`, `queryUi.*.customCell`)

macOS 27 renders Custom as `Настроить` 52× and `Пользовательский` 21×, `Свой` never. An option that opens an editor is
`Настроить…`; a stored custom value is `пользовательский`.

## Apple silicon: Mac с чипом Apple (`settings.mediaIndex.clip.notSupported`, `settings.ai.tooltipLocalDisabled`, `onboarding.stepAi.localTooltip`, `ai.local.notInstalled`, `settings.mediaIndex.parallelism.description`)

Apple localizes it (iPhone Mirroring and Rosetta strings): `Нужен Mac с чипом Apple`, not `Требуется Apple silicon`.

## Option labels and toggles: standing behavior (`settings.archives.opt.*`, `settings.behavior.fileSystemWatching.downloadsNotifications.opt.neither`, `settings.listing.sizeDisplay.opt.smart`)

An option names a standing behavior, so imperfective: `Просматривать` / `Открывать` / `Спрашивать` (Mail, DC);
`Открывать` differs from the `Открыть` command on purpose. `Выключено` over the dangling `Ни один`; Smart →
`Оптимально`, an adverb like its sibling `Автоматически`.

## A switch named after a command keeps its name (`settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.label`, `settings.fileSystemWatching.globalShortcutHint`)

The switch names the shortcut, so it matches `commands.downloadsGoToLatest.label` byte for byte
(`Перейти к последней загрузке`), even though an infinitive on a switch reads a little like a button.

## A label continued by a dropdown becomes a noun (`settings.behavior.textEditorApp.label`, `settings.behavior.openTerminalHereApp.label`)

`Редактор файлов`, `Терминал для команды «Открыть терминал здесь»`: a verb label read into the value breaks once the
dropdown shows `Как в системе (…)` (`Редактировать файлы в Как в системе (TextEdit)`).

## Host: хост over узел (`commands.networkSelectHost.label`, `commands.networkRefresh.label`, `commands.shareBack.label`, `fileExplorer.network.browser.*`, `fileExplorer.network.share.noSharesMessage`)

macOS says `хост` (DirectoryBinding Host → Хост, ssh `подлинность хоста`); MS's `узел` made one panel use two words for
one thing. Hostname → `имя хоста`.

## Network share: общая папка over общий ресурс (`commands.shareSelectShare.label`, `settings.section.smbNetworkShares`, `fileExplorer.network.share.*`, `errors.shareList.*`, `servers.hub.forgetShare`, `fileExplorer.networkMount.shareFallback`)

macOS's Sharing pane says `Общие папки`, MS `общая папка`; `общий ресурс` is Windows phrasing. Prose may say
`сетевая папка`; the mounted thing stays `том`.

## Mounting line and its fallback (`fileExplorer.networkMount.mounting`, `fileExplorer.networkMount.shareFallback`)

`Подключение: {target}…` over `Подключение тома {target}…`: `{target}` is a host name or the fallback share word, so a
type noun in front doubles it (`тома общая папка`) and calls a server a volume.

## Locked: Защита over заблокирован (`errors.write.fileLocked.*`, `errors.mutation.fileLocked`, `errors.write.permissionDenied.suggestion.deleteMac`, `errors.write.trashRefused.suggestion.notPermitted`)

Finder's Get Info checkbox is `Защита` and its command `Снять защиту`; `Разблокируйте файл` then
`снимите флажок «Защита»` names two things. Phone, keychain, and archive unlocks keep `разблокировать`.

## Incoming item: Входящий over Новый (`fileOperations.transferProgress.newLabel`/`.newFileLabel`/`.newFolderLabel`)

`Входящее:` / `Входящий файл:` / `Входящая папка:` beside `Существующий файл:`, because `Новый файл` and `Новая папка`
are the names of Cmdr's create commands.

## Home folder: папка пользователя, menu Личное (`commands.navGoHome.label`, `menu.go.home`, `fileExplorer.errorPane.goHome`, `fileExplorer.unreachable.openHome`, `indexing.phase.home`)

Finder's two forms: the Go menu item `Личное` (Finder 253.title), prose `папка пользователя` (TL_HELP_HOME).
`личная папка` and `домашняя папка` are blends Finder never uses.

## Drag: перетянуть (`settings.advanced.dragThreshold.label`, `askCmdr.composer.dropHint`)

macOS AppKit and Finder say `перетяните` / `перетягивание`; `перетащить` is the Windows word and passes in prose only.
"Drop to attach", shown mid-drag, is `Отпустите, чтобы прикрепить`.

## Extract: распаковать over извлечь (`askCmdr.decision.verbExtract`, `errors.write.archiveEntryNameRefused.message.parentTraversal`)

`извлечь` is Finder's Eject verb, so archive extraction says `распаковать` / `распаковка`.

## Dismiss: Закрыть vs Скрыть (`ui.toast.dismissAria`, `crashReporter.dialog.dismiss`, `errorReporter.sentToast.dismiss`, `downloads.fda.dismiss`, `lowDiskSpace.toast.closeTooltip`, `viewer.reloadToast.dismissTooltip`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`, `adb.hint.dismiss`)

A toast, notice, or dialog → `Закрыть` (macOS ControlCenter, NotificationCenter); a finished queue row or a one-time
hint put away for good → `Скрыть`. ❌ `Отклонить` reads as reject, `Пропустить` as skip.

## Remove from a list: name the list (`settings.mediaIndex.chosenFolders.remove`/`.removeAria`, `menu.volume.removeFavorite`, `askCmdr.attachment.remove`, `shortcuts.section.removeFromOther`)

Russian macOS uses `Удалить` for both deleting and removing from a list (`Удалить из бокового меню`), so always name the
list after `из`, or use `Убрать` / `исключить` where the list is implied. A bare `Удалить` reads as deleting files.

## Technical details: Технические подробности (`fileExplorer.errorPane.technicalDetails`, `fileOperations.errorDialog.technicalDetails`, `commands.errorPaneToggleTechnicalDetails.label`, `errors.write.trashRefused.suggestion.other`, `errors.write.readError.suggestion`, `errors.write.fallback.suggestion`)

The disclosure label is the words the suggestions point at (`проверьте технические подробности ниже`), so
`Технические подробности` over `Технические сведения` (Nautilus `Показать подробности`).

## Log: журнал over протокол (`settings.advanced.logLlmCalls.label`, `crashReporter.sentToast.sendLog`, `operationLog.*`)

macOS (`Журналы`) and MS agree; TC/DC's `протокол` collides with the network protocol (`servers.sheet.protocolLegend`).
`История` is the history concept, not a log.

## Permission: права доступа vs разрешение (`errors.listing.noPermissionErrno.title`, `errors.write.permissionDenied.*`)

File rights are `права доступа` (Finder's `Общий доступ и права доступа`); a macOS privacy grant is `разрешение` (System
Settings). Don't swap them.

## Minimize: Убрать в Dock (`menu.window.minimize`)

Finder and AppKit's Window menu; `Свернуть` is the Windows taskbar verb.

## Onboarding: Знакомство с приложением (`menu.app.onboarding`, `onboarding.stepBeta.checklist.title`)

Over `Начальная настройка`, which would sit beside `Настройки…` in the app menu and read as the same thing. Prose may
say `первоначальная настройка`.

## Model library: библиотека моделей (`onboarding.cloudSetup.step.ollamaModel`)

Over `каталог`, which in TC-lineage Russian means a folder.

## Git words (`errors.git.*`, `fileExplorer.git.*`, `settings.fileExplorer.git.*`)

`worktree` stays verbatim (as de, hu) and is masculine (`связанный worktree`); `рабочее дерево` is only the generic
working tree, so a bare repo is `репозиторий без рабочего дерева`. Dirty → `незафиксированные изменения`; commit →
`коммит`, branch → `ветка`; checkout → Pro Git ru's `извлечь` (`Извлечена ветка «{branch}»`).

## Detached HEAD (`fileExplorer.git.tooltip.worktreeDetached`)

`Отсоединенный HEAD: {id}` over the literal `Отсоединено на {id}`, which means nothing to a Russian git user.

## Toggle: name both states (`commands.selectionToggle.label`, `commands.selectionToggleAndDown.label`, `commands.sortToggleOrder.label`, `commands.tagsToggle*`)

`Изменить выбор` reads as "change your choice". Name both states like macOS (`Выделить или снять выделение`) or use
`Переключить` (`Переключить порядок сортировки`).

## Stalled transfer: замерла (`fileOperations.transferProgress.stall*`, `viewer.error.stoppedResponding`)

`Передача замерла` over `остановилась` (that's stop, and a stalled transfer is alive) and `зависла` (an app hang). "No
progress for {duration}" → `Без прогресса уже {duration}`.

## Share menu: Поделиться (`menu.context.shareNone`, `menu.context.shareLoading`)

The macOS Share menu is `Поделиться`; `отправка` is the send concept. Empty state → `Нечем поделиться`.

## Folder walk: сканирование (`viewer.search.stopTooltip`, `search.walkHandoff.counts`, `search.walkHandoff.finished`)

A live folder walk is `сканирование` / `просканирована`, not `просмотр` (the viewer) or `проверена` (a check). A running
count says so: `Пока найдено …`.

## Running: Работает vs Выполняется (`ai.local.statusRunning`, `operationLog.status.running`)

A server that is up `работает`; an operation in progress `выполняется`.

## Status: состояние over статус (`fileExplorer.navigation.driveIndex.ariaLabel`, `indexing.status.ariaLabel`)

macOS labels say `Состояние`; `статус` reads as social standing in a label.

## One Просмотр, scoped in prose (`viewer.window.fallbackTitle`, `menu.file.view`, `commands.fileView.label`, `settings.viewer.showTextCursor.description`, `settings.viewer.wordWrap.description`, `settings.appearance.datePreviewLabel`)

Russian merges viewer, View, and Preview (TC F3 `Просмотр`, Finder Preview → `Просмотр`). Prose disambiguates with
`окно просмотра` (never `просмотрщик`); the View menu is `Вид`; a sample line is `Пример:`. Two palette commands never
share a name: non-macOS Preview (`commands.fileQuickLook.other.label`) is `Предпросмотр`, beside `Просмотр` for View.

## View as: Показать как (`viewer.toolbar.viewMode.viewAsText`/`.viewAsImage`/`.viewAsPdf`)

`Показать как текст` over `Просмотр как текста`: `как` takes the case of what it compares, so a verb keeps the
nominative/accusative natural.

## Service: служба vs сервис (`settings.advanced.serviceResolveTimeout.label`/`.description`)

System network services (Bonjour) are `службы`; online services are `сервисы`.

## Roll back: откатить, even in prose (`operationLog.rollback.*`, `commands.operationLog.*`)

`Отменить` is taken by Undo and Cancel, so a rollback is `Откатить` / `откат` on buttons and in prose alike.

## Undo and Cancel are both Отменить (`menu.edit.undo`, `fileOperations.trash.undoAction`)

Russian macOS does exactly this; context separates them. Putting trashed files back is `Восстановить` / `Вернуть`.

## Sidebar vs side panel (`settings.askCmdr.enabled.description`)

Finder's sidebar is `боковое меню`, so the Ask Cmdr side panel is `боковая панель` without a clash.

## Left alone: Оставлено как есть over Без изменений (`fileOperations.cancelRollback.reason.*`, `askCmdr.renameUndo.skipReason.*`, `askCmdr.renameUndo.skipped`)

`Без изменений: {name}. Объект изменился…` contradicts itself, and `пропустить` is held for skip. Say
`Оставлено как есть: {name}.`, then the reason; a folder names its head noun (`Папка {name} оставлена как есть.`).

## Staged leftovers: a later transfer, not the next (`fileOperations.cancelRollback.stagedLeftover.*`)

`Cmdr уберет ее позже, при одной из следующих передач сюда` over `при следующей передаче`: cleanup skips files under an
hour old, so "the next one" overclaims.

## Trash refusals don't say часть (`errors.write.trashRefused.message.*`)

The count can be the whole selection, so `часть выбранных объектов` can be false. Use a counted label:
`macOS не переместила в Корзину выбранные объекты (не перемещено: {count})`.

## Insufficient space keeps its hedge (`errors.write.insufficientSpace.*`)

The English is an estimate ("may", "up to"): `Возможно, в целевом месте недостаточно места`,
`Может потребоваться до {required}`. ❌ No alarmist `только`; keep all three ideas of the suggestion.

## Counts without a plural param (`queue.chip.ariaLabel`, `settings.network.customTimeoutUnit`, `queryUi.age.years`, `transfer.fileOnly.*`)

A noun or participle after a pre-formatted number can't agree (`1 процентов`, `1 секунд`, `5 байты`). Use a counted
label colon (`готово {percentText} %`, `Скопировано: {n}, пропущено: {m}`), a unit that fits every number (`сек.`, `Б`,
`байт`), or a real plural.

## Chat memory size (`askCmdr.event.chatMemoryChanged`)

An ICU plural on `{tokens}` (`16 384 токена`), since the English's `{tokens, number}` plus a fixed noun can't agree.

## ETA fragments start lowercase (`indexing.eta.*`, `indexing.scan.etaRough`, `indexing.progress.percentEta`, `indexing.overall.eta`)

Hosts embed `осталось …` mid-line (`95 %, осталось 8 с`), so every ETA fragment is lowercase; "roughly" goes after:
`{eta} (примерно)`.

## Fallback names must fit their hosts (`errors.write.readOnlyDevice.*`, `search.coverage.unnamedDrive`, `search.coverage.uncovered.*`, `search.coverage.toast.*`)

A host with a type noun plus a noun-phrase fallback doubles it (`диск этот диск`). Every drive host says `диск {drive}`,
so the fallback is the postpositive `без названия`. The read-only lines use a colon frame,
`{deviceName}: доступ только для чтения.`, because the value can be a device, an archive, or a `.git` history.

## Disconnect mid-transfer (`errors.write.deviceDisconnected.sided.*`)

Counts as `(скопировано: {done} из {total})`, since `из {total} файлов` breaks at 1 in a raw string; the other drive is
`на том {counterpart}` (`том` is the same in the accusative, so the name stays bare).

## Cloud folders: active voice (`errors.provider.*`)

`Этой папкой управляет **{name}**` over the calqued passive `Эта папка управляется сервисом **{name}**`; the active
keeps `{name}` nominative.

## Managed by the organization (`settings.managed.*`, `ai.translateError.managed.title`, `updates.status.managedOff`)

"Your organization manages X" → `<X in the instrumental> управляет ваша организация` (`Этим параметром управляет …`):
fronting the object stops the indeclinable `ИИ` reading as the subject. The card title is
`Под управлением вашей организации`.

## IT team: ИТ-отдел (`ai.translateError.managed.body`, `ai.managed.hostNotAllowed`, `askCmdr.error.managedByOrganization`)

The everyday word for an employer's or school's IT staff; MS's `ИТ-службы` reads institutional. "Can tell you" →
`В вашем ИТ-отделе подскажут, …`.

## Retry durations (`servers.paneState.retryKeepsTrying`, `servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`)

`Всего попытки продлятся {duration}.` with accusative branches (`# минуту / минуты / минут`), over
`в течение {duration}`, which needs a genitive the nominative branches can't give.

## Reconnect, not connect at startup (`servers.sheet.autoReconnect`, `servers.sheet.autoReconnectInfoLabel`, `servers.sheet.needsStoredSecret`)

`Автоматически подключаться снова` over `Подключаться автоматически`, which reads as "connect at startup", the exact
misreading the label exists to prevent. Quoted copies follow.

## Known servers are named сервер (`servers.hub.editNearbyHint`, `servers.sheet.addedToast`, `servers.refusal.timedOut`, `servers.paneState.connecting`)

The type is known, so `Сервер {name}`, not the robotic `Объект {name}` or `Хост {host}`. SSH keeps `ключ хоста`. The
status column's `Подключено` beside masculine `Сохранен` is an open question (`review-queue.md`).

## Busy marker: (занято) (`menu.volume.ejectBusy`, `menu.volume.disconnectBusy`, `menu.volume.forgetServerBusy`, `menu.volume.forgetSavedPasswordBusy`)

The `busy` ruling's neuter state word, appended to the unchanged base label, the same marker on all four.

## Size unit symbols: Cyrillic (`common.sizeUnit.*`, `settings.appearance.fileSizeFormat.*`, `viewer.copyDialog.refuseBody`)

Cyrillic, as Finder (SP22–SP27). Binary: IEC `КиБ`…`ПиБ` (Dolphin, MS). SI: `кБ`, `МБ`…`ПБ`; lowercase `к` as GOST 8.417
reads `К` as 1024.

## Space key: Пробел (`settings.fileExplorer.suppressQuickLookHint.description`, `settings.fileViewer.suppressBinaryWarning.description`, `fileExplorer.quickLookHint.spaceSelects`)

macOS ru names the space bar `Пробел`, so `⇧Пробел`; `Enter`, `Escape`, and `Tab` stay Latin keycaps.

## Global shortcut marker (`settings.fileSystemWatching.globalShortcutHint`, `downloads.shortcutRow.commandName`, `downloads.shortcutRow.scopeTitle`)

The marker is `(глобальное)` everywhere (an implied `сочетание`, the `global` ruling's form), and the card title names
its noun: `Глобальные сочетания`. One form, so the hint and the shortcut list quote each other exactly.

## Modified-from-default tooltip (`shortcuts.section.modifiedTooltip`, `downloads.shortcutRow.modifiedTooltip`)

`Отличается от значения по умолчанию` over `Изменено значение по умолчанию`, which says the default itself changed.

## The update needs no action (`updates.toast.readyDetail`)

`Перезапустите сейчас или просто откройте Cmdr в следующий раз: обновление установится само.` over
`… или получите обновление …`, which reads as a second chore.

## Reversible delete marker (`suggestedOps.reversibleDeleteWritten`)

`Отмена удалит созданные файлы` over `Для отмены удалите созданные файлы`, which tells the user to delete files by hand.

## Two transfer-time plurals (`transfer.fileOnly.allDone`, `transfer.appearedDuringMove`, `transfer.changedDuringMove`)

One tense in every branch (`появился … и остался там` / `появилось … и они остались там`), never `остается` in one
branch and `осталось` in another; one folder is `в папке {folderName}`, never `Исходные папки: {folderName}`.

## Search result wording (`search.imageResults.similarTo`, `search.walkHandoff.*`)

A heading of similar images is `Похожие на {name}`, not `Похоже на` ("seems like"); results name the query in quotes
(`По запросу «{label}»`) over the calque `для {label}`.

## Eject toasts: the holders lead (`errors.eject.unmountRefusedByApps`, `errors.eject.unmountRefusedByProcesses`, `errors.eject.unmountRefused`, `errors.eject.busy`)

`{apps} еще держат там файлы открытыми.`: the list is the subject in the present tense, so no gender and no doubled
`Приложения … и другие приложения`. `busy` covers copy, move, and delete: `Cmdr еще работает с файлами на нем`.

## Timeout server lines (`errors.mount.timeout`, `errors.shareList.timeout`)

`Сервер «{server}» не ответил вовремя` over `Cmdr ожидал, но «{server}» не ответил`: the type noun carries the
masculine, and `ожидал` has no object.

## Keychain apps (`ai.secretError.keychainBody`, `ai.secretError.keyringBody`, `servers.refusal.certificateUntrusted`)

Name the app with a head noun (`Откройте приложение «Связка ключей»`); two Linux apps are two quoted names
(`«Пароли» или «Связки ключей»`), never one slashed name.

## Download vs load in memory (`ai.toast.downloadingTitle`, `ai.local.downloadModel`, `ai.toast.startingDescription`)

Names keep macOS's `Загрузить` / `Загрузка`; where one surface speaks of both fetching and loading a model, prose says
`скачать` / `скачивание` for the fetch and `загрузка в память` for the load.

## Operation context names the folder (`fileOperations.operationConflict.context`)

`{destination}` is a folder name, so `Копирование в папку {destination}`: the head noun takes the case, no colon crutch.

## Regex and Hex in chips (`queryUi.mode.regex.label`, `queryUi.recent.mode.regex`, `queryUi.ai.patternLabel.regex`, `viewer.search.regex`, `viewer.toolbar.viewMode.hex`)

Chips, toggles, and row labels say `Regex` and `Hex`, since every description asks for short and `Регулярное выражение`
is 20 characters; prose and placeholders keep `регулярное выражение`.

## Eject labels quote the volume (`fileExplorer.navigation.ejectVolumeAriaLabel`, `fileExplorer.navigation.ejectingVolumeAriaLabel`)

`Извлечь «{name}»`, as Finder ru writes it: the quotes let a Cyrillic name (`Флешка`) stay nominative.

## Open with and Reopen closed tab (`menu.context.openWith`, `menu.tab.reopenClosedTab`, `commands.tabReopen.label`)

`Открыть в приложении` is live Finder's label (300795.title), so not `в программе`; `Открыть закрытую вкладку` is
Chrome's Russian label as is.

## Example email: name@example.com (`common.attachEmailPlaceholder`, `settings.updates.emailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`)

A Latin `name@` over the literal `вы@example.com`: a Cyrillic local part isn't something anyone types into an email
field.
