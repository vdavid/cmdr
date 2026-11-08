# zh decisions

"X over Y because Z" rulings, each defending a choice against a well-meant "fix"; the brief pulls a section whose
heading cites a batch key. Forms: `terms.json`; voice and typography: `style.md`. Tiers: macOS zh-CN (1), Microsoft
zh-Hans (2), file managers (3); pile `_ignored/i18n/zh/` in the main clone.

## Settings vocabulary (`settings.section.*`, `settings.appearance.*`, `settings.summary.*`)

- Labels take full-width parens and `、` (`MTP（Android/Kindle/相机）`, `服务器（SFTP、WebDAV）`).
- Navigation `导航` (Finder's `导览` is the verb); theme `浅色` / `深色` / `跟随系统`; tint `着色` (macOS; Microsoft's
  `淡色` is another sense); toast `提示`; hidden developer strings open `内部：`.

## Views, columns, scopes (`shortcuts.scope.*`, `shortcuts.section.*`, `fileExplorer.columns.*`, `fileExplorer.navigation.group*`, `queryUi.*`)

- Views `完整` / `简洁` (scopes `…模式`); search mode `正则`; type toggle `两者`.
- Places `共享位置` over bare `位置` (file system) and `地点` (photos); tentative.

## macOS names in the reserved-shortcut list (`shortcuts.system.*`)

macOS's own names (`聚焦`, `字符检视器`, `调度中心`, `空间`, `强制退出`, `截屏`, `录屏`, `系统设置 > 键盘`);
`Finder 搜索窗口` stays Latin.

## Error-copy conventions (`errors.*`)

- A bare `{system_settings}`-style token takes a space on both sides (it may arrive CJK or Latin); in a bold path the
  trailing `里` follows the last pane name, never the token. Untokened panes: `Apple 账户`, `通用`, `登录项与扩展`.
- Permission denied `没有权限`; authentication `无法通过身份验证`. "Here's what to try:" `可以这样试试：`; "Navigate
  here again" `再次进入这里`; "if it keeps happening" `如果一直这样` (`；` for an English "or"); "couldn't tell what"
  `也说不清是什么` (`errors.mutation.unexpected` = `errors.eject.unexpected`).
- A runtime name the English leaves unquoted stays unquoted (`{app} 还在…`); quote where the English does, or where a
  Han/Latin name meets Han text with no fixed spacing (a destination).

## Conflict-policy buttons and small dialog words (`fileOperations.transferDialog.policySkip`/`.policyOverwrite`, `fileOperations.transferDialog.operationAria`)

`全部跳过` / `全部覆盖` even for one conflict (the policy acts on the set). `operationAria` `操作`, no colon; under the
cursor `光标所在的`; OK `好`; Quit & Reopen `退出并重新打开`.

## Licensing, AI, and viewer vocabulary (`licensing.*`, `ai.*`, `viewer.*`)

- Licensing `组织`, `续订`, `有效期至`. AI `拖放`, `预算`, `速率限制`, `配额已用完`, `约 {amount}`, `费用未知` (over
  Microsoft's `成本费`), `免费，本地运行`.
- Viewer encoding group `西文`; selection `所选内容`; `viewer.saveAs.defaultName` stays ASCII `selection` (a filename
  base).

## Indexing, downloads, reporter, MTP (`indexing.*`, `downloads.*`, `errorReporter.*`, `mtp.*`)

Index entry `条目` (`个`); replay `重放`; hotkey `注册`; report bundle `报告包`; daemon `守护进程`; `ptpcamerad` /
`udev` verbatim; free space `剩余`; refine `优化`; Page Up/Down `向上翻页` / `向下翻页`; "boring folders" stays playful,
`无聊的文件夹`.

## Operation queue catalog (`queue.*`)

- `操作队列` over `传输队列`: it lists deletes, renames, and creations too; same head noun as `操作日志`, so the
  View-menu items pair. Generic items keep `个项目`.
- `后台` over `背景` (a visual backdrop); resume `继续` over `恢复` (macOS: restore a version).
- Status `等待中`, `进行中`, `已暂停`, `已完成`, `已取消`, `无法完成`; Queued `等待中`, never `已排队`. Reorder `上移` /
  `下移` / `调整位置`, never `移动` (reads as a move operation).

## Double-click the pane background (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

`窗格背景` in the label, `空白区域` in the description (Double Commander uses both); `前往上层文件夹`; hint buttons
`不喜欢？` / `不再这样做` / `我喜欢`.

## FAT32 too-large-file error (`errors.write.filesTooLargeForFilesystem.*`)

`驱动器` over Finder's `磁盘` (the English says "drive"); "and N more files" `另有 {countText} 个文件`.

## Copy and delete dialog labels and the scan spinner (`fileOperations.shared.scanningTooltip`, `fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`)

`还不存在`; `Cmdr 会在拷贝时自动创建它` (`自动` reassures). `queue.row.label` arms all `正在[动词]`; `创建` is the act,
`新建文件夹` the menu label.

## Archive browsing (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.archive*`, `settings.archives.*`, `errors.mutation.archive*`)

- `压缩文件` over Finder's `归档` (every file manager browses INTO a `压缩文件`; `归档` means filing away and survives
  only in `照片归档`). `解压` over Nautilus's `提取` (one component).
