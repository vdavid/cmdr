# uk review queue

Open questions only a native reviewer can settle: a `tentative` ruling the sources couldn't decide, a phrasing that
reads correctly but maybe not naturally, a register call. One bullet per item, citing the keys in backticks. Not
translator input: translators work from the brief and `terms.json`. Remove an item once a reviewer settles it (and
record the outcome in `terms.json` or `decisions.md`).

## Terms and register

- **Lookup vs search** (`askCmdr.limit.*`, `askCmdr.event.contextTrimmed`, `askCmdr.error.repeatedToolCall`): the
  tentative ai-lookup «пошук» makes "rounds of lookups" «раундів пошуків», colliding with `askCmdr.toolGroup.searches`.
  Would «запит» read better?
- **Binary mode** (`viewer.toolbar.viewMode.binary`, `commands.viewerViewAsBinary.label`): does «двійкові дані» read as
  raw bytes, not base-2 digits?
- **Symlinks** (`errors.listing.symlinkLoopErrno.*`, `fileExplorer.entry.brokenSymlink`): is «Зациклені символічні
  посилання» idiomatic, and «ярлик» natural as a gloss? «(зламаний лінк)» would be shorter in the date column.
- **Git jargon** (`errors.git.bareRepo.*`, `errors.git.blobTooLarge.suggestion`, `errors.git.orphanedWorktree.title`,
  `fileExplorer.git.size.stashEntries`, `fileExplorer.git.tooltip.worktreeOnBranch`, `fileExplorer.git.tooltip.commits`,
  `fileExplorer.git.repoChip.tooltip.unborn`): «Bare-репозиторій», «зробіть його checkout», «схованка», «Відкрито
  гілку», «на ненародженій гілці» (maybe «на новій гілці (ще немає комітів)»). A native dev should confirm.
- **NFS export** (`servers.sheet.nfsExport` and the NFS family): tentative; reads «Спільна папка», same as SMB. What do
  Synology/QNAP uk UIs say?
- **Admin rights** (`errors.write.permissionDenied.suggestion.needsAdmin*`, `systemProtected*`): «права адміністратора»
  (Font Book) kept apart from file «привілеї»; Keychain says «привілеї адміністратора».
- **Your macOS user** (`errors.write.permissionDenied.message.named`): «Ваш користувач macOS» vs «Ваш обліковий запис
  macOS».
- **Router** (`errors.listing.networkConnectionDropped.explanation`): «роутер» (everyday) vs «маршрутизатор».
- **Stale handle** (`errors.listing.staleConnection.explanation`): «за старим посиланням», or «застарілий
  ідентифікатор»?
- **View as virtual folder** (`errors.write.sourcesSpanVolumes.*`): «віртуальна папка» for snapshots, zip interiors, and
  git history; right?
- **Storage provider** (`errors.listing.objectStoreRefused.title`): «постачальник сховища» for S3/R2 vendors?
- **Remote share** (`errors.listing.remotePermissionDenied.explanation`, `fileExplorer.navigation.spaceMtpHint`): «у
  спільній папці в мережі» vs «на мережевому ресурсі»; MTP's «ваші спільні файли» may read as network shares.
- **Manifest** (`errorReporter.dialog.manifestHeading`): «Маніфест» vs «Метадані звіту».
- **Anonymized** (`crashReporter.sentToast.sendLogHint`): «анонімізується» vs «знеособлюється».
- **Crates** (`licensing.acknowledgements.rustHeading`, `jumpToRust`): is «крейти» understood, or «пакети (crates)»?
- **Open beta** (`licensing.about.version`): «відкрите бета-тестування» vs «відкрита бета».
- **Personal license** (`licensing.commercialReminder.usingPersonal`): lowercase «особистою ліцензією»; check against
  the website's tier names once they exist.
- **Software Update** (`main.oldWebkit.body`): is «Оновлення ПЗ» the current System Settings pane name?
- **Apple status page** (`errors.provider.iCloud.serious`): «сторінку стану систем Apple» vs Apple's own «Стан системи».
- **VeraCrypt repair** (`errors.provider.veraCrypt.serious`): VeraCrypt's uk UI wasn't available to quote.
- **Cloud mount** (`errors.provider.cmVolumes.*`): «змонтоване хмарне сховище».
- **Tag names** (`commands.tagsToggle*.label`): confirm Finder's sidebar says Бузковий and Оранжевий (not Фіолетовий /
  Помаранчевий).
