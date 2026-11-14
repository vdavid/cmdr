# zh-Hant decisions

Distilled rulings: "X over Y because Z". Terms: `terms.json`; style: `style.md`; open questions: `review-queue.md`.
`pnpm i18n:brief` pulls sections whose heading cites a batch key.

Sources: **AP-TW** / **AP-HK** = macOS Finder + AppKit + SystemSettings; **AP live** = installed macOS bundles read by
English key (macOS 26.6.2, 25G83; `../reference-pile/how-to-mine.md` § "No pile on this machine"); **MS** = Microsoft
zh-Hant TBX; **NAU** / **DOL** / **THU** = Nautilus / Dolphin / Thunar zh-TW; **TC** / **DC** = Total / Double Commander
zh-TW; **AOSP** = Android `values-zh-rTW`. Reopen only with evidence of that weight.

## The get-it-back words (`menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.*`, `operationLog.dialog.rollBack`/`finishRollBack`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.rollback*Tooltip*`)

- Undo and Rollback share one dialog, so they differ (`還原` / `復原`); MS's `還原` for restore loses because it
  collides in our catalog. Old names back `改回去`.
- "Undo" of a rollback act is `復原` (`無法復原 {name}`): the act decides. A name change back never `放回原處` (nothing
  moves); rolling back a move is `放回原處`, never `刪除`.
- Finish rolling back `完成復原`: `繼續` is spent on `queue.row.resume`.

## What a cancelled rollback reports (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, `operationLog.rollback.partiallyRolledBackNotice`, `queue.row.reversalInFolder`)

- Skipped `維持原樣`; the `{name} 維持原樣：它…。` / `有 {countText} 個…維持原樣：它們…。` pair copies
  `askCmdr.renameUndo.skipReason.*`, so both reason lists read as one feature. ❌ Never merge into one plural.
- `spotTaken` `原本的位置已經被別的東西佔走了` mirrors `nameTaken` `原本的名稱又被佔走了`; `drift` `在 Cmdr 放好之後`
  (`放好` covers copy and move).
- Removed `刪除`, ❌ `移除` (Apple: `移除` takes out of a list), matching the dialog's promise.
- `done*` names the set (`Cmdr 寫入的全部`), `some*` only counts: ❌ don't collapse. `done*` has an `=1` arm
  (`那個項目`): `全部 1 個` reads wrong.
- The rest names its place (`複製過去的地方` / `移動過去的地方`): bare "still there" reads as `原處`, the opposite.
  `leftBehind` repeats `會略過沒有把握的部分`.
- `stagedLeftover.*`: `不完整副本`, cleared `清掉` (Cmdr's files, not the user's); retry `下次連線到那裡時`, never a
  time.
- `in {folder}` is trailing, unquoted `位於 {folder}` (as `downloads.toast.inSubdir`): after `reversalDeleting`, a
  preverbal `在…中` can't fit, and `位於` marks a place, not the target.

## An unfinished move's staging folder (`fileOperations.leftovers.stagingFolderKept`)

`還沒完成的移動`: `未完成` is the failed status label. `沒有動它們，全都留在…裡`, ❌ `留在原處` (the files aren't where
they started). They're complete and the user's own: ❌ not `不完整副本`, never hint they can go. Keep `隱藏資料夾`, the
one actionable fact.

## Failure-sentence shapes and busy tooltips (`fileExplorer.navigation.ejectBusyTooltip`, `.disconnectBusyTooltip`, `adb.disconnectBusyTooltip`)

- `無法`+verb+object`。`; `無法…，因為…。` (default); `因為…，無法…。` when the user can fix the reason;
  `…，因此無法…。` when a named actor blocks (`磁碟正由「%@」使用中，因此無法退出。`). Recovery ends `然後再試一次。`
  (AP-TW).
- Busy tooltips: `Cmdr 還在處理這個 Y 上的檔案。完成後再X。` (`macOS 還在處理…`).

