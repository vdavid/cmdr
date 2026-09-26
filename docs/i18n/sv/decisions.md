# sv decisions

Distilled Swedish rulings that need more than a `terms.json` line: "X over Y because Z", headings citing the keys they
cover so `pnpm i18n:brief` pulls them into a batch. The term rulings live in `terms.json`, the voice and mechanics in
`style.md`, open questions in `review-queue.md`. Source tiers: macOS (Finder, AppKit, System Settings, read live when
the pile lacks a bundle), then Microsoft terminology, then Total Commander / Nautilus / Thunar / Dolphin, then the
catalog.

## Apple translates Quick Look and Keychain (`commands.fileQuickLook.mac.label`, `menu.file.quickLook`, `ai.secretError.keychainTitle`/`.keychainBody`, `servers.sheet.remember`)

- Quick Look → `Överblick` (Finder `TL14`), so it's not on the don't-translate list, `fileExplorer.quickLookHint.*`
  included.
- The store is `nyckelring` (`Kom ihåg i nyckelringen`), the app is `Nyckelhanterare` (its `CFBundleDisplayName`).
  `servers.sheet.needsStoredSecret` quotes the checkbox label verbatim: change both or neither.

## Apple names are looked up live, never paraphrased (`commands.handler.zoomResetHintMenu`, `main.upgradeNudge.mac`, `errors.listing.ioSerious.suggestion`, `onboarding.stepOptional.*`)

- A string that names a menu item, System Settings pane, or Apple feature spells it as the running macOS does, looked up
  and dated: `Full skivtillgång` (never `fullständig åtkomst till skivan`), `Innehåll > Zoom > 100 %` (never
  `Visa > Zooma`), `Cmdr > Introduktion…`, `Skivverktyg > Skivkontroll`, `Lokalt nätverk` (never the descriptive
  `Lokal nätverksåtkomst`), `Startobjekt och tillägg` (never `Inloggningsobjekt och tillägg`).
- Why: a name that "sounds right" sends the user hunting for a pane that doesn't exist. `{full_disk_access}` in
  `errors.*` resolves from the OS at runtime, so a paraphrase elsewhere shows two names for one thing.

## Git: worktree kept, working tree translated (`errors.git.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

- git's own `worktree` stays English (en-word: `en länkad worktree`, `worktree:n`, plural `worktrees`); the generic
  "working tree" is prose and reads `arbetsträd`; "working directory" is `arbetskatalog`.

## Parent folder and the double-click hint (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- `överordnad mapp` everywhere, as Finder. A list row is a `filrad`; "What just happened?" → `Vad hände nyss?`.

## FAT32 files too large for the drive (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- `formaterad med {format}` over `som`, reusing `errors.listing.notSupportedErrno.suggestion`'s phrasing. "and N more"
  is front-loaded `och ytterligare {countText} …` so no trailing word agrees with anything.

## Transfer dialog fields (`fileOperations.transferDialog.*`, `queue.row.label`)

- "will create it during the copy/move" → active `Cmdr skapar den under kopieringen / flytten` over the pile's passive
  `skapas`, with the definite operation noun (`flytten`, never `flyttningen`).
- Queue-row arms are finite present verbs (`Kopierar`, `Byter namn`, `Skapar mapp`), fallback `Arbetar`.

## Archives (`fileExplorer.archiveEnterMenu.*`, `settings.archives.*`, `fileOperations.delete.archiveWarningStrong`)

- `arkiv` is neuter; the bare menu title `Arkiv` (File) never meets the zip sense in one string. A generic bundle is
  `paket`, app bundles `Appaket`, mirroring English's own bundle / app-bundle split.
