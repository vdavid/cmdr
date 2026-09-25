# French (fr) translation style guide

Working notes for translating Cmdr into French. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into French. Term rulings live in `terms.json`
(keyed by the shared `../concepts.json`), their rationale in `decisions.md`, and open questions in `review-queue.md`.

This is the language base (`fr`), the universal French set and the fallback for any future region variants (`fr-CA`,
`fr-CH`). Stick to standard metropolitan French here; push region-specific phrasing into a variant only when one is
added.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: `vous` / `votre` / `vos` everywhere, like macOS French (not a single `tu` in Finder or AppKit). Never
  `tu`.
- **Voice**: friendly, concise, active, calm. Never `erreur`, `échec`, `a échoué`, or `bloqué`: state the observation
  and the way out. Fallbacks: a generic error status → `Problème`; a stopped operation → `N’a pas pu se terminer`;
  "Couldn’t X" → `<Nom verbal> impossible` (`Lecture du dossier impossible`), or `Cmdr n’a pas pu <verbe>` when the
  English names Cmdr; "Something went wrong" → `Un problème est survenu`. No apologies in a notice that reports a
  deliberate choice. Prefer a verb to a noun phrase (`Rechercher`, not `Effectuer une recherche`).
- **Register by UI slot**:
  - buttons, menu items, labels, dialog titles: the infinitive (`Envoyer`, `Annuler`, `Copier`, `Ajouter un serveur`); a
    title that asks is an elliptical infinitive question (`Supprimer le modèle d’IA ?`), never `Voulez-vous vraiment…`;
  - Settings toggle labels: infinitive (`Afficher les fichiers cachés`); their descriptions: third-person present for
    what the option does (`Affiche…`, `Détecte…`), `vous` imperative once they ask for an action
    (`Laissez ce champ vide…`);
  - prose, hints, toasts: `vous` imperative (`Ouvrez…`, `Réessayez dans un instant.`);
  - progress: a verbal noun with a trailing `…` (`Analyse…`, `Envoi…`, `Connexion à {name}…`); a bare verbal noun takes
    `en cours` to stand as a line (`Copie en cours`);
  - a toggle command names both verbs with `ou`, never a slash (`Afficher ou masquer les fichiers cachés`);
  - internal (hidden) settings: `Interne : …` labels and `Indique si … a été …` descriptions; seen-flags are a noun
    phrase plus an agreeing participle (`Astuce … affichée`).
- **Menu-bar and Apple names follow French macOS**: `Fichier`, `Édition`, `Présentation`, `Aller`, `Fenêtre`, `Aide`;
  `Précédent` / `Suivant` in the Go menu; `Réduire/agrandir` (Window > Zoom); `Lire les informations`; `Coup d’œil`;
  `Trousseaux d’accès` (the app) vs `le trousseau` (the store); `Réglages Système`; `Accès complet au disque`;
  `Utilitaire de disque`, `S.O.S`, `Moniteur d’activité`; `Aperçu` (the Preview app); `Mise à jour de logiciels`.
  Localize what Apple localizes, whatever a `@key` says. Kept English: `le Finder`, `le Dock`,
  `le dossier Applications`, Spotlight, Mission Control, Spaces, Terminal, TextEdit, Apple Silicon. On a phone,
  Android’s own French wins (`Autoriser`, `débogage USB`, `appuyez sur`); a third-party button takes that vendor’s
  French UI when it has one (GitHub’s `Ajouter une étoile`). Cmdr’s own window is `les Réglages`; Apple’s app is
  `Réglages Système`.
- **Capitalization**: sentence case. Accents on capitals are mandatory: `État`, `Éjecter`, `À propos`, `Écraser`.
- **Typography, as macOS French writes it** (`mechanics.json`, checked by `i18n-mechanics`): a no-break space U+00A0
  before `:`, `;`, `!`, `?`, and `%`, and inside guillemets (`« {name} »`); `“…”` nested inside them; the curly
  apostrophe `’` everywhere, ICU and RAW keys alike (it’s no ICU escape, so it’s never doubled). Never an ASCII space
  there, never U+202F, never a straight `'` or `"`. No serial comma before `et` / `ou`. Speed multipliers spell out
  `fois` (`4 fois plus lente`); units are French (`2 Go`).