## metadata: `中繼資料` (`errors.listing.attributeNotFound.*`)

AP-TW `後設資料` and AP-HK `元數據` split; MS `中繼資料` is the only form tagged for both.

## Keychain and keyring: `鑰匙圈` (`ai.secretError.*`, `fileExplorer.network.share.credentialsStoredLocally`, `servers.sheet.remember`)

Also a Linux keyring (gnome-keyring, xfce4-session); Apple has zero `金鑰環`. `記住在鑰匙圈中` is quoted verbatim by
`servers.sheet.needsStoredSecret`.

## Repository: `存放庫` (`errors.git.*`, `onboarding.stepBeta.checklist.star`)

MS's source-control sense; `儲存庫` claimed GitHub's UI without proof; MS `儲存機制` is a data store.

## Go forward: `往前` (`menu.go.forward`, `commands.navForward.label`)

Finder's Go menu `返回` / `往前` (TW = HK); System Settings' `前進` is a VoiceOver description.

## Classifiers and list compounds (`errors.volume.notConnected`, `main.oldWebkit.body`, `errors.mount.unsupportedProtocol`, `settings.mediaIndex.progress.local`, `commands.tabMcpAction.label`)

`這支手機` (AOSP 9 `這支` / 1 `這部`), `這台電腦` for a computer in general. Pane tab action `動作`: `操作` is file
work.

## Selection (`selection.*`, `menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- Recent selections `最近的選取`: `選取範圍` is the SET of selected files (so `menu.context.selection` is `選取範圍`),
  these rows are past queries.
- Tooltips front the locative (`在焦點窗格中選取這些檔案`).
- `選取所有副檔名為 *.{extension} 的檔案` (TC/DC), mask spaced. `{mode}` is spaced: it can arrive as `AI`.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`/`contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- "Look inside" `查看…裡的內容`; `讀取` only where English says "read".
- Camera details `相機資訊` (AP Photos TW), ❌ `EXIF 資料`: consent copy is plain language. Photo place `拍攝地點`, a
  path's `位置` (Photos splits them too).
- Archive `封存檔` (zip, tar, 7z), ❌ zip-only `壓縮檔`. Title `標題`, author `作者`.
- Rail, settings intro, and consent make one promise in one wording: `問到某個檔案` + `查看…裡面的內容`;
  `沒有你的同意，絕不會更動任何檔案` (`任何檔案`, not `任何東西`: it writes its own notes).

## Tool lines name their target (`askCmdr.tool.*.doing`/`.done`/`.tried`, `askCmdr.tool.outcome.*`, `askCmdr.toolGroup.*`)

- A place goes before the verb (`正在 <places>…</places> 裡搜尋「{query}」` / `已在 … 裡搜尋`); `.tried` is `沒能…`
  (calm, never `失敗`), so it can't read as landed. Outcomes after the `，` separator are bare phrases that quote the
  settings names (`影像索引`, `語意搜尋模型`). Tool call `工具呼叫`, counted with `次`.

## System Settings pane names (`errors.git.*`, `errors.provider.*`, `*.emailPlaceholder`, `common.attachEmailPlaceholder`)

In a bold path, a trailing `裡` attaches to the last Han pane name, never to the token. Untokenized panes: `Apple 帳號`
(spaced against Apple), `一般`, `登入項目與延伸功能`.

## Blocking and old-OS notices (`main.oldWebkit.*`, `main.oldMacos.*`)

Quit `結束` (AppKit), ❌ zh-CN `退出`. `X 以上的版本` (SystemSettings), `盡力而為`, layout `版面配置` (MS). "Look off"
`不太對` (casual, dodges `錯誤`). `「軟體更新」`.

## Terminal and text editor pickers (`settings.behavior.openTerminalHereApp.*`/`textEditorApp.*`, `settings.navigationAndFileOps.card.terminal`/`textEditor`, `fileExplorer.edit.appMissing`/`launchRefused`)

