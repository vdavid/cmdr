# German (de) translation style guide

Working notes for translating Cmdr into German. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into German. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`), their rationale in `decisions.md`, and open questions in `review-queue.md`.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: informal lowercase `du` / `dich` / `dir` / `dein` everywhere, like macOS German (zero `Sie` in Finder or
  AppKit). Never Microsoft's or Google's `Sie`. Keep direct address light where German phrases neutrally („Wird geladen
  …“). Onboarding and About may speak as David in the first person, still with `du`.
- **Voice**: friendly, concise, active, calm. Prefer a verb to a verbal noun („Suchen“, not „Durchführen einer Suche“).
  Error copy states the problem and a next step and never uses `Fehler`, `fehlgeschlagen`, or `Fehlschlag` as a label:
  „Couldn't X“ → `X ließ sich nicht …` / `X nicht möglich`, a stopped operation → `Nicht abgeschlossen`, „Something went
  wrong“ → `Etwas ist schiefgelaufen`, „ran into a problem“ → `ist auf ein Problem gestoßen` (never
  `ist ein Problem aufgetreten`). Compounds like `Fehlerbericht` and `Tippfehler` are fine. No apologies where Cmdr made
  a deliberate choice; native regret (`Tut mir leid`) over the loanword `Sorry` where one is due.
- **Register by UI slot**:
  - buttons and menu items: infinitive, like Finder (`Umbenennen`, `Auswerfen`, `Abbrechen`), with the object first
    (`Server bearbeiten…`, `Zu Favoriten hinzufügen`);
  - progress lines: passive present with a space before the ellipsis (`Wird kopiert`, `Laufwerk wird durchsucht …`); a
    menu item or button that opens a dialog attaches the ellipsis with no space (`Einführung…`);
  - status chips: terse (`Wartet`, `Läuft`, `Angehalten`, `Fertig`, `Nicht abgeschlossen`);
  - a waiting line in body prose is a full sentence with a subject (`Cmdr wartet auf eine Antwort vom Ziel.`);
  - „Click to X“: short `Zum X klicken`, long `Klicken, um … zu …`.
- **Native menus and Apple names follow the German macOS**, even against the catalog's own wording: `Ablage` (File),
  `Darstellung` (View), `Gehe zu`, `Fenster`, `Hilfe`, `Dienste`, `Im Dock ablegen` (Minimize), `Zoomen` (Window >
  Zoom), `Widerrufen` / `Wiederholen` / `Einsetzen`, `Informationen` (Get Info), `geschützt` (Locked), `Übersicht`
  (Quick Look), `Schlüsselbund` / `Schlüsselbundverwaltung`, `Festplattenvollzugriff` (the pane's name),
  `Datenschutz & Sicherheit`, `Softwareupdate`, `Vorschau` (the app), the folder `Programme`, the Dock pair
  `Im Dock behalten` / `Aus dem Dock entfernen`. Kept English: Finder, Terminal, Spotlight, Mission Control, Spaces,
  Apple Account, Apple silicon. On a phone, Android's own German wins (`USB-Debugging`, `Erlauben`, `tippe auf`).
- **Capitalization**: sentence case, and all nouns capitalized (grammar, not title case). Don't lowercase a noun to
  match English.
- **Punctuation**: German quotes `„…“` around UI names and file names in running text, never `"…"`. Ellipsis is always
  the single `…`. A space before `%` and unit symbols (`42 %`, DIN 5008). Multipliers keep the digit with a hyphen
  (`4-mal`). ICU values double a straight apostrophe; RAW families (`errors.*`, `menu.*`) don't.
- **Menus and buttons in prose**: a menu is `das Menü „Hilfe“`, never `das Hilfe-Menü`; an on-screen button is
  `die Taste „+“`, never `Schaltfläche`. Quote the label byte-for-byte from the catalog.
- **Brand**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP` stay verbatim and take NO genitive-s: `die Oberfläche von Cmdr`,
  never `Cmdrs` (the don't-translate check reads it as a dropped brand). In a subordinate clause Cmdr is `es`.
  `Ask Cmdr` names only the chat panel; compounds couple through (`Ask-Cmdr-Einstellungen`). Prose about what the AI
  does says `Cmdr` or `die KI`.
- **Plurals**: CLDR `one` / `other`. Get case right inside each branch: a bare count phrase is nominative (`12 Ordner`),
  the dative `-n` needs a preposition (`in 3 Ordnern`, `von 12 Bildern`). Text after a plural block must work with both
  `ist` and `sind`.
- **Placeholders**: a `{name}`, `{path}`, `{app}`, `{server}` has no known gender, so never refer back with a pronoun or
  a possessive: repeat the noun (`das Objekt`, `der Server`), use a pronominal adverb (`darauf`, `darin`), or take the
  article (`am alten Ort`). Keep a placeholder out of case slots: nominative subject, or behind its own preposition
  (`Unter „{path}“ gibt es …`, `auf {volumeName}`, `namens {folderName}`). Two nouns of different gender take a generic
  noun, not a shared pronoun (`das Gerät` for „a Mac or NAS“).
- **Aria labels**: an `*Aria` value must contain its visible label verbatim; give the label the form the natural aria
  sentence needs (`Im Hintergrund` inside `Im Hintergrund weiterlaufen lassen`, `Anhalten` inside
  `Diesen Vorgang anhalten`).
- **One word per thing**: two keys with the same English get the same German (the term-consistency check enforces it),
  one control has one German name even where English has two (`Volume-Auswahl`), and one dialog keeps one word family
  (`hinzufügen` → `Wird hinzugefügt …` → `hinzugefügt`). Name a feature in full once (`Fehlerbericht`), then the short
  word (`Bericht`).
- **Top traps** (details in `terms.json`):
  - operation → `Vorgang` (m.: `diesen Vorgang`, `ihn`): `Vorgangswarteschlange`, `Vorgangsprotokoll`; `Operation` only
    for the protocol-level request and the Settings titles (`Dateioperationen`); transfer → `Übertragung` only where the
    English says transfer.
  - move → `bewegen`, never `verschieben`; but a message that also covers copy and delete says `Vorgang`. Putting files
    back where they were → `zurücklegen`; old names back → `zurücksetzen`.
  - item → `Objekt`, never `Element`; paste → `einsetzen`, never `einfügen`; save button → `Sichern` (its participle
    `gesichert`), storing data → `gespeichert`.
  - delete permanently → `endgültig`; `dauerhaft` only for a setting kept for good.
  - pause → `anhalten` / `Angehalten`, never the button noun `Pause` or `pausiert`; stop → `stoppen`, never `anhalten`;
    cancel → `abbrechen`, reserved for the Cancel action.
  - dismiss → `Schließen` for a dialog or toast, `Ausblenden` for a row or hint line hidden for good.
  - see/view → `ansehen` (the F3 action `Ansehen`); show → `anzeigen` / `einblenden`.
  - index → `indizieren`, never `indexieren`; scan → `durchsuchen` / `Durchlauf`, never `Scan`; browse into an archive
    or phone → `durchsehen`, the file-picker button → `Durchsuchen…`.
  - hidden files → `verborgen`; taken out of view → `ausgeblendet`.
  - drive → `Laufwerk` (what the user plugged in), volume → `Volume` (technical); disk image → `Image`.
  - pin → `fixieren` / `lösen` for tabs and servers, `im Dock behalten` / `aus dem Dock entfernen` for the Dock.
  - account → `Konto`; credentials → `Anmeldedaten`; the Enter key → `die Eingabetaste`, a terse hint `Enter`; Escape →
    `esc-Taste`.

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
- **Buttons and menu items: infinitive.** "Sichern", "Abbrechen", "Löschen", "Umbenennen", "Kopieren". This matches
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

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json`: `chosen`, accepted forms, usage
notes, forms to avoid with the reason, a confidence (`confirmed` / `high` / `tentative`), and sources. Tier order is
macOS (Tier 1) → Microsoft (Tier 2) → the file-manager catalogs (Tier 3); a vendor's own German UI (Apple, Android)
beats a `@key` description. Rationale worth more than a line sits in `decisions.md` under a heading that cites its keys,
and the term's `decision` field names that heading. German capitalizes all nouns (grammar), so a noun ruling stays
capitalized; a verb is lowercase in running text and capitalized as a button label. Never guess a term: mine the
reference pile first (`../reference-pile/how-to-mine.md`).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, plus the `{system_settings}`-style tokens. Enforced
by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`). macOS UI names Cmdr opens into
(System Settings panes, "Papierkorb") should match a German macOS. Quick Look is NOT verbatim: Apple's German macOS
calls it `Übersicht`, so Cmdr does too.

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
  der des Finders sieht. Belege und die Ausnahmen: `decisions.md` § Native Menüs.
