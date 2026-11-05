# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- `fileExplorer.open.didNotOpen`: "macOS couldn’t find the file, or an app that opens it" reads as either one missing,
  but a negated "X or Y" in most languages reads as neither; locales wrote "didn’t find X or Y" and avoided "neither".
  Consider "couldn’t find the file, or no app opens it". (all)
- `mtp.readiness.connecting`: "Unlock your phone" matches the `unlock` concept (password-protected archives), while
  `unlock-device` stays out of the batch. Add it to `unlock`'s `notMatch`. `mtp-browse.png` doesn't show the row. (all)
- `errors.eject.unmountRefusedByApp`, `…ByApps`: nothing in the descriptions says "it" / "them" now means the app, and
  `{app}` may be a command-line tool, so "close it" is loose for those. Locales repeat the noun "app" (no pronoun for an
  insert). (all)

- `fileExplorer.navigation.forgetConfirmButton`: one button serves the server, share, and saved-password alerts; locales
  that clear a password with another verb (zh `清除`) get a button that doesn't match the title. Consider a separate
  key.
- Cross-language rule proposal: when English prose names a button without quotes (`servers.sheet.addAnywayHelp`,
  `servers.pinHint.body`), a locale may quote it; or quote it in English too.
- `fileExplorer.archivedFile.label`, `askCmdr.sessions.archivedBadge`: both are "Archived" in English, so every locale
  needs a term-consistency allowlist entry. Consider "In cold storage" for the file glyph.
- `queryUi.date.preset.thisMonth`, `.lastMonth`: `{month}` arrives nominative, so Slavic locales can't say "from the 1st
  of October" (`с 1 октября`). Pass a preformatted day-and-month. (ru, uk)
- `servers.paneState.cancelCycleTooltip`: "Switch back to retry" doesn't say switch back to what. (ru, uk)
- Translation briefs: an `es` value that uses `Ajustes` (or another Spain-only form) needs an `es-419` fork, or
  `i18n-es-overlay.test.ts` fails; the brief's "Regional overlays" line says only "forks none yet". Say so there. (es)
- `fileExplorer.navigation.favoriteStatus.connects`, `favoriteUnreachable.forgotten`: `{place}` is a share, a server, or
  a phone, and nothing says which, so case languages can't put a type noun before it (`к серверу «…»`) and fall back to
  a parenthesis or colon. Pass a `{kind}` select (share / server / phone) alongside it. (ru)
- Tooling: the brief's reference-pile path (`<main clone>/_ignored/i18n/<tag>/`) doesn't exist on the M1 agent box, so
  "no ruling" terms there can't be mined as the brief requires. Sync the pile to the box, or have the brief say so and
  fall back to `terms.json` sources; vi mined the live Finder bundle instead (§ No pile on this machine). (ru, vi, sv)
- `transfer.metadataLeftBehind`: says "This server" though the description lists phones and cloud buckets too; "those"
  points back at the `{classes}` insert. Name the destination generically and use a noun ("that information").
- `transfer.metadataClass.*` joined by `formatDisjunctionList`: "or" inside a negation reads stiff in fr (wants `ni`),
  hu, and nl; a conjunction ("and") or a colon-list frame would read naturally everywhere. (fr, hu, nl)
- The reference pile has no git vocabulary (detached, unborn, ahead/behind, uncommitted changes): no tier names them.
  git's own `po/<lang>.po` (github.com/git/git, `po/`) is the authoritative translation for ~20 languages; propose
  adding it to the pile as a git-domain source. sv mined it live for `git-detached`, `git-unborn`, `git-dirty`. (sv, ru,
  es, de, uk)
- `concepts.json` `archive-chat` matches `fileExplorer.list.rowKind.archive` ("archive", a zip file), so the brief lists
  the chat-archiving verb for it. Add that key to `archive-chat`'s `notMatch` and let the `archive` concept match it; hu
  carries a per-locale `exceptions` entry meanwhile. (hu, nl, vi, pt, sv, es, ru, zh-Hant, de, uk)
- `decisions.md` is at its byte budget for es and zh, so the #457 rulings (zh genuine `真正的`, guest session
  `客人会话`) went to `review-queue.md` instead. Bump `decisionsBytes` in `i18n-termbase-baseline.json` if they belong
  there. (es, zh)
