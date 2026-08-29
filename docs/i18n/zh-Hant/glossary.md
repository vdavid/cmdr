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
- **select (verb)** · `選取` · AP, MS · `confirmed`
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
  `履歷表` (the CV sense) — do not use MS here.
- **overwrite** · `覆寫` · AP-TW = AP-HK (`Overwrite at Destination` → 覆寫目標), MS · `high`. TC/DC say `覆蓋`.
- **replace** · `取代` · AP-TW = AP-HK, NAU, THU, MS · `confirmed`
- **conflict** · `衝突` · AP-TW = AP-HK, MS · `confirmed`
- **remaining / time remaining** · `剩餘` / `剩餘時間` · AP-TW = AP-HK (`Estimating time remaining…` →
  `正在估計剩餘時間⋯`) · `high`
- **speed** · `速度` · AP-TW = AP-HK · `high`
- **transfer** · `傳輸` · TC · `high`. Apple uses `傳送` for sending items. ❗ MS's `移轉` is the business sense — don't
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
machine"). All **`confirmed`** — this is what the user's own Mac says (verified on macOS 26.6.2, build 25G83, key-match,
2026-08-29).

- **System Settings** · `系統設定` · TW = HK. Quote it in running text: `「系統設定」`.
- **Full Disk Access** · `完全取用磁碟` · TW; **HK says 完整磁碟取用** — Taiwan default applies.
- **Accessibility** · `輔助使用` · TW = HK. ❗ Not `協助工具` (that's Microsoft's Windows term).
- **Appearance** · `外觀` · TW = HK
- **Privacy & Security** · `隱私權與安全性` · TW; **HK says 私隱與保安** — Taiwan default applies.
- **Displays** · `顯示器` · TW; **HK says 螢幕** — Taiwan default applies.
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
  first `diagnostic data` hit is `遙測` (telemetry) — the wrong sense.
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

1. `無法` + verb + object `。` — `The open file operation failed.` → `無法執行開啟檔案的操作。`
2. `無法` + verb + object `，因為` + reason `。` — the default. `無法下載軟體，因為網路發生問題。`
3. `因為` + reason `，無法` + verb `。` — when the reason is what the user must act on.
4. state phrase `，因此無法` + verb `。` — when a named actor is the blocker. `磁碟正由「%@」使用中，因此無法退出。`

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
  generic list Apple ships, including our exact cases — a server list (`Finder/LocalizableMerged.json:MN3`, "clear the
  list of recent servers" → `清除…的列表`), a user list, and "remove from the list". TW and HK AGREE, so the Apple-zh-TW
  outlier rule doesn't fire and Apple beats Microsoft (which prefers `清單` 318/7). **No context split exists** — one
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
  `執行中` when the subject is a program, server, or feature (`Finder正在執行時…`, `執行中的應用程式`) — Cmdr's local AI
  server. `進行中` when the subject is an operation or task (`某些操作仍在進行中`, `配對進行中`) — Cmdr's background
  file operations.
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

## Notes

- **AP-TW's ellipsis glyph is `⋯` (U+22EF)** in strings quoted throughout this file (`連接伺服器⋯`). That's Apple's
  house glyph; **Cmdr writes `…` (U+2026)** everywhere. Quoted evidence above preserves Apple's glyph; your catalog
  values must not.
- **Apple runs CJK and Latin together with no space**; Cmdr spaces them. Same shape of exception, recorded with its
  counts in `style.md` § Spacing: put a space between Chinese and Latin.
- **Terms genuinely absent from every source**: breadcrumb, mount point, USB device, and "pane" in the two-pane sense
  (Apple only). Each is marked `tentative` or composed above, with what it was built from.
