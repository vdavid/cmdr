# hu decisions

Distilled rulings behind `terms.json`: "X over Y because Z", one entry per topic, edited in place. `pnpm i18n:brief`
pulls a section when its heading cites a batch key, so every heading cites its keys in backticks. Term choices, their
avoid-lists, and sources live in `terms.json`; this file keeps what a term ruling can't hold (sentence frames,
placeholder handling, family alignment). Voice is in `style.md`, open questions in `review-queue.md`.

## A macOS-vs-Windows hasadások

macOS wins because Finder users see its word: `Kuka` over `Lomtár` (MS reserves it for the Recycle Bin), `szerver` over
`kiszolgáló`, `Mégsem` over `Mégse`. Bookmark → `könyvjelző` (tentative); favorites stay `kedvenc`.

## A Beállítások szakaszai (`settings.section.*`)

Quote a section name verbatim from `settings.section.*`. Wilting `Hervadás`, Smart `Okos`, Personal
`Személyes (ingyenes)`.

## A kötetválasztó csoportcímei (`fileExplorer.navigation.group*`)

The Network group stays `Hálózat`, its Servers row `Szerverek`; don't collapse them.

## Gép vagy gazdagép (`fileExplorer.network.*`, `commands.networkSelectHost.*`, `errors.listing.host*`)

The network browser says `gép`, connection-failure prose `gazdagép`, deliberately. `kiszolgáló` in `errors.json` is the
participle "serving".

## Git-szavak (`errors.git.*`, `fileExplorer.git.*`, `settings.fileExplorer.git.*`)

`munkafa` is only the generic working tree (`errors.git.bareRepo`, `blobTooLarge`, `gitDirPermissionDenied`); a linked
checkout is `worktree` (`worktree-eket`). Bare `csupasz repó`, dirty `piszkos állapot` (tentative).

## Relatív időcímkék (`fileExplorer.*.ago*`)

`{count} p/ó/n/h/hó/é ezelőtt`, just now `most`; ETA `mp` / `p`, roughly `kb.`.

## Egyeztetés több fájlon át

`Don't show again` → `Ne jelenjen meg többé`. A failed update check is whole sentences, no `Hiba:` / `Probléma:` prefix
(`updates.failure.check`).

## Státuszcellák (`queue.row.status`, `operationLog.status.*`, `fileExplorer.status.*`)

The log's Didn't finish → `Nem fejeződött be`; fallback cells `Probléma` (the `@key` asks). The backgrounding Queue
button `Sorba` (DC `Várakozási sorba helyez`, shortened).

## Átviteli többesszámok (`transfer.*`)

`{verb, select, …}` opens with `Másolás` / `Áthelyezés`, inline `másolva` / `áthelyezve`; `{phrase}` follows a colon
(`Másolva: {phrase}.`). `transfer.fileOnly.mixedMove`'s was/were → `volt`.

## Dupla kattintás a panel hátterén (`*doubleClick*`)

`szülőmappa` over Finder's `tartalmazó mappa`: the catalog uses it everywhere, and switching is one whole-catalog
migration, never piecemeal. `Soha többé` (tentative).

## FAT32-korlát (`errors.write.filesTooLargeForFilesystem.*`, `tooLargeAndMore`, `errors.listing.notSupportedErrno.suggestion`)

`{maxSize} méretnél nagyobb`: suffix the base noun (a unit's vowel is unsafe). `%` reads `százalék`, so `{size}%-ra`
stays.

## A célmappa még nem létezik (`fileOperations.transferDialog.targetWillBeCreated*`)

`Ez a mappa még nem létezik.` (TC, DC), then one literal sentence per key, no ICU select.

## Archívumok böngészése (`settings.archives.*`, `errors.listing.archive*`, `fileOperations.delete.archiveWarning*`)

Cells nominal (`Böngészés` / `Megnyitás` / `Rákérdezés`). `Konfigurálás…` over `Beállítás…`, which collides with
`Beállítások`.

## Vágólap beillesztése fájlként (`*pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

The branch words carry the possessive (`{kind, select, image {képe} pdf {PDF-je} other {szövege}}`); the filename
follows a colon.

## A Műveletnapló (`operationLog.*`, `commands.logOperationLog.*`)

One name for the dialog and its Settings card. `settings.operationLog.intro` says `visszavonhatod` because its English
says undo. Provenance: `Te`, `Ágens`, `AI-kliens`.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`)

Placeholders nominal (`Kérdés a fájljaidról…`), a standalone welcome a `te` imperative. Tool chips: verbal noun
(doing) + `-va/-ve` (done) on one object; `searchPhotos` → `A fotóid átvizsgálása` (`X keresése` reads as searching FOR
the photos). `{provider}` arrives localized.

## Hálózati képindexelés (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.network*`/`.paused`)

