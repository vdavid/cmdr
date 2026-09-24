# zh-Hant decisions

The rationale journal for Traditional Chinese: why a term or a phrasing was chosen, per surface. Term rulings live in
`terms.json` (keyed by `../concepts.json`), open questions in `review-queue.md`, and the style rules in `style.md`. A
translator doesn't read this file end to end: `pnpm i18n:brief` pulls the sections whose heading cites a batch key.

Source abbreviations used below and in `terms.json`: **AP-TW** / **AP-HK** / **AP-CN** = macOS Finder + AppKit +
SystemSettings in that locale; **AP live** = the installed macOS bundles read by English-key match (a 12,418-string
zh_TW corpus from `/System/Library/ExtensionKit/`, `/System/Applications`, and `/System/Library/CoreServices`, with its
zh_HK twin, widened to 112,961 English→zh_TW→zh_HK rows from every `Localizable*.loctable`; macOS 26.6.2, build 25G83,
2026-08-29; recipe in `../reference-pile/how-to-mine.md` § "No pile on this machine"); **MS** = the Microsoft zh-Hant
terminology TBX; **NAU** / **DOL** / **THU** = GNOME Nautilus / KDE Dolphin / Xfce Thunar (zh-TW); **TC** / **DC** =
Total Commander / Double Commander (zh-TW). Pile evidence verified 2026-08-29. Don't re-litigate a ruling without new
evidence of that weight.

## The three get-it-back words: undo, rollback, restore (`menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.undo`, `operationLog.dialog.rollBack`, `fileOperations.cancelRollback.*`)

Cmdr ships three "get it back" concepts, sometimes on one dialog, so they stay three words:

- **undo → `還原`** (AP-TW = AP-HK, NAU). MS says `復原`, but Apple wins, and this catalog spends `復原` on rollback.
- **roll back → `復原`** (MS). Undo and Rollback share the transfer progress dialog, so they must differ. When English
  says "undo" about a rollback act ("Couldn't undo {name}" in a rollback toast), the act decides: `無法復原`.
- **restore (get a deleted file back from somewhere that kept it) → `回復`** (AP-TW Finder:
  `Some items can't be restored.` → `部分項目無法回復。`; `以回復的卷宗來取代`). NOT `還原` (undo) and NOT `復原`
  (rollback). MS says `還原`, and Nautilus and Dolphin say `從垃圾桶還原`; both lose, because a collision inside our own
  catalog costs more.
- **put back** (trash) → `放回原處`, and old names → `改回去` (`askCmdr.renameUndo.*`): nothing moves when a name
  changes back, so it never says `放回原處` or `還原` there.

## Failure-sentence shapes (Apple's four) and the busy-tooltip shape (`fileExplorer.navigation.ejectBusyTooltip`, `.disconnectBusyTooltip`, `adb.disconnectBusyTooltip`)

For the `無法` + verb rule in `style.md` § Voice and tone:

1. `無法` + verb + object `。`: `The open file operation failed.` → `無法執行開啟檔案的操作。`
2. `無法` + verb + object `，因為` + reason `。`: the default. `無法下載軟體，因為網路發生問題。`
3. `因為` + reason `，無法` + verb `。`: when the reason is what the user must act on.
4. state phrase `，因此無法` + verb `。`: when a named actor is the blocker. `磁碟正由「%@」使用中，因此無法退出。`

Recovery lines pair as `請確定你已連接網際網路，然後再試一次。` and `當目前的操作完成後，再試一次。` (AP-TW).

"Can't <verb> while operations are in progress on this X" → `這個 X 上有操作正在進行，無法<verb>`: the reason leads and
the `無法` clause closes. `ejectBusyTooltip` (`裝置` + `退出`), `disconnectBusyTooltip` (`伺服器` + `中斷連線`), and
`adb.disconnectBusyTooltip` (`裝置` + `中斷連線`) copy it exactly.

## metadata: `中繼資料`, and why macOS-first can't settle it (`errors.listing.attributeNotFound.explanation`/`.suggestion`)

Apple splits: AP-TW says `後設資料` (Photos), AP-HK says `元數據`. macOS-first assumes Apple speaks with one voice for
Traditional Chinese, and here it doesn't, so following it would pick a form wrong for half the audience of a catalog
shipped once for both markets. MS's `中繼資料` is the only candidate tagged for both (HKG + TWN). ❌ Don't "fix" it to
`後設資料` by citing macOS-first: the rule is a tiebreaker among sources, not among Apple's own regions, and it has
nothing to say when AP-TW and AP-HK disagree.

## Keychain and keyring: `鑰匙圈` (`ai.secretError.*`, `fileExplorer.network.share.credentialsStoredLocally`, `servers.sheet.remember`)

`Keychain Access.app` is `鑰匙圈存取` in zh_TW AND zh_HK (`CFBundleName`, `CFBundleDisplayName`), and the bare word is
`鑰匙圈` (`MainMenu.loctable:825.title`). A Linux system keyring is `鑰匙圈` too: GNOME's `gnome-keyring` and `gcr`
zh_TW translations and xfce4-session all say `鑰匙圈` (verified via the GNOME commit archive and Debian sources,
2026-09-24). Apple has zero `金鑰環`; it had drifted into three `ai.secretError.*` values and was aligned. `金鑰` alone
stays the cryptographic and API key.

## Repository: `存放庫` (`errors.git.*`, `onboarding.stepBeta.checklist.star`)

MS's source-control entry (`repository` / `repo` → `存放庫`, defined as "a central storage location … files under source
control") and 19 catalog values say `存放庫`. An older tentative `儲存庫` claimed GitHub's own zh-TW UI, which nobody
could verify; the one value using it (the GitHub star checklist row) was aligned. MS's other `repository` entry,
`儲存機制`, is the abstract data-store sense.

## Go forward: `往前` (`menu.go.forward`, `commands.navForward.label`)

Finder's Go menu, the lineage match, says `返回` (`211.title`) and `往前` (`249.title`), identical in zh-TW and zh-HK.
System Settings' accessibility labels say `前進`; that is a VoiceOver description, not the menu item.

## The termbase migration's drift fixes (`errors.listing.connectionRefused.explanation`, `main.oldWebkit.body`, `settings.behavior.textEditorApp.description`, `settings.behavior.openTerminalHereApp.description`, `errors.volume.notConnected`, `settings.search.autoApply.description`, `errors.mount.shareNotFound`, `errors.shareList.protocolError`, `fileOperations.delete.cloudOnlineOnly*`, `mtp.*.retry`, `commands.tabMcpAction.label`, `fileExplorer.network.forgotPassword`)

Probing every ruling against the catalog turned up values that had quietly left their ruling. Each rule below is now in
`terms.json`, so the drift check holds it; the boundaries worth knowing:

