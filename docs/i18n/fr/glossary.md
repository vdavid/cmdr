# fr glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/fr/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

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
- rollback (undo a transfer) → annuler et restaurer (button) / restauration (noun) / annulation (the rolling-back
  action) · no exact macOS term; "annuler et restaurer" spells out the stop-and-undo for the button, "restauration" for
  the noun, kept calm · tentative
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
- queue (transfer queue) → file d''attente · Double Commander ("Queue" → "File d''attente", "Add To Queue" → "Ajouter à
  la file d''attente", pervasive), MS terminology FRA ("file d''attente", 36+ hits) · high — "Transfer queue" → "File
  d''attente des transferts"; the standalone Queue button on the progress dialog → "File d''attente".
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

- Action (field label before the Copy/Move or Trash/Delete segmented control) → "Action :" · "Action" is a genuine
  French word (identical spelling), pile-pervasive as a UI noun (macOS Finder/AppKit "Action", MS terminology FRA
  "action"); the FR rendering differs from EN only by the catalog-wide ASCII space before the colon, so it's NOT
  identical-to-English (no justification needed) · high
- Route (field label before the "source → destination" line in the copy/move dialog) → "Trajet :" · NO settled pile term
  — in the references "route" is networking-only (MS terminology "routeur"/"route" = router/network route; macOS has no
  source-to-destination label). "Trajet" is the natural FR for the from-A-to-B movement of a transfer (a journey/route
  you travel), fitting the metaphor better than the filesystem-path "Chemin" (already used for `destPathAria` "Chemin de
  destination") or the heavier "Itinéraire". ASCII space before the colon per the catalog-wide spacing rule · tentative
  (Cmdr coinage, no pile source; matches the EN's own metaphorical "Route")
- "Scanning…" (spinner tooltip + SR label while the dialog counts selected items) → "Analyse…" · reuses the settled
  `scanning (transfer stage) → analyse` term (`transferProgress.stageScanning` = "Analyse"); the single … char kept
  verbatim (EN uses one … glyph, not three dots) · high
- "Scan complete" (checkmark tooltip + SR label once counting finished) → "Analyse terminée" · "analyse" is feminine, so
  the past participle agrees "terminée"; matches the settled `Done → Terminé` pattern · high
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