- **Teal vs cyan** (`settings.tint.teal`, `settings.tint.cyan`): «Бірюзовий» vs «Блакитний»; distinct enough?
- **Local LLM** (`settings.ai.provider.opt.local`): «Локальна LLM» or «Локальна мовна модель»?
- **Settings vs setup** (`settings.mediaIndex.progress.kept`, `settings.mediaIndex.reclaim.line`,
  `stateRecovery.localAi.title`): «параметр» for a config value where «налаштування» may read better.
- **Render, prefetch** (`settings.advanced.virtualizationBufferRows`/`Columns.*`,
  `settings.advanced.prefetchBufferSize.label`): «відмальовувати»; «попереднього завантаження» vs «попереднього
  читання».
- **Viewer jargon** (`settings.fileViewer.suppressBinaryWarning.*`, `viewer.statusBar.hint.image`): «сирі байти», «UTF-8
  із втратами», «банер»; «зсув» for pan.
- **Name mask** (`multiRename.nameMask`): «Маска імені», though the sheet says «назва» elsewhere; «Маска назви»?
- **EXIF date** (`multiRename.mediaField.exifTaken`): «Дата знімання» (Apple) vs «Дата зйомки».
- **Tick column** (`suggestedOps.columnInclude`, `includeAll`, `includeFile`, `suggestedOps.refusal.nothingSelected.*`):
  «Включити…» (or «Додати», «Враховувати»); «Позначте» for ticking, though позначити is on the select avoid list.
- **Security advisory** (`updates.toast.readAdvisory`, `whatsNew.securityNotice.readAdvisory*`): «бюлетень» follows a
  tentative ruling.
- **Cyrillic file name** (`viewer.saveAs.defaultName`): «вибране» or Latin «selection»?

## Tone and phrasing

- **AI errors** (`ai.translateError.rateLimited.title`, `serverError.title`, `authFailed.title`): the casual English
  ("out of room", "hit a snag", "got turned down") came out plain.
- **Ask Cmdr lines** (`askCmdr.tool.search*.done`, `askCmdr.tool.nothingToSuggest.*`, `askCmdr.tool.details`,
  `askCmdr.tool.preview.loading`, `askCmdr.tool.unknown.doing`, `askCmdr.tool.preview.status.unreachable`): the
  end-placed «завершено» on a one-line rail, «Що повернулося» (or «Результат запиту»), «Триває робота», «поза
  досяжністю».
- **Ask Cmdr voice** (`askCmdr.spendPause.question`, `askCmdr.limit.continueMessage`, `askCmdr.wakeDigest.removed`,
  `askCmdr.renameReview.title`, `askCmdr.renameReview.blocked`, `askCmdr.cost.underAmount`): «вилучений» vs «зниклий
  елемент»; imperative vs noun title; «менше ніж» vs «менше за».
- **Maker of Cmdr** (`commands.feedbackSend.description`): «авторові» vs «творцеві» / «розробникові».
- **Personified Cmdr** (`errors.write.changedWhileDisconnected.title`, `errors.listing.ambiguousName.explanation`,
  `errors.listing.duplicateName.explanation`, `servers.paneState.guestSessionRefused`,
  `fileOperations.cancelRollback.moveAlreadyLanded`, `errors.volume.staleDestinationHandle`): «не чіпав», «не став
  вибирати», «не став входити», «зупинився посеред…»; check register and that nothing reads alarming.
- **"In the way" titles** (`errors.write.destinationNotAFolder.title`, `errors.write.mountedVolumeInside.title`):
  «Заважає файл», «Заважає змонтований диск».
- **Eject on its own** (`errors.eject.timedOut`): reflexive «вийметься сам» vs «його буде вийнято».
- **Answer in time** (`errors.volume.connectionTimeout`): dropped "the connection" as subject: «Відповідь не надійшла
  вчасно».
- **Get Info** (`errors.mutation.fileLocked`): «Відімкніть його у Finder через «Досьє»».
- **Soft questions** (`feedback.dialog.invalid`, `fileExplorer.doubleClickHint.title`): «Скоротите її і спробуєте ще
  раз?», «Що це було?».
