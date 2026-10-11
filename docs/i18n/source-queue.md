# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

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
- `commands.dialogConfirm.label` ("Confirm open dialog"): "open" reads as the File > Open dialog, and de, fr, hu, nl,
  vi, and zh translated it that way. Say "Confirm the dialog on screen", or note that "open" is an adjective.
- No `start` concept: starting an operation now reads `starten`, `Iniciar`, `Démarrer`, `Indítás`, `Start`, `Запустить`,
  `Starta`, `Bắt đầu`, `开始` (`fileOperations.backgroundStart.*Aria`, `.tooltip`). Mine the pile and add a ruled
  concept.
