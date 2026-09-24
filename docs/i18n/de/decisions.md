# de decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`); open questions for a native reviewer live in `review-queue.md`. Style and voice: `style.md`.

Sources are mined from the reference pile (`_ignored/i18n/de/`) or, where the pile is missing, from the installed macOS
bundles (`../reference-pile/how-to-mine.md` § No pile on this machine?). Tier order: macOS, then Microsoft terminology,
then the file-manager catalogs.

## Einstellungsabschnitte (`settings.section.*`)

Die Abschnittsnamen stehen in Verweisen quer durch den Katalog („unter Einstellungen > KI“), also müssen sie überall
zeichengleich sein. Hergeleitet aus den Termbase-Regeln und dem Wortlaut der macOS-Systemeinstellungen:

- Appearance → Erscheinungsbild, Colors and formats → Farben und Formate, Zoom and density → Zoom und Dichte, File and
  folder sizes → Datei- und Ordnergrößen, Listing → Dateiliste, Behavior → Verhalten, Navigation & file operations →
  Navigation & Dateioperationen, File system watching → Dateisystemüberwachung, Search → Suche, AI → KI, File systems →
  Dateisysteme, SMB/Network shares → SMB-/Netzwerkfreigaben, MTP → MTP (Android/Kindle/Kameras), Git → Git, Viewer →
  Vorschau, Developer → Entwickler, MCP server → MCP-Server, Logging → Protokollierung, Updates & privacy → Updates &
  Datenschutz, Advanced → Erweitert, Keyboard shortcuts → Tastaturkurzbefehle, License → Lizenz.
- Section and card TITLES keep `Dateioperationen`, the one place the loanword survives; everything a user started is a
  `Vorgang` (`terms.json` `operation`).

## Der Doppelklick-Hinweis und der Bereichshintergrund (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- **pane background → `Bereichshintergrund`** · KDE Dolphin („Doppelklick auf den Hintergrund der Ansicht“), Double
  Commander („leeren Teil der Dateiansicht“) · high.
- **empty space around the file list → `leere Fläche rund um die Dateiliste`** · Double Commander sagt „Teil“, `Fläche`
  liest sich für den leeren Hintergrund natürlicher · high.
- **„go up a folder“ → `in den übergeordneten Ordner wechseln`**, Double Commanders Wortlaut für genau diese Einstellung
  („Wechsel in das übergeordnete Verzeichnis durch Doppelklick auf den leeren Teil der Dateiansicht aktivieren“), nur mit
  macOS' `Ordner` statt `Verzeichnis`. Der Hinweistext und der Brotkrumen-Tooltip sagen dagegen `navigieren` (macOS
  Finder: „Navigiert zu einem Ort …“), weil das Englische dort „navigates“ sagt.
- **„What just happened?“ → `Was ist gerade passiert?`**, **„I like it“ / „Don't like it?“ → `Gefällt mir` /
  `Gefällt dir das nicht?`** · `Gefällt mir` ist Apples und der sozialen Netzwerke Standard für „like“ · high.
- **„Never do this again“ → `Das nie wieder tun`** · die Taste schaltet das Verhalten ab, nicht nur den Hinweis, also
  passt macOS' „Nicht mehr anzeigen“ hier nicht · high.

## Zwischenablage als Datei einsetzen (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

- **„Paste clipboard content as a file“ → `Inhalt der Zwischenablage als Datei einsetzen`**; der Toast lautet
  „{Bild/PDF/Text} aus der Zwischenablage als {filename} eingesetzt“ · `paste → einsetzen` (macOS), `Zwischenablage`
  (macOS) · high.
- **„Do nothing“ → `Nichts tun`** · Double Commander kürzt auf „Nichts“, als volle Option liest sich `Nichts tun`
  idiomatisch · high.
- **PDF als Dokumentart ist Neutrum** (`das PDF`, `ein PDF`), weil macOS es als „PDF-Dokument“ führt · high. Im Toast
  bleibt der `select`-Zweig artikellos („PDF aus der Zwischenablage …“), damit sich kein Genus festlegen muss.

## Kopieren und Bewegen: Zielordner, Fortschrittszeilen und zu große Dateien (`fileOperations.transferDialog.targetWillBeCreatedCopy`/`.targetWillBeCreatedMove`, `queue.row.label`, `fileOperations.shared.scanningTooltip`, `fileOperations.errorDialog.tooLargeAndMore`, `errors.write.filesTooLargeForFilesystem.*`)

- **„This folder doesn't exist yet. Cmdr will create it during the copy/move.“ → `Diesen Ordner gibt es noch nicht.
  Cmdr erstellt ihn beim Kopieren.` / `… beim Bewegen.`** · `Ordner` ist maskulin (also `diesen Ordner`, `ihn`), das
  Existenz-Idiom ist das gesetzte `gibt es`, das aktive `Cmdr erstellt ihn` schlägt macOS' passives „wird erstellt“ nach
  der Aktiv-Regel, und „during the X“ wird zum Verb (`beim Kopieren`) · high.
- **Die Fortschritts-Arme von `queue.row.label` bleiben im Passiv Präsens**: `Wird kopiert`, `Wird bewegt`,
  `Wird umbenannt`, `Ordner wird erstellt`, `Datei wird erstellt`, `Archiv wird bearbeitet`. Das gilt auch dort, wo der
  beruhigende Satz im Dialog aktiv ist („Cmdr erstellt ihn“) · Nautilus („wird … umbenannt“) · high.
- **„Scanning…“ (Spinner, während der Dialog die Auswahl zählt) → `Wird durchsucht …`**, wie
  `transferProgress.stageScanning`; Fortschrittszeile, also Leerzeichen vor `…` · high.
- **„and N more files“ → `und {countText} weitere {count, plural, one {Datei} other {Dateien}}`** · GNOME Nautilus („%'d
  weitere Objekte ausgewählt“); das feminine `weitere` bleibt in beiden Zweigen gleich · high.
- **„too large“ → `zu groß`** („Datei zu groß für dieses Laufwerk“), **„formatted as“ → `mit … formatiert`** („mit FAT32
  formatiert“); die Formatnamen `FAT32` und `exFAT` bleiben stehen (macOS Finder, MS-Terminologie, `@key`) · high.
- **Die Begrenzung eines Dateisystems heißt `Begrenzung`** („keine solche Begrenzung“, KDE Dolphin „Keine Begrenzung“),
  sonst ist das Substantiv `Limit` · high.

## Kurzbefehle erfassen und reservierte Systemkürzel (`downloads.shortcutRow.*`, `shortcuts.system.*`, `shortcuts.section.pressKeys`, `shortcuts.conflict.*`)

- **„Press keys…“ → `Tasten drücken …`** · high.
- **registered / not registered → `registriert` / `nicht registriert`** (MS-Terminologie) · high.
- **combo → `Kombination`** (kurz für Tastenkombination), in den Konfliktwarnungen · high.
- **Die reservierten macOS-Kürzel**: „input source switching“ → `Wechsel der Eingabequelle`, „app switcher“ →
  `App-Umschalter`, „App windows“ → `App-Fenster`, „logging out“ → `das Abmelden`, „locking the screen“ →
  `das Sperren des Bildschirms` (nominalisiert für den `(…)`-Einschub der Konfliktwarnung), „screen recording“ →
  `Bildschirmaufnahme`, „screenshots“ → `Bildschirmfotos`; `Mission Control`, `Spaces` und `Spotlight` bleiben englisch
  wie im deutschen macOS (geprüft im macOS-Stapel, 2026-06-21) · high.

## Zeitangaben und Beispiel-Platzhalter (`queryUi.age.*`, `fileOperations.mkdir.placeholder`, `fileOperations.mkfile.placeholder`, `feedback.dialog.placeholder`, `settings.analytics.email.description`)

- **„{count}m/h/d/w/mo/y ago“ → `vor {count} Min./Std./T./Wo./Mon./J.`** · im Deutschen steht `vor` vorn, abgekürzt, damit
  der Tooltip knapp bleibt · high.
- **„Example:“ → `Beispiel:`** · high.
- **„you@example.com“ → `du@example.com`**, in allen Feldern gleich: der lokale Teil wird übersetzt, die Domain bleibt,
  weil `example.com` per RFC 2606 für Beispiele reserviert ist und `beispiel.de` eine echte Domain ist.

## Termbase-Abgleich: was vereinheitlicht wurde (`askCmdr.decision.*`, `askCmdr.wakeDigest.*`, `operationLog.rollback.partiallyRolledBack`, `whatsNew.dialog.title`, `fileOperations.rollbackConfirm.bodyStopAndMoveBack`/`.bodyUndoByMovingBack`, `queue.row.reversalMovingBack`)

Die Drift-Prüfung gegen `terms.json` hat Stellen gefunden, an denen der Katalog ein gesetztes Wort verfehlt hatte. Alle
sind behoben; die ersetzte Form steht jeweils als `avoid` in `terms.json`.

- **move → `bewegen`, nie `verschieben`**: elf Werte sagten noch `verschieben`/`zurückverschieben`, darunter der
  Programme-Dialog (`updates.moveToApplicationsDialog.title`, `settings.revealHandler.notInApplications`). Die
  Rücknahme einer Bewegung heißt `zurücklegen` (Finders „Put Back“): `Das legt alles zurück, was der Vorgang bisher
  bewegt hat`, `Dateien werden zurückgelegt`.
- **`Rollback` ist Neutrum**: der Katalog sagte `das Rollback` und `es`, aber `Teilweiser Rollback`; jetzt
  `Teilweises Rollback`.
- **item → `Objekt`, nie Microsofts `Element`**: die Ask-Cmdr-Zusammenfassungen (`askCmdr.wakeDigest.*`) und
  `askCmdr.decision.approved`/`.rejected`. Dort steht der geteilte Baustein `{verbName}` („das Kopieren von“) jetzt nach
  einem Doppelpunkt (`Du hast zugestimmt: …`), weil `zustimmen` den Dativ verlangt und `ablehnen` den Akkusativ: ein
  gemeinsamer Baustein passt sonst nur in einen der beiden Sätze.
- **paste → `einsetzen`, nie `einfügen`** (Lizenzschlüssel, API-Schlüssel, Pfadfeld, Toasts).
- **What's new → `Neuigkeiten`**: der Dialog hieß `Neuheiten in Cmdr`, der Menüpunkt, der ihn öffnet, `Neuigkeiten`.
- **Kein deklinierter Markenname**: drei `Cmdrs …` wurden zu `… von Cmdr` bzw. umgebaut.
- Kleinere Einzelfälle: `Tastenkürzel` → `Kurzbefehl`, `Scan` → `Durchlauf`, `abrufen` → `laden`, `Schaltfläche`/`Knopf`
  → `Taste`, `Aktionsprotokoll` → `Vorgangsprotokoll`, `Sorry` → `Tut mir leid`, `fehlgeschlagen`/`Fehlschlag` aus den
  Zustandszeilen, `Preview` → `Vorschau` (Apples App-Name), `Voller Festplattenzugriff` → `Festplattenvollzugriff`,
  `du@beispiel.de` → `du@example.com`, `Versuch es` → das gesetzte `Versuche es`.

## Archive browsing (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `fileOperations.delete.archiveWarningStrong`)

- archive (a zip/tar/7z Cmdr browses like a folder) → Archiv (plural Archive) · macOS Finder ("Zip-Archiv", "Komprimiert
  Objekte in ein Archiv", "%[Kind]@ is %[archives]@" → "ist Archiv") · high
- zip archive → Zip-Archiv (plural Zip-Archive); the file itself → Zip-Datei · macOS Finder ("Zip-Archiv") · high. Used
  "aus der Zip-Datei entfernt" for the delete-warning's second half (feminine Datei reads more naturally than bare "das
  Zip")
- app bundle (the .app/.bundle/.framework opaque-folder group) → App-Paket (plural App-Pakete; dative plural
  App-Paketen) · macOS Finder's own term for these is "Paket" ("Paketinhalt zeigen" = Show Package Contents), and modern
  macOS uses "App"/"Apps" (App Store, "Apps"); so "App-Paket" is the macOS-native compound · high. MS terminology has
  bundle→Bundle (Windows/dev term), noted as the alternative but rejected for a macOS file manager. Keep the SAME word
  in card.bundles, bundle.label, and the enterBehavior/summary prose
- browse (step INTO an archive/bundle and list it like a folder) → durchsehen · KDE Dolphin ("Browse through archives" →
  "Archive durchsehen") · high. Deliberately NOT "durchsuchen" — that's the settled scan/search verb (termbase scan →
  durchsuchen), and "durchsehen" (look through) is the file-manager-native, unambiguous term for browsing into an
  archive. Full form "Browse like a folder" → "Wie einen Ordner durchsehen"; segmented-control cell "Browse" →
  "Durchsehen"
- extract (an archive) → entpacken · Double Commander ("Entpacken", Cmdr's two-pane lineage) · high. MS terminology has
  extract→extrahieren; "entpacken" reads more naturally for archives and matches the orthodox family. Used in
  readOnly.archiveMessage ("durchsieht und entpackt tar- und 7z-Archive")
- edit (change a zip's contents: add/remove/rename entries) → bearbeiten · standard DE + MS terminology · high. The
  queue.row.label `archive_edit` arm ("Editing archive") → "Archiv wird bearbeitet", keeping the sibling arms' passive
  present
- Enter key (the Return/Enter key, "pressing Enter does X") → die Eingabetaste · settled in the existing de catalog
  (search.json, viewer.json: "warten immer auf die Eingabetaste", "die Eingabetaste (Öffnen …)") · high. Frame "What
  pressing Enter does on X" → "Was die Eingabetaste bei X bewirkt"
- Ask (segmented option: ask each time whether to browse or open) → Fragen (control cell); prose "ask each time" →
  "jedes Mal fragen" · MS terminology (Ask → Fragen); macOS uses "nachfragen" for confirm-prompts, but the short
  "Fragen" fits the segmented cell and reads clean · high
- Configure… (menu item opening Settings) → Konfigurieren… · MS terminology (configure → konfigurieren); ellipsis
  attached with no space (macOS menu-item-opens-dialog convention, per style guide) · high
- read-only archive → Schreibgeschütztes Archiv · termbase read-only → schreibgeschützt + archive → Archiv · high
- "There's no trash inside an archive." (bold delete-warning lead) → "In einem Archiv gibt es keinen Papierkorb." ·
  trash → Papierkorb + the catalog's settled "gibt es" existence idiom · high

## Archive-password dialog (`fileOperations.archivePassword.*`, `commands.fileCompress.*`, `settings.archives.compressionLevel.*`)

Terms settled while translating the encrypted-archive unlock modal (`fileOperations.archivePassword.*`; macOS AppKit +
Total/Double Commander de).

- password-protected → `passwortgeschützt` · TC/DC de phrasing + macOS · high. Body: "… ist passwortgeschützt."
- password (noun) → `Passwort` · macOS/MS · high. Input aria-label compounds to `Archivpasswort`.
- unlock (button + verb) → `Entsperren` · macOS AppKit locked-item button ("Entsperren") · high. Reused for the verb
  ("um es zu entsperren").
- archive (the `{name}` head / input label) → `Archiv` · settled de termbase · high.

Settled while translating the Compress feature:

- compress (verb / control label) → `Komprimieren` · Finder `de/macOS` ("Komprimieren", `Compress ${sources}` →
  „${sources}“ komprimieren) · high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and
  the `compress` branch of `titleVerbOnly`. `titleWithCounts` uses the lowercase infinitive `komprimieren` to match the
  sibling `kopieren`/`bewegen` pattern.
- compressing (progress -ing form) → `Wird komprimiert` · derived on the sibling `Wird kopiert`/`Wird bewegt` pattern ·
  high. Used in `titleActive`, `stageActive`, and the noun form `Komprimieren` in `scanTitleCompress` ("Prüfung vor dem
  Komprimieren …").
- compressed (result toast) → `komprimiert` (past participle) · mirrors `transfer.split.clean` (`{phrase} kopiert`) ·
  high.
- replace (overwrite warning) → `ersetzt` · Finder `Replace` → "Ersetzen" · high. `targetWillBeOverwritten` = "Eine
  Datei mit diesem Namen ist bereits hier. Cmdr ersetzt sie."
- archive (name) → `Archiv`/`Archivname` · Finder `Zip archive` → "Zip-Archiv" · high. `.zip` kept in straight double
  quotes per the en key's do-not-restyle note.
- compression level (slider label) → `Komprimierungsstufe` · TC `de` "Kompressionsrate (0-9)"; archiver UIs use
  `-stufe`/`-grad` for the 1–9 level · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Schneller` · TC `de` "schnellste Komprimierung (1)" (root `schnell`) · high. Marks
  quicker packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `Kleiner` · comparative of `klein`, pairs with `Schneller`; marks the smaller
  output file (TC `de` high end "maximale Komprimierung") · high. `.smaller`.

## Operation log (`operationLog.*`, `commands.logOperationLog.*`, `settings.operationLog.*`)

Terms settled while translating the Operation log dialog (`operationLog.*`; the retention settings under
`settings.operationLog.*` had already fixed the core feature vocabulary with the retention settings, and macOS Finder
confirms `Vorgang`).

- operation → `Vorgang` (plural `Vorgänge`) · macOS Finder ("Der Vorgang kann nicht abgeschlossen werden.",
  "Kopiervorgang"/"Löschvorgang") + the settled Cmdr `de` catalog (`errors.listing.*` use `Vorgang` throughout,
  `settings.operationLog.maxSize.description` "die ältesten Vorgänge") · high. NOT the loanword "Operation", with
  exactly two carve-outs, and they are the whole list: (a) the PROTOCOL-level sense, where an "Operation" is one request
  on the wire (`settings.network.smbConcurrency.label`, `settings.network.customTimeout.description`), and (b) the
  Settings section and card TITLES that are named after that heritage (`settings.section.navigationAndFileOps`,
  `settings.navigationAndFileOps.card.fileOperations`, `settings.advanced.card.fileOperations` = "Dateioperationen").
  Anything a USER started is a `Vorgang`, including in a tooltip or a settings DESCRIPTION: those had leaked to
  "Operationen" and are back.
- operation log → `Vorgangsprotokoll` · already settled with the retention settings
  (`settings.navigationAndFileOps.card.operationLog` → "Vorgangsprotokoll", log → Protokoll) · high. The dialog title
  (`operationLog.dialog.title`) and the command label (`commands.logOperationLog.label`) MUST match this
  Settings-section name.
- history (of operations) → `Verlauf` · `settings.operationLog.intro` "damit du deinen Verlauf ansehen … kannst",
  `maxAge.label` "Verlauf aufbewahren für"; macOS uses "Verlauf" for history · high. Used in the load-error string.
