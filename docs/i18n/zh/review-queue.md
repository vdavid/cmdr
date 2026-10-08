# zh review queue

Open questions for a future native Simplified Chinese reviewer. Not translator input: every item below already ships a
reasoned value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the
settled wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **compromised → `已泄露`** (`servers.refusal.hostKeyRevoked`): no Apple term; `已泄露` is the common security word and
  plainer than `已失陷`. Confirm it reads right for a revoked SSH host key.
- **Finish rolling back → `完成回滚`** (`operationLog.dialog.finishRollBack`,
  `fileOperations.rollbackConfirm.finishRollBack`): `继续回滚` says "not a fresh start" more plainly, but the English
  says Finish, and `继续` belongs to Resume. The two keys must stay identical.
- **Places → `共享位置`** (`shortcuts.scope.places`): today the list is SMB shares; once object-storage buckets land, a
  more neutral word may fit better. Bare `位置` is reserved for file-system locations, `地点` for photos.
- **"has stopped moving" → `停住不动了`** (`fileOperations.transferProgress.stallUnknown`,
  `viewer.error.stoppedResponding`): descriptive; chosen to stay clear of `已暂停` (paused) and the more alarming
  `卡住`.
- **Watching for phones → `正在留意接入的手机`** (`settings.adb.status.watching`, `settings.adb.status.notWatching`):
  Apple's nearest wording is `正在等待…`, which reads as blocked waiting; `留意` is passive watching. Confirm it isn't
  odd.
- **"come and go" → `来来去去`** (`settings.revealHandler.notProductionBuild`): a free rendering of dev builds appearing
  and disappearing.
- **"never goes above this folder" → `不会越过这个文件夹往上走`** (`servers.sheet.rootFolderHelp`): plain wording for
  the ceiling; `上层文件夹` was avoided because it's Finder's Enclosing Folder command.
- **Edit files in [app] → `编辑文件时使用`** (`settings.behavior.textEditorApp.label`): the label runs into the app
  dropdown, like `settings.behavior.openTerminalHereApp.label`.
- **distribution (Linux) → `发行版`** (`errors.mount.gvfsMissing`): the pile has no Linux sense (Microsoft gives only
  `分配`).
- **look-alike names → `看起来一样，但服务器存储的写法不同`** (`fileOperations.transferProgress.lookAlikeHint`,
  `errors.listing.ambiguousName.explanation`): `写法` for "spelled"; the English forbids `编码` / `规范化` / Unicode.
- **badge → `标记`** (`settings.mediaIndex.showFileStatusIcons.label`, `onboarding.fdaBadge.label` context): no macOS
  noun for an icon-overlay badge; `角标` was the literal alternative.
- **"needs attention" → `需要先处理`** (`askCmdr.renameReview.blocked`): kept as vague as the English about what's
  wrong.
- **token → `token`** (Latin, counted `个 token`): no settled Chinese UI term; `词元` exists but is rare in consumer UI.
- **Accepting incoming connections → `接受传入连接`** (`onboarding.stepOptional.networking.desc`): describes the
  firewall prompt rather than quoting a label.
- **"boring folders" → `无聊的文件夹`** (`queryUi.scope.toggle.hideBoring`): kept playful on purpose; confirm it lands.
- **watcher → `监视`** (`settings.advanced.card.fileWatching`, `settings.advanced.fileWatcherDebounce.label`): standard,
  but unsourced in the pile.
- **a single chat mid-sentence → `对话`** (`askCmdr.context.label`, `askCmdr.sessions.wakeThread`,
  `settings.askCmdr.proactive.description`) next to `聊天` everywhere else (the list, New chat, "chat with"). Converge
  to `聊天` if a reviewer finds `这个聊天` natural.

## Wording

- **Right-click → `右键点击`**: the Tier-3 file managers agree; macOS zh-CN has no hit for either compound in the pile.
  If a reviewer confirms `右键点按` from a live Mac, change every occurrence together.
- **restart in running prose → `重启`**, next to Apple's `重新启动` on buttons and status lines: confirm the split reads
  as register, not inconsistency.
- **The cloud-only delete warnings** (`fileOperations.delete.cloudOnlineOnlyMixedWarning`,
  `fileOperations.delete.cloudOnlineOnlyAllWarning`, `fileOperations.delete.cloudOnlineOnlyHandedBack`): long,
  `medium`-confidence drafts (prose now says `只在云端`); check they read well and don't overflow the narrow strip.
- **The account suffix `身份：{username}` / `身份：客人`** (`servers.hub.shareAccount`, `.guestAccount`): a noun label
  after a server name ("Container 身份：sven"). Confirm it reads naturally in the quieter suffix, or pick a better pair.
- **Sign in as… → `用其他账户登录…`** (`fileExplorer.network.share.signInAs`): adds "other", since Chinese has no bare
  "as" label. Confirm it reads right when the list is signed in as an account (not just as a guest).
- **The shared Forget button `忘记`** (`fileExplorer.navigation.forgetConfirmButton`) sits under the saved-password
  alert titled `清除保存的密码`. Confirm the verb mismatch doesn't confuse.
- **Archived cold-storage files → `已存档`** (`fileExplorer.archivedFile.label`, `errors.listing.coldStorage.*`,
  `errors.write.sourceInColdStorage.*`, `viewer.error.coldStorage`): AWS, Alibaba, and Tencent call the tier `归档`; the
  catalog keeps `存档` (Microsoft's tier word) so "Archived" has one rendering. Confirm it reads right to an S3 user.
- **Google's console labels in `servers.sheet.s3GcsKeyHelp`** (`互操作性` tab, `Cloud Storage 设置`): written from
  memory of the Google Cloud zh-CN console, not verified live.
- **On-device AI → `本地 AI`** (`ai.managed.cloudAiOff`, `ai.managed.localOnlyUnsupported`,
  `settings.managed.summary.onDeviceOnly` = `只限本地`): reuses the local provider's word so the person recognizes it;
  Apple Intelligence copy says `设备端`. Confirm `本地` reads as "runs on this Mac" to an IT-managed user.
- **Multi-rename labels** (`multiRename.*`): `名称规则` for "Name mask" (TC's `重命名规则`, over DC's `掩码`),
  `移除变音符号` for "Remove diacritics" (no macOS zh-CN source; Microsoft's `音调符号` rejected), the `说明` status
  column, and the preset/status pair `无变化` (same English "No change"). Confirm each reads naturally in the sheet.
