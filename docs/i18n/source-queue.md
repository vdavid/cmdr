# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- `whatsNew.securityNotice.title`: the English says "This version fixed", but the description says the count spans every
  version installed since the dialog was last seen. Say which it is ("These updates fixed …"?); all locales mirrored
  "this version". (all)
- `main.cliCommand.*` / `menu.app.cliCommand*`: one action, three verb pairs (Install/Uninstall in the menu, "Removed"
  in toasts, "Adding or removing" in `canceled`). Pick one pair; most locales mirrored the split. (de, es, zh-Hant)
- `main.cliCommand.notInApplications`: "From anywhere else" reads as installed-from or running-from; say "If you install
  it from anywhere else, …". (hu, zh)
- `commands.appToggleCliCommand.description`: "open it" can point at the path or the terminal; say "open that file or
  folder in Cmdr", which also spares gendered languages a pronoun. (de, es, hu, nl, pt, ru, sv)
- `main.cliCommand.takenByOther`: "a different “cmdr”" leaves open file, link, or folder; gendered languages guessed.
  The description could say it's usually a file or link from another tool. (nl, sv, es, de)
- No `uninstall` concept: every locale ruled it ad hoc (de `deinstallieren`, nl `verwijderen`, zh `卸载`, …). Add one,
  distinct from `remove`. Also proposed: an `administrator` concept. (all)
- The termbase matcher counts words inside file-system paths (`local` in `/usr/local/bin`, now a `notMatch`). A
  path-token rule in the matcher would cover the next path too. (hu, ru, sv, zh, zh-Hant)

- `fileExplorer.pane.selectedCount`: English has only `=0` and `other`, so every `one`-category locale fails `plural`
  until it adds a `one` branch (nl copied `other`). Add `one {{countText} selected}` to the English so translators see
  it.
- No `guide` concept for `commands.helpAccessibility.label` / `menu.help.accessibility` ("Accessibility guide"); add one
  so locales align it with macOS Help's "User Guide" (ru `Руководство пользователя` → `Руководство по …`). (ru)
- `concepts.json` `size` matches `commands.viewerToggleImageZoom.label` ("actual size" = image zoom 100%, not file
  size); add it to `size`'s `notMatch`. ru carries an `exceptions` entry meanwhile. (ru)
- `git` is a product name the descriptions say to keep, but it isn't in `BRAND_WORDS`
  (`apps/desktop/scripts/i18n-catalog-lib.ts`), so nothing enforces it.
- `fileExplorer.list.rowGitStatus`: the description says `{status}` arrives "already translated", but `labelFor` in
  `status-column.ts` returns hardcoded English (`Modified`, `Untracked`), also used as the Git cell's tooltip and aria.
  Move those nine words into the catalog.

- `unlock` concept means an archive password, but three keys now use "unlock" for Finder's Locked flag
  (`fileOperations.lockedSources.message`, `cancelRollback.reason.locked.*`), each patched via `notMatch`. Move the
  Finder sense into `locked` (or its own concept) so the next one doesn't trip `termbase`. (de, nl, ru)

- `fileExplorer.network.browser.removeHostConfirm`, `.hostRemoved`, `.hostRemoveFailed`: descriptions (and key names)
  still say "remove" while the English says "Forget". Say forget, and that the host is a saved server.
- `fileExplorer.network.share.useGuest`: "Use guest" is terse and the description doesn't say whether it reconnects as
  guest or re-lists the shares. Consider "Switch to guest" or "Browse as guest".
- `menu.network.unpin`, `servers.pinHint.body`: the descriptions still call it a "very short label" / quote "Unpin", but
  the English is now "Unpin from switcher". Update both; "switcher" alone doesn't hit the `volume-switcher` concept
  either.
- `servers.paneState.notConnected`: "{name} isn't connected" forces agreement with `{name}` in gendered languages, and
  the description says "yet" where the English doesn't. Prefer "Not connected: {name}"-style framing.
- `servers.hub.shareAccount`, `.guestAccount`: "as {username}" has no bare equivalent in many languages; say a label
  form ("Account: sven") or an added verb is fine, and add a screenshot.
- `fileExplorer.navigation.forgetConfirmButton`: one button serves the server, share, and saved-password alerts; locales
  that clear a password with another verb (zh `清除`) get a button that doesn't match the title. Consider a separate
  key.
- `servers.hub.editPickHint`: "Select a server" pulls locales toward their file-marking verb; the English means moving
  the cursor to a row.
- `askCmdr.renameUndo.skipReason.vanished.named`, `.counted`: English and descriptions say "file", but a rename run can
  include folders (see `folderNotEmpty`); gendered locales added "the file" as the noun. Say "item", or confirm files
  only.
- `askCmdr.renameUndo.skipReason.vanished.counted`: `{countText}` is "always two or more", yet the ICU carries a `one`
  branch, and "they were moved" sits outside the plural. Drop the branch or the claim. (de, nl, sv, es)
- `fileOperations.trash.undoGone`: the plural only picks a pronoun ("it's / they're") with no antecedent, and is present
  tense where `undoPartlyGone` is past. Name the items, align tense, and say languages without number may drop it.
- `i18n:brief` lists function words ("since", "longer") under "No concept yet", which demands a pile mining nobody
  needs. Filter stopwords out of concept detection. (de, nl, zh)
- Pronoun rule vs. the shipped `askCmdr.renameUndo.skipReason.*` family: zh-Hant and fr siblings point `它` / « il » at
  `{name}`. Rule family-wide, or exempt languages whose pronouns carry no gender. (zh-Hant, fr)
- Cross-language rule proposal: when English prose names a button without quotes (`servers.sheet.addAnywayHelp`,
  `servers.pinHint.body`), a locale may quote it; or quote it in English too.
- `list` concept: matches the verb "lists" (Cmdr lists …); add a verb-sense `notMatch` so locales stop needing
  exceptions.
