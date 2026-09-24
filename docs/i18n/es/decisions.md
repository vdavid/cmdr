# es decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`, plus `concepts-proposed.json`); open questions for a native reviewer live in `review-queue.md`.
Style and voice: `style.md`.

## Rulings that have no concept of their own

Settled once and still binding, but too small or too local for a shared concept. Sources in parentheses.

- **Settings and cards**: Appearance → Apariencia (macOS says Aspecto for its own pane; Apariencia reads better as a
  section title); Colors and formats → Colores y formatos; Zoom and density → Zoom y densidad; File and folder sizes →
  Tamaños de archivos y carpetas; Listing → Lista; Behavior → Comportamiento; File system watching → Vigilancia del
  sistema de archivos; Developer → Desarrollador; MCP server → Servidor MCP; Updates & privacy → Actualizaciones y
  privacidad; Advanced → Avanzado; Navigation & file ops → Navegación y operaciones de archivos (es has no terse "ops").
  Multi-word ones are tentative.
- **Option labels without a source**: Smart / Dynamic / Content / On disk / Rainbow / Wilting → Inteligente / Dinámico /
  Contenido / En disco / Arcoíris / Marchitamiento (tentative). Western (encoding group) → Occidental (Apple's encoding
  submenu, tentative). Detected → Detectada/Detectado, agreeing with the noun.
- **Tight abbreviations**: dir / DIR stay short in the status bar and size column; relative time "m/h/d/w/mo/y ago" →
  `hace {count} min/h/d/sem/mes/a` (es has no single-letter convention; tentative); time left s / m → `s` / `min`.
- **Status words**: restarting / starting / running / stopped (local AI server) → Reiniciando... / Iniciando... / En
  ejecución / Detenido; in memory / indexed (viewer badges) → en memoria / indexado; roughly → aproximadamente; almost
  done → Casi listo; Later → Más tarde.
- **Shortcuts**: in-app (scope) → en la app; combo → combinación (macOS combinación de teclas); register (a global
  hotkey) → registrar; Keyboard shortcuts → Atajos de teclado.
- **Error reporter**: manifest → Manifiesto (tentative); reference ID → ID de referencia; report ID → ID del informe;
  log bundle → paquete (tentative); redact → depurar (tentative, over MS tachar and ocultar).
- **Licensing**: license key → clave de licencia; perpetual → perpetua; valid until / expired on → válida hasta el /
  caducó el; subscription → suscripción; renew → renovar; organization → organización.