- `errors.write.sourcesSpanVolumes.title` / `.suggestion`: "a view" names a read-only virtual folder (backup snapshots,
  archive inside, git history), but the word is the View menu / F3 Viewer elsewhere and no UI surface calls these
  "views". A literal rendering (hu `nézet`) reads as the display mode. Consider wording the users see, like "inside a
  backup, archive, or repository history”, or add a `virtual-view` concept with that sense. (hu, de, zh, ru: used
  `виртуальная папка`, the catalog's word for the git portal's folders; sv used `vy`, same risk; vi used
  `khu vực chỉ để xem`, "a view-only area", since `chế độ xem` is the display mode; pt used `visualização`, same risk;
  fr used `vue`, same risk; zh-Hant used `虛擬資料夾`, since `檢視` is the verb and `顯示方式` the view mode; nl used
  `weergave`, same risk; es used `vista`, same risk, since it's also `la vista breve` / `la vista completa`; uk used
  `віртуальна папка`, like ru)
- `fileOperations.transferProgress.conflictPolicyCaption`: `{policy}` reuses the copy dialog's BUTTON labels, which most
  locales write as imperatives (nl `Overschrijf indien ouder`, `Vraag voor elk`), so the caption reads “If a file
  already exists: Overwrite if older!” as a command. Consider dedicated noun-phrase or state labels for the caption
  (“Skip”, “Overwrite if older”) as `*PolicyName` keys. (all with an imperative/infinitive split: nl, de, hu, sv, …)
- `askCmdr.renameUndo.skipReason.*.counted` (drift, nameTaken, unverifiable, failed): still say "file(s)" while
  `vanished.counted` now says "item(s)", though a rename run can include folders. Move the family to "item". (de, vi)
- `multiRename.swapsSkipped`, `askCmdr.renameUndo.swapsSkipped`: "Rename them one at a time" can point at the renames;
  on S3 the swapped entries can be folders. Say "Rename those items one at a time" in both. Their count frames also
  differ (`#` inside the branches vs `{countText}` first). (de, fr, hu, ru, sv)
- `stateRecovery.operationLog.title`: "so it starts fresh" can mean Cmdr or the log; say "so the log starts fresh", like
  the "the list starts empty" siblings. (fr, nl, pt, zh)
- `fileExplorer.navigation.favoriteUnreachable.storageUnplugged`: `{device}` is bare while `.phoneUnplugged` now quotes
  it; and the description says phone, though MTP also reaches cameras. Say which, and quote both. (hu, zh-Hant)
- `menu.volume.editFavoriteShortcut` is now "Set shortcut key…", but `fileExplorer.navigation.favoriteShortcutAriaLabel`
  and `.favoriteShortcutSaveFailed` still say "shortcut", so ru has two names for one control. Align the family; the
  menu item's description should also name the menu and say VERB. (ru, nl)
- `reply` / `ai-reply` concepts match a server's reply (`securityNotices.smbSignatureRefused.body`,
  `errors.volume.signatureInvalid`): the existing `server's reply` `notMatch` has a straight apostrophe, the English a
  curly one. Add the curly form and "the reply didn’t carry"; consider merging the two concepts. (nl, pt, sv, vi)
- `errors.volume.signatureInvalid`, `errors.write.replySignatureRefused.message`: "so Cmdr stopped" has no object, and
  verbs like sv `stoppade` need one; say what stopped in the description, or write "stopped there". (sv)
- `onboarding.stepAi.table.renameWithout`: "then review and apply it" leaves "it" to the reader; name the pattern. (pt)
- `uninstall` concept: note it's removing software, not unmounting (zh-CN macOS uses `卸载` for both). (zh)
- `ai.cloud.timedOut` calls the AI provider "the server", while `ai.cloud.providerRefused` on the same row and both
  onboarding twins say "the service". Pick one noun; every locale mirrored the split. (all)
- `fileOperations.transferProgress.undo`: “Undo” here reverses a whole finished operation (with confirmation), a fourth
  sense beside text undo, Put Back, and an AI rename run. Say so in the `undo` concept’s sense so locales with split
  words (nl `Herstel` / `Zet terug` / `Ongedaan maken`) pick deliberately. (all)
- `suggestedOps.facts.archive` (noun, the zip) and `askCmdr.sessions.archive` (verb) share the English "Archive", so
  every locale needs an `archive-chat` exception and an allowlist split. Rename one English label, or accept it. (all)
