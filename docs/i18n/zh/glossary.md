# zh glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/zh/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

## Terms

Core file/UI terms (Trash, copy, move, open, settings, etc.) live in [`style.md`](style.md) § Terminology and glossary;
this file adds the terms settled while translating the catalogs. All `zh-Hans` (Simplified).

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
  `{full_disk_access}`, OS-localized at runtime; never hand-translate. The git-suggestion strings use plain literals
  instead (matching the original git copy): `系统设置` / `隐私与安全性` / `文件和文件夹` (all macOS labels). · `high`

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

### Transfer queue catalog (queue window + pause/resume/background, 2026-06-21)

macOS zh-CN Tier 1, Total Commander zh-CN (the feature's origin: queue + background controls), Microsoft zh-Hans
cross-check.

- **pause** · `暂停` · macOS (`暂停`, `已暂停拷贝“^0”`), Total Commander (`暂停`), Microsoft TBX (`暂停`) · `confirmed`
- **resume (a paused operation)** · `继续` · Microsoft TBX (`resume` → `继续`), macOS (`继续`). NOTE: NOT `恢复` (that's
  restore/recover, e.g. macOS `恢复` = restore version) — `继续` is the resume-an-operation sense. · `high`
- **queue (noun)** · `传输队列` (the window/feature) / `队列` (bare) · Total Commander (`队列(&Q)`), Microsoft TBX
  (`队列`) · `confirmed`
- **add to queue / send to the transfer queue (the progress-dialog F2 button)** · `加入队列` (button) / `发送到传输队列`
  (aria) · descriptive, built on `队列` · `high`
- **background / running in the background** · `后台` (`在后台运行` / `在后台继续运行`) · Total Commander (`后台`,
  `所有上传/下载都在后台进行`), Microsoft TBX (`后台的`). NOTE: NOT `背景` (visual background, wrong sense). · `high`
- **transfer (the queued copy/move/delete unit)** · `传输` · reused; `传输队列` = transfer queue, `这个传输` = this
  transfer · `high`
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

### Copy/delete dialog field labels + scan spinner (dialog-polish pass)

- **Action: (field label before a Copy/Move or Trash/Delete two-option picker)** · `操作：` · Microsoft TBX (`action` →
  `操作`); matches result-column `操作` from the search pass. Full-width colon per style.md punctuation rule (matches
  sibling labels `来自：`, `AI 建议：`) · `high`
- **Route: (field label before a "source → destination" line in the copy/move dialog)** · `路线：` · no reference-pile
  source for this metaphorical "route" label; `路线` (route/itinerary) carries the en journey metaphor and avoids the
  file-path collision of `路径`. Full-width colon · `tentative`
- **Scanning… (spinner tooltip while counting selected items)** · `正在扫描…` · reused from the search pass
  (`正在扫描`); the locale-wide `正在…` in-progress pattern + single full-width `…` ellipsis normalization · `high`
- **Scan complete (checkmark tooltip once counting finished)** · `扫描完成` · macOS `完成` is the standard
  "complete/done" word (`完成` / `已完成`); `扫描` from the scan term · `high`
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
  catalog usage (`settings.search.autoApply.description` `按 Enter 键`, `queryUi` `按 Enter 搜索`, `⌘Enter`); macOS
  doesn't surface a Return-key word in this pile, and the catalog keeps `Enter` verbatim (one legacy `按回车键` in
  `search.runHint` is the outlier). · `high`
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
as one feature with the transfer queue.

- **operation log (the dialog / command name)** · `操作日志` · `操作` (operation, Microsoft TBX / search-pass result
  column) + `日志` (log, settings-pass `logging` → 日志). Standard, natural compound. · `high`
- **operation history** · `操作历史记录` · `历史记录` = history; loadError renders `无法加载你的操作历史记录。请稍后重试。`
  (no bare 失败/错误 per style.md; `请稍后重试` = try again in a moment, reusing settled `重试`) · `high`
- **lifecycle status words (match the transfer queue `queue.row.status` exactly)** · Queued `等待中` / Running `进行中` /
  Done `已完成` / Didn''t finish `无法完成` / Canceled `已取消` · reused verbatim from `queue.json` so the two surfaces
  agree; `无法完成` carries the style-guide "avoid failed" rule (same as the queue) · `high`
- **roll back (reverse an operation)** · `回滚` · reused from the file-ops pass (`rollback` → `回滚`, Microsoft TBX). Arms:
  Can''t roll back `无法回滚` / Can roll back `可回滚` / Rolling back `正在回滚` (locale-wide `正在…` in-progress) / Rolled
  back `已回滚` / Partly rolled back `已部分回滚` (`已…` perfective + `部分` = partly) · `high`
- **per-item outcome** · Done `已完成` / Skipped `已跳过` (settled `跳过` + `已` perfective) / Didn''t finish `无法完成`
  / Rolled back `已回滚` · reuses status + rollback terms · `high`
- **summary lines (perfective `已[动词] {countText} 个项目`)** · `已拷贝`/`已移动`/`已删除`/`已重命名`/`已创建`/`已压缩` +
  measure-word `个项目`/`个文件`/`个文件夹`; trash → `已将 {countText} 个项目移到废纸篓` (settled `移到废纸篓`); archive
  edit/extract → `已编辑压缩文件` / `已解压压缩文件` (settled `压缩文件` + `编辑`/`解压`). `已` matches sibling result
  toasts (`已拷贝`, `已压缩`). Chinese collapses each ICU plural to a single `other` branch holding `{countText}`. · `high`
- **"and N more items" (moreItems)** · `另有 {countText} 个项目` · reused verbatim from the FAT32-pass `另有 {countText}
  个文件` pattern (`另有` = in addition there are), items → 项目 · `high`
- **initiator / provenance labels** · You `你` (informal register, style.md) / AI client `AI 客户端` (keep AI verbatim +
  `客户端` = client) / Agent `代理` (settled agent → 代理) · `high`
- **"Load 50 more" (loadMore button)** · `再加载 50 条` · `再加载` = load more; `条` measure word for log records ·
  `high`
- No `sameAsSourceJustification` needed: every value differs from English (`AI 客户端` keeps only the brand token).
