# es glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/es/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

Settled during the `settings.json` pass (mined from `_ignored/i18n/es/`, mostly macOS Tier 1; grep over Finder +
AppKit + SystemSettings, 2026-06-21).

- settings → Ajustes · macOS SystemSettings ("Ajustes", "Ajustes del Sistema") · high. NOT "Configuración" (Windows
  term).
- appearance (Settings section) → Apariencia · macOS uses "Aspecto" for its own pane, but "Apariencia" is the broader,
  clearer noun and reads naturally as a section title; chosen for Cmdr's own section name · high
- folder → carpeta · macOS Finder ("Carpeta", "carpeta inteligente") · high
- directory → carpeta · same as folder; Spanish UI says "carpeta" for both (macOS never says "directorio" in Finder) ·
  high
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
- Smart / Dynamic / Content / On disk / Rainbow / Wilting (option names) → Inteligente / Dinámico / Contenido / En disco
  / Arcoíris / Marchitamiento · composed; these are Cmdr's own option labels with no source equivalent · tentative,
  review

### Settled during the `fileExplorer.json` pass (mostly macOS Tier 1; Finder + AppKit greps, 2026-06-21)

- copy → copiar · macOS Finder ("Copy"→"Copiar") · high
- move → mover · macOS Finder (label sense) · high
- delete → eliminar · macOS Finder ("Eliminar") · high
- delete permanently → Eliminar permanentemente · composed from macOS "Eliminar"; Cmdr's wording is "permanently" →
  "permanentemente" (vs macOS bypass-trash "Eliminar inmediatamente") · high
- rename → renombrar · macOS Finder ("Rename"→"Renombrar", keys RN24/N206) · high
- view (file) / edit (file) → ver / editar · infinitive labels, standard · high
- favorites → Favoritos · macOS Finder/AppKit ("Favorites"→"Favoritos") · high
- connect / connecting → conectar / Conectando... · macOS Finder ("Connect"→"Conectar", "Connecting…"→"Conectando…");
  catalog uses 3 ASCII dots · high
- disconnect → desconectar · macOS Finder ("Disconnect"→"Desconectar") · high
- host → host · technical network-device noun, kept as-is ("servidor" reserved for "server"; no macOS "anfitrión" in
  pile). "Hostname" → "Nombre de host" · tentative
- share (SMB noun) → recurso compartido · macOS ("recurso compartido"/"carpeta compartida") + MS; tight "Shares" column
  header → "Recursos" · high
- mount → montar · Xfce Thunar ("\_Mount"→"\_Montar") · high
- retry → reintentar · macOS AppKit ("Retry"→"Reintentar", NE106/PE110) · high
- try again → Reintentar (button) / inténtalo de nuevo (sentence) · macOS Finder ("Inténtalo de nuevo más tarde") · high
- refresh → actualizar · macOS AppKit ("Refresh"→"Actualizar", LA26) · high
- back → Atrás · macOS Finder ("Back"→"Atrás", 211.title) · high
- sign in / log in → iniciar sesión · macOS Finder ("Iniciar sesión…", NE104) · high
- password / username → contraseña / nombre de usuario · macOS Finder ("Contraseña:", "usuario") · high
- read-only → solo lectura · macOS Finder/AppKit ("Solo lectura", 138/pft) · high
- network → Red · macOS Finder ("Network"→"Red", 300516/FF22.1) · high
- volume → volumen · macOS Finder · high
- Keychain → Llavero (store) / Acceso a Llaveros (app) · macOS Spanish · high · localized Apple feature name; Apple
  ships a Spanish-localized Keychain Access app ("Acceso a Llaveros"), so use that name, not the English "Keychain"
  (supersedes the old "keep Keychain verbatim" rule, per i18n-translation.md § Term-choice principles)
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
- "Couldn''t read/find…" (error title) → "No se pudo leer/encontrar…" · impersonal "se pudo" is calmer than a bare
  label, fits Cmdr''s no-bare-"error" voice · high
- "{Verb} failed" (write-op title) → "No se pudo completar la acción {Verb}" · CRITICAL: `{verb}`/`{Verb}`/`{gerund}`
  placeholders hold an ENGLISH word at runtime (operationVerbMap is hardcoded en: copy/move/delete/move to trash;
  gerunds copying/moving/…). So frame them as the noun-like "la acción {verb}" / "la acción {gerund}" (mirrors fr
  "l''action {verb}"), NEVER as a Spanish verb slot, or the sentence reads "No se pudo copy". The `.title` keys use
  `{Verb}` (capitalized) — keep the capital · high
- handle (open file handle) → identificador · standard; "another open handle" → "otro identificador abierto" · tentative
- Disk Utility → Utilidad de Discos · macOS · high
- First Aid (Disk Utility) → Primera ayuda · macOS · high
- Activity Monitor → Monitor de Actividad · macOS · high
- Login Items & Extensions → Ítems de inicio y extensiones · macOS · high
- Storage (Settings section) → Almacenamiento · macOS · high
- Privacy & Security (pane, when written as a plain literal in git suggestions) → Privacidad y seguridad · macOS
  SystemSettings · high