- `這台 Mac` → `這部 Mac` (six values). A phone is `這支手機` (AOSP's classifier); a computer in general stays
  `這台電腦`.
- `點按` → `按一下` (three Settings descriptions): `點按` is macOS zh-CN's click.
- `清單` → `列表` for a generic list (share list, server list); the fixed compounds `檢查清單`, `郵寄清單`, `資訊清單`
  stay.
- The full-width semicolon `；` was in 14 values; each became `，` or `。` (`style.md` § Punctuation). The written `此`
  (`此儲存空間`, `此對話框`) became the spoken `這個`; fixed compounds (`在此`, `此刻`, `因此`) stay.
- English "internet" → `網際網路` even inside a short "Check your internet connection" (three values had `網路`).
- `移動` over `搬移` / `搬動`; `略過` over `跳過`; `跳至` over `跳到`; `加入` over `新增` for adding to a list
  (`加入快速鍵` / `已加入`); `圖像` over `圖示`; `App` over `應用程式` for an app in general; `配額` over `額度`;
  `通訊協定` over bare `協定`; `變更記錄` (changelog, as `menu.app.changelog`) over the Mainland `更新日誌`.
- "Pane tab action" is an action (`動作`), not an operation (`操作`). "Retry connection" on the MTP dialogs is a retry
  button (`重試連線`), not the reconnect state. "Forgot saved password" says `已忘記`, like its `忘記已儲存的密碼`
  siblings.
- The two online-only warnings now say `這個對話方塊` and echo the command name `設為離線可用`.

## Select / Deselect files dialog (`selection.*`)

The dialog the Select menu opens (`selection.*`, 15 keys). Verbs settled under Operations above; this is the phrasing
around them.

- **"Select these files" / "Deselect these files" (footer buttons)** · `選取這些檔案` / `取消選取這些檔案` · built
  straight on the two verbs, so the buttons agree with `menu.select.files` / `menu.select.deselectFiles` (`選取檔案…` /
  `取消選取檔案…`) and with the dialog titles · `high`
- **"… in the focused pane" (the buttons' tooltips)** · locative fronted, natural Chinese order:
  `在焦點窗格中選取這些檔案` / `在焦點窗格中取消選取這些檔案` · `high`. **A tooltip is its own sentence and need NOT
  open with its button's label**: the button's accessible name comes from the `…label` key (`QueryDialog.svelte` uses
  `primaryAction.ariaLabel ?? primaryAction.label`), and the tooltip is separate `use:tooltip` hover copy, so WCAG 2.5.3
  is satisfied by construction. English trails the scope because that is English word order; Chinese puts `在…中` before
  the verb, so front it. `焦點窗格` is what the catalog already says (`commands.navGoToPath.description`,
  `commands.favoritesAdd.description`).
- **"Press Enter to filter"** · `按 Enter 鍵篩選` · the catalog is unanimous on `按 Enter 鍵` (`search.runHint`,
  `queryUi.bar.runHint`, five `settings.*` strings) and on `篩選` (`queryUi.recent.filterPlaceholder`) · `high`. `Enter`
  stays Latin; there is no Traditional key name in the pile.
- **"recent selections" (the popover of past queries)** · `最近的選取` · the verb used as a noun, in parallel with the
  `queryUi.recent.*` twins' `最近的搜尋`; all five popover keys mirror those twins word for word with `搜尋` → `選取` ·
  `high`. ❗ **Not `最近的選取範圍`**: `選取範圍` is the SET of selected files
  (`commands.selectionSelectFiles.description` `加入選取範圍`), while these rows are past QUERIES.
- **"Matching what is shown in the list (the full path)."** · `比對的是列表中顯示的內容（完整路徑）。` · `比對` is the
  catalog's match verb (`queryUi.scope.toggle.caseSensitiveAria` `比對時區分大小寫`, `suggestedOps.fromPattern`
  `以樣式比對出來的`); `列表` per the `list (generic UI list)` ruling (❌ never `清單`); `完整路徑` is already in
  `errors.listing.nameTooLongErrno.*` · `high`
- **"Apply recent {mode} selection: {query}"** · `套用最近的 {mode} 選取：{query}` · `套用` = apply (entry above,
  `ai.local.applyContextSize`); full-width colon like `queryUi.recent.scopeSummary` (`範圍：{scope}`); spaces around
  `{mode}` because it can arrive Latin (`AI`) · `high`. `{query}` is uncontrolled user text and sits last, after the
  colon, so anything can land there.
- All 15 values differ from English, so none needs a `sameAsSourceJustification`. No apostrophes in the batch, so ICU's
  `''` rule doesn't bite here.

## Finishing an interrupted rollback (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

The Operation log gained a state: a rollback cancelled halfway leaves the row "partly rolled back", and that row's
button changes from "Roll back" to "Finish rolling back". All five values anchor on the rollback vocabulary this catalog
already ships; nothing was coined.

- **"Finish rolling back"** · `完成復原` · built on the settled `復原` (`operationLog.dialog.rollBack` `復原`,
  `rollingBack` `正在復原`, `partiallyRolledBack` `已部分復原`; the `還原` = undo / `復原` = roll back split under § The
  three get-it-back words still holds) · `medium-high`. `完成` says "carry this one to the end", not "start a fresh
  one", and it reads unambiguously beside the `已部分復原` badge on the same row. `繼續復原` ("continue") would be
  blunter about not restarting, but English says Finish rather than Continue, and `繼續` is already spent on
  `queue.row.resume`. Worth a second look if a native reviewer ever reads this batch.
- ⚠️ **`operationLog.dialog.finishRollBack` and `fileOperations.rollbackConfirm.finishRollBack` must stay
  byte-identical** (both `完成復原`): one English string, one action, the log-row button and the confirmation it opens.
  `i18n-terms` warns when one English string gets two renderings inside a locale, so never reword one alone.
- **"Finish rolling this back?"** · `要完成這項操作的復原嗎？` · same `要…嗎？` question shape and same `這項操作` as
  its sibling `fileOperations.rollbackConfirm.title` (`要把這項操作復原嗎？`) · `high`. The `完成…的復原` frame matches
  the confirming button `完成復原`.
- **The notice under the row reuses two shipped sentences verbatim** ·
  `Cmdr 能復原的都復原了，其餘的維持原樣。完成復原會再走一遍，仍然會略過沒有把握的部分。` · `維持原樣` is
  `fileOperations.rollbackConfirm.leaveAsIs` as it stands, and `略過沒有把握的部分` is lifted word for word from
  `fileOperations.rollbackConfirm.bodyUndoByDeleting` (`Cmdr 會略過沒有把握的部分，所以可能會留下一些。`), keeping the
  catalog's `略過` = skip · `high`. `再走一遍` renders "takes another pass". The sentence deliberately promises no
  complete reversal: files Cmdr can't match against its record get skipped again. No `失敗` and no `錯誤`, per
  `style.md` § Voice and tone.
- **"in {folder}"** · `位於 {folder}` · `high`. This one is a **trailing locative**, because
  `queue.row.reversalDeleting` (`正在刪除這項操作建立的東西`) and this key render as two separate elements in that fixed
  order, so Chinese's usual preverbal `在…中` order isn't available here. `位於` is the form Chinese uses to append a
  location, and two sources back it: this catalog already renders the identical English string `in {subdir}`
  (`downloads.toast.inSubdir`) as `位於 {subdir}`, and KDE Dolphin appends the same shape (zh-TW `位於 %7` in
  `%1，%2 %3 %4 %5 %6，位於 %7`; zh-CN `位于 %1` for `in location %1`). The row then reads
  `正在刪除這項操作建立的東西 位於 Backup`, where `位於` fixes the folder as a PLACE, so it can't be read as the thing
  about to be deleted, which is the bug the key exists to fix. (Dolphin zh-TW's live string for `in location %1` is
  `在位置 %1`; `位於` is shorter and is what this catalog already uses.)
- **The folder name takes no brackets** (`位於 {folder}`, never `位於「{folder}」`) · matches
  `downloads.toast.inSubdir`, and matches the bare folder names every other queue row puts in that same slot · `high`.
  ⚠️ **Deliberately unlike `de` and `es`**, which quote it (`in „{folder}“`, `en “{folder}”`). If quoting is ever
  standardized across locales, Traditional takes `「…」` per `style.md` § Punctuation, never `“…”`.
- All five values differ from English, so none needs a `sameAsSourceJustification`. No apostrophes in the batch, so
  ICU's `''` rule doesn't bite here.

## What a cancelled rollback reports (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

The toast shown after a Rollback finishes: a headline, then `leftBehind`, then a bulleted list of `reason.*` lines. It
is written as **Cmdr did the careful thing**, never as an apology, so no `失敗` and no `錯誤` anywhere in the batch
(`style.md` § Voice and tone). The whole family is anchored on `askCmdr.renameUndo.skipReason.*`, which already solved
this exact shape for the rename undo.

- **"Left … alone" (a skipped item)** · `維持原樣` · lifted from `fileOperations.rollbackConfirm.leaveAsIs` and the
  whole `askCmdr.renameUndo.skipReason.*` family · `high`. It carries "Left {name} alone" AND "Left {name} where it is"
  (`spotTaken`), because Chinese needs no separate word for the second: staying put IS 維持原樣.
- **The named-vs-counted pair shape** · `{name} 維持原樣：它…。` and
  `有 {countText} 個{count, plural, other {項目}}維持原樣：它們…。` · `high`. Copied verbatim from
  `askCmdr.renameUndo.skipReason.*.named` / `.counted`, so the two reason lists read as one feature. The leading `有`
  and the switch from `它` to `它們` are the only differences between the halves; keep it that way, and never collapse
  the pair into one plural (Chinese has only `other`, so the display choice can't live in the plural).
- **`folderNotEmpty.named` / `.counted` must stay byte-identical with their `askCmdr.renameUndo.skipReason` twins** ·
  `資料夾 {name} 維持原樣：它裡面現在有東西了。` and
  `有 {countText} 個{count, plural, other {資料夾}}維持原樣：它們裡面現在有東西了。` · `high`. The English of all four
  is one and the same string, so `i18n-terms` would warn on two renderings inside the locale. Reword neither alone.
- **"it changed after Cmdr put it there" (`drift`)** · `它在 Cmdr 放好之後有過更動` · `high`. Same frame as the rename
  twin's `它在重新命名之後有過更動`, with the rename swapped for `Cmdr 放好`. `放好` covers both branches this key
  serves (a copy WROTE the file, a move CARRIED it there) with one verb, which no more literal rendering does.
- **"something else now sits where it came from" (`spotTaken`)** · `它原本的位置已經被別的東西佔走了` · `high`.
  Deliberately parallel to `askCmdr.renameUndo.skipReason.nameTaken` (`它原本的名稱又被佔走了`): the two reasons are the
  same event on different axes, a taken NAME and a taken PLACE, so they share the `原本的…被…佔走了` frame with `名稱` →
  `位置`. `佔走` is not in the pile (Apple's only `佔` is `佔用空間`, disk usage), but it is already shipped in this
  catalog for the name case, and the sibling catalog outranks the pile on which rendering this app uses (`style.md` §
  "This is NOT a character conversion").
- **"Couldn't undo {name}" (`failed`)** · `無法復原 {name}。` · `high`. English says "undo", but the act is the
  ROLLBACK, so it takes `復原`, not the `還原` this catalog spends on Undo (see § The three get-it-back words). `無法` +
  verb is the house failure shape. **"Its drive may be disconnected or read-only"** →
  `它所在的磁碟機可能沒有連接，或是唯讀的。`, reusing the drive clause `fileOperations.trash.undoUnavailable` already
  ships (`它們所在的磁碟機沒有連接`) and the catalog's `唯讀` (`errors.listing.readOnly.*`).
- **`leftBehind`** · `Cmdr 會略過沒有把握的部分，所以這些留了下來：` · `high`. Repeats the Rollback confirmations'
  promise verb for verb, because the English does the same (`Cmdr skips anything it isn''t sure about`, in the dialog
  and in the toast): `會略過沒有把握的部分` is lifted straight from `fileOperations.rollbackConfirm.bodyUndoByDeleting`
  (`Cmdr 會略過沒有把握的部分，所以可能會留下一些。`). Full-width colon, because a bulleted list follows. ❌ Not
  `維持原樣` here: that is the reason lines' word below, and this line has to reconnect with the dialog the user just
  read.
- **"Removed …" in the headlines** · `刪除`, never `移除` · `high`. Apple splits them (`移除` = take out of a list or a
  container, `刪除` = destroy), and this really destroys files. It also has to agree with what the user was just
  promised and just watched: `fileOperations.rollbackConfirm.bodyUndoByDeleting` (`這會刪除…`),
  `transferProgress.rollbackTooltip` (`停止，並刪除…`), and `queue.row.reversalDeleting` (`正在刪除這項操作建立的東西`).
- **"Put … back"** · `放回原處` · `high`. The verb `fileOperations.trash.undone` already uses (`已把 … 放回原處。`), and
  `queue.row.reversalMovingBack` (`正在把檔案放回原處`), so the toast closes the sentence its own progress row opened.
- ⚠️ **English's definite article is what tells `doneX` from `someX`, and Chinese has no article.** `doneDeleting` /
  `doneMovingBack` (the undo managed everything) differ from `someDeleted` / `someMovedBack` (some things stayed) only
  by "the" in English. Rendered literally both pairs collapse into one Chinese sentence, and the clean-case toast would
  stop promising the destination is clear. The fix: the `done*` pair names the SET by who made it, the `some*` pair just
  counts. `已刪除 Cmdr 寫入的 {countText} 個項目。` and `已把 Cmdr 移動過的 {countText} 個項目放回原處。` against
  `已刪除 {countText} 個項目。` and `已把 {countText} 個項目放回原處。` `Cmdr 寫入的` is English's own wording in
  `doneDeleting`; `Cmdr 移動過的` extends the same device to the move headline, which English didn't need. ❌ Don't
  "simplify" the `done*` pair down to the `some*` wording: that erases the distinction the four keys exist for.
- **"The rest are still there"** · `其餘的還留在目標位置。` (after a cancelled copy) and **"The rest stayed where the
  move put them"** · `其餘的還留在移動過去的地方。` · `high`. `其餘的` is Apple's own word in almost this sentence
  (`無法拷貝一個或多個項目。是否要略過並拷貝其餘的項目？`, Finder zh-TW). ❗ Both need an explicit PLACE, because a bare
  "still there" has no Chinese equivalent that couldn't be read as `原處` = _back where they came from_, which is the
  opposite of what happened. `目標位置` is the termbase's destination term and already user-facing
  (`transferProgress.stallWaitingDestination`).
- **`fileOperations.rollbackConfirm.body` gained the family's third sentence** ·
  `這會刪除這項操作到目前為止寫入的每一個檔案。被它取代掉的檔案回不來了。Cmdr 會略過沒有把握的部分，所以可能會留下一些。`
  · `high`. The added sentence is `bodyUndoByDeleting`'s verbatim, which is the point: all four `rollbackConfirm.body*`
  keys make one promise in one wording, and `leftBehind` echoes it when the promise pays out.
- All 18 values differ from English, so none needs a `sameAsSourceJustification`. Chinese needs no apostrophe, so the
  doubled `''` in the English sources has no counterpart here.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

Sources: the live macOS 26.6.2 bundles (Photos.app `.loctable`s with zh_TW and zh_HK side by side, and the Spotlight
metadata schema at `Metadata.framework/Versions/A/Resources/zh_TW.lproj/schema.strings` + `zh_HK.lproj`), plus the
pile's AppKit, Nautilus, Dolphin, Thunar, Double Commander, and the Microsoft TBX.

- **look inside a file (the inspect tool)** · `查看…裡的內容` (`正在查看檔案裡的內容` / `已查看檔案裡的內容`;
  `可以查看你問到的檔案裡的內容` in the what's-new paragraph) · composed on the catalog's own tool-line pairs:
  `askCmdr.tool.listVolumes.*` already renders "look at" as `查看`, and `askCmdr.tool.imageFacts.*` renders "read what's
  in your photos" as `讀取你照片裡的內容` · `high`. `查看` (look at) rather than `讀取` (read) where the English says
  "look inside"; `讀取` stays for the `contentsRule` sentence, where the English says "read".
- **camera details (a photo's EXIF)** · `相機資訊` · AP Photos live TW (`No camera information` → `沒有相機資訊`; HK
  `沒有相機資料`, the HK `資料` variant this catalog already ruled against under _info_). `相機` alone is AP-TW = AP-HK
  (Photos `camera`, AppKit `NSStillCameraTemplate`), THU (`Camera` → 相機), NAU (`Camera Model` → 相機型號), MS (4×相機)
  · `high`. ❗ Not `EXIF 資料`: the consent copy is plain-language by design, and the English says "camera details".
- **where a photo was taken / a photo's location** · `拍攝地點` · composed from AP Photos live TW = HK: `拍攝` is the
  shooting verb (`Capture Date` → `拍攝日期`, `拍攝日期為…`), and `地點` is Photos' word for a place in a photo (`Place`
  / `Places` → `地點`, `你照片中的地點`) · `high`. ❗ Not `位置`, this catalog's word for a place in the FILESYSTEM (the
  path sense; see _location_ above). Photos draws the same line, so the boundary the _location_ entry predicted holds:
  `地點` for a physical place, `位置` for a path.
- **thumbnail** · `縮圖` · reused from `terms.json` `thumbnail`; MS `thumbnail` → 縮圖 (2), THU + DC `Thumbnails`
  → 縮圖 · `high`
- **archive (in the consent copy)** · `封存檔` · the termbase's general-archive noun (AP-TW = AP-HK `Archive` → 封存,
  NAU `Archive` → 封存檔, MS `archive file` → 封存檔案), and what `askCmdr.json` itself already ships (`verbCompress` =
  `壓縮成封存檔`) · `high`. The consent copy names the general concept (zip, tar, and 7z alike), so the zip-specific
  `壓縮檔` that `settings.*` and `errors.*` use for actual zip files doesn't apply here.
- **title and author (of a PDF)** · `標題` / `作者` · AP Spotlight schema TW = HK (`kMDItemTitle` → 標題,
  `kMDItemAuthors` → 作者), NAU + DOL (`Title` → 標題, `Author` → 作者), MS (`author` → 作者 4/5) · `high`
- **text (some lines of a text file)** · `文字` (`幾行文字`, `一些文字`) · AP Spotlight schema TW = HK
  (`kMDItemTextContent` → `文字內容`); consistent with the `純文字` ruling for _plain text_ · `high`
- **a few pages of a PDF** · `PDF 的幾頁` · on the _page_ entry (`頁` counts pages; never `分頁`, the tab word);
  Spotlight `kMDItemNumberOfPages` → `頁數` TW = HK · `high`
- **the list of files inside an archive** · `封存檔裡的檔案列表` · on the _list_ ruling (`列表`, never `清單`) · `high`
- **"Cmdr never sends whole files…"** · `Cmdr 絕不會送出整個檔案、照片或縮圖。` · `絕不會` is the catalog's own
  strong-negation form for a privacy promise (`ai.cloudConsent.logsNote` `絕不會送到任何地方`, the telemetry settings'
  `絕不會傳送檔名`); `整個檔案` (whole files) replaces the retired `檔案本身：不送檔案內容`, which promised that no
  contents ever leave, a promise the new English deliberately withdraws · `high`
- The two sentences carried over from the retired `askCmdr.consent.noContents` (photo search; "nothing happens to a file
  until you approve it") are reused verbatim. Only the opener changed: `照片搜尋也是同樣的做法：` ("works the same way")
  replaces the old "is the one thing that reaches inside your images", which is no longer true.
- **"looks inside a file only when you ask about it"** (`askCmdr.empty.hint`, `settings.askCmdr.intro`) ·
  `只有在你問到某個檔案時，才會查看它裡面的內容` · the same `問到某個檔案` + `查看…裡面的內容` pair as `contentsRule`,
  so the rail, the settings intro, and the consent screen make one promise in one wording · `high`. The retired
  `絕不讀取檔案內容` / `Ask Cmdr 是唯讀的…絕不會更動任何東西` are gone: both promised more than the new English does.
- **"never changes a file without your approval"** · `沒有你的同意，絕不會更動任何檔案` · `同意` is the catalog's
  _approve_ word (`askCmdr.decision.approved`, `contentsRule` `在你同意之前`); `任何檔案` rather than `任何東西`,
  because the assistant does write its own notes · `high`
- `provider` stays `提供者` (in the reused sentence), and `Ask Cmdr` / `Cmdr` / `PDF` stay Latin and spaced. All five
  values differ from English, so none needs a `sameAsSourceJustification`; the U+2019 apostrophes in the English have no
  counterpart in Chinese.

## Menu wording, System Settings tokens, and the name-restore verb (`commands.appShowAll.label`, `commands.appHideOthers.label`, `errors.git.*`, `errors.provider.*`, `settings.indexing.enabled.description`, `askCmdr.renameUndo.undone`/`.partial`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`)

Fallout from four `en` self-inconsistency fixes. Evidence is macOS 26.6.2 (build 25G83), read live off the installed
bundles with the `.loctable` / `MenuBar.strings` recipes in `docs/i18n/reference-pile/how-to-mine.md`, 2026-08-30, plus
`zh-Hant/microsoft-terminology/CHINESE (TRADITIONAL).tbx` from the pile. (The `zh-Hant` pile folder has no macOS tier,
so the live bundles ARE the Tier-1 source here; read the `zh_TW` key of each `.loctable`.)

- **`Show all` / `Hide others` (app menu) → `顯示全部` / `隱藏其他`** · Tier 1, three independent bundles agree: Finder
  `MenuBar.strings` `300730.title`/`300729.title`, TextEdit `Edit.loctable` `517.title`/`515.title`, Preview
  `MainMenu.loctable` `150.title`/`145.title`. Both already shipped and both already match `commands.appShowAll.label` /
  `commands.appHideOthers.label`, so the `en` sentence-case fix was a restamp: Chinese has no capitalization, and the
  wording was right. · `confirmed`
- **System Settings panes via tokens in the git and provider errors** · the eight `errors.git.*` / `errors.provider.*`
  suggestions now carry `{system_settings}` / `{privacy_and_security}` / `{files_and_folders}`, the same
  runtime-resolved placeholders the `errors.listing.*` family already used, so the literals `系統設定` /
  `隱私權與安全性` / `檔案與資料夾` are gone from them. The app substitutes the pane names as the USER'S Mac shows them,
  so never hand-translate a token. Spacing: the value can arrive CJK or Latin, so keep a space on both sides of a bare
  token (`在 {system_settings} 裡`), and inside a bold path keep
  `在 **{system_settings} > 一般 > 登入項目與延伸功能**裡` — the trailing 裡 attaches to the last CJK pane name, never
  to the token. · `high`
- **Pane names the tokens don't cover** · `Apple Account` → `Apple 帳號` (`ClassKitSettings.loctable` `APPLE_ID` says
  `Apple帳號`; Cmdr adds the Latin/CJK space per style.md § Spacing), `General` → `一般`, `Login Items & Extensions` →
  `登入項目與延伸功能` (`LoginItems.appex/Localizable.loctable`). All three were already correct. · `confirmed`
- **`settings.indexing.enabled.description`: `目錄大小` → `資料夾大小`** · English switched "directory sizes" → "folder
  sizes" because `folder` is the app's user-facing word. Matches this file's own rule: prefer `資料夾` in user-facing
  copy, keep `目錄` only where the English deliberately says "directory" in a technical/path sense. · `high`
- **"Put the old names back on N files" → `已把 {countText} 個檔案原本的名稱改回去。`** (`askCmdr.renameUndo.undone` /
  `.partial`) · English used to share one sentence with `fileOperations.trash.undone` and now names the OBJECT (the old
  name). The old Chinese (`已還原 … 個檔案。`) said the FILES were restored, which is the trash action, not this one:
  nothing moves here, only the name changes back. Reuses the family's own phrasing `把原本的名稱改回去` from
  `askCmdr.renameUndo.undoing` and the `skipReason.failed.*` pair. `fileOperations.trash.undone` keeps
  `已把 … 放回原處。` · `high`
- **Email placeholder stays `you@example.com`** (`settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`,
  `onboarding.stepBeta.emailPlaceholder`) · Microsoft Traditional Chinese keeps the sample address verbatim: in
  `CHINESE (TRADITIONAL).tbx` the `en-US` term `someone@example.com` maps to a `zh-Hant` term that is the same literal
  string (`user@example.com` likewise). Compare Vietnamese, where the same source DOES localize the local part. So a
  Latin-script local part is the Chinese convention, all three keys already agree, and the existing
  `sameAsSourceJustification` stands. `example.com` is RFC 2606's reserved domain. · `high`

## `fileOperations.cancelRollback.stagedLeftover.*`（Cmdr 自己留在目標位置的殘留）

2026-09-02 新增。兩條文案，說的是 Cmdr 自己建立的工作檔案沒能從目標位置清掉。它們**不屬於** `reason.*`
清單：那邊是 Cmdr 在保護使用者的檔案，這邊是 Cmdr 自己的殘留。

- **`unfinished copy` → `不完整副本`** · `不完整` 是 Apple 對 "incomplete" 的譯法（macOS
  `LA33`：「已損毀或不完整」），`副本` 是 `NE111` 裡的名詞（「保留可恢復的副本」）· `high`
- **`at the destination` → `目標位置`** · 目錄裡已在用的詞（`conflictsUnknown`）· `high`
- **`transfer`（名詞）→ `傳輸`**
  · 目錄已這樣說（`errors.listing.deviceReconnecting.explanation`：「傳輸被取消或中斷之後」）· `high`
- 第二句用 `清掉` 而不是 `刪除`：這是 Cmdr 自己的工作檔案，不是使用者的檔案。
- ⚠️ **寫 `之後往那裡傳輸時`，❌ 絕不寫「下次」。**
  Cmdr 的清理會跳過不滿一小時的檔案，所以立刻重試並不會清掉它。給一個兌現不了的承諾，正是這條文案要消除的毛病。

## WebKit 過舊時的攔截頁（`main.oldWebkit.*`）

三條文案，在 Mac 的 Safari 過舊時代替 Cmdr 的介面顯示。它們寫在 HTML 外殼裡而不是 app 裡，所以這是那位使用者能看到的 Cmdr 的全部內容。

- **`Software Update` → `軟體更新`** · macOS 系統設定中該面板的名稱；Finder 的 Tier
  1 證據佐證了這個詞（`Apple Device Software Update File` → `Apple裝置軟體更新檔案`）· `high`。
- **`Quit` → `結束`** · macOS AppKit 的 `Quit` 鍵 → `結束` ·
  `high`。簡體用「退出」，繁體用「結束」，兩份目錄不互相轉換。
- **`Safari`、`Mac`、`15.4` 保持原樣**，兩側依 § Spacing 加空格。`Safari` 已加入 `BRAND_WORDS`。
- 面板名用直角引號 `「軟體更新」`，與目錄裡其餘繁體文案一致。

## 舊版 macOS 提示（`main.oldMacos.*`）

低於 macOS
12 的 Mac 上只出現一次的對話框：Cmdr 跑得動，但超出了測試範圍。語氣坦率輕鬆，既不是道歉也不是警告，因為應用程式確實在跑。

- **`supported` → `支援`** · macOS Finder zh-TW（`無法完成此項操作，因為不支援此項操作。`）· `high`。
- **`X and up` → `X 以上的版本`** · macOS SystemSettings zh-TW（`需要OS X %@或以上版本。`）·
  `high`。Apple 寫得緊湊，我們在拉丁字元兩側加空格 (`style.md` § Spacing)。
- **`best effort` → `盡力而為`** · pile 裡沒有對應詞條（只有網路 QoS 的定義），但這是現成的中文說法 · `high`。
- **`layout` → `版面配置`** · Microsoft zh-Hant 的標準譯法 · `high`。
- **`look off` → `不太對`** · 口語，且避開語氣規則禁止的「錯誤」「失敗」。
- **最後一句是 David 的第一人稱**，仍用 `你`，與 `onboarding.stepBeta.greeting` 一致。

## 復原按鈕的兩條提示（`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`、`.rollbackAlreadyLandedTooltip`）

新介面：按鈕提示現在說清這一次復原會對檔案做什麼；跨檔案系統的移動一進入最後一步（所有檔案都已抵達目標位置，正在移除原檔案），按鈕就會關掉。

- **`rollbackTooltipStopAndMoveBack` → `停止，並把目前為止移動過的每一個檔案放回原處`** · 句式沿用同胞鍵
  `rollbackTooltip`（`停止，並…`），`放回原處` 是目錄裡已定的說法（`cancelRollback.doneMovingBack`「放回原處」）·
  `high`。❌ 不用 `刪除`：復原一次移動不會刪掉任何東西。
- **`rollbackAlreadyLandedTooltip`** · 前半句沿用 `cancelRollback.moveAlreadyLanded`
  的說法（「已經在目標位置了」），`復原` 是已定的術語（`rollbackUnavailableTooltip`），`取消`
  直接用旁邊按鈕自己的標籤（`fileOperations.button.cancel`），依目錄慣例加上引號 · `high`。
- 無 `sameAsSourceJustification`；兩個值都不含撇號。

## 「在此開啟終端機」與它的 App 選擇器（`settings.behavior.openTerminalHereApp.*`、`settings.navigationAndFileOps.card.terminal`）

新介面：`行為 > 導覽與檔案操作` 下的一張卡片，用來選這個指令要開哪個終端機 App。清單由 macOS 產生，這裡只翻譯標籤。

- **terminal（這類 App）→ `終端機`；Terminal（Apple 的 App）→ `終端機`** ·
  Apple 的繁體中文 macOS 會把名字譯出來（`在「終端機」中打開`，`zh-TW/macOS/Finder/LocalizableMerged.json` 的 `N67`
  鍵）· `high`。所以卡片標題照常翻譯，不加 `sameAsSourceJustification`。
- **Open terminal here（指令名稱）→ `在此開啟終端機`** · 沿用 Apple 的 `在「終端機」中打開`，用 `在此` 表示位置 ·
  `high`。指令本身的翻譯（選單、指令面板）必須用一模一樣的寫法。
- **Choose an app… → `選擇 App…`** · Apple 的 `Choose Application…`（`N137` 鍵）寫作 `選擇應用程式⋯`；`App` 沿用
  `terms.json` 裡已定的說法 · `high`。
- **terminal app → `終端機 App`** · 拉丁詞前後留空格，與目錄裡其餘 `App` 用法一致 · `high`。兩個值都不含撇號。

## git 的 worktree（`errors.git.orphanedWorktree.*`、`settings.fileExplorer.git.showVirtualGitPortal.description`、`fileExplorer.git.size.linkedWorktrees`）

英文把 "worktree" 和 "working tree" 當成兩個詞用，繁體中文目錄也照此分開。

- **worktree（git 的連結檢出）→ `worktree`，原樣保留** · en 的 `@key` 說明寫著 "\"worktree\" is a git term; do NOT
  translate"，`de`、`fr`、`nl`、`pt`、`vi`、`hu`、`sv` 都是原樣 · `high`。三處都用它：錯誤面板
  `errors.git.orphanedWorktree.*`（本來就是）、設定裡的
  `settings.fileExplorer.git.showVirtualGitPortal.description`、git 入口 Size 欄的
  `fileExplorer.git.size.linkedWorktrees` = `{countText} 個關聯 worktree`。此前後兩處寫作
  `工作樹`，和解釋它的錯誤面板對不上，同一件事有了兩個名字。
- **working tree（泛指工作區）→ `工作目錄樹`** · `errors.git.bareRepo`、`blobTooLarge`、 `gitDirPermissionDenied`
  裡是一般行文，維持 `工作目錄樹` 不變 · `high`。
- 拉丁詞前後留空格（`個關聯 worktree`），與目錄裡其餘拉丁詞一致；量詞仍是 `個`。`git worktree prune` 是指令，原樣。

## `Sort by relevance`：搜尋結果欄的浮動提示（`fileExplorer.columns.sortByRelevance`）

新介面：搜尋結果面板中作用中欄位標題的浮動提示。再按一次會把列還原成搜尋引擎自己的排序，最相符的排在最前面。

- **relevance（結果與搜尋的相符程度）→ `關聯性`** ·
  Apple 的 zh-TW 與 zh-HK 在三個來源上完全一致：WorkflowKit（`Relevance (WFSearchSortOrder)`）、AppStoreKit（`SEARCH_FACET_RELEVANCE`）、Automator（`%1$[關聯性]@ …`），所以〈Apple-zh-TW 離群規則〉不適用 ·
  `high`。「音樂」App 的 `相關資訊`（zh-HK 作
  `相關資料`）指的是另一個概念，相關的資料，不是排序依據，因此不採用。（在 macOS 26.6.2、版本 25G83 上以 `plutil`
  匯出隨附本地化檔案核對，2026-09-06）
- **句式 → `依關聯性排序`** · 與 `commands.json` 中同類鍵完全相同的格式（`依名稱排序`、`依大小排序`） · `high`。不需要
  `sameAsSourceJustification`，值中也沒有撇號。

## `Documents and packages`：新增的 OOXML 列（`settings.archives.ooxml.*`）

新介面：與 `Zip 壓縮檔` 同在一張卡片裡的一列，下方是 `App 套件`
卡片。這一列刻意同時涵蓋 Office 文件（.docx、.xlsx、.pptx）與應用程式套件（.jar、.apk），所以連英文原文也不點名 Office。

- **documents（檔案種類）→ `文件`** · macOS Finder（`TL6`/`GROUP_DOCUMENTS` → `文件`；種類名 `RTF文件`、
  `純文字文件`），與 `terms.json` 既有的 `document → 文件` 一致 · `high`。❗ 這裡的 `文件`
  是 document，不是簡體的 file。
- **packages（泛指，不只是 App）→ `套件`** · macOS Finder（`顯示套件內容`）· `high`。刻意用不帶 `App` 的
  `套件`，讓這一列比下方的 `App 套件` 卡片更寬，正如英文用 `packages` 對 `app bundles`。
- **連接詞 → `和`** · 目錄中「X and Y」型標籤幾乎都用 `和`（`顏色和格式`、`日期和時間`、`提示和警告`）· `high`。
- **句式 → `在 …、…、… 或 … 上按 Enter 鍵時的行為。`** · 與同類鍵 `settings.archives.zip.description`、
  `settings.archives.bundle.description` 完全相同的格式 · `high`。值中沒有撇號。

## 伺服器中心（`servers.hub.*`、`commands.servers*`、`fileExplorer.navigation.server*Toast`）

涵蓋 `servers.hub.*`、`commands.servers*`，以及 `fileExplorer.navigation.server*Toast` 這一組（2026-09-06）。

卷宗切換器裡原本叫 "Network" 的那一列改名為 "Servers"，點進去是一張表：使用者存起來的伺服器（SFTP、WebDAV、SMB）加上在附近找到的，欄位是 Name
/ Type / Address / Status / Last used，最後一列是 "Add server…"。這一列所屬的**群組**仍叫
`網路`（`fileExplorer.navigation.groupNetwork`）。

代理機上沒有參考資料堆，所以下面的詞都是照 `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this
machine"，直接從這部 Mac 上的 macOS 套件（`zh_TW.lproj` / `zh_HK.lproj`，macOS 26.6.2、build 25G83、2026-09-06）用
`plutil -convert json` 比對英文鍵得來的。

- **Servers（切換器裡的那一列、鍵盤快速鍵的區段標題）** · `伺服器` · AP-TW = AP-HK
  (`Mail/SMTPSettings.loctable:106.ibExternalAccessibilityDescription` = "Servers" → `伺服器`；`Localizable.loctable` 的
  `Server` 鍵同樣是 `伺服器`)，也是 `terms.json` 既有的 `server` · `confirmed`
- **Places（一部伺服器底下的位置清單：今天是 SMB 的共享資料夾，之後是儲存帳號的 bucket）** · `位置` ·
  Finder 側邊欄的 "Locations" → `位置`（`LocalizableMerged.strings:SD5`，TW = HK）· `high`。❗ 不用
  `地點`：那是 Freeform 和「尋找」裡的地理義（`CRLShapeLibrarianCategoryNames.loctable:Places_47`）。
- **Name / Type / Address / Status（欄位標題）** · `名稱` / `類型` / `位址` / `狀態` · 全部照英文鍵直接命中：Finder
  `LocalizableMerged.strings:N220` "Name" → `名稱`（TW = HK）、系統設定 `Localizable.loctable` 的 `Type` →
  `類型`、`Status` → `狀態`（皆 TW = HK）、`address` → TW `位址`（HK `地址`，依台灣優先取 `位址`，也和既有的
  `伺服器位址` 一致）· `high`
- **Last used（欄位標題）** · `上次使用` · AP-HK 的 `Last Used` 鍵就是 `上次使用`；AP-TW 那把鍵譯成
  `最近使用的裝置`（裝置清單專用，不通用），但 TW 的 Finder 欄位用的正是同一個 `上次…` 句式（"Last Opened" →
  `上次打開日期`）· `high`
- **Never（"Last used" 欄位裡從沒連過的那格）** · `從未使用` · AP-TW 的 "Never Played" → `從未執行`
  給出「從未+動詞」的形狀 · `high`。❗ 不用單獨的
  `永不`：AP-TW 拿它翻未來義的 "Never"（`永不允許`、`永不中斷連線`），放在「上次使用」欄裡會讀成「永遠不要用」。
- **Connected（狀態）** · `已連線` · AP-TW `Localizable.loctable:Connected`（HK `已連接`）· `confirmed`
- **Saved（狀態：存起來了但現在沒連）** · `已儲存` · AP-TW = AP-HK (`SignatureSaved` = "Saved" → `已儲存`)，也和
  `terms.json` 的 `save` = `儲存` 一致 · `high`
- **Found nearby（狀態：現在在區域網路上看得到）** · `在附近找到` · `nearby` = `附近` 是 AP-TW = AP-HK (`nearby_devices`
  "Nearby Devices" → `附近裝置`，`PERSON_DETAIL_FIND_BUTTON_SUBTITLE` "Nearby" → `附近`) · `high`
- **Signed out（狀態：因為少了密碼而斷掉的工作階段）** · `已登出` · AP-TW = AP-HK (`PROFILE_SIGNED_OUT_MAC_APP` "Profile
  (Signed out)" → `個人檔案（已登出）`)，和 `terms.json` 既有的 `sign in / signed out` 一致 ·
  `high`。❗ 這不是拒絕，也不是出錯，所以不寫 `無法`、不寫 `錯誤`。
- **Waiting for you to check the key（狀態）** · `等你確認主機金鑰` · `主機金鑰` 沿用 `terms.json`
  既有的條目；英文只說 "the key"，中文非補上 `主機` 不可，否則會讀成 API 金鑰 ·
  `high`。用第二人稱直接對使用者說話，和英文一樣。
- **pin / unpin（讓伺服器也出現在卷宗切換器裡，或拿掉）** · `釘選` / `取消釘選` · AP-TW = AP-HK（Music
  `Localizable.strings` 的 "Pin" → `釘選`、"Unpin" → `取消釘選`；Notes 的 "Pin Note" → `釘選備忘錄`
  給出「釘選+受詞」的形狀），也是目錄既有的 `menu.tab.pinTab` / `unpinTab` · `confirmed`。指令名稱裡的斜線用全形
  `／`，照 `settings.selection.recentSelections.maxCount.description` 的 `「選取／取消選取檔案」`。
- **volume switcher（挑磁碟的那個清單）** · `卷宗切換器` · `切換器`
  是目錄既有的 "switcher"（`commands.favoritesAdd.description` = `切換器的「喜好項目」`），`卷宗` 是既定的 volume ·
  `confirmed`。英文原本同一個介面有兩個名字（"volume chooser" 與 "volume switcher"），中文也照著分成 `卷宗選擇器` 與
  `卷宗切換器`；英文後來統一成 "volume switcher"，中文因此也統一成 `卷宗切換器`， `shortcuts.scope.volumeChooser` 和兩個
  `commands.pane*VolumeChooser.label` 都跟著改。訊息 KEY 仍拼作 `Chooser`，因為它對應存進設定檔的指令 id
  `pane.leftVolumeChooser` / `pane.rightVolumeChooser`，不能改名。
- **Add server…（表格最後一列）** · `加入伺服器…` · Apple 的「加入+受詞」句式（系統設定 `ADD_DEVICE` "Add Device…" →
  `加入裝置⋯`、`MainMenu.loctable` "Add Account…" → `加入帳號⋯`，TW = HK）· `high`。刪節號照目錄慣例寫
  `…`（U+2026），不跟 Apple 的 `⋯`。
- **local network（區域網路）** · `區域網路` · AP-TW 把隱私權面板的 `LOCAL_NETWORK` 鍵（"Local Network"）譯成
  `區域網路`，整份 SystemSettings 的行文也一律 `區域網路`（HK 是 `本地網絡`，台灣優先取 `區域網路`）·
  `confirmed`。整個目錄只有這一個寫法：`settings.network.enabled.description`、
  `settings.network.timeoutMode.optDesc.normal` 和 `onboarding.stepOptional.networking.desc`
  都是它。onboarding 那一條最要緊：它逐字引用 macOS 權限對話框上的標籤，而那個對話框寫的就是「區域網路」，換成別的字等於叫使用者去找一個螢幕上不存在的字。❌ 不要寫成
  `本機網路`。
- **discovery（探索）** · `探索` · 目錄既有的 `settings.network.firstTriggerDone.label` = `網路探索已啟動`、
  `settings.network.enabled.description` = `探索 SMB 伺服器` · `high`。所以 "Local network discovery is off." 是
  `區域網路探索已關閉。`
- **Turn it on in Settings（連結文字）** · `在「設定」中開啟` · 句式照目錄既有的 `shortcuts.window.editInSettings` =
  `在「設定」中編輯快速鍵`；`開啟` 是 Apple 的 "Turn on" · `high`。這裡的 Settings 是 Cmdr 自己的設定視窗，依 §
  Punctuation 加角括號。
- **伺服器的量詞是 `部`** · `{count, plural, other {{countText} 部伺服器}}` ·
  **自行組出來的**：Apple的繁體套件裡沒有帶量詞的 `伺服器`，但 `style.md` § Plurals 指定 `部`
  是 Mac 的量詞，目錄也已經寫 `這部 Mac`，而伺服器就是一部機器 · `tentative`。zh-Hant 只需要 `other` 這一支。
- 這 28 個值都不含撇號，也沒有 `sameAsSourceJustification`：每一個都和英文不同。

## 連線伺服器的表單與主機金鑰那一步（`servers.sheet.*`、`servers.hostKey.*`、`servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`、`goToPath.dialog.opensServer`/`.addsServer`、`commands.serversConnect.label`）

涵蓋 `servers.sheet.*`、`servers.hostKey.*`、`servers.paneState.signedOut` / `.signIn` / `.hostKeyChanged*`、
`goToPath.dialog.opensServer` / `.addsServer`，以及 `commands.serversConnect.label`（2026-09-07）。

這是「伺服器中心」的下一層：一張輸入伺服器（SFTP、WebDAV、SMB）的浮動表單，裡面包含第一次連線時要人確認 SSH 主機金鑰的那一步。

代理機上一樣沒有參考資料堆，所以下面的詞照 `docs/i18n/translation-learnings.md` § "Quoting a macOS UI
label"，直接從這部 Mac 上的 macOS 套件（`zh_TW.lproj` / `zh_HK.lproj`，macOS 26.6.2、build 25G83、2026-09-07）用
`plutil -convert json` 比對英文鍵得來的。最好用的兩份是 Apple 自己的「連接伺服器」對話框：
`NetAuthAgent.app/Contents/Resources/{AuthDialog,Localizable}.loctable` 和
`Finder.app/Contents/Resources/zh_TW.lproj/ConnectToWindow.strings`。

- **Connect（按鈕）** · `連線` · Finder `ConnectToWindow.strings:46.title`（TW = HK），NetAuthAgent 的 `CONNECT` 和
  `AuthDialog.loctable:600218.title` 同樣是 `連線`；目錄既有的 `fileExplorer.network.connect` 也已經是 `連線` ·
  `confirmed`
- **Connect to server…（指令名稱）** · `連接伺服器…` · Finder `MenuBar.strings:266.title`「Connect to Server…」→
  `連接伺服器⋯`（TW = HK），目錄既有的 `settings.network.permissionIntroConnectLink` 也是 `連接伺服器…` ·
  `confirmed`。❗ 動詞用 `連接` 而按鈕用 `連線`，是照 Apple 自己的分工：整句的「連接伺服器」是動作，光一顆按鈕是
  `連線`。刪節號照目錄慣例寫 `…`（U+2026）。
- **Connecting…** · `正在連線…` · NetAuthAgent `CONNECTING_TO_GENERIC` = `正在連線⋯`（TW = HK） · `confirmed`
- **Protocol** · `通訊協定` · AP-TW = AP-HK（`IP.loctable:100268.title`、`AirPortSettings.loctable:moMP`；Apple 也把
  `SSH Protocol 2` 譯成 `SSH通訊協定2`） · `confirmed`
- **hostname** · `主機名稱` · AP-TW = AP-HK（`AirPortSettings.loctable:wbHN`「Hostname」、多個 `Host Name:` →
  `主機名稱：`） · `confirmed`。`host` 本身仍是既有的 `主機`。
- **Username** · `使用者名稱` · 目錄既有的
  `errors.listing.authRequiredEauth.suggestion`（`enter your username and password again` =
  `重新輸入你的使用者名稱和密碼`）；AP-TW 的 NetAuthAgent 也一路寫 `使用者名稱`（HK 是 `用户名稱`，依台灣優先） ·
  `confirmed`
- **Guest / Connect as guest** · `訪客` / `以訪客身分連線` · `訪客` 是 AP-TW = AP-HK
  (`AuthDialog.loctable:RiA-l0-ASw.title`、`GUEST`)，整句是 `以…身分` + 上面那顆 `連線` 按鈕**自行組出來的** · `high`
- **How to connect（挑訪客或帳號的那組選項的無障礙名稱）** · `連線方式` · **自行組出來的**：上面那顆 `連線`
  加上目錄把英文 "How to X" 一律寫成 `X方式`
  的既有作法（`settings.appearance.dateTimeFormat.description`：`How to display dates and times in the file list.` =
  `檔案列表中日期和時間的顯示方式。`、`menu.view.sortBy`「Sort by」→ `排序方式`） · `high`
- **Advanced（收合起來的進階區塊）** · `進階` · AP-TW = AP-HK（數十處，含系統設定的
  `ADVANCED_PANE_TITLE`），也是目錄既有的 `settings.section.advanced` · `confirmed`
- **Browse…（開啟系統檔案選擇器的按鈕）** · `瀏覽…` · Finder `ConnectToWindow.strings:48.title`「Browse」→ `瀏覽`（TW =
  HK），同一張「連接伺服器」對話框上的按鈕 · `confirmed`
- **Keychain（單獨的那個字）** · `鑰匙圈` · AP-TW = AP-HK（`MainMenu.loctable:825.title`、`Keychain` 鍵本身）·
  `confirmed`。"Remember in Keychain" = `記住在鑰匙圈中`，是這個字加上目錄既有的
  `記住`（`settings.search.recentSearches.maxCount.label`：`Recent searches to remember` =
  `要記住的最近搜尋數`）組出來的；同一張表單的 `servers.sheet.needsStoredSecret`
  一字不差地引用這個標籤（`請開啟「記住在鑰匙圈中」`）。App 名稱仍是 `「鑰匙圈存取」`。
- **passphrase** · `密語` · AP-TW = AP-HK，數量很多且一致（`P12Password.loctable:23.title`「Enter Passphrase:」→
  `輸入密語：`、`SecErrorMessages.loctable:-25260`、DiskManagement 的一整組 FileVault 字串） · `confirmed`。❗ 不寫
  `通行密碼`，Apple 的繁體套件裡零次。
- **Key passphrase（解開 SSH 金鑰檔案的那組密語）** · `金鑰密語` · **自行組出來的**：`密語` 是 Apple 的 passphrase（上面
  `confirmed`），`金鑰` 是目錄保留給密碼學金鑰的字（見上面的 `主機金鑰` 條目） · `high`
- **Key file（要拿來登入的 SSH 私鑰檔案）** · `金鑰檔案` · **自行組出來的**，同樣是 `金鑰` + `檔案` ·
  `high`。❗ 不跟 Apple 的 `SSH密鑰` / `專用密鑰`：目錄已經在 `主機金鑰` 條目裡把 `密鑰` 排除掉了，整份目錄只用 `金鑰`。
- **fingerprint / Key fingerprint** · `指紋` / `金鑰指紋` · `指紋` 是 AP-TW = AP-HK
  (`Certificate.loctable:Fingerprints`、`CRLShapeLibrarianShapeNames.loctable:Fingerprint_372`，另外 Shortcuts 講的正是 SSH 伺服器的 fingerprint)
  · `confirmed`；加上 `金鑰` 的複合詞是自行組出來的 · `high`
- **trust（信任一把主機金鑰）** · `信任` · AP-TW = AP-HK（`Localizable.loctable:Trust`、
  `DeviceModeUnpairedViewController.loctable:ToI-Wa-f1o.title`），也是 `terms.json` 既有的條目 · `confirmed`。"Trust and
  connect" 是 `信任並連線`，"Trust the new key" 是 `信任新的金鑰`。
- **owner（伺服器的擁有者，要跟他核對指紋的那個人）** · `擁有者` · AP-TW =
  AP-HK（`PhotoLibraryServices.loctable:OWNER`、 `OID.loctable` 的 `Owner`、Spotlight
  `schema.strings:kMDItemFSOwnerUserID`） · `high`。❗ 不寫
  `管理者`：英文特意說 owner，因為家裡的 NAS 通常就是使用者自己或朋友。
- **Root folder（這部伺服器被限制在裡面的那個資料夾，Cmdr 絕不往它的上層走）** · `根資料夾` · Double Commander
  zh-TW（`Go to root directory` → `移到根資料夾`）、Thunar zh-TW 和 zh-HK（`file system root folder` →
  `檔案系統根資料夾`）、MS（`root directory` 的三個譯法之一，另兩個是 `根目錄`、`最上層資料夾`）· `high`。TC 和 MS 的
  `root folder` 條目寫 `根目錄`，但目錄在使用者看得到的地方一律取
  `資料夾`，英文也特意說 folder。❗ 標籤和每一句提到它的說明或拒絕訊息必須一字不差用同一個詞（`servers.sheet.rootFolder`、`.startFolderHelp`、
  `servers.refusal.startFolderOutsideRoot`），`起始資料夾` 同理（`servers.sheet.startFolder`、
  `servers.refusal.startFolderOutsideRoot`、`.startFolderNotFound`）。「比…更上層」的句型取自 Thunar 的
  `沒有比根目錄更上層的目錄`。
- **Start folder（開啟這部伺服器時窗格先顯示的資料夾，只能是根資料夾或它裡面的資料夾）** · `起始資料夾` ·
  **自行組出來的**：`起始` 是 Apple zh-TW 表示「從哪裡開始」的字（Finder `BulkRenameWindow` 的 `編號起始於：`、AppKit
  `在工具列起始處插入`），加上既定的 `資料夾` · `high`。❌ 不寫 `開始位置`：TC 和 DC 用它指搜尋或指令的 "Start in"，而且
  `位置` 在伺服器中心已經是 Places。❌ 不寫 `啟動資料夾`：那是 Windows 開機自動執行程式的 Startup 資料夾。
- **"Leave it empty"（欄位底下的說明）** · `留空的話，就…` · 目錄既有的
  `settings.fileOperations.adbBinaryPath.description` （`留空的話，Cmdr 會…`）· `high`
- **"your account can read it"** · `你的帳號有權限讀取` · `帳號` 見下面 § 被鎖住的伺服器身分，`讀取` 和「權限」沿用
  `fileExplorer.dirSize.noPermsTooltip`（`Cmdr 沒有讀取這個資料夾的權限`）· `high`
- **"didn't answer in time, so nothing was saved"** · `沒有及時回應，所以什麼都沒儲存` · 前半句一字不差沿用
  `servers.refusal.timedOut`（`{host} 沒有及時回應。`），"Try again in a moment" 取 `等一下再試一次`
  （`ai.translateError.timeout.body`、`errors.volume.deletePending`）· `high`
- **Reconnect automatically** · `自動重新連線` · `自動` 是 AP-TW = AP-HK 的 "Automatically"（數十處），`重新連線`
  是目錄既有的用法（`errors.listing.deviceReconnecting.title` = `正在重新連線到裝置`、`servers.paneState.reconnecting` =
  `正在重新連線到 {name}…`） · `high`。⚠️ Apple 自己在 `重新連接`（`MainMenu.loctable:Ozt-wA-9P8.title`）和
  `重新連線`（`ScreenSharing.loctable:RECONNECTBUTTON`）之間搖擺，目錄一律取 `重新連線`。
- **reinstalled（伺服器重灌過，主機金鑰才會變）** · `重新安裝過` · Apple 的 "reinstall" 一律 `重新安裝` ·
  `high`。❗ 不寫口語的 `重灌`，和目錄的書面語氣不合。
- **"{host}''s key changed"** · `{host} 的主機金鑰變了` · 直接沿用目錄既有的
  `fileExplorer.navigation.connectionTooltipNeedsHostKey`（`這個伺服器的主機金鑰變了。`） ·
  `high`。英文只說 "key"，中文一定要補 `主機`，否則讀成 API 金鑰。
- **"Cmdr stopped connecting to {name}"** · `Cmdr 停止連線到 {name}` · **自行組出來的** ·
  `high`。英文刻意不說「失敗」，中文照 § Voice 的規則也不寫 `失敗` / `錯誤`；這裡連 `無法`
  都不適合，因為 Cmdr 是「主動停下」而不是「辦不到」，所以取中性的 `停止`。
- **"Signed out of {name}"（窗格標題）** · `已從 {name} 登出` · `已登出`
  是伺服器中心那一組已經確認過的狀態詞，標題形只是把受詞補回去 · `high`
- **Sign in to {name} / Edit {name}（表單標題）** · `登入「{name}」` / `編輯「{name}」` · 動詞沿用目錄既有的
  `fileExplorer.network.signIn` = `登入`；名稱照 § Punctuation 加角括號 · `high`。英文的 `Sign in to {name}`
  沒有引號，角括號是中文這一側加的：§ Punctuation 要求檔名、選單名和設定名都用 `「…」` 框起來，伺服器名稱同理。
- **這一組有四個 `sameAsSourceJustification`**：`servers.sheet.protocolSmb` / `.protocolSftp` / `.protocolWebdav`
  （通訊協定名稱，Apple 的繁體套件也一律寫拉丁字母：`SMB密碼`、`WebDAV密碼`、`SSH通訊協定2`）和
  `.addressPlaceholder`（`nas.local` 是使用者會照打的主機名稱範例，翻了反而更難懂）。其餘 42 個值都和英文不同。

## 自動重連的窗格與「沒東西可問」那一行（`servers.paneState.reconnecting`、`servers.paneState.signedOutNothingToAsk`）

涵蓋 `servers.paneState.reconnecting` 和 `servers.paneState.signedOutNothingToAsk`（2026-09-07）。

這兩個鍵是「已登出／連線中」那組窗格的最後兩塊：一塊是連線自己斷掉、Cmdr 在退避迴圈裡自動要接回來時的標題（底下是轉圈圖示、下次重試的倒數，還有「立刻重試」／「取消」／「中斷連線」三顆按鈕），另一塊是伺服器用金鑰而不是密碼登入時，取代「登入…」按鈕的那一行說明。

代理機上一樣沒有參考資料堆，所以詞照 `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this
machine?"，直接從這部 Mac 的 `.loctable` 比對英文鍵得來（macOS 26.6.2、build 25G83、2026-09-07）。

- **Reconnecting…（標題）** · `正在重新連線…` · AP-TW = AP-HK，三份套件同一個值：「螢幕共享」的
  `ControlCommand.loctable:reconnectingMessage`、FaceTime 和「電話」的
  `RemotePeoplePicker.appex/Localizable.loctable:Reconnecting\U2026`，全部是 `正在重新連線⋯`（zh_CN 是
  `正在重新连接…`）· `confirmed`。刪節號照目錄慣例寫 `…`（U+2026），不跟 Apple 的 `⋯`。帶受詞的整句照 sibling
  `servers.paneState.connecting`（`正在連線到 {name}…`）寫成 `正在重新連線到 {name}…`。
- **`重新連線` 和 `重新連接` 的分工，Apple 自己其實有規律**
  · 上一節記的「Apple 在兩者之間搖擺」可以講得更準：進行中的**狀態**一律 `重新連線`（上面三個
  `Reconnecting…`），叫使用者**動手**把線插回去的祈使句才寫 `重新連接`（`AMPDevices.loctable` 的 "Disconnect and
  reconnect the iPhone…" → `請中斷連線後重新連接 iPhone`、「藍牙檔案交換」的 "Reconnect" 按鈕 → `重新連接`）·
  `high`。目錄一律取 `重新連線`，而這兩個鍵都是狀態，所以剛好同向。
- **key（用來登入的那把「自己的」金鑰，SSH 金鑰檔案或 ssh-agent 身分）** · 光一個 `金鑰`
  ·沿用目錄保留給密碼學金鑰的字（見上面的 `金鑰檔案` / `金鑰密語` 條目）· `high`。❗ **這裡不能寫
  `主機金鑰`**：`主機金鑰` 是伺服器亮出來給人核對的那把，這裡講的是使用者拿去登入的那把，兩者相反。同一句裡就有 `密碼`
  對照，所以不會被讀成 API 金鑰。❗ 也不跟 Apple「捷徑」的 `SSH密鑰`（`ActionKitUI.framework` 一路寫
  `SSH密鑰`、`公用密鑰`）：`密鑰` 已經在 `主機金鑰` 條目排除掉了。
- **rather than / instead of（把兩個選項擺在一起對照）** · `而不是` ·
  AP-TW 一致這樣寫（`指定的是檔案而不是檔案夾。`、`設定無線路由器使用 %dGHz 而不是 %.1fGHz`、
  `顯示指派給可點按元件的名稱，而不是你已在使用的數字`）·
  `high`。兩個對照項緊貼在同一個動詞底下（`用金鑰而不是密碼登入`），照 Apple 的排法，不要把否定的那半拖到句尾。
- **Open it again to retry（第二句）** · `重新開啟它就會再試一次。` · `重新開啟它` 直接沿用同一族的
  `servers.paneState.hostKeyChangedHint`（`再重新開啟它來核對指紋`），`再試一次` 是 `style.md` § Which Traditional norm
  wins 判給 retry 的字，Apple 的 "…then try again" 也一路是 `然後再試一次` · `high`。用 `就會…`
  直述會發生什麼，而不是用祈使句叫人做，符合英文不用允許語氣的規則。
- ⚠️ **散文裡的 retry 寫 `再試一次`，`重試` 只留給那顆短按鈕。** `servers.paneState.retryNow` = `立刻重試`、
  `.retryNowTooltip` = `立刻嘗試重新連線。` 已經出貨，按鈕求短可以留著，但 `style.md` 判的是 `再試一次`（MS 的 `重試`
  是 Windows house style），所以新的句子一律 `再試一次`。
- 這兩個值都不含撇號，也都和英文不同，沒有 `sameAsSourceJustification`。

## 釘選提示、已信任的主機金鑰，與 ADB 設定頁（`menu.network.pinToSwitcher`/`.unpin`、`servers.pinHint.*`、`settings.behavior.serversPinHintSeen.*`、`settings.section.servers`/`.adb`、`settings.summary.servers`/`.adb`、`settings.servers.*`、`settings.adb.*`、`settings.appearance.tintSmb.*`）

涵蓋 `menu.network.pinToSwitcher` / `.unpin`、`servers.pinHint.*`、`settings.behavior.serversPinHintSeen.*`、
`settings.section.servers` / `.adb`、`settings.summary.servers` / `.adb`、`settings.servers.trustedHostKeys.*`、
`settings.adb.*`，以及重新標記過的 `settings.appearance.tintSmb.*`（2026-09-07）。

代理機上一樣沒有參考資料堆，所以詞照 `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this
machine?"，直接從這部 Mac 的 `.loctable` / `.strings` 比對英文鍵得來（macOS 26.6.2、build 25G83、2026-09-07）。

- **Pin / Unpin（把伺服器留在卷宗切換器裡，或拿掉）** · `釘選` / `取消釘選` · AP-TW = AP-HK，「音樂」的
  `Localizable.strings:3b4vzpvjdc`「Pin」→ `釘選`、`xj54uhvn95`「Unpin」→ `取消釘選`（同一份檔案裡另有 58 組 `釘選X` /
  `取消釘選X`）· `confirmed`。目錄既有的 `menu.tab.pinTab` / `unpinTab` 也是同一組。
- **Pin to X（釘到某個地方）** · `釘選到 X` · AP-TW `Localizable.strings:q9rfaw0pw6`「Pin Music to Your Library」→
  `將音樂釘選到資料庫`，以及 `ybpw6kzxu6`「You can pin up to %S things to your Library」→
  `你可以將最多%S個項目釘選到資料庫` · `high`。所以 `menu.network.pinToSwitcher`「Pin to switcher」是
  `釘選到切換器`：右鍵選單裡受詞已經明擺著（那一列就是伺服器），中文照英文省略。
- **switcher** · `切換器` · AP-TW = AP-HK（`Localizable.loctable:App Switcher` → `App切換器`、 `calendar view switcher`
  → `行事曆檢視區切換器`）· `confirmed`。目錄既有的 `卷宗切換器` 因此站得住腳。
- **group（切換器裡的一段分組標題）** · `群組` · AP-TW = AP-HK（Finder `LocalizableMerged.strings:TL29`「Group」→
  `群組`、`Localizable.loctable:Groups` → `群組`）· `high`。切換器裡那一段的標題本身仍是
  `網路`（`fileExplorer.navigation.groupNetwork`），提示文案照 § Punctuation 加角括號寫成 `「網路」群組`。
- **"is getting long"** · `越來越長了` · **自行組出來的**：`愈來愈` 和 `越來越`
  在 Apple 的繁體語料裡都是零次（那份語料是 UI 標籤，不是這種觀察句），但 `越`
  是目錄自己一路在用的字（24 處「數值越高，…越…」，`愈` 零次），而 `越來越長` 是台灣現代口語的標準說法 ·
  `medium-high`。❗ 不寫 Apple 的 `過長`：那是 `名稱過長` 這種「太長以致不能用」的判定語氣，這裡只是友善提醒，不是警告。
- **Got it（收下提示的那顆按鈕）** · `知道了` · **刻意不取 Apple 的
  `瞭解`**（`Localizable.loctable: NOW_PLAYING_SCROLLING_TIP_DONE_BUTTON_TITLE`「Got It」→ TW `瞭解` / HK
  `明白`）：目錄已經在 `ai.toast.gotIt`、`main.oldMacos.gotIt`、`updates.moveToApplicationsDialog.gotIt` 三處寫
  `知道了`，`i18n-terms` 會對同一句英文的兩種寫法示警，同一個名字勝過更好的名字 · `high`。
- **Trusted X（已經被使用者信任的東西）** · `信任的 X` · AP-TW = AP-HK
  `Localizable.loctable:8021X_PROFILE_TRUSTED_SERVER_LABEL`「Trusted servers」→ `信任的伺服器`，
  `8021X_PROFILE_TRUSTED_CERTIFICATES_LABEL`「Trusted certificate」→ TW `信任的憑證` · `high`。所以卡片標題
  `settings.servers.card.trustedHostKeys` 是 `信任的主機金鑰`。
- **Trusted（日期前面的那個狀態字，讀作「已信任 2026-09-07」）** · `已信任` · 照 `style.md` § "Notes and
  decisions" 裡「表格格子裡的狀態字保留 `已…`」那一條，和伺服器中心的 `已連線` / `已儲存` / `已登出` 同一族 · `high`。
- **Forget（單獨那顆按鈕）** · `忘記` · AP-TW = AP-HK `AirPortSettings.loctable:deviceNotFound.forget`「Forget」→
  `忘記`，也是 `terms.json` 既有的 forget 條目 · `confirmed`。和 `menu.network.forgetServer`（`忘記伺服器`）同一個字。
- **"Forget this key?"（確認對話框標題）** · `要忘記這把主機金鑰嗎？` · 句式直接照 AP-TW
  `Localizable.loctable:REMOVE_ONE_NETWORK_TITLE`「Forget Wi‑Fi Network "%@"?」→ `要忘記「%@」的Wi‑Fi網路設定嗎？` ·
  `high`。Apple 寫 `此`，我們照 `style.md` § "Voice and tone" 取口語的 `這`；`把` 是目錄行文一路給 `金鑰`
  用的量詞。❗ 英文只說 "key"，中文照 `主機金鑰` 條目的規矩非補 `主機` 不可，否則讀成 API 金鑰。
- **fingerprint（在這一組裡）** · `金鑰指紋` · 沿用上一節已定的 `指紋` / `金鑰指紋` · `high`。英文的 "its
  fingerprint" 指的是伺服器那把金鑰的指紋，中文補上 `金鑰` 才不會被讀成別的。
- **Not found（狀態值）** · `找不到` · AP-TW = AP-HK，三份套件同值（`AirPortSettings.loctable:placeholder.notfound`、
  `MainMenu.loctable:100386.title`、`Localizable.loctable:Not found.`）· `confirmed`
- **Re-check（狀態旁邊那顆按鈕）** · `重新檢查` · AP-TW = AP-HK `fsck_appex.loctable:Rechecking volume.` →
  `重新檢查卷宗。` · `high`。`再次檢查`（`ConnectionDoctor.loctable:100017.title`「Check Again」，TW =
  HK）是次選，但英文寫的是 "Re-check"，`重新` 才對得上 `re-`。`settings.adb.install.intro` 裡引用按鈕時寫
  `「重新檢查」`，兩者必須一字不差。
- **"Found at {path}"（狀態值）** · `已找到，位於 {path}` · `位於`
  是目錄既有的後置地點詞（`queue.row.reversalInFolder`、 `downloads.toast.inSubdir`），`已找到` 和同胞的 `找不到` 成對 ·
  `high`。路徑很長且會換行，所以刻意讓 `{path}` 落在句尾，不用中文慣常的前置 `在…找到`。
- **detect（偵測到接上的手機）** · `偵測到` · AP-TW = AP-HK，語料裡 234 列（Finder `PE102.1`「has been detected」→
  `偵測到`、`PE43`「can't be detected」→ `無法偵測到`），也是目錄既有的 `settings.summary.mtp`（`透過 USB 偵測…`） ·
  `high`。
- **"Watching for phones." / "Not watching for phones right now."** · `手機一接上就會偵測到。` /
  `目前不會偵測有沒有手機接上。` · **自行組出來的**：Apple 的 `Watching` 全是「觀看」影片義（`Continue Watching` →
  `繼續觀看`），這個訂閱義在繁體語料裡沒有對應詞 · `medium-high`。照 en 的 `@key` 指示，兩句都不提 ADB
  server、訂閱或 socket，只講使用者需要知道的事：手機接上會不會被看到。
- **USB debugging** · `「USB 偵錯」` · 沿用 `terms.json` 的 `usb-debugging`；`settings.summary.adb` 因此是
  `瀏覽已開啟「USB 偵錯」的 Android 手機。`，和 `settings.fileOperations.adbEnabled.description` 一字相同 · `high`
- **Android platform tools（英文自己省掉 SDK 的那個短寫）** · `Android 平台工具` · `terms.json` 的
  `android-platform-tools` 判的是 `Android SDK 平台工具`，這裡英文自己寫 "the Android platform tools"，中文照著省 ·
  `high`
- **"Look for adb the usual way"（空欄位的預留文字）** · `用一般的方式尋找 adb` · 一字不差沿用
  `settings.fileOperations.adbBinaryPath.description`（`Cmdr 會用一般的方式尋找 adb`）· `high`
- **Browse…（開檔案選擇器的按鈕）／Choose the adb command（選擇器標題）** · `瀏覽…` / `選擇 adb 指令`
  · 兩者都是上一節已確認的條目（Finder `ConnectToWindow.strings:48.title`「Browse」→ `瀏覽`；Apple 的
  `Choose Application…` → `選擇應用程式⋯`，`選擇` = 挑一個東西）· `confirmed`。刪節號照目錄慣例寫 `…`（U+2026）。
- **`settings.section.servers` / `.adb` 的括號** · `伺服器（SFTP、WebDAV）` / `Android（ADB）` · 全形括號依 §
  Punctuation，句內短列表用 `、`（英文用的是逗號，不是 `settings.section.mtp` 那種斜線）· `high`。`Android`、`ADB`、
  `SFTP`、`WebDAV` 都是 BRAND_WORDS，原樣保留；全形括號不與被包住的字之間加空格。
- **重新標記過的伺服器窗格著色** · `settings.appearance.tintSmb.label` = `為伺服器窗格著色（SMB、SFTP、WebDAV）`、
  `.description` = `為顯示 SMB 共享資料夾、SFTP 伺服器或 WebDAV 伺服器的窗格加上的背景著色。` · 句式一字不差照同胞的
  `tintMtp.description`（`為顯示 Android、Kindle 或相機裝置的窗格加上的背景著色。`），`共享資料夾` 是 `terms.json`
  既有的 SMB share · `high`。英文從只講 SMB 擴成三種通訊協定，舊值的 `SMB/網路` 已經不對。
- 這 29 個值都不含撇號（中文不需要，`''` 規則咬不到），也都和英文不同，所以沒有 `sameAsSourceJustification`。`menu.*`
  兩個鍵是 RAW 家族，值裡本來就沒有撇號和 ICU 結構。

## Android 手機的連線窗格、就緒提示與那一行 USB 偵錯提示（`adb.connect.*`、`adb.readiness.*`、`adb.hint.*`、`adb.disconnect*`、`settings.behavior.adbHintDismissed.*`）

涵蓋 `adb.connect.*`、`adb.readiness.*`、`adb.hint.*`、`adb.disconnect*`，以及
`settings.behavior.adbHintDismissed.*`（2026-09-07）。

這一輪有兩類來源。macOS 的詞照 `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?"，直接掃這部 Mac 的
`.loctable` 比對英文鍵（macOS 26.6.2、build
25G83、2026-09-07）。**手機上那幾個字的權威來源不是 Apple 而是 Google**：使用者得在自己手機上找到那顆開關和那顆按鈕，所以字要跟 Android 自己的繁中一模一樣，來源是 AOSP 的
`values-zh-rTW`（`frameworks/base/packages/SettingsLib` 和 `packages/SystemUI`，`refs/heads/main`，2026-09-07 取得）。

- **USB debugging** · `「USB 偵錯」` · **AOSP 直接證實了 `terms.json` `usb-debugging` 原本靠 Google 文件下的判斷**：
  `SettingsLib/res/values-zh-rTW/strings.xml` 的 `enable_adb` 就是 `USB 偵錯`（同檔另有
  `clear_adb_keys`「撤銷 USB 偵錯授權」、`adb_warning_title`「允許 USB 偵錯嗎？」）· `confirmed`（原本
  `high`）。角括號照舊：這是使用者要在手機上找的標籤，和 `settings.fileOperations.mtpEnabled.description` 的
  `「設定 > USB 偏好設定」`、`「檔案傳輸」` 同一個處理方式。
- **Allow（Android 自己那個「允許 USB 偵錯嗎？」對話框上的按鈕）** · `「允許」` · AOSP
  `SystemUI/res/values-zh-rTW/strings.xml` 的 `usb_debugging_allow` = `允許`，標題 `usb_debugging_title` =
  `允許 USB 偵錯嗎？` · `confirmed`。加角括號，理由和上面那條一樣：這是手機螢幕上的字，不是 Cmdr 的按鈕。
- **tap（在手機上點一下）** · `輕觸` · AOSP zh-rTW 通篇如此（`bluetooth_devices_card_off_summary`「Tap to turn on」→
  `輕觸即可開啟`、`security_settings_remoteauth_enroll_introduction_animation_tap_notification`「Tap the notification」→
  `輕觸通知`）· `high`。❗ 不寫 macOS 的 `點一下`：這個動作發生在 Android 手機上，字要跟手機一致。
- **Android platform tools** · `Android 平台工具` · 沿用上一節既有的條目（英文自己省掉 SDK，中文照著省）· `high`
- **phone 的量詞** · `這支手機` · AOSP zh-rTW 自己的用法（`packages/apps/Settings` 9 次 `這支手機` / 1 次
  `這部手機`，SystemUI 1 次 `這支手機`）· `high`。Mac 仍照 `style.md` 寫 `這部 Mac`，兩個量詞各有各的來源。
- **"is not responding"** · `沒有回應` · AP-TW live 在四份以上套件裡一致（`ABStrings.loctable`「The %@ server “%@” is
  not responding.」→ `「%@」伺服器「%@」沒有回應。`、`HFLocalizable.loctable`「This accessory is not responding.」→
  `此配件沒有回應。`）· `confirmed`。❗ 和 `didn't answer in time` 分開：後者照目錄既有的
  `servers.refusal.timedOut`（`沒有及時回應`）寫，也對得上 Apple 的 `未及時回應`（`FoundationErrors.loctable`）。
- **"too old"** · `太舊` · AP-TW live 十餘列一致（`Errors.loctable`「The device OS is too old for the installed version
  of iTunes.」→ `裝置的OS版本太舊，不適用所安裝的iTunes版本。`）· `high`。Apple 的句式是 `無法…，因為…太舊`，我們照 §
  Voice 的 `無法` 失敗句型寫成 `…版本太舊，Cmdr 無法瀏覽。`
- **cable** · `連接線` · AP-TW live `Localizable.loctable`「Either the cable for %@ is not plugged in…」→
  `%@的連接線沒有接上電源…` · `high`。目錄既有的 `USB 連接線`（`settings.fileOperations.mtpEnabled.description`）同源。
- **reseat the cable** · `把連接線拔掉再插上` · 目錄既有的 `errors.provider.macDroid.transient`（`拔掉再插上 USB 線`）和
  `mtp.permissionDialog.helpText`（`拔除並重新插上裝置`）·
  `high`。英文的 "reseat" 是一個動作，中文照目錄拆成「拔掉再插上」，不自創 `重新插拔`。
- **wake (a screen)** · `喚醒` · AP-TW live（`BatteryUI.loctable`「Start up or wake」→ `開機或喚醒`、
  `DIErrors.loctable`「wake failed」→ `無法喚醒`）· `high`
- **How（那行提示末端的連結）** · `怎麼開啟？` · **自行組出來的** · `medium-high`。Apple 只有 "How to X" →
  `如何X`（`Localizable.loctable`「How to pair」→ `如何配對`），單獨一個 `如何`
  在中文站不住；英文這裡是口語的「那要怎麼做？」，所以取口語的 `怎麼`，動詞直接回收前一句的 `開啟`
  讓連結指向明確。問號是中文需要的，英文沒有。
- **Dismiss** · `關閉` · 沿用既有條目（AP live 13 個 `Dismiss` 鍵全是 `關閉`，TW = HK）· `high`。目錄裡已有八個
  `Dismiss` = `關閉`，`i18n-terms` 會盯同一句英文，所以 `adb.hint.dismiss` 非得同字不可。
- **"Disconnect {name}"（無障礙名稱）** · `中斷連線：{name}` · **一字不差沿用
  `fileExplorer.navigation.disconnectPlaceAriaLabel`** · `high`。兩個鍵的英文完全相同，`i18n-terms`
  會要求同一個譯法；自然的中文 `中斷與 {name} 的連線` 會把標籤切成兩半，過不了 `*Aria`
  包含規則，所以照冒號句式寫（`style.md` 裡 `*Aria` containment pairs 那一節已經記了這條）。
- **"Can't disconnect while operations are in progress on this device"** · `這個裝置上有操作正在進行，無法中斷連線`
  ·照既有的 `這個 X 上有操作正在進行，無法<verb>` 句型（`fileExplorer.navigation.ejectBusyTooltip` 是 `裝置` +
  `退出`，`disconnectBusyTooltip` 是 `伺服器` + `中斷連線`，這裡是 `裝置` + `中斷連線`）· `high`
- **Open Settings（窗格旁邊那顆按鈕）** · `開啟設定` · 目錄既有的 `commands.appSettings.label` 和
  `commands.handler.openTerminalHere.openSettings`（英文是小寫的 "Open settings"）· `high`。英文大小寫不同所以
  `i18n-terms` 不會比對，但同一顆功能的按鈕沒有理由兩種寫法。
- **"the whole filesystem"** · `整個檔案系統` · 目錄既有的 `settings.fileOperations.adbEnabled.description`
  （`存取它的整個檔案系統`）和 `settings.section.fileSystems`（`檔案系統`）· `high`
- **"Waiting for you to allow…"（就緒狀態的浮動提示）** · `正在等你允許…` · `正在…`
  是目錄一路在用的進行式句型（`servers.paneState.connecting`），第二人稱的等待句在伺服器中心也已經有一個（`等你確認主機金鑰`）·
  `high`。英文沒有句號，中文也不加。
- **"as soon as you do"（輕觸完就自己往下走）** · `你一輕觸，Cmdr 就會…` · 目錄既有的
  `settings.adb.status.watching`（`手機一接上就會偵測到。`）已經在用 `一…就…` 這個句型 · `high`。
- **`settings.behavior.adbHintDismissed.*`（內部旗標，不會出現在 UI）** · `已關閉「USB 偵錯」提示` /
  `是否已關閉那則建議開啟「USB 偵錯」的一次性提示。` · 句式一字不差照同胞的 `settings.behavior.serversPinHintSeen.*`
  （`已顯示「網路」群組過長的提示` / `是否已顯示過關於取消釘選伺服器的一次性提示。`）· `high`。英文從 `Seen` 換成
  `Dismissed`，中文跟著從 `已顯示` 換成 `已關閉`，和上面的 Dismiss 條目同字。
- 這 19 個值都不含撇號（中文不需要，ICU 的 `''` 規則咬不到），也都和英文不同，所以沒有 `sameAsSourceJustification`。
- **`You stopped opening your phone.`** · `你停止了開啟手機。`（`adb.connect.cancelled`）· 動詞照平行鍵
  `search.coverage.walk.cancelled`（`你停止了這次搜尋`）和 `errors.volume.cancelled` 取 `停止` · `high`。❌ 不寫
  `取消`：`取消`
  是按鈕的標籤（`fileOperations.button.cancel`），寫成「你取消了…」會像在指那顆按鈕，而不是在說發生了什麼。`停止`
  直接帶動詞短語是目錄既有的寫法（`停止建立索引`、`停止連線到 {name}`、`停止搜尋`）。 `開啟`
  是已定的開手機動詞（`adb.connect.waitingHint`）；主詞已經是 `你`，所以不再寫 `你的手機`。

## 被鎖住的伺服器身分（`servers.sheet.identityLocked`）

編輯已儲存的伺服器時，變灰的「位址」與「使用者名稱」兩個欄位底下的兩行說明。

- **`the account`（用來登入伺服器的那個欄位）→ `帳號`** · 目錄裡已有同一個意思的用法（`errors.json` 六處、
  `onboarding.json` 一處），也和 `Apple 帳號` 的既定寫法一致 · `high`。
- **這行提示裡的動作詞必須和它指向的按鈕一字不差**：`忘記` 取自 `menu.network.forgetServer`（「忘記伺服器」）， `加入`
  取自
  `servers.sheet.addTitle`（「加入伺服器」）。換成近義詞（「刪除」「新增」）會把讀者送去找一個不存在的選單項目。❗ 特別留意
  `加入` 不能寫成 `新增`：Apple 的「加入＋受詞」句式是這個目錄已經定下的寫法。
- **`are what name this server` → `決定了這是哪個伺服器`**
  · 表單本身另有「名稱」欄位（`servers.sheet.name`），所以這句不能用「命名」：會被讀成在講那個標籤。用「決定了這是哪個」才是原意（這兩個值就是這台伺服器本身）·
  `high`。
- 量詞沿用 `servers.json` 裡已有的 `這個伺服器`（三處），而不是 `errors.json` 的 `這台伺服器`。

## 本來就沒有密碼可忘記時的提示（`fileExplorer.navigation.forgetSecretNoneToast`）

- **`There was no saved password for {name}.` → `{name} 沒有已儲存的密碼。`**
  · 「已儲存的密碼」一字不差沿用已出貨的三個兄弟鍵（`menu.network.forgetSavedPassword`、`fileExplorer.navigation.forgetSecretConfirmTitle`
  = `忘記已儲存的密碼`，`.forgetSecretConfirm` = `要忘記 {name} 已儲存的密碼嗎？`，`.forgetSecretRefusedToast`）·
  `high`。使用者剛從那個確認對話框過來，用詞必須一致。
- **`{name}`
  放主語位置最自然**，也避開了「為 {name} 儲存的密碼」這種要補介詞的說法。中文不標時態，英文的過去式由「沒有」承擔。占位符後面留一個半形空格（§
  style.md 的拉丁占位符間距規則）。
- 不用「失敗」「錯誤」：什麼都沒出錯，這正是這個鍵存在的理由。
- 這台機器上沒有參考語料庫（主 clone 裡也沒有 `_ignored/i18n/`），所以這條決定依據的是已出貨的目錄和`terms.json`。

## 重試總時長、主機金鑰標題，以及 Android 的「允許」按鈕（`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`、`servers.paneState.hostKeyChanged`、`adb.readiness.waitingForAuthorization`）

- **`{seconds}`/`{minutes}`
  現在是帶兩個占位符的 ICU 複數區塊**（`servers.paneState.retryTotalSeconds`、`.retryTotalMinutes`）：`{seconds}`
  只負責選分支，使用者讀到的是 `{secondsText}`，也就是已依語言格式化好的數字。中文只有 `other` 一個類別（CLDR，§
  style.md），所以每個區塊只寫一個分支，但外層的 `{…, plural, other {…}}` 殼一定要留著，否則占位符會跟英文對不上 ·
  `high`。
- **兩個值都是
  `servers.paneState.retryKeepsTrying`（`會持續嘗試，總共 {duration}。`）的句子零件**，所以不帶介詞也不帶句號；`秒`、`分鐘`
  沿用 `indexing.eta.*` 的寫法，占位符和漢字之間留一個半形空格 · `high`。
- **`Cmdr won't connect to {name}` → `Cmdr 不會連線到 {name}`** · 「不會連線到」一字不差沿用兄弟鍵
  `servers.refusal.hostKeyRevoked`（`Cmdr 不會連線到它。`）。英文從 “stopped
  connecting”改成持續性的拒絕，所以拿掉「停止」，那讀起來像是中斷了一次嘗試 · `high`。
- **`Allow` 是 Android 自己的按鈕 → `「允許」`**，一字不差取自
  `adb.connect.unauthorized`（`請查看你的手機，然後輕觸「允許」。`），連引號和動詞「輕觸」一起沿用，這樣使用者在螢幕上能對到同一個詞·
  `high`。
- 這台機器上沒有參考語料庫（主 clone 裡也沒有 `_ignored/i18n/`），所以這條決定依據的是已出貨的目錄和`terms.json`。

## 伺服器列的右鍵選單：`開啟` 與 `編輯伺服器…`（`menu.network.open`、`menu.network.edit`）

- **`Open`（在伺服器列上）→ `開啟`**（`menu.network.open`）· 與 `menu.file.open`
  一字不差，因為是同一個意思：走進某個東西裡，而不是把檔案交給某個 App。中文不分這兩種意思，macOS 也不分：Finder 的
  `打開`（`LocalizableMerged` `N151`）、`打開檔案的應用程式`（`N152`）和
  `以新視窗打開`（`FV7`，走進去的那個意思）用的是同一個動詞（Finder 26.6.2，版本號 25G83，2026-09-07 讀取）·
  `high`。目錄沿用的是`terms.json`上面已經定案的 `open` → `開啟`（AP-HK、MS、五套檔案管理程式；AP-TW寫
  `打開`），所以這裡照抄 `開啟`，不改成 Finder TW 的 `打開`。
- **`Edit server…` → `編輯伺服器…`**（`menu.network.edit`），逐位元組抄自 `commands.serversEdit.label` ·
  `high`。兩者開的是同一張表單；兩個不一樣的標籤會被讀成兩個功能。刪節號是 `…` 這一個字元（U+2026），要留著。
- **這兩處相等有檢查在守**，不只是好看：`i18n-terms`
  會在兩個英文值相同的鍵於中文分叉時報出來。日後改寫其中一個，另一個要一起改。
- **`menu.*` 屬於 RAW 家族**：選單由 Rust 透過 `menu_t` 繪製，從不走 `t()`。所以撇號維持單個，寫成 `''` 會讓 `i18n-icu`
  失敗。這兩個值裡沒有撇號。
- 這台機器上沒有參考語料庫，但 `Finder.app` 可以直接從系統給出同樣的一級證據（`docs/i18n/reference-pile/how-to-mine.md`
  § "No pile on this machine?"）。

## AI 文案的主詞：`Cmdr` / `AI`，`Ask Cmdr` 只指面板（`suggestedOps.*`、`settings.askCmdr.*`、`askCmdr.error.notConfigured`、`ai.cloud.askCmdrOverrideHint`）

英文做了一次收尾：`Ask Cmdr` 現在只在**指聊天面板本身**時出現（面板標題、`menu.view.askCmdr`、
`commands.askCmdrToggle.label`、`settings.section.askCmdr`，以及開關它的那幾條 `settings.askCmdr.status.*` / `turnOn` /
`turnOff`）；凡是**描述 AI 在做什麼**的句子，主詞都換成 `Cmdr`，少數幾條換成 `the AI`。中文照搬這條分工。

- **句子主詞 `Cmdr` → 直接寫 `Cmdr`**，不要補成 `Ask Cmdr` · 目錄本來就這麼寫（`suggestedOps.cmdrFacts` =
  `Cmdr 知道的事實`、`ai.cloudConsent.askCmdr.contentsRule` 開頭的 `Cmdr 絕不會送出整個檔案`）· `high`
- **句子主詞 `the AI` → 寫 `AI`**（`suggestedOps.*` 那一組）· 這四條是刻意跟 `Cmdr` 分開的：`suggestedOps.agentReason`
  （`AI 的理由`）就緊鄰
  `suggestedOps.cmdrFacts`（`Cmdr 知道的事實`），對話框存在的意義就是把「模型說的」和「Cmdr 查證過的」分開。❗ 別把
  `AI 的理由` 統一成 `Cmdr …`，那剛好把這個區分抹掉 · `high`
- **指向設定裡那一段時仍寫 `Ask Cmdr`**（`Ask Cmdr 設定`、`「Ask Cmdr」區段`）· 英文保留了 “the Ask Cmdr settings /
  section”，因為那一段的名字沒變 · `high`
- **“Chatting” / “to start chatting”（拿掉主詞的兩條）→ `聊天` / `開始聊天`** · `askCmdr.error.notConfigured` =
  `聊天需要一個 AI 提供者。請在設定裡開啟一個。`、`settings.askCmdr.provider.off` =
  `在「設定 › AI」中開啟一個 AI 提供者，就能開始聊天。` · `high`。❗ 這裡用動詞的 `聊天`，不是名詞的
  `對話`；`terms.json` 的 `chat` 已經把兩者分開了。
- **“The chat”（當主詞，指那個面板裡的對話）→ `對話`** · `ai.cloud.askCmdrOverrideHint`（`對話用的是它自己的模型…`）、
  `settings.askCmdr.interactiveModel.description`（`對話使用的模型。`）· `high`
- **“What Cmdr sends” / “What Cmdr remembers” 是一對**，中文也要讀成一對：`Cmdr 會傳送的內容` / `Cmdr 記住的內容` ·
  `high`

## 狀態角落的兩條 AI 提示、`同意` 的統一，與 `上下文`（`askCmdr.wake.needsFullDiskAccess`、`settings.askCmdr.proactive.description`、`settings.ai.localContextSize.label`、`settings.ai.tooltipLocal`、`askCmdr.error.localWindowTooSmall`、`askCmdr.event.contextTrimmed`）

- **“AI features” → `AI 功能`** · 沿用 `settings.ai.tooltipOff`（`AI 功能已關閉`）· `high`
- **`askCmdr.wake.needsFullDiskAccess` 的第二句必須一字不差包含
  `search.coverage.setUpFullDiskAccess`**（`設定「完全取用磁碟」`）· `@key` 說明點名要求兩處措辭一致，寫成
  `按一下就能設定「完全取用磁碟」。` · `high`。日後改寫任何一條都要回頭看另一條。
- **“Click to …” → `按一下就能…`** · Apple zh-TW macOS 語料 `按一下` 50 次對 `點一下` 4 次（2026-09-09 量測）·
  `high`。❗ 不是 macOS zh-CN 的 `點按`，那是簡體那邊的詞。
- **approve → `同意`，整本目錄一致** · `terms.json` 已定 `approve` → `同意`；`settings.askCmdr.proactive.description`
  原本寫 `批准`，這輪跟 `suggestedOps.description` 對齊成 `在你同意之前，什麼都不會執行。` · `high`
- **context → `上下文`；context window → `上下文視窗`；context size → `上下文大小`** ·
  Apple 的兩套繁中語料完全沒有這個 LLM 概念（zh-TW 與 zh-HK 的 `脈絡` 都是 0 次、`上下文`
  都是 0 次，2026-09-09 量測），所以只剩 MS zh-Hant TBX：五條 `context` 詞條裡四條是同形異義的 `內容` /
  `執行內容`，只有 id 38882 → `上下文` 是我們要的那個意思 · `tentative`（一級來源整個缺席）。四個鍵一致：
  `settings.ai.localContextSize.label` = `上下文視窗`、`settings.ai.tooltipLocal` 裡的 `隨上下文大小而變`、
  `askCmdr.error.localWindowTooSmall` = `上下文視窗`、`askCmdr.event.contextTrimmed` = `模型的上下文`。

## AI 提供者設定精靈的用詞（`onboarding.cloudSetup.*`）

上一輪是繞過流程翻的，這輪照流程逐條查了證據：四條保留，一條改字。

- **placeholder → `預留位置`** · Microsoft zh-Hant 術語庫（`placeholder` id 92735 → `預留位置` id 92752）· `high`
- **deployment（Azure 上給模型取的部署名）→ `部署`** · Microsoft zh-Hant 術語庫（多條 `deployment` → `部署`）· `high`
- **endpoint → `端點`** · Microsoft zh-Hant 術語庫（四條 `endpoint` 全是 `端點`）· `high`
- **address（那個 endpoint URL 欄位）→ `網址`** · 與同組的 `onboarding.cloudSetup.step.endpoint`
  （`端點網址`）對齊；`位址` 在本目錄留給 IP／網路位址 · `high`
- **terminal → `終端機`** · 沿用 `commands.fileOpenTerminalHere.*` · `high`
- **pull（`ollama pull`）→ `提取`**（原本寫的是沒來源的 `拉`）· Microsoft zh-Hant 術語庫四條 `pull` 有三條是 `提取`（id
  95960 / 151531 / 2309059；剩下那條 `扣動` 是扣扳機的意思）· `high`。整句同時補上 `裡`：
  `在終端機裡用 ollama pull llama3.2 提取一個模型…`

## 「要不要把 Cmdr 留在 Dock 上」那則提示（`main.dockPinNudge.*`、`settings.behavior.dockPinNudgeOfferedAt.*`）

- **`Dock` 保持原文，前後留空格** · Apple 的正體中文從不翻譯它：zh-TW `加入Dock中`、zh-HK `加至Dock`（Finder `MenuBar`
  `300772.title`，英文側 `en_GB.lproj` 為 `Add to Dock`），Dock.app 自己的 `從Dock中移除`、`Dock設定⋯`
  也一樣。空格照 style.md §
  Spacing 加，目錄本來就這樣寫（`errors.listing.storageFull.suggestion`：`在 Dock 裡的垃圾桶圖像上按右鍵`）·
  `confirmed`。
- **`Finder` 保持原文** · Apple zh-TW 與 zh-HK 皆同（`CFBundleDisplayName` = `Finder`，`在Finder中搜尋`）· `confirmed`。
- **`Applications`（那個檔案夾）→ `「應用程式」資料夾`** · Apple zh-TW = zh-HK的側邊欄標籤是 `應用程式`（Finder
  `Localizable` 鍵 `Applications`）；`資料夾` 是本目錄對 folder 的定案（`terms.json` `folder`，`style.md` § The
  Apple-zh-TW outlier rule），而 Apple zh-TW 的 `檔案夾` 是那組離群值之一。整串一字不差沿用
  `updates.moveToApplicationsDialog.howTo`（`把它拖到「應用程式」資料夾`）· `high`。
- **`configuration profile` → `設定描述檔`** · Apple zh-TW 與 zh-HK 的 SystemSettings 同鍵 `Configuration Profile` 都是
  `設定描述檔`。Microsoft zh-Hant 的 `組態設定檔`（TBX，標 `HKG, TWN`）落敗：一級來源兩邊一致時 Apple 勝 · `high`。
- **`No Thanks` → `不，謝謝`** · Apple 自己的字，zh-TW = zh-HK，出現在
  `Problem Reporter.app`、`AppStoreDaemon.framework`、`PassKit.framework` 三個 `Localizable.loctable`（macOS
  26.6.2，版本號 25G83，2026-09-09 讀取）· `confirmed`。

### ❗ Dock 上的 pin / unpin 不用 `釘選`／`取消釘選`

目錄裡的 `釘選`／`取消釘選` 是
**Cmdr 自己介面**的詞（分頁、側邊欄的伺服器：`commands.tabTogglePin.label`、`settings.behavior.serversPinHintSeen.description`）。
**Dock 是 macOS 的介面，所以用 Apple 自己在 Dock 選單裡的那兩個詞**：

- pin（把圖像留在 Dock 上）→ **`保留在 Dock 上`**（Dock.app `DockMenus`，`Keep in Dock`，zh-TW = zh-HK）
- unpin（把它拿出來）→ **`從 Dock 中移除`**（同檔，`Remove from Dock`，zh-TW = zh-HK）

`unpinNote` 就是在告訴使用者怎麼反悔，而他要去按的正是 Dock 選單裡的那一項；寫 `取消釘選`
會讓他在 Dock 選單上找不到對應的字。這兩個詞在同一份選單裡就是一組相反詞，英文 pinned / unpin 的對應關係因此還在 ·
`high`。界線很好套用：**Cmdr 自己的東西用 `釘選`，macOS 的 Dock 用 `保留`／`移除`**，跟 `terms.json` `debug` 那條
`偵錯`／`除錯` 的界線是同一個形狀。

### 四則結果訊息不准說「失敗」

`added`、`addedButDockDidNotRestart`、`managedDock`、`notAdded` 都是錯誤類訊息，照 style.md § Voice and tone 一律走
`無法` + 動詞，不寫 `失敗`、`錯誤`：

- `notAdded` → `Cmdr 這次無法加入 Dock。`，後半給手動的做法（從「應用程式」資料夾拖到 Dock 上）。
- `managedDock` → `…所以 Cmdr 無法自己加進去。`，再點名誰能解（`管理這部 Mac 的人`），不出主意繞過去。`部`
  是 Mac 的量詞（§ style.md Plurals）。
- ❗ **`addedButDockDidNotRestart` 不可以寫成沒加成功** ——圖像已經放好了，少的只是重畫。所以寫
  `Cmdr 的圖像已經放好了，只是 Dock 還沒重新載入。`：`只是…還沒…` 讀起來是「還在路上」，不是「沒做到」。`圖像`
  是 icon（Apple zh-TW `圖像大小`、`Finder圖像`；目錄已在用），`登入` 是 log in（Apple `在登入時打開`）· `high`。

### 「幾天」不准變成數字

`body` 的 `a few days` 寫成 **`好幾天`**，絕不寫「三天」：門檻會變，而且 ledger 是上線那天才開始數的。`down there` →
`下面的 Dock 上`（Dock 預設就在螢幕底部，中文講得出來，所以留著）。`Yes, add it to my Dock` →
`好，加入 Dock`：中文自己的東西通常不帶所有格， `我的` 加上去反而不自然；動詞取 Apple 的
`加入`（`加入Dock中`），按鈕上省掉 `中` 比較好唸 · `high`。

## 右鍵按 Dock 圖像跳出來的那張選單（`menu.dock.*`）

五個 `menu.dock.*` 都是 Rust 直接畫的原生選單項目（RAW family：撇號不加倍，`{token}` 是字面替換），所以撇號維持單個、
`…` 一律 U+2026。**每一個都對得上目錄裡已經有的鍵，沒有新造詞**，這是刻意的：這個 locale 有跨批次用詞漂移的前科（選單寫
`命令選擇區…`、它打開的面板卻叫 `指令面板`），所以先 grep 目錄再定案。

- **`openCmdr` = `開啟 Cmdr`** · 動詞取 § The Apple-zh-TW outlier rule 的 `開啟`（Dock.app 自己的 `DockMenus.strings`
  `OPEN` 在 zh-HK 正是 `開啟`，zh-TW 才寫 `打開`，又一次落在那條 outlier 上）；`動詞 + 空格 + Cmdr`
  的形狀直接抄目錄已經在出貨的
  `menu.app.hide`（`隱藏 Cmdr`）、`menu.app.quit`（`結束 Cmdr`）、`menu.app.about`（`關於 Cmdr`）· `high`。❗ **不要加上
  `「」`**：Dock.app 的 `HIDE_NAME` / `SHOW_NAME` 寫成 `隱藏「%@」` 是因為那個 `%@`
  是執行期塞進去的任意 App 名稱，需要框起來；我們這裡的 `Cmdr` 是寫死在字串裡的品牌詞，跟 `menu.app.*`
  三個同款，加了角括號反而跟隔壁選單不一致。
- **`searchFiles` = `搜尋檔案…`** · 逐字複製目錄裡的 `menu.edit.searchFiles`（選單列上的同一個命令）·
  `confirmed`。brief 就是這樣要求的，而且兩個項目同時看得到。
- **`goToFolder` = `前往資料夾…`** · Finder `Go` 選單的同一項：`MenuBar.strings` `261.title` 在 zh-HK 是
  `前往資料夾⋯`、zh-TW 是 `前往檔案夾⋯`（`GotoWindow.strings` `1.title` 與 `Localizable.strings` `Go To Folder`
  兩處都同樣分裂）。folder = `資料夾` 的 outlier 裁決照套，取 HK/共識形 · `high`。❗ **這一項跟 `menu.go.goToPath`
  （`前往路徑…`）是兩個不同的鍵**，英文那邊也刻意分成 `Go to folder…` 和
  `Go to path…`：Dock 選單抄 Finder 的說法，選單列那一項用 Cmdr 自己的 `路徑`。不要把兩邊統一掉。
- **`connectToServer` = `連接伺服器…`** · 目錄裡已經出貨兩次（`commands.serversConnect.label`、
  `settings.network.permissionIntroConnectLink`），而且跟 Finder 完全一致：`MenuBar.strings` `266.title` 在 **zh-TW =
  zh-HK 都是 `連接伺服器⋯`** · `confirmed`。
- **`locationInParent` = `{name}（{parent}）`** · 照 `style.md` §
  Punctuation, 全形括號、括號跟裡面的字不空格；形狀抄目錄自己的同款消歧義列
  `fileExplorer.renameConflict.yours`（`{name}（你的）`）、
  `menu.context.openWithDefault`（`{app}（預設）`）、`menu.volume.eject`（`退出（{name}）`）·
  `high`。兩個 token 的順序跟英文一樣，名字在前、所在位置在後：中文這個位置本來就是後置限定，不需要倒過來，也不用補介詞（補了會變成一句話，而這是一列選單項目）。

## 「在 Finder 中顯示」的詢問與首次接手提示（`main.revealNudge.*`、`main.revealActivation.*`、`settings.behavior.reveal*`）

同一個功能的兩個時刻：一次性地詢問要不要讓其他 App 的「在 Finder 中顯示」改在 Cmdr 中開啟，以及這類要求第一次落到這裡時的一次性提示。兩處都指向 macOS 自己的指令，所以依 style.md
§ 系統介面，採用 macOS 的說法。

- **「Show in Finder」→ `「在 Finder 中顯示」`，用直角引號** · 已在 `settings.navigationAndFileOps.card.showInFinder` 與
  `settings.revealHandler.description` 中定稿 ·
  `high`。提示訊息沿用這個形式，讓設定卡片與提示用同一個名字稱呼同一個動作。
- **pane → `窗格`** · 目錄既有形式：`fileExplorer.doubleClickHint.body`（「窗格背景」）· `high`。
- **Settings（Cmdr 自己的視窗）→ `設定`** · `settings.window.title` · `high`。
- **「for a while now」→ `有一陣子了`** · 刻意含糊：門檻可能變動，所以 ❌ 絕不寫出天數。與
  `main.dockPinNudge.body`（「好幾天了」）同一條規則 · `high`。
- **首次接手提示 ❌ 不是道歉** · 它說明剛才發生了什麼、為什麼，以及開關在哪裡。所以寫
  `Cmdr 設定成接手這類要求`，❌ 不寫「抱歉」· `high`。

## 入門引導的檢查清單、步驟計數與開關摘要（`onboarding.stepBeta.*`、`onboarding.stepOptional.*`、`onboarding.moreAbout`、`onboarding.wizard.*`、`settings.revealHandler.notProductionBuild`）

第三步新增的檢查清單、第四步四條開關摘要，以及精靈框架幾個新鍵。用詞先 grep 目錄自己已經出貨的同義句，再回頭找來源。

- **star（GitHub 上按的那個星）** · 保留原文 `star`，數量講 `顆星` · Microsoft zh-Hant 術語庫 id 3097426 → id
  3109098，定義寫的正是 GitHub 的那個意思（`A bookmark or display of appreciation for a repository. Stars are a manual way to rank the popularity of projects.`），繁中側就是原文
  `star`，標 `HKG, TWN`；目錄自己的 `onboarding.stepBeta.checklist.star` 寫 `在 GitHub 上幫儲存庫按個 star`，`…starNote`
  寫 `225 顆星` · `high`。❗ 術語庫裡另外四條 `star` 全是別的意思，別拿錯：`星號`（清單裡的優先標記，id
  2329252）、`加上星號`（把東西加星號這個動作，id 2643747）、`星星`（emoji）、`主角`（Photos 影片的主角）。⚠️
  **GitHub 自己的繁中介面沒能直接查證**（要登入切語言才看得到），所以這條靠的是微軟這條 GitHub 專用詞條加上目錄內部一致性；哪天有人能看到 GitHub 的繁中 UI，值得回頭核一次。
- **repository（GitHub 上的那個 repo）** · `存放庫` · 見 `terms.json`
  `repository`（MS 的原始碼控制義項，目錄裡 19 處；原本猜測的 `儲存庫` 已取代） · `high`
- **Like（AlternativeTo 那顆按鈕）** · `按讚`（動詞）／`讚`（名詞）· Microsoft zh-Hant 術語庫 id 2349285 `Like` → id
  2352483 `讚`，標 `HKG, TWN` · `high`。AlternativeTo 站上沒有繁中介面，按鈕本身寫的是英文 `Like`，所以
  `alternativeToNote` 那條要把位置講清楚（`就在頁面最上方，「Cmdr」標題旁邊`），使用者才找得到。
- **checklist** · `檢查清單` · Microsoft zh-Hant 術語庫 id 30962 → id 30977，標 `HKG, TWN` · `high`
- **mailing list** · `郵寄清單` · Microsoft zh-Hant 術語庫兩條都一樣（id 2791 → id 2806、id 210512 → id 2773512），標
  `HKG, TWN` · `high`。❗ 不是 `電子報`（那是 newsletter，id 295551 → `電子報`），英文刻意說這不是 newsletter。
- **sign up（加入名單）／signup server** · `註冊`／`註冊伺服器` · Microsoft zh-Hant 術語庫 id 112681 → id 2510936，定義
  `To enroll in a service`，標 `HKG, TWN` · `high`
- **typo** · `打錯字` · 五份語料都沒有，就是日常國語的講法 · `tentative`
- **bug（要修的那種缺陷）** · `問題` · `onboarding.stepBeta.openBeta` 的 `fix bugs` 寫成
  `修正問題`；步驟 3 改版前還有一句幾乎一樣的 `spot bugs`（`幫我找出問題`）在回饋管道清單的開場，那段已隨改版刪掉 ·
  `high`。❗ 不取術語庫的 `錯誤`（id 24563）：`錯誤` 在本目錄留給 `錯誤報告` 那種終局狀態名詞，`style.md` § "Voice and
  tone" 禁的是拿它當句子的動詞。
- **More about X（資訊圖示的無障礙名稱）** · `進一步了解「{topic}」` · Apple zh-HK Finder
  `ICloudNoDocumentsView`（`進一步了解iCloud`）、`ICloudUpgradeView`（`進一步了解⋯`）、`LocalizableMerged`
  NE115（`進一步了解`）· `high`。**角括號是刻意的**：`{topic}`
  塞進來的是開關自己的標題，有的是漢字（`磁碟機索引`）、有的以拉丁文開頭（`MTP（Android 手機、Kindle、相機）`），角括號同時解決了「這是個 UI 名稱」和 §
  Spacing 的拉丁／漢字空格兩件事。
- **badge（`<alpha></alpha>` 那個狀態標記）** · 動詞化寫成 `標記`，不寫 `徽章` · 目錄自己出貨的
  `onboarding.stepBeta.openBeta`（`看到 <alpha></alpha> 標記的地方`）· `high`。術語庫的 `徽章`（id 1354385、id
  1606726）指的是成就徽章和鎖定畫面的計數徽章，不是這種狀態標籤；而 `標籤` 在本目錄專屬於 tag（`terms.json` `tag`）。
- **`Settings › X` 這種路徑** · `「設定 › 更新與隱私」`，`›` 原樣保留 · 形狀抄目錄自己的
  `settings.askCmdr.provider.off`（`在「設定 › AI」中…`）；`更新與隱私` 逐字取自 `settings.section.updatesAndPrivacy` ·
  `high`
- **"Local Network"（macOS 隱私面板裡的那一列）** · `「區域網路」` · 引號裡只放 Apple 的名字，`取用`
  留在引號外面當動詞： `onboarding.stepOptional.networking.summary` 與 `…networking.desc`
  一致（`請你允許「區域網路」取用`）。英文那邊也在 2026-09-09 從 `Local network access` 改成 Apple 自己的
  `Local Network`，理由相同：使用者要在系統設定裡找得到這個名字 · `high`
- **`{mandatory}+1` 的 `+1` 是字面的** · `第 {step} 步，共 {mandatory}+1 步` · 英文刻意寫 `3+1` 而不是
  `4`（最後一步是可選的），中文照搬；量詞用 `步`，形狀抄隔壁的 `onboarding.wizard.stepProgress` · `high`
- **released copy / Dev and test builds（`settings.revealHandler.notProductionBuild`）** · `正式發行的 Cmdr 版本` /
  `開發版和測試版` · MS（`CHINESE (TRADITIONAL).tbx`：`release` → `發行版本`，`production build` →
  `正式版本`）；這裡講的是給人用的版本，所以不用 `建置`（那是 `search.systemDirExclude.default`
  裡資料夾的說法），也不用 MS 的 `組建`。第二句沿用 `settings.revealHandler.notInApplications` 的
  `每一次「在 Finder 中顯示」的點擊都指向空的地方`，讓兩條提示讀起來是一對 · `high`（術語）；"come and go" 譯作
  `來來去去` 是意譯判斷，`tentative`
- **fetch（檢視器先把手機、伺服器或封存檔裡的檔案取到暫存檔再顯示）** · `取得`，標題寫 `正在取得「{fileName}」以便預覽`
  · AP-HK `Fetching Menu Item` = `正在取得⋯`（AP-TW 是 `擷取中⋯`，本目錄的 `擷取` 只用在
  `預先擷取`）；目錄裡 key 名就叫 fetch 的 `fileExplorer.navigation.spaceFetchFailed` = `無法取得磁碟空間` ·
  `high`。❌ 不寫 Finder 的 `下載`（`PE126` `正在下載^0`）：USB 手機和封存檔都不是下載。檔名照 Apple `FT1`
  （`正在將「^0」下載至「^1」`）加角括號。
- **"x of y"（位元組進度行，兩個值都帶單位）** · `{doneText} / {totalText}` · AP-TW `PW8` `已拷貝：^0/^1`、AP-HK
  `已複製：^0/^1`，進度行一律用斜線；斜線兩旁留空格沿用
  `errorReporter.dialog.counter`（`{currentText} / {maxText}`），因為單位裡本身有空格 · `high`。❗ 和 `style.md` §
  "Spacing: put a space between Chinese and Latin" 裡那句 `64.0 MB/1.33 GB`
  不衝突：那裡講的是別讓人往一段 Latin 裡面加空格，這裡是兩個占位符之間的分隔。
- **"so far"（總大小未知時已到的量）** · `目前已取得 {doneText}` · `目前…` 沿用
  `queryUi.results.live.matchesSoFar`（`目前找到`）、`fileOperations.transferProgress.rollbackTooltip`（`目前為止寫入`）；動詞與標題的
  `取得` 同字 · `high`
- **"stopped arriving"（45 秒沒有資料進來）** · `這個檔案的傳輸停住不動了` · 複用目錄已出貨的
  `fileOperations.transferProgress.stallUnknown`（`這項傳輸停住不動了`），刻意不寫 `已暫停` 或
  `沒有回應`；後半句逐字沿用 `errors.listing.*.suggestion` 的 `確認磁碟或裝置仍然連接著` 與 Apple 的 `然後再試一次` ·
  `tentative`（`停住不動了` 是既有目錄用字，五份語料都沒有來源）

## 手機版的過時索引提示（`fileExplorer.navigation.driveIndex.tooltipStalePhone`、`indexing.staleDialog.titlePhone`、`.bodyPhone`）

這三條是磁碟機版本（`tooltipStale`、`staleDialog.title`、`staleDialog.body`）的手機版：透過 ADB 接著的手機從不回報自己的檔案變動，所以索引在手機還插著的時候就顯示過時。用字照抄磁碟機那一家，只把原因從「中斷連線期間」換成「手機不會告訴 Cmdr」，❗ 不能提到中斷連線。參考素材裡沒有
`keep … up to date` 這種句型可抄（macOS 語料與 MS 術語庫都查過），以下以目錄既有用字為準。

- **phone（標題）** · `這支手機的索引可能過時了` · 形狀逐字抄
  `indexing.staleDialog.title`（`這個磁碟機的索引可能過時了`），量詞照 § Android 手機的連線窗格 的 `這支手機` · `high`
- **keep the index current with the changes Cmdr makes** · `Cmdr 會讓索引跟上自己做的變動` · **自行組出來的**：`跟上`
  取自 `indexing.rescan.watcherStartFailed`（`讓索引跟上`）與 `indexing.rescan.staleIndex`（`來跟上`），`變動`
  取自兩條磁碟機版本（`可能有變動`）；`自己` 回指主詞 Cmdr，省掉英文重複的第二個 `Cmdr` · `tentative`。❗ 不加
  `即時`：英文沒有這個承諾。
- **doesn't tell Cmdr** · `不會…告訴 Cmdr` · 刻意不寫 `通知`：目錄裡 `通知`
  是 macOS 通知那個字，這裡講的是手機不回報變動 · `high`
- **show up after a rescan** · `重新掃描後就會出現` · `重新掃描`
  是目錄既有動詞（`fileExplorer.navigation.driveIndex.menuRescan`），`就會…` 沿用 `tooltipStale` 的 `重新掃描就能更新` ·
  `high`
- **folder sizes and search** · `資料夾大小和搜尋結果` · 逐字沿用 `indexing.staleDialog.body` · `high`
- **like new photos** · `（例如新照片）` · 全形括號加 `例如`，形狀抄
  `errors.listing.notSupported.explanation`（`（例如手機儲存空間或某些網路磁碟機）`）· `high`
- **stays as a reminder（黃色狀態）** · `手機旁邊的黃色狀態會一直留著，提醒你這件事` · `黃色狀態` 與 `X 旁邊` 逐字沿用
  `indexing.staleDialog.body`；`提醒` 是目錄既有用字（`settings.fileExplorer.suppressQuickLookHint.description`）·
  `high`

## 共享資料夾裝載不了、共享列表載入不出來時的那句話（`errors.mount.*`、`errors.shareList.*`）

`無法裝載共享資料夾`（`fileExplorer.networkMount.mountFailedTitle`）和 `無法連線到 {hostName}`
（`fileExplorer.network.share.connectFailedTitle`）下面的那句話，加上
`fileExplorer.pane.directConnectionShareGoneToast`、`fileExplorer.pane.directConnectionMountNotRespondingToast`、
`fileExplorer.pane.directConnectionNotNetworkShareToast` 三則提示和 `servers.refusal.accountNotPermitted`。Tier
1取自系統裡裝著的 `NetAuthAgent.app/Contents/Resources/Localizable.loctable`（macOS
26.6.2，25G83，2026-09-11，TW 與 HK 兩邊）：它正是「連接伺服器」這些狀況的文案，參考素材裡沒有。

- **share → `共享資料夾`** · 目錄已定（標題就是它）· `high`。NetAuthAgent `EINFO_NO_SHARE` 寫
  `共享「%@」`，但這個目錄的名詞一律帶 `資料夾`。
- **guest → `訪客`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST`（TW = HK：`此伺服器不允許「訪客」連線。`）· `high`
- **reach → `連不上`**、**didn't answer in time → `沒有及時回應`**、**isn't responding → `沒有回應`** ·
  `servers.refusal.unreachable`、`servers.refusal.timedOut`、上面的 `is not responding` 一條 · `high`
- **didn't work（帳密）→ `不管用`** · `servers.refusal.authenticationRejected` · `high`
- **package → `套件`** · `licensing.acknowledgements.npmHeading`（`npm 套件`）、Microsoft `套件管理員` · `high`。
  **distribution（Linux）→ `發行版`** · 沒有任何來源有 Linux 義項 · `tentative`
- **Something went wrong → `出了點狀況`** · `errors.mutation.unexpected` · `high`
- **there's nothing to speed up → `沒有連線需要加速`** · 句形取自 `errors.eject.notAnSmbVolume` · `high`
- 英文相同的兩對譯文也逐字相同：`errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`、
  `errors.mount.authFailed` / `errors.shareList.authFailed`。角括號 `「…」`；`Cmdr`、`SMB 2`、`GVFS`、`smbclient`
  前後留空格。

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `文字編輯器`** · Microsoft 術語庫 `zh-Hant`（`text editor` → `文字編輯器`）·
  `high`。指這一類 App，❌ 不是 Apple 的「文字編輯」App（它的名字由 `{app}` 帶進來）。
- **default text editor → `預設文字編輯器`** · 沿用目錄裡的 `預設編輯器`（`commands.fileEdit.label`）· `high`。
- **Edit files in [app] → `編輯檔案時使用`** · 句子接到下拉選單裡的 App 名，與
  `settings.behavior.openTerminalHereApp.label` 的「……使用」同一結構 · `tentative`。
- `{app}` 前後留空格。Dismiss、Open settings 與 `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings` 一致。
- **system default → `跟隨系統`** · 目錄（`settings.appearance.language.opt.system`、
  `settings.appearance.dateTimeFormat.opt.system`）· `high`。`settings.behavior.textEditorApp.systemDefault`
  在後面用全形括號帶上 App 名，同 `settings.appearance.language.opt.systemWithLanguage`。
- 「Choose an app…」、「Checking your apps…」與 `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking` 一致。提示（`fileExplorer.edit.hint`）沿用
  `commands.handler.openTerminalHere.hint` 的句式，但不寫設定的位置，因為它的按鈕直接跳過去。

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected（退出或卸除進行中）→ `正在中斷連線`** · 沿用已定的 `disconnect → 中斷連線`，加 `正在`
  表示進行中，同 Thunar `zh-TW`（「正在卸載裝置」/「正在退出裝置」）· `high`。「Left its index as it
  was」→「維持原樣」，同 `operationLog.rollback.partiallyRolledBackNotice`；「try again in a
  moment」→「請稍後再試一次」，同一檔案裡的 `fileExplorer.pane.directConnectionMountNotRespondingToast`。

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected（已經發生，磁碟機已不在）→ `中斷了連線`** · 沿用已定的 `disconnect → 中斷連線`，取完成態，同
  `indexing.staleDialog.body`（「{name} 沒有連接的期間」）· `high`。不用
  `fileExplorer.navigation.driveIndex.driveLeaving` 的進行態「正在中斷連線」，那裡退出還沒結束。「Starts from
  scratch」→「從頭開始」，同 `indexing.rescan.incompletePreviousScan`；`掃描` 與 `資料夾大小` 取自同一檔案。

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

四個 `sided` 值是同一個對話框的內文，上面掛的標題就是
`errors.write.deviceDisconnected.title`（`裝置已中斷連線`），所以整組跟著既有的那幾條非 sided 內文走，不另立句型。

- **was disconnected（傳輸途中被拔掉）→ `斷線了`** · 逐字沿用同一個對話框的
  `errors.write.deviceDisconnected.message.copy`（`複製到一半時，裝置斷線了。`）與
  `errors.volume.deviceDisconnected`（`更動還沒完成，裝置就斷線了。`）· `high`。❗ 標題用已定的 `中斷連線`，內文用
  `斷線了`：這是這個家族既有的分工，不是漂移。這裡也不用 `indexing.needsFreshScan.afterDisconnect` 的
  `中斷了連線`，那是另一個檔案的句型。
- **兩側都不寫「來源 / 目標」字樣** · 英文也沒寫：`{volumeName}` 永遠是走掉的那台，`{counterpart}`
  是另一台，差別由哪個名字出現在哪個位置帶出來 · `high`。❗ 別好心加上 `來源磁碟機` /
  `目標磁碟機`：讀者看到的是自己磁碟機的名字，多一層標籤反而讓句子變成說明文。
- **copied {done} of {total} files → `把 {total} 個檔案中的 {done} 個複製…`** · 目錄的 `X of Y` 既有形狀是
  `（共 N 個）`（`indexing.enrich.progress`、`search.imageResults.countCapped`），但那是純計數；這裡數字要接在動詞上，所以取
  `A 中的 B` · `high`。兩個佔位符前後都留空格，`{done}`、`{total}` 進來就已經是格式化好的字串，不包 ICU。
- **the rest → `其餘的`，而且一定要指名地方（`還在磁碟機上`）** · 已定條目見上面
  `fileOperations.cancelRollback.stoppedMovingBack` 那一條；Apple Finder
  zh-TW 幾乎同一句（`無法拷貝一個或多個項目。是否要略過並拷貝其餘的項目？`）· `high`。英文的 "on the
  drive" 本身就是地方，所以這裡不會踩到 `原處` 那個歧義；`磁碟機` 是已定的 drive。
- **your originals → `你原本的檔案`** · 沿用 `errors.write.destinationNotFound.message.copy` （英文同樣是 "The originals
  are untouched."）· `high`。目錄另有 `原始檔案`
  （`errors.write.readOnlyDevice.source.suggestion`），兩個都已出貨；這裡取
  `原本的`，因為緊接著的那句「完全沒有被動過」就是從同一條抄來的，兩半要對得上。
- **untouched where they were → `都還在原處，完全沒有被動過`** · `完全沒有被動過` 逐字取自
  `errors.write.destinationNotFound.message.copy` 與 `errors.write.notConnected.message.destination` ·
  `high`。英文的 "where they were" 另外用 `都還在原處` 補上：這裡 `原處` 是對的，因為那些檔案本來就沒動過。
- **so nothing is lost → `什麼都沒被丟掉`** · 逐字沿用 `errors.write.originalsKeptAside.message.one` （"Nothing was
  thrown away."）· `high`。英文是 "lost"、目錄既有的是 "thrown
  away"，語意略有差；取既有說法，讓同一個對話框家族只有一種安撫句尾。
- **all your files are still on {counterpart} → `你的檔案全都還在 {counterpart} 上`** · `全都` 把英文的 "all" 接住 ·
  `high`。這一條刻意沒有數字：移動只要停下來，原檔案就一個都沒少，沒有「做到一半」可報。

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

❗ 這一組**不是**失敗文案。Cmdr 留著原檔案，正是因為它沒辦法證明複本已經落地，所以 `couldn't confirm`
照字面翻，❌ 不准升級成 `移動失敗` 或 `移動沒成功`。

- **Couldn't confirm → `無法確認`** · Apple Finder
  zh-TW 幾乎同一句（`無法確認是否可刪除資料夾「^1」。`），目錄也已在用（`suggestedOps.destinationUnknown` =
  `Cmdr 無法確認目標資料夾`）· `high`。同時正好是 `style.md` § Voice and tone 要的 `無法` + 動詞形狀。
- **the move（標題裡的那次移動）→ `這次移動`** · `style.md` § Voice and tone 要口語的 `這次`，不要書面的 `此` ·
  `high`。標題整句 `無法確認這次移動`。
- **the moved files → `移動過去的檔案`** · 沿用 `fileOperations.cancelRollback.stoppedMovingBack` 的 `移動過去的地方` ·
  `high`。
- **were saved on {volumeName} → `已經儲存到 {volumeName} 上`** · `儲存` 是已定術語（見上面 save 條，`confirmed`）·
  `high`。
- **at the destination → `目標位置`** · 已定術語（見上面 destination 條）· `high`。`.unnamed` 沒有佔位符，就用這個詞把
  `.named` 的 `{volumeName}` 換掉，其餘一字不動，兩條讀起來才是同一句話的兩個版本。
- **it kept your originals where they were → `把你原本的檔案留在原處沒有動`** · `原處`
  在這裡是對的（檔案真的沒離開過），`沒有動` 收尾把「刻意保留」講明 · `high`。❗ 不要縮成
  `原本的檔案還在`：英文的主詞是 Cmdr，這是它做的決定，不是碰巧的結果。
- **Have a look at the destination, then … → `打開目標位置看一下，然後再移動一次`** · 句形逐字取自
  `errors.write.newDataKeptAt.suggestion`（`打開 {keptAt} 看一下，然後幫它改名。`）· `high`。 `再移動一次` 跟著
  `errors.write.deviceDisconnected.suggestion` 的 `再試一次` 走。
- **Your originals haven't moved → `你原本的檔案都還在原處`** · 與內文那句 `留在原處` 對齊 ·
  `high`。建議句尾再講一次原檔案在哪，是英文刻意的重複，照留。

兩組共 8 個值都與英文不同，都不需要
`sameAsSourceJustification`。這一族屬於 RAW 家族，不經過 ICU，所有標點取全形；中文本來就不需要撇號，真要用也一律維持單引號。

## 沒做完的移動留在磁碟機上的那個資料夾（`fileOperations.leftovers.stagingFolderKept`）

磁碟機重新接上（或 Cmdr 啟動）時跳出來的一則資訊提示：Cmdr 發現某次移動的工作資料夾裡還有檔案，就刻意一個都不動。這些可能是使用者僅存的一份，因為移動也許已經把來源刪掉了。❗ 這條**不是**
`fileOperations.cancelRollback.stagedLeftover.named`
那一家：那邊是 Cmdr 自己清不掉的殘留（`不完整副本`），❌ 這裡的檔案是完整的、是使用者的，寫成 `不完整副本`
會說錯。也 ❌ 絕不暗示可以刪掉它。

- **an unfinished move → `一次還沒完成的移動`** · `移動` 是已定術語（見 `terms.json` `move`）；`還沒完成`
  逐字沿用目錄自己的 `errors.listing.cancelled.explanation`（`這項操作還沒完成就被取消了`）與
  `errors.volume.deviceDisconnected`（`更動還沒完成`）· `high`。❗ **不要寫 `未完成的移動`**：`未完成` 在本目錄是
  `operationLog.status.failed` 的狀態標籤，也就是「失敗」，用在這裡會把一件保護性的事說成出了問題。
- **left them in place → `沒有動它們，全都留在…裡`** · `沒有動` 取自
  `errors.write.moveNotConfirmed.message.named`（`把你原本的檔案留在原處沒有動`），`全都` 取自
  `errors.write.deviceDisconnected.sided.destination.move`（`你的檔案全都還在 {counterpart} 上`）· `high`。❗ **不要寫
  `留在原處`**：依 `style.md` § Notes and decisions 的那條，`原處`
  會被讀成「回到它們原本的地方」，但這些檔案正好不在原本的地方，而是在工作資料夾裡。這裡把地方指名出來（`隱藏資料夾`），正是那條規則要求的做法。
- **a hidden folder named X → `名為「{folderName}」的隱藏資料夾`** · `名為「…」的<名詞>` 是 Apple 的句型（AppKit
  `An item named “%@” already exists…` →
  `名為「%@」的項目已經存在此位置上。`），目錄自己也已經在用（`errors.mount.shareNotFound`：`名為「{share}」的共享資料夾`）；`隱藏`
  是已定術語（`settings.listing.showHiddenFiles.label`、`commands.viewShowHidden.label`、
  `fileExplorer.rename.hiddenAfterRename`），Dolphin zh-TW 也寫 `隱藏的 .directory 檔案` · `high`。寫成複合詞
  `隱藏資料夾` 而不是 `隱藏的資料夾`，因為前面已經有一個 `的`。**`隱藏`
  兩個字非留不可**：這是整條文案唯一能讓人採取行動的資訊（要先打開顯示隱藏檔案才看得到那個資料夾）。
- **兩個佔位符的處理** · `{volumeName}` 裸用、兩側留空格（`在 {volumeName} 上`，同
  `errors.write.deviceDisconnected.sided.destination.move`）；`{folderName}` 是 `.cmdr-staging-<uuid>`
  這種很長的拉丁字串，包進直角引號當名字看，引號與內文之間不空格，依 `style.md` § Punctuation · `high`。
- 值裡沒有撇號，所以 ICU 的雙寫撇號規則在這條用不到；標點全形，語序與英文相同（先說找到什麼，再說放在哪裡）。

## 喜好項目選單（`commands.favoritesOpen.*`、`commands.favoritesOpenByNumber.label`、`commands.favoritesAdd.description`、`fileExplorer.navigation.favoritesAddCurrent`／`.favoritesAlreadyAdded`／`.favoritesCantAddHere`／`.seeFavorites`、`menu.go.showFavorites`、`shortcuts.scope.favoritesMenu`）

⌃D 會在焦點窗格上打開一張選單，列出使用者收藏的資料夾，前九列各帶一個數字鍵，最後一列是
`0`：把窗格目前的資料夾加進清單。原本卷宗切換器裡的「喜好項目」區塊沒有了，改成切換器最上面一列，按下去就換成這張選單。

- **favorites menu → `喜好項目選單`** · `喜好項目` 是本目錄既定的 favorite（見 `terms.json` `favorites`，AP-TW =
  AP-HK），`選單` 是既定的 menu（同節，AP-HK／TC／DOL；MS 的 `功能表` 是 Windows 用語）· `high`。❗
  **三個近義介面要分清楚**，這正是本語言最容易漂移的地方（`docs/guides/i18n-translation.md` § Auditing a finished
  locale 就是拿 zh-Hant 當例子：曾經出現 `命令選擇區…` 打開一個叫 `指令面板` 的東西）： `喜好項目選單`
  是這張新的選單（`shortcuts.scope.favoritesMenu`）、`卷宗切換器`
  是裝磁碟和位置的那個下拉（`shortcuts.scope.volumeChooser`）、`指令面板`
  是 ⌘K 那個（`shortcuts.scope.commandPalette`）。❌ 不要把這張選單叫成 `面板`、`清單` 或
  `切換器`；它是一張真的選單，鍵盤操作也跟選單一樣。另外 `標籤`（tag）／`分頁`（tab）的老界線照舊，這批沒有動到。
- **Show favorites（雙胞胎）→ `顯示喜好項目`** · `commands.favoritesOpen.label` 與 `menu.go.showFavorites`
  英文同字，中文也必須同字（`desktop-i18n-term-consistency` 會抓）。`顯示` 取自目錄自己的
  `commands.serversShow.label`／`menu.go.showServers`（都是 `顯示伺服器`），Apple zh-TW 的 `Show …` 也一律是 `顯示…` ·
  `high`。動詞用 `顯示` 而不是 `開啟`：`開啟` 在本目錄是 open a file／folder 的保留字。
- **current folder → `目前的資料夾`** · 目錄既有寫法（`commands.favoritesAdd.description`、
  `queryUi.scope.currentFolder`）· `high`。❗ 指的是窗格現在待著的資料夾，不是游標底下那個；中文不用特別加字，因為
  `目前的資料夾` 本來就讀成「窗格現在在的這個」，要講游標那個時目錄一律寫 `游標所在的…` （見
  `commands.fileContextMenu.description`）。
- **mounted share → `已裝載的共享資料夾`** · 目錄既有寫法（`fileExplorer.network.browser.noMountedShares`：
  `{hostName} 沒有已裝載的共享資料夾`；`errors.mount.shareNotFound` 也用 `共享資料夾`）· `high`。❗
  **不要換成通訊協定名稱**：英文刻意不提 SMB／MTP／ADB，中文也一樣，因為使用者要判斷的是「這個地方像不像一顆磁碟」，不是它走什麼協定。
- **`fileExplorer.navigation.favoritesCantAddHere` 講的是「這個資料夾」，理由放在冒號後面** ·
  `這個資料夾不能加入喜好項目：喜好項目只能用在磁碟和已裝載的共享資料夾上` · `high`。開頭跟姊妹列
  `fileExplorer.navigation.favoritesAlreadyAdded`（`這個資料夾已經在喜好項目裡了`）一致，兩條灰列讀起來成對。❌不再用
  `指向`：那個動詞逼著每種有格變化的語言替受詞挑一個格；換成「用在…上」的處所說法後，原本句尾連著兩個 `資料夾`
  的疊字也一併消失了。用全形冒號，句末不加句號（懸停提示）。
- **press a number to jump to that favorite → 受詞寫出來** · `按數字鍵就能跳到對應的資料夾` · 中文的 `跳到`／`前往`
  一定要帶地方，不然讀起來是斷句 · `high`。英文本來停在 "go"，是中文自己補的受詞；英文現在也寫出 "that
  favorite" 了。中文仍取 `對應的資料夾` 而不是
  `對應的喜好項目`：每一項喜好項目本來就是一個資料夾，意思一樣，而這句開頭已經有
  `喜好項目選單`，再疊一次會拖沓。這個「數字↔項目對應」的說法跟 `commands.favoritesOpenByNumber.label` 的
  `開啟數字對應的喜好項目` 是同一套。動詞 `跳` 沿用 `commands.navGoToPath.description`（`讓焦點窗格跳至…`）。
- **數字鍵、`0`–`9`、⌃D 等鍵名一律不譯** · `數字鍵` 只是泛稱，鍵面上的字元保持原樣 · `confirmed`（`style.md` §
  "Punctuation"：數字一律用阿拉伯數字）。
- **`0` 那一列只有一個鍵：`fileExplorer.navigation.favoritesAddCurrent`** · `把目前的資料夾加入喜好項目` ·
  `high`。快速鍵清單是**引用**這一列來說明 `0`
  鍵，不是另寫一句，所以讀的是同一個值。從前那兩個鍵中文本來就完全一樣（英文只差一個冠詞，中文沒有冠詞可差）。❌不要為了讓快速鍵清單看起來不同而另造一句。真正要保持區別的是
  `commands.favoritesAdd.label`（`加入喜好項目`，那是指令本身的名字，不帶受詞），這個區別中文有守住。
- **`commands.favoritesAdd.description` 不再提「切換器的喜好項目」**
  · 那個區塊 M3 已經拿掉，目錄值不能描述一個不存在的介面。現在寫
  `把焦點窗格目前的資料夾加入喜好項目，以後從喜好項目選單就能回到這裡。` · `high`。
- **`.seeFavorites` 的複數形狀：只有 `=0` 和 `other`** ·
  `{count, plural, =0 {查看喜好項目} other {查看 {count} 個喜好項目}}` · `confirmed`（CLDR 中文只有 `other`
  一個複數類別，見 `style.md`；目錄裡每一條 ICU 複數都是這個形狀）。❗ **下一位譯者不要「補回」`one`
  那一支**：英文有三支是英文的事，中文寫 `one` 會是憑空多出來的分支。 `=0`
  不是複數類別而是明確值比對，所以留著，而且照英文的用意寫成「你還沒有」的說法，❌ 裡面不准出現 `0`。 `{count}`
  只出現在會顯示數字的那一支，`=0` 那支不帶；兩側留空格（`style.md` § "Spacing"）。量詞用 `個`，跟目錄裡
  `個{count, plural, other {項目}}` 一致，不用 `項`（會跟 `項目` 疊字）。
- **`查看`（See）** · 目錄既有寫法（`commands.logOperationLog.description`、`commands.appLicenseKey.seeDetails.label`
  都把 "See …" 寫成 `查看…`）· `high`。
- 這十條值裡都沒有撇號，所以 ICU 的雙寫撇號規則用不到；`menu.go.showFavorites`
  是 RAW 鍵，就算以後要加撇號也只能用單撇號。標點全形，只有英文原句有句號的那條（`commands.favoritesOpen.description`）才收
  `。`。

## 退出被擋下來時，說出是誰佔著磁碟機（`errors.eject.unmountRefusedBy*`、`errors.eject.otherApps`）

按下退出、macOS 不放行時的六條。全部落進 `fileExplorer.pane.ejectFailedToast`（`無法退出 {volumeName}：{message}`）或
`fileExplorer.pane.disconnectFailedToast`（`無法中斷連線：{message}`），所以每一條都是全形冒號**後面**那句話。整族的句形照抄已經上線的
`errors.eject.unmountRefused`（`還有東西在用這個磁碟機。請關掉開著的檔案和 App，然後再退出一次。`）：先講誰佔著，再講要關掉什麼，最後
`然後再退出一次`。❗ 這一族是 RAW 家族，不經過 ICU，`{app}` / `{apps}`
是字面替換，撇號一律單引號（中文這六條本來就沒有）。

- **`{app}` / `{apps}` 裸用、不加直角引號** · 目錄裡每個執行期填入的名字都是裸的（`無法退出 {volumeName}：`、
  `{name} 正在中斷連線`、`{volumeName} 在 Cmdr 把 …`）· `high`。❗ Apple 相反：AppKit 的
  `The disk could not be ejected because it is in use by “%@”.` → `磁碟正由「%@」使用中，因此無法退出。`
  有加引號。這裡**不跟**，因為 `{apps}` 是 `Intl.ListFormat`
  併好的一整串，沒有辦法逐項加引號，單數那條加了就跟複數那條不成一家。依 `style.md` §
  "Spacing"，兩個佔位符都兩側留空格。
- **is still using this drive → `還在用這個磁碟機`** · `磁碟機` 是已定的 drive（見 `terms.json` `drive`），`還在用`
  逐字沿用 `errors.eject.unmountRefused` 的 `還有東西在用這個磁碟機` · `high`。❗ 不要改寫成 Apple 的 `正在使用中`
  書面語：同一族的兄弟鍵已經上線，口語的 `還在用` 才是這個目錄的聲音。
- **Close anything it/they have open there → `請關掉它／它們在上面開著的東西`** · `關掉` + `開著`
  都是目錄自己的詞（`errors.eject.unmountRefused`、`errors.write.deletePending.suggestion` 的
  `關掉其他可能開著它的 App`）· `high`。`在上面` 回指前一句的 `這個磁碟機`（目錄的搭配是
  `還在磁碟機上`），避免第二次寫出 `這個磁碟機`。中文沒有數的變化，單複數兩條只差 `它` / `它們`。
- **other apps → `其他 App`** · 目錄自己的 app 就是拉丁的 `App`（111 處，`ai.local.warningCaution` 正好就是
  `其他 App 可能會變慢`），兄弟鍵 `errors.eject.unmountRefused` 也寫 `關掉開著的檔案和 App` · `high`。❗ **不要寫
  `其他應用程式`**：`應用程式` 在本目錄只留給 Apple 的既有標籤和「應用程式」資料夾（見 `terms.json`
  `app`、`applications-folder`）。小寫的英文 `other apps` 在中文沒有大小寫可分，純名詞片語，不加句號。
- **⚠️ `Intl.ListFormat('zh-Hant')` 的接合處是緊排的，字串救不了** · 實測（Node 24，2026-09-16）
  `['Preview','Warp','Photos','其他 App']` → `Preview、Warp、Photos和其他 App`，兩項時
  `mds_stores和Warp`。CLDR 的 zh-Hant 末項樣式是 `{0}和{1}`，`和` 直接貼上前面那個拉丁字，違反 `style.md` §
  "Spacing"。這是 ICU 資料，不是這六條的值能控制的； `其他 App` 自己的 `Han→Latin` 接合處有留空格。要修只能改
  `apps/desktop/src/lib/intl` 那層的併字方式，記在這裡免得有人回頭改字串。
- **disk image → `磁碟映像檔`** · AP-TW = AP-HK，Finder 簡介視窗的 `InfoWindowGeneralView.json`（`tvy-hx-Gou.title` =
  `磁碟映像檔：`），Finder 的 `BN53` 也是 `將磁碟映像檔「^0」燒錄至光碟⋯` · `confirmed`。第二次提到時縮成
  `那個映像檔`，跟英文的 `that image` 一樣。
- **A disk image stored on this drive is still open → `這個磁碟機上有一個磁碟映像檔還開著`**
  ·改成主題句（先講磁碟機、再講上面有什麼），比逐字的 `存在這個磁碟機上的磁碟映像檔` 好讀 · `high`。`開著` 不用
  `已裝載`：英文刻意講一般人聽得懂的 open。
- **macOS is still working with this drive → `macOS 還在處理這個磁碟機`** · `處理`
  是目錄自己的泛用進行式動詞（`askCmdr.tool.unknown.doing`、`queue.row.label` 的 `正在處理`）·
  `high`。❗ 這裡刻意**不**寫 `還在使用`：英文把 using（第 1、2 條）換成 working
  with，就是要讓人知道這件事沒有東西可關、只能等，`處理` 保住了那個差別。Apple 的
  `項目「^0」正由macOS所使用，所以無法打開。`（Finder `LA10`）證實 `macOS` 直接當主詞、原樣不譯。
- **Wait a minute / Wait a moment → `請稍等一下` / `請等一下`** · `稍等一下` 取自
  `errors.listing.resourceBusy.suggestion` · `high`。中文沒有辦法把這兩個一樣模糊的說法分得更開，靠 `稍`
  一個字留下差別就夠。
- **Cmdr itself → `Cmdr 自己`** · `自己` 扛下「是 Cmdr 的問題」這層意思 ·
  `high`。第 6 條的三段用逗號串成流水句（`請等一下再退出一次，如果一直這樣，請傳送一份報告。`），❌ 不用全形分號（`style.md`
  § "Punctuation"）。
- **send a report → `傳送一份報告`** · 見 `terms.json` `report`（`傳送報告`，AP-TW =
  AP-HK的「…並傳送報告給Apple」）；句中用量詞版本，與 `settings.updates.crashReports.description` 的 `自動傳送一份報告`
  一致，按鈕才是裸的 `傳送報告` · `high`。**if it keeps happening → `如果一直這樣`**，逐字沿用
  `errors.listing.resourceBusy.suggestion` · `high`。

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `種類`** · macOS Finder `ArrangeByMenu`
  `119.title`/`338.title`, the Kind sort criterion. Every source in this section comes from the reference pile's
  `zh-TW/` folder, which is where Traditional lives (`zh-Hant/` holds only the Microsoft sets) · `high`.
- **"Select all with extension `*.{extension}`" → `選取所有副檔名為 *.{extension} 的檔案`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" → `選擇所有相同副檔名`) and Total
  Commander (`WCMD.INC` `527` → `全選: 副檔名相同的項目`) name this exact command, and the mask replaces their "same
  extension" because Cmdr shows the concrete one · `high`. 遮罩是一段拉丁文字，依 `style.md` § "Spacing: put a space
  between Chinese and Latin" 的規定，兩側各空一格：`副檔名為 *.{extension} 的檔案`。
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `選取範圍`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description`（`加入選取範圍`） · `high`. Its siblings
  in that menu are verbs; this one names what the submenu holds. ❌ 不用動詞 `選取`（那是 `menu.bar.select`），也不用
  `最近的選取` 那個「過去查詢」的用法。
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

標題列上的警告小標籤和它的滑鼠提示，在 Cmdr 還沒拿到「完全取用磁碟」權限時顯示；按一下會把入門引導重新開到第 1 步。

- **`onboarding.fdaBadge.label` → `沒有「完全取用磁碟」權限`** · Apple 自己的「系統設定」項目名，在
  `SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable` 的 `ALL_FILES` 鍵上再次核對過（macOS 27.0
  build 26A428，2026-09-16 驗證）· `high`。TW 說 `完全取用磁碟`、HK 說 `完整磁碟取用`，依台灣預設。沿用目錄既有的 `「」`
  引號寫法，讓使用者一眼認出那是「系統設定」裡的項目名。
- **`onboarding.stepAi.bannerTitle.denied` 的英文和它完全相同**，`i18n-terms`
  會要求兩者一字不差；那一條本來就是這個寫法。❌ 兩條不能單獨改詞。
- **`onboarding.fdaBadge.ariaLabel`
  以標籤原文開頭**（`沒有「完全取用磁碟」權限。開啟入門引導的「完全取用磁碟」步驟。`），這正是 `i18n-aria`（WCAG
  2.5.3）要的包含關係。`i18n-aria` 比對時會把 `「」` 和 `。`
  都去掉，所以引號不影響包含判定，但 ❌ 單獨改標籤還是會把它弄壞。
- **提示裡的其他詞**：drive → `磁碟機`（已定），cloud folders → `雲端資料夾`，file → `檔案`，"files macOS keeps to
  itself" → `macOS 自己留著的檔案`（說人話，❌ 不要點名某個 macOS 功能），"Click to …" →
  `按一下就能…`（已定寫法），onboarding → `入門引導`（已定）。

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`, `errors.write.readError.title.trash`, `errors.write.writeError.title.trash`)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Chinese has no
  number agreement, so all three messages lead with `你選的項目裡有 {count} 個，…`, which reads the same at 1 as at 7.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails` (`技術詳細資訊`),
  because it points at that very control.

- **locked → `已鎖定`** · macOS Finder（`AXNODE1` `已鎖定`）與目錄既有寫法 · `high`。
- **"delete them permanently" → `永久刪除`** · 與 `commands.fileDeletePermanently.label`
  一致，讓建議裡的說法就是使用者要按的那個指令 · `high`。
- **badge（標題列上的那個小藥丸）→ `警告標記`** · `tentative`；描述性說法。❌ 不要用 `標籤`，本目錄把 `標籤`
  留給 tag。title bar → `標題列` · `high`。
- **❗ 引號不能巢狀，所以這句話改寫過。** 徽章文字本身已經帶 `「完全取用磁碟」`，再包一層外引號就會變成 `『…』`
  巢狀，而本指南§ Punctuation 說 `『…』` 在語料裡完全沒出現、要改寫而不是巢狀。做法是用冒號把徽章文字原樣帶出來：
  `標題列上的警告標記寫著：沒有「完全取用磁碟」權限。` 這樣既逐字對上 `onboarding.fdaBadge.label`，又沒有巢狀引號。
- "somewhere macOS keeps to itself" → `macOS 自己留著的地方`，沿用 `onboarding.fdaBadge.tooltip`
  已定的說法。說人話，❌ 不要點名某個 macOS 功能。

## 僅存雲端內容的刪除警告列（`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`）

雲端資料夾裡被選取的項目如果只存在雲端，放進垃圾桶得先把它下載回來。所以 Cmdr 改開永久刪除的對話方塊，並在警告列裡說清楚。警告列有兩個版本：一個用於混合選取，一個用於全部僅存在雲端的選取。兩者只在第一句和能提供的出路上不同。第三個鍵是 Cmdr 把一次按鍵交還給使用者時顯示的那行字。

- **`.cloudOnlineOnlyMixedWarning`** · 「僅存在雲端」對應 Finder 的「僅限線上」說法；「垃圾桶」「雲端服務」「副本」取自
  `terms.json` · medium。
- **`.cloudOnlineOnlyAllWarning`**
  · 同一段文字，只把「你選取的內容中有一部分」換成「你選取的內容全都」，並拿掉「取消選取」這條出路：全部僅存在雲端時，取消選取就什麼都不剩了 ·
  medium。
- **`.cloudOnlineOnlyHandedBack`** · 按鈕上方的那行字，出現在 Cmdr 刻意沒有執行的一次按鍵之後。語氣平實，不必道歉 ·
  medium。
- **四個事實都得保留**：（1）垃圾桶會先下載檔案，（2）所以 Cmdr 只提供刪除整個選取範圍，（3）之後垃圾桶裡沒有副本，但雲端服務自己有（❌ 不要寫成「反正還在垃圾桶裡」），（4）警告列裡點出的出路。
- **兩處 `<strong>` 必須保留**，分別在「先下載回來」和動詞「刪除」上。引號裡的「刪除」是按鈕的文字：始終與
  `fileOperations.delete.confirmDelete` 一致。
- 不需要 `sameAsSourceJustification`：所有值都與英文不同。
- 做溢位檢查時看一下：警告列很長，且位於檔案列表上方的窄條裡。

## 伺服器明講沒有這個共享資料夾（`fileExplorer.network.osMountFallback.shareNotOnServer`、`fileExplorer.pane.directConnectionShareNotOnServerToast`）

這一族裡唯一「再試也沒用」的情況：伺服器給了明確答案，上面沒有這個名字的共享資料夾。所以這則通知沒有按鈕，語氣也不能帶任何「暫時」的意味（不寫
`現在`，不寫 `再試一次`），這正是它跟兄弟鍵不一樣的地方。

- **"the server says it has no share by that name" → `因為伺服器說它沒有這個名字的共享資料夾`**
  ·目錄（`errors.mount.shareNotFound`）與 NetAuthAgent
  `EINFO_NO_SHARE`（系統裡裝著的 bundle，TW 與 HK 同字：`共享「%@」不存在於伺服器上。`，macOS
  26.6.2，25G83，2026-09-17）· `high`。Apple 寫 `共享`，這個目錄的名詞一律帶 `資料夾`（§ 共享資料夾裝載不了）。開頭沿用
  `fileExplorer.network.osMountFallback.message` 的 `無法直接連線到 X`。
- **"This one won't sort itself out" → `這種情況等下去也不會好`**
  ·語料裡沒有對應說法；這是最自然的口語講法，也點出這則通知跟其他幾則的差別：等不到結果 · `high`。
- **"may have been renamed or removed" → `可能……被重新命名或移除了`** · 逐字沿用
  `errors.write.destinationNotFound.suggestion` · `high`。
- **"so it's worth checking there" → `值得去那邊看看`** · `那邊` 回指伺服器，省掉重複 · `high`。
- **"a lot slower" → `慢上許多`** · 這裡英文沒給倍數，和給了 `4 倍` 的兄弟鍵不同 · `high`。
- **短提示省略主語（`所以會繼續使用系統連線`）** · 前半句已經把話題定在共享資料夾上，再寫一次就囉嗦；結尾
  `會繼續使用系統連線` 與 `fileExplorer.pane.directConnectionUnreachableToast` 等三則一致 · `high`。
- 兩條值都跟英文不同，不需要 `sameAsSourceJustification`；`{server}`、`macOS`、`SMB` 前後留空格。

## 伺服器上「看起來一樣、寫法不同」的名稱（`fileOperations.transferProgress.lookAlikeHint`、`errors.listing.ambiguousName.*`、`errors.volume.ambiguousName`）

SMB 共享上兩個名稱在畫面上一模一樣，但伺服器存成不同的字元（`é` 一個字元，或 `e`
加一個獨立的重音符號；有些伺服器還分大小寫）。這一族的四條錯誤訊息和一條衝突提示講的是同一件事，用同一套說法。

- **"the server spells them differently" / "stores them spelled differently" → `伺服器儲存的寫法不同`**
  · 語料沒有對應說法；`寫法`
  是口語裡講「同一個字寫成不同樣子」最自然的詞，而且不必搬出 Unicode、正規化之類的術語（英文描述明確要求）·
  `high`。兩處用同一個片語，❌ 別在其中一處改成 `拼法`（中文的 `拼` 指拼音，會讓人以為是讀音不同）。
- **accent mark / accented letter** · `重音符號` / `帶重音符號的字母` · MS zh-Hant TBX（`accent` → `重音符號`，另有
  `diacritic` → `變音符號`）、Total Commander zh-TW（`將帶有重音符號的名稱儲存到額外欄位中`）· `high`。❗ 不寫光禿的
  `重音`：那通常指讀音的輕重，不是字母上的符號。
- **"the one that's there"（衝突步驟裡目的地已有的那一個）** · `現有的那一個` · 呼應同一個步驟的欄位標籤
  `fileOperations.transferProgress.existingFolderLabel` `現有（資料夾）：` · `high`。
- **按鈕名稱在說明句裡** · `按一下「覆寫」` · 按鈕文字逐字沿用 `fileOperations.transferProgress.conflictOverwrite`
  `覆寫`，依 § Punctuation 加角括號，動詞依 style.md 的 `按一下` · `high`。
- **match（一個路徑符合幾個項目）** · `符合` · 目錄既有用法 · `high`。
- 所有值都與英文不同，不需要 `sameAsSourceJustification`。

## 「允許使用雲端 AI」開關，以及雲端 AI 關閉時的提示（`ai.cloudConsent.*`、`askCmdr.gate.*`）

這是一個隱私同意開關：預設關閉，開啟之前什麼都不會離開這部 Mac。文字要平靜，絕不能誇大 Cmdr 做的事。M1 上沒有參考資料庫，證據取自已安裝的 macOS，做法見
`docs/i18n/reference-pile/how-to-mine.md` 的 "No pile on this machine?" 一節。

- **Allow cloud AI（開關標籤，`ai.cloudConsent.label`）** · `允許使用雲端 AI` · `Allow` → `允許` 是 macOS
  zh-TW 和 zh-HK 權限請求裡的按鈕，兩地一致（`TCC.framework` `Localizable.loctable`，`REQUEST_ACCESS_ALLOW`，macOS
  26.6.2 build 25G83，2026-09-23 讀取）；`雲端 AI` 是 `settings.ai.provider.opt.cloud` 已發布的值。❗ 這裡是 `允許` 不是
  `同意`：`同意` 在本目錄是 _approve_（核可一個動作），而這個開關是放行資料送出，正是 Apple 的 `允許` · `high`
- **雲端 AI 關閉的狀態** · `雲端 AI 已關閉`，與 `ai.translateError.off.title`（`AI 已關閉`）同一形式 · `high`
- **引用開關時逐字照搬標籤。** `settings.ai.cloudConsent.lockedHint` 用
  `「允許使用雲端 AI」`；`askCmdr.gate.cloudOff.body` 和 `settings.askCmdr.cloudOffHint` 在句中直接寫
  `允許使用雲端 AI`。英文只說 “Allow it” 的地方寫 `允許使用` （`請到「設定 > AI」允許使用`）。
- **Settings > AI** · `「設定 > AI」`，角括號、`>`，與 `ai.translateError.*` 的兄弟字串一致。
- **AI service / cloud AI service** · `AI 服務` / `雲端 AI 服務`（`settings.ai.cloudProvider.description` 已用）；英文把
  `service` 和 `ai.cloudConsent.askCmdr.*` 裡的 `provider`（`提供者`）分開，中文也分開。
- **custom endpoints** · `自訂端點` · `high`
- **side panel** · `側邊面板` · `high`
- **chats（Ask Cmdr 的對話記錄）** · `對話`，沿用 `askCmdr.sessions.empty`（`還沒有對話記錄`）· `high`
- **摺疊區裡的功能名（`<b>` 內）**：`新資料夾的名稱建議`、`用自然語言搜尋`（沿用 `queryUi.bar.aria.ai`）、`依描述選取` ·
  `high`
- `settings.askCmdr.enabled.label` = `Ask Cmdr`，與英文相同，帶 `sameAsSourceJustification`（產品名，同
  `settings.section.askCmdr`）。

## 移動過程中有過更動的原檔（`transfer.changedDuringMove`，2026-09-25）

- **changed during the move → `移動過程中有 … 發生更動`** · Finder `PE56` 「燒錄時有一個或多個項目發生更動」(Finder
  `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25)；`更動` 也是
  `fileOperations.cancelRollback.reason.drift.counted` 的用字 · `high`。`移動過程中有` 和 `等來源資料夾`
  照搬同一則通知裡的 `transfer.appearedDuringMove`。
- **佔位符兩側加空格**：`留在 {scope…} 中`（style.md § Spacing）。順手把 `transfer.appearedDuringMove` 原本的
  `出現在{scope…}中` 也改成加空格的寫法，兩句同框才一致。

## 「開啟檔案的應用程式」和「分享」子選單裡的等待行（`menu.context.openWithLoading`、`.shareLoading`、`.shareNone`，2026-09-24）

## 「開啟檔案的應用程式」和「分享」子選單裡的等待行（`menu.context.openWithLoading`、`.shareLoading`、`.shareNone`）

- **Finding apps…** · `正在尋找 App…` · 沿用 `正在…` 進度模式（見 loading 列）；`App` 同 `選擇 App…` 和
  `settings.behavior.textEditorApp.checking`，中英之間加空格 · `high`
- **share options** · `分享選項` · `分享` 即子選單名（share (verb) 列） · `high`
- **No share options** · `沒有分享選項` · macOS 空選單的說法（`No Services Apply` → `沒有可套用的服務`） · `high`
