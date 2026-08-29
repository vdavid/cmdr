# zh-Hant glossary

The living term glossary for translating Cmdr into Traditional Chinese: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/zh-TW/` and `zh-HK/` (macOS + the five
  file managers) and `_ignored/i18n/zh-Hant/` (the Microsoft terminology TBX and style guide) for how each is rendered,
  and for similar sentences to model phrasing on. Recipes: `docs/i18n/reference-pile/how-to-mine.md`.
- **Read `style.md` first**, especially § The Apple-zh-TW outlier rule: five of the highest-traffic terms below take the
  pan-Traditional consensus form over Apple's zh-TW one, deliberately.
- **This folder is the language home.** Capture new term decisions here, other findings as sibling files.

Source abbreviations: **AP-TW** / **AP-HK** / **AP-CN** = macOS Finder + AppKit + SystemSettings in that locale;
**MS** = Microsoft zh-Hant terminology TBX; **NAU** / **DOL** / **THU** = GNOME Nautilus / KDE Dolphin / Xfce Thunar
(zh-TW); **TC** / **DC** = Total Commander / Double Commander (zh-TW). All evidence verified 2026-08-29.

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

- **file** · `檔案` · AP-TW (414), AP-HK (183), MS · `high`. ❗ Simplified 文件 = file, but Traditional 文件 = *document*.
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
  for *copy*, and Cmdr ships both commands (`commands.fileDuplicate.*`). DOL's `在此建立複本` and NAU's `再製` are the
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
  (its `設定面板` is a *settings* pane). TC calls a pane `視窗` (collides with window) and DC calls it `面板` (collides
  with a settings panel), so `窗格` is the only unambiguous choice. See § Two-pane vocabulary.
- **window** · `視窗` · AP-TW = AP-HK, MS, universal · `confirmed`
- **sidebar** · `側邊欄` · AP-TW = AP-HK, NAU, MS · `confirmed`
- **toolbar** · `工具列` · AP-TW = AP-HK, MS, DOL, THU, DC · `confirmed`
- **status bar** · `狀態列` · AP-TW = AP-HK, MS, DOL, THU · `confirmed`
- **column** · `直欄` · AP-TW = AP-HK (`Columns` → 直欄, `Column View` → 直欄顯示方式) · `high`. MS says `資料行`;
  Apple wins.
- **sort** · `排序` · AP-TW, MS, all five file managers; AP-HK says 排列 · `high`
- **view (menu noun / view mode)** · `顯示方式` · AP-TW = AP-HK (`View` → 顯示方式) · `high`
- **view (verb, "to look at")** · `檢視` · AP (`to View %@` → 檢視), MS, TC (`View` menu → 檢視) · `high`. ❗ Don't swap
  the two: see `style.md` § Notes.
- **preview** · `預覽` · AP-TW = AP-HK, MS, DOL, THU · `confirmed`
- **Quick Look** · `快速查看` · AP-TW = AP-HK = AP-CN · `confirmed`. Translated, not kept English (which is why it's
  not on the do-not-translate list).
- **menu** · `選單` · AP-HK, TC, DOL · `high`. MS's `功能表` is Windows house style; Cmdr is a Mac app.
- **dialog** · `對話方塊` · MS · `high`. Not in Apple's bundles.
- **settings** · `設定` · AP-TW (160), AP-HK (160), MS · `confirmed`. `設置` = 0 in both Apple corpora.
- **preferences** · `偏好設定` · AP-TW = AP-HK · `confirmed`
- **appearance** · `外觀` · AP-TW = AP-HK (SystemSettings) · `confirmed`
- **theme** · `佈景主題` · MS · `high`. Not in Apple's bundles.
- **dark / light mode** · `深色模式` / `淺色模式` · MS · `high`. The appearance-mode strings aren't in the mined Apple
  bundles (深色/淺色 = 0 there), so MS is the only evidence.
- **language** · `語言` · AP-TW = AP-HK, MS · `confirmed`
- **keyboard shortcut** · `鍵盤快速鍵` (short: `快速鍵`) · MS · `high`. Not in Apple's bundles, and Apple's `捷徑` is
  taken by the Shortcuts app.
- **search** · `搜尋` · AP-TW (82), AP-HK (82), MS · `confirmed`. `搜索` is the Simplified form (AP-CN 69).
- **filter** · `篩選` · MS, DC (`Quick Filter` → 快速篩選) · `high`. NAU/DOL say `過濾`; not in Apple's bundles.
- **index / indexing** · `索引` (noun) / `建立索引` (verb) · AP (`Updating tag index` → 更新標籤索引,
  `Indexed` → 已製作索引), MS · `high`
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
- **pause / resume** · `暫停` / `繼續` · AP-TW = AP-HK (`Resume` → 繼續), MS (pause) · `high`. ❗ MS renders *resume* as
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
- **in progress (the pattern)** · `正在` + verb · AP-HK is consistently `正在…`; AP-TW mixes a `…中` suffix
  (`載入中⋯`) · `high`. Cmdr uses `正在…` everywhere, matching its `zh` catalog.
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
- **connect** · `連線` (noun/label) / `連接` (verb in a sentence) · AP-TW = AP-HK (`Connect` → 連線;
  `連接伺服器⋯`), MS · `high`
- **disconnect** · `中斷連線` · AP-TW = AP-HK, MS · `confirmed`
- **remote** · `遠端` · AP-TW, MS, THU; AP-HK says 遙距 · `high`
- **local** · `本機` · AP-TW = AP-HK, MS, NAU · `confirmed`
- **cloud** · `雲端` · AP-TW = AP-HK (`Cloud Storage` → 雲端儲存空間), MS · `confirmed`
- **memory (RAM)** · `記憶體` · AP-TW = AP-HK (Info window `記憶體：`), MS · `confirmed`. AP-CN says 内存.
- **storage** · `儲存空間` · AP-TW = AP-HK (`Manage Storage…` → 管理儲存空間⋯), MS · `confirmed`
- **free space** · `可用空間` · AP-TW (`因為可用空間不足`), MS; AP-HK says 未使用空間 · `high`
- **size** · `大小` · AP-TW = AP-HK = AP-CN, MS · `confirmed`
- **modified date** · `修改日期` · AP-TW = AP-HK = AP-CN · `confirmed`
- **created date** · `製作日期` · AP-TW = AP-HK · `high`. AP-CN says 创建日期.
- **USB device** · `USB 裝置` · **composed**: `USB` is unattested in the Traditional macOS bundles and in MS, and
  `裝置` = device is standard · `tentative`
- **eject / device removal** · see `退出` above.

### Two-pane vocabulary (Total Commander + Double Commander, the orthodox lineage)

The concepts Finder has no word for. TC is the richer and cleaner source; **DC's zh-TW file carries Simplified
contamination** (`重復分頁`, `刷新`, `在新分頁中打開`), so weight TC higher and never lift a DC string verbatim.

- **pane** · `窗格` · see the entry above; ❗ TC says `視窗` and DC says `面板`, and **both collide** with terms we
  already use (window / settings panel). This is the one two-pane term where we don't take the orthodox pair's word.
- **left / right pane** · `左窗格` / `右窗格` · composed on `窗格` from TC's `左邊視窗` / `右邊視窗` and DC's
  `左面板` / `右面板` · `high`
- **active / source pane** · `作用中窗格` / `來源窗格` · TC (`來源視窗`, `目前視窗`), DC (`來源面板`) · `high`
- **target pane** · `目標窗格` · TC (`目標視窗`), DC (`目標面板`) · `high`
- **swap panes** · `交換窗格` · TC (`左右視窗交換`), DC (`交換面板`) · `high`
- **file list** · `檔案清單` · TC (`列印檔案清單:`, `檔案清單字型`), DC (`更新檔案清單`), MS · `high`
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
- TC menu roots, useful for section names: `檔案操作`, `設定`, `網路`, `剪貼簿`, `瀏覽`, `工具`, `檢視`, `排序`,
  `標記`, `說明`, `使用者`.

### macOS feature and System Settings names

Read straight off the shipped OS by English-key match (`Localizable.loctable` under `System Settings.app` and
`/System/Library/ExtensionKit/Extensions`, recipe in `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this
machine"). All **`confirmed`** — this is what the user's own Mac says (verified on macOS 26.6.2, build 25G83,
key-match, 2026-08-29).

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
- **Desktop & Dock** · `桌面與 Dock` · TW = HK (Apple writes it tight, `桌面與Dock`; we space the Latin per
  `style.md` § Spacing)
