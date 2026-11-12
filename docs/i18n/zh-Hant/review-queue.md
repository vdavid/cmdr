# zh-Hant review queue

Open questions for a future native Traditional Chinese reviewer (ideally one from Taiwan and one from Hong Kong). Not
translator input: every item below already ships a reasoned value, recorded in `terms.json` or `decisions.md`. Remove an
item once a reviewer settles it, and move the settled wording into `terms.json` (and `decisions.md` when the reason is
worth keeping).

## For David

- **`簡潔` vs `簡要` for the Brief view** (`brief-view`): `簡潔` is KDE Dolphin's word for Compact, and Apple's only
  `簡潔` is Writing Tools' "Make Concise". The orthodox pair renders Brief as `簡要` on an exact msgid match
  (`double-commander/doublecmd.po`: "Brief view" → `簡要`; "Show as Brief, Full or Thumbnails" →
  `以簡要、完整或縮圖方式檢視`), so the fully sourced pair would be `簡要顯示方式` / `完整顯示方式`. That changes
  shipped copy on Tier-3 evidence from a source flagged as contaminated, so it stays as shipped until David rules.
- **`總是` vs `永遠允許` on permission strings** (`always`): if the Taiwan-default rule applies literally rather than
  the both-audiences argument, "Always allow" has an exact Apple zh-TW key match at `永遠允許`.

## Terms

- **`裝載點`** (mount point): coined from Apple's `裝載` plus MS's `掛接點` pattern; unattested everywhere.
- **`共享資料夾`** for an SMB share (`network-share`): derived from Apple's `已共享` plus the folder ruling.
  NetAuthAgent says bare `共享` (`共享「%@」不存在於伺服器上。`).
- **`主機金鑰`** (`host-key`) and **`已遭洩漏`** (`compromised`): composed; Apple ships no host-key string.
- **`指令面板`** (`command-palette`): composed from two ruled halves; no source has the compound.
- **`token`** kept Latin (`token`), and **`上下文` / `上下文視窗`** (`context-window`): Apple has no LLM sense at all,
  so only MS id 38882 backs `上下文`.
- **`結束並重新打開`** (`quit-and-reopen`) and **`接受傳入連線`** (`incoming-connections`): composed from Apple's
  pieces; neither macOS string is in any bundle in the pile.
- **The server's classifier.** Counts use `部` (`{countText} 部伺服器`, composed on `這部 Mac`), `servers.json` says
  `這個伺服器`, and `errors.json` still has several `這台伺服器` / `這台主機`. Pick one for the demonstrative.
- **`USB 裝置`**, **`西歐`** (the Western encoding group), **`Glob`**, **`打錯字`**, **`軟體授權合約`**, **`發行版`** (a
  Linux distribution): unattested or composed.
- **`star`** (`github-star`): GitHub's own Traditional Chinese UI couldn't be checked (it needs a signed-in language
  switch); MS's GitHub-sense entry keeps `star`.
- **temperature** (an AI sampling setting): no ruling yet. Every Traditional source lacks the sense, and MS's
  `temperature` entry is `色溫` (colour temperature), so don't take it.
- **`除錯`** (`debug`): inherited, unsourced; the Android option stays `「USB 偵錯」` either way.
- **`完成復原`** (Finish rolling back, `operationLog.dialog.rollBackRest` =
  `fileOperations.rollbackConfirm.rollBackRest`): `繼續復原` would be blunter about not restarting, but `繼續` is spent
  on `queue.row.resume`.
- **`警告標記`** for the title-bar badge (`errors.write.trashRefused.suggestion.noFullDiskAccess`): descriptive.
- **`僅存在雲端`** (`online-only`; prose says `只存在雲端`) and the three `fileOperations.delete.cloudOnlineOnly*` rows:
  a draft never read by a human; check the long warning bar for overflow too.
- **`冷儲存空間`** (`cold-storage`, `fileExplorer.archivedFile.tooltip`): composed from MS `冷儲存體` + our `儲存空間`;
  no provider console in the pile.
- **Third-party labels quoted from memory**: Cloudflare's `帳戶 ID` / `概觀` (`servers.sheet.s3AccountId*`) and Google
  Cloud's `「互通性」分頁` (`servers.sheet.s3GcsKeyHelp`); check them against the live zh-TW dashboards.
- **`按鍵組合` vs `組合鍵`** (`key-combination`): the catalog splits 3 / 3; the ruling picks `按鍵組合`. Decide whether
  `shortcuts.conflict.*` and `globalGoToLatestShortcut.binding.description` should follow.

## Phrasing