- `go-back` concept: matches "forward" while its headword is "Go back"; split or rename.
- `servers.refusal.s3FieldMalformed`: "lowercase letters" means Latin a–z, but a Cyrillic or accented letter is
  lowercase too. Say "Latin letters" in the English or the description (ru added `латинские`).
- `fileExplorer.archivedFile.label`, `askCmdr.sessions.archivedBadge`: both are "Archived" in English, so every locale
  needs a term-consistency allowlist entry. Consider "In cold storage" for the file glyph.
- `fileExplorer.network.share.signIn{Title,Message}`, `fileExplorer.networkMount.signIn{Title,Message}`: the screenshot
  is the servers list, not the calm sign-in screen these strings sit on. Capture that pane (with and without the sheet).
- `fileOperations.transferProgress.stage*` (compress and archive-upload phases, and their `*Step` variants): coupled to
  `transfer-dialog.png`, whose note lists only scanning/paused/queued/finishing. Capture a compress-to-remote run
  showing the "Step 1 of 2" line, and name the compress phases in the note.
- `step` concept: its definition says onboarding, setup guide, or install; widen it to any numbered stage of a
  multi-step operation (the two-step compress-then-upload labels now use it).
- `zip` has no concept, yet it recurs in prose (`errors.write.archiveEntryName*`, `settings.archives.*`). Register it so
  each locale's prose form is ruled (de neuter `Zip`, sv `zip-fil`, hu `zip archívum`, zh-Hant `zip`).
- `errors.write.archiveEntryName*` messages: "some tools", "zip tools", and the verb "share" tripped `ai-tool` and
  `network-share`; `notMatch` entries now cover these, but a sense-aware matcher would stop the next one.
- `servers.hub.nearbyGroup`: no screenshot, and the description doesn't say whether a count-last shape is fine (hu, zh,
  and zh-Hant lead with "found nearby"). Recapture the hub with the group header showing, and say the order is free.
- `errors.write.destinationNotAFolder.message` leaves `{path}` bare while its one-line twin
  `errors.volume.notADirectory` quotes it (“{path}”). If the dialog styles the path itself, say so in the description;
  otherwise quote it in both.
- `commands.paneFocusControls.*`: no concept for keyboard focus or "controls". Register them from macOS Keyboard
  Settings (`DefaultShortcutsTable.loctable`, "Move focus to the window toolbar"; Accessibility prose "move focus
  between controls"), which all 11 locales followed. Add "move focus" and "keys move between" to `move`'s `notMatch`.
  `location` also matches "location bar"; de uses Finder's `Pfadleiste` there. (all 11)
- `pnpm i18n:brief --keys a b c` (space-separated) silently briefs only `a` (header says "1 key"). Reject stray
  positional args, or accept both separators.
- Tooling: `sync-locale-keys.ts --restamp` skips overlays (`es-419`), so a new overlay fork's `sourceHash` (the hash of
  the `es` value it overrides) had to be computed by hand with `sourceHash()` for
  `fileExplorer.pane.openLocalNetworkSettings`. Let `--restamp` (or a `--fork`) stamp overlay keys too.
- `{localNetwork}` keys (`fileExplorer.pane.directConnectionBlockedByThisMacToast`, `.openLocalNetworkSettings`,
  `servers.refusal.localNetworkHint`): no screenshot of the toast or the Add server hint. Capture both so translators
  can judge the button's width and whether the pane name reads better quoted (zh and zh-Hant quote it, the rest don't).
- `volume-switcher` matches only "volume switcher" / "volume chooser", so a bare "switcher"
  (`settings.behavior.serversPinHintSeen.label`, `menu.network.unpin`) shows as "No concept yet". Add `switcher` to its
  `match`, with `app switcher` in `notMatch`.
- `settings.adb.install.intro`, `settings.fileOperations.adbEnabled.description`: the descriptions say to use Google's
  localized name for the platform tools and never keep the English, but Google doesn't localize "SDK Platform Tools" in
  most locales (sv, nl, fr, de checked). Say: keep the English where Google does. Also say whether "choose Re-check"
  means clicking (sv, nl, and es wanted `klicka på` / `klik op` / `haz clic en`). (sv, nl, fr, de, ru)
- Go to folder: Finder pt-BR splits its menu and dialog wording, which `i18n-term-consistency` forbids; the `goToPath.*`
  `@key` notes should say the menu item wins. (pt)
- `errors.eject.unmountRefusedByProcesses`: zh-Hant's list join can run Latin into Han with no space
  (`cfprefsd和其他程序`). (zh-Hant)
- `servers.paneState.unreachable`, the eject-process keys: say in the descriptions which shipped sibling to mirror
  (`servers.refusal.unreachable`, `errors.eject.otherApps`); every locale converged on them anyway. (fr)
- `adb.disconnectBusyTooltip`: "Disconnect" doesn't say whether Cmdr drops the device or the person leaves it, which
  decides transitive vs reflexive in fr. (fr)
- No concept yet for "called", "others", "reach", "usual", "leaving", "android", "adb"; "switcher" is the one that
  matters (zh-Hant has both 卷宗切換器 and a bare 切換器). (de, zh-Hant)
- `fileExplorer.listingStalled.*`, `indexing.overall.*`: no `screenshot`. Capture the stalled-listing pane and the
  checklist with the whole-run line, so locales can judge length and the spinner context. (all)
- `indexing.overall.eta`: `{eta}` can be "Almost done", which arrives capitalized after the colon; several locales then
  read "Total: Almost done". Say in the description whether the inserted phrase is sentence-initial or not. (de, hu, vi)
- `errors.write.insufficientSpace.*`, `fileOperations.errorDialog.copyAnyway`: no screenshot of the dialog with "Copy
  anyway". (all)
- `downloads.fda.message`: the description requires keeping Full Disk Access in English. Instead require the localized
  macOS permission label, as other FDA keys do.