- **No hedged grammar**: never `fichier(s)`, `connecté(e)`, `connecté·e`, `le/la`, or `de/d’` before an insert. ICU
  `plural` / `select` when Cmdr knows the value; otherwise restructure, as the next two bullets say.
- **Gender**: name the object or the action, never gender the user, and no midpoint glyphs: `Session fermée`, not
  `Déconnecté(e)`; `Vous y avez bien accès`, not `Vous êtes connecté`; `La personne qui gère ce Mac`, not
  `l’administrateur`. Cmdr is masculine (`Cmdr lui-même`, `il`).
- **Placeholders**: an inserted `{name}`, `{path}`, `{app}`, or `{host}` sits in a neutral slot, after a preposition
  that neither elides nor agrees (`dans`, `sur`, `pour`, `vers`, `à`) or as a bare subject; never after an elidable
  article or contraction, never before an agreeing participle. Don’t refer back with `il` / `elle` when the nearest
  masculine noun is something else (`le volume`, `le disque`): repeat the object
  (`donc le fichier a peut-être quand même été renommé`).
- **Plurals**: CLDR `one` / `many` / `other`; write `many` identical to `other`; French counts 0 as `one`.
- **Brand**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, `Safari`, `Ask Cmdr` stay verbatim. `Ask Cmdr` names only the chat
  panel; a sentence about what the product does says `Cmdr` or `l’IA`.
- **Top traps** (details in `terms.json`):
  - rollback → `revenir en arrière` / `retour en arrière`, never `restaurer` or `annuler`; Cancel and Undo are both
    `Annuler`.
  - operation → `opération` (feminine); the queue → `File d’attente des opérations`; the operation log →
    `Historique des opérations` (`journal` is the log FILE: `fichier journal`, `journalisation`).
  - dismiss → `Ignorer` (`Fermer` closes a window); skip → `Ignorer` too.
  - remove from a list → `Retirer`, never `Supprimer` (that is delete).
  - connect → `se connecter` (the network); sign in → `s’identifier`; signed out → `Session fermée`; logging in to the
    Mac → `à l’ouverture de session`.
  - move to trash → `placer dans la corbeille`, noun `Placement dans la corbeille`, never `Mise à la corbeille`.
  - chat → `conversation`, never `discussion` or `chat`; token → `jeton`; API key → `clé d’API`.
  - onboarding → `prise en main` everywhere; feedback → `retour`; crash and error reports → `rapport d’incident` (plain
    `rapport` when nothing crashed); quit unexpectedly → `a quitté inopinément`.
  - drive → `disque` (never `lecteur`); item → `élément`; device → `appareil`, a phone → `téléphone`.
  - F3 View → `Visualiser`; Show → `Afficher`; See → `Voir`; the View menu → `Présentation`.
  - a hidden file (its attribute) → `caché`; something the UI hides → `masqué`.
  - trust a key → `approuver`; approve an AI suggestion → `Approuver`; allow → `Autoriser`; deny, reject → `Refuser`.
  - Got it → `D’accord`; Later / Not now → `Plus tard`; a definitive No, thanks → `Non, merci`.
  - reach a host → `joindre` (`injoignable`); a path → `accéder à`.

## Voice and tone

Friendly, concise, active, and never alarmist. Cmdr in French should sound like a calm, competent peer, not a corporate
support desk. Match the English register: warm and direct, never stiff. Error and crash copy stays reassuring and
factual; avoid dramatizing words, and prefer active voice ("Nous avons envoyé…" over "… a été envoyé").

