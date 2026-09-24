# zh decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`, plus this locale's `concepts-proposed.json`); open questions for a native reviewer live in
`review-queue.md`. Style and voice: `style.md`.

Evidence tiers throughout: macOS zh-CN (Finder, AppKit, System Settings, and the live `zh_CN.lproj` bundles) is Tier 1,
Microsoft zh-Hans terminology Tier 2, and the file-manager family (Double Commander, Total Commander, Nautilus, Thunar,
Dolphin) Tier 3. The reference pile is `_ignored/i18n/zh/` in the main clone.

## Settings vocabulary (`settings.section.*`, `settings.appearance.*`, `settings.summary.*`)

Terms with no shared concept, kept consistent across every catalog that names them:

- Section names: Appearance `外观`; Behavior `行为`; Language `语言`; AI `AI`; File systems `文件系统`; SMB/Network
  shares `SMB/网络共享`; MTP `MTP（Android/Kindle/相机）`; Git `Git`; Viewer `查看器`; Developer `开发者`; Updates &
  privacy `更新与隐私`; Advanced `高级`; Keyboard shortcuts `键盘快捷键`; License `许可证`; Servers (SFTP, WebDAV)
  `服务器（SFTP、WebDAV）`; Android (ADB) `Android（ADB）`.
- Navigation (settings section or card) → `导航` (Microsoft TBX; Finder's `导览` is its verb, and the UI noun is
  `导航`). File operations → `文件操作`.
- Theme modes Light / Dark / System → `浅色` / `深色` / `跟随系统` (Finder and System Settings labels). Tint → `着色`
  (macOS; Microsoft's `淡色` is the other sense), color swatches keep their color names.
- Buffer `缓冲区`, privacy `隐私`, logging `日志`, network `网络` (Microsoft TBX, macOS). A toast is rendered by
  meaning, `提示`, never transliterated.
- Hidden developer strings open with `内部：` (full-width colon), as `settings.indexing.silencedDrives.description`
  does.
- Full-width parens and the enumeration comma in labels: `Servers (SFTP, WebDAV)` → `服务器（SFTP、WebDAV）`.

## View modes, columns, groups, and shortcut scopes (`shortcuts.scope.*`, `shortcuts.section.*`, `fileExplorer.columns.*`, `fileExplorer.navigation.group*`, `queryUi.*`)

- View modes: Full `完整`, Brief `简洁`; as shortcut scopes `完整模式` / `简洁模式`.
- Shortcut scopes: App `应用`; Main window `主窗口`; File list `文件列表`; Volume chooser `宗卷选择器`; Servers
  `服务器`; Places `共享位置`; Favorites menu `个人收藏菜单`; Command palette `命令面板`; About window `关于窗口`;
  Onboarding `入门引导`.
- Shortcut filters: All `全部`; Modified `已修改`; Conflicts `冲突`. Badges: macOS `macOS` (verbatim); Fixed `固定`.
- Columns: Name `名称`; Ext `扩展名`; Size `大小`; Modified `修改日期`; Path `路径`; Actions `操作`.
- Search modes: AI `AI`, Filename `文件名`, Content `内容`, Regex `正则`; filter facets Pattern `模式`, Size `大小`,
  Modified `修改日期`, Search in `搜索范围`; type toggle Both `两者`, Files `文件`, Folders `文件夹`.
- Volume-switcher groups: Favorites `个人收藏`, Volumes `宗卷`, Cloud `云`, Mobile `移动设备`, Network `网络`.

## macOS names in the reserved-shortcut list (`shortcuts.system.*`)

Reuse the localized macOS name: Spotlight `聚焦`; Character Viewer `字符检视器`; Mission Control `调度中心`; App windows
`应用程序窗口`; Spaces `空间`; Force Quit `强制退出`; input source switching `切换输入源`; app switcher `应用切换器`;
screenshots `截屏`; screen recording `录屏`; logging out `退出登录`; locking the screen `锁定屏幕`;
`System Settings > Keyboard` → `系统设置 > 键盘`. Finder is the one exception and stays Latin (`Finder 搜索窗口`), per
the catalog-wide Finder decision in § 原生菜单.

## Error-copy conventions (`errors.*`)

- System Settings panes arrive as runtime tokens (`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}`,
  `{full_disk_access}`), OS-localized; never hand-translate one. Spacing and the pane names the tokens don't cover: §
  Shared `en` fixes.
- Runtime values (`{osMessage}`, `{deviceName}`, `{required}`, `{available}`, `{name}`, `{app}`) stay verbatim, with a
  space against Han text.
- macOS UI labels quoted in prose take full-width `“…”` (`“显示简介”`, `“已锁定”`, `“共享与权限”`).
- Permission denied → `无访问权限` / `没有权限`; authentication failed → `无法通过身份验证` (macOS says `需要认证` for
  "authentication needed"; the style guide bans a bare failure word).
- Get Info `显示简介`; Sharing & Permissions `共享与权限`; Storage (System Settings pane) `储存空间`; an open file
  handle `句柄`; Technical details `技术详情` (descriptive, no macOS source).
- "Here's what to try:" lists open with `可以这样试试：`; "Navigate here again" is `再次进入这里`, and "if it keeps
  happening" is `如果一直这样`.

## Conflict-policy buttons and small dialog words (`fileOperations.transferDialog.policySkip`/`.policyOverwrite`, `fileOperations.transferDialog.operationAria`)

- "Skip all" / "Overwrite all" → `全部跳过` / `全部覆盖`. Chinese collapses ICU `one`/`other` to `other`, so the
  single-conflict case reads `全部…` too; chosen because the policy radios act on the whole conflict set.
- `transferDialog.operationAria` (what the control chooses) → `操作`, with no colon: it's an aria label.
- Under the cursor → `光标所在的` (descriptive); the affirmative OK button → `好` (Apple's OK); Quit & Reopen (the macOS
  relaunch prompt) → `退出并重新打开`.

## Licensing, AI, and viewer vocabulary (`licensing.*`, `ai.*`, `viewer.*`)

- Licensing: organization `组织`; renew `续订`; valid until `有效期至` / validity `有效期`; open beta `公开测试版`.
  Licensing copy uses `你` like every other surface (style.md § Formality); the one `您` is the mail salutation in
  `licensing.dialog.mailtoBody`.
- AI: drop (drag-and-drop onto the composer) `拖放`; budget `预算`; rate-limited `速率限制`; out of quota `配额已用完`;
  estimated cost `预计费用`, "about {amount}" `约 {amount}`; cost unknown `费用未知` (`费用` over Microsoft's `成本费`);
  "free, on-device" `免费，本地运行`; Log AI model calls `记录 AI 模型调用`.
- Viewer: the Western encoding group `西文`; in memory `已在内存中`; the text selection `所选内容`;
  `viewer.saveAs.defaultName` stays the ASCII `selection` (a lowercase, filename-safe base the description requires).

## Indexing, downloads, error reporter, and MTP vocabulary (`indexing.*`, `downloads.*`, `errorReporter.*`, `mtp.*`)

- An index entry `条目` (measure word `个`); replay (recorded file-system changes) `重放`; jump to `跳转到`; register a
  hotkey `注册` / `已注册` / `未注册`; key combination `按键组合`; the report bundle `报告包`; a system daemon
  `守护进程`; `ptpcamerad` and `udev` verbatim; preparing the view `正在准备`.
- Report ID `报告 ID`; update available `可用`; recent `最近` (`最近的路径`, `最近使用`); remove from list
  `从列表中移除`; low disk space `磁盘空间不足`; free space in a readout `剩余`. `feedback.dialog.counter` stays the
  bare `{currentText} / {maxText}` fraction.
- Search: run a search `运行`; entry `条目`; coming soon `即将推出`; refine (AI search) `优化`; swap / switch panes and
  tabs `交换` / `切换`; reopen a tab `重新打开`; Page Up / Page Down `向上翻页` / `向下翻页`; remove a cloud download
  `移除下载`; the playful "boring folders" kept playful, `无聊的文件夹`.

## Operation queue catalog (`queue.*`)

Total Commander zh-CN (the feature's origin: queue plus background controls), Double Commander zh-CN (the same orthodox
two-pane feature), Microsoft zh-Hans cross-check.

- **operation → `操作`**: macOS Finder zh-CN (`NE1` `无法完成此操作。`, `NE82`
  `…因为正在进行其他操作，例如移动或拷贝项目…`, `NE83` `请在当前操作完成后重试。`), Microsoft TBX, Double Commander
  (`Current operation:` → `当前操作：`), and the catalog's own `操作日志` / `文件操作`. Same word as the Operation log,
  so the two View-menu items pair.
- **operation queue → `操作队列`**, never `传输队列`: the English widened from "Transfer queue" because the window also
  lists deletes, trashes, renames, and creations, and "transfer" already means copy-or-move one level down. Microsoft
  builds queue names the same way (`报告队列`, `响应队列`).
- **classifier `项`** for operations (`这项操作`, `{count} 项操作`, `一项系统操作`), the spoken `这项` over Double
  Commander's written `此操作`; generic items keep `{count} 个项目`.
- **background → `后台`** (`在后台运行`), Total Commander and Microsoft; never `背景`, the visual backdrop.
- **status words**: queued `等待中`, running `进行中`, paused `已暂停`, done `已完成`, cancelled `已取消`, failed
  `无法完成`; the toolbar `全部暂停` / `全部继续` / `取消所选`; add to queue `加入队列`.
- **resume → `继续`**, not `恢复` (restore/recover, as macOS uses it for versions).

## Double-click the pane background (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

macOS Finder zh-CN and Double Commander zh-CN (the same two-pane feature).

- Go to / navigate to the parent → `前往上层文件夹` (Finder `Go To Enclosing Folder`); the breadcrumb tooltip
  `点按前往 {path}`.
- Pane background → `窗格背景` in the label, `空白区域` in the description; Double Commander attests both framings
  (`双击视图背景`, `双击文件视图的空白区域`). The label reads `双击窗格背景前往上层文件夹` for both English wordings.
- "not a file row" → `而不是某个文件所在的行`, contrasting the empty area with a clickable row.
- The hint: title `刚刚发生了什么？`; buttons `不喜欢？` / `不再这样做` / `我喜欢`.

## FAT32 too-large-file error (`errors.write.filesTooLargeForFilesystem.*`)

macOS Finder `PE4.5` is the same error: `相对于宗卷的格式，项目"^0"太大，无法拷贝。`

- drive → `驱动器`: the English says "drive" (friendly), not "disk"; Finder's `外置磁盘` is the disk sense.
- too large → `太大`; formatted as FAT32 → `采用 FAT32 格式`; store → `存储` / `存入` (`存入这个驱动器`).
- "and N more files" → `另有 {countText} 个文件` (`另有` = in addition there are).

## Copy and delete dialog labels and the scan spinner (`fileOperations.shared.scanningTooltip`, `fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`)

- Scanning… (while counting the selection) → `正在扫描…`.
- "doesn't exist yet" (destination folder) → `还不存在` (Finder `PE131` `不再存在`; `还` carries "yet").
- "Cmdr will create it during the copy/move" → `Cmdr 会在拷贝时自动创建它` / `Cmdr 会在移动时自动创建它`; `自动` carries
  the reassurance.
- `queue.row.label` progress arms: `正在重命名` / `正在创建文件夹` / `正在创建文件` / `正在编辑压缩文件`, the
  `正在[动词]` shape of the copy and move arms. `创建` is the act (Finder `未能创建文件夹`); `新建文件夹` is the menu
  label.

## Archive browsing (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.archive*`, `settings.archives.*`, `errors.mutation.archive*`)

macOS Finder zh-CN + the two-pane/explorer file-manager family (Total/Double Commander, Nautilus, Thunar) for the
"browse an archive like a folder" feature; Microsoft zh-Hans cross-check.

- **archive (zip/tar/7z, the browsable compressed file)** · `压缩文件` · the whole file-manager family renders this
  exact feature with `压缩文件` (Nautilus/Thunar: `将压缩文件作为文件夹浏览` = browse the archive as a folder,
  `浏览压缩文件内容`, `解压缩文件`), and the existing zh catalog already uses `压缩文件` for compressed files
  (`settings.listing.sizeDisplay.description`: `磁盘映像和压缩文件`). macOS Finder's `归档` (Zip归档, "Kind is archives"
  →归档) is the alternative, but it carries the "compress-into / file-away records" packaging sense; `压缩文件` is what
  a user browsing INTO a zip actually sees across every file manager and reads naturally for zip/tar/7z alike. Chosen
  for the whole archive-browsing surface. · `high`
- **app bundle / bundle (.app/.bundle/.framework, a folder macOS shows as one item)** · `应用程序包` · composed from
  macOS `应用程序` (Applications) + `包` (macOS "Show Package Contents" → `显示包内容`, "Package" → `软件包`);
  `应用程序包` is the established Chinese term for a macOS app bundle. Generic "bundle" alongside "archive" also renders
  `应用程序包` here (the popup only ever targets app bundles). · `high`
- **browse (like a folder, step inside)** · `浏览` · macOS (`浏览` for Browse, 22 hits incl. `48.title` → 浏览) +
  file-manager family (`作为文件夹浏览`). "Browse like a folder" → `像文件夹一样浏览`; segmented-control cell → bare
  `浏览`; summary "browse inside" → `进入浏览`. · `high`
- **extract (an archive)** · `解压` · dominant everyday term for archives (`解压缩文件`); macOS Archive Utility expands
  with 解压缩. Nautilus uses `提取` (extract-a-component sense), rejected here as less idiomatic for whole-archive
  extraction. "browses and extracts" → `浏览和解压`. · `high`
- **damaged (archive/file)** · `已损坏` / `损坏` · macOS Finder (`NE59` `…因为它已损坏`, `LA33` `可能已损坏或不完整`) ·
  `high`
- **encrypted** · `加密` (`被加密`) · macOS Finder (`Encrypted` → 加密) · `high`
- **default app (open with)** · `默认应用` · macOS uses the full `默认应用程序` (`N141`); shortened to `默认应用` for
  the concise menu item `用默认应用打开`. · `high`
- **configure (opens Settings)** · `配置` · macOS (`Configure` → 配置); trailing full-width `…` per the ellipsis
  normalization rule. · `high`
- **pressing Enter / the Enter key** · keep `Enter` verbatim, phrased `按 Enter 键` · matches the dominant existing
  catalog usage (`settings.search.autoApply.description` `按 Enter 键`, `⌘Enter`); macOS doesn't surface a Return-key
  word in this pile, and AppKit keeps key names Latin in `zh-CN` (`FunctionKeyNames.json`: `Escape` → `Escape`, `Tab` →
  `Tab`, verified on macOS 26.6.2, 2026-08-30), so `Enter` stays verbatim and `回车键` is wrong. Every call site now
  says `按 Enter 键` + verb (`按 Enter 键搜索` ×2, `按 Enter 键筛选`, `按 Enter 键时的行为` ×4). · `confirmed`
- **the Escape key** · `Esc 键`, phrased `按 Esc 键` + verb · same shape as `按 Enter 键`; the Mac keycap reads `esc`
  and `Esc 键` is the everyday Chinese name, while AppKit's spelled-out `Escape` reads as a foreign word mid-sentence.
  Used by the Escape full-screen switch (`按 Esc 键退出全屏幕`) and `shortcuts.section.pressEscToClear`
  (`按 Esc 键清除`). · `high`
- **full screen (macOS window mode)** · `全屏幕` · macOS zh-CN (`Enter Full Screen` → 进入全屏幕, `Exit Full Screen`
  →退出全屏幕, `Full Screen Tile` → 全屏幕平铺; AppKit, reference pile) · `confirmed`
- **read-only archive** · `只读压缩文件` · settled `只读` (glossary) + `压缩文件`; mirrors `只读宗卷` / `只读设备`
  pattern. · `high`
- **archive_edit (queue arm, "Editing archive" = changing a zip's entries)** · `正在编辑压缩文件` · `正在[动词]` sibling
  style + function-key-bar verb `编辑` + settled `压缩文件`. · `high`
- **"removed from the zip for good" (delete-warning continuation)** · `将从 zip 中被永久移除` · `永久` = for good;
  `移除` = remove; `zip` kept verbatim (format token); reads as a natural continuation of `压缩文件里没有废纸篓。` ·
  `high`

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

macOS zh-CN Tier 1 (AppKit MenuCommands / Accessibility for paste + image), Double Commander zh-CN for the two-pane
paste op, Microsoft zh-Hans cross-check. Reuses settled `剪贴板`/`拷贝`/`重命名`/`设置` terms.

- **paste (verb)** · `粘贴` · macOS AppKit MenuCommands (`Paste` → 粘贴) + Double Commander (`Paste`/`&Paste` → 粘贴) ·
  `confirmed`. Reused from the search/commands pass (`粘贴` for the clipboard paste op; F5/F6 transfer ops keep
  `拷贝`/`移动`).
- **"paste clipboard content as a file" (settings label)** · `将剪贴板内容粘贴为文件` · composed from settled `剪贴板`
  (clipboard) + `粘贴` (paste) + `内容` (content) + `文件` (file); `将…粘贴为文件` = "paste … as a file", active voice ·
  `high`
- **"do nothing" (radio option, previous no-op behavior)** · `什么都不做` · everyday spoken Mandarin per style.md's
  friendly register (macOS has no single "do nothing" label; Microsoft `不执行任何操作` is stiffer). `high`
- **create file / create and rename (radio options)** · `创建文件` / `创建并重命名` · `创建` = the create verb (Double
  Commander `Create…` → 创建; macOS "未能创建文件夹" uses 创建; the `新建文件` menu label stays for the F-key bar) +
  settled `重命名`; `并` joins the two actions · `high`
- **"Pasted clipboard {image/PDF/text} as {filename}" (confirmation toast)** ·
  `已将剪贴板{图像/PDF/文本}粘贴为 {filename}` · `已` = perfective (done) matching sibling toasts (`已拷贝`, `已装载`);
  ICU `select` branch labels `image`/`pdf`/`other` kept verbatim; only the inside text (图像/PDF/文本) and framing
  translated · `high`
- **image (paste-kind branch)** · `图像` · macOS AppKit Accessibility (`Image` → 图像), Finder `GROUP_IMAGES` → 图像;
  matches the viewer-pass image kind · `confirmed`
- **text (paste-kind branch)** · `文本` · macOS Finder (`纯文本` for plain text) + existing zh viewer catalog
  (`viewer.toolbar.viewMode.text` → 文本) · `confirmed`
- **PDF (paste-kind branch)** · `PDF` · kept verbatim (format/brand token, like the settled `zip`/`FAT32`) · `confirmed`
- **⌘V (paste shortcut glyph)** · `⌘V` · kept verbatim per SYSTEM_TOKENS / do-not-translate (matches the catalog's
  `⌘C`/`⌘Enter` handling) · `confirmed`

## Archive-password dialog and Compress (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- password-protected → `受密码保护` · TC/DC zh phrasing + macOS · high. Body: "…… 受密码保护。"
- password (noun) → `密码` · macOS/MS · confirmed.
- unlock (button + verb) → `解锁` · macOS AppKit ("解锁") · high.
- archive (the `{name}` head / input label) → `压缩文件` (compressed file) · settled zh glossary · confirmed. Input
  aria-label "压缩文件密码".

Settled while translating the Compress feature:

- compress (verb / control label) → `压缩` · Finder `zh/macOS` ("压缩项目", `Compress ${sources}` → "压缩${sources}") ·
  high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb branches.
- compressing (progress form) → `正在压缩` · derived on the sibling `正在拷贝`/`正在移动` · high. `scanTitleCompress` =
  "压缩前正在核对…".
- compressed (result toast) → `已压缩` · mirrors `transfer.split.clean` ("已拷贝 {phrase}。") · high. Plural uses only
  the `other` CLDR category, matching the sibling toasts.
- replace (overwrite warning) → `替换` · Finder `Replace` → "替换" · high.
- archive (name) → `归档` · Finder `Zip archive` → "Zip归档" · high. `.zip` in straight double quotes, spaced from the
  surrounding Han text.
- compression level (slider label) → `压缩级别` · TC `zh` "内部 ZIP 压缩级别(0-9)" (exact term) · high.
  `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `更快` · TC `zh` "最快压缩(1)" (最快 = fastest); `更快` (faster) for the slider end
  · high. Marks quicker packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `更小` · pairs with `更快`; marks the smaller output file (TC `zh` high end
  "最大压缩") · high. `.smaller`.

## Operation log catalog (`operationLog.*` + `commands.logOperationLog.*`)

macOS zh-CN Tier 1, Microsoft zh-Hans cross-check. Reuses settled queue-status and transfer-verb terms so the log reads
as one feature with the operation queue (`操作队列`), whose head noun `操作` it shares.

- **operation log (the dialog / command name)** · `操作日志` · `操作` (operation, Microsoft TBX / search-pass result
  column) + `日志` (log, settings-pass `logging` → 日志). Standard, natural compound. · `high`
- **operation history** · `操作历史记录` · `历史记录` = history; loadError renders
  `无法加载你的操作历史记录。请稍后重试。` (no bare 失败/错误 per style.md; `请稍后重试` = try again in a moment,
  reusing settled `重试`) · `high`
- **lifecycle status words (match the operation queue's `queue.row.status` exactly)** · Queued `等待中` / Running
  `进行中` / Done `已完成` / Didn''t finish `无法完成` / Canceled `已取消` · reused verbatim from `queue.json` so the
  two surfaces agree; `无法完成` carries the style-guide "avoid failed" rule (same as the queue) · `high`
- **roll back (reverse an operation)** · `回滚` · reused from the file-ops pass (`rollback` → `回滚`, Microsoft TBX).
  Arms: Can''t roll back `无法回滚` / Can roll back `可回滚` / Rolling back `正在回滚` (locale-wide `正在…` in-progress)
  / Rolled back `已回滚` / Partly rolled back `已部分回滚` (`已…` perfective + `部分` = partly) · `high`
- **per-item outcome** · Done `已完成` / Skipped `已跳过` (settled `跳过` + `已` perfective) / Didn''t finish `无法完成`
  / Rolled back `已回滚` · reuses status + rollback terms · `high`
- **summary lines (perfective `已[动词] {countText} 个项目`)** ·
  `已拷贝`/`已移动`/`已删除`/`已重命名`/`已创建`/`已压缩` + measure-word `个项目`/`个文件`/`个文件夹`; trash →
  `已将 {countText} 个项目移到废纸篓` (settled `移到废纸篓`); archive edit/extract → `已编辑压缩文件` / `已解压压缩文件`
  (settled `压缩文件` + `编辑`/`解压`). `已` matches sibling result toasts (`已拷贝`, `已压缩`). Chinese collapses each
  ICU plural to a single `other` branch holding `{countText}`. · `high`
- **"and N more items" (moreItems)** · `另有 {countText} 个项目` · reused verbatim from the FAT32-pass
  `另有 {countText} 个文件` pattern (`另有` = in addition there are), items → 项目 · `high`
- **initiator / provenance labels** · You `你` (informal register, style.md) / AI client `AI 客户端` (keep AI verbatim +
  `客户端` = client) / Agent `代理` (settled agent → 代理) · `high`
- **"Load 50 more" (loadMore button)** · `再加载 50 条` · `再加载` = load more; `条` measure word for log records ·
  `high`

## Network-drive image indexing catalog (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndex*`, `search.imageResults.networkOff/paused`)

macOS zh-CN Tier 1 (Finder/Photos), Microsoft zh-Hans TBX cross-check. Reuses settled
`图像`/`网络驱动器`/`建立索引`/`断开连接`/`暂停`/`继续`/`文件夹` terms. Feature: opting a network (SMB) drive into
background indexing of the text inside its photos.

- **photo vs image — deliberate register split, mirroring the English source** · warm user-facing photo mentions →
  `照片` (macOS Photos app; measure word `张`, e.g. `张照片` in Finder); the feature name / internal label "image
  indexing" → `图像` (settled glossary, matching `fileExplorer.imageIndex.file.excluded` = `未纳入图像搜索` and
  `settings.mediaIndex.enabled.description` = `读取图像中的文字`). The English copy makes the same split (warm "photos"
  in the opt-in rows, technical "image" in the card/label); Chinese follows it faithfully. So `图像索引` = the feature,
  `照片` = the actual pictures. · `high`
- **network drive** · `网络驱动器` · reused from errors/settings glossary; confirmed in the Total Commander zh-CN pile
  (`网络驱动器`, `映射网络驱动器`, `断开网络驱动器连接`) · `confirmed`
- **turn on / opt in (a per-drive switch)** · `开启` · standard modern toggle-on verb (macOS/Microsoft); "turn them on
  here" → `在这里开启`, "turn this on for…" → `可以开启此项` · `high`
- **index (build an index for photos)** · `建立索引` (verb) / `索引` (noun) / `已索引` (perfective, "indexed") · reused
  from settings/indexing pass; "Indexing photos now" → `正在为照片建立索引` (locale-wide `正在…` in-progress);
  "{countText} photos indexed" → `已索引 {countText} 张照片` (Chinese collapses the ICU plural to a single `other`
  branch, measure word `张`); "Not indexed yet" → `尚未索引` · `high`
- **always index (rarely-browsed archive override)** · `始终索引` · `始终` (always) + settled `建立索引`/`索引`; "Always
  index this drive" → `始终索引此驱动器`, "Always-index drives/folders" (internal labels) →
  `始终索引的驱动器`/`始终索引的文件夹` · `high`
- **photo archive (rarely-browsed collection; NOT a zip)** · `照片归档` · `归档` = archive/file-away collection
  (distinct from the zip-browsing `压缩文件` sense settled in the archive-browsing pass — this is a rarely-opened photo
  store, not a compressed file); phrased `如果某个照片归档你很少浏览` to keep it natural · `high`
- **pause / resume / reconnect (indexing lifecycle)** · `暂停` / `继续` / `重新连接` · settled `暂停`/`继续` from the
  transfer-queue pass; `断开连接` (disconnect, settled) + `重新连接` (reconnect, attested in the zh-CN pile); "Paused,
  resumes when this drive reconnects" → `已暂停，将在此驱动器重新连接时继续` · `high`
- **gently (reads photos conservatively over the network)** · `很克制` · `克制` = restrained/measured, carrying the
  "only while you're not busy, limited speed, pauses on disconnect" intent from the @key description; chosen over a
  literal `温和地` (gentle) because the honesty is about restraint, not softness · `high`
- **"Internal:" prefix on hidden dev strings** · `内部：` · reused verbatim from
  `settings.indexing.silencedDrives.description` (`内部：用户已静默索引提示的驱动器。`); full-width colon per style.md ·
  `high`

## Ask Cmdr catalog (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `settings.section.askCmdr`, `commands.askCmdrToggle.*`)

