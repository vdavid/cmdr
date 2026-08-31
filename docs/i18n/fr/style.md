# French (fr) translation style guide

Working notes for translating Cmdr into French. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into French.

This is the language base (`fr`), the universal French set and the fallback for any future region variants (`fr-CA`,
`fr-CH`). Stick to standard metropolitan French here; push region-specific phrasing into a variant only when one is
added.

## Voice and tone

Friendly, concise, active, and never alarmist. Cmdr in French should sound like a calm, competent peer, not a corporate
support desk. Match the English register: warm and direct, never stiff. Error and crash copy stays reassuring and
factual; avoid dramatizing words. As in English, steer clear of "erreur" / "échec" framing in user-facing copy where a
calmer phrasing works, and prefer active voice ("Nous avons envoyé…" over "… a été envoyé").

The English ban on "error" / "failed" covers their French equivalents: **never "erreur", "échec", "a échoué", and not
"bloqué" either** (it reads as a verdict, and often a wrong one). State the observation and the way out instead: "Aucune
progression depuis 45s", "En attente d'une réponse de la destination", "Le transfert n'avance plus. Annulez-le ou
laissez-le continuer en arrière-plan." The settled non-alarmist fallbacks are "problème" for a generic error status and
"N'a pas pu se terminer" for a failed operation.