- **We voice** (`fileExplorer.listingStalled.title`, `fileExplorer.networkMount.mountingSlow`): «Досі чекаємо…» vs «Ця
  папка досі відкривається».
- **Status chips** (`fileExplorer.network.browser.status.*`, `servers.hub.status.signedOut`,
  `fileExplorer.navigation.connectionTooltipNeedsSignIn`): «Вхід виконано» for both logged-in states, «Проблема» for
  Error, «Ви вийшли» (or «Сеанс завершено»).
- **Onboarding voice** (`onboarding.stepFda.con.body`, `onboarding.stepFda.firstAsk.lede`,
  `onboarding.fdaBadge.tooltip`, `onboarding.stepAi.local.tooltip`, `onboarding.stepOptional.indexing.descIntro`,
  `onboarding.stepAi.choiceGroupAria`, `onboarding.stepBeta.checklist.alternativeTo`): «чималу владу», «без цього ніяк»,
  «береже для себе», «значно тупіша», «дуже класне», «Вибір щодо ШІ», «Вподобати».
- **About and main** (`licensing.about.tagline`, `main.oldMacos.body`, `main.revealActivation.title`,
  `main.escapeFullScreenHint.message`): «створений для клавіатури», «усе працює без гарантій».
- **Waiting while busy** (`indexing.enrich.pausedIdle`, `indexing.step.catchUp`): «Очікування, поки ви зайняті»,
  «Врахувати недавні зміни».
- **Playful labels** (`queryUi.scope.toggle.hideBoring`, `settings.appearance.dateColors.opt.wilting`,
  `settings.mediaIndex.importanceThreshold.floor`, `downloads.toast.learnIntro`): «нудні», «Вʼянення», «Мотлох».
- **Update dialogs** (`updates.moveToApplicationsDialog.title`, `.translocated`, `.readOnlyVolume`,
  `updates.toast.restart`, `errorReporter.prefill.updateNotGenuine`): an imperative title, «нікуди записатися»,
  «Перезапустити» without "now", «видавало себе за».
- **Background buttons** (`fileOperations.transferProgress.background`, `.queue`,
  `fileOperations.backgroundStart.startedToast`): do «У фоні» / «У чергу» read as actions? «Відправлено» vs «Переведено
  у фоновий режим».
- **Progress** (`fileOperations.trash.undoing`, `fileOperations.transferProgress.stallNotice`,
  `fileOperations.transferTree.preparing`, `fileOperations.transferDialog.scanUnresponsive`/`scanStopped`,
  `settings.mediaIndex.importanceThreshold.previewCounting`, `viewer.error.stoppedResponding`): «Повернення назад…»,
  «Немає прогресу» vs «Без руху», «Складання списку файлів…», «Обчислення охоплення…».
- **Trash gone** (`fileOperations.trash.undoGone`, `undoPartlyGone`): does «зник зі Смітника» sound alarming?
- **Radio labels** (`fileOperations.transferDialog.policyStop`, `policyAskEach`,
  `settings.behavior.fileSystemWatching.downloadsNotifications.opt.both`/`.neither`): «Спитати пізніше», «Питати щодо
  кожного», «Обидва / Жоден» (or «Усюди / Ніде»).
- **Long labels** (`settings.fileOperations.skipConfirmation.label`,
  `settings.behavior.doubleClickPaneNavigatesToParent.label`, `settings.listing.spaceCalculatesFolderSize.label`,
  `main.escapeFullScreenHint.switchLabel`, `errors.write.insufficientSpace.title`, `shortcuts.scope.errorScreen`,
  `fileOperations.mkdir.createIn`): shorter shapes welcome; «тло» vs «фон».
- **Narrow fields** (`fileExplorer.dirSize.noPerms`, `multiRename.mediaEditor.digits`,
  `multiRename.counterEditor.digits`, `servers.sheet.nfsUid`/`nfsGid`, `servers.hub.colLastUsed`, `ai.cloud.effort.max`,
  `settings.control.unitItems`, `multiRename.searchOptionHelp.on`/`.off`): «<без доступу>», «Кількість цифр», «ID
  користувача», «Востаннє», «Максимальний» vs «Макс.», «елем.», «Увімк.».
