# fr decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`, plus `concepts-proposed.json`); open questions for a native reviewer live in `review-queue.md`.
Style and voice: `style.md`.

ICU values double every apostrophe (`d''incident`); the RAW families (`errors.*`, `menu.*`, `licensing.windowTitle.*`,
`main.instanceLock.*`) use a single one. Quoted values below usually write the single apostrophe for readability; some
older sections quote the ICU value with its doubled one.

## Sections des réglages et noms de présentation (`settings.section.*`, `settings.navigationAndFileOps.card.*`, `settings.summary.navigationAndFileOps`)

Keep these consistent in every file that names them:

- Appearance → Apparence; Colors and formats → Couleurs et formats; Zoom and density → Zoom et densité; File and folder
  sizes → Tailles de fichiers et de dossiers; Listing → Liste; Behavior → Comportement; File operations → Opérations sur
  les fichiers; File system watching → Surveillance du système de fichiers; Search → Recherche; AI → IA; File systems →
  Systèmes de fichiers; SMB/Network shares → Partages SMB/réseau; MTP → MTP; Git → Git; Viewer → Visionneuse; Developer
  → Développeur; MCP server → Serveur MCP; Logging → Journalisation; Updates & privacy → Mises à jour et
  confidentialité; Advanced → Avancé; Keyboard shortcuts → Raccourcis clavier; License → Licence.
- **Navigation & file ops (the short sidebar section) → `Navigation et opérations`**: French has no clean casual
  abbreviation for "ops", so the word is spelled out; the card `settings.navigationAndFileOps.card.fileOperations` keeps
  the full `Opérations sur les fichiers`, and `…card.navigation` is identical to English (`Navigation`, with its
  `sameAsSourceJustification`).
- View modes: Full → Complet, Brief → Bref (`mode Complet`, `mode Bref`; after the feminine `présentation`:
  `Présentation complète`, `Présentation brève`). Columns: Name → Nom; Ext → Ext (kept short).
- In-app cross-references quote the section title verbatim: `Réglages > Mises à jour`,
  `Réglages > Mises à jour et confidentialité`, `Réglages › IA` (the separator mirrors the English key).

## Licence, visionneuse : les valeurs identiques à l'anglais (`licensing.dialog.*`, `licensing.section.*`, `viewer.binaryWarning.kind.*`, `viewer.saveAs.defaultName`)