- Plural instruction proposal: CLDR `one` does not mean exactly one. Audit counts such as 21/101; use `=1` for one-only
  wording and keep the displayed count in the ordinary `one` branch.
- `fileExplorer.tabBar.paneTabsAriaLabel`: `{paneId}` arrives as raw `left`/`right`, so screen readers say "панели
  left". Use a `select` in the English, like `fileExplorer.pane.filePaneAriaLabel`. (ru)
- `askCmdr.event.chatMemoryChanged`: `{tokens, number}` plus a fixed "tokens" can't agree in count-agreement languages
  (`16 384 токенов`). Make it a `{tokens, plural, …}` in the source. (ru)
- `queue.chip.ariaLabel`: the description wants "percent" as a word, which can't agree with a preformatted
  `{percentText}`. Allow `%`, or pass a numeric `percent` for an ICU plural. (ru)
- `queryUi.age.*`: `count` is passed as a string (`recent-items-utils.ts`), so count-dependent abbreviations (ru `г.` /
  `л.`) can't pluralize. Pass the number too. (ru)
- `queryUi.date.preset.thisMonth`, `.lastMonth`: `{month}` arrives nominative, so Slavic locales can't say "from the 1st
  of October" (`с 1 октября`). Pass a preformatted day-and-month. (ru)
- `errors.eject.*`: the host "Couldn't eject {volumeName}: …" puts a capitalized standalone sentence after a colon;
  Russian wants lowercase there. Say whether the fragment may start lowercase. (ru)
- `errors.eject.busy`: the English says "moving files there", but the description says copy, move, or delete; widen the
  English ("still working with files there"). (ru)
- `errors.write.readOnlyDevice.source.fallbackName`, `.destination.fallbackName`: the descriptions say "the subject of
  'is read-only'", which pushes gendered frames; say the value can be an archive or a `.git` history. (ru)
- `errors.listing.notSupportedErrno.suggestion` and other literal sizes: descriptions say keep `4 GB` Latin, while
  Russian macOS writes `4 ГБ`. Decide whether unit symbols localize, in the formatter and prose together. (ru)
- `settings.behavior.openTerminalHereApp.label`: "a label continued by the dropdown value" breaks in case languages
  (`… в Как в системе (…)`). Say a noun label is fine, as `settings.behavior.textEditorApp.label` does. (ru)
- `indexing.step.findFilesPhased`: the description says no trailing period on the second sentence, but the English has
  one. Align them. (ru)
- `fileOperations.transferDialog.rootEchoWarning`: "This place already starts in {rootFolder}" is hard to parse. Say
  "This location's path already starts with {rootFolder}…". (ru)
- `fileOperations.cancelRollback.reason.unverifiable.named`, `askCmdr.renameUndo.skipReason.unverifiable.named`:
  byte-identical English, but one is about items and the other about files; split the wording or say so. (ru)
- `menu.bar.select`: the description asks for an imperative verb, while `menu.context.selection` asks for its noun.
  Two-pane managers title this menu with a noun (TC "Mark", ru `Выделение`); allow either. (ru)
- `servers.paneState.cancelCycleTooltip`: "Switch back to retry" doesn't say switch back to what. (ru)
- `menu.volume.editFavoriteShortcut`: "Set shortcut…" captures one A–Z key, so locales write "key combination". Consider
  "Set key…". (ru)
- `servers.paneState.retryKeepsTrying`: say `{duration}` is the whole cycle, not the remaining time, so nobody writes
  "another N minutes". (ru)
- `crashReporter.sentToast.changeSettings`: "Settings > Updates" names a section that's now "Updates & privacy"
  (`settings.section.updatesAndPrivacy`). (ru)
