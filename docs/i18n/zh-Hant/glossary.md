# zh-Hant glossary

The living term glossary for translating Cmdr into Traditional Chinese: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/zh-TW/` and `zh-HK/` (macOS + the five
  file managers) and `_ignored/i18n/zh-Hant/` (the Microsoft terminology TBX and style guide) for how each is rendered,
  and for similar sentences to model phrasing on. Recipes: `docs/i18n/reference-pile/how-to-mine.md`.
- **Read `style.md` first**, especially § The Apple-zh-TW outlier rule: five of the highest-traffic terms below take the
  pan-Traditional consensus form over Apple's zh-TW one, deliberately.
- **This folder is the language home.** Capture new term decisions here, other findings as sibling files.

Source abbreviations: **AP-TW** / **AP-HK** / **AP-CN** = macOS Finder + AppKit + SystemSettings in that locale; **MS**
= Microsoft zh-Hant terminology TBX; **NAU** / **DOL** / **THU** = GNOME Nautilus / KDE Dolphin / Xfce Thunar (zh-TW);
**TC** / **DC** = Total Commander / Double Commander (zh-TW). All evidence verified 2026-08-29.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

### The five consensus overrides (Apple-zh-TW is the outlier)

Rationale, counts, and the "don't revert this" warning: `style.md` § The Apple-zh-TW outlier rule.

- **folder** · `資料夾` · AP-HK (228), MS, TC, DC, NAU, DOL, THU; AP-TW says 檔案夾 (233) and stands alone · `high`
- **copy** · `複製` · AP-HK, MS, all five file managers; AP-TW says 拷貝 · `high`
- **move** · `移動` · AP-HK, MS, TC, DC; AP-TW says 搬移 · `high`
- **open** · `開啟` · AP-HK, MS, all five file managers; AP-TW says 打開 · `high`
- **tab (browser-style)** · `分頁` · AP-HK, MS, NAU, DOL, THU, DC; AP-TW says 標籤頁, which also collides with 標籤 =
  tag · `high`

### Core file-manager nouns

- **file** · `檔案` · AP-TW (414), AP-HK (183), MS · `high`. ❗ Simplified 文件 = file, but Traditional 文件 =
  _document_.
- **document** · `文件` · AP-TW/HK (`Documents` → 文件), MS · `high`
- **directory** · `目錄` · MS, TC (13), DC (13), NAU (20), DOL (25), THU (34); Apple barely uses it · `high`. Prefer
  `資料夾` in user-facing copy; keep `目錄` where the English deliberately says "directory" (a technical/path sense).
- **subfolder** · `子資料夾` · AP-HK, TC, DC; AP-TW says 子檔案夾 (follows the folder override) · `high`
- **parent folder** · `上層資料夾` · AP-HK (`Go To Enclosing Folder`); AP-TW says 上層檔案夾 · `high`
- **home folder** · `個人專屬資料夾` · AP-HK; `Home` alone is `個人專屬` in both · `high`
- **desktop** · `桌面` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **item** · `項目` · AP-TW/HK (`Items`), MS · `confirmed`
- **path** · `路徑` · AP-TW/HK, MS · `confirmed`. Path bar → `路徑列` (AP).
- **filename** · `檔名` (41/41) or `檔案名稱` in full · AP-TW/HK · `high`
- **file extension** · `副檔名` · AP-TW (38), AP-HK (40), MS (`file name extension`) · `high`. `擴展名` = 0 in the pile.
  AP-TW is internally inconsistent here (`延伸功能` in two keys); take `副檔名`.
- **disk** · `磁碟` · AP-TW (69), AP-HK (70), MS · `confirmed`. `啟動磁碟` = startup disk. `磁盤` = 0 everywhere.
- **hard disk** · `硬碟` · AP-TW/HK · `high`
- **drive (the device)** · `磁碟機` · MS; Apple has no distinct word and reuses 磁碟 · `high`. Cmdr distinguishes
  "drive" from "disk" (drive indexing vs startup disk), so we take Microsoft's distinct term for the device and keep
  `磁碟` for the disk itself.
- **volume (mounted disk)** · `卷宗` · AP-TW (68), AP-HK (69) · `high`. ❗ NOT audio loudness (`音量`), and note AP-CN
  reverses the characters to 宗卷. MS says 磁碟區; Apple wins because TW and HK agree.
- **hidden file** · `隱藏檔案` / `隱藏檔` · AP-TW, NAU, THU, DOL, MS (`hidden` → 隱藏) · `high`
- **alias** · `替身` · AP-TW = AP-HK (`Make Alias` → 製作替身) · `confirmed`. Apple's word for a macOS alias.
- **shortcut (Windows-style link)** · `捷徑` · TC, AP (`Shortcuts`), MS · `high`. Distinct from 替身 and from
  `鍵盤快速鍵`.
- **symlink** · `符號連結` · DC, NAU, MS · `high`. Not in Apple's bundles.

### Operations

- **delete** · `刪除` · AP-TW = AP-HK, MS · `confirmed`
- **rename** · `重新命名` · AP-TW = AP-HK = AP-CN, MS, all file managers · `confirmed`
- **duplicate** · `製作副本` · AP-HK · `high`. **Deliberately the HK form**: AP-TW's `複製` is the word we already use
  for _copy_, and Cmdr ships both commands (`commands.fileDuplicate.*`). DOL's `在此建立複本` and NAU's `再製` are the
  alternatives we passed over.
- **Trash (noun)** · `垃圾桶` · AP-TW = AP-HK (`Trash` and `Bin` both) · `confirmed`. AP-CN says 废纸篓; a real term
  split, not just character shape.
- **Move to Trash** · `移至垃圾桶` · AP-HK, THU, DOL; AP-TW is itself inconsistent (`丟到垃圾桶` and `移至垃圾桶`) ·
  `high`
- **empty Trash** · `清空垃圾桶` · AP-TW = AP-HK · `high`
- **compress / zip** · `壓縮` · AP-TW = AP-HK (`Compress Items` → 壓縮項目), MS, TC, DC, NAU · `confirmed`
- **extract / unzip** · `解壓縮` · MS, TC, DC · `high`. Not in Apple's bundles; NAU's `取出` was passed over.
- **archive (noun)** · `封存檔` · AP-TW = AP-HK (`Archive` → 封存) · `high`. TC/DC say `壓縮檔`; use that when the
  English clearly means a zip file specifically rather than the general archive concept.
- **open with** · `開啟檔案的應用程式` · AP-HK; AP-TW says 打開方式 / 打開檔案的應用程式 · `high`
- **new folder** · `新增資料夾` · AP-HK, TC · `high`
- **save** · `儲存` · AP-TW (123), AP-HK (123), MS · `confirmed`. `保存` = 0 in both; it's the Simplified form (AP-CN
  110).
- **undo** · `還原` · AP-TW = AP-HK, NAU · `high`. MS says `復原`; Apple wins.
- **redo** · `重做` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **cut** · `剪下` · AP-TW = AP-HK, MS · `confirmed`
- **paste** · `貼上` · AP-TW = AP-HK, MS · `confirmed`
- **select all** · `全選` · AP-TW = AP-HK, MS · `confirmed`
- **select (verb)** · `選取` · AP, MS · `confirmed`. AP-TW keeps `選擇` for _choose one thing_ (`A27` `選擇啟動磁碟`,
  `N173` `選擇替身「^0」要打開的項目`) and uses `選取` for picking items out of a list (`SB18`
  `已選取^0個項目（共^1個）`, `IN_S49` `選取`). Cmdr's Select-files dialog is the list sense.
- **deselect (verb)** · `取消選取` · MS (`deselect` id 44722 → 取消選取 id 44741), AP-TW = AP-HK (`MenuBar.json`
  `300488.title` `Deselect All` → `取消全選`) · `high`. ⚠️ **Simplified diverges on that SAME Microsoft entry** (id
  44722 → `取消选择`), because its select verb is `选择`. Not a character mapping; never convert either way. See §
  Select / Deselect files dialog.
- **refresh** · `重新整理` · AP (`NSRefreshFreestandingTemplate`), MS, DC, NAU · `confirmed`. **reload** → `重新載入`
  (DOL). ❌ Avoid `刷新` (Simplified-influenced; it leaks into DC's zh-TW file).
- **eject** · `退出` · AP-TW = AP-HK, MS, NAU, THU · `confirmed`. (AP-CN's `推出` is a typo in Apple's own data.)
- **mount** · `裝載` · AP-TW = AP-HK (`couldn't be mounted` → 無法裝載卷宗…) · `high`. NAU/THU say `掛載`, MS `掛接`;
  Apple wins.