Section labels `kép`, per-drive strings `fotó`, as English. `Ez a meghajtó mindig legyen indexelve` (`mindig` + bare
verbal noun is ungrammatical). `{name} fotóinak indexelése`.

## Kizárás a képindexből (`askCmdr.renameReview.*`, `settings.mediaIndex.excludedFolders.*`, `fileExplorer.imageIndex.*`)

excluded → `kizár` over `kihagy` (`kihagyva` is the transfer's skipped). Next pass → `a következő átvizsgálás`.

## A képindex jelvényei (`fileExplorer.imageIndex.*`, `settings.mediaIndex.showFileStatusIcons.*`)

`Nem sikerült indexelni`, never Finder's `Hiba`; `file.excluded` → `Nem szerepel a képkeresésben` (`kizárva` would claim
a user choice). `{doneText} / {totalText} kép indexelve`; `Az összes kép ({totalText})`.

## Képindex-beállítások (`settings.mediaIndex.*`, `fileExplorer.imageIndex.file.indexing`)

`leírás alapján`; `Ezzel felszabadul {size}` keeps `{size}` unsuffixed.

## Forrás / Cél (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.*GroupTitle`)

From / To headings → `Forrás` / `Cél` (TC `662`/`663`, DC) over `Innen` / `Ide`, which dangle; `Innen:` stays the inline
scan-phase label.

## A meghajtó indexelésének kikapcsolása (`*IndexingOff*`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

`(a) meghajtó indexelése`, never `meghajtó-indexelés`; bare `indexelés` per drive.
`ott folytatja majd, ahol abbahagyta`. `Az indexeléssel együtt ki` (`együtt` forces the comitative in 25 chars).

## Változásellenőrzés (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `*tooltipCoalesced*`)

`Változások ellenőrzése`, `Fájllista frissítése` (sibling run headers); a full check is `átvizsgálás`.

## A megtorpant átvitel értesítése (`fileOperations.transferProgress.stall*`/`.close`)

`{duration} óta nincs előrehaladás`. `Várakozás a cél/forrás válaszára` (the dialog's sides) over `nem reagál` (a
fault). `Az átvitel megállt` (tentative) over `leállt` (ended). `részben ki van írva` over `kiírásra került`. `close` =
`Bezárás` beside `Mégsem`: window vs operation.

## Másolt útvonal (`fileExplorer.clipboard.copiedPath`)

The path renders on its own line: `Útvonal másolva, most már a vágólapon van:`.

## A műveleti sor (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

`Műveleti sor` pairs with `Műveletnapló` (one head noun for the View-menu pair); consonant-initial, so
`a műveleti sorba`.

## Sarokchip, elvetés (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

Toolbar `Mindet elveti` (finite like `Mindet szünetelteti`), aria `Ennek a műveletnek az elvetése`.
`Nem sikerült befejezni` + accusative, verb first in all nine arms, so toast and row agree. Screen-reader
`{percentText} százalék`; the tooltip keeps `42%`.

### `queue.chip.tooltip`: the dot-separated fact line

`{label}` arrives as a verbal noun, so each optional clause carries its own `·` inside its branch (like
`ai.toast.progress`). ` · ide: {destination}` (macOS `Áthelyezés ide: %@`). `{detail}` arrives in Hungarian.

## Ütközési kérdés (`fileOperations.operationConflict.*`)

`Másolás ide: {destination}`, Working `Folyamatban itt: {destination}`; an archive `{destination} szerkesztése`.
`pausedNote` says `szüneteltetve van`, the row's word.

## A progress-párbeszéd gombja üres sornál (`fileOperations.transferProgress.background*`)

The aria `Hagyd futni a háttérben` (from `queueTooltip`) contains `Háttérben` (WCAG 2.5.3).

## A kilépési kapu (`main.quit.*`)

