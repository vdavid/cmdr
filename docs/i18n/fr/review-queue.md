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
- **badge → `pastille`** (`pastille d’état`): no reference-pile hit for an icon-overlay status marker.
- **host key → `clé d’hôte`** and `la clé du serveur {host}`: no macOS bundle exposes the term; built on the catalog’s
  `clé`.
- **start folder → `dossier initial`**: coined; `dossier de départ` is Finder’s home folder and `démarrage` is the app’s
  or the Mac’s launch.
- **`compromise`** for a revoked SSH key (`servers.refusal.hostKeyRevoked`): catalog-only evidence.
- **scope → `portée`**, **tail → `suivi` / `suivre`**, **streaming** (kept English), **thinking → `Réflexion…`**,
  **unarchive → `désarchiver`**, **on-device → `en local`**, **side panel → `panneau latéral`**: no pile source for any
  of them.
- **Like (AlternativeTo) → `Aimer`**: AlternativeTo has no French UI, so this is the standard social verb.
- **source-available → `consultable publiquement`**, **GitHub issue → `ticket`**, **batch rename → `renommage par lot`**
  / mass-rename → `renommage en masse`, **Wilting → `Flétrissement`** (a Cmdr coinage).
- **page up / page down → `page précédente` / `page suivante`**, **jump to → `aller à`**.
- **Ask about X → `Poser une question sur X`**, and the compact button `Interroger la sélection`.
- **S3 bucket → `compartiment`** (AWS’s French console and MS FRA), while Google Cloud and Scaleway French say `bucket`:
  confirm French S3 users recognize it. Also `adressage de type chemin` (path-style), `stockage froid` (cold storage),
  and the Google console wording `onglet Interopérabilité des paramètres de Cloud Storage` (tentative).
- **`Échap` vs `esc`** (`escape-key`, 8 keys incl. `settings.fileExplorer.typeToJump.mode.description`): the ruling
  follows MS and TC, but macOS 27 French says `la touche esc` in Setup Assistant, Screen Sharing and VoiceOver (the
  symbol name alone says `touche Échap`), and the Mac keycap reads esc. Decide whether to move the whole catalog.
- **git dirty → `modifié`** (tentative, `fileExplorer.git.repoChip.dirty`): the masculine qualifies the working copy,
  but it sits right after a feminine branch name in the pill (`main · modifié`); confirm it doesn’t read as a
  misagreement with `branche`, or pick a noun (`modifications`).
- **Uploading badge → `Chargement`** (`fileExplorer.fileIcon.sync.uploading`): copied from Finder’s own iCloud badge
  (`AXBADGE7`/`NE88.6`), but the catalog also uses `Chargement` for "Loading"; confirm a screen-reader user hears it as
  an upload next to the `Téléchargement` badge.
- **blocked → `empêcher` vs the shipped `bloqué`** (`askCmdr.renameReview.overwriteTooltip`, `.status`): the style guide
  bans `bloqué`, but the rename preflight ships it as a status and a count (`# bloqués`); confirm, or pick a status word
  (`retenu`, `refusé`) for both.
- **tampering → `trafiquer`** (`securityNotices.*`, `errors.write.replySignatureRefused.suggestion`): copied from
  `errors.shareList.signatureInvalid`; confirm it reads calm, not slangy.
- **NFS export → `partage`** (tentative, `servers.sheet.nfsExport` and the `nfsExports*` / `export*` family): follows
  the NAS apps’ `dossier partagé`; French sysadmin docs also say `un export NFS`. Is `partage` clear enough beside the
  SMB `partage`, or should the field say `Export`?

## Phrasing

- **Server account switch** (`fileExplorer.network.share.useGuest` → `Utiliser l’accès invité`,
  `servers.hub.shareAccount` → `en tant que « {username} »`): confirm the button reads as switching back to guest, and
  that the guillemets aren’t too heavy for a quiet suffix after a server name.
- **`servers.paneState.notConnected` → `Pas de connexion à {name}`**: a heading for a connection the person chose not to
  make; confirm it doesn’t read as a failure.
