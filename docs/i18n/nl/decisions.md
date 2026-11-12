# nl decisions

Distilled rulings behind `terms.json` ("X over Y because Z"), each heading citing its keys so `pnpm i18n:brief` can pull
it. General rules live in `style.md`, terms in `terms.json`; neither is repeated here.

## Instellingssecties en kaartnamen (`settings.section.*`, `settings.summary.navigationAndFileOps`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `settings.appearance.showFunctionKeyBar.label`, `fileExplorer.functionKeyBar.hiddenToast`)

- Sections, identical wherever cited: Weergave, Gedrag, Bestandsbewerkingen, Bestandssysteem volgen (tentative), Zoeken,
  Bestandssystemen, SMB-/netwerkshares, MTP (Android/Kindle/camera's), Weergavevenster (tentative), Ontwikkelaar,
  Logboek, Updates en privacy, Geavanceerd, Sneltoetsen, Licentie, Kleuren en notaties, Zoom en dichtheid, Bestands- en
  mapgroottes, Lijst (tentative), Navigatie. "&" → `en`.
- `Navigatie en bewerkingen` mirrors EN's clipped "ops"; the card keeps `Bestandsbewerkingen`.
- `de paneelachtergrond`, explained as `de lege ruimte rondom de bestandenlijst`; `functietoetsbalk` everywhere.

## Voortgang van kopiëren en verplaatsen (`transfer.split.*`, `transfer.fileOnly.*`, `fileOperations.transferProgress.titleActive`, `queue.row.label`, `queue.failureToast.title`, `errors.write.filesTooLargeForFilesystem.*`)

- Participle last (`{phrase} gekopieerd`). Row arms `Bezig met naam wijzigen` / `map aanmaken` / `bestand aanmaken`;
  `failureToast.title` drops `Bezig met` and appends `niet voltooid`, so the two move together.
- File size → `te groot` (`te lang` is for names).

## De bewerkingenwachtrij (`queue.*`, `commands.queueShow.*`, `commands.queueMove*`, `fileOperations.transferProgress.queuedToast`, `.queuedToastCount`, `.background`, `.backgroundAria`)

- `Bewerkingenwachtrij` pairs with `Bewerkingenlogboek`; `queueShow.label` is the window name. Row aria `deze bewerking`
  (a row can be a delete); `pauseAria` keeps `deze overdracht`.
- Reorder `schuif … naar voren` / `naar achteren`, edges `vooraan` / `achteraan`, over `Verplaats` (F6) and `regel`.
- Dismiss → `Sluit` (… `deze bewerking`, `alles`) over `Wis` and `Verwijder uit lijst` (a delete verb on delete rows).
- `queuedToastCount` holds the verb (`gaat # bewerking` / `gaan # bewerkingen`) for `Er {countText} deze voor, …`. ⚠️
  One unit: never re-translate half.
- Spoken "percent" → `procent`; `chip.tooltip` gives the count its own `·` slot (the trash and `Bezig` arms take no
  `van`).
- `Op de achtergrond` over `Achtergrond` (a backdrop) and `Naar achtergrond` (window stacking); its aria
  `Op de achtergrond laten doorlopen` contains it, so reshape both.

## Het losse conflictvenster (`fileOperations.operationConflict.*`)

`Bezig met kopiëren naar X` matches the queue row (Finder's verb-final form fits only keys without one); an object goes
first (`Bezig met {destination} bewerken`); `totdat je antwoordt` (`reageren` is for devices).

## Vastgelopen overdracht (`fileOperations.transferProgress.stall*`, `.close`)

`Al {duration} geen voortgang` over bureaucratic `gedurende`; `Wachten tot X reageert`; `komt niet meer vooruit`
(tentative) over `ligt stil` (too near `Gepauzeerd`); `het logbestand`, since `logboek` names the feature.

## Stoppen terwijl er werk loopt (`main.quit.*`)

- Title after Finder `A17`/`A19`; `one` arm `een bewerking wordt` (`1 bewerking` reads like a tally).
- `opruimen` over `verwijdert`; `Wat al klaar is, blijft klaar.` over `blijft staan` (wrong for a delete).
- `Werk door` (tentative) over `Annuleer` (cancels the work), `Later`, and `Behoud` (keep-file); `herstarten` /
  `uitloggen` (macOS) over `opnieuw opstarten` / `afmelden`.

## Bewerkingenlogboek (`operationLog.*`, `commands.logOperationLog.*`)

`Naam van {countText} onderdeel gewijzigd` (Finder); lifecycle words from `queue.row.status`; You → `Jij`; `vastgelegd`
(tentative); `Niet terug te draaien`; `Terugdraaibewerking` over `Terugdraaiing`.

## Terugdraaien (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `queue.row.reversalInFolder`, `transferProgress.foregroundBusyToast`, `fileOperations.transferProgress.rollbackTooltip`/`.rollbackTooltipStopAndMoveBack`/`.rollbackAlreadyLandedTooltip`, `operationLog.dialog.rollBackRest`, `operationLog.rollback.partiallyRolledBackNotice`)

- "Leave alone" → `ongemoeid laten` (as `askCmdr.renameUndo.skipReason.*`), never `overgeslagen`. ⚠️
  `reason.folderNotEmpty.named`/`.counted` stay byte-identical to their `skipReason` twins.
- Reasons take no pronoun (`daar is iets aan gewijzigd`, `nadat Cmdr er klaar mee was`), so `.named` and `.counted`
  share a sentence; `controleren`, never `nagaan`.
- `Alles is teruggezet:` + count; `one` arm `Het onderdeel is teruggezet.` A move is put back, never deleted
  (`Stop en zet elk bestand terug …`); `verwijderen` and `terugzetten` never mix.
- `stagedLeftover.*`: `onvolledig exemplaar`, `Cmdr ruimt het op`, retry
  `de volgende keer dat het verbinding maakt met de bestemming`, never a time.
- `Terugdraaien` equals the opening button (`conflictRollback`); `Behoud de bestanden` (`ze` could mean the replaced
  ones). `Voltooi terugdraaien` (Finder `NE108`) over open-ended `Verder`. ⚠️ Both `finishRollBack` keys identical.
- `Antwoord nodig` over `Wachtend op reactie` (starts like `Wachten`); `Hier is iets anders open.` (the blocker may not
  be an operation); `in {folder}` unquoted.

## Prullenmand en terugzetten (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `fileOperations.delete.trashSwitch`, `.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `.targetGroupTitle`)

- `Zet terug` (Finder `N153.1`) over `Herstel` (also Revert, Repair) and `Ongedaan maken` (too long); `terugzetten` /
  `teruggezet` throughout (`PE130_V2`).
- `Ga naar prullenmand` goes there; `Naar prullenmand` moves files (switch and confirm button share it).
- Headings `Van` / `Naar` (Total/Double Commander); `bron` / `bestemming` for controls. `undonePartial` puts the verb in
  the `{skipped}` branches.

## Alleen-online inhoud (`fileOperations.delete.cloudOnlineOnly*`)

The download is a consequence (`Als je die onderdelen naar de prullenmand verplaatst, worden ze eerst gedownload`). Keep
all four facts (trash downloads, whole-selection delete only, the service keeps its own copy, the ways out).

## Prullenmandweigeringen (`errors.mutation.trash*`, `errors.write.trashRefused.*`, `errors.write.*.title.trash`)

- `Dit volume heeft geen prullenmand`; `dus definitief verwijderen is de enige manier` (no gender guess);
  `macOS wilde dit niet …`.
- The five titles share one string: reword all or none. `{count} van de onderdelen die je koos` fits any count.
- `suggestion.other` names `fileOperations.errorDialog.technicalDetails` and quotes `onboarding.fdaBadge.label`.

## Naam wijzigen en aanmaken geweigerd (`errors.mutation.*`, `errors.volume.*`, `fileOperations.validation.nameNotUsable`, `fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- `de beveiliging opheffen` (Finder `NE17`, `BN43`); `de hoofdmap van een volume`; `doelmap` (Finder) over
  `bestemmingsmap`. `Er staat niets meer op ‘{path}’.` / `Er staat al iets op ‘{path}’.` mirror each other.
- A timeout: `de wijziging gaat misschien toch nog door`. MTP restart: `heeft zijn verbinding opnieuw gestart`.
- `Deze bestandsnaam kan niet worden gebruikt` (Finder `RN31`), no period: it's inserted before
  `‘{name}’ behoudt zijn naam.` Chain toast: `Eén ander bestand` (Finder `MR101_V2`), `net als ‘{name}’: {reason}`.

## Toon info en Beveiligd (`errors.write.fileLocked.*`, `errors.write.permissionDenied.suggestion.delete`)

`deselecteer ‘Beveiligd’` is Finder's advice (`NE18`); `Vergrendeld` stays for other locks (the git index lock).

## Verplaatsen dat niet afkwam (`errors.write.moveNotConfirmed.*`, `fileOperations.leftovers.stagingFolderKept`, `errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

- Never "failed": `Kon het verplaatsen niet bevestigen` (never `de verplaatsing`);
  `dus heeft het je originelen laten staan waar ze stonden`; `Kijk even op …`.
- Files back from an unfinished move are the user's: never suggest deleting the folder. `voltooid` for file operations,
  `afgerond` for scans; `een verborgen map met de naam {folderName}` (Finder, never `genaamd`).
- `het origineel staat er nog`, `op beide plekken`.

## Een schijf die verdwijnt (`errors.write.deviceDisconnected.sided.*`, `indexing.needsFreshScan.afterDisconnect`, `fileExplorer.navigation.driveIndex.driveLeaving`)

- End on where the files are, in a main clause (`Je originelen zijn onaangeroerd`); `{done} van {total} bestanden`,
  never `van de`; `er is niets verloren gegaan` (Apple); `nadat Cmdr er … naartoe had gekopieerd`.
- In progress `wordt losgekoppeld`, gone `werd losgekoppeld`; `helemaal opnieuw`.

## Uitwerpen (`errors.eject.*`)

- Values follow the toast's colon: continue, don't repeat its verb (except `notAnSmbVolume`). Catch-all =
  `errors.mutation.unexpected`.
- Never `verwijderbaar` (reads as deletable): `Deze schijf kun je niet uitwerpen`.
  `Sluit open bestanden, stop geopende apps` (Finder); `wilde … niet`; `koppel het los`.
- Holders: `X gebruikt daar nog iets.` + `… en probeer het daarna opnieuw.`; `daar`, since it may be a share or a phone.
  `Sluit de app` / `die apps`, no pronoun for an insert; Eject all → `Werp alle uit`, since Finder's `Verwijder alle`
  (`N199.1`) reads as delete; `Cmdr heeft geen appnaam`, never `We`.
- `andere apps`, `nog andere` (`andere processen` reads as a name), `schijfkopie` (repeated), `staat nog open`
  (tentative); `Wacht een minuutje` (macOS) vs `Wacht even` (Cmdr).

## Archieven (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`, `fileOperations.archivePassword.*`, `errors.listing.archiveNeedsPassword.explanation`, `commands.fileCompress.*`, `fileOperations.transferDialog.*`)

- `Blader als een map` vs `Open met standaardapp`; `elk formaat` (`structuur` is for `archiefstructuur`).
- `Alleen-lezenarchief` / `-volume` / `-map` close up, `is alleen-lezen` stays open, `Alleen-lezen git-portaal` avoids a
  double hyphen.
- `… is beveiligd met een wachtwoord` (Total/Double Commander), `Ontgrendelen` (AppKit); contents are `vergrendeld`.
- `Comprimeer` everywhere; slider `Sneller` / `Kleiner` (Total Commander). OOXML row: bare `pakketten`, broader than
  `App-pakketten`, as EN splits.

## Klembord (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`, `fileExplorer.clipboard.copiedPath`)

`… als {filename} geplakt`; `Klembord-PDF` sits whole in each select branch; options `Niets doen`, `Bestand aanmaken`,
`Aanmaken en naam wijzigen`. `Pad gekopieerd, het staat nu op het klembord:` reads complete without the path below.

## Schijfindex (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `indexing.staleDialog.titlePhone`/`.bodyPhone`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`, `fileExplorer.navigation.driveIndex.*`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- `Controleren op wijzigingen` (Nautilus), `Bestandenlijst bijwerken`, `de controle die nu bezig is`.
- `Eerste volledige scan`, `Volledige herscan` (tentative), `Snelle update`; `nog 20 uur`;
  `het doorzoeken van de schijf`.
- Paths quote the label (`Zet het aan bij Indexeren > Schijf indexeren.`); terse slots compound
  (`Uit met schijfindexering`).
- Stale phone: `mogelijk verouderd`, `werkt de index bij` (`houdt bij` = keeps a record); never `losgekoppeld`.

## Afbeeldingen indexeren en doorzoeken (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

Labels `afbeelding`, warm rows `foto's`, as EN. In prose `het doorzoeken van afbeeldingen`; progress
`Afbeeldingen worden geïndexeerd` (the infinitive reads as the label). `Indexeren inschakelen`,
`Mappen om te indexeren`, `Foto's op beschrijving zoeken`, `het model voor semantisch zoeken`.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`, `ai.cloudConsent.askCmdr.*`)

- Tool lines: present while doing (`Kijkt in bestanden`), participle when done (`In bestanden gekeken`, tentative), one
  verb per tool. `limiet`, never "budget"; `Zet hier neer om bij te voegen`.- `miniatuur`, `cameragegevens` (tentative),
  `PDF-pagina's`, `titel en auteur` (MS `functie` is a job). `nooit hele bestanden` keeps the promise true.
- `Wijzig # bestandsnaam` fits the button; `cyclus van naamwijzigingen`; `van naam wisselen` over mechanical `roteren`;
  recent-past status in the perfect.

## AI-suggesties (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

`AI-suggesties` over `Voorgestelde bewerkingen`; aria `Waarom, volgens de AI:` holds `Waarom:`. `goedkeuren` over macOS
`Accepteer` (granting).

## Cloud-AI toestaan (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

In a sentence `Sta cloud-AI toe`; `cloud-AI` is a de-word. `Zet Ask Cmdr aan`; the bare name label is justified.

## Gebruiksstatistieken (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

A stable random id, so never `anoniem` or `pseudoniem`: `een willekeurige id` (MS), `gekoppeld aan`. Label and title
share `Gebruiksstatistieken versturen`.

## Crashrapporten (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`, `settings.updates.crashReports.description`)

`.keptRunning` / `.unknown` never say Cmdr crashed; `een probleem tegengekomen` (tentative) over
`een fout aangetroffen`; `is gewoon blijven werken`. The description says `een rapport` (crashes and background
problems).

## Een foutrapport aanvullen (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

`Toevoegen aan je foutrapport`; `notitie`, as `dialog.noteLabel`; `Bekijk het rapport of voeg notities toe`.

## Selecteren (`selection.*`, `menu.select.*`, `commands.selectionDeselectFiles.label`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `Selecteer` / `Deselecteer` (Finder `NE18`) over `selectie opheffen`; `Deselecteer alles` over Finder's
  `Maak selectie ongedaan` (no object, no rhyme). The title matches its menu item; the key is `Enter`.
- `Soort` (Finder); `Selecteer alles met extensie *.{extension}`; `extensie` over `achtervoegsel`; `Selectie` is the
  noun. Menu/command twins share one string: reword both.

## Eén ding, één naam (`queue.row.dismiss`, `menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.*`, `shortcuts.section.filterModified`, `commands.navBack.label`)

Not drift: `Vorige`/`Volgende` vs a lone `Terug` (`navBack` → Finder's `Ga terug`); `Aanmaakdatum`/`Bewerkingsdatum` vs
tooltip `Laatst gewijzigd` vs `Gewijzigd` (changed shortcuts); `Actief` (server) vs `Bezig` (task); `Gereed` (status) vs
`Klaar` (after a step); `Selecteer` (files) vs `Kies` (option); `Zoek` (button) vs `Zoeken` (feature).

## Native menu's (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, `commands.fileDuplicate.*`, `fileExplorer.columns.sortByRelevance`)

- Finder also gives `Bovenliggende map`, `Thuismap`, `Sorteer op`, `Minimaliseer`, `Toon alles` / `Verberg andere`,
  `Dupliceer` (`N154`), `Voeg ‘{color}’ toe` / `Verwijder ‘{color}’` (`TG5`/`TG6`), and
  `Rood, Oranje, Geel, Groen, Blauw, Paars, Grijs`.
- `Werp uit` (Nautilus, Thunar, Double Commander) because macOS's `Verwijder` is Cmdr's delete.
- MS: `Wijzigingenlogboek` (distinct from `Wat is er nieuw`), `Tekstterugloop`. `Apps zoeken…`, `Geen deelopties`,
  `Sorteer op relevantie`.

## Het Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`, `menu.dock.*`)

`vastzetten` / `losmaken` over `Permanent in Dock`; `een paar dagen`; `Wie deze Mac beheert, kan dat aanpassen.` Dock
menu from `DockMenus.strings`: `Open Cmdr`, `Ga naar map…`, `Verbind met server…`, `Zoek bestanden…`.

## Het Toon in Finder-aanbod (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

`‘Toon in Finder’` quoted; `al een tijdje`; the notice explains, never apologizes.

## Oude WebKit en oude macOS (`main.oldWebkit.*`, `main.oldMacos.*`)

`Software-update`, `Stop`; `ondersteund`, `X en nieuwer`, `doet het zijn best zonder garanties`, `er raar uitzien`; the
last line is David speaking.

## Escape en de schermvullende weergave (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

`Schermvullende weergave uitschakelen met Escape` (AppKit's `uitschakelen` for Exit); the notice reuses it verbatim.

## Rechtgezet Engels (`askCmdr.renameUndo.undone`/`.partial`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`)

`undone` names its object, so the whole sentence sits in the plural branches. Panes use Apple's Dutch (`Algemeen`,
`Inlogonderdelen en extensies`); `Apple Account` stays English as a pane, `Apple-account` in prose.

## Onboarding (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

Never `ster` (a rating) or `vind ik leuk` (Facebook); the Star and Like rows share a shape. `checklist` over
`controlelijst`; `mailinglijst` over `adressenlijst`; `superprivé`; `dev- en testbuilds`; `‘Lokaal netwerk’` is the pane
row, never `Lokale netwerktoegang`.

## Terminal en teksteditor (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`/`.textEditor`, `settings.behavior.textEditorApp.*`, `fileExplorer.edit.appMissing`/`.launchRefused`)

`terminal` / `Terminal` are one word (Finder `N67`), so the card is justified; `Terminal hier openen` everywhere;
`Kies app…` (`N137`). `teksteditor` (MS), never TextEdit; `standaardeditor`; `Bewerk bestanden in {app}`.

## Het favorietenmenu (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

`favorietenmenu`; `Toon favorieten` twins; `Voeg huidige map aan favorieten toe`. `nummer` names a favorite, `cijfer`
the key. `favoritesCantAddHere`: reason after a colon, `netwerkshares` (`gedeelde mappen` would repeat `mappen`).

## De badge voor volledige schijftoegang (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

`Geen volledige schijftoegang` (System Settings `ALL_FILES`), shared by `bannerTitle.denied`;
`bestanden die macOS voor zichzelf houdt`, ❌ never a feature name.

## Namen die er hetzelfde uitzien (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.volume.ambiguousName`, `ai.secretError.body`)

`onderdeel` even in Keychain Access (`Alle onderdelen`); `item` only for non-files. `slaat ze anders op` over `spellen`;
`Met ‘Overschrijf’ vervang je …`.

## Het weergavevenster haalt op (`viewer.pull.*`, `viewer.error.stoppedResponding`)

`{fileName} ophalen om te bekijken` (Finder `Ophalen…`), `tot nu toe`; no `komt niet meer vooruit` (no transfer shown).

## Servers (`servers.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`, `shortcuts.scope.servers`/`.places`, `goToPath.dialog.opensServer`/`.addsServer`, `menu.network.open`/`.edit`)

- macOS: `Verbinden met {name}…` (Finder `MN1`, unquoted), `Verbind als gast`, `Blader…` (not Safari's `Bladeren`),
  `Log in bij {name}` (CloudKit), `Uitgelogd bij {name}`, `Onthoud in Sleutelhanger`, `Sleutelhangertoegang`.
- `Locaties` (Finder sidebar); `Type`, never `Soort`; `Gevonden in de buurt` (tentative).
- `wachtzin` (Apple), never `wachtwoordzin` (`Sleutelwachtzin` tentative); `vingerafdruk`; EN "key" → `de sleutel`,
  "host key" → `serversleutel` (tentative, over `hostsleutel`).
- `extern`; `hoofdmap` (MS, Dolphin) over `rootmap`; `beginmap` (tentative, Double Commander `Beginpad`) over `startmap`
  (Start menu); `mag lezen` for a permission refusal.
- Forget: title = menu item, `{name} vergeten?`, `Vergeet`; `haalt … uit de lijst`; `Cmdr koppelt niets los`.
- `Gebruik gasttoegang` (tentative; `Gebruik gast` uses a person); `Verbreek de verbinding met {name}`.
- `Cmdr verbindt niet meer met {name}` (present: the pane waits) vs `Cmdr maakt geen verbinding met {name}` (never
  worked). `Opnieuw verbinden met {name}…`; `Open de server opnieuw` (`hem` could mean `de sleutel`);
  `er valt niets te typen` (tentative); `retryTotal*` are bare blocks.
- Locked identity: `bepalen welke … dit is` (the form has a `Naam` field), actions named as buttons; `Voeg toch toe`
  quoted; `er valt niets te wijzigen`. `menu.network.open` = `menu.file.open`; `Wijzig server…` = its command. Coined
  (tentative): `Sleutelbestand`, `Manier van verbinden`, `de servereigenaar`.

## NFS-exports en ID-velden (`servers.sheet.nfs*`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`, `servers.refusal.noNfsV3`, `stateRecovery.nfsPlaces.title`)

`export` (tentative loanword) over `gedeelde map`, which would blur it with the SMB share in the same form.
`bevoorrechte poort` and the quoted switch follow Synology nl; `insecure` stays English. `gebruikers-ID` / `groeps-ID`;
`Afgeleid van` over `Geraden` (sounds random).

## Vastzetten, serversleutels en ADB-instellingen (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

`Maak vast in volumekiezer` (Apple's order); macOS `Controleer opnieuw`, `Niet gevonden`, `Vertrouwde <ding>`,
`Blader op <plek>`; `Gevonden: {path}` (tentative) dodges `op` vs `in`; `pinHint.body` and `adb.install.intro` quote
labels.

## Het telefoonpaneel via ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

AOSP: `USB-foutopsporing` (`SettingsLib` `enable_adb`), `‘Toestaan’` (SystemUI `usb_debugging_allow`).
`De verbinding tussen Cmdr en je telefoon is verbroken.` keeps the brand; `Verbreek`, never `Werp uit` (still cabled);
`Je hebt het openen van je telefoon gestopt.` over `geannuleerd`; never name a diagnosis.

## Gedeelde mappen die niet aankoppelen (`errors.mount.*`, `errors.shareList.*`, `servers.refusal.accountNotPermitted`, `fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

`gedeelde map` in the pane, `netwerkshare` for "isn't a network share"; `gasten` (NetAuthAgent);
`Controleer of de server aanstaat`; twins match. Not on server: `Dit lost zichzelf niet op`, no retry; `die` for
`deze gedeelde map`.

## SMB-terugval (`fileExplorer.network.osMountFallback.*`)

`systeemeigen` (MS) to introduce, `de systeemverbinding` once SMB is clear; `4x zo langzaam als` (tentative, no pile
precedent); `Probeer direct te verbinden` echoes `Verbind direct`.

## SMB-versleuteling vereisen (`settings.network.requireSmbEncryption.*`, `*encryptionRequired*`, `*EncryptionRequired*`)

Label `Altijd SMB-versleuteling vereisen` (`vereist` is the catalog's verb, 15+ keys); elsewhere quote it plus
`staat aan` / `aanstaat` (the `… staat uit` pattern), never paraphrase "is set to". Managed value `Altijd vereist`.

## S3 (`servers.sheet.s3*`, `servers.refusal.s3*`, `*ShareLink*`, `*.coldStorage*`, `fileOperations.s3Cost.*`)

MS `bucket`, `regio`, `toegangssleutel-ID`, `geheime toegangssleutel`, `catalogusprijs`; `deellink`; `gearchiveerd`,
`koude opslag`; restore → `herstellen` (`terugzetten` is put-back).

## Wijzig meerdere namen (`multiRename.title`, `commands.fileMultiRename.label`, `menu.file.multiRename`)

Over TC `Uitgebreid hernoemen` (Finder `Wijzig naam van ^0 onderdelen…`). TC `Vervang door`, `Teller`, `Stapgrootte`,
`Cijfers`; MS `Hoofdlettergebruik`; `<label> aan/uit`; `tijdstip` (`de hele tijd` = constantly); `tekstcursor`.

## Range editor (`multiRename.rangeEditor.*`)

- `multiRename.rangeEditor.*`: ordinals `#e` (CLDR nl ordinal is `other` only); counted from the end →
  `#e vanaf het einde` (TC `Meet het eerste teken vanaf het einde`), never `#e-laatste`; edges `Begin`/`Einde` (counter:
  `Beginnummer`).
- `multiRename.mediaField.*`: Spotlight's `nl.lproj/schema.strings` names (`Opnamedatum`, `Tracknummer`, `Richting`,
  `Opmerking`), except focal length → `Brandpuntsafstand` (Apple's `Scherptediepte` is depth of field; its own ShortName
  says `brandpuntsafstand`); song → `nummer`; file metadata → `gegevens`; date format → `notatie`.