- **Nouns are always capitalized.** This is grammar, not title case. The app's sentence-case rule still holds (only the
  first word and nouns are capitalized), so "Datei umbenennen" but "Save"→"Sichern" at sentence start. Don't title-case
  adjectives/verbs.
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
  („am alten Ort“). Belege und die Wortwahl: `decisions.md` § Der Toast nach einem abgebrochenen Vorgang.
- **Case-marked placeholders are a trap.** A `{name}` that lands in a genitive/dative slot can't be inflected by the
  catalog. Restructure the sentence so the placeholder stays nominative, or carries its own preposition.
- **German case marking is what breaks aria containment** (the shared rule: `../../guides/i18n-translation.md` § An
  `*Aria` key must contain its visible label). The natural label and the natural aria sentence often want different
  cases: `In den Hintergrund` is not inside "… im Hintergrund weiterlaufen lassen". Take the case the aria needs. Worked
  example: `decisions.md` § The progress dialog's empty-queue button.
- **Numbers and dates come from the formatter layer** (comma decimal, period/space thousands). Never hardcode
  separators.
- **A space goes before the percent sign** (`{percent} %`, "Auf 100 % zoomen"), per DIN 5008 and the rest of the
  catalog. English writes `50%`; German doesn't. Same for unit symbols after a number.
