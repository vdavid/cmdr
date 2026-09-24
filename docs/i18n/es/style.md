# Spanish (es) translation style guide

Working notes for translating Cmdr into Spanish. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Spanish. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`, plus `concepts-proposed.json`), their rationale in `decisions.md`, and open
questions in `review-queue.md`.

`es` is the base (European-Spanish-leaning, because macOS base Spanish is es-ES). A region variant (`es-419`, `es-MX`,
…) would only carry overrides where Latin American usage diverges; the reference pile has `es-419`/`es-MX`/ `es-US`
folders if one is ever added.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: informal `tú` everywhere, like macOS Spanish (`¿Quieres…?`, `Selecciona…`, `abriste`). Sentences that
  address the user take the `tú` imperative (`Activa…`, `Haz clic…`); Microsoft's `usted` isn't ours.
- **Buttons, menu items, labels: infinitive** (`Copiar`, `Cancelar`, `Enviar informe`), macOS's own convention; dialog
  titles and yes/no questions too (`¿Revertir esta operación?`, `Añadir a tu informe de error`). Settings descriptions
  take a form that reads as both imperative and third person (`Accede a todo el sistema de archivos …`). Progress lines:
  a gerund with no subject (`Analizando…`, `Haciendo un análisis nuevo …`); with a named target the gerund is reflexive
  (`Conectándose a {name}…`). Tool-call "done" lines: impersonal `Se` + preterite (`Se buscó en tus fotos`).
- **One pan-regional base**: `archivo` (never `fichero`), no `vosotros`, `vale la pena` over `merece la pena`,
  `tomar una foto`, `coste` (catalog-settled). **A just-finished action takes the preterite**
  (`Cmdr ya envió este informe`, `Algo salió mal`, `No funcionó`), never the peninsular compound perfect; an operation
  still running keeps `ha escrito`. `ordenador` vs `equipo` is an open decision (`review-queue.md`); new copy says
  `tu equipo`.
- **Voice**: friendly, concise, calm, a verb over a verbal noun. Never the bare labels `error`, `fallo`, `falló`: say
  what happened and what to do. `X failed` / `Couldn't X` → `No se pudo X`; a stopped queue row →
  `No se pudo completar`; `Something went wrong` → `Algo salió mal`. A stalled transfer `ha dejado de avanzar` (❌
  `se ha detenido`, which reads as paused). "for {duration}" still running → `desde hace {duration}`.
- **Gender**: nothing may agree with an uncontrolled `{name}`, `{path}`, `{app}`, `{volumeName}`: put it in subject slot
  with a genderless verb (`{name} se quedó como está`), use a dative, `ahí`, `sin` + infinitive (`sigue sin indexar`),
  or say the noun (`la unidad`, `el archivo {fileName}`). Never gender the reader: no `listo/a`, `seguro/a`,
  `ocupado/a`, `conectado/a`, `tú mismo`; restructure (`Tienes acceso`, `cuando tengas conexión`,
  `Elige tú las carpetas`). People by role: `quien administre este Mac`, `quien posee el servidor`. No `@`/`x`/`-e`
  endings.
- **Capitalization**: sentence case. `Ajustes` is capitalized even for a lowercase English "settings" (`en Ajustes`);
  `papelera` is lowercase inside a sentence; the view modes are `la vista breve` / `la vista completa`. License tiers
  follow the English capital (`licencia Comercial` vs `suscripción comercial`).
- **Punctuation**: `¿…?` and `¡…!`. Quote a label Cmdr shows in curly `“…”` (`haz clic en “Eliminar”`), never `«…»`; a
  literal token the English already quotes (a command, a value, a foreign page title) keeps its straight `"…"`. Copy the
  English ellipsis per key (`…` or `...`). `{percentText}%` with no space. `4x` → `4 veces`. Menu paths mirror the
  English separator (`>`, `›`). ICU families double a straight apostrophe; RAW families (`menu.*`, `errors.*`) don't.
- **Plurals**: CLDR `one` / `many` / `other`, all three written (`many` repeats `other`). When the sentence continues
  past the counted noun, pull the WHOLE sentence into the branches (the tail agrees). A definite "the {countText} items"
  drops the numeral in `one` (`el elemento`, never `el 1 elemento`).
