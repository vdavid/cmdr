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
- **`完成復原`** (Finish rolling back, `operationLog.dialog.finishRollBack` =
  `fileOperations.rollbackConfirm.finishRollBack`): `繼續復原` would be blunter about not restarting, but `繼續` is
  spent on `queue.row.resume`.
- **`警告標記`** for the title-bar badge (`errors.write.trashRefused.suggestion.noFullDiskAccess`): descriptive.
- **`僅存在雲端`** (`online-only`; prose says `只存在雲端`) and the three `fileOperations.delete.cloudOnlineOnly*` rows:
  a draft never read by a human; check the long warning bar for overflow too.

## Phrasing

- **`越來越長了`** (`servers.pinHint.title`), **`手機一接上就會偵測到。` / `目前不會偵測有沒有手機接上。`**
  (`settings.adb.status.*`), **`怎麼開啟？`** (`adb.hint.*`): composed, no source has these observations.
- **`來來去去`** (`settings.revealHandler.notProductionBuild`), **`停住不動了`** (the viewer's fetch stall), **`跟上`**
  (`indexing.staleDialog.bodyPhone`), **`編輯檔案時使用`** (`settings.behavior.textEditorApp.label`): judgment calls.