- **Downloads (the folder)** · `下載項目` · AP-TW (12), AP-HK (11) · `confirmed`
- **Documents (the folder)** · `文件` · AP-TW/HK · `confirmed`

### Miscellaneous

- **user** · `使用者` · AP-TW (36), TC (51) · `high`. AP-HK avoids the noun entirely (0 for 使用者, 用戶, and 用家),
  phrasing in second person instead; when a sentence reads naturally with `你`, prefer that over the noun.
- **quit (the app)** · `結束` · AP-TW = AP-HK, by key match (`"Quit"` → `結束`) · `confirmed`. ❗ Not `退出`, which is
  taken by *eject*.
- **command** · `指令` · TC + the file managers (78) and MS (66); Apple has none · `high`. Used for both the command
  palette's commands and a terminal command. **`命令列`** stays the fixed compound for *command line* (TC-attested).
- **list (generic UI list)** · `列表` · AP-TW (53), AP-HK (58) · `high`. MS and TC prefer `清單`, and `檔案清單`
  stays the orthodox two-pane term for *file list*; Apple wins for a plain "list".
- **clipboard** · `剪貼板` · AP-TW (8), AP-HK (8) · `high`. TC says `剪貼簿`, which is the commoner word in Taiwan
  generally, but Apple's two Traditional locales agree and Cmdr is a Mac app.
- **feedback** · `意見反應` · MS (22) · `high`. The standard Taiwanese product term; `回饋` means feedback in the
  control-loop sense. Not in Apple's bundles.
- **Terminal (the app)** · `終端機` · TC + file managers (58), MS (86) · `high`
- **process (a running OS process)** · `程序` · AP-TW (2), AP-HK (2), MS (109) · `high`. No collision with *program*,
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

## Notes

- **AP-TW's ellipsis glyph is `⋯` (U+22EF)** in strings quoted throughout this file (`連接伺服器⋯`). That's Apple's
  house glyph; **Cmdr writes `…` (U+2026)** everywhere. Quoted evidence above preserves Apple's glyph; your catalog
  values must not.
- **Apple runs CJK and Latin together with no space**; Cmdr spaces them. Same shape of exception, recorded with its
  counts in `style.md` § Spacing: put a space between Chinese and Latin.
- **Terms genuinely absent from every source**: breadcrumb, mount point, USB device, and "pane" in the two-pane sense
  (Apple only). Each is marked `tentative` or composed above, with what it was built from.