The English ban on "error" / "failed" covers their French equivalents: **never "erreur", "échec", "a échoué", and not
"bloqué" either** (it reads as a verdict, and often a wrong one). State the observation and the way out instead: "Aucune
progression depuis 45s", "En attente d’une réponse de la destination", "Le transfert n’avance plus. Annulez-le ou
laissez-le continuer en arrière-plan." The settled non-alarmist fallbacks are "problème" for a generic error status and
"N’a pas pu se terminer" for a failed operation.

French UI copy drifts long and noun-heavy; resist it. Prefer a verb where the English uses one ("Rechercher", not
"Effectuer une recherche"). The Microsoft French style guide pushes the same way (warm, relaxed, short everyday words
over formal register), and it’s the right tonal fit for Cmdr’s voice (verified against the reference pile,
`fr/microsoft-style-guides/StyleGuide.pdf`, 2026-06-21).

## Formality: `vous`, settled

**Address the user as "vous"** (the polite second person) throughout. This is settled from the sources, not a guess:

- macOS French is fully `vous`. Across the mined Finder + AppKit strings, every second-person address uses "vous" /
  "votre" / "vos" (443 such markers); there is not a single `tu` address. Finder phrases user prompts as "Voulez-vous
  vraiment …", "Vous pouvez …", "Saisissez le nom …" (verified against the reference pile, `fr/macOS/`, grep over
  Finder + AppKit, 2026-06-21).
- Microsoft French agrees: the style guide explicitly prescribes the second-person pronoun "vous" for addressing the
  user ("The second-person pronoun 'vous' and direct, natural language clearly tell the user …"), verified against
  `fr/microsoft-style-guides/StyleGuide.pdf`, 2026-06-21.
- "Tu" would feel too familiar for a file manager addressing an unknown adult user, and inconsistent tu/vous is jarring.
  So "vous" everywhere, no exceptions. (French is the inverse of Cmdr’s German, where macOS settles on informal `du`;
  the formality call is per-language, driven by the native OS register.)

**Imperatives for UI actions** (buttons, menu items): use the infinitive, the French UI convention ("Envoyer",
"Annuler", "Copier", "Renommer", "Ignorer"), not the imperative mood. The infinitive is the neutral, label-style form
Apple and most French macOS software use for commands (verified against the reference pile,
`fr/macOS/Finder/MenuBar.json`: "Copier", "Renommer", "Éjecter", "Trier par", 2026-06-21).

## Decision points

Formality is settled above (`vous`). These are the remaining French-specific calls.

- **Regional variant: one base `fr` (France norm), no `fr-CA` / `fr-CH` split needed yet.** Apple and Microsoft both
  ship a single metropolitan French for most products; Canadian French (fr-CA) is a deferred variant (see
  `../language-selection-decisions.md`). The UI-visible divergences are narrow (fr-CA tends to translate more
  anglicisms, e.g. "courriel" for email, and differs on some spacing conventions), and none touch the core file-manager
  vocabulary. Recommendation: ship one `fr` on the France norm; only add fr-CA if a Canadian user reports specific
  friction. Confidence: high.
- **Gendered grammar: name the object or action, never the user, and no midpoint glyphs.** French agent and adjective
  forms are gendered ("connecté" / "connectée"). Per the global rule, achieve inclusivity by neutral RESTRUCTURING,
  never by the midpoint ("connecté·e", "utilisateur·rice·s"): those break screen readers (against Cmdr’s AA+ a11y
  principle) and are politically loaded. Apple and Microsoft French both dodge gendering the user the same way: they
  address with "vous", and phrase status to agree with the object, not the person. So prefer "Connexion établie" over
  "Vous êtes connecté(e)", and "Partagé avec 3 personnes" over a gendered role noun. The masculine generic is the
  documented last resort, only when natural restructuring genuinely isn’t available; flag those for David rather than
  shipping the bare gendered default. Confidence: high.
