# de decisions

Distilled rulings behind `terms.json`: per-key phrasing calls and the non-obvious why that defends them. Not read by
default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so every heading cites its keys in
backticks. Term rulings live in `terms.json`, open questions in `review-queue.md`, voice and typography in `style.md`.

## Einstellungsabschnitte (`settings.section.*`)

- Section names are cited verbatim across the catalog („unter Einstellungen > KI“), so each has exactly one form; the
  catalog's `settings.section.*` values are the source.
- Section and card TITLES keep `Dateioperationen`, the one place the loanword survives; anything a user started is a
  `Vorgang`.

## Der Doppelklick-Hinweis und der Bereichshintergrund (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- `Bereichshintergrund` / `leere Fläche rund um die Dateiliste` for the pane background (Dolphin, Double Commander).
- The setting says `in den übergeordneten Ordner wechseln` (Double Commander's wording for this exact setting); the hint
  body says `Das bringt dich …` (casual prose).
- `Das nie wieder tun` over macOS' „Nicht mehr anzeigen“ because the button turns the behavior off, not only the hint.

## Zwischenablage als Datei einsetzen (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

- `das PDF` is neuter (macOS „PDF-Dokument“), but the toast's `select` branch stays article-free („PDF aus der
  Zwischenablage …“) so no gender has to be fixed.

## Kopieren und Bewegen: Zielordner, Fortschrittszeilen und zu große Dateien (`fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`, `fileOperations.shared.scanningTooltip`, `fileOperations.errorDialog.tooLargeAndMore`, `errors.write.filesTooLargeForFilesystem.*`)

- Active `Cmdr erstellt ihn beim Kopieren.` over macOS' passive „wird erstellt“ (active-voice rule); the
  `queue.row.label` progress arms still stay passive present (`Wird kopiert`, `Archiv wird bearbeitet`).
- „and N more files“ → `und {countText} weitere {count, plural, one {Datei} other {Dateien}}`: feminine `weitere` fits
  both branches.
- A file system's cap is `Begrenzung` („keine solche Begrenzung“); elsewhere the noun is `Limit`.

## Kurzbefehle erfassen und reservierte Systemkürzel (`downloads.shortcutRow.*`, `shortcuts.system.*`, `shortcuts.section.pressKeys`, `shortcuts.conflict.*`)

- The reserved-shortcut reasons are nominalized (`das Sperren des Bildschirms`, `das Abmelden`) because they sit in the
  conflict warning's `(…)` aside; `Mission Control`, `Spaces`, and `Spotlight` stay English as in German macOS.

## Zeitangaben und Beispiel-Platzhalter (`queryUi.age.*`, `fileOperations.mkdir.placeholder`, `fileOperations.mkfile.placeholder`, `feedback.dialog.placeholder`, `settings.analytics.email.description`)

- „{count}m ago“ → `vor {count} Min.` (abbreviated: a tight tooltip).

## Geteilte Bausteine nach zustimmen/ablehnen (`askCmdr.decision.*`, `askCmdr.wakeDigest.*`)

- The shared `{verbName}` fragment („das Kopieren von“) goes after a colon (`Du hast zugestimmt: …`), because
  `zustimmen` takes the dative and `ablehnen` the accusative: one fragment can't fit both sentences otherwise.
- `das Rollback` is neuter everywhere (`Teilweises Rollback`, `es`).

## Archive browsing (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `fileOperations.delete.archiveWarningStrong`)

- Browsing into an archive is `durchsehen` (Dolphin „Archive durchsehen“), never `durchsuchen`, which is the scan/search
  verb.
- `App-Paket` over MS `Bundle`: Finder says „Paket“ („Paketinhalt zeigen“); keep one word across `card.bundles`,
  `bundle.label`, and the prose.
- `entpacken` over MS `extrahieren` for archives (Double Commander).
- The delete warning's tail says `aus der Zip-Datei entfernt`: feminine `Datei` reads better than bare `das Zip`.

## Archive-password dialog (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

- `titleWithCounts` uses the lowercase infinitive `komprimieren`, matching its siblings `kopieren` / `bewegen`.
- The slider ends are `Schneller` (level 1) / `Kleiner` (level 9, the smaller file), a comparative pair; the label is
  `Komprimierungsstufe`.

## Operation log (`operationLog.*`, `commands.logOperationLog.*`, `settings.operationLog.*`)

- `Operation` survives only in the protocol sense (`settings.network.smbConcurrency.label`,
  `settings.network.customTimeout.description`) and the Settings titles; anything a user started is a `Vorgang`, in
  tooltips and descriptions too.
- The dialog title and `commands.logOperationLog.label` must equal the Settings card name `Vorgangsprotokoll`.
- Prose says `rückgängig machen`; status chips keep the short noun `Rollback` (`Rollback möglich`, `Rollback läuft`
  without an ellipsis like its sibling chips, `Teilweises Rollback`) to fit the chip width.
