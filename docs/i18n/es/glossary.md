# es glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/es/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the source(s) and
  a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

## Terms

Settled during the `settings.json` pass (mined from `_ignored/i18n/es/`, mostly macOS Tier 1; grep over Finder + AppKit + SystemSettings, 2026-06-21).

- settings → Ajustes · macOS SystemSettings ("Ajustes", "Ajustes del Sistema") · high. NOT "Configuración" (Windows term).
- appearance (Settings section) → Apariencia · macOS uses "Aspecto" for its own pane, but "Apariencia" is the broader, clearer noun and reads naturally as a section title; chosen for Cmdr's own section name · high
- folder → carpeta · macOS Finder ("Carpeta", "carpeta inteligente") · high
- directory → carpeta · same as folder; Spanish UI says "carpeta" for both (macOS never says "directorio" in Finder) · high
- file → archivo · macOS/MS standard; never "fichero" (Spain-only, per style guide) · high
- pane → panel · Total Commander/Double Commander es ("panel"); macOS has no two-pane concept · high
- column → columna · macOS Finder ("columnas", "visualización como columnas") · high
- sidebar → barra lateral · macOS Finder ("Mostrar barra lateral") · high
- tab → pestaña · macOS Finder ("Nueva pestaña") · high
- search → buscar (verb/button) / búsqueda (noun) · macOS Finder ("Buscar:", "Búsqueda guardada") · high
- default (value) → por omisión · macOS Finder ("estilo por omisión", "aplicación por omisión") · high
- reset → restablecer · macOS ("Restablecer tamaños") · high
- loading → Cargando... · macOS ("Cargando…"); Cmdr catalog uses three ASCII dots to match source shape · high
- clear → borrar · macOS ("Borrar búsquedas recientes") · high
- eject → expulsar · macOS Finder ("Expulsar") · high
- trash → papelera · macOS ("Papelera") · high
- notifications → notificaciones · macOS ("Centro de notificaciones") · high
- downloads (folder) → Descargas · macOS ("Descargas") · high
- privacy → privacidad · macOS ("Privacidad y seguridad") · high
- update → actualización / actualizar · macOS/MS standard · high
- enable → activar · macOS ("activa Bluetooth") · high
- show / hide → mostrar / ocultar · macOS ("Mostrar barra lateral", "ocultar/mostrar") · high
- shortcut (keyboard) → atajo / atajos de teclado · macOS/MS standard · high
- timeout → tiempo de espera · MS terminology standard · high
- size → tamaño · macOS ("Restablecer tamaños") · high
- index/indexing → índice / indexación · MS/standard · high
- threshold → umbral · standard technical Spanish · tentative (no direct macOS hit)
- pixels → píxeles · standard · high
- toggle (in descriptions) → expressed via the action (activar/desactivar), not a noun · high
- server → servidor · macOS ("desmontar servidores") · high
- share (network) → recurso compartido · MS terminology standard for "network share" · high
- mount (verb) → montar · standard; macOS uses "desmontar servidores" · high
- word wrap → ajuste de línea · MS terminology standard · tentative

### Cmdr-internal view-mode and feature names (kept consistent across the catalog)
- Full (view mode) → Completa · Cmdr's own view-mode name; "vista completa" · tentative, review
- Brief (view mode) → Breve · Cmdr's own view-mode name; "vista breve" · tentative, review
- Smart / Dynamic / Content / On disk / Rainbow / Wilting (option names) → Inteligente / Dinámico / Contenido / En disco / Arcoíris / Marchitamiento · composed; these are Cmdr's own option labels with no source equivalent · tentative, review