- **Multipliers keep the digit and take a hyphen** (`4x` → „4-mal“, `100x` → „100-mal“, Duden). English can wedge the
  aside into the middle of the comparison („4x slower for most connections (sometimes 100x) than …“); German has to keep
  `langsamer … als` together, so move the aside to the end of the sentence.
- **`button` on screen is „die Taste“**, not MS-terminology's „Schaltfläche“ (Windows convention); macOS de and the
  existing catalog both say „Klicke unten auf die Taste …“. Terms: `terms.json`.
- **„auf ein Problem stoßen“, nicht „es ist ein Problem aufgetreten“.** Der Microsoft-Styleguide führt die
  `aufgetreten`-Form als Negativbeispiel; die `stoßen`-Form hält Cmdr im Nominativ und passt zur Stilregel „kein
  ‚Fehler‘“. Belege: `decisions.md` § Absturzdialog.
- **A toast value that lands AFTER a colon still has to be a whole sentence.** The `errors.eject.*` values are dropped
  into „{volumeName} ließ sich nicht auswerfen: …“ and „Trennen nicht möglich: …“, and the same value serves both
  frames. So write each one so it stands alone, and accept a repeated verb („… nichts zu trennen“ after „Trennen nicht
  möglich:“) rather than trimming the value to fit one frame. Worked sets: `decisions.md` § Auswerfen und Trennen, sowie
  § Wer das Laufwerk festhält (die benannten Halter: App, Image, macOS, Cmdr selbst).
- **Don't translate „moving files“ literally when the message also covers copying and deleting.** `bewegen` is the NAME
  of Cmdr's Move command in German, so it reads as that one operation; use the catalog's `Vorgang` instead („Auf diesem
  Laufwerk läuft noch ein Vorgang von Cmdr.“).
