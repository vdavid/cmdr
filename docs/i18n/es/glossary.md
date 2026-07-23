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
- queue (transfer queue) → cola · double-commander ("Queue"→"En cola"); macOS print "cola"; Total Commander "Adm. de
  transf. en segundo plano". "Transfer queue" → "Cola de transferencias"; per-row/dialog "Queue" button
  (send-to-background) → "Cola" · high
- queued / waiting (queue status) → Esperando · matches the existing "Esperando…" waiting precedent in
  `fileExplorer.json`; the row sits behind another transfer on the same drive · high
- background / send to background → en segundo plano · macOS/MS/Total Commander standard (already in glossary); "Send to
  the transfer queue" → "Enviar a la cola de transferencias", "keep running in the background" → "mantener … en
  ejecución en segundo plano" · high
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