### Settled during the `fileExplorer.json` pass (mostly macOS Tier 1; Finder + AppKit greps, 2026-06-21)
- copy → copiar · macOS Finder ("Copy"→"Copiar") · high
- move → mover · macOS Finder (label sense) · high
- delete → eliminar · macOS Finder ("Eliminar") · high
- delete permanently → Eliminar permanentemente · composed from macOS "Eliminar"; Cmdr's wording is "permanently" → "permanentemente" (vs macOS bypass-trash "Eliminar inmediatamente") · high
- rename → renombrar · macOS Finder ("Rename"→"Renombrar", keys RN24/N206) · high
- view (file) / edit (file) → ver / editar · infinitive labels, standard · high
- favorites → Favoritos · macOS Finder/AppKit ("Favorites"→"Favoritos") · high
- connect / connecting → conectar / Conectando... · macOS Finder ("Connect"→"Conectar", "Connecting…"→"Conectando…"); catalog uses 3 ASCII dots · high
- disconnect → desconectar · macOS Finder ("Disconnect"→"Desconectar") · high
- host → host · technical network-device noun, kept as-is ("servidor" reserved for "server"; no macOS "anfitrión" in pile). "Hostname" → "Nombre de host" · tentative
- share (SMB noun) → recurso compartido · macOS ("recurso compartido"/"carpeta compartida") + MS; tight "Shares" column header → "Recursos" · high
- mount → montar · Xfce Thunar ("_Mount"→"_Montar") · high
- retry → reintentar · macOS AppKit ("Retry"→"Reintentar", NE106/PE110) · high
- try again → Reintentar (button) / inténtalo de nuevo (sentence) · macOS Finder ("Inténtalo de nuevo más tarde") · high
- refresh → actualizar · macOS AppKit ("Refresh"→"Actualizar", LA26) · high
- back → Atrás · macOS Finder ("Back"→"Atrás", 211.title) · high
- sign in / log in → iniciar sesión · macOS Finder ("Iniciar sesión…", NE104) · high
- password / username → contraseña / nombre de usuario · macOS Finder ("Contraseña:", "usuario") · high
- read-only → solo lectura · macOS Finder/AppKit ("Solo lectura", 138/pft) · high
- network → Red · macOS Finder ("Network"→"Red", 300516/FF22.1) · high
- volume → volumen · macOS Finder · high
- Keychain → Keychain · kept verbatim per style guide do-not-translate (macOS UI says "Llavero") · confirmed (style guide)
- credentials → credenciales · standard · high
- symlink → enlace simbólico; "(broken symlink)" → "(enlace simbólico roto)" · standard · high
- permission denied → permiso denegado · standard · high
- home folder → carpeta de inicio · composed; macOS "Inicio" for Home · tentative
- dir (abbrev) → dir · kept short matching English abbrev in tight status-bar · tentative
- DIR (size-column marker) → DIR · kept as-is, short folder marker · tentative
- host/server unreachable → No se puede acceder a … · standard phrasing · high

### Settled during the `errors.json` pass (error/recovery copy; macOS Finder + AppKit + SystemSettings greps, 2026-06-21)
- locked (file) → bloqueado · macOS Finder ("el archivo está bloqueado", NE17) · high
- Get Info (Finder menu) → Obtener información · macOS Finder ("Selecciona Archivo > Obtener información", NE43) · high
- Locked (checkbox in Get Info) → Bloqueado · macOS Finder ("anula la selección de Bloqueado", NE18) · high
- authentication → autenticación · macOS Finder ("No se ha podido realizar la autenticación") · high
- timed out → tiempo de espera agotado · macOS ("Tiempo de espera agotado…") · high
- not enough space → no hay suficiente espacio · macOS Finder ("no hay suficiente espacio disponible") · high
- app (the noun) → app · macOS keeps "app"; matches Cmdr's casual voice · high
- unmount → desmontar · macOS Finder ("desmontar servidores") · high
- "Couldn''t read/find…" (error title) → "No se pudo leer/encontrar…" · impersonal "se pudo" is calmer than a bare label, fits Cmdr''s no-bare-"error" voice · high
- "{Verb} failed" (write-op title) → "No se pudo completar la acción {Verb}" · CRITICAL: `{verb}`/`{Verb}`/`{gerund}` placeholders hold an ENGLISH word at runtime (operationVerbMap is hardcoded en: copy/move/delete/move to trash; gerunds copying/moving/…). So frame them as the noun-like "la acción {verb}" / "la acción {gerund}" (mirrors fr "l''action {verb}"), NEVER as a Spanish verb slot, or the sentence reads "No se pudo copy". The `.title` keys use `{Verb}` (capitalized) — keep the capital · high
- handle (open file handle) → identificador · standard; "another open handle" → "otro identificador abierto" · tentative
- Disk Utility → Utilidad de Discos · macOS · high
- First Aid (Disk Utility) → Primera ayuda · macOS · high
- Activity Monitor → Monitor de Actividad · macOS · high
- Login Items & Extensions → Ítems de inicio y extensiones · macOS · high
- Storage (Settings section) → Almacenamiento · macOS · high
- Privacy & Security (pane, when written as a plain literal in git suggestions) → Privacidad y seguridad · macOS SystemSettings · high
- Files and Folders (pane literal) → Archivos y carpetas · macOS · high
- git/worktree/repo/blob/commit/clone → kept as-is per do-not-translate (git terms); "repo" inflects naturally ("este repo", "los repos") · confirmed (prompt)