- **Brand and Apple names**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, `Safari` verbatim. `el Finder` always takes the
  article; `el Dock`, masculine; `Cmdr` is masculine for a pronoun (`fijarlo`). Localize what Apple localizes:
  `Vista rápida`, `Obtener información`, `Acceso a Llaveros`, `Utilidad de Discos`, `Vista Previa`, the folder
  `Aplicaciones`, the menus `Archivo`, `Edición`, `Visualización`, `Ir`, `Ventana`, `Ayuda`. `Ask Cmdr` names only the
  chat panel; prose about the product says `Cmdr`, about the model `la IA`. On a phone, Android's own Spanish wins
  (`Depuración por USB`, `Permitir`, `toca`).
- **Top traps** (details in `terms.json`):
  - delete → `Eliminar`; clear (and Cmdr's own leftovers) → `borrar`; remove from a list → `Quitar`; forget → `olvidar`;
    dismiss a notice → `Descartar`.
  - move → `Mover` (❌ Finder's `trasladar`); rename → `renombrar`, also in prose; its noun → `el cambio de nombre`.
  - default → `por omisión` (❌ `predeterminado`); settings → `Ajustes` (❌ `Configuración`); add → `añadir` (❌
    `agregar`); click → `hacer clic`, press a KEY → `pulsar`; Enter → `Intro`.
  - file → `archivo`; archive → `archivo comprimido`; item → `elemento`; drive → `unidad`, but the drive being ejected →
    `disco`; parent folder → `carpeta superior`; root folder → `carpeta raíz`; home folder → `carpeta de inicio`.
  - operation queue → `Cola de operaciones`; queued → `Esperando` (no other status starts with it); rollback →
    `Revertir` / `la reversión`.
  - crash report → `informe de fallos`; error report → `informe de error`.
  - quit (the user) → `Salir`; the app ending itself → `cerrarse`; Try again as a button → `Reintentar`, in prose
    `inténtalo de nuevo`; stop → `Detener`.
  - show → `Mostrar`; see / the F3 action → `Ver`; the View menu → `Visualización`.
  - full disk access, naming the setting → `acceso total al disco`; pin → `fijar` / `desfijar`.

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
  infinitive. See the `Cola` / `En segundo plano` pair (`decisions.md` § El botón con la cola vacía).

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
    Latin America, which decides which way the neutral base leans (`review-queue.md`).
- **Gendered grammar: prefer direct `tú`-address and neutral nouns; no "@"/"x"/"e" inclusive endings in UI.** Spanish
  agent nouns are gendered ("el usuario" / "la usuaria"). macOS and Microsoft Spanish both avoid gendering the user by
  using direct address ("Selecciona…", "¿Quieres…?") and neutral nouns ("la cuenta", "la persona"), and neither ships
  the inclusive "@"/"x"/"-e" endings ("usuari@s", "usuarixs", "usuaries") in core product UI. Recommendation: same here:
  direct `tú`-address and neutral nouns, no inclusive-ending experiments. Confidence: high.
- **Inverted opening marks and curly quotes** are covered under Notes; they're orthography, not a judgment call.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` (and this locale's
`concepts-proposed.json` until the lead merges it): `chosen`, accepted forms, usage notes, forms to avoid with the
reason, a confidence (`confirmed` / `high` / `tentative`), and sources. Tier order is macOS (Tier 1) → Microsoft
(Tier 2) → the file-manager catalogs (Tier 3); a vendor's own Spanish UI (Apple, Android, GitHub) beats a `@key`
description. Rationale worth more than a line sits in `decisions.md` under a heading that cites its keys, and small
rulings with no concept of their own are in `decisions.md` § Rulings that have no concept of their own. Never guess a
term: mine the reference pile (`_ignored/i18n/es/`; recipes in `../reference-pile/how-to-mine.md`) and add the ruling to
`terms.json` as you settle it.

Two mining traps this locale hit, worth knowing before the next one:

- **The first TBX hit is often the wrong sense.** `endpoint` has seven senses in the Microsoft TBX, and the first bare
  hit by id order is `extremo` (the handle at the end of a line, id 51058), which the catalog carried for a while. Ours
  (id 535789, "the logical representation of a location, typically expressed in URL form") is `punto de conexión`, like
  five of the seven. Disambiguate by reading `<descrip type="definition">`, never by taking the top match.
- **A regional split can hide in a gender.** `terminal` (a shell window) is masculine in the explorer-family corpora
  (`del terminal` ×9, no feminine hit, 2026-09-09); parts of Latin America say `la terminal`. The pan-regional base
  ships the sourced masculine.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Safari, plus the `{system_settings}`-style tokens and
`{email}`. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.ts`). Quick Look is
NOT on that list: Apple localizes it (`Vista rápida`), so translate it to match the user's macOS.

## Plurals

CLDR categories: `one`, `many`, `other` (verified with `new Intl.PluralRules('es')`). Spanish nouns and articles carry
grammatical gender; article and adjective must agree with the counted noun in every branch.

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
  (`Visualización`, `Ocultar otras apps`, `Por omisión`), porque el usuario ve la barra de menús de Cmdr junto a la del
  Finder. La excepción es `Carpeta superior`, que traduce la frase de Cmdr («Parent folder») y no la de Apple
  («Enclosing Folder»). Evidencia: `decisions.md` § Menús nativos.
- **El menú del Dock tiene su propia fuente Tier 1, y no es el Finder.** Los ítems que salen al hacer clic derecho sobre
  el icono de una app los escribe el Dock, así que la referencia es
  `/System/Library/CoreServices/Dock.app/Contents/Resources/es.lproj/DockMenus.strings` (verificado en macOS 26.6.2,
  2026-09-09). De ahí sale el patrón «verbo + nombre de la app» sin preposición ni artículo (`Mostrar %@`,
  `Ocultar %@`), que es el que sigue `menu.dock.openCmdr` = `Abrir Cmdr`. Evidencia: `decisions.md` § El menú del icono
  en el Dock.
- **`folder` es `carpeta` y `path` es `ruta`, aunque los dos comandos lleven al mismo diálogo.** `menu.dock.goToFolder`
  dice `Ir a la carpeta…` (lo que dice el Finder) y `goToPath.dialog.title` dice `Ir a la ruta` (lo que dice su inglés).
  Frontera deliberada: cada clave sigue a su propia fuente, no se unifican.
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
- **Inverted opening marks.** Questions open with `¿` and exclamations with `¡`.
- **Ellipsis: copy the shape of the en value, character for character.** The in-progress button labels
  (`errorReporter.dialog.sending` = `Sending…`, `errorReporter.amend.submitting` = `Adding…`) use ONE Unicode `…`, and
  their `@key` descriptions say so, so the Spanish values do too: "Enviando…", "Añadiendo…". This also matches macOS,
  which writes "Enviando…". Check the actual en value before assuming: an older note here claimed three ASCII dots.
- **A just-finished action takes the preterite, not the peninsular compound perfect.** "Cmdr already sent this report" →
  `Cmdr ya envió este informe`, matching `fileExplorer.navigation.useSavedPasswordMessage` ("la contraseña que macOS ya
  guardó") and `crashReporter.dialog.body.ended` ("se cerró inesperadamente"). Spain would say "ya ha enviado"; the
  preterite is the form that reads right on both sides of the Atlantic, which is what the pan-regional base wants. A
  future `es-ES` variant is where the compound perfect belongs. The rule covers the AI copy too
  (`suggestedOps.description` `sugirió`, `askCmdr.wake.thinking` `lo que cambió`) and the catch-all `Algo salió mal`.
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
  no estás usando el Mac"; "once you're connected" → "cuando tengas conexión". Same rule as the gendered-grammar
  decision above, but the trap hides in ordinary emphasis rather than in role nouns.
- **Length: Spanish runs ~15–25% longer than English.** Overflow-check tight buttons ("Copiar", "Descartar", "Enviar
  informe") against the pseudolocale (`en-XA`). Watch `fileOperations.transferProgress.stallNotice` in particular: "Sin
  progreso desde hace 45 s" runs ~40% longer than the English, and the one key renders on two surfaces, the progress
  dialog and the narrow ETA slot of a queue row. Fit the narrow one; the dialog then shows the same short text, and
  there is no separate row string to trim on its own.
- **Photos are "taken" with `tomar`, never `hacer` or `sacar`.** "where it was taken" → `el lugar donde se tomó`
  (`ai.cloudConsent.askCmdr.contentsRule`). Spain says `hacer una foto` and Latin America `tomar una foto`; `tomar`
  reads fine on both sides, which is what the pan-regional base wants, and `sacar` is the colloquial one. Same shape as
  the `coste` / preterite decisions above. Evidence: `decisions.md` § Mirar dentro de un archivo.
- **A "last X" column takes a NOUN phrase, not a participle.** macOS `es` names its date columns `Última apertura` (Last
  Opened) and `Fecha de modificación`, and the catalog already settled `Modified` → `Modificación`. So `Last used` →
  `Último uso`, not `Usado por última vez`. Evidence: `decisions.md` § La tabla del hub de servidores.
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
  Evidence: `decisions.md` § El diálogo de fallos.
- **Un gerundio de progreso con destino nombrado va en reflexivo; sin destino, no.** `Connecting to X…` →
  `Conectándose a X…` y `Reconnecting to X…` → `Reconectándose a X…`, frente a `Connecting…` → `Conectando…` a secas. Lo
  fija Finder (`MN1`) y lo mantiene el par `servers.paneState.connecting` / `.reconnecting`, que el usuario ve en el
  mismo panel. macOS reparte el prefijo `re-` entre `Conectando de nuevo…` y `Reconectando…`, así que la elección entre
  ellos la decide el paralelismo interno, no la fuente. Evidence: `decisions.md` § Las dos líneas nuevas del panel.
- **Pulsar un botón es `hacer clic en`, no `pulsar`.** macOS `es` escribe `haz clic` 44 veces frente a 2 de `pulsa`
  (grep sobre Finder + AppKit + SystemSettings, 2026-09-09), y el catálogo ya lo sigue
  (`fileExplorer.breadcrumb.navigateTooltip`). Un inglés coloquial tipo «hit Save again» se traduce igual:
  `vuelve a hacer clic en Guardar` (`onboarding.stepBeta.signup.rejected`). `pulsar` queda para las TECLAS
  (`Pulsa Intro para buscar`).
- **Un nombre de botón que llega por marcador (`{nextLabel}`) va entre comillas curvas.** Es una etiqueta que Cmdr pinta
  en pantalla, así que cae del lado curvo de la frontera de comillas de más arriba, aunque el inglés use las rectas.
  Worked example: `onboarding.stepAi.missingKeyWarning`. Un literal ajeno (el título `"Cmdr"` de una página de
  AlternativeTo, la etiqueta `"Red local"` del panel de privacidad de macOS) conserva las rectas del inglés.
- **Un resumen de una línea junto a un interruptor no puede envolver, así que se recorta antes que alargarse.** Las
  cuatro claves `onboarding.stepOptional.*.summary` son telegráficas en inglés y lo siguen siendo en español: verbo en
  tercera persona, lista con comas, sin subordinadas («Ocupa 1 GB, acelera las búsquedas y muestra tamaños de carpeta»).
  Toman la terminología de su clave hermana `…desc`, que es la versión larga que el usuario abre justo al lado.
- **`type` es `escribir`; `introducir` se reserva para las credenciales.** "nothing to type" →
  `no hay nada que escribir`, pero "type your username and password" sigue siendo `introduce tu usuario y contraseña`
  (`fileExplorer.network.*`). macOS hace el mismo reparto: `escribir` para el acto de teclear, `introducir` cuando el
  objeto es una contraseña o el valor de un campo.
- **"Ask Cmdr" names ONLY the chat panel, never the AI in general.** The brand survives where it names the surface
  itself: the panel title (`askCmdr.title`), the View-menu item (`menu.view.askCmdr`), the palette command
  (`commands.askCmdrToggle.label`), the settings section (`settings.section.askCmdr`), the switch that turns it on
  (`settings.askCmdr.enabled.label`), and any sentence pointing back at that section ("en los ajustes de Ask Cmdr", "en
  la sección Ask Cmdr"). Everywhere else the sentence describes what the product does, and the subject is **Cmdr**
  ("Cmdr observa las carpetas…", "Qué envía Cmdr") or, when it is the model rather than the app, **la IA** ("La IA
  sugirió esto", "Motivo de la IA"). English makes exactly this split key by key: follow it, don't put the brand back
  into a descriptive sentence and don't strip it from a pointer to the section.
- **A pointer to a macOS setting reuses the label of the button that opens it, word for word.** "Click to set up full
  disk access." ends as "Haz clic para configurar el acceso a todo el disco.", where "configurar el acceso a todo el
  disco" is exactly `search.coverage.setUpFullDiskAccess`: both surfaces lead to the same System Settings pane and have
  to name it the same way.
