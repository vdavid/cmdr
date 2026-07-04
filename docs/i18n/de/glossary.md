# de glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/de/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

## Terms

- crash report → Absturzbericht · macOS (Apple uses "abgestürzt" / "unerwartet beendet" for crashes; "Absturzbericht" is
  the standard Apple/MS rendering of "crash report") · high
- crash (verb) → abstürzen; "quit unexpectedly" → "unerwartet beendet" · macOS Finder ("… wurde unerwartet beendet") ·
  high
- feedback → Feedback (kept; the loanword is standard in DE UI, macOS/MS both use it) · high
- report ID → Berichts-ID · compound of Bericht (report) + ID; standard DE compound formation · high
- email → E-Mail (-Adresse for the address) · macOS Mail, MS terminology · high
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
- permanently (delete) → dauerhaft ("Dauerhaft löschen", "dauerhaft gelöscht") · MS terminology, standard DE · high
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
- queue (transfer queue) → Warteschlange; "Transfer queue" → Übertragungs-Warteschlange · MS terminology
  (queue→Warteschlange); compound with transfer→Übertragung · high. The "Queue" button on the progress dialog
  (send-to-background + open the queue window) renders as "Warteschlange"
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
- limit (filesystem size cap) → Begrenzung ("keine solche Begrenzung") · KDE Dolphin ("No limit" → "Keine Begrenzung"),
  MS terminology (Begrenzung) · high
- "and N more files" (trailing line under a truncated file list) → "und {countText} weitere {count, plural, one {Datei}
  other {Dateien}}" · GNOME Nautilus ("%'d weitere Objekte ausgewählt" / "%'d weiteres Objekt …"); feminine "weitere" is
  invariant across DE one/other for Datei/Dateien · high
- preset (value in a settings-picker dropdown; opposite of the custom-value option) → Voreinstellung; "back to presets"
  → "Zurück zu den Voreinstellungen" · Microsoft terminology ("indexing preset" → "Indizierungsvoreinstellung"), macOS
  DE print dialog "Voreinstellungen" · high
- action (generic "Action:" field label before a Copy/Move or Trash/Delete segmented control) → Aktion ("Aktion:") ·
  macOS ("Aktion" appears as a bare label, 6× in the pile; "Diese Aktion …") · high
- route ("Route:" label before a source → destination line in the copy/move dialog) → Route (kept; identical to English)
  · no transfer-label source (TC/DC phrase it in full as "von X nach Y", not a label); "die Route" is a genuine German
  noun for a path between two points, fits the FROM→TO arrow and keeps the compact, evocative English register · high.
  Recorded as sameAsSourceJustification in the catalog
- "Scanning…" (spinner tooltip while the dialog counts selected items) → "Wird durchsucht …" · aligns with the settled
  scan → durchsuchen term and the existing `transferProgress.stageScanning` "Wird durchsucht"; progress-line
  space-before-ellipsis per style guide · high
- "Scan complete" (checkmark tooltip once counting finished) → "Durchsuchen abgeschlossen" · scan → durchsuchen +
  complete → abgeschlossen (matches the catalog pattern "Löschen abgeschlossen"/"Kopieren abgeschlossen") · high
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
