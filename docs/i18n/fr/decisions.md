# fr decisions

Distilled rulings behind `terms.json`, one section per surface. Headings cite their keys (`pnpm i18n:brief` pulls by
them) and keep a plain apostrophe (the `decision` pointers match them by prefix); values write the catalog's `’`. Voice
and the top traps live in `style.md`; don't repeat them here.

## Réglages : sections et présentation (`settings.section.*`, `navigationAndFileOps.card.*`, `settings.summary.navigationAndFileOps`)

- Section titles are byte-identical wherever named; a cross-reference keeps the source's separator (`Réglages › IA`).
- Navigation & file ops → `Navigation et opérations` (no casual "ops"); the card keeps `Opérations sur les fichiers`.
- `mode Complet` / `mode Bref`, but `Présentation complète` / `brève` (each agrees with its noun).

## Licence et visionneuse (`licensing.dialog.*`, `licensing.section.*`, `viewer.saveAs.defaultName`)

- `Active` agrees with `licence`; `Image`, `PDF`, `Unicode` are French too (justified). `viewer.saveAs.defaultName`
  stays `selection`: a file-name-safe literal.
- Tiers: `Commerciale perpétuelle`, `Abonnement commercial`, `Personnelle (gratuite)`.

## La prise en main (`onboarding.*`, `commands.cmdrOpenOnboarding.label`, `shortcuts.scope.onboarding`, `main.upgradeNudge.*`, `settings.onboarding.upgradeNudgeShown.*`, `settings.revealHandler.notProductionBuild`)

- One name on every surface, over `configuration` / `accueil`: the menu item and its window must name each other.
- The stuck banner keeps `Privacy & Security > Full Disk Access` as English literals; only `{systemSettings}` localizes.
- More about X → `En savoir plus sur {topic}` (Finder).
- A switch summary reuses its description's noun (`le processus macOS`); a bare `supprimer` would read as erase. A macOS
  permission is quoted verbatim (`« Réseau local »`).
- `dumber` → `nettement plus bête` (deliberately blunt). Released / dev builds → `version publiée` /
  `versions de développement et de test`.

## Transferts et compteurs (`transfer.*`, `feedback.dialog.counter`, `errorReporter.dialog.counter`, `whatsNew.dialog.title`)

- Participles agree masculine with `fichier` / `dossier`; each `transfer.movedPhrase` branch stands alone.

## Raccourcis (`shortcuts.scope.*`, `shortcuts.system.*`, `shortcuts.section.*`, `downloads.shortcutRow.*`)

