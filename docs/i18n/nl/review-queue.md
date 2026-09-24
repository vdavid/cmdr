# nl review queue

Open questions for a future native Dutch reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **`Bewerkingenwachtrij` vs `Bewerkingswachtrij`**: `-en-` pairs with the shipped `Bewerkingenlogboek`; Microsoft's
  `-ing` compounds would give `-s-`. Whichever wins, the queue and the log move together.
- **send → `versturen`** (macOS) over Microsoft's `verzenden`: confirm "Verstuur rapport" reads better than "Verzend
  rapport". Low stakes.
- **crash report → `crashrapport`**: macOS has only "Crashrapportage" (the feature). Confirm the compound for the
  report.
- **`bewaren` vs `opslaan`**: both occur (about 24 vs 31 values). Largely a real split (the button is `Bewaar`, the
  adjective is `opgeslagen`), but not everywhere; needs its own pass.
- **app bundle → `App-pakketten`**: `pakket` can also read as an installer (.pkg). Fallback `App-bundels`.
- **badge → `markering` / `statusmarkering`**: no Apple term for an icon-overlay status marker; confirm against the
  loanword `badge`.
- **review → `beoordelen`** for an approve/deny gate (`askCmdr.renameReview.title` "Naamwijzigingen beoordelen"), over
  macOS's look-over `bekijken`.
- **preview (verb) → `bekijken`** in `viewer.error.tooLargeToPreview`.
- **`gecompromitteerd`** (`servers.refusal.hostKeyRevoked`): no Tier-1 or Tier-2 source; `ingetrokken` says "no longer
  valid", not "proven unsafe". Confirm it isn't too formal in a small pane.
- **`inlogmethode`** (`servers.refusal.authMethodUnsupported`): unsourced; `aanmeldmethode` would need a sweep over the
  whole `inloggen` family.
- **`vingerafdruk` for an SSH fingerprint** (`servers.hostKey.fingerprintLabel`, `.firstContactBody`, `.changedBody`):
  Apple's two SSH strings keep `fingerprint`. Confirm a user comparing a `SHA256:` string recognizes it.
- **`Sleutelwachtzin`** (`servers.sheet.passphrase`) and **`Sleutelbestand`** (`servers.sheet.keyFile`): coined
  compounds. Fallbacks `Wachtzin voor de sleutel`, `Bestand met de sleutel`.
