# Spanish (es) translation style guide

Working notes for translating Cmdr into Spanish. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Spanish.

`es` is the base (European-Spanish-leaning, because macOS base Spanish is es-ES). A region variant (`es-419`, `es-MX`,
…) would only carry overrides where Latin American usage diverges; the reference pile has `es-419`/`es-MX`/ `es-US`
folders if one is ever added.

## Formality: `tú`, settled

**Address the user as `tú`** (informal second person) throughout. This is settled from the sources, not a guess:

- macOS Spanish is fully informal. Across the mined Finder/AppKit strings, second-person address is overwhelmingly `tú`:
  176 `quieres`, 67 `puedes`, 43 `haz`, 40 `estás`, 31 `tus` vs essentially no formal address (the 30 `quiere` hits are
  third-person "someone/it wants", e.g. "Alguien quiere enviarte algo…", not polite "you"). Finder phrases the very
  string we need informally: "The last time you opened %@, it unexpectedly quit … Do you want to …?" → "La última vez
  que abriste %@, se cerró inesperadamente … ¿Quieres …?" (verified in `es/macOS/`, grep over Finder + AppKit,
  2026-06-19).
- Microsoft Spanish leans formal `usted` (Windows convention). That's not ours.
- Cmdr is a macOS app with a friendly voice that signs onboarding as David, so `tú` is both the macOS-native choice and
  the right tonal fit.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Spanish UI copy drifts long and formal; resist it. Prefer a verb over a
verbal noun ("Buscar", not "Realizar una búsqueda").

Error messages stay calm and actionable and never use the bare labels "error"/"failed": state the problem and a next
step. Note: macOS Spanish itself does say "Error interno" / "Se ha producido…"; Cmdr's voice rule is stricter, so don't
copy that pattern.

## Formality mechanics

- **`tú`**, throughout (see Formality above). Imperatives addressed to the user take the `tú` form ("Selecciona…",
  "Activa…"), matching macOS Finder ("Selecciona Continuar…", "activa Bluetooth").
- **Buttons and menu items: infinitive.** "Copiar", "Cancelar", "Enviar", "Eliminar", "Buscar". This is the macOS
  convention for action buttons/menu items (Finder/AppKit: "Copiar", "Cancelar", "Enviar"). The infinitive is the label
  form; the `tú` imperative is for sentences that address the user.

## Decision points

Formality is settled above (`tú`). The big remaining Spanish call is the regional variant.

- **Regional variant: target a neutral peninsular `es` base, defer a `es-419` Latin American variant.** Spanish splits
  into European/peninsular (`es-ES`) and Latin American (`es-419`, with `es-MX`, `es-AR`, etc. under it). All five
  majors maintain both: Apple ships "Español (España)" and "Español (Latinoamérica)"; Microsoft, Google, Netflix, and
  Spotify all offer a Spain Spanish and a Latin American Spanish. The differences that surface in a file-manager UI are
  narrow but real:
  - **Second-person plural**: Spain uses "vosotros" (informal plural); Latin America uses "ustedes" for both registers.
    Cmdr addresses one user as singular `tú`, so this rarely surfaces, but any "you all" phrasing must avoid "vosotros"
    if a single neutral string is the goal.
  - **A few core verbs/terms differ**: "ordenador" (Spain) vs "computadora"/"computador" (LatAm); "fichero" (Spain,
    older) vs the now-universal "archivo" (use "archivo" everywhere); "papelera" (trash) is shared. Picking LatAm-safe
    vocabulary keeps one base usable for most of the Spanish-speaking world.
  - Recommendation: write the `es` base in a neutral peninsular register that avoids Spain-only vocabulary and
    "vosotros", so it reads acceptably across regions; add a dedicated `es-419` variant only when a Latin American user
    flags something. Confidence: high. The single David-only call: whether Cmdr's primary Spanish audience is Spain or
    Latin America, which decides which way the neutral base leans. Flag for David.
- **Gendered grammar: prefer direct `tú`-address and neutral nouns; no "@"/"x"/"e" inclusive endings in UI.** Spanish
  agent nouns are gendered ("el usuario" / "la usuaria"). macOS and Microsoft Spanish both avoid gendering the user by
  using direct address ("Selecciona…", "¿Quieres…?") and neutral nouns ("la cuenta", "la persona"), and neither ships
  the inclusive "@"/"x"/"-e" endings ("usuari@s", "usuarixs", "usuaries") in core product UI. Recommendation: same here
  - direct `tú`-address and neutral nouns, no inclusive-ending experiments. Confidence: high.
