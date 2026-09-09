# German (de) translation style guide

Working notes for translating Cmdr into German. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into German.

## Formality: `du`, settled

**Address the user as `du`** (informal, lowercase) throughout. This is settled from the sources, not a guess:

- macOS German is fully informal. Across the mined Finder/AppKit strings, every second-person address uses `du` / `dich`
  / `dir` / `dein` (583 such markers); there is not a single formal `Sie` address. The only capital-`Sie` hits are the
  pronoun "they/it" ("Sie werden auf all deinen Geräten …"), not the polite form. Finder phrases user prompts as
  "Möchtest du …", "Du kannst …", "Bitte sichere das Dokument …" (verified in `de/macOS/`, grep over Finder + AppKit,
  2026-06-19).
- Microsoft German is the opposite: the style guide and product strings use formal `Sie` ("Versuchen Sie es noch
  einmal.", "Möchten Sie fortfahren?", "Klicken Sie auf …"). This is the Windows convention, not ours.
- Cmdr is a macOS app with a friendly voice that even signs onboarding as David, so `du` is both the macOS-native choice
  and the right tonal fit. Use lowercase `du` (modern UI form, not the old letter-writing capitalized `Du`).

## Voice and tone

Friendly, concise, active, calm, warm even within `du`. German UI copy drifts long and noun-heavy; resist it. Prefer a
verb over a verbal noun where the English does ("Suchen", not "Durchführen einer Suche").

Error messages stay calm and actionable and never use "Fehler" or "fehlgeschlagen" as a bare label: state the problem
and a next step ("Die Datei konnte nicht umbenannt werden. Erneut versuchen?"). Note: macOS itself does use "Fehler beim
…" freely ("Fehler beim Umbenennen der Palette"); Cmdr's voice rule is stricter than macOS here, so don't copy that
pattern.

## Formality mechanics

- **`du`, lowercase**, throughout (see Formality above).
- **Buttons and menu items: imperative.** "Speichern", "Abbrechen", "Löschen", "Umbenennen", "Kopieren". This matches
  macOS Finder ("Umbenennen", "Auswerfen", "Kopieren").
- Keep direct address light; German UI often phrases neutrally ("Wird geladen …") where English would say "Loading your
  files". Don't force `du` into every line.

## Decision points

Formality is settled above (`du`). These are the remaining German-specific calls.

- **Regional variant: one base `de`, no `de-AT` / `de-CH` split needed.** German has three national standards. The only
  systematic UI-visible difference is `ß` vs `ss`: Switzerland (`de-CH`) dropped `ß` entirely and writes `ss`
  ("Strasse", "schliessen"); Germany and Austria keep `ß` ("Straße", "schließen"). Apple and Microsoft both ship a
  single German with `ß`, with no separate Swiss UI locale for most products; vocabulary differences (AT "Ordner" is the
  same; few file-manager terms diverge) are negligible. Recommendation: ship one `de` with `ß`, matching macOS and
  Microsoft. Only add `de-CH` if a Swiss user reports the `ß` as jarring. Confidence: high.
- **Capitalization is grammar, not style.** All nouns are capitalized mid-sentence ("die Datei", "der Ordner"); this is
  not title case and the app's sentence-case rule still holds (see Notes). A translator must not "fix" a capitalized
  noun to lowercase to match English casing. Confidence: confirmed (German orthography).
- **Gendered grammar: avoid generic personal nouns where possible, no `*`/`:` gender stars in UI.** German agent nouns
  are gendered ("der Benutzer" / "die Benutzerin"). macOS and Microsoft German both lean on neutral phrasings and direct
  `du`-address ("Möchtest du …") to sidestep gendering the user, and neither uses gender-star forms ("Benutzer\*innen")
  in core UI. Cmdr addresses the user directly as `du`, so this rarely bites; when a generic noun is unavoidable, prefer
  a neutral term ("Person", "Konto") or rephrase to direct address rather than a gender star. Recommendation: no gender
  stars; rephrase to `du` or a neutral noun. Confidence: high.
- **Length and compounds are the real overflow risk** (German runs 20–35% longer, compounds concatenate). This is a
  layout call, covered under Notes → Length; flagged here because it's the German decision most likely to force a copy
  rewrite. Confidence: confirmed.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Sources cite concrete evidence; tier order is macOS
(highest, Tier 1) → Microsoft (Tier 2) → GNOME/Xfce (Tier 3). Confidence is `confirmed` (human signed off), `high`
(authoritative sources agree), or `tentative` (sources conflict or none had it). German capitalizes all nouns (grammar),
so noun glossary terms stay capitalized; verbs are lowercase in running text, imperative-capitalized as button labels.

Straightforward (sources agree, `high`):

- file → Datei (plural Dateien) · macOS Finder, MS terminology (DEU/AUT/CHE) · high
- folder → Ordner · macOS Finder ("Der Ordner konnte nicht erstellt werden."), MS terminology (DEU/AUT/CHE) · high
- directory → Verzeichnis · MS terminology (DEU/AUT/CHE); technical sense only, prefer Ordner for the UI · high
- drive → Laufwerk · MS terminology (DEU/AUT/CHE) · high
- trash → Papierkorb · macOS Finder (consistent), same on Windows · high
- undo → widerrufen · macOS AppKit MenuCommands ("Undo Smart Dash" → "Intelligenten Bindestrich widerrufen"); NOT
  Nautilus' "Rückgängig" (Tier 3) · high
- put back (an item from the trash, to where it was) → zurücklegen · macOS Finder ("Put Back" → "Zurücklegen") · high.
  Keep it apart from `zurücksetzen`, which the catalog spends on undoing a RENAME (`askCmdr.renameUndo.*`).
- delete → löschen · macOS ("Delete"→"Löschen", "Erase"→"Löschen") · high
- copy → kopieren · macOS ("Copy"→"Kopieren") · high
- rename → umbenennen · macOS Finder ("Umbenennen …") · high
- viewer → Vorschau · macOS preview UI; Quick Look stays "Quick Look" (brand) · high
- eject → auswerfen · macOS AppKit ("NSNavEjectButton"→"auswerfen"), Finder ("Auswerfen") · high
- disconnect → trennen · macOS ("Disconnect"→"Trennen") · high
- server → Server · macOS ("Mit Server verbinden"), MS terminology · high
- search → suchen (verb) / Suche (noun) · macOS ("Search"→"Suchen") · high
- sort → sortieren · macOS sort UI · high
- settings → Einstellungen · macOS Systemeinstellungen, MS · high
- cancel → abbrechen · macOS ("Cancel"/"CANCEL"→"Abbrechen") · high
- overwrite → überschreiben · MS terminology (DEU/AUT/CHE) · high
- index / indexing → Index / Indizierung · MS terminology (Index, DEU/AUT/CHE) · high
- transfer → Übertragung · MS terminology (Übertragung), Xfce Thunar ("Dateiübertragung") · high
- tab → Tab (plural Tabs) · macOS, MS terminology (ProperNoun) · high
- bookmark → Lesezeichen · macOS, MS terminology ("Lesezeichen erstellen") · high
- sidebar → Seitenleiste · macOS Finder · high
- download → Download (noun) / laden (verb) · MS/macOS common usage · high
- removable (media) → Wechselmedium · macOS Finder („Wechselmedien auswerfen und von Servern trennen“), MS terminology
  (`removable media` → Wechselmedien) · high
- in use → in Verwendung · macOS Finder („Das Volume ist gerade in Verwendung …“) · high
- idle → nicht mehr beschäftigt · the catalog's own pair (`indexing.enrich.pausedIdle`, `fileExplorer.mtp.deviceBusy`) ·
  high
- unplug → abziehen · the catalog („Du musst nichts abziehen.“, „Zieh das USB-Kabel ab …“) · high
- operation (a copy/move/delete) → Vorgang · the catalog throughout (Vorgangswarteschlange, Vorgangsprotokoll) · high

Contested or sense-specific (read the block):

- move → Bewegen · macOS Finder vs Microsoft · high
  - macOS Finder is decisive and consistent: "Move"→"Bewegen", "Move Document"→"Dokument bewegen", "move to the Trash"→
    "in den Papierkorb bewegen", "Copy and Move Items"→"Objekte kopieren und bewegen".
  - Microsoft German uses "Verschieben" for move. Since Cmdr is a macOS app, pick Bewegen; note Verschieben is what a
    Windows-trained user might expect.
- move to trash → in den Papierkorb bewegen · macOS · high
  - macOS phrasings: "Trash ${entities}"→"${entities} in den Papierkorb bewegen", "Moves items to the Trash"→"Legt
    Objekte in den Papierkorb", "Möchtest du das Dokument wirklich in den Papierkorb bewegen?". Both "in den Papierkorb
    bewegen" and "in den Papierkorb legen" appear in Finder; prefer "bewegen" to stay consistent with the move verb
    above.
- volume → Volume · macOS · high
  - macOS keeps "Volume" for a mounted disk volume: "Servervolume", "Zielvolume", "Backup-Volume", "Volumeformat", "^1
    auf dem Volume". Do NOT use the MS-terminology first hit "Lautstärke", that is the audio-volume sense.
- pane → Bereich · macOS vs Microsoft · high
  - macOS uses "Bereich" for a panel/area of a window ("Der Bereich „Bewegen“ …", "Bereich „Schreibtools“ anzeigen").
    Microsoft terminology's "Blatt" is the spreadsheet-sheet sense and doesn't fit a file-list pane. Use Bereich;
    "Fensterbereich" only if disambiguation is needed.
- share (network) → Freigabe · macOS · high
  - macOS uses Freigabe for sharing ("Bildschirmfreigabe"). An SMB share is a Netzwerkfreigabe / SMB-Freigabe.
- listing → Dateiliste · no direct source · tentative
  - "listing" (the file list in a pane) has no single canonical source term. Dateiliste reads naturally and is
    unambiguous; macOS calls list view "Listendarstellung". Confirm with David if "Liste" alone reads better in context.
- item → Objekt · macOS · high
  - Not in the original glossary but pervasive: macOS Finder calls a file-or-folder row an "Objekt" ("Ausgewählte
    Objekte", "Objekte komprimieren", "^0 Objekte werden sofort gelöscht"). Use Objekt for the generic file-or-folder
    entity.

- file system → Dateisystem · macOS AppKit `DocumentDragging.loctable` („… kann im Dateisystem nicht gefunden werden.“),
  `ErrnoErrors.loctable` („Read-only file system“ → „Dateisystem ist schreibgeschützt“), `InfoPlist.loctable` („File
  System Plug-in“ → „Dateisystem-Plug-in“); Finder `NE29` („Das Objekt ist zu groß für dieses Dateisystem.“) (live macOS
  26.6.2, build 25G83, 2026-09-06) · high
- file access → Dateizugriff (Kompositum) bzw. „Zugriff auf Dateien“ (analytisch) · macOS TCC `Localizable.loctable`
  schreibt durchweg analytisch (`„%@“ möchte Zugriff auf Dateien auf einem Wechselmedium.`); das Kompositum
  `Dateizugriff` kommt in macOS `de` nicht vor, ist aber normale deutsche Wortbildung und hält ein Schalter-Label kurz.
  Für Fließtext die analytische Form nehmen, für ein Label das Kompositum (`settings.fileOperations.adbEnabled.label` =
  „Dateizugriff auf Android über ADB“) · high
- location (a place on disk a user picks or names) → Speicherort · macOS Finder `BU39` („Choose Location…“ →
  „Speicherort wählen …“), `BU37_V1` („at its original location“ → „am ursprünglichen Speicherort“) · high. Abgrenzung:
  der ALLGEMEINE Ort im Dateisystem heißt schlicht `Ort` (Finder `FI12` „Dieser Ort ist schreibgeschützt.“, `SD5`/`FI9`
  „Locations“ → „Orte“), und `PV56`/`PV5` „Location“ → „Standort“ ist der GEO-Ort eines Fotos. Für ein Feld, in das der
  Nutzer einen Programmpfad einträgt, gewinnt `Speicherort` (`settings.fileOperations.adbBinaryPath.label` =
  „Speicherort von adb“)
- USB debugging → USB-Debugging · Googles deutsche Android-Doku, die Entwickleroptionen-Bezeichnung im Gerät
  („Aktivieren Sie **USB-Debugging** in den Geräteeinstellungen unter **Entwickleroptionen**.“,
  developer.android.com/studio/debug/dev-options?hl=de, abgerufen 2026-09-06) · high. Der Feature-Name aus Googles
  Sprachhoheit, nicht übersetzen; Googles `Sie`-Register aus derselben Quelle NICHT übernehmen (Cmdr siezt nie).
- Android platform tools → Android Platform Tools · Googles deutsche adb-Doku nennt das Paket „Android SDK Platform
  Tools“ (developer.android.com/tools/adb?hl=de, abgerufen 2026-09-06); der englische Katalogtext sagt „Android platform
  tools“, also bleibt der Name unübersetzt und ohne `SDK`, damit er zum Nachbarschlüssel passt, der `Android SDK`
  separat nennt · high
- tooling (generisch, nicht als Produktname) → Tools · der Katalog selbst („Backup-Tools“ in
  `errors.listing.lockUnavailable.suggestion`, „externe Tools“ in `settings.developer.mcpPort.description`); NICHT
  `Werkzeuge`, das der Katalog für die Werkzeuge eines KI-Agenten reserviert (`askCmdr.tool.unknown.done`) · high

From the AI-copy sweep and the provider-setup pass (the app stopped calling its AI „Ask Cmdr“ everywhere it just meant
„AI“; the name now survives only where it names the chat panel):

- AI features → KI-Funktionen · der Katalog selbst (`settings.ai.tooltipOff` „KI-Funktionen sind ausgeschaltet“,
  `settings.ai.provider.description` „Wähle, wie KI-Funktionen betrieben werden.“, `onboarding.stepAi.intro`) · high
- the AI (als handelndes Subjekt, wo das Englische bewusst nicht „Cmdr“ sagt) → die KI · der Katalog
  (`settings.askCmdr.intro` „Chatte mit einer KI …“) · high. Abgrenzung: `Cmdr` bleibt `Cmdr`, und `Ask Cmdr` bleibt
  `Ask Cmdr`; siehe die Notiz unten dazu, welcher der drei Namen wann steht
- AI provider → KI-Anbieter · der Katalog durchgehend (`askCmdr.error.notConfigured`, `askCmdr.consent.intro`) · high
- file operations → Dateivorgänge · der Katalog (`commands.logOperationLog.description` „Verlauf deiner Dateivorgänge“,
  `fileExplorer` „schnelle Dateivorgänge“, `settings` mehrfach) · high. Die Settings-Karte heißt dagegen
  `Dateioperationen`, weil sie eine Rubrik benennt, keine laufenden Vorgänge
- placeholder → Platzhalter · Microsoft terminology (`GERMAN.tbx`, beide Sinne) · high
- endpoint → Endpunkt · Microsoft terminology (`GERMAN.tbx`, 4 von 6 Sinnen; `Teilnehmer`/`Endgerät` sind die Telefonie-
  und Geräte-Sinne) · high
- deployment (eine Azure-Bereitstellung) → Bereitstellung · Microsoft terminology (`GERMAN.tbx`, 5 Sinne) · high
- resource (eine Azure-Ressource) → Ressource · Microsoft terminology (`GERMAN.tbx`, 4 Sinne) · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`). macOS UI names
Cmdr opens into (System Settings panes, "Papierkorb") should match a German macOS.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('de')`). Write both branches: "1 Datei" / "{count}
Dateien".

- Case agreement interacts with counts and with surrounding prepositions. A counted noun often sits in a case the
  English doesn't mark ("in 3 Ordnern", dative plural `-n`). Get the case right inside each branch.
- **The dative `-n` needs a preposition to earn it.** A bare count phrase is NOMINATIVE: "12 Ordner", "12 Bilder", never
  "12 Ordnern". The `-n` only appears where something governs it ("in 3 Ordnern", "von 12 Bildern", "mit Dateien und
  Ordnern"). This is easy to get wrong when a fragment is written on its own and only later lands in a sentence, which
  is how `selectionInfo.noSelectionDirs` shipped as "und 12 Ordnern". Read the assembled sentence, not the fragment. In
  the reference pile, every `Ordnern` follows a preposition and every counted one reads `%d Ordner` (KDE Dolphin, GNOME
  Nautilus, Xfce Thunar) · high.
- German has grammatical gender (der/die/das); article and adjective must agree with the counted noun in every branch.
- **Text AFTER a plural block has to work with both branches' verbs.** English often puts the verb inside the branches
  ("# file is still open" / "# files are still open") and shares the rest outside. German keeps that split, so check the
  sentence reads with `ist` AND with `sind`: "1 Datei ist noch geöffnet und möglicherweise schon teilweise geschrieben."
  / "5 Dateien sind noch geöffnet und …" (`transferProgress.stallInFlight`). If a shared tail can't agree with both,
  move it inside the branches rather than bending the grammar.
- **A counted tail with NO plural param has nothing to agree with.** A message that passes only the formatted
  `{somethingText}` for a second count gives you no integer to select on. English gets away with it ("stayed" fits 1 and
  12); German doesn't. The fallback is to drop the finite verb and let the first half's noun carry the clause ("12
  Dateien zurückgelegt; 3 noch im Papierkorb."). Ask for an integer partner instead where you can:
  `fileOperations.trash.undonePartial` gained a `{skipped}` driver for exactly this reason, so its second half is now a
  normal plural with a real verb ("… zurückgelegt; {skippedText} {skipped, plural, one {Objekt blieb} other {Objekte
  blieben}} im Papierkorb.").

## Notes and decisions

- **Native Menüs folgen dem Finder-Wortlaut, nicht dem Katalog-Wortlaut.** Wo macOS ein Gegenstück hat, gewinnt es
  (`Ablage`, `Darstellung`, `Im Dock ablegen`, `Widerrufen`, `Einsetzen`), weil der Nutzer Cmdrs Menüleiste direkt neben
  der des Finders sieht. Belege und die Ausnahmen: `glossary.md` § Native Menüs.
- **Nouns are always capitalized.** This is grammar, not title case. The app's sentence-case rule still holds (only the
  first word and nouns are capitalized), so "Datei umbenennen" but "Save"→"Speichern" at sentence start. Don't
  title-case adjectives/verbs.
- **Compound nouns concatenate** ("Dateiübertragung", "Netzwerkfreigabe"). This is correct German, but it lengthens
  strings: see Length below.
- **Quotation marks: `„…“`** (low opening, high closing) is the standard German form, and macOS uses it consistently
  ("Möchtest du „%@“ … bewegen?"). Avoid English `"…"`.
- **Ellipsis: always the single character `…`, never three ASCII dots (`...`).** Two placements, settled across the
  catalog: progress / gerund status lines take a SPACE before it ("Wird geladen …", "Laufwerk wird durchsucht …",
  "Verbindung wird hergestellt …"); menu-item and button labels that open a dialog attach it with NO space, the macOS
  menu convention ("Einführung…", "Befehle suchen…", "Fehlerbericht senden…"). The English source mixes `...` and `…`
  freely; normalize to `…` either way.
- **A waiting line in BODY PROSE is a sentence, not a progress label.** macOS's own waiting strings are verbless
  fragments with an ellipsis ("Warten auf das Laufwerk …", "Auf Upload warten"), which is right for a title or a status
  chip. When the English is a full sentence ending in a period and sits next to other sentences, give it a subject
  instead of shipping a fragment plus a period, as `stallWaitingDestination` does: "Cmdr wartet auf eine Antwort vom
  Ziel." Active voice and a grammatical period beat a literal fragment.
- **Don't decline the brand `Cmdr` to a genitive `Cmdrs`.** Use the analytic genitive "… von Cmdr" ("die
  Protokolldateien von Cmdr", "die Vorschau von Cmdr"), matching how Apple leaves product names undeclined. A declined
  "Cmdrs" also trips `desktop-i18n-dont-translate` (it scans for the verbatim token "Cmdr").
- **Length: German is the worst overflow risk of the three** (often 20–35% longer than English, plus long compounds).
  Overflow-check the layout hard against the pseudolocale (`en-XA`); look for clipped buttons, labels, and toasts.
- **Ein Rückverweis auf `{name}` nimmt das Katalogwort, nie ein Pronomen.** `{name}` trägt einen fremden Dateinamen,
  also kein Genus, auf das sich „es“/„sie“ stützen könnte. Wo der zweite Satzteil auf das Objekt zurückkommt, steht das
  Nomen: „Habe {name} unverändert gelassen: **das Objekt** hat sich geändert …“
  (`fileOperations.cancelRollback.reason.*`), „… ob sich **die Datei** geändert hat“
  (`askCmdr.renameUndo.skipReason.*`). Possessive („sein alter Ort“) fallen aus demselben Grund weg; nimm den Artikel
  („am alten Ort“). Belege und die Wortwahl: `glossary.md` § Der Toast nach einem abgebrochenen Vorgang.
- **Case-marked placeholders are a trap.** A `{name}` that lands in a genitive/dative slot can't be inflected by the
  catalog. Restructure the sentence so the placeholder stays nominative, or carries its own preposition.
- **German case marking is what breaks aria containment** (the shared rule: `../../guides/i18n-translation.md` § An
  `*Aria` key must contain its visible label). The natural label and the natural aria sentence often want different
  cases: `In den Hintergrund` is not inside "… im Hintergrund weiterlaufen lassen". Take the case the aria needs. Worked
  example: `glossary.md` § The progress dialog's empty-queue button.
- **Numbers and dates come from the formatter layer** (comma decimal, period/space thousands). Never hardcode
  separators.
- **A space goes before the percent sign** (`{percent} %`, "Auf 100 % zoomen"), per DIN 5008 and the rest of the
  catalog. English writes `50%`; German doesn't. Same for unit symbols after a number.
- **Multipliers keep the digit and take a hyphen** (`4x` → „4-mal“, `100x` → „100-mal“, Duden). English can wedge the
  aside into the middle of the comparison („4x slower for most connections (sometimes 100x) than …“); German has to keep
  `langsamer … als` together, so move the aside to the end of the sentence.
- **`button` on screen is „die Taste“**, not MS-terminology's „Schaltfläche“ (Windows convention); macOS de and the
  existing catalog both say „Klicke unten auf die Taste …“. Terms: `glossary.md`.
- **„auf ein Problem stoßen“, nicht „es ist ein Problem aufgetreten“.** Der Microsoft-Styleguide führt die
  `aufgetreten`-Form als Negativbeispiel; die `stoßen`-Form hält Cmdr im Nominativ und passt zur Stilregel „kein
  ‚Fehler‘“. Belege: `glossary.md` § Absturzdialog.
- **A toast value that lands AFTER a colon still has to be a whole sentence.** The `errors.eject.*` values are dropped
  into „{volumeName} ließ sich nicht auswerfen: …“ and „Trennen nicht möglich: …“, and the same value serves both
  frames. So write each one so it stands alone, and accept a repeated verb („… nichts zu trennen“ after „Trennen nicht
  möglich:“) rather than trimming the value to fit one frame. Worked set: `glossary.md` § Auswerfen und Trennen.
- **Don't translate „moving files“ literally when the message also covers copying and deleting.** `bewegen` is the NAME
  of Cmdr's Move command in German, so it reads as that one operation; use the catalog's `Vorgang` instead („Auf diesem
  Laufwerk läuft noch ein Vorgang von Cmdr.“).
- **Ein Menü im Fließtext heißt `das Menü „Hilfe“`, nicht `das Hilfe-Menü`.** macOS `de` verweist genau so auf seine
  Menüs („Wähle es aus, wähle im Menü „Ablage“ die Option „Informationen“ …“, Finder `BN43`). Den Menünamen selbst
  nimmst du aus `menu.*` im Katalog, nicht aus einer Direktübersetzung des Englischen: `Help` ist `Hilfe`, `File` ist
  `Ablage`. Belege: `glossary.md` § Native Menüs.
- **Den vollen Feature-Namen einmal nennen, danach das kurze Wort.** Der Katalog trennt `Absturzbericht` /
  `Fehlerbericht` (die Feature-Namen) von schlichtem `Bericht` (das Ding, über das der Dialog gerade spricht). Ein
  Dialogtitel nennt die Sache voll, die Tasten und Toasts darunter kürzen auf `Bericht`: das hält die Tasten schmal und
  liest sich nicht gestelzt. Worked sets: `glossary.md` § Absturzdialog, `glossary.md` § Fehlerbericht nachträglich
  ergänzen.
- **Eine Wortfamilie pro Dialog durchhalten.** Wenn ein Dialog eine Aktion trägt, nehmen Beschreibung, Taste,
  Fortschrittslabel und Bestätigungs-Toast denselben Stamm (`hinzufügen` → `Wird hinzugefügt …` → `hinzugefügt` →
  `kommt … hinzu`). Das Englische wechselt hier freier („add“, „join“); im Deutschen wirkt der Wechsel wie zwei
  verschiedene Vorgänge.
- **Metadaten heißen `…angaben`, nicht `…daten` oder `…details`.** Der Katalog nennt Dateimetadaten `Dateiangaben`
  („Dateiangaben, nicht der Inhalt“) und die EXIF-Daten eines Fotos entsprechend `Kameraangaben`. Neue Metadaten-Arten
  folgen dem Muster. Belege: `glossary.md` § Ask Cmdr schaut jetzt in Dateien hinein.
- **Ist der Referenz-Stapel auf dieser Maschine nicht da, gilt der dokumentierte Ersatz: die installierten macOS-Bundles
  direkt auslesen.** `_ignored/i18n/de/` liegt nur auf Davids Laptop; auf der M1-Agentenkiste fehlt es komplett, und das
  ist NICHT die Worktree-Falle (der Hauptklon hat dort gar kein `_ignored/`). Die Tier-1-Belege sind trotzdem
  erreichbar: `Finder.app/Contents/Resources/{en,de}.lproj/*.strings` über `plutil -convert json` und, seit macOS 26,
  `*.loctable` in AppKit/TCC/CoreTypes (eine Datei, alle Sprachen, `jq '.de'`). Rezepte:
  `../reference-pile/how-to-mine.md` § No pile on this machine? Mine the live macOS bundles instead. Fehlt ein Term dort
  auch, ist der Hersteller des Produkts die nächste Instanz (für Android-Begriffe Googles deutsche Doku), mit Abrufdatum
  notiert.
- **Die ausgegraute „busy“-Form eines Menüeintrags hängt ` (in Benutzung)` an den unveränderten Grundeintrag an.** Der
  Marker ist über alle `*Busy`-Schlüssel in `menu.json` derselbe und der Grundwortlaut bleibt zeichengleich, damit beide
  Zustände als ein Eintrag lesbar bleiben: `Auswerfen ({name}) (in Benutzung)`, `Trennen (in Benutzung)`,
  `Server vergessen (in Benutzung)`, `Gespeichertes Passwort vergessen (in Benutzung)`. Keinen zweiten Marker erfinden.
  Abgrenzung zur Glossarzeile `in use → in Verwendung`: die gilt für den Fließtext im Finder-Ton („Das Volume ist gerade
  in Verwendung …“), der Menü-Marker bleibt `in Benutzung`, weil `menu.volume.ejectBusy` ihn gesetzt hat.
- **Zwei Nomen mit verschiedenem Genus vertragen kein gemeinsames Pronomen.** Wo das Englische mit „it“ auf eine
  Aufzählung zurückzeigt („turn on a Mac or NAS … and Cmdr will find it“), braucht das Deutsche ein Oberbegriff-Nomen,
  weil `ein Mac` maskulin und `ein NAS` neutrum ist: „… und Cmdr findet **das Gerät**.“ (`servers.hub.emptyMessage`).
  Dieselbe Mechanik wie die `{name}`-Regel oben, nur ohne Platzhalter.
- **Ein Bedienelement trägt im Deutschen EINEN Namen, auch wenn das Englische zwei hat.** „volume chooser“ und „volume
  switcher“ meinen dasselbe Aufklappmenü und heißen beide `Volume-Auswahl`. Beim Übersetzen eines neuen Strings prüfen,
  ob der Katalog die Fläche schon benannt hat, statt das englische Synonym mitzuübersetzen.
- **Ein Zustandsetikett mit Ziel nimmt den `Verbindung … wird …`-Rahmen, nicht Apples Kurzform.** macOS rendert das
  zielose `Reconnecting…` als `Erneut verbinden …` (ScreenSharing, HomeDataModel, ConversationKit), und das ist als
  Etikett auch richtig. Sobald ein `{name}` dranhängt, bricht der Satzbau, also übernimmt der Katalog den Rahmen, den
  Apple selbst für die Langform nimmt: `Verbindung zu {name} wird wiederhergestellt …` neben dem Geschwister
  `Verbindung zu {name} wird hergestellt …`. Das Präfix `wieder-` allein trägt den Unterschied
  Erstverbindung/Rückholung. Belege: `glossary.md` § Der Wiederverbindungs-Zyklus.
- **„There''s nothing to X.“ → `Du musst nichts X-en.`** Das unpersönliche englische „there's nothing to …“ wird im
  Deutschen zur `du`-Entlastung, wie schon in `errors.listing.deviceReconnecting.suggestion` („There''s nothing to
  unplug.“ → „Du musst nichts abziehen.“). Für Tastatureingabe heißt das Verb `eingeben` (Apples Wort in genau diesem
  Dialog, NetAuthAgent `FS_MSG_PASS`), nicht `tippen`.
- **Drei Namen, drei Rollen: `Ask Cmdr`, `Cmdr`, `die KI`.** `Ask Cmdr` benennt NUR noch das Chat-Panel selbst (sein
  Titel, der Eintrag im Menü `Darstellung`, der Paletten-Befehl, die Settings-Rubrik, der Ein/Aus-Schalter) und bleibt
  dort unübersetzt. Sobald ein Satz nur beschreibt, was die KI tut, steht dort `Cmdr` (das Produkt handelt) oder
  `die KI` (die fremde Modell-Instanz, etwa in `suggestedOps.*`). Übersetze, was das Englische an der Stelle sagt, und
  ergänze `Ask Cmdr` NICHT aus alter Gewohnheit: „Was Cmdr sendet“, nicht „Was Ask Cmdr sendet“. Zusammensetzungen mit
  dem Panelnamen werden durchgekoppelt: `im Ask-Cmdr-Bereich`, `in den Ask-Cmdr-Einstellungen` (`Cmdr` steht darin als
  ganzes Wort, also greift `desktop-i18n-dont-translate` nicht).
- **`moves` als Nomen hat kein brauchbares deutsches Nomen.** `Bewegungen` liest sich als Fortbewegung, und
  `Verschiebungen` widerspricht dem Glossar-Verb `bewegen` (macOS Finder). Wo das Englische Operationen aufzählt
  („renames, moves, and cleanups“), bau die Aufzählung aus Verben: „Cmdr kann vorschlagen, Dateien umzubenennen, zu
  bewegen und aufzuräumen“ (`askCmdr.consent.contentsRule`). Das deckt sich mit der Stilregel „lieber ein Verb als ein
  Verbalsubstantiv“.
- **„Click to X“ hat zwei Formen, und die Länge entscheidet.** Eine kurze Nominalisierung nimmt „Zum X klicken“
  (`suggestedOps.indicatorTooltip` „Zum Ansehen klicken.“, `fileExplorer.navigation.spaceFetchFailed` „Zum erneuten
  Versuch klicken“); eine längere Verbalphrase nimmt „Klicken, um … zu …“ (`fileExplorer.breadcrumb.navigateTooltip`,
  `askCmdr.wake.needsFullDiskAccess` „Klicken, um den Festplattenzugriff einzurichten.“). Ein erzwungenes „Zum
  Einrichten des Festplattenzugriffs klicken“ würde den Terminus `Festplattenzugriff einrichten` in einen Genitiv
  auflösen, den `search.coverage.setUpFullDiskAccess` nicht kennt.
- **„to start chatting“ → „um loszuchatten“.** `askCmdr.error.noConsent` hatte die Wendung schon für genau dasselbe
  Englisch; `settings.askCmdr.provider.off` übernimmt sie, statt ein zweites Wort dafür zu erfinden.
- Record case-by-case rulings here.

## Decisions to confirm with David

The formality and move calls are now settled from the sources (see above); the only open items are subjective:

- **listing → Dateiliste** (tentative): no canonical source. Confirm whether "Dateiliste" or plain "Liste" reads best in
  Cmdr's context.
- **The stall wording** (tentative): no source names a stalled transfer at all (no Microsoft `stall` entry, no
  file-manager string), so "Kein Fortschritt seit {duration}" and "Die Übertragung kommt nicht mehr voran." are
  constructions. Details and the runners-up: `glossary.md` § Stalled transfer. Also worth an eye during the overflow
  check: the queue row's German is noticeably wider than the ETA text it replaces ("Kein Fortschritt seit 2 Min. 30 s"
  vs "noch 2 Min. 30 s").
