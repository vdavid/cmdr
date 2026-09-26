# zh-Hant decisions

Distilled rulings for Traditional Chinese: "X over Y because Z". Term rulings live in `terms.json`, style in `style.md`,
open questions in `review-queue.md`. `pnpm i18n:brief` pulls the sections whose heading cites a batch key.

Sources: **AP-TW** / **AP-HK** = macOS Finder + AppKit + SystemSettings in that locale; **AP live** = the installed
macOS bundles read by English key (macOS 26.6.2, build 25G83; recipe in `../reference-pile/how-to-mine.md` § "No pile on
this machine"); **MS** = the Microsoft zh-Hant TBX; **NAU** / **DOL** / **THU** = Nautilus / Dolphin / Thunar zh-TW;
**TC** / **DC** = Total / Double Commander zh-TW; **AOSP** = Android's `values-zh-rTW`. Reopen a ruling only with
evidence of that weight.

## The get-it-back words (`menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.*`, `operationLog.dialog.rollBack`, `operationLog.dialog.finishRollBack`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`/`.rollbackAlreadyLandedTooltip`)

- Undo `還原` (AP-TW = AP-HK), roll back `復原` (MS), restore from a keeper `回復` (AP-TW Finder `部分項目無法回復。`),
  put back from Trash `放回原處`, old names back `改回去`. Undo and Rollback share one dialog, so they must differ; MS's
  `還原` for restore loses because it collides inside our own catalog.
- English "undo" about a rollback act takes `復原` (`無法復原 {name}`): the act decides. A name change back never says
  `放回原處`, since nothing moves.
- Finish rolling back `完成復原` over `繼續復原`: English says Finish, and `繼續` is spent on `queue.row.resume`.
- Rolling back a move `放回原處`, never `刪除`: undoing a move destroys nothing.

## What a cancelled rollback reports (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, `operationLog.rollback.partiallyRolledBackNotice`, `queue.row.reversalInFolder`)

- A skipped item is `維持原樣`; the `{name} 維持原樣：它…。` / `有 {countText} 個…維持原樣：它們…。` pair is copied from
  `askCmdr.renameUndo.skipReason.*`, so both reason lists read as one feature. Never merge the pair into one plural.
- `spotTaken` `原本的位置已經被別的東西佔走了` mirrors `nameTaken` `原本的名稱又被佔走了` (a taken place vs a taken
  name). `drift` `在 Cmdr 放好之後` because `放好` covers both a copy and a move.
- Removed is `刪除`, never `移除` (Apple: `移除` takes out of a list, `刪除` destroys), matching the dialog's promise.
- `done*` names the set (`Cmdr 寫入的全部`), `some*` only counts: ❌ don't collapse them. `done*` has an `=1` arm
  (`那個項目`): `全部 1 個` reads wrong.
- The rest names its place (`複製過去的地方` / `移動過去的地方`): a bare "still there" reads as `原處`, the opposite.
  `leftBehind` repeats `會略過沒有把握的部分`.
- `stagedLeftover.*`: `不完整副本`, cleared with `清掉` (Cmdr's own files, not the user's), and ❌ never `下次`: the
  cleanup skips files under an hour old, so an immediate retry doesn't clear them.
- `in {folder}` is a trailing, unquoted `位於 {folder}` (as `downloads.toast.inSubdir`): the row renders after
  `reversalDeleting`, so the preverbal `在…中` isn't available, and `位於` marks the folder as a place, not the target.

## An unfinished move's staging folder (`fileOperations.leftovers.stagingFolderKept`)

- `還沒完成的移動` over `未完成`: `未完成` is the failed status label (`operationLog.status.failed`).
- `沒有動它們，全都留在…裡` over `留在原處`: these files are not where they started. The files are complete and the
  user's own: ❌ not `不完整副本`, and never hint they can go. Keep `隱藏資料夾`: it's the one actionable fact.

## Failure-sentence shapes (Apple's four) and the busy-tooltip shape (`fileExplorer.navigation.ejectBusyTooltip`, `.disconnectBusyTooltip`, `adb.disconnectBusyTooltip`)

- `無法`+verb+object`。`; `無法…，因為…。` (default); `因為…，無法…。` when the reason is the user's to fix;
  `…，因此無法…。` when a named actor blocks (`磁碟正由「%@」使用中，因此無法退出。`). Recovery lines end
  `然後再試一次。` (AP-TW).
- "Can't X while operations are in progress on this Y" → `這個 Y 上有操作正在進行，無法X`: the reason leads.

## metadata: `中繼資料` (`errors.listing.attributeNotFound.explanation`/`.suggestion`)

AP-TW `後設資料` and AP-HK `元數據` split, so macOS-first can't decide; MS `中繼資料` is the only form tagged for both.

## Keychain and keyring: `鑰匙圈` (`ai.secretError.*`, `fileExplorer.network.share.credentialsStoredLocally`, `servers.sheet.remember`)

`鑰匙圈` for Apple's Keychain (the app is `鑰匙圈存取`) and a Linux keyring (gnome-keyring, xfce4-session). Apple has
zero `金鑰環`. `記住在鑰匙圈中` is quoted verbatim by `servers.sheet.needsStoredSecret`.

## Repository: `存放庫` (`errors.git.*`, `onboarding.stepBeta.checklist.star`)

MS's source-control sense; `儲存庫` claimed GitHub's UI without proof. MS `儲存機制` is the abstract data store.

## Go forward: `往前` (`menu.go.forward`, `commands.navForward.label`)

Finder's Go menu `返回` / `往前` (TW = HK). System Settings' `前進` is a VoiceOver description, not a menu item.

## Classifiers and list compounds (`errors.volume.notConnected`, `main.oldWebkit.body`, `errors.mount.unsupportedProtocol`, `settings.mediaIndex.progress.local`, `commands.tabMcpAction.label`)

- `這部 Mac`, `這支手機` (AOSP: 9 `這支` / 1 `這部`), `這台電腦` for a computer in general.
- A generic list is `列表`; the fixed compounds `檢查清單`, `郵寄清單`, `資訊清單` stay.
- "Pane tab action" is `動作`, not `操作` (an operation is file work).

## Selection (`selection.*`, `menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- Recent selections `最近的選取` over `最近的選取範圍`: `選取範圍` is the SET of selected files, these rows are past
  queries. `menu.context.selection` names that set, so it's `選取範圍`.
- The buttons' tooltips front the locative (`在焦點窗格中選取這些檔案`): Chinese puts `在…中` before the verb.
- Kind `種類` (Finder's Kind criterion); `選取所有副檔名為 *.{extension} 的檔案` from TC/DC's command, with the mask
  spaced as a Latin run. `{mode}` in the recent row is spaced: it can arrive as `AI`.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- "Look inside" `查看…裡的內容` (the `askCmdr.tool.*` pairs), and `讀取` only where English says "read".
- Camera details `相機資訊` (AP Photos TW; HK's `資料` is ruled out under info), ❌ not `EXIF 資料`: the consent copy is
  plain language. A photo's place `拍攝地點`; `位置` stays a filesystem location, as Photos also splits them.
- Archive here is the general `封存檔` (zip, tar, 7z alike), not the zip-only `壓縮檔`. Title `標題`, author `作者`.
- The rail, settings intro, and consent screen make one promise in one wording: `問到某個檔案` + `查看…裡面的內容`.
  Never changes files `沒有你的同意，絕不會更動任何檔案`: `任何檔案`, not `任何東西`, since it writes its own notes.

## System Settings tokens and pane names (`errors.git.*`, `errors.provider.*`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`)

- A `{system_settings}`-style token follows the Mac's language and can arrive Latin: space both sides. In a bold path, a
  trailing `裡` attaches to the last Han pane name, never to the token.
- Untokenized panes: `Apple 帳號` (spaced against Apple's tight form), `一般`, `登入項目與延伸功能`.

## Blocking and old-OS notices (`main.oldWebkit.*`, `main.oldMacos.*`)

- Quit `結束` (AppKit), never zh-CN's `退出`. `X 以上的版本` (SystemSettings `…或以上版本`), `盡力而為`, layout
  `版面配置` (MS). "Look off" `不太對`: casual, and dodges `錯誤` / `失敗`. `Software Update` `「軟體更新」`.

## Terminal and text editor pickers (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`, `settings.behavior.textEditorApp.*`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- Terminal `終端機` (Apple localizes the app); Open terminal here `在此開啟終端機` (`在此` is a fixed compound).
- Choose an app… `選擇 App⋯` (Apple's `選擇應用程式⋯`, with the catalog's `App`); text editor `文字編輯器` (MS), a
  category, ❌ not Apple's TextEdit (that name arrives in `{app}`). System default `跟隨系統`.

## git worktrees (`errors.git.orphanedWorktree.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

`worktree` stays Latin (the `@key` says so, and every locale keeps it); the generic working tree is `工作目錄樹`. Two
names for one thing was the bug.

## Sort by relevance: `關聯性` (`fileExplorer.columns.sortByRelevance`)

AP-TW = AP-HK in three bundles (`依關聯性排序`, like `依名稱排序`). Music's `相關資訊` is "related info", not a sort
key.

## Documents and packages (`settings.archives.ooxml.*`)

`文件` (Finder's document) and bare `套件` (Finder `顯示套件內容`), wider than the `App 套件` card below it, as English
contrasts packages with app bundles.

## Servers hub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`)

- Servers `伺服器`; Places `位置` (Finder's Locations), ❌ not `地點` (geographic in Freeform and Find My).
- Last used `上次使用` (AP-HK; Finder's `上次…` shape); Never `從未使用` over bare `永不`, which AP-TW spends on future
  "never" and would read as "never use".
- Status cells keep `已…` (`已連線`, `已儲存`, `已登出`), plus `在附近找到` and `等你確認主機金鑰`. Signed out is a
  state, not a failure: no `無法`.
- Pin `釘選` / `取消釘選` (Music, AP-TW = AP-HK). Volume switcher `卷宗切換器` everywhere (`切換器` = switcher).
- Local network `區域網路` over `本機網路`: the macOS permission dialog the user must find says `區域網路`.
- A server's classifier `部` (composed on `這部 Mac`; tentative, see `review-queue.md`).

## Server sheet and host key (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

- The button `連線`, the command `連接伺服器⋯`: Apple's own split in Finder's Connect to Server dialog.
- Passphrase `密語` (Apple, many keys), ❌ not `通行密碼`. `金鑰密語`, `金鑰檔案`, `金鑰指紋` over Apple Shortcuts'
  `密鑰`: this catalog uses `金鑰` alone for cryptographic keys. English "the key" of a server always takes `主機`, or
  it reads as an API key.
- Owner `擁有者`, ❌ not `管理者` (a home NAS is usually the user's own). Reinstalled `重新安裝過`, not slang `重灌`.
- Root folder `根資料夾` over TC/MS `根目錄` (user-facing `資料夾`); start folder `起始資料夾`, ❌ not `開始位置` (TC's
  Start in, and `位置` is Places) nor `啟動資料夾` (Windows Startup). Label and every mention match exactly.
- "Won't connect" `Cmdr 不會連線到 {name}` (a standing refusal, like `servers.refusal.hostKeyRevoked`), not `停止`.
- Sheet titles quote the server: `登入「{name}」` / `編輯「{name}」`.

## Reconnecting and the nothing-to-ask line (`servers.paneState.reconnecting`, `servers.paneState.signedOutNothingToAsk`)

- A state is `重新連線` (`正在重新連線⋯` in three Apple bundles); Apple's `重新連接` is only the imperative "plug it
  back in". The user's own login key is bare `金鑰`, ❌ never `主機金鑰` (that's the server's).
- Prose retry `再試一次`; `重試` only on a short button (`servers.paneState.retryNow`).

## Pin hint, trusted host keys, and the ADB page (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.behavior.serversPinHintSeen.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.servers.*`, `settings.adb.*`, `settings.appearance.tintSmb.*`)

- `釘選到切換器` (Music `釘選到資料庫`); group `群組`, the network group quoted `「網路」群組`.
- "Getting long" `越來越長了` over Apple's `過長`, a can't-use verdict; this is a friendly nudge.
- Got it `知道了` over Apple's `瞭解`: three keys already ship it.
- Trusted X `信任的 X`, and the dated status `已信任`. Re-check `重新檢查` (English says re-); Not found `找不到`;
  `已找到，位於 {path}` puts the long path last.
- "Watching for phones" is described by effect (`手機一接上就會偵測到。`): Apple's Watching is only video watching.

## Android pane, readiness, and the USB debugging hint (`adb.connect.*`, `adb.readiness.*`, `adb.hint.*`, `adb.disconnect*`, `settings.behavior.adbHintDismissed.*`)

- Phone-side words follow AOSP, not Apple: `「USB 偵錯」`, `「允許」`, tap `輕觸` (❌ not `點一下`), all quoted because
  the user finds them on the phone.
- Not responding `沒有回應` vs didn't answer in time `沒有及時回應` (both Apple). Too old `太舊`; cable `連接線`; reseat
  `拔掉再插上`; wake `喚醒`. The link "How" `怎麼開啟？`: a bare `如何` doesn't stand.
- The aria "Disconnect {name}" `中斷連線：{name}`: `中斷與 {name} 的連線` splits the label and fails containment.
- "You stopped opening your phone" `你停止了…`, ❌ not `取消`: that's the Cancel button's label.

## A locked server identity (`servers.sheet.identityLocked`)

The hint's verbs match the controls it points to exactly (`忘記`, `加入`, never `刪除` / `新增`). "Are what name this
server" `決定了這是哪個伺服器`: `命名` would read as the sheet's Name field.

## Server row menu (`menu.network.open`, `menu.network.edit`, `fileExplorer.navigation.forgetSecretNoneToast`)

Open `開啟` as `menu.file.open` (Finder uses one verb for both senses); Edit server… `編輯伺服器⋯` as
`commands.serversEdit.label`. No saved password: `{name} 沒有已儲存的密碼。`, the siblings' `已儲存的密碼` verbatim.

## AI copy's subject: `Cmdr` / `AI`, `Ask Cmdr` for the panel only (`suggestedOps.*`, `settings.askCmdr.*`, `askCmdr.error.notConfigured`, `ai.cloud.askCmdrOverrideHint`)

- A sentence about what the AI does takes `Cmdr`; `AI` in `suggestedOps.*`, where `AI 的理由` must stay apart from
  `Cmdr 知道的事實` (the dialog separates what the model said from what Cmdr verified). ❌ Don't unify them.
- `Ask Cmdr` only for the panel and its settings section. Chatting `聊天` (the verb); the chat as a subject `對話`.

## Status-corner AI hints, `同意`, and `上下文` (`askCmdr.wake.needsFullDiskAccess`, `settings.askCmdr.proactive.description`, `settings.ai.localContextSize.label`, `settings.ai.tooltipLocal`, `askCmdr.error.localWindowTooSmall`, `askCmdr.event.contextTrimmed`)

- Approve `同意` everywhere (`批准` was aligned). `needsFullDiskAccess` contains `search.coverage.setUpFullDiskAccess`
  verbatim, as its `@key` asks.
- Context `上下文`, `上下文視窗`, `上下文大小`: Apple has no LLM sense (0 `脈絡`, 0 `上下文`), and MS id 38882 is the
  only fit; its other `context` entries are `內容` / `執行內容`. Tentative.

## AI provider setup wizard (`onboarding.cloudSetup.*`)

Placeholder `預留位置`, deployment `部署`, endpoint `端點` (all MS). The endpoint URL field is `網址` (`端點網址`);
`位址` stays an IP or network address. Pull (`ollama pull`) `提取` (MS 3 of 4).

## The Dock nudge and the Dock menu (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`, `menu.dock.*`)

- `Dock` and `Finder` stay Latin (Apple never translates them). Configuration profile `設定描述檔` (AP-TW = AP-HK) over
  MS `組態設定檔`. No Thanks `不，謝謝` (Apple).
- On the Dock, pin is `保留在 Dock 上` and unpin `從 Dock 中移除` (Dock.app's own menu items), ❌ not `釘選`: that's
  Cmdr's own UI, and `unpinNote` sends the user to that exact Dock item.
- The four result messages never say `失敗`; `addedButDockDidNotRestart` is `只是…還沒…` (the icon is placed, only the
  redraw is pending).
- `開啟 Cmdr` over Dock.app zh-TW's `打開` (outlier rule), unquoted like `隱藏 Cmdr`. `前往資料夾⋯` (Finder's Go menu)
  stays distinct from `前往路徑⋯`. `{name}（{parent}）` as the catalog's other disambiguating rows.

## Show in Finder takeover (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`, `settings.revealHandler.notProductionBuild`)

`「在 Finder 中顯示」` quoted, as the settings card. "For a while now" `有一陣子了`, ❌ never a day count. The
first-takeover notice explains, it doesn't apologize. Dev and test builds `開發版和測試版`, a released copy
`正式發行的 Cmdr 版本` (MS), ❌ not `建置` / `組建`.

## Onboarding checklist, step count, and toggle summaries (`onboarding.stepBeta.*`, `onboarding.stepOptional.*`, `onboarding.moreAbout`, `onboarding.wizard.*`)

- GitHub star stays `star`, counted `顆星` (MS's GitHub-sense entry keeps it; its other `star` entries are other
  senses). Like `按讚` (MS). Mailing list `郵寄清單`, ❌ not `電子報` (newsletter). Sign up `註冊`. Bug `問題`, not
  `錯誤`.
- More about X `進一步了解「{topic}」` (AP-HK): the brackets mark a UI name and settle Latin-or-Han spacing at once.
- The `<alpha>` badge is `標記`, ❌ not `徽章` (achievements) nor `標籤` (tag). `「區域網路」` keeps `取用` outside the
  brackets. `{mandatory}+1` is literal.
- Fetch (the viewer pulls a file to a temp copy) `取得`, ❌ not Finder's `下載` (a phone or archive isn't a download).
  Byte progress `{doneText} / {totalText}`, spaced because the units hold spaces.

## The phone's stale-index hint (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

Copies the drive family's wording, only the cause changes: the phone `不會…告訴 Cmdr` (❌ not `通知`, the macOS
notification word), and ❌ never mention a disconnect. `Cmdr 會讓索引跟上自己做的變動`, without an unpromised `即時`.

## Mount and share-list sentences (`errors.mount.*`, `errors.shareList.*`, `fileExplorer.pane.directConnectionShareGoneToast`, `servers.refusal.accountNotPermitted`)

Share `共享資料夾` (NetAuthAgent says bare `共享`, but this catalog's noun takes `資料夾`); guest `訪客`; unreachable
`連不上`; credentials didn't work `不管用`; package `套件`; a Linux distribution `發行版` (tentative); something went
wrong `出了點狀況`.

## A share the server says doesn't exist (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

The one case where waiting can't help, so no temporary tone (no `現在`, no `再試一次`): `這種情況等下去也不會好`.

## A drive gone mid-work (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.*`, `errors.write.moveNotConfirmed.*`)

- The dialog title says `中斷連線`, its body `斷線了`: the family's existing split, not drift. In progress
  `正在中斷連線`; already gone `中斷了連線`.
- The sided bodies never label source or target: which name sits where tells it. `把 {total} 個檔案中的 {done} 個…`; the
  rest is placed explicitly (`還在磁碟機上`); `你原本的檔案`, `什麼都沒被丟掉`.
- A move that couldn't be confirmed is ❌ never a failure (`無法確認這次移動`): Cmdr kept the originals on purpose, so
  `把你原本的檔案留在原處沒有動` keeps Cmdr as the subject. `原處` is right here: those files never left.

## The favorites menu (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- Three look-alikes stay apart: `喜好項目選單` (this menu), `卷宗切換器`, `指令面板`. ❌ Never call this one `面板`,
  `清單`, or `切換器`.
- Show favorites `顯示喜好項目` (`顯示` as `顯示伺服器`; `開啟` opens files). Current folder `目前的資料夾`.
- Can't add here: `…只能用在磁碟和已裝載的共享資料夾上`, with no protocol names (English avoids them too).
- `.seeFavorites`'s `=0` branch holds no digit (`查看喜好項目`).

## Eject refused, and who holds the drive (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- `{app}` / `{apps}` bare, against AppKit's quoted `「%@」`: `{apps}` arrives as one joined list that can't be quoted
  per item, and the singular must match the plural.
- `還在用` over Apple's written `正在使用中`; `macOS 還在處理這個磁碟機` over `還在使用`: English switched to "working
  with" to say nothing can be closed, only waited for. Other apps `其他 App`.
- A disk image `磁碟映像檔` (AP-TW = AP-HK), then `那個映像檔`.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

`沒有「完全取用磁碟」權限`: TW `完全取用磁碟` over HK `完整磁碟取用`, bracketed as the System Settings item. The aria
opens with the label verbatim.

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`, `errors.write.readError.title.trash`, `errors.write.writeError.title.trash`)

- No plural machinery, so each message leads with `你選的項目裡有 {count} 個，…`, correct at 1 and 7.
- The badge is `警告標記` (tentative), ❌ not `標籤`. Its text already holds brackets, so the sentence introduces it
  with a colon (`寫著：沒有「完全取用磁碟」權限`) rather than nesting `『』`.

## Online-only delete warnings (`fileOperations.delete.cloudOnlineOnlyMixedWarning`, `fileOperations.delete.cloudOnlineOnlyAllWarning`, `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Four facts stay: the Trash downloads first, so Cmdr offers only Delete, no copy lands in the Trash but the cloud keeps
its own, and the way out. Prose says `只存在雲端`; the ruled label is `僅存在雲端`. The quoted `「刪除」` is the button.

## Look-alike names on a server (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

`伺服器儲存的寫法不同`, ❌ not `拼法` (`拼` suggests pinyin, a pronunciation difference). `重音符號` /
`帶重音符號的字母` (MS, TC), ❌ not bare `重音` (stress). `現有的那一個` echoes `現有（資料夾）：`.

## The cloud AI toggle (`ai.cloudConsent.*`, `askCmdr.gate.*`)

Allow `允許` (TCC's permission button), ❌ not `同意`, which is approve. Every mention quotes the label verbatim
(`允許使用雲端 AI`). AI service `AI 服務` stays apart from provider `提供者`, as English splits them.

## Open-with and Share loading rows (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`正在尋找 App⋯`, `分享選項`, `沒有分享選項` (macOS's empty-menu shape `沒有可套用的服務`).