- `common.attachEmailPlaceholder`, `settings.updates.emailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`:
  "localize the local part to your word for you" fails in Cyrillic scripts (`вы@example.com` isn't typeable). Allow a
  generic Latin `name@`. (ru)
- `pnpm i18n:brief --changed-since <ref>` diffs against the ref's tip, so a branch's brief also carries every key `main`
  changed after the split (about 11 per S3 brief). Compare against `git merge-base <ref> HEAD` instead. (all)
- S3 keys (`servers.sheet.s3*`, `servers.refusal.*` S3 arms, `*shareLink*`, `*coldStorage*`, `fileOperations.s3Cost.*`):
  none has a `screenshot`. Capture the S3 form, a refusal, the share-link submenu, and the cost line. (all)
- `errors.write.invalidName.suggestion` changed English (tabs and line breaks) in the same commit as the S3 batch but
  wasn't in its `--keys` brief; the stale check caught it. Brief stale keys alongside new ones. (all)
- `remove` concept: deleting a saved credential (`ai.secretError.removeTitle`, `onboarding.cloudSetup.removeKey`,
  `fileExplorer.network.share.forgetPasswordTooltip`) is a real deletion; consider a `notMatch` so vi stops needing
  exceptions. (vi)
- `network-share` concept: matches the verb in "share it yourself" (`errorReporter.dialog.managedOff`); add a verb-sense
  `notMatch` so locales drop their per-locale exceptions. (de, es, hu, nl, pt, ru, sv, vi, zh, zh-Hant)
- `organization` concept: its sense says only "the company a commercial license is issued to"; widen it to the employer
  or school that manages the Mac through MDM (`settings.managed.*`, `ai.managed.*`). (fr, hu, vi, zh, zh-Hant)
- `settings.managed.summary.upTo`, `.upToManualChecksOnly`, `updates.status.heldByPolicy`: a bare `{ceiling}` forces
  some locales to add "version", and "Up to" doesn't say it's inclusive. Say "Up to version {ceiling}" or "inclusive" in
  the description. (fr, nl)
- `errors.serverRequest.blockedByPolicy`: "turned this off" has no noun to translate; name it ("this feature") or say in
  the description what "this" refers to. (fr, ru)
- `ai.managed.cloudAiOff`, `.localOnlyUnsupported`, `settings.managed.summary.onDeviceOnly`: say "on-device AI" while
  the provider option is "Local"; use one English name, or say in the descriptions both mean the same model, and relate
  the `on-device` concept to it. (vi, zh, nl)
- "IT team" (`ai.translateError.managed.body`, `ai.managed.hostNotAllowed`, `askCmdr.error.managedByOrganization`) and
  MDM "manages" have no concepts, so locales split (team, department, staff). Register `it-team` and
  `managed-by-organization`. (ru, zh-Hant)
- `settings.managed.*`, `ai.managed.*`, `askCmdr.error.managedByOrganization`, `onboarding.stepBeta.analyticsManaged`:
  no screenshot of the managed card or the locked rows, so value widths are guesses. Capture them under a policy. (ru)
- `fileExplorer.compareDirectories.*`: no screenshot of the compare toasts, so their widths are unverified. (all)
- `settings.listing.spaceCalculatesFolderSize.*`, `fileExplorer.folderSizes.notConnected`: no screenshot of the switch
  or the Calculate-folder-sizes toast. (de…zh-Hant)
- `fileExplorer.quickFilter.*`, `settings.fileExplorer.typeToJump.mode.*`: no screenshot of the toast, the "Filter: …"
  badge with its ×, or the Jump/Filter toggle, so widths are guesses. Capture all three. (11)
- `settings.fileExplorer.typeToJump.mode.description`: the description says to use "the names printed on a Mac
  keyboard", but most non-US Mac keyboards print only the ⌫ glyph and an English "esc", so there's no printed name to
  copy. Say "the name macOS gives the key in your language (VoiceOver's key names)" instead; that's what the
  `delete-key` rulings record. (11)
- `commands.handler.getInfo.automationOff`, `.openAutomationSettings`: hardcode "System Settings > Privacy & Security >
  Automation" while siblings use the runtime `{system_settings}` / `{privacy_and_security}` / `{localNetwork}` tokens
  read from the user's Mac; use tokens (add an `{automation}` one) so a pane rename can't drift. (11)
- `commands.handler.getInfo.*`: no screenshot of either toast or the button, so widths are guesses. (11)
- Brief proposal: `pnpm i18n:brief --lang all` leaves out the `es-419` overlay, yet `i18n-es-overlay.test.ts` fails
  until new `es` keys using `Ajustes` or the compound perfect get forks. Have the brief list the overlay forks a batch
  needs. (es-419)
- `settings.appearance.tintMtp.*`: the notes say keep `ADB` and `Kindle` verbatim, but neither is in `BRAND_WORDS`, so
  `dont-translate` can't guard them. Add both (`ADB` alongside `MTP`). (11)
- Concept `unlock` says "opening a password-protected archive", but
  `fileExplorer.navigation.favoriteUnreachable.phoneUnplugged` uses it for unlocking a phone's screen, so the termbase
  match pins the archive word there. Broaden the sense (archive or device screen) or split a `unlock-device` concept.
  (de, zh-Hant)
- `fileExplorer.navigation.favoriteUnreachable.phoneUnplugged` vs `.driveUnplugged`: sibling toasts quote the name
  differently (`{device}` bare, `“{drive}”` quoted), though both are device names the user owns. Pick one; a locale that
  quotes owned names has to mirror the split or deviate. (hu)
- `settings.fileOperations.skipConfirmation.label`: "skip confirmation" isn't the `skip` concept (nothing is left out of
  the operation), yet the termbase pinned the conflict word `Ignorar` on it. Added `"notMatch": ["skip confirmation"]`
  to concept `skip`; other locales benefit. (pt)
- `fileExplorer.navigation.favoriteUnreachable.*`, `favoriteStatus.*`: no screenshot of the toasts or the dimmed-row
  tooltip, so widths are unverified. (pt)
- `fileExplorer.navigation.favoriteStatus.connects`, `favoriteUnreachable.forgotten`: `{place}` is a share, a server, or
  a phone, and nothing says which, so case languages can't put a type noun before it (`к серверу «…»`) and fall back to
  a parenthesis or colon. Pass a `{kind}` select (share / server / phone) alongside it. (ru)
- `fileExplorer.navigation.favoriteUnreachable.storageUnplugged`: "That storage on {device}" points at nothing the toast
  or tooltip names, so "that" has no antecedent; "The storage this folder was on" (or a `{storage}` name such as the SD
  card) would read clearly. (sv)
- `fileExplorer.navigation.favoriteUnreachable.mtpOff`, `.adbOff`: "Android phones are turned off in Settings" reads,
  translated literally, as the phones being powered off (vi `Điện thoại … đang tắt`). Name the feature: "Android phone
  access is turned off in Settings". (vi, zh-Hant)
- `multiRename.case.lower`, `.upper`, `.words`: say whether locales mirror the show-the-result casing. Proposed rule:
  such keys are exempt from sentence case, and scripts without case (zh, ja, ko) describe the transform. (fr, sv, vi,
  zh-Hant)
- `multiRename.removeDiacritics` and `.preset.removeDiacritics` share one English for a checkbox and a preset name; and
  say whether an everyday "accents" word is fine, since ß→ss goes beyond diacritics. (ru, pt, fr, nl)
- `onboarding.stepAi.table.renameWithout` ("batch rename UI") and `.rowRename` ("Mass-rename") likely name the new tool;
  align the English to "Multi-rename". (de, hu, es realigned their values)
- `multiRename.search` "Search for" is find-and-replace, yet the `search` concept means finding files; zh needed
  exceptions (查找 vs 搜索). Add a `find` concept or a `notMatch`. (zh)
- "Extension" is both `menu.sort.extension` and `multiRename.extensionMask`, so term-consistency forces one word; a
  field label might want the fuller form. Consider "Extension mask" or an allowlist entry. (sv)
