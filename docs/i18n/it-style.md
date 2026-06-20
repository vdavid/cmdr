# Italian (it) translation style guide

Working notes for translating Cmdr into Italian. Read [`README.md`](README.md) for how this fits the translation process,
and the app-wide [`/docs/style-guide.md`](../style-guide.md) for the English voice these notes carry into Italian.

## Voice and tone

Friendly, concise, active, calm. Italian UI copy from Apple reads natural and direct, not stiff. Error messages stay
calm and actionable and never use a bare "Errore"/"Operazione non riuscita" as a label: state the problem and a next
step. Note macOS Italian itself does use "Impossibile…" freely; Cmdr's voice is fine with that pattern ("Impossibile
rinominare il file. Riprovare?"), it reads as calm and is not the forbidden "error/failed" wording.

## Formality

**Verdict: informal `tu`, not `Lei`.** Consumer brands (IKEA, Spotify, Netflix, and peers) address Italian users
informally, which fits Cmdr's friendly personal voice. macOS itself dodges the choice via the infinitive, but where
Cmdr does address the user, the register is `tu`. Formality decision recorded in
[`formal-informal-decisions.md`](formal-informal-decisions.md).

- Buttons and menu items: imperative second-person-singular form, which in Italian looks like the verb stem ("Annulla",
  "Apri", "Elimina", "Sposta", "Copia", "Rinomina", "Cerca"). This matches macOS Finder exactly.
- Confirmation prompts: the infinitive ("Eliminare 3 elementi?") reads clean and Apple uses it, but `tu` ("Vuoi
  eliminare…") is correct and on-brand where a personal address fits. Never `Lei` ("Desidera eliminare…").
- Where a sentence needs a pronoun, use `tu`.

## Decision points

### Regional variant: standard Italian (it), single target
- Options: Italy standard (it / it-IT) vs Swiss Italian (it-CH).
- Majors: Apple and Microsoft both ship one Italian (it / it-IT); it-CH differs almost only in number/quote formatting,
  which the formatter layer handles, not the strings.
- Recommendation: target plain `it`. No region split needed. Confidence: high.

### Gender agreement (the real trap)
- Italian adjectives, articles, and past participles agree in gender and number with the noun. A generic
  file-or-folder entity is hard to phrase: "selezionato" (m) vs "selezionata" (f), "eliminato" vs "eliminata".
- Majors: Apple uses "elemento" (m, "elementi selezionati", "elementi eliminati") as the neutral generic term, which
  fixes agreement to masculine and dodges the problem. Microsoft does the same with "elemento".
- A counted placeholder like `{count} {item}` where `{item}` could be "file" (m) or "cartella" (f) means surrounding
  adjectives can't agree. Restructure so the adjective sits next to a known-gender noun, or use the neutral "elemento".
- Recommendation: adopt "elemento" (m) as the generic file-or-folder term, the macOS convention; this makes most
  agreement masculine and predictable. Confidence: high.

### Apostrophe / elision and ICU escaping
- Italian elides constantly: "l'elemento", "dell'archivio", "un'immagine". Every literal apostrophe in an ICU value
  must be doubled (`'` becomes `''`) or ICU swallows the text. This is the single highest-frequency mechanical risk for
  Italian. Confidence: high (mechanical fact).

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order: macOS (Tier 1) → Microsoft (Tier 2) →
GNOME/Xfce (Tier 3). Confidence: `confirmed` (human signed off), `high` (sources agree), `tentative` (conflict or
none). All from the mined `it/macOS/` Finder + AppKit unless noted.

- file → file (invariable, m; plural also "file") · macOS, MS · high
- folder → cartella (f) · macOS Finder · high
- directory → directory (f, invariable; technical) · MS · high
- drive → unità (f) · MS; macOS often "disco" for a disk · high
- item (generic file-or-folder) → elemento (m) · macOS ("elementi selezionati", "elementi eliminati") · high
- trash → Cestino (m) · macOS Finder ("Sposta … nel Cestino", "Svuota Cestino") · high
- delete → eliminare ("Elimina") · macOS · high
- copy → copiare ("Copia") · macOS · high
- move → spostare ("Sposta") · macOS Finder ("Sposta gli elementi nel Cestino") · high
- move to trash → spostare nel Cestino · macOS · high
- rename → rinominare ("Rinomina") · macOS ("Rinomina ${target} in …") · high
- open → aprire ("Apri") · macOS · high
- search → cercare ("Cerca") / trovare; Finder uses "Trova file e cartelle" · macOS · high
- cancel → annullare ("Annulla") · macOS · high
- replace / overwrite → sostituire ("Sostituisci") · macOS · high
- settings → Impostazioni (f pl) · macOS ("Impostazioni") · high
- preferences → Preferenze (f pl) · macOS · high
- volume → volume (m) · macOS keeps "Volume" for a mounted disk volume · high
  - Do NOT use "volume" in the audio-loudness reading; here it is the disk-volume sense, same word, fine.
- disconnect → disconnettere ("Disconnetti") · macOS · high
- eject → espellere ("Espelli") · macOS Finder · high (verify exact form against `it/macOS/` if a label needs it)
- server → server (m, invariable) · macOS ("Connessione al server") · high
- tab → scheda (f) · macOS uses "scheda" for a window tab; "Tabulazione" in the AppKit hit is the tab-key/indent sense,
  do NOT use it for a UI tab · high
- bookmark → segnalibro (m) · MS, common Apple usage · high
- sidebar → barra laterale (f) · macOS Finder · high
- sort → ordinare ("Ordina") · macOS sort UI · high
- pane → riquadro (m) · macOS uses "riquadro" for a window panel/area · tentative (confirm reads well for a file pane)
- listing → elenco (m) / elenco file · no single canonical source · tentative
- transfer → trasferimento (m) · MS · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`. macOS pane names Cmdr opens into should match an Italian macOS ("Cestino",
"Impostazioni di Sistema").

## Plurals

CLDR categories: `one`, `many`, `other` (verified `new Intl.PluralRules('it')`, 2026-06-20). The `many` category is for
compact/large-number forms; cover `one` and `other` at minimum and add `many` when a message can show large counts. All
branches must agree in gender with the counted noun (use "elemento" to keep it masculine).

## Notes and decisions

- **Capitalization: sentence case.** Apple Italian capitalizes only the first word and proper nouns ("Sposta gli
  elementi nel Cestino", not title case). "Cestino", "Finder", "Impostazioni di Sistema" stay capitalized as names.
  Cmdr's sentence-case rule matches Apple here.
- **Quotation marks: `«…»`** (guillemets) are standard Italian and Apple uses them ("Rinomina ${target} in «${newName}»"
  appears as curly quotes in the mined string; guillemets are the print standard, curly `"…"` also acceptable). Avoid
  straight `"…"`.
- **Length:** Italian runs ~15–25% longer than English. Overflow-check against the pseudolocale (`en-XA`).
- **Numbers/dates** come from the formatter layer (comma decimal, period thousands). Never hardcode separators.
- **ICU apostrophe doubling** is critical for Italian (frequent elision); see Decision points.

## Decisions to confirm with David

- **pane → riquadro** and **listing → elenco** (tentative): confirm both read naturally for a two-pane file manager;
  "riquadro" is the macOS panel word but a file pane is a specific concept.
