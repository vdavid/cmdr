# es decisions

Distilled rulings behind `terms.json`: why a form won, and what defends it against a well-meant "fix". The brief pulls a
section when its heading cites a batch key, so every heading cites its keys in backticks. Voice and grammar: `style.md`;
open questions: `review-queue.md`.

## Rulings that have no concept of their own

Small, local, binding (tentative unless sourced).

- **Settings sections**: Apariencia (reads better as a title than macOS's Aspecto), Colores y formatos, Zoom y densidad,
  Tamaños de archivos y carpetas, Lista, Comportamiento, Vigilancia del sistema de archivos, Desarrollador, Servidor
  MCP, Actualizaciones y privacidad, Avanzado, Navegación y operaciones de archivos.
- **Unsourced option labels**: Inteligente / Dinámico / Contenido / En disco / Arcoíris / Marchitamiento; Western
  (encoding) → Occidental; Detectada/Detectado agrees with its noun.
- **Tight abbreviations**: relative time → `hace {count} min/h/d/sem/mes/a`; time left `s` / `min`.
- **Shortcuts**: en la app; combinación (macOS combinación de teclas); registrar a hotkey; Atajos de teclado.
- **Error reporter**: Manifiesto; ID de referencia; ID del informe; paquete (log bundle); redact → depurar.
- **Licensing**: clave de licencia; perpetua; válida hasta el / caducó el; suscripción; renovar; organización.
- **Viewer**: transmisión; the word-wrap badge `ajuste`; tail → Seguir; reindexar / Reindexando….
- **Onboarding**: pros y contras, A favor: / En contra:; incidencias (GitHub issues, MS); seguir / hacer un fork;
  reservar una llamada; ¿Qué acaba de pasar?; Me gusta / ¿No te gusta?; No volver a hacer esto.
- **Search**: ámbito; patrón; comodín; Glob / Regex; personalizado; carpetas aburridas (the playful voice is
  deliberate); Pregunta lo que sea.
- **Misc**: vacío (AppKit); por ADB / por USB in a label, `a través de un cable USB` for the physical cable; en los
  sitios habituales; marcador de posición (MS); implementación (an Azure deployment, MS); OK → Aceptar; under cursor →
  bajo el cursor; umbral; ajuste de línea; píxeles; identificador (a file handle); enlace simbólico (roto); enlace
  físico; siendo usado por; a toggle in a description is the action itself (activar / desactivar), never a noun.
- **Verbatim**: git, worktree, repo (inflects: los repos), blob, commit, clone, byte(s), FAT32, exFAT, daemon, udev,
  ptpcamerad, Terminal, Ctrl+C, PTP.

## `worktree` frente a `árbol de trabajo` (`errors.git.*`, `fileExplorer.git.size.linkedWorktrees`)

A worktree (git's linked checkout) stays `worktree`, masculine, plural -s (`dos worktrees vinculados`); the generic
"working tree" in `errors.git.bareRepo` / `blobTooLarge` / `gitDirPermissionDenied` is prose, `árbol de trabajo`.

## Transfer toasts: agreement lives inside the plural branches (`transfer.*`)

`Copia completada` / `Movimiento completado` agree with their nouns; counted toasts put the whole clause in the plural
(`Se ha movido 1 archivo` / `Se han movido N archivos`), compress included.

## `Carpeta superior`, no `carpeta contenedora` (`commands.navParent.label`, `menu.go.parentFolder`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

`Carpeta superior` over Finder's `carpeta contenedora`: Finder's phrase translates "Enclosing Folder", not Cmdr's
"Parent folder"; Double Commander uses `carpeta superior` for this exact feature, and `subir a la carpeta superior`
carries the direction. Menu and palette must match. Pane background → `fondo del panel`, distinct from `espacio vacío`.

## Preajustes y el botón «Volver a …» (`settings.appearance.*`)

preset → `preajuste` (Double Commander). A bare Back is `Atrás`; "Back to X" needs the verb: `Volver a los preajustes`.

## FAT32: el archivo que no cabe (`errors.write.*`, `errors.listing.notSupportedErrno.suggestion`)

"formatted as X" → `tiene formato X` / `con formato X` (macOS's noun; dodges agreeing `formateada`). "{name} is {size}"
→ `{name} ocupa {size}`: the natural verb, and it doesn't agree with `{name}`.

## El diálogo de copiar y eliminar (`fileOperations.transferDialog.operationAria`, `fileOperations.transferDialog.targetWillBeCreatedCopy`/`targetWillBeCreatedMove`, `fileOperations.transferProgress.stageScanning`)

`Cmdr la creará durante la copia` / `… durante el movimiento`: two literal sentences, `la` agrees with `carpeta`, the
nouns are the catalog's op nouns. Scanning… → `Analizando…`, like `stageScanning`.

## Archivos comprimidos: explorar, extraer, eliminar (`settings.archives.*`, `fileOperations.delete.archiveWarningStrong`/`.archiveWarningRest`, `queue.row.label`)

- archive → `archivo comprimido`, never bare `archivo` (that's file); `el zip` where zip alone disambiguates.
- encrypted → `cifrado` over the pile's stale `Encriptado` (RAE, current macOS). extract → `extraer` over `descomprimir`
  (tar isn't compressed). preview (verb) → `previsualizar`; the noun stays `vista previa`.
- for good → `para siempre`, warmer than `permanentemente`; the two warning halves concatenate.

## Pegar el portapapeles como archivo (`fileExplorer.clipboard.pastedAsFile`, `settings.fileOperations.pasteClipboardAsFile.*`)

The toast's ICU select supplies article + noun so it agrees (`la imagen` / `el PDF` / `el texto`); impersonal
`Se ha pegado` doesn't gender the user.

## La contraseña de un archivo comprimido y Comprimir (`fileOperations.archivePassword.*`, `commands.fileCompress.label`, `settings.archives.compressionLevel.*`)

In the Compress dialog the name field says plain `archivo` (the zip is a file). Slider ends `Más rápido` (packing speed)
/ `Más pequeño` (output size).

## El registro de operaciones (`operationLog.*`, `commands.logOperationLog.*`)

- Summary lines: impersonal `Se` + preterite (history rows can be days old: `Se copió` / `Se copiaron`); badges match
  `queue.row.status`.
- "Didn't finish" → `No se completó`: neutral, avoids `Falló`, cousin of the queue's `No se ha podido completar`.
- Initiators `Tú` / `Cliente de IA` / `Agente`: no gendered noun.

## Ask Cmdr: el panel del chat (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- `chat` / `chats` (RAE loanword, universal in UI); `Chats`, `Ask Cmdr` keys carry `sameAsSourceJustification`.
- usage → `uso` over MS's formal `utilización`; "Spending" → `Gasto` (money), apart from "usage".
- cost → `coste` (Spain's form); token → `token` (MS's hits are security tokens).
- "Try again?" inline → `¿Lo intentas de nuevo?`, the catalog's dominant pattern.
- Tool-call lines: subjectless gerund for "doing", impersonal `Se` + perfect for "done" (`Se ha preparado un plan…`); a
  bare `Preparó…` reads as a third-person subject.
- "in settings" → `en Ajustes`, capitalized even for lowercase English. Menu-path separators mirror English per key.

## Indexación de fotos en unidades de red (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`/`.paused`)

Warm status and help lines say `foto`; feature and label names keep `imagen` (English splits the same way). "while
you're not busy" → `mientras no estás usando el Mac` (dodges gendered `ocupado`). photo archive → `colección de fotos`,
dodging both `archivo` collisions.

## Revisar los cambios de nombre (`askCmdr.renameReview.*`, `askCmdr.tool.imageFacts.*`, `askCmdr.tool.searchPhotos.*`, `askCmdr.tool.proposeRenamePlan.*`)

`Nombre actual` / `Nombre nuevo` stay parallel as a column pair, though macOS fronts the adjective in field labels;
`suggestedOps.columnNewName` and `editName` follow. `Permitir todos` / `Denegar todos` agree with
`los cambios de nombre`.

## La indexación de imágenes: carpetas, estados, insignias (`fileExplorer.imageIndex.*`, `settings.mediaIndex.scope.*`, `settings.mediaIndex.chosenFolders.*`, `settings.mediaIndex.showFileStatusIcons.*`, `settings.mediaIndex.progressSummary.title`)

- `pasada` (an indexing pass) stays apart from `análisis` (the full drive scan).
- `Carpetas para indexar` pairs with `Carpetas para indexar siempre`.
- "still searchable" → `se puede seguir buscando`; `buscable` reads unnatural.
- The agreeing participle (`indexada` / `indexadas`, `está` / `están`) goes inside the plural arms; `one` drops "All":
  `{totalText} imagen … indexada`.
- "Indexing now" → `Indexando ahora` in both keys (one English, one value).

## La búsqueda por descripción y el modelo (`settings.mediaIndex.cards.*`, `settings.mediaIndex.semanticSearch.label`, `settings.mediaIndex.clip.*`)

`Apple silicon` verbatim, as Apple's Spanish writes it. reclaim → `liberar`; `Eliminar modelo (liberar {size})` drops
the article to parallel `Descargar modelo (…)`.

## El interruptor de papelera y los encabezados Desde / Hacia (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`.targetGroupTitle`)

`Mover a la papelera` over Finder's `Trasladar`: the catalog keeps ONE move verb. From / To → `Desde` / `Hacia` (Double
Commander's exact pair); a bare `A` reads as a stray letter.

## La indexación de unidades apagada (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`.tooltipIndexingOff`/`.menuIndexingOffNote`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

- The path quotes the sidebar verbatim: `Indexación > Indexación de unidades`; never `indización`.
- "stays unindexed" → `sigue sin indexar`: no gender, survives any `{name}`.
- "picks up where it left off" → `continuará donde lo dejó`, ❌ not `seguirá` (bare `seguir` + place reads "stay put").
- A settings path followed by a `y` clause takes a comma, or it reads as a list.
- `overriddenBadge` → `Desactivado con la indexación` (masculine like the off labels; `de unidades` dropped, the badge
  only shows on that page).

## Índice de unidades: la pasada de comprobación de cambios (`indexing.step.*`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

"Checking for changes" → `Comprobación de cambios`, a noun phrase beside `Primer análisis completo`. `análisis` is this
catalog's word for a full check.

## Transferencia atascada: el aviso de "sin progreso" (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- `Sin progreso desde hace {duration}`: "for X, still" needs `desde hace`, never `durante` or bare `hace`.
- `Esperando a que X responda` (Finder), never the calque `esperando por`.
- "has stopped moving" → `ha dejado de avanzar`: ❌ not `se ha detenido` / `se ha quedado parada`, which read as the
  queue's `En pausa`.
- `stallInFlight` bakes the whole sentence into the plural arms (`esté` / `estén`, `escrito` / `escritos`).

## Ruta copiada: la confirmación del portapapeles (`fileExplorer.clipboard.copiedPath`)

The path renders on its own line below, so the sentence ends on a colon and stands alone:
`Ruta copiada, ya está en el portapapeles:` (no possessive; macOS always uses the article).

## Cola de operaciones: el cambio de nombre de la ventana (`queue.*`, `commands.queueShow.label`/`.description`, `fileOperations.transferProgress.queue*`, `fileOperations.transferProgress.backgroundedToast`)

`Cola de operaciones` (the window lists deletes, renames, and more, not only transfers) pairs with
`Registro de operaciones`. `commands.queueShow.label` is the bare window title, ❌ not `Mostrar la cola…`. The feminine
head noun keeps its clitics (`La encontrarás`); a masculine replacement would need them all redone. `transferencia`
still names the copy or move itself.

## El chip de la esquina y el aviso de operación sin terminar (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss`/`.dismissAria`, `queue.toolbar.dismissAll`)

- dismiss (clear a notice row) → `Descartar`; `Quitar` edits a list the user built. `Descartar todo` parallels
  `Pausar todo`.
- "Couldn't finish <action>" → `No se ha podido completar` + the op noun (la copia, el movimiento, el movimiento a la
  papelera, la eliminación, el cambio de nombre, la creación de la carpeta / del archivo, la edición del archivo
  comprimido): the row status verbatim. ❌ Not `terminar de copiar`.
- `{percentText}%` with no space, as the catalog writes it 10 to 1.
- "to {destination}" → ` a {destination}` (Nautilus); Finder's `en` reads locative with `Moviendo`.
- Optional clauses keep their leading space inside the branch.

## El aviso de conflicto de la ventana principal (`fileOperations.operationConflict.context`/`.pausedNote`)

`a {destination}` holds even with no object ("Copiando a Ana" may read as personal `a`, accepted); ❌ not
`a la carpeta {destination}`, since it can be a volume root or share. `Trabajando en {destination}` is locative.

## El botón con la cola vacía: "Background" (`fileOperations.transferProgress.background`/`.backgroundAria`)

`En segundo plano`: ❌ not bare `Segundo plano` (the foreground/background colour pair in Double Commander and MS), ❌
not `Pasar a segundo plano` (too long beside `Cola`). The infinitive-button rule yields: both states name a destination.
`backgroundAria` must still contain the label verbatim; rewrite them together.

## La salida con operaciones en curso (`main.quit.*`)

- quit, the user → `salir`; the app ending itself → `cerrarse` (`Cmdr se cerrará …`).
- still running → `en curso` in prose; `En ejecución` is the row badge.
- "Keep working" → `Seguir trabajando`: ❌ not `Cancelar`, which beside running operations reads as cancelling them.
- `se interrumpe donde esté`, ❌ not `se detiene`.
- `al reiniciar el Mac`: `el Mac` so it can't read as restarting Cmdr.

## Usage stats: fuera "anónimas", dentro "un identificador aleatorio" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

The stats carry a stable random id, so ❌ never `anónimas`, and ❌ never the jargon `seudónimo`:
`un identificador aleatorio`, tied to → `vincularse a`.

## Filas en cola a la espera de respuesta y la confirmación de reversión (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- "Needs your answer" → `Respuesta necesaria`: ❌ never anything starting with `Esperando`, the queued state in the same
  narrow column.
- "carries on" → `continuará`, ❌ not `se reanudará` (drags in the Reanudar button).
- "Keep them" → `Conservar los archivos`: a clitic would point at the replaced files.
- `foregroundBusyToast` names `esta operación`: English's "this one" has no Spanish antecedent.

## La cadena de renombrados: el aviso que cuenta los archivos que no cambiaron (`fileExplorer.rename.chainKeptOriginalName*`)

"kept its name" → `ha mantenido su nombre`: `conservar` is for a CHOICE, this is a fact. "and so did N others" →
`y otros {n} archivos también`; `one` is `y otro archivo también`, no numeral.

## El renombrado sin confirmar y el nombre que el sistema rechaza (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

Nobody knows whether the rename happened, so nothing may imply it didn't:
`es posible que el cambio sí se haya aplicado`, all arms plural (even `one` covers two renames), `ni el de` for the
others. `Ese nombre de archivo no puede usarse`: `Ese` is "that name you typed"; no full stop, it composes into
`chainKeptOriginalName`.

## Operaciones sugeridas: el diálogo de lo que propone Ask Cmdr (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

approve → `Aprobar` over macOS `Aceptar` (the counted variant authorizes an action). "Undo by deleting what it writes" →
`Deshacer eliminando lo creado`: `lo que crea` reads as `creer`.

## Duplicar: el comando que copia en la misma carpeta (`commands.fileDuplicate.*`)

`Duplicar` (Finder), beside `Copiar` / `Mover`; the description keeps `archivos` over Finder's `ítems`.

## Menús nativos: barra de menús, menús contextuales, títulos de ventana (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Finder (`MenuBar.strings`, `LocalizableMerged.strings`) decides; Safari for tabs; MS for the rest. RAW family.

- `Ocultar otras apps` in both keys; `Vista rápida` (Apple localizes it); `Por omisión`, never `Predeterminado`.
- zoom in / out → `Aumentar el zoom` / `Reducir el zoom` in menu AND palette: Safari's bare `Ampliar` needs a Zoom
  submenu the palette lacks.
- changelog → `Registro de cambios` (the document), apart from Help > `Novedades`.
- Tag row → `Añadir “{color}”` / `Quitar “{color}”`: Finder says `Eliminar`, but removing a tag deletes nothing.
- Remove from a list → `Quitar`, so removing a favourite never sounds like deleting files.

## El aviso de la conexión que macOS presta (`fileExplorer.network.osMountFallback.*`)

"You are connected" → `Tienes acceso` (no gendered `conectado/a`; ICU has no inflection). `la conexión del sistema` when
the sentence doesn't name macOS. `4x` → `4 veces más lenta`, digits kept. "for most connections" moves to the end so
`más lenta que…` sits beside its term.

## Los avisos de una línea de renombrar / crear (`errors.mutation.*`, `errors.volume.*`)

RAW family. Quote the long sibling (`errors.listing.*` / `errors.write.*`), don't reinvent: short line and explanation
say the same thing in the same words.

- `{path}` in curly quotes; nothing agrees with it.
- root folder (of a volume) → `carpeta raíz`, ❌ not `carpeta superior` (the parent).
- `sipProtected` → `Este elemento está bajo la protección de la integridad del sistema de macOS, así que …`, avoiding
  "protege … con la protección".
- `timedOut` isn't a failure; `deviceSessionReset` isn't a disconnect (`El dispositivo reinició su conexión.`).
- "Move it instead" → `Usa Mover.`, naming the F6 command.
- "stopped at your request" → `Cmdr detuvo esto porque se lo pediste.` (dodges formal `a petición tuya`).
- `trashRefused` → `macOS no permitió mover esto a la papelera.`: ❌ not `rechazó mover` (wants a noun), nor `no quiso`.
- `trashNotSupported` → `solo se puede eliminar permanentemente`: impersonal, no clitic to agree.

## El diálogo de fallos: las tres aperturas (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

Only `.ended` may say Cmdr closed (`se cerró inesperadamente`). "ran into a problem" → `tuvo un problema` (Cmdr as
subject, parallel to `.ended`). "kept running" → `siguió funcionando`, ❌ not `ejecutándose`, which names an OPERATION
here and invites that misreading right after `en segundo plano`.

## El texto de ajustes de informes ahora cubre los dos casos (`settings.updates.crashReports.description`)

Built from the crash-dialog pieces in the present; `un informe` without `de fallos`. The LABEL stays
`Enviar informes de fallos`: it's the setting's name.

## Expulsar y desconectar: los nueve avisos del selector de volúmenes (`errors.eject.*`)

Each follows `No se ha podido expulsar {volumeName}: …`. RAW family.

- drive being ejected → `disco` (Finder's whole eject corpus); elsewhere `unidad`.
- "Unplug it" → `Desconecta el cable`: `Desconéctalo` would echo the `Desconectar` command that just didn't work.
- `timedOut` / `volumeNotFound` / `unexpected` copy their `errors.mutation.*` siblings.
- `Cierra los archivos y las apps que tengas abiertos`: the `tener` relative avoids a stranded participle.

## El aviso de la papelera: deshacer y devolver a su sitio (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

put back → `devolver … a su sitio`: Finder's `Sacar de la papelera` fits a menu, and `restaurar` is reserved for NAMES
(a place comes back, a name is restored). The partial toast's second half conjugates on its own `{skipped}` count.

## Añadir a un informe ya enviado: el diálogo de la nota tardía (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- error report → `informe de error`; crash report → `informe de fallos`. Never cross them.
- Title infinitive `Añadir a tu informe de error`; `Añadir` over `agregar` (zero macOS hits).
- `Ya no se pueden añadir notas a ese informe`: no culprit, no `no se pudo`.
- "What was sent" → `Lo que se ha enviado`, the past of `errorReporter.dialog.detailsToggle`.
- `Nota añadida a tu informe.` agrees with `nota`, not the reader.

## El diálogo de seleccionar / deseleccionar archivos (`selection.*`)

deselect → `Deseleccionar` (Total / Double Commander; macOS has no verb, MS's `anular la selección` is too long). Menu,
palette, settings, and dialog title name the dialog identically. The popovers copy `queryUi.recent.*` with
`selecciones`.

## El nombre accesible tiene que contener la etiqueta visible (`queryUi.scope.toggle.caseSensitiveAria`, `viewer.search.caseSensitive`, `queryUi.recent.caseSensitive`)

case-sensitive → `Distinguir mayúsculas y minúsculas`, the full macOS form, and the aria wraps it (`… al buscar`); the
clipped form named one switch two ways.

## Una palabra inglesa, una palabra española: la revisión de deriva (`commands.helpSendErrorReport.label`, `menu.help.sendErrorReport`, `menu.zoom.in`/`.out`, `commands.handler.zoomResetHintMenu`, `menu.context.toggleSelection`, `fileExplorer.columns.modified`)

- Help menu and palette say `informe de error`: `informe de fallos` promised a crash report.
- `zoomResetHintMenu` points at `Visualización`, the real menu name.
- Toggle → `Activar o desactivar …`; `Modified` (date) → `Modificación` in all six keys.
- Short commands drop the article: `Copiar nombre de archivo`, `Mostrar archivos ocultos`.

### Fronteras deliberadas (no unificar)

Held in the term-consistency allowlist:

- `Ambos` (files and folders) / `Ambas` (notifications); `Operación cancelada` (pane title) / `Cancelado` (status).
- `Edición` / `Visualización` are MENUS; `Editar` / `Ver` the actions.
- `Problema` for the error status a user reads.
- `Modificación` is the date, `Modificados` the changed shortcuts; `Buscar` the action, `Búsqueda` the settings topic.
- `restaurar` for names, `devolver a su sitio` for places.
- `Connected` / `Connected!` and the like differ in English; `i18n-terms` groups them only because `¡` / `¿` survive its
  normalizer.

## Palabras que se separaron sin que ningún check pudiera verlo (`menu.go.parentFolder`, `licensing.dialog.typeCommercialPerpetual`/`.typeCommercialSubscription`, `errors.mutation.cantRenameVolumeRoot`, `askCmdr.renameUndo.applied`, `indexing.scan.counters`, `errors.write.trashNotSupported.suggestion`, `fileExplorer.renameConflict.overwriteTrash`, `settings.mediaIndex.clip.comingSoon`)

- default → `por omisión`, no exceptions (macOS es: zero `predeterminado`).
- License tiers follow the English capital: `licencia Comercial` (tier name) vs `suscripción comercial`.
- rename (verb) → `renombrar` even in prose; `cambiar el nombre` belongs to the noun.
- `papelera` lowercase in a sentence; macOS capitalizes only the standalone name.
- `permanentemente` (the F-key bar keeps `Permanente`); `Próximamente`.

## Los nombres de los paneles salen del Mac de quien usa Cmdr (`errors.git.*`, `errors.provider.*`)

`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` are filled from the user's Mac: a preposition may
precede them, an article or contraction may not. The iCloud keys open with `Abre {system_settings}, …` to avoid
`en … en …`.

## `Restaurar` nombra el objeto: el nombre anterior (`askCmdr.renameUndo.undone`/`.partial`)

`Se han restaurado los nombres anteriores de N archivos.`: the verb agrees with the names, so `one` is
`Se ha restaurado el nombre anterior de 1 archivo.`

## Una operación revertida a medias: terminar la reversión (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

`Terminar de revertir`: finishes what was left, never "start over"; shorter than `Completar la reversión`.
`en “{folder}”` in quotes, so the folder itself doesn't read as what gets removed.

## El aviso de una reversión que no lo pudo todo (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

Tone: Cmdr did the careful thing; never an apology or alarm.

- The reasons copy `askCmdr.renameUndo.skipReason.*`: `{name} se ha quedado como está: <motivo>.`; the two
  `folderNotEmpty` keys match their twins word for word.
- "it changed after Cmdr put it there" → `cambió después de que Cmdr terminara de escribir ahí`: keeps the brand AND
  agrees with nothing.
- The definite article is the only thing separating `done*` (`los {countText} elementos`) from `some*`; lose it and the
  partial notice promises a clean destination. `one` drops the numeral (`el elemento`).
- Rolling back a copy `elimina`, a move `devuelve … a su sitio`: never mix them.
- "Cmdr had written" → `que Cmdr había creado` (folders aren't written).
- `*.counted` keys only show for two or more: plural verb outside the branches, `one` unused. If one can ever be 1, redo
  all five.
- `stagedLeftover.*`: ⚠️ `en una transferencia posterior`, ❌ never `la próxima vez`: cleanup skips files younger than
  an hour, so an immediate retry removes nothing.

## La pantalla de bloqueo por WebKit antiguo (`main.oldWebkit.*`)

`Actualización de software` (System Settings); "or newer" → `o posterior`, Apple's formula.

## El aviso de macOS antiguo (`main.oldMacos.*`)

supported → `compatible`, ❌ not the calque `soportado`; best effort → `hace lo que puede`, ❌ not `mejor esfuerzo`;
look off → `verse raros`, away from the error register.

## Mirar dentro de un archivo: la herramienta `inspect_file` y la pantalla de consentimiento (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- `Mirando dentro de los archivos` / `Se ha mirado dentro de …`: `contenido` would promise more than it reads.
- camera details → `los datos de la cámara`; `detalles` sounds like a product sheet.
- where a photo was taken → `el lugar donde se hizo` (Spain's `hacer una foto`).
- `la lista de archivos que contiene un archivo comprimido`: `contener` breaks up `archivos dentro de un archivo`.
- The guarantee is `nunca cambia un archivo sin tu aprobación`: Ask Cmdr isn't read-only any more.

## Los dos textos de ayuda del botón Revertir (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

`Detener y devolver a su sitio todos los archivos movidos hasta ahora`: ❌ not `eliminar`, undoing a move deletes
nothing.

## “Abrir terminal aquí” y su selector de app (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Generic `terminal` and Apple's `Terminal` are the same loan; `Abrir terminal aquí` builds on Apple's `Abrir en Terminal`
and must read identically in menu and palette. Choose an app… → `Seleccionar app…`.

## `Sort by relevance`: la ayuda de la columna de resultados (`fileExplorer.columns.sortByRelevance`)

`Ordenar por relevancia`, framed like the sibling commands.

## `Documents and packages`: la fila OOXML (`settings.archives.ooxml.*`)

The row covers Office documents AND app packages, so bare `paquetes`, broader than the `Paquetes de apps` card below.

## El hub de servidores: conectar, desconectar y olvidar (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

- key (SSH) → `clave`, ❌ not `llave` (macOS keeps it for passkeys and `llavero`).
- compromised → `comprometida`: Apple's `filtrada` names a concrete leak, a `@revoked` mark isn't one.
- trust → `confiar en`, with macOS and Cmdr as active subjects.
- Try again as a BUTTON → `Reintentar`; in prose `inténtalo de nuevo` / `vuelve a intentarlo`.
- Busy tooltips copy `ejectBusyTooltip` word for word; `…Busy` menu items add ` (ocupado)`.
- Participles and clitics agree only with fixed nouns (`Sesión cerrada`, `Ábrelo` → el servidor), never `{name}`.

## La tabla del hub de servidores y sus comandos (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`serverUnpinnedToast`/`pinRefusedToast`/`networkVolume`, `shortcuts.scope.servers`/`places`)

- `Servidores` names the row; `Red` is only the group.
- `Places` → `Ubicaciones` (Finder's sidebar concept): it will cover buckets too, so ❌ not `Recursos compartidos`.
- `Last used` → `Último uso`: macOS date columns are nouns; ❌ not `Última conexión` (English chose "used" on purpose).
- Discovery → `detección`; `descubrimiento` sounds like a finding.
- `Añadir servidor…` drops the article for something new; commands on the selected server keep it.
- `Pin / unpin` → `Fijar o desfijar el servidor`: `o`, never a slash.

## La hoja para conectarse a un servidor y la confianza en la clave de host (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`signIn`/`hostKeyChanged`/`hostKeyChangedHint`, `goToPath.dialog.opensServer`/`addsServer`, `commands.serversConnect.label`)

- fingerprint → `huella digital`, ❌ not bare `huella` (Touch ID).
- `Conectando…` with no target; `Conectándose a X…` with one.
- Sheet titles are infinitive: `Añadir servidor` / `Iniciar sesión en {name}` / `Editar {name}`.
- Many values are forced by `i18n-terms` (same English elsewhere): search the catalog before translating a new sheet.
- `Trust and connect` → `Confiar en la clave y conectar`: `confiar` needs `en`.
- owner → `quien posee el servidor`, avoiding gendered `el propietario`; "something between you and it" →
  `hay algo entre tú y él`, no MITM jargon.
- `needsStoredSecret` is impersonal; `e inicia sesión` (`e` before `i-`).

## Las dos líneas nuevas del panel: reconexión automática y una sesión sin nada que teclear (`servers.paneState.reconnecting`/`signedOutNothingToAsk`)

`Reconectándose a {name}…` twins `Conectándose a {name}…` in the same panel (Apple splits between `Conectando de nuevo…`
and `Reconectando…`, so parallelism decides). type → `escribir`; `introducir` stays for credentials. The long line is
impersonal (`En este servidor se inicia sesión con una clave…`), so the server isn't the one signing in.

## Fijar servidores en el selector, las claves de host de confianza y la página de ADB (`menu.network.pinToSwitcher`/`unpin`, `servers.pinHint.*`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.servers.*`, `settings.adb.*`, `settings.appearance.tintSmb.*`)

- pin → `fijar` / `desfijar` across the family, over Safari's `anclar`; move all five together if it ever changes.
- Trusted → `de confianza`, ❌ not `fiable` (Apple's word for a certificate verdict).
- `la lista Servidores` names the row, unquoted; `la lista de servidores` elsewhere doesn't.
- ADB watching → `Cmdr detecta los teléfonos en cuanto los conectas`: ❌ not `Buscando teléfonos` (a search in
  progress).

## El teléfono Android por ADB: el panel de conexión, los avisos del selector y la línea de invitación (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Android's own Spanish (AOSP) decides what the user reads on the phone.

- `Depuración por USB`, ❌ not `depuración USB`; tap → `tocar`; the phone's button `Permitir`.
- wake a device → `activar`, ❌ not `despertar` (a person).
- `Abrir Ajustes` names the window; `Abrir los ajustes` (lowercase English) names the concept; move the three together.
- `You stopped opening your phone.` → `Has detenido la apertura de tu teléfono.`: ❌ not `Cancelaste`, which names the
  Cancel button.
- None of the 19 strings says `error`, `fallo`, or `no se pudo`.

## La identidad bloqueada del servidor (`servers.sheet.identityLocked`)

Names the actions like their buttons (`olvidar`, `añadir`), so the reader finds the menu. "are what name this server" →
`son las que identifican este servidor`: the sheet has its own `Nombre` field.

## El aviso cuando no había ninguna contraseña guardada (`fileExplorer.navigation.forgetSecretNoneToast`)

`No había ninguna contraseña guardada de {name}.`: the siblings' term and preposition; the imperfect gives the
matter-of-fact tone; `guardada` agrees with `contraseña`.

## La duración del reintento, el título de la clave del servidor y el botón Permitir de Android (`servers.paneState.retryTotalSeconds`/`retryTotalMinutes`, `servers.paneState.retryKeepsTrying`, `servers.refusal.hostKeyRevoked`)

The duration values are bare pieces of `retryKeepsTrying` (no preposition, no stop); the integer selects, the `…Text` is
read. `Cmdr no se conectará a {name}`: the future; `dejó de conectarse` sounded like an interrupted attempt.

## El menú contextual de la fila del servidor: `Abrir` y `Editar el servidor…` (`menu.network.open`, `menu.network.edit`)

Both equal their palette twins byte for byte (checked by `i18n-terms`).

## La barra de teclas de función (`settings.appearance.showFunctionKeyBar.label`)

`barra de teclas de función`, reused by the context-menu item and its notice.

## El aviso de «¿dejamos Cmdr en el Dock?» (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `el Dock`, `el Finder`: articles, masculine; `Cmdr` is masculine for a pronoun (`fijarlo`), avoiding a clash with
  `la app`.
- pin in the Dock → `fijar`, not the Dock menu's `Mantener en el Dock`: the family stays on `fijar`.
- Result keys state facts: `El icono de Cmdr ya está en su sitio, pero el Dock todavía no lo muestra`; `managedDock` →
  `Quien administre este Mac`.

## El menú del icono en el Dock (`menu.dock.*`)

The Dock's `DockMenus.strings` forms "verb + app name" with no article (`Abrir Cmdr`). `Ir a la carpeta…` (the item says
folder) stays apart from Cmdr's `Ir a la ruta` dialog (it says path). `{name} ({parent})` is identical to English.

## La oferta de «Mostrar en el Finder» y el aviso de la primera vez (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

“Mostrar en el Finder” in curly quotes, the card's exact name. "for a while now" → `un tiempo`, never a number. The
first-time notice isn't an apology.

## Las 23 claves del rediseño de la introducción (`onboarding.*`)

- star → `dar una estrella`, button `Estrella` (GitHub's Spanish); like (AlternativeTo) → `dar un me gusta`.
- checklist → `lista de tareas`; mailing list → `lista de correo`; sign up → `dar de alta`.
- typo → `errata`: dodges the `error` ban.
- handler (the process MTP displaces) → `proceso`; `controlador` stays free for a real driver.
- machine / this computer → `ordenador` (macOS es, 43 against 1 `equipo`), since es is written for Spain.
- `Local Network` → `“Red local”`, Apple's literal label, ❌ not a paraphrase.
- `superprivada` agrees with `la IA`; dumber → `más torpe` (frank, not insulting).

## El panel del visor mientras llega el archivo (`viewer.pull.*`, `viewer.error.stoppedResponding`)

Fetching → `Obteniendo`, ❌ not `Descargando` (a local archive isn't downloaded).
`Obteniendo el archivo {fileName} para previsualizarlo`: the noun first, so `lo` agrees with `el archivo`. so far →
`hasta ahora`, no participle.

## El índice desactualizado de un teléfono por ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`bodyPhone`)

The phone is still plugged in: nothing mentions a disconnect. `{name} no avisa a Cmdr` (genderless verb).
`los cambios que hace el propio Cmdr`, ❌ not `su índice` (ambiguous owner).

## La carpeta raíz y la carpeta inicial de un servidor guardado (`servers.sheet.rootFolder`/`rootFolderHelp`/`startFolder`/`startFolderHelp`/`nameHelp`, `servers.refusal.startFolderOutsideRoot`/`rootNotFound`/`startFolderNotFound`/`saveUnconfirmed`)

- root folder (the ceiling) → `Carpeta raíz`; start folder → `Carpeta inicial`, ❌ not `carpeta de inicio` (the home
  folder).
- `Dónde se abre el servidor.`: ❌ don't name `la carpeta`, or `Déjalo vacío` reads as "empty the folder".
- `usará la cuenta y el host como nombre del servidor`: ❌ not `por su cuenta` (that's "on its own").

## Por qué no se monta un recurso compartido o no carga su lista (`errors.mount.*`, `errors.shareList.*`)

NetAuthAgent words these cases. share → `recurso compartido` (its `volumen` for one share isn't taken). this computer →
`este ordenador`, not `Mac` (these lines also run on Linux). `cuando quieras`, no `listo/lista`. Same English, same
Spanish across the two families.

## F4 y su editor de texto (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

text editor is the app category, ❌ not TextEdit (that arrives in `{app}`). `Editar archivos con` takes any app name
without an article.

## Una unidad que se va a mitad de petición (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`)

`se está desconectando` / `se desconectó`: reflexives don't agree with `{name}`.

## Una unidad arrancada a mitad de transferencia (`errors.write.deviceDisconnected.*`)

The sentence exists to say where the files are.

- drive (disconnected on its own) → `unidad`, a deliberate boundary with the eject family's `disco`.
- `así que no has perdido nada`, ❌ not `no se pierde nada`, which hides whose files they are.
- to it → `ahí`; `después de que Cmdr copiara …` keeps the brand; `{counterpart}` follows a bare preposition.

## Un movimiento que no se pudo confirmar (`errors.write.moveNotConfirmed.*`)

Neither failure nor loss: Cmdr couldn't prove the copies landed. kept → `ha conservado` (on purpose), ❌ not `ha dejado`
(carelessness). `Tus originales siguen donde estaban.`: a positive statement reassures.

## La carpeta de trabajo de un movimiento que volvió con la unidad (`fileOperations.leftovers.stagingFolderKept`)

❌ Never imply the files can be deleted (they may be the only copy). unfinished move → `un movimiento sin terminar`, ❌
not `parcial` (promises part worked) nor `interrumpido`. `una carpeta oculta` is mandatory; `llamada {folderName}`
agrees with `carpeta`.

## El menú de favoritos: ⌃D, las teclas 1–9 y la fila que lo abre desde el selector (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`favoritesAlreadyAdded`/`favoritesCantAddHere`/`seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- favorites → `favoritos`, ❌ never `marcadores` (another feature in Double Commander and Dolphin).
- Deliberate `Ver {count} favoritos` (chooser row) / `Mostrar favoritos` (command) boundary: English splits See / Show.
- `seeFavorites`: `=0` + `one` + `many` + `other`.
- `la carpeta actual` keeps the article (an existing folder); `Esta carpeta ya está en favoritos`, the reverse of
  `Añadir a favoritos`.
- press a number → `pulsa un número` (keys `pulsar`, mouse `hacer clic`).

## Quién retiene el disco cuando la expulsión se rechaza (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- `{app}` opens the sentence with no article (a process name is a proper name).
- `Cierra lo que tenga abierto ahí`: `lo que` + subjunctive and `ahí` agree with nothing.
- `otras apps` joins through `Intl.ListFormat('es')` (`… y otras apps`).
- macOS itself → `macOS sigue trabajando con este disco`, kept apart from the apps' `sigue usando`.

## Seleccionar todo lo de la misma clase (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

kind → `Clase` (Finder's Kind). `menu.context.selection` is a noun, `Selección`. Twins stay identical bar the apostrophe
(RAW `menu.*` single, ICU `commands.*` doubled).

## La insignia de acceso total al disco en la barra de título (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

`acceso total al disco` is Apple's pane name, and every NAME uses it (the badge, `bannerTitle.*`, the System Settings
path in `bannerBody.stuck`). Running prose may say `acceso a todo el disco` (`proseAccept`). `fdaBadge.label` and
`bannerTitle.denied` share one English: reword both or neither. es-419 says `Acceso completo al disco`.

## El diálogo de rechazo de la papelera (`errors.write.trashRefused.*`)

- The title is shared with four `errors.write.*.title.trash` keys: reword all five.
- No plural machinery: `{count} de los elementos que elegiste` reads at 1 and 7 alike.
- `suggestion.other` reuses `Detalles técnicos` verbatim; the title-bar pill is quoted as `onboarding.fdaBadge.label`.

## El aviso de contenido solo en línea (`fileOperations.delete.cloudOnlineOnlyMixedWarning`, `fileOperations.delete.cloudOnlineOnlyAllWarning`, `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Four mandatory facts: the trash would download the files; so Cmdr offers only deleting the whole selection; no copy
stays in the trash (❌ don't soften it); the ways out. Keep both `<strong>` zones. `AllWarning` drops the deselect way
out.

## Cuando el servidor dice que ese recurso compartido no existe (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

The one case where retrying is useless: nothing may sound temporary (no `ahora mismo`, no `vuelve a intentarlo`).
`Esto no se va a arreglar solo`; `Sigues teniendo acceso`; `merece la pena echar un vistazo allí`.

## Nombres que se ven iguales en el servidor (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

No jargon (`Unicode`, `normalización`): `se ven iguales`, `el servidor los escribe de forma distinta`,
`letra con tilde`. `“Sobrescribir” reemplaza …` quotes the button, or the infinitive reads as a verbal noun.
`el elemento que ya está ahí`: a bare `el que` would agree with `nombres`.

## El interruptor “Permitir IA en la nube” y los estados con la IA en la nube desactivada (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

`Permitir IA en la nube` (the `Permitir <objeto>` mould); conjugated where English uses it as a verb
(`Permite … la IA en la nube`). `settings.askCmdr.enabled.label` is `Ask Cmdr`, identical to English.

### Esc y pantalla completa (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

`Salir de pantalla completa` (no article, macOS); `estar a pantalla completa`. `la tecla Esc` as a subject, bare `Esc`
in labels. `Solo verás este aviso una vez.`: names the notice, no gendered adjective.

### Líneas provisionales de “Abrir con” y “Compartir” (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`Buscando apps…`; share options → `opciones para compartir` (`recurso compartido` is the network sense).

## Coherencia del termbase (`goToPath.dialog.removeFromList`, `askCmdr.wakeDigest.removed`, `errors.write.fallback.message.*`, `onboarding.stepFda.con.body`, `indexing.staleDialog.body`, `indexing.rescan.*`)

- `Algo ha ido mal` everywhere; `errors.write.fallback.message.*` → `Algo ha ido mal al copiar.` (no `error`).
- `askCmdr.wakeDigest.removed` → `elementos que ya no están`: they may be trashed, deleted, or moved out.
- Cmdr is BSL: `su código fuente es público`, ❌ never `de código abierto`.
- Preview is `Vista Previa` on a Spanish Mac.
- The rescan lines are subjectless gerunds, like `Reiniciando el análisis desde cero`.
- watcher → `vigilancia`; view modes lowercase in prose (`la vista breve`).
- `indexing.staleDialog.body` → `estuvo sin conectar`, `la unidad`: nothing agrees with `{name}`.