- Lifecycle chips reuse `queue.row.status` verbatim (`Wartet`, `Läuft`, `Fertig`, `Nicht abgeschlossen`, `Abgebrochen`).
- The more-items line declines inside each branch (`weiteres Objekt` / `weitere Objekte`) because `Objekt` is neuter.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- `Tokens` takes the English `-s` plural (German AI usage, OpenAI's German help center).
- `Werkzeug` over the loanword `Tool`: the status shows to end users in the chat rail.
- Tool status pairs: doing is passive present (`Ein Ordner wird aufgelistet`), done drops `wird`
  (`Ein Ordner aufgelistet`); the subject stays nominative so the two stay parallel, and articles/possessives mirror the
  English.
- A bare infinitive (`Einen Plan vorbereiten`) reads as a command, never a status.
- Unarchiving a chat is `Archivierung aufheben` (the `Auswahl aufheben` pattern).
- „Same as Cmdr's AI“ → `Wie die KI von Cmdr` (no `Cmdrs`).

## Image-content indexing on network drives (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`, `search.imageResults.paused`)

- Keep the English split: `photo → Foto` in the network-drive/NAS strings, `image → Bild` on the local card. Don't
  collapse both to `Bild`.
- `Bildindizierung` names the indexing, `Bildersuche` the search it feeds; don't swap them.
- Auto-pause is `Angehalten` / `hält an`, never `pausiert` (macOS says `angehalten`).

## Indexing run-kind headers + hour-scale ETA (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

- `Erster` / `Erneuter vollständiger Durchlauf` differ in one word; the quick path is `Schnelle Aktualisierung`, not a
  `Durchlauf`.
- Only the hour scale spells units out, each an ICU plural (`noch 2 Stunden 5 Minuten`); sub-hour keys keep `Min.` /
  `s`, as English does.
- A running scan as subject is the verb form `Das Laufwerk wird noch durchsucht` (matches `indexing.scan.label`); the
  event noun is `Laufwerksdurchlauf`.

## Bulk rename review + image-index scope (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`)

- Per-row gate `Erlauben` / `Ablehnen`, never MS's harsh `verweigern` nor macOS' system-prompt `Nicht erlauben`.
- The overwrite badge is `(Überschreiben!)`: lowercase reads as an imperative, the opposite of a warning. The extension
  badge takes the short `(Endung)`.
- „needs attention“ → `Bei dieser Umbenennung ist noch etwas zu klären` (the literal `braucht Aufmerksamkeit` is an
  anglicism); its „continue“ means proceed (`ausgeführt werden kann`), not resume.
- `Lass Ask Cmdr sie erneut vorbereiten`: a sentence-initial `Bitte` reads as „please“.
- `Wichtigkeit der Ordner`, never the ad-hoc `Ordner-Wichtigkeit`; „the device holding {path}“ → `das Gerät mit {path}`
  like `errors.listing.deviceDisconnected.explanation`.

## Image-index status badges on files/folders/drives (`fileExplorer.imageIndex.file.*`, `fileExplorer.imageIndex.folder.*`, `fileExplorer.imageIndex.drive.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- `Statussymbol` over MS `Kennzeichen` (the flag sense) and `Abzeichen` / `Badge` (achievement sense).
- `von` governs the dative, so `{done} von {total} … other {Bildern}`; the nominative `Alle … Bilder` frames keep
  `Bilder`.
- „Image search is off“ → `Die Bildersuche ist … ausgeschaltet` (everyday prose; `deaktivieren` stays for buttons and
  settings).

## Image-index settings restructure: cards + semantic-search model delete (`settings.mediaIndex.clip.*`, `settings.mediaIndex.semanticSearch.*`, `settings.mediaIndex.progressSummary.title`)

- The friendly name is `Suche per Beschreibung` (never a coined `Beschreibungssuche`); the card title stays
  `Semantische Suche`.
- „Indexing now“ splits by slot: the Settings heading `Indizierung läuft`, the file tooltip `Wird gerade indiziert`.
- `{size} freigeben` / `Das gibt {size} frei` (macOS „Speicherplatz freigeben“).

## Delete-dialog trash switch + transfer From/To groups (`fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `fileOperations.transferDialog.targetGroupTitle`)

- `In den Papierkorb bewegen` over Finder's menu item „In den Papierkorb legen“, so the catalog keeps one move verb and
  matches every sibling trash string.
- The From/To headings are `Von` / `Nach` (Total and Double Commander ship this pair in the same dialog); the controls
  keep the nouns `Zielvolume`, `Zielpfad`.

## Master-switch-off strings for drive indexing (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `settings.indexing.overriddenBadge`)

- The path `unter Indizierung > Laufwerksindizierung` quotes the live labels; change either label and all three
  `fileExplorer` strings follow.
- „stays unindexed“ → `wird … nicht indiziert`, never the coinage `unindiziert`; `{name}` stays the nominative subject.
- Hidden from view is `ausgeblendet`; `verborgen` is only for hidden (dot) files.
- „keeps its own on or off choice“ → `behält seine eigene Einstellung` (`Ein-/Aus-Wahl` is a coinage).
- The badge is `Mit der Laufwerksindizierung aus`, never `Aus mit …`: „Aus mit X!“ is a fixed exclamation („X is over“).

## Drive index: the change-check run (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- `Prüfung auf Änderungen`, a noun phrase like its sibling headers.
- „check against the index“ → plain `mit dem Index verglichen` over database-register `abgleichen`.

## Stalled transfer: the honest-stall notice (`fileOperations.transferProgress.close`/`.stallNotice`/`.stallWaitingDestination`/`.stallWaitingSource`/`.stallUnknown`/`.stallInFlight`/`.stallLogHint`)

- `Kein Fortschritt seit {duration}` leads with the state because it replaces the ETA line; `.stallNotice` must fit the
  narrow queue row.
- `Die Übertragung kommt nicht mehr voran.` over `steht still` (too final: it may recover) and anything with `Fehler`.
- `Details stehen in der Protokolldatei.`: naming the FILE keeps it apart from the `Vorgangsprotokoll`.
- „may already be partly written“ → `teilweise`, never MS's technical `partiell`.
- The two ways out are offered with `Du kannst …`, not a bare imperative: the line offers choices.

## Kopierter Pfad: die Zwischenablage-Bestätigung (`fileExplorer.clipboard.copiedPath`)

- The path shows on its own line below, so the sentence ends on a colon and stands without it. No `dein` before
  `Zwischenablage`: there is only one (macOS never says „in deine“).

## Operation queue: the queue window's rename (`queue.*`, `commands.queueShow.label`)

- `Vorgangswarteschlange`, a closed compound like `Vorgangsprotokoll` (the View menu pairs them); `queue.windowTitle`,
  the View menu item, and the bare `commands.queueShow.label` stay byte-identical.
- `Vorgang` is masculine: aria labels `Diesen Vorgang anhalten` / `… abbrechen`, toast pronouns `er` / `ihn`. The
  copy/move dialog's `transferProgress.pauseAria` / `.resumeAria` keep `Diese Übertragung` because the English says
  transfer there.
- `queuedToast` says `dieser hier` instead of `er`: in the one-branch a bare `er` would point at the operation ahead.

## Corner progress chip + the failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- Dismissing a failed row is `Ausblenden` (Dolphin „Diesen Hinweis ausblenden“): `Entfernen` reads as acting on the
  operation, and `Schließen` is the dialog sense.
- „Couldn't finish X“ → nominalized infinitive + `nicht abgeschlossen` (`Kopieren nicht abgeschlossen`), object-first
  like `queue.row.label`; the trash arm reuses `In den Papierkorb bewegen`. Never `abgebrochen` (the cancelled status).
- `queue.failureToast.summary` and the first sentence of `queue.chip.failed` share one rendering.
- „to see why“ goes inside both plural branches of `queue.chip.failed` (`den Grund` / `die Gründe`), unlike English.
- The chip tooltip is a `·`-separated fact list (`Wird kopiert · 214 Objekte · nach Backup · 42 %`): German has no
  appositive for „Copying 214 items“. Each optional part carries its own leading `·` inside its branch.
- Only the aria label spells out `Prozent`; the visible tooltip keeps `{percentText} %`.

## Standalone conflict prompt: the operation-context line (`fileOperations.operationConflict.context`/`.pausedNote`)

- Destination inside the clause, verb-final: `Wird nach {destination} kopiert` (Finder's own progress line). The chip
  can't do this because its `{label}` arrives pre-composed.
- `bewegen` takes `nach` here too, so copy and move stay parallel.
- `archive_edit` → `{destination} wird bearbeitet` (subject-first keeps the name nominative); the fallback arm is the
  impersonal `In {destination} wird gearbeitet`.
- `Alles andere ist angehalten, bis du antwortest.`: state passive, it describes now.

## The progress dialog's empty-queue button (`fileOperations.transferProgress.background`/`.backgroundAria`)

- `Im Hintergrund`, never the bare noun `Hintergrund` (a backdrop in every higher-tier source) nor `In den Hintergrund`
  (sending a window behind others).
- The aria `Im Hintergrund weiterlaufen lassen` starts with the visible label; keep the label a prefix of the aria.

## The quit gate (`main.quit.*`)

- Title: state, then question (`Ein Vorgang läuft noch. Trotzdem beenden?`), like TC, AppKit, and the catalog's tab
  prompt.
- The heading `Noch aktiv` (Finder's word) avoids a second `läuft noch` right under the title.
- The cancel button is `Weiterarbeiten`, never `Abbrechen` (Cmdr's cancel-the-operation verb, the opposite meaning) nor
  `Später` (nothing is deferred). `Jetzt beenden`, never `Sofort beenden` (Force Quit).
- `Cmdr beendet sich …` (active reflexive) with a trailing `darauf`, so the sentence says `Cmdr` once.
- `entfernt`, never `löscht`, on a reassuring dialog.

## Usage stats: "anonymous" dropped, "a random id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- `eine zufällige ID`, never `pseudonym(isiert)` or MS's `Bezeichner`: the English is deliberately everyday. Tied to →
  `verknüpft mit`.

## Conflict-parked queue rows and the rollback confirmation (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- `Antwort erforderlich`, terse like its sibling chips; nothing with `warten`, which is the queued status in the same
  column.
- The title uses the prose verb (`rückgängig machen?`), the confirm button the noun `Rollback` the user just pressed.
- `Dateien behalten`, not bare `Behalten`: the body also names the replaced files.
- `foregroundBusyToast` names `diesen Vorgang` and the `Anzeigen` button; English's „this one“ has no German antecedent.

## Rename chaining: the counted "and so did N others" toast (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- Gapping `„A“ behält seinen Namen, ebenso 3 weitere Dateien.` (Finder's `ebenso`) avoids verb agreement; `weitere` over
  `andere` like every counted tail; the `one` branch spells out `eine weitere Datei`.

## Unconfirmed rename + the catch-all name rejection (`fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

- `unconfirmed*` means Cmdr couldn't tell (the rename may have worked); never let it blur with `chainKept*` (it
  definitely kept its name).
- `Es ließ sich nicht bestätigen, dass „X“ umbenannt wurde` (the catalog's frame,
  `fileExplorer.pane.trashUnconfirmedToast`): the `dass` clause keeps `{name}` nominative, where a `von` noun frame
  forces dative plural branches. The plural branches reuse `chainKeptOriginalNameAndOthers` verbatim.
- `die Umbenennung hat also womöglich trotzdem geklappt` names its subject: a bare `sie` would point at the files. The
  timeout hedges (`womöglich trotzdem`, `Das Volume ist vielleicht langsam`) match
  `fileOperations.mkdir.timeoutMessage`.
- `Dieser Dateiname kann nicht verwendet werden` (Finder's catch-all), no closing period: the value is composed into
  `fileExplorer.rename.keptOriginalName`.

## Duplizieren: der Befehl, der im selben Ordner kopiert (`commands.fileDuplicate.*`)

- `Duplizieren` (Finder „Ablage > Duplizieren“), clearly apart from `Kopieren` (F5) and `Bewegen` (F6).

## Native Menüs: Menüleiste, Kontextmenüs, Fenstertitel (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- The German macOS Finder decides (`de.lproj/MenuBar.strings`, `LocalizableMerged.strings`), Safari for tab words, MS
  terminology only where Apple has nothing. Raw family: plain apostrophes.
- The Select menu is `Auswählen` (Nautilus/Thunar/Dolphin), matching `Alles auswählen`.
- Window > Zoom is the verb `Zoomen`, the text-zoom submenu the noun `Zoom`, so the two senses stay apart.
- `Im Finder anzeigen` in the menu too; `Im Finder zeigen` is macOS' word for „Reveal in Finder“.
- `Änderungsprotokoll` (the changelog document) vs `Neuigkeiten` (What's new, the message).
- `Zeilenumbruch` over MS's second option `Textumbruch` (the usual editor word).
- `Tab lösen` is the natural reverse of Safari's `Tab fixieren` (Apple has no unpin string).
- Tag row: `Tags`, `„{color}“ hinzufügen` / `„{color}“ entfernen` (Finder `TG5` / `TG6`).
- The busy marker is ` (in Benutzung)` appended to the unchanged base label, the same on every `*Busy` key: MS's
  `beschäftigt` / `besetzt` read wrong for a disk.

## SMB-Fallback-Hinweis: die Freigabe hängt an der Systemverbindung (`fileExplorer.network.osMountFallback.*`)

- `die eigene SMB-Netzwerkverbindung von macOS`: `nativ` is unattested in German UI; `Systemverbindung` is the set word
  for this connection.
- `Die direkte Verbindung zu X kam nicht zustande.` fills English's missing subject with the noun, never blaming Cmdr.
- `Direkte Verbindung versuchen`, never two stacked infinitives (`Direkt verbinden versuchen`).
- The notification's X is `Schließen` (shares a `sourceHash` with `lowDiskSpace.toast.closeTooltip`).

## Umbenennen und Anlegen: die abgewiesenen Mutationen (`errors.mutation.*`, `errors.volume.*`)

- `{path}` is an uncontrolled insert: always inside „…“ and behind its own preposition (`Unter „{path}“ gibt es …`).
- `Systemintegritätsschutz` (Apple Support DE) over Finder's one-off mistranslation `System-Integrationsschutz` (`ET6`);
  open in `review-queue.md`.
- „a volume's top folder“ → `der oberste Ordner eines Volumes` (Nautilus), not MS's Windows `Stammordner`.
- The short form „Out of space“ is Finder's `Kein Platz mehr`; the long sibling keeps „nicht genügend freier Speicher“.
- `timedOut` is an open question, not a failure: `… die Änderung klappt also womöglich trotzdem noch.` Nothing with
  `Fehler`, `fehlgeschlagen`, or `abgebrochen`.
- `deviceSessionReset` never mentions unplugging; it reuses `errors.listing.deviceReconnecting.*`.
- „lost track of“ → `aus den Augen verloren`: the folder still exists, so never „findet … nicht mehr“ (reads as
  deleted).
- The two archive refusals share the head `Beim Umbenennen kann ein Objekt …`; `Bewege es stattdessen.` points at the
  `Bewegen` command.
- `passwordRejected`: the password is the subject (`Das Passwort hat nicht geklappt.`), not the person.

### Die beiden Papierkorb-Absagen (`errors.mutation.trashNotSupported`/`.trashRefused`)

- The command and its prose say `endgültig löschen` everywhere (`commands.fileDeletePermanently.label`,
  `fileOperations.delete.noTrashWarningRest`); `dauerhaft` is only for a setting kept for good
  (`settings.fileViewer.suppressBinaryWarning.description`).
- `trashRefused` → `macOS wollte das nicht in den Papierkorb bewegen.`: keeps macOS as the one refusing, where
  `Die Datei ließ sich nicht …` (already `errors.write.ioError.message.trash`) would lose it.

## Absturzdialog: die drei Eröffnungssätze (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`, `crashReporter.dialog.privacyNote`, `crashReporter.dialog.title.*`, `crashReporter.sentToast.message.*`, `crashReporter.dialog.alwaysSend`)

- `.keptRunning` and `.unknown` must never say Cmdr crashed or quit.
- „ran into a problem“ → `ist auf ein Problem gestoßen`, never `Bei Cmdr ist ein Problem aufgetreten`: the MS German
  style guide lists `aufgetreten` as a bad example, and it makes Cmdr the object. Settled; don't reopen it with the MS
  guide in hand.
- `ist … weitergelaufen` is composed from the catalog's `weiterlaufen` (no source has a past-tense „kept running“).
  Never `hat weitergearbeitet` (claims it stayed useful) nor `wurde fortgesetzt` (`fortsetzen` belongs to a Vorgang).
- `.keptRunning` drops only `Absturz-` from `.ended`'s second sentence (`ein Bericht`); that one missing piece carries
  „nothing crashed“.
- The single `privacyNote` serves all three cases, so it says `auf das Problem gestoßen`, never a crash word.
- Titles and toasts: `Absturzbericht` only for a real crash (`.title.crash`), otherwise `Bericht`. `alwaysSend` stays
  `Absturzberichte immer senden`: it toggles the setting whose canonical label is `Absturzberichte senden`.

## Der Einstellungstext für Absturzberichte deckt jetzt beide Fälle (`settings.updates.crashReports.description`)

- The description covers both cases (`unerwartet beendet wird` / `im Hintergrund auf ein Problem stößt`) and says
  `einen Bericht`; the label `settings.updates.crashReports.label` stays `Absturzberichte senden`, the setting's
  canonical name.

## Auswerfen und Trennen: die neun Absagen (`errors.eject.*`)

- Each value lands after a colon in `fileExplorer.pane.ejectFailedToast` or `.disconnectFailedToast`, so it's a whole
  sentence that stands alone; the repeated `trennen` in the disconnect frame is deliberate.
- `drive` is `Laufwerk` here: the thing the user plugged in.
- `notEjectable` states the build (`Dieses Laufwerk ist kein Wechselmedium, …`), not a failure.
- `in Verwendung` (Finder `NE66`), never `gesperrt` / `belegt` (`gesperrt` belongs to locked).
- Files are closed, apps quit: `Schließe offene Dateien und beende laufende Apps` (German has no shared verb); Cmdr's
  noun is `App`, not Apple's `Programm`.
- `idle` → `nicht mehr beschäftigt`, the same word field as `fileExplorer.mtp.deviceBusy`.
- A disconnect is `trennen`, never Nautilus' Linux `aushängen`.
- „wouldn't“ (the other side refusing) → `wollte nicht`, as in `errors.mutation.trashRefused`.
- `busy` says `ein Vorgang von Cmdr`, never `Cmdr bewegt noch Dateien` (it also covers copy and delete).
- `timedOut` mirrors `errors.mutation.timedOut`; `unexpected` equals `errors.mutation.unexpected`.

## Papierkorb-Toast: Widerrufen und Zurücklegen (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- `Widerrufen` (AppKit, and the catalog's `askCmdr.renameUndo.undo`), never Nautilus' `Rückgängig`.
- Putting items back to their old place is `zurücklegen` (Finder „Put Back“); `zurücksetzen` restores NAMES
  (`askCmdr.renameUndo.undone`), not places.
- The toast says `Laufwerk` because the English says drive.
- The second half has its own `{skipped}` count, so it carries a real verb (`Objekt blieb` / `Objekte blieben`) and
  counts `Objekt`, as the English says item there.
- The toast button and the command are both `Zum Papierkorb gehen`, like `Zum übergeordneten Ordner gehen`.

## Fehlerbericht nachträglich ergänzen: der Notiz-Dialog (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- A note joins the SAME report; no second report goes out, and the wording must carry that.
- The title names `Fehlerbericht` once; buttons and toasts shorten to `Bericht`.
- One stem through the dialog: `hinzufügen` → `Wird hinzugefügt …` → `es kommt zu dem hinzu, was das Team schon hat`.
- `Was gesendet wurde` is the past-tense twin of `errorReporter.dialog.detailsToggle` (`Was gleich gesendet wird`).
- `Deine Notiz` without „(optional)“: a note or email is required here.
- `Bericht ansehen oder Notiz hinzufügen` keeps both halves; `ergänzen` is unattested and leaves open what gets added.
- `Notiz zum Bericht hinzugefügt.` mirrors `errorReporter.sentToast.message` and drops the possessive: two `dein` in a
  row read clumsily.

## Der Auswahldialog: Dateien auswählen und abwählen (`selection.*`)

- `menu.select.deselectAll` keeps Finder's `Auswahl aufheben` (the whole selection goes), while
  `menu.select.deselectFiles`, `commands.selectionDeselectFiles.label`, and `selection.dialog.title.remove` say
  `Dateien abwählen` (MS, Double Commander): Finder's phrase takes no object. Don't unify them, or the dialog title
  breaks against the menu item that opens it.
- `Letzte Auswahlen` mirrors `queryUi.recent.*` (`Letzte Suchen`); `selection.recent.applyAria` mirrors
  `search.recent.runAria`, `{query}` last after the colon.
- A terse key hint says `Enter` (`Zum Filtern Enter drücken`, like `search.runHint`); prose says `die Eingabetaste`.
- The tooltip is its own sentence, verb-final: the button's accessible name comes from the label key, so WCAG 2.5.3
  holds by construction.

## Der zugängliche Name muss die sichtbare Beschriftung enthalten (WCAG 2.5.3, `fileOperations.transferProgress.pause`, `queue.row.pause`, `queryUi.scope.toggle.caseSensitiveAria`)

- Give the LABEL the form the natural aria sentence uses: `Anhalten` sits inside `Diesen Vorgang anhalten`, which is why
  the button isn't the loanword `Pause`. `/` counts for containment.
- `Groß-/Kleinschreibung beachten` (the positive of macOS' „… ignorieren“); the aria wraps it
  (`Beim Abgleich Groß-/Kleinschreibung beachten`). A rephrased „Übereinstimmung mit …“ breaks containment.

## Ein englisches Wort, ein deutsches Wort: die Drift-Prüfung (`commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel`, `menu.bar.file`, `menu.bar.view`, `menu.window.zoom`, `menu.view.zoom`, `menu.tag.purple`, `settings.tint.purple`, `queue.row.dismiss`, `queryUi.bar.runLabel`, `ai.cloud.checking`, `licensing.dialog.checking`, `updates.status.checking`)

Unified:

- The F3 action is `Ansehen` everywhere (`commands.fileView.label`, `menu.file.view`,
  `fileExplorer.functionKeyBar.viewLabel`), never `Anzeigen` (the show sense) nor `Vorschau` (the viewer noun; the
  `@key` asks for a verb).
- `Go to …` stays `Zum … gehen` on buttons and in Settings too, like its command.
- `Verborgene Dateien einblenden`, never `versteckt` / `anzeigen`, matching `menu.view.showHiddenFiles`.
- In-progress states take `Wird …` (`Wird gesucht …`); only `fileExplorer.navigation.spaceRetryingText` keeps the short
  noun `Erneuter Versuch …`, because it replaces a number in the narrow space bar.
- `Alles klar` for Got it (macOS); `Von:` before a path; the paused state is `angehalten` in sentences too.

Deliberate splits (don't unify):

- `Ablage` is the menu bar's File, `Datei` everything else; `Darstellung` is the View MENU, `Ansehen` the action.
- `Zoom` is the text size, `Zoomen` the window command.
- `Lila` is the Finder tag color (`menu.tag.purple`), `Violett` Cmdr's own tint (`settings.tint.purple`).
- `Wird geprüft` checks a connection or key; `Wird gesucht` looks for updates (Software Update's idiom).
- `Schließen` closes a window; `Ausblenden` hides a row (`queue.row.dismiss`).
- `Suche` is the noun; `Suchen` is the run button (`queryUi.bar.runLabel`).
- `Zuletzt verwendet` only as the command palette's group title (elsewhere `Letzte`); `Hinweis` for Cmdr's own toast,
  `Benachrichtigung` for the settings category.

## Wörter, die auseinandergelaufen sind, ohne dass ein Check es sehen konnte (`settings.fileOperations.allowFileExtensionChanges.*`, `askCmdr.wake.stop`, `settings.mediaIndex.clip.comingSoon`, `settings.whatsNew.lastSeenVersion.label`)

- `erlauben` in Settings too, never `zulassen`: the dialog that writes this setting
  (`fileExplorer.extensionChange.alwaysAllow`) says `erlauben`.
- `askCmdr.wake.stop` says `stoppen`: `anhalten` is the catalog's pause, a false promise where a transfer can really
  pause.
- `Bald verfügbar` (never `Demnächst`), and `Änderungsprotokoll` inside compounds too.

## Die Bereichsnamen kommen jetzt vom Mac des Nutzers (`errors.git.*`, `errors.provider.*`)

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` render the user's own macOS names, so a
  preposition may precede them but never an article (`Öffne {system_settings} und …`, not „in den …“).
- `Apple Account` stays English without a hyphen (macOS 26 de, prose included).
- `Allgemein` and `Anmeldeobjekte & Erweiterungen` have no token and are plain text (macOS 26 de).

## „Zurücksetzen“ nennt jetzt das Objekt: den alten Namen (`askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`, `menu.app.hideOthers`)

- `Die alten Namen von N Dateien zurückgesetzt.`: `zurücksetzen` restores names, `zurücklegen` stays with the trash.
- `Alle einblenden` / `Andere ausblenden` are macOS' own app-menu words.

## Ein halb zurückgenommener Vorgang: das Rollback zu Ende bringen (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- `Rollback abschließen` (never restart; `fortsetzen` belongs to `queue.row.resume`), identical in both keys that share
  the English. The button and the `Rollback abgeschlossen` badge never share a row.
- The notice echoes `fileOperations.rollbackConfirm.bodyUndoByDeleting` and promises no complete undo.
- `in „{folder}“` needs the quotes: without them „wird gelöscht in Backup“ reads as if the folder itself goes.

## Der Toast nach einem abgebrochenen Vorgang: was die Rücknahme geschafft hat (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Tone: Cmdr did the careful thing; never apologetic, never alarming. `Habe {name} unverändert gelassen` shares its
  voice with `askCmdr.renameUndo.skipReason.*`.
- `Objekt` even where the rename family says `Datei`: the undo also removes created folders.
- Three verbs stay apart like the English: `entfernt` (removed), `zurückgelegt` (put back), `löschen` (delete, in the
  confirmations).
- „the N items“ becomes a colon appendix (`1.234 Objekte entfernt: alles, was Cmdr geschrieben hatte.`): an article
  would give `Das 1 Objekt`.
- `Gestoppt, nachdem Cmdr … entfernt hat`, verbal over a noun pile.
- `am neuen Ort` / `am alten Ort`: `Ort`, because `Platz` is storage space; no possessive, so no gender bet on `{name}`.
- `seit Cmdr es dort abgelegt hat` (`seit`, like `askCmdr.renameUndo.skipReason.drift.*`).
- Both `folderNotEmpty` values equal `askCmdr.renameUndo.skipReason.folderNotEmpty.named` / `.counted`: change both or
  neither.
- `leftBehind` says `überspringt alles`, the same promise as the confirmation it follows.
- `rollbackConfirm.body` drops `davon` from its last sentence: right after „ersetzte Dateien kommen nicht zurück“ it
  would promise the opposite.
- `Cmdr konnte {name} nicht rückgängig machen` gives English's subjectless line a subject.

### `cancelRollback.stagedLeftover.*` (Cmdrs eigene Reste am Ziel)

- Cmdr's own leftover, not the person's file, so `Cmdr räumt sie weg` rather than `löschen`.
- `bei einer späteren Übertragung`, never „beim nächsten Mal“: cleanup skips anything under an hour old, so an immediate
  retry clears nothing.

## Der Block-Screen bei zu altem WebKit (`main.oldWebkit.*`)

- `Softwareupdate` in one word (the pane's name), never the generic `Software-Update`.

## Der Hinweis auf zu altes macOS (`main.oldMacos.*`)

- Honest and relaxed, no apology or warning: the app runs. `macOS 12 und neuer` (macOS' `oder neuer`).
- `best effort` → `nur so gut, wie es eben geht`, never contract German like „nach bestem Bemühen“; `look off` →
  `daneben liegen`.

## Ask Cmdr schaut jetzt in Dateien hinein: Zustimmungstexte und Werkzeugzeilen (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- The tool line is `Dateien werden durchgesehen`, never `durchsuchen` nor `Dateiinhalte werden gelesen`: `Dateiinhalte`
  is what Cmdr does NOT read (`askCmdr.empty.hint`). Prose says `hineinschauen`.
- `Miniatur` (Preview, Photos), never `Vorschaubild`: `Vorschau` is the viewer and Apple's app.
- `Kameraangaben` follows the catalog's `Dateiangaben` for metadata; the English avoids `EXIF`, so German does too.
- `Aufnahmeort`: a bare `Ort` next to files reads as the storage location.
- `Autor`, not Apple's gender-colon `Autor:in` (our no-glyph rule); open in `review-queue.md`.
- `was in einem Archiv steckt` keeps the English's loose „what's inside“.
- `contentsRule` no longer promises „keine Dateiinhalte“: it says `nie ganze Dateien, Fotos oder Miniaturen` and
  `einen begrenzten Teil davon`; its last two sentences keep the retired `askCmdr.consent.noContents` wording.
- „looks inside a file only when you ask“ → `schaut nur dann in eine Datei hinein, wenn du nach ihr fragst`
  (`askCmdr.empty.hint`, `settings.askCmdr.intro`).

## Die zwei Tooltips der Rollback-Taste (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- `Stoppen und alle bisher bewegten Dateien zurücklegen`: undoing a move deletes nothing, so never `löschen`.
- `.rollbackAlreadyLandedTooltip` quotes the neighbor button `Abbrechen` verbatim.

## „Terminal hier öffnen“ und seine App-Auswahl (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `Terminal` stays English (Finder „In Terminal öffnen“); the command is `Terminal hier öffnen` wherever it appears.
- `App auswählen …` over Apple's „Programm auswählen …“: the catalog says `App` throughout.

## Nach Relevanz sortieren: der Tooltip der Suchergebnis-Spalte (`fileExplorer.columns.sortByRelevance`)

- `Relevanz` (Automator), never `Häufigkeit`, which other Apple catalogs use but means frequency and would promise the
  wrong order.

## Dokumente und Pakete: die OOXML-Zeile (`settings.archives.ooxml.*`)

- The row covers Office documents and .jar/.apk, so bare `Pakete`, kept broader than the `App-Pakete` card below; the
  sentence frame copies `settings.archives.zip.description`.

## Der Server-Hub: Verbindungszustände, Trennen und Vergessen (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`)

- Disconnecting separates the connection, not the server: `Cmdr konnte die Verbindung zu {name} nicht trennen.`, aria
  `Verbindung zu {name} trennen` (Apple's FileProvider frame), never `{name} trennen`.
- `disconnectBusyTooltip` mirrors `ejectBusyTooltip` (`… nicht möglich, während auf diesem Server Vorgänge laufen`).
- `Die Verbindung wurde unterbrochen.`, never `abgebrochen` (the user-cancelled status).
- A certificate or key takes the dative: `macOS vertraut dem Zertifikat … nicht` (Apple's Trust strings, kept active).
- The host key in prose is plain `der Schlüssel` with `von {host}`: never `Hostschlüssel` (jargon the English avoids)
  nor `{host}s Schlüssel` (a placeholder never takes a case ending).
- `Anmeldemethode` over Apple's `Authentifizierungsmethode` (jargon the `@key` wants avoided).
- `Das Passwort hat für {username} nicht geklappt.`, never Apple's `ungültig`: nobody gets blamed.
- The sheet's field is the short `Adresse`; prose says `Serveradresse`.
- Back-references say `den Server`, never `ihn`: in `connectionTooltipNeedsHostKey` two masculine nouns precede it.
- The dialog titles equal their menu items (`menu.network.forgetServer`, `menu.network.forgetSavedPassword`).

## Die Server-Übersicht: Spalten, Zustände und die Zeile im Volume-Umschalter (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- The switcher row and shortcut scope are `Server`; the group above stays `Netzwerk`, so row and group differ.
- `Zuletzt benutzt` (Finder's recent servers) is the column; `Zuletzt verwendet` stays the palette group,
  `Zuletzt geöffnet` the file date.
- `Gespeichert`, never Podcasts' `Gesichert` (downloaded episodes there).
- `Wartet darauf, dass du den Schlüssel prüfst`, a verb over a noun pile.
- „Local network discovery is off.“ → `Die Suche im lokalen Netzwerk ist ausgeschaltet.`
- `Füge unten einen hinzu, …` then `das Gerät`: „a Mac or a NAS“ mixes genders, so no pronoun fits both.
- `servers.hub.rowCount` has identical CLDR branches (`{countText} Server`): the plural has no ending, not a forgotten
  branch.
- „Disconnect server“ → `Verbindung zum Server trennen`, never `Server trennen`.
- „volume chooser“ and „volume switcher“ are both `Volume-Auswahl`: one control, one name.
- `Places` → `Orte` (Finder), covering buckets too.

## Das Verbindungsblatt, der Hostschlüssel und die zwei Vorschauzeilen (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

Apple's own Connect to Server dialog (`NetAuthAgent.app` `AuthDialog` / `Localizable.loctable`) is the same surface and
the first source.

- `Bei {name} anmelden`: the verb goes last.
- `Mit Benutzername und Passwort anmelden` names the action: Apple's `Registrierte:r Benutzer:in` uses a gender colon.
- The checkbox `Im Schlüsselbund merken` (`servers.sheet.remember`) is quoted verbatim by
  `servers.sheet.needsStoredSecret`, which also opens with the other checkbox's label; one word family per dialog
  (`merken` → `gemerktes Passwort`).
- `Durchsuchen …` is the file-picker button (Finder); `settings.archives.opt.browse` is `Durchsehen`. The split is real
  and allowlisted in `apps/desktop/scripts/i18n-term-consistency-allowlist.json`: a new file-dialog button takes
  `Durchsuchen …`, a new archive surface `Durchsehen`.
- `Entfernter Ordner` (macOS renders every „Remote X“ as `Entfernter X`).
- `Schlüssel-Passphrase`, not Apple's usual `Passwort`: the `Passwort` field sits right above, and the `@key` rules out
  that mix-up. Open in `review-queue.md`.
- `Dem neuen Schlüssel vertrauen` (dative, like Apple); „something is sitting between you and it“ →
  `sich zwischen dich und den Server schalten`, never the jargon `Man-in-the-Middle`.
- The legend is `Verbindungsmodus`: German legends name the control, like the neighbor `Protokoll`.
- `Erste Verbindung zu {host}` describes a situation (routine, not alarming), never an imperative.
- `Der Schlüssel von {host} hat sich geändert` (analytic genitive, like `servers.refusal.hostKeyRevoked`).
- `Ich habe ihn geprüft` (first person per the `@key`); `ihn` is safe because both candidates are masculine.
- „the server's owner“ → `die Person, die den Server betreibt`: `Betreiber` / `Besitzer` are generic masculine.
- `Cmdr hat die Verbindung zu {name} gestoppt`, never `abgebrochen` (reads as the user's Cancel).
- The Go to path preview lines stay third person (`Öffnet {name}`): they describe the input's effect.

## Der Wiederverbindungs-Zyklus und die Anmeldung per Schlüssel (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- `Verbindung zu {name} wird wiederhergestellt …` beside the sibling `Verbindung zu {name} wird hergestellt …`;
  `wieder-` carries the difference. Apple's short `Erneut verbinden …` fits only without a target: with `{name}` it
  breaks.
- `Bei diesem Server meldest du dich mit einem Schlüssel an …, du musst also nichts eingeben.`: one subject throughout;
  `eingeben` is Apple's verb for filling a field (NetAuthAgent `FS_MSG_PASS`), never `tippen` (a gesture in macOS de).
  `ein Schlüssel`, never the jargon `SSH-Schlüssel`.
- `Öffne den Server erneut, …`: `ihn` would be ambiguous after the masculine `ein Schlüssel`.
- The link under it is `In den Einstellungen einschalten`, the same word family as `ausgeschaltet`.

## Server fixieren und lösen, die vertrauten Hostschlüssel und die ADB-Seite (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

- `fixieren` (Safari „Tab fixieren“), never Notes' `anpinnen`; `Lösen` alone in the context menu, never `Loslösen`.
- `In der Volume-Auswahl fixieren`: the surface's known name outweighs brevity; Finder keeps the article („Zum Dock
  hinzufügen“).
- `Erneut prüfen` (Software Update's „Check Again“), never `Erneut suchen`: nothing new is searched.
- The attribute is `vertrauenswürdig`, the verb `vertrauen` (Apple does the same). The date prefix is
  `Vertrauenswürdig seit <Datum>`: a bare `Vertraut` also means „familiar“.
- `Cmdr achtet auf Telefone.` / `… gerade nicht auf Telefone.`, a pair; the `@key` bans any ADB-server or socket talk.
- `Den Befehl „adb“ auswählen` mirrors `settings.behavior.openTerminalHereApp.chooseAppTitle`; `settings.adb.browse`
  copies `servers.sheet.browse` (`Durchsuchen …`).
- A group in prose is `die Gruppe „Netzwerk“`, like a menu, with its name from `fileExplorer.navigation.groupNetwork`.
- A request says `Klicke einen Server mit der rechten Maustaste an`; the noun `Rechtsklick` stays for tooltips.
- `Der Server bleibt in der Liste „Server“.`, never `Er`: the masculine `der Befehl` precedes it.
- The help texts front the „first time you connect“ clause (`Wenn du dich zum ersten Mal …`).
- `Bisher vertraust du keinem Schlüssel.`, a whole sentence over a participle fragment.
- The tint label is `Serverbereiche einfärben (SMB, SFTP, WebDAV)`, like `settings.appearance.tintMtp.label`.

## Das Telefon über ADB öffnen: Bereichsmeldungen, Zeilen-Tooltips und der Hinweisstreifen (`adb.*`, `settings.behavior.adbHintDismissed.*`)

For Android's own words, Google's German (AOSP `values-de`) is the authority, not Apple.

- Android's button is `Erlauben` (AOSP `usb_debugging_allow`), never `Zulassen` (the Wi-Fi variant).
- `tippen auf „…“` for the finger gesture, as Android writes it; entering text into a field stays `eingeben`.
- `Android Platform Tools` (the product) vs the generic `die Android-Tools`.
- `USB-Anschluss` / `ein anderer Anschluss`, never `Port` (macOS keeps that for network port numbers).
- A phone on a cable `reagiert nicht` (Apple's device verb); `antwortet nicht` is for the network (`servers.refusal.*`).
- Waking a screen is `aktivieren`, never `aufwecken` (macOS de keeps that for people).
- `adb.connect.deviceTooOld` puts Cmdr first and the reason in a `da` clause (Apple's „…, da seine Software zu alt
  ist“).
- `phone` → `Telefon`, `device` → `Gerät`, following the English key by key.
- `Disconnect {name}` equals `fileExplorer.navigation.disconnectPlaceAriaLabel` (same English).
- The hint line's × is `Ausblenden`: it hides the line for good, where `osMountFallback.closeTooltip`'s notice returns
  (`Schließen`). The line runs between closing and hiding for good, not between surfaces.
- `Warten darauf, dass du USB-Debugging erlaubst`, no ellipsis like its sibling tooltips.
- `Möchtest du das gesamte Dateisystem sehen?`: a German question needs its verb. `Schalte USB-Debugging ein.` (the
  catalog's switch verb).
- `How` → `Wie geht das?`: a bare `Wie` reads cut off.
- `Sieh auf dein Telefon …` means look at it; Apple's `Überprüfe` means check a setting.
- `adb.connect.cancelled` → `Du hast das Öffnen deines Telefons gestoppt.`, like `search.coverage.walk.cancelled`:
  `abgebrochen` would read as a pointer to the `Abbrechen` button.

## Der veraltete Index eines Telefons (`driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

- The phone is still connected, so never `getrennt`; the title equals `indexing.staleDialog.title` with `Telefons`.
- `{name} teilt Cmdr nicht mit, …`, never `meldet Cmdr nicht` (reads as an accusative); `Dateien darauf`, no possessive.
- `bleibt zur Erinnerung stehen`: `Hinweis` is the catalog's toast word.

## Die gesperrte Server-Identität (`servers.sheet.identityLocked`)

- `Konto`, never `Account` (that is Apple's `Apple Account`).
- The hint names the actions exactly as their buttons do (`vergessen`, `hinzufügen`); a synonym points at a menu item
  that doesn't exist.
- „are what name this server“ → `machen diesen Server aus`: the sheet has its own `Name` field.

## Der Toast, wenn gar kein Passwort gespeichert war (`fileExplorer.navigation.forgetSecretNoneToast`)

- `Für {name} war kein Passwort gespeichert.`: the participle as predicate avoids form-speak („kein gespeichertes
  Passwort vorhanden“); past tense, no regret (nothing went wrong).

## Die Wiederholdauer, die Host-Key-Überschrift und Androids Erlauben-Knopf (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`/`.retryKeepsTrying`/`.hostKeyChanged`, `adb.readiness.waitingForAuthorization`)

- The two duration keys are bare fragments for `retryKeepsTrying` (`Es wird insgesamt {duration} lang weiterversucht.`):
  no preposition, no period. `{seconds}` picks the branch, `{secondsText}` is read.
- `Cmdr verbindet sich nicht mit {name}`, present tense like `servers.refusal.hostKeyRevoked`: a standing refusal, not a
  stopped attempt.
- `antippst` over `tippst auf`, avoiding two `auf` in a row.

## Das Kontextmenü der Server-Zeile: Öffnen und Server bearbeiten … (`menu.network.open`, `menu.network.edit`)

- `Öffnen` equals `menu.file.open` (Finder uses one verb for both senses); `Server bearbeiten …` copies
  `commands.serversEdit.label` byte for byte.

## Das Dock-Angebot (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Apple's Dock menu (`Dock.app/Contents/Resources/de.lproj/DockMenus.strings`) supplies almost every phrase.

- `das Dock` is neuter; the Dock pair is `im Dock behalten` / `aus dem Dock entfernen` (`KEEP_IN_DOCK` /
  `REMOVE_FROM_DOCK`), never `fixieren` / `lösen` (tabs and servers).
- `Ja, zum Dock hinzufügen` drops English's „my“: Apple keeps the article, never a possessive.
- `Ordner „Programme“`, with `Ordner` first, reads best in prose.
- `Wer diesen Mac verwaltet`, never Apple's gender-glyph `deine:n Netzwerkadmin`.
- `seit ein paar Tagen`, never a number: the threshold can move.
- `addedButDockDidNotRestart` leads with the result (already in the Dock) and puts the catch in a clause.
- Back-references go through `das Symbol`, never a pronoun on Cmdr.
- `Nein, danke`, never `Später` / `Nicht jetzt`: Cmdr never asks again.

## Das Dock-Menü von Cmdr (`menu.dock.*`)

- The Dock menu takes Apple's wording (`DockMenus.strings`, Finder's Go menu): `Cmdr öffnen` (name first, no quotes on a
  product name), `Gehe zu Ordner …` even though Cmdr's menu bar calls the same dialog `Zu Pfad gehen …` (the English
  splits the two surfaces too).
- `Dateien suchen …` equals `menu.edit.searchFiles`; `Mit Server verbinden …` equals the palette entry.
- `{name} ({parent})` stays identical (Finder `IN_G6_V1`), with a `sameAsSourceJustification`.

## Das „Im Finder anzeigen“-Angebot und der Ersttreffer-Hinweis (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `„Im Finder anzeigen“` in quotes, exactly as `settings.navigationAndFileOps.card.showInFinder` names it.
- `schon eine Weile`, never a number (the threshold can move).
- The first-hit notice says what happened and where the switch is; no `Entschuldigung`, no `Leider`.

## Die Einführungs-Checkliste, der Schritt-Tooltip und die vier Kurzfazits (`onboarding.*`)

- `Sichern` for the Save button (shares a `sourceHash` with `servers.sheet.save`); its participle nearby is `gesichert`
  (`E-Mail-Adresse gesichert`), while plain storing of data is `gespeichert`.
- `Vergib dem Repo auf GitHub einen Stern` (GitHub's German docs call the button `Stern`; the UI isn't localized).
  `Gib Cmdr auf AlternativeTo ein Like`: the site shows an English `Like`, and MS's `gefällt mir` is too long for a
  link.
- `Checkliste` over the bureaucratic `Prüfliste`.
- `Mailingliste`, never MS's Exchange `Verteiler`; the list is the subject that refused the address, not Cmdr.
- The permission is `Lokales Netzwerk` (its System Settings name), never the descriptive „Zugriff auf das lokale
  Netzwerk“.
- One-line summaries say `Platz`; prose keeps `Speicherplatz`.
- `Mehr über {topic}` has no article: `{topic}` is a translated label of unknown gender.
- The four summaries stay verb-initial and parallel, each reusing its `…desc` sibling's words.
- `<field></field>` sits between object and separable particle (`Gib deine E-Mail-Adresse <field></field> ein, um …`).
- `stepTooltip` keeps the literal `+1`; its `select` branches start with a comma.
- `dumber` stays `dümmer`: softening it changes what David says.
- `stepAi.local.tooltip`'s `<strong>` quotes `stepAi.cloud.label` verbatim; change one, change both.
- `settings.revealHandler.notProductionBuild`: `veröffentlichte Version` (a release, not a second process) and
  `Dev- und Test-Builds`.

## Die Vorschau holt eine Datei erst herüber (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Fetching is `laden` (Finder „Laden …“), never MS's `abrufen`; `{fileName}` is the unquoted nominative subject, like
  `downloads.notification.title`.
- `{doneText} bisher geladen` when the total is unknown.
- `Von dieser Datei kommen keine Daten mehr an.`, not the stall line: the viewer shows no operation.

## Stamm- und Startordner eines gespeicherten Servers (`servers.sheet.rootFolder`/`.startFolder` samt `…Help`, `servers.sheet.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

- `Stammordner` (MS) names this field; „a volume's top folder“ elsewhere stays `der oberste Ordner`, as it's not a field
  name. `Startordner` (Dolphin).
- `ein Ordner darin` dodges a pronoun between two masculine nouns.
- `Prüfe, ob es ihn gibt und ob dein Konto ihn lesen darf.` (`darf`: missing rights are the usual cause).
- `deshalb hat Cmdr nichts gesichert` follows the `Sichern` button.
- `benennen` fits `nameHelp` (it really is about the `Name` field), unlike `identityLocked`.

## Warum eine Freigabe nicht eingebunden wird oder die Freigabenliste nicht lädt (`errors.mount.*`, `errors.shareList.*`)

- Apple's `NetAuthAgent.app` `Localizable.loctable` phrases these exact cases and is the first source.
- `„{server}“ zeigt Gästen keine Freigaben` avoids a possessive on the placeholder.
- A network share `antwortet nicht`; a device on a cable `reagiert nicht`.
- `dieser Computer`, never `Mac`: these lines also show on Linux.
- Back-references say `der Server`; `mountRefused` says the server refused, so nobody blames the password.
- Identical English, identical German: `errors.mount.hostUnreachable` = `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` = `errors.shareList.authFailed`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `Text-Editor` is the app kind, never Apple's TextEdit (that arrives as `{app}`); the default is `Standardeditor`
  (`commands.fileEdit.label`).
- `Dateien bearbeiten mit` runs into the dropdown; `in {app}` without an article fits any app name.
- The shared strings equal their `openTerminalHere` siblings.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- `wird gerade getrennt` while the eject runs; `wurde getrennt` once it's gone.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- `wurde getrennt` (done), never the in-progress `wird gerade getrennt`; `startet von vorn` like
  `indexing.rescan.incompletePreviousScan`.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

- The second sentence says where the files are now and ends the line, where German puts its stress; never shrink it to a
  bare fact.
- The drive was pulled, not ejected: `wurde getrennt`, nothing from the `auswerfen` family.
- `{volumeName}` and `{counterpart}` both hang on `auf`, so no `Quelllaufwerk` / `Ziellaufwerk` and no article on an
  unknown name; „to it“ → `darauf`.
- Every sentence says where something `liegt`; `es ist also nichts verloren gegangen` (Apple's `verloren gehen`, perfect
  tense).

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

- Not a failure: nothing is lost, and Cmdr kept the originals because it couldn't prove the write. Never upgrade
  „couldn't confirm“ to „ist schiefgegangen“.
- `Die Bewegung ließ sich nicht bestätigen`: `bestätigen` is acknowledging, `prüfen` is looking. `die Bewegung` names
  this ONE move; the „no noun for moves“ note applies only to lists of operation kinds.
- „were saved on“ → `geschrieben`, never `gesichert` (the button word).
- The reassurance ends the sentence (`deshalb hat es deine Originale dort gelassen, wo sie waren`); „haven't moved“ →
  `liegen noch dort, wo sie waren`, since `bewegt` would echo the command name.

## Ein wiedergefundener Staging-Ordner (`fileOperations.leftovers.stagingFolderKept`)

- The person's own files, possibly the only copy: reassurance, never advice to delete.
- `eine nicht abgeschlossene Bewegung`; `unvollständig` stays for Cmdr's own `unvollständige Kopie`.
- `sie an ihrem Platz gelassen` says Cmdr chose it; `liegen geblieben` reads as neglect.
- `verborgen`, never `ausgeblendet` or Tier 3's `versteckt`.
- `auf {volumeName}` and `namens {folderName}` keep the names uninflected; a colon replaces English's comma so the line
  ends on the folder name.

## Das Favoritenmenü (`menu.go.showFavorites`, `commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`, `shortcuts.scope.favoritesMenu`)

- `Favoriten` (Finder), never `Lesezeichen` (bookmarks). TC's `Verzeichnisliste` and DC's `☆-Tabs` name other features,
  so macOS stays the source even though Cmdr shares TC's key.
- „Show favorites“ → `Favoriten anzeigen` like `Server anzeigen`; `einblenden` is for toggles. „See N favorites“ →
  `… ansehen`, keeping see apart from show.
- The `=0` arm drops `{count}` so no „0“ shows.
- `eine eingebundene Freigabe`, never `gemountet`; `Ordner auf einem Laufwerk`, never `Volume` (plain-speech tooltip).
- The `0` row has ONE key, `fileExplorer.navigation.favoritesAddCurrent`, which the shortcut list quotes; don't invent a
  second version. `Favorit` is weak (`den Favoriten`).
- `mit einer Zifferntaste` says a key is meant, clearer than „eine Zahl drücken“.
- `favoritesCantAddHere` states a fact about THIS folder (`Dieser Ordner kann kein Favorit sein: …`), mirroring
  `favoritesAlreadyAdded`; a plain locative `auf Laufwerken … funktionieren` avoids forcing a case on the target.

## Wer das Laufwerk festhält: die sechs benannten Absagen (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- Same frame and rules as `errors.eject.*`; `unmountRefused` / `…ByApp` / `…ByApps` read as one family.
- `{app} verwendet dieses Laufwerk noch`: the name leads as nominative subject, right after the frame's colon (a
  lowercase tool name like `mds_stores` at the start is fine). Never `belegt`, `blockiert`, `greift zu`.
- No pronoun back to the holder in either line (`Schließe alles, was dort geöffnet ist, …`): a pronoun on `{app}` is
  barred, and one tail keeps the family uniform.
- `{apps}` stays the subject: `Intl.ListFormat('de')` ends on nominative `andere Apps`, which a dative slot would break.
- `Image`, never `Disk-Image` (drifts from two shipped keys) nor `Datenträgerabbild` (not macOS).
- `Auf diesem Laufwerk liegt ein Image, …` mirrors `errors.eject.busy`; `Wirf erst das Image aus, dann das Laufwerk.`
  gaps the second verb.
- „working with“ → `arbeitet noch mit`, not `verwenden`: the English itself distinguishes it (Spotlight, Time Machine).
- `Cmdr selbst verwendet …` is a handle Cmdr failed to release; `busy` is a running operation. Keep them apart.
- `sende einen Fehlerbericht`: the full name, since it's the only mention.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `Alle mit Endung *.{extension} auswählen`: the mask hangs on `mit`, so nothing agrees with it.
- `menu.context.selection` is the noun `Auswahl` (the submenu's title), never the verb `Auswählen` (`menu.bar.select`).
- Each `menu.select.*` / `commands.selectionSelectSameKind.*` label pair shares one English string; reword neither
  alone.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

- The pill names the pane (`Kein Festplattenvollzugriff`), shortest in a fixed-height title bar; it shares its English
  with `onboarding.stepAi.bannerTitle.denied`.
- The aria opens with the label verbatim; its second half is a `zum` dative, so containment rides on the first sentence.
- Names say `Festplattenvollzugriff`; running prose (`onboarding.stepFda.revoked.noAccess` and siblings) may keep the
  everyday `vollständiger Festplattenzugriff`.
- `Dateien …, die macOS für sich behält`, never a macOS feature name.

## The trash refusal dialog (`errors.write.trashRefused.*`)

- The title shares its English with `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash`: reword all five or none.
- No plural param, so `{count} der ausgewählten Objekte` works at 1 and at 7.
- `suggestion.other` reuses `fileOperations.errorDialog.technicalDetails` (`die technischen Details`).
- `geschützt` like every sibling, not Finder's `Gesperrt`.
- The title-bar pill is `der Hinweis` (not `Statussymbol`, the file-row marker), and its quoted text is
  `onboarding.fdaBadge.label` verbatim.

## Der Online-Warnhinweis im Löschdialog (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Four facts must survive: the trash would download the files; Cmdr only offers deleting the WHOLE selection; the trash
  keeps NO copy though the service has its own (never soften that); the ways out the banner names. `AllWarning` drops
  the „abwählen“ way out.
- Both `<strong>` spans stay; `„Löschen“` quotes `fileOperations.delete.confirmDelete`. `HandedBack` is matter-of-fact,
  no apology.

## Wenn der Server die Freigabe gar nicht kennt (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- The one case where retrying can't help: nothing temporary (no „gerade“, no „noch einmal versuchen“);
  `Das erledigt sich nicht von selbst`.
- „a lot slower“ stays unquantified (`deutlich langsamer`): the English gives no factor here.
- The toast's `sie` points at `diese Freigabe`; `{server}` never gets a pronoun in this family.

## Namen, die gleich aussehen (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.listing.ambiguousName.explanation`, `errors.listing.ambiguousName.suggestion`, `errors.volume.ambiguousName`)

- `der Server schreibt sie unterschiedlich` / `speichert sie in unterschiedlicher Schreibweise`: plain German, no
  Unicode jargon, as the `@key` asks.
- `zwischen Groß- und Kleinschreibung unterscheiden` keeps its `zwischen` (Finder).
- `„Überschreiben“ ersetzt das Objekt, das schon da ist.` quotes `fileOperations.transferProgress.conflictOverwrite`; a
  relative clause over the stiffer `das vorhandene Objekt`.

## Der Schalter „Cloud-KI erlauben“ und die Zustände „Cloud-KI ist ausgeschaltet“ (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

- The switch is `Cloud-KI erlauben`, quoted with „…“ when named; where English uses it as a verb, `Erlaube … Cloud-KI`
  without quotes. Pronoun `sie` (die KI).
- „X is off“ → `X ist ausgeschaltet`, and „turn it on“ in prose → `schalte … ein`: the everyday pair. Buttons and
  setting names keep `aktivieren` (`Ask Cmdr aktivieren`).
- `Seitenbereich`, never `Seitenleiste` (the Finder sidebar).
- `Ask-Cmdr-Chats` is fully hyphenated; `Die eigenen Server von Cmdr` (no `Cmdrs`).

## Vollbildmodus und esc-Taste (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `Vollbildmodus` in sentences (Apple keeps bare `Vollbild` for the window tile); the key is `esc-Taste`, lowercase like
  Apple's keycap.
- `Vollbildmodus mit esc-Taste beenden` on both switches (shared `sourceHash`).
- `… hat Cmdr aus dem Vollbildmodus geholt`: casual, no blame.
- `nur den Dialog oder das Menü`: masculine and neuter share no pronoun.

## Geänderte Originale nach dem Bewegen (`transfer.changedDuringMove`, 2026-09-25)

- **„changed during the move“ → `wurde/wurden während des Bewegens geändert`** · Finder `PE56` „… da mindestens ein
  Objekt während des Brennens geändert wurde“ (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle,
  2026-09-25); `während des Bewegens` und `Quellordner` wörtlich aus dem Geschwister `transfer.appearedDuringMove`, weil
  beide Sätze im selben Toast stehen · `high`.
- **„stays in {folderName}“ → `bleibt/bleiben in {folderName}`** · Verb in eigenem Plural-Block wie beim Geschwister ·
  `high`.

## Platzhalterzeilen in „Öffnen mit“ und „Teilen“ (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Platzhalterzeilen in „Öffnen mit“ und „Teilen“ (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- `Optionen zum Teilen`: `Freigabe` is the network share, and `Teilen-Optionen` reads clumsily.
