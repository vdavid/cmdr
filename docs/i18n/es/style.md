# Spanish (es) translation style guide

Working notes for translating Cmdr into Spanish. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Spanish.

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
  form; the `tú` imperative is for sentences that address the user. The one documented carve-out: when the English
  source is itself a bare noun or an elliptical phrase on a very tight button, a matching noun phrase can beat a longer
  infinitive. See the glossary's `Cola` / `En segundo plano` pair (§ El botón con la cola vacía).

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
- undo → deshacer · macOS AppKit MenuCommands ("Undo Smart Dash" → "Deshacer guion inteligente"), GNOME Nautilus
  ("Undo" → "Deshacer") · high
- put back (an item from the trash, to where it was) → devolver … a su sitio · the catalog's own `devolver`
  (`askCmdr.renameUndo.skipReason.failed.named`) plus `a su sitio` for "back where it was" · high. macOS Finder's
  command label is "Sacar de la papelera"; keep `restaurar` for undoing a RENAME. Full reasoning: `glossary.md` § El
  aviso de la papelera.
- trash → papelera · macOS Finder, shared with Windows · high
- add → añadir · macOS `es` throughout ("Añadir", "Añadir etiquetas…"); **`agregar` has zero hits** in the whole macOS
  `es` corpus, so it never wins here · high
- add to X (no direct object, "Add to report") → añadir a X · macOS Finder ("Añadir al Dock", "Añadir a favoritos",
  "Añadir a la barra lateral") licenses the same ellipsis Spanish would otherwise refuse · high
- the Help menu (referring to it in prose) → el menú Ayuda · macOS Finder `MenuBar` + AppKit ("Ayuda"), and
  `menu.bar.help` in the catalog; no quotes, matching macOS's "selecciona menú Apple > Ajustes del Sistema" · high
- view (look at something) → ver; show (reveal something) → mostrar · the catalog's own split (`menu.file.view` = "Ver",
  `commands.fileShowInFinder` = "Mostrar en el Finder") · high