- **Capitalization: sentence case, and keep the accent on capital letters.** The app’s sentence-case rule holds (only
  the first word and proper nouns capitalized; not English-style title case on every word). French additionally requires
  accents on capitals: write "État", "Éjecter", "Écraser", "À propos", never "Etat", "Ejecter", "A propos". macOS French
  is consistent on this ("Éjecter", "Écraser les extensions", "État", verified against the reference pile,
  `fr/macOS/Finder/`, 2026-06-21). A translator must keep the accent even on an all-caps or sentence-initial capital.
  Confidence: confirmed (French orthography).
- **Punctuation: a no-break space before `: ; ! ? %` and inside « ».** macOS French uses U+00A0 in every one of these
  places (the counts are in `mechanics.json`), so the catalog does too. It’s the French convention a translator working
  from English copy drops most often. Confidence: confirmed (French typography; macOS follows it).

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` (and, until they’re merged,
`concepts-proposed.json`): `chosen`, accepted forms, usage notes, forms to avoid with the reason, a confidence
(`confirmed` / `high` / `tentative`), and sources. Tier order is macOS (Tier 1, because Cmdr is a macOS app and this is
what the user sees in Finder) → Microsoft (Tier 2) → the explorer and orthodox file managers (Tier 3). A vendor’s own
French UI (Apple, Android, GitHub) beats a `@key` description. French verbs stay lowercase in running text and
infinitive-capitalized as button labels; nouns are lowercase unless sentence-initial or proper. Rationale worth more
than a line sits in `decisions.md` under a heading that cites its keys, and the term’s `decision` field names that
heading. Never guess a term: mine the reference pile first (`../reference-pile/how-to-mine.md`).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Safari, Ask Cmdr, plus the `{system_settings}`-style
tokens and any `{email}`-style placeholders. Enforced by `desktop-i18n-dont-translate`; curated list in
`apps/desktop/scripts/i18n-catalog-lib.ts`. Apple localizes Quick Look (`Coup d’œil`) and Keychain (`trousseau`), so
they’re not on the list. macOS UI names Cmdr opens into (System Settings panes, "Corbeille") should match a French
macOS. Product and command names stay verbatim too: Android SDK, Android Platform Tools (capitalized as the product even
where the English lowercases it), Homebrew, `adb` (the command, lowercase), `ADB` (the protocol and feature name),
`ptpcamerad`, `udev`.

## Plurals

French CLDR plural categories: `one`, `many`, `other` (verified with
`new Intl.PluralRules('fr').resolvedOptions().pluralCategories`). French treats 0 and 1 as singular ("one"), and `many`
covers large/compact-notation values (e.g. "2 millions"). Write the branches the message needs, not English’s two.

- Adjective and past-participle agreement must match the counted noun’s gender and number in every branch ("1 fichier
  sélectionné" / "{count} fichiers sélectionnés"; "1 image copiée" / "{count} images copiées"). Get the agreement right
  inside each branch.
- French nouns have gender (le/la); the article and any adjective must agree with the counted noun.
- **Pull a trailing clause INSIDE the branches when it has to agree.** English often leaves text after the closing `}}`
  ("… {count, plural, …} and may already be partly written."); French can’t when that text carries an agreeing
  participle. Move the whole sentence into each branch. The parity check compares only the placeholder SET
  (`apps/desktop/scripts/i18n-check-parity.ts`), so this is safe, not a structure break. See
  `fileOperations.transferProgress.stallInFlight`.
- **A counted tail with NO plural param has nothing to agree with.** A message that passes only the formatted
  `{somethingText}` for a second count gives you no integer to select on. English gets away with it ("stayed" fits 1 and
  12); French doesn’t. The fallback is to drop the finite verb and let the first half’s noun carry the clause : « 12
  fichiers remis en place ; 3 toujours dans la corbeille. » Ask for an integer partner instead where you can :
  `fileOperations.trash.undonePartial` gained a `{skipped}` driver for exactly this reason, so its second half now
  accords normally : « … ; {skippedText} {skipped, plural, one {élément est resté} many {éléments sont restés} other
  {éléments sont restés}} dans la corbeille. »
- **❌ Jamais d’article défini devant `{countText}`.** L’anglais écrit sans broncher « the {countText} items », y
  compris quand la branche `one` donne « the 1 item » ; « les 1 élément » est simplement faux. Quand la source marque la
  totalité par `the`, portez-la par `tout` et sortez le nombre de la portée de l’article : « Cmdr a supprimé tout ce
  qu’il avait écrit : {countText} éléments. » Exemple travaillé : `decisions.md` § La notification après un retour en
  arrière interrompu.
- Write `many` identical to `other` unless the message really formats compact/large values: plain integers never select
  `many`, but parity and the plural check both want the branch. This is what the whole `fr` set does.

## Notes and decisions

- **Les menus natifs suivent la formulation du Finder, pas celle du catalogue.** Là où macOS a un équivalent, il
  l’emporte (`Présentation`, `Départ`, `Réduire/agrandir`, `Coup d’œil`), parce que l’utilisateur voit la barre de menus
  de Cmdr à côté de celle du Finder. Preuves et exceptions : `decisions.md` § Menus natifs. Le menu du Dock suit la même
  règle : « Aller au dossier… » et « Se connecter au serveur… » sont copiés du menu Aller du Finder, et « Connexion au
  serveur » (le titre de la fenêtre chez Apple) n’est jamais un libellé de commande. Preuves : `decisions.md` § Le menu
  du Dock.
- **Accents on capitals are mandatory.** "État", "Éjecter", "À propos", "Écraser", never strip the accent on a capital
  (see Decision points → Capitalization). macOS French keeps them.
- **Punctuation spacing**: a no-break space (U+00A0) before `: ; ! ? %`, the character macOS French uses there
  (`« ^0 » :`, `^0 %`). It also keeps the mark from wrapping onto a line of its own. Never U+202F, never an ASCII space.
  Times, URLs, and a value that is only the mark (`?`) stay tight.
- **Quotation marks**: guillemets « … » with a no-break space inside, `“…”` for a quote inside a quote, like macOS ("Nom
  du nouveau dossier à l’intérieur de « ^0 » :").
- **No serial comma.** French lists join the last item without a comma before `et` / `ou`
  (`clair, sombre ou suivant le système`); a comma before `et` / `ou` only separates two independent clauses
  (`…, et Cmdr …`). The English Oxford comma is the most common calque in this catalog; check every list.
- **Apostrophes**: always the curly `’` (U+2019), the one macOS French writes (1,247 times in the pile against 3 ASCII).
  It isn’t an ICU escape character, so it’s written once in every family, ICU and RAW alike (`d’incident`,
  `Coup d’œil`), and French elision’s many apostrophes never meet the ICU doubling trap. A straight `'` is a
  `i18n-mechanics` finding.