French UI copy drifts long and noun-heavy; resist it. Prefer a verb where the English uses one ("Rechercher", not
"Effectuer une recherche"). The Microsoft French style guide pushes the same way (warm, relaxed, short everyday words
over formal register), and it's the right tonal fit for Cmdr's voice (verified against the reference pile,
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
  So "vous" everywhere, no exceptions. (French is the inverse of Cmdr's German, where macOS settles on informal `du`;
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
  vocabulary below. Recommendation: ship one `fr` on the France norm; only add fr-CA if a Canadian user reports specific
  friction. Confidence: high.
- **Gendered grammar: name the object or action, never the user, and no midpoint glyphs.** French agent and adjective
  forms are gendered ("connecté" / "connectée"). Per the global rule, achieve inclusivity by neutral RESTRUCTURING,
  never by the midpoint ("connecté·e", "utilisateur·rice·s"): those break screen readers (against Cmdr's AA+ a11y
  principle) and are politically loaded. Apple and Microsoft French both dodge gendering the user the same way: they
  address with "vous", and phrase status to agree with the object, not the person. So prefer "Connexion établie" over
  "Vous êtes connecté(e)", and "Partagé avec 3 personnes" over a gendered role noun. The masculine generic is the
  documented last resort, only when natural restructuring genuinely isn't available; flag those for David rather than
  shipping the bare gendered default. Confidence: high.
- **Capitalization: sentence case, and keep the accent on capital letters.** The app's sentence-case rule holds (only
  the first word and proper nouns capitalized; not English-style title case on every word). French additionally requires
  accents on capitals: write "État", "Éjecter", "Écraser", "À propos", never "Etat", "Ejecter", "A propos". macOS French
  is consistent on this ("Éjecter", "Écraser les extensions", "État", verified against the reference pile,
  `fr/macOS/Finder/`, 2026-06-21). A translator must keep the accent even on an all-caps or sentence-initial capital.
  Confidence: confirmed (French orthography).
- **Punctuation: a space before `: ; ! ?`, and guillemets « ».** French typography wants a thin space here; this catalog
  is settled on a **regular ASCII space** (uniform across the whole `fr` set, not U+202F) — see Notes for the rationale,
  the exact characters, and the ICU apostrophe trap. Folded into Notes because it's mechanical; flagged here because the
  space-before-`:` is the French convention most likely to be dropped by a translator working from English copy.
  Confidence: confirmed (French typography; macOS follows it).

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Sources cite concrete evidence; tier order is macOS
(highest, Tier 1, because Cmdr is a macOS app and this is what the user sees in Finder) → Microsoft (Tier 2) →
explorer/orthodox file managers (Tier 3). Confidence is `confirmed` (human signed off), `high` (authoritative sources
agree), or `tentative` (sources conflict or none had it). French verbs stay lowercase in running text and
infinitive-capitalized as button labels; nouns are lowercase unless sentence-initial or proper.

Straightforward (sources agree, `high`):

- file → fichier · macOS Finder ("Fichier", "Taille du fichier", "Navigateur de fichiers"), MS terminology FRA · high
- folder → dossier · macOS Finder ("Nouveau dossier", "Impossible de créer le dossier."), MS terminology FRA · high
- directory → répertoire · MS terminology FRA; technical sense only, prefer "dossier" for the UI · high
- drive → disque · macOS Finder ("Disque de démarrage" for both "Startup Disk" and "Startup Drive", "Disques externes",
  "Disques durs") · high
- trash → corbeille · macOS Finder ("Corbeille", "Vider la corbeille"), consistent everywhere · high
- delete → supprimer · macOS AppKit ("Delete"→"Supprimer", "Supprimer des favoris"); "Erase"→"Effacer" is the
  reformat/wipe sense, keep distinct · high
- copy → copier · macOS Finder/MenuBar ("Copier", "Copier en tant que lien") · high
- move → déplacer · macOS Finder ("Déplacer les éléments ici", "Copier et déplacer ${sources} vers ${destination}") ·
  high
- rename → renommer · macOS Finder ("Renommer", "Renommer les éléments du Finder :"), Nautilus ("Renommer") · high
- eject → éjecter · macOS AppKit ("NSNavEjectButton"→"éjecter"), Finder ("Éjecter", "Tout éjecter"), Nautilus
  ("Éjecter") · high
- disconnect → se déconnecter · macOS Finder ("Disconnect"→"Se déconnecter") · high
- server → serveur · macOS Finder ("Serveur :", "Serveurs favoris :", "Volumes serveur") · high
- search → rechercher (verb) / recherche (noun) · macOS Finder ("Rechercher :", "Recherche prédéfinie") · high
- sort → trier · macOS Finder MenuBar ("Trier par") · high
- settings → réglages · macOS names the preferences pane "Réglages" (modern macOS); Finder MenuBar still shows the older
  "Préférences…" · high
- cancel → annuler · macOS AppKit/Finder, pervasive ("Annuler") · high
- overwrite → écraser · macOS Finder ("Écraser les extensions", "Écraser à la destination", "… doivent être écrasés") ·
  high
- index / indexing → index / indexation · MS terminology FRA ("index"), macOS Finder ("Mise à jour de l'index des tags",
  "Indexé") · high
- transfer → transfert · MS terminology FRA ("transfert"), macOS Touch Bar ("NSTouchBarTransferDownloadTemplate") · high
- tab → onglet · macOS Finder MenuBar ("Nouvel onglet", "Masquer la barre d'onglets"), Double Commander ("Onglets
  ouverts") · high
- bookmark / favorite → favori (plural favoris) · macOS Finder ("Favoris", "Serveurs favoris :", "Supprimer des
  favoris") · high
- sidebar → barre latérale · macOS Finder ("barre latérale", "afficher/masquer la barre latérale") · high
- download → téléchargement (noun) / télécharger (verb) · macOS ("Téléchargements", "NSTouchBarDownloadTemplate"→
  "télécharger") · high
- pane → panneau · Double Commander ("Vers le panneau", "Copier dans le même panneau"), MS terminology FRA ("panneau") ·
  high
- file list / listing → liste des fichiers · Double Commander ("la liste des fichiers", "Alterner entre la liste de
  gauche et celle de droite") · high
- command line → ligne de commande · Double Commander ("Ajouter le nom du fichier dans la ligne de commande") · high
- share (network) → partage (noun) / partager (verb) · macOS Finder ("Partage et permissions :", "Partager…", "Partagé
  par") · high
- undo → annuler · macOS AppKit MenuCommands ("Undo Smart Dash" → "Annuler Tirets intelligents"), GNOME Nautilus
  ("Undo" → "Annuler") · high. French renders both `Undo` and `Cancel` as "Annuler"; macOS lives with the same
  ambiguity, so keep it. "Remettre" (Finder's "Put Back") is the sourced fallback if a surface ever needs the two apart.
- put back (an item from the trash, to where it was) → remettre en place · macOS Finder ("Put Back" → "Remettre") ·
  high. Not `restaurer`, which the catalog spends on undoing a RENAME (`askCmdr.renameUndo.*`).
- view (consult information) → voir; view (open a file's content) → afficher · the catalog's own split
  (`menu.app.licenseDetails` "Voir les détails de la licence" vs `fileExplorer.functionKeyBar.viewAction` "Afficher le
  fichier") · high. "Voir" is also the shorter of the two, which decides tight toast buttons.
- add to (attach something to an existing object) → ajouter à · macOS Finder/AppKit ("Ajouter à la barre latérale",
  "Ajouter au Dock", "Ajouter aux favoris"): French takes the bare `Ajouter à X` with no explicit object, exactly like
  English · high

Contested or sense-specific (read the block):

- item → élément · macOS vs Microsoft · high
  - macOS Finder calls a file-or-folder row an "élément" pervasively ("Obtenir les éléments sélectionnés", "Compresser
    des éléments", "Déplacer les éléments ici", "Placer ${entities} dans la corbeille"). Use "élément" for the generic
    file-or-folder entity. Microsoft terminology's first hit is "article": that's the wrong sense for a UI row; don't
    use it. macOS (Tier 1) wins.
- move to trash → placer dans la corbeille · macOS vs explorer family · high
  - macOS Finder phrasings: "Trash ${entities}"→"Placer ${entities} dans la corbeille", "Moves items to the Trash"→
    "Place des éléments dans la corbeille". Prefer "placer dans la corbeille" to stay consistent with macOS. GNOME
    Nautilus uses "Mettre à la corbeille", fine French, but pick the macOS form since Cmdr is a macOS app.
- volume → volume · macOS · high
  - macOS keeps "Volume" for a mounted disk volume ("Volume", "Volumes serveur", "Sélectionnez un volume pour le
    remplacement"). Same word as English; capitalize only when sentence-initial or a label. Don't reach for an
    audio-volume sense.
- sidebar → barre latérale (not "encadré") · macOS vs Microsoft · high
  - macOS uses "barre latérale" for the Finder side panel. Microsoft terminology's "encadré" is the publishing
    sidebar/callout sense and doesn't fit a navigation pane. Use "barre latérale" (Tier 1).
- listing → liste des fichiers / présentation par liste · sense split · high for "the file-list pane", tentative for
  "list view"
  - For the file list a pane shows, use "liste des fichiers" (the orthodox term, see above). macOS calls the list _view
    mode_ "présentation par liste" / "Liste": that's the view-style sense, not the pane content. Keep the two senses
    distinct; if a single short label is needed for the pane content and "liste des fichiers" is too long, "liste" alone
    is the fallback; confirm with David which reads best in context.

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{email}`-style placeholder
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`. macOS UI
names Cmdr opens into (System Settings panes, "Corbeille") should match a French macOS.

Existing crash-reporter term choices (keep consistent across the catalog):

- crash report → rapport d'incident · "incident" is Microsoft FRA's settled non-alarmist noun for a crash (`crash` →
  "incident", `crash recovery` → "récupération sur incident", `crash telemetry` → "télémétrie des incidents"); avoid
  "rapport de plantage", which is more colloquial and which the pile only attests as a glossary gloss (`crash group` →
  "groupe de plantages"). Apple is NOT a source here: French macOS leaves "Crash Reporter" in English
  (`fr/macOS/AppKit/NSExceptionAlert.json`) and has no "rapport d'incident" anywhere (verified against the reference
  pile, 2026-08-23).
- crashed / quit unexpectedly → s'est fermé de façon inattendue · the shipped value in
  `crashReporter.dialog.body.ended`. It deliberately does NOT follow macOS, which writes "a quitté inopinément"
  (`fr/macOS/AppKit/AppKitErrors.json`: "The last time you opened %@, it unexpectedly quit…" → "Lors de sa précédente
  ouverture, %@ a quitté inopinément…", verified 2026-08-23). Keep the catalog's calmer paraphrase; don't re-source it
  to Apple, and don't write the gendered "fermé(e)" (the subject is Cmdr, masculine).
- kept running (the APP carried on after a background problem) → est resté ouvert · never "a continué de fonctionner";
  see `glossary.md` § Le rapport quand Cmdr n'a PAS quitté
- report, with no "crash" (the non-crash variants of the dialog body) → rapport, plain · the sibling `.ended` value
  minus "d'incident", the one word that carries "crash"
- Report ID → identifiant du rapport
- Updates → Mises à jour · in-app navigation section
- Send → Envoyer
- Dismiss → Ignorer · "Ignorer" fits a non-destructive dismiss better than "Fermer" here
- Always → Toujours

## Plurals

French CLDR plural categories: `one`, `many`, `other` (verified with
`new Intl.PluralRules('fr').resolvedOptions().pluralCategories`). French treats 0 and 1 as singular ("one"), and `many`
covers large/compact-notation values (e.g. "2 millions"). Write the branches the message needs, not English's two.

- Adjective and past-participle agreement must match the counted noun's gender and number in every branch ("1 fichier
  sélectionné" / "{count} fichiers sélectionnés"; "1 image copiée" / "{count} images copiées"). Get the agreement right
  inside each branch.
- French nouns have gender (le/la); the article and any adjective must agree with the counted noun.
- **Pull a trailing clause INSIDE the branches when it has to agree.** English often leaves text after the closing `}}`
  ("… {count, plural, …} and may already be partly written."); French can't when that text carries an agreeing
  participle. Move the whole sentence into each branch. The parity check compares only the placeholder SET
  (`apps/desktop/scripts/i18n-check-parity.ts`), so this is safe, not a structure break. See
  `fileOperations.transferProgress.stallInFlight`.
- **A counted tail with NO plural param has nothing to agree with.** A message that passes only the formatted
  `{somethingText}` for a second count gives you no integer to select on. English gets away with it ("stayed" fits 1 and
  12); French doesn't. The fallback is to drop the finite verb and let the first half's noun carry the clause : « 12
  fichiers remis en place ; 3 toujours dans la corbeille. » Ask for an integer partner instead where you can :
  `fileOperations.trash.undonePartial` gained a `{skipped}` driver for exactly this reason, so its second half now
  accords normally : « … ; {skippedText} {skipped, plural, one {élément est resté} many {éléments sont restés} other
  {éléments sont restés}} dans la corbeille. »
- **❌ Jamais d'article défini devant `{countText}`.** L'anglais écrit sans broncher « the {countText} items », y
  compris quand la branche `one` donne « the 1 item » ; « les 1 élément » est simplement faux. Quand la source marque la
  totalité par `the`, portez-la par `tout` et sortez le nombre de la portée de l'article : « Cmdr a supprimé tout ce
  qu'il avait écrit : {countText} éléments. » Exemple travaillé : `glossary.md` § La notification après un retour en
  arrière interrompu.
- Write `many` identical to `other` unless the message really formats compact/large values: plain integers never select
  `many`, but parity and the plural check both want the branch. This is what the whole `fr` set does.

## Notes and decisions

- **Les menus natifs suivent la formulation du Finder, pas celle du catalogue.** Là où macOS a un équivalent, il
  l'emporte (`Présentation`, `Départ`, `Réduire/agrandir`, `Coup d'œil`), parce que l'utilisateur voit la barre de menus
  de Cmdr à côté de celle du Finder. Preuves et exceptions : `glossary.md` § Menus natifs.
- **Accents on capitals are mandatory.** "État", "Éjecter", "À propos", "Écraser", never strip the accent on a capital
  (see Decision points → Capitalization). macOS French keeps them.
- **Punctuation spacing**: French typography puts a thin space before `: ; ! ? %`. The standard permits a real narrow
  no-break space (U+202F) where the context supports it OR a regular space, the binding rule being "stay consistent
  within the catalog". **SETTLED for this catalog: a regular ASCII space (0x20).** The catalog uses a plain space before
  `: ? ! %` throughout, matching the `de` sibling and the bulk of the fr files; the three files that had shipped U+202F
  (`commands.json`, `errors.json`, `queryUi.json`) were normalized to a regular space so the whole `fr` set is uniform.
  Use a regular space here; do NOT reintroduce U+202F (it would re-split the catalog).
- **Quotation marks**: use French guillemets « … » with inner spacing when quoting, not English "…". macOS follows this
  ("Nom du nouveau dossier à l'intérieur de « ^0 » :").
- **Apostrophes**: in ICU strings (everything outside `errors.*`), double every apostrophe (`d''incident`). In
  `errors.*` keys, use normal apostrophes. The crash-reporter strings are ICU, so they double. French elision makes
  apostrophes frequent ("d'incident", "l'élément", "n'a pas"): this trap bites more often in French than in most
  languages, so check every value. **Always the ASCII apostrophe (U+0027), never the curly U+2019**, even when the
  English source string uses the curly one: the whole `fr` catalog is ASCII, and a curly apostrophe is not an ICU escape
  character, so it slips past every check as a silent consistency break.
- **Ellipsis**: keep the source's literal three dots ("Envoi...") rather than swapping to a single … character, to match
  the English catalog value.
- **Length**: French runs roughly 15–20% longer than English. Overflow-check the layout against the pseudolocale
  (`en-XA`); look for clipped buttons, labels, and toasts.
- **Numbers and dates come from the formatter layer** (French uses a comma decimal and a narrow space for thousands).
  Never hardcode separators.
- **Restructurable placeholders**: a `{name}` or `{path}` that lands next to a gendered adjective forces agreement the
  catalog can't resolve. Phrase so the placeholder stays in a neutral slot (name the object, not an agreeing
  participle): this is the same discipline as the gender rule above.
- **Speed multipliers spell out "fois".** English UI writes `4x` / `100x`; French prose writes "4 fois plus lente",
  "parfois 100 fois". Keep the digit, drop the `x`: the bare `x` form reads as English marketing copy. See `glossary.md`
  § Notification de repli sur le montage macOS.
- **« Couldn't <do X>: {error} » se rend par « <Nom verbal> impossible : {error} ».** Moule figé du catalogue, qui évite
  « erreur » / « échec » tout en restant court : « Préparation de l'aperçu impossible : », « Envoi du rapport d'incident
  impossible : », « Enregistrement du lot impossible : », « Ajout de votre note impossible : ». Nom verbal +
  `impossible` + espace ASCII + `:`, jamais « Impossible de… » en tête quand la valeur suit déjà un deux-points ailleurs
  (voir la puce suivante).
- **Une valeur insérée après un deux-points ne redit pas le moule.** Plusieurs clés `errors.*` sont livrées dans une
  notification qui porte déjà « Impossible de … : », donc la valeur commence directement par l'observation et jamais par
  un second « Impossible de … ». Elle garde sa majuscule initiale, comme toutes les valeurs `errors.*`. Exemple
  travaillé : `glossary.md` § Éjection et déconnexion refusées.
- Record case-by-case rulings here.

## Decisions to confirm with David

The formality (`vous`), move, and item calls are settled from the sources above; the only genuinely subjective item is:

- **listing → "liste des fichiers" vs plain "liste"** (tentative for the short-label case): "liste des fichiers" is the
  well-sourced orthodox term for the pane's file list, but it may be too long for a tight label. Confirm whether "liste"
  alone reads best where space is tight in Cmdr's context.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/fr/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
