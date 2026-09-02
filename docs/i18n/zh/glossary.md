# zh glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/zh-CN/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

Core file/UI terms (Trash, copy, move, open, settings, etc.) live in `style.md` § Terminology and glossary; this file
adds the terms settled while translating the catalogs. All `zh-Hans` (Simplified).

### Settings catalog (first pass, 2026-06-21)

- **Appearance** · `外观` · macOS SystemSettings, universal · `confirmed`
- **Behavior** · `行为` · standard · `high`
- **Language** · `语言` · macOS, Microsoft · `confirmed`
- **theme** · `主题` · standard · `high`
- **theme mode (Light / Dark / System)** · `浅色` / `深色` / `跟随系统` · macOS appearance modes (浅色/深色 are the
  Finder/System Settings labels), Microsoft `浅色`/`深色` · `confirmed`
- **notification** · `通知` · macOS, Microsoft · `confirmed`
- **tint (faint background color)** · `着色` (action) / tint-name swatches keep color names · macOS `着色`; Microsoft
  TBX `淡色` is the alt sense · `high`
- **pane** · `窗格` · macOS, Microsoft · `confirmed`
- **tab** · `标签页` · macOS, Microsoft · `confirmed`
- **search** · `搜索` · macOS (Simplified) · `confirmed`
- **settings** · `设置` · macOS (Simplified) · `confirmed`
- **preview** · `预览` · macOS · `confirmed`
- **provider (AI service provider)** · `提供方` · generic Chinese term (Microsoft TBX `提供方` for service-provider
  sense) · `high`
- **service** · `服务` · standard · `high`
- **server** · `服务器` · macOS · `confirmed`
- **share (network share)** · `共享` · macOS Finder (`共享`) · `confirmed`
- **connect to server / connection** · `连接服务器` / `连接` · macOS Finder · `confirmed`
- **network** · `网络` · macOS, Microsoft · `confirmed`
- **mount (a share)** · `装载` · Microsoft TBX; macOS uses 连接/装载 · `high`
- **drive** · `驱动器` · Microsoft, macOS · `confirmed`
- **index / indexing** · `索引` (noun) / `建立索引` (verb) · Microsoft TBX `索引` · `high`
- **cache** · `缓存` · Microsoft TBX · `confirmed`
- **timeout** · `超时` · Microsoft TBX · `confirmed`
- **port** · `端口` · macOS, Microsoft · `confirmed`
- **buffer** · `缓冲区` · Microsoft TBX · `high`
- **threshold** · `阈值` · Microsoft TBX · `confirmed`
- **default** · `默认` · macOS · `confirmed`
- **reset / reset to default** · `重置` (`恢复默认`) · macOS `还原`/`恢复默认`; `重置` is the common modern term ·
  `high`
- **advanced** · `高级` · macOS · `confirmed`
- **custom** · `自定义` · macOS · `confirmed`
- **updates** · `更新` · macOS, Microsoft · `confirmed`
- **privacy** · `隐私` · macOS, Microsoft · `confirmed`
- **license** · `许可证` · Microsoft TBX · `high`
- **word wrap** · `自动换行` · Microsoft TBX · `confirmed`
- **logging** · `日志` · Microsoft TBX (`记录`/`日志`) · `high`
- **verbose** · `详细` · Microsoft TBX `详细的` · `high`
- **context window** · `上下文窗口` · standard AI term · `high`
- **token (AI)** · `token` (kept Latin) · no settled Chinese UI term; kept verbatim · `tentative`
- **regex** · `正则表达式` · standard · `confirmed`
- **toast (transient notification)** · `提示` · rendered by meaning, not transliterated · `high`

### UI section names (keep consistent across catalogs)

- Appearance `外观`; Behavior `行为`; AI `AI`; File systems `文件系统`; SMB/Network shares `SMB/网络共享`; MTP `MTP`;
  Git `Git`; Viewer `查看器`; Developer `开发者`; Updates & privacy `更新与隐私`; Advanced `高级`; Keyboard shortcuts
  `键盘快捷键`; License `许可证`.
- View modes: Full `完整`; Brief `简洁`. Columns: Name `名称`; Ext `扩展名`.

### Errors catalog (first pass, 2026-06-21)

macOS Finder/AppKit zh-CN as Tier 1, Microsoft zh-Hans cross-check. Reuses settings-pass terms where they overlap.

- **volume (mounted disk)** · `宗卷` · macOS (mounted-disk sense, NOT audio `音量`) · `high`
- **mount / unmount (a FUSE or network volume, error context)** · `挂载` / `卸载` · general IT + Microsoft. NOTE: the
  settings pass settled `装载` for "mount a share"; in the error copy (force-unmount, remount, FUSE) `挂载`/`卸载` reads
  more naturally and is the dominant modern term. Both are understood; pick by context. · `high`
- **network drive** · `网络驱动器` · Microsoft (consistent with settings `驱动器`) · `high`
- **disk** · `磁盘` · macOS, Microsoft · `confirmed`
- **device** · `设备` · macOS, Microsoft · `confirmed`
- **host** · `主机` · Microsoft TBX · `high`
- **symbolic link / symlink** · `符号链接` · Microsoft TBX, general · `high`
- **quota** · `配额` · Microsoft TBX · `high`
- **credentials** · `凭证` · Microsoft TBX · `high`
- **handle (open file handle)** · `句柄` · Microsoft TBX · `confirmed`
- **read-only** · `只读` · macOS, Microsoft · `confirmed`
- **permission denied / no permission** · `无访问权限` / `没有权限` · macOS-style phrasing · `high`
- **path** · `路径` · macOS, Microsoft · `confirmed`
- **Disk Utility / First Aid** · `磁盘工具` / `急救` · macOS · `high`
- **Activity Monitor** · `活动监视器` · macOS · `high`
- **Spotlight (the search)** · `聚焦` · macOS · `high`
- **Get Info** · `显示简介` · macOS Finder · `high`
- **Sharing & Permissions** · `共享与权限` · macOS Finder Get Info · `high`
- **Storage (System Settings pane)** · `储存空间` · macOS · `high`
- **Apple Account** · `Apple 账户` · macOS (Sonoma+) · `high`
- **Technical details (error-panel section)** · `技术详情` · descriptive, no macOS source · `tentative`
- **App (application, in cloud-provider copy)** · `App` · Apple zh-CN keeps "App" verbatim · `high`
- **System Settings panes via tokens** · rendered by `{system_settings}`/`{privacy_and_security}`/`{files_and_folders}`/
  `{full_disk_access}`, OS-localized at runtime; never hand-translate. Every `errors.*` suggestion uses the tokens,
  including the git and provider ones; no `errors.*` string writes a pane name as a literal. Spacing rules and the pane
  names the tokens don't cover: § Shared `en` fixes (2026-08-30) at the end of this file. · `high`

### File explorer catalog (first pass, 2026-06-21)

macOS Finder/AppKit zh-CN Tier 1 (KEY-based en→zh lookup), Double Commander zh-CN for two-pane terms, Microsoft zh-Hans
cross-check. Aligned to the settled `窗格`/`标签页` above (DC's `面板` was rejected to stay consistent with the settings
pass).

