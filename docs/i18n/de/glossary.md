# de glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/de/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

- crash report → Absturzbericht · macOS (Apple uses "abgestürzt" / "unerwartet beendet" for crashes; "Absturzbericht" is
  the standard Apple/MS rendering of "crash report") · high
- crash (verb) → abstürzen; "quit unexpectedly" → "unerwartet beendet" · macOS Finder ("… wurde unerwartet beendet") ·
  high
- feedback → Feedback (kept; the loanword is standard in DE UI, macOS/MS both use it) · high
- report ID → Berichts-ID · compound of Bericht (report) + ID; standard DE compound formation · high
- note (the free-text the user writes into a report) → Notiz · MS terminology (`note`, Noun → `Notiz`, AUT/DEU/CHE/LUX)
  and the catalog itself (`errorReporter.dialog.noteLabel` = "Notiz hinzufügen (optional)") · high. ❌ Not "Hinweis":
  that is MS's other `note` sense, the editorial "Note:" callout, and it reads as advice FROM the app, not text BY the
  user.
- "Add to X" (a button/menu item that puts something into X) → "Zu X hinzufügen" · macOS Finder `de` ("Zum Dock
  hinzufügen" `300772.title`, "Zur Seitenleiste hinzufügen" `300790.title`, "Tags hinzufügen" `TL34`) · high
- view / see (a document, a list, a report) → ansehen · the catalog throughout ("Vollständiges Änderungsprotokoll
  ansehen", "Dateien ansehen"); macOS's "anzeigen" is the SHOW sense (make something visible), so keep the two apart ·
  high
- email → E-Mail (-Adresse for the address) · macOS Mail, MS terminology · high
- attach (an email address to a report) → anhängen · the catalog's settled pair (`common.attachEmail` = "Meine
  E-Mail-Adresse … anhängen", `settings.updates.attachEmailToReports.label` = "Meine E-Mail standardmäßig an Berichte
  anhängen") · high
- dismiss (button closing a dialog) → Schließen · macOS uses "Schließen" / "Ignorieren"; "Schließen" fits a
  close-without-action button · high
- send → senden · macOS Mail ("Senden") · high
- permission(s) → Berechtigung / Zugriffsrechte · macOS Finder uses both ("Berechtigung", "Zugriffsrechte"; the Get Info
  pane is "Teilen & Zugriffsrechte"); MS terminology "Berechtigung" · high. Usage split in the catalog: "Berechtigung"
  for the abstract OS-grant sense ("Cmdr hat keine Berechtigung …"), "Zugriffsrechte" for a file/folder's ACL ("die
  Zugriffsrechte des Ordners prüfen") — both correct, keep the sense split
- owner (file owner) → Eigentümer · macOS Finder Get-Info "Teilen & Zugriffsrechte" pane · high
- read-only → schreibgeschützt · MS terminology + standard DE; macOS "schreibgeschützt" for write-protected media · high
- write-protection switch → Schreibschutzschalter · standard DE compound (Schreibschutz + Schalter) · high
- locked (a protected file, macOS "Locked" checkbox) → geschützt · macOS Finder ("Geschützt" is the Get Info checkbox;
  "Das Objekt ist … geschützt …") · high
- Get Info (macOS context-menu item / pane) → Informationen · macOS Finder ("„Informationen“") · high
- timed out / timeout → Zeitüberschreitung · standard DE + MS terminology · high
- symbolic link / symlink → symbolische Verknüpfung; the loop term stays "Symlink-Schleife" · MS terminology
  ("symbolische Verknüpfung", AUT/DEU/CHE); "Symlink" kept in the short title for compactness · high
- mount (verb, a volume) → einbinden; unmount → aushängen; force-unmount → zwangsweise aushängen · macOS Finder
  ("eingebunden"/"einbinden"); MS "Einbinden" · high
- eject → auswerfen · macOS Finder ("Auswerfen") · high
- handle (open file handle, OS sense) → Handle (kept; no settled DE UI term, technical context only) · tentative
- quota → Kontingent · MS terminology ("Kontingent" for disk quota) · high
- attribute (file attribute / metadata) → Attribut · MS terminology, standard DE · high
- First Aid (Disk Utility feature) → Erste Hilfe · macOS Festplattendienstprogramm ("Erste Hilfe"); kept untranslated
  only where the en source's @key flags "Disk Utility"/"First Aid" as do-not-translate feature names — in body prose use
  "Erste Hilfe" · high
- Disk Utility → Festplattendienstprogramm · macOS app name; used in body prose (the en @key allows the localized macOS
  pane name) · high
- Activity Monitor → Aktivitätsanzeige · macOS app name · high
- repo / repository (git) → Repo / Repository · kept (git domain); "git" stays verbatim per do-not-translate · high
- worktree (git) → Worktree · kept (git term, do-not-translate) · high
- working tree (git) → Arbeitsbaum · DE git docs render "working tree" as "Arbeitsbaum" · high
- cloud provider → Cloud-Anbieter; cloud mount → Cloud-Mount · standard DE compound formation (loanword "Cloud" kept, as
  in macOS "iCloud") · high
- connect → verbinden; "Connect to server" → "Mit Server verbinden"; "Connecting…" → "Verbindung wird hergestellt …" ·
  macOS Finder ("Mit Server verbinden", button "Verbinden", "Serveradresse") · high
- disconnect → trennen; "Disconnected from X" → "Verbindung zu X getrennt" · macOS Finder · high
- reconnect → Verbindung wiederherstellen; "Reconnecting to server…" → "Verbindung zum Server wird wiederhergestellt …"
  · derived from macOS "Verbindung … herstellen" · high
- sign in / log in → anmelden (verb), Anmeldung (noun); "Sign in" button → "Anmelden" · macOS Finder ("Anmelden …", "Die
  Anmeldung beim Server …") · high
- credentials → Anmeldedaten · MS terminology lists "Anmeldeinfos"; "Anmeldedaten" is the more standard, natural DE UI
  term — use consistently · high
- guest → Gast · macOS ("Gast") · high
- host → Host (plural Hosts) · MS terminology (Host, masc., AUT/DEU/CHE/LUX) · high
- refresh / reload → aktualisieren · macOS Finder ("Aktualisieren") · high
- favorites → Favoriten · macOS Finder ("Favoriten", "Keine Favoriten") · high
- pinned (tab) → fixiert; "pin a tab" → "Tab fixieren" · macOS ("Tab fixieren", "fixierter Tab") · high
- remove → entfernen · macOS ("Entfernen") · high
- Keychain → Schlüsselbund (store) / Schlüsselbundverwaltung (the "Keychain Access" app) · macOS German · high ·
  localized Apple feature name, not a verbatim brand (per i18n-translation.md § Term-choice principles, same rule as
  Quick Look). Apple ships the credential store as "Schlüsselbund" and the app as "Schlüsselbundverwaltung" in German
  macOS, so Cmdr shows those. The local-store fallback string uses "System-Schlüsselbund" for the generic OS keyring
- file list → Dateiliste · style guide (listing → Dateiliste); used for the file-list aria label · high
- file extension → Endung; tight "Ext" column header → "Erw." (abbrev. of Erweiterung) · standard DE ("Endung" common
  term; "Erw." the compact column form) · high
- disk usage → Speicherbelegung · standard DE compound (Speicher + Belegung); macOS uses "Speicherplatz" for free space
  · high
- on disk (physical size) → Auf dem Datenträger · standard DE; pairs with "Inhalt" (content size) · high
- home folder → Persönlicher Ordner · macOS "persönlicher Ordner"/"Benutzerordner" framing; reads naturally for the home
  location · high
- "Volume"/"Volumes" (mounted disk) → Volume/Volumes (kept verbatim) · macOS keeps "Volume" (style guide volume→Volume)
  · high
- dir (short for directory, tight status-bar slot) → ORD (short for Ordner) · abbreviation matching the EN "DIR" tight
  slot; no canonical source · tentative
- timeout (network status cell) → Zeitüberschreitung · aligns with the settled timeout→Zeitüberschreitung term above
  (not the MS "Timeout" loanword) · high
- notification → Benachrichtigung; macOS system notification → macOS-Mitteilung (Apple's word for Notification Center
  items is "Mitteilung") · MS terminology (Benachrichtigung), macOS (Mitteilungen) · high
- enable / turn on → aktivieren; disable / turn off → deaktivieren · macOS Finder ("… aktivieren"), MS terminology ·
  high
- show (a UI element/column) → anzeigen; show/hide a panel → einblenden/ausblenden · macOS Finder ("Seitenleiste
  einblenden", "Anzeigen") · high
- restart → neu starten / Neustart ("Neustart erforderlich") · MS terminology · high
- reset → zurücksetzen ("auf Standard zurücksetzen") · MS terminology · high
- default → Standard; system default → Systemstandard · macOS, MS terminology · high
- custom (picker option / field) → eigen ("Eigenes …", "Eigene Zeitüberschreitung") · macOS "Eigene …" pattern · high
- threshold → Schwelle ("Warnschwelle") · MS terminology · high
- port → Port; "in use" (port) → belegt · MS terminology · high
- cache → zwischenspeichern (verb) / Cache (noun, "Cache-Dauer") · MS terminology · high
- provider → Anbieter · MS terminology · high
- service → Dienst · MS terminology · high
- word wrap → Zeilenumbruch · MS terminology · high
- tint → Farbton ("einfärben" for the verb "tint panes") · MS terminology · high
- warning → Warnung · MS terminology · high
- toast (Cmdr's transient notification) → Hinweis (reserve "Mitteilung" for the macOS-notification sense) · no direct
  source · tentative
- view mode: Full → Voll, Brief → Kompakt (mode → "Kompaktmodus") · no direct source; "Kompakt" matches macOS density
  wording · tentative
- content size (logical) → Inhalt / Inhaltsgröße (pairs with on-disk "Auf dem Datenträger") · style guide; pairs with
  the on-disk term above · high
- stale (index) → veraltet · macOS/MS common usage · high
- pane (Cmdr file list) → Bereich (style guide pane→Bereich); "tint X panes" → "X-Bereiche einfärben" · style guide ·
  high
- color names: Orange, Cyan, Indigo stay identical in DE; Amber→Bernstein, Lime→Limette, Teal→Petrol, Purple→Violett,
  Pink→Rosa · standard DE color vocabulary · high
- command palette → Befehlspalette · MS terminology ("Befehlspalette") · high
- clipboard → Zwischenablage; copy to clipboard → "In die Zwischenablage kopieren"; cut → ausschneiden; paste →
  einsetzen · macOS (Finder/AppKit: "Zwischenablage", "Ausschneiden", "Einsetzen") · high
- pattern (match pattern) → Muster · MS terminology ("Muster"); macOS Finder ("Muster einfügen") · high
- regular expression → regulärer Ausdruck (UI chip stays "Regex", brand/do-not-translate) · MS terminology · high
- query (search text) → Anfrage (noun); "query" verb → abfragen · MS terminology · high
- scope (search-in folders) → "Suchen in" for the filter label; the recent-search summary uses "Bereich: {scope}" ·
  derived (Suchen in = the action; Bereich for the abstract scope, aligning with pane→Bereich) · high
- zoom in / zoom out → Vergrößern / Verkleinern; "Zoom to N%" → "Auf N % zoomen"; zoom (noun) → Zoom · macOS
  ("vergrößern"/"verkleinern"), Nautilus ("Ansicht vergrößern") · high
- context menu → Kontextmenü · macOS Finder ("Kontextmenü einblenden") · high
- parent folder → übergeordneter Ordner · macOS Finder ("Übergeordneter Ordner") · high
- hidden files → verborgene Dateien · standard DE; macOS "ausgeblendet" also seen, "verborgen" reads cleaner for the
  dotfile sense · high
- overwrite → überschreiben; "Overwrite all"→"Alle überschreiben", "Overwrite all smaller/older"→"Alle kleineren/älteren
  überschreiben" · Double Commander ("&Überschreiben", "Alle überschreiben", "Alle größeren überschreiben"), MS
  terminology · high
- skip → überspringen; "Skip all"→"Alle überspringen" · macOS Finder copy dialog ("Überspringen"), Double Commander,
  Nautilus ("Überspringen") · high
- merge (folders) → zusammenführen ("wird mit einem bestehenden Ordner zusammengeführt") · Nautilus
  ("\_Zusammenführen"), MS terminology · high
- conflict → Konflikt; "Checking for conflicts"→"Konflikte werden geprüft" · MS terminology (Konflikt, AUT/DEU/CHE) ·
  high
- destination → Ziel (Zielvolume, Zielpfad, Zielordner); source → Quelle; target (symlink/overwrite target) → Ziel · MS
  terminology (Ziel, Quelle) · high
- rename → umbenennen; "Rename all"→"Alle umbenennen" · macOS Finder, Double Commander · high
- rollback → Rollback (kept; verb "Rollback läuft …" for "Rolling back") · MS terminology keeps "Rollback"; no settled
  DE UI translation, loanword standard in dev/file-op context · high
- retry / try again → erneut versuchen ("Erneut versuchen") · macOS ("Try Again"→"Erneut versuchen") · high
- scanning (transfer scan phase) → durchsuchen ("Wird durchsucht"); "Verifying before copy"→"Prüfung vor dem Kopieren" ·
  derived; Double Commander uses "Suche" but "durchsuchen" reads clearer for a file-tree walk, and it matches the
  drive-index "durchsuchen/Durchlauf" choice so "scan" renders one way everywhere · high
- hardlink / hardlinked → Hardlink (Hardlink-Dateien) · loanword kept; MS "fester Link" exists but "Hardlink" is the
  common DE dev term, consistent with "Symlink" · high
- existing / new (conflict comparison labels) → Bestehend / Neu · standard DE; pairs naturally for the side-by-side
  conflict rows · high
- permanently (delete) → endgültig ("Endgültig löschen", "endgültig gelöscht") · der Katalog durchgehend
  (`menu.file.deletePermanently`, `commands.fileDeletePermanently.label`,
  `fileExplorer.functionKeyBar.deletePermanentlyAction`/`.permanentlyLabel`, `errors.mutation.trashNotSupported`) ·
  high. ❌ Nicht "dauerhaft" für das Löschen: das Wort ist im Katalog für den ANDEREN Sinn von "permanently" reserviert,
  nämlich "for good" bei einer Einstellung ("die Warnung dauerhaft unterdrücken",
  `settings.fileViewer.suppressBinaryWarning.description`). Der Löschdialog sagte als Einziger "dauerhaft gelöscht"
  direkt neben der Taste "Endgültig löschen".
- under cursor → unter dem Cursor · standard DE; "Cursor" kept (common DE UI term) · high
- full disk access (macOS permission) → vollständiger Festplattenzugriff; System Settings pane → "Vollständiger
  Festplattenzugriff", "Privacy & Security"→"Datenschutz & Sicherheit" · macOS SystemSettings (Festplattenzugriff; de
  macOS PRIVACY_SECTION="Datenschutz & Sicherheit") · high
- Quit & Reopen (macOS relaunch button) → Beenden & erneut öffnen · macOS relaunch-prompt wording (beenden + erneut
  öffnen) · high
- macOS folder names: Applications→Programme, Desktop→Schreibtisch, Documents→Dokumente, Downloads→Downloads · macOS
  standard folder names · high
- network share → Netzwerkfreigabe; "Connect directly"→"Direkt verbinden" · style guide (share→Freigabe), derived for
  the direct-connect action · high
- select all → Alles auswählen; deselect all → Auswahl aufheben; deselect (verb) → abwählen · macOS ("Alles auswählen",
  "Auswahl aufheben") · high
- offline (make available offline) → "Offline verfügbar machen" · MS ("offline" kept); standard DE phrasing · high
- onboarding → Einführung (wizard → Einführungsassistent) · standard DE UI rendering of guided first-run · high
- view modes (commands): Brief → Kompaktansicht, Full → Vollansicht (aligns with view mode glossary entry above:
  Voll/Kompakt) · derived · high
- relative-time abbreviations (recent-search tooltips): "{count}m/h/d/w/mo/y ago" → "vor {count}
  Min./Std./T./Wo./Mon./J." (DE puts "vor" first; abbreviated to stay terse) · standard DE · high
- "boring folders" (playful) → "langweilige Ordner" (kept the playful tone per @key) · product voice · high
- comparator (size/date filter) → Vergleichsoperator · MS terminology · high
- clipboard → Zwischenablage · macOS AppKit ("Zwischenablage") · high
- "Do nothing" (radio/menu option for the no-op behavior) → Nichts tun · standard DE; Double Commander renders the terse
  menu form as "Nichts", but "Nichts tun" is the idiomatic full option label · high
- "paste clipboard content as a file" (settings label) → "Inhalt der Zwischenablage als Datei einsetzen" · paste →
  einsetzen (settled, macOS "Einsetzen") + clipboard → Zwischenablage; the paste-as-file toast reads "{Bild/PDF/Text}
  aus der Zwischenablage als {filename} eingesetzt" · high
- PDF (as a document/file kind, needing an article) → neuter "ein PDF" (das PDF) · macOS renders it
  "PDF-Dokument"/"PDF-Dokumente" (das Dokument, neuter), so PDF standalone takes das/ein; in the toast select the branch
  stays article-less ("PDF aus der Zwischenablage …") to sidestep gender entirely · high
- viewer (file viewer window) → Vorschau; "File viewer" → Dateivorschau; window title suffix keeps "| Vorschau" · style
  guide (viewer→Vorschau); MS "Zuschauer" is the wrong sense (a person watching), rejected · high
- Quick Look → Übersicht · macOS German · high. The localized Apple feature name — Apple's German Finder uses
  "Übersicht" for Quick Look, so the user sees that, not the English term. Applies wherever the macOS Quick Look preview
  is named (the `commands.fileQuickLook.mac.label` palette label, the binary-warning banners, the space-key hint).
  Distinct from Cmdr's own file viewer (Vorschau)
- encoding (text/character encoding) → Codierung; full compound → Zeichencodierung · MS terminology ("Codierung",
  AUT/DEU/CHE/LUX) · high
- detected (auto-detected encoding) → erkannt · MS terminology (detect→erkennen) · high
- line (text line) → Zeile (plural Zeilen); line number → Zeilennummer · MS terminology ("Zeilennummer"); macOS "Zeile";
  NOT "Linie" (geometric) · high
- match (search result) → Treffer; "No matches" → "Keine Treffer" · GNOME Nautilus ("Keine Treffer") · high
- case sensitive → Groß-/Kleinschreibung beachten · MS terminology (case-sensitive, AUT/DEU/CHE/LUX) · high
- streaming (viewer streaming mode) → Streaming; "streaming mode" → Streaming-Modus · MS terminology (Streaming, kept) ·
  high
- memory (RAM) → Speicher · MS terminology (memory→Speicher) · high
- tail (auto-follow a growing file) → Folgen (verb folgen/verfolgen); "tail mode" → Folgemodus · no canonical source;
  conveys auto-follow without the Unix `tail` jargon · tentative
- reload (file changed on disk) → neu laden ("Neu laden") · standard DE · high
- save (selection to a file) → sichern ("Auswahl sichern", "Als Datei sichern …") · macOS Finder ("Sichern", "Sichern
  unter …") · high
- license → Lizenz; license key → Lizenzschlüssel; license type → Lizenztyp · MS terminology (license→Lizenz) · high
- activate (a license) → aktivieren · macOS, MS terminology · high
- Personal (license tier) → Privat ("Privat (kostenlos)", "Privatlizenz"); private use → private Nutzung · standard DE;
  tier-label translation kept consistent across licensing strings · high
- Commercial (license tier) → Gewerblich ("Gewerbliche Lizenz", "Gewerbliches Abo", "Gewerblich unbefristet") · standard
  DE; consistent across licensing strings · high
- perpetual (license) → unbefristet · standard DE (one-time/non-expiring license) · high
- subscription → Abo (das Abo, "gewerbliches Abo") · standard DE short form of Abonnement · high
- organization (licensed org) → Organisation · MS terminology · high
- endpoint (API) → Endpunkt · MS terminology · high
- API key → API-Schlüssel · standard DE compound (API kept) · high
- model (AI model) → Modell · MS terminology · high
- "Example:" (placeholder prefix) → "Beispiel:" · standard DE · high
- startup disk (macOS boot volume) → Startvolume · macOS Finder Get-Info pane ("Startvolume:", value field key
  f82-Dg-tGh) · high
- What's new (post-update dialog) → Neuheiten ("Neuheiten in Cmdr") · MS terminology (Neuheiten); macOS "Neuheiten" ·
  high
- changelog → Änderungsprotokoll · MS terminology (Änderungsprotokoll, AUT/DEU/CHE) · high
- command (palette item) → Befehl (plural Befehle); "Search commands" → "Befehle suchen" · macOS/MS standard ("Befehl");
  aligns with command palette → Befehlspalette · high
- "Go to path" / "Go to" (Cmd-G dialog) → "Zu Pfad gehen" · derived (macOS "Gehe zu …" / "Gehe zu Ordner"; "Zu Pfad
  gehen" keeps the path noun the dialog needs) · high
- recent (recently used/visited) → Letzte ("Letzte Pfade", "Letzte {mode}-Suche"); "Recent" command-palette group →
  "Zuletzt verwendet" · macOS Finder ("Zuletzt benutzt"/"Letzte") · high
- complete (operation done) → abgeschlossen ("Löschen abgeschlossen", "Kopieren abgeschlossen") · standard DE; MS
  terminology · high
- skip (transfer) → überspringen ("übersprungen") · already in glossary; reused for transfer toasts · high
- "at the target" (transfer destination) → "am Ziel" · aligns with destination/target → Ziel · high
- onboarding (menu item label "Onboarding…") → "Einführung…" · glossary onboarding→Einführung; kept the trailing
  ellipsis to match the literal menu item · high
- low on space / running low → "der Speicherplatz wird knapp"; "Low disk space" (notification title) → "Wenig
  Speicherplatz" · standard DE; pairs with disk usage → Speicherbelegung · high
- update check error toast → "Beim Suchen nach Updates ist etwas schiefgelaufen: {message}" · style guide voice rule (no
  bare "Fehler:" label for the user-facing error toast); the calm-rephrase pattern for a raw-error-prefix string · high
- Settings sections (de): Appearance→Erscheinungsbild, Colors and formats→Farben und Formate, Zoom and density→Zoom und
  Dichte, File and folder sizes→Datei- und Ordnergrößen, Listing→Dateiliste, Behavior→Verhalten, File
  operations→Dateioperationen, File system watching→Dateisystemüberwachung, Search→Suche, AI→KI, File
  systems→Dateisysteme, SMB/Network shares→SMB-/Netzwerkfreigaben, MTP→MTP (Android/Kindle/Kameras), Git→Git,
  Viewer→Vorschau, Developer→Entwickler, MCP server→MCP-Server, Logging→Protokollierung, Updates & privacy→Updates &
  Datenschutz, Advanced→Erweitert, Keyboard shortcuts→Tastaturkurzbefehle, License→Lizenz · derived from glossary
  terms + macOS Systemeinstellungen wording · high
- error report → Fehlerbericht; "Send error report" → "Fehlerbericht senden" · macOS/MS standard rendering (Apple
  "Fehlerbericht"); the bare-"Fehler"-label voice rule targets failure toasts, not this established feature name · high
- "Couldn't X" (failure status/toast) → "X ließ sich nicht …" / "X nicht möglich" · style-guide voice rule (no bare
  "fehlgeschlagen"); the calm-rephrase pattern for register/prepare/send/save failures · high
- redact (logs) → bereinigen; "redacted" → "bereinigt"; "after redaction" → "nach Bereinigung" · standard DE for
  privacy-scrubbing log data · high
- log file / log lines → Protokolldatei / Protokollzeilen · macOS/MS (Protokoll); consistent with logging →
  Protokollierung · high
- scan (drive indexing) → durchsuchen ("Laufwerk wird durchsucht …"); "fresh scan" → "neuer Durchlauf"; "rescan" →
  "erneuter Durchlauf"; the index status panel and dir-size tooltip use the same verb ("Laufwerk wird durchsucht …", not
  "Scan läuft") · macOS ("durchsuchen"); "Durchlauf" reads naturally for the indexing pass, distinct from the search
  verb. ❌ Don't keep the loanword "Scan" — the EN source says "scan" everywhere (indexing.json, queryUi.json,
  settings.json), all of which map to durchsuchen/Durchlauf. EN "Indexing this drive…" (the drive-node tooltip) is the
  distinct sense → Indizierung · high
- entries (scanned files+folders) → Einträge · MS terminology (Eintrag); the index-row sense · high
- ETA "roughly {eta}" → "etwa {eta}"; "{n}s left" → "noch {n} s"; "{n}m left" → "noch {n} Min."; "Almost done" → "Fast
  fertig" · standard DE (DE puts "noch" first for remaining time) · high
- shortcut (keyboard) → Kurzbefehl (plural Kurzbefehle); "Keyboard shortcuts" → "Tastaturkurzbefehle" · macOS
  ("Kurzbefehl"); aligns with Settings-section term · high
- modifier (key) → Sondertaste · macOS/MS standard DE for ⌘/⌥/⌃/⇧ keys · high
- combo / key combination → Kombination · standard DE (short for Tastenkombination); used in shortcut-conflict warnings
  · high
- "Force Quit" (macOS) → Sofort beenden · macOS AppKit ("Force Quit %@" → "%@ sofort beenden") · high
- "Character Viewer" (macOS) → Zeichenübersicht · Apple's standard DE name for the emoji/symbols picker · high
- "Mission Control" / "Spaces" / "Spotlight" (macOS) → kept verbatim · macOS DE keeps all three untranslated (verified
  in macOS pile, 2026-06-21) · high
- "input source switching" → "Wechsel der Eingabequelle"; "app switcher" → "App-Umschalter"; "App windows" →
  "App-Fenster" · macOS Eingabequelle wording; standard DE compounds · high
- "logging out" → "das Abmelden"; "locking the screen" → "das Sperren des Bildschirms" · macOS ("Abmelden", "Bildschirm
  sperren"); nominalized to fit the mid-sentence "(…)" conflict-warning slot · high
- "screen recording" → Bildschirmaufnahme; "screenshots" → Bildschirmfotos · macOS ("Bildschirmfoto"); MS
  "Bildschirmaufnahme" · high
- USB device → USB-Gerät · standard DE compound (USB kept) · high
- udev / ptpcamerad / Terminal → kept verbatim (Linux/macOS process + app names); MTP/PTP stay verbatim per
  do-not-translate · high
- "in use by" (device held by a process) → "wird von … verwendet"; "exclusive access" → "exklusiver Zugriff" · standard
  DE · high
- preview (report preview) → Vorschau · macOS (Vorschau); distinct from the file viewer but same DE word · high
- bundle (log/report bundle) → Bündel · standard DE for a packaged set of files · high
- "Reveal in Finder" → "Im Finder zeigen"; "Show in Finder" → "Im Finder anzeigen" · macOS renders the two source verbs
  distinctly ("Finder/Reveal" → "Im Finder zeigen"; "Show in Finder" → "… im Finder anzeigen", both verified in
  `de/macOS/`, 2026-06-21). Keep the split: the `errorReporter` toast (Reveal) stays "zeigen", the
  `commands.fileShowInFinder` palette label (Show) stays "anzeigen" · high
- suggestion(s) (combobox) → Vorschläge · MS terminology (Vorschlag) · high
- toast scope: "in-app" → "in der App"; "globally"/"global shortcut" → "global"/"globaler Kurzbefehl"; "from any app" →
  "aus jeder App" · standard DE; pairs with the global-hotkey UI · high
- "jump to" (a file/download) → "springen zu" / "Zur Datei springen" · standard DE UI action · high
- "Press keys…" (shortcut capture) → "Tasten drücken …" · standard DE; ellipsis kept · high
- registered / not registered (global hotkey) → registriert / nicht registriert · MS terminology (registrieren) · high
- pause (transfer) → button "Pause" (noun, macOS NSPauseTemplate "Pause"); verb/aria "anhalten"; status "Angehalten"
  (macOS "Kopieren von „^0“ wurde angehalten", paused→angehalten) · macOS Finder + AppKit · high. macOS ships the pause
  control as the noun "Pause" but narrates the action with the verb "anhalten"; keep the visible button "Pause", use
  "anhalten" in aria/tooltip and "Angehalten" as the status chip
- resume (transfer) → fortsetzen ("Fortsetzen" button; "Kopieren fortsetzen", "Backup fortsetzen") · macOS Finder
  ("Kopieren fortsetzen", resume→fortsetzen) · high
- queue → Warteschlange; "Operation queue" (the window) → Vorgangswarteschlange · MS terminology (queue→Warteschlange,
  and the closed compound "Übertragungswarteschlange"), Double Commander ("Queue" → "Warteschlange", 18×) · high. The
  "Queue" button on the progress dialog (send-to-background + open the queue window) renders as the bare
  "Warteschlange". ❌ The old "Übertragungs-Warteschlange" is SUPERSEDED: the window lists deletes, renames, and archive
  edits too, so it took the category noun. See § Operation queue (2026-08-08)
- background / send to background (a transfer) → "im Hintergrund" (running); "keep running in the background" → "im
  Hintergrund weiterlaufen lassen" · macOS ("Synchronisierung im Hintergrund", "Drucken im Hintergrund"), MS ("im
  Hintergrund") · high. Cmdr's send-to-background action routes through the queue, so its button is "Warteschlange" and
  its toasts say "im Hintergrund"
- double-click → Doppelklick (noun) / doppelklicken (verb, du-imperative "Doppelklicke auf …") · Double Commander
  (`tfrmoptionsfilesviewscomplement.cbdblclicktoparent.caption` → "… durch Doppelklick auf den leeren Teil der
  Dateiansicht …"), macOS ("Doppelklick") · high
- navigate to (a folder/path) → zu … navigieren (verb) · macOS Finder ("Navigates the front Finder window to its
  enclosing folder" → "Navigiert im vorderen Finder-Fenster zu seinem übergeordneten Ordner"; "Navigates to a location
  …" → "Navigiert zu einem Ort …", verified in `de/macOS/Finder/Localizable.json`, 2026-06-26) · high. Used for the
  breadcrumb tooltip ("zu {path} navigieren") and the double-click hint body ("Das navigiert zum übergeordneten
  Ordner"). The settings switch's label/description use DC's "wechseln" (below) to match the source's "go up a folder"
  phrasing
- pane background → Bereichshintergrund (pane→Bereich, glossary); the empty backdrop of a file pane · KDE Dolphin
  ("double clicking view background" → "Doppelklick auf den Hintergrund der Ansicht"), Double Commander ("empty part of
  file view" → "leeren Teil der Dateiansicht") · high
- empty space (in/around a file list) → leere Fläche; "empty space around the file list" → "leere Fläche rund um die
  Dateiliste" · Double Commander ("empty part of file view" → "leeren Teil der Dateiansicht"; "Fläche" reads more
  natural than "Teil" for the empty backdrop sense) · high
- row (list/table row) → Zeile; "file row" → Dateizeile · Microsoft terminology (row → Zeile, AUT/DEU/CHE/LUX), Double
  Commander ("one per row" → "eins pro Zeile") · high
- "go up a folder" / "changing to parent folder" (the DC two-pane feature verb) → in den übergeordneten Ordner wechseln
  · Double Commander (the exact same setting: "Enable changing to parent folder when double-clicking on empty part of
  file view" → "Wechsel in das übergeordnete Verzeichnis durch Doppelklick auf den leeren Teil der Dateiansicht
  aktivieren"; Cmdr keeps macOS "Ordner" over DC's "Verzeichnis") · high
- "What just happened?" (one-time hint title) → Was ist gerade passiert? · standard DE friendly question; matches Cmdr's
  warm du-voice · high
- "I like it" / "Don''t like it?" (hint buttons) → Gefällt mir / Gefällt dir das nicht? · standard DE; "Gefällt mir" is
  Apple/social-standard for "like" · high
- "Never do this again" (turn the gesture off) → Das nie wieder tun · standard DE; turns the behavior off (not just
  hides the notice), so the literal "tun" phrasing fits better than macOS's notice-hiding "Nicht mehr anzeigen" · high
- preset (value in a settings-picker dropdown; opposite of the custom-value option) → Voreinstellung; "back to presets"
  → "Zurück zu den Voreinstellungen" · Microsoft terminology ("indexing preset" → "Indizierungsvoreinstellung"), macOS
  DE print dialog "Voreinstellungen" · high
- FAT32 / exFAT (filesystem-format names) → kept verbatim · macOS DE Finder keeps "FAT32" and "exFAT" untranslated
  ("ExFAT" → "exFAT", "MS-DOS (FAT)" → "MS-DOS-Dateisystem (FAT)"); MS terminology keeps "FAT32"; the en @key flags both
  as do-not-translate format names · high
- formatted as (a drive's filesystem) → "mit … formatiert" ("mit FAT32 formatiert", "mit exFAT formatiert") · macOS DE
  keeps the noun "Format"/"Format:" for the format field; "formatieren"/"formatiert" is the standard DE verb for
  formatting a disk (MS terminology "format" noun → "Format"). The "mit X formatiert" frame reads natural and keeps the
  format name verbatim · high
- too large (a file for a filesystem) → "zu groß" ("Datei zu groß für dieses Laufwerk") · standard DE; pairs with
  drive→Laufwerk · high
- limit (Substantiv, allgemein) → Limit ("Tab-Limit", "kein Limit", "Spaltenbreitenlimit", "Verlauf und Limits"); das
  Verb "limit to" → "begrenzen auf" · der Katalog durchgehend · high. Die eine Ausnahme steht direkt darunter.
- limit (filesystem size cap) → Begrenzung ("keine solche Begrenzung") · KDE Dolphin ("No limit" → "Keine Begrenzung"),
  MS terminology (Begrenzung) · high
- "and N more files" (trailing line under a truncated file list) → "und {countText} weitere {count, plural, one {Datei}
  other {Dateien}}" · GNOME Nautilus ("%'d weitere Objekte ausgewählt" / "%'d weiteres Objekt …"); feminine "weitere" is
  invariant across DE one/other for Datei/Dateien · high
- preset (value in a settings-picker dropdown; opposite of the custom-value option) → Voreinstellung; "back to presets"
  → "Zurück zu den Voreinstellungen" · Microsoft terminology ("indexing preset" → "Indizierungsvoreinstellung"), macOS
  DE print dialog "Voreinstellungen" · high
- action (what the Copy/Move/Compress segmented control chooses; screen-reader label `transferDialog.operationAria`) →
  Aktion · macOS ("Aktion" appears as a bare label, 6× in the pile; "Diese Aktion …") · high
- "Scanning…" (spinner tooltip while the dialog counts selected items) → "Wird durchsucht …" · aligns with the settled
  scan → durchsuchen term and the existing `transferProgress.stageScanning` "Wird durchsucht"; progress-line
  space-before-ellipsis per style guide · high
- "This folder doesn't exist yet. Cmdr will create it during the copy/move." (yellow warning under the dest-path box
  when the typed folder is missing) → "Diesen Ordner gibt es noch nicht. Cmdr erstellt ihn beim Kopieren." / "… beim
  Bewegen." · folder → Ordner (masc., so accusative "diesen Ordner" / pronoun "ihn"); existence via the catalog's
  settled "gibt es" idiom (matches `conflictExistsFolder` "In diesem Ordner gibt es bereits …"); active present "Cmdr
  erstellt ihn" preferred over macOS's passive "wird erstellt" per the active-voice rule; "during the X" →
  verb-preferred "beim Kopieren/Bewegen" (style guide: verb over verbal noun; copy→Kopieren, move→Bewegen settled). DC
  confirms create→erstellen ("Verzeichnis erstellen") and non-existence ("existiert nicht") · high
- **queue.row.label progress arms (rename / create folder / create file)** · `Wird umbenannt` / `Ordner wird erstellt` /
  `Datei wird erstellt` · keep the sibling arms' passive present ("Wird kopiert/bewegt"), so the progress label stays
  passive even though the auto-create _reassurance_ sentence uses active "Cmdr erstellt ihn"; rename via Nautilus ("wird
  … umbenannt"), create via settled `create → erstellen` · high

## Archive browsing

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
  "Archive durchsehen") · high. Deliberately NOT "durchsuchen" — that's the settled scan/search verb (glossary scan →
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
- read-only archive → Schreibgeschütztes Archiv · glossary read-only → schreibgeschützt + archive → Archiv · high
- "There's no trash inside an archive." (bold delete-warning lead) → "In einem Archiv gibt es keinen Papierkorb." ·
  trash → Papierkorb + the catalog's settled "gibt es" existence idiom · high

## Archive-password dialog (2026-07-08)

Terms settled while translating the encrypted-archive unlock modal (`fileOperations.archivePassword.*`; macOS AppKit +
Total/Double Commander de).

- password-protected → `passwortgeschützt` · TC/DC de phrasing + macOS · high. Body: "… ist passwortgeschützt."
- password (noun) → `Passwort` · macOS/MS · high. Input aria-label compounds to `Archivpasswort`.
- unlock (button + verb) → `Entsperren` · macOS AppKit locked-item button ("Entsperren") · high. Reused for the verb
  ("um es zu entsperren").
- archive (the `{name}` head / input label) → `Archiv` · settled de glossary · high.

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
- No `sameAsSourceJustification` needed: all values differ from English.

## Operation log (2026-07-10)

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
- operation log → `Vorgangsprotokoll` · already settled with the retention settings (`settings.section.operationLog` →
  "Vorgangsprotokoll", log → Protokoll) · high. The dialog title (`operationLog.dialog.title`) and the command label
  (`commands.logOperationLog.label`) MUST match this Settings-section name.
- history (of operations) → `Verlauf` · `settings.operationLog.intro` "damit du deinen Verlauf ansehen … kannst",
  `maxAge.label` "Verlauf aufbewahren für"; macOS uses "Verlauf" for history · high. Used in the load-error string.
- file operations (the user's ops collectively) → `Dateivorgänge` · `settings.operationLog.intro` "Cmdr protokolliert
  deine Dateivorgänge" · high. Used in the command description.
- roll back / undo (verb, user-facing prose) → `rückgängig machen` · `settings.operationLog.intro` "Aktionen rückgängig
  machen"; macOS "widerrufen"/"rückgängig" · high. Used for the friendly command description ("… und mach sie
  rückgängig"). Distinct from the technical status noun below.
- rollback (technical status chips) → `Rollback` (noun, kept) · glossary rollback→Rollback + MS terminology · high. Chip
  renderings: `Rollback möglich` / `Kein Rollback möglich` (calm "X nicht möglich" pattern, avoids "kann nicht") /
  `Rollback läuft` (glossary's illustrative "Rollback läuft …" ellipsis dropped here to match the sibling no-ellipsis
  status chips `Läuft`/`Wartet`) / `Rollback abgeschlossen` (complete→abgeschlossen, the "Löschen abgeschlossen"
  pattern; reused for both `rollback.rolledBack` and `outcome.rolledBack`) / `Teilweiser Rollback` (partly→teilweise).
  The short technical noun keeps the chips inside their width; the verb "rückgängig machen" stays for running prose.
- lifecycle status chips → reused verbatim from `queue.row.status` (`queue.json`): queued → `Wartet`, running → `Läuft`,
  done → `Fertig`, "Didn''t finish" (failed) → `Nicht abgeschlossen` (avoids "Fehler"/"fehlgeschlagen" per the voice
  rule, matching the en source's deliberate "Didn''t finish"), canceled → `Abgebrochen` · high.
- per-item outcome chips → done → `Fertig`, skipped → `Übersprungen` (glossary skip→überspringen), "Didn''t finish"
  (failed) → `Nicht abgeschlossen`, rolled back → `Rollback abgeschlossen` · high.
- summary lines (past-participle-final, item → `Objekt`/`Objekte`) → "{countText} Objekt(e) kopiert/bewegt/gelöscht/
  umbenannt/komprimiert", trash → "… in den Papierkorb bewegt" (verbatim `transfer.trash` frame), createFolder →
  "{countText} Ordner erstellt" (Ordner invariant in plural), createFile → "… Datei/Dateien erstellt", "Edited an
  archive" → `Archiv bearbeitet`, "Extracted an archive" → `Archiv entpackt` (extract→entpacken) · high. Mirrors the
  settled `transfer.*` participle pattern ("{phrase} kopiert", "… komprimiert").
- initiator / provenance labels → "You" → `Du` (standalone label, sentence-initial cap; du-address settled), "AI client"
  → `KI-Client` (AI→KI settled; client kept, MS "Client") · high · tentative on the loanword `KI-Client`. "Agent"
  (Cmdr''s own AI agent) → `Agent` (kept; the standard DE loanword for a software/AI agent, matching the en source''s
  bare "Agent") · tentative — flag for David, "Agent" standalone is slightly ambiguous.
- more-items line → "und {countText} {count, plural, one {weiteres Objekt} other {weitere Objekte}}" · item→Objekt
  (neuter, so "weiteres"/"weitere" declines inside each branch, unlike the invariant feminine "weitere Datei(en)"
  glossary entry) · high.
- No `sameAsSourceJustification` needed: every value differs from English.

## Ask Cmdr (2026-07-13)

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
  (archive→archivieren, archived→Archiviert); "aufheben" mirrors the glossary's own `deselect all → Auswahl aufheben`
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
- "Same as Cmdr's AI" (empty-field placeholder) → `Wie die KI von Cmdr` · analytic genitive per the glossary's "don't
  decline Cmdr to Cmdrs" rule, not `Cmdrs KI` · high

## Image-content indexing on network drives (2026-07-13)

Terms settled while translating the network-drive image-indexing opt-in (`settings.mediaIndex.networkVolumes.*`, the
`settings.mediaIndex.*Index*` internal lists, and `search.imageResults.networkOff`/`paused`).

- photo → `Foto` (plural `Fotos`) · macOS AirDrop ("1 Foto empfangen", "^0 Fotos empfangen"), macOS Photos app ("In
  „Fotos“ öffnen") · high. The EN source deliberately says "photos" (warmer) for the network-drive/NAS-archive strings
  while the local card says "images"; keep the split in DE too — `photo → Foto`, `image → Bild` (the local card stays
  `Bildinhalte`/`Bildersuche`). ❌ Don't collapse both to `Bild`
- network drive → `Netzlaufwerk` · Microsoft terminology (network drive → Netzlaufwerk) · high. An SMB-mounted drive
  Cmdr can index; distinct from the settled `network share → Netzwerkfreigabe` (the exported share itself)
- image indexing → `Bildindizierung` · already shipped in the de catalog (`search.imageResults.off`), reused for the
  card/settings label and the search hint · high
- photo archive → `Fotoarchiv` · compound of `Foto` + `Archiv` (archive → Archiv, glossary) · high. The rarely-browsed
  NAS photo store the "always index" switch targets
- indexing paused (auto-pause when a network drive disconnects) → `Angehalten` (status) / `hält an` (prose, verb
  `anhalten`) · macOS ("Kopieren von „^0“ wurde angehalten"), aligns with the settled transfer `pause → anhalten`
  /`Angehalten` · high. resume → `fortsetzen` ("wird fortgesetzt"), reconnect → "wieder verbunden" (derived from the
  settled connect/disconnect terms). ❌ Don't introduce the loanword "pausiert" — macOS uses `anhalten`/`angehalten`
- No `sameAsSourceJustification` needed: every value differs from English.

## Indexing run-kind headers + hour-scale ETA (2026-07-18)

Terms settled while filling the drive-indexing checklist headers (`indexing.run.*`), the spelled-out hour ETAs
(`indexing.eta.hours*Left`), and the "image indexing is queued behind the drive scan" feedback lines
(`indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`).

- run-kind headers → `Erster vollständiger Durchlauf` (first full scan) / `Erneuter vollständiger Durchlauf` (full
  rescan) / `Schnelle Aktualisierung` (quick update) · builds on the settled `scan → durchsuchen`/`Durchlauf` and
  `rescan → erneuter Durchlauf`; `full → vollständig` (glossary full-disk-access → "vollständiger Festplattenzugriff");
  `update → Aktualisierung` (glossary update → aktualisieren) · high. First/rescan share `vollständiger Durchlauf` and
  differ only in `Erster`/`Erneuter`, a clean parallel; the quick path is the light `Aktualisierung`, not a `Durchlauf`.
- spelled-out hour ETA → `noch {n} Stunde(n)` / `noch {n} Stunde(n) {m} Minute(n)` · extends the settled ETA pattern
  (`"{n}m left" → "noch {n} Min."`, DE puts "noch" first) to the full-word hour scale; `hour → Stunde/Stunden`,
  `minute → Minute/Minuten` (CLDR one/other) · high. The compact `s`/`Min.` abbreviations stay on the sub-hour keys;
  only the hour scale spells the unit out, matching the EN source.
- `Laufwerksdurchlauf` (drive scan, as a noun/event: "after the drive scan") · already in the de catalog
  (`indexing.rescan.fallback`: "Ein neuer Laufwerksdurchlauf …") · high. The running-subject phrasing ("The drive scan
  is still running") uses the verb form `Das Laufwerk wird noch durchsucht` instead, matching `indexing.scan.label`.
- No `sameAsSourceJustification` needed: every value differs from English.

## Bulk rename review + image-index scope (quality pass, 2026-07-20)

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
- extension badge (compact) → `(Endung)`; the tooltip and prose keep the full `Dateiendung` · glossary file extension →
  Endung; the catalog's shipped prose ("Das Ändern der Dateiendung ist nicht erlaubt", "Endungsänderungen immer
  erlauben") · high. The badge is a tight chip beside a filename, so it takes the short form.
- overwrite badge → `(Überschreiben!)` (capitalized nominalized verb) · glossary overwrite → überschreiben · high.
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
- No `sameAsSourceJustification` needed: every value differs from English.

## Image-index status badges on files/folders/drives (2026-07-22)

Terms settled while translating the 13 per-file/folder/drive image-index indicator strings
(`fileExplorer.imageIndex.file.*`, `.folder.*`, `.drive.*`, and the `settings.mediaIndex.showFileStatusIcons.*` toggle).

- status badge (the small overlay icon on an image/folder/drive row marking its image-index state) → `Statussymbol`
  (plural `Statussymbole`) · tentative — no perfect direct source. Rejected: MS `Kennzeichen` maps to "flag" (MS TBX id
  54732 = flag), and MS `Abzeichen`/`Badge` are the achievement/reward-badge sense ("a small image indicating roles,
  achievements…"), wrong register for a status overlay. `Statussymbol` reads as the native, transparent term for a small
  status icon (Symbol = icon in DE UI). Flagged for David to confirm vs `Statuskennzeichen`.
- indexed (image, adjective/participle) → `indiziert` · settled `index → indizieren`; matches the 33 shipped `indiziert`
  uses and macOS `Indiziert` · high. "Indexed for image search" → `Für die Bildersuche indiziert`; image search →
  `Bildersuche` (shipped term, `search.imageResults.*`).
- "Waiting to be indexed" (pending status) → `Wartet auf die Indizierung` · `indexing → Indizierung` + natural status
  phrasing · high.
- "Changed since indexing; will be re-indexed" (stale status) → `Seit der Indizierung geändert; wird neu indiziert` ·
  stale sense rendered as a full sentence (glossary stale → veraltet, but the source is a clause, not the bare word);
  re-index → `neu indizieren`, passive present `wird neu indiziert` · high. Semicolon preserved from source.
- "Couldn''t be indexed" (failed status, kept gentle) → `Ließ sich nicht indizieren` · glossary "Couldn't X" → "X ließ
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
- No `sameAsSourceJustification` needed: every value differs from English.

## Image-index settings restructure: cards + semantic-search model delete (2026-07-23)

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
- "couldn''t be removed just now" (model-delete failure, kept gentle) → `ließ sich gerade nicht entfernen` · glossary
  "Couldn''t X" → "X ließ sich nicht …" + the shipped `gerade nicht` calm idiom ("kann {name} gerade nicht indizieren",
  "konnte dich gerade nicht anmelden"); remove → entfernen · high. "Try again in a moment" →
  `Versuche es gleich noch einmal` (retry → erneut/noch einmal versuchen).
- `clip.deleting` "Deleting…" → `Wird gelöscht…` · matched the sibling transient-button label `clip.downloading` ("Wird
  heruntergeladen…", no space before the ellipsis) for a consistent download/delete button pair, rather than the
  status-line space-before-ellipsis form · high.
- "Apple silicon" kept verbatim (lowercase `silicon`, per the en @key "keep it"); no de-macOS rendering exists in the
  pile.
- No `sameAsSourceJustification` needed: every value differs from English.

## Delete-dialog trash switch + transfer From/To groups (2026-07-23)

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

## Master-switch-off strings for drive indexing (review, 2026-07-25)

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
  Not `verborgen`: it's literary here, and the glossary reserves `verborgen` for the dotfile sense (hidden files →
  verborgene Dateien).
- "keeps its own on or off choice" → `behält seine eigene Einstellung` · high. The literal `Ein-/Aus-Wahl` is a coinage
  no source has; German carries the on/off sense inside `Einstellung` in this slot.
- "Off with drive indexing" (the small override badge) → `Mit der Laufwerksindizierung aus` · high. ❌ Never
  `Aus mit der Laufwerksindizierung`: `Aus mit X!` is a fixed German exclamation ("Aus mit der Gemütlichkeit!") meaning
  "X is over", so it reads as a slogan, not a state. Badge length is the German cost here: `Laufwerksindizierung` alone
  is 20 characters, so a faithful badge can't reach the English's 22.
- No `sameAsSourceJustification` needed: every value differs from English.

## Drive index: the change-check run (2026-07-28)

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

## Stalled transfer: the honest-stall notice (2026-07-31)

Terms and phrasings settled for the eight stall strings (`fileOperations.transferProgress.close` / `.stallNotice` /
`.stallWaitingDestination` / `.stallWaitingSource` / `.stallUnknown` / `.stallInFlight` / `.stallLogHint`, plus
`queue.row.stalled`). These replace a confident countdown on a transfer that has stopped moving.

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
  · destination → `Ziel`, source → `Quelle` (glossary, MS terminology), and the catalog's own transfer-domain pair uses
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
  → "Schließen" in Finder + AppKit, key `FR26` and `NSTouchBarCloseTemplate`), and the glossary's settled dismiss-button
  term · high. It sits next to `Abbrechen`, and the two share no stem, so the pair stays distinct.
- **"leave it running in the background" → `im Hintergrund weiterlaufen lassen`** · the settled background phrasing,
  verbatim from `transferProgress.queueTooltip` · high. The two ways out are offered with `Du kannst …` (macOS DE's
  option-offering frame, "Du kannst auch auf „Sichern unter“ klicken") rather than a bare imperative: the line points at
  two choices, it doesn't order one.
- No `sameAsSourceJustification` needed: every value differs from English.

## Kopierter Pfad: die Zwischenablage-Bestätigung (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Ein Key: die Info-Toast-Zeile nach ⌘⌥C. Der Pfad selbst steht darunter in einer eigenen Monospace-Zeile, ist also KEIN
Platzhalter im Satz — der Satz endet auf einem Doppelpunkt und muss ohne den Pfad grammatisch stehen.

- **"Copied the path, it's now on your clipboard:" → `Pfad kopiert, er liegt jetzt in der Zwischenablage:`** · reuses
  the settled `clipboard → Zwischenablage` und `path → Pfad` (Glossar: "Zu Pfad gehen") · high. Das partizipiale
  `Pfad kopiert` folgt dem Muster der Geschwister-Toasts (`{countText} Objekte kopiert`). Kein `dein` vor
  `Zwischenablage`: es gibt nur eine, das Possessivum wäre im Deutschen unnatürlich (macOS sagt "in die Zwischenablage
  kopieren", nie "in deine").
- Kein `sameAsSourceJustification` nötig: der Wert unterscheidet sich vom Englischen.

## Operation queue: the queue window's rename (2026-08-08)

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
- No `sameAsSourceJustification` needed: every value differs from English.

## Corner progress chip + the failure notice (2026-08-08)

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
  and no dangling dot; the `=0 {}` / `other {}` arms stay empty. item → `Objekt` (glossary), destination
  `to {destination}` → `nach {destination}` (Total Commander de `663="NACH: "`, Double Commander de "Nach:", the same
  pair as the transfer dialog's `Von`/`Nach` headings).
- **The trailing `{detail}` needs no term work**: it arrives already formatted from the settled ETA keys ("noch 1 Min.
  20 s") or as the status word `Angehalten`.
- **The chip surface itself is unnamed in the UI.** No string calls it a "chip", so nothing was coined; if one ever
  does, use `Fortschrittsanzeige` (settled `Fortschritt` + macOS's `-anzeige`/`Fortschrittsfenster` pattern) ·
  tentative.
- No `sameAsSourceJustification` needed: every value differs from English.

## Standalone conflict prompt: the operation-context line (2026-08-09)

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

## The progress dialog's empty-queue button (2026-08-09)

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
- No `sameAsSourceJustification` needed: both values differ from English.

## The quit gate (`main.quit.*`, 2026-08-10)

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
  which is Apple's Force Quit (glossary above) and would promise a hard stop.
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
- No `sameAsSourceJustification` needed: every value differs from English.

## Usage stats: "anonymous" dropped, "a random id" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudonym` / `pseudonymisiert` —
that jargon is exactly what the copy avoids.

- **usage stats → `Nutzungsstatistiken`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anonyme` adjective was cut. Both keys now use the plural, matching English's one shared value · high
- **a random id → `eine zufällige ID`** · MS terminology (random → `zufällig`) · high. ❌ Not `Bezeichner` (MS for
  "identifier"): technical and rare in everyday German; `ID` is what a Mac user already knows (Apple-ID).
- **tied to → `verknüpft mit`** · the catalog's own verb for this exact relation (`onboarding.stepBeta.emailNote` "nie
  mit deinen Nutzungsstatistiken verknüpft") · high
- No `sameAsSourceJustification` needed: every value differs from English.

## Conflict-parked queue rows and the rollback confirmation (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`, 2026-08-13)

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
  glossary already records for the status chips · high.
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
- No `sameAsSourceJustification` needed: every value differs from English (`Rollback` differs from the source's two-word
  `Roll back`).

## Rename chaining: the counted "and so did N others" toast (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, 2026-08-18)

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

## Unconfirmed rename + the catch-all name rejection (`fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`, 2026-08-18)

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

## Duplizieren: der Befehl, der im selben Ordner kopiert (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (Befehl, der die Auswahl in ihren eigenen Ordner kopiert) → `Duplizieren`** · macOS Finder `de`, Menü
  „Ablage > Duplizieren“ (`N154`), dazu „Objekte duplizieren“ und „Dupliziert Objekte an ihrem aktuellen Ort“ (geprüft
  auf macOS 26.6.1, `Finder.app/Contents/Resources/de.lproj`, 2026-08-19) · high. Steht neben `Kopieren` (F5) und
  `Bewegen` (F6) und bleibt davon klar unterscheidbar.
- **„Make a copy of the selected files in the same folder“ →
  `Eine Kopie der ausgewählten Dateien im selben Ordner erstellen`** · Infinitiv wie die Nachbarbeschreibungen
  (`commands.editCopy.description`: „… kopieren“); „im selben Ordner“ meint den Ordner, in dem die Dateien schon liegen
  · high.

## Native Menüs: Menüleiste, Kontextmenüs, Fenstertitel (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

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
- **Get Info → `Informationen`**, **Show in Finder → `Im Finder zeigen`**, **Enclosing Folder →
  `Übergeordneter Ordner`**, **Go > Home → `Benutzerordner`**, **Sort By → `Sortieren nach`** · macOS Finder Tier 1 ·
  high. `Im Finder zeigen` weicht minimal von `commands.fileShowInFinder.mac.label` („Im Finder anzeigen“) ab; im
  nativen Menü gewinnt der Finder-Wortlaut, weil der Nutzer beide Menüs nebeneinander sieht.
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
- **busy (Volume in Benutzung) → `(in Benutzung)`** · beschreibend · tentative. MS bietet nur `beschäftigt`/`besetzt`
  (Person bzw. Telefon); für eine Festplatte liest sich beides falsch.
- **„Eject“ → `Auswerfen`, „Disconnect“ → `Trennen`, „Remove“ (aus einer Liste) → `Entfernen`** · macOS Finder · high.
- **forget (Server, Passwort) → `vergessen`** · bereits im Katalog (`fileExplorer.network.share.forgetPassword`) · high.
- **Deliberately identical to English** (`sameAsSourceJustification` gesetzt): `menu.bar.tab` (Tab), `menu.view.zoom`
  (Zoom), `menu.sort.name` (Name), `menu.tag.orange` (Orange), `menu.view.askCmdr` (Produktname).

## SMB-Fallback-Hinweis: die Freigabe hängt an der Systemverbindung (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

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

## Umbenennen und Anlegen: die abgewiesenen Mutationen (`errors.mutation.*`, `errors.volume.*`, 2026-08-23)

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
  jetzt zusätzlich in `style.md` § Decisions to confirm with David; bis dahin bleibt `Systemintegritätsschutz` stehen.
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
- Kein `sameAsSourceJustification` nötig: alle 31 Werte unterscheiden sich vom Englischen.

### Nachtrag: die beiden Papierkorb-Absagen (`errors.mutation.trashNotSupported`/`.trashRefused`, 2026-08-23)

- **`trashNotSupported` → `Dieses Volume hat keinen Papierkorb, deshalb kannst du hier nur endgültig löschen.`** ·
  gesetztes `trash → Papierkorb` (§ Terms, macOS Finder durchgängig) und `volume → Volume` (`style.md`); der
  `…, deshalb …`-Rahmen hält den String parallel zu seinen Geschwistern `volumeGone` und `sipProtected` · high. Der
  Geschwisterstring `errors.write.trashNotSupported.message` („Dieses Volume unterstützt keinen Papierkorb.“) sagt
  dasselbe für den Dialog; die Zeile hier nennt zusätzlich den Ausweg.
- **„delete permanently“ als Aktionsname → `endgültig löschen`, nicht `dauerhaft löschen`** · der Katalog nennt den
  Befehl überall `Endgültig löschen` (`commands.fileDeletePermanently.label`, `menu.file.deletePermanently`,
  `fileExplorer.functionKeyBar.deletePermanentlyAction`, und `errors.write.trashNotSupported.suggestion` zeigt mit „… um
  stattdessen endgültig zu löschen“ auf genau ihn) · high. Der Eintrag `permanently (delete) → dauerhaft` in § Terms
  gilt weiter für die adverbiale Prosa („Dateien werden dauerhaft gelöscht.“,
  `fileOperations.delete.noTrashWarningRest`); wo der Text auf den sichtbaren Befehl zeigt, gewinnt `endgültig`. Wert
  für David: die beiden Formen nebeneinander sind Absicht, keine Inkonsistenz.
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
- Beide neuen Werte enthalten keine Apostrophe; die ICU-Doppelapostroph-Regel greift hier nicht.
- Zu prüfen beim Overflow-Check: das deutsche `.keptRunning` läuft auf rund 160 Zeichen gegenüber rund 140 im
  Englischen, in einem Dialogtext, der schon Datenschutzhinweis und Berichts-ID trägt.

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
- **Zur Entscheidung offen (bei David)**: `…, ist aber weitergelaufen` statt der schlichten Reihung
  `und weitergelaufen`. Deutsch will an dieser Stelle normalerweise eine adversative Partikel, und die `aber`-Variante
  liest sich wärmer; die schlichte Reihung spiegelt dafür das Englische. Ein Einzeiler-Tausch, falls er sie vorzieht.

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

## Auswerfen und Trennen: die neun Absagen (`errors.eject.*`, 2026-08-23)

Neun kurze Zeilen in einem Toast oben rechts, jede als Satz HINTER einem Doppelpunkt: der Rahmen ist
`fileExplorer.pane.ejectFailedToast` („{volumeName} ließ sich nicht auswerfen: …“) oder `.disconnectFailedToast`
(„Trennen nicht möglich: …“). Rohfamilie (`errors.*`): einfache Apostrophe, kein ICU, kein Markdown. Jeder Wert muss
also für sich als vollständiger Satz lesbar sein und darf den Rahmen nicht wiederholen.

- **`drive` bleibt `Laufwerk`** · der ganze Katalog sagt so (`settings.indexing.*`,
  `onboarding.stepOptional.indexing.*`) · high. `Volume` bleibt dem technischen Sinn vorbehalten (§ Terms, `style.md`);
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
  Katalogstimme für „Cmdr couldn't tell“ (`errors.mutation.unexpected`), `Gerät` ist gesetzt (§ Terms) · high.
- **Die Wiederholung im Trenn-Rahmen ist gewollt**: „Trennen nicht möglich: Das ist keine Netzwerkfreigabe, es gibt also
  nichts zu trennen.“ Der Wert muss allein stehen können (er landet auch im Auswerf-Rahmen), deshalb bleibt das zweite
  `trennen` drin.
- Kein `sameAsSourceJustification` nötig: alle neun Werte unterscheiden sich vom Englischen.
- Zu prüfen beim Overflow-Check: `unmountRefused` (rund 125 Zeichen gegenüber 84 im Englischen) und
  `mtpDisconnectRefused` (rund 110) sind die längsten; beide erscheinen im Toast HINTER dem Rahmensatz, der bei einem
  langen `{volumeName}` schon breit ist.

## Papierkorb-Toast: Widerrufen und Zurücklegen (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

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
  Download gehen“). Zu prüfen beim Overflow-Check: 20 Zeichen gegenüber 11 im Englischen, in einem schmalen Toast neben
  „Widerrufen“.
- Kein `sameAsSourceJustification` nötig: alle neun Werte unterscheiden sich vom Englischen.

## Fehlerbericht nachträglich ergänzen: der Notiz-Dialog (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`, 2026-08-28)

Neue Oberfläche: Cmdr hat den Bericht selbst gesendet (Auto-Senden), der Toast „Fehlerbericht gesendet“ trägt jetzt eine
Taste, die einen Dialog öffnet. Dort sieht man, was gesendet wurde, und kann eine Notiz nachreichen, die an DENSELBEN
Bericht angehängt wird. Es geht kein zweiter Bericht raus, und genau das muss die Sprache tragen.

- **`Bericht` statt `Fehlerbericht` innerhalb des Dialogs und der Toasts** · dieselbe Trennung, die der Absturzdialog
  schon fährt (§ „Titel und Bestätigung“: `Absturzbericht` nur, wenn es wirklich abgestürzt ist) · high. Der DIALOGTITEL
  nennt die Sache einmal voll („Zu deinem Fehlerbericht hinzufügen“), damit klar ist, worum es geht; danach reicht
  `Bericht`, und die Tasten bleiben schmal.
- **`Add to your error report` → `Zu deinem Fehlerbericht hinzufügen`**, **`Add to report` → `Zum Bericht hinzufügen`**
  · macOS Finder `de` („Zum Dock hinzufügen“, „Zur Seitenleiste hinzufügen“) · high. Zu prüfen beim Overflow-Check: die
  Taste hat 22 Zeichen gegenüber 13 im Englischen und steht neben `Abbrechen`.
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
  Katalogmuster für Absagen („X ließ sich nicht …“, siehe § Terms) im Präsens · high. Kein „Fehler“, kein
  „fehlgeschlagen“ — die Stilregel gilt, und das Wort `Fehlerbericht` taucht in diesem Satz bewusst nicht auf.
- **Das Help-Menü heißt im Fließtext `das Menü „Hilfe“`** · macOS Finder `de` benennt Menüs genau so („Wähle es aus,
  wähle im Menü „Ablage“ die Option „Informationen“ …“, `BN43`), und `menu.bar.help` ist bereits `Hilfe` · high. Der
  Zielbefehl dahinter ist `menu.help.sendErrorReport` („Fehlerbericht senden…“).
- **`View or add notes to the report` → `Bericht ansehen oder Notiz hinzufügen`** · beide Hälften bleiben stehen, weil
  die Taste beides kann; `ansehen` aus dem Katalog, `hinzufügen` aus dem Finder · high. Verworfen:
  `Bericht ansehen oder ergänzen` (kürzer, aber `ergänzen` hat in der ganzen Referenzsammlung keinen Beleg und lässt
  offen, WAS man ergänzt). Zu prüfen beim Overflow-Check: 37 Zeichen gegenüber 31 im Englischen, in einem Toast neben
  „Einstellungen ändern“.
- **`Note added to your report.` → `Notiz zum Bericht hinzugefügt.`** · Satzbau wortgleich zum Geschwister
  `errorReporter.sentToast.message` („Fehlerbericht gesendet. Deine Referenz-ID ist“), damit die beiden Toasts als Paar
  lesbar sind · high. Das Possessiv fällt weg: zwei `dein` hintereinander („zu deinem Bericht. Deine Referenz-ID“) liest
  sich holprig, und das deutsche UI setzt das Possessiv ohnehin sparsamer als das Englische.
- Kein `sameAsSourceJustification` nötig: alle elf Werte unterscheiden sich vom Englischen.

## Der Auswahldialog: Dateien auswählen und abwählen (`selection.*`, 2026-08-29)

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
- **Die Enter-Taste heißt im Deutschen `Enter`** · `search.runHint` sagt bereits „Zum Suchen Enter drücken“, also „Zum
  Filtern Enter drücken“ · `high`. Kein „Eingabetaste“, kein „Return“.
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
- Kein `sameAsSourceJustification` nötig: alle fünfzehn Werte unterscheiden sich vom Englischen.

## Der zugängliche Name muss die sichtbare Beschriftung enthalten (WCAG 2.5.3, 2026-08-30)

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

## Ein englisches Wort, ein deutsches Wort: die Drift-Prüfung (2026-08-30)

Der Katalog trug 20 Stellen, an denen `de` denselben englischen Text zweimal verschieden benannte, meist weil ein
späterer Durchgang nur die Menüleiste anfasste und die Befehlspalette auf der alten Formulierung stehen ließ. Zehn davon
waren echte Drift und sind unten aufgelöst; die anderen zehn sind ABSICHTLICHE Grenzen und stehen darunter, damit der
nächste Durchgang sie nicht „vereinheitlicht“.

### Aufgelöst

- **`View` (die F3-Aktion, Datei im eingebauten Betrachter öffnen) → `Ansehen`**, überall: `commands.fileView.label`,
  `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel` · der Katalog hatte sich längst festgelegt, ohne dass die
  Tasten es mitbekamen: `settings.appearance.showFunctionKeyBar.description` zählt die Tasten der F-Tasten-Reihe als
  „(Umbenennen, **Ansehen**, Kopieren usw.)“ auf, während die Taste selbst „Anzeigen“ hieß. Dazu
  `askCmdr.wakeToast.action` und `suggestedOps.review` (= „Ansehen“ / „Dateien ansehen“) und der Glossareintrag view/see
  → ansehen · `high`. ❌ Nicht `Anzeigen`: das ist im Katalog der SHOW-Sinn (etwas sichtbar machen), und die beiden
  auseinanderzuhalten ist der ganze Zweck des Eintrags. ❌ Nicht `Vorschau`: das ist das Substantiv für das
  Betrachter-Fenster, kein Imperativ, und `menu.file.view` verlangt laut `@key` ausdrücklich ein Verb. Double Commander
  `de` sagt für dieselbe Aktion `Betrachten` (`tfrmmain.actview.caption`) — richtig, aber Tier 3, und der Katalog hat
  schon ein Wort.
- **`Show in Finder` → `Im Finder anzeigen`**, auch im Menü (`menu.file.showInFinder`, vorher „Im Finder zeigen“) · der
  Glossareintrag trennt die beiden englischen Quellverben schon: `Reveal in Finder` → „Im Finder zeigen“,
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
- **`paused` (Zustand) → `angehalten`, auch im Satz** · `settings.askCmdr.status.needsReview` sagte „Ask Cmdr ist
  pausiert“, `queue.row.status` sagt „Angehalten“ · `high`.
- **Die beiden Beta-E-Mail-Paare sagen wieder dasselbe** · `onboarding.stepBeta.signup.success` und
  `settings.updates.emailConfirmHint` teilen sich einen englischen Satz, ebenso `onboarding.stepBeta.signup.failure` und
  `settings.updates.emailSignupError`. Gewählt: „Sieh in deinem Postfach nach, …“ (idiomatischer als „Schau in dein
  Postfach“) und „Tut uns leid, die Anmeldung hat gerade nicht geklappt. Erneut versuchen?“ — natives Bedauern statt des
  Lehnworts „Sorry“, die schuldfreie Formulierung („hat nicht geklappt“ statt „wir konnten dich nicht anmelden“), und
  das im Glossar festgelegte „Erneut versuchen“ · `high`.

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
  und folgt dem Glossar-Farbnamen „Violett“ · `high`.
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

## Wörter, die auseinandergelaufen sind, ohne dass ein Check es sehen konnte (2026-08-30)

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

## Die Bereichsnamen kommen jetzt vom Mac des Nutzers (`errors.git.*`, `errors.provider.*`, 2026-08-30)

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

## „Zurücksetzen“ nennt jetzt das Objekt: den alten Namen (`askCmdr.renameUndo.undone` / `.partial`, 2026-08-30)

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

## Ein halb zurückgenommener Vorgang: das Rollback zu Ende bringen (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

- **`Finish rolling back` → `Rollback abschließen`** · bleibt im Substantiv-Wortfeld, das der Katalog für dieses Feature
  schon führt (`operationLog.dialog.rollBack` = „Rollback“, `rollingBack` = „Rollback läuft“) · `high`. `abschließen`
  sagt „zu Ende bringen“, nie „neu starten“; `fortsetzen` wäre die Alternative, ist aber schon an `queue.row.resume`
  vergeben. Der Wert steht identisch in `operationLog.dialog.finishRollBack` und
  `fileOperations.rollbackConfirm.finishRollBack` (gleicher englischer String, sonst schlägt `i18n-terms` an).
- **Die Taste `Rollback abschließen` und das Abzeichen `Rollback abgeschlossen` (`rolledBack`) sind Absicht.** Sie
  stehen nie in derselben Zeile: die Zeile trägt entweder das Abzeichen „Teilweiser Rollback“ plus die Taste oder das
  Abzeichen „Rollback abgeschlossen“ ohne Taste. Nach dem Drücken liest sich der Wechsel als Fortschritt.
- **`Finish rolling this back?` → `Rollback abschließen?`** · gleiche Frageform wie das Geschwister
  `fileOperations.rollbackConfirm.title` („Diesen Vorgang rückgängig machen?“), also blanker Infinitiv plus Fragezeichen
  · `high`. Ohne Artikel, weil der Katalog beim Genus von `Rollback` uneins ist (siehe unten).
- **Offen: das Genus von `Rollback`.** `operationLog.rollback.partiallyRolledBack` sagt „Teilweiser Rollback“
  (maskulin), `refusalUnexpected` sagt „das Rollback nicht starten“ (neutrum). Duden führt beide, aber der Katalog
  sollte sich für eines entscheiden. Die neuen Werte umgehen den Artikel, also blockiert nichts; ein späterer Durchgang
  sollte es vereinheitlichen · `tentative`.
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
- Kein `sameAsSourceJustification` nötig: alle fünf Werte unterscheiden sich vom Englischen.

## Der Toast nach einem abgebrochenen Vorgang: was die Rücknahme geschafft hat (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, 2026-08-31)

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
  `Objekt` ist das Katalogwort für „Datei oder Ordner“ (Glossar `item → Objekt`, macOS Finder „Ausgewählte Objekte“) ·
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
  Glossar `read-only → schreibgeschützt`), im Ton von `trash.undoUnavailable`.
- Kein `sameAsSourceJustification` nötig: alle 18 Werte unterscheiden sich vom Englischen.

## Der Block-Screen bei zu altem WebKit (`main.oldWebkit.*`, 2026-09-02)

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
- Kein `sameAsSourceJustification` nötig: alle drei Werte unterscheiden sich vom Englischen.

## Der Hinweis auf zu altes macOS (`main.oldMacos.*`, 2026-09-02)

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

## Ask Cmdr schaut jetzt in Dateien hinein: Zustimmungstexte und Werkzeugzeilen (`askCmdr.tool.inspectFile.*`, `askCmdr.consent.item.contents`, `askCmdr.consent.contentsRule`, `askCmdr.consent.whatsNew.body`, 2026-09-02)

Fünf Schlüssel rund um das `inspect_file`-Werkzeug: die beiden Werkzeugzeilen im Chat, der neue Aufzählungspunkt auf dem
Zustimmungsbildschirm, der Absatz `contentsRule` (ersetzt `askCmdr.consent.noContents`) und der „Was ist neu“-Absatz.
Belege: Pile `de/` (macOS Finder/AppKit, Microsoft-Terminologie, Nautilus/Thunar/Dolphin, TC/DC) plus die Live-Bundles
`Preview.app` und `Photos.app` (`.loctable`, macOS 26.6.2, 2026-09-02).

- **look inside files (Werkzeugzeile) → `Dateien werden durchgesehen` / `Dateien durchgesehen`** · `durchsehen` ist
  das gesetzte Verb fürs Hineinschauen-und-Auflisten (§ Archive browsing, KDE Dolphin „Archive durchsehen“), und die
  Form spiegelt die Geschwister `Deine Fotos werden durchsucht` / `Deine Fotos durchsucht` · `high`. ❌ Nicht
  `durchsuchen` (= suchen) und nicht `Dateiinhalte werden gelesen`: Das Werkzeug liest nur einen begrenzten Teil, und
  `Dateiinhalte` ist im Katalog das Wort für das, was Cmdr NICHT liest (`askCmdr.empty.hint`).
- **look inside a file (Prosa) → `hineinschauen`** („kann Ask Cmdr jetzt hineinschauen und einen Teil davon lesen“) ·
  Katalogpräzedenz aus dem alten `noContents` („was in deine Bilder hineinschaut“); umgangssprachlich, passt zum
  `du`-Ton · `high`.
- **thumbnail → `Miniatur` (Plural `Miniaturen`)** · Apple Tier 1: Vorschau `Thumbnails` → „Miniaturen“, Fotos
  `Show/Hide Thumbnails` → „Miniaturen einblenden/ausblenden“, AppKit „Miniatur des Tab-Auswahl-Bilds“; auch schon im
  alten `noContents` · `high`. Runners-up: Microsoft „Miniaturansicht“/„Miniaturbild“ (Windows), Thunar/DC/TC
  „Vorschaubilder“ (Tier 3). ❌ Nicht `Vorschaubild`: `Vorschau` ist im Katalog der Viewer (glossary viewer → Vorschau)
  und Apples Programmname.
- **camera details (EXIF eines Fotos) → `Kameraangaben`** · `Kamera` ist überall gleich (Vorschau-Inspektor „Kamera“,
  Microsoft `camera → Kamera`, Thunar „Kamera“, Nautilus „Kameramarke“/„Kameramodell“); das `-angaben` folgt dem
  Katalog, der Metadaten `Dateiangaben` nennt (`askCmdr.renameReview.evidence.metadata` „Dateiangaben, nicht der
  Inhalt“) · `high` für `Kamera`, `tentative` für die Zusammensetzung (keine Quelle hat „camera details“ als Begriff;
  Runner-up „Kameradaten“ wäre ebenso gängig). Nicht `EXIF`: Das Englische vermeidet das Kürzel absichtlich.
- **location / where it was taken (eines Fotos) → `Aufnahmeort`** · Apples Fotos-App sagt für den Ort eines Fotos
  schlicht „Ort“ (`Assign Location` → „Ort zuweisen“, `Hide Location` → „Ort ausblenden“), Thunar/Nautilus ebenso
  „Ort“, Microsoft `location → Standort` (Gerätesinn, Fotos-App „Standortinformationen“ beim Teilen). Auf dem
  Zustimmungsbildschirm steht das Wort neben Dateien, wo ein bloßes „Ort“ als Speicherort gelesen würde; das
  Kompositum aus Apples „Ort“ und dem „aufgenommen“ aus „where it was taken“ ist eindeutig · `tentative` (keine Quelle
  hat das Kompositum selbst). In der Prosa `einschließlich des Aufnahmeorts`, in der Liste `der Aufnahmeort eines
  Fotos`.
- **title and author (PDF-Metadaten) → `Titel und Autor`** · Vorschau-Inspektor `Title` → „Titel“, `Author` →
  „Autor:in“; Microsoft `title → Titel`, `author → Autor`; Dolphin `Author` → „Autor“ · `high` für `Titel`,
  `tentative` für `Autor`: Apple nutzt den Gender-Doppelpunkt, den unsere Regel (keine typografischen Glyphen,
  Screenreader) ausschließt, und eine neutrale Umschreibung („wer es verfasst hat“) liest sich in der Aufzählung
  gestelzt. `Autor` steht hier als Name des Metadatenfelds, nicht als Anrede einer Person. Siehe `style.md` §
  Decisions to confirm with David.
- **page (eines PDFs) → `Seite`/`Seiten`** · Vorschau `Pages` → „Seiten“, `Page Count` → „Seitenanzahl“ (auch
  Dolphin), Microsoft `page → Seite` · `high`. `PDF-Seiten` mit Bindestrich, `einige Seiten eines PDFs` (Genitiv
  `des PDFs`, Glossar: `das PDF`).
- **archive → `Archiv`**, „the list of files inside an archive“ → `die Liste der Dateien in einem Archiv`, „what’s
  inside an archive“ → `was in einem Archiv steckt` · gesetzt (§ Archive browsing); `steckt` hält den lockeren Ton des
  Englischen `what’s inside` · `high`.
- **provider → `Anbieter`** („gehen an deinen Anbieter, damit er sie finden kann“) · gesetzt (Microsoft), Wortlaut
  aus dem alten `noContents` unverändert übernommen · `high`.
- **`contentsRule`: die beiden letzten Sätze sind der alte `noContents`-Wortlaut** („Die Fotosuche …“ ist neu
  angeschlossen mit `funktioniert genauso`; „Der Text, den Cmdr in den passenden Fotos erkannt hat, und deren Tags
  gehen an deinen Anbieter, damit er sie finden kann.“ und „Ask Cmdr kann Umbenennungen, Verschiebungen und
  Aufräumaktionen vorschlagen, und an keiner Datei passiert etwas, bevor du zustimmst.“ wörtlich) · `high`. Der
  Absatz verspricht bewusst NICHT „keine Dateiinhalte“ mehr, sondern `nie ganze Dateien, Fotos oder Miniaturen` und
  `einen begrenzten Teil davon`. `es` für Cmdr wie in `askCmdr.error.noConsent` („was es sehen darf“).
- **`whatsNew.body`: zweiter Satz unverändert** („Das ist mehr, als du damals zugestimmt hast, deshalb hier noch
  einmal das Ganze.“); der erste Satz ist neu und nennt die vier Dinge in derselben Reihenfolge wie `contentsRule`.
- Kein `sameAsSourceJustification` nötig: alle fünf Werte unterscheiden sich vom Englischen.
- **„looks inside a file only when you ask about it“ → `schaut nur dann in eine Datei hinein, wenn du nach ihr
  fragst`** (`askCmdr.empty.hint`, `settings.askCmdr.intro`) · dasselbe `hineinschauen` wie oben, und `nach ihr fragst`
  wie in `contentsRule` („Wenn du nach einer Datei fragst“) · `high`. Beide Schlüssel haben das alte „nie
  Dateiinhalte“ / „ist schreibgeschützt … verändert nie etwas“ verloren; die Einstellungs-Einleitung sagt jetzt
  `verändert nie eine Datei ohne deine Zustimmung` (Zustimmung wie `bevor du zustimmst` in `contentsRule`).