### Settled during the `licensing.json` + `ai.json` + `viewer.json` pass (macOS Finder/AppKit + MS terminology greps, 2026-06-21)
- license → licencia · standard; macOS ("licencia"); tier names "Personal"/"Commercial" kept as proper tier labels (capitalized) where they badge a tier, while sentences use the adjective "comercial" ("licencia comercial") · high
- license key → clave de licencia · "clave" for key (macOS "Contraseña" is for password; license key is "clave de licencia") · high
- activate / activating → activar / Activando... · macOS ("Activar", NE100/IN_S52); catalog uses 3 ASCII dots · high
- perpetual (license) → perpetua · composed; standard adjective · high
- valid until / expired on → válida hasta el / caducó el · standard; "caducar" for expire (license/subscription sense) · high
- subscription → suscripción · standard · high
- renew → renovar · standard · high
- organization → organización · standard · high
- clipboard → portapapeles · macOS ("Portapapeles", Clipboard key; "Contenido del portapapeles") · high
- copy / paste → copiar / pegar · macOS ("Copiar"; "pegar los ítems del portapapeles") · high
- download / downloading → descargar / Descargando... · macOS ("descargar", "Descargas", "Descargando" AXBADGE8) · high
- model (AI) → modelo · Double Commander es ("Modelo de la cámara"); standard · high
- server → servidor · macOS · high (already in settings pass)
- endpoint (API) → extremo · MS terminology (TBX entries 51058/257427 "endpoint" → 51059/342292 "extremo", incl. the service-endpoint sense "An endpoint where an application or system uses a service"). Label "Endpoint" → "Extremo"; "Endpoint URL" → "URL del extremo". Reconciled across `ai.json` + `onboarding.json` so the AI-settings field and the cloud-setup field match · high
- API key → clave de API · "clave" + "API" kept · high
- encoding (character) → Codificación · MS terminology ("character encoding"→"codificación de caracteres") · high
- Western (encoding group) → Occidental · macOS character-encoding submenu name (not in this pile snapshot; established Apple term) · tentative
- detected → Detectada/Detectado · agrees with the noun (codificación → Detectada) · high
- streaming (viewer mode) → transmisión / transmitiendo · standard · tentative
- wrap (word wrap badge) → ajuste · short form of "ajuste de línea" (glossary) for the tight badge · tentative
- tail (follow file, toolbar) → Seguir · composed; "follow"→"seguir" reads naturally for the auto-follow toggle (no macOS equiv; `tail -f` concept) · tentative, review
- reindex / reindexing → reindexar / Reindexando… · composed from "índice/indexación" (glossary); keeps the source's Unicode ellipsis · tentative
- in memory / indexed (badges) → en memoria / indexado · standard · high
- viewer → Visor · macOS ("Visor"); matches Settings section name · high
- selection → selección · standard · high
- restarting / starting / running / stopped (server status) → Reiniciando... / Iniciando... / En ejecución / Detenido · standard · high
- timed out (AI request) → agotó el tiempo de espera · from "tiempo de espera" (glossary) · high
- provider (AI) → proveedor · standard · high
- IA (AI) → IA · per Settings section name (AI → IA) · high

