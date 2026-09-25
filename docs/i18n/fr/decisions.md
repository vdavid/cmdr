# fr decisions

Distilled rulings behind `terms.json`: "X over Y because Z", one section per surface, each heading citing its keys so
`pnpm i18n:brief` can pull it. The term rulings live in `terms.json`, typography in `mechanics.json`, open questions in
`review-queue.md`, voice in `style.md`. Headings keep a plain apostrophe (the `decision` pointers match them); quoted
values write the catalog's `’`.

## Sections des réglages et noms de présentation (`settings.section.*`, `settings.navigationAndFileOps.card.*`, `settings.summary.navigationAndFileOps`)

- Section titles stay identical in every file that names them (`Apparence`, `Opérations sur les fichiers`,
  `Surveillance du système de fichiers`, `Mises à jour et confidentialité`, `Journalisation`, `Avancé`, …).
- Navigation & file ops → `Navigation et opérations`: French has no casual "ops"; the card keeps
  `Opérations sur les fichiers`.
- View modes: `mode Complet` / `mode Bref` (agreeing with `mode`), but `Présentation complète` / `Présentation brève`
  (agreeing with the feminine `présentation`).
- A cross-reference quotes the section title verbatim with the source’s separator (`Réglages › IA`).

## Licence, visionneuse : les valeurs identiques à l'anglais (`licensing.dialog.*`, `licensing.section.*`, `viewer.binaryWarning.kind.*`, `viewer.saveAs.defaultName`)

- `Active` (a license) is the feminine of `actif`, agreeing with `licence`; `Image`, `PDF`, `Unicode`, `image`,
  `document` are French too. All carry `sameAsSourceJustification`.
- `viewer.saveAs.defaultName` stays `selection`: the `@key` wants a lowercase, unaccented, file-name-safe literal.
- Tiers: `Commerciale perpétuelle` (agrees with the implied `licence`), `Abonnement commercial`,
  `Personnelle (gratuite)`.

## La prise en main porte un seul nom (`onboarding.wizard.title`, `onboarding.wizard.progressLabel`, `commands.cmdrOpenOnboarding.label`, `shortcuts.scope.onboarding`, `main.upgradeNudge.*`, `settings.onboarding.upgradeNudgeShown.*`, `onboarding.stepAi.bannerBody.stuck`)

- Onboarding → `prise en main` on every surface (menu item, wizard title, progress bar, flag, prose) over
  `configuration` / `accueil` / `Bienvenue dans Cmdr`: the menu item and the window it opens must name each other.
- The stuck banner keeps `Privacy & Security > Full Disk Access` as English literals, like the source; only the
  `{systemSettings}` token is OS-localized.

## Transferts et compteurs (`transfer.*`, `feedback.dialog.counter`, `errorReporter.dialog.counter`, `whatsNew.dialog.title`)

- Participles agree masculine with `fichier` / `dossier`; each `transfer.movedPhrase` branch stands alone.

## Les raccourcis : portées, fonctions de macOS et valeurs identiques (`shortcuts.scope.*`, `shortcuts.system.*`, `shortcuts.section.*`, `downloads.shortcutRow.*`)

- Scope headings are nouns (`Fenêtre principale`, `Liste des fichiers`, `Sélecteur de volume`, `Menu des favoris`).
- Logging out of the Mac → `la fermeture de session` (Apple menu `Fermer la session`) over `la déconnexion`, which reads
  as dropping an SMB share.
- Fixed badge → `Fixe`; the Modified filter → `Modifiés` (plural: the shortcuts you changed, no date).
- Identical to English on purpose: `Global`, `OK`, `Options`, Spotlight, Mission Control, Spaces.

## La file d'attente : statuts et libellés des lignes (`queue.row.status`, `queue.row.label`, `queue.row.pause`, `fileOperations.transferProgress.pause`)

- Statuses: `En attente`, `En cours`, `En pause`, `Terminé`, `Annulé`, `N’a pas pu se terminer` (masculine, agreeing
  with the implied operation word).
- Row labels are the bare verbal nouns without `en cours`: `Copie`, `Déplacement`, `Suppression`,
  `Placement dans la corbeille`, `Renommage`, `Création du dossier`, `Création du fichier`, `Modification de l’archive`.
  Every toast or title naming an action reuses these nouns.

## Double-clic sur l'arrière-plan du panneau (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `settings.behavior.doubleClickOnPaneNotificationSeen.*`, `settings.section.navigationAndFileOps`, `fileExplorer.breadcrumb.navigateTooltip`)

- `espace vide` (Double Commander) for the pane background; the description keeps the English pane / file-list mix.
- Never do this again → `Ne plus jamais faire ça` (the navigation) over `ne plus afficher` (that would mean the hint). I
  like it → `J’aime bien` over the too-strong `J’aime`.
- The flag agrees with the feminine `astuce`: `Astuce … affichée`.

## Fichier trop volumineux pour le système de fichiers (`errors.write.filesTooLargeForFilesystem.*`, `errors.listing.notSupportedErrno.suggestion`)

- `trop volumineux` (Nautilus) over `trop grand`, which the pile keeps for image dimensions.
- `formaté en FAT32`; "can’t store files larger than X" → `ne peut pas stocker de fichiers de plus de X`.
- The overflow line follows Finder’s `et ^0 de plus`, with the noun in each plural branch.

## Les libellés du dialogue de copie (`fileOperations.transferDialog.operationAria`, `.scanFile`, `.scanDir`, `.targetWillBeCreatedCopy`, `.targetWillBeCreatedMove`)

- The chooser’s accessible name → `Action` (identical, justified). Scanning… → `Analyse…`.
- "Cmdr will create it" → `Cmdr le créera lors de la copie.` / `… du déplacement.`: active over Thunar’s passive
  `sera créé`.

## Parcourir les archives et les paquets (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `errors.listing.archiveUnreadable.*`, `errors.mutation.archive*`, `queue.row.label`)

- `archive` is feminine and inflects (`chiffrée`, `protégée`); the bare title `Archives` is identical and justified.
- App bundle → `paquet` (Finder `Afficher le contenu du paquet`); card and row both `Paquets d’application`.
- editable → `modifiable` over the passive `peuvent être modifiées`. Here `application par défaut` in full because macOS
  attests it; elsewhere `app`.
- The sender stays neutral: `la personne qui vous l’a envoyée` (the participle agrees with `archive`).

## Coller le presse-papiers comme fichier (`fileExplorer.clipboard.pastedAsFile*`, `settings.fileOperations.pasteClipboardAsFile.*`)

- "as X" → `en tant que X`; "saved as" → `enregistré sous` (AppKit save panel).
- The toast hangs `collé` off the masculine `Contenu` and puts the kind in a bare parenthetical, so no participle agrees
  with the varying kind or `{filename}`. Don’t move the participle after the kind.

