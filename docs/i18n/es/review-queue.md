# es review queue

Open questions for David or a future native Spanish reviewer. Not translator input: every item below already ships a
reasoned value, recorded in `terms.json` or `decisions.md`. Remove an item once it's settled, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## For David

- **Primary audience: Spain or Latin America?** The base is neutral peninsular-leaning (`style.md` § Decision points).
  The answer decides `coste` vs `costo`, `ordenador` vs `equipo` vs `computadora`, `añadir` vs `agregar` (LatAm macOS
  says Agregar), and whether an `es-419` variant is worth opening.
- **"your computer/machine" → `tu equipo` vs `ordenador`**: macOS `es` writes `ordenador` (43 against 1 `equipo` in
  Finder + AppKit + SystemSettings, 2026-09-09), but it's Spain-only; new copy says `tu equipo` (Microsoft's word,
  already in `errors.json`). Still on `ordenador`: `settings.mediaIndex.privacyNote`, `settings.ai.tooltipLocal`,
  `errors.listing.hostDown.explanation`, `errors.listing.deviceProblem.suggestion`. One word, one sweep once decided.
- **crash report → `informe de fallos`** (tentative): no canonical source. `fallos` is the gentlest fit for the
  non-alarmist voice; alternatives are `informe de bloqueos` (matches MS/macOS `bloqueo`) or the generic
  `informe del problema`.
- **Android platform tools → `herramientas de plataforma de Android`** (tentative): Google's own string is
  `Herramientas de la plataforma del SDK de Android`, too long for a settings row. Used in
  `settings.fileOperations.adbEnabled.description`, `settings.fileOperations.adbBinaryPath.description`,
  `settings.adb.install.intro`, and `adb.connect.adbNotInstalled`; all move together.
- **full disk access in running prose**: the name is now Apple's `acceso total al disco`, but the FDA onboarding step's
  prose (`onboarding.stepFda.revoked.noAccess` and siblings, `askCmdr.wake.needsFullDiskAccess`,
  `search.coverage.setUpFullDiskAccess`) still says `acceso a todo el disco`. Sweep it onto the pane name, or keep the
  split between naming and describing?

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
- **The beta signup lines** (`onboarding.stepBeta.signup.failure`, `settings.updates.emailSignupError`) say
  `no hemos podido darte de alta ahora mismo`, a peninsular compound perfect in David's voice; `no pudimos` would follow
  the preterite rule but reads odd beside `ahora mismo`.

## Surfaces

- **`a {destination}` with no object** (`fileOperations.operationConflict.context`): "Copiando a Ana" can momentarily
  read as a personal `a`.
- **The online-only delete banner** (`fileOperations.delete.cloudOnlineOnly*`): long, in a narrow strip above the file
  list; never reviewed by a human. Check for overflow.
- **`Cmdr no pudo revertir {name}`** (`fileOperations.cancelRollback.reason.failed.named`): reverting a single item
  stretches the verb; the fallback is `Cmdr no pudo deshacer lo hecho con {name}`.
- **`En segundo plano`** (the empty-queue button): if a reviewer finds it too elliptical, `Pasar a segundo plano` is the
  reserve (21 characters on a button whose other state says `Cola`).