- No `include` / `tick` concept: every locale ruled the file-list tick boxes (`columnInclude`, `includeAll`,
  `includeFile`) ad hoc; proposed: an `include-in-approval` concept that never reuses the file-selection verb. (nl, ru,
  sv)
- `suggestedOps.factUnknown` stands for a size or a date, so gendered locales needed a neutral form or a split; consider
  two keys. Also `suggestedOps.columnModified` ("Last modified") doubles as a tooltip phrase, long as a header. (pt, ru,
  sv)
- `suggestedOps.reversible.moveBack` / `renameBack` use `{count}` only to pick a pronoun; languages without number (vi,
  zh) need a workaround noun. Propose a rule allowing such a `{count}` to go unused, or a number-free English. (vi)
- `approve` ruling (nl) said suggested-ops buttons use the infinitive, against the button-imperative rule; check other
  locales' rulings for the same conflict. (nl)
- `suggestedOps.approvalNoticeDismiss` and the shipped "Don't show again" keys mean the same; align the English. (hu)
- Proposed: `sync-locale-keys.ts` drops termbase exceptions that name deleted keys (four locales failed on the retired
  "undo by deleting what it writes" pill key). (fr)
- The AI suggestions dialog keys ship without screenshots; the pills and fact lines need them most. (fr, hu)
- `suggestedOps.action.*`: the label renders in its own span before the AI's title, so verb-final languages can't
  reorder (`Bewegen Alte Installer`). Add a `{action} {title}` frame key, or call it a chip label. (de, hu, nl)
- `suggestedOps.newNameTaken` ("Name already taken") and `multiRename.status.targetExists` ("Name is taken") say one
  thing in two Englishes; align them so `i18n-terms` keeps locales in step. (all)
- `settings.ai.reasoningEfforts.description` (“More effort helps…”) is unchanged, so zh / zh-Hant still say `强度越高` /
  `強度越高`, a word none of the three new labels uses; consider wording the English around thinking time so no locale
  echoes a label it no longer shows. (zh, zh-Hant)
- `askCmdr.limit.repeatedCall.answered`, `askCmdr.limit.noProgress.answered`: "Cmdr stopped it looking" reads as a typo
  for "stopped it from looking"; consider "Cmdr had it stop looking and answer with what it found". (all)
- `settings.askCmdr.spendPause.label`: "Check in" (ask the user) collides with the `check` (verify) concept; patched
  with a `notMatch` "check in when", which a future "check in" string won't share. (all)
- `settings.askCmdr.spendPause.opt.never` vs `servers.hub.neverUsed`: "Never" as a choice vs never happened needed
  allowlist entries in vi, zh, zh-Hant; a distinct English ("Never used") would avoid it. (vi, zh, zh-Hant)

## Ukrainian first pass (#767)

All (uk) unless noted.

- A counted noun with no plural param forces a counted-label colon in Slavic locales:
  `errors.write.trashRefused.message.*`, `errors.write.deviceDisconnected.sided.*`,
  `errors.eject.unmountRefusedByProcesses`, `fileOperations.validation.*TooLong` (`maxBytes`),
  `licensing.dialog.pendingHint`, `licensing.dialog.retryExhausted`, `multiRename.presets.listFull`,
  `settings.control.unitItems`. `indexing.enrich.progress` drives the plural by `{done}` where the noun follows
  `{total}`. Pass plurals, or make the colon form the documented rule (native menus like `menu.edit.undoTrashMany` can't
  pluralize).
- Concept `notMatch` proposals: `global` ← "in-app" (`downloads.toast.inAppHint`, `*Notifications.opt.inApp`); `storage`
  ← "log storage"; `attachment` ← "attach my email" (`common.attachEmail*`,
  `settings.updates.attachEmailToReports.label`); `zoom` ← `menu.window.zoom`; `app` ← ".app"; `browse` ← "Browse…"
  (file picker); `just-now` ← "just now" meaning at the moment; `move` ← "↑↓ to move"; `context-menu` ← a bare
  "Right-click … choose"; `destination` ← a symlink's "target"; `complete` ← "may be incomplete"; `failed` ← "if it
  fails".
- Concept `permission` means file-system rights, but FDA and privacy copy uses it for a macOS consent
  (`onboarding.stepFda.*`, `onboarding.stepAi.bannerBody.denied`, `errors.listing.notPermitted.explanation`). Split a
  `privacy-permission` concept. Also `ai-lookup` vs search: say the two must stay distinct (`askCmdr.limit.*`).
