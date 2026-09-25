# nl decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each heading citing its keys in backticks so
`pnpm i18n:brief` can pull it. Term rulings live in `terms.json`, typography in `mechanics.json`, voice in `style.md`,
open questions in `review-queue.md`.

## Instellingssecties en kaartnamen (`settings.section.*`, `settings.summary.navigationAndFileOps`, `settings.behavior.doubleClickPaneNavigatesToParent.*`)

- Section names, identical in every file that cites them: Weergave, Gedrag, Bestandsbewerkingen, Bestandssysteem volgen
  (tentative), Zoeken, Bestandssystemen, SMB-/netwerkshares, MTP (Android/Kindle/camera's), Weergavevenster (tentative),
  Ontwikkelaar, Logboek, Updates en privacy, Geavanceerd, Sneltoetsen, Licentie, Kleuren en notaties, Zoom en dichtheid,
  Bestands- en mapgroottes, Lijst (tentative), Navigatie. "&" → `en`.
- `settings.section.navigationAndFileOps` → `Navigatie en bewerkingen`: mirrors EN's clipped "ops"; the card keeps
  `Bestandsbewerkingen`.
- Double-click the pane background → `Dubbelklik op de paneelachtergrond …`, explained as "de lege ruimte rondom de
  bestandenlijst".

## De wachtrijmelding telt mee (`fileOperations.transferProgress.queuedToast`, `.queuedToastCount`)

The count fragment carries the finite verb (`gaat # bewerking` / `gaan # bewerkingen`) inside
`Er {countText} deze voor, …`, because the verb agrees with the count. ⚠️ One unit: never re-translate either half
alone.

## Voortgang en uitkomst van kopiëren en verplaatsen (`transfer.split.*`, `transfer.fileOnly.*`, `fileOperations.transferProgress.titleActive`, `queue.row.label`, `queue.failureToast.title`, `errors.write.filesTooLargeForFilesystem.*`)

- `transfer.split.*` put the participle last (`{phrase} gekopieerd`), Dutch word order over EN's leading verb.
- `queue.row.label` arms: `Bezig met naam wijzigen` / `map aanmaken` / `bestand aanmaken`; `queue.failureToast.title`
  strips `Bezig met` and appends `niet voltooid`, so the two keys move together.
- "Too large" for a file size → `te groot` (`te lang` is for names).

## Archieven bladeren en bewerken (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`)

- Browsing over opening: `Blader als een map` vs `Open met standaardapp`.
- Read-only titles close into one compound (`Alleen-lezenarchief`, `Alleen-lezenvolume`, `Alleen-lezenmap`); the
  predicate stays open (`is alleen-lezen`), and `Alleen-lezen git-portaal` stays open to avoid a double hyphen.
- "each format" → `elk formaat`; `structuur` is reserved for `archiefstructuur`.

## Klembordinhoud als bestand plakken (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- Toast order follows `clipboard.copied`: `… als {filename} geplakt`.
- `Klembord-PDF` keeps its hyphen, so the whole compound sits inside each select branch.
- Radio options are parallel infinitives: `Niets doen`, `Bestand aanmaken`, `Aanmaken en naam wijzigen`.

## Het archiefwachtwoord (`fileOperations.archivePassword.*`, `errors.listing.archiveNeedsPassword.explanation`)

`… is beveiligd met een wachtwoord` (Total/Double Commander), button `Ontgrendelen` (AppKit). The archive is
`beveiligd`, its contents `vergrendeld` until you `ontgrendelt` it; Finder's Locked flag is a different `beveiligd`.

## Comprimeren (`commands.fileCompress.*`, `fileOperations.transferDialog.*`, `settings.archives.compressionLevel.*`)

`Comprimeer` on every button and title verb. Slider ends `Sneller` (quicker packing) and `Kleiner` (smaller output),
after Total Commander's "snelste compressie" / "maximale compressie".

## Bewerkingenlogboek (`operationLog.*`, `commands.logOperationLog.*`)

- Rename summary `Naam van {countText} onderdeel gewijzigd` / `Namen van … gewijzigd` (Finder's "De naam van het
  onderdeel … gewijzigd").
- Lifecycle words reuse `queue.row.status` (`Wachten`, `Bezig`, `Gereed`, `Niet voltooid`, `Geannuleerd`).
- Initiator You → `Jij` (contrastive, standalone); recorded → `vastgelegd` (tentative); `Niet terug te draaien`.

## Ask Cmdr: chat, hulpmiddelen en kosten (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`)

- `askCmdr.tool.*` doing/done pairs: subjectless present for doing (`Controleert wat je bekijkt`), participle-led for
  done (`Grootste mappen gevonden`), one distinct verb per tool.
- `askCmdr.error.budgetExhausted` says `limiet`, never "budget".
- `askCmdr.composer.dropHint` → `Zet hier neer om bij te voegen`.

## Afbeeldingen indexeren en doorzoeken (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

- Technical labels say `afbeelding`; warm rows about a photo archive say `foto's`, mirroring EN's split.
- The feature in prose is `het doorzoeken van afbeeldingen`; the in-progress status is the passive
  `Afbeeldingen worden geïndexeerd`, because bare `Afbeeldingen indexeren` reads as the Settings label.
- Card titles `Indexeren inschakelen`, `Mappen om te indexeren` (friendlier than "Te indexeren mappen").
- Semantic search: `Foto's op beschrijving zoeken`, `zoeken op trefwoord`, `het model voor semantisch zoeken` (no
  compound).

## Indexeren: runsoorten en resterende tijd (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

- Run kinds `Eerste volledige scan`, `Volledige herscan` (tentative), `Snelle update`.
- Hour ETAs lead with `nog` and keep `uur` invariant (`nog 20 uur`).
- "The drive scan" in prose → `het doorzoeken van de schijf`.

## Naamwijzigingen beoordelen (`askCmdr.renameReview.*`, `askCmdr.renameUndo.*`)

- `askCmdr.renameReview.rename` → `Wijzig # bestandsnaam` / `bestandsnamen`, short enough for the primary button.
- Rename cycle → `cyclus van naamwijzigingen`; "while rotating" → `terwijl deze bestanden van naam wisselen` (`roteren`
  reads mechanical).
- Recent-past events in status prose take the perfect (macOS "is mogelijk verplaatst"), never the simple past.

## Het verwijdervenster: Van, Naar en de prullenmandschakelaar (`fileOperations.delete.trashSwitch`, `.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `.targetGroupTitle`)

- Switch and confirm button read as one pair: `Naar prullenmand`, as in `transferDialog.titleVerbOnly`.
- Headings `Van` / `Naar` (Total and Double Commander ship the pair in this dialog); `bron` / `bestemming` stay for the
  controls.

## Het indexeren van schijven staat uit (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.tooltipDisabled`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- "Drive indexing" in prose → `het indexeren van schijven`, never the label `Schijf indexeren`, which reads as an
  imperative and gives `Zet het aan` no antecedent.
- A navigation path quotes the label verbatim (`Zet het aan bij Indexeren > Schijf indexeren.`).
- Terse slots compound: `schijfindexering`, `beeldindexering`; `overriddenBadge` → `Uit met schijfindexering`.

## Schijfindex: de wijzigingscontrole (`indexing.run.*`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

`Controleren op wijzigingen` (Nautilus's "controleren op mediawijzigingen"), `Bestandenlijst bijwerken`, and
`de controle die nu bezig is`, reusing `controle` as the catalog's word for a full check.

## Vastgelopen overdracht: het stall-bericht (`fileOperations.transferProgress.stall*`, `.close`)

- `stallNotice` → `Al {duration} geen voortgang` over `Geen voortgang gedurende …`, which reads bureaucratic.
- `Wachten tot X reageert` (Finder's clause form); `reageren` stays reserved for a device or share answering.
- "Has stopped moving" → `komt niet meer vooruit` (tentative) over `ligt stil`, too close to the neighbouring
  `Gepauzeerd`.
- `stallLogHint` says `het logbestand`, never `logboek`, which names the Bewerkingenlogboek feature.

## Gekopieerd pad: de klembordbevestiging (`fileExplorer.clipboard.copiedPath`)

`Pad gekopieerd, het staat nu op het klembord:` must read complete without the path, which renders below it on its own
line.

## Operation queue: de hernoeming van Overdrachtswachtrij (`queue.*`, `commands.queueShow.*`)

- `Bewerkingenwachtrij` (with `-en-`) over `Bewerkingswachtrij` because it pairs with `Bewerkingenlogboek` in the same
  View menu block; `overdracht` keeps the narrow copy-or-move sense.
- `commands.queueShow.label` is exactly the window name, like `commands.logOperationLog.label`.
- Queue-row aria says `deze bewerking` (a row can be a delete); `transferProgress.pauseAria` keeps `deze overdracht`
  because its English says "transfer".

## De voortgangschip en het niet-voltooid-bericht (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- Dismiss → `Sluit` / `Sluit deze bewerking` / `Sluit alles`, over `Wis` (erase) and `Verwijder uit lijst` (leads with
  the delete verb on a row that may say "Bezig met verwijderen").
- Failure headline arms reuse the row label minus `Bezig met` plus `niet voltooid`; the count headline pulls the copula
  into the plural branches.
- Screen-reader "percent" → `procent`; the visual `%` takes no space.
- `queue.chip.tooltip` gives the count its own `·` slot, since the trash arm and the `Bezig` fallback take no
  `van`-object; the ` naar {destination}` clause fits after either.

## Het losse conflictvenster (`fileOperations.operationConflict.*`)

- A destination follows the infinitive (`Bezig met kopiëren naar X`) so the line matches its queue row verbatim;
  Finder's verb-final `'^1' naar '^2' kopiëren` models only keys without a queue-row sibling.
- A direct object precedes it (`Bezig met {destination} bewerken`).
- "until you answer" → `totdat je antwoordt`; never `reageren` (reserved for devices).

## De knop voor een lege wachtrij: "Background" (`fileOperations.transferProgress.background`, `.backgroundAria`)

- `Op de achtergrond` over bare `Achtergrond` (macOS uses it only for a backdrop) and `Naar achtergrond` (moving a
  window behind another); Dutch has no verb "achtergronden".
- The aria `Op de achtergrond laten doorlopen` starts with the label verbatim; re-shape both together.

## Het stoppoortje: afsluiten terwijl er nog werk loopt (`main.quit.*`)

- Title `Stoppen terwijl er nog {countText} bewerkingen worden uitgevoerd?`: Finder's `A17`/`A19` almost word for word;
  the `one` arm says `een bewerking wordt`, since `1 bewerking` reads like a tally.
- `opruimen` over `verwijdert`, which reads as more deleting next to "Bezig met verwijderen" rows.
- `Wat al klaar is, blijft klaar.` over `blijft staan`, which promises the opposite for a delete.
- `Werk door` (tentative) over `Annuleer` (cancels the operations, the opposite outcome), `Later` (the countdown is
  deleted), and `Behoud` (keep-this-file).
- Restart / log out → `herstarten` / `uitloggen` (macOS `Herstart`, `Log uit`), over Windows's `opnieuw opstarten` /
  `afmelden`.

## Usage stats: "anonieme" dropped, "een willekeurige id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- The stats carry a stable random id, so never `anoniem`, and never the jargon `pseudoniem`: `een willekeurige id` (MS),
  `gekoppeld aan`.
- Label and onboarding title are one English string, so both read `Gebruiksstatistieken sturen` (infinitive: a toggle
  label describes a setting).

## De terugdraaibevestiging en de rij die op antwoord wacht (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

- Row status `Antwoord nodig` over Double Commander's `Wachtend op reactie`, which starts like the queued arm `Wachten`.
- `rollbackConfirm.rollBack` → `Terugdraaien`, identical to the button that opened it
  (`transferProgress.conflictRollback`), over the imperative `Draai terug`.
- `rollbackConfirm.keep` → `Behoud de bestanden`: the object is spelled out, since `ze` could point at the replaced
  files the body just named.
- `foregroundBusyToast` never claims the blocker is an operation (`Hier is iets anders open.`).

## De keten-hernoemtoast die meetelt (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- `net als {othersText} andere bestanden` / `één ander bestand` (no `-e` on a neuter noun after `één`, as Finder's
  `MR101_V2`; its `PE106_V3` "andere onderdeel" is Apple's slip).
- Keeps the verb and tense of `chainKeptOriginalName`.

## De onbevestigde naamwijziging en de onbruikbare naam (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

- Never say the file kept its name: Cmdr doesn't know. `We konden … niet bevestigen` and the tail
  `dus de naam is misschien toch gewijzigd`, both from `fileOperations.mkdir.timeoutMessage`.
- `Deze bestandsnaam kan niet worden gebruikt` (Finder `RN31`), with no final period: it's also inserted before
  `‘{name}’ behoudt zijn naam.`

## Voorgestelde bewerkingen: het venster met wat Ask Cmdr voorstelt (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- Approve → `Goedkeuren` over macOS's `Accepteer`: the count variant grants permission, it doesn't take something in.
- Reject → `Weigeren` (Finder's AirDrop pair); infinitive, like the sibling buttons.
- `Dit kun je niet ongedaan maken` (Finder, verbatim).

## Dupliceer: de opdracht die in dezelfde map kopieert (`commands.fileDuplicate.*`)

`Dupliceer` (Finder `Archief > Dupliceer`, `N154`), imperative like its siblings `Kopieer` and `Verplaats`.

## Native menu's: menubalk, contextmenu's, venstertitels (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- The menu bar is Finder's: `Archief`, `Wijzig`, `Weergave`, `Ga`, `Venster`, `Help`, `Voorzieningen`; Finder Tier 1
  also gives `Geef snel weer`, `Toon info`, `Bovenliggende map`, `Thuismap`, `Sorteer op`, `Minimaliseer`,
  `Vergroot/verklein`.
- Eject → `Werp uit` (Nautilus, Thunar, Double Commander) over macOS's `Verwijder`, which is Cmdr's delete.
- `Wijzigingenlogboek` (changelog, MS) stays distinct from `Wat is er nieuw`; word wrap → `Tekstterugloop` (MS).
- Tag row: `Tags`, `Voeg ‘{color}’ toe`, `Verwijder ‘{color}’` (Finder `TG5`/`TG6`); colors
  `Rood, Oranje, Geel, Groen, Blauw, Paars, Grijs` (`TG_COLOR_*`).

## De terugvalmelding voor de systeem-SMB-verbinding (`fileExplorer.network.osMountFallback.*`)

- `systeemeigen` (MS, native) where the notice introduces the connection; the short `de systeemverbinding`
  (`smbNativeNote`) where context already says SMB.
- Speed multipliers `4x zo langzaam als` over `4x langzamer dan`, which leaves open whether the factor applies to the
  difference (tentative; no pile precedent).
- `Direct verbinden met X lukte niet`; button `Probeer direct te verbinden`, echoing `Verbind direct`.

## De weigerberichten van naam wijzigen, nieuwe map en nieuw bestand (`errors.mutation.*`, `errors.volume.*`)

- Locked → `beveiligd`, unlock → `de beveiliging opheffen` (Finder `NE17`, `BN43`); System Integrity Protection stays
  English, as Finder keeps it.
- A volume's top folder → `de hoofdmap van een volume` (Thunar, Dolphin).
- `Er staat niets meer op ‘{path}’.` / `Er staat al iets op ‘{path}’.` mirror each other.
- Destination folder → `doelmap` (Finder), over `bestemmingsmap`.
- A timeout never reads as done or refused: `de wijziging gaat misschien toch nog door`.
- A restarted MTP session `heeft zijn verbinding opnieuw gestart`; `losgekoppeld` is reserved for `deviceDisconnected`.

## De twee prullenmandweigeringen (`errors.mutation.trash*`)

- `Dit volume heeft geen prullenmand` follows EN's "has no", unlike the older `errors.write.trashNotSupported.message`.
- `dus definitief verwijderen is de enige manier`: the nominalized infinitive avoids a pronoun guessing the item's
  gender.
- `macOS wilde dit niet naar de prullenmand verplaatsen.`: active voice with `macOS` as subject; bare `dit` stays
  neutral about what was refused.

## De drie crashdialoog-openingen (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

- `.keptRunning` and `.unknown` must never say Cmdr crashed or stopped.
- `een probleem tegengekomen` (tentative) over Finder's stiff `een fout aangetroffen`.
- `is gewoon blijven werken` over `doorlopen`, reserved for an operation that keeps running.

## De instellingstekst voor rapporten dekt nu beide gevallen (`settings.updates.crashReports.description`)

The description covers crashes and background problems (`een rapport`, no "crash"); the label stays
`Crashrapporten sturen`, the setting's name. `appversie` is closed, as in `crashReporter.dialog.privacyNote`.

## Get Info en Beveiligd (macOS-UI-namen die Apple wél vertaalt) (`errors.write.fileLocked.*`, `errors.write.permissionDenied.suggestion.deleteMac`)

- `Toon info` and `Beveiligd` (Finder `InfoWindowGeneralView`, AppKit `AutosaveButton`); `deselecteer ‘Beveiligd’` is
  Finder's own recovery advice (`NE18`).
- `Vergrendeld` stays for other locks (the git index lock), never the Finder flag.

## De negen uitwerp- en verbreekzinnen (`errors.eject.*`)

- Each value follows a colon in `fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast`, so it reads as a
  continuation and doesn't repeat the wrapper's verb (except `notAnSmbVolume`, as EN does).
- Never `verwijderbaar` for "removable", although macOS uses it: next to eject it reads as deletable. `notEjectable`
  says `Deze schijf kun je niet uitwerpen`.
- `Sluit open bestanden, stop geopende apps` (Finder); `wilde … niet` for "wouldn't"; `koppel het los` for unplug.
- The catch-all is the same sentence as `errors.mutation.unexpected`.

## De twee knoppen van de prullenmandmelding en de terugzet-familie (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- Undo on the trash toast → `Zet terug` (Finder's Put Back, `N153.1`), over `Herstel` (macOS also uses it for Revert and
  Repair) and `Ongedaan maken` (too long beside the second button). Generic undo elsewhere stays `Ongedaan maken`.
- `terugzetten` / `teruggezet` for button, progress, and result (Finder `PE130_V2`).
- "Go to trash" → `Ga naar prullenmand` (Finder's `Ga naar map…` shape), never `Naar prullenmand`, which already means
  moving to the trash (`delete.trashSwitch`).
- `undonePartial` puts the verb in the `{skipped}` plural branches (`onderdeel bleef` / `onderdelen bleven`).

## Aanvullen wat al verstuurd is: het notitievenster bij een automatisch foutrapport (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- `Voeg aan rapport toe`: the particle goes last, as AppKit's `Voeg aan begin van knoppenbalk toe`.
- Window title in the infinitive (`Toevoegen aan je foutrapport`), parallel to `errorReporter.dialog.title`.
- Note → `notitie` over `bericht`, matching `dialog.noteLabel` so one field has one word.
- `Bekijk het rapport of voeg notities toe` keeps both halves; `Bekijk of vul het rapport aan` loses the notes.

## Het selectievenster: bestanden selecteren en deselecteren (`selection.*`, `menu.select.*`, `commands.selectionDeselectFiles.label`)

- `Selecteer` / `Deselecteer` (Finder `NE18`, Double Commander) over Microsoft's `selectie opheffen`.
- `Deselecteer alles` over Finder's `Maak selectie ongedaan`, which takes no object and doesn't rhyme with
  `Selecteer alles`; the dialog title matches the menu item that opens it.
- `selection.recent.applyAria` mirrors `search.recent.runAria` (`Pas recente {mode}-selectie toe: {query}`).
- The key is `Enter`, as in `search.runHint`.

## Eén ding, één naam: de interne driftronde (`queue.row.dismiss`, `menu.edit.undo`, `fileOperations.trash.undoAction`, `askCmdr.renameUndo.*`, `shortcuts.section.filterModified`, `commands.navBack.label`)

The boundaries that look like drift and aren't:

- Button or menu item: bare imperative (`Verstuur foutrapport`); window title: `Foutrapport versturen`; Settings label:
  infinitive last (`Verborgen bestanden tonen`).
- Menu-bar titles are Apple's words (`Archief`, `Wijzig`, `Weergave`, `Vergroot/verklein`); the everyday word
  (`Bestand`, `Bewerk`, `Toon`, `Zoom`) names the thing elsewhere. ❌ Never swap a menu-bar title for it.
- Undo: `Herstel` (⌘Z, Edit menu), `Zet terug` (trash put-back), `Ongedaan maken` (Ask Cmdr rename run).
- `Vorige` pairs with `Volgende`; `Terug` is a lone back button; `commands.navBack.label` takes Finder's `Ga terug`.
- Column headers `Aanmaakdatum` / `Bewerkingsdatum`; the date tooltip uses participles (`Laatst gewijzigd`).
  `shortcuts.section.filterModified` → `Gewijzigd` (changed shortcuts, not a file date).
- `Actief` (a server runs) vs `Bezig` (a task runs); `Gereed` (status) vs `Klaar` (spoken after a checklist step);
  `Selecteer` (files) vs `Kies` (an option); `Zoek` (the button) vs `Zoeken` (the feature).

## Wat het Engels over zichzelf rechtzette, en wat dat voor `nl` betekende (`settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`, `menu.app.hideOthers`)

- `askCmdr.renameUndo.undone` names its object (`De oude naam van … is teruggezet`), so the whole sentence sits in the
  plural branches (`naam`/`namen` and `is`/`zijn` both agree).
- Panes without a token use Apple's Dutch: `Algemeen`, `Inlogonderdelen en extensies` (not the older `Inloggen`), and
  `Apple Account` stays English as a pane name (prose: `het juiste Apple-account`).
- `Toon alles` / `Verberg andere` (Finder `MenuBar.strings`).

## Een half teruggedraaide bewerking afmaken (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- `Voltooi terugdraaien` (Finder `NE108` "Voltooi kopiëren") over `Verder terugdraaien`, which promises no end. ⚠️ Keep
  `operationLog.dialog.finishRollBack` and `fileOperations.rollbackConfirm.finishRollBack` identical.
- `queue.row.reversalInFolder` stays `in {folder}`, same as EN and unquoted like its neighbours.

## De terugdraaimelding: wat er weg is, en wat Cmdr met rust liet (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

- "Leave alone" → `ongemoeid laten`, shared with `askCmdr.renameUndo.skipReason.*`; never `overgeslagen` (that's
  Skipped). ⚠️ `reason.folderNotEmpty.named`/`.counted` stay byte-identical to the `skipReason` twins.
- `{name}` may be a file or a folder, so no reason line uses a pronoun: `daar is iets aan gewijzigd`,
  `nadat Cmdr er klaar mee was`. `.named` and `.counted` share one reason sentence.
- ⚠️ `Cmdr kon niet controleren of …` in both this family and `skipReason.unverifiable.*`, never `nagaan`; no check
  guards it, since the two English sources differ only in the apostrophe glyph.
- A full rollback leads with `Alles` and puts the count after a colon (`Alles is teruggezet: {countText} …`): Dutch
  can't put `de` before a numeral.
- `verwijderen` (remove) and `terugzetten` (put back) never mix; `De rest staat er nog.`
- `stagedLeftover.*`: `onvolledig exemplaar`, `Cmdr ruimt het op`, and `bij een latere overdracht`, ❌ never "de
  volgende keer": cleanup skips anything younger than an hour.

## Het blokkeerscherm bij te oude WebKit (`main.oldWebkit.*`)

`Software-update` (macOS spelling), `Stop`; `Safari`, `Mac`, and `15.4` stay.

## De melding over een oude macOS (`main.oldMacos.*`)

- Honest and relaxed: `ondersteund`, `X en nieuwer`, "best effort" → `doet het zijn best zonder garanties` (no calque),
  "look off" → `er raar uitzien`.
- The last sentence is David in the first person, as `onboarding.stepBeta.greeting`.

## Ask Cmdr looks inside files: the `inspectFile` tool line and the reworded consent screen (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- `Kijkt in bestanden` / `In bestanden gekeken` (tentative), in the shape of the sibling tool lines.
- Thumbnail → `miniatuur`; EXIF → `cameragegevens` (tentative); PDF pages → `PDF-pagina's`; title and author →
  `titel en auteur` (Microsoft's `functie` is the job sense).
- `Cmdr verstuurt nooit hele bestanden …`: `hele` replaces the old promise of no contents, which is now false.
- `tot jij het goedkeurt` / `zonder jouw goedkeuring`: stressed forms mark the contrast.

## De twee tooltips van de Rollback-knop (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

`Stop en zet elk bestand terug dat tot nu toe is verplaatst`, never `verwijderen`: rolling back a move deletes nothing.

## ‘Terminal hier openen’ en de app-keuze (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- The generic word and Apple's app are both `terminal` / `Terminal` (Finder `N67`), so the card title is justified as
  same as EN.
- The command is `Terminal hier openen` everywhere it's named; `Kies app…` is Finder's own `Choose Application…`
  (`N137`, confirmed).

## `Sort by relevance`: de tooltip van de zoekresultatenkolom (`fileExplorer.columns.sortByRelevance`)

`Sorteer op relevantie`: four macOS bundles say `Relevantie`, and the frame matches `Sorteer op naam`.

## `Documents and packages`: de nieuwe OOXML-rij (`settings.archives.ooxml.*`)

Bare `pakketten` (Finder `Toon pakketinhoud`) keeps the row broader than the `App-pakketten` card below it, the same
split EN makes; the frame is that of `settings.archives.zip.description`.

## De serverhub: verbindingsscherm, verbindingstooltips en de vergeet-bevestigingen (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`)

- Keychain Access → `Sleutelhangertoegang` (its `InfoPlist.loctable`); Keychain alone → `Sleutelhanger`.
- `Verbinden met {name}…` (Finder `MN1`), unquoted like EN; server address → `serveradres`.
- The confirmation title repeats the menu item that opens it (`Vergeet server`); the question takes the infinitive-last
  form (`{name} vergeten?`) with an imperative button.
- `disconnectBusyTooltip` mirrors `ejectBusyTooltip` word for word.
- `disconnectPlaceAriaLabel` → `Verbreek de verbinding met {name}`: it starts with the visible `Verbreek`, and Dutch
  breaks a connection, never a server.
- Compromised → `gecompromitteerd` (tentative; Apple has no word for a revoked `known_hosts` key).

## De serverhub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`, `shortcuts.scope.servers`/`.places`)

- Places heading → `Locaties` (Finder's sidebar heading); column `Type`, never `Soort` (Finder's Kind).
- Found nearby → `Gevonden in de buurt` (participle first reads as a label, like Finder's `Gedeeld door`; tentative).
- Host key where EN says only "key" → `de sleutel`, over `hostsleutel`.
- `Wachten tot je de sleutel controleert`: the user compares a fingerprint, so `controleren` over `bekijken`.
- `commands.serversTogglePin.label` → `Zet server vast / maak hem los`: an English imperative takes a Dutch imperative,
  and `hem` is safe because it's the noun `server`, not an insert. The `aan/uit` form answers a different English
  ("Toggle …").
- Toasts naming `{name}` avoid pronouns (`De server is nog steeds opgeslagen.`, `waar {name} te zien is`).

## Het serververbindingsvenster en de sleutelvraag (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

- From macOS bundles: `Voeg server toe`, `Verbind`, `Verbind met server`, `Blader…` (a button, so not Safari's
  `Bladeren`), `Log in bij {name}` (CloudKit), `Uitgelogd bij {name}`, `Verbind als gast`, `Onthoud in Sleutelhanger`.
- Passphrase → `wachtzin` (Apple everywhere), never `wachtwoordzin`; `Sleutelwachtzin` tentative.
- Fingerprint → `vingerafdruk` over the untranslated `fingerprint` in Apple's weakest SSH strings;
  `Vingerafdruk van de sleutel` (Apple puts the owner after the noun).
- Remote → `extern` (`Externe map`); `Vertrouw en verbind`; `Verbind automatisch opnieuw`.
- "Cmdr stopped connecting" → `Cmdr verbindt niet meer met {name}`, present tense: the pane stays until the user acts.
- `{name}` and `{host}` never take a pronoun: `De sleutel van {host} is gewijzigd`.
- Coined (tentative): `Sleutelbestand`, `Manier van verbinden` (aria only), `de servereigenaar`.

## Twee paneelregels erbij: automatisch opnieuw verbinden en inloggen met een sleutel (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- `Opnieuw verbinden met {name}…` (Apple's `Opnieuw verbinden…`, `errors.listing.deviceReconnecting.title`) pairs with
  `Verbinden met {name}…`; a full sentence says `opnieuw verbinding maken` (the PPP/VPN family).
- `Open de server opnieuw om het nog eens te proberen.`: the noun is spelled out, since `hem` could point at
  `de sleutel`.
- `er valt niets te typen` (tentative): EN says "type", and there's no form left to fill in.

## De vastzet-hint, de vertrouwde serversleutels en het ADB-paneel (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

- `Maak vast in volumekiezer` / `Maak los`: Apple's own `Maak vast in <plek>` (the particle before the place is Apple's
  order here). `vastzetten` and `vast maken` are one verb pair: never `Maak server vast / maak hem los`, which repeats
  `maak`.
- `Controleer opnieuw` (Check Again), `Niet gevonden`, `Vergeet`, `Vertrouwde <ding>`, `Blader op <plek>`: all macOS.
- Host key where EN says "host key" → `serversleutel` (tentative): a settings list needs a noun, and `hostsleutel` was
  rejected.
- `Vertrouwd op <datum>`; `Gevonden: {path}` (tentative) dodges choosing `op` or `in` for a path.
- Quoted labels in `servers.pinHint.body` and `settings.adb.install.intro` use ‘…’ even where EN doesn't quote, as
  Finder does (`Klik op ‘Ga door’`).

## Het telefoonpaneel via ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

- On the phone, Android's own Dutch wins (AOSP `values-nl/strings.xml`): `USB-foutopsporing` (`SettingsLib`
  `enable_adb`), `‘Toestaan’` (SystemUI `usb_debugging_allow`), `tik op`, `Zet … aan`, `zet het scherm aan`. The word in
  Cmdr must be the word on the phone's screen, whatever the `@key` says.
- `De verbinding tussen Cmdr en je telefoon is verbroken.` keeps `Cmdr` in the sentence (Apple's passive
  `is verbroken`), since the don't-translate check flags a dropped brand.
- `Verbreek de verbinding met {name}`, never `Werp {name} uit`: the phone stays on the cable.
- `aangesloten` for the cable (`adb.connect.deviceGone`), `verbonden` for the logical link.
- `adb.connect.cancelled` → `Je hebt het openen van je telefoon gestopt.`, over `geannuleerd`, which points at the
  `Annuleer` button.
- Never name a diagnosis (no adb server, transport, daemon, or serial number).

## De vastgezette serveridentiteit (`servers.sheet.identityLocked`)

The hint names the actions as the buttons do (`Vergeet deze server en voeg hem opnieuw toe`), and "are what name this
server" → `bepalen welke server dit is`, since the form has its own `Naam` field.

## De toast als er helemaal geen wachtwoord opgeslagen was (`fileExplorer.navigation.forgetSecretNoneToast`)

`Er was geen opgeslagen wachtwoord voor {name}.`, reusing `opgeslagen wachtwoord` and `voor {name}` from its siblings.

## De herhaalduur, de kop over de hostsleutel en Androids knop Toestaan (`servers.paneState.retryTotalSeconds`, `.retryTotalMinutes`, `.retryKeepsTrying`, `servers.refusal.hostKeyRevoked`)

- `retryTotal*` are bare building blocks of `retryKeepsTrying` (no preposition, no period).
- "Cmdr won't connect to {name}" → `Cmdr maakt geen verbinding met {name}`, never `niet meer`, which implies it worked
  before.

## Het contextmenu van de serverrij: `Open` en `Wijzig server…` (`menu.network.open`, `menu.network.edit`, `commands.serversEdit.label`)

`Open` equals `menu.file.open` (Finder uses one verb for both senses); `Wijzig server…` equals
`commands.serversEdit.label` byte for byte, since two labels would read as two features.

## Function key bar context menu (`settings.appearance.showFunctionKeyBar.label`, `fileExplorer.functionKeyBar.hiddenToast`)

Function key bar → `functietoetsbalk`, one name for the setting, the menu item, and the toast.

## Het Dock-aanbod (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `Dock` stays English and takes `het` in a sentence (System Settings' `Toon/verberg het Dock automatisch`), none in a
  short label.
- Applications folder → `de map ‘Apps’` (Finder `TL_HELP_APPS`); `Programma's` survives only in `Hulpprogramma's`.
- `Voeg toe aan Dock` is Apple's verbatim label, the one exception to the particle-last rule.
- Pin / unpin → `vastzetten` / `losmaken`, the catalog pair, over Apple's `Permanent in Dock` menu label.
- `een paar dagen`, never a number; `Wie deze Mac beheert, kan dat aanpassen.` stays gender-neutral.

## Het Dock-menu van Cmdr zelf (`menu.dock.*`)

`Dock.app` `DockMenus.strings` is the Tier 1 source: `Open Cmdr` (Dock's `Open`), Finder's `Ga naar map…` (distinct from
`menu.go.goToPath` `Ga naar pad…`), `Verbind met server…`, and `Zoek bestanden…` equal to `menu.edit.searchFiles`.

## Het ‘Toon in Finder’-aanbod en de melding bij de eerste keer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

`‘Toon in Finder’` in quotes everywhere, as the settings card writes it; `al een tijdje`, never a number; the first-time
notice explains and never apologizes.

## De onboarding-herschrijving: de checklist, de stapteller en de vier verdictregels (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

- GitHub and AlternativeTo have no Dutch UI, so the buttons the user clicks say `Star` and `Like`:
  `Geef de repo een star op GitHub`, `Geef Cmdr een like op AlternativeTo`. Never `ster` (a rating) or `vind ik leuk`
  (Facebook's button). The two rows share one shape; rewrite both or neither.
- `checklist` over Microsoft's `controlelijst` (inspection); `Onboardingchecklist` closed, like `onboardingopties`.
- `<field></field>` goes after the particle: `Vul je e-mailadres in <field></field> om …`.
- `‘Lokaal netwerk’` quotes Apple's pane row verbatim, never the description `Lokale netwerktoegang`.
- A `…summary` line beside a switch must not wrap: telegram style, cut an adverb or article, never a fact.
- `mailinglijst` over Microsoft's `adressenlijst` (an admin's distribution list); `superprivé`.
- Released copy / dev and test builds → `uitgebrachte versie` / `dev- en testbuilds`.

## Het weergavevenster haalt een bestand eerst op (`viewer.pull.*`, `viewer.error.stoppedResponding`)

`{fileName} ophalen om te bekijken` (Finder `Ophalen…`, MS fetch), `{doneText} van {totalText}`, `tot nu toe`; "stopped
arriving" avoids the stall formula `komt niet meer vooruit`, since the viewer shows no transfer.

## De verouderde index van een telefoon via ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- Inherit the drive siblings' words (`mogelijk verouderd`, `werkt de index bij`); never `losgekoppeld`, the phone is
  still on the cable.
- `werkt … bij` over `houdt … bij`, which means "keeps a record" (`settings.operationLog.intro`).
- The dialog repeats `Cmdr` where `zijn` could point at `{name}`.

## Hoofdmap en beginmap van een opgeslagen server (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.refusal.*`)

- Root folder → `hoofdmap` (MS, Dolphin) over `rootmap`, `hoofddirectory`, and `Externe map`, which lacks the ceiling
  sense.
- Start folder → `beginmap` (tentative, after Double Commander's `Beginpad`) over `startmap` (Windows's Start menu).
- `hem mag lezen`: `mag` because it's a permission refusal.

## Waarom een gedeelde map niet aankoppelt of de lijst niet laadt (`errors.mount.*`, `errors.shareList.*`, `servers.refusal.accountNotPermitted`)

- In the pane, share → `gedeelde map`, matching its title; `netwerkshare` stays for "isn't a network share".
- Guests → `gasten` (NetAuthAgent); a server's power: `Controleer of de server aanstaat`.
- `{server}` and `{share}` take no pronoun; the sentence repeats the noun.
- Same English, same Dutch across `errors.mount.*` and `errors.shareList.*` twins.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- Text editor → `teksteditor` (MS), the kind of app, never Apple's TextEdit; `standaardeditor` as in
  `commands.fileEdit.label`.
- `Bewerk bestanden in {app}`, no article before `{app}`; the rest mirrors the terminal card's keys.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

In progress → `wordt losgekoppeld`; already gone → `werd losgekoppeld` (the next two sections). Don't swap them.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

`werd losgekoppeld`, like `indexing.staleDialog.body`; "starts from scratch" → `helemaal opnieuw`.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

- Every value ends on where the files are, never on what went wrong: `Je originelen zijn onaangeroerd`,
  `De rest staat nog op de schijf.`, in a main clause so the reassurance isn't buried after a `nadat` clause.
- `{done} van {total} bestanden`, never `van de`; `er is niets verloren gegaan` (Apple's `verloren gaan`).
- `{volumeName}` and `{counterpart}` take no article and no pronoun: `nadat Cmdr er … naartoe had gekopieerd`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

- Never say the move failed: title `Kon het verplaatsen niet bevestigen`, `het verplaatsen` over `de verplaatsing`
  (never a UI form).
- `dus heeft het je originelen laten staan waar ze stonden`: active voice shows Cmdr protected them.
- "Have a look" → `Kijk even op …`, lighter than `Controleer`.

## Files a drive brings back from a move that never finished (`fileOperations.leftovers.stagingFolderKept`)

- Never suggest the folder may go: these are the user's files (unlike `cancelRollback.stagedLeftover.*`, Cmdr's own).
- `waarvan het verplaatsen niet is voltooid`; `voltooid` for file operations, `afgerond` for scans and replies.
- `heeft ze allemaal laten staan`; `een verborgen map met de naam {folderName}` (Finder's `met de naam`, never
  `genaamd`).

## Het favorietenmenu (`commands.favoritesOpen.label`/`.description`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- `favorietenmenu` closed; `Toon favorieten` in both the Go menu and the palette (byte-identical twins).
- "See {count} favorites" → `Bekijk …`: EN contrasts See with Show, and the catalog keeps `Bekijk` for looking only.
- The `0` row has one key, `Voeg huidige map aan favorieten toe` (particle last); the shortcut list quotes it.
- A favorite's `nummer` identifies it; the key you press is a `cijfer`.
- `Deze map staat al in je favorieten`; `favoritesCantAddHere` gives the reason after a colon with `werken op`, and
  "mounted share" → `gekoppelde netwerkshare` over macOS's `activeren`, which the catalog never uses.

## Wie de schijf vasthoudt: de zes geweigerde-uitwerpzinnen (`errors.eject.unmountRefusedBy*`, `.otherApps`)

- One verb for the family: `{app} gebruikt deze schijf nog` / `{apps} gebruiken …`, the name first and without an
  article (a process name is a proper noun).
- ❌ No pronoun back to `{app}`: `Sluit alles wat daar openstaat`.
- `andere apps`, never `programma's`; `Intl.ListFormat('nl')` supplies the list conjunctions.
- Disk image → `schijfkopie`, repeated as a noun since `schijf` is also a de-word.
- `staat nog open` (tentative) keeps EN's everyday "open" over the technical `gekoppeld`.
- `Wacht een minuutje` (macOS) vs `Wacht even` (Cmdr), as EN distinguishes.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- Kind → `Soort` (Finder's sort criterion); `Selecteer alles met extensie *.{extension}` puts the mask where nothing
  agrees with it; `extensie` over Double Commander's `achtervoegsel`.
- `menu.context.selection` is the noun `Selectie`, never the verb `Selecteer` (`menu.bar.select`).
- Each `menu.select.*` / `commands.selectionSelectSameKind.*` pair shares one English string; reword both or neither.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

- `Geen volledige schijftoegang` is System Settings' own row (`ALL_FILES`); `bannerTitle.denied` shares the string.
- The aria opens with the label verbatim; reword both together.
- "Files macOS keeps to itself" → `bestanden die macOS voor zichzelf houdt`, ❌ never a macOS feature name.

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`, `errors.write.readError.title.trash`, `errors.write.writeError.title.trash`)

- The five titles share one English string; reword all or none.
- No plural here, so `{count} van de onderdelen die je koos` reads right at any count.
- `suggestion.other` names `fileOperations.errorDialog.technicalDetails` verbatim; the badge text quotes
  `onboarding.fdaBadge.label` in ‘…’.

## De waarschuwing voor alleen-online inhoud (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- The download is a consequence, not something the move does:
  `Als je die onderdelen naar de prullenmand verplaatst, worden ze eerst gedownload`.
- Keep all four facts (trash would download, only whole-selection delete, no copy in the trash but the service keeps its
  own, the ways out); ‘Verwijder’ equals `fileOperations.delete.confirmDelete`.

## Als de server zegt dat die gedeelde map er niet is (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Retrying can't help, so nothing sounds temporary: `Dit lost zichzelf niet op`, no `nu`, no `probeer het opnieuw`. `die`
points at `deze gedeelde map`, since `hij` floats between two de-words.

## Namen die er hetzelfde uitzien (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.volume.ambiguousName`, `ai.secretError.keychainBody`)

- Item → `onderdeel` even for a Keychain Access item (its own `Alle onderdelen`); `item` stays only for non-files
  (`menu-item`, `stash-item`, recent-list entries).
- `de server slaat ze anders op` over `spellen`, which reads oddly for a server.
- A button name as a subject: `Met ‘Overschrijf’ vervang je …`.

## De schakelaar ‘Cloud-AI toestaan’ en de toestanden waarin cloud-AI uit staat (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

- `Cloud-AI toestaan` (the `<object> toestaan` label shape), conjugated in a sentence: `Sta cloud-AI toe`.
- `cloud-AI` is a de-word: refer back with `die` or repeat it.
- `Zet Ask Cmdr aan`; `settings.askCmdr.enabled.label` is the bare product name, justified as same as EN.

## Escape en de schermvullende weergave (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

`schermvullende weergave` (AppKit); the switch `Schermvullende weergave uitschakelen met Escape` takes Apple's
`uitschakelen` for Exit, and the notice reuses it byte-identically.

## Wachtregels in "Open met" en "Deel" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`Apps zoeken…` (a progress line in the infinitive), `Geen deelopties` (macOS's empty-menu pattern).
