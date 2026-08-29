# Traditional Chinese (zh-Hant) translation style guide

Working notes for translating Cmdr into Traditional Chinese. Read `../README.md` for how this fits the translation
process, and the app-wide `docs/style-guide.md` for the English voice these notes carry into Chinese.

`zh-Hant` is a **full translation of `en`**, not an overlay of `zh`. The Simplified `zh` catalog is unreadable to this
locale's readers, so the script guard blocks every inheritance path: a missing key falls through to English, never to
`zh`. Every one of the 3,138 keys has to be written here. Rule and its three enforcing layers:
`apps/desktop/src/lib/intl/locale-inheritance.ts`.

One catalog serves **Taiwan, Hong Kong, and Macau**. That single fact drives most of the rulings below, and it's why the
tag is `zh-Hant` (the script) rather than `zh-TW` (one region).

Sources mined for this guide: macOS Finder / AppKit / SystemSettings in `zh-TW` and `zh-HK`, the Microsoft zh-Hant
terminology TBX and style guide, GNOME Nautilus / KDE Dolphin / Xfce Thunar in `zh-TW`, and the orthodox two-pane pair
Total Commander + Double Commander in `zh-TW`. Evidence verified against the reference pile on 2026-08-29; occurrence
counts throughout are from that pass.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## ❗ This is NOT a character conversion of the `zh` catalog

**Never run the Simplified catalog through a character converter.** Transliteration produces text that is technically
readable and unmistakably mainland, because the terminology differs far beyond character shape. A conversion would
"pass" every check we have and still read as a foreign product to every reader it targets.

The delta is large and systematic (Simplified → Traditional, all verified in the pile):

- 文件 → **檔案** (file). In Traditional, 文件 means _document_, so a converted catalog says "document" everywhere it
  means "file". This one alone poisons the whole catalog.