- Scope headings are nouns (`Fenêtre principale`, `Liste des fichiers`, `Sélecteur de volume`, `Menu des favoris`).
- Logging out of the Mac → `la fermeture de session` (Apple's `Fermer la session`); `déconnexion` reads as a share.
- `Fixe`; the Modified filter → `Modifiés` (the shortcuts you changed). Identical on purpose: `Global`, `OK`, `Options`,
  Spotlight, Mission Control, Spaces.

## La file d'attente des opérations (`queue.*`, `commands.queueShow.*`, `transferProgress.queue*`, `transferProgress.background*`, `transferProgress.pause`, `transferProgress.foregroundBusyToast`, `fileOperations.operationConflict.*`)

- Statuses: `En attente`, `En cours`, `En pause`, `Terminé`, `Annulé`, `N’a pas pu se terminer` (masculine: the implied
  operation). Needs your answer → `Réponse requise`, never `attente` (the queued status shares the column).
- Row labels are bare verbal nouns: `Copie`, `Déplacement`, `Suppression`, `Placement dans la corbeille`, `Renommage`,
  `Création du dossier`, `Création du fichier`, `Modification de l’archive`. Toasts and titles reuse them; the failure
  headline makes the noun the subject (`La copie n’a pas pu se terminer`), the `other` arm is bare.
- `Copie vers Backup` takes no `en cours` (it has a complement); don't "fix" it. Editing {destination} →
  `Modification de l’archive {destination}` (naming `archive` dodges the elision). Working in X →
  `Opération en cours dans X`; until you answer → `tant que vous n’avez pas répondu`.
- The window name is verbatim wherever named; `commands.queueShow.label` is the bare name; Show in queue →
  `Afficher dans la file d’attente des opérations` (long, see `review-queue.md`). `queueTooltip` keeps `transfert` (it
  describes one); `queuedToast` refers back in the feminine (`celle-ci`, `elle attend`).
- Row arias share `… cette opération`. Dismiss all → `Tout ignorer`, colliding with Skip all: accepted, they never share
  a surface. `foregroundBusyToast` names the operation (`affichez cette opération`).
- "to see why" → `pour savoir pourquoi` (number-neutral). Percent for screen readers → `pour cent`. Tooltip
  `{label} de {N} éléments vers {destination} · {P} % · {detail}`: `de` keeps the noun from reading as an imperative;
  optional parts keep their leading space inside their branch.
- Empty-queue button → `En arrière-plan` (MS FRA, Double Commander): bare `Arrière-plan` is a wallpaper, and
  `Passer en arrière-plan` wraps the 580 px row. Its aria ends in the label (WCAG 2.5.3):
  `Garder ce transfert en cours en arrière-plan`.

## Double-clic sur l'arrière-plan du panneau (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `settings.behavior.doubleClickOnPaneNotificationSeen.*`, `fileExplorer.breadcrumb.navigateTooltip`)

- Pane background → `espace vide` (Double Commander). Never do this again → `Ne plus jamais faire ça` (the navigation,
  not the hint); I like it → `J’aime bien` (`J’aime` is too strong).

## Fichier trop volumineux pour le système de fichiers (`errors.write.filesTooLargeForFilesystem.*`, `errors.listing.notSupportedErrno.suggestion`)

- `formaté en FAT32`; `ne peut pas stocker de fichiers de plus de X`. The overflow line follows Finder's
  `et ^0 de plus`, noun per branch.

## Les dialogues de copie et de suppression (`transferDialog.*`, `fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`)

- Chooser aria → `Action` (identical, justified); Scanning… → `Analyse…`. "Cmdr will create it" →
  `Cmdr le créera lors de la copie.` / `… du déplacement.`, active over Thunar's `sera créé`.
- Trash switch → `Placer dans la corbeille` (Finder); destructive button `Supprimer`. From / To → `De` / `À` (Total and
  Double Commander) over `Vers`; headings take no colon, path labels do (`De :`).

## Archives, paquets et compression (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `errors.listing.archiveUnreadable.*`, `errors.mutation.archive*`, `fileOperations.archivePassword.*`, `commands.fileCompress.*`, `transferProgress.scanTitleCompress`, `transfer.compress.*`)

- `archive` is feminine (`chiffrée`); `Archives` is identical, justified. The password body names `L’archive {name}`, so
  `archive`, never `{name}`, drives `protégée` and `la déverrouiller`. Sender → `la personne qui vous l’a envoyée`.
- The OOXML row `Documents et paquets` keeps bare `paquets` broader than `Paquets d’application`; its description says
  `un fichier` once.

## Presse-papiers (`fileExplorer.clipboard.*`, `settings.fileOperations.pasteClipboardAsFile.*`)

- "as X" → `en tant que X`; saved as → `enregistré sous` (AppKit). The toast hangs `collé` off masculine `Contenu`, the
  kind in a bare parenthetical; don't move the participle after the kind.
- `Chemin copié, il est maintenant dans le presse-papiers :`, definite article like macOS; the path has its own line.

## L'historique des opérations (`operationLog.*`, `commands.logOperationLog.*`)

- It pairs with `File d’attente des opérations` in the View menu. Didn't finish → `Non terminé`, never `Échec`.
- Summaries are count-led participles in every branch (`{countText} éléments copiés`); archive outcomes are feminine.
  Provenance `Vous`, `Client IA`, `Agent` (identical, justified).

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`, `suggestedOps.*`, `commands.suggestedOpsShow.*`, `errors.listing.deviceReconnecting.*`, `navigation.driveIndex.tooltipCoalesced*`)

- unarchive → `désarchiver`; badge `Archivée` (conversation); on-device → `en local`.
- Tool lines: verbal noun / `A <participe> …`, no pronoun (`Vérification de vos disques` / `A vérifié vos disques`),
  outcomes too. Pairs beat stem symmetry (`Recherche…` / `A cherché…`). `inspectFile` → `Lecture du contenu…` /
  `A lu le contenu…`. Inserts take a type noun (`du dossier {name}`).
- Ask about X → `Posez des questions sur X` (empty state), `Posez une question…` (field), button
  `Interroger la sélection`. `renameReview.expired` keeps the brand (`Demandez à Cmdr…`).
- rotate (through a cycle) → `permuter`; `faire pivoter` is images. `askCmdr.stalled` ends `…ou arrêter` (Stop).
- "This can't be undone" → `Cette opération est irréversible` (Finder). reason → `raison` (`motif` is a pattern).
- Rename undo → `Anciens noms restaurés pour N fichiers.`: `restaurer` for names, `remettre en place` for the trash.
- AI suggestions (`suggestedOps.title`) → `Suggestions de l’IA`, as `fileOperations.mkdir.aiSuggestions*`.

## IA dans le cloud et lecture des fichiers (`ai.cloudConsent.*`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`, `askCmdr.tool.inspectFile.*`)