- **unmount** · `卸除` · AP-TW = AP-HK (`unmount servers` → 卸除伺服器) · `high`. NAU/THU/MS say `卸載`.
- **mount point** · `裝載點` · **coined** from Apple's 裝載 + MS's 掛接點 pattern · `tentative`. Unattested in every
  source; flag it if a reviewer ever appears.
- **share (verb)** · `分享` · AP-TW (`#N166`), MS · `high`. Apple is messy here: TW writes `共享與權限：` where HK
  writes `分享與權限：`, i.e. the two locales swap the words. `分享` is the safer single choice.
- **permissions** · `權限` · AP-TW = AP-HK, MS, all file managers · `confirmed`
- **owner** · `擁有者` · AP-TW = AP-HK, MS, all file managers · `confirmed`
- **group** · `群組` · AP-TW = AP-HK, MS · `confirmed`
- **read / write** · `讀取` / `寫入` · AP-TW/HK, MS · `confirmed`. Read-only → `唯讀`.

### UI chrome

- **pane (the two-pane sense)** · `窗格` · MS; also what Cmdr's `zh` catalog uses · `high`. ❗ Apple has no word for it
  (its `設定面板` is a _settings_ pane). TC calls a pane `視窗` (collides with window) and DC calls it `面板` (collides
  with a settings panel), so `窗格` is the only unambiguous choice. See § Two-pane vocabulary.
- **window** · `視窗` · AP-TW = AP-HK, MS, universal · `confirmed`
- **sidebar** · `側邊欄` · AP-TW = AP-HK, NAU, MS · `confirmed`
- **toolbar** · `工具列` · AP-TW = AP-HK, MS, DOL, THU, DC · `confirmed`
- **status bar** · `狀態列` · AP-TW = AP-HK, MS, DOL, THU · `confirmed`
- **column** · `直欄` · AP-TW = AP-HK (`Columns` → 直欄, `Column View` → 直欄顯示方式) · `high`. MS says `資料行`; Apple
  wins.
- **sort** · `排序` · AP-TW, MS, all five file managers; AP-HK says 排列 · `high`
- **view (menu noun / view mode)** · `顯示方式` · AP-TW = AP-HK (`View` → 顯示方式) · `high`
- **view (verb, "to look at")** · `檢視` · AP (`to View %@` → 檢視), MS, TC (`View` menu → 檢視) · `high`. ❗ Don't swap
  the two: see `style.md` § Notes.
- **preview** · `預覽` · AP-TW = AP-HK, MS, DOL, THU · `confirmed`
- **Quick Look** · `快速查看` · AP-TW = AP-HK = AP-CN · `confirmed`. Translated, not kept English (which is why it's not
  on the do-not-translate list).
- **menu** · `選單` · AP-HK, TC, DOL · `high`. MS's `功能表` is Windows house style; Cmdr is a Mac app.
- **dialog** · `對話方塊` · MS · `high`. Not in Apple's bundles.
- **settings** · `設定` · AP-TW (160), AP-HK (160), MS · `confirmed`. `設置` = 0 in both Apple corpora.
- **preferences** · `偏好設定` · AP-TW = AP-HK · `confirmed`
- **appearance** · `外觀` · AP-TW = AP-HK (SystemSettings) · `confirmed`
- **theme** · `主題` · AP-TW = AP-HK live (`Theme` → 主題, `Use Theme Color` → 使用主題顏色, Appearance pane, macOS
  26.6.2) · `high`. ❌ **Not `佈景主題`**, which is a Windows-ism: zero in both Apple corpora and zero across 12,418
  live zh_TW strings; only MS's TBX carries it (21). When the label means the OS light/dark switch rather than a named
  theme, Apple calls that surface `外觀`.
- **dark / light mode** · `深色模式` / `淺色模式` · MS · `high`. The appearance-mode strings aren't in the mined Apple
  bundles (深色/淺色 = 0 there), so MS is the only evidence.
- **language** · `語言` · AP-TW = AP-HK, MS · `confirmed`
- **keyboard shortcut** · `鍵盤快速鍵` (short: `快速鍵`) · MS · `high`. Not in Apple's bundles, and Apple's `捷徑` is
  taken by the Shortcuts app.
- **search** · `搜尋` · AP-TW (82), AP-HK (82), MS · `confirmed`. `搜索` is the Simplified form (AP-CN 69).
- **filter** · `篩選` · MS, DC (`Quick Filter` → 快速篩選) · `high`. NAU/DOL say `過濾`; not in Apple's bundles.
- **index / indexing** · `索引` (noun) / `建立索引` (verb) · AP (`Updating tag index` → 更新標籤索引, `Indexed`
  → 已製作索引), MS · `high`
- **tag** · `標籤` · AP-TW = AP-HK (Finder `Tags` → 標籤) · `high`. ❗ Reserved for tags only; tab is `分頁`.
- **bookmark** · `書籤` · AP (`NSBookmarksTemplate`), NAU, DOL, THU · `confirmed`
- **favorite** · `喜好項目` · AP-TW = AP-HK (SystemSettings `FAVORITE_LABEL`; also `喜好的伺服器：`) · `high`. MS says
  `我的最愛`; Apple wins.

### Transfer and state

- **progress** · `進度` · AP-TW = AP-HK (`Show Progress Window` → 顯示進度視窗), MS, THU · `confirmed`
- **queue** · `佇列` · MS, DC · `high`. Not in Apple's bundles.
- **cancel** · `取消` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **retry** · `再試一次` · AP-TW; AP-HK says 再試, MS says 重試 · `high`
- **skip** · `略過` · AP-TW = AP-HK, NAU, THU, DC · `confirmed`. MS says `跳過`; the zh-Hant sources are unanimous on
  `略過`.
- **pause / resume** · `暫停` / `繼續` · AP-TW = AP-HK (`Resume` → 繼續), MS (pause) · `high`. ❗ MS renders _resume_ as
  `履歷表` (the CV sense), so don't use MS here.
- **overwrite** · `覆寫` · AP-TW = AP-HK (`Overwrite at Destination` → 覆寫目標), MS · `high`. TC/DC say `覆蓋`.
- **replace** · `取代` · AP-TW = AP-HK, NAU, THU, MS · `confirmed`
- **conflict** · `衝突` · AP-TW = AP-HK, MS · `confirmed`
- **remaining / time remaining** · `剩餘` / `剩餘時間` · AP-TW = AP-HK (`Estimating time remaining…` →
  `正在估計剩餘時間⋯`) · `high`
- **speed** · `速度` · AP-TW = AP-HK · `high`
- **transfer** · `傳輸` · TC · `high`. Apple uses `傳送` for sending items. ❗ MS's `移轉` is the business sense; don't
  use it.
- **copying… (in progress)** · `正在複製…` · AP-HK (`正在將「^1」複製到「^2」`); AP-TW uses 拷貝 per the override ·
  `high`
- **in progress (the pattern)** · `正在` + verb · AP-HK is consistently `正在…`; AP-TW mixes a `…中` suffix (`載入中⋯`)
  · `high`. Cmdr uses `正在…` everywhere, matching its `zh` catalog.
- **done / finished** · `完成` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **failed** · ❌ don't write `失敗` in a sentence; rewrite as `無法` + verb · AP-TW/HK do this systematically
  (`Failed to save…` → `無法儲存…`) · `high`. See `style.md` § Voice and tone.
- **warning** · `警告` · AP-TW, MS · `confirmed`. (AP-HK has a typo, `警吿` with U+544E; ignore it.)
- **confirm** · `確認` · AP-TW = AP-HK, MS · `confirmed`
- **apply** · `套用` · AP-TW = AP-HK, MS · `confirmed`
- **close** · `關閉` · AP-TW = AP-HK, MS · `confirmed`
- **back / forward** · `返回` / `前進` · AP-TW = AP-HK (SystemSettings accessibility labels) · `high`. ❗ MS is useless
  here (`back` → BACK 鍵, `forward` → 轉接).
- **up (to parent)** · `前往上層資料夾` · AP-HK (`Go To Enclosing Folder`) · `high`
- **loading** · `正在載入…` · AP-HK; AP-TW says 載入中⋯ · `high`
- **empty (verb)** · `清空` · AP-TW = AP-HK · `high`. `空白` is the adjective ("blank"), as in `空白光碟`.

### Network and storage

- **server** · `伺服器` · AP-TW = AP-HK (`Connect to Server…` → 連接伺服器⋯), MS, TC · `confirmed`
- **share (an SMB network share, noun)** · `共享資料夾` · derived from Apple's `Shared` → `已共享` plus the folder
  ruling · `tentative`. Not attested as a standalone noun in any source; the pile only has the adjective and
  `共享與權限：` / `分享與權限：`. Revisit if a reviewer appears.