- **Settings phrasing** (`settings.network.localNetworkAccessLabel`, `settings.network.permissionIntroSuffix`,
  `settings.network.enabled.label`, `settings.askCmdr.spendPause.label`, `settings.askCmdr.wakeDelay.label`,
  `settings.mediaIndex.parallelism.description`, `settings.mediaIndex.progress.coveredOfTotal`/`coveredDone`,
  `settings.mediaIndex.showInSearch.label`, `settings.listing.stripedRows.label`,
  `settings.listing.sizeDisplay.opt.smart`, `settings.appearance.uiDensity.opt.comfortable`,
  `settings.behavior.openTerminalHereApp.chooseAppTitle`): «Погодьтеся» vs «Дозвольте», «в охоплених папках», capital
  «Пошуку», «Смугасті рядки», «Оптимальний», «Зручно», «програма-термінал».
- **Trust buttons** (`servers.hostKey.trustAndConnect`, `trustNewKey`, certificate twins): imperfective «Довіряти…».
- **First person** (`servers.hostKey.changedDisclosure`, `servers.certificate.changedDisclosure`,
  `whatsNew.dialog.optOut`): impersonal «Ключ перевірено» vs a neutral first person; «Мене не цікавлять журнали змін».
- **Untranslated NAS/cloud labels** (`servers.refusal.reservedPortRequired`, `servers.sheet.s3GcsKeyHelp`,
  `servers.sheet.s3PathStyle`): English «Allow connections from non-privileged ports», «Interoperability», and
  «(path-style)» kept; no uk UI found to quote.
- **Search names** (`search.walkHandoff.*`, `search.coverage.line.uncovered.unavailable`, `search.coverage.pressEnter`,
  `selection.action.*.tooltip`): «Пошук «*.pdf»», «охопити пошуком {drive}», «перегляне пропущене», «на панелі у фокусі»
  vs «на активній панелі».
- **Queue rows** (`queue.row.label`, `queue.row.itemsInFolder`, `queryUi.mode.ai.label`, `queryUi.filters.type.both`,
  `queryUi.date.preset.thisMonth`/`lastMonth`): «Переміщення в Смітник 3 елементів у папці «Downloads»», «у папці
  «Macintosh HD»», «Запитайте будь-що», «Усе», «{month}, 1-ше число».
- **Other quoted names** (`ai.secretError.keyringBody`, `menu.context.openWithOther`,
  `menu.context.shareLinkSevenDays`/`OneDay`/`OneHour`, `menu.volume.editFavoriteShortcut`,
  `transfer.metadataClass.lockedAndHidden`, `fileExplorer.navigation.groupMobile`,
  `fileExplorer.pane.fileExplorerAriaLabel`, `ui.loadingIcon.finalizing`): nested «» in «Паролі та ключі», «Іншою…» vs
  Finder's «Інше…», «Спливає через …», «Призначити клавішу…», «Приховано», «Мобільні пристрої», «Файлові панелі»,
  «підготовка списку».
- **Select via Space** (`fileExplorer.quickLookHint.spaceSelects`): «додає … до вибраних»; «вибирає» once «вибир» is an
  accepted stem.

## Agreement with counts

- **Count frames** (`errors.write.trashRefused.message.*`, `errorReporter.dialog.totalLines`,
  `errorReporter.dialog.sampleFirstHeading`/`sampleLastHeading`, `errorReporter.sentToast.message`,
  `errorReporter.amendedToast.message`): «(кількість: {count})» reads formal; «перші 21 рядок»; the added colon before
  the ID badge.
- **Branch agreement** (`askCmdr.renameReview.coverage`, `askCmdr.renameUndo.undoJob`/`undoJobLabel`,
  `fileExplorer.network.reconnect.*`, `fileExplorer.clipboard.cutReady`, `multiRename.undoTooltip`,
  `multiRename.undoStarted`, `multiRename.summaryReading`, `multiRename.placeholderHelp.allButLast`,
  `multiRename.rangeEditor.reading.spanFromEnd`, `multiRename.mediaEditor.usesModified`/`missing`,
  `transfer.fileOnly.allSkippedMany`, `transfer.fileOnly.mixedCopy`, `transfer.appearedDuringMove`,
  `transfer.changedDuringMove`, `settings.mediaIndex.importanceThreshold.deltaAdd`, `fileOperations.shared.fileRate`,
  `fileOperations.transferProgress.stallInFlight`, `fileExplorer.navigation.driveIndex.tooltipUnreadable*`,
  `fileExplorer.navigation.driveIndex.tooltipCoalesced*`): singular vs plural verbs with 5+, «всі 21 пакет», «готові» vs
  «готово», dative/genitive with counts.