- **Viewer**: streaming → transmisión / transmitiendo (tentative); the word-wrap badge → ajuste; tail → Seguir
  (tentative); reindex → reindexar / Reindexando… (keeps the source's single `…`).
- **Onboarding copy**: pros and cons → pros y contras, Pro: / Con: → A favor: / En contra: (tentative); issues (GitHub)
  → incidencias (MS); star / watch / fork → dar una estrella / seguir / hacer un fork (tentative); book a call →
  reservar una llamada (tentative); "What just happened?" → ¿Qué acaba de pasar?; "I like it" / "Don't like it?" → Me
  gusta / ¿No te gusta?; "Never do this again" → No volver a hacer esto.
- **Search**: scope → ámbito (tentative); pattern → patrón; wildcard → comodín (MS carácter comodín, short form); glob
  and regex stay Glob / Regex; custom → personalizado; "boring folders" → carpetas aburridas (the playful voice is
  deliberate); Ask anything → Pregunta lo que sea (tentative).
- **From the old style-guide list**: empty (a field left blank) → vacío (AppKit `El valor de %@ no puede estar vacío`);
  over ADB / over USB in a label → por ADB / por USB, keeping `a través de un cable USB` for the physical cable
  (`settings.fileOperations.mtpEnabled.description`); the usual way (where a program normally lives) → en los sitios
  habituales (`search.systemDirExclude.default` "las carpetas habituales del sistema"); placeholder (the pre-filled
  example) → marcador de posición (MS 146442, 92736); deployment (an Azure model's deployment name) → implementación (MS
  44583 and four more); reply → responder; Show details → Mostrar detalles (NSExceptionAlert); Copied → Copiado;
  Sending… → Enviando…; the Updates section → Actualizaciones; the crash NOUN is bloqueo in MS and macOS
  NSExceptionAlert, but the user-facing report says informe de fallos.
- **Basics from the first passes**: OK → Aceptar (AppKit); loading → Cargando... (three ASCII dots where the source has
  them); toggle, in a description → the action itself (activar / desactivar), never a noun; under cursor → bajo el
  cursor; authentication → autenticación; network (the chooser group) → Red; Storage (macOS Settings) → Almacenamiento;
  Files and Folders (pane literal) → Archivos y carpetas; App windows (Mission Control) → Ventanas de la app
  (tentative); New tab → Nueva pestaña; Select all / Deselect all → Seleccionar todo / Deseleccionar todo; skipped →
  omitido / se omitió; "{n} transfer(s)" → "{n} transferencia(s)" (feminine, so `seleccionada(s)` agrees).
- **Misc**: threshold → umbral (tentative); word wrap → ajuste de línea (tentative); pixels → píxeles; byte/bytes stay;
  handle (an open file handle) → identificador (tentative); symlink → enlace simbólico, (broken symlink) → (enlace
  simbólico roto); hardlink → enlace físico (MS); exclusive access → acceso exclusivo; in use by → siendo usado por;
  suggestions → sugerencias; git, worktree, repo, blob, commit, clone stay verbatim ("repo" inflects: este repo, los
  repos); FAT32 / exFAT stay; daemon, udev, ptpcamerad, Terminal, Ctrl+C, PTP stay.

## `worktree` frente a `árbol de trabajo` (`errors.git.*`, `fileExplorer.git.size.linkedWorktrees`)

English draws the two apart and so do we. A worktree is git's own name for a linked checkout, so every key naming one
carries it verbatim (`errors.git.orphanedWorktree.*`, `settings.fileExplorer.git.showVirtualGitPortal.description`,
`fileExplorer.git.size.linkedWorktrees` = `{countText} worktree vinculado` / `worktrees vinculados`), while the generic
"working tree" in `errors.git.bareRepo`, `blobTooLarge`, and `gitDirPermissionDenied` is ordinary prose and stays
`árbol de trabajo`. Agreement: masculine, no accent, plural in -s (`un worktree vinculado`, `dos worktrees vinculados`).
"working directory" stays the separate `directorio de trabajo`.

## Transfer toasts: agreement lives inside the plural branches (`transfer.*`)

"Copy complete" / "Move complete" → `Copia completada` / `Movimiento completado`: the adjective agrees with the noun
(Copia feminine, Movimiento masculine). Counted toasts wrap the whole clause in the `{count, plural}` so the verb agrees
(`Se movió 1 archivo` / `Se movieron N archivos`). Compress follows the same shape: `Se comprimió` / `Se comprimieron`,
mirroring `transfer.split.clean` ("Se copió: {phrase}") and the `one`/`many`/`other` shape of
`transfer.fileOnly.allDone`.

## `Carpeta superior`, no `carpeta contenedora` (`commands.navParent.label`, `menu.go.parentFolder`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

Chosen over macOS Finder's `carpeta contenedora` ("Go To Enclosing Folder" → "Ir a la carpeta contenedora") and
Nautilus's `carpeta padre`. Reasons, in order:

1. The catalog standardizes on it: `commands.navParent.label` = `Ir a la carpeta superior` plus the `errors.json`
   suggestions, eleven keys in all.
2. Double Commander, the orthodox two-pane source, renders the literally identical feature ("Enable changing to parent
   folder when double-clicking on empty part of file view" → "Cambiar a la carpeta superior al hacer doble clic en una
   zona vacía de la vista de archivos"), and Thunar agrees ("Abrir la carpeta superior").
3. `superior` carries the upward direction of these strings: `subir a la carpeta superior` reads naturally.

Counter-evidence, so nobody rediscovers it: macOS `es` DOES say `Carpeta contenedora`, but that translates Apple's
English "Enclosing Folder", which isn't Cmdr's phrase ("Parent folder"). `menu.go.parentFolder` once said
`Carpeta contenedora`, so the Go menu and the palette named one action two ways; both now say `Carpeta superior`.

Around it: pane background → `fondo del panel`, kept distinct from "empty space in a file list" → `espacio vacío` (the
source's two phrasings stay two); navigate (to) → `ir (a)` (MS id 1624173); double-click → `hacer doble clic`
(`Haz doble clic`, preterite `Hiciste doble clic`); hint → `aviso` (the internal `doubleClickOnPaneNotificationSeen`
keys); row → `fila` ("not a file row" → `no la fila de un archivo`). The Settings label reads as the imperative
`Haz doble clic en el fondo del panel para subir a la carpeta superior`.

## Preajustes y el botón «Volver a …» (`settings.appearance.*`)

preset → `preajuste` (Double Commander `Preajustes`, `El preajuste «%s» ya existe`). The standalone Back button is the
adverb `Atrás`, but "Back to X" needs the verb: `Volver a los preajustes` (pile: `volver a la versión anterior`).

## FAT32: el archivo que no cabe (`errors.write.*`, `errors.listing.notSupportedErrno.suggestion`)

- too large → `demasiado grande` · macOS Finder PE4.5 "El ítem «^0» no puede copiarse porque es demasiado grande para el
  formato del volumen", NE77.
- formatted as X → `tiene formato X` / `con formato X`: macOS uses the noun `formato`, and the framing avoids the gender
  agreement of `formateada`.
- store files → `almacenar`; "files larger than X" → `archivos de más de X`; "files this large" →
  `archivos tan grandes`; "no such limit" → `no tiene ese límite`.
- "{name} is {size}" → `{name} ocupa {size}`: `ocupar` is the natural verb for the space a file takes, and it doesn't
  agree with `{name}`.

## El diálogo de copiar y eliminar (`fileOperations.transferDialog.operationAria`, `fileOperations.transferDialog.targetWillBeCreatedCopy`/`targetWillBeCreatedMove`, `fileOperations.transferProgress.stageScanning`)

- action (the screen-reader label of the control) → `Acción` (macOS Finder TL26, AppKit).
- Scanning… (the spinner tooltip) → `Analizando…`, matching `stageScanning` = `Analizando`; the source's single `…`.
- "This folder doesn't exist yet" → `Esta carpeta todavía no existe`; "Cmdr will create it during the copy/move" →
  `Cmdr la creará durante la copia` / `… durante el movimiento`: `la` agrees with `carpeta`, and the op nouns are the
  catalog's `la copia` / `el movimiento`. Two literal sentences, no ICU select, per the op-specific keys.

## Archivos comprimidos: explorar, extraer, eliminar (`settings.archives.*`, `fileOperations.delete.archiveWarningStrong`/`.archiveWarningRest`, `queue.row.label`)

- **archive → `archivo comprimido`**, never bare `archivo`: file is already `archivo`. Two Tier-1/orthodox sources agree
  (macOS ArchiveUtility "Archivo comprimido Zip", Total Commander "Propiedades del archivo comprimido"). Total
  Commander's `fichero comprimido` is Spain-only, rejected. Where `zip` alone disambiguates, `el zip` is fine (the
  delete warning, `errors.mutation.archiveReadOnly`).
- **app bundle → `paquete`**, card `Paquetes de apps` (macOS `Mostrar contenido del paquete`).
- **browse → `explorar`**; segmented cells `Explorar` / `Abrir` / `Preguntar`; "Browse like a folder" →
  `Explorar como una carpeta`.
- **encrypted → `cifrado`**, over the pile's only hit `Encriptado` (a stale FileVault string): RAE-preferred and what
  current macOS uses. Flagged for review.
- **extract → `extraer`**, over Total Commander's `descomprimir`: tar isn't compressed.
- **preview (verb) → `previsualizar`** (`demasiado grande para previsualizarlo`); the noun stays `vista previa`.
- **for good → `para siempre`** in the delete warning, warmer than `permanentemente`. The two halves concatenate:
  `Dentro de un archivo comprimido no hay papelera.` + `Estos elementos se eliminarán del zip para siempre.`
- `queue.row.label`'s `archive_edit` arm → `Editando archivo comprimido`, the gerund style of its siblings.

## Pegar el portapapeles como archivo (`fileExplorer.clipboard.pastedAsFile`, `settings.fileOperations.pasteClipboardAsFile.*`)

- clipboard content → `contenido del portapapeles` (Finder, verbatim); "Paste clipboard content as a file" →
  `Pegar el contenido del portapapeles como archivo` (`como archivo` drops the article).
- Do nothing → `No hacer nada` (Double Commander); Create file → `Crear archivo` (reuses
  `fileExplorer.functionKeyBar.newFileAction`); Create and rename → `Crear y renombrar`.
- The toast `Se pegó {X} del portapapeles como {filename}`: the ICU select fills X with article + noun so it agrees
  (`la imagen` / `el PDF` / `el texto`); impersonal preterite, like `Se movió`, and it doesn't gender the user.

## La contraseña de un archivo comprimido y Comprimir (`fileOperations.archivePassword.*`, `commands.fileCompress.label`, `settings.archives.compressionLevel.*`)

- password-protected → `protegido con contraseña` (Total/Double Commander); unlock → `Desbloquear`, verb form
  `desbloquearlo`; the input's aria `Contraseña del archivo comprimido`.
- compress → `Comprimir` (Finder `Compress ${sources}` → `Comprimir ${sources}`); progress `Comprimiendo`;
  `Verificando antes de comprimir...` for the scan title; replace (overwrite warning) → `reemplazará`.
- In the Compress dialog the archive name is plain `archivo` (the zip is a file) and `.zip` sits in straight double
  quotes.
- compression level → `Nivel de compresión`; slider ends `Más rápido` (level 1, quicker packing, not app speed) and
  `Más pequeño` (level 9, the smaller output file).

## El registro de operaciones (`operationLog.*`, `commands.logOperationLog.*`)

- operation log → `Registro de operaciones` (log → registro, as MS "Event log" → "registro de eventos"; matches the
  Logging section `Registro` and `registro de cambios`); history → `historial`.
- roll back → `revertir` / `la reversión`: `Se puede revertir`, `No se puede revertir`, `Revirtiendo`, `Revertido`,
  `Revertido en parte`; the command's "roll them back" → `reviértelas`.
- item → `elemento`, matching `fileOperations.json`.
- Summary lines use the impersonal preterite, `Se {verbo} {countText} elemento(s)`: `operationLog.summary.copy` Se copió
  / copiaron, move Se movió / movieron, delete Se eliminó / eliminaron, rename Se renombró / renombraron, createFolder
  Se creó / crearon carpeta(s), createFile Se creó / crearon archivo(s), compress Se comprimió / comprimieron, trash Se
  movió / movieron … a la papelera; `archiveEdit` → `Se editó un archivo comprimido`, `archiveExtract` →
  `Se extrajo un archivo comprimido`.
- Lifecycle badges match `queue.row.status`: queued `Esperando`, running `En ejecución`, done `Hecho`, canceled
  `Cancelado`.
- "Didn't finish" (`operationLog.status.failed` and `operationLog.outcome.failed`, one source) → `No se completó`:
  neutral, avoids `Falló`, and matches the source's "didn't" framing; a close cousin of the queue's
  `No se pudo completar`.
- Initiators → `Tú` / `Cliente de IA` / `Agente` (direct address, no gendered noun).

## Ask Cmdr: el panel del chat (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- **chat → `chat`**, plural `chats`: no pile source has a chat feature, and the RAE-recognized loanword is universal in
  Spanish UI. `askCmdr.threads.open` / `askCmdr.sessions.title` = `Chats` carry `sameAsSourceJustification` for that
  reason, as do `askCmdr.title`, `commands.askCmdrToggle.label`, `settings.section.askCmdr` (the kept product name).
- **message → `mensaje`**; **stop → `Detener`** (AppKit FunctionKeyNames); **attach / attachment → `adjuntar` /
  `adjunto`** (the message sense, not MS's disk-image `exponer`); **archive a chat → `archivar`**, **Archived →
  `Archivado`**, **unarchive → `Desarchivar`** (tentative); **quota → `cuota`**.
- **usage → `uso`**, over MS's formal `utilización`; `settings.askCmdr.spend.title` "Spending" → `Gasto` (money),
  distinct from `askCmdr.cost.label` "This chat's usage" → `El uso de este chat`.
- **on-device → `en el dispositivo`** (Finder `Se conservará en el dispositivo`, the stays-local sense).
- **cost → `coste`**, not `costo`: the catalog says `coste` throughout. Flag if a LatAm-primary audience is ever
  confirmed.
- **token → `token` / `tokens`**: no macOS or MS source covers the LLM sense (MS's hits are security tokens); Spanish AI
  products keep the loanword.
- **"Not now" → `Ahora no`** (AppKit Document).
- **"Try again?" as an inline question → `¿Lo intentas de nuevo?`**, the dominant catalog pattern
  (`commands.handler.favoriteAddFailed`, `feedback.dialog.softFailure`, `onboarding.stepBeta.signup.failure`,
  `queryUi.dialog.aiTranslateFailedToast`) over the older `¿Reintentar?`.
- **Tool-call status lines**: gerund with no subject for "doing" (the `queue.row.label` progress style; its `other` arm
  `Trabajando` is reused for `askCmdr.tool.unknown.doing`), impersonal `Se` + preterite for "done", agreeing with the
  object. A bare preterite (`Preparó un plan…`) reads as a third-person subject and breaks the family:
  `askCmdr.tool.proposeRenamePlan.done` is `Se preparó un plan de cambio de nombre`, beside `Se leyó`, `Se buscó`,
  `Se encontraron`. The specific verbs (comprobar for check, buscar for find, consultar / revisar for look at) are
  tentative.
- **"in settings" / "in Advanced settings"** → `en Ajustes` / `en Ajustes avanzados`, capitalized even when English is
  lowercase: `onboarding.json`, `ai.json`, `crashReporter.json`, and `whatsNew.json` all do.
- **"Settings › AI"**: keep the `›` exactly where the en `@key` calls it out; older `>` cross-references in `ai.json` /
  `crashReporter.json` keep theirs. Menu-path separators mirror English per key.

## Indexación de fotos en unidades de red (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`/`.paused`)

- network drive → `unidad de red` (Double Commander, Total Commander, MS id 84431).
- **photo vs image**: the warm status and help lines say `foto`; the feature and label names keep `imagen`
  (`indexación de imágenes`, `Búsqueda de imágenes`). English makes the same split on purpose. `fotos indexadas` agrees
  with feminine `fotos`.
- opt into → `activar`; always-index → `indexar siempre` (`Indexar siempre esta unidad`; the lists
  `Unidades/Carpetas para indexar siempre`, the verb form, unambiguous over a noun).
- "paused, resumes when the drive reconnects" → `En pausa, se reanuda cuando vuelvas a conectar la unidad` (Finder's
  exact resume-on-reconnect phrasing).
- gently → `con cuidado` (tentative); "while you're not busy" → `mientras no estás usando el Mac`, which avoids the
  gendered `ocupado`.
- photo archive (a rarely-browsed collection, not a zip) → `colección de fotos`, dodging both `archivo` collisions.

## Revisar los cambios de nombre (`askCmdr.renameReview.*`, `askCmdr.tool.imageFacts.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.proposeRenamePlan.*`)

- **rename, the NOUN → `cambio de nombre`** (Finder "Deshacer cambio de nombre", "El Finder quiere cambiar el nombre de
  ^0 ítems"); the VERB stays `renombrar`. So the counted primary button "Rename {n} files" → `Renombrar # archivo(s)`.
- allow / deny → `Permitir` / `Denegar`; plurals `Permitir todos` / `Denegar todos` agree with `los cambios de nombre`.
- Column headers `Nombre actual` / `Nombre nuevo`: macOS puts the adjective first in a field label ("Nuevo nombre para
  la imagen:"), but the two headers stay parallel so the table reads as a pair. `suggestedOps.columnNewName` follows
  (`Nombre nuevo`), as does `askCmdr.renameReview.editName` = `Nombre nuevo para {name}`.
- rename cycle → `ciclo de cambios de nombre`, badge `(ciclo)` (tentative); "(overwrite!)" → `(¡sobrescribir!)`, both
  marks inside the parentheses.

## La indexación de imágenes: carpetas, estados, insignias (`fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`, `settings.mediaIndex.chosenFolders.*`, `settings.mediaIndex.showFileStatusIcons.*`, `settings.mediaIndex.progressSummary.title`)

- Status labels: `Imágenes indexadas` / `Imágenes indexadas automáticamente` / `Imágenes sin indexar` /
  `Imágenes excluidas` / `Indexando imágenes`; `sin indexar` matches `Aún sin indexar`.
- indexing pass → `pasada` ("on the next pass" → `en la siguiente pasada`), kept distinct from `análisis`, the full
  drive scan (tentative).
- "Folders to index" → `Carpetas para indexar`, sibling of `Carpetas para indexar siempre`; a passive
  `Carpetas que se indexan` broke the pair.
- "still searchable" → `se puede seguir buscando`, matching `settings.mediaIndex.progress.kept` and the reclaim line;
  `buscable` appears nowhere in the catalog and reads unnatural.
- "whatever else you pick above" → `elijas lo que elijas arriba` (the doubled-subjunctive concessive).
- "might be slightly off" (folder sizes) → `podrían no ser del todo exactos`: says what is inexact.
- badge → `insignia` (`Mostrar la insignia del repositorio`, the alpha `insignias`); status badges →
  `insignias de estado`.
- **Fold the agreeing participle into the plural arms.** English wraps only `{image}/{images}` and leaves "indexed"
  outside; Spanish `indexada/indexadas` (and `está/están` in `fileExplorer.imageIndex.drive.done`) agree with number, so
  the whole `imagen indexada` / `imágenes indexadas` clause lives in each CLDR arm, mirroring
  `settings.mediaIndex.progress.ofTotal`. "All N …" → `Todas las {totalText} …` in the plural arms, collapsing to
  `{totalText} imagen indexada` in `one`. A single image's tooltip is feminine singular
  (`Indexada para la búsqueda de imágenes`); "Waiting to be indexed" → `Esperando a ser indexada`; "Couldn't be indexed"
  → `No se pudo indexar`; "still working" → `aún en curso` (tentative).
- "Indexing now" → `Indexando ahora` in both `progressSummary.title` and `fileExplorer.imageIndex.file.indexing` (one
  source, one value; the reflexive `Indexándose` was dropped).

## La búsqueda por descripción y el modelo (`settings.mediaIndex.cards.*`, `settings.mediaIndex.semanticSearch.label`, `settings.mediaIndex.clip.*`)

- "Search photos by description" → `Buscar fotos por descripción`; in a sentence `la búsqueda por descripción` (agrees
  with `está desactivada`); the card "Semantic search" → `Búsqueda semántica`.
- Apple silicon stays verbatim, lowercase s, as Apple's Spanish writes it: `un Mac con Apple silicon`.
- Enable indexing → `Activar la indexación`.
- reclaim → `liberar`: "Delete model (reclaim {size})" → `Eliminar modelo (liberar {size})`, article dropped to parallel
  `Descargar modelo (~{sizeText} MB)`; "This frees {size}" → `Esto libera {size}`.
- "The model couldn't be removed just now" → `No se pudo eliminar el modelo ahora mismo`; `Eliminando…` keeps the single
  `…` like `Descargando…`.
- keyword / tag search → `búsqueda por palabras clave` / `por etiquetas`.

## El interruptor de papelera y los encabezados Desde / Hacia (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

- "Move to trash" → `Mover a la papelera`, identical to every sibling (`transferDialog.titleVerbOnly`'s trash arm,
  `transfer.trash`). Finder's own item is `Trasladar a la papelera`; not taken, so the catalog keeps ONE move verb.
- "Delete" (the destructive confirm) → `Eliminar`.
- "From" / "To" → `Desde` / `Hacia`: Double Commander ships this exact pair as the copy/move field labels; `hacia` is
  the partner `desde` asks for, where a bare `A` reads as a stray letter. Total Commander's `DE:` / `EN:` rejected
  (uppercase, and `EN` is a locative). The destination CONTROLS keep `Volumen de destino` / `Ruta de destino`.

## La indexación de unidades apagada (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

- drive indexing → `la indexación de unidades`, quoting the catalog verbatim: `settings.section.indexing` =
  `Indexación`, `settings.section.driveIndexing` = `settings.indexing.enabled.label` = `Indexación de unidades`, so the
  path reads `Indexación > Indexación de unidades` exactly as the sidebar does. Never `indización` (1 pile hit against
  9).
- "stays unindexed" → `sigue sin indexar`: carries no gender or number, so it survives any `{name}`.
- "picks up where it left off" → `continuará donde lo dejó`, ❌ not `seguirá donde lo dejó`: bare `seguir` + a place
  reads as "stay put" and garden-paths the promise. Distinct from `reanudar`, the pause/resume action.
- A settings path followed by a `y` clause takes a comma (`…en Indexación > Indexación de unidades, y esta unidad…`), or
  `unidades y esta unidad` reads as a two-item list.
- "Each drive keeps its own on or off choice" → `Cada unidad recuerda si estaba activada o desactivada`: a `si` clause
  lets the participles agree with `unidad`. The tail "ready for when…" →
  `lista para cuando vuelvas a activar la indexación`, naming `la indexación` because a bare `la` would point at
  `unidad`.
- `overriddenBadge` "Off with drive indexing" → `Desactivado con la indexación`: masculine `Desactivado` matches the
  off-state labels (`settings.ai.provider.opt.off`); the badge only renders inside the `Indexación de unidades` page, so
  `de unidades` drops for length (29 chars against 23). The comitative `con` is tentative.

## Índice de unidades: la pasada de comprobación de cambios (`indexing.step.*`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- "Checking for changes" (run-kind header) → `Comprobación de cambios`, a noun phrase beside `Primer análisis completo`
  and `Actualización rápida`.
- "Update the file list" → `Actualizar la lista de archivos`, composed from `Guardar la lista de archivos` +
  `Actualizar el índice`.
- "the check running right now" → `el análisis que se está ejecutando ahora mismo`: `análisis` is this catalog's word
  for a full check (`tooltipCoalesced`, "el próximo análisis completo de Cmdr"), closing on `lo dejará al día`.

## Transferencia atascada: el aviso de "sin progreso" (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

The notice that replaces the ETA countdown when a copy or move stops moving.

- **"No progress for {duration}" → `Sin progreso desde hace {duration}`**: "for X, up to now" REQUIRES `desde hace`,
  never `durante` (a finished span) nor bare `hace`. `progreso` is MS and the catalog's `Progreso del tamaño`;
  `Sin avances` is an equal runner-up. Rejected `Detenida desde hace…`: it collides with the queue's paused state.
- **"Waiting for X to respond" → `Esperando a que X responda`**: Finder's own waiting sentences
  (`Esperando a que “^0” acepte…`); `esperando a que` + subjunctive, never the calque `esperando por`.
- **"has stopped moving" → `ha dejado de avanzar`**: says the transfer stopped ADVANCING without claiming it stopped or
  failed. Rejected `se ha detenido` / `se ha quedado parada`: both read as paused, which the queue labels `En pausa`.
- "leave it running in the background" → `déjala en ejecución en segundo plano`, quoting `queueTooltip` and
  `backgroundedToast`; `-la` agrees with `transferencia`.
- "partly written" → `parcialmente escrito` (Nautilus's `parcialmente copiado`); "# file is still open" →
  `# archivo sigue abierto`; "The log has the details." → `El registro tiene los detalles.`; `Close` → `Cerrar`,
  unmistakable beside `Cancelar`.
- **Bake the whole sentence into the plural branches when the tail agrees with the count.** `stallInFlight`'s English
  keeps "and may already be partly written" outside the plural; Spanish can't (`esté`/`estén`, `escrito`/`escritos`).

## Ruta copiada: la confirmación del portapapeles (`fileExplorer.clipboard.copiedPath`)

The path renders below on its own monospaced line, so it isn't a placeholder in the sentence: the sentence ends on a
colon and must stand without it. `Ruta copiada, ya está en el portapapeles:`. The leading participle follows the sibling
toasts (`{countText} elementos copiados`); no possessive (`tu portapapeles`): there is only one, and macOS always uses
the article.

## Cola de operaciones: el cambio de nombre de la ventana (`queue.*`, `commands.queueShow.label`/`.description`, `fileOperations.transferProgress.queue*`, `fileOperations.transferProgress.backgroundedToast`)

The English widened from "Transfer queue" to "Operation queue" across 14 keys: the window lists deletes, trashes,
renames, folder and file creations, and archive edits, not only transfers, and "transfer" already means copy-or-move one
level down.

- operation → `operación` (Finder NE82 "hay otra operación en curso", NE83 "la operación actual"; Double Commander
  `Operación actual`, `Operaciones con archivos`); the queue → `Cola de operaciones`, superseding
  `Cola de transferencias`.
- The View-menu pair stays parallel: `Cola de operaciones` / `Registro de operaciones`, same head noun, `cola` (running
  now) against `registro` (already ran).
- "this operation" (per-row arias) → `esta operación` (Finder CS203 "…para completar esta operación"): `Pausar` /
  `Reanudar` / `Cancelar` / `Seleccionar` + `esta operación`.
- `commands.queueShow.label` is the bare window title, `Cola de operaciones`, ❌ not `Mostrar la cola…`: palette entry,
  menu item, and window title read identically.
- The feminine head noun kept every clitic and participle that already agreed (`La encontrarás`,
  `Mantenla … gestiónala`, `pausarlas, reanudarlas o cancelarlas`). Worth knowing if the source ever widens to a
  masculine concept.
- `queuedToastCount` keeps its three branches: `one {# operación} many {# operaciones} other {# operaciones}`.
- `transferencia` isn't retired: it still names the copy or move itself (the progress dialog, `transfer.*`, the stall
  copy).

## El chip de la esquina y el aviso de operación sin terminar (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss`/`.dismissAria`, `queue.toolbar.dismissAll`)

- **dismiss (take a finished-badly row off the list; nothing is undone, retried, or deleted) → `Descartar`**, the
  catalog's nine existing hits (`crashReporter.dialog.dismiss`, `downloads.empty.dismiss`, `ui.toast.dismissAria` =
  `Descartar notificación`, …), AppKit "Discard" → "Descartar", MS dismiss → descartar. Near-miss: `Quitar` also takes a
  row off a list, but it edits a list the user built; this button clears a NOTICE.
- "Dismiss all" → `Descartar todo`, parallel to `Pausar todo` / `Reanudar todo`; ❌ not `Descartar todas`.
- **"Couldn't finish <action>" (the toast title's nine arms) → `No se pudo completar` + the operation NOUN.** The head
  is `queue.row.status`'s `failed` arm verbatim; the `other` arm is byte-identical to it. The nouns are the catalog's
  own: copy → `la copia`, move → `el movimiento`, trash → `el movimiento a la papelera`, delete → `la eliminación`,
  rename → `el cambio de nombre`, create_folder / create_file → `la creación de la carpeta` / `del archivo` (tentative),
  archive_edit → `la edición del archivo comprimido`. ❌ Rejected `No se pudo terminar de copiar`: a second verb where
  the row says `completar`.
- Counted "N operations couldn't finish" → verb-first `No se pudo(-ieron) completar {countText} operación(es)`, the
  whole clause inside each arm; used by `failureToast.summary` and the first sentence of `chip.failed`.
- "Show in operation queue" → `Mostrar en la cola de operaciones`; the spoken labels end on
  `Abre la cola de operaciones … para ver por qué`.
- "percent" spelled for a screen reader → `por ciento` (unsourced; what VoiceOver says for `%`).
- **The `%` sign keeps English's spacing, `{percentText}%`**: the catalog is 10 to 1 (`Zoom al 100%`,
  `indexing.progress.percentEta`); the outlier is `fileExplorer.summary.percentSelectedIn` (`({percent} %)`).
- "to {destination}" → ` a {destination}`, as Nautilus ("Copiando %'d archivos a «%s»"). Finder's copy string uses `en`,
  not taken: `{label}` is generic across copy/move/trash, and `Moviendo … en Backup` reads locative.
- Every optional clause keeps its leading space inside the branch; `=0 {}` / `other {}` stay empty. The time-left
  `{detail}` comes from `fileOperations.transferProgress.etaRemaining` (`Queda {duration}`) or the paused arm.

## El aviso de conflicto de la ventana principal (`fileOperations.operationConflict.context`/`.pausedNote`)

- The context line is `queue.row.label`'s gerund plus the chip's ` a {destination}`, nothing re-derived.
- `a {destination}` holds even with no object: Finder splits (`Copiando “^1” en “^2”` / `Trasladando “^1” a “^2”`),
  Nautilus uses `a` for both, and the chip already says `a`. The residual: "Copiando a Ana" can momentarily read as
  personal `a`. Rejected `Copiando a la carpeta {destination}`: `getFolderName()` can hand this a volume root, an SMB
  share, or `/` (tentative).
- "Working in {destination}" → `Trabajando en {destination}`: locative, and the kind may not have a folder.
- The two `archive_edit` arms differ on purpose: `Editando {destination}` names the zip; without one,
  `Editando un archivo comprimido` (a sentence needs the article; the badge doesn't).
- "Everything else is paused until you answer." → `Todo lo demás está en pausa hasta que respondas.`: `en pausa` is the
  `paused` arm verbatim, and `hasta que` + subjunctive promises the resume.

## El botón con la cola vacía: "Background" (`fileOperations.transferProgress.background`/`.backgroundAria`)

The SAME button as `queue` / `queueAria` of the progress dialog, in its other state. With the queue empty there's
nothing to line up behind, so English swaps the noun "Queue" for the verb "Background" ("get this out of the way").

- **`En segundo plano`**: MS terminology (background adj., "operating without interaction with the user" →
  `segundo plano`, all regions) and the catalog (`queueTooltip`, `backgroundedToast`). macOS es has no `segundo plano`
  at all; its visual backdrop is `Fondo`, so `segundo plano` can never read as a window's background.
- ❌ Not the bare noun `Segundo plano`, although Total Commander es puts it on this very button
  (`4004="&Segundo plano"`): in Double Commander's colour settings, `Primer plano` / `Segundo plano` are the
  foreground/background colour pair, and MS keeps that pair. The preposition disambiguates and supplies the elided verb.
- ❌ Not the full infinitive `Pasar a segundo plano` / `Continuar en segundo plano`: 21–25 characters on the button
  whose other state says `Cola` (4), and the `@key` asks for a short label. Keep them in reserve if a native reviewer
  finds `En segundo plano` too elliptical.
- The infinitive-buttons rule yields here on purpose: the sibling is already the noun `Cola`, and both states name the
  operation's DESTINATION.
- `backgroundAria` → `Mantenerla en ejecución en segundo plano`, echoing `queueTooltip` and the infinitive of
  `queueAria`. WCAG 2.5.3: the visible label is contained verbatim (bar the capital). ⚠️ The two keys are ONE unit:
  rewrite the label and the aria must still contain it.

## La salida con operaciones en curso (`main.quit.*`)

The modal that intercepts a quit while a copy, move, delete, trash, or archive edit is running.

- **quit, the USER's action → `salir`** (Finder A17 "No se puede salir del Finder porque hay operaciones en curso.";
  AppKit `Quit` → `Salir`; `commands.appQuit.label` = `Salir de Cmdr`).
- **quit, the APP ending itself → `cerrarse`** (Finder BN36 "El Finder está a punto de cerrarse."): the countdown says
  `Cmdr se cerrará …`. Not a collision with the crash's `se cerró inesperadamente`, where `inesperadamente` carries the
  crash.
- **"still running" (operations) → `en curso`**, Finder in this exact sense; the heading `Aún en curso`, the title the
  same two words. Deliberately NOT `queue.row.status`'s `En ejecución`, which is the per-row badge; prose says
  `en curso`.
- The title takes the catalog's infinitive-question shape: `¿Salir mientras hay {countText} operaciones en curso?`, like
  `¿Eliminar el modelo de IA?`; all three arms carry the whole sentence, and `one` says `una operación`.
- **"Keep working" → `Seguir trabajando`** (tentative wording): no pile source; `seguir` is the catalog's carry-on verb.
  ❌ Not `Más tarde` (the DEFER label) nor bare `Cancelar`, which beside a list of running operations would read as
  cancelling THEM.
- "Quit now" → `Salir ahora`: the "now" is load-bearing (the countdown quits anyway).
- "so a restart or logout never waits on Cmdr" → `para no hacerte esperar al reiniciar el Mac o cerrar sesión`:
  restructured onto the Tier-1 VERBS (Reiniciar, Cerrar sesión) because the nouns are unsourced; `el Mac` added so
  `al reiniciar` can't read as restarting Cmdr (tentative).
- "what it leaves half-written" → `lo que quede a medio escribir` (verbatim from
  `settings.advanced.showStagingTempFiles.description`); "clears away" → `borra`, deliberately not the delete verb.
- "anything still being written stops where it is" → `Lo que aún se está escribiendo se interrumpe donde esté`: **the
  body stays number-neutral** (one operation writes several files, several operations can run), and `interrumpir` is the
  catalog's word for a copy cut short. ❌ Not `se detiene` / `se para` (they read as paused).
- `countdownAria` isn't bound by WCAG 2.5.3: it labels a region, not a control with a visible label.

## Usage stats: fuera "anónimas", dentro "un identificador aleatorio" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id) and says plainly what they're tied to, in
deliberately everyday words, so ❌ never `seudónimo` / `seudonimizado`. usage stats → `estadísticas de uso`; a random id
→ `un identificador aleatorio` (a bare `id` reads clipped in prose); tied to → `vincularse a` (the catalog's own verb,
`onboarding.stepBeta.emailNote`).

## Filas en cola a la espera de respuesta y la confirmación de reversión (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- "Needs your answer" → `Respuesta necesaria` (macOS `Contraseña necesaria…`). ❌ Never `Esperando tu respuesta` or
  anything starting with `Esperando`: that IS the queued state, and the two share one narrow column.
- prompt → `la pregunta`; "this operation carries on" → `esta operación continuará` (not `se reanudará`, which drags in
  the Reanudar button).
- rollback → `Revertir`: title `¿Revertir esta operación?`, destructive button `Revertir`, matching the button that
  opened the dialog.
- "Keep them" → `Conservar los archivos` (macOS `Conservar`): the noun, because a clitic would be ambiguous right after
  the body mentions the REPLACED files.
- "written so far" → `escritos hasta ahora`; "Stop, and …" (tooltip) → `Detener y …` (macOS `Detener copia`),
  deliberately unlike `Cancelar`.
- `foregroundBusyToast` names the operation: English's "this one" has no antecedent in Spanish, so
  `… y luego muestra esta operación`, reusing `Mostrar` (`queue.row.foreground`).

## La cadena de renombrados: el aviso que cuenta los archivos que no cambiaron (`fileExplorer.rename.chainKeptOriginalName*`)

- "kept its name" → `mantuvo su nombre`: macOS `conservar` is for a CHOICE ("Conservar original"); here the file kept
  its name without anyone deciding, and `mantuvo` states that fact. The two keys are one rewritten notice, word for
  word.
- "and so did {n} other files" → `y otros {n} archivos también` (Finder PE106_V4 "… como “^1” y otros ^0 ítems"); the
  closing `también` carries "and so did".
- The singular arm has no numeral: `y otro archivo también` (`otro` already means one more; English's `one` arm also
  skips `{othersText}`).
- Curly quotes `“…”`, as the sibling and macOS.

## El renombrado sin confirmar y el nombre que el sistema rechaza (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

The sibling pair with the opposite sense: there the file surely kept its name; here nobody knows, and it may have been
renamed. Neither key may imply it kept its name.

- "the rename" → `el cambio de nombre` (Finder RN1/RN2, AppKit SavePanel "Name Change" → "Cambio de nombre"): a known
  masculine singular subject, so no participle depends on `{name}`.
- "Couldn't confirm …" → `No se pudo confirmar …`, as `fileOperations.mkdir.timeoutMessage` and
  `fileExplorer.pane.trashUnconfirmedToast`.
- "The volume may be slow" → `El volumen puede ir lento` (verbatim from `mkdir.timeoutMessage`).
- "may still have gone through" → `es posible que el cambio sí se haya aplicado`: the emphatic `sí` carries "still", as
  the siblings do; the short anaphoric `el cambio` keeps the notice light.
- Negative coordination `ni el de`: `No se pudo confirmar el cambio de nombre de “{name}” ni el de otros 3 archivos`.
- All three arms close in the plural (`los cambios sí se hayan aplicado`): even `one` covers two renames.
- "That filename can't be used" → `Ese nombre de archivo no puede usarse` (carpeta: `Ese nombre de carpeta…`), Finder
  RN5 / NE74. `Ese` translates "That" (the name you just typed), which is why it drops the siblings' article; no full
  stop, because it composes into `chainKeptOriginalName`.

## Operaciones sugeridas: el diálogo de lo que propone Ask Cmdr (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops → `operaciones`; title `Operaciones sugeridas` (the house `Operaciones de archivos`).
- approve → `Aprobar`, over macOS `Aceptar`: the counted variant ("Aprobar 3 archivos") authorizes an action.
- reject → `Rechazar` (Finder's AirDrop Aceptar/Rechazar pair).
- "This can't be undone" → `Esto no se puede deshacer` (Finder's immediate-delete alert, shortened).
- "Undo by deleting what it writes" → `Deshacer eliminando lo creado` (tentative): `lo que crea` reads as `creer`.
- suggestion → `sugerencia` (`ui.combobox`, `askCmdr`).

## Duplicar: el comando que copia en la misma carpeta (`commands.fileDuplicate.*`)

`Duplicar` (Finder N154 `Archivo > Duplicar`, `Duplica los ítems en las ubicaciones actuales`, macOS 26.6.1,
2026-08-19); it coexists with `Copiar` (F5) and `Mover` (F6). The description is third person, like its neighbours, and
keeps the catalog's `archivos` over Finder's `ítems`.

## Menús nativos: barra de menús, menús contextuales, títulos de ventana (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Sources: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/es.lproj`, `MenuBar.strings` + `LocalizableMerged.strings`)
is Tier 1 and decides almost everything; Safari 26 (`MainMenu.strings`) gives the tab vocabulary, Microsoft the rest.
RAW family: **single apostrophes**, a `''` would render doubled in the menu.

- Menu-bar titles → `Archivo`, `Edición`, `Visualización`, `Ir`, `Ventana`, `Ayuda`, `Servicios` (Finder, Safari). The
  Select menu → `Seleccionar` (Nautilus, Thunar, Dolphin), matching `Seleccionar todo`.
- Hide Others → `Ocultar otras apps` (Finder `300729.title`, macOS 26.6.2), in BOTH keys of the command
  (`menu.app.hideOthers`, `commands.appHideOthers.label`), which had drifted apart.
- Quick Look → `Vista rápida` (Finder TL14): Apple localizes this feature name, so it isn't on the do-not-translate
  list.
- Get Info → `Obtener información`; Go > Home → `Inicio`; Sort By → `Ordenar por`; Default → `Por omisión`, never
  Windows's `Predeterminado`. Parent folder is `Carpeta superior`, not Finder's `Carpeta contenedora` (§
  `Carpeta superior`).
- zoom in / out → `Aumentar el zoom` / `Reducir el zoom` in the menu AND the palette. Safari's bare `Ampliar` /
  `Reducir` work in a Zoom submenu that supplies the object; the palette has no such context, and the object form
  matches `commands.handler.zoomIncreased` (`Zoom aumentado al {size}%`).
- ascending / descending → `Ascendente` / `Descendente` (Dolphin).
- changelog → `Registro de cambios` (the document), distinct from Help > `Novedades` (the news).
- word wrap → `Ajuste de línea` (MS).
- pin / unpin tab → `Fijar pestaña` / `Desfijar pestaña`; Safari's `Anclar pestaña` stays recorded as the Tier-1 variant
  (§ Fijar servidores).
- Finder tag colours → `Rojo, Naranja, Amarillo, Verde, Azul, Morado, Gris` (`TG_COLOR_*`); the tag row
  (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Etiquetas`, `Añadir “{color}”`,
  `Quitar “{color}”` (TG5). Finder's TG6 says `Eliminar “^0”`, but removing a tag deletes nothing, so `Quitar`.
- busy (a volume in use) → `(ocupado)`; Eject → `Expulsar`, Disconnect → `Desconectar`, Remove (from a list) → `Quitar`
  (so removing a favourite doesn't sound like deleting files); forget → `olvidar`.
- Deliberately identical to English (with `sameAsSourceJustification`): `menu.view.zoom`, `menu.window.zoom`,
  `menu.zoom.percent*`, `menu.view.askCmdr`.

## El aviso de la conexión que macOS presta (`fileExplorer.network.osMountFallback.*`)

- "You are connected" → `Tienes acceso`: `estás conectado/a` would need a gendered adjective (macOS solves it with
  `^[Conectado](inflect: true…)`, which ICU lacks), and it avoids a fourth `conexión` in the sentence.
- "macOS's native SMB network connection" → `la conexión de red SMB nativa de macOS`. When a sentence doesn't name
  macOS, the catalog says `la conexión del sistema` (`fileExplorer.pane.directConnection*Toast`).
- Speed multipliers `4x`, `100x` → `4 veces más lenta`, `(a veces, 100 veces)`: Spanish UI prose writes `veces`; digits
  stay (technical comparisons keep their punch).
- "for most connections" → `en la mayoría de las conexiones`, before the comparative: `más lenta que…` must sit next to
  its term, so the parenthesis moves to the end.
- "Click the button below to try again." → `Haz clic en el botón de abajo para volver a intentarlo.` (AppKit Printing
  "haz clic en el botón Añadir (+)").
- "Try connecting directly" → `Intentar conectar directamente` (infinitive button;
  `fileExplorer.navigation.connectDirectly`).
- "Dismiss" (the X tooltip) → `Descartar`, the same source as `lowDiskSpace.toast.closeTooltip`.

## Los avisos de una línea de renombrar / crear (`errors.mutation.*`, `errors.volume.*`)

The line under the name field (or a short toast) when a rename, a new folder, or a new file is refused. RAW family:
single apostrophes, literal `{path}`. Nearly every key has a long sibling in `errors.listing.*` / `errors.write.*`, so
the rule is **quote the sibling, don't reinvent the sentence**: the short notice and the long explanation of the same
problem say it in the same words.

- Quotes around `{path}`: curly `“…”`, though English writes straight ones (the long siblings use backticks because they
  render as markdown; these are plain text).
- `{path}` is uncontrolled: nothing agrees with it; it stays a quoted subject or sits behind a preposition.
- root folder (of a volume) → `carpeta raíz` (MS "The uppermost directory on a computer, partition or volume"). ❌ NOT
  `carpeta superior`, which is parent folder here. `cantRenameVolumeRoot` takes Finder's prose shape (RN11 "No se puede
  cambiar el nombre de “^0” en estos momentos porque…").
- System Integrity Protection → `la protección de la integridad del sistema`, lowercase in the sentence (Finder ET6);
  the English "macOS protects this item with…" is reordered so it doesn't say "protege … con la protección".
- The verb rename in prose is `renombrar` (§ Palabras que se separaron):
  `No se puede renombrar este elemento debido a la protección de la integridad del sistema de macOS.`
- Quoted from the long siblings: "is on its way out" → `está de salida`; "the destination can't store that name" →
  `el destino no puede guardar ese nombre`; "no longer available" → `ya no está disponible`; "didn't respond in time" →
  `no respondió a tiempo`; "is locked" → `está bloqueado`; Get Info → `Obtener información`.
- `timedOut` isn't a failure: `así que es posible que el cambio sí se aplique`.
- `deviceSessionReset` isn't a disconnect (the MTP phone is still plugged in): `El dispositivo reinició su conexión.`,
  never anything that sounds like unplugging.
- "Move it instead" → `Usa Mover.`, naming Cmdr's F6 command; `renameOutOfArchive` / `renameAcrossArchives` stay
  parallel word for word.
- "Only zip archives can be changed" → `Solo se pueden modificar los archivos zip`: `zip` disambiguates alone.
- "Cmdr stopped this at your request" → `Cmdr detuvo esto porque se lo pediste.` (`detener`, the macOS stop verb, unlike
  `Cancelar`; the second person dodges the formal `a petición tuya`; the turn of phrase is tentative).
- "Something went wrong, and Cmdr couldn't tell what." → `Algo salió mal y Cmdr no pudo saber qué.`
- "The volume couldn't finish that" → `El volumen no pudo completarlo.`

### Segunda tanda: las dos claves de papelera (`errors.mutation.trashNotSupported`/`trashRefused`)

- Trash → `papelera`, lowercase in a sentence (AppKit, Finder PE60.2, the whole catalog).
- The verb stays `mover`, not Finder's `trasladar`: the family reads together.
- "has no Trash" → `no tiene papelera`: English varies on purpose between the long dialog ("doesn't support trash" →
  `Este volumen no admite la papelera.`) and this short line; `no hay papelera` is already the catalog's idiom.
- "the only way is to delete permanently" → `solo se puede eliminar permanentemente`: the impersonal `se` avoids a bare
  infinitive and a clitic that would agree with an item of unknown gender; `permanentemente` repeats the command label
  the user needs.
- "macOS wouldn't move this to the Trash." → `macOS no permitió mover esto a la papelera.` (AppKit "no permiten
  modificarlo"). ❌ Not `rechazó mover` (`rechazar` wants a noun object) nor the colloquial `no quiso`. `esto` keeps the
  line gender-free.

## El diálogo de fallos: las tres aperturas (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

The dialog at the next launch picks one of three openings. `.ended` doesn't change; the other two **can't say Cmdr
closed, failed, or stopped**, because none of that happened.

- "ran into a problem" → `tuvo un problema`: sources use the impersonal (`Ha habido un problema…`), but the siblings
  take `Cmdr` as subject, so parallelism with `.ended` decides.
- "kept running" → `siguió funcionando`. ❌ Not `siguió ejecutándose`: `ejecutarse` names an OPERATION that keeps
  running in this catalog (`transferProgress.backgroundedToast`), and right after `en segundo plano` it invites exactly
  that misreading. `funcionar` is also warmer.
- "in the background" → `en segundo plano` (MS, Double Commander, Dolphin, Total Commander); it hangs on the problem,
  not on the app carrying on. `segundo plano` appears nowhere in macOS es.
- "a report" (no "crash") → `un informe`; the second sentence is `.ended`'s minus `de fallos`, and `solucionarlo` now
  points at `un problema`.
- `se cerró inesperadamente` stays exclusive to `.ended`; `.unknown` names no outcome.
- Word order diverges on purpose: `.keptRunning` fronts `la última vez` to avoid stacking two adverbials; only one
  variant shows at a time.

## El texto de ajustes de informes ahora cubre los dos casos (`settings.updates.crashReports.description`)

The switch also sends a report when a background problem did NOT close the app. The pieces come from the crash-dialog
section, in the present: `cuando Cmdr se cierra inesperadamente` (`.ended`), `tiene un problema en segundo plano`
(`.keptRunning`), `un informe` without `de fallos` (the sentence covers both), and
`qué parte del código tuvo el problema` (`crashReporter.dialog.privacyNote`) instead of `la ubicación del fallo`. ❌ The
LABEL `settings.updates.crashReports.label` stays `Enviar informes de fallos`: it's the setting's name.

## Expulsar y desconectar: los nueve avisos del selector de volúmenes (`errors.eject.*`)

Each value is inserted after a colon in a short toast: `No se pudo expulsar {volumeName}: …` or
`No se pudo desconectar: …` (`fileExplorer.pane.ejectFailedToast` / `.disconnectFailedToast`). RAW family. Quote the
sibling before inventing the sentence.

- **drive (the one being ejected) → `disco`**: Finder's eject corpus always says `disco` (NE31/NE80 "El disco “^0” está
  en uso y no se puede expulsar.", TL_HELP_EJCT "Expulsar discos y desmontar servidores", AppKitErrors "No se ha podido
  expulsar el disco porque está siendo usado por “%@”."). Elsewhere drive is `unidad` (the user's drives, a drive that
  disconnected); here the eject sense rules.
- removable → `extraíble` (Finder `Volumen extraíble`, MS); idle → `inactivo` (MS).
- "Unplug it" → `Desconecta el cable`, ❌ not `Desconéctalo`, which would collide with the `Desconectar` command that
  just didn't work; naming the cable makes it the physical act.
- "The device wouldn't close its connection." → `El dispositivo no cerró su conexión.`: the fact in the past, no verdict
  (same fix as `trashRefused`).
- `timedOut` copies `errors.mutation.timedOut`:
  `El disco aún no ha respondido, así que es posible que sí se expulse por su cuenta.`
- `volumeNotFound` copies `errors.mutation.volumeGone`'s skeleton:
  `Ese disco ya no está conectado, así que no hay nada que expulsar.`
- `unexpected` IS the same English as `errors.mutation.unexpected`, so the same value.
- "Close any open files and apps" → `Cierra los archivos y las apps que tengas abiertos`: the relative with `tener`
  avoids a stranded participle.
- `busy` reuses `todavía está + gerundio`:
  `Cmdr todavía está moviendo archivos en este disco. Expúlsalo cuando termine.` (`-lo` agrees with `disco`).

## El aviso de la papelera: deshacer y devolver a su sitio (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- undo (button) → `Deshacer` (AppKit, Nautilus, `askCmdr.renameUndo.undo`).
- **put back → `devolver … a su sitio`**: the catalog's `devolver` (`askCmdr.renameUndo.skipReason.failed.named`) plus
  `a su sitio` for "back where it was". Finder's command label `Sacar de la papelera` is right for a menu but awkward
  here: the partial toast already names the trash in its second half. `restaurar` is reserved for undoing a RENAME, so
  the catalog keeps returning a PLACE apart from restoring a NAME.
- "This drive doesn't keep a trash." → `Esta unidad no tiene papelera.`, a fact with no verdict.
- "Nothing to put back." → `No hay nada que devolver.`
- The second half has its own count (`{skipped}`), so it conjugates:
  `…; {skippedText} {skipped, plural, one {elemento se quedó} many {elementos se quedaron} other {elementos se quedaron}} en la papelera.`
  (this driver was added for Spanish agreement).
- The toast button and the command are the same text, `Ir a la papelera`, like `Ir a la carpeta superior`.

## Añadir a un informe ya enviado: el diálogo de la nota tardía (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- "Add to" with no object → `Añadir a …` / `Añadir al informe`: macOS licenses the ellipsis (`Añadir a favoritos`,
  `Añadir al Dock`). ❌ Not `agregar`: zero hits in macOS es.
- The dialog title is infinitive, `Añadir a tu informe de error`, like `Enviar informe de error` and
  `Enviar comentarios`; `updates.moveToApplicationsDialog.title` ("Mueve Cmdr a…") is the exception of a title that
  gives an instruction.
- **error report → `informe de error` everywhere**, against `informe de fallos` for the CRASH report.
  `amend.unavailable` points at the menu without naming the report type, as English does.
- the Help menu → `el menú Ayuda`, no quotes (macOS writes `selecciona menú Apple > Ajustes del Sistema`).
- "can't take a note any more" → `Ya no se pueden añadir notas a ese informe`: impersonal `ya no` + present, no culprit,
  no `error`, `fallo`, or `no se pudo`.
- "To get your notes to the team" → `Para hacer llegar tus notas al equipo`; "attach your email" → `adjunta tu correo`
  (matching `common.attachEmail`); "it'll join what the team already has" → `se sumará a lo que el equipo ya tiene`.
- "already sent" → `ya envió`, the pan-regional preterite.
- "What was sent" → `Lo que se envió`, the past of `errorReporter.dialog.detailsToggle` ("Lo que está a punto de
  enviarse").
- "View or add notes to the report" → `Ver o añadir notas al informe` (29 characters against 31, still fits beside
  `Cambiar ajustes`); "Note added to your report." → `Nota añadida a tu informe.` (agrees with `nota`, not the reader).

## El diálogo de seleccionar / deseleccionar archivos (`selection.*`)

- **select → `Seleccionar`; deselect → `Deseleccionar`.** Finder gives `Seleccionar todo` but NO verb for the opposite
  (`No seleccionar nada`, a whole-scope phrase); MS gives `anular la selección`. The verb comes from the orthodox
  two-pane family, Cmdr's own surface: Total Commander's same dialog (`Seleccionar por nombre/extensión:` /
  `&Deseleccionar…`, buttons `&Seleccionar` / `&Deseleccionar`), Double Commander `&Deseleccionar todo`. `Deseleccionar`
  has no Tier-1 backing; the Tier-2 alternative `Anular la selección de…` is too long.
- The three places that name the dialog say the same (`menu.select.files` / `menu.select.deselectFiles`, the palette
  labels, `settings.selection.recentSelections.maxCount.description`, and the titles `selection.dialog.title.add` /
  `.remove`); the title once didn't match the menu that opens it.
- `… in the focused pane` → `… en el panel activo`; the tooltips start with the button text verbatim and only add the
  complement.
- `Press Enter to filter` → `Pulsa Intro para filtrar` (like `search.runHint`).
- The five popover texts copy their search twins `queryUi.recent.*`, swapping `búsquedas` for `selecciones`;
  `selection.recent.popoverAria` and `.listboxAria` share one English, so one value: `Selecciones recientes`.
- `Apply recent {mode} selection: {query}` → `Aplicar selección {mode} reciente: {query}`, the mould of
  `search.recent.runAria`; `{mode}` arrives translated and `{query}` is free text, both in neutral positions.
- "Matching what is shown in the list (the full path)." → `Coincide con lo que muestra la lista (la ruta completa).`

## El nombre accesible tiene que contener la etiqueta visible (`queryUi.scope.toggle.caseSensitiveAria`, `viewer.search.caseSensitive`, `queryUi.recent.caseSensitive`)

`desktop-i18n-aria-label` demands an `*Aria` value contain its visible label (WCAG 2.5.3). Voice control users say what
they READ, so give the LABEL the shape the accessible phrase already uses.

- case-sensitive → `Distinguir mayúsculas y minúsculas`, the full macOS form; the clipped `Distinguir mayúsculas` named
  the same switch a second way. In running text it's third person lowercase (`distingue mayúsculas y minúsculas`).
- The aria wraps the label, it doesn't rephrase it: `Distinguir mayúsculas y minúsculas al buscar`.

## Una palabra inglesa, una palabra española: la revisión de deriva (`commands.helpSendErrorReport.label`, `menu.help.sendErrorReport`, `menu.zoom.in`/`.out`, `commands.handler.zoomResetHintMenu`, `menu.context.toggleSelection`, `fileExplorer.columns.modified`)

Thirty-six places where `es` gave two names to one English text, mostly because a late pass touched `menu.json` and left
`commands.json` behind. The resolved ones, and why:

- `error report` is `informe de error` in the Help menu and the palette too: they said `Enviar informe de fallos…`,
  promising a crash report and opening a dialog titled `Enviar informe de error`.
- `Zoom in` / `Zoom out` → `Aumentar el zoom` / `Reducir el zoom` in menu and palette (§ Menús nativos).
- `commands.handler.zoomResetHintMenu` pointed at a menu called `Ver`; the menu bar is `Visualización`. (Same bug in
  `de`.)
- `Go to path` → `Ir a la ruta` in all four keys (macOS uses the definite article in this family).
- `Toggle` → `Activar o desactivar …` (AppKit; the catalog's own `Mostrar u ocultar archivos ocultos`).
- `{dir}` / `{dirs}` was untranslated in the three scan-stat keys (`4 dirs`): now `carpeta` / `carpetas`, like
  `fileExplorer.summary.dirNoun`.
- `Modified` (date) → `Modificación` in all six date keys, matching Finder.
- Short commands drop the article: `Copiar nombre de archivo` (symmetric with `menu.edit.copyPath` = `Copiar ruta`),
  `Mostrar archivos ocultos`, `Actualizar los hosts de red`, `Resultados de búsqueda`.
- `Preview:` → `Vista previa:`; `New name` → `Nombre nuevo`.
- `Límite de pestañas alcanzado`, a verbless label, for both tab-limit keys.
- `Stop` → `Detener` in all three; `Drive indexing` → `Indexación de unidades`; `Create new file` →
  `Crear archivo nuevo`; `Connect to server…` → `Conectarse a un servidor…`; `Brief mode` → `Modo breve`;
  `Go to home folder` → `Ir a la carpeta de inicio`; `This volume doesn't support trash` →
  `Este volumen no admite la papelera.`

### Fronteras deliberadas (no unificar)

These are held in the term-consistency allowlist, each with its reason.

- `Both` agrees in gender with what it covers: `Ambos` (files and folders) / `Ambas` (notifications).
- `Canceled`: `Operación cancelada` titles a pane (like `Operación interrumpida`); `Cancelado` is a lifecycle status.
- `Edit`: `Edición` is the MENU (Finder), `Editar` the verb.
- `View`: `Visualización` is the MENU (Finder), `Ver` the F3 action.
- `Error`: `Problema` is a status the user reads (`fileExplorer.network.browser.status.error`'s `@key` asks to avoid the
  literal word).
- `Modified`: `Modificación` is the DATE, `Modificados` the shortcuts you changed (masculine plural with `atajos`).
- `Search`: `Buscar` is the action, `Búsqueda` the Settings topic.
- `Put back …`: `restaurar` is for NAMES, `devolver a su sitio` for PLACES.
- `you@example.com` → `tu@example.com` in all three fields (their `@key`s demand it). ❌ Never `ejemplo.com`: a real,
  registrable domain, while `example.com` is reserved (RFC 2606).
- Four "divergences" that aren't: `Connected` / `Connected!`, `Copied` / `Copied!`, `Send report` / `Send report?`,
  `Start using Cmdr` / `Start using Cmdr!` have DIFFERENT English. `i18n-terms` groups them because its normalizer
  strips trailing punctuation but not the Spanish opening `¡` / `¿`. Don't touch these eight values.

## Palabras que se separaron sin que ningún check pudiera verlo (`menu.go.parentFolder`, `licensing.dialog.typeCommercialPerpetual`/`.typeCommercialSubscription`, `errors.mutation.cantRenameVolumeRoot`, `askCmdr.renameUndo.applied`, `indexing.scan.counters`, `errors.write.trashNotSupported.suggestion`, `fileExplorer.renameConflict.overwriteTrash`, `settings.mediaIndex.clip.comingSoon`)

`i18n-terms` only groups keys with IDENTICAL English; these differ slightly, so only a manual pass finds them.

- `Parent folder` → `Carpeta superior` in the menu bar too (§ `Carpeta superior`).
- `default` → `por omisión`, no exceptions: five keys said `predeterminado` against fifteen; macOS es has zero hits for
  it (`Ajuste por omisión`, `Restaurar ajustes por omisión`, `Usar por omisión`).
- `Commercial` had stayed English in the license-type names. Rule: **follow the English capital**, which already tells
  the TIER name from the description: `Commercial license` → `licencia Comercial`, `commercial subscription` →
  `suscripción comercial`.
- **rename (the verb) → `renombrar`, even in prose**: `cantRenameVolumeRoot` and `askCmdr.renameUndo.applied` used the
  periphrasis `cambiar el nombre`, which belongs to the NOUN (`cambio de nombre`).
- `entries` → `entradas` and `dirs` → `carpetas` in `indexing.scan.counters` (it said `ítems` and an untranslated
  `dirs`).
- `permanently` → `permanentemente` (not `de forma permanente`); the F-key bar keeps `Permanente`, the tightest slot.
- `trash` → `papelera` lowercase in a sentence (`a la Papelera` was wrong; macOS reserves the capital for `Papelera`
  standing alone).
- `Coming soon` → `Próximamente` (not `Muy pronto`).

Checked and left alone: `Queued` → `Esperando` (`operationLog.status.queued`) matches `queue.row.status`'s `queued`;
`Cola` translates the noun `Queue`, another thing.

## Los nombres de los paneles salen del Mac de quien usa Cmdr (`errors.git.*`, `errors.provider.*`)

Eight values carry the placeholders `{system_settings}`, `{privacy_and_security}`, and `{files_and_folders}`, which the
app fills with the pane names the user's Mac shows. RAW values: single quotes aren't doubled.

- A preposition may precede a placeholder; an article or contraction may not (the value is unknown).
  `en {system_settings}` is fine. `errors.provider.iCloud.serious` / `.transient` also avoid a doubled `en … en …`: they
  open with `Abre {system_settings}, …`.
- `Apple Account` → `Cuenta de Apple`, `General` → `General`, `Login Items & Extensions` →
  `Ítems de inicio y extensiones`: no placeholder covers them, so they're ordinary copy (macOS 26 es,
  `AppleIDSettings.appex`, `LoginItems.appex`, verified on macOS 26.6.2 25G83, 2026-08-30).

## `Restaurar` nombra el objeto: el nombre anterior (`askCmdr.renameUndo.undone`/`.partial`)

English once shared one sentence with the trash undo ("Put back {countText} {files}.") and now says WHAT comes back.
`Put the old names back on N files.` → `Se restauraron los nombres anteriores de N archivos.`: the verb agrees with the
name, not the file, so `one` says `Se restauró el nombre anterior de 1 archivo.`

## Una operación revertida a medias: terminar la reversión (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- `Finish rolling back` → `Terminar de revertir`: inside the family (`Revertir`, `Revirtiendo`, `Revertido`,
  `Revertido en parte`); `terminar de` + infinitive says "finish what was left half-done", never "start over".
  `Completar la reversión` was longer and more formal for a button in a row. Identical in both keys (one English).
- `Finish rolling this back?` → `¿Terminar de revertir esta operación?`, after `rollbackConfirm.title`.
- The notice repeats `rollbackConfirm.bodyUndoByDeleting`'s mould:
  `Cmdr revirtió lo que pudo y dejó el resto como estaba. Si terminas la reversión, Cmdr repasa la operación otra vez y vuelve a omitir todo aquello de lo que sigue sin estar seguro.`
  Preterite for the just-finished action; `Si terminas…` turns the English gerund into direct address, and still
  promises no full rollback.
- `in {folder}` → `en “{folder}”`: macOS quotes a folder name in running text (14 hits); the quotes keep "Eliminando lo
  creado en “Backup”" from reading as if the folder itself goes.

## El aviso de una reversión que no lo pudo todo (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

The user stops a copy or move with `Revertir`; when the rollback finishes, a notice of up to three parts appears: a
headline (what the rollback did), the `leftBehind` line, and a list of reasons, each in a NAMED (`*.named`) and a
COUNTED (`*.counted`) version. The tone is always "Cmdr did the careful thing", never an apology or an alarm.

- The family copies `askCmdr.renameUndo.skipReason.*`: both surfaces are the same gesture, so the reasons share one
  formula, `{name} se quedó como está: <motivo>.` / `{countText} elementos se quedaron como están: <motivo>.` The two
  `folderNotEmpty` keys share their English with the `askCmdr` ones, so the values match word for word.
- "Left {name} alone" → `{name} se quedó como está`; "Left {name} where it is" → `{name} se quedó donde está`.
  `se quedó` makes the item the subject, which avoids a gendered clitic.
- **Two constraints at once: the brand stays AND nothing agrees with `{name}`.** "it changed after Cmdr put it there"
  admits neither `lo dejara ahí` (agrees) nor a rewrite without the brand. Solution:
  **`cambió después de que Cmdr terminara de escribir ahí`**, Cmdr in a subordinate clause with a place instead of an
  object. The `counted` version repeats the formula, because the two read together.
- "something else now sits where it came from" → `ahora hay otra cosa en su sitio` (half the length of
  `en el lugar del que salió`, and it serves one or many).
- "Couldn't undo {name}" → `Cmdr no pudo revertir {name}`: the one reason that ISN'T Cmdr's decision breaks the mould in
  English and in Spanish. Reverting a single item stretches the verb, as English stretches "undo"; fallback if a
  reviewer finds it forced: `Cmdr no pudo deshacer lo hecho con {name}`.
- "Its drive may be disconnected or read-only." → `Puede que su unidad no esté conectada o sea de solo lectura.`
- "Stopped after …" → `La reversión se detuvo después de …`: Spanish needs a subject; `detener` is macOS's stop verb.
  macOS es uses `tras` in none of its 11,676 strings, so `después de`.
- "The rest are still there." → `El resto sigue ahí.`; "The rest stayed where the move put them." →
  `El resto se quedó donde lo dejó el movimiento.`: `el resto`, not `los demás` (macOS uses that for PEOPLE); the noun
  is `el movimiento`, not `traslado`.
- **"the {countText} items" drops the numeral in `one`**: `el 1 elemento` is ungrammatical, so
  `Se eliminó el elemento que Cmdr había creado.` (precedent: `transferProgress.titleReversalDeleting`).
  `desktop-i18n-parity` compares the whole value's placeholder set, so it passes.
- **The definite article is the only thing separating `done*` from `some*`**: `los {countText} elementos` (all of them)
  against `{countText} elementos` (only those). Lose it and the partial notice promises a clean destination.
- "Cmdr had written" (items, folders included) → `que Cmdr había creado`: a folder isn't written; `escrito` stays for
  files only (`transferProgress.rollbackTooltip`).
- **The two rollback verbs never mix**: rolling back a COPY `elimina`, a MOVE `devuelve … a su sitio`. The headline must
  tell whether something was deleted or only went back.
- `leftBehind` ends on a colon and names the noun: `…, así que estos elementos se quedaron donde están:`.
- `rollbackConfirm.body` ends on the same sentence as its `bodyUndo*` siblings:
  `Cmdr omite todo aquello de lo que no está seguro, así que puede que quede alguno.`; `ha escrito` stays compound
  because the operation is still running (the exception to the preterite rule).
- **In the `*.counted` keys the verb sits in the plural OUTSIDE the branches and the `one` arm is unused**: a counted
  key only shows for two or more. It's the conscious exception to the whole-sentence rule, because
  `folderNotEmpty.counted` must match its `askCmdr` twin. If a `counted` key can ever be 1, redo all five.

### `cancelRollback.stagedLeftover.*` (los restos del propio Cmdr en el destino)

A work file Cmdr itself created and couldn't remove from the destination; not part of the `reason.*` list (there Cmdr
protects the person's files; here it's its own leftover).

- `unfinished copy` → `copia parcial` (macOS NE111 "una copia parcial que puedas completar en otro momento").
- `at the destination` → `en el destino`; `transfer` → `transferencia`.
- The second sentence leaves Cmdr as the implicit subject (`La eliminará…`) to avoid repeating the name.
- ⚠️ **`en una transferencia posterior`, ❌ never «la próxima vez».** Cmdr's cleanup skips anything younger than an
  hour, so an immediate retry deletes nothing; promising otherwise is the very bug this line fixes.

## La pantalla de bloqueo por WebKit antiguo (`main.oldWebkit.*`)

What Cmdr shows instead of its UI when the Mac's Safari is too old; the only Cmdr text that person will see.
`Software Update` → `Actualización de software` (System Settings; Finder's
`Archivo de actualización de software del dispositivo Apple`); `Quit` → `Salir`; "or newer" → `o posterior`, Apple's
formula; `Safari`, `Mac`, and `15.4` stay.

## El aviso de macOS antiguo (`main.oldMacos.*`)

Shown once on a Mac below macOS 12: Cmdr runs, but outside the tested range. Honest and relaxed, neither apology nor
warning.

- supported → `compatible` (Finder `…porque no es compatible.`), NOT the calque `soportado`.
- `X and up` → `X o posterior` (SystemSettings `… requiere OS X %@ o posterior.`).
- `best effort` → `hace lo que puede`, deliberately NOT `mejor esfuerzo` (a calque that sounds contractual).
- `look off` → `verse raros`: everyday words, away from the `error` / `fallo` register.
- The last sentence is David in the first person and keeps `tú`, like `onboarding.stepBeta.greeting`.

## Mirar dentro de un archivo: la herramienta `inspect_file` y la pantalla de consentimiento (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

Ask Cmdr can read, on request, a bounded part of a file (lines of text, pages of a PDF with its title and author, the
list inside an archive, a photo's camera data and place).

- look inside → `mirar dentro de` (no pile source; macOS says `dentro de` for inside). Tool labels
  `Mirando dentro de los archivos` / `Se miró dentro de los archivos`, the impersonal mould of `Se buscó en tus fotos`.
  Rejected `Leyendo el contenido de los archivos`: `contenido` promises more than it reads.
- thumbnail → `miniatura` (AppKit, MS, Nautilus, Total Commander).
- camera details → `los datos de la cámara`: `datos` because `detalles de la cámara` sounds like a product sheet;
  `metadatos` is right but technical for a consent text (tentative).
- location of a photo → `ubicación` in the short lists; "where it was taken" → `el lugar donde se tomó`: Spain says
  `hacer una foto`, Latin America `tomar una foto`; `tomar` reads fine on both sides (tentative, regional).
- "the list of files inside an archive" → `la lista de archivos que contiene un archivo comprimido`: `contener` breaks
  up `archivos dentro de un archivo`.
- title and author → `su título y su autor` (`creador` is MS's cloud-document sense); page → `página`.
- "whole files" → `archivos completos`; "some" three ways: `algunas líneas de texto` (countable), `algo de texto`
  (article-less list item), `parte de su texto` (with a possessor); "a limited part of it" →
  `una parte limitada de su contenido`.
- "Photo search works the same way" → `La búsqueda de fotos funciona igual`; the rest of the sentence and the closing
  one are carried over literally so the screen doesn't change voice.
- No Oxford comma in the list item (`… un archivo comprimido y los datos de la cámara y la ubicación de una foto`), as
  `ai.cloudConsent.askCmdr.item.envelope` does.
- "looks inside a file only when you ask about it" → `solo mira dentro de un archivo cuando le preguntas por él`; the
  guarantee is `nunca cambia un archivo sin tu aprobación` (Ask Cmdr proposes renames and moves, so it isn't read-only
  any more).

## Los dos textos de ayuda del botón Revertir (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- `rollbackTooltipStopAndMoveBack` → `Detener y devolver a su sitio todos los archivos movidos hasta ahora`:
  `Detener y …` from `rollbackTooltip`, `devolver a su sitio` for the return. ❌ Not `eliminar`: undoing a move deletes
  nothing.
- `rollbackAlreadyLandedTooltip`: the first sentence echoes `cancelRollback.moveAlreadyLanded`
  (`ya está en el destino`), `reversión` is the settled noun, and `Cancelar` is the neighbouring button's label,
  verbatim.

## “Abrir terminal aquí” y su selector de app (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- terminal (the app type) → `terminal`; Terminal (Apple's app) → `Terminal`: Apple's Spanish keeps the name
  (`Abrir en Terminal`, Finder N67), and the generic Spanish word is the same loan. The card title carries
  `sameAsSourceJustification`.
- Open terminal here → `Abrir terminal aquí`, built on Apple's `Abrir en Terminal`; the command itself (menu, palette)
  must use exactly this.
- Choose an app… → `Seleccionar app…` (Apple `Seleccionar aplicación…`, with the catalog's `app`).

## `Sort by relevance`: la ayuda de la columna de resultados (`fileExplorer.columns.sortByRelevance`)

relevance → `relevancia` (WorkflowKit, AppStoreKit, Automator, Music; lowercase after `por`), framed like the sibling
commands: `Ordenar por relevancia`.

## `Documents and packages`: la fila OOXML (`settings.archives.ooxml.*`)

A row covering BOTH Office documents (.docx, .xlsx, .pptx) and app packages (.jar, .apk), which is why English names no
Office. documents → `Documentos` (Finder); packages → bare `paquetes`, so the row stays broader than the
`Paquetes de apps` card below it. Framed like its siblings: `Qué hace pulsar Intro en un …, … o ….`

## El hub de servidores: conectar, desconectar y olvidar (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

The panel that shows a server connection's state (SMB / SFTP / WebDAV), the connection-dot tooltips in the volume
chooser, and the two "forget" confirmations. Tier 1 from the live Mac (macOS 26.6.2, 25G83, `plutil -convert json` over
the system bundles, 2026-09-06).

- Disconnect → `Desconectar` (Finder MR10.1, N200); Connect → `Conectar`; Connecting to X… → `Conectándose a X…` (Finder
  MN1, the reflexive gerund); Connect to a server → `Conectarse a un servidor`.
- key (a server's SSH key) → `clave` (Keychain Access `clave privada`, `clave pública`). ❌ Not `llave`: macOS es uses
  it only for passkeys and `llavero`.
- Keychain Access → `Acceso a Llaveros`; keychain → `llavero`.
- compromised → `comprometida` (Apple `Contraseña comprometida`). Apple's other form, `filtrada`, names a concrete LEAK;
  a `@revoked` mark in `known_hosts` is an admin's flag, not a leak.
- trusted → `de confianza`; to trust → `confiar en`; English makes macOS and Cmdr active subjects, so
  `macOS no confía en el certificado de este servidor`, `Cmdr todavía no confía en la clave de {host}`.
- Signed out → `Sesión cerrada` (Finder NE103); it agrees with `sesión`, not the reader. sign-in method →
  `método de inicio de sesión`.
- **Try again as a BUTTON → `Reintentar`**, as the catalog's five button labels already do; `Inténtalo de nuevo` /
  `vuelve a intentarlo` are for PROSE.
- "Can't X while operations are in progress on this Y" copies `fileExplorer.navigation.ejectBusyTooltip` word for word
  (`No se puede desconectar mientras hay operaciones en curso en este servidor`), no full stop.
- `Olvidar el servidor` and `Olvidar la contraseña guardada` repeat the menu items exactly; the `…Busy` items add
  ` (ocupado)`.
- drop the connection → `cortar la conexión` (`errors.listing.connectionDropped.explanation`); reach → `acceder a`,
  reachable → `accesible`; doesn't support → `no admite`; this Mac → `este Mac`.
- **Gender, agents, placeholders**: no value genders the reader. Participles agree with fixed nouns (`Sesión cerrada`,
  `Guardado` with `servidor`, `comprometida` with `clave`); clitics point only at fixed-gender nouns (`Ábrelo` → el
  servidor, `recuperarla` → la conexión, `te la pedirá` → la contraseña). In these keys `{name}` and `{host}` are always
  a server, and nothing agrees with them anyway.
- `hostKeyRevoked` avoids two `en`: `Tus ajustes de SSH en este Mac marcan la clave de {host} como comprometida.`; the
  second sentence names the object (`a ese servidor`) instead of an ambiguous `a él`.
- `notAWebdavServer` inverts: `En esta dirección no hay nada que responda a WebDAV.`
- `fileExplorer.navigation.disconnectPlaceAriaLabel` = `Desconectar {name}`, after `ejectVolumeAriaLabel`; the WCAG
  2.5.3 substring is `Desconectar`, the exact word of `servers.paneState.disconnect`.

## La tabla del hub de servidores y sus comandos (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`serverUnpinnedToast`/`pinRefusedToast`/`networkVolume`, `shortcuts.scope.servers`/`places`)

The `Servidores` row of the volume chooser opens a hub with a table (Nombre / Tipo / Dirección / Estado / Último uso),
its `Añadir servidor…` row, the empty state, the discovery-off line, and five palette commands.

- Status → `Estado` (Network.appex), Address → `Dirección` (no `del servidor`: the table is of servers), Type → `Tipo`
  (Disk Utility), Name → `Nombre`, Connected → `Conectado` (also forced by `ai.cloud.connected`), Never → `Nunca`, Local
  Network → `red local` (lowercase in a sentence), Saved → `Guardado`.
- `Servers` (the row and heading) → `Servidores`, in both keys of that English. `Red` is now only the GROUP
  (`fileExplorer.navigation.groupNetwork`).
- `Places` (the shortcuts heading under a server) → `Ubicaciones`, the concept of Finder's sidebar `Locations` (SD5):
  today a host's shares, tomorrow a storage account's buckets. NOT `Recursos compartidos` (today's case only). Nautilus
  says `Lugares`; Apple wins.
- `Last used` → `Último uso`: macOS date columns are nouns (`Última apertura`), as `Modified` → `Modificación`. ❌ Not
  `Última conexión`: English chose "used" so the column survives other kinds of use (the exact word is tentative).
- `Found nearby` → `Encontrado cerca` (encontrar for network discovery, cerca from AirDrop); masculine like `Conectado`
  and `Guardado`.
- `Waiting for you to check the key` → `Esperando a que compruebes la clave`. The ban on starting with `Esperando` is
  the QUEUE's status column; this is another table.
- `Local network discovery is off.` → `La detección de servidores en la red local está desactivada.`: `descubrimiento`
  sounds like a finding. `Turn it on in Settings` → `Actívala en Ajustes` (`-la` agrees with `la detección`).
- `Add server…` → `Añadir servidor…`, no article for something NEW; commands on the server under the cursor keep it
  (`Olvidar el servidor`, `Desconectar el servidor`).
- `No servers yet` → `Aún no hay servidores` (the catalog's `Aún no hay X` mould).
- `emptyMessage`: imperative + future
  (`Añade uno aquí abajo, o enciende un Mac o un NAS de tu red y Cmdr lo encontrará.`); `aquí abajo` because bare
  `abajo` only leans on a noun.
- `Pin / unpin server` → `Fijar o desfijar el servidor`: `o`, not a slash (`commands.tabTogglePin.label`, macOS
  `Activar o desactivar …`).
- The other four commands copy their published siblings word for word; the pin toasts reuse `selector de volúmenes`,
  name the noun (`El servidor sigue guardado.`) rather than hang a participle on `{name}`, and
  `Cmdr no pudo cambiar dónde aparece {name}.`

## La hoja para conectarse a un servidor y la confianza en la clave de host (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`signIn`/`hostKeyChanged`/`hostKeyChangedHint`, `goToPath.dialog.opensServer`/`addsServer`, `commands.serversConnect.label`)

Tier 1 from the live Mac (macOS 26.6.2, 25G83, 2026-09-07).

- fingerprint → `huella digital` (ActionKit "La huella digital de la clave de host es %@", Security.framework "Huellas
  digitales"). ❌ Not bare `huella`: Apple keeps that for Touch ID. host key → `clave de host`.
- Connecting… with no target → `Conectando…` (NetAuthAgent); NOT `Conectándose…`, which is for a named target. It also
  has to match `fileExplorer.network.connecting` (`i18n-terms` ignores trailing dots).
- Save → `Guardar`; Protocol → `Protocolo`; Browse (a picker button) → `Explorar` (forced by
  `settings.archives.opt.browse`); Sign In… → `Iniciar sesión…`.
- «X is signed out» → Finder NE103 `Se ha cerrado la sesión de “X”`, hence the preposition of
  `Sesión cerrada de {name}`.
- Remember this password in my keychain → Apple's `Guardar esta contraseña en mi llavero`; Cmdr ships the short
  `Recordar en el Llavero` (`servers.sheet.remember`), quoted identically in `servers.sheet.needsStoredSecret`.
- Guest → `Invitado`; Connect As: → `Conectar como:`; passphrase → `contraseña` (Apple doesn't distinguish); Use X
  (checkbox) → `Usar X`; reconnect automatically → `volver a conectar automáticamente`; remote → `remoto`; hostname →
  `nombre de host`.
- Sign in to X → `Iniciar sesión en X` (CloudKit fixes `en`; Cmdr uses the infinitive so the three sheet titles run
  parallel).
- **The three sheet titles are infinitive**: `Añadir servidor` / `Iniciar sesión en {name}` / `Editar {name}`.
- Fourteen values are forced by `i18n-terms`, not chosen: `Conectar`, `Iniciar sesión`, `Cancelar`, `Conectando…`,
  `Dirección`, `Nombre de usuario`, `Contraseña`, `Recordar en el Llavero`, `Conectarse como invitado`, `Avanzado`,
  `Nombre`, `Explorar…`, `Iniciar sesión…`, `Conectarse a un servidor…`. Before translating a new sheet, search the
  exact English elsewhere in the catalog.
- `Key passphrase` → `Contraseña de la clave`; `Key file` → `Archivo de clave`; `How to connect` → `Cómo conectarse`.
- `Trust and connect` → `Confiar en la clave y conectar`: `confiar` governs `en`, so the calque `Confiar y conectar`
  isn't Spanish.
- `I've checked it` → `Ya la comprobé` (preterite; `la` agrees with `la huella digital` / `la clave`).
- `First time connecting to {host}` → `Primera conexión con {host}` (NetAuthAgent
  `establecer conexión con el servidor`).
- owner (of a server) → `quien posee el servidor`, avoiding the gendered `el propietario`.
- "something is sitting between you and it" → `hay algo entre tú y él`: Apple says «un ataque tipo “Man-in-the-middle”»,
  jargon English avoids on purpose.
- `Cmdr stopped connecting to {name}` → `Cmdr dejó de conectarse a {name}`; `Opens {name}` / `Adds a server` →
  `Abre {name}` / `Añade un servidor`.
- `needsStoredSecret` is impersonal (`Para volver a conectar por su cuenta hace falta una contraseña recordada.`), then
  `Activa … e inicia sesión una vez` (`e` before `i-`).
- Four keys are identical to English with `sameAsSourceJustification`, each because macOS es keeps the token: `SMB`,
  `SFTP`, `WebDAV`, and the example host `nas.local`.

## Las dos líneas nuevas del panel: reconexión automática y una sesión sin nada que teclear (`servers.paneState.reconnecting`/`signedOutNothingToAsk`)

- Apple splits "Reconnecting…" between `Conectando de nuevo…` (ScreenSharing, FaceTime) and `Reconectando…` (Home); both
  are Apple's, so internal parallelism decides. reconnect to X → `reconectarse a X` (Displays: "Reconectarse
  automáticamente a cualquier Mac o iPad cercano").
- `Reconnecting to {name}…` → `Reconectándose a {name}…`, the twin of `servers.paneState.connecting`
  (`Conectándose a {name}…`), which the user sees in the same panel.
- It diverges on purpose from `errors.listing.deviceReconnecting.title` (`Reconectando con el dispositivo`, a USB device
  with no placeholder and no on-screen sibling). Different English, so legal; move both together if they're ever
  unified.
- type → `escribir` (Apple's dominant form); `introducir` is kept for credentials. English chose the plain physical
  verb, and the sentence names no credential: there's NOTHING to type.
- The long line is impersonal: `En este servidor se inicia sesión con una clave…`; the calque
  `Este servidor inicia sesión` would make the server the one signing in.
- `rather than a password` → `en lugar de una contraseña` (the "Y, not X" mould is banned).
- `Open it again to retry.` → `Vuelve a abrirlo para reintentar.`: `vuelve a abrirlo` from `hostKeyChangedHint`;
  `reintentar` avoids a second `volver a` and matches the panel's `Reintentar` buttons. `lo` agrees with `el servidor`.

## Fijar servidores en el selector, las claves de host de confianza y la página de ADB (`menu.network.pinToSwitcher`/`unpin`, `servers.pinHint.*`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.servers.*`, `settings.adb.*`, `settings.appearance.tintSmb.*`)

- Trusted X → `X de confianza` (Network.appex "Servidores de confianza", FindMy, AppleAccount); `Trusted host keys` →
  `Claves de host de confianza`. ❌ Not `fiable`: Apple keeps it for a machine's verdict on a certificate.
- Check Again → `Comprobar de nuevo` (SoftwareUpdate, Mail ConnectionDoctor), the translation of `Re-check`.
- not found → `no encontrado`; Located at %@ → `está en %@` (FindMy).
- **pin / unpin → `fijar` / `desfijar`, NOT Safari's `anclar`**: the catalog shipped the whole family with `fijar`
  (`menu.tab.pinTab`, `menu.tab.unpinTab`, `commands.tabTogglePin.label`, `commands.serversTogglePin.label`, which is
  the very command these menu items run). Switching would split the family (if ever changed, move all five at once).
- `Pin to switcher` → `Fijar en el selector` (`fijar` governs `en`); `Unpin` → `Desfijar`.
- `Your Network group is getting long` → `Tu grupo Red se está haciendo largo`; `the Servers list` →
  `la lista Servidores`, names unquoted like `el menú Ayuda`. Differs on purpose from `la lista de servidores` in
  `fileExplorer.network.browser.removeHostConfirm`, whose English names no row.
- `It stays in the Servers list.` → `Seguirá en la lista Servidores.` (the future carries the promise).
- `Trusted` + date → `De confianza desde 2026-09-07` (a bare participle has no Spanish equivalent without an article;
  length tentative).
- `Favorites work the same way.` → `Los favoritos funcionan igual.` (plain, not the calque `de la misma manera`).
- `The SSH host keys you have trusted.` → `Las claves de host SSH en las que has confiado.` (`confiar` governs `en`, so
  the relative carries the preposition).
- `Forget` (the row button) → `Olvidar`.
- `Saved servers live in the Servers list, not on this page.` →
  `… están en la lista Servidores; esta página solo guarda las claves.` (the "Y, not X" mould is banned).
- `Nothing trusted yet.` → `Aún no hay ninguna clave de confianza.`
- `Watching for phones.` / `Not watching for phones right now.` → `Cmdr detecta los teléfonos en cuanto los conectas.` /
  `Ahora mismo Cmdr no detecta los teléfonos al conectarlos.`: Spanish needs a subject; `detectar` is the catalog's verb
  for noticing a device. ❌ Not `Buscando teléfonos` (a search in progress), ❌ not `pendiente de` (unresolved). The
  whole sentence is tentative.
- `Choose the adb command` → `Seleccionar el comando adb`; `Look for adb the usual way` →
  `Buscar adb en los sitios habituales`.
- `Install the Android platform tools, then press Re-check:` →
  `Instala las herramientas de plataforma de Android y después pulsa Comprobar de nuevo:`.
- `Tint server panes (SMB, SFTP, WebDAV)` → `Teñir paneles de servidor (SMB, SFTP, WebDAV)`, after
  `Teñir paneles de volúmenes locales` and `Teñir paneles MTP`.
- `settings.section.adb` (`Android (ADB)`) is identical to English: both names stay in macOS es.

## El teléfono Android por ADB: el panel de conexión, los avisos del selector y la línea de invitación (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Android's own vocabulary comes from AOSP `main` (2026-09-07), the authority for what the user reads ON THE PHONE.

- `USB debugging` → `Depuración por USB` (AOSP `enable_adb`, SystemUI `usb_debugging_title`; `values-es-rUS` identical).
  ❌ Not `depuración USB`.
- `Allow` (the phone's button) → `Permitir`, unquoted, the one catalog word quoted because the user must find it on
  ANOTHER screen.
- `tap` → `tocar` (AOSP throughout; macOS has no touch verb).
- `Android platform tools` → `herramientas de plataforma de Android` (tentative against Google's long
  `Herramientas de la plataforma del SDK de Android`; all keys move together).
- Waiting for X → `Esperando X` (FaceTime, Freeform, Podcasts); wake (a device or screen) → `activar`, ❌ not
  `despertar` (Clock's wake-up, the person); is not responding → `no responde`; no longer connected →
  `ya no está conectado`; too old → `demasiado antiguo/a`; Try another X → `Prueba con otro X`, always with `con`;
  reconnect the cable / reseat → `vuelve a conectar el cable`; Check your X → `Comprueba tu X` (here: look at the
  phone's screen); port → `puerto`; lost the connection to X → `perdió la conexión con X` (`con`, not `a`).
- `Cmdr couldn't find the Android platform tools.` → `Cmdr no pudo encontrar las herramientas de plataforma de Android.`
- `Open Settings` → `Abrir Ajustes`, capitalized, no article: it names the WINDOW (`settings.window.title`), like
  `downloads.fda.openSystemSettings`. It differs on purpose from `commands.appSettings.label` and
  `commands.handler.openTerminalHere.openSettings` (`Abrir los ajustes`, the concept, for a lowercase English). Move all
  three if they're ever unified.
- `The Android tools on this Mac didn't answer.` → `Las herramientas de Android en este Mac no respondieron.`
  (`de este Mac` would double the `de`).
- `This phone's Android version is too old for Cmdr to browse.` →
  `Este teléfono tiene una versión de Android demasiado antigua para que Cmdr pueda explorarlo.` (the phone as subject,
  `-lo` agrees with `teléfono`).
- `Cmdr opens your phone as soon as you do.` → `Cmdr abrirá tu teléfono en cuanto lo hagas.` (future: a promise).
- `Waiting for you to allow USB debugging` → `Esperando a que permitas la depuración por USB`.
- `Wake its screen, or reseat the cable.` → `Activa su pantalla o vuelve a conectar el cable.` (no comma before `o`).
- `Try another cable or port.` → `Prueba con otro cable u otro puerto.` (`u` before `o-`; `otro` repeated).
- `Want the whole filesystem? Turn on USB debugging.` →
  `¿Quieres todo el sistema de archivos? Activa la depuración por USB.`, echoing
  `settings.fileOperations.adbEnabled.description`.
- `How` → `Cómo se hace`: bare `Cómo` isn't an idiomatic link label (macOS never has it alone); tentative against
  `Más información` (the link leads to concrete INSTRUCTIONS).
- `Disconnect {name}` → `Desconectar {name}`, identical to the servers sibling;
  `…while operations are in progress on this device` copies `disconnectBusyTooltip` with `dispositivo` (also covers a
  tablet).
- The two internal keys follow `serversPinHintSeen.*`: `Aviso de depuración por USB descartado` /
  `Si ya se descartó el aviso único que propone activar la depuración por USB.`
- `You stopped opening your phone.` → `Detuviste la apertura de tu teléfono.` (`detener`, as
  `search.coverage.walk.cancelled` `Detuviste esta búsqueda`). ❌ Not `Cancelaste`: it would read as naming the Cancel
  button. `apertura` is tentative (no other key uses it).
- None of the 19 strings says `error`, `fallo`, or `no se pudo`: each says what is happening and, where there is one,
  the way out.

## La identidad bloqueada del servidor (`servers.sheet.identityLocked`)

- the account → `la cuenta` (`servers.sheet.needsStoredSecret`, `errors.json`).
- The notice names the actions like the buttons it points at: `olvidar` (`menu.network.forgetServer`) and `añadir`
  (`servers.sheet.addTitle`). A synonym sends the reader looking for a menu that doesn't exist.
- `are what name this server` → `son las que identifican este servidor`: the sheet has its own `Nombre` field, so "dar
  nombre" would read as that label.

## El aviso cuando no había ninguna contraseña guardada (`fileExplorer.navigation.forgetSecretNoneToast`)

`There was no saved password for {name}.` → `No había ninguna contraseña guardada de {name}.`: the term and the
preposition of the three siblings (`menu.network.forgetSavedPassword`,
`fileExplorer.navigation.forgetSecretConfirmTitle`, `.forgetSecretConfirm`). The imperfect `No había` gives the
matter-of-fact tone the `@key` asks for; `guardada` agrees with `contraseña`, never `{name}`.

## La duración del reintento, el título de la clave del servidor y el botón Permitir de Android (`servers.paneState.retryTotalSeconds`/`retryTotalMinutes`, `servers.paneState.retryKeepsTrying`, `servers.refusal.hostKeyRevoked`)

- `{seconds}` / `{minutes}` take an ICU plural with TWO placeholders: the integer selects, `{secondsText}` is read. All
  three arms (`one`, `many`, `other`); the forms come from `main.quit.countdown` and `indexing.eta.hoursMinutesLeft`.
- Both values are pieces of `retryKeepsTrying` (`Se seguirá intentando durante un total de {duration}.`), so they're
  bare: no preposition, no full stop.
- `Cmdr won't connect to {name}` → `Cmdr no se conectará a {name}`, the future of `hostKeyRevoked`; the older
  `dejó de conectarse` sounded like an interrupted attempt.
- `Allow`, Android's button → `Permitir`, copied from `adb.connect.unauthorized` (`toques` from `toca`).

## El menú contextual de la fila del servidor: `Abrir` y `Editar el servidor…` (`menu.network.open`, `menu.network.edit`)

- `Open` on a server row → `Abrir`, identical to `menu.file.open`: the same sense (enter), and neither Spanish nor
  Finder splits it (`Abrir`, `Abrir con`, `Abrir en una ventana nueva`).
- `Edit server…` → `Editar el servidor…`, byte for byte `commands.serversEdit.label`; the single `…` stays.
- Both equalities are checked by `i18n-terms`. `menu.*` is RAW: single apostrophes; a doubled `''` fails `i18n-icu`.

## La barra de teclas de función (`settings.appearance.showFunctionKeyBar.label`)

function key bar → `barra de teclas de función`, reused for the context-menu item and its notice.

## El aviso de «¿dejamos Cmdr en el Dock?» (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `Dock` → `Dock`, verbatim and masculine (`el Dock`, `al Dock`; Finder `Añadir al Dock`).
- `Finder` → `el Finder`, with the article (Finder A34 `Mostrar en el Finder`); here `junto al Finder`.
- `Applications` folder → `la carpeta Aplicaciones`, no quotes (Finder TL_HELP_APPS, AppKit "hasta la carpeta
  Aplicaciones", the exact model of `notAdded`).
- `configuration profile` → `perfil de configuración`; the verb `controla` from `Un perfil controla estos ajustes.`
- **pin / unpin in the Dock → `fijar` / `desfijar`**, not the Dock menu's `Mantener en el Dock` / `Quitar del Dock`: the
  family stays on `fijar`, and English says pinned/unpin.
- `Cmdr` is masculine when a pronoun stands for it (`fijarlo`, `desfijarlo`, `arrastrarlo`), like `el Finder` and
  `el Dock`, avoiding a clash with `la app`.
- «a few days» never becomes a number: `Llevas unos días usando Cmdr` (the threshold may move; any figure would lie).
- The four result keys say what's happening, not that something failed. `addedButDockDidNotRestart` states the positive
  fact first (`El icono de Cmdr ya está en su sitio`), then `pero el Dock todavía no lo muestra`: the `todavía` stops it
  reading as "not pinned". `managedDock` → `Quien administre este Mac` (not AppKit's gendered `el administrador`).
  `notAdded` → `Cmdr no pudo entrar en el Dock esta vez`.
- `Yes, add it to my Dock` → `Sí, añadir a mi Dock` (infinitive button, Finder's ellipsis); `No, thanks` →
  `No, gracias`; `change your mind` → `cambiar de idea`.
- The internal keys: `Oferta del Dock hecha` / `Si ya se hizo …`.

## El menú del icono en el Dock (`menu.dock.*`)

Tier-1 source: the Dock itself (`/System/Library/CoreServices/Dock.app/Contents/Resources/es.lproj/DockMenus.strings`,
macOS 26.6.2, 2026-09-09) plus Finder's `MenuBar.strings`. RAW family.

- `Open Cmdr` → `Abrir Cmdr`: `DockMenus.strings` forms "verb + app name" with no preposition or article (`Mostrar %@`,
  `Ocultar %@`).
- `Search files…` → `Buscar archivos…` (= `menu.edit.searchFiles`).
- `Go to folder…` → `Ir a la carpeta…` (Finder `261.title`). Cmdr's own dialog is `Ir a la ruta` because its English
  says path; this item says folder. Don't unify.
- `Connect to server…` → `Conectarse a un servidor…` (Finder `266.title`).
- `{name} ({parent})` stays identical, with `sameAsSourceJustification`: pure punctuation around two folder names, the
  pattern macOS publishes unchanged; nothing may agree with `{name}`.

## La oferta de «Mostrar en el Finder» y el aviso de la primera vez (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- “Show in Finder” → `“Mostrar en el Finder”`, curly quotes, as in `settings.navigationAndFileOps.card.showInFinder`;
  the toasts copy it so card and notice name one action.
- pane → `panel`; Settings (Cmdr's window) → `los Ajustes`.
- “for a while now” → `un tiempo`: deliberately vague, ❌ never a number (same rule as `unos días`).
- The first-time notice ❌ isn't an apology: what happened, why, where the switch is
  (`Cmdr está configurado para recogerlos`).

## Las 23 claves del rediseño de la introducción (`onboarding.*`)

- **star (GitHub's verb) → `dar una estrella`**; the button is `Estrella` (GitHub's Spanish UI and docs,
  `marcar con una estrella`); in a checklist row `Dale una estrella al repo en GitHub`, already the catalog's. ❌ Not
  `Destacar`.
- `repo` → `repo` (MS publishes it beside `repositorio`; the English is deliberately casual).
- **like (AlternativeTo's button) → `dar un me gusta`**: AlternativeTo is English-only, so Tier 2 decides (MS like → me
  gusta); unquoted.
- checklist → `lista de tareas` (MS Kaizala): four things you do and tick off;
  `Lista de comprobación de la introducción` runs to 62 characters.
- `Save` (beside the email field) → `Guardar`, unquoted in `signup.rejected` / `signup.unreachable`.
- `mailing list` → `lista de correo`, the short form of MS's `lista de distribución de correo`.
- `typo` → `errata` (tentative): no pile source; it dodges the `error` ban.
- `sign up` → `dar de alta` (the catalog's `darte de alta`, over MS `registrarse`); `signup server` →
  `servidor de altas` (tentative).
- `More about {topic}` → `Más información sobre {topic}` (Finder `Más información sobre iCloud`).
- `handler` (the macOS process MTP displaces) → `proceso`, matching `onboarding.stepOptional.mtp.desc`; `controlador`
  stays free for a real driver.
- `badge` → `insignia`.
- `machine` ("no data leaves your machine") → `tu equipo`: English says machine, not Mac. macOS es writes `ordenador`
  (43 against 1), but it's Spain-only; `equipo` is MS's word and already in `errors.json`. Two `settings.json` keys
  still say `ordenador`: open decision in `review-queue.md`.
- `{nextLabel}` in curly quotes: a label Cmdr paints (`onboarding.wizard.next`); `alternativeToNote` keeps the straight
  quotes around `"Cmdr"` (a foreign page title), and `networking.summary` around `"Red local"`.
- `Local Network` → `"Red local"`, Apple's literal label (`SecurityPrivacyExtension.appex` `LOCAL_NETWORK`, macOS
  26.6.2). ❌ Not the paraphrase `Acceso a la red local`: that name isn't in System Settings.
- `superprivate` → `superprivada`, agreeing with `la IA`: the English rewrite let the gendered `superprivado` go.
- `dumber` → `más torpe` (tentative): frank, like the English, without insulting or joking (`tonto`).
- released copy / Dev and test builds → `versión publicada` / `compilaciones de desarrollo y de prueba`
  (`settings.revealHandler.notProductionBuild`; MS build → compilación).

## El panel del visor mientras llega el archivo (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Fetching → `Obteniendo` (Finder IN_MD1, AppKit `Obteniendo versiones`). ❌ Not `Descargando`: a local archive isn't
  downloaded.
- to preview it → `Obteniendo el archivo {fileName} para previsualizarlo`: the noun first, so `lo` agrees with
  `el archivo`, never `{fileName}`.
- `{doneText} of {totalText}` → `{doneText} de {totalText}`; so far → `hasta ahora`, no participle (a `recibidos` would
  agree with the formatted unit).
- stopped arriving → `Dejaron de llegar los datos de este archivo` (`Este archivo dejó de llegar` is a calque); the
  second sentence copies `errors.listing.couldntReadUnknown.suggestion` and `ai.translateError.unavailable.body`.

## El índice desactualizado de un teléfono por ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`bodyPhone`)

The phone is still plugged in, so none mentions a disconnect.

- keep the index current → `mantener … al día` (`Indexada y al día`, `lo dejará al día`, `ponerlo al día`).
- `El índice de este teléfono` after `El índice de esta unidad`; after a rescan → `tras un nuevo análisis`.
- `{name} doesn't tell Cmdr` → `{name} no avisa a Cmdr` (`{name}` as subject of a genderless verb).
- the changes Cmdr makes itself → `los cambios que hace el propio Cmdr`. ❌ Not `mantiene su índice`: `su` could be
  either's.
- stays as a reminder → `sigue ahí para recordártelo` (tentative register).

## La carpeta raíz y la carpeta inicial de un servidor guardado (`servers.sheet.rootFolder`/`rootFolderHelp`/`startFolder`/`startFolderHelp`/`nameHelp`, `servers.refusal.startFolderOutsideRoot`/`rootNotFound`/`startFolderNotFound`/`saveUnconfirmed`)

- root folder (the ceiling Cmdr never climbs above) → `Carpeta raíz` (Double Commander, MS root → raíz), replacing the
  old label `Carpeta remota`: the field says where you can't leave, and `raíz` tells it.
- start folder → `Carpeta inicial` (Double Commander `&Ruta inicial:`; Nautilus `Tamaño inicial`). ❌ NOT
  `carpeta de inicio`: that's the home folder in this catalog (`commands.navGoHome.label`).
- «Where the server opens.» → `Dónde se abre el servidor.` (indirect question with the accent). ❌ Don't name
  `la carpeta` there: `Déjalo vacío` would read as "empty the folder" (tentative).
- «Leave it empty to X» → `Déjalo vacío y` + future.
- «call this server by its account and host» → `usará la cuenta y el host como nombre del servidor`. ❌ NOT
  `por su cuenta` (spent on "on its own").
- «Check that it exists and that your account can read it» → `Comprueba que exista y que tu cuenta pueda leerla`
  (subjunctive; `-la` agrees with `carpeta`).
- «didn't answer in time, so nothing was saved. Try again in a moment.» →
  `no respondió a tiempo, así que no se guardó nada. Inténtalo de nuevo en un momento.`

## Por qué no se monta un recurso compartido o no carga su lista (`errors.mount.*`, `errors.shareList.*`)

Tier 1 from the live `NetAuthAgent.app` (macOS 26.6.2, 25G83, 2026-09-11), which words these cases for "Connect to
Server".

- share → `recurso compartido` (NetAuthAgent `EINFO_NO_SHARES`; its `EINFO_NO_SHARE` says `volumen`, but the catalog
  settled `recurso compartido`).
- guest access → `acceso como invitado`; reach → `acceder a`; didn't answer in time → `no respondió a tiempo`.
- this computer → `este equipo` (these lines also run on Linux, so not `Mac`; tentative).
- You're signed in → `Tienes la sesión iniciada en …`; when you're ready → `cuando quieras` (no `listo/lista`).
- package → `paquete` (Dolphin); distribution (Linux) → `distribución` (tentative).
- Same English, same Spanish: `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`. Curly quotes, like `errors.volume.permissionDenied`.

## F4 y su editor de texto (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- text editor → `editor de texto`, the app category, ❌ not TextEdit (which arrives in `{app}`).
- default text editor → `editor de texto por omisión`; system default → `por omisión del sistema`, with the app name in
  parentheses after it (`settings.appearance.language.opt.systemWithLanguage`).
- Edit files in [app] → `Editar archivos con` (tentative; `con` takes any app name without an article).
- `{app}` follows `en` with no article. Dismiss, Open settings, Choose an app…, and Checking your apps… match the
  terminal siblings.

## Una unidad que se va a mitad de petición (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`)

- is being disconnected (eject or unmount under way) → `se está desconectando` (Thunar `Desmontando el dispositivo`);
  the reflexive doesn't agree with `{name}`. «Left its index as it was» → `como estaba`; «try again in a moment» →
  `en un momento`.
- was disconnected (it's gone) → `se desconectó`, the preterite; «Starts from scratch» → `desde cero`.

## Una unidad arrancada a mitad de transferencia (`errors.write.deviceDisconnected.*`)

Read right after someone pulled the cable mid-copy; the sentence exists to say WHERE the files are.

- was disconnected → `se desconectó` (the reflexive avoids agreeing with `{volumeName}`).
- **drive (it disconnected on its own) → `unidad`**, not `disco`: the closest siblings and
  `errors.write.destinationNotFound.suggestion` say `unidad` for a lost connection. **Deliberate boundary with §
  Expulsar y desconectar**, where the eject corpus rules `disco`. Don't unify.
- the move → `el movimiento`; your originals are untouched → `tus originales siguen intactos` (the siblings' `intactos`,
  the possessive kept; `siguen … donde estaban` carries "where they were").
- so nothing is lost → `así que no has perdido nada`, not the impersonal `no se pierde nada`, which hides whose files
  they are (macOS also addresses the reader about loss; tentative). The participle with `haber` is invariable.
- the rest are still on the drive → `el resto sigue en la unidad`.
- to it (the disk being copied to) → `ahí`: an adverb of place with no gender, per `style.md`.
- after Cmdr copied … → `después de que Cmdr copiara …` (subjunctive, Cmdr as subject, the brand kept).
- `{counterpart}` always follows a bare preposition, never an article.

## Un movimiento que no se pudo confirmar (`errors.write.moveNotConfirmed.*`)

Neither a failure nor a loss: Cmdr couldn't PROVE the copies landed, so it kept the originals. The four keys can't imply
the move went wrong or the files aren't at the destination.

- Couldn't confirm … → `No se pudo confirmar …`; the moved files were saved → `que los archivos movidos se guardaran`.
- it kept your originals → `conservó tus originales`, not `dejó`: `conservar` is macOS's verb for keeping something safe
  on purpose (`Conservar original`, `Conservar copia parcial`); `dejar` reads as carelessness.
- Have a look at the destination → `Revisa el destino`, the catalog's verb for checking after an unconfirmed operation
  (`Revisa la Papelera para asegurarte`).
- Your originals haven't moved. → `Tus originales siguen donde estaban.` (the literal forces `no se han movido`,
  peninsular, or `no se movieron`, a chronicle; the positive statement reassures; tentative).

## La carpeta de trabajo de un movimiento que volvió con la unidad (`fileOperations.leftovers.stagingFolderKept`)

A notice on reconnecting a drive: the staging folder of a move that never finished is still there, with files inside.
Cmdr leaves them on purpose (they may be the person's only copy). ❌ Never imply they can be deleted.

- unfinished move → `un movimiento sin terminar`. ❌ Not `movimiento parcial` (`parcial` is for the ARTEFACT,
  `copia parcial`, and would promise part of it worked), ❌ nor `interrumpido` (a cut connection).
- left them in place → `los conservó donde estaban`, not `los dejó`: same verb and close as
  `errors.write.moveNotConfirmed.message.named`.
- hidden folder → `una carpeta oculta` (the catalog and four sources); saying it is mandatory, since without showing
  hidden files the folder is invisible.
- named {folderName} → `llamada {folderName}` (Finder `Crear una carpeta llamada ${fileName} dentro de ${target}`):
  `llamada` agrees with `carpeta`, never the real name. `{volumeName}` follows a bare `en`.

## El menú de favoritos: ⌃D, las teclas 1–9 y la fila que lo abre desde el selector (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`favoritesAlreadyAdded`/`favoritesCantAddHere`/`seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D opens the saved folders as a menu over the active pane; the first nine rows take a number key, the last (`0`) adds
the pane's folder. The volume chooser has one row that opens this menu.

- favorites → `favoritos`; capital only as the chooser heading. ❌ Never `marcadores`: Double Commander's and Dolphin's
  word for another feature (their Directory Hotlist, Nautilus's Bookmark).
- favorites menu → `Menú de favoritos` (Finder `Menú de búsqueda`; the neighbouring headings are all noun + `de` +
  noun).
- Show favorites → `Mostrar favoritos` in both keys of that English; See {count} favorites → `Ver {count} favoritos`:
  **deliberate `ver` / `mostrar` boundary**, because English separates the chooser row's See from the command's Show.
  macOS es screen-reader descriptions that start with See do say `Mostrar`; another register, and they lose to internal
  consistency.
- `seeFavorites` takes `=0` + `one` + `many` + `other`: Spanish CLDR has `one`, `many`, `other`, not English's set;
  `many` repeats `other`; `=0` → `Ver favoritos`, the only arm without `{count}`.
- current folder → `la carpeta actual`, with the article: it's a specific existing folder (the no-article `Añadir X` is
  for something new).
- Add current folder to favorites → `Añadir la carpeta actual a favoritos`, one key; the shortcut list quotes the same
  row for the `0` key. Keep the distance from `commands.favoritesAdd.label` (`Añadir a favoritos`), which English marks
  on purpose.
- Open the favorite with that number → `Abrir el favorito de ese número` (like `la puerta del número 3`).
- This folder is already a favorite → `Esta carpeta ya está en favoritos`: `ya es un favorito` clashes with `carpeta`;
  `estar en favoritos` is the exact reverse of `Añadir a favoritos`. A calm statement, no `no se puede`.
- mounted share → `recurso compartido montado`, with no protocol jargon.
- `favoritesCantAddHere`:
  `Esta carpeta no puede estar en favoritos: los favoritos solo funcionan en discos y en recursos compartidos montados`,
  starting like its sister; `funcionar en` replaced `apuntar a`, which was longer. No full stop (a tooltip).
- `commands.favoritesAdd.description` names where the folder really goes:
  `… a favoritos, para volver ahí después desde el menú de favoritos.`; `commands.favoritesOpen.description` ends
  `…y pulsa un número para ir a ese favorito.`
- press a number → `pulsa un número` (`pulsar` for KEYS, `hacer clic` for the mouse). `Abre … y pulsa …` reads both as
  third person and as imperative, the double edge `style.md` prefers for descriptions.

## Quién retiene el disco cuando la expulsión se rechaza (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

Six keys extending § Expulsar y desconectar: Cmdr now NAMES what holds the disk. The family inherits `disco`, the close
`y vuelve a expulsarlo`, and `-lo`.

- is still using this drive → `sigue usando este disco`, literal from `errors.eject.unmountRefused`
  (`Algo sigue usando este disco`); plural `siguen usando` (`{apps}` is always two or more). Active voice, subject
  first.
- `{app}` opens the sentence with no article (a process name is a proper name: `Preview`, `Warp`, `mds_stores`).
- Close anything it has open there → `Cierra lo que tenga abierto ahí` (plural `tengan`): `lo que` + subjunctive avoids
  naming a gender; `ahí` avoids agreeing with the volume.
- other apps → `otras apps` (Finder `Ocultar otras apps`, `desde el Finder u otras apps`); the joining `y` comes from
  `Intl.ListFormat` (`Preview, Warp, Photos y otras apps`, checked with `new Intl.ListFormat('es')`, 2026-09-16).
- disk image → `imagen de disco` (Finder BN53); `disco` twice in one sentence is fine
  (`Una imagen de disco guardada en este disco`), as Finder does.
- macOS is still working with this drive → `macOS sigue trabajando con este disco` (the verb is tentative;
  `sigue usando` would blur it with the app keys, which ask the opposite).
- Wait a minute / Wait a moment → `Espera un minuto` / `Espera un momento`, kept apart as English does.
- Cmdr itself → `El propio Cmdr`, the mould of `errors.eject.busy`.
- if it keeps happening → `si sigue pasando` (nine `errors.listing.*` / `errors.provider.*` keys); the key copies
  `errors.listing.resourceBusy.suggestion`'s two-sentence form.
- send a report → `envía un informe` (`menu.help.sendErrorReport`, `errorReporter.amend.unavailable`).

## Seleccionar todo lo de la misma clase (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- kind → `Clase` (Finder ArrangeByMenu, the Kind sort criterion).
- "Select all with extension `*.{extension}`" → `Seleccionar todo con la extensión *.{extension}` (Double Commander,
  Total Commander name this command; the mask replaces their "same extension").
- `menu.context.selection` is a NOUN (the submenu's title) → `Selección`. ❌ Not the verb `Seleccionar`
  (`menu.bar.select`).
- The four label twins share one English each, so `i18n-terms` holds each pair identical; the only legitimate difference
  is the apostrophe (RAW `menu.*` single, ICU `commands.*` doubled).

## La insignia de acceso total al disco en la barra de título (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

The warning pill in the title bar while Cmdr lacks full disk access; clicking reopens onboarding at step 1.

- full disk access → **`acceso total al disco`**, Apple's own pane name from the live bundle
  (`SecurityPrivacyExtension.appex` `ALL_FILES`, macOS 27.0 26A428, 2026-09-16), superseding the composed
  `acceso a todo el disco`, which named no pane the user can find. `es-419` would say `Acceso completo al disco`.
- `onboarding.fdaBadge.label` → `Sin acceso total al disco`; `onboarding.stepAi.bannerTitle.denied` carries the SAME
  English, so the two are identical. ❌ Reword neither alone.
- `onboarding.fdaBadge.ariaLabel` opens with the label verbatim
  (`Sin acceso total al disco. Abrir el paso de acceso total al disco de la introducción.`), which satisfies
  `i18n-aria`.
- Tooltip: drive → `unidad`, cloud folders → `carpetas en la nube`, "files macOS keeps to itself" →
  `archivos que macOS se reserva` (plain, ❌ never a feature name), "Click to …" → `Haz clic para …`, onboarding →
  `introducción`.
- **Name vs prose**: `acceso total al disco` where a string NAMES the setting. The FDA step's running prose
  (`onboarding.stepFda.revoked.noAccess` and siblings) still says `acceso a todo el disco`; whether to sweep it is open
  (`review-queue.md`).

## El diálogo de rechazo de la papelera (`errors.write.trashRefused.*`)

macOS turned down a move to the trash, and Cmdr words the refusal three ways (permission-shaped, no trash there,
unclassified). RAW family. Four rules bind the group:

- **The title isn't a free choice**: `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English. ❌ Reword one,
  reword all five.
- **No plural machinery**: every `message.*` must read at `{count}` = 1 and 7 alike:
  `{count} de los elementos que elegiste` takes any numeral without agreement.
- **❌ Never "try again" in a suggestion**: retrying a permission refusal reproduces it (the bug report called that
  advice useless). Say what the user CAN do.
- **`suggestion.other` reuses the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Detalles técnicos`).

Terms: locked → `bloqueado`; "delete them permanently" → `eliminar permanentemente` (the command label); the title-bar
pill → `el aviso` (tentative), quoted with `onboarding.fdaBadge.label` verbatim in curly quotes; title bar →
`barra de título`; "somewhere macOS keeps to itself" → `un sitio que macOS se reserva`.

## El aviso de contenido solo en línea (`fileOperations.delete.cloudOnlineOnlyMixedWarning`, `fileOperations.delete.cloudOnlineOnlyAllWarning`, `fileOperations.delete.cloudOnlineOnlyHandedBack`)

If a selected item in a cloud folder is online-only, the trash would have to download it first, so Cmdr opens the
permanent-delete dialog and explains it in the banner (two variants: a mixed selection, and all online-only). The third
key is the line when Cmdr hands a press back.

- **The four facts are mandatory**: (1) the trash would download the files, (2) so Cmdr only offers to delete the WHOLE
  selection, (3) afterwards there's no copy in the trash, even if the service keeps its own (❌ don't soften it), (4)
  the ways out the banner names.
- Keep both `<strong>` zones (on `descargaría primero` and the verb `eliminar`). The quoted “Eliminar” is the button's
  label, identical to `fileOperations.delete.confirmDelete`, in curly quotes.
- `.cloudOnlineOnlyAllWarning` drops the deselect way out: with everything online-only nothing would stay selected.
- `.cloudOnlineOnlyHandedBack` is neutral, no apology.
- To check in the overflow pass: the banner is long, in a narrow strip above the file list. ⚠️ Draft, not yet reviewed
  by a human.

## Cuando el servidor dice que ese recurso compartido no existe (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

The one case in this family where retrying is useless: the server clearly says it has no share by that name. No button,
and nothing may sound temporary (no `ahora mismo`, no `vuelve a intentarlo`).

- "the server says it has no share by that name" →
  `porque el servidor dice que no tiene ningún recurso compartido con ese nombre` (`errors.mount.shareNotFound`;
  NetAuthAgent's es says `volumen`, the catalog keeps `recurso compartido`).
- "You are still connected" → `Sigues teniendo acceso`, after `Tienes acceso`.
- "This one won't sort itself out" → `Esto no se va a arreglar solo`: exactly what sets this notice apart.
- "may have been renamed or removed" → `puede que … se haya renombrado o eliminado`.
- "so it's worth checking there" → `así que vale la pena echar un vistazo allí`: `vale la pena`, not peninsular
  `merece la pena`.
- "a lot slower" → `mucho más lenta` (no figures here, unlike the `4 veces` sibling).
- The short notice ends `así que sigue usando la conexión del sistema`, like
  `fileExplorer.pane.directConnectionUnreachableToast`.

## Nombres que se ven iguales en el servidor (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

Two server items whose names look identical on screen but are stored with different characters (`é` as one character or
`e` + accent, or case).

- "look the same" → `se ven iguales`, from `errors.listing.ambiguousName.explanation`.
- "the server spells them differently" → `el servidor los escribe de forma distinta` in the short note; the long
  explanation says `los guarda escritos de forma distinta` (its English: "stores them spelled differently"). No jargon
  (`Unicode`, `normalización`).
- accented letter → `letra con tilde`, the everyday word on both sides of the Atlantic.
- **A button named in prose goes in curly quotes**: `“Sobrescribir” reemplaza el elemento que ya está ahí.` Unquoted,
  the sentence-initial infinitive reads as a verbal noun.
- "the one that's there" → `el elemento que ya está ahí`: a bare `el que` would agree with `nombres`; `elemento` covers
  file and folder.
- `{path}` in `errors.volume.ambiguousName` → `“{path}”`, the family's curly quotes.
- "rename one of them" → `renombra uno de ellos` (❌ not `cambia el nombre de`).

## El interruptor “Permitir IA en la nube” y los estados con la IA en la nube desactivada (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

A privacy switch: Cmdr sends nothing to a cloud AI service until it's on. Calm, promising no more than Cmdr does.

- Allow cloud AI (the switch) → `Permitir IA en la nube`: `IA en la nube` is `settings.ai.provider.opt.cloud`; the mould
  `Permitir <objeto>` is `settings.fileOperations.allowFileExtensionChanges.label`. Quoted in curly quotes in
  `lockedHint`; where English uses it as a verb (`askCmdr.gate.cloudOff.body`, `settings.askCmdr.cloudOffHint`) it's
  conjugated: `Permite … la IA en la nube`.
- cloud AI in a sentence → `la IA en la nube`, pronoun `la` (`Permítela en Ajustes > IA`).
- "X is off" → `X está desactivado/a` (like `servers.hub.discoveryOff`).
- Turn on Ask Cmdr → `Activar Ask Cmdr` (infinitive button); Open AI settings → `Abrir los ajustes de IA`.
- side panel → `panel lateral` (tentative); custom endpoints → `puntos de conexión personalizados`.
- Search in plain words → `Búsqueda con tus propias palabras` (describes the feature; tentative).
- `settings.askCmdr.enabled.label` is now just “Ask Cmdr”, with `sameAsSourceJustification`.

### Esc y pantalla completa (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- full screen → `pantalla completa`, no article after `salir de` (macOS `Salir de pantalla completa`,
  `Usar pantalla completa`); being in that mode → `estar a pantalla completa`.
- Escape → `Esc` / `la tecla Esc` (AppKit `la tecla Esc para cerrarlo`): `la tecla Esc` when it opens the sentence as
  subject, bare `Esc` in labels. The uppercase `ESC` of `shortcuts.section.pressEscToClear` is the one exception.
- Exit full screen on Escape → `Salir de pantalla completa con Esc`, the same in the notice's switch and in Settings.
- hint → `aviso` (`settings.advanced.oldMacosNoticeShown.*`: `Aviso de … mostrado`).
- “You'll only see this once.” → `Solo verás este aviso una vez.`: names the notice, no gendered adjective.

### Líneas provisionales de “Abrir con” y “Compartir” (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- “Finding apps…” → `Buscando apps…` (macOS `Buscando…`).
- “share options” → `opciones para compartir`, reusing the submenu's verb; `recurso compartido` is the network sense.
- “No share options” → `No hay opciones para compartir`, calmer and more natural than macOS's
  `Ningún servicio aplicable`.

## La migración al termbase: arreglos de coherencia (`goToPath.dialog.removeFromList`, `askCmdr.wakeDigest.removed`, `errors.write.fallback.message.*`, `onboarding.stepFda.con.body`, `indexing.staleDialog.body`, `indexing.rescan.*`)

Found while moving this glossary into `terms.json` and driving the termbase check to zero. Each one follows a ruling
that already existed.

- **Something went wrong → `Algo salió mal`, everywhere.** Seven keys said it; `ai.cloud.genericError` and
  `onboarding.cloudSetup.status.genericError` said `Algo ha ido mal` after a drift pass that misread the regional note.
  The pan-regional preterite rule of `style.md` decides. `errors.write.fallback.message.*` said
  `Ocurrió un error inesperado`, which broke the no-`error` voice rule: now `Algo salió mal al copiar.` and siblings.
- **`Quitar` takes an entry off a list**: `goToPath.dialog.removeFromList` said `Eliminar de la lista`.
- **`askCmdr.wakeDigest.removed` counts items gone any way** (trashed, deleted, moved out): `elementos que ya no están`,
  because `eliminados` claimed a delete.
- **network drive → `unidad de red`** in the seven `errors.listing.*` keys that said `disco de red` (feminine agreement
  fixed along with it).
- **rename → `renombrar` in prose** in the last four keys with `cambiar el nombre` (`errors.mutation.sipProtected`,
  `renameOutOfArchive`, `renameAcrossArchives`, `errors.write.duplicateSourceNames.suggestion.*`).
- **item → `elemento`**: five keys still said `ítem`.
- **The Finder takes its article**: about twenty keys said `en Finder` / `de Finder`.
- **Preview is `Vista Previa` on a Spanish Mac** (`Preview.app` `CFBundleDisplayName`, macOS 26): the safe-save
  description named it in English.
- **source-available isn't open source**: `onboarding.stepFda.con.body` said `es de código abierto`; Cmdr is BSL. Now
  `su código fuente es público`.
- **Nothing agrees with `{name}`**: `indexing.staleDialog.body` said `{name} estuvo desconectada` and `analizarla`; now
  `estuvo sin conectar` and `la unidad`.
- **The rescan lines are subjectless gerunds** (`Haciendo un análisis nuevo …`), like their sibling
  `Reiniciando el análisis desde cero`; they had mixed `Estamos haciendo` and `Se está haciendo`.
- **watcher → `vigilancia`** (the Settings card's word): the rescan lines said `el monitor`, the debounce row
  `el vigilante`.
- **The view-mode names are lowercase in prose**: `la vista breve` / `la vista completa`, as the View menu writes them;
  `modo Completa` also broke agreement.
- **Voice**: `askCmdr.decision.result` said `con errores` (now `sin completar`); the reconnect lines said `Si falla` and
  a compound perfect (now `Si tampoco funciona`, `Se reintentó`); `errors.listing.networkUnreachable.*` gendered the
  reader (`no estás conectado`, now `no tienes conexión`).