- **network** · `網路` · AP-TW (33), MS, TC; AP-HK says 網絡 (15) · `high`
- **connect** · `連線` (noun/label) / `連接` (verb in a sentence) · AP-TW = AP-HK (`Connect` → 連線; `連接伺服器⋯`), MS
  · `high`
- **disconnect** · `中斷連線` · AP-TW = AP-HK, MS · `confirmed`
- **remote** · `遠端` · AP-TW, MS, THU; AP-HK says 遙距 · `high`
- **local** · `本機` · AP-TW = AP-HK, MS, NAU · `confirmed`
- **cloud** · `雲端` · AP-TW = AP-HK (`Cloud Storage` → 雲端儲存空間), MS · `confirmed`
- **iCloud Drive** · `iCloud 雲碟` · AP-TW = AP-HK (both render it `iCloud雲碟`, 49 occurrences each; AP-CN says
  `iCloud云盘`) · `high`. Apple localizes the descriptor, so this is NOT a kept-English brand like the sibling
  `errors.provider.*` names. **Spaced**, against Apple's tight rendering: a brand + Han descriptor is a Latin run like
  any other, and the sources that share our spacing convention space these compounds (Microsoft zh-Hant 7,667 spaced /
  11 tight, including `Office LTSC 專業版` and `USB 磁碟機`). Full ruling and the counterargument it answers: `style.md`
  § Spacing.
- **memory (RAM)** · `記憶體` · AP-TW = AP-HK (Info window `記憶體：`), MS · `confirmed`. AP-CN says 内存.
- **storage** · `儲存空間` · AP-TW = AP-HK (`Manage Storage…` → 管理儲存空間⋯), MS · `confirmed`
- **free space** · `可用空間` · AP-TW (`因為可用空間不足`), MS; AP-HK says 未使用空間 · `high`
- **size** · `大小` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **modified date** · `修改日期` · AP-TW = AP-HK = AP-CN · `confirmed`
- **created date** · `製作日期` · AP-TW = AP-HK · `high`. AP-CN says 创建日期.
- **USB device** · `USB 裝置` · **composed**: `USB` is unattested in the Traditional macOS bundles and in MS, and `裝置`
  = device is standard · `tentative`
- **eject / device removal** · see `退出` above.

### Search, filters, and the query UI

- **regular expression** · `正規表示式`, tight chip `正規式` · TC ships both (`WCMD.LNG` 5616 `正規表示式(&2)`, 5615
  `正規式(&X)`) · `high`. MS and DC say `規則運算式`, which is Windows house style; TC is the lineage match and
  `正規表示式` is what Taiwanese developers write. ❌ **Never bare `正規`**: it's a bound morpheme, not a noun, and
  Microsoft's only six `正規*` entries are `正規化` = _normalization_, so a chip reading `正規` parses as a truncated
  "normaliz-". TC's own short form `正規式` is the chip; the full form goes in aria labels and tooltips, which have
  room.
- **wildcard** · `萬用字元` · MS, THU · `high`. ❌ Not `通配符`, which is the Simplified form.
- **comparator / comparison operator** · `比較運算子` · MS · `high`
- **byte (the unit word)** · `位元組` · MS · `high`. ❗ Simplified writes `字节`; Traditional does NOT.
- **scope** · `範圍` · MS · `high`
- **glob** · `Glob`, kept verbatim · **unattested**: 0 hits in AP-TW, AP-HK, MS, TC, and DC · `tentative`. Justified on
  `queryUi.ai.patternLabel.glob`.
- **entry (an index record)** · `項目` · reuses the `item` ruling; MS's `輸入` is the data-entry sense (trap 4) · `high`
- **cursor (the file-list cursor)** · `游標` · TC (`游標所在的檔案` = the file under the cursor, `游標顏色`) · `high`.
  ❗ MS's `資料指標` is the mouse-pointer sense; TC names exactly Cmdr's concept.
- **context menu** · `右鍵選單` · THU, DOL · `high`. AP-TW's `特色選單` (from `Show Contextual Menu`) is opaque, and
  MS's + TC's `操作功能表` carries the Windows `功能表`; NAU's `情境選單` was the runner-up. `右鍵` is already the
  catalog's word for a right-click (`fileExplorer.navigation.favoriteTooltip`).

### AI, chat, and the agent

- **agent (the AI acting for the user)** · `代理程式` · MS · `high`. ❗ Not `代理人`, which is a human proxy.
- **provider (an AI provider)** · `提供者` · MS; also what agent 1 shipped in `settings.askCmdr.provider.title` · `high`
- **endpoint** · `端點` · MS · `high`
- **model** · `模型` · MS · `confirmed`
- **token** · `token`, kept verbatim · unattested in every Traditional source, and the app already writes it bare in
  `askCmdr.error.localWindowTooSmall` · `tentative`. Always spaced: `1,234 個 token`.
- **chat (the noun, one conversation)** · `對話` · standard Traditional usage; `聊天` stays the verb ("to chat") ·
  `high`. The chat LIST is `對話記錄`.
- **archive (a chat, verb)** · `封存` · AP-TW = AP-HK (`Archive` → 封存) · `high`. Same word as the archive-file noun,
  and unambiguous in context.
- **wizard** · `精靈` · MS · `high`
- **repository (a GitHub repo)** · `儲存庫` · GitHub's own zh-TW UI; unattested in the pile, and MS's `儲存機制` is the
  abstract data-store sense · `tentative`

### Progress, rollback, and destructive actions

- **roll back / rollback** · `復原` · MS · `high`. ❗ Deliberately NOT `還原`, which this catalog spends on _undo_
  (`menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.undo`). Cmdr ships both concepts on the same
  progress dialog, so they must stay two words.
- **background (running out of sight)** · `背景` · MS, AP-TW (`背景` in AppKit) · `high`. ❗ The `*Aria` pair
  constraint: `fileOperations.transferProgress.background` = `背景執行` must stay a verbatim substring of
  `backgroundAria` = `讓它繼續在背景執行` (WCAG 2.5.3). Reword neither alone.
- **queue (the button that sends a transfer to the queue)** · `加入佇列` · composed on MS's `佇列` · `high`. ❗ Same
  containment pair: `加入佇列` opens `queueAria` = `加入佇列，移到「操作佇列」視窗管理`.
- **hard link** · `硬式連結` · DC (exact `Create hard link` msgid match) · `high`. ❗ MS's `永久連結` is the permalink
  sense (trap 4).
- **absolute path** · `絕對路徑` · THU · `high`. MS's `完整路徑` is taken: Cmdr uses it for the English "full path"
  (`fileOperations.validation.pathTooLong`).
- **null character** · `Null 字元` · MS · `high`
- **scroll** · `捲動` · AP-TW (`scroll up by a page` → `向上捲動一頁`) · `high`. Page Up / Page Down take Apple's whole
  phrase: `向上捲動一頁` / `向下捲動一頁`.
- **throughput** · `輸送量` · MS · `high`

### Viewer, media, and image metadata

Mined from the Apple apps that do these jobs (Preview, QuickTime Player, TextEdit, Photos, the Spotlight metadata
schema), read off the live OS as `.loctable`s carrying zh_TW and zh_HK side by side (macOS 26.6.2, build 25G83,
key-match, 2026-08-29).

- **viewer** · `檢視器` · TC (`內建檢視器`), DC (`Viewer` → 檢視器) · `high`. The orthodox pair is the lineage match:
  Apple's own viewer is the Quick Look brand and yields no generic noun.
- **thumbnail** · `縮圖` · AP-HK (Preview), MS (HKG, TWN), DC, NAU; **AP-TW says 縮覽圖 and stands alone** · `high`.
  Same shape as the five consensus overrides above, so the consensus form wins.
- **zoom / zoom in / zoom out** · `縮放` / `放大` / `縮小` · AP Preview (TW = HK), MS · `high`. **zoom to fit** →
  `縮放到適當大小`, **actual size** → `實際大小` (both AP Preview, TW = HK).
- **rotate** · `旋轉`; left/right → `向左旋轉` / `向右旋轉` · AP Preview (TW = HK) · `high`
- **page** · `頁面`, `第 {n} 頁` · AP Preview (TW = HK) · `high`. ❗ Never `分頁` for a document page: MS's second
  `page` hit renders it that way, and `分頁` is this catalog's word for a TAB.
- **full screen** · `全螢幕` · AP AppKit (TW = HK), MS · `high`
- **encoding** · `編碼` · AP TextEdit (`純文字編碼：`, TW = HK), TC, DC · `high`
- **Western (the encoding group)** · `西歐` · **unattested**: Apple doesn't localize the encoding-family headings in any
  bundle on the system, and MS's `Western` entry is `復古色調`, a photo filter · `tentative`
- **word wrap** · `自動換行` · MS (HKG, TWN), TC's Lister, DC · `high`. Apple has no toggle for it and phrases the wrap
  TARGET instead (`依視窗大小換行`).
