# de decisions

Distilled rulings behind `terms.json`: per-key phrasing calls and the non-obvious why. `pnpm i18n:brief` pulls the
sections whose heading cites a batch's keys, so every heading cites its keys in backticks. Rules already in `style.md`
or `terms.json` aren't repeated here.

## Einstellungsabschnitte (`settings.section.*`)

- Section names are quoted verbatim across the catalog („unter Einstellungen > KI“): one form each, sourced from
  `settings.section.*`. Titles keep `Dateioperationen`, the loanword's one survival.

## Der Doppelklick-Hinweis und der Bereichshintergrund (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- `Bereichshintergrund` / `leere Fläche rund um die Dateiliste` (Dolphin, DC); the setting says
  `in den übergeordneten Ordner wechseln` (DC's wording for this setting).
- `Das nie wieder tun` over macOS „Nicht mehr anzeigen“: the button turns the behavior off, not only the hint.

## Zwischenablage als Datei einsetzen (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

- `das PDF` is neuter, but the toast's `select` branch stays article-free („PDF aus der Zwischenablage …“).

## Kopieren und Bewegen: Zielordner, Fortschrittszeilen und zu große Dateien (`fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`, `fileOperations.shared.scanningTooltip`, `fileOperations.errorDialog.tooLargeAndMore`, `errors.write.filesTooLargeForFilesystem.*`)

- Active `Cmdr erstellt ihn beim Kopieren.` over macOS' passive; `queue.row.label` arms stay passive present.
- „and N more files“ → `und {countText} weitere {count, plural, …}`: feminine `weitere` fits both branches.
- A file system's cap is `Begrenzung`; elsewhere `Limit`.

## Kurzbefehle erfassen und reservierte Systemkürzel (`downloads.shortcutRow.*`, `shortcuts.system.*`, `shortcuts.section.pressKeys`, `shortcuts.conflict.*`)

- Reserved-shortcut reasons are nominalized (`das Sperren des Bildschirms`) because they sit in the conflict warning's
  `(…)` aside.

## Zeitangaben und Beispiel-Platzhalter (`queryUi.age.*`, `fileOperations.mkdir.placeholder`, `fileOperations.mkfile.placeholder`, `feedback.dialog.placeholder`, `settings.analytics.email.description`)

- „{count}m ago“ → `vor {count} Min.` (tight tooltip).

## Geteilte Bausteine nach zustimmen/ablehnen (`askCmdr.decision.*`, `askCmdr.wakeDigest.*`)

- The shared `{verbName}` fragment goes after a colon (`Du hast zugestimmt: …`): `zustimmen` takes the dative,
  `ablehnen` the accusative, so no slot fits both. `das Rollback` is neuter everywhere.

## Archive browsing (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `fileOperations.delete.archiveWarningStrong`)

- `App-Paket` over MS `Bundle` (Finder „Paketinhalt zeigen“), one word across `card.bundles`, `bundle.label`, and prose.
  `entpacken` over MS `extrahieren` (DC).
- The delete warning says `aus der Zip-Datei entfernt`, not bare `das Zip`.

## Archive-password dialog (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `titleWithCounts` uses lowercase `komprimieren` like its siblings `kopieren` / `bewegen`. Slider ends `Schneller` /
  `Kleiner` (a comparative pair); label `Komprimierungsstufe`.

## Operation log (`operationLog.*`, `commands.logOperationLog.*`, `settings.operationLog.*`)

- Title and `commands.logOperationLog.label` equal the card name `Vorgangsprotokoll`.
- Prose says `rückgängig machen`; chips keep the short noun `Rollback` (`Rollback möglich`, `Rollback läuft` without
  ellipsis, `Teilweises Rollback`) for width. Lifecycle chips reuse `queue.row.status` verbatim.
- The more-items line declines inside each branch (`weiteres Objekt` / `weitere Objekte`). Rollback lines share
  `Rollback des Vorgangs „…“`; latest → `Letztes` (macOS `Letzte Synchronisierung`).

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- `Tokens` with English `-s` (OpenAI's German help); `Werkzeug` over `Tool` (end users see it).
- Tool lines: doing is passive present (`<place>{name}</place> wird aufgelistet`), done drops `wird`, „tried“ →
  `nicht gespeichert`; never a bare infinitive (reads as a command).
- Unarchive → `Archivierung aufheben` (the `Auswahl aufheben` pattern).

## Image-content indexing on network drives (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`, `search.imageResults.paused`)

- Keep the English split: `Foto` in network/NAS strings, `Bild` on the local card. `Bildindizierung` is the indexing,
  `Bildersuche` the search it feeds.

## Indexing run-kind headers + hour-scale ETA (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

- `Erster` / `Erneuter vollständiger Durchlauf` differ in one word; the quick path is `Schnelle Aktualisierung`, not a
  `Durchlauf`.
- Only the hour scale spells units out, each an ICU plural (`noch 2 Stunden 5 Minuten`); sub-hour keys keep `Min.` /
  `s`.
- A running scan as subject is `Das Laufwerk wird noch durchsucht`; the event noun is `Laufwerksdurchlauf`.

## Bulk rename review + image-index scope (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`)

- Per-row gate `Erlauben` / `Ablehnen`, never macOS' system-prompt `Nicht erlauben`.
- Badges `(Überschreiben!)` (lowercase reads as an imperative) and `(Endung)`.
- „needs attention“ → `Bei dieser Umbenennung ist noch etwas zu klären` (the literal is an anglicism); its „continue“
  means proceed (`ausgeführt werden kann`). `Lass Ask Cmdr sie erneut vorbereiten`: a leading `Bitte` reads as „please“.
- `Wichtigkeit der Ordner`, never `Ordner-Wichtigkeit`; „the device holding {path}“ → `das Gerät mit {path}`.

## Image-index status badges on files/folders/drives (`fileExplorer.imageIndex.file.*`, `fileExplorer.imageIndex.folder.*`, `fileExplorer.imageIndex.drive.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- `Statussymbol` over MS `Kennzeichen` (flag) and `Abzeichen` / `Badge` (achievement).
- `{done} von {total} … other {Bildern}` (dative after `von`); nominative `Alle … Bilder` keeps `Bilder`.

## Image-index settings restructure: cards + semantic-search model delete (`settings.mediaIndex.clip.*`, `settings.mediaIndex.semanticSearch.*`, `settings.mediaIndex.progressSummary.title`)

- Friendly name `Suche per Beschreibung` (never `Beschreibungssuche`); card title `Semantische Suche`.
- „Indexing now“: heading `Indizierung läuft`, file tooltip `Wird gerade indiziert`. `{size} freigeben` (macOS
  „Speicherplatz freigeben“).

## Delete-dialog trash switch + transfer From/To groups (`fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `fileOperations.transferDialog.targetGroupTitle`)

- `In den Papierkorb bewegen` over Finder's „… legen“: one move verb across every trash string.
- Headings `Von` / `Nach` (TC and DC in the same dialog); controls keep `Zielvolume`, `Zielpfad`.

## Master-switch-off strings for drive indexing (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `settings.indexing.overriddenBadge`)

- `unter Indizierung > Laufwerksindizierung` quotes live labels: change a label, update all three `fileExplorer`
  strings.
- „stays unindexed“ → `wird … nicht indiziert`, never `unindiziert`. „keeps its own on or off choice“ →
  `behält seine eigene Einstellung`.
- Badge `Mit der Laufwerksindizierung aus`, never `Aus mit …` (a fixed exclamation, „X is over“).

## Drive index: the change-check run (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- `Prüfung auf Änderungen`, a noun phrase like its sibling headers.

## Stalled transfer: the honest-stall notice (`fileOperations.transferProgress.close`/`.stallNotice`/`.stallWaitingDestination`/`.stallWaitingSource`/`.stallUnknown`/`.stallInFlight`/`.stallLogHint`)

- `Kein Fortschritt seit {duration}` leads with the state (it replaces the ETA line); `.stallNotice` must fit the queue
  row.
- `Die Übertragung kommt nicht mehr voran.` over `steht still` (too final; it may recover).
- `Details stehen in der Protokolldatei.`: naming the file keeps it apart from `Vorgangsprotokoll`. `teilweise`, never
  `partiell`.
- The ways out start `Du kannst …`: the line offers choices, not orders.

## Kopierter Pfad: die Zwischenablage-Bestätigung (`fileExplorer.clipboard.copiedPath`)

- Ends on a colon (the path shows below) and stands without it; no `dein` before `Zwischenablage` (there is only one).

## Operation queue: the queue window's rename (`queue.*`, `commands.queueShow.label`)

- `Vorgangswarteschlange` pairs with `Vorgangsprotokoll` in the View menu; `queue.windowTitle`, the menu item, and
  `commands.queueShow.label` stay byte-identical.
- `transferProgress.pauseAria` / `.resumeAria` keep `Diese Übertragung` (English says transfer). `queuedToast` says
  `dieser hier`: a bare `er` would point at the operation ahead.

## Corner progress chip + the failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- Dismissing a failed row is `Ausblenden`: `Entfernen` reads as acting on the operation.
- „Couldn't finish X“ → `Kopieren nicht abgeschlossen` (object-first, like `queue.row.label`), never `abgebrochen` (the
  cancelled status). `queue.failureToast.summary` and the first sentence of `queue.chip.failed` share one rendering; „to
  see why“ goes inside both plural branches (`den Grund` / `die Gründe`).
- The tooltip is a `·` fact list (`Wird kopiert · 214 Objekte · nach Backup · 42 %`), each optional part carrying its
  own `·` in its branch. Only the aria spells out `Prozent`.

## Standalone conflict prompt: the operation-context line (`fileOperations.operationConflict.context`/`.pausedNote`)

- `Wird nach {destination} kopiert` (Finder's progress line); `bewegen` takes `nach` too. The chip can't, since its
  `{label}` arrives pre-composed.
- `archive_edit` → `{destination} wird bearbeitet` (nominative); fallback `In {destination} wird gearbeitet`.
  `Alles andere ist angehalten, bis du antwortest.` (state passive).

## The progress dialog's empty-queue button (`fileOperations.transferProgress.background`/`.backgroundAria`)

- `Im Hintergrund`, never bare `Hintergrund` (a backdrop) nor `In den Hintergrund` (sending a window behind); it's the
  prefix of the aria `Im Hintergrund weiterlaufen lassen`.

## The quit gate (`main.quit.*`)

- Title `Ein Vorgang läuft noch. Trotzdem beenden?` (TC, AppKit); heading `Noch aktiv` avoids a second `läuft noch`.
- Cancel is `Weiterarbeiten`, never `Abbrechen` (the opposite meaning here) nor `Später`; `Jetzt beenden`, never
  `Sofort beenden` (Force Quit).
- `Cmdr beendet sich …` with a trailing `darauf`, so `Cmdr` appears once; `entfernt`, never `löscht`, on a reassuring
  dialog.

## Usage stats: "anonymous" dropped, "a random id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- `eine zufällige ID`, never `pseudonym(isiert)` or MS `Bezeichner`: the English is deliberately everyday.

## Conflict-parked queue rows and the rollback confirmation (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `Antwort erforderlich`, nothing with `warten` (the queued status in the same column).
- Title uses the verb (`rückgängig machen?`), the confirm button the noun `Rollback`. `Dateien behalten`, not bare
  `Behalten` (the body names replaced files too).
- `foregroundBusyToast` names `diesen Vorgang` and the `Anzeigen` button: „this one“ has no German antecedent.

## Rename chaining: the counted "and so did N others" toast (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- `3 weitere Dateien behalten ihre bisherigen Namen, ebenso „A“: …` (Finder's `ebenso`);
  `„A“ behält den bisherigen Namen`, never `seinen`.

## The catch-all name rejection (`fileOperations.validation.nameNotUsable`)

- `Dieser Dateiname kann nicht verwendet werden` (Finder), no period: it's composed into
  `fileExplorer.rename.keptOriginalName`.

## Duplizieren: der Befehl, der im selben Ordner kopiert (`commands.fileDuplicate.*`)

- `Duplizieren` (Finder), apart from `Kopieren` (F5) and `Bewegen` (F6).

## Native Menüs: Menüleiste, Kontextmenüs, Fenstertitel (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Source order: German Finder (`MenuBar.strings`, `LocalizableMerged.strings`), Safari for tab words, MS only where
  Apple has nothing.
- Select menu `Auswählen` (Nautilus/Thunar/Dolphin), matching `Alles auswählen`. `Zoomen` is Window > Zoom, `Zoom` the
  text-zoom submenu.
- `Im Finder anzeigen` in the menu too (`Im Finder zeigen` is macOS' „Reveal“). `Änderungsprotokoll` (the document) vs
  `Neuigkeiten` (What's new). `Zeilenumbruch` over `Textumbruch`. `Tab lösen` reverses Safari's `Tab fixieren`.
- Tag row: `Tags`, `„{color}“ hinzufügen` / `„{color}“ entfernen` (Finder `TG5` / `TG6`).
- The busy marker is ` (in Benutzung)` on the unchanged base label, every `*Busy` key: MS `beschäftigt` / `besetzt` read
  wrong for a disk.

## SMB-Fallback-Hinweis: die Freigabe hängt an der Systemverbindung (`fileExplorer.network.osMountFallback.*`)

- `die eigene SMB-Netzwerkverbindung von macOS` (`nativ` is unattested); `Systemverbindung` is the set word.
- `Die direkte Verbindung zu X kam nicht zustande.` fills the missing subject without blaming Cmdr.
  `Direkte Verbindung versuchen`, never stacked infinitives.
- The X is `Schließen` (shares a `sourceHash` with `lowDiskSpace.toast.closeTooltip`).
- require → `verlangen` (`SMB-Verschlüsselung immer verlangen`, managed value `Immer verlangt`); prose quotes that
  label + `eingeschaltet`.

## Umbenennen und Anlegen: die abgewiesenen Mutationen (`errors.mutation.*`, `errors.volume.*`)

- `Systemintegritätsschutz` (Apple Support DE) over Finder's one-off `System-Integrationsschutz` (`ET6`); open in
  `review-queue.md`.
- „a volume's top folder“ → `der oberste Ordner eines Volumes` (Nautilus), not Windows `Stammordner`. „Out of space“ →
  Finder's `Kein Platz mehr`; the long sibling keeps „nicht genügend freier Speicher“.
- `timedOut` is open, not failed: `… die Änderung klappt also womöglich trotzdem noch.` `deviceSessionReset` reuses
  `errors.listing.deviceReconnecting.*`, never unplugging.
- „lost track of“ → `aus den Augen verloren`: `findet … nicht mehr` reads as deleted.
- The two archive refusals share `Beim Umbenennen kann ein Objekt …`; `Bewege es stattdessen.` points at `Bewegen`.

### Die beiden Papierkorb-Absagen (`errors.mutation.trashNotSupported`/`.trashRefused`)

- `trashRefused` → `macOS wollte das nicht in den Papierkorb bewegen.`: keeps macOS as the refuser, which
  `Die Datei ließ sich nicht …` (`errors.write.ioError.message.trash`) loses.

## Absturzdialog: die drei Eröffnungssätze (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`, `crashReporter.dialog.privacyNote`, `crashReporter.dialog.title.*`, `crashReporter.sentToast.message.*`, `crashReporter.dialog.alwaysSend`)

- `.keptRunning` and `.unknown` never say Cmdr crashed or quit. `ist … weitergelaufen` (composed from `weiterlaufen`),
  never `hat weitergearbeitet` (claims usefulness) nor `wurde fortgesetzt` (a Vorgang's verb).
- `ist auf ein Problem gestoßen`: the MS guide lists `aufgetreten` as a bad example and it makes Cmdr the object.
  Settled.
- `.keptRunning` drops only `Absturz-` from `.ended`'s second sentence; that carries „nothing crashed“. The shared
  `privacyNote` says `auf das Problem gestoßen`.
- `Absturzbericht` only for `.title.crash`, otherwise `Bericht`; `alwaysSend` stays `Absturzberichte immer senden` after
  the setting `Absturzberichte senden`.

## Der Einstellungstext für Absturzberichte deckt jetzt beide Fälle (`settings.updates.crashReports.description`)

- Covers both cases (`unerwartet beendet wird` / `im Hintergrund auf ein Problem stößt`) with `einen Bericht`; the label
  keeps `Absturzberichte senden`.

## Auswerfen und Trennen: die neun Absagen (`errors.eject.*`)

- Each value follows either toast frame and concerns a drive, share, phone, or server: a standalone sentence that says
  `dort`, never `dieses Laufwerk`, and ends `dann versuche es erneut`, never `wirf es … aus`.
- `notEjectable` states the build (`… kein Wechselmedium, …`). `Schließe offene Dateien und beende laufende Apps` (no
  shared verb). `idle` → `nicht mehr beschäftigt`; „wouldn't“ → `wollte nicht`.
- `busy` → `Dort läuft noch ein Vorgang von Cmdr`. `timedOut` / `unexpected` mirror `errors.mutation.*`.

## Papierkorb-Toast: Widerrufen und Zurücklegen (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- `Widerrufen` (AppKit, `askCmdr.renameUndo.undo`), never Nautilus' `Rückgängig`. The toast says `Laufwerk` (English:
  drive).
- The second half's `{skipped}` count carries a real verb (`Objekt blieb` / `Objekte blieben`).
- Button and command are `Zum Papierkorb gehen`.

## Fehlerbericht nachträglich ergänzen: der Notiz-Dialog (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- A note joins the SAME report; the wording must say no second report goes out. One stem: `hinzufügen` →
  `Wird hinzugefügt …` → `es kommt zu dem hinzu, was das Team schon hat`.
- `Was gesendet wurde` twins `errorReporter.dialog.detailsToggle`. `Deine Notiz` without „(optional)“ (one of note or
  email is required).
- `Bericht ansehen oder Notiz hinzufügen` (`ergänzen` leaves open what's added); `Notiz zum Bericht hinzugefügt.` drops
  the second `dein`.

## Der Auswahldialog: Dateien auswählen und abwählen (`selection.*`)

- `menu.select.deselectAll` keeps Finder's `Auswahl aufheben`; `menu.select.deselectFiles`,
  `commands.selectionDeselectFiles.label`, and `selection.dialog.title.remove` say `Dateien abwählen` (MS, DC): Finder's
  phrase takes no object. Unifying breaks the title against its menu item.
- `Letzte Auswahlen` mirrors `Letzte Suchen`; `selection.recent.applyAria` mirrors `search.recent.runAria`.
- The tooltip is its own verb-final sentence: the accessible name comes from the label key.

## Der zugängliche Name muss die sichtbare Beschriftung enthalten (WCAG 2.5.3, `fileOperations.transferProgress.pause`, `queue.row.pause`, `queryUi.scope.toggle.caseSensitiveAria`)

- `Anhalten` sits inside `Diesen Vorgang anhalten`, which is why the button isn't `Pause`; `/` counts for containment.
- `Groß-/Kleinschreibung beachten` (the positive of macOS „… ignorieren“), wrapped as
  `Beim Abgleich Groß-/Kleinschreibung beachten`.

## Ein englisches Wort, ein deutsches Wort: die Drift-Prüfung (`commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel`, `menu.bar.file`, `menu.bar.view`, `menu.window.zoom`, `menu.view.zoom`, `menu.tag.purple`, `settings.tint.purple`, `queue.row.dismiss`, `queryUi.bar.runLabel`, `ai.cloud.checking`, `licensing.dialog.checking`, `updates.status.checking`)

Unified: F3 `Ansehen` (never `Vorschau`: the `@key` wants a verb); `Zum … gehen` on buttons and in Settings;
`Verborgene Dateien einblenden`; in-progress `Wird …` except `fileExplorer.navigation.spaceRetryingText`
(`Erneuter Versuch …`, narrow space bar); `Alles klar` for Got it; `Von:` before a path.

Deliberate splits (don't unify):

- `Ablage` / `Datei`; `Darstellung` (menu) / `Ansehen` (action); `Zoom` (text) / `Zoomen` (window).
- `Lila` (Finder tag) / `Violett` (Cmdr's tint). `Wird geprüft` (connection, key) / `Wird gesucht` (updates).
- `Suche` (noun) / `Suchen` (run button). `Zuletzt verwendet` only as the palette group (else `Letzte`); `Hinweis` for
  Cmdr's toast, `Benachrichtigung` for the settings category.

## Wörter, die auseinandergelaufen sind, ohne dass ein Check es sehen konnte (`settings.fileOperations.allowFileExtensionChanges.*`, `askCmdr.wake.stop`, `settings.mediaIndex.clip.comingSoon`, `settings.whatsNew.lastSeenVersion.label`)

- Settings say `erlauben` like the dialog that writes the setting (`fileExplorer.extensionChange.alwaysAllow`).
  `askCmdr.wake.stop` says `stoppen`: `anhalten` would promise a pause.
- `Bald verfügbar` (never `Demnächst`); `Änderungsprotokoll` in compounds too.

## Die Bereichsnamen kommen jetzt vom Mac des Nutzers (`errors.git.*`, `errors.provider.*`)

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` render the user's own macOS names: a preposition
  may precede them, never an article (`Öffne {system_settings} und …`).
- `Apple Account` stays English, unhyphenated; `Allgemein` and `Anmeldeobjekte & Erweiterungen` have no token and are
  plain text (macOS 26 de).

## „Zurücksetzen“ nennt jetzt das Objekt: den alten Namen (`askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`, `menu.app.hideOthers`)

- `Die alten Namen von N Dateien zurückgesetzt.`; `Alle einblenden` / `Andere ausblenden` are macOS' app-menu words.

## Ein halb zurückgenommener Vorgang: das Rollback zu Ende bringen (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- `Rollback abschließen` (never restart; `fortsetzen` is `queue.row.resume`), identical in both keys sharing the
  English.
- The notice echoes `rollbackConfirm.bodyUndoByDeleting` and promises no complete undo. `in „{folder}“` needs quotes, or
  the folder itself reads as deleted.

## Der Toast nach einem abgebrochenen Vorgang: was die Rücknahme geschafft hat (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Tone: Cmdr did the careful thing, never apologetic or alarming. `Habe {name} unverändert gelassen` shares its voice
  with `askCmdr.renameUndo.skipReason.*`.
- `Objekt` over the rename family's `Datei` (folders too). Three verbs stay apart: `entfernt`, `zurückgelegt`,
  `löschen`.
- Subjectless English gets Cmdr as subject (`Cmdr hat alle 1.234 Objekte entfernt, die es geschrieben hatte.`); a `one`
  branch drops the count (`Das 1 Objekt` reads wrong).
- `am neuen Ort` / `am alten Ort` (`Platz` is storage space); `seit Cmdr es dort abgelegt hat`.
- Both `folderNotEmpty` values equal `askCmdr.renameUndo.skipReason.folderNotEmpty.named` / `.counted`: change both or
  neither. `leftBehind` says `überspringt alles`, like its confirmation.
- `rollbackConfirm.body` drops `davon` from its last sentence: after „ersetzte Dateien kommen nicht zurück“ it would
  promise the opposite.

### `cancelRollback.stagedLeftover.*` (Cmdrs eigene Reste am Ziel)

- Cmdr's own leftover: `Cmdr räumt sie weg`, not `löschen`. Retry `wenn es sich das nächste Mal mit dem Ziel verbindet`,
  never a time.

## Der Block-Screen bei zu altem WebKit (`main.oldWebkit.*`)

- `Softwareupdate`, one word (the pane's name).

## Der Hinweis auf zu altes macOS (`main.oldMacos.*`)

- Relaxed, no apology or warning: the app runs. `macOS 12 und neuer`; `best effort` → `nur so gut, wie es eben geht`
  (never „nach bestem Bemühen“); `look off` → `daneben liegen`.

## Ask Cmdr schaut jetzt in Dateien hinein: Zustimmungstexte und Werkzeugzeilen (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- Tool line `Dateien werden durchgesehen`, never `Dateiinhalte werden gelesen`: `Dateiinhalte` is what Cmdr does NOT
  read (`askCmdr.empty.hint`). Prose says `hineinschauen`.
- `Miniatur` (Preview, Photos), never `Vorschaubild`. `Kameraangaben` (no `EXIF`, like the English). `Aufnahmeort`: bare
  `Ort` reads as storage. `Autor`, not Apple's `Autor:in` (open in `review-queue.md`).
- `contentsRule` says `nie ganze Dateien, Fotos oder Miniaturen` and `einen begrenzten Teil davon`.
- „looks inside a file only when you ask“ → `schaut nur dann in eine Datei hinein, wenn du nach ihr fragst`.

## Die zwei Tooltips der Rollback-Taste (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- `Stoppen und alle bisher bewegten Dateien zurücklegen` (undoing a move deletes nothing);
  `.rollbackAlreadyLandedTooltip` quotes `Abbrechen` verbatim.

## „Terminal hier öffnen“ und seine App-Auswahl (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `Terminal hier öffnen` everywhere (Finder „In Terminal öffnen“); `App auswählen …` over Apple's „Programm auswählen
  …“.

## Nach Relevanz sortieren: der Tooltip der Suchergebnis-Spalte (`fileExplorer.columns.sortByRelevance`)

- `Relevanz` (Automator), never `Häufigkeit` (frequency: the wrong order).

## Dokumente und Pakete: die OOXML-Zeile (`settings.archives.ooxml.*`)

- Bare `Pakete` (Office documents and .jar/.apk), broader than the `App-Pakete` card; the frame copies
  `settings.archives.zip.description`.

## Der Server-Hub: Verbindungszustände, Trennen und Vergessen (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`, `fileExplorer.network.browser.*Host*`, `fileExplorer.network.share.useGuest`/`.signInAs`)

- Disconnecting ends the connection: `Cmdr konnte die Verbindung zu {name} nicht trennen.`, aria
  `Verbindung zu {name} trennen` (Apple's FileProvider), never `{name} trennen`.
- The `*BusyTooltip` trio shares `Cmdr arbeitet noch mit Dateien auf diesem … . …, sobald das erledigt ist.`; eject says
  `Wirf das Gerät aus` (`es` could be Cmdr).
- `Die Verbindung wurde unterbrochen.`, never `abgebrochen`.
- A certificate or key takes the dative (`vertraut dem Zertifikat … nicht`); `*.changedDisclosure` names its noun
  (`Ich habe den Schlüssel geprüft`). In prose `der Schlüssel von {host}`, never `Hostschlüssel` nor
  `{host}s Schlüssel`.
- `Anmeldemethode` over Apple's `Authentifizierungsmethode`; `Das Passwort hat für {username} nicht geklappt.`, never
  `ungültig`.
- Back-references say `den Server`: in `connectionTooltipNeedsHostKey` two masculine nouns precede it.
- Dialog titles equal their menu items; `Freigabe vergessen` copies `Server vergessen`. Forget toasts take Cmdr as
  subject: bare `{hostName} vergessen` reads as an order, `ließ sich nicht vergessen` as „unforgettable“.
- `als {username}`, `als Gast` lowercase after the name; buttons `Anmelden als …` (Finder `SN1`) and
  `Als Gast durchsehen` (browse verb).

## Die Server-Übersicht: Spalten, Zustände und die Zeile im Volume-Umschalter (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- Switcher row and shortcut scope `Server`; the group above stays `Netzwerk`.
- Column `Zuletzt benutzt` (Finder's recent servers); `Zuletzt verwendet` is the palette group, `Zuletzt geöffnet` the
  file date.
- `Gespeichert`, never Podcasts' `Gesichert`. `Wartet darauf, dass du den Schlüssel prüfst` (verb over noun pile).
  „Local network discovery is off.“ → `Die Suche im lokalen Netzwerk ist ausgeschaltet.`
- `servers.hub.rowCount` has identical branches (`{countText} Server`), deliberately. „Disconnect server“ →
  `Verbindung zum Server trennen`. `Places` → `Orte` (Finder), buckets too.

## Das Verbindungsblatt, der Hostschlüssel und die zwei Vorschauzeilen (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

Apple's Connect to Server dialog (`NetAuthAgent.app` `Localizable.loctable`) is the first source.

- `Bei {name} anmelden`; `Mit Benutzername und Passwort anmelden` (Apple's `Registrierte:r Benutzer:in` uses a gender
  colon).
- `servers.sheet.needsStoredSecret` quotes the checkbox `Im Schlüsselbund merken` verbatim (`merken` →
  `gemerktes Passwort`).
- `Durchsuchen …` (file picker) vs `settings.archives.opt.browse` `Durchsehen` is allowlisted in
  `apps/desktop/scripts/i18n-term-consistency-allowlist.json`.
- `Entfernter Ordner` (macOS' „Remote X“). `Schlüssel-Passphrase`, not `Passwort` (that field sits right above); open in
  `review-queue.md`.
- `Dem neuen Schlüssel vertrauen`; „something is sitting between you and it“ →
  `sich zwischen dich und den Server schalten`, never `Man-in-the-Middle`.
- Legend `Verbindungsmodus` (names the control, like `Protokoll`). `Erste Verbindung zu {host}` describes, never
  commands. `Der Schlüssel von {host} hat sich geändert`.
- „the server's owner“ → `die Person, die den Server betreibt` (`Betreiber` is generic masculine).
  `Cmdr hat die Verbindung zu {name} gestoppt`, never `abgebrochen`.
- `Hinzufügen` / `Hinzufügen und öffnen` / `Trotzdem hinzufügen` share a stem; the help line says `speichert`.
  Go-to-path preview lines stay third person (`Öffnet {name}`).

## Der Wiederverbindungs-Zyklus und die Anmeldung per Schlüssel (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- `Verbindung zu {name} wird wiederhergestellt …` beside `… wird hergestellt …`; Apple's short `Erneut verbinden …`
  breaks once `{name}` attaches.
- `Bei diesem Server meldest du dich mit einem Schlüssel an …, du musst also nichts eingeben.`: `eingeben` is Apple's
  field verb (NetAuthAgent `FS_MSG_PASS`), `tippen` a gesture; `ein Schlüssel`, never `SSH-Schlüssel`.
- `Öffne den Server erneut, …` (`ihn` is ambiguous after `ein Schlüssel`). The link `In den Einstellungen einschalten`
  pairs with `ausgeschaltet`.

## Server fixieren und lösen, die vertrauten Hostschlüssel und die ADB-Seite (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

- `fixieren` (Safari), never `anpinnen`; reverse `Aus der Volume-Auswahl lösen` (quoted in `servers.pinHint.body`),
  never `Loslösen` or `entfernen`. `In der Volume-Auswahl fixieren` keeps the surface's name.
- `Erneut prüfen` (Software Update's „Check Again“): nothing new is searched.
- `vertrauenswürdig` / `vertrauen`; `Vertrauenswürdig seit <Datum>` (bare `Vertraut` means „familiar“).
  `Bisher vertraust du keinem Schlüssel.`
- `Cmdr achtet auf Telefone.` / `… gerade nicht auf Telefone.`; the `@key` bans ADB-server talk.
  `Den Befehl „adb“ auswählen`; `settings.adb.browse` copies `servers.sheet.browse`.
- A request says `Klicke einen Server mit der rechten Maustaste an`; tooltips keep `Rechtsklick`.
  `Der Server bleibt in der Liste „Server“.`, never `Er` (after `der Befehl`).
- Help texts front `Wenn du dich zum ersten Mal …`. Tint label `Serverbereiche einfärben (SMB, SFTP, WebDAV)`, like
  `tintMtp`.

## Das Telefon über ADB öffnen: Bereichsmeldungen, Zeilen-Tooltips und der Hinweisstreifen (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Android's own German (AOSP `values-de`) outranks Apple for Android's words.

- `Android Platform Tools` (product) vs `die Android-Tools`. `USB-Anschluss`; `Port` stays for network ports.
- A cabled phone `reagiert nicht` (Apple's device verb); the network `antwortet nicht`. Waking a screen is `aktivieren`
  (`aufwecken` is for people).
- `adb.connect.deviceTooOld` puts Cmdr first, reason in a `da` clause (Apple). `phone` → `Telefon`, `device` → `Gerät`,
  key by key.
- `Disconnect {name}` equals `fileExplorer.navigation.disconnectPlaceAriaLabel`. The hint line's × is `Ausblenden` (gone
  for good), unlike the returning `osMountFallback` notice.
- `Warten darauf, dass du USB-Debugging erlaubst`, no ellipsis. `Möchtest du das gesamte Dateisystem sehen?` (a German
  question needs its verb). `Wie geht das?` (bare `Wie` reads cut off).
- `Sieh auf dein Telefon …` (look); Apple's `Überprüfe` means check a setting. `adb.connect.cancelled` →
  `Du hast das Öffnen deines Telefons gestoppt.` (`abgebrochen` points at the button).

## Der veraltete Index eines Telefons (`driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- Still connected, so never `getrennt`; the title is `indexing.staleDialog.title` with `Telefons`.
  `{name} teilt Cmdr nicht mit, …` (`meldet Cmdr nicht` reads as accusative); `Dateien darauf`.
  `bleibt zur Erinnerung stehen`.

## Die gesperrte Server-Identität (`servers.sheet.accountLocked`)

- `Konto`, never `Account`. The hint names actions as their buttons do (`vergessen`, `hinzufügen`). „are what name this
  server“ → `machen diesen Server aus`: the sheet has its own `Name` field.

## Der Toast, wenn gar kein Passwort gespeichert war (`fileExplorer.navigation.forgetSecretNoneToast`)

- `Für {name} war kein Passwort gespeichert.`: no form-speak, no regret.

## Die Wiederholdauer, die Host-Key-Überschrift und Androids Erlauben-Knopf (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`/`.retryKeepsTrying`/`.hostKeyChanged`, `adb.readiness.waitingForAuthorization`)

- The duration keys are bare fragments for `Es wird insgesamt {duration} lang weiterversucht.`: no preposition, no
  period; `{seconds}` picks the branch.
- `Cmdr verbindet sich nicht mit {name}` (present: a standing refusal). `antippst` avoids two `auf` in a row.

## Das Kontextmenü der Server-Zeile: Öffnen und Server bearbeiten … (`menu.network.open`, `menu.network.edit`)

- `Öffnen` equals `menu.file.open`; `Server bearbeiten …` copies `commands.serversEdit.label`.

## Das Dock-Angebot (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Apple's `Dock.app` `DockMenus.strings` supplies almost every phrase (`KEEP_IN_DOCK` / `REMOVE_FROM_DOCK`).

- `Ja, zum Dock hinzufügen` drops „my“ (Apple keeps the article). `Ordner „Programme“`, noun first.
- `Wer diesen Mac verwaltet`, never Apple's `deine:n Netzwerkadmin`. `seit ein paar Tagen`, never a number.
- `addedButDockDidNotRestart` leads with the result. Back-references go through `das Symbol`. `Nein, danke`: Cmdr never
  asks again.

## Das Dock-Menü von Cmdr (`menu.dock.*`)

- `Cmdr öffnen` (Apple). `Dateien suchen …` = `menu.edit.searchFiles`; `Mit Server verbinden …` = the palette entry.
  `{name} ({parent})` stays identical (Finder `IN_G6_V1`).

## Gehe zu Ordner: Finders Wortlaut auf jeder Fläche (`menu.go.goToPath`, `commands.navGoToPath.label`, `goToPath.dialog.*`)

- Finder's `Gehe zu Ordner …` (MenuBar `261.title`) / `Gehe zu Ordner` (GotoWindow `1.title`) for menu, command, and
  title.
- The „Go“ button is `Gehe zu` (`menu.bar.go`, same English). Finder's `Öffnen` (`FR24`) would split the term; switching
  needs an allowlist entry David approves.

## Das „Im Finder anzeigen“-Angebot und der Ersttreffer-Hinweis (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `„Im Finder anzeigen“` quotes `settings.navigationAndFileOps.card.showInFinder`. `schon eine Weile`, never a number.
  No `Entschuldigung`, no `Leider`.

## Die Einführungs-Checkliste, der Schritt-Tooltip und die vier Kurzfazits (`onboarding.*`)

- `Sichern` shares a `sourceHash` with `servers.sheet.save`; `E-Mail-Adresse gesichert` beside it.
- `Vergib dem Repo auf GitHub einen Stern` (GitHub's German docs); `Gib Cmdr auf AlternativeTo ein Like` (the site shows
  English; `gefällt mir` is too long).
- `Checkliste` over `Prüfliste`; `Mailingliste`, never MS `Verteiler`, and the list is what refused the address.
- `Lokales Netzwerk` (the System Settings name). Summaries say `Platz`, prose `Speicherplatz`; they stay verb-initial
  and parallel, reusing their `…desc` words.
- `Mehr über {topic}`, no article. `stepTooltip` keeps `+1`; its `select` branches start with a comma. `dumber` stays
  `dümmer` (David's voice).
- `stepAi.local.tooltip`'s `<strong>` quotes `stepAi.cloud.label`. `settings.revealHandler.notProductionBuild`:
  `veröffentlichte Version`, `Dev- und Test-Builds`.

## Die Vorschau holt eine Datei erst herüber (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- `laden` (Finder „Laden …“), never MS `abrufen`; `{fileName}` is the unquoted nominative subject.
  `{doneText} bisher geladen` without a total.
- `Von dieser Datei kommen keine Daten mehr an.`: the viewer shows no operation, so not the stall line.

## Stamm- und Startordner eines gespeicherten Servers (`servers.sheet.rootFolder`/`.startFolder` samt `…Help`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

- `Stammordner` (MS) names this field only; `Startordner` (Dolphin). `ein Ordner darin` dodges a pronoun between two
  masculine nouns.
- `Prüfe, ob es ihn gibt und ob dein Konto ihn lesen darf.`; `deshalb hat Cmdr nichts gesichert`. `benennen` fits
  `nameHelp` (the `Name` field), unlike `accountLocked`.

## Warum eine Freigabe nicht eingebunden wird oder die Freigabenliste nicht lädt (`errors.mount.*`, `errors.shareList.*`)

- NetAuthAgent `Localizable.loctable` phrases these cases first. `„{server}“ zeigt Gästen keine Freigaben` (no
  possessive).
- `dieser Computer`, never `Mac`: these lines show on Linux too. `mountRefused` says the server refused, so the password
  isn't blamed.
- `errors.mount.hostUnreachable` = `errors.shareList.hostUnreachable`, `errors.mount.authFailed` =
  `errors.shareList.authFailed`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `Text-Editor` is the app kind (TextEdit arrives as `{app}`); default `Standardeditor`. `Dateien bearbeiten mit` runs
  into the dropdown; `in {app}`, no article. Shared strings equal their `openTerminalHere` siblings.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- `wird gerade getrennt` while ejecting, `wurde getrennt` once gone.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- `wurde getrennt`; `startet von vorn` like `indexing.rescan.incompletePreviousScan`.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

- The second sentence ends on where the files are now (German stress); never a bare fact. Pulled, not ejected: nothing
  from `auswerfen`.
- `{volumeName}` and `{counterpart}` hang on `auf` (no `Quelllaufwerk`, no article); „to it“ → `darauf`. Every sentence
  says where something `liegt`; `es ist also nichts verloren gegangen`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

- Not a failure: Cmdr kept the originals because it couldn't prove the write; never `ist schiefgegangen`.
- `Die Bewegung ließ sich nicht bestätigen` (`bestätigen` acknowledges, `prüfen` looks); `die Bewegung` names this one
  move.
- „were saved on“ → `geschrieben`. The reassurance ends the sentence; „haven't moved“ → `liegen noch dort, wo sie waren`
  (`bewegt` echoes the command).

## Ein wiedergefundener Staging-Ordner (`fileOperations.leftovers.stagingFolderKept`)

- Possibly the only copy: reassure, never advise deleting. `eine nicht abgeschlossene Bewegung` (`unvollständig` is
  Cmdr's own copy).
- `sie an ihrem Platz gelassen` (Cmdr chose it; `liegen geblieben` reads as neglect). `verborgen`, never Tier 3's
  `versteckt`.
- `auf {volumeName}`, `namens {folderName}`; a colon lets the line end on the folder name.

## Das Favoritenmenü (`menu.go.showFavorites`, `commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`, `shortcuts.scope.favoritesMenu`)

- `Favoriten` (Finder), never `Lesezeichen`; TC's `Verzeichnisliste` and DC's `☆-Tabs` name other features.
- `Favoriten anzeigen` like `Server anzeigen`. `eingebunden`, never `gemountet`; `Laufwerk` in plain-speech tooltips.
- The `0` row has ONE key, `favoritesAddCurrent`, quoted by the shortcut list. `Favorit` is weak (`den Favoriten`).
  `mit einer Zifferntaste`.
- `Dieser Ordner kann kein Favorit sein: …` mirrors `favoritesAlreadyAdded`; `Favoriten gibt es für Ordner auf …` avoids
  „zeigen auf Ordner auf“.

## Wer das Laufwerk festhält: die sechs benannten Absagen (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- Same frame as `errors.eject.*`: `X verwendet dort noch etwas.` + `…, dann versuche es erneut.`; generic
  `unmountRefused` → `Dort wird noch etwas verwendet.`
- `{app}` / `{apps}` lead as nominative subject (`Intl.ListFormat('de')` ends on nominative `andere Apps`). Never
  `belegt`, `blockiert`, `greift zu`. `Schließe die App` / `diese Apps`: no pronoun for an insert. Eject all →
  `Alle auswerfen` (Finder `N199.1`).
- `Dort liegt ein Image, das noch geöffnet ist.` (Finder `PE24`); `Wirf erst das Image aus, dann versuche es erneut.`
- `Warte eine Minute` (macOS) vs `einen Moment` (Cmdr) follows English. `sende einen Fehlerbericht` (only mention, full
  name).
- `Ein Prozess namens {process} …`; the aside keeps Cmdr as subject, never `Wir`. `otherProcesses` → `weitere`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `Alle mit Endung *.{extension} auswählen` (nothing agrees with the mask). `menu.context.selection` is the noun
  `Auswahl`. Each `menu.select.*` / `commands.selectionSelectSameKind.*` pair shares one English: reword both.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

- The pill `Kein Festplattenvollzugriff`, shared with `onboarding.stepAi.bannerTitle.denied`; the aria opens with it
  verbatim.
- Running prose (`onboarding.stepFda.revoked.noAccess` and siblings) may say `vollständiger Festplattenzugriff`.
  `Dateien …, die macOS für sich behält`, no feature name.

## The trash refusal dialog (`errors.write.trashRefused.*`)

- The title shares its English with the four `errors.write.*.title.trash` keys: reword all five or none.
- No plural param, so `{count} der ausgewählten Objekte`. `suggestion.other` reuses `die technischen Details`.
  `geschützt`, not Finder's `Gesperrt`.
- The pill is `der Hinweis` (not `Statussymbol`), quoting `onboarding.fdaBadge.label`.

## Der Online-Warnhinweis im Löschdialog (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Four facts survive: the trash would download the files; Cmdr only offers deleting the WHOLE selection; the trash keeps
  NO copy (the service has its own; never soften); the ways out. `AllWarning` drops „abwählen“.
- Both `<strong>` spans stay; `„Löschen“` quotes `fileOperations.delete.confirmDelete`. `HandedBack` is matter-of-fact.

## Wenn der Server die Freigabe gar nicht kennt (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- Retrying can't help: no „gerade“, no „noch einmal versuchen“; `Das erledigt sich nicht von selbst`.
  `deutlich langsamer`, unquantified.
- The toast's `sie` points at `diese Freigabe`; `{server}` never gets a pronoun.

## Namen, die gleich aussehen (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.listing.ambiguousName.explanation`, `errors.listing.ambiguousName.suggestion`, `errors.volume.ambiguousName`)

- `der Server schreibt sie unterschiedlich`, no Unicode jargon. `zwischen Groß- und Kleinschreibung unterscheiden`
  (Finder).
- `„Überschreiben“ ersetzt das Objekt, das schon da ist.` quotes `conflictOverwrite`.

## Der Schalter „Cloud-KI erlauben“ und die Zustände „Cloud-KI ist ausgeschaltet“ (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

- Named switch quoted `„Cloud-KI erlauben“`; as a verb `Erlaube … Cloud-KI` unquoted; pronoun `sie`. `Seitenbereich`,
  never `Seitenleiste` (Finder's sidebar). `Ask-Cmdr-Chats`.

## Vollbildmodus und esc-Taste (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `Vollbildmodus` in sentences (Apple's bare `Vollbild` is the window tile). `Vollbildmodus mit esc-Taste beenden` on
  both switches (shared `sourceHash`).
- `… hat Cmdr aus dem Vollbildmodus geholt` (no blame); `nur den Dialog oder das Menü` (no shared pronoun).

## Platzhalterzeilen in „Öffnen mit“ und „Teilen“ (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- `Optionen zum Teilen`: `Freigabe` is the network share, `Teilen-Optionen` clumsy.

## Das Original blieb stehen (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

- `an beiden Orten` (Finder `FI12`); `Um das Bewegen abzuschließen`. `Es ist nichts verloren gegangen.` repeats
  `deviceDisconnected.sided.destination.copy`.

## Mehrfach-umbenennen-Tasten (`commands.multiRename*.label`)

- `Option „…“ ein-/ausschalten` over `umschalten` (English: turn on or off).

## Tooltips im Mehrfach-Umbenennen (`multiRename.placeholderHelp.*`, `multiRename.searchOptionHelp.*`)

- `Ein`/`Aus` like `settings.*`; caret → `Einfügemarke` (macOS 27 AppKit); `Beginnt mit` like Finder's rename.

## NFS-Server: Freigaben, Benutzer-ID und die Absagen (`servers.sheet.nfs*`, `servers.sheet.identityLockedNfs`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`, `servers.hub.nfsHint*`)

- export → `Freigabe` / `NFS-Freigabe`, never `Export`: German NAS forums, Linux docs, and Synology's DE help all say
  so. The field label is `Freigabe`.
- `servers.refusal.reservedPortRequired` quotes Synology DSM de's switch
  `Verbindungen von nicht-privilegierten Ports zulassen` (dropping „(Ports über 1024)“ like the English); its `zulassen`
  is a quoted label, so the `allow` ruling yields.
- A pronoun after `Freigabe` never reaches back past `Liste` or `Option` (both feminine): repeat the noun or take the
  article (`gib den Pfad ein`).

## Bereichseditor (`multiRename.rangeEditor.*`)

- `Anfang`/`Ende` mit `Von vorn`/`Von hinten` statt „Ende: Vom Ende“; `#. von hinten` statt „3.-letzte“; take →
  `übernehmen`; Zeilen umordnen → `nach vorn/hinten rücken` wie `menu.queue.moveUp`.

## Medienfelder (`multiRename.media*`)

- `Feld` (TC `Pluginfelder`); names per Nautilus/Thunar; artist → `Interpret` (MS) over `Künstler`;
  `hat dieses Feld nicht` dodges a gendered `keine(s)`.

## KI-Vorschläge (`suggestedOps.*`, `menu.view.suggestedOps`, `commands.suggestedOpsShow.label`)

- Title `KI-Vorschläge` (same English as `fileOperations.mkdir.aiSuggestions*`) over `Vorgeschlagene Aktionen`; `Why:` →
  `Warum:` like `Warum?`.
- Pills pair `Du kannst das widerrufen:` / `Das lässt sich nicht widerrufen:`; the app's notice is a `Hinweis` (not
  `Notiz`); tick box → `einbeziehen` (macOS „Anhänge einbeziehen“).