- Seeding should cross-check sibling concepts: uk's `select`, `deselect`, and `invert-selection` rulings disagreed.
- Quote real labels in English: `ai.cloudConsent.logsNote` ("Log AI model calls"), `settings.askCmdr.cloudOffHint`,
  `errors.listing.couldntReadUnknown.explanation` ("Technical details"),
  `fileExplorer.navigation.driveIndex.refusedIndexingOff`, `menuIndexingOffNote`,
  `fileExplorer.quickLookHint.configurable` (Settings paths; or pass them as placeholders).
  `fileOperations.transferDialog.smbNativeNote` quotes "Connect directly", which no item says.
- Descriptions name the wrong thing: `viewer.markdown.remoteImages.notice` (the button is "Load images from the web"),
  `updates.toast.later` (update-ready toast), `viewer.search.searching` ("three dots" vs …),
  `fileOperations.transferProgress.smallerDisabledTooltip` and `errors.listing.deletePending.suggestion` (dash type),
  `settings.mediaIndex.networkVolumes.intro` / `importanceThreshold.label` (stale Settings path),
  `onboarding.stepAi.table.searchWithout` (no such date label), `search.walkHandoff.counts` / `finished` (params not in
  English), `errors.provider.cmVolumes.displayName` / `genericCloudStorage.displayName` (never rendered; dead keys?).
- English wording: `commands.navUp.label` / `navDown` "Select …" only moves the cursor ("Move cursor to …");
  `multiRename.reason.*` say "file" where folders count ("item"); `errors.write.readOnlyDevice.destination.fallbackName`
  "The target device" → "The destination"; `errors.volume.connectionTimeout` personifies the connection;
  `ai.cloudConsent.askCmdr.title` "it" → "Cmdr"; `ai.translateError.emptyResponse.body` "bigger budget" is vague;
  `onboarding.stepFda.revoked.intro` "accepted" → "granted"; `fileExplorer.navigation.disconnectBusyTooltip` /
  `ejectBusyTooltip` read as imperatives; `errorReporter.sentToast.message` / `amendedToast.message` end in a colon
  before the badge; `settings.mediaIndex.reclaim.confirmTitle` lacks "?";
  `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged.label` is jargon.
- English consistency: menu vs command ellipses (`menu.app.acknowledgements`, `menu.app.licenseEnter`,
  `menu.help.sendFeedback`); command descriptions mix imperative and third person; "Full/Brief view" vs "mode"
  (`settings.listing.showExtensionInName.description`, `settings.listing.stripedRows.description`,
  `settings.fileExplorer.git.showStatusColumn.description`); hyphen ranges in `settings.appearance.formatHelp*`;
  `licensing.about.srTitle` = `menu.app.about` must match or diverge on purpose; `ui.select.placeholder` "Select…"
  collides with the Select menu; `settings.fileSystemWatching.globalShortcutHint` should name one canonical command key.
- `settings.mediaIndex.networkVolumes.alwaysAria` doesn't contain its visible label (WCAG 2.5.3) in English either.
- `settings.fileOperations.showAlreadyGoneToast.description`: calls the toast a "note" ("a short note", "in the note",
  "skip the note"), which collides with the `note` concept (text the user writes) and the `skip` concept (leaving an
  item out of an operation). Every locale needed exceptions. Say "notice" in the English, or add `notMatch` for these
  phrases. (de, fr, hu, nl, ru, sv, uk, vi, zh, zh-Hant)
- `settings.fileOperations.showAlreadyGoneToast.label`: "Say when a deleted item was already gone" is hard to phrase as
  a toggle; "Show a notice when a deleted item was already gone" translates more cleanly. (uk, zh)
- `fileExplorer.typeToJump.clear`: "Clear jump" treats a mode name as something clearable, which forces a rewrite in
  languages where the mode word is a verb (de: `Eingabe zum Springen löschen`). "Clear typed text" would translate
  cleanly; `fileExplorer.quickFilter.clear` could follow. (de)
- `type-to-jump` concept: the brief didn't list it for `commands.paneQuickFilter.description` or the `modeJump` keys:
  its `match` (`type-to-jump`, `switch to jump`, `=jump`) misses "between Filter and Jump" and "Jump: …". Proposal: add
  those phrasings. (de)