## Le mot de passe d'une archive (`fileOperations.archivePassword.*`)

- The body names `L’archive {name}`, so the known feminine `archive` drives `protégée` and `la déverrouiller`, never the
  uncontrolled `{name}`.

## Compresser (`commands.fileCompress.*`, `fileOperations.transferDialog.toggleCompress`, `.confirmCompress`, `fileOperations.transferProgress.scanTitleCompress`, `transfer.compress.*`, `settings.archives.compressionLevel.*`)

- `Compresser` (Finder) over `Comprimer`; the progress noun is `Compression`.

## L'historique des opérations (`operationLog.*`, `commands.logOperationLog.*`)

- Operation log → `Historique des opérations` over `journal`, which stays the log FILE (`fichier journal`,
  `journalisation`); it pairs with `File d’attente des opérations` in the View menu.
- `Annulé` (stopped before it ran) and the rollback pills (`retour en arrière`) must stay distinct at a glance: that’s
  why rollback never takes `annul-`.
- Didn’t finish → `Non terminé`, never `Échec`. Summaries are count-led participles in every branch
  (`{countText} éléments copiés`); archive outcomes are feminine (`Archive modifiée`).
- Provenance: `Vous`, `Client IA`, `Agent` (identical, justified).

## Ask Cmdr : la conversation, les jetons et les lignes d'outil (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- chat (noun) → `conversation` everywhere, over `discussion` and `chat` (a cat in French); the verb → `discuter`.
- token → `jeton` on every key (MS FRA; macOS has neither), so a number in a message matches its setting.
- unarchive → `désarchiver`; the badge `Archivée` agrees with `conversation`. on-device → `en local`.
- Tool lines pair a verbal noun (`Vérification de vos disques`) with `A <participe> …` (`A vérifié vos disques`), no
  pronoun. Cross-pair consistency outranks stem symmetry: `Recherche dans vos photos` / `A cherché dans vos photos`.
- "Ask about X" → `Posez des questions sur X` for the empty state and `Posez une question…` for the one-message field;
  the compact button → `Interroger la sélection`.
- `askCmdr.renameReview.expired` keeps the brand (`Demandez à Cmdr…`). Panel, section, command, and switch `Ask Cmdr`
  carry `sameAsSourceJustification`.

## L'indexation des images sur les disques réseau (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndexVolumes.*`, `settings.mediaIndex.alwaysIndexFolders.*`, `search.imageResults.networkOff`, `.paused`)

- network drive → `disque réseau` over Windows’s `lecteur réseau`. `photo` is feminine in every branch.
- gently / limited speed → `en douceur` / `à vitesse limitée` (tentative). Internal lists read like
  `settings.indexing.silencedDrives.*` (`Interne : …`).

## Revue qualité : renommages proposés et lignes d'outil (`askCmdr.renameReview.*`, `askCmdr.tool.*`, `askCmdr.stalled`, `errors.listing.deviceReconnecting.*`, `fileExplorer.imageIndex.*`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- Allow / deny → `Autoriser` / `Refuser`; all → `Tout autoriser` / `Tout refuser` (the `Tout <verbe>` pattern).
- review → `vérifier` (AppKit `Vérifier les modifications…`) over `revoir` and MS `revue`.
- rotate (files through a cycle) → `permuter` over macOS `faire pivoter`, which is the image sense.
- Removing a folder from the indexing list → `Retirer` over macOS `Supprimer`: the help text promises nothing is
  deleted, and `supprimer` would say the opposite.
- `askCmdr.stalled` ends `…ou arrêter`, matching the Stop button.

## Les pastilles d'état de l'index d'images (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- Per-file statuses agree feminine with `image` (`indexée`, `incluse`, `réindexée`); couldn’t index →
  `Indexation impossible`.