- **Length**: French runs roughly 15–20% longer than English. Overflow-check the layout against the pseudolocale
  (`en-XA`); look for clipped buttons, labels, and toasts. Known tight spots: `review-queue.md` § Overflow.
- **Numbers and dates come from the formatter layer** (French uses a comma decimal and a narrow space for thousands).
  Never hardcode separators.
- **Restructurable placeholders**: a `{name}` or `{path}` that lands next to a gendered adjective forces agreement the
  catalog can’t resolve. Phrase so the placeholder stays in a neutral slot (name the object, not an agreeing
  participle): this is the same discipline as the gender rule above.
- **Speed multipliers spell out "fois".** English UI writes `4x` / `100x`; French prose writes "4 fois plus lente",
  "parfois 100 fois". Keep the digit, drop the `x`: the bare `x` form reads as English marketing copy. See
  `decisions.md` § Notification de repli sur le montage macOS.
- **« Couldn’t <do X>: {error} » se rend par « <Nom verbal> impossible : {error} ».** Moule figé du catalogue, qui évite
  « erreur » / « échec » tout en restant court : « Préparation de l’aperçu impossible : », « Envoi du rapport d’incident
  impossible : », « Enregistrement du lot impossible : », « Ajout de votre note impossible : ». Nom verbal +
  `impossible` + espace insécable + `:`, jamais « Impossible de… » en tête quand la valeur suit déjà un deux-points
  ailleurs (voir la puce suivante).