- **Ein Menü im Fließtext heißt `das Menü „Hilfe“`, nicht `das Hilfe-Menü`.** macOS `de` verweist genau so auf seine
  Menüs („Wähle es aus, wähle im Menü „Ablage“ die Option „Informationen“ …“, Finder `BN43`). Den Menünamen selbst
  nimmst du aus `menu.*` im Katalog, nicht aus einer Direktübersetzung des Englischen: `Help` ist `Hilfe`, `File` ist
  `Ablage`. Belege: `decisions.md` § Native Menüs.
- **Den vollen Feature-Namen einmal nennen, danach das kurze Wort.** Der Katalog trennt `Absturzbericht` /
  `Fehlerbericht` (die Feature-Namen) von schlichtem `Bericht` (das Ding, über das der Dialog gerade spricht). Ein
  Dialogtitel nennt die Sache voll, die Tasten und Toasts darunter kürzen auf `Bericht`: das hält die Tasten schmal und
  liest sich nicht gestelzt. Worked sets: `decisions.md` § Absturzdialog, `decisions.md` § Fehlerbericht nachträglich
  ergänzen.
- **Eine Wortfamilie pro Dialog durchhalten.** Wenn ein Dialog eine Aktion trägt, nehmen Beschreibung, Taste,
  Fortschrittslabel und Bestätigungs-Toast denselben Stamm (`hinzufügen` → `Wird hinzugefügt …` → `hinzugefügt` →
  `kommt … hinzu`). Das Englische wechselt hier freier („add“, „join“); im Deutschen wirkt der Wechsel wie zwei
  verschiedene Vorgänge.
- **Metadaten heißen `…angaben`, nicht `…daten` oder `…details`.** Der Katalog nennt Dateimetadaten `Dateiangaben`
  („Dateiangaben, nicht der Inhalt“) und die EXIF-Daten eines Fotos entsprechend `Kameraangaben`. Neue Metadaten-Arten
  folgen dem Muster. Belege: `decisions.md` § Ask Cmdr schaut jetzt in Dateien hinein.
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
  Abgrenzung zur Termbase-Regel `in use → in Verwendung`: die gilt für den Fließtext im Finder-Ton („Das Volume ist
  gerade in Verwendung …“), der Menü-Marker bleibt `in Benutzung`, weil `menu.volume.ejectBusy` ihn gesetzt hat.
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
  Erstverbindung/Rückholung. Belege: `decisions.md` § Der Wiederverbindungs-Zyklus.
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
  `Verschiebungen` widerspricht dem Termbase-Verb `bewegen` (macOS Finder). Wo das Englische Operationen aufzählt
  („renames, moves, and cleanups“), bau die Aufzählung aus Verben: „Cmdr kann vorschlagen, Dateien umzubenennen, zu
  bewegen und aufzuräumen“ (`ai.cloudConsent.askCmdr.contentsRule`). Das deckt sich mit der Stilregel „lieber ein Verb
  als ein Verbalsubstantiv“.
- **„Click to X“ hat zwei Formen, und die Länge entscheidet.** Eine kurze Nominalisierung nimmt „Zum X klicken“
  (`suggestedOps.indicatorTooltip` „Zum Ansehen klicken.“, `fileExplorer.navigation.spaceFetchFailed` „Zum erneuten
  Versuch klicken“); eine längere Verbalphrase nimmt „Klicken, um … zu …“ (`fileExplorer.breadcrumb.navigateTooltip`,
  `askCmdr.wake.needsFullDiskAccess` „Klicken, um den Festplattenzugriff einzurichten.“). Ein erzwungenes „Zum
  Einrichten des Festplattenzugriffs klicken“ würde den Terminus `Festplattenzugriff einrichten` in einen Genitiv
  auflösen, den `search.coverage.setUpFullDiskAccess` nicht kennt.
