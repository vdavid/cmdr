# Latin American Spanish (es-419) style guide

`es-419` is an **overlay** of `es`, not a translation. `es` is written for Spain, like macOS's base Spanish; this
catalog carries only the keys where a Latin American reader would otherwise meet a Spain-only word or tense, and every
other key resolves through `es`. Mechanics of that contract (what coverage, parity, and stale each do to an overlay):
`docs/guides/i18n.md` § Overlay catalogs.

Everything in `docs/i18n/es/style.md` applies here unchanged (`tú`, infinitive buttons, the gender and hedge rules, the
typography, the term rulings) except the forks below. Read it first; this file records only what forks and why.

## Who reads it

CLDR parents every Latin American and US Spanish tag (`es-MX`, `es-AR`, `es-CO`, `es-US`, …) to `es-419`, so they all
open this catalog, and Spain (`es-ES`) keeps `es`. Apple ships the same split: `es.lproj` for Spain, `es_419.lproj` for
Latin America, and an `es_US.lproj` that agrees with `es_419` on the Full Disk Access pane name below.

## What forks

779 of `es`'s 3,707 keys (21%). By kind (a key can carry several):

- **Tense (about 475 keys)**: a finished action takes the preterite (`No se pudo copiar`, `Algo salió mal`,
  `Cmdr ya envió este reporte`), where `es` follows macOS es-ES's compound perfect.
- **Vocabulary (about 350 keys)**: the Settings name, Trash, add, press, report, and the smaller terms below.

`es-419` was seeded from the `es` values before `es` became Spain Spanish (it was pan-regional and preterite-based),
with every later typography, hedge, and voice fix in `es` kept, and only the regional choices reverted.

## Rulings

Evidence, unless a ruling says otherwise: every `.loctable` under `/System/Library` on macOS 27.0 (26A428), extracted
with `plutil`, 236,620 strings that exist in `en`, `es`, and `es_419` (2026-09-25). Counts read "`es` / `es-419`".

- **Preterite for a finished action.** `no se ha podido` 3,052 / 0 vs `no se pudo` 36 / 3,584; `ha dejado de` 50 / 0 vs
  `dejó de` 0 / 767; `se ha mostrado` 6 / 0 vs `se mostró` 1 / 20. The compound perfect stays where Latin America uses
  it too: a state still open, with `aún` / `todavía` / `ya` (`El disco aún no ha respondido`,
  `todavía no se ha conectado`), and `hasta ahora` (`lo que la operación ha escrito hasta ahora`). Apple's es-419 turns
  some of those into the Mexican `aún no llega` / `aún no se instala`, which is itself regional, so we don't follow it
  there.
- **Trash → `basurero`** (masculine): `papelera` 183 / 3, `basurero` 0 / 180; Finder's `Vaciar basurero`,
  `Transferir al basurero`. Articles and agreement follow the gender (`al basurero`, `del basurero`, `el basurero`,
  `un basurero propio`). Cmdr keeps `es`'s `Mover` for move, so the command is `Mover al basurero`. Lowercase inside a
  sentence, as `es` does for `papelera`. A cloud service's own trash isn't the Mac's: the online-only delete banners say
  `los servicios en la nube suelen conservar los archivos eliminados`, so `basurero` never names two things in one
  sentence.
- **add → `agregar`**: `añad-` 4,648 / 9, `agreg-` 116 / 4,664. `añadir` is an `-ir` verb and `agregar` an `-ar` one, so
  swap the whole form (`añadido` → `agregado`, `añade` → `agrega`), never the stem.
- **Settings → `Configuración`**: `Ajustes…` → `Configuración…` in the app menu (AppKit `Settings…`), System Settings →
  `Configuración del Sistema`; `ajustes` 4,938 / 27, `configuración` 1,074 / 6,040. It's a feminine singular, so
  everything around it moves: `los ajustes de IA` → `la configuración de IA`, `estos ajustes avanzados` →
  `esta configuración avanzada`, and a pointer to Cmdr's section drops the article (`en Configuración > IA`). One
  setting (the singular `ajuste`) → `opción`, or `configuración` when it's the value (`tu configuración cubre …`). ❌
  `Ajuste de línea` is word wrap, not a setting: it doesn't fork (`menu.viewer.wordWrap`,
  `settings.viewer.wordWrap.label`, `viewer.statusBar.*`).
- **press a key → `presionar`**: `puls-` 1,412 / 66, `presion-` 38 / 1,194. A keystroke (`pulsación`) →
  `tecla presionada`. Clicking stays `hacer clic` in both.
- **computer → `computadora`** (feminine): `ordenador` 1,059 / 10, `computadora` 2 / 1,047 (`esta computadora`,
  `a otra computadora`).
- **report → `reporte`**: `informe(s)` 379 / 32, `reporte(s)` 0 / 827; CoreTypes names a crash report
  `Reporte de fallos` in es-419 (`Informe de error` in es). So `reporte de fallos`, `reporte de error`,
  `Enviar reporte`.
- **default → `predeterminado`**: `por omisión` 650 / 4, `predetermin-` 10 / 723. An adjective, so it agrees
  (`la app predeterminada`, `los valores predeterminados`); the adverb is `de forma predeterminada`
  (`Desactivado de forma predeterminada`). `System Default` → `Predeterminado del sistema` (macOS writes
  `Valor predeterminado del sistema`).