- **Une valeur insérée après un deux-points ne redit pas le moule.** Plusieurs clés `errors.*` sont livrées dans une
  notification qui porte déjà « Impossible de … : », donc la valeur commence directement par l’observation et jamais par
  un second « Impossible de … ». Elle garde sa majuscule initiale, comme toutes les valeurs `errors.*`. Exemple
  travaillé : `decisions.md` § Éjection et déconnexion refusées.
- **« Voici ce que vous pouvez essayer : »** ouvre chaque liste de suggestions à puces (« Here’s what to try: »), avec
  l’espace insécable avant le deux-points. Les jetons de volets (`{system_settings}`, `{privacy_and_security}`,
  `{files_and_folders}`, `{full_disk_access}`) sont remplacés à l’exécution par les noms que le Mac de la personne
  affiche : ne les traduisez pas. Détails : `decisions.md` § Les noms de sous-fenêtres viennent du Mac de la personne.
- **« appareil photo » + « photo » dans la même proposition : passer par l’incise `pour une photo, …`.** « les détails
  de l’appareil photo d’une photo » colle deux `photo` ; on écrit
  `…, et, pour une photo, les détails de l’appareil photo et la localisation`. Termes et preuves : `decisions.md` § Ce
  qu’Ask Cmdr lit à l’intérieur d’un fichier.