- Switch `Autoriser l’IA dans le cloud`; as a verb, the same words conjugated (`Autorisez-la …`). "X is off" agrees with
  X, like `servers.hub.discoveryOff`. Turn on → `Activer Ask Cmdr`; custom endpoints →
  `points de terminaison personnalisés`; side panel → `panneau latéral` (tentative).
- The promise is about approval: `ne modifie jamais un fichier sans votre approbation`.

## Index d'images (`settings.mediaIndex.*`, `search.imageResults.networkOff`, `search.imageResults.paused`, `fileExplorer.imageIndex.*`)

- network drive → `disque réseau`; `photo` feminine. gently / limited speed → `en douceur` / `à vitesse limitée`
  (tentative). Internal lists mirror `settings.indexing.silencedDrives.*`.
- Per-file statuses agree with `image` (`indexée`, `incluse`, `réindexée`); couldn't → `Indexation impossible`. Indexing
  now → `Indexation en cours`, distinct from queued `En attente d’indexation`.
- `folder.allIndexed` carries "all" by the missing fraction, never `toutes` ("Toutes les 1 image"). Drive tooltips open
  `Sur ce disque,` (no double `sur` near "X sur Y").
- Search by description → `la recherche par description`; the card stays `Recherche sémantique`. Delete model (reclaim
  {size}) → `Supprimer le modèle (libérer {size})`.

## Index de disque (`navigation.driveIndex.*`, `settings.indexing.masterOffNote`, `settings.indexing.overriddenBadge`, `search.coverage.toast.deferredUntilSearchEnds`, `indexing.*`)

- The master switch in prose → bare `l’indexation`; `l’indexation du disque` is the per-drive sense these strings rule
  out, and the scope marker (`pour ce disque` / `dans les Réglages`) carries it. Unindexed → `Cmdr n’indexe pas {name}`.
- `Cmdr effectue une recherche sur {name}` over `parcourir`; the wait → `dès la fin de cette recherche`
  (`dès qu’elle sera terminée` could bind to `l’indexation`).
- Checking for changes → `Recherche des changements`, nominal like its siblings; `analyse` is a full check.
- Stale phone: never mention disconnecting (still plugged in); `téléphone` as the English says;
  `{name} ne prévient pas Cmdr quand ses fichiers changent`.
