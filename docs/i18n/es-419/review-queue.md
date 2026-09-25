# es-419 review queue

Open questions for a native Latin American Spanish reviewer. Not translator input: every item below already ships a
reasoned value (or a reasoned non-fork) recorded in `style.md`. Remove an item once it's settled.

## Forks to confirm

- **Trash → `basurero`** (43 keys): matches macOS es-419 exactly, but some readers say `papelera` or
  `papelera de reciclaje` (Windows). Confirm that the Mac's own word wins, as it does for `en-GB`'s `Bin`.
- **Settings → `Configuración`** without an article when it names Cmdr's section (`en Configuración > IA`), and
  `la configuración` when it's prose (`en la configuración de Ask Cmdr`). Does the split read naturally?
- **A single setting → `opción`**: `no es una opción de privacidad de tu Mac`,
  `Estas opciones son para usuarios avanzados`. `ajuste` might read fine in Latin America too.
- **Keystroke → `tecla presionada`** (`tras la última tecla presionada`): a little long for a settings description.
- **`vale la pena`, `considera`, `sus contribuciones`**: no macOS evidence either way, standard usage only.

## Non-forks to confirm

- **`Móvil`** (`fileExplorer.navigation.groupMobile`), the sidebar group holding phones and other MTP devices. Latin
  America says `celular` for the phone itself, but the group also holds cameras and e-readers, so `Móvil` stays as
  "mobile devices". `Dispositivos móviles` would be clearer but wider.
- **`inténtalo de nuevo` / `vuelve a intentarlo`** (40 keys) against Apple es-419's `Intenta de nuevo`.
- **`Vaya, no pudimos darte de alta ahora mismo`** (`onboarding.stepBeta.signup.failure`,
  `settings.updates.emailSignupError`): `Vaya` and `darte de alta` lean Spain; `Uy, no pudimos registrarte` is the
  alternative.
- **`aún no ha respondido`**-style compound perfects stay (see `style.md` § Rulings). Confirm none reads Spanish.
- **`a su lugar`** in the rollback toasts (`fileOperations.cancelRollback.*`), where `es` says `a su sitio`:
  `en su lugar` can also mean "instead"; `ahora hay otra cosa en su lugar` should still read as "in its place".
