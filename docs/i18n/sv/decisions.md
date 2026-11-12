# sv decisions

Distilled rulings too long for a `terms.json` line, under headings citing their keys (`pnpm i18n:brief` pulls them in).
Terms: `terms.json`; voice: `style.md`; open questions: `review-queue.md`. Tiers: macOS (read live if the pile lacks a
bundle), Microsoft, file managers, the catalog.

## Apple translates Quick Look and Keychain (`commands.fileQuickLook.label`, `menu.file.quickLook`, `ai.secretError.title`/`.body`, `servers.sheet.remember`)

- Quick Look → `Överblick` (Finder `TL14`), `fileExplorer.quickLookHint.*` too. Store `nyckelring`
  (`Kom ihåg i nyckelringen`), app `Nyckelhanterare` (`CFBundleDisplayName`); `servers.sheet.needsStoredSecret` quotes
  the checkbox verbatim.

## Apple names are looked up live, never paraphrased (`commands.handler.zoomResetHintMenu`, `main.upgradeNudge`, `errors.listing.ioSerious.suggestion`, `onboarding.stepOptional.*`)

- `Full skivtillgång`, `Innehåll > Zoom > 100 %`, `Cmdr > Introduktion…`, `Skivverktyg > Skivkontroll`, `Lokalt nätverk`
  (❌ `Lokal nätverksåtkomst`), `Startobjekt och tillägg` (❌ `Inloggningsobjekt …`). A paraphrase sends users hunting,
  and `{full_disk_access}` in `errors.*` resolves from the OS (two names for one thing).

## Git: worktree kept, working tree translated (`errors.git.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees`)

- git's `worktree` stays (en-word: `en länkad worktree`, `worktree:n`, `worktrees`); working tree `arbetsträd`, working
  directory `arbetskatalog`.

## Parent folder and the double-click hint (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- `överordnad mapp` (Finder); a list row `filrad`; `Vad hände nyss?`.

## FAT32 files too large for the drive (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- `formaterad med {format}` (as `errors.listing.notSupportedErrno.suggestion`); "and N more" front-loads
  `och ytterligare {countText} …` so nothing trails to agree.

## Transfer dialog fields (`fileOperations.transferDialog.*`, `queue.row.label`)

- Active `Cmdr skapar den under kopieringen / flytten` over passive `skapas`; `flytten`, ❌ `flyttningen`. Queue-row
  arms are finite verbs (`Kopierar`, `Skapar mapp`), fallback `Arbetar`.

## Archives (`fileExplorer.archiveEnterMenu.*`, `settings.archives.*`, `fileOperations.delete.archiveWarningStrong`)

- `arkiv` is neuter and never meets the menu title `Arkiv` in one string. Bundle `paket`, app bundle `Appaket`; the
  OOXML row says `Dokument` and bare `paket`, as English's packages vs app bundles. Out of a zip: `ta bort … ur`. Rows
  read `Vad Retur gör med en …`.

## Paste clipboard as a file (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- Active `Klistrade in … som {filename}` over Nautilus's `Inklistrad`; `{kind}` arms carry their article, `från urklipp`
  covers all three (no `urklipps-` compound), and the sentence ends on `{filename}`.

## Archive password and compression (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `archivePassword.message` agrees with the file (`lösenordsskyddad`, `låsa upp den`); after `arkivet`, neuter
  (`lösenordsskyddat`). Slider ends `Snabbare` / `Mindre` (Total Commander) mean packing speed and size.

## Operation log labels (`operationLog.*`, `commands.logOperationLog.*`)

- Chips copy `queue.row.status`. Initiators `Du` / `AI-klient` / `Agent` (justified same-as-source); skipped
  `Överhoppad`.

## Shortcut conflicts and the macOS features they name (`shortcuts.system.*`, `shortcuts.conflict.*`, `downloads.shortcutRow.*`)

- Spotlight, Mission Control, Spaces stay English; `Teckenvisare`, `Avsluta tvingat`, `byte av inmatningskälla` are
  unverified (review queue). `specialtangent` is macOS sv's own word.