### Settled during the `onboarding.json` + `fileOperations.json` pass (macOS Finder/AppKit + Nautilus greps, 2026-06-21)
- OK (confirm button) → Aceptar · macOS AppKit ("OK"→"Aceptar") · high
- close → cerrar · macOS AppKit ("Cerrar") · high
- overwrite → sobrescribir · macOS Finder ("Sobrescribir en la carpeta de destino"); Nautilus uses "Reemplazar" but macOS Tier-1 wins · high
- skip → omitir · Nautilus ("_Omitir", "_Omitir archivos"); macOS has no direct file-op skip · high
- merge (folders) → fusionar · composed; Nautilus uses "Mezclar" but "fusionar" reads more standard for "merge with existing" in es UI · tentative (Nautilus says "Mezclar")
- rollback → revertir / reversión (noun) · composed; no macOS source. "Revertir" for the button, "la reversión" for the noun · tentative
- full disk access → acceso a todo el disco · composed from macOS permission naming; matches the FDA pane sense · tentative
- onboarding (the flow) → introducción · composed; "Introducción a Cmdr" / "progreso de la introducción" reads natural; no macOS source · tentative
- under cursor → bajo el cursor · standard · high
- hardlink/hardlinked → enlace físico · MS terminology standard (vs symlink "enlace simbólico") · high
- destination → destino · macOS ("carpeta de destino") · high
- conflict → conflicto · standard · high
- scan/scanning (counting files) → analizar / Analizando · standard; chosen over "escanear" (image-scan sense) · tentative
- feedback → comentarios · MS terminology standard ("Enviar comentarios") · high
- command palette → paleta de comandos · standard/MS · high
- issues (GitHub) → incidencias · MS terminology ("issue"→"incidencia") · high
- star/watch/fork (GitHub) → dar una estrella / seguir / hacer un fork · composed; "fork" kept (GitHub term), "seguir" for watch, "estrella" for star · tentative
- API key → clave de API · MS terminology ("clave de API") · high
- endpoint URL → URL del extremo · see the `endpoint (API) → extremo` entry above (reconciled with `ai.json`); "URL" kept · high
- pros and cons → pros y contras; Pro:/Con: bullet labels → "A favor:" / "En contra:" · composed · tentative
- toast (corner status) → aviso · composed; transient corner message (no macOS "tostada") · tentative
- source-available → código abierto · composed; renders the public-source sense plainly · tentative

### Cmdr-internal Settings section/subsection titles (so cross-references stay consistent)
- Appearance → Apariencia; Colors and formats → Colores y formatos; Zoom and density → Zoom y densidad; File and folder sizes → Tamaños de archivos y carpetas; Listing → Lista; Behavior → Comportamiento; File operations → Operaciones de archivos; File system watching → Vigilancia del sistema de archivos; Search → Búsqueda; AI → IA; File systems → Sistemas de archivos; SMB/Network shares → SMB/Recursos de red; MTP → MTP; Git → Git; Viewer → Visor; Developer → Desarrollador; MCP server → Servidor MCP; Logging → Registro; Updates & privacy → Actualizaciones y privacidad; Advanced → Avanzado; Keyboard shortcuts → Atajos de teclado; License → Licencia · composed/Cmdr-own; confidence tentative for the multi-word ones, review