- Leaving → `{name} est en cours de déconnexion` (nominal, Thunar's `Éjection du périphérique` shape); unplugged →
  `a été déconnecté` (agrees with `disque`); starts from scratch → `repart de zéro`.

## Le dialogue d'incident (`crashReporter.dialog.*`, `settings.updates.crashReports.description`)

- `a quitté inopinément` over the unattested `s’est fermé de façon inattendue`. kept running →
  `a continué son exécution` (AppKit) over `a continué de fonctionner` (the pile's compatibility sense) and
  `est resté ouvert` (a window).
- `.keptRunning` holds no closing word; `.unknown` asserts neither. `la dernière fois` ends the clause (every pile hit);
  `continuer à` + infinitive. The setting's label keeps `Envoyer les rapports d’incident`.

## Réseau : connexion, identification et partages (`fileExplorer.network.*`, `errors.listing.authRequiredEauth.explanation`, `errors.listing.authRequiredEneedauth.explanation`, `errors.provider.*`, `errors.mount.*`, `errors.shareList.*`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- Connected → `Connecté`, Logged in → `Identifié` (one column, two words); `Identification refusée` / `requise`.
  Participles agree with the host. Terms from NetAuthAgent: `partage`, `invités`; Linux package → `paquet`.
- Signing in to an app follows Apple's `Se connecter à`, never as a participle on the reader: an action or the session
  (`Vérifiez que votre session iCloud est ouverte`). Signed in → `L’identification sur « {server} » … a réussi`.
- macOS fallback: `la connexion réseau SMB native de macOS` (MS FRA). `La connexion directe … n’a pas pu être établie`
  (as `directConnectionUnexpectedToast`); `Essayer de se connecter directement`; "for most connections" →
  `dans la plupart des cas` (no fourth `connexion`).
- Share not on the server is the one case where retrying can't help, so nothing temporary:
  `le serveur indique qu’il n’a aucun partage de ce nom` (over `dit`), `Vous y avez toujours accès`,
  `Cette situation ne se réglera pas d’elle-même`. In the toast, `qui` binds to `ce partage` (`il` could be `{server}`).
- require → `exiger` (macOS `Exiger un mot de passe`): `Toujours exiger le chiffrement SMB`, in prose
  `l’option « … » est activée dans Cmdr`.

## Transferts à l'arrêt et déplacements incomplets (`transferProgress.stall*`, `transferProgress.close`, `errors.write.deviceDisconnected.*`, `errors.write.moveNotConfirmed.*`, `errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*`, `errors.write.deletePending.message`, `fileOperations.leftovers.*`)

- Close → `Fermer` (the transfer keeps running), distinct from `Annuler` / `Ignorer`.
  `Aucune progression depuis {duration}` (preformatted), `Le transfert n’avance plus.`,
  `En attente d’une réponse de la destination / de la source`, `Le fichier journal donne les détails.`. `stallInFlight`
  pulls "may already be partly written" into the branches.
- Drive pulled: the reassurance stays last; `sur` / `vers {counterpart}`. `{done} des {total} fichiers` (raw, no plural;
  `{done} fichiers sur {total}` gives "1 fichiers"); `Les autres fichiers sont toujours sur ce disque.`; possessives
  follow the English key by key; `alors que Cmdr avait déjà copié`.
- Unconfirmed move is no failure: `Impossible de confirmer le déplacement`, never `Déplacement impossible`;
  `… et a donc laissé vos originaux là où ils étaient` (an `il` would bind to `{volumeName}`);
  `Vos originaux restent où ils sont.`; `Jetez un œil à la destination`; `réessayez le déplacement`.
- Original kept: `le dossier qui le contient` (no pronoun agreeing with the item); `supprimez vous-même l’original`.
- Leftovers: never suggest deleting them (maybe the only copy). unfinished → `incomplet` (as `copie incomplète`),
  `interrompu` for interrupted; `issus d’un déplacement` (tentative; `d’un` reads as ownership);
  `les a laissés là où ils sont`, Cmdr the subject.

## La visionneuse récupère d'abord le fichier (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- `{doneText} sur {totalText}` (Finder) over Thunar's `de`; so far → `pour l’instant` (`reçus` would agree with an
  unknown unit). `La récupération de ce fichier n’avance plus.`

## La barrière de sortie (`main.quit.*`)

- `Quitter alors qu’une opération est en cours ?` (concessive `alors que` over `pendant que`). "stays done" →
  `Tout ce qui est déjà terminé le reste.` (true for deletes too). logout → `une fermeture de session`.
- Countdown `Cmdr quitte dans …` (`se ferme` collides with `fermeture`). Keep working → `Continuer à travailler` (bare
  `Annuler` cancels the operations, bare `Continuer` is macOS's go-ahead); `Quitter maintenant`; `Toujours en cours`.

## Statistiques d'usage (`settings.analytics.enabled.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`, `onboarding.stepBeta.analyticsTitle`)

- `statistiques d’usage` `relié à` `un identifiant aléatoire`, over `anonymes` (false) and `pseudonyme` (jargon the
  English avoids); `identifiant` over MS `identificateur`.

## Retour en arrière (`transferProgress.rollback*`, `transferProgress.conflictRollback`, `transferProgress.titleRollingBack`, `operationLog.rollback.*`, `operationLog.tree.rollupRolledBack`, `operationLog.dialog.rollBack`, `operationLog.dialog.rollBackRest`, `commands.logOperationLog.description`, `settings.operationLog.intro`, `fileOperations.rollbackConfirm.*`, `fileOperations.cancelRollback.*`, `queue.row.reversalInFolder`)

- A rollback DELETES what the operation wrote, so `restaurer` promises the opposite (and names real restores); `annul-`
  collides with Cancel and `Annulé`. Always with `en`: `retour arrière` is Backspace and a rewind. Pills:
  `Retour en arrière possible` / `impossible` / `en cours` / `effectué` / `partiel`.
- Finish → `Terminer le retour en arrière` (Finder `Terminer la copie`), both `finishRollBack` keys identical. another
  pass → `un nouveau passage` (tentative) over `reprend l’opération` and `une seconde passe`; `dans {folder}`, or the
  folder itself seems deleted.
- Confirm: carries on → `continuera` (`reprendre` is resume); `Conserver les fichiers`; `Arrêter et …`, never `Annuler`.
  `Arrêter et remettre en place tous les fichiers déplacés jusqu’à présent` (undoing a move deletes nothing).
- Interrupted: `reason.*` reuse `askCmdr.renameUndo.skipReason.*` (`{name} laissé tel quel : …`); `élément` in both
  families. Totality → `tout` and the count after a colon
  (`Cmdr a supprimé tout ce qu’il avait écrit : {countText} éléments.`). `Retour en arrière arrêté après …` over
  `Cmdr s’est arrêté` (quitting); `donc voici ce qui n’a pas bougé :` (`en place` means put back);
  `Cmdr n’a pas pu revenir en arrière sur {name}.`; `réessaiera la prochaine fois qu’il se connectera à cet endroit`.

## Renommage et création (`fileExplorer.rename.*`, `fileOperations.newEntry.stillCreating`, `fileExplorer.clipboard.stillPasting`, `fileOperations.validation.nameNotUsable`, `errors.mutation.*`, `errors.volume.*`)

- "and so did “{name}”: {reason}" → `ainsi que « {name} » : {reason}` (Finder's "X et N autres"), the name by the colon.
- Slow volume: `<Nom verbal> de « X » toujours en cours. Le volume met du temps à répondre.`, never `il` / `elle` for X.
  `Renommage de « X » et de {n} autres fichiers` (`de` repeats: complements).
  `Le nom du fichier ne peut pas être utilisé`, no period (it fills `{reason}`).
- `« {path} » n’existe plus.` (Finder). `Impossible de renommer ici le dossier racine d’un volume.` SIP →
  `la protection de l’intégrité du système`, in Finder's `en raison de …`. Out of / between archives → `sortir` /
  `faire passer` (`Déplacer` is the command named next).
- `Ce mot de passe n’est pas le bon.` (tentative; softer than macOS `incorrect`). `errors.mutation.timedOut` is no
  failure (`… donc la modification peut encore aboutir`); `errors.volume.deviceSessionReset` is no unplug.

## Corbeille (`errors.mutation.trashNotSupported`, `errors.mutation.trashRefused`, `fileOperations.trash.*`, `commands.*Trash.*`, `menu.edit.undoTrash`, `errors.write.trashRefused.*`, `errors.write.*.title.trash`, `fileOperations.delete.cloudOnline*`)

- `Ce volume n’a pas de corbeille : la seule option est de supprimer définitivement.`;
  `macOS a refusé de placer cet élément dans la corbeille.` (`a refusé` for "wouldn't").
- Undo → `Annuler le placement dans la corbeille` (Finder's `Annuler le déplacement de…`). Put back →
  `remettre en place` (Finder `Remettre`). `de retour` is invariable; the partial result agrees via `{skipped}`. Button
  and command share `Aller à la corbeille`.
- The five titles share `Placement dans la corbeille impossible`. `{count} des éléments que vous avez choisis` (no
  plural). Never suggest retrying (a permission refusal repeats); `suggestion.other` names `Détails techniques`
  verbatim; the badge quote is `onboarding.fdaBadge.label` verbatim.
- Online-only: `en ligne uniquement` (Finder, tentative). Keep all four facts: trashing would download first, so Cmdr
  offers only deleting the whole selection, which leaves NO copy in the trash (don't soften), plus the ways out. «
  Supprimer » is the button's value; available offline → `disponibles hors connexion`, never `hors ligne`.

## Éjection et déconnexion refusées (`errors.eject.*`)

- Each value follows `Impossible d’éjecter {volumeName} :` / `Impossible de se déconnecter :`: capitalized, never a
  second `Impossible de …`.
- Cmdr drops a device → `déconnecter`; leaving a server → `se déconnecter`. idle → `quand il n’est plus occupé`;
  `il n’y a donc rien à éjecter`. `Cmdr n’a pas reconnu cet appareil et ne peut donc pas le déconnecter` (a second `il`
  would bind to `cet appareil`). `timedOut` is no failure: `… donc l’éjection peut encore aboutir d’elle-même.`
- Who holds it: the place may be a disk, share, or phone, so `y` (never `ce disque`) and `puis réessayez` (never
  `éjectez-le à nouveau`). `X y utilise encore quelque chose.`; `{app}` is a bare subject; `Fermez cette app` /
  `ces apps`, no pronoun for an insert; `Cmdr n’a pas de nom d’app`, never `Nous`. `d’autres apps` (lowercase, no
  period; `Intl.ListFormat` joins); stored there → `qui s’y trouve`; `envoyez un rapport`, never `un retour`.

## Compléter un rapport déjà envoyé (`errorReporter.amend*`)

- `Ajouter à votre rapport d’incident`, `Ajouter au rapport`, `Ajout…` (Apple's `Ajouter à X`) over `Compléter`.
  `Ce qui a été envoyé` mirrors `Ce qui va être envoyé`; join → `rejoindra`; View or add notes →
  `Voir le rapport ou y ajouter des notes`.

## Sélection (`selection.*`, `menu.select.*`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `Sélectionner` / `Désélectionner` (Finder `Tout désélectionner`, Total and Double Commander); menus, commands,
  setting, and titles name the window alike. Title `des fichiers` vs button `ces fichiers`; tooltips start with the
  button text. The recent popover mirrors `queryUi.recent.*`; the two `Recent selections` keys stay identical.
- kind → `Type` (Finder). `Sélectionner tous les fichiers d’extension *.{extension}` (Total and Double Commander), so
  nothing agrees with the mask. `menu.context.selection` is the noun `Sélection`.

## Un mot anglais, un mot français (`commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel`, `fileExplorer.summary.dirNoun`, `settings.control.resetToDefault`, `settings.advanced.resetAll`, `fileOperations.scanPhase.fromLabel`, `settings.appearance.language.opt.system`, `settings.appearance.dateTimeFormat.opt.system`, `indexing.scan.counters`, `commands.viewFullMode.label`, `commands.viewBriefMode.label`, `commands.fileDuplicate.*`, `fileExplorer.columns.sortByRelevance`)

- From: → `De :`.
- Deliberate splits, don't unify: Back → `Précédent` (Go menu) / `Retour` (buttons); Edit → `Édition` / `Modifier`;
  Unknown agrees (`Inconnu` model, `(inconnue)` size); App → `App` (a color source) / `Application` (a scope); Running →
  `En cours d’exécution` (a process) / `En cours` (a task); Canceled → `Opération annulée` (a title) / `Annulé` (a
  status); `Rechercher` / `Recherche` (a section); Put back → `restauré` (names) / `remis en place` (places).
- Send feedback: title `un retour`, button `le retour` (like `Envoyer un rapport d’incident` / `Envoyer le rapport`).
- System default → `Par défaut du système`, never `Réglages Système`. entries → `entrées` in both counters.

## Apps externes (`settings.behavior.openTerminalHereApp.*`, `navigationAndFileOps.card.terminal`, `settings.behavior.textEditorApp.*`, `navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`, `menu.context.openWithLoading`, `menu.context.shareLoading`, `menu.context.shareNone`)

- `Modifier les fichiers avec` continues into the menu; `avec` / `dans` take any app name bare. Its option and button
  labels copy their twins verbatim.
- `Recherche d’apps…`, `options de partage`, `Aucune option de partage` (macOS's empty-menu shape).

## Menus natifs (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Beyond `style.md`'s list: `Services`, `Dossier parent`, `Départ`, `Trier par`; text zoom stays `Zoom`. Select menu →
  `Sélectionner` (Linux; Finder has none); `Croissant` / `Décroissant`.
- changelog → `Journal des modifications` vs Help > `Nouveautés`; word wrap → `Retour à la ligne automatique`; tabs
  follow Safari (`Épingler l’onglet`, `Désépingler l’onglet`). Server row: `Ouvrir`, `Modifier le serveur…` (=
  `commands.serversEdit.label`).
- Identical on purpose: `menu.app.services`, `menu.sort.extension`, `menu.view.zoom`, `menu.tag.orange`,
  `menu.tag.rowLabel`, `menu.view.askCmdr`.

## Les noms de sous-fenêtres viennent du Mac de la personne (`errors.git.*`, `errors.provider.*`)

- The `{system_settings}`-style tokens become the Mac's own names: a plain `dans` before them, never elision or
  agreement. Untokenized names are macOS text: `Compte Apple`, `Général`, `Ouverture et extensions`.

## macOS et WebKit trop anciens (`main.oldWebkit.*`, `main.oldMacos.*`)

- supports → `prend en charge` over `supporte`; best effort → `il fait au mieux`, never contract French. The last
  sentence is David in the first person.

## Le hub des serveurs : la table et le panneau (`servers.hub.*`, `servers.paneState.*`, `servers.refusal.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.section.servers`, `commands.servers*`, `navigation.groupNetwork`, `navigation.forget*`, `navigation.disconnect*`, `menu.network.*`, `shortcuts.scope.servers`, `shortcuts.scope.places`, `settings.appearance.tintSmb.*`)

- Row `Serveurs` in group `Réseau`; Places → `Emplacements` (Finder's sidebar), broader than `Partages`. Columns `Nom`,
  `Type` (Finder's Kind), `Adresse`, `État`, `Dernière utilisation`; never used → `Jamais utilisé`. Empty →
  `Pas encore de serveur`.
- States: `Connecté`, `Enregistré` (never `Inactif` / `Hors ligne`: verdicts), `Découvert à proximité`,
  `En attente de votre vérification de la clé` (`examiner` is looking at a key).
- `Épingler ou désépingler le serveur`. `Épingler au sélecteur` shortens `sélecteur de volume` where English says
  "switcher"; `Désépingler du sélecteur`; `pinHint.body` quotes it. Trusted <date> → `Approuvée le` (agrees with `clé`).
  Host key → `la clé du serveur {host}` (`de` would elide; tentative).
- Confirm titles repeat the menu labels; forget confirmations name the object after the question
  (`Cmdr n’affiche plus ce serveur` / `ce partage`); button `Oublier`. `Se déconnecter de {name}` contains the visible
  label (never Finder's `Déconnecter`). The disabled tooltip copies `ejectBusyTooltip`; `(occupé)` is menus only.
- Toasts after an insert take a colon or a verb, never an agreeing participle: `Serveur oublié : {hostName}`,
  `Cmdr a enregistré {name} dans votre liste Serveurs`, `Pas de connexion à {name}` (a heading),
  `Le serveur reste enregistré.`, `Aucun mot de passe n’était enregistré pour {name}.` (a fact, no apology); refusals
  keep `Cmdr n’a pas pu …`.
- `en tant que « {username} »` (no `qu’` elision); `en tant qu’invité`; `S’identifier en tant que…` over Finder's
  `Se connecter comme…`; `Utiliser l’accès invité` (tentative).
- `Reconnexion à {name}…` (mold of `Connexion à {name}…`). "signs in with a key" →
  `Ce serveur vous identifie par une clé plutôt que par un mot de passe` (the server proves itself). enter → `saisir`
  (Finder), never `taper`: `…, il n’y a donc rien à saisir.`
- `{seconds}` / `{minutes}` only select; `{secondsText}` shows; the values are bare fragments of `retryKeepsTrying`.

## Le hub des serveurs : la feuille (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`, `goToPath.dialog.addsServer`, `commands.serversConnect.label`)

- `Protocole`; SMB / SFTP / WebDAV stay. passphrase → `phrase secrète`. remote → `distant`, postposed; `Fichier de clé`.
- Sheet titles are infinitives (`Ajouter un serveur`, `S’identifier sur {name}`); `Comment se connecter`. Username and
  password spelled out over the ambiguous `identifiants`. `Cmdr ne se connectera pas à {name}` (permanent).
- `Ajouter` / `Ajouter et ouvrir` / `Ajouter quand même` (Apple's `Enregistrer quand même`); `Facultatif`. `nas.local`
  stays. Go-to-path previews are third person (`Ouvre {name}`, `Ajoute un serveur`).
- The locked-identity hints name actions as their commands; "name this server" → `identifient ce serveur` (the sheet has
  its own `Nom`).
- start folder → `dossier initial` (tentative); `Laissez ce champ vide pour …`; inside it → `à l’intérieur de celui-ci`.

## Le stockage S3 (`servers.sheet.s3*`, `servers.sheet.accessKeyId`, `servers.sheet.secretAccessKey`, `*.coldStorage*`)

- bucket → `compartiment`, `ID de clé d’accès`, `clé d’accès secrète` (AWS FR console, MS FRA); cold storage →
  `stockage froid`; archived → `archivé`.

## Les serveurs NFS (`servers.sheet.nfs*`, `servers.sheet.identityLockedNfs`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`, `servers.refusal.noNfsV3`, `servers.hub.nfsHint*`, `stateRecovery.nfsPlaces.*`)

- export → `partage` (tentative) over `export`: Synology's and QNAP's French UIs say `dossier partagé`, and `export`
  reads as `exportation`. Never `ses partages` after `{host}`: `la liste des partages`.
- The `insecure` switch's NAS label → `Permettre les connexions à partir des ports non privilégiés` (Synology DSM FR).
  `ID utilisateur` / `ID de groupe`; "Guessed" → `Déduits` (agrees with the IDs).

## Le Dock, le Finder et Aller au dossier (`menu.dock.*`, `main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`, `main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`, `menu.go.goToPath`, `goToPath.dialog.*`, `commands.navGoToPath.label`)

- keep in the Dock / unpin → `garder dans le Dock` / `supprimer du Dock` (Dock.app's menu, read beside it) over
  `épingler`. `addedButDockDidNotRestart` never says the add failed.
- `Aller au dossier` (Finder); `Rechercher des fichiers…` (as `menu.edit.searchFiles`); the Go button → `Aller` (Finder
  `FR24`). `{name} ({parent})` is identical (Finder's `^0 (^1)`), justified.
- `« Afficher dans le Finder »` quoted as the settings card; the first-time notice explains, never apologizes
  (`Cmdr est configuré pour les récupérer`).

## Le menu des favoris (`navigation.favorites*`, `menu.go.showFavorites`, `commands.favorites*`, `shortcuts.scope.favoritesMenu`)

- `favori` (Finder), never `signet`; `le menu des favoris`. Number vs digit: `Ouvrir le favori portant ce numéro` /
  `appuyer sur un chiffre`.
- `Ce dossier ne peut pas rejoindre vos favoris : un favori pointe vers un dossier sur un disque, un partage, un serveur ou un téléphone`:
  no protocol names, the reader can't tell which.

## La pastille « accès complet au disque » (`onboarding.fdaBadge.*`)

- `Pas d’accès complet au disque` (calmer than `Aucun accès`; = `onboarding.stepAi.bannerTitle.denied`). The aria starts
  with the label; the tooltip opens `Ce n’est pas obligatoire, mais …` (no `obligé` agreeing with the reader).

## Le téléphone Android (`adb.*`, `settings.adb.*`, `settings.section.adb`, `settings.behavior.adbHintDismissed.*`)

- `activez son écran` (Android's `activer l’écran`, not `réveiller`); button names unquoted, like the English.
  `En attente : sur votre téléphone, appuyez sur Autoriser` (phone first: no double `sur`).
- `rebranchez le câble`; `ne répond pas`; `Cmdr a perdu la connexion avec votre téléphone` (`perdre` is suffered,
  `interrompre` chosen). `Comment faire` (bare `Comment` dangles). `Regardez votre téléphone` (tentative).
  `Les outils Android de ce Mac`, never a daemon or protocol. `Vous avez arrêté l’ouverture de votre téléphone.`
- Settings: `Introuvable`; `Trouvé : {path}` (Apple's `<mot> : %@`); `branché`;
  `Cmdr détecte un téléphone dès qu’il est branché.`; `Android (ADB)` identical, justified.

## Les noms « sosies » (`transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

- `semblent identiques`, `les écrit différemment`, `majuscules et minuscules` over `casse` and `Unicode`.
  `l’élément déjà présent`; `Choisissez plutôt l’élément voulu dans son dossier` (a pronoun would bind to the plural).

## Échap et le plein écran (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `Quitter le mode plein écran avec Échap` (Finder), identical in the notice and the setting. `la touche Échap` at first
  mention (MS FRA). "a dialog or menu" takes the masculine (`est ouvert`, `le fermer`).