- 文件夹 → **資料夾**, 保存 → **儲存**, 搜索 → **搜尋**, 设置 → **設定**, 默认 → **預設**, 视频 → **影片**, 程序 →
  **程式**, 信息 → **資訊**, 用户 → **使用者**, 网络 → **網路**, 内存 → **記憶體**, 服务器 → **伺服器**, 缓存 →
  **快取**, 扩展名 → **副檔名**, 窗口 → **視窗**, 菜单 → **選單**, 剪切 → **剪下**, 粘贴 → **貼上**, 刷新 →
  **重新整理**, 加载 → **載入**, 屏幕 → **螢幕**, 打印 → **列印**, 软件 → **軟體**, 硬件 → **硬體**, 兼容 →
  **相容**, 性能 → **效能**, 优化 → **最佳化**, 质量 → **品質**, 数据 → **資料**, 登录 → **登入**, 界面 →
  **介面**, 快捷方式 → **捷徑**, 收藏 → **喜好項目**, 芯片 → **晶片**, 密钥 → **金鑰**, 设备 → **裝置**, 废纸篓 →
  **垃圾桶**, 创建 → **製作**/**新增**, 自定义 → **自訂**.
- Punctuation differs too: Simplified quotes filenames with `“…”`, Traditional with `「…」` (see § Punctuation).

Translate each key from the **English** source. The `zh` catalog is useful as a structural precedent (it already solved
the ICU shapes, the placeholder spacing, and the sentence order for the same key), and `translation-learnings.md` is
right that a sibling catalog outranks the pile on _which_ rendering this app uses. But every noun and verb gets
re-decided against this guide's glossary.

## Decisions to confirm with David

The rest of this guide assumes these. Both are recorded rulings, not open questions, but they're the two a reviewer
would want to see stated.

- **One catalog for TW + HK + MO, written to a pan-Traditional consensus rather than pure Apple-zh-TW.** See § The
  Apple-zh-TW outlier rule below; it is the single most consequential call in this guide.
- **Formality: `你` everywhere, including licensing** (Apple zh-TW 398 `你` / 0 `您`; zh-HK 413 / 0; Apple's own
  purchase UI 110 / 0). `zh` keeps a blanket `您` carve-out over its `licensing.*` keys; on the evidence in § Formality
  that carve-out looks wrong for `zh` too, but correcting a shipped catalog outside this locale is David's call.

## The Apple-zh-TW outlier rule (read this before picking any term)

Cmdr's house rule is macOS-first: prefer the term the user's own Finder shows (`docs/guides/i18n-translation.md` §
Term-choice principles). For Traditional Chinese that rule needs one documented refinement, because **Apple's zh-TW is a
lone outlier on a handful of the highest-traffic words in a file manager**, disagreeing with Apple's own zh-HK, with
Microsoft, and with all five file-manager corpora at once.

The measured picture (macOS bundle counts, `zh-TW` vs `zh-HK`; the file managers are all `zh-TW`):

| concept | Apple zh-TW  | Apple zh-HK  | MS zh-Hant | Nautilus / Dolphin / Thunar / TC / DC | Cmdr picks |
| ------- | ------------ | ------------ | ---------- | ------------------------------------- | ---------- |
| folder  | 檔案夾 (233) | 資料夾 (228) | 資料夾     | 資料夾 (193 / 174 / 93 / 184 / 108)   | **資料夾** |
| copy    | 拷貝         | 複製         | 複製       | 複製 (all five)                       | **複製**   |
| move    | 搬移         | 移動         | 移動       | 移動                                  | **移動**   |
| open    | 打開         | 開啟         | 開啟       | 開啟 (all five)                       | **開啟**   |
| tab     | 標籤頁       | 分頁         | 分頁       | 分頁 (all)                            | **分頁**   |

Every one of those splits is 100% clean (233 vs 0 and 0 vs 228 for folder, and the same shape for the rest). **The rule:
when Apple-zh-TW stands alone against Apple-zh-HK + Microsoft + the file managers, take the consensus form.**

Why the consensus wins here, even though macOS-first normally decides:

1. **One catalog serves both norms.** The Apple-zh-TW form is a word a Hong Kong reader's own Finder never shows. The
   consensus form is familiar to _both_ audiences: it's what Apple itself shows in Hong Kong, and what every non-Apple
   Taiwanese product shows. It's the only choice that doesn't make half the audience feel written-around.
2. **The macOS-first rule exists to match user expectation**, and on these five words Apple-zh-TW is the least
   representative source of Taiwanese expectation, not the most. Its own community's file managers all disagree with it.
3. **拷貝 vs 複製 and 打開 vs 開啟 aren't dialect, they're Apple house style**, the same way Apple US English says
   "Trash" where the rest of computing says "Recycle Bin".

❌ **Don't "fix" these back to the Apple-zh-TW forms by citing the macOS-first rule.** That's the exact shape of the
future cleanup pass this section exists to stop. Everywhere Apple zh-TW and zh-HK _agree_, Apple still wins over
Microsoft (卷宗 over 磁碟區, 還原 over 復原, 直欄 over 資料行, 喜好項目 over 我的最愛, 略過 over 跳過).

## Voice and tone

Cmdr's Traditional Chinese voice is friendly, concise, active, and never alarmist, matching the English. Short, spoken,
modern Mandarin as written in Taiwan; not bureaucratic, not literary, not translationese.

- **Demonstratives: prefer the spoken `這個` / `這項` / `這次` over the written `此` / `該`.** Same call as the `zh`
  catalog; `此` reads as legal/technical register and clashes with the friendly voice.
- **Error and warning messages stay calm and actionable.** Keep the English rule of avoiding the words "error" and
  "failed". Traditional Chinese has an idiomatic way to do this, and it happens to be **Apple's own systematic
  pattern**: rewrite the failure as `無法` + verb, optionally with a `因為…` clause. Apple zh-TW does this consistently
  (`Failed to open color palette.` → `無法打開色盤。`; `The operation can't be completed…` → `無法完成此項操作，因為…`),
  and never writes `失敗` inside a sentence even though it exists as a bare status label. Use `無法…`, not `失敗` or
  `錯誤`.
- **Chinese runs SHORT.** A Traditional string is often half the character count of the English, so overflow is rarely
  the risk (too-sparse buttons can be). Still overflow-check, but the bigger care is that terse Chinese reads naturally
  rather than cryptically clipped.
- **No letter case**: Han characters are unicameral, so the sentence-case rule is moot. Just keep Latin brand words
  (Cmdr, macOS) as they are.

## Formality

- **Verdict: address the user as `你` (neutral), never the polite `您`.** Apple's Traditional localizations use `你`
  exclusively: zh-TW macOS 398 `你` / 0 `您`, zh-HK macOS 413 / 0, across Finder, AppKit, and SystemSettings. `zh` rules
  the same way for the app at large, so both Chinese catalogs address the reader identically everywhere except
  `licensing.*`, where `zh` still carries the older blanket carve-out (24 `您`) that the bullets below retire here.
- **The community file managers disagree, and lose.** Total Commander (78 `您` / 0 `你`), Nautilus (49 / 0), Dolphin (96
  / 0), and Thunar (47 / 0) all use the polite `您`; only Double Commander sides with Apple (1 / 25). They're Tier 3 and
  reflect an older localization register; Apple is Tier 1 and matches Cmdr's friendly consumer voice. Recorded so nobody
  re-opens it after grepping Total Commander.
- **`您` belongs to AGREEMENT PROSE only, and this catalog ships none.** The boundary sits between the contract document
  and the UI wrapped around it, never around money as a topic. Apple draws it cleanly: its live Traditional SLA is `您`
  throughout (`您` ×107, `你` 0, archaic `閣下` 0 in Feedback Assistant's `License.rtf`), while its own purchase,
  subscription, and billing UI is `你` with zero `您` (AppStoreKit 98, App Store.app 12, over 購買 / 訂閱 / 帳號 copy).
  Every `licensing.*` key Cmdr ships is the second kind: "Get a license", "Enter license key", "Your commercial license
  has expired", "Paste your license key from the email you received after purchase". They take `你`, like the rest of
  the catalog. Cmdr's actual agreement lives on the website; the in-app consent line (`onboarding.stepBeta.terms.*`) is
  the checkbox AROUND that document and is `你` too. (All four counts measured on the live bundles rather than the pile:
  macOS 26.6.2, build 25G83, 2026-08-29.)
- ❌ **Don't reinstate a "legal and billing" carve-out.** Register follows the SURFACE, not the topic. That blunter rule
  is what made `licensing.json` ship as a 22-instance `您` island in an otherwise all-`你` catalog, so a reader walking
  from Settings into the licensing dialog changed register mid-app.
- **One `您` stands, on a different axis: `licensing.dialog.mailtoBody` opens `您好，`.** That value is a pre-filled
  email the USER sends to support, so the addressee is us rather than the reader, and `您好` is the ordinary
  Traditional-Chinese business-letter salutation. Any future string written in the user's voice to an outside recipient
  follows it. If Cmdr ever ships real contract prose in-app, that text (and only that text) takes `您`.
- ⚠️ **The evidence is vintage-sensitive, so re-date it before re-opening this.** macOS also carries Traditional SLAs
  localized off a Simplified base a decade ago; they say `閣下` ×111 and `許可證` ×47, and they hand a miner the
  opposite ruling with full confidence. An RTF header dates itself (`\cocoartf<N>`, and `\fcharset134` = Simplified
  codepage vs `136` = Big5): `docs/i18n/reference-pile/how-to-mine.md` § Legal register.
- **Buttons and menu items: bare verb, no politener.** `複製`, `移動`, `開啟`, `刪除`, `取消`. A bare verb isn't rude in
  Chinese; it's the correct register for a macOS action label.

## Decision points

### Which Traditional norm wins where TW and HK genuinely diverge

Beyond the Apple-outlier set above, TW and HK diverge on real words. **Taiwan's norm wins by default**: it's the larger
audience, Apple's zh-TW is the better-stocked source, and Microsoft's zh-Hant TBX (flagged `HKG, TWN`) mostly follows
it. Recorded rulings, with the divergence measured in the two macOS corpora:

| concept         | Taiwan    | Hong Kong  | Cmdr picks    | note                                                                                        |
| --------------- | --------- | ---------- | ------------- | ------------------------------------------------------------------------------------------- |
| network         | 網路 (33) | 網絡 (15)  | **網路**      | 100% clean split; MS agrees with TW                                                         |
| software        | 軟體 (7)  | 軟件 (7)   | **軟體**      | 100% clean split                                                                            |
| info (Get Info) | 資訊 (49) | 資料       | **資訊**      | HK's 資料 also carries the "data" sense, so it's ambiguous                                  |
| remote          | 遠端      | 遙距       | **遠端**      | MS agrees with TW                                                                           |
| free space      | 可用空間  | 未使用空間 | **可用空間**  | MS agrees with TW                                                                           |
| retry           | 再試一次  | 再試       | **再試一次**  | reads more natural; MS's 重試 is Windows house style                                        |
| sort            | 排序      | 排列       | **排序**      | here HK is the outlier; MS and all five file managers say 排序                              |
| duplicate       | 複製      | 製作副本   | **製作副本**  | HK's form, chosen deliberately; see the note under the table                                |
| loading         | 載入中⋯   | 正在載入⋯  | **正在載入…** | HK's form; TW mixes a `中`-suffix that clashes with our consistent `正在…` progress pattern |

**On `duplicate`**: Taiwan's 複製 is the word this catalog already uses for _copy_, and Cmdr ships Copy and Duplicate as
two separate commands. Taking Hong Kong's 製作副本 keeps them distinct.

Where TW and HK agree, there's nothing to rule on, and the term differs from Simplified
anyway: 程式 (176/177), 設定 (160/160), 儲存 (123/123), 搜尋 (82/82), 磁碟 (69/70), 卷宗 (68/69), 預設 (27/27), 影片 (16/14), 記憶體, 硬碟, 垃圾桶, 副檔名, 刪除, 重新命名.

### Tech-term strategy: Apple first, then Microsoft, then the two-pane pair

- Traditional Chinese has mature native IT vocabulary, so prefer the established Chinese term over an English loan.
- **Apple (zh-TW + zh-HK agreeing) is the top authority**, with the outlier refinement above.
- **Where Apple is silent, Microsoft's zh-Hant TBX fills
  in**: 窗格 (pane), 佇列 (queue), 篩選 (filter), 鍵盤快速鍵 (keyboard shortcut), 深色模式 (dark
  mode), 對話方塊 (dialog), 磁碟機 (drive as a device), 佈景主題 (theme).
- **For the two-pane concepts every OS vendor lacks, the orthodox pair is the lineage match**: 檔案清單 (file
  list), 命令列 (command line), 功能鍵列 (function-key bar), 常用資料夾 (directory
  hotlist), 比對資料夾, 同步資料夾, 多檔重新命名. Details and the pane trap: `glossary.md` § Two-pane vocabulary.

### Gender and inclusive language: inherently neutral

Chinese has no grammatical gender on nouns or verbs and no verb agreement. UI rarely needs a third-person pronoun
because Cmdr addresses the user in second person (`你`, ungendered) and refers to files as things. No special handling;
keep strings second-person or item-referring and gender never arises. `high`.

## Punctuation

Get this right once: punctuation is what most makes CJK copy read foreign. All counts below are Apple's zh-TW Finder
bundle unless noted.

- **Quote with corner brackets `「…」`, never `“…”`.** Apple zh-TW uses `「」` 620 times against 2 curly quotes (and
  those 2 are a quote-style _sample_ string, not real quoting); zh-HK matches at 592. Simplified is the opposite (`“…”`
  858), which is exactly why a converted catalog reads mainland. Quote filenames, menu names, and setting names this
  way: `無法開啟「%@」。` `『…』` is for nesting only and is **unattested in the pile** — restructure rather than nest.
- **Full-width punctuation throughout**: `，` `。` `：` `？` `！` `（）`. Apple zh-TW: 371 `，`, 664 `。`, 135 `：`, 100
  full-width parens against 23 half-width.
- **❌ Never use the full-width semicolon `；`.** Apple uses it zero times in either Traditional corpus. Split the
  sentence or use `，` instead.
- **`、` for tight in-sentence lists** (`KiB、MiB、GiB`), used but sparingly (15–20 occurrences). Use it between short
  list items inside a sentence; use `，` between clauses.
- **Ellipsis is `…` (U+2026), one character.** ❗ Apple writes `⋯` (U+22EF) in Traditional, and we deliberately do NOT
  follow it: Cmdr normalizes on `…` across every catalog (157 in `en`, 147 in `zh`, 0 `⋯` anywhere). Keeping one
  ellipsis form app-wide beats matching Apple's glyph. Never the doubled literary `……`, never ASCII `...`.
- **Arabic numerals (0-9)** for every count, size, and percentage, as Apple and all majors do. Chinese numerals (一二三)
  are prose-only.

### Spacing: put a space between Chinese and Latin

**Write `macOS 通知已關閉`, not `macOS通知已關閉`.** Insert a space on both sides of any Latin word, number, or
placeholder embedded in Chinese text.

This is a **deliberate departure from Apple**, and the one place we don't follow Tier 1, so it's recorded with its
evidence rather than left to drift:

- Apple zh-TW runs everything tight (`顯示iCloud進度`, `執行「%@」應用程式需要較新版的macOS。`): 384 tight vs 2 spaced.
- **But that's Apple's UI CHROME, not Apple. Apple's own modern Traditional legal prose SPACES**: 224 spaced against 5
  tight in Feedback Assistant's `License.rtf` (`cocoartf2761`), and all 5 tight are the single date `2024年9月9日`,
  which is the date carve-out below. So the departure is narrower than "we don't follow Apple": on the one Apple surface
  written as running prose rather than as labels, Apple already agrees with us, date compound included. Count Han
  (U+4E00–U+9FFF) against `[A-Za-z0-9]` only; letting full-width `、「（` count as Han inflates the tight side to 105
  with bracket adjacencies no spacing rule covers. (Measured on macOS 26.6.2, build 25G83, 2026-08-29.)
- Every community Traditional catalog spaces it: Nautilus 248 spaced / 15 tight, Dolphin 108 / 2, Thunar 84 / 0, Total
  Commander 288 / 1, Double Commander 101 / 17.
- **Cmdr's own `zh` catalog spaces it**, 845 spaced / 33 tight, and the two Chinese catalogs must not disagree on
  typography.

It also earns its keep functionally: several placeholders arrive **pre-formatted as Latin text** (`{duration}` renders
`45s` / `2m 30s`, sizes render `4.2 GB`), so they land mid-sentence as a Latin run. `剩餘約 {duration}` is legible;
`剩餘約{duration}` is not. Space both sides of every placeholder that renders Latin.

**Carve-out: a Chinese date compound is one unit.** Write `8月1日`, `2026 年7月1日`, `今天 0:00`, never `8月 1 日`. The
generic rule above exists to keep a Latin RUN legible inside Han text; a date is a fixed Chinese compound where the
digits are structural, and spacing it apart reads as broken. A bare four-digit `{year}` placeholder still takes its
space (`2026 年`), because that one really is a Latin run arriving from outside. Instances: `queryUi.date.preset.*`.

**Carve-out: a `{placeholder}` the OS fills still gets its spaces.** `{systemSettings}` and friends follow the SYSTEM
language, not the app language, so a `zh-Hant` user on an English macOS gets a Latin word in that slot. Space it
(`開啟 {systemSettings}`), which is what `fileExplorer.restrictedFolder.tooltip` and `errors.*` already do.

**Ruled: a Latin brand + Han descriptor stays SPACED too (`iCloud 雲碟`), like every other Latin run.** This one looks
like it should be a carve-out and isn't, so the reasoning is recorded here to stop it being flipped back and forth.

The tempting argument: Apple writes `iCloud雲碟` tight in 49 of 49 occurrences across zh-TW and zh-HK (and `iCloud云盘`
49 times in zh-CN), with the spaced form appearing nowhere. Same shape for `Word文件`, `Zip封存檔`, `iCloud設定`. So a
brand fused with a Han descriptor into one product name looks like a single lexical unit that ought to follow Apple
tight, distinct from a Latin word merely sitting in a Chinese sentence.

**That argument doesn't survive contact with the counts, because Apple's tightness isn't about compounds at all.** Apple
runs EVERY Latin/Han junction together, 384 tight against 2 spaced corpus-wide (§ Spacing). `iCloud雲碟` is just that
one global convention applying again, so it carries no information about compounds specifically. The catalog already
rejects that convention wholesale; re-importing it for one word class would be taking the rule back through a side door.

The discriminating test is what a source that DOES space generally does when it hits a brand compound. Both such sources
space them, and it isn't close:

- **Microsoft zh-Hant is 7,667 spaced against 11 tight** across the whole terminology database, and the 11 are
  truncation glitches (`P位址` = a mangled `IP 位址`). Its spaced entries include canonical product names of exactly the
  contested shape: `Office LTSC 專業版`, `Microsoft 知識庫`, `Windows 連絡人`, `SQL 資料庫`, `USB 磁碟機`,
  `Web 應用程式`.
- **The zh-TW file managers space them too**: `KDE 軟體`, `GNOME 桌面`, `USB 磁碟`, `ZIP 指令`, `SSH 檔案傳輸協定`. A
  scan for genuine tight compounds in those catalogs returns nothing but escape-sequence artifacts (`\n動態`, `\t取消`).
- **Cmdr's own `zh` catalog writes `iCloud 云盘` spaced** in `errors.provider.iCloud.needsAction`, and the two Chinese
  catalogs must not disagree on typography, which is the same argument that decided § Spacing.

There's also no boundary anyone could apply later. This catalog already ships `MTP 裝置`, `SMB 伺服器`, `USB 連接埠`,
`AI 提供者`, and `AI 模型` spaced, all of them a Latin token fused to a Han descriptor. A rule that tightened "brand
compounds" would leave a future agent unable to say which side `Zip 封存檔` or `SMB 伺服器` falls on, and an unappliable
rule drifts by definition.

❌ **Don't retighten these by citing Apple.** Apple's rendering of a brand compound is evidence about Apple's spacing
convention, which this catalog deliberately departs from, not evidence about brand compounds. Recognition doesn't turn
on the space: a reader who knows the Finder sidebar's `iCloud雲碟` reads `iCloud 雲碟` as the same product. And the
citation is weaker than it looks, because **Apple isn't self-consistent**: its UI chrome is 384 tight / 2 spaced while
its modern legal prose is 224 spaced / 5 tight (§ Spacing). "Apple writes it tight" is therefore a claim about label
typography in one bundle, never a house rule, and it can't outrank the four spacing sources this catalog does follow.

Exception: don't add spaces _inside_ a Latin run (`64.0 MB/1.33 GB` stays as it is), and don't space a full-width
bracket against the text it wraps.

## Terminology and glossary

The full sourced term list is in `glossary.md`, in `chosen · sources · confidence` format. Read it before translating
and extend it as you settle terms. The highest-traffic head terms, for orientation:

檔案 (file) · 資料夾 (folder) · 目錄 (directory) · 磁碟 (disk) · 磁碟機 (drive) · 卷宗 (volume) · 路徑 (path)
·副檔名 (file extension) · 項目 (item) · 複製 (copy) · 移動 (move) · 刪除 (delete) · 重新命名 (rename)
·製作副本 (duplicate) · 垃圾桶 (Trash) · 壓縮 (compress) · 解壓縮 (extract) · 開啟 (open) · 儲存 (save) ·搜尋 (search)
· 設定 (settings) · 預設 (default) · 窗格 (pane) · 分頁 (tab) · 視窗 (window) · 選單 (menu) ·標籤 (tag)
· 書籤 (bookmark) · 喜好項目 (favorite) · 索引 (index) · 佇列 (queue) · 略過 (skip) · 覆寫 (overwrite) ·取消 (cancel)
· 再試一次 (retry) · 網路 (network) · 伺服器 (server) · 記憶體 (memory) · 可用空間 (free space).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, plus the `{system_settings}`-style tokens. The
curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

Chinese doesn't inflect, so the brand-inflection principle is a no-op here: `Cmdr` always appears bare, with a space on
each side when it sits in Chinese text.

**Quick Look is translated**, not kept English: Apple's Traditional localizations say `快速查看` (zh-TW = zh-HK =
zh-CN), which is why it isn't on the do-not-translate list.

## Plurals

CLDR category: **`other` only** (verified with `new Intl.PluralRules('zh-Hant').resolvedOptions().pluralCategories`,
2026-08-29). Chinese has no grammatical number on nouns; one form covers one and many, and counting uses measure words
(classifiers), not inflection.

- Every ICU plural message needs only the `other` branch. `desktop-i18n-plural` requires the categories the language
  needs; for Traditional Chinese that's just `other`.
- Write the `other` branch to read naturally for any count, including 1. Counted nouns want a measure word:
  `{count} 個項目`, `{countText} 個檔案`, `{countText} 張圖片`. Mind which classifier the noun takes (個 for items and
  files, 張 for images and photos, 部 for a Mac, 項 for operations).
- Keep the ICU `{count, plural, other {…}}` wrapper even with one branch; parity is checked against the English
  structure.

## Notes and decisions

- **`檢視` is the verb, `顯示方式` is the View menu's noun.** Apple splits them (`View` → `顯示方式` as a menu title and
  view-mode noun; `to view %@` → `檢視`). Picking the wrong one makes a button sound like a view-mode switch. Same trap
  the `zh` catalog records for `查看` vs `显示`.
- **`標籤` is overloaded, so `tab` must not use it.** Apple zh-TW writes both "tag" and "tab" with 標籤 (標籤 / 標籤頁).
  Cmdr ships Finder tags AND browser-style tabs in the same UI, so we take the pan-Traditional `分頁` for tab and leave
  `標籤` to mean tag alone. This is a second, independent reason for the `分頁` ruling above.
- **`預設` means "default" here, which collides with "preset".** Microsoft's zh-Hant TBX renders _preset_ as `預設` too.
  No shipped value currently contains the word "preset" (it appears only in `@key` descriptions and in
  `queryUi.date.preset.*` key NAMES, whose values are "Today", "Yesterday", …), so nothing is broken today. If a visible
  "preset" ever appears, write `預設組合` and keep bare `預設` for _default_.
- **Menu names in running text get corner brackets, not a path.** Write `請從「說明」選單傳送新的報告。` in prose, and
  reserve the bold `Cmdr > 引導設定…` path shape for step-by-step onboarding instructions. Menu names must match the
  `menu.*` keys exactly, so a copy edit can't drift the two apart.
- **Toast strings that follow a colon carry the wrapper's verb, so don't repeat it.** Several error values are dropped
  into a wrapper key and read as the sentence AFTER the colon; write them to continue that sentence and pick a different
  construction for the second clause. Same mechanic the `zh` catalog documents.
- **Keep the trailing `…` wherever the English has one** (a menu item or button that opens a further dialog), and keep
  the `*Aria` containment rule in mind: an aria value must contain its visible label verbatim and in order. Chinese
  doesn't inflect, so this is easy here — just don't paraphrase the label inside the aria sentence.

### `*Aria` containment pairs that are load-bearing

WCAG 2.5.3 asks an accessible name to CONTAIN its visible label verbatim. Chinese doesn't inflect, so this is usually
free, but four pairs in this catalog only hold because the aria sentence was shaped around the label rather than the
other way round. Re-wording either half alone breaks it silently, and no check catches it:

- `fileOperations.transferProgress.background` = `背景執行` ⊂ `backgroundAria` = `讓它繼續在背景執行`
- `fileOperations.transferProgress.queue` = `加入佇列` ⊂ `queueAria` = `加入佇列，移到「操作佇列」視窗管理`
- `queryUi.filters.chip.scope` / `queryUi.scope.popover.label` = `搜尋範圍` ⊂ `queryUi.scope.popover.aria` =
  `搜尋範圍：選擇資料夾` (a literal `在資料夾中搜尋` would NOT have contained it)
- `queryUi.scope.toggle.caseSensitive` = `區分大小寫` ⊂ `caseSensitiveAria` = `比對時區分大小寫`

The fix shape, whenever a new pair appears: open the aria with the label's exact words, then continue the sentence.

### The `verbName` family assembles with `把`

`askCmdr.decision.rejected` / `.approved` drop `askCmdr.decision.verb*` into a `verbName` slot. English puts the verb
BEFORE the count ("You turned down moving 5 items"); Chinese can't, because a bare verb has nothing to govern. The
family is written so the object comes first with `把`:

`你拒絕了把 {countText} 個項目` + `移到垃圾桶` → `你拒絕了把 5 個項目移到垃圾桶`

So every `verb*` value is a verb PHRASE that completes a `把` construction (`移到別的資料夾`, `複製到別的資料夾`,
`移到垃圾桶`, `永久刪除`, `重新命名`, `壓縮成封存檔`, `解壓縮`), not a bare verb. The nine keys are one unit: changing
either sentence means re-checking all seven verbs against it.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Traditional Chinese rarely needs one, but any apostrophe in an English fragment must be doubled.
- The RAW families never meet ICU, so their apostrophes stay SINGLE and their `{token}`s are literal: `errors.*`, plus
  the native ones Rust draws (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`).
- Keep every `{placeholder}` and `<tag>` verbatim and identical to English. Full rules: the agent-handoff block in
  `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary is in `glossary.md`. Read it before translating and add to it as you settle terms, each sourced
from the reference pile (`_ignored/i18n/zh-TW/`, `zh-HK/`, and `zh-Hant/` for the Microsoft sources; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
