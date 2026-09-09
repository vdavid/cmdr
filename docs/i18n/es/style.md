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
- file system / filesystem → sistema de archivos · macOS AppKit `DocumentDragging.loctable` ("could not be found in the
  file system" → "no se ha encontrado en el sistema de archivos", verified on macOS 26.6.2 build 25G83, 2026-09-06) ·
  high
- location (of a file or a binary on disk) → ubicación · macOS Finder `Localizable.strings` ("Location" → "Ubicación",
  plus the "Ubicación:" field label in `InfoWindowGeneralView` and `BulkRenameWindow`) · high
- path → ruta · macOS Finder `Toolbar.strings` ("Path" → "Ruta") and the catalog throughout · high
- empty (a field or value left blank) → vacío · macOS AppKit `Bindings.loctable` ("Value for %@ cannot be empty" → "El
  valor de %@ no puede estar vacío") · high
- debugging (a mode you switch on) → depuración · Apple's own Safari `es` UI ("Enable … debug mode" → "Activar modo de
  depuración …", verified on macOS 26.6.2, 2026-09-06); the catalog already uses "nivel de depuración"
  (`settings.developer.verboseLogging.description`) · high
- USB debugging (the Android developer setting) → depuración por USB · AOSP `main`,
  `SettingsLib/res/values-es/ strings.xml` `enable_adb` = "Depuración por USB" and SystemUI `usb_debugging_title` =
  "¿Permitir depuración por USB?"; `values-es-rUS` is identical, so the form is pan-regional (verified 2026-09-07).
  Matches what Apple + the catalog had already composed ("depuración" from Safari es, `por USB` from
  `settings.fileOperations.mtpConnectionWarning.description`) · high
- Allow (the button on Android's own "Allow USB debugging?" dialog) → Permitir · AOSP `main`, SystemUI
  `res/values-es/strings.xml` `usb_debugging_allow`; identical in `values-es-rUS` (verified 2026-09-07) · high
- tap (on a phone screen) → tocar · AOSP `es` throughout ("Toca el sensor", "Toca para activarlo"); macOS has no touch
  verb, so Android is the right source here · high
- over ADB / over USB (the transport in a label) → por ADB / por USB · the catalog's compact `por USB` pattern above;
  keep "a través de un cable USB" for the physical-cable sense (`settings.fileOperations.mtpEnabled.description`) · high
- Android phone → teléfono Android · the catalog throughout (`settings.fileOperations.mtpEnabled.description`,
  `errors.provider.macDroid.*`) · high
- turned on (a device or app setting) → activado / tenga activada … · the catalog
  (`errors.provider.macDroid.needsAction` "que el modo … esté activado en tu teléfono") · high
- command (a shell/CLI command) → comando · the catalog (`mtp.ptpcameradDialog.explanation` "ejecuta el siguiente
  comando en Terminal") · high. NOT "orden".
- Android platform tools → herramientas de plataforma de Android · composed; "herramientas" is the catalog's word for a
  tool set (`settings.developer.mcpPort.description`, `errors.provider.veraCrypt.serious`), and `Android` stays
  verbatim. Google's own es string is the longer "Herramientas de la plataforma del SDK de Android", which is too heavy
  for a settings row · tentative
- the usual way (where a program is normally found) → en los sitios habituales · "habitual" is the catalog's adjective
  for the default/expected case (`search.systemDirExclude.default` "las carpetas habituales del sistema") · high
- chat (una conversación con el asistente, y el panel que las guarda) → chat · the catalog itself
  (`askCmdr.threads.open` "Chats", `askCmdr.sessions.back` "Volver al chat", `askCmdr.consent.local` "Tus chats se
  quedan en tu Mac") · high. `conversación` stays available for the times English itself says "conversation"
  (`askCmdr.consent.proactive` "abre una conversación"); both are shipped and neither is wrong, the English picks which.
- AI / the AI → IA / la IA · the catalog (`ai.translateError.timeout.title` "La IA tardó demasiado", `parseError.title`
  "No se pudo leer la respuesta de la IA") · high
- AI features → funciones de IA · the catalog (`settings.ai.tooltipOff` "Las funciones de IA están desactivadas",
  `settings.ai.provider.description`, `onboarding.stepAi.intro`) · high
- (AI) provider → proveedor (de IA) · the catalog (`ai.translateError.unavailable.title` "No se puede acceder a tu
  proveedor de IA", `askCmdr.composer.providerOff`) · high
- Click to <verb> → Haz clic para <infinitivo> · the catalog (`fileExplorer.breadcrumb.navigateTooltip` "Haz clic para
  ir a {path}", `fileExplorer.navigation.spaceStillUnavailable` "Haz clic para reintentar") · high
- endpoint (an API address) → punto de conexión · MS terminology, entry "The logical representation of a location,
  typically expressed in URL form" (id 535789 → "punto de conexión", tagged for ESP, MEX, ARG, 419 and every other
  Spanish region, so it is pan-regional; mined from the reference pile 2026-09-09) · high. Label phrasing follows
  `endpoint address` → `dirección de punto de conexión` and `API Endpoint` → `punto de conexión de API`, so
  `Endpoint URL` is `URL del punto de conexión` (`onboarding.cloudSetup.step.endpoint`, the only key in any locale that
  carries the label). Apple is silent: `es/macOS/` publishes none of the three candidate words, so Tier 2 decides alone.
  **NOT "extremo"**, which the catalog carried for a while. `extremo` IS in the TBX, which is exactly why it's a trap:
  it renders the two senses that aren't ours (id 51058, the handle at the end of a line or arc; id 257427, a company in
  a B2B data exchange), and it is the FIRST bare `endpoint` hit in the file by id order. Five of the seven senses, ours
  included, read `punto de conexión`. This is `../reference-pile/how-to-mine.md` trap 4 (the first hit is often the
  wrong sense) caught in the act; disambiguate by reading the `<descrip type="definition">`, never by taking the top
  match.
- placeholder (the pre-filled example you replace in a field) → marcador de posición · MS terminology (ids 146442 and
  92736, mined 2026-09-09) · high
- deployment (Azure: the name you give a deployed model) → implementación · MS terminology (ids 44583, 542746, 1579560,
  1758508, 2133305, mined 2026-09-09) · high
- terminal (a shell window) → **el** terminal, masculine · the explorer-family corpora are unanimous ("del terminal" ×9,
  "un terminal" ×1, no feminine hit; mined 2026-09-09). Spain says `el terminal`, parts of Latin America say
  `la terminal`; the sourced masculine is what the pan-regional base ships · high

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
- **Un ítem de menú desactivado por estar en uso lleva ` (ocupado)` al final, y nada más.** La forma `…Busy` de un ítem
  repite palabra por palabra el texto del ítem base y le añade solo ese marcador: `Desconectar (ocupado)`,
  `Olvidar el servidor (ocupado)`, `Olvidar la contraseña guardada (ocupado)`, siguiendo a `menu.volume.ejectBusy`
  (`Expulsar ({name}) (ocupado)`), que fijó la convención. `ocupado` aquí concuerda con el volumen o el servidor, no con
  quien lee, así que la trampa de género de "busy" (más abajo) no se aplica; queda invariable en masculino singular en
  los cuatro ítems.
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
- **Two kinds of quotation mark, and they don't mix.** Curly `“…”` quotes a label Cmdr itself shows on screen
  (`settings.behavior.openTerminalHereApp.label` = "“Abrir terminal aquí” usa"). A literal token the English source
  already quotes (a command name, a value you type, an OS menu choice) keeps the source's straight `\"` so the two
  catalogs stay diffable: `settings.fileOperations.mtpEnabled.description` quotes \"Transferencia de archivos\", and
  `settings.fileOperations.adbEnabled.description` quotes \"adb\" the same way.
- **"Leave this empty and X happens" takes an imperative plus the future.** `Déjalo vacío y Cmdr buscará …`
  (`settings.fileOperations.adbBinaryPath.description`). The present (`y Cmdr busca`) reads as a statement of fact
  rather than a consequence; the imperative-plus-future is the idiomatic Spanish pairing.
- **A settings toggle's description reads the same as an imperative or a third person, so pick the form that works both
  ways.** `Accede a todo el sistema de archivos …`, `Detecta y conecta con dispositivos Android …`. This dodges the
  choice English makes for free and keeps the whole Settings screen in one register.
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
- **A "last X" column takes a NOUN phrase, not a participle.** macOS `es` names its date columns `Última apertura` (Last
  Opened) and `Fecha de modificación`, and the catalog already settled `Modified` → `Modificación`. So `Last used` →
  `Último uso`, not `Usado por última vez`. Evidence: `glossary.md` § La tabla del hub de servidores.
- **A command that toggles in both directions spells both verbs with `o`, never a slash.** English writes "Pin / unpin
  server" and "Toggle pin tab"; Spanish writes `Fijar o desfijar el servidor`, matching `commands.tabTogglePin.label`
  and the macOS `es` pattern `Activar o desactivar …`. The slash is an English shorthand, not a Spanish command-palette
  form.
- **`Añadir X` drops the article when X is new; commands acting on the thing under the cursor keep it.**
  `Añadir servidor…` (macOS: `Añadir personas`, `Añadir contraseña`) beside `Olvidar el servidor` /
  `Desconectar el servidor` (the catalog's own menu items). Both shapes are deliberate; don't unify them.
- **`ejecutarse` names an OPERATION that keeps running, never the app.** The catalog already spends it on "Sigue
  ejecutándose en segundo plano" (`transferProgress.backgroundedToast`), so an app that survived a problem
  `siguió funcionando`. Putting `ejecutándose` right after `en segundo plano` would read as the operation, not Cmdr.
  Evidence: `glossary.md` § El diálogo de fallos.
- **Un gerundio de progreso con destino nombrado va en reflexivo; sin destino, no.** `Connecting to X…` →
  `Conectándose a X…` y `Reconnecting to X…` → `Reconectándose a X…`, frente a `Connecting…` → `Conectando…` a secas. Lo
  fija Finder (`MN1`) y lo mantiene el par `servers.paneState.connecting` / `.reconnecting`, que el usuario ve en el
  mismo panel. macOS reparte el prefijo `re-` entre `Conectando de nuevo…` y `Reconectando…`, así que la elección entre
  ellos la decide el paralelismo interno, no la fuente. Evidence: `glossary.md` § Las dos líneas nuevas del panel.
- **`type` es `escribir`; `introducir` se reserva para las credenciales.** "nothing to type" →
  `no hay nada que escribir`, pero "type your username and password" sigue siendo `introduce tu usuario y contraseña`
  (`fileExplorer.network.*`). macOS hace el mismo reparto: `escribir` para el acto de teclear, `introducir` cuando el
  objeto es una contraseña o el valor de un campo.

- **"Ask Cmdr" names ONLY the chat panel, never the AI in general.** The brand survives where it names the surface
  itself: the panel title (`askCmdr.title`), the View-menu item (`menu.view.askCmdr`), the palette command
  (`commands.askCmdrToggle.label`), the settings section (`settings.section.askCmdr`), the switch that turns it on
  (`settings.askCmdr.turnOn` / `turnOff`), and any sentence pointing back at that section ("en los ajustes de Ask Cmdr",
  "en la sección Ask Cmdr"). Everywhere else the sentence describes what the product does, and the subject is **Cmdr**
  ("Cmdr observa las carpetas…", "Qué envía Cmdr") or, when it is the model rather than the app, **la IA** ("La IA
  sugirió esto", "Motivo de la IA"). English makes exactly this split key by key: follow it, don't put the brand back
  into a descriptive sentence and don't strip it from a pointer to the section.
- **A pointer to a macOS setting reuses the label of the button that opens it, word for word.** "Click to set up full
  disk access." ends as "Haz clic para configurar el acceso a todo el disco.", where "configurar el acceso a todo el
  disco" is exactly `search.coverage.setUpFullDiskAccess`: both surfaces lead to the same System Settings pane and have
  to name it the same way.
- **The preterite rule reaches the AI copy too.** `suggestedOps.description` and `suggestedOps.changedUnderReview` had
  shipped `ha sugerido` / `ha cambiado`; they are now `sugirió` / `cambió`, and `askCmdr.wake.thinking` says
  `lo que cambió`, not `lo que ha cambiado`. Same reasoning as the note above: the preterite reads right on both sides
  of the Atlantic, which is what the pan-regional base wants.

## Decisions to confirm with David

- **"Android platform tools" → "herramientas de plataforma de Android"** (tentative): no macOS or Microsoft source names
  this Android SDK component, and Google's own Spanish string is "Herramientas de la plataforma del SDK de Android",
  which is too long for a settings row. The short form is in `settings.fileOperations.adbEnabled.description` and
  `settings.fileOperations.adbBinaryPath.description`; both would change together if you prefer the full Google wording.
- **crash report → "informe de fallos"** (tentative): no single canonical source. "fallos" is the gentlest, most natural
  fit for Cmdr's non-alarmist voice; the more technical alternatives are "informe de bloqueos" (matches MS/macOS
  "bloqueo" for crash) or keeping it generic as "informe del problema". Confirm which reads best.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/es/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