- No screenshot of the Multi-Rename sheet, so label lengths (counter row, title-case option) are unverified. (all)
- Tooling: the brief tells translators to restamp and edit `terms.json`/`source-queue.md`, which races when 11 run in
  parallel. Consider a `--parallel` brief mode that routes termbase writes to a proposals file. (nl)
- `multiRename.swapsSkipped` vs `askCmdr.renameUndo.swapsSkipped`: the description says match the sibling, but the tails
  differ ("Rename those files one at a time." vs "Rename them one at a time."). Align the English. (all)
- `multiRename.status.missing`: the description says "a file", but a picked row may be a folder; say "item" so locales
  don't pick a file-only subject (all kept it subject-free). (ru, de)
- `commands.multiRenameSavePreset.label` says "current settings" while `multiRename.presets.saveAs` / `.update` say
  "current" / "current fields". Pick one noun; the locales used "settings" for the command and "fields" for the menu.
  (all)
- `multiRename.presets.saveAs` "Save current as…" is clipped English that most languages can't mirror; consider "Save as
  preset…", which every locale wrote anyway. (all)
- `multiRename.status.ready`, `multiRename.reason.*`: the notes say "a file", but a row may be a folder, and "Ready"
  forces agreement in gendered languages (es, fr, pt, ru restructured). Say "item" in the notes. (es, fr, pt, ru)
- `time-left` concept matched "it's left out" (`multiRename.reason.missing`) through "s left"; added
  `notMatch: left out`. A word-boundary-aware `match` would stop the next one. (all)
- `multiRename.presets.*`: no screenshot of the Presets menu, submenu, or name popover, so lengths are unverified. (all)
- `multiRename.placeholderHelp.grandparent` "the folder above that one": "that one" points back at the `[P]` line, but
  the note doesn't say that line sits right above it, so it reads as a free-standing noun phrase. Say so in the note, or
  write "the folder above the file's folder". (fr, es, pt, hu, ru, vi, zh-Hant)
- The `step` concept (onboarding stage) matches the counter's step in prose (`multiRename.placeholderHelp.counter`,
  `.counterAll`), so `termbase` flags the `rename-counter` word (es `incremento`). Add those keys to `step`'s
  `notMatch`, or match only "Step N of M", so no locale needs per-key exceptions. (es, pt, zh-Hant)
- `multiRename.searchOptionHelp.on` / `.off` share "On"/"Off" with the settings switch options, so `term-consistency`
  forces the settings form, while in gendered languages the tooltip row agrees with "option" (pt wanted `Desativada`,
  shipped `Desativado` to pass). Add an `agreesWith`-style hook or an allowlisted reason so a locale can agree with the
  option. (pt, ru)
- Tooling: the brief's reference-pile path (`<main clone>/_ignored/i18n/<tag>/`) doesn't exist on the M1 agent box, so
  "no ruling" terms there can't be mined as the brief requires. Sync the pile to the box, or have the brief say so and
  fall back to `terms.json` sources; vi mined the live Finder bundle instead (§ No pile on this machine). (ru, vi, sv)
- `servers.sheet.s3EndpointMoveHelp`: "with its favorites, open tabs, and saved secret access key" reads as each bucket
  owning a secret; the secret belongs to the account. Say "…comes along, with their favorites and open tabs, and the
  saved secret access key comes too". (all locales rendered the intended reading)
- `servers.sheet.addressMoveHelp`, `s3EndpointMoveHelp`, `servers.refusal.addressTaken`: no screenshot of the edit sheet
  in its move state; one would confirm the line sits under the Address / Endpoint field.
- `back` concept: matches "renames # files back to their old names" (`multiRename.undoTooltip`), which restores names;
  add a `notMatch` for "back to (its|their) old name(s)", then drop the es, fr, vi, zh, and zh-Hant exceptions.
- `rename` concept: matches "rename(s|ing) … back" (`multiRename.undoStarted`, `.undoTooltip`), which most locales say
  as "restore the old name"; add a `notMatch` or a put-back concept, then drop the de, es, hu, and vi exceptions.
- `start-up` concept: matches "startup disk"; add it to `notMatch` (hu carries five identical exceptions).
- `search-match` concept: fires on `multiRename.results*` and `.editedName`, which name the Results button, not search
  hits; add a `notMatch`.
- `multiRename.editedName`: say whether "in Results" means the button or the list file it opens (decides quoting).
- `multiRename.undoTooltip` vs `.undoStarted`: descriptions should spell out tooltip (what the button does) vs toast
  (progress); sv had shipped them byte-identical.
- `commands.multiRenameFieldHistory.label`: say that "values" means the text the user typed into the field earlier.
- Process: add a button's own key in the same batch as any string that quotes it (`multiRename.results.gone` quoted a
  Results label each locale had to invent).
- No `start` concept: starting an operation now reads `starten`, `Iniciar`, `Démarrer`, `Indítás`, `Start`, `Запустить`,
  `Starta`, `Bắt đầu`, `开始` (`fileOperations.backgroundStart.*Aria`, `.tooltip`). Mine the pile and add a ruled
  concept.
- `servers.sheet.smbAddressMoveHelp`: in "follow it the first time it opens there", both `it`s could be the server or
  the share, and `there` is vague. Say "…follow once that share opens at the new address". (all locales named the share)
- `servers.refusal.shareMounted`: "{name} is still mounted" makes gendered locales agree with an insert; most named the
  type first ("The share {name}…"). Consider "The share {name} is still mounted…" in English too. (es, fr, pt, ru, sv)
- `fileExplorer.clipboard.copiedPaths` vs `.copied`: two toasts of one family, but "Copied 5 paths." ends with a period
  and "Copied 5 items" doesn't. Pick one; every locale mirrors whichever English does.