macOS zh-CN Tier 1 (no macOS coverage for this domain: Apple doesn't ship an AI-chat feature), Microsoft zh-Hans TBX
Tier 2 cross-check. Reuses settled settings/errors-pass terms (`提供方`, `模型`, `API 密钥`, `设置`, `配额`, `超时`,
`重试`, `驱动器`, `只读`, `附件`).

- **chat (noun, a conversation with the AI)** · `聊天` · Microsoft TBX (`chat` noun → 聊天, CHN); no macOS tier exists
  for this domain (Apple doesn't localize an AI-chat feature) · `high`
- **chats (the list/history of past chats)** · `聊天记录` · descriptive, matching the everyday Chinese term for a chat
  history list (the same collocation WeChat uses for its chat-history view); distinguishes the collection ("聊天记录")
  from a single conversation ("聊天") throughout the catalog · `high`
- **New chat (button)** · `新建聊天` · composed from settled `新建` (create-new, matches `新建文件夹`) + `聊天` · `high`
- **archive / unarchive (a chat, hide from the active list)** · `存档` / `取消存档` · Microsoft TBX (`archive` verb →
  `存档`, the dominant sense across 2 of 3 TBX hits; `档案` was rejected as the noun/record sense, wrong part of speech
  here) · `high`. Archived badge → `已存档` (perfective, matches the shortcuts-pass badge convention).
- **attach / attachment (a file or folder added to a question)** · `附加` (verb) / `附件` (noun) · Microsoft TBX
  (`attach` → 附加, `attachment` → 附件) · `confirmed`
- **drop (drag-and-drop a file onto the composer)** · `拖放` · standard, ubiquitous Chinese IT term for drag-and-drop
  (no pile-specific hit; ubiquitous across every major OS/app localization) · `high`
- **budget (a tool-step/time limit for one answer)** · `预算` · Microsoft TBX (`budget` → 预算) · `high`
- **rate-limited / out of quota** · `速率限制` / `配额已用完` · Microsoft TBX (`rate limiting` → 速率限制); `配额`
  reused from the errors-pass glossary · `high`
- **token (AI usage-count unit)** · kept verbatim `token`, counted with the measure word `个` (`{countText} 个 token`,
  matching the existing `{count} 个项目` counted-noun pattern and the settings pass's own `token 数`) · reaffirms the
  earlier settings-pass "no settled Chinese UI term" call · `tentative`
- **usage / spending (a chat's token count + estimated cost)** · `用量` (footer label "this chat's usage") / `花费` (the
  settings section heading "Spending") · Microsoft TBX (`usage` → 使用情况, shortened to the more idiomatic `用量` for a
  consumption metric; `spend` noun → 花费) · `high`
- **estimate / estimated cost** · `约 {amount}` ("about {amount}") / `预计费用` ("estimated cost") · `约` = about,
  standard; `预计` = estimated, standard · `high`
- **cost unknown** · `费用未知` · `费用` = cost/fee (chosen over Microsoft's compound `成本费` for a cleaner, more
  common noun); `未知` = unknown (style.md: no bare 失败/错误, `未知` is a neutral honest state) · `high`
- **dashboard (a third-party AI provider's billing/usage web page)** · `仪表板` · Microsoft TBX (`dashboard` → 仪表板) ·
  `high`
- **free, on-device (cost readout for a local-model answer)** · `免费，本地运行` · `免费` = free (standard); `本地运行`
  reuses the phrasing pattern already in the existing catalog (`ai.local.notInstalled`: "完全在你的设备上运行"),
  shortened for the terse footer register · `high`
- **Settings › AI (a settings-path reference inside a sentence)** · `“设置 › AI”` · `设置` = Settings (settled); `›`
  kept verbatim (a literal typographic separator, not a token); wrapped in full-width quotes per the Simplified quoting
  convention for UI-label references (style.md) · `high`
- **Log AI model calls (advanced setting; the local LLM-call log feature)** · `记录 AI 模型调用` · `记录` = log/record
  (verb-led descriptive title, matching sibling advanced-setting labels like `在 SMB 上过滤安全保存产生的临时文件`) ·
  `high`
- No `sameAsSourceJustification` needed except the three literal "Ask Cmdr" product-name keys (`askCmdr.title`,
  `settings.section.askCmdr`, `commands.askCmdrToggle.label`), each justified per-key as the product name kept verbatim.

## Bulk rename review, image-index scope, and Ask Cmdr tool labels (`askCmdr.renameReview.*`, `settings.mediaIndex.scope.*`)

macOS zh-CN Tier 1 (AppKit save/review dialogs, Finder), Microsoft zh-Hans TBX Tier 2, Double Commander zh-CN for the
rename surface. Reuses settled `重命名`/`覆盖`/`移除`/`添加`/`索引`/`照片` terms.

- **review (the modal where the user vets proposed changes before they apply)** · `复查` · macOS AppKit
  (`Review Changes…` → `复查更改…`, `Review Unsaved` → `复查未保存的文稿`, `If you don''t review your documents…` →
  `如果不复查你的文稿…`) — the same surface shape as Cmdr's rename-review modal. `askCmdr.renameReview.title` →
  `复查文件重命名`; `…expired` → `这次复查已过期`. NOTE: `检查` is reserved for "check" (`检查更新`, `正在检查`) and is
  used all over the catalog, so it can''t carry "review"; `审核` (audit/vetting, Microsoft `评审`) was rejected as
  bureaucratic against style.md''s spoken register · `high`
- **allow / deny (per-row approval of one proposed rename)** · `允许` / `拒绝` · Microsoft TBX (`Allow` → 允许, `Deny`
  → 拒绝) + the onboarding-pass macOS permission verbs. "Allow all" / "Deny all" → `全部允许` / `全部拒绝` (settled
  `全部` prefix) · `confirmed`
- **filename extension** · `扩展名` · macOS Finder (`Whether to overwrite or preserve an existing file extension` →
  `要覆盖还是保留现有文件扩展名`). Badge `（扩展名）`, full-width parens per style.md · `high`
- **rename cycle (a→b→a dependency loop needing one temporary name)** · `重命名循环` · Microsoft TBX (`Cycle` → 循环;
  the `周期` sense is time-period, wrong here). Badge `（循环）` · `high`
- **source file (the original file behind a rename row)** · `源文件` · Double Commander zh-CN
  (`Auto-rename source files` → `自动重命名源文件`), the same rename surface · `high`
- **"needs attention" (a rename row blocked by preflight)** · `需要先处理` · the en is deliberately vague about WHAT is
  wrong, so the Chinese stays equally open (`这项重命名需要先处理才能继续。`); no pile source names this state ·
  `tentative`
- **image, in the image-index surfaces** · `图像`, never `图片` · locale-wide consistency:
  `fileExplorer.imageIndex.file.excluded` `未纳入图像搜索`, `settings.section.imageIndexing` and `indexing.enrich.label`
  both `图像索引`, `search.imageResults.*` `图像`. The `fileExplorer.imageIndex.*` status-bar family was reconciled from
  `图片` to `图像` in this pass. The warm/technical split from the 2026-07-13 network-drive pass still holds: actual
  pictures the user thinks of as photos → `照片` (`settings.mediaIndex.chosenFolders.*`), the feature and its status
  labels → `图像` · `high`
- **importance (Cmdr''s ranking of how much a folder matters to this user)** · `重要性` · matches
  `askCmdr.tool.folderImportance` (`正在检查文件夹的重要性`); the scope radio reads `自动，按文件夹的重要性` (was
  `重要程度`, reconciled to one noun) · `high`
- **"lost track of file system changes" (macOS coalesced-events tooltip)** · `没能跟上文件系统的改动` · `改动` matches
  `settings.advanced.fileWatcherDebounce.description` (`文件系统发生改动后…`); phrased as "couldn''t keep up", which
  stays calm and avoids `错误`/`失败` per style.md · `high`

## Image-index status badges (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

macOS zh-CN Tier 1 (AppKit `Indexed` → `已索引`), Total Commander zh-CN (`编入索引`), Dolphin/Nautilus for the index
verb; Microsoft zh-Hans cross-check. Small per-file/folder/drive badges showing image-search indexing state. Reuses
settled `图像搜索`/`图像`/`建立索引`/`已索引`/`驱动器`/`此驱动器` terms.

- **"Indexed for image search" (file badge)** · `已为图像搜索建立索引` · settled `建立索引` + `图像搜索`, with `为…`
  ("for…") carrying the purpose; faithful to the English "for image search" · `high`
- **"Waiting to be indexed" (file badge, queued)** · `等待建立索引` · `等待` (waiting) + settled `建立索引` · `high`
- **"Changed since indexing; will be re-indexed" (file badge, stale)** · `索引后有改动，将重新索引` · `改动` matches
  `settings.advanced.fileWatcherDebounce.description`; `索引` used verbally here ("索引后" / "重新索引"), attested
  verbal `索引` in Dolphin (`对您的文件进行索引`) · `high`
- **"Couldn''t be indexed" (file badge, failed)** · `无法建立索引` · the `无法…` calm pattern (style.md: no 失败/错误) ·
  `high`
- **"Not included in image search" (file badge, excluded)** · `未纳入图像搜索` · `纳入` = include/incorporate; covers
  excluded-folder / out-of-scope / unsupported / too-big without naming the reason (matches the vague English) · `high`
- **folder / drive aggregate counts** · reuse the existing `已索引 {countText} 张照片` pattern (settings network-drive
  pass) but with `图像` (feature register, not `照片`): all-indexed → `已索引全部 {totalText} 张图像`; partial →
  `{totalText} 张图像中已索引 {doneText} 张`; drive-wide in-progress →
  `此驱动器上 {totalText} 张图像中已索引 {doneText} 张；仍在继续。`; drive done →
  `此驱动器上全部 {totalText} 张图像均已索引。`. Measure word `张` per the settled `张图像`/`张照片` pattern; Chinese
  collapses each ICU plural to a single `other` branch holding `张图像` · `high`
- **"still working" (drive in-progress tail)** · `仍在继续` · calm "still going" · `high`
- **badge (small status indicator overlaid on a file icon)** · `标记` · no exact macOS/Microsoft "badge" noun in the
  pile (macOS Finder's icon-overlay badges are the internal `AXBADGE`, surfaced only as verbs like `正在上传`); `标记`
  (mark) is well-attested across the pile and reads naturally for "a small status mark on each image". `角标` (corner
  badge) was the more literal alternative but is absent from the pile and reads more jargon-y. Label
  `在图像文件上显示状态标记`; description `…添加一个小标记…` · `tentative` (term choice; the strings themselves read
  cleanly)
- **"indexed for search" (settings description)** · `已建立搜索索引` · settled `建立索引` + `搜索` · `high`

## Image-indexing settings restructure + semantic-search model (`settings.mediaIndex.cards.*`, `.progressSummary.*`, `.semanticSearch.*`, `.clip.*`, `fileExplorer.imageIndex.file.indexing`)

macOS zh-CN Tier 1 (no macOS coverage for "Apple silicon" or on-device semantic-search UI; Apple's canonical marketing
term used instead), Microsoft zh-Hans TBX cross-check. Three settings-card titles, the Semantic search card, and the
"indexing now" file badge. Reuses settled `语义搜索`/`模型`/`索引`/`建立索引`/`释放`/`关闭`/`标签`/`文件夹` terms.

- **"Indexing now" (both the file badge and the live-progress heading)** · `正在建立索引` · the locale-wide `正在…`
  in-progress form + settled `建立索引`; contrasts cleanly with the sibling `等待建立索引` (pending/queued). Same value
  for `fileExplorer.imageIndex.file.indexing` and `settings.mediaIndex.progressSummary.title` (one active-pass concept)
  · `high`
- **"Enable indexing" (card title, master toggle)** · `启用索引` · settled `启用` (matches
  `settings.network.enabled.label` `启用网络`) + `索引` · `high`
- **"Folders to index" (card title)** · `要索引的文件夹` · `要…的` = "to-be-…" + settled `索引`/`文件夹`; concise card
  title · `high`
- **"search by description" (the semantic-search feature, as a noun phrase inside sentences)** · `“通过描述搜索”` ·
  reuses the existing catalog's `通过描述…搜索照片` framing (`clip.ready` = `通过描述搜索你的照片`); wrapped in
  full-width quotes to mark it as the feature name when it sits mid-sentence (notSupported / offButInstalled /
  deleteConfirmBody). The toggle label "Search photos by description" → `通过描述搜索照片` (no quotes, it IS the label)
  · `high`
- **Apple silicon** · `Apple 芯片` · Apple's canonical zh-CN marketing term; "a Mac with Apple silicon" →
  `搭载 Apple 芯片的 Mac` (Apple's own phrasing on apple.com.cn). NOT in this reference-pile slice (Finder/AppKit don't
  mention it), but it's the established Apple Chinese rendering, so kept over a literal `Apple 硅`. `Apple`/`Mac`
  verbatim · `high`
- **"reclaim / frees {size}" (disk space from deleting the model)** · `释放` · reused from the reclaim pass
  (`reclaim.freed` = `已释放约 {size}`, `reclaim.button` = `…并释放约 {size}`); no `约` here since the English says a
  flat "reclaim {size}" / "frees {size}". `deleteButton` → `删除模型（释放 {size}）` (full-width parens per style.md,
  matching `clip.download` `下载模型（~{sizeText} MB）`) · `high`
- **"Deleting…" (button progress)** · `正在删除…` · `正在…` in-progress + settled `删除` + single full-width `…` ·
  `high`
- **"Delete the semantic search model?" (confirm title)** · `删除语义搜索模型？` · settled `语义搜索`/`模型`/`删除` +
  full-width `？` · `high`
- **keyword search / tag search (other search kinds that keep working)** · `关键词搜索` / `标签搜索` · `关键词` (search
  keywords, the natural modern collocation; Microsoft TBX also attests `关键字`, both understood) + settled `标签`
  (tags, from `settings.listing.showTags.description` `macOS Finder 标签`) · `high`
- **"couldn't be removed just now" (delete-failure toast)** · `暂时无法删除模型。请稍后重试。` · `暂时无法…` (can't for
  now, calm) + `请稍后重试` reused from `operationLog.dialog.loadError`; no bare 失败/错误 per style.md · `high`

## Delete-dialog trash switch + transfer From/To group headings (`fileOperations.delete.trashSwitch`/`confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

- **"Move to trash" (switch in the delete dialog, on = 废纸篓, off = permanent delete)** · `移到废纸篓` · macOS Finder
  zh-CN AL13/N153 verbatim; identical to this file's `transferDialog.titleVerbOnly` `other {移到废纸篓}` arm, so the
  switch and the confirm button read as one pair · `high`
- **"Delete" (destructive confirm button while the switch is off)** · `删除` · settled delete verb, identical to
  `transferDialog.titleVerbOnly`'s `delete {删除}` arm · `high`
- **"From" / "To" (headings over the source path and over the destination volume + path)** · `来源` / `目标` · Total
  Commander zh renders the copy/move source→target pair as `来源: […]` / `目标: […]` (message 112), and `目标` is what
  the group's own controls already carry (`destVolumeAria` = `目标宗卷`, `destPathAria` = `目标路径`), so heading and
  contents agree. TC/DC's other rendering (`从:` / `到:`) rejected: `从` and `到` are coverbs that need a following
  object, so they read as fragments once the path sits BELOW the heading instead of after a colon · `high`

## Drive-indexing master-switch strings (`driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`overriddenBadge`)

- **Settings-path references inside a sentence: wrap the WHOLE path in full-width quotes, `>` with ASCII spaces** ·
  `在“索引 > 驱动器索引”中开启` · the locale-wide settled shape, attested nine times before this batch
  (`在“设置 > AI”中`, `在“设置 > 更新”中更改`, `在“设置 > 更新与隐私”中重新开启`,
  `<settingsLink>设置 > 键盘快捷键</settingsLink>`). Never leave the path bare: `可在 索引 > 驱动器索引 中开启` puts
  ASCII spaces between Han characters, which style.md forbids. (macOS zh-CN quotes each element separately,
  `选取“文件”>“显示简介”`; Cmdr's own one-pair form wins for catalog consistency.) · `high`
- **"Off with drive indexing" (badge on a settings row the master switch overrode)** · `已随驱动器索引关闭` · a badge is
  a STATE label, so it takes the perfective `已…` like the catalog's other badges (`已存档`, `已索引`, `已暂停`); bare
  `随驱动器索引关闭` can read as an instruction. `随…关闭` = "off along with…" · `high`
- **index (drive indexing, this whole family)** · `建立索引` (verb) / `索引` (noun) / `已索引` (perfective) · already
  settled at 34 / 138 / 14 hits. `编入索引` (Total Commander) is NOT this catalog's form; the one leftover in
  `search.imageResults.notIndexed` was reconciled to `尚未建立索引`. Terse status labels may compress to `尚未索引`
  (`settings.mediaIndex.networkVolumes.notIndexedYet`); running prose uses `建立索引` · `high`
- **"turn it on" pointing at a Settings toggle** · `开启` · ~25 catalog hits, all Settings-toggle sense
  (`在“设置 > 更新与隐私”中重新开启`). The per-drive index menu keeps its own `打开索引` (`为此驱动器打开索引`); the two
  are not interchangeable, pick by which switch the string points at · `high`
- **"this drive" in the `driveIndex.*` family** · `此驱动器` · the family is uniformly `此驱动器` (`为此驱动器打开索引`,
  `忘记此驱动器的索引`, the tooltips), which is exactly the terse-label carve-out style.md grants `此`; elsewhere the
  spoken `这个驱动器` still wins · `high`

## 驱动器索引：检查更改这一趟 (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`)

- **"Checking for changes" (run-kind header)** · `检查更改` · sibling headers are verb-object phrases (`首次完整扫描`,
  `快速更新`); `检查` is the settled checking verb (glossary `正在检查`, macOS Finder BN9 `正在检查“^0”的内容`), `更改`
  is catalog-settled (`同步最近的更改`). Header, not live status, so no `正在` prefix · `high`.
- **"Update the file list"** · `更新文件列表` · composed from the settled siblings `保存文件列表` + `更新索引` · `high`.
- **"the check running right now"** · `正在进行的这次检查` · reuses `检查` as this catalog's settled word for a full
  check (`tooltipCoalesced`: `下一次完整检查`) and that string's closing `恢复准确` · `high`.

## 传输停滞提示 / stalled-transfer notice (`fileOperations.transferProgress.stall*`, `close`)

The copy/move progress dialog and the queue row when a transfer has stopped moving (a parked network share or phone),
replacing the countdown we no longer believe. macOS zh-CN Tier 1, Total Commander zh-CN for the
wait-on-a-remote-endpoint phrasing (the exact same surface), Microsoft zh-Hans TBX cross-check.

- **no progress / stalled (nothing has moved for a while)** · `没有进度` (`已有 {duration} 没有进度`) · Microsoft TBX
  `Progress` → `进度`, and Cmdr's own `大小进度`/`文件进度`. `进展` is also attested in TBX compounds (`朗读进展`,
  `写作进展`) but was rejected to keep ONE progress word across the catalog; `已有 X 没有进度` is the colloquial
  "nothing has happened for X" frame · `high`
- **respond / "waiting for X to respond"** · `响应`, as `正在等待…响应` · macOS AppKit
  (`did not respond to the request for services` → `没有响应服务请求`) plus Total Commander zh-CN, which has this exact
  surface (`等待服务器响应...`, `正在发送数据，等待响应...`, `没有响应(超时)!`). TC uses a bare `等待…`; Cmdr adds the
  locale-wide `正在…` in-progress prefix (`正在扫描…`, `正在检查冲突…`) · `high`
- **destination (the drive/share/phone being written TO, inside a sentence)** · `目标位置` · the settled transfer
  `目标位置`, consistent with `目标宗卷`/`目标路径`/`transferDialog.targetGroupTitle` = `目标`. macOS Finder's own word
  is `目的位置`/`目的宗卷`, but the catalog is uniformly `目标`; bare `目标` was rejected mid-sentence because it reads
  as "goal" without the heading around it · `high`
- **source (the drive/share/phone being read FROM, inside a sentence)** · `来源` · reused from
  `transferDialog.sourceGroupTitle` and TC's copy/move `来源:` / `目标:` pair. Deliberately asymmetric with the
  `目标位置` above: the two strings are alternatives and never render together, so each is optimized for reading alone ·
  `high`
- **"has stopped moving" (stalled, and NOT paused)** · `停住不动了` · descriptive. Must stay clearly distinct from the
  settled `已暂停` (paused) so a stall never reads as a pause the person caused, and it avoids both `卡住` (stuck, more
  alarming than the en) and the banned `失败`/`错误` · `tentative`
- **"leave it running in the background"** · `让它在后台继续运行` · lifted verbatim from the settled
  `transferProgress.queueTooltip`; `后台` from the queue pass (TC `后台`, NOT `背景`). The two-way-out sentence renders
  `可以取消它，也可以让它在后台继续运行。` — Chinese's standard way to offer a choice, where a bare imperative pair
  would read as an instruction · `high`
- **"N files are still open"** · `# 个文件仍处于打开状态` · macOS Finder PE
  (`… can't be moved to the Trash because they are open` → `因为它们已打开`) gives the `打开` root; `仍处于…状态` is the
  standard status construction on it. Measure word `个` per the `{count} 个项目` pattern · `high`
- **"partly written" (an open file that already has bytes at the destination)** · `可能已经写入了一部分内容` · `写入`
  reused from `transferProgress.titleFlushing` (`正在写入最后一部分…`) · `high`
- **"The log has the details."** · `日志里有详细信息。` · `日志` (settings pass) + Microsoft TBX `Details` → `详细信息`.
  Kept as a STATEMENT, matching the en; the catalog's other framing `详情请看操作日志` (askCmdr) is directive and points
  at the operation log, a different surface · `high`
- **Close (dialog button that dismisses while the work keeps running)** · `关闭` · macOS AppKit `Close` → `关闭`
  (`Document`, `WindowTabs`), reusing the settled term. Sits next to `取消` (Cancel) and is unmistakable against it at
  two characters each · `confirmed`
- **Pre-formatted Latin tokens inside Chinese prose.** `{duration}` arrives already formatted and unlocalized (`45s`,
  `2m 30s`, `1h 5m` from `units/duration.ts`), so it lands as Latin text in a Chinese sentence: keep a space on BOTH
  sides (`已有 {duration} 没有进度`), the same way `剩余约 {duration}` already does · `high`

## 已拷贝路径：剪贴板确认提示 (`fileExplorer.clipboard.copiedPath`)

一个键：按 ⌘⌥C 之后的信息提示行。路径本身在下一行以等宽字体单独显示，因此它并不是句中的占位符——句子以全角冒号结尾，去掉路径后也必须读得通。

- **"Copied the path, it's now on your clipboard:" → `已将路径拷贝到剪贴板：`** · 复用词汇表中已确认的 `path → 路径` 与
  `clipboard → 剪贴板`，动词沿用 Finder 的 `拷贝` · confirmed。`已将 X 拷贝到 Y`
  与同批的粘贴提示 (`已将剪贴板{图像/PDF/文本}粘贴为 {filename}`) 同构，`已` 表示动作已完成。英文的 "it's now on your
  clipboard" 合并进 `到剪贴板`：中文不给唯一的剪贴板加物主代词。冒号用全角 `：`。

## Corner progress chip + failure notice (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

Nine keys for two new surfaces: the main window's ~80 px corner progress chip (a button that opens the queue window) and
the never-auto-dismissing failure notice plus its failed queue row. Head noun `操作`, window name `操作队列`, and the
classifier `项` all come from the operation-queue section above; this section only records what that one doesn't.

- **dismiss (button that removes a failed row / a notice, undoing and retrying nothing)** · `关闭` · all nine Dismiss
  keys carry it; the macOS evidence is in the Dismiss entry of the 2026-08-30 convergence block below. Beyond that, the
  pile offers no competing first-party term: Microsoft TBX gives `消除`/`关闭` (both defined as "turn off a system
  notification"), and none of the five file managers has "dismiss" at all. `清除` (macOS `Clear Menu` → `清除菜单`) and
  `移除` (macOS `Remove` → `移除`) were rejected: both read as deleting something, and the row deletes nothing.
  `queue.row.dismissAria` = `关闭这项操作的记录`; the block below says why it can't be shortened · `confirmed`
- **dismiss all (toolbar)** · `全部关闭` · the `全部 + 动词` family shape this window already uses (`全部暂停`,
  `全部继续`) and the rename-review pass's `全部允许`/`全部拒绝` · `confirmed`
- **"Couldn''t finish <action>" (the failure notice's nine `select` arms)** · `无法完成…操作` · built on the settled
  `queue.row.status` failed arm `无法完成` so the toast and the row say the same thing, and closed with the head noun
  `操作` so every arm is grammatical: 完成 wants a nominal object, and a bare `无法完成移到废纸篓` (a full verb phrase)
  is not one. Disyllabic verbs compound directly (`无法完成拷贝操作`, `移动`, `删除`, `重命名`); multi-word verb phrases
  take `的` (`无法完成移到废纸篓的操作`, `创建文件夹的`, `创建文件的`, `编辑压缩文件的`). The `other` arm is
  `无法完成这项操作`, which is macOS Finder `NE1` (`无法完成此操作。`) with style.md's spoken `这项` for the written
  `此`. Never `失败`/`错误` here · `high`
- **"N operations couldn''t finish" (toast summary + chip)** · `{countText} 项操作无法完成` · classifier `项` per the
  operation-queue section; predicate-final so the count leads the line the way English does · `high`
- **"Show in operation queue" (the notice's button)** · `在操作队列中显示` · macOS Finder zh-CN renders every
  `Show in X` as `在X中显示` (`A34`/`N207` `Show in Finder` → `在访达中显示`, `N162` `Show in Enclosing Folder` →
  `在上层文件夹中显示`), and Cmdr's own catalog already ships `在 Finder 中显示`. No spaces inside, since every
  character is Han · `high`
- **"Open the operation queue to see why." (the chip's second sentence)** · `打开操作队列即可查看原因。` · `即可` keeps
  the promise (press it and you get the reason) without an imperative; `查看` is the settled view verb · `high`
- **percent, spoken (`queue.chip.ariaLabel`)** · `已完成 {percentText}%`, the `%` sign, NOT a spelled-out
  `百分之 {percentText}` · macOS Finder zh-CN `MR22` (`^0% complete` → `已完成^0%`) and `PW13.1` (`^0%` → `^0%`) both
  keep the sign; Chinese has no short spelled-out percent form, and VoiceOver zh-CN reads `42%` as 百分之四十二 on its
  own. macOS's own accessibility phrasing `PW13.2` (`Percent complete: ^0` → `已完成百分比：^0`) confirms `已完成` as
  the progress frame. ❌ No space before `%` in Chinese (that rule is de/fr/sv) · `high`
- **the chip tooltip's shape (`queue.chip.tooltip`)** · the middle dots keep an ASCII space on BOTH sides, exactly as
  English: an unspaced `·` is reserved in Chinese for the components of a transliterated name (`史蒂夫·乔布斯`), so
  `个项目·42%` would misread as one compound, and the segments it joins are digit- and Han-initial in turn, where the
  Han↔Latin spacing rule already wants the space · `high`
- **the tooltip's item count is its OWN dot-separated fact (` · 共 {countText} 个项目`), not a noun phrase glued to
  `{label}`** · `{label}` arrives pre-composed from `queue.row.label`, and four of its nine arms are verb phrases that
  already carry their complement (`正在移到废纸篓`, `正在创建文件夹`, `正在创建文件`, `正在编辑压缩文件`); English can
  append an object to them ("Moving to trash 214 items") but Chinese cannot, so the count moved out into its own fact.
  `共` = in total, matching the placeholder's "how many items the operation covers in total"; measure word `个项目` per
  the settled `{count} 个项目` pattern · `high`
- **the tooltip's destination is quoted, `到“{destination}”`, with no space** · a folder name can be Han (`备份`) or
  Latin (`Backup`), so no fixed spacing is right for both; macOS Finder zh-CN wraps exactly this kind of name in
  full-width quotes (`已暂停拷贝“^0”`, `无法移除“^0”`), which separates the two scripts cleanly either way. The clause
  stays glued to `{label}` because `到` is the verb's complement (`正在拷贝到“Backup”`, and `正在拷贝到“Backup”` still
  reads right when the count clause drops out) · `high`
- **time left, in the tooltip's `{detail}` slot** · nothing to settle: the runtime fills it from the already-settled
  `fileOperations.transferProgress.etaRemaining` = `剩余 {duration}`, or from `queue.row.status`'s `已暂停` · n/a

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

The context line under the `文件已存在` title of the main-window conflict prompt, plus the quiet note under its buttons.
The context line is a `select` VARIANT of `queue.row.label`, so its arms start from that key's settled `正在[动词]`
forms and only add the destination clause.

- **A destination attached directly to a transfer verb in running text** · `正在拷贝到“{destination}”` /
  `正在移动到“{destination}”` · this is the first place in the zh catalog where the destination is the VERB'S COMPLEMENT
  rather than its own dot-separated fact (`queue.chip.tooltip` keeps it as ` · 目标：“{destination}”`, and every other
  surface names it with the noun `目标位置`). Copy takes macOS Finder zh-CN verbatim (`CP3` `Preparing to copy to “^0”`
  → `正在准备拷贝到“^0”`, `CP4_V1` → `正在将“^1”拷贝到“^2”`). Move takes `移动到`, NOT Finder's contracted `移到` (`MV3`
  `正在准备移到“^0”`): `queue.row.label`'s settled arm is `正在移动`, and appending `到` keeps the prompt reading as the
  same operation the queue row named, which is the whole job of this line. The file-manager family agrees (Nautilus
  zh-CN `Moving “%s” to “%s”` → `正在移动“%s”到“%s”`; Double Commander `正在将 "%s" 移动到 "%s"`; Total Commander
  `复制到`/`移动到`). Finder's `移到` stays reserved for the fixed idiom `移到废纸篓` · `high`
- **The destination name is wrapped in full-width `“…”`** · same reason as the chip tooltip: a folder name arrives as
  Han (`备份`) or Latin (`Backup`) and no fixed spacing suits both, while macOS Finder zh-CN quotes exactly this name in
  exactly these strings (`拷贝到“^2”`, `移到“^2”`, `已暂停拷贝“^0”`). Quotes are kept even in running text under a title
  · `high`
- **`archive_edit` splits by design, and the split is real in Chinese too** · `hasDestination: yes` names the archive
  itself, `正在编辑“{destination}”` (Finder's file-name quoting); the `other` arm keeps `queue.row.label`'s generic
  `正在编辑压缩文件`. Never collapse the two · `high`
- **"Working in {destination}" (the generic `other` arm with a destination)** · `正在“{destination}”中进行操作` · the
  sibling's bare `正在处理` is idiomatic ALONE as a status label but strands the sentence once a locative is attached
  (`处理` wants an object), so the arm switches to the head noun `操作` with `进行`, which the queue's own running
  status `进行中` already carries. The no-destination arm stays the sibling's `正在处理` verbatim; the two never render
  together · `high`
- **"Everything else is paused until you answer." → `在你做出选择之前，其余操作会一直暂停。`** · shaped on the catalog's
  own `errors.listing.archiveNeedsPassword.explanation` (`在你解锁之前，里面的内容会一直锁着。`): fronted `在你 V 之前`,
  then `会一直…` for a state that lasts until the boundary and quietly implies it ends there. `暂停` is
  `queue.row.status`'s paused word. `其余` (the rest of a known set) carries "everything else" without a `都` pile.
  `做出选择` over a literal `回答`: the Chinese title `文件已存在` is a STATEMENT, not a question, so "answer" has
  nothing to answer, while the buttons below are literally a choice · `high`

## Empty-queue state of the progress dialog's F2 button (`fileOperations.transferProgress.background`/`backgroundAria`)

The same button as `transferProgress.queue`, worded for an EMPTY operation queue: with nothing to queue behind, it names
what it does instead. Total Commander zh-CN carries this one (its copy dialog has this exact button pair in one
`{COMMON}` block: `4004="后台(&B)"` next to `4005="队列(&Q)"`); Microsoft zh-Hans TBX and Double Commander zh-CN
cross-check. macOS has NO coverage: every `background` hit in the Finder/AppKit pile is the visual-backdrop sense
(`背景颜色`, `选择图片作为“^0”的背景`), which is why `背景` stays banned for this concept.

- **"Background" (the button, empty-queue state)** · `后台运行` · TC gives the button as a bare noun `后台`, but Cmdr
  already rejected TC's bare `队列` for the action `加入队列`, so this sibling takes the verb too: `后台运行` is the
  attested action form (TC `后台运行时不刷新`, Double Commander `程序在后台运行时(&B)`) and echoes the toast the press
  produces, `transferProgress.backgroundedToast` = `仍在后台运行。`. It also lands at four characters, exactly the width
  of `加入队列` on the same button, so the label doesn't jump when the queue empties. `转入后台` / `放到后台` were
  rejected as unattested coinages · `high`
- **"Keep this running in the background" (the aria)** · `让它继续在后台运行` · the settled
  `transferProgress.queueTooltip` phrasing (`让它在后台继续运行`) with `继续` moved ahead of `在后台`, purely so the
  visible label `后台运行` survives as a verbatim substring (WCAG 2.5.3 Label in Name: a voice-control user says the
  label they see). `继续在后台运行` is the equally idiomatic word order, so nothing is lost. **Don't "fix" this back to
  the tooltip's order**: that silently breaks the containment · `high`

## Quit-while-running gate (`main.quit.*`)

The modal that appears when the user quits (⌘Q, the menu, closing the main window) while copies, moves, deletes,
trashes, or archive edits are still going: a title, a reassuring body, a list of the running operations, and a 15-second
countdown after which Cmdr quits on its own. macOS zh-CN Tier 1 (Finder has this EXACT surface), AppKit for the
quit/restart/logout verbs, Double Commander zh-CN cross-check. Head noun `操作` and classifier `项` come from the
operation-queue section above.

- **quit (the app stopping)** · `退出` · macOS AppKit Menus (`Quit` → `退出`, `Quit Anyway` → `仍要退出`,
  `Quit and Keep Windows` → `退出并保留窗口`), Finder `A17`/`BN36`, Double Commander zh-CN
  (`Are you sure you want to quit?` → `您确定要退出吗？`; DC's formal `您` is its register, not ours), plus the
  catalog's own `commands.appQuit.label` = `退出 Cmdr` · `confirmed`
- **"Quit while N operations are running?" (title)** · `有 {countText} 项操作仍在进行，要退出吗？` · macOS Finder `A17`
  is the same sentence from the other side (`The Finder can't quit because some operations are still in progress.` →
  `“访达”不能退出，因为有些操作仍在进行。`), so `操作仍在进行` is lifted from it; the catalog's own
  `settings.mediaIndex.importanceThreshold.waitingForDriveIndex` already ships `仍在进行中`. Chinese fronts the
  condition and asks at the end, which is why the title isn't verb-initial like the English. `有` also gives
  `{countText}` a character to sit after, so the Latin digits keep a space on BOTH sides · `high`
- **"Still running" (heading over the operation rows)** · `仍在进行` · the same `A17` phrase, cut to four characters for
  a small heading; deliberately the same wording as the title's clause so the two read as one thought. Distinct from
  `queue.row.status`'s running arm `进行中`, which labels ONE row's state · `high`
- **"Whatever's finished stays done." (body, first sentence)** · `已经完成的都会保留。` · `已完成` is the settled
  done-status word (`queue.row.status`), `保留` the catalog's settled keep/retain verb (~10 hits, e.g.
  `settings.operationLog.maxAge.label` `保留历史记录时长`) · `high`
- **"anything still being written" (what the app stops mid-write)** · `正在写入的项目` · **the body must stay
  number-neutral**: one operation writes several files at once and several operations can run at once, so the
  classifier-bound `那个项目` states something false. Chinese nouns carry no number, so dropping `那个` is the whole
  fix; `写入` is settled (`fileOperations.transferProgress.titleFlushing` `正在写入最后一部分…`). "stops where it is" →
  `会就此停下`, calm and non-alarmist · `high`
- **"what it leaves half-written" (what quitting leaves behind, and Cmdr removes)** · `写了一半的文件` · the catalog's
  own `settings.advanced.showStagingTempFiles.description` says `半个文件`
  (`这样崩溃就不会留下用真实名称保存的半个文件`), but the measure word `个` binds that to exactly one, so the quit
  dialog takes the verbal `写了一半的` instead · `high`
- **"clears away" (removing that partial file)** · `清理掉` · `清理` = tidy away; chosen over `清除` (which the catalog
  reserves for clearing an index or a search: `清除索引`, `清除搜索`) and over `删除`, because the point is cleanup, not
  a delete the user asked for · `high`
- **"a restart or logout" (the OS's, not Cmdr's)** · `系统重新启动或退出登录` · macOS AppKit Menus verbatim (`Restart` →
  `重新启动`, `Log Out` → `退出登录`); the leading `系统` is added because the sentence already carries `退出` in the
  app sense, and without it `退出登录` could read as Cmdr's own sign-out. NOT Windows' `注销` (the macOS term wins) ·
  `high`
- **"Quitting in {secondsText} seconds, so …" (countdown)** ·
  `将在 {secondsText} 秒后自动退出，这样系统重新启动或退出登录时就不用等 Cmdr。` · `将在 X 秒后` is the standard Chinese
  countdown frame, and the catalog already counts seconds this way (`indexing.eta.secondsLeft` `剩余 {secondsText} 秒`);
  `自动` carries "on its own"; the subject is dropped at the front because `Cmdr` is named at the end, exactly as the
  English does it · `high`
- **"Time until Cmdr quits on its own" (aria on the countdown region)** · `Cmdr 自动退出前的剩余时间` · `剩余` is the
  settled remaining-time word (`fileOperations.transferProgress.etaRemaining` `剩余 {duration}`). This aria has no
  visible label to contain (the visible text is the counting sentence), so WCAG 2.5.3 Label in Name doesn't bind it ·
  `high`
- **"Keep working" (the button that calls the quit off entirely)** · `继续工作` · deliberately NOT `稍后` (the settled
  "later" button in the update surfaces), NOT `取消` (which, next to a list of running operations, would read as
  cancelling the OPERATIONS), and NOT `不退出` (a bare negation, colder than the English). `继续` is the catalog's
  settled carry-on verb, and both readings of `继续工作` land on the right outcome: you keep working AND the operations
  keep going. Nothing is paused on this dialog, so the queue's `继续` = resume-a-paused-operation sense can't be
  triggered here · `high`
- **"Quit now" (the destructive primary; "now" = skip the countdown)** · `立即退出` · macOS Finder's `立即 + 动词`
  button family carries exactly this "don't wait" sense (`立即删除` = Delete Immediately, `立即停止刻录`, `立即备份`),
  so `立即` is what makes the skip-the-wait meaning land. Four characters, matching `继续工作` beside it · `high`
- **Pre-formatted Latin count tokens** · `{countText}` and `{secondsText}` arrive as Latin digits, so they keep a space
  on BOTH sides (`有 {countText} 项操作`, `将在 {secondsText} 秒后`), per style.md and the `{duration}` precedent ·
  `high`

## 使用统计：去掉“匿名”，写明“一个随机标识符” (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `假名化` / `匿名化` — that jargon is
exactly what the copy avoids.

- **usage stats → `使用统计`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the `匿名` modifier
  was cut · high
- **a random id → `一个随机标识符`** · MS terminology zh-Hans (random → `随机`, identifier → `标识符`) · `high`.
  `标识符` is the established native term the style guide prefers over an English `ID` loan, and it is plain enough for
  consumer copy (Apple's Chinese privacy wording uses the same word).
- **tied to → `关联到`** · the catalog's own verb (`onboarding.stepBeta.emailNote` “绝不会和你的使用统计关联”) · `high`

## 等待回答的队列行 + 回滚确认框 (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, 改写的 `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- **"Needs your answer" (queue-row status) → `需要你回答`** · ⚠️ must not open on `等待`: `等待中` is the queued status
  in the same narrow column. `回答` is the catalog's own answering verb (`askCmdr` "来回答问题") and keeps the second
  person `你` per style.md · high
- **the prompt (the on-screen question) → `那个问题`** · the conflict prompt IS a question; `提示` is already the
  catalog's word for a hint/tip banner, so it would read as the wrong surface · high. Main window stays `主窗口`
  (`queue.row.foregroundAria`; Total Commander zh-CN `2083="主窗口"`).
- **"carries on" → `就会继续`** · `继续` is the catalog's continue verb (`main.quit.keepWorking` = `继续工作`) · high
- **"Keep them" (the safe button) → `保留文件`** · macOS "Keep" → `保留`, `保留部分副本`. Spelled out to `保留文件`
  because a bare `保留` beside `回滚` could read as "keep the operation" · high
- **"Roll back" / "Roll this operation back?" → `回滚` / `要回滚这项操作吗？`** · the settled `回滚` family
  (`transferProgress.conflictRollback`, the `operationLog.rollback.*` chips); the `要…吗？` question shape mirrors
  `main.quit.title` · high
- **"Stop" in the rollback tooltip → `停止`** · macOS Finder `PE107` = `停止`, "停止该进程并保留部分副本". ❌ Never
  `取消` here: that IS Cancel, which keeps the finished files, and the tooltip exists to say rollback doesn't · high
- **the files an operation overwrote → `被覆盖掉的文件`** · the settled `覆盖` (overwrite) · high. "won't come back" →
  `找不回来了`, the spoken register style.md asks for.
- `foregroundBusyToast` no longer claims another operation holds the window (`这里已经打开了别的东西`): the blocker can
  be any dialog. "bring this one up" → `再显示这一项`, tying to the row's `显示` (Show) button · high

## 重命名链的其他文件计数 (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

macOS Finder zh-CN Tier 1 (KEY-based en→zh), Xfce Thunar zh-CN cross-check.

- **"and so did {N} other files" (the counted tail of the growing rename toast)** → `其他 {othersText} 个文件也是如此` ·
  macOS Finder renders this exact `"X" and N other items` construction as `“^1”和其他^0个项目` (`MR101_V2/V3` Receiving,
  `MR201_V2/V3` Sending, `PE106_V3/V4` Merge), so "other files" → `其他…个文件`; Thunar zh-CN agrees on `其他文件` for
  "other files". Measure word `个` per the settled `{count} 个文件` pattern, with spaces around the Latin-digit
  placeholder per style.md · `high`
- **Why a separate clause instead of macOS's merged `“{name}”和其他 N 个文件都…`** · the merged subject would bind
  `{reason}` to all N+1 files, but the `@key.description` says the reason describes ONE file only (the earlier ones can
  have had different reasons). `，…也是如此` keeps the reason scoped to `{name}` while still counting the rest · `high`
- **"kept its name" stays `保留了原来的名称`** · verbatim from the sibling `fileExplorer.rename.chainKeptOriginalName`
  (the two are one toast that grows, so they must read as one sentence family) · `confirmed`
- Plural: only the `other` branch, per the Chinese CLDR category; the branch reads naturally for `{others}` = 1 as well
  (`其他 1 个文件也是如此`).

## 无法确认的重命名 + 名称不可用 (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

macOS Finder/AppKit zh-CN Tier 1, plus the zh catalog's own already-settled "couldn't confirm" family.

- **"Couldn''t confirm the rename of X" → `无法确认“{name}”是否已重命名`** · the locale already has this exact sentence
  shape for the same situation one op over: `fileOperations` renders "Couldn''t confirm the folder was created" as
  `无法确认文件夹是否已创建。这个宗卷可能比较慢，所以文件夹也许已经创建好了。` and the trash one as
  `无法确认文件是否已移到废纸篓。该宗卷可能较慢…`. Reused verbatim as a pattern so the whole "we timed out, it may still
  have worked" family reads as one voice, hedge for hedge. `无法…` per style.md (no bare `失败`/`错误`) · `high`
- **"The volume may be slow" → `这个宗卷可能比较慢`** · settled `宗卷` (mounted-disk sense, style.md) + the spoken
  `这个` over `此`/`该`. Keeps the sibling folder-creation string's `可能` hedge verbatim: the English hedges here too,
  because a timeout says nothing about the volume · `high`
- **"the rename may still have gone through" → `所以名称也许已经改好了`** (plural arm: `所以这些名称也许已经改好了`) ·
  `也许已经…了` is the sibling's hedge (`也许已经创建好了`), and macOS Finder attests `可能已` for this "we can''t tell,
  but probably" register (`NE103` `项目可能已过期`, `NE61` `一个或多个项目可能已删除`). This toast must never say the
  file kept its name: `保留了原来的名称` belongs to the `chainKeptOriginalName*` pair, which means the OPPOSITE (the
  rename definitely didn''t apply). `名称…改好了` is the deliberate mirror of it · `high`
- **Why `名称也许已经改好了` and not `重命名也许已经完成了`** · `重命名` is a verb in Chinese and reads awkwardly as the
  subject noun; `名称` is the noun this toast family already turns on (`保留了原来的名称`), so the two toasts contrast
  on the same word · `high`
- **The counted tail reuses `其他 {othersText} 个文件`** · identical to the `chainKeptOriginalNameAndOthers` tail (macOS
  Finder `“^1”和其他^0个项目`); here it sits inside the subject (`“{name}”和其他 N 个文件是否已重命名`) because the
  English counts the files, not a second clause · `high`. Plural: `other` branch only, per the Chinese CLDR category.
- **"That filename can''t be used" → `这个文件名不能使用`** (folder arm `这个文件夹名不能使用`) · macOS Finder `RN31`
  `不能使用名称“^0”。` and AppKit `The name "%@" can't be used.` → `不能使用名称“%@”。` give the `不能使用` verb; the
  subject-first order and the `文件名`/`文件夹名` nouns come from the sibling validation strings in this same catalog
  (`文件名不能为空`, `文件名过长（…）`, `文件名中不能包含“/”或空字符`). "That" → the spoken `这个` per style.md. No
  closing `。`: the string is composed into `{reason}。“{name}”保留了原来的名称。` · `high`

## 建议的操作：Ask Cmdr 提议内容的对话框（`suggestedOps.*`、`commands.suggestedOpsShow.*`）

- ops（代理提议的文件操作）→ `操作`；标题定为 `建议的操作` · 沿用目录中的 "File operations" → `文件操作` · high
- approve → `批准` · 通用译法；未采用 macOS 的 `接受`（那是 AirDrop 接收文件的用词），此处是授权执行 · high
- reject → `拒绝` · macOS Finder AirDrop 面板的 接受/拒绝 词对（Tier 1）· high
- "This can't be undone" → `此操作无法撤销` · macOS Finder 原句（立即删除警告）· high
- pattern → `模式` · 已在 `queryUi.json` 中 · high

## 复制（Duplicate）：在同一文件夹内拷贝的命令（`commands.fileDuplicate.*`）

- **duplicate（把所选项目拷贝到它自己所在文件夹的命令）→ `复制`** · macOS Finder
  zh-CN 的“文件 > 复制”（`N154`），另有“复制项目”和“在当前位置复制项目”（在 macOS 26.6.1 的
  `Finder.app/Contents/Resources/zh_CN.lproj` 中核实，2026-08-19）· `high`。**记住这对术语的分工**：`拷贝` =
  Copy（F5 传输与剪贴板），`复制` = Duplicate。这正是 macOS
  Finder 自己的区分，用户在 Finder 里看到的就是这一对，所以两个命令挨着出现也不算冲突。
- **"Make a copy of the selected files in the same folder" → `在当前文件夹中为选中的文件创建副本`** · 沿用目录里已有的
  `当前文件夹`（`commands.editPaste.description`）和 `副本`（`commands.cloudRemoveDownload.description` 的 `本地副本`）·
  `high`。

## 原生菜单：菜单栏、右键菜单、窗口标题（`menu.*`、`licensing.windowTitle.*`、`main.instanceLock.*`）

这一组的证据来源：macOS 26.5.2 Finder（`Finder.app/Contents/Resources/zh_CN.lproj` 的 `MenuBar.strings` 与
`LocalizableMerged.strings`）是 Tier 1，几乎决定了全部选词；英文一侧读 `en_GB.lproj`，因为 `Base.lproj`
里只有编译过的nib。Safari
26（`MainMenu.strings`）提供标签页词汇，Microsoft 术语库补上 Apple 没有命名的概念。RAW 家族：**用单个撇号**， `''`
会在菜单里显示成两个。

- **菜单栏 → `文件`、`编辑`、`显示`、`前往`、`窗口`、`帮助`、`服务`** · macOS Finder 与 Safari `zh-CN` · high。
- **Select 菜单（选择文件）→ `选择`** · Nautilus/Dolphin `zh-CN` · high。Finder 没有对应菜单。
- **⚠️ Apple 的简体中文把 Finder 叫作「访达」，Cmdr 仍写 `Finder`。** Finder `zh-CN` 的 `A34`
  是「在访达中显示」，但整个 zh 目录（`commands.fileShowInFinder.mac.label` 等）一直用拉丁字母的
  `Finder`，`menu.file.showInFinder` 因此保持
  `在 Finder 中显示`。这是有意的一致性取舍，不是漏译；若以后决定跟随 Apple，需要整目录一起改。
- **Quick Look → `快速查看`** · macOS Finder（`TL14`）·
  high。Apple 会翻译这个功能名，所以它不在 don't-translate 列表里。
- **Get Info → `显示简介`、Enclosing Folder → `上层文件夹`、Go > Home → `个人`、Sort By → `排序方式`、Duplicate →
  `复制`、Copy → `拷贝`** · macOS Finder Tier 1 · high。注意 `复制` 是 Duplicate，`拷贝` 才是 Copy，两者不能互换。
- **pane → `窗格`** · Microsoft 术语库 `zh-Hans`，Double Commander `zh-CN`（「左侧面板」）· high。目录里一直用 `窗格`。
- **ascending / descending → `升序` / `降序`** · Thunar + Dolphin `zh-CN` · high。
- **changelog → `更新日志`**（`menu.app.changelog`）· 与
  `whatsNew.dialog.seeFullChangelog`（`查看完整更新日志`）指同一份文档，所以同字；Microsoft 术语库的 `更改日志`
  曾让菜单项和对话框各叫各的 · high。与帮助 > `新增功能` 区分：一个指文档，一个指消息。
- **word wrap → `自动换行`** · Microsoft 术语库 `zh-Hans`，Double Commander `zh-CN` · high。
- **pin / unpin tab → `固定标签页` / `取消固定标签页`** · Safari `zh-CN`（「固定标签页」）· high。
- **Finder 标签颜色 → `红色、橙色、黄色、绿色、蓝色、紫色、灰色`** · macOS Finder（`TG_COLOR_*`）· high。
- **标签行（`menu.tag.rowLabel`、`menu.tag.addNamed`、`menu.tag.removeNamed`）→
  `标签`、`添加“{color}”`、`移除“{color}”`** · macOS Finder（`TG5`、`TG6`、`N169.37`）· high。动词与
  `commands.tagsToggleRed.description`（“添加或移除”）一致。
- **busy（宗卷正在使用）→ `（占用中）`** · Microsoft 术语库（`忙碌`）· high。磁盘用「占用中」比「忙碌」自然。
- **Eject → `推出`、Disconnect → `断开连接`、Remove（从列表中移除）→ `移除`** · macOS Finder · high。
- **括号与引号用全角**：`{app}（默认）`、`推出（{name}）`、`拷贝“{name}”`。占位符本身保持半角原样。

## 系统连接回退通知（`fileExplorer.network.osMountFallback.*`）

Cmdr 没能建立自己的直接连接，共享改走 macOS 提供的连接时弹出的通知。语气是安抚，不是报错：共享能用，只是慢。

- **native（macOS 内建的）→ `内建`** · macOS Finder/AppKit `zh-CN` 只用 `内建`（4 处，`内置`、`自带` 各 0 处）·
  `high`。「macOS 原生的某个东西」一律写 `内建`（`onboarding.stepOptional.mtp.summary` 的 native handler 也是
  `macOS 内建的处理程序`）；`内置驱动器`（internal
  drive）、`Apple 内置的 Vision 框架`（built-in）是别的英文，不受这条约束。
- **macOS's native SMB network connection → `macOS 内建的 SMB 网络连接`** · 与目录里已定的 `系统连接`
  （`fileExplorer.pane.directConnection*Toast`、`fileExplorer.navigation.connectionTooltipSystem`）指同一件事；这条正文第一次介绍它，所以写全称，短提示里继续用
  `系统连接` · `high`。
- **"4x slower" 这类倍数 → `慢 4 倍`** · 用阿拉伯数字 + `倍`，前后加空格。中文口语里 `慢 N 倍`
  略有歧义（1/N 还是 1/(N+1)），但这里传达的是「慢很多」，精确值不承重；需要精确时改写成 `速度只有…的 1/4` · `high`。
- **click（按钮/链接）→ `点按`** · macOS `zh-CN` 全用 `点按`（`点击` 0 处），onboarding 的
  `点按下方的 <strong>…</strong>` 已是同一句式 · `high`。名词用法也避开 `点击`：写 `每一次点按“在 Finder 中显示”`，不写
  `…的点击`（`settings.revealHandler.*`）。
- **Try connecting directly（按钮）→ `试试直接连接`** · 复用已定的 `直接连接`（`fileExplorer.navigation.connectDirectly`
  = `直接连接，访问更快`）；`试试` 是动词重叠的祈使式，保留英文 "Try" 的「不一定成」的意味，比 `尝试`
  更贴 Cmdr 的口语声音 · `high`。
- **Dismiss（关闭通知的 X 的悬停提示）→ `关闭`** · 与 `lowDiskSpace.toast.closeTooltip` 完全同一个控件、同一个
  `sourceHash`，直接复用 · `confirmed`。所有 Dismiss 都是 `关闭`，见 § 术语漂移审计。

## 重命名/新建被拒绝时的一行提示（`errors.mutation.*`、`errors.volume.*`）

重命名、新建文件夹、新建文件被拒绝时，在名称输入框下方或提示条里显示的一句话。RAW 家族：**用单个撇号**，`{path}`
原样保留。这一批几乎全部复用目录里 `errors.listing.*` / `errors.write.*`
已定的说法，让同一件事在浏览路径和写入路径上说得一样。

- **top folder（宗卷最上面的那层文件夹）→ `顶层文件夹`** · Microsoft 术语库 `zh-Hans`（`root folder` / `root directory`
  / `top-level folder` 都给 `顶层文件夹`）；macOS Finder 只有 `TL_HELP_COMP`「前往电脑的最上一层」，没有名词。整句写
  `宗卷的顶层文件夹无法在这里重命名。`，`无法在这里…` 沿用同一场景的
  `fileExplorer.readOnly.renameMessage`（`这是一个只读宗卷。无法在这里重命名。`）· `high`
- **System Integrity Protection → `“系统完整性保护”`** · macOS Finder zh-CN `ET6`
  （`Some items in the Trash cannot be deleted because of System Integrity Protection.` →
  `由于“系统完整性保护”，无法删除废纸篓中的部分项目。`）·
  `confirmed`。Apple 的原句不给这个词配动词，直接用「因为…，无法…」，中文因此写
  `因为 macOS 的“系统完整性保护”，这个项目无法重命名。`：既避开 `保护…保护` 的重复，也保持平静。连词用
  `因为`（目录里 8 处）而不是 Apple 的 `由于`（目录里仅 1 处）· `high`
- **locked / Get Info（已锁定的项目）→ `已锁定` / `“显示简介”`** · macOS Finder `AXNODE1`（`Locked` → `已锁定`）与
  `NE18`（`Choose File > Get Info, deselect "Locked," and then try again.` →
  `选取“文件”>“显示简介”，取消选择“锁定”，然后重试。`）；目录里 `errors.write.fileLocked.suggestion.mac` 已经是
  `在 Finder 里解锁它（显示简介 > 取消勾选“已锁定”），然后重试。` · `confirmed`
- **"lost track of"（MTP 设备重编号后目标文件夹句柄失效）→ `跟丢了`** · 目录里 `lost track of file system changes`
  已写作 `没能跟上文件系统的改动`（`fileExplorer.navigation.driveIndex.tooltipCoalesced`），同一个 `跟` 词根；`跟丢`
  是口语里现成的说法，比 `句柄失效` 这类术语更贴 style.md 的声音。后半句 "Open it again" 指重新进入这个文件夹，写
  `请再次进入这个文件夹`，沿用 `errors.listing.*` 的 `再次进入这里` · `high`
- **"didn''t work"（密码被拒）→ `不起作用`** · 目录里 `errors.volume.passwordRejected` 就是
  `这个密码不起作用。`；`fileOperations.archivePassword.retryMessage` 的 `这个密码没能解锁…` 是同一件事的长版本。❌ 不写
  `密码错误` · `confirmed`
- **"couldn''t tell what"（说不出具体原因的兜底）→ `也说不清是什么`** · `出了点问题` 是目录里已定的兜底说法（如
  `ai.cloud.genericError`），`说不清` 是日常口语，承接英文有意的谦虚语气 · `high`
- **"may still land"（超时但操作可能仍会成功）→ `也许仍会生效`** · `生效` 目录里已用（6 处，如
  `onboarding.stepFda.postAction.intro`）；`还没有响应` 沿用停滞传输那一批定下的
  `响应`。⚠️ 这条**不是失败**：句子必须保持「还在等」的语气，不能写成没做成 · `high`
- **"restarted its connection"（MTP 会话重置，设备并没有拔掉）→ `设备的连接已重启`** · 逐字复用
  `errors.listing.deviceReconnecting.explanation` 的 `连接已重启` 与其 suggestion 的
  `请等待几秒钟，然后重试。`。⚠️ 主语必须是**连接**，不能写成设备重启，也不能提拔线 · `high`
- **"on its way out" / "something still has it open"（删除挂起）→ `正在退场` / `还有东西占着它`** · 两者都来自
  `errors.write.deletePending.message`（`这个文件正在退场。服务器已标记它待删除，但另一个打开的句柄一直占着它…`）；一行版把
  `句柄` 收成英文同样含糊的 `东西` · `high`
- **"the destination can''t hold that name" → `目标位置存不下这个名称`** · 逐字来自
  `errors.listing.invalidName.explanation`（`…的名称是目标位置存不下的`）；`目标位置` 是已定的 destination 词 · `high`
- **"Move it instead."（压缩文件内外的重命名要改用移动命令）→ `请改用“移动”。`** · `移动`
  是已定的 Move 命令名，加全角引号标明它指的是那个命令。动词对：移出压缩文件 `移出` / 进另一个压缩文件
  `移入`。❌ 不用 Finder 的 `移到`，那是 `移到废纸篓` 的固定搭配 · `high`
- **"archive edit"（改写 zip 条目的那次操作）→ `压缩文件编辑`** · 沿用已定的 `压缩文件` 与队列臂
  `正在编辑压缩文件`；`The archive edit didn''t start.` 写作 `这次压缩文件编辑没能开始。`，`没能`
  是目录里常用的平静说法（18 处）· `high`
- **"There''s nothing at X any more" / "There''s already something at X"** → `“{path}”已经不存在了。` /
  `“{path}”那里已经有东西了。` · 前者用目录里已定的 `已经不存在了`（6 处，`errors.write.sourceNotFound.message.*`
  一族）；后者对应 `errors.listing.alreadyExists.explanation` 的
  `{path} 处已经有一个文件或文件夹`，但英文有意含糊成 "something"，中文照样收成 `东西` · `high`
- **`{path}` 用全角 `“…”` 包起来** · 英文用的是 ASCII 双引号；简体中文按 macOS
  Finder 的习惯（`拷贝“^2”`、`无法移除“^0”`）改全角，路径可能是汉字也可能是拉丁字母，全角引号两种都分得干净 · `high`
- **"has no Trash" / "delete permanently"（第二批，`errors.mutation.trashNotSupported`）→
  `这个宗卷没有废纸篓，只能彻底删除。`** · `废纸篓` 是 Trash 的简体名（style.md），`彻底删除`
  是目录里已定的「永久删除」命令名（功能键栏
  `fileExplorer.functionKeyBar.deletePermanentlyAction`、`menu.file.deletePermanently`、`commands.fileDeletePermanently.label`、`fileExplorer.renameConflict.overwriteDelete`，共 4 处），所以这句话里的说法正好等于用户要去按的那个命令；最近的同义句
  `fileOperations.delete.noTrashWarningStrong/Rest`（`这个宗卷不支持废纸篓。文件将被彻底删除。`）也是这么写的；
  `errors.write.trashNotSupported.suggestion`（`改用 {deletePermanentlyKey} 来彻底删除。`）和
  `operationLog.rollback.refusalPermanentDelete` 同样用 `彻底删除`，目录里不再有 `永久删除` · `high`
- **"macOS wouldn't …"（系统拒绝了这次操作）→ `macOS 拒绝把这个项目移到废纸篓。`** · 动词 `拒绝` 来自 macOS Finder zh-CN
  `MR100`（`“^0”已拒绝你的请求。`），是「系统/服务器不肯照做」这个意思的现成说法；`移到废纸篓`
  是已定的固定搭配（`errors.write.*.trash`
  一族）。英文有意写得短，因为具体原因另在“技术详情”里显示，所以中文也不补原因；宾语用 `这个项目`（同
  `errors.mutation.fileLocked` 的 `这个项目`），因为提示显示在名称输入框下方，光写 `它` 没有先行词 · `high`

## 崩溃对话框的三种开场白（`crashReporter.dialog.body.*`）

下次启动时的崩溃报告对话框现在按报告实际记录的情况，从三句里挑一句。`.ended`
是原来那句（真的意外退出了），`.keptRunning` 和 `.unknown`
是新增的，**这两句都绝不能说 Cmdr 崩溃、退出、关闭或停止**——后台线程 panic 之后应用还在跑，是用户自己退出的；`.unknown`
则来自旧版本写的报告，根本没记录后来是否还在运行，所以对两种结局都必须成立。

- **"ran into a problem" → `出现了问题`** · macOS AppKit `zh-CN` 的固定说法（`…取回服务信息时出现了问题。`
  多处），Finder另有 `发生问题` · `high`。没有采用 `遇到问题`：那是 Windows 蓝屏 "Your PC ran into a
  problem" 的中文说法，按术语原则 2，macOS 优先。
- **"in the background"（问题发生的地方）→ `在后台`** · 沿用队列那一轮定下的 `后台`（Total Commander
  `后台`；Microsoft术语库 `后台的`）。注意仍然**不是** `背景`（那是视觉背景）· `high`
- **"and kept running" → `之后一直在运行`** · `一直在运行` 表持续，`之后` 把它锁在句首 `上次` 已经设好的过去时间框里 ·
  `high`。**别改成 `仍在后台运行`**（`transferProgress.backgroundedToast` 那句）：`仍在`
  是现在时，而这里用户看到对话框时应用早已被他自己退出，那样写就成了假话。
- **第二句 `这是一份报告，里面的详情有助于修复这个问题。`** · 直接取自本 locale `.ended` 的后半句，只删掉 `崩溃`
  两个字 · `high`。英文这里也从 "a crash report" 改成了 "a report"，因为什么都没崩溃；`.ended` 那句保留 `崩溃报告`
  不变。
- **`.unknown` 的中立性靠 `出现了问题`
  本身**（只说“出了问题”，不交代结局），中文在这里不需要额外的时体标记，所以一句话对“退出了”和“还在跑”两种情况都成立·
  `high`
- **`问题` 在一句里出现两次是有意的**（`出现了问题` … `修复这个问题`）：后一个回指前一个，是中文自然的衔接方式，和
  `.ended` 的句式保持一致。指示词用口语的 `这个`，不用 `此`（`style.md`）· `high`

## 崩溃报告设置项的说明现在对两种结局都成立（`settings.updates.crashReports.description`）

后台 panic 之后应用还在跑时，这个开关照样会把报告发出去，所以说明不能再只讲“意外退出”。所有措辞都取自上面的崩溃对话框那一节，换成现在时：

- **`当 Cmdr 意外退出`** 取自 `crashReporter.dialog.body.ended`；**`在后台出现问题`** 取自 `.keptRunning`（去掉表过去的
  `了`）· `high`
- **`一份报告`**，不是 `崩溃报告`：这句话同时覆盖两种结局，和 `.title.report` 的删法一致 · `high`。❌ 标签
  `settings.updates.crashReports.label` 仍是 `发送崩溃报告`，那是这个设置项本身的名字。
- **第二句取自 `crashReporter.dialog.privacyNote`**（`代码中出现问题的部分`），替掉只在真崩溃时才成立的 `崩溃位置` ·
  `high`。顺带把 `App 版本` 统一成 `应用版本`：同一个字段在两个界面上不该有两种写法，而 `App`
  那条术语是给云服务商文案用的。

## Eject / disconnect error copy (`errors.eject.*`)

Toast sentences that land after a colon in `fileExplorer.pane.ejectFailedToast` (`无法推出 {volumeName}：…`) or
`fileExplorer.pane.disconnectFailedToast` (`无法断开连接：…`), so they start mid-sentence and stay one or two short
clauses. macOS Finder zh-CN as Tier 1 (verified against the reference pile, 2026-08-23).

- **eject (verb, in running error copy)** · `推出` · macOS Finder `TL15`/`N199` (`Eject`), `NE31`
  (`你不能推出“^0”，因为它正在使用中。`), `NE66`, `NE79` · `confirmed`. Matches the settled explorer-pass term.
- **removable / not removable** · `可移除` / `不可移除` · macOS Finder `KIND_FORMATTER_28_1` (`Removable` → `可移除`),
  `GV3`/`GV3.1` (`Removable Volumes` → `可移除的宗卷`) · `high`
- **in use / something is still using it** · `还有东西在使用…` · macOS `NE52`
  (`Some files on these disks may be in use. Quit any open applications, and then try again.` →
  `这些磁盘上的一些文件可能正在使用中。请退出所有打开的应用程序，然后重试。`), plus the catalog's own
  `errors.volume.deletePending` (`还有东西占着它`) · `high`. Kept vaguer than Finder's `正在使用中` because the English
  is deliberately unspecific ("Something").
- **unplug (a phone or camera)** · `拔下线缆` · catalog precedent (`errors.provider.macDroid.*`: `拔下再插上 USB 线缆`;
  `errors.listing.deviceReconnecting.suggestion`: `无需拔下设备`) · `high`. `拔下线缆` beats `拔下设备` here because the
  sentence already has `设备` as its subject.
- **drive (the thing being ejected)** · `驱动器` · settings-pass term, reused so all nine strings say the same noun.
  Finder's own eject copy says `磁盘`, but the English source says "drive", and the catalog already writes
  `驱动器断开了` (`errors.write.destinationNotFound.suggestion`) · `high`
- **network share** · `网络共享` · reused from `errors.listing.notSupportedErrno.explanation` / `remotePermissionDenied`
  · `high`

Conventions worth keeping for this family:

- **Don't echo the wrapper's verb back at the user.** `无法断开连接：` already says "couldn't disconnect", so the
  sentence after it uses a different construction (`没法断开它`, `没有连接需要断开`) rather than a second
  `无法断开连接`.
- **`没法` is the friendly stand-in for a second `无法`** in a sentence whose wrapper already spent `无法`. Both are
  neutral; `没法` is the spoken register `style.md` asks for.
- **A timeout is not a failure.** `errors.eject.timedOut` says the drive may still finish on its own
  (`可能过一会儿它会自己推出`), no `失败`/`错误`, matching the English intent.
- **`errors.eject.unexpected` is word-for-word `errors.mutation.unexpected`** (`出了点问题，Cmdr 也说不清是什么。`): the
  English sources are identical, so the Chinese is too.

## 废纸篓提示条：撤销与前往废纸篓（`fileOperations.trash.*` + `commands.fileGoToTrash.*`）

文件被移到废纸篓后立刻出现的提示条，带 `撤销` 和 `前往废纸篓` 两个按钮，外加同名的命令面板命令。复用 `废纸篓`、
`驱动器`、`个文件`、`个项目`。新定的词：

- **put back（从废纸篓放回原来的位置）** · `放回原处`（句中可用 `放回`） · macOS Finder `zh-CN` Tier 1：`N153.1`
  （`Put Back`
  → 「放回原处」）、`PE130_V1`/`PE130_V2`（「“^1”无法放回。」「^0个项目无法放回。」），2026-08-27 在 pile 中核对 ·
  `high`。这就是 Finder 自己对同一动作的菜单项，所以 Tier 1 优先。❌ 不用 `恢复`：目录已经把它留给了改回旧名称的
  `askCmdr.renameUndo.*`，而这里文件是真的移回原位。❌ 不用 Nautilus `zh-CN` 的「从回收站恢复」：它连废纸篓都写成
  `回收站`，不是 macOS 的用词。
- **undo（提示条上的按钮）** · `撤销` · macOS `zh-CN` `ME13`/AppKit（`Undo` → 「撤销」），目录里
  `askCmdr.renameUndo.undo` 也已经是 `撤销` · `high`。与 `回滚`（rollback，传输回滚）分工不同：这里是真正的撤销。
- **go to trash** · `前往废纸篓` · 目录的 `前往` 系列（`commands.navGoToPath.label`、
  `commands.downloadsGoToLatest.label`）和 macOS `zh-CN` 「前往个人文件夹」（`TL_HELP_HOME`） ·
  `high`。中文只有 5 个字，提示条按钮不会挤。
- **"stayed in the trash"** · `{skippedText} {skipped, plural, other {个项目}}仍在废纸篓中` · `仍`
  是目录里表示「还是那样」的词（`仍处于打开状态`、`连接仍然可用`） · `high`。`{skipped}` 是 `{skippedText}`
  的整数搭档；中文没有数的变化，所以只写 `other` 一支，量词短语 `个项目` 对任何数目都成立。
- **"the drive you're browsing"** · `你正在浏览的驱动器` · 目录的 `askCmdr.empty.hint`（「你正在浏览的内容」） ·
  `high`。第二层用 `…上的废纸篓`，避免两个 `的` 直接连用。
- **"This drive doesn't keep a trash."** · `这个驱动器没有废纸篓。` · 与姐妹句
  `fileOperations.delete.archiveWarningStrong`（「压缩文件里没有废纸篓。」）同一句式 ·
  `high`。这是在讲驱动器的事实，不是说用户做错了；按 style.md 的口语指示代词规则用 `这个`，`此驱动器`
  只留给已经定型的驱动器索引短标签。

## 给已发送的错误报告补充备注（`errorReporter.amend.*`、`errorReporter.amendedToast.message`、`errorReporter.autoSentToast.viewOrAddNotes`）

自动发送的错误报告发出去之后，提示条上多了一个按钮，打开一个对话框：里面能看到刚才发出去了什么，也能写备注附到**同一份**报告上（不会再传一次）。如果那份报告已经不能再补充（Cmdr 重启过，或服务器没留入口），对话框改为提示，并把人指向「帮助 > 发送错误报告…」。复用上一轮已经定下的
`错误报告`、`备注`、`参考编号`、`报告包`、`团队`、`忽略`，以及 `common.attachEmail*` 的 `附上你的邮箱`。新定的词：

- **add to (an already-sent report)** · `添加到…`（标题 `添加到你的错误报告`，按钮 `添加到报告`） · macOS Finder `zh-CN`
  的 `N169.13`（`Add to Dock` → 「添加到程序坞」）就是这个句式；`添加`
  本身在 Finder 里到处都是（`RN21`、`IN_A7`、连接服务器窗口的工具提示），2026-08-28 在 pile 中核对 ·
  `high`。英文标题和按钮特意互相呼应（`Add to your error report` / `Add to report`），中文照做，所以两处都用
  `添加到`。❌ 不用 `补充`：它偏书面，且与目录里已有的 `添加备注（可选）`（`errorReporter.dialog.noteLabel`）对不上。
- **adding…（按钮进行时）** · `正在添加…` · 与同一目录的 `正在发送…`、`正在准备预览…` 同一模式 ·
  `high`。省略号是 U+2026 单字符。
- **what was sent（已发生的那次）** · `已发送的内容` · 与发送对话框的 `即将发送的内容`
  （`errorReporter.dialog.detailsToggle`）成对：`即将` 对 `已`，一眼能分出「要发的」和「发过的」 · `high`。
- **view (the report contents)** · `查看` · macOS `zh-CN` 用 `查看` 表示「看内容」（`NE57`「没有权限查看其内容」、
  `TL_HELP_INFO`「查看所选文件和文件夹的信息」、`快速查看`），2026-08-28 在 pile 中核对 · `high`。❌ 不用
  `显示`：那是菜单栏 `View` 的名字（`menu.bar.view` = `显示`），指的是改变视图，不是读内容。
- **"View or add notes to the report"（提示条按钮）** · `查看报告或添加备注` ·
  `high`。英文两半都要保住（看 + 加），中文把宾语拆开成「查看报告」和「添加备注」，9 个字，紧挨着 `更改设置` 也不挤。
- **"can''t take a note any more"** · `这份报告已经无法再添加备注。` · style.md 的中性说法 `无法…`，不用 `错误`/`失败` ·
  `high`。`错误` 只保留在产品功能名 `错误报告` 里。
- **指向帮助菜单** · `请从“帮助”菜单发送一份新报告。`
  · 目录里已有一模一样的说法（`settings.updates.errorReports.description`：「你随时可以从“帮助”菜单手动发送报告」），菜单名
  `帮助` 与 `menu.bar.help` 一致，也是 macOS AppKit `MenuCommands` 的 `Help` → 「帮助」 · `high`。
  **约定**：正文里引用菜单名时，简体用直角外的弯引号 `“…”` 包住菜单名，后面接 `菜单`，不写路径式的 `帮助 > …`
  （路径写法留给 onboarding 里那种加粗的操作指引）。
- **"Note added to your report."** · `备注已添加到你的报告。` · 与 `errorReporter.sentToast.message`
  （`错误报告已发送。你的参考编号是`）同一节奏，句尾不加标点，后面紧跟参考编号徽章 · `high`。
- **"Couldn''t add your note: {error}"** · `无法添加你的备注：{error}` · 与同目录的
  `无法发送错误报告：{error}`、`无法保存报告包：{error}` 同一句式，全角冒号 · `high`。

## 选择/取消选择文件对话框（`selection.*`）

Tier 1 是 macOS Finder `zh-CN`（`MenuBar.json`、`LocalizableMerged.json`，英文一侧读 `en-GB/macOS/Finder/`），Microsoft
`zh-Hans` TBX 补充动词条目，Nautilus/Double Commander `zh-CN` 作旁证。

- **select（动词，从文件列表里挑出项目）** · `选择` · macOS Finder `zh-CN`（`MenuBar.json` `172.title` `Select All` →
  `全选`；`LocalizableMerged.json` `N30` `请选择“^0”`、`SB18` `选择了^0项（共^1项）`）、MS `zh-Hans` TBX（`select` id
  109605 → `选择` id 109623）、NAU `zh-CN`（`Select Items Matching` → `选择匹配的项目`） · `high`。与目录里已有的
  `menu.select.files` / `commands.selectionSelectFiles.label`（`选择文件…`）一致，对话框标题因此和打开它的菜单项对得上。
- **deselect（动词）** · `取消选择` · MS `zh-Hans` TBX（`deselect` id 44722 → `取消选择` id 2612168）、macOS Finder
  `zh-CN`（`MenuBar.json` `300488.title` `Deselect All` → `取消全选`；`LocalizableMerged.json` `NE18`
  `取消选择“锁定”`）、DC `zh-CN`（`Unselect a Group…` → `取消选择一组文件`） · `high`。
- ⚠️ **繁简在这里是真正的用词分歧，不是字形转换。** 同一条 Microsoft 词条（id 44722）繁体给的是
  `取消選取`，因为繁体的 select 是 `選取` 而不是 `選擇`；Apple 的繁体也把 `選擇` 留给「choose 一个东西」，`選取`
  才是「从列表里挑项目」。两边各自按自己的 macOS 源翻，永远不要互转。繁体一侧见
  `../zh-Hant/terms.json`（`select`、`deselect`）。
- **"Select these files" / "Deselect these files"（对话框底部主按钮）** · `选择这些文件` / `取消选择这些文件`
  ·在上面两个动词上直接构词 · `high`。
- **"… in the focused pane"（按钮的悬停提示）** · 处所状语提到句首：`在焦点窗格中选择这些文件` /
  `在焦点窗格中取消选择这些文件` · `high`。**提示语是独立的一句，不必以按钮文字开头**：按钮的无障碍名取自 `…label`
  键（`QueryDialog.svelte` 用 `primaryAction.ariaLabel ?? primaryAction.label`），提示只是 `use:tooltip`
  的悬停文案，WCAG 2.5.3已由构造保证。英文那句把范围放在句尾是英文的语序，中文照搬会别扭，所以按中文语序把 `在…中`
  提到动词前面。 `焦点窗格`
  是目录里已经定下的说法（`commands.navGoToPath.description`、`commands.favoritesAdd.description`）。
- **"Press Enter to filter"** · `按 Enter 键筛选` · 沿用本文件「pressing Enter / the Enter key」条目定下的
  `按 Enter 键`（目录里另有 9 处这么写）加上 `筛选`（`queryUi.recent.filterPlaceholder` `筛选最近的搜索`） ·
  `high`。孪生键 `search.runHint` 与 `queryUi.bar.runHint` 同样是 `按 Enter 键搜索`，整份目录里 `回车键` 一次都不出现。
- **"recent selections"（最近用过的查询弹窗）** · `最近的选择` · 上面的动词当名词用，与 `queryUi.recent.*` 的孪生键
  `最近的搜索` 完全对仗 · `high`。五个弹窗键逐字照搬孪生键，只把 `搜索` 换成 `选择`。
- **"Matching what is shown in the list (the full path)."** · `匹配的是列表中显示的内容（完整路径）。` · `匹配`
  是目录的 match 动词（`queryUi.scope.toggle.caseSensitiveAria` `区分大小写匹配`、
  `commands.selectionSelectFiles.description` `将匹配的文件加入选择`），`列表` 与 `完整路径` 也都是现成的说法 · `high`。
- **"Apply recent {mode} selection: {query}"** · `应用最近的 {mode} 选择：{query}` · `应用` =
  Apply（`ai.local.applyContextSize`）；全角冒号与 `queryUi.recent.scopeSummary`（`范围：{scope}`）一致；`{mode}`
  两侧加空格，因为它可能是拉丁文的 `AI` · `high`。`{query}` 是不可控的用户输入，放在冒号后的句尾，落什么进来都读得通。

## 术语漂移审计：同一英文串的多种译法

`desktop-i18n-term-consistency` 把 `zh`
报出 28 处「同一条英文、两种中文」。逐条查证后：15 处是真漂移，已收敛；13 处是**真正的语义分界**，两种译法各自正确，故意保留。分界必须写成规则，否则下一轮翻译会「修」回去。

### 收敛掉的真漂移

- **Dismiss** · `关闭` · macOS `zh-CN` 把 `Dismiss Popover` 译作 `关闭弹出窗口`；同一份语料里 `忽略`
  专门留给真正的「ignore」（`Ignore Spelling` → `忽略拼写`、`Ignored` → `已忽略`、`ignores ownership` →
  `忽略所有权`）（macOS 26.6.2 语料，2026-08-30 核对） · `confirmed`。目录里原有 5 个键写成 `忽略`，其中
  `errorReporter.sentToast.dismiss`
  是**报告发送成功**后的提示，按钮却写着「忽略」，等于让用户「无视」自己刚做成的事。九个 Dismiss 全部统一为 `关闭`。
  - 连带：`queue.row.dismissAria` 原为 `忽略这项操作`，改为 `关闭这项操作的记录`。**不要写成 `关闭这项操作`**：中文的
    `关闭+操作` 会被读成「终止这项操作」，而这个按钮只是让那一行不再显示，什么都不撤销、不重试、不删除。
- **Example:（占位符示例）** · `示例：` · 完整词 Example 用 `示例`（GNOME Nautilus `Examples:` →
  `示例:`），缩写 e.g. 才用 `例如`（KDE Dolphin `(e.g. smb://…)` → `(例如： smb://…)`） ·
  `high`。`onboarding.cloudSetup.*` 的 4 个键原写 `例如：`，与 `ai.cloud.*` 的 `示例：` 打架；英文两处都是完整词
  `Example:`，故统一为 `示例：`。
- **On disk（占用磁盘的物理大小）** · `占用磁盘` · Double Commander `zh-CN` `Size on disk:` → `占用磁盘空间`；
  `占用空间` 太笼统，逻辑大小也是「占空间」 · `high`。`settings.listing.sizeDisplay.opt.physical` 原为 `占用空间`，与
  `fileExplorer.dirSize.onDiskLabel` / `selectionTooltip.onDiskHeader` / `mismatchTooltipPrefix` 的 `占用磁盘`
  不一致；两处的对立面都是 `内容`（Content），同一组对立不该有两个名字。
- **From（传输的来源）** · `来源` · 目录已经用 `来源` 指代传输源（`transferDialog.scanStopped` `没能统计完来源`、
  `scanUnresponsive` `来源没有响应`、`sourceGroupTitle` `来源`／`targetGroupTitle` `目标`） · `high`。
  `fileOperations.scanPhase.fromLabel` 原为 `来自：`，是唯一的例外，改为 `来源：`。
- **Go to home folder** · `前往个人文件夹` · `fileExplorer.errorPane.goHome` 原为
  `打开个人文件夹`；这个按钮是导航，不是「打开」，且 `commands.navGoHome.label` 已定 `前往`（macOS 的 `Go` 菜单即
  `前往`） · `high`。
- **Go to latest download** · `前往最新下载` · 去掉 `settings.behavior…globalGoToLatestShortcut.enabled.label` 多出的
  `的`，与 `commands.downloadsGoToLatest.label`／`menu.go.goToLatestDownload` 对齐 · `high`。
- **Tab limit reached** · `已达到标签页数量上限` · `commands.handler.tabLimitReached` 原写 `已达`，与
  `fileExplorer.tabs.limitReached` 的 `已达到` 不一致；取更完整的 `已达到` · `high`。
- **Press Enter to search** · `按 Enter 键搜索` · 见本文件「pressing Enter / the Enter key」条目 · `confirmed`。
  `search.runHint` 原为 `按回车键搜索`（把键名译成了中文），`queryUi.bar.runHint` 原为 `按 Enter 搜索`（少了
  `键`）。两处都改成本文件早已定下的 `按 Enter 键`＋动词，`回车键` 从目录里彻底消失。
- **计数名词要带量词** · `个文件` / `个目录` · `fileExplorer.summary.fileNoun`／`dirNoun` 原为光秃的
  `文件`／`目录`，拼出来是「3 / 10 文件」——中文数词后面必须有量词，这不只是不一致，是不合语法 ·
  `confirmed`。目录其余每一处计数都写
  `个文件`／`个目录`（`transferDialog.filesPart`、`dirSize.fileCount`、`fileOperations.shared.fileRate` `个文件/秒`）。
- **两条重复的散文** · `onboarding.stepBeta.signup.success` 与 `settings.updates.emailConfirmHint` 是同一句英文，统一为
  `请查看收件箱，确认你的邮箱。谢谢你的帮助！`；`onboarding.stepBeta.signup.failure` 与
  `settings.updates.emailSignupError` 统一为 `抱歉，我们现在没能帮你注册。要再试一次吗？`（`没能` 比 `无法`
  更软，合乎风格指南「不用响亮的失败词」）。

### 故意保留的分界（英文一词多义，中文必须分开）

英文用一个词兼了两份差事，中文合并反而会错。每条都写清界线在哪：

- **App** · 作为「作用域／范围」时 `应用`（`shortcuts.scope.app`，兄弟项全是中文范围名）；作为**与 `macOS` 并列的来源**
  时保留拉丁 `App`（`settings.appearance.dateColors.opt.app`、`…downloadsNotifications.opt.inApp` `App 内`，同组还有
  `macOS`）· `high`。界线：`App` 与 `macOS` 对举时保留原形，单独当范围词时译。
- **Back** · 导航返回 `返回`（macOS `Back`／`Go Back`／`go back` 一律 `返回`，Tier 1）；**向导的上一步** `上一步`
  （`onboarding.wizard.back`，与 `onboarding.wizard.next` `下一步` 成对）·
  `confirmed`。界线：回到上一个位置 vs 回到上一个步骤。
- **Both** · 单独的开关格 `两者`（`queryUi.filters.type.both`，兄弟项 `文件`／`文件夹`；macOS `Keep Both` →
  `保留两者`）；与 `都不用` 成对的选项写 `两者都用`（`…downloadsNotifications.opt.both`／`.neither`）·
  `high`。界线：光杆名词 vs 必须与否定项对仗的动宾短语。
- **Done** · 清单步骤读屏时念的那一声 `完成`（`indexing.step.statusDone`；macOS SystemSettings `Done` →
  `完成`）；操作的生命周期状态 `已完成`（`operationLog.status.done`／`.outcome.done`）·
  `high`。界线：一声宣告 vs 一个状态值。
- **Error** · 面向用户的状态格 `出现问题`（`fileExplorer.network.browser.status.error`；英文 `@key`
  自己就写了「风格指南若有更友好的说法就别用 error 的字面词」，而本语言风格指南正是这么规定的）·
  `confirmed`。目录里已经没有 `错误` 这样的诊断前缀：检查更新没成功时，说的是完整的句子（`updates.failure.check`）。
- **Modified** · 文件的修改日期 `修改日期`（macOS Finder `ArrangeByMenu` `Modified` → `修改日期`，Tier 1）；
  **快捷键被用户改过** `已修改`（`shortcuts.section.filterModified`，兄弟项 `shortcuts.section.modifiedTooltip`
  `已从默认值更改`）· `confirmed`。这里英文的 `Modified` 根本不是日期，套 `修改日期` 会彻底错。
- **Put back** · 从废纸篓放回原处 `放回原处`（macOS Finder `Put Back` → `放回原处`，Tier
  1；`fileOperations.trash.undone`）； **撤销重命名**后把旧名字还原 `已恢复`（`askCmdr.renameUndo.undone`／`.partial`）·
  `confirmed`。界线：`放回原处` 明说「回到原来的位置」，而重命名撤销根本没动位置，照搬会撒谎。
- **Regex** · 局促的模式芯片用简称 `正则`（`queryUi.mode.regex.label`、`queryUi.ai.patternLabel.regex`、
  `queryUi.recent.mode.regex`）；**悬停提示与无障碍名**写全称 `正则表达式`（`viewer.search.regex`，该按钮的可见文字只是
  `.*` 字形，读屏用户需要完整术语）· `high`。全称有 Tier 3 全票支持（KDE `Regular Expression` → `正则表达式`、Double
  Commander、Xfce）。界线：地方紧就缩，读屏和提示就展开。
- **Running** · 本地 AI 服务器进程在跑 `运行中`（`ai.local.statusRunning`）；一项任务在进行 `进行中`
  （`operationLog.status.running`）· `high`。检查脚本自己的注释就把这一对列为正当分歧。
- **Scanning** · 带省略号的进行时 `正在扫描…`（`fileOperations.shared.scanningTooltip`）；两步指示器里的**步骤名**
  `扫描`（`fileOperations.transferProgress.stageScanning`）· `confirmed`。macOS AppKit 做的正是同一个区分： `Searching`
  → `搜索`，`Searching…` → `正在搜索…`。目录其余每处进行时都写 `正在扫描`。
- **Select** · 动词／菜单标题 `选择`（`menu.bar.select`、`onboarding.stepAi.table.rowSelect`）；下拉框未选时的占位符
  `请选择`（`ui.select.placeholder`，英文是 `Select...`）· `high`。界线：命令用户去做 vs 提示用户还没做。
- **Unreachable** · 主机连不上
  `无法连接`（`fileExplorer.network.browser.status.unreachable`）；标签页指向的文件夹／宗卷够不着
  `无法访问`（`fileExplorer.tabBar.unreachableAriaLabel`）·
  `high`。界线：连接的对象是服务器，访问的对象是路径；宗卷已推出时说「无法连接」是错的，压根没有连接可言。
- **View** · 动词，用内置查看器打开 `查看`（`menu.file.view`、`commands.fileView.label`、
  `fileExplorer.functionKeyBar.viewLabel`）；名词，菜单栏的「显示」菜单 `显示`（`menu.bar.view`）· `confirmed`。已记在
  `style.md`，此处只做交叉引用。

### 复核这批时的坑

- 改了某个可见标签，**必须同时看它的 `*Aria` 兄弟键**。把 `queue.row.dismiss` 从 `忽略` 改成 `关闭` 时，
  `queue.row.dismissAria` 仍写着 `忽略这项操作`，`desktop-i18n-aria-label` 立刻报 WCAG 2.5.3 不达标。
- 占位符示例键的英文自带尾部 `...`（`Example: sk-abc123...`），改写译文时别把它抹掉。

## 术语漂移审计：英文不同、中文该同的那一半

`desktop-i18n-term-consistency`
只看得见**英文完全相同**的键。英文稍有出入的漂移它一概看不到，而这一半往往更难看：菜单栏和命令面板本来就用不同的英文措辞指同一个动作。按
`docs/guides/i18n-translation.md` 的三趟脚本排查后：

### 收敛掉的（脚本看不见，但用户看得见）

- **Go back / Back（历史导航）** · `返回` · macOS `zh-CN` 把 `Back`、`Go Back`、`go back` 一律译作 `返回`，Finder更是把
  `Back/Forward` 直接给成 `返回/前进`；整份 macOS 语料里 `后退` 出现 **0 次**（macOS 26.6.2 语料，2026-08-30 核对） ·
  `confirmed`。`commands.navBack.label`（命令面板）与 `fileExplorer.errorPane.goBack` 原写 `后退`，而
  `menu.go.back`（菜单栏）写 `返回` —— **同一个动作，菜单栏和命令面板各叫各的**。兄弟键 `commands.navForward.label`
  早就是 `前进`，本来就该配 `返回`。
- **Dismiss 的两个漏网键** · `queue.toolbar.dismissAll` `全部忽略` → `全部关闭`，`ui.toast.dismissAria` `忽略通知` →
  `关闭通知`。英文分别是 `Dismiss all` 和 `Dismiss notification`，与 `Dismiss` 不是同一条串，所以脚本报不出来。
- **Example: 的两个漏网键** · `fileOperations.mkdir.placeholder`／`mkfile.placeholder` 的 `例如：` → `示例：`。
- **copying** · `拷贝` · `askCmdr.decision.verbCopy` 原写 `复制`，而目录里 43 处 copy 都是 `拷贝`，`复制` 是留给
  **duplicate** 的（`commands.fileDuplicate.label`、`menu.file.duplicate`） · `confirmed`。这个词会落进 Ask
  Cmdr 的批准／拒绝句里（「要…这些文件吗」），在一句确认提示里把 copy 说成 `复制`
  正好撞上「制作副本」那个命令。同组其余六个动词（`移动`、`删除`、`重命名`、`压缩`、`解压`、`移到废纸篓`）本来就都跟目录一致，只有它跑偏。
- **archive（压缩包，名词）** · `压缩文件` · `fileOperations.transferDialog.pathErrorNotZip` 原写
  `归档名称`，与词汇表既定的 `压缩文件` 不一致 · `high`。注意 askCmdr 的 `存档`／`已存档`
  是**另一个义项**（把聊天收起来），不动。
- **click** · `点按` · 风格指南早就定了（macOS `zh-CN` 全用 `点按`，`Click Calculate to show` → `点按“计算”以显示`
  等多处，`点击` 0 次），并留了「其余的顺手收敛」的话。这一趟把 8 个键的 `点击` 收敛为 `点按`，`点击`
  作为单独的词从目录里消失。

### 顺手修掉的一处英文残留

- `settings.fileViewer.suppressBinaryWarning.description` 里写着 `Cmdr''s 文件查看器` —— 中文句子里夹了个英文所有格
  `'s`。改为 `Cmdr 的文件查看器`。

### 有意不动的（有证据支持的边界）

- **`双击` 与 `右键点击` 不跟着 `点按` 走。** 单独的 click 用 Apple 的 `点按`（Tier 1）；但复合词 double-click /
  right-click 在 macOS 语料里查无实据，而 Tier 3 四家（GNOME、Xfce、KDE、Double Commander）**一致**写 `双击`、
  `右键点击`。没有 Tier 1 反证就不要动它们，尤其别凭印象改成 `连按`／`右键点按`。 · `high`。后来有五个键又写成了
  `右键点按`（`errors.listing.noPermissionErrno.suggestion`、`errors.listing.permissionDenied.suggestion`、
  `errors.listing.diskFullErrno.suggestion`、`errors.listing.storageFull.suggestion`、`servers.pinHint.body`），已收回
  `右键点击`。
- **`Queued` = `等待中`**（`operationLog.status.queued`），不写 `已排队`。它跟 `进行中`／`已完成`
  是同一组状态值，跟队列这个名词（`队列`）不必同形。 · `high`

### 排查结论：这几对近义词目前是干净的

`copy 拷贝` / `duplicate 复制`、`undo 撤销` / `roll back 回滚`、`tab 标签页` / `tag 标签`、 `API key 密钥` /
`license key 许可证密钥`、`remove 移除` / `delete 删除`、`folder 文件夹` / `dir 目录`、 `cancel 取消` /
`stop 停止`、`open 打开` / `go to 前往` —— 全目录逐键对照，除上面那条 `verbCopy`
外没有互串。下次复审可以从这份清单接着往下走。

## Shared `en` fixes: menu wording, System Settings tokens, name-restore verb

Fallout from four `en` self-inconsistency fixes. Evidence is macOS 26.6.2 (build 25G83), read live off the installed
bundles with the `.loctable` / `MenuBar.strings` recipes in `docs/i18n/reference-pile/how-to-mine.md`, 2026-08-30, plus
`zh-Hans/microsoft-terminology/CHINESE (SIMPLIFIED).tbx` from the pile.

- **`Show all` / `Hide others` (app menu) → `全部显示` / `隐藏其他`** · Tier 1, three independent bundles agree: Finder
  `MenuBar.strings` `300730.title`/`300729.title`, TextEdit `Edit.loctable` `517.title`/`515.title`, Preview
  `MainMenu.loctable` `150.title`/`145.title`. Both already shipped and both already match `commands.appShowAll.label` /
  `commands.appHideOthers.label`, so the `en` sentence-case fix was a restamp: Chinese has no capitalization, and the
  wording was right. · `confirmed`
- **System Settings panes via tokens, now in the git and provider errors too** · the eight `errors.git.*` /
  `errors.provider.*` suggestions carry `{system_settings}` / `{privacy_and_security}` / `{files_and_folders}`, so the
  literals `系统设置` / `隐私与安全性` / `文件和文件夹` are gone from them. (This supersedes the "the git-suggestion
  strings use plain literals" carve-out in the settings-pass entry above.) Spacing: the runtime value can arrive CJK or
  Latin, so keep a space on both sides of a bare token (`在 {system_settings} 里`), and inside a bold path keep the
  existing shape `在 **{system_settings} > 通用 > 登录项与扩展**里` — the trailing 里 attaches to the last CJK pane
  name, never to the token. · `high`
- **Pane names the tokens don't cover** · `Apple Account` → `Apple 账户` (`ClassKitSettings.loctable` `APPLE_ID` says
  `Apple账户`; Cmdr adds the Latin/CJK space per style.md), `General` → `通用`, `Login Items & Extensions` →
  `登录项与扩展` (`LoginItems.appex/Localizable.loctable`). All three were already correct. · `confirmed`
- **`settings.indexing.enabled.description`: `目录大小` → `文件夹大小`** · English switched "directory sizes" → "folder
  sizes" because `folder` is the app's user-facing word. The glossary already splits `folder 文件夹` / `dir 目录` (see
  the near-synonym sweep at the end of this file); this string is user-facing help text, so it takes `文件夹`. · `high`
- **"Put the old names back on N files" → `已恢复 {countText} 个文件原来的名称。`** (`askCmdr.renameUndo.undone` /
  `.partial`) · English used to share one sentence with `fileOperations.trash.undone` and now names the OBJECT (the old
  name). The old Chinese (`已恢复 … 个文件。`) said the FILES were restored, which is the trash action, not this one:
  nothing moves here, only the name changes back. Reuses `原来的名称` from the family's own `undoing`
  ("正在恢复原来的名称…") and `skipReason.*`. `fileOperations.trash.undone` keeps `已将 … 放回原处。` · `high`
- **Email placeholder stays `you@example.com`** (`settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`,
  `onboarding.stepBeta.emailPlaceholder`) · Microsoft Simplified Chinese keeps the sample address verbatim: in
  `CHINESE (SIMPLIFIED).tbx` the `en-US` term `someone@example.com` maps to a `zh-Hans` term that is the same literal
  string (`user@example.com` likewise). Compare Vietnamese, where the same source DOES localize the local part. So a
  Latin-script local part is the Chinese convention, all three keys already agree, and the existing
  `sameAsSourceJustification` stands. `example.com` is RFC 2606's reserved domain. · `high`

## 完成中断的回滚（`operationLog.dialog.finishRollBack`、`operationLog.rollback.partiallyRolledBackNotice`、`fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`、`queue.row.reversalInFolder`）

操作日志多了一种状态：回滚做到一半被取消，那一行就变成“已部分回滚”，按钮从“回滚”换成“完成回滚”。这一批五条值全部锚在目录已有的回滚词汇上，没有新造词。

- **"Finish rolling back" → `完成回滚`** · 沿用目录里定好的 `回滚`（`operationLog.dialog.rollBack` =
  `回滚`、`rollingBack` = `正在回滚`、`partiallyRolledBack` = `已部分回滚`，见本文件 `roll back (reverse an operation)`
  条）· `medium-high`。`完成` 说的是“把这一次回滚做完”，不会读成“重新回滚一次”；和同一行的徽章 `已部分回滚`
  连起来看，意思很清楚。备选 `继续回滚` 在“不是新开一次”这一点上更直白，但英文写的是 Finish 而不是 Continue，而且 `继续`
  已经给了 `queue.row.resume`，再用一次会撞车（待母语复核，见 `review-queue.md`）。
- ⚠️ **`operationLog.dialog.finishRollBack` 和 `fileOperations.rollbackConfirm.finishRollBack` 必须逐字相同**（都写
  `完成回滚`）· 同一个英文串、同一个动作，一个是日志行上的按钮，一个是它打开的确认框上的按钮；同一 locale 里同串异译会被
  `i18n-terms` 报警。改动其中一条就要同时改另一条。
- **"Finish rolling this back?" → `要完成这项操作的回滚吗？`** · 与孪生键
  `fileOperations.rollbackConfirm.title`（`要回滚这项操作吗？`）同一个 `要…吗？` 句式、同一个 `这项操作` · `high`。用
  `完成…的回滚` 这个框架，和确认按钮 `完成回滚` 对得上。
- **提示行 `partiallyRolledBackNotice` 逐字复用两条现成说法** ·
  `Cmdr 能回滚的都回滚了，其余的保持原样。完成回滚会再走一遍，仍然会跳过没有把握的部分。` · `保持原样` 直接取自
  `fileOperations.rollbackConfirm.leaveAsIs`；`跳过没有把握的部分` 逐字取自
  `fileOperations.rollbackConfirm.bodyUndoByDeleting`（`Cmdr 会跳过没有把握的部分，所以可能会剩下一些。`）·
  `high`。`再走一遍` 对应 "takes another
  pass"。这句有意不承诺“全都能回滚回来”：跟记录对不上的文件还是会被跳过。全句不出现“错误”“失败”。
- **"in {folder}" → `位于 {folder}`** ·
  `high`。这是**后置的处所短语**：`queue.row.reversalDeleting`（`正在删除新建的内容`）和这一条是并排渲染的两个独立元素，顺序固定，所以中文常规的“在…中 + 动词”语序在这里用不上。`位于`
  正是中文用来把地点接在后面的说法，而且有两处现成依据：目录里同一个英文串
  `in {subdir}`（`downloads.toast.inSubdir`）已经写成 `位于 {subdir}`；KDE Dolphin `zh-CN` 把 `in location %1` 译成
  `位于 %1`，也是当尾巴接在别的片段后面（`%1 个选中的项目，网格布局，位于 %2`）。整行读作
  `正在删除新建的内容 位于 Backup`，`位于`
  把文件夹坐实成“地点”，不会再被读成“这个文件夹要被删了”——那正是这个键要修的毛病。
- **文件夹名不加引号**（写 `位于 {folder}`，不写 `位于“{folder}”`）· 与同串的 `downloads.toast.inSubdir`
  一致，也与队列行里别的行一样：那一格平时就只放一个裸的文件夹名 · `high`。⚠️ 这一点**和 `de`、`es`
  有意不同**，那两个 locale 写的是 `in „{folder}“` / `en “{folder}”`。以后若决定各 locale 统一加引号，简体按 style
  guide 用 `“…”`。

## 回滚结束后的提示条 (`fileOperations.cancelRollback.*`、`fileOperations.rollbackConfirm.body`)

覆盖 `fileOperations.cancelRollback.*` 与改写后的 `fileOperations.rollbackConfirm.body`（2026-08-31）。

用户在拷贝／移动进行中按了「回滚」，撤销跑完后弹出的提示条：一句标题 + `leftBehind` 铺垫 + 一串 `reason.*`
项目符号。全批的调子是「Cmdr 做了稳妥的处理」，不道歉、不报警。词汇全部锚在目录已有的回滚家族和
`askCmdr.renameUndo.skipReason.*` 上，没有新造词。

- **`reason.*` 整组照搬 `askCmdr.renameUndo.skipReason.*` 的句式 `保留了 X：原因。`**
  ·两个家族是同一个东西的两次实现（撤销重命名 / 撤销传输），英文的句式也一模一样，中文跟着走，读者一眼就认出是同一类清单 ·
  `high`。`{name}` **不加引号**，与孪生家族一致（目录别处的 `“{name}”` 用在散文句子里，这里是项目符号清单）。
- ⚠️ **`reason.folderNotEmpty.named` / `.counted` 必须与 `askCmdr.renameUndo.skipReason.folderNotEmpty.*` 逐字相同**
  （`保留了文件夹 {name}：里面现在有东西了。` /
  `保留了 {countText} {count, plural, other {个文件夹}}：里面现在有东西了。`）·这两条的**英文原串完全相同**，`desktop-i18n-term-consistency`
  会把同串异译报成分歧，而 `zh` 现在只有 `notYetReviewed` 计数（只降不升）·
  `confirmed`。改一条就要同时改另一条。其余几条英文的撇号写法不同（`’` vs
  `''`），归一化后不同组，所以不受这条约束，但仍然照抄了同一句式。
- **「item」→ `个项目`**（不是
  `个文件`）· 撤销会连同新建的文件夹一起删，所以这一批数的是「项目」；孪生的 renameUndo 只动文件，才写 `个文件` ·
  `high`。沿用目录里 `{countText} {count, plural, other {个项目}}`
  的量词写法（`fileOperations.trash.undonePartial`、`fileOperations.delete.overflowMore`）。
- **「remove / delete the files it wrote」→ `删除`，不是 `移除`** · 整个回滚家族已经定死了 `删除`
  （`rollbackConfirm.bodyUndoByDeleting`「这会删除这项操作创建的文件和文件夹」、`queue.row.reversalDeleting`
  「正在删除新建的内容」、`transferProgress.rollbackTooltip`）· `confirmed`。`移除`
  留给「从列表／压缩包里拿掉」（`fileOperations.delete.archiveWarningRest`）。
- **「Put … back」→ `放回原处`** · 按 `fileOperations.trash.undone`（`已将 … 放回原处。`）走，macOS Finder `Put Back` →
  `放回原处` 是 Tier 1，而且英文这里和废纸篓提示条用的是同一个动词 · `high`。
- **移动回滚的整个家族都用 `放回`，不用 `挪回`。** 队列行 `正在把文件放回原处`（`queue.row.reversalMovingBack`、
  `fileOperations.transferProgress.titleReversalMovingBack`）、确认框 `这会把文件放回原来的位置`
  （`rollbackConfirm.bodyUndoByMovingBack`、`.bodyStopAndMoveBack`）和这批提示条说的是同一个动作，英文三处也都是 "put/move
  back"，所以检查抓不到分歧；`放回原处` 是 Finder 的 Tier 1 词，而且明说「回到原来的位置」· `high`。
- **「the …」（doneDeleting／doneMovingBack 里那个定冠词）→ `全部`** ·英文靠 `the` 把「干净收场」和只报部分的
  `someDeleted`／`someMovedBack` 分开，中文没有冠词，用 `全部` 扛这个对比：`已删除 Cmdr 写入的全部 …` vs `已删除 …` ·
  `high`。⚠️ 别给 `some*` 那两条加 `全部`，它们后面紧跟着 `leftBehind`，说「全部」就是撒谎。
- **「The rest are still there」→ `其余的都还在。`**
  · 有意不点地点：这一条同时服务拷贝和压缩，说「目标位置」会把压缩包的情形讲拧；`都还在` 已经把「没被删掉」说清楚了 ·
  `high`。macOS Finder 的 `剩下的项目`（「你要跳过它们并拷贝剩下的项目吗？」）是「the rest」的 Tier
  1 依据，这里取了目录自己的 `其余的`（`operationLog.rollback.partiallyRolledBackNotice`
  「其余的保持原样」）以保持家族一致。
- **「The rest stayed where the move put them」→ `其余的还留在这次移动把它们放到的地方。`**
  ·这一条必须点地点（移动的目的地），否则和「回到原处」混淆；`这次移动` 取自
  `operationLog.rollback.refusalDirectoryMerge`（「这次移动把文件夹并入了…」）· `high`
- **「Stopped after …ing N items」→ `…N 个项目后停止了。`**
  · 中文把从句放前面是常规语序，英文的 "Stopped" 前置只是英文的重心习惯；`停止` 是回滚家族已定的词（macOS Finder `PE107`
  = `停止`，见本文件回滚确认框一节）· `high`
- **`leftBehind` 逐字复用 `跳过没有把握的部分`** · `Cmdr 会跳过没有把握的部分，所以这些都保持了原样：` ·前半句取自
  `rollbackConfirm.bodyUndoByDeleting`，后半句的 `保持原样` 取自 `rollbackConfirm.leaveAsIs` ·
  `confirmed`。这句的作用是**先给期待再列原因**，所以承诺必须和确认框一字不差，否则用户会觉得是两回事。结尾用全角冒号
  `：`，因为下面接的是项目符号清单。
- **「something else now sits where it came from」→ `它原来的位置现在被别的东西占用了。`** · `已被占用` 是 macOS Finder
  Tier 1（`名称“^0”已被占用，请选取其他名称。`），`位置` 也是 Finder 的词（`此位置是只读的。`）· `high`。与孪生的
  `renameUndo.skipReason.nameTaken`（`它原来的名称已被占用。`）形成 `名称`／`位置` 的对照，正是两个家族的差别所在。保留
  `别的东西` 是为了跟英文一样具体、口语。
- **「Couldn't undo {name}」→ `Cmdr 没能撤销 {name} 的改动。`** · 这一条**有意跳出** `保留了 X：`
  的句式，因为它不是 Cmdr 的主动选择，而是驱动器不给写 · `high`。`撤销` 而不是 `回滚`：英文写的是 undo，而且 `回滚`
  在目录里是**整项操作**的动作，安到单个文件上不通（见本文件近义词排查 `undo 撤销` / `roll back 回滚`）。加 `的改动`
  是因为「撤销一个文件」在中文里不成话。`没能` 而不是 `无法`：英文暗示重试可能成功，`没能` 说的是这一次没做成，目录里
  `fileOperations.archivePassword.retryMessage`（`这个密码没能解锁 …`）已经是这个用法。全句不出现「错误」「失败」。
- **「Its drive may be disconnected or read-only」→ `它所在的驱动器可能未连接，或者是只读的。`** ·
  `它们所在的驱动器未连接` 逐字取自 `fileOperations.trash.undoUnavailable`；`只读` 是 macOS
  Finder 的词（`此位置是只读的。`）· `high`
- **`rollbackConfirm.body` 重译**（英文加了第三句，并改口称 `Cmdr`）· 前两句保留原有译文，第三句逐字接上
  `bodyUndoByDeleting` 的 `Cmdr 会跳过没有把握的部分，所以可能会剩下一些。`
  —— 英文那一句在两个键里**完全相同**，中文也就必须相同 · `confirmed`

### `cancelRollback.stagedLeftover.*`（Cmdr 自己留在目标位置的残留）

2026-09-02 新增。两条文案，说的是 Cmdr 自己建的工作文件没能从目标位置清掉。它们**不属于** `reason.*`
列表：那边是 Cmdr 在保护用户的文件，这边是 Cmdr 自己的残留。

- **`unfinished copy` → `不完整副本`** · `不完整` 是 Apple 对 "incomplete" 的译法（macOS
  `LA33`：「已损坏或不完整」），`副本` 是 `NE111` 里的名词（「保留可恢复的副本」）· `high`
- **`at the destination` → `目标位置`** · 目录里已在用的词（`conflictsUnknown`、`stallWaitingDestination`）· `high`
- **`transfer`（名词）→ `传输`**
  · 目录已这样说（`errors.listing.deviceReconnecting.explanation`：「传输被取消或中断之后」）· `high`
- 第二句用 `清掉` 而不是 `删除`：这是 Cmdr 自己的工作文件，不是用户的文件。
- ⚠️ **写 `之后往那里传输时`，❌ 绝不写「下次」。**
  Cmdr 的清理会跳过不满一小时的文件，所以马上重试并不会清掉它。给一个兑现不了的承诺，正是这条文案要消除的毛病。

## WebKit 过旧时的拦截页（`main.oldWebkit.*`）

三条文案，在 Mac 的 Safari 过旧时代替 Cmdr 的界面显示。它们写在 HTML 外壳里而不是应用里，所以这是那位用户能看到的 Cmdr 的全部内容。

- **`Software Update` → `软件更新`** · macOS 系统设置中该面板的名称；Finder 的 Tier
  1 证据佐证了这个词（`Apple Device Software Update File` → `Apple设备软件更新文件`）· `high`。
- **`Quit` → `退出`** · macOS AppKit 的 `Quit` 键 → `退出` · `high`。此前不在词汇表里，现补上。
- **`Safari`、`Mac`、`15.4` 保持原样**，两侧按 § 间距规则加空格。`Safari` 已加入 `BRAND_WORDS`。
- 面板名用直角引号之外的全角引号 `“软件更新”`，与目录里其余简体文案一致。

## 旧版 macOS 提示（`main.oldMacos.*`）

低于 macOS
12 的 Mac 上只出现一次的对话框：Cmdr 能跑，但超出了测试范围。语气坦率轻松，既不是道歉也不是警告，因为应用确实在运行。

- **`supported` → `支持`** · macOS Finder（`无法完成此操作，因为不支持此操作。`）· `high`。
- **`X and up` → `X 及更高版本`** · macOS SystemSettings（`需要OS X %@或更高版本。`）·
  `high`。Apple 写得紧凑，我们在拉丁字符两侧加空格 (`style.md` § Numerals, punctuation, and spacing)。
- **`best effort` → `尽力而为`** · pile 里没有对应词条（只有网络 QoS 的定义），但这是中文里现成的说法 · `high`。
- **`look off` → `不太对`** · 口语，且避开了语气规则禁止的「错误」「失败」。
- **最后一句是 David 的第一人称**，仍用 `你`，与 `onboarding.stepBeta.greeting` 一致。

## Ask Cmdr inspect-file consent + tool labels (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

macOS zh-CN Tier 1 (Finder/AppKit pile + live Preview.app and Photos.app `zh_CN` loctables, macOS 26, `plutil`),
Microsoft zh-Hans TBX Tier 2, Nautilus/Thunar/Dolphin/TC/DC zh-CN Tier 3. Reuses settled `压缩文件`, `文本`, `照片`,
`标签`, `提供方`, `查看`, and the old `askCmdr.consent.noContents` sentences where they still hold.

- **look inside files (the inspect tool line, doing/done)** · `正在查看文件内容` / `已查看文件内容` · `查看` = "look at
  the contents" (settled; macOS `zh-CN` `NE57`, and the sibling `askCmdr.tool.appState.*` `正在查看…`); `文件内容` names
  what the tool reads. Chinese has no number, so the plural-neutral English needs nothing extra. Same `正在…` / `已…`
  shape and length class as `searchPhotos.*` / `imageFacts.*` / `listDir.*` · `high`
- **look inside a file (prose, the retired what's-new text)** · `查看你问到的文件里的内容` · same verb as the tool line
  so the what's-new paragraph and the rail label read as one feature; `问到` = "ask about" · `high`
- **thumbnail** · `缩略图` · macOS Finder `zh-CN` (`缩略图大小：`, `小/中等/大缩略图大小`), Microsoft TBX (`thumbnail`
  → 缩略图), Nautilus/Thunar/TC/DC all agree; the old `noContents` value already used it · `high`
- **whole files (never sent)** · `整个文件` · plain "the whole file"; the old `文件本身：不发送文件内容…` wording was
  deliberately dropped because the new copy must NOT promise that no contents are ever sent · `high`
- **camera details (a photo's EXIF: camera, lens, settings)** · `相机信息` · Photos.app `zh_CN` info panel
  (`IPXInfoPanelLCDUnknownCamera` → `无相机信息`), Preview.app `Camera` → `相机`, catalog precedent `相机` for camera
  devices (`settings.section.mtp`); Microsoft TBX also has `摄像头`/`照相机` but those are the webcam/device senses,
  wrong here · `high`
- **location / where it was taken (a photo's GPS place)** · `拍摄地点` · Photos.app `zh_CN` calls photo places `地点`
  (`IPXPlaceBrowserTabLabel`, `你照片中的地点`) and "taken" `拍摄` (`这张照片是在…拍摄的吗？`); `拍摄地点` is the
  everyday compound. NOT `位置`: in this catalog `位置` is a file-system location (`目标位置`, `原来的位置`), and
  reusing it for a photo would read as the file's path · `high`
- **some lines of text (of a text file)** · `几行文本` · settled `文本` (macOS `纯文本`, viewer `文本` mode); `几行` = a
  few lines. Kept distinct from `文字` (the recognized text inside photos, `识别出的文字`, settled in
  `ai.cloudConsent.askCmdr.memory`): `文本` is file content, `文字` is writing seen in an image · `high`
- **some text (consent list item)** · `一些文本` · same `文本`; `一些` for the vaguer "some" · `high`
- **a few pages of a PDF** · `PDF 的几页` · `页` = page (AppKit Printing `第%ld页`, Microsoft TBX `页`, Dolphin `页数`,
  DC `逐页`); `PDF` verbatim (settled format token), spaced from the Han text · `high`
- **PDF pages (list item)** · `PDF 页面` · `页面` for the bare noun (Preview.app `页面大小`) · `high`
- **title and author (of a PDF)** · `标题和作者` · Preview.app `zh_CN` PDF inspector (`INSPECTOR_FILE_INFO_PDF_TITLE` →
  `标题`, `INSPECTOR_FILE_INFO_PDF_AUTHOR` → `作者`), AppKit `Title` → `标题`, Microsoft TBX `author` → 作者, Dolphin
  `Author` → 作者 · `high`
- **the list of files inside an archive** · `压缩文件里的文件列表` · settled `压缩文件` (the browsable zip/tar/7z; NOT
  Finder's `归档`, see the archive-browsing section) + `文件列表`; the shorter list item says `压缩文件里有哪些文件`
  ("which files are in the archive") for the English "what's inside an archive" · `high`
- **a limited part of it** · `其中有限的一部分` · plain rendering; `有限` = limited · `high`
- **Photo search works the same way** · `照片搜索也是同样的方式：` · the clause after the colon is reused verbatim from
  the old `askCmdr.consent.noContents` (`Cmdr 在匹配照片中识别出的文字及其标签会发送给你的提供方，以便它找到这些照片`),
  as is the closing sentence (`Ask Cmdr 可以建议重命名、移动和整理，在你批准之前，任何文件都不会有变化。`), so the
  paragraph stays consistent with the rest of the consent screen · `confirmed` (previously shipped wording)
- **"That's a bigger promise than the one you agreed to…"** · kept verbatim from the previous what's-new text
  (askCmdr.consent.whatsNew.body, retired) (`这比你当初同意的范围更大，所以这里再完整说明一次。`) · `confirmed`
- **looks inside a file only when you ask about it (`askCmdr.empty.hint`, `settings.askCmdr.intro`)** ·
  `只有在你问到某个文件时才会查看它的内容` · the same `查看…内容` / `问到` wording as the tool line and the retired
  what's-new text above; the old `从不读取文件内容` / `是只读的…从不修改任何内容` promises were removed because the
  English no longer makes them · `high`
- **never changes a file without your approval** · `未经你批准，绝不会更改任何文件` · `批准` matches the settled
  `在你批准之前，任何文件都不会有变化` (`consent.contentsRule`); `更改` for "change" (macOS AppKit `复查更改…`) · `high`

## 回滚按钮的两条提示 (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

新界面：按钮提示现在说清这一次回滚会对文件做什么；跨文件系统的移动一进入最后一步（所有文件都已到达目标位置，正在移除原文件），按钮就会关掉。

- **`rollbackTooltipStopAndMoveBack` → `停止操作，并把目前已移动的所有文件放回原处`** · 句式沿用同胞键
  `rollbackTooltip`（`停止操作，并…`），`放回原处` 是目录里已定的说法（`cancelRollback.doneMovingBack`“放回原处”）·
  `high`。❌ 不用 `删除`：回滚一次移动不删除任何东西。
- **`rollbackAlreadyLandedTooltip`** · 前半句沿用 `cancelRollback.moveAlreadyLanded`
  的说法（“已经在目标位置了”），`回滚` 是已定的术语（`rollbackUnavailableTooltip`），`取消`
  直接用旁边按钮自己的标签（`fileOperations.button.cancel`），按目录惯例加上引号 · `high`。

## “在此处打开终端”与它的 App 选择器（`settings.behavior.openTerminalHereApp.*`、`settings.navigationAndFileOps.card.terminal`）

新界面：`行为 > 导航与文件操作` 下的一张卡片，用来选这个命令启动哪个终端 App。列表由 macOS 生成，这里只翻译标签。

- **Open terminal here（命令名）→ `在此处打开终端`** · 沿用 Apple 的 `在终端中打开`，用 `在此处` 表示位置 ·
  `high`。命令本身的翻译（菜单、命令面板）必须用完全一样的写法。
- **Choose an app… → `选取 App…`** · Apple 的 `Choose Application…`（`N137` 键）写作 `选取应用程序…`；`App`
  沿用词汇表里已定的说法 · `high`。
- **terminal app → `终端 App`** · 拉丁词前后留空格，与目录里其余 `App` 用法一致 · `high`。两个值都不含撇号。

## git 的 worktree（`errors.git.orphanedWorktree.*`、`settings.fileExplorer.git.showVirtualGitPortal.description`、`fileExplorer.git.size.linkedWorktrees`）

英文把 "worktree" 和 "working tree" 当成两个词用，中文目录也照此分开。

- **worktree（git 的关联检出）→ `worktree`，原样保留** · en 的 `@key` 说明写着 "\"worktree\" is a git term; do NOT
  translate"，`de`、`fr`、`nl`、`pt`、`vi`、`hu`、`sv` 都是原样 · `high`。三处都用它：错误面板
  `errors.git.orphanedWorktree.*`（本来就是）、设置里的
  `settings.fileExplorer.git.showVirtualGitPortal.description`、git 门户 Size 栏的
  `fileExplorer.git.size.linkedWorktrees` = `{countText} 个关联 worktree`。此前后两处写作
  `工作树`，和解释它的错误面板对不上，同一个东西有了两个名字。
- **working tree（泛指工作区）→ `工作树`** · `errors.git.bareRepo`、`blobTooLarge`、 `gitDirPermissionDenied`
  里是普通行文，保持 `工作树` 不变 · `high`。
- 拉丁词前后留空格（`个关联 worktree`），与目录里其余拉丁词一致；量词仍是 `个`。`git worktree prune` 是命令，原样。

## `Sort by relevance`：搜索结果列的悬停提示（`fileExplorer.columns.sortByRelevance`）

新界面：搜索结果面板中当前排序列的列头悬停提示。再点一次会把行恢复成搜索引擎自己的顺序，最匹配的排在最前。

- **relevance（结果与搜索的匹配程度）→ `相关性`** · macOS 的四个来源一致：WorkflowKit（`Relevance (WFSearchSortOrder)` →
  `相关性`）、AppStoreKit（`SEARCH_FACET_RELEVANCE` → `相关性`）、Automator（`%1$[相关性]@ …`）和“音乐” ·
  `high`。（在 macOS 26.6.2、版本号 25G83 上用 `plutil` 导出随系统附带的本地化文件核对，2026-09-06）

## `Documents and packages`：新增的 OOXML 行（`settings.archives.ooxml.*`）

新界面：与 `Zip 压缩文件` 同在一张卡片里的一行，下方是 `应用程序包`
卡片。这一行有意同时覆盖 Office 文档（.docx、.xlsx、.pptx）和应用程序包（.jar、.apk），所以连英文原文也不点名 Office。

- **documents（文件种类）→ `文稿`** · macOS Finder（`TL6`/`GROUP_DOCUMENTS` → `文稿`；种类名 `RTF文稿`、
  `纯文本文稿`），与词汇表已有的 `document → 文稿` 一致 · `high`。❌ 不写 `文档`：Apple 的简体中文一律用 `文稿`。
- **packages（泛指，不只是 App）→ `软件包`** · macOS（`iOS Package Archive` → `iOS软件包归档`）· `high`。刻意不用
  `应用程序包`，那是下方卡片的词；`软件包` 让这一行比下方卡片更宽，正如英文用 `packages` 对 `app bundles`。
- **连接词 → `和`** · 目录中「X and Y」型标签几乎都用 `和`（`颜色和格式`、`日期和时间`、`提示和警告`）· `high`。
- **句式 → `在 …、…、… 或 … 上按 Enter 键时的行为。`** · 与同类键 `settings.archives.zip.description`、
  `settings.archives.bundle.description` 完全相同的格式 · `high`。值中没有撇号。

## 服务器面板与卷切换器里的服务器行（`servers.*`、`fileExplorer.navigation.connectionTooltip*`/`disconnect*`/`forget*`）

新界面：窗格里的服务器连接状态（正在连接 / 被拒绝的各种原因），以及卷切换器里每一行服务器的连接圆点提示、断开连接按钮、“忘记服务器”和“清除保存的密码”两个确认对话框。

26.6.2 / 25G83，`plutil` 读 `zh_CN.lproj`，2026-09-06）。

- **server → `服务器`** · Finder zh_CN（`Connect to Server` → `连接服务器`）、NetAuthAgent、目录里既有用法 ·
  `confirmed`。量词用 `台`（`这台服务器`），与目录里已有的 `这台服务器的管理员` 一致。
- **connect → `连接`；Connecting to X… → `正在连接到 {name}…`** · Finder zh_CN `Connect` → `连接`；句式沿用同胞键
  `fileExplorer.network.share.connecting`（`正在连接到 {hostName}…`）· `confirmed`。
- **disconnect → `断开连接`** · Finder zh_CN `Disconnect` → `断开连接`，目录里
  `fileExplorer.unreachable.disconnect`、`menu.network.disconnect` 都是它 · `confirmed`。`servers.paneState.disconnect`
  必须与它们一字不差（`i18n-terms` 会比）。
- **Try again（按钮）→ `重试`** · Finder zh_CN `Try Again` → `重试`；目录里
  `fileExplorer.errorPane.tryAgain`、`mtp.tryAgain`、`networkMount.tryAgain`、`licensing.dialog.tryAgain` 全是 `重试` ·
  `confirmed`。❌ 别写 `再试一次`（那是 `zh-Hant` 的选择）。
- **Cancel → `取消`** · 全目录唯一说法 · `confirmed`。
- **Keychain Access（App 名）→ `钥匙串访问`，加引号** · “钥匙串访问” App 的 `Localizable.loctable`
  zh_CN（`Keychain Access` → `钥匙串访问`），目录里 `ai.secretError.keychainBody` 已经写作 `打开“钥匙串访问”` ·
  `confirmed`。
- **certificate → `证书`；trust → `信任`** · 同一个包（`root certificate` → `根证书`，`trusted` → `信任`）·
  `confirmed`。
- **host key（SSH 主机密钥）→ `主机密钥`** · macOS 26.6.2 的 SSH 未知主机提示 zh_CN 写作
  `主机密钥的指纹为%@。`（`plutil` 扫 loctable 命中，2026-09-06；行业通用说法也是它）·
  `high`。`connectionTooltipNeedsHostKey` 第二句里回指时用短的 `密钥`，避免一句话里两次 `主机密钥`。
- **Sign in → `登录`；Signed out → `已退出登录`** · macOS zh_CN `Sign In` → `登录`、`Sign Out` → `退出登录`；目录里
  `fileExplorer.network.signIn` = `登录` · `confirmed`。
- **Forget server → `忘记服务器`** · 沿用目录里已有的 `menu.network.forgetServer` ·
  `confirmed`。`fileExplorer.navigation.forgetServerConfirmTitle` 与它同源，必须同字。
- **Forget saved password → `清除保存的密码`** · 沿用 `menu.network.forgetSavedPassword` 和
  `fileExplorer.network.share.forgetPassword` · `confirmed`。所以对话框正文和提示条也用
  `清除`（`要清除 {name} 保存的密码吗？`），不跟着 `忘记` 走：同一个对话框里两个动词会对不上。
- **注意 Apple 的 `忽略此网络`**：macOS 把 Wi-Fi 的 “Forget This Network” 译成
  `忽略此网络`（`WiFiSettingsKit.framework`，26.6.2）。这里没跟它——目录里 `忘记服务器` / `清除保存的密码` 先落地了，而且
  `忽略` 在“从列表里移除一台服务器”的语境下会读成“跳过”。
- **`disconnectPlaceAriaLabel` → `断开连接：{name}`** · 无障碍名必须原样包含可见标签的词（WCAG 2.5.3）。`断开连接`
  连续出现，满足包含关系；冒号只是分隔，`i18n-aria` 的比较会剥掉全角标点 · `confirmed`。
- **`disconnectBusyTooltip` → `此服务器上有操作正在进行，无法断开连接`** · 句式照抄同一位置的兄弟键
  `fileExplorer.navigation.ejectBusyTooltip`（`此设备上有操作正在进行，无法推出`），所以这里保留 `此` 而不是 `这个` ·
  `high`。
- **Cmdr couldn't X → `Cmdr 无法X`** · 目录通用写法 · `confirmed`。`servers.refusal.unreachable` 例外，用更口语的
  `Cmdr 连不上 {host}。`，与 `无法连到` 同义但更短，适合窗格里的一行。
- **compromised（主机密钥被吊销）→ `已泄露`** · 无 Apple 对应词；`已泄露` 在中文安全语境里通用，且比 `已失陷` 好懂 ·
  `tentative`（待母语复核，见 `review-queue.md`）。

## 服务器中心：表格列、状态与固定到宗卷选择器（`servers.hub.*`、`commands.servers*`、`fileExplorer.navigation.*Pin*`、`shortcuts.scope.servers`/`places`）

卷切换器里原来的「网络」行改叫「服务器」，点开是一张表：所有保存过的服务器（SFTP / WebDAV /
SMB）加上本地网络上找到的，列是 名称 / 类型 / 地址 / 状态 / 上次使用时间，最后一行是「添加服务器…」。行所在的**分组**仍叫
`网络` （`fileExplorer.navigation.groupNetwork`）。

`.strings` 的 `zh_CN`，全部验证于 macOS 26.6.2 / 25G83，2026-09-06）。

- **Name（列头）→ `名称`** · Finder zh_CN（`N220`），目录里 `fileExplorer.columns.name`、`menu.sort.name`、
  `queryUi.results.col.name` 也都是它 · `confirmed`。
- **Type（协议种类，列头）→ `类型`** · SystemSettings `str_Detail_globalproxy_Type`，目录里 `queryUi.ai.filter.type` ·
  `confirmed`。⚠️ 不要跟 Finder 的 `种类`（Kind，文件种类）混：那是文件的类别，这里是协议。
- **Address（列头）→ `地址`** · 全系统统一（Mail 工具栏、通讯录 `ABLabelsAndProperties`、CUPS 打印机 `Address`）·
  `confirmed`。
- **Status（列头）→ `状态`** · SystemSettings、Finder（`状态栏`、`iCloud状态`），目录里 `licensing.section.labelStatus`
  · `confirmed`。
- **Last used（列头）→ `上次使用时间`** · Apple 在**列头**里带上时间/日期名词：`钥匙串访问`/`Security.prefPane` 的
  `Last Used` → `上次使用时间`，Mail `AddressHistory` 的同名列头也是它；Finder 的 `Date Last Opened` → `上次打开日期`
  是同一习惯 · `high`。裸的 `上次使用`（ContactsUICore）用在句中，不用作列头。
- **Connected（状态）→ `已连接`** · 全系统统一（Finder `Connected servers` →
  `已连接的服务器`、Wi-Fi、蓝牙、VPN），目录里 `ai.cloud.connected`、`fileExplorer.network.browser.status.connected` ·
  `confirmed`。
- **Saved（状态：保存过但没连着）→ `已保存`** · 预览 App `SignatureSaved`，目录里
  `fileExplorer.navigation.connectionTooltipSaved`（`已保存。打开它即可连接。`）· `confirmed`。与 `已连接` 同为 `已…`
  的状态形容词，两个状态并排读起来是一组。
- **Found nearby（状态：本地网络上现在能看到）→ `在附近发现`** · `附近` 是 Apple 的说法（隔空投送 `people nearby` →
  `附近的用户`，蓝牙 `Nearby Devices` → `附近设备`）· `high`。没写成 `已在附近发现`：这不是用户造成的状态，`已…`
  会读得像刚刚完成的动作。
- **Signed out（状态：会话因为缺密码结束了）→ `已退出登录`** · 沿用本文件上一节已定的 `Sign in → 登录` /
  `Signed out → 已退出登录`（macOS `Sign Out` → `退出登录`）· `confirmed`。❗ 这不是「被拒绝」，别用 `被拒绝` /
  `认证失败` 一类的词。
- **Waiting for you to check the key（状态：SSH 主机密钥还没核对）→ `等你核对主机密钥`** · 沿用上一节的
  `host key → 主机密钥`；`等你…` 保留英文直接对话的口吻，比 `等待用户确认` 亲切 · `high`。状态列里没有上文，所以写全
  `主机密钥`，不像 `connectionTooltipNeedsHostKey` 那样简称 `密钥`。
- **Never（「上次使用」列里从没连过）→ `从未`** · 日历 App、`SecurityPrivacyExtension` 的 `Never` · `confirmed`。⚠️ 别用
  `永不`（那是「以后也不要」的设置选项，见 `SHARE_AGE_OPTION_NEVER_SHARE_TITLE`）。
- **Local network discovery → `本地网络发现`** · `Local Network` → `本地网络`（`Security.prefPane`、
  `AppSystemSettingsUI`），`discovery` → `发现` 沿用目录里的 `settings.network.firstTriggerDone.label`
  （`网络发现已启动`）和 `settings.network.enabled.description`（`发现你本地网络上的 SMB 服务器…`）· `high`。
- **Turn it on in Settings（链接）→ `在设置中开启`** · `Settings` → `设置`（系统里 App 设置窗口的通名），`在设置中开启…`
  是目录里现成的句式（`askCmdr.error.notConfigured`、`askCmdr.composer.providerOff`）· `confirmed`。
- **Add server… → `添加服务器…`** · Finder `Add` → `添加`（`RN21`、`IN_A7`），`Connect to Server` → `连接服务器`
  的构词 · `confirmed`。省略号保留 U+2026。
- **Show servers（命令）→ `显示服务器`** · 跟英文的动词走：目录里 `Show X` 一律是 `显示X`
  （`显示技术详情`、`在 Finder 中显示`），`前往X` 留给英文写 `Go to` 的那几条 · `high`。
- **Pin / unpin server（命令）→ `固定/取消固定服务器`** · 沿用本文件已定的 Safari `固定标签页` / `取消固定标签页` ·
  `high`。斜杠不加空格：中文界面里 `开启/关闭` 这类成对动词一向紧贴斜杠，目录里带空格的 `/`
  只用在数字分数（`{currentText} / {maxText}`）。
- **Disconnect server（命令）→ `断开服务器连接`** · `disconnect` = `断开连接` 已定；带宾语时拆成 `断开…连接`，与目录里
  `Cmdr 无法断开与 {name} 的连接。` 一致 · `high`。❗ 不能用 `推出`（Eject）：服务器没有可拔的东西。
- **Forget saved password（命令）→ `清除保存的密码`** · 与 `menu.network.forgetSavedPassword`、
  `fileExplorer.network.share.forgetPassword`、`fileExplorer.navigation.forgetSecretConfirmTitle` 一字不差（`i18n-terms`
  会比）· `confirmed`。
- **Edit server… → `编辑服务器…`** · `edit` 在目录里是 `编辑`（`commands.fileEdit.label` = `用默认编辑器编辑`）·
  `high`。
- **volume switcher（提示条正文里）→ `宗卷选择器`** · 目录里 `shortcuts.scope.volumeChooser`、
  `commands.paneLeftVolumeChooser.label`（`打开左侧宗卷选择器`）·
  `confirmed`。英文这里写 switcher、别处写 chooser，中文是同一个控件，用同一个词。
- **固定/取消固定的两条提示条不说「固定」这个词**：英文是大白话（"is in your volume switcher
  now"），中文照做（`{name} 现在会出现在你的宗卷选择器里。` / `{name} 已移出你的宗卷选择器。它仍然保存着。`）·
  `high`。第二句必须留着：它回答「是不是被删了」。
- **Servers（卷切换器里的行 / 快捷键分区）→ `服务器`** · 两个键英文同字，中文也必须同字 ·
  `confirmed`。⚠️ 这两个键以前装的是旧英文的译法（都写作 `网络`），这次一起改；`网络` 只留给它们所在的**分组**
  `fileExplorer.navigation.groupNetwork`。
- **Places（快捷键分区：一台服务器里面的那层列表）→ `共享位置`** · 今天是 SMB 主机上的共享文件夹（`Sharing.appex`
  `Shared Folders` → `共享文件夹`），以后会是对象存储的桶 · `tentative`。没有用裸的 `位置`：目录把 `位置`
  留给文件系统位置；也没有用 `地点`（留给照片的
  `拍摄地点`）。等桶落地后建议复审，可能要换成更中性的词。这个键以前装的是旧英文 `Share browser` 的译法 `共享浏览器`。
- **NAS 保持拉丁原样** · 目录通用（`settings.network.smbConcurrency.description`、
  `onboarding.stepOptional.networking.desc` 都写 `NAS`）· `confirmed`。`turn on a Mac or NAS` 里的 "turn on" 是通电，写
  `开机`，不是 `打开`。
- **量词** · 服务器用 `台`（`在下面添加一台`、`{countText} 台服务器`），与本文件上一节的 `这台服务器` 一致 ·
  `confirmed`。

## 添加服务器的模态表单、SSH 主机密钥确认、前往路径的预览行（`servers.sheet.*`、`servers.hostKey.*`、`servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`、`goToPath.dialog.opensServer`/`.addsServer`、`commands.serversConnect.label`）

服务器中心里「添加服务器…」打开的那张表单（协议 SMB / SFTP / WebDAV、地址、账户、`高级`
折叠区），第一次连 SSH 服务器时核对主机密钥的那一步，以及「前往路径」输入框下面的两行预览。

整个目录都没有，不是 worktree 陷阱），改用指南许可的实时 macOS 包取词：`plutil` 读 `.loctable` 的 `zh_CN`
分支，全部验证于 macOS 26.6.2 / 25G83，2026-09-07。

- **Connect（按钮）→ `连接`；Connecting… → `正在连接…`** · NetAuthAgent `Localizable.loctable`（`CONNECT` → `连接`、
  `CONNECTING_TO_GENERIC` → `正在连接…`，就是 Apple 自己的「连接服务器」对话框），目录里
  `fileExplorer.network.connect`/`.connecting` 也是同一对词 · `confirmed`。
- **Connect to server…（命令）→ `连接服务器…`** · NetAuthAgent `CONNECT_TO_SERVER` → `连接服务器` · `confirmed`。
- **Save（按钮）→ `保存`** · AppKit `Document.loctable`（`Save` → `保存`、`Save…` → `保存…`、`Don't Save` → `不保存`）·
  `confirmed`。⚠️ Apple 早年的 `存储` 已经不是现在的用词，别按旧印象写。
- **Sign in to {name}（表单标题）→ `登录 {name}`，不加介词** · AppSSOKerberos `LAPOLICY_REASON`（`sign in to %@` →
  `登录%@`）、CloudSharing（`sign in to your Apple Account` → `登录Apple账户`）·
  `confirmed`。拉丁词/占位符两侧留空格是本目录的写法，Apple 自己不留。
- **Signed out of {name} → `已从 {name} 退出登录`** · 沿用已定的 `Sign Out → 退出登录`；带宾语时用 `从…退出登录`，比
  `退出 {name} 的登录` 顺 · `high`。
- **Protocol（无障碍名）→ `协议`** · AddPrinter `IP.plugin` 的 IP 协议选择器无障碍描述（`Protocol` → `协议`）·
  `confirmed`。⚠️ 别用 Finder 的 `种类`（Kind，文件种类）。
- **hostname → `主机名`** · Automator `Variables.loctable`、Security `OID.loctable`、Terminal `ServiceBrowser`
  （`host name` → `主机名`），目录里 `fileExplorer.network.browser.tooltip.resolving` 已经是它 · `confirmed`。
- **Username → `用户名`；Password → `密码`；Name → `名称`；Address → `地址`；Advanced → `高级`；Cancel → `取消`** ·除
  `用户名` 外，都与目录里同英文的键一字不差（`fileOperations.archivePassword.placeholder`、
  `fileExplorer.columns.name`、`servers.hub.colAddress`、`settings.section.advanced`、`fileOperations.button.cancel`）；
  `Username` 在目录里只此一处，取 NetAuthAgent `GENERIC_MSG_NONAME`（`Enter your user name and password.` →
  `输入你的用户名和密码。`）· `confirmed`。`i18n-terms` 会比这几组。
- **passphrase → `密码短语`，所以 Key passphrase → `密钥密码短语`** ·
  Apple 全系统统一（DiskManagement、DiskImages2、Security `SecErrorMessages` 与 `OID`、Certificate Assistant
  `Enter Passphrase:` → `输入密码短语：`）· `confirmed`。写全是有原因的：这一栏解锁的是密钥文件，不是账户密码，`密码`
  两个字会把两栏读混。
- **Key file → `密钥文件`** · 与 `主机密钥` 同一个 `密钥`；`钥匙串访问` 把 `private key` 译作 `专用密钥`，但 UI 里
  `密钥文件` 更直白 · `high`。
- **Browse…（按钮）→ `浏览…`** · AppleAccountUI `PROFILE_BROWSE_PHOTO`（`Browse...` → `浏览…`）·
  `confirmed`。省略号是 U+2026。
- **Remember in Keychain → `记住到钥匙串`** · Apple 自己的说法是 `在我的钥匙串中记住此密码` （NetAuthAgent
  `AuthDialog.loctable`），词根 `钥匙串` 一致 · `confirmed`。`servers.sheet.needsStoredSecret` 正文里引用这个复选框时用
  `“记住到钥匙串”`，必须与复选框同字，`i18n-terms` 会比。
- **Connect as guest → `以客人身份连接`** · macOS `zh_CN` 里 Guest 一律是 `客人`（NetAuthAgent `GUEST` 与
  `EINFO_NO_ACCESS_GUEST`、LoginUIKit `GUEST_ACCOUNT_RECORD_NAME`、AppKit `NSUserGuest`；pile 里 `来宾`
  0 次），只有微软用 `来宾`，所以按「macOS 优先」取 `客人` · `high`。三个键同字：`servers.sheet.connectAsGuest`、
  `fileExplorer.network.browser.status.guest`（`客人`）、`errors.shareList.signingRequired`（`不向客人显示共享`）；改其中一个就要三个一起改。
- **Sign in with a username and password → `用用户名和密码登录`** · NetAuthAgent `GENERIC_MSG_NONAME`
  （`Enter your user name and password.` → `输入你的用户名和密码。`）· `high`。它是 `以来宾身份连接`
  的对照项，Apple对应的单选是 `注册用户`，但我们的英文是动词短语，所以照动词写。
- **How to connect（无障碍名）→ `连接方式`** · NetAuthAgent 的同位控件叫
  `连接身份：`（`CONNECT_AS`），我们这组选项问的是用不用账户，`连接方式` 更贴 · `high`。
- **远程文件夹那个字段现在叫「根文件夹」** · 见本文件末尾 § 服务器表单的根文件夹与起始文件夹。`Remote` →
  `远程`（FinderKit `LocalizableMerged.loctable`）这条词根仍然成立。
- **Reconnect automatically → `自动重新连接`** · AppSSOKerberos `MainMenu.loctable`（`Reconnect` →
  `重新连接`）、ClassroomKit（`Automatically` → `自动`），目录里 `fileExplorer.smbReconnect.*` 一直写 `重新连接` ·
  `confirmed`。
- **host key → `主机密钥`；fingerprint → `指纹`；Key fingerprint（标签）→ `密钥指纹`** · ActionKit
  `Localizable.loctable` 的 OpenSSH 提示（`The host key's fingerprint is %@.` → `主机密钥的指纹为%@。`；
  `The authenticity of host '%@' can't be established…` 同段）· `confirmed`。目录里
  `servers.refusal.hostKeyUntrusted`、`servers.hub.status.waitingForKey` 已经是 `主机密钥`，标题和提示里都写全，只有
  `connectionTooltipNeedsHostKey` 那种一句两提的地方才简称 `密钥`。
- **Trust（动词）→ `信任`** · AMPDevices 的设备信任按钮（`Trust` → `信任`）、OpenDirectoryConfigUI（`Don't Trust` →
  `不信任`）· `confirmed`。`Trust and connect` → `信任并连接`，`Trust the new key` → `信任新密钥`。
- **“something is sitting between you and it” → `有东西夹在你和它之间`** · 英文刻意不说
  `中间人攻击`（ActionKit 里 Apple 说的是 `中间人攻击`），中文照着英文的大白话走，别把术语补回去 · `high`。
- **the server''s owner → `这台服务器的所有者`** · `Owner` → `所有者`（PhotoLibraryServices `OWNER`、Photos
  `IPXSharedAlbumOwnerLabel`）· `high`。故意不写 `管理员`：那是目录里 `errors.listing.authRequiredEneedauth.suggestion`
  给 `administrator` 留的词，家里的服务器说 `所有者` 更准。
- **I''ve checked it（展开三角的标签）→ `我核对过了`** · 第一人称、用户在说话，配合状态列已定的 `等你核对主机密钥`
  用同一个动词 `核对` · `high`。
- **Cmdr stopped connecting to {name} → `Cmdr 已停止连接到 {name}`** · 与 `servers.paneState.connecting`
  （`正在连接到 {name}…`）同一句式，读起来是同一件事的两个结局 · `high`。
- **Opens {name} / Adds a server（前往路径的预览行）→ `打开 {name}` / `添加一台服务器`**
  ·第三人称描述回车会做什么；量词沿用服务器的 `台` · `high`。

## 自动重连的面板标题 + 无需输入的退出登录说明（`servers.paneState.reconnecting`、`.signedOutNothingToAsk`）

连接掉线后 Cmdr 自己按退避节奏重连时那个面板的标题（下面是转圈、倒计时和「立即重试 / 取消 / 断开连接」），以及服务器用 SSH 密钥（或 ssh-agent 身份）证明自己时，替代「登录…」按钮的那一行说明。

的 `zh_CN` 分支，全部验证于 macOS 26.6.2 / 25G83，2026-09-07。

- **Reconnecting… → `正在重新连接…`，所以 `Reconnecting to {name}…` → `正在重新连接到 {name}…`** ·
  Apple全系统统一：ScreenSharing `ScreenSharing.loctable`（`reconnectingMessage`）、HomeDataModel
  `HFLocalizable.loctable`（`HFServiceDescriptionReconnecting`）、FaceTime/Phone 的 `RemotePeoplePicker.appex` 都是
  `正在重新连接…`；`Reconnect` → `重新连接` 也早已定过（AppSSOKerberos）· `confirmed`。到 `{name}` 的介词沿用
  `servers.paneState.connecting`（`正在连接到 {name}…`），两个状态读起来是同一句式。
- **“there''s nothing to X” → `没有要 X 的内容`** · Apple 的固定句式：`There''s nothing to send.` →
  `没有要发送的内容。`、`There is nothing to print.` → `没有要打印的内容。`（AppIntents、Printing `.loctable`）·
  `confirmed`。所以 `nothing to type` → `没有要输入的内容`，别写成 `没什么可打的` 这类口语过头的说法。
- **signs in with a key rather than a password → `登录这台服务器用的是密钥，而不是密码`**
  ·主语挪到「登录」这件事上，而不是照英文让服务器去「登录」：中文里「服务器登录」会读成服务器是登录方 · `high`。
  `密钥`（已定，与 `主机密钥`/`密钥文件` 同词）、`密码`（已定）、量词 `台`（已定）都是沿用，不是新词。
- **Open it again to retry. → `重新打开它即可重试。`** · `重新打开` 是目录里既有的说法（`fileExplorer` 的
  `重新打开这个文件夹再试一次`），`…即可重试` 与同组的
  `servers.paneState.cancelCycleTooltip`（`切换回来即可重试。`）同型 · `high`。同组的 `hostKeyChangedHint` 写的是
  `然后再打开它来核对指纹`，那里的 `再` 由 `先…然后…` 撑着；单句开头用 `重新打开` 更顺。
- **不用「认证 / 验证身份」**
  · 英文这里刻意说大白话（`signs in with a key`），中文照着走，别把 SSH 术语补回来。同一条原则见上面
  `有东西夹在你和它之间` 那一行。

## 固定/取消固定的右键项与一次性提示、可信主机密钥页、Android（ADB）设置页（`menu.network.pinToSwitcher`/`.unpin`、`servers.pinHint.*`、`settings.servers.*`、`settings.adb.*`、`settings.section.servers`/`.adb`、`settings.summary.servers`/`.adb`、`settings.behavior.serversPinHintSeen.*`、`settings.appearance.tintSmb.*`）

宗卷选择器里服务器行的右键菜单（固定 / 取消固定），「网络」分组太长时弹的一次性提示条，设置里新的「服务器（SFTP、WebDAV）」页（可信主机密钥列表）和「Android（ADB）」页（adb 状态、重新查找、安装说明、路径选择），以及改名后的服务器窗格着色项。

整个目录都没有，不是 worktree 陷阱），继续按指南许可的方式从实时 macOS 包取词：`plutil` 读 `.loctable` 的 `zh_CN`
分支，全部验证于 macOS 26.6.2 / 25G83，2026-09-07。

- **Pin to switcher（右键项）→ `固定到宗卷选择器`；Unpin → `取消固定`** · 沿用目录里已定的
  `menu.tab.pinTab`/`menu.tab.unpinTab`（`固定标签页` / `取消固定标签页`）和 `commands.serversTogglePin.label`
  （`固定/取消固定服务器`）；控件名 `宗卷选择器` 也是已定的 · `high`。⚠️ **故意不跟 Apple 的 `置顶`**：macOS `zh_CN`
  在列表里把 pin/unpin 一律译作 `置顶` / `取消置顶`（备忘录 `Pin Note`、地图 `Unpin`、快捷指令
  `Pin`、提醒事项、音乐，全部 2026-09-07 核对），但 `置顶`
  说的是「挪到最上面」，而这里是「留在选择器里」。Apple 自己在这个意思上也用 `固定`（快捷指令 `Pin in Menu Bar` →
  `在菜单栏中固定`），Safari 的 `固定标签页` 同理。
- **`servers.pinHint.body` 里的「Unpin」必须与右键项一字不差** · 正文写 `选择“取消固定”`，`i18n-terms`
  之外没有检查会比这一对，靠这条记着 · `confirmed`。
- **group（宗卷选择器里的分组）→ `分组`** · 目录里这个概念一直叫分组（本文件多处、`fileExplorer.navigation.group*`
  三个键的中文是分组名本身）· `high`。⚠️ 不用 Apple 的
  `群组`：那是「一群人/一组设备」（日历、家庭、课堂），不是列表里的分节。提示条标题写 `你的“网络”分组越来越长了`，分组名
  `网络` 与 `fileExplorer.navigation.groupNetwork` 一字不差。
- **Got it（提示条按钮）→ `知道了`** · 目录里同英文的三个键（`ai.toast.gotIt`、`main.oldMacos.gotIt`、
  `updates.moveToApplicationsDialog.gotIt`）都是它 · `confirmed`。
- **Trusted host keys（卡片标题）→ `受信任的主机密钥`** · Apple 的定语式就是 `受信任…`（Network.appex
  `Trusted certificate` → `受信任证书`、`Trusted servers` → `受信任服务器`；iCloudSettings `Trusted devices list` →
  `信任的设备列表`）· `high`。`主机密钥` 沿用上面已定的 `host key`。
- **Trusted（日期前缀，读作「已信任 2026-09-07」）→ `已信任`** · Apple 没有这个位置的对应项；照本文件已定的「状态用
  `已…` 形容词」写，而且这确实是用户自己做过的动作 · `high`。
- **Forget（每行上的按钮）→ `忘记`** · 与 `menu.network.forgetServer`（`忘记服务器`）同词根，去掉宾语 ·
  `confirmed`。确认标题 `忘记这个密钥？` 用 `这个`（本文件已定：口语的 `这个` 优先于书面的 `此`）。
- **Status（状态行标签）→ `状态`** · 目录里同英文的 `servers.hub.colStatus`、`licensing.section.labelStatus` 都是它 ·
  `confirmed`。
- **Not found（adb 找不到时的状态值）→ `未找到`** · AppKit `FindPanel.loctable`（`Not found` → `未找到`）、AirPort 工具
  `placeholder.notfound`、照片 `PGErrorFormatNotFound` · `high`。⚠️ 不写成目录里 `errors.listing.notFound.title` 的
  `找不到路径` 那种带宾语的句式：这里是一个裸的状态值。
- **Found at {path} → `已找到：{path}`** · Apple 没有对应句式；`已找到` 与 `未找到`
  成对，全角冒号后接可能很长的路径，换行点干净 · `high`。
- **Re-check（按钮）→ `再次检查`** · Apple 全系统统一（`Check Again` → `再次检查`：Mail `ConnectionDoctor`、
  `SoftwareUpdate.framework` 的 `CheckAgain`、`TextToSpeechVoiceBankingUI`）· `high`。安装说明里引用它时一字不差：
  `然后点按“再次检查”：`（`点按` 是本文件已定的 click）。
- **Watching for phones. → `正在留意接入的手机。`；Not watching… → `目前没有在留意接入的手机。`** · Apple 没有 "watching
  for" 这个说法，最近的是 `Waiting for iPhone or iPad…` → `正在等待iPhone或iPad…` （Setup Assistant
  `PROXIMITY_PAIRING_TITLE`），但 `等待` 会读成「现在正卡着等」，而英文说的是被动的随时留意 · `tentative`。`留意`
  在目录里有先例（`onboarding.stepBeta.openBeta` 的
  `留意那些 <alpha></alpha> 徽章`）。❗ 按英文的要求，两句都不提 ADB 服务器、订阅或套接字。
- **Look for adb the usual way（占位符）→ `按常规方式查找 adb`** · 与
  `settings.fileOperations.adbBinaryPath.description` 里已经落地的 `Cmdr 会按常规方式查找 adb` 一字不差 · `confirmed`。
- **Choose the adb command（文件选择器标题）→ `选取 adb 命令`** · 目录里同类标题
  `settings.behavior.openTerminalHereApp.chooseAppTitle`（`Choose a terminal app` → `选取终端 App`）·
  `confirmed`。macOS 的文件选择器按钮也是 `选取`，不是 `选择`。
- **Browse…（按钮）→ `浏览…`** · 与 `servers.sheet.browse` 一字不差（AppleAccountUI `PROFILE_BROWSE_PHOTO`）·
  `confirmed`。省略号是 U+2026。
- **Android platform tools → `Android 平台工具`；USB debugging → `USB 调试`；`adb` / `ADB` 保持拉丁**
  ·已在本文件上方的术语表里定过，来源是 Google 自己的 zh-CN 文档；这一批只是复用 · `confirmed`。
- **括号用全角，并列用顿号** · `Servers (SFTP, WebDAV)` → `服务器（SFTP、WebDAV）`、`Android (ADB)` →
  `Android（ADB）`，与目录里 `settings.section.mtp`（`MTP（Android/Kindle/相机）`）、`adb.volumeLabelWithSuffix`
  （`{deviceName}（ADB）`）一致 · `confirmed`。英文的 `, ` 在中文并列里写 `、`。
- **服务器窗格着色改名** · 英文从 "Tint SMB panes" 改成了 `Tint server panes (SMB, SFTP, WebDAV)`，中文跟着改成
  `为服务器窗格着色（SMB、SFTP、WebDAV）`，说明句照兄弟键 `settings.appearance.tintLocal.description` 的句式写成
  `为显示 SMB 共享、SFTP 服务器或 WebDAV 服务器的窗格添加的背景着色。` · `high`。旧值只提 SMB 和网络共享，已经不成立。

## Android（ADB）手机的窗格状态、卷切换器提示与那条一行提示（`adb.*`、`settings.behavior.adbHintDismissed.*`）

三个新界面：手机没打开成的时候窗格里那条整版消息（`adb.connect.*`）、宗卷选择器里手机那一行的悬停提示（`adb.readiness.*`、
`adb.disconnect*`），以及手机走普通「照片和音乐」连接时窗格顶上那条安静的提示（`adb.hint.*`）。

整个目录都没有，不是 worktree 陷阱），所以两路取词：Apple 的词从实时 macOS 包取（`plutil` 扫 `.loctable` 的 `zh_CN`
分支，验证于 macOS 26.6.2 / 25G83，2026-09-07）；Android 自己的词直接从 AOSP 的 `values-zh-rCN` 取（2026-09-07 抓取
`main` 分支）。

- **USB debugging → `USB 调试`** · AOSP `frameworks/base/packages/SettingsLib/res/values-zh-rCN/strings.xml` 的
  `enable_adb` 就是 `USB 调试`，SystemUI 的 `usb_debugging_title` 写作 `允许 USB 调试吗？` ·
  `confirmed`。手机上「开发者选项」里的开关一字不差就是这四个字，所以用户能照着 Cmdr 的话在手机上找到它。术语表里已有的 Google
  zh-CN 文档来源这次由 AOSP 源码直接坐实。
- **Allow（Android 自己那个对话框上的按钮）→ `允许`，并加全角引号** · AOSP SystemUI `values-zh-rCN` 的
  `usb_debugging_allow` = `允许`（`wifi_debugging_allow` 同）·
  `confirmed`。屏幕上是哪几个字，Cmdr就说哪几个字。引号沿用目录里引用界面名称的写法（`打开“钥匙串访问”`、`选择“取消固定”`）。
- **tap（在手机上点一下）→ `点按`** · AOSP zh-CN 自己压倒性地用 `点按`（Settings 89 : 14，SystemUI 34 : 5压过
  `点击`），与本文件已定的 macOS `点按` 一致 · `confirmed`。中英两边的动词碰巧同一个词，省了一次概念切换。
- **Android platform tools → `Android 平台工具`；`adb` / `ADB` 保持拉丁**
  · 术语表已定，来源是 Google 自己的 zh-CN 文档；这一批只是复用 · `confirmed`。
- **phone 的量词是 `部`，不是 `台`** · AOSP zh-CN 只写 `这部手机`（Settings 里 8 处，`这台手机` 0 处，2026-09-07）·
  `high`。⚠️ 与本文件已定的服务器量词 `台`（`这台服务器`）和目录里的 `这台 Mac`
  并存，是有意的：手机随 Android 自己的说法。
- **Cmdr couldn't find X → `Cmdr 找不到 X。`** · 目录里 `errors.listing.notFound.explanation`、
  `errors.listing.pathNotFoundErrno.explanation` 都是 `Cmdr 找不到 …` · `confirmed`。没写成
  `Cmdr 无法找到`：更啰嗦，而且这里不是「被拒绝」而是「没有」。
- **didn't answer in time → `没能及时响应`** · 与 `servers.refusal.timedOut`（`{host} 没能及时响应。`）、
  `errors.volume.connectionTimeout` 一个句式 · `confirmed`。
- **三条「连不上」互相不重样，各用各的动词** · `deviceGone`（手机被拔了）→ `已经断开连接了`；`transport`（线掉了）→
  `连接中断了`；`serverUnreachable`（这台 Mac 上的工具不吭声）→ `没有响应` ·
  `high`。三条会在同一个位置轮流出现，动词一样的话用户分不出发生了什么。`中断`
  在目录里有先例（`errors.listing.staleConnection.explanation` 的 `连接被中断`）。❗ 按英文的要求，`serverUnreachable`
  只说 `Android 工具`，不提后台程序、协议或套接字。
- **Waiting for you to X → `等你X`** · 沿用服务器中心已定的 `等你核对主机密钥`（英文同为 "Waiting for you to…"）·
  `confirmed`。所以是 `等你允许 USB 调试`，不写 `正在等待用户授权`：那读起来像系统日志。
- **Wake（唤醒屏幕）→ `唤醒`** · macOS `Localizable.loctable` 的 `str_mcx_EnergySaver_scheduletype_wake` （`Wake` →
  `唤醒`）· `high`。
- **reseat the cable → `把线缆拔下再插上`** · 目录里 `errors.provider.macDroid.transient` 已经写作
  `拔下再插上 USB 线缆`；`拔下` 也是 Apple 的说法（AirPort 工具 `Unplug “%@”` → `拔下“%@”`）· `confirmed`。
- **Try another cable or port → `换一根线缆或另一个端口试试`** · 目录里 `errors.listing.deviceProblem.suggestion` 的
  `换一个 USB 端口或线缆` 同源，这里按中文量词分开写（线缆用 `根`，端口用 `个`）· `high`。
- **How（那条提示末尾的链接）→ `怎么做`** · Apple 没有单独一个 "How" 链接可抄；`怎么做`
  是目录里现成的口语问法（`你想怎么做？`、`要怎么做？`、`你想怎么处理？`）·
  `high`。英文要的就是「这怎么弄？」的语气，`如何操作` 太书面，`了解更多` 又不是这个意思。
- **Dismiss → `关闭`** · 目录里 9 个同英文的键（`queue.row.dismiss`、`crashReporter.dialog.dismiss`、
  `downloads.fda.dismiss` 等）全是 `关闭`，`i18n-terms` 会比 · `confirmed`。
- **Open Settings → `打开设置`** · 与 `commands.appSettings.label`、 `commands.handler.openTerminalHere.openSettings`
  一字不差（英文同字，`i18n-terms` 会比）· `confirmed`。
- **`adb.disconnectDeviceAriaLabel` → `断开连接：{name}`** · 英文与 `fileExplorer.navigation.disconnectPlaceAriaLabel`
  一字不差（`Disconnect {name}`），所以中文必须同字，`i18n-terms` 会比 ·
  `confirmed`。手机和服务器在同一个位置用同一个按钮，说法本来也该一样。
- **`adb.disconnectBusyTooltip` → `此设备上有操作正在进行，无法断开连接`** · 把同一位置两个兄弟键拼起来：主语取
  `fileExplorer.navigation.ejectBusyTooltip` 的 `此设备`（英文这里也是 "on this device"），谓语取
  `fileExplorer.navigation.disconnectBusyTooltip` 的 `无法断开连接` · `confirmed`。保留 `此` 而不是
  `这个`，与那两条一致。
- **一次性提示的内部键跟着兄弟键的句式走** · `settings.behavior.serversPinHintSeen.label`/`.description`
  是模板（`已显示“网络”分组过长提示` / `是否已显示过关于取消固定服务器的一次性提示。`），所以 `adbHintDismissed` 写成
  `已关闭 USB 调试提示` / `是否已关闭那条建议开启 USB 调试的一次性提示。` ·
  `high`。这两个键从不出现在界面上，但覆盖率检查要它们。动词跟着英文的 dismissed 走，用已定的 `关闭`。
- **`You stopped opening your phone.` → `你停止了打开手机。`**（`adb.connect.cancelled`）· 动词照平行键
  `search.coverage.walk.cancelled`（`你停止了这次搜索`）和 `errors.volume.cancelled` 取 `停止` · `high`。❌ 不写
  `取消`：`取消`
  是按钮的标签（`fileOperations.button.cancel`），写成「你取消了…」会像在指那颗按钮，而不是在说发生了什么。`停止`
  后面直接带动词短语是目录里现成的写法（`停止建立索引`、`停止连接到 {name}`、`停止搜索`）。 `打开`
  是已定的开手机动词（`adb.connect.waitingHint`）；主语已经是 `你`，所以不再写 `你的手机`。

## 被锁住的服务器身份（`servers.sheet.identityLocked`）

编辑一台已保存的服务器时，置灰的「地址」和「用户名」两个输入框下面的两行说明。

- **`the account`（用来登录服务器的那个输入框）→ `账户`** · 目录里已有同一含义的用法（`errors.json` 六处、
  `onboarding.json` 一处）· `high`。
- **这行提示里的动作词必须和它指向的按钮一字不差**：`忘记` 取自 `menu.network.forgetServer`（「忘记服务器」）， `添加`
  取自 `servers.sheet.addTitle`（「添加服务器」）。换成近义词（「删除」「新建」）会让读者去找一个根本不存在的菜单项。
- **`are what name this server` → `决定了这是哪台服务器`**
  · 表单本身另有一个「名称」字段（`servers.sheet.name`），所以这句不能用「命名」：那会被读成在讲那个标签。用「决定了这是哪台」说的才是原意（这两个值就是这台服务器本身）·
  `high`。
- 量词沿用 `servers.json` 里已有的 `这台服务器`（六处）。

## 本来就没有保存过密码时的提示（`fileExplorer.navigation.forgetSecretNoneToast`）

- **`There was no saved password for {name}.` → `{name} 没有保存的密码。`**
  · 「保存的密码」一字不差沿用已发布的三个兄弟键（`menu.network.forgetSavedPassword`、`fileExplorer.navigation.forgetSecretConfirmTitle`
  = `清除保存的密码`， `.forgetSecretConfirm` = `要清除 {name} 保存的密码吗？`，`.forgetSecretRefusedToast`）·
  `high`。用户刚从那个确认对话框过来，用词必须一致。
- **`{name}`
  放主语位置最自然**，也避开了「为 {name} 保存的密码」这类要补介词的说法。中文不标时态，英文的过去式由「没有」直接承担；不加「过」，否则会读成「从来没保存过」，而不是「这次查下来没有」。
- 占位符后面留一个半角空格（§ style.md 的拉丁占位符间距规则）。不用「失败」「错误」：什么都没出错。

## 重试总时长、主机密钥标题，以及 Android 的“允许”按钮 (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`)

- **`{seconds}`/`{minutes}`
  现在是带两个占位符的 ICU 复数块**（`servers.paneState.retryTotalSeconds`、`.retryTotalMinutes`）：`{seconds}`
  只负责选分支，用户读到的是 `{secondsText}`，也就是已按语言格式化好的数字。中文只有 `other` 一个类别（CLDR，§
  style.md），所以每块只写一个分支，但外层的 `{…, plural, other {…}}` 壳必须保留，否则占位符与英文对不上 · `high`。
- **两个值都是
  `servers.paneState.retryKeepsTrying`（`会持续尝试，总共 {duration}。`）的句子零件**，所以不带介词也不带句号；`秒`、`分钟`
  沿用 `indexing.eta.*` 的写法，占位符和汉字之间留一个半角空格 · `high`。
- **`Cmdr won't connect to {name}` → `Cmdr 不会连接到 {name}`** · 「不会连接到」一字不差沿用兄弟键
  `servers.refusal.hostKeyRevoked`（`Cmdr 不会连接到它。`）。英文从 “stopped
  connecting”改成了持续性的拒绝，所以去掉「已停止」，那读起来像是中断了一次尝试 · `high`。
- **`Allow` 是 Android 自己的按钮 → `“允许”`**，一字不差取自
  `adb.connect.unauthorized`（`看一下你的手机，然后点按“允许”。`），连引号和动词「点按」一起沿用，这样用户在屏幕上能对上同一个词·
  `high`。

## 服务器行的右键菜单：打开与编辑服务器… (`menu.network.open`、`menu.network.edit`)

- **`Open`（在服务器行上）→ `打开`**（`menu.network.open`）· 与 `menu.file.open`
  一字不差，因为是同一个意思：走进某个东西里，而不是把文件交给某个 App。中文不区分这两种意思，macOS 也不区分：Finder 的
  `打开`（`LocalizableMerged` `N151`）、`打开方式`（`N152`）和
  `在新窗口中打开`（`FV7`，走进去的那个意思）用的是同一个动词（Finder 26.6.2，版本号 25G83，2026-09-07 读取）· `high`。
- **`Edit server…` → `编辑服务器…`**（`menu.network.edit`），逐字节抄自 `commands.serversEdit.label` ·
  `high`。两者打开的是同一张表单；两个不一样的标签会被读成两个功能。省略号是 `…` 这一个字符（U+2026），必须保留。
- **这两处相等有检查兜底**，不只是好看：`i18n-terms`
  会在两个英文值相同的键在中文里分叉时报出来。以后要改写其中一个，必须把另一个一起改。
- **`menu.*` 属于 RAW 家族**：菜单由 Rust 通过 `menu_t` 绘制，从不走 `t()`。所以撇号保持单个，写成 `''` 会让 `i18n-icu`
  失败。这两个值里没有撇号。

## Function key bar context menu (`fileExplorer.functionKeyBar.*`)

- function key
  bar（窗口底部的功能键命令按钮行）→ 功能键栏 · 已在目录中确定（`settings.appearance.showFunctionKeyBar.label`）；用于右键菜单项及其提示 ·
  high

## AI 文案改写：主语从 “Ask Cmdr” 换成 `Cmdr` / `AI`

英文做了一次扫尾：`Ask Cmdr` 现在只在**指代聊天面板本身**时出现（面板标题、`menu.view.askCmdr`、
`commands.askCmdrToggle.label`、`settings.section.askCmdr`、开关它的那几条 `settings.askCmdr.status.*` / `turnOn` /
`turnOff`）；凡是**描述 AI 在做什么**的句子，主语都改成了 `Cmdr`，少数几条改成 `the AI`。中文照搬这条分工。

- **句子主语 `Cmdr` → 直接写 `Cmdr`**，不要补成 `Ask Cmdr` · 目录里本来就这么写（`suggestedOps.cmdrFacts` =
  `Cmdr 掌握的信息`、`ai.cloudConsent.askCmdr.contentsRule` 开头的 `Cmdr 从不发送整个文件`）· `high`
- **句子主语 `the AI` → 写 `AI`**（`suggestedOps.*` 那一组）· 这四条是故意跟 `Cmdr` 分开的：`suggestedOps.agentReason`
  （`AI 给出的理由`）就挨着
  `suggestedOps.cmdrFacts`（`Cmdr 掌握的信息`），对话框存在的意义就是把「模型说的」和「Cmdr 核实过的」分开。❗ 别把
  `AI 给出的理由` 统一成 `Cmdr …`，那正好把这个区分抹掉 · `high`
- **指向设置里那一节时仍写 `Ask Cmdr`**（`Ask Cmdr 设置`、`Ask Cmdr 部分`）· 英文保留了 “the Ask Cmdr settings /
  section”，因为那一节的名字没变 · `high`
- **“Chatting” / “to start chatting”（去掉主语的两条）→ `聊天` / `开始聊天`** · `askCmdr.error.notConfigured` =
  `聊天需要一个 AI 提供方。请在设置中开启一个。`、`settings.askCmdr.provider.off` =
  `在“设置 › AI”中开启一个 AI 提供方，即可开始聊天。` · `high`
- **“What Cmdr sends” / “What Cmdr remembers” 是一对**，中文也要读成一对：`Cmdr 发送的内容` / `Cmdr 记住的内容` · `high`

## 状态角落的两条 AI 提示与“复查”这个动词 (`askCmdr.wake.*`, `askCmdr.wakeToast.*`)

- **“AI features” → `AI 功能`** · 沿用 `settings.ai.tooltipOff`（`AI 功能已关闭`）· `high`
- **`askCmdr.wake.needsFullDiskAccess` 的第二句必须一字不差包含
  `search.coverage.setUpFullDiskAccess`**（`设置完全磁盘访问权限`）· `@key` 说明点名要求两处措辞一致，写成
  `点按即可设置完全磁盘访问权限。` · `high`。改写其中任何一条都要回头看另一条。
- **“Click to …” → `点按即可…`** · `点按` 是 macOS zh-CN 唯一的写法（`点击` 0 次），style.md 已定 · `high`
- **review（复查建议的操作）→ `复查`** · 沿用 `suggestedOps.review`（`复查这些文件`）与
  `commands.suggestedOpsShow.description` · `high`。❗ 边界：`复查`
  只用在「再看一遍待批准的建议」这一个动作上；读报告、看文件内容仍是 `查看`（style.md 的 `查看` / `显示`
  之分不受影响）。所以建议提示条的主按钮 `askCmdr.wakeToast.action`（打开待批准的建议列表）是 `复查`，它的设置说明
  `settings.askCmdr.wakeToast.description` 写 `待你复查的内容`；旁边那个安静的链接 `askCmdr.wakeToast.openThread`
  说的是「为什么」，写 `看看原因`。入门引导里「review and apply」是先看一眼再应用（`检查后应用`），升级提示里「review
  them」是去看新选项（`查看`），都不是批准门槛，不用 `复查`。

## AI 提供方设置向导的用词（`onboarding.cloudSetup.*`）

上一轮是绕过流程翻的，这轮按流程逐条核过证据，结论是**五条都保留原样**。

- **placeholder → `占位符`** · Microsoft zh-Hans 术语库（`placeholder` id 92735 → `占位符` id 92751）· `high`
- **deployment（Azure 上给模型起的部署名）→ `部署`** · Microsoft zh-Hans 术语库（多条 `deployment` → `部署`）· `high`
- **endpoint → `端点`** · Microsoft zh-Hans 术语库 id 51076；Windows 专有条目里的 `终结点` 不适用于 API 端点 · `high`
- **address（那个 endpoint URL 字段）→ `地址`** · 目录里 `地址` 14 次对 `网址` 1 次，macOS zh-CN 也是 7:1 · `high`
- **terminal → `终端`**（App 时写 `终端 App`）· 沿用 `commands.fileOpenTerminalHere.*` · `high`
- **pull（`ollama pull`）→ `拉取`** · Microsoft zh-Hans 术语库的现代条目（id 2306935 / 2309495 → `拉取`）；早期的 `请求`
  是 pull request 的一半，不适用 · `high`

## 程序坞邀请（`main.dockPinNudge.*`、`settings.behavior.dockPinNudgeOfferedAt.*`）

用了几天之后弹一次的通知：问用户要不要把 Cmdr 放进 macOS 的程序坞，加上答应之后的四条结果提示。设置里那两个键是内部状态，界面上永远看不到。

参考堆这次**在这台机器上**（`_ignored/i18n/zh/`
是把简体来源汇总起来的软链接目录，可读），另外 Dock 自己的菜单不在堆里，直接从系统包取：`plutil` 读
`/System/Library/CoreServices/Dock.app/Contents/Resources/zh_CN.lproj/DockMenus.strings`（macOS 26.6.2，2026-09-09）。

- **Dock → `程序坞`** · Apple 一级证据三处对上：Finder `LocalizableMerged` `N169.13`（`Add to Dock` →
  `添加到程序坞`）、Finder `MenuBar` `300772.title`、AppKit `Common`（`…getting the desktop image from Dock` →
  `…从程序坞获取桌面图像…`）· `confirmed`。中文这个词一律译出，不留拉丁字母的 `Dock`。
- **Add to Dock（接受按钮）→ `添加到程序坞`** · 逐字用 Finder `N169.13` 的标签，用户在 Finder 里就是看到这几个字 ·
  `confirmed`。按钮整体写 `好，添加到程序坞`（`好` = OK，本文件已定的 macOS 肯定式）。
- **Keep in Dock（标题的说法）→ `留在程序坞里`** · Dock 自己的菜单项 `KEEP_IN_DOCK` 是
  `在程序坞中保留`；标题要读成一句口语问句，所以改成 `要把 Cmdr 留在程序坞里吗？`，动词 `留` 与 Apple 的 `保留` 同根 ·
  `high`。
- **pin / unpin（在程序坞里固定）→ `固定` / `取消固定`** · 沿用本文件 § 固定/取消固定 已定的那一对（Safari
  `固定标签页`、`固定/取消固定服务器`），正文写 `固定在下面的 Finder 旁边`，提示行写 `取消固定` · `high`。⚠️
  Dock 菜单自己是 `在程序坞中保留` / `从程序坞中移除`（`KEEP_IN_DOCK` /
  `REMOVE_FROM_DOCK`），不是一对可配的词；这里取目录内已定的 `固定`/`取消固定`，因为英文那两句要求两个词看得出是一对。
- **Finder → `Finder`（保持拉丁字母）** · 本文件 § 原生菜单 的既定取舍：Apple 简体叫「访达」，整个 zh 目录仍写 `Finder`
  · `high`。这次不破例。
- **Applications（文件夹）→ `“应用程序”文件夹`** · AppKit
  `AppKitErrors`（`Try dragging “%@” from the Trash to your Applications folder.` →
  `请尝试将%[tt]@从废纸篓拖到你的“应用程序”文件夹中。`），引号形式与本文件的 `“下载”文件夹` 一致 ·
  `confirmed`。那句 Apple 原文也是 `从 X 拖到 Y` 这个句式的出处。
- **configuration profile → `配置描述文件`** · SystemSettings `InfoPlist`（`Configuration Profile` →
  `配置描述文件`）；同一个包的 `MDMDisabledSettingsPane`（`These settings are controlled by a profile.` →
  `这些设置受描述文件控制。`）是这句话的句式范本 · `confirmed`。
- **log in（登录这台 Mac）→ `登录`，写成 `下次登录 Mac 时`** · Dock `OPEN_AT_LOGIN` → `登录时打开`；AppKit `Menus`
  `Log Out` → `退出登录` · `confirmed`。加上 `Mac` 是为了和目录里「登录服务器」的那些串分开，跟本文件
  `系统重新启动或退出登录` 加 `系统` 是同一个理由。
- **the Dock didn''t reload → `程序坞还没刷新`** · `刷新` 是目录已定的 refresh（macOS AppKit）· `high`。⚠️
  **这条绝不能写成「没固定上」**：图标其实已经写进去了，缺的只是重画，所以中文先说 `图标已经放好了`，再用
  `只是…还没刷新` 交代那半件事。
- **四条结果串都不许出现 `失败`/`错误`** · 按 style.md：`无法把自己加进去`（受管的程序坞）、`这次没能添加到程序坞`
  （没落地）。`没能` 带「这一次没成」的意味，正好对上英文的 `couldn''t … this time`；`无法` 留给持续性的限制 · `high`。
- **offer（这次邀请，内部设置用词）→ `邀请`** · 描述性选词，堆里没有对应项；标签 `已发出程序坞邀请`，说明
  `是否已经发出过将 Cmdr 添加到程序坞的一次性邀请。`，句式抄 `settings.behavior.serversPinHintSeen.description`
  （`是否已显示过关于取消固定服务器的一次性提示。`）· `tentative`。
- **「a few days」不许变成数字 → `好几天`** · 阈值会改，写成「三天」对一半人就是假话；`好几天`
  是中文里正好的模糊小量词 · `high`。

## 程序坞图标右键菜单（`menu.dock.*`）

右键点按程序坞里的 Cmdr 图标弹出的那个原生菜单：置前主窗口、三条常用命令，外加几行最近去过的文件夹。原生菜单没有截图，
`@key` 说明就是全部依据。

一级来源是 Dock 自己的菜单串，参考堆里没有，直接从系统包取：`plutil -convert json -o -` 读
`/System/Library/CoreServices/Dock.app/Contents/Resources/zh_CN.lproj/DockMenus.strings`；Finder 的菜单栏用
`Finder.app/Contents/Resources/{en_GB,zh_CN}.lproj/MenuBar.strings`（按 nib 对象 id 对照）。均验证于 macOS 26.6.2 /
25G83，2026-09-09。

- **Open Cmdr → `打开 Cmdr`** · `打开` 是 Dock `OPEN` 的原词，也是本文件已定的 open 动词；`动词 + 应用名`
  不加引号，跟目录里
  `menu.app.hide`（`隐藏 Cmdr`）、`menu.app.quit`（`退出 Cmdr`）、`menu.app.about`（`关于 Cmdr`）读成一套 · `high`。
- ⚠️ **别给应用名加引号。** Dock 的 `HIDE_NAME` / `SHOW_NAME` / `OPEN_FILENAME` 确实写成
  `隐藏“%@”`、`打开“%@”`，但那对引号是给运行时替换进来的任意名字用的（跟本目录 `menu.context.copyNamed` 的
  `拷贝“{name}”` 同一个理由）。名字在写串的时候就定死时，Apple 自己不加引号：Finder 应用菜单是
  `隐藏访达`、`退出访达`，Safari 是 `隐藏Safari浏览器`、`关于Safari浏览器`。Cmdr 这条属于后者。
- **Search files… → `搜索文件…`** · 与菜单栏里同一条命令 `menu.edit.searchFiles` 逐字一致（`@key` 明确要求同措辞）·
  `confirmed`。
- **Go to folder… → `前往文件夹…`** · Finder `MenuBar` `261.title`（`Go to Folder…`）逐字照搬 · `confirmed`。⚠️ 不要跟
  `menu.go.goToPath` 的 `前往路径…` 混：那条英文是 `Go to path…`，是另一条命令，两个值本来就该不一样。
- **Connect to server… → `连接服务器…`** · Finder `MenuBar`
  `266.title`（`Connect to Server…`）逐字照搬，和本文件已定的connect to server（`连接服务器`）、`menu.network.*` 的
  `服务器` 对齐 · `confirmed`。
- **`{name} ({parent})` → `{name}（{parent}）`**
  · 同名两行时用上层文件夹区分。语序不动（中文限定语放在括号里跟在名字后面最自然），只把半角括号换成全角、去掉前面的空格：目录里
  `fileExplorer.functionKeyBar.actionWithShortcut`
  （`{action}（{shortcut}）`）就是这个形状，`menu.volume.eject`（`推出（{name}）`）、
  `menu.context.openWithDefault`（`{app}（默认）`）同样 · `high`。
- **省略号照旧是单个全角 `…`（U+2026）**，见本文件 § Ellipsis normalization。`menu.*`
  是 RAW 家族，不过 Chinese 这五条里没有撇号，ICU 转义无关。

## “在 Finder 中显示”的邀请与首次接手提示（`main.revealNudge.*`、`main.revealActivation.*`、`settings.behavior.reveal*`）

同一个功能的两个时刻：一次性地询问是否让其他 App 的“在 Finder 中显示”改在 Cmdr 中打开，以及这类请求第一次落到这里时的一次性提示。两处都指向 macOS 自己的命令，所以按 style.md
§ 系统界面，采用 macOS 的说法。

- **“Show in Finder” → `“在 Finder 中显示”`，用中文弯引号** · 已在 `settings.navigationAndFileOps.card.showInFinder` 和
  `settings.revealHandler.description` 中定稿 · `high`。提示条照抄这个形式，让设置卡片和提示用同一个名字称呼同一个动作。
- **pane → `窗格`** · 目录既有形式：`fileExplorer.doubleClickHint.body`（“窗格背景”）· `high`。
- **Settings（Cmdr 自己的窗口）→ `设置`** · `settings.window.title` · `high`。
- **“for a while now” → `有一阵子了`** · 故意含糊：阈值可能变动，所以 ❌ 绝不写具体天数。与
  `main.dockPinNudge.body`（“好几天了”）同一条规则 · `high`。
- **首次接手提示 ❌ 不是道歉** · 它说明刚才发生了什么、为什么，以及开关在哪里。所以写
  `Cmdr 设置成了接手这类请求`，❌ 不写“抱歉”· `high`。

## 入门引导改版：清单、步骤提示、可选项摘要 (`onboarding.stepBeta.checklist.*`, `onboarding.stepOptional.*`, `onboarding.wizard.stepTooltip`, `onboarding.stepFda.*`, `onboarding.stepAi.*`)

覆盖 `onboarding.moreAbout`、`onboarding.wizard.stepTooltip`、`onboarding.stepFda.*`、`onboarding.stepAi.*`、
`onboarding.stepBeta.checklist.*`、`onboarding.stepBeta.signup.*`、`onboarding.stepOptional.*.summary` 这一批。macOS
zh-CN 为 Tier 1，GitHub 自家中文文档用于 GitHub 专有动词，Microsoft zh-Hans TBX 交叉校验。

- **Save（邮箱字段旁的按钮）** · `保存` · macOS zh-CN：AppKit `Document`/`Preferences`/`Printing`/`SavePanel` 的 `Save`
  键、Finder `LocalizableMerged` 的 `AL2`/`BN38`，四处一致都是 `保存`（reference pile，2026-09-09）。⚠️ 不是
  `存储`：Apple 现在的简体中文用 `保存`。`onboarding.stepBeta.signup.rejected` / `.unreachable` 在句中引用这个按钮时写
  `“保存”`（全角引号），与按钮标签逐字一致 · `high`
- **star（GitHub 的动词）/ stars（数量）** · `加星标` / `星标` · GitHub 自家简体中文文档（`docs.github.com/zh`
  §「保存带星标的仓库」：`单击“星标”`、`已加星标`、`星标数`，2026-09-09）。GitHub的网页界面本身不出简体中文版，所以它的中文文档就是能拿到的最权威来源。这条现在是全目录唯一的说法：另一处用口语借词
  `点 star` / `个 star` 的旧反馈段落已随第 3 步改版一并删除。仓库仍是 `仓库`（目录里 18 处，与 `errors.git.*`
  一致），不是 Microsoft TBX 的 `存储库` · `high`
- **like（AlternativeTo 的点赞动词）** · `点赞` ·
  AlternativeTo 只有英文站，没有中文界面，因此没有「站点自家的中文动词」可抄（2026-09-09 核实过该站无语言切换器）。改用中文网站通用的
  `点赞` · `high`
- **checklist** · `清单` · Microsoft zh-Hans TBX（`checklist` → `清单`，CHN/SGP）。标题写 `入门引导清单`，复用已定的
  `入门引导` · `high`
- **mailing list** · `邮件列表` · ⚠️ Microsoft TBX 第一个命中是
  `邮寄列表`，那是「寄实物」的义项（how-to-mine.md 的 source-quality trap 4）。电子邮件订阅列表在中文里一律是 `邮件列表`
  · `high`
- **sign up / signup server** · `注册` / `注册服务器` · Microsoft zh-Hans TBX（`sign up` → `注册`）· `high`
- **typo（提示用户检查拼写）** · `看看是不是打错了` · 按 style.md 的口语register改写成动作，不落 `错误` 这个词 · `high`
- **Local Network（macOS 隐私面板里的项目）** · `本地网络` · 实机
  `SecurityPrivacyExtension.appex/.../Localizable.loctable` 的 `LOCAL_NETWORK` 键（macOS
  26.6.2，`plutil`，2026-09-09）；同一文件里 `ALL_FILES` =
  `完全磁盘访问权限`，与本文件已定的说法对上。`onboarding.stepOptional.networking.summary` 和
  `onboarding.stepOptional.networking.desc` 现在都逐字引用 `“本地网络”`，`访问` 二字留在引号外面当动词。❌不要写成
  `“本地网络访问”`：那不是用户在系统设置里能找到的名字（权限弹窗本身是整句「想要查找并连接到本地网络上的设备」，压根没有名词短语）·
  `high`
- **Settings › Updates & privacy（句中引用应用自己的设置路径）** · `“设置 › 更新与隐私”` ·
  `settings.section.updatesAndPrivacy` = `更新与隐私`；整条路径加全角引号、分隔符照抄英文原文（英文写 `›` 就写 `›`，写
  `>` 就写 `>`），与 `whatsNew.optOutToast`（`“设置 > 更新与隐私”`）、
  `settings.askCmdr.provider.shared`（`“设置 › AI”`）同形 · `high`
- **Learn more / More about X（信息图标的无障碍名称）** · `进一步了解“{topic}”` · macOS Finder `LocalizableMerged`
  `NE115`（`Learn More` → `进一步了解`）。`{topic}` 是运行时替换进来的任意标签（可能是中文，也可能以 Latin 开头，比如
  `MTP（…）`），所以照 Apple「运行时替换的名字才加引号」的规矩包一对全角引号，顺带解决中英夹排的空格问题 · `high`
- **Step {step} of {mandatory}+1（步骤圆点的 tooltip）** · `第 {step} 步，共 {mandatory}+1 步` ·沿用兄弟键
  `onboarding.wizard.stepProgress` 已定的 `第 {step} 步，共 {total} 步` 句形；字面量 `+1`
  原样保留（英文特意不写成 4，是要点明最后一步可选）· `high`
- **native handler（被 Cmdr 临时抑制的 macOS 进程）** · `macOS 内建的处理程序` · 与 `onboarding.stepOptional.mtp.desc`
  里的 `抑制 macOS 的那个进程` 同指，摘要一行取更短的名词说法；native 按 § 系统连接回退通知 用 Apple 的 `内建`，不用
  `原生` · `high`
- **dumber（本地模型比云端模型弱）** · `笨不少` · 英文特意用口语的 "dumber"，中文保留同样直白的口语调子，不改写成
  `能力较弱` 这类中性说法（style.md：保留刻意的随意语气）· `high`
- **released copy / Dev and test builds（`settings.revealHandler.notProductionBuild`）** · `正式发布的 Cmdr 版本` /
  `开发版和测试版` · Microsoft 术语（`CHINESE (SIMPLIFIED).tbx`：`release` → `发布` / `版本`，`production build` →
  `生产版本`）；这里说的是给人用的版本，所以不用 `构建`（那是 `search.systemDirExclude.default`
  里文件夹的说法）。第二句沿用 `settings.revealHandler.notInApplications` 的
  `每一次“在 Finder 中显示”的点击都指向空处`，让两条提示读起来是一对 · `high`（术语）；"come and go" 译作 `来来去去`
  是意译判断，`tentative`
- **fetch（查看器先把手机、服务器或压缩文件里的文件取到临时文件再显示）** · `获取`，标题写
  `正在获取“{fileName}”以便预览` · 目录里 key 名就叫 fetch 的 `fileExplorer.navigation.spaceFetchFailed` =
  `无法获取磁盘空间`（目录共 8 处 `获取`，0 处 `抓取`）；macOS zh-CN `Fetching Menu Item` 是 `正在抓取…`，但 `抓取`
  读起来像爬网页，不取。❌ 不写 Finder 的 `下载`（`PE126` `正在下载^0`）：USB 手机和压缩文件都不是下载。文件名照 Apple
  `FT1`（`正在将“^0”下载到“^1”`）加全角引号、不留空格 · `high`
- **"x of y"（字节进度行，两个值都带单位）** · `{doneText} / {totalText}` · macOS Finder `PW8` `已拷贝：^0/^1`、`PW3`
  `^0/^1 - ^2`，进度行一律用斜线；带空格的 `/`
  是目录留给数值分数的写法（`errorReporter.dialog.counter`），而且单位里本身有空格，不留空格会读成 `MB/250` · `high`
- **"so far"（总大小未知时已到的量）** · `目前已获取 {doneText}` · `目前已…` 沿用
  `fileOperations.transferProgress.rollbackTooltip`（`目前已写入`）、`queryUi.results.live.matchesSoFar`（`目前找到`）；动词与标题的
  `获取` 同字 · `high`
- **"stopped arriving"（45 秒没有数据到达）** · `这个文件的传输停住不动了` · 复用停滞传输那一批的 `停住不动了`（本身
  `tentative`），刻意不写 `已暂停` 或 `没有响应`，让它读成「停住了」而不是用户暂停或设备报错；后半句逐字沿用
  `errors.listing.*.suggestion` 的 `确认磁盘或设备仍连接着` 与 `然后重试` · `tentative`（随 `停住不动了`）

## ADB 手机的索引“可能已过期”（`fileExplorer.navigation.driveIndex.tooltipStalePhone`、`indexing.staleDialog.titlePhone` / `.bodyPhone`）

驱动器那三条（`tooltipStale`、`staleDialog.title`、`staleDialog.body`）的手机版。手机一直插着，只是从不报告自己的文件改动，所以 ❌ 不能提断开连接。

- **phone → `手机`，量词 `部`** · 复用本文件 § Android（ADB）手机 已定的说法（AOSP zh-CN `这部手机`）·
  `confirmed`。标题因此是 `这部手机的索引可能已过期`，与 `这个驱动器的索引可能已过期` 只差主语。
- **rescan → `重新扫描`** · 目录已定；参考堆里 Microsoft TBX（`重新扫描`）、Total Commander（`F2 重新扫描`）、KDE 一致 ·
  `confirmed`。
- **"keeps this index current with the changes it makes" → `Cmdr 自己做的更改会及时更新到这个索引里`** · 刻意 ❌ 不写
  `让索引保持最新`：这个圆点正是在说索引「可能不是最新」，`保持最新` 会读成打包票。`自己做的`
  把范围限定在 Cmdr 的拷贝、移动、删除上 · `high`。
- **"show up after a rescan" → `要重新扫描后才会显示`** · `才` 承担英文里「在那之前看不到」的意思；弹窗正文补上
  `在目录大小和搜索结果里`，这几个字逐字取自 `staleDialog.body`，所以两版读起来是一家 · `high`。
- **yellow status → `黄色状态`**，句形沿用 `staleDialog.body` 的 `驱动器旁边始终会显示黄色状态`，改成
  `手机旁边会一直显示黄色状态，提醒你这一点` · `high`。
- **new photos → `新照片`** · `照片` 是目录已定的说法（`已索引 {countText} 张照片`）；Photos.app 语境 · `high`。
- `{name}` 是手机的显示名（常见如 `Pixel 9 Pro XL`），放在句首，后面留半角空格；三个值都没有撇号，也不含 `错误` /
  `失败`。

## 服务器表单的根文件夹与起始文件夹（`servers.sheet.rootFolder*`、`.startFolder*`、`.nameHelp`、`servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`）

编辑 SFTP /
WebDAV 服务器的表单里两个字段：根文件夹是这台服务器的「天花板」，Cmdr 不会往它上面走；起始文件夹是打开服务器时窗格进入的位置，必须是根文件夹本身或它里面的文件夹。九个键里两个词必须一字不差。参考堆（`_ignored/i18n/zh/`）和实时 macOS 包都查过，验证于 macOS
26.6.2 / 25G83，2026-09-11。

- **root folder → `根文件夹`** · Microsoft TBX `zh-Hans`（`root folder` / `root directory` 给
  `根文件夹`、`根目录`、`顶层文件夹` 三个）、Total Commander zh-CN（`根文件夹` 4 处，如
  `切换驱动器时总是跳转到根文件夹`）、Double Commander zh-CN（`Go to root directory` → `转到根文件夹`）；Thunar 用
  `根目录` · `high`。取 `根文件夹` 是因为 `文件夹` 是已定的 folder。⚠️ 不用 `顶层文件夹`：那是 `errors.mutation.*`
  给宗卷自己最上面那层留的词，而这里是用户自选的上限，英文也是另一个词。这个字段的旧标签 `远程文件夹`（英文 "Remote
  folder"）已随键删除。
- **start folder → `起始文件夹`** · macOS Safari `MainMenu.strings`（`ZCQ-eG-J6s.title`：`Show Start Page` →
  `显示起始页`）、Microsoft TBX（`start page` → `起始页`）· `high`。⚠️ 不用 Double Commander 的
  `启动文件夹`（`Start in directory`）：在 Windows 中文里它是开机自动运行程序的「启动」文件夹，读者会想歪；DC 自己另一处又写
  `开始文件夹`，并不统一。
- **"Leave it empty to…" → `留空则…`** · style.md 已定的留空句式；`nameHelp` 写
  `留空则用账户和主机作为这台服务器的名称。`，用 `名称` 扣住上面的字段标签 `servers.sheet.name`，`账户`、`主机`
  都是已定词 · `high`。
- **"never goes above this folder" → `不会越过这个文件夹往上走`** · 大白话，说的就是「天花板」；刻意没写
  `上层文件夹`（那是 Finder 的 Enclosing Folder 命令名，会读成一个菜单项）· `tentative`。
- **"Cmdr can''t open … Check that it exists and that your account can read it." →
  `Cmdr 无法打开 {host} 上的这个文件夹。请确认它存在，并且你的账户能读取它。`** · `Cmdr 无法打开…` 与
  `errors.git.gitDirPermissionDenied.title` 同型，`请确认…` 与 `fileExplorer.navigation.driveIndex.refusedUpgradeFailed`
  同型 · `high`。起始文件夹那条只多 `起始` 两个字。
- **"didn''t answer in time, so nothing was saved. Try again in a moment." →
  `{host} 没能及时响应，所以什么都没保存。过一会儿再试一次。`** · `没能及时响应` 逐字沿用
  `servers.refusal.timedOut`；`过一会儿再试一次` 与 `ai.translateError.timeout.body` 同型 · `high`。

## 共享装载不上、共享列表加载不出来时的那句话（`errors.mount.*`、`errors.shareList.*`）

`无法装载共享`（`fileExplorer.networkMount.mountFailedTitle`）和 `无法连接到 {hostName}`
（`fileExplorer.network.share.connectFailedTitle`）下面的那句话，外加
`fileExplorer.pane.directConnectionShareGoneToast`、
`fileExplorer.pane.directConnectionMountNotRespondingToast`、`fileExplorer.pane.directConnectionNotNetworkShareToast`
三条提示和 `servers.refusal.accountNotPermitted`。Tier 1 取自系统里装着的
`NetAuthAgent.app/Contents/Resources/Localizable.loctable`（macOS
26.6.2，25G83，2026-09-11）：它正是「连接服务器」这些出错情形的文案，参考堆里没有。

- **share → `共享`** · NetAuthAgent `EINFO_NO_SHARE`（`服务器上不存在共享“%@”。`）· `high`
- **guest → `客人`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST`，与 `servers.sheet.connectAsGuest`
  同字（见 § 添加服务器的模态表单）· `high`。
- **reach → `连不上`** · `servers.refusal.unreachable` · `high`
- **didn't answer in time → `没能及时响应`**、**isn't responding → `没有响应`** · `servers.refusal.timedOut`、
  `adb.readiness.offline` · `high`
- **this computer → `这台电脑`** · 这些句子在 Linux 上也会出现，所以不写 `Mac` · `high`
- **package → `软件包`** · KDE Dolphin（`无法找到 %1 软件包。`）、`settings.archives.ooxml.label` · `high`。
  **distribution（Linux）→ `发行版`** · 参考堆没有 Linux 义项（Microsoft TBX 只有 `分配`）· `tentative`
- **Something went wrong → `出了点问题`** · `errors.mutation.unexpected` · `high`
- **there's nothing to speed up → `所以没有连接需要加速`** · 句形取自 `errors.eject.notAnSmbVolume` · `high`
- 英文相同的两对译文也逐字相同：`errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`、
  `errors.mount.authFailed` / `errors.shareList.authFailed`。引号 `“…”` 紧贴汉字，与 `errors.volume.permissionDenied`
  一致；`Cmdr`、`SMB 2`、`GVFS`、`smbclient` 与汉字之间留空格。

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `文本编辑器`** · Microsoft 术语库 `zh-Hans`（`text editor` → `文本编辑器`）·
  `high`。指这一类 App，❌ 不是 Apple 的"文本编辑"App（它的名字由 `{app}` 带进来）。
- **default text editor → `默认文本编辑器`** · 沿用目录里的 `默认编辑器`（`commands.fileEdit.label`）· `high`。
- **Edit files in [app] → `编辑文件时使用`** · 句子接到下拉菜单里的 App 名，与
  `settings.behavior.openTerminalHereApp.label` 的"……使用"同一结构 · `tentative`。
- `{app}` 前后留空格。Dismiss、Open settings 与 `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings` 一致。
- **system default → `跟随系统`** · 目录（`settings.appearance.language.opt.system`、
  `settings.appearance.dateTimeFormat.opt.system`）· `high`。`settings.behavior.textEditorApp.systemDefault`
  在后面用全角括号带上 App 名，同 `settings.appearance.language.opt.systemWithLanguage`。
- "Choose an app…"、"Checking your apps…" 与 `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking` 一致。提示（`fileExplorer.edit.hint`）沿用
  `commands.handler.openTerminalHere.hint` 的句式，但不写设置的位置，因为它的按钮直接跳过去。

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected（推出或卸载进行中）→ `正在断开连接`** · 沿用已定的 `disconnect → 断开连接`，加 `正在`
  表示进行中，同 Thunar `zh`（「正在卸载设备」/「正在弹出设备」）· `high`。「Left its index as it was」→「保持原样」，同
  `operationLog.rollback.partiallyRolledBackNotice`；「try again in a moment」→「请稍后重试」，同一文件里的
  `fileExplorer.pane.directConnectionMountNotRespondingToast`。

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected（已经发生，驱动器已不在）→ `断开了连接`** · 沿用已定的 `disconnect → 断开连接`，取完成态，同
  `indexing.staleDialog.body`（「在 {name} 断开连接期间」）· `high`。不用
  `fileExplorer.navigation.driveIndex.driveLeaving` 的进行态「正在断开连接」，那里弹出还没结束。「Starts from
  scratch」→「从头开始」，同 `indexing.rescan.incompletePreviousScan`；`扫描` 与 `文件夹大小` 取自同一文件，
  `indexing.staleDialog.body`、`indexing.staleDialog.bodyPhone`、`indexing.firstConnect.body` 也都写 `文件夹大小`。

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

四条句子的骨架都取自 `indexing.needsFreshScan.afterDisconnect` 的「{name} 在 Cmdr
…时断开了连接」，把驱动器留在句首，和英文一致。

- **was disconnected → `断开了连接`** · 沿用 `indexing.needsFreshScan.afterDisconnect`
  的完成态（英文是已发生的事，驱动器已经不在了）· `confirmed`。不用 `fileExplorer.navigation.driveIndex.driveLeaving`
  的进行态「正在断开连接」。
- **drive → `驱动器`；copy → `拷贝`；move → `移动`** · glossary 已定的词，同
  `errors.write.deviceDisconnected.message.copy` 与 `errors.write.deviceDisconnected.message.move` · `confirmed`。
- **{done} of {total} files → `{done} 个文件（共 {total} 个）`** · macOS Finder
  zh-CN 的「^0项（共^1项）」（`PW35`、`SB18`）就是这个形状；本目录
  `onboarding.wizard.stepProgress`（`共 {total} 步`）、`viewer.search.matchPosition` 用同一句式 ·
  `high`。两个占位符都是已格式化好的字符串，不套 ICU number。
- **the source/destination distinction（哪台驱动器掉线）**
  · 中文不重复「源/目标」这组词，靠介词区分：掉线的是目标盘时写「Cmdr 往它拷贝了…」，掉线的是源盘时写「Cmdr 往 {counterpart} 拷贝了…」·
  `high`。目录里 `来源`/`目标位置`
  （`fileOperations.transferDialog.sourceGroupTitle`、`errors.write.destinationExists.message`）是界面标签用词，这里塞进句子会变生硬，所以不用。Microsoft
  TBX 的 `source drive → 源驱动器` 备查，未采用。
- **your originals are untouched where they were → `你的原文件都还在原处，没有被动过`** · `原文件` 同
  `errors.write.destinationNotFound.message.copy`、`fileOperations.transferProgress.titleRemovingOriginals`；
  `没有被动过` 同 `errors.write.destinationNotFound.message.copy`；`还在原处` 同
  `errors.write.readOnlyDevice.source.suggestion`（「原文件会留在原处」）· `confirmed`。macOS Finder 写 `原始项目`
  （`PE117`），是 Apple 的「item」用词，本目录统一说 `文件`，故不采用。
- **so nothing is lost → `所以什么都没丢失`** · `丢失` 取自 macOS AppKit zh-CN 的
  `…将会丢失`；本目录此前没有这句安慰话，加 `什么都` 让它读起来是口语的宽心话，而不是一句状态播报 ·
  `high`。这半句是整条消息的重点，中文再短也不能省。
- **the rest are still on the drive → `其余的都还在这个驱动器上`** · `其余` 同
  `operationLog.rollback.partiallyRolledBackNotice`、`indexing.phase.wholeDrive`；`都`
  是补上去的，中文光说「其余的还在」容易读成随口一提，`都` 才把「一个没少」说满 · `high`。
- **all your files are still on {counterpart} → `你的文件都还在 {counterpart} 上`** · 同上，`都` 承担英文 `all` 的分量 ·
  `high`。这条按英文的设计不带计数：移动没做完就等于原件全在。

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

- **couldn't confirm → `无法确认`** · 目录里已成句式，同 `fileExplorer.pane.trashUnconfirmedToast`、
  `fileExplorer.rename.unconfirmed`、`askCmdr.renameUndo.skipReason.unverifiable.named`；Microsoft TBX `confirm → 确认`
  · `confirmed`。❗ 不许升级成「移动失败」：文件可能已经过去了，Cmdr 正是因为没法确认才把原件留着。
- **title `Couldn't confirm the move` → `无法确认这次移动`** · 与 `errors.write.readError.title.move`（`无法移动`）同为
  `无法…` 开头的标题；`这次` 取本目录偏口语的指示词（style.md 的 `这个`/`这次` 一条），同
  `fileOperations.transferProgress.rollbackAlreadyLandedTooltip`（「这次移动」）· `high`。
- **the moved files were saved on X → `移过去的文件是否已经保存到 X 上`** · 句式同
  `fileExplorer.pane.trashUnconfirmedToast`（「无法确认文件是否已移到废纸篓」）；`保存到` 取自 macOS AppKit
  zh-CN（`它将被保存到“%@”文件夹中`）· `high`。没用 `写入`，那是设备层面的技术说法，这条是说给用户听的。
- **it kept your originals where they were → `把你的原文件留在了原处`** · `原文件` + `留在…原处` 同
  `errors.write.readOnlyDevice.source.suggestion`、`fileOperations.cancelRollback.moveAlreadyLanded` · `confirmed`。
- **Have a look at the destination → `去目标位置看一下`** · `看一下` 同 `errors.write.newDataKeptAt.suggestion`、
  `errors.write.originalsKeptAside.suggestion.one`；`目标位置` 是 destination 的既定译法，同
  `errors.write.destinationFull.title` · `confirmed`。
- **Your originals haven't moved → `你的原文件还在原来的位置`** · `原来的位置` 同
  `fileOperations.rollbackConfirm.bodyUndoByMovingBack` ·
  `high`。直译「没有移动过」偏否定式，改成正面说它们在哪儿，和这一族「告诉你文件在哪」的语气一致。

## A staging folder kept after an unfinished move (`fileOperations.leftovers.stagingFolderKept`)

驱动器重新接上（或 Cmdr 启动）时的一条提示条：一次移动没走完，Cmdr 找到了它的工作文件夹，里面还装着用户的文件，就**原样不动**地留着。⚠️ 它跟
`fileOperations.cancelRollback.stagedLeftover.named`
那一族**不是一回事**：那边是 Cmdr 自己写了一半的残留，可以删；这边是**用户的文件**，可能是仅存的一份，所以既不能说「不完整」，也**绝不能**提删除。

- **found（不是搜出来的，是撞见的）→ `发现`** · 同 `errors.listing.symlinkLoopErrno.explanation`（「Cmdr 在 `{path}`
  发现了一串首尾相连的符号链接」），句首骨架也照搬它的「Cmdr 在 X 发现了…」· `high`。没用 `找到`：本目录里 `找到`
  归搜索结果和定位用（`search.imageResults.count`、`settings.adb.status.notFound`），这条不是用户找过东西。
- **unfinished（move）→ `没完成的`** · 同 `errors.volume.deviceDisconnected`（「改动还没完成」）、
  `indexing.rescan.incompletePreviousScan`（「上一次扫描没有完成」）· `high`。❗ 不用 `不完整`：那是
  `fileOperations.cancelRollback.stagedLeftover.named` 里 "incomplete
  copy" 的词，用在这儿会把用户完好的文件说成残品。口语的 `没做完` 也通，选 `没完成` 是为了跟上面两条对齐。
- **left them in place → `把它们都留在了原处`** · 几乎逐字取自今天同批的
  `errors.write.moveNotConfirmed.message.named`（「把你的原文件留在了原处」）；`原处` 有 Tier 1 佐证，macOS
  Finder 的 Put Back 就是 `放回原处` · `confirmed`。`都` 是刻意加的，同
  `errors.write.deviceDisconnected.sided.destination.copy` 那条的理由：中文光说「留在了原处」容易读成随口一提，`都`
  才把「一个没少」说满。保留 `把` 字句是因为英文强调这是 Cmdr 的**有意**之举，不是没来得及收拾。
- **hidden folder → `隐藏文件夹`** · `隐藏` 是已定的点文件义（`menu.view.showHiddenFiles` = `显示隐藏文件`、
  `commands.viewShowHidden.label`、`fileExplorer.rename.hiddenAfterRename`），`文件夹` 见术语表 ·
  `high`。说出「隐藏」是这条文案里唯一可操作的信息：不打开隐藏文件就看不到它。
- **a folder named {folderName} → `一个名为 {folderName} 的隐藏文件夹`** · `名为 … 的文件夹` 取自 Nautilus
  zh-CN（「此文件夹已包含一个名为 “%s” 的文件夹。」）；本目录 `errors.mount.shareNotFound` 也是 `名为…的共享` · `high`。
- **`{folderName}` 不加引号** · 规则是**跟着英文走**：英文加引号的键中文才加（`errors.mount.shareNotFound` 的英文写
  `"{share}"`），这条英文没引号，同一个占位符在 `transfer.appearedDuringMove` 里也是光板的 ·
  `high`。两个占位符都是拉丁字符，按 `style.md` § Numerals, punctuation, and
  spacing 两侧加空格（`在 {volumeName} 上`、`名为 {folderName} 的`）。
- **「in place, in a hidden folder…」这个同位语 → `…原处，也就是在…里`** · 英文用逗号同位，中文直接接一个 `在…里`
  会读成后加的状语（像是 Cmdr 把文件搬了进去）。`也就是在…` 明确它就是「原处」本身，事情的经过没变 · `high`。

## 个人收藏菜单（`commands.favoritesOpen.label`/`.description`、`commands.favoritesOpenByNumber.label`、`commands.favoritesAdd.description`、`fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`、`menu.go.showFavorites`、`shortcuts.scope.favoritesMenu`）

⌃D 在焦点窗格上拉出一个菜单，列出用户收藏的文件夹，前九行各带一个数字键 1–9，最后一行是 `0`
「把当前文件夹加入个人收藏」。宗卷选择器里原来的「个人收藏」分区没有了，换成顶部一行「查看 N 项个人收藏」，点它就切到这个菜单。

- **favorites（这份列表本身）→ `个人收藏`** · macOS Finder `FI10`/`TL4`/`SD8.1` 都是 `个人收藏`，`MN2`
  写「不能将服务器“^0”添加到你的个人收藏」；目录里 `fileExplorer.navigation.groupFavorites`、
  `fileExplorer.navigation.favoritesEmpty`、`commands.favoritesAdd.label`、`menu.go.addToFavorites` 已经全部用它 ·
  `confirmed`。❗ 这份列表只有这一个名字，别再造第二个（`收藏夹`、`书签`、`喜好`
  都不要）。这一族里只要英文出现 favorite/favorites，中文一律 `个人收藏`，包括单数的那一条
  `fileExplorer.navigation.favoritesAlreadyAdded`：中文不为单复数换词。
- **favorites menu（这个菜单）→ `个人收藏菜单`** · `菜单` 是目录里 menu 的既定说法（`commands.fileContextMenu.label` =
  `打开右键菜单`）· `high`。`shortcuts.scope.favoritesMenu` 就写 `个人收藏菜单`，长度和相邻的
  `shortcuts.scope.volumeChooser`（`宗卷选择器`）、`shortcuts.scope.commandPalette`（`命令面板`）一档。❗ 不要把它叫成
  `个人收藏选择器` 或 `个人收藏面板`：`选择器` 归宗卷选择器，`面板` 归命令面板，混了就是 `i18n-translation.md` §
  Auditing a finished locale 里讲的那种漂移（繁体曾经菜单写 `命令選擇區…`、面板标题写 `指令面板`）。
- **`显示` 与 `查看` 的分界（这批最容易被后人「改回去」的一处）** · 英文本来就是两个词，中文跟着分：
  - `Show favorites` → `显示个人收藏`，用在 `menu.go.showFavorites` 和
    `commands.favoritesOpen.label`。依据是同在「前往」菜单 / 命令族里的 `menu.go.showServers` = `显示服务器`（英文同为
    `Show …`），两条必须读起来是一家 · `high`。
  - `See {count} favorites` → `查看 {count} 项个人收藏`，只用在宗卷选择器顶部那一行
    `fileExplorer.navigation.seeFavorites`。`style.md` 已经定下「`查看` 是去看内容，`显示`
    是「显示」菜单」，这里英文也确实换了词 · `high`。
  - ❗ 所以这不是漂移，是英文自己的分界。别把 `查看 {count} 项个人收藏` 统一成 `显示`，也别反过来。
- **`seeFavorites` 的复数形状：只写 `=0` 和 `other`** · `new Intl.PluralRules('zh')` 只有 `other`
  一个 CLDR 类别（`style.md` § "Plurals"，2026-06-20 核实），中文名词也没有数的变化 · `confirmed`。英文那三条 `=0` /
  `one` / `other` 是英文的，别照抄。❗ 后来人别「补回」缺失的 `one`：`desktop-i18n-plural`
  要的是本语言需要的类别，多写一个 `one` 分支只会让同一句话有两个永远不会都命中的版本。 `=0`
  分支保留，写「还没有」那版措辞 `查看个人收藏`，且**不带** `{count}`；`other` 分支必须带 `{count}`。
- **量词用 `项`，不用 `个`** · `个人收藏` 前面接 `个` 会写成「{count} 个个人收藏」，叠字读着别扭；`项`
  在目录里已经管抽象条目（`queue.row.reversalInFolder`
  一族的「{countText} 项操作」、`fileExplorer.git.size.stashEntries` 的「{countText} 项贮藏」）· `high`。`{count}`
  是拉丁数字，按 `style.md` § Numerals, punctuation, and spacing 两侧加空格。
- **current folder → `当前文件夹`** · 目录里已经定下，同 `commands.favoritesAdd.description` ·
  `confirmed`。指的是窗格此刻所在的文件夹，不是光标所在的那个（那是 `光标所在的`，见上文术语表）。
- **Add current folder to favorites → `把当前文件夹加入个人收藏`，只此一个键** · 动词 `加入` 取自
  `commands.favoritesAdd.label`、`menu.go.addToFavorites`、`menu.context.addToFavorites`（均为 `加入个人收藏`）·
  `confirmed`。这一行只有 `fileExplorer.navigation.favoritesAddCurrent` 一个键：键盘快捷键列表是**引用**这一行来说明 `0`
  键，不是另写一句，所以读的是同一个值。以前那两个键中文本来就完全相同（英文只差一个冠词 the，中文没有冠词）。❌ 别再为快捷键列表另造一句。真正要守住的分界是它与
  `commands.favoritesAdd.label`（`加入个人收藏`，不带宾语）之间的那条。
- **Open the favorite with that number → `打开数字键对应的个人收藏`** · 键盘快捷键列表里的只读行，写法参照同为只读行的
  `commands.volumeSelectByName.label`（`按名称选择窗格宗卷`）· `high`。没用 `该数字`：`style.md` § Voice and
  tone要求口语的 `这个`/`这项` 而不是书面的 `此`/`该`，而「that number」在中文里本来也不需要指示词，说 `数字键`
  就够了（列表左边那一列正好显示 1–9）。
- **press a number to jump to that favorite → `按数字键前往对应的文件夹`** · `前往` 是目录里 go 的既定动词，但**中文的
  `前往` 必须带宾语**： `commands.navGoToPath.label`（`前往路径…`）、`commands.navParent.label`（`前往上层文件夹`）、
  `commands.fileGoToTrash.label`（`前往废纸篓`）无一例外，光写「按数字键前往。」会断在半空 · `high`。英文原先停在 "to
  go"，中文自己补出了落点；这条意见英文已经采纳，现在英文自己写出 "that favorite" 了。中文仍写 `对应的文件夹` 而不是
  `对应的个人收藏`：每一项个人收藏本来就是一个文件夹，意思完全一样，而这句开头已经有 `个人收藏菜单`，再重复一次会拖沓。
- **a mounted share → `已装载的共享`** · `装载` 有 Tier 1 佐证（macOS
  Finder：`Mount the volume and try opening the document again.` →
  `装载此宗卷并再次尝试打开该文稿。`），目录里也已定型：`fileExplorer.network.browser.noMountedShares` =
  `{hostName} 没有已装载的共享`、`settings.network.enabled.description`（`仍可在已装载的共享上读写文件`）·
  `confirmed`。❗ 别改成错误文案那一族的
  `挂载`（见上文术语表的分工），也别写出协议名：英文特意避开了 SMB/MTP/ADB，中文照办。
- **`favoritesCantAddHere` 说的是「这个文件夹」，理由放在冒号后面** ·
  `这个文件夹不能加入个人收藏：个人收藏只能用在磁盘和已装载的共享上` · `high`。开头跟姐妹行
  `fileExplorer.navigation.favoritesAlreadyAdded`（`这个文件夹已经在个人收藏里了`）一致，两条灰行读起来成对。❌不再用
  `指向`：那个动词逼着每种有格变化的语言为宾语选一个格，而且原来那句 `磁盘或已装载的共享上的文件夹` 连着两层
  `的`，换成「用在…上」这种处所说法干脆得多。用全角冒号，句末不加句号（悬停提示）。
- **`commands.favoritesAdd.description` 不再提「切换器的个人收藏」**
  · 那个分区 M3 已经删掉了，目录里的值不能描述一个不存在的界面。现在写
  `把焦点窗格的当前文件夹加入个人收藏，以后从个人收藏菜单就能回到这里。` · `high`。
- **This folder is already a favorite → `这个文件夹已经在个人收藏里了`** · 句式取自
  `main.dockPinNudge.added`（`Cmdr 已经在程序坞里了。`）、`fileOperations.transferDialog.pathErrorAlreadyThere`（`“{name}”已经在这个位置了`）·
  `high`。英文没有句号，中文也不加（悬停提示里的短句）。语气是陈述不是报错，所以没有 `无法`/`不能` 开头。
- **`menu.go.showFavorites`
  是 RAW 键**：用普通半角撇号（这条中文没有撇号，无影响），不带省略号 —— 它拉开的是菜单不是对话框，和
  `menu.go.showServers` 一样光板收尾，而 `menu.go.goToPath`（`前往路径…`）那种开对话框的才留 `…`。

## 推出被拒时说清是谁占着盘（`errors.eject.unmountRefusedByApp`/`…ByApps`/`otherApps`/`…ByDiskImage`/`…BySystem`/`…ByCmdr`）

`errors.eject.unmountRefused`
的六个具名分支：macOS 拒绝推出时，Cmdr 已经查出是谁占着盘，这几条就把名字说出来并给出唯一一个动作。都落在
`fileExplorer.pane.ejectFailedToast`（`无法推出 {volumeName}：…`）的冒号之后，所以全部承接上一句往下写，句尾沿用
`unmountRefused` 的 `…，然后再次推出。`。`errors.*` 是 **raw 族，不是 ICU**：撇号不加倍，`{app}` / `{apps}`
只是纯文本替换位。

- **is still using this drive → `还在使用这个驱动器`** · 与同族
  `errors.eject.unmountRefused`（`还有东西在使用这个驱动器`）、 `errors.eject.busy`（`Cmdr 还在读写…`）同一句式；macOS
  AppKit zh-CN 的 `The disk could not be ejected because it is in use by “%@”.` →
  `未能推出该磁盘，因为它正在被“%@”使用。` 是 Tier 1 佐证 ·
  `confirmed`。中文无主谓一致，单个 App 与多个 App 的谓语完全相同，两条只差 `它` / `它们`。
- **`{app}` 不加引号** · Apple 在这条里给 `%@` 加了 `“…”`，本目录跟英文走（英文源没有引号），同
  `fileOperations.leftovers.stagingFolderKept` 的 `{folderName}` · `high`。`{app}`
  是运行时读到的真实进程名（`Preview`、`Warp`、`mds_stores`），必然是拉丁字符：句中按 `style.md`
  的规则两侧留空格（`{app} 还在…`），句首紧跟全角冒号时不留（不与全角标点之间加空格）。
- **Close anything it has open there → `请关闭它在上面打开的内容`** · `关闭` 沿用 `unmountRefused` 的
  `请关闭打开的文件和 App`；`内容` 取 Apple 的泛指用法（`没有要发送的内容`，见 `style.md`）· `high`。没写 `文件`：英文的
  `anything` 故意比文件宽。没用 macOS `NE20` 的
  `请退出应用程序`（quit），英文说的是关掉它打开的东西，不是退出这个App。`在上面` 回指上一句的
  `这个驱动器`，避免第二次写出这个名词。
- **other apps（列表末位）→ `其他 App`** · `App` 保留拉丁形是本目录在**错误文案里**的既定写法（`ai.local.warningCaution`
  的 `其他 App 可能会变慢`、`errors.listing.deletePending.suggestion` 的 `关掉其他可能打开着这个文件的 App`、
  `errors.write.deletePending.suggestion`），也是 Apple zh-CN 自己的写法 · `confirmed`。 **不加量词**：`其他`
  直接修饰名词就够了，加 `几个` 会凭空给出英文没有的数量。 `Intl.ListFormat('zh')` 用 `、` 和 `和` 拼接且不加空格，实测
  `Preview、Warp、Photos和其他 App`（Node 24，2026-09-16），末位读起来正常，连接词绝不能写死在串里。
- **disk image → `磁盘映像`** · macOS Finder zh-CN `InfoWindowGeneralView`（`磁盘映像：`）、`BN53`
  （`将磁盘映像“^0”刻录到光盘…`），本目录 `errors.listing.readOnly.explanation`、
  `errors.listing.readOnlyVolumeErrno.explanation`、`settings.listing.sizeDisplay.description` 已在用 · `confirmed`。
- **is still open（磁盘映像）→ `还开着`** · 跟英文的口语 register 走：英文写 `open` 而不是 `mounted`，中文也用日常词 ·
  `high`。⚠️ 本目录 `updates.moveToApplicationsDialog.readOnlyVolume` 写作 `仍然挂载着的磁盘映像`，那条英文说的就是
  `mounted`，两者是有意的分界，别去统一。句式 `这个驱动器上有一个磁盘映像还开着`
  是存现句（`桌子上有一本书摊开着`），把驱动器留在句首，与本族其他几条一致。
- **Eject that image first, then eject this drive → `请先推出那个映像，再推出这个驱动器`** · 用 `先…再…` 这组关联词，比
  `然后` 更紧凑；第二次提映像时简写成 `那个映像`，紧接上文不会有歧义 · `high`。这条英文本身就没走 `eject again`
  的句尾，所以中文也不套本族的 `然后再次推出`。
- **macOS is still working with this drive → `macOS 还在处理这个驱动器`** · `处理` 而不是
  `使用`：占着盘的是系统的后台活儿（聚焦建索引、时间机器），跟 App「在用」不是一回事，英文也特意换了动词 ·
  `high`。`macOS` 保留原形，后面留空格。
- **Wait a minute / Wait a moment → `请稍等一会儿` / `请稍等片刻`** · 英文用了两个不同长短的等待，中文照分： `稍等片刻`
  同 `errors.write.deletePending.suggestion`、`errors.listing.resourceBusy.suggestion` · `high`。
- **Cmdr itself → `Cmdr 自己`** · 同 `fileExplorer.navigation.driveIndex.tooltipStalePhone` 的 `Cmdr 自己做的更改` ·
  `high`。这条是 Cmdr 自己的 bug，中文保持主语在 Cmdr 身上，不推给用户。
- **send a report → `发送一份报告`** · 同 `settings.updates.crashReports.description`（`自动发送一份报告`）·
  `high`。❗ 不写 `发送错误报告`（`menu.help.sendErrorReport` 的菜单项名）：英文这里故意用泛指的
  `a report`，而且 Cmdr 的文案不出现「错误」二字。
- **if it keeps happening → `如果一直这样`** · 本目录已成句式，见 `errors.listing.resourceBusy.suggestion`、
  `errors.listing.diskReadProblem.suggestion`、`errors.serverRequest.refused` 等九处 · `confirmed`。用全角分号 `；`
  接在前半句后面，对应英文的 `or`，比拆成第三句更贴原文。

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `种类`** · macOS Finder `zh`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `选择所有扩展名为 *.{extension} 的文件`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" → `选择所有扩展名相同的文件`) and
  Total Commander (`WCMD.INC` `527` → `选择扩展名相同的文件`) name this exact command, and the mask replaces their "same
  extension" because Cmdr shows the concrete one · `high`. 掩码是一段拉丁文本，按 `style.md`
  的规则两侧各留一个空格：`扩展名为 *.{extension} 的文件`。
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `选择`** · the catalog's settled noun for the
  SET of selected files, from `commands.selectionSelectFiles.description`（`将匹配的文件加入选择`） · `high`. Its
  siblings in that menu are verbs; this one names what the submenu holds. `menu.bar.select` 也是
  `选择`，两者相同；中文不作名动之分，右键子菜单标题与「选择」菜单同名反而更一致。
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`)

标题栏里的警告小标签和它的悬停提示，在 Cmdr 还没拿到完全磁盘访问权限时显示；点按会把入门引导重新打开到第 1 步。

- **`onboarding.fdaBadge.label` → `没有完全磁盘访问权限`** · Apple 自己的「系统设置」条目名（见上面的 full disk
  access 条目；在 `SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable` 的 `ALL_FILES`
  键上再次核对过，macOS 27.0 build 26A428，2026-09-16 验证）· `high`。
- **`没有…`，不是更短的 `无…`** · `onboarding.stepAi.bannerTitle.denied` 的英文和它完全相同，`i18n-terms`
  会要求两者一字不差；那一条早就用 `没有`，而且口语化的 `没有` 也更贴本指南的语气。❌ 两条不能单独改词。
- **`onboarding.fdaBadge.ariaLabel`
  以标签原文开头**（`没有完全磁盘访问权限。打开入门引导的完全磁盘访问权限步骤。`），这正是 `i18n-aria`（WCAG
  2.5.3）要的包含关系。❌ 单独改标签会把它弄坏。
- **提示里的其他词**：drive → `驱动器`（已定），cloud folders → `云端文件夹`，"files macOS keeps to itself" →
  `macOS 自己留着的文件`（说人话，❌ 不要点名某个 macOS 功能），"Click to …" → `点按…`（同
  `fileExplorer.breadcrumb.navigateTooltip`），onboarding → `入门引导`（已定）。

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Chinese has no
  number agreement, so all three messages lead with `你选中的项目里有 {count} 个，…`, which reads the same at 1 as at 7.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails` (`技术详情`),
  because it points at that very control.

- **locked → `已锁定`** · macOS Finder（`AXNODE1` `已锁定`）与目录既有写法 · `high`。
- **"delete them permanently" → `彻底删除`** · 与 `commands.fileDeletePermanently.label`
  一致，让建议里的说法就是用户要按的那个命令 · `high`。
- **badge（标题栏上的那个小药丸）→ `标记`** · `tentative`；描述性说法，语料里没有对应词。title bar → `标题栏` · `high`。
- 引号里的徽章文字逐字取自 `onboarding.fdaBadge.label`，用目录通行的
  `“…”`。❌ 单独改一边，这句话就会指向一个写着别的字的标记。
- "somewhere macOS keeps to itself" → `macOS 自己留着的地方`，沿用 `onboarding.fdaBadge.tooltip`
  已定的说法。说人话，❌ 不要点名某个 macOS 功能。
- `{count}` 前后留半角空格（`有 {count} 个`），与目录里其他数字占位符一致。

## 仅存云端内容的删除警告条（`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`）

云文件夹里被选中的项目如果仅存在云端，放进废纸篓得先把它下载回来。所以 Cmdr 改开永久删除的对话框，并在警告条里说清楚。警告条有两个版本：一个用于混合选择，一个用于全部仅存在云端的选择。两者只在第一句和能给出的出路上不同。第三个键是 Cmdr 把一次按键交还给用户时显示的那行字。

- **`.cloudOnlineOnlyMixedWarning`**
  · 「仅存在云端」对应 Finder 对被清出本地的文件的说法（`commands.cloudRemoveDownload.description` 也写
  `让它仅存在云端`）；「废纸篓」「云服务」取自 `terms.json`。移到废纸篓用固定搭配 `移到废纸篓`，出路写
  `设为离线可用`，与 `commands.cloudMakeOffline.label` 同字 · medium。
- **`.cloudOnlineOnlyAllWarning`**
  · 同一段文字，只把「你选中的内容里有一部分」换成「你选中的内容全都」，并去掉「取消选择」这条出路：全部仅存在云端时，取消选择就什么都不剩了 ·
  medium。
- **`.cloudOnlineOnlyHandedBack`** · 按钮上方的那行字，出现在 Cmdr 有意没有执行的一次按键之后。语气平实，不必道歉 ·
  medium。
- **四个事实都得保留**：（1）废纸篓会先下载文件，（2）所以 Cmdr 只提供删除整个选择，（3）之后废纸篓里没有副本，但云服务自己有（❌ 不要写成「反正还在废纸篓里」），（4）警告条里点出的出路。
- **两处 `<strong>` 必须保留**，分别在「先下载回来」和动词「删除」上。引号里的「删除」是按钮的文字：始终与
  `fileOperations.delete.confirmDelete` 一致。
- 做溢出检查时看一下：警告条很长，且位于文件列表上方的窄条里。

## 服务器明确说没有这个共享（`fileExplorer.network.osMountFallback.shareNotOnServer`、`fileExplorer.pane.directConnectionShareNotOnServerToast`）

这一族里唯一「再试也没用」的情况：服务器给出明确答复，上面没有这个名字的共享。所以这条通知不带按钮，语气里也不能有任何「暂时」的暗示（不写
`暂时`，不写 `再试一次`），这正是它和兄弟键的区别。

- **"the server says it has no share by that name" → `因为服务器说它没有这个名字的共享`**
  ·目录（`errors.mount.shareNotFound`）与 NetAuthAgent
  `EINFO_NO_SHARE`（系统里装着的 bundle：`服务器上不存在共享“%@”。`，macOS 26.6.2，25G83，2026-09-17）·
  `high`。开头沿用兄弟键 `fileExplorer.network.osMountFallback.message` 的 `无法直接连接到 X`。
- **"This one won't sort itself out" → `这种情况等下去也不会好`**
  · 语料里没有对应说法；这是中文口语里最自然的讲法，也正好点出这条通知和别的不一样：等不来结果 · `high`。❌ 不写
  `不会自动恢复`，那读起来像系统故障播报。
- **"may have been renamed or removed" → `可能……被重命名或删除了`** · 逐字沿用
  `errors.write.destinationNotFound.suggestion` · `high`。
- **"so it's worth checking there" → `值得去那边看看`** · `那边` 回指服务器，省掉重复 · `high`。
- **"a lot slower" → `慢得多`** · 这里英文没给倍数，和给了 `4 倍` 的兄弟键不同 · `high`。
- **短提示省略主语（`所以继续使用系统连接`）** · 前半句已经把话题定在共享上，中文再写一次 `此共享` 就啰嗦了；结尾
  `继续使用系统连接` 与 `fileExplorer.pane.directConnectionUnreachableToast` 等三条一致 · `high`。

## 服务器上「看起来一样」的名称（`fileOperations.transferProgress.lookAlikeHint`、`errors.listing.ambiguousName.explanation`、`errors.volume.ambiguousName`）

两个名称在屏幕上一模一样，只是服务器存的字符不同（带重音的字母两种存法，或大小写不同）。这一族共用同一套说法，改一处要连着看另外两处。

- **"look the same, but the server spells them differently / stores them spelled differently" →
  `看起来一样，但服务器存储的写法不同`** · 两个键逐字一致。`写法` 是口语里「拼法」的说法；❌ 不写 `编码`、`规范化` 或
  `Unicode`（英文描述明确要求不用技术词）。语料里没有这个概念（Finder 只有 `忽略大小写`、`区分大小写`），所以是
  `tentative`。
- **upper and lower case** · `大小写`（`区分大小写`）· macOS AppKit `忽略大小写`，目录里 `viewer.search.caseSensitive` ·
  `high`。
- **"More than one item on the server matches X" → `服务器上有不止一个项目匹配 X`**
  · 两个错误键同一句式，路径不再放句首；`errors.listing.*` 里的 `{path}` 用反引号并两侧留空格，`errors.volume.*` 里用
  `“{path}”` 不留空格 · `high`。
- **"Overwrite" 在句中指按钮** · 写成 `“覆盖”`，与 `fileOperations.transferProgress.conflictOverwrite`
  完全一致；按钮在正文里加 `“…”` 的先例是 `adb.connect.unauthorized`（`点按“允许”`）· `high`。
- **"the one that's there" → `现有的那一项`** · `现有` 与同一对话框的
  `fileOperations.transferProgress.existingFolderLabel`（`现有（文件夹）：`）一致；`项`
  回指「项目」，文件和文件夹都适用 · `high`。
- **错误正文里的「改名」「上一级文件夹」** · `errors.listing.ambiguousName.suggestion` 沿用 `errors.json`
  里已有的口语说法（`先给其中一个改名`，`进入上一级文件夹`），不换成按钮/菜单用的
  `重命名`、`上层文件夹`：正文和标签分开 · `high`。

## “允许使用云端 AI”开关，以及云端 AI 关闭时的提示（`ai.cloudConsent.*`、`askCmdr.gate.*`）

这是一个隐私同意开关：默认关闭，打开之前什么都不会离开这台 Mac。文字要平静，绝不能夸大 Cmdr 做的事。M1 上没有参考资料库，证据取自已安装的 macOS，做法见
`docs/i18n/reference-pile/how-to-mine.md` 的 "No pile on this machine?" 一节。

- **Allow cloud AI（开关标签，`ai.cloudConsent.label`）** · `允许使用云端 AI` · `Allow` → `允许` 是 macOS
  zh-CN 权限请求里的按钮（`TCC.framework` `Localizable.loctable`，`REQUEST_ACCESS_ALLOW`，macOS 26.6.2 build
  25G83，2026-09-23读取）；`云端 AI` 是 `settings.ai.provider.opt.cloud` 已发布的值，也就是紧挨着上方的那个选项。加
  `使用`，因为 `允许云端 AI` 读起来缺个动词 · `high`
- **云端 AI 关闭的状态** · `云端 AI 已关闭`，与 `ai.translateError.off.title`（`AI 已关闭`）同一形式 · `high`
- **引用开关时逐字照搬标签。** `settings.ai.cloudConsent.lockedHint` 加引号写 `“允许使用云端 AI”`；
  `askCmdr.gate.cloudOff.body` 和 `settings.askCmdr.cloudOffHint` 在句中直接写
  `允许使用云端 AI`，与标签一字不差。英文只说“Allow it” 的地方写 `允许使用`（`请到“设置 > AI”中允许使用`）。
- **Settings > AI** · `“设置 > AI”`，加引号、用 `>`，与 `ai.translateError.*` 的兄弟字符串一致。
- **AI service / cloud AI service** · `AI 服务` / `云端 AI 服务`；英文把 `service` 和 `ai.cloudConsent.askCmdr.*` 里的
  `provider`（`提供方`）分开，中文也分开。
- **custom endpoints** · `自定义端点` · `high`
- **side panel** · `侧边面板` · `high`
- **折叠区里的功能名（`<b>` 内）**：`新建文件夹的名称建议`、`用自然语言搜索`（沿用 `queryUi.bar.aria.ai` 的
  `自然语言`）、 `按描述选择` · `high`
- `settings.askCmdr.enabled.label` = `Ask Cmdr`，与英文相同，带 `sameAsSourceJustification`（产品名，同
  `settings.section.askCmdr`）。

## 移动过程中被改动的原文件（`transfer.changedDuringMove`，2026-09-25）

- **changed during the move → `移动过程中有 … 发生了改动`** · Finder `PE56` 「在刻录时一个或多个项目发生更改」(Finder
  `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25)； `改动`
  是本目录已定的词（`fileOperations.cancelRollback.reason.drift.counted` 的 `有改动`）· `high`。句首的 `移动过程中有` 和
  `等源文件夹` 照搬同一条提示里的 `transfer.appearedDuringMove`。
- **占位符两侧加空格**：`留在 {scope…} 中`（style.md § Numerals, punctuation, and spacing）。顺手把
  `transfer.appearedDuringMove` 原来的 `出现在{scope…}中` 也改成加空格的写法，两句同框时才一致。

## “打开方式”和“共享”子菜单里的等待行（`menu.context.openWithLoading`、`.shareLoading`、`.shareNone`，2026-09-24）

## “打开方式”和“共享”子菜单里的等待行（`menu.context.openWithLoading`、`.shareLoading`、`.shareNone`）

- **Finding apps…** · `正在查找 App…` · `正在…` 模式（macOS `Searching…` → `正在搜索…`）；`App` 同
  `settings.behavior.textEditorApp.checking`，中英之间加空格 · `high`
- **share options** · `共享选项` · `共享` 即子菜单名 · `high`
- **No share options** · `没有共享选项` · macOS 空菜单的说法（`No Services Apply` → `没有服务可应用`） · `high`

## Apple app and product names Apple localizes (`settings.advanced.showSafeSaveFiles.description`, `errors.provider.iCloud.*`, `errors.listing.emptyRootICloud.*`, `shortcuts.system.finderSearch`)

What a zh-CN Mac shows wins over the English name, the same rule as Quick Look → `快速查看` and Dock → `程序坞`:

- **iCloud Drive → `iCloud 云盘`** · macOS zh-CN writes `iCloud云盘` (49 hits in the pile, `iCloud Drive` 5), with
  Cmdr's Latin/Han space · `high`. Third-party products (Google Drive, Box Drive, pCloud Drive, Synology Drive, Proton
  Drive) keep their Latin names.
- **TextEdit / Preview → `“文本编辑”` / `“预览”`** · Apple's zh-CN app names, quoted in running text like
  `“钥匙串访问”`.
- **Finder stays Latin** everywhere, including the reserved-shortcut list (`Finder 搜索窗口`); see § 原生菜单.

## Whole-catalog wording rules checked by the termbase (`viewer.saveAs.*`, `viewer.copyDialog.*`, `askCmdr.renameReview.evidence.*`, `settings.onboarding.completed.label`, `onboarding.wizard.restart`, `onboarding.stepFda.postAction.body`)

Rules the termbase drift check holds the catalog to, with the keys that last strayed:

- **Save → `保存`, never `存储`** (Apple's older word): the viewer's save-selection family (`viewer.saveAs.*`,
  `viewer.copyDialog.saveAsFile`, `.refuseBody`) says `保存`. `存储` stays for storing data on a drive
  (`无法存储大于…的文件`).
- **Copy → `拷贝`, never `复制`** outside Duplicate: `errors.write.readOnlyDevice.source.*`,
  `servers.sheet.addressHelp`, `settings.archives.compressionLevel.description`,
  `settings.advanced.showStagingTempFiles.description`.
- **Image indexing → `图像`, never `图片`**, including the rename-review evidence labels
  (`askCmdr.renameReview.evidence.imageText` / `.imageTags`).
- **Onboarding → `入门引导`** in full, internal labels included (`settings.onboarding.completed.label` =
  `已完成入门引导`).
- **Restart buttons take Apple's `重新启动`**: `onboarding.wizard.restart` = `重新启动 Cmdr`, and the sentence that
  quotes it (`onboarding.stepFda.postAction.body`) matches it verbatim. Running prose may say `重启`.
- **No `失败` / `错误` as a label**: `askCmdr.decision.result` counts `{failedText} 个无法完成`,
  `onboarding.stepOptional.mtp.desc` says macOS `通常连不上`, and a typo hint says `有没有打错`
  (`licensing.error.badSignatureHint`).
- **Dock → `程序坞`** in error suggestions too (`errors.listing.diskFullErrno.suggestion`,
  `errors.listing.storageFull.suggestion`).
- **Credentials → `凭证` or `登录信息`**, never `凭据` (`errors.listing.remotePermissionDenied.suggestion`); **AI
  provider → `提供方`**, never `提供商` (`settings.mediaIndex.privacyNote`); **Apple silicon → `Apple 芯片`**
  (`ai.local.notInstalled`, `onboarding.stepAi.localTooltip`).
- **On-disk size → `占用磁盘`** in every description that contrasts it with content size
  (`settings.listing.sizeDisplay.description`, `settings.listing.sizeMismatchWarning.description`).
- **Make available offline → `设为离线可用`** in the command and in the warnings that point at it; online-only →
  `仅存在云端` (`commands.cloudRemoveDownload.description`).
- **The folder count scanned by a search is `已扫描`** (`queryUi.results.live.foldersScanned`), the scan verb, not
  `已查看`.
