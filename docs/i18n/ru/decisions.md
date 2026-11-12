# ru decisions

Distilled rulings behind `terms.json`: "X over Y because Z", edited in place. `pnpm i18n:brief` pulls a section whose
heading cites a batch key (exact or `*` wildcard), so every heading cites its keys in backticks.

## Lowercase вы

Over Apple's `Вы` (129 `Вы`, 0 lowercase in macOS ru): letter etiquette; MS, Google, and Yandex write lowercase.

## `е` over `ё`

macOS 27 ru writes zero `ё`; recast an ambiguous `все` / `всё`.

## Selection: выделение over выбор (`*elect*`, `commands.queue*`)

Product-owner call: TC's `Выделить` / `Снять выделение` / `Инвертировать выделение` (TC/DC ×9 vs macOS `Отменить выбор`
×2; Nautilus, Dolphin, MS agree). Picking an option, volume, server, or queue row (`queue.row.selectAria`,
`queue.toolbar.*Selected*`, `Выбрано: #`) stays `выбрать`; cursor moves name the file (`Следующий файл`). Remembered
selections (`selection.recent.*`) are `недавние выделения` (`выборов` reads as elections, `выборка` as a DB sample);
quote their dialogs by title.

## Terminal: Терминал (`*erminal*`, `errors.listing.*.suggestion`, `mtp.ptpcameradDialog.*`, `*macFuse.serious`, `*gitDirPermissionDenied.suggestion`)

Product-owner call: macOS 27 (Finder N67 `Открыть в Терминале`), inflected, unquoted; commands stay English code spans.

## Ask Cmdr stays verbatim (`*skCmdr*`)

A product name; `Спросить Cmdr` blurs into the ask verb. Masculine, never inflects; a head noun takes the case.

## Full Disk Access: row label vs prose (`*Fda*`, `*fda*`, `*FullDiskAccess`)

The row is `Доступ к диску` (SecurityPrivacyExtension `ALL_FILES`, 26A428); prose `полный доступ к диску` (Apple's
ALL_FILES_QUIT_*). ❌ `Доступ ко всему диску`: an older label.

## Quit & Reopen: Перезапустить (`onboarding.stepFda.step3`)

The TCC button (`QUIT_APP`, 26A428); a back-translated `Завершить и открыть заново` names no real button.

## Click and double-click: нажать over щелкнуть (`*oubleClick*`, `fileExplorer.network.browser.tooltip.*`, `viewer.binaryWarning.body`, `*navigateTooltip`)

macOS (Finder MR20 `Нажмите «Отменить»`, AppKit `Нажать для выбора`, zero `Щелкните`): `нажмите`, `дважды нажмите` /
`двойное нажатие`. MS's `щелкнуть` passes the check but isn't the default; one verb per catalog.

## Full and brief view: Подробный / Краткий режим (`menu.view.*View`, `commands.view*Mode.label`, `*fullMode`, `*briefMode`, `*stripedRows.description`)

TC pairs 301 `Краткий` with 302 `Подробный`, DC agrees; `полный` reads as complete. Commands name the mode, no
`Переключить на …`.

## No «ошибка» in error copy (`*enericError*`, `*.error.generic`, `askCmdr.error.provider`, `*serverError.title`, `askCmdr.decision.result`, `*tooltipFailed`, `*status.error`)