- `errors.volume.linkTargetUnreachable`: "This sits behind a link" has no clear referent, and for a new folder or file
  the thing doesn't exist yet. "This location sits behind a link" translates more cleanly. (fr)
- `link` concept (`docs/i18n/concepts.json`): its sense covers hyperlinks and shareable addresses only, while
  `*.linkTargetUnreachable*` match it for a DFS link. Widen the sense, or add a `dfs-link` concept. (fr)
- `fileExplorer.typeToJump.clear`: "Clear jump" uses "jump" as a noun that few languages have for this mode; "Clear
  typed text", or a description naming the mode's label as the word to quote, would help. (fr)
- `commands.paneQuickFilter.description`: "Opens the filter badge on the current pattern" leaves "on the current
  pattern" unexplained (seeded with it? showing it?), and the badge can open in Jump mode too, so "filter badge" isn't
  always right. (es)
- `fileExplorer.typeToJump.clear`: "Clear jump" names no object; the button removes the typed text, so "Clear typed
  text" would translate more safely. (es)
- `servers.sheet.nfsOtherGidsTooMany`: "this lists {count}" has no noun for "this", which every language has to invent
  ("the list", "you listed"). (es)
- Concept `link`: its sense covers only hyperlinks, yet it matches the DFS link in the `*.linkTargetUnreachable` keys;
  add a DFS sense or a `notMatch`, since a locale could want a different word for a network link. (es)
- `*.linkTargetUnreachable*`: "sits behind a link" is a spatial metaphor with no natural Portuguese, and `{path}` can't
  take an agreeing verb. "Reaching this goes through a link to another server" would translate more plainly in every
  family member. (pt)
- `servers.sheet.nfsOtherGidsTooMany`: "carries" reads as a metaphor for a protocol limit; "sends" (the `@key`'s own
  word) is clearer. (pt)
- `errors.volume.linkTargetUnreachable`, `commands.paneQuickFilter.description`, `fileExplorer.typeToJump.clear`: same
  problems as the fr and es entries above; hu wrote `Ez a hely`, `szűrőjelvény` (also wrong when the badge opens in Jump
  mode), and `Ugrás törlése` as best readings. (hu)
- `askCmdr.renameReview.checkUnfinished`: "hasn't finished checking 3 files" can read as 3 files in progress; the `@key`
  says they're still unchecked. "3 files are still unchecked" would translate more safely. (hu)
- Concept `badge`: its sense is a marker on a file icon or title bar, while `commands.paneQuickFilter.description` means
  the small box holding typed text, so the ruled word fits it poorly. Widen the sense, or add a typing-badge concept.
  (nl)
- `*.linkTargetUnreachable*`, `fileExplorer.typeToJump.clear`, `commands.paneQuickFilter.description`,
  `servers.sheet.nfsOtherGidsTooMany`, `askCmdr.renameReview.checkUnfinished`: same problems as the fr, es, pt, and hu
  entries above; uk's best readings are `Шлях … веде через посилання`, `Очистити «Перехід»`, `значок фільтра`,
  `а в цьому списку їх {count}`, and `ще не перевірив # файл`. (uk)
- `askCmdr.renameReview.needsAnotherLook`: "Take another look" doesn't say at what; the `@key` implies the names, so uk
  wrote `Перегляньте їх знову`. Naming it ("Check the names and try again") would translate more safely. (uk)
- `errors.volume.linkTargetUnreachable`, `commands.paneQuickFilter.description`, `fileExplorer.typeToJump.clear`: same
  problems as the fr and es entries above; sv wrote `Den här platsen`, `filtermärket` (also wrong when the badge opens
  in Jump mode), and `Rensa Hoppa-texten` as best readings. `nfsOtherGidsTooMany`: "carries" became `kan skicka`, "this"
  became `den här listan`. (sv)
- `commands.paneQuickFilter.description`, `fileExplorer.typeToJump.clear`, `errors.volume.linkTargetUnreachable`,
  `*.linkTargetUnreachable*`, concepts `badge` and `link`: same problems as the fr, es, nl, and pt entries above; ru
  wrote `значок фильтра с текущим шаблоном`, `Очистить поле «Переход»`, `К этому месту ведет ссылка`, as best readings.
  (ru)