- **„to start chatting“ → „um loszuchatten“.** Die entfernte Meldung askCmdr.error.noConsent hatte die Wendung schon für
  genau dasselbe Englisch; `settings.askCmdr.provider.off` übernimmt sie, statt ein zweites Wort dafür zu erfinden.
- **`pin`/`unpin` hat im Deutschen ZWEI Wortpaare, je nach Fläche.** Für Tabs und Server heißt es `fixieren` / `lösen`
  (`menu.tab.unpinTab`, `commands.serversTogglePin.label`, Safari `de` „Tab fixieren“); für das Dock heißt es
  `im Dock behalten` / `aus dem Dock entfernen`, weil Apples Dock-Menü genau dieses Paar führt
  (`Dock.app/…/DockMenus.strings` `KEEP_IN_DOCK`/`REMOVE_FROM_DOCK`). Beim Übersetzen eines neuen `pin`-Strings also
  erst schauen, um welche Fläche es geht. Belege: `decisions.md` § Das Dock-Angebot.
- **Wo Apple ein Kontextmenü für dieselbe Handlung hat, gewinnt sein Wortlaut auch im Fließtext.** Der Dock-Toast
  übernimmt `Im Dock behalten`, `Aus dem Dock entfernen` und `Zum Dock hinzufügen` zeichengleich aus dem Dock-Menü, das
  der Nutzer beim Rechtsklick auf dasselbe Symbol sieht. Dieselbe Logik wie bei den nativen Menüs oben, nur eine Ebene
  weiter: die Fläche muss nicht Cmdrs eigene sein.
- **Das Dock-Menü zählt zu den nativen Menüs und nimmt Apples Wortlaut, auch gegen Cmdrs eigene Menüleiste.** Beim
  Rechtsklick auf Cmdrs Dock-Symbol steht der Eintrag neben Apples Dock-Menü und Finders `Gehe zu`, also gewinnt deren
  Formulierung: `Go to folder…` → `Gehe zu Ordner…` (Finder), obwohl Cmdrs Menüleiste denselben Dialog `Zu Pfad gehen…`
  nennt. Das Englische unterscheidet die beiden Flächen genauso. Belege: `decisions.md` § Das Dock-Menü von Cmdr.
- **Zwei Schlüssel mit demselben englischen Wert bekommen EIN deutsches Wort.** `desktop-i18n-term-consistency` meldet
  jede Stelle, an der derselbe englische String zweimal verschieden übersetzt ist, und identische Werte tragen denselben
  `sourceHash`, sodass man es beim Übersetzen sofort sieht. Deshalb vor jedem Ein-Wort-Label kurz im `en`-Katalog nach
  demselben Wert greppen: `Save` steht in `onboarding.stepBeta.checklist.emailSave` UND `servers.sheet.save`, also
  heißen beide `Sichern`. Belege und die Abgrenzung `sichern`/`speichern`: `decisions.md` § Die Einführungs-Checkliste.
- **Ein einzeiliges Fazit neben einem Schalter darf nicht umbrechen.** Die `…summary`-Schlüssel stehen direkt unter dem
  Titel ihres Schalters, die Langfassung im Geschwister `…desc`. Also verbinitial bauen, jedes Wort weglassen, das der
  `desc` ohnehin trägt, und im Zweifel die kurze Wortform nehmen (`Platz` statt `Speicherplatz`). Deutsch läuft hier
  sonst 20–30 % über die englische Zeile.
- **Ein leeres Tag mitten im Satz (`<field></field>`) ist ein Bedienelement, kein Text.** Der Satz braucht eine Stelle,
  an der ein Kasten natürlich sitzt; ein trennbares Verb liefert sie („Gib deine E-Mail-Adresse <field></field> ein, um
  …“). Nicht das Tag ans Satzende schieben: das Englische setzt es bewusst in die Mitte.
- Record case-by-case rulings here.

## Open questions

Subjective calls, coined terms, and the overflow checks a native reviewer should confirm live in `review-queue.md`; each
already ships a reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: the rationale journal, one section per feature, headings citing their keys.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line.