`在此開啟終端機` (`在此` is fixed). `選擇 App⋯` (Apple's `選擇應用程式⋯` with our `App`). `文字編輯器` is a category, ❌
not TextEdit (that arrives in `{app}`). System default `跟隨系統`.

## git worktrees (`errors.git.orphanedWorktree.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

`worktree` stays Latin (the `@key` says so; every locale keeps it); a generic working tree `工作目錄樹`. Two names for
one thing was the bug.

## Sort by relevance: `關聯性` (`fileExplorer.columns.sortByRelevance`)

`依關聯性排序`, like `依名稱排序`. Music's `相關資訊` is "related info".

## Documents and packages (`settings.archives.ooxml.*`)

`文件` (Finder's document) and bare `套件` (Finder `顯示套件內容`), wider than `App 套件`, as English contrasts them.

## Servers hub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`/`.forget*`, `fileExplorer.network.share.*`, `fileExplorer.network.browser.*Host*`)

- Places `位置` (Finder's Locations), ❌ `地點` (geographic in Freeform, Find My).
- Last used `上次使用` (AP-HK); Never `從未使用`: AP-TW's bare `永不` is future "never".
- Status cells: `style.md` § Notes and decisions. Signed out is a state: no `無法`.
- Volume switcher `卷宗切換器` everywhere. Local network `區域網路`: the macOS permission dialog says it.
- A server's classifier `部` (tentative, from `這部 Mac`).
- Signed in as `以 {username} 身分登入` / `以訪客身分登入`: a bare `以 X 身分` dangles. Buttons too: `以其他身分登入⋯`,
  `以訪客身分瀏覽`.
- Forget `忘記` everywhere, hosts too (`已忘記` / `無法忘記`). Confirm bodies name the object (`這部主機`,
  `這個共享資料夾`), never a pronoun on the insert.

## Server sheet and host key (`servers.sheet.*`, `servers.hostKey.*`, `servers.refusal.passwordMissing`/`.export*`/`.reservedPortRequired`, `servers.paneState.signedOut`/`.signIn`/`.notConnected`/`.hostKeyChanged*`/`.retryTotal*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

- Button `連線`, command `連接伺服器⋯`: Finder's own split in Connect to Server.
- Passphrase `密語` (Apple), ❌ `通行密碼`; `金鑰密語`, `金鑰檔案`, `金鑰指紋`, ❌ Shortcuts' `密鑰`. A server's "key"
  is always `主機金鑰`, or it reads as an API key.
- Owner `擁有者`, ❌ `管理者` (a home NAS is the user's own). Reinstalled `重新安裝過`, ❌ slang `重灌`.
- Root folder `根資料夾`, ❌ TC/MS `根目錄`; start folder `起始資料夾`, ❌ `開始位置` (`位置` is Places) or `啟動資料夾`
  (Windows Startup). Label and every mention match.
- Won't connect `Cmdr 不會連線到 {name}` (a standing refusal, ❌ `停止`); cancelled `{name} 尚未連線` (a state). Titles
  quote the server (`登入「{name}」`). Add anyway `仍要加入` (Apple `仍要儲存`), quoted in `addAnywayHelp`. Optional
  `選填` (`可選` = a step).
- NFS export `共享資料夾` (Synology `共用資料夾`), ❌ `匯出`. Privileged port `特殊權限連接埠`; the NAS option quotes
  Synology's `「允許來自非特殊權限之連接埠的連線」`. `使用者 ID` / `群組 ID`.

## Reconnecting and the nothing-to-ask line (`servers.paneState.reconnecting`, `servers.paneState.signedOutNothingToAsk`)

A state is `重新連線` (`正在重新連線⋯`, three Apple bundles); Apple's `重新連接` is the "plug it back in" imperative.
The user's own login key is bare `金鑰`, ❌ `主機金鑰`. Retry `再試一次`; `重試` only on a short button.

## Pin hint, host keys, ADB page (`menu.network.pinToSwitcher`/`unpin`, `servers.pinHint.*`, `settings.behavior.serversPinHintSeen.*`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.servers.*`, `settings.adb.*`, `settings.appearance.tintSmb.*`)

- `釘選到切換器` / `從切換器取消釘選` (Music `釘選到資料庫`), the unpin label quoted exactly in `servers.pinHint.body`;
  `「網路」群組`.
- "Getting long" `越來越長了`, ❌ Apple's `過長` (a verdict; this is a nudge). Got it `知道了`, ❌ `瞭解` (three keys
  ship it).
- `信任的 X`, status `已信任`. `重新檢查`; Not found `找不到`; `已找到，位於 {path}` (long path last).
- "Watching for phones" by effect, `手機一接上就會偵測到。`: Apple's Watching is video.

## Android pane and USB debugging hint (`adb.connect.*`, `adb.readiness.*`, `adb.hint.*`, `adb.disconnect*`, `settings.behavior.adbHintDismissed.*`)

- Phone-side words follow AOSP, quoted: `「USB 偵錯」`, `「允許」`, tap `輕觸`.
- Not responding `沒有回應` vs not in time `沒有及時回應` (Apple). `太舊`; cable `連接線`; reseat `拔掉再插上`; wake
  `喚醒`. Link "How" `怎麼開啟？` (bare `如何` doesn't stand).
- "You stopped opening your phone" `你停止了…`, ❌ `取消` (the Cancel button).

## A locked server identity (`servers.sheet.accountLocked`, `servers.sheet.identityLockedNfs`)

Verbs match the controls (`忘記`, `加入`, ❌ `刪除` / `新增`). "Are what name this server" `決定了這是哪個伺服器`:
`命名` reads as the Name field.

## Server row menu (`menu.network.open`, `menu.network.edit`, `fileExplorer.navigation.forgetSecretNoneToast`)

Open `開啟` as `menu.file.open`; `編輯伺服器⋯` as `commands.serversEdit.label`; `{name} 沒有已儲存的密碼。`, siblings'
`已儲存的密碼` verbatim.

## AI copy's subject: `Cmdr` / `AI`, `Ask Cmdr` for the panel only (`suggestedOps.*`, `settings.askCmdr.*`, `askCmdr.error.notConfigured`, `ai.cloud.askCmdrOverrideHint`)

The dialog is `AI 建議` (as the mkdir header and indicator). Its "Why:" label `理由：` (aria `理由，根據 AI 的說法：`;
❌ `為什麼：`, that's onboarding's question) stays apart from `Cmdr 知道的事實` (model's claim vs what Cmdr verified):
❌ don't unify. Chatting `聊天` (verb); the chat as subject `對話`.

## Status-corner AI hints, `同意`, `上下文` (`askCmdr.wake.needsFullDiskAccess`, `settings.askCmdr.proactive.description`, `settings.ai.localContextSize.label`/`tooltipLocal`, `askCmdr.error.localWindowTooSmall`, `askCmdr.event.contextTrimmed`)

Approve `同意` everywhere (`批准` was aligned). `needsFullDiskAccess` contains `search.coverage.setUpFullDiskAccess`
verbatim, as its `@key` asks. Context `上下文`: Apple has no LLM sense (0 `脈絡`, 0 `上下文`), MS id 38882 the only fit
(its other entries are `內容` / `執行內容`). Tentative.

## AI provider setup wizard (`onboarding.cloudSetup.*`)

Deployment `部署`, endpoint `端點` (MS). The endpoint URL field `端點網址`; `位址` stays an IP or network address. Pull
(`ollama pull`) `提取` (MS 3 of 4).

## The Dock nudge and the Dock menu (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`, `menu.dock.*`)

- Configuration profile `設定描述檔` (AP-TW = AP-HK), ❌ MS `組態設定檔`. No Thanks `不，謝謝`.
- On the Dock, `保留在 Dock 上` / `從 Dock 中移除` (Dock.app's items), ❌ `釘選` (Cmdr's own UI): `unpinNote` sends the
  user to that exact item.
- Results never say `失敗`; `addedButDockDidNotRestart` is `只是…還沒…` (icon placed, redraw pending).
- `開啟 Cmdr` (outlier rule over `打開`). `前往資料夾⋯` stays apart from `前往路徑⋯`. `{name}（{parent}）`.

## Show in Finder takeover (`main.reveal*`, `settings.behavior.reveal*`, `settings.revealHandler.notProductionBuild`)

`「在 Finder 中顯示」` quoted, as the card. "For a while now" `有一陣子了`, ❌ a day count. `開發版和測試版`, released
`正式發行的 Cmdr 版本` (MS), ❌ `建置` / `組建`.

## Onboarding checklist and toggles (`onboarding.stepBeta.*`, `onboarding.stepOptional.*`, `onboarding.moreAbout`, `onboarding.wizard.*`)

- GitHub `star` stays Latin, counted `顆星` (MS's GitHub-sense entry). Like `按讚`. Mailing list `郵寄清單`, ❌
  `電子報`. Bug `問題`, ❌ `錯誤`.
- `進一步了解「{topic}」` (AP-HK): brackets mark a UI name and settle the spacing.
- `<alpha>` badge `標記`, ❌ `徽章` (achievements) / `標籤` (tag). `「區域網路」` keeps `取用` outside. `{mandatory}+1`
  is literal.
- Fetch (viewer pulls a temp copy) `取得`, ❌ `下載` (a phone or archive isn't a download).

## The phone's stale-index hint (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.*Phone`)

Copies the drive family; only the cause changes: `不會…告訴 Cmdr` (❌ `通知`, the macOS notification), ❌ a disconnect.
`Cmdr 會讓索引跟上自己做的變動`, no unpromised `即時`.

## Mount and share-list sentences (`errors.mount.*`, `errors.shareList.*`, `fileExplorer.pane.directConnectionShareGoneToast`, `servers.refusal.accountNotPermitted`)

Share `共享資料夾` (NetAuthAgent's bare `共享` takes `資料夾` here); guest `訪客`; credentials didn't work `不管用`; a
Linux distribution `發行版` (tentative); something went wrong `出了點狀況`.

## A share the server says doesn't exist (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Waiting can't help: no `現在`, no `再試一次`; `這種情況等下去也不會好`.

## A drive gone mid-work (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.*`, `errors.write.moveNotConfirmed.*`)

- Title `中斷連線`, body `斷線了`: the family's split, not drift. `正在中斷連線` / `中斷了連線`.
- Sided bodies never label source or target: `把 {total} 個檔案中的 {done} 個…`, the rest placed (`還在磁碟機上`),
  `你原本的檔案`, `什麼都沒被丟掉`.
- An unconfirmed move isn’t a failure (`無法確認這次移動`): Cmdr kept the originals on purpose,
  `把你原本的檔案留在原處沒有動`. `原處` fits: those files never left.

## The favorites menu (`commands.favoritesOpen*`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favorites*`/`.switcherFavorites`/`.favoriteUnreachable.*`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- `喜好項目選單`, `卷宗切換器`, `指令面板` stay apart: ❌ `面板`, `清單`, or `切換器` here.
- `顯示喜好項目` (as `顯示伺服器`; `開啟` opens files). `目前的資料夾`.
- Can't add here: `…指向的是磁碟、共享資料夾、伺服器和手機上的資料夾`, no protocol names.
- Off in Settings names the support (`Android 手機支援已⋯關閉`): `手機已關閉` reads as powered off.

## Eject refused (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`, `.otherProcesses`)

- Names stay bare, against AppKit's `「%@」`: a joined list can't be quoted per item.
- A wrapper may concern a share or a phone: `X 還在使用那裡的某些內容。…，然後再試一次。`, ❌ `這個磁碟機` /
  `再退出一次`. `請把它關掉` / `請把它們關掉` point at the app or apps; Eject all → `全部退出` (Finder `N199.1`); the
  aside keeps Cmdr as subject, ❌ `我們`.
- `磁碟映像檔` (AP-TW = AP-HK), then `那個映像檔`. `…，分別是 {processes}。`, ❌ `名為…的程序` (the list can end
  `其他程序`).

## The full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

`沒有「完全取用磁碟」權限`: TW `完全取用磁碟` over HK `完整磁碟取用`, bracketed as the System Settings item.

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.*.title.trash`)

- No plural machinery: `你選的項目裡有 {count} 個，…`, right at 1 and 7.
- Badge `警告標記` (tentative), ❌ `標籤`. Its text holds brackets, so a colon introduces it
  (`寫著：沒有「完全取用磁碟」權限`), ❌ nested `『』`.

## Online-only delete warnings (`fileOperations.delete.cloudOnlineOnly*`)

Four facts stay: the Trash downloads first, so only Delete is offered; no copy lands in the Trash; the cloud keeps its
own; the way out. Prose `只存在雲端`, the label `僅存在雲端`; `「刪除」` is the button.

## Look-alike names on a server (`fileOperations.transferProgress.lookAlikeHint`, `errors.*.ambiguousName*`)

`伺服器儲存的寫法不同`, ❌ `拼法` (pinyin). `重音符號` / `帶重音符號的字母`, ❌ bare `重音` (stress). `現有的那一個`
echoes `現有（資料夾）：`.

## The cloud AI toggle (`ai.cloudConsent.*`, `askCmdr.gate.*`)

Allow `允許` (TCC's button), ❌ `同意` (approve); mentions quote `允許使用雲端 AI` verbatim. `AI 服務` stays apart from
`提供者`.

## Open-with and Share loading rows (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`正在尋找 App⋯`, `分享選項`, `沒有分享選項` (macOS `沒有可套用的服務`).

## 移動完成了，原本的檔案卻還在（`errors.write.sourceNotRemoved.*`、`errors.write.permissionDenied.*.source*`、`errors.write.deletePending.message`）

Cmdr's failed step `移除`, the user's `刪除`. `原本的檔案` as in `destinationNotFound.*`, `moveNotConfirmed.*`.

## Managed by organization (`settings.managed.*`, `ai.managed.*`, `updates.status.managedOff`)

`由你的組織管理` (Apple MDM wording); the card's Off is the state `已關閉`, ❌ the option `關閉`. IT team `IT 人員`
(neutral, no possessive).

## Size units (`common.sizeUnit.*`, `settings.appearance.fileSizeFormat.*`)

`KiB` / `kB` stay Latin (DOL, MS), ❌ `KB`. `4 GiB 以上`.

## Text caret and hover (`multiRename.placeholderHelp.counterEditHint`)

Caret `插入點` (Apple), ❌ `游標` (our pane cursor). Point at it `把指標停在…上`.

## Spoken counts (`fileOperations.transferProgress.*ValueText`, `viewer.pull.progress`, `settings.sidebar.matchCount`)

X of Y `{doneText}，共 {totalText}`, ❌ `/`. Section `區段`, ❌ `部分` (= partly).

## Tell me about this (`commands.fileTellMeAbout.label`, `aboutThis.verdict.*`, `aboutThis.standardFolder.*`)

Command `告訴我這是什麼` wherever it's named. AI take `AI 看法`, labeled `由 AI 撰寫`; the card's facts `卡片上的事實`.
Standard folders quote Finder's names (`「資源庫」`, `「使用者」` TW over HK `用户`). Notarization ticket `公證票證` (no
pile hit, tentative).