- **`un nouveau passage`** (`operationLog.rollback.partiallyRolledBackNotice`, "finishing the rollback takes another
  pass"): no corpus source; `reprend l’opération` reads as restarting the original operation, `une seconde passe` is
  jargon. Top priority if a native reviewer shows up.
- **`Quitter et rouvrir`** for macOS’s own "Quit & Reopen" button (`onboarding.stepFda.step3`): verify the exact wording
  on a live French macOS.
- **`Ce mot de passe n’est pas le bon`** (`errors.mutation.*`, "That password didn’t work"): macOS says
  `le mot de passe est incorrect`; the English deliberately softens, and this keeps the refusal on the password.
- **`Regardez votre téléphone`** (`adb.connect.unauthorized`): the verb choice alone; `appuyez sur Autoriser` is
  sourced.
- **`Continuer à travailler`** (`main.quit.keepWorking`): the shape is settled, the exact verb isn’t.
- **`Tout ce qui est déjà terminé le reste.`** (`main.quit.body`): a momentary garden path on `le reste`, accepted.
- **`Ouvrez la file d’attente des opérations pour savoir pourquoi.`** (`queue.failureToast.summary`): the purpose
  clause.
- **The double-click hint’s conversational lines** (`fileExplorer.doubleClickHint.*`): `Que s’est-il passé ?`,
  `Vous n’aimez pas ?`, `Ne plus jamais faire ça`, `J’aime bien`.
- **`Déposer pour joindre`** (the composer’s drop hint), **`Ne rien faire`** (paste-as-file option), **`en douceur`** /
  **`à vitesse limitée`** (network photo indexing), **`Celle-ci a atteint sa limite`**,
  **`reste disponible dans la recherche`** and **`passage`** (indexing), **`un Mac équipé d’une puce Apple Silicon`**.
- **`Les autres fichiers sont toujours sur ce disque`** (`errors.write.deviceDisconnected.sided.source.*`),
  **`issus d’un`** (`fileOperations.leftovers.stagingFolderKept`), **`sont pris en compte`** and
  **`reste affiché pour vous le rappeler`** (`indexing.staleDialog.bodyPhone`,
  `fileExplorer.navigation.driveIndex.tooltipStalePhone`), **`Modifier les fichiers avec`**
  (`settings.behavior.textEditorApp.label`), **`Vous ne verrez ce message qu’une seule fois.`**
  (`main.escapeFullScreenHint.*`), **`Le transfert n’avance plus.`** (the sentence shape).
- **The online-only delete warnings** (`fileOperations.delete.cloudOnline*`): shipped as a draft, never read by a human.
- **The Multi-Rename Tool** (`multiRename.*`): `Initiale De Chaque Mot En Majuscule` (`case.words`, the capitals show
  the result, as in English; French has no title case, so a native should confirm it reads as a demo, not a typo);
  `Retirer les signes diacritiques` for « Remove diacritics » (MS and Thunar's term, precise for ç and ü; is it too
  bookish for a checkbox?); `Première correspondance uniquement`; `une recherche avec remplacement`
  (`commands.fileMultiRename.description`); the counter popover's preview line
  `Donne 1, 3, 5…, affichés sous la forme 001, 003, 005` (`counterEditor.counts*`: is `Donne` natural with the counter
  as implied subject?); `# sans changement` in the footer summary (picked over `inchangé` so no agreement is needed).
- **Multi-rename tooltips** (`multiRename.placeholderHelp.*`, `searchOptionHelp.*`): `point d’insertion` for the text
  caret in `counterEditHint` (picked over `curseur`, which names the file-list cursor here; the reference pile wasn’t
  available to confirm Apple’s form); `Le caractère {at} seul` (no ordinal available); `Activé` / `Désactivé` as the
  on/off row labels (masculine, matching the shipped `Désactivé`, though the implied noun is `option`).
- **Multi-rename media fields** (`multiRename.mediaField.*`, `mediaPicker.*`, `mediaEditor.*`): `tags` for music and
  video tags (the `tag` ruling is the Finder sense, but `tags ID3` is everyday French); `Marque de l’appareil photo`
  over the pile’s `Fabricant`; `média` for the button; `{field} : aucune valeur` and `Champ inséré : {field}`, and
  `dans le masque` for `insertedMark`, all phrased so no participle agrees with a field name of unknown gender.
- **Transfer tree row states** (`fileOperations.transferTree.state.*`, `failedCount`): `Non terminé` /
  `{n} non terminés` for a file that couldn’t be transferred (reuses `operationLog.tree.statusUnfinished`; does it read
  as "not attempted yet"?); `Compression` stays bare to match `transferProgress.stageCompressing`, while the siblings
  say `Copie en cours` / `Déplacement en cours` because a bare `Copie` reads as "a copy" to a screen reader.
- **Damaged-state notices** (`stateRecovery.*`, `transfer.durabilityUnconfirmed*`): `historique d’annulation` for "undo
  history" (the UI elsewhere says `Historique des opérations`); `vous repartez donc de zéro` for the chats (the people
  restart, not the chats); `Cmdr a conservé l’ancien fichier sous {path}` (`sous` because {path} ends in the file name);
  `est défaillant` for a failing drive; subjunctive `confirmer que les copies soient …`.
