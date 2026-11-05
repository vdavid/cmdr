# es decisions

Distilled rulings behind `terms.json` and what defends them against a well-meant "fix". Headings cite their keys in
backticks (the brief pulls by key). Voice and grammar: `style.md`; open questions: `review-queue.md`.

## Rulings that have no concept of their own

Small, local, binding (tentative unless sourced).

- **Settings sections**: Apariencia (better title than macOS's Aspecto), Colores y formatos, Zoom y densidad, Tamaños de
  archivos y carpetas, Lista, Comportamiento, Vigilancia del sistema de archivos, Desarrollador, Servidor MCP,
  Actualizaciones y privacidad, Avanzado, Navegación y operaciones de archivos.
- **Unsourced option labels**: Inteligente / Dinámico / Contenido / En disco / Arcoíris / Marchitamiento; Western
  (encoding) → Occidental; Detectada/Detectado agrees with its noun.
- **Tight abbreviations**: relative time `hace {count} min/h/d/sem/mes/a`; time left `s` / `min`.
- **Shortcuts**: en la app; combinación (de teclas, macOS); registrar a hotkey; Atajos de teclado.
- **Error reporter**: Manifiesto; ID de referencia; ID del informe; paquete (log bundle); redact → depurar.
- **Licensing**: clave de licencia; perpetua; válida hasta el / caducó el; suscripción; renovar; organización.
- **Viewer**: transmisión; word-wrap badge `ajuste`; tail → Seguir; reindexar / Reindexando….
- **Onboarding**: pros y contras, A favor: / En contra:; incidencias (GitHub issues, MS); seguir / hacer un fork;
  reservar una llamada; ¿Qué acaba de pasar?; Me gusta / ¿No te gusta?; No volver a hacer esto.
- **Search**: ámbito; patrón; comodín; Glob / Regex; personalizado; carpetas aburridas (playful on purpose); Pregunta lo
  que sea.
- **Misc**: vacío (AppKit); por ADB / por USB in a label, `a través de un cable USB` for the cable; en los sitios
  habituales; marcador de posición (MS); implementación (Azure deployment, MS); OK → Aceptar; umbral; ajuste de línea;
  píxeles; identificador (file handle); enlace simbólico (roto); enlace físico; siendo usado por; a toggle in a
  description is the action (activar / desactivar), never a noun.
- **Verbatim**: git, worktree, repo (los repos), blob, commit, clone, byte(s), FAT32, exFAT, daemon, udev, ptpcamerad,
  Terminal, Ctrl+C, PTP.

## `worktree` (`errors.git.*`, `fileExplorer.git.size.linkedWorktrees`)

git's linked checkout stays `worktree` (masculine, `dos worktrees vinculados`); generic "working tree" in
`errors.git.bareRepo` / `blobTooLarge` / `gitDirPermissionDenied` is prose: `árbol de trabajo`.

## Transfer toasts (`transfer.*`)

`Copia completada` / `Movimiento completado`; counted toasts are whole-clause plurals (`Se ha movido 1 archivo`),
compress included.

## `Carpeta superior` (`commands.navParent.label`, `menu.go.parentFolder`, `settings.behavior.doubleClickPane*`, `fileExplorer.doubleClickHint.*`)

Finder's `carpeta contenedora` is "Enclosing Folder"; Double Commander uses `carpeta superior` for this feature, and
`subir a la carpeta superior` carries the direction. Menu = palette. Pane background → `fondo del panel` (≠
`espacio vacío`).

## Preajustes (`settings.appearance.*`)

preset → `preajuste` (Double Commander). Bare Back → `Atrás`; "Back to X" → `Volver a los preajustes`.

## FAT32 (`errors.write.*`, `errors.listing.notSupportedErrno.suggestion`)

"formatted as X" → `tiene formato X` / `con formato X` (macOS's noun, no agreeing `formateada`); "{name} is {size}" →
`{name} ocupa {size}`.

## Copiar y eliminar (`fileOperations.transferDialog.operationAria`, `fileOperations.transferDialog.targetWillBeCreated*`, `fileOperations.transferProgress.stageScanning`)

`Cmdr la creará durante la copia` / `… durante el movimiento` (`la` = carpeta; the op nouns). Scanning… → `Analizando…`.

## Archivos comprimidos (`settings.archives.*`, `fileOperations.delete.archiveWarning*`, `queue.row.label`)

- `el zip` where zip alone disambiguates. encrypted → `cifrado` (RAE, current macOS; ❌ the pile's `Encriptado`).
- extract → `extraer` (tar isn't compressed, ❌ `descomprimir`); preview verb → `previsualizar`, noun `vista previa`.
- for good → `para siempre`, warmer than `permanentemente`; the two warning halves concatenate.

## Pegar como archivo (`fileExplorer.clipboard.pastedAsFile`, `settings.fileOperations.pasteClipboardAsFile.*`)

The ICU select supplies article + noun (`la imagen` / `el PDF` / `el texto`); impersonal `Se ha pegado`.

## Comprimir y su contraseña (`fileOperations.archivePassword.*`, `commands.fileCompress.label`, `settings.archives.compressionLevel.*`)

The Compress name field says plain `archivo` (the zip is a file). Slider: `Más rápido` / `Más pequeño`.

## El registro de operaciones (`operationLog.*`, `commands.logOperationLog.*`)

Badges match `queue.row.status`. "Didn't finish" → `No se completó` (❌ `Falló`). Initiators `Tú` / `Cliente de IA` /
`Agente`: no gendered noun.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

`chat` / `chats` (RAE loan; `Chats` and `Ask Cmdr` carry justifications). usage → `uso` (❌ MS `utilización`); Spending
→ `Gasto`. token → `token` (MS's hits are security tokens). A bare `Preparó…` reads as a third-person subject.

## Las líneas de herramienta (`askCmdr.tool.*`, `askCmdr.toolGroup.*`, `askCmdr.turnStats.*`)

Done and tried lines are all `Se ha …` / `Se ha intentado …` (`memoryWrite.done` and `memoryEdit.done` realigned to
match). Outcomes dodge agreement with an unknown count or noun: `sin resultados`, `pendientes de revisar`,
`la nota sigue igual`, `El modelo ha visto {givenText} de {totalText}`. tool call → `consulta`; a stopped answer →
`Detenida`.

## Fotos en unidades de red (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`/`.paused`)

Status and help lines say `foto`; feature and label names keep `imagen` (English splits the same way). photo archive →
`colección de fotos`, dodging both `archivo` senses.

## Revisar los cambios de nombre (`askCmdr.renameReview.*`, `askCmdr.tool.imageFacts.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.proposeRenamePlan.*`)

`Nombre actual` / `Nombre nuevo` stay parallel (as `suggestedOps.columnNewName`, `editName`). `Permitir todos` /
`Denegar todos` agree with `los cambios de nombre`.

## La indexación de imágenes (`fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`, `settings.mediaIndex.chosenFolders.*`, `settings.mediaIndex.showFileStatusIcons.*`, `settings.mediaIndex.progressSummary.title`)

- `pasada` (an indexing pass) ≠ `análisis` (the full drive scan). `Carpetas para indexar` / `… siempre`.
- "still searchable" → `se puede seguir buscando` (❌ `buscable`). "Indexing now" → `Indexando ahora` in both keys.
- `indexada(s)`, `está` / `están` go inside the plural arms; `one` drops "All": `{totalText} imagen … indexada`.

## La búsqueda por descripción (`settings.mediaIndex.cards.*`, `settings.mediaIndex.semanticSearch.label`, `settings.mediaIndex.clip.*`)

`Apple silicon` verbatim (Apple es). reclaim → `liberar`; `Eliminar modelo (liberar {size})` parallels
`Descargar modelo (…)`.

## El interruptor de papelera y Desde / Hacia (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.*GroupTitle`)

`Mover a la papelera`: one move verb. From / To → `Desde` / `Hacia` (Double Commander); a bare `A` reads as a stray
letter.

## La indexación de unidades apagada (`fileExplorer.navigation.driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

- Path quotes the sidebar: `Indexación > Indexación de unidades`; ❌ `indización`. A path then a `y` clause takes a
  comma.
- "stays unindexed" → `sigue sin indexar`; "picks up where it left off" → `continuará donde lo dejó` (❌ `seguirá`,
  "stay put").
- `overriddenBadge` → `Desactivado con la indexación` (masculine like the off labels; the page implies `de unidades`).

## Índice de unidades: comprobación de cambios (`indexing.step.*`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

"Checking for changes" → `Comprobación de cambios`, a noun phrase beside `Primer análisis completo`.

## Transferencia atascada (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

`Esperando a que X responda` (Finder), ❌ the calque `esperando por`. `stallInFlight` bakes the whole sentence into the
arms (`esté` / `estén`, `escrito` / `escritos`).

## Ruta copiada (`fileExplorer.clipboard.copiedPath`)

The path renders below, so: `Ruta copiada, ya está en el portapapeles:` (no possessive).

## Cola de operaciones (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`, `fileOperations.transferProgress.backgroundedToast`)

It lists deletes and renames too, and pairs with `Registro de operaciones`. `queueShow.label` is the bare title (❌
`Mostrar la cola…`). Feminine head noun: swapping it means redoing every `esta`. `transferencia` still names a copy or
move.

## El chip y el aviso de operación sin terminar (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- `Descartar todo` parallels `Pausar todo`; `Quitar` edits a list the user built.
- "Couldn't finish <action>" → `No se ha podido completar` + op noun (la copia, el movimiento, el movimiento a la
  papelera, la eliminación, el cambio de nombre, la creación de la carpeta / del archivo, la edición del archivo
  comprimido), the row status verbatim; ❌ `terminar de copiar`.
- "to {destination}" → ` a {destination}` (Nautilus; Finder's `en` reads locative after `Moviendo`), also with no object
  (`Copiando a Ana` may read as personal `a`: accepted). ❌ `a la carpeta {destination}`: it can be a root or share.
  `Trabajando en {destination}` is locative. Optional clauses keep their leading space inside the branch.

## El botón con la cola vacía (`fileOperations.transferProgress.background*`, `fileOperations.operationConflict.context`/`.pausedNote`)

`En segundo plano`: ❌ bare `Segundo plano` (the colour pair in Double Commander and MS), ❌ `Pasar a segundo plano`
(too long beside `Cola`); both states name a destination. Rewrite `backgroundAria` with it.

## La salida con operaciones en curso (`main.quit.*`)

`en curso` in prose, `En ejecución` the badge. "Keep working" → `Seguir trabajando` (❌ `Cancelar`: reads as cancelling
the operations). `se interrumpe donde esté` (❌ `se detiene`). `al reiniciar el Mac`, so it can't mean restarting Cmdr.

## Usage stats: un identificador aleatorio (`settings.analytics.enabled.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analytics*`)

The stats carry a stable random id: ❌ `anónimas`, ❌ jargon `seudónimo`. Say `un identificador aleatorio`; tied to →
`vincularse a`.

## Filas a la espera de respuesta y la confirmación de reversión (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

"Needs your answer" → `Respuesta necesaria`. "carries on" → `continuará` (❌ `se reanudará`, the Reanudar button). "Keep
them" → `Conservar los archivos` (a clitic would point at the replaced files). `foregroundBusyToast` says
`esta operación`.

## Renombrados que no cambiaron (`fileExplorer.rename.chainKeptOriginalName*`, `fileOperations.validation.nameNotUsable`)

`mantener`, ❌ `conservar` (a choice). `Se ha mantenido el nombre “{name}”` (❌ `su nombre`). `nameNotUsable` →
`Ese nombre de archivo no puede usarse`, no full stop: it composes into `chainKeptOriginalName`.

## Operaciones sugeridas (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

approve → `Aprobar` (❌ `Aceptar`). Title → `Sugerencias de IA`. `Why:` → `Por qué:`, spoken `Por qué, según la IA:`.
`hoy, {time}` (❌ `a las`: 1:00 is `a la`).

## Duplicar (`commands.fileDuplicate.*`)

`Duplicar` (Finder); the description keeps `archivos` (❌ Finder's `ítems`).

## Menús nativos (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

RAW family. Finder (`MenuBar.strings`, `LocalizableMerged.strings`) decides, Safari for tabs, MS for the rest.

- `Ocultar otras apps` in both keys.
- zoom → `Aumentar el zoom` / `Reducir el zoom` in menu AND palette (Safari's `Ampliar` needs a Zoom submenu).
- changelog → `Registro de cambios`, apart from Help > `Novedades`.
- Tag row → `Añadir “{color}”` / `Quitar “{color}”`: Finder's `Eliminar` would delete nothing.
- `menu.network.open` / `.edit` equal their palette twins byte for byte.

## El aviso de la conexión que macOS presta (`fileExplorer.network.osMountFallback.*`)

`la conexión del sistema` when macOS isn't named. "for most connections" moves to the end, so `más lenta que…` sits
beside its term. require → `requerir` (macOS `Requerir contraseña`): `Requerir siempre cifrado SMB`, quoted in prose
after `Cmdr tiene activado`; `El proveedor {provider}` keeps `ocupado` off the brand.

## Avisos de una línea de renombrar / crear (`errors.mutation.*`, `errors.volume.*`)

RAW family. Quote the long sibling (`errors.listing.*` / `errors.write.*`) word for word.

- `{path}` in curly quotes. root (of a volume) → `carpeta raíz`, ❌ `carpeta superior`.
- `sipProtected` → `Este elemento está bajo la protección de la integridad del sistema de macOS, así que …`.
- `timedOut` isn't a failure; `deviceSessionReset` isn't a disconnect (`El dispositivo reinició su conexión.`).
- "Move it instead" → `Usa Mover.` (the F6 command). "stopped at your request" →
  `Cmdr detuvo esto porque se lo pediste.` (❌ formal `a petición tuya`).
- `trashRefused` → `macOS no permitió mover esto a la papelera.` (❌ `rechazó mover`, `no quiso`); `trashNotSupported` →
  `solo se puede eliminar permanentemente`.

## El diálogo de fallos (`crashReporter.dialog.body.*`, `settings.updates.crashReports.description`)

Only `.ended` may say Cmdr closed (`se cerró inesperadamente`). "ran into a problem" → `tuvo un problema`. "kept
running" → `siguió funcionando`. The settings text reuses these pieces in the present (`un informe`, no `de fallos`);
its LABEL stays `Enviar informes de fallos`.

## Expulsar y desconectar (`errors.eject.*`)

RAW family; each follows `No se ha podido expulsar {volumeName}: …`. The ejected drive is `disco` (Finder's eject
corpus).

- "Unplug it" → `Desconecta el cable` (`Desconéctalo` echoes the command that just didn't work).
- `timedOut` / `volumeNotFound` / `unexpected` copy `errors.mutation.*`.
- `Cierra los archivos y las apps que tengas abiertos`: the `tener` relative avoids a stranded participle.

## El aviso de la papelera (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

put back → `devolver … a su sitio`: Finder's `Sacar de la papelera` fits only a menu; `restaurar` is for names.

## Añadir a un informe ya enviado (`errorReporter.amend*`, `errorReporter.autoSentToast.viewOrAddNotes`)

`Ya no se pueden añadir notas a ese informe` (no culprit). "What was sent" → `Lo que se ha enviado`, past of
`errorReporter.dialog.detailsToggle`. `Nota añadida a tu informe.` agrees with `nota`.

## Seleccionar / deseleccionar (`selection.*`)

deselect → `Deseleccionar` (Total / Double Commander; macOS has no verb, MS's `anular la selección` is too long). Menu,
palette, settings, and title name the dialog identically; popovers copy `queryUi.recent.*` with `selecciones`.

## El nombre accesible contiene la etiqueta (`queryUi.scope.toggle.caseSensitiveAria`, `viewer.search.caseSensitive`, `queryUi.recent.caseSensitive`)

`Distinguir mayúsculas y minúsculas`, the full macOS form; the aria wraps it (`… al buscar`).

## Una palabra inglesa, una española (`commands.helpSendErrorReport.label`, `menu.help.sendErrorReport`, `menu.zoom.in`/`.out`, `commands.handler.zoomResetHintMenu`, `menu.context.toggleSelection`, `fileExplorer.columns.modified`)

Help menu and palette say `informe de error`. `zoomResetHintMenu` points at `Visualización`. Toggle →
`Activar o desactivar …`. `Modified` → `Modificación` in all six keys. Short commands drop the article
(`Copiar nombre de archivo`).

### Fronteras deliberadas (no unificar)

Held in the term-consistency allowlist: `Ambos` (files and folders) / `Ambas` (notifications); `Operación cancelada`
(pane title) / `Cancelado` (status); `Edición` / `Visualización` menus vs `Editar` / `Ver` actions; `Problema` for the
error status; `Modificación` (date) / `Modificados` (shortcuts); `Buscar` / `Búsqueda` (settings topic); `restaurar`
names / `devolver a su sitio` places. `Connected` / `Connected!` differ in English; `i18n-terms` groups them because `¡`
/ `¿` survive its normalizer.

## Palabras que se separaron (`menu.go.parentFolder`, `licensing.dialog.typeCommercial*`, `errors.mutation.cantRenameVolumeRoot`, `askCmdr.renameUndo.applied`, `indexing.scan.counters`, `errors.write.trashNotSupported.suggestion`, `fileExplorer.renameConflict.overwriteTrash`, `settings.mediaIndex.clip.comingSoon`)

`por omisión` with no exceptions (macOS es: zero `predeterminado`). `cambiar el nombre` belongs to the noun only.
`permanentemente` (the F-key bar keeps `Permanente`); `Próximamente`.

## Paneles del Mac de quien usa Cmdr (`errors.git.*`, `errors.provider.*`)

`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` come from the user's Mac: a preposition may precede
them, an article or contraction may not. The iCloud keys open `Abre {system_settings}, …` to avoid `en … en …`.

## `Restaurar` el nombre anterior (`askCmdr.renameUndo.undone`/`.partial`)

The verb agrees with the names: `Se han restaurado los nombres anteriores de N archivos.`, `one`
`Se ha restaurado el nombre anterior de 1 archivo.`

## Terminar la reversión (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

`Terminar de revertir` (never "start over"; shorter than `Completar la reversión`). `en “{folder}”` quoted, so the
folder doesn't read as what gets removed.

## El aviso de una reversión que no lo pudo todo (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

Cmdr did the careful thing: no apology, no alarm.

- Reasons copy `askCmdr.renameUndo.skipReason.*`; the two `folderNotEmpty` keys match their twins word for word.
- `todos los {countText} elementos` is what separates `done*` from `some*`; lose it and the partial notice promises a
  clean destination.
- Rolling back a copy `elimina`, a move `devuelve … a su sitio`. "Cmdr had written" → `que Cmdr había creado`.
- `*.counted`: each branch is a whole sentence, verb and tail agreeing (`se ha quedado` / `se han quedado`).
- `stagedLeftover.*` retries `la próxima vez que se conecte a ese sitio`, never a time; `es-419` forks only the tense
  and `lugar`.

## Avisos de macOS y WebKit antiguos (`main.oldWebkit.*`, `main.oldMacos.*`)

`Actualización de software`; "or newer" → `o posterior`. supported → `compatible` (❌ `soportado`); best effort →
`hace lo que puede`; look off → `verse raros`.

## Mirar dentro de un archivo (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

`Mirando dentro de los archivos` / `Se ha mirado dentro de …` (`contenido` promises more). camera details →
`los datos de la cámara`. `la lista de archivos que contiene un archivo comprimido` breaks up the `archivo` echo. The
guarantee: `nunca cambia un archivo sin tu aprobación` (Ask Cmdr isn't read-only).

## Los textos del botón Revertir (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

`Detener y devolver a su sitio todos los archivos movidos hasta ahora`: undoing a move deletes nothing.

## “Abrir terminal aquí” (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

`terminal` and Apple's `Terminal` are one loan; built on Apple's `Abrir en Terminal`, identical in menu and palette.
Choose an app… → `Seleccionar app…`.

## Columnas y filas sueltas (`fileExplorer.columns.sortByRelevance`, `settings.archives.ooxml.*`, `settings.appearance.showFunctionKeyBar.label`, `commands.queueMoveEarlier/Later.label`, `queue.reorder.*`)

- `Ordenar por relevancia`, framed like its siblings.
- The OOXML row covers Office documents AND app packages: bare `paquetes`, broader than the `Paquetes de apps` card.
- `barra de teclas de función`, reused by the context-menu item and its notice.
- `Adelantar` / `Retrasar en la cola` (❌ `Subir` / `Bajar`): it's about starting sooner.

## Servidores (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`disconnect*`/`forget*`/`server*Toast`/`pinRefusedToast`/`networkVolume`, `commands.servers*`, `shortcuts.scope.servers`/`places`, `goToPath.dialog.opensServer`/`addsServer`, `menu.network.pinToSwitcher`/`unpin`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.servers.*`, `settings.adb.*`, `settings.appearance.tintSmb.*`)

- **Hub**: `Servidores` names the row (`la lista Servidores`, unquoted), `Red` only the group. Places → `Ubicaciones`
  (Finder; it covers buckets, ❌ `Recursos compartidos`). `Último uso` (❌ `Última conexión`: English chose "used").
  discovery → `detección` (❌ `descubrimiento`, a finding).
- **Pin**: `fijar` / `desfijar` (❌ Safari's `anclar`; move all five together); `Desfijar del selector` twins
  `Fijar en el selector`, quoted bare in `pinHint.body`. Trusted → `de confianza` (❌ `fiable`, a certificate verdict).
- **Keys**: SSH key → `clave` (❌ `llave`: passkeys, `llavero`); fingerprint → `huella digital` (❌ bare `huella`, Touch
  ID); compromised → `comprometida` (❌ `filtrada`, a concrete leak). trust → `confiar en` with macOS or Cmdr as
  subject; `Trust and connect` names what (`la clave`, `el certificado`). Owner → `quien posee el servidor`; "something
  between you and it" → `hay algo entre tú y él`, no MITM jargon.
- **Forget**: confirmations name the type first (`¿Olvidar el recurso compartido {name}?`) so `lo` / `montado` agree;
  the toasts say `olvidar` too (`Cmdr ha olvidado {hostName}`). `No había ninguna contraseña guardada de {name}.`
  (imperfect, matter-of-fact). Busy tooltips copy `ejectBusyTooltip`.
- **Account**: `como {username}` / `como invitado`; `Entrar como invitado`; `Iniciar sesión como…`.
  `Sin conexión con {name}`: no participle on `{name}`. `needsStoredSecret` is impersonal (`e inicia sesión`).
- **Sheet**: titles infinitive (`Añadir servidor` / `Iniciar sesión en {name}` / `Editar {name}`). `i18n-terms` forces
  many values: search the catalog first. Locked identity → `son los que identifican este servidor` (the sheet has its
  own `Nombre` field), naming actions as their buttons (`olvidar`, `añadir`).
- **Folders**: start folder → `Carpeta inicial` (❌ `carpeta de inicio`, home). `Dónde se abre el servidor.`: naming
  `la carpeta` makes `Déjalo vacío` read "empty the folder". `usará la cuenta y el host como nombre` (❌
  `por su cuenta`).
- **Pane**: the long signed-out line is impersonal (`En este servidor se inicia sesión con una clave…`). Retry durations
  are bare pieces of `retryKeepsTrying` (the integer selects, `…Text` is read). `Cmdr no se conectará a {name}` (❌
  `dejó de conectarse`, an interrupted attempt).
- **ADB settings**: `Cmdr detecta los teléfonos en cuanto los conectas` (❌ `Buscando teléfonos`, a search in progress).
- **S3** (`servers.sheet.s3*`, `*coldStorage*`, `*ShareLink*`): `el bucket` (AWS es; ❌ MS `cubo`);
  `ID de clave de acceso`, `clave de acceso secreta`; cold storage → `archivado`, `restaurar`, `el archivo {path}`;
  share link → `enlace para compartir`.
- **NFS** (`servers.sheet.nfs*`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`): export →
  `exportación`, feminine (QNAP es), ❌ `recurso compartido` (the SMB share). The port switch is quoted as Synology es
  writes it: `Permitir conexiones desde puertos no privilegiados`. uid / gid → `ID de usuario` / `ID de grupo`, plural
  `los ID`. Nothing points back at `{host}` (`En la lista de {host}`, `la lista de exportaciones`).

## El teléfono Android por ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

AOSP's Spanish decides what the user reads on the phone: `Depuración por USB` (❌ `depuración USB`), `tocar`, button
`Permitir`. wake → `activar` (❌ `despertar`). `Abrir Ajustes` the window, `Abrir los ajustes` the concept: move the
three together. `You stopped opening your phone.` → `Has detenido la apertura de tu teléfono.` (❌ `Cancelaste`, the
button). None of the 19 strings says `error`, `fallo`, or `no se pudo`.

## «¿Dejamos Cmdr en el Dock?» (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

`fijar` (❌ the Dock menu's `Mantener en el Dock`). Result keys state facts:
`El icono de Cmdr ya está en su sitio, pero el Dock todavía no lo muestra`; `managedDock` → `Quien administre este Mac`.

## El menú del icono en el Dock (`menu.dock.*`)

`DockMenus.strings`: verb + app name, no article (`Abrir Cmdr`). `Ir a la carpeta…` is Finder's. `{name} ({parent})` is
identical to English.

## «Mostrar en el Finder» (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

“Mostrar en el Finder” quoted, the card's exact name. "for a while now" → `un tiempo`. The first-time notice isn't an
apology.

## La introducción (`onboarding.*`)

- star → `dar una estrella`, button `Estrella` (GitHub es); like (AlternativeTo) → `dar un me gusta`.
- checklist → `lista de tareas`; mailing list → `lista de correo`; sign up → `dar de alta`.
- typo → `errata` (dodges `error`); handler (the process MTP displaces) → `proceso`, keeping `controlador` for a driver.
- machine / this computer → `ordenador` (macOS es 43 against one `equipo`). `Local Network` → `“Red local”`, Apple's
  label.
- `superprivada` agrees with `la IA`; dumber → `más torpe`.

## El visor mientras llega el archivo (`viewer.pull.*`, `viewer.error.stoppedResponding`)

Fetching → `Obteniendo` (❌ `Descargando`: a local archive isn't downloaded).
`Obteniendo el archivo {fileName} para previsualizarlo`, so `lo` agrees with `el archivo`. so far → `hasta ahora`.

## El índice desactualizado de un teléfono (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`bodyPhone`)

The phone is still plugged in: no disconnect. `{name} no avisa a Cmdr`; `los cambios que hace el propio Cmdr` (❌
`su índice`).

## Por qué no se monta un recurso compartido (`errors.mount.*`, `errors.shareList.*`)

NetAuthAgent words these (its `volumen` for one share isn't taken). this computer → `este ordenador`, ❌ `Mac` (these
also run on Linux). `cuando quieras` (❌ `listo/lista`). Same English, same Spanish across both families.

## F4 y su editor de texto (`settings.behavior.textEditorApp.*`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`/`launchRefused`)

text editor is the category, ❌ TextEdit (that arrives in `{app}`). `Editar archivos con` takes a bare app name.

## Una unidad que se va (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.*`)

- `se está desconectando` / `se desconectó`: reflexives don't agree with `{name}`.
- Mid-transfer, the sentence exists to say where the files are. It's `unidad` (❌ the eject family's `disco`);
  `así que no has perdido nada` (❌ `no se pierde nada`, which hides whose files). to it → `ahí`;
  `después de que Cmdr copiara …` keeps the brand; `{counterpart}` follows a bare preposition.

## Un movimiento que no se pudo confirmar (`errors.write.moveNotConfirmed.*`, `fileOperations.leftovers.stagingFolderKept`)

- Neither failure nor loss: Cmdr couldn't prove the copies landed. kept → `ha conservado` (❌ `ha dejado`,
  carelessness). `Tus originales siguen donde estaban.`
- The staging folder: ❌ never imply its files can be deleted (maybe the only copy). unfinished move →
  `un movimiento sin terminar` (❌ `parcial`, `interrumpido`). `una carpeta oculta` is mandatory;
  `llamada {folderName}`.

## Favoritos (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `fileExplorer.navigation.favoriteUnreachable.removeFavorite`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

`favoritos` (❌ `marcadores`). `la carpeta actual` keeps the article; `Esta carpeta ya está en favoritos` and
`Quitar de favoritos` mirror `Añadir a favoritos`. press a number → `pulsa un número`.

## Quién retiene el disco (`errors.eject.unmountRefusedBy*`, `errors.eject.other*`)

- These follow `No se ha podido expulsar {volumeName}:` or `No se ha podido desconectar:` and may be a share or a phone:
  one skeleton, `X todavía está usando algo ahí.` + `… y vuelve a intentarlo.`, ❌ `este disco`, `vuelve a expulsarlo`.
- `{app}` opens with no article (a proper name); `Cierra la app` / `esas apps`: no pronoun for an insert. Eject all →
  `Expulsar todo` (Finder `N199.1`). `otras apps` / `otros` join through `Intl.ListFormat('es')`. Cmdr stays subject
  (`Cmdr no tiene el nombre de la app`), ❌ `No tenemos`.

## Seleccionar lo de la misma clase (`menu.select.*`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

kind → `Clase` (Finder). `menu.context.selection` is the noun `Selección`. Twins match bar the apostrophe (RAW single,
ICU doubled).

## La insignia de acceso total al disco (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

Every NAME uses Apple's pane name (the badge, `bannerTitle.*`, the path in `bannerBody.stuck`). `fdaBadge.label` and
`bannerTitle.denied` share one English: reword both or neither.

## El rechazo de la papelera (`errors.write.trashRefused.*`)

The title is shared with four `errors.write.*.title.trash` keys: reword all five.
`{count} de los elementos que elegiste` reads at 1 and 7 alike. `suggestion.other` reuses `Detalles técnicos`; the pill
is quoted as `onboarding.fdaBadge.label`.

## El aviso de contenido solo en línea (`fileOperations.delete.cloudOnlineOnly*`)

Four mandatory facts: the trash would download the files; so Cmdr offers only deleting the whole selection; no copy
stays in the trash (❌ don't soften); the ways out. Keep both `<strong>` zones; `AllWarning` drops the deselect way out.

## El recurso compartido que no existe (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Retrying is useless, so nothing sounds temporary (❌ `ahora mismo`, `vuelve a intentarlo`):
`Esto no se va a arreglar solo`; `Sigues teniendo acceso`; `merece la pena echar un vistazo allí`.

## Nombres que se ven iguales (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

No jargon (`Unicode`, `normalización`): `se ven iguales`, `el servidor los escribe de forma distinta`,
`letra con tilde`. Quote the button (`“Sobrescribir” reemplaza …`). `el elemento que ya está ahí` (bare `el que` agrees
with `nombres`).

## “Permitir IA en la nube” (`ai.cloudConsent.label`/`description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

The `Permitir <objeto>` mould; conjugated where English uses a verb (`Permite … la IA en la nube`).

## Esc y pantalla completa (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`, `menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`Salir de pantalla completa` (macOS, no article); `estar a pantalla completa`. `la tecla Esc` as subject, bare `Esc` in
labels. `Solo verás este aviso una vez.` Loading lines: `Buscando apps…`; share options → `opciones para compartir`.

## Coherencia del termbase (`goToPath.dialog.removeFromList`, `askCmdr.wakeDigest.removed`, `errors.write.fallback.message.*`, `onboarding.stepFda.con.body`, `indexing.staleDialog.body`, `indexing.rescan.*`)

- `errors.write.fallback.message.*` → `Algo ha ido mal al copiar.`
- `askCmdr.wakeDigest.removed` → `elementos que ya no están` (trashed, deleted, or moved out).
- Cmdr is BSL: `su código fuente es público`, ❌ `de código abierto`.
- `indexing.staleDialog.body` → `estuvo sin conectar`, `la unidad`.

## El original se quedó (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

`en las dos ubicaciones`; `elimina tú el original` (❌ `tú mismo`).

## Lo que gestiona tu organización (MDM) (`*.managed.*`, `multiRename.*Help.*`)

Nothing agrees with the reader (`Lo que gestiona…`); `*Off` agrees with its row (`Desactivadas`), as System Settings
does. Multi-rename help: `On`/`Off` → `Activado`/`Desactivado`; caret → `cursor de texto` (bare `cursor` is the pane's).

## El editor de rangos (`multiRename.rangeEditor.*`, `multiRename.presets.moveUp/Down`)

- A position in prose is a cardinal after the noun (`el carácter 3`, `Del carácter 2 al 5`), like `charRange`;
  `el 3.º carácter` would need the `3.er` apocope. A bare option label keeps `#.º`. From the end: `último`, `penúltimo`,
  `el carácter # desde el final`. Range edges are `Inicio` / `Fin` (`Inicio` as in the counter).
- Presets reorder with `Subir` / `Bajar`: a place in a list, not run order, so not the queue's `Adelantar`.