- `像文件夹一样浏览`, summary `进入浏览`; full screen `全屏幕` (AppKit); `正在编辑压缩文件` / `压缩文件编辑`;
  `将从 zip 中被永久移除`.

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

`什么都不做` over Microsoft's `不执行任何操作`. Toast `已将剪贴板{图像/PDF/文本}粘贴为 {filename}`, the shape of
`已将路径拷贝到剪贴板：`; no possessive for the one clipboard.

## Archive-password dialog and Compress (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

`受密码保护`, `解锁`, aria `压缩文件密码`. Slider ends `更快` / `更小` (Total Commander's poles as comparatives), label
`压缩级别`. `pathErrorNotZip` says `压缩文件名`, `“.zip”` hugging Han.

## Operation log catalog (`operationLog.*` + `commands.logOperationLog.*`)

Status words are `queue.row.status` verbatim. Rollback `无法回滚` / `可回滚` / `正在回滚` / `已回滚` / `已部分回滚`;
summaries `已[动词] {countText} 个项目`, `另有 {countText} 个项目`; `再加载 50 条` (`条` for records); initiators `你` /
`AI 客户端` / `代理`.

## Network-drive image indexing catalog (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndex*`, `search.imageResults.networkOff/paused`)

`照片` for the user's pictures, `图像` for the feature (`图像索引`, `未纳入图像搜索`), the English's own warm/technical
split. "gently" `很克制` over `温和地` (the promise is restraint). `始终索引`; terse status `尚未索引`, prose
`建立索引`.

## Ask Cmdr catalog (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `settings.section.askCmdr`, `commands.askCmdrToggle.*`)

Chat `聊天`, the list `聊天记录` (WeChat), `新建聊天`. `token` Latin with `个` (tentative). Usage `用量` over
Microsoft's `使用情况`; Spending `花费`.

## Tool lines (`askCmdr.tool.*`, `askCmdr.toolGroup.*`)

`<place>` names stay bare and spaced (the link marks them); quoted `{query}` keeps `“…”`. "and N more" → `和另外 N 个`
(`等 N 个` would count the first). "Tried to" → `尝试…`.

## Rename review + index scope (`askCmdr.renameReview.*`, `settings.mediaIndex.scope.*`)

"needs attention" `需要先处理` (the English stays vague; tentative). Importance `重要性` everywhere
(`自动，按文件夹的重要性`). `没能跟上文件系统的改动` (`改动` matches the watcher). Badges `（扩展名）` / `（循环）`;
`重命名循环` (`周期` is a time period).

## Image-index status badges (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

Badge `标记` over jargon-y `角标` (tentative). Counts `已索引全部 {totalText} 张图像`,
`{totalText} 张图像中已索引 {doneText} 张`.

## Image-index settings + semantic search (`settings.mediaIndex.cards.*`, `.progressSummary.*`, `.semanticSearch.*`, `.clip.*`, `fileExplorer.imageIndex.file.indexing`)

The feature mid-sentence `“通过描述搜索”`; the toggle IS the label, unquoted. `搭载 Apple 芯片的 Mac` (apple.com.cn),
never `Apple 硅`. `删除模型（释放 {size}）`, no `约` for a flat size. `file.indexing` = `progressSummary.title` =
`正在建立索引`.

## Trash switch + From/To headings (`fileOperations.delete.trashSwitch`/`confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

- `来源` / `目标` (Total Commander) over `从` / `到`: coverbs need an object, and the path sits below. In a sentence
  `目标位置` (bare `目标` is "goal").
- `移到废纸篓` and `删除` equal `transferDialog.titleVerbOnly`'s arms.

## Drive-indexing master-switch strings (`driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`overriddenBadge`)

- One quoted pair `在“索引 > 驱动器索引”中开启` over Apple's per-element quotes, for consistency.
- Badge `已随驱动器索引关闭` (a state; bare `随…关闭` reads as an instruction).
- `开启` for a Settings toggle, `打开索引` for the per-drive menu (`为此驱动器打开索引`): pick by which switch the
  string points at.

## Drive-index change check (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`)

Header `检查更改` without `正在`, like `首次完整扫描` / `快速更新`.

## Stalled-transfer notice (`fileOperations.transferProgress.stall*`, `close`)

- `已有 {duration} 没有进度` (`进度`, never `进展`). "stopped moving" `停住不动了`: never `已暂停` (user-caused) or
  alarming `卡住` (tentative).
- `正在等待…响应` (Total Commander). `目标位置` / `来源` asymmetry is fine: they never render together.
- `可以取消它，也可以让它在后台继续运行。` (a bare imperative pair reads as an order); `日志里有详细信息。`

## Corner progress chip + failure notice (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- Dismiss `关闭` over `清除` / `移除` (both read as deleting). Aria `关闭这项操作的记录`: `关闭这项操作` would mean
  stopping it.
- `无法完成…操作`: `完成` wants a noun, so `无法完成拷贝操作`, `无法完成移到废纸篓的操作`, `other` `无法完成这项操作`.
- The tooltip's `·` is spaced (unspaced joins a name, `史蒂夫·乔布斯`); the count is its own fact
  (` · 共 {countText} 个项目`), since four `{label}` arms can't take an object. Destination `到“{destination}”`, quoted
  and glued (Han or Latin).

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

- `正在移动到“{destination}”` over Finder's `移到` (must match `queue.row.label`'s `正在移动`; `移到` is for
  `移到废纸篓`); copy `正在拷贝到“…”`.