- **enter a value → `ingresar`**: `introduc-` 3,957 / 39, `ingres-` 47 / 3,894 (`Ingresa la contraseña`,
  `Ingresar clave de licencia`). ❌ `Introducción` is Cmdr's name for onboarding, a noun that doesn't fork.
- **expire → `vencer`**: `caduc-` 448 / 18, `venc-` 42 / 465 (`Esta licencia venció el {date}`, Keychain Access's
  `Esta clave venció`).
- **manage → `administrar`**, file manager → `administrador de archivos`: `gestion-` 1,050 / 3, `administr-` 520 /
  1,881.
- **icon → `ícono`**: `icono(s)` 589 / 10, `ícono(s)` 0 / 717.
- **backup → `respaldo`**: `copia(s) de seguridad` 744 / 0, `respaldo` 6 / 703.
- **Full Disk Access → `Acceso completo al disco`**: the Privacy & Security pane is `Acceso total al disco` in `es` and
  `Acceso completo al disco` in `es_419` and `es_US`
  (`SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable`, key `ALL_FILES`). Prose that names the
  setting follows it (`Sin acceso completo al disco`). The descriptive `acceso a todo el disco` isn't regional and
  doesn't fork.
- **OK button → `OK`**: AppKit's and Finder's `OK` read `Aceptar` in es and `OK` in es-419 (`Aceptar` 788 / 308, `OK`
  257 / 580). Only the three bare `OK` buttons fork; a quoted macOS dialog button keeps its own text.
- **cost → `costo`** (`coste` 8 / 0, `costo` 0 / 17); **take a photo → `tomar`** (`hacer una foto` 54 / 0,
  `tomar una foto` 3 / 60).
- **a place → `lugar`**: `otro sitio` 10 / 0, `otro lugar` 0 / 9 (`a su lugar`, `desde cualquier lugar`,
  `a ningún lado`). `sitio web` stays.
- **`vale la pena`** over `merece la pena`, and **no `vosotros`** (`vuestras contribuciones` → `sus contribuciones`).
  Neither shows up in the macOS sweep; both are the standard Latin American forms. `plantéate` → `considera` rides along
  in the one key that had it.

## The fork test

**Fork what a Latin American reader would read as Spain's. Skip what reads the same on both sides of the Atlantic.** A
fork costs forever: it freezes a copy that goes stale on every `es` edit to that key. So Apple's es-419 flipping a word
is evidence, not a verdict. Considered and deliberately NOT forked:

- **`Intro` for the Enter key.** macOS es-419 writes `Intro` too (Spotlight's `ENTER_KEY` is `Intro` in both; `Retorno`
  45 / 44).
- **`sólo` with an accent.** es-419 macOS writes it (`solo` 1,899 / 100, `sólo` 2 / 1,727), but the RAE and ASALE rule
  (unaccented unless genuinely ambiguous) binds every Spanish, so it's house style, not region.
- **`inténtalo de nuevo` → `intenta de nuevo`.** Apple flips it (1,432 / 8 vs 0 / 382), yet `inténtalo de nuevo` and
  `vuelve a intentarlo` read naturally everywhere. 40 keys of churn for no reader benefit.
- **`comprobar` → `verificar`.** Apple's es-419 still writes `comprob-` 56 times; it's pan-Hispanic.
- **`previsualizar` → `vista previa`.** es-419 keeps 49 `previsualiz-`; Cmdr uses the verb in three prose keys.
- **Pin: `fijar` vs `anclar`.** Apple writes `Anclar pestaña` in both es and es-419, so it's an `es` question, not a
  regional one (`docs/i18n/es/review-queue.md`).
- **Move: `Transferir al basurero`.** Finder es-419 says `Transferir`; Cmdr keeps `es`'s `Mover` for every move.
- **Quotes and typography.** es-419 macOS writes `“…”` like es (10,848 `“`, no `«`). No `mechanics.json` here: the
  overlay inherits `es`'s.
- **`la Terminal`, `Vaya`, `ahora mismo`, `darte de alta`, `bastante`.** Understood across the region; see
  `review-queue.md` for the ones a native reviewer might still want.

## Catalog mechanics

- **Stamp `@key.sourceHash` from the `es` value you override**, so an `es` edit marks the fork stale.
- **A value identical to `es` is a coverage finding**, and the fix is always to delete the key.
- **`apps/desktop/scripts/i18n-es-overlay.test.ts` holds the vocabulary above**: it sweeps every `es` value for each
  Spain-only form and fails on a key this catalog doesn't fork, or a fork still using the form. When `es` gains a string
  with `papelera`, `añadir`, `Ajustes`, `pulsa`, and the rest, fork it here in the same change. A new Spain-only form
  goes into that test's list and this file together.
- The tense has no complete sweep (a compound perfect can be right, see above); that test holds only its loudest frames
  (`ha podido`, `ha ido mal`). Review a new `es` string's tense by eye.
- `gen-locale-skeleton.ts` refuses overlay tags and `sync-locale-keys.ts` skips them; this catalog is written by hand.
- No `terms.json`: the termbase covers full translations only (`docs/i18n/termbase.md`), so the rulings live here.
- Open questions for a Latin American reviewer: `review-queue.md`.
