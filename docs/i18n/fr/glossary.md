# fr glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/fr/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

Settled during the `fileExplorer` pass (2026-06-21):

- host → hôte · macOS Finder ("Serveurs favoris", "Adresse du serveur"), MS terminology FRA ("ordinateur hôte") · high
- hostname → nom d''hôte · MS terminology FRA; macOS uses "Adresse du serveur" for the address field · high
- mount (verb) / mounting → monter / montage · macOS AppKit ("Le volume « %@ » n''a pas pu être monté.") · high
- share (network, noun) → partage · macOS ("Partage et permissions"), Nautilus ("dossier partagé"), Dolphin ("dossier
  partagé") · high
- guest → invité · macOS AppKit ("NSUserGuest" → "invité", "Se connecter comme…") · high
- sign in → s''identifier · macOS ("Saisissez le nom d''utilisateur"); calmer than "se connecter" which is reserved for
  the network connect action · high
- connect (to server) → se connecter · macOS Finder ("Connexion au serveur", "Se connecter comme…") · high
- credentials → identifiants · standard FR UI term; macOS frames sign-in as "nom d''utilisateur"/"mot de passe"; MS
  "infos de connexion" is the consumer-account sense, "identifiants" fits SMB sign-in better · high
- username → nom d''utilisateur · macOS Finder ("Saisissez le nom d''utilisateur ou de groupe") · high
- password → mot de passe · macOS, pervasive · high
- read-only → en lecture seule · macOS Finder ("Cet emplacement est en lecture seule.") · high
- refresh (rescan) → actualiser · macOS AppKit ("NSRefreshTemplate" → "actualiser") · high
- pinned / pin → épinglé / épingler · macOS ("onglet épinglé", "Épingler l''onglet") · high
- symbolic link (symlink) → lien symbolique · Nautilus ("liens symboliques") · high
- broken symlink → lien symbolique rompu · "rompu" for a broken/dangling link (Nautilus/Dolphin family) · high
- permission denied → autorisation refusée · macOS Finder ("vous ne disposez pas de l''autorisation…") · high
- timeout → délai dépassé · macOS pattern ("délai… dépassé"); calmer than "expiré". Covers the timed-out STATUS sense
  everywhere it surfaces, including a request that times out (`ai.translateError.timeout.body` → "Le délai de la requête
  a été dépassé.", NOT "a expiré"). Distinct from the WAIT-DURATION setting sense "délai d''attente" (see the settings
  pass) and from licence/subscription expiry, which legitimately uses "a expiré". · high
- unreachable → inaccessible · standard FR; macOS uses "inaccessible" for unreachable resources · high
- empty folder → dossier vide · Nautilus/Dolphin family · high
- browse (servers/network) → parcourir · macOS Finder ("Parcourir les serveurs disponibles") · high
- home folder → dossier personnel · macOS Finder convention · high
- on disk → sur le disque · pairs with "Contenu" for the content-vs-physical size split · high
- jump (type-to-jump) → aller à · neutral navigation phrasing · tentative
- error (non-alarmist status) → problème · style guide steers away from "erreur"; "problème" is the calm fallback for
  the generic ⚠️ Error status · tentative

UI section names referenced (keep consistent in other files):

- Settings → Réglages · macOS modern naming (per style guide) · high
- Keyboard shortcuts (Settings section) → Raccourcis clavier · macOS convention · high
- Quick Look → Coup d''œil · macOS French · high — Apple FEATURE name that Apple localizes per-OS, so use the term the
  user sees in their French Finder, NOT the English "Quick Look". The lowercase generic action "quick view" → "aperçu
  rapide".
- Keychain (credential store) → trousseau; Keychain Access (app) → Trousseaux d''accès · macOS French · high — Apple
  FEATURE name that Apple localizes per-OS (same principle as Quick Look), so use the term the user sees in their French
  macOS, NOT the English "Keychain". "Keychain" is NOT on the don''t-translate brand list; any earlier "keep Keychain
  verbatim" note is superseded by this. The store sense ("saved in/access to the Keychain") → "le trousseau"; the app
  name ("open Keychain Access") → "Trousseaux d''accès". `ai.secretError.*` already uses "Trousseau macOS" / "Trousseaux
  d''accès"; `fileExplorer` store-sense strings use "le trousseau".

Settled during the `settings` pass (2026-06-21):

- settings → réglages · macOS "Réglages Système" (SystemSettings `CFBundleName`) · high
- tint → teinte (verb teinter) · descriptive FR; matches macOS color usage · high
- timeout (a configurable WAIT-DURATION setting, e.g. "Network timeout mode") → délai d''attente · macOS / MS FRA list
  "délai d''attente". NOTE the sense split: the fileExplorer pass uses "délai dépassé" for the _timed-out status_; this
  "délai d''attente" is the _duration you wait_. Keep both senses distinct. · high
- threshold → seuil · MS terminology FRA (`>threshold<`→"seuil") · high
- buffer → tampon (mémoire tampon) · MS terminology FRA · high
- word wrap → retour automatique à la ligne · MS terminology FRA (id 134158, geo FRA/BEL/CAN/CHE) · high
- viewer (built-in file viewer) → visionneuse · MS terminology FRA ("Visionneuse"); avoid "lecteur" (drive/player sense)
  · high
- logging → journalisation; log file → fichier journal · MS terminology FRA · high
- reset → réinitialiser · MS terminology FRA · high
- provider (AI) → fournisseur · MS terminology FRA · high
- toast / transient notification → notification · no separate FR UI term; rendered plainly, kept calm · high
- chip / badge (status pill) → pastille · descriptive FR · tentative (no exact reference-pile hit)
- Full Disk Access → Accès complet au disque · standard Apple French TCC name; NOT in the bundled reference pile (lacks
  privacy-pane TCC strings) · tentative — flag for review
- Local Network (permission) → Réseau local · standard Apple French TCC name; same caveat; injected via `{localNetwork}`
  at runtime anyway · tentative
- System Settings → Réglages Système (capital S) · the macOS app''s own `CFBundleName` is "Réglages Système", and the
  reference pile maps both "System Settings" and "System Preferences" to it (capital S); lowercase "réglages système"
  only appears mid-sentence as a common noun ("la sous-fenêtre des réglages système"). When NAMING the app or a Réglages
  Système > … breadcrumb, use capital S. Settled catalog-wide on this form. · high
- Appearance (macOS pane) → Apparence · macOS SystemSettings · high
- startup disk → disque de démarrage · macOS Finder ("Startup Disk…"→"Disque de démarrage…") · high
- striped rows → lignes alternées · descriptive FR · high
- wilting (date-color metaphor) → Flétrissement · descriptive FR for the plant metaphor · tentative (Cmdr coinage)

Settings section names (keep consistent across catalog files):

- Appearance → Apparence; Colors and formats → Couleurs et formats; Zoom and density → Zoom et densité; File and folder
  sizes → Tailles de fichiers et de dossiers; Listing → Liste; Behavior → Comportement; File operations → Opérations sur
  les fichiers; File system watching → Surveillance du système de fichiers; Search → Recherche; AI → IA; File systems →
  Systèmes de fichiers; SMB/Network shares → Partages SMB/réseau; MTP → MTP; Git → Git; Viewer → Visionneuse; Developer
  → Développeur; MCP server → Serveur MCP; Logging → Journalisation; Updates & privacy → Mises à jour et
  confidentialité; Advanced → Avancé; Keyboard shortcuts → Raccourcis clavier; License → Licence
- View modes: Full → Complet; Brief → Bref (mode Bref). Columns: Name → Nom; Ext → Ext (kept short)

Settled during the `errors` pass (2026-06-21, friendly-error catalog: listing, git, provider, write errors). RAW strings
here, so single apostrophes in the actual values; doubled below only to match this doc's convention:

- retry / try again → réessayer · macOS Finder ("Réessayer" / "réessayez", 18+ hits) · high
- authentication required → authentification requise · macOS (verbatim) · high
- not found / path not found → introuvable / chemin introuvable · macOS ("introuvable", 37+ hits) · high
- disk is full → le disque est plein · macOS ("disque est plein") · high
- locked (file) / unlock → verrouillé / déverrouiller · macOS ("L''élément est verrouillé") · high
- Get Info (Finder menu) → Lire les informations · macOS Finder · high
- Activity Monitor → Moniteur d''activité · macOS app name · high
- Disk Utility → Utilitaire de disque · macOS app name · high
- First Aid → S.O.S · macOS Disk Utility''s "First Aid" renders "S.O.S" in French · high
- handle (open file handle) → handle · kept as-is, no concise FR equivalent in the pile (deletePending strings) ·
  tentative
- error-title pattern "Couldn''t/Can''t X" → noun-phrase "… impossible" · macOS ("Impossible d''ouvrir/de graver…");
  used as "Lecture du dossier impossible", "Accès à cet emplacement impossible" to stay calm and avoid "erreur/échec" ·
  high

Phrasing notes for this catalog:

- "Here''s what to try:" → "Voici ce que vous pouvez essayer :" (regular ASCII space before the colon, per the
  catalog-wide settled spacing rule — see style.md § Punctuation spacing), leading every bullet-list suggestion.
- `errors.write.*` carries `{verb}` / `{Verb}` / `{gerund}` placeholders that the current code fills with ENGLISH words
  ("copy" / "Copy" / "copying" / "move to trash") — the verb map in `transfer-error-messages.ts` is not localized yet.
  French wraps them as "l''action {verb}" / "{Verb} a échoué" so the sentence stays grammatical, but the placeholder
  content renders in English at runtime. Flagged for review; matches how the `de` sibling handled it.
- The OS-pane placeholders (`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}`, `{full_disk_access}`)
  are substituted with OS-localized names at runtime — left as tokens, not translated. The git `permissionDenied` and
  `gitDirPermissionDenied` suggestions intentionally keep the pane names as English literals ("System Settings > Privacy
  & Security > Files and Folders") to match the original git copy, NOT placeholders; preserved verbatim.

Settled during the `licensing` + `ai` + `viewer` pass (2026-06-21):

- clipboard → presse-papiers · macOS Finder ("Afficher le presse-papiers", "presse-papiers") · high
- copy / paste / select all → copier / coller / tout sélectionner · macOS Finder MenuBar ("Copier", "Coller", "Tout
  sélectionner") · high
- encoding (character) → encodage · MS terminology FRA ("Encodage", "codage de caractères"); macOS uses "encodage" ·
  high
- reload (file changed on disk) → recharger · standard FR; distinct from "actualiser" (rescan a listing) · high
- match (search result) → correspondance · MS terminology FRA ("correspondance…") · high
- regex (short UI label) → Regex · kept short per the @key note; long form is "expression régulière" (MS FRA) · high
- detected (auto-detected encoding) → détecté · macOS/MS pattern ("détection automatique", "détecté") · high
- viewer (built-in file viewer) → visionneuse · MS terminology FRA; matches the settings-pass choice · high
- tail / follow (auto-follow a growing file) → suivre / suivi · descriptive FR ("Mode suivi : suivre les changements");
  matches de "Folgen" · tentative
- word wrap (viewer badge/hint) → retour ligne (badge, kept short) / retour à la ligne · MS FRA "retour automatique à la
  ligne", trimmed for the tight badge/hint slots · high
- streaming (large-file viewer mode) → streaming · kept verbatim, no concise FR equivalent in the pile; matches de ·
  tentative
- license → licence; license key → clé de licence · standard FR (licence is feminine, drives "Commerciale perpétuelle")
  · high
- Personal / Commercial (license tiers) → Personnelle / Commerciale · agree with feminine "licence" ("licence
  personnelle", "licence commerciale perpétuelle") · high
- subscription → abonnement · standard FR · high
- perpetual (license) → perpétuelle · standard FR, agrees with "licence" · high
- provider (AI service) → fournisseur · matches the settings-pass choice; MS terminology FRA · high
- endpoint (API) → point de terminaison · MS terminology FRA · high
- API key → clé d''API · standard FR · high
- rate-limiting → limiter le débit (des requêtes) · MS terminology FRA · high
- quota → quota · identical in FR · high
- runtime (AI runtime to extract) → environnement d''exécution · MS terminology FRA · high
- AI → IA · matches the settings-pass section name · high
- Apple Silicon → Apple Silicon · brand/hardware name, kept verbatim · high

Phrasing notes for this pass:

- "Active" (license validity / status) stays "Active" in FR — identical spelling (feminine of "actif"), legitimately
  unchanged; flagged by the coverage check as identical-to-English but correct.
- The kind words "image" / "document" (binary-view warning) and "Image" / "PDF" / "Unicode" (view-mode labels) are
  identical or near-identical in FR; left unchanged on purpose.
- `viewer.saveAs.defaultName` → "selection" kept as a file-name-safe literal (the @key says lowercase, no spaces, safe
  as a file name), so not translated.
- License-tier labels: "Commercial perpetual" → "Commerciale perpétuelle", "Commercial subscription" → "Abonnement
  commercial", "Personal (free)" → "Personnelle (gratuite)". The standalone "Commercial perpetual" type value drops the
  noun, so the adjective agrees with the implied feminine "licence".

Settled during the `queryUi` + `commands` pass (2026-06-21):

- search (verb) → rechercher; (noun) recherche · macOS Finder ("Rechercher", "Rechercher dans le Finder") · high
- pattern (match pattern) → motif · macOS pattern; "motif" is the FR UI term for a match pattern (avoid "modèle") · high
- regular expression → expression régulière · MS terminology FRA (id 147617); "regex" kept verbatim as the short
  chip/label form · high
- wildcard → caractère générique · standard FR UI term; `*` / `?` glyphs stay literal · high
- scan / scanning (index build) → analyse / analyse en cours · standard FR; pairs with "index" (indexation) · high
- scope (search-in folders) → portée · standard FR UI term for a search/effect scope · tentative
- view (the View MENU + view mode) → présentation · macOS Finder ("Présentation", "Par liste") — so "Switch to
  Brief/Full view" → "présentation Bref/Complet", and the "View > Zoom" menu path renders "Présentation > Zoom" · high
- zoom in / zoom out → zoom avant / zoom arrière · MS terminology FRA ("zoom avant" id 2131086, "zoom arrière"
  id 135725) · high
- sort ascending / descending → ordre croissant / décroissant · GNOME Nautilus (po: "croissant"/"décroissant"), Double
  Commander · high
- paste → coller; cut → couper; clipboard → presse-papiers · macOS (pervasive: "Coller", "Couper", "Presse-papiers") ·
  high
- new tab → nouvel onglet; next/previous tab → onglet suivant/précédent; close tab → fermer l''onglet · macOS Finder
  ("Nouvel onglet", "Afficher l''onglet suivant/précédent", "Fermer l''onglet") · high
- quit (app) → quitter; hide → masquer; show all → tout afficher; select all → tout sélectionner; deselect all → tout
  désélectionner · macOS app menu (verbatim) · high
- Get info (Finder) → Lire les informations; Show in Finder → Afficher dans le Finder; Quick Look (action) → Coup d''œil
  · macOS Finder (verbatim) · high
- about → à propos (de) · macOS ("À propos du Finder") · high
- command palette → palette de commandes · descriptive FR; "palette de commandes" reads naturally and matches the VS
  Code FR convention · high
- onboarding → prise en main · MS/Apple FR convention for guided first-run setup (avoid the anglicism "onboarding") ·
  high
- feedback → retour · RESOLVED catalog-wide to "retour" (style-guide friendly register: "Envoyer un retour"), NOT
  "commentaire". The whole `feedback.*` dialog, the "Aide > Envoyer un retour…" menu path, and the
  `commands.feedbackSend.label` command all use "retour"; the earlier "commentaire(s)" rendering of the command label
  was drift and is fixed. MS FRA "Commentaires" exists but loses the warmth. · high
- what''s new → nouveautés · standard FR app-menu term (macOS/MS) · high
- parent folder → dossier parent · macOS Finder ("Accéder au dossier parent") · high
- page up / page down → page précédente / page suivante · descriptive FR (the keys map to scrolling a page) · tentative
- offline (make available offline) → hors connexion · standard FR (iCloud Drive FR: "disponible hors connexion") · high
- cursor (file-list cursor) → curseur · standard FR · high

Phrasing notes for this catalog:

- Zoom percentages: "Zoom à 100 %" and "Zoom augmenté à {size} %." use a regular ASCII space before "%" (catalog-wide
  settled spacing, see style.md § Punctuation spacing). The literal "%" in the source string is kept; only the FR space
  is added before it.
- "{Verb}/{verb}" placeholders don''t occur in these two files; no English-verb-leak issue here (that''s
  `errors.write.*`).

Settled during the `fileOperations` + `onboarding` pass (2026-06-21). ICU values, so single apostrophes doubled below to
match this doc's convention:

- skip → ignorer · macOS Finder/AppKit ("Ignorer" pervasive), Nautilus ("\_Ignorer") · high
- skip all → tout ignorer · composed from "ignorer"; matches the "Tout éjecter" all-variant pattern · high
- overwrite → écraser · style guide (macOS "Écraser à la destination") · high
- overwrite all → tout écraser · composed; same all-variant pattern · high
- replace → remplacer · macOS Finder ("Souhaitez-vous le remplacer…"), Nautilus ("\_Remplacer") · high
- merge (folders) → fusionner · Nautilus ("\_Merge"→"\_Fusionner", "Fusionner le dossier") · high
- conflict → conflit · Nautilus ("créerait un conflit avec un fichier existant") · high
- rollback (undo a transfer) → revenir en arrière (verb) / retour en arrière (noun) · not `restaurer` (that's Restore,
  and rollback doesn't bring an overwritten file back) and not `annuler` (that's Cancel, and `Annulé` is already
  `operationLog.status.canceled`) · tentative · full arbitration over the 14 keys: § « La famille `rollback` » at the
  end of this file
- destination → destination · macOS Finder ("${destinationFolder}" framing); same word · high
- target (of a link / a clash) → cible · standard FR; macOS uses "cible" for link targets · high
- free of (space) → libre sur · macOS Finder ("Disponible :"); "{free} libre sur {total}" reads natural · high
- remaining (ETA) → restant · macOS Finder ("Estimation du temps restant…") · high
- endpoint (API) → point de terminaison · MS terminology FRA standard · high
- model (AI) → modèle · standard FR · high
- provider (AI/cloud) → fournisseur · matches the `settings` pass · high
- batch rename → renommage par lot; mass-rename → renommage en masse · descriptive FR · tentative
- command palette → palette de commandes · descriptive FR; common app convention · high
- issue (GitHub) → ticket · common FR rendering of a GitHub issue · tentative
- feedback → retour · style guide friendly register; "Envoyer un retour" for "Send feedback" · high
- under cursor → sous le curseur · descriptive FR · high
- dir (abbrev. of directory in scan stats) → rép. (abbrev. of répertoire) · keeps the source''s short form. The
  standalone status-bar marker `fileExplorer.selectionInfo.dir` (en "DIR", rendered in capitals) uses the capitalized
  form "RÉP." to honor both this abbrev and the source''s all-caps marker style; it formerly read "DOSS" (a "dossier"
  coinage), which was drift from this "rép." choice and is fixed. · high
- scanning (transfer stage) → analyse · descriptive FR for the count-files phase · high
- source-available → consultable publiquement · descriptive FR (no settled term); conveys "code can be viewed" without
  implying open-source · tentative

Onboarding-specific phrasing:

- full disk access → accès complet au disque · descriptive FR; NOT the bundled reference pile (lacks the TCC pane
  string) but the standard Apple French rendering — same caveat as the `errors`/`settings` pass on TCC names · tentative
  — flag for review
- "Quit & Reopen" (macOS relaunch button) → "Quitter et rouvrir" · macOS shows this button itself; standard French label
  · tentative — verify exact macOS wording
- onboarding (the flow) → not surfaced as a noun; rendered as "configuration" where needed ("Progression de la
  configuration"), and the wizard title became "Bienvenue dans Cmdr" · tentative
- The `stuck`-banner breadcrumb keeps "Privacy &amp; Security &gt; Full Disk Access" as English literals (like the git
  pane breadcrumb), since the `{systemSettings}` token is the only OS-localized part; matches the source.

Settled during the `search` + `feedback` + `crashReporter` + `goToPath` + `transfer` + `updates` + `lowDiskSpace` +
`commandPalette` + `whatsNew` + `common` + `notifications` + `main` pass (2026-06-21). ICU values, so single apostrophes
doubled below to match this doc's convention:

- show all → tout afficher · macOS AppKit ("Show All" → "Tout afficher") · high
- restart (app) → redémarrer · macOS Menus ("Restart" → "Redémarrer") · high
- later (dismiss-for-now button) → plus tard · standard FR (iOS/iCloud "Plus tard"); no clean Finder hit · high
- go to path / path → aller au chemin / chemin · macOS uses "chemin" for a filesystem path · high
- checking (update check in progress) → vérification · standard FR · high
- changelog → journal des modifications · standard FR (VS Code/GitHub FR convention) · high
- new version available → nouvelle version disponible · macOS pattern ("disponible") · high
- send feedback → envoyer un retour · matches the `fileOperations` pass ("retour"); the dialog title and the submit
  button both render "Envoyer un retour"/"Envoyer le retour" · high
- crash report → rapport d''incident; "send crash report" → "envoyer le rapport d''incident" · style guide (Apple
  "rapport d''incident", non-alarmist) · high
- error report (the report-sending flow) → rapport d''incident · same flow as crash reports; kept consistent · high
- "Error:" prefix (non-alarmist toast) → "Problème :" · style guide steers away from "erreur"; matches the `errors` pass
  "problème" calm fallback. Applied to user-facing toasts (`updates.checkToast.errorPrefix`, `ai.cloud.unknownError`,
  etc.). EXCEPTION: `settings.updates.errorPrefix` stays "Erreur :" — its `@key` description explicitly marks it a
  developer/diagnostic label where "Error" is acceptable. · high
- running low on space → l''espace libre devient faible / espace disque faible · descriptive FR, calm; pairs with
  "disque de démarrage" · high
- free (space, adj.) → libre(s) · macOS Finder ("Disponible"/"libre") · high
- onboarding (menu item "Onboarding…") → "Prise en main…" · RESOLVED. The command/menu label is
  `commands.cmdrOpenOnboarding.label` = "Prise en main…" and `shortcuts.scope.onboarding` = "Prise en main", so the
  `main.upgradeNudge.*` menu path "Cmdr > Prise en main…" was aligned to match (it formerly read "Configuration…", a
  forward-reference guess that diverged from the actual menu label). The generic phrase "onboarding options" still
  renders descriptively as "options de configuration"; only the literal menu-item label is "Prise en main…". · high
- Downloads folder → dossier Téléchargements · macOS Finder ("Téléchargements") · high

Phrasing notes for this pass:

- `transfer.*` plurals written with FR CLDR `one`/`many`/`other`; past participles agree masculine ("fichier
  copié"/"fichiers copiés", "dossier déplacé"/"dossiers déplacés") since "fichier"/"dossier" are masculine. The
  `movedPhrase` fragment is built so each `kind` branch stands alone grammatically.
- `feedback.dialog.counter` ("{currentText} / {maxText}") is pure-placeholder, legitimately identical to English.
- Regular ASCII space before `:` and `%` and `?` per the catalog-wide settled spacing rule ("Identifiant du rapport :",
  "({percentText} %)", "Envoyer le rapport d''incident ?"). See style.md § Punctuation spacing.
- `whatsNew.dialog.title` keeps the source's curly apostrophe context (none in FR rendering) — "Nouveautés de Cmdr".
- Settings-section cross-refs kept consistent: "Réglages > Mises à jour" and "Réglages > Mises à jour et
  confidentialité" per the settings-pass section names.

Settled during the `indexing` + `downloads` + `errorReporter` + `shortcuts` + `mtp` + `ui` pass (2026-06-21). ICU
values, so single apostrophes doubled below to match this doc's convention:

- entry (file-or-folder scan unit) → élément · matches the `item → élément` choice; "{entriesText} éléments" in scan
  counters · high
- dirs (abbrev. of directories, compact status) → rép. (abbrev. of répertoires) · matches the `fileOperations` pass dir
  abbrev · high
- event (recorded filesystem change) → évènement · standard FR (modern spelling) · high
- roughly (ETA qualifier) → environ · standard FR · high
- almost done (ETA) → bientôt terminé · calm, reassuring FR · high
- fresh scan / rescan → nouvelle analyse · pairs with "analyse" (scan); "Une nouvelle analyse est en cours…" for the
  rescan toasts · high
- watcher (file-change watcher) → surveillant (des modifications de fichiers) · descriptive FR; "surveillance" already
  used for the FS-watching setting section · high
- buffer / channel overflow → saturé (a saturé le tampon / le canal) · descriptive calm FR; avoids "débordement" alarm ·
  high
- index (drive index) → index; indexing → indexation; indexer (verb) → indexer · matches style-guide glossary · high
- jump to (a download/file) → aller à · matches the `queryUi` "aller à" choice · high
- download (noun) → téléchargement; latest/most recent download → dernier / le plus récent téléchargement · style-guide
  glossary · high
- global shortcut / globally → raccourci global / globalement · standard FR for a system-wide hotkey · high
- in-app → dans l''app · concise FR; "app" kept (common FR usage, matches catalog) · high
- modifier (key) → touche de modification · macOS FR convention; the ⌘⌃⌥⇧ glyphs stay literal · high
- register (a global hotkey) → enregistrer; registered/not registered → enregistré / non enregistré · standard FR · high
- combo (key combination) → combinaison · "combinaison" for a key combo in conflict warnings · high
- error report (the report flow) → rapport d''incident · matches the `crashReporter` flow; "incident" stays non-alarmist
  (Apple) · high
- reference ID → identifiant de référence · standard FR · high
- redact / scrub (logs) → expurger / effacer; redaction → expurgation · standard FR for privacy-stripping logs · high
- manifest (report metadata) → manifeste · standard FR technical term · high
- sample (of log lines) → échantillon · standard FR · high
- bundle (report bundle to disk) → lot · descriptive FR; kept consistent across the saveToDisk/saveFailed strings ·
  tentative
- preview (report preview) → aperçu · macOS "Aperçu" convention · high
- daemon (system daemon) → daemon · kept verbatim (no concise FR equivalent; macOS keeps it);
  ptpcamerad/udev/Terminal/Ctrl+C also verbatim · high
- exclusive access (to a device) → accès exclusif · standard FR · high
- USB device → appareil USB; "Retry connection" → "Réessayer la connexion" · standard FR · high

Keyboard-shortcut / macOS feature names (shortcuts.json — reuse macOS French wording; brand names verbatim):

- Spotlight → Spotlight; Mission Control → Mission Control; Spaces → Spaces · macOS keeps these verbatim in French
  (reference pile: NSTouchBar templates) · high
- Force Quit → Forcer à quitter · macOS AppKit ("Force Quit…" → "Forcer à quitter…") · high
- Character Viewer → Visualiseur de caractères · standard macOS FR name · high
- Finder search window → fenêtre de recherche du Finder · descriptive FR; "Finder" verbatim · high
- App windows → Fenêtres de l''application; the app switcher → le sélecteur d''applications · descriptive FR macOS
  feature names · high
- input source switching → le changement de source de saisie; screen recording → l''enregistrement de l''écran;
  screenshots → les captures d''écran; logging out → la déconnexion; locking the screen → le verrouillage de l''écran ·
  descriptive FR, lowercase mid-sentence per the source · high
- scope group headings (shortcuts) → App → Application; Main window → Fenêtre principale; File list → Liste des
  fichiers; Brief/Full mode → Mode Bref/Complet; Volume chooser → Sélecteur de volume; Share browser → Navigateur de
  partages; Command palette → Palette de commandes; About window → Fenêtre À propos; Onboarding → Prise en main · high
- Fixed (badge, hardcoded key) → Fixe · descriptive FR; "Modified" filter chip → "Modifiés" · high

Phrasing notes for this pass:

- ICU plurals use FR CLDR `one`/`many`/`other`; `many` written identical to `other` for the line-count and file-count
  messages (plain integers never select `many`, but the parity check requires the branch). Past participles agree
  masculine: "fichier chargé"/"fichiers chargés".
- `errorReporter.dialog.counter` ("{currentText} / {maxText}") is pure-placeholder, legitimately identical to English
  (same as the `feedback` counter).
- `shortcuts.section.alreadyBound` quotes the command with French guillemets « {command} » (the source uses straight
  quotes ''{command}''); `<b>` tag preserved.
- Regular ASCII space (0x20) before `:` / `?` / `!` / `%`, the catalog-wide settled spacing (style.md § Punctuation
  spacing); never U+202F.
- Legitimately identical-to-English in fr: "Global" (downloads scopeTitle, valid FR), "OK" (mtp/ui), "macOS" (badge,
  brand), "Options" (ui popover, identical FR), and the Spotlight/Mission Control/Spaces brand feature names.

Settled during the `queue` + new `fileOperations`/`commands` pause-queue-background keys pass (2026-06-21). ICU values,
so single apostrophes doubled below to match this doc's convention:

- pause (verb) → mettre en pause; pause (noun / button label) → Pause; paused (status) → en pause / En pause · macOS
  ("NSPauseTemplate" → "pause", "Pause" → "Pause", "Mettre en pause toutes les animations"), Double Commander ("&Pause
  all" → "Mettre tout en pause", "Paused" → "En pause") · high — the standalone "Pause" button label is legitimately
  identical to English (it's also valid FR; macOS keeps "Pause").
- resume → reprendre · macOS Finder ("Resume" → "Reprendre", "Reprendre la copie"), Double Commander ("&Resume" →
  "Reprendre") · high — calm, the Apple/file-manager term for continuing a paused transfer.
- pause all → tout mettre en pause; resume all → tout reprendre · composed from the above; "tout mettre en pause"
  matches Double Commander's "&Pause all" → "Mettre tout en pause" (reordered to the "Tout éjecter"/"Tout ignorer"
  all-variant pattern used catalog-wide) · high
- queue → file d''attente · Double Commander ("Queue" → "File d''attente", "Add To Queue" → "Ajouter à la file
  d''attente", pervasive), MS terminology FRA ("file d''attente", 36+ hits) · high — the head noun "file d''attente"
  still stands; the standalone Queue button on the progress dialog is still "File d''attente". **SUPERSEDED, the
  QUALIFIER only**: "Transfer queue" → "File d''attente des transferts" is no longer the window''s name. English widened
  the product noun to "Operation queue", so the qualifier is now "des opérations" — see the operation-queue-rename pass
  at the end of this file.
- background / send to background (keep a transfer running while the user works) → arrière-plan / en arrière-plan ·
  Double Commander ("Work in background" → "Travailler en arrière-plan", "in the &background" → "en arrière-plan"),
  Total Commander ("en arrière-plan"), MS terminology FRA ("arrière-plan", 79+ hits) · high — "Keep this running in the
  background" → "Garder ce transfert en cours en arrière-plan".

Phrasing notes for this pass:

- `queue.row.status` "Couldn''t finish" (the gentle non-alarmist wording for a failed op) → "N''a pas pu se terminer",
  staying away from "erreur"/"échec" per the style guide. "Waiting" (queued) → "En attente"; "Done" → "Terminé";
  "Cancelled" → "Annulé"; participles masculine (agreeing with implied "transfert").
- `queue.row.label` mirrors the `fileOperations.transferProgress.titleActive` gerund set, dropping "en cours" since
  these are short row labels: copy → "Copie", move → "Déplacement", delete → "Suppression", trash → "Placement dans la
  corbeille".
- FR CLDR `one`/`many`/`other` on `selectedCount` and `queuedToastCount`; `many` written identical to `other` (plain
  integers never select `many`, but the parity check requires the branch). `#` placeholders preserved.
- The standalone "Pause" button (`queue.row.pause`, `fileOperations.transferProgress.pause`) is legitimately identical
  to English (valid FR, macOS keeps it); the coverage check flags it but it's correct.

Re-validated against the reference pile during the `easy-navi` navigation + double-click-to-parent pass (2026-06-26).
The glossary-only first pass of these 14 keys held up: the pile CONFIRMS every term-based choice (and the orthodox
two-pane family carries the exact feature). A later same-day copy reword (David, coordinator-relayed) shortened the two
`doubleClickPaneNavigatesToParent` values; they reuse the terms below (see the reword note at the end). ICU values,
single apostrophes doubled below to match this doc's convention:

- double-click (noun) → double-clic; double-click (verb, imperative "Double-click …") → double-cliquez; (past participle
  "you double-clicked") → double-cliqué · Double Commander ("lorsqu''on double-clique dans un espace vide d''un
  panneau"), Total Commander ("Lors d''un double-clic sur la barre…"), KDE Dolphin ("double-clic", "Déclencheurs sur
  double-clics"), Nautilus ("\_Double-clic pour activer les éléments") · high — hyphenated "double-clic" /
  "double-cliquer" is unanimous across the orthodox + explorer families.
- pane background → arrière-plan du panneau · `arrière-plan` from KDE Dolphin ("Action à déclencher lors d''un
  double-clic sur l''arrière-plan de la vue") and the catalog-settled `background → arrière-plan`; `panneau` from the
  glossary's settled `pane → panneau` (Double Commander / Total Commander "panneau de fichiers") · high
- navigate to / go up to the parent folder → accéder au / remonter au dossier parent · macOS Finder Tier-1 for "accéder
  à" (the Go-menu item "Accéder au dossier parent", help text "Accède au dossier parent dans la fenêtre du Finder au
  premier plan"); "remonter au dossier parent" is the natural FR for the "go up" sense (Double Commander frames it
  "changement vers le répertoire-parent", but we keep macOS-Tier-1 "dossier parent", not DC's "répertoire-parent") ·
  high — the reworded `…label` uses "pour remonter au dossier parent" (the EN became "go up a folder"); the
  `fileExplorer.doubleClickHint.body` also uses "remonte au dossier parent".
- empty space (of a pane / file list) → espace vide · Double Commander ("un espace vide d''un panneau"), exact · high —
  the `…description` keeps the source's "file list" word as "liste de fichiers", mirroring the English mix of "pane"
  (label) vs "file list" (description).
- hint (the one-time double-click-to-parent notification / tip) → astuce · macOS Finder Tier-1 ("Astuces pour votre
  Mac"); Microsoft terminology FRA renders both "hint" and "tip" as "conseil", but macOS "astuce" wins (Cmdr is a macOS
  app) · high — feminine, so the agreeing participle is "affichée" in
  `settings.behavior.doubleClickOnPaneNotificationSeen.label` ("Astuce … affichée").
- row / file row (a row representing a file in the file list) → ligne / ligne de fichier · Microsoft terminology FRA
  (`row` → "ligne", feminine, FRA), matching the catalog's settled "striped rows → lignes alternées" · high — used in
  the reworded `…description` to contrast the pane background with a file row.

Conversational microcopy in the `doubleClickHint.*` notification (no direct pile source; idiomatic UI judgment, friendly
`vous` register):

- "What just happened?" → "Que s''est-il passé ?" · the punchy idiomatic surprise phrase; the English "just" is carried
  by context, not a literal "juste" · tentative (idiomatic, no pile hit)
- "Don''t like it?" → "Vous n''aimez pas ?" · friendly `vous`, the "it" dropped as natural FR · tentative
- "Never do this again" → "Ne plus jamais faire ça" · casual register matching the warm hint voice; refers to the
  navigation behavior, distinct from "ne plus afficher" (which would mean the hint) · tentative
- "I like it" → "J''aime bien" · natural casual FR for liking a feature (not the over-strong "J''aime"/"Je l''aime") ·
  tentative

Phrasing notes for this pass:

- Section/card consistency: `settings.section.navigationAndFileOps` → "Navigation et opérations" (concise rendering of
  the casual "Navigation & file ops"; French has no clean casual abbrev for "ops", so spelled out); the card
  `…card.fileOperations` keeps the settled "Opérations sur les fichiers"; `…card.navigation` is identical "Navigation"
  (carries `sameAsSourceJustification`). The summary lists the Oxford comma as ", et".
- Regular ASCII space before `?` throughout ("Que s''est-il passé ?", "Vous n''aimez pas ?"), per the catalog-wide
  settled spacing (style.md § Punctuation spacing); never U+202F.
- `fileExplorer.breadcrumb.navigateTooltip` → "Cliquez pour accéder à {path}" · macOS pattern ("cliquez", "accéder à");
  `{path}` placeholder preserved · high.
- Copy reword applied 2026-06-26 (David picked shorter wording; coordinator-relayed). The two
  `doubleClickPaneNavigatesToParent` values were updated to the new EN, reusing the terms above:
  - label, new EN "Double-click the pane background to go up a folder" → "Double-cliquez sur l''arrière-plan du panneau
    pour remonter au dossier parent" (imperative `double-cliquez` + settled `arrière-plan du panneau` + the "go up" verb
    `remonter au dossier parent`).
  - description, new EN "That''s the empty space around the file list, not a file row." → "C''est l''espace vide autour
    de la liste de fichiers, pas une ligne de fichier." ("That''s" → concise friendly "C''est", referring back to the
    pane background named in the label; settled `espace vide` + `liste de fichiers`; "around" → "autour de";
    `ligne de fichier` per the new row term).
- preset (value in a settings-picker dropdown) → présélection; "back to presets" → "Retour aux présélections" ·
  Microsoft terminology ("indexing preset" → "présélection d’indexation"), Double Commander fr ("Présélections"). macOS
  print uses "Préréglages" but that bundle is not in the pile · high

Settled during the `filesystem-size-guard` pass (FAT32-too-large write error + "and N more" overflow line, 2026-06-30).
RAW `errors.*` strings use single apostrophes; the `fileOperations.*` ICU string doubles them (none occur here):

- too large (a file exceeds a size/capacity limit) → trop volumineux · macOS Finder ("Cet élément est trop volumineux
  pour ce système.", "Impossible de copier « ^0 » car cet élément est trop volumineux pour le format du volume.", "Le
  contenu de « ^0 » est trop volumineux pour tenir sur le disque." — `LocalizableMerged.json` NE29/PE4.5/NE77), GNOME
  Nautilus ("Fichier trop volumineux pour la destination") · high — the `.title.one` "File too large for this drive" →
  "Fichier trop volumineux pour ce disque" tracks the Nautilus title almost verbatim (destination → "ce disque"); use
  "trop volumineux" (NOT "trop grand", which the pile reserves for image dimensions).
- formatted as <fs-format> → formaté en <fs-format> · standard FR construction ("formaté en FAT32", "formaté en exFAT");
  macOS frames it as "le format du volume" (PE4.5) and the in-catalog `errors.listing.notSupportedErrno.suggestion`
  already uses "formaté avec un système de fichiers", but when NAMING a concrete format "formaté en X" is the idiomatic
  fit · high
- can''t store files larger than X → ne peut pas stocker de fichiers de plus de X · reuses the exact in-catalog
  precedent at `errors.listing.notSupportedErrno.suggestion` ("FAT32 ne peut pas stocker de fichiers de plus de 4 Go",
  line 274) for consistency · high
- FAT32 / exFAT (filesystem-format names) → kept verbatim · do-not-translate (format names); the EN `@key` marks both as
  "keep as-is" · high
- "and {countText} more {file/files}" (overflow trailing line) → "et {countText} {…fichier/fichiers} de plus" · macOS
  Finder Tier-1 pattern "et ^0 de plus" (`LocalizableMerged.json` N141.3 "\n\tet ^0 de plus.") for the "and N more"
  shape; the file/files plural reuses the catalog''s settled `one {fichier} many {fichiers} other {fichiers}` fragment
  (FR CLDR `one`/`many`/`other`, `many` identical to `other` per the parity check) · high
- preset (value in a settings-picker dropdown) → présélection; "back to presets" → "Retour aux présélections" ·
  Microsoft terminology ("indexing preset" → "présélection d’indexation"), Double Commander fr ("Présélections"). macOS
  print uses "Préréglages" but that bundle is not in the pile · high

Settled during the `dialog-polish` copy/delete-dialog field-label pass (2026-06-30). ICU values, so single apostrophes
doubled below to match this doc's convention:

- Action (what a control chooses; screen-reader label `transferDialog.operationAria`) → "Action" · "Action" is a genuine
  French word (identical spelling), pile-pervasive as a UI noun (macOS Finder/AppKit "Action", MS terminology FRA
  "action"). With no colon on this key the FR value lands byte-identical to EN, so it carries a
  `sameAsSourceJustification` in the catalog · high
- "Scanning…" (spinner tooltip + SR label while the dialog counts selected items) → "Analyse…" · reuses the settled
  `scanning (transfer stage) → analyse` term (`transferProgress.stageScanning` = "Analyse"); the single … char kept
  verbatim (EN uses one … glyph, not three dots) · high
- "This folder doesn''t exist yet. Cmdr will create it during the copy/move." (yellow inline warning under the
  destination box when the typed target folder is missing) → "Ce dossier n''existe pas encore. Cmdr le créera lors de la
  copie." / "… lors du déplacement." · "doesn''t exist (yet)" → "n''existe pas (encore)" (pile: Double Commander "Le
  répertoire « %s » n''existe pas. Voulez-vous le créer ?"); "Cmdr will create it" rendered ACTIVE per the style guide
  as "Cmdr le créera" ("le" = the masculine "dossier"; not the passive "sera créé" the pile shows in Thunar); "during
  the copy/move" → "lors de la copie" / "lors du déplacement" (pile-attested "lors de la copie"; reuses the settled
  `copy → copie` / `move → déplacement` nouns). Two literal sentences, operation-specific verb, no ICU select · high
- **queue.row.label progress arms (rename / create folder / create file)** · `Renommage` / `Création du dossier` /
  `Création du fichier` · verbal-noun style of the sibling arms (Copie, Déplacement); Nautilus ("Renommage de …",
  "Création des …"), settled `dossier`/`fichier` · high

Settled during the `archive-browsing` pass (2026-07-05, browse-into-zip/tar/7z + app bundles). ICU values double
apostrophes; the RAW `errors.*` keys use single apostrophes:

- archive (a zip/tar/7z browsed like a folder) → archive (feminine: "une archive", "l''archive") · macOS Finder
  ("Archive ZIP", "Compresse des éléments dans une archive.", "Choisissez un mot de passe pour l’archive.", "Déplacer
  l’archive vers…") · high — same word as EN but genuinely FR (feminine), so NOT flagged identical where it inflects;
  the bare card title `settings.archives.card.archives` / section `settings.section.archives` ("Archives") IS
  identical-to-English and carries `sameAsSourceJustification`. zip/tar/7z format tokens kept verbatim.
- app bundle (.app/.bundle/.framework, a macOS package folder shown as one item) → paquet ("Paquets d''application") ·
  macOS Finder ("Afficher le contenu du paquet" = Show Package Contents; "Archive de paquet iOS") · high — Finder calls
  a bundle a "paquet"; "App bundles" card/row titles → "Paquets d''application" (keys 16 & 19 use the SAME word, per the
  brief's consistency note).
- extract (pull files out of an archive) → extraire ("Cmdr parcourt et extrait…") · GNOME Nautilus ("fichier extrait"),
  Total Commander ("Extraire les fichiers"), MS terminology FRA ("extraire") · high — the browse verb is the settled
  `browse → parcourir`; "browses and extracts" → "parcourt et extrait".
- editable / can be edited (a zip whose entries can be added/removed/renamed) → modifiable ("seules les archives zip
  sont modifiables") · standard FR; rendered with the adjective to stay active and dodge the passive "peuvent être
  modifiées" · high
- encrypted → chiffré(e) · macOS ("Chiffrement", "Chiffrer") · high — agrees with the subject: feminine "archive"
  ("chiffrée") in the listing explanation, masculine "fichier" ("chiffré") in the viewer error.
- damaged → endommagé(e) · macOS ("Impossible d’ouvrir cette application car elle est peut-être endommagée…") · high —
  chosen over "corrompu" (macOS uses both; "endommagé" is the softer, more common Finder wording). Agrees with subject
  gender.
- open with default app → ouvrir avec l''application par défaut · macOS ("Ouvrir avec", "Aucune application par défaut…
  pour ouvrir") · high — used the full "application" (Tier-1 macOS) rather than the catalog's casual "app" for these
  default-app / another-app senses, since macOS attests "application par défaut" / "une autre application" directly.
- Enter (the Return/Enter key, in "what pressing Enter does") → la touche Entrée · existing fr catalog precedent
  ("Appuyez sur Entrée", "les recherches par IA attendent toujours la touche Entrée") · high — Enter renders "Entrée"
  catalog-wide; kept, not the English "Enter".
- Ask (segmented-control cell, "ask each time") → Demander · existing fr catalog ("Toujours demander", "Tout demander"),
  macOS pattern · high. Browse cell → Parcourir; Open cell → Ouvrir (settled `browse`/`open`).
- "Editing archive" (queue.row.label `archive_edit` arm) → "Modification de l''archive" · verbal-noun style of the
  sibling arms (Copie, Déplacement, Renommage); edit → modification; settled `archive` · high
- fresh copy (ask the sender for one) → une nouvelle copie · macOS ("Une nouvelle copie de « %@ » a été créée.") · high
  — the sender phrased gender-neutrally as "la personne qui vous l''a envoyée" (name the person, past participle agrees
  with the feminine "archive" via the preceding "l''").

Settled during the `paste-clipboard-as-file` pass (2026-07-07, ⌘V pastes clipboard text/image/PDF as a new file + its
setting). None of these 7 values contain an apostrophe, so no ICU doubling arose. The terms:

- paste clipboard content as a file → coller le contenu du presse-papiers en tant que fichier · settled `paste → coller`
  - `clipboard → presse-papiers`; "as X" → "en tant que X" from macOS Finder ("Copier en tant que lien" = Copy as Link,
    `LocalizableMerged.json` N48.1_V1) · high — infinitive label form matching the sibling
    `allowFileExtensionChanges.label` ("Autoriser…").
- as / saved as (naming a created file) → en tant que / enregistré sous · macOS Finder "Copier en tant que lien" for the
  bare "as"; macOS AppKit save panel "Enregistrer sous…" / "Enregistrer sous :" for the Save-As "as {name}" sense · high
- do nothing (radio option: ⌘V does nothing on non-file clipboard) → Ne rien faire · standard FR; no direct pile hit
  (Double Commander lists "Do nothing" untranslated, the file-manager pofiles carry no "ne rien faire") · tentative
  (standard FR, unambiguous, no source)
- create file / create and rename (paste-as-file radio options) → Créer un fichier / Créer et renommer · reuses
  `fileExplorer.functionKeyBar.newFileAction` ("Créer un fichier") and settled `create → créer` + `rename → renommer` ·
  high

Phrasing note for this pass:

- The paste-confirmation toast `fileExplorer.clipboard.pastedAsFile` is an ICU `select` on `{kind}` (image/pdf/other)
  with an uncontrolled `{filename}` → "Contenu du presse-papiers collé dans {filename} ({kind, select, image {image} pdf
  {PDF} other {texte}})". GENDER-SAFE by anchoring the past participle "collé" to the masculine head noun "Contenu"
  (invariant across every `kind` branch), keeping the varying `kind` noun in a bare parenthetical (no agreement needed),
  and leaving `{filename}` in a neutral slot after "dans" (safe for any generated name). `text → texte`; `image`/`PDF`
  unchanged. Branch NAMES `image`/`pdf`/`other` kept verbatim. This dodges the trap where a participle placed after the
  varying `kind` would have to agree (image → collée vs texte → collé).

Settled during the `archive-password` pass (2026-07-08, encrypted-zip unlock modal `fileOperations.archivePassword.*`).
ICU values, so every apostrophe is doubled in the catalog.

- password-protected → `protégé par un mot de passe` (fem. `protégée` when agreeing with `archive`) · TC/DC fr phrasing
  · high.
- password (noun) → `mot de passe` · macOS/MS fr · high. Input aria-label "Mot de passe de l''archive".
- unlock (button + verb) → `Déverrouiller` · macOS AppKit ("Déverrouiller") · high. Verb form "la déverrouiller".
- archive (fem.) → `archive` · settled fr glossary · high.
- GENDER PATTERN: the body names the archive explicitly, `L''archive <archive>{name}</archive> est protégée…`, so the
  feminine antecedent `archive` (not the uncontrolled `{name}`) drives every agreement — `protégée`, and the pronoun
  `la` in "la déverrouiller". Never let agreement hang off `{name}`, whose gender is unknown at runtime.

Settled while translating the Compress feature:

- compress (verb / control label) → `Compresser` · Finder `fr/macOS` ("Compresser", `Compress ${sources}` → "Compresser
  ${sources}"), NOT "Comprimer" · high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and
  both title-verb branches.
- compression (progress form) → noun `Compression` in the select branch, assembled with the sibling "… en cours..." tail
  · derived on `Copie`/`Déplacement` · high. `scanTitleCompress` = "Vérification avant la compression...".
- compressed (result toast) → past participle `compressés` · mirrors `transfer.split.clean` ("{phrase} copiés") and the
  `one`/`many`/`other` shape of `fileOnly.allDone` · high.
- replace (overwrite warning) → `remplacera` · Finder `Replace` → "Remplacer" · high.
- archive (name) → `l''archive` (ICU-doubled apostrophe) · Finder `Zip archive` → "Archive ZIP" · high. `.zip` in
  straight double quotes.
- compression level (slider label) → `Niveau de compression` · fr Finder/DC `compression` + `niveau`; standard 7-Zip
  term `Niveau de compression` · high. TC `fr` LNG lacks the pack-dialog IDs.
  `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Plus rapide` · fr comparative, MS/archiver usage · high. Marks quicker packing,
  not app speed. `.faster`.
- smaller (slider high end, level 9) → `Plus petit` · pairs with `Plus rapide`; marks the smaller output file · high.
  `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

Settled while translating the Operation log feature (alpha history-of-operations dialog + its command). ICU values, so
apostrophes are doubled in the catalog:

- operation log → `Historique des opérations` · macOS "historique"
  (`NSToolbarHistoryTemplate`/`NSTouchBarHistoryTemplate` → "historique", "historique des versions"), Double Commander
  ("Historique des commandes", "Historique des dossiers") · high — the feature IS a history view and its English `@key`
  descriptions call it "operation history" throughout, so the user-facing "historique" (Apple's word for history views)
  fits better than the technical "journal" (reserved for `journalisation`/`fichier journal`, the log-file sense). Used
  verbatim for `operationLog.dialog.title` AND `commands.logOperationLog.label` (same sourceHash 2c97965).
- operation (a logged file operation) → `opération` (feminine) · reuses the settled
  `File operations → Opérations sur les fichiers` section name · high.
- roll back / rollback (reverse a COMPLETED operation, operation-log sense) → SUPERSEDED, see § « La famille `rollback`
  » at the end of this file. The Canceled-vs-Rolled-back reasoning below still holds; only the word family changed, from
  `restaur-` to `retour en arrière`. **DIVERGENCE from the live-transfer catalog, deliberate:** the transfer surface
  renders the rolling-back action as `annulation` (`fileOperations.transferProgress.titleRollingBack` = "Annulation en
  cours..."), but the operation log must keep `Canceled` and `Rolled back` as DISTINCT status pills. Anchoring rollback
  to the `restaur-` family reserves `annuler`/`Annulé` exclusively for `Canceled`, preserving the
  you-canceled-before-it-ran vs you-reversed-it-after semantic split. The five rollback pills read as one concept:
  - Can roll back → `Restauration possible`; Can''t roll back → `Restauration impossible` (macOS calm "… impossible"
    pattern)
  - Rolling back → `Restauration en cours`
  - Rolled back → `Restauré` (masc., agrees with implied `élément` for the per-item outcome, kept masc. for the
    operation status pill to match the sibling masculine pills)
  - Partly rolled back → `Partiellement restauré`
  - `commands.logOperationLog.description` "…and roll them back" → "…et restaurez l''état précédent" (stays in the
    `restaur-` family; "them" rendered as "l''état précédent" = restore the prior state, which is what rollback does).
- Status pills matched to the existing `queue.row.status` renderings (brief-mandated consistency): Queued →
  `En attente`, Running → `En cours`, Done → `Terminé`, Canceled → `Annulé` · high. `Didn''t finish` (status + item
  outcome, non-alarmist wording for a stopped op) → `Non terminé`, NOT "Échec" (brief-mandated; avoids "erreur"/"échec"
  per the style guide).
- Per-item outcomes: Done → `Terminé`; Skipped → `Ignoré` (settled `skip → ignorer`); Didn''t finish → `Non terminé`;
  Rolled back → `Restauré` · high. `status.done`/`outcome.done` (same sourceHash) and `status.failed`/`outcome.failed`
  and `status.rolledBack`/`outcome.rolledBack` render identically, as their shared hashes require.
- summary lines (one-line op summaries) → count-led past participle agreeing masc. with `élément`/`fichier`/`dossier`:
  Copied → "{countText} élément(s) copié(s)", Moved → "…déplacé(s)", Deleted → "…supprimé(s)", Moved to trash →
  "…placé(s) dans la corbeille" (settled `move to trash → placer dans la corbeille`), Renamed → "…renommé(s)",
  Compressed → "…compressé(s)", Created N folders/files → "{countText} dossier(s)/fichier(s) créé(s)" · high — mirrors
  `transfer.fileOnly.allDone`''s participle-agreement discipline; FR CLDR `one`/`many`/`other` with `{countText}` in
  every branch (`many` identical to `other`).
- Edited an archive → `Archive modifiée`; Extracted an archive → `Archive extraite` · past participle agreeing with the
  feminine `archive` (settled glossary term); `extract → extraire`, `edit archive → modification` · high.
- provenance labels: You (op you started) → `Vous` (macOS uses "Vous"); AI client → `Client IA` (`AI → IA`); Agent →
  `Agent` (identical, genuine FR word, carries `sameAsSourceJustification`) · high.
- "and {countText} more item(s)" (overflow line) → "et {countText} élément(s) de plus" · reuses the macOS "et ^0 de
  plus" overflow pattern settled in the `filesystem-size-guard` pass · high.
- No other `sameAsSourceJustification` needed: every value except `Agent` differs from English.

Settled during the `ask-cmdr` pass (2026-07-13, the read-only AI chat rail: `askCmdr.*`, `settings.askCmdr.*`,
`settings.advanced.logLlmCalls.*`, `settings.section.askCmdr`, `commands.askCmdrToggle.*`, ~97 keys). ICU values, so
every apostrophe is doubled in the catalog:

- chat / a saved conversation with the assistant (noun) → `conversation` · MS terminology FRA (`chat` → "conversation
  instantanée"/"clavardage"/"messagerie instantanée", all live-chat-feature senses that don''t fit; the plain
  `conversation` entry, feminine, is the generic term); macOS has no Messages-app bundle in the pile, so MS is the
  anchor here. Confirmed by the EN source ITSELF: `askCmdr.consent.local` says "what each **conversation** costs" for
  the very same saved-chat entity that `askCmdr.sessions.*` calls a "chat" — so English already treats the two words as
  synonyms, and FR settles on the one word, `conversation`, everywhere. "New chat" → "Nouvelle conversation"; the
  "Chats" panel heading/tooltip → "Conversations"; "chat title" → "Titre de la conversation" · high.
- chat (verb, casually "to chat with the AI") → `discuter` · distinct from the noun above; matches the EN source's own
  verb choice ("Ask Cmdr **chats** with", "start **chatting**") and macOS/MS''s general "discuter"/"conversation"
  family; keeps `conversation` free for the noun sense (a saved thread) so the two senses don''t collide · high.
- token (AI usage-cost unit) → `jeton` · MS terminology FRA (plain `token` → "jeton", masc.; distinct from "jeton de
  sécurité"/"jeton d''authentification" which are the auth-token senses, wrong here) · high. FR CLDR
  `one`/`many`/`other` written for `askCmdr.cost.tokens` (`many` identical to `other`, matching the catalog-wide
  plain-integer convention).
- archive (verb, put a chat away without deleting it) → `archiver`; unarchive → `désarchiver` · MS terminology FRA
  (`archive` verb → "archiver", high); no pile hit for the un- form, but `désarchiver` is the standard, unambiguous FR
  antonym (same des- + verb pattern as `désélectionner`/`désactiver` already in this catalog) · high for archiver,
  tentative for désarchiver. NOTE: this is a DIFFERENT sense from the existing `archive (noun, a zip/tar/7z) → archive`
  glossary entry (archive-browsing pass) — same English word, two unrelated senses (put-away-a-conversation vs.
  compressed-file), exactly as in English; no collision because the chat sense is a VERB here and the zip sense stays a
  noun.
- archived (badge on a put-away conversation) → `Archivée` · agrees feminine with the implicit `conversation` (the
  object, not a person), per the gender-restructuring rule · high.
- on-device (a cost readout for the free local model) → `en local` · descriptive FR, no pile hit for this exact
  compound; pairs with the already-brand-kept `Local LLM` provider option without reusing the brand name itself (the
  cost readout is a plain-language footnote, not a provider label) · tentative.
- "Ask about X" (a short invitation/placeholder to query files or a selection) → verb-first
  `Poser une/des question(s) sur X`, EXCEPT the compact composer-attach button, which uses `Interroger X` (`interroger`
  = query/ask a system) to stay short as a button label · tentative (idiomatic rendering, no exact pile phrase for
  either).
- thinking (assistant status while reasoning before it replies) → `Réflexion…` · descriptive FR noun-status, matching
  the catalog''s existing noun+ellipsis progress-label convention (`Analyse…`, `Vérification…`); single `…` character
  kept per the EN source (not three dots) · tentative.
- tool-call status lines (present/past pairs shown while the assistant runs a read-only tool, e.g. "Checking your
  drives" / "Checked your drives") → present tense as a deverbal-noun phrase (`Vérification de vos disques`), past tense
  as `A [participe]é …` (`A vérifié vos disques`) · descriptive FR pattern, no direct pile precedent for this exact
  present/past UI shape; chosen to read naturally as two tenses of the same action without needing a subject pronoun ·
  tentative. Applied to all seven `askCmdr.tool.*` pairs (`appState`, `listDir`, `largestDirs`, `importantFolders`,
  `folderImportance`, `listVolumes`, `operationsList`, `operationsGet`) plus the `unknown` fallback (`Travail en cours`
  / `A utilisé un outil`).
- "That request wasn''t available" (a read-only tool refusing an unsupported action) →
  `Cette demande n''était pas disponible` · plain, calm FR; avoids "erreur"/"échec" per the style guide · high.
- "This one hit its limit" (a single answer that used up its tool-step/time budget) → `Celle-ci a atteint sa limite` ·
  `celle-ci` (fem.) refers back to the implicit `réponse` (the answer), agreeing with it rather than exposing an
  ungendered pronoun · tentative.
- "Not now" (consent-screen decline button) → `Plus tard` · reuses the catalog''s already-settled
  `later (dismiss-for-now button) → plus tard` term (the `search`/`feedback`/… pass); same dismiss-without-committing
  action · high.
- log AI model calls (the LLM-call-logging Advanced setting, `settings.advanced.logLlmCalls.*`) →
  `Journaliser les appels au modèle d''IA` (toggle label, infinitive verb form matching the catalog''s
  `Activer le réseau`-style toggle labels); the consent-screen note (`askCmdr.consent.logsNote`) refers back to the same
  phrase as a noun (`la journalisation des appels au modèle d''IA`) for consistency between the two surfaces · high
  (reuses the settled `logging → journalisation` term).
- drop to attach (a drag-and-drop hint on the composer) → `Déposer pour joindre` · `déposer` from macOS''s "Boîte de
  dépôt" (Drop box, the only pile hit for "drop"); `joindre` from the catalog''s existing attach-an-email-address
  precedent (`crashReporter.dialog.attachEmail` → "Joindre mon adresse e-mail…") · tentative (composed from two
  separately-sourced roots, no single pile phrase for the whole hint).
- attachment (a file/folder staged onto a chat message) → `pièce jointe` (noun); remove attachment →
  `Retirer la pièce jointe` · MS terminology FRA (`attachment` → "pièce jointe", fem.); `retirer` matches macOS''s
  sidebar-removal register · high.
- Provider/model settings-path breadcrumb `Settings › AI` → `Réglages › IA` · reuses the settled `AI → IA` section name
  and the catalog''s in-app `Réglages >` breadcrumb convention (the `updates`/`whatsNew` pass) · high.
- `Ask Cmdr` (the product/brand name) is `sameAsSourceJustification`''d everywhere it appears alone (`askCmdr.title`,
  `settings.section.askCmdr`, `commands.askCmdrToggle.label`) per its own `@key` description ("keep it as-is"); every
  other value in this pass differs from English.

Settled during the `media-ml-index` network-drive image-indexing pass (2026-07-13, opting an SMB drive into background
photo-content indexing + its status lines; `settings.mediaIndex.networkVolumes.*`, the internal
`settings.mediaIndex.{networkVolumes,alwaysIndexVolumes,alwaysIndexFolders}.{label,description}`,
`search.imageResults.{networkOff,paused}`). ICU values, so apostrophes are doubled in the catalog:

- network drive → `disque réseau` · settled `drive → disque` (macOS Finder, Tier 1) + `réseau` (macOS "Réseau",
  pervasive). DELIBERATELY NOT Microsoft''s Windows term "lecteur réseau" (MS terminology FRA id 84433) — Cmdr is a
  macOS app, so `disque` wins over the Windows `lecteur` per the style-guide term-choice rule 2 · high.
- photo → `photo` (feminine: "une photo", "les photos") · macOS/pile ("photo"/"photos") · high — same word as EN but
  genuinely FR and gendered, so agreeing participles are feminine: "photo indexée" / "photos indexées"
  (`networkVolumes.indexed` FR CLDR `one`/`many`/`other`, `many` identical to `other`; feminine agreement in every
  branch), "photos … indexées" (`search.imageResults.paused`).
- reconnect → `se reconnecter` · macOS pile ("reconnecter"); pairs with the settled `disconnect → se déconnecter`.
  "resumes when this drive reconnects" → "reprend quand ce disque se reconnecte" (settled `resume → reprendre`,
  `paused → en pause`) · high.
- gently (reads photos over the network gently) → `en douceur` · natural calm FR, no exact pile phrase · tentative.
- at a limited speed → `à vitesse limitée` · descriptive FR, no pile hit · tentative.
- always index this drive (the rarely-browsed-archive override) → `Toujours indexer ce disque` /
  `Toujours indexer les photos sur {name}` (aria) · composed from settled `index → indexer` + the catalog''s `Toujours`
  (crash-reporter "Always → Toujours"); the internal list labels are `Disques à toujours indexer` /
  `Dossiers à toujours indexer` · high.
- "get indexed anyway" (always-index help) → `soient indexées malgré tout` · settled `browse → parcourir` in the same
  string ("que vous parcourez rarement"); "photo archive" → "archive de photos" (settled feminine `archive`) · high.
- Internal (hidden dev-setting) label/description strings translated like the `settings.indexing.silencedDrives.*`
  sibling: `Interne : …` lead, third-person `l''utilisateur` · high.
- No `sameAsSourceJustification` needed: every value differs from English (each carries the FR ASCII-space-before-`:`, a
  translated term, or French agreement).

Settled during the `quality-pass` review of the 54 keys added by the bulk-rename, image-index-scope, and Ask Cmdr tool
features (`askCmdr.renameReview.*`, `askCmdr.tool.{searchPhotos,imageFacts,proposeRenamePlan}.*`, `askCmdr.stalled`,
`errors.listing.deviceReconnecting.*`, `fileExplorer.imageIndex.*`,
`fileExplorer.navigation.driveIndex.tooltipCoalesced*`, `settings.mediaIndex.*`). ICU values double their apostrophes;
the three `errors.*` keys use single ones:

- allow (per-row approval button) → `Autoriser`; allow all → `Tout autoriser` · macOS Finder ("Allow Anyway" →
  "Autoriser quand même", "Allow me to be discovered by:" → "Autoriser la détection…") + the catalog-wide `Tout <verbe>`
  all-variant pattern · high.
- deny (per-row refusal button) → `Refuser`; deny all → `Tout refuser` · MS terminology FRA (`deny` verb → "refuser";
  the button ProperNoun entry → "Refuser"); macOS has no Deny button string in the pile · high.
- review (verb, the "check this list of proposed changes" action) → `vérifier`; the surface as a noun → `vérification` ·
  macOS AppKit ("Review Changes…" → "Vérifier les modifications…") · high. So "Review file renames" → "Vérifier les
  renommages" and "This review expired" → "Cette vérification a expiré". NOT "revoir" (macOS uses that for re-reading
  documents) and NOT the MS noun "revue" (publishing sense).
- rename cycle (A→B, B→A dependency loop) → `cycle de renommage`; the badge `(cycle)` is legitimately identical to
  English · MS terminology FRA (`cycle` → "cycle", masc.) · high.
- rotate (files through a name cycle) → `permuter` · MS terminology FRA (`swap` → "permuter"); deliberately NOT macOS's
  "rotation"/"faire pivoter", which the pile reserves for the SPATIAL image-rotation sense ("rotation à gauche") and
  would read as turning the photos · high.
- filename extension → `extension` · macOS Finder ("Show all filename extensions" → "Afficher toutes les extensions de
  fichiers", "Hide Extension" → "Masquer l'extension") · high — the `(extension)` badge is legitimately
  identical-to-English and carries a `sameAsSourceJustification`.
- overwrite (badge naming the clash) → `écrasement` (noun) · derived from the settled `overwrite → écraser` (macOS
  "Écraser à la destination", "Écraser les extensions") · high. ASCII space before the `!` per the settled spacing rule:
  `(écrasement !)`.
- remove (take a folder off the indexing list) → `Retirer`, NOT `Supprimer` · DELIBERATE divergence from macOS Tier 1,
  which renders "Remove" as "Supprimer" everywhere ("Remove from Sidebar" → "Supprimer de la barre latérale"). In this
  catalog `supprimer` is the settled `delete` term, and the help text's whole job is to promise that removing a folder
  is NOT a deletion, so `Supprimer` would say the opposite of the copy. `Retirer` matches the catalog's existing
  `Retirer la pièce jointe` · high.
- searchable (what stays findable after a folder leaves the list) → `reste disponible dans la recherche` · no pile term
  for the adjective; rendered as a verb phrase anchored on the settled `search → recherche` so the promise stays about
  SEARCH, not mere viewing ("consultable" loses that) · tentative.
- indexing pass (one sweep of the indexer over a drive) → `passage` ("au prochain passage") · descriptive FR; pairs with
  the settled `indexation` · tentative.

Phrasing notes for this pass:

- **Tool-line doing/done pairs keep the settled shape**: present = deverbal noun phrase, past = `A <participe> …`
  (glossary, `ask-cmdr` pass). `proposeRenamePlan.done` had drifted to the participle-final "Plan de renommage préparé"
  and is now `A préparé un plan de renommage`, parallel with its `Préparation d'un plan de renommage` twin and with all
  nine sibling pairs.
- `searchPhotos` keeps `Recherche dans vos photos` / `A cherché dans vos photos`: the `chercher` past participle looks
  like a stem mismatch with `Recherche`, but it is EXACTLY what the sibling `operationsList` pair already ships, and
  cross-pair consistency on the same rail outranks stem symmetry. Don't "fix" one without the other.
- **Apostrophe form**: the whole `fr` catalog uses ASCII apostrophes (doubled `''` in ICU values, single `'` in
  `errors.*`). Three of these keys had shipped the curly U+2019 (copied from the English source, which uses it) and were
  normalized. A curly apostrophe is not an ICU escape, so it passes every check silently: it's a consistency break the
  tooling can't catch. (Two pre-existing `fileExplorer.smbReauth.*` values still carry U+2019, outside this pass's
  scope.)
- `askCmdr.stalled` ends "…ou arrêter", mirroring the Stop button's own label `askCmdr.composer.stop` = "Arrêter". The
  earlier "ou l'arrêter" left the pronoun `l'` with no antecedent (and an unknowable gender).
- `askCmdr.renameReview.expired` says "Demandez à Cmdr…", not "Demandez à Ask Cmdr…": the English sentence uses the
  brand as a verb phrase ("Ask Cmdr to prepare it again"), which in French collapses into the verb `demander`; keeping
  the brand whole would read as "demandez à Ask". The brand still appears (`Cmdr`), so the don't-translate check holds.
- `errors.listing.deviceReconnecting.suggestion` was the catalog's only `tu` address ("Patiente… réessaie…") and is now
  `vous` ("Patientez quelques secondes, puis réessayez."), per the settled formality.
- ASCII space before `%` in `fileExplorer.imageIndex.indexingTooltip` ("{percent} % du travail est fait") and before `;`
  in the `renameReview.status` screen-reader summary, per the catalog-wide settled spacing rule.
- The two `driveIndex.tooltipCoalesced*` tooltips were confirmed unchanged: FR CLDR `one`/`many`/`other` on all three
  counts, no "erreur"/"échec" wording, and the calm close ("remettra tout d'aplomb" / "rien de grave donc") matches the
  reassuring register the `@key` description asks for.

Settled for the per-file/folder/drive image-search index status badges in the file list (2026-07-22:
`fileExplorer.imageIndex.{file,folder,drive}.*`, `settings.mediaIndex.showFileStatusIcons.*`, 13 keys). ICU values, so
every apostrophe is doubled in the catalog:

- image search (the OCR/photo-content search FEATURE) → `recherche d''images` · settled catalog-wide, NOT re-derived:
  `settings.mediaIndex.card` and `settings.section.imageSearch` both already render "Recherche d''images", and
  `search.imageResults.*` uses "images". Reused verbatim for every "image search" mention (`file.indexed`,
  `file.excluded`, `drive.ariaLabel`, `drive.off`) · high.
- image (the file/noun, feminine: "une image", "les images") → `image` · macOS/pile pervasive; same word as EN but
  genuinely FR and gendered, so agreeing participles are feminine: "image indexée" / "images indexées". The badge sits
  on an image file, so every per-file status agrees feminine (indexée, incluse, modifiée, réindexée) · high.
- indexed (an image is in the image-search index) → `indexée` (fem., agrees with the implicit `image`); indexing (the
  noun) → `indexation`; re-indexed → `réindexée` · reuses the style-guide glossary
  `index / indexing → index / indexation` term; the `driveIndex.*` (folder-size disk index) surface already uses
  "indexation"/"indexé" · high.
- badge / status badge (the small marker over a file icon; the small colored dot next to a drive) → `pastille` /
  `pastille d''état` · reuses the settled (tentative) `chip / badge (status pill) → pastille` glossary term; "pastille"
  (small disc/lozenge) fits both the file-icon badge and the literal drive "dot" · tentative (no exact reference-pile
  hit; consistent with the prior badge choice).
- "Couldn''t be indexed" (gentle, no "error"/"failed") → `Indexation impossible` · reuses the settled
  `Couldn''t/Can''t X → "… impossible"` calm macOS pattern (the `errors` pass), staying away from "erreur"/"échec" per
  the style guide · high.
- off (image search turned off for a drive) → `désactivée` · mirrors the sibling `driveIndex.tooltipDisabled`
  ("L''indexation est désactivée pour ce disque.") · high.
- "X of Y" (progress count) → `{doneText} sur {totalText}` · macOS "sur" for counts (settled `free of → libre sur`);
  used in `folder.someIndexed` and `drive.indexing` · high.

Phrasing notes for this pass:

- `folder.allIndexed` / `folder.someIndexed` are headline fragments (no trailing period, matching the EN per-file
  tooltips): "{totalText} images indexées" and "{doneText} sur {totalText} images indexées". The English "All" is
  carried by the ABSENCE of the "sur {doneText}" fraction (allIndexed shows only the total; someIndexed adds "done of"),
  so no literal "toutes" is forced (which would break the FR `one` branch, "Toutes les 1 image"). FR CLDR
  `one`/`many`/`other` with the past participle folded into each branch ("image indexée" / "images indexées"),
  `{totalText}` kept in a single slot outside the plural so it appears in every rendering.
- The two drive tooltips lead with "Sur ce disque," (locative) to keep the drive context and avoid burying "on this
  drive" in a double-"sur" clash with the "X sur Y" count. `drive.indexing` closes "; indexation en cours." (calm "en
  cours" progress convention, NOT "toujours en train de travailler"); `drive.done` uses present-tense "sont indexées"
  (are indexed), which states completeness without a fragile "toutes les {n}".
- Regular ASCII space before `;` in `file.stale` ("… ; sera réindexée") and `drive.indexing` ("… ; indexation en
  cours."), per the catalog-wide settled spacing rule (style.md § Punctuation spacing); never U+202F.
- `settings.mediaIndex.showFileStatusIcons.label/description` use infinitive-label "Afficher des pastilles d''état sur
  les images" and third-person help "Ajoute une petite pastille sur chaque image de la liste des fichiers…" (settled
  `file list → liste des fichiers`), matching the catalog's toggle-label + help-text register.
- No `sameAsSourceJustification` needed: every value differs from English.

Settled for the image-indexing settings restructure (2026-07-22: three card titles, the Semantic search card, one
file-list badge; `settings.mediaIndex.{cards.enable,cards.folders, progressSummary.title,semanticSearch.label,clip.*}`,
`fileExplorer.imageIndex.file.indexing`, 12 keys). ICU values, so every apostrophe is doubled in the catalog:

- "Indexing now" (active status: an image being processed RIGHT NOW, contrasted with `pending` = queued) →
  `Indexation en cours` · reuses the settled `indexing → indexation` term + the catalog''s calm "en cours" progress
  convention (`drive.indexing` closes "; indexation en cours."). Deliberately distinct from `file.pending` ("En attente
  d''indexation", the queued sense). Used for BOTH `fileExplorer.imageIndex.file.indexing` (the badge tooltip) and
  `settings.mediaIndex.progressSummary.title` (the live-progress heading) · high.
- "Enable indexing" (card title over the master toggle) → `Activer l''indexation` · settled `index → indexation` +
  macOS-pattern `Activer` (catalog `Activer le réseau`, `driveIndex.menuEnable` "Activer l''indexation…") · high.
- "Folders to index" (card title) → `Dossiers à indexer` · settled `folder → dossier` + `index → indexer`; mirrors the
  existing `alwaysIndexFolders.label` "Dossiers à toujours indexer" shape · high.
- search by description (the CLIP semantic-search feature, "find a photo by describing it") →
  `la recherche par description` · anchored on the existing catalog phrasing `clip.ready` ("recherchez vos photos par
  description") and `clip.description` ("en décrivant ce qu''elles contiennent"); distinct from the card title
  `clip.title` = "Recherche sémantique" (kept for the model name in `deleteConfirmTitle` "modèle de recherche
  sémantique") · high. The toggle label "Search photos by description" (`semanticSearch.label`) →
  `Rechercher des photos par description` (infinitive-label form).
- Apple silicon → `Apple Silicon` · kept verbatim per the settled glossary term (licensing/ai pass, line ~165); no
  reference-pile hit for a French rendering. "a Mac with Apple silicon" phrased naturally as
  `un Mac équipé d''une puce Apple Silicon` (`clip.notSupported`) · high (brand verbatim), the "équipé d''une puce"
  framing tentative (idiomatic, no pile phrase).
- "Delete model (reclaim {size})" → `Supprimer le modèle (libérer {size})` · settled `delete → supprimer`,
  `model → modèle`; "reclaim" reuses `reclaim.button`''s `libérer` verb (dropping its "environ" since the source has no
  "roughly" here). "Deleting…" → `Suppression…` (noun+ellipsis progress convention, sibling of `clip.downloading`
  "Téléchargement…"; single `…` char per the source) · high.
- keyword → `mot-clé`; tag (Finder tag, in "keyword and tag search") → `tag` · settled catalog-wide (`showTags.label`
  "Afficher les tags") · high. `deleteConfirmBody`: "Keyword and tag search keep working" → "La recherche par mot-clé et
  par tag continue de fonctionner".
- "The model couldn''t be removed just now. Try again in a moment." (non-alarmist delete-failure) →
  `Le modèle n''a pas pu être supprimé pour le moment. Réessayez dans un instant.` · reuses the calm
  `N''a pas pu se terminer`/`réessayez` register (queue + errors passes), avoiding "erreur"/"échec" per the style guide
  · high.
- No `sameAsSourceJustification` needed: every value differs from English.

Settled during the dialog-polish pass (`fileOperations.json`, 2026-07-23): the delete dialog swapped its Trash/Delete
picker for a "Move to trash" switch plus a matching confirm button, and the copy/move/compress dialog groups the source
path and the destination volume+path under "From" and "To" headings.

- "Move to trash" (`delete.trashSwitch`; switch in the delete dialog, on = trash, off = permanent delete) →
  `Placer dans la corbeille` · macOS Finder AL13/N153 verbatim, and identical to this file's
  `transferDialog.titleVerbOnly` `other {Placer dans la corbeille}` arm · high
- "Delete" (`delete.confirmDelete`; destructive confirm button while the switch is off) → `Supprimer` · settled delete
  verb, identical to `transferDialog.titleVerbOnly`'s `delete {Supprimer}` arm · high
- "From" / "To" (`transferDialog.sourceGroupTitle` / `targetGroupTitle`; headings over the source path and over the
  destination volume + path) → `De` / `À` · Total Commander fr (`662="De : "`, `663="À : "`) and Double Commander fr
  ("De :"/"A :") both ship this label pair in the same copy/move dialog, and "De … à …" is the idiomatic French from/to
  pair. macOS's `Déplacer vers :` ("Move To:") is verb-bound, so it settles the destination PREPOSITION inside a verb
  phrase, not the standalone heading; bare "Vers" was weighed on that basis and set aside for the pile-attested,
  symmetrical pair. No space before a colon applies here: the headings carry no colon · high

Settled during the review of the five master-drive-indexing-off keys
(`fileExplorer.navigation.driveIndex.{refusedIndexingOff,tooltipIndexingOff,menuIndexingOffNote}`,
`settings.indexing.{masterOffNote,overriddenBadge}`). ICU values, so apostrophes are doubled in the catalog:

- **"Drive indexing" (the master switch), in PROSE → `l''indexation` (bare), NOT `l''indexation du disque`.** The
  catalog already renders the concept bare everywhere it speaks about it (`driveIndex.tooltipDisabled` "L''indexation
  est désactivée pour ce disque.", `menuEnable`/`menuDisable`/`menuStop` "… l''indexation …"), and the scope marker
  carries the global-vs-per-drive distinction the English marks with "Drive indexing" vs "Indexing": `… pour ce disque`
  (this one drive) vs `… dans les Réglages` + `aucun disque` (the master switch) · high. Writing
  `l''indexation du disque` in running prose reads as "the indexing of THE drive", which is exactly the per-drive
  meaning these three strings exist to rule out; worse, `menuIndexingOffNote` then puts "du disque" and "ce disque" one
  clause apart. `settings.indexing.enabled.label` stays "Indexation du disque" and is quoted VERBATIM in the navigation
  path ("Activez-la dans Indexation > Indexation du disque"), so the pointer to the actual control survives.
- "stays unindexed" (with the uncontrolled `{name}`) → `Cmdr n''indexe pas {name}` · rendered ACTIVE with Cmdr as the
  subject so no participle agrees with `{name}`, whose gender is unknown at runtime (the archive-password pass's gender
  rule). Mirrors the sibling `refusedGeneric` "Cmdr ne peut pas indexer {name} pour le moment." · high.
- "picks up where it left off" → `reprendra là où il s''était arrêté` · settled `resume → reprendre`; French wants the
  `là où` correlative, not a bare `où` · high.
- "folder sizes stay hidden" → `les tailles des dossiers restent masquées` · reuses `driveIndex.tooltipDisabled`''s
  "voir les tailles des dossiers"; the plural `des dossiers` is the catalog form (a bare "tailles de dossier" was drift)
  · high.
- "Off with drive indexing" (short override badge) → `Désactivé avec l''indexation` · a STATE label, masculine agreeing
  with the implied `réglage`; kept to three words for the badge slot, dropping "du disque" since the badge already sits
  inside the Indexation section · high.
- "ready for when you turn this back on" → `prêt pour le moment où vous la réactiverez` · the pronoun `la` points back
  to the feminine `l''indexation` that opens the note (the only feminine singular antecedent); the deictic "ceci" was
  vaguer than French tolerates here · high.

Phrasing notes for this pass:

- All five values had shipped with LONE apostrophes (`L'indexation`, `n'est`, `s'était`) while every `driveIndex.*`
  neighbour doubles them. Now doubled. None of them sat before `{`/`}`/`#`, so nothing rendered wrong and no check
  fired: it is exactly the silent consistency break the ICU rule exists to prevent.
- Same sweep fixed five PRE-EXISTING apostrophe defects elsewhere in `fr`:
  `settings.operationLog.{intro,maxAge.label, maxSize.description}` carried lone apostrophes, and
  `fileExplorer.smbReauth.{savedPasswordFailed,passwordFailed}` carried the curly U+2019 the earlier pass parked. The
  whole `fr` set is now uniformly ASCII-and-doubled outside `errors.*` (the only remaining lone `'` are the deliberate
  ICU escapes in `fileExplorer.dirSize.{noPerms, dirPlaceholder}`, `'<'`).
- No `:` `?` `!` `%` `»` in any of the five values, so the settled ASCII-space-before-punctuation rule doesn''t apply
  here. Register is `vous` throughout ("Activez-la", "vous la réactiverez"); no "erreur"/"échec".
- No `sameAsSourceJustification` needed: every value differs from English.

## Index de disque : l'analyse des changements (2026-07-28)

- **"Checking for changes" (run-kind header) → `Recherche des changements`** · nominal phrase matching the sibling
  headers (`Première analyse complète`, `Mise à jour rapide`); `Recherche de…` is the standard French UI shape for a
  "checking for X" label, and `changements` is catalog-settled (`Rattraper les changements récents`) · high.
- **"Update the file list" → `Mettre à jour la liste des fichiers`** · composed from the settled siblings
  `Enregistrer la liste des fichiers` + `Mettre à jour l''index` · high.
- **"the check running right now" → `l''analyse en cours`** · reuses `analyse` as this catalog's settled word for a full
  check (`tooltipCoalesced`: "la prochaine analyse complète de Cmdr") and that string's closing
  `remettre tout d''aplomb` · high.

## Transferts à l'arrêt : les 8 clés du bandeau de blocage (2026-07-31)

Settled during the stalled-transfer pass (`fileOperations.transferProgress.stall*` + `close`, `queue.row.stalled`). ICU
values, so single apostrophes doubled below to match this doc's convention:

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
  `askCmdr.renameUndo.refusedBatches` ("The operation log has the details." → "Le journal des opérations donne les
  détails.") and the settled `log file → fichier journal` (`settings.logging.openLogFile`: "Ouvrir le fichier journal",
  MS terminology FRA) · high — "fichier journal" (not bare "journal") because this string points at Cmdr''s log FILE,
  while "journal des opérations" is the separate in-app operation history.

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
- No `sameAsSourceJustification` needed: all eight values differ from English.
- `{duration}` arrives pre-formatted ("45s", "2m 30s") from `$lib/units`, so it is NOT localized by this catalog; the
  sentence is built so any length or shape reads correctly after "depuis".

## Chemin copié : la confirmation du presse-papiers (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Une clé : la ligne de la notification d'information après ⌘⌥C. Le chemin s'affiche en dessous, sur sa propre ligne en
police à chasse fixe : ce n'est donc PAS un paramètre dans la phrase, qui se termine par deux-points et doit tenir sans
lui.

- **"Copied the path, it's now on your clipboard:" → `Chemin copié, il est maintenant dans le presse-papiers :`** ·
  reprend `path → chemin` et `clipboard → presse-papiers` du glossaire (macOS Finder) · high. Espace ASCII normale avant
  les deux-points, conformément à style.md § Punctuation spacing ; jamais U+202F. Pas de possessif ("votre
  presse-papiers") : macOS emploie l'article défini.
- Pas de `sameAsSourceJustification` : la valeur diffère de l'anglais.

## The operation-queue rename (2026-08-08, 14 keys in `queue` / `commands` / `fileOperations`)

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
  the medical sense); already settled in this glossary as the Operation log''s head noun and in the
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
- No `sameAsSourceJustification` needed: all 14 values differ from English.

## The corner progress chip and the failure notice (2026-08-08, 9 keys in `queue`)

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
  the settled destination preposition. `item → élément` per the glossary's contested block; macOS uses "éléments" in
  this very string, so files-and-folders is covered.
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
- No `sameAsSourceJustification` needed: all nine values differ from English.
- **Overflow watch** (French runs long, and both surfaces are tight):
  - `queue.failureToast.action` is 46 characters against English's 23, on a button in a ~360 px toast. It can't be
    shortened without breaking the `@key`'s "identical to the window title" requirement.
  - `queue.failureToast.title`'s `trash` arm, "Le placement dans la corbeille n''a pas pu se terminer" (52 chars vs 31),
    is the longest headline. Nautilus's shorter "Mise à la corbeille" was weighed and set aside: the brief binds the
    arms to the `queue.row.label` nouns so the toast and the row agree, and that row says "Placement dans la corbeille".
  - The chip itself only ever shows `queue.row.label` / `queue.row.status`, which this pass didn't touch, so the ~80 px
    chip carries no new risk.

## The standalone conflict prompt (2026-08-09, `fileOperations.operationConflict.{context,pausedNote}`)

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
- No `sameAsSourceJustification` needed: both values differ from English.

## The progress dialog's empty-queue button label (2026-08-09, 2 keys in `fileOperations.transferProgress`)

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
- Neither value is identical to English, so no `sameAsSourceJustification` is needed.

## The quit gate (2026-08-10, 7 keys in `main.quit`)

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
  away" → `supprime`, per the glossary''s `delete → supprimer` (and NOT `efface`, which style.md reserves for the
  erase/wipe sense).
- **logout (the OS session, in the countdown''s reason clause) → `une fermeture de session`** · macOS Tier-1
  (`AppKit/Menus.json` "Log Out" → "Fermer la session", the item the user sees in the Apple menu); Microsoft terminology
  FRA agrees (`log off` → "fermer une session", FRA) · high. restart → `un redémarrage` · `AppKit/Menus.json` "Restart"
  → "Redémarrer" · high.
  - **Deliberate divergence from `shortcuts.system.loggingOut` ("la déconnexion")**, recorded so the next pass sees it
    instead of "fixing" one side blind. That shortcuts entry was settled as "descriptive FR" with no Tier-1 citation,
    and it sits in a list of system shortcuts where nothing competes with it. Here the neighbouring words are a file
    manager''s own: this catalog uses `se déconnecter` for leaving a SERVER (macOS Finder "Disconnect" → "Se
    déconnecter"), so "un redémarrage ou une déconnexion" inside Cmdr could read as dropping an SMB share.
    `fermeture de session` is unambiguous and is what the user''s Apple menu says. **Open item for a future pass**:
    re-settle `shortcuts.system.loggingOut` to "la fermeture de session" so the pair agrees; it is outside this pass''s
    seven keys.
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
- No `sameAsSourceJustification` needed: all seven values differ from English.

## Usage stats: "anonymes" dropped, "un identifiant aléatoire" named (2026-08-12, 5 keys in `settings.analytics.enabled`, `settings.updates`, `onboarding.stepBeta`)

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
- No `sameAsSourceJustification` needed: every value differs from English.

## Lignes de file en attente de réponse et la confirmation d'annulation-restauration (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`, 2026-08-13)

- **"Needs your answer" (pastille de statut dans la file) → `Réponse requise`** · macOS `fr` ("Authentification requise
  pour effectuer cette opération.", "Un mot de passe est requis pour désactiver le chiffrement.") · high. ❌ Jamais
  `Réponse attendue` ni rien en `attente` : `En attente` EST le statut "en file derrière une autre opération"
  (`queue.row.status`), et les deux doivent rester distinguables dans la même colonne étroite.
- **"prompt" (la question affichée sur laquelle l'opération est arrêtée) → `la question`** · aligné sur
  `operationConflict.pausedNote` ("tant que vous n'avez pas répondu") · high.
- **"this operation carries on" → `cette opération continuera`** · standard ; évite `reprendra`, réservé au sens
  `resume → reprendre` (sortir de pause) · high.
- **rollback → `Annuler et restaurer`** · réaffirme l'entrée déjà posée (`transferProgress.conflictRollback`) ; le titre
  est `Annuler et restaurer cette opération ?` et le bouton destructeur reprend le même libellé, pour coller au bouton
  qui a ouvert le dialogue · high · **tentative, à revoir** : le nouveau corps dit explicitement que les fichiers
  écrasés ne reviennent pas, donc le `restaurer` du terme promet plus que ce que l'action fait. À arbitrer sur toute la
  famille `rollback` du catalogue, pas clé par clé.
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
- Aucun `sameAsSourceJustification` nécessaire : toutes les valeurs diffèrent de l'anglais.

## La famille `rollback` : `restaurer` abandonné pour `revenir en arrière` (2026-08-13, 14 clés dans `fileOperations`, `operationLog`, `commands`, `settings`)

Arbitrage sur TOUTE la famille, comme le demandait la note « à revoir » de la section précédente. Le corps de
`rollbackConfirm` dit noir sur blanc que les fichiers écrasés ne reviennent pas : le rollback SUPPRIME ce que
l'opération a écrit, il ne REND rien. `restaurer` promettait donc l'inverse de ce que l'action fait.

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

## Renommage en chaîne : « et N autres » se rend par `ainsi que …` (2026-08-18, `fileExplorer.rename.chainKeptOriginalNameAndOthers`)

Le toast grandissant qui compte les renommages non appliqués. Il prolonge la phrase du frère `chainKeptOriginalName` («
{reason}. « {name} » a gardé son nom. »), donc même voix, mêmes guillemets, même « a gardé son nom ».

- **"and so did N other files" → `ainsi que {n} autres fichiers`** · macOS Finder Tier 1 pour la forme « X et N autres
  éléments » (`LocalizableMerged.json` : « Envoi de « ^1 » et de ^0 autres éléments. », « … en gardant les éléments les
  plus récents tels que « ^1 » et ^0 autres éléments. ») ; `ainsi que` déjà employé dans le catalogue
  (`onboarding.stepBeta.feedback.discord`) et attesté dans KDE Dolphin `fr` · high. Le « so did » anglais n'a pas
  d'équivalent direct : `ainsi que` porte le parallélisme sans allonger la phrase (le catalogue `fr` dérive déjà long).
  `fichier` et non `élément` parce que l'anglais dit explicitement « file ».
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

## Renommage non confirmé : le volume ne répond pas (2026-08-18, `fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

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
- Aucun `sameAsSourceJustification` nécessaire : les trois valeurs diffèrent de l'anglais.

## Opérations suggérées : la fenêtre de ce que propose Ask Cmdr (2026-08-19, `suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (l'ensemble d'opérations proposé par l'agent) → `opérations` ; le titre devient `Opérations suggérées` · terme
  maison ("File operations" → "Opérations sur les fichiers") · high
- approve → `Approuver` · MS ; retenu plutôt que le `Accepter` de macOS, car la variante avec décompte ("Approuver 3
  fichiers") autorise une action au lieu d'accepter un objet · high
- reject → `Refuser` · macOS Finder, paire Accepter/Refuser du panneau AirDrop (Tier 1) · high
- "This can't be undone" → `Cette opération est irréversible` · macOS Finder, mot pour mot (alerte de suppression
  immédiate) · high
- "Ask Cmdr's reason" → `Raison donnée par Ask Cmdr` · composé ; `motif` est déjà pris par pattern, d'où `raison` · high
- "Matched by a pattern" → `Correspond à un motif` · reprend `motif` du glossaire · high

## Dupliquer : la commande qui copie dans le même dossier (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (commande qui copie la sélection dans son propre dossier) → `Dupliquer`** · macOS Finder `fr`, menu «
  Fichier > Dupliquer » (`N154`), plus « Dupliquer des éléments » et « Duplique des éléments dans leurs emplacements
  actuels » (vérifié sur macOS 26.6.1, `Finder.app/Contents/Resources/fr.lproj`, 2026-08-19) · high. Ne chevauche ni
  `Copier` (F5) ni `Déplacer` (F6).
- **« Make a copy of the selected files in the same folder » →
  `Créer une copie des fichiers sélectionnés dans le même dossier`** · infinitif, comme les descriptions voisines («
  Copier les fichiers sélectionnés… ») ; « le même dossier » = celui où les fichiers se trouvent déjà · high.

## Menus natifs : barre de menus, menus contextuels, titres de fenêtre (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

Sources de tout ce lot : macOS 26.5.2 Finder (`Finder.app/Contents/Resources/fr.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) est le Tier 1 et tranche presque tout ; le côté anglais se lit dans `en_GB.lproj`, car
`Base.lproj` ne contient que des nibs compilés. Safari 26 (`MainMenu.strings`) donne le vocabulaire des onglets, la
terminologie Microsoft ce qu'Apple ne nomme pas. Famille RAW : **apostrophes simples**, un `''` s'afficherait en double
dans le menu.

- **Titres de la barre → `Fichier`, `Édition`, `Présentation`, `Aller`, `Fenêtre`, `Aide`, `Services`** · macOS Finder
  et Safari `fr` · high.
- **Menu Select (sélection de fichiers) → `Sélectionner`** · Nautilus/Thunar/Dolphin `fr` · high. Le Finder n'a pas
  d'équivalent ; l'infinitif s'accorde avec `Tout sélectionner` du même menu.
- **Quick Look → `Coup d'œil`** (avec l'apostrophe typographique U+2019, comme Apple) · macOS Finder (`TL14`) · high.
  Apple localise ce nom de fonction, d'où son absence de la liste ne-pas-traduire.
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
- **busy (volume occupé) → `(occupé)`** · terminologie Microsoft · high.
- **Eject → `Éjecter`, Disconnect → `Se déconnecter`, Remove (d'une liste) → `Retirer`** · macOS Finder · high.
  `Retirer` évite que le retrait d'un favori se lise comme une suppression de fichiers.
- **`{name}` entre guillemets → `« {name} »`**, avec l'espace ASCII normale des deux côtés, conformément au réglage
  typographique du catalogue `fr`.
- **Identiques à l'anglais à dessein** (avec `sameAsSourceJustification`) : `menu.app.services`, `menu.sort.extension`,
  `menu.view.zoom`, `menu.tag.orange`, `menu.view.askCmdr`.

## Notification de repli sur le montage macOS (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

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

## Refus de renommage et de création : les 31 clés `errors.mutation.*` / `errors.volume.*` (2026-08-23)

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
  est déjà au glossaire.
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
  au glossaire, et `Déplacer` est le libellé de la commande Cmdr). Le sujet est le nom `Le renommage`, comme
  `queue.row.label` (`Renommage`).
- **`Something went wrong, and Cmdr couldn't tell what` →
  `Un problème est survenu, et Cmdr n'a pas pu identifier lequel.`** · `problème` est le repli calme déjà retenu pour
  `error` (voir § Terms) ; ni « erreur » ni « échec » · high.
- **`The volume couldn't finish that` → `Le volume n'a pas pu terminer cette opération.`** · reprend le repli
  `Couldn't finish → N'a pas pu se terminer` de la passe `queue` · high.
- **`The connection didn't answer in time` → `La connexion n'a pas répondu à temps.`** · macOS AppKit atteste
  `n'a pas répondu` (« L'application « %@ » n'a pas répondu à la demande de service. ») · high. On garde le verbe plutôt
  que le nom `délai dépassé`, qui reste le terme du STATUT (voir § Terms).
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

Aucun `sameAsSourceJustification` nécessaire : les 31 valeurs diffèrent de l'anglais.

## Refus de mise à la corbeille : `errors.mutation.trashNotSupported` / `trashRefused` (2026-08-23)

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

## Le rapport quand Cmdr n'a PAS quitté (2026-08-23, `crashReporter.dialog.body.keptRunning` / `.unknown`)

Le dialogue de rapport au démarrage suivant choisit désormais sa phrase d'ouverture parmi trois, selon ce que le rapport
a réellement enregistré. `.ended` (Cmdr est tombé avec l'incident qu'il rapporte) ne bouge pas. Les deux nouvelles clés
décrivent des cas où Cmdr n'a PAS quitté : `.keptRunning` (le problème a touché une tâche en arrière-plan, l'app a
poursuivi, et c'est l'utilisateur qui l'a quittée ensuite) et `.unknown` (rapport écrit par une version plus ancienne,
qui n'enregistrait pas la suite). ❌ Aucun mot de fermeture n'a le droit d'y figurer, et `.unknown` doit rester vrai que
Cmdr ait quitté ou non, donc il n'affirme ni l'un ni l'autre. Valeurs ICU ; aucune des deux ne contient d'apostrophe,
donc aucun doublement n'a été nécessaire (les valeurs citées ci-dessous le sont selon la convention de ce document).

- **"ran into a problem", Cmdr en sujet → `Cmdr a rencontré un problème`** · la structure app-en-sujet + `a rencontré`
  est attestée Tier 1 (`fr/macOS/Finder/LocalizableMerged.json`, `NE105` : "« ^0 » encountered an error." → « « ^0 » a
  rencontré une erreur. »), et le guide de style Microsoft FRA atteste la collocation verbe + nom (§ 4.1.9 : « Nous
  avons rencontré un problème… », son rendu sanctionné de "We''ve hit a snag"). `problème` remplace `erreur`, que
  style.md interdit, et reprend le repli calme déjà settled de ce glossaire (`error (non-alarmist status) → problème`) ·
  high (vérifié sur la pile de référence, 2026-08-23).
  - **Pourquoi pas la tournure impersonnelle d'Apple** (« Un problème s'est produit lors de… »,
    `fr/macOS/AppKit/AppKitErrors.json` ; Nautilus `fr` « Il y a eu un problème lors de l'exécution de ce logiciel. ») :
    les trois variantes remplissent la même phrase du même dialogue, et `.ended` est déjà livré avec Cmdr en sujet. Dans
    `.keptRunning`, le sujet DOIT rester Cmdr, puisque c'est Cmdr qui est resté ouvert.
- **"kept running", l'APP et non une opération → `est resté ouvert`** · le catalogue `fr` a déjà settled ce prédicat
  exact : `main.instanceLock.alertTitle` rend "Cmdr is already running" par « Cmdr est déjà ouvert », et c'est aussi la
  formule de macOS pour une app déjà lancée · high. Le participe s'accorde au masculin avec Cmdr, comme le
  `s''est fermé` de `.ended`.
  - ❌ **Pas `a continué de fonctionner`** : la famille `fonctionner` n'est attestée nulle part dans la pile `fr` pour
    cet emploi (macOS, les quatre `.po`, Total Commander et la terminologie MS : zéro occurrence), et l'adopter
    coûterait au catalogue son mot unique pour « Cmdr tourne ». La pile ne propose que des formes plus techniques ou
    plus rares : « en cours d'exécution » (Nautilus, Thunar, Dolphin, Double Commander ; registre process) et « en
    service » (macOS `N144`, « quand le Finder est en service »).
  - **Bénéfice voulu : `est resté ouvert` est l'antonyme direct du `s''est fermé` de `.ended`.** Les deux clés occupent
    le même emplacement et ne s'affichent jamais ensemble, donc le contraste rend la distinction immédiate pour qui voit
    l'une ou l'autre. Ne pas « harmoniser » une clé sur l'autre.
  - **Ne pas confondre avec `en cours`**, le mot du catalogue pour une OPÉRATION qui tourne
    (`main.quit.operationsHeading` « Toujours en cours », `queue.row.status`). Ici le sujet est l'app, pas une
    opération.
- **"in the background" → `en arrière-plan`** · unanime dans la pile (terminologie MS FRA `background task` → « tâche en
  arrière-plan » ; Double Commander "When application is in the &background" → « Quand l'application est en
  &arrière-plan » ; Dolphin « une indexation de vos fichiers en arrière-plan ») et déjà settled dans ce glossaire pour
  le bouton du dialogue de progression · high. « en tâche de fond » n'a aucune occurrence dans la pile `fr`.
- **"a report" et non "a crash report" → `un rapport` tout court** · la 2e phrase reprend mot pour mot celle de `.ended`
  en retirant le seul mot qui porte « crash » : « Voici un rapport d''incident avec des détails qui peuvent aider à
  corriger ça. » → « Voici un rapport avec des détails qui peuvent aider à corriger ça. » C'est exactement ce que fait
  l'anglais, et `incident` est justement le mot que ce glossaire réserve au plantage
  (`crash report → rapport d''incident`). La terminologie MS FRA confirme le nom nu (`Diagnostic Report` → « Rapport de
  diagnostic » ; `report` au sens « document généré par une application » → « rapport » dans cinq entrées sur sept, la
  septième, « état », appartenant au registre ERP) · high.
- **`.keptRunning` met `La dernière fois,` en tête**, là où `.ended` et `.unknown` le laissent en fin de proposition.
  Raison : `.keptRunning` porte déjà `en arrière-plan`, et empiler « en arrière-plan la dernière fois » alourdit la
  phrase sans rien gagner. Les trois valeurs ne se croisent jamais à l'écran, donc l'ordre peut différer ; la voix,
  elle, reste identique. `.unknown` calque `.ended` mot pour mot, prédicat mis à part.
- Valeurs :
  - `.keptRunning` :
    `La dernière fois, Cmdr a rencontré un problème en arrière-plan et est resté ouvert. Voici un rapport avec des détails qui peuvent aider à corriger ça.`
  - `.unknown` :
    `Cmdr a rencontré un problème la dernière fois. Voici un rapport avec des détails qui peuvent aider à corriger ça.`
- Aucune des deux valeurs n'est identique à l'anglais : pas de `sameAsSourceJustification`. Aucun `: ; ! ? %`, donc la
  règle d'espace avant ponctuation ne se pose pas ; aucune apostrophe, donc aucun doublement ICU ; aucun U+2019 ni
  U+202F.

## Le dialogue d'incident : `a quitté inopinément`, `a continué son exécution`, `la dernière fois` en fin de proposition

Trois décisions prises sur preuves de la pile de référence, dont une correction d'une valeur déjà livrée.

- **CORRECTION — « quit unexpectedly » → `a quitté inopinément`** · `fr/macOS/AppKit/AppKitErrors.json` : « Lors de sa
  précédente ouverture, %@ a quitté inopinément pendant la réouverture des fenêtres. » · high. La valeur livrée
  auparavant, `s''est fermé de façon inattendue`, n'a **aucune attestation** dans toute la pile ; c'est une paraphrase.
  Apple a créé le concept et l'utilisateur francophone lit cette formule dans les dialogues de plantage du système, donc
  c'est elle qui fait autorité. Seule la locution verbale change ; `la dernière fois` reste où elle était.
- **« kept running » → `a continué son exécution`** · `fr/macOS/AppKit/NSExceptionAlert.json` `69.title` : « … pour
  continuer l'exécution de l'application dans un état instable … » · high. C'est le dialogue d'exception d'Apple
  lui-même, c'est-à-dire exactement notre surface. Le catalogue confirme le champ lexical avec `en cours` :
  `Toujours en cours en arrière-plan.` et `Garder ce transfert en cours en arrière-plan`
  (`fileOperations.transferProgress.*`). ❌ Pas `en cours d''exécution` : zéro occurrence dans le catalogue, ne pas
  l'introduire. ❌ Pas `a continué de fonctionner` : `fonctionner` veut dire « marcher / être compatible » partout dans
  la pile (macOS `LA20`/`LA35`, Thunar, Double Commander), jamais « rester en marche ». ❌ Pas `est resté ouvert` :
  décrit une fenêtre, pas un processus.
- **`la dernière fois` se place EN FIN de proposition, jamais en tête** · les trois seules occurrences de la pile la
  placent en fin (`AppKit/Document.json` « … ouvert ou enregistré pour la dernière fois. », Thunar `:2659`, Dolphin
  `:4744`), et les cinq occurrences du catalogue livré font pareil · high. Quand Apple veut l'antéposer, elle change de
  construction (`Lors de sa précédente ouverture, …`), elle ne déplace pas `la dernière fois`. Les trois variantes du
  corps partagent donc la même charpente, `Cmdr <verbe> … la dernière fois.`
- **`continuer à` + infinitif, pas `continuer de`** · macOS `PE79`/`PE80`/`PE81` (« continuer à copier les autres »),
  Thunar `:3174`. `continuer de` n'apparaît qu'une fois dans toute la pile (Thunar `:3161`), sur la même chaîne anglaise
  qu'une variante en `à`. Noté ici parce que la question revient à chaque phrase de continuité, même si la valeur
  retenue ci-dessus emploie `continuer` + complément de nom.
- **« a report » sans « crash » → `un rapport`** · la seconde phrase reprend celle de `.ended` sans `d''incident`, et
  `rapport d''incident` reste réservé au vrai plantage. Titre et confirmation suivent la même coupe :
  `Envoyer le rapport d''incident ?` / `Envoyer le rapport ?`, `Rapport d''incident envoyé. …` / `Rapport envoyé. …` ·
  high.
- **`crashReporter.dialog.privacyNote` était déjà neutre** (« la partie du code concernée »), donc la valeur ne bouge
  pas alors que l'anglais est passé de « crashed » à « ran into the problem ». Seule l'empreinte a été rafraîchie.

## Le texte du réglage des rapports couvre désormais les deux cas (`settings.updates.crashReports.description`)

Le réglage envoie aussi un rapport quand un problème en arrière-plan n'a PAS fait quitter l'app, donc l'aide ne peut
plus parler seulement d'une fermeture. Tout est repris de la section du dialogue d'incident ci-dessus, au présent :

- **`quand Cmdr quitte inopinément`** applique enfin la CORRECTION attestée (`fr/macOS/AppKit/AppKitErrors.json`) à
  cette clé, à la place de `se ferme de façon inattendue`, la paraphrase non attestée qu'elle traînait encore · high.
- **`rencontre un problème en arrière-plan`** vient de `crashReporter.dialog.body.keptRunning` · high.
- **`un rapport` tout court**, pas `un rapport d''incident` : la phrase couvre les deux cas, même opération que
  `.title.report` · high. ❌ Le LIBELLÉ `settings.updates.crashReports.label` garde `Envoyer les rapports d''incident` :
  c'est le nom du réglage.
- **Deuxième phrase reprise de `crashReporter.dialog.privacyNote`** (`la partie du code concernée`), à la place de
  `l''emplacement de l''incident`, vrai seulement en cas de plantage · high. La virgule de série que l'ancienne valeur
  avait héritée de l'anglais disparaît au passage : le français ne l'utilise pas (règle déjà notée plus haut).

## Éjection et déconnexion refusées : les neuf clés `errors.eject.*` (2026-08-23)

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

Aucun `sameAsSourceJustification` nécessaire : les neuf valeurs diffèrent de l'anglais. Aucun `: ; ! ? %` dans les
valeurs, donc la règle d'espace avant ponctuation ne se pose pas ; aucune apostrophe doublée (famille RAW), aucun U+2019
ni U+202F.

## La notification de corbeille : annuler et remettre en place (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

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

## Compléter un rapport déjà envoyé : les 11 clés `errorReporter.amend*` (2026-08-28)

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
- **`Couldn't add your note: {error}` → `Ajout de votre note impossible : {error}`** · moule figé de la famille
  (`prepareFailed` « Préparation de l'aperçu impossible : », `sendFailedToast` « Envoi du rapport d'incident impossible
  : », `saveFailedToast` « Enregistrement du lot impossible : ») : nom verbal + `impossible` + espace ASCII + `:` · high
- **`Note added to your report. Your reference ID is` →
  `Note ajoutée à votre rapport. Votre identifiant de référence est`** · seconde moitié identique au frère
  `errorReporter.sentToast.message` (la valeur s'arrête juste avant le badge, sans ponctuation finale) ; `ajoutée`
  s'accorde avec `note`, féminin · high
- **`View or add notes to the report` → `Voir le rapport ou y ajouter des notes`** · les deux moitiés (regarder ET
  ajouter) sont tenues, et le pronom `y` évite de répéter « au rapport », ce qui garde le bouton court (38 caractères
  contre 31 en anglais) dans une notification où il voisine « Modifier les réglages » · high. `Voir` plutôt
  qu'`Afficher` : le catalogue réserve `Afficher` à l'ouverture d'un contenu (`fileExplorer.functionKeyBar.viewAction` «
  Afficher le fichier ») et utilise `Voir` pour consulter une information (`menu.app.licenseDetails` « Voir les détails
  de la licence », `whatsNew.dialog.seeFullChangelog`), ce qui est le sens ici. `Afficher` coûterait aussi trois
  caractères de plus.

## La fenêtre sélectionner / désélectionner des fichiers (`selection.*`, 2026-08-29)

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

## Un mot anglais, un mot français : la revue de dérive (2026-08-30)

Le catalogue portait 38 endroits où `fr` donnait deux noms différents au même texte anglais, le plus souvent parce
qu'une passe tardive avait touché `menu.json` en laissant `commands.json` sur l'ancienne formulation. Treize étaient de
vraies dérives et ont disparu ; les vingt-cinq restantes sont des frontières VOLONTAIRES (ou des angles morts du
vérificateur) et sont décrites plus bas pour que la prochaine passe ne les « uniformise » pas.

### Corrigé

- **`View` (l'action F3, ouvrir le fichier dans la visionneuse intégrée) → `Visualiser`**, partout :
  `commands.fileView.label`, `menu.file.view`, `fileExplorer.functionKeyBar.viewLabel` (qui disait `Afficher`) ·
  `high`. ❌ Pas `Afficher` : c'est le verbe SHOW du catalogue (`commands.viewShowHidden.label` =
  `Afficher ou masquer les fichiers cachés`), et confondre les deux est exactement ce que cette entrée empêche.
  ❌ Pas `Présentation` : c'est le MENU (voir la frontière plus bas). Double Commander `fr` dit `Voir` pour la même
  action (`tfrmmain.actview.caption`) — correct, mais Tier 3, plus vague dans une palette de commandes, et deux clés sur
  trois disaient déjà `Visualiser`. À surveiller au contrôle de débordement : 10 caractères sur la barre F-touches.
- **`menu.file.quickLook` = `Coup d'œil`, avec UNE apostrophe ASCII** · la famille `menu.*` est NATIVE : Rust la lit
  par simple recherche dans une table, sans moteur ICU, donc un `''` s'affiche littéralement comme deux apostrophes
  dans la barre de menus de macOS (`isRawKey` dans `i18n-catalog-lib.ts`). La clé jumelle
  `commands.fileQuickLook.mac.label` est ICU et s'écrit donc `Coup d''œil` · `high`. Elle portait aussi l'apostrophe
  courbe U+2019, contre la règle « toujours U+0027 » de `style.md`.
- **`error report` → `rapport d'incident` partout** · `settings.updates.sendErrorReport` disait
  `rapport d'erreur`, seul contre cinq · `high`.
- **`Check for updates` → `Rechercher les mises à jour`** · `settings.updates.checkForUpdates` disait `Vérifier` alors
  que la commande et le menu disaient `Rechercher`. macOS `fr` tranche : Software Update rend
  `Checking for updates…` par `Recherche des mises à jour en cours…` et `Check for Updates` par
  `Rechercher les mises à jour` (vérifié sur macOS 26.6.2, build 25G83, 2026-08-30) · `high`.
- **`Operation log` → `Historique des opérations`** · `settings.navigationAndFileOps.card.operationLog` disait
  `Journal des opérations`, contre la commande, le menu et le titre de la fenêtre · `high`.
- **`Reset to default` → `Réinitialiser au réglage par défaut`** et **`Reset all to defaults` → `Tout réinitialiser aux
  réglages par défaut`** · `settings.control.resetToDefault` et `settings.advanced.resetAll` disaient
  `Réinitialiser par défaut`, qui ne veut pas dire la même chose (« par défaut » y devient un adverbe) · `high`.
- **`API key` → `Clé d'API`** aussi dans l'assistant (`onboarding.cloudSetup.apiKeyAria`, `.apiKeyPlaceholder.generic`
  disaient `Clé API`), conformément à l'entrée de glossaire déjà en place · `high`.
- **`Got it` → `D'accord`** dans les deux · macOS `fr` traduit « Got It » par `D'accord` (vérifié sur macOS 26.6.2,
  2026-08-30) ; le catalogue disait `Compris` d'un côté et `J'ai compris` de l'autre · `high`.
- **`Press Enter to search` → `Appuyez sur Entrée pour rechercher`** dans les deux (`search.runHint` disait
  `pour lancer la recherche`) ; **`Word wrap` → `Retour à la ligne automatique`** dans les deux
  (`settings.viewer.wordWrap.label` inversait l'ordre des mots) ; **`Go to home folder` → `Aller au dossier
  personnel`** aussi sur le bouton de l'écran d'erreur ; **`Check your inbox…`** dit la même chose dans l'assistant et
  dans les réglages · `high`.
- **`{dir}` / `{dirs}` → `rép.` partout**, y compris `fileExplorer.summary.dirNoun` qui disait
  `dossier`/`dossiers` · `high`. C'est la décision déjà prise pour les statistiques d'analyse, et le voisin immédiat
  dans la barre d'état (`fileExplorer.selectionInfo.dir`) affiche `RÉP.`.
- **`From:` devant un chemin → `De :`** · `fileOperations.scanPhase.fromLabel` disait `Depuis :` alors que le couple du
  dialogue de transfert est `De` / `À`. `De :` est aussi la forme des en-têtes de courrier · `high`.

### Frontières volontaires (ne pas uniformiser)

- **`Connect to server` : `Connexion au serveur` titre le dialogue, `Se connecter au serveur` est la commande** ·
  macOS `fr` ship les DEUX, exactement ainsi (le point du menu Aller est `Se connecter au serveur…`, le titre de la
  fenêtre est `Connexion au serveur`). `fileExplorer.network.browser.connectToServerRow` est une ligne qui déclenche
  l'action, donc verbe ; `connectDialog.title` est le titre, donc nom · `high`.
- **`Back` : `Précédent` dans le menu Aller, `Retour` sur les boutons de l'app** · `@menu.go.back` demande le mot exact
  du Finder, et macOS `fr` n'utilise que `Précédent` (6 occurrences dans `fr/macOS/`, aucun `Retour` isolé). Les
  boutons réseau et l'assistant sont des surfaces Cmdr et gardent `Retour` · `high`.
- **`View` : `Présentation` est le MENU, `Visualiser` est l'action** · macOS Finder `fr` nomme son menu View
  `Présentation`, et `@menu.bar.view` demande ce mot · `high`.
- **`Edit` : `Édition` est le MENU, `Modifier` est le verbe** · même raison, `@menu.bar.edit` · `high`.
- **`Zoom` : `Zoom` est la taille du texte, `Réduire/agrandir` est la fenêtre** · macOS `fr` nomme l'action de la
  pastille verte `Réduire/agrandir`, et le `@key` dit d'employer le mot de macOS ICI même s'il diffère · `high`.
- **`(unknown)` s'accorde avec ce qu'il remplace** · `fileExplorer.network.browser.unknown` = `(inconnu)` (le nombre de
  partages, masculin) ; `fileOperations.transferProgress.sizeUnknown` = `(inconnue)` (la taille, féminin). Les deux
  sont justes et aucune ne va à la place de l'autre · `high`.
- **`App` : `App` est la couleur de l'app, `Application` est la portée** · les options de couleurs désignent l'app comme
  SOURCE d'une teinte (étiquette de bouton très courte) ; `shortcuts.scope.app` désigne les raccourcis valables dans
  toute l'application · `high`.
- **`Running` : `En cours d'exécution` est un PROCESSUS, `En cours` est une tâche** · le serveur d'IA local tourne ;
  une opération du journal progresse. Le français distingue les deux · `high`.
- **`Rolling back` : le titre du dialogue prend les points de suspension, le statut prend `en cours`** ·
  `fileOperations.transferProgress.titleRollingBack` = `Retour en arrière...` (les points disent la progression) ;
  `operationLog.rollback.rollingBack` est une cellule de statut sans points, qui doit donc le dire en toutes lettres,
  en parallèle de `operationLog.status.running` = `En cours` · `high`.
- **`Canceled` : `Opération annulée` titre un panneau, `Annulé` est un statut** · les titres `errors.listing.*.title`
  nomment le sujet sous-entendu, comme `Interrupted` → `Opération interrompue` · `high`.
- **`Send feedback` : le TITRE et la commande disent `un retour`, le BOUTON d'envoi dit `le retour`** · c'est le motif
  déjà en place dans tout le catalogue : `errorReporter.dialog.title` = `Envoyer un rapport d'incident` contre
  `errorReporter.dialog.send` = `Envoyer le rapport`. Le titre nomme l'action en général ; le bouton agit sur l'objet
  précis qui est devant vous · `high`.
- **`Ask about your files` : le titre invite à plusieurs questions, le champ en attend une** · `askCmdr.empty.title`
  est l'état vide d'une conversation (`Posez des questions…`), `askCmdr.composer.placeholder` est le champ d'un seul
  message (`Posez une question…`). Le français explicite un nombre que l'anglais laisse ouvert · `high`.
- **`Modified` : `Modifié` est la DATE, `Modifiés` sont les raccourcis que vous avez changés** ·
  `shortcuts.section.filterModified` s'accorde au masculin pluriel avec les commandes, sans aucune date · `high`.
- **`Error` : `Problème` est un état lu par l'utilisateur, `Erreur :` est une étiquette de diagnostic** · les deux
  `@key` le disent explicitement · `high`.
- **`Search` : `Rechercher` est l'action, `Recherche` est le thème** · en français l'infinitif titre les dialogues et
  les boutons, mais une sous-section de la barre latérale des réglages est un nom · `high`.
- **`Put back …` : `restauré` concerne des NOMS, `remis en place` concerne des EMPLACEMENTS** · l'anglais réutilise une
  phrase pour deux annulations différentes · `high`.
- **`you@example.com` : les réglages gardent l'adresse telle quelle, les dialogues la traduisent** ·
  `@settings.updates.emailPlaceholder` dit « keep it exactly » (avec un `sameAsSourceJustification` à l'appui) ·
  `high`.
- **Sept « divergences » n'en sont pas, et voici pourquoi elles reviendront** : `AI suggestions` / `AI suggestions:`,
  `Connected` / `Connected!`, `Copied` / `Copied!`, `On disk` / `On disk:`, `Preview` / `Preview:`,
  `Send report` / `Send report?` et `Start using Cmdr` / `Start using Cmdr!` ont un anglais DIFFÉRENT. `i18n-terms` les
  regroupe quand même : son normalisateur retire la ponctuation FINALE, mais la typographie française met une espace
  insécable AVANT `: ! ?`, si bien que `Copié !` se réduit à `Copié ` (avec l'espace) et ne coïncide plus avec `Copié`.
  Ne touchez pas à ces quatorze valeurs : le défaut est dans le normalisateur, pas dans la traduction.