- `archive_edit` splits: `正在编辑“{destination}”` vs `正在编辑压缩文件`. Generic arm `正在“{destination}”中进行操作`
  (`处理` wants an object).
- `在你做出选择之前，其余操作会一直暂停。`: `做出选择` over `回答`, since the title `文件已存在` isn't a question.

## Progress dialog F2 button (`fileOperations.transferProgress.background`/`backgroundAria`)

`后台运行` over bare `后台`: an action like `加入队列`, same width, echoes `仍在后台运行。` The aria
`让它继续在后台运行` keeps the label a verbatim substring; don't "restore" the tooltip's order.

## Quit-while-running gate (`main.quit.*`)

- `有 {countText} 项操作仍在进行，要退出吗？` (Finder's quit refusal, question last); the heading repeats `仍在进行` on
  purpose, `进行中` labels a row.
- `清理掉` over `清除` (an index or search) and `删除` (user-asked). `系统重新启动或退出登录` (`系统` keeps it from
  reading as Cmdr's sign-out; never `注销`).
- `继续工作` (never `稍后`, `取消` which cancels the operations, or cold `不退出`) and `立即退出` (Finder's "don't
  wait").

## Usage stats (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

`使用统计`, no `匿名` (a stable random id was never anonymous); `一个随机标识符` over an `ID` loan; `关联到`. ❌ Never
`假名化` / `匿名化`.

## Awaiting-answer row + rollback confirm (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

`需要你回答`, never opening `等待` (the queued status). The prompt `那个问题` (`提示` is the hint banner). Keep
`保留文件` (bare `保留` could mean the operation). The rollback tooltip's Stop `停止`, ❌ `取消` (Cancel keeps finished
files).

## Unusable name + rename chain (`fileOperations.validation.nameNotUsable`, `fileExplorer.rename.chainKeptOriginalName*`)

- `trashUnconfirmedToast`: `无法确认…是否已…`, then `这个宗卷可能比较慢，但…也许已经…了`.
- Chain toast: `{reason}` last after `：`, hung on `“{name}”…也一样` alone (macOS's merged form would bind it to every
  file).
- `这个文件名不能使用` / `这个文件夹名不能使用` (Finder), no `。`: composed into `{reason}。`

## Suggested ops dialog (`suggestedOps.*`, `commands.suggestedOpsShow.*`, `menu.view.suggestedOps`)

Title, menu, palette `AI 建议` (the mkdir hints) over `建议的操作`. Approve `批准` over macOS's `接受` (AirDrop); reject
`拒绝`; pills `这项操作可以撤销：` / `这项操作无法撤销：` (Finder). Why `原因：` (aria `原因，按 AI 的说法：`) over
`为什么：`.

## Duplicate (`commands.fileDuplicate.*`)

Finder's own File-menu pair keeps the commands apart: `复制` Duplicate, `拷贝` Copy. Description
`在当前文件夹中为选中的文件创建副本`.

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Finder's own labels (Home `个人`, Sort By `排序方式`); the Select menu `选择` (Nautilus/Dolphin; no Finder
  counterpart).
- ⚠️ Finder stays Latin across the whole catalog: switching to `访达` means changing every key at once.
- Changelog `更新日志` in `menu.app.changelog` and `whatsNew.dialog.seeFullChangelog` (one document; Microsoft's
  `更改日志` split them); What's new `新增功能`. Busy `（占用中）` over Microsoft's `忙碌`.

## OS-mount fallback notice (`fileExplorer.network.osMountFallback.*`)

Native `内建` (`内置` is built-in hardware); first `macOS 内建的 SMB 网络连接`, then `系统连接`. `慢 4 倍` (the ratio
isn't load-bearing); `试试直接连接` over `尝试` (keeps Try's "might not work"). Reassure: the share works, only slower.

## Rename/create refusals (`errors.mutation.*`, `errors.volume.*`)

- Top folder `顶层文件夹` (the volume's top), distinct from the chosen `根文件夹`.
  `因为 macOS 的“系统完整性保护”，这个项目无法重命名。` (`因为` over Apple's `由于`).
- MTP handle lost `跟丢了`; delete pending `正在退场` / `还有东西占着它`; timeout `也许仍会生效` (⚠️ still waiting, not
  a failure); reset `设备的连接已重启` (never the device).
- Across an archive boundary `请改用“移动”。`, `移出` / `移入`, never `移到`. `“{path}”已经不存在了。` /
  `“{path}”那里已经有东西了。` (vague `东西`, like the English). No trash `这个宗卷没有废纸篓，只能彻底删除。` (the
  command's name). `macOS 拒绝把这个项目移到废纸篓。` (`它` has no antecedent under the name field).

## Crash dialog (`crashReporter.dialog.body.*`, `settings.updates.crashReports.description`)

- ⚠️ `.keptRunning` / `.unknown` never say Cmdr crashed or quit (the app may have run on): `出现了问题` (AppKit), not
  `遇到问题`. `之后一直在运行`, never present-tense `仍在后台运行`.
- The second sentence `这是一份报告…` drops `崩溃`; `.ended` keeps `崩溃报告`. The setting says `一份报告` for both,
  label `发送崩溃报告`; `应用版本` over `App 版本`.

## Eject / disconnect error copy (`errors.eject.*`)

- These continue `fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast` after the colon, so they don't echo its
  verb (`没法断开它`, `没有连接需要断开`).
- Unplug `拔下线缆` (subject `设备`); a timeout isn't failure, `可能过一会儿它会自己推出`.
- Either wrapper may be a share or a phone: `X 还在使用那里的某些内容。…，然后再试一次。`, never `这个驱动器` or
  `再次推出`. `请把它关掉` / `请把它们关掉` point at the app or apps. Eject all → `全部推出` (Finder `N199.1`). Cmdr
  stays subject, never `我们`.
- A disk image `还开着` (English "open", unlike `挂载着`); `那里存放的`. `请稍等一会儿` / `请稍等片刻` mirror two
  lengths. Cmdr's bug `发送一份报告`, never `发送错误报告`. Processes last, `分别是 {processes}` (`名为…的` can't hold
  `其他`).

## Trash toast (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `menu.edit.undoTrash*`)

Undo `撤销`, not `回滚`; `撤销将“{name}”移到废纸篓` (Finder's `撤销将标签颜色设定为…`). `这个驱动器没有废纸篓。`, like
`压缩文件里没有废纸篓。`

## Error-report notes (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

`添加到` over bookish `补充`, title and button echoing each other; `已发送的内容` pairs `即将发送的内容`; button
`查看报告或添加备注` (`查看` reads contents; `显示` would sound like the View menu).

## Select/deselect dialog (`selection.*`)

- Tooltips front the location (`在焦点窗格中选择这些文件`): a tooltip needn't open on the button's words, whose
  accessible name comes from the label.
- `最近的选择` twins `最近的搜索`; `应用最近的 {mode} 选择：{query}`, `{query}` last so any input reads right.

## Converged drift (`queue.row.dismiss*`, `ui.toast.dismissAria`, `commands.navBack.label`, `fileExplorer.errorPane.goBack`, `askCmdr.decision.verbCopy`, `fileExplorer.summary.fileNoun`)

- Dismiss never `忽略`: macOS keeps it for real ignoring (`忽略拼写`), and on a success toast it told users to ignore
  what they just did.
- `示例：` for "Example:", `例如` only for "e.g."; on disk `占用磁盘` (Double Commander), since `占用空间` also fits
  logical size.
- Back `返回`, never `后退` (zero macOS hits); home `前往个人文件夹`, never `打开`. `askCmdr.decision.verbCopy` `拷贝`
  (`复制` collides with Duplicate).

## Deliberate splits (`shortcuts.scope.app`, `onboarding.wizard.back`, `indexing.step.statusDone`, `shortcuts.section.filterModified`, `askCmdr.renameUndo.undone`, `viewer.search.regex`, `ai.local.statusRunning`, `fileOperations.transferProgress.stageScanning`, `ui.select.placeholder`, `fileExplorer.tabBar.unreachableAriaLabel`)

One English word, two Chinese renderings; don't merge:

- Back: navigation `返回`, wizard `上一步` (pairs `下一步`). Both: a toggle cell `两者`; `都不用`, `两者都用`. Done:
  announced step `完成`, status `已完成`.
- Error status cell `出现问题`. Modified: date `修改日期`, user-changed shortcut `已修改`.
- Put back: trash `放回原处`; an undone rename `已恢复…原来的名称` (nothing moved, so `原处` would lie).
- Regex: chip `正则`, tooltip or accessible name `正则表达式`. Running: a local server `运行中`, a task `进行中`.
  Scanning: progress `正在扫描…`, step `扫描` (AppKit). Select: command `选择`, unset dropdown `请选择`.

## Finish rollback (`operationLog.dialog.rollBackRest`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleRest`/`.rollBackRest`, `queue.row.reversalInFolder`, `fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`/`.rollbackAlreadyLandedTooltip`)

- `完成回滚` over `继续回滚` (`继续` is `queue.row.resume`; native review pending). The notice reuses `保持原样` and
  `跳过没有把握的部分` verbatim.
- `in {folder}` is a trailing `位于 {folder}` (as `downloads.toast.inSubdir`), so the cell never reads as the folder
  being deleted; bare folder name.
- Rolling back a move deletes nothing: `…放回原处`, ❌ `删除`.

## Rollback-done notice (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- `reason.*` shares `askCmdr.renameUndo.skipReason.*`'s shape (`style.md`); only `failed.*` breaks it,
  `Cmdr 没能撤销 {name} 的改动。` (the drive refused).
- Count `个项目` (created folders count too; renameUndo counts `个文件`). Remove what it wrote `删除`; `移除` is out of
  a list or zip; moving back `放回`, never `挪回`.
- `done*`'s all `全部`, with a `=1` arm `这一项` (never `全部 1 个`); ⚠️ never `全部` on `some*`. `stopped*` share
  `其余的还留在 Cmdr/这次移动把它们放到的地方。`; `…N 个项目后停止了。`; `leftBehind` repeats `跳过没有把握的部分` then
  `：`.
- Occupied `它原来的位置现在被别的东西占用了。` (renameUndo's `名称` twin). The staged leftover is Cmdr's file:
  `不完整副本`, `清掉`; retry `下次连接到那里时`, never a time.

## Old WebKit / macOS (`main.oldWebkit.*`, `main.oldMacos.*`)

`“软件更新”` quoted; `X 及更高版本` (System Settings); `尽力而为`; "look off" `不太对`. Frank and light: no apology, no
warning.

## Inspect-file consent (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- Prose `只有在你问到某个文件时才会查看它的内容`; no "contents are never sent" promise, because the English dropped it.
- Camera details `相机信息` (Photos.app; `摄像头` / `照相机` are devices). `文本` is file content, `文字` is text in a
  photo (`识别出的文字`).
- PDF `标题和作者` (Preview.app); `其中有限的一部分`; `未经你批准，绝不会更改任何文件`.

## Open terminal here (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

`在此处打开终端` identical in menu, palette, and settings; `选取 App…` (a picker); `终端 App`.

## git worktree (`errors.git.orphanedWorktree.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

`worktree` (the git feature) Latin, as the `@key` asks, one name across the error pane, setting, and Size column;
generic prose `工作树`.

## OOXML row (`settings.archives.ooxml.*`)

`文稿` (Apple), never `文档`; packages `软件包`, wider than the card below's `应用程序包`, as the English contrasts
them.

## Server rows (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`disconnect*`/`forget*`)

- Try again `重试`, never `再试一次` (`zh-Hant`'s call). Host key `主机密钥`, a second mention `密钥`.
  `servers.refusal.unreachable` `Cmdr 连不上 {host}。` (one-line pane). Compromised `已泄露` over `已失陷` (tentative).
- Forget saved password `清除保存的密码` (dialog body too), not Apple's Wi-Fi `忽略此网络`. Every Forget alert's button
  is `忘记` (`forgetConfirmButton`), the checkbox `同时清除保存的密码`. "Nothing gets unmounted" `卸载`
  (`removeHostConfirm`): `推出` would read as disconnecting.

## Servers hub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.*Pin*`, `shortcuts.scope.servers`/`places`)

- `上次使用时间` (Keychain Access). Type `类型` (protocol), never `种类` (file kind). Signed out never `被拒绝` /
  `认证失败`.
- `等你核对主机密钥` over log-like `等待用户确认`; never `从未`, not `永不` (a setting's "never again").
- `显示服务器` (Show → `显示`, `前往` only for Go to); `断开服务器连接`, never `推出`. Pin toasts don't say `固定`, like
  the English; the unpin toast answers "was it deleted?".
- Account suffix is a noun label, `身份：{username}` / `身份：客人` (`servers.hub.shareAccount`/`.guestAccount`): button
  `以客人身份连接`. Browse as guest `以客人身份浏览`.

## Add-server sheet + host key (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

- `登录 {name}`, no preposition; `已从 {name} 退出登录`. Passphrase `密码短语`, the key's `密钥密码短语` (bare `密码` is
  the account's).
- Guest `客人` (macOS; `来宾` is Microsoft's): `servers.sheet.connectAsGuest`,
  `fileExplorer.network.browser.status.guest`, `errors.shareList.signingRequired` change together.
- `连接方式` over NetAuthAgent's `连接身份` (asks whether to use an account). `有东西夹在你和它之间`, never
  `中间人攻击`.
- Owner `所有者`, not `管理员`; `我核对过密钥了` / `证书了`; `仍然添加` (like `仍然使用`); Sign in as… `用其他账户登录…`
  (`fileExplorer.network.share.signInAs`); a cancelled connect `{name} 还没有连接` (a state).

## Reconnect + nothing-to-ask (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

`登录这台服务器用的是密钥，而不是密码`: the subject is the sign-in ("the server signs in" reads as the server logging
in); no `认证` / `验证身份`. The two reconnect and nothing-to-X shapes are in `style.md`.

## Pin, trusted keys, ADB settings (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.appearance.tintSmb.*`)

- Pin `固定`, ❌ `置顶` (move to top); Apple uses `固定` this way (`在菜单栏中固定`). `从宗卷选择器中取消固定` mirrors
  `固定到宗卷选择器`; `servers.pinHint.body` quotes it whole.
- `受信任的主机密钥`, date prefix `已信任`; `未找到`; `已找到：{path}`; `再次检查` (Check Again).
- `正在留意接入的手机` (`等待` reads as stuck; tentative); no ADB server or sockets. `选取 adb 命令`.

## ADB phone states (`adb.*`, `settings.behavior.adbHintDismissed.*`)

- `部` sits deliberately beside `这台服务器` / `这台 Mac`. `Cmdr 找不到 X。` over `无法找到` (absence, not refusal).
- Three unreachable states, three verbs (`已经断开连接了`, `连接中断了`, `没有响应`): they show in one spot in turn.
- `等你允许 USB 调试`, not `正在等待用户授权`; `怎么做` over `如何操作`; `你停止了打开手机。` (`取消` points at Cancel);
  `把线缆拔下再插上`; `换一根线缆或另一个端口试试`.

## Locked server identity (`servers.sheet.accountLocked`, `fileExplorer.navigation.forgetSecretNoneToast`)

The hint uses the buttons' exact words (`忘记`, `添加`); `决定了这是哪台服务器`, never `命名` (the form has a Name
field). `{name} 没有保存的密码。`, no `过` ("never saved").

## Retry totals + server-row menu (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `menu.network.open`, `menu.network.edit`)

The retry totals keep the `plural` shell, no preposition or `。` (parts of `retryKeepsTrying`). `Cmdr 不会连接到 {name}`
(standing), never `已停止`. A server row's Open `打开`, as in Finder.

## AI subject (`suggestedOps.agentReason`, `suggestedOps.reversible.moveBack`, `askCmdr.error.notConfigured`, `settings.askCmdr.provider.off`)

The rule is in `style.md`. `AI` in `suggestedOps.*` keeps what the model said apart from what Cmdr checked; ❗ don't
unify. Subjectless `聊天` / `开始聊天`; `Cmdr 发送的内容` pairs `Cmdr 记住的内容`.

## Review verb (`askCmdr.wake.*`, `askCmdr.wakeToast.*`)

`复查` (AppKit `复查更改…`) is ONLY the approval gate (`askCmdr.wakeToast.action`, `待你复查的内容`); the why-link
`看看原因`, onboarding's `检查后应用`. `askCmdr.wake.needsFullDiskAccess` contains `search.coverage.setUpFullDiskAccess`
verbatim.

## AI provider wizard (`onboarding.cloudSetup.*`)

Endpoint `端点` over Windows' `终结点`; `地址` over `网址` (catalog and macOS ~7:1); `ollama pull` `拉取` (`请求` is
half of "pull request").

## Dock nudge + menu (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`, `menu.dock.*`)

- `好，添加到程序坞` (Finder); `要把 Cmdr 留在程序坞里吗？` (Dock's `在程序坞中保留`); `固定` / `取消固定` (the Dock's
  items don't pair). `下次登录 Mac 时` (not a server).
- Not reloaded: `图标已经放好了…只是…还没刷新`, ⚠️ never "not pinned". `这次没能添加到程序坞` vs `无法把自己加进去` (a
  managed Dock, lasting).
- Dock menu `搜索文件…` = `menu.edit.searchFiles`; `前往文件夹…` (Finder), not `前往路径…`; `{name}（{parent}）`.

## Reveal nudge (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

`“在 Finder 中显示”` quoted as the settings card names it; the first-takeover notice explains what happened and where
the switch is, ❌ no apology.

## Onboarding (`onboarding.stepBeta.checklist.*`, `onboarding.stepOptional.*`, `onboarding.wizard.stepTooltip`, `onboarding.stepFda.*`, `onboarding.stepAi.*`)

- `加星标` / `星标` (GitHub zh docs); `仓库`, not `存储库`; AlternativeTo `点赞`. `邮件列表`, ⚠️ not the TBX's
  `邮寄列表` (postal). `看看是不是打错了`.
- `“本地网络”` quoted with `访问` outside; `进一步了解“{topic}”` (Finder; runtime name, so quoted).
  `第 {step} 步，共 {mandatory}+1 步` (the literal `+1` flags the optional step).
- `macOS 内建的处理程序`; "dumber" `笨不少` (keep the bluntness); `正式发布的 Cmdr 版本` / `开发版和测试版`, not `构建`.
- `正在获取“{fileName}”以便预览` over `抓取` (crawling) and `下载` (a phone isn't a download);
  `{doneText} / {totalText}` spaced because the units are.

## ADB stale index (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone` / `.bodyPhone`)

❌ No disconnect (the phone stays plugged in). `Cmdr 自己做的更改会及时更新到这个索引里`, never `让索引保持最新` (it may
NOT be current). `要重新扫描后才会显示` (`才` = not before then).

## Root and start folder (`servers.sheet.rootFolder*`, `.startFolder*`, `.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

`根文件夹`, never `顶层文件夹` (the volume's top); `起始文件夹` (Safari `起始页`), never `启动文件夹` (Windows
autostart); `不会越过这个文件夹往上走`, not `上层文件夹` (a command name; tentative).

## Mount and share-list errors (`errors.mount.*`, `errors.shareList.*`)

`这台电脑`, never `Mac` (Linux too); `发行版` tentative; `“{server}”上`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

`文本编辑器` (the class), never the `“文本编辑”` app; `默认文本编辑器`; `编辑文件时使用` + app, the terminal picker's
shape (tentative); system default `跟随系统`, the app after it in full-width parens.

## Drive left mid-operation (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.destination.copy`, `errors.write.moveNotConfirmed.title`, `fileOperations.leftovers.stagingFolderKept`)

- `正在断开连接` / `断开了连接`; `{done} 个文件（共 {total} 个）` (Finder). Which drive left rides the preposition
  (`往它拷贝` / `往 {counterpart} 拷贝`), not stiff `来源` / `目标位置`.
- Reassurance keeps `都` (`其余的都还在…`, `都留在了原处`), else it reads as an aside; `所以什么都没丢失` is the point.
- ❗ `无法确认这次移动`, never "failed": the files may have arrived. The staging folder holds the USER's files: `发现`,
  `没完成的`, ❌ `不完整` or a delete; `…原处，也就是在…里` keeps Cmdr from seeming to move them.

## Favorites menu (`commands.favoritesOpen.label`/`.description`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- One name, `个人收藏`, never `收藏夹` / `书签`; `个人收藏菜单`, never `…选择器` / `…面板`; `显示个人收藏`; classifier
  `项` (`个个人收藏` doubles).
- `按数字键前往对应的文件夹` (`前往` needs an object); `已经不…了`; fold state `展开`, never `打开`.
  `favoritesCantAddHere` pairs `favoritesAlreadyAdded`, reason after `：`, no `。`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

Kind `种类` (Finder); `选择所有扩展名为 *.{extension} 的文件`; `menu.context.selection` the noun `选择`, same as the
menu.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`)

`没有完全磁盘访问权限`, `没有` over `无` (must equal `onboarding.stepAi.bannerTitle.denied`; the aria opens with it);
`macOS 自己留着的文件`, no feature name.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

Messages open `你选中的项目里有 {count} 个，…` (reads right at 1 and 7). The badge is quoted from
`onboarding.fdaBadge.label`; `suggestion.other` names `技术详情`.

## Cloud-only delete warning (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Prose `只在云端`; names keep Finder's `仅存在云端`.
- Four facts stay: trash downloads first; so only Delete is offered; no copy in the trash but the cloud keeps one (❌
  never "still in the trash"); `设为离线可用`. The all-online variant drops "deselect". `“删除”` is `confirmDelete`'s
  label; both `<strong>` stay.

## Share not on server (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Retrying can't help: no `暂时`, no `再试一次`; `这种情况等下去也不会好` over system-fault `不会自动恢复`; `慢得多` (no
ratio).

## Look-alike names (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.explanation`, `errors.volume.ambiguousName`)

`看起来一样，但服务器存储的写法不同`, ❌ `编码` / `规范化` / `Unicode` (tentative). `“覆盖”`; `现有的那一项`; error
prose keeps spoken `改名` / `上一级文件夹`.

## Cloud AI consent (`ai.cloudConsent.*`, `askCmdr.gate.*`)

`允许使用云端 AI` (`使用` supplies the verb); quote it verbatim wherever referenced, "Allow it" `允许使用`. `AI 服务`
stays distinct from `提供方`.

## Open With / Share loading rows (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`正在查找 App…`; `没有共享选项` (macOS's `没有服务可应用` shape).

## Apple app and product names Apple localizes (`settings.advanced.showSafeSaveFiles.description`, `errors.provider.iCloud.*`, `errors.listing.emptyRootICloud.*`, `shortcuts.system.finderSearch`)

Third-party "Drive" products keep their Latin names; Apple's are in `style.md`.

## Whole-catalog wording rules (`viewer.saveAs.*`, `viewer.copyDialog.*`, `settings.onboarding.completed.label`, `onboarding.wizard.restart`, `onboarding.stepFda.postAction.body`, `queryUi.results.live.foldersScanned`)

`存储` only for data on a drive (`无法存储大于…的文件`). Restart buttons `重新启动` (Apple), quoted to match; prose may
say `重启`. Scanned folders `已扫描`, not `已查看`.

## Source not removed (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

Cmdr's failed step `移除` (like `正在移除原文件…`), the user's `删除`; `什么都没丢失` repeats
`deviceDisconnected.sided.destination.copy`, not `originalsKeptAside`.

## Managed settings (`settings.managed.*`, `ai.managed.*`, `updates.status.managedOff`, `errorReporter.dialog.managedOff`)

`由你的组织管理` (Apple MDM `受…组织管理`); `你的组织关闭了…`, plain, no apology. Terse card values `关闭`
(`settings.ai.provider.opt.off`), `只限本地`, `只能手动检查`.

## Multi-rename (`multiRename.*`, `commands.fileMultiRename.*`, `menu.file.multiRename`)

Total Commander zh-CN labels: `批量重命名`, mask `名称规则` (TC `重命名规则`, not DC's `掩码`), `计数器` / `起始值` /
`步长` / `位数`, `不变` / `全部小写` / `全部大写`. `查找` / `替换为` (Finder BulkRename, TC 6613), not `搜索`; status
`状态`, not `备注`.

## NFS export (`servers.sheet.nfs*`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`, `servers.sheet.identityLockedNfs`)

- Export `共享文件夹` (Synology DSM zh-CN) over `导出` (a data-export button); the port switch quotes Synology's
  `允许来自非特权端口的连接`.
- uid/gid `用户 ID` / `群组 ID` (Finder Get Info's 用户/群组; `分组` is rows).

## Range editor (`multiRename.rangeEditor.*`)

- Range editor: `起点` / `终点` (a position, not the counter's `起始值`); ordinals `第 # 个` / `倒数第 # 个`, `=1` →
  `最后一个`.
- Media fields (`multiRename.mediaField.*`): music names follow Music.app zh_CN (`艺人`, `专辑艺人`, `音轨编号`,
  `光盘编号`, `类型`, `注释`), photo ones Photos/Spotlight (`拍摄日期`, `镜头`, `焦距`, `方向`); a field is TC's `字段`,
  the button `媒体` (DC `媒体类型`). Camera model is `型号`, never the AI `模型`.

## Security notices (`securityNotices.updateNotGenuine.*`, `updates.failure.notGenuine`, `servers.paneState.guestSessionRefused*`)

- Genuine `真正的` (`不是真正的 Cmdr 版本`): a real release, never `正版` (piracy). Guest session `客人会话`, pairing
  the shipped `以客人身份连接`. Both await a native read (`review-queue.md`).
