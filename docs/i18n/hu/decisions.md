# hu decisions

Distilled rulings behind `terms.json`: "X over Y because Z", one entry per topic, edited in place. `pnpm i18n:brief`
pulls a section when its heading cites a batch key, so every heading cites its keys in backticks. Rulings live in
`terms.json`, voice in `style.md`, open questions in `review-queue.md`.

## A macOS-vs-Windows hasadások: Kuka, szerver, Mégsem, könyvjelző

Where macOS and Microsoft split, macOS wins, because Finder users see its word: `Kuka` over `Lomtár` (MS reserves it for
the Windows Recycle Bin), `szerver` over `kiszolgáló`, `Mégsem` over `Mégse`. Bookmark → `könyvjelző` (tentative); the
favorites list stays `kedvenc`.

## A Beállítások szakaszai (`settings.section.*`)

- Quote a section name verbatim wherever another string names it (`settings.section.*` is the source).
- A Settings path in a sentence never takes a suffix: `itt: Beállítások > AI`; the separator glyph mirrors English per
  key.
- Option values: Wilting = `Hervadás`, Smart = `Okos`, Personal = `Személyes (ingyenes)`.

## A kötetválasztó csoportcímei (`fileExplorer.navigation.group*`)

The Network group stays `Hálózat` and its Servers row `Szerverek`; don't collapse the two.

## Gép vagy gazdagép: a host két regisztere (`fileExplorer.network.*`, `commands.networkSelectHost.*`, `errors.listing.host*`)

The network browser says `gép` (`Gépnév`, `Hálózati gép`), connection-failure prose `gazdagép`; the split is deliberate.
`kiszolgáló` in `errors.json` is the participle "serving", not the noun server.

## Git-szavak (`errors.git.*`, `fileExplorer.git.*`, `settings.fileExplorer.git.*`)

`git`, `worktree`, `commit`, `blob` stay verbatim, repo → `repó`. `munkafa` is only the generic working tree
(`errors.git.bareRepo`, `blobTooLarge`, `gitDirPermissionDenied`); every linked checkout is `worktree`
(`worktree-eket`). Bare repo `csupasz repó`, dirty `piszkos állapot` (tentative).

## Relatív időcímkék (`fileExplorer.*.ago*`)

Terse chips: `{count} p/ó/n/h/hó/é ezelőtt`, just now `most`; ETA `mp` / `p`, roughly `kb.`.

## A kereszt-fájlos egyeztetés

