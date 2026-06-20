# German (de) translation style guide

Working notes for translating Cmdr into German. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
German.

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
- Keep direct address light; German UI often phrases neutrally ("Wird geladen…") where English would say "Loading your
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

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`). macOS UI names
Cmdr opens into (System Settings panes, "Papierkorb") should match a German macOS.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('de')`). Write both branches: "1 Datei" / "{count}
Dateien".

- Case agreement interacts with counts and with surrounding prepositions. A counted noun often sits in a case the
  English doesn't mark ("in 3 Ordnern", dative plural `-n`). Get the case right inside each branch.
- German has grammatical gender (der/die/das); article and adjective must agree with the counted noun in every branch.

## Notes and decisions

- **Nouns are always capitalized.** This is grammar, not title case. The app's sentence-case rule still holds (only the
  first word and nouns are capitalized), so "Datei umbenennen" but "Save"→"Speichern" at sentence start. Don't
  title-case adjectives/verbs.
- **Compound nouns concatenate** ("Dateiübertragung", "Netzwerkfreigabe"). This is correct German, but it lengthens
  strings: see Length below.
- **Quotation marks: `„…“`** (low opening, high closing) is the standard German form, and macOS uses it consistently
  ("Möchtest du „%@“ … bewegen?"). Avoid English `"…"`.
- **Length: German is the worst overflow risk of the three** (often 20–35% longer than English, plus long compounds).
  Overflow-check the layout hard against the pseudolocale (`en-XA`); look for clipped buttons, labels, and toasts.
- **Case-marked placeholders are a trap.** A `{name}` that lands in a genitive/dative slot can't be inflected by the
  catalog. Restructure the sentence so the placeholder stays nominative, or carries its own preposition.
- **Numbers and dates come from the formatter layer** (comma decimal, period/space thousands). Never hardcode
  separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

The formality and move calls are now settled from the sources (see above); the only open items are subjective:

- **listing → Dateiliste** (tentative): no canonical source. Confirm whether "Dateiliste" or plain "Liste" reads best in
  Cmdr's context.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/de/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