### Settled during the `commands.json` + `queryUi.json` pass (command palette + search dialog; macOS Finder + AppKit + MS terminology greps, 2026-06-21)
- cut → cortar · macOS AppKit MenuCommands ("Cut"→"Cortar") · high
- paste → pegar · macOS AppKit MenuCommands ("Paste"→"Pegar") · high
- clipboard → portapapeles · macOS + MS ("Portapapeles") · high
- select all / deselect all → Seleccionar todo / Deseleccionar todo · macOS ("Seleccionar todo"); "deseleccionar" is the standard antonym · high
- command palette → paleta de comandos · MS terminology ("command palette"→"paleta de comandos") · high
- context menu → menú contextual · macOS Finder ("Mostrar menú contextual"); chosen over MS "menú de función rápida" (macOS Tier 1 wins) · high
- Quick Look → Vista rápida · macOS Finder ("Quick Look"→"Vista rápida"); the brand "Quick Look" is do-not-translate, but the macOS-localized action label is "Vista rápida", which Cmdr's mac variant reuses · high
- preview (non-mac fallback) → Vista previa · MS terminology ("preview"→"vista previa") · high
- Show in Finder → Mostrar en el Finder · macOS Finder (A34/N207) · high
- Get info → Obtener información · macOS Finder (N165/TL22) · high. File properties (non-mac) → Propiedades del archivo
- New folder / New tab → Nueva carpeta / Nueva pestaña · macOS Finder (N156/FR13) · high
- back / forward (nav) → Atrás / Adelante · macOS Finder ("Atrás", "adelante") · high
- zoom in / out (UI text size) → Aumentar el zoom / Reducir el zoom · macOS keeps the noun "Zoom" for window-zoom; for text-size zoom "Aumentar/Reducir el zoom" reads naturally and matches MS "acercar/alejar" sense. "Zoom to X%" → "Zoom al X%" · tentative
- ascending / descending (sort) → ascendente / descendente · standard; no macOS hit ("Ordenar por" is macOS's only sort label) · tentative
- wildcard → comodín · MS terminology ("wildcard"→"carácter comodín"); short form "comodín" for tight UI · high
- glob → Glob · kept verbatim (technical wildcard-pattern term; matches the en @key note) · high
- regex → Regex · kept verbatim (brand-like technical term) · high
- offline (make available offline) → sin conexión · MS ("offline"→"desconectado"/"sin conexión"); "sin conexión" reads more natural for files · high
- feedback → comentarios · MS/standard ("Enviar comentarios") · high
- onboarding → introducción · composed; "asistente de introducción" for the wizard · tentative
- scope (search) → ámbito · standard technical term for search scope · tentative
- pattern → patrón · standard · high
- query (search text) → consulta · MS/standard · high
- scan / scanning → análisis / "Análisis en curso" · standard; "analizar/análisis" for index building · tentative
- byte/bytes (unit word) → byte/bytes · macOS/MS keep these untranslated · high
- "boring folders" (playful) → carpetas aburridas · literal, preserves the intentional playful voice per the en @key note · tentative
- custom (cell/value) → personalizado · MS/standard · high
- Ask anything (AI mode) → Pregunta lo que sea · composed; Cmdr's own AI-mode label · tentative, review
- coming soon → próximamente · standard · high
- relative-time abbrevs (m/h/d/w/mo/y ago) → "hace {count} min/h/d/sem/mes/a" · es has no terse single-letter convention, so short words used; weeks→sem, months→mes, years→a · tentative, review

### Settled during the `indexing.json` + `downloads.json` + `errorReporter.json` + `shortcuts.json` + `mtp.json` + `ui.json` pass (macOS Finder/AppKit greps, 2026-06-21)
- drive (storage unit) → unidad · standard; macOS uses "unidad" for drives/volumes · high
- scan / scanning (drive index) → análisis / Analizando... · same as the scan/analizar choice in the fileOperations pass; "analizar" over "escanear" · tentative
- outdated / out of date (index) → desactualizado · macOS Finder ("no estén actualizados", NE103/NE105 for "may be out of date"); "desactualizado" is the natural adjective form · high
- entries (index entries) → ítems · macOS uses "ítems" broadly for files/folders/entries; reused for scanned "entries" · high
- dirs (terse status abbrev) → dirs · kept short matching the English terse abbrev in the compact status line · tentative
- s/m (time-left abbrevs, seconds/minutes) → s/min · "s" for seconds (universal); "min" for minutes (es has no terse single "m" minute convention) · tentative, review
- roughly (rough ETA) → aproximadamente · standard · high
- almost done → Casi listo · standard reassuring phrase · high
- background (run in the background) → en segundo plano · macOS/MS standard · high
- jump to (navigate to) → saltar a · composed; "saltar a la última descarga" reads natural for the quick-nav action · tentative
- global (shortcut scope) → global · MS standard ("atajo global"); kept short for the scope title · high
- in-app (shortcut scope) → en la app · composed; contrasts with "global" · tentative
- combo (key combination) → combinación · macOS uses "combinación de teclas"; short "combinación" in tight warnings · high
- register (a global hotkey) → registrar · MS standard · high
- modifier (key) → modificador · macOS/MS standard · high
- error report → informe de error · composed from "informe" (report, glossary) + "error"; the report-type proper name (the app's no-bare-"error" voice rule targets stand-in labels, not this named feature) · tentative, review
- redact / redacted (logs) → depurar / depurado · chosen over MS "tachar" (text-strikethrough sense) and "ocultar"; "depurar" reads as cleaning/sanitizing logs · tentative
- manifest (report metadata) → Manifiesto · standard technical term · tentative
- reference ID → ID de referencia · "ID" kept (macOS/MS), "de referencia" qualifies it · high
- preview (report preview) → vista previa · MS terminology (matches queryUi pass) · high
- bundle (log bundle) → paquete · standard; "paquete" for a packaged set of files · tentative
- note (free-text note) → nota · standard · high
- Reveal in Finder → Mostrar en el Finder · macOS Finder (matches commands.json "Mostrar en el Finder") · high
- Force Quit (macOS) → Forzar salida · macOS Finder ("Force Quit %@"→"Forzar salida de %@") · high
- Spotlight / Mission Control / Spaces → kept verbatim · macOS Spanish keeps these feature names untranslated · high
- Character Viewer (macOS) → Visor de caracteres · established Apple term (macOS emoji/symbol picker is "Emojis y símbolos"; the Character Viewer feature name is "Visor de caracteres") · tentative
- input source (keyboard) → fuente de entrada · standard macOS keyboard-layout term · tentative
- app switcher (macOS) → selector de apps · composed; Command-Tab switcher · tentative
- App windows (Mission Control) → Ventanas de la app · composed from macOS "ventanas" · tentative
- daemon (system process) → daemon · kept as the technical Unix term (ptpcamerad is a named daemon); no macOS UI translation · tentative
- udev / ptpcamerad / Terminal / Ctrl+C / PTP → kept verbatim · process/tool/protocol names (do-not-translate spirit); "Terminal" is the macOS app name · high
- exclusive access (device) → acceso exclusivo · standard · high
- in use by → siendo usado por · standard; "El dispositivo está siendo usado por …" · high
- combobox empty / suggestions → sugerencias · standard ("Cargando sugerencias", "Mostrar sugerencias") · high
- modal/dialog close (×) → Cerrar · macOS AppKit ("Cerrar") · high
- Keyboard shortcuts (Settings section) → Atajos de teclado · matches the Cmdr Settings section list · high
- conflict / conflicts (shortcuts) → conflicto / Conflictos · standard · high

### Settled during the wave-1 prep pass (`search` + `feedback` + `crashReporter` + `goToPath` + `transfer` + `updates` + `lowDiskSpace` + `commandPalette` + `whatsNew` + `main` + `common` + `notifications`; macOS Finder/AppKit + MS terminology greps, 2026-06-21)
- path → ruta · MS terminology ("path"→"ruta de acceso", all regions incl. ESP/419); short "ruta" in tight UI. "Go to path" → "Ir a la ruta" (macOS "Go To…"→"Ir a…", FR16/FR17) · high
- Restart → Reiniciar · macOS AppKit Menus ("Restart"→"Reiniciar") · high
- Later (defer button) → Más tarde · macOS standard defer-button label · high
- command → comando · MS terminology ("command"→"comando", all regions); "command palette" → "paleta de comandos" (already in glossary) · high
- startup disk → disco de arranque · macOS ("Startup Disk"→"Disco de arranque", A27/A28) · high
- running low on space → se está quedando sin espacio · composed; reads natural and calm for the low-disk warning · high
- Remove from list → Eliminar de la lista · macOS Finder ("Remove from Sidebar"→"Eliminar de la barra lateral", N169.2); "Eliminar de …" pattern · high
- crash report → informe de fallos · style-guide decision (gentlest non-alarmist word; "fallo" over technical "bloqueo") · tentative, confirm with David
- crashed / quit unexpectedly → se cerró inesperadamente · macOS AppKit ("it unexpectedly quit"→"se cerró inesperadamente") · high
- crashed (which part of the code) → falló · "qué parte del código falló" reads naturally for the privacy-note line; "fallar" ties to "fallos" · high
- Report ID → ID del informe · "ID" kept (macOS/MS); "del informe" qualifies it · high
- Show report details → Mostrar detalles del informe · from "Mostrar detalles" (macOS AppKit "Show Details") · high
- What''s new → Novedades · Apple App Store / Software Update term for "What''s New"; "Novedades de Cmdr" for the dialog title · high
- changelog / change log → registro de cambios · MS "change log" first hit is the quorum-log sense (wrong); "registro de cambios" is the standard ES term for a software changelog · high
- feedback → comentarios · MS terminology ("Send feedback"→"Enviar comentarios"); already in glossary, reaffirmed · high
- note (feedback note) → nota · standard (matches errorReporter pass) · high
- Enter (key name) → Intro · macOS Spanish keyboards label the Return/Enter key "Intro"; "Pulsa Intro" · tentative (no direct value-grep hit; Apple HW convention)
- press (a key) → pulsar · macOS uses "pulsa" for key/button presses · high
- book a call → reservar una llamada · composed; "reserva"/"reservar" standard for booking · tentative
- target (copy/move destination) → destino · macOS ("carpeta de destino"); "ya en el destino" for "already at the target" · high
- skipped (file op) → omitido / se omitió · from "omitir" (skip, glossary fileOperations pass) · high
- disable (notifications) → desactivar · MS terminology ("disable"→"desactivar") · high
- transfer-toast verb agreement → bake gender/number agreement into the ICU branches. "Copy complete"/"Move complete" → "Copia completada"/"Movimiento completado" (the adjective agrees: Copia fem., Movimiento masc.). Counted toasts wrap the whole clause in the `{count, plural}` so the verb agrees ("Se movió 1 archivo" / "Se movieron N archivos") · high
- Updates & privacy (Settings section, cross-ref) → Actualizaciones y privacidad · matches the Settings section list · high