- **Restic backups** (`errors.write.sourcesSpanVolumes.*`, `fileOperations.resticPassword.*`): `vue` for the English
  "view" (a read-only virtual folder); does it read as a display mode? `à l’instant` for "couldn’t check the password
  just now" (meaning "right now").
- **Ask Cmdr tool lines** (`askCmdr.tool.*`, `askCmdr.toolGroup.*`, `askCmdr.turnStats.stopped`): `A traité : …` /
  `Traitement en cours : …` for "Worked through"; `vous attend` for "waiting for you"; `Fermeture de Cmdr` for "Quitting
  Cmdr" (`quitter` has no verbal noun); `a montré 5 sur 6` (the tool as subject); `Réponse arrêtée · …` names the answer
  so the participle can agree; `Demande de l’outil « x »` adds `l’outil` English leaves implicit; `et de 3 autres` (and
  `de 1 suggestion`) before a numeral; `Ne rien changer` for "Leave them as I had them".
- **Refused-name dialog title** (`fileOperations.transferProgress.titleRefusedNameNotAllowed`):
  `Nom non autorisé à destination` follows the destination ruling's `à destination`; does it read as "on arrival" rather
  than "at the destination"? Fallback: `Nom refusé par la destination` (loses the `autoriser` term).
- **Operation log item tree** (`operationLog.tree.rollupRolledBack`, `togglesCapped`, `dialog.selectedCount`): the
  folder tally `{countText} rétablis` (beside `{countText} ignorés`) is a coined participle, since `retour en arrière`
  has none; does it read right for deleted copies too? The count line is a colon form, `Sélection : 3 sur 12`, because
  the plural driver is the total.
- **Windows shortcut-conflict labels** (`shortcuts.system.*`): Windows' UI never names the Alt+Space menu, so
  `le menu de la fenêtre` is descriptive (the TBX's `menu système` is a developer term); `le parcours des fenêtres` for
  Alt+Esc and `le passage d’une app à l’autre` for Alt+Tab are coined. Do they read as Windows features?
- **Search dialog keys** (`commands.searchModeByNumber.label`, `shortcuts.scope.searchDialog`):
  `Passer au mode de recherche portant ce numéro` and the heading `Rechercher et sélectionner des fichiers` (both dialog
  titles joined, verbs kept); natural enough?

## Bascules

- `commands.tagsToggle*.label` ship `Activer/désactiver le tag …`, the slash form style.md forbids (`toggle` ruling:
  `Activer ou désactiver`). Fixing changes macOS-read text, so it needs `ACCEPTED_TRANSLATION_CHANGES` entries; until
  then the keys sit in the ruling's `exceptions`.

## Overflow (check against the `en-XA` pseudolocale)

- `Initiale De Chaque Mot En Majuscule` in the letter-case menu (35 characters against 22).
- `Visualiser` on the function key bar (10 characters).
- `Rechercher à nouveau` (the adb Re-check button, three words in a narrow button).
- `Comment faire` at the end of an already long adb hint line.
- `queue.failureToast.action` (`Afficher dans la file d’attente des opérations`, 46 characters against 23) and the trash
  arm of `queue.failureToast.title` (`Le placement dans la corbeille n’a pas pu se terminer`).
- `Placement dans la corbeille impossible` as the trash error title (the five `errors.write.*.title.trash` keys).
- The online-only delete warning banner, long text in a narrow strip above the list.