Mapping in `style.md`; extra forms: status cell `Проблема`, "hit a snag" `Что-то помешало Cmdr…`, `Не удалось` for
"Couldn't" (macOS ru's own).

## Cmdr couldn't: Cmdr не удалось (`*RefusedToast`, `*refusedUpgradeFailed`, `*launchRefused`, `errors.*`)

35 keys over `Cmdr не смог` (13): reads as macOS's `Finder не удается`, genders nothing. Past masculine where Cmdr
already acts (`Cmdr успел`).

## AI provider: поставщик over провайдер (`*rovider*`, `ai.translateError.*`, `askCmdr.error.*`, `*pickerTitle`)

MS keeps `провайдер` for internet providers, `поставщик (услуг)` for a service company; macOS Home and Font Book agree.
A vendor's billing page is `личный кабинет`.

## Advanced: Дополнения (`*dvanced*`, `ai.cloudConsent.logsNote`, `*settingsLine`)

Every Apple tab says it (Finder 261.label, Mail, VPN); `Расширенные` is Windows, `Дополнительно` an adverb. Prose:
`дополнительные настройки`.

## Delete permanently: безвозвратно (`*eletePermanently*`, `*permanentlyLabel`)

Over Finder's `Удалить немедленно` (timing, not permanence); the F-key word `Безвозвратно` matches the command.

## Custom: Пользовательский / Настроить… (`*ustom*`)

macOS 27: `Настроить` 52×, `Пользовательский` 21×, `Свой` 0. Opens an editor → `Настроить…`; a stored value →
`пользовательский`.

## Apple silicon: Mac с чипом Apple (`*clip.notSupported`, `*ocalDisabled`, `*localTooltip`, `ai.local.notInstalled`, `*parallelism.description`)

Apple's form (iPhone Mirroring, Rosetta): `Нужен Mac с чипом Apple`.

## Option labels and toggles: standing behavior (`settings.archives.opt.*`, `*.opt.neither`, `*.opt.smart`, `*GoToLatestShortcut.enabled.label`, `settings.behavior.*App.label`)

- Imperfective (`Просматривать` / `Открывать` / `Спрашивать`, Mail, DC); `Выключено` over a dangling `Ни один`; Smart →
  `Оптимально` beside `Автоматически`.
- A switch named after a command keeps it byte for byte (`Перейти к последней загрузке`).
- A label read into a dropdown is a noun (`Редактор файлов`), or `Редактировать файлы в Как в системе (TextEdit)`.

## Host: хост over узел (`commands.network*`, `commands.shareBack.label`, `fileExplorer.network.*`)

macOS (DirectoryBinding Хост, ssh `подлинность хоста`); MS's `узел` gave one panel two words.

## Network share: общая папка over общий ресурс (`*hareSelectShare*`, `*NetworkShares`, `fileExplorer.network.share.*`, `errors.shareList.*`, `*forgetShare`)

macOS Sharing `Общие папки`, MS `общая папка`; `общий ресурс` is Windows. Prose may say `сетевая папка`; mounted is
`том`.

## Locked: Защита (`*ileLocked*`, `*d.suggestion.delete`, `*notPermitted`)

Finder checkbox `Защита`, command `Снять защиту`; `Разблокируйте` names another thing. Phones, keychains, and archives
keep `разблокировать`.

## Incoming item: Входящий over Новый (`fileOperations.transferProgress.new*Label`)

`Входящий файл:` beside `Существующий файл:`, since `Новый файл` / `Новая папка` are create commands.

## Home folder: папка пользователя, menu Личное (`*.home`, `*Home`)

Finder 253.title `Личное`, TL_HELP_HOME `папка пользователя`; never `личная` / `домашняя папка`.

## Drag: перетянуть (`settings.advanced.dragThreshold.label`, `askCmdr.composer.dropHint`)

AppKit and Finder; `перетащить` (Windows) is prose-only. Drop to attach → `Отпустите, чтобы прикрепить`.

## Extract: распаковать (`*verbExtract`, `*parentTraversal`)

`извлечь` is Eject.

## Dismiss: Закрыть vs Скрыть (`*ismiss*`, `*closeTooltip`)

Toast, notice, dialog → `Закрыть` (ControlCenter, NotificationCenter); a done queue row or one-time hint → `Скрыть`. ❌
`Отклонить` (reject), `Пропустить` (skip).

## Remove from a list: name the list (`*emove*`)

macOS `Удалить` covers both (`Удалить из бокового меню`), so name the list or use `Убрать` / `исключить`.

## Technical details: Технические подробности (`*echnicalDetails*`, `*trashRefused.suggestion.other`, `*readError.suggestion`, `*fallback.suggestion`)

The label is what suggestions point at (`проверьте технические подробности ниже`); Nautilus `подробности`.

## Log: журнал over протокол (`*logLlmCalls.label`, `*sendLog`, `operationLog.*`)

macOS `Журналы`, MS agree; TC/DC `протокол` collides with the network protocol. `История` is history.

## Permission: права доступа vs разрешение (`*ermissionDenied*`, `*noPermissionErrno.title`)

File rights `права доступа` (Finder); a macOS privacy grant `разрешение`.

## Minimize: Убрать в Dock (`menu.window.minimize`)

Finder and AppKit; `Свернуть` is Windows.

## Onboarding: Знакомство с приложением (`menu.app.onboarding`, `*checklist.title`)

`Начальная настройка` would read as `Настройки…` beside it. Prose may say `первоначальная настройка`.

## Model library: библиотека моделей (`onboarding.cloudSetup.step.ollamaModel`)

`каталог` means a folder in TC Russian.

## Git words (`errors.git.*`, `fileExplorer.git.*`, `settings.fileExplorer.git.*`)

`worktree` verbatim, masculine; `рабочее дерево` only generic (`репозиторий без рабочего дерева`). Dirty →
`незафиксированные изменения`, commit `коммит`, branch `ветка`, checkout Pro Git's `извлечь`. Detached HEAD
(`fileExplorer.git.tooltip.worktreeDetached`) → `Отсоединенный HEAD: {id}`; pill `(нет ветки)`.

## Toggle: name both states (`commands.selectionToggle*`, `commands.sortToggleOrder.label`, `commands.tagsToggle*`)

`Изменить выбор` reads "change your choice": `Выделить или снять выделение` or `Переключить …`.

## Stalled transfer: замерла (`fileOperations.transferProgress.stall*`, `viewer.error.stoppedResponding`)

Over `остановилась` (stop) and `зависла` (app hang). No progress → `Без прогресса уже {duration}`.

## Share menu: Поделиться (`menu.context.share*`)

macOS's menu; empty → `Нечем поделиться`.

## Folder walk: сканирование (`viewer.search.stopTooltip`, `search.walkHandoff.*`)

Not `просмотр` (viewer) or `проверена` (check); a running count `Пока найдено …`.

## Running: Работает vs Выполняется (`ai.local.statusRunning`, `operationLog.status.running`)

A server `работает`, an operation `выполняется`.

## Status: состояние (`fileExplorer.navigation.driveIndex.ariaLabel`, `indexing.status.ariaLabel`)

macOS labels; `статус` reads as social standing.

## One Просмотр, scoped in prose (`viewer.window.fallbackTitle`, `*ileView*`, `settings.viewer.*.description`, `*PreviewLabel`, `*QuickLook.other*`, `*viewAs*`)

TC F3 and Finder Preview are both `Просмотр`; prose `окно просмотра`, View menu `Вид`, sample `Пример:`. Non-macOS
Preview is `Предпросмотр` so two commands never share a name. View as → `Показать как текст` (a verb keeps `как`
nominative).

## Service: служба vs сервис (`settings.advanced.serviceResolveTimeout.*`)

Bonjour network services `службы`; online services `сервисы`.

## Roll back: откатить, even in prose (`operationLog.rollback.*`, `commands.operationLog.*`)

`Отменить` is Undo and Cancel.

## Undo and Cancel are both Отменить (`menu.edit.undo`, `fileOperations.trash.undoAction`)

As Russian macOS. Putting trashed files back → `Восстановить` / `Вернуть`.

## Sidebar vs side panel (`settings.askCmdr.enabled.description`)

Finder's sidebar is `боковое меню`, so Ask Cmdr's is `боковая панель`.

## Left alone: Оставлено как есть (`fileOperations.cancelRollback.reason.*`, `askCmdr.renameUndo.skip*`)

`Без изменений: {name}. Объект изменился…` contradicts itself; `пропустить` is skip. Folder:
`Папка {name} оставлена как есть.`

## Staged leftovers (`fileOperations.cancelRollback.stagedLeftover.*`)

`попробует еще раз, когда в следующий раз подключится туда`: no time, and `туда` avoids `к тому {volumeName}`.

## Hedges and totals stay honest (`errors.write.trashRefused.message.*`, `errors.write.insufficientSpace.*`)

The count can be all of them: `не переместила в Корзину выбранные объекты (не перемещено: {count})`, ❌ `часть`. Space
is an estimate: `Возможно, … недостаточно места`, `Может потребоваться до {required}`, ❌ `только`; keep all three
ideas.

## Counts without a plural param (`queue.chip.ariaLabel`, `*TimeoutUnit`, `queryUi.age.*`, `transfer.fileOnly.*`, `*chatMemoryChanged`)

Counted label colon, a unit fitting every number (`сек.`, `Б`, `байт`), or a real plural: chat memory got an ICU plural
on `{tokens}` (`16 384 токена`).

## Fragments fit their hosts (`indexing.*eta*`, `indexing.*Eta`, `*readOnlyDevice*`, `search.coverage.*`)

ETAs are lowercase, "roughly" after (`{eta} (примерно)`). Drive hosts say `диск {drive}`, so the fallback is
`без названия`; read-only lines use `{deviceName}: доступ только для чтения.` (a device, archive, or `.git` history).

## Disconnect mid-transfer (`errors.write.deviceDisconnected.sided.*`)

`(скопировано: {done} из {total})` (raw string, no plural); `на том {counterpart}` (accusative = nominative).

## Cloud folders: active voice (`errors.provider.*`)

`Этой папкой управляет **{name}**`, keeping `{name}` nominative.

## Managed by the organization (`*anaged*`)

Front the object (`Этим параметром управляет ваша организация`) so indeclinable `ИИ` isn't read as the subject; card
`Под управлением вашей организации`. IT staff → `ИТ-отдел` (MS `ИТ-службы` reads institutional):
`В вашем ИТ-отделе подскажут, …`.

## Require SMB encryption (`settings.network.requireSmbEncryption.*`, `*encryptionRequired*`, `*EncryptionRequired*`)

Label `Всегда требовать шифрование SMB`; messages quote it as `в Cmdr включен параметр «…»` (masculine agrees with
`параметр`). Busy provider: `Поставщик {provider} сейчас занят`, progress `Повторная попытка (2 из 4)…` (catalog's
form).

## Servers (`servers.paneState.*`, `*autoReconnect*`, `*needsStoredSecret`, `*editNearbyHint`, `*addedToast`, `*timedOut`, `*.timeout`)

- Retry: `Всего попытки продлятся {duration}.` with accusative branches; `в течение` needs a genitive.
- `Автоматически подключаться снова`: `Подключаться автоматически` reads as at startup.
- `Сервер {name}`, not `Объект` / `Хост`; `Сервер «{server}» не ответил вовремя` (the noun carries the masculine). SSH
  keeps `ключ хоста`.

## Busy marker: (занято) (`menu.volume.*Busy`)

The neuter state word after the unchanged label, on all four.

## Size unit symbols: Cyrillic (`common.sizeUnit.*`, `*fileSizeFormat.*`, `*refuseBody`)

Finder SP22–SP27. IEC `КиБ`…`ПиБ` (Dolphin, MS); SI `кБ`, lowercase per GOST 8.417 (`К` reads as 1024).

## Space key: Пробел (`settings.*suppress*.description`, `*spaceSelects`)

macOS ru names it so (`⇧Пробел`).

## Global shortcut marker (`*globalShortcutHint`, `downloads.shortcutRow.*`, `*modifiedTooltip`)

`(глобальное)` everywhere, card `Глобальные сочетания`, so hint and list quote each other. Modified →
`Отличается от значения по умолчанию` (`Изменено значение…` says the default changed).

## Don't assign a chore (`updates.toast.readyDetail`, `suggestedOps.reversible.removeCopies`)

`…или просто откройте Cmdr в следующий раз: обновление установится само.`; `Это можно отменить: Cmdr удалит копии`, ❌
`Для отмены удалите…` (deleting by hand).

## AI suggestions (`suggestedOps.title`, `menu.view.suggestedOps`, `suggestedOps.agentReason*`)

`Предложения ИИ` over `Предложенные операции`, matching the mkdir hints. `Почему:` (a cause) over `Зачем` (a purpose,
`onboarding.stepFda.why`); the aria `Почему, по словам ИИ:` holds it.

## Two transfer-time plurals (`transfer.fileOnly.allDone`, `transfer.*DuringMove`)

One tense per branch set; one folder is `в папке {folderName}`, ❌ `Исходные папки: {folderName}`.

## Search result wording (`search.imageResults.similarTo`, `search.walkHandoff.*`)

`Похожие на {name}` (`Похоже на` = seems like); `По запросу «{label}»` over `для {label}`.

## Eject toasts: the holders lead (`errors.eject.*`, `fileExplorer.navigation.eject*VolumeAriaLabel`)

`{apps} еще что-то там используют.` (present, no gender); `Закройте это приложение` / `эти приложения` repeat the noun;
Eject all → `Извлечь все тома` (Finder `N199.1`); busy → `Cmdr еще работает там с файлами`. Labels quote the volume:
`Извлечь «{name}»`.

## Keychain apps (`ai.secretError.key*Body`, `servers.refusal.certificateUntrusted`)

`Откройте приложение «Связка ключей»`; two Linux apps are two quoted names, never slashed.

## Download vs load in memory (`ai.toast.downloadingTitle`, `ai.local.downloadModel`, `ai.toast.startingDescription`)

Names keep `Загрузить`; prose with both senses: `скачать` for the fetch, `загрузка в память` for the load.

## Operation context names the folder (`fileOperations.operationConflict.context`)

`Копирование в папку {destination}`.

## Regex and Hex in chips (`*.regex*`, `*.hex`)

Short slots say `Regex` / `Hex` (`Регулярное выражение` is 20 characters); prose keeps `регулярное выражение`.

## Open with and Reopen closed tab (`menu.context.openWith`, `menu.tab.reopenClosedTab`, `commands.tabReopen.label`)

`Открыть в приложении` (Finder 300795.title); `Открыть закрытую вкладку` (Chrome ru).

## Example email (`*mailPlaceholder`)

`name@example.com`: nobody types a Cyrillic local part.

## NFS export is `общая папка` (`servers.*xport*`, `*LockedNfs`, `*nfsStartFolderHelp`)

Over `экспорт` (reads as exporting), as Synology DSM ru says (kb.synology.com/ru-ru, 2026-10-22).
