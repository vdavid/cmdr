# zh decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each defending a choice against a well-meant "fix". The
brief pulls a section when its heading cites a batch key, so every heading cites keys in backticks. Term forms live in
`terms.json`; voice and typography in `style.md`. Evidence tiers: macOS zh-CN (Tier 1), Microsoft zh-Hans (Tier 2), the
file-manager family (Tier 3); the pile is `_ignored/i18n/zh/` in the main clone.

## Settings vocabulary (`settings.section.*`, `settings.appearance.*`, `settings.summary.*`)

- Section names: `外观`, `行为`, `语言`, `文件系统`, `SMB/网络共享`, `MTP（Android/Kindle/相机）`, `查看器`, `开发者`,
  `更新与隐私`, `高级`, `键盘快捷键`, `许可证`, `服务器（SFTP、WebDAV）`, `Android（ADB）`: full-width parens and `、`
  inside a label.
- Navigation `导航` (the UI noun; Finder's `导览` is its verb). Theme `浅色` / `深色` / `跟随系统`; tint `着色` (macOS;
  Microsoft's `淡色` is the other sense). A toast is `提示`, never transliterated.
- Hidden developer strings open with `内部：`.

## View modes, columns, groups, and shortcut scopes (`shortcuts.scope.*`, `shortcuts.section.*`, `fileExplorer.columns.*`, `fileExplorer.navigation.group*`, `queryUi.*`)

- Views `完整` / `简洁`, as scopes `完整模式` / `简洁模式`. Scopes: `应用`, `主窗口`, `文件列表`, `宗卷选择器`,
  `服务器`, `共享位置`, `个人收藏菜单`, `命令面板`, `关于窗口`, `入门引导`.
- Columns `名称`, `扩展名`, `大小`, `修改日期`, `路径`, `操作`. Search modes `AI`, `文件名`, `内容`, `正则`; facets
  `模式`, `大小`, `修改日期`, `搜索范围`; type toggle `两者` / `文件` / `文件夹`.
- Switcher groups `个人收藏`, `宗卷`, `云`, `移动设备`, `网络`. `网络` is the GROUP; its server row and the scope are
  `服务器`. Don't let them merge back.
- `共享位置` (Places) over bare `位置` (file-system location) and `地点` (photos); tentative until bucket storage lands.

## macOS names in the reserved-shortcut list (`shortcuts.system.*`)

Reuse macOS's localized names (`聚焦`, `字符检视器`, `调度中心`, `空间`, `强制退出`, `截屏`, `录屏`, `系统设置 > 键盘`).
Finder stays Latin (`Finder 搜索窗口`): § 原生菜单.

## Error-copy conventions (`errors.*`)

- System Settings panes arrive as tokens (`{system_settings}`, `{privacy_and_security}`, `{full_disk_access}`), never
  hand-translated. A bare token gets a space on both sides (it may arrive CJK or Latin); inside a bold path the trailing
  `里` attaches to the last pane name, never the token. Pane names without a token: `Apple 账户`, `通用`,
  `登录项与扩展`.
- macOS UI labels in prose take `“…”` (`“显示简介”`, `“已锁定”`). Permission denied `没有权限`; authentication
  `无法通过身份验证`, never a bare failure word.
- "Here's what to try:" `可以这样试试：`; "Navigate here again" `再次进入这里`; "if it keeps happening" `如果一直这样`
  (joined with `；` for an English "or").
- "Something went wrong" `出了点问题`; "couldn't tell what" `也说不清是什么` (`errors.mutation.unexpected` and
  `errors.eject.unexpected` identical).
- A runtime app or folder name the English leaves unquoted stays unquoted (`{app} 还在…`, `名为 {folderName} 的`); quote
  only where the English quotes, or where a Han/Latin name meets Han text with no fixed spacing (a destination).

## Conflict-policy buttons and small dialog words (`fileOperations.transferDialog.policySkip`/`.policyOverwrite`, `fileOperations.transferDialog.operationAria`)

- `全部跳过` / `全部覆盖` even for one conflict: zh has only `other`, and the policy acts on the whole set.
- `operationAria` `操作`, no colon. Under the cursor `光标所在的`; OK `好`; Quit & Reopen `退出并重新打开`.

## Licensing, AI, and viewer vocabulary (`licensing.*`, `ai.*`, `viewer.*`)

- Licensing: `组织`, `续订`, `有效期至`. `你` throughout; the one `您` is the salutation in
  `licensing.dialog.mailtoBody` (the user writes to us).
- AI: `拖放`, `预算`, `速率限制`, `配额已用完`, `约 {amount}`, `费用未知` (`费用` over Microsoft's `成本费`),
  `免费，本地运行`.
- Viewer: encoding group `西文`; selection `所选内容`; `viewer.saveAs.defaultName` stays ASCII `selection` (a filename
  base).

## Indexing, downloads, error reporter, and MTP vocabulary (`indexing.*`, `downloads.*`, `errorReporter.*`, `mtp.*`)

- Index entry `条目` (`个`); replay `重放`; hotkey `注册` / `已注册`; report bundle `报告包`; daemon `守护进程`;
  `ptpcamerad`, `udev` verbatim.
- Free space in a readout `剩余`; `feedback.dialog.counter` stays the bare `{currentText} / {maxText}` fraction.
- Refine (AI search) `优化`; Page Up/Down `向上翻页` / `向下翻页`; "boring folders" stays playful, `无聊的文件夹`.

## Operation queue catalog (`queue.*`)

- `操作队列` over `传输队列`: the window also lists deletes, trashes, renames, and creations, and `传输` means a
  copy-or-move in flight. Same head noun as `操作日志`, so the two View-menu items pair.
- Classifier `项` (`这项操作`, `{count} 项操作`); generic items keep `个项目`.
- `后台` over `背景` (the visual backdrop). Resume `继续` over `恢复` (restore, as macOS uses it for versions).
- Status set: `等待中`, `进行中`, `已暂停`, `已完成`, `已取消`, `无法完成`; `Queued` is `等待中`, never `已排队` (it's a
  status, not the queue noun).

## Double-click the pane background (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

`窗格背景` in the label, `空白区域` in the description (Double Commander uses both); `前往上层文件夹` (Finder). Hint
buttons `不喜欢？` / `不再这样做` / `我喜欢`.

## FAT32 too-large-file error (`errors.write.filesTooLargeForFilesystem.*`)

`驱动器` over Finder's `磁盘` because the English says "drive"; "and N more files" `另有 {countText} 个文件`.

## Copy and delete dialog labels and the scan spinner (`fileOperations.shared.scanningTooltip`, `fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`)

- "doesn't exist yet" `还不存在`; "Cmdr will create it" `Cmdr 会在拷贝时自动创建它` (`自动` carries the reassurance).
- `queue.row.label` arms all `正在[动词]`; `创建` is the act, `新建文件夹` the menu label.

## Archive browsing (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.archive*`, `settings.archives.*`, `errors.mutation.archive*`)

- `压缩文件` over Finder's `归档`: every file manager names browsing INTO a zip with `压缩文件`, and `归档` carries a
  file-away sense. `归档` survives only for a rarely-opened collection (`照片归档`).
- `解压` over Nautilus's `提取` (a component, not a whole archive). Browse like a folder `像文件夹一样浏览`; summary
  `进入浏览`.
- Keys stay Latin: `按 Enter 键` + verb, `按 Esc 键` + verb, never `回车键` (AppKit keeps key names Latin in zh-CN).
  Full screen `全屏幕` (AppKit).
- Archive edit `正在编辑压缩文件` / `压缩文件编辑`; "removed from the zip for good" `将从 zip 中被永久移除`.

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- "Do nothing" `什么都不做` over Microsoft's stiffer `不执行任何操作`.
- Toast `已将剪贴板{图像/PDF/文本}粘贴为 {filename}`; `fileExplorer.clipboard.copiedPath` has the same shape
  (`已将路径拷贝到剪贴板：`) and drops "it's now on your clipboard" (Chinese adds no possessive to the one clipboard).

## Archive-password dialog and Compress (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `受密码保护`, `解锁`; the password field's aria `压缩文件密码`.
- Compression slider ends `更快` / `更小` (Total Commander's `最快压缩` / `最大压缩`, as comparatives); label
  `压缩级别`.
- The archive name in `fileOperations.transferDialog.pathErrorNotZip` is `压缩文件名`, and `“.zip”` hugs its Han
  neighbours.

## Operation log catalog (`operationLog.*` + `commands.logOperationLog.*`)

- Status words are `queue.row.status` verbatim, so the log and the queue agree.
- Rollback arms `无法回滚` / `可回滚` / `正在回滚` / `已回滚` / `已部分回滚`. Summaries `已[动词] {countText} 个项目`;
  "and N more" `另有 {countText} 个项目`; load more `再加载 50 条` (`条` for records).
- Initiators `你` / `AI 客户端` / `代理`.

## Network-drive image indexing catalog (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndex*`, `search.imageResults.networkOff/paused`)

- `照片` (classifier `张`) for the user's pictures, `图像` for the feature and its labels (`图像索引`,
  `未纳入图像搜索`): the English makes the same warm/technical split.
- "gently" `很克制` over `温和地`: the promise is restraint (limited speed, pauses on disconnect), not softness.
- Always index `始终索引`; terse status may say `尚未索引`, prose says `建立索引`.

## Ask Cmdr catalog (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `settings.section.askCmdr`, `commands.askCmdrToggle.*`)

- A chat `聊天`, the list of chats `聊天记录` (WeChat's word), New chat `新建聊天`.
- `token` stays Latin with `个` (`{countText} 个 token`); tentative, no settled Chinese UI term.
- Usage `用量` (a consumption metric, over Microsoft's `使用情况`), the Spending heading `花费`.
- A settings path in a sentence is quoted whole with the English separator: `“设置 › AI”`.

## Bulk rename review, image-index scope, and Ask Cmdr tool labels (`askCmdr.renameReview.*`, `settings.mediaIndex.scope.*`)

- "needs attention" `需要先处理`: the English is deliberately vague about what's wrong (tentative).
- Importance `重要性`, one noun (`askCmdr.tool.folderImportance`, the scope radio `自动，按文件夹的重要性`).
- "lost track of file system changes" `没能跟上文件系统的改动`: calm, `改动` matches the watcher copy.
- Badges `（扩展名）` / `（循环）` in full-width parens; rename cycle `重命名循环` (`周期` is a time period).

## Image-index status badges (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- Badge `标记` over `角标` (absent from the pile, jargon-y); tentative.
- File badges `已为图像搜索建立索引`, `等待建立索引`, `索引后有改动，将重新索引`, `无法建立索引`, `未纳入图像搜索`.
- Counts: `已索引全部 {totalText} 张图像`, `{totalText} 张图像中已索引 {doneText} 张`; the drive-wide pair keeps the
  settled `此驱动器` of the drive-index family.

## Image-indexing settings restructure + semantic-search model (`settings.mediaIndex.cards.*`, `.progressSummary.*`, `.semanticSearch.*`, `.clip.*`, `fileExplorer.imageIndex.file.indexing`)

- The feature named mid-sentence is quoted, `“通过描述搜索”`; the toggle IS the label, `通过描述搜索照片`, unquoted.
- `搭载 Apple 芯片的 Mac` (apple.com.cn) over a literal `Apple 硅`.
- Delete button `删除模型（释放 {size}）`; no `约` where the English gives a flat size.
- `fileExplorer.imageIndex.file.indexing` and `settings.mediaIndex.progressSummary.title` are both `正在建立索引` (one
  concept).

## Delete-dialog trash switch + transfer From/To group headings (`fileOperations.delete.trashSwitch`/`confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

- From / To headings `来源` / `目标` (Total Commander) over `从` / `到`: coverbs need an object, and the path sits BELOW
  the heading. In a sentence the destination is `目标位置` (bare `目标` reads as "goal").
- The switch `移到废纸篓` and the confirm button `删除` equal `transferDialog.titleVerbOnly`'s arms.

## Drive-indexing master-switch strings (`driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`overriddenBadge`)

- A settings path in a sentence is quoted whole, `在“索引 > 驱动器索引”中开启`, never bare (bare puts spaces between
  Han). Apple quotes each element; Cmdr's one-pair form wins for consistency.
- The overridden badge `已随驱动器索引关闭`: a badge is a state, so `已…`; bare `随…关闭` reads as an instruction.
- `开启` for a Settings toggle; the per-drive menu keeps `打开索引` (`为此驱动器打开索引`). Pick by which switch the
  string points at.
- `此驱动器` is the drive-index family's terse carve-out; elsewhere `这个驱动器`.

## 驱动器索引：检查更改这一趟 (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`)

The header `检查更改` has no `正在` (a header, not live status), like its siblings `首次完整扫描` / `快速更新`.

## 传输停滞提示 / stalled-transfer notice (`fileOperations.transferProgress.stall*`, `close`)

- `已有 {duration} 没有进度`: one progress word, `进度`, over `进展`.
- "has stopped moving" `停住不动了`: never `已暂停` (a pause the user caused) and never the more alarming `卡住`
  (tentative).
- Waiting for a response `正在等待…响应` (Total Commander's exact surface, plus Cmdr's `正在`).
- Destination `目标位置` and source `来源` are deliberately asymmetric: the two strings never render together.
- Two ways out: `可以取消它，也可以让它在后台继续运行。` (a bare imperative pair reads as an order).
- "The log has the details." stays a statement, `日志里有详细信息。`

## Corner progress chip + failure notice (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- Dismiss `关闭` over `清除` / `移除` (both read as deleting, and the row deletes nothing). The aria is
  `关闭这项操作的记录`: `关闭这项操作` would read as stopping the operation.
- "Couldn't finish X" `无法完成…操作`: `完成` wants a nominal object, so disyllabic verbs compound (`无法完成拷贝操作`)
  and phrases take `的` (`无法完成移到废纸篓的操作`); `other` is `无法完成这项操作`.
- Percent keeps the sign, `已完成 {percentText}%` (Finder); VoiceOver reads it.
- The tooltip's `·` keeps a space on both sides: an unspaced `·` joins a transliterated name (`史蒂夫·乔布斯`).
- The item count is its own fact (` · 共 {countText} 个项目`): four `{label}` arms are verb phrases that can't take an
  appended object in Chinese.
- The destination is quoted and glued, `到“{destination}”`: the name may be Han or Latin, so no spacing fits both.

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

- `正在移动到“{destination}”` over Finder's `移到`: it must read as the operation `queue.row.label` named (`正在移动`);
  `移到` stays for `移到废纸篓`. Copy is Finder's `正在拷贝到“…”`.
- `archive_edit` splits for real: `正在编辑“{destination}”` vs `正在编辑压缩文件`. Never collapse them.
- With a location, the generic arm is `正在“{destination}”中进行操作` (`处理` wants an object).
- The paused note `在你做出选择之前，其余操作会一直暂停。`: `做出选择` over `回答`, because the title `文件已存在` is a
  statement, not a question.

## Empty-queue state of the progress dialog's F2 button (`fileOperations.transferProgress.background`/`backgroundAria`)

- `后台运行` over a bare `后台`: an action like its sibling `加入队列`, the same four characters wide, and echoes
  `仍在后台运行。`
- The aria `让它继续在后台运行` puts `继续` first only so the label `后台运行` stays a verbatim substring. Don't
  "restore" the tooltip's order.

## Quit-while-running gate (`main.quit.*`)

- Title `有 {countText} 项操作仍在进行，要退出吗？`: `操作仍在进行` from Finder's own quit refusal; the question goes
  last. The heading `仍在进行` repeats it on purpose; `进行中` labels one row.
- `清理掉` over `清除` (reserved for an index or a search) and `删除` (a delete the user asked for).
- `系统重新启动或退出登录`: `系统` keeps `退出登录` from reading as Cmdr's sign-out; never Windows' `注销`.
- Buttons `继续工作` (never `稍后`, `取消`, which would cancel the operations, or a cold `不退出`) and `立即退出`
  (Finder's `立即` family means "don't wait").

## 使用统计：去掉“匿名”，写明“一个随机标识符” (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

`使用统计` with no `匿名` (a stable random id means they never were anonymous); `一个随机标识符` over an `ID` loan;
`关联到`. ❌ Never `假名化` / `匿名化`: the English avoids exactly that jargon.

## 等待回答的队列行 + 回滚确认框 (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `需要你回答`: never opening on `等待`, which is the queued status in the same column.
- The prompt is `那个问题`: `提示` is the hint banner.
- Keep button `保留文件`: a bare `保留` beside `回滚` could mean keeping the operation.
- Stop in the rollback tooltip `停止`, ❌ never `取消` (Cancel keeps the finished files; the tooltip exists to say
  rollback doesn't).

## 无法确认的重命名 + 名称不可用 + 重命名链计数 (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`, `fileExplorer.rename.chainKeptOriginalName*`)

- "Couldn't confirm" family: `无法确认…是否已…`, then `这个宗卷可能比较慢，所以…也许已经…了`, the same hedge across
  rename, folder creation, and trash.
- `名称也许已经改好了` mirrors `保留了原来的名称` (the opposite outcome) on the same noun; `重命名` is a verb and reads
  badly as a subject. ❌ The unconfirmed toast never says the file kept its name.
- Chain toast: `{reason}` stays last after `：`, hung on `“{name}”…也一样` alone; macOS's merged `“X”和其他 N 个文件都…`
  would bind it to every file.
- `这个文件名不能使用` / `这个文件夹名不能使用` (Finder `不能使用名称`), no `。`: it's composed into `{reason}。`

## 建议的操作：Ask Cmdr 提议内容的对话框（`suggestedOps.*`、`commands.suggestedOpsShow.*`）

- Approve `批准` over macOS's `接受` (AirDrop receiving); reject `拒绝`.
- `这项操作无法撤销` (Finder's sentence, with the spoken `这项`).

## 复制（Duplicate）：在同一文件夹内拷贝的命令（`commands.fileDuplicate.*`）

`复制` is Duplicate (Finder's File menu), `拷贝` is Copy: Finder's own pair, so the two commands side by side don't
clash. Description `在当前文件夹中为选中的文件创建副本`.

## 原生菜单：菜单栏、右键菜单、窗口标题（`menu.*`、`licensing.windowTitle.*`、`main.instanceLock.*`）

- Menu bar `文件`, `编辑`, `显示`, `前往`, `窗口`, `帮助`, `服务`; the Select menu `选择` (Nautilus/Dolphin, no Finder
  counterpart).
- ⚠️ Finder stays Latin (`在 Finder 中显示`), never Apple's `访达`: a deliberate whole-catalog choice; switching means
  changing every key at once.
- Get Info `显示简介`, Enclosing Folder `上层文件夹`, Go > Home `个人`, Sort By `排序方式`.
- Changelog `更新日志` in both `menu.app.changelog` and `whatsNew.dialog.seeFullChangelog` (one document); Microsoft's
  `更改日志` split them. What's new `新增功能` is the message, not the document.
- Busy `（占用中）` over Microsoft's `忙碌`, full-width, no space (`推出（{name}）（占用中）`).
- A name fixed in the string is bare (`打开 Cmdr`, `关于 Cmdr`); only a runtime-substituted name is quoted
  (`拷贝“{name}”`), as Apple does (`隐藏访达` vs Dock's `隐藏“%@”`).

## 系统连接回退通知（`fileExplorer.network.osMountFallback.*`）

- Native `内建` (macOS zh-CN has only `内建`; `内置` is "built-in hardware", another English word). First mention
  `macOS 内建的 SMB 网络连接`, later `系统连接`.
- "4x slower" `慢 4 倍`: the exact ratio isn't load-bearing.
- Try connecting directly `试试直接连接` over `尝试` (keeps the "might not work" of Try).
- Reassure, don't report an error: the share works, only slower.

## 重命名/新建被拒绝时的一行提示（`errors.mutation.*`、`errors.volume.*`）

- Top folder `顶层文件夹` (the volume's own top), distinct from the user-chosen `根文件夹`.
- `因为 macOS 的“系统完整性保护”，这个项目无法重命名。`: Apple gives the name no verb; `因为` over Apple's `由于` (the
  catalog's everyday word).
- MTP handle lost `跟丢了`; delete pending `正在退场` / `还有东西占着它`; timeout `也许仍会生效` (⚠️ still waiting,
  never a failure); MTP reset `设备的连接已重启` (the connection, never the device, restarted).
- A password that didn't work `不起作用`, ❌ never `密码错误`.
- Rename across an archive boundary `请改用“移动”。` (the command's name, quoted); `移出` / `移入`, never `移到`.
- `“{path}”已经不存在了。` / `“{path}”那里已经有东西了。` (the English is vague, so `东西`). No trash:
  `这个宗卷没有废纸篓，只能彻底删除。` (`彻底删除` is the command the user presses).
- macOS refused `macOS 拒绝把这个项目移到废纸篓。`: the object is `这个项目`, since `它` has no antecedent under the
  name field.

## 崩溃对话框与崩溃报告设置（`crashReporter.dialog.body.*`、`settings.updates.crashReports.description`）

- ⚠️ `.keptRunning` and `.unknown` never say Cmdr crashed, quit, or stopped: after a background panic the app ran on,
  and `.unknown` must hold for both outcomes. `出现了问题` (AppKit) says only that something happened; not Windows'
  `遇到问题`.
- `之后一直在运行`, never `仍在后台运行`: `仍在` is present tense, and the user already quit.
- The second sentence drops `崩溃` (`这是一份报告…`); `.ended` keeps `崩溃报告`.
- The settings description uses `一份报告` for both outcomes; the label stays `发送崩溃报告`. `应用版本` over `App 版本`
  (the Latin `App` is for cloud and error copy).

## Eject / disconnect error copy (`errors.eject.*`)

- Sentences land after a colon in `fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast`, so they continue it.
  Don't echo the wrapper's verb: `没法断开它`, `没有连接需要断开`; `没法` is the spoken stand-in for a second `无法`.
- `驱动器` over Finder's `磁盘` (the English says "drive"). Unplug `拔下线缆` (the subject is already `设备`).
- A timeout isn't a failure: `可能过一会儿它会自己推出`.
- The named refusals end like `unmountRefused` (`…，然后再次推出。`). Singular and plural app differ only in `它` /
  `它们`. `其他 App` takes no measure word (`几个` would invent a count); `Intl.ListFormat` supplies the joiner.
- `请关闭它在上面打开的内容` (`内容`, since "anything" is wider than files; close, not quit). A disk image `还开着` (the
  English says "open", unlike the "mounted" `挂载着` elsewhere). `macOS 还在处理这个驱动器`: `处理`, since the system's
  background work isn't an app using it.
- `请稍等一会儿` / `请稍等片刻` keep the English's two lengths. Cmdr's own bug: `发送一份报告`, never `发送错误报告`.

## 废纸篓提示条：撤销与前往废纸篓（`fileOperations.trash.*` + `commands.fileGoToTrash.*`）

- Put back `放回原处` (Finder's menu item) over `恢复`, which belongs to restoring an AI rename's old name, and over
  Nautilus's `从回收站恢复`.
- Undo `撤销` (a real undo), distinct from `回滚`.
- `这个驱动器没有废纸篓。` (a fact about the drive), same shape as `压缩文件里没有废纸篓。`

## 给已发送的错误报告补充备注（`errorReporter.amend.*`、`errorReporter.amendedToast.message`、`errorReporter.autoSentToast.viewOrAddNotes`）

- Add to `添加到` (Finder `添加到程序坞`) over the bookish `补充`; title and button echo each other like the English.
- `已发送的内容` pairs with `即将发送的内容`.
- View `查看` (read contents), ❌ never `显示` (the View menu). Button `查看报告或添加备注`.
- A menu in prose: `请从“帮助”菜单发送一份新报告。`; the bold `帮助 > …` path is for onboarding steps.

## 选择/取消选择文件对话框（`selection.*`）

- Select `选择`, deselect `取消选择`; ⚠️ Traditional uses `選取` for this sense, a real vocabulary split, not a
  character conversion.
- Tooltips front the location, `在焦点窗格中选择这些文件`: a tooltip is its own sentence and needn't start with the
  button's words (the accessible name comes from the label key).
- Recent selections `最近的选择`, the twin of `最近的搜索`; `应用最近的 {mode} 选择：{query}`, with `{query}` last so
  any input reads right.

## 收敛掉的真漂移（`queue.row.dismiss*`、`ui.toast.dismissAria`、`commands.navBack.label`、`fileExplorer.errorPane.goBack`、`askCmdr.decision.verbCopy`、`fileExplorer.summary.fileNoun`）

- Dismiss `关闭`, never `忽略`: macOS keeps `忽略` for real "ignore" (`忽略拼写`), and on a success toast `忽略` told
  the user to ignore what they just did.
- `示例：` for the full word "Example:", `例如` only for "e.g.".
- On disk `占用磁盘` (Double Commander `占用磁盘空间`); `占用空间` is too loose, since logical size also takes space.
- Back (history) `返回`, never `后退` (zero hits in macOS; the menu and palette had split). Go to home folder
  `前往个人文件夹`, never `打开` (it navigates).
- `askCmdr.decision.verbCopy` `拷贝`: in a confirm prompt `复制` collides with the Duplicate command.
- Counted nouns always carry a measure word (`个文件`, `个目录`): `3 / 10 文件` is ungrammatical, not just inconsistent.
- `双击` / `右键点击` stay: Tier 3 agrees unanimously and macOS has no counter-evidence, so don't coin `连按` /
  `右键点按`.

## 故意保留的分界（英文一词多义，中文必须分开）（`shortcuts.scope.app`、`onboarding.wizard.back`、`indexing.step.statusDone`、`shortcuts.section.filterModified`、`askCmdr.renameUndo.undone`、`viewer.search.regex`、`ai.local.statusRunning`、`fileOperations.transferProgress.stageScanning`、`ui.select.placeholder`、`fileExplorer.tabBar.unreachableAriaLabel`）

One English word, two correct Chinese renderings. Don't merge them:

- App: a scope `应用`; beside `macOS` as a source, the Latin `App` (`App 内`).
- Back: navigation `返回`; the wizard's step `上一步` (pairs `下一步`).
- Both: a toggle cell `两者`; opposite `都不用`, `两者都用`.
- Done: the announced step `完成`; the lifecycle status `已完成`.
- Error status cell: `出现问题`, never `错误`.
- Modified: the date `修改日期`; a user-changed shortcut `已修改` (not a date at all).
- Put back: from the trash `放回原处`; an undone rename `已恢复…原来的名称` (nothing moved, so `原处` would lie).
- Regex: a cramped chip `正则`; a tooltip or accessible name `正则表达式` (the button shows only `.*`).
- Running: a local server process `运行中`; a task `进行中`.
- Scanning: progress `正在扫描…`; a step name `扫描` (AppKit makes the same split).
- Select: the command `选择`; an unset dropdown `请选择`.
- Unreachable: a host `无法连接`; a path or volume `无法访问` (an ejected volume has no connection).
- View: the verb `查看`; the menu `显示`.

## 完成中断的回滚（`operationLog.dialog.finishRollBack`、`operationLog.rollback.partiallyRolledBackNotice`、`fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`、`queue.row.reversalInFolder`、`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`/`.rollbackAlreadyLandedTooltip`）

- Finish rolling back `完成回滚` over `继续回滚`: the English says Finish, and `继续` is `queue.row.resume` (native
  review pending).
- The notice promises nothing more than the confirm dialog: it reuses `保持原样` and `跳过没有把握的部分` verbatim.
- `in {folder}` is `位于 {folder}`: a trailing locative beside a fixed first element (as `downloads.toast.inSubdir`
  does), so `正在删除新建的内容 位于 Backup` never reads as "this folder gets deleted". The folder name is bare, like
  that cell elsewhere.
- Rolling back a move deletes nothing: `…放回原处`, ❌ never `删除`.

## 回滚结束后的提示条 (`fileOperations.cancelRollback.*`、`fileOperations.rollbackConfirm.body`)

- `reason.*` shares the `保留了 X：原因。` shape of `askCmdr.renameUndo.skipReason.*`, with a bare `{name}`. Break it
  only for `failed.*` (`Cmdr 没能撤销 {name} 的改动。`): the drive refused, it wasn't Cmdr's choice.
- Count `个项目` here (the rollback also removes created folders); renameUndo counts `个文件`.
- Remove what it wrote `删除` (the whole rollback family); `移除` is for taking something out of a list or zip. Moving
  back is `放回`, never `挪回`.
- `all` in `done*` is `全部`, plus a `=1` arm `这一项` (never `全部 1 个`); ⚠️ never `全部` on `some*`: it would lie.
- `stopped*` share one frame: `其余的还留在 Cmdr/这次移动把它们放到的地方。`
- Stopped after N `…N 个项目后停止了。` (clause first). `leftBehind` repeats `跳过没有把握的部分` exactly, then `：`.
- Occupied `它原来的位置现在被别的东西占用了。`, the `位置` twin of renameUndo's `名称`.
- The staged leftover is Cmdr's own file: `不完整副本`, cleared with `清掉`. ⚠️ `之后往那里传输时`, never `下次`: the
  cleanup skips files under an hour old, so an immediate retry won't clear it.

## 旧 WebKit 与旧 macOS 提示（`main.oldWebkit.*`、`main.oldMacos.*`）

- Pane name quoted, `“软件更新”`; `Safari`, `Mac`, `15.4` spaced from Han.
- `X 及更高版本` (System Settings); best effort `尽力而为`; "look off" `不太对`. Frank and light: no apology, no
  warning.

## Ask Cmdr inspect-file consent + tool labels (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- The tool line `正在查看文件内容` / `已查看文件内容`; prose `只有在你问到某个文件时才会查看它的内容`. The old promise
  that no contents are ever sent is gone because the English dropped it.
- Camera details `相机信息` (Photos.app); `摄像头` / `照相机` are device senses.
- A photo's place `拍摄地点` (Photos.app `地点`), never `位置` (a file-system location here).
- `文本` is file content, `文字` is writing inside a photo (`识别出的文字`).
- PDF title and author `标题和作者` (Preview.app); a limited part `其中有限的一部分`.
- Never changes a file without approval `未经你批准，绝不会更改任何文件`.

## “在此处打开终端”与它的 App 选择器（`settings.behavior.openTerminalHereApp.*`、`settings.navigationAndFileOps.card.terminal`）

The command `在此处打开终端` is identical in menu, palette, and settings. Choose an app `选取 App…` (Apple's `选取` for
a picker); a terminal app `终端 App`.

## git 的 worktree（`errors.git.orphanedWorktree.*`、`settings.fileExplorer.git.showVirtualGitPortal.description`、`fileExplorer.git.size.linkedWorktrees`）

`worktree` (the git feature) stays Latin, as the `@key` asks; generic "working tree" prose is `工作树`. One thing gets
one name across the error pane, the setting, and the Size column.

## `Documents and packages`：新增的 OOXML 行（`settings.archives.ooxml.*`）

Documents `文稿` (Apple), never `文档`; packages `软件包`, deliberately wider than the card below's `应用程序包`, as the
English contrasts packages with app bundles.

## 服务器面板与卷切换器里的服务器行（`servers.*`、`fileExplorer.navigation.connectionTooltip*`/`disconnect*`/`forget*`）

- Classifier `台` (`这台服务器`). Try again `重试`, never `再试一次` (that's `zh-Hant`'s call).
- Forget saved password `清除保存的密码`, and its dialog body says `清除` too; not Apple's Wi-Fi `忽略此网络` (`忽略`
  reads as "skip").
- Host key `主机密钥`; a sentence mentioning it twice shortens the second to `密钥`.
- `servers.refusal.unreachable` `Cmdr 连不上 {host}。`, shorter for a one-line pane.
- Compromised `已泄露` over `已失陷` (tentative, native review pending).

## 服务器中心：表格列、状态与固定到宗卷选择器（`servers.hub.*`、`commands.servers*`、`fileExplorer.navigation.*Pin*`、`shortcuts.scope.servers`/`places`）

- A date column heading carries the time noun: `上次使用时间` (Keychain Access); the bare `上次使用` is mid-sentence.
- Type `类型` (protocol), never Finder's `种类` (file kind).
- Status set `已连接` / `已保存` / `已退出登录`; found nearby `在附近发现` without `已` (the user didn't cause it);
  signed out is never `被拒绝` / `认证失败`.
- Waiting for you `等你核对主机密钥` over `等待用户确认` (reads like a system log). Never `从未`, not `永不` (a
  setting's "never again").
- Show servers `显示服务器` (English Show → `显示`; `前往` only for Go to). Pin/unpin `固定/取消固定服务器`, slash
  unspaced. Disconnect server `断开服务器连接`, never `推出`.
- The pin toasts don't say `固定`, like the English; the unpin toast's second sentence answers "was it deleted?".
- A NAS "turn on" is power, `开机`.

## 添加服务器的模态表单、SSH 主机密钥确认、前往路径的预览行（`servers.sheet.*`、`servers.hostKey.*`、`servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`、`goToPath.dialog.opensServer`/`.addsServer`、`commands.serversConnect.label`）

- Save `保存`, never Apple's older `存储`. Sign in to X `登录 {name}`, no preposition; signed out
  `已从 {name} 退出登录`.
- Passphrase `密码短语`, so the key's is `密钥密码短语`: a bare `密码` would read as the account password.
- Guest `客人` (macOS zh-CN; `来宾` is Microsoft's); `servers.sheet.connectAsGuest`,
  `fileExplorer.network.browser.status.guest`, and `errors.shareList.signingRequired` change together.
- How to connect `连接方式` over NetAuthAgent's `连接身份`: the options ask whether to use an account.
- "something is sitting between you and it" `有东西夹在你和它之间`: the English avoids "man in the middle", so don't add
  `中间人攻击`.
- The server's owner `所有者`, not `管理员` (reserved for an administrator). "I've checked it" `我核对过了`, the verb of
  `等你核对主机密钥`.

## 自动重连的面板标题 + 无需输入的退出登录说明（`servers.paneState.reconnecting`、`.signedOutNothingToAsk`）

- `正在重新连接到 {name}…`, the shape of `正在连接到 {name}…`, so connecting and reconnecting read as two states.
- "there's nothing to X" is Apple's `没有要 X 的内容` (`没有要输入的内容`), not a chattier `没什么可…的`.
- `登录这台服务器用的是密钥，而不是密码`: the subject moves to the sign-in, since "the server signs in" reads as the
  server logging in. No `认证` / `验证身份`: the English is plain.

## 固定/取消固定的右键项与一次性提示、可信主机密钥页、Android（ADB）设置页（`menu.network.pinToSwitcher`/`.unpin`、`servers.pinHint.*`、`settings.servers.*`、`settings.adb.*`、`settings.section.servers`/`.adb`、`settings.appearance.tintSmb.*`）

- Pin `固定`, ❌ never Apple's list `置顶` (move to top): this keeps a row in the switcher; Apple itself uses `固定` for
  this sense (`在菜单栏中固定`).
- A group in the switcher is `分组`, never Apple's `群组` (people or devices).
- Trusted host keys `受信任的主机密钥`; the date prefix `已信任`. Not found `未找到` (a bare status); found
  `已找到：{path}`. Re-check `再次检查` (Apple's Check Again).
- Watching for phones `正在留意接入的手机`: `等待` would read as stuck (tentative). Never mention the ADB server or
  sockets.
- Choose the adb command `选取 adb 命令` (a file picker says `选取`).

## Android（ADB）手机的窗格状态、卷切换器提示与那条一行提示（`adb.*`、`settings.behavior.adbHintDismissed.*`）

- Android's own Chinese wins on the phone: `USB 调试` (AOSP), the `“允许”` button quoted, tap `点按` (AOSP's majority).
- Phone classifier `部` (AOSP `这部手机`), deliberately beside `这台服务器` and `这台 Mac`.
- `Cmdr 找不到 X。` over `无法找到` (it's absence, not refusal).
- Three unreachable states use three verbs (`已经断开连接了`, `连接中断了`, `没有响应`): they show in one spot in turn.
- `等你允许 USB 调试`, not `正在等待用户授权`. How `怎么做` over the bookish `如何操作`.
- `You stopped opening your phone.` `你停止了打开手机。`: `取消` would point at the Cancel button.
- Reseat the cable `把线缆拔下再插上`; another cable or port `换一根线缆或另一个端口试试` (each noun its measure word).

## 被锁住的服务器身份与已清除的密码（`servers.sheet.identityLocked`、`fileExplorer.navigation.forgetSecretNoneToast`）

- The hint's verbs are the buttons' exact words (`忘记`, `添加`): a synonym sends the reader after a menu item that
  doesn't exist. `决定了这是哪台服务器`, never `命名` (the form has a Name field).
- `{name} 没有保存的密码。`: no `过`, which would mean "never saved".

## 重试总时长与服务器行的右键菜单 (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `menu.network.open`, `menu.network.edit`)

- The retry totals keep the `{…, plural, other {…}}` shell (placeholders must match) and have no preposition or `。`:
  they're parts of `servers.paneState.retryKeepsTrying`.
- `Cmdr 不会连接到 {name}` (a standing refusal), never `已停止` (an interrupted attempt).
- A server row's Open is `打开`, the same verb as opening a file, as in Finder.

## AI 文案改写：主语从 “Ask Cmdr” 换成 `Cmdr` / `AI`（`suggestedOps.agentReason`、`suggestedOps.cmdrFacts`、`askCmdr.error.notConfigured`、`settings.askCmdr.provider.off`）

- `Ask Cmdr` only names the panel (its title, the View-menu item, the palette command, the settings section and its
  on/off copy); a sentence about what the AI does says `Cmdr`.
- The four `suggestedOps.*` sentences say `AI`: `AI 给出的理由` sits beside `Cmdr 掌握的信息`, and the dialog exists to
  separate what the model said from what Cmdr checked. ❗ Don't unify them.
- Chatting without a subject `聊天` / `开始聊天`; `Cmdr 发送的内容` / `Cmdr 记住的内容` read as a pair.

## 状态角落的两条 AI 提示与“复查”这个动词 (`askCmdr.wake.*`, `askCmdr.wakeToast.*`)

- Review `复查` (AppKit `复查更改…`) over `检查` (that's check, used all over) and `审核` (bureaucratic).
- ❗ `复查` is ONLY the approval gate: `askCmdr.wakeToast.action` and `待你复查的内容`. The quiet why-link is
  `看看原因`; onboarding's "review and apply" `检查后应用`; reading a report or file `查看`.
- `askCmdr.wake.needsFullDiskAccess` contains `search.coverage.setUpFullDiskAccess` verbatim (`设置完全磁盘访问权限`).

## AI 提供方设置向导的用词（`onboarding.cloudSetup.*`）

Endpoint `端点` over Windows' `终结点`; address `地址` over `网址` (the catalog and macOS agree about 7:1);
`ollama pull` `拉取`, not `请求` (half of "pull request").

## 程序坞邀请与程序坞菜单（`main.dockPinNudge.*`、`settings.behavior.dockPinNudgeOfferedAt.*`、`menu.dock.*`）

- Dock `程序坞`, never Latin. Accept `好，添加到程序坞` (Finder's label). Title `要把 Cmdr 留在程序坞里吗？`, a spoken
  question built on Dock's `在程序坞中保留`.
- Pin/unpin `固定` / `取消固定`: the Dock's own menu items don't pair, and the English needs a visible pair.
- `下次登录 Mac 时`: `Mac` separates it from signing in to a server.
- The Dock didn't reload: `图标已经放好了…只是…还没刷新`, ⚠️ never "not pinned" (the icon is there).
- `这次没能添加到程序坞` (this time) vs `无法把自己加进去` (a managed Dock, a lasting limit).
- The Dock menu: `搜索文件…` equals `menu.edit.searchFiles`; `前往文件夹…` (Finder) is not `前往路径…` (another
  command); `{name}（{parent}）`, full-width, no space.

## “在 Finder 中显示”的邀请与首次接手提示（`main.revealNudge.*`、`main.revealActivation.*`、`settings.behavior.reveal*`）

- `“在 Finder 中显示”` quoted, as the settings card names it.
- The first-takeover notice explains what happened and where the switch is; ❌ no apology.
- A noun click is `点按` too (`每一次点按“在 Finder 中显示”`), never `…的点击`.

## 入门引导改版：清单、步骤提示、可选项摘要 (`onboarding.stepBeta.checklist.*`, `onboarding.stepOptional.*`, `onboarding.wizard.stepTooltip`, `onboarding.stepFda.*`, `onboarding.stepAi.*`)

- GitHub star `加星标` / `星标` (GitHub's zh docs); repository `仓库`, not Microsoft's `存储库`. AlternativeTo like
  `点赞` (the site has no Chinese UI).
- Mailing list `邮件列表`, ⚠️ not the TBX's first hit `邮寄列表` (physical mail).
- Typo hint `看看是不是打错了`, never `错误`.
- Local Network is quoted as macOS names it, `“本地网络”`, with `访问` outside the quotes.
- Learn more about X `进一步了解“{topic}”` (Finder); the topic is runtime-substituted, so quoted.
- Step dots `第 {step} 步，共 {mandatory}+1 步`: the literal `+1` signals the optional last step.
- Native handler `macOS 内建的处理程序`. "dumber" `笨不少`: keep the English's deliberate bluntness.
- Released copy `正式发布的 Cmdr 版本` / `开发版和测试版`, not `构建`.
- Fetch for preview `正在获取“{fileName}”以便预览` over `抓取` (web crawling) and Finder's `下载` (a phone isn't a
  download). Byte progress `{doneText} / {totalText}`: the units carry spaces, so the slash does too.

## ADB 手机的索引“可能已过期”（`fileExplorer.navigation.driveIndex.tooltipStalePhone`、`indexing.staleDialog.titlePhone` / `.bodyPhone`）

- ❌ No disconnect: the phone stays plugged in and simply never reports changes.
- `Cmdr 自己做的更改会及时更新到这个索引里`, never `让索引保持最新` (the dot says it may NOT be current).
- `要重新扫描后才会显示`: `才` carries "not before then".

## 服务器表单的根文件夹与起始文件夹（`servers.sheet.rootFolder*`、`.startFolder*`、`.nameHelp`、`servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`）

- Root folder `根文件夹`, never `顶层文件夹` (the volume's own top, another English word).
- Start folder `起始文件夹` (Safari `起始页`), never Double Commander's `启动文件夹` (Windows' autostart folder).
- `不会越过这个文件夹往上走`, not `上层文件夹` (a Finder command name); tentative.
- `留空则…` for a field that may be blank.

## 共享装载不上、共享列表加载不出来时的那句话（`errors.mount.*`、`errors.shareList.*`）

`这台电脑`, never `Mac` (these show on Linux too). Distribution (Linux) `发行版` is tentative (the pile has no Linux
sense). Quotes hug Han (`“{server}”上`).

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- A text editor `文本编辑器` (the class of app), never Apple's `“文本编辑”` app. Default `默认文本编辑器`.
- `编辑文件时使用` + the app, the shape of the terminal picker's label (tentative).
- System default `跟随系统`, the app name after it in full-width parens.

## 驱动器中途离开与无法确认的移动（`fileExplorer.navigation.driveIndex.driveLeaving`、`indexing.needsFreshScan.afterDisconnect`、`errors.write.deviceDisconnected.sided.destination.copy`、`errors.write.moveNotConfirmed.title`、`fileOperations.leftovers.stagingFolderKept`）

- In progress `正在断开连接`; already happened `断开了连接`.
- `{done} 个文件（共 {total} 个）` (Finder's `^0项（共^1项）`).
- Which drive left is carried by the preposition (`往它拷贝` vs `往 {counterpart} 拷贝`), not by the label words `来源`
  / `目标位置`, which go stiff in a sentence.
- Reassurance keeps `都` (`其余的都还在…`, `你的文件都还在…`, `都留在了原处`): without it the line reads as an aside,
  not "nothing is missing". `所以什么都没丢失` is the point of its message.
- ❗ `无法确认这次移动`, never "move failed": the files may have arrived, which is why the originals stay.
- The staging folder holds the USER's files: found `发现` (not a search), unfinished `没完成的`, ❌ never `不完整` or a
  delete. `…原处，也就是在…里` keeps the apposition from reading as Cmdr moving them in.

## 个人收藏菜单（`commands.favoritesOpen.label`/`.description`、`commands.favoritesOpenByNumber.label`、`commands.favoritesAdd.description`、`fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`、`menu.go.showFavorites`、`shortcuts.scope.favoritesMenu`）

- The list has one name, `个人收藏`, singular or plural; never `收藏夹` / `书签`. The menu `个人收藏菜单`: never
  `…选择器` (the volume chooser) or `…面板` (the palette).
- ❗ Show `显示个人收藏` (`menu.go.showFavorites`, `commands.favoritesOpen.label`, like `显示服务器`) vs See
  `查看 {count} 项个人收藏` (`fileExplorer.navigation.seeFavorites`): the English uses two verbs. Don't unify.
- `seeFavorites` has `=0` (no count, `查看个人收藏`) and `other` only.
- Classifier `项`: `{count} 个个人收藏` doubles the character.
- `前往` needs an object in Chinese, so `按数字键前往对应的文件夹`, never a bare `前往。`
- Mounted share `已装载的共享` (Finder), not the error family's `挂载`; name no protocol, like the English.
- `favoritesCantAddHere` pairs with `favoritesAlreadyAdded` (`这个文件夹…`), the reason after `：`, no final `。`
  (tooltips).

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- Kind `种类` (Finder's Kind criterion). `选择所有扩展名为 *.{extension} 的文件` (the Commanders' "same extension", with
  the concrete mask spaced).
- `menu.context.selection` is a noun, `选择`, the same as the menu: Chinese doesn't split the noun and verb.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`)

- `没有完全磁盘访问权限`, `没有` over `无`: it must equal `onboarding.stepAi.bannerTitle.denied`, and the aria opens
  with it.
- "files macOS keeps to itself" `macOS 自己留着的文件`: plain words, name no macOS feature.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

- Every message opens `你选中的项目里有 {count} 个，…`, which reads right at 1 and at 7 (no plural machinery here).
- The badge text is quoted from `onboarding.fdaBadge.label` verbatim; `suggestion.other` names `技术详情`.

## 仅存云端内容的删除警告条（`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`）

- Prose says the everyday `只在云端` (the ruling's `proseAccept`); the command and names keep Finder's `仅存在云端`.
- Four facts stay: the trash downloads first; so only Delete is offered; no copy in the trash, but the cloud keeps its
  own (❌ never "it's still in the trash"); the way out (`设为离线可用`). The all-online variant drops "deselect", which
  would leave nothing.
- The quoted `“删除”` is `fileOperations.delete.confirmDelete`'s label; both `<strong>` stay.

## 服务器明确说没有这个共享（`fileExplorer.network.osMountFallback.shareNotOnServer`、`fileExplorer.pane.directConnectionShareNotOnServerToast`）

The one case where retrying can't help: no `暂时`, no `再试一次`. `这种情况等下去也不会好` over `不会自动恢复` (reads
like a system fault). "a lot slower" `慢得多` (no ratio here).

## 服务器上「看起来一样」的名称（`fileOperations.transferProgress.lookAlikeHint`、`errors.listing.ambiguousName.explanation`、`errors.volume.ambiguousName`）

- `看起来一样，但服务器存储的写法不同`, ❌ never `编码` / `规范化` / `Unicode` (the English avoids tech words);
  tentative.
- The Overwrite button in prose `“覆盖”`; the existing item `现有的那一项`. Error prose keeps the spoken `改名` /
  `上一级文件夹`, not the command names.

## “允许使用云端 AI”开关，以及云端 AI 关闭时的提示（`ai.cloudConsent.*`、`askCmdr.gate.*`）

- `允许使用云端 AI`: `允许` is macOS's permission button, and `使用` supplies the verb `允许云端 AI` lacks.
- Quote the switch verbatim wherever it's referenced; "Allow it" alone is `允许使用`.
- AI service `AI 服务` stays distinct from provider `提供方`, like the English.

## “打开方式”和“共享”子菜单里的等待行（`menu.context.openWithLoading`、`.shareLoading`、`.shareNone`）

`正在查找 App…`, `没有共享选项` (macOS's empty-menu shape, `没有服务可应用`).

## Apple app and product names Apple localizes (`settings.advanced.showSafeSaveFiles.description`, `errors.provider.iCloud.*`, `errors.listing.emptyRootICloud.*`, `shortcuts.system.finderSearch`)

`iCloud 云盘` (macOS zh-CN, spaced); third-party "Drive" products keep their Latin names. `“文本编辑”` / `“预览”` quoted
in prose.

## Whole-catalog wording rules (`viewer.saveAs.*`, `viewer.copyDialog.*`, `settings.onboarding.completed.label`, `onboarding.wizard.restart`, `onboarding.stepFda.postAction.body`, `queryUi.results.live.foldersScanned`)

- `存储` survives only for storing data on a drive (`无法存储大于…的文件`); saving a file is `保存`.
- Restart buttons `重新启动` (Apple), and a sentence quoting one matches it; running prose may say `重启`.
- A search's scanned-folder count `已扫描`, the scan verb, not `已查看`.
