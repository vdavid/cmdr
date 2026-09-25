# hu review queue

Open questions for a future native Hungarian reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping). David doesn't break ties for Hungarian
(`style.md` § Open terms), so these wait for evidence or a native reviewer.

## Terms

- **pane → `panel`, viewer → `megjelenítő`, listing → `fájllista`, bookmark → `könyvjelző`**: no Tier-1 source (Finder
  is single-pane and has no viewer of its own); the two-pane pair settles `panel`, the rest stay tentative.
- **toast → `buborék`** (the internal/settings label) and **chip → `címke`** (`repozitóriumcímke`): no pile term.
- **command palette → `parancspaletta`**, **onboarding → `bevezető`** (`Bevezető…`, `bevezető varázsló`): descriptive,
  no Tier-1 term.
- **git dirty state → `piszkos állapot`**, **debounce → `pergésmentesítés`**, **hardlinked → `hardlinkelt`**: literal or
  loanword coinages in advanced settings.
- **tail → `Követés`**, **streaming → `streamelés`**: no pile term for the viewer's modes.
- **app switcher → `appváltó`**, **App windows → `Appablakok`**: descriptive; Apple names neither in Hungarian.
- **global shortcut → `globális parancs`**: descriptive.
- **Spaces** stays English in `shortcuts.system.spaces`: the Hungarian Mission Control name wasn't found in the pile or
  the live KeyboardSettings bundle (its `Spaces` key there is the whitespace sense, `Szóközök`).
- **compromised → `kompromittált`** (`servers.refusal.hostKeyRevoked`): no Apple source; `visszavont` is revoked.
- **start folder → `kezdőmappa`** (`servers.sheet.startFolder`): Total Commander says `Induló mappa`; no Tier-1 source.
- **relevance → `relevancia`** (`fileExplorer.columns.sortByRelevance`): Apple's catalogs say four different things.
- **typo → the verb `elgépel`**, **Linux distribution → `disztribúció`**, **Make available offline →
  `Elérhetővé tétel offline`**, **camera details → `kameraadatok`**, **side panel → `oldalpanel`**: no source.
- **parent folder → `szülőmappa`**: macOS says `tartalmazó mappa` (Go To Enclosing Folder). Switching would be one
  migration of the whole catalog, never piecemeal.
- **upgrade page → `Frissítési oldal`** (`commands.aboutOpenUpgrade.label`): `frissítés` is also the update word, and
  the page sells a license. No ruling yet.

## Wording

- **AI tool chips** (`askCmdr.tool.*`): the verbal-noun / `-va/-ve` pair is a novel construction; `inspectFile`'s
  `Fájlok átnézése` / `Fájlok átnézve` only hints at looking inside. `nothingToSuggest.done`
  (`Nem talált semmi említésre méltót`) is the family's one finite verb: a negative done state has no clean participle.
- **`main.quit.keepWorking` → `Munka folytatása`**: `Folytatás` alone is the queue's Resume, so it may read for a moment
  as resuming an operation.
- **`fileOperations.transferProgress.stallNotice` → `{duration} óta nincs előrehaladás`**: `óta` usually takes a point
  in time; `{duration} alatt nem történt előrehaladás` is the unambiguous but longer alternative.
- **`errorReporter.autoSentToast.viewOrAddNotes`** drops the word `jelentés` for width;
  **`errorReporter.amend.description`**'s second clause (`és odakerül a többi mellé, ami már a csapatnál van`) carries
  the English's casual image, and other renderings would do as well.
- **`fileOperations.delete.cloudOnlineOnly*`**: drafted without a human check.
- **`mtp.connectedToast.title`** says `Csatlakozva ehhez: {deviceName}` while network states say `Kapcsolódva`; a USB
  device may rightly keep `csatlakoz-`.
- **Corpus-less forms**: `félbemaradt`, `leválasztódott`, `megállt`, `sikerülhetett`, `átneveződött`, `emlékeztetőül`,
  `Hogyan?` (the ADB hint link), `Nem, köszönöm`, `Soha többé`, `még egy kör`, `odatette`: each is sound grammar with no
  pile attestation.
- **`Mac-eden` vs `Macen`**: both ship; new text uses the majority `Macen` / `Macet`.

## Length

- **`queue.failureToast.title`**'s longest arm, `Nem sikerült befejezni az áthelyezést a Kukába` (46 chars vs English's
  31), may wrap in a ~360 px toast; the fix would be `a Kukába helyezést`.
- **`fileExplorer.network.osMountFallback.retry` → `Próbálkozás közvetlen kapcsolattal`** (34 vs 24 chars) in a narrow
  bubble.
- **`main.escapeFullScreenHint.switchLabel` → `Kilépés a teljes képernyős módból az Escape billentyűvel`** (57 vs 26
  chars) in the one-time bubble.