- "Active" (a license's validity) stays `Active`: the same spelling, the feminine of `actif`, agreeing with `licence`.
- The kind words `image` / `document` (binary-view warning) and `Image` / `PDF` / `Unicode` (view-mode labels) are
  identical or near-identical in French; left as they are on purpose.
- `viewer.saveAs.defaultName` stays `selection`: the `@key` asks for a lowercase, unaccented, file-name-safe literal.
- License-tier labels: `Commerciale perpétuelle`, `Abonnement commercial`, `Personnelle (gratuite)`. The standalone
  "Commercial perpetual" value drops the noun, so the adjective agrees with the implied feminine `licence`.

## La prise en main porte un seul nom (`onboarding.wizard.title`, `onboarding.wizard.progressLabel`, `commands.cmdrOpenOnboarding.label`, `shortcuts.scope.onboarding`, `main.upgradeNudge.*`, `settings.onboarding.upgradeNudgeShown.*`, `onboarding.stepAi.bannerBody.stuck`)

- **`prise en main`, everywhere**: the menu item and command (`Prise en main…`), the shortcut scope, the wizard title
  (`Prise en main de Cmdr`), its progress bar (`Progression de la prise en main`), the internal flag, and any sentence
  about the flow (`Vous pouvez terminer la prise en main maintenant`). ❌ Not `configuration`, not `accueil`, and not a
  standalone title like `Bienvenue dans Cmdr`: the menu item and the window it opens must recognize each other. The
  `main.upgradeNudge.*` menu path once read `Configuration…` (a forward guess) and now matches the real label; the
  generic phrase "onboarding options" stays descriptive (`options de configuration`).
- The stuck banner's breadcrumb keeps `Privacy & Security > Full Disk Access` as English literals, like the source: the
  `{systemSettings}` token is the only OS-localized part.
- "Quit & Reopen" (the button macOS shows itself) → `Quitter et rouvrir` (unverified against a live macOS; see
  `review-queue.md`).

## Transferts et compteurs (`transfer.*`, `feedback.dialog.counter`, `errorReporter.dialog.counter`, `whatsNew.dialog.title`)

- Past participles agree masculine with `fichier` / `dossier` (`fichier copié` / `fichiers copiés`, `dossier déplacé` /
  `dossiers déplacés`). `transfer.movedPhrase` is built so each `kind` branch stands alone grammatically.
- The counters `{currentText} / {maxText}` are pure placeholders, legitimately identical to English.
- `whatsNew.dialog.title` → `Nouveautés de Cmdr`.

## Les raccourcis : portées, fonctions de macOS et valeurs identiques (`shortcuts.scope.*`, `shortcuts.system.*`, `shortcuts.section.*`, `downloads.shortcutRow.*`)

- Scope group headings: App → Application; Main window → Fenêtre principale; File list → Liste des fichiers; Brief/Full
  mode → Mode Bref/Complet; Volume chooser → Sélecteur de volume; Command palette → Palette de commandes; About window →
  Fenêtre À propos; Onboarding → Prise en main; Favorites menu → Menu des favoris; Servers → Serveurs; Share browser →
  Navigateur de partages.
- Reserved-shortcut list (lowercase mid-sentence, as the source): `le changement de source de saisie`,
  `l'enregistrement de l'écran`, `les captures d'écran`, `la fermeture de session` (logging out of the Mac, the Apple
  menu's `Fermer la session`; `la déconnexion` would read as dropping an SMB share), `le verrouillage de l'écran`,
  `le sélecteur d'applications`, `Fenêtres de l'application`, `la fenêtre de recherche du Finder`. Spotlight, Mission
  Control, and Spaces stay English (macOS keeps them).
- `shortcuts.section.alreadyBound` quotes the command in `« {command} »` (the source uses straight quotes); the `<b>`
  tag stays.
- Fixed (badge) → `Fixe`; the Modified filter chip → `Modifiés` (the shortcuts you changed, plural).
- Legitimately identical to English: `Global` (downloads scope title), `OK`, `macOS`, `Options`, and the Spotlight /
  Mission Control / Spaces names.

## La file d'attente : statuts et libellés des lignes (`queue.row.status`, `queue.row.label`, `queue.row.pause`, `fileOperations.transferProgress.pause`)

- `queue.row.status`: Waiting → `En attente`; Running → `En cours`; Paused → `En pause`; Done → `Terminé`; Canceled →
  `Annulé`; Couldn't finish → `N'a pas pu se terminer` (the gentle wording for a stopped operation). Participles are
  masculine, agreeing with the implied operation word of the row.
- `queue.row.label` mirrors the `transferProgress.titleActive` verbal nouns without `en cours` (short row labels):
  Copie, Déplacement, Suppression, Placement dans la corbeille, Renommage, Création du dossier, Création du fichier,
  Modification de l'archive (Nautilus "Renommage de …", "Création des …").
- The standalone `Pause` button is identical to English and valid French (macOS keeps it).

## Double-clic sur l'arrière-plan du panneau (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `settings.behavior.doubleClickOnPaneNotificationSeen.*`, `settings.section.navigationAndFileOps`, `fileExplorer.breadcrumb.navigateTooltip`)

- The label reads `Double-cliquez sur l'arrière-plan du panneau pour remonter au dossier parent`; the description
  `C'est l'espace vide autour de la liste de fichiers, pas une ligne de fichier.` keeps the English mix of "pane"
  (label) and "file list" (description), and `C'est` points back at the pane background named in the label.
  `espace vide` is Double Commander's exact phrase ("un espace vide d'un panneau").
- The hint's conversational lines (friendly `vous`, no pile source, tentative): What just happened? →
  `Que s'est-il passé ?`; Don't like it? → `Vous n'aimez pas ?`; Never do this again → `Ne plus jamais faire ça` (the
  navigation, as opposed to `ne plus afficher`, which would mean the hint); I like it → `J'aime bien` (not the
  over-strong `J'aime`).
- The internal flag's participle agrees with the feminine `astuce`: `Astuce … affichée`.
- `fileExplorer.breadcrumb.navigateTooltip` → `Cliquez pour accéder à {path}` (macOS `cliquez`, `accéder à`).
- The summary line uses no serial comma before `et`.

## Fichier trop volumineux pour le système de fichiers (`errors.write.filesTooLargeForFilesystem.*`, `errors.listing.notSupportedErrno.suggestion`)

- `.title.one` "File too large for this drive" → `Fichier trop volumineux pour ce disque`, tracking Nautilus's title
  almost verbatim; `trop volumineux`, never `trop grand` (the pile keeps that for image dimensions).
- Naming a concrete format: `formaté en FAT32` (macOS speaks of `le format du volume`, and
  `errors.listing.notSupportedErrno.suggestion` says `formaté avec un système de fichiers` for the generic case).
- "can't store files larger than X" reuses that suggestion's exact precedent:
  `ne peut pas stocker de fichiers de plus de X`. FAT32 and exFAT stay verbatim.
- The overflow line "and {countText} more file(s)" → `et {countText} fichier(s) de plus`, Finder's `et ^0 de plus`
  (`LocalizableMerged.json` N141.3), with the catalog's `one` / `many` / `other` noun fragment.

## Les libellés du dialogue de copie (`fileOperations.transferDialog.operationAria`, `.scanFile`, `.scanDir`, `.targetWillBeCreatedCopy`, `.targetWillBeCreatedMove`)

- The screen-reader label for what the control chooses → `Action`: a genuine French word, byte-identical to English, so
  it carries a `sameAsSourceJustification`.
- "Scanning…" (spinner tooltip and label while the dialog counts) → `Analyse…`, the settled scan noun, single `…`.
- "This folder doesn't exist yet. Cmdr will create it during the copy/move." →
  `Ce dossier n'existe pas encore. Cmdr le créera lors de la copie.` / `… lors du déplacement.`: active
  (`Cmdr le créera`, not Thunar's passive `sera créé`), `le` for the masculine `dossier`, two literal sentences with no
  ICU select.

## Parcourir les archives et les paquets (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `errors.listing.archiveUnreadable.*`, `errors.mutation.archive*`, `queue.row.label`)

- `archive` is feminine and genuinely French, so it inflects (`une archive`, `chiffrée`, `protégée`); the bare card and
  section title `Archives` is identical to English and carries `sameAsSourceJustification`.
- App bundles: Finder calls a bundle a `paquet` (`Afficher le contenu du paquet`); the card and the row both say
  `Paquets d'application`, the same words, as the brief asked.
- editable → `modifiable` (`seules les archives zip sont modifiables`): the adjective keeps it active and dodges the
  passive `peuvent être modifiées`.
- open with the default app → `ouvrir avec l'application par défaut`: the full `application` here, because macOS attests
  `application par défaut` / `une autre application` directly; elsewhere the catalog says `app`.
- The Enter key → `la touche Entrée` catalog-wide. Segmented cells: Ask → `Demander`, Browse → `Parcourir`, Open →
  `Ouvrir`.
- "Editing archive" (the `archive_edit` arm of `queue.row.label`) → `Modification de l'archive`.
- A fresh copy (ask the sender for one) → `une nouvelle copie` (macOS). The sender stays gender-neutral:
  `la personne qui vous l'a envoyée`, whose participle agrees with the feminine `archive` through `l'`.

## Coller le presse-papiers comme fichier (`fileExplorer.clipboard.pastedAsFile*`, `settings.fileOperations.pasteClipboardAsFile.*`)

- "as X" → `en tant que X` (Finder's `Copier en tant que lien`); "saved as {name}" → `enregistré sous` (the AppKit save
  panel).
- Radio options: Do nothing → `Ne rien faire` (standard French, no pile hit); Create file / Create and rename →
  `Créer un fichier` / `Créer et renommer`, reusing `fileExplorer.functionKeyBar.newFileAction`.
- **The paste toast is gender-safe by construction**:
  `Contenu du presse-papiers collé dans {filename} ({kind, select, image {image} pdf {PDF} other {texte}})`. The
  participle `collé` hangs off the masculine head noun `Contenu`, the varying kind sits in a bare parenthetical, and
  `{filename}` stays in a neutral slot after `dans`. A participle placed after the kind would have to agree (image →
  `collée`, texte → `collé`). The branch names stay verbatim.

## Le mot de passe d'une archive (`fileOperations.archivePassword.*`)

- The body names the archive explicitly, `L'archive <archive>{name}</archive> est protégée…`, so the feminine antecedent
  `archive` (never the uncontrolled `{name}`) drives every agreement: `protégée`, and the pronoun in `la déverrouiller`.
  The input's accessible name is `Mot de passe de l'archive`.

## Compresser (`commands.fileCompress.*`, `fileOperations.transferDialog.toggleCompress`, `.confirmCompress`, `fileOperations.transferProgress.scanTitleCompress`, `transfer.compress.*`, `settings.archives.compressionLevel.*`)

- `Compresser` everywhere (Finder), never `Comprimer`. The progress select branch takes the noun `Compression` with the
  sibling `… en cours...` tail; `scanTitleCompress` = `Vérification avant la compression...`.
- The result toast's participle `compressés` mirrors `transfer.split.clean` (`{phrase} copiés`) and the `one` / `many` /
  `other` shape of `fileOnly.allDone`.
- The overwrite warning uses `remplacera`, and names `l'archive`; `.zip` goes in straight double quotes.
- The level slider: `Niveau de compression`, with `Plus rapide` (level 1, quicker packing, not app speed) and
  `Plus petit` (level 9, the smaller output file).

## L'historique des opérations (`operationLog.*`, `commands.logOperationLog.*`)

- **operation log → `Historique des opérations`**: the feature is a history view, and its English `@key` descriptions
  call it "operation history" throughout, so Apple's history word (`historique`) fits better than the technical
  `journal`, which stays the log-FILE sense (`journalisation`, `fichier journal`). The dialog title and the command
  share one value, and the View menu pairs it with `File d'attente des opérations` on the same head noun.
- **`Annulé` and the rollback pills must stay distinct at a glance**: `Canceled` (you stopped it before it ran) is
  `Annulé`; the rollback family is `retour en arrière`. That is why rollback never takes `annul-` (see the `rollback`
  section below).
- Status pills match `queue.row.status`: Queued → `En attente`, Running → `En cours`, Done → `Terminé`, Canceled →
  `Annulé`; Didn't finish (status and item outcome) → `Non terminé`, never `Échec`. Per-item outcomes: `Terminé`,
  `Ignoré`, `Non terminé`, and the rollback outcome shares `Retour en arrière effectué` with its status twin.
  `status.done` / `outcome.done` and `status.failed` / `outcome.failed` share their English, so they render identically.
- Summary lines are count-led past participles agreeing masculine with `élément` / `fichier` / `dossier`
  (`{countText} éléments copiés`, `… placés dans la corbeille`, `{countText} dossiers créés`), with `{countText}` in
  every `one` / `many` / `other` branch. Edited / extracted an archive → `Archive modifiée` / `Archive extraite`
  (feminine).
- Provenance: You → `Vous` (macOS), AI client → `Client IA`, Agent → `Agent` (identical, a genuine French word, with
  `sameAsSourceJustification`).
- The overflow line "and {countText} more item(s)" → `et {countText} élément(s) de plus`.

## Ask Cmdr : la conversation, les jetons et les lignes d'outil (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- **chat (a saved thread) → `conversation`, one word everywhere.** MS FRA's `chat` renderings
  (`conversation instantanée`, `clavardage`) are live-chat features; the English source itself uses "chat" and
  "conversation" for the same entity. The verb "to chat" → `discuter`, which keeps `conversation` free for the noun.
  Three keys that had drifted to `discussion` were aligned, and "Chat memory" is `mémoire de conversation` (never
  `chat`, which is a cat in French).
- **token → `jeton`** on every key: MS FRA renders `token` as `jeton` in every technical sense, including the parsing
  one closest to ours, and French macOS publishes neither word, so Tier 2 decides. It mattered because
  `askCmdr.error.localWindowTooSmall` told the reader to pick "32 768 jetons" in a setting whose description said
  "tokens".
- archive a chat → `archiver`; unarchive → `désarchiver` (standard, no pile hit, the `dés-` pattern of
  `désélectionner`); the badge `Archivée` agrees with the implicit `conversation`.
- on-device (the free local model's cost readout) → `en local`, a plain-language footnote rather than the provider
  label.
- "Ask about X": verb-first `Posez une / des question(s) sur X`, except the compact attach button,
  `Interroger la sélection`, short enough for a button.
- thinking → `Réflexion…`, the noun-plus-ellipsis progress convention (`Analyse…`, `Vérification…`), single `…`.
- **Tool status lines** pair a deverbal noun phrase (`Vérification de vos disques`) with `A <participe> …`
  (`A vérifié vos disques`), with no subject pronoun; the `unknown` fallback is `Travail en cours` /
  `A utilisé un outil`. Every pair keeps this shape (`proposeRenamePlan.done` = `A préparé un plan de renommage`).
  `searchPhotos` keeps `Recherche dans vos photos` / `A cherché dans vos photos`, exactly like the `operationsList`
  pair: cross-pair consistency outranks stem symmetry, so don't "fix" one without the other.
- "That request wasn't available" → `Cette demande n'était pas disponible`; "This one hit its limit" →
  `Celle-ci a atteint sa limite` (`celle-ci` agrees with the implicit `réponse`); "Not now" on the consent screen →
  `Plus tard`.
- The logging setting → `Journaliser les appels au modèle d'IA`, and the consent note refers back to it as the noun
  `la journalisation des appels au modèle d'IA`.
- drop to attach → `Déposer pour joindre` (macOS `Boîte de dépôt` plus the catalog's `joindre`, tentative); attachment →
  `pièce jointe`, remove → `Retirer la pièce jointe`.
- `askCmdr.stalled` ends `…ou arrêter`, mirroring the Stop button `Arrêter`; an `l'` before it had no antecedent.
- `askCmdr.renameReview.expired` says `Demandez à Cmdr…`: the English uses the brand as a verb phrase, which collapses
  into `demander`; `Cmdr` stays, so the don't-translate check holds.
- `Ask Cmdr` alone (the panel title, the Settings section, the command, the switch) carries `sameAsSourceJustification`.

## L'indexation des images sur les disques réseau (`settings.mediaIndex.networkVolumes.*`, `settings.mediaIndex.alwaysIndexVolumes.*`, `settings.mediaIndex.alwaysIndexFolders.*`, `search.imageResults.networkOff`, `.paused`)

- network drive → `disque réseau`, deliberately not Microsoft's Windows `lecteur réseau`.
- `photo` is feminine: `photo indexée`, `photos indexées`, in every plural branch.
- The resumption line: `reprend quand ce disque se reconnecte`.
- gently → `en douceur`; at a limited speed → `à vitesse limitée` (natural French, no pile phrase, tentative).
- always index → `Toujours indexer ce disque` / `Toujours indexer les photos sur {name}`; the internal lists
  `Disques à toujours indexer` / `Dossiers à toujours indexer`; "get indexed anyway" → `soient indexées malgré tout`;
  photo archive → `archive de photos`.
- Internal (hidden) settings read like `settings.indexing.silencedDrives.*`: an `Interne : …` lead and the third-person
  `l'utilisateur`.

## Revue qualité : renommages proposés et lignes d'outil (`askCmdr.renameReview.*`, `askCmdr.tool.*`, `askCmdr.stalled`, `errors.listing.deviceReconnecting.*`, `fileExplorer.imageIndex.*`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- allow / deny per row → `Autoriser` / `Refuser`; all → `Tout autoriser` / `Tout refuser` (the catalog-wide
  `Tout <verbe>` pattern: `Tout éjecter`, `Tout ignorer`, `Tout écraser`).
- review (the approve-or-deny list) → `vérifier`, noun `vérification`: macOS AppKit's `Vérifier les modifications…`. Not
  `revoir` (re-reading a document) and not MS's `revue` (publishing).
- rename cycle → `cycle de renommage`; the badge `(cycle)` is identical to English. rotate (files through the cycle) →
  `permuter` (MS `swap`); deliberately not macOS's `rotation` / `faire pivoter`, the spatial image sense.
- The overwrite badge → `(écrasement !)`, with the ASCII space before `!`.
- **remove a folder from the indexing list → `Retirer`, not macOS's `Supprimer`.** The help text exists to promise that
  removing a folder deletes nothing, and `supprimer` is the catalog's delete, so it would say the opposite.
- searchable (what stays findable after a folder leaves the list) → `reste disponible dans la recherche`, anchored on
  `recherche` so the promise stays about search (`consultable` loses that; tentative).
- one indexing pass → `passage` (`au prochain passage`, tentative).
- `errors.listing.deviceReconnecting.suggestion` was the catalog's only `tu` address and is now `vous`
  (`Patientez quelques secondes, puis réessayez.`).
- The `driveIndex.tooltipCoalesced*` tooltips close calmly (`remettra tout d'aplomb`, `rien de grave donc`), the
  reassuring register their `@key` asks for.

## Les pastilles d'état de l'index d'images (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- `image` is feminine and the badge sits on an image file, so every per-file status agrees feminine: `indexée`,
  `incluse`, `modifiée`, `réindexée`.
- "Couldn't be indexed" → `Indexation impossible`; off (for a drive) → `désactivée`, mirroring
  `driveIndex.tooltipDisabled`.
- `folder.allIndexed` / `folder.someIndexed` are fragments with no final period: `{totalText} images indexées` and
  `{doneText} sur {totalText} images indexées`. "All" is carried by the absence of the fraction, never a literal
  `toutes`, which would break the `one` branch ("Toutes les 1 image"); `{totalText}` sits outside the plural so it shows
  in every rendering.
- The drive tooltips lead with `Sur ce disque,` to avoid a double `sur` next to "X sur Y"; `drive.indexing` closes
  `; indexation en cours.` and `drive.done` says `sont indexées`, which states completeness without a fragile
  `toutes les {n}`.
- The settings pair: `Afficher des pastilles d'état sur les images` and a third-person help line
  (`Ajoute une petite pastille sur chaque image de la liste des fichiers…`).

## Les réglages de l'indexation des images (`settings.mediaIndex.cards.*`, `settings.mediaIndex.progressSummary.title`, `settings.mediaIndex.semanticSearch.label`, `settings.mediaIndex.clip.*`, `fileExplorer.imageIndex.file.indexing`)

- "Indexing now" (the image being processed right now) → `Indexation en cours`, for both the badge tooltip and the
  live-progress heading; deliberately distinct from `file.pending` (`En attente d'indexation`, queued).
- Enable indexing → `Activer l'indexation`; Folders to index → `Dossiers à indexer`.
- search by description → `la recherche par description`, anchored on `clip.ready` / `clip.description`; the card title
  stays `Recherche sémantique` (and the model is the `modèle de recherche sémantique`). The toggle →
  `Rechercher des photos par description`.
- "a Mac with Apple silicon" → `un Mac équipé d'une puce Apple Silicon` (`clip.notSupported`).
- Delete model (reclaim {size}) → `Supprimer le modèle (libérer {size})`; Deleting… → `Suppression…`.
- The delete-failure line stays calm: `Le modèle n'a pas pu être supprimé pour le moment. Réessayez dans un instant.`

## Le dialogue de suppression : l'interrupteur corbeille, De et À (`fileOperations.delete.trashSwitch`, `.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`, `.targetGroupTitle`)

- The switch → `Placer dans la corbeille` (Finder AL13 / N153), identical to the `titleVerbOnly` trash arm; the
  destructive button → `Supprimer`, identical to its delete arm.
- From / To headings → `De` / `À`: Total Commander (`662="De : "`, `663="À : "`) and Double Commander ship this pair in
  the same copy/move dialog. macOS's `Déplacer vers :` settles a preposition inside a verb phrase, not a standalone
  heading, so bare `Vers` was set aside. The headings carry no colon; the scan phase's and the delete dialog's path
  labels do (`De :`).

## L'indexation des disques désactivée (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- **"Drive indexing" (the master switch), in prose → `l'indexation`, bare.** The catalog renders the concept bare
  everywhere, and the scope marker carries the global-vs-per-drive distinction: `… pour ce disque` (one drive) vs
  `… dans les Réglages` plus `aucun disque` (the master switch). `l'indexation du disque` reads as "the indexing of THE
  drive", exactly the per-drive meaning these strings exist to rule out. The setting label stays `Indexation du disque`
  and is quoted verbatim in the path (`Activez-la dans Indexation > Indexation du disque`).
- "stays unindexed" with the uncontrolled `{name}` → `Cmdr n'indexe pas {name}`: active, so nothing agrees with a name
  of unknown gender; it mirrors `refusedGeneric`.
- "picks up where it left off" → `reprendra là où il s'était arrêté` (French wants `là où`).
- "folder sizes stay hidden" → `les tailles des dossiers restent masquées`, the catalog's plural form.
- The override badge → `Désactivé avec l'indexation`: a masculine state label agreeing with the implied `réglage`, three
  words for the badge slot.
- "ready for when you turn this back on" → `prêt pour le moment où vous la réactiverez`: `la` is the feminine
  `l'indexation` that opens the note.

## Le dialogue d'incident : `a quitté inopinément`, `a continué son exécution`, `la dernière fois` en fin de proposition (`crashReporter.dialog.*`, `settings.updates.crashReports.description`)

The report dialog at the next start picks its opening sentence from three, by what the report recorded: `.ended` (Cmdr
went down with the incident), `.keptRunning` (a background problem, the app carried on, and the user quit it later), and
`.unknown` (a report from an older version that didn't record what followed). ❌ No closing word belongs in
`.keptRunning`, and `.unknown` must stay true whether or not Cmdr quit, so it asserts neither.

- **quit unexpectedly → `a quitté inopinément`** · `fr/macOS/AppKit/AppKitErrors.json` (« Lors de sa précédente
  ouverture, %@ a quitté inopinément pendant la réouverture des fenêtres. ») · high. Apple coined the concept and the
  French user reads this phrase in the system's own crash dialogs. The earlier `s'est fermé de façon inattendue` had no
  attestation anywhere in the pile; it was a paraphrase.
- **kept running (the APP, not an operation) → `a continué son exécution`** · `fr/macOS/AppKit/NSExceptionAlert.json`
  `69.title` (« … pour continuer l'exécution de l'application dans un état instable … »), Apple's own exception dialog,
  which is exactly this surface · high. ❌ Not `en cours d'exécution` (zero catalog hits), ❌ not
  `a continué de fonctionner` (`fonctionner` means "work / be compatible" everywhere in the pile), ❌ not
  `est resté ouvert` (it describes a window, not a process).
- **ran into a problem, Cmdr as subject → `Cmdr a rencontré un problème`** · Finder `NE105` (« « ^0 » a rencontré une
  erreur. ») for the app-as-subject shape, MS FRA style guide § 4.1.9 for the collocation (« Nous avons rencontré un
  problème… »); `problème` replaces the banned `erreur` · high. Apple's impersonal « Un problème s'est produit lors de…
  » was set aside: the three variants fill one sentence of one dialog, and `.ended` already has Cmdr as subject.
- in the background → `en arrière-plan` (MS FRA `tâche en arrière-plan`, Double Commander, Dolphin); `en tâche de fond`
  has no pile hit.
- **`la dernière fois` goes at the END of the clause, never in front** · all three pile occurrences put it last
  (`AppKit/Document.json`, Thunar, Dolphin), and so do the catalog's; when Apple wants it first it changes the
  construction (`Lors de sa précédente ouverture, …`). So the three bodies share one frame,
  `Cmdr <verbe> … la dernière fois.`
- `continuer à` + infinitive, not `continuer de` (macOS PE79–PE81 « continuer à copier les autres », Thunar), a question
  that comes back with every continuity sentence.
- **"a report" without "crash" → `un rapport`, plain.** The second sentence of `.keptRunning` / `.unknown` repeats
  `.ended`'s minus `d'incident`, the one word that carries "crash", exactly as the English does; `rapport d'incident`
  stays the crash's. Title and confirmation follow the same cut: `Envoyer le rapport d'incident ?` /
  `Envoyer le rapport ?`, `Rapport d'incident envoyé. …` / `Rapport envoyé. …`.
- `crashReporter.dialog.privacyNote` was already neutral (`la partie du code concernée`), so it didn't move when the
  English went from "crashed" to "ran into the problem".
- **The setting's description covers both cases** (`settings.updates.crashReports.description`):
  `quand Cmdr quitte inopinément ou rencontre un problème en arrière-plan`, `un rapport` plain, and the privacy sentence
  of `privacyNote`. ❌ The LABEL keeps `Envoyer les rapports d'incident`: it is the setting's name.

## Les états du navigateur réseau : connecté ou identifié (`fileExplorer.network.browser.status.*`, `fileExplorer.network.browser.tooltip.requiresLogin`, `errors.listing.authRequiredEauth.explanation`, `errors.listing.authRequiredEneedauth.explanation`)

- **Connected → `Connecté`; Logged in → `Identifié`.** Both statuses show in the same column of the network browser, so
  they can't share a word; `se connecter` is the network action and `s'identifier` the sign-in (`terms.json` `sign-in`).
  Login failed → `Identification refusée`, Login needed → `Identification requise`, "This host requires login" →
  `Cet hôte demande une identification`. The participle agrees with the host, never the person.
- A server "rejecting the current login" → `refuse vos identifiants actuels`; "requires you to log in" →
  `vous demande de vous identifier`.

## Se connecter à une app ou à un compte (`errors.provider.*`)

- Signing in to an app or an Apple account follows Apple's own `Se connecter à iCloud`, but ❌ never as a participle
  that genders the reader (`Assurez-vous d'être connecté`). Phrase it as an action or name the session:
  `Connectez-vous à {app} si ce n'est pas déjà fait`, `Vérifiez que vous utilisez le bon compte Apple`,
  `Vérifiez que votre session iCloud est ouverte`, `… et que votre session est ouverte`.

## Une recherche en cours sur un disque (`fileExplorer.navigation.driveIndex.deferredEnable`, `.deferredRescan`, `search.coverage.toast.deferredUntilSearchEnds`)

- "Cmdr is searching {name} right now" → `Cmdr effectue une recherche sur {name}`: a search is running, and `parcourir`
  would read as browsing. The follow-up names what it waits for, `dès la fin de cette recherche`, because
  `dès qu'elle sera terminée` could bind to `l'indexation` / `la nouvelle analyse` (both feminine) as easily as to the
  search.

## Index de disque : l'analyse des changements (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- **"Checking for changes" (run-kind header) → `Recherche des changements`** · nominal phrase matching the sibling
  headers (`Première analyse complète`, `Mise à jour rapide`); `Recherche de…` is the standard French UI shape for a
  "checking for X" label, and `changements` is catalog-settled (`Rattraper les changements récents`) · high.
- **"Update the file list" → `Mettre à jour la liste des fichiers`** · composed from the settled siblings
  `Enregistrer la liste des fichiers` + `Mettre à jour l''index` · high.
- **"the check running right now" → `l''analyse en cours`** · reuses `analyse` as this catalog's settled word for a full
  check (`tooltipCoalesced`: "la prochaine analyse complète de Cmdr") and that string's closing
  `remettre tout d''aplomb` · high.

## Transferts à l'arrêt : le bandeau de blocage (`fileOperations.transferProgress.stall*`, `.close`)

Settled during the stalled-transfer pass (`fileOperations.transferProgress.stall*` + `close`). ICU values, so single
apostrophes doubled below to match this doc's convention:

- close (button that closes the progress dialog while the transfer keeps running) → **Fermer** · macOS AppKit
  (`Document.json`, `WindowTabs.json`: "Close" → "Fermer"), MS terminology FRA ("Close" → "Fermer") · high — distinct
  from "Annuler" (Cancel) sitting next to it, and from the crash-reporter dismiss "Ignorer" (that one is a
  dismiss-without-acting, this one really closes a window).
- stalled / no progress (a transfer that has stopped moving) → **Aucune progression depuis {duration}** · "progression"
  is macOS's word for transfer progress (`NSProgressPanel` "Progression", Finder `AirDropProgressView`) and is already
  the catalog's (`Progression de la taille`, `Progression des fichiers`); "depuis + durée" is the standard FR "for the
  past X" shape · high — deliberately NOT "bloqué"/"échec"/"erreur": it states the observation, not a verdict.
- "the transfer has stopped moving" → **Le transfert n''avance plus.** · descriptive calm FR; no pile source names this
  state (no "stall" entry in MS terminology FRA, no hit in the four file-manager catalogs), so this is composed from the
  settled `transfert` + the plain negative "n''avance plus" · high for the term, tentative for the sentence shape.
- waiting for X to respond → **En attente d''une réponse de {la destination / la source}** · Double Commander ("Waiting
  for user response" → "En attente de la réponse utilisateur", "Waiting for access to file source" → "En attente de
  l''accès au fichier source"), macOS ("En attente de la mise à jour", "En attente du chargement", `SavePanel` "Waiting
  for disc drive…" → "Attente du lecteur de disque…"), MS terminology FRA ("stop responding" → "ne plus répondre") ·
  high — "En attente de…" is the pile's dominant shape; the indefinite "d''une réponse" avoids implying a specific
  expected reply.
- source (the device/share being read FROM) → **la source** (feminine) · Double Commander ("Source" → "Source"), Total
  Commander ("répertoire de source", "disque de source") · high — pairs with the settled `destination → destination`
  (also feminine), so both take "de la".
- still open (a file the transfer hasn''t closed yet) → **encore ouvert / encore ouverts** · KDE Dolphin ("…are open
  within an application" → "…sont ouverts dans une application"), Double Commander ("the file is open in another
  program" → "le fichier est ouvert dans un autre programme") · high.
- partly written → **partiellement écrit / partiellement écrits** · the catalog''s own settled shape
  (`errors.git.missingObject.message`: "Le dépôt est peut-être partiellement récupéré"), macOS ("partiellement
  disponible") · high.
- "the log has the details" → **Le fichier journal donne les détails.** · reuses the catalog''s existing near-twin
  `askCmdr.renameUndo.refusedBatches` ("The operation log has the details." → "L'historique des opérations donne les
  détails.") and the settled `log file → fichier journal` (`settings.logging.openLogFile`: "Ouvrir le fichier journal",
  MS terminology FRA) · high — "fichier journal" (not bare "journal") because this string points at Cmdr''s log FILE,
  while `Historique des opérations` is the separate in-app operation history.

Phrasing notes for this pass:

- **`stallInFlight` moves the trailing clause INSIDE the plural branches.** English keeps "and may already be partly
  written." outside the `{count, plural, …}`; French can''t, because the participle has to agree with the counted noun
  ("ouvert et … écrit" vs "ouverts et … écrits"). Parity only compares the placeholder SET
  (`apps/desktop/scripts/i18n-check-parity.ts`), so pulling literal text into the branches is safe and is the right move
  whenever a trailing clause has to agree. The ellipsis of the second verb ("est encore ouvert et peut-être déjà
  partiellement écrit", not "et est peut-être…") is what keeps it readable.
- FR CLDR `one`/`many`/`other` on `stallInFlight`; `many` written identical to `other` (plain integers never select
  `many`, but the parity/plural checks want the branch), matching every other plural in the `fr` set. French counts 0 as
  `one`, and "0 fichier est encore ouvert" is correct French, so the singular branch is safe there too.
- Non-alarmist throughout, as the whole point of these strings: no "erreur", no "échec", no "a échoué", no "bloqué". The
  copy states what is observed ("Aucune progression depuis…", "En attente d''une réponse…", "n''avance plus") and offers
  the two ways out.
- `stallUnknown` drops the English comma before "ou" (French doesn''t take one in a two-item choice) and uses the `vous`
  imperative pair "Annulez-le ou laissez-le continuer en arrière-plan", reusing the settled
  `background → en arrière-plan`.
- No `:` `;` `!` `?` `%` in any of the eight values, so the catalog''s ASCII-space-before-punctuation rule doesn''t come
  up. All apostrophes are ASCII and doubled (`d''une`, `n''avance`); rendering was verified with `intl-messageformat`
  under locale `fr` for counts 0/1/2/5/1 000 000.
- `{duration}` arrives pre-formatted ("45s", "2m 30s") from `$lib/units`, so it is NOT localized by this catalog; the
  sentence is built so any length or shape reads correctly after "depuis".

## Chemin copié : la confirmation du presse-papiers (`fileExplorer.clipboard.copiedPath`)

Une clé : la ligne de la notification d'information après ⌘⌥C. Le chemin s'affiche en dessous, sur sa propre ligne en
police à chasse fixe : ce n'est donc PAS un paramètre dans la phrase, qui se termine par deux-points et doit tenir sans
lui.

- **"Copied the path, it's now on your clipboard:" → `Chemin copié, il est maintenant dans le presse-papiers :`** ·
  reprend `path → chemin` et `clipboard → presse-papiers` de `terms.json` (macOS Finder) · high. Espace ASCII normale
  avant les deux-points, conformément à style.md § Punctuation spacing ; jamais U+202F. Pas de possessif ("votre
  presse-papiers") : macOS emploie l'article défini.

## Le renommage de la file d'attente des opérations (`queue.windowTitle`, `queue.heading`, `queue.list.aria`, `queue.row.pauseAria`, `.resumeAria`, `.cancelAria`, `.selectAria`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`, `.backgroundedToast`)

English renamed the product noun: the window that was the **"Transfer queue"** is now the **"Operation queue"**. This is
a meaning change, not a copy tweak. The window lists deletes, trashes, renames, folder and file creations, and archive
edits, not only transfers, and "transfer" already means copy-or-move one level down in Cmdr (the transfer progress
dialog, the transfer driver). French had to widen the same way; a hash restamp would have left the catalog saying
"transferts" for a window that is not about transfers.

- **operation (a running or queued file job: copy, move, delete, trash, rename, create, archive edit) → `opération`
  (feminine)** · macOS Finder/AppKit Tier-1, which uses "opération" for exactly this category, including the
  same-concept sentence `LocalizableMerged.json` NE82 ("Impossible de terminer l''opération pour le moment car une autre
  opération, telle que le déplacement ou la copie d''un élément…"), plus "Une opération est toujours en cours",
  "Terminez les opérations et réessayer"; MS terminology FRA (`operation` → "opération", four entries, unanimous outside
  the medical sense); already settled in `terms.json` as the Operation log''s head noun and in the
  `File operations → Opérations sur les fichiers` settings section · high.
- **operation queue (the window, the View menu item, the command palette entry) → `File d''attente des opérations`** ·
  composed from the settled `queue → file d''attente` (Double Commander "File d''attente" / "Ajouter à la file
  d''attente"; MS terminology FRA `queue` → "file d''attente", four of five entries) plus the `opération` head noun
  above. The MS pile attests the exact `file d''attente des <plural noun>` shape ("file d''attente des appels", "file
  d''attente des éléments de travail") · high. Used verbatim for `queue.windowTitle`, `commands.queueShow.label`, and
  inside every string that names the window (`transferProgress.queueAria`, `.queueTooltip`, `.queuedToast`,
  `.backgroundedToast`), as the English `@key` descriptions require.
- **The View-menu PAIR is preserved**: `File d''attente des opérations` (running now) next to
  `Historique des opérations` (already ran). Both hang off the same head noun `opérations`, so French carries the same
  present-vs-past pairing English does. No divergence from the Operation log''s word.
- **operations (the queue''s heading + the list''s screen-reader label) → `Opérations`** · the bare plural of the head
  noun, staying a noun rather than a verb as the `@key` description asks · high.
- **"this operation" (per-row screen-reader labels) → `cette opération`** · feminine demonstrative, agreeing with
  `opération`: "Mettre cette opération en pause", "Reprendre cette opération", "Annuler cette opération", "Sélectionner
  cette opération". Reuses the settled `pause → mettre en pause`, `resume → reprendre`, `cancel → annuler`,
  `select → sélectionner` · high.

Phrasing notes for this pass:

- **The rename flips two toast pronouns to feminine.** `queuedToast` and `backgroundedToast` referred to the waiting or
  backgrounded job with a masculine clitic ("celui-ci", "il", "Retrouvez-le"), which agreed with the old implicit
  masculine "transfert". The job is now an `opération` (feminine), and in `queuedToast` the noun is literally on screen
  next to the pronoun (`{countText}` renders "1 opération" / "3 opérations"), so a masculine "celui-ci" would have read
  as a visible agreement break. Both toasts now use the feminine: "…devant celle-ci, elle attend donc son tour.
  Retrouvez-la dans la file d''attente des opérations." and "Toujours en cours en arrière-plan. Retrouvez-la dans la
  file d''attente des opérations." Keeping the two parallel matters: they fire from the same dialog moments apart.
- `queueTooltip` keeps "Garder ce transfert en cours en arrière-plan…": that tooltip lives on the transfer progress
  dialog and genuinely describes a transfer, so only the window''s NAME changed there. Same for the `queueAria` verb
  ("Envoyer dans…").
- `commands.queueShow.label` dropped its "Afficher" ("Show"): the English label is now the bare window name, and the
  `@key` requires the command, the View menu item, and the window title to read identically.
- FR CLDR `one`/`many`/`other` on `queuedToastCount`, `many` written identical to `other` (plain integers never select
  `many`, but the parity/plural checks want the branch), matching every other plural in the `fr` set. Verified with
  `intl-messageformat` under locale `fr` for 0/1/2/5/1 000 000: French counts 0 as `one`, and "0 opération" is correct.
- No `:` `;` `!` `?` `%` in any of the 14 values, so the catalog''s ASCII-space-before-punctuation rule doesn''t arise.
  Every apostrophe is ASCII (U+0027) and doubled, since all 14 are ICU keys.
- **Length**: "File d''attente des opérations" is exactly as long as the "File d''attente des transferts" it replaces
  (29 characters), so the rename adds no new overflow risk to the window title or the menu item. It was already long for
  a macOS window title, and still is.

## La pastille de progression et l'avis « N'a pas pu se terminer » (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

Two new surfaces: a ~80 px progress chip in the main window's top-right corner (a button that opens the queue window,
with a hover tooltip and a stopped-before-finishing state), and a persistent failure toast plus a Dismiss button on the
failed queue row. The window's name and the `opération` head noun come from the rename pass above; nothing here
re-derives them. ICU values, so single apostrophes are doubled below to match this doc's convention.

- **dismiss (stop showing a row / a notice, without undoing, retrying, or deleting anything) → `Ignorer`; "Dismiss all"
  → `Tout ignorer`; the per-row screen-reader label → `Ignorer cette opération`** · the catalog's own settled Dismiss
  term, already shipping on six surfaces (`crashReporter.dialog.dismiss`, `downloads.empty.dismiss`,
  `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`, `errorReporter.bundleSavedToast.dismiss`,
  `fileOperations.mkdir.timeoutDismiss`), and recorded in style.md § Brand and do-not-translate ("Ignorer" fits a
  non-destructive dismiss better than "Fermer"). Microsoft terminology FRA confirms it independently for exactly this
  sense: `dismiss` "to turn off a system notification" has two FRA renderings, `ignorer` and `masquer` · high.
  - `Tout ignorer` follows the catalog-wide all-variant pattern ("Tout éjecter", "Tout écraser", "Tout reprendre") and
    stays parallel to its toolbar neighbours `Tout mettre en pause` / `Tout reprendre`.
  - **Known collision, accepted**: `fileOperations.transferProgress.conflictSkipAll` (Skip all) is also `Tout ignorer`.
    The two never share a surface (the conflict dialog has no dismiss; the queue toolbar has no skip), and diverging
    would cost the catalog its single settled Dismiss word. Don't "fix" it by renaming one of them.
  - `Ignorer cette opération` matches the recorded per-row aria FAMILY shape from the rename pass ("Mettre cette
    opération en pause", "Reprendre cette opération", "Annuler cette opération", "Sélectionner cette opération"), not
    the adjacent line.
- **"Couldn''t finish <action>" (the failure toast headline) → `<L''action> n''a pas pu se terminer`** · built from the
  settled `queue.row.status` failed arm `N''a pas pu se terminer` plus the `queue.row.label` verbal nouns, so the toast
  and the row can't describe the same stop with two words · high. The nine arms make the action noun the SUBJECT, which
  keeps the sentence impersonal (no agent, no gendered participle on the user) and lets each arm carry its own article:
  `La copie` / `Le déplacement` / `La suppression` / `Le placement dans la corbeille` / `Le renommage` /
  `La création du dossier` / `La création du fichier` / `La modification de l''archive`, with the `other` arm the bare
  `N''a pas pu se terminer`. No "erreur", no "échec", no "a échoué", per style.md.
- **"N operations couldn''t finish" (the summary toast + the chip's failed state) →
  `{countText} opération n''a pas pu se terminer` / `{countText} opérations n''ont pas pu se terminer`** · same house
  wording, with the verb agreeing in the plural branches. FR CLDR `one`/`many`/`other`, `many` identical to `other` as
  everywhere else in this set · high.
- **"Open the operation queue to see why." → `Ouvrez la file d''attente des opérations pour savoir pourquoi.`** · `vous`
  imperative, because this is a prose sentence, not a button (style.md reserves the infinitive for labels), and the
  settled window name verbatim · high for the terms, tentative for the purpose clause. **Why not "pour en connaître la
  raison"**: the same string serves counts 1 and N, and French would want "les raisons" for N; `savoir pourquoi` is
  number-neutral, shorter, and matches the friendly register. Don't re-derive this.
- **"Show in operation queue" (the failure toast's button) → `Afficher dans la file d''attente des opérations`** ·
  `Afficher dans …` is the macOS Tier-1 shape for this ("Finder/Reveal" → "Finder/Afficher dans le Finder"), and the
  window name stays byte-identical to `queue.windowTitle` as the `@key` requires · high. See the overflow note below.
- **"percent", spelled out for screen readers → `pour cent`** (two words) · the standard French reading of `%`, so a
  French screen reader says the same thing it would for the symbol, and the aria label stays free of the
  space-before-`%` rule · high.
- **The chip tooltip's line shape → `{label} de {N} éléments vers {destination} · {P} % · {detail}`** · this exact
  progress-line shape is Tier-1 and Tier-3 attested: macOS Finder `PW5_V2` "Préparation de la copie de ^0 éléments" and
  the AirDrop panel's "Copie de « quelque chose » vers « un endroit »"; GNOME Nautilus "Copying %'d files to “%s”" →
  "Copie de %'d fichiers vers « %s »", "Moving %'d files to “%s”" → "Déplacement de %'d fichiers vers « %s »" · high.
  The verbal-noun label needs the linking `de` (a bare "Copie 3 éléments" would read as an imperative), and `vers` is
  the settled destination preposition. `item → élément` per `terms.json`; macOS uses "éléments" in this very string, so
  files-and-folders is covered.
  - The destination is left BARE, not in guillemets, even though both piles quote it: English doesn't quote, and the
    tooltip is one tight line.
  - `{percentText} %` carries the settled ASCII space before `%` (style.md § Punctuation spacing), Tier-1 confirmed by
    Finder's progress window `PW13.1` "^0 %". Never U+202F.

Phrasing notes for this pass:

- **Every optional clause carries its own leading space inside its branch** (` de {countText} éléments`,
  ` vers {destination}`, ` · {detail}`), and the `=0 {}` / `other {}` arms stay empty, exactly as English does it. That
  is what makes an absent part vanish without leaving a double space or a stranded `·`. All four combinations (count 0/3
  × destination absent/present) were rendered with `intl-messageformat` under locale `fr`: "Copie · 42 %", "Copie vers
  Backup · 42 %", "Copie de 3 éléments · 42 %", "Copie de 3 éléments vers Backup · 42 %".
- The tooltip's `{label}` is ALWAYS the action word (`OperationChip.svelte` passes `verb`), never "En pause", so the
  `de` linkage is safe. The aria label's `{label}` can be "En pause" or a verb, and both read correctly there ("En
  pause, 42 pour cent.").
- `{detail}` arrives pre-rendered from this catalog's own `fileOperations.transferProgress.etaRemaining` ("{duration}
  restant") or the `En pause` status word, so it needs no translation here; the slot is neutral and takes any length.
  The pile agrees on `restant` for the remaining sense (Xfce Thunar "%lu minutes restantes", GNOME Nautilus "%s
  restantes", macOS Finder `PW2` "Estimation du temps restant…").
- FR CLDR `one`/`many`/`other` throughout; `many` written identical to `other`. Rendered for counts 0/1/2/5/1 000 000:
  French counts 0 as `one`, and "0 opération n''a pas pu se terminer" is correct French.
- Register is `vous` ("Ouvrez"); every apostrophe is ASCII (U+0027) and doubled; no U+2019 leaked in from the English
  source.
- **Overflow watch** (French runs long, and both surfaces are tight):
  - `queue.failureToast.action` is 46 characters against English's 23, on a button in a ~360 px toast. It can't be
    shortened without breaking the `@key`'s "identical to the window title" requirement.
  - `queue.failureToast.title`'s `trash` arm, "Le placement dans la corbeille n''a pas pu se terminer" (52 chars vs 31),
    is the longest headline. Nautilus's shorter "Mise à la corbeille" was weighed and set aside: the brief binds the
    arms to the `queue.row.label` nouns so the toast and the row agree, and that row says "Placement dans la corbeille".
  - The chip itself only ever shows `queue.row.label` / `queue.row.status`, which this pass didn't touch, so the ~80 px
    chip carries no new risk.

## Le dialogue de conflit autonome (`fileOperations.operationConflict.context`, `.pausedNote`)

The context line under the dialog title `Le fichier existe déjà`, naming which background operation is asking, plus the
quiet note under the buttons. Both are ICU, so apostrophes are doubled. The verbal nouns come from `queue.row.label` and
the destination preposition from `queue.chip.tooltip`; nothing here re-derives them.

- **A bare verbal noun needs `en cours` to stand as a line; one with a complement doesn''t.** `Copie vers Backup` and
  `Modification de l''archive photos.zip` read as running text under the title, but a lone `Copie` reads as a row label,
  so the no-destination arms take the catalog''s progress qualifier: `Copie en cours` / `Déplacement en cours` (the
  shape of `transferProgress.titleActive`, "Copie en cours..."). English needs no such split, because "-ing" is
  progressive on its own; don''t "fix" the asymmetry between the two branches · high.
- **"Copying/Moving to {destination}" → `Copie vers {destination}` / `Déplacement vers {destination}`** · `vers` is the
  settled destination preposition (`queue.chip.tooltip`), Tier-1 attested in exactly this progress-line shape by macOS
  Finder ("Copie de « ^1 » vers « ^2 »", "Préparation de la copie vers « ^0 »", "Déplacement de ^0 éléments vers « ^2
  »") and by GNOME Nautilus ("Copie de « %s » vers « %s »") · high. The destination stays BARE, no guillemets, as the
  corner-chip pass settled.
- **"Editing {destination}" (the archive itself) → `Modification de l''archive {destination}`, NOT
  `Modification de {destination}`** · `de` before an uncontrolled name would need elision on a vowel-initial one
  ("d''Archives 2026"), which the catalog can''t do; naming the settled noun `archive` moves the elision onto
  `l''archive` where it is fixed. Same discipline as the archive-password pass''s gender rule · high. The no-destination
  arm keeps the sibling verbatim, `Modification de l''archive`: French already reads that as generic, so English''s "an
  archive" needs no indefinite here.
- **"Working (in {destination})" → `Opération en cours dans {destination}` / `Opération en cours`** · the settled head
  noun `opération` (operation-queue rename pass), already in-catalog at `commands.queueShow.description` ("chaque
  opération en cours et en attente"); `dans` for the locative, since a catch-all operation happens IN a folder, not
  toward it · high. `queue.row.label`''s bare `En cours` was set aside: as a line under the dialog title it names
  nothing.
- **"until you answer" → `tant que vous n''avez pas répondu`** · macOS Tier-1 renders "until" with the
  `tant que … ne … pas` shape ("Ne déconnectez pas l''appareil tant que l''effacement n''est pas terminé.", "Vos
  modifications ne seront pas enregistrées tant que le problème ne sera pas résolu."), not "jusqu''à ce que" · high.
  Full line: `Tout le reste est en pause tant que vous n''avez pas répondu.`, on the settled `paused → en pause`
  (`queue.row.status`).

Phrasing notes for this pass:

- All 10 branch combinations were rendered with `intl-messageformat` under locale `fr` (both `hasDestination` values ×
  the five `type` paths, with destinations "Backup", "photos.zip", "Archives 2026", "Été"). Every apostrophe is ASCII
  (U+0027) and doubled; no `:` `;` `!` `?` `%`, so the spacing rule doesn''t arise.

## Le bouton du dialogue de progression quand la file est vide (`fileOperations.transferProgress.background`, `.backgroundAria`)

The progress dialog's primary action is ONE button with two wordings: `queue` ("File d''attente") when the operation
queue already holds work, and `background` when it's empty. The `background` arm is a COMMAND (English uses "Background"
as a verb), so French can't take the bare noun; it also can't take a full infinitive phrase, because the button row is
already at its width budget. ICU values; neither value contains an apostrophe, so no doubling arose.

- **background (the empty-queue arm of the progress dialog''s primary button, "put this transfer in the background") →
  `En arrière-plan`** · Microsoft terminology FRA renders `background` in exactly this sense (adjective, "operating
  without interaction with the user while the user is working on another task", id 18761) as "en arrière-plan", and
  `background task` as "tâche en arrière-plan"; Double Commander agrees ("in the &background" → "en arrière-plan", "Work
  in background" → "Travailler en arrière-plan"), as does Total Commander · high. The label is the ELLIPTICAL form of
  the settled phrase, mirroring English''s own elliptical "Background": the implied verb is spelled out one paragraph up
  in this very dialog, in `transferProgress.stallUnknown` ("Annulez-le ou laissez-le **continuer en arrière-plan**."),
  which is why the ellipsis reads on this surface.
  - **Why not the bare noun `Arrière-plan`**: in the pile the bare noun is the BACKDROP sense almost everywhere (macOS
    Finder "Arrière-plan :" for a folder''s picture, Nautilus "Définir comme arrière-plan…", Dolphin and Double
    Commander colour settings), so a button reading "Arrière-plan" would name a wallpaper. The preposition is what
    carries the process sense, and MS FRA attaches it for exactly that reason.
  - **Why not the full infinitive `Continuer / Passer en arrière-plan`** (the shape style.md prescribes for buttons, and
    the shape MS FRA uses for the PowerPoint control "Play in Background" → "exécuter en arrière-plan", id 1761393):
    **width, a hard constraint here.** See the overflow note below. `Continuer` alone also sits badly next to its
    neighbour: the Pause button''s other state reads "Reprendre", and two adjacent keep-going verbs invite a misread.
  - **Known shape resemblance, accepted**: `En <noun>` is this catalog''s STATUS shape (`queue.row.status` → "En
    attente", "En pause"), and the pile bears that out (macOS "En attente…", "En attente du téléchargement"; Double
    Commander "En pause", "En cours", "En avant-plan" for a colour setting) — French UI keeps bare prepositional phrases
    for states and positions, and puts actions in the infinitive. It is accepted here because the SURFACE disambiguates:
    this is a footer button with a list icon, in a row with "Pause", "Annuler", and "Annuler et restaurer", and buttons
    are read as actions. There is no actual collision to trip over either: no status string in the `fr` catalog reads
    "En arrière-plan" (`queue.row.status` never uses it). Don''t "fix" this by expanding the label without re-checking
    the width.
- **"Keep this running in the background" (the same button''s screen-reader name) →
  `Garder ce transfert en cours en arrière-plan`** · the first clause of the shipped `transferProgress.queueTooltip`
  verbatim, which is this locale''s settled rendering of the same English sentence, so the button''s two states, its
  tooltip, and its accessible name all describe the action with one phrase and nothing is invented · high.
  - **WCAG 2.5.3 (Label in Name)**: the accessible name contains the visible label, so voice control understands
    "cliquez sur En arrière-plan". Containment is case-insensitive at the END of the string ("… en arrière-plan"),
    exactly the bar English keeps with "Background" ⊂ "Keep this running in the background". Only the label''s capital
    `E` differs. Don''t reword the aria''s tail: dropping or splitting "en arrière-plan" breaks the containment.
- `queueTooltip` is unchanged: it is shared by both button states and describes a transfer, which is correct either way.

Phrasing notes for this pass:

- **Width is the binding constraint on this label, not a warning.** The dialog is a fixed 580 px (its readout''s
  fixed-width columns are why: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`), and in the rollback-capable
  running state the `.button-row` carries four buttons: "Pause" + icon, this one + icon, "Annuler", and "Annuler et
  restaurer" (20 chars). Each `Button` adds 40 px of padding and the row adds 12 px gaps, which puts the row within a
  few pixels of the content width when this button reads the 14-char "File d''attente". "En arrière-plan" (15) costs
  about 7 px more, so it carries no new risk; "Passer en arrière-plan" (22) and "Continuer en arrière-plan" (25) push
  the row past the budget, and `.button-row` has `flex-wrap: wrap`, so it would not clip, it would WRAP the footer. The
  English `@key` says it directly: "Short control label; must fit the same button as \"Queue\"".
- No `: ; ! ? %` in either value, so the ASCII-space-before-punctuation rule doesn''t arise; no apostrophe, so no ICU
  doubling; no U+2019 or U+202F leaked in.

## La barrière de sortie (`main.quit.*`)

The modal Cmdr raises when the user quits (⌘Q, the menu, or closing the main window) while a copy, move, delete, trash,
or archive edit is still running: a title asking whether to go ahead, a reassuring body, a short list of what's running,
a live countdown from 15, and two buttons. ICU values, so single apostrophes are doubled below to match this doc's
convention. The head noun `opération` and the `en cours` status word come from the operation-queue rename pass above;
nothing here re-derives them.

- **"Quit while … running?" (the dialog title) → `Quitter alors qu''une opération est en cours ?` /
  `Quitter alors que {countText} opérations sont en cours ?`** · macOS Tier-1 states this exact situation in
  `Finder/LocalizableMerged.json` A17 ("The Finder can''t quit because some operations are still in progress." → "Vous
  ne pouvez quitter le Finder parce que certaines opérations sont toujours en cours."), which settles both the head noun
  and the `être en cours` predicate for a running file operation; `quitter` for an app is AppKit Tier-1 (`Document.json`
  "Quit" → "Quitter", "Quit Anyway" → "Quitter quand même") and already in-catalog at `commands.appQuit.label` ("Quitter
  Cmdr") · high.
  - **The elliptical infinitive question is this catalog''s settled title shape**, not a calque of English''s ellipsis:
    `ai.local.deleteDialogTitle` ("Supprimer le modèle d''IA ?"), `fileExplorer.extensionChange.title` ("Changer
    l''extension du fichier ?"), `indexing.firstConnect.title` ("Indexer {name} ?"). Don''t expand it to "Voulez-vous
    vraiment quitter…".
  - `alors que` over `pendant que`: the dialog asks about quitting DESPITE running work, and `alors que` carries that
    concessive edge where `pendant que` is purely temporal. (Finder N144 renders a different "while" with `quand`, but
    that one is plain simultaneity.)
- **"stays done" (nothing already completed is undone) → `Tout ce qui est déjà terminé le reste.`** · the neutral
  pronominal `le reste` is the only short form that stays true for EVERY operation type: "reste en place" or "est
  conservé" would be wrong for a delete or a trash, where "done" means the files are gone · high for the terms,
  tentative for the phrasing. Known momentary garden path: a reader can start to parse `le reste` as the noun "the
  remainder", but that reading leaves the sentence verbless and self-corrects. Don''t "fix" it with a keeping verb.
- **"anything still being written" → `Ce qui est encore en cours d''écriture`** · **the body must stay number-neutral**:
  one operation writes several files at once and several operations can run at once, so a singular
  (`Le seul élément encore en cours d''écriture`) states something false, and the free relative `Ce qui` scopes it
  without a numeral · high. The English `@key` defines the state as "currently being written", so French carries the
  concrete writing sense; `en cours` is the catalog''s settled running word (`queue.row.status`).
- **"what it leaves half-written" → `tout fichier à moitié écrit`** · `fichier à moitié écrit` is already shipped
  verbatim in this catalog at `settings.advanced.showStagingTempFiles.description` ("Un plantage ne peut donc pas
  laisser un fichier à moitié écrit sous un vrai nom."), which describes the same temp-file mechanism · high. `tout` +
  singular is French''s number-neutral form, so the settled phrase survives where the definite `le fichier …` could not;
  it also avoids a second `il` (the first binds to `ce qui`) and an echo of `rester` from the opening sentence. "clears
  away" → `supprime`, per `terms.json`'s `delete → supprimer` (and NOT `efface`, which style.md reserves for the
  erase/wipe sense).
- **logout (the OS session, in the countdown''s reason clause) → `une fermeture de session`** · macOS Tier-1
  (`AppKit/Menus.json` "Log Out" → "Fermer la session", the item the user sees in the Apple menu); Microsoft terminology
  FRA agrees (`log off` → "fermer une session", FRA) · high. restart → `un redémarrage` · `AppKit/Menus.json` "Restart"
  → "Redémarrer" · high.
  - **Why not `déconnexion`**: this catalog uses `se déconnecter` for leaving a SERVER (macOS Finder "Disconnect" → "Se
    déconnecter"), so "un redémarrage ou une déconnexion" inside Cmdr could read as dropping an SMB share.
    `fermeture de session` is unambiguous and is what the user''s Apple menu says. `shortcuts.system.loggingOut` says
    `la fermeture de session` too, so the pair agrees.
- **"Quitting in N seconds" (the live countdown) → `Cmdr quitte dans {secondsText} seconde(s)`** · intransitive
  `quitter` with the app as subject is AppKit Tier-1 ("%@ a quitté inopinément pendant la réouverture des fenêtres"),
  and it keeps the whole dialog on ONE verb root (title "Quitter alors que…", button "Quitter maintenant", aria "avant
  que Cmdr quitte…") · high. **Why not `Fermeture de Cmdr dans …` or `Cmdr se ferme dans …`** (both also Tier-1, Finder
  BN36 "Le Finder est sur le point de se fermer"): the `ferm-` root would collide with `fermeture de session` three
  words later, and it would split the dialog across two verbs. Naming Cmdr is load-bearing here: the English value
  carries the brand, and `desktop-i18n-dont-translate` flags a dropped one.
  - The reason clause is restructured from English''s "so a restart or logout never waits on Cmdr" to
    `pour ne jamais retarder un redémarrage ou une fermeture de session`: same meaning, with Cmdr as the already-named
    implied subject, so the brand isn''t repeated twice in one short line.
- **"Time until Cmdr quits on its own" (the countdown''s screen-reader label) →
  `Temps restant avant que Cmdr quitte de lui-même`** · `temps restant` is Finder Tier-1 (`PW2`, the copy window''s
  "Estimation du temps restant…"), and `de lui-même` is the standard French for "on its own" · high. **Not a WCAG 2.5.3
  pair**: the countdown region has no visible label of its own (the visible text is the sentence being announced), so
  there is nothing to contain; keep it short and naming what the number measures.
- **"Keep working" (the button that calls the quit off entirely) → `Continuer à travailler`** · a full infinitive per
  style.md''s button rule, and the object is what makes it safe · high for the shape, tentative for the exact verb.
  - **Why not `Annuler`**: this catalog uses `Annuler` for cancelling an OPERATION on the queue surfaces
    (`queue.row.cancel`, `queue.row.cancelAria` "Annuler cette opération", `queue.toolbar.cancelSelected`), and this
    dialog lists running operations directly above its buttons, so a bare "Annuler" would read as the exact opposite of
    what the button does.
  - **Why not a bare `Continuer`**: macOS uses "Continuer" as the GO-AHEAD button in confirmation alerts (Finder BN23
    "Cliquez sur Continuer pour éjecter le disque…"), so alone it would read as "continue quitting". The object
    `à travailler` flips it back and, unlike "Plus tard" or "Me le rappeler", carries no postponement: the countdown is
    deleted, not deferred (the English `@key` is explicit about this).
- **"Quit now" → `Quitter maintenant`** · `maintenant` keeps the load-bearing "now": the app quits either way when the
  countdown ends, and this button only skips the wait · high. Distinct from macOS''s "Quitter quand même" (Quit Anyway),
  which answers a "you shouldn''t" objection rather than a wait.
- **"Still running" (the heading over the list of operations) → `Toujours en cours`** · lifted from the Finder A17
  sentence above ("… sont toujours en cours"), so the heading and the rows under it speak the catalog''s one running
  word; the bare `En cours` (`queue.row.status`) drops the "still" that makes it a heading · high.

Phrasing notes for this pass:

- All branches were rendered with `intl-messageformat` under locale `fr`: the title for 0/1/2/15/1 000 000 and the
  countdown for 0/1/2/15. French counts 0 as `one`, so the countdown''s zero tick reads "0 seconde" (correct); the
  title''s `one` branch at 0 is unreachable, since the dialog only opens with work running. FR CLDR `one`/`many`/`other`
  with `many` written identical to `other`, as everywhere else in this set.
- Both plurals keep the agreeing verb INSIDE the branches (`est en cours` / `sont en cours`, `seconde` / `secondes`),
  per style.md § Plurals.
- The two title branches end in `?` with the settled ASCII space before it, never U+202F. No other `: ; ! %` occurs.
  Every apostrophe is ASCII (U+0027) and doubled: `qu''une`, `d''écriture`, `s''arrête`, `qu''il`.
- Neither "erreur" nor "échec" appears, per style.md; the body stays matter-of-fact rather than warning-shaped.

## Statistiques d'usage : pas « anonymes », mais « un identifiant aléatoire » (`settings.analytics.enabled.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`, `.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudonyme` / `pseudonymisé` — that
jargon is exactly what the copy avoids.

- **usage stats → `statistiques d'usage`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anonymes` adjective was cut. `settings.analytics.enabled.label` used the shorter `stats d'usage`; both keys now carry
  the same value because the English label and the onboarding title are one identical string · high
- **a random id → `un identifiant aléatoire`** · MS terminology (random → `aléatoire`; `identifiant` is the everyday
  entry, and the one Apple uses in "identifiant Apple") · high. ❌ Not `identificateur`, MS's other entry: that's the
  technical/computing sense.
- **tied to → `relié à`** · the catalog's own verb (`onboarding.stepBeta.emailNote` "n'est jamais reliée à vos
  statistiques d'usage") · high
- The list "noms de fichiers, chemins, termes de recherche ou prompts" drops the Oxford comma the old value carried over
  from English: French doesn't use it.

## Lignes de file en attente de réponse et la confirmation du retour en arrière (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `fileOperations.transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- **"Needs your answer" (pastille de statut dans la file) → `Réponse requise`** · macOS `fr` ("Authentification requise
  pour effectuer cette opération.", "Un mot de passe est requis pour désactiver le chiffrement.") · high. ❌ Jamais
  `Réponse attendue` ni rien en `attente` : `En attente` EST le statut "en file derrière une autre opération"
  (`queue.row.status`), et les deux doivent rester distinguables dans la même colonne étroite.
- **"prompt" (la question affichée sur laquelle l'opération est arrêtée) → `la question`** · aligné sur
  `operationConflict.pausedNote` ("tant que vous n'avez pas répondu") · high.
- **"this operation carries on" → `cette opération continuera`** · standard ; évite `reprendra`, réservé au sens
  `resume → reprendre` (sortir de pause) · high.
- **rollback → `Revenir en arrière`** : le titre est `Revenir en arrière sur cette opération ?` et le bouton destructeur
  reprend le même verbe, pour coller au bouton qui a ouvert le dialogue. Le choix de la famille (et pourquoi pas
  `restaurer`, que ce corps contredirait) : section suivante.
- **"Keep them" (la réponse sûre) → `Conserver les fichiers`** · macOS `fr` ("Conserver", "Tout conserver", "Conserver
  la copie partielle") · high. Le nom est explicité plutôt que `Les conserver` : le corps vient de nommer les fichiers
  REMPLACÉS, un pronom serait ambigu.
- **"written so far" → `écrits jusqu'à présent`** · reprend `written → écrit` du catalogue
  (`transferProgress.stallInFlight` "peut-être déjà partiellement écrit") · high. Accord au masculin pluriel avec le COD
  antéposé (`les fichiers que l'opération a écrits`).
- **"Stop, and …" (infobulle de rollback) → `Arrêter et …`** · macOS `fr` ("Arrêter la copie", "Arrêter le déplacement",
  "Arrêter l'effacement") · high. Distinct d'`Annuler`, ce que l'infobulle ne doit justement PAS évoquer.
- **foregroundBusyToast : nommer l'opération.** Le "this one" anglais n'a pas d'antécédent en français, donc la valeur
  l'explicite : « … puis affichez cette opération », en reprenant le bouton `Afficher` (`queue.row.foreground`) · high.
- Espace ASCII normale devant `?` et `:` (règle catalogue, style.md § Punctuation spacing) ; apostrophes ASCII doublées
  dans les valeurs ICU.

## La famille `rollback` : `revenir en arrière`, jamais `restaurer` (`fileOperations.transferProgress.rollback*`, `.conflictRollback`, `.titleRollingBack`, `operationLog.rollback.*`, `operationLog.outcome.rolledBack`, `operationLog.dialog.rollBack`, `commands.logOperationLog.description`, `settings.operationLog.intro`)

Arbitrage sur TOUTE la famille, pas clé par clé. Le corps de `rollbackConfirm` dit noir sur blanc que les fichiers
écrasés ne reviennent pas : le rollback SUPPRIME ce que l'opération a écrit, il ne REND rien. `restaurer` promettait
donc l'inverse de ce que l'action fait.

- **rollback → `retour en arrière` (nom) / `revenir en arrière` (verbe)** · `tentative`. Les trois familles candidates
  et pourquoi les deux autres sont écartées :
  - ❌ `restaur-` : c'est `Restore` en français, y compris dans le domaine gestionnaire de fichiers (Nautilus `fr` «
    Ann_uler la restauration depuis la corbeille », Time Machine « Restaurer »), et le catalogue s'en sert déjà pour la
    vraie restauration (`askCmdr.renameUndo.*`, où les anciens noms sont bel et bien rendus). Microsoft `fr` donne
    `roll back → restaurer` / `rollback → restauration`, mais c'est le sens TRANSACTION de base de données, où l'état
    antérieur revient vraiment : le piège de sens n° 4 de `docs/i18n/reference-pile/how-to-mine.md`.
  - ❌ `annul-` : c'est le mot de l'undo en français (macOS `fr` `Undo` → « Annuler » ; Nautilus « Annuler la copie » ;
    Double Commander « Annuler (en arrière) » ; Microsoft `undo` → « annulation »), MAIS `Annuler` est déjà le bouton
    Cancel (`transferProgress.conflictCancel`) et `Annulé` déjà le statut `operationLog.status.canceled`. Or le journal
    doit garder « vous avez annulé avant » et « vous êtes revenu en arrière après » distinguables d'un coup d'œil.
  - ✅ `retour en arrière` : libre des deux collisions, se décline sur les six pastilles comme sur les boutons, et ne
    promet qu'une direction, jamais une récupération. La forme AVEC `en` est délibérée : `retour arrière` tout court est
    le nom de la touche Retour arrière et, dans macOS `fr`, le retour rapide d'un lecteur média (« retour arrière de 15
    secondes »).
- Le bouton fait 18 caractères, sous les 20 de `Annuler et restaurer` que le budget de largeur de `.button-row` avait
  mesurés (voir la section sur le libellé du bouton file d'attente vide), donc la rangée ne bouge pas.
- Le bouton seul ne dit pas que des fichiers sont supprimés : c'est voulu, l'anglais `Rollback` non plus. L'infobulle («
  Arrêter et supprimer tous les fichiers écrits jusqu'à présent ») et la confirmation obligatoire portent
  l'avertissement, et le rollback demande TOUJOURS confirmation.
- Les six pastilles : `Retour en arrière possible` / `… impossible` / `… en cours` / `… effectué` / `… partiel`, et
  `operationLog.outcome.rolledBack` reprend `… effectué` (l'anglais utilise la même chaîne aux deux endroits).
- `settings.operationLog.intro` disait « annuler des actions » : aligné sur `revenir en arrière`, sinon l'intro et les
  pastilles du journal juste en dessous ne parlent pas de la même chose.
- Inchangés parce que déjà exacts : `rollbackConfirm.body`, `rollbackConfirm.keep`, `transferProgress.rollbackTooltip`.

## Renommage en chaîne : « et N autres » se rend par `ainsi que …` (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

Le toast grandissant qui compte les renommages non appliqués. Il prolonge la phrase du frère `chainKeptOriginalName` («
{reason}. « {name} » a gardé son nom. »), donc même voix, mêmes guillemets, même « a gardé son nom ».

- **"and so did N other files" → `ainsi que {n} autres fichiers`** · macOS Finder Tier 1 pour la forme « X et N autres
  éléments » (`LocalizableMerged.json` : « Envoi de « ^1 » et de ^0 autres éléments. », « … en gardant les éléments les
  plus récents tels que « ^1 » et ^0 autres éléments. ») ; `ainsi que` attesté dans KDE Dolphin `fr` · high. Le « so did
  » anglais n'a pas d'équivalent direct : `ainsi que` porte le parallélisme sans allonger la phrase (le catalogue `fr`
  dérive déjà long). `fichier` et non `élément` parce que l'anglais dit explicitement « file ».
- **`ainsi que` passe DANS les branches du pluriel, pas devant.** L'élision l'impose : la branche `one` doit lire
  `ainsi qu''un autre fichier`, les autres `ainsi que {othersText} autres fichiers`. Même discipline que la règle
  style.md § Plurals sur les clauses qui doivent s'accorder ; le contrôle de parité ne compare que l'ENSEMBLE des
  placeholders, donc c'est sans risque.
- Branches CLDR `fr` : `one` / `many` / `other`, `many` identique à `other` (les entiers simples ne sélectionnent jamais
  `many`, mais le contrôle de parité veut la branche).
- Valeur :
  `{reason}. « {name} » a gardé son nom, {others, plural, one {ainsi qu''un autre fichier} many {ainsi que {othersText} autres fichiers} other {ainsi que {othersText} autres fichiers}}.`
- `{reason}` arrive sans point final et hors du contrôle de Cmdr : il reste en tête de phrase, suivi du point, comme
  dans le frère. `{name}` reste entre guillemets « » avec espaces ASCII, dans une position neutre (aucun accord ne
  dépend de lui).

## Renommage non confirmé : le volume ne répond pas (`fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

Paire sœur du couple `chainKeptOriginalName*` juste au-dessus, mais de SENS OPPOSÉ : là où `chainKept*` affirme que le
fichier a gardé son nom, `unconfirmed*` dit qu'on n'en sait rien et que le renommage est peut-être passé. La valeur ne
doit jamais laisser entendre que le nom est resté inchangé.

- **"Couldn't confirm the rename of X" → `Impossible de confirmer le renommage de « X »`** · le catalogue tient déjà
  deux toasts du même moule pour la même situation (volume qui ne répond pas à temps) :
  `fileOperations.mkdir.timeoutMessage` (« Impossible de confirmer la création du dossier. Le volume est peut-être lent,
  donc le dossier a peut-être quand même été créé. ») et `fileExplorer.pane.trashUnconfirmedToast`. Même moule, même
  ordre, même « quand même » pour le `anyway` anglais · high. `Impossible de …` est aussi la forme macOS `fr` pour un
  `Couldn't` (« Impossible de créer le dossier. », « Impossible de copier « ^0 » … ») et ne tombe sous aucune des
  interdictions de style.md (ni `erreur`, ni `échec`, ni `bloqué`).
- **Nommer le fichier dans la 2e phrase, ne pas pronominaliser.** ❌ Jamais « donc il a peut-être quand même abouti » :
  l'antécédent masculin le plus proche est `le volume`, et le toast dirait alors le contraire de ce qu'il veut dire. Les
  deux toasts précédents appliquent déjà cette discipline (« donc LE DOSSIER a peut-être quand même été créé »). D'où «
  donc le fichier a peut-être quand même été renommé » / « donc les fichiers ont peut-être quand même été renommés ». Le
  verbe `renommer` au participe dit littéralement l'inverse de `a gardé son nom` du frère, ce qui est exactement la
  distinction à préserver. L'anglais nomme l'ACTION (« the rename may still have gone through »), le `fr` nomme l'OBJET
  (« le fichier … renommé ») : c'est le moule `mkdir` (« the folder may still have been created » → « le dossier a
  peut-être quand même été créé »), et ça évite de répéter `renommage` deux phrases de suite. Ne pas « corriger » vers
  `donc le renommage a peut-être quand même abouti`.
- **"the rename of X and N other files" → `les renommages de « X » et de {n} autres fichiers`** · macOS Finder Tier 1
  pour ce moule exact, nom verbal + complément partagé (`LocalizableMerged.json` : « Envoi de « ^1 » et de ^0 autres
  éléments. ») · high. Noter la divergence VOULUE avec le `ainsi que` de `chainKeptOriginalNameAndOthers` : là-bas les
  autres fichiers sont un second SUJET (« « X » a gardé son nom, ainsi qu'un autre fichier »), ici ce sont des
  compléments de `le renommage de`, donc la préposition `de` doit se répéter et `ainsi que` alourdirait sans rien
  apporter. Même famille de voix, slot grammatical différent.
- Le pluriel `les renommages` est porté par la clé entière (toutes les branches comptent au moins deux renommages), et
  `et` reste DEVANT le `{others, plural, …}`, seul `d''un` / `de {othersText}` entre dans les branches : l'élision de
  `d'un` l'impose. Branches CLDR `fr` `one` / `many` / `other`, `many` identique à `other`.
- **Le doublement de `peut-être` est voulu**, pas une maladresse :
  `Le volume est peut-être lent, donc le fichier a peut-être quand même été renommé` calque mot pour mot
  `mkdir.timeoutMessage`. Les deux `peut-être` ne portent pas sur la même chose (l'un sur la lenteur du volume, l'autre
  sur l'issue du renommage), et c'est justement ce que le toast doit dire : on ne sait ni pourquoi ça traîne, ni si
  c'est passé.
- **"That filename can't be used" → `Le nom du fichier ne peut pas être utilisé`** (et `… du dossier …`) · macOS Finder
  `fr` (« L'extension « ^0 » est réservée au système et ne peut pas être utilisée. ») · high. Le moule
  `Le nom du {fichier,dossier} ne peut pas …` est repris tel quel des trois sœurs `fileOperations.validation.empty` /
  `.disallowedChars` / `.nameTooLong`, plutôt que le démonstratif `Ce nom de fichier …` qui casserait la colonne. Sans
  point final : la valeur s'insère aussi dans `{reason}` de `chainKeptOriginalName`, ce qui donne « Le nom du fichier ne
  peut pas être utilisé. « rapport final.pdf » a gardé son nom. »
- Guillemets « » à espaces ASCII autour de `{name}`, qui reste dans une position neutre (aucun accord n'en dépend).
  Seule apostrophe des trois valeurs : `d''un`, ASCII et doublée.

## Opérations suggérées : la fenêtre de ce que propose Ask Cmdr (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (l'ensemble d'opérations proposé par l'agent) → `opérations` ; le titre devient `Opérations suggérées` · terme
  maison ("File operations" → "Opérations sur les fichiers") · high
- approve → `Approuver` · MS ; retenu plutôt que le `Accepter` de macOS, car la variante avec décompte ("Approuver 3
  fichiers") autorise une action au lieu d'accepter un objet · high
- reject → `Refuser` · macOS Finder, paire Accepter/Refuser du panneau AirDrop (Tier 1) · high
- "This can't be undone" → `Cette opération est irréversible` · macOS Finder, mot pour mot (alerte de suppression
  immédiate) · high
- "Ask Cmdr's reason" → `Raison donnée par Ask Cmdr` · composé ; `motif` est déjà pris par pattern, d'où `raison` · high
- "Matched by a pattern" → `Correspond à un motif` · reprend `motif` de `terms.json` · high

## Dupliquer : la commande qui copie dans le même dossier (`commands.fileDuplicate.*`)

- **duplicate (commande qui copie la sélection dans son propre dossier) → `Dupliquer`** · macOS Finder `fr`, menu «
  Fichier > Dupliquer » (`N154`), plus « Dupliquer des éléments » et « Duplique des éléments dans leurs emplacements
  actuels » (vérifié sur macOS 26.6.1, `Finder.app/Contents/Resources/fr.lproj`, 2026-08-19) · high. Ne chevauche ni
  `Copier` (F5) ni `Déplacer` (F6).
- **« Make a copy of the selected files in the same folder » →
  `Créer une copie des fichiers sélectionnés dans le même dossier`** · infinitif, comme les descriptions voisines («
  Copier les fichiers sélectionnés… ») ; « le même dossier » = celui où les fichiers se trouvent déjà · high.

## Menus natifs : barre de menus, menus contextuels, titres de fenêtre (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Sources de tout ce lot : macOS 26.5.2 Finder (`Finder.app/Contents/Resources/fr.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) est le Tier 1 et tranche presque tout ; le côté anglais se lit dans `en_GB.lproj`, car
`Base.lproj` ne contient que des nibs compilés. Safari 26 (`MainMenu.strings`) donne le vocabulaire des onglets, la
terminologie Microsoft ce qu'Apple ne nomme pas. Famille RAW : **apostrophes simples**, un `''` s'afficherait en double
dans le menu.

- **Titres de la barre → `Fichier`, `Édition`, `Présentation`, `Aller`, `Fenêtre`, `Aide`, `Services`** · macOS Finder
  et Safari `fr` · high.
- **Menu Select (sélection de fichiers) → `Sélectionner`** · Nautilus/Thunar/Dolphin `fr` · high. Le Finder n'a pas
  d'équivalent ; l'infinitif s'accorde avec `Tout sélectionner` du même menu.
- **Quick Look → `Coup d'œil`** · macOS Finder (`TL14`) · high. Apple localise ce nom de fonction, d'où son absence de
  la liste ne-pas-traduire. Apostrophe ASCII simple dans `menu.*` (famille native), comme partout dans le catalogue
  (voir § Un mot anglais, un mot français).
- **Get Info → `Lire les informations`, Enclosing Folder → `Dossier parent`, Go > Home → `Départ`, Sort By →
  `Trier par`, Default → `Par défaut`, Other… → `Autre…`** · macOS Finder Tier 1 · high.
- **Window > Zoom → `Réduire/agrandir`** vs **sous-menu de zoom du texte → `Zoom`** · macOS Finder (`300667.title`) ·
  high. L'anglais dit deux fois « Zoom » ; le français distingue les deux, ce qui est un gain, pas une perte.
- **ascending / descending → `Croissant` / `Décroissant`** · Thunar + Dolphin `fr` · high.
- **changelog → `Journal des modifications`** · terminologie Microsoft · high. À distinguer d'Aide > `Nouveautés` : l'un
  nomme le document, l'autre la nouvelle.
- **word wrap → `Retour à la ligne automatique`** · terminologie Microsoft · high.
- **pin / unpin tab → `Épingler l'onglet` / `Désépingler l'onglet`** · Safari `fr` · high.
- **vue complète / vue abrégée (les deux modes d'affichage d'un panneau) → `Présentation complète` /
  `Présentation brève`** · `présentation` est le terme macOS pour un mode d'affichage (« Présentation par liste ») ·
  high.
- **Couleurs de tag du Finder → `Rouge, Orange, Jaune, Vert, Bleu, Violet, Gris`** · macOS Finder (`TG_COLOR_*`) · high.
- **Rangée de tags (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Tags`, `Ajouter « {color} »`,
  `Retirer « {color} »`** · macOS Finder (`TG5` = `Ajouter « ^0 »`, `N169.37` = `Tags`) · high. `TG6` dit
  `Supprimer « ^0 »`, mais on garde `Retirer` : retirer un tag ne supprime rien, et le catalogue le dit déjà ainsi
  (`commands.tagsToggleRed.description` « Ajoute ou retire le tag »), comme la règle `remove → Retirer`. Finder met des
  espaces insécables dans les guillemets ; le catalogue garde son espace ASCII.
- **busy (volume occupé) → `(occupé)`** · terminologie Microsoft · high.
- **Eject → `Éjecter`, Disconnect → `Se déconnecter`, Remove (d'une liste) → `Retirer`** · macOS Finder · high.
  `Retirer` évite que le retrait d'un favori se lise comme une suppression de fichiers.
- **`{name}` entre guillemets → `« {name} »`**, avec l'espace ASCII normale des deux côtés, conformément au réglage
  typographique du catalogue `fr`.
- **Identiques à l'anglais à dessein** (avec `sameAsSourceJustification`) : `menu.app.services`, `menu.sort.extension`,
  `menu.view.zoom`, `menu.tag.orange`, `menu.tag.rowLabel`, `menu.view.askCmdr`.

## Notification de repli sur le montage macOS (`fileExplorer.network.osMountFallback.*`)

Trois chaînes : le corps de la notification qui explique qu'un partage tourne sur la connexion SMB fournie par macOS,
son bouton de reprise, et l'infobulle de sa croix de fermeture.

- **native (au sens « fournie par le système ») → `native`** · terminologie Microsoft FRA (`native format` →
  `format natif`, `native integration` → `intégration native`) · high. « la connexion réseau SMB native de macOS ».
- **network connection → `connexion réseau`** · terminologie Microsoft FRA (`network connection` → `connexion réseau`) ·
  high.
- **Multiplicateurs de vitesse (`4x`, `100x`) → `4 fois`, `100 fois`** · le français écrit un multiplicateur en toutes
  lettres dans une phrase ; `4x` est une forme marketing anglaise. Nautilus `fr` atteste le comparatif nu (« La
  recherche sera plus lente ») · high.
- **slower → `plus lent(e)`** · Nautilus `fr` (« La recherche sera plus lente », « Afficher les fichiers cachés de
  manière ombragée (plus lent) ») · high. L'accord se fait avec l'objet nommé (`la connexion … plus lente`), jamais avec
  la personne.
- **Click the button below → `Cliquez sur le bouton ci-dessous`** · macOS Finder atteste `ci-dessous` (« Faites glisser
  vos tags favoris dans la zone ci-dessous ») · high.
- **Dismiss (infobulle de fermeture d'une notification) → `Ignorer`** · terminologie Microsoft FRA (`dismiss` →
  `ignorer`) et `lowDiskSpace.toast.closeTooltip` du catalogue `fr` · confirmed. Même choix que la liste crash-reporter
  du guide de style ; `Fermer` est réservé à la fermeture d'une fenêtre.
- **Try connecting directly (bouton) → `Essayer de se connecter directement`** · reprend le verbe de
  `fileExplorer.navigation.connectDirectly` (« Se connecter directement pour un accès plus rapide ») et de
  `fileOperations.transferDialog.smbNativeNote` (« Se connecter directement ») · high. L'infinitif est la forme des
  libellés d'action ; on garde le verbe plutôt que la tournure nominale « Tenter une connexion directe », le catalogue
  `fr` dérivant déjà vers le nom.
- **`You are connected` → `Vous y avez bien accès`** · le participe `connecté` genre l'utilisateur ; on nomme l'accès,
  pas la personne (règle du guide de style). `bien` porte la réassurance de l'anglais : le partage fonctionne, il est
  seulement plus lent.
- **`Couldn't directly connect` → `La connexion directe … n'a pas pu être établie`** · calque exactement
  `fileExplorer.pane.directConnectionUnexpectedToast` (« La connexion directe à {server} n'a pas pu être établie »), ce
  qui garde la famille cohérente et évite « erreur » / « échec ».
- **`for most connections` → `dans la plupart des cas`** · départ délibéré du littéral : `connexion` apparaît déjà trois
  fois dans la phrase, et `pour la plupart des connexions` en ajoutait une quatrième. `dans la plupart des cas` est la
  tournure française idiomatique et ne perd rien du sens.

## Refus de renommage et de création (`errors.mutation.*`, `errors.volume.*`)

Une phrase par clé, affichée sous le champ de nom du flux Renommer / Nouveau dossier / Nouveau fichier, ou dans une
notification brève. Famille RAW (`errors.*`) : apostrophes ASCII simples, `{path}` littéral, aucun ICU. `{path}` est un
insert non contrôlé, donc il reste toujours dans une position neutre (sujet ou complément entre guillemets), jamais
devant un participe ou un article qui devrait s'accorder.

- **Guillemets autour de `{path}` → `« {path} »`** · l'anglais met `"{path}"` ; le `fr` passe aux guillemets à espaces
  ASCII, comme le reste du catalogue (voir style.md § Guillemets, et la passe `unconfirmed*` ci-dessus) · high.
- **`There's nothing at "{path}" any more` → `« {path} » n'existe plus.`** · macOS Finder Tier 1, mot pour mot
  (`LocalizableMerged.json` PE131 : « « ^0 » n'existe plus. ») · high. Vaut pour `errors.mutation.notFound` et
  `errors.volume.notFound`, deux clés au même anglais.
- **top folder of a volume (racine d'un disque ou d'un partage) → `le dossier racine d'un volume`** · GNOME Nautilus («
  Toplevel files cannot be renamed » → « Impossible de renommer les fichiers racines »), Xfce Thunar (« The root folder
  has no parent » → « Le dossier racine n'a pas de parent »), Total Commander / Double Commander (« aller à la racine du
  lecteur ») · high. La phrase entière reprend le moule `Impossible de …` déjà retenu pour les `Can't X` : « Impossible
  de renommer ici le dossier racine d'un volume. »
- **`Check the folder's permissions in Finder` → `Vérifiez les autorisations du dossier dans Finder.`** · réutilisation
  exacte de `errors.listing.permissionDenied.suggestion` et `.noPermissionErrno.suggestion` du même fichier · high.
- **`Finder` sans article dans `errors.json`** · le fichier est homogène sur `dans Finder` / `utilisez Finder` (5
  occurrences, aucune avec article), alors que macOS `fr` dit « le Finder ». On garde la cohérence interne du fichier ;
  le génitif reste `du Finder` là où il apparaît ailleurs dans le catalogue (« Fenêtre de recherche du Finder ») · high.
- **`Unlock it in Finder's Get Info panel` → `Déverrouillez-le dans Finder (Lire les informations)`** · calque de
  `errors.write.fileLocked.suggestion.mac` du même fichier, lui-même issu de macOS Finder NE18 (« Choisissez Fichier >
  Lire les informations, désélectionnez « Verrouillé », puis réessayez. ») · high. `Get Info` → `Lire les informations`
  est déjà dans `terms.json`.
- **System Integrity Protection → `la protection de l'intégrité du système`** · macOS Finder `fr`, mot pour mot («
  Certains éléments de la corbeille ne peuvent pas être supprimés en raison de la protection de l'intégrité du système.
  ») · high. Apple LOCALISE ce nom, il ne fait donc pas partie des marques à garder en anglais. La clé reprend aussi le
  moule `en raison de …` de cette phrase, ce qui évite la répétition « protège … par la protection » : « Cet élément ne
  peut pas être renommé en raison de la protection de l'intégrité du système de macOS. »
- **`isn't available any more` (volume) → `n'est plus disponible`** · macOS Finder NE7 (« … car le disque « ^0 » n'est
  plus disponible. ») · high.
- **`Only zip archives can be changed` → `Seules les archives zip sont modifiables`** · reprend `editable → modifiable`
  de la passe `archive-browsing` (adjectif plutôt que passif) · high. `zip` reste verbatim.
- **Sortir / faire passer un élément d'une archive** · `renameOutOfArchive` → `sortir un élément d'une archive` ;
  `renameAcrossArchives` → `faire passer un élément d'une archive à une autre` · high (choix rédactionnel). Les deux
  verbes évitent `déplacer`, réservé à la commande nommée juste après : `Utilisez plutôt Déplacer.` (`move → déplacer`
  dans `terms.json`, et `Déplacer` est le libellé de la commande Cmdr). Le sujet est le nom `Le renommage`, comme
  `queue.row.label` (`Renommage`).
- **`Something went wrong, and Cmdr couldn't tell what` →
  `Un problème est survenu, et Cmdr n'a pas pu identifier lequel.`** · `problème` est le repli calme déjà retenu pour
  `error` (voir `terms.json`) ; ni « erreur » ni « échec » · high.
- **`The volume couldn't finish that` → `Le volume n'a pas pu terminer cette opération.`** · reprend le repli
  `Couldn't finish → N'a pas pu se terminer` de la passe `queue` · high.
- **`The connection didn't answer in time` → `La connexion n'a pas répondu à temps.`** · macOS AppKit atteste
  `n'a pas répondu` (« L'application « %@ » n'a pas répondu à la demande de service. ») · high. On garde le verbe plutôt
  que le nom `délai dépassé`, qui reste le terme du STATUT (voir `terms.json`).
- **`The destination can't hold that name` → `La destination ne peut pas stocker ce nom.`** · reprend
  `errors.listing.invalidName.explanation` (« … un nom que la destination ne peut pas stocker ») du même fichier · high.
  La suite `Choisissez-en un autre.` calque macOS Finder RN17 (« Veuillez choisir un autre nom. »), sans le « Veuillez »
  que le guide de style écarte.
- **`password-protected` → `protégé(e) par un mot de passe`** · GNOME Nautilus, mot pour mot (« “%s” is
  password-protected » → « « %s » est protégé par un mot de passe. ») · high. Accord au féminin avec `archive` : « Cette
  archive est protégée par un mot de passe. »
- **`That password didn't work` → `Ce mot de passe n'est pas le bon.`** · tentative. macOS Finder PE77 atteste « le mot
  de passe est incorrect », mais l'anglais choisit délibérément une formulation plus douce ; `n'est pas le bon` garde le
  registre chaleureux et impute le refus au mot de passe, pas à la personne. Double Commander écrit « Mot de passe
  erroné ! », trop sec et exclamatif.

Deux pièges de sens à ne jamais « corriger » :

- **`errors.mutation.timedOut` ne dit PAS un échec.** L'opération n'a pas été annulée et peut encore réussir : « Le
  volume n'a pas encore répondu, donc la modification peut encore aboutir. » Même famille que
  `fileOperations.mkdir.timeoutMessage` et `fileExplorer.rename.unconfirmed`, et même discipline : on nomme l'OBJET
  (`la modification`), on ne pronominalise pas, sinon l'antécédent masculin le plus proche serait `le volume`.
- **`errors.volume.deviceSessionReset` ne dit PAS un débranchement.** L'appareil (téléphone MTP) est toujours branché et
  a seulement redémarré sa session : « L'appareil a redémarré sa connexion. Patientez quelques secondes, puis réessayez.
  » Calque de `errors.listing.deviceReconnecting.explanation` (« La connexion à l'appareil … a redémarré … L'appareil
  est toujours branché »). Le voisin `errors.volume.deviceDisconnected`, lui, décrit un vrai débranchement.

## Refus de placement dans la corbeille (`errors.mutation.trashNotSupported`, `.trashRefused`)

Deux clés ajoutées après les 31 ci-dessus, même surface (une ligne sous le champ de nom, ou une notification brève),
même famille RAW.

- **`This volume has no Trash` → `Ce volume n'a pas de corbeille`** · `trash → corbeille` est déjà au guide de style
  (macOS Finder « Corbeille », « Vider la corbeille ») · high. On écarte « ne prend pas en charge la corbeille »
  (`errors.write.trashNotSupported.message` du même fichier) : l'anglais dit ici « has no Trash », plus direct, et la
  ligne doit tenir dans une boîte de dialogue étroite.
- **`delete permanently` → `supprimer définitivement`** · macOS Finder, mot pour mot (« Voulez-vous vraiment supprimer
  définitivement cette version du document « %@ » ? ») ; identique à `errors.write.trashNotSupported.suggestion` du
  catalogue `fr` · high. La phrase entière : « Ce volume n'a pas de corbeille : la seule option est de supprimer
  définitivement. » Le deux-points remplace le `so` anglais, et on garde le verbe plutôt que le nom « la suppression
  définitive », que le guide de style écarte.
- **`macOS wouldn't move this to the Trash` → `macOS a refusé de placer cet élément dans la corbeille.`** ·
  `move to trash → placer dans la corbeille` vient du guide de style et de macOS Finder (« Placer dans la corbeille », «
  Le Finder souhaite placer « ^1 » dans la corbeille ») · high. `a refusé` reste factuel et non alarmiste, et le
  catalogue `fr` s'en sert déjà (« Connexion refusée », « Authentification refusée ») ; ni « erreur » ni « échec ».
  `this` devient `cet élément`, le nom générique du catalogue, plutôt qu'un pronom sans antécédent. La phrase reste
  courte à dessein : la raison technique s'affiche à part sous « Détails techniques ».

## Éjection et déconnexion refusées : les neuf clés `errors.eject.*`

Une phrase par clé, affichée dans une notification brève en haut à droite. Chaque valeur est la phrase qui suit le
deux-points d'un des deux moules du catalogue `fr` : `fileExplorer.pane.ejectFailedToast` (« Impossible d'éjecter
{volumeName} : {message} ») ou `.disconnectFailedToast` (« Impossible de se déconnecter : {message} »). D'où deux
contraintes : la valeur commence par une majuscule (comme toutes les valeurs `errors.*`) et ne redit jamais « Impossible
de … », que le moule porte déjà. Famille RAW : apostrophes ASCII simples, aucun ICU, aucun `{token}`.

- **removable → `amovible`** · macOS Tier 1 (« Volume amovible », « Volumes amovibles », « Amovible ») ; Nautilus et
  Thunar `fr` confirment (« médias amovibles », « Disque amovible ») · high.
- **network share → `partage réseau`** · terminologie Microsoft FRA (entrée `network share` → `partage réseau`,
  masculin) et déjà en place dans le catalogue `fr` (`errors.listing.remotePermissionDenied.explanation`,
  `settings.indexing.askForEachDrive.description`) · high. macOS `fr` ne nomme pas le concept (il dit « Dossier partagé
  », qui est le partage local du Finder, un autre concept).
- **disconnect, TRANSITIF (Cmdr déconnecte un appareil) → `déconnecter` tout court** · macOS `fr` (« Ne déconnectez pas
  l'appareil tant que l'effacement n'est pas terminé. ») et le catalogue (`fileExplorer.mtp.disconnected` « L'appareil a
  été déconnecté. ») · high. Le pronominal `se déconnecter` reste réservé au fait de quitter un SERVEUR (macOS Finder «
  Disconnect » → « Se déconnecter »), qui est le sens du moule `.disconnectFailedToast`. Les deux emplois coexistent
  dans ce lot, ne pas les uniformiser.
- **`isn't connected any more` → `n'est plus connecté`** · calque du moule déjà livré dans ce fichier
  (`errors.mutation.volumeGone` « Ce volume n'est plus disponible, rien n'a donc été modifié. ») ; macOS `fr` atteste
  l'adjectif (`Connecté` / `Non connecté`, « Mac connectés à votre identifiant Apple ») · high.
- **`there's nothing to eject / to disconnect` → `il n'y a donc rien à éjecter` / `… à déconnecter`** · reprend
  `errors.listing.deviceReconnecting.suggestion` du même fichier (« Il n'y a rien à débrancher. ») · high.
- **`Something is still using this drive` → `Quelque chose utilise encore ce disque.`** · macOS Tier 1 donne le verbe («
  Impossible d'éjecter le disque car il est utilisé par « %@ ». », « Impossible d'éjecter « ^0 » car il est utilisé. »)
  et le catalogue atteste l'agent indéfini (`errors.volume.deletePending` « … et quelque chose le maintient encore
  ouvert. ») · high. On garde la voix active de l'anglais plutôt que le passif d'Apple.
- **`wouldn't close its connection` → `a refusé de fermer sa connexion`** · `a refusé` est le prédicat non alarmiste
  déjà retenu pour les `wouldn't` du catalogue (« macOS a refusé de placer cet élément dans la corbeille. », « Connexion
  refusée », « Authentification refusée ») · high. Ni « erreur » ni « échec ».
- **`idle` (un appareil MTP qui ne travaille plus) → `quand il n'est plus occupé`** · négation du settled
  `busy → occupé` (terminologie Microsoft, § Menus natifs) · high. La pile `fr` n'a aucun équivalent direct d'« idle » ;
  passer par `occupé` garde le lot cohérent avec l'infobulle `fileExplorer.navigation.ejectBusyTooltip`.
- **`unplug` → `débrancher`** · catalogue `fr` (`errors.listing.deviceReconnecting.suggestion`,
  `mtp.permissionDialog.helpText` « débranchez puis rebranchez l'appareil ») · high.
- **`Cmdr couldn't tell which device this is` → `Cmdr n'a pas reconnu cet appareil`** · `reconnaître` est le verbe du
  catalogue pour une référence que la machine ne rattache plus à rien (`errors.listing.staleConnection.explanation` « …
  une ancienne référence que le serveur ne reconnaît plus ») · high. La suite est coordonnée
  (`et ne peut donc pas le déconnecter`) plutôt que juxtaposée avec un `il` : le `il` d'une seconde proposition
  reprendrait `cet appareil`, l'antécédent masculin le plus proche, et dirait le contraire.
- **`Something went wrong, and Cmdr couldn't tell what` →
  `Un problème est survenu, et Cmdr n'a pas pu identifier lequel.`** · valeur reprise MOT POUR MOT de
  `errors.mutation.unexpected`, même anglais, même repli calme `error → problème` · high.

Un piège de sens à ne jamais « corriger » :

- **`errors.eject.timedOut` ne dit PAS un échec.** L'éjection n'a pas été annulée et peut encore se terminer toute seule
  : « Le disque n'a pas encore répondu, donc l'éjection peut encore aboutir d'elle-même. » Même moule que
  `errors.mutation.timedOut` (« Le volume n'a pas encore répondu, donc la modification peut encore aboutir. »), verbe
  `aboutir` compris, et même discipline : on nomme l'OBJET (`l'éjection`, féminin, d'où `d'elle-même`), on ne
  pronominalise pas, sinon l'antécédent le plus proche serait `le disque`.

Autres valeurs du lot : `busy` → « Cmdr déplace encore des fichiers sur ce disque. Éjectez-le une fois l'opération
terminée. » (`l'opération` est le nom générique du catalogue, cf. `errors.volume.cancelled` et `.notSupported` ; il
couvre la copie, le déplacement et la suppression que l'anglais résume par « that »). `notEjectable` → « Ce disque n'est
pas amovible, il reste donc connecté. »

valeurs, donc la règle d'espace avant ponctuation ne se pose pas ; aucune apostrophe doublée (famille RAW), aucun U+2019
ni U+202F.

## Le refus d'éjection nommé : qui tient le disque (`errors.eject.unmountRefusedBy*`, `.otherApps`)

Six clés qui affinent `errors.eject.unmountRefused` : au lieu de « Quelque chose utilise encore ce disque. », macOS a pu
dire QUI tient le disque. Mêmes contraintes que le lot précédent (valeur après le deux-points du moule, majuscule
initiale, famille RAW, apostrophes ASCII simples). Les trois clés `unmountRefused*` se lisent comme une seule famille :
même verbe, même seconde phrase, même chute « puis éjectez-le à nouveau ».

- **`X is still using this drive` → `X utilise encore ce disque.`** · reprend MOT POUR MOT le verbe du frère générique
  `errors.eject.unmountRefused` (« Quelque chose utilise encore ce disque. ») · high. macOS `fr` dit le même fait au
  passif (« Impossible d'éjecter le disque car il est utilisé par « %@ ». », `Finder`) ; on garde la voix active, comme
  le lot précédent l'avait déjà tranché, et le passif d'Apple ne sert que de caution pour le verbe `utiliser`.
- **Le `{app}` en tête de phrase ne prend ni article ni accord.** Le genre du nom inséré est inconnu à l'écriture («
  Preview », « Warp », « mds_stores »), donc il occupe un créneau de sujet nu, exactement comme
  `fileExplorer.navigation.driveIndex.driveLeaving` (« {name} est en cours de déconnexion, … »),
  `indexing.needsFreshScan.afterDisconnect` (« {name} a été déconnecté … ») et
  `errors.write.deviceDisconnected.sided.destination.copy` (« {volumeName} a été déconnecté … ») · high. Le verbe se met
  à la 3e personne du singulier pour `{app}` et du pluriel pour `{apps}`, qui porte toujours au moins deux noms.
- **`Close anything it / they have open there` → `Fermez ce que cette app / ces apps y a / ont ouvert`** · le
  démonstratif REMPLACE le pronom anglais · high. ❌ Jamais « ce qu'il y a ouvert » : l'antécédent masculin le plus
  proche est `ce disque`, et la phrase dirait que le disque a ouvert quelque chose (le même piège d'antécédent que la
  section précédente documente déjà deux fois). `app` couvre aussi les outils en ligne de commande, comme en anglais et
  comme `otherApps`.
- **`other apps` (dernier élément de la liste) → `d'autres apps`** · macOS `fr` l'écrit tel quel en position finale de
  liste : « … en le faisant glisser ici depuis le Finder ou d'autres apps. »
  (`fr/macOS/Finder/ICloudNoDocumentsView.json`, relevé 2026-09-16) · high. ❌ Pas « d'autres applications » : le
  catalogue `fr` dit « apps » partout (`errors.eject.unmountRefused` « les fichiers et les apps ouverts »,
  `errors.listing.notEnoughMemory.*` « beaucoup d'apps sont ouvertes », « Fermez quelques apps ») et macOS `fr` aussi («
  Apps », « toutes les apps », `AppKit/TouchBar.json`) ; « applications » est le registre Microsoft, pas le registre
  Apple. La valeur reste minuscule et sans point : `Intl.ListFormat('fr')` la soude au reste (« Preview et d'autres apps
  », « Preview, Warp, Photos et d'autres apps » — pas de virgule d'Oxford en français, et le joignant vient du
  formateur, jamais de la chaîne).
- **`disk image` → `image disque`** · macOS Tier 1 (`Finder/InfoWindowGeneralView.json` « Image disque : », « Valeur
  d'image disque » ; `Finder` « Graver l'image disque « ^0 » sur le disque… ») et déjà en place dans ce fichier
  (`errors.listing.readOnlyVolumeErrno.*` « une image disque montée en lecture seule ») · high. Féminin : « une image
  disque … ouverte ».
- **La clé de l'image disque part du DISQUE pour ne pas empiler trois « disque » d'affilée.** L'anglais attaque par
  l'image (« A disk image stored on this drive is still open. ») ; « Une image disque stockée sur ce disque » colle le
  mot deux fois en cinq syllabes. On écrit « Ce disque contient une image disque encore ouverte. Éjectez d'abord
  l'image, puis le disque. » : même information, sujet connu en tête, et la seconde phrase fait l'ellipse du verbe pour
  donner l'ordre des deux éjections d'un coup.
- **`still working with this drive` (macOS) → `travaille encore sur ce disque`** · `travailler sur` est la collocation
  française (`travailler avec` est un calque) et le catalogue atteste le verbe
  (`errors.listing.lockUnavailable.suggestion` « celles qui travaillent avec de nombreux fichiers ») · high. Verbe
  DIFFÉRENT d'`utiliser` à dessein, comme en anglais : `utiliser` est réservé à ce qui tient le disque et qu'on peut
  fermer ; ici il n'y a rien à fermer, seulement à attendre. `macOS` reste tel quel, y compris en tête de phrase avec sa
  minuscule (déjà livré : `errors.listing.notPermitted.explanation` « macOS a empêché Cmdr … »).
- **`Wait a minute` → `Patientez une minute` ; `Wait a moment` → `Patientez un instant`** · `patienter` est le verbe du
  catalogue pour une attente courte (`errors.listing.resourceBusy.suggestion` « Patientez un instant, puis revenez ici.
  », `errors.listing.deviceReconnecting.suggestion` « Patientez quelques secondes ») · high. La distinction minute /
  instant de l'anglais est gardée.
- **`Cmdr itself` → `Cmdr lui-même`** · Cmdr est masculin dans tout le catalogue `fr` (voir la note
  `crashed → s'est fermé de façon inattendue` du style guide) · high.
- **`send a report` → `envoyez un rapport`** · le catalogue nomme déjà l'action manuelle ainsi
  (`settings.updates.errorReports.description` « Vous pouvez toujours envoyer un rapport manuel depuis le menu Aide. »,
  `errorReporter.amend.unavailable` « envoyez un nouveau rapport depuis le menu Aide ») · high. ❌ Pas « envoyez un
  retour » : c'est l'autre surface (`feedback.dialog.title` « Envoyer un retour »), celle des idées et des envies, pas
  celle du problème technique. `rapport` nu, sans « d'incident », parce qu'il n'y a pas eu de plantage (règle du style
  guide).
- **`if it keeps happening` → `si cela continue`** · déjà livré dans ce fichier
  (`errors.listing.unexpectedSystemResponse.suggestion` « Si cela continue, ouvrez **Utilitaire de disque** … ») · high.

(famille RAW), aucun U+2019 ni U+202F.

## La notification de corbeille : annuler et remettre en place (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Nouvelle surface : après un déplacement vers la corbeille, une notification propose deux boutons (« Annuler », « Aller à
la corbeille ») ; la même commande existe dans la palette.

- **`undo` (bouton) → `Annuler`** · macOS AppKit MenuCommands (« Undo Smart Dash » → « Annuler Tirets intelligents »),
  GNOME Nautilus (« Undo » → « Annuler ») et le catalogue lui-même (`askCmdr.renameUndo.undo`) · high. À savoir : le
  français rend `Undo` ET `Cancel` par « Annuler », et ici le bouton apparaît juste après une opération, donc une
  lecture « annuler l'opération » est possible. macOS vit avec la même ambiguïté, et le résultat visé par l'utilisateur
  est le même dans les deux lectures, donc on garde « Annuler ». « Remettre » (le « Put Back » du Finder) serait la
  seule alternative sourcée si l'ambiguïté gênait un jour.
- **`put back` (ramener un élément de la corbeille là où il était) → `remettre en place`** · macOS Finder `N153.1` («
  Put Back » → « Remettre », `LocalizableMerged`) · high. « en place » explicite le « back where it was » de l'anglais.
  ❌ Pas `restaurer` (Nautilus, Tier 3) : le catalogue le réserve au renommage annulé (`askCmdr.renameUndo.undone`, «
  fichier restauré »), et la distinction entre rendre un EMPLACEMENT et rendre un NOM mérite d'être gardée.
- **`This drive doesn't keep a trash.` → `Ce disque n'a pas de corbeille.`** · un fait sur le disque, sans verdict, dans
  la lignée de `fileOperations.delete.noTrashWarningStrong` (« Ce volume ne prend pas en charge la corbeille. »).
  L'anglais dit `drive`, donc `disque` · high
- **`Nothing to put back.` → `Rien à remettre en place.`** · même moule que `askCmdr.renameUndo.unavailable` (« Rien à
  restaurer. … ou son disque n'est pas connecté. ») · high
- **`These items may already be back` → `Ces éléments sont peut-être déjà de retour`** · `de retour` est invariable,
  donc aucun participe à accorder avec un contenu qu'on ne connaît pas · high
- **La seconde moitié a son propre paramètre de comptage (`{skipped}`)**, donc elle porte un verbe conjugué et accordé :
  « … remis en place ; {skippedText} {skipped, plural, one {élément est resté} many {éléments sont restés} other
  {éléments sont restés}} dans la corbeille. » Le nom compté est `élément`, le mot du catalogue pour l'`item` que dit la
  source dans cette moitié, pas `fichier` comme dans la première · high
- **Le bouton de la notification et le nom de la commande portent le même texte** (« Aller à la corbeille »), comme ses
  voisins `commands.navParent.label` (« Aller au dossier parent ») et `commands.downloadsGoToLatest.label` (« Aller au
  dernier téléchargement »).
- Espace ASCII avant le `;` de `undonePartial`, apostrophes ASCII doublées dans `undoUnavailable` et `noTrashHere`,
  aucun U+2019 ni U+202F. Aucun `sameAsSourceJustification` nécessaire : les neuf valeurs diffèrent de l'anglais.

## Compléter un rapport déjà envoyé : les 11 clés `errorReporter.amend*`

Nouvelle surface : quand Cmdr a envoyé un rapport d'incident tout seul (envoi automatique), la notification « Rapport
d'incident envoyé » porte un bouton qui ouvre une fenêtre montrant ce qui est parti et où l'utilisateur peut écrire une
note **rattachée au même rapport** (rien n'est renvoyé une seconde fois). Si le rapport n'accepte plus rien, la fenêtre
le dit et renvoie vers Aide > Envoyer un rapport d'incident…

Les 11 valeurs sont ICU (apostrophes doublées), espace ASCII avant `:`, aucun U+2019 ni U+202F, aucun
`sameAsSourceJustification` nécessaire (les 11 diffèrent de l'anglais).

- **`Add to your error report` (titre) → `Ajouter à votre rapport d'incident`** · le moule `Ajouter à X` sans objet
  explicite est attesté tel quel chez Apple (macOS `Ajouter à la barre latérale`, `Ajouter au Dock`,
  `Ajouter aux favoris`), donc l'infinitif transitif sans complément d'objet passe en français comme en anglais · high.
  `Compléter votre rapport` se lit bien mais n'est pas attesté dans la pile comme verbe d'interface (la terminologie
  Microsoft ne l'a qu'en `autocomplétion`), et il casserait la chaîne `Ajouter…` qui court sur le titre, le bouton et la
  notification de confirmation.
- **`Add to report` (bouton) → `Ajouter au rapport`** · même verbe que le titre, court pour un bouton serré · high
- **`Adding…` → `Ajout…`** · exactement le moule du frère `errorReporter.dialog.sending` (« Envoi… ») : nom verbal +
  U+2026 · high
- **`Your note` → `Votre note`** · `note` est déjà le mot du catalogue pour ce champ (`errorReporter.dialog.noteLabel` «
  Ajouter une note (facultatif) », `noteTooLong` « La note est trop longue. ») · high
- **`What was sent` → `Ce qui a été envoyé`** · le frère `errorReporter.dialog.detailsToggle` dit « Ce qui va être
  envoyé » pour le futur ; le passé composé passif garde le parallélisme exact des deux bascules · high
- **`it'll join what the team already has` → `cela rejoindra ce que l'équipe a déjà reçu`** · `rejoindre` porte le
  `join` de la source sans promettre un second envoi · high. L'anglais enchaîne « Write a note, or attach your email,
  and … » ; le français passe par un deux-points (« Écrivez une note ou joignez votre e-mail : … »), plus naturel que la
  virgule avant `ou` calquée de l'anglais.
- **`attach your email` → `joignez votre e-mail`** · `joindre` est le verbe du catalogue pour rattacher l'e-mail à un
  rapport (`settings.updates.emailPrivacyNote` « … pour joindre à un rapport que vous envoyez ») · high
- **`That report can't take a note any more.` → `Ce rapport n'accepte plus de note.`** · un constat, sans verdict ni
  `erreur` / `échec` / `bloqué` (règle du guide de style) · high
- **`from the Help menu` → `depuis le menu Aide`** · `Aide` est le titre du menu chez Apple (`menu.bar.help`, Finder et
  Safari `fr`) et le catalogue dit déjà « depuis le menu Aide » dans `settings.updates.errorReports.description` · high
- **`Couldn't add your note. {reason}` → `Ajout de votre note impossible. {reason}`** · moule figé de la famille
  (`sendFailedToast` « Envoi du rapport d'incident impossible. », `prepareFailed` « Préparation de l'aperçu impossible.
  ») : nom verbal + `impossible` + point, puis la raison déjà traduite ; seul `saveFailedToast` « Enregistrement du lot
  impossible : » garde l'espace ASCII + `:` devant son détail technique · high
- **`Note added to your report. Your reference ID is` →
  `Note ajoutée à votre rapport. Votre identifiant de référence est`** · seconde moitié identique au frère
  `errorReporter.sentToast.message` (la valeur s'arrête juste avant le badge, sans ponctuation finale) ; `ajoutée`
  s'accorde avec `note`, féminin · high
- **`View or add notes to the report` → `Voir le rapport ou y ajouter des notes`** · les deux moitiés (regarder ET
  ajouter) sont tenues, et le pronom `y` évite de répéter « au rapport », ce qui garde le bouton court (38 caractères
  contre 31 en anglais) dans une notification où il voisine « Modifier les réglages » · high. `Voir` plutôt
  qu'`Afficher` : dans le catalogue, `Afficher` rend l'anglais **Show**, faire apparaître quelque chose à l'écran
  (`menu.view.showHiddenFiles` « Afficher les fichiers cachés », `queue.row.foreground` « Afficher »,
  `crashReporter.dialog.showDetails` « Afficher les détails du rapport »), alors que `Voir` rend **See**, consulter une
  information (`menu.app.licenseDetails` et `commands.appLicenseKey.seeDetails.label` « Voir les détails de la licence
  », `whatsNew.dialog.seeFullChangelog`, `askCmdr.wakeToast.openThread` « Voir pourquoi »), et c'est le sens ici.
  `Afficher` coûterait aussi trois caractères de plus. Le `View` de la barre des touches F est un troisième verbe,
  `Visualiser`, propre à la visionneuse (`fileExplorer.functionKeyBar.viewLabel` / `.viewAction`).

## La fenêtre sélectionner / désélectionner des fichiers (`selection.*`)

Sources du lot : macOS 26 Finder `fr` (`MenuBar.json`, ids `172.title` / `300488.title`), Total Commander `fr`
(`WCMD.INC.utf8` 542/544/3304-3316) et Double Commander `fr` (`doublecmd.po`, `&Unselect All`). La zone passe par ICU,
donc les apostrophes se doubleraient ; aucune valeur du lot n'en contient (les tournures retenues les évitent toutes).

- **select → `Sélectionner` ; deselect → `Désélectionner`** · macOS Finder `fr` : `Tout sélectionner` (`172.title`) et
  **`Tout désélectionner`** (`300488.title`) · high (Tier 1), déjà posé plus haut pour le menu Présentation. La famille
  orthodoxe confirme le verbe pour la surface exacte de Cmdr : Total Commander `fr` dit
  `Désélectionner tous les fichiers` / `Désélectionner un groupe`, Double Commander `fr` dit `Tout désélectionner`.
  Aucune source ne diverge, ce qui rend le français le plus simple des trois langues romanes du lot.
- **Les trois endroits qui nomment la fenêtre disent la même chose** : `menu.select.files` / `menu.select.deselectFiles`
  (`Sélectionner des fichiers…` / `Désélectionner des fichiers…`), `commands.selectionSelectFiles.label` /
  `commands.selectionDeselectFiles.label`, `settings.selection.recentSelections.maxCount.description`
  (`la boîte de dialogue Sélectionner / Désélectionner des fichiers`) et désormais les titres
  `selection.dialog.title.add` / `.remove`. Le bug corrigé par ce lot, c'était justement le titre qui contredisait
  l'élément de menu qui l'ouvre · high.
- **`Select these files` → `Sélectionner ces fichiers` ; `Deselect these files` → `Désélectionner ces fichiers`** · même
  paire de verbes que les titres, à l'infinitif (convention de libellé du `style.md`) · high. Le titre prend
  `des fichiers` (indéfini, la fenêtre ne sait pas encore lesquels) et le bouton `ces fichiers` (le résultat affiché
  au-dessus) : c'est l'anglais qui fait la même distinction (`Select files` / `Select these files`).
- **`… in the focused pane` → `… dans le panneau actif`** · forme déjà publiée dans le catalogue
  (`commands.navGoToPath.description` « Placer le panneau actif sur un chemin… », `commands.favoritesAdd.description` «
  le dossier actuel du panneau actif ») · high. **Les infobulles commencent littéralement par le texte du bouton** et
  n'ajoutent que le complément (`Sélectionner ces fichiers dans le panneau actif`) : le bouton et son infobulle se
  lisent d'un seul tenant.
- **`Press Enter to filter` → `Appuyez sur Entrée pour filtrer`** · décalque du frère `search.runHint` (« Appuyez sur
  Entrée pour lancer la recherche ») · high. **La touche s'appelle `Entrée`** (entrée déjà posée plus haut : `Enter` →
  `la touche Entrée`), jamais `Enter`. `filtrer` seul suffit ici, là où `rechercher` demandait la périphrase « lancer la
  recherche ».
- **`recent selections` → `sélections récentes`** · déjà publié dans
  `settings.selection.recentSelections.maxCount.label` (« Sélections récentes à mémoriser ») · high. Les cinq textes du
  popover reprennent la grammaire et le registre de leurs jumeaux de recherche `queryUi.recent.*`, en remplaçant
  `recherches` par `sélections` : `Afficher toutes les sélections récentes`, `Toutes les sélections récentes`,
  `Filtrer les sélections récentes`, `Aucune sélection récente ne correspond à ce filtre.`, `Sélections récentes`.
- **`selection.recent.popoverAria` et `.listboxAria` partagent le même anglais (`Recent selections`)** : leur valeur
  `fr` doit être strictement identique, sinon `i18n-terms` le signale. Les deux : `Sélections récentes`.
- **`Apply recent {mode} selection: {query}` → `Appliquer la sélection {mode} récente : {query}`** · décalque du moule
  déjà publié dans `search.recent.runAria` (« Relancer la recherche {mode} récente : {query} »), espace ASCII avant le
  deux-points comme partout dans le catalogue (`style.md` § Punctuation spacing) · high. `{mode}` arrive déjà traduit
  (`IA`, `Regex`, `Nom de fichier`) et `{query}` est du texte libre : le moule laisse les deux dans une position neutre,
  sans accord à résoudre.
- **`Matching what is shown in the list (the full path).` →
  `Correspond à ce que la liste affiche (le chemin complet).`** · `correspondre` est le verbe du catalogue pour « match
  » (`commands.selectionSelectFiles.description` « les fichiers correspondants ») et `chemin complet` est déjà posé
  (`errors.listing.nameTooLongErrno.*`) · high. Sujet sous-entendu (le motif), et `la liste affiche` plutôt que
  `ce qui s'affiche dans la liste` : actif, plus court, et aucune apostrophe à doubler.

## Un mot anglais, un mot français : la revue de dérive

Le catalogue portait 38 endroits où `fr` donnait deux noms différents au même texte anglais, le plus souvent parce
qu'une passe tardive avait touché `menu.json` en laissant `commands.json` sur l'ancienne formulation. Treize étaient de
vraies dérives et ont disparu ; vingt-quatre sont des frontières VOLONTAIRES (ou des angles morts du vérificateur) et
sont décrites plus bas pour que la prochaine passe ne les « uniformise » pas. La trente-huitième, `Connect to server`,
n'est plus une frontière du tout : le catalogue ne garde que la forme verbale, et le terme vit désormais dans la liste §
Termes.

### Corrigé

- **`View` (l'action F3, ouvrir le fichier dans la visionneuse intégrée) → `Visualiser`**, partout :
  `commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel` (qui disait `Afficher`) · `high`.
  ❌ Pas `Afficher` : c'est le verbe SHOW du catalogue (`commands.viewShowHidden.label` =
  `Afficher ou masquer les fichiers cachés`), et confondre les deux est exactement ce que cette entrée empêche. ❌ Pas
  `Présentation` : c'est le MENU (voir la frontière plus bas). Double Commander `fr` dit `Voir` pour la même action
  (`tfrmmain.actview.caption`) — correct, mais Tier 3, plus vague dans une palette de commandes, et deux clés sur trois
  disaient déjà `Visualiser`. À surveiller au contrôle de débordement : 10 caractères sur la barre F-touches.
- **`menu.file.quickLook` = `Coup d'œil`, avec UNE apostrophe ASCII** · la famille `menu.*` est NATIVE : Rust la lit par
  simple recherche dans une table, sans moteur ICU, donc un `''` s'affiche littéralement comme deux apostrophes dans la
  barre de menus de macOS (`isRawKey` dans `i18n-catalog-lib.ts`). La clé jumelle `commands.fileQuickLook.mac.label` est
  ICU et s'écrit donc `Coup d''œil` · `high`. Elle portait aussi l'apostrophe courbe U+2019, contre la règle « toujours
  U+0027 » de `style.md`.
- **`error report` → `rapport d'incident` partout** · `settings.updates.sendErrorReport` disait `rapport d'erreur`, seul
  contre cinq · `high`.
- **`Check for updates` → `Rechercher les mises à jour`** · `settings.updates.checkForUpdates` disait `Vérifier` alors
  que la commande et le menu disaient `Rechercher`. macOS `fr` tranche : Software Update rend `Checking for updates…`
  par `Recherche des mises à jour en cours…` et `Check for Updates` par `Rechercher les mises à jour` (vérifié sur macOS
  26.6.2, build 25G83, 2026-08-30) · `high`.
- **`Operation log` → `Historique des opérations`** · `settings.navigationAndFileOps.card.operationLog` disait
  `Journal des opérations`, contre la commande, le menu et le titre de la fenêtre · `high`.
- **`Reset to default` → `Réinitialiser au réglage par défaut`** et **`Reset all to defaults` →
  `Tout réinitialiser aux réglages par défaut`** · `settings.control.resetToDefault` et `settings.advanced.resetAll`
  disaient `Réinitialiser par défaut`, qui ne veut pas dire la même chose (« par défaut » y devient un adverbe) ·
  `high`.
- **`API key` → `Clé d'API`** aussi dans l'assistant (`onboarding.cloudSetup.apiKeyAria`, `.apiKeyPlaceholder.generic`
  disaient `Clé API`), conformément à l'entrée de `terms.json` · `high`.
- **`Got it` → `D'accord`** dans les deux · macOS `fr` traduit « Got It » par `D'accord` (vérifié sur macOS 26.6.2,
  2026-08-30) ; le catalogue disait `Compris` d'un côté et `J'ai compris` de l'autre · `high`.
- **`Press Enter to search` → `Appuyez sur Entrée pour rechercher`** dans les deux (`search.runHint` disait
  `pour lancer la recherche`) ; **`Word wrap` → `Retour à la ligne automatique`** dans les deux
  (`settings.viewer.wordWrap.label` inversait l'ordre des mots) ; **`Go to home folder` → `Aller au dossier personnel`**
  aussi sur le bouton de l'écran d'erreur ; **`Check your inbox…`** dit la même chose dans l'assistant et dans les
  réglages · `high`.
- **`{dir}` / `{dirs}` → `rép.` partout**, y compris `fileExplorer.summary.dirNoun` qui disait `dossier`/`dossiers` ·
  `high`. C'est la décision déjà prise pour les statistiques d'analyse, et le voisin immédiat dans la barre d'état
  (`fileExplorer.selectionInfo.dir`) affiche `RÉP.`.
- **`From:` devant un chemin → `De :`** · `fileOperations.scanPhase.fromLabel` disait `Depuis :` alors que le couple du
  dialogue de transfert est `De` / `À`. `De :` est aussi la forme des en-têtes de courrier · `high`.

### Frontières volontaires (ne pas uniformiser)

- **`Back` : `Précédent` dans le menu Aller, `Retour` sur les boutons de l'app** · `@menu.go.back` demande le mot exact
  du Finder, et macOS `fr` n'utilise que `Précédent` (6 occurrences dans `fr/macOS/`, aucun `Retour` isolé). Les boutons
  réseau et l'assistant sont des surfaces Cmdr et gardent `Retour` · `high`.
- **`View` : `Présentation` est le MENU, `Visualiser` est l'action** · macOS Finder `fr` nomme son menu View
  `Présentation`, et `@menu.bar.view` demande ce mot · `high`.
- **`Edit` : `Édition` est le MENU, `Modifier` est le verbe** · même raison, `@menu.bar.edit` · `high`.
- **`Zoom` : `Zoom` est la taille du texte, `Réduire/agrandir` est la fenêtre** · macOS `fr` nomme l'action de la
  pastille verte `Réduire/agrandir`, et le `@key` dit d'employer le mot de macOS ICI même s'il diffère · `high`.
- **`Unknown` employé seul s'accorde avec ce qu'il remplace** · `ai.local.modelUnknown` = `Inconnu` (le modèle,
  masculin) ; `fileOperations.transferProgress.sizeUnknown` = `(inconnue)` (la taille, féminin). Les deux sont justes et
  aucune ne va à la place de l'autre · `high`. Seule la seconde porte les parenthèses de la source ; ne les ajoutez ni
  ne les retirez, l'accord et la ponctuation sont deux décisions séparées.
- **`App` : `App` est la couleur de l'app, `Application` est la portée** · les options de couleurs désignent l'app comme
  SOURCE d'une teinte (étiquette de bouton très courte) ; `shortcuts.scope.app` désigne les raccourcis valables dans
  toute l'application · `high`.
- **`Running` : `En cours d'exécution` est un PROCESSUS, `En cours` est une tâche** · le serveur d'IA local tourne ; une
  opération du journal progresse. Le français distingue les deux · `high`.
- **`Rolling back` : le titre du dialogue prend les points de suspension, le statut prend `en cours`** ·
  `fileOperations.transferProgress.titleRollingBack` = `Retour en arrière...` (les points disent la progression) ;
  `operationLog.rollback.rollingBack` est une cellule de statut sans points, qui doit donc le dire en toutes lettres, en
  parallèle de `operationLog.status.running` = `En cours` · `high`.
- **`Canceled` : `Opération annulée` titre un panneau, `Annulé` est un statut** · les titres `errors.listing.*.title`
  nomment le sujet sous-entendu, comme `Interrupted` → `Opération interrompue` · `high`.
- **`Send feedback` : le TITRE et la commande disent `un retour`, le BOUTON d'envoi dit `le retour`** · c'est le motif
  déjà en place dans tout le catalogue : `errorReporter.dialog.title` = `Envoyer un rapport d'incident` contre
  `errorReporter.dialog.send` = `Envoyer le rapport`. Le titre nomme l'action en général ; le bouton agit sur l'objet
  précis qui est devant vous · `high`.
- **`Ask about your files` : le titre invite à plusieurs questions, le champ en attend une** · `askCmdr.empty.title` est
  l'état vide d'une conversation (`Posez des questions…`), `askCmdr.composer.placeholder` est le champ d'un seul message
  (`Posez une question…`). Le français explicite un nombre que l'anglais laisse ouvert · `high`.
- **`Modified` : `Modifié` est la DATE, `Modifiés` sont les raccourcis que vous avez changés** ·
  `shortcuts.section.filterModified` s'accorde au masculin pluriel avec les commandes, sans aucune date · `high`.
- **`Error` : `Problème` est un état lu par l'utilisateur, `Erreur :` est une étiquette de diagnostic** · les deux
  `@key` le disent explicitement · `high`.
- **`Search` : `Rechercher` est l'action, `Recherche` est le thème** · en français l'infinitif titre les dialogues et
  les boutons, mais une sous-section de la barre latérale des réglages est un nom · `high`.
- **`Put back …` : `restauré` concerne des NOMS, `remis en place` concerne des EMPLACEMENTS** · l'anglais réutilise une
  phrase pour deux annulations différentes · `high`.
- **`you@example.com` → `vous@example.com`, la même dans les trois champs** · `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder` et `onboarding.stepBeta.emailPlaceholder` portent la même adresse, et leurs `@key`
  l'exigent · `high`. La partie locale se traduit (`vous@`), le domaine reste `example.com`. ❌ Pas `exemple.com` : ce
  domaine est réel et enregistrable, alors que `example.com` est réservé aux exemples (RFC 2606).
- **Sept « divergences » n'en sont pas, et voici pourquoi elles reviendront** : `AI suggestions` / `AI suggestions:`,
  `Connected` / `Connected!`, `Copied` / `Copied!`, `On disk` / `On disk:`, `Preview` / `Preview:`, `Send report` /
  `Send report?` et `Start using Cmdr` / `Start using Cmdr!` ont un anglais DIFFÉRENT. `i18n-terms` les regroupe quand
  même : son normalisateur retire la ponctuation FINALE, mais la typographie française met une espace insécable AVANT
  `: ! ?`, si bien que `Copié !` se réduit à `Copié ` (avec l'espace) et ne coïncide plus avec `Copié`. Ne touchez pas à
  ces quatorze valeurs : le défaut est dans le normalisateur, pas dans la traduction.

## Mots qui ont divergé sans qu'aucun check puisse le voir

`i18n-terms` ne regroupe que des clés dont l'anglais est IDENTIQUE. Celles-ci ont un anglais légèrement différent, donc
seule la passe manuelle les trouve. Toutes sont corrigées.

- **`System default` disait « Réglages Système », c'est-à-dire le nom de l'app macOS** ·
  `settings.appearance.language.opt.system`, `.opt.systemWithLanguage` et
  `settings.appearance.dateTimeFormat.opt.system` · `high`. La première option du sélecteur de langue annonçait donc «
  Réglages Système », et sa description disait « « Réglages Système » suit la langue de votre Mac », ce qui ne veut rien
  dire. Désormais **`Par défaut du système`**, dans la famille `par défaut` que le reste du catalogue emploie
  (`menu.context.openWithDefault` « {app} (par défaut) », `settings.control.resetToDefault`). Ce n'était pas une dérive
  de terme mais une erreur de traduction.
- **`View` → `Visualiser` aussi dans les deux clés que le vérificateur ne voit pas** ·
  `fileExplorer.functionKeyBar.viewAction` (« Visualiser le fichier ») et
  `settings.appearance.showFunctionKeyBar.description`, qui énumère les touches F comme « (Renommer, **Visualiser**,
  Copier, etc.) » : l'énumération reprend le nom du bouton · `high`.
- **`error report` → `rapport d'incident` jusque dans les réglages** · `settings.updates.errorReports.label`,
  `settings.developer.verboseLogging.description` et `settings.advanced.maxLogStorageMb.description` disaient
  `rapport d'erreur` · `high`. Le français fond `crash report` et `error report` dans un seul terme (c'est la décision
  de `terms.json`, et le ton non alarmiste la motive), donc `settings.updates.attachEmailToReports.description`
  n'énumère plus « un rapport d'incident ou d'erreur » : les deux sont le même mot.
  `onboarding.stepBeta.crashReportsNote` disait encore `rapports de plantage`, une cinquième forme.
- **`Finder tag` → `tag`, pas `étiquette`** · les quatorze `commands.tagsToggle*` disaient `étiquette` contre
  `settings.listing.showTags.label` « Afficher les tags ». macOS Finder `fr` tranche pour `tag` · `high`.
- **`Passer en présentation Complet` / `Bref` ne s'accordait pas** · `présentation` est féminin, donc
  `commands.viewFullMode.label` / `.viewBriefMode.label` disent maintenant `complète` / `brève`, comme le menu
  (`menu.view.fullView` = « Présentation complète ») · `high`. La forme masculine reste juste après `mode` :
  `mode Complet`, `mode Bref`, où l'adjectif s'accorde avec `mode` et sert de NOM de la présentation.
- **`entries` → `entrées`** · `indexing.scan.counters` disait `éléments` contre `queryUi.results.indexReadyStatus` («
  Index prêt ({countText} entrées) »), deux compteurs de la même chose · `high`. `dirs` reste `rép.`, comme décidé.

### Frontières confirmées lors de cette passe (ne pas uniformiser)

- **`Keep` : `Garder les deux`, mais `Conserver` un fichier** · macOS `fr` ship exactement ce partage
  (`Garder les deux`, `Garder la sélection` d'un côté ; `Conserver`, `Conserver la copie partielle`,
  `Conserver le téléchargement` de l'autre). `garder` = laisser tel quel ou laisser tourner
  (`downloads.warnToast.keepOn`, `fileOperations.transferProgress.backgroundAria`, `viewer.search.stopTooltip`) ;
  `conserver` = préserver une chose ou une durée (`fileExplorer.extensionChange.keepOld`,
  `fileOperations.rollbackConfirm.keep`, `settings.operationLog.maxAge.label`) · `high`.
- **`Retry` : `Réessayer` est le bouton, `Nouvelle tentative` est l'état en cours** · le participe présent anglais
  (`Retrying…`) n'a pas d'équivalent verbal court en français, donc la progression prend le groupe nominal, et
  `fileExplorer.errorPane.retryInfo` compte les `Tentative nº {count}` · `high`.
- **`Roll back` : `revenir en arrière` est le verbe, `retour en arrière` le nom** · le bouton et le titre de la
  confirmation prennent le verbe (`fileOperations.rollbackConfirm.rollBack`/`.title`), les pastilles de statut et le
  titre de progression prennent le nom · `high`. Détail du choix de famille : § « La famille `rollback` ».

## Les noms de sous-fenêtres viennent du Mac de la personne (`errors.git.*`, `errors.provider.*`)

Huit valeurs écrivaient les noms de sous-fenêtres à la main, et les deux valeurs `errors.git.*` les gardaient même en
anglais (« System Settings > Privacy & Security > Files and Folders »). Elles portent désormais les jetons
`{system_settings}`, `{privacy_and_security}` et `{files_and_folders}`, que l'app remplace à l'exécution par les noms
tels que le Mac de la personne les affiche. Ces valeurs sont RAW (pas d'ICU), donc pas d'apostrophes doublées.

- **Une préposition simple passe, une élision ou un accord non** · la valeur est inconnue à l'écriture, donc
  `dans {system_settings}` va bien, et tout ce qui devrait s'élider ou s'accorder avec elle est interdit. Les six
  valeurs gardent toutes `dans` ou la forme chemin `**… > … > …**` · `high`.
- **`Apple Account` → `Compte Apple`, `General` → `Général`, `Login Items & Extensions` → `Ouverture et extensions`** ·
  aucun jeton ne les couvre, ce sont donc du texte ordinaire ; macOS 26 `fr` (`AppleIDSettings.appex`,
  `InfoPlist.loctable`, `CFBundleDisplayName` ; `LoginItems.appex`, `Localizable.loctable` ; vérifié sur macOS 26.6.2,
  build 25G83, 2026-08-30) · `high`. Cohérent avec `errors.listing.diskFullErrno.suggestion`
  (`**{system_settings} > Général > Stockage**`).

## `Restaurer` nomme l'objet : l'ancien nom (`askCmdr.renameUndo.undone`, `.partial`)

L'anglais partageait une phrase avec l'annulation de la corbeille (« Put back {countText} {files}. ») et dit maintenant
CE QUI revient : l'ancien nom.

- **`Put the old names back on N files.` → « Anciens noms restaurés pour N fichiers. »** · `restaurer` est ce que le
  catalogue réserve à l'annulation d'un renommage (`askCmdr.renameUndo.undoing`, « Restauration des anciens noms… » ;
  `askCmdr.renameUndo.unavailable`, « Rien à restaurer. »), et `remettre en place` reste celui de la corbeille
  (`fileOperations.trash.undone`) · `high`. La phrase garde le participe sans verbe conjugué des clés sœurs
  (`askCmdr.renameUndo.applied`, « {countText} fichiers renommés. »), et l'accord suit le nom, pas le fichier.
- **`menu.app.showAll` / `menu.app.hideOthers` étaient déjà justes** · « Tout afficher » et « Masquer les autres » sont
  mot pour mot ce qu'affiche le menu de l'app sur macOS 26 `fr` (Finder `MenuBar.strings`, `300730.title` /
  `300729.title`, vérifié sur macOS 26.6.2, build 25G83, 2026-08-30), et ils correspondent à `commands.appShowAll.label`
  / `commands.appHideOthers.label` · `high`.

## Une opération à moitié revenue en arrière : terminer le retour (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Terminer le retour en arrière`** · reste dans le champ lexical que le catalogue tient déjà
  pour cette fonction (`operationLog.dialog.rollBack` = « Revenir en arrière »,
  `operationLog.rollback.partiallyRolledBack` = « Retour en arrière partiel ») · high. Le moule `Terminer + <nom>` est
  celui d'Apple : macOS Finder `fr` écrit « Terminer la copie » pour `Finish Copying` (vérifié sur macOS 26 Finder `fr`,
  2026-08-30). `Terminer` dit « mener à son terme », jamais « relancer », ce qui est tout l'enjeu de la clé : la ligne
  propose de FINIR un retour interrompu, pas d'en lancer un nouveau.
- **Les deux clés `finishRollBack` doivent rester strictement identiques.** `operationLog.dialog.finishRollBack` et
  `fileOperations.rollbackConfirm.finishRollBack` partagent le même anglais (`Finish rolling back`) et la même action ;
  si leurs valeurs `fr` divergent, `i18n-terms` le signale. Le catalogue faisait déjà ce couple pour `Roll back` («
  Revenir en arrière » des deux côtés).
- **`Finish rolling this back?` → `Terminer le retour en arrière sur cette opération ?`** · décalque du frère
  `fileOperations.rollbackConfirm.title` (« Revenir en arrière sur cette opération ? ») : même infinitif, même
  `cette opération`, même espace ASCII avant le point d'interrogation (`style.md` § Punctuation spacing) · high.
- **L'avis sous la ligne recycle `fileOperations.rollbackConfirm.bodyUndoByDeleting`** : « Cmdr est revenu en arrière
  sur ce qu''il a pu et a laissé le reste tel quel. Terminer le retour en arrière demande un nouveau passage, et Cmdr
  laisse de côté tout ce dont il n''est toujours pas sûr. » La seconde moitié reprend mot pour mot le frère (« laisse de
  côté tout ce dont il n''est pas sûr »), et `a laissé le reste tel quel` tient le ton de `rollbackConfirm.leaveAsIs` («
  Laisser tel quel ») · high. La phrase ne promet volontairement pas un retour complet : ce qui reste peut être des
  fichiers que Cmdr n'arrive pas à rapprocher de ce qu'il a enregistré, et ceux-là seront de nouveau laissés de côté.
  `revenu` s'accorde au masculin (Cmdr), comme partout ailleurs dans le catalogue.
- **Point faible du lot : `another pass` → `un nouveau passage`** · tentative. Aucune source du corpus ne rend l'idée «
  repasser une fois de plus sur l'opération » ; le tour reste compréhensible, mais il n'est pas sourcé. Deux pistes
  écartées : `reprend l'opération`, qui se lit comme relancer l'opération d'origine (exactement le contresens que cette
  clé existe pour éviter), et `une seconde passe`, du jargon. À revoir en priorité si un relecteur natif se présente.
- **`in {folder}` → `dans {folder}`** · high. Le français n'accorde rien après `dans`, donc n'importe quel nom de
  dossier passe sans retouche. Avec `queue.row.reversalDeleting` la ligne se lit « Suppression de ce qui a été créé dans
  Backup » : c'est la préposition qui dit que la suppression a lieu à l'INTÉRIEUR du dossier. Sans elle, le nom seul
  laisse croire que le dossier lui-même va disparaître, et c'est le bug que cette clé corrige.

## La notification après un retour en arrière interrompu (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

Nouvelle surface : l'utilisateur a lancé une copie ou un déplacement, a demandé le retour en arrière, et cette
notification dit ce que le retour a pu défaire. Elle empile jusqu'à trois parties : un titre (une seule des six clés),
la ligne `leftBehind`, puis la liste à puces des `reason.*`. Le ton est « Cmdr a fait attention », jamais une excuse ni
une alerte.

- **Le moule des `reason.*` est celui d'`askCmdr.renameUndo.skipReason.*`, repris tel quel** : « {name} laissé tel quel
  : <raison>. » et « {countText} {count, plural, …} tels quels : <raison>. » · le catalogue lui-même · high. C'est la
  même mécanique produit (Cmdr refuse de toucher ce qu'il ne peut pas rapprocher de ce qu'il a écrit), donc les deux
  listes doivent se lire pareil. **Deux clés ont un anglais mot pour mot identique à leur sœur `renameUndo`**
  (`folderNotEmpty.named` et `folderNotEmpty.counted`) : leurs valeurs `fr` sont strictement identiques, sinon
  `i18n-terms` le signale, à juste titre. `unverifiable.named` s'en approche mais n'est pas identique (l'apostrophe est
  courbe côté `renameUndo`, doublée côté `cancelRollback`), donc `i18n-terms` ne la contraint pas : le `fr` reste
  identique quand même, parce que c'est la même phrase.
- **`item` → `élément`, pas `fichier`.** Les sœurs `renameUndo` disent `fichier` parce que leur anglais dit « file » ;
  ici l'anglais dit « item » et couvre les dossiers créés par l'opération · `élément` de `terms.json` (macOS Tier 1) ·
  high. Ne pas uniformiser les deux familles sur un seul nom.
- **`laissé` reste au masculin singulier devant un `{name}` inconnu**, comme dans `renameUndo.skipReason.drift.named`.
  Ce n'est pas un accord au petit bonheur : `fichier`, `dossier` et `élément` sont tous masculins, donc le nom implicite
  est masculin quel que soit le nom de fichier qui arrive · high.
- **Les titres « complets » (`doneDeleting`, `doneMovingBack`) portent la totalité par `tout`, jamais par un article
  devant le nombre.** « Cmdr a supprimé tout ce qu'il avait écrit : {countText} éléments. » et « Cmdr a tout remis en
  place : {countText} éléments. » ❌ Pas `les {countText} éléments` : la branche `one` donnerait « les 1 élément ».
  L'anglais a le même problème et le règle autrement, en passant la phrase entière dans le pluriel pour que sa branche
  `one` puisse dire « the item » sans le nombre ; le deux-points suivi du décompte met le nombre hors de portée de
  l'article, et les deux branches françaises restent identiques · high. `remettre en place` est bien le verbe de
  `fileOperations.trash.undone`, comme le demande la description de la clé.
- **Les titres « partiels » (`someDeleted`, `someMovedBack`) reprennent le moule participial du catalogue** («
  {countText} éléments supprimés. », « {countText} éléments remis en place. »), celui de `trash.undone` et de
  `askCmdr.renameUndo.applied` · high. Le contraste complet/partiel passe donc par `Cmdr a tout …` contre un simple
  décompte, ce qui rend exactement le `the` / pas de `the` de l'anglais sans rien promettre de trop.
- **« Stopped after … » → `Retour en arrière arrêté après …`** · reprend les pastilles du journal
  (`operationLog.rollback.partiallyRolledBack` « Retour en arrière partiel ») · high. ❌ Écarté : « Cmdr s'est arrêté
  après avoir supprimé … », plus court et plus actif, mais « Cmdr s'est arrêté » se lit une demi-seconde comme « Cmdr a
  quitté », ce qu'une notification calme ne peut pas se permettre. Nommer le retour en arrière coûte quelques caractères
  et lève l'ambiguïté.
- **« The rest are still there. » → `Les autres sont toujours là.` ; « The rest stayed where the move put them. » →
  `Les autres sont restés là où le déplacement les avait mis.`** · `déplacement` de `terms.json` · high. `Les autres`
  plutôt que `Le reste` : le décompte qui précède fournit l'antécédent pluriel et le participe s'accorde normalement.
- **`leftBehind` recycle mot pour mot la promesse des confirmations** : « Cmdr laisse de côté tout ce dont il n'est pas
  sûr » (`rollbackConfirm.bodyUndoByDeleting` et sœurs) · high. La notification répète ainsi la phrase que l'utilisateur
  venait de lire avant de lancer le retour, ce que demande la description de la clé. La liste des `reason.*` garde
  `laissé tel quel` : `laisser de côté` (la règle) et `laisser tel quel` (le constat, ligne par ligne) sont assez
  proches pour ne pas dérouter et assez distincts pour que la ligne d'annonce ne se confonde pas avec ses puces.
- **« so these stayed where they are: » → `donc voici ce qui n'a pas bougé :`** · high. `voici` annonce la liste à puces
  qui suit, comme le fait `these` en anglais. ❌ Pas `ce qui est resté en place` : `en place` est déjà pris par
  `remettre en place` (revenir à l'emplacement d'origine), et ces éléments-là sont justement restés à la DESTINATION.
- **`spotTaken` : « where it is » → `sur place`, « where it came from » → `sa place d'origine`** · high. `sur place`
  évite une seconde structure de pluriel dans la version comptée (« … laissés sur place : … leur place d'origine. »), et
  `place d'origine` fait écho à `remettre en place` : ce qui empêche la remise en place, c'est que la place est prise.
  Le sujet reste `quelque chose d'autre`, aussi vague que l'anglais, et jamais un pronom qui pourrait renvoyer à Cmdr.
- **`drift` : « after Cmdr put it there » → `depuis que Cmdr l'a mis là`** · high. `depuis que` calque le
  `depuis le renommage` de la sœur `renameUndo.skipReason.drift.named` et dit mieux qu'`après que` qu'un changement est
  survenu entre-temps. Le participe reste invariable à l'oreille (`mis` / `mis`), donc aucun accord ne dépend du
  `{name}`.
- **`failed` : « Couldn't undo {name}. » → `Cmdr n'a pas pu revenir en arrière sur {name}.`** · même moule que
  `operationLog.rollback.refusalUnexpected` (« Cmdr n'a pas pu lancer le retour en arrière. ») et
  `askCmdr.renameUndo.refusedBatches` · high. Deux pistes écartées : `Retour en arrière impossible pour {name}`, le
  moule `<nom verbal> impossible` de `style.md`, qui redirait mot pour mot la pastille
  `operationLog.rollback.notRollbackable` (« pas éligible ») alors qu'ici Cmdr a essayé ; et
  `Cmdr n'a pas pu annuler {name}`, qui rouvrirait la collision `Annuler` = Cancel que toute la famille
  `retour en arrière` existe pour éviter.
- **« Its drive may be disconnected or read-only. » → `Son disque est peut-être déconnecté ou en lecture seule.`** ·
  macOS Finder `fr` (« Cet emplacement est en lecture seule. », « un volume en lecture seule », vérifié dans le corpus
  de référence `fr/macOS/Finder/`, 2026-08-31) et le catalogue (`fileExplorer.navigation.locationUnreachableToast`, « Il
  est peut-être déconnecté. ») · high.
- **`rollbackConfirm.body` remis à jour** : la première moitié existante est conservée et les deux phrases sont soudées
  par `, et` comme en anglais ; la phrase ajoutée est reprise mot pour mot de la sœur `bodyUndoByDeleting` (« Cmdr
  laisse de côté tout ce dont il n'est pas sûr, il peut donc en rester quelques-uns. »), dont l'anglais est identique.
- Branches CLDR `fr` `one` / `many` / `other`, `many` identique à `other`. Les clés `.counted` comptent toujours au
  moins deux éléments, donc `tels quels` et `leur place d'origine` restent hors des branches, exactement comme les sœurs
  `renameUndo.skipReason.*.counted`.
- Espace ASCII avant chaque `:` , apostrophes ASCII doublées (ICU), aucun U+2019 ni U+202F. Aucun
  `sameAsSourceJustification` : les 18 valeurs diffèrent de l'anglais.

### `cancelRollback.stagedLeftover.*` (les restes de Cmdr lui-même à destination)

Deux lignes sur un fichier de travail créé par Cmdr et qu'il n'a pas réussi à retirer de la destination. Elles ne font
PAS partie de la liste `reason.*` : là, Cmdr protège les fichiers de la personne ; ici, il s'agit de son propre reste.

- **`unfinished copy` → `copie incomplète`** · `incomplet` est le mot d'Apple (macOS `LA33` : « endommagée ou incomplète
  »), `copie` le substantif de `NE111` (« conserver une copie réactivable ») · `high`
- **`at the destination` → `à destination`** · forme déjà employée par `conflictsUnknown` (« ce qui se trouve déjà à
  destination ») · `high`
- **`transfer` (substantif) → `transfert`** · déjà dans le catalogue (`errors.listing.deviceReconnecting.explanation` :
  « après un transfert annulé ou interrompu ») · `high`
- La deuxième phrase reprend `Cmdr` comme sujet, comme `leftBehind` (« Cmdr laisse de côté… »).
- ⚠️ **`lors d'un transfert ultérieur`, ❌ jamais « la prochaine fois ».** Le nettoyage de Cmdr épargne tout ce qui a
  moins d'une heure : une nouvelle tentative immédiate ne retire donc rien. Promettre le contraire serait exactement le
  défaut que cette ligne corrige.

## L'écran de blocage quand le WebKit est trop ancien (`main.oldWebkit.*`)

Trois chaînes que Cmdr affiche à la place de son interface quand le Safari du Mac est trop ancien. Elles vivent dans la
coquille HTML, pas dans l'app : c'est tout ce que cette personne verra de Cmdr.

- **`Software Update` → `Mise à jour de logiciels`** · nom du volet dans les Réglages Système ; la trace Tier 1 de
  Finder confirme le terme (`Apple Device Software Update File` → `Fichier de mise à jour logicielle d'appareil Apple`)
  · `high`.
- **`Quit` → `Quitter`** · déjà dans `terms.json`, confirmé par la clé `Quit` d'AppKit · `high`.
- **Apostrophe ASCII doublée, comme tout le catalogue** (`d''une`, `L''interface`) : ces clés sont ICU (l'anglais écrit
  `Cmdr''s`). Une apostrophe courbe n'est pas un échappement ICU et passe tous les contrôles sans bruit ; la seule
  parade est un balayage périodique `rg '’' apps/desktop/src/lib/intl/messages/fr`.
- **`Safari`, `Mac` et `15.4` restent tels quels.** `Safari` est dans `BRAND_WORDS`.

## L'avis « ancien macOS » (`main.oldMacos.*`)

Une boîte de dialogue affichée une seule fois sur un Mac sous macOS 12 : Cmdr démarre, mais on est hors de la plage
testée. Ton honnête et détendu, ni excuse ni avertissement, puisque l''app fonctionne.

- **`supports` → `prend en charge`** · macOS Finder (`… car elle n''est pas prise en charge.`) · `high`. Pas `supporte`,
  qui est un anglicisme.
- **`X and up` → `X et les versions plus récentes`** · macOS AppKit (`… requiert une version plus récente de macOS.`) et
  SystemSettings (`OS X %@ ou ultérieur`) · `high`.
- **`best effort` → `il fait au mieux`** · le pile ne contient pas le terme (seulement des définitions QoS réseau) ·
  `high` pour la paraphrase. Surtout pas `au mieux de ses efforts`, qui est du français de contrat.
- **`look off` → `être décalés`** · registre courant, et il évite « erreur » / « échec », interdits par la voix.
- **La dernière phrase, c''est David à la première personne**, au `vous` comme le reste du catalogue.
- Apostrophes ICU doublées : `j''aimerais`.

## Ce qu'Ask Cmdr lit à l'intérieur d'un fichier : consentement et rail (`ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.tool.inspectFile.*`)

Cinq clés ICU (apostrophes ASCII doublées, espace ASCII avant `:`). `contentsRule` remplace l'ancienne
`askCmdr.consent.noContents` : ses deux dernières phrases (la recherche de photos ; les suggestions qui attendent votre
accord) sont reprises mot pour mot de l'ancienne traduction, et seule la promesse d'ouverture change.

- **thumbnail → `vignette`** · macOS AppKit `fr` (« Taille de vignette : », « vignette de grande taille », « vignette de
  l’image du sélecteur d’onglets »), Total Commander `fr` (`25="&Vignettes"`), Double Commander `fr` (« Enregistrer les
  vignettes en cache ») ; Microsoft terminologie FRA dit `miniature`, Nautilus et Thunar mélangent les deux · high
  (macOS + les deux gestionnaires orthodoxes). L'ancienne `noContents` disait `miniatures` ; comme aucune autre clé du
  catalogue ne porte l'un ou l'autre mot, le terme Tier 1 s'impose sans rien casser. Vérifié dans
  `~/projects-git/vdavid/cmdr/_ignored/i18n/fr/`, 2026-09-02.
- **camera (d'une photo) → `appareil photo`** · macOS AppKit `fr` (`NSStillCameraTemplate` → « appareil photo »),
  Microsoft terminologie FRA (`camera` → « appareil photo », sens photo ; « caméra » est le sens vidéo/webcam) et le
  catalogue (« Prise en charge Android/Kindle/appareil photo ») · high. **camera details →
  `les détails de l'appareil photo`** : `détails` reprend `askCmdr.renameReview.evidence.metadata` (« Détails du
  fichier, pas son contenu ») · high.
- **location (d'une photo, où elle a été prise) → `localisation`** · macOS Finder `fr`, panneau d'aperçu d'une image
  (`PV5` / `PV56` « Location » → « Localisation », à côté de « Détail de l’image » et « Exif ») : c'est exactement ce
  champ-là · high. ❌ Pas `emplacement` : c'est le mot du catalogue (et de Dolphin) pour l'emplacement d'un FICHIER sur
  le disque, et la phrase parle justement d'autre chose. En prose, « including where it was taken » se rend par
  `y compris l'endroit où elle a été prise` (`contentsRule`), comme l'anglais lui-même varie entre `location` et
  `where it was taken`.
- **title and author (d'un PDF) → `son titre et son auteur`** · macOS AppKit `fr` (`Title` → « Titre »), KDE Dolphin
  `fr` (« Author » → « Auteur », « Title » → « Titre »), Microsoft terminologie FRA (`author` → « auteur ») · high.
- **page (d'un PDF) → `page`** · macOS Finder `fr` (« Page ^0 sur ^1 », « Pages ») · high. « a few pages of a PDF » →
  `quelques pages d'un PDF` ; « PDF pages » (liste) → `des pages de PDF`.
- **archive → `archive`** (fém.), **provider → `fournisseur`**, **tag → `tag`** : termes déjà posés, réutilisés tels
  quels.
- **what's inside an archive → `ce que contient une archive`** ; **the list of files inside an archive →
  `la liste des fichiers contenus dans une archive`** · racine `contenir` dans les deux, pour éviter deux
  `à l'intérieur` dans la même phrase de l'ancien texte des nouveautés (askCmdr.consent.whatsNew.body, retiré) («
  regarder à l'intérieur d'un fichier … ») · high (choix rédactionnel).
- **Parts of files → `Des parties des fichiers`** · pas `extraits`, qui irait au texte et aux pages mais pas à la liste
  d'une archive ni aux données Exif · high.
- **When you ask about a file → `Quand vous lui posez une question sur un fichier`** ; **a file you ask about →
  `un fichier sur lequel vous lui posez une question`** · calque de `askCmdr.empty.title` / `composer.placeholder` («
  Posez une question sur vos fichiers ») · high.
- **`inspectFile.doing` / `.done` → `Lecture du contenu de fichiers` / `A lu le contenu de fichiers`** · même moule que
  la sœur `imageFacts` (« Lecture du contenu de vos photos » / « A lu le contenu de vos photos ») : nom verbal au
  présent, `A <participe>` au passé (règle de la passe `ask-cmdr`). `contenu` dit explicitement que Cmdr lit dans le
  fichier, ce que l'anglais `inside` veut faire entendre sur cette ligne de transparence ; « Consultation de fichiers »
  aurait été pris pour un simple listage (`listDir` = « Consultation d'un dossier ») · high.

Notes de rédaction :

- **« a photo's camera details and location » se rend avec l'incise `pour une photo, …`** : « les détails de l'appareil
  photo … d'une photo » colle deux `photo` à trois mots d'écart.
  `… et, pour une photo, les détails de l'appareil photo et la localisation` (liste et ancien texte des nouveautés,
  retiré) ; `… ou, pour une photo, les détails de l'appareil photo, y compris l'endroit où elle a été prise`
  (`contentsRule`). Le `pour une photo` porte sur les deux compléments.
- **« never sends whole files, photos, or thumbnails » →
  `n'envoie jamais de fichiers entiers, de photos ni de vignettes`** : `de` répété après la négation, `ni` devant le
  dernier terme.
- **`askCmdr.empty.hint` et `settings.askCmdr.intro` tiennent la même promesse.** La seconde phrase des deux reprend les
  termes ci-dessus : « looks inside a file only when you ask about it » →
  `ne regarde à l'intérieur d'un fichier que lorsque vous lui posez une question à son sujet` (`regarder à l'intérieur`
  = ancien texte des nouveautés (retiré), `poser une question sur` = le moule du catalogue) ; « never changes a file
  without your approval » → `ne modifie jamais un fichier sans votre approbation` (racine `approuver` de `contentsRule`,
  « tant que vous ne l'avez pas approuvé »). ❌ Plus de `en lecture seule` ni de `ne change jamais rien` : Ask Cmdr
  écrit ses notes et propose des renommages, la promesse porte sur l'accord de la personne, pas sur l'absence
  d'écriture.

## Les deux info-bulles du bouton de retour en arrière (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Nouvelle surface : l'info-bulle dit maintenant ce que CE retour en arrière fait aux fichiers, et le bouton est désactivé
dès qu'un déplacement entre deux systèmes de fichiers atteint sa dernière étape (la suppression des originaux, alors que
tout est déjà arrivé à destination).

- **`rollbackTooltipStopAndMoveBack` → `Arrêter et remettre en place tous les fichiers déplacés jusqu'à présent`** ·
  même cadre que le frère `rollbackTooltip` (`Arrêter et …`), et `remettre en place` est le verbe déjà retenu pour le
  retour à l'emplacement d'origine (`cancelRollback.doneMovingBack`, « a tout remis en place ») · `high`. ❌ Pas
  `supprimer` : annuler un déplacement ne supprime rien.
- **`rollbackAlreadyLandedTooltip`** · la première proposition reprend l'image de `cancelRollback.moveAlreadyLanded` («
  est déjà arrivé à destination »), `retour en arrière` est le terme retenu pour le rollback
  (`rollbackUnavailableTooltip`), et `Annuler` est l'étiquette du bouton voisin (`fileOperations.button.cancel`), donc
  elle passe telle quelle · `high`.

## « Ouvrir un terminal ici » et son sélecteur d’app (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Nouvelle surface : une carte dans `Comportement > Navigation et opérations` qui choisit l’app de terminal lancée par la
commande. C’est macOS qui construit la liste ; seules les étiquettes se traduisent ici.

- **terminal (la catégorie d’app) → `terminal` ; Terminal (l’app d’Apple) → `Terminal`** · le macOS français d’Apple
  garde le nom en anglais (`Ouvrir dans Terminal`, clé `N67` de `macOS/Finder/LocalizableMerged.json`), et le mot
  générique français est le même emprunt · `high`. D’où la `sameAsSourceJustification` sur le titre de carte
  `settings.navigationAndFileOps.card.terminal` : il est identique à l’anglais à dessein.
- **Open terminal here (le nom de la commande) → `Ouvrir un terminal ici`** · construit sur le `Ouvrir dans Terminal`
  d’Apple, avec `ici` pour le lieu · `high`. La traduction de la commande elle-même (menu, palette) doit reprendre
  exactement cette forme.
- **Choose an app… → `Choisir une app…`** · le `Choose Application…` d’Apple (clé `N137`) dit `Choisir une application…`
  ; `app` plutôt qu’`application`, comme partout dans le catalogue · `high`.
- Les apostrophes des valeurs sont doublées pour ICU (`qu''il`, `l''astuce`, `d''une`).

## `Sort by relevance`: the search-results column tooltip (`fileExplorer.columns.sortByRelevance`)

New surface: the hover tooltip on the active column header of a search-results pane. The next click puts the rows back
into the search engine's own best-match-first order.

- **relevance (how well a result matches the search) → `pertinence`** · all four macOS sources agree: WorkflowKit
  (`Relevance (WFSearchSortOrder)` → `Pertinence`), AppStoreKit (`SEARCH_FACET_RELEVANCE` → `Pertinence`), Automator
  (`%1$[La pertinence]@ …`), and Musique · `high`. Lowercase after `par`, as everywhere else in the catalog. (verified
  on macOS 26.6.2 build 25G83, `plutil` dump of the shipped localizations, 2026-09-06)
- **Sentence frame → `Trier par pertinence`** · exactly the pattern of its sibling keys in `commands.json`
  (`Trier par nom`, `Trier par taille`) · `high`. No `sameAsSourceJustification`, and the value carries no apostrophe.

## `Documents and packages` : la ligne OOXML (`settings.archives.ooxml.*`)

New surface: a row in the same card as `Archives zip`, above the `Paquets d''application` card. It deliberately covers
BOTH Office documents (.docx, .xlsx, .pptx) and app packages (.jar, .apk), which is why even the English avoids naming
Office.

- **documents (the file kind) → `Documents`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Documents`; kinds
  `Document RTF`, `Document format texte`) · `high`. The whole value differs from English, so no
  `sameAsSourceJustification` is needed.
- **packages (generic, not only apps) → `paquets`** · macOS Finder (`Afficher le contenu du paquet`) and `terms.json`'s
  `app bundle → paquet` · `high`. Bare `paquets` keeps the row broader than the `Paquets d''application` card below it,
  the same split English makes with `packages` vs `app bundles`.
- **Sentence frame → `Ce que fait la touche Entrée sur un fichier …, …, … ou ….`** · takes `un fichier` from the sibling
  `settings.archives.zip.description` (`sur un fichier zip`) instead of repeating `un` five times, and drops the comma
  before `ou` (French has no serial comma) · `high`. `settings.archives.bundle.description` follows the same rule
  (`un .app, un .bundle ou un .framework`).

## Le hub des serveurs : panneau de connexion, refus et oubli (`servers.refusal.*`, `servers.paneState.*`, `fileExplorer.navigation.forget*`, `fileExplorer.navigation.disconnect*`, `menu.network.forgetServer`, `.forgetSavedPassword`)

Nouvelle surface : un panneau qui affiche l'état d'une connexion à un serveur (SMB, SFTP, WebDAV), la raison d'un refus,
et les confirmations pour oublier un serveur ou son mot de passe. Le tas de références (`_ignored/i18n/fr/`) est absent
de cette machine ; les termes ci-dessous viennent donc des paquets macOS installés (`plutil -convert json` sur les
`.strings` / `.loctable`, macOS 26.6.2 build 25G83, 2026-09-06) et du catalogue `fr` déjà livré.

- **Keychain Access (le nom de l'app) → `Trousseaux d''accès`** · `CFBundleDisplayName` de
  `/System/Library/CoreServices/Applications/Keychain Access.app/Contents/Resources/InfoPlist.loctable`, entrée `fr`
  (macOS 26.6.2 build 25G83, 2026-09-06) · `high`. Le pluriel est celui d'Apple, gardez-le. À distinguer du `trousseau`
  au singulier, qui nomme le MAGASIN et que le catalogue emploie déjà (`servers.sheet.remember`,
  `ai.secretError.keychainTitle`).
- **to trust / not trusted (un certificat) → `approuver` / `n''approuve pas`** · Security.framework,
  `authorization.prompts.loctable` (`is trying to trust a certificate` → `tente d''approuver un certificat`) et
  `SecErrorMessages.loctable` (`The root or anchor certificate is not valid.` →
  `Le certificat racine ou de point d''ancrage n''est pas approuvé.`), macOS 26.6.2 build 25G83, 2026-09-06 · `high`.
  Pas `faire confiance à`, plus long et non attesté chez Apple.
- **host key (la clé d'hôte SSH) → `la clé de {host}` / `la clé de ce serveur`** · aucun paquet macOS n'expose le terme
  ; construit sur `clé`, que le catalogue emploie déjà pour une clé d'API (`ai.secretError.*`) · `tentative`. Le génitif
  anglais `{host}''s key` passe par `la clé de {host}`, ce qui garde le remplacement dans un emplacement neutre.
- **signed out (l'état, pas la personne) → `Session fermée`** · restructuration exigée par la règle de genre : un
  participe accordé au sujet donnerait `Déconnecté(e)`. `session` est féminin et porte l'accord, la personne n'apparaît
  pas · `high`. L'action reste `s''identifier` (`terms.json`).
- **to forget (un serveur, un mot de passe) → `oublier`** · déjà livré par `menu.network.forgetServer`
  (`Oublier le serveur`), `menu.network.forgetSavedPassword` et `fileExplorer.network.share.forgetPassword`
  (`Oublier le mot de passe enregistré`) · `high`. Les deux titres de confirmation reprennent le libellé de menu MOT
  POUR MOT, sinon `desktop-i18n-term-consistency` compte une divergence.
- **doesn't support yet → `ne prend pas encore en charge`** · `prendre en charge` est la forme d'Apple
  (`URLs with the type “%@:” are not supported.` → `Les URL de type « %@: » ne sont pas prises en charge.`, NetAuthAgent
  `Localizable.loctable`, macOS 26.6.2 build 25G83, 2026-09-06) · `high`.
- **Connecting to {name}… → `Connexion à {name}…`** · Finder `LocalizableMerged.strings` clé `MN1` :
  `Connexion à « ^0 »…` (macOS 26.6.2 build 25G83, 2026-09-06), et le voisin déjà livré
  `fileExplorer.network.share.connecting` · `high`. Sans guillemets, comme la source anglaise et comme le voisin.

Notes de formulation :

- **L'étiquette d'accessibilité `disconnectPlaceAriaLabel` doit CONTENIR le libellé visible** (WCAG 2.5.3). Le libellé
  de l'action est `Se déconnecter` (`servers.paneState.disconnect`, `fileExplorer.unreachable.disconnect`,
  `menu.network.disconnect`), donc l'étiquette est `Se déconnecter de {name}` : la sous-chaîne `Se déconnecter` y figure
  telle quelle et dans l'ordre. ❌ Ne reprenez PAS le `Déconnecter` transitif du Finder (`LocalizableMerged.strings`,
  clés `MR10.1` / `N200`) : il casse la containment et divergerait des trois clés déjà livrées.
- **L'info-bulle grisée calque sa sœur `eject`.** `disconnectBusyTooltip` reprend mot pour mot la structure de
  `fileExplorer.navigation.ejectBusyTooltip`
  (`Impossible d''éjecter tant que des opérations sont en cours sur cet appareil`), en changeant seulement le verbe et
  l'objet : `Impossible de se déconnecter tant que des opérations sont en cours sur ce serveur`. Pas de `(occupé)` ici :
  ce marqueur est réservé aux éléments de MENU (§ busy).
- **`Cmdr couldn''t …` garde la marque.** Le catalogue a les deux moules : `Nom verbal impossible : {error}` quand une
  valeur suit un deux-points, et `Cmdr n''a pas pu <verbe>` quand l'anglais nomme le produit
  (`commands.handler.openTerminalHere.launchRefused`). Les quatre notifications de refus de ce lot prennent le second,
  sinon `desktop-i18n-dont-translate` signale la marque `Cmdr` perdue.
- **`Forget {name} ?` prend l'espace ASCII avant le `?`**, comme tout le set `fr` (style guide § Notes).
  `ne l''affiche plus` évite l'accord : l'élision de `le` rend le pronom neutre quel que soit le nom inséré.

## Le hub des serveurs : la table, ses colonnes et ses états (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.groupNetwork`, `shortcuts.scope.servers`, `shortcuts.scope.places`)

La rangée « Network » du sélecteur de volume devient **« Servers »** et ouvre un hub : une table de tous les serveurs
enregistrés (SFTP, WebDAV, SMB) plus ceux détectés sur le réseau local, avec les colonnes Nom / Type / Adresse / État /
Dernière utilisation et une dernière rangée « Ajouter un serveur… ». Le GROUPE qui contient la rangée reste « Réseau »
(`fileExplorer.navigation.groupNetwork`). Le tas de références (`_ignored/i18n/fr/`) est absent de cette machine ; les
termes viennent donc des paquets macOS installés (`plutil -convert json` sur les `.strings` / `.loctable`, macOS 26.6.2
build 25G83, 2026-09-06) et du catalogue `fr` déjà livré.

- **Servers (la rangée, la portée de raccourcis) → `Serveurs`** · le catalogue emploie déjà `serveur` partout
  (`servers.refusal.*`, `menu.network.forgetServer` → `Oublier le serveur`), et Finder dit `serveurs récents` (`MN3`) ·
  `high`. La rangée et le groupe portent maintenant deux mots distincts : `Serveurs` dans `Réseau`.
- **Places (la liste des partages sous un serveur) → `Emplacements`** · Finder nomme `Locations` la section de sa barre
  latérale qui contient disques et serveurs : `LocalizableMerged.strings` clé `SD5` → `Emplacements` (macOS 26.6.2 build
  25G83, 2026-09-06) · `high`. Mot volontairement générique : aujourd'hui des partages SMB, demain les compartiments
  d'un compte de stockage. Ne pas rétrécir en `Partages`.
- **Status (l'en-tête de colonne) → `État`** · Feedback Assistant `CommonStrings.loctable` (`STATUS_SECTION_TITLE` →
  `État`), Réglages Bluetooth (`Status: %@` → `État : %@`), et le catalogue lui-même (`fileExplorer.columns.gitTitle` →
  `État Git`) · `high`. L'accent sur la capitale est obligatoire.
- **Address (l'en-tête de colonne) → `Adresse`** · Finder `ConnectToWindow.strings` fr (`YEA-3L-WnW.placeholderString` →
  `Adresse du serveur`), ControlCenter `WiFi.loctable` / `Bluetooth.loctable` (`Address: %@` → `Adresse : %@`) · `high`.
- **Last used (l'en-tête de colonne) → `Dernière utilisation`** · `Security.prefPane/Localizable.loctable`, clé
  `Last Used` → `Dernière utilisation` (macOS 26.6.2 build 25G83, 2026-09-06) · `high`. Deux mots, comme l'anglais.
- **Never (la valeur de cette colonne) → `Jamais`** · même paquet, clé `Never` → `Jamais` · `high`.
- **Type (l'en-tête de colonne du protocole) → `Type`, identique à l'anglais** · Finder rend la colonne `Kind` par
  `Type` (`LocalizableMerged.strings`, `N224` et `N169.35`), et les panneaux de détail de profil d'Apple livrent `Type`
  → `Type` (`ManagedClient.app`, `mcx` / `iChat` `profileDomainPlugin`, `str_Detail_globalproxy_Type`) · `high`. D'où la
  `sameAsSourceJustification` sur cette clé : c'est le mot français, pas un oubli de traduction.
- **Connected (l'état) → `Connecté`** · `Security.prefPane/Localizable.loctable` et ControlCenter `Bluetooth.loctable`
  (`CONNECTED` → `Connecté`) · `high`. L'accord se fait sur `le serveur` (masculin), jamais sur la personne : la règle
  de genre est respectée sans restructuration.
- **Saved (l'état d'un serveur enregistré mais inactif) → `Enregistré`** · le voisin déjà livré
  `fileExplorer.navigation.connectionTooltipSaved` (`Enregistré. Ouvrez-le pour vous connecter.`) · `high`. Rien ne va
  mal : pas de `Inactif` ni de `Hors ligne`, qui se liraient comme un verdict.
- **Found nearby (l'état) → `Découvert à proximité`** · `IOBluetoothUI.framework/Localizable.loctable`,
  `PROX_PAIRING_OPTIONS_HEADER%@` : `“%@” discovered nearby` → `« %@ » découvert à proximité`, et `GROUP_FOUND_DEVICES`
  → `Appareils à proximité` (macOS 26.6.2 build 25G83, 2026-09-06) · `high`. Cohérent avec la découverte réseau déjà
  livrée (`settings.network.firstTriggerDone.label` → `Découverte réseau lancée`).
- **Waiting for you to check the key → `En attente de votre vérification de la clé`** · moule d'état sanctionné par le
  style guide (`En attente d''une réponse de la destination`), et `vérifier` est le verbe du catalogue pour contrôler
  quelque chose (`Vérifiez que le partage est accessible`) · `high`. À distinguer de `examiner`, réservé au
  `look at the key` du voisin `fileExplorer.navigation.connectionTooltipNeedsHostKey`. `votre` garde l'adresse directe
  sans accord de genre.
- **local network discovery → `la découverte du réseau local`** · `découverte` vient du catalogue
  (`settings.network.firstTriggerDone.label`), `réseau local` est le nom qu'Apple donne à l'autorisation de
  confidentialité et que le catalogue reprend (`settings.network.permissionWithout`, `Réseau local`) · `high`.
- **pin / unpin (un serveur) → `épingler` / `désépingler`** · le catalogue les a déjà pour les onglets
  (`menu.tab.pinTab` → `Épingler l''onglet`, `menu.tab.unpinTab` → `Désépingler l''onglet`,
  `commands.tabTogglePin.label` → `Épingler ou désépingler l''onglet`) · `high`. ❗ AppKit dit
  `Ne plus épingler l'onglet` (`MenuCommands.loctable`, `Unpin Tab`) : ne le reprenez PAS, les trois clés déjà livrées
  font foi et une divergence se compterait dans `desktop-i18n-term-consistency`.
- **volume switcher → `le sélecteur de volume`** (singulier `volume`) · fixé par les clés livrées
  `commands.paneLeftVolumeChooser.label` (`Ouvrir le sélecteur de volume gauche`), `commands.volumeClose.label`
  (`Fermer le sélecteur de volume`), `shortcuts.scope.volumeChooser` (`Sélecteur de volume`) · `high`.

Notes de formulation :

- **`Pin / unpin server` perd la barre oblique.** La commande devient `Épingler ou désépingler le serveur`, mot pour mot
  la structure de `commands.tabTogglePin.label` déjà livrée. Les deux commandes se lisent côte à côte dans la palette ;
  une barre oblique là où sa jumelle écrit `ou` se lirait comme deux commandes différentes.
- **`Forget saved password` reprend le libellé de menu MOT POUR MOT** : `Oublier le mot de passe enregistré`, identique
  à `menu.network.forgetSavedPassword` et à `fileExplorer.navigation.forgetSecretConfirmTitle`.
- **`Disconnect server` garde le pronominal du catalogue** : `Se déconnecter du serveur`, construit sur
  `menu.network.disconnect` (`Se déconnecter`) et `fileExplorer.navigation.disconnectPlaceAriaLabel`
  (`Se déconnecter de {name}`). Jamais `Éjecter` : un serveur n'a rien à débrancher.
- **Les deux notifications d'épinglage nomment l'objet, jamais la personne.** `It''s still saved.` devient
  `Le serveur reste enregistré.` plutôt qu'un `Il est toujours enregistré` qui accorderait un participe sur un `{name}`
  de genre inconnu. Même logique pour `Cmdr n''a pas pu changer l''endroit où {name} s''affiche.` : `s''affiche` est un
  verbe, il ne s'accorde pas.
- **`Turn it on in Settings` → `Activez-la dans les Réglages`.** Le `la` reprend `la découverte du réseau local` de la
  ligne précédente ; `les Réglages` est le nom de la fenêtre de réglages de Cmdr en français
  (`commands.appSettings.label` → `Ouvrir les réglages`, `driveIndex.tooltipIndexingOff` →
  `désactivée dans les Réglages`), pas `Réglages Système` qui nomme l'app d'Apple.
- **L'état vide suit `askCmdr.sessions.empty`** (`No chats yet` → `Pas encore de conversation`) : `No servers yet`
  devient `Pas encore de serveur`, au singulier comme son modèle.
- **Le pluriel prend les trois catégories CLDR du français** (`one` / `many` / `other`), comme tout le set `fr`
  (`operationLog.summary.*`, `queue.chip.tooltip`).
- **Les `…` restent le caractère unique U+2026** là où l'anglais l'emploie (`Modifier le serveur…`,
  `Ajouter un serveur…`) : la note « trois points » du style guide vise les clés dont la source anglaise écrit `...`.
- Une seule valeur est identique à l'anglais (`servers.hub.colType`), et elle porte sa `sameAsSourceJustification`.
  Toutes les apostrophes sont ASCII et doublées (`n''a`, `l''endroit`, `s''affiche`).

## Le hub des serveurs : la feuille de connexion et la clé d'hôte SSH (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.*`, `goToPath.dialog.opensServer`, `.addsServer`, `commands.serversConnect.label`)

La feuille modale qui ajoute, modifie, ou ré-identifie un serveur (SMB, SFTP, WebDAV), l'étape d'approbation de la clé
d'hôte SSH qui s'y insère, et deux lignes d'aperçu sous « Aller au chemin ». Le tas de références (`_ignored/i18n/fr/`)
est absent de cette machine (chemin absolu du clone principal vérifié, `~/projects-git/vdavid/cmdr/_ignored/` n'existe
pas) ; les termes viennent donc des paquets macOS installés (`plutil -convert json` sur les `.strings` / `.loctable`,
macOS 26.6.2 build 25G83, 2026-09-07) et du catalogue `fr` déjà livré.

- **Protocol (l'en-tête du sélecteur) → `Protocole`** · `AddPrinter.app/PlugIns/IP.plugin` (`IP.loctable`,
  `100257.ibExternalAccessibilityDescription`), Wireless Diagnostics (`WDWiFiScan.loctable`,
  `AYh-hx-E2Q.headerCell.title`), et System Profiler (`SPStorageReporter`, clé `protocol`) rendent tous `Protocol` par
  `Protocole` · `high`.
- **SMB / SFTP / WebDAV → verbatim** · Apple garde les sigles de protocole en français : NetAuthAgent
  `Localizable.loctable` (`SMB_PASSWORD` → `Mot de passe SMB`, `WEBDAV_PASSWORD` → `Mot de passe WebDAV`),
  NetworkSettingsIntents (`SMB` → `SMB`), Sharing.appex (`SSH_INFO_GENERAL` → « … via SSH et SFTP »), WorkflowKit («
  serveurs SMB/CIFS, NFS, FTP (lecture seule) ou WebDAV ») · `high`. Les trois options du sélecteur portent donc une
  `sameAsSourceJustification`.
- **passphrase → `phrase secrète`** · AuthenticationServices `fr.lproj/Localizable.strings`, clé `Passphrase` →
  `Phrase secrète` (macOS 26.6.2 build 25G83, 2026-09-07) · `high`. `Key passphrase` (celle qui déverrouille le FICHIER
  de clé SSH, pas le mot de passe du compte) devient `Phrase secrète de la clé`. À distinguer de `mot de passe`, que le
  catalogue réserve au compte.
- **fingerprint (l'empreinte d'une clé) → `empreinte`** · Apple rend `fingerprint` par `empreinte numérique` dans Safari
  (`fr.lproj/Localizable.strings` : `Fingerprinting defense` → `Protection contre le vol des empreintes numériques`, et
  « pour créer une « empreinte numérique » et vous identifier ») · `high`. Notre contexte est déjà celui d'une clé, donc
  `empreinte` seule suffit ; `Key fingerprint` → `Empreinte de la clé`. Le mot est féminin, ce qui rend
  `Je l''ai vérifiée` sûr côté accord.
- **to trust (une clé d'hôte) → `approuver`** · reprend le terme déjà fixé pour les certificats (§ Le hub des serveurs :
  panneau de connexion) et la valeur livrée `servers.refusal.hostKeyUntrusted`
  (`Cmdr n''a pas encore approuvé la clé de {host}.`) · `high`. ❗ Apple traduit le `Trust` NU d'un appairage d'appareil
  par `Se fier` (SecurityInterface `Localizable.loctable`, UsersGroups.appex, RemotePairingDevice, clé `Trust`) : ne le
  reprenez PAS ici, `Se fier à la nouvelle clé` divergerait de la clé déjà livrée et `desktop-i18n-term-consistency`
  compterait l'écart.
- **remote (ce qui est sur le serveur) → `distant`, postposé** · Apple construit `serveur distant` (Directory Utility
  `Localizable.loctable`, ScreenSharing, WorkflowKit) et `ordinateur distant` (AppleScript, Sharing.appex) · `high`.
  `Remote folder` → `Dossier distant`. Pas `À distance`, que System Profiler et FinderKit réservent au `Remote` employé
  seul comme valeur de colonne.
- **Connect to server… → `Se connecter au serveur…`, au verbe, sur toutes les surfaces** · c'est le point du menu Aller
  de macOS `fr`, mot pour mot ; les deux clés qui portent la phrase (`commands.serversConnect.label`,
  `settings.network.permissionIntroConnectLink`) sont des actions, donc l'infinitif · `high`. Le nom
  `Connexion au serveur` que Finder donne à SA fenêtre ne s'emploie pas ici : nos titres de feuille nomment ce qu'on
  fait (`Ajouter un serveur`, `S''identifier sur {name}`).
- **Browse… (le bouton qui ouvre le sélecteur de fichiers système) → `Parcourir…`** · Finder
  `fr.lproj/ConnectToWindow.strings`, clé `48.title` : `Browse` → `Parcourir`, dans la fenêtre « Connexion au serveur »
  elle-même (macOS 26.6.2 build 25G83, 2026-09-07) · `high`. Même mot que `settings.archives.opt.browse`, déjà livré.
- **key file (le fichier de clé privée SSH) → `Fichier de clé`** · construit sur `clé`, le terme du catalogue pour une
  clé (`ai.secretError.*`, `servers.refusal.hostKeyUntrusted`) · `high`. Pas `Fichier de clé privée` : l'anglais ne dit
  pas `private`, et le champ vit déjà sous « Avancé ».
- **ssh, ssh-agent, Nextcloud → verbatim** · noms de commande et de produit ; `ssh` reste en minuscules comme dans la
  source · `high`.

Notes de formulation :

- **Les titres de la feuille sont des infinitifs**, comme tous les libellés d'action du set `fr` (style guide §
  Formality) : `Ajouter un serveur`, `Modifier {name}`, `S''identifier sur {name}`. Le verbe `s''identifier` vient du
  `terms.json` et de la clé livrée `fileExplorer.network.signIn` (`S''identifier`) ; `se connecter` reste réservé à
  l'action réseau (`fileExplorer.network.connect` → `Se connecter`).
- **Trois libellés reprennent MOT POUR MOT une clé livrée AILLEURS**, sinon `desktop-i18n-term-consistency` compte une
  divergence : `Se connecter` (`fileExplorer.network.connect`), `S''identifier` (`fileExplorer.network.signIn`),
  `Avancé` (`settings.section.advanced`). Deux autres n'existent que sur la feuille et fixent donc leur formulation ici
  : `Mémoriser dans le trousseau` (`servers.sheet.remember`) et `Se connecter en tant qu''invité`
  (`servers.sheet.connectAsGuest`).
- **`Sign in with a username and password` s'écrit en toutes lettres** : `servers.sheet.signInWithCredentials` →
  `S''identifier avec un nom d''utilisateur et un mot de passe`. Pas d'abrégé en `identifiants`, qui en français désigne
  aussi bien le seul nom d'utilisateur que le couple entier ; la source nomme les deux champs, le français les nomme
  aussi (`servers.sheet.username` → `Nom d''utilisateur`, `servers.sheet.password` → `Mot de passe`).
- **`How to connect` pose une question, ce n'est pas un nom de champ** : `servers.sheet.connectionModeLegend` →
  `Comment se connecter`. Pas `Mode de connexion`, qui nommerait un réglage alors que la légende demande de choisir.
- **La citation d'un libellé de case à cocher garde les guillemets français** avec espace intérieure :
  `Activez « Mémoriser dans le trousseau » et identifiez-vous une fois.` (style guide § Notes).
- **`Signed out of {name}` nomme la session, pas la personne** : `Session fermée sur {name}`, sur le moule de
  `servers.hub.status.signedOut` (`Session fermée`) et de `connectionTooltipNeedsSignIn`. Un participe accordé au sujet
  donnerait `Déconnecté(e)`, que la règle de genre interdit.
- **`Cmdr stopped connecting to {name}` → `Cmdr a interrompu la connexion à {name}`.** L'anglais dit un arrêt délibéré,
  pas un échec, donc ni le moule `Cmdr n''a pas pu…` ni `erreur` / `échec` (bannis). `interrompre` nomme l'observation
  et laisse la suite au corps du message.
- **`I''ve checked it` → `Je l''ai vérifiée`** : première personne, l'utilisateur qui parle. L'accord porte sur
  `l''empreinte` (féminin) affichée juste au-dessus, jamais sur la personne. `vérifier` est le verbe du catalogue pour
  contrôler quelque chose (`servers.hub.status.waitingForKey` → `En attente de votre vérification de la clé`) ; il se
  distingue d'`examiner`, réservé au `look at the key` de `connectionTooltipNeedsHostKey`.
- **`nas.local` reste tel quel** : `NAS` est le même sigle en français (`servers.hub.emptyMessage` écrit déjà « un Mac
  ou un NAS ») et `.local` est le suffixe mDNS réservé. Traduire l'exemple afficherait une adresse qui ne résout pas.
  D'où sa `sameAsSourceJustification`.
- **Les deux lignes d'aperçu de « Aller au chemin » restent à la 3e personne du présent**, comme la source :
  `Ouvre {name}`, `Ajoute un serveur`. Pas d'infinitif ici : ce ne sont pas des libellés d'action mais la description de
  ce que fera la touche Entrée.
- **Les `…` suivent la source caractère pour caractère** : U+2026 dans `Connexion…`, `Parcourir…`, `S''identifier…`,
  `Se connecter au serveur…`.
- Quatre valeurs sont identiques à l'anglais (`servers.sheet.protocolSmb`, `protocolSftp`, `protocolWebdav`,
  `addressPlaceholder`) et portent chacune leur `sameAsSourceJustification`. Toutes les apostrophes sont ASCII et
  doublées (`S''identifier`, `d''hôte`, `qu''invité`, `l''empreinte`, `s''est`, `d''approuver`).

## Le panneau de reconnexion et la session fermée sans mot de passe à saisir (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Le tas de références (`_ignored/i18n/fr/`) est absent de cette machine ; tout ce qui suit est miné directement dans les
paquets macOS installés (macOS 26.6.2 build 25G83, `plutil -convert json` sur les `.loctable` / `.strings`, 2026-09-07),
selon la recette de repli documentée dans `docs/i18n/reference-pile/how-to-mine.md`.

Termes :

- **to reconnect → `reconnecter` ; Reconnecting… (titre d'état) → `Reconnexion…`** · HomeDataModel
  `HFLocalizable.loctable`, clé `HFServiceDescriptionReconnecting` : `Reconnecting…` → `Reconnexion…` ; même paquet,
  `Reconnect HomePod to “%@”` → `Reconnecter le HomePod à « %@ »`, qui atteste aussi la préposition `à` ; Finder
  `fr.lproj/LocalizableMerged.strings`, clé `NE111.1` : `reconnect “^0”` → `reconnecter « ^0 »` ; AirPort.menu et
  WiFiAgent `Localizable.loctable`, clé `kAirPortBaseStationPPPStatusReconnecting` : `PPPoE Reconnecting` →
  `PPPoE en cours de reconnexion` · `high`. Le nom `reconnexion` était déjà livré dans
  `servers.paneState.retryProgressAriaLabel` (« Temps avant la prochaine tentative de reconnexion ») ; la source le
  confirme.
- **rather than / instead of (contraste entre deux moyens) → `plutôt que`** · WiFiSettingsKit `Localizable.loctable` :
  `Use “%@” Wi-Fi instead of cellular?` → `Utiliser le réseau Wi-Fi « %@ » plutôt que les données cellulaires ?`,
  `Prefer 5G over Wi-Fi` → `Utiliser la 5G plutôt que le Wi-Fi` · `high`. Le catalogue l'employait déjà une fois
  (`fileExplorer` : « une nouvelle analyse est en cours plutôt que de rejouer les changements un à un »).
- **to type / to enter (une valeur dans un champ) → `saisir`** · Finder `fr.lproj/LocalizableMerged.strings`, série
  `Enter the name of…` → `Saisissez le nom de…` et `enter the name and password for an administrator` →
  `saisir le nom et le mot de passe d''un administrateur` · `high`. Déjà livré dans
  `fileExplorer.network.browser.tooltip.requiresLogin` (« Cet hôte demande une connexion. Double-cliquez pour saisir vos
  identifiants. »).
- **SSH key → `clé SSH` ; public key → `clé publique` ; private key → `clé privée`** · ActionKitUI
  `Localizable.loctable` (`SSH Key` → `Clé SSH`, `No SSH Key` → `Aucune clé SSH`, `Copy Public Key` →
  `Copier la clé publique`, `Replace SSH Key` → `Remplacer la clé SSH`) et ActionKit `Localizable.loctable`
  (`Private key is not a valid RSA private key.` → `La clé privée n''est pas une clé privée RSA valide.`) · `high`.
  Cohérent avec `clé` déjà fixé pour la clé d'hôte (§ Le hub des serveurs : la feuille de connexion) et avec
  `servers.sheet` § `key file` → `Fichier de clé`.

Notes de formulation :

- **`Reconnecting to {name}…` → `Reconnexion à {name}…`**, sur le moule exact du voisin `servers.paneState.connecting`
  (`Connexion à {name}…`) et de Finder (« Connexion au serveur »). La paire se lit comme un seul état en deux temps ;
  n'introduisez pas `En cours de reconnexion vers…`, plus long et hors moule. `à` ne s'élide pas, donc `{name}` peut
  commencer par n'importe quel caractère.
- **`This server signs in with a key…` ne prend PAS `Ce serveur s''identifie…`.** Dans ce même fichier, le serveur qui «
  s'identifie » est le sens de la clé d'hôte (`servers.paneState.hostKeyChanged*`, où c'est bien le serveur qui prouve
  son identité). Ici c'est la personne qui est identifiée, par sa clé SSH, donc le serveur est l'agent :
  `Ce serveur vous identifie par une clé plutôt que par un mot de passe`. Le verbe reste actif, comme le veut le style
  guide, et rien ne s'accorde sur la personne.
- **La deuxième proposition reprend le moule `…, il n''y a donc rien à <verbe>.`**, déjà livré deux fois dans
  `errors.eject.*` (« Ce disque n'est plus connecté, il n'y a donc rien à éjecter. », « Ce n'est pas un partage réseau,
  il n'y a donc rien à déconnecter. »). Même forme ici avec `saisir`. Pas de « Désolé », pas d'« erreur » : c'est un
  constat, pas une faute.
- **`Open it again to retry.` → `Ouvrez-le à nouveau pour réessayer.`**, mot pour mot la fin du voisin
  `servers.paneState.hostKeyChangedHint` (« … ouvrez-le à nouveau pour vérifier l''empreinte. »). `le` reprend
  `Ce serveur` (masculin), sûr côté accord. `réessayer` est le terme du `terms.json`.
- Le `…` de `reconnecting` est U+2026, comme la source anglaise et comme `Connexion à {name}…`. Toutes les apostrophes
  sont ASCII et doublées (`n''y`).

## Épingler un serveur au sélecteur, les clés d'hôte approuvées et la ligne ADB des réglages (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.appearance.tintSmb.*`)

Trois surfaces : les deux éléments de menu contextuel qui épinglent ou retirent un serveur du sélecteur de volume plus
la notification ponctuelle qui les explique ; la page Réglages > Systèmes de fichiers > Serveurs, qui liste les clés
d'hôte SSH approuvées ; et la page Android (ADB), qui dit où se trouve la commande `adb`.

Le tas de références (`_ignored/i18n/fr/`) est absent de cette machine (`~/projects-git/vdavid/cmdr/_ignored/` n'existe
pas, chemin absolu du clone principal vérifié). Repli documenté : `docs/i18n/reference-pile/how-to-mine.md` § "No pile
on this machine?" décrit ce repli. Minage direct des paquets macOS installés, `plutil -convert json` sur les
`.loctable`, macOS 26.6.2 build 25G83, 2026-09-07. Termes :

- **not found (une commande absente) → `Introuvable`** · AppKit `FindPanel.loctable`, clé `Not found` → `Introuvable` ;
  PhotosGraph `Localizable.loctable`, clé `PGErrorFormatNotFound` → `Introuvable` ; PrintCore `cups.loctable`,
  `Not Found` → `Introuvable` · `high`. Reprend le `not found → introuvable` déjà fixé au § passe `errors`.
- **found at {path} (la valeur d'état quand la commande existe) → `Trouvé : {path}`** · Apple rend ce genre de valeur
  d'état par `<mot> : %@` : PassKit `Localizable.loctable`, `SEARCH_PASS_ADDED` (`Added %@` → `Ajout : %@`),
  SpotlightServices `SpotlightServices.loctable`, `DATE_MODIFIED_FORMAT` (`Modified %@` → `Modification : %@`) · `high`.
  La paire `Trouvé` / `Introuvable` partage sa racine, exactement comme `Found` / `Not found` en anglais. Espace ASCII
  avant le deux-points, comme tout le set `fr`.
- **Re-check (le bouton qui relance la recherche de `adb`) → `Rechercher à nouveau`** · le verbe vient d'Apple, qui rend
  `Check for X` par `Rechercher X` (AMPDevices `Localizable.loctable`, clé `6bm5j9gkkw` : `Check for Update` →
  `Rechercher les mises à jour`) ; la reprise vient de PassKit `Localizable.loctable`,
  `IDENTITY_VERIFICATION_ID_SCAN_AGAIN_BUTTON_TITLE` (`Scan Again` → `Scanner à nouveau`) · `high`. ❗ Pas `Vérifier` :
  le catalogue a déjà écarté ce verbe pour `Check for updates` (§ `Check for updates`). ⚠️ Trois mots pour un bouton
  étroit : à repasser au pseudolocale `en-XA`.
- **trusted (une clé approuvée) → `approuvé` / `approuvée`** · confirme le `to trust → approuver` déjà fixé (§ Le hub
  des serveurs : panneau de connexion) : PrintCore `cups.loctable` (`untrusted certificate` →
  `certificat non approuvé`), Security `SecErrorMessages.loctable` clé `-66996` (`signer is not trusted` →
  `le signataire n''est pas approuvé`) · `high`. L'accord porte sur `la clé` (féminin), jamais sur la personne.
- **`Trusted <date>` (l'étiquette juste avant la date) → `Approuvée le`** · Apple met la date derrière une préposition
  ou un deux-points, jamais collée au participe : PaperKit et AnnotationKit `AKSignature.loctable` (`Created %@` →
  `Création le %@`), PassKit `LAST_UPDATED_FORMAT` (`Updated %@` → `Mise à jour : %@`) · `high`. `le` est obligatoire en
  français ; `Approuvée 2026-09-07` serait agrammatical.
- **fingerprint (l'empreinte d'une clé) → `empreinte`** · source plus proche que le Safari cité au § feuille de
  connexion : Security `Certificate.loctable` et SecurityFoundation `Certificate.loctable`, clé `Fingerprints` →
  `Empreintes`, et Security `Trust.loctable` (`Anchor does not match pinned fingerprint` →
  `Le point d''ancrage ne correspond pas à l''empreinte épinglée.`) · `high`. Féminin, donc `l''approuvez` / `vérifiée`
  s'accordent sans risque.
- **plugged in (un téléphone branché en USB) → `branché`** · Setup Assistant `Localizable.loctable`
  (`Make sure your Mac is plugged in…` → `Assurez-vous que votre Mac est branché…`), AMPDevices
  (`is connected to a low-speed USB 1.1 port` → `est branché sur un port USB 1.1 à faible vitesse`) · `high`.
- **Choose the adb command → `Choisir la commande adb`** · moule `Choose X…` → `Choisir X…` d'Apple (clé `N137` du
  Finder, `Choose Application…` → `Choisir une application…`), déjà repris par `settings.behavior.openTerminalHereApp`
  (§ « Ouvrir un terminal ici ») · `high`. `adb` reste en minuscules : c'est le nom de la commande.

Notes de formulation :

- **`Pin to switcher` raccourcit `sélecteur de volume` en `sélecteur`, exactement comme l'anglais.** Le terme complet
  reste `le sélecteur de volume` (§ Le hub des serveurs : la table), mais l'élément de menu vit dans une liste
  déroulante étroite et l'anglais y écrit `switcher` tout court : `Épingler au sélecteur`. Le verbe est celui déjà fixé
  pour les onglets et les serveurs (`commands.serversTogglePin.label` → `Épingler ou désépingler le serveur`).
- **`Unpin` → `Désépingler`**, jamais `Ne plus épingler` (la forme AppKit) ni `Retirer` : `retirer` se lirait comme une
  suppression, et le serveur reste enregistré. Les trois clés livrées (`menu.tab.unpinTab`, `commands.tabTogglePin`,
  `commands.serversTogglePin`) font foi.
- **Le corps de la notification nomme le serveur, jamais un pronom accordé** : `It stays in the Servers list.` devient
  `Le serveur reste dans la liste Serveurs.`, sur le moule déjà fixé (`It''s still saved.` →
  `Le serveur reste enregistré.`). `Serveurs` en capitale nomme la rangée du sélecteur, comme `servers.hub.*`.
- **Le nom de commande inséré prend les guillemets français** avec espace intérieure :
  `utilisez « {command} » dans la palette de commandes` (style guide § Notes ; la source anglaise met déjà des
  guillemets droits).
- **`Got it` → `D''accord`**, mot pour mot les trois clés livrées (`ai.toast.gotIt`,
  `updates.moveToApplicationsDialog.gotIt`, `main.oldMacos.gotIt`), sinon `desktop-i18n-term-consistency` compte une
  divergence.
- **`Your Network group is getting long` → `Votre groupe Réseau s''allonge`.** `Réseau` reprend mot pour mot l'en-tête
  du sélecteur (`fileExplorer.navigation.groupNetwork`). Constat, pas avertissement : ni `trop long`, ni
  `commence à être`.
- **L'état vide de la page des clés suit `askCmdr.sessions.empty`** : `Nothing trusted yet.` →
  `Pas encore de clé approuvée.`, comme `Pas encore de serveur` et `Pas encore de conversation`.
- **`and asks` a besoin d'un objet en français** : `vous demande votre accord`. `demander` seul resterait suspendu.
- **`Watching for phones.` ne nomme ni l'abonnement ni le serveur ADB** :
  `Cmdr détecte un téléphone dès qu''il est branché.` / `Cmdr ne détecte pas les téléphones pour l''instant.` Voix
  active, verbe `détecter` déjà employé par les voisins MTP (`settings.summary.mtp` → `Détecter les appareils Android…`,
  `mtpEnabled.description` → `Détecte et se connecte…`), et `branché` sourcé chez Apple. Pas de
  `Surveillance des téléphones`, qui se lirait comme un module technique.
- **Le champ vide reprend MOT POUR MOT la description livrée** : `Chercher adb de la manière habituelle` calque
  `settings.fileOperations.adbBinaryPath.description` (« … Cmdr cherche adb de la manière habituelle … »).
- **`then press Re-check:` devient `puis cliquez sur Rechercher à nouveau :`**, sans guillemets : l'anglais n'en met pas
  et Apple écrit la même phrase nue (AMPDevices, clé `rr4pkgnu56` :
  `Cliquez sur Rechercher les mises à jour pour vérifier…`). Espace ASCII avant le deux-points.
- **`Android platform tools` garde sa capitale de produit** : `les Android Platform Tools`, comme
  `settings.fileOperations.adbEnabled.description` déjà livrée (`terms.json`, style guide).
- **La teinte des panneaux couvre maintenant trois protocoles.** `settings.appearance.tintSmb.*` ne dit plus
  `panneaux SMB/réseau` : le libellé devient `Teinter les panneaux de serveur (SMB, SFTP, WebDAV)` et la description
  calque la structure de sa sœur `tintMtp.description` (`Teinte de fond appliquée aux panneaux affichant …`).
- **Une seule valeur est identique à l'anglais** (`settings.section.adb`, « Android (ADB) ») et porte sa
  `sameAsSourceJustification` : Apple n'a aucun équivalent (un grep de `Android` sur tous les `.loctable` du système ne
  renvoie rien sur macOS 26.6.2 build 25G83), et `terms.json` fixe déjà les deux mots comme verbatim.
- Toutes les apostrophes des valeurs ICU sont ASCII et doublées (`s''allonge`, `D''accord`, `l''astuce`, `d''hôte`,
  `l''empreinte`, `l''approuvez`, `qu''il`, `l''instant`). Les deux clés `menu.*` sont RAW et n'en contiennent aucune.

## Le téléphone Android : le panneau de connexion, les info-bulles et l'astuce ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Trois surfaces : le panneau plein qui remplace la liste des fichiers quand l'ouverture d'un téléphone Android s'arrête
(`adb.connect.*`, même voix que `servers.refusal.*` / `servers.paneState.*`), les info-bulles de la rangée du téléphone
dans le sélecteur de volume (`adb.readiness.*`, `adb.disconnect*`), et la ligne discrète en haut d'un panneau MTP qui
propose le débogage USB (`adb.hint.*`). Les deux clés `settings.behavior.adbHintDismissed.*` sont internes (un drapeau «
déjà vu ») et calquent leur sœur `settings.behavior.serversPinHintSeen.*`.

Le tas de références (`_ignored/i18n/fr/`) est absent de cette machine (`~/projects-git/vdavid/cmdr/_ignored/` n'existe
pas, chemin absolu du clone principal vérifié). Deux replis, tous deux documentés par
`docs/i18n/reference-pile/how-to-mine.md` § No pile on this machine? Mine the live macOS bundles instead : minage direct
des paquets macOS installés (`plutil -convert json` sur les `.loctable`, macOS 26.6.2 build 25G83, 2026-09-07), et, pour
le vocabulaire propre à Android, les chaînes traduites d'AOSP (`android.googlesource.com`, branche `main`, récupérées le
2026-09-07).

Termes :

- **`Allow` (le bouton du dialogue Android) → `Autoriser`** · AOSP SystemUI `res/values-fr/strings.xml`,
  `usb_debugging_allow` (`Allow` → `Autoriser`) et `usb_debugging_title` (`Allow USB debugging?` →
  `Autoriser le débogage USB ?`) ; confirmé quatre fois par Settings `res/values-fr/` (`allow`,
  `accessibility_dialog_button_allow`, `wifi_scan_always_confirm_allow`, `request_manage_credentials_allow`) · `high`.
  C'est le mot exact que la personne lit sur son téléphone, donc jamais `Accepter` ni `Approuver` (ce dernier est
  réservé aux clés d'hôte SSH, § Le hub des serveurs).
- **`tap <bouton>` → `appuyez sur <bouton>`** · AOSP Settings `service_manage_description`
  (`Tap Settings to control it.` → `Appuyez sur "Paramètres" pour contrôler ce client.`) et `tap_to_wake_summary`
  (`Double-tap anywhere on the screen…` → `Appuyez deux fois n'importe où sur l'écran…`) · `high`. Sans guillemets
  autour du nom du bouton : la source anglaise n'en met pas, et le catalogue a déjà fixé cette règle pour
  `settings.adb.install.intro` (§ Épingler un serveur au sélecteur).
- **`USB debugging` → `débogage USB`** · confirmé à la source cette fois : AOSP SettingsLib `res/values-fr/strings.xml`,
  `enable_adb` (`USB debugging` → `Débogage USB`), `enable_adb_summary` (`Mode de débogage en connexion USB`),
  `clear_adb_keys` (`Revoke USB debugging authorizations` → `Révoquer les autorisations de débogage USB`) · `high`.
  Majuscule uniquement en tête de phrase ou comme libellé d'interrupteur ; `USB` toujours en capitales. Le style guide
  citait la documentation Google ; ces trois clés sont l'interface elle-même.
- **`allow USB debugging` (l'attente, côté Cmdr) → `votre autorisation de débogage USB`** · le nom vient mot pour mot de
  `clear_adb_keys` ci-dessus (`les autorisations de débogage USB`) ; le moule `En attente de …` est déjà celui du
  catalogue (style guide § Voix : « En attente d'une réponse de la destination ») · `high`. `votre` porte le « you » de
  l'anglais sans nommer la personne, donc aucun accord de genre.
- **`wake <the> screen` (d'un téléphone) → `activez son écran`** · AOSP Settings `ambient_display_wake_screen_title`
  (`Wake up display` → `Activer l'écran`) et `doze_title` (`Wake screen for notifications` →
  `Activer l'écran si notifications`) · `high`. Android rend le réveil de l'écran par `activer`, pas par `réveiller` ;
  Apple n'a pas ce sens (un balayage complet des `.loctable` ne donne `réveiller` que pour le sommeil humain et les
  réveils de l'app Horloge). Le sens « réveiller l'appareil » d'Android est `réactiver` (`tap_to_wake` →
  `Appuyer pour réactiver`), qui ne convient pas ici : c'est l'écran qui est visé.
- **`reseat the cable` → `rebranchez le câble`** · Apple AMPDevices `Localizable.loctable`, clé `etkzfs9ykr`
  (`Please unplug and replug your iPod…` → `Veuillez le débrancher puis le rebrancher…`) · `high`. `rebrancher` dit à
  lui seul « débrancher puis rebrancher », donc pas besoin des deux verbes.
- **`cable` / `port` → `câble` / `port`, et `another` → `un autre`** · Apple AirPort Utility `SetupRecommendations`
  (`USB port` → `Port USB`), `AirPortSettings` `ConflictingPortTCPACP` (`Choose a different port number.` →
  `Choisissez un autre numéro de port.`), BluetoothSetupAssistant (`unplug the USB cable` → `débranchez le câble USB`) ·
  `high`. `Try another cable or port.` répète le déterminant en français : `Essayez un autre câble ou un autre port.`
- **`not responding` → `ne répond pas`** · Apple loginwindow `loginwindow.loctable`, `APP_NOT_RESPONDING_SUFFIX`
  (`(not responding)` → `(ne répond pas)`) ; CoreServicesUIAgent `LaunchErrors.loctable`, clé `-1712.message.noname`
  (`because it is not responding` → `car elle ne répond pas`) · `high`.
- **`lost the connection` → `a perdu la connexion`** · Apple CFNetwork `Localizable.loctable`, `Err-1005`
  (`The network connection was lost.` → `La connexion réseau a été perdue.`) · `high`. Cmdr garde la voix active
  (`Cmdr a perdu la connexion avec votre téléphone.`) là où Apple passive. ❗ Frontière à tenir : `perdre` nomme une
  rupture subie, `interrompre` une rupture décidée par Cmdr (`servers.paneState.hostKeyChanged` →
  `Cmdr a interrompu la connexion à {name}`).
- **`How` (le lien vers le mode d'emploi Android) → `Comment faire`** · le catalogue rend déjà `How to connect` par
  `Comment se connecter` (`servers.sheet.connectionModeLegend`) · `high`. `Comment` seul reste suspendu en français ;
  `Comment faire` est la forme minimale qui se tient. ⚠️ Deux mots là où l'anglais en a un, au bout d'une ligne déjà
  longue : à repasser au pseudolocale `en-XA`.

Notes de formulation :

- **`Disconnect {name}` reprend MOT POUR MOT la clé serveur déjà livrée** : `Se déconnecter de {name}`, comme
  `fileExplorer.navigation.disconnectPlaceAriaLabel`. Le bouton dit `Disconnect` et non `Eject` parce que rien n'est
  rendu sûr à débrancher ; le français fait la même distinction (`se déconnecter` / `éjecter`, `terms.json`), donc les
  deux surfaces s'accordent sans effort.
- **L'info-bulle grisée croise ses deux sœurs déjà livrées** : `fileExplorer.navigation.disconnectBusyTooltip` donne
  `Impossible de se déconnecter tant que des opérations sont en cours sur ce serveur`, `ejectBusyTooltip` donne
  `… sur cet appareil` ; la clé ADB prend le verbe de la première et le complément de la seconde.
- **`Open Settings` (le bouton du panneau) → `Ouvrir les réglages`**, mot pour mot les deux clés déjà livrées
  (`commands.appSettings.label`, `commands.handler.openTerminalHere.openSettings`). La majuscule de l'anglais ne change
  rien : c'est la fenêtre de Cmdr, pas l'app d'Apple (style guide § Notes).
- **`The Android tools on this Mac` reste générique** : `Les outils Android de ce Mac`, jamais
  `les Android Platform Tools`. L'anglais choisit délibérément le mot générique ici, et
  `settings.fileOperations.adbEnabled.description` livre déjà `si vous n'avez aucun outil Android installé`. La
  description interdit de nommer le programme d'arrière-plan ou le protocole : la valeur française ne dit donc ni
  `serveur adb`, ni `démon`, ni `transport`.
- **`too old for Cmdr to browse` suit le moule de `servers.refusal.authMethodUnsupported`** :
  `Ce téléphone utilise une version d'Android trop ancienne pour que Cmdr puisse le parcourir.` calque
  `Ce serveur utilise une méthode d'identification que Cmdr ne prend pas encore en charge.` Le sujet est le téléphone,
  donc `le parcourir` s'y rattache sans ambiguïté ; on ne parcourt pas une version. `ancienne`, pas `vieille`, comme
  `main.oldWebkit.body` (`une version plus ancienne`).
- **`as soon as you do` → `dès que c'est fait`** : la subordonnée anglaise reprend le verbe de la phrase précédente
  (`tap Allow`), ce que le français ne peut pas faire. `dès que` est déjà le connecteur de
  `settings.adb.status.watching` (`dès qu'il est branché`). Rassurance, pas consigne : pas d'impératif.
- **`Check your phone` → `Regardez votre téléphone`** : la personne doit lever les yeux vers un écran qui affiche déjà
  une question. `Vérifiez` (le moule Apple, BluetoothSetupAssistant `kPair_Device_InstructionTextKey`) suggère une
  inspection ; `Consultez` sonne administratif. `Regardez` est plus court et plus juste. `tentative` sur le seul choix
  du verbe d'ouverture ; `appuyez sur Autoriser` est, lui, sourcé.
- **`Want the whole filesystem?` → `Besoin de tout le système de fichiers ?`** : tournure elliptique, comme l'anglais.
  `système de fichiers` est le terme fixé (`terms.json`) et `tout le` reprend `l'ensemble du système de fichiers` de
  `settings.fileOperations.adbEnabled.description`. Espace ASCII avant le `?`, comme tout le set `fr`.
- **Le drapeau interne calque sa sœur `serversPinHintSeen`** : `Astuce sur le débogage USB ignorée` /
  `Indique si la ligne unique proposant le débogage USB a été ignorée.` — même structure `Indique si … a été …`, et
  `ignorée` parce que la ligne se ferme d'un `Dismiss` (rendu `Ignorer` par onze clés du catalogue), là où l'astuce des
  serveurs se contentait d'être `affichée`.
- **`adb.volumeLabelWithSuffix` garde sa `sameAsSourceJustification`** d'une passe précédente : un espace réservé plus
  l'acronyme `ADB`, rien à traduire.
- Toutes les apostrophes des valeurs sont ASCII et doublées (`n''a`, `n''ont`, `n''est`, `d''Android`, `c''est`) : les
  deux fichiers sont ICU.
- **`You stopped opening your phone.` → `Vous avez arrêté l''ouverture de votre téléphone.`** (`adb.connect.cancelled`)
  · `arrêter`, comme la clé parallèle `search.coverage.walk.cancelled` (« You stopped this search » →
  `Vous avez arrêté cette recherche`) · `high`. ❌ Pas `annulé` : `Annuler` est le LIBELLÉ du bouton
  (`fileOperations.button.cancel`), et le français rend déjà `Undo` par le même mot, donc la phrase deviendrait ambiguë.
  Le nom déverbal `l''ouverture` porte le gérondif anglais ; `ouvrir` est le verbe fixé pour ouvrir un téléphone
  (`adb.connect.waitingHint`, `Cmdr ouvre votre téléphone…`). Apostrophe ASCII doublée, le fichier est ICU.

## L'identité verrouillée du serveur (`servers.sheet.identityLocked`)

Les deux lignes sous les champs grisés `Adresse` et `Nom d'utilisateur`, quand l'utilisateur MODIFIE un serveur
enregistré.

- **`the account` (le champ avec lequel on se connecte au serveur) → `le compte`** · le catalogue l'emploie déjà dans ce
  sens (six occurrences dans `errors.json`, deux dans `fileExplorer.json`) · `high`.
- **L'indication nomme les actions exactement comme les commandes vers lesquelles elle renvoie** : `oublier` de
  `menu.network.forgetServer` (« Oublier le serveur ») et `ajouter` de `servers.sheet.addTitle` (« Ajouter un serveur
  »). Un synonyme (« supprimer », « créer ») envoie le lecteur chercher un menu qui n'existe pas.
- **`are what name this server` → `identifient ce serveur`** · la feuille possède son propre champ `Nom`
  (`servers.sheet.name`), donc la phrase ne peut pas passer par « nommer » : on croirait qu'il s'agit de cette
  étiquette. « identifier » dit ce qui est visé (ces deux valeurs SONT le serveur) · `high`.
- **`add it again` → `ajoutez-le à nouveau`** · `à nouveau` est déjà la forme du fichier (`servers.json` l'emploie deux
  fois) · `high`.

## L'avis quand aucun mot de passe n'était enregistré (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Aucun mot de passe n''était enregistré pour {name}.`** · reprend
  `mot de passe enregistré` et le `pour {name}` des trois clés sœurs déjà livrées (`menu.network.forgetSavedPassword` et
  `fileExplorer.navigation.forgetSecretConfirmTitle` = « Oublier le mot de passe enregistré », `.forgetSecretConfirm`,
  `.forgetSecretRefusedToast`) · `high`.
- **L'imparfait porte le constat** que demande le `@key` (rien n'a échoué, il n'y avait rien à faire), donc pas d'excuse
  et pas de « impossible ». `enregistré` s'accorde avec `mot de passe`, jamais avec `{name}`, qui reste dans une
  position neutre derrière `pour`.
- **Apostrophe doublée** (`n''était`) : `fileExplorer.json` passe par ICU, contrairement aux familles brutes.
- La pile de référence était absente de cette machine (`_ignored/i18n/` n'existe pas non plus dans le clone principal) ;
  la décision s'appuie donc sur le catalogue déjà livré et sur `terms.json`.

## La durée des tentatives, le titre de clé d'hôte et le bouton Autoriser d'Android (`servers.paneState.retryTotalSeconds`, `.retryTotalMinutes`, `servers.paneState.hostKeyChanged`, `adb.readiness.waitingForAuthorization`)

- **`{seconds}`/`{minutes}` portent désormais un pluriel ICU à DEUX paramètres** (`servers.paneState.retryTotalSeconds`,
  `.retryTotalMinutes`) : `{seconds}` ne sert qu'à choisir la branche, ce qui s'affiche est `{secondsText}`, le nombre
  déjà formaté. Le français exige `one`, `many` et `other` (CLDR, § style.md), donc les trois branches sont écrites même
  quand `many` et `other` sont identiques. Les formes reprennent `main.quit.countdown` (`{secondsText} seconde(s)`) et
  `indexing.eta.hoursMinutesLeft` (`{minutesText} minute(s)`) · `high`.
- **`retryTotalMinutes` garde un `sameAsSourceJustification`, réaccordé sur la nouvelle valeur anglaise** : « minute »
  et « minutes » s'écrivent pareil dans les deux langues et il n'existe pas d'autre mot français. La valeur EST traduite
  (elle porte les catégories françaises), mais `desktop-i18n-coverage` la lit comme de l'anglais parce que seul le jeu
  de catégories change. La clé sœur des secondes diverge (`seconde` / `secondes`), donc elle, n'a pas besoin de
  justification.
- **Les deux valeurs sont des morceaux de `servers.paneState.retryKeepsTrying`** (« Cmdr continuera d'essayer pendant
  {duration} au total. ») : elles restent nues, sans préposition ni point.
- **`Cmdr won't connect to {name}` → `Cmdr ne se connectera pas à {name}`** · même futur que la sœur
  `servers.refusal.hostKeyRevoked` (« Cmdr ne s'y connectera pas. »). L'anglais est passé d'un « stopped connecting » à
  un refus permanent ; le passé composé (« a interrompu la connexion ») laissait croire à une tentative abandonnée ·
  `high`.
- **`Allow` est le bouton d'Android → `Autoriser`**, repris mot pour mot de `adb.connect.unauthorized` (« Regardez votre
  téléphone et appuyez sur Autoriser. »), sans guillemets comme là-bas, pour que le mot lu soit celui affiché à l'écran.
  Dans `adb.readiness.waitingForAuthorization` le téléphone passe en tête (« En attente : sur votre téléphone, appuyez
  sur Autoriser ») afin d'éviter « appuyez sur Autoriser sur votre téléphone », deux `sur` collés · `high`.
- La pile de référence était absente de cette machine (`_ignored/i18n/` n'existe pas non plus dans le clone principal) ;
  la décision s'appuie donc sur le catalogue déjà livré et sur `terms.json`.

## Le menu contextuel de la ligne serveur : `Ouvrir` et `Modifier le serveur…` (`menu.network.open`, `menu.network.edit`)

- **`Open` (sur une ligne de serveur) → `Ouvrir`** (`menu.network.open`) · identique à `menu.file.open`, car c'est le
  même sens : entrer dans quelque chose, et non confier un fichier à une app. Le français ne sépare pas les deux
  acceptions, et macOS non plus : le Finder emploie le même verbe pour `Ouvrir` (`LocalizableMerged` `N151`),
  `Ouvrir avec` (`N152`) et `Ouvrir dans une nouvelle fenêtre` (`FV7`, le sens « entrer ») (Finder 26.6.2, build 25G83,
  lu le 2026-09-07) · `high`.
- **`Edit server…` → `Modifier le serveur…`** (`menu.network.edit`), copié octet pour octet depuis
  `commands.serversEdit.label` · `high`. Les deux ouvrent la même feuille ; deux libellés différents se liraient comme
  deux fonctions. Les points de suspension sont le caractère UNIQUE `…` (U+2026) et restent.
- **Les deux égalités sont vérifiées par un contrôle**, pas seulement souhaitables : `i18n-terms` signale deux clés de
  même valeur anglaise qui divergent en français. Réécrire l'une oblige à réécrire l'autre.
- **`menu.*` est une famille RAW** : Rust dessine le menu via `menu_t`, jamais `t()`. Les apostrophes restent SIMPLES et
  un `''` doublé fait échouer `i18n-icu`. Aucune des deux valeurs n'en contient.
- La pile de référence est absente de cette machine, mais `Finder.app` fournit la même preuve de niveau 1 directement
  depuis le système (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## La proposition d'ajout au Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Termes fixés (source de niveau 1 : le paquet `Dock.app` du système, `fr.lproj/DockMenus.strings`, macOS 26.6.2 build
25G83, lu le 2026-09-09 ; plus la pile `fr/macOS/`).

- Dock → **le Dock**, mot anglais, masculin, majuscule · Finder `MenuBar.json` `300772.title` et `LocalizableMerged`
  `N169.13` (« Add to Dock » → « Ajouter au Dock ») ; `Dock.app` `DOCK_SETTINGS` → « Réglages du Dock… » · high
- Finder → **le Finder**, mot anglais, masculin, toujours précédé de l'article · Finder `LocalizableMerged` (« Afficher
  dans le Finder », « Réglages du Finder », « les fenêtres du Finder ») · high
- Applications folder → **le dossier Applications**, le nom du dossier reste en anglais · AppKit `AppKitErrors.json` («
  Try dragging “%@” from the Trash to your Applications folder. » → « Essayez de faire glisser l'application « %@ » de
  la corbeille vers votre dossier Applications. ») ; Finder `TL5` / `GROUP_APPLICATIONS` → « Applications » · high
- keep in the Dock / pin to the Dock → **garder dans le Dock**, et son inverse unpin → **supprimer du Dock** ·
  `Dock.app` `DockMenus.strings` `KEEP_IN_DOCK` → « Garder dans le Dock », `REMOVE_FROM_DOCK` → « Supprimer du Dock » ·
  high. ❌ Ne pas réutiliser ici la paire `épingler` / `désépingler` du catalogue (onglets, serveurs) : elle est juste
  pour un onglet ou une ligne de serveur, mais Apple nomme l'action du Dock autrement, et l'utilisateur lit le menu du
  Dock à côté de notre notification. La paire garder/supprimer reste liée par le complément « du Dock », ce que la
  description de `unpinNote` demande.
- add to the Dock → **ajouter au Dock** · Finder `MenuBar.json` `300772.title` · high. Le bouton d'acceptation prend
  l'infinitif nu (« Oui, ajouter au Dock ») : convention des libellés d'action, et le possessif anglais « my Dock » ne
  se rend pas (le français laisse tomber le possessif dans une étiquette de bouton, comme Apple).
- configuration profile → **profil de configuration** · Réglages Système `InfoPlist.json` (« Configuration Profile » → «
  Profil de configuration ») ; confirmé par la terminologie Microsoft FRA. Apple décrit d'ailleurs le cas avec le même
  mot : `SystemSettings/Localizable.json` `MDMDisabledSettingsPane` → « Ces réglages sont contrôlés par un profil. » ·
  high
- (when you) log in → **à l'ouverture de session** · `LoginItems.appex` `Localizable.loctable`, fr (« These items will
  open automatically when you log in. » → « Ces éléments s'ouvriront automatiquement à l'ouverture de session. » ; «
  Open at Login » → « Ouvrir avec la session ») ; AppKit `Menus.json` « Log Out » → « Fermer la session ». Lu sur le
  système, macOS 26.6.2 build 25G83, 2026-09-09 · high. ❌ Pas « quand vous vous reconnectez » : « se connecter » est
  déjà pris par les serveurs dans ce catalogue.

Décisions de formulation :

- **`No, thanks` → « Non, merci », et surtout PAS « Plus tard ».** Le catalogue rend `Not now` par « Plus tard »
  (`updates.toast.later`, et l'ancien bouton askCmdr.consent.decline, retiré). Ici le refus est définitif : Cmdr ne
  repose jamais la question. Deux refus de nature différente doivent se lire différemment · high
- **« quelques jours » ne devient jamais un nombre.** Le seuil peut bouger et le compteur n'a commencé qu'à la
  livraison. « depuis quelques jours » porte le vague de l'anglais « for a few days now ».
- **`down there` → « en bas ».** Le Dock est en bas par défaut (`Dock.app` `BOTTOM` → « En bas »), mais il peut être
  placé à gauche ou à droite : la formulation française porte exactement le même pari que l'anglaise, volontairement. Le
  corps ne redit pas « Dock » (le titre et le bouton le disent déjà) ; « qu'il reste » porte le sens de `pinned`,
  c'est-à-dire que l'icône demeure même app fermée.
- **`addedButDockDidNotRestart` ne dit surtout pas que l'ajout a échoué**, parce qu'il a eu lieu : « L'icône de Cmdr est
  en place, mais le Dock ne s'est pas rechargé. » Le sujet de la deuxième phrase est « Elle » = l'icône (féminin), pas
  Cmdr. Aucun mot de la liste noire (« erreur », « échec », « a échoué », « bloqué »).
- **`notAdded` suit le moule sanctionné « n'a pas pu »** (§ style.md, « N'a pas pu se terminer »), pas « n'a pas réussi
  », trop proche de « échec » : « Cmdr n'a pas pu s'ajouter au Dock cette fois-ci. » La sortie manuelle reprend le geste
  d'Apple (« faire glisser … depuis votre dossier Applications »), avec le `y` qui renvoie au Dock nommé juste avant.
- **`managedDock` nomme une personne sans la genrer** : « La personne qui gère ce Mac peut modifier ce réglage. » Ni «
  l'administrateur », ni un point médian ; c'est la restructuration prévue par la règle d'inclusivité.
- **Les deux clés `settings.behavior.dockPinNudgeOfferedAt.*` sont internes** et copient le moule des voisines
  (`serversPinHintSeen`, `doubleClickOnPaneNotificationSeen`) : libellé en syntagme nominal terminé par un participe («
  Proposition d'ajout au Dock faite »), description en « Indique si … a été … ».
- **Apostrophes ICU doublées partout** (`qu''il`, `d''avis`, `L''icône`, `s''est`, `s''y`, `n''a`, `s''ajouter`, `l''y`,
  `d''ajout`, `d''ajouter`), toutes en ASCII U+0027. Espace ASCII simple avant les `?`.
- La pile de référence était bien lisible sur cette machine
  (`/Users/veszelovszki/projects-git/vdavid/cmdr/_ignored/i18n/fr/`), complétée par les paquets système pour `Dock.app`
  et `LoginItems.appex`, que la pile ne contient pas.

## Le menu du Dock : les cinq clés `menu.dock.*`

Famille RAW (`menu.*`, tirée par Rust, jamais par ICU) : apostrophes simples, `{name}` et `{parent}` sont des cibles de
remplacement littérales. Aucune capture ne peut photographier un menu natif, donc tout vient de la pile
(`/Users/veszelovszki/projects-git/vdavid/cmdr/_ignored/i18n/fr/`) et du paquet `Dock.app` du système.

- Open <App> (l'élément qui ramène la fenêtre au premier plan depuis le menu du Dock) → **Ouvrir <App>** · `Dock.app`
  `fr.lproj/DockMenus.strings` : `OPEN` → « Ouvrir », et le moule à nom d'app y est nu, sans guillemets (`SHOW_NAME` → «
  Afficher %@ », `HIDE_NAME` → « Masquer %@ ») ; les guillemets sont réservés aux noms de fichiers (`OPEN_FILENAME` → «
  Ouvrir « %@ » »). Donc « Ouvrir Cmdr », pas « Ouvrir « Cmdr » ». Lu sur macOS 26.6.2 build 25G83, 2026-09-09 · high
- Go to Folder… → **Aller au dossier…** · Finder `MenuBar.json` `261.title` (élément Aller > Aller au dossier…),
  confirmé par `LocalizableMerged` `N83` et `GotoWindow.json` `1.title` (« Aller au dossier ») · high. Le catalogue a
  déjà « Aller au chemin… » à `menu.go.goToPath` : c'est une AUTRE commande (un chemin qu'on saisit), et les deux
  libellés doivent rester distincts.
- Connect to Server… → **Se connecter au serveur…** · Finder `MenuBar.json` `266.title` (élément Aller > Se connecter au
  serveur…) et `LocalizableMerged` `N84` · high. ❌ Pas « Connexion au serveur », qui est chez Apple le TITRE de la
  fenêtre qui s'ouvre (`ConnectToWindow.json` `1.title`, `LocalizableMerged` `PW28` / `FR15`), pas la commande. Le
  libellé de commande prend le verbe pronominal, cohérent avec `disconnect → se déconnecter` déjà dans `terms.json`.
- Search files… → **Rechercher des fichiers…** · repris mot pour mot de `menu.edit.searchFiles`, déjà livré : c'est la
  même commande, atteinte depuis la barre de menus au lieu du Dock, et la description anglaise demande explicitement la
  même formulation · high

Décisions de formulation :

- **`{name} ({parent})` reste identique à l'anglais**, avec `sameAsSourceJustification` sur la clé. Le français garde
  l'ordre nom-puis-qualifiant et la même parenthèse : Apple écrit sa propre ligne de désambiguïsation `^0 (^1)` (Finder
  `LocalizableMerged` `SB_iCloudDetail`), et le catalogue livre déjà ce moule à
  `fileExplorer.functionKeyBar.actionWithShortcut`. Pas d'espace avant la parenthèse ouvrante au-delà de l'espace normal
  : la parenthèse n'est pas une ponctuation double.
- **Les points de suspension restent U+2026**, comme la source anglaise, conformément à la règle « caractère pour
  caractère » de `style.md`, et comme le Finder français qui écrit lui aussi U+2026 dans ces deux éléments.
- **Aucune apostrophe dans les cinq valeurs**, donc le piège du doublage ICU ne se pose pas ici ; il se poserait si un
  jour un libellé du menu du Dock prenait une élision, et la réponse serait alors l'apostrophe SIMPLE (famille RAW).

## La proposition « Afficher dans le Finder » et l'avis de première fois (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Deux moments de la même fonction : la proposition unique d'ouvrir dans Cmdr le « Afficher dans le Finder » des autres
apps, et l'avis unique la première fois qu'une de ces demandes arrive ici. Les deux surfaces désignent des commandes
propres à macOS, donc la terminologie macOS l'emporte (style.md § surfaces système).

- **“Show in Finder” → `« Afficher dans le Finder »`, guillemets français avec espaces insécables** · Déjà fixé dans
  `settings.navigationAndFileOps.card.showInFinder` et `settings.revealHandler.description` · `high`. Les toasts
  reprennent exactement cette forme, pour que la fiche des réglages et l'avis nomment la même action pareil.
- **pane → `panneau`** · Forme du catalogue dans `fileExplorer.doubleClickHint.body` · `high`.
- **Settings (la fenêtre propre à Cmdr) → `les Réglages`** · `settings.window.title` = « Réglages » · `high`.
- **“for a while now” → `depuis un moment`** · Volontairement vague : le seuil peut bouger, donc ❌ jamais de chiffre.
  Même règle que `main.dockPinNudge.body` (« quelques jours ») · `high`.
- **L'avis de première fois ❌ n'est pas une excuse** · Il dit ce qui vient de se passer, pourquoi, et où se trouve
  l'option. D'où `Cmdr est configuré pour les récupérer`, ❌ jamais « désolé » · `high`.

## La prise en main : l'assistant, l'étape IA et la liste de contrôle (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

Famille ICU : apostrophes DOUBLÉES partout. Couvre l'étape 1 et le cadre de l'assistant, l'étape IA, la liste de
contrôle de l'étape 3 et les résumés de l'étape 4.

Termes :

- More about <X> (nom accessible de la pastille d'info) → **En savoir plus sur {topic}** · macOS Finder
  `ICloudNoDocumentsView.json` (`URC-Za-pdT.title` → « En savoir plus sur iCloud ») et `ICloudUpgradeView.json`
  (`3Lg-B5-cFZ.title` → « En savoir plus… »), relevés dans le tas de références 2026-09-09 · high. `{topic}` porte un
  libellé déjà traduit et de genre inconnu : la tournure « sur {topic} » ne demande ni article ni accord, c'est
  exactement la restructuration que la règle des insertions non contrôlées attend.
- checklist → **liste de contrôle** · MS terminology FRA (`FRENCH.tbx`, entrée 30962 → 30965), relevé 2026-09-09 · high.
  Le titre complet devient « Liste de contrôle de la prise en main » : `onboarding` reste `prise en main` partout
  (entrée de `terms.json`), et le compte « each takes 30 seconds » se rend par « 30 secondes par point », parce que «
  chacune » n'aurait pas d'antécédent féminin en français.
- Save (le bouton à côté du champ e-mail) → **Enregistrer** · macOS AppKit, pervasif (`SavePanel.json`,
  `NSLocalSavePanel.json`, `NSRemoteSavePanel.json`, `Document.json`, `Printing.json`, `Preferences.json` → «
  Enregistrer »), relevé 2026-09-09 · high. Les deux messages d'échec du champ (`signup.rejected`, `.unreachable`)
  citent ce mot tel quel : les trois clés forment une unité, ne renommez le bouton que dans les trois à la fois.
- star (le verbe de GitHub) → **ajouter une étoile** · GitHub localise son interface en français et son bouton dit «
  Ajouter une étoile » (état marqué : « Marqué d'une étoile »), d'après la doc GitHub française
  `docs.github.com/fr/get-started/exploring-projects-on-github/saving-repositories-with-stars`, relevée 2026-09-09 ·
  high. La ligne de la liste, `onboarding.stepBeta.checklist.star`, la reprend telle quelle : « Ajouter une étoile au
  dépôt sur GitHub ». `dépôt` pour `repo` était déjà en place.
- Like (le bouton d'AlternativeTo) → **Aimer** · AlternativeTo n'a PAS d'interface française (le bouton dit « Like » en
  anglais quel que soit le visiteur, vérifié sur `alternativeto.net` 2026-09-09), donc aucun terme éditeur à reprendre.
  On prend le verbe français standard du bouton social (Facebook FR « J'aime »), à l'infinitif comme tout libellé
  d'action · tentative. ❌ Pas « Liker », anglicisme familier.
- mailing list → **liste de diffusion** · MS terminology FRA (`FRENCH.tbx`, entrées 210512 → 724756 et 2791 → 724755),
  relevé 2026-09-09 · high.
- `API key` → **clé d'API** aussi dans les deux clés neuves de l'assistant (`stepAi.cloud.help`,
  `stepAi.missingKeyWarning`), conformément à l'entrée de `terms.json` · high.

Décisions de formulation :

- **Le résumé d'un interrupteur reprend le NOM de sa description longue, pas forcément son verbe.**
  `stepOptional.mtp.summary` dit « suspend le processus macOS natif » là où `.desc` dit « Cmdr doit supprimer ce
  processus macOS pendant son exécution » : le nom partagé (« le processus macOS ») fait le lien, et `suspendre` est le
  sens réel de `suppress` ici (macOS le retrouve à la fermeture de Cmdr). ❌ `supprimer` seul, sans le contexte de la
  description, se lirait « effacer ».
- **Le nom d'une autorisation macOS se cite mot pour mot, jamais paraphrasé.** L'étiquette d'Apple est « Réseau local »
  (clé `LOCAL_NETWORK` de `SecurityPrivacyExtension.appex`, macOS 26.6.2 build 25G83, lue le 2026-09-09), et
  `stepOptional.networking.summary` comme `.desc` la reprennent telle quelle, entre guillemets français. ❌ Pas « accès
  au réseau local » : ce nom-là ne se trouve nulle part dans Réglages, donc l'utilisateur le cherche en vain.
- **`<field></field>` se place après l'objet, pas en fin de phrase.** La balise est un champ de saisie rendu au milieu
  de la phrase : « Saisissez votre adresse e-mail <field></field> pour recevoir… ». Le verbe reste `saisir` (Finder),
  jamais `taper`.
- **Un renvoi à une section des Réglages recopie mot pour mot le titre livré**, ici `settings.section.updatesAndPrivacy`
  = « Mises à jour et confidentialité », et garde le chevron `›` de la source : « Réglages › Mises à jour et
  confidentialité » (`stepBeta.signup.unreachable`).
- **« Couldn't reach X » reste sur le verbe `joindre`**, comme `onboarding.cloudSetup.status.connectionError` («
  Impossible de joindre le service pour le moment ») : ni « erreur », ni « échec », le constat puis la sortie.
- **`dumber` se traduit franchement.** `stepAi.local.tooltip` dit « nettement plus bête que les modèles cloud » : la
  source choisit délibérément un mot cru et honnête, et l'adoucir en « moins performant » trahirait l'aveu.
- **`<strong>Oui, je veux l'IA</strong>` dans l'info-bulle du modèle local recopie `stepAi.cloud.label`**, caractère
  pour caractère : l'info-bulle envoie l'utilisateur vers cette option-là, les deux doivent se reconnaître.
- **`2 GB` → `2 Go`**, comme `stepOptional.indexing.descCost` écrit déjà « 1 Go ».
- **released copy / Dev and test builds → `version publiée` / `versions de développement et de test`**
  (`settings.revealHandler.notProductionBuild`) · MS terminology FRA (`FRENCH.tbx` : `release` → `version`,
  `production build` → `version de production`). « version » plutôt que « build » ou « compilation » : l'info-bulle
  oppose deux sortes de versions de l'app, pas des dossiers (`search.systemDirExclude.default` garde « compilation »
  pour les dossiers). La seconde phrase reprend le moule de `settings.revealHandler.notInApplications`
  (`laisserait chaque clic sur « Afficher dans le Finder » sans destination`) · `high`.

## La visionneuse récupère d'abord le fichier (`viewer.pull.*`, `viewer.error.stoppedResponding`)

Famille ICU : apostrophes doublées. Le panneau s'affiche au milieu de la visionneuse quand la copie d'un fichier distant
(téléphone, serveur, archive) vers un fichier temporaire dure plus d'une seconde.

Termes :

- fetch (copier un fichier distant sur le Mac avant de l'afficher) → **récupération** (nom d'état) / **récupérer** ·
  macOS Réglages Système, `fr/macOS/SystemSettings/Localizable.json`, clé `Fetching Menu Item` (`Fetching…` →
  `Récupération…`), relevé dans le tas de références 2026-09-10 · high. Le titre suit le moule nominal des fenêtres de
  progression du Finder (`LocalizableMerged.strings` `PW5_V1` = `Préparation de la copie de « ^1 »`, `PW45.2` =
  `Préparation du partage de « ^0 »`, macOS 26.6.2 build 25G83, lu le 2026-09-10). Les guillemets autour de `{fileName}`
  règlent aussi l'élision : `de « image.png »` ne demande jamais `d'`. `téléchargement` reste réservé à Internet
  (`settings.mediaIndex.clip.failed`).
- x of y (quantité reçue sur la taille totale) → **{doneText} sur {totalText}** · Finder `PW3` (`^0 of ^1 – ^2` →
  `^0 sur ^1 – ^2`, la ligne de taille de la fenêtre de copie) et `PW8` (`^0 sur ^1 copiés`), macOS 26.6.2 build 25G83,
  2026-09-10 ; le catalogue écrit déjà `fileExplorer.imageIndex.folder.someIndexed` « {doneText} sur {totalText} » ·
  high. Thunar dit `%s de %s` : Tier 3, le Finder l'emporte.
- so far (après une taille) → **pour l'instant** · moule du catalogue (`queryUi.results.live.matchesSoFar`,
  `search.walkHandoff.counts`) · high. ❌ Pas `{doneText} reçus` : le participe devrait s'accorder avec une unité
  inconnue (`1 octet reçu` / `12 Mo reçus`).

Décision de formulation :

- **« This file stopped arriving » → « La récupération de ce fichier n'avance plus. »** Le constat reprend le moule du
  transfert bloqué (style guide § Voix : « Le transfert n'avance plus ») et le nom du titre, donc le panneau et le
  message se lisent comme la même opération. La suite recopie `errors.listing.couldntReadUnknown.suggestion` (« Vérifiez
  que le disque ou l'appareil est toujours connecté ») et finit sur `puis réessayez`. `connecté` s'accorde avec
  `le téléphone ou le serveur`, deux masculins fixés par la phrase, jamais avec la personne · `high`.

## L'index d'un téléphone ADB paraît obsolète (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

Famille ICU : apostrophes doublées. Versions « téléphone » des trois clés de l'index obsolète : un téléphone Android en
débogage USB ne signale jamais ses changements, donc son index reste jaune alors qu'il est toujours branché. ❌ Aucune
des trois ne parle de déconnexion.

Termes :

- phone → **téléphone** · repris du catalogue (`adb.connect.*`, `settings.adb.status.*`, § Le téléphone Android) · high.
  `appareil` reste le mot MTP générique (Kindle, appareil photo) ; ici la source dit `phone`.
- keep the index current with the changes Cmdr makes → **mettre l'index à jour avec les changements qu'il effectue** ·
  `à jour` et `Mettre à jour l'index` viennent de `indexing.step.updateIndex` et `driveIndex.tooltipFresh` (« Indexé et
  à jour ») ; `changements` suit la famille `driveIndex.*` / `indexing.run.changeCheck` (« Recherche des changements »)
  · high.
- show up (in folder sizes and search) after a rescan → **sont pris en compte (dans la taille des dossiers et la
  recherche) après une nouvelle analyse** · `nouvelle analyse`, `la taille des dossiers` et `recherche` recopient les
  sœurs `staleDialog.body` et `tooltipStale`. `apparaître dans la taille` se lirait mal : ce sont des changements, pas
  des éléments · tentative (paraphrase, aucune source ne l'a mot pour mot).
- stays as a reminder (le statut jaune) → **reste affiché pour vous le rappeler** · `affiché` s'accorde avec
  `le statut`, jamais avec la personne ; `le` reprend la situation, comme le « le signale toujours » de
  `staleDialog.body` · tentative.

Décisions de formulation :

- **`{name}` sujet d'un verbe sans accord** : `{name} ne prévient pas Cmdr quand ses fichiers changent`. Le nom du
  téléphone vient de l'appareil (« Pixel 9 Pro XL ») ; aucun participe ni adjectif ne s'accorde avec lui.
- **`on the phone itself` → `directement sur le téléphone`** dans l'info-bulle, plus naturel que `lui-même` ; le corps
  garde `sur le téléphone`, comme l'anglais.

## La feuille du serveur : dossier racine et dossier initial (`servers.sheet.rootFolder*`, `.startFolder*`, `.nameHelp`, `servers.refusal.startFolderOutsideRoot`, `.rootNotFound`, `.startFolderNotFound`, `.saveUnconfirmed`)

Deux champs de la feuille d'ajout ou de modification d'un serveur SFTP ou WebDAV : le plafond sur le serveur (Cmdr ne
remonte jamais au-dessus) et le dossier où le panneau s'ouvre. Famille ICU : apostrophes ASCII doublées. Tas de
références miné (`_ignored/i18n/fr/`, 2026-09-11) et paquets macOS installés (macOS 26.6.2 build 25G83).

Termes :

- root folder (le plafond d'un serveur enregistré) → **dossier racine** · MS terminology FRA, id 2130701 (« dossier
  racine » pour `root folder` ET `root directory`) ; Double Commander (« Go to root directory » → « Aller au dossier
  racine ») ; Xfce Thunar (« The root folder has no parent » → « Le dossier racine n'a pas de parent ») ; déjà livré
  dans `errors.mutation.cantRenameVolumeRoot` · high. Remplace l'ancien libellé `Dossier distant` du même champ.
- start folder (le dossier où s'ouvre le serveur) → **dossier initial** · tentative. ❌ Pas `dossier de départ` : le
  Finder français nomme ainsi le dossier personnel (Finder `LocalizableMerged` `TL_HELP_HOME` « Go to your Home folder »
  → « Aller à votre dossier Départ », `FF21` / `TL2` « Home » → « Départ »), alors même qu'Apple rend `Start Location`
  par `Point de départ` (WorkflowKit) et que Double Commander écrit « Chemin de départ ». ❌ Pas `dossier de démarrage`
  non plus : Dolphin (« Afficher au démarrage ») et macOS (« Disque de démarrage ») réservent `démarrage` au lancement
  de l'app ou du Mac. Aucune source n'atteste `dossier initial` mot pour mot ; le catalogue emploie déjà `initial` au
  sens « du début » (`analyse initiale`, `configuration initiale`).
- your account (celui avec lequel on se connecte) → **votre compte** · reprend `le compte` de
  `servers.sheet.identityLocked` et d'`errors.listing.remotePermissionDenied.*` · high.

Décisions de formulation :

- **`Leave it empty to …` → `Laissez ce champ vide pour …`**, sur le moule de
  `settings.fileOperations.adbBinaryPath.description` (« Laissez ce champ vide et Cmdr cherche… »). `Laissez-le vide`
  ferait porter `le` sur un nom que la ligne ne répète pas.
- **`call this server by its account and host` → `désigner ce serveur par son compte et son hôte`** : `nommer` se
  confondrait avec la valeur du champ `Nom` lui-même.
- **`Where the server opens.` devient une phrase complète** : `Le serveur s''ouvre dans ce dossier.` Le fragment
  `Là où le serveur s''ouvre.` sonne bancal en français.
- **`a folder inside it` → `un dossier à l''intérieur de celui-ci`** · `à l''intérieur de` est la forme du Finder (« Nom
  du nouveau dossier à l'intérieur de « ^0 » ») ; `celui-ci` renvoie sans ambiguïté au dernier nommé, le dossier racine,
  là où `qu''il contient` pourrait se rattacher au sujet.
- **`Cmdr can''t open … Check that it exists and that your account can read it.` →
  `Cmdr ne peut pas ouvrir … Vérifiez qu''il existe et que votre compte peut le lire.`** · Cmdr reste sujet, comme les
  sœurs `servers.refusal.*` ; `Vérifiez que …` est le moule d'`errors.listing.*.suggestion`. `{host}` suit `sur`, sans
  accord.
- **`{host} didn''t answer in time, so nothing was saved.` →
  `{host} n''a pas répondu à temps, Cmdr n''a donc rien enregistré.`** · la première proposition recopie
  `servers.refusal.timedOut` ; la voix active remplace le passif anglais (style guide § Voice).
  `Réessayez dans un instant.` recopie cinq clés livrées (`ai.translateError.timeout.body`,
  `operationLog.rollback.refusalUnexpected`, …).

## Pourquoi un partage ne se monte pas ou sa liste ne se charge pas (`errors.mount.*`, `errors.shareList.*`)

Les phrases sous « Impossible de monter le partage » (`fileExplorer.networkMount.mountFailedTitle`) et « Impossible de
se connecter à {hostName} » (`fileExplorer.network.share.connectFailedTitle`), plus les notifications
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` et `servers.refusal.accountNotPermitted`. Tier 1 : le paquet
installé `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11), qui rédige ces
mêmes cas pour « Se connecter au serveur » et n'est pas dans le tas de références.

- **share → `partage`** · NetAuthAgent `EINFO_NO_SHARE` (« Le partage « %@ » n'existe pas sur le serveur. ») · high
- **guests → `invités`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` (« Ce serveur n'accepte pas les invités. ») · high
- **reach → `joindre` / `joignable`** · la sœur `fileExplorer.pane.directConnectionUnreachableToast` (« injoignable »)
  et la ligne « Couldn't reach X » plus haut · high. `servers.refusal.unreachable` dit `atteindre` : écart antérieur,
  pas touché ici.
- **You're signed in → `L'identification sur « {server} » … a réussi`** · `Vous êtes identifié` s'accorderait avec la
  personne ; la phrase s'accorde avec l'identification · high. **when you're ready → `quand vous le souhaitez`**.
- **SMB 2 or later → `SMB 2 ou ultérieur`** · moule de `settings.ai.tooltipLocalDisabled` (« M1 ou ultérieur ») · high
- **package → `paquet`** · KDE Dolphin (« Impossible de trouver le paquet %1. »),
  `licensing.acknowledgements.npmHeading` · high. **distribution (Linux) → `distribution`** · aucune source dans ce sens
  (la TBX n'a que le sens logistique) · tentative
- **there's nothing to speed up → `il n'y a donc rien à accélérer`** · moule de `errors.eject.notAnSmbVolume` · high
- **Try again in a moment → `Réessayez dans un instant.`** · `operationLog.dialog.loadError` · high
- Même anglais, même français : `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`. Les trois notifications et
  `servers.refusal.accountNotPermitted` doublent l'apostrophe (ICU) ; les `errors.*` bruts gardent l'apostrophe simple.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `éditeur de texte`** · terminologie Microsoft (`text editor` → `éditeur de texte`) · `high`. La
  catégorie d’app, ❌ pas l’app TextEdit d’Apple, dont le nom arrive dans `{app}`.
- **default text editor → `éditeur de texte par défaut`** · le catalogue (`commands.fileEdit.label` « Modifier dans
  l’éditeur par défaut ») · `high`.
- **Edit files in [app] → `Modifier les fichiers avec`** · la phrase continue dans le menu ; `avec` accepte tout nom
  d’app sans article · `tentative`.
- `{app}` suit `dans` sans article. Dismiss et Open settings identiques à `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings`.
- **system default → `par défaut du système`** · le catalogue (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) · `high`. `settings.behavior.textEditorApp.systemDefault` ajoute le
  nom de l’app entre parenthèses, comme `settings.appearance.language.opt.systemWithLanguage`.
- « Choose an app… » et « Checking your apps… » identiques à `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. L’astuce (`fileExplorer.edit.hint`) suit
  `commands.handler.openTerminalHere.hint`, sans dire où se trouve le réglage : son bouton y mène directement.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (éjection ou démontage en cours) → `est en cours de déconnexion`** · nom d’action du
  `déconnecter` transitif déjà fixé, sur le modèle nominal de Thunar `fr` (« Démontage du périphérique » / « Éjection du
  périphérique ») · `high`. La tournure nominale évite tout accord avec `{name}`. « Left its index as it was » → « tel
  quel », comme `operationLog.rollback.partiallyRolledBackNotice` ; « try again in a moment » → « Réessayez dans un
  instant », comme `errors.eject.notResponding`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (déjà fait, le disque est parti) → `a été déconnecté`** · passé composé du `déconnecter` déjà fixé,
  avec l’accord au masculin de « disque » que retient `indexing.staleDialog.body` (« Pendant que {name} était déconnecté
  ») · `high`. Ce n’est PAS la tournure nominale « est en cours de déconnexion » de
  `fileExplorer.navigation.driveIndex.driveLeaving`, qui décrit une éjection encore en cours. « Starts from scratch » →
  « repart de zéro », comme `indexing.rescan.incompletePreviousScan` ; `analyse` et `taille des dossiers` viennent de la
  même famille.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

Les quatre valeurs « sided » disent de quel CÔTÉ le disque est parti et où sont les fichiers maintenant. Le câble a été
tiré, ce n'est pas une éjection voulue. Elles s'affichent dans le même dialogue que
`errors.write.deviceDisconnected.message.move`, donc le vocabulaire ne peut pas diverger. La réassurance finale est la
raison d'être de chaque phrase : elle reste en FIN de valeur, là où l'anglais la met, parce que c'est la dernière chose
que la personne lit.

- **was disconnected → `a été déconnecté`** · même passé composé et même accord masculin (« disque ») que la sœur
  `indexing.needsFreshScan.afterDisconnect` et que `indexing.staleDialog.body` · `high`. ❌ Pas « est en cours de
  déconnexion » (`fileExplorer.navigation.driveIndex.driveLeaving`) : là-bas l'éjection est encore en cours, ici le
  disque est déjà parti.
- **the drive that left → `ce disque` ; l'autre disque → `{counterpart}` nu** · `drive → disque` déjà fixé, et
  `ce disque` reprend `errors.eject.volumeNotFound` (« Ce disque n'est plus connecté ») · `high`. ❌ Jamais d'article
  élidable ni de contraction devant `{volumeName}` ou `{counterpart}` : leur genre et leur initiale sont inconnus à
  l'écriture. D'où `sur {counterpart}` et `vers {counterpart}`, qui ne demandent ni article ni accord ; `vers` est le
  macOS Finder Tier 1 du geste (`FR2`/`FR3` : « Déplacer vers » / « Copier vers »).
- **{done} of {total} files → `{done} des {total} fichiers`** · `high`. Le partitif est la seule forme juste quand
  `{done}` vaut 1 (« 1 des 5 fichiers ») ; « {done} fichiers sur {total} », pourtant plus proche du moule macOS
  (`LocalizableMerged.json` `PW8` « ^0 sur ^1 copiés », `PW35` « Éléments à mettre à jour : ^0 sur ^1 »), écrirait « 1
  fichiers », et la famille est brute : aucun pluriel ICU n'est disponible pour rattraper. macOS atteste la contraction
  devant un compteur (`N169.21_V2` : « Supprimer toutes les sauvegardes des ^0 éléments. »).
- **Your originals are untouched → `Vos originaux sont intacts`** · paire minimale déjà dans le catalogue :
  `errors.write.destinationNotFound.message.copy` rend « The originals are untouched. » par « Les originaux sont
  intacts. », et `errors.write.notConnected.message.destination` rend « Your files are untouched. » par « Vos fichiers
  sont intacts. » · `high`. Le possessif suit donc l'anglais clé par clé : `vos` quand il dit « your ».
- **so nothing is lost → `rien n'est donc perdu`** · le `donc` postposé du catalogue (`errors.mutation.volumeGone` : «
  …, rien n'a donc été modifié. ») ; « perdu » est le mot macOS `fr` pour « lost » (`FF45`, `FF46`) · `high`.
- **The rest are still on the drive → `Les autres fichiers sont toujours sur ce disque.`** · `high` pour les mots,
  `tentative` pour le choix de tournure. « Le reste » est pourtant attesté en Tier 1 (`PE30` : « …les ignorer et copier
  le reste ? ») et dans le catalogue (`fileOperations.transferProgress.rollbackAlreadyLandedTooltip` : « le reste des
  originaux »), mais « Le reste est toujours… » repasse au singulier juste après un compte pluriel et rend la
  réassurance vague ; nommer les fichiers dit explicitement ce qui n'a pas bougé.
- **before Cmdr could finish the move → `avant que Cmdr ait pu terminer le déplacement`** · moule exact de
  `errors.listing.connectionDropped.explanation` (« avant que Cmdr ait pu terminer la lecture ») · `high`.
- **after Cmdr copied … → `alors que Cmdr avait déjà copié …`** · `high`. Le plus-que-parfait avec `déjà` dit ce que
  l'anglais dit avec « after », sans le « après que » + indicatif qui alourdit une phrase déjà longue. Dans la variante
  destination, « to it » devient le pronom `y` (« y avait déjà copié ») : neutre en genre, donc sans pari sur
  `{volumeName}`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

Ni panne ni déconnexion : le déplacement s'est arrêté à sa dernière étape parce que la destination n'a pas pu garantir
que les copies étaient écrites. Rien n'est perdu, et Cmdr a gardé les originaux JUSTEMENT parce qu'il ne peut pas
prouver que les copies sont arrivées. La valeur ne doit jamais laisser entendre que le déplacement a mal tourné.

- **Couldn't confirm the move → `Impossible de confirmer le déplacement`** · moule figé du catalogue pour exactement
  cette situation (`fileOperations.mkdir.timeoutMessage`, `fileExplorer.rename.unconfirmed`,
  `fileExplorer.pane.trashUnconfirmedToast` : « Impossible de confirmer … ») · `high`. ❌ Pas « Déplacement impossible
  », qui est déjà le titre d'un vrai refus de lecture (`errors.write.readError.title.move`) et qui dirait que rien n'a
  eu lieu. Titre sans point final, comme les autres titres de la famille.
- **Cmdr couldn't confirm → `Cmdr n'a pas pu confirmer`** · voix active avec Cmdr sujet, comme le demande `style.md` ;
  `confirm → confirmer` est la terminologie Microsoft FRA (ids 1475887, 37786) et le verbe des trois toasts ci-dessus ·
  `high`.
- **were saved on {volumeName} → `étaient bien enregistrés sur {volumeName}`** · `enregistrer` est le verbe macOS `fr`
  de « save » (« Vos modifications ont été enregistrées ») · `high`. Le `bien` porte ce que la confirmation aurait
  établi ; sans lui, la phrase se lit comme un simple constat technique.
- **at the destination → `à destination`** · déjà dans le catalogue (`fileOperations.cancelRollback.moveAlreadyLanded` :
  « est déjà arrivé à destination ») · `high`.
- **it kept your originals where they were → `et a donc laissé vos originaux là où ils étaient`** · `high`. Sujet
  coordonné, ❌ PAS le pronom `il` : l'antécédent masculin le plus proche est `{volumeName}`, et la phrase dirait que
  c'est le disque qui a gardé les originaux. Même discipline que « Renommage non confirmé » plus haut. Ne pas « corriger
  » vers `il a donc laissé`.
- **Your originals haven't moved → `Vos originaux restent où ils sont.`** · phrase déjà expédiée par
  `errors.write.readOnlyDevice.source.suggestion` (« The originals stay where they are. » → « Les originaux restent où
  ils sont. »), avec le possessif que l'anglais porte ici · `high`. ❌ Pas « n'ont pas bougé » (idiomatique, mais absent
  du pile), ❌ pas « ne se sont pas déplacés », qui répéterait `déplacement` de la phrase précédente.
- **Have a look at the destination → `Jetez un œil à la destination`** · `settings.askCmdr.memory.description` («
  Jetez-y un œil, ou repartez de zéro. ») · `high`. La ligature `œ` est celle du catalogue et de macOS (« Coup d'œil »)
  ; seules les apostrophes restent ASCII.
- **try the move again → `réessayez le déplacement`** · `réessayez` est le verbe de reprise de la famille
  (`errors.write.deviceDisconnected.suggestion` : « …et réessayez. », `errors.eject.notResponding` : « Réessayez dans un
  instant. ») · `high`.
- Famille brute : apostrophes ASCII SIMPLES dans les huit valeurs, aucun `sameAsSourceJustification`.

## Les fichiers d'un déplacement incomplet retrouvés sur un disque (`fileOperations.leftovers.stagingFolderKept`)

Notification d'information affichée quand un disque revient (ou au démarrage de Cmdr) et que Cmdr retrouve le dossier de
travail d'un déplacement qui ne s'est jamais terminé, avec des fichiers encore dedans. Cmdr les laisse tous où ils sont,
parce qu'ils peuvent être l'unique exemplaire de la personne : le déplacement a peut-être déjà retiré les originaux. On
ne demande rien, rien n'est en danger ; la ligne existe pour que des fichiers qui manquent aient une adresse. ❌ Ne
jamais suggérer de supprimer le dossier, et ❌ jamais « erreur » / « échec ».

- **hidden (l'attribut du fichier lui-même : nom commençant par un point, ou drapeau « hidden » du système) → `caché`**
  · le catalogue le pose déjà sur les trois surfaces qui NOMMENT la fonction (`menu.view.showHiddenFiles` et
  `settings.listing.showHiddenFiles.label` « Afficher les fichiers cachés », `commands.viewShowHidden.label` « Afficher
  ou masquer les fichiers cachés ») et sur celle qui la DÉFINIT (`settings.listing.showHiddenFiles.description` « … les
  éléments que le système marque comme cachés ») ; les cinq gestionnaires de fichiers du tas sont unanimes sur «
  fichiers cachés » (Nautilus `nautilus.po` « Afficher ou masquer les fichiers cachés », Dolphin `dolphin.po` « Afficher
  les fichiers cachés », Thunar `thunar.po` « Nombre de fichiers cachés », Total Commander `WCMD.INC` « Affiche les
  fichiers cachés », Double Commander `doublecmd.po` « Afficher les fichiers cachés et système »), et Microsoft FRA
  donne les deux formes pour la même définition « Not visible to the user » (`caché` id 326097, `masqué` id 61317), donc
  ne tranche pas · `high`. Relevé dans `~/projects-git/vdavid/cmdr/_ignored/i18n/fr/`, 2026-09-16.
  - **La frontière `caché` / `masqué`** : `caché` qualifie ce que le fichier EST (son attribut, qui ne dépend pas de la
    vue) ; `masqué` qualifie ce que l'interface NE MONTRE PAS en ce moment (`fileExplorer.functionKeyBar.hiddenToast` «
    La barre des touches de fonction est maintenant masquée », `queryUi.pathPills.hiddenAria` « Segments de chemin
    masqués », `settings.indexing.silencedDrives.label` « Demandes d'indexation masquées »). `masquer` reste le VERBE de
    l'action, chez Apple comme chez nous (`commands.viewShowHidden.label` réunit les deux : « Afficher ou masquer les
    fichiers cachés »).
  - Le macOS français ne publie aucun « fichiers cachés » : il tourne la chose au verbe, y compris pour le point initial
    (`SavePanel` / `Document` : « Le Finder masque les fichiers commençant par un point. », Finder `FI16` : « … le
    fichier sera masqué. »). Ce n'est donc pas une preuve Tier 1 CONTRE `caché`, seulement l'absence de la collocation ;
    le catalogue et les cinq gestionnaires décident.
  - Raison décisive pour cette clé : son seul contenu actionnable est d'aller activer l'affichage, et l'interrupteur
    s'appelle « Afficher les fichiers cachés ». Le mot de la notification doit être celui de l'interrupteur, comme le
    veut `style.md` (« Un renvoi à un réglage … réutilise le libellé du bouton qui l'ouvre, mot pour mot »).
  - `fileExplorer.rename.hiddenAfterRename` parle de l'attribut, donc « les fichiers cachés ne sont pas affichés ».
- **unfinished (move) → `incomplet`** · la sœur d'à côté dans le même fichier fixe déjà `unfinished copy` →
  `copie incomplète` (`fileOperations.cancelRollback.stagedLeftover.named`), et les deux notifications peuvent se suivre
  chez la même personne · `high`. Le tas de références ne contient ni `unfinished` ni `inachevé` (vérifié 2026-09-16),
  donc c'est la cohérence interne qui décide. ❌ Pas `interrompu`, qui rend `interrupted` ailleurs
  (`settings.advanced.showStagingTempFiles.description` « les restes d'une copie interrompue »,
  `errors.write.connectionInterrupted.title`) : garder les deux mots anglais distincts. `inachevé` était l'autre
  candidat naturel, écarté pour ne pas ouvrir un deuxième mot sur la même notion.
- **files from a <opération> → `des fichiers issus d''un <opération>`** · `tentative` pour le connecteur. Le moule du
  catalogue pour l'origine est le génitif nu (`showStagingTempFiles.description` : « Les restes d'une copie interrompue
  »), mais il marche parce que « restes de » porte déjà l'origine ; « des fichiers d'un déplacement » se lit comme une
  appartenance et non comme une provenance, d'où `issus de`. Aucune attestation dans le tas pour l'un ou l'autre.
- **left them in place → `les a laissés là où ils sont`** · reprise mot pour mot du registre déjà posé par
  `errors.write.moveNotConfirmed.message.named` (« a donc laissé vos originaux là où ils étaient ») et
  `errors.write.readOnlyDevice.source.suggestion` (« Les originaux restent où ils sont. ») · `high`. Le PRÉSENT, pas
  l'imparfait de la sœur : là-bas la phrase raconte ce que Cmdr a décidé sur le moment, ici elle dit où les fichiers se
  trouvent maintenant, ce qui est justement l'information utile.
- **a folder named {x} → `un dossier nommé {x}`** · macOS Finder `Localizable` (« Create a folder named
  ${fileName}
  inside ${target} » → « Créer un dossier nommé ${fileName} dans ${target} »), `LocalizableMerged` `A32` /
  `A35` (« un nouveau dossier nommé « ^0 » ») et Double Commander (« Il existe déjà un dossier nommé "%s". ») · `high`.
  Apple encadre le nom de guillemets ; le catalogue ne le fait pas (`stagedLeftover.named` laisse `{name}` nu) et
  l'anglais non plus, donc `{folderName}` reste nu ici aussi.
- **Le créneau des deux `{placeholder}`** : `sur {volumeName}` (préposition sans article ni accord, exactement comme
  `errors.write.moveNotConfirmed.message.named` et les quatre `deviceDisconnected.sided.*`), et `{folderName}` en fin de
  phrase après `nommé`, qui s'accorde avec `dossier` (masculin singulier) et jamais avec l'insertion. Le participe
  `laissés` s'accorde avec `des fichiers`, connu à l'écriture. Aucun des deux placeholders ne touche un article, un
  genre ou une élision.
- Sujet unique `Cmdr` et deuxième verbe coordonné (`et les a laissés`), ❌ jamais le pronom `il` : l'antécédent masculin
  le plus proche serait `{volumeName}`, et la phrase dirait que c'est le disque qui a laissé les fichiers. Même piège et
  même parade que § A move that could not be confirmed.
- Famille ICU : `d''un` avec l'apostrophe ASCII doublée, aucun U+2019, pas de deux-points donc pas d'espace avant. Pas
  de `sameAsSourceJustification` : la valeur diffère de l'anglais.

## Le menu des favoris (`fileExplorer.navigation.favorites*`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `commands.favorites*`, `shortcuts.scope.favoritesMenu`)

⌃D ouvre la liste des dossiers mis en favori sous forme de menu par-dessus le panneau actif. Les neuf premières lignes
portent une touche chiffrée (1–9) qui y mène directement ; la dernière porte le `0` et ajoute le dossier courant du
panneau à la liste. Le sélecteur de volume n'a plus de SECTION Favoris : il n'en garde qu'une ligne de tête qui bascule
vers ce menu.

- **favorite (le dossier mis en signet) → `favori` / `favoris`** · déjà dans `terms.json`
  (`bookmark / favorite → favori`) et tenu par tout le catalogue : `fileExplorer.navigation.groupFavorites` « Favoris »,
  `fileExplorer.navigation.favoritesEmpty` « (Vos favoris s''afficheront ici) », `commands.favoritesAdd.label` « Ajouter
  aux favoris », `menu.go.addToFavorites` « Ajouter aux favoris ». Source Tier 1 : macOS Finder (« Favoris », « Ajouter
  aux favoris », « Supprimer des favoris », « Serveurs favoris : ») · `high`. ❌ Jamais `signet` : c'est le mot de GNOME
  Nautilus (« Ajouter aux _signets », « Nouveau signet »), pas celui du Mac, et le catalogue en a déjà dix-huit
  occurrences en `favori`.
- **the favorites menu → `le menu des favoris`** · le concept n'existe pas chez Apple, mais la paire orthodoxe le nomme
  exactement : Total Commander `WCMD.INC` id 526 « Menu local des répertoires favoris » (le _directory hotlist_), Double
  Commander `doublecmd.po` « dossiers favoris (.hotlist) » · `high` pour le mot, avec la réserve de la gotcha 2 du guide
  : la _hotlist_ orthodoxe est une fonctionnalité voisine mais distincte (un menu arborescent configurable), donc on lui
  emprunte le mot, pas la définition. `shortcuts.scope.favoritesMenu` est livré « Menu des favoris », au registre de ses
  voisins (`shortcuts.scope.volumeChooser` « Sélecteur de volume », `shortcuts.scope.commandPalette` « Palette de
  commandes »).
- **Show favorites → `Afficher les favoris`, See favorites → `Voir les favoris`** · l'anglais tient deux verbes pour
  deux surfaces, et le français les tient pareil. `Afficher` pour ce qui OUVRE le menu (`menu.go.showFavorites` dans le
  menu Aller et son jumeau `commands.favoritesOpen.label` dans la palette), sur le moule du menu Présentation du Finder
  (« Afficher la barre latérale », « Afficher l'aperçu », « Afficher les éléments ») · `high`. `Voir` pour la ligne de
  tête du sélecteur (`fileExplorer.navigation.seeFavorites`), parce que c'est le plus court des deux et que la ligne
  partage sa largeur avec la pastille du raccourci ; ce partage est déjà celui de l'entrée `view` plus haut. Pas de
  points de suspension : ça ouvre un menu, pas une fenêtre.
- **Pluriel de `seeFavorites` : `=0` + `one` + `many` + `other`** · les catégories CLDR du français sont bien `one` /
  `many` / `other` (voir `style.md` § Plurals), et le français range 0 dans `one`. La branche `=0` l'emporte sur toute
  catégorie, donc la branche `one` ne voit jamais que 1 (« Voir 1 favori ») et reste juste ; `=0` porte le texte « vous
  n'en avez pas encore » et ne montre donc pas `{count}`. `many` est écrit à l'identique de `other`, comme partout dans
  le `fr` : un entier simple ne sélectionne jamais `many`, mais la parité et le check de pluriel veulent la branche.
  Vérifié en exécutant les cinq cas (0, 1, 2, 5, 1 000 000) dans `intl-messageformat` avec la locale `fr`.
- **current folder (celui que le panneau AFFICHE, pas celui sous le curseur) → `le dossier actuel`** · le catalogue le
  pose déjà dans `commands.favoritesAdd.description` · `high`. ❌ Pas `dossier courant`, l'anglicisme habituel, ni
  `répertoire`, réservé au sens technique. La dernière ligne du menu n'a plus qu'une clé,
  `fileExplorer.navigation.favoritesAddCurrent` (« Ajouter le dossier actuel aux favoris ») : la liste des raccourcis la
  CITE pour expliquer la touche `0` au lieu de la redécrire, donc elle lit la même valeur. ❌ Ne pas réinventer une
  seconde formulation pour cette liste. La distinction qui compte, elle, tient : `commands.favoritesAdd.label` reste «
  Ajouter aux favoris », court et sans complément.
- **mounted share → `un partage monté`** · le catalogue le dit déjà mot pour mot
  (`fileExplorer.network.browser.noMountedShares` « Aucun partage monté depuis {hostName} »,
  `settings.network.enabled.description` « … sur les partages déjà montés »), et les deux moitiés viennent du tas :
  `share → partage` (macOS « Partage et permissions : ») et `mount → monter` (macOS AppKit « Le volume « %@ » n'a pas pu
  être monté. ») · `high`. ❌ Ne jamais remonter à un nom de protocole (`SMB`, `MTP`, `ADB`) dans cette phrase :
  l'anglais évite le jargon exprès, parce que la personne visée est justement celle qui ne sait pas sur quoi elle se
  trouve.
- **`favoritesCantAddHere` parle de CE dossier, la raison vient après les deux-points** ·
  `Ce dossier ne peut pas rejoindre vos favoris : les favoris ne fonctionnent que sur un disque ou un partage monté` ·
  `high`. Le sujet reprend celui de sa jumelle `fileExplorer.navigation.favoritesAlreadyAdded` (« Ce dossier est déjà
  dans vos favoris »), et le possessif reste celui de `favoritesEmpty`, donc les deux lignes grisées se lisent en paire.
  Le moule restrictif `ne … que` vient du Finder, qui formule la même contrainte : « Vous ne pouvez créer un alias que
  dans un dossier ou disque. » (macOS Finder, relevé dans le tas 2026-09-16). ❌ Plus de verbe de pointage ici
  (`mener à`, `pointer vers`) : il imposait un complément régi au dossier cible ; `fonctionner sur` est un locatif plat.
  Espace avant les deux-points, pas de point final (info-bulle).
- **already a favorite → `est déjà dans vos favoris`** · l'info-bulle de la ligne grisée
  (`fileExplorer.navigation.favoritesAlreadyAdded`) nomme l'appartenance à la LISTE, pas une qualité du dossier ; le
  possessif reprend celui de `fileExplorer.navigation.favoritesEmpty` (« Vos favoris s'afficheront ici ») · `high`.
  C'est un constat, pas un refus : ni « erreur », ni « impossible », ni point final (aucune des info-bulles voisines
  n'en porte).
- **Open the favorite with that number → `Ouvrir le favori portant ce numéro`** · ligne en lecture seule de la liste des
  raccourcis, donc un CONSTAT au registre infinitif de ses voisines (`commands.volumeSelect.label` « Sélectionner le
  volume », `commands.volumeClose.label` « Fermer le sélecteur de volume ») · `high`. `numéro` (le rang attribué au
  favori), pas `chiffre` (la touche) : c'est le favori qui porte le numéro. Les chiffres 0–9 et les glyphes ⌘ ⌥ ⌃ ⇧ ne
  se traduisent jamais.
- **`…, and press a number to jump to that favorite` →
  `…, puis appuyer sur un chiffre pour aller au favori correspondant`** · deuxième ligne de la palette
  (`commands.favoritesOpen.description`), au moule des descriptions voisines : infinitif, `panneau actif`. Ici c'est
  bien `chiffre` (la touche qu'on presse) et non `numéro`, l'inverse de la clé du dessus · `high`. L'anglais disait « to
  go », sans destination, ce qui laissait « pour y aller » suspendu sur un `y` sans antécédent ; il nomme désormais le
  favori, et le français le nomme aussi. `correspondant` reprend l'idée de la colonne de touches, comme
  `commands.favoritesOpenByNumber.label` (« le favori portant ce numéro »). ❌ Pas « ouvrir … pour ouvrir » : `aller à`
  évite la répétition du verbe de tête.
- **`commands.favoritesAdd.description` nomme la vraie destination**, le menu des favoris (le sélecteur de volume n'a
  plus de section Favoris) :
  `Ajouter le dossier actuel du panneau actif aux favoris, pour y revenir ensuite depuis le menu des favoris.` · `high`.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Type`** · macOS Finder `fr`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Sélectionner tous les fichiers d'extension *.{extension}`** · Double
  Commander (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" →
  `Sélectionner tous les fichiers de même extension`) and Total Commander (`WCMD.INC` `527` →
  `Sélectionner les fichiers d'extension choisie`) name this exact command, and the mask replaces their "same extension"
  because Cmdr shows the concrete one · `high`. Le masque suit `d'extension`, donc rien ne s'accorde avec `{extension}`.
  L'apostrophe est simple dans `menu.*` (famille RAW) et doublée dans `commands.*` (ICU).
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Sélection`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` (« ajouter … à la sélection ») et le menu
  `Mark` de Double Commander `fr`, qui dit lui aussi `Sélection` · `high`. Its siblings in that menu are verbs; this one
  names what the submenu holds. ❌ Not the verb `Sélectionner`, which is `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## La pastille « accès complet au disque » de la barre de titre (`onboarding.fdaBadge.*`)

La pastille d'avertissement de la barre de titre et son infobulle, affichées tant que Cmdr n'a pas l'accès complet au
disque ; un clic rouvre la prise en main à l'étape 1.

- **`onboarding.fdaBadge.label` → `Pas d''accès complet au disque`** · Apple's own Réglages Système row (see the
  `Full Disk Access` entry above) · high. ICU key, so the apostrophe is doubled. `Aucun accès complet au disque` reads
  as a verdict; the `Pas de` form is the calm status the badge wants.
- **`onboarding.stepAi.bannerTitle.denied` already carried this exact string**, and `i18n-terms` holds the two identical
  because they share one English source. ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Pas d''accès complet au disque. Ouvrir l''étape Accès complet au disque de la prise en main.`), which is what
  satisfies `i18n-aria` (WCAG 2.5.3). ❌ Re-wording the label alone breaks it.
- **Gender**: the natural `Vous n'êtes pas obligé(e) de l'accorder` would expose an agreeing participle, so the tooltip
  opens `Ce n''est pas obligatoire, mais …` instead. Neutral restructuring, no typographic glyph.
- **Tooltip terms**: the benefit list is nominal after the colon
  (`la recherche dans tout votre disque, la lecture des dossiers cloud et la modification des fichiers que macOS garde pour lui`),
  which reads better in French than three infinitives; drive → `disque` (settled), cloud folders → `dossiers cloud`,
  "Click to …" → `Cliquez pour …` (as in `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `prise en main`
  (settled). Keep the space before the colon.

## The trash refusal dialog (`errors.write.trashRefused.*`, `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`, `errors.write.readError.title.trash`, `errors.write.writeError.title.trash`)

macOS turned down a move to the trash, and Cmdr words the refusal three ways (permission-shaped, no trash at that
location, unclassified). RAW family, so single apostrophes and `{count}` is a literal replacement target. Four rules
bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical: `Placement dans la corbeille impossible`, the queue row's noun plus the
  `<nom verbal> impossible` moule (never Nautilus's `Mise à la corbeille`, which the whole `errors.write.*` trash family
  had drifted to). ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. French solves it
  with `{count} des éléments que vous avez choisis`, which takes any numeral without agreement.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Détails techniques`), because it points at that very control.

- **locked → `verrouillé`** · macOS (`AXNODE1` `Verrouillé`) and the settled catalog term · high.
- **"delete them permanently" → `supprimer définitivement`** · matches `commands.fileDeletePermanently.label`, so the
  suggestion names the command the user will run · high.
- **badge (the title-bar pill) → `la pastille`** · reuses the settled `chip / badge (status pill) → pastille` · high.
  title bar → `barre de titre` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in `« … »` with the usual spaces. ❗ It is the ICU key
  that doubles its apostrophe (`Pas d''accès`); here, in the RAW `errors.*` family, the SAME text is written with a
  single `'` (`Pas d'accès`). `i18n-terms` normalizes that difference away, `i18n-icu` fails the other way round.
- "somewhere macOS keeps to itself" → `à un endroit que macOS garde pour lui`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## L'avertissement « contenu en ligne uniquement » (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Si un élément sélectionné d'un dossier cloud est disponible en ligne uniquement, la corbeille devrait d'abord le
télécharger. Cmdr ouvre donc la boîte de dialogue de suppression définitive et l'explique dans le bandeau. Deux
variantes du bandeau : une pour une sélection mixte, une pour une sélection entièrement en ligne. Elles ne diffèrent que
par la première phrase et par les issues qu'elles peuvent proposer. La troisième clé est la ligne affichée quand Cmdr
rend la main après un appui.

- **`.cloudOnlineOnlyMixedWarning`** · `en ligne uniquement` est la formule du Finder pour un fichier évincé ;
  `corbeille` et `service cloud` viennent de `terms.json` · tentative.
- **`.cloudOnlineOnlyAllWarning`** · même texte, avec « Tout ce que vous avez sélectionné » au lieu de « Une partie de
  votre sélection », et sans l'issue « désélectionner » : si tout est évincé, il ne resterait rien de sélectionné ·
  tentative.
- **`.cloudOnlineOnlyHandedBack`** · la ligne au-dessus du bouton après un appui que Cmdr n'a délibérément pas exécuté.
  Ton factuel, sans excuses · tentative.
- **Les quatre faits sont obligatoires** : (1) la corbeille téléchargerait les fichiers, (2) Cmdr ne propose donc que de
  supprimer TOUTE la sélection, (3) ensuite il n'y a AUCUNE copie dans la corbeille, même si le service garde la sienne
  (❌ ne pas adoucir), (4) les issues que le bandeau nomme.
- **Les deux zones `<strong>` restent**, sur « téléchargerait d'abord » et sur le verbe « supprimer ». Et « Supprimer »
  entre guillemets est le libellé du bouton : toujours le même mot que `fileOperations.delete.confirmDelete`.
- ⚠️ L'apostrophe ICU se double (`d''abord`).
- « make these files available offline » → `rendez d''abord ces fichiers disponibles hors connexion`, les mots de la
  commande `Rendre disponible hors connexion` vers laquelle le bandeau renvoie (jamais `hors ligne`).

## Quand le serveur répond qu'il n'a pas ce partage (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Le seul cas de la famille où réessayer ne sert à rien : le serveur répond clairement qu'aucun partage ne porte ce nom.
La notification n'a donc pas de bouton, et le ton ne doit rien suggérer de temporaire (ni « pour le moment », ni «
réessayez »), contrairement à ses sœurs.

- **« the server says it has no share by that name » → `car le serveur indique qu''il n''a aucun partage de ce nom`** ·
  NetAuthAgent `EINFO_NO_SHARE` (« Le partage « %@ » n'existe pas sur le serveur. », paquet installé, macOS 26.6.2,
  25G83, 2026-09-17) et le catalogue (`errors.mount.shareNotFound`) · high. Le cadre reste
  `La connexion directe à X n''a pas pu être établie`, repris de `fileExplorer.network.osMountFallback.message`.
- **`indique`, pas `dit`** · le serveur donne une réponse nette ; `indiquer` est le verbe de la terminologie Microsoft
  pour un système qui rapporte un état, et il évite la personnification familière · high.
- **« You are still connected » → `Vous y avez toujours accès`** · reprend la restructuration déjà fixée dans
  `fileExplorer.network.osMountFallback.message` (`Vous y avez bien accès`), qui nomme l'accès et non la personne ·
  high.
- **« This one won't sort itself out » → `Cette situation ne se réglera pas d''elle-même`** · aucune source dans le tas
  ; c'est la tournure française courante, et elle porte exactement ce qui distingue cette notification : attendre ne
  changera rien · high. Espace avant le `:` qui suit, comme partout dans le catalogue `fr`.
- **« may have been renamed or removed » → `a peut-être été renommé ou supprimé`** · littéralement
  `errors.write.destinationNotFound.suggestion` · high.
- **« so it's worth checking there » → `il vaut donc la peine d''y jeter un œil`** · registre familier et chaleureux du
  guide de style ; `y` évite de répéter `serveur` · high.
- **Dans la notification courte, `qui` rattache la relative à `ce partage`** · `il` serait ambigu, `{server}` étant
  masculin lui aussi · high. La fin `reste donc sur la connexion système` est celle des trois sœurs
  (`fileExplorer.pane.directConnectionUnreachableToast`…).

## Les noms « sosies » sur un serveur (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

Deux noms identiques à l'écran que le serveur enregistre avec des caractères différents (é composé ou décomposé, ou une
casse différente sur un serveur sensible à la casse). Les quatre clés doivent parler de la même chose avec les mêmes
mots.

- **« look the same » → `semblent identiques`** · ❌ jamais « se ressemblent à l'identique » (tournure bancale) ni « se
  ressemblent » seul (perd le « identiques » : ils ne se distinguent pas du tout) · high.
- **« spells / stores them spelled differently » → `les écrit différemment`** ; « an accented letter can be stored two
  ways » → `une lettre accentuée peut s'écrire de deux façons` · mots de tous les jours, comme l'exige la description
  (pas de « Unicode », « normalisation », ni « stocke écrits ») · high.
- **« upper and lower case » → `majuscules et minuscules`** · macOS dit « Sensible à la casse », mais « casse » est du
  jargon typographique ; la phrase explicative garde les mots courants · high.
- **Le bouton cité dans une phrase se met entre guillemets, avec la valeur exacte du bouton** : « Écraser » =
  `fileOperations.transferProgress.conflictOverwrite`, comme « Supprimer » ailleurs dans le catalogue · high.
- **« the one that's there » → `l'élément déjà présent`** · plus clair que « celui qui s'y trouve », dont l'antécédent
  flotte · high.
- **« Choose it from its folder » → `Choisissez plutôt l'élément voulu dans son dossier`** · ❌ pas « Choisissez-le » :
  l'antécédent le plus proche est « plusieurs éléments » (pluriel), le pronom ne renvoie à rien. `dans son dossier` suit
  le Finder (« dans le dossier de destination ») · high.
- **Le constat « so Cmdr didn't pick one » passe après un deux-points : `… : Cmdr n'en a donc choisi aucun.`** · même
  moule dans l'explication du panneau et dans la notification courte ; la virgule d'avant faisait une phrase soudée ·
  high.

## L'interrupteur « Autoriser l'IA dans le cloud » et les états où l'IA dans le cloud est désactivée (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Un interrupteur de confidentialité : tant qu'il est désactivé, Cmdr n'envoie rien à un service d'IA dans le cloud. Ton
calme, sans jamais promettre plus que ce que fait Cmdr. La pile de référence n'était pas sur la machine de traduction ;
chaque choix s'appuie sur des termes déjà documentés dans `terms.json` et sur le catalogue.

- **Allow cloud AI (nom de l'interrupteur) → `Autoriser l'IA dans le cloud`** · `IA dans le cloud` est l'option
  `settings.ai.provider.opt.cloud` ; `autoriser` vient de macOS Finder (allow → `Autoriser`, fixé plus haut) et le moule
  `Autoriser <objet>` de `settings.fileOperations.allowFileExtensionChanges.label` · `high`. Cité entre guillemets « … »
  dans `settings.ai.cloudConsent.lockedHint`. Là où l'anglais emploie « Allow cloud AI » comme verbe
  (`askCmdr.gate.cloudOff.body`, `settings.askCmdr.cloudOffHint`), on écrit `Autorisez … l'IA dans le cloud` : les mêmes
  mots, conjugués.
- **cloud AI (dans la phrase) → `l'IA dans le cloud`**, pronom `la` (`Autorisez-la dans Réglages > IA`) · `high`.
- **« X is off » → `X est désactivé(e)`** · comme `servers.hub.discoveryOff` · `high`.
- **Turn on Ask Cmdr (bouton) → `Activer Ask Cmdr`** · infinitif de bouton, comme
  `fileExplorer.navigation.driveIndex.menuEnable` · `high`.
- **Open AI settings → `Ouvrir les réglages d'IA`** · comme `commands.appSettings.label` (« Ouvrir les réglages ») ·
  `high`.
- **side panel → `panneau latéral`** · `tentative` (pas de précédent dans le catalogue).
- **custom endpoints → `points de terminaison personnalisés`** · `point de terminaison` déjà dans
  `onboarding.cloudSetup.hint.azureEndpoint` · `high`.
- **Ask Cmdr chats → `conversations Ask Cmdr`** · chat (nom) → `conversation`, fixé pendant la passe `ask-cmdr` ·
  `high`.
- `settings.askCmdr.enabled.label` n'est plus que « Ask Cmdr » (le nom du produit sur l'interrupteur) et porte une
  `sameAsSourceJustification`.

## Échap et le plein écran (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

La notification unique qui suit la sortie du plein écran par Échap, et l'interrupteur correspondant dans Réglages >
Avancé > Saisie.

- **full screen (le mode fenêtre de macOS) → `mode plein écran`, ou `plein écran` quand la phrase le permet** · macOS
  Finder (`FV20` « Quitter le mode plein écran », `FV21` / MenuBar `300944.title` « Activer le mode plein écran »,
  `fr/macOS/Finder/`, tas de références relevé le 2026-09-23) · `high`.
- **Exit full screen on Escape (interrupteur) → `Quitter le mode plein écran avec Échap`** · reprend mot pour mot la
  commande Finder « Quitter le mode plein écran » ; le libellé de la notification (`switchLabel`) et celui des réglages
  sont identiques · `high`.
- **Escape (la touche) → `Échap`, `la touche Échap` en première mention** · terminologie Microsoft FRA (`ESC key` → «
  touche Échap », id 51841/51844) ; Total Commander écrit `<Échap>` · `high`. Le raccourci
  `shortcuts.section.pressEscToClear` écrit la capsule « ÉCHAP » en capitales ; en prose on garde `Échap`.
- **You'll only see this once → `Vous ne verrez ce message qu'une seule fois.`** · `tentative` (pas de source, formule
  idiomatique).
- **dialog or menu (reprise par un pronom) →
  `Si une boîte de dialogue ou un menu est ouvert, Échap se contente de le fermer.`** · accord au masculin (règle du «
  ou » avec un nom masculin), ce qui évite « celui-ci/celle-ci » · `high`.
- Le drapeau interne suit le moule `oldMacosNoticeShown` et l'astuce `doubleClickOnPaneNotificationSeen` : « Astuce «
  Échap et plein écran » affichée » (hint → astuce, féminin).

## Originaux modifiés pendant le déplacement (`transfer.changedDuringMove`, 2026-09-25)

- **« changed during the move » → `a changé / ont changé pendant le déplacement`** · voix active, comme
  `fileOperations.cancelRollback.reason.drift.counted` (« ils ont changé ») ; le Finder `PE56` écrit au passif « ont été
  modifiés au cours de la gravure » (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25), mais le
  catalogue préfère l'actif. `pendant le déplacement` et `dossiers source` repris mot pour mot du voisin
  `transfer.appearedDuringMove`, affiché dans la même notification · `high`.

## Lignes d'attente de « Ouvrir avec » et « Partager » (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Lignes d'attente de « Ouvrir avec » et « Partager » (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- **Finding apps… → `Recherche d'apps…`** · nom verbal comme macOS (« Searching… » = « Recherche… »), `apps` comme
  `settings.behavior.textEditorApp.checking` ; apostrophe ASCII simple (clé native) · `high`.
- **share options → `options de partage`** · `partage`, le nom du verbe du sous-menu `Partager` · `high`.
- **No share options → `Aucune option de partage`** · moule de macOS pour un menu vide (« No Services Apply » = « Aucun
  service adéquat ») · `high`.