- `line` / `move` concepts: fired on the queue reorder keys (`commands.queueMoveEarlier/Later.label`,
  `queue.reorder.*`), where "in line" is the queue order and "move" a reorder. Added `notMatch` forms ("in line", "moved
  up/down", "move the waiting operation", "operations can move"); a `queue-position` concept would rule "in line" across
  locales. (fr, nl: `in de wachtrij`, `vooraan` / `achteraan`; es: `en la cola`, `Adelantar` / `Retrasar`; ru:
  `в очереди`, `выше` / `ниже`, `Место в очереди: {position} из {waiting}`; sv: `i kön`, `Flytta upp` / `Flytta ner`,
  `Nu på plats {position} av {waiting} i kön`; vi: `trong hàng đợi`, `Đưa … lên trên` / `xuống dưới`,
  `đứng thứ {position} trong số {waiting} thao tác đang chờ`; zh: `在队列中上移` / `下移` (Nautilus `上移` / `下移`),
  `现在排第 {position} 位，共 {waiting} 项在等待`, `已经排在最前面了`, and `调整位置` for the bare reorder verb because
  `移动操作` already means a move operation; zh-Hant: `把等待中的操作往前移` / `往後移` (front of the line = sooner,
  so 前/後 over 上/下), `已往前移。現在排第 {position} 位，共 {waiting} 項在等待。`, `已經排在最前面。`, and `調整順序`
  for the bare reorder verb, same reason as zh)
- No `previous` concept (`commands.queueCursorUp.label`, `.queuePreviousControl.label`): fr already ships `précédent` 8
  times (tabs, palette, pages); add a ruled concept so locales converge. (fr; nl ships `vorig` / `vorige` across tabs
  and the file list; es ships `anterior` in `Pestaña anterior`, `Resultado anterior`; ru ships `Предыдущая вкладка`,
  `Предыдущий результат`, `Предыдущий файл`; sv ships `föregående` in `Föregående flik`, `Föregående resultat`,
  `Markera föregående fil`; vi ships `trước` in `Tab trước`, `Kết quả trước`, `trước đó` in `Chọn tệp trước đó`; zh
  ships `上一个` / `下一个` in `上一个标签页`, `上一个结果`, `选中上一个文件` (macOS AppKit `上一个版本`, Dolphin
  `上一个`), with the classifier swapped to `项` for operations: `上一项操作`; zh-Hant ships `上一個` / `下一個` in
  `上一個分頁`, `上一個結果`, `選取上一個檔案`, `上一個相符項目` (macOS zh-TW 8 files and zh-HK 9 with `上一個`,
  `前一個` 0), so `上一項操作` and `同一列的上一個按鈕`)
- Tooling: `sync-locale-keys.ts es-419 --restamp` skips overlays, and an overlay's `sourceHash` is the hash of the
  PARENT (`es`) value, not `en`; the brief's restamp step says neither. A new es key that trips
  `i18n-es-overlay.test.ts` (here `añadir`, `no se ha podido`) needs a hand-stamped fork. Teach `--restamp` overlays, or
  say so in the brief. (es)
- `fileOperations.sourcesGone.moreItems` vs `operationLog.dialog.moreItems`: the same list tail, but "and 17 more" drops
  the noun the other keeps. Pick one; every locale reused the "more items" form, which reads right either way.
- `fileOperations.sourcesGone.*`: "this delete" / "A queued delete" use delete as a noun, and "since you queued it" has
  no `queued` concept. Locales rendered it as "while it waited its turn", matching the queue's Waiting status.
- `transfer.metadataLeftBehind`: says "This server" though the description lists phones and cloud buckets too; "those"
  points back at the `{classes}` insert. Name the destination generically and use a noun ("that information").
- `transfer.metadataClass.*` joined by `formatDisjunctionList`: "or" inside a negation reads stiff in fr (wants `ni`),
  hu, and nl; a conjunction ("and") or a colon-list frame would read naturally everywhere. (fr, hu, nl)
- `back` and `running` concepts: "Scroll back to the files in progress" (`fileOperations.transferTree.followTooltip`)
  hit both, though it navigates nowhere and "in progress" describes files; add `notMatch` for "scroll back". (vi, ru,
  zh)
- No `listing` or `follow-mode` concept: `fileOperations.transferTree.preparing` and `.follow` had no ruling, and the
  viewer's tail toggle is the only follow precedent (`tail`). Add both, or `follow` to `tail`. (pt, zh, zh-Hant)
- Term consistency ties `fileOperations.transferTree.state.compressing` (a row's state) to
  `fileOperations.transferProgress.stageCompressing` (the dialog's stage); fr and nl wanted different forms. (fr, nl)
- `fileOperations.transferProgress.etaRemaining`: the description doesn't say whether "{duration} left" starts its cell
  or sits mid-line, so de (`noch`), nl (`nog`), and vi (`còn`) ship it lowercase while es and ru capitalize. Now that
  `filesLeft` / `itemsLeft` start the same line opposite it (capitalized: `Noch …`, `Nog …`, `Còn …`), say where the ETA
  lands (the readout cell and the queue chip) so those three can realign to sentence case. (de, nl, vi)
- `queue.row.itemCount`, `.folderCount`, `.fileCount`: the row splices them after the `queue.row.label` action word
  ("Copying 3 items"), but a case language wants the genitive after a verbal noun (ru `Копирование 3 объектов`) and the
  count also stands alone in the tooltip, so one form can't fit both. Consider a whole-row key with the counts inside,
  or say in the descriptions that the chunk reads as a separate column (ru ships nominative for now). (ru)
- No `upload` concept: `fileExplorer.fileIcon.sync.uploading` had no ruling. Finder's own cloud-status badges
  (`AXBADGE7`, `AXBADGE8` in `LocalizableMerged`) are the Tier-1 source for this whole badge set; add `upload`. (fr, hu,
  pt, ru, vi, zh-Hant, es, de)
- Term consistency ties the badge `fileExplorer.fileIcon.sync.downloading` ("Downloading") to the progress line
  `settings.mediaIndex.clip.downloading` ("Downloading…"), since edge punctuation is stripped. Finder's own badge reads
  differently (de `Laden …`, `AXBADGE8`), so a locale can't follow Finder there without an allowlist entry. Decide
  whether the badge states split from that group or stay tied. (de)
- `fileExplorer.git.repoChip.tooltip.aheadBehind` names the tracked branch bare (`behind {upstream}`), while its sibling
  `fileExplorer.git.tooltip.aheadBehind` quotes it (`behind “{vs}”`). Same sentence, two quoting styles; pick one so
  locales can share the frame byte for byte. Same split for `fileExplorer.git.repoChip.tooltip.onBranch` (bare
  `{branch}`) vs `fileExplorer.git.tooltip.stashedOnBranch` (quoted). (hu, pt, ru, sv, vi, zh-Hant, es, de)
- The reference pile has no git vocabulary (detached, unborn, ahead/behind, uncommitted changes): no tier names them.
  git's own `po/<lang>.po` (github.com/git/git, `po/`) is the authoritative translation for ~20 languages; propose
  adding it to the pile as a git-domain source. sv mined it live for `git-detached`, `git-unborn`, `git-dirty`. (sv, ru,
  es, de)
- `concepts.json` `archive-chat` matches `fileExplorer.list.rowKind.archive` ("archive", a zip file), so the brief lists
  the chat-archiving verb for it. Add that key to `archive-chat`'s `notMatch` and let the `archive` concept match it; hu
  carries a per-locale `exceptions` entry meanwhile. (hu, nl, vi, pt, sv, es, ru, zh-Hant, de)
- `concepts.json` `package` (.jar, .apk, Linux packages) matches `fileExplorer.list.rowKind.package`, which is a macOS
  package (Finder's "Show Package Contents"), so the brief offers the wrong sense. Route the key to `app-bundle` (or
  widen that concept to macOS packages) and add it to `package`'s `notMatch`. (zh-Hant, de)
- Term consistency ties `fileExplorer.list.parentRowName` ("Parent folder", a spoken noun naming the `..` row) to
  `menu.go.parentFolder` ("Parent folder", the Go command, which follows Finder's Enclosing Folder: ru
  `На уровень выше`). The description asks for a noun, so the row reads `Родительская папка`. Reword one English source,
  or add a term-consistency allowlist entry per locale (ru needs one now). (ru)
- Term consistency ties `fileOperations.transferProgress.sizeProgressValueText` (spoken by a screen reader) to
  `viewer.pull.progress` (a visible line), both "{doneText} of {totalText}". A compact slash suits the visible line but
  reads badly aloud. hu realigned both to its shipped `{free} szabad, összesen {total}` frame; vi, zh, and zh-Hant now
  diverge. Reword one English source, or allow the split per locale. (hu)
- `fileExplorer.git.status.*` descriptions say "a file", so es and pt agree the participles masculine (archivo, arquivo:
  `Modificado`, `Eliminado`). If the Git column ever shows a status on a folder row (git reports untracked or ignored
  directories), those read wrong for carpeta / pasta. State in the descriptions whether a folder row can carry a status.
  (es, pt)
- `concepts.json` `remove` matches `settings.dataWipe.doneMessage` ("remove the app" = trash it) and
  `settings.dataWipe.partialMessage` ("remove these yourself" = delete leftovers): both are deletions. Add them to
  `remove`'s `notMatch`; fr, es, zh, and vi carry `exceptions` meanwhile. (fr, es, zh, vi, zh-Hant)
- `settings.dataWipe.introWithSize`: "the {sizeText}" invites a definite article before a formatted amount, the same
  trap as "the {countText} items". fr restructured to `…, soit {sizeText}, …`, es to `…, {sizeText} en total, …`;
  mention it in the description. (fr, es, hu, nl, de, pt, sv)
- No `data` or `privacy policy` concept, though nine `settings.dataWipe.*` keys use "data" and `.stays` names the
  privacy policy. Add both to `concepts.json`; locales used gegevens, adatok, Daten, dữ liệu, 数据. (nl, hu, ru, de, vi,
  sv)
- `concepts.json` `item` matches "items in Keychain Access" (`settings.dataWipe.leftover.*`), which is the `entry`
  sense; add it to `item`'s `notMatch`. de carries two `exceptions` meanwhile. Also say in the description what Keychain
  Access calls one entry. (de, ru)
- `concepts.json` `image-search` groups photo, semantic, by-description, keyword, and tag search under one term, though
  each has its own UI name, so the term check flags every locale. Split it or give each feature its own match. (zh-Hant)
- `settings.dataWipe.description` reads as an imperative ("Delete everything…") but describes what the button does; say
  which in the description. `.stays`: say whether "the privacy policy" is a link, and that "Cmdr's servers" are Cmdr's
  own online servers, not the user's. `.refused.interrupted`: say whether "Try again?" is a button. (ru, hu, zh, nl)
- `operationLog.dialog.recordingOff`: its `screenshotNote` says the string belongs to "loading, empty, or a load error",
  but it's a notice above the list in every state. Say where it sits, or capture the dialog with recording off. (all)
- `settings.operationLog.enabled.description`: "Undo needs this." is a bare fragment whose "this" is the switch, and it
  reads as if undo stops entirely, which the last sentence then walks back. Consider "Turn this off and new operations
  can't be undone." Locales rendered it as "needed for undo". (de, fr, nl, pt, ru, sv)
- No concept for recording file operations (the new switch, `settings.operationLog.intro`): added `record-operations`
  with `tentative` rulings, since macOS has no data-recording sense to anchor them. (all)
- `errors.volume.signatureInvalid`: "wasn’t signed" says the reply carried no signature, but the description (and
  `errors.shareList.signatureInvalid`) say it had no VALID one. Consider "The server’s reply had no valid signature, so
  Cmdr stopped." Locales mirrored "wasn't signed". (all)
- No concept for the security event log or a reply's signature: added `security-event` and `signature`, and taught
  `watcher` ("events"), `note` ("that notes"), and both `reply` concepts ("server's reply") to skip these strings. (all)
- `servers.paneState.guestSessionRefusedHint`: "something is tampering with the connection", while every sibling (#457
  notices, `errors.shareList.signatureInvalid`) says "someone". Confirm or align; locales split. (de, nl, ru, vi,
  zh-Hant)
- `securityNotices.mcpForeignOrigin.title`: "its MCP server" can point at the web page once translated; most locales
  dropped the possessive. Say "Cmdr's MCP server" or "the MCP server". (hu, vi, zh, zh-Hant)
- `securityNotices.mcpForeignOrigin.openSettings`: the Settings section it opens is titled "MCP server"; name it in the
  description (and consider matching the label) so locales can quote it. (fr, ru)
- `securityNotices.smbSignatureRefused.body` opens with "It", pointing back at the separate title; pronoun languages had
  to name "this reply". Proposed rule: a toast body never refers to its title by pronoun. (fr)
- `securityNotices.smbSignatureRefused.titleUnnamed`: "a reply from a network share" reads oddly (the server replies,
  not the share). Consider "from a server". (sv)
- `securityNotices.mcpForeignOrigin.*`: the title says "web page", the body "site"; pick one. (nl, sv)
- `securityNotices.updateNotGenuine.body`: say whether "David" may take a case ending or transliteration in running
  prose (ru wrote `Дэвиду`, following the existing `с Дэвидом`). (ru)
- No `tamper` concept, though five #457 keys and `errors.shareList.signatureInvalid` use it (de `manipulieren`, ru
  `вмешательство`, zh `篡改`). Add one; ru also proposes a `genuine` concept (an update really from Cmdr). (de, ru, zh)
- `up-to-date` matches "latest version", so "Get the latest version" (`securityNotices.updateNotGenuine.body`) is held
  to it, and zh's short `最新版` then fails the `version` ruling. Consider dropping that match. (es, nl, pt, sv, zh)
- `blocked` spans a rename-preflight STATUS and an actor turning something away (#457's MCP notice); French can't use
  one word for both. Consider splitting it. (fr)
- Both `reply` concepts matched the #457 SMB strings ("Server reply refused", "refused a reply from"): taught them
  `server reply` and `a reply from`. (all)
- `decisions.md` is at its byte budget for es and zh, so the #457 rulings (zh genuine `真正的`, guest session
  `客人会话`) went to `review-queue.md` instead. Bump `decisionsBytes` in `i18n-termbase-baseline.json` if they belong
  there. (es, zh)
- `stateRecovery.goToPathHistory.title`: English says "Go to path", but the dialog is now "Go to folder"
  (`goToPath.dialog.title`). Rename it in the English; hu quoted the shipped title; de added `go to path` to `path`'s
  `notMatch` instead. (hu, de, vi, zh, nl, pt, zh-Hant, sv, ru, es, fr)
- `transfer.durabilityUnconfirmedUnnamed`: "written to the disk" then "If the drive…" names one device two ways in one
  sentence pair; consider "written to the drive" to match `transfer.durabilityUnconfirmed`'s second sentence. (vi, zh,
  pt, nl, zh-Hant, sv, hu, es, fr, ru)
- `stateRecovery.operationLog.title`: "your undo history" names a record the UI elsewhere calls the operation log (its
  Settings section, the multi-rename undo hint). A reader can't tell they're the same thing; consider naming the
  feature, or say in the `@key` description which UI name it should match. (zh-Hant, pt, sv, fr, es, ru)
- `stateRecovery.multiRenameLastSettings.title`: "the tool opens" tripped `ai-tool`; added `the tool opens` to its
  `notMatch`. (hu)
- `stateRecovery.*`: two more English senses tripped concepts; taught them in `notMatch`: "back to the defaults"
  (`back`, `stateRecovery.shortcuts.title`) and "shows as" (`show`, `stateRecovery.license.body`, meaning "appears as").
  (de)
- `stateRecovery.chatHistory.title`: "so they start fresh" says the chats themselves restart, while the description says
  earlier chats are gone. A literal "die beginnen opnieuw" reads odd for chats; consider "so your chat list starts
  empty", matching the favorites/presets "the list starts empty" frame. (nl, zh-Hant, sv, pt, fr, es, hu, ru)
- `errorReporter.prefill.pageCrash`: "stopped working" tripped `stop` (halting a running thing), but here it means a
  malfunction; added `stopped working` / `stops working` to its `notMatch`. (de, fr)
- `errorReporter.pageCrash.body`: "Reloading usually helps … sending a report helps get it fixed" repeats "helps" in
  adjacent sentences; several languages varied the verb. Consider "a report lets us fix it" or similar. (de, fr, ru, zh)
- `viewer.markdown.remoteImages.notice`: "Images from the web are hidden" tripped `hidden` (dotfiles), but it means held
  back from view (`hide`); added the phrase to `hidden`'s `notMatch`. (de)
- `format` concept: "View formatted" / "format this document" (Markdown viewer) mean rich-text rendering, not a drive,
  archive, or display format; added both to `format`'s `notMatch`. Consider a `rendered-view` concept if more keys use
  this sense. (fr)
- `fileOperations.transferProgress.canceledAlertMessage`: "You’ve canceled this compress." uses "compress" as a noun,
  which reads odd in English; consider "this compression" (most languages used their compression noun). The `en-GB` /
  `en-AU` overlays fork `errors.write.cancelled.title.*` ("cancelled"), but not these two new keys, so British readers
  get "canceled" here only. (all)
- `errors.write.linkCantMoveAcrossVolumes.*`: bare "link" means a symlink, but the matcher maps it to hyperlink `link`
  (de, nl logged exceptions). Say "symbolic link" once in the message, or teach the matcher this sense. (de, nl, all)
