# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- `servers.sheet.nfsExportsEmptyList`: one string covers "the server listed none" and "not asked yet"; split the key if
  the two should read differently. (hu, nl, ru, vi)
- The termbase matcher now skips paths and domains, but it still can't tell one sense of a word from another (verb
  "lists" vs the list, "share it yourself" vs a network share), so each new sense needs a `notMatch` patch. A
  sense-aware matcher needs a design. (hu, ru, sv, zh, zh-Hant)

- `fileExplorer.navigation.forgetConfirmButton`: one button serves the server, share, and saved-password alerts; locales
  that clear a password with another verb (zh `清除`) get a button that doesn't match the title. Consider a separate
  key.
- Cross-language rule proposal: when English prose names a button without quotes (`servers.sheet.addAnywayHelp`,
  `servers.pinHint.body`), a locale may quote it; or quote it in English too.
- `fileExplorer.archivedFile.label`, `askCmdr.sessions.archivedBadge`: both are "Archived" in English, so every locale
  needs a term-consistency allowlist entry. Consider "In cold storage" for the file glyph.
- `errors.eject.unmountRefusedByProcesses`: zh-Hant's list join can run Latin into Han with no space
  (`cfprefsd和其他程序`). (zh-Hant)
- `queryUi.date.preset.thisMonth`, `.lastMonth`: `{month}` arrives nominative, so Slavic locales can't say "from the 1st
  of October" (`с 1 октября`). Pass a preformatted day-and-month. (ru)
- `fileOperations.cancelRollback.reason.unverifiable.named`, `askCmdr.renameUndo.skipReason.unverifiable.named`:
  byte-identical English, but one is about items and the other about files; split the wording or say so. (ru)
- `servers.paneState.cancelCycleTooltip`: "Switch back to retry" doesn't say switch back to what. (ru)
- `fileExplorer.navigation.favoriteStatus.connects`, `favoriteUnreachable.forgotten`: `{place}` is a share, a server, or
  a phone, and nothing says which, so case languages can't put a type noun before it (`к серверу «…»`) and fall back to
  a parenthesis or colon. Pass a `{kind}` select (share / server / phone) alongside it. (ru)
- "Extension" is both `menu.sort.extension` and `multiRename.extensionMask`, so term-consistency forces one word; a
  field label might want the fuller form. Consider "Extension mask" or an allowlist entry. (sv)
- `multiRename.searchOptionHelp.on` / `.off` share "On"/"Off" with the settings switch options, so `term-consistency`
  forces the settings form, while in gendered languages the tooltip row agrees with "option" (pt wanted `Desativada`,
  shipped `Desativado` to pass). Add an `agreesWith`-style hook or an allowlisted reason so a locale can agree with the
  option. (pt, ru)
- Tooling: the brief's reference-pile path (`<main clone>/_ignored/i18n/<tag>/`) doesn't exist on the M1 agent box, so
  "no ruling" terms there can't be mined as the brief requires. Sync the pile to the box, or have the brief say so and
  fall back to `terms.json` sources; vi mined the live Finder bundle instead (§ No pile on this machine). (ru, vi, sv)
- `fileOperations.sourcesGone.*`: "this delete" / "A queued delete" use delete as a noun, and "since you queued it" has
  no `queued` concept. Locales rendered it as "while it waited its turn", matching the queue's Waiting status.
- `transfer.metadataLeftBehind`: says "This server" though the description lists phones and cloud buckets too; "those"
  points back at the `{classes}` insert. Name the destination generically and use a noun ("that information").
- `transfer.metadataClass.*` joined by `formatDisjunctionList`: "or" inside a negation reads stiff in fr (wants `ni`),
  hu, and nl; a conjunction ("and") or a colon-list frame would read naturally everywhere. (fr, hu, nl)