## Ask Cmdr chat (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- "this one" / "the reply" → `Svaret …` (`budgetExhausted`, `unfinishedReply`): a bare pronoun has no neutral
  antecedent.
- `token` / `tokens` stays English (the pile's `säkerhetstoken` is another sense). A pronoun for Cmdr is fine only
  inside one sentence (`askCmdr.empty.hint`, `ai.cloudConsent.askCmdr.contentsRule`).
- Tool lines (`askCmdr.tool.*`): tool call → `verktygsanrop` (Microsoft's `anrop`); "it" (the AI) → `AI:n` or a
  pronoun-free form (`finns inte som verktyg`); a folder insert takes its noun
  (`hur viktig mappen <place>…</place> är`); stopped answer → `Stoppat` (agrees with `svaret`).

## Ask Cmdr names only the chat panel (`askCmdr.wake.needsFullDiskAccess`, `settings.ai.tooltipOff`, `settings.ai.provider.description`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.proactive`)

- `Ask Cmdr` only where the key's current English keeps it; else `Cmdr` / `AI:n`. `chatt` / `samtal` follow each key's
  chat / conversation. `needsFullDiskAccess` copies `search.coverage.setUpFullDiskAccess`, as `@key` requires.

## Allow cloud AI (`ai.cloudConsent.*`, `askCmdr.gate.*`)

- `Tillåt moln-AI` (TCC); `moln-AI` is neuter (`Moln-AI är avstängt`, `det`), `Ask Cmdr` takes `den`. Unquoted as an
  instruction, `”…”` after `Slå på` in `settings.ai.cloudConsent.lockedHint`. `tjänst` and `leverantör` stay distinct.

## Ask Cmdr looks inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- A photo's location → `var den togs`: after `en bilds`, `plats` reads as where the file lives.
- `kamerauppgifter` (`detaljer` is the technical-details section), `miniatyr` over `miniatyrbilder`; the archive list →
  `vilka filer som finns i ett arkiv`.
- ❌ `skrivskyddad`: Cmdr writes notes and proposes renames; say it changes no file without approval.

## Image indexing: network drives, scope, badges, progress (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.networkOff`/`.paused`, `askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.imageFacts.*`)

- photo → `bild` (Apple's `Bilder`); only `onboarding.stepOptional.mtp.desc` says `foton` (phone photos).
- exclude → `utesluta` (❌ `undanta`: `undantag` is exception); state `Ingår inte i bildsökningen`.
- rename noun `namnbyte` (Thunar / Dolphin), ❌ `filbyte`. needs attention → `behöver ses över`; caches → `cachemappar`
  (no settled plural); "Ask Cmdr to …" → `Be Cmdr att …`.
- Badges agree with `bild`: `Indexerad`, `Väntar på att indexeras`, `Indexeras nu`, `indexeras om`; drive plurals carry
  the tail in both branches.
- `sökning med beskrivning`, toggle `Sök bilder med en beskrivning`. `Apple silicon` stays. Network opt-in: `välja in`
  in prose, `aktivera` on the toggle; gently → `skonsamt`.

## Delete dialog trash switch, transfer From/To (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

- The switch equals `transferDialog.titleVerbOnly`'s `Flytta till papperskorgen`. Headings `Från` / `Till` (Total
  Commander); controls keep `mål` (`Målvolym`).

## Drive indexing's master switch (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`, `settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`)

- `<X> indexing` compounds (`enhetsindexering`, Dolphin `Filindexering`; zero `indexering av` in the pile).
- Badge `Kräver enhetsindexering`, ❌ `Av med …` (reads "off with it!"). Unindexed → `indexeras inte`, ❌ `oindexerad`.

## Drive index change check (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- Nominal header `Kontroll av ändringar` like its siblings (`koll` only in `tappade koll på`); the running check is
  `genomsökningen`.

## Stalled transfer (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- `Inget har hänt på {duration}` (fits the row's `{duration} kvar` slot); `står stilla`, ❌ `har stannat` (overclaims an
  end). Neither `förlopp` nor `framsteg` fits.
- `Väntar på att X ska svara` (Finder). `Stäng` leaves it running; `Detaljerna finns i loggfilen`, since `åtgärdsloggen`
  is the other log.

## Copied path (`fileExplorer.clipboard.copiedPath`)

- The path sits on its own line: `…, den finns nu i urklipp:`, no possessive (one clipboard).

## Operation queue (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- Title and command indefinite, prose definite (`Visa i åtgärdskön`). Order: `i kön` (❌ `rad`), `Flytta upp` / `ner`,
  `plats {position}`.

## Progress chip and failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- `Avfärda` (AppKit), ❌ `Ta bort`: on an operation row it reads as re-deleting files.
- Headline `Gick inte att slutföra` + definite verbal noun (Finder), byte-identical to the row's failed arm;
  `skapandet av mappen` because English means THE folder.
- Aria spells `procent`, the tooltip `{percentText} %`. The `{label}` + count wobble is the English key's shape.

## Standalone conflict prompt (`fileOperations.operationConflict.context`/`.pausedNote`)

- Arms are `queue.row.label`'s verbs + `till {destination}` unquoted (Finder); fallback `Arbetar i …`;
  `Redigerar ett arkiv` needs its article. `Allt annat är pausat` matches the rows' state.

## Queue button with an empty queue (`fileOperations.transferProgress.background`/`.backgroundAria`)

- `I bakgrunden` (Total Commander), ❌ bare `Bakgrund` (a backdrop, not contained in `i bakgrunden`); reserve
  `Kör i bakgrunden`. The aria `Håll igång den här i bakgrunden` opens `queueTooltip` byte for byte.

## Quit gate (`main.quit.*`)

- `Avsluta medan en åtgärd pågår?` (Finder). Countdown `Cmdr avslutas om …`: active `Avslutar` is Finder's "Finishing".
- `rensar bort`, ❌ `raderar` (the user's delete). `Fortsätt arbeta` / `Avsluta nu`, ❌ `Avsluta ändå` (the button skips
  the wait).

## Usage stats without "anonymous" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- A stable id: ❌ `anonym`, ❌ `pseudonym(iserad)`: `ett slumpmässigt id`, `kopplad till`.

## Awaiting-answer row and rollback confirm (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `Behöver ditt svar` (the column already shows `Väntar`). Stop is `Stoppa`: `Avbryt` IS Cancel, which keeps the files.
  "replaced" → `skrivit över`.

## Rollback family: `ångra`, not `återställ` (`fileOperations.transferProgress.*`, `operationLog.*`, `commands.logOperationLog.*`, `fileOperations.cancelRollback.*`)

- MS's `återställa` for roll back is the database sense; a rollback deletes what it wrote.
- `Ångra resten` (like `Kopiera resten`), ❌ `Slutför ångringen`; both `rollBackRest` keys stay identical.
- `lät resten ligga kvar`, ❌ `som den var`; `smbNativeNote` uses the verb `ångra`. A move's rollback `lägg tillbaka`,
  ❌ `radera` (it deletes nothing).

## Rollback toast (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Reasons copy `askCmdr.renameUndo.skipReason.*` where English matches; `spotTaken` says
  `något annat finns nu där den kom ifrån`, unlike the name-taken reason.
- Removed → `Raderade`, ❌ `Tog bort`. `one` drops the number (`Raderade objektet som Cmdr hade skrivit`);
  `stoppedDeleting` `där Cmdr lade dem`, `stoppedMovingBack` `där flytten lade dem`.
- `stagedLeftover.*`: `ofullständig kopia`, `rensar bort`, `nästa gång Cmdr ansluter dit`, never a time.

## Rename toasts: chained (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, `fileOperations.validation.nameNotUsable`)

- `behåller sitt namn` (Total Commander); "and so did" → `, liksom …`, ❌ `och det gör …` / `och detsamma gäller`.
  `Det här mappnamnet / filnamnet kan inte användas` (Finder), no period.

## AI-förslag (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- `AI-förslag` (as the indicator); aria `Varför, enligt AI:n:` holds `Varför:`.
- `Godkänn` (❌ AirDrop `Ta emot`) / `Avböj`; tick boxes `Ta med`, ticking `bocka för` (`Markera` selects files).

## Duplicate (`commands.fileDuplicate.*`)

- `Duplicera` (Finder), no clash with `Kopiera` (F5).

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- View → `Innehåll` (Finder AND Safari). `Window > Zoom` → `Zooma`, the text-zoom submenu `Zoom`.
- Tabs `Fäst flik` / `Lossa flik` (❌ Safari's `Nåla fast`). Changelog `Ändringslogg`, apart from Help's `Nyheter`.
  `Öppna i redigeraren`.
- Tags `Lägg till ”{color}”` / `Ta bort ”{color}”` (Finder); the submenu is the noun `Markering`.
- Dock menu `Öppna Cmdr`, ❌ `Öppna ”Cmdr”` (opens a file). `menu.network.open` = `menu.file.open`;
  `menu.network.edit` = `commands.serversEdit.label`.
- The `menu.select.same*` / `.allFolders` / `.noExtension` keys and their `commands.selectionSelectSameKind.*` twins
  change together; `med filtillägget *.{extension}`. Loading `Söker efter appar…`; `delningsalternativ` (from `Dela`).
- Justified same-as-source: `menu.view.zoom`, `menu.tag.orange`, `menu.view.askCmdr`.

## System SMB connection fallback (`fileExplorer.network.osMountFallback.*`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- native → `inbyggd` (❌ MS's `ursprunglig`); `systemanslutning` names it elsewhere; "network" is dropped from a heavy
  triple compound. Share-not-on-server is permanent: no `just nu` or `försök igen`; `säger sig inte ha` avoids two
  `den`.

## Require SMB encryption (`settings.network.requireSmbEncryption.*`, `*encryptionRequired*`, `*EncryptionRequired*`, `askCmdr.*providerBusy.*`)

- Label `Kräv alltid SMB-kryptering`; messages say `inställningen ”…” är aktiverad`, so `aktiverad` agrees with
  `inställningen`. Managed value `Krävs alltid` (like `Loggas alltid`).
- Busy provider: `har fullt upp` / `har för mycket att göra`, invariant, so nothing agrees with a brand of unknown
  gender.

## Mutation and volume errors (`errors.mutation.*`, `errors.volume.*`)

- Top folder `rotmapp` (Thunar, TC); System Integrity Protection stays (Finder `ET6`). No blame:
  `Ett namnbyte kan inte …`, ❌ `Du kan inte …`.
- `svarade inte i tid` (❌ `tidsgränsen nåddes`); "may still land" → `kan fortfarande gå igenom`; "that" →
  `den åtgärden`.
- `har ingen papperskorg`; `macOS nekade flytten till papperskorgen` names macOS, as English does.

## Crash dialog variants and the reports setting (`crashReporter.dialog.body.keptRunning`/`.unknown`, `settings.updates.crashReports.description`)

- `.keptRunning` / `.unknown` never say crashed, quit, or stopped, and say `en rapport`.
- `stötte på ett problem` keeps `Cmdr` as subject (❌ `råkade ut för`); kept running → `fortsatte köra` (AppKit), ❌
  `höll igång` / `fortsatte fungera`.
- All three share `förra gången`; if `.ended` changes, `När Cmdr senast kördes …` is the attested swap for all.

## Eject and disconnect errors (`errors.eject.*`, `fileExplorer.navigation.ejectBusyTooltip`, `fileExplorer.navigation.disconnectBusyTooltip`)

- Values follow a colon in `ejectFailedToast` / `disconnectFailedToast`: say why and what to do, never repeat
  `det gick inte att`.
- in use → `används` (Finder; `upptagen` is the menu marker); idle → `när den inte används`. removable → `borttagbar`
  (Finder), ❌ `flyttbar`. disk image → `skivavbild` / `avbilden`, ❌ `avbildning`.
- `timedOut` uses the noun `utmatningen`: `så den kan fortfarande matas ut` reads "you can still eject it".
- `unmountRefused*` share `X använder fortfarande något där.` + `… och försök sedan igen.`, never `den här enheten` or
  `mata ut igen` (share or phone). Bare active `{app}`, unquoted per `@key`; `Stäng appen` / `apparna`; Eject all →
  `Mata ut alla` (Finder `N199.1`); never `Vi`; `andra appar` vs bare `andra` for processes.

## Trash toast (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `menu.edit.undoTrash`)

- `lägga tillbaka` (Finder), ❌ `återställa` / `flytta tillbaka`. Undo → `Ångra flytt till papperskorgen` (MT9).
  `{skipped}` branches are identical (`objekt` doesn't inflect).

## Amending a sent error report (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- add to → `lägga till i` (`bifoga` is the email in the same dialog). `från Hjälp-menyn`, ❌ `menyn Hjälp` /
  `Hjälpmenyn`. `Visa rapporten eller lägg till en notering` keeps `notering`, tying it to the field.

## Selection dialog (`selection.*`)

- `selection.recent.applyAria` mirrors `search.recent.runAria`.

## One thing, one name: the term-drift audit (`menu.file.delete`, `commands.fileDelete.label`, `settings.mediaIndex.clip.*`, `commands.selectionSelectAll.label`, `commands.selectionDeselectAll.label`, `fileExplorer.errorPane.goHome`, `menu.select.all`, `menu.select.deselectAll`)

- F8 is `Radera` everywhere: `Radera` / `Radera permanent` differ in strength, `Ta bort` / `Radera permanent` don't. The
  AI model is `Radera`d; `settings.mediaIndex.reclaim.*` keeps `ta bort` (index rows).
- Bare `All` → `allt` (`Återställ allt till förval`); with a head noun it inflects (`Stäng övriga flikar`, Safari).
- Bare "Copied" → supine `Kopierat`. `Gå till hemmappen`; `mapp`, ❌ `kat.`.

## Same concept, different English (`commands.viewShowHidden.label`, `settings.fileViewer.suppressBinaryWarning.label`, `settings.fileExplorer.suppressQuickLookHint.label`)

- `Göm` has eleven Finder strings against zero `Dölj`; `Visa eller göm dolda filer` avoids `dölj dolda`.
  `mappstorlekar`, ❌ `katalogstorlekar`.

## Boundaries: both forms are right, don't flatten them (`updates.status.checking`, `ai.local.statusRunning`, `operationLog.status.running`, `shortcuts.section.filterModified`, `menu.bar.file`, `menu.bar.view`, `menu.bar.select`, `menu.view.zoom`, `menu.window.zoom`)

- `okänd` agrees with an implied en-word. `Ändrad` for one file's attribute, `Ändrade` on the chip over a set. File,
  View, Select, Zoom: title vs action, as § Native menus.

## Menu and palette twins, System Settings panes (`menu.app.hideOthers`, `commands.appHideOthers.label`, `menu.app.showAll`, `askCmdr.renameUndo.undone`/`.partial`)

- `Göm övriga` in menu and palette (three Apple bundles), ❌ `Göm andra`.
- Restored names: `Det gamla namnet återställdes … fil` / `De gamla namnen återställdes … filer` (both branches agree);
  trash stays `Lade tillbaka`.

## Old WebKit and old macOS screens (`main.oldWebkit.*`, `main.oldMacos.*`)

- `Programuppdatering` (❌ `Mjukvaruuppdatering`); `så gott det går`; look off → `se konstiga ut`. The last sentence is
  David speaking.

## Open terminal here (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `Öppna terminal här` (Finder `Öppna i Terminal`); the card title is justified same-as-source; `Välj app…` (Finder).

## Sort by relevance (`fileExplorer.columns.sortByRelevance`)

- `Sortera efter relevans`, indefinite like its siblings.

## Server hub: connection state, refusals, forget dialogs (`servers.paneState.*`, `servers.refusal.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`, `fileExplorer.network.browser.*Host*`, `servers.hub.forgetShare*`)

- trust → `lita på`, `betrodd` / `betrott` / `betrodda` (SecurityInterface). Host key: bare `nyckel`,
  `nyckeln från {host}`. Reconnect loop `arbetar på att …`.
- forget → `glöm`, ❌ `Ta bort`; dialogs copy the menu labels; `Glöm den delade mappen` mirrors `Glöm servern`.
  `slutar visa servern i listan`: `anslutningen` and `servern` are both en-words. A browser host is `servern` in prose
  (`värden` also means "the values").
- `{name}` takes no participle: `Servern {name} är sparad`, `Inte ansluten till {name}`.
- `Den här servern använder en nyckel …` (as `authMethodUnsupported`), ❌ `loggar in med`. `retryTotalSeconds` /
  `.retryTotalMinutes` are bare blocks for `retryKeepsTrying`.

## Server hub: table and statuses (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`.serverUnpinnedToast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- Status cells agree with `servern` (`Ansluten`, `Sparad`, `Hittad i närheten`, `Utloggad`); a gender change rewrites
  the column. `Platser` is broader than `delade mappar` (buckets too). nearby → `i närheten`, ❌ `upptäckt`.
- `Servern är fortfarande sparad` names the noun (`volymväljaren` is en too). `som {username}` / `som gäst`;
  `Använd gäståtkomst` (❌ `Använd gäst`, a person). Picking a server → `Välj`, ❌ `Markera`.

## Server hub: connect sheet, host key, root and start folder (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`, `servers.paneState.reconnecting`/`.signedOutNothingToAsk`)

- `lösenfras` (❌ `lösenordsfras`, an OID name); `Nyckelns lösenfras` / `Nyckelns fingeravtryck` as a pair.
- `lita på` needs an object: `Lita på nyckeln och anslut`, `Jag har kontrollerat nyckeln`.
- `Mapp på servern` (❌ `fjärrmapp`). `Lämna fältet tomt`: `Lämna den tom` after `Mappen …` means an empty folder.
- `ditt konto får läsa den`; the `*NotFound` siblings share a frame. `accountLocked` says `identifierar` (the sheet has
  a `Namn` field). `Öppna servern igen för att försöka på nytt` names the noun.
- NFS export → `export` (`exporten`), like Swedish NAS guides.

## Pinned servers, trusted host keys, Android settings rows (`menu.network.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.fileOperations.adb*`)

- `Fäst i volymväljaren` / `Lossa från volymväljaren` (the server stays listed). UI names by apposition
  (`listan Servrar`). Got it → `Uppfattat` (`OK` is a default button). `värdnyckel` only for "host key".
- `Hittades: {path}` (the path is the binary). Re-check `Leta igen` (ties to `Cmdr letar efter adb …`). Picker
  `Välj kommandot adb`, its button `Bläddra…`. `Sökväg till adb`, ❌ `Plats`. `platform tools` stays (tentative).

## Android over ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`, `fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- Phone `tryck på`, Mac `klicka`. Reseat → `dra ur och sätt i kabeln igen`.
- `Koppla från {name}` matches the server's `disconnectPlaceAriaLabel`. `Du stoppade öppnandet av din telefon` (❌
  `avbröt`, the Cancel button). How → `Hur?` (tentative). The row says `har USB-felsökning aktiverad`.
- The stale index uses `telefonens` and passive `görs` (no subject fits).

## Dock offer (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- Apple's surface says `Behåll i Dock` / `Ta bort från Dock`, ❌ Cmdr's `fäst` / `lossa`; `dra … från mappen Appar`.
- profile → `styrs av`, person or org → `hanteras av` (MDM). `Cmdrs symbol är på plats, men Dock startade inte om`: the
  icon IS there. `Nej tack`, ❌ `Inte nu` (no later ask). No number for a movable threshold.

## Show in Finder offer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `”Visa i Finder”` quoted as the card; ❌ `tyvärr`. `notProductionBuild`: `släppt version` /
  `utvecklings- och testversioner`, unlike `instanceLock`'s `kopia`.

## AI provider setup (`onboarding.cloudSetup.*`)

- deployment → `distribution`, endpoint → `slutpunkt` (MS).

## Onboarding rewrite

### Beta-step checklist (`onboarding.stepBeta.checklist.*`)

- `Checklista för att komma igång`; bare `each` → `varje punkt`. `stjärnmärk`, `Gilla`; `repot` (neuter). An empty
  `<field></field>` / `<alpha>` / `<chip>` sits where Swedish wants the object.

### Mailing-list signup (`onboarding.stepBeta.signup.*`)

- `e-postlista` (❌ `distributionslista`); typo `stavfel` (TC's `Skrivfel!` is Write error). `rejected` / `.unreachable`
  quote `checklist.emailSave` verbatim.

### AI step (`onboarding.stepAi.*`)

- `dummare` stays blunt (`@key`). `local.tooltip` quotes `cloud.label` (`Ja, jag vill ha AI`). `<em>och</em>` without
  `både`. `strunta i`; badge → `märke`, ❌ `aktivitetsikon`.

### Optional-step summaries (`onboarding.stepOptional.*`)

- Near English length (they sit beside switches). `macOS inbyggda hanterare`; suppress a process → `håller tillbaka`.

## Viewer fetches first (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Heading like Finder's `Förbereder kopiering av ”^1”`; `för förhandsvisning` needs no pronoun. Stopped arriving →
  `Hämtningen av den här filen står stilla`.

## Mount and share-list errors (`errors.mount.*`, `errors.shareList.*`)

- `den här datorn` (also on Linux); `försök på nytt när servern är online igen`. The `hostUnreachable` and `authFailed`
  pairs stay identical.

## Text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `textredigerare` (shared stem, ❌ `textredigeringsprogram`; tentative). Shared strings copy the terminal picker.

## Drive disconnected mid-work (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`, `errors.write.moveNotConfirmed.*`, `fileOperations.leftovers.stagingFolderKept`)

- `håller på att kopplas från` / `kopplades från`. Sided messages end on WHERE the files are:
  `Dina original är orörda där de låg`, `så ingenting har gått förlorat`; `dit` avoids `{volumeName}`'s gender.
- Unconfirmed moves: `hade sparats på` (❌ `skrevs till`), `så dina original ligger kvar där de låg`,
  `Titta i målmappen`.
- Staging folder `avbruten` (vs `stagedLeftover`'s `ofullständig`); `lät dem ligga kvar` (Cmdr chose). ❌ Never suggest
  deleting it: they may be the only copies.

## Favorites menu (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- Scope heading `Favoritmeny`, prose `favoritmenyn`. See and Show are one action: `Visa`.
- `favoritesAddCurrent` is the one `0`-row key the list quotes. Number keys `siffra`. `favoritesCantAddHere`:
  `pekar på mappar som finns på …`.

## Title-bar full disk access badge and the trash refusal dialog (`onboarding.fdaBadge.*`, `errors.write.trashRefused.*`)

- `fdaBadge.label` and `stepAi.bannerTitle.denied` share one English. The aria opens with the label (❌ definite
  `skivtillgången`). `märket`, ❌ `statussymbol` (a file-row overlay); no macOS feature name for "somewhere …".
- The five trash titles share one English; `{count} av objekten du valde` fits any number.

## Online-only warning (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Keep all four facts (trashing downloads them; only the WHOLE selection; no trash copy but the service keeps one, ❌
  don't play it down; the ways out), the `<strong>` spans, and `”Radera”` from `confirmDelete`. Prose `bara online`.

## Look-alike names (`errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`, `fileOperations.transferProgress.lookAlikeHint`)

- `stavar dem olika`, `stora och små bokstäver` (Finder), ❌ `skiftlägeskänslig`; `Skriv över ersätter …` unquoted.

## Escape leaves full screen (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `lämna helskärmsläge` (AppKit), `tog Cmdr ur helskärmsläget`; `Escape` in prose, `ESC` on the keycap; `switchLabel`
  equals the settings label.

## Flytten vars original låg kvar (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

- Cmdr's step `ta bort originalet`; the user's `Radera originalet själv`. `Ingenting gick förlorat` differs from
  `originalsKeptAside`'s `Ingenting slängdes` on purpose.

## Range editor (`multiRename.rangeEditor.*`)

- Ordinals are `#:a` / `#:e` from the `selectordinal` category; from the end, `sista` (=1) and `näst sista` (=2), else
  `det 3:e sista tecknet`. Edges `Början` / `Slut`; grandparent `Överordnad mapp` beside the parent's `Mapp`.