- **file pane** · `文件窗格` · uses settled `窗格` (NOT DC's `面板`) · `high`
- **file list** · `文件列表` · DC (`file list` → 文件列表) · `high`
- **favorites** · `个人收藏` · macOS Finder (FI10 `Favorites` → 个人收藏) · `confirmed`
- **eject** · `推出` · macOS (TL15/N199 → 推出) · `confirmed`
- **sign in / log in** · `登录` · macOS (NE104 `Sign In…` → 登录…) · `confirmed`
- **guest** · `来宾` · Microsoft TBX · `high`
- **credentials** · `登录信息` · reused from settings pass; the errors pass uses `凭证` (both understood, pick by
  surface: sign-in copy → 登录信息, low-level error copy → 凭证) · `high`
- **authentication failed** · `无法通过身份验证` · style guide (no bare 失败/错误); macOS "authentication needed" is
  `需要认证` (CS203) · `high`
- **password / username** · `密码` / `用户名` · macOS (N15 密码), Microsoft 用户名 (NOT MS password→`访问代码`) ·
  `confirmed`
- **Keychain** -> `钥匙串` · macOS Chinese (Simplified) · `high` · the localized Apple FEATURE name (Apple localizes it
  per-OS, so Cmdr uses the term the user sees, not the English "Keychain"); same Decision-1 rule as Quick Look. The
  credential store is `钥匙串` (`macOS Keychain` → `macOS 钥匙串`); the **Keychain Access** app is `钥匙串访问`. (The
  Finder/AppKit/SystemSettings reference pile doesn't surface the term — those apps don't mention Keychain — but
  `钥匙串` / `钥匙串访问` are the established Apple Chinese (Simplified) names.) Supersedes any earlier "keep Keychain
  verbatim" note.
- **host / hostname** · `主机` / `主机名` · Microsoft TBX · `high`
- **disconnect** · `断开连接` · macOS (N200/MR10.1) · `confirmed`
- **read-only volume** · `只读宗卷` · macOS FI12 `read-only` → 只读, + 宗卷 · `high`
- **on disk (vs content size)** · `占用磁盘` · macOS "X on disk" → 占用磁盘空间; shortened to 占用磁盘 for the tight
  label · `tentative`
- **Quick Look** -> `快速查看` · macOS Chinese (Simplified) · `high` · the localized Apple FEATURE name (Apple localizes
  it per-OS, so Cmdr uses the term the user sees in Finder, not the English "Quick Look"). macOS Finder `TL14`/`N169.*`
  and AppKit `NSQuickLookTemplate` both render `快速查看`; "close Quick Look" → `关闭快速查看`. quick-view/quick-preview
  sense also `快速查看`/`快速预览`.
- **MTP device** · `MTP 设备` · keep MTP verbatim · `confirmed`
- **dir (status-bar abbrev. for directory/folder)** · `目录` · standard · `high`
- Function-key bar verbs: 拷贝 / 移动 / 重命名 / 删除 / 查看 / 编辑 / 新建文件 / 新建文件夹 / 彻底删除 (彻底 for
  "permanently") · macOS verbs · `high`
- Volume-switcher groups: Favorites `个人收藏` · Volumes `宗卷` · Cloud `云` · Mobile `移动设备` · Network `网络`

### File operations + onboarding catalog (first pass, 2026-06-21)

macOS zh-CN Tier 1 (key-based en→zh), Double Commander + GNOME Nautilus zh-CN for conflict-dialog verbs, Microsoft
zh-Hans cross-check.

- **overwrite** · `覆盖` · DC (`Confirm overwrites` → 确认覆盖), Nautilus · `high`
- **replace** · `替换` · macOS AppKit SavePanel (`Replace` → 替换); Cmdr's transfer dialog uses `覆盖` (overwrite sense)
  · `high`
- **skip** · `跳过` · DC + Nautilus (`Skip` → 跳过) · `confirmed`
- **rename** · `重命名` · DC + macOS function-key bar · `confirmed`
- **merge** · `合并` · Nautilus (`Merge` → 合并) · `confirmed`
- **retry** · `重试` · Nautilus (`Retry` → 重试) · `confirmed`
- **rollback (undo partial transfer)** · `回滚` · Microsoft TBX (`roll back` → 回滚) · `high`
- **conflict** · `冲突` · Microsoft TBX · `high`
- **hard link / hardlinked** · `硬链接` · Microsoft TBX · `high`
- **stop / cancel** · `停止` / `取消` · macOS AppKit · `confirmed`
- **close** · `关闭` · macOS AppKit (`Close` → 关闭) · `confirmed`
- **OK (affirmative button)** · `好` · macOS convention (Apple uses `好` for OK) · `high`
- **trash (verb, move to trash)** · `移到废纸篓` · macOS Finder (`Move to Trash`) · `high`. Trash noun stays `废纸篓`
  (style.md).
- **under cursor** · `光标所在的` · descriptive, no single macOS source · `tentative`
- **all (in "Skip all"/"Overwrite all")** · `全部` · Chinese collapses ICU one/other to `other`, so the single-conflict
  case also renders `全部跳过`/`全部覆盖`; chosen because the policy radios act on the whole conflict set · `high`
- **technical details** · `技术详情` · reused from errors pass · `high`

### Onboarding catalog terms

- **onboarding** · `入门引导` · macOS-flavored (`引导`/`入门` are the Apple setup-flow words) · `high`
- **full disk access** · `完全磁盘访问权限` · macOS Ventura+ Privacy pane label (Simplified) · `high`. Pane breadcrumb
  uses errors-pass `隐私与安全性` + `系统设置` (the `{systemSettings}` token).
- **Quit & Reopen (macOS relaunch dialog button)** · `退出并重新打开` · macOS (`Quit` → 退出, `Reopen` → 重新打开) ·
  `high`
- **Applications (folder)** · `应用程序` · macOS Finder (`Applications` → 应用程序) · `confirmed`
- **deny / allow (permission)** · `拒绝` / `允许` · macOS permission-dialog verbs · `high`
- **agent (AI assistant)** · `代理` · standard · `high`
- **API key** · `API 密钥` · macOS/Microsoft (密钥 = key) · `high`
- **model (AI model)** · `模型` · Microsoft TBX (`model` → 模型) · `high`
- **endpoint** · `端点` · Microsoft TBX · `high`
- **command palette** · `命令面板` · standard · `high`
- **open beta** · `公开测试` · standard · `high`
- **Local network access / Accepting incoming connections (macOS prompt labels)** · `本地网络访问` / `接受传入连接` ·
  macOS firewall/privacy prompt wording (not in this pile slice; standard macOS labels) · `tentative`

### Search UI + commands catalog (first pass, 2026-06-21)

macOS Finder/AppKit zh-CN Tier 1 (KEY-based en→zh lookup), Microsoft zh-Hans cross-check. Reuses
settings/errors/explorer terms where they overlap (`窗格`/`标签页`/`搜索`/`宗卷`/`主机`/`驱动器`/`索引`/`路径`).

- **search query / query (noun)** · `查询` · standard (matches the command-palette/search domain) · `high`
- **run (a search)** · `运行` · Microsoft TBX (`run` → 运行); reused for "run search"/"execute command" · `high`
- **results** · `结果` · standard; "previous/next result" → `上一个/下一个结果` · `confirmed`
- **scanning / scan in progress** · `正在扫描` · macOS Finder (`Searching…` → 正在搜索 pattern; scan = 扫描) · `high`
- **entry (indexed file count)** · `条目` · standard measure-word noun for index entries (`{count} 个条目`) · `high`
- **filter (noun/verb)** · `筛选` · macOS/Microsoft (`Filter` → 筛选) · `confirmed`
- **pattern (match pattern)** · `模式` · standard · `high`
- **glob** · `Glob` · no settled Chinese UI term; kept verbatim like the brand row label (matches en intent) ·
  `tentative`
- **comparator (filter operator)** · `比较符` · descriptive; standard math/IT term · `high`
- **scope (search scope) / "Search in"** · `搜索范围` · descriptive; matches macOS "Search:" scope row intent · `high`
- **case-sensitive** · `区分大小写` · macOS/Microsoft standard · `confirmed`
- **wildcard** · `通配符` · macOS/Microsoft standard · `confirmed`
- **coming soon** · `即将推出` · standard product phrasing · `high`
- **refine (AI search)** · `优化` · rendered by meaning (improve the query) · `tentative`
- **agent (AI agent, transparency-strip voice)** · `代理` · reused from onboarding pass (glossary consistency; no
  special case). NOTE: the en uses a deliberate first-person "agent" voice; `代理` carries it. `智能体` (the modern
  Chinese "AI agent" term) was considered but rejected to stay consistent with the settled `代理`. · `high`
- **zoom (UI text size)** · `缩放` (verb in/out → `放大`/`缩小`) · macOS AppKit (`Zoom` → 缩放) · `confirmed`
- **clipboard** · `剪贴板` · macOS/Microsoft standard · `confirmed`
- **copy to clipboard / cut / paste** · `拷贝` (Finder copy verb) / `剪切` / `粘贴` · macOS AppKit MenuCommands (`Cut`
  → 剪切, `Paste` → 粘贴, `Select All` → 全选). NOTE: F5/F6 transfer ops keep the function-key-bar `拷贝`/`移动`;
  clipboard ops use `拷贝到剪贴板`/`剪切`/`粘贴`. · `confirmed`
- **select all / deselect all** · `全选` / `取消全选` · macOS (`Select All` → 全选) · `confirmed`
- **select / deselect (the bare verbs)** · `选择` / `取消选择` · macOS Finder `zh-CN`, MS `zh-Hans` TBX · `high`. Full
  evidence, and why Traditional says `選取`/`取消選取` on the same Microsoft entry, in § 选择/取消选择文件对话框.
- **ascending / descending** · `升序` / `降序` · standard sort terms · `confirmed`
- **sort by / sort order** · `按…排序` / `排序方向` · macOS Finder (`Sort By` → 排序方式) · `high`
- **swap / switch (panes/tabs)** · `交换` / `切换` · standard · `high`
- **refresh** · `刷新` · macOS AppKit (`refresh` → 刷新) · `confirmed`
- **reopen (tab)** · `重新打开` · macOS (`Reopen` → 重新打开) · `confirmed`
- **parent folder** · `上层文件夹` · macOS Finder (`Enclosing Folder` → 上层文件夹) · `confirmed`
- **page up / page down** · `向上翻页` / `向下翻页` · standard · `high`
- **toggle** · `切换` · standard · `confirmed`
- **make available offline / remove download (cloud)** · `设为离线可用` / `移除下载` · descriptive (cloud-file sense) ·
  `tentative`
- **onboarding (command label + every reference)** · `入门引导` · unified across the whole locale: the wizard noun, the
  `Onboarding…` menu-command label (`commands.cmdrOpenOnboarding`), the `main.upgradeNudge` references to it, the
  `shortcuts.scope.onboarding` scope, and the `settings.onboarding.*` internal copy all use `入门引导`. (The first-pass
  command label was `新手引导`; reconciled to the dominant wizard noun so the menu item and the wizard title read as one
  feature.) · `high`
- **feedback / What''s new / error report (Help menu commands)** · `反馈` / `新增功能` / `错误报告` · macOS/Microsoft
  standard menu wording · `high`
- **boring folders (playful)** · `无聊的文件夹` · kept the friendly/playful en tone literally (style.md: preserve
  deliberate casual voice) · `tentative`

UI section/label names captured (keep consistent): search modes AI `AI` / Filename `文件名` / Content `内容` / Regex
`正则`; filter facets Pattern `模式` / Size `大小` / Modified `修改日期` / Search-in `搜索范围`; type toggle Both `两者`
/ Files `文件` / Folders `文件夹`; result columns Name `名称` / Path `路径` / Size `大小` / Modified `修改日期` /
Actions `操作`.

### Notes (errors catalog)

- **`{verb}`/`{Verb}`/`{gerund}` placeholders inject ENGLISH words** ("copy"/"move"/"delete"/"copying"). Chinese
  sentences are phrased so the insertion sits where a verb goes (`无法{verb}到相同位置`, `{gerund}时出现了意外问题`,
  `无法{verb}这个文件`). The mixed-language result is unavoidable until the verb map itself is localized (tracked task
  #5).
- **`{osMessage}`, `{deviceName}`, `{required}`, `{available}`, `{name}`, `{app}`, `{deletePermanentlyKey}`** are
  runtime values; kept verbatim with natural Chinese spacing around them.
- Quotes around macOS UI labels use full-width `“…”` (`“显示简介”`, `“已锁定”`, `“共享与权限”`), per the Simplified
  convention.

### Licensing / AI / Viewer catalogs (wave 1, 2026-06-21)

macOS zh-CN Tier 1, Microsoft zh-Hans cross-check.

- **Formality in licensing.json: formal `您` throughout** · the whole file is contractual/billing copy (license,
  payment, terms), so per `style.md` § Formality it uses `您`, not the neutral `你`. ai.json and viewer.json use `你`
  (the default friendly register). · `high`
- **license** · `许可证` · Microsoft TBX; macOS · `high`
- **license key** · `许可证密钥` · `密钥` (key/secret), not `钥匙` · `high`
- **API key** · `API 密钥` · standard; `密钥` = secret key · `confirmed`
- **activate / deactivate (a license)** · `激活` / `停用` · standard · `high`
- **perpetual (license)** · `永久` · standard · `high`
- **commercial / subscription** · `商业` / `订阅` · standard · `high`
- **organization** · `组织` · standard · `high`
- **renew (a subscription)** · `续订` · standard · `high`
- **expire / expired** · `过期` · macOS-style (no bare 失败/错误) · `high`
- **valid until / validity** · `有效期至` / `有效期` · standard · `high`
- **open beta** · `公开测试版` · standard · `high`
- **provider (AI service)** · `提供方` · reused from settings pass (Microsoft TBX) · `high`
- **endpoint** · `端点` · Microsoft TBX (`端点`) · `confirmed`
- **model (AI)** · `模型` · standard · `confirmed`
- **server (local AI)** · `服务器` · macOS, reused from settings · `confirmed`
- **clipboard** · `剪贴板` · macOS (AppKit MenuCommands `Clipboard` → 剪贴板) · `confirmed`
- **copy / paste / select all** · `拷贝` / `粘贴` / `全选` · macOS zh-CN MenuCommands · `confirmed`
- **encoding (text)** · `编码` · Microsoft TBX (`Encoding` → 编码) · `confirmed`
- **Western (encoding group)** · `西文` · standard for Latin-script encodings; NOT Microsoft TBX's first hit `西部电影`
  (Western movies, wrong sense) · `high`
- **Unicode** · `Unicode` · kept verbatim (standard name) · `confirmed`
- **streaming (large-file mode)** · `流式` / `流式读取` · Microsoft TBX `流式处理`; shortened to `流式读取` for the
  viewer badge · `high`
- **word wrap (viewer)** · `换行` / `自动换行` · reused from settings pass (`自动换行`); the terse badge uses `换行` ·
  `high`
- **tail (auto-follow file)** · `跟随` · rendered by meaning (follow), not transliterated · `high`
- **index / indexing (viewer)** · `索引` / `建立索引` · reused from settings pass · `high`
- **in memory** · `已在内存中` · standard · `high`
- **viewer (read-only file viewer)** · `查看器` · reused from UI section names · `confirmed`
- **document (file kind)** · `文稿` · macOS uses 文稿 for document; image kind = `图像` · `high`
- **App (application, in cloud/AI copy)** · `App` · Apple zh-CN keeps "App" verbatim (reused from errors pass) · `high`
- **selection (text, in viewer)** · `所选内容` · standard · `high`
- **retry / reload** · `重试` / `重新加载` · standard · `high`
- **`viewer.saveAs.defaultName` kept as `selection`** (NOT translated) · it's a filename base; description requires
  lowercase, no spaces, filename-safe · `confirmed`

### Indexing / downloads / errorReporter / shortcuts / mtp / ui catalogs (wave 1, 2026-06-21)

macOS zh-CN Tier 1, Microsoft zh-Hans cross-check. Reuses prior-pass terms (`索引`/`建立索引`, `驱动器`, `缓冲区`,
`快捷键`, `命令面板`, `重置`, `脱敏`).

- **index (build an index for a drive)** · `建立索引` (verb) / `索引` (noun) · reused from settings pass · `high`
- **scan / rescan (a drive)** · `扫描` / `重新扫描` · macOS Finder (`Searching…` → 正在搜索 pattern) · `high`
- **entry (indexed file/folder)** · `条目` (measure word `个`) · reused from search pass · `high`
- **directory (status/aggregation context)** · `目录` · standard; reused dir abbrev from explorer pass · `high`
- **replay (recorded fs changes)** · `重放` · rendered by meaning (re-apply changes) · `tentative`
- **drive (external/network drive)** · `驱动器` · reused from settings/errors · `confirmed`
- **download (noun, the file) / Downloads (folder)** · `下载内容` (the thing) / `“下载”文件夹` (the folder, macOS Finder
  folder name `下载`) · macOS · `high`
- **jump to (a file/download)** · `跳转到` · standard · `high`
- **global shortcut (system-wide hotkey)** · `全局快捷键` · standard (vs `应用内` in-app) · `high`
- **in-app (scope, vs global)** · `应用内` · standard · `high`
- **modifier (key)** · `修饰键` · macOS/standard · `high`
- **register (claim a hotkey)** · `注册` / `已注册` / `未注册` · standard · `high`
- **key combination / combo** · `按键组合` · standard · `high`
- **error report (the feature/bundle)** · `错误报告` · reused from search-pass Help-menu command (macOS/Microsoft).
  NOTE: this is the one place `错误` is used deliberately — it's the established product-feature noun, not a loud
  failure label; the "Couldn''t …" status strings still render `无法…`. · `high`
- **redact / redaction (scrub logs)** · `脱敏` · standard privacy/security term (`脱敏` = remove sensitive data) ·
  `high`
- **reference ID** · `参考编号` · descriptive · `high`
- **manifest** · `清单` · Microsoft TBX (`manifest` → 清单) · `high`
- **bundle (report bundle)** · `报告包` · descriptive (a packaged bundle of logs) · `tentative`
- **note (free-text field)** · `备注` · macOS/standard · `high`
- **MTP device / USB device** · `MTP 设备` / `USB 设备` · keep MTP, USB verbatim · `confirmed`
- **ptpcamerad / udev / Terminal** · `ptpcamerad` / `udev` kept verbatim; Terminal → `终端` (macOS zh-CN app name) ·
  `high`
- **daemon (system daemon)** · `守护进程` · standard · `high`
- **process** · `进程` · standard · `confirmed`
- **exclusive access** · `独占访问权限` · standard · `high`
- **suggestions (combobox)** · `建议` · standard · `high`
- **dismiss (a toast/notification)** · `忽略` · macOS-style (dismiss a notification) · `high`
- **finalize / preparing view (loading)** · `准备视图` / `正在准备` · descriptive · `high`

### macOS system-feature names (shortcut-conflict warnings; reuse the localized macOS name)

zh-CN macOS labels: Spotlight `聚焦`; Finder `访达`; Character Viewer `字符检视器`; Mission Control `调度中心`; App
windows `应用程序窗口`; Spaces `空间`; Force Quit `强制退出`; input source switching `切换输入源`; app switcher
`应用切换器`; screenshots `截屏`; screen recording `录屏`; logging out `退出登录`; locking the screen `锁定屏幕`.
`System Settings > Keyboard` → `系统设置 > 键盘` (plain literal, matching the errors-pass `系统设置`). · `high`

### UI section names (this wave; keep consistent across catalogs)

- Shortcut scopes: App `应用`; Main window `主窗口`; File list `文件列表`; Brief mode `简洁模式`; Full mode `完整模式`;
  Volume chooser `宗卷选择器`; Network `网络`; Share browser `共享浏览器`; Command palette `命令面板`; About window
  `关于窗口`; Onboarding `入门引导`. (Brief/Full align with the explorer pass's view-mode `简洁`/`完整`.)
- Shortcut filters: All `全部`; Modified `已修改`; Conflicts `冲突`. Badges: macOS `macOS` (verbatim); Fixed `固定`.

### Wave 1 prep catalogs (search/feedback/crashReporter/goToPath/transfer/updates/lowDiskSpace/commandPalette/whatsNew/main/common/notifications, 2026-06-21)

macOS zh-CN Tier 1, Microsoft zh-Hans cross-check. Reuses prior-pass terms.

- **feedback** · `反馈` · reused from search/commands pass (Help-menu wording) · `high`
- **send feedback** · `发送反馈` · standard · `high`
- **crash report** · `崩溃报告` · macOS/Microsoft standard (`crash` → 崩溃) · `high`
- **error report** · `错误报告` · reused from search/commands pass · `high`
- **report ID** · `报告 ID` · keep ID verbatim · `high`
- **dismiss / close (toast/dialog button)** · `关闭` · reused (`Close` → 关闭) · `confirmed`
- **copy / copied (clipboard confirmation)** · `拷贝` / `已拷贝` · macOS Finder copy verb (reused) · `confirmed`
- **restart (the app, to apply update)** · `重新启动` · macOS (`Restart` → 重新启动) · `high`
- **What''s new** · `新增功能` · reused from search/commands pass (Help-menu wording) · `high`
- **changelog** · `更新日志` · standard · `high`
- **update / updates** · `更新` · reused from settings pass · `confirmed`
- **available (new version available)** · `可用` · standard · `high`
- **later (dismiss-for-now button)** · `稍后` · standard · `high`
- **checking / downloading / installing / ready (update status)** · `正在检查` / `正在下载` / `正在安装` / `已就绪` ·
  standard progress wording · `high`
- **go to path** · `前往路径` · macOS Finder (`Go to Folder` → 前往文件夹; path = 路径) · `high`
- **recent (recent paths/searches)** · `最近` (`最近的路径` / `最近使用`) · macOS (`Recent` → 最近) · `high`
- **remove from list** · `从列表中移除` · standard (`Remove` → 移除) · `high`
- **startup disk (boot volume)** · `启动磁盘` · macOS (`Startup Disk` → 启动磁盘) · `high`
- **low disk space** · `磁盘空间不足` · macOS/Microsoft standard · `high`
- **free (space)** · `剩余` · descriptive (rephrased; not literal "free") · `high`
- **target (destination folder, in transfer copy)** · `目标位置` · descriptive; matches the destination sense · `high`
- **trash (verb, move to trash)** · `移到废纸篓` · reused from file-ops pass; Trash noun `废纸篓` (style.md) · `high`
- **sending… (in-progress button)** · `正在发送…` · standard · `high`
- **`feedback.dialog.counter` kept identical** (`{currentText} / {maxText}`) · pure-placeholder fraction, no
  translatable text · `confirmed`

### Operation queue catalog (queue window + pause/resume/background, 2026-06-21; head noun renamed 2026-08-08)

macOS zh-CN Tier 1, Total Commander zh-CN (the feature's origin: queue + background controls), Double Commander zh-CN
(the same orthodox two-pane feature: operation queues), Microsoft zh-Hans cross-check.

- **pause** · `暂停` · macOS (`暂停`, `已暂停拷贝“^0”`), Total Commander (`暂停`), Microsoft TBX (`暂停`) · `confirmed`
- **resume (a paused operation)** · `继续` · Microsoft TBX (`resume` → `继续`), macOS (`继续`). NOTE: NOT `恢复` (that's
  restore/recover, e.g. macOS `恢复` = restore version) — `继续` is the resume-an-operation sense. · `high`
- **operation (the head noun: any queued copy, move, delete, trash, rename, folder/file creation, or archive edit)** ·
  `操作` · macOS Finder zh-CN (`NE1` `无法完成此操作。`, `NE82` `…因为正在进行其他操作，例如移动或拷贝项目…`, `NE83`
  `请在当前操作完成后重试。`), Microsoft TBX (`operation` → `操作`), Double Commander zh-CN (`Current operation:` →
  `当前操作：`, `File operations` → `文件操作`), and the zh catalog's own 56 existing `操作` hits (`操作日志`,
  `文件操作`, `这项操作`). Same word as the Operation log window, so the two View-menu items pair. · `confirmed`
- **operation queue (the window/feature)** · `操作队列` · `操作` (above) + `队列` (below); Microsoft TBX builds queue
  names exactly this way (`报告队列`, `响应队列`, `呼叫队列`), and Double Commander zh-CN puts the two words in one
  sentence for this very feature (`…move operations between queues` → `使用拖放在队列之间进行移动操作`). Pairs with
  `操作日志` (Operation log) in the same View menu block. · `high`. **Supersedes `传输队列`** (the 2026-06-21 term): the
  English widened from "Transfer queue" to "Operation queue" because the window also lists deletes, trashes, renames,
  and folder/file creations, and "transfer" already means copy-or-move one level down (the transfer progress dialog, the
  transfer driver). Never reintroduce `传输队列` for this window.
- **queue (bare noun)** · `队列` · Total Commander (`队列(&Q)`), Double Commander (`New queue` → `新队列`), Microsoft
  TBX (`队列`) · `confirmed`. Unchanged by the rename: `队列中没有任务`, `加入队列`.
- **add to queue / send to the operation queue (the progress-dialog F2 button)** · `加入队列` (button) /
  `发送到操作队列` (aria) · descriptive, built on `队列` + the renamed `操作队列` · `high`
- **background / running in the background** · `后台` (`在后台运行` / `在后台继续运行`) · Total Commander (`后台`,
  `所有上传/下载都在后台进行`), Microsoft TBX (`后台的`). NOTE: NOT `背景` (visual background, wrong sense). · `high`
- **transfer (a copy or move, the narrow sense)** · `传输` · still the right word for the transfer progress dialog and
  SMB/USB transfer copy (`这个传输停住不动了`, `文件传输`), but NO LONGER the queue's head noun: a queued unit is
  `操作`. · `high`
- **"this operation" (per-row aria labels)** · `这项操作` · `项` is the settled classifier for 操作 in this catalog (six
  `这项操作` hits, plus `一项系统操作`), and style.md prefers the spoken `这项` over the written `此` that Double
  Commander uses (`此操作`) · `high`
- **counted operations (`{count} 项操作`)** · classifier `项`, not `个` · matches `这项操作` / `一项系统操作`; the
  generic `{count} 个项目` pattern keeps `个` for items · `high`
- **status words (queue row)** · queued `等待中` / running `进行中` / paused `已暂停` / done `已完成` / cancelled
  `已取消` / failed `无法完成` (style.md: no bare 失败/错误) · macOS-style · `high`
- **pause all / resume all / cancel selected (toolbar)** · `全部暂停` / `全部继续` / `取消所选` · built on settled
  verbs + `全部`/`所选` · `high`

### Navigation & file-ops settings + double-click-to-parent hint (reference-pile pass, 2026-06-26)

macOS Finder zh-CN Tier 1, Double Commander zh-CN (the exact two-pane feature) + Microsoft TBX cross-check.

- **navigation (settings section/card)** · `导航` · Microsoft TBX (`Navigation` → 导航, CHN); macOS Finder uses `导览`
  for the verb `navigate`, but `导航` is the standard UI noun for a navigation section · `high`
- **file operations** · `文件操作` · Microsoft TBX (`operation` → 操作) · `high`
- **parent folder** · `上层文件夹` · macOS Finder (`Go To Enclosing Folder` → 前往上层文件夹;
  `Navigates … to its enclosing folder` → 导览至其上层文件夹). NOTE: Double Commander uses `父文件夹`, but macOS-Tier-1
  wins — keep `上层文件夹` (matches the explorer-pass `上层文件夹`) · `confirmed`
- **go to / navigate to (parent, a path piece)** · `前往` · macOS Finder (`Go to ${location}` → 前往${location};
  `Go To Folder` → 前往文件夹). The breadcrumb tooltip `Click to navigate to {path}` → `点击前往 {path}` · `confirmed`
- **double-click** · `双击` · macOS + Double Commander (`双击文件视图的空白区域时，切换到父文件夹`) · `confirmed`
- **pane background / empty space around the file list (double-click target)** · `窗格背景` (label) / `空白区域`
  (description) · Double Commander attests both framings: `双击视图背景` (view background → 背景) and
  `双击文件视图的空白区域` (empty area → 空白区域). `窗格` from the settled pane term. Label
  `双击窗格背景前往上层文件夹` is unchanged across the two en wordings ("…navigates to parent folder" and the shorter
  "…to go up a folder") — Chinese collapses both to one concise form · `high`
- **row (a file row in the list)** · `行` · Microsoft TBX (`row` → 行). Description renders "not a file row" as
  `而不是某个文件所在的行` (the row a file sits on), contrasting the empty area with a clickable file row · `high`
- **one-time hint (notification)** · `一次性…提示` · descriptive; `提示` = hint, `已显示` = shown · `high`
- **"What just happened?" (hint title)** · `刚刚发生了什么？` · natural rendering, full-width `？` · `high`
- **"Don''t like it?" / "Never do this again" / "I like it" (hint buttons)** · `不喜欢？` / `不再这样做` / `我喜欢` ·
  friendly informal `你`-register per style.md; concise · `high`

### Ellipsis normalization

- **Ellipsis: always the single full-width `…` (U+2026), regardless of the en source''s `...` vs `…`.** Chinese
  typography uses `…`, not ASCII three-dots, so every zh status/label string renders `…` (`正在发送…`, `正在加载…`,
  `正在取消…`). This is a deliberate, locale-wide normalization (not source-faithful byte-copying): the whole zh catalog
  is consistent on the single `…`. (The doubled literary `……` is NOT used here, even in prose tooltips, to keep one
  ellipsis form across the UI.)
- preset (value in a settings-picker dropdown) → 预设; "back to presets" → "返回预设" · Microsoft terminology ("indexing
  preset" → "索引预设"); 预设 dominates the corpus over 预置 (~30:1) · high

### FAT32 too-large-file error (2026-06-30)

macOS Finder zh-CN Tier 1 (`PE4.5` = the same "too large for the volume's format" error:
`相对于宗卷的格式，项目"^0"太大，无法拷贝。`), Microsoft zh-Hans TBX cross-check.

- **drive (removable/USB/SD disk, the FAT32 error context)** · `驱动器` · reused settled glossary term (`驱动器`,
  macOS/Microsoft); the en deliberately says "drive" (friendly) not "disk", and `驱动器` is the established equivalent.
  macOS Finder's `外置磁盘` (external disk) uses 磁盘 for the "disk" sense; kept `驱动器` for catalog consistency ·
  `high`
- **too large (file exceeds a limit)** · `太大` · macOS Finder `PE4.5` (`…太大，无法拷贝`) · `high`
- **format (a filesystem's format, noun) / formatted as X** · `格式` / `采用 X 格式` · macOS Finder `PE4.5`
  (`宗卷的格式`); Microsoft TBX `format` (noun) → 格式. "formatted as FAT32" rendered `采用 FAT32 格式` (uses FAT32
  format) · `high`
- **store (files on a disk)** · `存储` · Microsoft TBX `store` (verb, CHN) → 存储. "store into a drive" rendered `存入`
  (存入这个驱动器); "store files larger than X" → `存储大于 X 的文件` · `high`
- **FAT32 / exFAT (filesystem-format names)** · `FAT32` / `exFAT` · kept verbatim (Apple Finder keeps `ExFAT`,
  `MS-DOS (FAT)` verbatim in zh-CN; do-not-translate format names) · `confirmed`
- **"and N more files" (trailing line under a too-large list)** · `另有 {countText} 个文件` · `另有` = "in addition
  there are" (the "more" sense); measure word `个` per the `{count} 个项目` glossary pattern; Chinese plural collapses
  to a single `other` branch holding `文件` · `high`
- preset (value in a settings-picker dropdown) → 预设; "back to presets" → "返回预设" · Microsoft terminology ("indexing
  preset" → "索引预设"); 预设 dominates the corpus over 预置 (~30:1) · high

### Copy/delete dialog labels + scan spinner (dialog-polish pass)

- **action (what a control chooses; screen-reader label `transferDialog.operationAria`)** · `操作` · Microsoft TBX
  (`action` → `操作`); matches result-column `操作` from the search pass. No colon: it's an aria-label, not a visible
  field label · `high`
- **Scanning… (spinner tooltip while counting selected items)** · `正在扫描…` · reused from the search pass
  (`正在扫描`); the locale-wide `正在…` in-progress pattern + single full-width `…` ellipsis normalization · `high`
- **"doesn''t exist yet" (destination folder, yellow inline warning)** · `还不存在` · macOS Finder PE131
  (`doesn''t exist anymore` → `不再存在`) + Nautilus (`does not exist` → `不存在`); `还` carries the "yet" nuance ·
  `high`
- **"Cmdr will create it during the copy/move" (auto-create reassurance)** · `Cmdr 会在拷贝时自动创建它` /
  `Cmdr 会在移动时自动创建它` · `创建` reused from this file''s `创建文件夹` (mkdir); `拷贝`/`移动` settled verbs;
  `自动` carries the "automatically" reassurance from the @key description; brand `Cmdr` kept verbatim with surrounding
  space · `high`
- **queue.row.label progress arms (rename / create folder / create file)** · `正在重命名` / `正在创建文件夹` /
  `正在创建文件` · "正在[动词]" style of the sibling arms (正在拷贝/移动); reuses settled `重命名` and `创建`
  (创建文件夹 from mkdir); macOS uses 创建 as the create verb ("未能创建文件夹") while 新建文件夹 is the menu label ·
  high

### Archive browsing catalog (2026-07-05)

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
- **read-only archive** · `只读压缩文件` · settled `只读` (glossary) + `压缩文件`; mirrors `只读宗卷` / `只读设备`
  pattern. · `high`
- **archive_edit (queue arm, "Editing archive" = changing a zip's entries)** · `正在编辑压缩文件` · `正在[动词]` sibling
  style + function-key-bar verb `编辑` + settled `压缩文件`. · `high`
- **"removed from the zip for good" (delete-warning continuation)** · `将从 zip 中被永久移除` · `永久` = for good;
  `移除` = remove; `zip` kept verbatim (format token); reads as a natural continuation of `压缩文件里没有废纸篓。` ·
  `high`

### Paste-clipboard-as-file catalog (2026-07-07)

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

### Archive-password dialog (encrypted-zip unlock modal, `fileOperations.archivePassword.*`, 2026-07-08)

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
- No `sameAsSourceJustification` needed: all values differ from English.

### Operation log catalog (`operationLog.*` + `commands.logOperationLog.*`, 2026-07-09)

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
- No `sameAsSourceJustification` needed: every value differs from English (`AI 客户端` keeps only the brand token).

### Network-drive image indexing catalog (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndex*`, `search.imageResults.networkOff/paused`, 2026-07-13)

macOS zh-CN Tier 1 (Finder/Photos), Microsoft zh-Hans TBX cross-check. Reuses settled
`图像`/`网络驱动器`/`建立索引`/`断开连接`/`暂停`/`继续`/`文件夹` terms. Feature: opting a network (SMB) drive into
background indexing of the text inside its photos.

- **photo vs image — deliberate register split, mirroring the English source** · warm user-facing photo mentions →
  `照片` (macOS Photos app; measure word `张`, e.g. `张照片` in Finder); the feature name / internal label "image
  indexing" → `图像` (settled glossary, matching the existing card `settings.mediaIndex.card` = `图像搜索` and
  `enabled.description` = `读取图像中的文字`). The English copy makes the same split (warm "photos" in the opt-in rows,
  technical "image" in the card/label); Chinese follows it faithfully. So `图像索引` = the feature, `照片` = the actual
  pictures. · `high`
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
- No `sameAsSourceJustification` needed: every value differs from English (brand `Cmdr`, `Mac`, `SMB`, and
  `{name}`/`{countText}` placeholders are the only verbatim tokens).

### Ask Cmdr catalog (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `settings.section.askCmdr`, `commands.askCmdrToggle.*`, 2026-07-13)

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

### Bulk rename review, image-index scope, and Ask Cmdr tool labels (quality pass, 2026-07-20)

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
- **image, in the image-index surfaces** · `图像`, never `图片` · locale-wide consistency: `settings.mediaIndex.card`
  `图像搜索`, `settings.section.imageSearch` `图像搜索`, `indexing.enrich.label` `图像索引`, `search.imageResults.*`
  `图像`. The `fileExplorer.imageIndex.*` status-bar family was reconciled from `图片` to `图像` in this pass. The
  warm/technical split from the 2026-07-13 network-drive pass still holds: actual pictures the user thinks of as photos
  → `照片` (`settings.mediaIndex.chosenFolders.*`), the feature and its status labels → `图像` · `high`
- **importance (Cmdr''s ranking of how much a folder matters to this user)** · `重要性` · matches
  `askCmdr.tool.folderImportance` (`正在检查文件夹的重要性`); the scope radio reads `自动，按文件夹的重要性` (was
  `重要程度`, reconciled to one noun) · `high`
- **"lost track of file system changes" (macOS coalesced-events tooltip)** · `没能跟上文件系统的改动` · `改动` matches
  `settings.advanced.fileWatcherDebounce.description` (`文件系统发生改动后…`); phrased as "couldn''t keep up", which
  stays calm and avoids `错误`/`失败` per style.md · `high`
- No `sameAsSourceJustification` needed anywhere in this pass: every value differs from English (only `Cmdr`, `macOS`,
  `Ask Cmdr`, and the `{path}`/`{folder}`/`{percent}` placeholders stay verbatim).

### Image-index status badges (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`, 2026-07-22)

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
- No `sameAsSourceJustification` needed: every value differs from English; only the `{doneText}`/`{totalText}`
  placeholders stay verbatim.

### Image-indexing settings restructure + semantic-search model (`settings.mediaIndex.cards.*`, `.progressSummary.*`, `.semanticSearch.*`, `.clip.*`, `fileExplorer.imageIndex.file.indexing`, 2026-07-23)

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
  now, calm) + `请稍后重试` reused from `operationLog.loadError`; no bare 失败/错误 per style.md · `high`
- No `sameAsSourceJustification` needed: every value differs from English; only `Apple`/`Mac`/`Cmdr` and the `{size}`
  placeholder stay verbatim.

### Delete-dialog trash switch + transfer From/To group headings (`fileOperations.delete.trashSwitch`/`confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`, 2026-07-23)

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

### Drive-indexing master-switch strings (`driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`overriddenBadge`, review pass 2026-07-27)

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
- No `sameAsSourceJustification` needed: every value differs from English; only `{name}` stays verbatim.

## 驱动器索引：检查更改这一趟 (2026-07-28)

- **"Checking for changes" (run-kind header)** · `检查更改` · sibling headers are verb-object phrases (`首次完整扫描`,
  `快速更新`); `检查` is the settled checking verb (glossary `正在检查`, macOS Finder BN9 `正在检查“^0”的内容`), `更改`
  is catalog-settled (`同步最近的更改`). Header, not live status, so no `正在` prefix · `high`.
- **"Update the file list"** · `更新文件列表` · composed from the settled siblings `保存文件列表` + `更新索引` · `high`.
- **"the check running right now"** · `正在进行的这次检查` · reuses `检查` as this catalog's settled word for a full
  check (`tooltipCoalesced`: `下一次完整检查`) and that string's closing `恢复准确` · `high`.

## 传输停滞提示 / stalled-transfer notice (`fileOperations.transferProgress.stall*`, `close`, `queue.row.stalled`, 2026-07-31)

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
- No `sameAsSourceJustification` needed: all eight values differ from English.

## 已拷贝路径：剪贴板确认提示 (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

一个键：按 ⌘⌥C 之后的信息提示行。路径本身在下一行以等宽字体单独显示，因此它并不是句中的占位符——句子以全角冒号结尾，去掉路径后也必须读得通。

- **"Copied the path, it's now on your clipboard:" → `已将路径拷贝到剪贴板：`** · 复用词汇表中已确认的 `path → 路径` 与
  `clipboard → 剪贴板`，动词沿用 Finder 的 `拷贝` · confirmed。`已将 X 拷贝到 Y`
  与同批的粘贴提示 (`已将剪贴板{图像/PDF/文本}粘贴为 {filename}`) 同构，`已` 表示动作已完成。英文的 "it's now on your
  clipboard" 合并进 `到剪贴板`：中文不给唯一的剪贴板加物主代词。冒号用全角 `：`。
- 无需 `sameAsSourceJustification`：该值与英文不同。

### Corner progress chip + failure notice (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`, 2026-08-08)

Nine keys for two new surfaces: the main window's ~80 px corner progress chip (a button that opens the queue window) and
the never-auto-dismissing failure notice plus its failed queue row. Head noun `操作`, window name `操作队列`, and the
classifier `项` all come from the operation-queue section above; this section only records what that one doesn't.

- **dismiss (button that removes a failed row / a notice, undoing and retrying nothing)** · `忽略` · the zh catalog had
  already settled the English "Dismiss" BUTTON as `忽略` in four places (`downloads.empty.dismiss`,
  `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`, `errorReporter.bundleSavedToast.dismiss`) plus
  `ui.toast.dismissAria` = `忽略通知`, and the catalog outranks the pile for a concept the app already ships. The pile
  offers no competing first-party term: macOS has only `Dismiss Popover` → `关闭弹出窗口` (a popover, not a list row),
  Microsoft TBX gives `消除`/`关闭` (both defined as "turn off a system notification"), and none of the five file
  managers has "dismiss" at all. `清除` (macOS `Clear Menu` → `清除菜单`) and `移除` (macOS `Remove` → `移除`) were
  rejected: both read as deleting something, and the row deletes nothing. `queue.row.dismissAria` = `忽略这项操作`,
  matching the sibling arias `暂停/继续/取消/选择这项操作` verbatim · `high`
- **dismiss all (toolbar)** · `全部忽略` · the `全部 + 动词` family shape this window already uses (`全部暂停`,
  `全部继续`) and the rename-review pass's `全部允许`/`全部拒绝` · `high`
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
- No `sameAsSourceJustification` needed: all nine values differ from English.

### Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`, 2026-08-09)

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

### Empty-queue state of the progress dialog's F2 button (`fileOperations.transferProgress.background`/`backgroundAria`, 2026-08-09)

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
- No `sameAsSourceJustification` needed: both values differ from English.

### Quit-while-running gate (`main.quit.*`, 2026-08-10)

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
- No `sameAsSourceJustification` needed: all seven values differ from English.

### 使用统计：去掉“匿名”，写明“一个随机标识符” (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `假名化` / `匿名化` — that jargon is
exactly what the copy avoids.

- **usage stats → `使用统计`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the `匿名` modifier
  was cut · high
- **a random id → `一个随机标识符`** · MS terminology zh-Hans (random → `随机`, identifier → `标识符`) · `high`.
  `标识符` is the established native term the style guide prefers over an English `ID` loan, and it is plain enough for
  consumer copy (Apple's Chinese privacy wording uses the same word).
- **tied to → `关联到`** · the catalog's own verb (`onboarding.stepBeta.emailNote` “绝不会和你的使用统计关联”) · `high`
- No `sameAsSourceJustification` needed: every value differs from English.

### 等待回答的队列行 + 回滚确认框 (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, 改写的 `transferProgress.foregroundBusyToast`/`.rollbackTooltip`, 2026-08-13)

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
- No `sameAsSourceJustification` needed: all eight values differ from English.

### 重命名链的其他文件计数 (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, 2026-08-18)

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

### 无法确认的重命名 + 名称不可用 (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`, 2026-08-18)

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

## 建议的操作：Ask Cmdr 提议内容的对话框（`suggestedOps.*`、`commands.suggestedOpsShow.*`，2026-08-19）

- ops（代理提议的文件操作）→ `操作`；标题定为 `建议的操作` · 沿用目录中的 "File operations" → `文件操作` · high
- approve → `批准` · 通用译法；未采用 macOS 的 `接受`（那是 AirDrop 接收文件的用词），此处是授权执行 · high
- reject → `拒绝` · macOS Finder AirDrop 面板的 接受/拒绝 词对（Tier 1）· high
- "This can't be undone" → `此操作无法撤销` · macOS Finder 原句（立即删除警告）· high
- pattern → `模式` · 已在 `queryUi.json` 中 · high

## 复制（Duplicate）：在同一文件夹内拷贝的命令（`commands.fileDuplicate.*`，2026-08-19）

- **duplicate（把所选项目拷贝到它自己所在文件夹的命令）→ `复制`** · macOS Finder
  zh-CN 的“文件 > 复制”（`N154`），另有“复制项目”和“在当前位置复制项目”（在 macOS 26.6.1 的
  `Finder.app/Contents/Resources/zh_CN.lproj` 中核实，2026-08-19）· `high`。**记住这对术语的分工**：`拷贝` =
  Copy（F5 传输与剪贴板），`复制` = Duplicate。这正是 macOS
  Finder 自己的区分，用户在 Finder 里看到的就是这一对，所以两个命令挨着出现也不算冲突。
- **"Make a copy of the selected files in the same folder" → `在当前文件夹中为选中的文件创建副本`** · 沿用目录里已有的
  `当前文件夹`（`commands.editPaste.description`）和 `副本`（`commands.cloudRemoveDownload.description` 的 `本地副本`）·
  `high`。

## 原生菜单：菜单栏、右键菜单、窗口标题（`menu.*`、`licensing.windowTitle.*`、`main.instanceLock.*`，2026-08-19）

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
- **changelog → `更改日志`** · Microsoft 术语库 `zh-Hans` · high。与帮助 > `新增功能` 区分：一个指文档，一个指消息。
- **word wrap → `自动换行`** · Microsoft 术语库 `zh-Hans`，Double Commander `zh-CN` · high。
- **pin / unpin tab → `固定标签页` / `取消固定标签页`** · Safari `zh-CN`（「固定标签页」）· high。
- **Finder 标签颜色 → `红色、橙色、黄色、绿色、蓝色、紫色、灰色`** · macOS Finder（`TG_COLOR_*`）· high。
- **busy（宗卷正在使用）→ `（占用中）`** · Microsoft 术语库（`忙碌`）· high。磁盘用「占用中」比「忙碌」自然。
- **Eject → `推出`、Disconnect → `断开连接`、Remove（从列表中移除）→ `移除`** · macOS Finder · high。
- **括号与引号用全角**：`{app}（默认）`、`推出（{name}）`、`拷贝“{name}”`。占位符本身保持半角原样。
- **有意与英文相同**（已写 `sameAsSourceJustification`）：`menu.zoom.percent*` 与 `menu.view.askCmdr`。

## 系统连接回退通知（`fileExplorer.network.osMountFallback.*`，2026-08-21）

Cmdr 没能建立自己的直接连接，共享改走 macOS 提供的连接时弹出的通知。语气是安抚，不是报错：共享能用，只是慢。

- **native（macOS 内建的）→ `内建`** · macOS Finder/AppKit `zh-CN` 只用 `内建`（4 处，`内置`、`自带` 各 0 处）·
  `high`。目录里此前混用 `内置`（4 处）和 `自带`（1 处），以后统一到 `内建`。
- **macOS's native SMB network connection → `macOS 内建的 SMB 网络连接`** · 与目录里已定的 `系统连接`
  （`fileExplorer.pane.directConnection*Toast`、`fileExplorer.navigation.connectionTooltipSystem`）指同一件事；这条正文第一次介绍它，所以写全称，短提示里继续用
  `系统连接` · `high`。
- **"4x slower" 这类倍数 → `慢 4 倍`** · 用阿拉伯数字 + `倍`，前后加空格。中文口语里 `慢 N 倍`
  略有歧义（1/N 还是 1/(N+1)），但这里传达的是「慢很多」，精确值不承重；需要精确时改写成 `速度只有…的 1/4` · `high`。
- **click（按钮/链接）→ `点按`** · macOS `zh-CN` 全用 `点按`（`点击` 0 处），onboarding 的
  `点按下方的 <strong>…</strong>` 已是同一句式 · `high`。目录里 `点击` 还有 10 处，以后向 `点按` 收敛。
- **Try connecting directly（按钮）→ `试试直接连接`** · 复用已定的 `直接连接`（`fileExplorer.navigation.connectDirectly`
  = `直接连接，访问更快`）；`试试` 是动词重叠的祈使式，保留英文 "Try" 的「不一定成」的意味，比 `尝试`
  更贴 Cmdr 的口语声音 · `high`。
- **Dismiss（关闭通知的 X 的悬停提示）→ `关闭`** · 与 `lowDiskSpace.toast.closeTooltip` 完全同一个控件、同一个
  `sourceHash`，直接复用 · `confirmed`。注意与 `queue.row.dismiss*` 的 `忽略` 区分：`忽略`
  是写在按钮上的「不再管它」，X 的提示是 `关闭`。

## 重命名/新建被拒绝时的一行提示（`errors.mutation.*`、`errors.volume.*`，2026-08-23）

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
- **"didn''t work"（密码被拒）→ `不起作用`** · 目录里 `fileExplorer.smbReauth.savedPasswordFailed` 就是
  `保存的密码不起作用了。`；`fileOperations.archivePassword.retryMessage` 的 `这个密码没能解锁…`
  是同一件事的长版本。❌ 不写 `密码错误` · `confirmed`
- **"couldn''t tell what"（说不出具体原因的兜底）→ `也说不清是什么`** · `出了点问题`
  是目录里已定的兜底说法（5 处，`ai.cloud.genericError`、`updates.checkToast.errorPrefix` 等），`说不清`
  是日常口语，承接英文有意的谦虚语气 · `high`
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
  `fileOperations.delete.noTrashWarningStrong/Rest`（`这个宗卷不支持废纸篓。文件将被彻底删除。`）也是这么写的。目录里另有一处
  `永久删除`（`errors.write.trashNotSupported.suggestion`），以后向 `彻底删除` 收敛 · `high`
- **"macOS wouldn't …"（系统拒绝了这次操作）→ `macOS 拒绝把这个项目移到废纸篓。`** · 动词 `拒绝` 来自 macOS Finder zh-CN
  `MR100`（`“^0”已拒绝你的请求。`），是「系统/服务器不肯照做」这个意思的现成说法；`移到废纸篓`
  是已定的固定搭配（`errors.write.*.trash`
  一族）。英文有意写得短，因为具体原因另在“技术详情”里显示，所以中文也不补原因；宾语用 `这个项目`（同
  `errors.mutation.fileLocked` 的 `这个项目`），因为提示显示在名称输入框下方，光写 `它` 没有先行词 · `high`
- 无需 `sameAsSourceJustification`：这一批 33 条全部与英文不同。

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

### Eject / disconnect error copy (`errors.eject.*`, 2026-08-23)

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

## 废纸篓提示条：撤销与前往废纸篓（2026-08-27；`fileOperations.trash.*` + `commands.fileGoToTrash.*`）

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
- 九条值都与英文不同，无需 `sameAsSourceJustification`。

## 给已发送的错误报告补充备注 / amending a sent error report（`errorReporter.amend.*`、`errorReporter.amendedToast.message`、`errorReporter.autoSentToast.viewOrAddNotes`，2026-08-28）

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
- 十一条值都与英文不同，无需 `sameAsSourceJustification`。中文没有撇号，ICU 的 `''` 规则在这一批里用不上。

## 选择/取消选择文件对话框（`selection.*`，2026-08-29）

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
  才是「从列表里挑项目」。两边各自按自己的 macOS 源翻，永远不要互转。繁体一侧见 `../zh-Hant/glossary.md`。
- **"Select these files" / "Deselect these files"（对话框底部主按钮）** · `选择这些文件` / `取消选择这些文件`
  ·在上面两个动词上直接构词 · `high`。
- **"… in the focused pane"（按钮的悬停提示）** · 处所状语提到句首：`在焦点窗格中选择这些文件` /
  `在焦点窗格中取消选择这些文件` · `high`。**提示语是独立的一句，不必以按钮文字开头**：按钮的无障碍名取自 `…label`
  键（`QueryDialog.svelte` 用 `primaryAction.ariaLabel ?? primaryAction.label`），提示只是 `use:tooltip`
  的悬停文案，WCAG 2.5.3已由构造保证。英文那句把范围放在句尾是英文的语序，中文照搬会别扭，所以按中文语序把 `在…中`
  提到动词前面。 `焦点窗格`
  是目录里已经定下的说法（`commands.navGoToPath.description`、`commands.favoritesAdd.description`）。
- **"Press Enter to filter"** · `按 Enter 键筛选` · 沿用本文件「pressing Enter / the Enter key」条目定下的
  `按 Enter 键`（目录里另有 9 处这么写）加上 `筛选`（`queryUi.recent.filterPlaceholder` `筛选最近的搜索`） · `high`。❗
  **有意不跟 `search.runHint` 的 `按回车键搜索`**：那一条本文件早就记成唯一的历史遗留写法，该被收敛，不该被复制。
- **"recent selections"（最近用过的查询弹窗）** · `最近的选择` · 上面的动词当名词用，与 `queryUi.recent.*` 的孪生键
  `最近的搜索` 完全对仗 · `high`。五个弹窗键逐字照搬孪生键，只把 `搜索` 换成 `选择`。
- **"Matching what is shown in the list (the full path)."** · `匹配的是列表中显示的内容（完整路径）。` · `匹配`
  是目录的 match 动词（`queryUi.scope.toggle.caseSensitiveAria` `区分大小写匹配`、
  `commands.selectionSelectFiles.description` `将匹配的文件加入选择`），`列表` 与 `完整路径` 也都是现成的说法 · `high`。
- **"Apply recent {mode} selection: {query}"** · `应用最近的 {mode} 选择：{query}` · `应用` =
  Apply（`ai.local.applyContextSize`）；全角冒号与 `queryUi.recent.scopeSummary`（`范围：{scope}`）一致；`{mode}`
  两侧加空格，因为它可能是拉丁文的 `AI` · `high`。`{query}` 是不可控的用户输入，放在冒号后的句尾，落什么进来都读得通。
- 15 条值全部与英文不同，无需 `sameAsSourceJustification`。这批里没有撇号，ICU 的 `''` 规则用不上。

## 术语漂移审计：同一英文串的多种译法（全目录，2026-08-30）

`desktop-i18n-term-consistency` 把 `zh`
报出 28 处「同一条英文、两种中文」。逐条查证后：15 处是真漂移，已收敛；13 处是**真正的语义分界**，两种译法各自正确，故意保留。分界必须写成规则，否则下一轮翻译会「修」回去。

### 收敛掉的 15 处（真漂移）

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
  `个文件`／`个目录`（`transferDialog.filesPart`、`dirSize.fileCount`、`scanPhase.throughputFiles` `个文件/秒`）。
- **两条重复的散文** · `onboarding.stepBeta.signup.success` 与 `settings.updates.emailConfirmHint` 是同一句英文，统一为
  `请查看收件箱，确认你的邮箱。谢谢你的帮助！`；`onboarding.stepBeta.signup.failure` 与
  `settings.updates.emailSignupError` 统一为 `抱歉，我们现在没能帮你注册。要再试一次吗？`（`没能` 比 `无法`
  更软，合乎风格指南「不用响亮的失败词」）。

### 故意保留的 13 处分界（英文一词多义，中文必须分开）

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
  自己就写了「风格指南若有更友好的说法就别用 error 的字面词」，而本语言风格指南正是这么规定的）；开发者／诊断前缀 `错误`
  （`settings.updates.errorPrefix`，英文 `@key` 明说这里 `Error` 可以照用）· `confirmed`。**界线由英文的 `@key`
  描述自己划定**，不是译者的偏好。
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

## 术语漂移审计：英文不同、中文该同的那一半（手工排查，2026-08-30）

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
  `右键点击`。没有 Tier 1 反证就不要动它们，尤其别凭印象改成 `连按`／`右键点按`。 · `high`
- **`Queued` = `等待中`**（`operationLog.status.queued`），不写 `已排队`。它跟 `进行中`／`已完成`
  是同一组状态值，跟队列这个名词（`队列`）不必同形。 · `high`

### 排查结论：这几对近义词目前是干净的

`copy 拷贝` / `duplicate 复制`、`undo 撤销` / `roll back 回滚`、`tab 标签页` / `tag 标签`、 `API key 密钥` /
`license key 许可证密钥`、`remove 移除` / `delete 删除`、`folder 文件夹` / `dir 目录`、 `cancel 取消` /
`stop 停止`、`open 打开` / `go to 前往` —— 全目录逐键对照，除上面那条 `verbCopy`
外没有互串。下次复审可以从这份清单接着往下走。

## Shared `en` fixes: menu wording, System Settings tokens, name-restore verb (2026-08-30)

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

## 完成中断的回滚（`operationLog.dialog.finishRollBack`、`operationLog.rollback.partiallyRolledBackNotice`、`fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`、`queue.row.reversalInFolder`，2026-08-30）

操作日志多了一种状态：回滚做到一半被取消，那一行就变成“已部分回滚”，按钮从“回滚”换成“完成回滚”。这一批五条值全部锚在目录已有的回滚词汇上，没有新造词。

- **"Finish rolling back" → `完成回滚`** · 沿用目录里定好的 `回滚`（`operationLog.dialog.rollBack` =
  `回滚`、`rollingBack` = `正在回滚`、`partiallyRolledBack` = `已部分回滚`，见本文件 `roll back (reverse an operation)`
  条）· `medium-high`。`完成` 说的是“把这一次回滚做完”，不会读成“重新回滚一次”；和同一行的徽章 `已部分回滚`
  连起来看，意思很清楚。备选 `继续回滚` 在“不是新开一次”这一点上更直白，但英文写的是 Finish 而不是 Continue，而且 `继续`
  已经给了 `queue.row.resume`，再用一次会撞车。母语审校时这条值得再看一眼。
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
- 五条值都与英文不同，无需 `sameAsSourceJustification`。这批里没有撇号，ICU 的 `''` 规则用不上。

## 回滚结束后的提示条

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
- ⚠️ **待收敛：`挪回` vs `放回`。** 同一次移动回滚里，队列行写
  `正在把文件挪回原处`（`queue.row.reversalMovingBack`），确认框写
  `这会把文件挪回原来的位置`（`rollbackConfirm.bodyUndoByMovingBack`），而这批提示条写
  `放回原处`。两者中文都通顺，语义也一样，英文那边三处也都是 "put/move
  back"，所以检查抓不到。下一次做这个家族时建议统一到 `放回原处`（它是 Finder 的 Tier
  1 词，而且明说「回到原来的位置」）· `tentative`。
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
- 18 条值都与英文不同，无需 `sameAsSourceJustification`。中文侧没有撇号，ICU 的 `''` 规则用不上； `{count}` 只写 `other`
  分支（中文 CLDR 只有这一类）。
## WebKit 过旧时的拦截页（`main.oldWebkit.*`，2026-09-02）

三条文案，在 Mac 的 Safari 过旧时代替 Cmdr 的界面显示。它们写在 HTML 外壳里而不是应用里，所以这是那位用户能看到的 Cmdr 的
全部内容。

- **`Software Update` → `软件更新`** · macOS 系统设置中该面板的名称；Finder 的 Tier 1 证据佐证了这个词（`Apple Device
  Software Update File` → `Apple设备软件更新文件`）· `high`。
- **`Quit` → `退出`** · macOS AppKit 的 `Quit` 键 → `退出` · `high`。此前不在词汇表里，现补上。
- **`Safari`、`Mac`、`15.4` 保持原样**，两侧按 § 间距规则加空格。`Safari` 已加入 `BRAND_WORDS`。
- 面板名用直角引号之外的全角引号 `“软件更新”`，与目录里其余简体文案一致。