Catalog-wide: `Modified` → `Módosítva`; `Don't show again` → `Ne jelenjen meg többé`; `Example:` → `Példa:` (never
`Például:`, that's "for example"); `Something went wrong` → `Valami nem sikerült`. A failed update check is whole
sentences, no `Hiba:` / `Probléma:` prefix (`updates.failure.check`).

## A státuszcellák és a buborékok hangja (`queue.row.status`, `operationLog.status.*`, `fileExplorer.status.*`)

- Row states: `Várakozik`, `Fut`, `Szüneteltetve`, `Kész`, `Megszakítva`, failed `Nem sikerült befejezni`; the log's
  Didn't finish `Nem fejeződött be`.
- Fallback cells say `Probléma`, never `Hiba` (the `@key` asks for the friendlier word).
- The Queue button that backgrounds a transfer = `Sorba` (Double Commander's `Várakozási sorba helyez`, shortened).

## Az átviteli ICU-többesszámok (`transfer.*`)

`{verb, select, …}` opens with `Másolás` / `Áthelyezés` and uses `másolva` / `áthelyezve` inline; the `{phrase}`
fragment sits after a colon (`Másolva: {phrase}.`) so it stands alone. `transfer.fileOnly.mixedMove`'s was/were
collapses to `volt`.

## A dupla kattintás a panel hátterén (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

`szülőmappa` over Finder's `tartalmazó mappa` because the catalog uses it everywhere (`commands.navParent.label`, six
`errors.json` suggestions); switching is one whole-catalog migration, never piecemeal. Never do this again =
`Soha többé` (tentative).

## A FAT32-korlát üzenetei (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

larger than {maxSize} → `{maxSize} méretnél nagyobb`: the base noun takes the suffix, since a unit's spoken vowel isn't
safe (`KB` reads kábé or kilobájt). `%` always reads `százalék`, so `{size}%-ra` stays.

## A célmappa még nem létezik (`fileOperations.transferDialog.targetWillBeCreatedCopy`/`…Move`)

`Ez a mappa még nem létezik.` (TC and DC say `nem létezik`), then one literal sentence per key, no ICU select.

## Archívumok böngészése (`settings.archives.*`, `errors.listing.archive*`, `fileOperations.delete.archiveWarning*`)

Cells nominal (`Böngészés` / `Megnyitás` / `Rákérdezés`). `Konfigurálás…` over `Beállítás…` so the menu item doesn't
collide with `Beállítások`.

## A vágólap beillesztése fájlként (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

The toast's branch words carry the possessive (`{kind, select, image {képe} pdf {PDF-je} other {szövege}}`) and the
filename follows a colon.

## A Műveletnapló (`operationLog.*`, `commands.logOperationLog.*`)

- `Műveletnapló` names the dialog and its Settings card alike. Roll back → `visszagörgetés` (the transfer dialog's
  engine word); `settings.operationLog.intro` says `visszavonhatod` because its English says undo.
- Provenance: You `Te`, Agent `Ágens`, AI client `AI-kliens`.

## Ask Cmdr: helyőrzők, címek, eszközsorok (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`)

- Input placeholders are nominal (`Kérdés a fájljaidról…`); a standalone welcome is a `te` imperative
  (`Beszélgess a Cmdrrel a fájljaidról`).
- Tool chips pair a verbal noun (doing) with a `-va/-ve` participle (done) on one object. `searchPhotos` =
  `A fotóid átvizsgálása`, because `X keresése` reads as searching FOR the photos.
- `{provider}` arrives localized from `settings.ai.provider.opt.*`.

## Hálózati meghajtók képindexelése (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`/`.paused`)

- Mirror English: section labels `kép`, per-drive user strings `fotó`.
- always index → `Ez a meghajtó mindig legyen indexelve`: `mindig` + a bare verbal noun is ungrammatical.
- `{name}` is a bare possessor, never suffixed: `{name} fotóinak indexelése`.

## Tömeges átnevezés, képindex-hatókör, Ask Cmdr-eszközök (`askCmdr.renameReview.*`, `settings.mediaIndex.excludedFolders.*`, `fileExplorer.imageIndex.*`)

excluded → `kizár` / `kizárva` over `kihagy`, because `kihagyva` is the transfer's skipped outcome. Next pass →
`a következő átvizsgálás`.

## A képindex jelvényei (`fileExplorer.imageIndex.file.*`/`.folder.*`/`.drive.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- `file.failed` → `Nem sikerült indexelni`, never Finder's `Hiba`. `file.excluded` → `Nem szerepel a képkeresésben`,
  over `kizárva`, which would claim a user's choice.
- Count-of-count uses the slash: `{doneText} / {totalText} kép indexelve`. "All N images" is
  `Az összes kép ({totalText})`, so no article touches the number.

## A képindexelés beállításainak átrendezése (`settings.mediaIndex.*`, `fileExplorer.imageIndex.file.indexing`)

By description → `leírás alapján`. The delete-model body dodges suffixing `{size}` with `Ezzel felszabadul {size}`.

## A törlés kapcsolója és a Forrás / Cél fejlécek (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

From / To headings → `Forrás` / `Cél` (TC `662`/`663`, DC; matches `Célkötet`), over `Innen` / `Ide`, which dangle as
headings. `Innen:` stays the inline label before a scan-phase source path.

## A meghajtó indexelésének kikapcsolása (`fileExplorer.navigation.driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

- The feature is `(a) meghajtó indexelése`, never `meghajtó-indexelés`; the bare `indexelés` is the per-drive short
  form.
- picks up where it left off → `ott folytatja majd, ahol abbahagyta` (`ott … ahol` is the correlative pair).
- `overriddenBadge` → `Az indexeléssel együtt ki`: `együtt` forces the comitative reading in 25 characters.

## Meghajtóindex: a változásellenőrző futás (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

`Változások ellenőrzése` and `Fájllista frissítése` match the sibling run headers; a full check is `átvizsgálás`.

## A megtorpant átvitel értesítése (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- No progress for {duration} → `{duration} óta nincs előrehaladás`: `{duration}` is unlocalized (`45s`), so it only
  stands before a postposition. `előrehaladás` is the catalog's progress noun.
- Waiting for X → `Várakozás a cél/forrás válaszára`, naming the sides with the dialog's own `Cél` / `Forrás`; over
  `nem reagál`, which reads as a fault.
- The transfer has stopped moving → `Az átvitel megállt` (tentative), over `leállt` (reads as ended).
- partly written → `lehet, hogy már részben ki van írva`, over the bureaucratic `kiírásra került`.
- `transferProgress.close` = `Bezárás` beside `Mégsem`: one closes the window, the other stops the operation.

## Másolt útvonal: a vágólap-visszajelzés (`fileExplorer.clipboard.copiedPath`)

The path renders on its own line below, so the sentence ends in a colon and stands alone:
`Útvonal másolva, most már a vágólapon van:`.

## A műveleti sor: a Transfer queue átnevezése (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- Operation queue → `Műveleti sor`, beside `Műveletnapló`: one head noun `művelet` for the View-menu pair.
- Never the compound `Műveletsor`: Microsoft assigns it to task flow / restore sequence (a sequence of steps). Not
  `várakozási sor` either: long for a window title.
- `műveleti` starts with a consonant, so `a műveleti sorba` / `sorban`, never `az`.

## A sarokchip és a befejezetlen műveletek értesítése (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- dismiss → `Elvetés` (button), `Mindet elveti` (toolbar, finite like `Mindet szünetelteti`),
  `Ennek a műveletnek az elvetése` (aria). Over the pile's `Bezárás`, which is the catalog's Close and would make
  Dismiss look like closing a window.
- couldn't finish X → `Nem sikerült befejezni` + the action in the accusative, verb first in all nine `select` arms, so
  the toast and the row word one event the same way.
- Screen-reader percent → `{percentText} százalék`, unsuffixed; the visual tooltip keeps `42%`.

### `queue.chip.tooltip`: the dot-separated fact line

`{label}` arrives as a verbal noun (`Másolás`), so `Másolás 214 elem` can't continue it: every optional clause carries
its own `·` inside its branch, and the line is a flat fact list (like `ai.toast.progress`). Destination is
` · ide: {destination}` (macOS `Áthelyezés ide: %@`), never a suffix on the folder name. `{detail}` arrives already in
Hungarian.

## Az önálló ütközési kérdés (`fileOperations.operationConflict.*`)

- `context`: copy/move take `Másolás ide: {destination}`, the Working arm `Folyamatban itt: {destination}`, tracking
  English's to/in split. Editing an archive → `{destination} szerkesztése`: the possessor slot needs no suffix and no
  article.
- `pausedNote` → `Minden más szüneteltetve van, amíg nem válaszolsz.`, over `szünetel`, so the prompt and the row share
  the word `szüneteltetve`.

## A progress-párbeszéd gombja üres sornál (`fileOperations.transferProgress.background`, `.backgroundAria`)

- Background → `Háttérben` (Total Commander `4004`, right next to `Sorba állít`). Not `Háttérbe`: `háttérbe helyez`
  means "sideline", the opposite promise. Not `Háttér`: that's the backdrop.
- The aria reuses `Hagyd futni a háttérben` from `queueTooltip`, and `Háttérben` ⊂ it for WCAG 2.5.3.

## A kilépési kapu (`main.quit.*`)

- Quit now → `Kilépés most` (the `<Noun> most` shape is macOS), over `Kilépés mindenképp`, which answers a refusal.
- Running in prose → `folyamatban van` (macOS's own quit-blocked sentence); the heading `Még folyamatban` is verbless
  because `Még fut` / `Még futnak` break at one or many rows. The `Fut` status cell stays: two registers, like macOS.
- Keep working → `Munka folytatása` (tentative), over `Mégsem`, which next to running operations reads as "cancel them",
  and over `Később`, since the countdown is deleted, not deferred.
- in {n} seconds → `{secondsText} másodperc múlva`: the postposition keeps the placeholder bare.
- clears away → `eltávolít` over `törli`: the sentence reassures, so it must not flash "deletes a file".

## Usage stats: "névtelen" dropped, "egy véletlenszerű azonosító" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

usage stats → `használati statisztika`, a random id → `egy véletlenszerű azonosító`. ❌ Never `álnevesített` /
`pszeudonim`: the English stays everyday on purpose. `Mac` + a possessive suffix takes a hyphen: `Mac-eden`.

## Visszagörgetés-megerősítő + a válaszra váró sor állapota (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

- Needs your answer → `Válaszolnod kell` (tentative), over `Válaszra vár`, which blurs with `Várakozik`, and
  `Választ kér` (`választ` is also the verb "choose").
- Keep them → `Fájlok megtartása`: bare `Megtartás` could point at the overwritten files the body just named.
- `rollbackTooltip` → `Leállítás, és minden eddig kiírt fájl törlése`, over `Megszakítás`, which the `@key` says must
  not read as a plain Cancel.
- `foregroundBusyToast` says `Itt valami más van nyitva`: the blocker may not be an operation.

## Az átnevezés-láncban nevüket megtartó fájlok számláló buboréka (`fileExplorer.rename.chainKeptOriginalName`/`AndOthers`)

`{reason}` stands last after a colon (it may end in its own punctuation): `„{name}” megtartotta a nevét: {reason}`, and
`{othersText} másik fájl megtartotta a nevét, és „{name}” is: {reason}`. The gapped `is` carries "and so did" and puts
the named file right before its colon, since `{reason}` covers only that file. `one` spells out `Egy másik fájl`.

## A meg nem erősített átnevezés buboréka és a fel nem használható név (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

- Couldn't confirm → `Nem sikerült megerősíteni, hogy „{name}” átneveződött`, the family's opener
  (`trashUnconfirmedToast`, `mkdir.timeoutMessage`). The object form `… átnevezését megerősíteni` reads as "approve".
- The mediopassive `átneveződött` over `átnevezték`, which implies someone outside Cmdr.
- `az átnevezés attól még sikerülhetett` names its subject: dropped, it would read as the volume. The subject is the
  rename, never `a fájl`, since a folder can stand there too.
- That filename can't be used → `A fájlnév nem használható` / `A mappa neve nem használható` (macOS), no final period:
  it's embedded in a longer sentence.

## Javasolt műveletek: az Ask Cmdr javaslatainak ablaka (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

approve → `Jóváhagyás` over macOS's `Elfogadás` (the AirDrop accept); reject → `Elutasítás`.

## Megkettőzés: a parancs, amely ugyanabba a mappába másol (`commands.fileDuplicate.*`)

duplicate → `Megkettőzés` (Finder `Fájl > Megkettőzés`); the description
`Másolat készítése a kijelölt fájlokról ugyanabban a mappában`.

## Natív menük: menüsor, helyi menük, ablakcímek (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Finder's wording wins over the catalog's, since the menu bar sits beside Finder's: `Nézet`, `Ugrás`, Home `Saját`,
Deselect all `Kijelölés törlése`, Zoom `Méretezés`. Mined from `Finder.app/.../hu.lproj` (macOS 26.5.2).

- zoom in / out → `Felnagyítás` / `Lekicsinyítés` (Safari), so `Nagyítás` stays free for the submenu title.
- changelog → `Módosítási napló`, apart from Help > `Újdonságok`; word wrap → `Sortörés`.
- Edit in editor → `Megnyitás szerkesztésre` (tentative): the literal form repeats one stem.
- Image-index toggles → `Képek indexelésének tiltása itt` / `… engedélyezése itt`: labels stay nominal, no `te`.
- Tag row → `„{color}” hozzáadása` / `eltávolítása` (Finder `TG5`/`TG6`); the color name stays unsuffixed.
- `Kilépés a cmdrből` keeps English's lowercase `cmdr`. `menu.zoom.percent*` and `menu.view.askCmdr` are deliberately
  identical to English.

## busy: a kiszürkített menüpontok jelölője (`menu.volume.ejectBusy`, `menu.volume.disconnectBusy`, `menu.volume.forgetServerBusy`, `menu.volume.forgetSavedPasswordBusy`)

The busy item is the base label unchanged + ` (foglalt)`. One marker for every busy key; don't invent another.

## Rendszerkapcsolatra visszaeső SMB-buborék (`fileExplorer.network.osMountFallback.*`)

- native → `natív` (MS); `beépített` is built-in. Elsewhere the same connection is `rendszerkapcsolat`, because there
  English says "system connection".
- Try connecting directly → `Próbálkozás közvetlen kapcsolattal` (tentative): keeps English's "try", since the direct
  connection already failed once.
- `transferDialog.smbNativeNote` quotes the menu item's full name, `„Közvetlen kapcsolat a gyorsabb hozzáférésért”`,
  because that's what the user finds in the volume picker.

## Átnevezés/létrehozás elutasításai: a `errors.mutation.*` és `errors.volume.*` egysoros üzenetek

- `{path}` goes in a colon slot (`itt: „{path}”`) or leads bare and quoted (`„{path}” mappa, nem fájl.`), never
  suffixed.
- locked → `zárolva van`, unlock → `Oldd fel a zárolását` (Finder `PE13`); SIP → `Rendszerintegritás-védelem`, phrased
  `… védelme alatt áll` so the `véd-` stem doesn't repeat.
- didn't answer in time → `nem válaszolt időben`, over `nem reagál` (the fault register).
- That password didn't work → `Ez a jelszó nem jó.`, over macOS's formal `helytelen` and `hibás` (the `hiba` stem).
- The destination can't hold that name → `Ez a név nem használható a célhelyen.` (Finder's `név nem használható`).
- `errors.mutation.timedOut` doesn't call it a failure: `… így a módosítás attól még végbemehet.`
- `errors.volume.deviceSessionReset` never says unplugged; `deviceDisconnected` (dropped on its own) →
  `Megszakadt a kapcsolat az eszközzel`, over the user-initiated `leválasztás`.
- macOS wouldn't move this to the Trash → `A macOS nem engedte ezt a Kukába helyezni.`: a refusal, so not
  `nem sikerült`.

## Ha a Cmdr nem állt le: az összeomlásjelentő két új nyitómondata (`crashReporter.dialog.body.keptRunning`/`.unknown`)

- ran into a problem → `problémába ütközött` (Finder `NE105`'s `-ba ütközik`; Apple's crash dialogs say `probléma`). ❌
  Not `gondba ütközött`: `gond` has zero hits in the whole pile. Not Apple's `egy probléma miatt`: it demands a verb
  that says the app stopped.
- kept running → `és tovább futott` (the catalog's `Tovább fut a háttérben`).
- These keys must never say it stopped: no `váratlanul kilépett`, `bezárult`, `összeomlott`. `.unknown` doesn't name the
  background either, since an old report doesn't say whether the app kept running.
- The title and toast split the same way: `összeomlási` only where there was a crash.

## A jelentésküldés beállításszövege már mindkét kimenetelre igaz (`settings.updates.crashReports.description`)

The description covers both outcomes with the dialog's verbs (`váratlanul bezárul`, `a háttérben problémába ütközik`)
and a bare `egy jelentést`. The label stays `Összeomlás-jelentések küldése`: it's the setting's name.

## Kiadás és leválasztás: a `errors.eject.*` buboréküzenetek

- These follow the wrapper `Nem sikerült kiadni: {volumeName}: {message}`, so each is a sentence after a colon.
- in use → `használatban van` (Finder `NE66`); removable → `cserélhető` (macOS and MS agree); network share →
  `hálózati megosztás`, over macOS's `szerverkötet`, which names the mounted volume.
- `errors.eject.timedOut` isn't a failure: `… de a kiadás magától is befejeződhet.`
- `errors.eject.unexpected` deliberately differs from the identical-English `errors.mutation.unexpected`:
  `A Cmdr problémába ütközött, és nem tudta megállapítani, hogy mi.`, because after `Nem sikerült kiadni:` the settled
  `Valami nem sikerült` would repeat itself.

## A macOS-panelnevek magyarul: `Infó megjelenítése` és `Zárolt` (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.noPermissionErrno.suggestion`, `errors.listing.permissionDenied.suggestion`)

Apple translates both, so they're Hungarian: Get Info → `Infó megjelenítése`, Locked → `„Zárolt”` (quoted in prose, as
Apple does), Sharing & Permissions → `Megosztás és jogok` (the running-text form). The labels are Apple's, the sentence
ours: `vedd ki a „Zárolt” pipát`, never Apple's `szüntesse meg a … kijelöltségét`. A menu item is a `parancs`, never a
`menü`.

## A Kuka-értesítés két gombja és a visszavonás családja (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- put back (from the Trash) → `visszahelyezés` (Finder `PE130`), over `visszaállítás`, which is the old-name undo.
- Go to trash → `Ugrás a Kukába` (Finder) on both the button and the palette.
- This drive doesn't keep a trash → `Ezen a meghajtón nincs Kuka.`: a fact, not the `nem támogatja` error register.

## A már elküldött jelentés kiegészítése (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- The title carries the full noun (`Hozzáadás a hibajelentésedhez`), the button the short one
  (`Hozzáadás a jelentéshez`), like `dialog.title` / `dialog.send`.
- note → `megjegyzés`, over `jegyzet` (Microsoft reserves it for Notes items). Add a note → `megjegyzést hozzáadni`,
  over `megjegyzést fűzni`: the pile knows `fűz` only as append.
- What was sent → `Mi került elküldésre`, the past of the sibling `Mi kerül elküldésre`: the English differs only in
  tense, so the pair keeps one shape.

## Kijelölés és a kijelölés törlése: a Select / Deselect párbeszéd (`selection.*`)

- deselect → `kijelölés törlése` (Finder `300488`), over the orthodox pair's `megszüntetése`. ❌ Don't bring
  `megszüntetése` back.
- Dialog titles `Fájlok kijelölése` / `Fájlok kijelölésének törlése` match the menu items that open them.
- The tooltip needn't contain the label: the button's name comes from its label key, so WCAG 2.5.3 holds by
  construction.
- `selection.recent.*` twins `queryUi.recent.*`; the raw `{query}` sits at the end after a colon.

## Belső driftszedés: egy fogalom, egy név

`desktop-i18n-term-consistency` holds one Hungarian form per English value. Settled: Quit Cmdr → `Kilépés a Cmdrből`;
Connect to server → `Kapcsolódás szerverre` (Finder `N84`, over `szerverhez`); Connected → `Kapcsolódva`, Connecting →
`Csatlakozás…` (Apple's `SavePanel`); Retrying → `Újrapróbálás`; case-sensitive → `Kis- és nagybetűérzékeny`.

### A határvonalak, amiket NEM szabad elsimítani

- **Cancel**: `Mégsem` dismisses a dialog, `Megszakítás` stops a running operation, `Leállítás` stops a service.
- **View**: `Nézet` is the menu title, `Megtekintés` the verb that opens the viewer.
- **Zoom**: `Nagyítás` is text zoom, `Méretezés` the window action.
- **Select**: `Kijelölés` marks files, `Válassz` is a dropdown placeholder.
- **Bytes**: `Bájtok` beside `Fájlok`, `Bájt` beside `kB` / `MB`.
- **Purple**: `Bíbor` is Finder's tag color, `Lila` Cmdr's volume tint.
- **Put back**: `visszaállítva` an old name, `visszahelyezve` from the Trash; English's one phrase is its own blur.
- **Rolling back**: `Visszagörgetés…` titles the window, `Visszagörgetés folyamatban` is the log cell.
- **Send report**: the title asks (`Elküldöd a jelentést?`), the button names (`Jelentés küldése`).
- **Scan**: `átnézés` is the live folder walk, `átvizsgálás` the index and size scan, `keresés` the Search feature.
- **memory**: `memória` is RAM, `jegyzet` Ask Cmdr's memory.

## Az angol önellentmondásainak magyar utóélete

### A régi NÉV visszaadása mondat megnevezi a tárgyát (`askCmdr.renameUndo.undone`/`.partial`)

`{countText} fájl régi neve visszaállítva.`: the possessive names the object, which English now does too; the Trash's
`fileOperations.trash.undone` keeps `visszahelyezve`.

### `mappa` az indexelés súgószövegében (`settings.indexing.enabled.description`)

`azonnali mappaméretekért`: the UI word is `mappa`; `könyvtár` is only technical.

### A macOS-panelnevek magyarul, a futásidejű tokenek mellett

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` stay verbatim and never take a suffix or article
  (`{system_settings}ben` would clash with `Rendszerbeállítások`): use `itt: {system_settings}`.
- Panels no token covers are Apple's Hungarian: Apple Account → `Apple-fiók`, General → `Általános`, Login Items &
  Extensions → `Indítóelemek és bővítmények` (macOS 26.6.2, the `.loctable` files, 2026-08-30).

### A natív menüsor két Apple-tétele (`menu.app.showAll`/`.hideOthers`, `commands.appShowAll.label`, `commands.appHideOthers.label`)

`Összes megjelenítése` / `Többi elrejtése` (Finder `MenuBar.strings` `300730` / `300729`).

## Egy félbehagyott visszagörgetés befejezése (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- Finish rolling back → `Visszagörgetés befejezése` (Finder's `Másolás befejezése`); both `finishRollBack` keys stay
  identical.
- another pass → `még egy kör` (tentative, no source): everyday, over `átfutás` / `menet`.
- in {folder} → `itt: {folder}`: the row reads `A létrehozottak törlése itt: Backup`, so the deletion happens inside the
  folder and no suffix or article touches the name.

## A megszakított visszagörgetés eredményértesítése (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Left X alone → `<alany> változatlan maradt: <indok>.`, the `askCmdr.renameUndo.skipReason.*` frame, in every reason
  row: the `folderNotEmpty` pairs share their English, and one notice mustn't mix frames. The intro keeps `kihagyja`. ❌
  Not `békén hagyja` / `érintetlenül hagyja`: zero pile hits.
- Result rows say `visszahelyez`, progress rows `visszavitel`: `visz` is the motion, `helyez` the end state, and
  `visszavíve` doesn't read.
- Removed → `eltávolítva`, never `törölve`: the notice reassures.
- Full vs partial lives in the sentence: `A Cmdr mindent eltávolított, amit létrehozott: {countText} elem.` vs
  `{countText} elem eltávolítva.` The full notice's `one` branch drops the count: `A Cmdr eltávolította az elemet, …`.
- it changed → `módosult` (macOS) over `megváltozott`; check → `ellenőriz`, apart from the `megerősít` (confirm) family.
- Couldn't undo {name} → `Nem sikerült visszagörgetni: „{name}”.`: the per-item outcome word is `visszagörgetés`, and
  `visszavonás` is for an operation.
- `counted` rows agree singular with the numeral subject, and the clause after the colon may go plural.
- put it there → `odatette` (tentative): true for copy and move alike, where `odamásolta` isn't.
- A named item leads bare and quoted (`„{name}” változatlan maradt`), never with an article, which follows the name's
  unknown first sound. Where English adds a noun (the folder {name}), the reason carries it:
  `„{name}” változatlan maradt: a mappában már van valami.`
- `askCmdr.renameUndo.undoJob` → `Az összes {csomag} visszavonása ({countText})`: the article agrees with `összes`, a
  word we pick, and the number sits in parentheses (macOS `Az összes lemez (^0)`).
- `rollbackConfirm.body`'s new third sentence copies `bodyUndoByDeleting`'s tail verbatim.

### `cancelRollback.stagedLeftover.*` (a Cmdr saját maradéka a célhelyen)

unfinished copy → `hiányos másolat` (macOS `LA33`). ⚠️ `egy későbbi átvitel során`, ❌ never `legközelebb`: the cleanup
skips anything younger than an hour, so an immediate retry clears nothing.

## A túl régi WebKit blokkoló képernyője (`main.oldWebkit.*`)

Software Update → `Szoftverfrissítés`; `Safari 15.4-es vagy újabb verzió` (a digit takes a hyphenated suffix).

## A régi macOS értesítése (`main.oldMacos.*`)

X and up → `X és az annál újabb`; look off → `félremehet`, something broken → `valami elromlottnak tűnik` (both keep out
of the `hiba` register). The last sentence is David speaking, `te`.

## Belenézés a fájlokba: az `inspect_file` eszköz és a hozzájárulási képernyő új ígérete (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- thumbnail → `bélyegkép` (Finder) over MS's `miniatűr`; camera details → `kameraadatok`, not `EXIF` (lay reader).
- where a photo was taken → `hol készült` in a sentence, `készítési helye` in a list (tentative).
- A PDF page is `oldal`, never `lap` (that's tab): `PDF-oldalak`.
- what's inside an archive → `az archívumban lévő fájlok listája`, over `az archívum tartalma`, which would clash with
  the screen's promise that whole files never leave.
- look inside → `belenéz` in prose; the chip pair is `Fájlok átnézése` / `Fájlok átnézve` (tentative), because `belenéz`
  has no stative `-va/-ve` form and `átvizsgálás` belongs to `searchPhotos`.
- limited part → `egy korlátozott részét`, over `egy kis részét`: English promises a limit, not a size.

## A Rollback gomb két buboréksúgója (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

`Leállítás, és minden eddig áthelyezett fájl visszahelyezése`, in the sibling `rollbackTooltip`'s frame. ❌ Not
`törlés`: rolling a move back deletes nothing.

## A „Terminál megnyitása itt” és az appválasztója (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

The app kind is `terminál` (MS), Apple's app stays `Terminal` (`a Terminalban`, Finder `N67`). Open terminal here →
`Terminál megnyitása itt`; Choose an app… → `App kiválasztása…` (the catalog says `app` throughout).

## `Sort by relevance`: a találati oszlop elemleírása (`fileExplorer.columns.sortByRelevance`)

relevance → `relevancia` (tentative, Apple's WorkflowKit sort order), over `Fontosság` / `Találati pontosság`, which
name neighboring ideas.

## `Documents and packages`: az új OOXML-sor (`settings.archives.ooxml.*`)

`Dokumentumok és csomagok`: bare `csomagok` stays wider than the `Alkalmazáscsomagok` card below, as English splits
packages from app bundles. The description keeps its siblings' frame, `Mit tesz az Enter egy … fájlon.`

## A szerverközpont: kapcsolódási állapotok, elutasítások, elfelejtés (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`.disconnect*`/`.forget*`)

- Connecting to {name}… → `Kapcsolódás ide: {name}…` (Finder `MN1`, verbatim). Quote only where English quotes.
- The server pane's buttons are dialog buttons: `Mégsem`, not the running-op `Megszakítás`.
- host key → `{host} kulcsa`: the possessive lands on `kulcs`. Trust as a statement → `megbízhatónak tekint`; ❌ never
  `elfogad`, which hides the trust decision.
- compromised → `kompromittált` (tentative), over `visszavont`, which is revoked and reads administrative.
- Keychain Access → `Kulcskarika-elérés` (the live bundle's name beats the support site's `Kulcskarika-hozzáférés`); the
  store is `kulcskarika`.
- Open X to Y → a `-hoz/-hez` purpose noun (`a kapcsolódáshoz`), never a `hogy` clause.
- The forget questions name the type and move the name behind a colon: `Elfelejted ezt a szervert: „{name}”?`,
  `Elfelejted a mentett jelszót ehhez: „{name}”?`.
- `{name} leválasztása` is the aria, like `{name} kiadása`.

## A szerverközpont táblázata és a kötetváltó rögzítése (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

- Places → `Helyek`, over the old `Megosztásböngésző`, which can't hold storage buckets.
- Address column → `Cím` (the table is about servers already).
- Last used → `Utolsó használat` (macOS `Security.prefPane`, the same column role), over the file list's
  `Utoljára használva` pattern: a literal Tier-1 match beats a family pattern with no source.
- Found nearby → `A közelben felfedezve` (IOBluetoothUI).
- Pin / unpin → `Szerver rögzítése / rögzítés feloldása` (Notes), ❌ not `megszüntetése`.
- volume switcher → `kötetválasztó`: English has two names for one surface, Hungarian one.
- The pin toasts put `{name}` bare in subject position: `{name} mostantól ott van a kötetválasztódban.`
- `Mac` takes `Macet` (161 macOS hits); `NAS-t`. Empty state `Még nincs szerver`.

## A szerverlap, az SSH-kulcs jóváhagyása és a Go-to-path előnézete (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

- Trust (the button) → `Beállítás megbízhatóként` (UsersGroups); statements keep `megbízhatónak tekint`.
- man in the middle → `valami beékelődött közéd és a szerver közé`: Apple's stem, our sentence, no attack named.
- ⚠️ Deliberate split: the pane line is `Kapcsolódás ide: {name}…`, but the sheet's Connect button and everything on the
  sheet use `csatlakoz-`, because `Connect` = `Csatlakozás` is forced by `fileExplorer.network.connect` and the button
  rewrites itself to `Csatlakozás…`. Don't sweep them together.
- Remember in Keychain → `Megjegyzés a kulcskarikában`, with no `jelszó` object: SFTP may save a key passphrase.
- Key passphrase → `Kulcsjelmondat` (`jelmondat` sets it apart from `Jelszó`).
- `Kijelentkezve innen: {name}` over `Kijelentkeztél…`: the panel's three lines stay one nominal family.
- I've checked it → `Ellenőriztem`: the user's own words, first person.
- `ssh`, `ssh-agent` take `az` (esz-esz-há): `Az ssh-agent használata`; `Nextcloud-cím` is hyphenated.

## Az automatikus újracsatlakozás és a kulcsos bejelentkezés panelsora (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- reconnect → `újracsatlakozás`, over macOS's `újrakapcsolódás`: the shipped family uses it, and two siblings on the
  same screen already do. A stem switch within one view is worse than across views.
- `Újracsatlakozás ide: {name}…`: the frame comes from the pane lines, the stem from the family.
- `Ez a szerver jelszó helyett kulcsot használ a bejelentkezéshez…`: the server stays the subject, like its siblings.
- Open it again to retry → `Nyisd meg újra a kapcsolódáshoz.`: `újra` already carries the retry.

## A rögzítési tipp, a megbízható szerverkulcsok lapja és az ADB-állapotsor (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.behavior.serversPinHintSeen.*`, `settings.appearance.tintSmb.*`)

- Unpin → `Rögzítés feloldása` (Notes), ❌ not `Rögzítés megszüntetése`. Pin to switcher → `Rögzítés a kötetválasztóban`
  (WorkflowUI's `Rögzítés a menüsoron` shape).
- The hint title is possessive, `Kezd hosszúra nyúlni a Hálózat csoportod`: neutral, not a warning.
- The hint body names the command after a colon: `futtasd a parancspalettából ezt a parancsot: „{command}”`.
- host key as a head noun → `szerverkulcs` (`Megbízható szerverkulcsok`), ❌ not `hosztkulcs`: the catalog says
  `szerver` to users.
- The date label Trusted → `Megbízhatóként jelölve` (SecurityInterface): `megbízhatónak tekint` has no `-va/-ve` form
  that works before a date. ❌ Not `Jóváhagyva`: it loses the trust decision.
- Re-check → `Újraellenőrzés` (one word fits the mini button).
- watch → `figyel`, with `A Cmdr` as the subject: a subjectless third person dangles under a bare `Állapot` label.
- `settings.summary.servers` states, never negates (`…; ez az oldal csak a kulcsokról szól.`), keeping clear of the "Y,
  not X" shape.

## Az Android-telefon panelüzenetei, elemleírásai és a hibakeresés-tipp (`adb.connect.*`, `adb.readiness.*`, `adb.hint.*`, `adb.disconnect*`, `settings.behavior.adbHintDismissed.*`)

- Allow (Android's own button) → `„Engedélyezés”` (AOSP `usb_debugging_allow`), quoted with a `gomb` head noun:
  `koppints az „Engedélyezés” gombra`. The name is known and vowel-initial, so `az` is right.
- USB debugging → `USB-hibakeresés` (our hyphen, per AkH); `az Android platform tools csomag` carries the suffix on the
  base noun; the tooling is `Android-fejlesztőeszközök`, never bare `Android-eszköz` (that's a device).
- How (the hint link) → `Hogyan?` (tentative, no Tier-1 hit), over `Bővebben`, which is Learn more.
- Disconnect aria = the server's `{name} leválasztása`, never `kiadás`: the phone stays on the cable.
- reseat the cable → `húzd ki és dugd vissza a kábelt` (`errors.provider.macDroid.transient`).
- You stopped opening your phone → `Leállítottad a telefonod megnyitását.`, the `leállít` stem of
  `search.coverage.walk.cancelled`, never `Mégsem`'s: the sentence reports, it doesn't name the button.

## A zárolt szerveridentitás (`servers.sheet.identityLocked`)

The hint uses the buttons' own verbs (`elfelejt`, `hozzáad`), since a synonym sends the reader to a menu item that
doesn't exist. are what name this server → `azonosítja ezt a szervert`: the form has its own `Név` field.

## Időtartam-helyőrző mellé kell a névutó (`servers.paneState.retryKeepsTrying`)

A bare `{duration}` reads as a subject ("2 minutes keeps trying"), so a duration always takes a postposition (`ideig`,
`alatt`, `múlva`). Before `ideig` the value takes `-nyi`: `Összesen {duration} ideig…` with `2 percnyi` from
`retryTotalMinutes`, since `2 perc ideig` is wrong.

## A toast, amikor nem is volt mentett jelszó (`fileExplorer.navigation.forgetSecretNoneToast`)

`Nem volt mentett jelszó ehhez: „{name}”.`: the sibling `Mentett jelszó elfelejtése` wording, the name after a colon,
and no `nem sikerült` or `hiba`, because nothing went wrong.

## Az újrapróbálkozás hossza, a gazdakulcs-fejléc és az Android Engedélyezés gombja (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `servers.hostKey.*`, `adb.connect.unauthorized`)

- `{seconds}` / `{minutes}` only pick the plural branch; both branches read the same (`60 másodpercnyi`).
- Cmdr won't connect to {name} → `A Cmdr nem kapcsolódik ide: {name}`: present tense, a standing refusal.

## A szerversor helyi menüje: `Megnyitás` és `Szerver szerkesztése…` (`menu.network.open`, `menu.network.edit`)

Open → `Megnyitás`, like `menu.file.open` (Finder uses one verb for both senses). Edit server… is byte-identical to
`commands.serversEdit.label`: both open one sheet, and `i18n-terms` holds the pair.

## A funkcióbillentyű-sáv helyi menüje (`fileExplorer.functionKeyBar.hiddenToast`)

function key bar → `funkcióbillentyű-sáv`, as `settings.appearance.showFunctionKeyBar.label`.

## A Dockba kerülés egyszeri ajánlata (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

The `Dock` / `Finder` suffixes and `Alkalmazások` (never `Programok`) live in `style.md`.

### A többi eldöntött szó

- configuration profile → `konfigurációs profil` (MS; macOS has no hit).
- No, thanks → `Nem, köszönöm` (tentative), over `Most nem`, which promises "maybe later": Cmdr never asks again.
- Yes, … → `Igen, kerüljön a Dockomba`: the subjunctive names no actor, where `tedd` would address Cmdr.

### Mondatszintű döntések

- The title carries macOS's Keep in Dock: `Maradjon a Cmdr a Dockodban?`.
- None of the four outcomes is an error message: `A Cmdr most nem került be a Dockba.`
- `addedButDockDidNotRestart` → `… a helyén van, csak a Dock nem töltődött újra.`: `csak` says one detail is missing,
  where `de` would suggest the pin failed.
- `managedDock` → `Ezen az tud változtatni, aki ezt a Macet felügyeli.`, blaming no one.

## A Dock helyi menüje (`menu.dock.*`)

Mined live from `/System/Library/CoreServices/Dock.app/Contents/Resources/hu.lproj/DockMenus.strings`
(`plutil -convert json`; the pile lacks the Dock; macOS 26.6.2, 2026-09-09), since that's the very menu these items
join.

- Open Cmdr → `Cmdr megnyitása`: the Dock's app pattern is a bare name + a nominal action, no article (`%@ elrejtése`).
- Go to folder… → `Ugrás mappához…` (Finder `261`), ❌ not `Ugrás útvonalra…`, which is `menu.go.goToPath`'s different
  English.
- `{name} ({parent})` stays as English (Finder `^0 (^1)`), with a `sameAsSourceJustification`.

## A „Megjelenítés a Finderben” ajánlat és az első találat értesítése (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Show in Finder → `„Megjelenítés a Finderben”`, quoted, as the Settings card names it. for a while now → `már egy ideje`,
❌ never a number. The first-hit notice explains, never apologizes.

## A bevezető átírt lépései: ellenőrzőlista, lépésbuborék, összefoglalók (`onboarding.*`)

### Az eldöntött szavak

- star → `csillagoz`, like → `kedvel` (Microsoft's verb senses): GitHub and AlternativeTo ship no Hungarian UI, so no
  Tier-1 answer exists. ❌ Not `lájkol` (MS knows `lájk` only as a noun).
- repo → `repó`, ❌ not `adattár` / `tároló`. checklist → `ellenőrzőlista`, mailing list → `levelezőlista`, badge →
  `jelvény`.
- More about {topic} → `Bővebben: {topic}` (macOS ships `Bővebben:`), since `{topic}` can't take `-ról`.
- typo → the verb `elgépel` (tentative): `nem gépelted-e el`.

### Mondatszintű döntések

- Checklist rows are `te` imperatives (`Csillagozd meg a repót a GitHubon`): `checklist.email` must be a sentence around
  its field, and a mixed form shows inside one list.
- `az AlternativeTo oldalán`: the name's final `o` isn't a Hungarian `o`, so a base noun carries the suffix.
- A known literal gets the real article: `a <code>brew install cmdr</code> parancsot`.
- The step counter is `Lépés: {step}/{total}`: no article or suffix touches either number.
- Local Network → `Helyi hálózat`, Apple's label verbatim (`SecurityPrivacyExtension`, `LOCAL_NETWORK`), ❌ not
  `Helyi hálózat elérése`, which the user won't find.
- The four `stepOptional.*.summary` lines stay within English's length, reusing their `desc` words.
- `openBeta` and `feedbackIntro` keep one frame and differ only where English does (fix vs spot bugs).
- `local.tooltip` quotes `stepAi.cloud.label` verbatim (`Igen, szeretnék AI-t`); change them together.
- dumber → `butább`, deliberately blunt, as the `@key` asks.
- Acronyms take a hyphenated suffix by pronunciation: `RAM-ot`, `CPU-t`, `LLM-edet`, `AI-t`.
- released copy → `kiadott verzió`, dev and test builds → `fejlesztői és tesztbuildek`, over `példány` (a running
  instance).

## A megjelenítő betölti a telefonon, szerveren vagy archívumban lévő fájlt (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Fetching → `Betöltés`, the viewer's own verb, ❌ not Finder's `Begyűjtés` (metadata) or `Letöltés` (nothing is
  downloaded from an archive). The title puts the name after a colon.
- {done} of {total} → `{doneText} / {totalText}` (Foundation `Progress`); so far → `Eddig {doneText}`.
- stopped arriving → `A fájl betöltése megállt.`

## A telefon elavult indexe (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

phone → `telefon`, ❌ not `okostelefon` or `eszköz` (that's device). The family reuses its drive siblings' words; the
phone's name leads its sentence bare, unquoted (`{name} nem szól a Cmdrnek`).

## A szerver gyökérmappája és kezdőmappája (`servers.sheet.rootFolder`/`.rootFolderHelp`/`.startFolder`/`.startFolderHelp`/`.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

- root folder → `gyökérmappa` (MS, Thunar, DC), ❌ not `gyökérkönyvtár`.
- start folder → `kezdőmappa` (tentative), over TC's `induló mappa`: one word, paired with `gyökérmappa`. ❌ Never
  `saját mappa`, which is Home.
- account and host → `a fiók és a cím`, ❌ not `gép`: the sheet's field is `Cím`.

## Miért nem csatolható egy megosztás, és miért nem töltődik be a megosztáslista (`errors.mount.*`, `errors.shareList.*`)

- Name the type, then quote the name after a colon: `A Cmdr nem tudta elérni ezt a szervert: „{server}”.`; a name as
  subject leads bare (`„{server}” vendégeknek nem mutatja meg…`); the share with its server →
  `„{share}” (szerver: „{server}”)`.
- connect to a share → `csatlakoz-` (the title above it), while the pane line keeps `kapcsolódás`.
- this computer → `ez a számítógép`, not `Mac`: these also show on Linux.
- Something went wrong → `Valami nem sikerült, miközben a Cmdr …`.
- distribution → `disztribúció` (tentative); `az smbclient`; `a gvfs-smb csomagját`.
- Identical English stays identical (`hostUnreachable`, `authFailed` in both families).

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

text editor → `szövegszerkesztő` (the app kind). Edit files in [app] → `Szerkesztésre használt app` (tentative), since a
label can't run on into an unknown app name. `{app}` never takes a suffix: `a fájl ebben nyílt meg: {app}`.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

`{name} leválasztása folyamatban van`: the possessor slot keeps the name bare.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

was disconnected → `leválasztódott` (tentative mediopassive), agentless like English.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

- `{volumeName} leválasztódott`: the mediopassive keeps the name bare, where `{volumeName}-t leválasztották` would need
  a suffix. The `leválasztás` side of the split, because a drive left the mount table (the sibling keys).
- {done} of {total} files → `{total} fájlból {done} fájlt`: the suffix lands on `fájl`.
- to / on {counterpart} → `ide: {counterpart}` / `itt van: {counterpart}`.
- nothing is lost → `semmi sem veszett el`, over the alarming `nincs adatvesztés`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

- confirm (make sure it was written) → `ellenőrizni`, ❌ not `megerősíteni`, which reads as the user should have
  approved.
- were saved on {volumeName} → `kiíródtak-e ide: {volumeName}`.
- Your originals haven't moved → `Az eredetiek nem mozdultak el.`, deliberately apart from the line above's
  `a helyükön hagyta`, as English varies too.

## A visszadugott meghajtón maradt áthelyezési munkamappa (`fileOperations.leftovers.stagingFolderKept`)

The notice never suggests deleting: these may be the only copies. Its one actionable fact is that the folder is hidden.

- unfinished (an operation) → `félbemaradt` (tentative). ❌ Not `befejezetlen` (accounting only in the pile), not
  `félbeszakadt` (reserved for interrupted, implies a cause), not `hiányos` (a half-written file, `stagedLeftover`).
- found → `találta`, over `bukkant rá` (absent from the catalog) and `megtalálta` (implies Cmdr searched).
- The drive goes in a colon slot (`ezen a meghajtón: {volumeName}`); the folder in
  `egy rejtett, „{folderName}” nevű mappában` (Finder's `„^0” nevű elem`), where `egy` agrees with nothing. Quoted,
  because the 45-character id must be found in Finder; `rejtett` comes first so it isn't lost behind the id.

## A kedvencek menüje (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`, `fileExplorer.navigation.favoritesAlreadyAdded`, `fileExplorer.navigation.favoritesCantAddHere`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- The list stays `kedvenc` / `Kedvencek`, ❌ never `könyvjelző`.
- favorites menu: the title is appositive `Kedvencek menü` (like `Apple menü`), prose the possessive
  `a kedvencek menüje`. Deliberate: a table label isn't a sentence.
- Show favorites → `Kedvencek megjelenítése` (Finder's `X megjelenítése`); See {count} favorites →
  `{count} kedvenc megtekintése`, a different verb, as English uses two.
- mounted share → `csatolt megosztás` (macOS `csatol`), ❌ not `csatlakoztatott` (DC's device word).
- disk here → `lemez`, since English says disk.
- `favoritesCantAddHere` → `Ez a mappa nem lehet kedvenc: a kedvencek csak lemezen és csatolt megosztáson működnek`,
  paired with its sibling's subject; a plain locative over a `-ra/-re` "points to".
- already a favorite → `Ez a mappa már a kedvencek között van`: `között` names the list the user is looking at.

## Az elutasított kiadás megnevezi, KI fogja a meghajtót (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- Every key keeps the family skeleton `X még használja ezt a meghajtót.` + a `te` imperative, active like English, over
  Apple's passive `által használatban van`.
- `{app}` leads bare, unquoted (`{app} még használja…`): a process name's first sound is unknown, so no article.
- `{apps}` arrives as an `Intl.ListFormat` list ending in `egyéb alkalmazások`, so it takes no article either, and a
  plural verb (`még használják`) that holds for any list.
- other apps → `egyéb alkalmazások` (Thunar's msgid), over `más`, which suggests a different kind.
- disk image → `lemezkép`; is still open → `még nyitva van`, over `csatolva van`, since English says open.
- Cmdr itself → `Maga a Cmdr`, in the family's subject-first skeleton. send a report → `küldj jelentést`, never
  `hibajelentést`.
- `Várj egy percet` (system) vs `Várj egy pillanatot` (Cmdr) follows English's minute vs moment.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- kind → `Fajta` (Finder's Kind sort).
- Select all with extension → `Összes *.{extension} fájl kijelölése` (DC, TC): the mask stands as an attribute, so no
  suffix hangs on it.
- The four label twins share their English, so each menu / command pair stays identical.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

- The badge NAMES the setting, so it uses Apple's row: `Nincs teljes hozzáférés a lemezhez`. `bannerTitle.denied` shares
  its English, so the two stay identical.
- The aria opens with the label verbatim, which carries WCAG 2.5.3; the second sentence's `-ről` form wouldn't.
- Running prose may say `teljes lemezhozzáférés` (names vs prose).
- "files macOS keeps to itself" stays plain, ❌ never a macOS feature name.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

- The title shares its English with four `*.title.trash` keys, so all five stay identical.
- No plural machinery: `a kiválasztott elemek közül {count} darab…` reads right at 1 and 7.
- `Shift+F8` takes no suffix: `A Shift+F8 billentyűparanccsal viszont …`.

## A csak online tartalom figyelmeztetése (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Keep all four facts: the Trash would download the files; so Cmdr offers deleting the whole selection; no copy stays in
the Trash, but the service keeps one (❌ never "it goes to the Trash anyway"); the named ways out. The quoted `„Törlés”`
is `fileOperations.delete.confirmDelete` verbatim. Tentative wording throughout.

## Amikor a szerver szerint nincs is ilyen megosztás (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- Retrying can't help, so there's no `most` or `próbáld újra`, unlike the siblings.
- `A szerver szerint ugyanis nincs ilyen nevű megosztás.` is its own sentence: the settled opener ends on the name after
  a colon, so no clause can follow; `ugyanis` carries "because".
- The short toast says `{server} szerint`: the postposition leaves the name bare.
- a lot slower → `sokkal lassabb`: no multiplier, since English gives none.

## Egyformának látszó nevek a szerveren (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.explanation`, `errors.listing.ambiguousName.suggestion`, `errors.volume.ambiguousName`)

- spelled differently → `eltérő írásmóddal tárolja őket` (tentative), on both surfaces; ❌ never `Unicode` or
  `normalizálás`: the `@key` asks for everyday words.
- matches (a path) → `illik`, over DC's `illeszkedik`.
- The path goes after a colon: `illik erre az útvonalra: {path}`.
- One `hogy` per sentence: `Legközelebb elkerülheted, ha az egyiket átnevezed úgy, hogy…`.

## A „Felhő-AI engedélyezése” kapcsoló és a kikapcsolt felhő-AI állapotai (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Allow cloud AI → `Felhő-AI engedélyezése` (quoted when named); as a verb `engedélyezd a felhő-AI-t`. side panel →
`oldalpanel` (tentative). Brands stay bare: `Az Ollama, az LM Studio … esetében`. `Ask Cmdr-csevegések` is hyphenated.

## Kilépés a teljes képernyőből az Escape billentyűvel (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- full screen (the window state) → `teljes képernyős mód` (AppKit `Kilépés a teljes képernyős módból`), ❌ not bare
  `teljes képernyő`, which macOS uses for Entire Screen.
- Escape takes a base noun, `az Escape billentyűvel`: its final `e` is silent, so a suffix would need a hyphen.

## A „Megnyitás ezzel” és a „Megosztás” almenü helyőrző sorai (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

Finding apps… → `Appok keresése…`; share options → `megosztási lehetőségek` (tentative), over `beállítás` (settings); No
share options → `Nincs megosztási lehetőség`.