- **line number** · `行號` · MS · `high`. ❗ Apple's `行數` means line COUNT, not the number of a line.
- **hexadecimal** · `十六進位` · MS (both `hex` and `hexadecimal`, HKG + TWN), TC's Lister (`16進位`) · `high`.
  **binary** → `二進位`. ❌ Not DC's `十六進制` / `進制`, which is the Mainland form.
- **plain text** · `純文字` · AP Finder + TextEdit (TW = HK), MS, TC · `confirmed`
- **image / photo** · `影像` / `照片` · AP (`JPEG影像`, Preview `影像大小：`); **AP-HK says 相片 for photo** · `high`
- **resolution** · `解析度` · AP Preview + Spotlight; **AP-HK says 解像度** · `high`. ❗ MS's `解析` is the
  "resolving/settlement" sense.
- **dimensions** · `尺寸` (an image's own → `影像大小`) · AP Finder Get Info + Preview (TW = HK) · `high`
- **aspect ratio** · `顯示比例` · AP Photos + Preview (TW = HK) · `high`. MS's `外觀比例` was passed over.
- **metadata** · `後設資料` · AP-TW (Photos) · `tentative`. **A genuine three-way split**: AP-HK says `元數據`, MS says
  `中繼資料` (tagged HKG + TWN). No consensus exists; macOS-first picks Apple's TW form. Worth a reviewer's eye.
- **EXIF** · `EXIF`, kept Latin · MS (HKG, TWN); Apple ships no localized label · `high`
- **exposure / ISO / aperture** · `曝光` / `ISO 感光度` / `光圈值` · AP Preview + Spotlight (TW = HK) · `high`
- **duration (of media)** · `播放時間` · AP QuickTime panel label (TW = HK) · `high`. ❗ Apple has three renderings;
  `持續時間` is the Spotlight attribute and `時間長度` the HK error text. The panel is the lineage match.
- **frame** · `影格` · AP QuickTime (TW = HK) · `high`. Frame rate → `影格率` (AP-HK says `格率`).
- **play / pause / mute / volume** · `播放` / `暫停` / `靜音` / `音量` · AP AppKit + QuickTime (TW = HK) · `high`. ❗
  MS's `mute` is `停用通知`, the silence-a-notification sense.
- **slideshow** · `幻燈片秀` · AP-TW (AP-HK says `幻燈片`) · `high`. ❗ MS's and DC's `投影片放映` is the
  PowerPoint-presentation sense.
- **inspector** · `檢閱器` · AP Preview · `high` (AP-HK writes `檢閲器`, a glyph variant)

### AI and chat, second pass

Confirms the terms agent 3 settled, and adds what the AI rail needed. Sources as above plus the Microsoft TBX.

- **API key** · `API 金鑰` · MS (HKG, TWN) · `high`
- **prompt (the LLM noun)** · `提示詞` · already shipped in `settings.json`; MS attests only the VERB (`提示`) · `high`
- **response** · `回應` · MS · `high`. **streaming** → `串流` (MS, HKG + TWN).
- **generate** · `生成` · AP's own generative-AI copy (`Generated Text` → 生成的文字, `Generated Image` → 生成的影像) ·
  `high`. ❗ MS says `產生` (`regenerate` → 重新產生). Cmdr takes Apple's AI register and uses the `生成` family
  throughout (`停止生成`, `重新生成`) rather than mixing the two.
- **rate limit / quota** · `速率限制` / `配額` · MS (HKG, TWN) · `high`
- **offline** · `離線` · MS (HKG, TWN) · `high`
- **context window** · `上下文長度` · composed from MS `context` → 上下文 · `tentative`. ❗ Three of the TBX's four
  `context` entries give `內容`, the "content" homograph; only one is right.
- **temperature (sampling)** · unattested; MS's `temperature` is `色溫`, colour temperature · `tentative`
- **approve** · `同意` · already shipped in `askCmdr.decision.approved` and `askCmdr.consent.*` · `high`. Apple's own
  register for permitting an action is `允許`, and MS's `核准` is the manager-approval sense; `同意` is kept because the
  chat already reports the act that way and one word beats a better one.

### macOS surfaces named in onboarding

- **Quit & Reopen** (the macOS relaunch button) · `結束並重新打開` · **composed** from Apple zh-TW's own pieces: `Quit`
  → `結束` and `Reopen` → `重新打開` (AppKit `AppKitErrors`), plus SystemSettings'
  `必須先結束「系統設定」，然後將它重新打開` · `tentative`. The button string itself is in no bundle in the pile.
- **Spotlight** · `Spotlight`, kept English · AP-TW = AP-HK (`NSTouchBarControlStripSpotlightTemplate` = `Spotlight` in
  both) · `confirmed`. Verified rather than assumed, since Apple DOES localize it into some other languages.
- **Local network (the macOS permission)** · `本機網路` · agent 1 shipped it in `settings.network.enabled.description`;
  MS agrees on `本機` · `high`
- **Accepting incoming connections (the macOS prompt)** · `接受傳入連線` · **composed** from `連線` plus standard
  Traditional `傳入`; unattested as a whole string · `tentative`

### Two-pane vocabulary (Total Commander + Double Commander, the orthodox lineage)

The concepts Finder has no word for. TC is the richer and cleaner source; **DC's zh-TW file carries Simplified
contamination** (`重復分頁`, `刷新`, `在新分頁中打開`), so weight TC higher and never lift a DC string verbatim.

- **pane** · `窗格` · see the entry above; ❗ TC says `視窗` and DC says `面板`, and **both collide** with terms we
  already use (window / settings panel). This is the one two-pane term where we don't take the orthodox pair's word.
- **left / right pane** · `左窗格` / `右窗格` · composed on `窗格` from TC's `左邊視窗` / `右邊視窗` and DC's `左面板` /
  `右面板` · `high`
- **active / source pane** · `作用中窗格` / `來源窗格` · TC (`來源視窗`, `目前視窗`), DC (`來源面板`) · `high`
- **target pane** · `目標窗格` · TC (`目標視窗`), DC (`目標面板`) · `high`
- **swap panes** · `交換窗格` · TC (`左右視窗交換`), DC (`交換面板`) · `high`
- **file list** · `檔案列表` · follows the `list` ruling below · `high`. ⚠️ **Reversed on evidence**: TC's `檔案清單`
  was adopted as a two-pane term Finder supposedly lacks, but Finder does not lack lists, so the lineage argument
  doesn't apply. See `list (generic UI list)`.
- **command line** · `命令列` · TC (`顯示命令列`, `命令列歷史記錄`) · `high`
- **function-key bar** · `功能鍵列` · TC (`顯示功能鍵列`, `顯示/隱藏 功能鍵列`) · `high`
- **button bar** · `按鈕列` · TC calls its drive-button row `磁碟按鈕列` · `high`. Drop the `磁碟` qualifier unless the
  bar really is the drive buttons.
- **directory hotlist / favorites list** · `常用資料夾清單` · TC, DC · `high`. ❗ Per the mining gotchas this names a
  DIFFERENT feature from Cmdr's bookmarks; use `書籤` for a Cmdr bookmark and reach for this only if Cmdr ever ships a
  hotlist.
- **compare directories** · `比對資料夾` · TC; DC says 比較資料夾 · `high`
- **synchronize directories** · `同步資料夾` · TC, DC · `high`
- **multi-rename** · `多檔重新命名` · TC; DC says 多重命名工具 · `high`
- **attributes** · `屬性` · DC · `high`
- TC menu roots, useful for section names: `檔案操作`, `設定`, `網路`, `剪貼簿`, `瀏覽`, `工具`, `檢視`, `排序`, `標記`,
  `說明`, `使用者`.

### macOS feature and System Settings names

Read straight off the shipped OS by English-key match (`Localizable.loctable` under `System Settings.app` and
`/System/Library/ExtensionKit/Extensions`, recipe in `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this
machine"). All **`confirmed`**: this is what the user's own Mac says (verified on macOS 26.6.2, build 25G83, key-match,
2026-08-29).

- **System Settings** · `系統設定` · TW = HK. Quote it in running text: `「系統設定」`.
- **Full Disk Access** · `完全取用磁碟` · TW; **HK says 完整磁碟取用**. The Taiwan default applies.
- **Accessibility** · `輔助使用` · TW = HK. ❗ Not `協助工具` (that's Microsoft's Windows term).
- **Appearance** · `外觀` · TW = HK
- **Privacy & Security** · `隱私權與安全性` · TW; **HK says 私隱與保安**. The Taiwan default applies.
- **Displays** · `顯示器` · TW; **HK says 螢幕**. The Taiwan default applies.
- **Notifications** · `通知` · TW = HK
- **General** · `一般` · TW = HK
- **Keyboard** · `鍵盤` · TW = HK
- **Sound** · `聲音` · TW = HK
- **Storage** · `儲存空間` · TW = HK
- **Text size** · `文字大小` · TW = HK
- **Login Items** · `登入項目` · TW = HK
- **Language & Region** · `語言與地區` · TW = HK
- **Desktop & Dock** · `桌面與 Dock` · TW = HK (Apple writes it tight, `桌面與Dock`; we space the Latin per `style.md` §
  Spacing)
- **Downloads (the folder)** · `下載項目` · AP-TW (12), AP-HK (11) · `confirmed`
- **Documents (the folder)** · `文件` · AP-TW/HK · `confirmed`

### Crash, reports, and updates

Mined from the live macOS bundles this catalog's readers actually run (macOS 26.6.2, build 25G83, key-match,
2026-08-29): `CrashReporterSupport`, `Problem Reporter`, `Console`, `Software Update`, `App Store`, and Safari's
download UI. Those surfaces are Cmdr's exact analogues, so they outrank the general pile here.

- **crash (noun)** · `當機` · AP-TW (Console `Crash Reports` → 當機報告; the Privacy
  pane's 「…共享當機和使用狀況資料」); **AP-HK says 故障** · `high`. ❗ Microsoft's `損毀` is a false friend: in Apple's
  Traditional localizations 損毀 means _corrupt/damaged_ (「%@」已損毀，無法開啟), so using it for a crash would say
  Cmdr is corrupted.
- **quit unexpectedly** · `意外結束` · AP-HK (`AppKitErrors`: 「…它於重新開啟視窗時意外結束」) · `high`. AP-TW's crash
  reporter writes `未預期的結束` (`CrashReporterSupport/unexpectedly_quit_header`) and AP-HK's writes `突然結束`. All
  three are read in both regions, so this is register rather than the TW/HK vocabulary split the Taiwan-default rule
  governs, and `未預期的` is exactly the bureaucratic register `style.md` § Voice and tone rules out. Chosen for the
  spoken voice, recorded so it isn't "corrected" to Apple-TW's nominalization later.
- **error report** · `錯誤報告` · already shipped in `settings.json`, `updates.json`, and `errorReporter.json` · `high`.
  Apple's nearest term is `問題報告` (Problem Reporter, TW = HK) and it's the more Apple-native pick, but `錯誤報告` is
  the name three shipped files already give the feature and a single name beats a better one. `錯誤` as a terminal-state
  NOUN is Apple's own practice (`Install Error` → 安裝錯誤); the house ban is on `錯誤`/`失敗` as the verb of a sentence
  about what went wrong.
- **report / send a report** · `報告` / `傳送報告` · AP-TW = AP-HK (`CrashReporterSupport`: 「…並傳送報告給Apple」) ·
  `high`. **Don't send** → `不要傳送`; **Ignore** → `忽略`; **Don't ask me again** → `別再詢問` (all Problem Reporter,
  TW = HK).
- **diagnostic information** · `診斷資訊` · AP-TW (Feedback Assistant); **AP-HK says 診斷資料** · `high`. ❗ Microsoft's
  first `diagnostic data` hit is `遙測` (telemetry), which is the wrong sense.
- **stack trace** · `堆疊追蹤` · MS (HKG, TWN) · `high`. ❗ The TBX's first hit is a bare `追蹤`; the right entry is
  three entries later.
- **details** · `詳細資訊` · AP-TW (Problem Reporter `Show Details` → 顯示詳細資訊); **AP-HK says 詳細資料** · `high`
- **anonymously** · `以匿名方式` · AP-TW = AP-HK (Problem Reporter: 「此資訊是以匿名方式收集。」) · `high`
- **personal data** · `個人資料` · AP-TW = AP-HK · `high`
- **timestamp** · `時間戳記` · AP-TW = AP-HK (Notes), MS (HKG, TWN) · `high`
- **log** · `記錄` (log file `記錄檔`) · AP-TW = AP-HK (Console `Log File` → 記錄檔), MS, TC · `high`. ❌ Never `日誌`,
  which is the Mainland form; DC's zh-TW leaks it once as conversion residue.
- **history** · `歷史記錄` · TC (`歷史記錄`, `資料夾歷史記錄`) · `high`. MS's `歷程記錄` is Windows house style.
- **update (an available one, noun)** · `更新項目` · AP-TW (`Check for Updates` → 檢查更新項目;
  `Unable to check for updates` → 無法檢查更新項目; App Store `Updates` → 更新項目) · `high`. AP-HK trends shorter
  (`更新`). The bare verb stays `更新`.
- **up to date** · `已是最新狀態` · AP-TW; **AP-HK says 是最新版本** · `high`
- **restart** · `重新啟動` for relaunching an APP, which is all Cmdr does · `high`. ❗ AP-TW says `重新開機` where the
  MACHINE restarts (Software Update); don't carry that over, and note AP-HK uses `重新啟動` for both.
- **release notes** · `發行備註` · MS (HKG, TWN) · `high`. Apple ships `更多資訊⋯` instead and has no term.
- **operation** · `操作` · AP-TW = AP-HK (`The operation could not be completed.` → 無法完成操作。), DC · `high`. ❗
  AP-TW sometimes writes `作業` and MS renders _operation_ as `作業`; take `操作`, the form both Apple locales use in
  the canonical string and the only one AP-HK ever uses.
- **client** · `用戶端` · MS (HKG, TWN) · `high`
- **manifest** · `資訊清單` · MS (`application manifest` → 應用程式資訊清單) · `high`

**Failure-sentence shapes** (Apple's four, for the `無法` + verb rule in `style.md` § Voice and tone):

1. `無法` + verb + object `。`: `The open file operation failed.` → `無法執行開啟檔案的操作。`
2. `無法` + verb + object `，因為` + reason `。`: the default. `無法下載軟體，因為網路發生問題。`
3. `因為` + reason `，無法` + verb `。`: when the reason is what the user must act on.
4. state phrase `，因此無法` + verb `。`: when a named actor is the blocker. `磁碟正由「%@」使用中，因此無法退出。`

Recovery lines pair as `請確定你已連接網際網路，然後再試一次。` and `當目前的操作完成後，再試一次。` (AP-TW).

### Licensing and purchase

Register for this whole surface is `你`, not `您`: `style.md` § Formality.

- **license (the entitlement)** · `授權` · AP-TW legal (`授權` ×59 in Feedback Assistant's `License.rtf`,
  `cocoartf2761`), and already the shipped form catalog-wide (53 in `licensing.json`, plus `menu.app.license*` and
  `settings.section.license`) · `high`
- **license agreement (the document)** · `軟體授權合約`; Apple titles its own 軟體授權與保密協議 · `tentative`. Cmdr
  ships no agreement text in the catalog, so nothing depends on this yet.
- ❌ **Never `許可證`** for either sense. It's the Simplified-derived form, correct in `zh` (`许可证`) and wrong here.
  Its only Traditional footprint is the decade-old `zh_TW` SLAs localized off a Simplified base (`許可證` ×47 in the
  Install Command Line Developer Tools one), which is exactly the stale source a future miner will find first;
  `docs/i18n/reference-pile/how-to-mine.md` § Legal register says how to date those before quoting them. Zero
  occurrences in this catalog, and it stays that way.
- **license key** · `授權碼` · the short `CMDR-XXXX-XXXX-XXXX` code the user actually pastes · `high`
- ❌ **Never `授權金鑰` for a license key.** `金鑰` is reserved catalog-wide for cryptographic and API keys (`API 金鑰`,
  `加密金鑰`, `金鑰環` = keychain), and `licensing.error.badFormatHint` leans on that contrast to tell the short
  `授權碼` apart from the longer `加密金鑰` a purchase email may carry. All four keys sharing the English "Enter license
  key" (`menu.app.licenseEnter`, `commands.appLicenseKey.enterKey.label`, `licensing.dialog.enterTitle`,
  `licensing.section.enterKey`) render `輸入授權碼`.
- **activate / deactivate (a license)** · `啟用` / `停用` · AP-TW = AP-HK · `high`
- **subscription** · `訂閱` · AP-TW App Store and AppStoreKit (訂閱項目 for a subscription as an item) · `high`
- **perpetual** · `永久` · shipped (`商業永久授權`) · `high`
- **renew** · `續訂` for a subscription; `更新` stays the word for a software update · `high`
- **commercial / personal license** · `商業授權` / `個人授權` · `high`

### Miscellaneous

- **user** · `使用者` · AP-TW (36), TC (51) · `high`. AP-HK avoids the noun entirely (0 for 使用者, 用戶, and 用家),
  phrasing in second person instead; when a sentence reads naturally with `你`, prefer that over the noun.
- **quit (the app)** · `結束` · AP-TW = AP-HK, by key match (`"Quit"` → `結束`) · `confirmed`. ❗ Not `退出`, which is
  taken by _eject_.
- **command** · `指令` · TC + the file managers (78) and MS (66); Apple has none · `high`. Used for both the command
  palette's commands and a terminal command. **`命令列`** stays the fixed compound for _command line_ (TC-attested).
- **list (generic UI list)** · `列表` · AP-TW (53), AP-HK (58), live TW (75), live HK (71) · `high`. ❌ **Never
  `清單`**: it is ZERO in all four Apple corpora, while `列表` covers both the view mode (`列表顯示方式`) and every
  generic list Apple ships, including our exact cases: a server list (`Finder/LocalizableMerged.json:MN3`, "clear the
  list of recent servers" → `清除…的列表`), a user list, and "remove from the list". TW and HK AGREE, so the Apple-zh-TW
  outlier rule doesn't fire and Apple beats Microsoft (which prefers `清單` 318/7). **No context split exists**: one
  word for all of them, `檔案列表` included. `資訊清單` ("Manifest") is an unrelated fixed compound and stays.
- **clipboard** · `剪貼板` · AP-TW (8), AP-HK (8) · `high`. TC says `剪貼簿`, which is the commoner word in Taiwan
  generally, but Apple's two Traditional locales agree and Cmdr is a Mac app.
- **feedback** · `意見反應` · MS (22) · `high`. The standard Taiwanese product term; `回饋` means feedback in the
  control-loop sense. Not in Apple's bundles.
- **Terminal (the app)** · `終端機` · TC + file managers (58), MS (86) · `high`
- **process (a running OS process)** · `程序` · AP-TW (2), AP-HK (2), MS (109) · `high`. No collision with _program_,
  which is `程式`.
- **email address** · `電子郵件地址` · MS (`電子郵件` 129, `地址` 47) · `high`. ❗ `位址` is the network-address sense
  (an IP or URL), not an email one.
- **cache** · `快取` · MS · `high`
- **default** · `預設` · AP-TW = AP-HK (27/27) · `confirmed`. ❗ Collides with "preset"; see `style.md` § Notes.
- **custom** · `自訂` · MS, standard Traditional usage · `high`
- **advanced** · `進階` · MS, standard · `high`
- **enable / disable** · `啟用` / `停用` · MS, standard · `high`
- **notification** · `通知` · AP-TW = AP-HK, MS · `confirmed`
- **software / hardware** · `軟體` / `硬體` · AP-TW; AP-HK says 軟件 · `high`
- **program / app process** · `程式` · AP-TW (176), AP-HK (177) · `confirmed`. AP-CN says 程序.
- **application** · `應用程式` · AP-TW = AP-HK · `confirmed`
- **info** · `資訊` · AP-TW (49); AP-HK says 資料 · `high`
- **message** · `訊息` · AP-TW = AP-HK · `high`
- **video** · `影片` · AP-TW (16), AP-HK (14), AP-CN (15) · `confirmed`. `視頻` = 0 everywhere, even in Simplified.
- **image / photo** · `影像` (image) / `照片` (photo) · standard Traditional usage; Apple uses 圖像 in AppKit
  accessibility strings · `high`. Cmdr's `zh` catalog settled on 图像 over 图片 for "image"; keep the parallel and use
  `影像` consistently rather than mixing in `圖片`.
- **icon** · `圖像` · AP-TW Finder's view menu reads `圖像 / 列表 / 直欄` for Icons / List / Columns, and `圖像顯示方式`
  for Icon view; `圖示` appears 0 times in the macOS zh-TW pile against 50 for `圖像` (verified in the reference pile,
  2026-08-29) · `high`. ❌ Don't reach for `圖示`: it's the Microsoft/Windows term, and mixing it in makes `名稱和圖像`
  and a warning-icon sentence disagree on one Settings pane.
- **screen** · `螢幕` · standard Traditional usage · `high`
- **font** · `字體` / `字型` · TC (`檔案清單字型`) · `high`. Prefer `字型` for a typeface.
- **percent / number formatting** · Arabic numerals, half-width digits, `%` directly after the number · AP · `high`

### Terms the consistency audit settled

Each of these shipped with TWO renderings because separate translation passes each settled it privately. Ruled by mining
the pile PLUS the live macOS bundles (a 12,418-string zh_TW corpus from `/System/Library/ExtensionKit/`,
`/System/Applications`, and `/System/Library/CoreServices`, with its zh_HK twin; macOS 26.6.2, build 25G83, 2026-08-29).
**`AP live` below means that corpus.** Don't re-litigate one without new evidence of that weight.

- **operation** · `操作` · AP-HK 74/0; AP-TW splits 61/14 and all 14 `作業` are minimal pairs where zh-HK writes `操作`
  (`LA17`, `NE83`, `PE87` = `執行此項作業的權限` TW vs `執行此操作的權限` HK); THU 28/0, DC 31/1, TC 22/7 · `high`.
  Textbook § Apple-zh-TW outlier rule; MS's `作業` is the lone dissenter. ❗ `作業` survives ONLY as _job/session/OS_:
  `作業系統`, `列印作業`, `作業階段`, `背景作業`.
- **always** · `總是` · AP live HK 63 `總是` / **0** `永遠` / **0** `一律`; AP live TW mixes 38/27/2 with no boundary
  (`Always Allow` → 永遠允許 but `Always Show` → 總是顯示) · `high`. Because TW has no internal norm, `總是` is the only
  form BOTH audiences see, which is the outlier rule's rationale even though its formal trigger doesn't fire. `一律` is
  weakest everywhere and is retired. ❗ Don't touch `永遠` when the English is _never_
  (`shortcuts.conflict.systemShortcut`: "may never reach Cmdr").
- **Dismiss** · `關閉` · AP live: 13 distinct `Dismiss` keys, all `關閉`, TW = HK (`Dismiss All` → 關閉全部; AppKit
  `Dismiss Popover` → 關閉彈出式項目); MS agrees · `high`. ❗ **`忽略` is Apple's word for _Ignore_ (10/10)**, so a
  toast offering `忽略` promises to ignore what it actually closes. Distinct from `Close`, which is also `關閉`; the
  collision is Apple's and is fine.
- **Go to / Jump to** · `前往` / `跳至` · a real two-word split, and **the boundary is the English verb** · `high`.
  `前往` is Finder's Go menu (`前往檔案夾`, `前往位置`, `前往上層資料夾`, 36/36 in both locales); `跳至` is Jump/Skip to
  (`跳至下一頁`, `跳至結尾處`, AP live 4/0). ❌ Never `跳到` (Apple's older AppKit media-seek form, zero in the live
  corpus) and ❌ never `跳過去` (unattested, and it reads as `跳過` = _skip_, the opposite promise on a jump-to-download
  toast).
- **unknown** · `未知` · AP-TW 12/0, AP-HK 12/0, AP live TW 56/0 (`未知的錯誤`, `未知的顯示器`) · `high`. ❗ `不明` is
  NOT a Simplified form (AP-CN also says `未知`); it's simply the rarer one.
- **forget** · `忘記` · AP live TW 30 / HK 32; `忘掉` is **zero** in every source · `high`. Apple's
  `SHEET_FORGET_NETWORK` "Forget This Network…" → `忘記此網路設定⋯` (HK `忘記此網絡⋯`), Bluetooth "Forget This Device…"
  → `忘記此裝置設定⋯`. Covers the server, the drive index, the saved password, and Ask Cmdr's memory alike.
- **ready** · `準備就緒` (a bare status) / `準備好` (before a verb) · both AP live, TW = HK · `high`. A status chip or
  label reading just "Ready" is `準備就緒` (`索引已準備就緒`); "ready to <verb>" inside a sentence keeps `準備好` +
  verb. ❌ Bare `就緒` never stands alone in Apple, and MS only welds it into proper nouns (`AI 就緒`).
- **on disk** · `磁碟上` · AP-TW/HK `IN_G1` `^1 (^0 on disk)` → `^1（磁碟上^0）`, 9/9 in the pile and 7/7 live · `high`.
  Pairs with `內容` (Content), exactly as English pairs "On disk" with "Content". ❗ `佔用` is **zero** in all four
  Apple corpora, so neither `佔用磁碟` nor `佔用空間` was attested; both are retired.
- **running / in progress** · `執行中` / `進行中` · a legitimate split; **the boundary is process vs task** · `high`.
  `執行中` when the subject is a program, server, or feature (`Finder正在執行時…`, `執行中的應用程式`), like Cmdr's
  local AI server. `進行中` when the subject is an operation or task (`某些操作仍在進行中`, `配對進行中`), like Cmdr's
  background file operations.
- **view mode suffix** · `顯示方式`, never `模式` · AP-TW 49 / AP-HK 41 (live 13/11), and Apple's settings-label strings
  use it too (`總是以列表顯示方式打開`) · `high`. Apple reserves `模式` for BEHAVIOUR modes (`深色模式`, `勿擾模式`,
  `高耗電模式`). So a Settings card heading for a view is `簡潔顯示方式`, not `簡潔模式`.
- **column** · see the `column` entry above; the split is `直欄` (the noun and any header), `欄寬` (the fixed compound
  _column width_, AP-TW live `欄寬控點`; HK writes `直欄闊度`, so Taiwan-default picks `欄寬`), and bare `欄` only as a
  classifier when counting (`{n} 欄`). ❌ Bare `欄` never stands alone as a noun.

**Open, for David** (recorded rather than silently applied):

- **`簡潔` vs `簡要` for the Brief view.** `簡潔` is KDE Dolphin's word for **"Compact"**, and Apple's only `簡潔` is
  Writing Tools' "Make Concise". The orthodox pair renders "Brief" as `簡要` on an exact msgid match
  (`double-commander/doublecmd.po`: "Brief view" → `簡要`, "Show as Brief, Full or Thumbnails" →
  `以簡要、完整或縮圖方式檢視`). The fully-sourced pair would be `簡要顯示方式` / `完整顯示方式`. That's shipped copy
  changing on Tier-3 evidence from a source this guide already flags as contaminated, so it stays as shipped until David
  rules.
- **`總是` vs `永遠允許` on the permission strings.** If the Taiwan-default rule is applied literally rather than the
  both-audiences argument above, "Always allow" specifically has an exact Apple-TW key match at `永遠允許`.

### Terms the catalog uses that had no entry

Sourced in one pass after the consistency audit, to close the gap between "what the catalog says" and "what the glossary
can defend". **Every shipped form below was already correct**; these entries exist so the next pass can't re-decide
them. Evidence is the pile plus **AP live**: 112,961 English→zh_TW→zh_HK rows from every `Localizable*.loctable` under
`/System/Library/ExtensionKit/Extensions/`, `/System/Applications`, and `/System/Library/CoreServices` (macOS 26.6.2,
build 25G83, key-matched, 2026-08-29). Rebuild it in ~2 minutes from `../reference-pile/how-to-mine.md` § "No pile on
this machine".

- **command palette** · `指令面板` · **composed**, unattested as a compound anywhere, but both halves are ruled here:
  `指令` = command (TC + the file managers 78, MS 66) and `面板` = panel (AP live TW 280: `儲存面板`,
  `「取得資訊」面板`; MS `panel` → 面板) · `tentative`. ❌ **Never Microsoft's `命令選擇區`**, even though the TBX has
  that exact entry: it re-imports `命令` against this glossary's `指令` ruling, and `選擇區` is a Windows/VS-Code-ism
  with ZERO occurrences across the 112,961-string live Apple corpus and zero in TC, DC, and Thunar, while the `X面板`
  shape is what DC (52), DOL (46), and THU (7) all use. No internal collision: a pane is `窗格` here, so `面板` is free
  for a real panel.
- **action (a UI action)** · `動作` · AP live TW 24/24 on exact `Action` / `Actions` keys, HK 23/24; AP-TW pile 40,
  AP-HK 36; MS (HKG, TWN) · `high`. ❗ **No collision with `操作`** (operation): Apple keeps them apart on exact-key
  evidence, `Operation` → `操作` 6/6 TW = HK against `Action` → `動作` 24/24.
- **location (a place in the filesystem)** · `位置` · AP-TW = AP-HK (Finder `Localizable.json:Location`,
  `LocalizableMerged.json:FI9`/`SD5` = the "Locations" sidebar header); AP live 41/41 exact; MS; all five file managers
  · `high`. ❗ `地點` is the calendar/physical-place sense (Calendar's `Location-XX01` is TW 地點 / HK 位置); never for
  a path.
- **navigate / navigation** · `導覽` · AP-TW = AP-HK by exact key
  (`Photos.app/IPXTouchBar.loctable:IPXEditToolNavigation` = "Navigate";
  `AccessibilitySettingsExtension.appex:switchControl.header.navigation`); AP live 197/198 · `high`. ❗ **Don't take
  Microsoft here**: its `navigate` is 瀏覽 / 巡覽, where `瀏覽` is _browse_ and `巡覽` is Windows house style.
- **destination (of a copy/move)** · `目標位置`; `目標資料夾` when the English says "destination folder" · AP-TW/HK live
  on our exact sentences (`The destination is read-only.` → `目標位置為唯讀。`), folder form AP-HK 4/4, NAU 3, TC 8 ·
  `high`. ❗ **Not `目的地`**, though MS and NAU/DOL/THU all say it: Apple reserves 目的地 for the maps/AirPlay sense.
  Pairs with `目標窗格` and `覆寫目標`.
- **recent (recently used)** · `最近` as a prefix, `最近使用` as a standalone heading · AP-TW = AP-HK (Finder
  `LocalizableMerged.json:GT6`/`NE91` = "Recents" on the Go menu → 最近使用; `FI1` = "Recent Places" → 最近使用過的位置)
  · `high`. ❗ MS's `recent` → `最新動向` is the news sense; ignore it.
- **result (a search result)** · `結果` · AP-TW = AP-HK (Finder `LocalizableMerged.json:QK28`; `Search Results` →
  `搜尋結果`), 337/339 live; MS (60); TC 9, DOL 12, NAU 8 · `high`
- **scan (verb + noun)** · `掃描` · AP-TW = AP-HK on exact keys (`Preview.app:Scan`,
  `Disk Utility.app:Scan image confirm`), 161/160 live; MS (85) · `high`. Progress form `正在掃描…` is Apple's own
  (`Wireless Diagnostics.app:kWDLocStatusScanning`).
- **toggle (verb, on a command label)** · `切換` · AP-TW = AP-HK in exactly our command shape (`Toggle Sidebar`
  →切換側邊欄, `Toggle Flag` → 切換旗標), 615/606 live; MS; all five file managers · `high`. Unanimous.
- **reset (to defaults)** · `重設` · **AP-HK 28/28** on exact `Reset` keys, MS all six TBX entries, NAU 7/0, DOL 2/0,
  THU 2/0; **AP-TW says `重置` and stands alone** (28/28, and `重設` is ZERO across 112,961 live TW strings) · `high`.
  ❌ **Don't "fix" this to `重置` by citing macOS-first**: it's a textbook § Apple-zh-TW outlier, the second one found
  outside the original five (after `thumbnail`). Minimal pair: `Keychain Access.app/…/Localizable.loctable:Reset` → TW
  `重置` / HK `重設`.
- **turn on / turn off (a feature)** · `開啟` / `關閉` · AP live TW = HK (`Turn On` → 開啟 in 299 of ~314 rows,
  `Turn Off` → 關閉 unanimously); MS agrees · `high`. ❗ Both words are already spent here (`開啟` = _open_, `關閉` =
  _close_ AND _dismiss_); the overload is Apple's own and reads fine in context. ❌ Don't reach for `啟用` / `停用`,
  which this catalog spends on enable/disable and on activating a licence.
- **level (a setting's steps)** · `層級`; the compound is `壓縮層級` · MS TBX has the exact `compression level` →
  `壓縮層級` entry (HKG, TWN), and TC ships it three times in literally this feature (`WCMD.LNG` 5235, 5495, 6754);
  Apple uses 層級 as its generic graded-setting word (75 TW / 47 HK: `安全層級`, `Zoom Level` → 縮放層級) · `high`. ❗
  `等級` is the rating/grade sense, not a setting's steps.
- **password** · `密碼` · AP-TW = AP-HK (Finder `Localizable.json:Password`), pile 48/48, live 1,187/1,139; MS; TC 40 ·
  `high`. ❗ MS's FIRST `password` hit is `存取碼`, the passcode sense; take the second.
- **host (a network machine)** · `主機`; **hostname** · `主機名稱` · AP-TW = AP-HK on exact keys
  (`ADCertificate…:str_ADCertificate_Detail_Server` = "Host" 3/3; `Sharing.appex:NAME_SHEET_HOSTNAME_LABEL` 2/2), 63/63
  live; MS (146); TC 9 · `high`
- **network connection** · `網路連線`; **internet connection** · `網際網路連線` · AP-TW, exact matches for our own
  strings (`Check your network connection and try again.` → `請檢查你的網路連線，然後再試一次。`, 243 live). Internet is
  a real TW/HK split (AP-TW `網際網路` vs AP-HK `互聯網`, 176 live) and Taiwan-default picks `網際網路` · `high`. ❗
  **The rule is the English word, not the vibe**: English "network" → `網路`, English "internet" → `網際網路`. Three
  keys had drifted to `網路` for "internet" and were corrected.
- **version** · `版本` · AP-TW = AP-HK (Finder `LocalizableMerged.json:N226`, `N169.34`), live 23/23 exact; MS (91); TC
  24 · `high`
- **edit (verb and menu title)** · `編輯` · AP-TW = AP-HK (AppKit `Document.json:Edit`, Finder
  `MenuBar.json:163.title`), live 71/71 exact plus the verb shape (`Edit Name` → 編輯名稱); MS (132); TC 35, DC 37 ·
  `high`. One word covers both.

### Finder tag colours

Read off the exact strings Finder's own Tags UI shows (`macOS/Finder/LocalizableMerged.json`, keys
`TG_COLOR_1`–`TG_COLOR_7`). **zh-TW and zh-HK are byte-identical on all seven**, and all seven match what Cmdr ships, so
`commands.tagsToggle*` and `menu.tag.*` name the colours the user's own Finder does · `high`.

- **red** `紅色` (`TG_COLOR_6`) · **orange** `橙色` (`TG_COLOR_7`) · **yellow** `黃色` (`TG_COLOR_5`) · **green** `綠色`
  (`TG_COLOR_2`) · **blue** `藍色` (`TG_COLOR_4`) · **purple** `紫色` (`TG_COLOR_3`) · **gray** `灰色` (`TG_COLOR_1`)
- ❌ **Don't "Taiwanize" orange to `橘色`**: Finder itself says `橙色` in Taiwan (33 TW / 28 HK live, against `橘色`
  2/2). `灰色` has no competitor (49/50). Both are TW = HK on exact-key matches too.
- `TG_COLOR_0` = "No Color" → `沒有顏色` (TW = HK), if a clear-tags string ever needs it.

### Select / Deselect files dialog (`selection.*`, 2026-08-29)

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

### Finishing an interrupted rollback (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

The Operation log gained a state: a rollback cancelled halfway leaves the row "partly rolled back", and that row's
button changes from "Roll back" to "Finish rolling back". All five values anchor on the rollback vocabulary this catalog
already ships; nothing was coined.

- **"Finish rolling back"** · `完成復原` · built on the settled `復原` (`operationLog.dialog.rollBack` `復原`,
  `rollingBack` `正在復原`, `partiallyRolledBack` `已部分復原`; the `還原` = undo / `復原` = roll back split under §
  Progress, rollback, and destructive actions still holds) · `medium-high`. `完成` says "carry this one to the end", not
  "start a fresh one", and it reads unambiguously beside the `已部分復原` badge on the same row. `繼續復原` ("continue")
  would be blunter about not restarting, but English says Finish rather than Continue, and `繼續` is already spent on
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

### What a cancelled rollback reports (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, 2026-08-31)

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
  ROLLBACK, so it takes `復原`, not the `還原` this catalog spends on Undo (see § Progress, rollback, and destructive
  actions). `無法` + verb is the house failure shape. **"Its drive may be disconnected or read-only"** →
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
  opposite of what happened. `目標位置` is the glossary's destination term and already user-facing
  (`transferProgress.stallWaitingDestination`).
- **`fileOperations.rollbackConfirm.body` gained the family's third sentence** ·
  `這會刪除這項操作到目前為止寫入的每一個檔案。被它取代掉的檔案回不來了。Cmdr 會略過沒有把握的部分，所以可能會留下一些。`
  · `high`. The added sentence is `bodyUndoByDeleting`'s verbatim, which is the point: all four `rollbackConfirm.body*`
  keys make one promise in one wording, and `leftBehind` echoes it when the promise pays out.
- All 18 values differ from English, so none needs a `sameAsSourceJustification`. Chinese needs no apostrophe, so the
  doubled `''` in the English sources has no counterpart here.

### Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `askCmdr.consent.item.contents`, `askCmdr.consent.contentsRule`, `askCmdr.consent.whatsNew.body`, 2026-09-02)

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
- **thumbnail** · `縮圖` · reused from § Viewer, media, and image metadata; MS `thumbnail` → 縮圖 (2), THU + DC
  `Thumbnails` → 縮圖 · `high`
- **archive (in the consent copy)** · `封存檔` · the glossary's general-archive noun (AP-TW = AP-HK `Archive` → 封存,
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
  strong-negation form for a privacy promise (`askCmdr.consent.logsNote` `絕不會送到任何地方`, the telemetry settings'
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

## Notes

- **AP-TW's ellipsis glyph is `⋯` (U+22EF)** in strings quoted throughout this file (`連接伺服器⋯`). That's Apple's
  house glyph; **Cmdr writes `…` (U+2026)** everywhere. Quoted evidence above preserves Apple's glyph; your catalog
  values must not.
- **Apple runs CJK and Latin together with no space**; Cmdr spaces them. Same shape of exception, recorded with its
  counts in `style.md` § Spacing: put a space between Chinese and Latin.
- **Terms genuinely absent from every source**: breadcrumb, mount point, USB device, and "pane" in the two-pane sense
  (Apple only). Each is marked `tentative` or composed above, with what it was built from.

## Shared `en` fixes: menu wording, System Settings tokens, name-restore verb (2026-08-30)

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

### `cancelRollback.stagedLeftover.*`（Cmdr 自己留在目標位置的殘留）

2026-09-02 新增。兩條文案，說的是 Cmdr 自己建立的工作檔案沒能從目標位置清掉。它們**不屬於** `reason.*` 清單：那邊是
Cmdr 在保護使用者的檔案，這邊是 Cmdr 自己的殘留。

- **`unfinished copy` → `不完整副本`** · `不完整` 是 Apple 對 "incomplete" 的譯法（macOS `LA33`：「已損毀或不完
  整」），`副本` 是 `NE111` 裡的名詞（「保留可恢復的副本」）· `high`
- **`at the destination` → `目標位置`** · 目錄裡已在用的詞（`conflictsUnknown`）· `high`
- **`transfer`（名詞）→ `傳輸`** · 目錄已這樣說（`errors.listing.deviceReconnecting.explanation`：「傳輸被取消或中斷
  之後」）· `high`
- 第二句用 `清掉` 而不是 `刪除`：這是 Cmdr 自己的工作檔案，不是使用者的檔案。
- ⚠️ **寫 `之後往那裡傳輸時`，❌ 絕不寫「下次」。** Cmdr 的清理會跳過不滿一小時的檔案，所以立刻重試並不會清掉它。給一
  個兌現不了的承諾，正是這條文案要消除的毛病。

## WebKit 過舊時的攔截頁（`main.oldWebkit.*`，2026-09-02）

三條文案，在 Mac 的 Safari 過舊時代替 Cmdr 的介面顯示。它們寫在 HTML 外殼裡而不是 app 裡，所以這是那位使用者能看到的 Cmdr 的全部內容。

- **`Software Update` → `軟體更新`** · macOS 系統設定中該面板的名稱；Finder 的 Tier
  1 證據佐證了這個詞（`Apple Device Software Update File` → `Apple裝置軟體更新檔案`）· `high`。
- **`Quit` → `結束`** · macOS AppKit 的 `Quit` 鍵 → `結束` ·
  `high`。簡體用「退出」，繁體用「結束」，兩份目錄不互相轉換。
- **`Safari`、`Mac`、`15.4` 保持原樣**，兩側依 § Spacing 加空格。`Safari` 已加入 `BRAND_WORDS`。
- 面板名用直角引號 `「軟體更新」`，與目錄裡其餘繁體文案一致。

## 舊版 macOS 提示（`main.oldMacos.*`，2026-09-02）

低於 macOS
12 的 Mac 上只出現一次的對話框：Cmdr 跑得動，但超出了測試範圍。語氣坦率輕鬆，既不是道歉也不是警告，因為應用程式確實在跑。

- **`supported` → `支援`** · macOS Finder zh-TW（`無法完成此項操作，因為不支援此項操作。`）· `high`。
- **`X and up` → `X 以上的版本`** · macOS SystemSettings zh-TW（`需要OS X %@或以上版本。`）·
  `high`。Apple 寫得緊湊，我們在拉丁字元兩側加空格 (`style.md` § Spacing)。
- **`best effort` → `盡力而為`** · pile 裡沒有對應詞條（只有網路 QoS 的定義），但這是現成的中文說法 · `high`。
- **`layout` → `版面配置`** · Microsoft zh-Hant 的標準譯法 · `high`。
- **`look off` → `不太對`** · 口語，且避開語氣規則禁止的「錯誤」「失敗」。
- **最後一句是 David 的第一人稱**，仍用 `你`，與 `onboarding.stepBeta.greeting` 一致。
