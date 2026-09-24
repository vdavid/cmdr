# fr review queue

Open questions for a future native French reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **rollback → `revenir en arrière` / `retour en arrière`** (tentative): chosen to stay clear of `restaurer` (promises
  the overwritten files come back) and `annuler` (Cancel, and the `Annulé` status). Confirm it reads naturally on the
  six status pills (`Retour en arrière possible`, `… effectué`, `… partiel`) and on the button.
- **listing → `liste des fichiers` vs plain `liste`**: the well-sourced orthodox term may be too long for a tight label.
  Confirm where plain `liste` reads best.
- **badge → `pastille`** (`pastille d'état`): no reference-pile hit for an icon-overlay status marker.
- **host key → `clé d'hôte`** and `la clé de {host}`: no macOS bundle exposes the term; built on the catalog's `clé`.
- **start folder → `dossier initial`**: coined; `dossier de départ` is Finder's home folder and `démarrage` is the app's
  or the Mac's launch.
- **`compromise`** for a revoked SSH key (`servers.refusal.hostKeyRevoked`): catalog-only evidence.
- **scope → `portée`**, **tail → `suivi` / `suivre`**, **streaming** (kept English), **thinking → `Réflexion…`**,
  **unarchive → `désarchiver`**, **on-device → `en local`**, **side panel → `panneau latéral`**: no pile source for any
  of them.
- **Like (AlternativeTo) → `Aimer`**: AlternativeTo has no French UI, so this is the standard social verb.
- **source-available → `consultable publiquement`**, **GitHub issue → `ticket`**, **batch rename → `renommage par lot`**
  / mass-rename → `renommage en masse`, **Wilting → `Flétrissement`** (a Cmdr coinage).
- **page up / page down → `page précédente` / `page suivante`**, **jump to → `aller à`**.
- **Ask about X → `Poser une question sur X`**, and the compact button `Interroger la sélection`.

## Phrasing

- **`un nouveau passage`** (`operationLog.rollback.partiallyRolledBackNotice`, "finishing the rollback takes another
  pass"): no corpus source; `reprend l'opération` reads as restarting the original operation, `une seconde passe` is
  jargon. Top priority if a native reviewer shows up.
- **`Quitter et rouvrir`** for macOS's own "Quit & Reopen" button (`onboarding.stepFda.step3`): verify the exact wording
  on a live French macOS.
- **`Ce mot de passe n'est pas le bon`** (`errors.mutation.*`, "That password didn't work"): macOS says
  `le mot de passe est incorrect`; the English deliberately softens, and this keeps the refusal on the password.
- **`Regardez votre téléphone`** (`adb.connect.unauthorized`): the verb choice alone; `appuyez sur Autoriser` is
  sourced.
- **`Continuer à travailler`** (`main.quit.keepWorking`): the shape is settled, the exact verb isn't.
- **`Tout ce qui est déjà terminé le reste.`** (`main.quit.body`): a momentary garden path on `le reste`, accepted.
- **`Ouvrez la file d'attente des opérations pour savoir pourquoi.`** (`queue.failureToast.summary`): the purpose
  clause.
- **The double-click hint's conversational lines** (`fileExplorer.doubleClickHint.*`): `Que s'est-il passé ?`,
  `Vous n'aimez pas ?`, `Ne plus jamais faire ça`, `J'aime bien`.
- **`Déposer pour joindre`** (the composer's drop hint), **`Ne rien faire`** (paste-as-file option), **`en douceur`** /
  **`à vitesse limitée`** (network photo indexing), **`Celle-ci a atteint sa limite`**,
  **`reste disponible dans la recherche`** and **`passage`** (indexing), **`un Mac équipé d'une puce Apple Silicon`**.
- **`Les autres fichiers sont toujours sur ce disque`** (`errors.write.deviceDisconnected.sided.source.*`),
  **`issus d'un`** (`fileOperations.leftovers.stagingFolderKept`), **`sont pris en compte`** and
  **`reste affiché pour vous le rappeler`** (`indexing.staleDialog.bodyPhone`,
  `fileExplorer.navigation.driveIndex.tooltipStalePhone`), **`Modifier les fichiers avec`**
  (`settings.behavior.textEditorApp.label`), **`Vous ne verrez ce message qu'une seule fois.`**
  (`main.escapeFullScreenHint.*`), **`Le transfert n'avance plus.`** (the sentence shape).
- **The online-only delete warnings** (`fileOperations.delete.cloudOnline*`): shipped as a draft, never read by a human.

## Overflow (check against the `en-XA` pseudolocale)

- `Visualiser` on the function key bar (10 characters).
- `Rechercher à nouveau` (the adb Re-check button, three words in a narrow button).
- `Comment faire` at the end of an already long adb hint line.
- `queue.failureToast.action` (`Afficher dans la file d'attente des opérations`, 46 characters against 23) and the trash
  arm of `queue.failureToast.title` (`Le placement dans la corbeille n'a pas pu se terminer`).
- `Placement dans la corbeille impossible` as the trash error title (the five `errors.write.*.title.trash` keys).
- The online-only delete warning banner, long text in a narrow strip above the list.