- **`越來越長了`** (`servers.pinHint.title`), **`手機一接上就會偵測到。` / `目前不會偵測有沒有手機接上。`**
  (`settings.adb.status.*`), **`怎麼開啟？`** (`adb.hint.*`): composed, no source has these observations.
- **`來來去去`** (`settings.revealHandler.notProductionBuild`), **`停住不動了`** (the viewer's fetch stall), **`跟上`**
  (`indexing.staleDialog.bodyPhone`), **`編輯檔案時使用`** (`settings.behavior.textEditorApp.label`): judgment calls.
- **`以 {username} 身分登入` / `以訪客身分登入`** (`servers.hub.shareAccount` / `.guestAccount`) and the buttons
  `以其他身分登入⋯` / `以訪客身分瀏覽` (`fileExplorer.network.share.*`): composed; check the quiet suffix reads well
  after a name and fits the share-list header.
- **`開啟或關閉「…」`** (`commands.multiRename{MatchCase,FirstMatchOnly,…}.label`): "Turn X on or off" rows quote the
  checkbox label verbatim; the older toggle commands say `切換…`. Check the two styles sit well side by side in the
  Keyboard shortcuts list. **`起始值`** and **`依序編號 {first}、{second}、{third}⋯，顯示為 …`**
  (`multiRename.counterEditor.*`): composed for the counter popover.
- **`三項一起設定，增量用負數就會倒數`** (`multiRename.placeholderHelp.counterAll`): `倒數` for "counts down" can also
  read as a countdown timer; `數字會遞減` is the fallback. **`只取日期中的「日」`** (`.datePart`) and **`以…為例：`**
  (`.forFile`, `.forSampleDate`, `.forFirstFiles`, where `以最前面的三個檔案` dodges a `以前` misreading): composed for
  the placeholder tooltips.
- **`來自網站的影像`** (`viewer.markdown.image.remote`, `.remoteImages.notice`): "from the web" became "from websites"
  to dodge `網路` (ruled as the network, used for the LAN elsewhere) and to pair with `那些網站`. **`往前到下一份文件`**
  (`commands.viewerGoForward.label`): keeps the `往前` ruling but reads a bit stiff; `前往下一份文件` is the fallback.
  **`以排版檢視`** / **`排版`** (`viewer.toolbar.viewMode.viewFormatted`, `viewer.markdown.error`): "formatted" as
  rendered, dodging the disk sense of `格式化`.
- **`共享資料夾` for an NFS export** (`servers.sheet.nfsExport` and every `nfs*` / `refusal.export*` key): shares the
  SMB share's word, so the two protocols' folders read alike; Synology DSM zh-TW says `共用資料夾`, MS/Linux `匯出`.
- **AI 建議 batch headings and pills** (`suggestedOps.sweepWhen.*`, `.reversible.*`, `.irreversible.*`): headings end in
  `建議的` (`今天 {time} 建議的`), pills open `可以還原：` / `無法還原：`, and the reason label is `理由：`. Confirm
  they read naturally in the dialog, and that `{date} 建議的` reads right after an ISO date.
- **`篩選標記` for the pane's typed-text badge** (`commands.paneQuickFilter.description`): follows the `badge` ruling,
  but the thing is a small box holding typed text; confirm it reads right.
- **`清除「跳至」的輸入內容`** (`fileExplorer.typeToJump.clear`): bare `清除跳至` parses as a broken verb chain, so it
  leaves the `清除篩選` frame and names what goes. Screen readers only.
- **`要透過連結連到另一個伺服器`** (`*.linkTargetUnreachable*`): the plain rendering of "sits behind a link"; `連結`
  alone may read as a web link to someone who doesn't know DFS.
- **Windows-only shortcut-conflict labels** (`shortcuts.system.*`): `安全畫面` (`securityScreen`, from Microsoft's zh-TW
  keyboard-shortcuts page, which is machine-translated) and `視窗的系統功能表` (`windowMenu`, from the TBX's system
  menu) are tentative, since Windows' UI never titles either. `「開始」功能表` keeps Windows' own `功能表` against the
  `選單` ruling (key `exceptions` in `menu`). A Windows zh-TW reader should confirm all three.
- **`對應的 App` for "the associated app"** (`viewer.binaryWarning.body*`,
  `settings.fileViewer.suppressBinaryWarning.explanation`): kept from the shipped banner; Windows' own file-association
  word is `關聯`, so the Windows branch may want `關聯的 App`.
- **`這可不是按一下 {renameKey} 就能搞定的事` / `隨手改個名`** (`errors.write.homeFolderRename.suggestion.isHome*`): the
  light joke, rephrased to drop the shipped `那麼簡單` (style: no 簡單). Confirm the tone lands.