- Removing from a zip is `ta bort … ur` (out of a container). Settings rows read `Vad Retur gör med en …`.
- The OOXML row (`settings.archives.ooxml.*`) says `Dokument` and bare `paket`: broader than the `Appaket` card below,
  as English's `packages` vs `app bundles`.

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- Active past `Klistrade in … som {filename}` over Nautilus's adjectival `Inklistrad`; `{kind}` arms carry their own
  article, `från urklipp` modifies all three (an `urklipps-` compound doesn't read on all), and the sentence ends on the
  uncontrolled `{filename}`.

## Archive password and compression (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `archivePassword.message` agrees with `{name}` (the file: `lösenordsskyddad`, `låsa upp den`); where the sentence says
  `arkivet` the neuter wins (`errors.volume.needsPassword`: `lösenordsskyddat`).
- Slider ends `Snabbare` / `Mindre` name packing speed and output size (Total Commander), not app speed.

## Operation log labels (`operationLog.*`, `commands.logOperationLog.*`)

- Status chips reuse `queue.row.status` word for word. Initiators `Du` / `AI-klient` / `Agent` (the last is a justified
  same-as-source). The skipped chip is `Överhoppad` (review queue).

## Shortcut conflicts and the macOS features they name (`shortcuts.system.*`, `shortcuts.conflict.*`, `downloads.shortcutRow.*`)

- Spotlight, Mission Control, Spaces stay English. `Teckenvisare`, `Avsluta tvingat`, `byte av inmatningskälla` are
  Apple-style and unverified (review queue). `modifierare` because MS's `låstangent` is the lock-key sense.

## Ask Cmdr chat (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- `svar` names "this one" / "the reply" in `askCmdr.error.budgetExhausted` / `.unfinishedReply` (`Svaret …`): a bare
  pronoun has no gender-neutral Swedish antecedent.
- `token` / `tokens` stays English in both branches (Swedish tech press; the pile's `säkerhetstoken` is another sense).
- Cmdr's own behavior repeats `Cmdr` across sentences over `den`/`det` (`askCmdr.empty.hint`,
  `ai.cloudConsent.askCmdr.contentsRule`); a pronoun is fine when its antecedent sits in the same sentence.

## Ask Cmdr names only the chat panel (`askCmdr.wake.needsFullDiskAccess`, `settings.ai.tooltipOff`, `settings.ai.provider.description`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.proactive`)

- `Ask Cmdr` stays only where the key's current English keeps it (the panel, its menu item, command, and settings);
  elsewhere the subject is `Cmdr` or `AI:n`, as that key's English says. Read the current English, not memory.
- `chatt` and `samtal` aren't interchangeable: each follows its own key's `chat` / `conversation`.
- `needsFullDiskAccess`'s second sentence copies `search.coverage.setUpFullDiskAccess`, which `@key` requires.

## Allow cloud AI (`ai.cloudConsent.*`, `askCmdr.gate.*`)

- `Tillåt moln-AI` (TCC's `Tillåt`, the shipped `Moln-AI` option). `moln-AI` is neuter: `Moln-AI är avstängt`, pronoun
  `det`. `Ask Cmdr` as a subject takes `den` (`Ask Cmdr är avstängd`).
- The imperative doubles as the label, so it's written unquoted where it's an instruction and quoted with `”…”` in
  `settings.ai.cloudConsent.lockedHint` after `Slå på`, as the English.
- `service` → `tjänst` and `provider` → `leverantör` both stay: the English distinguishes them.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- A photo's location → `var den togs`, never `plats`: right after `en bilds`, `plats` reads as where the FILE lives.
- `kamerauppgifter` over `kameradetaljer` (`detaljer` is the expandable technical-details section). `miniatyr` over
  Nautilus's `miniatyrbilder` (first-party word).
- "the list of files inside an archive" → `vilka filer som finns i ett arkiv` (a list-of calque reads badly).
- A list whose last item contains `och` takes `samt` before it (`… samt en bilds kamerauppgifter och var den togs`).
- `askCmdr.empty.hint` and `settings.askCmdr.intro` no longer promise `skrivskyddad` / "never changes anything": Cmdr
  writes notes and proposes renames, so they say it never changes a file without approval.

## Image indexing: network drives, scope, badges, progress (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.networkOff`/`.paused`, `askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.imageFacts.*`)

- photo → `bild` uniformly (Apple's Photos app is `Bilder`), so the feature reads as one word; the one legitimate
  `foton` is `onboarding.stepOptional.mtp.desc` (copying photos off a phone, not the feature).
- exclude → `utesluta`, never `undanta` (the pile's `undantag` only means exception). A passive per-image state is
  `Ingår inte i bildsökningen`, distinct from the user action.
- rename noun `namnbyte` (Thunar / Dolphin; macOS has only the verb), never `filbyte` (reads as swapping files). The
  warning badge is the noun `(överskrivning!)`: an imperative badge would command the overwrite it blocks.
- needs attention → `behöver ses över` over the calque `kräver uppmärksamhet`.
- caches → `cachemappar`: Swedish has no settled plural of `cache`, and the sentence means cache folders.
- "Ask Cmdr to prepare it again" → `Be Cmdr att …`: the English "Ask" is the verb, not the feature.
- Badges agree with the en-word `bild` (`Indexerad`), queued `Väntar på att indexeras` (Finder's badge pattern), active
  `Indexeras nu`, re-index `indexeras om`. Drive plurals put the tail inside both branches so `indexerad(e)` agrees.
- search by description → `sökning med beskrivning` (bare feature noun), toggle `Sök bilder med en beskrivning`.
- `Apple silicon` stays English and lowercase (the pile lacks Apple's `Apple-kisel`; the key says keep it).
- A network drive opts in with `välja in` internally and `aktivera` on the toggle; gently → `skonsamt`.

## Delete dialog trash switch, transfer From/To (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

- The switch reads `Flytta till papperskorgen`, identical to `transferDialog.titleVerbOnly`'s arm, so switch and button
  read as one pair. Headings `Från` / `Till` (Total Commander); the controls keep `mål` (`Målvolym`).

## Drive indexing's master switch (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`, `settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`)

- `<X> indexing` is a compound (`enhetsindexering`), never `indexering av <X>`: Dolphin's `Filindexering`, zero
  `indexering av` in the pile, and a bare singular after `av` is ungrammatical anyway.
- A switched-off feature `är avstängd`, never `är av`.
- The overridden badge is `Kräver enhetsindexering`, never `Av med …`: `av med` reads as "off with it!", an imperative
  badge. `Kräver X` is the catalog's cause pattern (review queue).
- stays unindexed → `indexeras inte` over the unattested `oindexerad`.

## Drive index change check (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- The run header is nominal, `Kontroll av ändringar`, to match its sibling headers; `kontroll` over colloquial `koll`
  (which lives only in the idiom `tappade koll på`). The running check is `genomsökningen`.

## Stalled transfer (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- No source has "stalled": `Inget har hänt på {duration}` (fits the queue row where it replaces `{duration} kvar`) and
  `står stilla` over `har stannat`, which overclaims that the transfer is over. `förlopp` is the progress indicator,
  `framsteg` the achievement sense; neither fits.
- "Waiting for X to respond" → `Väntar på att X ska svara` (Finder verbatim).
- `Stäng` for the button that leaves the transfer running; `Detaljerna finns i loggfilen`, since `åtgärdsloggen` is the
  other log.
- `stallInFlight` puts the tail inside both branches: `öppen … skriven` vs `öppna … skrivna` agree with the count.

## Copied path (`fileExplorer.clipboard.copiedPath`)

- The path shows on its own line, so the sentence ends on a colon and stands alone: `…, den finns nu i urklipp:`.
  `i urklipp` (settled), no possessive: there's only one clipboard.

## Operation queue (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- English widened "Transfer queue" to the category word, so Swedish does too: `Åtgärdskö`, paired with `Åtgärdslogg`.
  `överföring` stays right one level down (a copy or move in flight). ❌ Never `överföringskö` / `Överföringar`.
- The window title and the command label are indefinite `Åtgärdskö`; prose, including prepositional button labels
  (`Visa i åtgärdskön`), is definite. Don't flatten the two.

## Progress chip and failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- `Avfärda` over MS's `stäng` (AppKit `Avfärda popover`, and `Stäng` is reserved for windows). ❌ Not `Ta bort`: on an
  operation row it reads as re-deleting the files.
- Headline family `Gick inte att slutföra` + definite verbal noun (Finder's
  `Det gick inte att slutföra synkroniseringen`), `skapandet av mappen` over a compound because English points at THE
  folder. The headline stays clipped, byte-identical to the row's failed arm; body copy keeps `Det gick inte att …`.
- The aria spells `procent`; the visible tooltip keeps `{percentText} %`.
- `{label}` plus the count clause wobbles ("Flyttar till papperskorgen 3 objekt"), as English does; the fix belongs to
  the English key's shape, not one locale.

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

- The context arms are `queue.row.label`'s verbs plus `till {destination}` (Finder `Kopierar ”^1” till ”^2”`),
  destination unquoted as in the English. The fallback takes `i` (`Arbetar i …`): where work happens, not where items
  go. `Redigerar ett arkiv` needs the article because it's a sentence.
- `Allt annat är pausat` (neuter `Pausad` after `allt annat`) so the note and the rows read as one state.

## Queue button with an empty queue (`fileOperations.transferProgress.background`/`.backgroundAria`)

- `I bakgrunden` (Total Commander's neighbouring button ID in the same dialog). ❌ Not bare `Bakgrund` (the backdrop, a
  label not a command, and not contained in the definite `i bakgrunden`). `Kör i bakgrunden` is the fuller reserve.
- The aria `Håll igång den här i bakgrunden` is byte-identical to `queueTooltip`'s opening; containment is
  case-insensitive (`I` / `i`).

## Quit gate (`main.quit.*`)

- `Avsluta medan en åtgärd pågår?` (Finder's `en åtgärd fortfarande pågår`), keeping Cmdr's `åtgärd` over Total
  Commander's `aktivitet`.
- The countdown says `Cmdr avslutas om …`: an app quitting itself takes the deponent `-s`, and active `Avslutar`
  collides with Finder's progress stage "Finishing".
- `rensar bort` over `raderar`, which is the user's own destructive delete.
- `Fortsätt arbeta` / `Avsluta nu`; ❌ not `Avsluta ändå` ("should I at all?"), since this button skips the wait.

## Usage stats without "anonymous" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- The stats carry a stable random id, so ❌ never `anonym`, and ❌ never `pseudonym(iserad)`: that jargon is what the
  English avoids. `ett slumpmässigt id` over clunky `identifierare`; `kopplad till`.

## Awaiting-answer row and rollback confirm (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `Behöver ditt svar`, ❌ not `Väntar på svar`: the same column shows `Väntar` for queued rows.
- The rollback tooltip's stop is `Stoppa`, ❌ never `Avbryt`: that IS the Cancel button, which keeps the files.
- Overwritten files are `filer som den har skrivit över`; English "replaced" is the overwrite sense, not `ersätta`.

## Rollback family: `ångra`, not `återställ` (`fileOperations.transferProgress.*`, `operationLog.*`, `commands.logOperationLog.*`, `fileOperations.cancelRollback.*`)

- `återställa` is restore, and a rollback deletes what it wrote; MS's `återställa` for roll back is the database sense.
  The catalog keeps `återställa` for names that really come back (`askCmdr.renameUndo.*`) and resets.
- `Ångra klart` (Finder's `kopiera klart`) finishes a half rollback; ❌ not `Slutför ångringen` (the noun `ångring` is
  stilted, and the button sits in a list row). `operationLog.dialog.finishRollBack` and
  `fileOperations.rollbackConfirm.finishRollBack` stay identical; `Ångra klart den här åtgärden?` mirrors the title.
- `partiallyRolledBackNotice` says `lät resten ligga kvar`, ❌ not `som den var` (reads as "as before the operation").
- `smbNativeNote` uses the verb (`Det kan ta tid att avbryta eller ångra`) over the noun `ångring`.
- The tooltip `rollbackTooltipStopAndMoveBack` uses `lägg tillbaka`; ❌ never `radera`: a move's rollback deletes
  nothing.

## Rollback toast (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Reason lines copy `askCmdr.renameUndo.skipReason.*` word for word where the English matches (`… lämnades som den är`);
  `spotTaken` switches frame with the English (`lämnades där den ligger`), and says
  `något annat finns nu där den kom ifrån` so it can't sound like the neighbouring name-taken reason.
- Removed → `Raderade` (files off disk), ❌ never `Tog bort`.
- "all {countText} items" → `alla {countText} objekt` (a quantifier, no article), and `one` drops the number
  (`Raderade objektet som Cmdr hade skrivit`). `stoppedDeleting` says `där Cmdr lade dem` (it covers copy and compress),
  `stoppedMovingBack` `där flytten lade dem`.
- `stagedLeftover.*` is Cmdr's own work file: `ofullständig kopia`, `rensar bort`, and `vid en senare överföring`, ❌
  never "nästa gång": cleanup skips anything younger than an hour, so the next try may clear nothing.

## Rename toasts: chained and unconfirmed (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, `fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

- The two families mean opposite things (definitely kept vs unknown) and must never blur.
- "kept its name" → present `behåller sitt namn` (Total Commander's `Behåll namnet`): the state the file is in.
- "and so did …" → `, liksom …`. ❌ Not `och det gör …` (unreadable without a comma) nor bureaucratic
  `och detsamma gäller`.
- Unconfirmed follows `fileOperations.mkdir.timeoutMessage`: `så filen kan ändå ha bytt namn`. ❌ Not `gått igenom` or
  `lyckats` (the house voice avoids that status word). Several renames take the definite plural `namnbytena av`.
- `Det här mappnamnet / filnamnet kan inte användas` (Finder's `Namnet … kan inte användas`), no final period.

## Suggested operations (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- approve → `Godkänn` over macOS's `Ta emot` (AirDrop receiving); reject → `Avböj`.

## Duplicate (`commands.fileDuplicate.*`)

- `Duplicera` (Finder's File menu); no collision with `Kopiera` (F5).

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Raw family: single apostrophes, a doubled `''` shows twice on a real menu.
- The View menu is `Innehåll`, not `Visa`: Finder AND Safari agree, so it's Apple's standard, not a Finder quirk.
- `Window > Zoom` is the verb `Zooma`, the text-zoom submenu the noun `Zoom`; English says `Zoom` both times.
- Pin / unpin tab → `Fäst flik` / `Lossa flik` over Safari's `Nåla fast`: `lossa` is the natural opposite and the
  catalog already uses the stem. Changelog → `Ändringslogg`, apart from Help's `Nyheter`.
- `Öppna i redigeraren` over the stem-repeating `Redigera i redigeraren`.
- The tag row reads `Lägg till ”{color}”` / `Ta bort ”{color}”` (Finder). The selection submenu title is the noun
  `Markering`, not the verb `Markera` (`menu.context.selection`).
- The Dock menu (`menu.dock.*`) takes the app-name form `Öppna Cmdr`, ❌ not the file-name form `Öppna ”Cmdr”`, which
  reads as opening a file. `{name} ({parent})` stays as punctuation.
- `menu.network.open` is `Öppna` like `menu.file.open` (same sense); `menu.network.edit` copies
  `commands.serversEdit.label` byte for byte.
- `menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` and their `commands.selectionSelectSameKind.*`
  twins share one English each: reword neither alone. `*.{extension}` follows `med filtillägget`, so nothing inflects.
- Loading rows: `Söker efter appar…`, `delningsalternativ` (from `Dela`, not the network `delad mapp`).
- Justified same-as-source: `menu.view.zoom`, `menu.tag.orange`, `menu.view.askCmdr`.

## System SMB connection fallback (`fileExplorer.network.osMountFallback.*`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- native → `inbyggd`, ❌ not MS's `ursprunglig` (original format). `inbyggd` describes; `systemanslutning` names the
  same path elsewhere. The English "network" is dropped: `SMB-nätverksanslutning` is a heavy triple compound.
- "share not on server" is the one case where retrying won't help: no `just nu`, no `försök igen`. The short toast says
  `säger sig inte ha` so two `den` don't sit side by side.

## Mutation and volume errors (`errors.mutation.*`, `errors.volume.*`)

- Raw family. A volume's top folder is `rotmapp` (Thunar, Total Commander; macOS has none). System Integrity Protection
  stays English (Finder `ET6`).
- No blame on the person: `Ett namnbyte kan inte flytta ut ett objekt ur ett arkiv`, not `Du kan inte …`.
- `svarade inte i tid` over `tidsgränsen nåddes`: shorter, and names who went quiet. The change "may still land" →
  `kan fortfarande gå igenom`.
- `notSupported` / `ioError` need a head noun for the English "that": `den åtgärden`.
- "has no Trash" is indefinite `har ingen papperskorg` (no such thing); `macOS nekade flytten till papperskorgen` names
  macOS as the refuser, as the English does, ❌ not the impersonal `Det gick inte att …`.

## Crash dialog variants and the reports setting (`crashReporter.dialog.body.keptRunning`/`.unknown`, `settings.updates.crashReports.description`)

- `.keptRunning` and `.unknown` never say Cmdr crashed, quit, or stopped, and say `en rapport`, not `kraschrapport`.
- `stötte på ett problem` keeps `Cmdr` as subject like `.ended`; every pile phrasing is impersonal. ❌ Not
  `råkade ut för` (accident tone). kept running → `fortsatte köra` (AppKit's exception dialog); ❌ not `höll igång` (the
  queue's transitive verb) nor `fortsatte fungera` (a feature working, not a process living on).
- The three variants share `förra gången` although Apple says `När du senast …`: siblings must share a frame. If
  `.ended` is ever reworded, `När Cmdr senast kördes …` is the attested alternative for all three.
- The setting's description covers both outcomes in present tense; its label stays `Skicka kraschrapporter`.

## Eject and disconnect errors (`errors.eject.*`, `fileExplorer.navigation.ejectBusyTooltip`, `fileExplorer.navigation.disconnectBusyTooltip`)

- Raw family, and every value follows a colon in `fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast`, so it
  never repeats the frame's `det gick inte att`; it says why and what to do.
- in use → `används` (Finder), ❌ not MS's `upptagen`, which is reserved for the menu marker `(upptagen)`. idle →
  `när den inte används`, same thread.
- removable → `borttagbar` (Finder), over the MS / Thunar / Total Commander `flyttbar` Apple never uses.
- `koppla från` is programmatic, `koppla ur` the device out of the port, `dra ur` the cable.
- The noun `utmatningen` in `timedOut` (tentative): ❌ not `så den kan fortfarande matas ut`, which reads as "you can
  still eject it".
- Named-app refusals (`unmountRefusedBy*`) keep `unmountRefused`'s frame with `{app}` as a bare subject; active over
  AppKit's passive `används av ”%@”`, and the key forbids quoting the name. `other apps` → indefinite `andra appar`.
  macOS `arbetar fortfarande med` keeps the English split from an app's `använder`; `Vänta en minut` vs `Vänta en stund`
  keeps the English split too.
- disk image → `skivavbild`, short `avbilden`; ❌ not MS's `avbildning` (Windows side).
- A disabled button's tooltip says `Det går inte att koppla från medan åtgärder pågår på den här servern`, without the
  menus' `(upptagen)` marker.

## Trash toast (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- put back → `lägga tillbaka` (Finder's `Lägg tillbaka`), ❌ not `återställa` (restore, and used for names) nor
  `flytta tillbaka`: the menu item's word is the action's name.
- `Gå till papperskorgen` is 21 characters against 11; overflow-check it beside `Ångra` (review queue).
- `{skipped}` is a real selector, but `objekt` and the verb don't inflect, so both branches are identical.

## Amending a sent error report (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- add to → `lägga till i` (`Lägg till i Dock`), ❌ not `bifoga`, already taken by the email in the same dialog.
- The menu pointer is `från Hjälp-menyn` (hyphenated proper-name compound), never `menyn Hjälp` or `Hjälpmenyn`.
- `Visa rapporten eller lägg till en notering` keeps both halves and the word `notering` that ties it to the field.

## Selection dialog (`selection.*`)

- `markera` / `avmarkera` for files (Finder `Markera allt` / `Avmarkera allt`); `Välj` is for picking an option.
- `selection.recent.applyAria` mirrors `search.recent.runAria`; Enter is `Retur` everywhere.

## One thing, one name: the term-drift audit (`menu.file.delete`, `commands.fileDelete.label`, `settings.mediaIndex.clip.*`, `commands.selectionSelectAll.label`, `commands.selectionDeselectAll.label`, `fileExplorer.errorPane.goHome`, `menu.select.all`, `menu.select.deselectAll`)

- F8 is `Radera` in the menu, palette, key bar, and dialog: the pair must differ in strength (`Radera` /
  `Radera permanent`), which `Ta bort` / `Radera permanent` doesn't. The AI model is `Radera`d like `ai.local.*`;
  `settings.mediaIndex.reclaim.*` keeps `ta bort` (index rows, not files).
- A bare English `All` is `allt` (`Markera allt`, `Återställ allt till förval`): `alla` dangles without a head noun.
  With a head noun it inflects normally (`Stäng övriga flikar`, Safari's `övriga` over `andra`).
- `Kopierat` (supine) for a bare "Copied": it fits whatever was copied, and `ett id` is neuter anyway.
- `Gå till hemmappen` joins the `Gå till …` family. `mapp` / `mappar` over the abbreviation `kat.`.

## Same concept, different English (`commands.viewShowHidden.label`, `settings.fileViewer.suppressBinaryWarning.label`, `settings.fileExplorer.suppressQuickLookHint.label`)

- Hide → `Göm` (eleven Finder strings, zero `Dölj`); hidden → `dold` (the file-system term). Suppress is another verb
  and keeps `Dölj`. So `Visa eller göm dolda filer` avoids `dölj dolda`.
- `mappstorlekar` over `katalogstorlekar`; `katalog` stays where English means a technical directory.
- share → `delad mapp`; `-resurs` only inside a compound (Swedish can't compound a two-word phrase); `delning` only in
  the share counter.

## Boundaries: both forms are right, don't flatten them (`updates.status.checking`, `ai.local.statusRunning`, `operationLog.status.running`, `shortcuts.section.filterModified`, `menu.bar.file`, `menu.bar.view`, `menu.bar.select`, `menu.view.zoom`, `menu.window.zoom`)

- Checking: `Checking for X` → `Söker efter X` (looking for something that may exist), `Checking X` → `Kontrollerar X`
  (verifying something you have).
- Running: a process `Körs`, an operation `Pågår` (Finder's minimal pair).
- `unknown` agrees with the implied head noun: every shipped one is an en-word (`okänd`).
- Modified: `Ändrad` on one file's attribute, `Ändrade` on the filter chip over a set.
- File, View, Select, Zoom: menu-bar title vs action, as in § Native menus.

## Menu and palette twins, System Settings panes (`menu.app.hideOthers`, `commands.appHideOthers.label`, `menu.app.showAll`, `askCmdr.renameUndo.undone`/`.partial`)

- `Göm övriga` in both the menu and the palette (three Apple bundles), ❌ not `Göm andra`.
- The git and provider errors use the runtime tokens `{system_settings}` / `{privacy_and_security}` /
  `{files_and_folders}`; never hand-translate them or hang an ending on one (`i {system_settings}`).
- Restored names read `Det gamla namnet återställdes … fil` / `De gamla namnen återställdes … filer`, both branches
  spelled out because noun and participle agree; trash stays `Lade tillbaka`.

## Old WebKit and old macOS screens (`main.oldWebkit.*`, `main.oldMacos.*`)

- `Programuppdatering` over the anglicized `Mjukvaruuppdatering`. best effort → `så gott det går`; look off →
  `se konstiga ut`, which avoids the forbidden status words. The last sentence is David in first person.

## Open terminal here (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `Öppna terminal här` builds on Finder's `Öppna i Terminal`; the card title is a justified same-as-source. `Välj app…`
  is Finder's `Choose Application…` verbatim.

## Sort by relevance (`fileExplorer.columns.sortByRelevance`)

- `Sortera efter relevans`, indefinite like the sibling sort labels.

## Server hub: connection state, refusals, forget dialogs (`servers.paneState.*`, `servers.refusal.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

- trust → `lita på`, trusted `betrodd` / `betrott` / `betrodda` (SecurityInterface).
- The host key is bare `nyckel` where the English says "key", and `nyckeln från {host}`, never a genitive on the
  uncontrolled `{host}` (it may end in an s-sound).
- Cmdr's reconnect loop `arbetar på att …` (the catalog never says `jobbar`).
- Forget dialogs inherit the menu labels verbatim; `slutar visa servern i listan` names the noun because both
  `anslutningen` and `servern` are en-words and a lone `den` would point two ways.
- `Den här servern använder en nyckel …` mirrors `authMethodUnsupported`; ❌ not `loggar in med`, which makes the server
  the one logging in somewhere.
- Retry lengths (`retryTotalSeconds` / `.retryTotalMinutes`) are bare building blocks for `retryKeepsTrying`: no
  preposition, no period.

## Server hub: table and statuses (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`.serverUnpinnedToast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- Status cells are en-word participles agreeing with `servern` (`Ansluten`, `Sparad`, `Hittad i närheten`, `Utloggad`);
  if the row noun ever changes gender, rewrite the whole column.
- `Platser` (under a server) is broader than `delade mappar`: buckets will live there too. nearby → `i närheten`, ❌ not
  `upptäckt` (mDNS finds in the browser); English chose the plain "found".
- `Fäst / lossa server` keeps the slash: ONE command toggling both ways, unlike keys whose English says "or".
- `Servern är fortfarande sparad` names the noun: `volymväljaren` and `servern` are both en-words.

## Server hub: connect sheet, host key, root and start folder (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`, `servers.paneState.reconnecting`/`.signedOutNothingToAsk`)

- passphrase → `lösenfras`, ❌ not `lösenordsfras` (an OID field name). `Nyckelns lösenfras` / `Nyckelns fingeravtryck`
  read as a pair; a compound is opaque.
- Swedish can't leave `lita på` bare, so the buttons name the object (`Lita på nyckeln och anslut`), and "I've checked
  it" → `Jag har kontrollerat nyckeln`: both `nyckeln` and `fingeravtrycket` are in the box.
- Remote folder → `Mapp på servern`, ❌ not an unattested `fjärrmapp`.
- try → `prova` (test something); `Försök igen` is reserved for Try again.
- `Lämna fältet tomt`, ❌ not `Lämna den tom`: after `Mappen …` it would mean an empty folder.
- `ditt konto får läsa den` (permission) over `kan`. The two `*NotFound` siblings share a frame.
- `identityLocked` says `identifierar`, ❌ not a "name" verb (the sheet has its own `Namn` field), and uses the button
  verbs `glöm` / `lägg till` verbatim.
- `Öppna servern igen för att försöka på nytt` names the noun after a sentence ending on `nyckel` / `lösenord`.

## Pinned servers, trusted host keys, Android settings rows (`menu.network.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.fileOperations.adb*`)

- `Fäst i volymväljaren` / bare `Lossa` keep the English asymmetry; ❌ not `Ta bort` (the server stays listed).
- UI names are quoted by apposition (`listan Servrar`, `Gruppen Nätverk`), since a compound would respell them.
- Got it → `Uppfattat` over macOS's `OK`, which the catalog keeps for a dialog's default button.
- `värdnyckel` only where the English writes "host key".
- Found at {path} → `Hittades: {path}`, ❌ not `i {path}`: the path is the binary itself, not its folder.
- Re-check → `Leta igen`, tying the button to the placeholder `Leta efter adb på vanligt sätt`.
- The picker's title is `Välj kommandot adb` (Apple's picker button `Välj`); the button opening it is `Bläddra…`.
- Location of adb → `Sökväg till adb`, ❌ not Finder's `Plats` (the enclosing folder).
- `platform tools` stays English (the SDK Manager shows `Platform-Tools` in every locale; tentative).

## Android over ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`, `fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- `USB-felsökning` and `Tillåt` are AOSP's own Swedish, quoted as the names on the phone's screen: ❌ never `Godkänn` or
  `Acceptera`. A phone takes `tryck på`; the Mac's mouse takes `klicka`.
- reseat the cable → `dra ur och sätt i kabeln igen`: a bare `sätt i` doesn't say it comes out first.
- `Koppla från {name}` for a phone matches the server's `disconnectPlaceAriaLabel`: English chose Disconnect over Eject
  on both, and `mata ut` is reserved for ejecting.
- `Du stoppade öppnandet av din telefon`: `stoppa` like `search.coverage.walk.cancelled`, ❌ not `avbröt`, which reads
  as a reference to the Cancel button.
- How → `Hur?` (tentative): Apple has no one-word "How" link, and bare `Hur` reads truncated.
- Turn on a feature in a hint → `Slå på USB-felsökning`; the settings row states `har USB-felsökning aktiverad`.
- The phone's stale index keeps its siblings' frame with `telefonens`; passive `görs` for changes made on the phone,
  since no subject fits (the user or another app).

## Dock offer (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `Dock` takes no article, inflection, or possessive (`i Dock`), and pin / unpin on Apple's surface are `Behåll i Dock`
  / `Ta bort från Dock` (Dock's own menu), ❌ not Cmdr's `fäst` / `lossa`.
- The Applications folder is `Appar` since macOS 26: `dra … från mappen Appar` (AppKit's model sentence).
- managed by → `styrs av` (Apple's MDM formula); `hanterar` stays for the person administering the Mac.
- `Cmdrs symbol är på plats, men Dock startade inte om`: never claim the pin failed; the icon IS there.
- `Nej tack`, ❌ not `Inte nu`, which promises a later ask Cmdr never makes.
- Never a number for a movable threshold (`några dagar`, `ett tag nu`).

## Show in Finder offer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `”Visa i Finder”` quoted as in the settings card; the first-hit notice explains, ❌ never `tyvärr`.
- `settings.revealHandler.notProductionBuild` says `släppt version` / `utvecklings- och testversioner`: a release, not a
  second running `kopia` as in `main.instanceLock.alertBody`.

## AI provider setup (`onboarding.cloudSetup.*`)

- download → `hämta`: `step.install` was the catalog's only `Ladda ner` against 43 `hämta`. Check how often a form
  already appears before writing a new one. deployment → `distribution`, endpoint → `slutpunkt` (MS).

## Onboarding rewrite

### Beta-step checklist (`onboarding.stepBeta.checklist.*`)

- The heading `Checklista för att komma igång` follows the wizard's `komma igång` framing, not `introduktion`.
- A bare English `each` needs a head noun: `varje punkt tar 30 sekunder`.
- GitHub (UI localization ended 2016) and AlternativeTo have no Swedish UI to quote, so their verbs translate:
  `stjärnmärk`, `Gilla` (MS). `repo` is neuter (`repot`).
- The empty `<field></field>` renders the input inside the sentence, so it goes where Swedish wants the object
  (`Ange din e-postadress <field></field>, så …`). `<alpha></alpha>` and `<chip></chip>` get the same reflex.

### Mailing-list signup (`onboarding.stepBeta.signup.*`)

- `e-postlista`, ❌ not MS's `distributionslista` (an Exchange address group). typo → `stavfel`; ❌ Total Commander's
  `Skrivfel!` is a false friend (Write error).
- `signup.rejected` / `.unreachable` quote `checklist.emailSave` (`Spara`) verbatim: a cited label is a unit.

### AI step (`onboarding.stepAi.*`)

- `dummare` stays blunt, as `@key` asks. The `<strong>` block in `local.tooltip` quotes `cloud.label` verbatim
  (`Ja, jag vill ha AI`): change both or neither.
- `<em>och</em>` carries the emphasis alone; ❌ adding `både` makes it redundant.
- `strunta i` over bureaucratic `bortse från`; `badge` → `märke` (MS), ❌ not `aktivitetsikon` (the app-icon counter).

### Optional-step summaries (`onboarding.stepOptional.*`)

- The summaries sit beside switches and must not wrap, so they stay near English length and borrow each `…desc`.
- `macOS inbyggda hanterare`; suppresses a process → `håller tillbaka`, apart from visual Suppress → `Dölj`.

## Viewer fetches first (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- fetch → `hämta`; the heading follows Finder's `Förbereder kopiering av ”^1”` (present verb, quoted name), and
  `för förhandsvisning` needs no pronoun to agree with the file.
- "This file stopped arriving" → `Hämtningen av den här filen står stilla`, the catalog's stall wording.

## Mount and share-list errors (`errors.mount.*`, `errors.shareList.*`)

- share → `delad mapp`, although NetAuthAgent says `Delningspunkten`. this computer → `den här datorn`: the strings also
  show on Linux. `igen … igen` is avoided with `försök på nytt när servern är online igen`.
- Identical English, identical Swedish: the `hostUnreachable` and `authFailed` pairs.

## Text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `textredigerare` over MS's `textredigeringsprogram`: the catalog's shared stem `redigerare` decides (tentative).
- Shared strings (Choose an app…, Checking your apps…, the hint) copy the terminal picker's.

## Drive disconnected mid-work (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`, `errors.write.moveNotConfirmed.*`, `fileOperations.leftovers.stagingFolderKept`)

- In progress `håller på att kopplas från`; already gone `kopplades från`.
- Every sided message ends on WHERE the files are. Reassurance in present, place in past:
  `Dina original är orörda där de låg`; `så ingenting har gått förlorat` (perfect: the state stands).
- `dit` and a bare preposition before `{counterpart}` / `{volumeName}` avoid guessing the name's gender.
- Unconfirmed moves use the house formula `Det gick inte att bekräfta` / `Cmdr kunde inte bekräfta`, `hade sparats på`
  over `skrevs till` (writing is the in-flight transfer), and present `så dina original ligger kvar där de låg` answers
  where the files are now. `Titta i målmappen`, since `Titta i målet` doesn't read.
- The staging folder: unfinished move → `avbruten` (a process ended early) vs `stagedLeftover`'s `ofullständig` (a
  half-made object). `lät dem ligga kvar` says Cmdr CHOSE to keep them, ❌ not `blev kvar`. ❌ Never suggest deleting
  the folder: they may be the only copies.

## Favorites menu (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- The scope heading is indefinite `Favoritmeny` like its neighbours; prose is definite `favoritmenyn`. No aria quotes
  the heading, so the forms may differ; if one ever does, the heading changes form.
- English See and Show are one action, so both are `Visa`; `Se` is for looking at something.
- `fileExplorer.navigation.favoritesAddCurrent` is the one key for the `0` row, and the shortcut list quotes it: ❌ no
  second, definite wording.
- `a disk` → `disk` (English disk), `enhet` is drive, `skiva` only in Finder's own wording.
- The number keys are `siffra`, not `nummer` (a running number, as in `radnumren`).
- `favoritesCantAddHere` states the reason after a colon with `fungerar bara på`, a flat locative that avoids the split
  `på en disk` / `i en delad mapp`.

## Title-bar full disk access badge and the trash refusal dialog (`onboarding.fdaBadge.*`, `errors.write.trashRefused.*`)

- `onboarding.fdaBadge.label` and `onboarding.stepAi.bannerTitle.denied` share one English: reword neither alone.
- The aria opens with the label verbatim; ❌ never the definite `skivtillgången`, which stops containing it.
- badge → `märket` for the title-bar pill, ❌ not `statussymbol` (the overlay marker on a file row). "somewhere macOS
  keeps to itself" stays plain, ❌ never a macOS feature name.
- The five trash titles (`errors.write.fallback.title.trash` and its `ioError` / `readError` / `writeError` siblings)
  share one English: reword all or none. With no plural machinery, `{count} av objekten du valde` fits any number.

## Online-only warning (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- All four facts stay: the trash would download the files, so Cmdr offers only to delete the WHOLE selection, no copy
  lands in the trash but the service keeps its own (❌ don't play it down), and the ways out. The `<strong>` spans stay,
  and the quoted `”Radera”` is `fileOperations.delete.confirmDelete` verbatim.
- Prose says `bara online` (the everyday form); `endast online` is the ruled name. selection → `markering`, not `urval`.

## Look-alike names (`errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`, `fileOperations.transferProgress.lookAlikeHint`)

- Plain `stavar dem olika` / `lagrar dem med olika stavning`, no Unicode jargon as `@key` asks;
  `stora och små bokstäver` (Finder) over technical `skiftlägeskänslig`. The button label is the subject unquoted:
  `Skriv över ersätter …`.

## Escape leaves full screen (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `lämna helskärmsläge` (AppKit), definite for the state left (`tog Cmdr ur helskärmsläget`). The key is `Escape` in
  sentences (AppKit), `ESC` only on the keycap. `switchLabel` and the settings label share one value.