- **Inverted opening marks and curly quotes** are covered under Notes; they're orthography, not a judgment call.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order is macOS (Tier 1) → Microsoft (Tier 2) →
GNOME/Xfce (Tier 3). Confidence: `confirmed` (human signed off), `high` (authoritative sources agree), `tentative`
(sources conflict or none had it).

- copy → copiar · macOS AppKit MenuCommands ("Copy"→"Copiar") · high
- copied → enviado/copiado pattern, here "Copiado" · macOS uses "Enviado" for the parallel sent-state badge; "Copiado"
  is the regular past participle · high
- send → enviar · macOS Finder AirDrop ("Enviar", "Enviando…", "Enviado") · high
- sending → enviando… · macOS Finder ("Enviando…") · high
- cancel → cancelar · macOS AppKit (29× "Cancelar") · high
- dismiss → descartar · macOS AppKit ("Descartar"); chosen over "Ignorar"/"Omitir"/"Cerrar" because it closes-without-
  acting, which "Descartar" conveys · high
- show details → mostrar detalles · macOS AppKit/NSExceptionAlert ("Show Details"→"Mostrar detalles", "Mostrar
  detalles") · high
- crash (verb, "quit unexpectedly") → cerrarse inesperadamente · macOS AppKit ("it unexpectedly quit"→"se cerró
  inesperadamente") · high
- crash (noun) → bloqueo · MS terminology ("crash"→"bloqueo", all regions incl. ESP/419); macOS NSExceptionAlert also
  uses "Bloqueo" · high. For a user-facing "crash report" Cmdr prefers the softer "informe de fallos" over "informe de
  bloqueos" (see below) · tentative
- report (noun) → informe · MS terminology ("report"→"informe", all regions incl. ESP/419); GNOME ("Informe de errores")
  · high
- crash report → informe de fallos · composed; "fallo" reads as the gentlest, most natural word for "something went
  wrong" in es UI and keeps Cmdr's non-alarmist voice (vs the more technical "bloqueo"). macOS has no single "crash
  report" string to copy. · tentative, confirm with David
- report ID → ID del informe · "ID" is kept as-is (macOS/MS both keep "ID"); "del informe" ties it to the report · high
- version → versión · MS terminology · high
- settings → Ajustes · macOS System Settings ("Ajustes del Sistema", "Ajustes") · high. (NOT "Configuración", which is
  the Windows term.)
- updates (the Settings section) → Actualizaciones · macOS uses "actualización/actualizaciones" for software updates;
  this is Cmdr's own in-app section name, kept consistent with the "Ajustes" naming · high
- email → correo · macOS uses "correo"/"correo electrónico"; "correo" alone is fine and shorter · high
- reply → responder · macOS ("responder") · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens
and `{email}`. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `many`, `other` (verified with `new Intl.PluralRules('es')`). Spanish nouns and articles carry
grammatical gender; article and adjective must agree with the counted noun in every branch. None of the crash-reporter
strings are counted, so no plural branches are needed there.

## Notes and decisions

- Roster: Cmdr ships one pan-regional Spanish (archivo not fichero, avoid ordenador, ustedes-safe); a Spain variant
  (es-ES) is deferred. See [`language-selection-decisions.md`](../language-selection-decisions.md).
- **Quotation marks: macOS Spanish uses `“…”`** (curly), not `«…»`, in its UI strings (verified in `es/macOS/Finder/`,
  2026-06-19). Match macOS.
- **Inverted opening marks.** Questions open with `¿` and exclamations with `¡`. (No question/exclamation strings in the
  crash set.)
- **Ellipsis in "Sending…".** The en catalog value is `Sending...` (three ASCII dots, per its `@key` description "Ends
  with three dots"), so the Spanish value uses three ASCII dots too: "Enviando...". macOS's own string is "Enviando…"
  (one Unicode char); we follow Cmdr's catalog convention, not macOS's, to keep the source/translation shapes aligned.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Length: Spanish runs ~15–25% longer than English.** Overflow-check tight buttons ("Copiar", "Descartar", "Enviar
  informe") against the pseudolocale (`en-XA`).

## Decisions to confirm with David

- **crash report → "informe de fallos"** (tentative): no single canonical source. "fallos" is the gentlest, most natural
  fit for Cmdr's non-alarmist voice; the more technical alternatives are "informe de bloqueos" (matches MS/macOS
  "bloqueo" for crash) or keeping it generic as "informe del problema". Confirm which reads best.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/es/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