- Files and Folders (pane literal) → Archivos y carpetas · macOS · high
- git/worktree/repo/blob/commit/clone → kept as-is per do-not-translate (git terms); "repo" inflects naturally ("este
  repo", "los repos") · confirmed (prompt)

### Settled during the `licensing.json` + `ai.json` + `viewer.json` pass (macOS Finder/AppKit + MS terminology greps, 2026-06-21)

- license → licencia · standard; macOS ("licencia"); tier names "Personal"/"Commercial" kept as proper tier labels
  (capitalized) where they badge a tier, while sentences use the adjective "comercial" ("licencia comercial") · high
- license key → clave de licencia · "clave" for key (macOS "Contraseña" is for password; license key is "clave de
  licencia") · high
- activate / activating → activar / Activando... · macOS ("Activar", NE100/IN_S52); catalog uses 3 ASCII dots · high
- perpetual (license) → perpetua · composed; standard adjective · high
- valid until / expired on → válida hasta el / caducó el · standard; "caducar" for expire (license/subscription sense) ·
  high
- subscription → suscripción · standard · high
- renew → renovar · standard · high
- organization → organización · standard · high
- clipboard → portapapeles · macOS ("Portapapeles", Clipboard key; "Contenido del portapapeles") · high
- copy / paste → copiar / pegar · macOS ("Copiar"; "pegar los ítems del portapapeles") · high
- download / downloading → descargar / Descargando... · macOS ("descargar", "Descargas", "Descargando" AXBADGE8) · high
- model (AI) → modelo · Double Commander es ("Modelo de la cámara"); standard · high
- server → servidor · macOS · high (already in settings pass)
- endpoint (API) → extremo · MS terminology (TBX entries 51058/257427 "endpoint" → 51059/342292 "extremo", incl. the
  service-endpoint sense "An endpoint where an application or system uses a service"). Label "Endpoint" → "Extremo";
  "Endpoint URL" → "URL del extremo". Reconciled across `ai.json` + `onboarding.json` so the AI-settings field and the
  cloud-setup field match · high
- API key → clave de API · "clave" + "API" kept · high
- encoding (character) → Codificación · MS terminology ("character encoding"→"codificación de caracteres") · high
- Western (encoding group) → Occidental · macOS character-encoding submenu name (not in this pile snapshot; established
  Apple term) · tentative
- detected → Detectada/Detectado · agrees with the noun (codificación → Detectada) · high
- streaming (viewer mode) → transmisión / transmitiendo · standard · tentative
- wrap (word wrap badge) → ajuste · short form of "ajuste de línea" (glossary) for the tight badge · tentative
- tail (follow file, toolbar) → Seguir · composed; "follow"→"seguir" reads naturally for the auto-follow toggle (no
  macOS equiv; `tail -f` concept) · tentative, review
- reindex / reindexing → reindexar / Reindexando… · composed from "índice/indexación" (glossary); keeps the source's
  Unicode ellipsis · tentative
- in memory / indexed (badges) → en memoria / indexado · standard · high
- viewer → Visor · macOS ("Visor"); matches Settings section name · high
- selection → selección · standard · high
- restarting / starting / running / stopped (server status) → Reiniciando... / Iniciando... / En ejecución / Detenido ·
  standard · high
- timed out (AI request) → agotó el tiempo de espera · from "tiempo de espera" (glossary) · high
- provider (AI) → proveedor · standard · high
- IA (AI) → IA · per Settings section name (AI → IA) · high

### Settled during the `onboarding.json` + `fileOperations.json` pass (macOS Finder/AppKit + Nautilus greps, 2026-06-21)

- OK (confirm button) → Aceptar · macOS AppKit ("OK"→"Aceptar") · high
- close → cerrar · macOS AppKit ("Cerrar") · high
- overwrite → sobrescribir · macOS Finder ("Sobrescribir en la carpeta de destino"); Nautilus uses "Reemplazar" but
  macOS Tier-1 wins · high
- skip → omitir · Nautilus ("\_Omitir", "\_Omitir archivos"); macOS has no direct file-op skip · high
- merge (folders) → fusionar · composed; Nautilus uses "Mezclar" but "fusionar" reads more standard for "merge with
  existing" in es UI · tentative (Nautilus says "Mezclar")
- rollback → revertir / reversión (noun) · composed; no macOS source. "Revertir" for the button, "la reversión" for the
  noun · tentative
- full disk access → acceso a todo el disco · composed from macOS permission naming; matches the FDA pane sense ·
  tentative
- onboarding (the flow) → introducción · composed; "Introducción a Cmdr" / "progreso de la introducción" reads natural;
  no macOS source · tentative
- under cursor → bajo el cursor · standard · high
- hardlink/hardlinked → enlace físico · MS terminology standard (vs symlink "enlace simbólico") · high
- destination → destino · macOS ("carpeta de destino") · high
- conflict → conflicto · standard · high
- scan/scanning (counting files) → analizar / Analizando · standard; chosen over "escanear" (image-scan sense) ·
  tentative
- feedback → comentarios · MS terminology standard ("Enviar comentarios") · high
- command palette → paleta de comandos · standard/MS · high
- issues (GitHub) → incidencias · MS terminology ("issue"→"incidencia") · high
- star/watch/fork (GitHub) → dar una estrella / seguir / hacer un fork · composed; "fork" kept (GitHub term), "seguir"
  for watch, "estrella" for star · tentative
- API key → clave de API · MS terminology ("clave de API") · high
- endpoint URL → URL del extremo · see the `endpoint (API) → extremo` entry above (reconciled with `ai.json`); "URL"
  kept · high
- pros and cons → pros y contras; Pro:/Con: bullet labels → "A favor:" / "En contra:" · composed · tentative
- toast (corner status) → aviso · composed; transient corner message (no macOS "tostada") · tentative
- source-available → código abierto · composed; renders the public-source sense plainly · tentative

### Cmdr-internal Settings section/subsection titles (so cross-references stay consistent)

- Appearance → Apariencia; Colors and formats → Colores y formatos; Zoom and density → Zoom y densidad; File and folder
  sizes → Tamaños de archivos y carpetas; Listing → Lista; Behavior → Comportamiento; File operations → Operaciones de
  archivos; File system watching → Vigilancia del sistema de archivos; Search → Búsqueda; AI → IA; File systems →
  Sistemas de archivos; SMB/Network shares → SMB/Recursos de red; MTP → MTP; Git → Git; Viewer → Visor; Developer →
  Desarrollador; MCP server → Servidor MCP; Logging → Registro; Updates & privacy → Actualizaciones y privacidad;
  Advanced → Avanzado; Keyboard shortcuts → Atajos de teclado; License → Licencia · composed/Cmdr-own; confidence
  tentative for the multi-word ones, review

### Settled during the `commands.json` + `queryUi.json` pass (command palette + search dialog; macOS Finder + AppKit + MS terminology greps, 2026-06-21)

- cut → cortar · macOS AppKit MenuCommands ("Cut"→"Cortar") · high
- paste → pegar · macOS AppKit MenuCommands ("Paste"→"Pegar") · high
- clipboard → portapapeles · macOS + MS ("Portapapeles") · high
- select all / deselect all → Seleccionar todo / Deseleccionar todo · macOS ("Seleccionar todo"); "deseleccionar" is the
  standard antonym · high
- command palette → paleta de comandos · MS terminology ("command palette"→"paleta de comandos") · high
- context menu → menú contextual · macOS Finder ("Mostrar menú contextual"); chosen over MS "menú de función rápida"
  (macOS Tier 1 wins) · high
- Quick Look → Vista rápida · macOS Finder ("Quick Look"→"Vista rápida"); the brand "Quick Look" is do-not-translate,
  but the macOS-localized action label is "Vista rápida", which Cmdr's mac variant reuses · high
- preview (non-mac fallback) → Vista previa · MS terminology ("preview"→"vista previa") · high
- Show in Finder → Mostrar en el Finder · macOS Finder (A34/N207) · high
- Get info → Obtener información · macOS Finder (N165/TL22) · high. File properties (non-mac) → Propiedades del archivo
- New folder / New tab → Nueva carpeta / Nueva pestaña · macOS Finder (N156/FR13) · high
- back / forward (nav) → Atrás / Adelante · macOS Finder ("Atrás", "adelante") · high
- zoom in / out (UI text size) → Aumentar el zoom / Reducir el zoom · macOS keeps the noun "Zoom" for window-zoom; for
  text-size zoom "Aumentar/Reducir el zoom" reads naturally and matches MS "acercar/alejar" sense. "Zoom to X%" → "Zoom
  al X%" · tentative
- ascending / descending (sort) → ascendente / descendente · standard; no macOS hit ("Ordenar por" is macOS's only sort
  label) · tentative
- wildcard → comodín · MS terminology ("wildcard"→"carácter comodín"); short form "comodín" for tight UI · high
- glob → Glob · kept verbatim (technical wildcard-pattern term; matches the en @key note) · high
- regex → Regex · kept verbatim (brand-like technical term) · high
- offline (make available offline) → sin conexión · MS ("offline"→"desconectado"/"sin conexión"); "sin conexión" reads
  more natural for files · high
- feedback → comentarios · MS/standard ("Enviar comentarios") · high
- onboarding → introducción · composed; "asistente de introducción" for the wizard · tentative
- scope (search) → ámbito · standard technical term for search scope · tentative
- pattern → patrón · standard · high
- query (search text) → consulta · MS/standard · high
- scan / scanning → análisis / "Análisis en curso" · standard; "analizar/análisis" for index building · tentative
- byte/bytes (unit word) → byte/bytes · macOS/MS keep these untranslated · high
- "boring folders" (playful) → carpetas aburridas · literal, preserves the intentional playful voice per the en @key
  note · tentative
- custom (cell/value) → personalizado · MS/standard · high
- Ask anything (AI mode) → Pregunta lo que sea · composed; Cmdr's own AI-mode label · tentative, review
- coming soon → próximamente · standard · high
- relative-time abbrevs (m/h/d/w/mo/y ago) → "hace {count} min/h/d/sem/mes/a" · es has no terse single-letter
  convention, so short words used; weeks→sem, months→mes, years→a · tentative, review

### Settled during the `indexing.json` + `downloads.json` + `errorReporter.json` + `shortcuts.json` + `mtp.json` + `ui.json` pass (macOS Finder/AppKit greps, 2026-06-21)

- drive (storage unit) → unidad · standard; macOS uses "unidad" for drives/volumes · high
- scan / scanning (drive index) → análisis / Analizando... · same as the scan/analizar choice in the fileOperations
  pass; "analizar" over "escanear" · tentative
- outdated / out of date (index) → desactualizado · macOS Finder ("no estén actualizados", NE103/NE105 for "may be out
  of date"); "desactualizado" is the natural adjective form · high
- entries (index entries) → ítems · macOS uses "ítems" broadly for files/folders/entries; reused for scanned "entries" ·
  high
- dirs (terse status abbrev) → dirs · kept short matching the English terse abbrev in the compact status line ·
  tentative
- s/m (time-left abbrevs, seconds/minutes) → s/min · "s" for seconds (universal); "min" for minutes (es has no terse
  single "m" minute convention) · tentative, review
- roughly (rough ETA) → aproximadamente · standard · high
- almost done → Casi listo · standard reassuring phrase · high
- background (run in the background) → en segundo plano · macOS/MS standard · high
- jump to (navigate to) → saltar a · composed; "saltar a la última descarga" reads natural for the quick-nav action ·
  tentative
- global (shortcut scope) → global · MS standard ("atajo global"); kept short for the scope title · high
- in-app (shortcut scope) → en la app · composed; contrasts with "global" · tentative
- combo (key combination) → combinación · macOS uses "combinación de teclas"; short "combinación" in tight warnings ·
  high
- register (a global hotkey) → registrar · MS standard · high
- modifier (key) → modificador · macOS/MS standard · high
- error report → informe de error · composed from "informe" (report, glossary) + "error"; the report-type proper name
  (the app's no-bare-"error" voice rule targets stand-in labels, not this named feature) · tentative, review
- redact / redacted (logs) → depurar / depurado · chosen over MS "tachar" (text-strikethrough sense) and "ocultar";
  "depurar" reads as cleaning/sanitizing logs · tentative
- manifest (report metadata) → Manifiesto · standard technical term · tentative
- reference ID → ID de referencia · "ID" kept (macOS/MS), "de referencia" qualifies it · high
- preview (report preview) → vista previa · MS terminology (matches queryUi pass) · high
- bundle (log bundle) → paquete · standard; "paquete" for a packaged set of files · tentative
- note (free-text note) → nota · standard · high
- Reveal in Finder → Mostrar en el Finder · macOS Finder (matches commands.json "Mostrar en el Finder") · high
- Force Quit (macOS) → Forzar salida · macOS Finder ("Force Quit %@"→"Forzar salida de %@") · high
- Spotlight / Mission Control / Spaces → kept verbatim · macOS Spanish keeps these feature names untranslated · high
- Character Viewer (macOS) → Visor de caracteres · established Apple term (macOS emoji/symbol picker is "Emojis y
  símbolos"; the Character Viewer feature name is "Visor de caracteres") · tentative
- input source (keyboard) → fuente de entrada · standard macOS keyboard-layout term · tentative
- app switcher (macOS) → selector de apps · composed; Command-Tab switcher · tentative
- App windows (Mission Control) → Ventanas de la app · composed from macOS "ventanas" · tentative
- daemon (system process) → daemon · kept as the technical Unix term (ptpcamerad is a named daemon); no macOS UI
  translation · tentative
- udev / ptpcamerad / Terminal / Ctrl+C / PTP → kept verbatim · process/tool/protocol names (do-not-translate spirit);
  "Terminal" is the macOS app name · high
- exclusive access (device) → acceso exclusivo · standard · high
- in use by → siendo usado por · standard; "El dispositivo está siendo usado por …" · high
- combobox empty / suggestions → sugerencias · standard ("Cargando sugerencias", "Mostrar sugerencias") · high
- modal/dialog close (×) → Cerrar · macOS AppKit ("Cerrar") · high
- Keyboard shortcuts (Settings section) → Atajos de teclado · matches the Cmdr Settings section list · high
- conflict / conflicts (shortcuts) → conflicto / Conflictos · standard · high

### Settled during the wave-1 prep pass (`search` + `feedback` + `crashReporter` + `goToPath` + `transfer` + `updates` + `lowDiskSpace` + `commandPalette` + `whatsNew` + `main` + `common` + `notifications`; macOS Finder/AppKit + MS terminology greps, 2026-06-21)

- path → ruta · MS terminology ("path"→"ruta de acceso", all regions incl. ESP/419); short "ruta" in tight UI. "Go to
  path" → "Ir a la ruta" (macOS "Go To…"→"Ir a…", FR16/FR17) · high
- Restart → Reiniciar · macOS AppKit Menus ("Restart"→"Reiniciar") · high
- Later (defer button) → Más tarde · macOS standard defer-button label · high
- command → comando · MS terminology ("command"→"comando", all regions); "command palette" → "paleta de comandos"
  (already in glossary) · high
- startup disk → disco de arranque · macOS ("Startup Disk"→"Disco de arranque", A27/A28) · high
- running low on space → se está quedando sin espacio · composed; reads natural and calm for the low-disk warning · high
- Remove from list → Eliminar de la lista · macOS Finder ("Remove from Sidebar"→"Eliminar de la barra lateral", N169.2);
  "Eliminar de …" pattern · high
- crash report → informe de fallos · style-guide decision (gentlest non-alarmist word; "fallo" over technical "bloqueo")
  · tentative, confirm with David
- crashed / quit unexpectedly → se cerró inesperadamente · macOS AppKit ("it unexpectedly quit"→"se cerró
  inesperadamente") · high
- crashed (which part of the code) → falló · "qué parte del código falló" reads naturally for the privacy-note line;
  "fallar" ties to "fallos" · high
- Report ID → ID del informe · "ID" kept (macOS/MS); "del informe" qualifies it · high
- Show report details → Mostrar detalles del informe · from "Mostrar detalles" (macOS AppKit "Show Details") · high
- What''s new → Novedades · Apple App Store / Software Update term for "What''s New"; "Novedades de Cmdr" for the dialog
  title · high
- changelog / change log → registro de cambios · MS "change log" first hit is the quorum-log sense (wrong); "registro de
  cambios" is the standard ES term for a software changelog · high
- feedback → comentarios · MS terminology ("Send feedback"→"Enviar comentarios"); already in glossary, reaffirmed · high
- note (feedback note) → nota · standard (matches errorReporter pass) · high
- Enter (key name) → Intro · macOS Spanish keyboards label the Return/Enter key "Intro"; "Pulsa Intro" · tentative (no
  direct value-grep hit; Apple HW convention)
- press (a key) → pulsar · macOS uses "pulsa" for key/button presses · high
- book a call → reservar una llamada · composed; "reserva"/"reservar" standard for booking · tentative
- target (copy/move destination) → destino · macOS ("carpeta de destino"); "ya en el destino" for "already at the
  target" · high
- skipped (file op) → omitido / se omitió · from "omitir" (skip, glossary fileOperations pass) · high
- disable (notifications) → desactivar · MS terminology ("disable"→"desactivar") · high
- transfer-toast verb agreement → bake gender/number agreement into the ICU branches. "Copy complete"/"Move complete" →
  "Copia completada"/"Movimiento completado" (the adjective agrees: Copia fem., Movimiento masc.). Counted toasts wrap
  the whole clause in the `{count, plural}` so the verb agrees ("Se movió 1 archivo" / "Se movieron N archivos") · high
- Updates & privacy (Settings section, cross-ref) → Actualizaciones y privacidad · matches the Settings section list ·
  high

### Settled during the `queue.json` + new transfer-queue keys pass (transfer-queue window + pause/resume/background; macOS Finder + double-commander + Total Commander greps, 2026-06-21)

- pause (verb/button) → Pausar · macOS Finder ("Se ha pausado la copia de …", NE110); standard verb for the
  copy/transfer sense · high
- paused (state) → En pausa · double-commander ("Paused"→"Pausado"); "En pausa" reads cleaner as a status badge and
  matches macOS state phrasing ("en pausa") · high. ("Paused" dialog title → "En pausa")
- resume → Reanudar · macOS Finder ("Resume"→"Reanudar", NE101/PE108.1; "Reanudar copia", N158.1) — exact copy-resume
  sense, Tier 1 · high
- queue (the window) → cola · double-commander ("Queue"→"En cola"); macOS print "cola"; Total Commander "Adm. de transf.
  en segundo plano"; per-row/dialog "Queue" button (send-to-background) → "Cola" · high. ⚠️ The window NAME settled
  here, "Cola de transferencias", is SUPERSEDED: it is now **Cola de operaciones** (see § Cola de operaciones: el cambio
  de nombre de la ventana). The bare `cola` = queue mapping still stands.
- queued / waiting (queue status) → Esperando · matches the existing "Esperando…" waiting precedent in
  `fileExplorer.json`; the row sits behind another transfer on the same drive · high
- background / send to background → en segundo plano · macOS/MS/Total Commander standard (already in glossary); "Send to
  the operation queue" → "Enviar a la cola de operaciones" (window name superseded, see the entry above), "keep running
  in the background" → "mantener … en ejecución en segundo plano" · high
- transfer (the operation) → transferencia · reaffirmed (already used across the catalog); counted phrase "{n}
  transfer(s)" → "{n} transferencia(s)" (fem., so "seleccionada(s)" agrees) · high
- "Couldn''t finish" (failed row status, no-bare-"failed" voice) → No se pudo completar · from the errors-pass "No se
  pudo …" pattern; calm, avoids the bare "failed" label · high
- "Cancel selected" (toolbar) → Cancelar lo seleccionado · "lo seleccionado" for the neutral "the selection" sense ·
  high

### Settled during the double-click-to-parent navigation pass (Navigation & file ops settings + breadcrumb + double-click hint; macOS Finder + Double Commander + Thunar + MS terminology, 2026-06-26)

- parent folder → carpeta superior · CHOSEN over macOS Finder's "carpeta contenedora" ("Go To Enclosing Folder" → "Ir a
  la carpeta contenedora", `es/macOS/Finder/Localizable.json`) and Nautilus's "carpeta padre". Reasons, in order: (1)
  the es catalog already standardizes on it — `commands.navParent.label` = "Ir a la carpeta superior", plus four
  `errors.json` suggestions — so consistency settles it; (2) Double Commander, the orthodox two-pane source, renders the
  literally-identical feature ("Enable changing to parent folder when double-clicking on empty part of file view" →
  "Cambiar a la carpeta superior al hacer doble clic en una zona vacía de la vista de archivos"), and Thunar agrees
  ("Open the parent folder" → "Abrir la carpeta superior"); (3) "superior" carries the upward directionality of these
  go-up strings, so "subir a la carpeta superior" reads more naturally and concisely than the formal "carpeta
  contenedora" · high (overrides macOS Tier 1 on consistency + orthodox-two-pane + naturalness grounds; macOS-vs-file-
  manager split noted for the reviewer)
- double-click (verb) → hacer doble clic · MS terminology ("double-click"→"hacer doble clic", all regions incl. ESP,
  419, id 2133499); Double Commander ("al hacer doble clic"). Imperative `tú`: "Haz doble clic"; preterite "Hiciste
  doble clic" (matches macOS preterite address, e.g. "abriste") · high
- pane background → fondo del panel · "panel" = pane (glossary); "fondo" = the empty backdrop. Kept distinct from "empty
  space in a file list" (below) to preserve the source's two distinct phrasings · high
- empty space (in a file list) → espacio vacío · literal; Double Commander uses the equivalent "zona vacía de la vista
  de archivos" for the same gesture · high
- file list → lista de archivos · Double Commander (orthodox two-pane: "Refresh file list"→"Actualizar la lista de
  archivos", "left and right file list"→"la lista de archivos izquierda y derecha") · high
- navigate (to) → ir (a) · MS terminology ("navigate"→"ir", all regions, id 1624173); macOS Finder ("Ir a…"). "Click to
  navigate to {path}" → "Haz clic para ir a {path}" ({path} preserved) · high
- hint (one-time notification, internal label) → aviso · the doubleClickHint notification; "aviso" (notice) fits the
  transient-notification framing, consistent with "toast → aviso" (onboarding pass). Only on the internal/hidden
  `doubleClickOnPaneNotificationSeen` keys; no user-facing string names it "hint" · tentative (internal-only, low
  stakes)
- "go up to" (navigation) → subir a · natural with "carpeta superior"; "subir a la carpeta superior" · high
- "What just happened?" → ¿Qué acaba de pasar? · idiomatic; inverted ¿ · high
- "I like it" / "Don''t like it?" (hint buttons) → Me gusta / ¿No te gusta? · idiomatic short button copy · high
- "Never do this again" (hint button) → No volver a hacer esto · infinitive, per the button-label convention · high
- "Navigation & file ops" (settings subsection) → Navegación y operaciones de archivos · "file ops" = "operaciones de
  archivos" (File operations, settings-titles list); es has no terse short form, so the full noun phrase is used · high
- row / file row → fila / "la fila de un archivo" · MS terminology ("row"→"fila", all regions incl. ESP, 419, id
  106411); macOS ("Filas", NSTableOptionsPanel); Double Commander ("one per row"→"uno por fila"). "not a file row" → "no
  la fila de un archivo" (contrasts the empty pane background with an actual file''s row) · high
- "go up a folder" (shorter "go to parent") → subir a la carpeta superior · same destination as "go up to the parent
  folder"; reuses settled "carpeta superior". Label reworded to the imperative "Haz doble clic en el fondo del panel
  para subir a la carpeta superior" · high
- preset (value in a settings-picker dropdown) → preajuste; "back to presets" → "Volver a los preajustes". Note: the
  standalone "Back" button is the adverb "Atrás" (backArrow), but "Back to X" needs the verb "Volver a X" (pile: "volver
  a la versión anterior") · Double Commander es ("Preajustes"; "El preajuste «%s» ya existe") · high

### Settled during the FAT32-too-large filesystem-guard pass (copy/move error when a file exceeds the FAT32 4 GB cap; macOS Finder greps, 2026-06-30)

- too large (for a drive/format) → demasiado grande · macOS Finder, exact-concept hits: PE4.5 "El ítem «^0» no puede
  copiarse porque es demasiado grande para el formato del volumen" (file-too-large-for-format, our scenario) and NE77
  "«^0» es demasiado grande y no cabe en el disco" · high
- formatted as X / drive format (filesystem) → "tiene formato X" / "con formato X" · macOS uses the noun "formato"
  ("formato del volumen", PE4.5); the "tiene/con formato FAT32" framing avoids the participle gender agreement of
  "formateada" and reads cleanly · high
- FAT32 / exFAT → kept verbatim · filesystem-format names; the en `@key` says keep as-is. (macOS Disk Utility labels
  them "MS-DOS (FAT)" and "ExFAT", but Cmdr's source uses FAT32/exFAT, so those stay) · confirmed (prompt)
- store (files) → almacenar · standard verb for holding data; macOS uses "Capacidad del soporte" for capacity. "can''t
  store files larger than X" → "no puede almacenar archivos de más de X" · high
- "files larger than X" / "files this large" → "archivos de más de X" / "archivos tan grandes" · standard comparative
  phrasing · high
- file size statement "{name} is {size}" → "{name} ocupa {size}" · "ocupar" is the natural verb for how much space a
  file takes ("este archivo ocupa 5 GB"); macOS states sizes plainly (IN_G5_V2) · high
- "no such limit" → "no tiene ese límite" · standard · high
- drive (in this error) → unidad · reaffirms the existing glossary entry (drive → unidad); kept over macOS's
  context-specific "disco"/"volumen" for catalog consistency · high
- preset (value in a settings-picker dropdown) → preajuste; "back to presets" → "Volver a los preajustes". Note: the
  standalone "Back" button is the adverb "Atrás" (backArrow), but "Back to X" needs the verb "Volver a X" (pile: "volver
  a la versión anterior") · Double Commander es ("Preajustes"; "El preajuste «%s» ya existe") · high

### Settled during the copy/delete dialog-polish pass (Action label + scan tooltip; macOS Finder/AppKit + MS terminology, 2026-06-30)

- action (what a control chooses; screen-reader label `fileOperations.transferDialog.operationAria`) → Acción · macOS
  ("Action"→"Acción", e.g. Finder TL26/SP95, AppKit 200/201.title) · high
- "Scanning…" (spinner tooltip while counting items) → Analizando… · reuses the settled
  scan/scanning→analizar/Analizando choice; matches this file''s `transferProgress.stageScanning` = "Analizando". Source
  uses a Unicode ellipsis (U+2026), so the value does too · high
- "This folder doesn''t exist yet" (destination-not-found warning) → Esta carpeta todavía no existe · "carpeta" = folder
  (glossary); "todavía no existe" for "doesn''t exist yet" (macOS uses both "aún no" e.g. "iCloud aún no tiene…" and "ya
  no existe" for the negative-existence sense; "todavía no" reads natural and friendly) · high
- "Cmdr will create it during the copy/move" (same warning, op-specific) → Cmdr la creará durante la copia / Cmdr la
  creará durante el movimiento · "crear la carpeta" = create the folder (macOS Finder "Crear una carpeta llamada…", "No
  se ha podido crear la carpeta"); "la creará" agrees with fem. "carpeta"; "durante la copia" / "durante el movimiento"
  reuse the settled copy→Copia / move→Movimiento nouns (transfer-toast pattern). Two literal sentences, no ICU select,
  per the op-specific keys · high
- **queue.row.label progress arms (rename / create folder / create file)** · `Renombrando` / `Creando carpeta` /
  `Creando archivo` · gerund progress style of the sibling arms; Nautilus ("Renombrando", "Creando"), settled
  `carpeta`/`archivo` · high

### Settled during the archive-browsing pass (browse into zip/tar/7z + app bundles; Enter-behavior settings + read-only/delete warnings + viewer errors; macOS Finder/ArchiveUtility + Total Commander + MS terminology, 2026-07-05)

- **archive (noun: a zip/tar/7z browsed like a folder) → archivo comprimido** · macOS ArchiveUtility/Finder ("Zip
  archive"→"Archivo comprimido Zip", "%[Kind]@ is %[archives]@"→"archivo comprimido", "Apple Archive"→"archivo
  comprimido de Apple") + Total Commander ("Propiedades del archivo comprimido", "Comprobar (archivo comprimido)"). Two
  Tier-1/orthodox sources agree. NOTE the catalog collision: "file" is already `archivo` (glossary), so an archive is
  the qualified `archivo comprimido`, never bare `archivo`. Reads for all three formats (zip/tar/7z). TC also shows the
  Spain-only "fichero comprimido", rejected per the `archivo`-not-`fichero` style rule · confirmed (two authoritative
  sources)
- **app bundle → paquete** (Cmdr's "App bundles" card/section → **Paquetes de apps**) · macOS ("Show Package Contents"→
  "Mostrar contenido del paquete"); a .app/.bundle/.framework is a "paquete" in macOS Spanish. "de apps" uses the
  settled casual `app` (glossary) · high
- **browse (step inside an archive/bundle, list like a folder) → explorar** · MS terminology ("Browse"→"Explorar";
  "browse mode"→"modo de exploración") + Nautilus ("explorar el sistema de archivos"). Segmented-control cell "Browse"→
  "Explorar" (single word, fits the tight cell); "Browse like a folder"→"Explorar como una carpeta" · high
- **Open / Ask (segmented-control cells) → Abrir / Preguntar** · macOS ("Abrir"); "Preguntar" for the ask-each-time
  option (standard) · high
- **Enter (key name) → Intro** · reaffirms the existing glossary/style entry (Apple HW convention); "pulsar Intro"
  (press → pulsar) · tentative
- **encrypted → cifrado** · CHOSEN over the pile's only hit "Encriptado" (a single stale FileVault/disk-burning string
  in `es/macOS/`): "cifrado" is the RAE-preferred term and what current macOS uses broadly for data encryption, and
  reads more professional in a file-manager error. Flagged for review given the pile conflict · tentative
- **damaged → dañado** · macOS Finder ("...no puede abrirse porque está dañado") · high
- **extract (pull files out of an archive) → extraer** · standard; chosen over Total Commander's compress-specific
  "descomprimir" because tar isn't compressed, so "extraer" fits zip/tar/7z generically ("Cmdr explora y extrae...") ·
  high
- **preview (verb, in the Visor) → previsualizar** · standard; noun stays "vista previa" (glossary); "demasiado grande
  para previsualizarlo" · high
- **configure → configurar** · standard/MS; keeps the trailing "…" (settings-window signal) · high
- **"for good" (permanent delete, colloquial) → para siempre** · warmer colloquial match for "for good" over the formal
  "permanentemente"; fits the delete-warning banner · high
- **archive delete-warning halves** · Strong "Dentro de un archivo comprimido no hay papelera." + Rest "Estos elementos
  se eliminarán del zip para siempre." · phrased so the two concatenate naturally; "items"→"elementos" to match the
  sibling `fileOperations.json` (which uses "elementos", not macOS's "ítems") · high
- **queue.row.label `archive_edit` arm → Editando archivo comprimido** · gerund progress style of the sibling arms
  (Copiando/Moviendo); "Editing archive" = changing a zip's entries; edit→editar (glossary) + archive→archivo comprimido
  · high

### Settled during the paste-clipboard-as-file pass (⌘V pastes text/image/PDF from the clipboard as a new file; Behavior > file-ops settings + paste-confirm toast; macOS Finder/AppKit + Double Commander, 2026-07-07)

- **clipboard content → contenido del portapapeles** · macOS Finder exact string ("Contenido del portapapeles: ^0");
  reuses settled clipboard→portapapeles. "Paste clipboard content as a file" → "Pegar el contenido del portapapeles como
  archivo" (paste→pegar, glossary; "como archivo" drops the article, natural in es) · high
- **do nothing (radio-option label) → No hacer nada** · Double Commander es (orthodox two-pane, exact concept: "Do
  nothing"→"No hacer nada"); matches the infinitive option-label convention · high
- **Create file (paste option) → Crear archivo** · reuses `fileExplorer.functionKeyBar.newFileAction` = "Crear archivo"
  (create→crear + file→archivo) for cross-catalog consistency · high
- **Create and rename (paste option) → Crear y renombrar** · composed from create→crear + rename→renombrar (glossary) ·
  high
- **paste-confirm toast (`Pasted clipboard {X} as {filename}`) → "Se pegó {X} del portapapeles como {filename}"** · the
  ICU select fills X with the article+noun so it agrees ("la imagen"/"el PDF"/"el texto"); impersonal "Se pegó"
  (preterite) matches the settled transfer-toast "Se movió" pattern and avoids gendering the user; "como {filename}"
  reads correctly for any generated name · high

### Settled during the archive-password dialog pass (encrypted-zip unlock modal, `fileOperations.archivePassword.*`; macOS AppKit + Total/Double Commander es, 2026-07-08)

- password-protected → `protegido con contraseña` · TC/DC es phrasing · high. Body: "… está protegido con contraseña."
- password (noun) → `Contraseña` · macOS/MS es · high.
- unlock (button + verb) → `Desbloquear` · macOS AppKit ("Desbloquear") · high. Verb form "desbloquearlo".
- archive (the `{name}` head / input label) → `archivo comprimido` · settled es glossary · high. Input aria-label
  "Contraseña del archivo comprimido".

Settled while translating the Compress feature:

- compress (verb / control label) → `Comprimir` · Finder `es/macOS` ("Comprimir", `Compress ${sources}` → "Comprimir
  ${sources}") · high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb
  branches.
- compressing (progress -ing form) → `Comprimiendo` · derived on the sibling `Copiando`/`Moviendo` gerunds · high. Used
  in `titleActive`, `stageActive`; `scanTitleCompress` = "Verificando antes de comprimir...".
- compressed (result toast) → `Se comprimió` / plural `Se comprimieron` · mirrors `transfer.split.clean` ("Se copió:
  {phrase}") and the `one`/`many`/`other` shape of `fileOnly.allDone` · high.
- replace (overwrite warning) → `reemplazará` · Finder `Replace` → "Reemplazar" · high.
- archive (name) → rendered as `archivo` (the zip is a file; avoids the archivo≈file ambiguity of "archivo comprimido")
  · high. `.zip` in straight double quotes.
- compression level (slider label) → `Nivel de compresión` · TC `es` "Compresión ZIP interno (0-9)"; standard 7-Zip term
  `Nivel de compresión` · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Más rápido` · TC `es` "compresión más rápida (1)" · high. Marks quicker packing,
  not app speed. `.faster`.
- smaller (slider high end, level 9) → `Más pequeño` · pairs with `Más rápido`; marks the smaller output file (TC `es`
  high end "compresión máxima") · high. `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

### Settled during the Operation log pass (`operationLog.json` + `commands.logOperationLog.*`; alpha dialog listing recent file operations with rollback; macOS + Double/Total Commander + MS terminology, 2026-07-10)

- **operation log (dialog title / command label) → Registro de operaciones** · "log" → "registro" (MS "Event log" →
  "registro de eventos"; matches the settled Logging Settings section → Registro and changelog → registro de cambios).
  "operation" → "operación". Used for `operationLog.dialog.title` and `commands.logOperationLog.label` (shared
  sourceHash `2c97965`) · high
- **history (the record shown) → historial** · macOS ("NSToolbarHistoryTemplate" → "historial", "version history" →
  "historial de versiones"). Used in the command description "Consulta el historial de tus operaciones de archivos…" and
  the load-error string · high
- **roll back / rollback → revertir (verb) / reversión (noun)** · REAFFIRMS the settled fileOperations glossary entry;
  the catalog already uses "Revertir" (`transferProgress.conflictRollback`) and "La reversión"
  (`rollbackUnavailableTooltip`). So: "Can roll back" → "Se puede revertir", "Can''t roll back" → "No se puede
  revertir", "Rolling back" → "Revirtiendo" (gerund), "Rolled back" → "Revertido", "Partly rolled back" → "Revertido en
  parte". Command "roll them back" → imperative "reviértelas" · high (consistency-settled)
- **item (in this dialog) → elemento** · matches the sibling `fileOperations.json` "elementos" (not macOS "ítems"), per
  the archive-pass note; used across the summary plurals and the item-list strings · high
- **operation-summary lines (past-tense impersonal) → "Se {verb-preterite} {countText} elemento(s)"** · mirrors the
  settled transfer-toast pattern ("Se movió"/"Se movieron", "Se copió"/"Se comprimió"). copy→Se copió/copiaron, move→Se
  movió/movieron, delete→Se eliminó/eliminaron, rename→Se renombró/renombraron, createFolder→Se creó/crearon carpeta(s),
  createFile→Se creó/crearon archivo(s), compress→Se comprimió/comprimieron, trash→Se movió/movieron … a la papelera.
  archiveEdit "Edited an archive" → "Se editó un archivo comprimido"; archiveExtract → "Se extrajo un archivo
  comprimido" (archive→archivo comprimido, extract→extraer, glossary) · high
- **lifecycle status badges → match `queue.row.status`** · queued→Esperando, running→En ejecución, done→Hecho,
  canceled→Cancelado (queue uses `cancelled {Cancelado}`) · high
- **"Didn''t finish" (failed status/outcome, no-bare-"failed" voice) → No se completó** · literal neutral rendering of
  "Didn''t finish"; calm, avoids "Falló". Close cousin of the queue''s "No se pudo completar" but shorter and matches
  the source''s "didn''t" framing. Used for both `status.failed` and `outcome.failed` (shared sourceHash `59ea57b`) ·
  high
- **initiator provenance labels → Tú / Cliente de IA / Agente** · "You"→"Tú" (direct-address, no gendered noun); "AI
  client"→"Cliente de IA" (AI→IA, glossary); "Agent"→"Agente" · high
- No `sameAsSourceJustification` needed: every value differs from English.

### Settled during the Ask Cmdr pass (`askCmdr.json` + `settings.askCmdr.*`/`settings.advanced.logLlmCalls.*`/

`settings.section.askCmdr` + `commands.askCmdrToggle.*`; read-only AI chat rail: rail UI, tool-call labels, error copy,
sessions, attachments, consent screen, cost footer, settings section; macOS AppKit/Finder + MS terminology greps,
2026-07-13)

- **chat (noun) → chat** · not in the pile (no chat feature in any of the five file managers or in Apple's macOS bundles
  here); kept as the settled Spanish tech loanword (RAE-recognized, universal in WhatsApp/Slack/Teams-style Spanish UI:
  "chat", plural "chats"). `askCmdr.threads.open`/`askCmdr.sessions.title` = "Chats" is genuinely identical to the
  English value for this reason, so both carry `sameAsSourceJustification` rather than a forced re-wording · tentative
  (no first-party source, but the loanword is uncontroversial)
- **message → mensaje** · MS terminology ("message"→"mensaje", id 79920/342318, all regions incl. ESP/419) · high
- **stop (button, halts an in-progress action) → Detener** · macOS AppKit FunctionKeyNames ("Stop"→"Detener") · high
- **attach (verb, include a file with a message) → adjuntar**; **attachment (noun) → adjunto** · MS terminology
  ("attach"→"adjuntar", id 16016/16017; "attachment"→"adjunto", id 16067/1815092), the message-attachment sense (not the
  disk-image "attach"→"exponer" sense, id 1080693/1080066, which is a different concept) · high
- **archive (verb, move a chat out of the active list, no delete) → archivar**; **archived → Archivado** · MS
  terminology, the move-to-storage sense ("archive"→"archivar", id 14239/2699136/1250398 across three separate entries;
  "Archived"→"Archivado", id 2110499/2265410). Distinct from the file-archive noun `archivo comprimido` (glossary,
  archive-browsing pass) — this is the chat-list action, not a zip · high
- **unarchive → Desarchivar** · composed; no direct pile hit, but "des-" is the established Spanish antonym prefix
  already used across the catalog (desactivar, desconectar, desbloquear) · tentative
- **quota → cuota** · MS terminology ("quota"→"cuota", id 1638643/1724756, all regions incl. ESP/419) · high
- **usage (spend/usage sense, not disk "utilización") → uso** · chosen over MS terminology's formal "utilización" (id
  607526/773199, mass-noun register); "uso" is the shorter, more natural word Cmdr's UI voice prefers and matches how
  macOS/iOS commonly label per-feature usage. `settings.askCmdr.spend.title` = "Spending" → **Gasto** (the money-spent
  framing, distinct from `askCmdr.cost.label` "This chat's usage" → "El uso de este chat", which uses "uso" for the
  token/usage sense) · tentative (usage), high (quota)
- **on-device (processing stays local, never leaves the Mac) → en el dispositivo** · macOS Finder ("Se conservará en el
  dispositivo", AXBADGE12/NE88.3.2, iCloud-optimize-storage sense, same "stays local" concept) · high
- **cost/coste → coste** (NOT "costo") · the `es` catalog already uses "coste" throughout (`onboarding.json` descCost,
  networking desc); peninsular spelling, kept for catalog consistency even though the style guide's
  LatAm-safe-vocabulary recommendation would lean "costo" — flag if David ever confirms a LatAm-primary audience · high
  (consistency), tentative (regional choice)
- **token (LLM unit of text, not the MS "security token" sense) → token / tokens** · kept as the industry-standard
  loanword; no macOS/MS/file-manager source covers the LLM sense (MS's only "token" hits are the security-credential
  sense, a different concept) — matches how Spanish-language AI products (ChatGPT, Claude apps) render it · tentative
- **"Not now" (decline/dismiss button) → Ahora no** · macOS AppKit Document ("Not Now"→"Ahora no") · high
- **"Try again?" (inline question, not a button) → ¿Lo intentas de nuevo?** · REAFFIRMS the dominant catalog pattern (5
  existing hits: `commands.handler.favoriteAddFailed`, `feedback.dialog.softFailure`,
  `onboarding.stepBeta.signup.failure`, `queryUi.dialog.aiTranslateFailedToast`) over the older, less common
  "¿Reintentar?" (2 hits in `fileExplorer.json`) · high (consistency-settled)
- **tool-call status lines (doing/done pairs, no subject) → gerund for "doing", impersonal "Se + preterite" for "done"**
  · the gerund-no-subject form reuses the settled `queue.row.label` progress-arm pattern (Copiando, Renombrando, and its
  literal `other` fallback "Trabajando" — reused verbatim for `askCmdr.tool.unknown.doing` "Working"); the past-tense
  "Se + preterite" form reuses the settled operation-log summary pattern ("Se copió", "Se encontraron", singular/plural
  verb agreement with the object). Applied across all seven `askCmdr.tool.*` pairs (appState, listDir, largestDirs,
  importantFolders, folderImportance, listVolumes, operationsList, operationsGet) · high (pattern), tentative (the
  specific verb choices: comprobar for "check", buscar for "find/search", consultar/ revisar for "look at")
- **"in settings"/"in Advanced settings" (generic pointer to the app's own Settings window, lowercase in English) → en
  Ajustes / en Ajustes avanzados** (capitalized) · REAFFIRMS the dominant catalog pattern (`onboarding.json`, `ai.json`,
  `crashReporter.json`, `whatsNew.json` all capitalize "Ajustes" even when the English source is lowercase generic
  "settings") · high (consistency-settled)
- **"Settings › AI" (settings cross-reference with the explicit › separator)** · kept the `›` character exactly as the
  en `@key` describes it ("a right-pointing angle separating the settings path"), rather than substituting the plain `>`
  used in older `ai.json`/`crashReporter.json` cross-references — the en source deliberately calls out this specific
  character for this key, so the translation preserves it verbatim · high
- No `sameAsSourceJustification` needed except `askCmdr.title`, `commands.askCmdrToggle.label`,
  `settings.section.askCmdr` (all "Ask Cmdr", the kept product name) and `askCmdr.threads.open`/
  `askCmdr.sessions.title` (both "Chats", the settled chat loanword).

### Settled during the network-drive image-indexing pass (`settings.mediaIndex.networkVolumes.*`/`alwaysIndex*` + `search.imageResults.networkOff`/`paused`; opting an SMB drive into background photo-content indexing + honest status lines; macOS Finder/AirDrop + Double/Total Commander + MS terminology, 2026-07-13)

- **network drive → unidad de red** · Double Commander es (orthodox two-pane, exact concept: "Connect to network
  drive"→"Conectar a unidad de red", "Disconnect from network drive"→"Desconectar de unidad de red") + Total Commander
  ("Unidad de Red") + MS terminology (id 84431 "unidad de red"). Reuses settled drive→unidad + network→red. Plural
  "unidades de red" · high
- **photo(s) → foto(s)** · macOS Finder/AirDrop ("Recibiendo ^0 fotos", "quiere enviarte una foto", "Abrir en Fotos").
  The warm user-facing status/help lines say "photo" and get "foto"; kept DISTINCT from image→imagen, which stays for
  the feature/label names (the en source makes the same photo-vs-image split deliberately). "photos indexed" → "fotos
  indexadas" (participle agrees with fem. fotos) · high
- **image indexing (feature/label name) → indexación de imágenes** · reuses index/indexing→índice/indexación
  (glossary) + image→imagen; used for the internal list label and the search opt-in pointer, kept parallel with the
  "Image search" card → "Búsqueda de imágenes" · high
- **opt into (indexing) → activar** · reuses enable→activar (glossary); "opted into background image indexing" → "activó
  la indexación de imágenes en segundo plano" (background→en segundo plano) · high
- **always-index (drive/folder) → indexar siempre** · the switch "Always index this drive" → "Indexar siempre esta
  unidad"; the internal list labels "Always-index drives/folders" → "Unidades/Carpetas para indexar siempre" (verb form,
  unambiguous over a noun like "indexación permanente") · high
- **paused, resumes when the drive reconnects → En pausa, se reanuda cuando vuelvas a conectar la unidad** · reuses
  paused-state→En pausa + resume→reanudar (queue-pass glossary); "cuando vuelvas a conectar" is macOS Finder's exact
  resume-on-reconnect phrasing (`Finder/LocalizableMerged.json`: "puedas reanudar en otro momento cuando vuelvas a
  conectar «^0»") · high
- **gently (reading over the network) → con cuidado** · composed; no direct pile hit. "reads photos over the network
  gently" → "lee las fotos a través de la red con cuidado". "while you''re not busy" restructured to the non-gendered
  "mientras no estás usando el Mac" (avoids the gendered "ocupado", per the gender rule) · tentative (gently), high
  (restructure)
- **photo archive (a rarely-browsed collection, NOT a zip) → colección de fotos** · chosen over "archivo de fotos" to
  avoid the archivo≈file / archivo comprimido≈zip collision (glossary); "colección" is warm and unambiguous for the
  NAS-archive case · high
- No `sameAsSourceJustification` needed: every value differs from English (SMB kept verbatim inside a translated
  sentence, per do-not-translate).

### Settled during the quality pass over the bulk-rename review + image-index scope + Ask Cmdr tool labels (`askCmdr.renameReview.*`, `askCmdr.tool.{imageFacts,searchPhotos,proposeRenamePlan}.*`, `fileExplorer.imageIndex.*`, `settings.mediaIndex.{scope,chosenFolders}.*`, `errors.listing.deviceReconnecting.*`; macOS Finder/AppKit + MS terminology greps, 2026-07-21)

- **rename (the noun: one proposed name change) → cambio de nombre** · macOS Finder es ("Undo Rename" → "Deshacer cambio
  de nombre", "Redo Rename" → "Rehacer cambio de nombre"; "El Finder quiere cambiar el nombre de ^0 ítems"). Spanish has
  no noun for "a rename", and macOS itself uses this nominal phrase, so the review UI says "cambio de nombre" for the
  row/plan noun while the VERB stays the settled `renombrar` · high
- **rename (the verb / action button) → renombrar** · macOS Finder ("Renombrar", "Renombrar ^0 ítems…") + the whole es
  catalog (`commands.fileRename.label`, `fileExplorer.functionKeyBar.rename*`, `operationLog.summary.rename`). So the
  counted primary button "Rename {n} files" → "Renombrar # archivo(s)", NOT the longer "Cambiar nombre de # archivos" ·
  high
- **allow / deny (per-row review buttons) → Permitir / Denegar** · MS terminology ("allow"→"permitir" id 1054938/1132447
  and the ProperNoun button "Allow"→"Permitir" id 184378/2507115; "deny"→"denegar" id 44527/44535 and "Deny"→"Denegar"
  id 2158845/2202645, all regions incl. ESP/419); macOS es has "Permitir de todos modos". Plurals: "Permitir todos" /
  "Denegar todos" (masc. pl. agreeing with "los cambios de nombre") · high
- **Current name / New name (rename-table column headers) → Nombre actual / Nombre nuevo** · macOS puts the adjective
  first in a field label ("Nuevo nombre para la imagen:"), but the two column headers are kept parallel in form so the
  table reads as a pair; both orders are correct Spanish · high (Nombre actual), tentative (Nombre nuevo, parallelism
  chosen over the macOS collocation)
- **rename cycle (A→B→A dependency loop) → ciclo de cambios de nombre**; badge "(cycle)" → "(ciclo)" · composed from the
  settled rename noun; no pile source names this concept · tentative
- **"(overwrite!)" badge → "(¡sobrescribir!)"** · overwrite→sobrescribir (glossary, macOS Finder); Spanish opens the
  exclamation with `¡`, so the badge carries both marks inside the parentheses · high
- **tool-call done label: always impersonal "Se + preterite"** · a bare preterite ("Preparó un plan…") reads as a
  third-person subject and breaks the pattern every sibling arm uses; `askCmdr.tool.proposeRenamePlan.done` is now "Se
  preparó un plan de cambio de nombre", parallel with "Se leyó", "Se buscó", "Se encontraron" · high
  (consistency-settled)
- **image-index status labels (status bar under a pane) → Imágenes indexadas / Imágenes indexadas automáticamente /
  Imágenes sin indexar / Imágenes excluidas / Indexando imágenes** · reuses index/indexing→índice/indexación +
  image→imagen (glossary); "sin indexar" is the natural negative state (matches
  `settings.mediaIndex.networkVolumes.notIndexedYet` = "Aún sin indexar") · high
- **indexing pass → pasada** · "on the next pass" → "en la siguiente pasada"; kept distinct from the drive-index
  scan→análisis (glossary), which names the full drive scan, not one incremental sweep · tentative
- **"Folders to index" (the chosen-folders list title) → Carpetas para indexar** · matches the settled
  `alwaysIndexFolders.label` = "Carpetas para indexar siempre", so the two lists read as siblings; the passive "Carpetas
  que se indexan" broke that parallel · high (consistency-settled)
- **remove (take a row off a list, NOT delete) → Quitar** · the es catalog already settles this
  (`fileExplorer.network.browser.removeHostConfirmButton` = "Quitar", `askCmdr.attachment.remove` = "Quitar adjunto",
  `shortcuts.section.removeShortcutTooltip` = "Quitar atajo"). macOS/MS both render "remove" as "eliminar", but
  `eliminar` is the settled DELETE verb, and this button explicitly does not delete anything, so the catalog's "Quitar"
  wins on unambiguity · high (consistency-settled; deliberate departure from macOS/MS Tier 1-2)
- **add (button that opens a picker) → Añadir <noun>…, no article** · macOS es drops the article in button labels
  ("Añadir personas", "Añadir contraseña", "Añadir a favoritos") and so does the catalog ("Añadir atajo"), so "Add a
  folder…" → "Añadir carpeta…". Note "Añadir" is the peninsular form (LatAm macOS says "Agregar"); kept per the style
  guide's peninsular-base decision · high
- **"still searchable" → se puede seguir buscando** · matches the sibling `settings.mediaIndex.progress.kept` ("todavía
  se puede buscar") and `reclaim.line` ("siguen disponibles para búsquedas"); the adjective "buscable" is not used
  anywhere in the catalog and reads unnatural · high (consistency-settled)
- **"whatever else you pick above" → elijas lo que elijas arriba** · the doubled-subjunctive concessive is the idiomatic
  Spanish rendering of "whatever you pick"; clearer than the flatter "sea cual sea la opción de arriba" · high
- **"might be slightly off" (folder sizes) → podrían no ser del todo exactos** · states what is inexact; "no coincidir
  del todo" left open what the sizes fail to match · high
- No `sameAsSourceJustification` needed anywhere in this pass: all 54 values differ from English.

### Settled during the image-index indicator pass (`fileExplorer.imageIndex.*` file/folder/drive badge tooltips + `settings.mediaIndex.showFileStatusIcons.*`; the small badges on image files/folders/drives showing image-search indexing state; MS terminology + catalog-consistency, 2026-07-22)

- **badge → insignia** · the es catalog already settles it: `settings.fileExplorer.git.showRepoChip.label` = "Mostrar la
  insignia del repositorio" and the alpha badges in `onboarding.stepBeta.openBeta` = "insignias". MS terminology offers
  "distintivo"/"insignia"/"notificación" for "badge"; the catalog's "insignia" wins on consistency. "status badges" →
  "insignias de estado" · high (consistency-settled)
- **indexed (participle, agrees with the counted noun) → indexada/indexadas** · reuses index/indexing→índice/indexación
  (glossary) + the fem. gender of imagen/foto. Matches the sibling `settings.mediaIndex.progress.ofTotal` ("imagen
  indexada" / "imágenes indexadas") and `indexing.enrich.progress`, so the folder/drive count tooltips read parallel to
  the existing progress lines. A single image file's tooltip is fem. sing. ("Indexada para la búsqueda de imágenes") ·
  high
- **image search (in a sentence) → búsqueda de imágenes** · reaffirms the "Image search" card → "Búsqueda de imágenes"
  (network-drive pass); lowercased inside a sentence per sentence-case · high
- **"Couldn''t be indexed" (no-bare-"failed"/"error" voice) → No se pudo indexar** · reuses the settled "No se pudo …"
  calm-failure pattern (errors pass); avoids "falló"/"error" · high
- **"Waiting to be indexed" → Esperando a ser indexada** · reuses the settled waiting state (queue pass: queued/waiting
  → Esperando); fem. agreement for the image · high
- **"still working" (drive still indexing) → aún en curso** · composed; calm progress phrasing, no personal subject,
  parallel with the settled `stageActive`/`titleActive` progress voice · tentative
- **folder/drive count tooltips: fold the agreeing participle/verb INTO the plural arms** · English wraps only
  `{image}/{images}` in the plural and keeps "indexed" outside, but Spanish "indexada"/"indexadas" (and "está"/"están"
  in `drive.done`) must agree with number, so the whole "imagen indexada"/"imágenes indexadas" clause lives inside each
  CLDR arm (one/many/other), mirroring `settings.mediaIndex.progress.ofTotal`. `{totalText}`/`{doneText}` stay inside
  every arm; `{total}` is the selector; `{done}` is unused (English doesn't use it either). "All N …" → definite "Todas
  las {totalText} …" in the plural arms, collapsing to "{totalText} imagen indexada" in the one arm · high
- No `sameAsSourceJustification` needed: every value differs from English.

### Settled during the image-index settings restructure + progress-UX pass (`settings.mediaIndex.cards.*`, `progressSummary.title`, `semanticSearch.label`, `clip.{notSupported,offButInstalled,deleteButton,deleting,deleteConfirmTitle,deleteConfirmBody,deleteFailed}`, `fileExplorer.imageIndex.file.indexing`; three card titles + Semantic search card + a file badge; catalog-consistency + macOS, 2026-07-22)

- **search by description (the semantic-search feature, phrased plainly) → búsqueda por descripción (noun) / Buscar
  fotos por descripción (label)** · reuses the settled catalog phrasing: `clip.ready` = "busca tus fotos por
  descripción" and `clip.description` = "describiendo lo que aparece en ellas". So the toggle "Search photos by
  description" → "Buscar fotos por descripción" (photo→foto, infinitive label), and the sentence-internal "search by
  description" → "la búsqueda por descripción" (fem., agrees "está desactivada"). Kept distinct from the card title
  "Semantic search" → "Búsqueda semántica" (`clip.title`) · high (consistency-settled)
- **Apple silicon → Apple silicon (kept verbatim)** · the en `@key` for `clip.notSupported` says "keep it" (Apple's own
  term for its M-series chips); Apple's Spanish keeps "Apple silicon" untranslated. "a Mac with Apple silicon" → "un Mac
  con Apple silicon" · high
- **Enable indexing (card title) → Activar la indexación** · enable→activar + index/indexing→indexación (glossary) ·
  high
- **Folders to index (card title) → Carpetas para indexar** · REAFFIRMS the settled entry (rename-review pass): matches
  `alwaysIndexFolders.label` = "Carpetas para indexar siempre" so the lists read as siblings · high
  (consistency-settled)
- **Indexing now → Indexando ahora (heading) / Indexándose ahora (single-file badge)** · the `progressSummary.title`
  heading takes the subjectless active "Indexando ahora" (parallel to the status-bar "Indexando imágenes"); the
  per-image badge `file.indexing` takes the reflexive fem. "Indexándose ahora", matching its fem-perspective sibling
  tooltips (`file.indexed` "Indexada…", `file.pending` "Esperando a ser indexada"). Same en source + sourceHash, two
  contexts · high
- **reclaim (disk space, on the delete button) → liberar** · reuses the settled free-space verb (`reclaim.freed` = "Se
  liberaron unos {size}", `reclaim.button` = "liberar unos {size}"). "Delete model (reclaim {size})" → "Eliminar modelo
  (liberar {size})" (article dropped to parallel `clip.download` = "Descargar modelo (~{sizeText} MB)"); "This frees
  {size}" → "Esto libera {size}" · high
- **delete model / removed (no-bare-"failed" voice) → eliminar el modelo / No se pudo eliminar** · delete→eliminar +
  model→modelo (glossary). "The model couldn''t be removed just now" → "No se pudo eliminar el modelo ahora mismo"
  (reuses the calm "No se pudo …" failure pattern); "Deleting…" → "Eliminando…" (Unicode ellipsis, matching
  `clip.downloading` = "Descargando…") · high
- **keyword / tag search → búsqueda por palabras clave / por etiquetas** · tag→etiqueta (`showTags` = "Mostrar
  etiquetas"); "Keyword and tag search keep working" → "La búsqueda por palabras clave y por etiquetas sigue
  funcionando" · high
- No `sameAsSourceJustification` needed: every value differs from English.

### Settled during the dialog-polish pass: delete-dialog trash switch + transfer From/To groups (`fileOperations.delete.trashSwitch`/`confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`; macOS Finder + Total/Double Commander, 2026-07-23)

- "Move to trash" (switch in the delete dialog, on = trash, off = permanent delete) → Mover a la papelera · identical to
  every sibling trash string in this file (`transferDialog.titleVerbOnly`'s `other {Mover a la papelera}`,
  `transfer.trash`) and to the settled `move → mover`. macOS Finder's own menu item is "Trasladar a la papelera" (Finder
  AL13/N153); not taken, so the catalog keeps ONE move verb · high
- "Delete" (destructive confirm button while the switch is off) → Eliminar · settled delete verb, identical to
  `transferDialog.titleVerbOnly`'s `delete {Eliminar}` arm · high
- "From" / "To" (headings over the source path and over the destination volume + path) → Desde / Hacia · Double
  Commander es ships this exact pair as the copy/move dialog's field labels ("Desde:"/"Hacia:"); the directional "hacia"
  is the partner "desde" asks for, where a bare "A" would read as a stray single letter above a group. Total Commander
  es (`662="DE:  "`, `663="EN: "`) rejected: uppercase, and "EN" is a locative, not a destination. The settled nouns
  origen / destino stay for the destination CONTROLS ("Volumen de destino", "Ruta de destino"); the headings take the
  light prepositional pair the English uses · high

### Settled during the master-switch-off review pass (`fileExplorer.navigation.driveIndex.{refusedIndexingOff,tooltipIndexingOff,menuIndexingOffNote}` + `settings.indexing.{masterOffNote,overriddenBadge}`; the copy shown while the MASTER drive-indexing switch is off; MS terminology + catalog-consistency, 2026-07-27)

- **drive indexing (the master switch / the concept) → la indexación de unidades** · REAFFIRMS the settled
  index/indexing→índice/indexación entry + drive→unidad. It also quotes the catalog verbatim:
  `settings.section.indexing` = "Indexación", `settings.section.driveIndexing` = `settings.indexing.enabled.label` =
  "Indexación de unidades", so the navigation path in the new strings reads "Indexación > Indexación de unidades"
  exactly as the sidebar does. MS terminology backs the root ("content indexing" → "indexación de contenido" id
  361626/2026484; "Indexing Service" → "servicio de indexación" id 65942/2141628; "index" noun → "índice"). Never
  "indización" (1 pile hit vs 9 for "indexación") · high (consistency-settled)
- **"stays unindexed" → sigue sin indexar** · the `sin + infinitive` passive-adjectival ("el problema sigue sin
  resolver") matches the settled negative state `Imágenes sin indexar` / `Aún sin indexar`, and it carries NO gender or
  number agreement, so it survives any `{name}` a drive can have · high
- **"picks up where it left off" → continuará donde lo dejó** · NOT "seguirá donde lo dejó": bare `seguir` + a place
  adverbial reads as "stay put" ("seguirá donde estaba"), which garden-paths a resumption promise. `continuar` carries
  the resumption sense unambiguously. Kept distinct from the settled resume→`reanudar` (macOS Tier 1), which names the
  pause/resume ACTION on a transfer, not an implicit "carry on from the saved progress" · high
- **A settings path followed by an `y` clause takes a comma** · "…en Indexación > Indexación de unidades, y esta
  unidad…". Without it, "unidades y esta unidad" reads as a two-item list. Spanish normally drops the comma before `y`,
  but the source has one and the ambiguity here earns it · high
- **"Each drive keeps its own on or off choice" → Cada unidad recuerda si estaba activada o desactivada** · a literal
  "su propia elección de activada o desactivada" is unidiomatic (the `de` + participle pair has nothing to agree with).
  A subordinate `si`-clause lets `activada`/`desactivada` agree with fem. `unidad` naturally. The trailing "ready for
  when…" stays an appositive that also agrees: "lista para cuando vuelvas a activar la indexación" (naming
  `la indexación` rather than a bare `esto`/`la`, whose nearest antecedent would wrongly be `unidad`) · high
- **`overriddenBadge` "Off with drive indexing" → Desactivado con la indexación** · kept: masc. `Desactivado` matches
  the catalog's off-state labels (`settings.ai.provider.opt.off`), and the badge only renders inside the
  `Indexación de unidades` page, so the short `la indexación` is unambiguous there. "de unidades" is dropped for length
  (badges are inline chips next to a row label; 29 chars vs the source's 23) · high (kept), tentative (the comitative
  `con`, which mirrors what fr/pt/nl/sv all chose)
- No `sameAsSourceJustification` needed: all five values differ from English.

## Índice de unidades: la pasada de comprobación de cambios (2026-07-28)

- **"Checking for changes" (run-kind header) → `Comprobación de cambios`** · nominal phrase matching the sibling headers
  (`Primer análisis completo`, `Actualización rápida`); `Comprobando` is macOS ES's checking verb (Finder BN9
  "Comprobando si los contenidos…"), `cambios` is catalog-settled (`los cambios recientes`) · high.
- **"Update the file list" → `Actualizar la lista de archivos`** · composed from the settled siblings
  `Guardar la lista de archivos` + `Actualizar el índice` · high.
- **"the check running right now" → `el análisis que se está ejecutando ahora mismo`** · reuses `análisis` as this
  catalog's settled word for a full check (`tooltipCoalesced`: "el próximo análisis completo de Cmdr") and that string's
  closing `lo dejará al día` · high.

## Transferencia atascada: el aviso de "sin progreso" (2026-07-31)

Settled while translating the seven `fileOperations.transferProgress.stall*` strings plus `queue.row.stalled` (the
notice that replaces the ETA countdown when a copy/move stops moving). Mined from macOS Finder/AppKit, MS terminology,
Nautilus, and Total/Double Commander.

- **"No progress for {duration}" → `Sin progreso desde hace {duration}`** · `progress`→`progreso` is MS terminology (id
  2371066/2375015) and matches the catalog's own `Progreso del tamaño` / `Progreso de archivos`. The "for X (up to now)"
  sense REQUIRES `desde hace`, never `durante` (which names a finished span) nor a bare `hace` · high. Runner-up
  `Sin avances desde hace…` reads equally natural; `progreso` won on catalog consistency. Rejected
  `Detenida desde hace…` (shorter, but collides with the queue's own paused state `En pausa`).
- **"Waiting for X to respond" → `Esperando a que X responda`** · macOS Finder's own waiting sentences take exactly this
  shape: `Esperando a que “^0” acepte…`, `Esperando a que se complete la transferencia con “^0”…` (`es/macOS/Finder/`,
  2026-07-31); `respond`→`responder` is MS terminology. `esperando a que` + subjunctive, NOT `esperando por` (a calque)
  · high. Double Commander's `Esperando la respuesta del usuario` is the nominal variant; the verbal one keeps Cmdr's
  sentences short.
- **source / destination (the two ends of a transfer) → `el origen` / `el destino`** · reaffirms the settled pair; MS
  terminology (`source`→`origen`, `destination`→`destino`) plus Total Commander (`¡Origen y destino diferentes!`) and
  Nautilus (`la carpeta de destino` / `la carpeta origen`). Both take the definite article here because the dialog has
  exactly one of each · high.
- **"has stopped moving" (a stalled transfer) → `ha dejado de avanzar`** · says the transfer stopped ADVANCING without
  claiming it stopped or that anything went wrong, so it stays inside Cmdr's no-"error"/no-"fallo" rule. Rejected
  `se ha detenido` / `se ha quedado parada`: both read as "paused", which the queue already labels `En pausa` · high.
- **"leave it running in the background" → `déjala en ejecución en segundo plano`** · quotes the catalog's own
  `queueTooltip` ("Mantenla en ejecución en segundo plano…") and `backgroundedToast` ("Sigue ejecutándose en segundo
  plano."); `background`→`segundo plano` is MS/macOS/TC standard · high. The clitic `-la` agrees with fem.
  `transferencia`.
- **"partly written" (a file with data already on disk) → `parcialmente escrito`** · Nautilus's parallel
  `¿Quieres eliminar el archivo parcialmente copiado?` is the model; the adverb goes before the participle · high.
- **"# file is still open" → `# archivo sigue abierto`** · `seguir` + adjective is the natural "still be X"; avoids the
  heavier `Todavía hay # archivo abierto` · high.
- **"The log has the details." → `El registro tiene los detalles.`** · quotes the catalog's existing
  `askCmdr.renameUndo.refusedBatches` ("El registro de operaciones tiene los detalles."); `log`→`registro` is MS
  terminology · high.
- **`Close` (the button that closes the progress dialog while the transfer finishes) → `Cerrar`** · reaffirms
  close→cerrar (macOS AppKit); sits next to `Cancelar`, and the two are unmistakable in Spanish · high.
- **Bake the whole sentence into the ICU plural branches when the tail agrees with the count.** `stallInFlight`'s
  English keeps "and may already be partly written" OUTSIDE the plural; Spanish can't, because `esté`/`estén` and
  `escrito`/`escritos` agree with the counted noun. The es value is one plural block whose branches each carry the full
  sentence. Same rule as the transfer-toast verb-agreement entry above · high.
- No `sameAsSourceJustification` needed: all eight values differ from English.

## Ruta copiada: la confirmación del portapapeles (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Una clave: la línea del aviso informativo tras ⌘⌥C. La ruta va debajo, en su propia línea monoespaciada, así que NO es
un marcador dentro de la frase: la frase acaba en dos puntos y tiene que sostenerse sin la ruta.

- **"Copied the path, it's now on your clipboard:" → `Ruta copiada, ya está en el portapapeles:`** · reutiliza
  `path → ruta` y `clipboard → portapapeles` del glosario (macOS "Portapapeles") · high. El participio inicial sigue el
  patrón de los avisos hermanos (`{countText} ítems copiados`). Sin posesivo (`tu portapapeles`): solo hay uno y macOS
  usa siempre el artículo.
- No hace falta `sameAsSourceJustification`: el valor difiere del inglés.

## Cola de operaciones: el cambio de nombre de la ventana (2026-08-08)

The English source widened from **"Transfer queue"** to **"Operation queue"** across 14 keys (`queue.windowTitle`,
`queue.heading`, the four `queue.row.*Aria`, `queue.list.aria`, `commands.queueShow.label`/`.description`, and the five
`fileOperations.transferProgress.queue*`/`backgroundedToast` keys). The window lists deletes, trashes, renames, folder
and file creations, and archive edits, not only transfers, so the narrow word was wrong on the facts; "transfer" also
already means copy-or-move one level down (the transfer progress dialog, the transfer driver). `es` had to widen the
same way, which is why a hash restamp was not an option.

- **operation (the category: a copy, move, delete, trash, rename, folder/file creation, or archive edit) → operación** ·
  macOS Finder Tier 1, in exactly this sense: NE82 "another operation is in progress, such as moving or copying an item
  or emptying the Trash" → "hay otra operación en curso, como trasladar o copiar un ítem o vaciar la papelera"; NE83
  "the current operation" → "la operación actual". Double Commander (orthodox two-pane) agrees throughout: "Current
  operation:" → "Operación actual:", "File operations" → "Operaciones con archivos", "Executing operations" → "Ejecución
  de operaciones", "operations panel" → "panel de operaciones"; Total Commander "operaciones activas en segundo plano".
  The es catalog had already settled it (30+ hits, incl. `operationLog.dialog.title` = "Registro de operaciones" and
  `settings.navigationAndFileOps.card.fileOperations` = "Operaciones de archivos") · high
- **operation queue (the window name) → Cola de operaciones** · `cola` = queue is unchanged from the June queue pass
  (Double Commander "New queue" → "Cola nueva", "Add To Queue" → "Añadir a cola"; macOS print "cola"), and the catalog
  already says `queue.empty.title` = "No hay nada en la cola". Composed with the settled `operación` · high. Supersedes
  **"Cola de transferencias"** (June queue pass), which now names the wrong scope.
- **The View-menu pair stays parallel.** "Operation queue" / "Operation log" → **Cola de operaciones** / **Registro de
  operaciones**: same head noun, differing only in `cola` (present, running now) vs `registro` (past, already ran),
  exactly as the English pair does · high
- **"this operation" (the per-row aria labels) → esta operación** · macOS Finder Tier 1, verbatim: CS203 "Authentication
  needed to complete this operation." → "Es necesario autenticarse para completar esta operación.", plus CS205/CS207. So
  Pausar / Reanudar / Cancelar / Seleccionar + "esta operación", reusing the settled pause→Pausar, resume→Reanudar,
  cancel→Cancelar, select→seleccionar · high
- **`commands.queueShow.label` is now the bare window title, not a "Mostrar…" phrase** · the English label changed from
  a "Show …" command to plain "Operation queue" so the palette entry, the View menu item, and the window title read
  identically. The es value follows: "Cola de operaciones", NOT "Mostrar la cola de operaciones" · high
- **The feminine head noun keeps every clitic and participle that already agreed.** `transferencia` → `operación` are
  both feminine singular, so `queuedToast`'s "por delante de esta … La encontrarás", `queueTooltip`'s "Mantenla …
  gestiónala", and `queueShow.description`'s "pausarlas, reanudarlas o cancelarlas" all stayed correct unchanged; only
  the noun and the window name moved. Worth knowing if the source ever widens again to a masculine concept · high
- **`queuedToastCount` keeps its three CLDR branches**: `one {# operación} many {# operaciones} other {# operaciones}` ·
  high
- `transfer` → `transferencia` is NOT retired: it still names the copy-or-move operation itself (the progress dialog,
  `transfer.*`, the stall copy). Only the QUEUE's name widened · high
- No `sameAsSourceJustification` needed: all 14 values differ from English.

## El chip de la esquina y el aviso de operación sin terminar (2026-08-08)

Nine new keys: the corner progress chip (`queue.chip.*`), the failure notice (`queue.failureToast.*`), and the Dismiss
buttons (`queue.row.dismiss`/`.dismissAria`, `queue.toolbar.dismissAll`). The window name, the head noun `operación`,
and its feminine agreement come from § Cola de operaciones: el cambio de nombre de la ventana; don't re-derive them.

- **dismiss (take a finished-badly row off the list; nothing is undone, retried, or deleted) → Descartar** · REAFFIRMS
  the style-guide entry and the catalog's nine existing hits (`crashReporter.dialog.dismiss`, `downloads.empty.dismiss`,
  `errorReporter.sentToast.dismiss`, `lowDiskSpace.toast.closeTooltip`, `ui.toast.dismissAria` = "Descartar
  notificación", …). Sourced from macOS AppKit `Document.json` ("Discard" → "Descartar") and MS terminology ("dismiss" →
  "descartar", id 780443/1053425, all regions incl. ESP/419; its second sense "ignorar" is the ignore-a-warning sense,
  not this one). NOTE the near-miss: the settled `remove → Quitar` (rename-review pass) also takes a row off a list, but
  `Quitar` is the catalog's word for editing a list the user built (a favourite, an attachment, a shortcut); this button
  clears a NOTICE, which is `Descartar`'s job everywhere else in the catalog · high (consistency-settled)
- **"Dismiss all" (toolbar) → Descartar todo** · parallel with the settled `Pausar todo` / `Reanudar todo`, so the three
  toolbar buttons read as one family. NOT "Descartar todas" (which would agree with `operaciones` and break the
  parallel) · high
- **"Dismiss this operation" (per-row aria) → Descartar esta operación** · slots into the existing per-row aria family
  verbatim (`Pausar` / `Reanudar` / `Cancelar` / `Seleccionar` + "esta operación") · high
- **"Couldn't finish <action>" (the toast title's nine arms) → "No se pudo completar" + the operation NOUN** · the head
  is `queue.row.status`'s `failed` arm verbatim, so the toast and the row can't word the same fact two ways, and the
  `other` arm is byte-identical to it. The nouns are the catalog's own, not composed: `errors.*` already ships "Se
  desconectó el dispositivo durante **la copia** / **el movimiento** / **el movimiento a la papelera** / **la
  eliminación**", and `queue.empty.body` says "Las copias, los movimientos y las **eliminaciones**". So copy → la copia,
  move → el movimiento, trash → el movimiento a la papelera, delete → la eliminación, rename → **el cambio de nombre**
  (the settled rename NOUN, macOS "Deshacer cambio de nombre"), create_folder/create_file → la creación de la carpeta /
  del archivo (composed; macOS has only "Fecha de creación"), archive_edit → **la edición del archivo comprimido**
  (macOS "Deshacer edición de etiquetas" is the `edición de X` model; `archivo comprimido` is settled) · high (the six
  sourced arms), tentative (the two `creación` arms)
- **❌ Rejected for those arms: "No se pudo terminar de copiar".** It's fluent and shorter, but `terminar` is a second
  verb where the row says `completar`, which is exactly the two-renderings-of-one-concept defect this family exists to
  avoid · high
- **Counted "N operations couldn't finish" → verb-first "No se pudo(-ieron) completar {countText} operación(es)"** ·
  leads with the same house wording as the row and the toast title, and follows the settled counted-toast shape (the
  whole clause lives inside each CLDR arm so the verb agrees: "Se movió" / "Se movieron"). Used identically by
  `failureToast.summary` and the first sentence of `chip.failed` · high
- **"Show in operation queue" (the toast's button) → Mostrar en la cola de operaciones** · `show → mostrar` (glossary)
  in the catalog's own `Mostrar en el Finder` shape, with the window name unchanged so the button and the window title
  match · high
- **"Open the operation queue …" (the promise both spoken labels end on) → Abre la cola de operaciones …** · `tú`
  imperative, matching the catalog's ~20 "Abre …" suggestions; "to see why" → "para ver por qué" (the catalog's "para
  ver …" pattern) · high
- **"percent", spelled as a word for the screen reader → por ciento** · no pile hit (neither macOS nor MS terminology
  has a `percent` entry), but `por ciento` is the standard written-out form and is what Spanish VoiceOver says for `%`
  anyway, so spelling it keeps English's intent without risking an odd reading · tentative (unsourced, uncontroversial)
- **The `%` SIGN keeps English's spacing: `{percentText}%`, no space** · the es catalog is 10-to-1 on this
  (`Zoom al 100%`, `indexing.progress.percentEta`, `lowDiskSpace.toast.message`); the one outlier is
  `fileExplorer.summary.percentSelectedIn` ("({percent} %)"). Unlike de/fr/sv, Spanish has no hard space-before-`%`
  requirement in these sources, and the chip tooltip is width-tight · high (consistency-settled). Nearby defect worth a
  look: that outlier is the catalog's only spaced `%`.
- **"items" in the chip tooltip (files and folders alike) → elemento / elementos** · the catalog is 44-to-8 on
  `elemento(s)` over macOS's `ítem(s)`, and the sibling `fileOperations.json` + `operationLog.json` both use
  `elementos`; REAFFIRMS the archive-pass note · high (consistency-settled)
- **"to {destination}" in the chip tooltip → " a {destination}"** · Nautilus ships the literally-identical sentence
  ("Copying %'d files to “%s”" → "Copiando %'d archivos a «%s»"). macOS Finder's own copy string uses `en` ("Copiando ^0
  ítems **en** “^2”"), NOT taken: `{label}` here is generic across copy/move/trash, and "Moviendo … en Backup" reads
  locative rather than directional, while `a` works for all of them (macOS itself says "Trasladando ^0 ítems **a** la
  papelera"). Kept distinct from the `Desde` / `Hacia` dialog HEADINGS, which are field labels, not a running phrase ·
  high (deliberate departure from macOS Tier 1, on genericness grounds)
- **Every optional clause keeps its own leading space INSIDE the branch, and `=0 {}` / `other {}` stay empty.** Verified
  by assembling all four combinations plus the no-detail ones: no double space, no dangling `·`. Spanish needs no
  reordering here, so the English structure survives verbatim · high
- **The time-left `{detail}` is NOT translated in this file**: the chip fills it from
  `fileOperations.transferProgress.etaRemaining` = "Queda {duration}" (or from `queue.row.status`'s paused arm, "En
  pausa"). If that key ever changes shape, the tooltip's tail changes with it · high
- No `sameAsSourceJustification` needed: all nine values differ from English.

## El aviso de conflicto de la ventana principal (2026-08-09)

Two keys (`fileOperations.operationConflict.context` / `.pausedNote`), the line under the title "El archivo ya existe"
and the quiet note under the buttons.

- **The context line is `queue.row.label`'s gerund arm plus the chip's destination clause, nothing re-derived** ·
  `Copiando` / `Moviendo` / `Trabajando` come from `queue.row.label` verbatim, and ` a {destination}` from
  `queue.chip.tooltip`, so the chip, the row, and the prompt name one running operation the same way · high
  (consistency-settled)
- **`a {destination}` holds even with no direct object in the sentence** · macOS Finder's copy-progress pair is split
  (`CP4_V1` "Copiando “^1” **en** “^2”" vs `MV4_V1` "Trasladando “^1” **a** “^2”"), and Nautilus uses `a` for both
  ("Copiando %'d archivos **a** «%s»"). Taking `en` for the copy arm alone would make the copy sentence disagree with
  the chip tooltip for the same operation, so `a` wins for both arms, as it did in the chip · high. The residual: with
  no object in the slot, "Copiando a Ana" can momentarily read as personal-`a`. Rejected the safer "Copiando a la
  carpeta {destination}": `getFolderName()` can hand this a volume root, an SMB share name, or `/`, so "la carpeta"
  would over-claim · tentative (flagged for review)
- **"Working in {destination}" → `Trabajando en {destination}`** · English's `in` is locative here, not directional, and
  the arm covers operation kinds whose destination may not be a folder, so `en` stays and no noun is added · high
- **The two `archive_edit` arms stay different on purpose** · with a destination the arm names the zip itself
  (`Editando {destination}` → "Editando fotos.zip"); without one it says `Editando un archivo comprimido` (the article
  is what a sentence needs and the bare `queue.row.label` badge doesn't) · high
- **"Everything else is paused until you answer." → `Todo lo demás está en pausa hasta que respondas.`** · `en pausa` is
  `queue.row.status`'s `paused` arm verbatim, so the note and the rows it describes use one word for one state; "hasta
  que + subjunctive" matches the catalog's own `tú` pattern ("hasta que la elimines", "hasta que lo desbloquees"), and
  it promises the resume rather than warning about the stop · high

## El botón con la cola vacía: "Background" (2026-08-09)

Dos claves, `fileOperations.transferProgress.background` + `.backgroundAria`: el MISMO botón que `queue` / `queueAria`
del diálogo de progreso, en su otro estado. Con la cola de operaciones vacía no hay nada a lo que ponerse detrás, así
que el inglés cambia el sustantivo "Queue" por el verbo "Background" (un imperativo: "quita esta operación de en
medio"), no por el sustantivo del fondo de una imagen.

- **"Background" (el botón, estado de cola vacía) → `En segundo plano`** · el sentido "que se ejecuta sin estorbar" es
  `segundo plano` en MS terminology (entrada 16344_18758_18759, "background" adjetivo, definición "operating without
  interaction with the user while the user is working on another task" → `segundo plano`, todas las regiones) y ya está
  en este glosario (`background (run in the background) → en segundo plano`), además de en el propio catálogo
  (`queueTooltip`, `backgroundedToast`, la copia de `stall*`) · high
  - **macOS es (Tier 1) no aporta nada aquí**: en este volcado del corpus no hay ni una sola aparición de "segundo
    plano", y el telón de fondo visual de macOS es `Fondo` ("Fondo:", "color de fondo"). Eso es una ventaja del español
    frente al neerlandés y el sueco: `segundo plano` NUNCA puede leerse como el fondo de una ventana, porque ese
    sustantivo es `fondo`.
  - ❌ **No el sustantivo pelado `Segundo plano`, aunque Total Commander es lo lleve en ESTE mismo botón**
    (`WCMD.LNG.utf8` `{COMMON}`: `4001="Aceptar"`, `4002="Cancelar"`, `4003="Ayuda"`, **`4004="&Segundo plano"`**,
    `4005="Para &después"` = el botón Queue, `4006="Solo &Errores"`). El motivo es una colisión de la propia categoría:
    en los ajustes de color de Double Commander es, `Primer plano` / `Segundo plano` son el par de colores de texto y de
    fondo ("Color segundo plano:", "Segundo plano:"), y MS mantiene el mismo par ("primer plano", "color de primer
    plano"). La preposición desambigua y, de paso, aporta el verbo elidido: `En segundo plano` = "[déjala en] segundo
    plano". Los hermanos sueco y neerlandés llegaron a la misma forma preposicional (`I bakgrunden`,
    `Op de achtergrond`) por un camino distinto.
  - ❌ **Tampoco el infinitivo completo `Pasar a segundo plano` / `Continuar en segundo plano`.** Son mandatos
    impecables y encajan con la regla de estilo "botones en infinitivo", pero miden 21–25 caracteres en el botón que en
    su otro estado dice `Cola` (4), y la clave inglesa pide expresamente "short control label; must fit the same button
    as Queue". Guárdalos por si una revisión nativa encuentra `En segundo plano` demasiado elíptico.
  - **La regla "botones en infinitivo" cede aquí a propósito**: este par de estados ya vive fuera de ella, porque el
    hermano `queue` es el sustantivo `Cola`. Los dos botones nombran el DESTINO de la operación (la cola / el segundo
    plano), y así el cambio de estado no cambia de registro.
- **"Keep this running in the background" (la etiqueta para el lector de pantalla) →
  `Mantenerla en ejecución en segundo plano`** · calca la primera oración del propio `queueTooltip` del catálogo
  ("Mantenla en ejecución en segundo plano…") y adopta el infinitivo del hermano `queueAria` ("Enviar a la cola de
  operaciones"), que es el registro de las arias de este diálogo. El clítico `-la` concuerda con la femenina
  `la operación` · high
  - **WCAG 2.5.3 (Label in Name)**: la etiqueta visible `En segundo plano` va contenida en el aria como
    `…en ejecución **en segundo plano**`, verbatim salvo la mayúscula inicial. Es exactamente el listón del inglés
    ("Background" ⊂ "…in the background") y satisface el criterio, cuya comparación no distingue mayúsculas. ⚠️ Las dos
    claves son UNA unidad: si alguna vez se reescribe la etiqueta, hay que rehacer el aria para que siga conteniéndola.
  - La containment exacta (aria empezando por `En segundo plano`) obligaría a algo como "En segundo plano: mantenerla en
    ejecución", que rompe el paralelo de registro con `queueAria` y suena a título con subtítulo. Se descartó a
    conciencia.
- Ninguno de los dos valores lleva apóstrofo, así que no hay nada que duplicar para ICU; tampoco hay marcadores.
- No hace falta `sameAsSourceJustification`: los dos valores difieren del inglés.

## La salida con operaciones en curso (2026-08-10)

Seven keys (`main.quit.*`): the modal that intercepts a quit (⌘Q, the menu, closing the main window) while a copy, move,
delete, trash, or archive edit is still running. Title + body + live countdown + a list heading + two buttons.

- **quit (the USER's action, in a title or a button) → salir** · macOS Tier 1, in exactly this concept: Finder A17 "The
  Finder can't quit because some operations are still in progress." → "No se puede salir del Finder porque hay
  operaciones en curso."; AppKit `Quit` → "Salir", `Quit Anyway` → "Salir", `Quit and Close All Windows` → "Salir y
  cerrar todas las ventanas". The catalog already settles it too (`commands.appQuit.label` = "Salir de Cmdr") · high
- **quit (the APP as subject, ending itself) → cerrarse** · a Spanish app doesn't "sale", it "se cierra": macOS Finder
  BN36/BN23 "The Finder is about to quit." → "El Finder está a punto de cerrarse." So the countdown says "Cmdr se
  cerrará …" while the title and the button keep the user-action "Salir". ⚠️ This is NOT a collision with the crash
  phrase "se cerró inesperadamente" (glossary): there the crash sense is carried by "inesperadamente", not by "cerrarse"
  · high
- **"still running" (said of operations) → en curso** · macOS Finder Tier 1 in this exact sense (A17 above; NE82
  "another operation is in progress" → "hay otra operación en curso"), and Double Commander (orthodox two-pane) agrees
  ("Show operations progress initially in" → "Mostrar inicialmente las operaciones en curso en"). The heading takes "Aún
  en curso" (carrying the "still"), and the title reuses the same two words, so the dialog names one state one way ·
  high
  - Deliberately NOT `queue.row.status`'s `running` arm "En ejecución": that is the per-row status BADGE, while the
    title and heading are prose about the same fact, and "en curso" is what macOS writes in prose. Worth knowing if a
    later consistency pass tries to merge the two.
- **The title takes the catalog's own infinitive-question shape** · "¿Salir mientras hay {countText} operaciones en
  curso?" follows the settled dialog-title pattern ("¿Eliminar el modelo de IA?", "¿Enviar informe de fallos?", "¿Copiar
  {size} al portapapeles?", "¿Indexar {name}?") rather than a macOS-style "¿Seguro que quieres…?" · high
  (consistency-settled). All three CLDR arms carry the WHOLE sentence, because "una operación" / "operaciones" sits
  inside it; the `one` arm says "una operación" and skips `{countText}`, exactly as English does.
- **"Keep working" (the button that calls the quit off) → Seguir trabajando** · composed: no source in the pile names
  this button (Total Commander's equivalent prompt is the formal "¿Está seguro de querer salir?" with plain yes/no,
  Nautilus, Thunar, Dolphin, and Double Commander have no such dialog). "Seguir" is the catalog's own carry-on verb
  ("sigue funcionando", "se puede seguir buscando"), the infinitive matches the button-label convention, and it carries
  no hint of postponing. ❌ Rejected "Más tarde" (the settled DEFER label, precisely the wrong sense here) and a bare
  "Cancelar", which next to a list of running operations would read as cancelling THEM · tentative (unsourced wording),
  high (that it can't be misread as defer-or-cancel)
- **"Quit now" → Salir ahora** · the "now" is load-bearing (Cmdr quits either way when the countdown ends; this button
  only skips the wait), and Spanish carries it with the plain adverb, no restructuring needed · high
- **the countdown's "so a restart or logout never waits on Cmdr" → "para no hacerte esperar al reiniciar el Mac o cerrar
  sesión"** · restructured onto the two Tier-1 VERBS (AppKit Menus "Restart" → "Reiniciar", "Log Out" → "Cerrar sesión")
  because the matching NOUNS are unsourced: MS terminology carries `restart` and `sign out` as verbs only, and neither
  "reinicio" nor "cierre de sesión" appears as an entry, nor in the macOS corpus in this sense. "el Mac" is added
  because a bare "al reiniciar" could be read as restarting Cmdr; the catalog already says "el Mac" ("mientras no estás
  usando el Mac") · high (the verbs), tentative (adding "el Mac")
- **"what it leaves half-written" → lo que quede a medio escribir** · `a medio escribir` is verbatim from the catalog's
  own `settings.advanced.showStagingTempFiles.description` ("no puede dejar un archivo a medio escribir con un nombre
  real"), which describes this very mechanism · high (consistency-settled). A free relative, NOT the definite
  `el archivo a medio escribir`: see the number-neutral rule below
- **"clears away" (Cmdr removing its own temp leftover) → borra** · deliberately NOT the settled delete verb `eliminar`,
  which names the user-facing delete OPERATION; English softens to "clears away" for the same reason, and `borrar` is
  the catalog's non-operation removal verb (glossary: clear → borrar, macOS "Borrar búsquedas recientes") · high
- **"anything still being written stops where it is" → Lo que aún se está escribiendo se interrumpe donde esté** · **the
  body must stay number-neutral**: one operation writes several files at once and several operations can run at once, so
  a singular ("El único elemento que aún se está escribiendo") states something false, and `Lo que` scopes it without a
  numeral. "interrumpir" is the catalog's own word for a copy cut short ("Los restos de una copia interrumpida siempre
  se muestran"). ❌ Avoided "se detiene" / "se para": the style guide rules those out nearby because they read as
  PAUSED, which is `En pausa` · high
- **"item" → elemento** · REAFFIRMS the 44-to-8 catalog preference over macOS's "ítem" · high (consistency-settled)
- **`countdownAria` is not bound by WCAG 2.5.3** · it labels the countdown REGION, not a control whose visible label is
  another key, so there is no containment to satisfy; it only names what the number measures ("Tiempo hasta que Cmdr se
  cierre solo") · high
- None of the seven values contains an apostrophe, so there is nothing to double for ICU.
- No `sameAsSourceJustification` needed: all seven values differ from English.

## Usage stats: fuera "anónimas", dentro "un identificador aleatorio" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `seudónimo` / `seudonimizado` — that
jargon is exactly what the copy avoids.

- **usage stats → `estadísticas de uso`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anónimas` adjective was cut. MS terminology agrees (usage data → `datos de uso`) · high
- **a random id → `un identificador aleatorio`** · MS terminology (random → `aleatorio`, identifier → `identificador`) ·
  high. `identificador` is the ordinary Spanish word, not jargon; a bare `un id aleatorio` reads clipped in running
  prose.
- **tied to → `vincularse a` / `se vinculan a`** · the catalog's own verb for this relation
  (`onboarding.stepBeta.emailNote` "nunca se vincula a tus estadísticas de uso") · high
- No `sameAsSourceJustification` needed: every value differs from English.

## Filas en cola a la espera de respuesta y la confirmación de reversión (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`, 2026-08-13)

- **"Needs your answer" (badge de estado en la cola) → `Respuesta necesaria`** · macOS `es` ("Contraseña necesaria para
  desactivar la encriptación"), el mismo patrón "X necesaria" · high. ❌ Nunca `Esperando tu respuesta` ni nada que
  empiece por `Esperando`: `Esperando` ES el estado de "en cola detrás de otra operación" (`queue.row.status`), y los
  dos tienen que distinguirse en la misma columna estrecha.
- **"prompt" (la pregunta en pantalla que ha parado la operación) → `la pregunta`** · coincide con
  `operationConflict.pausedNote` ("hasta que respondas") · high.
- **"this operation carries on" → `esta operación continuará`** · estándar; evita `se reanudará`, que arrastra el
  sentido de `Reanudar` (resume tras pausa) · high.
- **rollback → `Revertir`** · reafirma la entrada ya establecida (`transferProgress.conflictRollback` = "Revertir"); el
  título es `¿Revertir esta operación?` y el botón destructivo `Revertir`, así coincide con el botón que abrió el
  diálogo · high.
- **"Keep them" (la respuesta segura) → `Conservar los archivos`** · macOS `es` ("Conservar", "Conservar todo",
  "Conservar original") · high. Se nombra el sustantivo en vez del clítico `Conservarlos`: el cuerpo menciona justo
  antes los archivos REEMPLAZADOS, así que un pronombre quedaría ambiguo.
- **"written so far" → `escritos hasta ahora`** · reutiliza `written → escrito` del catálogo
  (`transferProgress.stallInFlight` "puede que ya esté parcialmente escrito") · high.
- **"Stop, and …" (tooltip de reversión) → `Detener y …`** · macOS `es` ("Detener copia", "Detener eliminación",
  "Detener traslado") · high. Distinto de `Cancelar`, que es justo lo que el tooltip NO debe parecer.
- **foregroundBusyToast: nombra la operación.** El "this one" del inglés no tiene antecedente en español, así que el
  valor lo explicita: "… y luego muestra esta operación", reutilizando el botón `Mostrar` (`queue.row.foreground`) ·
  high.
- No hace falta `sameAsSourceJustification`: todos los valores difieren del inglés.

## La cadena de renombrados: el aviso que cuenta los archivos que no cambiaron (`fileExplorer.rename.chainKeptOriginalName*`, 2026-08-18)

- **"kept its name" → `mantuvo su nombre`** · reafirma lo que ya usa el hermano `chainKeptOriginalName` ("{reason}.
  “{name}” mantuvo su nombre."); macOS `es` usa `conservar` para el sentido de "quedarse con" en diálogos de elección
  ("Conservar original", "¿Quieres conservarla?"), pero ahí el usuario ELIGE conservar, mientras que aquí el archivo se
  quedó con su nombre sin que nadie lo decidiera, y `mantuvo` cuenta ese hecho sin sugerir una elección · high
  (consistencia con el hermano + matiz de sentido). Los dos son un solo aviso que se reescribe, así que la fórmula tiene
  que ser idéntica palabra por palabra.
- **"and so did {n} other files" (un elemento nombrado + el recuento de los demás) → `y otros {n} archivos también`** ·
  macOS `es` Finder tiene el patrón exacto (nombrar uno entre comillas y contar el resto): PE106_V4 "… como “^1” y otros
  ^0 ítems", y AirDrop MR201_V3/MR101_V3 "Enviando/Recibiendo “^1” y ^0 ítems más". Nautilus/Thunar `es` confirman "y
  otros archivos". Se elige `y otros N archivos` sobre el `y N ítems más` de AirDrop porque el inglés dice "other files"
  y el catálogo ya fija file → `archivo`; el `también` final es lo que carga el "and so did" · high.
- **Rama singular sin numeral: `y otro archivo también`** · en español `otro` ya dice "uno más", así que meter el
  numeral ("y 1 otro archivo") sería agramatical. El inglés hace lo mismo (su rama `one` escribe "one other file" y no
  usa `{othersText}`), así que `{othersText}` solo aparece en `many`/`other` · high.
- **Comillas: `“…”` (curvas), como el hermano y como macOS `es`** · nunca `«…»` ni comillas rectas, aunque otras claves
  viejas de `fileExplorer.json` (`renameConflict.description`) todavía usen las rectas · high.
- No hace falta `sameAsSourceJustification`: el valor difiere del inglés.

## El renombrado sin confirmar y el nombre que el sistema rechaza (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`, 2026-08-18)

Pareja hermana de `chainKeptOriginalName*`, con el sentido contrario: allí el archivo se quedó con su nombre, seguro;
aquí no se sabe, y puede que sí se haya renombrado. Estas dos claves nunca deben insinuar que el archivo conservó su
nombre.

- **"the rename" (el sustantivo) → `el cambio de nombre`** · macOS `es` Finder RN1/RN2 ("Rehacer/Deshacer cambio de
  nombre") y AppKit SavePanel ("Name Change" → "Cambio de nombre"). Se elige el sustantivo de Apple en vez de inventar
  "el renombrado", y además da un sujeto masculino singular conocido para la segunda frase, así que el participio no
  depende del género de lo que haya detrás de `{name}` (archivo o carpeta) · high
- **"Couldn''t confirm …" → `No se pudo confirmar …`** · reafirma el patrón que el catálogo ya usa para este mismo caso
  de "no hubo respuesta a tiempo": `fileOperations.mkdir.timeoutMessage` ("No se pudo confirmar que la carpeta se
  creara…") y `fileExplorer.pane.trashUnconfirmedToast`. confirm → confirmar (macOS AppKit Common, "Confirm" →
  "Confirmar") · high
- **"The volume may be slow" → `El volumen puede ir lento`** · copia literal de `mkdir.timeoutMessage`, que traduce esa
  misma frase inglesa; `trashUnconfirmedToast` usa la variante "Puede que el volumen vaya lento". La locución del
  catálogo para un volumen que tarda es `ir lento` · high
- **"the rename may still have gone through" → `es posible que el cambio sí se haya aplicado`** · el `sí` enfático es el
  recurso con el que los hermanos cargan el "still" del inglés ("es posible que la carpeta sí se haya creado", "quizá el
  archivo sí se haya movido"). `aplicarse` para un cambio que surte efecto: macOS `es` ("Estos ajustes se aplicarán…") ·
  high. El inglés vuelve a nombrar el sujeto ("the rename"), igual que el hermano `mkdir` repite "the folder", así que
  el español también lo nombra, pero con el anafórico corto `el cambio`: repetir las cuatro palabras "el cambio de
  nombre" en un aviso tan breve pesa demasiado. El sujeto es el cambio, no el archivo, así que no hay género que
  adivinar detrás de `{name}`
- **Coordinación negativa: `ni el de`** · "the rename of X and N other files" bajo una negación pide `ni` en español:
  "No se pudo confirmar el cambio de nombre de “{name}” ni el de otros 3 archivos". El `el de` elide "el cambio de
  nombre" y deja claro que lo no confirmado también es el cambio de nombre de los otros · high
- **Las tres ramas cierran en plural (`los cambios sí se hayan aplicado`)** · incluso en la rama `one` hay dos cambios
  de nombre (el nombrado más el otro). Ramas CLDR `one`/`many`/`other`; `{othersText}` solo aparece en `many`/`other`,
  igual que en `chainKeptOriginalNameAndOthers`, porque `otro archivo` ya dice "uno más" · high
- **"That filename can''t be used" → `Ese nombre de archivo no puede usarse`** (carpeta:
  `Ese nombre de carpeta no puede usarse`) · macOS `es` Finder tiene el concepto exacto: RN5 "El nombre “^0” no puede
  usarse porque está reservado para el sistema", NE74 "…no puede usarse porque es demasiado largo", RN23 "…no se puede
  usar". El demostrativo `Ese` traduce el "That" del inglés (el nombre que acabas de escribir), y por eso no lleva el
  artículo de los hermanos (`empty`, `disallowedChars`, `nameTooLong` usan "El nombre de la carpeta / del archivo"); el
  sustantivo carpeta/archivo sí es el mismo · high. Sin punto final, porque se compone dentro de la frase de
  `chainKeptOriginalName`: "Ese nombre de archivo no puede usarse. “foo.txt” mantuvo su nombre."
- Ningún valor lleva apóstrofo, así que no hay nada que doblar para ICU.
- No hace falta `sameAsSourceJustification`: los tres valores difieren del inglés.

## Operaciones sugeridas: el diálogo de lo que propone Ask Cmdr (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (el conjunto de operaciones de archivo del agente) → `operaciones`; el título queda `Operaciones sugeridas` ·
  reutiliza el término de la casa ("File operations" → "Operaciones de archivos" en `settings.json`) · high
- approve → `Aprobar` · MS (familia "aprobación"); elegido sobre el `Aceptar` de macOS porque la variante con recuento
  ("Aprobar 3 archivos") autoriza una acción en vez de aceptar un objeto · high
- reject → `Rechazar` · macOS Finder, el par Aceptar/Rechazar del panel de AirDrop (Tier 1) · high
- "This can't be undone" → `Esto no se puede deshacer` · macOS Finder ("Esta acción no se puede deshacer", alerta de
  eliminación inmediata), acortado para un marcador de una línea · high
- "Undo by deleting what it writes" → `Deshacer eliminando lo creado` · compuesto; se evita "lo que crea" porque se lee
  como el verbo "creer" · tentative
- suggestion → `sugerencia` · ya en el catálogo (`ui.combobox`, `askCmdr`) · high

## Duplicar: el comando que copia en la misma carpeta (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (comando que copia la selección dentro de su propia carpeta) → `Duplicar`** · macOS Finder `es`, menú
  "Archivo > Duplicar" (`N154`), más "Duplicar ítems" y "Duplica los ítems en las ubicaciones actuales" (verificado en
  macOS 26.6.1, `Finder.app/Contents/Resources/es.lproj`, 2026-08-19) · high. Convive con `Copiar` (F5) y `Mover` (F6)
  sin solaparse.
- **"Make a copy of the selected files in the same folder" →
  `Crea una copia de los archivos seleccionados en la misma carpeta`** · tercera persona, como las descripciones vecinas
  ("Copia los archivos seleccionados…"); se mantiene `archivos` del catálogo en lugar del `ítems` de Finder · high.

## Menús nativos: barra de menús, menús contextuales, títulos de ventana (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

Fuentes de todo este grupo: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/es.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) es Tier 1 y decide casi todo; el lado inglés está en `en_GB.lproj`, porque `Base.lproj`
solo trae nibs compilados. Safari 26 (`MainMenu.strings`) aporta el vocabulario de pestañas, y la terminología de
Microsoft lo que Apple no nombra. Familia RAW: **apóstrofos simples**, un `''` saldría duplicado en el menú.

- **Títulos de la barra → `Archivo`, `Edición`, `Visualización`, `Ir`, `Ventana`, `Ayuda`, `Servicios`** · macOS Finder
  y Safari `es` · high.
- **Menú Select (selección de archivos) → `Seleccionar`** · Nautilus/Thunar/Dolphin `es` · high. El Finder no tiene
  equivalente; el infinitivo encaja con `Seleccionar todo` del mismo menú.
- **Hide Others → `Ocultar otras apps`** · macOS Finder (`300729.title`, macOS 26.6.2, build 25G83, 2026-08-30) · high.
  Es lo que el usuario ve en su Mac, y más claro que `Ocultar los demás`. Va en las DOS claves del mismo comando:
  `menu.app.hideOthers` (barra de menús) y `commands.appHideOthers.label` (paleta y lista de atajos), que se habían
  separado.
- **Quick Look → `Vista rápida`** · macOS Finder (`TL14`) · high. Apple sí localiza este nombre de función, por eso no
  está en la lista de no-traducir.
- **Get Info → `Obtener información`, Enclosing Folder → `Carpeta contenedora`, Go > Home → `Inicio`, Sort By →
  `Ordenar por`, Default → `Por omisión`** · macOS Finder Tier 1 · high. `Por omisión` (no `Predeterminado`, que es la
  convención de Windows) ya se usa en `commands.fileEdit.label`.
- **zoom in / out → `Ampliar` / `Reducir`** · Safari `es` (menú Visualización) · high. Más corto y más natural que el
  `Aumentar/Reducir el zoom` del catálogo, que se queda en la paleta de comandos.
- **ascending / descending → `Ascendente` / `Descendente`** · Dolphin `es` (Thunar dice `Orden ascendente`) · high.
- **changelog → `Registro de cambios`** · terminología de Microsoft · high. Se distingue de Ayuda > `Novedades`: uno
  nombra el documento, el otro la noticia.
- **word wrap → `Ajuste de línea`** · terminología de Microsoft · high.
- **pin / unpin tab → `Fijar pestaña` / `Desfijar pestaña`** · el catálogo (`commands.tabTogglePin.label`) y la mayoría
  de navegadores en español · high. Nota: Safari `es` dice `Anclar pestaña`; se mantiene `fijar` por coherencia con el
  resto del catálogo, y `anclar` queda registrado como la variante Tier 1.
- **Colores de etiqueta del Finder → `Rojo, Naranja, Amarillo, Verde, Azul, Morado, Gris`** · macOS Finder
  (`TG_COLOR_*`) · high.
- **busy (volumen en uso) → `(ocupado)`** · terminología de Microsoft · high.
- **Eject → `Expulsar`, Disconnect → `Desconectar`, Remove (de una lista) → `Quitar`** · macOS Finder · high. `Quitar`
  evita que borrar un favorito suene a borrar archivos.
- **forget (servidor, contraseña) → `olvidar`** · ya en el catálogo (`fileExplorer.network.share.forgetPassword`) ·
  high.
- **Idénticos al inglés a propósito** (con `sameAsSourceJustification`): `menu.view.zoom`, `menu.window.zoom`,
  `menu.zoom.percent*` y `menu.view.askCmdr`.

## El aviso de la conexión que macOS presta (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

Tres claves: el cuerpo del aviso que aparece cuando Cmdr no consiguió abrir su propia conexión al recurso compartido y
este funciona con la que da macOS, el botón que reintenta, y el tooltip de la X.

- **"You are connected" → `Tienes acceso`** · reestructurado para no generar al usuario: `estás conectado/a` obliga a un
  adjetivo con género (macOS lo resuelve con `^[Conectado](inflect: true…)`, un mecanismo que ICU no tiene).
  `Tienes acceso` dice el mismo hecho tranquilizador, es neutro y evita una cuarta aparición de "conexión" en la frase ·
  high
- **"macOS's native SMB network connection" → `la conexión de red SMB nativa de macOS`** · calco directo; `SMB` y
  `macOS` verbatim (lista de no-traducir). Nota: cuando la frase NO nombra a macOS, el catálogo ya dice
  `la conexión del sistema` (`fileExplorer.pane.directConnection*Toast`); aquí el inglés sí la nombra, así que se
  traduce entera · high
- **Multiplicadores de velocidad (`4x`, `100x`) → `4 veces más lenta`, `(a veces, 100 veces)`** · el español no usa la
  notación `4x` en prosa de interfaz; se escribe con `veces`. Se mantienen las cifras en dígitos (no `cuatro`) porque
  son datos técnicos comparativos y así conservan el golpe que tienen en inglés · high
- **"for most connections" → `en la mayoría de las conexiones`**, colocado antes del comparativo · en español el
  `más lenta que…` tiene que quedar junto a su término de comparación, así que el paréntesis `(a veces, 100 veces)` se
  mueve al final de la frase, detrás de `la conexión directa de Cmdr` · high
- **"Click the button below to try again." → `Haz clic en el botón de abajo para volver a intentarlo.`** ·
  `haz clic en el botón` es la fórmula de macOS (AppKit Printing, "click the Add (+) button" → "haz clic en el botón
  Añadir (+)") y ya está en el catálogo (`onboarding`, "basta con que hagas clic en el botón de abajo");
  `volver a intentarlo` es la forma del catálogo, hermana del `Inténtalo de nuevo` de macOS · high
- **"Try connecting directly" (botón) → `Intentar conectar directamente`** · infinitivo, como todo botón (guía de
  estilo), y reutiliza el verbo ya asentado en `fileExplorer.navigation.connectDirectly` ("Conectar directamente para un
  acceso más rápido") y `fileExplorer.pane.connectedDirectlyToast` · high
- **"Dismiss" (tooltip de la X) → `Descartar`** · misma clave de origen (`sourceHash` `48845bf`) que
  `lowDiskSpace.toast.closeTooltip`, que ya dice `Descartar`; reafirma la entrada de la guía de estilo · high
  (consistency-settled)
- Sin `sameAsSourceJustification`: los tres valores difieren del inglés.

## Los avisos de una línea de renombrar / crear (`errors.mutation.*`, `errors.volume.*`, 2026-08-23)

31 claves nuevas: la línea que aparece bajo el campo del nombre (o en un aviso breve) cuando se rechaza un renombrado,
una carpeta nueva o un archivo nuevo. Familia RAW, así que apóstrofos simples y `{path}` literal. Casi todas tienen ya
un hermano largo en este mismo archivo (`errors.listing.*` / `errors.write.*`), así que la regla de este bloque es
**citar al hermano, no reinventar la frase**: el aviso corto y la explicación larga del mismo fallo tienen que decir lo
mismo con las mismas palabras.

- **Comillas alrededor de `{path}`: `“…”` (curvas)** · el inglés escribe `"{path}"` con comillas rectas, pero la regla
  ya asentada del catálogo es la curva (§ La cadena de renombrados; macOS `es` "^0" siempre entre `“…”`). Los hermanos
  largos usan backticks porque se renderizan como markdown; estas claves son texto plano, así que van con comillas ·
  high
- **`{path}` es un inserto incontrolado**: ninguna frase concuerda con él (nunca "la ruta {path} está…"), así que las
  seis claves con marcador lo dejan como sujeto entrecomillado o detrás de una preposición · high
- **root folder (of a volume) → `carpeta raíz`** · terminología de Microsoft, con la definición exacta de nuestro caso
  ("The uppermost directory on a computer, partition or volume" → `carpeta raíz` / `directorio raíz` /
  `carpeta de nivel superior`) · high. ❌ NO `carpeta superior`, que en este catálogo es "parent folder" (§ navegación
  con doble clic): usarla aquí diría que lo que no se puede renombrar es la carpeta de arriba. `cantRenameVolumeRoot` =
  "No se puede cambiar el nombre de la carpeta raíz de un volumen desde aquí.", con la forma de prosa de macOS Finder
  (RN11 "No se puede cambiar el nombre de “^0” en estos momentos porque…").
- **System Integrity Protection → `la protección de la integridad del sistema`** · macOS Finder Tier 1, verbatim y en
  minúsculas dentro de la frase: ET6 "Some items in the Trash cannot be deleted because of System Integrity Protection."
  → "Algunos ítems de la papelera no se pueden eliminar debido a la protección de la integridad del sistema" · high. Se
  reordena la frase inglesa ("macOS protects this item with…") para no decir "protege … con la protección": el valor es
  "No se puede cambiar el nombre de este elemento debido a la protección de la integridad del sistema de macOS."
- **rename en prosa → `cambiar el nombre`** (el verbo de botón sigue siendo `renombrar`) · macOS Finder escribe estas
  frases así (RN11, RN33 "No puede cambiarse el nombre del ítem “^0”."), y el sustantivo asentado es `cambio de nombre`.
  Las tres claves de este bloque que hablan de renombrar usan la perífrasis · high
- **"is on its way out" → `está de salida`**, **"the destination can't store that name" →
  `el destino no puede guardar ese nombre`**, **"no longer available" → `ya no está disponible`**, **"didn't respond in
  time" → `no respondió a tiempo`**, **"is locked" → `está bloqueado`**, **"Get Info" → `Obtener información`** · todos
  citados literalmente de los hermanos largos del propio `errors.json` (`listing.deletePending`, `write.invalidName`,
  `listing.notFound`, `listing.connectionTimedOut`, `write.fileLocked`) y de macOS Finder (NE7, NE17, NE18) · high
  (consistencia)
- **`timedOut` no es un fallo** · la operación puede aún completarse, así que se copia la fórmula ya asentada para el
  renombrado sin confirmar: "así que es posible que el cambio sí se aplique" (`fileExplorer.rename.unconfirmed`,
  `fileOperations.mkdir.timeoutMessage`). El `sí` enfático es lo que carga el "may still" del inglés · high
- **`deviceSessionReset` no es una desconexión** · el teléfono (MTP) sigue enchufado. Se cita el hermano
  `errors.listing.deviceReconnecting`: "El dispositivo sigue conectado…" + "Espera unos segundos y vuelve a intentarlo."
  El valor dice "El dispositivo reinició su conexión.", nunca nada que suene a desenchufar · high
- **"Move it instead" → `Usa Mover.`** · nombra el comando de Cmdr (F6 `Mover`), como ya hace el catálogo con "Usa
  {deletePermanentlyKey}…" y "usa Finder para esta operación". Las dos claves hermanas (`renameOutOfArchive` /
  `renameAcrossArchives`) quedan en paralelo palabra por palabra · high
- **"Only zip archives can be changed" → `Solo se pueden modificar los archivos zip`** · aquí `zip` desambigua por sí
  solo, así que no hace falta el `archivo comprimido` completo; el catálogo ya dice "el zip" a secas (aviso de borrado
  dentro de un comprimido). El resto de claves del bloque sí usan `archivo comprimido` · high
- **"Cmdr stopped this at your request" → `Cmdr detuvo esto porque se lo pediste.`** · `detener` es el verbo de macOS
  para parar una operación ("Detener copia"), distinto de `Cancelar`; el "at your request" se resuelve con la segunda
  persona en vez del formal "a petición tuya" · high (el verbo), tentative (el giro)
- **"Something went wrong, and Cmdr couldn't tell what." → `Algo salió mal y Cmdr no pudo saber qué.`** ·
  `Algo salió mal` ya está en el catálogo (`ai.cloud.genericError`, `licensing.error.generic`) y esquiva la prohibición
  de "error"/"fallo" · high
- **"The volume couldn't finish that" → `El volumen no pudo completarlo.`** · reutiliza el patrón tranquilo "No se pudo
  completar" con el volumen como sujeto explícito, que es lo que hace el inglés · high
- Ningún valor lleva apóstrofo, así que no hay nada que doblar (y en esta familia RAW tampoco se debe).
- No hace falta `sameAsSourceJustification`: los 31 valores difieren del inglés.

### Segunda tanda: las dos claves de papelera (`errors.mutation.trashNotSupported`/`trashRefused`, 2026-08-23)

Mismo aviso de una línea, misma familia RAW. Las dos hablan de la papelera de macOS, así que la regla del bloque sigue
siendo citar a los hermanos largos del catálogo.

- **Trash (la papelera de macOS) → `papelera`, en minúsculas dentro de la frase** · macOS AppKit escribe "could not be
  moved to the trash" → "no han podido trasladarse a la papelera" y Finder PE60.2 "No se puede trasladar “^0” a la
  papelera porque tiene una ruta demasiado larga."; todo el catálogo (`errors.write.*.trash`,
  `fileOperations.delete.trashSwitch`) ya dice `papelera` en minúscula · high (ya asentado, se reafirma)
- **El verbo sigue siendo `mover`, no `trasladar`** · Finder dice "Trasladar a la papelera", pero el catálogo tiene
  asentado `move → mover` en toda la familia ("Mover a la papelera", "No se pudo mover a la papelera"), y las dos frases
  se leen juntas · high (consistencia)
- **"has no Trash" → `no tiene papelera`** · el inglés varía a propósito entre el diálogo largo ("doesn't support trash"
  → `Este volumen no admite papelera.`, `fileOperations.delete.noTrashWarningStrong`) y este aviso corto; el giro
  `no hay papelera` ya está en el catálogo (`fileOperations.delete.archiveWarningStrong`, "Dentro de un archivo
  comprimido no hay papelera."), así que `no tiene papelera` es el mismo idioma del catálogo y además es más directo que
  el técnico `no admite` · high
- **"the only way is to delete permanently" → `solo se puede eliminar permanentemente`** · el impersonal con `se` evita
  el infinitivo sin objeto (`eliminar permanentemente` a secas suena cojo en español) y también el clítico
  `eliminarlo`/`eliminarla`, que generaría concordancia con un ítem cuyo género y número no conocemos. `permanentemente`
  repite literalmente la etiqueta del comando (`commands.fileDeletePermanently.label` = "Eliminar permanentemente"), que
  es justo lo que la persona tiene que usar · high. Valor: "Este volumen no tiene papelera, así que solo se puede
  eliminar permanentemente."
- **"macOS wouldn't move this to the Trash." → `macOS no permitió mover esto a la papelera.`** · el inglés
  antropomorfiza el sistema ("wouldn't"), y en español el giro tranquilo y natural para eso es `no permitió`, calcado de
  macOS AppKit ("The permission settings … don't allow it to be modified." → "Los ajustes de permisos … no permiten
  modificarlo.") · high. ❌ No `rechazó mover`: `rechazar` pide objeto nominal (Finder: "“^0” ha rechazado tu
  solicitud."), y `no quiso` es demasiado coloquial. `esto` mantiene la frase sin género, igual que
  `errors.volume.cancelled` ("Cmdr detuvo esto…"); la clave es corta a propósito porque la razón técnica sale aparte en
  `Detalles técnicos`
- Ninguno de los dos valores lleva apóstrofo ni marcador, y los dos difieren del inglés, así que no hace falta
  `sameAsSourceJustification`.

## El diálogo de fallos: las tres aperturas (`crashReporter.dialog.body.ended`/`.keptRunning`/`.unknown`)

El diálogo del siguiente arranque elige una de tres frases según lo que el informe registró. `.ended` no cambia; las dos
nuevas **no pueden decir que Cmdr se cerró, falló o se detuvo**, porque no ocurrió nada de eso.

- **"ran into a problem" → `tuvo un problema`** · el corpus de macOS ES usa la forma impersonal (`AppKitErrors.json`:
  "Ha habido un problema al intentar obtener…"; Finder `PE90` "hay un problema con el archivo"; Nautilus "Hubo un
  problema al ejecutar este programa"), y Microsoft `SPANISH.tbx` fija `problem` → `problema` (31254_1597568_1679737).
  Ninguna fuente puede tomar `Cmdr` como sujeto, y las tres claves hermanas lo hacen, así que decide el paralelismo con
  `.ended` · high.
- **"kept running" → `siguió funcionando`** · high. ❌ No `siguió ejecutándose`: en este catálogo `ejecutarse` nombra
  una _operación_ que sigue en segundo plano (`transferProgress.backgroundedToast`: "Sigue ejecutándose en segundo
  plano"), que es justo la lectura equivocada aquí, y colocado detrás de `en segundo plano` la invita. `funcionar`
  además es más cálido y menos técnico. `sigue/siguió funcionando` no tiene ni una aparición en todo el pile (macOS no
  trae ninguna cadena de tipo "still running"), así que la elección se apoya en la coherencia del catálogo.
- **"in the background" → `en segundo plano`** · ya fijado en el glosario; Microsoft `SPANISH.tbx` (`background`
  adjetivo, 16344_18758_18759), Double Commander ("Cuando la aplicación esté en segundo plano"), Dolphin ("indexando los
  archivos en segundo plano"), Total Commander ("operaciones activas en segundo plano") · confirmed. Se engancha al
  problema, no al hecho de seguir funcionando. Dato negativo: **`segundo plano` no aparece en ninguna parte del corpus
  de macOS ES** (Finder, AppKit ni Ajustes del Sistema), así que el término se apoya en Microsoft y los gestores de
  archivos.
- **"a report" (sin "crash") → `un informe`** · Microsoft `report` → `informe`. La segunda frase es la de `.ended`
  literal, quitando `de fallos`. El clítico de `solucionarlo` sigue concordando: su antecedente pasa a ser `un problema`
  (masc. sing.) · confirmed.
- **El verbo del fallo, `se cerró inesperadamente`, queda exclusivo de `.ended`.** `.unknown` no nombra ningún
  desenlace, así que es verdadera tanto si Cmdr se cerró como si siguió.
- **Orden de palabras, divergencia deliberada**: `.unknown` deja `la última vez` al final, igual que `.ended`;
  `.keptRunning` la antepone, porque dejarla en su sitio apilaría dos circunstanciales sin pausa ("un problema en
  segundo plano la última vez y siguió…"), pesado en español, y anteponerla es además lo que hace macOS. Solo se ve una
  de las tres variantes cada vez, así que la divergencia es invisible para el usuario; queda anotada por si una futura
  pasada de coherencia intenta unificarlas.
- Ningún valor necesita `sameAsSourceJustification`: los dos difieren del inglés.

## El texto de ajustes de informes ahora cubre los dos casos (`settings.updates.crashReports.description`)

El interruptor también envía un informe cuando un problema en segundo plano NO cerró la app, así que la ayuda ya no
puede hablar solo de que Cmdr se cierra. Todas las piezas salen de la sección del diálogo de fallos de arriba, en
presente:

- `cuando Cmdr se cierra inesperadamente` de `crashReporter.dialog.body.ended`; `tiene un problema en segundo plano` de
  `.keptRunning` · high. El presente es morfología, no una decisión de término nueva.
- `un informe` (sin `de fallos`) porque la frase cubre los dos casos, la misma operación que en `.title.report` · high.
  ❌ La ETIQUETA `settings.updates.crashReports.label` sigue siendo `Enviar informes de fallos`: es el nombre del
  ajuste.
- La segunda frase se toma de `crashReporter.dialog.privacyNote` (`qué parte del código tuvo el problema`) y sustituye a
  `la ubicación del fallo`, que solo era cierta si algo falló · high.

## Expulsar y desconectar: los nueve avisos del selector de volúmenes (`errors.eject.*`, 2026-08-23)

Nueve claves nuevas. Cada valor se inserta detrás de dos puntos, en un aviso breve de la esquina superior derecha:
`No se pudo expulsar {volumeName}: …` o `No se pudo desconectar: …` (`fileExplorer.pane.ejectFailedToast` /
`.disconnectFailedToast`). Familia RAW: apóstrofos simples y sin marcadores. Como en el bloque de `errors.mutation.*`,
la regla es **citar al hermano antes que inventar la frase**.

- **"drive" (el volumen que se expulsa) → `disco`** · macOS Finder es el corpus más denso que hay para esta frase exacta
  y siempre dice `disco`: NE31/NE80 "El disco “^0” está en uso y no se puede expulsar.", NE79 "Un disco en “^0” está en
  uso y no se puede expulsar.", TL_HELP_EJCT "Expulsar discos y desmontar servidores", AppKit `AppKitErrors` "The disk
  could not be ejected because it is in use by “%@”." → "No se ha podido expulsar el disco porque está siendo usado por
  “%@”." · high. Ojo con los otros dos usos de "drive" ya asentados en el catálogo: `unidades` cuando es la lista de
  volúmenes del usuario ("your drives" → "tus unidades", `askCmdr.tool.listVolumes.doing`) y `disco de red` /
  `disco externo` en prosa larga. Aquí manda el sentido de expulsión, así que `disco`.
- **removable → `extraíble`** · macOS Finder (`KIND_FORMATTER_28_0` "Volumen extraíble", `GV3` "Volúmenes extraíbles") +
  terminología de Microsoft (`removable` → `extraíble`, 17166_639981_654018; `removable drive` → `unidad extraíble`) ·
  high. Valor: "Este disco no es extraíble, así que se queda conectado."
- **idle (un dispositivo que ya no está trabajando) → `inactivo`** · terminología de Microsoft (`idle` → `inactivo`,
  9514_63893_63894) · high.
- **"Unplug it" → `Desconecta el cable`**, no `Desconéctalo` · el catálogo ya traduce "unplug" con `desconectar`
  (`errors.listing.deviceReconnecting.suggestion`, "There's nothing to unplug." → "No hace falta desconectar nada."),
  pero aquí `Desconéctalo` chocaría con el comando `Desconectar` de Cmdr, que es justo lo que acaba de no funcionar.
  Nombrar el cable deja claro que es la acción física, y el cable ya está en el catálogo
  (`errors.listing.deviceDisconnected.suggestion`, "asegúrate de que el cable esté bien sujeto"). El pile no tiene
  ninguna cadena con `desenchufar` · high (el giro), tentative (que `cable` sea siempre exacto: un MTP va por USB, así
  que sí lo es hoy)
- **"The device wouldn't close its connection." → `El dispositivo no cerró su conexión.`** · mismo antropomorfismo
  inglés que `trashRefused`, y la misma solución: el hecho en pasado y sin veredicto. ❌ No `no quiso cerrar` (demasiado
  coloquial) ni `rechazó cerrar` (`rechazar` pide objeto nominal) · high
- **`timedOut` no es un fallo**, se copia la fórmula ya asentada de `errors.mutation.timedOut` ("El volumen aún no ha
  respondido, así que es posible que el cambio sí se aplique."): "El disco aún no ha respondido, así que es posible que
  sí se expulse por su cuenta." El `sí` enfático carga el "may still" · high (consistencia)
- **`volumeNotFound` calca a `errors.mutation.volumeGone`** ("Ese volumen ya no está disponible, así que no cambió
  nada."), con el mismo esqueleto `Ese … ya no está …, así que …`: "Ese disco ya no está conectado, así que no hay nada
  que expulsar." El giro `no hay nada que + infinitivo` ya está en el catálogo (`askCmdr.renameUndo.unavailable`, "No
  hay nada que restaurar.") · high
- **network share → `recurso compartido de red`** · ya asentado en el catálogo (`errors.listing.notSupportedErrno`,
  `settings.indexing.askForEachDrive`, `settings.network.timeoutMode`) sobre macOS + terminología de Microsoft · high
  (se reafirma)
- **`unexpected` es literalmente la misma cadena inglesa que `errors.mutation.unexpected`**, así que reutiliza su valor
  palabra por palabra: "Algo salió mal y Cmdr no pudo saber qué." Dos claves distintas con el mismo texto es lo
  correcto: el inglés también las tiene idénticas · high
- **"Close any open files and apps" → `Cierra los archivos y las apps que tengas abiertos`** · la relativa con `tener`
  evita el participio suelto (`los archivos y las apps abiertos` obliga a un masculino plural que suena forzado con un
  sustantivo femenino al lado) y suena a instrucción de macOS. `app` en minúscula ya está en el glosario · high
- **`busy` reutiliza el `todavía está + gerundio` del catálogo** (`errors.mutation.archiveEditNotReady`, "Cmdr todavía
  se está iniciando"): "Cmdr todavía está moviendo archivos en este disco. Expúlsalo cuando termine." El clítico `-lo`
  concuerda con `disco`, así que no hay género que exponer · high
- Ningún valor lleva apóstrofo ni marcador `{}`, y los nueve difieren del inglés, así que no hace falta
  `sameAsSourceJustification`.

## El aviso de la papelera: deshacer y devolver a su sitio (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

Superficie nueva: después de mover elementos a la papelera aparece un aviso con dos botones ("Deshacer", "Ir a la
papelera"), y el mismo comando está en la paleta de comandos.

- **`undo` (botón) → `Deshacer`** · macOS AppKit MenuCommands ("Undo Smart Dash" → "Deshacer guion inteligente"), GNOME
  Nautilus ("Undo" → "Deshacer") y el propio catálogo (`askCmdr.renameUndo.undo`) · high
- **`put back` (devolver un elemento de la papelera a donde estaba) → `devolver … a su sitio`** · el catálogo ya usa
  `devolver` para esto (`askCmdr.renameUndo.skipReason.failed.named`, "Cmdr no pudo devolverle su nombre anterior"), y
  `a su sitio` es lo que aporta el "back where it was" del inglés · high. El Finder en español llama al comando "Sacar
  de la papelera" (macOS `N153.1`), correcto para un rótulo de menú pero incómodo aquí: el aviso parcial ya nombra la
  papelera en su segunda mitad y repetirla sonaría torpe. Nautilus dice "Restaurar … de la papelera"; `restaurar` está
  reservado en el catálogo para deshacer un renombrado (`askCmdr.renameUndo.undone`), así que se mantiene la distinción
  entre devolver un SITIO y restaurar un NOMBRE.
- **`This drive doesn't keep a trash.` → `Esta unidad no tiene papelera.`** · dato sobre la unidad, sin veredicto, en la
  línea de `fileOperations.delete.noTrashWarningStrong` ("Este volumen no admite papelera."). El inglés dice `drive`, y
  `unidad` es lo que el catálogo usa para eso (`askCmdr.renameUndo.unavailable`, "su unidad no esté conectada") · high
- **`Nothing to put back.` → `No hay nada que devolver.`** · el giro `no hay nada que + infinitivo` ya está asentado
  (`askCmdr.renameUndo.unavailable`, "No hay nada que restaurar.") · high
- **La segunda mitad trae su propio parámetro de cantidad (`{skipped}`)**, así que conjuga con normalidad: "… a su
  sitio; {skippedText} {skipped, plural, one {elemento se quedó} many {elementos se quedaron} other {elementos se
  quedaron}} en la papelera." El sustantivo contado es `elemento`, la palabra del catálogo para el `item` que dice la
  fuente en esta mitad, no `archivo` como en la primera · high
- **El botón del aviso y el nombre del comando son el mismo texto** ("Ir a la papelera"), igual que sus hermanos
  `commands.navParent.label` ("Ir a la carpeta superior") y `commands.downloadsGoToLatest.label` ("Ir a la última
  descarga").
- Ningún valor necesita `sameAsSourceJustification`: los nueve difieren del inglés.

## Añadir a un informe ya enviado: el diálogo de la nota tardía (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`, 2026-08-28)

Superficie nueva: cuando Cmdr envía un informe por su cuenta, el aviso trae un botón que abre un diálogo con lo que ya
se envió y una caja para escribir una nota que se engancha a ESE mismo informe (no se sube nada por segunda vez). Si el
informe ya no admite añadidos, el diálogo lo dice y remite al menú Ayuda.

- **`add to` (sin objeto directo, "Add to your error report" / "Add to report") → `Añadir a …` / `Añadir al informe`** ·
  macOS `es` usa exactamente esta elipsis en sus propios comandos: `Añadir a favoritos`, `Añadir a la barra lateral`,
  `Añadir al Dock` (Finder `MenuBar`/`Localizable`) · high. El español normalmente pide objeto tras `añadir`, así que
  sin este precedente habría que meter un `algo` de relleno; con él, el título y el botón conservan la brevedad del
  inglés. ❌ No `agregar`: **cero apariciones** en todo el corpus de macOS `es` frente a las 20+ de `añadir`, y el
  catálogo ya dice `Añade una nota (opcional)` en el diálogo hermano.
- **El título del diálogo va en infinitivo** (`Añadir a tu informe de error`), igual que su hermano
  `errorReporter.dialog.title` (`Enviar informe de error`) y `feedback.dialog.title` (`Enviar comentarios`) · high. El
  imperativo de `updates.moveToApplicationsDialog.title` ("Mueve Cmdr a…") es la excepción de un título que da una
  instrucción, no la norma.
- **`error report` sigue siendo `informe de error`** aquí, por coherencia con las claves vecinas de `errorReporter.*` ·
  tentative (heredado). ⚠️ Costura conocida: el ítem de menú `menu.help.sendErrorReport` dice
  `Enviar informe de fallos…` mientras que este diálogo dice `informe de error`. Por eso `amend.unavailable` remite al
  menú SIN nombrar el tipo de informe (`envía un informe nuevo desde el menú Ayuda`): así no elige bando en una costura
  que nadie ha resuelto todavía. Si algún día se unifica, esta clave no hay que tocarla.
- **`the Help menu` → `el menú Ayuda`** · el nombre del menú es `Ayuda` en macOS (Finder `MenuBar` `300630`/`300631`,
  AppKit `MenuCommands`/`HelpManager`) y el catálogo ya lo fija en `menu.bar.help` · high. Sin comillas: macOS escribe
  `selecciona menú Apple > Ajustes del Sistema` (entrecomilla los paneles, no los menús).
- **`can't take a note any more` → `Ya no se pueden añadir notas a ese informe`** · el `ya no + presente` impersonal es
  el giro de macOS para una capacidad que se acabó ("Este documento ya no está disponible", "ya no podrás acceder a
  ellos") · high. Impersonal a propósito: no hay culpable, no hay veredicto, y no aparecen `error`, `fallo` ni
  `no se pudo`, que es lo que pide la voz de Cmdr para esta clave.
- **`To get your notes to the team` → `Para hacer llegar tus notas al equipo`** · `hacer llegar` traslada el "get … to"
  sin inventar un verbo de envío que chocaría con el `envía` de la misma frase · high.
- **`already sent` → `ya envió` (pretérito), no `ya ha enviado`** · el `es` base es panregional (ver `style.md` §
  Decision points), y el pretérito es el que funciona en las dos orillas; el catálogo ya lo hace en
  `fileExplorer.navigation.useSavedPasswordMessage` ("la contraseña que macOS ya guardó") y en
  `crashReporter.dialog.body.ended` ("se cerró inesperadamente") · high. El compuesto peninsular queda registrado como
  la variante que pediría un futuro `es-ES`.
- **`attach your email` → `adjunta tu correo`** · lo pide la coherencia con la casilla que está justo debajo en el mismo
  diálogo (`common.attachEmail`, "Adjuntar mi correo electrónico … para que puedas responderme") y con
  `settings.updates.attachEmailToReports.label` · high. `adjuntar` para el sentido "incluir con un mensaje" ya estaba
  fijado por la terminología de Microsoft (§ pase de Ask Cmdr).
- **`and it'll join what the team already has` → `y se sumará a lo que el equipo ya tiene`** · `sumarse a` evita repetir
  `añadir` por tercera vez en dos frases y no obliga a decidir si el sujeto es la nota o el correo (ambos singulares,
  así que el verbo concuerda con cualquiera de los dos) · high.
- **`What was sent` → `Lo que se envió`** · calco exacto del hermano `errorReporter.dialog.detailsToggle` ("Lo que está
  a punto de enviarse") pasado a pasado, que es justo el contraste que hace el inglés · high.
- **`View or add notes to the report` → `Ver o añadir notas al informe`** · las dos mitades se conservan, que es lo que
  pide la clave. `View` → `Ver` y `Show` → `Mostrar` es la partición del catálogo (`menu.file.view` = `Ver`,
  `commands.fileShowInFinder` = `Mostrar en el Finder`) · high. 29 caracteres frente a los 31 del inglés: no crece, así
  que sigue cabiendo junto a `Cambiar ajustes` en el aviso.
- **`Note added to your report.` → `Nota añadida a tu informe.`** · el participio concuerda con `nota` (femenino), no
  con la persona, así que no expone ningún género · high.
- Ningún valor lleva apóstrofo, así que no hay nada que duplicar para ICU; `{error}` va literal en
  `No se pudo añadir tu nota: {error}`, con el mismo molde que `errorReporter.dialog.sendFailedToast`. Los once difieren
  del inglés, así que ninguno necesita `sameAsSourceJustification`.

## El diálogo de seleccionar / deseleccionar archivos (`selection.*`, 2026-08-29)

Fuentes de la tanda: macOS 26 Finder `es` (`MenuBar.json`, ids `172.title` / `300488.title`), terminología de Microsoft
`es`, Total Commander `es` (`WCMD.LNG.utf8` 7603/7604/7613/7614, `WCMD.INC.utf8` 522/524/3304-3316) y Double Commander
`es` (`doublecmd.po`, `&Unselect All`). El diálogo es ICU, así que los apóstrofos irían dobles; ningún valor de la tanda
lleva ninguno.

- **select → `Seleccionar`; deselect → `Deseleccionar`** · macOS Finder da `Seleccionar todo` (`172.title`); para el
  contrario **el Finder NO da un verbo**: dice `No seleccionar nada` (`300488.title`), una frase de alcance total que no
  sirve para "deseleccionar los archivos que coincidan". La terminología de Microsoft tampoco da verbo simple
  (`deselect` → `anular la selección`, todas las regiones). El verbo viene de la familia ortodoxa de dos paneles, que es
  justo la superficie de Cmdr: Total Commander `es` rotula el mismo diálogo `Seleccionar por nombre/extensión:` /
  `&Deseleccionar por nombre /extensión:` y los botones `&Seleccionar` / `&Deseleccionar`, y Double Commander `es` dice
  `&Deseleccionar todo`. · high para `Seleccionar` (Tier 1); **`Deseleccionar` es high dentro de la familia ortodoxa
  (Tier 3) pero no tiene respaldo Tier 1**, así que queda anotado: si algún día se revisa, la alternativa con respaldo
  Tier 2 sería `Anular la selección de…`, demasiado larga para un título y un botón.
- **Los tres sitios que nombran el diálogo tienen que decir lo mismo**: `menu.select.files` /
  `menu.select.deselectFiles` (`Seleccionar archivos…` / `Deseleccionar archivos…`),
  `commands.selectionSelectFiles.label` / `commands.selectionDeselectFiles.label`,
  `settings.selection.recentSelections.maxCount.description` (`el diálogo Seleccionar / Deseleccionar archivos`) y ahora
  los títulos `selection.dialog.title.add` / `.remove`. El bug que arregla esta tanda era justamente que el título no
  coincidía con el menú que lo abre · high.
- **`Select these files` → `Seleccionar estos archivos`; `Deselect these files` → `Deseleccionar estos archivos`** ·
  mismo par de verbos que los títulos, en infinitivo (convención de botón del `style.md`) · high.
- **`… in the focused pane` → `… en el panel activo`** · `panel activo` es la forma ya publicada del catálogo
  (`commands.navGoToPath.description` "Lleva el panel activo a…", `commands.favoritesAdd.description` "la carpeta actual
  del panel activo") · high. **Los tooltips empiezan literalmente por el texto del botón** y solo le añaden el
  complemento (`Seleccionar estos archivos en el panel activo`): el botón y su tooltip se leen como una sola frase, y
  reordenarlos rompería esa lectura.
- **`Press Enter to filter` → `Pulsa Intro para filtrar`** · calco del hermano `search.runHint`
  (`Pulsa Intro para buscar`), que ya fija `press → pulsar` e `Enter → Intro` (§ tanda de `commands.json` +
  `queryUi.json`) · high para la estructura; `Intro` sigue `tentative` como nombre de tecla (convención de teclado de
  Apple, sin hit directo en el corpus).
- **`recent selections` → `selecciones recientes`** · ya publicado en
  `settings.selection.recentSelections.maxCount.label` (`Selecciones recientes que recordar`) · high. Los cinco textos
  del popover copian la gramática y el registro de sus gemelos de búsqueda `queryUi.recent.*`, cambiando `búsquedas` por
  `selecciones`: `Mostrar todas las selecciones recientes`, `Todas las selecciones recientes`,
  `Filtrar selecciones recientes`, `Ninguna selección reciente coincide con ese filtro.`, `Selecciones recientes`.
- **`selection.recent.popoverAria` y `.listboxAria` comparten el mismo inglés (`Recent selections`)**, así que tienen
  que decir exactamente lo mismo en `es` o salta `i18n-terms`. Ambas: `Selecciones recientes`.
- **`Apply recent {mode} selection: {query}` → `Aplicar selección {mode} reciente: {query}`** · calco del molde ya
  publicado en `search.recent.runAria` (`Ejecutar búsqueda {mode} reciente: {query}`) · high. `{mode}` llega ya
  traducido (`IA`, `Regex`, `Nombre de archivo`) y `{query}` es texto libre del usuario: el molde los deja a los dos en
  posición neutra, sin concordancia que resolver.
- **`Matching what is shown in the list (the full path).` → `Coincide con lo que muestra la lista (la ruta completa).`**
  · `coincidir` es el verbo del catálogo para "match" (`commands.selectionSelectFiles.description` "los archivos
  coincidentes") y `ruta completa` ya está fijado (`fileOperations.validation.pathTooLong`) · high. Sujeto tácito (el
  patrón), que es lo que mantiene el aviso corto y tranquilo en vez de sonar a advertencia.

## El nombre accesible tiene que contener la etiqueta visible (WCAG 2.5.3, 2026-08-30)

`desktop-i18n-aria-label` exige que un valor `*Aria` contenga literalmente su etiqueta visible (se ignoran mayúsculas,
puntuación y espacios). Quien usa control por voz dice lo que LEE, así que el camino correcto es dar a la ETIQUETA la
forma que la frase accesible ya usa de forma natural, en vez de forzar la frase accesible.

- **case-sensitive → `Distinguir mayúsculas y minúsculas`** (forma completa, la de macOS) · macOS es
  (`Distinguir mayúsculas y minúsculas` en el panel de búsqueda de AppKit; la negativa es
  `Ignorar mayúsculas/minúsculas`), verificado en el montón de referencia 2026-08-30 · `high`. La forma recortada
  `Distinguir mayúsculas` queda descartada: `viewer.search.caseSensitive` ya publicaba la completa, así que el mismo
  interruptor se llamaba de dos maneras según dónde lo abrieras. En texto corrido el adjetivo va en tercera persona y
  minúscula (`queryUi.recent.caseSensitive` = `distingue mayúsculas y minúsculas`), porque se suma a una lista de datos
  separados por comas.
- **El nombre accesible envuelve la etiqueta, no la reformula**: `queryUi.scope.toggle.caseSensitiveAria` =
  `Distinguir mayúsculas y minúsculas al buscar`. Un `Coincidencia que distingue…` cambia el verbo a tercera persona y
  rompe la regla de contención.

## Una palabra inglesa, una palabra española: la revisión de deriva (2026-08-30)

El catálogo llevaba 36 sitios donde `es` daba dos nombres distintos al mismo texto inglés, casi siempre porque una
pasada tardía tocó `menu.json` y dejó `commands.json` con la redacción vieja. Veintitrés eran deriva de verdad y ya no
están; las trece restantes son fronteras DELIBERADAS y quedan anotadas abajo para que la próxima pasada no las
"unifique".

### Resuelto

- **`error report` es `informe de error` también en el menú Ayuda y en la paleta** ·
  `commands.helpSendErrorReport.label` y `menu.help.sendErrorReport` decían `Enviar informe de fallos…`, que es el
  término de CRASH report (glosario: crash report → `informe de fallos`, error report → `informe de error`). O sea: el
  menú prometía un informe de fallos y abría un diálogo titulado `Enviar informe de error`, y de paso llamaba "fallo" a
  algo que no lo es · `high`. La entrada del glosario ya avisaba de esta grieta; ahora está cerrada.
- **`Zoom in` / `Zoom out` → `Aumentar el zoom` / `Reducir el zoom`**, en el menú y en la paleta · `menu.zoom.in`/`.out`
  decían `Ampliar`/`Reducir` y la paleta decía otra cosa · `high`. macOS `es` usa el escueto `Aumentar`/`Reducir`, y ahí
  funciona porque el submenú Zoom ya pone el objeto; la paleta de comandos no tiene ese contexto, así que gana la forma
  con objeto, que además concuerda con `commands.handler.zoomIncreased` (`Zoom aumentado al {size}%`).
- **`Ver > Zoom > 100%` no existía**: `commands.handler.zoomResetHintMenu` mandaba al usuario a un menú llamado `Ver`,
  pero la barra de menús se llama `Visualización` (`menu.bar.view`). Ahora la pista nombra el menú real · `high`. (El
  mismo fallo estaba en `de`, y se corrigió allí.)
- **`Go to path` → `Ir a la ruta`** en las cuatro claves · el comando y el menú decían `Ir a una ruta…` y el diálogo que
  abren se titulaba `Ir a la ruta`. macOS `es` usa el artículo determinado en toda esta familia
  (`Ir a la carpeta Documentos`, `Ir a la carpeta Aplicaciones`) · `high`.
- **`Toggle` → `Activar o desactivar …`** · `menu.context.toggleSelection` decía `Alternar selección`; macOS `es` usa
  `Activar o desactivar …` (AppKit, `Activar o desactivar el bloque de cita`) y el propio catálogo ya usa el par
  explícito en `commands.viewShowHidden.label` (`Mostrar u ocultar archivos ocultos`) · `high`.
- **`{dir}` / `{dirs}` estaba SIN TRADUCIR** en las tres claves de estadísticas de análisis
  (`fileOperations.delete.scanDir`, `.transferDialog.scanDir`, `.scanPhase.scanDir`): el usuario leía `4 dirs`. Ahora
  `carpeta`/`carpetas`, igual que `fileExplorer.summary.dirNoun` · `high`.
- **`Modified` (fecha) → `Modificación`** en las seis claves de fecha · `fileExplorer.columns.modified` y
  `.renameConflict.modified` decían `Modificado`, así que la columna de la lista de archivos y la de resultados de
  búsqueda se llamaban distinto una al lado de la otra. macOS `es` da `Modificación` y `Fecha de modificación` · `high`.
- **Sin artículo en las órdenes cortas**: `Copiar nombre de archivo` (no `Copiar el nombre del archivo`, que rompía la
  simetría con su vecino `menu.edit.copyPath` = `Copiar ruta`), `Mostrar archivos ocultos`,
  `Actualizar los hosts de red`, `Resultados de búsqueda` · `high`.
- **`Preview:` → `Vista previa:`** · `settings.appearance.datePreviewLabel` decía `Previsualización:` contra el
  `Vista previa` ya asentado · `high`.
- **`New name` → `Nombre nuevo`** · `suggestedOps.columnNewName` decía `Nuevo nombre`; el par de `askCmdr.renameReview`
  (`newName`, `editName` = `Nombre nuevo para {name}`) ya tenía la forma pospuesta · `high`.
- **Registro peninsular neutro, según `style.md`**: `Algo ha ido mal` (no `Algo salió mal`) y
  `Límite de pestañas alcanzado` (no `Se alcanzó el límite…`, que además usa el pretérito indefinido latinoamericano).
  Los dos pares de correo beta dicen ya lo mismo:
  `Vaya, no hemos podido darte de alta ahora mismo. ¿Lo intentas de nuevo?` · `high`.
- **`Stop` → `Detener`** en las tres (`search.walkHandoff.stop` decía `Parar`); **`Drive indexing` →
  `Indexación de unidades`** en las tres; **`Create new file` → `Crear archivo nuevo`** también en el nombre hablado del
  botón F7; **`Connect to server…` → `Conectarse a un servidor…`** también en el enlace de ajustes, que su propio `@key`
  mandaba traducir igual que el comando; **`Brief mode` → `Modo breve`** (mayúscula inicial solo, según la guía de
  estilo); **`Indexing now` → `Indexando ahora`** en las dos (el reflexivo `Indexándose` sobraba); **`Go to home folder`
  → `Ir a la carpeta de inicio`** también en el botón de la pantalla de error; **`This volume doesn't support trash` →
  `Este volumen no admite la papelera.`** en las dos · `high`.

### Fronteras deliberadas (no unificar)

- **`Both` concuerda en género con lo que enumera** · `queryUi.filters.type.both` = `Ambos` (archivos y carpetas, mezcla
  → masculino); `settings.…downloadsNotifications.opt.both` = `Ambas` (notificaciones, femenino). Las dos son correctas
  y ninguna sirve en el sitio de la otra · `high`.
- **`Canceled`: `Operación cancelada` titula un panel, `Cancelado` es un estado** · los títulos de
  `errors.listing.*.title` nombran el sujeto tácito (`Interrupted` → `Operación interrumpida`), mientras
  `operationLog.status.canceled` es el estado de ciclo de vida en una celda · `high`.
- **`Edit`: `Edición` es el MENÚ, `Editar` es el verbo** · macOS Finder `es` llama `Edición` a su menú Edit, y
  `@menu.bar.edit` pide esa palabra exacta · `high`.
- **`View`: `Visualización` es el MENÚ, `Ver` es la acción F3** · mismo motivo; `@menu.bar.view` pide la palabra de
  Finder · `high`.
- **`Error`: `Problema` es un estado que lee el usuario, `Error:` es una etiqueta de diagnóstico** · el `@key` de
  `fileExplorer.network.browser.status.error` pide evitar la palabra literal, y el de `settings.updates.errorPrefix`
  dice que ahí sí vale · `high`.
- **`Modified`: `Modificación` es la FECHA, `Modificados` son los atajos que tú cambiaste** ·
  `shortcuts.section.filterModified` filtra comandos cuyo atajo modificó el usuario, sin fecha ninguna, y concuerda en
  masculino plural con ellos · `high`.
- **`Search`: `Buscar` es la acción, `Búsqueda` es el tema** · en español el infinitivo titula diálogos y botones
  (`search.dialog.title`, `queryUi.bar.runLabel`), pero un apartado de la barra lateral de Ajustes es un sustantivo:
  `Buscar` ahí se leería como una orden · `high`.
- **`Put back …`: `restaurar` son NOMBRES, `devolver a su sitio` son SITIOS** · el inglés reutiliza una frase para dos
  deshacer distintos; macOS Finder `es` llama `Devolver` al Put Back de la papelera · `high`.
- **`you@example.com` → `tu@example.com`, la misma en los tres campos** · `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder` y `onboarding.stepBeta.emailPlaceholder` llevan la misma dirección, y sus `@key` lo
  exigen · `high`. Se traduce la parte local (`tu@`) y se conserva el dominio `example.com`. ❌ Nada de `ejemplo.com`:
  es un dominio real y registrable, mientras que `example.com` está reservado para ejemplos (RFC 2606).
- **Cuatro "divergencias" que no lo son, y por qué siguen apareciendo**: `Connected` / `Connected!`, `Copied` /
  `Copied!`, `Send report` / `Send report?` y `Start using Cmdr` / `Start using Cmdr!` tienen inglés DISTINTO (la
  exclamación o la interrogación está en el original, y los `@key` la piden). `i18n-terms` las agrupa igual porque su
  normalizador quita la puntuación FINAL pero no la apertura española `¡` / `¿`, así que `¡Copiado!` se reduce a
  `¡Copiado` y ya no coincide con `Copiado`. No toques estos ocho valores: el fallo está en el normalizador, no en la
  traducción.

## Palabras que se separaron sin que ningún check pudiera verlo (2026-08-30)

`i18n-terms` solo agrupa claves cuyo inglés es IDÉNTICO. Las de abajo tienen un inglés ligeramente distinto, así que
solo aparecen en la pasada manual. Todas están corregidas.

- **`Parent folder` → `Carpeta superior` también en la barra de menús** · `menu.go.parentFolder` decía
  `Carpeta contenedora`, o sea que el menú Ir y la paleta de comandos (`commands.navParent.label` =
  `Ir a la carpeta superior`) nombraban distinto la misma acción, y las otras diez claves del catálogo también dicen
  `carpeta superior` · `high`. Contraprueba, para que nadie la vuelva a descubrir: macOS `es` SÍ dice
  `Carpeta contenedora` / `Ir a la carpeta contenedora`, pero eso traduce el inglés de Apple _Enclosing Folder_, que no
  es la frase de Cmdr (`Parent folder`), así que la regla de "usa la palabra de Finder" no se aplica limpiamente aquí y
  gana la coherencia interna.
- **`default` → `por omisión`, sin excepciones** · cinco claves decían `predeterminado`
  (`settings.appearance.language.opt.system`/`.opt.systemWithLanguage`/`.description`,
  `settings.appearance.dateTimeFormat.opt.system`, `settings.network.smbConcurrency.description`) contra las quince que
  ya decían `por omisión` · `high`. macOS `es` no usa `predeterminado` en ningún sitio (cero coincidencias en
  `es/macOS/`; sí `Ajuste por omisión`, `Restaurar ajustes por omisión`, `Usar por omisión`), y el glosario ya lo tenía
  decidido dos veces.
- **`Commercial` se había quedado en inglés en los nombres de tipo de licencia** ·
  `licensing.dialog.typeCommercialPerpetual` decía `Commercial perpetua` y `.typeCommercialSubscription`
  `Suscripción Commercial`, en la misma pantalla donde `licensing.about.perpetual` dice `Licencia comercial perpetua` ·
  `high`. Regla: **sigue la mayúscula del inglés**, que ya distingue el nombre del NIVEL de la descripción. Inglés
  `Commercial license` → `licencia Comercial`; inglés `commercial subscription` → `suscripción comercial`.
- **`rename` (verbo) → `renombrar`, incluso en prosa** · `errors.mutation.cantRenameVolumeRoot` y
  `askCmdr.renameUndo.applied` usaban la perífrasis `cambiar el nombre`, que el glosario reserva para el SUSTANTIVO
  (`cambio de nombre`) · `high`.
- **`entries` → `entradas` y `dirs` → `carpetas`** · `indexing.scan.counters` decía
  `{entriesText} ítems, {dirsText} dirs`: dos palabras distintas para lo mismo que `queryUi.results.indexReadyStatus`
  llama `entradas`, y un `dirs` sin traducir · `high`.
- **`permanently` → `permanentemente`** · `errors.write.trashNotSupported.suggestion` decía `de forma permanente` ·
  `high`. La etiqueta corta de la barra de teclas F sigue siendo `Permanente`: es el hueco más estrecho de la interfaz y
  ahí cabe solo el adjetivo.
- **`trash` → `papelera` en minúscula dentro de una frase** · `fileExplorer.renameConflict.overwriteTrash` decía
  `a la Papelera`; macOS `es` escribe `Trasladar a la papelera`, `Ir a la papelera`, `Vaciar papelera`, y reserva la
  mayúscula para cuando `Papelera` va sola como nombre del sitio · `high`.
- **`Coming soon` → `Próximamente`** · `settings.mediaIndex.clip.comingSoon` decía `Muy pronto` · `high`.

Revisado y NO tocado a propósito: `Queued` → `Esperando` (`operationLog.status.queued`) concuerda con la rama `queued`
de `queue.row.status`, así que las dos superficies que muestran ese estado ya dicen lo mismo; `Cola` traduce el
sustantivo `Queue`, que es otra cosa.

## Los nombres de los paneles salen ahora del Mac de quien usa Cmdr (`errors.git.*`, `errors.provider.*`, 2026-08-30)

Ocho valores llevaban los nombres de los paneles escritos a mano. Ahora llevan los marcadores `{system_settings}`,
`{privacy_and_security}` y `{files_and_folders}`, que la app sustituye en tiempo de ejecución por los nombres tal como
los muestra el Mac de quien la usa. Los valores son RAW (no ICU), así que las comillas simples no se duplican.

- **Una preposición puede ir delante; un artículo o una contracción, no** · el valor es desconocido al escribir, así que
  `en {system_settings}` está bien y cualquier cosa que tuviera que concordar con él, no.
  `errors.provider.iCloud.serious` y `.transient` evitan además el `en … en …` doble: ahora abren con
  `Abre {system_settings}, …` · `high`.
- **`Apple Account` → `Cuenta de Apple`, `General` → `General`, `Login Items & Extensions` →
  `Ítems de inicio y extensiones`** · ningún marcador los cubre, así que son texto normal; macOS 26 `es`
  (`AppleIDSettings.appex`, `InfoPlist.loctable`, `CFBundleDisplayName`; `LoginItems.appex`, `Localizable.loctable`;
  verificado en macOS 26.6.2, build 25G83, 2026-08-30) · `high`. Coincide con lo que ya dice
  `errors.listing.diskFullErrno.suggestion` (`**{system_settings} > General > Almacenamiento**`).

## `Restaurar` nombra ahora el objeto: el nombre anterior (`askCmdr.renameUndo.undone` / `.partial`, 2026-08-30)

El inglés compartía una frase con el deshacer de la papelera ("Put back {countText} {files}.") y ahora dice QUÉ vuelve:
el nombre anterior.

- **`Put the old names back on N files.` → `Se restauraron los nombres anteriores de N archivos.`** · `restaurar` es lo
  que el catálogo reserva para deshacer un renombrado (`askCmdr.renameUndo.undoing`, "Restaurando los nombres
  anteriores…"; `askCmdr.renameUndo.unavailable`, "No hay nada que restaurar."), y `devolver … a su sitio` sigue siendo
  el de la papelera (`fileOperations.trash.undone`) · `high`. El verbo concuerda con el nombre, no con el archivo, así
  que la rama `one` dice `Se restauró el nombre anterior de 1 archivo.`

## Una operación revertida a medias: terminar la reversión (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

- **`Finish rolling back` → `Terminar de revertir`** · se queda dentro de la familia que el catálogo ya usa para esta
  función (`operationLog.dialog.rollBack` = `Revertir`, `rollingBack` = `Revirtiendo`, `rolledBack` = `Revertido`,
  `partiallyRolledBack` = `Revertido en parte`) · high. La perífrasis `terminar de` + infinitivo dice "acabar lo que
  quedó a medias" y nunca "empezar de nuevo"; `Completar la reversión` era la alternativa, más larga y más formal para
  un botón dentro de una fila. El valor es idéntico en `operationLog.dialog.finishRollBack` y
  `fileOperations.rollbackConfirm.finishRollBack` (mismo inglés, o salta `i18n-terms`).
- **`Finish rolling this back?` → `¿Terminar de revertir esta operación?`** · calcado del hermano
  `fileOperations.rollbackConfirm.title` (`¿Revertir esta operación?`), mismo registro y misma forma de pregunta en
  infinitivo · high. El `this` del inglés se resuelve como `esta operación`, igual que hace el hermano.
- **El aviso repite literalmente el molde de `fileOperations.rollbackConfirm.bodyUndoByDeleting`** · "Cmdr revirtió lo
  que pudo y dejó el resto como estaba. Si terminas la reversión, Cmdr repasa la operación otra vez y vuelve a omitir
  todo aquello de lo que sigue sin estar seguro." `omite todo aquello de lo que no está seguro` viene de ahí,
  `reversión` de `transferProgress.rollbackUnavailableTooltip` y `refusalUnexpected`, y `como estaba` de
  `refusalAlreadyRolledBack` (`Esta ya está como estaba antes.`) · high. Pretérito (`revirtió`, `dejó`) por la regla de
  la acción recién terminada del `style.md`. El `Si terminas…` convierte el gerundio inglés ("Finishing takes another
  pass") en trato directo `tú`, que es lo que pide el `style.md`; la frase sigue sin prometer una reversión completa.
- **`in {folder}` → `en “{folder}”`** · macOS `es` escribe así el nombre de una carpeta o un elemento dentro de una
  frase (`en “^0”`, 14 apariciones en Finder/AppKit; también `en la carpeta “%@”`) · high. Las comillas son funcionales:
  marcan el nombre como cita para que "Eliminando lo creado en “Backup”" no se lea como que la carpeta misma se va. Sin
  artículo ni concordancia, así que cualquier nombre encaja; `en la carpeta {folder}` diría lo mismo pero ocupa más en
  una línea que el label ya comparte con el progreso.
- No hace falta `sameAsSourceJustification`: los cinco valores se diferencian del inglés.

## El aviso de una reversión que no lo pudo todo (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`, 2026-08-31)

Superficie nueva: el usuario para una copia o un movimiento con `Revertir`, y cuando la reversión termina aparece un
aviso de hasta tres partes: un titular (lo que la reversión sí consiguió), la línea `leftBehind` (que prepara al
usuario), y una lista de motivos, cada uno en dos versiones, una que NOMBRA el elemento (`*.named`) y otra que los
CUENTA (`*.counted`). El tono es siempre "Cmdr hizo lo prudente", nunca una disculpa ni una alarma.

- **La familia calca el molde ya asentado de `askCmdr.renameUndo.skipReason.*`** · las dos superficies son el mismo
  gesto (deshacer algo y omitir lo que no se puede verificar), y el usuario puede ver las dos, así que los motivos
  comparten fórmula: `{name} se quedó como está: <motivo>.` / `{countText} elementos se quedaron como están: <motivo>.`
  · high. Las dos claves de `folderNotEmpty` tienen el MISMO inglés que sus hermanas de `askCmdr`, así que el valor es
  idéntico palabra por palabra (o salta `i18n-terms`).
- **"Left {name} alone" → `{name} se quedó como está`; "Left {name} where it is" → `{name} se quedó donde está`** · el
  inglés distingue los dos casos (no lo tocamos / no lo movimos de sitio) y el español los distingue igual. `se quedó`
  pone el elemento de sujeto, que es lo que evita el clítico con género · high.
- **Dos restricciones a la vez: la marca se queda Y nada concuerda con `{name}`.** El elemento puede ser un archivo
  (masculino) o una carpeta (femenino), y `{name}` llega tal cual del disco, así que ningún clítico, artículo ni
  participio puede referirse a él; y `Cmdr` es palabra de marca, así que tiene que aparecer literal en el valor
  (`desktop-i18n-dont-translate` avisa si se cae). Por eso `it changed after Cmdr put it there` no admite ni
  `lo dejara ahí` (concuerda) ni una reescritura sin la marca (`cambió después de llegar ahí`, que deja la frase
  diciendo solo que el archivo cambió en algún momento, y ahí se pierde el motivo por el que Cmdr no lo toca). La
  solución deja el elemento de sujeto del cambio y mete a Cmdr en una subordinada con lugar en vez de objeto:
  **`cambió después de que Cmdr terminara de escribir ahí`** · `escribir` es el verbo que la familia ya usa para lo que
  Cmdr deja en el destino (`transferProgress.rollbackTooltip`, "los archivos escritos hasta ahora") y `terminar de` +
  infinitivo ya está asentado en `operationLog.dialog.finishRollBack` ("Terminar de revertir") · high. La versión
  `counted` repite la misma fórmula aunque ahí sí se conozca el género (`elementos`), porque las dos se leen seguidas.
- **"something else now sits where it came from" → `ahora hay otra cosa en su sitio`** · `su sitio` es justo lo que el
  catálogo ya usa para "el lugar al que pertenece" (`fileOperations.trash.undone`, "devolver … a su sitio"), y sirve
  igual para uno que para varios, que es lo que permite que `named` y `counted` compartan la segunda mitad. La
  alternativa `en el lugar del que salió` (de `rollbackConfirm.bodyUndoByMovingBack`) dice lo mismo pero ocupa el doble
  en una línea de lista · high.
- **"Couldn't undo {name}" → `Cmdr no pudo revertir {name}`** · este motivo es el único de la lista que NO es una
  decisión de Cmdr (la unidad dijo que no), y el inglés lo marca rompiendo el molde: frase propia, sin
  `se quedó como está`. El español rompe igual. `revertir` es el verbo de la función (`operationLog.rollback.*`), y
  nombrar a Cmdr como sujeto es lo que ya hace `refusalUnexpected` ("Cmdr no pudo iniciar la reversión") · high por
  consistencia, pero el objeto es nuevo: hasta ahora se revertía una OPERACIÓN, y aquí se revierte un elemento suelto,
  igual que el inglés estira su "undo {name}". Si una revisión nativa lo ve forzado, la alternativa es
  `Cmdr no pudo deshacer lo hecho con {name}`, más larga y más vaga.
- **"Its drive may be disconnected or read-only." → `Puede que su unidad no esté conectada o sea de solo lectura.`** ·
  la primera mitad es literal de `fileOperations.trash.undoUnavailable` ("que su unidad no esté conectada");
  `de solo lectura` es macOS `es` (Finder PE45, "el disco es de solo lectura"; AppKit "un volumen de solo lectura") ·
  high.
- **"Stopped after …" → `La reversión se detuvo después de …`** · el inglés no tiene sujeto y el español lo necesita, y
  el sujeto natural es `la reversión`, la palabra que el catálogo ya usa para esta operación
  (`transferProgress.rollbackUnavailableTooltip`). `detener` es el verbo de macOS `es` para parar una operación
  ("Detener copia", "Detener"), y Double Commander traduce "Stopped" por "Detenido" · high. macOS `es` no usa `tras` en
  ninguna de sus 11.676 cadenas, así que la subordinada va con `después de` (2026-08-31).
- **"The rest are still there." → `El resto sigue ahí.`** y **"The rest stayed where the move put them." →
  `El resto se quedó donde lo dejó el movimiento.`** · `el resto` (macOS `es`, "el resto de la frase", "Cerrar el resto
  de pestañas") en vez de `los demás`, que en macOS `es` casi siempre son PERSONAS ("los demás ya no tendrán acceso").
  El sustantivo de la operación es `el movimiento`, el que ya usa el catálogo (`errors.write.cancelled.title.move`,
  "Movimiento cancelado"; `queue.failureToast.title`), no `traslado` · high.
- **"the {countText} items" (titular con artículo determinado) pierde el numeral en la rama `one`.** `el 1 elemento` es
  agramatical en español, así que la rama `one` dice `Se eliminó el elemento que Cmdr había creado.` y el numeral solo
  aparece en `many`/`other`. Precedente en el propio catálogo: `transferProgress.titleReversalDeleting` ("Deleting the
  file it created" → "Eliminando el archivo creado") · high. `desktop-i18n-parity` compara el CONJUNTO de placeholders
  del valor entero, así que `{countText}` sigue presente y la comprobación pasa.
- **El artículo determinado es lo único que separa `done*` de `some*`, y hay que conservarlo.** `doneDeleting` /
  `doneMovingBack` dicen "los {countText} elementos" (fueron todos) y `someDeleted` / `someMovedBack` dicen "{countText}
  elementos" (solo esos), igual que el inglés con su "the". Si se pierde el artículo, el aviso parcial promete que el
  destino quedó limpio, que es justo lo que la clave no debe decir · high.
- **"Cmdr had written" (elementos, no solo archivos) → `que Cmdr había creado`** · aquí `item` incluye las carpetas que
  la operación hizo, y en español no se "escribe" una carpeta. El catálogo ya usa `crear` para esto
  (`rollbackConfirm.bodyUndoByDeleting`, "los archivos y carpetas que creó la operación";
  `transferProgress.titleReversalDeleting`, "los {countText} archivos creados") · high. `escrito` se queda para cuando
  la fuente habla solo de archivos (`transferProgress.rollbackTooltip`, "los archivos escritos hasta ahora").
- **Los dos verbos de la reversión no se mezclan nunca**: la reversión de una COPIA `elimina`
  (`Se eliminaron los … elementos`) y la de un MOVIMIENTO `devuelve … a su sitio`
  (`Se devolvieron los … elementos a su sitio`). Son los verbos ya asentados (`operationLog.summary.delete`,
  `fileOperations.trash.undone`), y el aviso nunca los intercambia: el usuario tiene que poder leer del titular si algo
  se borró o solo volvió a su carpeta · high.
- **`leftBehind` cierra con dos puntos y nombra el sustantivo**: "Cmdr omite todo aquello de lo que no está seguro, así
  que estos elementos se quedaron donde están:". La primera mitad es literal de `rollbackConfirm.bodyUndoByDeleting`, y
  el español añade `elementos` porque un `estos` a secas queda desnudo delante de una lista · high.
- **`rollbackConfirm.body` recupera la tercera promesa.** El inglés cambió y ahora admite que la reversión puede dejar
  cosas, así que el valor termina con la misma frase que sus hermanas `bodyUndo*`: "Cmdr omite todo aquello de lo que no
  está seguro, así que puede que quede alguno." Las tres frases van sueltas (el inglés une las dos primeras con "and");
  en español se leen mejor separadas · high. `ha escrito` se queda en perfecto compuesto porque la operación sigue en
  marcha, que es la excepción a la regla del pretérito del `style.md`.
- **En las claves `*.counted` el verbo va en plural FUERA de las ramas, y la rama `one` no se usa.** Una clave `counted`
  solo aparece con dos o más (lo dice su `@key.description`), así que el único contenido que cambia por rama es el
  sustantivo contado, igual que en las hermanas de `askCmdr.renameUndo.skipReason.*`. Es la excepción consciente a la
  regla del `style.md` de meter la frase entera en las ramas: `folderNotEmpty.counted` tiene que ser idéntica a su
  hermana de `askCmdr` (mismo inglés), así que arreglar la concordancia de una rama muerta rompería la simetría de la
  familia sin cambiar ni un píxel en pantalla. Si algún día una `counted` puede valer 1, hay que rehacer las cinco a la
  vez.
- No hace falta `sameAsSourceJustification`: los 18 valores difieren del inglés.
## La pantalla de bloqueo por WebKit antiguo (`main.oldWebkit.*`, 2026-09-02)

Tres cadenas que Cmdr muestra en lugar de su interfaz cuando el Safari del Mac es demasiado antiguo. Viven en el
armazón HTML, no en la app, así que son lo único que esa persona verá de Cmdr.

- **`Software Update` → `Actualización de software`** · macOS lo nombra así en Ajustes del Sistema; el rastro Tier 1 de
  Finder confirma el término (`Apple Device Software Update File` → `Archivo de actualización de software del
  dispositivo Apple`) · `high`.
- **`Quit` → `Salir`** · clave `Quit` de AppKit en macOS → `Salir` · `high`. Entra también en el glosario general, que
  hasta ahora no la recogía.
- **`o posterior`, no `o más nuevo`**, para «or newer» hablando de versiones: es la fórmula que usa Apple.
- **`Safari`, `Mac` y `15.4` se quedan tal cual.** `Safari` ya está en `BRAND_WORDS`.