- `folder.allIndexed` carries "all" by the missing fraction, never `toutes` (the `one` branch would read "Toutes les 1
  image").
- Drive tooltips open `Sur ce disque,` to avoid a double `sur` next to "X sur Y".

## Les réglages de l'indexation des images (`settings.mediaIndex.cards.*`, `settings.mediaIndex.progressSummary.title`, `settings.mediaIndex.semanticSearch.label`, `settings.mediaIndex.clip.*`, `fileExplorer.imageIndex.file.indexing`)

- Indexing now → `Indexation en cours`, distinct from queued `En attente d’indexation`.
- Search by description → `la recherche par description`; the card stays `Recherche sémantique`.
- Delete model (reclaim {size}) → `Supprimer le modèle (libérer {size})`.

## Le dialogue de suppression : l'interrupteur corbeille, De et À (`fileOperations.delete.trashSwitch`, `.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `.targetGroupTitle`)

- The switch → `Placer dans la corbeille` (Finder); the destructive button → `Supprimer`.
- From / To headings → `De` / `À` (Total and Double Commander’s copy dialog) over `Vers`; headings take no colon, path
  labels do (`De :`).

## L'indexation des disques désactivée (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- The master switch in prose → bare `l’indexation`, over `l’indexation du disque`, which reads as the per-drive meaning
  these strings exist to rule out. The scope marker (`pour ce disque` vs `dans les Réglages`) carries the difference.
- "stays unindexed" with `{name}` → `Cmdr n’indexe pas {name}`: active, so nothing agrees with the name.

## Le dialogue d'incident : `a quitté inopinément`, `a continué son exécution`, `la dernière fois` en fin de proposition (`crashReporter.dialog.*`, `settings.updates.crashReports.description`)

- quit unexpectedly → `a quitté inopinément` (AppKit’s own crash wording) over the unattested
  `s’est fermé de façon inattendue`.
- kept running (the app) → `a continué son exécution` (AppKit exception dialog), over `a continué de fonctionner`
  (`fonctionner` means "work / be compatible" in the pile) and `est resté ouvert` (a window).
- `.keptRunning` holds no closing word; `.unknown` asserts neither quitting nor running.
- `la dernière fois` goes at the END of the clause, as in every pile occurrence. `continuer à` + infinitive.
- A report without a crash → plain `un rapport`; the setting’s LABEL keeps `Envoyer les rapports d’incident`.

## Les états du navigateur réseau : connecté ou identifié (`fileExplorer.network.browser.status.*`, `fileExplorer.network.browser.tooltip.requiresLogin`, `errors.listing.authRequiredEauth.explanation`, `errors.listing.authRequiredEneedauth.explanation`)

- Connected → `Connecté`, Logged in → `Identifié`: both sit in one column, so they can’t share a word. Login failed /
  needed → `Identification refusée` / `requise`. Participles agree with the host, never the person.

## Se connecter à une app ou à un compte (`errors.provider.*`)

- Signing in to an app follows Apple’s `Se connecter à`, but never as a participle on the reader
  (`Assurez-vous d’être connecté`): phrase it as an action or name the session
  (`Vérifiez que votre session iCloud est ouverte`).

## Une recherche en cours sur un disque (`fileExplorer.navigation.driveIndex.deferredEnable`, `.deferredRescan`, `search.coverage.toast.deferredUntilSearchEnds`)

- `Cmdr effectue une recherche sur {name}` over `parcourir` (browsing). The wait names its object,
  `dès la fin de cette recherche`, because `dès qu’elle sera terminée` could bind to `l’indexation` instead.

## Index de disque : l'analyse des changements (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- Checking for changes → `Recherche des changements`, nominal like its sibling run headers; `analyse` is the catalog’s
  word for a full check (`l’analyse en cours`).

## Transferts à l'arrêt : le bandeau de blocage (`fileOperations.transferProgress.stall*`, `.close`)

- Close (the dialog; the transfer keeps running) → `Fermer`, distinct from Cancel `Annuler` and dismiss `Ignorer`.
- Stalled → `Aucune progression depuis {duration}` and `Le transfert n’avance plus.`: the observation, never `bloqué`.
- Waiting for X → `En attente d’une réponse de la destination / de la source` (the pile’s dominant shape).
- The log → `Le fichier journal donne les détails.` (`fichier journal` because it points at the log FILE).
- `stallInFlight` pulls "and may already be partly written" INTO the plural branches so the participles agree.
- `{duration}` arrives preformatted, so the sentence reads for any shape after `depuis`.

## Chemin copié : la confirmation du presse-papiers (`fileExplorer.clipboard.copiedPath`)

- `Chemin copié, il est maintenant dans le presse-papiers :` with the definite article like macOS, never `votre`; the
  path shows on its own line, so the sentence stands without it.

## Le renommage de la file d'attente des opérations (`queue.windowTitle`, `queue.heading`, `queue.list.aria`, `queue.row.pauseAria`, `.resumeAria`, `.cancelAria`, `.selectAria`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`, `.backgroundedToast`)

- operation → `opération` (feminine, Finder); the queue → `File d’attente des opérations`, verbatim in every string that
  names the window, and `commands.queueShow.label` is the bare window name.
- Per-row aria labels share one family: `Mettre cette opération en pause`, `Reprendre…`, `Annuler…`, `Sélectionner…`,
  `Ignorer cette opération`.
- `queuedToast` and `backgroundedToast` refer back in the feminine (`celle-ci`, `Retrouvez-la`), since `opération` is on
  screen; keep the two parallel.
- `queueTooltip` keeps `transfert`: that tooltip really describes a transfer.

## La pastille de progression et l'avis « N'a pas pu se terminer » (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- dismiss → `Ignorer`, all → `Tout ignorer`. Known collision with Skip all (`conflictSkipAll`), accepted: they never
  share a surface, and splitting would cost the single Dismiss word.
- The failure headline makes the action noun the subject (`La copie n’a pas pu se terminer`), from the `queue.row.label`
  nouns, so toast and row agree; the `other` arm is bare `N’a pas pu se terminer`.
- "Open the queue to see why" → `… pour savoir pourquoi` over `pour en connaître la raison`: number-neutral for 1 or N.
- Show in queue → `Afficher dans la file d’attente des opérations`, byte-identical window name (long; see
  `review-queue.md`).
- Percent for screen readers → `pour cent`. Tooltip line:
  `{label} de {N} éléments vers {destination} · {P} % · {detail}` (Finder, Nautilus); `de` is needed or the verbal noun
  reads as an imperative; the destination stays unquoted like the English.
- Optional parts keep their leading space inside their branch so an absent part leaves no double space or stray `·`.

## Le dialogue de conflit autonome (`fileOperations.operationConflict.context`, `.pausedNote`)

- A bare verbal noun takes `en cours` to stand as a line (`Copie en cours`); one with a complement doesn’t
  (`Copie vers Backup`). Don’t "fix" the asymmetry.
- Editing {destination} → `Modification de l’archive {destination}`: naming `archive` fixes the elision that
  `de {destination}` couldn’t.
- Working (in X) → `Opération en cours dans X`; until you answer → `tant que vous n’avez pas répondu` (macOS
  `tant que … ne … pas`).

## Le bouton du dialogue de progression quand la file est vide (`fileOperations.transferProgress.background`, `.backgroundAria`)

- The empty-queue arm → `En arrière-plan` (MS FRA, Double Commander): the bare noun `Arrière-plan` names a wallpaper,
  and `Continuer / Passer en arrière-plan` wraps the fixed 580 px button row (the `@key` says it must fit like `Queue`).
  The status-like shape is accepted because buttons read as actions.
- The aria → `Garder ce transfert en cours en arrière-plan`, ending in the visible label (WCAG 2.5.3); don’t reword its
  tail.

## La barrière de sortie (`main.quit.*`)

- Title → `Quitter alors qu’une opération est en cours ?` (elliptical infinitive question, never
  `Voulez-vous vraiment…`); `alors que` for its concessive edge over temporal `pendant que`.
- "stays done" → `Tout ce qui est déjà terminé le reste.`: the only short form true for deletes and trashes too.
- logout → `une fermeture de session` over `déconnexion` (servers). Countdown → `Cmdr quitte dans …`, one verb root for
  the whole dialog, over `se ferme` (collides with `fermeture`).
- Keep working → `Continuer à travailler`: bare `Annuler` means cancel the listed operations, and bare `Continuer` is
  macOS’s go-ahead. Quit now → `Quitter maintenant`; heading → `Toujours en cours`.

## Statistiques d'usage : pas « anonymes », mais « un identifiant aléatoire » (`settings.analytics.enabled.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`, `.analyticsTitle`)

- `statistiques d’usage` tied to `un identifiant aléatoire`, over `anonymes` (false) and `pseudonyme` (the jargon the
  English avoids); `identifiant` over MS’s technical `identificateur`; tied to → `relié à`.

## Lignes de file en attente de réponse et la confirmation du retour en arrière (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- Needs your answer → `Réponse requise`, never anything with `attente`: `En attente` is the queued status in the same
  column.
- carries on → `continuera` (`reprendre` is resume). Keep them → `Conserver les fichiers`, naming the files (a pronoun
  would be ambiguous). Stop, and … → `Arrêter et …`, never `Annuler`.
- `foregroundBusyToast` names the operation (`affichez cette opération`), since "this one" has no antecedent.

## La famille `rollback` : `revenir en arrière`, jamais `restaurer` (`fileOperations.transferProgress.rollback*`, `.conflictRollback`, `.titleRollingBack`, `operationLog.rollback.*`, `operationLog.outcome.rolledBack`, `operationLog.dialog.rollBack`, `commands.logOperationLog.description`, `settings.operationLog.intro`)

- rollback → `retour en arrière` / `revenir en arrière` (tentative). The rollback DELETES what the operation wrote, so
  `restaurer` promises the opposite (and the catalog uses it for real restores); `annul-` collides with Cancel and the
  `Annulé` status.
- Always WITH `en`: `retour arrière` is the Backspace key and a media rewind.
- The pills: `Retour en arrière possible` / `impossible` / `en cours` / `effectué` / `partiel`; the button fits the
  button-row budget.

## Renommage en chaîne : « et N autres » se rend par `ainsi que …` (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- "and so did N other files" → `ainsi que … autres fichiers` (Finder’s "X et N autres" shape, Dolphin), inside the
  branches because the `one` arm elides (`ainsi qu’un autre fichier`). `fichier` because the English says file.

## Renommage non confirmé : le volume ne répond pas (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

- The opposite of `chainKept*`: never imply the name stayed. `Impossible de confirmer le renommage de « X »`, on the
  `mkdir.timeoutMessage` mold, with the same `quand même`.
- Name the file in the second sentence (`donc le fichier a peut-être quand même été renommé`), never `il`: the nearest
  masculine noun is `le volume`. The doubled `peut-être` is deliberate (two different doubts).
- Several files → `les renommages de « X » et de {n} autres fichiers`: `de` repeats because they’re complements (unlike
  `ainsi que` above).
- `Le nom du fichier ne peut pas être utilisé`, matching its `validation.*` siblings, no final period (it also fills
  `{reason}`).

## Opérations suggérées : la fenêtre de ce que propose Ask Cmdr (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- approve → `Approuver` (it authorizes an action) over macOS `Accepter`; reject → `Refuser` (AirDrop pair).
- "This can’t be undone" → `Cette opération est irréversible` (Finder verbatim). reason → `raison`, since `motif` is
  pattern.

## Dupliquer : la commande qui copie dans le même dossier (`commands.fileDuplicate.*`)

- `Dupliquer`, as Finder’s File menu.

## Menus natifs : barre de menus, menus contextuels, titres de fenêtre (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Finder wins on every native menu label, because the user reads Cmdr’s menu bar next to Finder’s: `Fichier`, `Édition`,
  `Présentation`, `Aller`, `Fenêtre`, `Aide`, `Services`, `Coup d’œil`, `Lire les informations`, `Dossier parent`,
  `Départ`, `Trier par`, `Réduire/agrandir` (Window > Zoom; text zoom stays `Zoom`).
- The Select menu → `Sélectionner` (Linux file managers; Finder has none). Sort order → `Croissant` / `Décroissant`.
- changelog → `Journal des modifications` (the document) vs Help > `Nouveautés`. word wrap →
  `Retour à la ligne automatique`. Tabs follow Safari (`Épingler l’onglet`, `Désépingler l’onglet`).
- Finder tag colors verbatim (`Rouge, Orange, Jaune, Vert, Bleu, Violet, Gris`); tag row `Tags`, `Ajouter « {color} »`,
  `Retirer « {color} »` over Finder’s `Supprimer` (removing a tag deletes nothing).
- Eject → `Éjecter`, Disconnect → `Se déconnecter`, Remove from a list → `Retirer`.
- Identical on purpose: `menu.app.services`, `menu.sort.extension`, `menu.view.zoom`, `menu.tag.orange`,
  `menu.tag.rowLabel`, `menu.view.askCmdr`.

## Notification de repli sur le montage macOS (`fileExplorer.network.osMountFallback.*`)

- `la connexion réseau SMB native de macOS` (MS FRA `native`). Speed multipliers spell out `fois` (`4 fois`).
- You are connected → `Vous y avez bien accès`: names the access, not a gendered participle.
- Couldn’t connect directly → `La connexion directe … n’a pas pu être établie`, matching
  `directConnectionUnexpectedToast`. Try connecting directly → `Essayer de se connecter directement`.
- "for most connections" → `dans la plupart des cas`: a fourth `connexion` in one sentence reads badly.

## Refus de renommage et de création (`errors.mutation.*`, `errors.volume.*`)

- `{path}` stays in a neutral, quoted slot. "nothing at {path} any more" → `« {path} » n’existe plus.` (Finder
  verbatim).
- Top folder of a volume → `le dossier racine d’un volume`; `Impossible de renommer ici le dossier racine d’un volume.`
- System Integrity Protection → `la protection de l’intégrité du système` (Apple localizes it), in the Finder’s
  `en raison de …` frame.
- Moving an item out of or between archives → `sortir` / `faire passer`, keeping `Déplacer` for the command named right
  after.
- That password didn’t work → `Ce mot de passe n’est pas le bon.` (tentative): softer than macOS `incorrect`, as the
  English is.
- `errors.mutation.timedOut` is NOT a failure (`… donc la modification peut encore aboutir`), and
  `errors.volume.deviceSessionReset` is NOT an unplug: the device is still attached.

## Refus de placement dans la corbeille (`errors.mutation.trashNotSupported`, `.trashRefused`)

- `Ce volume n’a pas de corbeille : la seule option est de supprimer définitivement.` (direct like the English "has no
  Trash").
- `macOS a refusé de placer cet élément dans la corbeille.`: `a refusé` is the calm verb for "wouldn’t", and
  `cet élément` over a pronoun without antecedent.

## Éjection et déconnexion refusées : les neuf clés `errors.eject.*`

- Each value follows the colon of `Impossible d’éjecter {volumeName} :` or `Impossible de se déconnecter :`, so it
  starts with a capital and never repeats `Impossible de …`.
- Transitive disconnect (Cmdr drops a device) → `déconnecter`; leaving a server → `se déconnecter`. Keep both.
- idle → `quand il n’est plus occupé` (the negation of `busy → occupé`). "nothing to eject" →
  `il n’y a donc rien à éjecter`.
- Couldn’t tell which device → `Cmdr n’a pas reconnu cet appareil et ne peut donc pas le déconnecter`: coordinated,
  since a second-clause `il` would bind to `cet appareil`.
- `errors.eject.timedOut` is NOT a failure: `… donc l’éjection peut encore aboutir d’elle-même.`

## Le refus d'éjection nommé : qui tient le disque (`errors.eject.unmountRefusedBy*`, `.otherApps`)

- `{app} utilise encore ce disque.`: active like the generic sibling; `{app}` is a bare subject (unknown gender).
- Close what it has open → `Fermez ce que cette app y a ouvert`, never `ce qu’il y a ouvert` (binds to `ce disque`).
- other apps → `d’autres apps`, lowercase, no period; `Intl.ListFormat` joins the list.
- The disk-image key starts from the drive (`Ce disque contient une image disque encore ouverte.`) to avoid stacking
  three `disque`. macOS still working → `travaille encore sur ce disque`, a different verb from `utiliser` on purpose
  (nothing to close, only wait).
- send a report → `envoyez un rapport` (plain: nothing crashed), never `un retour` (the feedback surface).

## La notification de corbeille : annuler et remettre en place (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- Undo → `Annuler`, living with the Undo/Cancel ambiguity as macOS does; `Remettre` is the sourced fallback.
- Put back → `remettre en place` (Finder `Remettre`) over `restaurer`, which the catalog keeps for rename undo (names,
  not locations).
- `Ces éléments sont peut-être déjà de retour`: `de retour` is invariable. The partial result’s second half agrees
  through its own `{skipped}` plural.
- The button and the command share `Aller à la corbeille`.

## Compléter un rapport déjà envoyé : les 11 clés `errorReporter.amend*`

- Title → `Ajouter à votre rapport d’incident`, button `Ajouter au rapport`, progress `Ajout…`: one `Ajouter` chain
  (Apple’s `Ajouter à X`) over `Compléter`.
- `Ce qui a été envoyé` mirrors the sibling `Ce qui va être envoyé`. join → `rejoindra` (no second send promised).
- View or add notes → `Voir le rapport ou y ajouter des notes`: `Voir` renders See (consult), `Afficher` renders Show,
  `Visualiser` is the F3 viewer.

## La fenêtre sélectionner / désélectionner des fichiers (`selection.*`)

- `Sélectionner` / `Désélectionner` (Finder `Tout désélectionner`, Total and Double Commander). The menu items, the
  commands, the setting, and the dialog titles all name the window the same way.
- Title `des fichiers` (not yet known) vs button `ces fichiers` (the shown result), like the English. Tooltips start
  with the button text verbatim.
- The recent-selections popover mirrors its `queryUi.recent.*` twins with `sélections` for `recherches`; the two keys
  sharing `Recent selections` stay identical.

## Un mot anglais, un mot français : la revue de dérive (`commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel`, `fileExplorer.summary.dirNoun`, `settings.control.resetToDefault`, `settings.advanced.resetAll`, `fileOperations.scanPhase.fromLabel`)

- F3 View → `Visualiser` over `Afficher` (Show) and `Présentation` (the menu).
- Reset to default → `Réinitialiser au réglage par défaut`: `Réinitialiser par défaut` makes `par défaut` an adverb.
- `{dir}` counts → `rép.`, matching the status bar’s `RÉP.`. From: → `De :`, pairing with `À`.
- Deliberate splits, don’t unify: Back → `Précédent` in the Go menu, `Retour` on Cmdr buttons; Edit → `Édition` (menu)
  vs `Modifier`; Unknown agrees with what it replaces (`Inconnu` model, `(inconnue)` size); App → `App` (a color source)
  vs `Application` (a scope); Running → `En cours d’exécution` (a process) vs `En cours` (a task); Canceled →
  `Opération annulée` (a panel title) vs `Annulé` (a status); Search → `Rechercher` (action) vs `Recherche` (a section);
  Put back → `restauré` (names) vs `remis en place` (locations).
- Send feedback: the title says `un retour`, the send button `le retour` (general action vs the object in front of you),
  like `Envoyer un rapport d’incident` / `Envoyer le rapport`.

## Mots qui ont divergé sans qu'aucun check puisse le voir (`settings.appearance.language.opt.system`, `settings.appearance.dateTimeFormat.opt.system`, `indexing.scan.counters`, `commands.viewFullMode.label`, `commands.viewBriefMode.label`)

- System default → `Par défaut du système`, never `Réglages Système` (Apple’s app).
- Finder tag → `tag` over `étiquette` (Finder). entries → `entrées` in both index counters.
- Keep → `Garder` to leave as is or keep running (`Garder les deux`), `Conserver` to preserve a thing or a duration
  (macOS makes the same split).
- Retry → `Réessayer` (button) vs `Nouvelle tentative` (the running state).

## Les noms de sous-fenêtres viennent du Mac de la personne (`errors.git.*`, `errors.provider.*`)

- The `{system_settings}`-style tokens are replaced at runtime with the Mac’s own names: a plain preposition before them
  (`dans`), never an elision or agreement.
- Names no token covers are plain text from macOS: `Compte Apple`, `Général`, `Ouverture et extensions`.

## `Restaurer` nomme l'objet : l'ancien nom (`askCmdr.renameUndo.undone`, `.partial`)

- `Anciens noms restaurés pour N fichiers.`: `restaurer` is the rename-undo verb, `remettre en place` the trash one; the
  participle agrees with the names.

## Une opération à moitié revenue en arrière : terminer le retour (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- Finish rolling back → `Terminer le retour en arrière` (Finder `Terminer la copie`): finish, never restart. Both
  `finishRollBack` keys stay identical.
- another pass → `un nouveau passage` (tentative) over `reprend l’opération` (reads as rerunning the original) and
  `une seconde passe` (jargon).
- in {folder} → `dans {folder}`: without the preposition the folder itself seems to be deleted.

## La notification après un retour en arrière interrompu (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- The `reason.*` lines reuse `askCmdr.renameUndo.skipReason.*`'s mold (`{name} laissé tel quel : …`); keys with
  identical English stay identical.
- `item` → `élément` here (the English says item), `fichier` in the rename twins; don’t unify.
- Complete titles carry totality with `tout` and move the count after a colon
  (`Cmdr a supprimé tout ce qu’il avait écrit : {countText} éléments.`), never `les {countText}`, which gives "les 1
  élément".
- Stopped after … → `Retour en arrière arrêté après …` over `Cmdr s’est arrêté` (reads as Cmdr quitting).
- these stayed → `donc voici ce qui n’a pas bougé :` over `resté en place` (`en place` means back where it was).
- Couldn’t undo {name} → `Cmdr n’a pas pu revenir en arrière sur {name}.`, never `annuler`.
- Leftovers happen `lors d’un transfert ultérieur`, never "la prochaine fois": cleanup spares anything under an hour
  old.

## L'écran de blocage quand le WebKit est trop ancien (`main.oldWebkit.*`)

- Software Update → `Mise à jour de logiciels` (the System Settings pane).

## L'avis « ancien macOS » (`main.oldMacos.*`)

- supports → `prend en charge` over the anglicism `supporte`; X and up → `X et les versions plus récentes`.
- best effort → `il fait au mieux`, never contract French (`au mieux de ses efforts`). The last sentence is David in the
  first person.

## Ce qu'Ask Cmdr lit à l'intérieur d'un fichier : consentement et rail (`ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.tool.inspectFile.*`)

- thumbnail → `vignette` (macOS, Total and Double Commander) over MS `miniature`. camera → `appareil photo`; photo
  location → `localisation` (Finder’s image panel) over `emplacement` (a file’s place on disk).
- "a photo’s camera details and location" → `pour une photo, les détails de l’appareil photo et la localisation`: the
  aside avoids two `photo` in three words.
- `inspectFile` → `Lecture du contenu de fichiers` / `A lu le contenu de fichiers`, like `imageFacts`; `contenu` says
  Cmdr reads inside.
- The promise is about approval, not read-only: `ne modifie jamais un fichier sans votre approbation`.

## Les deux info-bulles du bouton de retour en arrière (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- `Arrêter et remettre en place tous les fichiers déplacés jusqu’à présent`: undoing a move deletes nothing, so never
  `supprimer`.

## « Ouvrir un terminal ici » et son sélecteur d’app (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- `terminal` (the category) and `Terminal` (Apple’s app) stay English, like Apple’s `Ouvrir dans Terminal`; the card
  title is justified. The command → `Ouvrir un terminal ici`; Choose an app… → `Choisir une app…`.

## `Sort by relevance`: the search-results column tooltip (`fileExplorer.columns.sortByRelevance`)

- `Trier par pertinence`: `pertinence` in every macOS source, the frame of the `Trier par …` siblings.

## `Documents and packages` : la ligne OOXML (`settings.archives.ooxml.*`)

- `Documents et paquets`: bare `paquets` stays broader than the `Paquets d’application` card, the English’s own split.
- The description takes `un fichier` once instead of five `un`.

## Le hub des serveurs : panneau de connexion, refus et oubli (`servers.refusal.*`, `servers.paneState.*`, `fileExplorer.navigation.forget*`, `fileExplorer.navigation.disconnect*`, `menu.network.forgetServer`, `.forgetSavedPassword`)

- Keychain Access (the app) → `Trousseaux d’accès` (Apple’s plural); the store → `le trousseau`.
- trust → `approuver` (Security.framework) over `faire confiance à`. Host key → `la clé de {host}` (tentative).
- Signed out → `Session fermée`: the session carries the agreement, never `Déconnecté(e)`.
- Confirm titles repeat the menu labels verbatim (`Oublier le serveur`, `Oublier le mot de passe enregistré`).
- `disconnectPlaceAriaLabel` → `Se déconnecter de {name}`, containing the visible `Se déconnecter`, never Finder’s
  transitive `Déconnecter`.
- The disabled tooltip copies `ejectBusyTooltip`'s structure; `(occupé)` is for menu items only.
- Refusal toasts that name Cmdr keep it (`Cmdr n’a pas pu …`).

## Le hub des serveurs : la table, ses colonnes et ses états (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.groupNetwork`, `shortcuts.scope.servers`, `shortcuts.scope.places`)

- The row → `Serveurs`, inside the group `Réseau`. Places → `Emplacements` (Finder’s sidebar section), deliberately
  broader than `Partages`.
- Columns: `Nom`, `Type` (identical, Finder’s Kind column), `Adresse`, `État`, `Dernière utilisation`; never used →
  `Jamais`.
- States: `Connecté`, `Enregistré` (never `Inactif` / `Hors ligne`, which read as verdicts), `Découvert à proximité`,
  `En attente de votre vérification de la clé` (`vérifier`, distinct from `examiner` for looking at a key).
- pin / unpin → `épingler` / `désépingler` like the tabs, never AppKit’s `Ne plus épingler`. The toggle command uses
  `ou`, never a slash (`Épingler ou désépingler le serveur`).
- Toasts name the server, never a pronoun with a participle (`Le serveur reste enregistré.`). Empty state →
  `Pas encore de serveur` (like `Pas encore de conversation`).

## Le hub des serveurs : la feuille de connexion et la clé d'hôte SSH (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.*`, `goToPath.dialog.opensServer`, `.addsServer`, `commands.serversConnect.label`)

- `Protocole`; SMB / SFTP / WebDAV stay (Apple keeps them, justified). passphrase → `phrase secrète`
  (`Phrase secrète de la clé`), distinct from the account’s `mot de passe`.
- fingerprint → `empreinte` (feminine, so `Je l’ai vérifiée` agrees with it). trust a key → `approuver`, never Apple’s
  device-pairing `Se fier`.
- remote → `distant`, postposed (`Dossier distant`). Key file → `Fichier de clé`.
- Connect to server… → `Se connecter au serveur…` (the Go menu item) on every action surface; `Connexion au serveur` is
  only Apple’s window title.
- Sheet titles are infinitives (`Ajouter un serveur`, `S’identifier sur {name}`); How to connect →
  `Comment se connecter` (a question, not a field name).
- Sign in with a username and password spells both out, over `identifiants`, which is ambiguous in French.
- Cmdr won’t connect → `Cmdr ne se connectera pas à {name}` (a permanent refusal, like `hostKeyRevoked`).
- `nas.local` stays (a resolvable example). The Go-to-path previews are third person (`Ouvre {name}`,
  `Ajoute un serveur`): they describe what Enter will do.

## Le panneau de reconnexion et la session fermée sans mot de passe à saisir (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- `Reconnexion à {name}…`, the exact mold of `Connexion à {name}…`; `reconnexion` is the state noun, `reconnecter` the
  verb.
- "This server signs in with a key" → `Ce serveur vous identifie par une clé plutôt que par un mot de passe`: in this
  file `s’identifier` is the host-key sense, where the server proves itself.
- enter a value → `saisir` (Finder), never `taper`; `…, il n’y a donc rien à saisir.`

## Épingler un serveur au sélecteur, les clés d'hôte approuvées et la ligne ADB des réglages (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.appearance.tintSmb.*`)

- `Épingler au sélecteur` shortens `sélecteur de volume` exactly where the English says "switcher". Unpin →
  `Désépingler`, never `Retirer` (reads as removing the server).
- not found → `Introuvable`; found at {path} → `Trouvé : {path}` (Apple’s `<mot> : %@` status shape).
- Re-check → `Rechercher à nouveau` (Apple’s `Rechercher les mises à jour`) over `Vérifier`.
- Trusted <date> → `Approuvée le` (French needs `le`; agrees with `clé`). plugged in → `branché`.
- Watching for phones → `Cmdr détecte un téléphone dès qu’il est branché.`, never a technical module name.
- The server tint now covers three protocols: `Teinter les panneaux de serveur (SMB, SFTP, WebDAV)`.
- `settings.section.adb` (`Android (ADB)`) is identical and justified.

## Le téléphone Android : le panneau de connexion, les info-bulles et l'astuce ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

- Android’s own French wins on the phone (AOSP): `Autoriser`, `débogage USB`, `appuyez sur`, `activez son écran`
  (Android’s `activer l’écran`, not `réveiller`). Button names go unquoted, like the English.
- reseat the cable → `rebranchez le câble`; not responding → `ne répond pas`; lost the connection →
  `Cmdr a perdu la connexion avec votre téléphone` (`perdre` is suffered, `interrompre` is Cmdr’s choice).
- How → `Comment faire` (a bare `Comment` dangles). Check your phone → `Regardez votre téléphone` (tentative).
- `The Android tools on this Mac` stays generic (`Les outils Android de ce Mac`), never naming the daemon or protocol.
- You stopped opening your phone → `Vous avez arrêté l’ouverture de votre téléphone.`, never `annulé` (the button).

## L'identité verrouillée du serveur (`servers.sheet.identityLocked`)

- The hint names the actions exactly as their commands (`oublier`, `ajouter`); "name this server" →
  `identifient ce serveur`, since the sheet has its own `Nom` field.

## L'avis quand aucun mot de passe n'était enregistré (`fileExplorer.navigation.forgetSecretNoneToast`)

- `Aucun mot de passe n’était enregistré pour {name}.`: the imperfect states a fact, with no apology and no
  `impossible`.

## La durée des tentatives, le titre de clé d'hôte et le bouton Autoriser d'Android (`servers.paneState.retryTotalSeconds`, `.retryTotalMinutes`, `servers.paneState.hostKeyChanged`, `adb.readiness.waitingForAuthorization`)

- `{seconds}` / `{minutes}` only select the branch; `{secondsText}` shows. The values are bare fragments of
  `retryKeepsTrying`. `retryTotalMinutes` keeps its justification (the words match English).
- `En attente : sur votre téléphone, appuyez sur Autoriser` puts the phone first to avoid two `sur` in a row.

## Le menu contextuel de la ligne serveur : `Ouvrir` et `Modifier le serveur…` (`menu.network.open`, `menu.network.edit`)

- Open (a server row) → `Ouvrir`, like Finder. Edit server… → `Modifier le serveur…`, byte-identical to
  `commands.serversEdit.label`.

## La proposition d'ajout au Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `le Dock`, `le Finder`, `le dossier Applications` stay English with their article.
- keep in the Dock / unpin → `garder dans le Dock` / `supprimer du Dock` (Dock.app’s own menu) over the catalog’s
  `épingler`, because the user reads the Dock menu beside it.
- log in → `à l’ouverture de session` (Login Items), never `se reconnecter`.
- No, thanks → `Non, merci`, never `Plus tard`: this refusal is final.
- `addedButDockDidNotRestart` never says the add failed (it happened); `managedDock` names
  `La personne qui gère ce Mac`, never `l’administrateur`.

## Le menu du Dock : les cinq clés `menu.dock.*`

- Open <App> → `Ouvrir Cmdr`, unquoted (Dock.app quotes file names only). Go to Folder… → `Aller au dossier…`, distinct
  from `Aller au chemin…`. Connect to Server… → `Se connecter au serveur…`, never `Connexion au serveur` (Apple’s window
  title). Search files… → `Rechercher des fichiers…`, as `menu.edit.searchFiles`.
- `{name} ({parent})` is identical to English (Finder’s own `^0 (^1)` line) and justified.

## La proposition « Afficher dans le Finder » et l'avis de première fois (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `« Afficher dans le Finder »` quoted exactly as the settings card; Cmdr’s own window → `les Réglages`.
- The first-time notice explains, never apologizes (`Cmdr est configuré pour les récupérer`).

## La prise en main : l'assistant, l'étape IA et la liste de contrôle (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

- More about X → `En savoir plus sur {topic}` (Finder), a slot with no article or agreement.
- checklist → `liste de contrôle`; "each takes 30 seconds" → `30 secondes par point` (`chacune` would lack an
  antecedent). mailing list → `liste de diffusion`.
- Save → `Enregistrer`; the two email-field messages quote it, so rename all three together.
- GitHub’s star → `Ajouter une étoile` (GitHub’s French UI). AlternativeTo’s Like → `Aimer` (tentative; no French UI),
  never `Liker`.
- A switch summary reuses its description’s NOUN (`le processus macOS`): `suspendre` is the real sense, a bare
  `supprimer` would read as erase.
- A macOS permission is quoted verbatim (`« Réseau local »`), never paraphrased.
- `dumber` → `nettement plus bête`: the source is deliberately blunt. Released vs dev builds → `version publiée` /
  `versions de développement et de test`.

## La visionneuse récupère d'abord le fichier (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- fetch → `récupération` / `récupérer` (System Settings `Récupération…`); `téléchargement` stays for the internet.
- x of y → `{doneText} sur {totalText}` (Finder) over Thunar’s `de`. "so far" → `pour l’instant`, never `reçus` (it
  would agree with an unknown unit).
- `La récupération de ce fichier n’avance plus.` on the stalled-transfer mold.

## L'index d'un téléphone ADB paraît obsolète (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

- None of the three mentions disconnecting: the phone is still plugged in. `téléphone` because the English says phone
  (`appareil` stays the generic MTP word).
- `{name} ne prévient pas Cmdr quand ses fichiers changent`: the name is a subject with no agreement.

## La feuille du serveur : dossier racine et dossier initial (`servers.sheet.rootFolder*`, `.startFolder*`, `.nameHelp`, `servers.refusal.startFolderOutsideRoot`, `.rootNotFound`, `.startFolderNotFound`, `.saveUnconfirmed`)

- root folder → `dossier racine` (MS FRA, Thunar, Double Commander). start folder → `dossier initial` (tentative) over
  `dossier de départ` (Finder’s home folder is `Départ`) and `démarrage` (launching the app or Mac).
- Leave it empty → `Laissez ce champ vide pour …`; "a folder inside it" → `à l’intérieur de celui-ci`, which binds to
  the root folder unambiguously.

## Pourquoi un partage ne se monte pas ou sa liste ne se charge pas (`errors.mount.*`, `errors.shareList.*`)

- Terms from NetAuthAgent: `partage`, `invités`. reach → `joindre` / `joignable`.
- You’re signed in → `L’identification sur « {server} » … a réussi`: the sentence agrees with the identification, not
  the person.
- SMB 2 or later → `SMB 2 ou ultérieur`; Linux package → `paquet`. Keys with identical English stay identical.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- text editor → `éditeur de texte` (the category, not TextEdit, which arrives in `{app}`). The label
  `Modifier les fichiers avec` continues into the menu; `avec` and `dans` take any app name without an article.
- System default, Choose an app…, Dismiss, and Open settings copy their existing twins verbatim.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- `{name} est en cours de déconnexion`: the nominal form avoids agreeing with `{name}` (Thunar’s
  `Éjection du périphérique` shape).

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- `a été déconnecté` (done, masculine with `disque`), distinct from the in-progress `en cours de déconnexion`; "starts
  from scratch" → `repart de zéro`.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

- The reassurance stays LAST, where the English puts it. `sur {counterpart}` / `vers {counterpart}`: no article or
  elision before an inserted name.
- {done} of {total} files → `{done} des {total} fichiers`: raw family, no plural, and `{done} fichiers sur {total}`
  gives "1 fichiers".
- The rest → `Les autres fichiers sont toujours sur ce disque.` over the vaguer singular `Le reste`.
- Possessives follow the English key by key (`Vos originaux sont intacts`); after → `alors que Cmdr avait déjà copié`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

- Nothing went wrong: `Impossible de confirmer le déplacement`, never `Déplacement impossible` (a real read refusal).
- `Cmdr n’a pas pu confirmer … et a donc laissé vos originaux là où ils étaient`: coordinated, never `il`, which would
  bind to `{volumeName}`.
- `Vos originaux restent où ils sont.`; have a look → `Jetez un œil à la destination`; `réessayez le déplacement`.

## Les fichiers d'un déplacement incomplet retrouvés sur un disque (`fileOperations.leftovers.stagingFolderKept`)

- Never suggest deleting the folder: the files may be the person’s only copy.
- hidden (the file’s attribute) → `caché`, as in `Afficher les fichiers cachés` which the notice points to; `masqué` is
  what the UI isn’t showing now, and `masquer` stays the verb.
- unfinished → `incomplet` (matching `copie incomplète` nearby), keeping `interrompu` for interrupted. Origin →
  `issus d’un déplacement` (tentative), since `d’un` reads as ownership.
- `les a laissés là où ils sont` (present: where they are now); `Cmdr` stays the subject, never `il`.

## Le menu des favoris (`fileExplorer.navigation.favorites*`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `commands.favorites*`, `shortcuts.scope.favoritesMenu`)

- `favori` (Finder), never Nautilus’s `signet`. The menu → `le menu des favoris` (the orthodox hotlist’s word).
- Show favorites → `Afficher les favoris` (opens the menu); See favorites → `Voir les favoris` (the shorter row label).
- `seeFavorites` carries `=0` plus `one` / `many` / `other`; `=0` wins, so `one` only ever sees 1.
- current folder → `le dossier actuel`, over the anglicism `dossier courant`.
- `Ce dossier ne peut pas rejoindre vos favoris : les favoris ne fonctionnent que sur un disque ou un partage monté`:
  never name a protocol there, the reader doesn’t know which one they’re on.
- Number vs digit: `Ouvrir le favori portant ce numéro` (its rank), `appuyer sur un chiffre` (the key).

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- kind → `Type` (Finder’s Kind criterion). With extension → `Sélectionner tous les fichiers d’extension *.{extension}`
  (Total and Double Commander), so nothing agrees with the mask.
- `menu.context.selection` is a NOUN (the submenu title) → `Sélection`, not the verb `Sélectionner`.
- Each menu / command twin shares one English string, so it stays identical.

## La pastille « accès complet au disque » de la barre de titre (`onboarding.fdaBadge.*`)

- `Pas d’accès complet au disque`: calmer than `Aucun accès`, and identical to `onboarding.stepAi.bannerTitle.denied`.
- The aria starts with the label verbatim. The tooltip opens `Ce n’est pas obligatoire, mais …` to avoid `obligé`
  agreeing with the reader.

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`, `errors.write.readError.title.trash`, `errors.write.writeError.title.trash`)

- All five titles share `Placement dans la corbeille impossible`, never Nautilus’s `Mise à la corbeille`.
- No plural machinery, so `{count} des éléments que vous avez choisis` reads for any count.
- Never suggest trying again: a permission refusal repeats. `suggestion.other` names the `Détails techniques` control
  verbatim, and the quoted badge text is `onboarding.fdaBadge.label` verbatim.

## L'avertissement « contenu en ligne uniquement » (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- `en ligne uniquement` is Finder’s phrase for an evicted file (tentative).
- The four facts stay: the trash would download first, so Cmdr offers only deleting the whole selection, which leaves NO
  copy in the trash (don’t soften it), plus the ways out.
- The quoted « Supprimer » is the button’s value; "available offline" → `disponibles hors connexion`, the command’s
  words, never `hors ligne`.

## Quand le serveur répond qu'il n'a pas ce partage (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- The only case where retrying won’t help: nothing temporary in the tone.
  `le serveur indique qu’il n’a aucun partage de ce nom`; `indique` over the familiar `dit`.
- `Vous y avez toujours accès`; `Cette situation ne se réglera pas d’elle-même`. In the short toast, `qui` binds to
  `ce partage`, since `il` could mean `{server}`.

## Les noms « sosies » sur un serveur (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

- `semblent identiques`, `les écrit différemment`, `majuscules et minuscules`: everyday words over `casse` and
  `Unicode`.
- The one that’s there → `l’élément déjà présent`; choose it → `Choisissez plutôt l’élément voulu dans son dossier` (a
  pronoun would bind to the plural).

## L'interrupteur « Autoriser l'IA dans le cloud » et les états où l'IA dans le cloud est désactivée (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

- `Autoriser l’IA dans le cloud` (the switch); where the English uses it as a verb, the same words conjugated
  (`Autorisez-la …`).
- "X is off" agrees with X (`est désactivé` / `est désactivée`), like `servers.hub.discoveryOff`.
- Turn on → `Activer Ask Cmdr`; custom endpoints → `points de terminaison personnalisés`; side panel → `panneau latéral`
  (tentative).

## Échap et le plein écran (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `Quitter le mode plein écran avec Échap` (Finder’s command verbatim); the notice’s switch label and the setting are
  identical.
- Escape → `Échap`, `la touche Échap` at first mention (MS FRA). "a dialog or menu" is picked up with the masculine
  (`est ouvert`, `le fermer`).

## Originaux modifiés pendant le déplacement (`transfer.changedDuringMove`, 2026-09-25)

- **« changed during the move » → `a changé / ont changé pendant le déplacement`** · voix active, comme
  `fileOperations.cancelRollback.reason.drift.counted` (« ils ont changé ») ; le Finder `PE56` écrit au passif « ont été
  modifiés au cours de la gravure » (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25), mais le
  catalogue préfère l'actif. `pendant le déplacement` et `dossiers source` repris mot pour mot du voisin
  `transfer.appearedDuringMove`, affiché dans la même notification · `high`.

## Lignes d'attente de « Ouvrir avec » et « Partager » (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Lignes d'attente de « Ouvrir avec » et « Partager » (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- `Recherche d’apps…`, `options de partage`, `Aucune option de partage` (macOS’s empty-menu shape).