- drive → unidad · the catalog throughout (`askCmdr.renameUndo.unavailable`) · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens
and `{email}`. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`).

## Plurals

CLDR categories: `one`, `many`, `other` (verified with `new Intl.PluralRules('es')`). Spanish nouns and articles carry
grammatical gender; article and adjective must agree with the counted noun in every branch. None of the crash-reporter
strings are counted, so no plural branches are needed there.

- Write all three branches even though `many` (compact forms like `1.000.000`) usually repeats `other` verbatim; the
  catalog already does this (`queue.toolbar.selectedCount`, `transferProgress.queuedToastCount`).
- **When the sentence continues past the counted noun, pull the WHOLE sentence into the branches.** English often leaves
  the tail outside the plural ("{n} files are still open and may already be partly written."); Spanish can't, because
  the tail's verb and participle agree with the count. `transferProgress.stallInFlight` is the worked example.
- **A counted tail with NO plural param has nothing to agree with.** A message that passes only the formatted
  `{somethingText}` for a second count gives you no integer to select on. English gets away with it ("stayed" fits 1 and
  12); Spanish doesn't. The fallback is to drop the conjugated verb and let the first half's noun carry the clause ("Se
  devolvieron 12 archivos a su sitio; 3 todavía en la papelera."). Ask for an integer partner instead where you can:
  `fileOperations.trash.undonePartial` gained a `{skipped}` driver for exactly this reason, so its second half now
  conjugates normally ("…; {skippedText} {skipped, plural, one {elemento se quedó} many {elementos se quedaron} other
  {elementos se quedaron}} en la papelera.").
- **A definite "the {countText} items" drops the numeral in the `one` branch.** `el 1 elemento` is ungrammatical in
  Spanish, so write `Se eliminó el elemento que Cmdr había creado.` for `one` and keep `{countText}` in `many`/`other`
  (`fileOperations.cancelRollback.doneDeleting`). The catalog already does it where English itself drops the count
  (`transferProgress.titleReversalDeleting`, "Deleting the file it created" → "Eliminando el archivo creado"), and
  `desktop-i18n-parity` compares the placeholder set of the WHOLE value, so the check still passes. Keep the article
  itself, though: it is often the only thing separating a "that was all of it" toast from its partial sibling.

## Notes and decisions

- **Los menús nativos siguen el texto del Finder, no el del catálogo.** Donde macOS tiene un equivalente, gana
  (`Visualización`, `Ocultar otras apps`, `Carpeta contenedora`, `Por omisión`), porque el usuario ve la barra de menús
  de Cmdr junto a la del Finder. Evidencia y excepciones: `glossary.md` § Menús nativos.
- Roster: Cmdr ships one pan-regional Spanish (archivo not fichero, avoid ordenador, ustedes-safe); a Spain variant
  (es-ES) is deferred. See `../language-selection-decisions.md`.
- **Quotation marks: macOS Spanish uses `“…”`** (curly), not `«…»`, in its UI strings (verified in `es/macOS/Finder/`,
  2026-06-19). Match macOS.
- **Inverted opening marks.** Questions open with `¿` and exclamations with `¡`. (No question/exclamation strings in the
  crash set.)
- **Ellipsis: copy the shape of the en value, character for character.** The in-progress button labels
  (`errorReporter.dialog.sending` = `Sending…`, `errorReporter.amend.submitting` = `Adding…`) use ONE Unicode `…`, and
  their `@key` descriptions say so, so the Spanish values do too: "Enviando…", "Añadiendo…". This also matches macOS,
  which writes "Enviando…". Check the actual en value before assuming: an older note here claimed three ASCII dots.
- **A just-finished action takes the preterite, not the peninsular compound perfect.** "Cmdr already sent this report" →
  `Cmdr ya envió este informe`, matching `fileExplorer.navigation.useSavedPasswordMessage` ("la contraseña que macOS ya
  guardó") and `crashReporter.dialog.body.ended` ("se cerró inesperadamente"). Spain would say "ya ha enviado"; the
  preterite is the form that reads right on both sides of the Atlantic, which is what the pan-regional base wants. A
  future `es-ES` variant is where the compound perfect belongs.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Speed multipliers (`4x`, `100x`) spell the "times" out: `4 veces más lenta`, `(a veces, 100 veces)`.** The `4x`
  notation belongs to English UI prose; Spanish writes `N veces`. Keep the figure in digits (not `cuatro`): these are
  comparative technical numbers and the digits carry the same punch they do in English. Worked example:
  `fileExplorer.network.osMountFallback.message`.
- **"for {duration}" (a stretch of time still running) → `desde hace {duration}`.** `durante` names a finished span and
  `hace` alone names a point in the past; only `desde hace` says "for the last X, and still". Applies to every
  elapsed-time line ("No progress for 45s" → "Sin progreso desde hace 45 s").
- **Never say something went wrong, say what is happening.** The ban on "error"/"failed" covers Spanish "error", "ha
  fallado", and "fallo" in these status lines. For a transfer that stalls, `ha dejado de avanzar` carries the fact
  without the verdict; `se ha detenido` / `se ha quedado parada` are also off-limits for a different reason (they read
  as "paused", which the queue labels `En pausa`). That second ban covers a PROGRESS line only: when the user really did
  stop something and the line reports the finished result, `detener` is the right verb and the one macOS uses ("Detener
  copia"), so `La reversión se detuvo después de …` (`fileOperations.cancelRollback.stopped*`) is fine. Naming the
  subject is what keeps it from reading as a pause.
- **Nothing may agree with a `{name}`.** The name comes off the disk and can be a file (`archivo`, masculine) or a
  folder (`carpeta`, feminine), so any clitic, article, or participle that agrees with it is wrong half the time. Put
  the item in the SUBJECT slot and use a verb that carries no gender ("{name} se quedó como está"), reach for a dative
  ("Cmdr no pudo devolverle su nombre anterior"), or say the noun outright when the key is folder-only ("La carpeta
  {name}"). The `askCmdr.renameUndo.skipReason.*` and `fileOperations.cancelRollback.reason.*` families are the worked
  examples. Watch the second constraint that usually rides along: a brand word in the English (`Cmdr`) has to survive
  the restructuring too (`desktop-i18n-dont-translate`), so "after Cmdr put it there" can drop neither the agreement nor
  the brand; it became `cambió después de que Cmdr terminara de escribir ahí`, with Cmdr as the subject of a subordinate
  clause and a place instead of an object pronoun.
- **Watch the quiet gendered words in emphatic English.** "yourself", "busy", "sure", "ready" all reach for a `-o`/`-a`
  adjective in Spanish and silently gender the reader. Restructure with the pronoun instead of the adjective: "Pick the
  folders yourself" → "Elige tú las carpetas" (not "Elige las carpetas tú mismo"); "while you're not busy" → "mientras
  no estás usando el Mac". Same rule as the gendered-grammar decision above, but the trap hides in ordinary emphasis
  rather than in role nouns.
- **Length: Spanish runs ~15–25% longer than English.** Overflow-check tight buttons ("Copiar", "Descartar", "Enviar
  informe") against the pseudolocale (`en-XA`). Watch `queue.row.stalled` in particular: "Sin progreso desde hace 45 s"
  is ~40% longer than the English it replaces, in the narrow ETA slot of an operation row.
- **Photos are "taken" with `tomar`, never `hacer` or `sacar`.** "where it was taken" → `el lugar donde se tomó`
  (`askCmdr.consent.contentsRule`). Spain says `hacer una foto` and Latin America `tomar una foto`; `tomar` reads fine
  on both sides, which is what the pan-regional base wants, and `sacar` is the colloquial one. Same shape as the `coste`
  / preterite decisions above. Evidence: `glossary.md` § Mirar dentro de un archivo.
- **`ejecutarse` names an OPERATION that keeps running, never the app.** The catalog already spends it on "Sigue
  ejecutándose en segundo plano" (`transferProgress.backgroundedToast`), so an app that survived a problem
  `siguió funcionando`. Putting `ejecutándose` right after `en segundo plano` would read as the operation, not Cmdr.
  Evidence: `glossary.md` § El diálogo de fallos.

## Decisions to confirm with David

- **`informe de error` vs `informe de fallos` is a live seam.** The `errorReporter.*` dialog says `informe de error`
  while the menu item that opens it (`menu.help.sendErrorReport`) says `Enviar informe de fallos…`, so the same feature
  has two names on screen. English has one ("error report") and one for crashes ("crash report"), so this is ours to
  fix, not a source problem. Copy written since then routes around it (`errorReporter.amend.unavailable` says "envía un
  informe nuevo" without naming the type). Pick one and sweep both families.
- **crash report → "informe de fallos"** (tentative): no single canonical source. "fallos" is the gentlest, most natural
  fit for Cmdr's non-alarmist voice; the more technical alternatives are "informe de bloqueos" (matches MS/macOS
  "bloqueo" for crash) or keeping it generic as "informe del problema". Confirm which reads best.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/es/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