- `servers.sheet.nfsOtherGidsTooMany`: `{count}` is a bare number with no plural param, so a noun can't agree with it;
  ru leans on `в этом списке их {count}`. An integer plural would let every language name the groups. (ru)
- `*.linkTargetUnreachable*`, `fileExplorer.typeToJump.clear`, `commands.paneQuickFilter.description`,
  `commands.paneQuickFilter.label`, `servers.sheet.nfsOtherGidsTooMany`, concept `badge`, concept `link`: same problems
  as the fr, es, pt, hu, and nl entries above; vi's best readings are `Vị trí này nằm sau một liên kết`,
  `Xóa nội dung “Nhảy tới”`, `huy hiệu bộ lọc`, `Bộ lọc nhanh` against the prefix `Lọc`, and
  `còn danh sách này có {count} nhóm`. (vi)
- Concept `type-to-jump`: a locale without a ruling for it isn't told so, because its `match` misses every phrasing in
  these keys (see the de entry above); vi had none until this batch. (vi)
- `commands.paneQuickFilter.description`: "current pattern" is the text typed so far (`quick-filter-session.svelte.ts`),
  and neither the English nor the `@key` says so. In zh the ruled `模式` also means "mode", so beside Filter and Jump it
  reads as the typing mode. "Opens the badge with what you've typed so far" would translate safely. zh wrote
  `当前输入的内容`. (zh)
- `*.linkTargetUnreachable*`, `fileExplorer.typeToJump.clear`, `servers.sheet.nfsOtherGidsTooMany`, concepts `badge` and
  `link`: same problems as the fr, es, and pt entries above; zh's best readings are
  `访问这里要通过一个指向另一台服务器的链接`, `清除跳转`, `这里列出了 {count} 个`, and `筛选标记`. (zh)
- Concept `group`: one ruling covers both a group of rows and a Unix file group, and zh needs different words (`分组`,
  `群组`), so each NFS group key takes its own exception. Split out a `unix-group` concept. (zh)
- `*.linkTargetUnreachable*`, `commands.paneQuickFilter.description`, `fileExplorer.typeToJump.clear`,
  `servers.sheet.nfsOtherGidsTooMany`, concepts `badge` and `link`: same problems as the fr, es, nl, and pt entries
  above; zh-Hant wrote `要透過連結連到另一個伺服器`, `開啟篩選標記，沿用目前的樣式` (also wrong when the badge opens in
  Jump mode), `清除「跳至」的輸入內容`, and `這裡列出了 {count} 個` as best readings. (zh-Hant)
- Concept `credentials`: the sense says "a username and password pair", too narrow for
  `ai.cloudConsent.askCmdr.protectedContents`'s "cloud and tool credentials" (tokens, key files). Widen it. (all)
- `pnpm i18n:brief --lang pt` points at the European `_ignored/i18n/pt/` pile, while `docs/i18n/pt/style.md` requires
  `pt-BR`. Point it at `_ignored/i18n/pt-BR/`. (pt)
- Concept `show`: "Cmdr shows how to unpin some" (`settings.behavior.serversPinHintSeen.description`) is the explain
  sense, yet it matched Show; consider `notMatch` "shows how". (hu, vi, zh, zh-Hant)
- Concept `upgrade` (a Cmdr license) matched "upgrade your plan" in `askCmdr.error.modelNotInPlan.*`, which upgrades a
  plan with the AI provider; I added `notMatch` "upgrade your plan" and the new concept `ai-plan`. A future "upgrade"
  about a provider plan needs the same exclusion, so consider widening `ai-plan` to own it. (all)
- `fileExplorer.pane.directConnectionNegotiationInvalidToast`, `…EncryptionUnavailableToast`: "so Cmdr stays on the
  system connection", while every sibling toast says the share stays (`…ShareNotOnServerToast`, `…UnreachableToast`).
  Pick one subject for the family; locales followed the English (Cmdr) this time. (all)
- The six SMB negotiation and encryption keys list "security", "agreed", "misbehaving", "cause", "someone", "stays",
  "directly" under "No concept yet": ordinary words with nothing to mine. Consider a stop list for the brief, and one
  concept for "the security settings both sides agreed on" (SMB negotiation), now in four keys. (all)
- Concept `encrypted`: the sense says "an archive or entry", but the `requireSmbEncryption` family uses it for SMB
  connection encryption, including the verb ("Cmdr encrypts"); widen the sense. uk added the stem `шифру`. (all)