- **Left alone** (`fileOperations.cancelRollback.reason.*`, `askCmdr.renameUndo.skipReason.*`): «Залишено як є папку
  {name}» and «їх» after a `one`-branch 21.
- **On a quoted volume** (`fileOperations.cancelRollback.stagedLeftover.*`, `fileOperations.leftovers.*`,
  `errors.write.deviceDisconnected.sided.*`, `errors.write.changedWhileDisconnected.message`): a bare «на / до
  «{volumeName}»» for a disk, phone, or NAS.
- **Mixed agreement** (`fileOperations.transferProgress.annotationLarger`, `annotationNewer`): «(більший)» with size,
  «(новіша)» with date, or neutral «(більше)» / «(пізніше)»?
- **Queued sources** (`fileOperations.sourcesGone.message`, `nothingLeftToast`): long sentences on impersonal 3pl
  «перемістили або видалили».
- **Billing** (`fileOperations.s3Cost.line`, `fileOperations.s3Cost.infoText`): «за публічними цінами», «включені
  обсяги».

## Catalog consistency to fix

- **Keychain checkbox** (`servers.sheet.remember`, `fileOperations.resticPassword.remember`,
  `servers.refusal.s3SecretNotStored`): «Зберегти пароль у Вʼязці ключів» adds «пароль», untrue for an S3 key; consider
  «Запамʼятати у Вʼязці ключів».
- **Full Disk Access** (`askCmdr.wake.needsFullDiskAccess`, `search.coverage.setUpFullDiskAccess`): quoted «Повний
  доступ до диска» vs the button «Налаштувати повний доступ до диска».
- **Git portal** (`fileExplorer.readOnly.gitPortalTitle`, `settings.fileExplorer.git.showVirtualGitPortal.label`):
  «Git-портал» vs «віртуальний портал git»; no portal ruling yet.
- **Already-gone toast** (`settings.fileOperations.showAlreadyGoneToast.label`, `fileOperations.alreadyGone.many`):
  «Повідомляти, якщо видаленого елемента вже не було» and the «Уже немає {countText} елемента» frame for 21, 31… need a
  native check; confirm, or suggest a «зник» frame.
- **DFS link frame** (`errors.listing.linkTargetUnreachable.explanation`, `errors.volume.linkTargetUnreachable`,
  `viewer.error.linkTargetUnreachable`): «Шлях … веде через посилання на інший сервер» needs a native check for how
  natural it reads.
- **Jump badge clear** (`fileExplorer.typeToJump.clear`): «Очистити «Перехід»» needs a native check, since a screen
  reader drops the quotes.
- **Stays home** (`ai.cloudConsent.askCmdr.protectedContents`): «Деякий вміст залишається вдома» keeps the English image
  in a consent text; confirm it reads as "never leaves your Mac" to a native reader.
- **Tool group header** (`askCmdr.toolGroup.summary`, `askCmdr.toolGroup.summaryRunning`): «Опрацьовано: 6 папок, 2
  пошуки» and «Опрацювання: …» need a native check for how natural they read with counts.
- **Operation log status words** (`operationLog.tree.statusMovedBack`, `operationLog.tree.statusRenamedBack`,
  `operationLog.tree.rollupRolledBack`): «Переміщено назад», «Перейменовано назад», and the lowercase rollup «відкочено
  3, пропущено 2» (lowercase to match the shipped `fileOperations.transferTree.skippedCount`, even when it stands alone
  in the cell) need a native check.
- **Checkbox verbs** (`commands.operationLogToggleItem.label` and siblings): «Вибрати елемент або скасувати вибір»
  follows the select and deselect rulings; confirm it reads naturally for a checkbox.