- **`Autor` in `askCmdr.consent.contentsRule`** (tentative): the PDF metadata field is „Autor:in“ in Apple's German
  Preview inspector, and the gender-glyph ban (screen readers) rules that form out. Shipping the bare field name
  `Autor`; the neutral rewrites („wer es verfasst hat“, „Verfasserangabe“) read stilted inside the list. Confirm, or
  pick a rewrite. Evidence: `glossary.md` § Ask Cmdr schaut jetzt in Dateien hinein.
- **„AI suggestions are waiting.“ → „KI-Vorschläge warten auf dich.“** (`suggestedOps.indicatorTooltip`, tentative):
  nichts im Referenz-Stapel formuliert wartende Vorschläge, also ist die Wendung gemünzt. Die Alternative „Es liegen
  KI-Vorschläge bereit.“ klingt sachlicher und weniger nach Anstupsen; bestätige, welche im Statuseck besser wirkt.
- **`Klicken, um in den Einstellungen einen einzurichten.`** (`askCmdr.wake.needsApiKey`, tentative): das englische „set
  one up“ verweist mit `one` auf den Anbieter aus dem ersten Satz, und das Deutsche gibt das mit dem bloßen Pronomen
  `einen` wieder. Grammatisch einwandfrei, aber am Satzende etwas kahl; die Alternative wiederholt schlicht
  `einen Anbieter`. Bestätige, welche in einem Tooltip besser liest.
- **`Systemintegritätsschutz` vs Apple's on-screen `System-Integrationsschutz`** (product-voice call, currently shipping
  the first). `errors.mutation.sipProtected` uses Apple's German DOCUMENTATION name for System Integrity Protection.
  Apple's German Finder shows a different word in exactly one string, and it's a visible mistranslation (Integration
  instead of Integrität): `LocalizableMerged` `ET6`, "Einige Objekte im Papierkorb konnten aufgrund des
  **System-Integrationsschutzes** nicht gelöscht werden." (en `ET6` = "Some items in the Trash cannot be deleted because
  of System Integrity Protection."; re-verified on macOS 26.5.2, 2026-08-24). Term-choice principle 1 says match what
  the user sees in their Finder, which here would mean shipping Apple's typo. The recommendation is to keep
  `Systemintegritätsschutz`: Apple's string only ever appears when emptying the Trash (not Cmdr's surface, which is a
  rename refusal), both share the head `System…schutz` so a user who did see Apple's wording still recognizes ours, and
  Apple's own German support page (support.apple.com/de-de/102149) uses our form, so a user who searches for help lands
  in the right place. The counter-argument is real though, so David decides. Full evidence: `glossary.md` § the
  `Systemintegritätsschutz` row.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/de/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
