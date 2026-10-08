# es review queue

Open questions for David or a future native Spanish reviewer. Not translator input: every item below already ships a
reasoned value, recorded in `terms.json` or `decisions.md`. Remove an item once it's settled, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## For David

- **crash report → `informe de fallos`** (tentative): no canonical source. `fallos` is the gentlest fit for the
  non-alarmist voice; alternatives are `informe de bloqueos` (matches MS/macOS `bloqueo`) or the generic
  `informe del problema`.
- **Android platform tools → `herramientas de plataforma de Android`** (tentative): Google's own string is
  `Herramientas de la plataforma del SDK de Android`, too long for a settings row. Used in
  `settings.fileOperations.adbEnabled.description`, `settings.fileOperations.adbBinaryPath.description`,
  `settings.adb.install.intro`, and `adb.connect.adbNotInstalled`; all move together.

## Terms

- **`Deseleccionar`** has Tier-3 backing only (Total/Double Commander); macOS has no verb, MS says
  `anular la selección`, too long for a title and a button.
- **encrypted → `cifrado`** over the pile's single stale `Encriptado`.
- **merge → `fusionar`** over Nautilus's `Mezclar`.
- **onboarding → `introducción`**: no macOS source.
- **Brief / Full view → `vista breve` / `vista completa`**: Cmdr's own mode names, now lowercase in prose.
- **`Anclar` vs `fijar`** for pin: Safari says `Anclar pestaña`; the catalog shipped `fijar` across five keys.
- **host → `host`** kept as-is (no `anfitrión` in the pile).
- **home folder → `carpeta de inicio`**: composed from macOS `Inicio`.
- **Enter → `Intro`**: Apple's Spanish keyboard labelling, no direct string hit.
- **zoom in / out → `Aumentar el zoom` / `Reducir el zoom`**: Safari's bare `Ampliar` / `Reducir` only work inside a
  Zoom submenu.
- **redact → `depurar`** and debug → `depuración` share a root; the object tells them apart today
  (`los registros se depuran` vs `nivel de depuración`). Worth a second look if a string ever needs both.
- **`Encontrado cerca`, `Último uso`, `De confianza desde`**: composed; the last is three words in a narrow row.
- **`Cómo se hace`** (the ADB hint's link) over `Más información`.
- **`apertura`** in `adb.connect.cancelled` (`Detuviste la apertura de tu teléfono.`): no other key uses the noun.
- **`typo` → `errata`**, **`dumber` → `más torpe`**, **`signup server` → `servidor de altas`**: unsourced.
- **Terminal's article**: the catalog writes `En la Terminal` 24 times and `en Terminal` five; Apple's Spanish help says
  `Abrir en Terminal` with no article. Settle one form.

## Surfaces

- **`a {destination}` with no object** (`fileOperations.operationConflict.context`): "Copiando a Ana" can momentarily
  read as a personal `a`.
- **The online-only delete banner** (`fileOperations.delete.cloudOnlineOnly*`): long, in a narrow strip above the file
  list; never reviewed by a human. Check for overflow.
- **`Cmdr no pudo revertir {name}`** (`fileOperations.cancelRollback.reason.failed.named`): reverting a single item
  stretches the verb; the fallback is `Cmdr no pudo deshacer lo hecho con {name}`.
- **`En segundo plano`** (the empty-queue button): if a reviewer finds it too elliptical, `Pasar a segundo plano` is the
  reserve (21 characters on a button whose other state says `Cola`).
- **`Entrar como invitado`** (`fileExplorer.network.share.useGuest`): English "Use guest" has no macOS twin;
  `Usar acceso de invitado` is the reserve if `Entrar` reads as a fresh sign-in rather than a switch back.
- **`Sin conexión con {name}`** (`servers.paneState.notConnected`): must not read as an error;
  `Aún no te has conectado a {name}` is the reserve.
- **`herramientas de plataforma de Android`** (`settings.adb.install.intro`,
  `settings.fileOperations.adbEnabled.description`, `adb.connect.adbNotInstalled`): Google's Spanish docs
  (machine-translated) say `Herramientas de la plataforma del SDK`; a native reader should say whether the catalog
  should follow that wording.
- **S3 console labels** (`servers.sheet.s3AccountIdHelp`, `servers.sheet.s3GcsKeyHelp`, `servers.sheet.s3PathStyle`):
  `página de información general` (Cloudflare's Overview), `pestaña Interoperabilidad` /
  `configuración de Cloud Storage` (Google), and `direccionamiento de estilo de ruta` (AWS's path-style) weren't checked
  against the live Spanish consoles; a reviewer with access should confirm them.
- **Renombrado múltiple** (`multiRename.*`): `Quitar diacríticos` (`multiRename.removeDiacritics`, the preset of the
  same name) is accurate (ž, ß too) but bookish; `Quitar acentos` is the casual reserve if a reader finds it clear
  enough. `marcador` for a mask token (`Insertar marcador`, `A un marcador le falta el ] de cierre`) is tentative;
  Microsoft's full `marcador de posición` is the reserve.
