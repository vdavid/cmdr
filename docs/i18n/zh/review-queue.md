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
- **the one-time offer → `邀请`** (`settings.behavior.dockPinNudgeOfferedAt.label`,
  `settings.behavior.revealNudgeOfferedAt.label`): internal labels, never on screen; descriptive.
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
- **glob → `Glob`** (Latin): no settled Chinese term.
- **refine (AI search) → `优化`** (`queryUi.ai.refine`, `queryUi.ai.refineAria`): rendered by meaning.
- **replay (recorded file-system changes) → `重放`** (`indexing.rescan.replayOverflow`): rendered by meaning.
- **report bundle → `报告包`** (`errorReporter.dialog.saveToDisk`, `errorReporter.bundleSavedToast.message`):
  descriptive.
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
  `medium`-confidence drafts that never had a reading pass; also check they don't overflow the narrow strip.