Quit now → `Kilépés most` (macOS `<Noun> most`). Prose `folyamatban van` (macOS's quit-blocked sentence); the heading
`Még folyamatban` is verbless since `Még fut/futnak` breaks between one and many; the `Fut` cell stays. Keep working →
`Munka folytatása` (tentative), also over `Később`: the countdown is deleted, not deferred.
`{secondsText} másodperc múlva`; `eltávolít` over `törli` (it reassures).

## Usage stats (`settings.analytics.enabled.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analytics*`)

`használati statisztika`, `egy véletlenszerű azonosító`.

## Visszagörgetés-megerősítő (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

`Válaszolnod kell` (tentative). Keep them → `Fájlok megtartása` (bare `Megtartás` could point at the overwritten files).
`rollbackTooltip` → `Leállítás, és minden eddig kiírt fájl törlése` (the `@key`: not a plain Cancel).
`Itt valami más van nyitva`: the blocker may not be an operation.

## Átnevezés-lánc (`fileExplorer.rename.chainKeptOriginalName*`)

`{reason}` stands last after a colon: `„{name}” megtartotta a nevét: {reason}`;
`{othersText} másik fájl megtartotta a nevét, és „{name}” is: {reason}` (the gapped `is` puts the named file before the
colon, since `{reason}` covers only it). `one` → `Egy másik fájl`.

## Nem használható név (`fileOperations.validation.nameNotUsable`)

`A fájlnév nem használható` / `A mappa neve nem használható` (macOS), no period: it's embedded.

## AI-javaslatok (`suggestedOps.*`, `commands.suggestedOpsShow.*`, `menu.view.suggestedOps`)

Title `AI-javaslatok` (same English as `fileOperations.mkdir.aiSuggestions*`) over `Javasolt műveletek`; `Why:` →
`Miért:` like `Miért?`, spoken `Miért, az AI szerint:`. Pills `Ezt vissza tudod vonni:` / `Ezt nem lehet visszavonni:`;
facts `Forrás` / `Cél` (transfer dialog); tick box `belefoglalás` (Finder); reject → `Elutasítás`.

## Megkettőzés (`commands.fileDuplicate.*`)

`Megkettőzés` (Finder `Fájl > Megkettőzés`).

## Natív menük (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Finder's wording wins (`Finder.app/.../hu.lproj`, macOS 26.5.2): `Nézet`, `Ugrás`, `Saját`, `Kijelölés törlése`,
`Méretezés`. `Felnagyítás` / `Lekicsinyítés` (Safari) free `Nagyítás` for the submenu. `Módosítási napló` (apart from
`Újdonságok`), `Sortörés`, `Megnyitás szerkesztésre` (tentative). `Képek indexelésének tiltása itt` /
`… engedélyezése itt`. `„{color}” hozzáadása` / `eltávolítása` (Finder `TG5`/`TG6`). `Kilépés a cmdrből` keeps English's
lowercase; `menu.zoom.percent*`, `menu.view.askCmdr` match English.

## busy (`menu.volume.*Busy`)

Base label unchanged + ` (foglalt)`; one marker for every busy key.

## SMB-rendszerkapcsolat (`fileExplorer.network.osMountFallback.*`)

Elsewhere `rendszerkapcsolat`, where English says system connection. `Próbálkozás közvetlen kapcsolattal` (tentative)
keeps "try": the direct connection already failed once. `smbNativeNote` quotes
`„Közvetlen kapcsolat a gyorsabb hozzáférésért”`.

## Átnevezés/létrehozás elutasításai (`errors.mutation.*`, `errors.volume.*`)

- `{path}` in a colon slot (`itt: „{path}”`) or leading bare and quoted (`„{path}” mappa, nem fájl.`).
- `Oldd fel a zárolását` (Finder `PE13`); SIP `… védelme alatt áll` (no repeated `véd-`). `nem válaszolt időben` over
  `nem reagál`. `Ez a név nem használható a célhelyen.` (Finder). `Ez a jelszó nem jó.`
- `timedOut` isn't a failure: `… így a módosítás attól még végbemehet.` `deviceSessionReset` never says unplugged;
  `deviceDisconnected` → `Megszakadt a kapcsolat az eszközzel`. `A macOS nem engedte ezt a Kukába helyezni.`: a refusal,
  not `nem sikerült`.

## Ha a Cmdr nem állt le (`crashReporter.dialog.body.keptRunning`/`.unknown`)

`problémába ütközött` (Finder `NE105`'s `-ba ütközik`), not `egy probléma miatt` (needs a verb saying it stopped).
`és tovább futott`. Never say it stopped (`váratlanul kilépett`, `bezárult`, `összeomlott`); `.unknown` doesn't name the
background either. `összeomlási` only where there was a crash.

## Jelentésküldés beállítása (`settings.updates.crashReports.description`)

The description uses both dialog verbs (`váratlanul bezárul`, `a háttérben problémába ütközik`); the label stays
`Összeomlás-jelentések küldése`.

## Kiadás és leválasztás (`errors.eject.*`)

Each follows `Nem sikerült kiadni: {volumeName}: {message}`, so it's a sentence after a colon. `használatban van`
(Finder `NE66`); network share `hálózati megosztás` over `szerverkötet` (the mounted volume). `timedOut`:
`… de a kiadás magától is befejeződhet.` `unexpected` →
`A Cmdr problémába ütközött, és nem tudta megállapítani, hogy mi.`, apart from `errors.mutation.unexpected`: after the
wrapper, `Valami nem sikerült` would repeat itself.

## A macOS-panelnevek magyarul (`errors.write.fileLocked.suggestion`, `errors.write.permissionDenied.suggestion.delete`, `errors.listing.*ermission*.suggestion`)

Apple translates them: `„Zárolt”` (quoted, as Apple), `Megosztás és jogok`, `Apple-fiók`, `Általános`,
`Indítóelemek és bővítmények` (macOS 26.6.2 `.loctable`, 2026-08-30). A menu item is a `parancs`, never a `menü`.
`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` take no suffix or article:
`itt: {system_settings}`.

## A Kuka-értesítés (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

`Ugrás a Kukába` (Finder). `Ezen a meghajtón nincs Kuka.`: a fact, not `nem támogatja`.

## A már elküldött jelentés kiegészítése (`errorReporter.amend*`, `errorReporter.autoSentToast.viewOrAddNotes`)

note → `megjegyzés` over `jegyzet` (MS: Notes items); `megjegyzést hozzáadni`. `Mi került elküldésre` mirrors
`Mi kerül elküldésre`: only tense differs.

## Kijelölés törlése (`selection.*`)

`kijelölés törlése` (Finder `300488`) over the orthodox pair's `megszüntetése`. Titles match the menu items
(`Fájlok kijelölésének törlése`). The tooltip needn't contain the label. `selection.recent.*` twins `queryUi.recent.*`;
`{query}` ends after a colon.

## Egy fogalom, egy név

`desktop-i18n-term-consistency` holds one form per English value: `Kilépés a Cmdrből`, `Kapcsolódás szerverre` (Finder
`N84`), `Kapcsolódva`, Connecting `Csatlakozás…` (`SavePanel`), `Kis- és nagybetűérzékeny`.

### A határvonalak, amiket NEM szabad elsimítani

`Nézet` menu / `Megtekintés` viewer; `Nagyítás` text / `Méretezés` window; `Kijelölés` files / `Válassz` placeholder;
`Bájtok` beside `Fájlok` / `Bájt` beside `kB`; `Bíbor` Finder tag / `Lila` Cmdr tint; `Visszagörgetés…` window /
`Visszagörgetés folyamatban` log cell; `Elküldöd a jelentést?` title / `Jelentés küldése` button; `memória` RAM /
`jegyzet` Ask Cmdr's memory.

## Az angol önellentmondásai (`askCmdr.renameUndo.undone`/`.partial`, `settings.indexing.enabled.description`, `menu.app.showAll`/`.hideOthers`, `commands.app*.label`)

`{countText} fájl régi neve visszaállítva.` (the Trash keeps `visszahelyezve`). `azonnali mappaméretekért`.
`Összes megjelenítése` / `Többi elrejtése` (Finder `300730` / `300729`).

## Félbehagyott visszagörgetés (`*finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `*titleFinish`, `queue.row.reversalInFolder`)

`Visszagörgetés befejezése` (Finder `Másolás befejezése`), identical in both keys. `még egy kör` (tentative) over
`átfutás` / `menet`. `A létrehozottak törlése itt: {folder}`.

## A megszakított visszagörgetés eredményértesítése (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- `<alany> változatlan maradt: <indok>.` in every reason row (the `skipReason.*` frame); the intro keeps `kihagyja`. ❌
  Not `békén` / `érintetlenül hagyja` (zero pile hits).
- Result rows `visszahelyez`, progress rows `visszavitel` (`visszavíve` doesn't read); `eltávolítva`, never `törölve`.
- Full vs partial lives in the sentence (`A Cmdr mindent eltávolított, amit létrehozott: {countText} elem.` vs
  `{countText} elem eltávolítva.`).
- `módosult`; `ellenőriz` apart from `megerősít`; `Nem sikerült visszagörgetni: „{name}”.`; `odatette` (tentative).
  `undoJob` → `Az összes {csomag} visszavonása ({countText})` (macOS `Az összes lemez (^0)`).

### `cancelRollback.stagedLeftover.*`

`hiányos másolat` (macOS `LA33`); `újra megpróbálja, amikor legközelebb kapcsolódik oda`, never a time.

## Régi WebKit (`main.oldWebkit.*`)

`Szoftverfrissítés`; `Safari 15.4-es vagy újabb verzió`.

## Régi macOS (`main.oldMacos.*`)

`félremehet`, `valami elromlottnak tűnik` (out of the `hiba` register). The last sentence is David, `te`.

## Belenézés a fájlokba (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.*`)

`kameraadatok`, not `EXIF`. `hol készült` / `készítési helye` (tentative). `az archívumban lévő fájlok listája` over
`az archívum tartalma` (would clash with "whole files never leave"). Prose `belenéz`; chips `Fájlok átnézése` /
`Fájlok átnézve` (tentative): `belenéz` has no `-va/-ve`, `átvizsgálás` is `searchPhotos`'. `egy korlátozott részét`:
English promises a limit, not a size.

## A Rollback gomb súgói (`*rollbackTooltipStopAndMoveBack`, `*rollbackAlreadyLandedTooltip`)

`Leállítás, és minden eddig áthelyezett fájl visszahelyezése`; ❌ not `törlés`: a move rollback deletes nothing.

## Terminál megnyitása itt (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

The kind is `terminál`, Apple's app `Terminal` (`a Terminalban`, Finder `N67`). `App kiválasztása…`.

## Sort by relevance (`fileExplorer.columns.sortByRelevance`)

`relevancia` (tentative) over `Fontosság` / `Találati pontosság` (neighboring ideas).

## Documents and packages (`settings.archives.ooxml.*`)

Bare `csomagok` stays wider than `Alkalmazáscsomagok`, as English splits them. Frame `Mit tesz az Enter egy … fájlon.`

## A szerverközpont: kapcsolódás (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`.disconnect*`/`.forget*`)

- `Kapcsolódás ide: {name}…` (Finder `MN1`); quote only where English does. `{name} ezen a szerveren: {server}`.
- Pane buttons are dialog buttons: `Mégsem`, even `servers.paneState.cancel` (Finder's connect wait).
- `{host} kulcsa`; trust statement `megbízhatónak tekint`, ❌ never `elfogad` (hides the decision).
- Open X to Y → a `-hoz/-hez` purpose noun (`a kapcsolódáshoz`), never `hogy`.
- `Elfelejted ezt a szervert: „{name}”?`; the browser keeps `gép` and unquoted `{hostName}`. `Semmit nem csatol le`.
  Aria `{name} leválasztása`.
- `{username} felhasználóként` / `vendégként`; `Bejelentkezés másként…` (`Mentés másként`); `Böngészés vendégként`;
  `Hozzáadás mindenképp` (AppKit).
- Not connected (chosen) → `Nem kapcsolódik: {name}` (AppKit), ❌ not `Nincs csatlakoztatva`. Optional field
  `Nem kötelező` (MS); an optional step `választható`.

## A szerverközpont táblázata (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

Address column `Cím`. Found nearby → `A közelben felfedezve` (IOBluetoothUI). `Szerver rögzítése / rögzítés feloldása`
(Notes); `kötetválasztó` for both English names; `{name} mostantól ott van a kötetválasztódban.` `Macet` (161 macOS
hits), `NAS-t`.

## A szerverlap (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.*Server`, `commands.serversConnect.label`)

- Trust button `Beállítás megbízhatóként` (UsersGroups). `valami beékelődött közéd és a szerver közé` (Apple's stem, no
  attack named).
- ⚠️ The pane line keeps `kapcsolód-`, the sheet `csatlakoz-` (`fileExplorer.network.connect` forces `Csatlakozás…`).
  Don't sweep them together.
- `Megjegyzés a kulcskarikában`, no `jelszó` (SFTP may save a passphrase); `Kulcsjelmondat`.
- `Kijelentkezve innen: {name}` keeps the pane lines nominal; `Ellenőriztem a kulcsot` / `a tanúsítványt`.
  `az ssh-agent` (esz-esz-há); `Nextcloud-cím`.

## Az automatikus újracsatlakozás (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

`Újracsatlakozás ide: {name}…`: the frame from the pane lines, the stem from the family. The server stays subject
(`Ez a szerver jelszó helyett kulcsot használ…`). `Nyisd meg újra a kapcsolódáshoz.`

## Rögzítési tipp, szerverkulcsok, ADB-állapot (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.behavior.serversPinHintSeen.*`, `settings.appearance.tintSmb.*`)

`Rögzítés a kötetválasztóban` (WorkflowUI `Rögzítés a menüsoron`). `Kezd hosszúra nyúlni a Hálózat csoportod` (neutral);
`ezt a parancsot: „{command}”`. Trusted date label `Megbízhatóként jelölve` (SecurityInterface), ❌ not `Jóváhagyva`.
`Újraellenőrzés`. watch → `figyel` with `A Cmdr` as subject. `settings.summary.servers` states, never negates.

## Android-telefon (`adb.*`, `settings.behavior.adbHintDismissed.*`)

`koppints az „Engedélyezés” gombra` (AOSP). `az Android platform tools csomag`; `Android-fejlesztőeszközök`, never
`Android-eszköz`. `Hogyan?` (tentative) over `Bővebben` (Learn more). `húzd ki és dugd vissza a kábelt`;
`Leállítottad a telefonod megnyitását.` (`leállít`, as `search.coverage.walk.cancelled`).

## Zárolt szerveridentitás (`servers.sheet.accountLocked`)

The hint uses the buttons' own verbs (`elfelejt`, `hozzáad`); `azonosítja ezt a szervert` (the form has its own `Név`
field).

## Időtartam és névutó (`servers.paneState.retryKeepsTrying`)

A bare `{duration}` reads as a subject, so it takes a postposition; before `ideig`, `-nyi` (`2 percnyi`).

## Nem volt mentett jelszó (`fileExplorer.navigation.forgetSecretNoneToast`)

`Nem volt mentett jelszó ehhez: „{name}”.`: nothing went wrong.

## Újrapróbálkozás hossza (`servers.paneState.retryTotal*`, `servers.hostKey.*`, `adb.connect.unauthorized`)

Both plural branches read the same (`60 másodpercnyi`). `A Cmdr nem kapcsolódik ide: {name}`: present, a standing
refusal.

## A szerversor helyi menüje (`menu.network.open`, `menu.network.edit`)

`Megnyitás` like `menu.file.open`; Edit server… byte-identical to `commands.serversEdit.label`.

## Funkcióbillentyű-sáv (`fileExplorer.functionKeyBar.hiddenToast`)

`funkcióbillentyű-sáv`, as `settings.appearance.showFunctionKeyBar.label`.

## A Dockba kerülés egyszeri ajánlata (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

`Nem, köszönöm` (tentative) over `Most nem` ("maybe later"; Cmdr never asks again). `Igen, kerüljön a Dockomba`: the
subjunctive names no actor. `Maradjon a Cmdr a Dockodban?` (macOS Keep in Dock). No outcome is an error:
`A Cmdr most nem került be a Dockba.`; `… csak a Dock nem töltődött újra.` (`de` would suggest failure); `managedDock`
blames no one.

## A Dock helyi menüje (`menu.dock.*`)

Mined live from `Dock.app/Contents/Resources/hu.lproj/DockMenus.strings` (`plutil -convert json`; not in the pile; macOS
26.6.2, 2026-09-09): bare name + nominal action, no article (`Cmdr megnyitása`, `%@ elrejtése`). `Ugrás mappához…`
(Finder `261`), ❌ not `Ugrás útvonalra…` (`menu.go.goToPath`). `{name} ({parent})` as English (Finder `^0 (^1)`).

## Megjelenítés a Finderben (`main.reveal*`, `settings.behavior.reveal*`)

`„Megjelenítés a Finderben”`, quoted as the Settings card names it; `már egy ideje`, ❌ never a number. The notice never
apologizes.

## A bevezető átírt lépései (`onboarding.*`)

- `kedvel` (MS), ❌ not `lájkol`. ❌ Not `adattár` / `tároló`. `ellenőrzőlista`, `levelezőlista`, `jelvény`.
  `Bővebben: {topic}` (macOS). typo → `elgépel` (tentative): `nem gépelted-e el`.
- `a <code>brew install cmdr</code> parancsot`; `Lépés: {step}/{total}`. `Helyi hálózat` (`LOCAL_NETWORK`), ❌ not
  `… elérése`.
- `stepOptional.*.summary` stay within English's length; `openBeta` and `feedbackIntro` share a frame; `local.tooltip`
  quotes `stepAi.cloud.label` (change together). `butább` (the `@key` asks). `kiadott verzió`,
  `fejlesztői és tesztbuildek`, over `példány`.

## A megjelenítő betöltése (`viewer.pull.*`, `viewer.error.stoppedResponding`)

The name after a colon. `{doneText} / {totalText}` (Foundation `Progress`), `Eddig {doneText}`,
`A fájl betöltése megállt.`

## A telefon elavult indexe (`*StalePhone`, `indexing.staleDialog.*Phone`)

`telefon`, ❌ not `okostelefon` / `eszköz`; the drive siblings' words; `{name} nem szól a Cmdrnek`.

## A szerver gyökérmappája és kezdőmappája (`servers.sheet.rootFolder*`/`.startFolder*`/`.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

`kezdőmappa` (tentative) over TC's `induló mappa`: one word, paired with `gyökérmappa`. account and host →
`a fiók és a cím` (the field is `Cím`).

## Megosztás csatolása, megosztáslista (`errors.mount.*`, `errors.shareList.*`)

`A Cmdr nem tudta elérni ezt a szervert: „{server}”.`; as subject, bare (`„{server}” vendégeknek nem mutatja meg…`);
`„{share}” (szerver: „{server}”)`. Share connect `csatlakoz-`; `ez a számítógép` (also shows on Linux);
`Valami nem sikerült, miközben a Cmdr …`. `disztribúció` (tentative); `az smbclient`; `a gvfs-smb csomagját`. Identical
English stays identical across both families.

## F4 és a szövegszerkesztő (`settings.behavior.textEditorApp.*`, `*card.textEditor`, `fileExplorer.edit.*`)

`Szerkesztésre használt app` (tentative): a label can't run on into an unknown app name.
`a fájl ebben nyílt meg: {app}`.

## Leválasztódó meghajtó (`*driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`)

`{name} leválasztása folyamatban van`; `leválasztódott` (tentative): agentless and name-bare (over
`{volumeName}-t leválasztották`), `leválasztás` because the drive left the mount table. `{total} fájlból {done} fájlt`;
`ide: {counterpart}` / `itt van: {counterpart}`; `semmi sem veszett el` over `nincs adatvesztés`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

`ellenőrizni`, ❌ not `megerősíteni` (reads as user approval). `kiíródtak-e ide: {volumeName}`.
`Az eredetiek nem mozdultak el.`, apart from `a helyükön hagyta`, as English varies.

## Maradék munkamappa (`fileOperations.leftovers.stagingFolderKept`)

Never suggest deleting (they may be the only copies); the actionable fact is that the folder is hidden. `félbemaradt`
(tentative), also ❌ not `hiányos` (a half-written file). `találta` over `bukkant rá` / `megtalálta` (implies
searching). `ezen a meghajtón: {volumeName}`; `egy rejtett, „{folderName}” nevű mappában` (Finder `„^0” nevű elem`):
quoted so the 45-char id can be found, `rejtett` first so it isn't lost.

## A kedvencek menüje (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

❌ Never `könyvjelző`. Title `Kedvencek menü` (like `Apple menü`), prose `a kedvencek menüje`. disk → `lemez`.
`favoritesCantAddHere`: `…lemezeken, megosztásokon, szervereken és telefonokon lévő mappák` (`lévő` over
`-ra/-re mutat`, a symlink reading). `Ez a mappa már a kedvencek között van`.

## Ki fogja a meghajtót (`errors.eject.unmountRefusedBy*`, `errors.eject.other*`)

- Skeleton `X még használ ott valamit.` + `…, majd próbáld újra.`, active, over Apple's passive
  `által használatban van`; `ott`, never `ezt a meghajtót` (a share or phone may be meant).
- `{app}` leads bare, unquoted; `{apps}` takes a plural verb (`használnak`), `Zárd be az appot` / `ezeket az appokat`;
  `A Cmdr nem ismeri…`, never `nem ismerjük`.
- `egyéb alkalmazások` (Thunar) over `más`; `egyéb folyamatok`; `lemezkép`; `még nyitva van` over `csatolva van`;
  `Maga a Cmdr`; `küldj jelentést`, never `hibajelentést`. `Várj egy percet` (system) vs `Várj egy pillanatot` (Cmdr).
  `Egy {process} nevű folyamat…`; the plural lists after a colon. Eject all → `Összes kiadása` (Finder `N199.1`).

## Azonos fajta kijelölése (`menu.select.*`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

`Fajta` (Finder's Kind). `Összes *.{extension} fájl kijelölése` (DC, TC). Menu / command twins stay identical.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

The badge names Apple's row `Nincs teljes hozzáférés a lemezhez`, identical in `bannerTitle.denied`. The aria opens with
the label (WCAG 2.5.3). Prose may say `teljes lemezhozzáférés`.

## Kuka-elutasítás (`errors.write.trashRefused.*`)

The title matches four `*.title.trash` keys. `a kiválasztott elemek közül {count} darab…` reads right at 1 and 7.
`A Shift+F8 billentyűparanccsal viszont …`.

## Csak online tartalom (`fileOperations.delete.cloudOnlineOnly*`)

Keep all four facts: the Trash would download them; so Cmdr offers deleting the whole selection; no copy in the Trash
but the service keeps one (❌ never "it goes to the Trash anyway"); the ways out. `„Törlés”` quotes `confirmDelete`.
Tentative.

## Nincs ilyen megosztás (`*shareNotOnServer*`, `*ShareNotOnServer*`)

No `most` or `próbáld újra` (retrying can't help). `A szerver szerint ugyanis nincs ilyen nevű megosztás.` stands alone
(the opener ends on a colon slot). Toast `{server} szerint`; `sokkal lassabb`, no multiplier.

## Egyformának látszó nevek (`*lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

`eltérő írásmóddal tárolja őket` (tentative), ❌ never `Unicode` / `normalizálás`. `illik` over DC's `illeszkedik`:
`illik erre az útvonalra: {path}`. One `hogy` per sentence.

## A „Felhő-AI engedélyezése” kapcsoló (`ai.cloudConsent.label`/`.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Quoted when named; verb `engedélyezd a felhő-AI-t`. `oldalpanel` (tentative). `Az Ollama, az LM Studio … esetében`;
`Ask Cmdr-csevegések`.

## Teljes képernyő és Escape (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

`az Escape billentyűvel`.

## Almenü-helyőrzők (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

`Appok keresése…`; `megosztási lehetőségek` (tentative) over `beállítás`; `Nincs megosztási lehetőség`.

## Visszamaradt eredeti (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

`Minden megérkezett ide: {landedAt}`, `az eredetit innen: {path}`; `engedély` over `jogosultság`, like
`permissionDenied.message.*`.

## Archívumfájl megnyitása ezzel (`fileExplorer.openWith.copyRefused.*`)

`nem nyitható meg ezzel: {appName}`; `„{fileName}” kicsomagolása … nem sikerült`.

## S3 (`servers.sheet.s3*`, `servers.refusal.*`, `servers.hub.*Account*`, `errors.*.coldStorage*`, `*shareLink*`)

`bucket` with front suffixes: no console ships Hungarian, and tech prose writes `S3 bucketbe`.
`titkos hozzáférési kulcs`, `régió` (MS); `archiválva van`, `visszaállít`, `konzol`. `Megosztási link` (macOS
`Link másolása`); the submenu stays nominal: `Lejárat hét nap múlva`.

## NFS (`*nfs*`, `*Nfs*`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`)

- export → `megosztás` (tentative): Hungarian NAS apps and admins say `megosztott mappa` / `NFS megosztás` (MS shared
  folder → `megosztott mappa`); ❌ not `export` (MS knows only the data-export verb `exportál`).
- `Felhasználóazonosító` / `Csoportazonosító` (MS).
- `{host}` leads bare; "its exports" avoids a possessive back to it (`nem sorolja fel, milyen megosztásokat kínál`).

## MDM (`*.managed.*`, `*managedOff`, `updates.status.heldByPolicy`, `errors.serverRequest.blockedByPolicy`)

`A szervezeted kezeli` (MS `kezel`); `IT-csapat`; Off → `Ki`. `legfeljebb ezt a verziót engedi: {ceiling}`. require →
`kötelező` (macOS `Kötelező megadni`): `Mindig kötelező SMB-titkosítás` / `Mindig kötelező`; prose quotes it:
`be van kapcsolva a „…” beállítás`.

## Csoportos átnevezés súgói (`multiRename.placeholderHelp.*`, `multiRename.searchOptionHelp.*`)

`ezen a helyen: {at}`, the example file after `esetén` or before `nevű`, the date before `időpontban`. `Be` / `Ki`.

## A karaktertartomány-szerkesztő (`multiRename.rangeEditor.*`)

- An ordinal's article goes on `elölről`/`hátulról` (`az elölről 3. karakter`, `a hátulról 2. karaktertől`), never on
  the number: CLDR's ordinal `one` (1, 5) misses `az 50.`–`az 59.`. Start/End → `Eleje`/`Vége`, not `Kezdőérték`.

## Az átvitel élő fájlfája (`fileOperations.transferTree.*`)

"X of Y" aria → `X kész ennyiből: Y` (a size can't take `-ból/-ből`). `Követés` (the viewer's tail toggle); rows reuse
queue and log words.

## A Windows- és Linux-ágak szavai (`errors.listing.*.suggestion`, `errors.write.homeFolder*`, `errors.write.*.trash`)

- Windows: `Fájlkezelő`, `Feladatkezelő`, `Erőforrás-figyelő` (ms) > `Társított leírók` (ms handle → leíró, tentative),
  `Start menü`, `Gépház > Hálózat és internet` / `Rendszer > Tárterület`, `Tulajdonságok` > `Biztonság` /
  `Eszközök > Ellenőrzés`, `Lomtár ürítése`, the quoted checkbox `„Írásvédett”` beside prose `csak olvasható`.
- PC → `a géped` / `ez a gép`, user folder → `felhasználói mappa` (tentative), `Windows-fiók`, `hálózati adapter`.
- Linux: `egy terminálban`, `a fájlkezelődben`, `rendszerfigyelő app`, `Jogosultságok` lap (GNOME, KDE, Xfce), unmount →
  `leválaszt`, mount → `csatol`, `GNOME Lemezek`, `a számítógéped`.