- **La ligne Android (ADB) des réglages.** `settings.fileOperations.adbEnabled.*` et `adbBinaryPath.*` suivent le moule
  déjà en place pour MTP : le libellé décrit la fonction ("Accès aux fichiers Android via ADB", comme "Prise en charge
  Android/Kindle/appareil photo"), la description commence à la troisième personne quand elle décrit ce que fait
  l’option ("Donne accès à…", "Détecte et se connecte…") et passe au « vous » impératif dès qu’elle demande une action
  ("Laissez ce champ vide et…", "Indiquez un chemin si…"). Le connecteur reste « via » pour un lien matériel ou un
  protocole, comme dans les clés MTP voisines.
- **La forme grisée « (busy) » d’un élément de menu : le libellé de base, mot pour mot, suivi de ` (occupé)`.** Un seul
  marqueur pour tout le catalogue, fixé par `menu.volume.ejectBusy` (« Éjecter ({name}) (occupé) ») et repris tel quel
  par `menu.volume.disconnectBusy`, `forgetSavedPasswordBusy`, et `forgetServerBusy` : « Se déconnecter (occupé) », «
  Oublier le mot de passe enregistré (occupé) », « Oublier le serveur (occupé) ». La parenthèse ne s’accorde pas : elle
  qualifie le volume ou le serveur visé (masculin), pas le complément du libellé, donc `(occupé)` reste invariable même
  après « le mot de passe enregistré ». N’inventez jamais un second marqueur (« en cours d’utilisation », « indisponible
   ») : la paire actif/grisé doit se lire comme un seul élément dans deux états. Terme : `terms.json` `busy`.
- **Une commande « à bascule » écrit les deux verbes avec « ou », jamais avec une barre oblique.** L’anglais
  `Pin / unpin server` devient « Épingler ou désépingler le serveur », sur le moule de `commands.tabTogglePin.label` («
  Épingler ou désépingler l’onglet »). Les deux se lisent côte à côte dans la palette de commandes : une barre oblique
  chez l’une et « ou » chez l’autre ferait croire à deux commandes de nature différente. Preuves : `decisions.md` § Le
  hub des serveurs : la table.
- **La fenêtre de réglages de Cmdr, c’est « les Réglages » ; « Réglages Système » nomme l’app d’Apple.** Un lien du type
  « Turn it on in Settings » se rend par « Activez-la dans les Réglages », comme
  `fileExplorer.navigation.driveIndex.tooltipIndexingOff` déjà livrée.
- **`Reconnexion` est le nom d’état, `reconnecter` le verbe.** `Reconnecting to {name}…` suit exactement le moule du
  voisin `Connexion à {name}…` : « Reconnexion à {name}… ». Preuves Apple et pièges : `decisions.md` § Le panneau de
  reconnexion.
- **Un constat « il n’y a rien à faire » se rend par `…, il n’y a donc rien à <verbe>.`** Moule déjà livré par
  `errors.eject.*` (« … il n’y a donc rien à éjecter. ») et repris par `servers.paneState.signedOutNothingToAsk` (« … il
  n’y a donc rien à saisir. »). Ni excuse ni « erreur » : c’est un constat. Le verbe pour remplir un champ est `saisir`
  (Finder), jamais `taper`.
- **« Ask Cmdr » ne nomme QUE le panneau de discussion, jamais l’IA en général.** La marque survit là où elle désigne la
  surface elle-même : le titre du panneau (`askCmdr.title`), l’élément du menu Présentation (`menu.view.askCmdr`), la
  commande de la palette (`commands.askCmdrToggle.label`), la section des réglages (`settings.section.askCmdr`),
  l’interrupteur qui l’active (`settings.askCmdr.enabled.label`), et toute phrase qui renvoie à cette section (« dans
  les réglages d’Ask Cmdr », « dans la section Ask Cmdr »). Partout ailleurs, la phrase décrit ce que fait le produit et
  le sujet est **Cmdr** (« Cmdr surveille les dossiers… », « Ce que Cmdr envoie ») ou, quand elle parle du modèle plutôt
  que de l’app, **l’IA** (« L’IA a suggéré ceci », « Raison donnée par l’IA »). L’anglais fait exactement ce partage clé
  par clé : suivez-le, ne réintroduisez pas la marque dans une phrase descriptive et ne la retirez pas d’un renvoi à la
  section.
- **Un renvoi à un réglage macOS réutilise le libellé du bouton qui l’ouvre, mot pour mot.** « Click to set up full disk
  access. » se termine par « Cliquez pour configurer l’accès complet au disque. », où « configurer l’accès complet au
  disque » est exactement `search.coverage.setUpFullDiskAccess` : les deux surfaces mènent au même panneau des Réglages
  Système et doivent le nommer pareil. Même règle pour un renvoi vers une commande de Cmdr
  (`Rendre disponible hors connexion`) ou un réglage (`Afficher les fichiers cachés`).
- **Un `{placeholder}` qui porte un libellé déjà traduit se met dans un créneau sans article ni accord.** Son genre est
  inconnu à l’écriture : « En savoir plus sur {topic} » marche pour « Réseau » comme pour « Indexation du disque », là
  où « Plus d’infos sur le {topic} » casserait un mot sur deux. Même discipline que la règle de genre : nommez l’action
  ou l’objet, ne pariez jamais sur la forme de l’insertion.
- **Le verbe d’un bouton tiers vient de l’interface française de cet éditeur, quand elle existe.** GitHub localise la
  sienne et dit « Ajouter une étoile » ; on la reprend. Quand l’éditeur ne localise pas du tout (AlternativeTo, dont le
  bouton dit « Like » à tout le monde), il n’y a pas de terme à citer : on prend le verbe français standard de ce geste
  et on marque le choix `tentative`. Preuves : `decisions.md` § La prise en main : l’assistant, l’étape IA et la liste
  de contrôle.

## Open questions

Subjective calls and coined terms that a native reviewer should confirm live in `review-queue.md`; each already ships a
reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `concepts-proposed.json`: concepts this locale needed that `../concepts.json` doesn’t have yet, to be merged there.
- `terms.json`: this locale’s ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: distilled rulings ("X over Y because Z"), one section per surface, headings citing their keys.
- `mechanics.json`: the typography set (quotes, apostrophe, spacing) and the hedge patterns `i18n-mechanics` checks.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line.