- file operations (the user's ops collectively) → `Dateivorgänge` · `settings.operationLog.intro` "Cmdr protokolliert
  deine Dateivorgänge" · high. Used in the command description.
- roll back / undo (verb, user-facing prose) → `rückgängig machen` · `settings.operationLog.intro` "Aktionen rückgängig
  machen"; macOS "widerrufen"/"rückgängig" · high. Used for the friendly command description ("… und mach sie
  rückgängig"). Distinct from the technical status noun below.
- rollback (technical status chips) → `Rollback` (noun, kept) · termbase rollback→Rollback + MS terminology · high. Chip
  renderings: `Rollback möglich` / `Kein Rollback möglich` (calm "X nicht möglich" pattern, avoids "kann nicht") /
  `Rollback läuft` (termbase's illustrative "Rollback läuft …" ellipsis dropped here to match the sibling no-ellipsis
  status chips `Läuft`/`Wartet`) / `Rollback abgeschlossen` (complete→abgeschlossen, the "Löschen abgeschlossen"
  pattern; reused for both `rollback.rolledBack` and `outcome.rolledBack`) / `Teilweises Rollback` (partly→teilweise; `Rollback` is neuter).
  The short technical noun keeps the chips inside their width; the verb "rückgängig machen" stays for running prose.
- lifecycle status chips → reused verbatim from `queue.row.status` (`queue.json`): queued → `Wartet`, running → `Läuft`,
  done → `Fertig`, "Didn''t finish" (failed) → `Nicht abgeschlossen` (avoids "Fehler"/"fehlgeschlagen" per the voice
  rule, matching the en source's deliberate "Didn''t finish"), canceled → `Abgebrochen` · high.
- per-item outcome chips → done → `Fertig`, skipped → `Übersprungen` (termbase skip→überspringen), "Didn''t finish"
  (failed) → `Nicht abgeschlossen`, rolled back → `Rollback abgeschlossen` · high.
- summary lines (past-participle-final, item → `Objekt`/`Objekte`) → "{countText} Objekt(e) kopiert/bewegt/gelöscht/
  umbenannt/komprimiert", trash → "… in den Papierkorb bewegt" (verbatim `transfer.trash` frame), createFolder →
  "{countText} Ordner erstellt" (Ordner invariant in plural), createFile → "… Datei/Dateien erstellt", "Edited an
  archive" → `Archiv bearbeitet`, "Extracted an archive" → `Archiv entpackt` (extract→entpacken) · high. Mirrors the
  settled `transfer.*` participle pattern ("{phrase} kopiert", "… komprimiert").
- initiator / provenance labels → "You" → `Du` (standalone label, sentence-initial cap; du-address settled), "AI client"
  → `KI-Client` (AI→KI settled; client kept, MS "Client") · high · tentative on the loanword `KI-Client`. "Agent"
  (Cmdr''s own AI agent) → `Agent` (kept; the standard DE loanword for a software/AI agent, matching the en source''s
  bare "Agent") · tentative (open question in `review-queue.md`).
- more-items line → "und {countText} {count, plural, one {weiteres Objekt} other {weitere Objekte}}" · item→Objekt
  (neuter, so "weiteres"/"weitere" declines inside each branch, unlike the invariant feminine "weitere Datei(en)"
  termbase entry) · high.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

Terms settled while translating the Ask Cmdr chat rail (`askCmdr.*`, `settings.askCmdr.*`,
`settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`): the read-only AI chat feature, its rail UI, tool status
labels, error copy, sessions, attachments, the consent screen, and the cost footer.

- chat (the AI conversation, not a file-op concept) → `Chat` (singular)/`Chats` (plural) · Microsoft terminology maps
  chat/instant messaging → `Chat`; kept as the standard German loanword · high. `sameAsSourceJustification` recorded on
  `askCmdr.threads.open` and `askCmdr.sessions.title` (both bare "Chats").
- token (LLM usage-accounting unit) → `Token` (singular) / `Tokens` (plural) · confirmed via OpenAI's German help center
  ("Was sind Tokens und wie zählt man sie?") and general German AI/dev usage: unlike native nouns ending in `-en`
  (invariant plural), the LLM-token loanword takes the English plural `-s` in German technical writing · high
- tool (an LLM/agent tool call) → `Werkzeug` · generic, user-facing fallback status (`askCmdr.tool.unknown.done`); not
  the dev-facing "Tool" loanword, since this surfaces in the chat rail to end users · tentative — no direct source,
  picked for approachability over the AI-dev-tooling loanword
- attachment (a file/folder staged in the Ask Cmdr composer to ask about) → `Anhang` (plural `Anhänge`); attach (verb) →
  `anhängen` · macOS AppKit ("RTF mit Anhängen"); standard DE email-attachment vocabulary · high. Distinct from the
  Archive-browsing `Archiv`/`archivieren` sense
- drop (drag-and-drop deposit, e.g. "Drop to attach") → `ablegen` · macOS SystemSettings ("Im Dock ablegen" = drop in
  the Dock) · high
- archive a chat / unarchive (hide/restore a conversation from the active list; distinct from the zip-archive sense) →
  `archivieren` (verb) / `Archiviert` (badge); unarchive → `Archivierung aufheben` · Microsoft terminology
  (archive→archivieren, archived→Archiviert); "aufheben" mirrors the termbase's own `deselect all → Auswahl aufheben`
  pattern for reversing a state · high for archive/archiviert, tentative for "Archivierung aufheben" (no direct
  unarchive source)
- tool-call status lines (doing/done pairs, e.g. `askCmdr.tool.*`) → doing uses passive present
  `{Objekt} wird/werden {Partizip}`; done drops `wird`/`werden` and keeps the bare `{Objekt} {Partizip}` · derived from
  the app's existing progress-line convention (`Wird durchsucht`, `Wird kopiert`) and the participle-final summary-line
  pattern (`{items} kopiert`) · high. Article/possessive presence mirrors the English source exactly (`a folder` →
  `Ein Ordner`, `your drives` → `Deine Laufwerke`) so doing/done stay parallel.
- reply (an Ask Cmdr assistant reply, distinct from email "reply") → `Antwort`; "didn't finish" (a reply that stopped
  mid-stream) → reuses the settled lifecycle-chip term `nicht abgeschlossen` (`operationLog.*`) · high
- provider-side generic failure ("Something went wrong on the provider's side") →
  `Beim {Anbieter} ist etwas schiefgelaufen` · reuses the settled calm-rephrase pattern from the update-check error
  toast · high
- rate-limited/out-of-quota (AI provider) → `ist ausgelastet` (busy) / `Kontingent ist ausgeschöpft` (quota exhausted) ·
  aligned with the existing `ai.translateError.rateLimited.*` strings (same concept, already shipped) · high
- API key rejected ("didn't accept your key") → `hat deinen API-Schlüssel abgelehnt` · aligned with the existing
  `ai.translateError.authFailed.title` ("wurde abgelehnt") for one term across the app · high
- budget/step limit exhausted (a single answer's tool-step or time budget) → `Limit erreicht` · aligned with the
  existing `Tab-Limit erreicht` pattern (`commands.handler.tabLimitReached` / `fileExplorer.tabs.limitReached`) · high
- on-device / free (local-model cost readout, `askCmdr.cost.free`) → `lokal` · aligned with the existing `Lokales LLM`
  provider-option translation (`settings.ai.provider.opt.local`) · high
- consent/opt-in screen tone → warm second-person imperative headline ("Sprich mit Cmdr über deine Dateien"), calm
  declarative body; "it can't: there's no tool that reads them" → `Das kann es auch gar nicht: …` (emphatic negation,
  not a literal capability statement) · product voice, no direct source · tentative
- Ask Cmdr model / Ask Cmdr-Modell (settings field, brand+native-noun compound) → hyphenated `Ask Cmdr-Modell` ·
  standard DE compounding rule for a multi-word loanword + native noun (parallel to `iPhone-App`) · high
- "Same as Cmdr's AI" (empty-field placeholder) → `Wie die KI von Cmdr` · analytic genitive per the termbase's "don't
  decline Cmdr to Cmdrs" rule, not `Cmdrs KI` · high

## Image-content indexing on network drives (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`, `search.imageResults.paused`)

Terms settled while translating the network-drive image-indexing opt-in (`settings.mediaIndex.networkVolumes.*`, the
`settings.mediaIndex.*Index*` internal lists, and `search.imageResults.networkOff`/`paused`).

- photo → `Foto` (plural `Fotos`) · macOS AirDrop ("1 Foto empfangen", "^0 Fotos empfangen"), macOS Photos app ("In
  „Fotos“ öffnen") · high. The EN source deliberately says "photos" (warmer) for the network-drive/NAS-archive strings
  while the local card says "images"; keep the split in DE too — `photo → Foto`, `image → Bild` (the local card stays
  `Bildinhalte`/`Bildersuche`). ❌ Don't collapse both to `Bild`
- network drive → `Netzlaufwerk` · Microsoft terminology (network drive → Netzlaufwerk) · high. An SMB-mounted drive
  Cmdr can index; distinct from the settled `network share → Netzwerkfreigabe` (the exported share itself)
- image indexing → `Bildindizierung` · already shipped in the de catalog (`settings.section.imageIndexing` =
  `Bildindizierung`), reused for the card/settings label and the search hint · high. ❌ Don't swap in `Bildersuche`:
  that's the settled word for the search the index feeds (`fileExplorer.imageIndex.drive.off` = „Die Bildersuche ist für
  dieses Laufwerk deaktiviert.“), not for the indexing itself
- photo archive → `Fotoarchiv` · compound of `Foto` + `Archiv` (archive → Archiv, termbase) · high. The rarely-browsed
  NAS photo store the "always index" switch targets
- indexing paused (auto-pause when a network drive disconnects) → `Angehalten` (status) / `hält an` (prose, verb
  `anhalten`) · macOS ("Kopieren von „^0“ wurde angehalten"), aligns with the settled transfer `pause → anhalten`
  /`Angehalten` · high. resume → `fortsetzen` ("wird fortgesetzt"), reconnect → "wieder verbunden" (derived from the
  settled connect/disconnect terms). ❌ Don't introduce the loanword "pausiert" — macOS uses `anhalten`/`angehalten`

## Indexing run-kind headers + hour-scale ETA (`indexing.run.*`, `indexing.eta.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

Terms settled while filling the drive-indexing checklist headers (`indexing.run.*`), the spelled-out hour ETAs
(`indexing.eta.hours*Left`), and the "image indexing is queued behind the drive scan" feedback lines
(`indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`).

- run-kind headers → `Erster vollständiger Durchlauf` (first full scan) / `Erneuter vollständiger Durchlauf` (full
  rescan) / `Schnelle Aktualisierung` (quick update) · builds on the settled `scan → durchsuchen`/`Durchlauf` and
  `rescan → erneuter Durchlauf`; `full → vollständig` (termbase full-disk-access → "vollständiger Festplattenzugriff");
  `update → Aktualisierung` (termbase update → aktualisieren) · high. First/rescan share `vollständiger Durchlauf` and
  differ only in `Erster`/`Erneuter`, a clean parallel; the quick path is the light `Aktualisierung`, not a `Durchlauf`.
- spelled-out hour ETA → `noch {n} Stunde(n)` / `noch {n} Stunde(n) {m} Minute(n)` · extends the settled ETA pattern
  (`"{n}m left" → "noch {n} Min."`, DE puts "noch" first) to the full-word hour scale; `hour → Stunde/Stunden`,
  `minute → Minute/Minuten` (CLDR one/other) · high. The compact `s`/`Min.` abbreviations stay on the sub-hour keys;
  only the hour scale spells the unit out, matching the EN source.
- `Laufwerksdurchlauf` (drive scan, as a noun/event: "after the drive scan") · already in the de catalog
  (`indexing.rescan.fallback`: "Ein neuer Laufwerksdurchlauf …") · high. The running-subject phrasing ("The drive scan
  is still running") uses the verb form `Das Laufwerk wird noch durchsucht` instead, matching `indexing.scan.label`.

## Bulk rename review + image-index scope (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`, `fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`)

Terms settled while re-checking the natural-language bulk-rename review (`askCmdr.renameReview.*`,
`askCmdr.tool.proposeRenamePlan.*`), the per-pane image-index status labels (`fileExplorer.imageIndex.*`), and the
image-index scope settings (`settings.mediaIndex.scope.*`, `.chosenFolders.*`).

- **index (verb) → `indizieren`, NEVER `indexieren`** · macOS DE (`Indiziert`), Microsoft terminology
  (`indizieren`/`Indizierung`; `indexieren` does not exist in the TBX at all), plus the whole shipped de catalog
  (`Bildindizierung`, `Immer zu indizierende Ordner`, `{countText} Fotos indiziert`) · high. ❌ `indexieren` is a
  false-friend anglicism; the `fileExplorer.imageIndex.*` family was the only place it had leaked in.
- percent sign → **space before `%`** (`{percent} %`) · DIN 5008 and the rest of the de catalog (`Auf 100 % zoomen`,
  `{freeText} frei ({percentText} %)`) · high. ❌ Never `{percent}%`.
- allow / deny (per-row rename gate) → `Erlauben` / `Ablehnen`; "Allow all" / "Deny all" → `Alle erlauben` /
  `Alle ablehnen` · macOS Finder ("Allow Anyway" → "Trotzdem erlauben", "allow opening" → "erlauben") · high. ❌ Not MS
  terminology's harsher `verweigern`, and not macOS's permission-prompt `Nicht erlauben` (that's a system-dialog pair,
  not a per-row toggle).
- rename cycle → `Umbenennungszyklus`; the badge → `(Zyklus)` · Microsoft terminology (cycle → Zyklus, masc.) · high.
  Tooltip stays active-voice ("Cmdr verwendet einen temporären Namen, während es diese Dateien zyklisch umbenennt"), per
  the active-voice rule.
- extension badge (compact) → `(Endung)`; the tooltip and prose keep the full `Dateiendung` · termbase file extension →
  Endung; the catalog's shipped prose ("Das Ändern der Dateiendung ist nicht erlaubt", "Endungsänderungen immer
  erlauben") · high. The badge is a tight chip beside a filename, so it takes the short form.
- overwrite badge → `(Überschreiben!)` (capitalized nominalized verb) · termbase overwrite → überschreiben · high.
  Lowercase `(überschreiben!)` reads as an imperative button ("overwrite it!"), the opposite of the warning intent; the
  nominalized form labels the risk instead.
- "needs attention" (a blocked row) → `Bei dieser Umbenennung ist noch etwas zu klären` · no direct source; the literal
  `braucht Aufmerksamkeit` is an anglicism, and the voice rule bans a bare "Fehler" label · high. Note "continue" here
  means _proceed_, not _resume_, so it renders `ausgeführt werden kann`, NOT the settled resume verb `fortsetzen`.
- "Ask Cmdr to prepare it again" → `Lass Ask Cmdr sie erneut vorbereiten` · product voice · high. ❌ Don't open with
  `Bitte Ask Cmdr, …`: sentence-initial "Bitte" reads as "please", so the imperative-of-_bitten_ meaning is lost.
- importance (of a folder, as a ranking criterion) → `Wichtigkeit der Ordner` · the shipped catalog's own wording
  (`askCmdr.tool.folderImportance.*` "Die Wichtigkeit eines Ordners", `importanceThreshold.waitingForImportance` "welche
  Ordner wichtig sind") · high. ❌ Not the hyphenated ad-hoc compound `Ordner-Wichtigkeit`.
- "the device holding `{path}`" → `das Gerät mit {path}` · already shipped in the sibling
  `errors.listing.deviceDisconnected.explanation`; kept identical so the two device panels read as one family · high.
- Tool doing/done pairs: `proposeRenamePlan` follows the settled passive-present pattern
  (`Ein Umbenennungsplan wird vorbereitet` / `Ein Umbenennungsplan vorbereitet`), with the subject in the NOMINATIVE so
  the done arm is the doing arm minus `wird`, exactly like `Ein Ordner wird aufgelistet` / `Ein Ordner aufgelistet` ·
  high. ❌ A bare infinitive (`Einen Umbenennungsplan vorbereiten`) reads as a command, not a status.

## Image-index status badges on files/folders/drives (`fileExplorer.imageIndex.file.*`, `fileExplorer.imageIndex.folder.*`, `fileExplorer.imageIndex.drive.*`, `settings.mediaIndex.showFileStatusIcons.*`)

Terms settled while translating the 13 per-file/folder/drive image-index indicator strings
(`fileExplorer.imageIndex.file.*`, `.folder.*`, `.drive.*`, and the `settings.mediaIndex.showFileStatusIcons.*` toggle).

- status badge (the small overlay icon on an image/folder/drive row marking its image-index state) → `Statussymbol`
  (plural `Statussymbole`) · tentative — no perfect direct source. Rejected: MS `Kennzeichen` maps to "flag" (MS TBX id
  54732 = flag), and MS `Abzeichen`/`Badge` are the achievement/reward-badge sense ("a small image indicating roles,
  achievements…"), wrong register for a status overlay. `Statussymbol` reads as the native, transparent term for a small
  status icon (Symbol = icon in DE UI). The runner-up `Statuskennzeichen` is in `review-queue.md`.
- indexed (image, adjective/participle) → `indiziert` · settled `index → indizieren`; matches the 33 shipped `indiziert`
  uses and macOS `Indiziert` · high. "Indexed for image search" → `Für die Bildersuche indiziert`; image search →
  `Bildersuche` (shipped term, `search.imageResults.*`).
- "Waiting to be indexed" (pending status) → `Wartet auf die Indizierung` · `indexing → Indizierung` + natural status
  phrasing · high.
- "Changed since indexing; will be re-indexed" (stale status) → `Seit der Indizierung geändert; wird neu indiziert` ·
  stale sense rendered as a full sentence (termbase stale → veraltet, but the source is a clause, not the bare word);
  re-index → `neu indizieren`, passive present `wird neu indiziert` · high. Semicolon preserved from source.
- "Couldn''t be indexed" (failed status, kept gentle) → `Ließ sich nicht indizieren` · termbase "Couldn't X" → "X ließ
  sich nicht …" calm-rephrase pattern; avoids the banned "Fehler"/"fehlgeschlagen" · high.
- "Not included in image search" (excluded status) → `Nicht in der Bildersuche enthalten` · included → enthalten +
  Bildersuche · high.
- folder/drive counts → `von` governs DATIVE, so the counted-noun plural branch is `Bildern` (dative pl), not `Bilder`:
  `{doneText} von {totalText} {total, plural, one {Bild} other {Bildern}} indiziert` (someIndexed, drive.indexing). The
  `alle …`/`sind indiziert` frames are NOMINATIVE, so those keep `Bilder`:
  `Alle {totalText} … {one {Bild} other {Bilder}} …` (folder.allIndexed, drive.done). one-branch dative singular is bare
  `Bild` · high (style guide: "in 3 Ordnern", dative plural -n).
- "on this drive" → `auf diesem Laufwerk`; "still working" → `läuft noch`; "Image search is off" →
  `Die Bildersuche ist … deaktiviert` (turn off → deaktivieren) · high.

## Image-index settings restructure: cards + semantic-search model delete (`settings.mediaIndex.clip.*`, `settings.mediaIndex.semanticSearch.*`, `settings.mediaIndex.progressSummary.title`)

Terms settled while translating the 12-key settings restructure (three card titles, the semantic-search toggle + model
delete flow, and the "Indexing now" file badge).

- "search by description" (the friendly name for semantic search, kept distinct from the card title "Semantic search" →
  `Semantische Suche`) → `Suche per Beschreibung` · already shipped in `clip.ready` ("suche deine Fotos per
  Beschreibung"), reused verbatim across `semanticSearch.label` ("Fotos per Beschreibung suchen"), `clip.notSupported`,
  `clip.offButInstalled`, and `clip.deleteConfirmBody` for one consistent term · high. ❌ Don't coin a
  `Beschreibungssuche` compound; `per Beschreibung` is the shipped choice.
- "Enable indexing" (card title) → `Indizierung aktivieren`; "Folders to index" (card title) → `Zu indizierende Ordner`
  (parallels the shipped internal labels `Immer zu indizierende Ordner`/`… Laufwerke`) · enable → aktivieren, index →
  indizieren · high.
- "Indexing now" — rendered by context, not one string: the settings heading above live per-drive progress
  (`progressSummary.title`) → `Indizierung läuft` (status heading, running → `läuft`); the per-file badge tooltip
  (`fileExplorer.imageIndex.file.indexing`, THIS image being processed now) → `Wird gerade indiziert` (passive present +
  `gerade`, distinct from pending `Wartet auf die Indizierung`) · high.
- reclaim / free (disk space, model-delete flow) → `freigeben` · macOS/pile ("Speicherplatz freigeben",
  "Festplattenplatz freigeben") · high. `deleteButton` "reclaim {size}" → `{size} freigeben`; `deleteConfirmBody` "This
  frees {size}" → `Das gibt {size} frei` (separable verb).
- "the model is downloaded" (state) → `Das Modell ist heruntergeladen` · download → herunterladen (shipped
  `clip.download` "Modell herunterladen") · high. "download the model again" → `das Modell … wieder herunterladen`.
- keyword search → `Suche nach Stichwörtern`; tag search → `Suche nach Tags` · keyword → Stichwort (KDE/GNOME pile
  "Stichwort"), tag → Tag (shipped "Finder-Tag") · high. `deleteConfirmBody`: "Die Suche nach Stichwörtern und Tags
  funktioniert weiter".
- "couldn''t be removed just now" (model-delete failure, kept gentle) → `ließ sich gerade nicht entfernen` · termbase
  "Couldn''t X" → "X ließ sich nicht …" + the shipped `gerade nicht` calm idiom ("kann {name} gerade nicht indizieren",
  "konnte dich gerade nicht anmelden"); remove → entfernen · high. "Try again in a moment" →
  `Versuche es gleich noch einmal` (retry → erneut/noch einmal versuchen).
- `clip.deleting` "Deleting…" → `Wird gelöscht…` · matched the sibling transient-button label `clip.downloading` ("Wird
  heruntergeladen…", no space before the ellipsis) for a consistent download/delete button pair, rather than the
  status-line space-before-ellipsis form · high.
- "Apple silicon" kept verbatim (lowercase `silicon`, per the en @key "keep it"); no de-macOS rendering exists in the
  pile.

## Delete-dialog trash switch + transfer From/To groups (`fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `fileOperations.transferDialog.targetGroupTitle`)

New `fileOperations.json` keys from the dialog-polish pass: the delete dialog swapped its Trash/Delete picker for a
"Move to trash" switch plus a matching confirm button, and the copy/move/compress dialog groups the source path and the
destination volume+path under "From" and "To" headings.

- "Move to trash" (`delete.trashSwitch`; switch in the delete dialog, on = trash, off = permanent delete) →
  `In den Papierkorb bewegen` · the catalog's settled `move → bewegen` (macOS Finder "Bewegen", not the Microsoft
  "Verschieben"), and identical to every sibling trash string in this file (`transferDialog.titleVerbOnly`'s
  `other {In den Papierkorb bewegen}`, `transfer.trash`). macOS Finder's own menu item is `In den Papierkorb legen`
  (Finder AL13/N153); not taken, so the catalog keeps ONE move verb · high
- "Delete" (`delete.confirmDelete`; destructive confirm button while the switch is off) → `Löschen` · settled delete
  verb, identical to `transferDialog.titleVerbOnly`'s `delete {Löschen}` arm · high
- "From" / "To" (`transferDialog.sourceGroupTitle` / `targetGroupTitle`; headings over the source path and over the
  destination volume + path) → `Von` / `Nach` · Total Commander de (`662="VON:  "`, `663="NACH: "`) and Double Commander
  de ("Von:"/"Nach:") both ship this label pair in the same copy/move dialog, and "von X nach Y" is the German transfer
  idiom. The settled nouns `Quelle` / `Ziel` stay for the destination CONTROLS (`Zielvolume`, `Zielpfad`); the headings
  take the light prepositional pair the English uses · high

## Master-switch-off strings for drive indexing (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `settings.indexing.overriddenBadge`)

Terms settled while reviewing the five strings the master-toggle feature added
(`fileExplorer.navigation.driveIndex.refusedIndexingOff` / `.tooltipIndexingOff` / `.menuIndexingOffNote`,
`settings.indexing.masterOffNote`, `settings.indexing.overriddenBadge`).

- drive indexing (the master switch and the feature) → `Laufwerksindizierung` · the settled catalog term
  (`settings.indexing.enabled.label`, `settings.section.driveIndexing`, `settings.summary.driveIndexing`,
  `indexing.status.ariaLabel`, `onboarding.stepOptional.indexing.title`) · high. The navigation-path fragment quotes the
  live labels verbatim: `unter Indizierung > Laufwerksindizierung` (`settings.section.indexing` = `Indizierung`,
  `settings.indexing.enabled.label` = `Laufwerksindizierung`). Change one and all three `fileExplorer` strings must
  follow.
- "stays unindexed" → `wird … nicht indiziert` · ❌ NOT the coinage `unindiziert`: German doesn't form a `un-` +
  participle here, and the catalog's own frame is the passive `wird … indiziert` (`tooltipIndexingOff` "wird gerade kein
  Laufwerk indiziert") · high. Keeping `{name}` the nominative subject of a passive also means an arbitrary drive name
  needs no article or case ending.
- "(folder sizes) stay hidden" → `bleiben ausgeblendet` · the catalog's hide-from-view word is `ausgeblendet` (8 uses;
  `settings.ai.tooltipOff` "sind dann einfach ausgeblendet"), Microsoft terminology (hidden → ausgeblendet) · high. ❌
  Not `verborgen`: it's literary here, and the termbase reserves `verborgen` for the dotfile sense (hidden files →
  verborgene Dateien).
- "keeps its own on or off choice" → `behält seine eigene Einstellung` · high. The literal `Ein-/Aus-Wahl` is a coinage
  no source has; German carries the on/off sense inside `Einstellung` in this slot.
- "Off with drive indexing" (the small override badge) → `Mit der Laufwerksindizierung aus` · high. ❌ Never
  `Aus mit der Laufwerksindizierung`: `Aus mit X!` is a fixed German exclamation ("Aus mit der Gemütlichkeit!") meaning
  "X is over", so it reads as a slogan, not a state. Badge length is the German cost here: `Laufwerksindizierung` alone
  is 20 characters, so a faithful badge can't reach the English's 22.

## Drive index: the change-check run (`indexing.run.changeCheck`, `indexing.step.findFilesChangeCheck`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- **"Checking for changes" (run-kind header) → `Prüfung auf Änderungen`** · nominal phrase matching the sibling headers
  (`Erster vollständiger Durchlauf`, `Schnelle Aktualisierung`); `überprüfen` is macOS DE's checking verb (Finder BN9
  "Inhalt von „^0“ überprüfen"), `Änderungen` is the catalog-settled plural (`Neueste Änderungen nachholen`,
  `Dateisystemänderungen`) · high.
- **"Update the file list" → `Dateiliste aktualisieren`** · composed from the two settled siblings
  `Dateiliste speichern` + `Index aktualisieren`; the run writes only what changed, so the verb swaps and the object
  doesn't · high.
- **"check against the index" → `mit dem Index verglichen`** · deliberately plain `vergleichen` over `abgleichen`, which
  leans database-register; the English avoids that register on purpose · high.
- **"the check running right now" → `der gerade laufende Durchlauf`** · reuses `Durchlauf` as this catalog's settled
  word for a full check (`tooltipCoalesced`: "der nächste vollständige Durchlauf von Cmdr"), plus that string's own
  closing `bringt das wieder in Ordnung` · high.

## Stalled transfer: the honest-stall notice (`fileOperations.transferProgress.close`/`.stallNotice`/`.stallWaitingDestination`/`.stallWaitingSource`/`.stallUnknown`/`.stallInFlight`/`.stallLogHint`)

Terms and phrasings settled for the seven stall strings (`fileOperations.transferProgress.close` / `.stallNotice` /
`.stallWaitingDestination` / `.stallWaitingSource` / `.stallUnknown` / `.stallInFlight` / `.stallLogHint`). These
replace a confident countdown on a transfer that has stopped moving. `.stallNotice` renders on both surfaces, the
progress dialog and the narrow queue row, so it has to fit the row.

- **stalled / "no progress" (a transfer) → `Kein Fortschritt seit {duration}`** · `Fortschritt` is the settled progress
  noun across the tiers: macOS DE ("Kopierfortschritt anzeigen", "Fortschrittsfenster einblenden"), Double Commander
  ("Operationsfortschritt", "Gesamtfortschritt") · high for the noun, `tentative` for the frame. No source names the
  stall CONCEPT: Microsoft terminology has no `stall` / `stalled` entry and none of the file-manager catalogs has a
  stalled-transfer string, so the "Kein Fortschritt seit …" frame is a construction, not a lifted term. It leads with
  the negation like the English (it replaces the ETA line `noch ~{duration}`, so the state word has to come first).
- **"The transfer has stopped moving." → `Die Übertragung kommt nicht mehr voran.`** · `Übertragung` is the settled
  transfer noun; `kommt nicht mehr voran` is general German, not a term (unattested in the pile, which has no
  stalled-transfer string at all) · tentative on the phrasing. ❌ Not `Die Übertragung ist fehlgeschlagen` or anything
  with `Fehler`: nothing went wrong yet, and the voice rule forbids both words. `steht still` was the runner-up; it
  reads more final than the situation is (the transfer may still recover on its own).
- **"Waiting for the destination/source to respond." → `Cmdr wartet auf eine Antwort vom Ziel.` / `… von der Quelle.`**
  · destination → `Ziel`, source → `Quelle` (termbase, MS terminology), and the catalog's own transfer-domain pair uses
  exactly these bare nouns (`errors.write.readError.message` "Aus der Quelle ließ sich nicht lesen.",
  `errors.write.writeError.message` "Ins Ziel ließ sich nicht schreiben."); GNOME Nautilus confirms the bare noun ("The
  destination is not a folder." → "Das Ziel ist kein Ordner."); respond → `antworten` (MS terminology, AUT/DEU/CHE/LUX;
  macOS AppKit "did not respond to the request" → "hat auf die Dienstanfrage nicht geantwortet") · high. **Named subject
  on purpose**: macOS's own waiting lines are verbless progress fragments that take an ellipsis ("Warten auf das
  Laufwerk …", "Auf Upload warten"), but these two sit in body prose next to a full sentence and end in a period, so a
  bare `Warten auf …` fragment would clash. `Cmdr wartet …` keeps the active voice, matches the catalog's
  Cmdr-as-subject sentences ("Cmdr erstellt ihn beim Kopieren."), and makes the period grammatical.
- **"still open" (a file whose handle is open mid-write) → `noch geöffnet`** · macOS DE uses the participle for the
  open-file state ("Möchtest du „^0“ wirklich im geöffneten Zustand umbenennen?", "Das Umbenennen eines geöffneten
  Programms …") · high. The ICU tail after the plural block shares the branch's `ist` / `sind`, so both branches read as
  one clause: "1 Datei ist noch geöffnet und möglicherweise schon teilweise geschrieben." / "5 Dateien sind noch
  geöffnet und …". Both `geöffnet` and `geschrieben` are state passives, so the shared auxiliary is grammatical in both
  branches.
- **"may already be partly written" → `möglicherweise schon teilweise geschrieben`** · plain DE; ❌ not MS terminology's
  `partiell`, which is technical register the English deliberately avoids · high.
- **"The log has the details." → `Details stehen in der Protokolldatei.`** · reuses the catalog's own
  details-live-in-the-log frame (`askCmdr.renameUndo.refusedBatches` "Details stehen im Aktionsprotokoll.") with the
  settled `log file → Protokolldatei`; naming the FILE disambiguates it from the operation log (`Vorgangsprotokoll`),
  which is a different surface · high.
- **`close` (the button that closes the progress dialog while the transfer finishes) → `Schließen`** · macOS DE ("Close"
  → "Schließen" in Finder + AppKit, key `FR26` and `NSTouchBarCloseTemplate`), and the termbase's settled dismiss-button
  term · high. It sits next to `Abbrechen`, and the two share no stem, so the pair stays distinct.
- **"leave it running in the background" → `im Hintergrund weiterlaufen lassen`** · the settled background phrasing,
  verbatim from `transferProgress.queueTooltip` · high. The two ways out are offered with `Du kannst …` (macOS DE's
  option-offering frame, "Du kannst auch auf „Sichern unter“ klicken") rather than a bare imperative: the line points at
  two choices, it doesn't order one.

## Kopierter Pfad: die Zwischenablage-Bestätigung (`fileExplorer.clipboard.copiedPath`)

Ein Key: die Info-Toast-Zeile nach ⌘⌥C. Der Pfad selbst steht darunter in einer eigenen Monospace-Zeile, ist also KEIN
Platzhalter im Satz — der Satz endet auf einem Doppelpunkt und muss ohne den Pfad grammatisch stehen.

- **"Copied the path, it's now on your clipboard:" → `Pfad kopiert, er liegt jetzt in der Zwischenablage:`** · reuses
  the settled `clipboard → Zwischenablage` und `path → Pfad` (Termbase: "Zu Pfad gehen") · high. Das partizipiale
  `Pfad kopiert` folgt dem Muster der Geschwister-Toasts (`{countText} Objekte kopiert`). Kein `dein` vor
  `Zwischenablage`: es gibt nur eine, das Possessivum wäre im Deutschen unnatürlich (macOS sagt "in die Zwischenablage
  kopieren", nie "in deine").

## Operation queue: the queue window's rename (`queue.*`, `commands.queueShow.label`)

The English widened the queue window's name from "Transfer queue" to "Operation queue": the window lists deletes,
trashes, renames, folder and file creations, and archive edits, not only copies and moves, and "transfer" already means
copy-or-move one level down (the transfer progress dialog, the transfer driver). German widened the same way, on the
head noun the catalog had already settled for the Operation log.

- **operation (the category word for one queued/logged job) → `Vorgang` (plural `Vorgänge`)** · unchanged from §
  Operation log (2026-07-10); reconfirmed for the queue by Double Commander de, which renders this exact window
  ("Operations Viewer" / "File operations" → `Vorgänge in Warteschlange`), "Current operation:" → `Aktueller Vorgang:`,
  "Cancel Current Operation" → `Aktuellen Vorgang abbrechen`, and "File operations active" → `Laufende Vorgänge`; macOS
  Finder agrees ("Der Vorgang kann nicht abgeschlossen werden.", 58 `Vorgang` hits) · high. ❌ Still not the loanword
  "Operation", which stays reserved for the low-level concurrent-SMB sense (`settings.network.smbConcurrency`) and the
  Settings section name `Dateioperationen`.
- **"Operation queue" (the window, the View menu item, the command-palette entry) → `Vorgangswarteschlange`** ·
  `Vorgang` + the settled `queue → Warteschlange` (MS terminology; Double Commander de "Queue" → "Warteschlange") ·
  high. Written as a CLOSED compound, matching MS's own `Übertragungswarteschlange`/`Zielwarteschlange` and this
  catalog's other 20+ character closed compounds (`Vorgangsprotokoll`, `Laufwerksindizierung`,
  `Dateisystemüberwachung`); at 21 characters it is shorter than the `Übertragungs-Warteschlange` it replaces, so the
  rename costs no width. It pairs with `Vorgangsprotokoll` in the same View menu block exactly as the English pairs
  "Operation queue" with "Operation log", one head noun for both.
- **`commands.queueShow.label` dropped its verb.** The English is now the bare window name, so the German is the bare
  `Vorgangswarteschlange` too (was "Übertragungs-Warteschlange anzeigen"). It must stay byte-identical to
  `queue.windowTitle` and to the View menu item, and it sits next to the equally verbless
  `commands.logOperationLog.label` = `Vorgangsprotokoll`.
- **Queue-window headings → `Vorgänge`** (`queue.heading`, `queue.list.aria`) · the bare category plural, matching
  `operationLog.dialog.empty` ("Noch keine Vorgänge.") · high.
- **Per-row aria labels → `Diesen Vorgang anhalten` / `… fortsetzen` / `… abbrechen` / `… auswählen`** · `Vorgang` is
  masculine, so "this operation" in an accusative object slot is `diesen Vorgang`; `Diesen Vorgang abbrechen` is Double
  Commander's own wording minus its "Aktuellen" · high. ❌ Don't carry the old feminine `Diese Übertragung` frame over.
  The sibling `fileOperations.transferProgress.pauseAria`/`.resumeAria` keep `Diese Übertragung`: those sit on the
  copy/move progress dialog, where the English still says "transfer". The split is deliberate.
- **The gender flip needs the toast pronouns re-checked, not just the noun swapped.** The two queue toasts referred to
  the operation with feminine `sie` (agreeing with `die Übertragung`); with `der Vorgang` they take `er`/`ihn`:
  `backgroundedToast` → "Läuft weiter im Hintergrund. Du findest ihn in der Vorgangswarteschlange." · high.
- **`queuedToast` names the referent instead of pronouncing it**: "{countText} davor, daher wartet dieser hier noch. Du
  findest ihn in der Vorgangswarteschlange." · high. A bare `er` would be ambiguous in the one-branch ("1 Vorgang davor,
  daher wartet er noch." reads as if the one AHEAD were waiting), because `{countText}` now renders the same masculine
  noun. `dieser hier` is the English's own "this one" and pins it; the closing `ihn` then has a clear antecedent.

## Corner progress chip + the failure notice (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

Terms settled for the nine keys the background-progress chip and the failure notice added (`queue.row.dismiss*`,
`queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`). The window's own name, the head noun `Vorgang`, and
its masculine gender are unchanged: see § Operation queue (2026-08-08).

- **dismiss (a row for an operation that couldn''t finish) → `Ausblenden`**; per-row aria `Diesen Vorgang ausblenden`;
  toolbar `Alle ausblenden` · KDE Dolphin de ships this exact frame for dismissing a notice ("Dismiss This Reminder" →
  "Diesen Hinweis ausblenden"), and `ausblenden` is the catalog's settled hide-from-view verb (`Archivierte ausblenden`,
  `bleiben ausgeblendet`), which macOS de confirms for a progress surface ("Fortschrittsfenster ausblenden") · high. The
  button only stops SHOWING the row, so the hide verb is the literal one. ❌ Not `Entfernen`: the catalog's
  `Aus Liste entfernen` (`goToPath.dialog.removeFromList`) is structurally similar, but on a queue row "entfernen" reads
  as doing something to the operation itself. ❌ Not MS terminology's `dismiss → schließen`: that's the close-a-dialog
  sense, already glossed above as `dismiss (button closing a dialog) → Schließen`. The aria label completes the
  `Diesen Vorgang …` family (`anhalten` / `fortsetzen` / `abbrechen` / `auswählen`), and `Alle ausblenden` is parallel
  to `Alle anhalten` / `Alle fortsetzen`.
- **"Couldn''t finish X" (failure-notice headline) → `{nominalisierter Infinitiv} nicht abgeschlossen`** · built from
  the shipped `queue.row.status` failed arm `Nicht abgeschlossen` so the notice and the row say one thing; macOS Finder
  de backs both the head noun and the participle ("Der Vorgang konnte nicht abgeschlossen werden.", 20+ variants) ·
  high. The nine arms: `Kopieren` / `Bewegen` / `Löschen` / `In den Papierkorb bewegen` / `Umbenennen` /
  `Ordner erstellen` / `Datei erstellen` / `Archiv bearbeiten`, each + ` nicht abgeschlossen`; the `other` arm is the
  bare status word `Nicht abgeschlossen`. Object-first order matches the `queue.row.label` arms (`Ordner wird erstellt`
  → `Ordner erstellen`), and the trash arm reuses the settled chunk `In den Papierkorb bewegen`
  (`fileOperations.delete.trashSwitch`) rather than coining a second trash phrasing. ❌ Never `Fehler` /
  `fehlgeschlagen` (voice rule), and ❌ not `abgebrochen`, which is the cancelled status.
- **"{n} operation(s) couldn''t finish" → `{countText} Vorgang nicht abgeschlossen` /
  `{countText} Vorgänge nicht abgeschlossen`** · one rendering, used verbatim by BOTH `queue.failureToast.summary` and
  the first sentence of `queue.chip.failed` (same English clause on two surfaces) · high.
- **"Open the operation queue to see why." → `Öffne die Vorgangswarteschlange, um den Grund zu sehen.`** (plural:
  `… um die Gründe zu sehen.`) · the catalog's own purpose-clause frame (19 `um … zu sehen` strings) plus its reason
  noun (`queryUi.dialog.runQueryUnknownReason` "Der Grund ist unklar.") · high. **The sentence lives INSIDE both plural
  branches of `queue.chip.failed`**, unlike the English, which keeps it outside: German has to agree the reason noun
  with the count. ❌ Don't end on a bare `…, um zu sehen, warum.`: every sibling in the catalog completes the embedded
  clause. The du-imperative keeps the promise that pressing the chip opens the window.
- **"percent" spelled out (screen-reader label) → `Prozent`** · macOS de spells it out in its own progress-percentage
  string ("Percent complete: ^0" → "Prozent abgeschlossen: ^0") · high. Only the aria label spells it; the visible
  tooltip keeps the sign with the mandatory space (`{percentText} %`, DIN 5008).
- **The chip's tooltip is a dot-separated FACT LIST, with a `·` before the item count too** · the German action label is
  a passive clause (`Wird kopiert`), so English's appositive `Copying 214 items` has no grammatical German equivalent:
  `Wird kopiert 214 Objekte` fails subject-verb agreement, and a trailing `Wird kopiert nach Backup` is marked word
  order. Each fact therefore stands on its own dot: `Wird kopiert · 214 Objekte · nach Backup · 42 % · noch 1 Min. 20 s`
  · high. Every optional clause carries its own leading `·` inside its branch, so a missing part leaves no double space
  and no dangling dot; the `=0 {}` / `other {}` arms stay empty. item → `Objekt` (termbase), destination
  `to {destination}` → `nach {destination}` (Total Commander de `663="NACH: "`, Double Commander de "Nach:", the same
  pair as the transfer dialog's `Von`/`Nach` headings).
- **The trailing `{detail}` needs no term work**: it arrives already formatted from the settled ETA keys ("noch 1 Min.
  20 s") or as the status word `Angehalten`.
- **The chip surface itself is unnamed in the UI.** No string calls it a "chip", so nothing was coined; if one ever
  does, use `Fortschrittsanzeige` (settled `Fortschritt` + macOS's `-anzeige`/`Fortschrittsfenster` pattern) ·
  tentative.

## Standalone conflict prompt: the operation-context line (`fileOperations.operationConflict.context`/`.pausedNote`)

Two keys (`fileOperations.operationConflict.context` / `.pausedNote`), the line above the file comparison naming which
background operation is asking, and the note under the buttons. Head noun, verbs, and `Angehalten` are unchanged; what
was new is how the destination gets INSIDE the passive clause.

- **Destination inside the action label → verb-final `Wird nach {destination} kopiert` / `… bewegt`** · macOS Finder de
  ships exactly this frame in its own copy-progress status line (`Finder/ProgressStatusView.json` 104.title: "Es wird
  „etwas“ nach „etwas“ kopiert."), GNOME Nautilus de confirms it ("»%s« wird nach »%s« kopiert", "… verschoben"), and
  `nach` is the catalog's settled destination preposition (`transferDialog.targetGroupTitle` = `Nach`,
  `queue.chip.tooltip` " · nach {destination}", Total Commander de `663="NACH: "`) · high. The placeholder sits after
  its own preposition, so an arbitrary folder name needs no article and no case ending. ❌ Not the English word order
  `Wird kopiert nach {destination}`: that trailing form is the marked one § Corner progress chip already rejected. The
  chip has to split the fact out onto its own `·` because its `{label}` arrives pre-composed; a per-arm sentence can
  integrate it properly, so it does.
- **`bewegen` takes `nach` here, not macOS's `in`** · macOS pairs `bewegen` with `in` + accusative ("in „^1“ bewegt"),
  but that frame wants a definite target ("in den Papierkorb"), while `nach X bewegt` is attested too (AppKit
  `TouchBar.json` "%1$@ nach %2$@ an den Index %3$ld bewegt") · high. One preposition for copy and move keeps this line
  parallel with the chip tooltip and the transfer dialog's `Von`/`Nach` headings, which never split by verb.
- **`archive_edit` names the archive as the subject → `{destination} wird bearbeitet`** (generic arm:
  `Ein Archiv wird bearbeitet`) · the sibling `queue.row.label` arm is the article-less generic
  `Archiv wird bearbeitet`; the English deliberately splits "Editing archive" (queue) from "Editing an archive" (here),
  so the German mirrors the article the way the Ask Cmdr tool pairs do (`Ein Ordner wird aufgelistet`) · high.
  Subject-first keeps `{destination}` nominative, so an arbitrary archive name needs no inflection.
- **"Working in {destination}" → impersonal passive `In {destination} wird gearbeitet`** · tentative. No pile source
  names this state (the fallback arm covers operation kinds that don't exist yet), but the impersonal passive is
  ordinary German ("Hier wird gearbeitet") and keeps the arm inside the `Wird …` family; the bare fallback arm stays the
  sibling's `In Arbeit`. ❌ Not `Arbeitet in {destination}`: German isn't pro-drop, so a bare finite verb has no
  subject.
- **"Everything else is paused until you answer." → `Alles andere ist angehalten, bis du antwortest.`** · `angehalten`
  is the settled `queue.row.status` paused arm (macOS "wurde angehalten"), and `…, bis du {Präsens}` is the catalog's
  own frame (`errors` "bis du es entsperrst", `settings` "bis du ihn löschst") · high. State passive `ist angehalten`,
  not `wurde angehalten`: the line describes the situation now, and it reassures rather than reports an event.
  `antworten` is the settled respond verb (`transferProgress.stallWaitingDestination` "eine Antwort vom Ziel").

## The progress dialog's empty-queue button (`fileOperations.transferProgress.background`/`.backgroundAria`)

Two keys (`transferProgress.background`, `.backgroundAria`): the same button as `.queue`, worded for an EMPTY operation
queue. "Background" is a VERB in the English; German says that action with the preposition, never with the bare noun.

- **"Background" (button label, empty queue) → `Im Hintergrund`** · the settled background phrasing
  (`im Hintergrund weiterlaufen lassen`, § Operation queue and `transferProgress.queueTooltip`) shortened to its
  prepositional head; the pile's action pattern is always `im Hintergrund` + verb (Total Commander de
  `1185="Im &Hintergrund laden"`, `1189="Im &Hintergrund senden"`, `1214="Übertragung im Hintergrund"`; Double Commander
  de "Im Hintergrund ausführen", "Vorgänge anzeigen, die im Hintergrund ablaufen") · high. At 14 characters it fits the
  same button as the 13-character `Warteschlange`.
- ❌ **Not the bare noun `Hintergrund`**, even though Total Commander de ships exactly that on this very button
  (`4004="&Hintergrund"`, sitting right next to `4005="Warteschlange"`): standing alone, `Hintergrund` is the BACKDROP
  in every higher-tier source (macOS de has only `Hintergrundfarbe`, `Hintergrundbild`, `Hintergrund:` in view options;
  MS terminology glosses `background` → `Hintergrund` with wallpaper senses; Nautilus "Als Hintergrund festlegen";
  Dolphin "Hintergrund der Ansicht"), and this catalog uses it that way itself (`Bereichshintergrund`,
  `Hintergrund-Farbton`). The preposition is what turns the thing into the action: `im` can only answer "wo läuft das
  weiter?", so the label can't be read as a backdrop setting.
- ❌ **Not `In den Hintergrund`** (accusative, directional): that is the German for sending a WINDOW behind the others
  (z-order), so on a dialog it reads as "hide this window" rather than "keep the copy running". The dative `im` says the
  transfer keeps running there, which is what the button does.
- **"Keep this running in the background" (screen-reader label) → `Im Hintergrund weiterlaufen lassen`** ·
  byte-identical to the settled phrase and to the leading clause of `transferProgress.queueTooltip` · high. Infinitive
  command, exactly like the sibling `queueAria` = `Zur Vorgangswarteschlange senden`; the visible label is its
  elliptical short form, the same label/aria split the sibling pair already ships.
- **WCAG 2.5.3 (Label in Name) holds by construction**: the aria STARTS with the visible label, so `Im Hintergrund` is
  an exact substring of `Im Hintergrund weiterlaufen lassen`, capitalization included. German case marking is the trap
  here: a directional label (`In den Hintergrund`) would force the aria to be rebuilt around the accusative and to drop
  the settled wording. If either string is ever re-worded, keep the label a prefix of the aria.

## The quit gate (`main.quit.*`)

Seven keys: the modal Cmdr raises when ⌘Q lands while a copy, move, delete, trash, rename, create, or archive edit is
still running. Head noun `Vorgang` and the running status `Läuft` are unchanged (§ Operation queue).

- **"Quit while N operation(s) are running?" (title) → `Ein Vorgang läuft noch. Trotzdem beenden?` /
  `{countText} Vorgänge laufen noch. Trotzdem beenden?`** · high. The English single clause becomes the catalog's own
  state-then-question shape, which is what every tier ships for this exact dialog: Total Commander de renders the
  quit-with-running-operations warning as `WARNUNG: %i Operationen aktiv im Hintergrund!\nTrotzdem beenden?`
  (`WCMD.LNG.utf8` 1237), macOS AppKit renders `Quit Anyway` → `Trotzdem beenden` (`Document.json`), and the shipped de
  catalog already uses the pattern next door (`Dieser Tab ist fixiert. Trotzdem schließen?`, itself mirroring TC's
  `Dieser Tab ist gesperrt! Trotzdem schließen?`). ❌ Not the literal `Beenden, während ein Vorgang läuft?`: a bare
  infinitive plus a `während`-clause is stiff, and the pile has no such title. ❌ Not TC's loanword `Operationen` —
  `Vorgang` is settled. The verb is the queue's own `läuft` (`queue.row.status` running arm), so the title, the row
  list, and the queue window all say one thing.
- **"Still running" (heading over the operation rows) → `Noch aktiv`** · macOS Finder de says exactly this about
  in-progress Finder jobs ("The Finder can''t quit because some operations are still in progress." → `A17` = "Der Finder
  kann nicht beendet werden, da noch Vorgänge aktiv sind.", and `A19` "… da auf dem iOS-Gerät noch ein Vorgang aktiv
  ist.") · high. Deliberately NOT a second `Läuft noch` two lines under the title: German repeats badly at that
  distance, and Apple's own word for the same state is `aktiv`. ❌ Not Double Commander's `Laufende Vorgänge`
  (`rsmsgfileoperationsactive`): that names the rows as a noun phrase, while the English heading is a bare state and the
  rows below already say `Wird kopiert`.
- **"Keep working" (the button that calls the quit off) → `Weiterarbeiten`** · tentative — no pile source names this
  button, so it's constructed. It's built from the catalog's settled continue-sense `weiter` (`Weiter umbenennen`,
  `Läuft weiter im Hintergrund`, `Im Hintergrund weiterlaufen lassen`) and reads as "carry on", never as "later". ❌
  Absolutely not `Abbrechen`, even though that's macOS's word on an unsaved-changes quit alert: in Cmdr's German
  `abbrechen` IS the cancel-the-operation verb (`Diesen Vorgang abbrechen`, `Alle abbrechen`), so on this dialog it
  would read as the exact opposite of what the button does. ❌ Not `Später` / `Nicht jetzt`: the countdown is deleted,
  not deferred.
- **"Quit now" → `Jetzt beenden`** · quit → beenden (macOS `Beenden`, `Finder beenden`); the load-bearing "now" is macOS
  Finder's own `jetzt beenden` ("Du kannst das Kopieren jetzt beenden oder …", `NE111`) · high. ❌ Not `Sofort beenden`,
  which is Apple's Force Quit (termbase above) and would promise a hard stop.
- **The countdown →
  `Cmdr beendet sich in {secondsText} Sekunde(n), damit ein Neustart oder eine Abmeldung nie darauf warten muss.`** ·
  high. Active reflexive `beendet sich` over macOS's passive `wird beendet` per the active-voice rule, and it keeps the
  sentence to one `Cmdr` (the closing `darauf` carries the second mention, which a literal "nie auf Cmdr warten muss"
  would repeat). restart → `Neustart` and logout → `Abmeldung` are the settled terms (`Neustart`, "logging out" →
  `das Abmelden`); the nominal `eine Abmeldung` is used here so it stays parallel with `ein Neustart`. Only
  `Sekunde`/`Sekunden` differs between the two branches, which is exactly what the plural block is for.
- **"Time until Cmdr quits on its own" (aria) → `Zeit, bis sich Cmdr von selbst beendet`** · high. `von selbst` is the
  catalog's own self-acting phrase (`errors` "der sich … von selbst klärt"). No visible label to contain, so WCAG 2.5.3
  doesn't bind this one; it just names what the number measures.
- **"clears away what it leaves half-written" → `entfernt, was dabei halb geschrieben zurückbleibt`** ·
  `halb geschrieben` is already shipped in the de catalog (`settings.advanced.showStagingTempFiles.description`), and
  remove → `entfernen` is settled · high. A free relative, NOT the definite `die halb geschriebene Datei`: a definite
  noun phrase can't stay number-neutral (see below). The English picks the warm "clears away" over "deletes"; German has
  no attested warm equivalent (`wegräumen` is unsourced, and macOS's `Aufräumen` is Finder's tidy-icons command), so the
  neutral `entfernen` carries it and the warmth sits in the opening `Was fertig ist, bleibt fertig.` ❌ Not `löschen`:
  that's the user-facing delete verb and would read as data loss on a dialog whose whole job is reassurance.
- **"anything still being written" → `Alles, was gerade geschrieben wird`** · **the body must stay number-neutral**: one
  operation writes several files at once and several operations can run at once, so a singular
  (`Das eine Objekt, das gerade geschrieben wird`) states something false · high. `Alles, was` scopes it without a
  numeral, and it keeps the sentence off a third `was` clause. It "stops where it is" → `stoppt genau dort, wo es ist`;
  `stoppen` is macOS Finder's verb for halting a running job (`CP5` "Kopieren stoppen", `NE111.1` "den Vorgang
  stoppen"), kept distinct from the button verb `abbrechen`.

## Usage stats: "anonymous" dropped, "a random id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudonym` / `pseudonymisiert` —
that jargon is exactly what the copy avoids.

- **usage stats → `Nutzungsstatistiken`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anonyme` adjective was cut. Both keys now use the plural, matching English's one shared value · high
- **a random id → `eine zufällige ID`** · MS terminology (random → `zufällig`) · high. ❌ Not `Bezeichner` (MS for
  "identifier"): technical and rare in everyday German; `ID` is what a Mac user already knows (Apple-ID).
- **tied to → `verknüpft mit`** · the catalog's own verb for this exact relation (`onboarding.stepBeta.emailNote` "nie
  mit deinen Nutzungsstatistiken verknüpft") · high

## Conflict-parked queue rows and the rollback confirmation (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- **"Needs your answer" (queue status chip) → `Antwort erforderlich`** · macOS `de` ("Authentifizierung ist
  erforderlich, damit „^0“ den Vorgang abschließen kann.") · high. Deliberately neutral, not the du-address "Braucht
  deine Antwort": the style guide says to keep direct address light where German phrases neutrally, and the sibling
  chips are terse (`Wartet`, `Läuft`, `Angehalten`). ❌ Never anything built on `warten` here — `Wartet` is the
  QUEUED-behind-another-operation status, and the two must stay distinguishable in the same narrow column.
- **"prompt" (the on-screen question the operation is parked on) → `die Frage`** · matches
  `operationConflict.pausedNote` ("bis du antwortest") and the conflict step's own wording · high. Not
  `Eingabeaufforderung` (MS's `prompt` entry is the command-prompt sense).
- **"this operation carries on" → `dann läuft dieser Vorgang weiter`** · settled `operation → Vorgang` +
  `laufen`/`Läuft` from the queue status chips · high.
- **rollback dialog: verb in prose, noun on the button.** Title `Diesen Vorgang rückgängig machen?` uses the settled
  prose verb (`roll back / undo → rückgängig machen`); the confirming button is the bare technical noun `Rollback`, so
  it matches the `Rollback` button the user just pressed (`transferProgress.conflictRollback`), exactly the split this
  termbase already records for the status chips · high.
- **"Keep them" (the safe answer) → `Dateien behalten`** · macOS `de` ("Behalten", "Original behalten", "Beide Dateien
  behalten") · high. The noun is kept: standalone `Behalten` right after a body sentence that also names the REPLACED
  files would be ambiguous about which files stay.
- **"written so far" → `bisher geschrieben`** · reuses the catalog's `written → geschrieben`
  (`transferProgress.stallInFlight` "möglicherweise schon teilweise geschrieben") · high.
- **"won't come back" (an overwritten file is gone) → `kommen nicht zurück`** · plain register matching the deliberately
  plain English; the replaced-file noun is `Ersetzte Dateien` (settled `replace → ersetzen`) · high.
- **"Stop, and …" (rollback tooltip) → `Stoppen und …`** · macOS Finder ("Kopieren stoppen", "Löschen stoppen", "Bewegen
  stoppen") · high. Kept distinct from `abbrechen`, which is the plain Cancel the tooltip must NOT read like.
- **foregroundBusyToast: name the operation, don't lean on a pronoun.** English's "bring this one up" has no antecedent
  in German, so the value spells it out: "… und zeige dann diesen Vorgang an", reusing the `Anzeigen` button label
  (`queue.row.foreground`) so the instruction names the control the user has to press · high.

## Rename chaining: the counted "and so did N others" toast (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

The growing warning toast for an arrow-key rename run: it names the most recent file that kept its name and counts the
earlier ones. Must read as one voice with its sibling `fileExplorer.rename.chainKeptOriginalName` („{name}“ behält
seinen Namen.), same quotes, same verb.

- **"and so did N other files" → `ebenso {othersText} weitere Dateien`** · macOS Finder `de` renders the counted-other
  tail elliptically (`Alle neueren Objekte wie „^1“ und ^0 weitere werden beibehalten.`, key `PE106_V4`; the `V3`
  singular is "^0 weiteres"), and uses `ebenso` for "so did/too" ("Geteilte Objekte in diesem Ordner werden ebenso
  gelöscht.") · high. The gapping construction (`„A“ behält seinen Namen, ebenso 3 weitere Dateien.`) drops the repeated
  verb, so the singular/plural agreement question never arises, and it stays short — welcome for a toast in the longest
  of Cmdr's languages.
- **`weitere`, not macOS's alternative `andere`** ("und ^0 andere Objekte", `MR101_V3`): the sense here is "N additional
  files did the same", and the catalog already renders every counted trailing tail with `weitere`
  (`fileOperations.errorDialog.tooLargeAndMore`, `operationLog.dialog.moreItems`) · high.
- **The `one` branch spells the word out (`eine weitere Datei`), no number** · mirrors the en source's "one other file"
  and reads better than "1 weitere Datei"; `{othersText}` still carries every count ≥ 2 · high.

## Unconfirmed rename + the catch-all name rejection (`fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

The `unconfirmed*` pair is the sibling of the `chainKept*` pair above and shares its toast shape, but carries the
opposite meaning: `chainKept*` says the file definitely kept its name, `unconfirmed*` says Cmdr couldn't tell and the
rename may well have gone through. The German must never let the two blur.

- **"Couldn't confirm the rename of X" → `Es ließ sich nicht bestätigen, dass „X“ umbenannt wurde`** · the catalog
  already settles this exact frame twice for the same situation: `fileExplorer.pane.trashUnconfirmedToast` ("Es ließ
  sich nicht bestätigen, dass die Datei in den Papierkorb bewegt wurde.") and `fileOperations.mkdir.timeoutMessage`
  ("Die Erstellung des Ordners ließ sich nicht bestätigen.") · high. The `dass`-clause is preferred over the noun frame
  (`Die Umbenennung von „X“ …`) because it keeps `{name}` nominative, dodging the case trap the style guide flags: the
  noun frame needs `von` + dative, which then forces `einer weiteren Datei` / `{othersText} weiteren Dateien` in the
  plural branches and diverges from the `chainKept*` pair's nominative wording.
- **The plural branches reuse `chainKeptOriginalNameAndOthers` verbatim** (`eine weitere Datei` /
  `{othersText} weitere Dateien`), so the two toast pairs read as one voice · high.
- **"so the rename may still have gone through" → `die Umbenennung hat also womöglich trotzdem geklappt`** (counted
  variant: `die Umbenennungen haben also …`) · `womöglich trotzdem` is the catalog's settled hedge for exactly this
  timeout case (`mkdir.timeoutMessage`: "der Ordner wurde also womöglich trotzdem erstellt"), and `klappen` is in-voice
  for a Cmdr outcome (`fileOperations.archivePassword.retryTitle` "Das hat nicht geklappt",
  `onboarding.stepBeta.signup.failure`) · high. Name the subject (`die Umbenennung`) the way the en source and
  `mkdir.timeoutMessage` both do; a bare `sie`-pronoun would point at `Dateien` and read as the files (not the renames)
  having worked out. `Umbenennung`/`Umbenennungen` as a noun is attested in the pile (Thunar, Double Commander: "mit der
  Umbenennung der restlichen Dateien fortfahren", "die bisher durchgeführten Umbenennungen rückgängig machen").
- **"The volume may be slow" → `Das Volume ist vielleicht langsam`** · verbatim from the two sibling timeout toasts
  (`fileExplorer.pane.trashUnconfirmedToast`, `fileOperations.mkdir.timeoutMessage`), so all three hedge alike · high.
  `volume → Volume` is settled in `style.md`.
- **"That filename can't be used" → `Dieser Dateiname kann nicht verwendet werden`** (folder: `Dieser Ordnername …`) ·
  macOS Finder `de` is decisive for this exact catch-all: "Der Name „^0“ kann nicht verwendet werden.", plus the
  reason-carrying variants "… da er vom System reserviert ist." / "… da er zu lang ist." · high. `Dieser` (not `Der`)
  mirrors the source's deictic "That"; the passive matches the English's own passive and the sibling validation keys'
  register (`Der Dateiname darf nicht leer sein`, `… ist zu lang`). No closing period: the value is composed into
  `fileExplorer.rename.keptOriginalName` / `.chainKeptOriginalName`, which supply it.

## Duplizieren: der Befehl, der im selben Ordner kopiert (`commands.fileDuplicate.*`)

- **duplicate (Befehl, der die Auswahl in ihren eigenen Ordner kopiert) → `Duplizieren`** · macOS Finder `de`, Menü
  „Ablage > Duplizieren“ (`N154`), dazu „Objekte duplizieren“ und „Dupliziert Objekte an ihrem aktuellen Ort“ (geprüft
  auf macOS 26.6.1, `Finder.app/Contents/Resources/de.lproj`, 2026-08-19) · high. Steht neben `Kopieren` (F5) und
  `Bewegen` (F6) und bleibt davon klar unterscheidbar.
- **„Make a copy of the selected files in the same folder“ →
  `Eine Kopie der ausgewählten Dateien im selben Ordner erstellen`** · Infinitiv wie die Nachbarbeschreibungen
  (`commands.editCopy.description`: „… kopieren“); „im selben Ordner“ meint den Ordner, in dem die Dateien schon liegen
  · high.

## Native Menüs: Menüleiste, Kontextmenüs, Fenstertitel (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Quellenlage für diese ganze Gruppe: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/de.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) ist Tier 1 und entscheidet fast alles; die englische Seite steht in `en_GB.lproj`, weil
`Base.lproj` nur kompilierte Nibs enthält. Safari 26 (`MainMenu.strings`) liefert die Browser-Tab-Wörter,
MS-Terminologie die Begriffe, die Apple gar nicht hat. Rohfamilie: **einfache Apostrophe**, `''` würde im Menü doppelt
erscheinen.

- **File-Menü → `Ablage`** · macOS Finder + Safari `de` (`300764.title`, `83.title`) · high. Nicht „Datei“: Apple nennt
  das Menü seit jeher „Ablage“, und das ist es, was Nutzende in jedem Mac-Programm sehen.
- **View-Menü → `Darstellung`**, **Go → `Gehe zu`**, **Window → `Fenster`**, **Help → `Hilfe`**, **Services →
  `Dienste`** · macOS Finder Tier 1 · high.
- **Select-Menü (Dateiauswahl) → `Auswählen`** · Nautilus/Thunar/Dolphin `de` („Auswählen“) · high. Finder hat kein
  Gegenstück; das Verb passt zu `Alles auswählen` im selben Menü.
- **Minimize → `Im Dock ablegen`** · macOS Finder `de` (`300666.title`) · high. Überraschend, aber genau das steht im
  deutschen Fenstermenü; „Minimieren“ ist die AppKit-Variante und nicht das, was der Finder zeigt.
- **Window-Zoom → `Zoomen` (Verb)** vs. **Text-Zoom-Untermenü → `Zoom` (Substantiv)** · macOS Finder (`300667.title`)
  bzw. Browserüblich · high. Die beiden Bedeutungen bleiben so unterscheidbar, obwohl das Englische zweimal „Zoom“ sagt.
- **Quick Look → `Übersicht`** · macOS Finder (`TL14`) · high. Apple lokalisiert diesen Feature-Namen, deshalb steht er
  NICHT auf der Don't-translate-Liste.
- **Get Info → `Informationen`**, **Enclosing Folder → `Übergeordneter Ordner`**, **Go > Home → `Benutzerordner`**,
  **Sort By → `Sortieren nach`** · macOS Finder Tier 1 · high. **Show in Finder** heißt auch im Menü
  `Im Finder anzeigen`, wie der Paletten-Befehl: `Im Finder zeigen` ist macOS' Wort für die Quelle „Reveal in Finder“
  (§ Ein englisches Wort, ein deutsches Wort).
- **Undo → `Widerrufen`, Redo → `Wiederholen`, Paste → `Einsetzen`** · macOS Finder `de` · high. Nicht „Rückgängig“ /
  „Einfügen“ (Windows-Konvention).
- **ascending / descending → `Aufsteigend` / `Absteigend`** · Thunar + Dolphin `de` · high. Der Finder sortiert über
  „Sortieren nach“ ohne eigene Richtungswörter.
- **changelog → `Änderungsprotokoll`** · MS-Terminologie („change log“) · high. Abgegrenzt von Help > `Neuigkeiten`
  (What's new): das eine nennt das Dokument, das andere die Nachricht.
- **word wrap → `Zeilenumbruch`** · MS-Terminologie · high. `Textumbruch` ist die zweite MS-Variante; `Zeilenumbruch`
  ist im Editor-Kontext das gängigere Wort.
- **pin / unpin tab → `Tab fixieren` / `Tab lösen`** · Safari `de` („Tab fixieren“) · high. Das Gegenstück ist bei Apple
  nicht belegt; `lösen` ist die natürliche Umkehrung und deckt sich mit `commands.tabTogglePin.label` („Tab
  fixieren/lösen“).
- **Finder-Tag-Farben → `Rot, Orange, Gelb, Grün, Blau, Lila, Grau`** · macOS Finder (`TG_COLOR_*`) · high.
- **Tag-Leiste (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Tags`, `„{color}“ hinzufügen`,
  `„{color}“ entfernen`** · macOS Finder (`TG5` = `„^0“ hinzufügen`, `TG6` = `„^0“ entfernen`, `N169.37` = `Tags`) ·
  high. Dasselbe Verbpaar wie `commands.tagsToggleRed.description` („Fügt … hinzu oder entfernt ihn“).
- **busy (Volume in Benutzung) → `(in Benutzung)`** · beschreibend · tentative. MS bietet nur `beschäftigt`/`besetzt`
  (Person bzw. Telefon); für eine Festplatte liest sich beides falsch.
- **„Eject“ → `Auswerfen`, „Disconnect“ → `Trennen`, „Remove“ (aus einer Liste) → `Entfernen`** · macOS Finder · high.
- **forget (Server, Passwort) → `vergessen`** · bereits im Katalog (`fileExplorer.network.share.forgetPassword`) · high.
- **Deliberately identical to English** (`sameAsSourceJustification` gesetzt): `menu.bar.tab` (Tab), `menu.view.zoom`
  (Zoom), `menu.sort.name` (Name), `menu.tag.orange` (Orange), `menu.tag.rowLabel` (Tags), `menu.view.askCmdr`
  (Produktname).

## SMB-Fallback-Hinweis: die Freigabe hängt an der Systemverbindung (`fileExplorer.network.osMountFallback.*`)

- **"macOS's native SMB network connection" → `die eigene SMB-Netzwerkverbindung von macOS`** · Katalog-Konsistenz mit
  `Systemverbindung` (siehe unten) plus die Stilregel „kein Genitiv auf Markennamen“ · high. `nativ` bleibt draußen: die
  de-Referenzsammlung belegt das Adjektiv nirgends im UI-Deutsch (nur in englischen MS-`descrip`-Texten),
  `eigene … von macOS` sagt dasselbe mit deutschen Mitteln.
- **system connection (die vom OS bereitgestellte SMB-Verbindung) → `Systemverbindung`** · bereits im Katalog gesetzt
  (`fileExplorer.pane.directConnectionTooSlowToast`/`.directConnectionUnexpectedToast`,
  `fileExplorer.navigation.connectionTooltipSystem`, `fileOperations.transferDialog.smbNativeNote`) · high. Der
  Fallback-Hinweis beschreibt dieselbe Verbindung ausführlicher, bleibt aber im selben Wortfeld.
- **"Couldn't directly connect to X" → `Die direkte Verbindung zu X kam nicht zustande.`** · wortgleicher Rahmen aus
  `fileExplorer.pane.directConnectionUnexpectedToast` · high. Kein „Fehler“, kein „fehlgeschlagen“ (Stilregel), und der
  fehlende Subjekt-Slot des Englischen wird durch das Nominalsubjekt gefüllt, statt Cmdr die Schuld zu geben.
- **"Try connecting directly" (Taste im Hinweis) → `Direkte Verbindung versuchen`** · deckt sich mit
  `fileExplorer.navigation.connectingDirectly` („Direkte Verbindung wird hergestellt …“) und dem gesetzten „Connect
  directly“ → „Direkt verbinden“ · high. ❌ Nicht „Direkt verbinden versuchen“: zwei Infinitive hintereinander lesen
  sich holprig; das Nomen `Direkte Verbindung` trägt dieselbe Bedeutung.
- **"Dismiss" (Tooltip auf dem X einer Benachrichtigung) → `Schließen`** · identisch zu
  `lowDiskSpace.toast.closeTooltip` (gleicher `sourceHash`) und zum gesetzten Eintrag „dismiss (button closing a dialog)
  → Schließen“ · high. Abgegrenzt von `Ausblenden`, das für das Wegräumen einer Vorgangszeile reserviert ist.
- **Multiplikatoren `4x` / `100x` → `4-mal` / `100-mal`** · Duden-Schreibung mit Ziffer (Bindestrich) · high. In der
  Referenzsammlung ist kein Multiplikator belegt, deshalb entscheidet die Rechtschreibnorm. Das Englische klebt das
  „(sometimes 100x)“ mitten in den Vergleich; im Deutschen muss `langsamer … als` zusammenbleiben, also wandert der
  Nachtrag ans Satzende: „… 4-mal langsamer ist als die direkte Verbindung von Cmdr, manchmal sogar 100-mal.“
- **button (Bedienelement auf dem Bildschirm) → `die Taste`** · macOS de („klicke … auf die Taste „+“ (Hinzufügen)“) und
  der bestehende de-Katalog („Klicke unten auf die Taste „+“…“, „klicke einfach auf die Taste unten“) · high. ❌ Nicht
  MS-Terminologies `Schaltfläche`: das ist die Windows-Konvention.

## Umbenennen und Anlegen: die abgewiesenen Mutationen (`errors.mutation.*`, `errors.volume.*`)

31 einzeilige Meldungen unter dem Namensfeld von „Umbenennen“ / „Neuer Ordner“ / „Neue Datei“, oder als kurzer Hinweis.
Rohfamilie (`errors.*`): **einfache Apostrophe**, `{path}` bleibt wörtlich stehen, und `{path}` ist ein unkontrollierter
Einschub, steht deshalb überall in der eigenen deutschen Anführung „…“ und nie in einem Kasus-Slot.

- **System Integrity Protection → `Systemintegritätsschutz`** · Apple Support DE, „Informationen zum
  Systemintegritätsschutz auf dem Mac“ (support.apple.com/de-de/102149, geprüft 2026-08-23) · high. ❌ Nicht Apples
  eigener Finder-String `System-Integrationsschutz` (`Finder/LocalizableMerged` `ET6`, macOS 26.5.2: „Einige Objekte im
  Papierkorb konnten aufgrund des System-Integrationsschutzes nicht gelöscht werden.“): das ist eine sichtbare
  Fehlübersetzung (Integration statt Integrität) in genau einem String, und Apples eigene deutsche Dokumentation
  schreibt den Feature-Namen anders. Beide teilen den Kopf `System…schutz`, die Doku-Form ist die korrekte. Nachgeprüft
  am 2026-08-24 auf macOS 26.5.2: `ET6` steht en/de Schlüssel an Schlüssel gegenüber, der Befund stimmt. Weil das eine
  Produktstimmen-Entscheidung ist (Apples Bildschirmwort kopieren oder Apples korrektes Doku-Wort nehmen), steht sie
  zusätzlich in `review-queue.md`; bis dahin bleibt `Systemintegritätsschutz` stehen.
- **„a volume's top folder“ → `der oberste Ordner eines Volumes`** · GNOME Nautilus `de` liefert genau diese Absage
  („Toplevel files cannot be renamed“ → „Dateien im obersten Ordner können nicht umbenannt werden“, `nautilus.po`) ·
  high. MS-Terminologie hat `Stammordner` (Definition „The uppermost directory on a computer, partition or volume“,
  Term-ID 233488), das ist die Windows-Form; der Finder hat keine Entsprechung, also gewinnt die Explorer-Familie.
  `volume → Volume` bleibt gesetzt (`style.md`).
- **„Out of space“ / „no room left“ → `kein Platz mehr`** · macOS Finder `de` (`AXBADGE10` und `NE88.10` = „Out of
  space“ → „Kein Platz mehr“) · high. Der ausführliche Geschwisterstring bleibt bei „nicht genügend freier Speicher“
  (`errors.listing.storageFull.explanation`); die Kurzform nimmt Apples eigenes Wort.
- **„didn''t answer in time“ → `hat nicht rechtzeitig geantwortet`** · macOS AppKit `de` („… did not finish in time“ →
  „… wurde nicht rechtzeitig beendet“, `AppKitErrors`) plus das gesetzte `respond → antworten` · high.
- **`timedOut`: kein Misserfolg, sondern eine offene Frage →
  `Das Volume hat noch nicht geantwortet, die Änderung klappt also womöglich trotzdem noch.`** · `womöglich trotzdem`
  ist die im Katalog gesetzte Hedge für genau diesen Timeout-Fall (`fileOperations.mkdir.timeoutMessage`,
  `fileExplorer.rename.unconfirmed`), `klappen` ist die Katalogstimme für einen Cmdr-Ausgang · high. ❌ Nichts mit
  `Fehler`, `fehlgeschlagen` oder `abgebrochen`: der Vorgang läuft noch und kann noch gelingen.
- **`deviceSessionReset`: das Gerät steckt weiter →
  `Das Gerät hat seine Verbindung neu gestartet. Warte ein paar Sekunden und versuche es dann erneut.`** · wortgleich
  zum schon ausgelieferten Geschwisterpaar `errors.listing.deviceReconnecting.explanation` („… wurde neu gestartet … Das
  Gerät ist weiterhin angeschlossen“) und `.suggestion` („Warte ein paar Sekunden und versuche es dann erneut.“) · high.
  ❌ Kein Wort vom Abziehen oder Trennen.
- **„went through“ / „finish“ (eine Änderung kommt an) → `abgeschlossen`** · gesetztes `complete → abgeschlossen`, macOS
  Finder („Der Vorgang kann nicht abgeschlossen werden.“) · high. `deviceDisconnected` = „… bevor die Änderung
  abgeschlossen war.“, `volume.ioError` = „Das Volume konnte das nicht abschließen.“
- **„at your request“ (selbst abgebrochen) → `auf deinen Wunsch abgebrochen`** · Satzrahmen von macOS Finder `MR5`
  („„^0“ hat den Kopiervorgang abgebrochen.“), `abbrechen` ist der gesetzte Cancel-Verb · high für den Rahmen,
  `tentative` für den Baustein `auf deinen Wunsch`: die Referenzsammlung kennt keinen String für „vom Nutzer
  abgebrochen“, die Wendung ist Standarddeutsch, aber unbelegt. Neutral gehalten, nicht entschuldigend.
- **„lost track of“ → `aus den Augen verloren`** · idiomatisch für „den Überblick verlieren, obwohl es die Sache noch
  gibt“ · tentative (kein Beleg in der Sammlung). Wichtig für die Bedeutung: der Zielordner existiert weiter, nur Cmdrs
  Handle ist veraltet, deshalb ❌ nicht „findet den Ordner nicht mehr“ (das liest sich wie gelöscht).
- **`deletePending` →
  `Die Datei ist auf dem Weg hinaus, und etwas hält sie noch geöffnet. Versuche es gleich noch einmal.`** · „auf dem Weg
  hinaus“ steht schon im Katalog (`errors.listing.deletePending.explanation`), „Versuche es gleich noch einmal“ ist die
  gesetzte Retry-Wendung · high.
- **`fileLocked` → `geschützt` + `„Informationen“`** · gesetztes `locked → geschützt` und `Get Info → Informationen`;
  der Satz kürzt den ausführlichen Geschwisterstring `errors.write.fileLocked.suggestion.mac` („Hebe den Schutz im
  Finder auf …“) · high.
- **Archivbearbeitung (das Umschreiben eines Zips) → `Archivbearbeitung`; „edits aren''t ready“ →
  `lassen sich noch nicht bearbeiten`** · gesetztes `edit → bearbeiten` (§ Archive browsing) als geschlossenes
  Kompositum wie `Vorgangsprotokoll` · high.
- **„Renaming can't take an item out of / from one archive to another“ →
  `Beim Umbenennen kann ein Objekt sein Archiv nicht verlassen.` / `… nicht von einem Archiv in ein anderes wechseln.`**
  · `item → Objekt`, `archive → Archiv`; der gemeinsame Kopf `Beim Umbenennen kann ein Objekt …` hält die beiden
  Geschwister parallel · high. Der Nachsatz `Bewege es stattdessen.` nutzt das gesetzte `move → bewegen` (Finder), damit
  er auf Cmdrs Befehl `Bewegen` zeigt.
- **`invalidName` → `Das Ziel kann diesen Namen nicht speichern. Wähle einen anderen.`** · übernimmt die Formulierung
  des Geschwisterstrings `errors.listing.invalidName.explanation` („einen Namen, den das Ziel nicht speichern kann“);
  die Lösung ist immer ein anderer Name, nie ein erneuter Versuch, deshalb steht der Imperativ am Ende · high.
- **`passwordRejected` → `Das Passwort hat nicht geklappt.`** · `fileOperations.archivePassword.retryTitle` = „Das hat
  nicht geklappt“ · high. Das Passwort ist das Subjekt, nicht die Person.
- **Existenz-Sätze bleiben beim gesetzten `gibt es`-Idiom**: `Unter „{path}“ gibt es nichts mehr.` /
  `Unter „{path}“ gibt es bereits etwas.` · dasselbe Idiom wie `conflictExistsFolder` und
  `errors.write.destinationExists.message` · high. Die Präposition `Unter` hält `{path}` außerhalb jedes Kasus-Slots.

### Nachtrag: die beiden Papierkorb-Absagen (`errors.mutation.trashNotSupported`/`.trashRefused`)

- **`trashNotSupported` → `Dieses Volume hat keinen Papierkorb, deshalb kannst du hier nur endgültig löschen.`** ·
  gesetztes `trash → Papierkorb` (`terms.json`, macOS Finder durchgängig) und `volume → Volume` (`style.md`); der
  `…, deshalb …`-Rahmen hält den String parallel zu seinen Geschwistern `volumeGone` und `sipProtected` · high. Der
  Geschwisterstring `errors.write.trashNotSupported.message` („Dieses Volume unterstützt keinen Papierkorb.“) sagt
  dasselbe für den Dialog; die Zeile hier nennt zusätzlich den Ausweg.
- **„delete permanently“ als Aktionsname → `endgültig löschen`, nicht `dauerhaft löschen`** · der Katalog nennt den
  Befehl überall `Endgültig löschen` (`commands.fileDeletePermanently.label`, `menu.file.deletePermanently`,
  `fileExplorer.functionKeyBar.deletePermanentlyAction`, und `errors.write.trashNotSupported.suggestion` zeigt mit „… um
  stattdessen endgültig zu löschen“ auf genau ihn) · high. Das gilt auch für die adverbiale Prosa
  (`fileOperations.delete.noTrashWarningRest` „Dateien werden endgültig gelöscht.“): der Löschdialog sagte als Einziger
  „dauerhaft gelöscht“ direkt neben der Taste „Endgültig löschen“. `dauerhaft` bleibt dem ANDEREN Sinn von
  „permanently“ vorbehalten, einer Einstellung „für immer“ („die Warnung dauerhaft unterdrücken“,
  `settings.fileViewer.suppressBinaryWarning.description`).
- **`trashRefused` → `macOS wollte das nicht in den Papierkorb bewegen.`** · `in den Papierkorb bewegen` ist der
  gesetzte Baustein (macOS Finder „Trash ${entities}“ → „${entities} in den Papierkorb bewegen“, AppKit „could not be
  moved to the trash“ → „konnten nicht in den Papierkorb bewegt werden“) · high für den Baustein, `tentative` für
  `wollte nicht`: die Referenzsammlung kennt für „wouldn''t“ nur unpersönliche Passiv-Absagen, `wollte nicht` ist
  Standarddeutsch und trägt denselben trockenen Ton wie das Englische. Bewusst kurz: der technische Grund steht separat
  unter „Technische Details“. ❌ Nicht `Die Datei ließ sich nicht …` (so lautet schon
  `errors.write.ioError.message.trash`, und macOS als Absender geht dabei verloren).

## Absturzdialog: die drei Eröffnungssätze (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

Der Dialog beim nächsten Start wählt einen von drei Sätzen, je nachdem, was der Bericht festgehalten hat. `.ended`
bleibt unverändert; die beiden anderen dürfen NICHT behaupten, Cmdr sei abgestürzt oder beendet worden, denn genau das
ist nicht passiert.

- **"ran into a problem" → `ist auf ein Problem gestoßen`** · Microsoft-Styleguide de gibt „We've hit a snag.“ als „Wir
  sind leider auf ein Problem gestoßen.“ wieder · high. Cmdr bleibt Subjekt im Nominativ (keine Genitivfalle beim
  Markennamen), und „Fehler“ bleibt draußen (Stilregel). ❌ Nicht „Bei Cmdr ist ein Problem aufgetreten“: derselbe
  Styleguide führt „ist ein Problem aufgetreten“ ausdrücklich als Negativbeispiel; macOS Finder benutzt die Form zwar
  („da ein Problem mit dem Laufwerk aufgetreten ist“, `PE37`), sie macht Cmdr aber zum Objekt eines Nebensatzes und
  klingt nach Amtsdeutsch.
- **"and kept running" → `und weitergelaufen`** · das Weiterlauf-Wortfeld des Katalogs (`Läuft weiter im Hintergrund`,
  `Im Hintergrund weiterlaufen lassen`) · high. Perfekt mit geteiltem Hilfsverb `ist`; beide Verben nehmen `sein`. In
  der Referenzsammlung gibt es KEINEN Beleg für „kept running“ (kein `weiterlaufen`, `läuft weiter`, `fortgesetzt` in
  allen acht Quellen, geprüft 2026-08-23), deshalb entscheidet die Katalogkonsistenz.
- **"a report" (nicht „a crash report“) → `ein Bericht`** · der zweite Satz ist wortgleich mit `.ended`, nur ohne
  `Absturz-`: „Hier ist ein Bericht mit Details, die bei der Behebung helfen können.“ · high. Dieses eine fehlende
  Wortglied trägt die ganze Aussage „nichts ist abgestürzt“.
- **`.unknown` sagt weder das eine noch das andere**: „Cmdr ist beim letzten Mal auf ein Problem gestoßen.“ Kein
  `im Hintergrund`, kein `weitergelaufen`, kein `beendet` — der Satz stimmt, egal ob Cmdr danach lief oder nicht.

### Nachtrag: was hinter `und weitergelaufen` steckt

- **`ist … weitergelaufen` ist ZUSAMMENGESETZT, nicht gefunden.** Keine Quelle irgendwo in der Referenzsammlung liefert
  einen Satz in der Vergangenheit, der sagt, dass eine App einen Fehler überlebt hat: weder macOS noch Microsoft noch
  die orthodoxen (Total Commander, Double Commander) oder die Explorer-Manager (Nautilus, Thunar, Dolphin) haben so eine
  Zeichenkette überhaupt im Bestand. Der Ausdruck entsteht aus dem im Katalog gesetzten Wortstamm (`weiterlaufen`,
  `Läuft weiter im Hintergrund`) plus normalem Perfekt. `high` bleibt die Konfidenz, weil die Morphologie keine
  Übersetzungsentscheidung ist, aber das Protokoll muss sagen, dass hier zusammengesetzt und nicht belegt wurde.
- ❌ Nicht `hat weitergearbeitet`: „arbeiten“ behauptet mehr, als wir wissen (dass die App weiter NÜTZLICH war), während
  wir nur wissen, dass der Prozess noch lief.
- ❌ Nicht `wurde fortgesetzt`: Passiv mit fehlendem Agens, und im Katalog gehört `fortsetzen` zu einem VORGANG, den
  jemand fortsetzt, nicht zur App selbst.
- **Der Streit um `auf ein Problem gestoßen` ist entschieden und soll nicht neu aufgerollt werden.** Microsofts
  deutscher Styleguide führt `ist ein Problem aufgetreten` ausdrücklich als Negativbeispiel; macOS benutzt genau diese
  Form überall; unser `auf ein Problem gestoßen` ist eine dritte Form, die Apple ebenfalls ausliefert und die Cmdr im
  Nominativ hält. Wer künftig mit dem MS-Styleguide in der Hand „korrigieren“ will, findet hier die Antwort.
- **`.ended` bleibt unangetastet**: macOS Problem Reporter gibt `%@ quit unexpectedly.` als
  `%@ wurde unerwartet beendet.` wieder, wortgleich mit unserer ausgelieferten Zeile in der tragenden Hälfte.
  `confirmed`.
- Die Alternative `…, ist aber weitergelaufen` steht als offene Frage in `review-queue.md`.

### Datenschutzhinweis: `auf das Problem gestoßen`, nicht `abgestürzt`

`crashReporter.dialog.privacyNote` ist EINE Zeichenkette für alle drei Fälle des Dialogs, also darf sie kein
Absturz-Wort tragen: „welcher Teil des Codes auf das Problem gestoßen ist“ stimmt auch dann, wenn Cmdr weitergelaufen
ist. Gleiche Wendung wie in den Textkörpern, damit der Dialog aus einer Stimme spricht · high.

### Titel und Bestätigung: `Absturzbericht` nur, wenn es wirklich abgestürzt ist

Der Katalog trennt `Absturzbericht` und schlichten `Bericht` schon konsequent, und genau diese Trennung trägt hier:

- `crashReporter.dialog.title.crash` → `Absturzbericht senden?` (unverändert), `.title.report` → `Bericht senden?`
- `crashReporter.sentToast.message.crash` → `Absturzbericht gesendet. …` (unverändert), `.message.report` →
  `Bericht gesendet. …`

Beides ist dieselbe Operation wie im Englischen: das Absturz-Wortglied entfällt, sonst ändert sich nichts · high. ❌
`crashReporter.dialog.alwaysSend` bleibt `Absturzberichte immer senden`: das Kästchen schaltet die Einstellung
`updates.crashReports`, deren kanonisches Label in Einstellungen > Updates `Absturzberichte senden` heißt. Neutral
formuliert würde es so klingen, als schlösse es auch Fehlerberichte ein, was es nicht tut.

## Der Einstellungstext für Absturzberichte deckt jetzt beide Fälle (`settings.updates.crashReports.description`)

Der Schalter schickt auch dann einen Bericht, wenn ein Panic im Hintergrund die App NICHT beendet hat, also darf der
Hilfetext nicht mehr nur vom unerwarteten Beenden sprechen. Alle Bausteine stammen aus dem Absturzdialog-Abschnitt oben,
nur im Präsens:

- `wenn Cmdr unerwartet beendet wird` aus `crashReporter.dialog.body.ended`, `im Hintergrund auf ein Problem stößt` aus
  `.keptRunning` · high. Die Präsensform ist reine Morphologie, keine neue Termentscheidung.
- `einen Bericht` statt `Absturzberichte`, weil der Satz beide Fälle abdeckt: dieselbe Operation wie bei `.title.report`
  · high. ❌ Das LABEL `settings.updates.crashReports.label` bleibt `Absturzberichte senden` — das ist der kanonische
  Name der Einstellung, genau wie bei `crashReporter.dialog.alwaysSend`.
- Zweiter Satz aus `crashReporter.dialog.privacyNote` (`welcher Teil des Codes auf das Problem gestoßen ist`), hier als
  Akkusativobjekt mit Relativsatz (`den Teil des Codes, der …`), weil der Einstellungstext artikellos aufzählt. Ersetzt
  `Absturzstelle`, das nur für den Absturzfall stimmte · high.

## Auswerfen und Trennen: die neun Absagen (`errors.eject.*`)

Neun kurze Zeilen in einem Toast oben rechts, jede als Satz HINTER einem Doppelpunkt: der Rahmen ist
`fileExplorer.pane.ejectFailedToast` („{volumeName} ließ sich nicht auswerfen: …“) oder `.disconnectFailedToast`
(„Trennen nicht möglich: …“). Rohfamilie (`errors.*`): einfache Apostrophe, kein ICU, kein Markdown. Jeder Wert muss
also für sich als vollständiger Satz lesbar sein und darf den Rahmen nicht wiederholen.

- **`drive` bleibt `Laufwerk`** · der ganze Katalog sagt so (`settings.indexing.*`,
  `onboarding.stepOptional.indexing.*`) · high. `Volume` bleibt dem technischen Sinn vorbehalten (`terms.json`);
  die Eject-Zeilen sprechen von dem Ding, das der Nutzer angesteckt hat, deshalb `Laufwerk`.
- **„removable“ → `Wechselmedium`** · macOS Finder `de` (`NE31` „Das Wechselmedium „^0“ wird gerade verwendet …“,
  `NE80`, `TL_HELP_EJCT` „Wechselmedien auswerfen und von Servern trennen“) und MS-Terminologie (`removable media` →
  `Wechselmedien`, Term-ID 177320, AUT/DEU/CHE/LUX) · high. `notEjectable` wird damit zur Aussage über die Bauart des
  Laufwerks, nicht über einen Misserfolg: „Dieses Laufwerk ist kein Wechselmedium, es bleibt also verbunden.“
- **„in use“ → `in Verwendung`** · macOS Finder `de` (`NE66` „The volume can’t be ejected because it’s currently in
  use.“ → „Das Volume ist gerade in Verwendung und kann nicht ausgeworfen werden.“) · high. ❌ Nicht `belegt` oder
  `gesperrt`: `gesperrt`/`geschützt` gehört im Katalog schon zu `locked`.
- **„Close any open files and apps“ → `Schließe offene Dateien und beende laufende Apps`** · macOS `NE52` („Quit any
  open applications, and then try again.“ → „Beende die Programme und versuche es erneut.“) liefert das Verb `beenden`
  für Programme; das Substantiv nimmt Cmdrs eigenes `App` (`errors.listing.resourceBusy.explanation` „eine andere App
  oder ein anderer Prozess“), nicht Apples `Programm` · high. Dateien werden geschlossen, Apps beendet: ein gemeinsames
  Verb für beides gibt es im Deutschen nicht.
- **„idle“ → `nicht mehr beschäftigt`** · der Katalog setzt beides schon: `indexing.enrich.pausedIdle` („Waiting until
  you're idle“ → „Wartet, bis du nicht mehr beschäftigt bist“) und `fileExplorer.mtp.deviceBusy` („Device is busy.“ →
  „Gerät ist beschäftigt.“) · high. Damit trägt `mtpDisconnectRefused` dasselbe Wortfeld wie die MTP-Meldung, die der
  Nutzer vorher gesehen hat.
- **„unplug“ → `abziehen`** · `errors.listing.deviceReconnecting.suggestion` („There's nothing to unplug.“ → „Du musst
  nichts abziehen.“) und `errors.provider.macDroid.*` („Zieh das USB-Kabel ab …“) · high.
- **„wouldn't“ (eine Absage der Gegenseite) → `wollte nicht`** · dieselbe Entscheidung wie bei
  `errors.mutation.trashRefused` („macOS wollte das nicht in den Papierkorb bewegen.“) · high für die Konsistenz,
  `tentative` für den Baustein selbst: die Referenzsammlung kennt für „wouldn't“ nur unpersönliche Passiv-Absagen. Wert:
  „Das Gerät wollte seine Verbindung nicht schließen.“
- **`disconnect` bleibt `trennen`, auch für eine Freigabe** · macOS `TL_HELP_EJCT` („Eject disks and unmount servers“ →
  „Wechselmedien auswerfen und von Servern trennen“) · high. ❌ Nicht `aushängen` (GNOME Nautilus `de`: „Unmount“ →
  „Aushängen“), das ist das Linux-Idiom und steht in keinem macOS-String.
- **`busy` nimmt das gesetzte `Vorgang`** · „Auf diesem Laufwerk läuft noch ein Vorgang von Cmdr. Wirf es aus, sobald er
  fertig ist.“ · Satzrahmen von `main.quit.title` („Ein Vorgang läuft noch. Trotzdem beenden?“), `Vorgang` ist im
  Katalog der Name für Kopieren/Bewegen/Löschen (`Vorgangswarteschlange`, `Vorgangsprotokoll`) · high. ❌ Nicht wörtlich
  „Cmdr bewegt noch Dateien“: `bewegen` ist in Cmdr der NAME des Move-Befehls, und die Meldung erscheint genauso beim
  Kopieren und Löschen. Der Markenname bleibt im Satz (Quellstring nennt Cmdr, `desktop-i18n-dont-translate` prüft das).
- **`timedOut` ist keine Absage, sondern eine offene Frage** → „Das Laufwerk hat noch nicht geantwortet, das Auswerfen
  klappt also womöglich trotzdem noch.“ · wortgleicher Bau wie das schon ausgelieferte Geschwister
  `errors.mutation.timedOut` („Das Volume hat noch nicht geantwortet, die Änderung klappt also womöglich trotzdem
  noch.“) · high. ❌ Kein reflexives „wirft sich selbst aus“, ❌ nichts mit `Fehler` oder `abgebrochen`.
- **`unexpected` ist wortgleich mit `errors.mutation.unexpected`** — gleicher englischer Quellsatz, gleiche deutsche
  Zeile: „Etwas ist schiefgelaufen, und Cmdr konnte nicht erkennen, was genau.“ · high.
- **`mtpIdMissingDevicePrefix` →
  `Cmdr konnte nicht erkennen, welches Gerät das ist, und kann es deshalb nicht trennen.`** · `erkennen` ist die
  Katalogstimme für „Cmdr couldn't tell“ (`errors.mutation.unexpected`), `Gerät` ist gesetzt (`terms.json`) · high.
- **Die Wiederholung im Trenn-Rahmen ist gewollt**: „Trennen nicht möglich: Das ist keine Netzwerkfreigabe, es gibt also
  nichts zu trennen.“ Der Wert muss allein stehen können (er landet auch im Auswerf-Rahmen), deshalb bleibt das zweite
  `trennen` drin.

## Papierkorb-Toast: Widerrufen und Zurücklegen (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Neue Oberfläche: nachdem Cmdr Objekte in den Papierkorb bewegt hat, erscheint ein Toast mit zwei Tasten („Widerrufen“,
„Zum Papierkorb gehen“); dazu kommt derselbe Befehl in der Befehlspalette.

- **`undo` (Taste) → `Widerrufen`** · macOS AppKit MenuCommands („Undo Smart Dash“ → „Intelligenten Bindestrich
  widerrufen“) und der Katalog selbst (`askCmdr.renameUndo.undo` = „Widerrufen“) · high. ❌ Nicht das Nautilus-Wort
  „Rückgängig“ (GNOME `de`, Tier 3): macOS gewinnt, und der Katalog hat sich schon festgelegt.
- **`put back` (Objekte aus dem Papierkorb an ihren alten Ort) → `zurücklegen`** · macOS Finder `N153.1` („Put Back“ →
  „Zurücklegen“, `LocalizableMerged`) · high. Nicht mit `zurücksetzen` verwechseln: das ist im Katalog das
  Umbenennen-Widerrufen (`askCmdr.renameUndo.undone` = „… zurückgesetzt.“), also NAMEN zurück, nicht ORTE. Nautilus'
  „aus dem Papierkorb wiederherstellen“ ist korrektes Deutsch, aber Tier 3 und deutlich länger.
- **`This drive doesn't keep a trash.` → `Dieses Laufwerk hat keinen Papierkorb.`** · reine Tatsachenaussage im Ton von
  `fileOperations.delete.noTrashWarningStrong` („Dieses Volume unterstützt keinen Papierkorb.“); die englische Quelle
  sagt hier `drive`, also `Laufwerk`, nicht `Volume` · high.
- **`Nothing to put back.` → `Es gibt nichts zurückzulegen.`** · gleicher Satzbau wie das schon ausgelieferte
  `askCmdr.renameUndo.unavailable` („Es gibt nichts zurückzusetzen. … oder ihr Laufwerk ist nicht verbunden.“), nur mit
  dem Ortsverb · high.
- **Die zweite Hälfte hat ihren eigenen Zähl-Parameter `{skipped}`**, trägt also ein finites Verb: „… zurückgelegt;
  {skippedText} {skipped, plural, one {Objekt blieb} other {Objekte blieben}} im Papierkorb.“ Gezählt wird `Objekt`, das
  Katalogwort für das `item`, das die Quelle in dieser Hälfte sagt, nicht `Datei` wie in der ersten · high.
- **Toast-Taste und Befehlsname sind identisch** („Zum Papierkorb gehen“), passend zu den Geschwistern
  `commands.navParent.label` („Zum übergeordneten Ordner gehen“) und `commands.downloadsGoToLatest.label` („Zum neuesten
  Download gehen“).

## Fehlerbericht nachträglich ergänzen: der Notiz-Dialog (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

Neue Oberfläche: Cmdr hat den Bericht selbst gesendet (Auto-Senden), der Toast „Fehlerbericht gesendet“ trägt jetzt eine
Taste, die einen Dialog öffnet. Dort sieht man, was gesendet wurde, und kann eine Notiz nachreichen, die an DENSELBEN
Bericht angehängt wird. Es geht kein zweiter Bericht raus, und genau das muss die Sprache tragen.

- **`Bericht` statt `Fehlerbericht` innerhalb des Dialogs und der Toasts** · dieselbe Trennung, die der Absturzdialog
  schon fährt (§ „Titel und Bestätigung“: `Absturzbericht` nur, wenn es wirklich abgestürzt ist) · high. Der DIALOGTITEL
  nennt die Sache einmal voll („Zu deinem Fehlerbericht hinzufügen“), damit klar ist, worum es geht; danach reicht
  `Bericht`, und die Tasten bleiben schmal.
- **`Add to your error report` → `Zu deinem Fehlerbericht hinzufügen`**, **`Add to report` → `Zum Bericht hinzufügen`**
  · macOS Finder `de` („Zum Dock hinzufügen“, „Zur Seitenleiste hinzufügen“) · high.
- **`Adding…` → `Wird hinzugefügt …`** · die Katalogkonvention für laufende Vorgänge, wortgleich gebaut wie das
  Geschwister `errorReporter.dialog.sending` („Wird gesendet …“), Leerzeichen vor den Auslassungspunkten · high.
- **`What was sent` → `Was gesendet wurde`** · exakte Vergangenheitsform des Geschwisters
  `errorReporter.dialog.detailsToggle` („Was gleich gesendet wird“) · high. Die beiden Klapp-Panels stehen im selben
  Dialoggerüst, also müssen sie sich auch im Deutschen als Zeitform-Paar lesen.
- **`Your note` → `Deine Notiz`** · `du`-Anrede wie im ganzen Katalog; ohne „(optional)“, weil hier anders als beim
  Sendedialog eine Notiz oder eine E-Mail-Adresse nötig ist, bevor die Taste greift · high.
- **`…and it'll join what the team already has` → `… und es kommt zu dem hinzu, was das Team schon hat`** · konstruiert,
  `high` für die Bausteine (`hinzukommen zu` ist Standarddeutsch, `das Team` steht schon in
  `errorReporter.dialog.description`), `tentative` für die Satzform: die Referenzsammlung kennt keinen Beleg für „einen
  bereits gesendeten Bericht ergänzen“. Die Formulierung hält bewusst den `hinzu…`-Wortstamm des Dialogs durch, damit
  Beschreibung, Taste und Toast eine Familie bilden.
- **`That report can't take a note any more.` → `Zu diesem Bericht lässt sich keine Notiz mehr hinzufügen.`** · das
  Katalogmuster für Absagen („X ließ sich nicht …“, siehe `terms.json`) im Präsens · high. Kein „Fehler“, kein
  „fehlgeschlagen“ — die Stilregel gilt, und das Wort `Fehlerbericht` taucht in diesem Satz bewusst nicht auf.
- **Das Help-Menü heißt im Fließtext `das Menü „Hilfe“`** · macOS Finder `de` benennt Menüs genau so („Wähle es aus,
  wähle im Menü „Ablage“ die Option „Informationen“ …“, `BN43`), und `menu.bar.help` ist bereits `Hilfe` · high. Der
  Zielbefehl dahinter ist `menu.help.sendErrorReport` („Fehlerbericht senden…“).
- **`View or add notes to the report` → `Bericht ansehen oder Notiz hinzufügen`** · beide Hälften bleiben stehen, weil
  die Taste beides kann; `ansehen` aus dem Katalog, `hinzufügen` aus dem Finder · high. Verworfen:
  `Bericht ansehen oder ergänzen` (kürzer, aber `ergänzen` hat in der ganzen Referenzsammlung keinen Beleg und lässt
  offen, WAS man ergänzt).
- **`Note added to your report.` → `Notiz zum Bericht hinzugefügt.`** · Satzbau wortgleich zum Geschwister
  `errorReporter.sentToast.message` („Fehlerbericht gesendet. Deine Referenz-ID ist“), damit die beiden Toasts als Paar
  lesbar sind · high. Das Possessiv fällt weg: zwei `dein` hintereinander („zu deinem Bericht. Deine Referenz-ID“) liest
  sich holprig, und das deutsche UI setzt das Possessiv ohnehin sparsamer als das Englische.

## Der Auswahldialog: Dateien auswählen und abwählen (`selection.*`)

- **select (Dateien über ein Muster) → `auswählen`; deselect → `abwählen`** · macOS Finder `de`, `MenuBar.json`
  `172.title` = „Alles auswählen“ (Tier 1, gegengeprüft am laufenden System, macOS 26.6.2, Build 25G83, 2026-08-29).
  Finders Gegenstück zu „Deselect All“ (`MenuBar.json` `300488.title`) ist „Auswahl aufheben“, ein Ausdruck ohne
  Objektstelle, mit dem sich „Dateien …“ nicht bilden lässt. Das transitive Verb kommt aus Microsoft Terminology
  (`deselect` → „Abwählen“, Term-ID `2612123`, AUT/DEU/CHE/LUX) und aus Double Commander `de`, dem orthodoxen
  Zwei-Fenster-Manager in Cmdrs Linie („Unselect a Gro&up...“ → „Gru&ppe abwählen …“, „&Unselect All“ → „Alle
  a&bwählen“, „Unselect all files with same name“ → „Alle Dateien mit gleichem Namen abwählen“) · `high`.
- **„Auswahl aufheben“ und „abwählen“ stehen nebeneinander, und das ist Absicht.** `menu.select.deselectAll` bleibt
  Finders „Auswahl aufheben“ (die ganze Auswahl fällt weg), während `menu.select.deselectFiles`,
  `commands.selectionDeselectFiles.label` und `selection.dialog.title.remove` „Dateien abwählen“ sagen (ein Objekt fällt
  weg). Wer die drei „vereinheitlicht“, bricht den Dialogtitel gegen den Menüpunkt, der ihn öffnet: genau der Fehler,
  den dieser Katalogbereich behebt.
- **recent selections → `Letzte Auswahlen`** · gespiegelt an den Geschwistern in `queryUi.recent.*` („Letzte Suchen“),
  gleiche Grammatik, nur `Suchen` → `Auswahlen`. Plural von `Auswahl` ist `Auswahlen` · `high`.
- **`selection.recent.applyAria` folgt `search.recent.runAria`** · dort steht „Letzte {mode}-Suche ausführen: {query}“,
  hier „Letzte {mode}-Auswahl anwenden: {query}“. `apply` → `anwenden` aus macOS AppKit (`NSFontOptionsPanel`
  `100411.title` und `NSPreferences` `7TY-1Z-cs2.title` = „Anwenden“) · `high`. Die `{mode}-`-Komposition ist vom
  Geschwister übernommen; `{query}` steht am Satzende hinter dem Doppelpunkt, damit beliebiger Nutzertext passt.
- **Ein knapper Tastenhinweis nennt die Taste `Enter`** · `search.runHint` sagt bereits „Zum Suchen Enter drücken“, also
  „Zum Filtern Enter drücken“ · `high`. Im Fließtext heißt sie weiter `die Eingabetaste` („Drücke die Eingabetaste …“,
  `search.coverage.pressEnter`), nie „Return“ (`terms.json` `enter-key`).
- **Der Tooltip ist ein eigener Satz und nimmt die natürliche Wortstellung.** Er muss die Beschriftung der Taste NICHT
  wiederholen: `QueryDialog.svelte` baut den zugänglichen Namen der Taste aus
  `config.primaryAction.ariaLabel ?? config.primaryAction.label`, also aus dem Label-Schlüssel, während der Tooltip an
  einem inneren `span` per `use:tooltip` hängt. WCAG 2.5.3 ist damit schon durch den Aufbau erfüllt, und der Katalog
  lässt die beiden anderswo bewusst auseinanderlaufen (`search.action.showAll.label` gegenüber seinem `.tooltip`). Also
  verbfinal, wie im Deutschen üblich: „Diese Dateien im fokussierten Bereich auswählen“ / „… abwählen“ · `high`.
- **`fokussierter Bereich` → `im fokussierten Bereich`** · aus `commands.navGoToPath.description` („Den fokussierten
  Bereich zu einem … Pfad springen lassen.“) und `commands.favoritesAdd.description` · `high`.
- **`selection.notice.snapshotPane` → „Abgeglichen wird, was in der Liste steht (der vollständige Pfad).“** ·
  beruhigend, keine Warnung, wie das `@key` verlangt; `der vollständige Pfad` ist die Katalogform (siehe
  `errors.listing.nameTooLongErrno.explanation`) · `high`.

## Der zugängliche Name muss die sichtbare Beschriftung enthalten (WCAG 2.5.3, `fileOperations.transferProgress.pause`, `queue.row.pause`, `queryUi.scope.toggle.caseSensitiveAria`)

`desktop-i18n-aria-label` verlangt, dass ein `*Aria`-Wert seine sichtbare Beschriftung wörtlich enthält (Groß-/
Kleinschreibung, Satzzeichen und Leerraum werden ignoriert, `/` aber NICHT). Wer per Sprachsteuerung „Anhalten“ sagt,
muss damit genau die Taste treffen, auf der „Anhalten“ steht. Der richtige Weg dahin ist, der BESCHRIFTUNG die Form zu
geben, die der natürliche Aria-Satz ohnehin benutzt, und den Aria-Satz nicht zu verbiegen.

- **pause (Taste) → `Anhalten`; resume → `Fortsetzen`; paused → `Angehalten`** · macOS Podcasts
  (`PLAY_BUTTON_PAUSE`/`AX_PAUSE` = „Anhalten“, `EPISODE_ACTION_RESUME` = „Fortsetzen“, geprüft auf macOS 26.6.2, Build
  25G83, 2026-08-30); macOS Finder (`NE101`/`PE108.1` = „Fortsetzen“) · `high`. ❌ Nicht das Lehnwort „Pause“ auf der
  Taste: der restliche Katalog sagt durchgehend „anhalten“ (`queue.toolbar.pauseAll` = „Alle anhalten“,
  `queue.row.status` = „Angehalten“), und die beiden Aria-Sätze („Diese Übertragung anhalten“, „Diesen Vorgang
  anhalten“) verlangen genau dieses Wort.
- **case-sensitive → `Groß-/Kleinschreibung beachten`** · macOS zeigt die Verneinung „Groß-/Kleinschreibung ignorieren“,
  also ist „beachten“ die positive Form · `high`. Der zugängliche Name wickelt die Beschriftung ein, statt sie zu
  ersetzen: `queryUi.scope.toggle.caseSensitiveAria` = „Beim Abgleich Groß-/Kleinschreibung beachten“. Ein
  umformuliertes „Übereinstimmung mit …“ bricht die Enthaltensregel.

## Ein englisches Wort, ein deutsches Wort: die Drift-Prüfung (`commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel`, `menu.bar.file`, `menu.bar.view`, `menu.window.zoom`, `menu.view.zoom`, `menu.tag.purple`, `settings.tint.purple`, `queue.row.dismiss`, `queryUi.bar.runLabel`, `ai.cloud.checking`, `licensing.dialog.checking`, `updates.status.checking`)

Der Katalog trug 20 Stellen, an denen `de` denselben englischen Text zweimal verschieden benannte, meist weil ein
späterer Durchgang nur die Menüleiste anfasste und die Befehlspalette auf der alten Formulierung stehen ließ. Zehn davon
waren echte Drift und sind unten aufgelöst; die anderen zehn sind ABSICHTLICHE Grenzen und stehen darunter, damit der
nächste Durchgang sie nicht „vereinheitlicht“.

### Aufgelöst

- **`View` (die F3-Aktion, Datei im eingebauten Betrachter öffnen) → `Ansehen`**, überall: `commands.fileView.label`,
  `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel` · der Katalog hatte sich längst festgelegt, ohne dass die
  Tasten es mitbekamen: `settings.appearance.showFunctionKeyBar.description` zählt die Tasten der F-Tasten-Reihe als
  „(Umbenennen, **Ansehen**, Kopieren usw.)“ auf, während die Taste selbst „Anzeigen“ hieß. Dazu
  `askCmdr.wakeToast.action` und `suggestedOps.review` (= „Ansehen“ / „Dateien ansehen“) und der Termbase-Eintrag view/see
  → ansehen · `high`. ❌ Nicht `Anzeigen`: das ist im Katalog der SHOW-Sinn (etwas sichtbar machen), und die beiden
  auseinanderzuhalten ist der ganze Zweck des Eintrags. ❌ Nicht `Vorschau`: das ist das Substantiv für das
  Betrachter-Fenster, kein Imperativ, und `menu.file.view` verlangt laut `@key` ausdrücklich ein Verb. Double Commander
  `de` sagt für dieselbe Aktion `Betrachten` (`tfrmmain.actview.caption`) — richtig, aber Tier 3, und der Katalog hat
  schon ein Wort.
- **`Show in Finder` → `Im Finder anzeigen`**, auch im Menü (`menu.file.showInFinder`, vorher „Im Finder zeigen“) · der
  Termbase-Eintrag trennt die beiden englischen Quellverben schon: `Reveal in Finder` → „Im Finder zeigen“,
  `Show in Finder` → „Im Finder anzeigen“ (beides in `de/macOS/` belegt). Das Menü hatte die Reveal-Form für die
  Show-Quelle übernommen, sodass Menüleiste und Befehlspalette denselben Befehl verschieden nannten · `high`.
- **`Go to …` bleibt `Zum … gehen`, auch auf Tasten und in Einstellungen** ·
  `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.label` sagte „springen“,
  `fileExplorer.errorPane.goHome` sagte „öffnen“. Beide heißen jetzt wie ihr Befehl in der Palette („Zum neuesten
  Download gehen“, „Zum persönlichen Ordner gehen“), passend zu `commands.navParent.label` · `high`.
- **`hidden files` bleibt `verborgene Dateien`, und `show hidden files` → `Verborgene Dateien einblenden`** ·
  `settings.listing.showHiddenFiles.label`/`.description` sagten als Einzige „versteckt“ und „anzeigen“, gegen
  `menu.view.showHiddenFiles` und `commands.viewShowHidden.label` („ein-/ausblenden“) · `high`. Die Dateiverwaltungen
  (Double Commander, Nautilus, Dolphin) sagen mehrheitlich „versteckte Dateien anzeigen“, aber das ist Tier 3 gegen eine
  bereits getroffene Katalogentscheidung.
- **`Search` als Aufgabenname ist das Substantiv `Suche`** · `onboarding.stepAi.table.rowSearch` ist laut `@key` ein
  „short noun“ und sagte „Suchen“ · `high`.
- **In-Arbeit-Zustände nehmen die `Wird …`-Passivform**: `fileExplorer.network.browser.searching` = „Wird gesucht …“
  (vorher „Suche läuft …“), gleich wie `askCmdr.sessions.searching`, `queryUi.results.searching`,
  `viewer.search.searching` · `high`. Ausnahme mit Grund: `Retrying` bleibt die kürzere Nominalform „Erneuter Versuch
  …“, weil `fileExplorer.navigation.spaceRetryingText` an die Stelle einer Zahl im schmalen Speicherplatz-Balken tritt;
  `fileExplorer.unreachable.retrying` gab dafür sein „läuft“ auf.
- **`Got it` → `Alles klar`** · macOS `de` übersetzt „Got It“ genau so (geprüft auf macOS 26.6.2, Build 25G83,
  2026-08-30) · `high`. `ai.toast.gotIt` sagte „Verstanden“.
- **`From:` vor einem Pfad → `Von:`** · `fileOperations.scanPhase.fromLabel` sagte „Aus:“, während das Gegenstück
  `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle` schon das Paar „Von“/„Nach“ führt · `high`.
- **`paused` (Zustand) → `angehalten`, auch im Satz** · der entfernte Einstellungsstatus
  settings.askCmdr.status.needsReview sagte „Ask Cmdr ist pausiert“, `queue.row.status` sagt „Angehalten“ · `high`.
- **Die beiden Beta-E-Mail-Paare sagen wieder dasselbe** · `onboarding.stepBeta.signup.success` und
  `settings.updates.emailConfirmHint` teilen sich einen englischen Satz, ebenso `onboarding.stepBeta.signup.failure` und
  `settings.updates.emailSignupError`. Gewählt: „Sieh in deinem Postfach nach, …“ (idiomatischer als „Schau in dein
  Postfach“) und „Tut uns leid, die Anmeldung hat gerade nicht geklappt. Erneut versuchen?“ — natives Bedauern statt des
  Lehnworts „Sorry“, die schuldfreie Formulierung („hat nicht geklappt“ statt „wir konnten dich nicht anmelden“), und
  das in der Termbase festgelegte „Erneut versuchen“ · `high`.

### Absichtliche Grenzen (bitte nicht vereinheitlichen)

- **`File`: `Ablage` ist die Menüleiste, `Datei` ist alles andere** · macOS Finder nennt sein File-Menü „Ablage“, und
  `@menu.bar.file` verlangt ausdrücklich Finders eigenes Wort. Eine Tabellenspalte (`suggestedOps.columnFile`) heißt
  natürlich „Datei“ · `high`.
- **`View`: `Darstellung` ist der MENÜTITEL, `Ansehen` ist die Aktion** · macOS Finder nennt sein View-Menü
  „Darstellung“ (Substantiv, es hält die Ansichtsoptionen), `@menu.bar.view` verlangt genau dieses Wort. Siehe oben ·
  `high`.
- **`Zoom`: `Zoom` ist die Textgröße, `Zoomen` ist das Fenster** · `menu.window.zoom` ist macOS' Fenster-Menüpunkt
  (grüner Knopf), den macOS `de` „Zoomen“ nennt; `menu.view.zoom` ist das Untermenü für Cmdrs Textgröße. Das `@key` sagt
  selbst, dass hier macOS' Wort gilt, auch wenn es vom Textzoom abweicht · `high`.
- **`Purple`: `Lila` ist die Finder-Tag-Farbe, `Violett` ist die Farbpalette** · `@menu.tag.purple` verlangt den exakten
  macOS-Namen dieser Tag-Farbe, und macOS `de` sagt „Lila“; `settings.tint.purple` ist Cmdrs eigene Volume-Einfärbung
  und folgt dem Termbase-Farbnamen „Violett“ · `high`.
- **`Checking`: `Wird geprüft` prüft etwas nach, `Wird gesucht` sucht nach Updates** · macOS `de` übersetzt „Checking
  for updates…“ als „Nach Updates suchen …“ (Software Update, geprüft auf macOS 26.6.2, 2026-08-30), also ist Suchen
  hier das Idiom. `ai.cloud.checking` und `licensing.dialog.checking` prüfen dagegen eine Verbindung bzw. einen
  Schlüssel · `high`.
- **`Dismiss`: `Schließen` schließt ein Fenster, `Ausblenden` blendet eine Zeile aus** · alle acht Dialog-/Toast-Tasten
  schließen etwas. `queue.row.dismiss` entfernt laut `@key` nur die Zeile aus der Liste: nichts wird widerrufen,
  wiederholt oder gelöscht. Eine Zeile kann man nicht „schließen“, und „Ausblenden“ sagt genau, was passiert · `high`.
- **`Indexing now`: `Wird gerade indiziert` meint DIESE Datei, `Indizierung läuft` meint den Durchlauf** ·
  `fileExplorer.imageIndex.file.indexing` ist der Tooltip auf dem Abzeichen EINES Bildes, also ein Passivzustand;
  `settings.mediaIndex.progressSummary.title` ist die Überschrift über der Fortschrittsliste, also nominal · `high`.
- **`Search`: `Suche` ist das Substantiv, `Suchen` ist die Taste** · `queryUi.bar.runLabel` ist der Auslöser neben dem
  Eingabefeld und braucht den Imperativ; Dialogtitel, Tab-Name, Einstellungsabschnitt und Aufgabenname sind Substantive
  · `high`.
- **`Put back …`: `zurückgesetzt` sind NAMEN, `zurückgelegt` sind ORTE** · schon im Papierkorb-Abschnitt (2026-08-27)
  entschieden: macOS Finder `N153.1` gibt „Put Back“ als „Zurücklegen“, und der Katalog benutzt „zurücksetzen“ für das
  Umbenennen-Widerrufen. Englisch teilt sich einen Satz für zwei verschiedene Rücknahmen · `high`.
- **`you@example.com` → „du@example.com“, in allen drei Feldern gleich** · `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder` und `onboarding.stepBeta.emailPlaceholder` tragen dieselbe Adresse; ihre
  `@key`-Beschreibungen schreiben das vor · `high`. Der lokale Teil wird übersetzt („du@“), die Domain bleibt
  `example.com`. ❌ Kein „du@beispiel.de“: `beispiel.de` ist eine echte registrierbare Domain, `example.com` ist per RFC
  2606 für Beispiele reserviert.

## Wörter, die auseinandergelaufen sind, ohne dass ein Check es sehen konnte (`settings.fileOperations.allowFileExtensionChanges.*`, `askCmdr.wake.stop`, `settings.mediaIndex.clip.comingSoon`, `settings.whatsNew.lastSeenVersion.label`)

`i18n-terms` sieht nur Schlüsselpaare mit IDENTISCHEM Englisch. Diese hier haben leicht unterschiedliche Quelltexte und
sind deshalb nur beim Handdurchgang aufgefallen. Alle sind behoben.

- **`allow` → `erlauben`, auch in den Einstellungen** · `settings.fileOperations.allowFileExtensionChanges.label`/
  `.opt.yes`/`.opt.no` sagten „zulassen“ / „Immer zulassen“ / „Nie zulassen“, während der Dialog, der genau diese
  Einstellung schreibt (`fileExplorer.extensionChange.alwaysAllow`), „erlauben“ sagt — ebenso
  `askCmdr.renameReview.allow`, `onboarding.stepFda.ifAllow` und `notifications.permissionDenied` · `high`.
- **`Stop` → `stoppen`, nie `anhalten`** · `askCmdr.wake.stop` sagte „Anhalten, was Ask Cmdr gerade tut“, und „anhalten“
  ist im Katalog das Wort für PAUSE (`queue.row.pause` = „Anhalten“, `queue.row.status` = „Angehalten“). In einer
  Sitzung, in der man eine Übertragung wirklich anhalten kann, ist das kein Stilproblem, sondern eine falsche Zusage ·
  `high`.
- **`Coming soon` → `Bald verfügbar`** · `settings.mediaIndex.clip.comingSoon` sagte als Einziges „Demnächst verfügbar“
  · `high`.
- **`changelog` → `Änderungsprotokoll`, auch im Kompositum** · `settings.whatsNew.lastSeenVersion.label` sagte
  „Changelog-Version“; jetzt „Zuletzt gesehene Version des Änderungsprotokolls“ · `high`.

Nicht angefasst, weil bereits als Grenze dokumentiert und korrekt angewendet: `Recent` → „Zuletzt verwendet“ nur als
Gruppentitel der Befehlspalette (sonst „Letzte“), und `notification` → „Hinweis“, wenn es Cmdrs eigener Toast ist,
gegenüber „Benachrichtigung“ für die Einstellungskategorie.

## Die Bereichsnamen kommen jetzt vom Mac des Nutzers (`errors.git.*`, `errors.provider.*`)

Acht Werte trugen die macOS-Bereichsnamen noch als Text, in den beiden `errors.git.*`-Werten sogar englisch („System
Settings > Privacy & Security > Files and Folders“). Sie tragen jetzt die Platzhalter `{system_settings}`,
`{privacy_and_security}` und `{files_and_folders}`, die die App zur Laufzeit durch die Namen ersetzt, die der Mac des
Nutzers zeigt. Die Werte sind RAW (kein ICU), also keine verdoppelten Apostrophe.

- **Eine Präposition darf davor stehen, ein Artikel nicht** · „in den Systemeinstellungen“ geht nicht mehr: der Artikel
  müsste sich an einen unbekannten Wert anpassen. `errors.provider.iCloud.serious` und `.transient` beginnen die Zeile
  jetzt mit „Öffne {system_settings} und …“; die Pfadzeilen behalten „unter **{system_settings} > … > …**“, weil dort
  kein Artikel steht · `high`.
- **`Apple Account` bleibt englisch und ohne Bindestrich** · macOS 26 `de` nennt den Bereich „Apple Account“
  (`AppleIDSettings.appex`, `InfoPlist.loctable`, `CFBundleDisplayName`) und schreibt auch im Fließtext „mit deinem
  Apple Account“ (`Localizable.loctable`, `SPYGLASS_DESCRIPTION_SIGN_IN_REBRAND`; geprüft auf macOS 26.6.2, Build 25G83,
  2026-08-30) · `high`. ❌ Nicht „Apple-Account“, auch wenn die deutsche Kompositumsregel es nahelegt.
- **`General` → „Allgemein“, `Login Items & Extensions` → „Anmeldeobjekte & Erweiterungen“** · beide deckt kein
  Platzhalter ab, sie sind also gewöhnlicher Text; macOS 26 `de` (`LoginItems.appex`, `Localizable.loctable`) · `high`.
  Passt zu `errors.listing.diskFullErrno.suggestion`, das „**{system_settings} > Allgemein > Speicher**“ schon so
  schreibt.

## „Zurücksetzen“ nennt jetzt das Objekt: den alten Namen (`askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`, `menu.app.hideOthers`)

Englisch teilte sich einen Satz mit dem Papierkorb-Widerruf („Put back {countText} {files}.“) und sagt jetzt, WAS
zurückkommt: der alte Name.

- **`Put the old names back on N files.` → „Die alten Namen von N Dateien zurückgesetzt.“** · `zurücksetzen` ist im
  Katalog das Verb fürs Umbenennen-Widerrufen (`askCmdr.renameUndo.undoing` = „Alte Namen werden zurückgesetzt…“,
  `askCmdr.renameUndo.skipReason.failed.named` = „… den alten Namen nicht zurücksetzen“); `zurücklegen` bleibt dem
  Papierkorb vorbehalten (`fileOperations.trash.undone`) · `high`. Der Satz behält den Partizip-Schluss der Geschwister
  (`askCmdr.renameUndo.applied` = „… umbenannt.“).
- **`menu.app.showAll` / `menu.app.hideOthers` waren schon richtig** · „Alle einblenden“ und „Andere ausblenden“ sind
  wörtlich das, was macOS 26 `de` im Programm-Menü zeigt (Finder `MenuBar.strings`, `300730.title` / `300729.title`,
  geprüft auf macOS 26.6.2, Build 25G83, 2026-08-30), und sie stimmen mit `commands.appShowAll.label` /
  `commands.appHideOthers.label` überein · `high`.

## Ein halb zurückgenommener Vorgang: das Rollback zu Ende bringen (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Rollback abschließen`** · bleibt im Substantiv-Wortfeld, das der Katalog für dieses Feature
  schon führt (`operationLog.dialog.rollBack` = „Rollback“, `rollingBack` = „Rollback läuft“) · `high`. `abschließen`
  sagt „zu Ende bringen“, nie „neu starten“; `fortsetzen` wäre die Alternative, ist aber schon an `queue.row.resume`
  vergeben. Der Wert steht identisch in `operationLog.dialog.finishRollBack` und
  `fileOperations.rollbackConfirm.finishRollBack` (gleicher englischer String, sonst schlägt `i18n-terms` an).
- **Die Taste `Rollback abschließen` und das Abzeichen `Rollback abgeschlossen` (`rolledBack`) sind Absicht.** Sie
  stehen nie in derselben Zeile: die Zeile trägt entweder das Abzeichen „Teilweises Rollback“ plus die Taste oder das
  Abzeichen „Rollback abgeschlossen“ ohne Taste. Nach dem Drücken liest sich der Wechsel als Fortschritt.
- **`Finish rolling this back?` → `Rollback abschließen?`** · gleiche Frageform wie das Geschwister
  `fileOperations.rollbackConfirm.title` („Diesen Vorgang rückgängig machen?“), also blanker Infinitiv plus Fragezeichen
  · `high`. Ohne Artikel, damit die Frage so knapp bleibt wie das Geschwister.
- **`Rollback` ist Neutrum** (`das Rollback`, `es`, `Teilweises Rollback`) · Duden führt beide Genera; der Katalog
  sagte schon „das Rollback nicht starten“ (`refusalUnexpected`) und „Du siehst es in der Warteschlange“, also folgt
  `partiallyRolledBack` dem Neutrum · `high`.
- **Der Hinweis unter der Zeile echot `fileOperations.rollbackConfirm.bodyUndoByDeleting`** · „Cmdr hat rückgängig
  gemacht, was möglich war, und den Rest so gelassen, wie er war. Beim Abschließen geht Cmdr alles noch einmal durch und
  überspringt alles, bei dem es sich immer noch nicht sicher ist.“ Das Verb `rückgängig machen` kommt aus
  `rollbackConfirm.title`, `überspringt alles, bei dem es sich nicht sicher ist` wörtlich aus `bodyUndoByDeleting`, und
  `so gelassen, wie er war` hält den Ton von `leaveAsIs` („So lassen“) · `high`. Der Satz verspricht bewusst keine
  vollständige Rücknahme.
- **`in {folder}` → `in „{folder}“`** · macOS `de` schreibt genau so, wenn ein Ordner- oder Objektname in einen Satz
  fällt (`in „^0“`, 11 Treffer in Finder/AppKit; GNOME Nautilus schreibt „im Ordner »%s«“) · `high`. Die
  Anführungszeichen sind hier funktional: sie machen aus dem Namen ein Zitat, damit „Angelegtes wird gelöscht in
  „Backup““ nicht so klingt, als ginge der Ordner selbst weg. `im Ordner „{folder}“` wäre noch deutlicher, kostet aber
  Breite in einer Zeile, die sich das Label schon mit dem Fortschritt teilt. Kein Artikel, kein Kasus am Namen, also
  passt jeder Name.

## Der Toast nach einem abgebrochenen Vorgang: was die Rücknahme geschafft hat (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

Neue Oberfläche: Der Nutzer hat ein Kopieren oder Bewegen mit `Rollback` abgebrochen, die Rücknahme ist durch, und ein
Toast berichtet. Er besteht aus bis zu drei Teilen: einer Überschrift, der Erwartungszeile `leftBehind` und einer
Aufzählung von `reason.*`-Zeilen. Grundton: Cmdr hat das Umsichtige getan. Nie entschuldigend, nie alarmierend.

- **`Left {name} alone` → `Habe {name} unverändert gelassen`** · `unverändert lassen` ist macOS-Wortlaut (AppKit
  `Document.json`, „Wenn du die Datei unverändert lassen und mit einer Kopie arbeiten möchtest …“), und der Katalog
  fährt es schon in der Schwesterfamilie `askCmdr.renameUndo.skipReason.*` · `high`. Die ganze Aufzählung erbt damit
  eine Stimme von der Umbenennen-Rücknahme, die derselbe Nutzer schon kennt.
- **Ein Satz, der auf `{name}` zurückverweist, nimmt das Katalogwort, nie ein Pronomen** · „das Objekt hat sich
  geändert“, „ob sich das Objekt geändert hat“. `{name}` trägt kein Genus, das der Katalog kennen könnte, also wäre „es
  hat sich geändert“ eine Wette auf den Dateinamen. Die Umbenennen-Familie löst es genauso, dort mit „die Datei“ ·
  `high`. Konvention steht auch in `style.md` § Notes.
- **`item` → `Objekt`, auch wo die Umbenennen-Familie `Datei` sagt** · die Rücknahme entfernt auch angelegte Ordner, und
  `Objekt` ist das Katalogwort für „Datei oder Ordner“ (Termbase `item → Objekt`, macOS Finder „Ausgewählte Objekte“) ·
  `high`.
- **Löschen und Zurücklegen bleiben zwei Wörter** · `Removed …` → `… entfernt` (Katalog `remove → entfernen`,
  durchgehend: „Aus Liste entfernen“, „entfernt Cmdr zuerst die ältesten Vorgänge“), `Put … back` → `… zurückgelegt`
  (macOS Finder `N153.1` „Zurücklegen“, wie `fileOperations.trash.undone`) · `high`. `löschen` bleibt dem englischen
  `delete` in den Bestätigungstexten (`rollbackConfirm.body`, `bodyUndoByDeleting`); so trennt das Deutsche die drei
  Verben genauso wie die Quelle.
- **Das bestimmte „the N items“ trägt im Deutschen ein Doppelpunkt-Anhang, kein Artikel** · „1.234 Objekte entfernt:
  alles, was Cmdr geschrieben hatte.“ gegenüber dem Geschwister „1.234 Objekte entfernt.“ Ein Artikel scheidet aus, weil
  die `one`-Verzweigung „Das 1 Objekt“ ergäbe; der Anhang sagt die Vollständigkeit, um die es geht · `high`. Ebenso
  `doneMovingBack`: „… zurückgelegt: alles ist wieder da, wo es war.“, im Ton von `refusalAlreadyRolledBack` („Das ist
  schon wieder so wie vorher.“).
- **`Stopped after removing …` → `Gestoppt, nachdem Cmdr … entfernt hat`** · verbal, nicht nominal („Nach 12 entfernten
  Objekten gestoppt“ wäre die substantivlastige Fassung, vor der `style.md` § Voice and tone warnt); `stoppen` ist das
  gesetzte Wort für `stop` (macOS Finder „Kopieren stoppen“), nie `anhalten` (= Pause) · `high`. Die Zählphrase steht im
  Nominativ („12 Objekte“), weil keine Präposition ein Dativ-`-n` verlangt.
- **`Left {name} where it is` → `Habe {name} am neuen Ort gelassen`, und der Grund heißt
  `am alten Ort liegt jetzt etwas anderes`** · `Ort` statt `Platz`: im Katalog und im Finder ist `Platz` der
  SPEICHERPLATZ („nicht genügend freier Platz“, „Kein Platz mehr“), während macOS für den Ablageort `Ort` sagt (Finder
  `PE24`, „an spezielle Orte innerhalb des Zieles abgelegt“) · `high`. Das Paar neuer/alter Ort kommt ohne Possessiv
  aus, also ohne Genus-Wette auf `{name}`.
- **`after Cmdr put it there` → `seit Cmdr es dort abgelegt hat`** · `ablegen`/`legen` ist Finders Verb fürs Hinlegen an
  einen Ort (`PE24`; „Legt Objekte in den Papierkorb“) · `high`. `seit` statt „nachdem“, passend zum Geschwister
  `askCmdr.renameUndo.skipReason.drift.*` („seit dem Umbenennen“).
- **Die beiden `folderNotEmpty`-Werte sind WORTGLEICH mit
  `askCmdr.renameUndo.skipReason.folderNotEmpty.named`/`.counted`** · das Englische ist bei diesem Paar zeichengleich,
  also würde jede andere Formulierung `desktop-i18n-term-consistency` als neue Divergenz zählen (`de` steht bei
  `notYetReviewed: 8`, und die Zahl geht nur runter) · `high`. Wer eine der beiden Familien anfasst, ändert beide oder
  keine.
- **`leftBehind` sagt `überspringt alles`, genau wie die Bestätigungstexte davor** · beide Oberflächen geben dasselbe
  Versprechen, also müssen sie dasselbe Verb tragen; das Englische tut es auch
  (`Cmdr skips anything it isn''t sure about`, im Dialog wie im Toast) · `high`. ❌ Nicht `lässt alles unverändert`: das
  ist der Wortlaut der Aufzählungspunkte darunter („Habe … unverändert gelassen“), und die Erwartungszeile soll an den
  Dialog anschließen, den der Nutzer eben gelesen hat, nicht an die Liste, die sie einleitet.
- **`rollbackConfirm.body` bekommt den dritten Satz ohne `davon`** · die Schwester `bodyUndoByDeleting` endet auf „also
  bleibt womöglich einiges davon übrig“, hier aber stünde direkt davor „ersetzte Dateien kommen nicht zurück“, und
  `davon` würde sich an die ersetzten Dateien hängen und das Gegenteil versprechen. Also „… also bleibt womöglich
  einiges übrig.“ · `high`.
- **`Couldn''t undo {name}` → `Cmdr konnte {name} nicht rückgängig machen`** · `rückgängig machen` ist das gesetzte
  Prosa-Verb fürs Zurücknehmen eines Vorgangs (§ Operation log), und das Deutsche braucht ein Subjekt, wo das Englische
  keins hat; `Cmdr` steht schon in `refusalUnexpected` („Cmdr konnte das Rollback nicht starten.“) · `high`. Der Hinweis
  danach nennt das Laufwerk ohne Possessiv („Vielleicht ist das Laufwerk nicht verbunden oder schreibgeschützt.“,
  Termbase `read-only → schreibgeschützt`), im Ton von `trash.undoUnavailable`.

### `cancelRollback.stagedLeftover.*` (Cmdrs eigene Reste am Ziel)

Neu 2026-09-02. Zwei Zeilen über eine Arbeitsdatei, die Cmdr selbst angelegt und am Ziel nicht mehr weggeräumt bekommen
hat. Sie gehören NICHT zur `reason.*`-Liste: dort schützt Cmdr die Dateien der Person, hier ist es Cmdrs eigener Rest.

- **`unfinished copy` → `unvollständige Kopie`** · `unvollständig` ist Apples Wort für „incomplete" (macOS `LA33`: „…
  beschädigt oder unvollständig ist."), `Kopie` das Substantiv aus macOS `NE111` („eine fortsetzungsfähige Kopie
  behalten") · `high`
- **`at the destination` → `am Ziel`** · gesetzt (`destination → Ziel`), gleiche Form wie `stallWaitingDestination` ·
  `high`
- **`transfer` (Substantiv) → `Übertragung`** · der Katalog sagt es schon so
  (`errors.listing.deviceReconnecting.explanation`: „nach einer abgebrochenen oder unterbrochenen Übertragung") · `high`
- **`Cmdr clears it` → `Cmdr räumt sie weg`** · `wegräumen` statt `löschen`, weil es Cmdrs eigene Arbeitsdatei ist und
  nicht die der Person · `high`
- ⚠️ **`bei einer späteren Übertragung`, ❌ nie „beim nächsten Mal".** Cmdrs Aufräumen überspringt alles, was jünger als
  eine Stunde ist, also räumt ein sofortiger zweiter Versuch nichts weg. Ein Versprechen, das nicht hält, ist genau der
  Fehler, den diese Zeile beseitigen soll.

## Der Block-Screen bei zu altem WebKit (`main.oldWebkit.*`)

Drei Strings, die Cmdr statt seiner Oberfläche zeigt, wenn das Safari des Macs zu alt ist. Sie stehen in der HTML-Hülle,
nicht in der App, also sieht der Mensch sonst nichts von Cmdr: der Ton muss beim ersten Lesen sitzen.

- **`Software Update` → `Softwareupdate`** (ein Wort, ohne Bindestrich) · macOS schreibt `Softwareupdate` als Namen der
  Systemeinstellung; die Finder-Tier-1-Belege bestätigen das Wort (`Apple Device Software Update File` →
  `Updatedatei für Gerätesoftware von Apple`) · `high`. Nicht „Software-Update", das ist die generische Schreibung, kein
  Pane-Name.
- **`Quit` → `Beenden`** · macOS Finder AppKit-Schlüssel `Quit` → `Beenden` · `high`.
- **Kein Genitiv der Marke.** `Cmdr''s interface` wird zu `Die Oberfläche von Cmdr`, nach der Regel in `style.md` §
  Brand and do-not-translate.
- **`Safari` und `Mac` bleiben stehen.** `Safari` ist neu in `BRAND_WORDS`; die Versionsnummer `15.4` bleibt Ziffern.

## Der Hinweis auf zu altes macOS (`main.oldMacos.*`)

Ein einmaliger Dialog auf einem Mac unter macOS 12, der Cmdr zwar startet, aber außerhalb der getesteten Spanne liegt.
Ton: ehrlich und entspannt, keine Entschuldigung und keine Warnung, denn die App läuft ja.

- **`X and up` → `X und neuer`** · macOS SystemSettings (`… benötigt OS X %@ oder neuer.`) · `high`. Der Beleg schreibt
  `oder neuer`; unser Satz zählt zwei Dinge auf, also `macOS 12 und neuer`.
- **`supports` → `unterstützt`** · macOS Finder (`… da er nicht unterstützt wird.`) · `high`.
- **`best effort` → `nur so gut, wie es eben geht`** · keine Quelle im Pile führt den Begriff (die einzigen Treffer sind
  Netzwerk-QoS-Definitionen) · `high` für die Umschreibung. Bewusst KEIN Kalk: „nach bestem Bemühen“ ist Vertragsdeutsch
  und passt nicht zu Cmdrs Stimme. Der `@key.description` weist Übersetzende ausdrücklich auf die Umschreibung hin.
- **`look off` → `daneben liegen`** · Alltagsdeutsch, keine Quelle nötig; hält den Ton leicht und vermeidet das
  verbotene „falsch/Fehler“-Register.
- **Der letzte Satz ist David in der Ich-Form** und bleibt beim `du`, wie `onboarding.stepBeta.greeting`.

## Ask Cmdr schaut jetzt in Dateien hinein: Zustimmungstexte und Werkzeugzeilen (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

Fünf Schlüssel rund um das `inspect_file`-Werkzeug: die beiden Werkzeugzeilen im Chat, der neue Aufzählungspunkt auf dem
Zustimmungsbildschirm, der Absatz `contentsRule` (ersetzt `askCmdr.consent.noContents`) und der „Was ist neu“-Absatz.
Belege: Pile `de/` (macOS Finder/AppKit, Microsoft-Terminologie, Nautilus/Thunar/Dolphin, TC/DC) plus die Live-Bundles
`Preview.app` und `Photos.app` (`.loctable`, macOS 26.6.2, 2026-09-02).

- **look inside files (Werkzeugzeile) → `Dateien werden durchgesehen` / `Dateien durchgesehen`** · `durchsehen` ist das
  gesetzte Verb fürs Hineinschauen-und-Auflisten (§ Archive browsing, KDE Dolphin „Archive durchsehen“), und die Form
  spiegelt die Geschwister `Deine Fotos werden durchsucht` / `Deine Fotos durchsucht` · `high`. ❌ Nicht `durchsuchen`
  (= suchen) und nicht `Dateiinhalte werden gelesen`: Das Werkzeug liest nur einen begrenzten Teil, und `Dateiinhalte`
  ist im Katalog das Wort für das, was Cmdr NICHT liest (`askCmdr.empty.hint`).
- **look inside a file (Prosa) → `hineinschauen`** („kann Ask Cmdr jetzt hineinschauen und einen Teil davon lesen“) ·
  Katalogpräzedenz aus dem alten `noContents` („was in deine Bilder hineinschaut“); umgangssprachlich, passt zum
  `du`-Ton · `high`.
- **thumbnail → `Miniatur` (Plural `Miniaturen`)** · Apple Tier 1: Vorschau `Thumbnails` → „Miniaturen“, Fotos
  `Show/Hide Thumbnails` → „Miniaturen einblenden/ausblenden“, AppKit „Miniatur des Tab-Auswahl-Bilds“; auch schon im
  alten `noContents` · `high`. Runners-up: Microsoft „Miniaturansicht“/„Miniaturbild“ (Windows), Thunar/DC/TC
  „Vorschaubilder“ (Tier 3). ❌ Nicht `Vorschaubild`: `Vorschau` ist im Katalog der Viewer (termbase viewer → Vorschau)
  und Apples Programmname.
- **camera details (EXIF eines Fotos) → `Kameraangaben`** · `Kamera` ist überall gleich (Vorschau-Inspektor „Kamera“,
  Microsoft `camera → Kamera`, Thunar „Kamera“, Nautilus „Kameramarke“/„Kameramodell“); das `-angaben` folgt dem
  Katalog, der Metadaten `Dateiangaben` nennt (`askCmdr.renameReview.evidence.metadata` „Dateiangaben, nicht der
  Inhalt“) · `high` für `Kamera`, `tentative` für die Zusammensetzung (keine Quelle hat „camera details“ als Begriff;
  Runner-up „Kameradaten“ wäre ebenso gängig). Nicht `EXIF`: Das Englische vermeidet das Kürzel absichtlich.
- **location / where it was taken (eines Fotos) → `Aufnahmeort`** · Apples Fotos-App sagt für den Ort eines Fotos
  schlicht „Ort“ (`Assign Location` → „Ort zuweisen“, `Hide Location` → „Ort ausblenden“), Thunar/Nautilus ebenso „Ort“,
  Microsoft `location → Standort` (Gerätesinn, Fotos-App „Standortinformationen“ beim Teilen). Auf dem
  Zustimmungsbildschirm steht das Wort neben Dateien, wo ein bloßes „Ort“ als Speicherort gelesen würde; das Kompositum
  aus Apples „Ort“ und dem „aufgenommen“ aus „where it was taken“ ist eindeutig · `tentative` (keine Quelle hat das
  Kompositum selbst). In der Prosa `einschließlich des Aufnahmeorts`, in der Liste `der Aufnahmeort eines Fotos`.
- **title and author (PDF-Metadaten) → `Titel und Autor`** · Vorschau-Inspektor `Title` → „Titel“, `Author` →
  „Autor:in“; Microsoft `title → Titel`, `author → Autor`; Dolphin `Author` → „Autor“ · `high` für `Titel`, `tentative`
  für `Autor`: Apple nutzt den Gender-Doppelpunkt, den unsere Regel (keine typografischen Glyphen, Screenreader)
  ausschließt, und eine neutrale Umschreibung („wer es verfasst hat“) liest sich in der Aufzählung gestelzt. `Autor`
  steht hier als Name des Metadatenfelds, nicht als Anrede einer Person. Offen in `review-queue.md`.
- **page (eines PDFs) → `Seite`/`Seiten`** · Vorschau `Pages` → „Seiten“, `Page Count` → „Seitenanzahl“ (auch Dolphin),
  Microsoft `page → Seite` · `high`. `PDF-Seiten` mit Bindestrich, `einige Seiten eines PDFs` (Genitiv `des PDFs`,
  Termbase: `das PDF`).
- **archive → `Archiv`**, „the list of files inside an archive“ → `die Liste der Dateien in einem Archiv`, „what’s
  inside an archive“ → `was in einem Archiv steckt` · gesetzt (§ Archive browsing); `steckt` hält den lockeren Ton des
  Englischen `what’s inside` · `high`.
- **provider → `Anbieter`** („gehen an deinen Anbieter, damit er sie finden kann“) · gesetzt (Microsoft), Wortlaut aus
  dem alten `noContents` unverändert übernommen · `high`.
- **`contentsRule`: die beiden letzten Sätze sind der alte `noContents`-Wortlaut** („Die Fotosuche …“ ist neu
  angeschlossen mit `funktioniert genauso`; „Der Text, den Cmdr in den passenden Fotos erkannt hat, und deren Tags gehen
  an deinen Anbieter, damit er sie finden kann.“ und „Ask Cmdr kann Umbenennungen, Verschiebungen und Aufräumaktionen
  vorschlagen, und an keiner Datei passiert etwas, bevor du zustimmst.“ wörtlich) · `high`. Der Absatz verspricht
  bewusst NICHT „keine Dateiinhalte“ mehr, sondern `nie ganze Dateien, Fotos oder Miniaturen` und
  `einen begrenzten Teil davon`. `es` für Cmdr wie in der entfernten Meldung askCmdr.error.noConsent („was es sehen
  darf“).
- **Entfernter Neuigkeiten-Absatz (askCmdr.consent.whatsNew.body): zweiter Satz unverändert** („Das ist mehr, als du
  damals zugestimmt hast, deshalb hier noch einmal das Ganze.“); der erste Satz ist neu und nennt die vier Dinge in
  derselben Reihenfolge wie `contentsRule`.
- **„looks inside a file only when you ask about it“ → `schaut nur dann in eine Datei hinein, wenn du nach ihr fragst`**
  (`askCmdr.empty.hint`, `settings.askCmdr.intro`) · dasselbe `hineinschauen` wie oben, und `nach ihr fragst` wie in
  `contentsRule` („Wenn du nach einer Datei fragst“) · `high`. Beide Schlüssel haben das alte „nie Dateiinhalte“ / „ist
  schreibgeschützt … verändert nie etwas“ verloren; die Einstellungs-Einleitung sagt jetzt
  `verändert nie eine Datei ohne deine Zustimmung` (Zustimmung wie `bevor du zustimmst` in `contentsRule`).

## Die zwei Tooltips der Rollback-Taste (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Neue Oberfläche: Der Tooltip sagt jetzt, was DIESE Rücknahme mit den Dateien macht, und die Schaltfläche wird
abgeschaltet, sobald eine Bewegung über eine Dateisystemgrenze im letzten Schritt steht (die Originale werden entfernt,
alles liegt schon am Ziel).

- **`rollbackTooltipStopAndMoveBack` → `Stoppen und alle bisher bewegten Dateien zurücklegen`** · Rahmen vom Geschwister
  `rollbackTooltip` (`Stoppen und …`), und `zurücklegen` ist im Katalog das Verb für ORTE (macOS Finder „Put Back“ →
  „Zurücklegen“; `cancelRollback.doneMovingBack` „… zurückgelegt“) · `high`. ❌ Nicht `löschen`: die Rücknahme einer
  Bewegung löscht nichts.
- **`rollbackAlreadyLandedTooltip`** · die erste Hälfte nimmt das Bild von `cancelRollback.moveAlreadyLanded` auf („ist
  schon am Ziel“), `zurücknehmen` ist das Verb zum schon gesetzten `Rücknahme`, und `Abbrechen` ist die Beschriftung der
  Nachbarschaltfläche (`fileOperations.button.cancel`), also steht sie unverändert im Satz · `high`.

## „Terminal hier öffnen“ und seine App-Auswahl (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Neue Fläche: eine Karte in `Verhalten > Navigation & Dateioperationen`, auf der die Terminal-App für den Befehl steht.
Die Liste baut macOS; hier werden nur die Beschriftungen übersetzt.

- **terminal (die App-Gattung) → `Terminal`; Terminal (Apples App) → `Terminal`** · Apples deutsches macOS behält den
  Namen englisch (`In Terminal öffnen`, Schlüssel `N67` in `macOS/Finder/LocalizableMerged.json`), und das generische
  deutsche Wort ist dasselbe Lehnwort · `high`. Deshalb trägt die Kartenüberschrift
  `settings.navigationAndFileOps.card.terminal` eine `sameAsSourceJustification`: sie ist absichtlich identisch.
- **Open terminal here (der Befehlsname) → `Terminal hier öffnen`** · nach Apples `In Terminal öffnen`, mit `hier` für
  den Ort · `high`. Wird der Befehl selbst übersetzt (Menü, Befehlspalette), muss genau diese Fassung stehen.
- **Choose an app… → `App auswählen…`** · Apples eigenes `Choose Application…` (Schlüssel `N137`) sagt
  `Programm auswählen …`; `App` statt `Programm`, weil der Katalog durchgehend `App` schreibt · `high`.
- **terminal app → `Terminal-App`** · Bindestrich-Komposition wie die übrigen `App`-Komposita · `high`. Kein Apostroph
  in den Werten, die ICU-Dopplung `''` entfällt.

## Nach Relevanz sortieren: der Tooltip der Suchergebnis-Spalte (`fileExplorer.columns.sortByRelevance`)

Neue Fläche: der Hover-Tooltip auf der aktiven Spaltenüberschrift eines Suchergebnis-Bereichs. Der nächste Klick stellt
die Reihenfolge der Suchmaschine wieder her, bester Treffer zuerst.

- **relevance (Güte eines Suchtreffers) → `Relevanz`** · Apples Automator
  (`Automator.framework/…/LibrarySmartGroupsEditor.loctable`, `%[Relevance]@ …` → `%1$[Relevanz]@ …`) · `tentative`. ❌
  Nicht `Häufigkeit`, obwohl Apples übrige deutsche Kataloge dort genau das schreiben (WorkflowKit
  `Relevance (WFSearchSortOrder)`, AppStoreKit `SEARCH_FACET_RELEVANCE`, Musik und TV): das Wort heißt „Frequenz“ und
  benennt damit einen anderen Begriff, „Nach Häufigkeit sortieren“ würde dem Lesenden etwas Falsches versprechen.
  `Relevanz` ist das Standardwort der deutschen Oberfläche für die Suchtreffergüte. Die Quellen widersprechen sich,
  daher `tentative`. (geprüft auf macOS 26.6.2, Build 25G83, `plutil`-Auszug der mitgelieferten Lokalisierungen,
  2026-09-06)
- **Satzrahmen → `Nach Relevanz sortieren`** · genau das Muster der Geschwisterschlüssel in `commands.json`
  (`Nach Name sortieren`, `Nach Größe sortieren`) · `high`. Kein `sameAsSourceJustification`, kein Apostroph im Wert.

## Dokumente und Pakete: die OOXML-Zeile (`settings.archives.ooxml.*`)

Neue Fläche: eine Zeile in derselben Karte wie `Zip-Archive`, über der Karte `App-Pakete`. Sie deckt bewusst BEIDES ab,
Office-Dokumente (.docx, .xlsx, .pptx) und App-Pakete (.jar, .apk), deshalb nennt schon das Englische kein Office.

- **documents (die Dateiart) → `Dokumente`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Dokumente`; Dateiarten
  `RTF-Dokument`, `Reines Textdokument`) · `high`.
- **packages (generisch, nicht nur Apps) → `Pakete`** · macOS Finder `Paketinhalt zeigen` (Show Package Contents) und
  der Termbase-Eintrag `app bundle → App-Paket` derselben Wortfamilie · `high`. Bewusst das nackte `Pakete`, damit die
  Zeile breiter bleibt als die Karte `App-Pakete` darunter — genau die Trennung, die das Englische mit `packages` vs.
  `app bundles` macht.
- **Satzrahmen → `Was die Eingabetaste bei einer …, … oder … bewirkt.`** · wörtlich der Rahmen der Geschwisterschlüssel
  `settings.archives.zip.description` und `settings.archives.bundle.description` · `high`. Kein Apostroph im Wert.

## Der Server-Hub: Verbindungszustände, Trennen und Vergessen (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `.disconnect*`, `.forget*`)

Neue Fläche: der Bereichszustand beim Verbinden mit einem SMB-/SFTP-/WebDAV-Server, die Absagen des Servers, die
Verbindungspunkt-Tooltips im Volume-Umschalter und die beiden Bestätigungsdialoge (Server vergessen, gespeichertes
Passwort vergessen). Belege aus den installierten macOS-Bundles (macOS 26.6.2, Build 25G83, gelesen 2026-09-06), weil
der Referenz-Stapel auf der M1-Kiste fehlt.

- **„Connecting to {name}…“ → `Verbindung zu {name} wird hergestellt …`** · zeichengleich zum schon ausgelieferten
  `fileExplorer.network.share.connecting` und zur Termbase-Regel `connect → verbinden` · `high`. Leerzeichen vor dem `…`,
  weil es eine Fortschrittszeile ist (`style.md` § Ellipsis).
- **„couldn''t disconnect from {name}“ → `Cmdr konnte die Verbindung zu {name} nicht trennen.`** · Apples eigener
  Satzrahmen: `FileProvider.framework/Errors.loctable` `UnsafeDisconnect` („“%@” couldn’t be disconnected.“ →
  „Verbindung zu „%@“ konnte nicht getrennt werden.“) · `high`. Cmdr füllt den Subjekt-Slot, statt passiv zu bleiben,
  wie der Nachbar `fileExplorer.pane.disconnectFailedToast` („Trennen nicht möglich: …“).
- **`disconnectPlaceAriaLabel` → `Verbindung zu {name} trennen`** · derselbe Apple-Rahmen; enthält `trennen` und erfüllt
  damit die Aria-Containment-Regel gegenüber der sichtbaren Beschriftung `Trennen` (`servers.paneState.disconnect`,
  `fileExplorer.unreachable.disconnect`), da der Vergleich Groß-/Kleinschreibung ignoriert · `high`. ❌ Nicht
  `{name} trennen` nach dem Muster von `ejectVolumeAriaLabel` („{name} auswerfen“): ein Server wird nicht getrennt,
  sondern die Verbindung zu ihm.
- **`disconnectBusyTooltip` → `Trennen nicht möglich, während auf diesem Server Vorgänge laufen`** · wortgleicher Rahmen
  wie das Geschwister `fileExplorer.navigation.ejectBusyTooltip` („Auswerfen nicht möglich, während auf diesem Gerät
  Vorgänge laufen“) · `high`.
- **„The connection dropped.“ → `Die Verbindung wurde unterbrochen.`** · Apples Rendering von „the connection … was
  lost“ (`Localizable.loctable` `quhhcv3a9g`, `bzaszpfvpc`, `8c9739tzbr`: „… weil die Verbindung zum iPod unterbrochen
  wurde.“) · `high`. ❌ Nicht `abgebrochen`: das ist im Katalog der Abbruch-durch-den-Nutzer-Status.
- **„Cmdr is working on getting it back.“ → `Cmdr stellt sie gerade wieder her.`** · gesetztes
  `reconnect → Verbindung wiederherstellen` · `high`. `sie` zeigt auf `die Verbindung` im selben String, nicht auf einen
  Platzhalter, ist also kein Fall der Pronomenregel.
- **„Keychain Access“ → `Schlüsselbundverwaltung`** · `CFBundleDisplayName` in
  `/System/Library/CoreServices/Applications/Keychain Access.app/…/InfoPlist.loctable` (macOS 26.6.2, 2026-09-06); deckt
  sich mit dem schon ausgelieferten `ai.secretError.keychainBody` · `high`.
- **„doesn''t trust … certificate“ → `vertraut dem Zertifikat … nicht`** · Apples Dativ-Rektion:
  `Security.framework/Trust.loctable` („“%@” certificate is not trusted“ → „Zertifikat „%@“ wird nicht vertraut“),
  `Certificate.loctable` („Root certificate is not trusted.“ → „Root-Zertifikat wird nicht vertraut.“) · `high`. Cmdr
  behält das aktive `macOS vertraut … nicht`, weil die Stimme aktiv ist; die Rektion ist Apples.
- **SSH-Hostschlüssel im Fließtext → schlicht `der Schlüssel`, mit `von {host}` statt Genitiv** · das Englische
  vermeidet bewusst den Fachbegriff, und ein `{host}`-Platzhalter darf nie in einem Kasus-Slot stehen (`style.md` §
  Case-marked placeholders) · `high`. Also `Cmdr vertraut dem Schlüssel von {host} noch nicht.` und
  `Der Schlüssel von {host} ist … markiert.` ❌ Kein `Hostschlüssel`, ❌ kein `{host}s Schlüssel`.
- **„marked as compromised“ → `als kompromittiert markiert`** · `kompromittiert` ist Apples Wort für genau diesen Sinn
  (Wallet, `Localizable.loctable`: „… wenn du den Verdacht hast, dass deine aktuelle Nummer kompromittiert wurde.“) ·
  `high`.
- **„sign-in method“ → `Anmeldemethode`** · Kompositum aus dem gesetzten `sign in → anmelden` und Apples
  `authentication method → Authentifizierungsmethode` (`MailFramework.loctable`, `DMCLocalizable.loctable`,
  `NetworkMIDILocalizable.loctable`) · `high`. Der Kopf `-methode` ist belegt, das Bestimmungswort kommt aus Cmdrs
  freundlicherer Stimme; `Authentifizierungsmethode` wäre Apples Wort, liest sich hier aber als Jargon, den der `@key`
  ausdrücklich vermeiden will.
- **„not supported yet“ → `unterstützt … noch nicht`** · Apples durchgängiges `wird nicht unterstützt` in aktiver
  Wendung, damit Cmdr Subjekt bleibt · `high`.
- **„didn''t answer in time“ → `hat nicht rechtzeitig geantwortet`** · schon gesetzt (siehe § Umbenennen und Anlegen);
  `servers.refusal.timedOut` nimmt denselben Wortlaut · `high`.
- **„That password didn''t work for {username}.“ → `Das Passwort hat für {username} nicht geklappt.`** · wortgleich zum
  ausgelieferten `errors.volume.passwordRejected` („Das Passwort hat nicht geklappt.“), nur um den Kontonamen erweitert
  · `high`. ❌ Kein `ungültig` wie bei Apple (`EMSG_INVALID_PWD`): der `@key` sagt ausdrücklich, dass niemandem ein
  Vorwurf gemacht wird.
- **„server address“ → `Serveradresse`** · Finder `ConnectToWindow.strings` `de` (`YEA-3L-WnW.placeholderString` =
  „Serveradresse“), deckt sich mit dem ausgelieferten `servers.refusal.invalidUrl` („Das sieht nicht nach einer
  Serveradresse aus.“) · `high`. Abgrenzung: das Feld im Blatt heißt kürzer `Adresse` (`servers.sheet.address`).
- **„stops listing it“ (der Server verschwindet aus dem Umschalter) → `zeigt den Server nicht mehr an`** · gesetztes
  `show → anzeigen`, und der Rückverweis nimmt das Nomen statt eines Pronomens, weil davor der Platzhalter `{name}`
  steht (`style.md` § Ein Rückverweis auf `{name}`) · `high`.
- **„Cmdr asks for it the next time you connect.“ → `Cmdr fragt danach, wenn du dich das nächste Mal verbindest.`** ·
  `du`-Anrede wie im ganzen Katalog, und Apples `um dich mit „%@“ zu verbinden` (`NetAuthAgent/Localizable.loctable`
  `GENERIC_MSG_PASS`) belegt das reflexive `sich verbinden` · `high`.
- **`Server vergessen` / `Gespeichertes Passwort vergessen` als Dialogtitel** · zeichengleich zu den schon
  ausgelieferten Menüeinträgen `menu.network.forgetServer` und `menu.network.forgetSavedPassword` bzw. zu
  `fileExplorer.network.share.forgetPassword`; Dialog und Menü müssen dasselbe Wort tragen · `high`.
- **Der Rückverweis in den Tooltips heißt `den Server`, nicht `ihn`** · in `connectionTooltipNeedsHostKey` stünde `ihn`
  zwischen zwei maskulinen Bezugswörtern (`der Schlüssel`, `dieses Servers`) und wäre mehrdeutig;
  `connectionTooltipSaved` hat vor `Öffne …` überhaupt kein Bezugswort · `high`.

## Die Server-Übersicht: Spalten, Zustände und die Zeile im Volume-Umschalter (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

Die Zeile `Netzwerk` im Volume-Umschalter heißt jetzt `Server` und öffnet eine Tabelle aller gespeicherten und in der
Nähe gefundenen Server (Name / Typ / Adresse / Status / Zuletzt benutzt) mit einer Zeile `Server hinzufügen…` am Ende.
Die GRUPPE darüber bleibt `Netzwerk` (`fileExplorer.navigation.groupNetwork`). Belege aus den installierten
macOS-Bundles (macOS 26.6.2, Build 25G83, gelesen 2026-09-06), weil der Referenz-Stapel auf der M1-Kiste fehlt.

- **Spaltentitel `Name` / `Typ` / `Adresse` / `Status`** · Finder `LocalizableMerged` `N220` („Name“ → „Name“); `Type` →
  `Typ` (WorkflowKit/ActionKit `Localizable.loctable`); `Address` → `Adresse` (durchgängig, u. a.
  `Security.framework/OID.loctable`, `MapKit`, `Contacts`); `Status` → `Status` (CalendarLink `Localizable.loctable`,
  dazu Finder „iCloud-Status“, „Statusleiste“) · `high`. `Name` und `Status` sind zeichengleich zum Englischen und
  tragen deshalb ein `sameAsSourceJustification`.
- **„Last used“ → `Zuletzt benutzt`** · Finder `de.lproj/ConnectToWindow.strings` („Clear Recent Servers“ → „Zuletzt
  benutzte Server löschen“) und ContactsUICore `PREFERRED_LINE_PICKER_LAST_USED_VALUE` („Last Used“ → „Zuletzt benutzt“)
  · `high`. Abgrenzung: `Zuletzt verwendet` bleibt der Gruppentitel der Befehlspalette (`commandPalette.groupRecent`),
  `Zuletzt geöffnet` die Datei-Zeitangabe (`fileExplorer.dateTooltip.lastOpened`).
- **„Add server…“ → `Server hinzufügen…`** · gesetztes „Add to X“ → „Zu X hinzufügen“ plus Finders `Hinzufügen`
  (`ConnectToWindow.strings` `dFo-pT-NQm.ibShadowedToolTips[0]`) · `high`. Menü-/Tastenpunkt, also `…` ohne Leerzeichen.
- **`Connected` → `Verbunden`** · AppKit `SavePanel.loctable` („Connected“ → „Verbunden“) und Finder `SD13` („Connected
  servers“ → „Verbundene Server“) · `high`.
- **`Saved` (Zustand eines gespeicherten Servers) → `Gespeichert`** · zeichengleich zum schon ausgelieferten
  `fileExplorer.navigation.connectionTooltipSaved` („Gespeichert. Öffne den Server, …“) · `high`. ❌ Nicht Apples
  `Gesichert` (Podcasts): das ist dort „gesicherte“ Folgen im Sinn von heruntergeladen.
- **`Found nearby` → `In der Nähe gefunden`** · Apples eigener Satz „%@ have been found nearby“ → „„%@“ wurden in der
  Nähe gefunden“ (Find My) · `high`. `nearby` → `in der Nähe` ist bei Apple durchgängig.
- **`Signed out` → `Abgemeldet`** · zeichengleich zum ausgelieferten `connectionTooltipNeedsSignIn` („Abgemeldet. Öffne
  diesen Server, …“); Apple rendert „Profile (Signed out)“ als „Profil (abgemeldet)“ · `high`.
- **„Waiting for you to check the key“ → `Wartet darauf, dass du den Schlüssel prüfst`** · direkte `du`-Anrede wie im
  englischen `@key` verlangt, und `der Schlüssel` statt `Hostschlüssel` nach der schon gesetzten Zeile oben · `high`. ❌
  Keine Nominalisierung („Warten auf deine Prüfung des Schlüssels“): die Stilregel zieht das Verb vor.
- **`Never` (Spalte „Zuletzt benutzt“) → `Nie`** · Mail `PreferencesWindow.loctable` („Never“ → „Nie“) · `high`.
- **„Local network discovery is off.“ → `Die Suche im lokalen Netzwerk ist deaktiviert.`** · `discovery` → `Suche` aus
  dem eigenen Katalog (`settings.network.firstTriggerDone.label` = „Netzwerksuche gestartet“), `Local Network` →
  `Lokales Netzwerk` aus macOS' Privatsphäre-Einstellung, und „X is off“ → „X ist deaktiviert“ aus dem Katalog
  (`ai.translateError.off.title` und der entfernte Status settings.askCmdr.status.off) · `high`.
- **„Turn it on in Settings“ → `In den Einstellungen aktivieren`** · `settings.window.title` = „Einstellungen“, und
  `aktivieren` hält die Wortfamilie mit dem `deaktiviert` der Zeile darüber zusammen (`settings.network.enabled.label` =
  „Netzwerk aktivieren“) · `high`.
- **„No servers yet“ / „Add one below…“ → `Noch keine Server` / `Füge unten einen hinzu, …`** · wortgleicher Rahmen wie
  `settings.mediaIndex.chosenFolders.empty` („Noch keine Ordner. Füge einen hinzu, um …“) · `high`. Der Rückverweis am
  Satzende heißt `das Gerät`, nicht `ihn`/`es`: „ein Mac oder ein NAS“ mischt Maskulinum und Neutrum, ein Pronomen
  könnte sich also nicht auf beide beziehen. `NAS` bleibt stehen (schon im Katalog,
  `errors.listing.hostDown.suggestion`).
- **`Server` ist im Plural endungsgleich**, also tragen beide CLDR-Zweige von `servers.hub.rowCount` denselben Text
  (`{countText} Server`). Das ist korrektes Deutsch, kein vergessener Zweig.
- **„Pin / unpin server“ → `Server fixieren/lösen`** · zeichengleich zum Muster von `commands.tabTogglePin.label` („Tab
  fixieren/lösen“) und zur Termbase-Regel `pin / unpin tab` · `high`. Ohne Leerzeichen um den Schrägstrich, wie beim
  Tab-Befehl.
- **„Disconnect server“ (Befehl) → `Verbindung zum Server trennen`** · dieselbe Rektion wie
  `fileExplorer.navigation.disconnectPlaceAriaLabel` („Verbindung zu {name} trennen“): getrennt wird die Verbindung,
  nicht der Server · `high`. ❌ Nicht `Server trennen`.
- **„volume switcher“ im sichtbaren Text → `Volume-Auswahl`** · das Englische wechselt zwischen „volume chooser“
  (Befehle, Kurzbefehl-Bereich) und „volume switcher“ (Toasts); im Deutschen trägt beides den schon ausgelieferten Namen
  `Volume-Auswahl` (`commands.paneLeftVolumeChooser.label`, `commands.volumeClose.label`,
  `shortcuts.scope.volumeChooser`), damit der Nutzer eine Sache wiedererkennt · `high`.
- **`Places` (Kurzbefehl-Bereich unter einem Server) → `Orte`** · Apples durchgängiges Rendering, u. a. Finder `SD5`/
  `FI9` („Locations“ → „Orte“) und 40 `Places` → `Orte`-Treffer in den Systembundles · `high`. Ersetzt das frühere
  `Freigabe-Browser`, weil der Bereich jetzt auch Buckets und nicht nur SMB-Freigaben abdeckt.
- **`Servers` als Zeile im Umschalter und als Kurzbefehl-Bereich → `Server`** · Singular und Plural sind endungsgleich;
  die Gruppe darüber bleibt `Netzwerk`, sodass sich Zeile und Gruppe im Umschalter unterscheiden · `high`.
- **„Cmdr couldn''t change where {name} shows.“ → `Cmdr konnte nicht ändern, wo {name} angezeigt wird.`** · wortgleicher
  Rahmen wie die Geschwister `forgetServerRefusedToast` („Cmdr konnte {name} nicht vergessen.“) und
  `disconnectRefusedToast`; `{name}` bleibt Nominativ-Subjekt des Passivs · `high`.

## Das Verbindungsblatt, der Hostschlüssel und die zwei Vorschauzeilen (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

Das Blatt, in dem ein Server angelegt, angemeldet oder bearbeitet wird, plus die SSH-Vertrauensfrage darin. Der
Referenz-Stapel fehlt auf dieser Maschine, also kommen alle Tier-1-Belege direkt aus den installierten macOS-Bundles
(Rezept: `../reference-pile/how-to-mine.md` § No pile on this machine?; alles unten auf **macOS 26.6.2, 2026-09-07**
geprüft). Die ergiebigste Quelle ist `NetAuthAgent.app/Contents/Resources/{AuthDialog,Localizable}.loctable`: das ist
Apples eigener „Mit Server verbinden“-Dialog, also dieselbe Fläche wie unsere.

Begriffe:

- **`Connect` → `Verbinden`** · Finder `ConnectToWindow.strings` `46.title`, NetAuthAgent `AuthDialog` `600218.title`,
  `Localizable` `CONNECT` · `high`. Zeichengleich zum schon ausgelieferten `fileExplorer.network.connect`.
- **`Sign in` → `Anmelden`**, **`Sign in to {name}` → `Bei {name} anmelden`** · AppSSOKerberos `MainMenu.loctable`
  („Sign In“ → „Anmelden“), CloudSharing („sign in to your Apple Account“ → „melde dich bei deinem Apple Account an“) ·
  `high`. Der Titel stellt den Server voran (`Bei {name} anmelden`), weil das Verb im Deutschen ans Satzende gehört.
- **`Save` → `Sichern`** · AppKit `Document`/`SavePanel`/`Preferences`/`Printing.loctable` („Save“ → „Sichern“) ·
  `high`. ❌ Nicht `Speichern`, das ist die Microsoft-Konvention.
- **`Guest` → `Gast`, `Connect as guest` → `Als Gast verbinden`** · NetAuthAgent `AuthDialog` `RiA-l0-ASw.title`,
  `Localizable` `GUEST` · `high`. Das gesetzte `connect → verbinden` trägt die zweite Hälfte.
- **`Sign in with a username and password` → `Mit Benutzername und Passwort anmelden`** · Apples Gegenstück heißt
  `Registrierte:r Benutzer:in` (NetAuthAgent `REGISTERED_USER`), also mit Gender-Doppelpunkt, den die Stilregel wegen
  Screenreadern verbietet. Die Handlung statt der Person zu benennen umgeht das sauber und deckt sich mit Apples eigenem
  Fließtext „Gib Benutzername und Passwort für den Server „%@“ ein.“ (`PS_MSG_BOTH`) · `high`.
- **`Keychain` → `Schlüsselbund`** · NetAuthAgent `AuthDialog` `600268.title` („Remember this password in my keychain“ →
  „Passwort im Schlüsselbund sichern“) · `high`. Die Checkbox heißt `Im Schlüsselbund merken`
  (`servers.sheet.remember`), und `servers.sheet.needsStoredSecret` zitiert sie im Fließtext wortgleich.
- **`Advanced` → `Erweitert`** · AppKit `AccessibilityImageDescriptions.loctable` `NSAdvanced`, ImageKit
  `kIKScannerDeviceView_Advanced`; zeichengleich zum ausgelieferten `settings.section.advanced` · `high`.
- **`Protocol` → `Protokoll`** · `AddPrinter.app` IP-Plug-in `100268.title` („Protocol:“ → „Protokoll:“) · `high`.
- **`Browse…` (Taste, die den Dateiauswahl-Dialog öffnet) → `Durchsuchen…`** · Finder `ConnectToWindow.strings`
  `48.title` („Browse“ → „Durchsuchen“) und Finders eigener Tooltip „Verfügbare Server in einem Fenster im Finder
  durchsuchen“ · `high`. ⚠️ Das kollidiert mit `settings.archives.opt.browse` = `Durchsehen` (dort heißt „Browse“ „im
  Archiv wie in einem Ordner blättern“, nicht „einen Dateidialog öffnen“). `i18n-terms` sieht beide Schlüssel als EIN
  englisches Wort; die Spaltung ist echt und steht mit Begründung als `Browse` in
  `apps/desktop/scripts/i18n-term-consistency-allowlist.json` (§ `reviewed.de`). ❌ Also nicht zusammenführen: ein
  weiterer Dateidialog-Knopf nimmt `Durchsuchen…`, eine weitere Archiv-Fläche `Durchsehen`.
- **`Remote folder` → `Entfernter Ordner`** · macOS rendert `Remote X` durchgängig als `Entfernter X`: PrintCore
  („Remote Printer“ → „Entfernter Drucker“, „Remote host“ → „Entfernter Host“), LaunchServices („Remote Disc“ →
  „Entfernte CD/DVD“), ActionKit („Remote Host Identification Has Changed“ → „Entfernte Hostidentifizierung hat sich
  geändert“) · `high`.
- **`Key file` → `Schlüsseldatei`** · macOS `de` hat den Begriff nicht, bildet aber beide Hälften: `SSH-Schlüssel` /
  `privater Schlüssel` (ActionKit) und `-datei`-Komposita („Steuerungsdatei“, „Datendatei“, CUPS) · `high`.
- **`Key passphrase` → `Schlüssel-Passphrase`** · `tentative`. macOS `de` kennt `Passphrase` in der Oberfläche gar
  nicht: DiskManagement übersetzt jedes `passphrase` mit `Passwort` („A passphrase is required …“ → „… wird ein Passwort
  benötigt.“). Das geht hier nicht, weil direkt darüber das Feld `Passwort` steht und der englische `@key` genau diese
  Verwechslung ausschließen will. Die Kompositform folgt Apples eigenem Muster für „X password“ (`FTP-Passwort`,
  `Netzwerkpasswort`, `Drucker-Passwort`, NetAuthAgent), und `Passphrase` ist der eingeführte Begriff der SSH-Welt. Offen in
  `review-queue.md`.
- **`fingerprint` → `Fingerabdruck`, `host key` → `Hostschlüssel`** · ActionKit `Localizable.loctable`, Apples eigene
  SSH-Aktion: „The host key's fingerprint is %@.“ → „Der Fingerabdruck des Hostschlüssels ist %@.“ · `high`. Im Blatt
  steht der Server schon fest, also reicht `der Schlüssel` (wie in der schon ausgelieferten Zeile
  `servers.refusal.hostKeyUntrusted`); das Label darüber heißt `Schlüssel-Fingerabdruck`.
- **`Trust` → `Vertrauen`** · SecurityInterface `Localizable.loctable` („Trust“ → „Vertrauen“, „Always trust this
  certificate“ → „Diesem Zertifikat immer vertrauen“) · `high`. Daher `Vertrauen und verbinden` und
  `Dem neuen Schlüssel vertrauen` (Dativ, wie bei Apple).
- **`man in the middle` → `sich zwischen dich und den Server schalten`** · Apple benennt den Angriff direkt
  („Man-in-the-Middle-Angriff“, Directory Utility `LDAPv3Panel`, ActionKit) · `high`. Cmdrs Englisch vermeidet den
  Fachbegriff bewusst („something is sitting between you and it“), also übernimmt das Deutsche die Umschreibung; die
  Bewegungsvariante `sich dazwischenschalten` ist die idiomatische deutsche Form dafür.
- **`Reconnect automatically` → `Automatisch erneut verbinden`** · Kerberos-Menüleiste `Ozt-wA-9P8.title` („Reconnect“ →
  „Erneut verbinden“) plus IOBluetoothUI („This device will not reconnect automatically.“ → „Dieses Gerät wird sich
  nicht automatisch erneut verbinden.“) · `high`.
- **`Username` → `Benutzername`, `Password` → `Passwort`, `Address` → `Adresse`, `Name` → `Name`** · NetAuthAgent
  (`TOOLTIP_NAME_SMB` „Domain\Benutzername“, `UfI-08-e8v.title` „Passwort:“, `SV8-VX-EVJ.title` „Name:“), Finder
  `ConnectToWindow` `YEA-3L-WnW.placeholderString` („Server Address“ → „Serveradresse“) · `high`.

Wortlaut-Entscheidungen:

- **Die Legende der Gast-oder-Konto-Auswahl heißt `Verbindungsmodus`**, obwohl das Englische hier mit `How to connect`
  fragt. Deutsche Legenden benennen das Bedienelement, statt eine Frage zu stellen: die Nachbarlegende im selben Blatt
  heißt schlicht `Protokoll` (`servers.sheet.protocolLegend`). Apples Alternative wäre `Verbinden als:` (NetAuthAgent
  `CONNECT_AS`), passt aber nicht über beide Optionen, weil die zweite ein `anmelden` ist.
- **`First time connecting to {host}` → `Erste Verbindung zu {host}`**, kein „Zum ersten Mal mit {host} verbinden“: die
  Überschrift beschreibt eine Lage, sie fordert nichts. Der `@key` verlangt „routine, not alarming“, und ein Nominalsatz
  ist hier ruhiger als ein Imperativ.
- **`{host}''s key changed` → `Der Schlüssel von {host} hat sich geändert`** · analytischer Genitiv, weil `{host}` einen
  fremden Namen trägt und kein Genitiv-s bekommen darf. Zeichengleicher Rahmen wie das ausgelieferte
  `servers.refusal.hostKeyRevoked` („Der Schlüssel von {host} ist … markiert.“). Das Verb kommt aus ActionKit („… hat
  sich geändert“).
- **`I''ve checked it` → `Ich habe ihn geprüft`** · erste Person, wie der englische `@key` verlangt. Das `ihn` ist
  eindeutig: der Satz darüber endet auf „Prüfe den Fingerabdruck …“, und beide möglichen Bezugswörter (`Fingerabdruck`,
  `Schlüssel`) sind maskulin.
- **`the server''s owner` → `die Person, die den Server betreibt`** · kein `Betreiber`/`Besitzer`: die sind im Deutschen
  generisch maskulin, und die Stilregel verbietet sowohl den Gender-Stern als auch den ausgestellten maskulinen Default.
  `Person` ist der dort empfohlene neutrale Ausweg.
- **`Cmdr stopped connecting to {name}` → `Cmdr hat die Verbindung zu {name} gestoppt`** · nicht `abgebrochen`: das ist
  im Katalog das Wort für `Cancel` (`Abbrechen`) und läse sich, als hätte der Nutzer selbst abgebrochen. Cmdr ist das
  Subjekt, damit klar ist, dass die App bewusst angehalten hat.
- **`needsStoredSecret` zitiert beide Nachbarn wörtlich**: der Satz beginnt mit der Checkbox-Beschriftung („Automatisch
  erneut verbinden geht nur mit …“) und nennt die andere in deutschen Anführungszeichen („Im Schlüsselbund merken“).
  Eine Wortfamilie pro Dialog: `merken` → `gemerktes Passwort`, `anmelden` → `melde dich einmal an`.
- **Die drei Protokollnamen und der Beispiel-Hostname bleiben zeichengleich** und tragen deshalb je eine
  `sameAsSourceJustification`: `SMB`, `SFTP`, `WebDAV` schreibt macOS `de` genauso („SMB-Passwort“, „WebDAV-Passwort“,
  „SSH-Schlüssel“), und `nas.local` ist ein mDNS-Name. `Name` ist ebenfalls zeichengleich, mit Apples eigener
  Feldbeschriftung als Beleg.
- **Die Vorschauzeilen von „Zu Pfad gehen“ bleiben im Präsens der dritten Person**, wie das Englische: `Öffnet {name}`
  und `Fügt einen Server hinzu`. Kein `du`, weil die Zeile die Wirkung der Eingabe beschreibt und nicht den Nutzer
  anspricht.
- **`Connect to server…` (Befehlspalette) → `Mit Server verbinden…`** · Finder `ConnectToWindow` `1.title` und
  NetAuthAgent `CONNECT_TO_SERVER` („Connect to Server“ → „Mit Server verbinden“); die Auslassungspunkte hängen ohne
  Leerzeichen an, wie bei allen anderen dialogöffnenden Einträgen im Katalog (`Server hinzufügen…`,
  `Server bearbeiten…`).
- **`Connecting…` im Blatt → `Verbindung wird hergestellt …`** · zeichengleich zu `fileExplorer.network.connecting` und
  `servers.paneState.connecting`; Fortschrittszeilen bekommen das Leerzeichen vor den Auslassungspunkten.

## Der Wiederverbindungs-Zyklus und die Anmeldung per Schlüssel (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Zwei Zeilen im Bereichszustand: die Überschrift, während Cmdr eine abgerissene Verbindung von selbst zurückholt
(Backoff-Schleife, darunter Spinner, Countdown und die Tasten `Jetzt erneut versuchen` / `Abbrechen` / `Trennen`), und
die Zeile unter `Von {name} abgemeldet`, die dort steht, wo sonst `Anmelden…` stünde: der Server weist sich mit einem
Schlüssel aus, es gibt also nichts einzutippen. Belege aus den installierten macOS-Bundles (macOS 26.6.2, Build 25G83,
gelesen 2026-09-07), weil der Referenz-Stapel auf dieser Maschine fehlt.

- **`Reconnecting to {name}…` → `Verbindung zu {name} wird wiederhergestellt …`** · die schon gesetzte Termbase-Regel
  `reconnect → Verbindung wiederherstellen` (`terms.json`) trägt die Wortwahl; der Satzrahmen ist zeichengleich zum
  Geschwister `servers.paneState.connecting` (`Verbindung zu {name} wird hergestellt …`), das in derselben
  Bereichsfläche sitzt · `high`. Das Präfix `wieder-` ist der ganze Unterschied zwischen Erstverbindung und Rückholung,
  genau wie im Englischen `connecting` / `reconnecting`.
  - ❌ Nicht Apples Kurzform `Erneut verbinden …` (ScreenSharing `ScreenSharing.loctable` `reconnectingMessage`,
    HomeDataModel `HFLocalizable.loctable` `HFServiceDescriptionReconnecting`, ConversationKit — alle drei rendern das
    bare `Reconnecting…` so). Die passt als Zustandsetikett ohne Ziel; mit `{name}` daran („Erneut verbinden mit
    Naspolya …“) bricht der Satzbau. Apple selbst wechselt für die Langform in denselben Rahmen wie wir: „Stelle die
    Verbindung neu her“ (AMPDevices `kjryjsqgau`), „um die Verbindung wiederherzustellen“ (BiometricKitUI
    `BUDDY_SI_FAILED_SUBTITLE_TOUCH_ID`).
  - Leerzeichen vor dem `…`, weil es eine Fortschrittszeile ist (`style.md` § Ellipsis); Apples drei Treffer oben
    schreiben es genauso.
  - `{name}` steht nach `zu` im Dativ, bekommt als fremder Eigenname aber keine Endung: derselbe risikofreie Slot wie im
    ausgelieferten `connecting`.
- **`This server signs in with a key rather than a password, so there''s nothing to type.` →
  `Bei diesem Server meldest du dich mit einem Schlüssel an statt mit einem Passwort, du musst also nichts eingeben.`**
  · `high`.
  - Der Verbrahmen `mit … anmelden` ist im Katalog schon gesetzt (`servers.sheet.signInWithCredentials` =
    `Mit Benutzername und Passwort anmelden`, § Das Verbindungsblatt), also trägt die Schlüssel-Variante denselben. Verb
    statt Nominalisierung (`Anmeldemethode`, `Anmeldung über einen Schlüssel`), weil die Stilregel das Verb vorzieht und
    das Englische auch eins nimmt.
  - `du`-Anrede mit `du meldest dich an`: sie gibt der zweiten Hälfte (`du musst also nichts eingeben`) dasselbe
    Subjekt, sodass der Satz ohne Subjektwechsel durchläuft.
  - **`nothing to type` → `du musst nichts eingeben`** · derselbe Rahmen wie das ausgelieferte
    `errors.listing.deviceReconnecting.suggestion` („There''s nothing to unplug.“ → „Du musst nichts abziehen.“), und
    `eingeben` ist Apples Verb fürs Tippen genau dieser Sache: NetAuthAgent `Localizable.loctable` `FS_MSG_PASS` („Gib
    das Passwort für den Server „%@“ ein.“), `PS_MSG_BOTH`, `ENTER_CREDENTIALS` („Accountdaten eingeben“) · `high`. ❌
    Kein `tippen`: macOS `de` benutzt es für Tastatureingabe als Geste, nicht fürs Ausfüllen eines Felds.
  - **`a key` bleibt schlicht `ein Schlüssel`** · wie die schon gesetzte Zeile für den Hostschlüssel im Fließtext (§ Der
    Server-Hub); `SSH-Schlüssel` (ActionKit `Localizable.loctable`, „Auf diesem Gerät wurde kein SSH-Schlüssel
    gefunden.“) wäre der Fachbegriff, den der englische `@key` bewusst vermeidet. Der Bereich nennt den Server direkt
    darüber, das Wort ist also eindeutig.
- **`Open it again to retry.` → `Öffne den Server erneut, um es noch einmal zu versuchen.`** · der Rückverweis nimmt das
  Nomen statt `ihn`, weil `ein Schlüssel` im Satz davor ebenfalls maskulin ist und `ihn` mehrdeutig wäre (dieselbe
  Mechanik wie bei `connectionTooltipNeedsHostKey`, § Der Server-Hub) · `high`. Der Imperativrahmen ist zeichengleich zu
  `fileExplorer.navigation.connectionTooltipSaved` („Öffne den Server, um dich zu verbinden.“);
  `um es noch einmal zu versuchen` steht so schon in `errors.listing.emptyRootICloud.suggestion`.

## Server fixieren und lösen, die vertrauten Hostschlüssel und die ADB-Seite (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.appearance.tintSmb.*`)

Drei neue Flächen: der Kontextmenü-Punkt, mit dem ein Server aus der Volume-Auswahl verschwindet, der einmalige Hinweis,
wenn die Gruppe `Netzwerk` zu lang wird, und die beiden neuen Einstellungs-Unterabschnitte (`Server (SFTP, WebDAV)` mit
den vertrauten Hostschlüsseln, `Android (ADB)` mit dem `adb`-Status). Der Referenz-Stapel fehlt auf dieser Maschine,
also kommen alle Tier-1-Belege direkt aus den installierten macOS-Bundles (Rezept: `../reference-pile/how-to-mine.md` §
No pile on this machine?; alles unten auf **macOS 26.6.2, Build 25G83, 2026-09-07** geprüft).

Begriffe:

- **`Pin` → `fixieren`** · Safari `de.lproj/MainMenu.strings` `PrR-Dj-zwG.title` („Pin Tab“ → „Tab fixieren“) · `high`.
  ❌ Nicht Apples `anpinnen` aus Notes (`Localizable.loctable`, „Pin Note“ → „Notiz anpinnen“): der Katalog hat mit
  `menu.tab.pinTab` und `commands.serversTogglePin.label` schon Safaris Wort gesetzt, und Safaris Tab-Leiste ist die
  nähere Fläche.
- **`Unpin` → `Lösen`** · Notes `Localizable.loctable` („Unpin Note“ → „Notiz lösen“, „Unpin Notes“ → „Notizen lösen“) ·
  `high`. Deckt sich zeichengleich mit dem ausgelieferten `menu.tab.unpinTab` („Tab lösen“) und der zweiten Hälfte von
  `commands.serversTogglePin.label` („Server fixieren/lösen“). Im Kontextmenü steht das Wort allein, also
  imperativ-großgeschrieben: `Lösen`. ❌ Nicht `Loslösen` (Apples Langform in „Pin or Unpin Notes“): Notes' eigener
  Menüeintrag kürzt selbst auf `lösen`.
- **`Pin to switcher` → `In der Volume-Auswahl fixieren`** · das Ziel trägt den schon ausgelieferten Namen
  `Volume-Auswahl` (§ Die Server-Übersicht), und der `zu X`-Rahmen folgt Finders eigenen Menüeinträgen
  (`de.lproj/MenuBar.strings` `300790.title` „Add to Sidebar“ → „Zur Seitenleiste hinzufügen“, `300772.title` „Add to
  Dock“ → „Zum Dock hinzufügen“: Apple behält den Artikel) · `high`. Der Eintrag ist deutlich länger als das englische
  `Pin to switcher`; der Name der Fläche wiegt schwerer als die Kürze, weil der Nutzer sie wiedererkennen können muss.
- **`Not found` → `Nicht gefunden`** · AppKit `FindPanel.loctable` und Foundation `URL.loctable` (beide „Not found“ →
  „Nicht gefunden“) · `high`.
- **`Re-check` → `Erneut prüfen`** · Apples eigene Taste neben einem Suchergebnis:
  `SoftwareUpdate.framework/…/SUSoftwareUpdateController.loctable` („Check Again“ → „Erneut prüfen“) · `high`. ❌ Nicht
  `Erneut suchen` (Apples `Check for Updates` → „Nach Updates suchen“): gesucht wird hier nichts Neues, sondern derselbe
  Befehl noch einmal geprüft.
- **`trusted` (Attribut) → `vertrauenswürdig`** · SecurityInterface `Localizable.loctable` („This certificate will be
  marked as trusted …“ → „Dieses Zertifikat wird … als vertrauenswürdig markiert.“), Keychain Access `Errors.loctable`
  („the trusted application list“ → „die Liste der vertrauenswürdigen Programme“) · `high`. Daher `Trusted host keys` →
  `Vertrauenswürdige Hostschlüssel`. Das VERB bleibt `vertrauen` (§ Das Verbindungsblatt), die beiden Formen sind kein
  Auseinanderlaufen: Apple hält es genauso.
- **`Trusted <Datum>` (Datumspräfix in der Schlüsselzeile) → `Vertrauenswürdig seit <Datum>`** · `high` für das Wort,
  `tentative` für das `seit`. Das englische Partizip steht bloß vor dem Datum; ein bloßes deutsches `Vertraut` wäre
  zweideutig (`vertraut` heißt auch „bekannt/familiär“), also trägt `seit` die Zeitangabe. Die Zeile rendert ein
  absolutes Datum (`DateLabel`), nicht „heute“, also liest sich das sauber.
- **`Found at {path}` → `Gefunden unter {path}`** · `tentative`. macOS `de` hat keinen Satz der Form „found at <Pfad>“
  (Suche über alle `.loctable` in `/System/Library` ohne Treffer); `unter` ist die normale deutsche Präposition für eine
  Pfadangabe, und das Nachbar-Label nennt dieselbe Sache `Speicherort von adb`
  (`settings.fileOperations.adbBinaryPath.label`, `terms.json`).
- **`Watching for phones.` → `Cmdr achtet auf Telefone.`** · `tentative`. Der englische `@key` verbietet ausdrücklich
  jede Erwähnung von ADB-Server, Abo oder Socket, es gibt also nichts Technisches zu übersetzen. Der Katalog rendert
  `watch` schon dreifach (`im Blick behalten` in `ai.cloudConsent.askCmdr.proactive`, `beobachten` in `askCmdr.wake.*`,
  `überwachen` für den Downloads-Ordner); `achten auf` ist die knappste Form und hält die Zeile so kurz wie das
  Englische. Der Gegenwert `Cmdr achtet gerade nicht auf Telefone.` bleibt wortgleich, damit die beiden Zustände als ein
  Paar lesbar sind. `Telefon` ist das Katalogwort (`settings.fileOperations.adbEnabled.description`: „eines
  Android-Telefons“).
- **`Choose the adb command` → `Den Befehl „adb“ auswählen`** · wortgleicher Rahmen wie der andere Dateiauswahl-Titel im
  Katalog, `settings.behavior.openTerminalHereApp.chooseAppTitle` („Choose a terminal app“ → „Terminal-App auswählen“);
  `den Befehl „adb“` steht so schon in `settings.fileOperations.adbEnabled.description` · `high`.
- **`Browse…` (Taste, die den Dateiauswahl-Dialog öffnet) → `Durchsuchen…`** · schon gesetzt (§ Das Verbindungsblatt);
  `settings.adb.browse` übernimmt es zeichengleich von `servers.sheet.browse`. Die Spaltung gegen
  `settings.archives.opt.browse` („Durchsehen“) ist dort begründet und in der `reviewed`-Liste von `i18n-terms`
  eingetragen.

Wortlaut-Entscheidungen:

- **Eine Gruppe im Fließtext heißt `die Gruppe „Netzwerk“`**, genau wie ein Menü (`style.md` § Ein Menü im Fließtext).
  Also `Deine Gruppe „Netzwerk“ wird lang` und `Hinweis zur langen Gruppe „Netzwerk“ gezeigt`. Der Gruppenname kommt aus
  `fileExplorer.navigation.groupNetwork`, nicht aus einer Direktübersetzung.
- **`Right-click a server` → `Klicke einen Server mit der rechten Maustaste an`** · der Katalog nimmt für die
  AUFFORDERUNG genau diesen Rahmen (`errors.listing.permissionDenied.suggestion`: „klicke den Ordner mit der rechten
  Maustaste an“); das kurze Nomen `Rechtsklick` bleibt den Tooltips vorbehalten
  (`fileExplorer.navigation.favoriteTooltip`).
- **`use "{command}" from the command palette` → `führe „{command}“ in der Befehlspalette aus`** · Rahmen aus
  `main.upgradeNudge.other` („Öffne die Befehlspalette und führe Einführung… aus“). Die deutschen Anführungszeichen um
  den Platzhalter bleiben, weil er einen Befehlsnamen trägt.
- **`It stays in the Servers list.` → `Der Server bleibt in der Liste „Server“.`** · kein `Er`: davor steht
  `„{command}“`, und `der Befehl` ist ebenfalls maskulin, das Pronomen wäre also mehrdeutig (dieselbe Mechanik wie in
  `connectionTooltipNeedsHostKey`, § Der Server-Hub). Die Wiederholung von `Server` ist der Preis dafür und in Ordnung.
- **`Cmdr` nimmt im Nebensatz das Pronomen `es`** („… fragt Cmdr, bevor **es** dessen Schlüssel vertraut …“), wie schon
  in `settings.askCmdr.proactive.description` („… wenn **es** etwas vorzuschlagen hat“) und
  `onboarding.stepOptional.mtp.desc` („… solange **es** läuft“).
- **Die Hilfetexte drehen den englischen Satzbau um**, weil `the first time you connect` im Deutschen als Nebensatz vorn
  steht:
  `Wenn du dich zum ersten Mal mit einem Server verbindest, fragt Cmdr, bevor es dessen Schlüssel vertraut, und merkt sich deine Antwort hier.`
  Dasselbe im Leerzustand und in der Bestätigung, wo der Rahmen `wenn du dich das nächste Mal … verbindest` aus
  `fileExplorer.navigation.forgetSecretConfirm` kommt.
- **`Nothing trusted yet.` → `Bisher vertraust du keinem Schlüssel.`** · kein Partizip-Fragment: die Stilregel will im
  Fließtext einen ganzen Satz, und die `du`-Form sagt zugleich, wer entschieden hat. Der Ton bleibt sachlich, keine
  Warnung.
- **`Forget` bleibt `Vergessen`** und der Bestätigungstitel `Diesen Schlüssel vergessen?` · zeichengleich zur
  `forget`-Familie im Katalog (`menu.network.forgetServer` = „Server vergessen“,
  `fileExplorer.navigation.forgetServerConfirm` = „{name} vergessen? …“).
- **`Servers (SFTP, WebDAV)` → `Server (SFTP, WebDAV)`** · Singular und Plural sind endungsgleich, der Wert weicht also
  nur um das englische Plural-`s` ab; die drei Protokollnamen bleiben stehen.
- **Der Server-Farbton heißt jetzt `Serverbereiche einfärben (SMB, SFTP, WebDAV)`** · der Farbton deckt nicht mehr nur
  SMB ab. Der Rahmen bleibt der der Geschwister (`settings.appearance.tintMtp.label` = „MTP-Bereiche einfärben“,
  `tintLocal.description` = „Hintergrund-Farbton für Bereiche, die …“), `Bereich` ist das gesetzte Wort für `pane`.
- **Zwei `sameAsSourceJustification`**: `settings.adb.status.label` („Status“, dieselbe Begründung wie
  `servers.hub.colStatus`) und `settings.section.adb` („Android (ADB)“, Produktname plus Abkürzung aus Googles
  Sprachhoheit, wie schon `adb.volumeLabelWithSuffix`). Alle übrigen 25 Werte weichen vom Englischen ab.

## Das Telefon über ADB öffnen: Bereichsmeldungen, Zeilen-Tooltips und der Hinweisstreifen (`adb.*`, `settings.behavior.adbHintDismissed.*`)

19 Schlüssel für die drei neuen ADB-Flächen: die Vollbild-Meldung im Bereich, wenn ein Android-Telefon nicht aufgeht,
die Schwebehilfen auf der Telefonzeile in der Volume-Auswahl und die stille Hinweiszeile über einem MTP-Bereich. Der
Referenz-Stapel fehlt auf dieser Maschine, also kommen die Apple-Belege direkt aus den installierten Bundles (Rezept:
`../reference-pile/how-to-mine.md` § No pile on this machine?; alles auf **macOS 26.6.2, Build 25G83, 2026-09-07**
geprüft). Für Androids eigene Wörter ist Google die Instanz, nicht Apple.

Begriffe:

- **`USB debugging` → `USB-Debugging`** · jetzt erstquellig belegt statt aus Googles Doku: AOSP
  `frameworks/base/packages/SettingsLib/res/values-de/strings.xml`, `enable_adb` = „USB-Debugging“ (das Label des
  Schalters in den Entwickleroptionen), `adb_warning_title` = „USB-Debugging zulassen?“; dieselbe Schreibweise in
  SystemUI `usb_debugging_title` (abgerufen 2026-09-07, Zweig `main`) · `high`. Bestätigt die Zeile in `style.md` §
  Terminology; der Nutzer findet den Schalter am Telefon wortgleich wieder.
- **`Allow` (die Taste auf Androids eigenem Dialog) → `Erlauben`** · AOSP
  `frameworks/base/packages/SystemUI/res/values-de/strings.xml`, `usb_debugging_allow` = „Erlauben“ (abgerufen
  2026-09-07) · `high`. ❌ Nicht `Zulassen`: das ist die WLAN-Variante derselben Datei (`wifi_debugging_allow`), und der
  Nutzer steckt am Kabel. Deckt sich mit der Katalogregel `allow → erlauben` (§ Wörter, die auseinandergelaufen sind).
- **`tap` → `tippen auf`, mit dem Tastennamen in `„…“`** · AOSP `de` schreibt genau so („Tippe auf „Übersicht““,
  Settings; „Tippe zum Fortfahren auf das Symbol „Entsperren““, SystemUI) · `high`. Kein Widerspruch zur Termbase-Regel ❌
  `tippen` in § Der Wiederverbindungs-Zyklus: die verbietet `tippen` fürs AUSFÜLLEN eines Felds (dort `eingeben`), hier
  ist es die Fingergeste auf einem Touchscreen, für die Android selbst `tippen` sagt.
- **`Android platform tools` → `Android Platform Tools`** · unverändert aus `terms.json`
  (developer.android.com/tools/adb?hl=de); `adb.connect.adbNotInstalled` übernimmt den Namen zeichengleich von
  `settings.fileOperations.adbEnabled.description`.
- **`the Android tools` (generisch, nicht der Produktname) → `die Android-Tools`** · schon im Katalog („Wenn du keine
  Android-Tools installiert hast“, `settings.fileOperations.adbEnabled.description`) und die Termbase-Regel
  `tooling → Tools` · `high`. Der Produktname steht groß und ohne Bindestrich, das generische Wort klein mit: so bleiben
  die beiden Schlüssel auseinanderzuhalten.
- **`USB port` → `USB-Anschluss`, `another port` → `ein anderer Anschluss`** · macOS `de` durchweg: AirPort Utility
  `SetupRecommendations.loctable` („USB port“ → „USB-Anschluss“), `AirPortSettings.loctable` („… an den USB-Anschluss
  der Basisstation anschließt“), Mobile Device `Localizable.loctable` („… an einen USB-2.0-Anschluss … anschließen“) ·
  `high`. ❌ Nicht `Port`: das reserviert macOS `de` für die NETZWERK-Portnummer („Wähle einen anderen Port“, derselbe
  `AirPortSettings.loctable`).
- **`cable` → `Kabel`, das Verb dazu `anschließen`** · macOS `de` `Localizable.loctable` („Schließe dieses iPad mit
  einem USB-Kabel an …“, „Trenne das USB-Kabel von der Maus …“) · `high`. `reseat the cable` →
  `schließe das Kabel neu an`: `tentative` für das `neu`, weil macOS `de` den Vorgang nur zweiteilig kennt („Trenne das
  Kabel … Stecke das Ladegerät aus“) und die Schwebehilfe für zwei Sätze keinen Platz hat.
- **`isn''t responding` → `reagiert nicht`** · macOS `de` HomeKit `HFLocalizable.loctable` („Your vacuum is not
  responding.“ → „Der Staubsauger reagiert nicht.“, also GERÄT + `reagiert nicht`), loginwindow und LaunchErrors ebenso
  · `high`. Die Variante `antwortet nicht` benutzt Apple fürs NETZ (NetAuth: „Der Server „%@“ … antwortet nicht.“), und
  genau die trägt schon `servers.refusal.*`; das Telefon am Kabel nimmt daher `reagiert nicht`.
- **`wake its screen` → `aktiviere seinen Bildschirm`** · Apples Verb fürs Aufwecken eines Geräts ist `aktivieren`:
  Intents `Localizable.loctable` („Wake on Wrist Raise“ → „Durch Armheben aktivieren“), BatteryUI („Wake for network
  access“ → „Ruhezustand … beenden“) · `high` fürs Verb. ❌ Kein `aufwecken`: macOS `de` spart es für den Schlaf eines
  Menschen auf (Health, „bis du aufwachst“).
- **`too old` → `zu alt`, im Apple-Satzbau `… kann nicht …, da seine … zu alt ist`** · Mobile Device
  `Localizable.loctable` („The iPhone „%1$S“ cannot be synced because its software is too old.“ → „Das iPhone „%1$S“
  kann nicht synchronisiert werden, da seine Software zu alt ist.“) · `high`. Deshalb steht in
  `adb.connect.deviceTooOld` Cmdr vorn und der Grund hinten, obwohl das Englische umgekehrt baut: der Nebensatz mit `da`
  ist die deutsche Normalform und vermeidet ein Genitiv-`{…}`-Konstrukt.
- **`browse` (ein Telefon durchsehen) → `durchsehen`** · schon gesetzt in `settings.summary.adb` („Ein Android-Telefon
  durchsehen, auf dem USB-Debugging eingeschaltet ist.“) · `high`.
- **`phone` → `Telefon`** · das Katalogwort (§ Server fixieren und lösen …, `Cmdr achtet auf Telefone.`;
  `settings.fileOperations.adbEnabled.description`: „eines Android-Telefons“) · `high`. `device` bleibt `Gerät`, und die
  beiden Wörter folgen dem Englischen Schlüssel für Schlüssel: `adb.disconnectBusyTooltip` sagt `Gerät`, weil das
  Englische `device` sagt.

Wortlaut-Entscheidungen:

- **`Disconnect {name}` → `Verbindung zu {name} trennen`, zeichengleich zum ausgelieferten
  `fileExplorer.navigation.disconnectPlaceAriaLabel`** · beide Schlüssel tragen denselben englischen Wert, also muss
  `i18n-terms` sie gleich sehen. Der `@key` begründet `Disconnect` statt `Eject` damit, dass nichts sicher zum Abziehen
  gemacht wird; im Deutschen trägt `trennen` (Termbase: `disconnect → trennen`) genau das, während `auswerfen` das
  Volume-Wort bliebe.
- **`Can''t disconnect while operations are in progress on this device` →
  `Trennen nicht möglich, während auf diesem Gerät Vorgänge laufen`** · Rahmen wortgleich aus dem Geschwisterschlüssel
  `fileExplorer.navigation.disconnectBusyTooltip` („… während auf diesem Server Vorgänge laufen“); nur `Server` wird zu
  `Gerät`. Kein Schlusspunkt, wie im Englischen.
- **`Dismiss` (das × auf der Hinweiszeile) → `Ausblenden`** · der englische `@key` sagt selbst „hides that line for
  good“, und die Termbase-Regel § Wörter, die auseinandergelaufen sind ordnet einer ZEILE `Ausblenden` zu (`Schließen`
  schließt ein Fenster oder einen Toast). Die Zeile verschwindet dauerhaft, `Ausblenden` sagt genau das, und das
  Einstellungs-Flag daneben heißt entsprechend `ausgeblendet`. Der nächste Nachbar auf einer Bereichsfläche,
  `fileExplorer.network.osMountFallback.closeTooltip`, sagt dagegen `Schließen`: dessen Meldung kommt beim nächsten Mal
  wieder, unsere nie. Die Grenze läuft also nicht an der Fläche, sondern daran, ob etwas geschlossen oder für immer
  verborgen wird.
- **`Waiting for you to allow USB debugging` → `Warten darauf, dass du USB-Debugging erlaubst`** · Apples Wartefragment
  ist nominal-infinitivisch („Warten auf das Laufwerk …“, `style.md` § Notes), und das trägt hier den `dass`-Satz, ohne
  ein Subjekt erfinden zu müssen. Keine Auslassungspunkte: das Englische hat keine, und die beiden
  Geschwister-Schwebehilfen auf derselben Zeile sind ebenfalls punktlose Zustandssätze.
- **`Want the whole filesystem?` → `Möchtest du das gesamte Dateisystem sehen?`** · das Verb muss dazu, weil ein
  deutscher Fragesatz ohne Prädikat abgehackt klingt; `das gesamte Dateisystem` steht wortgleich in
  `settings.fileOperations.adbEnabled.description`. `Turn on USB debugging.` → `Schalte USB-Debugging ein.`, weil der
  Katalog `ein-/ausschalten` für Schalter führt („auf dem USB-Debugging eingeschaltet ist“); Androids eigenes
  `aktivieren` bleibt Androids Wort für seinen eigenen Dialog.
- **`How` → `Wie geht das?`** · der `@key` verlangt „the way a person asks ‚how do I do that?‘“; ein deutsches `Wie`
  allein ist als Linktext kein Satz und liest sich wie ein abgeschnittenes Wort. Drei Wörter statt einem, dafür die
  gemeinte Frage.
- **`Check your phone and tap Allow.` → `Sieh auf dein Telefon und tippe auf „Erlauben“.`** · zwei Imperative wie im
  Englischen; `Sieh auf …` heißt „schau hin“, was hier gemeint ist, während Apples `Überprüfe dein Gerät …` (MIDI-CI,
  `Localizable.loctable`) das Nachprüfen einer Einstellung meint.
- **`Cmdr opens your phone as soon as you do.` → `Sobald du das tust, öffnet Cmdr dein Telefon.`** · der Nebensatz nach
  vorn, weil das englische Pro-Verb `do` im Deutschen kein Gegenstück am Satzende hat. Bleibt Zusicherung, keine
  Aufforderung.
- **Das Einstellungspaar folgt seinem Geschwister** (`settings.behavior.serversPinHintSeen.*`):
  `Hinweis zu USB-Debugging ausgeblendet` / `Ob der einmalige Hinweis zu USB-Debugging ausgeblendet wurde.` Beide sind
  interne Flags und erscheinen nie in der Oberfläche, brauchen aber Deckung.
- **`You stopped opening your phone.` → `Du hast das Öffnen deines Telefons gestoppt.`** (`adb.connect.cancelled`) ·
  `stoppen`, nicht `abbrechen`: die parallele Katalogzeile `search.coverage.walk.cancelled` („You stopped this search“)
  steht schon als „Du hast diese Suche gestoppt“, und `Abbrechen` ist die BESCHRIFTUNG der Taste
  (`fileOperations.button.cancel`), also läse „abgebrochen“ wie ein Verweis auf den Knopf statt wie eine Feststellung.
  `stoppen` ist ohnehin macOS Finders Verb fürs Anhalten eines laufenden Vorgangs („Kopieren stoppen“) · `high`. Der
  substantivierte Infinitiv `das Öffnen` trägt das englische Gerundium, `öffnen` ist das gesetzte Verb fürs Aufmachen
  eines Telefons (`adb.connect.waitingHint`), und der Genitiv `deines Telefons` bleibt beim Katalogwort `Telefon`.

## Der veraltete Index eines Telefons (`driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

Die Telefon-Fassungen der drei Laufwerks-Geschwister. Ein Telefon über ADB meldet keine Änderungen, also liest sich sein
Index als veraltet, obwohl es angeschlossen ist; die Texte dürfen deshalb kein `getrennt` enthalten.

- **Titel: wortgleich mit `indexing.staleDialog.title`, nur `Laufwerks` → `Telefons`** („Der Index dieses Telefons ist
  womöglich nicht mehr aktuell“), damit beide als ein Paar lesbar bleiben · `high`. `phone → Telefon` wie in § Das
  Telefon über ADB öffnen.
- **`keeps this index current with the changes it makes` → `hält diesen Index mit den eigenen Änderungen aktuell`** ·
  Wortfamilie `aktuell` aus den Geschwistern (`title` „nicht mehr aktuell“, `driveIndex.tooltipFresh` „Indiziert und
  aktuell“); „etwas mit X aktuell halten“ ist die normale deutsche Fügung, `eigenen` bezieht sich aufs Subjekt `Cmdr`
  und spart die dritte Nennung. Der Stapel hat die Fügung nicht (macOS `de` führt `aktuell` nur als Adjektiv) · `high`.
- **`{name} doesn''t tell Cmdr when its files change` → `{name} teilt Cmdr nicht mit, wenn sich Dateien darauf ändern`**
  · ❌ Nicht `meldet Cmdr nicht`: das liest sich zuerst als Akkusativ („meldet Cmdr [irgendwo]“), `mitteilen` regiert
  eindeutig den Dativ. `Dateien darauf` statt `seine Dateien` (kein Possessiv auf `{name}`, `style.md` § Notes),
  wortgleich mit `indexing.staleDialog.body` · `high`.
- **`show up (in folder sizes and search) after a rescan` →
  `sind nach einem erneuten Durchlauf (in Ordnergrößen und Suchergebnissen) sichtbar`** · `erneuter Durchlauf` aus der
  Zeile `scan`, `Ordnergrößen und Suchergebnisse` wortgleich aus `body`; `sichtbar` in Tooltip und Dialog gleich ·
  `high`.
- **`stays as a reminder` → `bleibt zur Erinnerung stehen`** · `tentative`: kein Beleg im Stapel. KDE Dolphin nennt
  einen „Reminder“ `Hinweis`, aber `Hinweis` ist im Katalog der Toast (Zeile `toast`); `zur Erinnerung` ist die feste
  Wendung.

## Die gesperrte Server-Identität (`servers.sheet.identityLocked`)

Die zwei Zeilen unter den ausgegrauten Feldern `Adresse` und `Benutzername`, wenn ein gespeicherter Server bearbeitet
wird.

- **`the account` (das Feld, mit dem man sich am Server anmeldet) → `Konto`** · der Katalog führt genau diesen Sinn
  schon: `errors.listing.remotePermissionDenied.explanation` schreibt „das Konto, mit dem du verbunden bist“, und
  `.suggestion` „das Konto, mit dem du die Freigabe verbunden hast“ · `high`. ❌ Nicht „Account“: das bleibt Apples Wort
  für den `Apple Account` (`errors.provider.iCloud.needsAction`), und die Vermischung liest sich schlampig.
- **Der Hinweis nennt die Aktionen genauso wie die Knöpfe, auf die er zeigt**: `vergessen` aus
  `menu.network.forgetServer` („Server vergessen“) und `hinzufügen` aus `servers.sheet.addTitle` („Server hinzufügen“).
  Wer hier ein Synonym wählt („entfernen“, „anlegen“), schickt den Leser zu einem Menüpunkt, den es nicht gibt.
- **`are what name this server` → `machen diesen Server aus`** · das Blatt hat ein eigenes Feld `Name`
  (`servers.sheet.name`), also darf der Satz nicht mit „benennen“ arbeiten: sonst liest er sich, als ginge es um die
  Beschriftung. „ausmachen“ trifft den gemeinten Sinn (die beiden Werte SIND der Server) · `high`.

## Der Toast, wenn gar kein Passwort gespeichert war (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Für {name} war kein Passwort gespeichert.`** · Begriff wortgleich aus
  den drei ausgelieferten Geschwistern (`menu.network.forgetSavedPassword` und
  `fileExplorer.navigation.forgetSecretConfirmTitle` = „Gespeichertes Passwort vergessen“, `.forgetSecretConfirm` = „Das
  gespeicherte Passwort für {name} vergessen? …“, `.forgetSecretRefusedToast`) · `high`. Der Toast trägt den Begriff als
  PRÄDIKAT: das Partizip `gespeichert` sagt dasselbe wie `gespeichertes Passwort`, ohne dass der Satz in den Nominalstil
  kippt („kein gespeichertes Passwort vorhanden“ klingt nach Formular). Präteritum und kein Bedauern, wie der `@key`
  verlangt: es ist nichts schiefgegangen.
- **Der Platzhalter steht hinter `Für` und bleibt unflektiert** — dieselbe Mechanik wie die Regel gegen case-marked
  placeholders in `style.md`. Kein Pronomen dahinter, der Katalog nennt die Sache beim Namen.
- Die Referenzsammlung war auf dieser Maschine nicht vorhanden (`_ignored/i18n/` fehlt auch im Haupt-Klon), die
  Entscheidung stützt sich daher auf den ausgelieferten Katalog und diese Termbase.

## Die Wiederholdauer, die Host-Key-Überschrift und Androids Erlauben-Knopf (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`/`.retryKeepsTrying`/`.hostKeyChanged`, `adb.readiness.waitingForAuthorization`)

- **`{seconds}`/`{minutes}` tragen jetzt einen ICU-Plural mit ZWEI Platzhaltern**
  (`servers.paneState.retryTotalSeconds`, `.retryTotalMinutes`): `{seconds}` wählt nur den Zweig, gelesen wird
  `{secondsText}`, die schon formatierte Zahl. Deutsch braucht `one` und `other` (CLDR, § style.md), also `1 Sekunde` /
  `60 Sekunden` und `1 Minute` / `2 Minuten`, wortgleich aus `main.quit.countdown` (`{secondsText} Sekunde(n)`) und
  `indexing.eta.hoursMinutesLeft` (`{minutesText} Minute(n)`) · `high`.
- **Beide Werte sind Satzbausteine für `servers.paneState.retryKeepsTrying`**
  (`Es wird insgesamt {duration} lang weiterversucht.`), stehen also nackt vor `lang`: „Es wird insgesamt 2 Minuten lang
  weiterversucht.“ ❌ Keine Präposition, kein Punkt im Baustein.
- **`Cmdr won't connect to {name}` → `Cmdr verbindet sich nicht mit {name}`** · Präsens, wortgleich mit der Schwester
  `servers.refusal.hostKeyRevoked` (`Cmdr verbindet sich nicht mit diesem Server.`). Das Englische wechselte von
  „stopped connecting“ zu einer stehenden Weigerung; ein Perfekt („hat die Verbindung gestoppt“) klänge nach einem
  abgebrochenen Versuch · `high`.
- **`Allow` ist Androids eigener Knopf → `„Erlauben“`**, wortgleich aus `adb.connect.unauthorized`
  (`Sieh auf dein Telefon und tippe auf „Erlauben“.`), damit der Satz und der Bildschirm dasselbe Wort zeigen. In
  `adb.readiness.waitingForAuthorization` steht `antippst` statt `tippst auf`, weil „auf deinem Telefon auf „Erlauben“
  tippst“ zwei `auf` hintereinander hätte: dasselbe Verb, nur die trennbare Form · `high`.
- Die Referenzsammlung war auf dieser Maschine nicht vorhanden (`_ignored/i18n/` fehlt auch im Haupt-Klon), die
  Entscheidung stützt sich daher auf den ausgelieferten Katalog und diese Termbase.

## Das Kontextmenü der Server-Zeile: Öffnen und Server bearbeiten… (`menu.network.open`, `menu.network.edit`)

- **`Open` (auf einer Server-Zeile) → `Öffnen`** (`menu.network.open`) · wortgleich mit `menu.file.open`, denn es ist
  derselbe Sinn: in etwas hineingehen, nicht eine Datei an ein Programm übergeben. Das Deutsche trennt die beiden
  Bedeutungen nicht, und macOS auch nicht: Finder zeigt `Öffnen` (`LocalizableMerged` `N151`), `Öffnen mit` (`N152`) und
  `In neuem Fenster öffnen` (`FV7`, das Hineingehen) alle mit demselben Verb (Finder 26.6.2, Build 25G83, gelesen am
  2026-09-07) · `high`.
- **`Edit server…` → `Server bearbeiten…`** (`menu.network.edit`), Byte für Byte aus `commands.serversEdit.label`
  kopiert · `high`. Beide öffnen dasselbe Blatt; zwei Beschriftungen läsen sich wie zwei Funktionen. Die Auslassung ist
  das EINE Zeichen `…` (U+2026) und bleibt stehen.
- **Beide Paare sind erzwungen, nicht nur schön**: `i18n-terms` meldet es, wenn zwei Schlüssel mit demselben englischen
  Wert im Deutschen auseinanderlaufen. Wer einen der beiden Werte später umformuliert, muss den Partner mitziehen.
- **`menu.*` ist eine RAW-Familie**: Rust zeichnet das Menü über `menu_t`, nie über `t()`. Apostrophe bleiben also
  EINFACH, ein verdoppeltes `''` lässt `i18n-icu` scheitern. In diesen beiden Werten kommt keiner vor.
- Die Referenzsammlung fehlt auf dieser Maschine, aber `Finder.app` liefert dieselbe Tier-1-Evidenz direkt aus dem
  System (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## Das Dock-Angebot (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Ein einmaliger Toast, der nach ein paar Tagen fragt, ob Cmdr sein Symbol im Dock behalten darf, plus die vier
Ergebnismeldungen danach. Apples eigenes Dock-Menü liefert für fast jeden Begriff die fertige deutsche Formulierung, und
zwar als geschlossenes Wortpaar — deshalb ist hier praktisch nichts konstruiert.

Quelle für den ganzen Block: `/System/Library/CoreServices/Dock.app/Contents/Resources/{en,de}.lproj/DockMenus.strings`
(`plutil -convert json`, live macOS 26.6.2, Build 25G83, 2026-09-09), ergänzt um die Referenzsammlung
(`_ignored/i18n/de/macOS/`).

- **`Dock` bleibt `Dock`** und ist NEUTRUM: Apple schreibt „Im Dock behalten“, „Aus dem Dock entfernen“, „Zum Dock
  hinzufügen“, „aus dem Dock“ · `DockMenus` `KEEP_IN_DOCK`/`REMOVE_FROM_DOCK`, Finder `de` `N169.13`/`300772.title`,
  AppKit („Beim Abrufen des Schreibtischbilds aus dem Dock …“) · high. Der Katalog benutzt es schon so
  (`errors.listing.diskFullErrno.suggestion`: „das Papierkorb-Symbol im Dock“).
- **`Finder` bleibt `Finder`** · `DockMenus` `SHOW_IN_FINDER` → „Im Finder anzeigen“, `OPEN_IN_FINDER` → „Im Finder
  öffnen“ · high.
- **`Applications` (der Ordner) → `Ordner „Programme“`** · Finder `de` `TL_HELP_APPS` („Go to the Applications folder“ →
  „Gehe zum Ordner „Programme““), Finder-Seitenleiste `Applications` → `Programme`, AppKit („Versuche, „%@“ aus dem
  Papierkorb in deinen „Programme“-Ordner zu bewegen.“) · high. Apple schreibt beide Stellungen; die Form mit
  vorangestelltem `Ordner` liest sich im Fließtext besser und steht so auch im Katalog
  (`…downloadsNotifications.description`: „deinem Ordner „Downloads““).
- **`pin` / `unpin` (Dock-Sinn) → `im Dock behalten` / `aus dem Dock entfernen`** · `DockMenus` `KEEP_IN_DOCK` („Im Dock
  behalten“) und `REMOVE_FROM_DOCK` („Aus dem Dock entfernen“) · high. Apples Paar trägt denselben Gegensatz wie das
  englische `pinned`/`unpin`, deshalb übernimmt der Toast es wörtlich: der Titel ist zeichengleich
  `Cmdr im Dock behalten?`, und `unpinNote` sagt „wieder aus dem Dock entfernen“. ❌ NICHT `fixieren`/`lösen`: das ist
  im Katalog schon der Server- und Tab-Sinn (`menu.tab.unpinTab`, `commands.serversTogglePin.label`), und Apple selbst
  benutzt `fixieren` nur für Safari-Tabs (`MainMenu.strings` `PrR-Dj-zwG.title` „Pin Tab“ → „Tab fixieren“).
- **`Add to Dock` → `Zum Dock hinzufügen`** · Finder `de` `300772.title` · high. Die Zustimmungstaste wird damit
  `Ja, zum Dock hinzufügen` (vier Wörter, passt in eine schmale Taste). Das englische Possessiv „my Dock“ fällt weg:
  Apple behält im Deutschen durchweg den Artikel, nie ein Possessiv (`terms.json`, „Add to X“).
- **`log in` (sich am Mac anmelden) → `Anmeldung`** · `DockMenus` `OPEN_AT_LOGIN` → „Bei der Anmeldung öffnen“ · high.
  Daher `Bei deiner nächsten Anmeldung ist es da.`
- **`configuration profile` → `Konfigurationsprofil`** · macOS-Referenzsammlung, Schlüssel `Configuration Profile` in
  `de/macOS/` · high.
- **`drag` (etwas mit der Maus irgendwohin ziehen) → `ziehen`** · AppKit `de` („Ziehe deine Favoriten unten aus dem
  Bildschirm in die Touch Bar …“, „Du kannst das Symbol eines Dokuments nicht aus diesem Fenster ziehen …“) · high.
- **`Whoever manages this Mac` → `Wer diesen Mac verwaltet`** · Finder `de` `LA32` nennt genau diese Rolle („… oder an
  die Person, die deinen Computer verwaltet.“) · high. Der `Wer …`-Relativsatz ist die kürzere neutrale Form; ❌ Apples
  `deine:n Netzwerkadmin` aus derselben Zeile NICHT übernehmen, Gender-Glyphen sind wegen der Screenreader gesperrt.

Formulierungsentscheidungen in diesem Set:

- **„ein paar Tage“ trägt die Vagheit, nie eine Zahl.** Die Schwelle kann sich verschieben, also steht im Body
  `seit ein paar Tagen` und sonst nichts.
- **`addedButDockDidNotRestart` darf NICHT nach Misserfolg klingen**, denn das Symbol liegt schon im Dock; es fehlt nur
  das Neuzeichnen. Deshalb die Reihenfolge „liegt schon im Dock, … hat es nur noch nicht neu geladen“: das Ergebnis
  zuerst, die Einschränkung als Nebensatz. Kein `Fehler`, kein `fehlgeschlagen` (Cmdr-Stilregel, gilt auch für die
  anderen drei Ergebnismeldungen).
- **`notAdded` sagt, was geht, statt was nicht ging**: „Cmdr ist diesmal nicht ins Dock gekommen.“ und danach der
  Handweg. `dorthin` zeigt auf das Dock zurück und spart die zweite Nennung.
- **Rückverweis über `das Symbol`, nicht über ein Pronomen an Cmdr.** Der Body führt `das Symbol` ein, `unpinNote` und
  `notAdded` greifen es auf. Damit hängt kein `es` in der Luft, und der Genitiv `Cmdrs` bleibt draußen (Stilregel: „von
  Cmdr“).
- **`No, thanks` → `Nein, danke`, ❌ nicht `Später` / `Nicht jetzt`.** Cmdr fragt danach nie wieder, ein Aufschub-Wort
  wäre also gelogen. Apples `Not Now` → `Später` und der entfernte Katalogeintrag askCmdr.consent.decline („Nicht
  jetzt“) meinen beide den vertagten Fall, nicht diesen.
- **Die zwei `settings.behavior.dockPinNudgeOfferedAt.*`-Werte sind intern** und folgen dem Nachbarpaar
  `settings.behavior.adbHintDismissed.*` in der Form (Partizip-Label, `Ob das einmalige …`-Beschreibung).

## Das Dock-Menü von Cmdr (`menu.dock.*`)

Fünf Einträge im Menü, das beim Rechtsklick auf Cmdrs Dock-Symbol aufgeht. Das ist eine native Fläche, also gibt es kein
Bildschirmfoto; die Belege kommen aus genau der Fläche, die der Nutzer daneben sieht: Apples eigenes Dock-Menü
(`/System/Library/CoreServices/Dock.app/Contents/Resources/{en,de}.lproj/DockMenus.strings`, `plutil -convert json`,
live macOS 26.6.2, Build 25G83, 2026-09-09) und Finders Menüleiste (`de/macOS/Finder/MenuBar.json` gegen
`en-GB/macOS/Finder/MenuBar.json` in der Referenzsammlung). Rohfamilie `menu.*`: **einfache Apostrophe**, `{name}` und
`{parent}` sind wörtliche Einschübe, keine ICU-Argumente.

- **`Open Cmdr` → `Cmdr öffnen`** · `DockMenus` `OPEN` → „Öffnen“ und `OPEN_FILENAME` („Open “%@”“ → „„%@“ öffnen“) ·
  high. Apples Muster in genau diesem Menü stellt den Namen voran und hängt das Verb hinten an, wie auch `HIDE_NAME`
  („%@ ausblenden“) und `SHOW_NAME` („%@ einblenden“). Anführungszeichen bekommt der Name hier nicht: die stehen bei
  Apple um einen beliebigen Dateinamen, ein Produktname steht blank (AppKit „Force Quit %@“ → „%@ sofort beenden“).
- **`Search files…` → `Dateien suchen…`** · zeichengleich zu `menu.edit.searchFiles`, wie der englische `@key` verlangt
  (derselbe Befehl in der Menüleiste) · high.
- **`Go to folder…` → `Gehe zu Ordner…`** · Finder `de` `MenuBar` `261.title` („Go to Folder…“ → „Gehe zu Ordner …“) ·
  high. Der Dock-Eintrag nimmt Finders Wortlaut, obwohl Cmdrs eigene Menüleiste denselben Dialog `Zu Pfad gehen…` nennt
  (`menu.go.goToPath`, englisch „Go to path…“): das Englische unterscheidet die beiden Flächen genauso, und der Nutzer
  vergleicht das Dock-Menü mit Finders `Gehe zu`-Menü, nicht mit Cmdrs Menüleiste.
- **`Connect to server…` → `Mit Server verbinden…`** · Finder `de` `MenuBar` `266.title` („Connect to Server…“ → „Mit
  Server verbinden …“), NetAuthAgent `CONNECT_TO_SERVER`, und zeichengleich zum schon ausgelieferten
  Befehlspaletten-Eintrag (§ Das Verbindungsblatt) · high.
- **Auslassungspunkte ohne Leerzeichen**, obwohl Apple in beiden Finder-Einträgen ein (geschütztes) Leerzeichen davor
  setzt („Gehe zu Ordner …“, „Dock-Einstellungen …“). Der Katalog hat die dialogöffnende Form ohne Leerzeichen gesetzt
  (`style.md` § Ellipsis, `Zu Pfad gehen…`, `Dateien suchen…`, `Server hinzufügen…`), und Einheitlichkeit innerhalb
  Cmdrs Menüs wiegt hier schwerer als das eine Zeichen bei Apple.
- **`{name} ({parent})` bleibt zeichengleich** und trägt deshalb eine `sameAsSourceJustification` · Finder
  `LocalizableMerged` `IN_G6_V1` („^2 (^3)“), AppKit `Menus` („Location (%@)“ → „Umgebung (%@)“) · high. Deutsch
  qualifiziert einen Namen mit demselben Klammerzusatz in derselben Reihenfolge; es gibt nichts umzustellen.

## Das „Im Finder anzeigen“-Angebot und der Ersttreffer-Hinweis (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Zwei Momente derselben Funktion: das einmalige Angebot, „Im Finder anzeigen“ aus anderen Apps in Cmdr zu öffnen, und der
einmalige Hinweis beim ersten Mal, dass so eine Anfrage hier landet. Beide Oberflächen zeigen auf macOS-eigene Befehle,
also gewinnt die macOS-Wortwahl (style.md § Systemoberflächen).

- **„Show in Finder“ → `„Im Finder anzeigen“`, in deutschen Anführungszeichen** · Bereits gesetzt in
  `settings.navigationAndFileOps.card.showInFinder` und `settings.revealHandler.description` · `high`. Die Toast-Strings
  übernehmen exakt diese Form, damit Karte und Hinweis dieselbe Aktion beim selben Namen nennen.
- **pane → `Bereich`** · Katalogform aus `fileExplorer.doubleClickHint.body` („Bereichshintergrund“) · `high`. ❌ Nicht
  `Fenster`, ❌ nicht `Panel`.
- **Settings (Cmdrs eigenes Fenster) → `Einstellungen`** · `settings.window.title` · `high`.
- **„for a while now“ → `schon eine Weile`** · Bewusst vage: die Schwelle kann sich verschieben, also ❌ nie eine Zahl
  einsetzen. Gleiche Regel wie `main.dockPinNudge.body` („ein paar Tage“) · `high`.
- **Der Ersttreffer-Hinweis ist ❌ keine Entschuldigung** · Er sagt, was passiert ist und warum, und wo der Schalter
  sitzt. Deshalb `Cmdr ist so eingestellt, dass es das übernimmt.`, ❌ nicht „Entschuldigung“ oder „Leider“ · `high`.

## Die Einführungs-Checkliste, der Schritt-Tooltip und die vier Kurzfazits (`onboarding.*`)

Der umgeschriebene Einführungsassistent: die vier Zeilen der Mitmach-Checkliste in Schritt 3, die beiden Meldungen unter
dem E-Mail-Feld, die vier einzeiligen Fazits neben den Schaltern in Schritt 4 sowie `moreAbout`, `wizard.stepTooltip`
und `stepFda.why`. Onboarding ist die eine Fläche, auf der Cmdr in der Ich-Form als David spricht; das `du` bleibt.

- **`Save` (die Taste neben dem E-Mail-Feld) → `Sichern`** · macOS ist das Save-Wort durchweg `Sichern`, und der Katalog
  hat es mit `servers.sheet.save` bereits gesetzt · high. Die beiden Schlüssel tragen **denselben englischen Wert und
  denselben `sourceHash`** (`1509f56`), also erzwingt `desktop-i18n-term-consistency` ohnehin ein Wort für beide. Nicht
  Microsofts `Speichern` (Windows-Konvention).
- **`saved` (im Umfeld dieser Taste) → `gesichert`** · AppKit `de` („%@ (Automatisch gesichert)“, „Das Dokument „%1$@“
  konnte nicht gesichert werden.“, „Die letzten Änderungen werden im Versionsverlauf gesichert.“) · high. Abgrenzung:
  der Katalog sagt `gespeichert`, wo es ums bloße ABLEGEN von Daten geht („Anmeldedaten gespeichert“, „Notiz über dich
  gespeichert“). Hier hängt das Partizip aber direkt an der Taste `Sichern`, also gewinnt die Wortfamilie der Taste
  (`style.md` § Eine Wortfamilie pro Dialog durchhalten): `E-Mail-Adresse gesichert`,
  `Deine Adresse ist auf diesem Mac gesichert`.
- **`star` (GitHubs eigenes Verb) → `Stern`, als Handlung `einen Stern vergeben`** · GitHub lokalisiert seine Oberfläche
  NICHT ins Deutsche (die Einstellung „Preferred spoken language“ betrifft nur Kommunikationsfunktionen), es gibt also
  keinen deutschen Button-Text. Die deutsche GitHub-Doku nennt die Taste `Stern` und die Handlung „mit einem Stern
  versehen“ / „mit einem Stern markieren“ (GitHub-Doku `de`, „Repositorien markiert mit Sternen sichern“: „Klicke in der
  oberen rechten Ecke der Seite auf **Stern**.“, abgerufen 2026-09-09) · high. Die Checklistenzeile
  `onboarding.stepBeta.checklist.star` sagt es ausgeschrieben: `Vergib dem Repo auf GitHub einen Stern`. `Repo` bleibt
  die Kurzform, wie im Englischen.
- **`Like` (AlternativeTos eigenes Verb) → `ein Like geben`** · AlternativeTo ist einsprachig englisch, der Nutzer sieht
  dort also eine Taste `Like`; Microsofts Terminologie führt `like` (Verb, Beitrag) als `gefällt mir`, was aus Facebook/
  Dynamics stammt und als Linktext zu lang ist · tentative. `das Like` / `liken` stehen im Duden. Die Form
  `Gib Cmdr auf AlternativeTo ein Like` hält außerdem den Parallelbau zur Zeile darüber
  (`Vergib dem Repo auf GitHub einen Stern`).
- **`checklist` → `Checkliste`** · Microsoft-Terminologie führt beides: `Prüfliste` für die generische Definition (eine
  QA-Liste möglicher Fehler) und `Checkliste` für die Planner-Funktion · high. `Prüfliste` klingt behördlich; Cmdrs
  Tonfall nimmt das Alltagswort. `onboarding` selbst heißt im Katalog `Einführung` (`menu.app.onboarding`,
  `commands.cmdrOpenOnboarding.label`), daher `Checkliste zur Einführung`.
- **`mailing list` → `Mailingliste`** · Microsofts `Verteiler` / `Adressenliste` sind der Exchange-Sinn (eine
  Verteilerliste im Firmenadressbuch), nicht die Opt-in-Liste, um die es hier geht · tentative. `Mailingliste` ist das
  übliche deutsche Wort dafür und hält die Liste als handelndes Subjekt („Die Mailingliste hat diese Adresse nicht
  angenommen.“), was wichtig ist: nicht Cmdr weist die Adresse ab.
- **`typo` → `Tippfehler`** · der Katalog selbst (`licensing.error.badSignatureHint` „Prüfe ihn bitte auf Tippfehler …“)
  · high.
- **`signup` → `Anmeldung`, `signup server` → `Anmeldeserver`** · der Katalog (`onboarding.stepBeta.signup.failure` „die
  Anmeldung hat gerade nicht geklappt“) · high.
- **Die macOS-Berechtigung `Local Network` → `Lokales Netzwerk`** · `SecurityPrivacyExtension.appex`
  `Localizable.loctable`, Schlüssel `LOCAL_NETWORK`, und `AppSystemSettingsUI.framework` `Local Network`
  (`plutil -convert json`, live macOS 26.6.2, Build 25G83, 2026-09-09) · high. Beide Nachbarn zitieren die Zeile
  wörtlich, das Kurzfazit `stepOptional.networking.summary` wie die lange `…networking.desc`. ❌ Nicht die beschreibende
  Form „Zugriff auf das lokale Netzwerk“: unter diesem Namen steht die Berechtigung in den Systemeinstellungen nicht.
- **`space` (Plattenplatz in einer Zeile, die nicht umbrechen darf) → `Platz`** · Finder `de` („Kein Platz mehr“, „Das
  Objekt „^0“ kann nicht kopiert werden, da nicht genügend freier Platz zur Verfügung steht.“) · high. `Speicherplatz`
  bleibt die Langform für Fließtext (macOS nutzt beide); in `stepOptional.indexing.summary` und `stepAi.local.tooltip`
  kostet die Langform zu viel Breite.
- **`More about X` (der Info-Punkt neben einem Label) → `Mehr über {topic}`** · macOS `de` nutzt die knappe
  `Mehr …`-Form („Mehr Infos …“ = Learn More…, „Mehr anzeigen“) · high. Ohne Artikel, weil `{topic}` ein schon
  übersetztes Label trägt, dessen Genus der Katalog nicht kennt; `über` regiert den Akkusativ, der im Singular
  unmarkiert ist, also bleibt jeder Einschub grammatisch.
- **Der Einstellungspfad → `Einstellungen › Updates & Datenschutz`** · die beiden Hälften kommen aus
  `settings.section.updatesAndPrivacy`; das `›` bleibt wie im Englischen und wie in `askCmdr.error.notConfigured`
  („Einstellungen › KI“) · high.

Formulierungsentscheidungen in diesem Set:

- **Die vier Kurzfazits bleiben untereinander parallel und verbinitial**: `Braucht einmal die Berechtigung …`,
  `Braucht 1 GB Platz, …`, `Eine winzige Anfrage …`, `Verbindet Android-Handys …`. Sie stehen als eine Zeile neben einem
  Schalter und dürfen nicht umbrechen, also fällt jedes Füllwort weg, das die lange Fassung im Geschwister-`desc` schon
  trägt.
- **Jedes Kurzfazit erbt die Wörter seines `…desc`.** `Vorgang`/`Prozess` für den macOS-MTP-Dienst, `und Ähnliches` für
  „and the such“ (der `desc` sagt „und Ähnlichem“, hier steht der Akkusativ), `winzige Anfrage` für „tiny check“,
  `Ordnergrößen` wie `settings.section.fileAndFolderSizes`.
- **`<field></field>` steht zwischen Objekt und Verbzusatz.** Das leere Tag ist ein Eingabefeld mitten im Satz, also
  braucht der Satz eine Stelle, an der ein Kasten natürlich sitzt: `Gib deine E-Mail-Adresse <field></field> ein, um …`.
  Ein trennbares Verb liefert die Stelle gratis; ein Satz mit dem Verb am Anfang würde den Kasten ans Ende drängen.
  `eingeben` ist Apples Wort für Tastatureingabe (`style.md`), nicht `tippen`.
- **`stepTooltip` behält das wörtliche `+1`**, weil genau das die Aussage ist („drei Pflichtschritte und ein
  optionaler“). Die `select`-Zweige sind Satzanhängsel und beginnen deshalb mit Komma:
  `, es folgt noch ein optionaler Schritt` / `, das ist der letzte, optionale Schritt`. Der Rahmen kommt aus dem
  Geschwister `wizard.stepProgress` („Schritt {step} von {total}“).
- **`dumber` bleibt `dümmer`.** Das Englische wählt das grobe Wort bewusst; eine Abmilderung („weniger leistungsfähig“)
  wäre eine andere Aussage als die, die David trifft.
- **Der `<strong>`-Block in `stepAi.local.tooltip` muss `stepAi.cloud.label` zeichengleich zitieren**
  (`Ja, ich will KI`). Wird das Label umformuliert, muss der Tooltip mit.
- **`{nextLabel}` steht in deutschen Anführungszeichen** („{nextLabel}“), wie jeder andere zitierte Tastentext im
  Katalog (`onboarding.stepFda.step2.tip` „die Taste „+““).
- **released copy / Dev and test builds → `veröffentlichte Version` / `Dev- und Test-Builds`**
  (`settings.revealHandler.notProductionBuild`) · Microsoft terminology (`GERMAN.tbx`: `build` → `Build`,
  `production build` → `Produktions-Build`) für das Lehnwort, das Leser dieses Tooltips (Leute, die Cmdr selbst bauen)
  ohnehin benutzen. `Version` statt `Kopie`: der Satz meint eine Veröffentlichung wie in
  `commands.appCheckForUpdates.description` („neuere Version von Cmdr“), keinen zweiten laufenden Prozess wie
  `main.instanceLock.alertBody`. Der zweite Satz spiegelt `settings.revealHandler.notInApplications` („… jeden Klick auf
  „Im Finder anzeigen“ ins Leere laufen lassen“), damit beide Tooltips als Geschwister lesen · `high`

## Die Vorschau holt eine Datei erst herüber (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- **fetch (eine Datei vom Telefon, Server oder Archiv in eine temporäre Kopie holen) → `laden`
  (`{fileName} wird für die Vorschau geladen`)** · macOS Finder `IN_MD1` („Fetching…“ → „Laden …“), System Settings
  `Fetching Menu Item` („Fetching…“ → „Laden …“); der Katalog (`viewer.loading` „Wird geladen …“,
  `downloads.notification.title` „{fileName} geladen“). ❌ Nicht Microsofts `abrufen` (`GERMAN.tbx`): Apple rendert
  dasselbe Englisch als `laden` · `high`. `{fileName}` steht als Subjekt im Nominativ und ohne Anführungszeichen, wie in
  `downloads.notification.title`.
- **„{doneText} of {totalText}“ → `{doneText} von {totalText}`** · Finder `PW8` („^0 von ^1 kopiert“), Nautilus („%s von
  %s“), der Katalog (`fileExplorer.diskSpace.free`) · `high`
- **„so far“ (Datenmenge ohne bekannte Gesamtgröße) → `{doneText} bisher geladen`** · `bisher` aus
  `queryUi.results.live.matchesSoFar` und der Zeile „written so far → bisher geschrieben“; `geladen` hält die
  Wortfamilie des Titels · `high`
- **„stopped arriving“ (seit etwa 45 Sekunden keine Daten) → `Von dieser Datei kommen keine Daten mehr an.`** · keine
  Quelle formuliert das; bewusst nicht die Stall-Formel „Die Übertragung kommt nicht mehr voran.“, weil die Vorschau
  keinen Vorgang zeigt · `tentative`. Der Folgesatz ist belegt: `errors.listing.couldntReadUnknown.suggestion` („Prüfe,
  ob … noch verbunden ist“), Finder `N178` („Versuche es dann erneut.“) · `high`

## Stamm- und Startordner eines gespeicherten Servers (`servers.sheet.rootFolder`/`.startFolder` samt `…Help`, `servers.sheet.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

Das Blatt zum Hinzufügen und Bearbeiten eines SFTP- oder WebDAV-Servers: ein Ordner, über den Cmdr auf dem Server nie
hinausgeht, und der Ordner, in dem sich ein Bereich beim Öffnen des Servers zeigt.

- **root folder (die Obergrenze auf dem Server) → `Stammordner`** (maskulin) · Microsoft terminology (`GERMAN.tbx`,
  Term-ID 233488 „root folder“ → „Stammordner“, AUT/DEU/CHE/LUX); Double Commander „Go to root directory“ → „Zum
  Stammverzeichnis wechseln“ und Total Commander „Tab-Stammverzeichnis“ teilen den Stamm `Stamm-` · `high`. macOS hat
  keinen Begriff dafür. ❌ Nicht `Stammverzeichnis` (Verzeichnis ist nur der technische Sinn, `style.md`), nicht Thunars
  `Basisverzeichnis` oder Nautilus' `Basisordner` (Tier 3). Abgrenzung zur Zeile `der oberste Ordner eines Volumes`: die
  gilt für das englische „top folder“, das kein vom Nutzer gesetzter Begriff ist; hier benennt das Englische ein Feld,
  und MS führt genau dieses Wort. Das alte, entfernte Label „Entfernter Ordner“ meinte dasselbe Feld, sagt aber nichts
  über die Obergrenze, also kein Weiterführen.
- **start folder (wo sich der Server öffnet) → `Startordner`** (maskulin) · KDE Dolphin („Home URL“ → „Startordner“,
  „Select Home Location“ → „Startordner auswählen“), Double Commander („in all start path…“ → „in allen Startpfaden …“)
  · `high` (nur Tier 3, aber ohne Widerspruch; macOS und MS haben keinen Eintrag).
- **„a folder inside it“ → `ein Ordner darin`** · kürzer als `ein Ordner in ihm`, und das Pronominaladverb vermeidet den
  Rückbezug auf zwei maskuline Nomen · `high`.
- **„Check that it exists and that your account can read it.“ →
  `Prüfe, ob es ihn gibt und ob dein Konto ihn lesen darf.`** · `gibt es` ist das gesetzte Existenz-Idiom („Diesen
  Ordner gibt es noch nicht.“), `Konto` aus § Die gesperrte Server-Identität, `Prüfe, ob …` aus
  `errors.listing.couldntReadUnknown.suggestion`. `darf`, weil der häufigste Grund fehlende Rechte sind · `high`.
- **„so nothing was saved“ → `deshalb hat Cmdr nichts gesichert`** · das Partizip folgt der Taste `Sichern`
  (`servers.sheet.save`, § Die Einführungs-Checkliste), aktiv mit `Cmdr` als Subjekt; der erste Halbsatz ist
  zeichengleich zu `servers.refusal.timedOut` · `high`.
- **„Leave it empty to …“ → `Leer lassen, um … zu …`** · der Katalog (`settings.askCmdr.interactiveModel.description`
  „Leer lassen, um dasselbe Modell … zu verwenden.“) · `high`. `benennen` passt hier, anders als in `identityLocked`,
  weil der Hinweis wirklich über die Beschriftung im Feld `Name` spricht.

## Warum eine Freigabe nicht eingebunden wird oder die Freigabenliste nicht lädt (`errors.mount.*`, `errors.shareList.*`)

Die Sätze unter den Titeln „Freigabe ließ sich nicht einbinden“ (`fileExplorer.networkMount.mountFailedTitle`) und
„Verbindung zu {hostName} nicht möglich“ (`fileExplorer.network.share.connectFailedTitle`), dazu die drei Toasts
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` und `servers.refusal.accountNotPermitted`. Tier 1 aus dem
LIVE-Bundle `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11): es formuliert
genau diese Fälle für „Mit Server verbinden“ und fehlt im Stapel.

- **share → `Freigabe`** · NetAuthAgent `EINFO_NO_SHARE` („Die Freigabe „%@“ existiert nicht auf dem Server.“) · `high`
- **guests → `Gäste`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` („Dieser Server erlaubt keinen Gastzugriff.“) · `high`.
  `„{server}“ zeigt Gästen keine Freigaben` spart das Possessiv `seine` auf dem Platzhalter.
- **SMB version → `SMB-Version`** · `EINFO_UNSUPPORTED_VERSION` („Die Version des Servers …“), Bindestrich wie
  `SMB-Freigaben` · `high`
- **isn't responding (eine Netzwerkfreigabe) → `antwortet nicht`** · die Zeile `isn''t responding` oben:
  `antwortet nicht` fürs Netz, `reagiert nicht` fürs Gerät am Kabel · `high`
- **this computer → `dieser Computer`** · der Katalog (`settings.mediaIndex.privacyNote` „deinen Computer“) · `high`.
  Die Sätze erscheinen auch unter Linux, deshalb nicht `Mac`.
- **package → `Paket`** · KDE Dolphin („Das Paket %1 kann nicht gefunden werden.“),
  `licensing.acknowledgements.npmHeading` („npm-Pakete“) · `high`. **distribution (Linux) → `Distribution`** · kein
  Beleg im Linux-Sinn; Microsofts `Verteilung` ist der Logistik-Sinn · `tentative`
- **Rückverweis auf `{server}` → `der Server`**, nie `er`/`es` (`style.md` § Notes) · `high`. `mountRefused` sagt
  `der Server hat das Öffnen … abgelehnt`, damit niemand das Passwort für die Ursache hält.
- **„there's no connection to speed up“ → `es gibt also nichts zu beschleunigen`** · Rahmen aus
  `errors.eject.notAnSmbVolume` und `errors.eject.volumeNotFound` · `high`
- **„Try again in a moment“ → `Versuche es gleich noch einmal.`** · wortgleich mit `errors.volume.deletePending` ·
  `high`
- Gleiches Englisch, gleiches Deutsch: `errors.mount.hostUnreachable` = `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` = `errors.shareList.authFailed`. `errors.*` ist roh: normale Apostrophe, `„…“` wie
  `errors.volume.permissionDenied`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `Text-Editor`** · Microsoft-Terminologie (`text editor` → `Text-Editor`, AUT/DEU/CHE/LUX; daneben
  `Editor`) · `high`. Bindestrich wie `Terminal-App`. Das Wort meint die App-Gattung, ❌ nicht Apples App TextEdit, die
  `{app}` als Namen trägt.
- **default text editor → `Standardeditor`** · der Katalog (`commands.fileEdit.label` „Im Standardeditor bearbeiten“) ·
  `high`. Der ausgeschriebene `Standard-Text-Editor` wäre schwerfällig.
- **Edit files in [App] → `Dateien bearbeiten mit`** · der Satz läuft ins Dropdown weiter; `mit` trägt jeden App-Namen
  ohne Artikel · `tentative`.
- `{app}` steht nach `in` ohne Artikel („in {app} geöffnet“), so passt jeder App-Name. Dismiss und Open settings
  wortgleich mit `commands.handler.openTerminalHere.dismiss` / `commands.handler.openTerminalHere.openSettings`.
- **system default → `Systemstandard`** · der Katalog (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) und der Eintrag „system default → Systemstandard“ oben · `high`.
  `settings.behavior.textEditorApp.systemDefault` setzt den App-Namen in Klammern dahinter, wie
  `settings.appearance.language.opt.systemWithLanguage`.
- „Choose an app…“ und „Checking your apps…“ wortgleich mit `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. Der Hinweis (`fileExplorer.edit.hint`) folgt
  `commands.handler.openTerminalHere.hint`, nennt aber keinen Ort in den Einstellungen: sein Knopf führt direkt hin.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (eject or unmount in progress) → `wird gerade getrennt`** · Passiv-Verlaufsform des gesetzten
  `disconnect → trennen`, nach Thunar `de` („Gerät wird ausgehängt“ / „Gerät wird ausgeworfen“), das den laufenden
  Vorgang ebenso passivisch sagt · `high`. „Left its index as it was“ folgt
  `operationLog.rollback.partiallyRolledBackNotice` („so gelassen, wie er war“), „try again in a moment“
  `errors.eject.notResponding` („gleich noch einmal“).

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (abgeschlossen, das Laufwerk ist weg) → `wurde getrennt`** · Vergangenheitsform des gesetzten
  `disconnect → trennen`, wie `indexing.staleDialog.body` („Während {name} getrennt war“) · `high`. Bewusst NICHT die
  Verlaufsform „wird gerade getrennt“ aus `fileExplorer.navigation.driveIndex.driveLeaving`: dort läuft das Auswerfen
  noch, hier ist es schon passiert. „Starts from scratch“ → „startet von vorn“ wie
  `indexing.rescan.incompletePreviousScan`; `Durchlauf` und `Ordnergrößen` stammen aus derselben Datei.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

Vier Sätze für den Moment, in dem jemandem mitten im Kopieren oder Bewegen das Laufwerk herausgezogen wurde. Rohfamilie:
kein ICU, einfache Apostrophe, und `{volumeName}`, `{counterpart}`, `{done}`, `{total}` bleiben zeichengleich. Die
Arbeit macht jeweils der ZWEITE Satz: er sagt, wo die Dateien jetzt liegen. Er darf nie zu einer bloßen Sachmeldung
schrumpfen, und er steht am Satzende, weil das Deutsche dort seine Betonung trägt.

- **„was disconnected“ (das Laufwerk ist schon weg) → `wurde getrennt`** · dieselbe Vergangenheitsform wie in § A drive
  unplugged mid-index und in `errors.volume.deviceDisconnected` („Das Gerät wurde getrennt, bevor die Änderung
  abgeschlossen war.“) · `high`. Bewusst NICHT die Verlaufsform „wird gerade getrennt“ aus
  `fileExplorer.navigation.driveIndex.driveLeaving`, und bewusst nichts aus der `auswerfen`-Wortfamilie (§ Auswerfen und
  Trennen): hier wurde gezogen, nicht ausgeworfen. Der Dialogtitel daneben bleibt
  `errors.write.deviceDisconnected.title` („Gerät getrennt“), der Satz widerspricht ihm also nicht.
- **„the drive“ (das gezogene Laufwerk) → `das Laufwerk`** · gesetzt in § Auswerfen und Trennen; `Volume` bleibt dem
  technischen Sinn vorbehalten · `high`.
- **Quell- und Ziellaufwerk bekommen KEIN eigenes Wort** · `{volumeName}` ist immer das getrennte Laufwerk,
  `{counterpart}` immer die Gegenseite, und beide stehen mit `auf` im Satz („auf {counterpart} kopiert“, „liegen noch
  auf {counterpart}“). So braucht kein Satz `Quelllaufwerk`/`Ziellaufwerk` (Total Commander `1141`, Double Commander
  `doublecmd.po`), und kein Artikel muss sich auf das unbekannte Genus eines fremden Laufwerksnamens festlegen · `high`.
- **„Your originals are untouched where they were“ → `Deine Originale liegen unberührt an ihrem Platz`** · der Katalog
  hat beide Hälften schon: `errors.write.destinationNotFound.message.copy` übersetzt dasselbe englische „The originals
  are untouched.“ mit „Die Originale sind unberührt.“, und `fileOperations.cancelRollback.moveAlreadyLanded` sagt
  „liegen noch an ihrem alten Platz“ · `high`. `Originale` ist damit gesetzt, passend zu
  `fileOperations.transferProgress.titleRemovingOriginals` („Originale werden entfernt …“).
- **„so nothing is lost“ → `es ist also nichts verloren gegangen`** · macOS `de` sagt Verlust durchweg mit
  `verloren gehen` (Finder `FF45`/`FF46` „Wenn du nicht sicherst, gehen deine Änderungen verloren.“; AppKit „Deine
  Änderungen gehen verloren, wenn du diese nicht sicherst.“) · `high`. Perfekt statt Präsens, weil der Vorfall vorbei
  ist.
- **„The rest are still on the drive“ → `Der Rest liegt noch auf dem Laufwerk`** · `Der Rest` ist die gesetzte
  Entsprechung: `fileOperations.cancelRollback.stoppedDeleting` übersetzt „The rest are still there.“ mit „Der Rest ist
  noch da.“, `fileOperations.cancelRollback.stoppedMovingBack` „The rest stayed …“ mit „Der Rest liegt noch dort, …“ ·
  `high`. Das Verb `liegen` hält alle vier Sätze zusammen: jeder sagt, WO etwas liegt.
- **„before Cmdr could finish the move“ → `bevor Cmdr das Bewegen abschließen konnte`** · substantivierter Infinitiv wie
  im Geschwister `errors.write.deviceDisconnected.message.move` („während des Bewegens“), Satzbau nach
  `errors.volume.deviceDisconnected` · `high`. `bewegen` ist das gesetzte Move-Verb (macOS Finder, `terms.json`).
- **„to it“ → `darauf`, nicht `auf es`/`auf sie`** · `{volumeName}` trägt einen fremden Laufwerksnamen ohne bekanntes
  Genus, und das Pronominaladverb `darauf` ist genusfrei · `high`. Dieselbe Mechanik wie die `{name}`-Regel in
  `style.md` § Notes and decisions.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

Der Dialog nach einem Bewegen, dessen Ankunft das Ziel nicht quittiert hat. Bewusst KEINE Misserfolgsmeldung: nichts ist
verloren, und Cmdr hat die Originale gerade DESHALB behalten, weil es den Schreibvorgang nicht nachweisen konnte.
`couldn't confirm` bleibt wörtlich und darf nicht zu „ist schiefgegangen“ aufgewertet werden.

- **„Couldn't confirm the move“ → `Die Bewegung ließ sich nicht bestätigen`** · `ließ sich nicht …` ist das gesetzte
  Titelmuster für eine nicht geglückte Handlung (`errors.write.permissionDenied.title` „Dieser Ort ließ sich nicht
  öffnen“), und `bestätigen` ist das gesetzte Wort für `confirm` in genau diesem Sinn:
  `fileExplorer.pane.trashUnconfirmedToast`, `fileExplorer.rename.unconfirmed` und `fileOperations.mkdir.timeoutMessage`
  übersetzen „Couldn't confirm …“ alle mit „Es ließ sich nicht bestätigen, dass …“ · `high`. ❌ Nicht
  `Fehler`/`fehlgeschlagen` (Stilregel), und nicht `prüfen`: `prüfen` gehört im Katalog zum Nachsehen, `bestätigen` zum
  Quittieren.
- **`die Bewegung` (das Nomen für diesen EINEN Move-Vorgang)** · der Katalog führt es bereits:
  `fileOperations.transferProgress.rollbackAlreadyLandedTooltip` („lässt sich die Bewegung jetzt nicht mehr
  zurücknehmen“) und `queue.empty.body` („Kopier-, Bewegungs- und Löschvorgänge“) · `high`. Abgrenzung zur
  `style.md`-Notiz „`moves` als Nomen hat kein brauchbares deutsches Nomen“: die gilt für die AUFZÄHLUNG mehrerer
  Vorgangsarten, wo Verben besser tragen; der eine benannte Vorgang heißt `die Bewegung`.
- **„were saved on {volumeName}“ → `auf {volumeName} geschrieben wurden`** · die `@key`-Beschreibung meint „written to
  disk“, und `schreiben` ist dafür das Katalogwort (`errors.write.destinationFull.message` „bevor alles geschrieben
  war“, `errors.write.newDataKeptAt.message` „vollständig geschrieben“) · `high`. ❌ Nicht `gesichert`: `Sichern` ist im
  Katalog Apples Save-Wort für die Taste (§ Die Einführungs-Checkliste), nicht das Wort für einen Schreibvorgang auf die
  Platte.
- **„at the destination“ → `am Ziel`** · MS-Terminologie (`destination` → `Ziel`, AUT/DEU/CHE/LUX) und Nautilus `de`
  („Das Ziel ist kein Ordner.“), und der Katalog sagt es genauso (`errors.write.destinationFull.message` „Am Ziel war
  kein Speicher mehr frei“) · `high`.
- **„so it kept your originals where they were“ → `deshalb hat es deine Originale dort gelassen, wo sie waren`** ·
  `deshalb` als Hauptsatz-Anschluss wie in `errors.write.originalsKeptAside.message.many`, das Versprechen selbst wie in
  `errors.write.readOnlyDevice.source.suggestion` („Die Originale bleiben, wo sie sind.“) · `high`. Der Nebensatz steht
  am ENDE, damit der Satz auf der Entwarnung endet und nicht auf dem Problem.
- **„Have a look at the destination“ → `Sieh am Ziel nach`** · `nachsehen` ist die Katalogwendung für dieses freundliche
  „have a look“ (`fileExplorer.pane.trashUnconfirmedToast` „Sieh zur Sicherheit im Papierkorb nach.“) · `high`.
- **„then try the move again“ → `und versuche es dann noch einmal`** · `es` zeigt auf die im Titel benannte Bewegung
  zurück, wie `errors.write.deviceDisconnected.suggestion` („und versuche es erneut“) · `high`. Eine zweite Nennung der
  `Bewegung` im selben kurzen Absatz liest sich gestelzt.
- **„Your originals haven't moved.“ → `Deine Originale liegen noch dort, wo sie waren.`** · positiv gewendet statt
  verneint: `errors.write.readOnlyDevice.source.suggestion` sagt „Die Originale bleiben, wo sie sind.“ für dasselbe
  Versprechen · `high`. Ein wörtliches „haben sich nicht bewegt“ würde `bewegen` als Cmdrs Befehlsnamen anklingen lassen
  (`style.md` § Notes and decisions).

## Ein wiedergefundener Staging-Ordner (`fileOperations.leftovers.stagingFolderKept`)

Ein Info-Toast beim Wiederverbinden eines Laufwerks: Cmdr hat den Arbeitsordner einer nie beendeten Bewegung gefunden,
und darin liegen noch Dateien der Person. Cmdr lässt jede einzelne davon bewusst liegen, weil sie die einzige Kopie sein
kann. Der Ton ist Entwarnung, keine Aufgabe: nichts ist zu tun, nichts ist in Gefahr. ❌ Nie zum Löschen raten.
Abgrenzung zu `### cancelRollback.stagedLeftover.*`: dort sind es Cmdrs EIGENE Arbeitsdateien, hier die der Person.

- **„an unfinished move“ → `eine nicht abgeschlossene Bewegung`** · `nicht abgeschlossen` ist die gesetzte Form für
  „unfinished“ (`askCmdr.error.unfinishedReply` „Die Antwort wurde nicht abgeschlossen.“, `queue.failureToast.title`
  „Bewegen nicht abgeschlossen“), und `die Bewegung` ist das gesetzte Nomen für diesen EINEN Move-Vorgang (§ A move that
  could not be confirmed) · `high`. ❌ Nicht `unvollständig`: die Termbase hält es für „incomplete“ frei, also für Cmdrs
  eigene `unvollständige Kopie` in `fileOperations.cancelRollback.stagedLeftover.named`.
- **„left them in place“ → `sie an ihrem Platz gelassen`** · `an ihrem Platz` steht schon für dieselbe Entwarnung in
  `errors.write.deviceDisconnected.sided.destination.copy` („liegen unberührt an ihrem Platz“), und das Verb `lassen`
  trägt die Absicht wie in `errors.write.moveNotConfirmed.message.named` („hat es deine Originale dort gelassen, wo sie
  waren“) · `high`. ❌ Nicht `liegen geblieben` o. Ä.: eine Zustandsbeschreibung ohne Handelnden liest sich wie ein
  Versäumnis, und der Satz soll sagen, dass Cmdr sich dafür ENTSCHIEDEN hat.
- **„hidden“ (Dot-Ordner) → `verborgen`** · der Katalog hat den Sinn schon gesetzt
  (`settings.listing.showHiddenFiles.description` „die das System als verborgen markiert“), Microsoft-Terminologie führt
  `verborgenes Feld` · `high`. ❌ Nicht `ausgeblendet` (die Termbase reserviert es für „aus der Ansicht genommen“) und
  nicht `versteckt`, obwohl Nautilus, Thunar und Dolphin mehrheitlich so sagen: Tier 3 gegen eine getroffene
  Katalogentscheidung.
- **Platzhalter-Kniff: `auf {volumeName}` und `namens {folderName}`** · `{volumeName}` hängt wie in der ganzen
  `deviceDisconnected.sided.*`-Familie an `auf`, also braucht der fremde Laufwerksname weder Artikel noch Genus.
  `{folderName}` steht als Apposition nach `namens` und bleibt damit unflektiert, genau wie in
  `errors.write.duplicateSourceNames.message` („zwei Objekte namens {name}“) · `high`. Ohne Anführungszeichen, weil das
  Englische hier keine setzt.
- **Der Doppelpunkt ersetzt das englische Komma.** „left them in place, in a hidden folder …“ ist eine lose Apposition,
  die im Deutschen mit Komma nachklappert; der Doppelpunkt sagt sauber, WO dieser Platz ist, und lässt den Satz auf dem
  Ordnernamen enden, den die Person suchen soll.

## Das Favoritenmenü (`menu.go.showFavorites`, `commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`.favoritesAlreadyAdded`/`.favoritesCantAddHere`/`.seeFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D klappt die gemerkten Ordner als Menü über dem fokussierten Bereich auf; die ersten neun Zeilen tragen die Ziffern
1–9, die letzte die `0` und legt den aktuellen Ordner dazu. Die Favoriten-Rubrik in der Volume-Auswahl ist dafür
weggefallen und durch eine einzelne Zeile ersetzt, die das Menü öffnet.

- **favorites (Cmdrs Liste gemerkter Ordner) → `Favoriten`** · macOS Tier 1 durchgehend: Finder `FI10`/`TL4`/`SD8.1`
  `Favorites` → `Favoriten`, `MN2` „Der Server „^0“ konnte nicht zu deinen Favoriten hinzugefügt werden.“, AppKit
  `Ruler.json` `Add To Favorites` → `Als Favoriten sichern`; der Katalog hatte das Wort schon
  (`fileExplorer.navigation.groupFavorites`, `commands.favoritesAdd.label`) · `high`. ❌ Nie `Lesezeichen`: die Termbase
  hält es für „bookmark“ frei (Dolphin `&Bookmarks` → `&Lesezeichen`), und zwei Wörter für dieselbe Liste wären genau
  der Term-Drift, den der Audit-Abschnitt beschreibt.
- **Der orthodoxe Zwei-Fenster-Stapel liefert hier KEIN Wort** · Total Commander nennt seine `Strg+D`-Liste
  `Verzeichnisliste` (`WCMD.LNG.utf8` `1538="Verzeichnisliste (Strg+D)"`), Double Commander schreibt seine „Favorite
  Tabs“ als Glyphe `☆-Tabs`. Beides ist ein ANDERES Feature bzw. gar kein generischer Begriff (die vierte Mining-Falle
  in `../../guides/i18n-translation.md` § Researching terms), also bleibt macOS die Quelle, obwohl Cmdr dieselbe Taste
  belegt · `high`.
- **„Show favorites“ → `Favoriten anzeigen`** (`menu.go.showFavorites` UND `commands.favoritesOpen.label`, gleicher
  englischer Wert, gleicher `sourceHash`, also zwingend ein Wortlaut) · das direkte Geschwister im Katalog ist
  `Show servers` → `Server anzeigen` (`menu.go.showServers`, `commands.serversShow.label`), und beide öffnen eine Liste
  · `high`. ❌ Nicht `einblenden`: das reserviert der Katalog für Umschalter (`menu.view.showHiddenFiles` „Verborgene
  Dateien einblenden“, macOS `Alle einblenden`), hier wird nichts ein- und ausgeblendet.
- **„See {count} favorites“ → `{count} Favoriten ansehen`, `=0` → `Favoriten ansehen`** · `ansehen` ist die Katalogform
  für „see/view“ (`suggestedOps.indicatorTooltip` „Zum Ansehen klicken.“) und hält die Zeile bewusst von `anzeigen` (=
  „show“) getrennt, so wie das Englische `See` von `Show` trennt · `high`.
- **Plural: `one` + `other`, plus der `=0`-Arm aus dem Englischen** · das sind Deutschs echte CLDR-Kategorien
  (`new Intl.PluralRules('de')`, im Style-Guide § Plurals festgehalten), also keine Kopie der englischen Dreiteilung.
  `{count}` steht in `one` und `other`, im `=0`-Arm bewusst nicht, damit dort keine „0“ auftaucht. Die Zählphrase bleibt
  NOMINATIV (`5 Favoriten`, nie `5 Favoriten n`-Dativ): kein Vorwort regiert sie (Style-Guide § Plurals).
- **„the current folder“ → `der aktuelle Ordner`** · gesetzt im Katalog (`commands.favoritesAdd.description` „Den
  aktuellen Ordner des fokussierten Bereichs …“, `commands.editPaste.description`,
  `ai.cloudConsent.askCmdr.item.envelope`) · `high`. Gemeint ist der Ordner, den der Bereich GERADE ZEIGT, nicht der
  unter dem Cursor; das Deutsche trägt das wie das Englische implizit.
- **„a mounted share“ → `eine eingebundene Freigabe`** · der Katalog hat genau diesen Ausdruck schon
  (`fileExplorer.network.browser.noMountedShares` „Keine eingebundenen Freigaben von {hostName}“), `einbinden` ist
  durchgehend das Verb für „mount“ (`errors.listing.staleConnection.*`, `errors.listing.readOnlyVolumeErrno.explanation`
  „schreibgeschützt eingebunden“), und `Freigabe` ist die Termbase-Regel für „share“ · `high`. ❌ Nicht `gemountet` und
  nicht `Netzwerkfreigabe`: das Englische vermeidet hier bewusst Protokolljargon und sagt nur, dass der Mac die Freigabe
  selbst eingebunden hat.
- **„a folder on a disk“ → `Ordner auf einem Laufwerk`** · `Laufwerk` ist die Termbase-Regel für „drive“ (MS-Terminologie
  DEU/AUT/CHE) und das schlichteste Wort für „disk“ im Fließtext · `high`. ❌ Nicht `Volume`: das reserviert der Katalog
  für den technischen Volume-Begriff (Volume-Auswahl, `Zielvolume`), und der Tooltip spricht bewusst Alltagssprache.
- **„point at“ → `verweisen auf`** · trägt die Zeiger-Idee, die `zeigen` im Deutschen nur schwach hat, und hält die
  Aussage im Aktiv · `high`.
- **„Favorites menu“ (Abschnittsüberschrift der Tastaturkurzbefehle) → `Favoritenmenü`** · Kompositum wie das
  Nachbarpaar `Volume switcher` → `Volume-Auswahl` (`shortcuts.scope.volumeChooser`) und `Kontextmenü` im Katalog
  (`commands.fileContextMenu.label`) · `high`. Abgrenzung zur Style-Guide-Notiz „Ein Menü im Fließtext heißt
  `das Menü „Hilfe“`“: die gilt für einen benannten Menüleisten-Eintrag, hier benennt das Kompositum eine Fläche.
- **Die `0`-Zeile hat GENAU EINEN Schlüssel: `fileExplorer.navigation.favoritesAddCurrent`**
  (`Aktuellen Ordner zu Favoriten hinzufügen`, artikellos-knapp, Wortfamilie von `Zu Favoriten hinzufügen`). Die
  Kurzbefehl-Liste zitiert dieselbe Zeile, statt sie zu beschreiben, also liest sie denselben Wert · `high`. ❌ Keine
  zweite Fassung mit bestimmten Artikeln dafür erfinden. Die Abgrenzung, die bleibt, ist die zum echten Befehl
  `commands.favoritesAdd.label` (`Zu Favoriten hinzufügen`, ohne Objekt). `Favorit` ist schwach dekliniert, im Akkusativ
  also `den Favoriten` (`commands.favoritesOpenByNumber.label`: `Den Favoriten mit dieser Nummer öffnen`).
- **„press a number“ → `mit einer Zifferntaste`** · `tentative`: weder macOS (`de/macOS/`, Wertsuche über alle Bundles)
  noch die Microsoft-Terminologie kennt `Zifferntaste`/`Zahlentaste`; das Wort ist Standarddeutsch (Duden) und sagt
  klarer als „eine Zahl drücken“, dass eine Taste gemeint ist. Die Ziffern selbst bleiben unübersetzt. Das Englische
  nennt inzwischen das Ziel („jump to that favorite“), also nennt es das Deutsche auch:
  `commands.favoritesOpen.description` endet auf `direkt zum jeweiligen Favoriten springen` · `high`.
- **`commands.favoritesAdd.description` sagt jetzt, WOHIN der Ordner geht, nicht mehr „in die Favoriten des
  Umschalters“** · die Rubrik in der Volume-Auswahl gibt es nicht mehr, und ein Katalogwert darf keine Fläche
  beschreiben, die weg ist. Gelieferte Fassung:
  `Den aktuellen Ordner des fokussierten Bereichs zu den Favoriten hinzufügen, damit das Favoritenmenü jederzeit wieder dorthin führt.`
  · `high`. Der zweite Teil bleibt unpersönlich (kein `du`), wie die Nachbardescriptions.
- **`fileExplorer.navigation.favoritesCantAddHere` ist eine Tatsache über DIESEN Ordner, nicht eine Regel über
  Favoriten** ·
  `Dieser Ordner kann kein Favorit sein: Favoriten funktionieren nur auf Laufwerken und eingebundenen Freigaben` ·
  `high`. Das Subjekt `Dieser Ordner` spiegelt die Schwesterzeile `fileExplorer.navigation.favoritesAlreadyAdded`
  (`Dieser Ordner ist bereits ein Favorit`), sodass beide Absagen gleich anfangen. Der Doppelpunkt trägt die Begründung
  (Style-Guide: `:` erklärt). ❌ Nicht mehr `verweisen auf`: das zwang einen Kasus auf das Ziel;
  `auf Laufwerken … funktionieren` ist ein schlichter Lokativ. Kein Punkt am Ende (Tooltip).

## Wer das Laufwerk festhält: die sechs benannten Absagen (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

Sechs Zeilen, die die generische Absage `errors.eject.unmountRefused` aufschlüsseln: macOS hat das Auswerfen abgelehnt,
und Cmdr kann diesmal sagen, WER das Laufwerk hält. Sie erben den Rahmen und die Regeln von § Auswerfen und Trennen
(Rohfamilie, einfache Apostrophe, kein ICU, jeder Wert ein eigenständiger Satz hinter einem Doppelpunkt). Die drei
Geschwister `unmountRefused` / `…ByApp` / `…ByApps` müssen als eine Familie lesbar sein.

- **„is still using this drive“ → `verwendet dieses Laufwerk noch`** · macOS Finder `de` sagt genau diesen Vorwurf mit
  `verwenden`: „Das Objekt „^0“ wird von macOS verwendet …“, „Das Wechselmedium „^0“ wird gerade verwendet und kann
  nicht ausgeworfen werden.“, „… da das Laufwerk möglicherweise gerade von einem anderen Programm verwendet wird“ ·
  `high`. Damit trägt die Zeile dasselbe Wortfeld wie das Geschwister `unmountRefused` („noch in Verwendung“), nur
  aktiv, weil hier ein Subjekt zum Nennen da ist. ❌ Nicht `belegt`, `blockiert` oder `greift zu`.
- **Der Platzhalter steht am Satzanfang, aktiv, ohne Artikel** · `{app}` trägt einen echten Prozessnamen („Vorschau“,
  „Warp“, „mds_stores“), also ein fremdes Wort ohne bekanntes Genus. Als Subjekt im Nominativ braucht es keinen Artikel
  und keine Flexion, und der Name steht sofort hinter dem Doppelpunkt des Rahmens, wo er am meisten nützt. Die
  Passiv-Variante nach macOS-Vorbild („Dieses Laufwerk wird noch von {app} verwendet.“) wäre grammatisch ebenso sauber,
  begräbt den Namen aber in der Satzmitte; die Stilregel „lieber aktiv“ entscheidet zusätzlich. Ein kleingeschriebener
  Werkzeugname am Satzanfang (`mds_stores …`) ist dabei hinzunehmen, das Englische hat dieselbe Stelle.
- **Der Rückverweis auf den Halter entfällt in BEIDEN Zeilen** · das Englische sagt „Close anything **it** has open
  there“ / „**they** have open there“. Im Singular ist das Pronomen versperrt: `style.md` § Notes and decisions
  verbietet den Pronomen-Rückverweis auf einen `{name}`-Platzhalter, und ein „es“ für die App würde sich mit dem „es“
  für das Laufwerk im nächsten Teilsatz stoßen. Also steht in beiden Zeilen derselbe Nachsatz („Schließe alles, was dort
  geöffnet ist, …“), obwohl der Plural ein genusfreies „sie“ vertragen hätte: die drei Geschwister bleiben so als eine
  Familie lesbar, und der Halter ist im ersten Satz ohnehin benannt. `dort` zeigt auf das zuletzt genannte Laufwerk,
  genau wie das englische „there“.
- **`other apps` → `andere Apps`, Nominativ Plural** · `Intl.ListFormat('de')` hängt es ohne Komma an („Vorschau, Warp,
  Fotos **und** andere Apps“, gemessen mit Node), und `{apps}` ist in der deutschen Fassung IMMER das Satzsubjekt. Damit
  braucht das Listenende keine Kasusform: die Grundform stimmt. Würde ein späterer Satzbau `{apps}` in einen Dativ
  schieben („von anderen Apps“), müsste das `-n` mit; genau deshalb bleibt der Satz aktiv mit dem Platzhalter als
  Subjekt. `App` ist im Katalog gesetzt (§ Auswerfen und Trennen), auch für Kommandozeilen-Werkzeuge, wie schon im
  Englischen.
- **`disk image` → `Image`** · der Katalog sagt es zweimal genau so (`errors.listing.readOnlyVolumeErrno.suggestion`
  „Wenn es ein Image ist, binde es mit Schreibzugriff erneut ein“, `updates.moveToApplicationsDialog.readOnlyVolume`
  „meist einem noch geöffneten Image“), und macOS `de` nimmt im Fließtext dieselbe Kurzform („Image „^0“ auf das Medium
  brennen …“) · `high`. Der Zweitplatzierte `Disk-Image` steht in macOS nur als Info-Fenster-Etikett (Finder
  `InfoWindowGeneralView` „Disk-Image-Wert“); er wäre für sich klarer, würde aber gegen zwei bereits ausgelieferte
  Katalogstellen driften. ❌ Nicht `Datenträgerabbild` (kommt in macOS `de` gar nicht vor).
- **Der Image-Satz nimmt den Rahmen von `errors.eject.busy`** · „**Auf diesem Laufwerk liegt** ein Image, das noch
  geöffnet ist.“ neben „**Auf diesem Laufwerk läuft** noch ein Vorgang von Cmdr.“ · `liegen` ist das Katalogverb für „wo
  etwas ist“ (§ A drive pulled mid-transfer), und die vorangestellte Ortsangabe sagt genau das, worauf es ankommt: die
  DATEI des Images liegt auf dem Laufwerk. Eine eingeschobene Relativkonstruktion („Ein Image, das auf diesem Laufwerk
  liegt, ist noch geöffnet.“) wäre dieselbe Aussage, liest sich aber verschachtelt.
- **„Eject that image first, then eject this drive.“ → `Wirf erst das Image aus, dann das Laufwerk.`** · Gapping mit
  geteiltem trennbaren Verb ist normales Deutsch („Räum erst die Küche auf, dann das Bad.“) und hält den Satz kurz. Das
  zweite `wirf … aus` auszuschreiben wäre in einem so knappen Toast Wortwiederholung.
- **„is still working with this drive“ → `arbeitet noch mit diesem Laufwerk`** · bewusst NICHT `verwenden`: das
  Englische unterscheidet an dieser Stelle selbst („working with“ statt „using“), weil hier niemand etwas zu schließen
  hat, sondern Spotlight oder Time Machine im Hintergrund arbeitet. `mit … arbeiten` steht im Katalog schon für genau
  diesen Sinn (`errors.listing.lockUnavailable.suggestion` „Apps, die mit vielen Dateien arbeiten“) · `high`.
- **`macOS` steht unverändert am Satzanfang** · die Kleinschreibung bleibt (Apples Schreibweise), und der Katalog setzt
  das schon so: `errors.listing.notPermitted.explanation` „macOS hat Cmdr den Zugriff auf `{path}` verwehrt.“ ❌ Keine
  Kasus-Endung, kein `Mac OS`, kein `das System`.
- **`Cmdr itself` → `Cmdr selbst`, direkt hinter dem Subjekt** · „Cmdr selbst verwendet dieses Laufwerk noch.“ Die
  Abgrenzung zu `errors.eject.busy` ist wichtig und darf nicht verwischen: `busy` meint einen LAUFENDEN Vorgang („Auf
  diesem Laufwerk läuft noch ein Vorgang von Cmdr.“), `unmountRefusedByCmdr` meint einen Griff, den Cmdr fälschlich
  nicht losgelassen hat. Deshalb hier `verwenden` und nicht `Vorgang`.
- **„send a report“ → `sende einen Fehlerbericht`** · der volle Feature-Name, weil die Zeile ihn zum ersten und einzigen
  Mal nennt (`style.md` § Notes and decisions: voll nennen, dann kürzen). Er ist zeichengleich mit dem, was der Nutzer
  im Menü und im Toast sieht: `menu.help.sendErrorReport` / `ui.toast.sendErrorReport` „Fehlerbericht senden“ · `high`.
  ❌ Nicht `Absturzbericht` (der ist für einen echten Absturz reserviert, § Absturzdialog) und nicht das bloße
  `Bericht`, das erst nach einer vollen Nennung in der Nähe zulässig ist.
- **„if it keeps happening“ → `wenn das immer wieder passiert`** · die gesetzte Katalogwendung, mehrfach in
  `errors.listing.*.suggestion` („Wenn das immer wieder passiert, …“) · `high`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Art`** · macOS Finder `de`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Alle mit Endung *.{extension} auswählen`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" →
  `Alle mit gleicher Erweiterung wählen`) and Total Commander (`WCMD.INC` `527` →
  `Alle Dateien mit gleicher Erweiterung markieren`) name this exact command, and the mask replaces their "same
  extension" because Cmdr shows the concrete one · `high`. Die Maske hängt an der Präposition `mit`, also muss nichts
  mit `{extension}` kongruieren.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Auswahl`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` („… zur Auswahl hinzuzufügen“) · `high`.
  Its siblings in that menu are verbs; this one names what the submenu holds. ❌ Not the verb `Auswählen`, which is
  `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

The warning pill in the macOS title bar plus its hover tooltip, shown while Cmdr has no Full Disk Access; a click
reopens onboarding at step 1.

- **`onboarding.fdaBadge.label` → `Kein Festplattenvollzugriff`** · Apple's own Systemeinstellungen row (see the
  full-disk-access entry above for the live-bundle evidence) · high. The badge exists so the user can match the pill to
  that row, and it sits in a fixed-height title bar, so the shortest accurate form wins.
- **`onboarding.stepAi.bannerTitle.denied` carries the SAME English string** ("No full disk access"), so `i18n-terms`
  holds the two identical. It was moved from `Kein vollständiger Festplattenzugriff` onto the pane name in the same
  pass. ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Kein Festplattenvollzugriff. Den Schritt zum Festplattenvollzugriff in der Einführung öffnen.`), which is what
  satisfies `i18n-aria` (WCAG 2.5.3). The second half puts the term in a `zum`-dative, which would NOT contain the
  nominative label, so the containment rides on the opening sentence. ❌ Re-wording the label alone breaks it.
- **Tooltip terms**: drive → `Laufwerk` (the settled term, as in `onboarding.stepOptional.indexing.benefit1`), cloud
  folders → `Cloud-Ordner`, "files macOS keeps to itself" → `Dateien …, die macOS für sich behält` (plain, ❌ never a
  macOS feature name), "Click to …" → the `du`-imperative `Klicke, um …` (as in
  `fileExplorer.network.browser.tooltip.doubleClickToConnect`), since the tooltip already addresses `du`.
- **Name vs prose, the boundary**: `Festplattenvollzugriff` where a string NAMES the setting; the FDA step's running
  prose (`onboarding.stepFda.revoked.noAccess` and its siblings) still says `vollständiger Festplattenzugriff`, which is
  natural German for the concept the way English's lowercase "full disk access" is. Whether the prose should move onto
  the pane name too is an open question in `review-queue.md`.

## The trash refusal dialog (`errors.write.trashRefused.*`)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. German solves it
  with `{count} der ausgewählten Objekte`, which takes any numeral without agreement.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Technische Details`, declined to `die technischen Details` in running text), because it points at that very control.

- **locked → `geschützt`** (the settled catalog term, the Get Info checkbox), ❌ not Finder's `AXNODE1` `Gesperrt`:
  `geschützt` is what every other `errors.write.*` string in this dialog already says · high.
- **"delete them permanently" → `endgültig löschen`** · matches `commands.fileDeletePermanently.label`
  (`Endgültig löschen`), so the suggestion names the command the user will run · high.
- **badge (the title-bar pill) → `der Hinweis`** · tentative. ❌ Not `Statussymbol` (the catalog reserves that for the
  small overlay marker on a file row) and ❌ not `Abzeichen` (Microsoft's achievement sense, already rejected in this
  termbase). `Hinweis` reads as a plain notice, which is what the pill is. title bar → `Titelleiste` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in German `„…“` quotes. ❌ Re-word one without the
  other and the sentence points at a badge that reads differently.
- "somewhere macOS keeps to itself" → `an einem Ort, den macOS für sich behält`, reusing the wording already settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## Der Online-Warnhinweis im Löschdialog (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Ist ein ausgewähltes Objekt in einem Cloud-Ordner nur online verfügbar, würde der Papierkorb es erst herunterladen. Cmdr
öffnet deshalb den Dialog zum endgültigen Löschen und erklärt das im Banner oben. Zwei Banner-Varianten: eine für eine
gemischte Auswahl, eine für eine Auswahl, die komplett nur online verfügbar ist. Sie unterscheiden sich nur im ersten
Satz und darin, welche Auswege sie anbieten können. Der dritte Schlüssel ist die Zeile, die erscheint, wenn Cmdr einen
Klick zurückgibt.

- **`.cloudOnlineOnlyMixedWarning`** · `nur online verfügbar` ist die Wendung, die der Finder für eine ausgelagerte
  Datei benutzt; `Cloud-Dienst` und `Papierkorb` aus `terms.json` (beide macOS Finder) · medium.
- **`.cloudOnlineOnlyAllWarning`** · gleicher Text, nur „Alles, was du ausgewählt hast“ statt „Ein Teil deiner Auswahl“,
  und ohne den Ausweg „abwählen“: bei komplett ausgelagerter Auswahl bliebe nichts übrig · medium.
- **`.cloudOnlineOnlyHandedBack`** · die Zeile über dem Knopf, nachdem ein Klick bewusst nicht ausgeführt wurde.
  Sachlich, ohne Entschuldigung · medium.
- **Vier Tatsachen müssen stehen bleiben**: (1) der Papierkorb würde die Dateien herunterladen, (2) Cmdr bietet deshalb
  nur das Löschen der GESAMTEN Auswahl an, (3) im Papierkorb liegt danach KEINE Kopie, der Dienst hat aber eine eigene
  (❌ nicht zu „ist ja trotzdem im Papierkorb“ abschwächen), (4) die Auswege, die das Banner nennt.
- **Die beiden `<strong>`-Spannen bleiben**, auf „zuerst herunterladen“ und auf dem Verb „löschen“. Und `„Löschen“` in
  Anführungszeichen ist die Beschriftung des Knopfs: immer derselbe Wortlaut wie `fileOperations.delete.confirmDelete`.

## Wenn der Server die Freigabe gar nicht kennt (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Der eine Fall, in dem ein neuer Versuch nichts bringt: Der Server antwortet klar, dass es keine Freigabe dieses Namens
gibt. Deshalb steht neben diesem Hinweis keine Taste, und der Ton darf nichts Vorübergehendes andeuten (kein „gerade“,
kein „noch einmal versuchen“) — genau darin unterscheidet er sich von seinen Geschwistern.

- **„the server says it has no share by that name“ →
  `weil der Server sagt, dass es dort keine Freigabe mit diesem Namen gibt`** · NetAuthAgent `EINFO_NO_SHARE` („Die
  Freigabe „%@“ existiert nicht auf dem Server.“, LIVE-Bundle, macOS 26.6.2, 25G83, 2026-09-17) und der Katalog
  (`errors.mount.shareNotFound`) · `high`. Der Satzrahmen bleibt `Die direkte Verbindung zu X kam nicht zustande` aus
  dem Nachbarn `fileExplorer.network.osMountFallback.message`.
- **„This one won't sort itself out“ → `Das erledigt sich nicht von selbst`** · kein Beleg in der Sammlung, aber die
  gängige deutsche Wendung dafür · `high`. Sie trägt genau das, was diesen Hinweis von den anderen trennt: Warten hilft
  nicht.
- **„may have been renamed or removed“ → `wurde vielleicht umbenannt oder … entfernt`** · wortgleich mit
  `errors.write.destinationNotFound.suggestion` · `high`.
- **„a lot slower“ → `deutlich langsamer`** · der Nachbar nennt die Faktoren (`4-mal`, `100-mal`); hier sagt das
  Englische nur „viel“, also bleibt auch das Deutsche unbeziffert · `high`.
- **Im Toast verweist `sie` auf die Freigabe, nie auf `{server}`** · der Server bekommt in dieser Familie nie ein
  Pronomen (§ Warum eine Freigabe nicht eingebunden wird) · `high`. Deshalb steht dort die bestimmte Form
  `diese Freigabe` (sie trägt den Bezug für das `sie`) statt der verneinten `keine Freigabe mit diesem Namen`, die im
  längeren Hinweis steht, wo kein Pronomen folgt. Das Satzende `daher bleibt sie bei der Systemverbindung` folgt
  `fileExplorer.pane.directConnectionUnreachableToast` und seinen zwei Geschwistern.

## Namen, die gleich aussehen (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.title`, `errors.listing.ambiguousName.explanation`, `errors.listing.ambiguousName.suggestion`, `errors.volume.ambiguousName`)

Eine SMB-Freigabe kann zwei Namen halten, die auf dem Bildschirm identisch aussehen (`é` als ein Zeichen oder als `e`
plus Akzent, oder nur Groß-/Kleinschreibung verschieden).

- **item → `Objekt`, auch hier** · Finder `PE82` („Das Objekt „^0“ kann nicht kopiert werden, da es denselben Namen hat
  wie ein anderes Objekt auf dem Ziel …“) · `high`. Die erste Fassung dieser Familie sagte `Element`; das ist Microsofts
  Wort und widerspricht der Termbase.
- **„the server spells them differently“ → `der Server schreibt sie unterschiedlich`**, im längeren Fehlertext **„stores
  them spelled differently“ → `speichert sie in unterschiedlicher Schreibweise`** · kein Beleg im Stapel benennt
  Unicode-Normalisierung; beide Formen sind schlichtes Deutsch ohne Fachwort, wie die `@key`-Beschreibung verlangt ·
  `tentative`. ❌ Nicht `speichert sie unterschiedlich geschrieben` (doppeltes Partizip, holprig).
- **„tell upper and lower case apart“ → `zwischen Groß- und Kleinschreibung unterscheiden`** · Finder („… und dieses bei
  Dateinamen nicht zwischen Groß- und Kleinschreibung unterscheidet.“) · `high`. Das `zwischen` gehört dazu.
- **Der Tastenname im Fließtext steht in `„…“`**: `„Überschreiben“ ersetzt das Objekt, das schon da ist.` Der Tastenname
  ist wortgleich mit `fileOperations.transferProgress.conflictOverwrite`, das Verb `ersetzen` folgt der Termbase-Regel
  `replace → ersetzt`. „the one that's there“ wird ein Relativsatz, weil `das vorhandene Objekt` steifer klingt ·
  `high`.
- **`{path}` im Rohtext steht in `„{path}“`**, nicht in ASCII-`"`, wie die übrigen sechs `„{path}“` in `errors.json` ·
  `high`.

## Der Schalter „Cloud-KI erlauben“ und die Zustände „Cloud-KI ist deaktiviert“ (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Ein Datenschutzschalter: Cmdr sendet nichts an einen Cloud-KI-Dienst, bevor er an ist. Der Ton ist ruhig und sagt nie
mehr, als Cmdr tut. Der Referenzstapel lag auf dem Übersetzungsrechner nicht vor; jede Wahl stützt sich auf bereits in
dieser Termbase belegte Begriffe und auf den Katalog.

- **Allow cloud AI (Schaltername) → `Cloud-KI erlauben`** · `Cloud-KI` ist die Option `settings.ai.provider.opt.cloud`,
  `erlauben` die Regel „`allow` → `erlauben`, auch in den Einstellungen“ weiter oben, und die Form `<Objekt> erlauben`
  folgt `settings.fileOperations.allowFileExtensionChanges.label` · `high`. Zitiert wird der Name mit `„…“`
  (`settings.ai.cloudConsent.lockedHint`). Wo das Englische „Allow cloud AI“ als Satzverb benutzt
  (`askCmdr.gate.cloudOff.body`, `settings.askCmdr.cloudOffHint`), steht `Erlaube … Cloud-KI`: dieselben Wörter,
  gebeugt, ohne Anführungszeichen.
- **cloud AI (Fließtext) → `Cloud-KI`, Pronomen `sie`** (die KI): `Erlaube sie unter Einstellungen > KI` · `high`.
- **„X is off“ → `X ist deaktiviert`** · wie `servers.hub.discoveryOff` und `settings.mediaIndex.clip.offButInstalled`;
  passt zu `turn on/off → aktivieren/deaktivieren` · `high`.
- **Turn on Ask Cmdr (Schaltfläche) → `Ask Cmdr aktivieren`** · Infinitiv-Schaltfläche wie `Alle erlauben` · `high`.
- **Open AI settings → `KI-Einstellungen öffnen`** · wie `commands.appSettings.label` („Einstellungen öffnen“) · `high`.
- **side panel → `Seitenbereich`** · `tentative`: bewusst nicht `Seitenleiste`, das in macOS die Finder-Seitenleiste
  meint.
- **custom endpoints → `eigene Endpunkte`** · `Eigenes` für „Custom“ (`settings.network.timeoutMode.opt.custom`),
  `Endpunkt` aus `onboarding.cloudSetup.hint.azureEndpoint` · `high`.
- **Cmdr's own servers → `Die eigenen Server von Cmdr`** · analytischer Genitiv statt `Cmdrs` (§ Ask Cmdr) · `high`.
- **Ask Cmdr chats → `Ask-Cmdr-Chats`** · durchgekoppelt wie `Ask-Cmdr-Einstellungen` in
  `ai.cloudConsent.askCmdr.memory` · `high`.
- `settings.askCmdr.enabled.label` ist nur noch „Ask Cmdr“ (der Produktname auf dem Schalter) und trägt eine
  `sameAsSourceJustification`.

## Vollbildmodus und esc-Taste (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

Der einmalige Hinweis, nachdem die esc-Taste das Hauptfenster aus dem Vollbildmodus geholt hat, plus der passende
Schalter unter Einstellungen > Erweitert > Eingabe.

- **full screen → `Vollbildmodus`** · macOS AppKit `MenuCommands` („Enter Full Screen“ → „Vollbildmodus“, „Exit Full
  Screen“ → „Vollbildmodus aus“, „Make Window Full Screen“ → „Vollbildmodus für Fenster“), Finder `MenuBar` `300944`,
  `LocalizableMerged` `FV20`/`FV21` (Referenz-Stapel `de/macOS/`) · `high`. Das kurze „Vollbild“ nimmt Apple nur für die
  Fenster-Kachel; im Satz steht der Modus.
- **Escape (die Taste) → `esc-Taste`**, klein wie Apples Tastenkappe · macOS AppKit `FunctionKeyNames` („Escape“ →
  „esc-Taste“) · `high`. Abgrenzung: `shortcuts.section.pressEscToClear` schreibt „ESC“ (älterer Wert, nicht angefasst).
- **Exit full screen on Escape (Schalter) → `Vollbildmodus mit esc-Taste beenden`** · Label ohne Artikel wie „Mit Server
  verbinden“; `beenden` folgt Apples „Vollbildmodus aus“ als Aktion · `high`. Toast-Schalter und Settings-Schalter
  tragen denselben `sourceHash`, also denselben Wert.
- **„X took Cmdr out of full screen“ → `… hat Cmdr aus dem Vollbildmodus geholt`** · umgangssprachlich und ohne Schuld,
  wie das Englische; die Settings-Beschreibung nimmt dasselbe Verb („holt das Hauptfenster aus dem Vollbildmodus“) ·
  `high`.
- **„in a dialog or menu … only closes that“ → `schließt sie weiterhin nur den Dialog oder das Menü`** · `Dialog` (m)
  und `Menü` (n) vertragen kein gemeinsames Pronomen (style.md § Zwei Nomen mit verschiedenem Genus), also das Nomen
  wiederholen · `high`.
- **„You'll only see this once.“ → `Du siehst diesen Hinweis nur einmal.`** · `Hinweis` ist Cmdrs Toast (Zeile „toast“
  oben) · `high`.

## Geänderte Originale nach dem Bewegen (`transfer.changedDuringMove`, 2026-09-25)

- **„changed during the move“ → `wurde/wurden während des Bewegens geändert`** · Finder `PE56` „… da mindestens ein
  Objekt während des Brennens geändert wurde“ (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle,
  2026-09-25); `während des Bewegens` und `Quellordner` wörtlich aus dem Geschwister `transfer.appearedDuringMove`, weil
  beide Sätze im selben Toast stehen · `high`.
- **„stays in {folderName}“ → `bleibt/bleiben in {folderName}`** · Verb in eigenem Plural-Block wie beim Geschwister ·
  `high`.

## Platzhalterzeilen in „Öffnen mit“ und „Teilen“ (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Platzhalterzeilen in „Öffnen mit“ und „Teilen“ (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- **„Finding apps…“ → `Apps werden gesucht …`** · Fortschrittszeile, also Leerzeichen vor `…` (style.md § Ellipsis);
  macOS setzt es auch im Menü so (SystemSettings `Fetching Menu Item` = „Laden …“) · `high`.
- **„share options“ → `Optionen zum Teilen`** · das Untermenü heißt `Teilen` (Finder), und `Freigabe` ist hier falsch:
  die gehört zur Netzwerkfreigabe (style.md § share). Ein Kompositum `Teilen-Optionen` liest sich holprig · `tentative`.
- **„No share options“ → `Keine Optionen zum Teilen`** · Muster wie macOS' leeres Dienste-Menü („No Services Apply“ =
  „Keine Dienste verfügbar“) · `high`.