- **`servereigenaar`** (`servers.hostKey.firstContactBody`) and **`serversleutel`**
  (`settings.servers.card.trustedHostKeys`, `settings.summary.servers`): coined; confirm neither misreads ("owner of
  servers", "key TO the server").
- **`cameragegevens`** (`ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`): coined for a
  photo's EXIF block. Fallback `camera-informatie`.
- **`de inschrijfserver`** and **`mailinglijst`** (`onboarding.stepBeta.signup.unreachable`, `.rejected`): the first is
  coined, the second deliberately differs from Microsoft's `adressenlijst` (a different concept).
- **`de macOS-afhandeling`** for "native handler" (`onboarding.stepOptional.mtp.summary`): coined for length;
  `het eigen macOS-proces` is longer and `eigen` reads ambiguously.
- **"Places" → `Locaties`** (`shortcuts.scope.places`): Dutch can't carry EN's Places-vs-Locations split; confirm it
  doesn't clash with `Ga naar locatie`.
- **`lokale netwerkdetectie`** (`servers.hub.discoveryOff`): both halves sourced, the compound not. Looser alternative
  `Detectie in je lokale netwerk`.
- **"platform tools" kept English, "Android tooling" → `Android-tools`**: no pile source; confirm, and whether "de
  Android platform tools" wants a hyphen.
- **`Onboardingchecklist`** closed (19 letters) vs `Onboarding-checklist`.
- **`Ask Cmdr-model`**: hyphen after a two-word English brand; confirm it doesn't read awkwardly.
- **`Uit archief halen`** (`askCmdr.sessions.unarchive`): no single-word reverse of `Archiveer`.
- **pin in the Dock → `vastzetten` / `losmaken`** (`main.dockPinNudge.body`, `.unpinNote`): the catalog pair over
  Apple's Dock-menu labels `Permanent in Dock` / `Verwijder uit Dock`.

## Phrasing and tone

- **The crash-dialog openings** (`crashReporter.dialog.body.keptRunning`, `.unknown`): `een probleem tegengekomen` over
  the better-sourced but stiffer `heeft … aangetroffen`, and `is gewoon blijven werken` over
  `is gewoon actief gebleven`.
- **The Ask Cmdr tool-status pairs** (`askCmdr.tool.*`): coined as a set (present tense for doing, participle-led for
  done). Confirm the family reads coherent.
- **`askCmdr.composer.dropHint`** "Zet hier neer om bij te voegen": no pile source for a drop invitation.
- **`Al {duration} geen voortgang`** (`fileOperations.transferProgress.stallNotice`): alternatives
  `Geen voortgang in {duration}`, `{duration} geen voortgang`.
- **`De overdracht komt niet meer vooruit`** (`fileOperations.transferProgress.stallUnknown`): `ligt stil` is more
  idiomatic but reads as paused next to the real paused state.
- **`4x zo langzaam als`** (`fileExplorer.network.osMountFallback.message`) over the more colloquial `4x langzamer dan`.
- **`errors.eject.notEjectable`** sidesteps macOS's `Verwijderbaar` ("Deze schijf kun je niet uitwerpen, dus hij blijft
  aangesloten").
- **`Werk door`** (`main.quit.keepWorking`): coined; alternatives `Blijf werken`, `Ga door met werken`, the attested
  `Stop niet`.
- **`opruimen` for "clears away"** (`main.quit.body`): judgment; `verwijdert` was rejected on the delete collision.
- **`Antwoord nodig`** (`queue.row.statusAwaitingAnswer`): coined; alternative `Jouw antwoord nodig`.
- **`De archiefbewerking is niet gestart.`**: coined compound; alternative "Het bewerken van het archief is niet
  gestart."
- **`Verplaats het in plaats daarvan.`**: correct but heavy; `Gebruik daarvoor Verplaats.` may read better.
- **`naar het andere brengen`** (`errors.mutation.renameAcrossArchives`): avoids a second `verplaatsen`; confirm it
  isn't vague.
- **`Er staat niets meer op ‘{path}’.`**: the location form over Apple's shorter `'^0' bestaat niet meer.`
- **`macOS wilde dit niet naar de prullenmand verplaatsen.`** (`errors.mutation.trashRefused`) vs the harder `weigerde`.
- **`ongemoeid gelaten`** (`fileOperations.cancelRollback.reason.*`, `askCmdr.renameUndo.skipReason.*`): confirm the
  register in a toast, and that `Map {name} ongemoeid gelaten` (no article, tied byte-for-byte to its askCmdr twin)
  doesn't read clipped.
- **`nadat Cmdr er klaar mee was`** (`fileOperations.cancelRollback.reason.drift.*`): trades EN's place adverbial for
  gender neutrality; and whether `De rest staat nog op de nieuwe plek` is the best "where the move put them".
- **`Bekijk het rapport of voeg notities toe`** (`errorReporter.autoSentToast.viewOrAddNotes`): 39 chars against 31; the
  compact `Bekijk of vul het rapport aan` drops the notitie.
- **"Android file access over ADB" → `Toegang tot Android-bestanden via ADB`**: its sibling toggle opens with a
  different word; confirm the pair reads as one section.
- **`Cmdr verbindt niet meer met {name}`** (`servers.paneState.hostKeyChanged`): present tense where EN is past; confirm
  it reads as "blocked now", not "no longer supported".
- **`Voor het eerst verbinden met {host}`** (`servers.hostKey.firstContactTitle`): infinitive title over an imperative
  button; confirm the two registers together.
- **`Manier van verbinden`** (`servers.sheet.connectionModeLegend`, screen reader only): alternative
  `Hoe je verbinding maakt`.
- **`Voeg het in Sleutelhangertoegang toe`** (`servers.refusal.certificateUntrusted`): particle last per style; spoken
  Dutch prefers `Voeg het toe in Sleutelhangertoegang`.
- **`een sleutel`** and **`er valt niets te typen`** (`servers.paneState.signedOutNothingToAsk`): the user's key vs the
  server's key share one word, as in EN; `hoef je niets in te vullen` is warmer but shifts the subject.
- **`Cmdr let op telefoons.` / `Cmdr let nu niet op telefoons.`** (`settings.adb.status.watching`, `.notWatching`):
  alternatives `Cmdr houdt in de gaten of er een telefoon wordt aangesloten.`,
  `Cmdr ziet het meteen als je een telefoon aansluit.`
- **`Gevonden: {path}`** (`settings.adb.status.foundAt`) vs `Gevonden op {path}`.
- **`Vertrouwd op`** (`settings.servers.trustedHostKeys.approvedPrefix`): check against every date format.
- **`Je groep Netwerk wordt lang`** (`servers.pinHint.title`): alternatives `De groep Netwerk wordt lang`,
  `Je groep Netwerk begint lang te worden`.
- **`De verbinding tussen Cmdr en je telefoon is verbroken.`** (`adb.connect.transport`): passive, keeps the brand the
  don't-translate check requires.
- **`De Android-versie van deze telefoon is te oud; Cmdr kan er niet op bladeren.`** (`adb.connect.deviceTooOld`): the
  semicolon splits what EN says in one clause.
- **`Hoe?`** (`adb.hint.how`): alternatives `Hoe dan?`, `Uitleg`.
- **`Hint voor USB-foutopsporing gesloten`** vs its sibling's `getoond` (`settings.behavior.adbHintDismissed.label`):
  internal only.
- **`Er staan AI-suggesties klaar.`** (`suggestedOps.indicatorTooltip`) over `Er wachten AI-suggesties`.
- **`Klik om er een te kiezen in instellingen.`** (`askCmdr.wake.needsApiKey`): "set up" became "kiezen".
- **`dus het lijkt goed te bevallen`** (`main.dockPinNudge.body`): freer than "it seems to be working for you".
- **`staat nog open`** for a mounted disk image (`errors.eject.unmountRefusedByDiskImage`) vs `is nog gekoppeld`;
  `Werp eerst die schijfkopie uit en daarna deze schijf.` drops the second `uit`; `Wacht een minuutje` vs
  `Wacht een minuut` (`errors.eject.unmountRefusedBySystem`).
- **`waarvan het verplaatsen niet is voltooid`** (`fileOperations.leftovers.stagingFolderKept`): stiffer than EN.
- **Favorites menu** (`fileExplorer.navigation.favoritesAlreadyAdded`, `.favoritesCantAddHere`,
  `commands.favoritesOpen.description`): `Deze map staat al in je favorieten` (a where-form) vs the literal
  `Deze map is al een favoriet`; `gekoppelde netwerkshare` over Apple's `geactiveerd`; the added target in "druk op een
  cijfer om naar die favoriet te springen" and the `cijfer` / `nummer` split.
- **`Uit met schijfindexering`** (`settings.indexing.overriddenBadge`): `met` can read instrumentally; alternatives
  `Mee uit met schijfindexering`, `Volgt schijfindexering`.
- **`Schijf indexeren is supergaaf!`** (`onboarding.stepOptional.indexing.descIntro`): the one place the bare toggle
  label is a sentence subject, echoing the heading above it; the locale-wide rule would say
  `Het indexeren van schijven is supergaaf!`.
- **`Vraag het maar`** (`queryUi.mode.ai.label`), **`Bevalt het niet?`** / **`Ik vind het leuk`**
  (`fileExplorer.doubleClickHint.dontLikeIt`, `.iLikeIt`), **`wijzigingen verwerkt`** (`indexing.replay.detail`), and
  the playful `downloads.toast.learnIntro`: subjective tone calls.
- **`Stop en open opnieuw`** (`onboarding.stepFda.step3`): the macOS FDA relaunch button, unconfirmed against a live
  Dutch macOS.
- **The inspect-file consent text** (`ai.cloudConsent.askCmdr.contentsRule`): `wat tekst` / `wat regels tekst` vs the
  safer `een deel van de tekst`; `De fotozoekfunctie werkt net zo` vs `op dezelfde manier`.

## Layout (check against the pseudolocale)

- `queue.failureToast.title` trash arm (42 chars vs 31), `queue.failureToast.action` `Toon in de bewerkingenwachtrij`
  (fallback `Toon in de wachtrij`).
- `fileOperations.transferProgress.background` `Op de achtergrond` (17 vs 10; fallback `Achtergrond`, which costs the
  action reading and the exact aria containment).
- `main.quit.title` (55 vs 45; fallback `Stoppen terwijl er nog 3 bewerkingen lopen?`) and `main.quit.body`.
- `servers.hub.status.foundNearby` `Gevonden in de buurt` (20 vs 12), `commands.serversTogglePin.label` (30 vs 18),
  `settings.appearance.tintSmb.label`, `adb.readiness.offline`, `adb.hint.text`.
- `fileOperations.cancelRollback.reason.*` (75–90 chars vs 50–60), `operationLog.dialog.finishRollBack` and
  `fileOperations.rollbackConfirm.titleFinish`, `ai.cloudConsent.askCmdr.contentsRule`,
  `fileOperations.delete.cloudOnlineOnlyMixedWarning`, `fileExplorer.navigation.favoritesCantAddHere`,
  `fileOperations.leftovers.stagingFolderKept`.
- `settings.section.navigationAndFileOps` `Navigatie en bewerkingen`: fallback `Navigatie en bestandsbewerkingen` if the
  clip reads odd.

## Source (`en`) follow-ups

- `queue.empty.body` still names only copies, moves, and deletes (`Kopieer-, verplaats- en verwijderacties`), matching
  its English; if the English empty state widens to all operations, this key follows with `bewerkingen`.
- `onboarding.stepFda.ifAllow` "Three easy steps" trivializes, against `docs/style-guide.md`; the translation follows
  the source.
- `fileOperations.delete.cloudOnlineOnly*`: drafted, not yet read by a human.