- Term consistency ties `fileOperations.transferTree.state.compressing` (a row's state) to
  `fileOperations.transferProgress.stageCompressing` (the dialog's stage); fr and nl wanted different forms. (fr, nl)
- `queue.row.itemCount`, `.folderCount`, `.fileCount`: the row splices them after the `queue.row.label` action word
  ("Copying 3 items"), but a case language wants the genitive after a verbal noun (ru `Копирование 3 объектов`) and the
  count also stands alone in the tooltip, so one form can't fit both. Consider a whole-row key with the counts inside,
  or say in the descriptions that the chunk reads as a separate column (ru ships nominative for now). (ru)
- Term consistency ties the badge `fileExplorer.fileIcon.sync.downloading` ("Downloading") to the progress line
  `settings.mediaIndex.clip.downloading` ("Downloading…"), since edge punctuation is stripped. Finder's own badge reads
  differently (de `Laden …`, `AXBADGE8`), so a locale can't follow Finder there without an allowlist entry. Decide
  whether the badge states split from that group or stay tied. (de)
- The reference pile has no git vocabulary (detached, unborn, ahead/behind, uncommitted changes): no tier names them.
  git's own `po/<lang>.po` (github.com/git/git, `po/`) is the authoritative translation for ~20 languages; propose
  adding it to the pile as a git-domain source. sv mined it live for `git-detached`, `git-unborn`, `git-dirty`. (sv, ru,
  es, de)
- `concepts.json` `archive-chat` matches `fileExplorer.list.rowKind.archive` ("archive", a zip file), so the brief lists
  the chat-archiving verb for it. Add that key to `archive-chat`'s `notMatch` and let the `archive` concept match it; hu
  carries a per-locale `exceptions` entry meanwhile. (hu, nl, vi, pt, sv, es, ru, zh-Hant, de)
- Term consistency ties `fileExplorer.list.parentRowName` ("Parent folder", a spoken noun naming the `..` row) to
  `menu.go.parentFolder` ("Parent folder", the Go command, which follows Finder's Enclosing Folder: ru
  `На уровень выше`). The description asks for a noun, so the row reads `Родительская папка`. Reword one English source,
  or add a term-consistency allowlist entry per locale (ru needs one now). (ru)
- Term consistency ties `fileOperations.transferProgress.sizeProgressValueText` (spoken by a screen reader) to
  `viewer.pull.progress` (a visible line), both "{doneText} of {totalText}". A compact slash suits the visible line but
  reads badly aloud. hu realigned both to its shipped `{free} szabad, összesen {total}` frame; vi, zh, and zh-Hant now
  diverge. Reword one English source, or allow the split per locale. (hu)
- `concepts.json` `image-search` groups photo, semantic, by-description, keyword, and tag search under one term, though
  each has its own UI name, so the term check flags every locale. Split it or give each feature its own match. (zh-Hant)
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
  `weergave`, same risk; es used `vista`, same risk, since it's also `la vista breve` / `la vista completa`)
- `fileExplorer.restic.forgetPassword.noBackup`: the description says to keep “Browse snapshots” in English; confirm
  that folder's name is never localized, or the quoted label goes stale the day it is. (all)
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
- `fileExplorer.navigation.favoriteUnreachable.phoneUnplugged`: say a gendered language may keep a type noun before the
  quoted `“{device}”` (es kept "El teléfono"); proposed rule: dropping a pronoun for a quoted name never forbids the
  object's noun. (es)
- `menu.volume.editFavoriteShortcut` is now "Set shortcut key…", but `fileExplorer.navigation.favoriteShortcutAriaLabel`
  and `.favoriteShortcutSaveFailed` still say "shortcut", so ru has two names for one control. Align the family; the
  menu item's description should also name the menu and say VERB. (ru, nl)
- `reply` / `ai-reply` concepts match a server's reply (`securityNotices.smbSignatureRefused.body`,
  `errors.volume.signatureInvalid`): the existing `server's reply` `notMatch` has a straight apostrophe, the English a
  curly one. Add the curly form and "the reply didn’t carry"; consider merging the two concepts. (nl, pt, sv, vi)
- `errors.volume.signatureInvalid`, `errors.write.replySignatureRefused.message`: "so Cmdr stopped" has no object, and
  verbs like sv `stoppade` need one; say what stopped in the description, or write "stopped there". (sv)
- `errors.eject.*`: the description lets the fragment start lowercase after the host's colon, but every shipped sibling
  starts capitalized. Say it for the whole family. (hu)
- Proposed rule: a version number or other insert after a word that must agree with it (an article that depends on
  pronunciation, a case ending) goes in a colon slot (`settings.managed.summary.upTo`). (hu)
- Proposed rule: a first-person checkbox label ("I’ve checked the key") may go impersonal where the first person would
  gender the user (ru past tense). Say so in such descriptions. (ru)
- `servers.refusal.s3FieldMalformed`: say a locale may name the classes ("letters a–z, digits 0–9") as long as both
  ranges stay verbatim. (fr)
- `errors.serverRequest.blockedByPolicy`: say which noun "this feature" should use (match
  `onboarding.stepAi.table.colFeature`?). (fr)
- `onboarding.stepAi.table.renameWithout`: "then review and apply it" leaves "it" to the reader; name the pattern. (pt)
- `uninstall` concept: note it's removing software, not unmounting (zh-CN macOS uses `卸载` for both). (zh)
- `fileOperations.trash.undoGone`: say a language without plural forms may use an `=1` arm when one item and many need
  different words (zh, vi did, following `fileOperations.cancelRollback.doneMovingBack`). (zh, vi)
- `ai.cloud.timedOut` calls the AI provider "the server", while `ai.cloud.providerRefused` on the same row and both
  onboarding twins say "the service". Pick one noun; every locale mirrored the split. (all)
- `fileOperations.transferProgress.undo`: “Undo” here reverses a whole finished operation (with confirmation), a fourth
  sense beside text undo, Put Back, and an AI rename run. Say so in the `undo` concept’s sense so locales with split
  words (nl `Herstel` / `Zet terug` / `Ongedaan maken`) pick deliberately. (all)
- `multiRename.mediaButtonName` (aria of `mediaButton`) invites a compound (de `Medienfelder`) that contains the label
  only as a substring; say whether WCAG 2.5.3 containment needs a whole word. (de; sv wrote `Fält för media`, since
  `Mediefält` lacks `media`)
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
- `askCmdr.tokens.thousands` / `.millions`: `formatTokenCount` pre-divides by 1,000 and 1,000,000, which can't express
  Chinese 万 (10⁴), so zh and zh-Hant fall back to Latin K/M. Consider `Intl` compact notation instead. (zh, zh-Hant)
- Proposed rule: a compact count suffix follows the CLDR short compact form, joined with a no-break space where CLDR
  spaces it (de `Tsd.`/`Mio.`, ru `тыс.`/`млн`, sv `tn`/`mn`, vi `N`/`Tr`). (all)
- `suggestedOps.action.*`: the label renders in its own span before the AI's title, so verb-final languages can't
  reorder (`Bewegen Alte Installer`). Add a `{action} {title}` frame key, or call it a chip label. (de, hu, nl)
- `suggestedOps.newNameTaken` ("Name already taken") and `multiRename.status.targetExists` ("Name is taken") say one
  thing in two Englishes; align them so `i18n-terms` keeps locales in step. (all)
