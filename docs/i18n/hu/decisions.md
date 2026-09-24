# hu decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json` and `concepts-proposed.json`); open questions for a native reviewer live in `review-queue.md`.
Style and voice: `style.md`.

## A macOS-vs-Windows hasadások: Kuka, szerver, Mégsem, könyvjelző

Where macOS and Microsoft disagree, macOS wins, because Finder users see its word.

- **trash → `Kuka`**: macOS `Kuka` 30×, zero `Lomtár`; GNOME and Xfce agree (`Kukába dobva`, `Áthelyezés a Kukába`).
  Microsoft gives both but reserves `Lomtár` for the Windows Recycle Bin product. **move to trash →
  `Áthelyezés a Kukába`** (macOS and Xfce), the nominal label; macOS also has `Kukába helyezés`.
- **server → `szerver`**: macOS `szerver` 38× (`Kapcsolódás szerverre…`); Microsoft terminology and GNOME/Xfce say
  `kiszolgáló`.
- **cancel → `Mégsem`**: macOS `Mégsem` 52×, zero `Mégse`; Microsoft, GNOME, Xfce, TC, and DC say `Mégse`. Never
  `Visszavonás` (undo).
- **bookmark → `könyvjelző`** (tentative): macOS `Kedvenc` names the Favorites sidebar and Microsoft says `kedvenc`,
  while GNOME's explicit bookmark action is `könyvjelző`. Cmdr's favorites list stays `kedvenc`; no shipped key says
  bookmark today.

## A `{verb}`/`{Verb}`/`{gerund}` helyőrzők (`errors.write.*`)

These RAW tokens are filled with **English** words at runtime ("copy", "moving", "Copy"): `transfer-error-messages.ts`'s
`operationVerbMap` is hardcoded English, not localized. A raw English verb can't take a Hungarian case suffix, so each
is wrapped in an apposition noun: `a(z) {verb} művelet` ("the {verb} operation"), `a(z) {gerund} művelet közben`, and
`A(z) {Verb} művelet …` for titles. The `a(z)` covers the unknown article of the inserted English word. The operation
verb stays English on screen until that map is localized (tracked in `review-queue.md`); the surrounding sentence is
correct Hungarian regardless.

## A Beállítások szakaszai (`settings.section.*`)

Keep these verbatim wherever another file names a Settings section: Appearance = `Megjelenés` (mac), Colors and formats
= `Színek és formátumok`, Zoom and density = `Nagyítás és sűrűség`, File and folder sizes = `Fájl- és mappaméretek`,
Listing = `Fájllista`, Behavior = `Viselkedés`, File operations = `Fájlműveletek` (ms), File system watching =
`Fájlrendszer figyelése`, Search = `Keresés` (mac), AI = `AI`, File systems = `Fájlrendszerek`, SMB/Network shares =
`SMB-/hálózati megosztások`, MTP = `MTP (Android/Kindle/kamerák)`, Git = `Git`, Viewer = `Megjelenítő`, Developer =
`Fejlesztői`, MCP server = `MCP-szerver`, Logging = `Naplózás`, Updates & privacy = `Frissítések és adatvédelem`,
Advanced = `Speciális` (mac/ms), Keyboard shortcuts = `Billentyűparancsok` (mac), License = `Licenc`, Navigation & file
ops = `Navigáció és fájlműveletek`, Drive indexing = `Meghajtó indexelése`, Image indexing = `Képek indexelése` (the two
sibling sections share the possessive shape).

- A Settings path in a sentence never takes a suffix: `itt: Beállítások > AI`,
  `itt: Beállítások › Frissítések és adatvédelem`. The separator glyph (`>` or `›`) mirrors English per key.
- `settings.section.updatesAndPrivacy` and the crashReporter / whatsNew mentions all say
  `Beállítások > Frissítések és adatvédelem`.
- Setting option values: Wilting (date colors) = `Hervadás`, Smart (sizes) = `Okos`, Personal (license tier) =
  `Személyes (ingyenes)`.

## A kötetválasztó csoportcímei (`fileExplorer.navigation.group*`)

Favorites = `Kedvencek`, Volumes = `Kötetek`, Cloud = `Felhő`, Mobile = `Mobil`, Network = `Hálózat`. The Servers row
inside the Network group is `Szerverek`; the group stays `Hálózat`, don't collapse the two.

## Gép vagy gazdagép: a host két regisztere (`fileExplorer.network.*`, `commands.networkSelectHost.*`, `errors.listing.host*`)

The split is intentional. The network browser names an auto-discovered box `gép` (column `Gépnév`, `Hálózati gép`), as
macOS calls the manual-connect entity `szerver`; connection-failure prose says `gazdagép` (`A gazdagép nem érhető el`),
the natural full word there. `kiszolgáló` in `errors.json` is the participle hosting/serving, not the noun server, so it
doesn't break `server → szerver`.

## Git-szavak (`errors.git.*`, `fileExplorer.git.*`, `settings.fileExplorer.git.*`)

Git terms stay verbatim per the en `@key` notes: `git`, `worktree`, `commit`, `blob`, and `repó` for repo. The verbatim
`worktree` covers every key naming git's linked checkout, the git-portal ones included (`errors.git.orphanedWorktree.*`,
`settings.fileExplorer.git.showVirtualGitPortal.description`, `fileExplorer.git.size.linkedWorktrees` =
`{countText} csatolt worktree`); `munkafa` is only the generic working tree in `errors.git.bareRepo`, `blobTooLarge`,
and `gitDirPermissionDenied`, and working directory stays the separate `munkakönyvtár`. Consonant-final loanwords take
their suffix with a hyphen (`worktree-eket`). Bare repo = `csupasz repó`; git browser = `git böngésző`; dirty state =
`piszkos állapot` (tentative); the repository chip = `repozitóriumcímke`.

## Relatív időcímkék (`fileExplorer.*.ago*`)

The `{count}m/h/d/w/mo/y ago` chips keep terse Hungarian abbreviations (`{count} p`, `ó`, `n`, `h`, `hó`, `é` for perc,
óra, nap, hét, hónap, év) plus `ezelőtt`; just now = `most`. ETA lines use `{n} mp` / `{n} p` and roughly = `kb.`;
Almost done = `Mindjárt kész`.

## A kereszt-fájlos egyeztetés

Drift the per-file fan-out left behind, found in a whole-catalog pass:

- **Ellipsis: the single character `…` everywhere**, like its `„…”` quotes and native date order.
- **Quotation marks: `„…”`**, never English `"…"` (`commands.handler.favoriteAdded` = `A(z) „{name}” …`).
- **`Modified` (column/filter/chip) → `Módosítva`** uniformly; the `-va` participle is the column form.
- **`Don't show again` → `Ne jelenjen meg többé`.**
- **`Endpoint URL` → `Végpont URL-címe`; `Example:` (a placeholder lead-in) → `Példa:`**, never `Például:` (that is "for
  example"); `On disk` → `Lemezen`; `Reset all to defaults` → `Összes visszaállítása alapértékre`;
  `Go to latest download` → `Ugrás a legutóbbi letöltéshez`; `Press Enter to search` →
  `Nyomd meg az Entert a kereséshez`; `Tab limit reached` → `Elérted a lapok korlátját`; `Something went wrong` →
  `Valami nem sikerült`.
- **No update-check prefix**: a failed update check reads as whole sentences (`updates.failure.check`,
  `A Cmdr nem tudta megkeresni a frissítéseket. {reason}`), so neither `Hiba:` nor `Probléma:` appears.

## A státuszcellák és a buborékok hangja (`queue.row.status`, `operationLog.status.*`, `fileExplorer.status.*`)

- queue-row states: queued = `Várakozik`, running = `Fut`, paused = `Szüneteltetve`, done = `Kész`, cancelled =
  `Megszakítva`, failed = `Nem sikerült befejezni`. The operation log's softer Didn't finish = `Nem fejeződött be`.
- The status fallback cell and `tooltip.errorWithType` read `Probléma`, not `Hiba`: the English `@key` asks for the
  friendlier word.
- `queue.row.label` arms reuse the nominal verbs (`Másolás`, `Áthelyezés`, `Törlés`, `Áthelyezés a Kukába`, `Átnevezés`,
  `Mappa létrehozása`, `Fájl létrehozása`, `Archívum szerkesztése`); the Working fallback = `Folyamatban`.
- The Queue button that backgrounds a transfer = `Sorba` (short for Double Commander's `Várakozási sorba helyez`).

## Az átviteli ICU-többesszámok (`transfer.*`)

- The counted noun stays singular in both branches (`{count, plural, one {fájl} other {fájl}}`); the `other` branch is
  still required.
- `transfer.fileOnly.mixedMove`'s was/were select collapses: the verb is `volt` whatever the count, so only the noun
  plural remains, and the placeholder set (`{skippedText}`, `{skipped}`) is kept.
- `{verb, select, …}` opens with the nominal `Másolás` / `Áthelyezés` and uses the participles `másolva` / `áthelyezve`
  inline; the reusable `{phrase}` fragment sits after a colon (`Másolva: {phrase}.`) so it stays grammatically
  standalone.

## A dupla kattintás a panel hátterén (`settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.*`)

- The label is nominal: `Dupla kattintás a panel hátterére a szülőmappába lépéshez`; the description
  `Ez a fájllista körüli üres terület, nem pedig egy fájlsor.` reuses `fájllista` and `fájlsor`.
- Never do this again (the playful off button) = `Soha többé` (impersonal, tentative); I like it / Don't like it? =
  `Tetszik` / `Nem tetszik?`.
- The breadcrumb tooltip Click to navigate to {path} = `Kattints ide az ugráshoz: {path}`, the path after a colon.
- `szülőmappa` stays for catalog consistency (`commands.navParent.label` and six `errors.json` suggestions). macOS
  Finder says `tartalmazó mappa` (Go To Enclosing Folder); switching would take one migration of the whole catalog,
  never a piecemeal change.

## A FAT32-korlát üzenetei (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- too large for this drive → `túl nagy ehhez a meghajtóhoz` (macOS `A fájl túl nagy a célhoz`); formatted as FAT32 →
  `FAT32 formátumú`.
- larger than {maxSize} → `{maxSize}-nál nagyobb`. Suffixing a placeholder is normally forbidden, but a formatted size
  always ends in a back-vowel unit (bájt, kilobájt, megabájt, gigabájt, terabájt), so `-nál` is always right. The same
  constrained-domain reasoning allows `{percent}%-nál` (read `százaléknál`).
- and {countText} more files → `és {countText} további fájl` (macOS `és ^0 további elem`), singular in both branches.

## A célmappa még nem létezik (`fileOperations.transferDialog.targetWillBeCreatedCopy`/`…Move`)

`Ez a mappa még nem létezik. A Cmdr létrehozza a másolás során.` / `… az áthelyezés során.`: Total Commander and Double
Commander phrase a missing target as `nem létezik`, and two literal sentences follow the en `@key` (no ICU select).

## Archívumok böngészése (`settings.archives.*`, `errors.listing.archive*`, `fileOperations.delete.archiveWarning*`)

- The segmented cells are nominal: `Böngészés` / `Megnyitás` / `Rákérdezés`. The longer `Mindig kérdezzen` of
  `allowFileExtensionChanges.opt.ask` fits a wider control.
- `Konfigurálás…` over `Beállítás…`, so the menu item doesn't collide with `Beállítások`.
- There's no trash inside an archive → `Egy archívumon belül nincs Kuka.`; removed from the zip for good →
  `Ezek az elemek véglegesen törlődnek a zipből.`
- Open with default app → `Megnyitás az alapértelmezett appban`.

## A vágólap beillesztése fájlként (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile`)

- clipboard content → `vágólaptartalom`; the label `Vágólaptartalom beillesztése fájlként`; image → `kép`, text →
  `szöveg` (macOS AppKit), PDF verbatim.
- The options are nominal like the archive cells: `Nincs művelet`, `Fájl létrehozása`, `Létrehozás és átnevezés`.
- The toast `A vágólap {kind, select, image {képe} pdf {PDF-je} other {szövege}} fájlként beillesztve: {filename}`: the
  branch words carry the possessive, and the filename sits after a colon.

## Az archívum jelszava (`fileOperations.archivePassword.*`)

The retry body attaches the accusative to a head noun, `… nem oldotta fel a(z) <archive>{name}</archive> fájlt`, so no
case suffix ever lands on the runtime name.

## A Műveletnapló (`operationLog.*`, `commands.logOperationLog.*`)

- `Műveletnapló` is reused verbatim from `settings.navigationAndFileOps.card.operationLog`: the Settings card and the
  dialog name one feature.
- roll back → `visszagörgetés`, reconciled to the shipped `fileOperations.transferProgress.*` strings that surface the
  same engine. Status forms: `Visszagörgethető`, `Nem görgethető vissza`, `Visszagörgetés folyamatban`,
  `Visszagörgetve`, `Részben visszagörgetve`. `settings.operationLog.intro` says `visszavonhatod` because its English
  says undo, a different word.
- Summaries use the possessive verbal noun with a singular counted noun: `{countText} elem másolása`, `… áthelyezése`,
  `{countText} mappa létrehozása`, `Archívum szerkesztése`, `Archívum kicsomagolása`; and N more items =
  `és {countText} további elem`.
- Provenance labels: You = `Te`, Agent = `Ágens`, AI client = `AI-kliens`.

## Ask Cmdr: helyőrzők, címek, eszközsorok (`askCmdr.*`, `settings.askCmdr.*`, `commands.askCmdrToggle.*`)

- Input placeholders and the small empty-state header follow the catalog's nominal search-placeholder pattern:
  `Kérdés a fájljaidról` / `Kérdés a fájljaidról…`, `Csevegések keresése…`, `Kérdés a kijelölésről`. A standalone warm
  welcome takes the `te` imperative instead (`Beszélgess a Cmdrrel a fájljaidról`), like the onboarding headings.
- Close X → `X bezárása`; Close Ask Cmdr → `Az Ask Cmdr bezárása`.
- The tool chips pair a verbal noun (doing) with a `-va/-ve` participle (done) on the same object, so a screen reader or
  the scrollback tells the states apart. `searchPhotos` keeps `A fotóid átvizsgálása` / `… átvizsgálva`: the sibling
  `operationsList` renders the same frame that way, and `X keresése` would read as searching FOR the photos.
- Provider and model labels in `{provider}` arrive pre-localized from `settings.ai.provider.opt.*`; only the sentence
  around them is translated.
- Chat with an AI about your files, drives, and history (`commands.askCmdrToggle.description` and the first sentence of
  `settings.askCmdr.intro`, the same English) → `Csevegj egy AI-val a fájljaidról, meghajtóidról és előzményeidről`, the
  English imperative kept.
- Error copy reuses the calm building blocks: `Valami nem sikerült`, `Próbáld újra?`, `nem fejeződött be`, `korlátját`.

## Hálózati meghajtók képindexelése (`settings.mediaIndex.networkVolumes.*`, `search.imageResults.networkOff`/`.paused`)

- Mirror the English image/photo split: the section and internal labels say `kép` (`Képek indexelése`, `képindexelés`),
  the per-drive user strings `fotó` (a NAS holds photos).
- always index → `Ez a meghajtó mindig legyen indexelve`: `mindig` plus a bare verbal noun is ungrammatical, so the
  impersonal subjunctive carries it. Internal descriptions say `folyamatos indexelés`.
- `{name}` stands as a bare nominative possessor, never suffixed: `{name} fotóinak indexelése`,
  `{name} fotói mindig legyenek indexelve`.
- The drive dropping on its own → `nincs csatlakoztatva` / `megszakad a kapcsolat a meghajtóval`, never the
  user-initiated `leválasztva`. gently → `kíméletesen`; only while you're not busy →
  `csak amikor épp nem vagy elfoglalt`.

## Tömeges átnevezés, képindex-hatókör, Ask Cmdr-eszközök (`askCmdr.renameReview.*`, `settings.mediaIndex.excludedFolders.*`, `fileExplorer.imageIndex.*`)

- excluded → `kizár` / `kizárva`, never `kihagy`: `Kizárt mappák` names the same feature, and `kihagyva` is the
  transfer's skipped outcome.
- `askCmdr.renameReview.cancel` is `Mégsem` (it was the catalog's lone `Mégse`).
- `askCmdr.tool.proposeRenamePlan.done` is `Átnevezési terv előkészítve`, pairing with the doing label.
- next pass → `a következő átvizsgálás`, the `driveIndex` family's scan word.

## A képindex jelvényei (`fileExplorer.imageIndex.file.*`/`.folder.*`/`.drive.*`, `settings.mediaIndex.showFileStatusIcons.*`)

- Badge states follow Finder's file-icon badges: done = `-va/-ve` participle (`Indexelve a képkereséshez`), waiting =
  `Várakozás a(z) X-re` (`Várakozás az indexelésre`).
- `file.failed` → `Nem sikerült indexelni`, never Finder's badge word `Hiba`.
- `file.excluded` (Not included in image search, for several non-user reasons) → `Nem szerepel a képkeresésben`, never
  `kizárva`, which would over-claim a user's choice.
- `drive.ariaLabel` → `Meghajtó képkeresési állapota`, parallel to its neighbor dot `Meghajtó indexállapota`.
- Count-of-count fragments use the `x / y` slash idiom, never a suffixed count:
  `{doneText} / {totalText} kép indexelve`. Terse fragments drop `van`, the full sentence keeps it.

## A képindexelés beállításainak átrendezése (`settings.mediaIndex.*`, `fileExplorer.imageIndex.file.indexing`)

- search by description → `leírás szerinti keresés` (attributive) / `leírás alapján` (adverbial); the toggle
  `Fotók keresése leírás alapján`.
- Card titles `Indexelés bekapcsolása`, `Indexelendő mappák`; Indexing now → `Indexelés folyamatban` on both surfaces
  that share the English.
- The delete-model flow mirrors its download siblings: `Modell törlése ({size} felszabadítása)`, `Törlés…`, the question
  `Törlöd a szemantikus keresés modelljét?`; the body dodges suffixing `{size}` with the intransitive
  `Ezzel felszabadul {size}`.

## A törlés kapcsolója és a Forrás / Cél fejlécek (`fileOperations.delete.trashSwitch`/`.confirmDelete`, `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`)

- `Áthelyezés a Kukába` (switch) and `Törlés` (the confirm button while it's off).
- From / To → `Forrás` / `Cél`: Total Commander (`662`/`663`) and Double Commander ship this pair in the same dialog,
  and `Cél` matches the group's own controls (`Célkötet`, `Célútvonal`). The deictic `Innen` / `Ide` only works inside a
  verb phrase and dangles as a heading; `Innen:` stays the inline label before a scan-phase source path.

## A meghajtó indexelésének kikapcsolása (`fileExplorer.navigation.driveIndex.*IndexingOff*`, `settings.indexing.masterOffNote`/`.overriddenBadge`)

- drive indexing (the feature) → `(a) meghajtó indexelése`, never `meghajtó-indexelés`: the possessive phrase ships 6×
  as the feature's name, and Microsoft writes index compounds solid, so a compound would be `meghajtóindexelés`.
- The full form names the global switch (and a `Indexelés > Meghajtó indexelése` path quotes it); the bare `indexelés`
  is the anaphoric short form in per-drive controls.
- stays unindexed → `továbbra sem lesz indexelve` (Total Commander ships `nincs indexelve`).
- picks up where it left off → `ott folytatja majd, ahol abbahagyta`: `ott … ahol` is the correlative pair (`onnan`
  pairs with `ahonnan`).
- `settings.indexing.overriddenBadge` → `Az indexeléssel együtt ki`: 25 characters, and `együtt` forces the comitative
  reading.

## Meghajtóindex: a változásellenőrző futás (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalesced*`)

- **"Checking for changes" (run-kind header) → `Változások ellenőrzése`** · deverbal-noun phrase matching the sibling
  headers (`Első teljes átvizsgálás`, `Gyors frissítés`); `ellenőrzése` is macOS HU's checking noun (Finder BN9 „^0”
  tartalmának ellenőrzése), `változások` is catalog-settled (`Legutóbbi változások pótlása`) · high.
- **"Update the file list" → `Fájllista frissítése`** · composed from the settled siblings `Fájllista mentése` +
  `Index frissítése` · high.
- **"the check running right now" → `az éppen futó átvizsgálás`** · reuses `átvizsgálás` as this catalog's settled word
  for a full check (`tooltipCoalesced`: "a Cmdr következő teljes átvizsgálása") and that string's closing
  `ezt rendbe hozza` · high.

## A megtorpant átvitel értesítése (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- **"No progress for {duration}" → `{duration} óta nincs előrehaladás`** · `előrehaladás` is the pile's progress noun
  (macOS Finder `1.title` = `Előrehaladás paraméterei`, Xfce Thunar "File operation progress" =
  `Fájlművelet előrehaladása`) and is already what this catalog calls it (`sizeProgressAria` = `Méret-előrehaladás`,
  `fileProgressAria` = `Fájl-előrehaladás`) · high. The `{placeholder} óta` shape is pile-attested (Nautilus "Since %s"
  = `%s óta`) and keeps the placeholder UNSUFFIXED, which the style guide requires: `{duration}` renders as the
  un-localized `45s` / `2m 30s` / `1h 5m` (`$lib/units/duration.ts` formats digits + Latin unit letters, no locale
  branch), so no `-e`/`-ja` adverbial suffix could vowel-harmonize with it. Duration-first word order also mirrors the
  line it replaces (`etaRemaining` = `~{duration} van hátra`). Residual `tentative` point: `óta` most often takes a
  point in time; with a measured span it's idiomatic in the plural (`hetek óta`) and reads fine with an abbreviated
  value, but a native reviewer may prefer `{duration} alatt nem történt előrehaladás` (unambiguously a span, longer, and
  slightly past-tense). One key carries the sentence, with no final period, and it renders on both the progress dialog
  and the narrow queue-row cell, so the Hungarian has to fit the narrow one.
- **"Waiting for X to respond" → `Várakozás a X válaszára`** (destination = `Várakozás a cél válaszára.`, source =
  `Várakozás a forrás válaszára.`) · Total Commander `1384="Adatküldés, várakozás a válaszra..."` is the exact
  waiting-for-a-response phrase, and the `Várakozás a …-ra/-re` frame is macOS Tier 1 (AppKit "Waiting for disc drive…"
  = `Várakozás a lemezmeghajtóra…`; Finder `Várakozás a feltöltésre`, `Várakozás a letöltésre`,
  `Várakozás „^0” általi fogadásra…`), plus Double Commander (`Várakozás a fájlforrás elérésére`,
  `Várakozás felhasználói válaszra`) · high. The two sides are named with the dialog's OWN group headings `Cél` /
  `Forrás` (settled 2026-07-23 from TC `662/663` and DC), so the notice and the boxes it explains use one word each;
  don't fork to `célhely`/`céleszköz` here. `nem reagál` (macOS AppKit's "did not respond") is the negative,
  fault-flavored form and is deliberately NOT used: the notice states what Cmdr is doing (waiting), not that something
  is broken.
- **"The transfer has stopped moving" → `Az átvitel megállt`** · no source names a stalled-but-alive transfer (mining
  gotcha 3's shape: the concept is absent from every corpus; macOS HU has zero `megállt`/`leállt` hits, Microsoft
  terminology has no `stall`/`stalled`/`unresponsive` entry), so this is composed from settled `transfer → átvitel` plus
  the plain intransitive `megáll` · tentative. NOT `leállt` (reads as "shut down / ended", and the transfer is still
  alive), NOT MS's `leállítás` (that's the deliberate "stop" command), and NOT a second `nincs előrehaladás`, which
  would just repeat the line above it.
- **"Cancel it, or leave it running in the background." → `Szakítsd meg, vagy hagyd futni a háttérben.`** · reuses the
  settled running-op `cancel → megszakítás` (imperative `szakítsd meg`, informal `te` per Formality, as in the settled
  `próbáld újra`) and the settled `Hagyd futni a háttérben` (from `queueTooltip`) verbatim · high. Comma before the
  clause-joining `vagy` is correct and pile-attested (Nautilus/Dolphin:
  `Nevezze át a szimbolikus hivatkozást, vagy nyomja meg a Kihagyás gombot.`).
- **"{N} file(s) is/are still open" → `{count, plural, one {# fájl van még nyitva} other {# fájl van még nyitva}}`** ·
  Total Commander `616="Túl sok fájl van nyitva."` gives both the term and the `… fájl van nyitva` word order; `még`
  carries the English "still" · high. Both branches identical (Hungarian no-pluralize-after-a-numeral rule, as in
  `queuedToastCount`/`selectedCount`), and the counted noun keeps the singular verb, so the trailing clause stays
  singular too.
- **"and may already be partly written" → `és lehet, hogy már részben ki van írva`** · `kiír` is this catalog's verb for
  writing bytes out (`transferProgress.titleFlushing` = `Az utolsó darab kiírása…`), and the stative `ki van írva`
  avoids both the bureaucratic `kiírásra került` and a `-tuk/-tük` first-person that would put words in Cmdr's mouth ·
  high. Kept OUTSIDE the plural braces, as in English.
- **"The log has the details." → `A részleteket megtalálod a naplóban.`** · reuses settled `log → napló` (`naplófájl`,
  `Naplózás`; TC `5390="Napló fájl"`, DC `Naplófájl megtekintése`) and the catalog's own "you'll find it there" shape
  (`backgroundedToast` = `Megtalálod a műveleti sorban.`), which reads warmer than the literal
  `A napló tartalmazza a részleteket.` · high.
- **`transferProgress.close` (dismiss the dialog while the transfer finishes) → `Bezárás`** · the catalog-wide,
  macOS-sourced Close (`ui.modalDialog.close`, `fileOperations.errorDialog.close`, and 8 more) · high. It sits next to
  `fileOperations.button.cancel` = `Mégsem`, so the pair is unambiguous: `Bezárás` closes the window, `Mégsem` stops the
  operation.

## Másolt útvonal: a vágólap-visszajelzés (`fileExplorer.clipboard.copiedPath`)

Egy kulcs: a ⌘⌥C utáni információs toast szövege. Maga az útvonal alatta, külön, fix szélességű sorban jelenik meg,
tehát NEM helyőrző a mondatban: a mondat kettősponttal zárul, és önmagában is állnia kell.

- **"Copied the path, it's now on your clipboard:" → `Útvonal másolva, most már a vágólapon van:`** · a bevett
  `clipboard → vágólap` és `path → útvonal` (`Ugrás útvonalra`) szótári döntéseket használja · high. Az `-va/-ve`
  határozói igenév a testvér toastok mintája (`{countText} elem másolva`). Birtokos rag nélkül (`a vágólapon`, nem
  `a vágólapodon`): egy vágólap van, a macOS is névelővel mondja.

## A műveleti sor: a Transfer queue átnevezése (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

The window is named "Operation queue": it lists deletes, trashes, renames, folder/file creations, and archive edits too,
and "transfer" already means copy-or-move one level down (the transfer dialog, the transfer driver). So the Hungarian
head noun is the wide one:

- **operation queue (the window, the View-menu item, the command) → `Műveleti sor`** · the widened name covers the whole
  window; `transfer → átvitel` is untouched by it and stays correct for the copy/move dialog · high. Built from two
  settled parts: the head noun `művelet` (below) and the catalog's settled `queue → sor` (Double Commander `New queue` =
  `Új sor`, `Put first in queue` = `Első helyre tétele a sorban`). The `<activity>-i sor` shape is Tier-2 attested
  (Microsoft `print queue` = `nyomtatási sor`), and the adjectival `műveleti` + head-noun formation is
  Double-Commander-attested (`operations panel` = `műveleti panel`).
  - **NOT the solid compound `Műveletsor`**, even though it would look more parallel to `Műveletnapló`: Microsoft
    terminology already assigns `műveletsor` to `task flow` (id 2335491) and `visszaállítási műveletsor` to
    `restore sequence` (id 2225865) — that is, a SEQUENCE of steps, not a waiting line. The compound would name the
    wrong concept.
  - NOT Microsoft's generic `queue` = `várakozási sor` either: the catalog settled the file-manager-native `sor` in June
    and `várakozási sor` is long for a window title.
  - Inflects regularly (back-vowel `sor`): illative `a műveleti sorba` (`transferProgress.queueAria`), inessive
    `a műveleti sorban` (`queueTooltip`, `queuedToast`, `backgroundedToast`). **Watch the article**: `műveleti` starts
    with a consonant, so every one of those sites takes `a`, never `az`.
- **operation (the category word: a copy, move, delete, trash, rename, folder/file creation, or archive edit) →
  `művelet`** · macOS Tier 1 throughout (`Művelet` as a bare label; "A művelet nem hajtható végre.", "Ez a művelet nem
  vonható vissza.", `Gyorsműveletek`), Microsoft terminology (`operation` = `művelet`, two entries), Double Commander
  (`Current operation:` = `Aktuális művelet:`, `Executing operations` = `Műveletek végrehajtása`, `File operations` =
  `Fájlműveletek`) · high. **Matches the shipped Operation log window** (`Műveletnapló`) and the settled
  `action/operation → művelet` (`Fájlműveletek`), so the deliberate English View-menu pair "Operation queue" /
  "Operation log" survives as `Műveleti sor` / `Műveletnapló`. Do NOT fork the head noun: two different words in two
  neighbouring menu items would be the defect. Inflects front-vowel: dative `a műveletnek`, accusative `a műveletet`,
  plural `Műveletek`.
- Row screen-reader labels keep their nominal shape and only swap the noun: `Ennek a műveletnek a szüneteltetése` /
  `… a folytatása` / `… a megszakítása` / `… a kijelölése`. The heading and the list aria (`Operations`) are the bare
  plural `Műveletek`.
- `commands.queueShow.label` is the bare window title `Műveleti sor`, matching the sibling
  `commands.logOperationLog.label` = `Műveletnapló`, which is also bare.
- Counted-noun plural keeps the singular in both branches, as always: `queuedToastCount` =
  `{count, plural, one {# művelet} other {# művelet}}`.
- `queue.empty.title` = `A sor üres` (bare anaphoric `sor`) and `transferProgress.titleCancellingSlow` =
  `Megszakítás… (USB-átvitelek befejezése)` (a real transfer, not the queue) keep their own words.

## A sarokchip és a befejezetlen műveletek értesítése (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

Two surfaces: a ~80 px chip in the main window's top-right corner previewing the operation running in the background (a
verb, a bar, a hover tooltip, and a stopped-early state), and a never-auto-dismissing toast for an operation that
couldn't finish, with a matching row + Dismiss button in the queue window. The window's name, the head noun `művelet`,
and their inflection come from the section above.

- **dismiss (stop showing a notice or a finished-with row) → `Elvetés`** (button), `Mindet elveti` (toolbar),
  `Ennek a műveletnek az elvetése` (row aria) · the shipped `hu` catalog is decisive: `Elvetés` already renders every
  `Dismiss` in the app (`crashReporter.dialog.dismiss`, `lowDiskSpace.toast.closeTooltip`, `downloads.empty.dismiss`,
  `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`, `errorReporter.bundleSavedToast.dismiss`,
  `fileOperations.mkdir.timeoutDismiss`), and `ui.toast.dismissAria` = `Értesítés elvetése` is this exact
  dismiss-a-notification sense · high (seven shipped sites plus a matching aria is stronger evidence than the pile).
  Deliberately NOT the pile term: macOS has one hit only (AppKit TouchBar "Dismiss Popover" = `Előugró üzenet bezárása`)
  and Microsoft terminology gives `bezárás` / `leállítás` for the notification sense, but `Bezárás` is the catalog's
  settled `Close` (`ui.modalDialog.close` and 10 more), so adopting it would make the queue row's Dismiss
  indistinguishable from a window's Close.
  - `Mindet elveti` takes the finite-verb shape its two neighbours settled (`Mindet szünetelteti`, `Mindet folytatja`),
    not the nominal shape of `Kijelöltek megszakítása`; English's "Dismiss all" is parallel to "Pause all"/"Resume all",
    so the Hungarian is too.
  - The row aria joins the `Ennek a műveletnek a …` family (`… a szüneteltetése` / `… a folytatása` / `… a megszakítása`
    / `… a kijelölése`) and is the only member taking `az`: `elvetése` is vowel-initial.
- **"couldn't finish <action>" (the failure toast's nine `select` arms) → `Nem sikerült befejezni` + the action in the
  accusative** · built from the `queue.row.status` `failed` arm (`Nem sikerült befejezni`) so the toast and the queue
  row can't word the same event differently, with each action's noun taken verbatim from `queue.row.label` /
  `operationLog.summary.*` and put in the accusative: `a másolást`, `az áthelyezést`, `a törlést`,
  `az áthelyezést a Kukába`, `az átnevezést`, `a mappa létrehozását`, `a fájl létrehozását`,
  `az archívum szerkesztését`; the `other` arm is the bare row wording · high. Verb-first in every arm (not the
  topic-first `A másolást nem sikerült befejezni`) so the nine read as one family and the `other` arm is a clean
  truncation of the rest, exactly as in English. The `nem sikerült` calm-voice rule keeps `hiba` / `sikertelen` out.
- **"{n} operations couldn't finish" (summary toast + chip) → `{countText} műveletet nem sikerült befejezni`** · the
  same house wording with the count as its accusative object, which is how Hungarian expresses this impersonally · high.
  Counted noun stays SINGULAR in both plural branches (`műveletet`, never `műveleteket`), as in `queuedToastCount` /
  `selectedCount`.
- **"Show in operation queue" (failure-toast button) → `Megjelenítés a műveleti sorban`** · the catalog's own
  `Show in <place>` shape, shipped twice as `Megjelenítés a Finderben` (`commands.fileShowInFinder.mac.label`,
  `errorReporter.bundleSavedToast.reveal`); inessive `a műveleti sorban` per the rename block · high.
- **"Open the operation queue [to see why]" (the chip's promise, `chip.ariaLabel` + `chip.failed`) →
  `Nyisd meg a műveleti sort` / `Nyisd meg a műveleti sort, hogy megtudd, miért.`** · informal `te` imperative, matching
  the catalog's other second-person instructions (`próbáld újra`, `Szakítsd meg, vagy hagyd futni a háttérben.`,
  `Kattints ide az ugráshoz: {path}`); accusative `a műveleti sort` · high. The imperative is deliberate over a
  declarative "the reason is in the queue": the chip IS the button, so the sentence has to name the action pressing it
  performs.
- **"percent" spelled as a word (screen-reader label) → `százalék`, unsuffixed** · Hungarian screen readers expand `%`
  to `százalék` anyway, so spelling it out costs nothing and removes the dependency; `{percentText} százalék` needs no
  case suffix, which also keeps the placeholder unsuffixed (per `style.md` § Notes and decisions) · high. NOTE the split
  with the chip TOOLTIP, which is read by eye and keeps the glyph: Hungarian sets `%` tight against the number with NO
  space (`42%`), unlike de/fr/sv.
- **item (a file-or-folder in a count) → `elem`** · macOS Finder Tier 1 throughout (`Kuka elemei`,
  `Másolni kívánt elemek`, `Nincsenek törölni kívánt elemek.`) and already the catalog's counted-item noun
  (`fileExplorer.clipboard.copied` = `{countText} … elem másolva`, `operationLog.summary.*` = `{countText} elem …`,
  `operationLog.dialog.moreItems`) · high. Singular in both plural branches.

### `queue.chip.tooltip`: the dot-separated fact line

The hardest string in the batch. English continues a phrase before switching to middle dots
(`Copying 214 items to Backup · 42% · 1m 20s left`); Hungarian can't, because `{label}` arrives as a verbal NOUN
(`Másolás`, from `queue.row.label`), and `Másolás 214 elem` is ungrammatical. So **every optional clause carries its own
`·` inside its branch** and the whole line is one flat fact list:

`{label}{count, plural, =0 {} one { · {countText} elem} other { · {countText} elem}}{hasDestination, select, yes { · ide: {destination}} other {}} · {percentText}%{hasDetail, select, yes { · {detail}} other {}}`

- The flat shape is in-catalog precedent, not an invention: `ai.toast.progress` =
  `{percentText} · {downloaded} / {total} · {speed}/s{eta, select, none {} other { · {eta} van hátra}}` is the same
  Hungarian dot-separated progress line with the same optional-clause-carries-its-own-separator discipline.
- **destination → ` · ide: {destination}`**, never a case suffix on the folder name · macOS Tier 1 ships exactly this
  action-then-`ide:` shape (`Áthelyezés ide: %@` 78×, `„^0” letöltése ide: „^1”`, `Tömörítés ide: „^0”`,
  `… másolása és áthelyezése ide: ${destination}`), and the verb it wants is already at the head of the line · high. A
  runtime folder name can't take a harmonizing suffix (`Backupba`/`Backupbe`), which `style.md` forbids (§ Notes and
  decisions). Runner up was the settled transfer-dialog heading `Cél:`; `ide:` wins because it reads as a continuation
  of the leading verb instead of a form label.
- `{detail}` arrives ALREADY in Hungarian and must not be re-derived here: `OperationChip.svelte` fills it from
  `fileOperations.transferProgress.etaRemaining` (= `{duration} van hátra`) or from the `queue.row.status` `paused` arm
  (`Szüneteltetve`). So the time-left phrasing for this surface is inherited, not chosen.
- Assembled and read for all four combinations, all clean (no double space, no dangling `·`): `Másolás · 42%` ·
  `Másolás · ide: Backup · 42%` · `Másolás · 3 elem · 42%` · `Másolás · 3 elem · ide: Backup · 42%`, each optionally
  plus ` · 1m 20s van hátra`.
- The `=0 {}` and `other {}` empty arms stay empty, and `{label}` keeps NO leading separator: it is the only
  always-present part before `{percentText}`.

If the failure toast's longest arm wraps badly in its ~360 px toast (tracked in `review-queue.md`), the fix is the
shorter macOS variant `a Kukába helyezést` (attested alongside `Áthelyezés a Kukába`), not a new failure wording.

## Az önálló ütközési kérdés (`fileOperations.operationConflict.*`)

The prompt is hosted by the main window when a backgrounded operation hits a name clash, so a context line names which
operation is asking and a quiet note explains why the rest of the queue stopped.

- **`operationConflict.context`: the four `hasDestination: yes` arms keep the settled `queue.row.label` verbal nouns and
  add the destination as a deictic colon clause, never as a case suffix** · `Másolás ide: {destination}` /
  `Áthelyezés ide: {destination}` · macOS Finder Tier 1 (`Áthelyezés ide: %@`, plus the menu items `Másolás ide`,
  `Áthelyezés ide…`) and Nautilus (`Fájlok másolása ide: „%s”…`, `„%s” másolása ide: „%s”`) both ship this exact
  verb-then-`ide:` shape, and it is already the catalog's own rendering in `queue.chip.tooltip`
  (` · ide: {destination}`) · high. Unquoted, matching English and the chip tooltip; macOS quotes the name in this
  shape, but mixing quoted and unquoted arms inside one select would be the worse defect.
- **`ide:` (illative, "to") vs `itt:` (locative, "in") tracks English's own preposition split across the arms** ·
  copy/move say "to {destination}" so they take `ide:`; the `other` arm says "Working **in** {destination}" (work
  happening inside a folder, not items going into one) so it takes `Folyamatban itt: {destination}`, the catalog's
  settled `itt: {placeholder}` neutral slot (`errors.*` `nem található itt: {hostName}`,
  `Bármikor visszavonhatod itt: {systemSettings}`) on top of the sibling's `Folyamatban` · high.
- **An uncontrolled placeholder can sit in the POSSESSOR slot of a possessive verbal-noun phrase, which needs no suffix
  on it at all**: "Editing {destination}" (the archive itself) → `{destination} szerkesztése` · the possessor is
  unmarked in Hungarian, so only the head noun inflects (`-e`), and the pile ships the shape with a runtime name in that
  slot (macOS `„^0” másolása szüneteltetve lett`, Nautilus `„%s” másolása ide: „%s”`; Thunar/TC `Fájlnév szerkesztése`,
  `Eszközsor fájl szerkesztése`) · high. This is a third suffix-dodge alongside the postposition and the `itt:`/`ide:`
  colon slot already recorded in `style.md` § Notes and decisions, and the only one that keeps the value in subject
  position. No article, since `a`/`az` would have to agree with an unknown first sound. The generic `other`-branch arm
  stays the sibling's `Archívum szerkesztése` ("Editing an archive"), so the English yes/other distinction survives.
- **`operationConflict.pausedNote` "Everything else is paused until you answer." →
  `Minden más szüneteltetve van, amíg nem válaszolsz.`** · `szüneteltetve` is lifted verbatim from the
  `queue.row.status` `paused` arm so the prompt and the queue rows word one state with one word (macOS confirms it in
  running prose: `A(z) „^0” másolása szüneteltetve lett`); the trailing `amíg nem …` clause is macOS Tier 1
  (`Tartsa csatlakoztatva az eszközt, amíg a törlés be nem fejeződik.`) and needs no `addig` correlative; informal `te`
  (`válaszolsz`) per Formality; `minden más` is already the catalog's phrase (`zárj be minden más appot`) · high.
  Deliberately the stative `szüneteltetve van` over the intransitive `szünetel`: the latter is correct Hungarian but
  would show the user a different word than the row they are being told about.

## A progress-párbeszéd gombja üres sornál (`fileOperations.transferProgress.background`, `.backgroundAria`)

Same button as `.queue` = `Sorba`, worded for an EMPTY operation queue: with nothing to queue behind, it names what
pressing it does instead.

- **"Background" (the button, a verb: put this transfer in the background) → `Háttérben`** · Total Commander hu is a
  direct hit on THIS control: `4004="Háttérben"` sits in the copy-dialog button strip right next to `4005="Sorba állít"`
  (Queue) and `4002="Mégse"`, so the pile ships the exact two-state pair Cmdr mirrors, and the catalog's `Sorba` is
  already the short form of TC's `4005`. Double Commander agrees on the form (`Háttérben futtatás` = "Work in
  background", `Ha az alkalmazás a háttérben fut`), as does Microsoft (`background task` = `háttérben futó feladat`) ·
  high. No macOS tier for this sense: Finder has no such control and `hu/macOS/` holds `háttér` only in the backdrop
  sense (`Háttérkép`, `háttérszín`), so the Tier-1 tiebreak is absent (mining gotcha 2), not missing.
  - **NOT the illative `Háttérbe`**, even though it would look more parallel to `Sorba`: Hungarian puts work INTO a
    queue (`sorba állít`) but runs it IN the background (`háttérben futtat`), the illative has ZERO attestation across
    the whole `hu` pile (only the unrelated adjective `háttérbeli`), and `háttérbe helyez`/`szorít` idiomatically means
    "sideline, deprioritize" — the opposite of the promise that the transfer keeps running.
  - NOT the bare noun `Háttér`: that's the backdrop (`Háttérszín`, `Háttérkép`). The inessive is the case-inflected,
    non-noun short form the sibling `Sorba` establishes for this button.
- **"Keep this running in the background" (`.backgroundAria`) → `Hagyd futni a háttérben`** · REUSED verbatim: this
  exact English sentence is already the first clause of the tooltip on the SAME button (`queueTooltip` =
  `Hagyd futni a háttérben, és kezeld a műveleti sorban (F2)`) and closes `transferProgress.stallUnknown`. One sentence,
  one rendering; the aria is the tooltip's opening clause, exactly as in English · high. Informal `te` imperative per
  Formality, no period (matching `queueAria`).
- **WCAG 2.5.3 containment**: the aria ends in `a háttérben`, so the visible label `Háttérben` is a whole-word substring
  of it (case-insensitively, the same bar English meets with "Background" ⊂ "…in the background"; a capital mid-sentence
  would be ungrammatical in Hungarian). Choosing the label's CASE FORM to be the one the natural aria sentence already
  uses is what makes this free — see `style.md` § Notes and decisions. The sibling pair holds the same way: `Sorba` ⊂
  `Áthelyezés a műveleti sorba`.

## A kilépési kapu (`main.quit.*`)

The backend refuses to quit silently while a copy, move, delete, trash, or archive edit is running, so a modal asks
whether to go ahead, lists the running operations, and counts 15 seconds down to an automatic quit.

- **quit → `kilépés`; the button "Quit now" → `Kilépés most`** · macOS Tier 1 throughout (`Kilépés`,
  `Kilépés a Finderből`, `Kilépés mindenképp`, `Kilépés és az ablakok megtartása`), Microsoft terminology (`quit` =
  `kilépés`, `Exit` = `Kilépés`), Double Commander (`E&xit` = `Kilépé&s`, `E&xit program` = `Kilépés a &programból`),
  Total Commander (`Alt+F4 Kilépés`) · high. The `<Noun> most` shape carrying "now" is macOS Tier 1 as well
  (`Biztonsági mentés most`, `Letöltés most`), and it keeps English's load-bearing "now": the app quits either way, this
  button skips the wait. NOT macOS's `Kilépés mindenképp` ("Quit anyway"): that answers a refusal, while Cmdr's dialog
  is a countdown the button short-circuits.
- **"an operation is running" (the state, in running prose) → `folyamatban van`; the heading "Still running" →
  `Még folyamatban`** · macOS Finder Tier 1 ships this exact surface, a quit blocked by unfinished file operations:
  `A Finder nem képes kilépni, mert néhány művelet még folyamatban van.` (plus
  `… mert egy művelet még folyamatban van egy iOS-eszközön.` and
  `… mert egy másik művelet van folyamatban, mint például egy elem mozgatása vagy másolása`), and the verbless heading
  form is macOS-attested too (`Első biztonsági mentés folyamatban`); Microsoft terminology agrees (`in progress` =
  `folyamatban`) · high.
  - **NOTE the register split with `queue.row.status` running = `Fut`**, which stays as it is: `Fut` is a one-word
    status cell in a table column, `folyamatban van` is the running-prose form, and macOS uses exactly this pair of
    registers itself. Same shape as the settled `host` split (`gép`/`Gépnév` in the browser column vs `gazdagép` in
    error prose). Don't "unify" them.
  - The heading is deliberately verbless: `Még fut` would be singular while the list holds 1..N rows, `Még futnak`
    breaks at one row, and `Futó műveletek` would repeat the noun the title just used (English avoids that with the
    terse "Still running").
- **The title is a second-person question: `Kilépsz, amíg egy művelet folyamatban van?`** · every question-shaped title
  in the shipped `hu` catalog uses informal `te` per `style.md` § Formality (`Törlöd az AI-modellt?`,
  `Elküldöd az összeomlási jelentést?`, `Megváltoztatod a fájlkiterjesztést?`, `Mindenképp bezárod?`) · high. Counted
  noun singular in BOTH plural branches (`{countText} művelet folyamatban van`, never `műveletek`), and the verb stays
  singular with it, per the no-pluralize-after-a-numeral rule.
- **"Keep working" (the button that calls the quit off) → `Munka folytatása`** · no source names this control (mining
  gotcha 3: the concept is absent from macOS, Microsoft, and all five file managers, none of which offers a
  stay-in-the-app button on a quit countdown), so it is composed from macOS's own Tier-1 `<Noun> folytatása` shape
  (`Biztonsági mentés folytatása`, `Másolás folytatása`) plus the nominal-label rule · tentative (`review-queue.md`).
  - Deliberately NOT `Mégsem`, the catalog's settled dialog Cancel: next to a list of running operations it would read
    as "cancel the operations", the exact opposite of what the button does.
  - Deliberately NOT `Később` (the settled dismiss-for-now word) or any "remind me" wording: the countdown is deleted,
    not deferred.
  - Residual risk a reviewer should judge: `Folytatás` alone is the queue's Resume, so `Munka folytatása` could be read
    for a moment as resuming an operation. The object `munka` (the user's work, not an operation) is what separates
    them, and English carries the same overlap ("Keep working" vs "Resume").
- **"in {n} seconds" → `{secondsText} másodperc múlva`** · the postposition `múlva` is the only correct Hungarian for
  this and needs no suffix on the placeholder (per `style.md` § Notes and decisions); zero pile attestation, since no
  corpus counts a quit down · high on the grammar, and `másodperc` itself is macOS Tier 1 (`Kb. ^0 másodperc`). Singular
  `másodperc` in both plural branches.
- **restart / logout (the operating system's, not Cmdr's) → `újraindítás` / `kijelentkezés`** · macOS Tier 1
  (`Újraindítás`, `Kijelentkezés`, `Kijelentkezés…`) and Microsoft terminology (`restart` = `újraindít`,
  `log off`/`log out`/`sign out` = `kijelentkezik`, `Sign Out` = `Kijelentkezés`) · high. Lowercase mid-sentence, as
  Hungarian sentence case requires.
- **The countdown's "so … never waits on Cmdr" → `így egy újraindítás vagy kijelentkezés soha nem vár a Cmdrre.`** ·
  indicative `így …` rather than a subjunctive `hogy … ne …`, matching English's plain "so … never waits" and reading
  lighter · high. The brand suffix `Cmdrre` follows `style.md`'s hyphen-free, front-vowel pattern (`Cmdrben`, `Cmdrből`,
  `Cmdrnek` in the shipped catalog), with the `r` doubled by the sublative `-re`.
- **"what it leaves half-written" → `minden félig megírt fájlt`** · `félig megírt fájl` is lifted verbatim from the
  shipped catalog, where the identical English phrase already renders this way
  (`settings.advanced.showStagingTempFiles.description` = `… nem hagyhat félig megírt fájlt valódi néven`) · high.
  `minden` + singular is the Hungarian generic, which keeps the settled phrase while staying number-neutral (see below).
  **"clears away" → `eltávolít`**, the catalog's and Microsoft's `remove` = `eltávolítás`, chosen over `törli`: the
  sibling `transferProgress.rollbackTooltip` uses `törlése` for the same cleanup, but that is a destructive-action
  button label, while this sentence is reassurance and must not flash "Cmdr deletes a file" at the reader · high.
- **"anything still being written" → `Ami éppen íródik`** · **the body must stay number-neutral**: one operation writes
  several files at once and several operations can run at once, so a definite singular (`Az éppen írás alatt álló elem`)
  states something false · high. `íródik` over the participial `írás alatt álló` only to keep `áll` out of a clause that
  already ends in `ott áll meg`. "stops where it is" → `ott áll meg, ahol tart`.
- **"Whatever's finished stays done." → `Ami elkészült, az kész marad.`** · reuses the settled `Done` = `Kész`
  (`queue.row.status` done arm) · high.
- **`countdownAria` → `Hátralévő idő a Cmdr automatikus kilépéséig`** · nominal, like every other aria label in the
  catalog (`Ennek a műveletnek a szüneteltetése`); `hátralévő idő` is the Microsoft-attested shape (`remaining duration`
  = `hátralévő időtartam`, `remaining work` = `hátralévő munka`), and `automatikus` carries "on its own" · high. **Not
  bound by WCAG 2.5.3**: it names a countdown REGION, not a control with a visible label, so there is no label to
  contain (the visible text is the countdown sentence itself).

## Usage stats: "névtelen" dropped, "egy véletlenszerű azonosító" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `álnevesített` / `pszeudonim` — that
jargon is exactly what the copy avoids.

- **usage stats → `használati statisztika`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `névtelen` adjective was cut. MS terminology's `használati adatok` is the data sense; the statistics reading fits the
  UI · high
- **a random id → `egy véletlenszerű azonosító`** · MS terminology (random → `véletlenszerű`, identifier → `azonosító`)
  · high. `azonosító` is ordinary Hungarian, not jargon.
- **tied to → `-hoz/-hez/-höz kötődik`** · takes the harmonized case suffix on the noun it attaches to
  (`az azonosítóhoz kötődik`, `a nevedhez kötődik`); no placeholder is involved, so the suffix is safe here · high
- **`Mac` + case suffix takes a hyphen: `Mac-eden`** · the written final `c` doesn't spell the pronounced /k/, so AkH's
  hyphen rule applies. Already what `onboarding.stepBeta.emailNote` ships; `settings.updates.emailPrivacyNote` now
  matches it (its old `a Macedre tárolva` was both unhyphenated and in the wrong case for `tárol`) · high

## Visszagörgetés-megerősítő + a válaszra váró sor állapota (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

A futó másolás/áthelyezés `Visszagörgetés` gombja most megerősítést kér, a műveleti sor pedig külön állapotot mutat, ha
egy sor azért állt meg, mert a főablakban kérdés vár a felhasználóra.

- **"Needs your answer" (sorállapot) → `Válaszolnod kell`** · a pile közvetlen találata a fogalomra a Double Commander
  műveletnézete (`Waiting for user response` = `Várakozás felhasználói válaszra`), de az en `@key` kiköti, hogy ez az
  állapot NE legyen összetéveszthető a `queued` ággal (`Várakozik`), és a DC alakja pont a `vár-` tővel kezdődik · high
  a `válasz` tőre, `tentative` a formára. A `Válaszolnod kell` a `te`-regisztert használja (style.md § Formality), ahogy
  az angol is közvetlenül szólítja meg a felhasználót ("your"), és két szó a szűk oszlopban.
  - ❌ NEM `Válaszra vár`: idiomatikus, de a `vár` miatt egy pillantásra a `Várakozik` ággal mosódik össze.
  - ❌ NEM `Választ kér`: a `választ` egyben a `választ` ige alakja is, tehát homográf-félreolvasás kockázata.
- **`awaitingAnswerTooltip` → `Válaszolj a kérdésre a főablakban, és ez a művelet folytatódik.`** · a `válaszol` ige a
  testvér `operationConflict.pausedNote`-ból jön (`amíg nem válaszolsz`), a `főablak` a szótár szava
  (`queue.row.foregroundAria` = `Megjelenítés a főablakban`); a "prompt" itt `kérdés`, mert a megerősítő szövegek is
  ezzel a szóval beszélnek róla · high.
- **`rollbackConfirm.title` → `Visszagörgeted ezt a műveletet?`** · a katalógus minden kérdés-címe `te`-alakú, definit
  ragozással (`Törlöd az AI-modellt?`, `Megváltoztatod a fájlkiterjesztést?`) · high. A `visszagörget` a szótár szava.
- **`rollbackConfirm.body` → `Ez törli az összes fájlt, amit a művelet eddig kiírt. Amit felülírt, az nem jön vissza.`**
  · a "written" a katalógus `ki van írva` alakja (`transferProgress.stallInFlight`), a "so far" mindenütt `eddig`
  (`queryUi.results.live.matchesSoFar`, `search.imageResults.paused`), a `felülír` a szótár `overwrite` szava, macOS
  Tier 1 (`Felülírás a célhelyen`) · high. A második mondat szabad vonatkozói szerkezet (`Amit felülírt, az …`), hogy
  szám-semleges maradjon: az angol "any file" sem egy konkrét fájlról beszél.
- **`rollbackConfirm.keep` ("Keep them", a biztonságos válasz) → `Fájlok megtartása`** · macOS Tier 1 a
  `<Főnév> megtartása` alakra (AppKit `Keep` = `Megtartás`, `Mindkettő megtartása`, `Az összes megtartása`,
  `Letöltött megtartása`) · high. A puszta `Megtartás` azért nem elég: a törzsszöveg utolsó mondata a FELÜLÍRT fájlokat
  nevezi meg, így a tárgy kimondása nélkül egy pillanatra rossz tárgyra vonatkozhatna.
- **`rollbackConfirm.rollBack` → `Visszagörgetés`** · szó szerint az a gomb, amelyik a párbeszédet nyitotta
  (`transferProgress.conflictRollback`); az en `@key` kifejezetten kéri az egyezést · high.
- **`transferProgress.rollbackTooltip` (új angol: "Stop, and delete every file written so far") →
  `Leállítás, és minden eddig kiírt fájl törlése`** · a `Leállítás` macOS Tier 1 az abbahagyásra (`Másolás leállítása`,
  `Kettőzés leállítása`, `Leállítás…`), és a katalógus is ezt használja (`queryUi` `Keresés leállítása`) · high.
  Szándékosan NEM `Megszakítás`: az a futó művelet Cancel-szava, az en `@key` viszont pont azt köti ki, hogy a tooltip
  ne olvasódjon sima Cancelként.
- **`transferProgress.foregroundBusyToast` (új angol: "Something else is open here. Close it, then bring this one up.")
  → `Itt valami más van nyitva. Zárd be, aztán hozd elő ezt.`** · az új angol szándékosan nem állítja, hogy a blokkoló
  egy másik MŰVELET (lehet Új mappa vagy törlés-megerősítés is), így a korábbi `Egy másik művelet …` kezdet hamis lett;
  az `itt` a most előtérbe hozott főablak · high. Informal `te` a két felszólító alakban.

## Az átnevezés-láncban nevüket megtartó fájlok számláló buboréka (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

A soron belüli átnevezőben a fel/le nyíllal végigfutó átnevezésekből több is elmaradt. Ugyanaz az EGY buborék, mint a
`chainKeptOriginalName`, csak újraírva: megnevezi a legutóbbi fájlt, a korábbiakat pedig megszámolja.

- **A testvérkulcs első tagmondata szó szerint marad**: `A(z) „{name}” megtartotta a nevét`. A buborék a helyén íródik
  újra a felhasználó szeme előtt, ezért csak a farka nőhet, ahogy az angolban is. Az idézőjel `„…”` (style.md), a
  `megtartotta a nevét` a testvérkulcs igéje.
- **"and so did {N} other files" → `és még {othersText} másik fájl is.`** · a `„^1” és ^0 másik elem` alak macOS Tier 1
  (Finder AirDrop: `„^1” és ^0 másik elem fogadása.` / `… küldése.`, valamint az egyesítés-párbeszéd
  `a(z) „^1” és ^0 további elem`), tehát az `és {szám} másik {főnév}` szerkezet és a szám utáni EGYES SZÁMÚ főnév is
  attesztált · high. A záró `is` (az azonos állítmány elhagyása, magyar gapping: „Péter megtartotta a nevét, és Anna
  is.”) pont az angol "and so did" megfelelője, ezért nem kell megismételni az igét · a NYELVTANRA high, a FORMÁRA
  tentative: a pile rövid címkékből és hibaüzenetekből áll, egyetlen ` is.`-re végződő mondat sincs benne, tehát erre a
  záró alakra nincs korpuszfedezet.
  - ❌ NEM összevont alany (`A(z) „{name}” és még {N} másik fájl megtartotta a nevét.`): rövidebb ugyan, de az en `@key`
    kiköti, hogy a `{reason}` CSAK a megnevezett fájlra vonatkozik, az egy alannyá olvasztás pedig az egész csoportra
    vinné át az indoklást.
  - ❌ NEM `és ugyanez történt még {N} másik fájllal`: nyelvtanilag rendben van, de a pile-ban a `történt` szinte
    kizárólag a `Hiba történt …` / `… hiba történt` fordulatban él (macOS AppKit 8 találatból 6, Nautilus, Thunar,
    Double Commander), és a buborék hangja szándékosan kerüli a hiba-regisztert (style.md § Voice and tone).
- **Az `one` ág kiírja a számnevet: `egy másik fájl`**, ahogy az angol is ("one other file") és a
  `settings.mediaIndex.reclaim.line` mintája; az `other` ág a `{othersText}` formázott értéket használja. A főnév
  MINDKÉT ágban egyes számú (`másik fájl`, soha `másik fájlok`) a szám utáni nem-többesítés szabálya szerint · high.

## A meg nem erősített átnevezés buboréka és a fel nem használható név (`fileExplorer.rename.unconfirmed`/`unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

Lassú köteten (hálózati megosztás, telefon) az átnevezésre nem érkezik visszajelzés időben. A buborék NEM állíthatja,
hogy a fájl megtartotta a nevét: azt mondja, hogy a Cmdr nem tudja. Ez a `chainKeptOriginalName*` pár testvére, de a
JELENTÉSE ellentétes, ezért a nyitó tagmondat szándékosan más.

- **"Couldn't confirm X" → `Nem sikerült megerősíteni, hogy …`** · a katalógusban EZ a család bevett nyitánya ugyanerre
  a meg-nem-erősített-művelet helyzetre: `fileExplorer.pane.trashUnconfirmedToast`
  (`Nem sikerült megerősíteni, hogy a fájl a Kukába került.`) és `fileOperations.mkdir.timeoutMessage`
  (`Nem sikerült megerősíteni, hogy a mappa létrejött.`) · high.
  - ❌ NEM tárgyas `Nem sikerült megerősíteni a(z) „X” átnevezését`: a pile-ban a `megerősítés` tárgyas alakja kizárólag
    a jóváhagyás-értelmet viszi (Double Commander `Felülírások megerősítése`, `megerősítés kérése nélkül`; Nautilus
    `Jelszó megerősítése`; macOS AppKit `Confirm` = `Megerősítés`), a `hogy`-os mellékmondat viszont egyértelműen az
    ellenőrzés-értelem.
- **"the rename of X" → `a(z) „X” átneveződött` (mediopasszív)** · a testvér buborék pontosan ezt az alakot használja a
  meg nem erősített műveletre (`trashUnconfirmedToast`: `a fájl így is áthelyeződhetett`), és az `-ódik/-ődik`
  mediopasszív a pile-ban is a gépi ágens nélküli állítás alakja (Nautilus, Thunar, Dolphin: `törlődnek`, `másolódnak`,
  `mentődnek`) · high a szerkezetre, `tentative` a szótőre: konkrétan `átneveződ*` alak sem a pile-ban, sem a
  katalógusban nincs.
  - ❌ NEM `átnevezték` (a katalógus 3. sz. többes határozatlan alakja, pl. `errors.write.sourceNotFound.suggestion`):
    az KÜLSŐ ágenst jelöl (valaki a Cmdren kívül nevezte át), itt viszont maga a Cmdr nevezett át.
- **"The volume may be slow" → `Lehet, hogy a kötet lassú`** · szó szerint a két testvér buborék második mondata
  (`trashUnconfirmedToast`, `mkdir.timeoutMessage`); `kötet` a szótár szava (macOS Tier 1), a `lassú` melléknévre
  Nautilus (`A keresés lassú lehet, …`) és Double Commander (`(lassú)`, `(lassabb)`) a fedezet · high.
- **"the rename may still have gone through" → `így az átnevezés attól még sikerülhetett`** (többesben
  `az átnevezések attól még sikerülhettek`) · az `attól még` + `-hat/-het` potenciális alak szó szerint a
  `mkdir.timeoutMessage` mintája (`így a mappa attól még létrejöhetett`) · high a szerkezetre, `tentative` a
  `sikerülhetett` alakra: a `sikerül` ige a pile-ban 88-szor, a katalógusban 100-szor szerepel, de szinte kizárólag a
  tagadó `nem sikerült` fordulatban, erre az állító potenciális alakra nincs korpuszfedezet.
  - **Az alanyt KI KELL mondani** (`az átnevezés`), pro-drop itt hibás: a második mondat élén `a kötet` az utolsó
    alanyeset, így a `így attól még átneveződhetett` egy pillanatra „a kötet neveződhetett át” olvasatot ad. A két
    testvér pontosan ezért ismétli meg a maga alanyát (`a mappa`, `a fájl`).
  - Az alany `az átnevezés`, nem `a fájl`: az `unconfirmed` kulcs alatt MAPPA is állhat, tehát a testvérek főneve itt
    hamis állítás lenne. Az angol is ezt a főnevet nevezi meg ("the rename(s)").
  - ❌ NEM `megtörténhetett`: a pile-ban a `történt` szinte kizárólag a `Hiba történt …` fordulatban él (lásd a fenti
    `chainKeptOriginalNameAndOthers` blokkot), a buborék hangja pedig kerüli a hiba-regisztert.
- **Az `AndOthers` számnév-ágai szó szerint a `chainKeptOriginalNameAndOthers`-éi** (`egy másik fájl` /
  `{othersText} másik fájl`), hogy a két buborékpár egy hangon szóljon. Az összetett alany után az első tagmondat egyes
  számú állítmányt kap (`átneveződött`, a magyar alapeset számnévi tag után), a második viszont többeset
  (`átneveződhettek`), hogy a lehetőség az EGÉSZ csoportra vonatkozzon, ne csak az utolsó tagra · high.
- **"That filename can't be used" → `A fájlnév nem használható` / `A mappa neve nem használható`** · a `nem használható`
  névre alkalmazva macOS Tier 1 (Finder `A(z) „^0” név nem használható.`, `… mert túl hosszú.`,
  `… mert a rendszer számára van fenntartva.`; AppKit Document `A(z) „%@” név nem használható.`) · high. A főnév a
  testvérkulcsokéval azonos (`validation.empty`, `.disallowedChars`, `.nameTooLong`: `A fájlnév` / `A mappa neve`). Záró
  pont nincs: az érték hosszabb mondatba épül be (`{reason}. A(z) „{name}” megtartotta a nevét.`).
  - Az angol `That` mutató névmása elmarad: a `mappanév` összetétel a pile EGYIK forrásában sem szerepel (a `fájlnév`
    60+ találattal igen), az `Ez a mappa neve …` pedig félreolvasható „ennek a mappának a neve” értelemben. A magyar
    határozott névelő amúgy is a beírt névre mutat.

## Javasolt műveletek: az Ask Cmdr javaslatainak ablaka (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (az ügynök által javasolt fájlműveletek) → `műveletek`; a cím `Javasolt műveletek` · a katalógus `fájlművelet`
  szóhasználatához igazítva · high
- approve → `Jóváhagyás` · ms; a macOS `Elfogadás` az AirDrop-elfogadás párja, itt viszont engedélyezésről van szó ·
  high
- reject → `Elutasítás` · macOS Finder AirDrop-panel (Tier 1) · high
- "This can't be undone" → `Ezt nem lehet visszavonni` · macOS Finder ("Ezt a műveletet nem vonhatja vissza"),
  tegező-semleges alakra hozva, mert a Cmdr tegez · high
- pattern → `minta` · már a katalógusban (`queryUi.json`) · high
- undo → `visszavonás` · már a katalógusban (`askCmdr.renameUndo`) · high

## Megkettőzés: a parancs, amely ugyanabba a mappába másol (`commands.fileDuplicate.*`)

- **duplicate (a kijelölést a saját mappájába másoló parancs) → `Megkettőzés`** · macOS Finder `hu`, „Fájl >
  Megkettőzés” (`N154`), valamint „Elemek megkettőzése” és „Megkettőzi az elemeket a jelenlegi helyükön” (ellenőrizve
  macOS 26.6.1 alatt, `Finder.app/Contents/Resources/hu.lproj`, 2026-08-19) · high. A névszói alak illeszkedik a
  `Másolás` / `Áthelyezés` / `Átnevezés` sorhoz, és egyikkel sem keveredik.
- **„Make a copy of the selected files in the same folder” →
  `Másolat készítése a kijelölt fájlokról ugyanabban a mappában`** · a szomszédos leírások névszói alakja („Kijelölt
  fájlok másolása…”); a `másolatot készít valamiről` vonzat a természetes magyar szerkezet, az „ugyanabban a mappában”
  pedig arra a mappára utal, amelyben a fájlok már benne vannak · high.

## Natív menük: menüsor, helyi menük, ablakcímek (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

A csoport forrásai: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/hu.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) a Tier 1, és szinte mindent eldönt; az angol oldal az `en_GB.lproj`-ban van, mert a
`Base.lproj` csak lefordított nibeket tartalmaz. A Safari 26 (`MainMenu.strings`) adja a lapokra vonatkozó
szóhasználatot, a Microsoft-terminológia azt, aminek az Apple-nél nincs neve. RAW család: **egyszeres aposztróf**, a
`''` a menüben kettőnek látszana.

- **Menüsor: `Fájl`, `Szerkesztés`, `Nézet`, `Ugrás`, `Ablak`, `Súgó`, `Szolgáltatások`** · macOS Finder és Safari `hu`
  · high.
- **pane → `panel`, immár `high` (eddig `tentative`)** · Double Commander `hu` („Bal panel”, „Jobb panel”) és Total
  Commander `hu` („a célpanelben”, `WCMD.INC` 911) · high. Az ortodox kétpaneles pár a Cmdr közvetlen rokona, tehát ez a
  megfelelő családból származó bizonyíték; a `style.md` nyitott kérdései közül ez lezárult.
- **Select menü (fájlkijelölés) → `Kijelölés`** · Nautilus `hu` („Kijelölés”), és illeszkedik az `Összes kijelölése`
  sorhoz · high.
- **Deselect all → `Kijelölés törlése`** · macOS Finder `hu` (`300488.title`) · high. A Finder szóhasználata (a Tier-3
  `megszüntetése` alak kerülendő); a `Fájlok kijelölésének törlése…` ennek a párja.
- **Go > Home → `Saját`** · macOS Finder `hu` (`253.title`) · high. Rövid, és pontosan ezt látja a felhasználó a
  Finderben.
- **Window > Zoom → `Méretezés`, Minimize → `Minimalizálás`** · macOS Finder `hu` · high.
- **zoom in / out → `Felnagyítás` / `Lekicsinyítés`** · Safari `hu` (Nézet menü) · high. Így a `Nagyítás` szabadon marad
  a zoom-almenü címének, és nem ütközik a saját elemével.
- **Quick Look → `Gyorsnézet`** · macOS Finder (`TL14`) · high. Az Apple lefordítja ezt a funkciónevet, ezért nincs a
  ne-fordítsd listán.
- **ascending / descending → `Növekvő` / `Csökkenő`** · Thunar + Dolphin `hu` · high.
- **changelog → `Módosítási napló`** · Microsoft-terminológia · high. Elkülönül a Súgó > `Újdonságok` elemtől: az egyik
  a dokumentumot nevezi meg, a másik a hírt.
- **word wrap → `Sortörés`** · Microsoft-terminológia · high.
- **pin / unpin tab → `Lap rögzítése` / `Lap rögzítésének feloldása`** · Safari `hu` („Lap rögzítése”) · high.
- **„Edit in editor” → `Megnyitás szerkesztésre`** · leíró · tentative. A szó szerinti „Szerkesztés szerkesztőben”
  ismétlődik, mert magyarul az `edit` és az `editor` ugyanabból a tőből jön; a megnyitás-szerkesztésre szerkezet
  idiomatikus, és megkülönböztethető a fölötte álló `Megtekintés`-től.
- **„Don't index images in this folder” / „Index images here again” → `Képek indexelésének tiltása itt` /
  `Képek indexelésének engedélyezése itt`** · névszói címkealak, a magyar UI-konvenció szerint · tentative. A tiltó
  felszólító mód (`Ne indexeld…`) tegező közvetlen megszólítás lenne, amit a címkéknél a `style.md` kerül.
- **Finder-címkeszínek → `Piros, Narancs, Sárga, Zöld, Kék, Bíbor, Szürke`** · macOS Finder (`TG_COLOR_*`) · high.
- **Címkesor (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Címkék`, `„{color}” hozzáadása`,
  `„{color}” eltávolítása`** · macOS Finder (`TG5` = `„^0” hozzáadása`, `TG6` = `„^0” eltávolítása`, `N169.37` =
  `Címkék`) · high. A színnév változatlanul kerül be, ezért áll idézőjelben az igenév előtt, toldalék nélkül; ugyanaz a
  szópár, mint a `commands.tagsToggleRed.description` („Hozzáadja vagy eltávolítja”) mondatban.
- **busy (használatban lévő kötet) → `(foglalt)`** · Microsoft-terminológia (`foglalt` = vonal foglalt) · high.
- **Eject → `Kiadás`, Disconnect → `Leválasztás`, Remove (listából) → `Eltávolítás`** · macOS Finder · high.
- **A márkanév toldalékolása: `Kilépés a cmdrből`** · a `style.md` kötőjel nélküli, kiejtés szerinti szabálya
  („commander” → elöl képzett magánhangzók → `-ből`) · high. Az `en` érték szándékosan kisbetűs `cmdr`, ezért a magyar
  is az marad.
- **Szándékosan azonos az angollal** (`sameAsSourceJustification`): `menu.zoom.percent*` és `menu.view.askCmdr`.

## busy: a kiszürkített menüpontok jelölője (`menu.volume.ejectBusy`, `menu.volume.disconnectBusy`, `menu.volume.forgetServerBusy`, `menu.volume.forgetSavedPasswordBusy`)

A kiszürkített („busy”) menüpont az alapcímke + ` (foglalt)`: az alapváltozat szövege betűre változatlan marad, és csak
a záró ` (foglalt)` kerül a végére. A `(foglalt)` névszói állapotjelző (Microsoft-terminológia: `foglalt` = a vonal
foglalt), ezért bármelyik címke után áll, akár ige, akár főnévi szerkezet az alap. Egyetlen jelölő van; új „busy” kulcs
ne találjon ki másikat.

## Rendszerkapcsolatra visszaeső SMB-buborék (`fileExplorer.network.osMountFallback.*`)

Három kulcs: a buborék szövege, a benne lévő gomb és a bezáró X elemleírása. Akkor jelenik meg, amikor a Cmdr saját,
gyorsabb kapcsolata nem jött létre, és a megosztás a macOS kapcsolatán fut. A hangnem megnyugtató, nem riasztó: a
megosztás működik, csak lassabb.

- **native (a macOS saját SMB-kapcsolata) → `natív`** · Microsoft-terminológia (Tier 2, `native` = `natív`, melléknév,
  HUN; `natív mód`, `natív formátum`, `natív fájl`) · high. A macOS `hu` sehol nem használja a szót (az Apple kerüli),
  tehát nincs Tier-1 ellenbizonyíték. A `beépített` NEM ennek a szava: az a `built-in` fordítása, és így is szállítjuk
  (`settings.mediaIndex.privacyNote` „Apple beépített Vision keretrendszerével”).
- **Ugyanezt a kapcsolatot a katalógus máshol `rendszerkapcsolat`-nak hívja** (`smbNativeNote` ×2, a négy
  `pane.directConnection*Toast`), mert ott az angol is „system connection”. Itt az angol kifejezetten megnevezi a
  macOS-t és az SMB-t, ezért a hosszabb, névadó alak marad. Ugyanaz a dolog, két angol megnevezés.
- **„Couldn't directly connect to X” → `Nem sikerült közvetlenül csatlakozni ehhez: X`** · a katalógus saját, szállított
  szerkezete (`network.share.connectFailedTitle` = „Nem sikerült csatlakozni ehhez: {hostName}”,
  `navigation.driveIndex.refusedUpgradeFailed` = „A Cmdr nem tudott közvetlenül csatlakozni ehhez: {name}…”) · high. A
  kettőspontos hely azért kell, mert a `{share}` értéke ismeretlen, tehát nem toldalékolható (`style.md` §
  Agglutination). A `<shareName>` címke a kettőspont utáni névre kerül.
- **„Try connecting directly” (gomb) → `Próbálkozás közvetlen kapcsolattal`** · névszói címkealak (`style.md` §
  Formality), és a fejszava a szállított `navigation.connectDirectly` = „Közvetlen kapcsolat a gyorsabb hozzáférésért”
  szókincse · high a szókincsre, `tentative` a gombalakra. A puszta `Közvetlen kapcsolat` rövidebb lenne, de elveszne
  belőle az angol „Try” óvatossága (a közvetlen kapcsolat egyszer már nem jött létre). Hossz: 34 karakter az angol 24
  helyett, szűk buborékban ez az egyetlen túlcsordulási kockázat a három kulcs közül.
- **Dismiss → `Elvetés`** · VÁLTOZATLANUL átvéve a `lowDiskSpace.toast.closeTooltip` értékéből (azonos `sourceHash`,
  `48845bf`), és ez a katalógus hét helyén szállított `Dismiss` · high.
- **„4x slower … (sometimes 100x)” → `négyszer … (néha százszor is)`**: a szorzószám magyarul kiírva, számjegy nélkül.
  Indoklás és forrás: `style.md` § Notes and decisions, szorzószámok.
- **„for most connections” → `a legtöbb esetben`** (nem `a legtöbb kapcsolatnál`): a mondatban már három `kapcsolat`
  szerepel (`kapcsolatán`, `kapcsolatánál`, plus a gomb), a negyedik magyarul zsúfolt. A jelentés ugyanaz.
- **A `fileOperations.transferDialog.smbNativeNote` a menüpont valódi nevét idézi**:
  `„Közvetlen kapcsolat a gyorsabb hozzáférésért”` (`fileExplorer.navigation.connectDirectly`). Az angol rövidítve idézi
  („Connect directly”), de a felhasználó a kötetválasztóban a teljes címkét látja, és azt kell megtalálnia.

## Átnevezés/létrehozás elutasításai: a `errors.mutation.*` és `errors.volume.*` egysoros üzenetek

31 kulcs: a név mező alatt vagy egy rövid buborékban megjelenő EGY mondat, amikor egy átnevezés, Új mappa vagy Új fájl
nem megy át. RAW család (nincs ICU), tehát egyszeres aposztróf, és a `{path}` szó szerint marad. A `{path}` értéke
ismeretlen futásidejű útvonal, ezért mindenhol a katalógus bevett kettőspontos, toldalék nélküli helyére kerül
(`itt: „{path}”`, `ehhez: „{path}”`), vagy `A(z) „{path}”` alanyi helyre; idézőjel mindig `„…”` (style.md).

- **locked (a macOS zárolás-jelzője) → `zárolva van`; feloldása → `Oldd fel a zárolását`** · macOS Finder Tier 1 (`PE13`
  = „A művelet nem hajtható végre, mert a(z) „^0” elem zárolva van.”, `NE17` ugyanez fájlra, `AXNODE1` = `Zárolva`, a
  jelölőnégyzet `Zárolt`) · high. A `feloldás` a szótár szava (`archivePassword.unlock`).
- **Get Info (a Finder infóablaka) → `Infó megjelenítése`** · macOS Finder Tier 1 (`N165`, `TL22`, és futó szövegben
  `NE18`/`BN43`: „Válassza a Fájl > Infó megjelenítése parancsot…”) · high. Ez a szótárban már szerepel a
  `commands.json` passzból; itt az ABLAK megnevezéseként használjuk: `a Finder Infó megjelenítése ablakában`. ⚠️ A
  katalógusban két régebbi érték (`errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.*.suggestion`)
  még az angol „Get Info” alakot írja; egy külön passzban érdemes egységesíteni.
- **System Integrity Protection → `Rendszerintegritás-védelem`** · macOS Finder Tier 1 (`ET6` = „Néhány elem nem
  törölhető a Kukában a Rendszerintegritás-védelem miatt.”) · high. Az Apple lefordítja ezt a funkciónevet, ezért nem a
  ne-fordítsd listára tartozik. A mondat a `védelem alatt áll` bevett magyar vonzatot használja
  (`Ez az elem a macOS Rendszerintegritás-védelme alatt áll…`), így a `véd-` tő nem ismétlődik közvetlenül egymás után.
- **„can't be renamed” → `nem nevezhető át`** · macOS Finder Tier 1 (`RN33`, `RN37` = „A(z) „^0” elem nem nevezhető
  át.”, `RN11`) · high.
- **„isn't available any more” (kötet) → `már nem érhető el`** · macOS Finder Tier 1 (`NE7` = „…mert a(z) „^0” lemez nem
  érhető el többé.”) · high. A `már nem` az idiomatikusabb sorrend ugyanarra a tőre.
- **„didn't answer in time” → `nem válaszolt időben`** · macOS AppKit Tier 1 az `időben` határozóra („…nem fejeződött be
  időben”), a `válasz` tő pedig Total Commander (`Adatküldés, várakozás a válaszra…`) · high. Szándékosan NEM
  `nem reagál`: az a hibás-működés regisztere (lásd a megtorpant átvitel blokkját).
- **„no room left” → `nincs több szabad hely`** · a szállított `errors.listing.storageFull.explanation` („Nincs elég
  szabad hely ezen a köteten…”) és a macOS `PE18` (`a lemez megtelt`) ugyanezt a fogalmat nevezi meg · high. A
  katalógusbeli alakot folytatjuk, hogy a két üzenet egy hangon szóljon.
- **„That password didn't work.” → `Ez a jelszó nem jó.`** · a jelszót minősíti, nem a felhasználót; macOS Tier 1 a
  `helytelen` alakot hozza (`PE77`), a TC/DC pedig a `hibás`/`ROSSZ JELSZÓ` alakot · high a jelentésre, `tentative` a
  formára. Az angol szándékosan a lágyabb „didn't work”-öt választja a „is incorrect” helyett, ezért a magyar sem a
  hivatalos `helytelen`; a `hibás` pedig a `hiba` tő miatt esik ki (style.md § Voice and tone).
- **„The destination can't hold that name.” → `Ez a név nem használható a célhelyen.`** · macOS Finder Tier 1 a névre
  alkalmazott `nem használható` alakra (`A(z) „^0” név nem használható.`), és pontosan ezt használja a testvér
  `fileOperations.validation.nameNotUsable` is (`A fájlnév nem használható`) · high. A `célhely` a szótár szava. Így a
  javítás iránya (másik név, nem újrapróbálkozás) egyértelmű marad.
- **`errors.mutation.timedOut` NEM kudarcként fogalmaz**:
  `A kötet még nem válaszolt, így a módosítás attól még végbemehet.` · szó szerint a
  `fileOperations.mkdir.timeoutMessage` `attól még …-hat` mintája („így a mappa attól még létrejöhetett”) · high. Az
  `attól még` viszi az angol „may still land” engedékeny jelentését.
- **`errors.volume.deviceSessionReset` NEM kihúzásról szól**:
  `Az eszköz újraindította a kapcsolatot. Várj néhány másodpercet, majd próbáld újra.` · a második mondat szó szerint a
  szállított `errors.listing.deviceReconnecting.suggestion` („Várj néhány másodpercet, majd próbáld újra.”), amely
  ugyanezt az MTP-munkamenet-újraindulást magyarázza · high. Az eszköz csatlakoztatva marad, ezért
  `leválasztás`/`kihúzás` szó nem szerepel benne.
- **`errors.volume.deviceDisconnected` (a kapcsolat magától szakad meg) → `Megszakadt a kapcsolat az eszközzel, …`** · a
  szótár `megszakad a kapcsolat a meghajtóval` döntése (hálózati meghajtó lecsatlakozása); a felhasználó által kezdett
  `leválasztás` itt hamis lenne · high.
- **„archive edit” → `az archívum szerkesztése`** · a szállított `queue.row.label` `archive_edit` ága
  (`Archívum szerkesztése`) · high. Kisbetűs `zip` formátumnév kötőjeles összetételben: `zip-archívumok` (szótár,
  helyesírás).
- **„Move it instead.” → `Helyezd át inkább.`** · a tegező felszólító a katalógus más utasításainak alakja
  (`próbáld újra`, `Szakítsd meg…`), és az igető ugyanaz, mint az `Áthelyezés` parancsé, tehát a felhasználó tudja,
  melyik parancsra utal · high.
- **„Something went wrong, and Cmdr couldn't tell what.” →
  `Valami nem sikerült, és a Cmdr nem tudta megállapítani, hogy mi.`** · a `Something went wrong → Valami nem sikerült`
  a katalógus egyeztetett alakja (2026-06-21 passz), a `megállapít` pedig a szótár „work out” igéje · high.
- **„Cmdr stopped this at your request.” → `A Cmdr a kérésedre leállította ezt a műveletet.`** · semleges, nem
  mentegetőző; a `leállít` a macOS Tier-1 abbahagyás-igéje (`Másolás leállítása`), a `művelet` a szótár szava · high. A
  futó átvitel `Megszakítás` gombjától szándékosan eltér: itt nem gombfeliratról van szó.

Két további kulcs ugyanebbe a családba (Kukába helyezés elutasításai):

- **Trash (a macOS kukája) → `Kuka`, nagybetűvel, a funkció neveként** · macOS Finder/AppKit Tier 1 (`Trash` = `Kuka`,
  „Moves items to the Trash” = „Elemeket helyez át a Kukába”), és a katalógus már settled alakja (`delete.trashSwitch` =
  `Áthelyezés a Kukába`, `errors.write.*.trash` ág) · high. A Windows-os `Lomtár` itt nem jön szóba (macOS-app).
- **„This volume has no Trash, so the only way is to delete permanently.” →
  `Ezen a köteten nincs Kuka, ezért csak a végleges törlés marad.`** · a `kötet` a szótár szava; az `ezen a köteten`
  helyhatározós alak a szállított `errors.volume.storageFull` mintája („Nincs több szabad hely ezen a köteten.”), a
  `végleges törlés` pedig szó szerint a parancs neve (`commands.fileDeletePermanently.label` = `Végleges törlés`), így a
  felhasználó tudja, melyik parancsot keresse · high. A `csak … marad` viszi az angol „the only way is” jelentését
  anélkül, hogy kudarcnak nevezné a helyzetet.
- **„macOS wouldn't move this to the Trash.” → `A macOS nem engedte ezt a Kukába helyezni.`** · a „wouldn't” elutasítás,
  nem kudarc, ezért NEM `nem sikerült` (a style.md tiltja a `hiba`/`sikertelen` címkét); a `nem engedte …` az AppKit
  jogosultsági mondatainak igéjéhez („nincs jogosultsága a fájlt a kukába helyezni”) áll a legközelebb, de általánosabb
  · high. Szándékosan rövid: a technikai ok külön, a „Technikai részletek” alatt jelenik meg. Az `A macOS` névelője a
  kiejtés szerinti („makOS”), ahogy a katalógus máshol is (`errors.mutation.sipProtected`).

## Ha a Cmdr nem állt le: az összeomlásjelentő két új nyitómondata (`crashReporter.dialog.body.keptRunning`/`.unknown`)

Az összeomlásjelentő párbeszéd nyitómondata már nem egyetlen fix mondat: a jelentés maga rögzíti, hogy a Cmdr leállt-e.
A `.ended` (`váratlanul bezárult`) változatlan; ez a két új kulcs viszont olyan esetet ír le, amelyben a Cmdr NEM állt
le, tehát egyikük sem állíthatja, hogy leállt.

- **„ran into a problem” → `problémába ütközött`** · a `-ba/-be ütközik` szerkezet macOS Finder, Tier 1:
  `A(z) „^0” hibába ütközött.` (`LocalizableMerged`, `NE105`). A főnév `probléma`, mert az Apple
  összeomlás-párbeszédének egész családja ezt használja (`CrashReporterSupport.framework/hu.lproj`:
  `A számítógépe újraindult egy probléma miatt.`, `Grafikai problémát észlelt a rendszer.`,
  `jelentést küldhet a problémáról`; ellenőrizve macOS 26.5.2 alatt, 2026-08-23), a Microsoft-terminológia is
  `probléma`, és a katalógus hangneme kerüli a puszta `hiba` szót (`error → Probléma`) · high.
  - ❌ NEM `gondba ütközött`: a `gond`/`gondba` szóra a teljes `hu` pile NULLA találatot ad (macOS, Microsoft, Nautilus,
    Thunar, Dolphin, Total Commander, Double Commander). A fenti `problem / glitch → gond · tentative` sor ezzel megdől.
  - ❌ NEM az Apple `egy probléma miatt` szerkezete: az mindig leállást jelentő főigét kíván (`újraindult`,
    `nem nyitható meg`), tehát pont azt állítaná, amit ennek a két kulcsnak tagadnia kell.
  - ❌ NEM `Probléma történt a Cmdrben`: a pile-ban a `történt` szinte kizárólag a `Hiba történt …` fordulatban él, és a
    párbeszéd hangja kerüli a hiba-regisztert.
- **„and kept running” → `és tovább futott`** · a katalógus saját, szállított alakja ugyanerre a fogalomra:
  `transferProgress.stallUnknown` „Still running in the background” = `Tovább fut a háttérben`, mellette
  `Hagyd futni a háttérben`. A pile-ban erre a jelentésre NINCS közvetlen találat (`tovább fut`, `továbbra is fut`,
  `fut tovább` mind nulla); a legközelebbi az AppKit `NSExceptionAlert` („…ha szeretné folytatni a futtatást az
  inkonzisztens állapot ellenére”), ami ugyanez a fogalom, de felhasználói döntésként. Az ige `fut` alakját a Double
  Commander is hozza (`Ha az alkalmazás a háttérben fut`) · high a szókincsre, `tentative` a múlt idejű `futott` alakra:
  arra sem a pile-ban, sem a katalógusban nincs fedezet.
- **„in the background” → `a háttérben`** · a szótár szava (`background → háttér`); a futás értelmében a kétpaneles pár
  és a Microsoft a forrás (Total Commander `Letöltés a háttérben`, Double Commander `Ha az alkalmazás a háttérben fut`,
  ms `háttérben futó feladat`). A macOS `hu` a szót KIZÁRÓLAG látvány értelemben ismeri (`Háttérkép`, `háttérszín`),
  tehát itt nincs Tier-1 döntőbíró, nem pedig hiányzik (2. bányászati csapda) · high.
- **Szórend: `A Cmdr legutóbb a háttérben problémába ütközött`** · a `.ended` testvér mintája
  (`A Cmdr legutóbb váratlanul bezárult`): `A Cmdr` + `legutóbb` + a módosító + az ige. A fókuszpozícióba (közvetlenül
  az ige elé) a `problémába` kerül, a `a háttérben` a topikmezőbe: ez a semleges magyar olvasat · high.
- **A második mondat szó szerint a `.ended`-é, az `összeomlási` jelző nélkül:
  `Itt egy jelentés a részletekkel, ami segíthet ezt kijavítani.`** · az angol is „a report”-ot mond „a crash report”
  helyett, mert semmi nem omlott össze. A diagnosztikai `jelentés` Tier 1 (`CrashReporterSupport`: `Jelentés…` gomb,
  `küldjön jelentést az Apple számára`) és Microsoft (`report` = `jelentés`) · high.
- **A két kulcson tiltott szavak** (a pile szerint ezek viszik a „leállt” jelentést): `váratlanul kilépett` (Apple
  összeomlás-párbeszéd), `váratlanul bezárult` (AppKit, és a `.ended` sajátja), `összeomlott` / `összeomlás` (ms, AppKit
  `Összeomlás` gomb). A puszta `leáll` és `kilép` a pile-ban végig szándékos megállítást jelöl, de ezek a kulcsok
  egyiket sem használják.
- **A `.unknown` a hátteret sem nevezi meg**: régi Cmdr-verzió jelentése áll mögötte, amely nem rögzítette, hogy az app
  tovább futott-e, ezért a mondatnak mindkét kimenetelre igaznak kell lennie.
- A cím ugyanezt a hasítást követi: `crashReporter.dialog.title.crash` = `Elküldöd az összeomlási jelentést?` marad,
  `.title.report` = `Elküldöd a jelentést?` az a két eset, amelyik nem állíthat összeomlást. Ugyanez a művelet a
  visszajelző pirítósnál (`sentToast.message.crash` / `.message.report`): csak az `összeomlási` jelző esik ki.

## A jelentésküldés beállításszövege már mindkét kimenetelre igaz (`settings.updates.crashReports.description`)

A kapcsoló akkor is küld jelentést, ha egy háttérbeli panic NEM zárta be az appot, tehát a súgószöveg nem szólhat csak a
váratlan bezárulásról. Minden elem a fenti összeomlásjelentő-szakaszból jön, jelen időben:

- **`ha a Cmdr váratlanul bezárul`** a `crashReporter.dialog.body.ended` igéjéből (`váratlanul bezárult`, AppKit), a
  kulcs korábbi `váratlanul kilép` alakja helyett: a két felület így ugyanazt az igét mondja ugyanarra a kimenetelre ·
  high.
- **`a háttérben problémába ütközik`** a `.keptRunning`-ból, ugyanazzal a szórenddel (topik + módosító + fókusz + ige) ·
  high. A jelen idő puszta morfológia, nem új termdöntés.
- **`egy jelentést`** az `összeomlás-` jelző nélkül, mert a mondat mindkét esetre vonatkozik · high. ❌ A CÍMKE
  (`settings.updates.crashReports.label`) marad `Összeomlás-jelentések küldése`: az a beállítás neve.
- **A második mondat a `crashReporter.dialog.privacyNote`-ból jön** (`hogy a kód melyik része ütközött a problémába`),
  az `összeomlás helye` helyett, ami csak összeomláskor volt igaz · high.

## Kiadás és leválasztás: a `errors.eject.*` buboréküzenetek

Kilenc kulcs. Mind a KETTŐSPONT UTÁNI mondat egy rövid buborékban: a burkoló vagy `fileExplorer.pane.ejectFailedToast`
(`Nem sikerült kiadni: {volumeName}: {message}`), vagy `fileExplorer.pane.disconnectFailedToast`
(`Nem sikerült a leválasztás: {message}`). RAW család (nincs ICU), tehát egyszeres aposztróf; a `hu/errors.json`
egyetlen `''` párt sem tartalmaz, ez a fájl bevett alakja. Egy-két rövid mondat, markdown nélkül.

- **„in use” (a kötetet/meghajtót valami fogja) → `használatban van`** · macOS Finder Tier 1, sok találat: `NE66` („A
  kötet nem adható ki, mert jelenleg használatban van.”), `NE31`, `NE79`, `NE80`, `PE7`, `PE19` (ellenőrizve macOS
  26.5.2, `LocalizableMerged`, 2026-08-23) · high.
- **„Close any open files and apps, then eject again.” →
  `Zárd be a nyitott fájlokat és alkalmazásokat, majd add ki újra.`** · szerkezetében szó szerint a macOS `NE52` („Az
  ezeken a lemezeken lévő néhány fájl még használatban lehet. Léptessen ki minden nyitott alkalmazást, majd próbálja
  újra.”), önözésből tegezésbe téve · high. Az Apple `léptessen ki` (= quit) helyett `zárd be`, mert az angol is a
  lágyabb „close”-t mondja, és a mondat fájlokra is vonatkozik.
- **removable (meghajtóra) → `cserélhető`** · macOS Finder Tier 1 (`KIND_FORMATTER_28_0` = `Cserélhető kötet`,
  `KIND_FORMATTER_28_1` = `Cserélhető`, `GV3` = `Cserélhető kötetek`) ÉS Microsoft (`removable drive` =
  `cserélhető meghajtó`) · high. A két forrás egyetért, ezért nincs macOS-vs-Windows hasadás. Nem `eltávolítható`: arra
  egyik forrásban sincs fedezet.
- **network share → `hálózati megosztás`** · Microsoft-terminológia Tier 2 (`network share` = `hálózati megosztás`,
  HUN), a `megosztás` tő pedig már a szótár szava · high. A macOS a fogalmat `szerverkötet`-nek hívja (`FF22.2`), de az
  a csatolt kötetet nevezi meg, nem magát a megosztást, ezért itt a Microsoft-alak a pontosabb.
- **device (telefon, tablet, kamera kábelen) → `eszköz`** · macOS Finder Tier 1 (`PE5.1` = „A művelet nem hajtható
  végre, mert az eszköz eltűnt.”, `PE92`), Double Commander („külső eszközök (például okostelefonok)”) · high.
- **„Unplug it once it's idle.” → `Húzd ki, amikor már nincs használatban.`** · az `unplug`-ra a `hu` pile-ban nincs
  közvetlen találat (a macOS `Tartsa csatlakoztatva az eszközt` a legközelebbi, ellentétes irányból), a `kihúz` viszont
  a köznyelvi alak, és a második fele a fenti Tier-1 `használatban van` tagadása · high a `használatban` részre,
  `tentative` a `Húzd ki` igére.
- **`errors.eject.timedOut` NEM kudarcként fogalmaz**:
  `A meghajtó még nem válaszolt, de a kiadás magától is befejeződhet.` · a testvér `errors.mutation.timedOut` mintája
  (`A kötet még nem válaszolt, így a módosítás attól még végbemehet.`) és a `nem válaszolt` Tier-1 töve · high. A
  `-hat/-het` viszi az angol „may still eject on its own” engedékeny jelentését; a `kiadás` főnév kerüli a
  `kiadódik`-féle kényszeredett visszaható alakot.
- **`errors.eject.unexpected` SZÁNDÉKOSAN eltér a szó szerint azonos angolú `errors.mutation.unexpected`-tól.** Angol
  mindkettőnél: „Something went wrong, and Cmdr couldn't tell what.” (azonos `sourceHash`, `0c9d9f5`).
  - `errors.mutation.unexpected` marad `Valami nem sikerült, és a Cmdr nem tudta megállapítani, hogy mi.`
  - `errors.eject.unexpected` = `A Cmdr problémába ütközött, és nem tudta megállapítani, hogy mi.`
  - **Miért**: a kiadás-buborék burkolója maga `Nem sikerült kiadni: …`-val kezdődik, tehát a settled alak közvetlen
    szóismétlést adna („Nem sikerült kiadni: Naspolya: Valami nem sikerült, és…”). A `problémába ütközött` a fenti
    összeomlásjelentő-blokkban már bizonyított Tier-1 szerkezet (`NE105` = „A(z) „^0” hibába ütközött.”), a mondat
    második fele pedig szó szerint a settled alaké, így a két kulcs továbbra is egy hangon szól · high.
- **`errors.eject.busy`**: `A Cmdr még fájlokat mozgat ezen a meghajtón. Add ki, amint ez befejeződik.` A `Cmdr` alanyos
  szerkezet a katalógus bevett alakja (22 `A Cmdr nem …` érték); az `ezen a meghajtón` helyhatározós forma kerüli a
  toldalékolt helyettesítőt, ahogy a `style.md` § Notes and decisions előírja.

## A macOS-panelnevek magyarul: `Infó megjelenítése` és `Zárolt` (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.noPermissionErrno.suggestion`, `errors.listing.permissionDenied.suggestion`)

A `errors.write.fileLocked.suggestion.mac` és a `errors.write.permissionDenied.suggestion.deleteMac` addig angolul
hagyta a két panelnevet („Get Info”, „Locked”) egyébként magyar mondatban. Az Apple MINDKETTŐT lefordítja, tehát a

1. terminológiai alapelv (fordítsd, amit az Apple fordít) szerint magyarul kell állniuk; a `BRAND_WORDS` sem tartalmazza
   egyiket sem.

- **Get Info → `Infó megjelenítése`** · macOS Finder Tier 1, közvetlen kulcs-egyeztetéssel: `Localizable`
  `"Get Info" = "Infó megjelenítése"`, továbbá `MenuBar.strings` `300801.title` (en_GB `Get Info` → hu
  `Infó megjelenítése`), `LocalizableMerged` `N165`, `TL22`, és futó szövegben `NE18`, `BN43`, `N30`, `PE14` („Válassza
  a Fájl > Infó megjelenítése parancsot…”). Ellenőrizve macOS 26.5.2 (`sw_vers`), 2026-08-23 · high.
- **Locked (a jelölőnégyzet az infóablakban) → `Zárolt`** · macOS Finder Tier 1: `InfoWindowGeneralView.strings`
  `1073.title` (en_GB `Locked` → hu `Zárolt`). Az Apple futó szövegben idézőjelbe teszi (`NE18`: „szüntesse meg a
  „Zárolt” kijelöltségét”, `NE43`, `PE14`), ezért a katalógusban is `„Zárolt”` a `style.md` idézőjel-szabálya szerint.
  Ellenőrizve macOS 26.5.2, 2026-08-23 · high.
- **A regiszter marad Cmdr-es, csak a CÍMKÉK az Apple-éi.** Az Apple mondata önöz és hivatalos („szüntesse meg a …
  kijelöltségét”); a miénk tegez és köznyelvi: `vedd ki a „Zárolt” pipát`. A menüutat az Apple sem teszi idézőjelbe, a
  jelölőnégyzet nevét igen; ezt követjük.
- **A két `errors.listing.*` kulcs is magyar** (`noPermissionErrno.suggestion`, `permissionDenied.suggestion`): a
  „válaszd a Get Info menüt, és nézd meg a Sharing & Permissions részt” helyén most
  `válaszd az Infó megjelenítése parancsot, és nézd meg a Megosztás és jogok részt` áll.
- **Sharing & Permissions → `Megosztás és jogok`** · macOS Finder Tier 1, kulcs-egyeztetéssel:
  `InfoWindowPermissionsView.strings` `6.title` (en_GB `Sharing & Permissions:` → hu `Megosztási jogok:`, ez a panel
  FEJLÉCE), futó szövegben pedig `LocalizableMerged` `N30`/`N32`/`NE43` (en „check the Sharing & Permissions section” →
  hu „kattintson a Megosztás és jogok részre”). Ellenőrizve macOS 26.5.2, 2026-08-24 · high. A futó szöveges alakot
  választjuk, mert a mi mondatunk is futó szöveg; a panel fejlécében az Apple maga is rövidít.
- **`parancsot`, nem `menüt`** · az Apple futó szövege a menüelemre `parancs`-ként hivatkozik („válassza a Fájl > Infó
  megjelenítése parancsot”); a korábbi „a Get Info menüt” tárgyilag is téves volt (menüelem, nem menü). A névelő `az`,
  mert az `Infó` magánhangzóval kezdődik.

## A Kuka-értesítés két gombja és a visszavonás családja (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Kilenc új kulcs: a Kukába helyezés után felbukkanó értesítés két gombja, a visszavonás folyamat- és eredményszövegei, és
a parancspaletta „Go to trash” parancsa.

- **undo (a gomb) → `Visszavonás`** · macOS Finder `ME13` Tier 1 (`Undo` = `Visszavonás`), és a katalógus már ezt
  szállítja ugyanerre az angol gombra (`askCmdr.renameUndo.undo`) · high. Egy szó, elfér a keskeny értesítésben.
- **put back (a művelet, amit a gomb elindít) → `visszahelyezés`** · macOS Finder `PE130_V1`/`PE130_V2` Tier 1 („^0
  items could not be put back.” = „^0 elem visszahelyezése nem sikerült.”) · high. A Finder MENÜPARANCSA `Visszatevés`
  (`N153.1`), tehát a tő közös; a mi szövegeink mondatok, nem menücímkék, ezért a mondatbeli alakot vesszük, ami
  ráadásul a szótár `move → áthelyezés` sorával is egy családba esik. NEM `visszaállítás`: azt az `askCmdr.renameUndo.*`
  a RÉGI NÉV visszaadására használja, más művelet.
- **„Go to trash” → `Ugrás a Kukába`** · macOS Finder `TL_HELP_TCAN` Tier 1 („Go to the Trash” = „Ugrás a Kukába”) ·
  high. Ugyanaz az érték a gombon és a parancspaletta címkéjén, ahogy az angolban is. A `Kuka` nagybetűs marad (settled,
  a funkció neve), ahogy a Finder is írja ebben a sorban.
- **„Putting them back...” → `Az elemek visszahelyezése…`** · főnévi folyamatalak, ahogy a fájl többi haladásjelzője
  (`transferDialog.checkingConflicts` = `Ütközések keresése…`); `elem` a settled item-szó. A `hu` katalógus ebben a
  fájlban `…`-t (U+2026) használ, nem három pontot, akkor is, ha az angol forrás `...`-ot ír.
- **„Put back N files.” → `{countText} … visszahelyezve.`** · pontosan a testvér `transfer.trash` alakja
  (`{countText} {count, plural, one {fájl} other {fájl}} áthelyezve a Kukába`), csak az igenév cserélődik. A számnév
  után a főnév EGYES SZÁMBAN marad mindkét ágban (a `style.md` § Plurals fő szabálya).
- **A részleges eredmény második fele kap egy főnevet.** Az angol forrás itt `item`-et mond, tehát a settled `elem` a
  szó: `{skippedText} {skipped, plural, one {elem} other {elem}} a Kukában maradt`. A `{skippedText}` mellett ott van a
  `{skipped}` egész számú társ is, de a magyarnak nincs rá szüksége: a számnév utáni főnév mindkét ágban egyes szám
  marad, csak az ICU kéri, hogy mindkettőt kiírjuk.
- **„Nothing to put back. …” →
  `Nincs mit visszahelyezni. Lehet, hogy ezek az elemek már a helyükön vannak, vagy a meghajtójuk nincs csatlakoztatva.`**
  · a testvér `askCmdr.renameUndo.unavailable` mondatszerkezetét viszi tovább
  (`Nincs mit visszaállítani. Lehet, hogy …, vagy a meghajtója nincs csatlakoztatva.`), csak a művelet szava más · high.
- **„This drive doesn't keep a trash.” → `Ezen a meghajtón nincs Kuka.`** · a már rögzített `Ezen a köteten nincs Kuka.`
  alak, `kötet` helyett `meghajtó`, mert az angol itt `drive`-ot mond · high. Tényközlés, nem hibaüzenet: ezért nem a
  `errors.write.trashNotSupported.message` („nem támogatja”) regisztere.
- **A parancs leírása → `Az aktuális meghajtó Kukájának megnyitása`** · főnévi leírásforma, ahogy a `commands.json`
  többi leírása (`Másolat készítése a kijelölt fájlokról ugyanabban a mappában`); az `aktuális` a katalógus szava a
  „current”-re (`commands.editPaste.description`, `commands.favoritesAdd.description`) · high.

## A már elküldött jelentés kiegészítése (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

Tizenegy új kulcs: a Cmdr magától elküldi a hibajelentést, az értesítés gombja pedig megnyit egy ablakot, ahol a
felhasználó megnézheti, mi ment el, és megjegyzést fűzhet UGYANAHHOZ a jelentéshez (nincs második feltöltés).

- **add to X → `Hozzáadás a(z) X-hoz/-hez`** · macOS Tier 1 (`Hozzáadás a Kedvencekhez`, `Hozzáadás a Dockhoz`,
  `Hozzáadás az oldalsávhoz`), Microsoft-terminológia (`add` = `Hozzáadás`) · high. Innen `amend.title` =
  `Hozzáadás a hibajelentésedhez` és `amend.submit` = `Hozzáadás a jelentéshez`. A címke/gomb párost szándékosan ugyanaz
  a viszony köti össze, mint a küldő ablak `dialog.title` (`Hibajelentés küldése`) és `dialog.send` (`Jelentés küldése`)
  párját: a CÍM a teljes `hibajelentés` szót viszi, a GOMB a rövid `jelentés`-t.
- **note (a felhasználó szabad szövege) → `megjegyzés`** · Microsoft-terminológia (`comment` = „A note or annotation
  that an author or reviewer adds to a document” = `megjegyzés`; a `note` szócikk hu oldalán is szerepel a
  `megjegyzés`), Double Commander (`Fájl/mappa megjegyzés`, `Megjegyzés szerk&esztése...`) · high. Megerősíti a már
  szállított `errorReporter.dialog.noteLabel` = `Megjegyzés hozzáadása (nem kötelező)`. NEM `jegyzet`: a Microsoft azt a
  külön álló Notes-elem értelmére tartja fenn (`Jegyzetek`, `Skype-jegyzetek`), a macOS pile-ban pedig a `megjegyzés`
  csak a „remember” jelentésben fordul elő (`Helyesírás megjegyzése`), ami itt nem zavaró, mert a mi mondataink tárgya
  mindig maga a szöveg.
- **view (a művelet) → `megtekintés`** · macOS Tier 1 (`A(z) %@ megtekintéséhez jelentkezzen be`,
  `Megosztott (csak megtekintés)`), Microsoft-terminológia (`view` = `megtekint`) · high.
- **„Your note” → `A megjegyzésed`** · a mezőcímkék határozott névelős birtokos alakja már a katalógus szokása
  (`settings` „Your email address” = `Az e-mail-címed`) · high. Az angol itt szándékosan elhagyja a „(optional)”-t, mi
  is elhagyjuk.
- **„What was sent” → `Mi került elküldésre`** · a testvér `errorReporter.dialog.detailsToggle` = `Mi kerül elküldésre`
  MÚLT IDEJŰ alakja, betű szerint ugyanaz a szerkezet. A két kapcsoló egymás mellett él ugyanabban a funkcióban, és az
  angol is csak igeidőben tér el („What's about to be sent” / „What was sent”), ezért a magyar sem hoz be új
  szerkezetet. A `kerül + -ásra/-ésre` alak a fájl saját idiómája (`eltávolításra kerülnek`, `nem kerülnek elküldésre`).
- **„Adding…” → `Hozzáadás…`** · a testvér `dialog.sending` = `Küldés…` főnévi folyamatalakja ugyanezzel a lemmával ·
  high. U+2026, nem három pont.
- **„That report can't take a note any more. …” →
  `Ehhez a jelentéshez már nem lehet megjegyzést hozzáadni. Ha el szeretnéd juttatni a megjegyzésedet a csapathoz, küldj új jelentést a Súgó menüből.`**
  · a `már nem` tagadás a macOS mintája (`… vagy Ön már nem rendelkezik engedéllyel …`); a `Súgó` a szótár szava (mac
  Tier 1, `menu.bar.help` = `Súgó`), a menüből-irányítás pedig a macOS „válassza az Apple menü > …” mondatainak tegező
  változata. A `hozzáadni` igét azért választottuk a szebb `megjegyzést fűzni` helyett, mert a `fűz` lemma a pile-ban
  csak az `összefűz`/`hozzáfűz` (append) jelentésben él, a `hozzáad` viszont az egész funkció settled igéje. Nincs benne
  sem `hiba`, sem `nem sikerült`: tényközlés, nem hibaüzenet.
- **„Couldn't add your note: {error}” → `Nem sikerült hozzáadni a megjegyzésedet: {error}`** · pontosan a testvérek
  szerkezete (`Nem sikerült elküldeni a hibajelentést: {error}`, `Nem sikerült menteni a csomagot: {error}`) · high.
- **„Note added to your report. Your reference ID is” →
  `A megjegyzésed bekerült a jelentésbe. A hivatkozási azonosítód:`** · a testvér `sentToast.message`
  (`A hibajelentés elment. A hivatkozási azonosítód:`) második mondata változatlanul, kettősponttal, mert az azonosító
  közvetlenül utána jön egy jelvényben. A `hivatkozási azonosító` settled.
- **„View or add notes to the report” → `Megtekintés vagy megjegyzés hozzáadása`** · (`review-queue.md`). Mindkét fele
  megvan (nézés + hozzáadás), főnévi címkeformában, és elfér az értesítés keskeny gombsorában a rövidebb
  `Beállítások módosítása` mellett. A „to the report” nem jelenik meg külön: fölötte ott áll az értesítés címe
  (`A hibajelentés elment`), tehát a tárgy egyértelmű, a teljes `Jelentés megtekintése vagy megjegyzés hozzáadása`
  viszont már 47 karakter lenne.

## Kijelölés és a kijelölés törlése: a Select / Deselect párbeszéd (`selection.*`)

- **select → `kijelölés`, deselect → `kijelölés törlése`** · macOS 26.6.2 Finder `hu`
  (`Finder.app/Contents/Resources/hu.lproj/MenuBar.strings`, `172.title` = `Összes kijelölése`, `300488.title` =
  `Kijelölés törlése`; ellenőrizve 2026-08-29) · high. Ez a natív menük szakaszának `Deselect all` döntését erősíti meg,
  és innen jön a párbeszéd két címe: `Fájlok kijelölése` / `Fájlok kijelölésének törlése`, szóról szóra a
  `menu.select.files` / `menu.select.deselectFiles` alakja, hogy a cím és az őt megnyitó menüpont ne mondjon mást.
- **A `megszüntetése` alak sehol nem maradt**: a `commands.selectionDeselectFiles.label` és a
  `commands.selectionDeselectAll.label` is a `törlése` alakot viszi, tehát a parancspaletta, a menüsor és a párbeszéd
  ugyanazt a szót mondja. A Tier-3 ortodox pár ugyan a `megszüntetése` szót használja (Total Commander `hu`
  „Csoportkijelölés megszüntetése”, `WCMD.INC` 522; Double Commander `hu` „Csoport kijelölésének megszüntetése”), de a
  Tier-1 Finder a `törlése`, és a Finder nyer. ❌ Ne told vissza a `megszüntetése` alakra.
- **A súgóbuboréknak NEM kell tartalmaznia a feliratot** (`selection.action.*`). A gomb hozzáférhető neve a felirat
  kulcsából jön (`QueryDialog.svelte`: `aria-label={config.primaryAction.ariaLabel ?? config.primaryAction.label}`), a
  súgóbuborék pedig egy belső `span` `use:tooltip` akciója, tehát a WCAG 2.5.3 már a felépítésből adódóan teljesül. A
  ház precedense ugyanezt mondja: a `search.action.showAll.label` (`Összes megjelenítése a főablakban`) és a `.tooltip`
  (`A találatok megnyitása az aktív panelen`) szándékosan más szavakat használ. A buborék tehát szabadon fogalmazható;
  csak ugyanazt a műveletet nevezze meg, mint a felirat, és mondja ki, hogy a fókuszált panelen történik.
- **Ettől függetlenül a felirat így is a buborék eleje lett**, mert a két legközelebbi testvér ezt a formát viszi:
  főnévi szerkezet + helyhatározó (`A találatok megnyitása az aktív panelen`, `A fájl megnyitása az aktív panelen`). Nem
  kényszerből, hanem mert ez a kulcscsalád háziformája.
- **A `fájlok kijelölésének törlése` alak nyelvtani döntés, nem a buborékért van**: az `itt látható fájlok` birtokos
  szerkezete EGY szintű (pont a `menu.select.deselectFiles` alakja), míg az
  `Ezeknek a fájloknak a kijelölésének a törlése` kétszintű birtokos lánc lenne. Ez a felirat önmagában is jobb,
  akárhogy szól majd a buborék.
- **focused pane → `a fókuszált panel`** · a katalógus szava (`commands.navGoToPath.description`,
  `commands.favoritesAdd.description`) · high. Helyhatározóban `a fókuszált panelen`. A `search.action.*.tooltip`
  `az aktív panelen` alakja az angol „active pane” párja, tehát nem ugyanaz a kulcscsalád.
- **`selection.runHint` a `search.runHint` mintája**: `Nyomd meg az Entert a szűréshez` (a testvér
  `Nyomd meg az Entert a kereséshez`). Az Enter billentyű neve `Enter`, tárgyesetben `az Entert`, ahogy a katalógus
  mindenütt írja.
- **`selection.recent.*` a `queryUi.recent.*` ikertestvére**, csak a „keresés” helyén `kijelölés` áll:
  `Az összes legutóbbi kijelölés megjelenítése`, `Összes legutóbbi kijelölés`, `Legutóbbi kijelölések szűrése`,
  `Nincs a szűrőnek megfelelő legutóbbi kijelölés.`, `Legutóbbi kijelölések` (a popover és a lista az angolban is
  szándékosan azonos). Az `applyAria` a `search.recent.runAria` szerkezetét viszi:
  `Legutóbbi {mode} kijelölés alkalmazása: {query}`. A `{query}` a felhasználó nyers szövege, ezért a mondat végén,
  kettőspont után áll, így bármi elfér benne.

## Belső driftszedés: egy fogalom, egy név

A `desktop-i18n-term-consistency` 28 divergenciát talált a `hu` katalógusban (egy angol érték, két magyar alak).
Tizenhat valódi drift volt, tizenkettő szándékos határvonal. A drift két oka: (a) egy szót menet közben újradöntöttünk,
de csak a hívóhelyek egy részét írtuk át, és (b) a menüsoros passz csak a `menu.json`-t frissítette, így a
parancspaletta a régi szót vitte tovább. A határvonalakat ez a szakasz írja le, hogy a következő passz ne lapítsa el
őket.

### A javított driftek

- **Quit Cmdr → `Kilépés a Cmdrből`** · macOS AppKit `hu` (`Quit` = `Kilépés`) és Finder `hu` (`Kilépés a Finderből`) ·
  high. A `commands.appQuit.label` `Cmdr bezárása` alakja a `close`-t mondta a `quit` helyett.
- **zoom in / out → `Felnagyítás` / `Lekicsinyítés`** · Safari `hu` `MainMenu.strings` `438.title` / `439.title` (a
  telepített macOS 26.x-ből, ellenőrizve 2026-08-30) · high. A `commands.viewZoom*` a régi `Nagyítás` / `Kicsinyítés`
  alakot vitte, ami ütközött volna a zoom-almenü saját címével.
- **Connect to server → `Kapcsolódás szerverre`** · macOS Finder `hu` `N84` = `Kapcsolódás szerverre…`, `FR15` =
  `Kapcsolódás a szerverre` · high. ❗ A katalógus két kulcsa a nyelvtanilag „helyesebbnek” tűnő `szerverhez` alakot
  vitte; a Tier-1 Apple-szóhasználat a `-ra/-re`, és az nyer.
- **Connected → `Kapcsolódva`** · macOS AppKit `hu` `SavePanel` (`Connected` = `Kapcsolódva`) · high. A `Csatlakozva`
  alak három kulcsból eltűnt (`ai.cloud.connected`, `ai.cloud.connectedNoModels`,
  `fileExplorer.network.browser.status.connected`). A folyamatban lévő `Connecting…` marad `Csatlakozás…`, mert az Apple
  is ezt az igét használja rá (`SavePanel`).
- **Try again → `Próbáld újra`, Retrying → `Újrapróbálás`** · a katalógus tegező regisztere · high. A két fogalom külön
  alakot kap: a gomb felszólít, a folyamatjelző főnévvel nevez. Az `Újrapróbálkozás` alak megszűnt.
- **case-sensitive → `Kis- és nagybetűérzékeny`** · macOS `hu` (`Kis- és nagybetűérzékeny`, egybeírt összetétel) · high.
  A `Kis- és nagybetűre érzékeny` és a `Kis- és nagybetűk megkülönböztetése` alak is erre cserélődött, a
  `queryUi.scope.toggle.caseSensitiveAria` is (`… illesztés`), hogy a WCAG 2.5.3 tartalmazása megmaradjon.
- **Dismiss → `Elvetés`** mindenütt, a `viewer.reloadToast.dismissTooltip` `Eltüntetés` alakja is.
- **`settings.section.imageIndexing` → `Képek indexelése`**, mint a testvére, a `settings.section.driveIndexing`
  (`Meghajtó indexelése`) és az `indexing.enrich.label`.
- Egy-egy alakra hozva: `Go to home folder` → `Ugrás a saját mappára` (a paletta, a Ugrás menü és a hibapanel gombja),
  `Low disk space` → `Kevés lemezterület`, `On disk` → `Lemezen(:)`, `Example: {model}` → `Példa: {model}`, és a két
  béta-feliratkozós mondat (siker + kudarc) az onboarding alakjára.

### A határvonalak, amiket NEM szabad elsimítani

- **Cancel**: `Mégsem` a párbeszéd elvető gombja (macOS Finder), `Megszakítás` az, ami egy FUTÓ műveletet állít le
  (`queue.row.cancel`, `transferProgress.titleCancelling`, `errors.write.cancelled.*`, `operationLog.status.canceled`).
  A `Leállítás` a harmadik: egy szolgáltatást állít le (szerver, indexelés, Ask Cmdr).
- **View**: `Nézet` a menüsor NÉVSZÓI menücíme, `Megtekintés` az IGE, ami az F3 megjelenítőt nyitja (`menu.file.view`,
  `commands.fileView.label`). Az angol `@key` leírás is így különbözteti meg őket.
- **Zoom**: `Nagyítás` a szövegnagyítás (View almenü), `Méretezés` a Window > Zoom ablakművelet (macOS AppKit `hu`
  `Zoom All` = `Összes méretezése`). Az angol leírás külön kiemeli, hogy a kettő nem ugyanaz.
- **Select**: `Kijelölés` a fájlkijelölő menü és művelet, `Válassz` a legördülő lista helyőrzője
  (`ui.select.placeholder`) — ott a felhasználót szólítjuk meg, nem fájlt jelölünk ki.
- **Error**: `Probléma` a felhasználónak szóló állapotcella (az angol `@key` maga kéri a barátságosabb szót).
  Diagnosztikai `Hiba` előtag már nincs a katalógusban: ha a frissítéskeresés nem jár sikerrel, a Cmdr teljes
  mondatokban szól (`updates.failure.check`).
- **Bytes**: `Bájtok` a folyamatsáv címkéje, mert a párja a `Fájlok`; `Bájt` a mértékegység-választó gombja, mert a
  szomszédjai `kB`, `MB`, `GB`.
- **From**: `Forrás` a `Cél` párja az átviteli párbeszéd fejlécében; `Innen:` a beágyazott útvonal előtti címke.
- **Purple**: `Bíbor` a Finder hét címkeszínének EGYIKE (macOS Finder `TG_COLOR_3`, szó szerint kell), `Lila` a Cmdr
  saját 12 színű kötetszínezőjében, ahol a köznyelvi színnév a helyes.
- **Put back**: `visszaállítva` a RÉGI NÉV visszaadása (`askCmdr.renameUndo.*`), `visszahelyezve` a Kukából való
  visszatétel (`fileOperations.trash.undone`). A `style.md` szótára már ezt írja elő; az angol mindkettőt „Put back”-nek
  mondja, ami az ANGOL pontatlansága, nem a miénk.
- **Rolling back**: `Visszagörgetés…` a folyamatablak címe (a három pont viszi a folyamatban-lévőséget),
  `Visszagörgetés folyamatban` az `operationLog` állapotcellája, ahol nincs három pont, és a szomszédai a KÉPESSÉGET
  nevezik meg (`Visszagörgethető`, `Nem görgethető vissza`) — ott a puszta főnév félreérthető lenne.
- **Send report**: `Elküldöd a jelentést` a párbeszéd CÍME (a felhasználót kérdezi), `Jelentés küldése` a GOMB.
- **`errors.eject.unexpected` ≠ `errors.mutation.unexpected`**: külön blokk indokolja fent (a kiadás-buborék burkolója
  már `Nem sikerült kiadni:`-val kezdődik, tehát a settled alak szóismétlést adna).
- **Az átnézés/átvizsgálás/keresés hármas**: `átnézés` az élő mappabejárás (`queryUi.results.live.*`,
  `search.walkHandoff.*`), `átvizsgálás` az index- és méret-átvizsgálás (`indexing.*`, `fileExplorer.dirSize.*`) meg az
  Ask Cmdr „végigmegy egy gyűjteményen” sorai, `keresés` maga a keresés funkció. Három fogalom, három szó.
- **memory**: `memória` a RAM (`ai.local.*`), `jegyzet` az Ask Cmdr emlékezete (`settings.askCmdr.memory.*`,
  `askCmdr.tool.memory*`).

## Az angol önellentmondásainak magyar utóélete

Az `en` katalógus öt helyen javította ki önmagát; itt az, amit ez magyarul eldöntött.

### A példa e-mail-cím: `te@example.com` (`settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`)

- **Helyi rész magyarul, domain `example.com`** · ms terminológia hu (`valaki@example.com`, `user@example.com`), RFC
  2606 · high. Mind a három mező ugyanezt viseli: `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`,
  `onboarding.stepBeta.emailPlaceholder`.
- `te@` a `te`-regiszterből jön (`style.md` § Formality), az angol `you@` közvetlen párja. A Microsoft hu terminológia a
  „someone” mintát viszi (`valaki@example.com`), tehát a helyi rész LEFORDÍTÁSA a bevett gyakorlat.
- ❌ NEM `pelda.hu` vagy `example.hu`: azok valóban regisztrálható domainek, tehát valakinek az igazi címe lehet. Az
  `example.com` az RFC 2606 példacélra fenntartott domainje, ezért soha nem lesz senkié.
- Ez felülírja a korábbi „you@example.com verbatim mindenhol” bejegyzést: az `en` `@key` most kifejezetten a helyi rész
  lokalizálását kéri.

### A régi NÉV visszaadása mondat megnevezi a tárgyát (`askCmdr.renameUndo.undone`/`.partial`)

- `askCmdr.renameUndo.undone` / `.partial` →
  **`{countText} {count, plural, one {fájl} other {fájl}} régi neve visszaállítva.`** · a család már meglévő
  szóhasználata (`.undoing` = „A régi nevek visszaállítása…”, `.skipReason.failed.*` = „a régi nevét”) · high.
- Az angol korábban ugyanazt a „Put back {countText} {files}.” mondatot adta a NÉV-visszaállításnak és a Kukából való
  visszahelyezésnek; magyarul ez a kettő már addig is külön volt (`visszaállítva` vs `visszahelyezve`), most az angol is
  megnevezi a tárgyat. A magyar tárgymegnevezés a birtokos szerkezet (`… fájl régi neve`), mert számnév után a magyar se
  a főnevet, se a birtokot nem többesíti (`style.md` § Plurals).
- `fileOperations.trash.undone` változatlan: ott `visszahelyezve` a helyes, és marad.

### `mappa` az indexelés súgószövegében (`settings.indexing.enabled.description`)

- `settings.indexing.enabled.description` → **`azonnali mappaméretekért`** · a `könyvtár` csak technikai értelemben
  járja, a UI szava a `mappa` (`terms.json` `folder`) · high. Az angol is „folder sizes”-t mond.

### A macOS-panelnevek magyarul, a futásidejű tokenek mellett

Nyolc `errors.*` kulcs angolul beégetett panelneveket hordozott; ezek most vagy futásidejű tokenek, vagy magyar
Apple-szóhasználat.

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` **szó szerint marad**: a futásidőben a
  FELHASZNÁLÓ Mac-jén látható panelnevet kapja.
- ❌ **Soha ne ragassz esetragot, névelőt vagy toldalékot közvetlenül egy tokenre.** A régi „a System Settingsben” alak
  `{system_settings}ben`-t adna, ami a `Rendszerbeállítások` mellé rossz (a hangrend `-ban`-t kér). A katalógus máshol
  is használt `itt: {system_settings}` szerkezet a kiút (`style.md` § Agglutination).
- A tokenek által NEM fedett panelnevek magyarul mennek, ahogy az Apple írja őket:
  - **Apple Account → `Apple-fiók`** · macOS 26.6.2 (25G83),
    `AppleIDSettings.appex/Contents/Resources/InfoPlist.loctable` `hu.CFBundleDisplayName`, 2026-08-30 · high.
  - **General → `Általános`** · `hu/macOS/SystemSettings/Localizable.json` `GENERAL` · high.
  - **Login Items & Extensions → `Indítóelemek és bővítmények`** · macOS 26.6.2 (25G83),
    `LoginItems.appex/Contents/Resources/Localizable.loctable` `hu["Login Items & Extensions"]`, 2026-08-30 · high.

### A natív menüsor két Apple-tétele (`menu.app.showAll`/`.hideOthers`, `commands.appShowAll.label`, `commands.appHideOthers.label`)

`menu.app.showAll` / `menu.app.hideOthers` (és a párjuk, `commands.appShowAll.label` / `commands.appHideOthers.label`) →
**`Összes megjelenítése`** / **`Többi elrejtése`** · macOS 26.6.2 (25G83),
`Finder.app/Contents/Resources/hu.lproj/MenuBar.strings` `300730.title` / `300729.title`, 2026-08-30 · high. Az Apple
szóhasználata, a magyar mondatkezdő nagybetűvel (ami itt egybeesik az Apple alakjával). A `menu.*` család natív, ICU
nélkül renderelődik: aposztróf ott EGYSZER írandó.

## Egy félbehagyott visszagörgetés befejezése (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Visszagörgetés befejezése`** · macOS 26 Finder `hu`: `Másolás befejezése` (`NE108`, „Finish
  Copying”) és `Tömörítés befejezése` (`AR4`, „Finish Compressing”), tehát a `<főnév> befejezése` az Apple saját magyar
  alakja erre a műveletre (ellenőrizve 2026-08-30) · high. A `visszagörgetés` szótő a katalógusé
  (`operationLog.dialog.rollBack`, `rollingBack`, `partiallyRolledBack`), a nominális címkeforma pedig a `style.md` §
  Formality házirendje. A `befejezése` egyértelműen azt mondja, hogy VÉGIGVISZI a félbemaradt visszagörgetést, sosem
  azt, hogy újat indít.
- **A két `finishRollBack` kulcsnak betű szerint azonosnak kell maradnia.** Az `operationLog.dialog.finishRollBack` és a
  `fileOperations.rollbackConfirm.finishRollBack` angolja és művelete ugyanaz (`Finish rolling back`); ha a magyar
  értékük eltér, az `i18n-terms` figyelmeztet. A `Roll back` párnál a katalógus már így csinálja: `Visszagörgetés`
  mindkét helyen.
- **`Finish rolling this back?` → `Befejezed a művelet visszagörgetését?`** · a testvér
  `fileOperations.rollbackConfirm.title` („Visszagörgeted ezt a műveletet?”) regiszterét viszi: tegező kérdés, ugyanaz a
  `művelet` főnév · high. A mutató névmás szándékosan marad el: az `ennek a műveletnek a visszagörgetését` kétszintű
  birtokos lánc egy párbeszédcímben, az angol pedig maga is rövidít a testvéréhez képest (`Finish rolling this back?` a
  `Roll this operation back?` mellett). Egy modális ablakban úgyis egyetlen műveletről van szó.
- **A sor alatti magyarázó mondat a `fileOperations.rollbackConfirm.bodyUndoByDeleting` szavait viszi tovább**: „A Cmdr
  visszagörgette, amit tudott, a többit pedig úgy hagyta, ahogy volt. A befejezéshez még egy kör kell, és amiben a Cmdr
  továbbra sem biztos, azt megint kihagyja.” A `kihagyja` szóról szóra a testvéré, az `úgy hagyta, ahogy volt` pedig az
  `operationLog.rollback.refusalAlreadyRolledBack` („Ez már úgy van, ahogy korábban volt.”) és a
  `rollbackConfirm.leaveAsIs` („Maradjon így”) hangját tartja · high. A mondat szándékosan nem ígér teljes visszaállást:
  ami maradt, az lehet olyan fájl, amit a Cmdr nem tud a saját feljegyzéséhez kötni, és azt megint kihagyja.
- **Mérlegelés, nem forrás: `another pass` → `még egy kör`** · tentative. A pile egyik forrásában sincs erre alak, tehát
  ez saját döntés: a `még egy kör` köznyelvi és illik a tegező hanghoz. A `még egy átfutás` hivatalosabb, a
  `még egy menet` sportosabb; egyik sem jobban sourcolt. Ha egy későbbi menet jobbat talál, ez a hely cserélhető.
- **`in {folder}` → `a(z) {folder} mappában`: a ragot a KÖZNÉVRE tesszük, sosem a névre** · high. Tetszőleges
  mappanévhez nem lehet helyes toldalékot választani, mert a `-ban`/`-ben` illeszkedése és a névelő `a`/`az` alakja is a
  névtől függ. Ezért a `{folder}` jelzőként áll, a `-ban` rag pedig a `mappa` szóra kerül, ami mindig ugyanaz. Az `a(z)`
  a katalógus háziformája placeholder előtt (harminc körüli előfordulás, például
  `Eltávolítod a(z) {hostName} gépet a szerverlistából?`), és a macOS `hu` is így ír (`A(z) „^0”…`). A
  `queue.row.reversalDeleting` mellett a sor így szól: „A létrehozottak törlése a(z) Backup mappában” — a helyhatározó
  mondja ki, hogy a törlés a mappán BELÜL történik, különben a magában álló név úgy hat, mintha maga a mappa tűnne el.
  Ezt a hibát javítja a kulcs.

## A megszakított visszagörgetés eredményértesítése (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

Tizenhét új kulcs: a felhasználó `Visszagörgetés`-t nyomott egy futó másoláson vagy áthelyezésen, a visszacsinálás
lefutott, és ez az értesítés mondja el, mi sikerült belőle. Legfeljebb három rész, ebben az olvasási sorrendben: egy
címsor (`doneDeleting` / `doneMovingBack` / `someDeleted` / `someMovedBack` / `stoppedDeleting` / `stoppedMovingBack`,
mindig csak egy), a `leftBehind` bevezető sor, és alatta felsorolásban a `reason.*` indokok, mindegyik vagy MEGNEVEZI az
egy elemet (`*.named`), vagy MEGSZÁMOLJA őket (`*.counted`). Plusz a `rollbackConfirm.body`, aminek az angolja bővült.
Az egész hang: a Cmdr a gondos dolgot tette. Se bocsánatkérés, se riasztás.

- **"Left X alone: …" → a szállított `askCmdr.renameUndo.skipReason.*` keret: `<alany> változatlan maradt: <indok>.`** ·
  `high`, és két okból kötelező. (1) A `reason.folderNotEmpty.named`/`.counted` angolja BETŰ SZERINT azonos az
  `askCmdr.renameUndo.skipReason.folderNotEmpty.named`/`.counted` angoljával, tehát a `desktop-i18n-term-consistency`
  egyetlen magyar alakot vár rájuk, és a két család értéke betű szerint együtt mozog (lásd a névelő-szabályt lentebb).
  (2) Ha csak az a két sor igazodna, a felsorolásban négy `kimaradt` mellett állna egy `változatlan maradt`, amit a
  felhasználó EGY pillantással lát; a két funkció eltérése viszont sosem kerül egymás mellé. Az értesítés belső
  egyöntetűsége erősebb szempont, ezért a `drift` / `unverifiable` / `spotTaken` / `folderNotEmpty` mind a nyolc sora
  ugyanazt a keretet viszi.
  - **A `leftBehind` bevezető sora marad `kihagyja`**, szó szerint a testvér `rollbackConfirm.bodyUndoByDeleting`-ből
    (`Amiben a Cmdr nem biztos, azt kihagyja`) · high. A munkamegosztás így is megvan: a bevezető mondja ki az ÍGÉRETET
    és a következményt (`ezek a helyükön maradtak`), a sorok pedig elemenként az ÁLLAPOTOT (`változatlan maradt`).
  - **A `drift` sor látszólagos ellentmondása (`változatlan maradt: módosult…`) örökölt, és feloldható**: a
    `változatlan` a VISSZAGÖRGETÉSRE vonatkozik (a Cmdr nem nyúlt hozzá), a `módosult` pedig arra, ami korábban történt
    vele. Pontosan így él a szállított `askCmdr.renameUndo.skipReason.drift.named` is
    (`A(z) „{name}” változatlan maradt: az átnevezés után módosult.`), tehát a keret ezt az olvasatot már elbírja.
  - ❌ NEM `békén hagyja` / `érintetlenül hagyja`: mindkettő értelmes magyar, de a `hu` pile egyikre sem ad egyetlen
    találatot sem.
- **A visszatétel igéje az EREDMÉNY-sorokban `visszahelyez`, a FOLYAMAT-sorokban marad `visszavitel`** · macOS Finder
  `PE130` Tier 1 (`^0 elem visszahelyezése nem sikerült.`), a testvér `fileOperations.trash.undone`
  (`{countText} fájl visszahelyezve.`), és az en `@key` kifejezetten ezt a testvért kéri · high. A katalógus
  folyamatszövegei ugyanennek a visszagörgetésnek a futása közben `visszavitel`-t mondanak
  (`transferProgress.titleReversalMovingBack` = `A fájl visszavitele…`, `queue.row.reversalMovingBack`,
  `rollbackConfirm.bodyUndoByMovingBack` = `Ez visszaviszi a fájlokat oda, ahonnan jöttek.`), és ez a kettősség
  MEGMARAD: a `visz` a mozgásra utal (miközben tart), a `helyez` a végállapotra (amikor megérkezett), és a magyar ezt a
  két aspektust külön szóval mondja. Gyakorlati bizonyíték is van rá: a `visszavisz` `-va/-ve` igeneve (`visszavíve`)
  egy értesítésben olvashatatlan, a `visszahelyezve` viszont pont a katalógus bevett eredményalakja.
  - Ezzel a „Put back” családnak három tagja van, mindegyik más művelet: `visszaállítva` = a RÉGI NÉV visszaadása
    (`askCmdr.renameUndo.*`), `visszahelyezve` = a Kukából és a visszagörgetésből való visszatétel
    (`fileOperations.trash.undone`, `cancelRollback.*`), `visszavitel` = a visszagörgetés futó folyamata. Lásd fentebb:
    § A Kuka-értesítés két gombja.
- **"Removed" → `eltávolítva`, sosem `törölve`** · a szótár `remove → eltávolítás` sora, és ugyanaz az érv, ami a
  kilépés-visszaszámláló "clears away" → `eltávolít` döntésénél: az értesítés megnyugtatás, nem szabad, hogy „a Cmdr
  fájlt töröl” villanjon fel benne · high. Az angol is szándékosan `Removed`-et mond, miközben a megerősítő párbeszéd
  `deletes`-t.
- **A „the N items” (teljes) kontra „N items” (részleges) szembeállítást a MONDATSZERKEZET hordozza, nem névelő** ·
  high. A `done*` pár `A Cmdr mindent eltávolított, amit létrehozott: {countText} elem.` /
  `A Cmdr mindent visszahelyezett: {countText} elem.` alakot kap (kimondott `mindent` = véglegesség, plusz a cselekvő
  megnevezése), a `some*` pár puszta igeneves számlálás: `{countText} elem eltávolítva.` /
  `{countText} elem visszahelyezve.`
  - ❌ NEM `Mind a(z) {countText} elem eltávolítva`: `count = 1` esetén `Mind az 1 elem …` lesz belőle, ami nem magyar.
    A `mind a(z)` + `{countText}` szerkezet minden ilyen kulcsban ez a csapda; a `mindent` + kettőspontos szám elkerüli.
  - A `done*` sorok megtartják a minősítést (`amit létrehozott`): a puszta `A Cmdr mindent eltávolított.` ijesztő, mert
    nem mondja meg, MIT.
- **"Stopped after …ing" → `{countText} elem <művelet>e után leállítva.`** · a `leállítás` a szótár stop-szava
  (`transferProgress.rollbackTooltip` = `Leállítás, és minden eddig kiírt fájl törlése`), a birtokos igenévi szerkezet
  pedig a macOS `^0 elem visszahelyezése` mintája · high. `The rest are still there.` → `A többi ott maradt.`;
  `The rest stayed where the move put them.` → `A többi ott maradt, ahová az áthelyezés vitte.` (`áthelyezés` = a szótár
  `move` szava).
- **"it changed" → `módosult`** · macOS Tier 1 (`A(z) „%@” fájl nem módosult a közelmúltban.`) · high. NEM
  `megváltozott`: a pile-ban a fájlra vonatkozó alak a `módosul`.
- **"Cmdr couldn't check whether …" → `a Cmdr nem tudta ellenőrizni, hogy …`** · az `ellenőriz` a katalógus szava
  (`transferProgress.scanTitleCopy` = `Ellenőrzés a másolás előtt…`), a `nem sikerült`/`nem tudta` a nyugodt hangnem
  bevett alakja · high. Szándékosan NEM a `Nem sikerült megerősíteni, hogy …` család
  (`fileOperations.mkdir.timeoutMessage`): az angol itt `check`-et mond, nem `confirm`-ot, és a két fogalom külön él a
  katalógusban.
- **"something else now sits where it came from" → `már valami más van ott, ahonnan jött.`** · a `valami más` a testvér
  `transferProgress.foregroundBusyToast` szava (`Itt valami más van nyitva.`), az `ott, ahonnan jött` pedig szó szerint
  a `rollbackConfirm.bodyUndoByMovingBack` fordulata (`oda, ahonnan jöttek`) · high.
- **"Couldn't undo {name}" → `A(z) „{name}” visszagörgetése nem sikerült.`** · macOS Finder `PE130` Tier 1 a
  mondatformára (`A(z) „^1” visszahelyezése nem sikerült.` / `^0 elem visszahelyezése nem sikerült.`) · high. Az angol
  itt a köznyelvibb `undo`-t mondja, a magyar mégis a szótár `visszagörgetés` szavát viszi: a katalógus per-ELEM
  kimenetele már `Visszagörgetve` (`operationLog.outcome.rolledBack`), tehát az egy elemre vonatkozó visszagörgetés már
  bevett, a `visszavonás` pedig a katalógusban egy MŰVELETRE vonatkozik, nem egy fájlra. A második mondat a testvér
  `fileOperations.trash.undoUnavailable` szerkezetét viszi (`Lehet, hogy … a meghajtójuk nincs csatlakoztatva`), a
  `csak olvasható` pedig a szótár szava, macOS Tier 1 (`egy csak olvasható köteten van`).
- **A `named` és a `counted` ág UGYANAZT a szerkezetet viszi, csak az alany más** · high. A `{countText} elem` alany
  magyarul EGYES számban egyeztet, ezért az állítmány mindkét ágban `változatlan maradt`; a kettőspont utáni indoklás
  viszont TÖBBES számú igét kaphat a számláló ágban (`… változatlan maradt: módosultak, …`), mert ott már a halmazra
  utalunk vissza. Ez nem lazaság: pontosan ezt csinálja a szállított `askCmdr.renameUndo.skipReason.drift.counted`
  (`… változatlan maradt: az átnevezés után módosultak.`). Így a számláló ágnak nincs szüksége kitett `ezek` névmásra
  sem.
- **Mérlegelés, nem forrás: `put it there` → `odatette`** · tentative. A pile egyik forrásában sincs erre alak. Az
  `odatesz` azért nyert, mert MÁSOLÁSRA és ÁTHELYEZÉSRE is igaz (az `odamásolta` csak az egyikre), és a `kiírta`
  (`transferProgress.rollbackTooltip` `kiírt fájl`) mappára nem áll, az `item` pedig itt mappát is jelent.
- **Névelő + `{name}` MINDIG `A(z) „{name}”`** · macOS Tier 1 (`A(z) „^0” elemet…`,
  `A(z) „^1” visszahelyezése nem sikerült.`) és a `hu` katalógus 22 olyan kulcsa, ahol névelő áll egy név előtt · high.
  A névelő `a`/`az` alakja a név ELSŐ HANGJÁN múlik, amit írás közben senki nem tud (`alma.txt` → `az`, `beszámoló.pdf`
  → `a`), tehát a puszta `A {name}` minden magánhangzóval kezdődő fájlnévnél hibás magyar. Az idézőjel ugyanabból a
  forrásból jön, és a hosszú vagy szóközös neveket is elhatárolja.
  - **A névelő NÉLKÜLI helyeket nem érinti**: kettőspont vagy birtokos szerkezet után a placeholder csupaszon marad
    (`Letöltve: {fileName}`, `{name} megnyitása`), mert ott nincs mit egyeztetni.
  - **A `{name}` továbbra sem kap RAGOT.** Ahol az angol köznevet is mond (`the folder {name}`), a köznév áll utána, és
    az visel minden ragot: `A(z) „{name}” mappa változatlan maradt`. Ugyanaz az elv, mint a `queue.row.reversalInFolder`
    `a(z) {folder} mappában` sorában.
  - **Ehhez KÉT család mozdult együtt** (2026-08-31): a négy új `cancelRollback.reason.*.named` sor, és a szállított
    `askCmdr.renameUndo.skipReason.*.named` mind az öt sora (`drift`, `nameTaken`, `unverifiable`, `folderNotEmpty`,
    `failed`), ami addig `A {name}`-et írt. Együtt kellett menniük, mert a `folderNotEmpty` pár angolja betű szerint
    azonos, tehát a `desktop-i18n-term-consistency` egyetlen magyar alakot vár rájuk; és mert egy félig javított család
    rosszabb bármelyik végállapotnál (a rename-undo értesítésben is egyszerre látszanak a sorok). Az öt szállított kulcs
    ANGOLJA nem változott, tehát a `sourceHash`-ük érintetlen: ez fordítási minőségjavítás, nem újrafordítás.
  - **A két nyitott család is lezárva** (2026-09-02). Az `errors.provider.appBased.transient`/`.needsAction`/`.serious`
    mostantól `a(z) **{name}**` és `a(z) {app}` alakot ír: a szolgáltatói névsor tényleg vegyes (`az iCloud`,
    `az OneDrive` szemben a `a Dropbox`, `a pCloud` alakkal), és a sorokban KÉT független ismeretlen áll, mert a
    `{name}` a `displayName`, a `{app}` az `appName` kulcsból jön. ⚠️ Kulcsonként KÉT névelőhely van, a `serious`-ban
    HÁROM (`a(z) {app} appból` és `a(z) {name} állapotoldalát` is), tehát a sorokat végig kell olvasni: az elsőt
    javítani és továbbmenni pont olyan félkész állapot, mint amit a fenti bekezdés tilt.
  - **Az `errors.provider.iCloud.*` három sora ugyanezt hozta, de ott a névelő NEM ismeretlen**: a `{name}` mindig az
    egyetlen `iCloud Drive` displayName, ezért a helyes alak a kiírt `az **{name}**`, nem az `a(z)`. Ahol a placeholder
    értékkészlete egyelemű, ott a hedge fölösleges, és rosszabb magyar; a hedge az ISMERETLEN kezdőhangnak szól, nem a
    placeholdernek magának.
- **`askCmdr.renameUndo.undoJob` → `Az összes {csomag} visszavonása ({countText})`: ÁTFOGALMAZÁS, nem névelő** · macOS
  Tier 1 a szerkezetre (`Az összes lemez (^0) kiadásához kattintson az Összes kiadása gombra…`), és a pile-ban egyetlen
  `Mind a/az` + számnév alak sincs · high.
  - A `Mind a {countText} csomag visszavonása` azért rossz, mert a névelő a SZÁMNÉV kiejtésén múlik: `a kettő`,
    `a három`, `a négy`, de `az öt`, `a hat`, … `az ezer`. Az `a` minden ötödik-ezredik esetben hibás.
  - **A fenti ❌ (`Mind a(z) {countText} elem`) indoklása viszont ITT nem áll**, és ezt érdemes pontosan tudni: az
    `undoJob` gomb csak `jobOperationIds.length > 1` esetén jelenik meg (`AskCmdrMessage.svelte`), tehát a `count` soha
    nem 1, és a `Mind az 1 csomag` eset elő sem fordul. A KÖVETKEZTETÉS mégis ugyanaz marad, csak más okból: az `a(z)`
    írott nyelvi mankó, egy szűk oldalsávba szánt rövid GOMBFELIRATBAN pedig ez a mankó látszik a legjobban. A `@key`
    kifejezetten rövidséget kér.
  - A megoldás elve ugyanaz, mint az `*Aria`-párok egyeztetésénél: **a névelőt olyan szóhoz kötjük, amit mi
    választunk**. Az `összes` kezdőhangja fix (`ö`), tehát `Az összes` mindig helyes, a szám pedig zárójeles értelmezőbe
    kerül, ahol semmivel nem kell egyeztetnie. Ugyanaz a fogás, mint a `done*` soroknál a `mindent` + kettőspontos szám.
  - A `{count}` a parity miatt marad benne (`desktop-i18n-parity` pontos placeholder-halmazt vár), mindkét ága `csomag`:
    az `összes` után a magyar amúgy is egyes számot mond.
  - ❌ **Ez NEM felhatalmazás a `Mind a(z)` + számnév kiseprésére.** Két szállított kulcs viszi ezt az alakot
    (`fileExplorer.imageIndex.folder.allIndexed`, `ui.loadingIcon.finalizing`), mindkettő folyó szövegben, ahol a hedge
    helyénvaló. Az átfogalmazás ott nyer, ahol a szám zárójelbe vagy kettőspont mögé mozdítható, és gombon a legerősebb
    az érv.
  - Mind a hét érintett kulcs ANGOLJA változatlan, tehát a `sourceHash`-ük érintetlen: fordítási minőségjavítás.
- **`rollbackConfirm.body`**: az angol egy harmadik mondattal bővült, ami betű szerint azonos a `bodyUndoByDeleting`
  záró mondatával (`Cmdr skips anything it isn't sure about, so a few may stay behind.`), ezért a magyar is szó szerint
  annak a farkát veszi át (`Amiben a Cmdr nem biztos, azt kihagyja, szóval maradhat belőlük egy-kettő.`). Az első két
  mondat változatlan marad. Így a négy `rollbackConfirm` törzsszöveg egyetlen ígéretet mond, egyetlen megfogalmazásban.

### `cancelRollback.stagedLeftover.*` (a Cmdr saját maradéka a célhelyen)

2026-09-02-én kerültek be. Két sor egy olyan munkafájlról, amelyet maga a Cmdr hozott létre, és nem tudott eltakarítani
a célhelyről. NEM tartoznak a `reason.*` listához: ott a Cmdr a felhasználó fájljait védi, itt a saját maradékáról van
szó.

- **`unfinished copy` → `hiányos másolat`** · a `hiányos` az Apple szava az „incomplete"-re (macOS `LA33`: „sérült vagy
  hiányos"), a `másolat` a `NE111`-ből („megőrizhet egy folytatható másolatot") · `high`
- **`at the destination` → `a célhelyen`** · a katalógus szava (`conflictsUnknown`, `célhely`) · `high`
- **`transfer` (főnév) → `átvitel`** · a katalógus már így mondja (`errors.listing.deviceReconnecting.explanation`:
  „megszakított vagy félbeszakadt átvitel után") · `high`
- A „nem sikerült" szerkezet a `reason.failed.*` párja, így a család egy hangon szól.
- ⚠️ **`egy későbbi átvitel során`, ❌ soha nem „legközelebb".** A Cmdr takarítása kihagy mindent, ami egy óránál
  fiatalabb, tehát egy azonnali újrapróbálkozás nem takarít el semmit. Egy be nem tartható ígéret pontosan az a hiba,
  amit ez a sor megszüntet.

## A túl régi WebKit blokkoló képernyője (`main.oldWebkit.*`)

Három szöveg, amit a Cmdr a felülete helyett mutat, ha a Mac Safarija túl régi. A HTML-vázban élnek, nem az appban,
tehát ez az egyetlen, amit az illető a Cmdrből lát.

- **`Software Update` → `Szoftverfrissítés`** · a macOS így nevezi a Rendszerbeállítások paneljét; a Finder Tier-1 nyoma
  megerősíti a szót (`Apple Device Software Update File` → `Apple-eszköz szoftverfrissítési fájlja`) · `high`.
- **`Quit` → `Kilépés`** · macOS AppKit `Quit` → `Kilépés` · `high`.
- **A márka kötőjel nélkül toldalékolódik: `A Cmdrnek`**, a `style.md` § Brand and do-not-translate szabálya szerint.
- **`Safari 15.4-es vagy újabb verzió`**: a verziószám számjegy marad, a magyar toldalék kötőjellel kapcsolódik hozzá.
- **`Mac` marad `Mac`, ragozva `Macen`.** A `Safari` mostantól a `BRAND_WORDS` listán van.

## A régi macOS értesítése (`main.oldMacos.*`)

Egyszeri párbeszédpanel egy macOS 12-nél régebbi Macen: a Cmdr elindul, de a tesztelt tartományon kívül van. A hang
őszinte és laza, nem bocsánatkérés és nem figyelmeztetés, hiszen az app fut.

- **`supported` → `támogatott`** · mac (Finder `A művelet nem hajtható végre, mert az nem támogatott.`) · `high`.
- **`X and up` → `X és az annál újabb`** · mac (SystemSettings `OS X %@ vagy újabb rendszer szükséges`) · `high`. Az
  Apple önöző mondatából csak a TERMINOLÓGIA jön; a mondat a miénk, tehát tegező.
- **`best effort` → `a tőle telhető legjobbat nyújtja`** · a pile-ban nincs rá terminus (csak hálózati QoS-definíciók) ·
  `high` a körülírásra. Szándékosan nem tükörfordítás.
- **`look off` → `félremehet`** · köznyelvi; a `hiba` regisztert a hang tiltja, ezért kerüljük.
- **`something broken` → `valami elromlottnak tűnik`** · ugyanezért: nem `hibás`.
- **Az utolsó mondat David egyes szám első személyben**, tegezve, mint az `onboarding.stepBeta.greeting`.
- **A `macOS 12-t` tárgyragos alak kötőjellel áll**, mert számjegy után jön a rag.

## Belenézés a fájlokba: az `inspect_file` eszköz és a hozzájárulási képernyő új ígérete (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

Az Ask Cmdr mostantól kérésre beleolvas egy fájlba (néhány sor szöveg, egy PDF néhány oldala a címével és szerzőjével,
egy archívum fájllistája, egy fotó kameraadatai és a készítés helye), ezért a hozzájárulási szöveg újra megjelenik. A
`consent.contentsRule` a régi `consent.noContents` helyébe lép: annak utolsó két mondata (fotókeresés; a javaslatok
jóváhagyásra várnak) szó szerint átkerült, csak az első mondatok újak.

- **thumbnail → `bélyegkép`** · macOS Finder Tier 1 (`Bélyegképméret:`, `kis/közepes/nagy bélyegképméret`), Xfce Thunar
  (`Bélyegképek megjelenítése:`), a régi `consent.noContents` is ezt használta · high. A Microsoft `miniatűr` a Windows
  szava, kimarad (macOS-vs-Windows-szakadás, a macOS nyer).
- **camera → `kamera`; camera details (a fotó EXIF-adatai: gép, objektív, beállítások) → `kameraadatok`** (birtokos:
  `egy fotó kameraadatai`) · `kamera`: MS terminológia (`camera` = `kamera`), macOS AppKit (`beépített kamera`), és a
  katalógus már összetételben használja (`kameraeszköz`, `kameradémon`); az `-adatok` utótag mintája az MS
  `location data` = `helyadatok` · high a `kamera`, tentative az összetétel (a pile-ban nincs „camera details”, de
  mindkét fele forrásolt, és az összetétel átlátszó). NEM `EXIF`: a hozzájárulási szöveg laikusnak szól, az angol is
  kerüli.
- **location (where a photo was taken) → `hely`; mondatban `hol készült`, birtokosként `a készítés helye` /
  `készítési helye`** · macOS Finder és AppKit (`Location` = `Hely`, `Hely:`), MS (`location` = `hely`, `geolocation` =
  `Földrajzi hely`, `location data` = `helyadatok`) · high a `hely`, tentative a `készítési hely` kollokáció (nincs
  pile-találat a fotó-készítés értelemre; a `tartózkodási hely` az MS-ben a felhasználó helyét jelenti, nem a fotóét,
  ezért kimarad). A `contentsRule`-ban az angol mellékmondatot (`including where it was taken`) tartja a magyar is:
  `beleértve azt, hogy hol készült`; a listás kulcsokban a rövid birtokos alak (`készítési helye`) áll.
- **page (egy PDF oldala) → `oldal`, SOHA nem `lap`** · MS terminológia (`page` = `oldal` a dokumentumoldal értelemben;
  a `lap` az MS-nél a munkalap/fül) · high. A `lap` a katalógusban a `tab` foglalt szava (`style.md` § Terminology),
  tehát a PDF-oldal csak `oldal` lehet. Összetételben kötőjellel, a rövidítés szabálya szerint: `PDF-oldalak` (mint a
  szótár `PDF-je` alakja).
- **title and author (a PDF metaadata) → `cím` és `szerző`** · `cím`: macOS AppKit (`Title` = `Cím`), MS (`Cím`);
  `szerző`: MS (`author` = `szerző`, három bejegyzés) · high. Birtokos szerkezetben:
  `egy PDF néhány oldalát a címével és a szerzőjével együtt`.
- **text (a szöveges fájl tartalma) → `szöveg`; some lines of text → `néhány sor szöveg`; some text → `némi szöveg`;
  some of its text → `a szövegének egy része`** · macOS Finder és AppKit (`Text` = `Szöveg`), MS (`Szöveg`) · high.
- **archive → `archívum`** (szótár, settled); **the list of files inside an archive →
  `az archívumban lévő fájlok listája`** (a kulcsokban határozatlan névelővel: `egy archívumban lévő fájlok listája`) ·
  high. A rövidebb `consent.item.contents` angolja (`what’s inside an archive`) is ezt a hosszabb alakot kapja, mert a
  rövidebb `egy archívum tartalma` a „fájltartalom” ígéretével ütközne épp azon a képernyőn, amelyik azt mondja, hogy
  egész fájl soha nem megy el.
- **provider → `szolgáltató`, `a szolgáltatód`** (szótár, settled; a `consent.proactive` és a régi `noContents` is
  `a szolgáltatódhoz`) · high.
- **look inside (a file) → `belenéz` (ige, a próza), `Fájlok átnézése` / `Fájlok átnézve` (az eszközsor címkepárja)** ·
  a `belenéz` a katalógus saját szava erre (`search.coverage.denied` = `belenézzen ebbe a mappába`, a régi `noContents`
  = `belenéz a képeidbe`; a `renameReview.*` tooltipek `a fájl belsejéből`), pile-találat nincs. Az eszközsor címkéi a
  szótár `askCmdr.tool.*` szabályát követik (igenév a folyamatra, `-va/-ve` határozói igenév a kész állapotra, ugyanazon
  a tárgyon): a `belenéz` ebbe a mintába nem fér bele (`A fájlokba belenézve` nem állapot, hanem módhatározó), ezért a
  tárgyas `átnéz` viszi a párt (`Fájlok átnézése` / `Fájlok átnézve`), pont úgy, ahogy a `searchPhotos` sor is a családi
  mintát választotta a szó szerinti ige helyett. Az `átvizsgálás` (scan) foglalt a `searchPhotos`/`operationsList`
  sorokon, ezért nem az. Névelő nélkül, mert az angol is puszta többes (egy hívás 1–200 fájlt fed) · tentative
  (`review-queue.md`): az `átnéz` a „belső” jelentést csak sugallja.
- **limited part (of a file) → `egy korlátozott részét`** · leíró (MS `limited` = `korlátozott`) · high. NEM
  `egy kis részét`: az angol a határt ígéri, nem a méretet.
- **whole files → `egész fájlokat`** · leíró · high. A régi `magukat a fájlokat` alak azért nem maradt, mert az új angol
  szándékosan a „teljes fájl vs. egy része” szembeállítást mondja ki.
- A kivezetett újdonság-szöveg (askCmdr.consent.whatsNew.body) második mondata
  (`Ez többet ígér annál, mint amihez hozzájárultál, ezért itt van újra az egész.`) változatlanul átkerült a régi
  fordításból; az első mondat a `belenézhet abba a fájlba, amelyről kérdezel` + egy tárgyas felsorolás
  (`elolvashatja a szövegének egy részét, …`), hogy a lista ne `-ba/-be` ragok láncán lógjon.
- **Utólag ugyanez a két rövid ígéret** (`askCmdr.empty.hint`, `settings.askCmdr.intro`): a „looks inside a file only
  when you ask about it” tagmondat mindkettőben `egy fájlba csak akkor néz bele, ha arról kérdezed` (a settled `belenéz`
  ige), a „never changes a file without your approval” pedig `a jóváhagyásod nélkül egyetlen fájlt sem változtat meg`, a
  `contentsRule` `amíg jóvá nem hagyod` zárásának párja. A régi `fájltartalmakat soha` és
  `csak olvasásra képes … soha semmit nem változtat` alakok kikerültek: az új angol se ígéri őket.

## A Rollback gomb két buboréksúgója (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Új felület: a gomb súgója most azt mondja meg, hogy EZ a visszagörgetés mit tesz a fájlokkal, a gomb pedig kikapcsol,
amint egy fájlrendszerek közti áthelyezés az utolsó lépéséhez ér (az eredetik eltávolítása, miközben minden már a
célhelyen van).

- **`rollbackTooltipStopAndMoveBack` → `Leállítás, és minden eddig áthelyezett fájl visszahelyezése`** · a testvér
  `rollbackTooltip` adja a keretet (`Leállítás, és …`), a `visszahelyez` pedig a katalógus bevett igéje a régi helyre
  való visszatérésre (`cancelRollback.doneMovingBack`: „mindent visszahelyezett”) · `high`. ❌ Nem `törlés`: egy
  áthelyezés visszagörgetése semmit sem töröl.
- **`rollbackAlreadyLandedTooltip`** · az első tagmondat a `cancelRollback.moveAlreadyLanded` képét veszi át („már a
  célhelyen van”), a `visszagörgetés` a rollback bevett szava (`rollbackUnavailableTooltip`), a `Mégsem` pedig a
  szomszédos gomb saját felirata (`fileOperations.button.cancel`), így ragozás nélkül áll a mondatban · `high`.
- A `Cmdrt` tárgyeset a márkanév kiejtés szerinti ragozása (style.md).

## A „Terminál megnyitása itt” és az appválasztója (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Új felület: egy kártya a `Viselkedés > Navigáció és fájlműveletek` alatt, ahol a parancs által indított terminálapp áll.
A listát a macOS építi; itt csak a feliratok fordulnak.

- **terminal (az appfajta) → `terminál`; Terminal (az Apple appja) → `Terminal`** · a magyar macOS megtartja az angol
  nevet (`Megnyitás a Terminalban`, `N67` kulcs a `macOS/Finder/LocalizableMerged.json`-ban), a köznévre viszont a
  Microsoft-terminológia `terminál`-t ad (`HUN`) · `high`. A kártyacím ezért `Terminál`; a tulajdonnév (`a Terminalban`)
  és a köznév (`terminál`) között itt fut a határ.
- **Open terminal here (a parancs neve) → `Terminál megnyitása itt`** · az Apple `Megnyitás a Terminalban` mintájára,
  köznévi `terminál`-lal · `high`. A parancs saját fordításának (menü, parancspaletta) pontosan ezt kell használnia.
- **Choose an app… → `App kiválasztása…`** · az Apple `Choose Application…` (`N137`) `Alkalmazás kiválasztása…`-t ad;
  `app`, mert a katalógus végig azt írja · `high`.
- **terminal app → `terminálapp`** · egybeírt összetétel · `high`.

## `Sort by relevance`: a találati oszlop elemleírása (`fileExplorer.columns.sortByRelevance`)

Új felület: a keresési találatokat mutató panel aktív oszlopfejlécének elemleírása. A következő kattintás visszaállítja
a keresőmotor saját sorrendjét, elöl a legjobb találattal.

- **relevance (mennyire illik egy találat a keresésre) → `relevancia`** · az Apple WorkflowKitje
  (`Relevance (WFSearchSortOrder)` → `Relevancia`), amely pontosan ugyanezt a fogalmat, egy keresési rendezési sorrendet
  nevezi meg · `tentative`. Az Apple magyar katalógusai négyfélét mondanak, közös szótő nélkül: `Relevancia`
  (WorkflowKit), `Fontosság` (Automator), `Találati pontosság` (AppStoreKit), `Témábavágóság` (Zene és TV), ezért marad
  `tentative`. A `relevancia` mellett szól, hogy a magyar felületek bevett szava a találati jóságra, és egyedül ez a
  fogalmat nevezi meg, nem egy szomszédosat (fontosság, pontosság). (macOS 26.6.2, 25G83 verzión ellenőrizve, a
  mellékelt honosítások `plutil`-kiírásával, 2026-09-06)

## `Documents and packages`: az új OOXML-sor (`settings.archives.ooxml.*`)

Új felület: egy sor ugyanabban a kártyában, ahol a `Zip archívumok` áll, az `Alkalmazáscsomagok` kártya fölött.
Szándékosan MINDKETTŐT lefedi: az Office-dokumentumokat (.docx, .xlsx, .pptx) és az alkalmazáscsomagokat (.jar, .apk),
ezért már az angol sem nevezi meg az Office-t.

- **documents (a fájlfajta) → `Dokumentumok`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Dokumentumok`; fájlfajták
  `RTF-dokumentum`, `Egyszerű szöveges dokumentum`) · `high`.
- **packages (általánosan, nem csak alkalmazások) → `csomagok`** · macOS Finder `Csomag tartalmának megjelenítése`, és a
  `terms.json` `app-bundle` / `package` bejegyzése · `high`. Szándékosan a puszta `csomagok`, hogy a sor tágabb
  maradjon, mint az alatta lévő `Alkalmazáscsomagok` kártya — ugyanaz a szétválasztás, amit az angol csinál a `packages`
  és az `app bundles` között.
- **A mondat kerete → `Mit tesz az Enter egy …, … vagy … fájlon.`** · pontosan a testvérkulcsok
  (`settings.archives.zip.description`, `settings.archives.bundle.description`) kerete · `high`.

## A szerverközpont: kapcsolódási állapotok, elutasítások, elfelejtés (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`.disconnect*`/`.forget*`)

Új felület: egy panelnézet, amely a szerverkapcsolat állapotát mutatja (kapcsolódás, elutasítás, leválasztás), plusz a
kötetváltó szerversorainak buboréksúgói és a két „elfelejtés” megerősítő párbeszéd.

- **Connecting to {name}… → `Kapcsolódás ide: {name}…`** · mac (Finder `LocalizableMerged` `MN1` = „Kapcsolódás ide:
  ^0…”, macOS 26.6.2) · `high`. Betű szerinti Apple-megfelelő ugyanerre az angol mondatra, és a kettőspontos alak
  megoldja a ragozási csapdát is (a `{name}` semmilyen toldalékot nem kap). Idézőjel nincs benne: a katalógus akkor
  idéz, ha az ANGOL is idéz (`servers.pinHint.body`: `"{command}"` → „{command}”), itt pedig nem.
- **Disconnect → `Leválasztás`; Cancel → `Mégsem`; Try again → `Próbáld újra`** · a szállított alakok nyernek
  (`fileExplorer.unreachable.disconnect`, `servers.paneState.disconnect`, `menu.network.disconnect`; 15+ `Mégsem`; 5
  `Próbáld újra`), és mind macOS-megerősített (Finder `MR10.1`/`N200` = „Leválasztás”, NetAuthAgent `CANCEL` = „Mégsem”
  pont a szerverre kapcsolódás lapján) · `high`. A `queue.row.cancel` `Megszakítás` alakja továbbra is a futó művelet
  megszakítása, nem párbeszédgomb; a szerverpanel gombja az utóbbi, tehát `Mégsem`.
- **server address → `szervercím`** · mac (Finder `ConnectToWindow` `YEA-3L-WnW.placeholderString` = „Szervercím”) ·
  `high`.
- **certificate → `tanúsítvány`; trust (megbízhatóság) → `megbízik benne` / `megbízható`** · mac (Kulcskarika-elérés
  `Localizable.loctable`: „egyéni megbízhatósági beállításokkal rendelkező tanúsítvány”, „megbízható alkalmazások
  listája”) · `high`.
- **host key → `a szerver kulcsa` / `{host} kulcsa`** · mac (`ActionKit.framework/Localizable.loctable`: „A hoszt
  kulcsának ujjlenyomata %@”, „SSH-kulcs”, „SSH-szerver”) · `high`. A birtokos rag a `kulcs`-ra kerül, nem a helyőrzőre,
  ezért a `{host}` ragozatlan marad: `{host} kulcsát`, `{host} kulcsa`.
- **trust a host key (a jóváhagyás aktusa) → `megbízhatónak tekint`** · a mac `megbízható` melléknévre építve · `high`.
  ❌ NEM `elfogad` (elmossa, hogy bizalmi döntésről van szó), és ❌ nem `visszavont` (az a `revoked`, lásd lent).
- **compromised (kulcsról) → `kompromittált`** · nincs Apple-forrás rá (a `veszélyeztetett` a hu rendszerben csak
  „endangered species” értelemben szerepel, a `visszavont` pedig a `revoked` szava: `spext_revoked` = „Visszavont”,
  `CERT_REVOKED_ERROR`) · `tentative`. A `kompromittál` régi, köznyelvben is meglévő magyar szó, tehát nem szakzsargon,
  és megőrzi az angol szándékos súlyát: a `hostKeyRevoked` végleges, nincs alóla visszaút. A `visszavont` gyengítene,
  mert adminisztratív aktust ír le, nem veszélyt (`review-queue.md`).
- **Keychain Access (az Apple appja) → `Kulcskarika-elérés`** · mac (a `Keychain Access.app` `InfoPlist.loctable`
  `CFBundleDisplayName` = „Kulcskarika-elérés”, és a saját `Localizable.loctable` prózája: „nyissa meg a dokumentumot a
  Kulcskarika-elérés appban”; macOS 26.6.2) · `high`. A support.apple.com weboldal `Kulcskarika-hozzáférés` alakja
  helyett az élő bundle a magasabb rendű forrás, mert a felhasználó azt látja; az `ai.secretError.keychainBody` is ezt
  viszi. A tár (store) jelentés marad `kulcskarika`.
- **Signed out (állapot) → `Kijelentkezve`; Saved (állapot) → `Mentve`** · a semleges, tárgyhoz igazodó állapotalak (nem
  „Ki vagy jelentkezve”), ahogy a nemsemlegességi szabály kéri · `high`.
- **Open X to Y → nem „hogy Y”, hanem célhatározós `-hoz/-hez/-höz`** · a szállított `askCmdr.renameReview.coverageThin`
  mintája (`Open the file to check the name.` = „Nyisd meg a fájlt a név ellenőrzéséhez.”) · `high`. Innen a három
  buboréksúgó egységes záró tagmondata: `az újbóli bejelentkezéshez`, `a kulcs megtekintéséhez`, `a kapcsolódáshoz`.
- **A „busy” súgó a testvére szerkezetét másolja.** `disconnectBusyTooltip` =
  `Nem választható le, amíg ezen a szerveren műveletek vannak folyamatban`, pontosan a szállított `ejectBusyTooltip`
  („Nem adható ki, amíg ezen az eszközön műveletek vannak folyamatban”) sablonjára, `eszköz` → `szerver` cserével ·
  `high`. A kettő egymás mellett jelenik meg ugyanabban a kötetváltóban.
- **Forget server → `Szerver elfelejtése`; Forget saved password → `Mentett jelszó elfelejtése`** · kényszerítve, mert
  az angol betű szerint azonos a szállított `menu.network.forgetServer` / `menu.network.forgetSavedPassword` /
  `fileExplorer.network.share.forgetPassword` kulcsokkal (`desktop-i18n-term-consistency`) · `high`.
- **A megerősítő kérdés tegező, és a helyőrző alaptagot kap.** `Elfelejted a(z) „{name}” szervert?`,
  `Elfelejted a(z) „{name}” mentett jelszavát?` — az `a(z)` a házi hedge az ismeretlen kezdőhangra, a `„…”` a
  felhasználó saját nevét jelöli, a birtokos rag pedig a `jelszó`-ra kerül (`jelszavát`), nem a helyőrzőre.
- **`{name} leválasztása` az aria-címke** · a szállított `ejectVolumeAriaLabel` („{name} kiadása”) mintája · `high`. A
  WCAG 2.5.3 tartalmazás a `leválasztás` ⊂ `leválasztása` részkarakterláncon áll (a látható címke a
  `servers.paneState.disconnect` / `fileExplorer.unreachable.disconnect` = `Leválasztás`).
- **`Cmdr couldn't …` → `A Cmdr nem tudta …`, a helyőrző kettőspont mögé** (`… ezt: {name}`) · a szállított család
  (`operationLog.rollback.refusalUnexpected`, `suggestedOps.destinationUnknown`,
  `fileExplorer.navigation.driveIndex.refusedUpgradeFailed`) · `high`.

## A szerverközpont táblázata és a kötetváltó rögzítése (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`/`.pinRefusedToast`/`.networkVolume`, `shortcuts.scope.servers`/`.places`)

28 kulcs: a kötetváltó „Servers” sora egy táblázatot nyit (Név / Típus / Cím / Állapot / Utolsó használat), alul egy
`Szerver hozzáadása…` sorral, plusz az öt parancspaletta-parancs, a rögzítés két buboréka és a billentyűparancs-lista
két új szakaszcíme.

- **Servers (sor- és szakaszcím) → `Szerverek`** · mac (`AppKit.framework/Menus.loctable` `Servers`,
  `Sharing.framework/Localizable.loctable` `Servers`, `CoreServices/SystemFolderLocalizations` `Servers` — ez utóbbi
  maga a macOS „Servers” mappájának neve) · `high`. A `server → szerver` szótári döntés többes száma; a `kiszolgálók` a
  Microsoft-ág, nem a miénk. Ugyanez az érték áll a `fileExplorer.navigation.networkVolume` és a
  `shortcuts.scope.servers` kulcsban (az angoljuk betű szerint azonos, `desktop-i18n-term-consistency`). ❗ A CSOPORT,
  amelyben a sor ül, továbbra is `Hálózat` (`fileExplorer.navigation.groupNetwork`) — a két kulcs most vált szét.
- **Places (a szerveren belüli helyek listája: SMB-megosztások, később tárolóvödrök) → `Helyek`** · mac (Maps
  `[PlaceList] Places`, Photos `IPXPlaceBrowserTabLabel`, Journal `Places` — 31 találat, mind `Helyek`) · `high`. A
  macOS-találatok földrajzi értelműek, de a `Places` egyszavas, általános fogalomcímke, és a magyar `Helyek` ugyanígy
  általános; a korábbi `Megosztásböngésző` a régi, SMB-re szűkített angol címet fordította, és nem bírja el a
  tárolóvödröket.
- **Address (oszlopcím) → `Cím`** · mac (39 találat, például Weather `Address`), és a Finder `ConnectToWindow`
  `Szervercím` alakjának alaptagja · `high`. A `Név` melletti `Cím` névjegykártya-olvasata pont a kívánt jelentés; a
  teljes `Szervercím` az oszlopban fölösleges, mert a táblázat egésze a szerverekről szól.
- **Last used (oszlopcím) → `Utolsó használat`** · mac (`PreferencePanes/Security.prefPane/Localizable.loctable`
  `Last Used`, 5 találat — ugyanez a szerep: oszlopcím egy táblázatban) · `high`. ⚠️ Tudatos eltérés a fájllista
  dátumoszlopainak `-va/-ve` mintájától (`Módosítva`, `Létrehozva`): ott nincs Apple-forrás az adott szóra, itt viszont
  betű szerinti Tier-1 találat van ugyanerre az angol oszlopcímre, és az erősebb bizonyíték nyer. Nem
  `Utoljára használva`.
- **Never (az „Utolsó használat” cellában) → `Soha`** · mac (87 találat, például `FamilyOutOfProcessUIExtension`
  `SHARE_AGE_OPTION_NEVER_SHARE_TITLE`; a PassKit `SE_STORAGE_CLEANUP_LAST_USED_NEVER` = „Legutóbbi használat: soha”
  pont ebben a szerepben) · `high`.
- **Found nearby (állapot) → `A közelben felfedezve`** · mac (`IOBluetoothUI.framework` `PROX_PAIRING_OPTIONS_HEADER%@`
  = „„%@” a közelben felfedezve” — ugyanaz a fogalom: a rendszer a közelben észlelt egy eszközt) · `high`. A `felfedez`
  tő a katalógusban már él (`fileExplorer.network.browser.cannotRemoveDiscovered` = `A felfedezett gépek…`,
  `settings.network.permissionWithout` = `nem fedezhetsz fel új szervereket`), és a `-va/-ve` alak illeszkedik a többi
  állapothoz (`Kapcsolódva`, `Mentve`, `Kijelentkezve`).
- **local network discovery → `helyi hálózati felderítés`** · a katalógus szállított alakjai
  (`settings.network.firstTriggerDone.label` = `Hálózati felderítés elindult`, `settings.network.enabled.description` =
  `SMB-szerverek felderítése a helyi hálózaton`) · `high`. A kikapcsolt állapot mondata a szállított
  `fileExplorer.navigation.driveIndex.tooltipDisabled` sablonját másolja (`Az indexelés ki van kapcsolva …`), innen
  `A helyi hálózati felderítés ki van kapcsolva.` és a hozzá tartozó link `Kapcsold be a Beállításokban`. A macOS Tier-1
  gombalakja `Bekapcsolás`, de ez nem gomb, hanem a mondat folytatása, ezért tegező felszólítás.
- **Pin / unpin server → `Szerver rögzítése / rögzítés feloldása`** · mac (Notes `Pin or Unpin Notes` = „Jegyzetek
  rögzítése vagy rögzítés feloldása”, `Unpin Note` = „Jegyzet rögzítésének feloldása”) · `high`. Betű szerinti
  Apple-minta ugyanerre a kettős szerkezetre, és egybevág a szótár `pin / unpin tab` sorával (`Lap rögzítése` /
  `Lap rögzítésének feloldása`). A perjel marad, ahogy az angolban, mert egy parancs mindkét irány. ❌ Nem
  `Rögzítés megszüntetése` (a Music-ág alakja): a `feloldása` az, amit a szótár már visz.
- **volume switcher → `kötetválasztó`** · a katalógus szállított, látható alakja (`commands.volumeClose.label` =
  `Kötetválasztó bezárása`, `fileOperations`-beli `kötetválasztóban`, `shortcuts.scope.volumeChooser` = `Kötetválasztó`)
  · `high`. Az angol két néven hívja ugyanazt a felületet („volume chooser” és „volume switcher”); a magyar egy néven. A
  `commands.favoritesAdd.description` puszta `váltó` alakja egyedi, ne terjeszd.
- **A rögzítés két buborékában a `{name}` alanyi helyen áll, toldalék és névelő nélkül**:
  `{name} mostantól ott van a kötetválasztódban.` / `{name} kikerült a kötetválasztódból. Továbbra is mentve van.` · a
  szállított `servers.refusal.timedOut` (`{host} nem válaszolt időben.`) mintája · `high`. Így semmi nem függ a beszúrt
  érték kezdőhangjától vagy hangrendjétől. A második mondat a `Mentve` állapotcímkére rímel, ez viszi az „semmi nem
  veszett el” jelentést.
- **`Cmdr couldn't change where {name} shows.` → `A Cmdr nem tudta megváltoztatni, hogy {name} hol jelenjen meg.`** · a
  `A Cmdr nem tudta …` nyitány a szállított család kötelező alakja (`operationLog.rollback.refusalUnexpected`,
  `suggestedOps.destinationUnknown`) · `high`. Itt a `{name}` nem tárgy, hanem egy `hogy`-os mellékmondat alanya, ezért
  nem kell a családi `… ezt: {name}` kettőspontos kitérő: toldalék így sem kerül rá.
- **Waiting for you to check the key → `Arra vár, hogy ellenőrizd a kulcsot`** · az `ellenőrizd` ige a szállított
  `ai.translateError.authFailed.body` (`Ellenőrizd a kulcsodat…`) alakja, a `kulcs` pedig a szótár
  `host key → a szerver kulcsa` sorának alaptagja · `high`. Az `arra` utalószó nélkül a mondat csonka lenne; a
  szerversoron belül a puszta `a kulcsot` egyértelmű, ahogy az angol `the key` is.
- **Kényszerített, mert az angol betű szerint azonos egy szállított kulcséval** (`desktop-i18n-term-consistency`):
  `Name` → `Név` (`fileExplorer.columns.name`), `Type` → `Típus` (`queryUi.ai.filter.type`), `Status` → `Állapot`
  (`licensing.section.labelStatus`), `Connected` → `Kapcsolódva` (`fileExplorer.network.browser.status.connected`,
  `ai.cloud.connected`), `Forget saved password` → `Mentett jelszó elfelejtése` (`menu.network.forgetSavedPassword` és
  két testvére). Mind egybevág a Tier-1 forrásokkal is (mac: `Név`, `Típus`, `Állapot`; AirPort Utility `connected.1` =
  `Kapcsolódva`).
- **Saved (állapot) → `Mentve`; Signed out (állapot) → `Kijelentkezve`** · a fenti § A szerverközpont passz döntése,
  most szállítva; a `Mentve` alakot a Preview `SignatureSaved` (`Mentve`) is hozza, a `Kijelentkezve` töve a mac
  `Sign Out` = `Kijelentkezés` (54 találat) · `high`. A melléknévi `Mentett` (Podcasts `LIBRARY_SAVED_EPISODES`) jelzői
  szerepre való, nem állapotcellába.
- **Parancscímkék főnévi alakban**, a szótár címkeszabálya és a szállított minták szerint: `Szerverek megjelenítése`
  (`commands.appShowAll.label` = `Összes megjelenítése`), `Szerver leválasztása` (`leválasztás` szótári alak),
  `Szerver szerkesztése…` (`fileExplorer.functionKeyBar.editAction` = `Fájl szerkesztése`), `Szerver hozzáadása…`
  (`menu.go.addToFavorites` = `Hozzáadás a kedvencekhez`). A `…` (U+2026) mindkét helyen megmarad.
- **Az üres állapot a szállított `askCmdr.sessions.empty` (`No chats yet` = `Még nincs csevegés`) mintáját követi**:
  `Még nincs szerver`. A magyar számnév nélküli főnév egyes számban áll, tehát `szerver`, nem `szerverek`.
- **A `Mac` tárgyesete `Macet`, kötőjel nélkül** · mac (161 találat, például „nem képes törölni ezt a Macet”) · `high`.
  Illeszkedik a katalógus `Macen` alakjához. A `NAS` betűszó marad, ragja kötőjeles (`NAS-t`), ahogy az
  `onboarding.stepOptional.networking.desc` `NAS-hoz` alakja.
- **`servers.hub.rowCount`: mindkét ág `{countText} szerver`** · a magyar számnév után a főnév egyes számú (style.md §
  Plurals) · `high`.

## A szerverlap, az SSH-kulcs jóváhagyása és a Go-to-path előnézete (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`.signIn`/`.hostKeyChanged*`, `goToPath.dialog.opensServer`/`.addsServer`, `commands.serversConnect.label`)

46 kulcs: az „Add server” lap (SMB / SFTP / WebDAV), a benne élő SSH-hosztkulcs-jóváhagyás, a panelnézet két új
állapota, plusz két Go-to-path előnézetsor és egy parancspaletta-parancs.

- **fingerprint (kulcs ujjlenyomata) → `ujjlenyomat`**; a `Key fingerprint` címke `A kulcs ujjlenyomata` · mac
  (`ActionKit.framework/Localizable.loctable`: „A hoszt kulcsának ujjlenyomata %@”, `Security.framework/Certificate`
  `Fingerprints` = „Ujjlenyomatok”) · `high`. A birtokos szerkezet Apple-mintájú, és a névelő azért marad benne, mert a
  címke egy megjelenített ÉRTÉK fölött áll, nem beviteli mező mellett.
- **trust (a kulcs jóváhagyásának GOMBJA) → `beállítás megbízhatóként`** · mac (`UsersGroups.appex` `Trust` = „Beállítás
  megbízhatóként”, `SecurityInterface` `Always Trust` = „Mindig legyen megbízható”) · `high`. Innen a három összetartozó
  sor: `Beállítás megbízhatóként és csatlakozás`, `Az új kulcs beállítása megbízhatóként`, és a prózazáró
  `majd állítsd be megbízhatóként`. A szótár korábbi `megbízhatónak tekint` sora ÉRVÉNYBEN MARAD a kijelentő prózára
  (`servers.refusal.hostKeyUntrusted` = „A Cmdr még nem tekinti megbízhatónak {host} kulcsát.”): a fogalmat mindkettőben
  a `megbízható` melléknév viszi, csak az ige más a gombon és a tényközlésben. ❌ Nem `elfogad`.
- **man in the middle („something is sitting between you and it”) → `valami beékelődött közéd és a szerver közé`** · mac
  (`ActionKit`: „Ez utalhat egy közbeékelődéses támadásra”) · `high`. Az Apple `közbeékelődéses` tövét visszük, de a
  mondat a miénk marad: tegező, és nem nevezi meg a támadást, ahogy az angol sem.
- **owner (a szerver gazdája) → `tulajdonos`** · mac (FindMy `Owner` = „Tulajdonos”, CloudDocs „a megosztott mappa
  tulajdonosa”) · `high`. `a szerver tulajdonosától kaptál`, `egyeztesd … a szerver tulajdonosával`.
- **Connecting… → `Csatlakozás…`, a LAPON** · mac (Home `HUDropIn_Label_Connecting_State` = „Csatlakozás…”, 14 találat)
  · `high`. ⚠️ **Tudatos kettősség, ne söpörd össze**: a PANEL nyitósora a szállított `servers.paneState.connecting` =
  `Kapcsolódás ide: {name}…` (Finder `MN1`), a LAP főgombja viszont a `servers.sheet.connect` = `Csatlakozás` alakot
  veszi fel, mert az angol `Connect` betű szerint azonos a szállított `fileExplorer.network.connect` kulccsal
  (`desktop-i18n-term-consistency`), és ugyanaz a gomb írja át magát `Connecting…`-ra. Ha itt `Kapcsolódás…` állna, a
  felhasználó a saját szeme előtt látná a gombot tövet váltani. A lap többi kapcsolódás-szava ezért végig `csatlakoz-`:
  `Első csatlakozás ide: {host}`, `Csatlakozás vendégként`, `Beállítás megbízhatóként és csatlakozás`,
  `Automatikus újracsatlakozás` (ez utóbbi a szállított `servers.paneState.reconnecting` =
  `Újracsatlakozás ide: {name}…` tövével is egyezik).
- **Reconnect automatically → `Automatikus újracsatlakozás`** · mac (`DisplaysSettingsIntentsExtension`: „Az
  »Automatikus újracsatlakozás bármely közeli Machez vagy iPadhez« beállítás…”) · `high`. Betű szerinti Apple-alak
  ugyanerre a beállításnévre.
- **Remember in Keychain → `Megjegyzés a kulcskarikában`** · mac (Music `Remember password` = „Jelszó megjegyzése”,
  `IMDaemonCore` „Remember this password in my keychain” = „Jelszó megjegyzése a kulcskarikámon”, AE `Authentication`
  „Add to keychain?” = „Hozzáadás a kulcskarikához?”) · `high`. Az Apple-sorok kiteszik a `jelszó` tárgyat, mi
  **tudatosan nem**: a jelölőnégyzet SFTP-nél kulcsjelmondatot is elmenthet, és az angol is épp ezért általános. A
  `kulcskarika` a tár jelentése, tehát kisbetűs és ragozódik (`terms.json` `keychain`); az APP neve marad
  `Kulcskarika-elérés`. A `servers.sheet.needsStoredSecret` betű szerint ezt a címkét idézi:
  `a „Megjegyzés a kulcskarikában” beállítást`.
- **Key passphrase → `Kulcsjelmondat`; Key file → `Kulcsfájl`** · mac (`Security.framework/OID` `Passphrase` =
  „Jelmondat”, `SecErrorMessages` -25293 „user name or passphrase” = „felhasználónév vagy jelmondat”; az összetétel
  mintája a `DiskManagement` „Disk Passphrase” = „lemezjelszó”) · `high`. A `jelmondat` az, ami elválasztja a fiók
  `Jelszó`-jától, pont ahogy az angol `passphrase` a `password`-től. Mindkettő egybeírt összetétel, mert a lap többi
  mezőcímkéje is puszta főnév (`Felhasználónév`, `Jelszó`, `Név`, `Cím`, `Távoli mappa`).
- **Protocol → `Protokoll`** · mac (AirPort Utility `moMP`) · `high`. **Remote folder → `Távoli mappa`** · mac (Finder
  `KIND_FORMATTER_29_1` `Remote` = „Távoli”, `Sharing.appex` „Remote Login” = „Távoli bejelentkezés”) · `high`.
  **Browse… → `Böngészés…`** · mac (`AppleAccountUI` `PROFILE_BROWSE_PHOTO` „Browse...” = „Böngészés…”) · `high`. **Save
  → `Mentés`** · mac (Finder `LocalizableMerged` `AL2`, 171 találat) · `high`. **Guest → `Vendég`** · mac
  (`NetAuthAgent` `GUEST` — épp a szerverre kapcsolódás lapja) · `high`.
- **Kényszerítve, mert az angol betű szerint azonos egy szállított kulcséval** (`desktop-i18n-term-consistency`):
  `Connect` → `Csatlakozás` (`fileExplorer.network.connect`), `Sign in` → `Bejelentkezés`
  (`fileExplorer.network.signIn`), `Password` → `Jelszó` (`fileOperations.archivePassword.placeholder`), `Name` → `Név`
  (`fileExplorer.columns.name`), `Address` → `Cím` (`servers.hub.colAddress`), `Cancel` → `Mégsem` (20 kulcs),
  `Advanced` → `Speciális` (`settings.section.advanced`), `Connect to server…` → `Kapcsolódás szerverre…`
  (`settings.network.permissionIntroConnectLink`, egyben a Finder saját menüparancsa). Mind egybevág a Tier-1
  forrásokkal is. A `Username` → `Felhasználónév` a lap egyetlen mezőcímkéje, amelynek nincs testvére máshol a
  katalógusban, tehát nem kényszerített; a mac forrás viszont ugyanide mutat (`Security.framework` `SecErrorMessages`
  -25293 „user name or passphrase” = „felhasználónév vagy jelmondat”).
- **A helyőrző sehol nem kap toldalékot.** `Bejelentkezés ide: {name}` (mac `iCloud.app/CloudKit` `Sign In to %1$@` =
  „Bejelentkezés: %1$@”, plusz a szállított `ide:` idióma), `Első csatlakozás ide: {host}`,
  `Kijelentkezve innen: {name}`, `A Cmdr leállította a kapcsolódást ide: {name}`, `Megnyitja ezt: {name}`. A két kivétel
  alanyi/birtokos helyen áll, ahol amúgy sem kellene rag: `{name} szerkesztése`, `{host} kulcsa megváltozott` (a
  birtokos rag a `kulcs`-ra megy, § A szerverközpont).
- **`Kijelentkezve innen: {name}`, nem `Kijelentkeztél…`** · a szállított `servers.hub.status.signedOut` =
  `Kijelentkezve` állapotalakja, és a testvér panelcím `Kapcsolódás ide: {name}…` névszói mintája · `high`. Így a panel
  három nyitósora egy család, és a semleges, tárgyhoz igazodó állapotalak marad (nemsemlegességi szabály).
- **`I’ve checked it` → `Ellenőriztem`** · a szállított `ai.translateError.authFailed.body` (`Ellenőrizd a kulcsodat…`)
  igetöve · `high`. Egyes szám első személy, mert az angol is a FELHASZNÁLÓ szava; ez a katalógus egyetlen ilyen sora,
  és a lenyíló mögé rejtett gomb súlyát ez adja.
- **`Adds a server` → `Hozzáad egy szervert`; `Opens {name}` → `Megnyitja ezt: {name}`** · mac (Journal „Adds a title to
  a journal entry.” = „Hozzáad egy címet a naplóbejegyzéshez.”, Notes „Opens an existing folder…” = „Megnyit egy meglévő
  mappát…”) · `high`. Egyes szám harmadik személy, kijelentő jelen idő, ahogy az angol előnézetsor.
- **`ssh`, `ssh-agent`, `SFTP`, `WebDAV`, `Nextcloud` marad angolul.** A parancsnév kisbetűs, összetételben kötőjeles
  (`ssh-sor`), a névelője `az` (esz-esz-há), ahogy a szótár `adb`-sora előírja: `Az ssh-agent használata`. A
  tulajdonnév + köznév összetétel az AkH szerint kötőjeles: `Nextcloud-cím`.
- **`sameAsSourceJustification` ebben a passzban négy kulcson**: `servers.sheet.protocolSmb` / `.protocolSftp` /
  `.protocolWebdav` (protokoll-betűszavak; a magyar macOS is változatlanul hozza őket, és a szállított
  `servers.refusal.notAWebdavServer` is `WebDAV-on` alakban ragozza) és `servers.sheet.addressPlaceholder` (`nas.local`,
  egy beviteli mező példa-gépneve, nem lefordítandó szöveg). A többi 42 érték eltér az angoltól. Aposztróf egyik magyar
  értékben sincs, tehát ICU-kettőzés sem kellett.

## Az automatikus újracsatlakozás és a kulcsos bejelentkezés panelsora (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Két kulcs: a panelnézet címe, amíg a Cmdr magától visszaszerez egy megszakadt szerverkapcsolatot (alatta pörgő, a
következő próbáig tartó visszaszámlálás, és az `Újra most` / `Mégsem` / `Leválasztás` gombok), plusz a
`Kijelentkezve innen: {name}` cím alatti egy sor, amely a `Bejelentkezés…` gomb HELYETT jelenik meg, ha a szerver
kulccsal (SSH-kulcs vagy ssh-agent) azonosít, tehát nincs mit begépelni.

- **reconnect (a magától helyreálló hálózati kapcsolat) → `újracsatlakozás`, NEM `újrakapcsolódás`** · a katalógus
  szállított családja (`errors.listing.deviceReconnecting.title`: `Reconnecting to the device` =
  `Újracsatlakozás az eszközhöz`; `fileExplorer.unreachable.detailGaveUp`: `tried reconnecting` =
  `megpróbált újracsatlakozni`; `indexing.enrich.pausedDisconnected` = `újracsatlakozáskor folytatódik`;
  `servers.sheet.autoReconnect` = `Automatikus újracsatlakozás`) · `high`. ⚠️ A macOS ad Tier-1 találatot a másik tőre
  is (`ScreenSharing.framework` `reconnectingMessage`: `Reconnecting…` = `Újrakapcsolódás…`, `ConversationKit`
  `Reconnecting` = `Újrakapcsolódás`, `NetworkExtension` `Try reconnecting.` = `Próbáljon újrakapcsolódni.`), de a
  szállított alak nyer, és itt ez különösen erős: a cím alatt EGYSZERRE látszik a két testvérkulcs, amely már
  `újracsatlakoz-` tövű (`retryProgressAriaLabel` = `Idő a következő újracsatlakozási próbáig`, `retryNowTooltip` =
  `Azonnali újracsatlakozás.`). Egy nézeten belüli tőváltás rosszabb, mint két nézet közötti.
- **`Reconnecting to {name}…` → `Újracsatlakozás ide: {name}…`** · a tő a fenti sorból, a keret a panelcímek szállított
  idiómája (`servers.paneState.connecting` = `Kapcsolódás ide: {name}…`, Finder `LocalizableMerged` `MN1`) · `high`. A
  panelcímeket nem a TŐ köti össze (az angoljuk is más: `Connecting` vs `Reconnecting`), hanem az `ide: {name}…` keret,
  amely egyben a ragozási csapdát is megoldja. Az Apple maga is kettősponttal kerüli ki a helyőrző ragozását
  (`HomeDataModel` `Reconnect HomePod to “%@”` = `HomePod ismételt csatlakoztatása a következőhöz: „%@”`, Home
  `Reconnect in %@` = `Újracsatlakozás: %@`).
- **`This server signs in with a key rather than a password…` →
  `Ez a szerver jelszó helyett kulcsot használ a bejelentkezéshez…`** · a mondatkezdet a szállított testvérek kötelező
  alakja (`servers.refusal.needsCredentials` = `Ez a szerver jelszót kér.`, `servers.refusal.authMethodUnsupported` =
  `Ez a szerver olyan bejelentkezési módot használ, amit a Cmdr még nem támogat.`) · `high`. A SZERVER marad az alany,
  nem a felhasználó, és a `használ … a bejelentkezéshez` szerkezet elkerüli, hogy a szerver „jelentkezzen be” (a magyar
  ige így visszahatóan olvasódna). ❌ Ne `Ehhez a szerverhez … jelentkezel be`: a `Ez a szerver …` a család mintája.
- **rather than / instead of → `helyett`, a kiváltott dolog ELŐTT** · mac (`CommerceKit`
  `TOUCHID_SETTINGS_FAILED_CONTINUE_BUY`: `will use your Apple Account and password instead of Touch ID` =
  `a Touch ID helyett az Apple-fiókja és a jelszava lesz használva`) · `high`. Innen `jelszó helyett kulcsot`. Így a
  mondat nem lesz „Y, nem X” alakú tagadás, ami a házi hangban kerülendő.
- **there's nothing to type → `nincs mit beírnod`** · a `Nincs mit + főnévi igenév` szerkezet Apple-attesztált
  (`AppleIDSetup` `QR_CODE_SCANNING_DEVICE_TOAST_NO_COPY`: `Nothing to copy` = `Nincs mit másolni`, Calendar
  `Printing.loctable`: `There is nothing to print.` = `Nincs mit kinyomtatni.`), a `beír` ige pedig a katalógus
  szállított szava a `type`-ra (`onboarding.cloudSetup.apiKeyPlaceholder.saved`: `Type a new one to replace it.` =
  `Írj be egy újat a cseréjéhez.`) · `high`. A `-d` személyragot azért tesszük ki (`beírnod`), mert a mondat tegez; az
  Apple ragtalan alakja az önöző regiszteréből jön, nem érv ellene.
- **`Open it again to retry.` → `Nyisd meg újra a kapcsolódáshoz.`** · betű szerinti szállított minta
  (`fileExplorer.navigation.connectionTooltipSaved`: `Saved. Open it to connect.` =
  `Mentve. Nyisd meg a kapcsolódáshoz.`), plusz a szótár „Open X to Y → célhatározós `-hoz/-hez/-höz`” sora és a
  szállított `servers.paneState.hostKeyChangedHint` (`… nyisd meg újra az ujjlenyomat ellenőrzéséhez.`) · `high`. A
  `retry` ismétlését az igei `újra` viszi, ezért a célhatározó a puszta `kapcsolódás`; az `az újrapróbálkozáshoz` csak
  megduplázná az `újra` tövet. Nem `az újbóli bejelentkezéshez` (az a
  `fileExplorer.navigation.connectionTooltipNeedsSignIn` jelszavas esete, ahol tényleg be KELL jelentkezni).
- **Nyitott követés**: a szerverpanel `Kapcsolódás` (`kapcsolód-`) és `Újracsatlakozás` (`csatlakoz-`) töve tudatosan
  eltér, mert az angoljuk is más szó és mindkettőnek külön szállított horgonya van. Ha egy későbbi passz egységesíteni
  akarja, a `retryProgressAriaLabel` / `retryNowTooltip` / `servers.sheet.autoReconnect` hármast kell hozzáigazítania,
  nem ezt az egy címet.

## A rögzítési tipp, a megbízható szerverkulcsok lapja és az ADB-állapotsor (`menu.network.pinToSwitcher`/`.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`.adb`, `settings.summary.servers`/`.adb`, `settings.behavior.serversPinHintSeen.*`, `settings.appearance.tintSmb.*`)

31 kulcs: a kötetváltó szerversorának két új helyi menüpontja, az egyszeri „hosszú a Hálózat csoportod” értesítés négy
sora, a Beállítások két új alszakasza (`Szerverek (SFTP, WebDAV)` és `Android (ADB)`) minden szövegével, plusz a
szerverpanel-színezés két átcímkézett kulcsa.

- **Unpin (puszta, helyi menüben) → `Rögzítés feloldása`** · mac (`NotesShared.framework/Localizable.loctable` `Unpin` =
  „Rögzítés feloldása”; `AppKit/MenuCommands.loctable` `Unpin Tab` = „Lap rögzítésének feloldása”) · `high`. Betű
  szerinti Apple-találat pont erre az egyszavas menüpontra, és egybevág a szótár `pin / unpin tab` sorával, valamint a
  szállított `commands.serversTogglePin.label` (`Szerver rögzítése / rögzítés feloldása`) második felével. ❌ Nem
  `Rögzítés megszüntetése` (a Music/Maps-ág alakja): a `feloldása` az, amit a katalógus már visz.
- **Pin to switcher → `Rögzítés a kötetválasztóban`** · mac (`WorkflowUI.framework/Localizable.loctable`
  `Pin in Menu Bar` = „Rögzítés a menüsoron” — ugyanaz a szerkezet: `Rögzítés` + a felület helyhatározós neve) · `high`.
  A `kötetválasztó` a szótár szállított alakja, a `-ban` rag pedig a szállított
  `fileExplorer.navigation.serverPinnedToast` (`… ott van a kötetválasztódban`) esete. Az angol elhagyja a névelőt, a
  magyar nem teheti.
- **A tipp címe birtokos, mert a szállított buborék is az**: `Kezd hosszúra nyúlni a Hálózat csoportod` · a
  `serverPinnedToast` `kötetválasztódban` alakja · `high`. A `Hálózat` a kötetváltó csoportcímkéje
  (`fileExplorer.navigation.groupNetwork`), változatlanul; a `kezd hosszúra nyúlni` a semleges, nem figyelmeztető
  olvasat („is getting long”), ahogy az angol is kéri.
- **A tipp törzse a szállított mintákat fűzi össze**: `Kattints jobb gombbal …`
  (`fileExplorer.navigation.favoriteTooltip`), `válaszd a „…” lehetőséget`
  (`settings.behavior.openTerminalHereApp.description`), `futtasd a parancspalettából a(z) „{command}” parancsot`
  (`main.upgradeNudge.other` = „nyisd meg a parancspalettát, és futtasd a Bevezető… parancsot”), `Továbbra is …`
  (`fileExplorer.navigation.serverUnpinnedToast`) · `high`. A záró mondat alanya elmarad, ahogy az angolban is; a
  `Továbbra is ott marad a Szerverek listában.` viszi az „semmi nem veszett el” jelentést. A `{command}` elé `a(z)`
  kerül, mert a beszúrt parancsnév kezdőhangja ismeretlen.
- **Got it → `Értem`** · kényszerítve, mert az angol betű szerint azonos három szállított kulcséval (`ai.toast.gotIt`,
  `updates.moveToApplicationsDialog.gotIt`, `main.oldMacos.gotIt`), `desktop-i18n-term-consistency` · `high`.
- **host key (a kártya és a lap főneve) → `szerverkulcs`** · a szótár `host key → a szerver kulcsa` sorának egybeírt
  összetétele · `high`. A birtokos szerkezet marad, ahol van birtokos (`{host} kulcsának ujjlenyomata`); ahol az angol
  puszta többes szám áll („Trusted host keys”), ott az összetétel a természetes: `Megbízható szerverkulcsok`. ❌ Nem
  `hosztkulcs`: a `hoszt` Apple-attesztált ugyan (`hosztnév`, „Tanúsítvány kérése a hoszttól”), de a felhasználó felé a
  katalógus egységesen `szerver`-t mond.
- **Trusted X → `Megbízható X`** · mac (`Network.appex` `8021X_PROFILE_TRUSTED_SERVER_LABEL` `Trusted servers` =
  „Megbízható szerverek”, `8021X_PROFILE_TRUSTED_CERTIFICATES_LABEL` = „Megbízható tanúsítvány”) · `high`.
- **A megbízhatóvá tétel IGÉJE a szállított `megbízhatónak tekint`** (`servers.refusal.hostKeyUntrusted` = „A Cmdr még
  nem tekinti megbízhatónak {host} kulcsát.”), ezért a lap prózája is ezt viszi:
  `mielőtt megbízhatónak tekintené egy szerver kulcsát`, `A megbízhatónak tekintett SSH-szerverkulcsok.`,
  `Még nincs megbízhatónak tekintett kulcs.` · `high`.
- **⚠️ Egy kivétel: a dátum előtti `Trusted` címke `Megbízhatóként jelölve`** · mac
  (`SecurityInterface.framework/Localizable.loctable`: „Ez a tanúsítvány … megbízhatóként lesz jelölve”) · `high`. A
  `tekint` igéből nincs használható `-va/-ve` alak egy dátum elé („Megbízhatónak tekintve 2026. 09. 07.” nem magyar), az
  Apple viszont pont a „megjelölés megbízhatóként” aktusára ad Tier-1 alakot, és ez az, amit a sor rögzít: mikor döntött
  így a felhasználó. A fogalmat mindkét alakban a `megbízható` melléknév viszi, ezért a lap nem esik szét. ❌ Nem
  `Jóváhagyva` (a `PermissionKit` `Approved` alakja): elveszne belőle a bizalmi döntés.
- **Forget (puszta gomb) → `Elfelejtés`** · mac (`BluetoothUIServer.app/Localizable.loctable`
  `kBTUIServerUSBPairedWhileLoggedOffActionButtonTitle` `Forget` = „Elfelejtés”) · `high`. Ugyanaz a tő, mint a
  szállított `menu.network.forgetServer` (`Szerver elfelejtése`) családban, ahogy az angol `@key` kéri.
- **A megerősítő párbeszéd a szállított „elfelejtés” család mintája**: `Elfelejted ezt a kulcsot?` +
  `A következő kapcsolódáskor a Cmdr megmutatja {host} kulcsának ujjlenyomatát, és rákérdez, megbízol-e benne.` · a
  szállított `fileExplorer.navigation.forgetSecretConfirm`
  (`Elfelejted a(z) „{name}” mentett jelszavát? A Cmdr a következő kapcsolódáskor újra elkéri.`) · `high`. A `{host}`
  birtokos helyen áll, a rag a `kulcs`-ra megy, tehát a helyőrző ragozatlan marad; ez oldja meg a „connect to {host}”
  ragozási csapdáját is.
- **connect to a server → `kapcsolódás szerverre`** · a szótár szállított döntése (Finder `N84` = „Kapcsolódás
  szerverre…”) · `high`. Innen `Amikor először kapcsolódsz egy SFTP-szerverre, …`.
- **Status → `Állapot`; Browse… → `Böngészés…`** · kényszerítve, mert az angoljuk betű szerint azonos a szállított
  `servers.hub.colStatus` / `licensing.section.labelStatus`, illetve `servers.sheet.browse` kulcsokéval · `high`.
- **Not found → `Nem található`** · mac (`AppKit.framework/FindPanel.loctable` `Not found` = „Nem található”) · `high`.
  Egybevág a szállított `fileExplorer.network.share.notFound` alakjával.
- **Found at {path} → `Megtalálva itt: {path}`** · a `megtalálva` a szállított `indexing.summary.found`
  (`{countText} megtalálva`) és `askCmdr.tool.importantFolders.done` alakja, az `itt: {x}` pedig a katalógus házi
  idiómája a ragozhatatlan helyőrzőre (`fileExplorer.network.share.notFound` = „… nem található itt: {hostName}”) ·
  `high`.
- **Re-check → `Újraellenőrzés`** · mac (`TextToSpeechVoiceBankingUI.framework` `VB_CHECK_AGAIN` `Check Again` =
  „Újraellenőrzés”) · `high`. A `SoftwareUpdate` `Ismételt ellenőrzés` alakja is Tier 1, de két szó, és ez a gomb `mini`
  méretű. A `settings.adb.install.intro` betű szerint idézi a gombcímkét: `majd nyomd meg az Újraellenőrzés gombot:` (a
  `nyomd meg a … gombot` keret a szállított `downloads.toast.inAppHint` és `mtp.ptpcameradDialog.helpText` alakja).
- **watch (Cmdr figyeli a csatlakozó eszközöket) → `figyel`** · a katalógus szállított töve
  (`settings.advanced.card.fileWatching` = `Fájlfigyelés`, `settings.askCmdr.proactive.description`:
  `Cmdr watches the folders you work in` = `A Cmdr figyeli azokat a mappákat, amikben dolgozol`, `downloads.fda.message`
  = `… a Letöltések mappa figyeléséhez.`) · `high`. Innen `A Cmdr figyeli a csatlakozó telefonokat.` és
  `A Cmdr most nem figyeli a csatlakozó telefonokat.` A két sor szándékosan egy mondatpár: az angol is csak a tagadásban
  és a `right now`-ban tér el. Az ADB-szerver, az előfizetés és a socket egyikben sem jelenik meg, ahogy az `@key` kéri.
- **A `Cmdr` alanyt kitesszük mindkét figyelő sorban**, mert az angol alanytalan mondata („Watching for phones.”)
  magyarul gazdátlan harmadik személy lenne egy olyan sorban, amely fölött csak az `Állapot` címke áll · `high`.
- **Az adb-sorok a szállított `settings.fileOperations.adb*` kulcsok szóhasználatát viszik tovább**:
  `Az adb keresése a szokásos módon` (a `settings.fileOperations.adbBinaryPath.description` = „a Cmdr a szokásos módon
  keresi az adb-t”), `Az adb parancs kiválasztása` (a `kiválasztása` a szállított
  `settings.behavior.openTerminalHereApp.description` `„App kiválasztása…”` alakja, plusz a mac `X kiválasztása`
  mintája), `Telepítsd az Android platform tools csomagot` (a szótár `platform tools` sora),
  `be van kapcsolva az USB-hibakeresés` (a szótár `USB debugging` sora és a szállított
  `settings.fileOperations.adbEnabled.description` mondata) · `high`. Az `adb` prózában itt idézőjel NÉLKÜL áll, mert az
  angol sem idézi ezen a két kulcson (a szótár szabálya az idézést az angolhoz köti).
- **`settings.summary.servers` nem tagad**: `A megbízhatónak tekintett SSH-szerverkulcsok.`, és a kártya súgójának
  második mondata is állító (`… a Szerverek listában találod; ez az oldal csak a kulcsokról szól.`) az angol „…, not on
  this page.” helyett · a házi hangszabály („Y, nem X” kerülendő) · `high`.
- **`settings.appearance.tintSmb.*` átcímkézve**: a színezés már nemcsak SMB-t fed le, ezért
  `Szerverpanelek színezése (SMB, SFTP, WebDAV)` és
  `Háttérszínezés az SMB-megosztást, SFTP- vagy WebDAV-szervert mutató paneleken.` A címke a testvérek mintáját tartja
  (`MTP-panelek színezése`, `Helyi kötetek paneljeinek színezése`), a leírás pedig betű szerint a
  `settings.appearance.tintMtp.description` keretét (`Háttérszínezés az … mutató paneleken.`) · `high`. A `sourceHash`-t
  nem nyúltuk, azt a lead bélyegzi újra.
- **`sameAsSourceJustification` ebben a passzban egy kulcson**: `settings.section.adb` (`Android (ADB)`) — terméknév +
  betűszó, mindkettőt a magyar Android és a katalógus szállított sorai is változatlanul hozzák. A többi 30 érték eltér
  az angoltól.

## Az Android-telefon panelüzenetei, elemleírásai és a hibakeresés-tipp (`adb.connect.*`, `adb.readiness.*`, `adb.hint.*`, `adb.disconnect*`, `settings.behavior.adbHintDismissed.*`)

A telefonpanel 17 sora és a hozzájuk tartozó két belső beállításkulcs. A Tier-1 bizonyíték a telepített macOS-ből jön
(`.loctable`-söprés, macOS 26.6.2, build 25G83, 2026-09-07), az Android saját szavai pedig az AOSP magyar fordításából.

- **Allow (az Android saját párbeszédének gombja): `Engedélyezés`** · AOSP `frameworks/base/packages/SystemUI`
  `values-hu/strings.xml`, `usb_debugging_allow` = „Engedélyezés” (a `usb_debugging_title` = „Engedélyezi az USB
  hibakeresést?” ugyanezen a párbeszéden), plusz a macOS `.loctable`-söprés, amely az angol `Allow`-ra `Engedélyezés` és
  `Engedélyez` alakot ad · `high`. A két forrás egyezik, tehát a szó, amit a felhasználó a telefonján lát, ugyanaz, amit
  a Cmdr mond. Prózában idézőjelbe kerül (`az „Engedélyezés” gombra`) a szótár UI-címke-idézési szabálya szerint
  (`„Zárolt”`, `„Fájlátvitel”`), és a névelő `az`, mert a szó ismert és magánhangzóval kezdődik: itt nincs mit
  hedge-elni.
- **USB debugging: marad `USB-hibakeresés`** (a `style.md` szótársora), a `settings.summary.adb` és a
  `settings.fileOperations.adbEnabled.description` szállított alakjával egyezően. Újraellenőrizve: AOSP
  `SettingsLib/res/values-hu/strings.xml` `enable_adb` = „USB hibakeresés”, `clear_adb_keys` = „USB-s hibakeresésre
  vonatkozó engedélyek visszavonása” · `high`. A kötőjel továbbra is a miénk (AkH + a katalógus `USB-kábel`,
  `USB-eszköz`, `USB-fájlátviteli mód` sorai).
- **Android platform tools: marad `az Android platform tools csomag`** (a `style.md` szótársora), ahogy a szállított
  `settings.adb.install.intro` és `settings.fileOperations.adbEnabled.description` írja. Az
  `adb.connect.adbNotInstalled` ezért `A Cmdr nem találja az Android platform tools csomagot.` — az alaptag viszi a
  ragot, a név ragozatlan · `high`.
- **„The Android tools on this Mac”: `Az Android-fejlesztőeszközök`** · a `style.md` `Android tooling` sora és a
  szállított `settings.fileOperations.adbEnabled.description` („Ha nincs telepítve Android-fejlesztőeszköz”) · `high`. A
  puszta `Android-eszköz` tilos, mert az `Android device`-t jelentene. A helyhatározó a többségi `Macen` alak.
- **How (a hibakeresés-tipp végén álló hivatkozás): `Hogyan?`** · nincs Tier-1 találat: a teljes macOS-söprés (minden
  `.loctable` `/System/Library/{CoreServices,Frameworks,PrivateFrameworks}`, `/System/Applications`, `/Applications`
  alatt) az önálló `How` sztringre NULLA magyar párt ad; ami van, az a `Learn More` = `Bővebben` / `További információ`
  és a `Dismiss` = `Elvetés` / `Bezárás` · `tentative`. A `Bővebben` a „Learn more” szava, és elnyelné a kérdés
  hangsúlyát, amit az angol `How` visz; a `Hogyan?` az a magyar alak, ahogy egy ember rákérdez. Ha valaha natív
  ellenőrzés lesz, ez az egy sor az, amit érdemes megnézni.
- **Dismiss: `Elvetés`** · a katalógus 10 másik `Dismiss` kulcsa mind `Elvetés`, és a macOS-söprés is adja (`Dismiss` =
  `Elvetés`) · `high`. A `desktop-i18n-term-consistency` egyébként is kényszeríti: az angol betű szerint azonos.
- **`adb.disconnectDeviceAriaLabel` betű szerint a szerveres testvéréé**: `{name} leválasztása`, mert az angol
  (`Disconnect {name}`) betű szerint azonos a `fileExplorer.navigation.disconnectPlaceAriaLabel`-lel, tehát a
  `desktop-i18n-term-consistency` egyetlen magyar alakot követel. Ez amúgy is a helyes döntés: a `leválasztás` a szótár
  `disconnect` szava, és tudatosan NEM `kiadás` (`eject`), mert a telefon a kábelen marad, semmi nem lesz biztonságosan
  kihúzható.
- **`adb.disconnectBusyTooltip` a „busy” elemleírások keretét hozza**:
  `Nem választható le, amíg ezen az eszközön műveletek vannak folyamatban` · a szállított
  `fileExplorer.navigation.disconnectBusyTooltip` (`… ezen a szerveren …`) és `fileExplorer.navigation.ejectBusyTooltip`
  (`… ezen az eszközön …`) keveréke, pontosan ahogy az angolja is az · `high`.
- **A panelsorok hangja a `servers.*` panelállapotoké**: rövid, tegező, nem hibáztat.
  `A telefonod nem válaszolt időben.` a `servers.refusal.timedOut` (`{host} nem válaszolt időben.`) mintája;
  `A Cmdr elvesztette a kapcsolatot a telefonoddal.` a `servers.refusal.unreachable` alanyi szerkezetéé. A `hiba` /
  `sikertelen` regiszter egyik sorban sem jelenik meg.
- **`adb.connect.waitingHint`: `Amint megteszed, a Cmdr megnyitja a telefonodat.`** Az angol „as soon as you do” a
  fölötte álló mondat („koppints az „Engedélyezés” gombra”) igéjére utal vissza; magyarul a `megteszed` viszi ezt, mert
  a `koppintasz` megismétlése a két sorban egymás után szóismétlés lenne. Megnyugtatás, nem utasítás · `high`.
- **`adb.readiness.offline` a szállított kábeles mondatot használja**: `húzd ki és dugd vissza a kábelt` · a
  `errors.provider.macDroid.transient` szállított sora („Húzd ki és dugd vissza az USB-kábelt”) · `high`. A `reseat`-re
  nincs egyszavas magyar UI-alak; ez a kétigés szerkezet a katalógus sajátja.
- **`adb.readiness.noPermissions`: `USB-n keresztül`** · a szállított `fileExplorer.navigation.spaceMtpHint` („USB-n
  keresztül a Cmdr csak…”) · `high`. A `port` marad `port` (az Android magyar `adb_wireless_ip_addr_preference_title` =
  „IP-cím és port”).
- **A két belső beállításkulcs a testvérét másolja**: a `settings.behavior.serversPinHintSeen.*` mintájára
  `Az USB-hibakeresés tippje elvetve` + `Elvetették-e már az USB-hibakeresést felajánló egyszeri sort.` Sosem látszik a
  felületen, de a lefedettség miatt le kell fordítani; a személytelen alak a testvérétől jön („Megjelent-e már…”).
- **`adb.volumeLabelWithSuffix` változatlan**: egy helyőrző plusz az `ADB` betűszó, a `sameAsSourceJustification` már
  korábbról áll rajta.
- **`You stopped opening your phone.` → `Leállítottad a telefonod megnyitását.`** (`adb.connect.cancelled`) · a
  `leállít` tő a párhuzamos `search.coverage.walk.cancelled` szállított alakjából jön („You stopped this search” →
  `Leállítottad ezt a keresést`), és ugyanez a tő áll az `errors.volume.cancelled`-ban is
  (`A Cmdr a kérésedre leállította ezt a műveletet.`) · `high`. ❌ NEM a `Mégsem` töve: az a GOMB felirata
  (`fileOperations.button.cancel`), a mondat viszont nem a gombot nevezi meg, hanem azt mondja el, mi történt. A
  `megnyitás` a telefon kinyitásának szállított igéje (`adb.connect.waitingHint`, „a Cmdr megnyitja a telefonodat”); a
  birtokos `a telefonod megnyitását` a természetes magyar alak, és nem ragoz helyőrzőt.

## A zárolt szerveridentitás (`servers.sheet.identityLocked`)

A két sor a kiszürkített `Cím` és `Felhasználónév` mezők alatt, amikor mentett szervert szerkesztesz.

- **`the account` (az a mező, amivel a szerverre bejelentkezel) → `fiók`** · a katalógus már ezt használja ugyanebben az
  értelemben (`errors.json` és `onboarding.json`, például „a fiók, amellyel csatlakoztál”) · `high`.
- **A súgósor pontosan úgy nevezi meg a műveleteket, ahogy a gombok, amikre mutat**: `elfelejt` a
  `menu.network.forgetServer`-ből („Szerver elfelejtése”) és `hozzáad` a `servers.sheet.addTitle`-ből („Szerver
  hozzáadása”). Szinonima („eltávolítás”, „felvétel”) olyan menüponthoz küldi az olvasót, ami nincs.
- **`are what name this server` → `azonosítja ezt a szervert`** · az űrlapon van saját `Név` mező
  (`servers.sheet.name`), ezért a mondat nem építhet az „elnevezés”-re: úgy hangzana, mintha arról a címkéről lenne szó.
  Az „azonosít” azt mondja, amit kell (a két érték MAGA a szerver) · `high`.
- **`to change them` → `a módosításukhoz`** · a `módosít` tő a katalógus szava a beállítások megváltoztatására
  (`settings.json` „módosításához”, `onboarding.json` „módosíthatod”) · `high`.

## Időtartam-helyőrző mellé kell a névutó (`servers.paneState.retryKeepsTrying`)

`"Összesen {duration} próbálkozik tovább."` úgy olvasódik, hogy „összesen 2 perc próbálkozik tovább”: a `{duration}`
alanyként áll, és a mondat azt állítja, hogy az idő próbálkozik. A javított alak
`"Összesen {duration} ideig próbálkozik tovább."`.

**A szabály, nem csak ez az egy kulcs**: az angol `for a total of {duration}` az elöljárójával jelöli a szerepet, a
magyarban viszont a `{duration}` egy nominatívuszi kifejezést (`2 perc`, `60 másodperc`) hoz, amit ragozni nem lehet
(nem tudjuk, milyen szó áll benne). Ilyenkor **névutót kell tenni utána** (`ideig`, `alatt`, `múlva`), az mondja meg,
hogy időtartamról van szó. ❌ Csupasz helyőrző időtartamra soha.

## A toast, amikor nem is volt mentett jelszó (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `A(z) „{name}” szervernek nem volt mentett jelszava.`** · a szállított
  testvérek szava (`menu.network.forgetSavedPassword` és `fileExplorer.navigation.forgetSecretConfirmTitle` =
  `Mentett jelszó elfelejtése`, `.forgetSecretConfirm` = `Elfelejted a(z) „{name}” mentett jelszavát?`,
  `.forgetSecretRefusedToast`) · `high`.
- **A ragok a fejnévre kerülnek, a helyőrző ragozatlan marad**: a részes rag a `szerver`-re (`szervernek`), a birtokos
  személyjel a `jelszó`-ra (`jelszava`). Ugyanaz a fogás, mint a megerősítő párbeszédben, ahol a rag a `jelszavát`
  alakra megy. Az `a(z)` + `„…”` a ház alakja az ismeretlen kezdőhangú, felhasználó adta névre (§ style.md).
- **Múlt idő, mentegetőzés nélkül**, ahogy az angol `@key` kéri: nem hibázott semmi, ezért se `nem sikerült`, se `hiba`.
  A `nem volt` a katalógus meglévő tagadó egzisztenciális alakja (`fileOperations.trash.undoUnavailable`:
  `Nincs mit visszahelyezni.`).

## Az újrapróbálkozás hossza, a gazdakulcs-fejléc és az Android Engedélyezés gombja (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `servers.hostKey.*`, `adb.connect.unauthorized`)

- **A `{seconds}`/`{minutes}` mostantól ICU-többesszámot visz, KÉT helyőrzővel** (`servers.paneState.retryTotalSeconds`,
  `.retryTotalMinutes`): a `{seconds}` csak az ágat választja, az olvasó a `{secondsText}`-et látja, a már formázott
  számot. A magyar CLDR-kategóriák `one` és `other`, és a főnév szám után egyes számban marad (§ style.md), ezért a két
  ág szövege azonos · `high`.
- **A `{duration}` a `servers.paneState.retryKeepsTrying` `ideig` névutója elé kerül**, ezért a két érték `-nyi` képzős
  mennyiségjelző: `60 másodpercnyi`, `2 percnyi`. Így áll össze a nyelvtanilag helyes „Összesen 2 percnyi ideig
  próbálkozik tovább.” ❌ A csupasz `2 perc` itt nem jó: a „2 perc ideig” a köznyelvben előfordul, de helytelen. Ezzel
  egészül ki a fenti § „Időtartam-helyőrző mellé kell a névutó”: ha a névutó `ideig`, akkor a helyőrzőnek `-nyi` képzős
  alakot kell hoznia, nem nominatívuszit · `high`.
- **`Cmdr won't connect to {name}` → `A Cmdr nem kapcsolódik ide: {name}`** · a `nem kapcsolódik` szó szerint a testvér
  `servers.refusal.hostKeyRevoked`-ból (`A Cmdr nem kapcsolódik hozzá.`). Az angol a „stopped connecting”-ról állandó
  elutasításra váltott, ezért jelen idő, nem múlt („leállította”), ami félbehagyott próbálkozásnak hangzana. Az
  `ide: {name}` a ház fogása a ragozhatatlan helyőrzőre (§ style.md) · `high`.
- **Az `Allow` az Android saját gombja → `„Engedélyezés”`**, szó szerint az `adb.connect.unauthorized`-ból
  (`Nézd meg a telefonodat, és koppints az „Engedélyezés” gombra.`), a `gomb` fejnévvel, hogy a rag ne a gombnévre
  kerüljön. A `koppint` ige is onnan jön, így a szöveg és a képernyő ugyanazt a szót mutatja · `high`.

## A szerversor helyi menüje: `Megnyitás` és `Szerver szerkesztése…` (`menu.network.open`, `menu.network.edit`)

- **`Open` (szerversoron) → `Megnyitás`** (`menu.network.open`) · szó szerint ugyanaz, mint a `menu.file.open`, mert ez
  ugyanaz a jelentés: belépünk valamibe, nem pedig átadunk egy fájlt egy alkalmazásnak. A magyar nem választja szét a
  kettőt, és a macOS sem: a Finder ugyanazt az igét viszi a `Megnyitás` (`LocalizableMerged` `N151`), `Megnyitás ezzel`
  (`N152`) és `Megnyitás új ablakban` (`FV7`, a belépős értelem) címkékben (Finder 26.6.2, build 25G83, olvasva
  2026-09-07) · `high`. A nominális `-ás` alak illik a testvérekhez is (`Leválasztás`, `Szerver elfelejtése`).
- **`Edit server…` → `Szerver szerkesztése…`** (`menu.network.edit`), bájtra pontosan a `commands.serversEdit.label`-ből
  másolva · `high`. A kettő ugyanazt a lapot nyitja meg; két különböző felirat két külön funkciónak olvasódna. A három
  pont az EGYETLEN `…` karakter (U+2026), és marad.
- **Mindkét egyezést ellenőrzés őrzi**, nem csak ízlés kérdése: az `i18n-terms` szól, ha két azonos angol értékű kulcs
  magyarul szétcsúszik. Ha valaki később átfogalmazza az egyiket, a párját is vinnie kell.
- **A `menu.*` RAW család**: a menüt a Rust rajzolja `menu_t`-vel, sosem `t()`-vel. Az aposztróf tehát EGYSZERES marad,
  a megkettőzött `''` elbuktatja az `i18n-icu`-t. Ebben a két értékben egy sincs.

## A funkcióbillentyű-sáv helyi menüje (`fileExplorer.functionKeyBar.hiddenToast`)

- function key bar (az ablak alján lévő F-billentyűs parancsgombok sora) → funkcióbillentyű-sáv · már rögzítve a
  katalógusban (`settings.appearance.showFunctionKeyBar.label`); újrafelhasználva a helyi menü elemhez és a hozzá
  tartozó toasthoz · high

## A Dockba kerülés egyszeri ajánlata (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Kilenc sor egy értesítésre (cím, törzs, megnyugtató harmadik sor, két gomb, négy kimeneti üzenet), plus a két belső
beállításkulcs. A `_ignored/i18n/hu/` referenciakupac ezen a gépen MEGVAN, és abból dolgoztunk (macOS Finder + AppKit +
System Settings JSON-dumpok, Microsoft-terminológia `HUNGARIAN.tbx`, GNOME Nautilus, Xfce Thunar, KDE Dolphin, Total
Commander, Double Commander).

### A három macOS-felületnév

- **Dock → `Dock`, ragozva kötőjel nélkül** (`a Dockban`, `a Dockba`, `a Dockodban`, `a Dockomba`, `a Dockodat`) · mac
  (Finder `LocalizableMerged` `N169.13` és System Settings `300772.title`: `Add to Dock` = „Hozzáadás a Dockhoz”;
  AppKit: „…háttérképét a Dockról”), plusz a szállított katalógus (`errors.listing.diskFullErrno.suggestion`,
  `errors.listing.storageFull.suggestion`: „kattints jobb gombbal a Kuka ikonra a Dockban”) · `high`. Az Apple NEM
  fordítja le, és a `Dock` végi `k` a kiejtett hangot írja, tehát az AkH kötőjelszabálya nem lép be. A
  magánhangzó-illeszkedés a kiejtett „dokk” hátsó hangrendjéhez igazodik: `-ban`, `-ba`, `-hoz`, `-om`, `-od`.
- **Finder → `Finder`, ragozva kötőjel nélkül** (`a Finderben`, `a Finder mellé`) · mac (Finder `CFBundleDisplayName` =
  „Finder”, „Megjelenítés a Finderben”, „Fájlok és mappák keresése a Finderben”), plusz a szállított
  `commands.fileShowInFinder.mac.label` · `high`. Összetételben viszont kötőjeles (`Finder-ablak`, `Finder-elem`,
  `Finder-címke`), ahogy az Apple is írja.
- **Applications (a mappa) → `Alkalmazások`** · mac (Finder `Applications` = „Alkalmazások”, `TL5` oldalsávcímke,
  `GROUP_APPLICATIONS`, `TL_HELP_APPS` = „Ugrás az Alkalmazások mappába”; AppKit: „Try dragging „%@” from the Trash to
  your Applications folder.” = „Próbálja a(z) „%@” alkalmazást a kukából az **Alkalmazások mappába** húzni.”) · `high`.
  A kupacban NULLA `Programok` találat van.
  - ⚠️ **A `Programok` a régi Mac OS X-es név, ❌ ne kerüljön vissza.** Az 1. szintű, betű szerinti Apple-találat veri a
    katalógus családi mintáját (§ „A szerverközpont táblázata” precedense). A `desktop-i18n-term-consistency` nem fogja
    el a visszaesést, mert az érintett kulcsok angolja nem betű szerint azonos, tehát ez a bejegyzés az egyetlen
    védelem.

### A többi eldöntött szó

- **pin / unpin → `rögzítés` / `rögzítés feloldása`** · mac (AppKit `MenuCommands` `Pin Tab` = „Lap rögzítése”,
  `Unpin Tab` = „Lap rögzítésének feloldása”), ms (`pin` = `rögzítés`/`rögzít`, `unpin` = `rögzítés feloldása`) ·
  `high`. Már rögzített döntés (§ „A rögzítési tipp…”), itt csak újrahasznosítjuk. Ez tartja rokonságban az angol
  `pinned` ↔ `unpin` párt: `Rögzítsük odalent…` ↔ `feloldhatod a rögzítést`.
- **configuration profile → `konfigurációs profil`** · ms (`HUNGARIAN.tbx`, `configuration profile` = „konfigurációs
  profil”, HUN) · `high`. A macOS-kupacban nincs rá találat (a `Profiles`/`Device Management` panel nincs a dumpban),
  tehát ez a 2. szintű forrás dönt; egybevág az Apple magyar támogatási szóhasználatával.
- **log in (bejelentkezés a Mac-fiókba) → `bejelentkezés`, időhatározóként `a következő bejelentkezéskor`** · mac
  (`Bejelentkezés…`, `Log Out` = „Kijelentkezés”) · `high`. A `következő …kor` keret a szállított
  `fileExplorer.navigation.forgetSecretConfirm` („A Cmdr a következő kapcsolódáskor újra elkéri.”) idiómája.
- **No, thanks → `Nem, köszönöm`** · nincs OS-forrás (sem a macOS-dumpban, sem a Microsoft-terminológiában nincs
  udvarias elutasító gomb) · `tentative` a forrás hiánya miatt, de a jelentés nem kétséges: ez a magyar köznyelv
  udvarias visszautasítása, és pontosan az a regiszter, amit a style.md fogyasztói-márkás `te`-hangja kér. ❌ NEM
  `Most nem`: az a korábban szállított, azóta kivezetett askCmdr.consent.decline, más angolra (`Not now`), és „később
  talán”-t ígér, amit ez a gomb nem tesz (a Cmdr soha többé nem kérdez).
- **Yes, … (igenlő gomb a felhasználó szájából) → `Igen, …`** · a szállított `onboarding.stepAi.cloud.label`
  (`Yes, I want AI` = „Igen, szeretnék AI-t”) · `high`. Innen `Igen, kerüljön a Dockomba` (négy szó, gombba fér). A
  kötőmód (`kerüljön`) azért jó, mert nem kell megnevezni a cselekvőt: a `tedd` a Cmdrt tegezné, pedig a katalógusban a
  felhasználót tegezzük.

### Mondatszintű döntések

- **A cím a macOS „Keep in Dock” jelentését viszi**: `Maradjon a Cmdr a Dockodban?` Az app futás közben amúgy is ott van
  az ikonsávban, a kérdés az, hogy KINT MARADJON-e; a `maradjon` pont ezt mondja, és valódi kérdés marad, nem
  felszólítás. A birtokos (`Dockodban`) a szállított `fileExplorer.navigation.serverPinnedToast` („… ott van a
  kötetválasztódban”) mintája: ahol az angol `your`-t mond, a magyar birtokos személyjelet tesz.
- **„a few days” → `Néhány napja`**, szám nélkül · a `néhány` a kupac és a katalógus bevett homályos kis mennyisége
  (mac: „Távolítson el néhány fájlt”; katalógus: „várj néhány másodpercet”) · `high`. ❌ Soha nem `három napja`: a
  küszöb mozoghat.
- **„down there” → `odalent`**, megtartva: a magyar Dock is alapból a képernyő alján ül, és a
  `Rögzítsük odalent, a Finder mellé?` sorrend a helyhatározót teszi előre, ahogy a magyar szórend kéri.
- **A négy kimeneti üzenet EGYIKE SEM hibaüzenet-regiszterű**: se `hiba`, se `sikertelen`, se `nem sikerült`. A
  `notAdded` nyitánya a szállított `fileExplorer.pane.directConnectionUnavailableToast` („Most nem jött létre közvetlen
  kapcsolat, …”) mintája: `A Cmdr most nem került be a Dockba.` A `most` viszi az angol `this time`-ot, és tényt közöl,
  nem kudarcot.
- **`addedButDockDidNotRestart` nem tagadhatja le a rögzítést**, mert az megtörtént:
  `A Cmdr ikonja a helyén van, csak a Dock nem töltődött újra.` A `csak` (nem `de`) az a kötőszó, ami a magyarban azt
  mondja, hogy egyetlen apróság hiányzik; a `de` szembeállítana, és a rögzítés meghiúsulásának olvasatát erősítené. Ez a
  passz legkockázatosabb sora.
- **`managedDock` megnevezi, ki oldhatja fel, és nem hibáztat**: `Ezen az tud változtatni, aki ezt a Macet felügyeli.` A
  `felügyeli` (nem a mondat elején már használt `kezeli`) kerüli a szóismétlést, és az adminisztrátori szerepet nevezi
  meg. A `Macet` tárgyeset kötőjel nélkül, a style.md szerint.
- **A két belső kulcs a `settings.behavior.*Seen.*` család alakját másolja** (`serversPinHintSeen`,
  `openTerminalHereToastSeen`, `doubleClickOnPaneNotificationSeen`): a címke `A <valami> megjelent`, a leírás
  `Megjelent-e már az egyszeri …`. Innen `A Dock-ajánlat megjelent` és
  `Megjelent-e már az egyszeri ajánlat, hogy a Cmdr bekerüljön a Dockba.` A `Dock-ajánlat` kötőjeles, mert tulajdonnév +
  köznév összetétele (`Finder-címke`, `USB-kábel` mintája). Sosem látszik a felületen, de a lefedettség kéri.

## A Dock helyi menüje (`menu.dock.*`)

Öt natív menüpont arra a menüre, amit a Cmdr Dock-ikonjára jobb gombbal kattintva kapunk. RAW család (`menu.*`), tehát
**egyszeres aposztróf** és literál `{name}` / `{parent}` helyőrző; a záró `…` U+2026 marad.

**Új, elsőrangú forrás ehhez a felülethez: maga a macOS Dock.** A `Dock.app` a `hu.lproj/DockMenus.strings` fájlban
szállítja a saját helyi menüje szövegeit, vagyis pontosan azt a menüt, amelybe a Cmdr elemei kerülnek. Kiolvasása:
`plutil -convert json -o - /System/Library/CoreServices/Dock.app/Contents/Resources/hu.lproj/DockMenus.strings` (a
kulcsok beszédesek: `OPEN`, `HIDE_NAME`, `SHOW_NAME`, `QUIT`, `KEEP_IN_DOCK`, `REMOVE_FROM_DOCK`). A referenciakupac
`hu/macOS/` mappája csak a Finder, az AppKit és a System Settings dumpját tartalmazza, a Dockét nem, ezért ezt élőben
kell kiolvasni. (macOS 26.6.2, build 25G83, `plutil` + `jq`, 2026-09-09.)

- **`Open Cmdr` → `Cmdr megnyitása`** · mac Dock (`DockMenus.strings` `HIDE_NAME` = „%@ elrejtése”, `SHOW_NAME` = „%@
  elem megjelenítése”, `OPEN` = „Megnyitás”), plusz a szállított `menu.app.hide` = „Cmdr elrejtése” · `high`. A Dock
  saját mintája appnév esetén **puszta név + névszói cselekvés, névelő nélkül**; a `A(z) „%@” megnyitása` alak
  (`OPEN_FILENAME`) FÁJLNÉVRE való, ahol a kezdőhang ismeretlen. Egy appnévnél nincs mit hedgelni, tehát nem kell az
  `a(z)`. Ugyanez a Finderé a menüsorban: „Finder elrejtése” (`MenuBar` `300728.title`), névelő nélkül.
- **`Search files…` → `Fájlok keresése…`** · a szállított `menu.edit.searchFiles` · `high`. Betű szerint azonos angol
  (`sourceHash` `149a9d1`), tehát a `desktop-i18n-term-consistency` amúgy is egy alakot kér; ugyanaz a parancs a
  menüsorban és a Dockban.
- **`Go to folder…` → `Ugrás mappához…`** · mac Finder `hu` (`MenuBar` `261.title` = „Ugrás mappához…”, és a hozzá
  tartozó ablakcím `GotoWindow` `1.title` = „Ugrás mappához”) · `high`. Betű szerinti Tier-1 találat, pontosan arra a
  menüpontra, amit az angol leírás megnevez (Finder > Ugrás > Ugrás mappához…). ❌ NEM `Ugrás útvonalra…`: az a
  szállított `menu.go.goToPath`, más angolra (`Go to path…`), és a két kulcs szándékosan más szót visz.
- **`Connect to server…` → `Kapcsolódás szerverre…`** · mac Finder `hu` (`MenuBar` `266.title`, ablakcím
  `ConnectToWindow` `1.title` = „Kapcsolódás szerverre”), plusz a szállított `commands.serversConnect.label` és
  `settings.network.permissionIntroConnectLink`, amelyek angolja betű szerint ugyanez · `high`. A `szerver` (nem
  `kiszolgáló`) a szótár szállított döntése (`style.md` § Digest, top traps).
- **`{name} ({parent})` → változatlan, `sameAsSourceJustification`-nel** · mac Finder `hu` (`LocalizableMerged`
  `SB_iCloudDetail` = „^0 (^1)”, `IN_G6_V1` = „^2 (^3)”) · `high`. A magyar ugyanabban a sorrendben és ugyanazzal a
  zárójelezéssel írja a név + minősítő párost, mint az angol, tehát nincs mit átrendezni. Toldalék egyik helyőrzőre sem
  kerülhet (mindkettő ismeretlen végű mappanév, `style.md` § Agglutination), így valóban nem marad változtatnivaló.

## A „Megjelenítés a Finderben” ajánlat és az első találat értesítése (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Ugyanannak a funkciónak két pillanata: az egyszeri ajánlat, hogy a más appokból érkező „Megjelenítés a Finderben” a
Cmdrben nyíljon meg, és az egyszeri értesítés, amikor egy ilyen kérés először landol itt. Mindkét felület macOS saját
parancsára mutat, tehát a macOS szóhasználata nyer (style.md § rendszerfelületek).

- **„Show in Finder” → `„Megjelenítés a Finderben”`, magyar idézőjelekkel** · Már rögzítve:
  `settings.navigationAndFileOps.card.showInFinder` és `settings.revealHandler.description` · `high`. A toastok pontosan
  ezt a formát veszik át, hogy a beállítási kártya és az értesítés ugyanazt az akciót ugyanúgy nevezze.
- **pane → `panel`** · A katalógus formája: `fileExplorer.doubleClickHint.body` („panel hátterére”) · `high`.
- **Settings (a Cmdr saját ablaka) → `Beállítások`** · `settings.window.title` · `high`.
- **„for a while now” → `már egy ideje`** · Szándékosan homályos: a küszöb mozoghat, ezért ❌ soha nem szám. Ugyanaz a
  szabály, mint a `main.dockPinNudge.body`-nál („néhány napja”) · `high`.
- **A márkanév ragozódik** · `a Cmdrben`, `a Cmdrnek` — a kiejtés („kommander”) szerinti magas hangrendű toldalék,
  style.md § márkanevek · `high`.
- **Az első találat értesítése ❌ nem bocsánatkérés** · Azt mondja el, mi történt, miért, és hol a kapcsoló. Innen az
  `A Cmdr úgy van beállítva, hogy elkapja ezeket`, ❌ nem „sajnáljuk” · `high`.

## A bevezető átírt lépései: ellenőrzőlista, lépésbuborék, összefoglalók (`onboarding.*`)

Huszonhárom kulcs a bevezető varázsló négy lépéséről: a lépésjelző buboréka, az FDA-lépés `Miért?` linkje, az AI-lépés
két hosszú magyarázata, a 3. lépés új ellenőrzőlistája (csillagozás, AlternativeTo, e-mail-mező), és a 4. lépés négy
egysoros összefoglalója. ICU család (nem RAW), tehát kettőzött aposztróf járna, de egyetlen magyar érték sem tartalmaz
aposztrófot. Négy kulcs angolja átíródott, ezért a tárolt hash `sync-locale-keys.ts hu --restamp` alakkal frissült.

### Az eldöntött szavak

- **star (ige, „megjelöl valamit csillaggal”) → `csillagoz`** · ms (`HUNGARIAN.tbx`, `star` Verb, definíció: „To mark an
  entity with a star.” → `csillagoz`, HUN), plusz a szállított `onboarding.stepBeta.checklist.star` („Csillagozd meg a
  repót a GitHubon”) · `high`. ❗ A **GitHub nem szállít magyar felületet** (a támogatott nyelvei közt nincs magyar),
  tehát a „használd, amit a GitHub mond a te nyelveden” utasításra itt nincs Tier-1 válasz: a magyar felhasználó angol
  `Star` gombot lát. A Microsoft-terminológia viszont pontosan erre a jelentésre ad alakot, és egyezik a szállított
  katalógussal, ami eldönti.
- **repo → `repó`** · a szállított `onboarding.stepBeta.checklist.star` · `high`. ❌ NEM `adattár` (az a Microsoft
  `repository` alakja, és általános adattárolót nevez meg, nem Git-repót) és nem `tároló` (az a `container` felé
  csúszik). Az angol is a köznyelvi rövidítést használja („repo”), tehát a regiszter is egyezik.
- **like (ige, „tetszést fejez ki egy elemre”) → `kedvel`** · ms (`HUNGARIAN.tbx`, `like` Verb, definíció: „To express
  approval for a certain item.” → `kedvel`, HUN) · `high`. Az AlternativeTo Like gombja pontosan ez a jelentés (egy
  elem, nem egy közösségi poszt), ezért nem a másik MS-alak (`tetszik`, definíció: „Action taken by a user on a post”)
  jön. Az AlternativeTo sem szállít magyar felületet, tehát itt sincs Tier-1 forrás. ❌ NEM `lájkol`: a Microsoft csak
  FŐNÉVKÉNT ismeri el a `lájk`-ot (a `kedvelés` mellett), igei alakra nem ad forrást, és a katalógus máshol sem használ
  szleng igét.
- **checklist → `ellenőrzőlista`** · ms (`HUNGARIAN.tbx`, `checklist` → `ellenőrzőlista`, HUN) · `high`.
- **mailing list → `levelezőlista`** · ms (`HUNGARIAN.tbx`, két külön szócikk, mindkettő `levelezőlista`, HUN) · `high`.
- **badge → `jelvény`** · ms (`HUNGARIAN.tbx`, két szócikk, mindkettő `jelvény`, HUN), plusz a szállított
  `onboarding.stepBeta.openBeta` · `high`.
- **Save (az e-mail-mező melletti gomb) → `Mentés`** · mac (a `hu/macOS/` kupacban kilenc puszta „Mentés” előfordulás,
  plusz „Mentés…”, „Mentés mint:”, „Mentés mindenképp”) · `high`. Prózában a gombra a `Mentés gombot` alakkal
  hivatkozunk (`signup.rejected`, `signup.unreachable`), ahogy a katalógus a többi gombra is
  (`Kattints … a <strong>Kilépés és újranyitás</strong> gombra`).
- **More about {topic} → `Bővebben: {topic}`** · mac („Bővebben az iCloudról”, „Bővebben”, „Bővebben:”, „Bővebben…”) ·
  `high`. Az Apple ragozott alakja (`az iCloudról`) itt nem másolható: a `{topic}` egy futásidőben behelyettesített,
  ismeretlen végű címke, amire nem tehető rag (`style.md` § Agglutination). A kettőspontos alakot maga a macOS is
  szállítja, tehát nem kitalált kerülőút.
- **typo → az `elgépel` ige** · nincs forrás a kupacban (sem a macOS, sem a Microsoft-terminológia, sem a
  stílusútmutató-PDF nem ismeri) · `tentative`. A `signup.rejected` ezért nem főnevet használ, hanem átfogalmaz:
  `Érdemes megnézni, nem gépelted-e el`. Ha egy későbbi passz talál rá forrást, ez a mondat az első hely, ahol érdemes
  felülvizsgálni.

### Mondatszintű döntések

- **Az ellenőrzőlista sorai FELSZÓLÍTÓK, nem névszóiak.** A `style.md` § Formality a névszói alakot a gombokra, menükre
  és fejlécekre írja elő; ezek linkszövegek egy teendőlistában, és a testvérsoruk (`checklist.email`) mindenképp mondat
  kell legyen (a `<field></field>` mezőt egy mondat közepén kell körbefognia). Egy listán belül a vegyes alak látszik, a
  listák közötti eltérés nem, ezért az egész sor tegező felszólítás: `Csillagozd meg a repót a GitHubon`,
  `Kedveld a Cmdrt az AlternativeTo oldalán`, `Add meg az e-mail-címedet …`.
- **`AlternativeTo` nem kap ragot, hanem alaptagot: `az AlternativeTo oldalán`.** Ugyanaz a minta, mint az
  `Android platform tools csomag`-nál (`style.md`): a név végi `o` az angol kiejtésben nem magyar `o`, tehát sem a
  kötőjeles (`AlternativeTo-n`), sem a kötőjel nélküli (`AlternativeTón`) rag nem védhető. Az `oldalán` alaptag
  ragozatlanul hagyja a nevet, és a mondat így is rövid marad. A névelő `az`, mert a kiejtés magánhangzóval kezdődik.
- **`Cmdrtől`, `Cmdrre`: a márka toldaléka ELÖL HANGRENDŰ.** A `style.md` „hátsó hangrendű toldalék” megfogalmazása
  félrevezető; a felsorolt példái (`Cmdrben`, `Cmdrrel`, `Cmdrnek`) és a szállított katalógus is végig elöl hangrendű
  (24× `Cmdrnek`, 9× `Cmdrben`, 4× `Cmdrből`, 3× `Cmdrhez`, 2× `Cmdrre`). A kiejtett „commander” utolsó magánhangzója
  `e`, és a magyar hangrend a szó UTOLSÓ magánhangzójához igazodik, tehát `-től`, `-re`, `-nek`, `-ben`.
- **A `<code>` parancs elé VALÓDI névelő kerül, nem `a(z)` hedge.** A `checklist.starNote` tartalma mindig ugyanaz a
  literál (`brew install cmdr`, `b`-vel kezdődik), tehát a `style.md` szabálya („egyetlen lehetséges értékű helyőrző a
  valódi névelőt kapja”) érvényes: `a <code>brew install cmdr</code> parancsot`. A testvér
  `onboarding.stepBeta.checklist.starNote` ugyanezt a szabályt követi: `a <code>brew install cmdr</code> parancsot`,
  valódi névelővel.
- **A `{nextLabel}` szintén valódi névelőt kap: `a „{nextLabel}” gombra`.** Az értéke a magyarban vagy `Tovább`, vagy
  `Befejezés` (`onboarding.wizard.next` / `.finish`), mindkettő mássalhangzóval kezdődik, tehát `a`. Az idézőjel a házi
  `„…”` alak, ahogy a katalógus minden felületi címkét idéz (`a „+” gombra`, `a „Zárolt” pipát`).
- **`{mandatory}+1-ből`: a toldalék az `1`-hez igazodik, nem a helyőrzőhöz.** A `wizard.stepTooltip` „+1” része literál,
  tehát a rag mindig az `egy` kiejtéséhez harmonizál (`-ből`), a helyőrző ismeretlen értéke nem befolyásolja. A keret a
  testvér `wizard.stepProgress` idiómája (`{step}. lépés a(z) {total}-ból`), az `a(z)` hedge pedig ott is, itt is a szám
  kiejtésének ismeretlensége miatt kell (`style.md`: `a három`, de `az öt`).
- **`Settings › Updates & privacy` → `Beállítások › Frissítések és adatvédelem`.** A két fél a szállított
  `settings.section.updatesAndPrivacy` (`Frissítések és adatvédelem`) és a katalógus `Beállítások ›` mintája
  (`settings.askCmdr.provider.shared`/`.off`). Az útvonal nem kap ragot: a mondat `itt:` kettősponttal vezeti fel
  (`vagy bármikor később itt: Beállítások › Frissítések és adatvédelem`), ugyanaz a kerülőút, mint az `itt: {path}`-nál.
- **`Local Network` → `Helyi hálózat`, szó szerint az Apple saját címkéje.** A kupac `hu/macOS/SystemSettings/` dumpja
  csak a Rendszerbeállítások keretszövegeit tartalmazza, az Adatvédelem panel engedélyneveit nem, ezért ez élőben
  olvasva jött: `SecurityPrivacyExtension.appex/…/Localizable.loctable`, `LOCAL_NETWORK` kulcs (macOS 26.6.2, 25G83
  build, `plutil`, 2026-09-09). Az `onboarding.stepOptional.networking.summary` és a `…desc` is ezt idézi, a ragozás
  (`engedélyezését`, `hozzáférést`) az idézőjelen kívülre kerül. ❌ Nem `Helyi hálózat elérése`: azt a nevet a
  felhasználó nem találja meg az Adatvédelem panelen.
- **A négy `stepOptional.*.summary` a testvér `desc` szavait viszi, és nem érhet túl az angol hosszán.** Egy sorba kell
  férniük a kapcsoló mellett: `1 GB helyet foglal…` (a `descCost` „nagyjából 1 GB index” szava),
  `…elnyomja a macOS kezelőjét` (a `mtp.desc` „el kell nyomnia azt a macOS folyamatot” igéje). Az `mtp.summary`-ból a
  hely kedvéért kimarad a `saját` jelző (angolul „native”); a teljes állítás a `desc`-ben van.
- **`openBeta` záró mondata és a `feedbackIntro` szándékosan más igét visz.** Az angol az egyiken `fix bugs`
  (`kijavítsam a hibákat`), a másikon `spot bugs` (`észrevegyem a hibákat`); a keret
  (`A visszajelzésed segít, hogy … és rangsoroljam a funkciókat`) viszont szó szerint azonos, hogy a két mondat egymás
  mellett ne olvasódjon két külön ígéretnek.
- **A `local.tooltip` harmadik sora a `stepAi.cloud.label`-t IDÉZI**, tehát a `<strong>` tartalma betű szerint
  `Igen, szeretnék AI-t` kell legyen. Ha az a címke valaha változik, ez a sor is vele megy; a két kulcs egy egység.
- **`dumber` → `butább`, tudatosan.** Az angol `@key.description` külön kiemeli, hogy a nyers szó szándékos; a magyar
  sem szépíti (`nem gyengébb`, `nem korlátozottabb`), mert a mondat egész értelme az őszinte figyelmeztetés.
- **Betűszó + rag kötőjellel: `RAM-ot`, `CPU-t`, `LLM-edet`, `AI-t`, `API-kulcsot`.** A katalógus `USB-` családjának
  mintája (`style.md`). A `CPU` kiejtése `cé-pé-ú`, tehát a tárgyrag `-t`; az `LLM` kiejtése `el-el-em`, tehát elöl
  hangrendű (`-edet`).
- **released copy / Dev and test builds → `kiadott verzió` / `fejlesztői és tesztbuildek`**
  (`settings.revealHandler.notProductionBuild`) · ms (`HUNGARIAN.tbx`: `build` → `build`, `release` → `kiadás`,
  `production build` → `élő build`), katalógus (`build mappákat` a `search.systemDirExclude.default`-ban, `Cmdr-verzió`
  a `commands.appCheckForUpdates.description`-ben) · high. `verzió`, nem `példány`: a mondat kiadásról szól, nem egy
  második futó példányról (`main.instanceLock.alertBody`). A második mondat a `settings.revealHandler.notInApplications`
  mintáját követi (`… törlése után minden „Megjelenítés a Finderben” kattintás a semmibe mutatna`), hogy a két tooltip
  testvérként olvasódjon.

## A megjelenítő betölti a telefonon, szerveren vagy archívumban lévő fájlt (`viewer.pull.*`, `viewer.error.stoppedResponding`)

A megjelenítőablak középső panelje, amíg a Cmdr egy távoli fájlt ideiglenes fájlba másol (cím, folyamatjelző, „x / y”
sor, Mégsem gomb), és az üzenet, ha kb. 45 másodpercig nem érkezik adat. Tier-1 bizonyíték a telepített macOS 26.6.2-ből
(build 25G83), 2026-09-10.

- **„Fetching” (a fájl áthozása megjelenítés előtt) → `Betöltés`** · a megjelenítő saját igéje (`viewer.loading` =
  `Betöltés…`, `viewer.error.timeout` = `Nem sikerült betölteni a fájlt.`), a „to preview” pedig ugyanennek a fájlnak a
  `megjelenít` töve (`viewer.error.tooLargeToPreview`) · `high`. ❌ NEM a Finder `Begyűjtés…` alakja (`IN_MD1`): az az
  Infó-ablak metaadat-gyűjtése, más jelentés. ❌ NEM `Letöltés` (Foundation `Progress.loctable` „Downloading”): egy
  archívumban lévő fájlt senki nem tölt le. A cím kettőspontos keretet kap, mint a szomszédos `viewer.content.ariaLabel`
  (`Fájltartalom: {fileName}`), így a `{fileName}` elé nem kell `a(z)`, és rag sem kerül rá.
- **„{done} of {total}” (a folyamatjelző alatti méretsor) → `{doneText} / {totalText}`** · Foundation
  `Progress.loctable`, `%@ of %@` kulcs → `%1$@ / %2$@`, pontosan az `NSProgress` „12,4 MB of 250 MB” sora; a Finder
  másolási sora (`PW3` `^0 / ^1 – ^2`) és a szállított `ai.toast.progress` (`{downloaded} / {total}`) ugyanez · `high`.
- **„{done} so far” (ismeretlen teljes méret) → `Eddig {doneText}`** · a „so far” mindenütt `eddig`, és az elöl álló,
  ige nélküli alak az élő találatszámlálóé (`queryUi.results.live.matchesSoFar` = `Eddig {countText} találat`) · `high`.
- **„stopped arriving” → `A fájl betöltése megállt.`** · a `megáll` az elakadt, de élő átvitel szava
  (`Az átvitel megállt`, lásd fent), a `betöltés` a cím töve, így az ablak egy igét használ · `high` a tőre; a `megáll`
  örökli a fenti bejegyzés `tentative` jelölését. A második mondat a szállított
  `errors.listing.couldntReadUnknown.suggestion` (`Ellenőrizd, hogy … még csatlakozik`) és a
  `viewer.error.tooLargeToPreview` `aztán` kötőszava.

## A telefon elavult indexe (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`.bodyPhone`)

A meghajtós elavult-index család telefonos változata: az ADB-n csatlakozó Android-telefon nem jelzi a saját
fájlváltozásait, ezért az indexe csatlakoztatva is sárga. A szöveg nem említheti a leválasztást.

- **phone: `telefon`** · ms (`HUNGARIAN.tbx`, `phone` → `telefon`, HUN; ugyanott `okostelefon` a másodlagos alak),
  katalógus (`adb.connect.*`, `adb.readiness.*`, `settings.adb.status.*`, `errors.listing.notConnected.*`,
  `fileExplorer.navigation.spaceMtpHint`) · high. ❌ NEM `okostelefon` (hosszabb, és a katalógus sehol nem használja) és
  nem `eszköz`: az a `device` szava (`style.md`), amely a kamerát és a Kindle-t is lefedi.
- **A család szavai a testvérkulcsokéi** · high: `index … elavulhatott` (`staleDialog.title`), `újbóli átvizsgálás`
  (`staleDialog.body`, `driveIndex.menuRescan`), `mappaméretek és … keresés`, `A meghajtó melletti sárga állapot` →
  `A telefon melletti sárga állapot`. A „current” szava `naprakész`, a `driveIndex.tooltipFresh`
  (`Indexelve és naprakész.`) alakja.
- **„the changes Cmdr makes” → `a saját változtatásaival`, „changes made on the phone” →
  `a telefonon végzett változtatások`** · leíró · tentative. Ugyanaz a főnév áll mindkét helyen, ahogy az angol is
  kétszer mondja a `changes`-t. A `a saját változtatásaival tartja naprakészen` szórendben a fókusz (ige előtti hely)
  hordozza a „csak a sajátjaival” jelentést, amit az angol a mondatpárral mond, ezért nem kell külön `csak`.
- **`{name}` a mondat elején: `A(z) {name} nem szól a Cmdrnek`** · a `style.md` `a(z)` szabálya és a szállított
  `driveIndex.refusedDisconnected` (`A(z) {name} le van választva.`) · high. Idézőjel nincs, mert a testvér
  `staleDialog.body` (`Amíg a(z) {name} le volt választva`) sem tesz, és a telefon neve jellemzően a gyártóé.
- **„stays as a reminder” → `emlékeztetőül ott marad`** · leíró, a katalógus `emlékeztető` szava (`settings.json`, a
  Space-emlékeztető leírása) · tentative. A pile-ban nincs `emlékeztetőül` / `emlékeztetőként` találat.

## A szerver gyökérmappája és kezdőmappája (`servers.sheet.rootFolder`/`.rootFolderHelp`/`.startFolder`/`.startFolderHelp`/`.nameHelp`, `servers.refusal.startFolderOutsideRoot`/`.rootNotFound`/`.startFolderNotFound`/`.saveUnconfirmed`)

A szerverlap két mappamezője: a gyökérmappa a plafon (a Cmdr sosem lép fölé), a kezdőmappa az, ahol a szerver megnyílik
(a gyökérmappa vagy egy benne lévő mappa; üresen a gyökérmappa).

- **root folder → `gyökérmappa`** · ms (`HUNGARIAN.tbx` `root folder` → `gyökérmappa`, mellette `gyökérkönyvtár` és
  `legfelső szintű mappa`), xf (Thunar „The root folder has no parent” = „A gyökérmappának nincs szülője”), Double
  Commander („Change directory to root” = „Váltás gyökérmappára”), Total Commander („a gyökér mappában”) · `high`. A
  pile macOS-ágában nincs találat. ❌ NEM `gyökérkönyvtár`: a `könyvtár` a technikai szó, a felület `mappa`-t mond
  (style.md). A törölt „Remote folder” címke (`Távoli mappa`) utódja, de az angol jelentése is megváltozott („Remote
  folder” → „Root folder”: a mező plafon lett), ezért a régi alak nem folytatható.
- **start folder → `kezdőmappa`** · Total Commander („&Induló mappa:”, egy parancsikon indítási mappája; „Az induló
  mappa nem létezik.”), Double Commander („in all start path” = „az összes kezdő útvonalban”), ms (`kezdőmappa` létezik,
  de a `home folder` jelentésben) · `tentative`. Tier-1 forrás nincs. A `kezdőmappa` azért nyer az `induló mappa` ellen,
  mert egyszavas összetétel, ami párban áll a `gyökérmappa`-val és a lap többi címkéjével (`Kulcsfájl`,
  `Kulcsjelmondat`), és a DC `kezdő-` előtagját viszi. A Microsoft-jelentés ütközése (home folder) macOS-en nem jön elő:
  a Finder a saját mappát `Saját mappa`-nak hívja (`TL_HELP_HOME` = „Ugrás a Saját mappájába”), és a katalógus is
  (`fileExplorer.unreachable.openHome` = „Saját mappa megnyitása”, `commands.navGoHome.label` = „Ugrás a saját
  mappára”). ❌ Soha ne `saját mappa` a kezdőmappára.
- **A `{host}` ragozatlan helyen áll**: `A Cmdr nem tudja megnyitni ezt a mappát itt: {host}.` a szállított
  `fileExplorer.network.share.notFound` (`… nem található itt: {hostName}`) idiómája, a `{host} nem válaszolt időben, …`
  pedig a `servers.refusal.timedOut` mondatkezdő alanya · `high`.
- **Leave it empty to … → `Hagyd üresen, hogy …`** · `settings.askCmdr.interactiveModel.description` („Hagyd üresen,
  hogy ugyanazt a modellt használja…”) · `high`.
- **account and host (a névtelen szerver megjelenített neve, `ada@nas.local`) → `a fiók és a cím`** ·
  `servers.sheet.identityLocked` („A cím és a fiók azonosítja ezt a szervert.”) és a lap `Cím` mezőcímkéje · `high`. ❌
  NEM `gép` / `gazdagép`: a lapon a mező neve `Cím`, és a súgósor arra mutat. Itt az `elnevez` ige a helyes, mert a sor
  épp a `Név` mező alatt áll (a `identityLocked` sor ezt szándékosan kerülte, lásd fent).
- **never goes above this folder → `soha nem lép ennél a mappánál feljebb`** · a `lép` a katalógus navigációs igéje
  (`errors.listing.*.suggestion`: „Lépj a szülőmappába”) · `high`.
- **Check that it exists and that your account can read it →
  `Ellenőrizd, hogy létezik-e, és hogy a fiókod olvashatja-e.`** ·
  `fileExplorer.navigation.driveIndex.refusedUpgradeFailed` („Ellenőrizd, hogy a megosztás elérhető-e”) · `high`.
- **nothing was saved → `a Cmdr nem mentett semmit`; Try again in a moment → `Próbáld újra egy pillanat múlva.`** ·
  cselekvő alak a style guide szerint; a második mondat szó szerint a `errors.volume.deletePending` és a
  `settings.mediaIndex.clip.deleteFailed` alakja · `high`.

## Miért nem csatolható egy megosztás, és miért nem töltődik be a megosztáslista (`errors.mount.*`, `errors.shareList.*`)

A mondatok a „Nem sikerült csatolni a megosztást” (`fileExplorer.networkMount.mountFailedTitle`) és a „Nem sikerült
csatlakozni ehhez: {hostName}” (`fileExplorer.network.share.connectFailedTitle`) cím alatt, plusz a
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` toast és a `servers.refusal.accountNotPermitted` sor. Tier 1 az
élő `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11): pontosan a
„Kapcsolódás szerverre” hibaeseteit fogalmazza meg, és a kupacban nincs benne.

- **Az idézett helyőrző alaptagot kap: `a(z) „{server}” szerver…`, `a(z) „{share}” megosztás…`** · mac NetAuthAgent
  `EMSG_CONNECTION_FAILED` („a(z) „%@” szerverhez való kapcsolódás”), `EINFO_NO_SHARE` („A(z) „%@” megosztás nem létezik
  a szerveren.”), és a `style.md` `A(z) „{name}”` szabálya · `high`. A rag mindig az alaptagra kerül (`szervert`,
  `szerverhez`, `szerveren`, `megosztást`), a helyőrző ragozatlan marad. Az angol itt idézi a neveket, ezért ez
  természetesebb, mint a kettőspontos `ehhez: {host}` keret.
- **share → `megosztás`**, **guest → `vendég`** · NetAuthAgent `EINFO_NO_SHARE`, `EINFO_NO_ACCESS_GUEST` („Ez a szerver
  nem engedélyezi a vendéghozzáférést.”) · `high`
- **connect (megosztáshoz) → `csatlakoz-`** · a mellette álló szállított cím
  (`fileExplorer.network.share.connectFailedTitle`) és `fileExplorer.navigation.driveIndex.refusedUpgradeFailed`
  („közvetlenül csatlakozni”) · `high`. Az Apple `kapcsolódás` töve a panel nyitósorában marad
  (`servers.paneState.connecting`).
- **„Cmdr couldn't reach” → `A Cmdr nem tudta elérni`** · a szótár `Cmdr couldn't … → A Cmdr nem tudta …` sora · `high`
- **„didn't answer in time” → `nem válaszolt időben`**, **„isn't responding” → `nem válaszol`** ·
  `servers.refusal.timedOut`, `adb.readiness.offline`, `fileOperations.transferDialog.scanUnresponsive` · `high`
- **„turned on … same network” → `be van-e kapcsolva, és ugyanazon a hálózaton van-e`** ·
  `errors.listing.hostUnreachable.suggestion` · `high`. **this computer → `ez a számítógép`**: a mondatok Linuxon is
  megjelennek, ezért nem `Mac`.
- **account → `fiók`**, **„as {username}” → `„{username}” néven`** · a szótár `the account` sora; a `néven` nem ragozza
  a helyőrzőt · `high`
- **Something went wrong → `Váratlan probléma történt, miközben a Cmdr …`** · `style.md`: a próza problémaszava
  `probléma`; a kezdés az `errors.write.fallback.message.copy` alakja · `high`. Az alany `a Cmdr`, mert egy
  `…megosztáshoz való csatlakozáskor` névszói lánc nehezen olvasható.
- **package → `csomag`** · `licensing.acknowledgements.npmHeading` („npm csomagok”), ms `Csomagkezelő` · `high`.
  **distribution (Linux) → `disztribúció`** · egyik forrásban sincs Linux-értelemben (az ms `elosztás` a logisztikai
  jelentés) · `tentative`. Az `az smbclient` névelője `az`, mert a betűszó kiejtése magánhangzóval kezdődik; a
  `gvfs-smb csomagját` alaptag viszi a ragot.
- **„SMB 2 or later” → `az SMB 2-es vagy újabb verzióját`** · a `-es` a számra, a birtokos rag a `verzió`-ra kerül ·
  `high`
- **„there's no connection to speed up” → `így nincs mit felgyorsítani`** · az `errors.eject.notAnSmbVolume` („így nincs
  mit leválasztani”) kerete · `high`
- **„Try again in a moment” → `Próbáld újra egy pillanat múlva.`** · `errors.volume.deletePending` · `high`
- Az azonos angol értékek magyarja is azonos: `errors.mount.hostUnreachable` = `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` = `errors.shareList.authFailed`. Aposztróf egyik értékben sincs.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `szövegszerkesztő`** · Microsoft-terminológia (`text editor` → `szövegszerkesztő`; mellette
  `szerkesztő`, `szerkesztőprogram`) · `high`. Az app-fajta neve, ❌ nem az Apple TextEdit appja, amelynek a neve a
  `{app}` helyére kerül.
- **default text editor → `alapértelmezett szövegszerkesztő`** · a katalógus `alapértelmezett szerkesztő` szerkezete
  (`commands.fileEdit.label`) · `high`.
- **Edit files in [app] → `Szerkesztésre használt app`** · a magyar címke nem folytatódhat toldalékkal egy ismeretlen
  appnévbe, ezért névszói címke, ahogy a terminál sora (`settings.behavior.openTerminalHereApp.label`) · `tentative`.
- **`{app}` toldalék nélkül**: „a fájl ebben nyílt meg: {app}”, mert a `-ban/-ben` illeszkedése a futásidőben érkező
  névtől függene. Dismiss és Open settings azonos a `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings` fordításával.
- **system default → `Rendszer szerinti`** · a katalógus (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) · `high`. A `settings.behavior.textEditorApp.systemDefault`
  zárójelben teszi mögé az app nevét, mint a `settings.appearance.language.opt.systemWithLanguage`.
- A „Choose an app…” és a „Checking your apps…” azonos a `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking` fordításával. A tipp (`fileExplorer.edit.hint`) a
  `commands.handler.openTerminalHere.hint` mintáját követi, a beállítás helye nélkül, mert a gombja egyenesen odavisz; a
  `{app}` itt is toldalék nélkül áll.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (kiadás vagy leválasztás folyamatban) → `A(z) {name} leválasztása folyamatban van`** · a
  szándékos kiadásra már rögzített `leválasztás` (lásd fent: a magától megszakadó kapcsolat `megszakad a kapcsolat`, ez
  nem az), a Thunar `hu` „Eszköz leválasztása” alakjára építve · `high`. A főnévi szerkezet miatt a `{name}` nem kap
  toldalékot. „Left its index as it was” → „úgy hagyta …, ahogy volt”, mint az
  `operationLog.rollback.partiallyRolledBackNotice`; „try again in a moment” → „egy pillanat múlva”, mint a
  `fileExplorer.pane.directConnectionMountNotRespondingToast`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (már megtörtént, a meghajtó nincs ott) → `leválasztódott`** · a rögzített `leválaszt` mediopasszív
  alakja; az `indexing.staleDialog.body` ugyanerre az állapotra a „le volt választva” formát használja · a szótő `high`,
  a mediopasszív alak `tentative` (a referenciákban a tárgyas „Eszköz leválasztása” szerepel, cselekvő nélküli múltra
  nincs bennük minta). Szándékosan NEM a `fileExplorer.navigation.driveIndex.driveLeaving` folyamatban lévő alakja
  („leválasztása folyamatban van”). „Starts from scratch” → „az elejéről”, mint az
  `indexing.rescan.incompletePreviousScan`; az `átvizsgálás` és a `mappaméretek` ugyanebből a fájlból.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

Négy kulcs: ugyanaz az átviteli hibapanel, mint a `errors.write.deviceDisconnected.title` alattiak, de megnevezi, MELYIK
meghajtó tűnt el, és megmondja, hol vannak most a fájlok. A felhasználó ezt egy kirántott meghajtó után olvassa, tehát a
mondat súlya a megnyugtatáson van, nem a diagnózison. RAW család (nincs ICU), egyszeres aposztróf; a négy érték
egyikében sincs aposztróf. A `{volumeName}`, `{counterpart}`, `{done}` és `{total}` fordíthatatlan, és
`{done}`/`{total}` már kész sztringként érkezik, tehát semmilyen szám-formázás nem kerülhet rájuk.

- **„was disconnected” (a meghajtót átvitel közben kihúzták) → `leválasztódott`** · a tegnapi testvérkulcs,
  `indexing.needsFreshScan.afterDisconnect` pontosan ezt az eseményt (menet közben kihúzott meghajtó)
  `A(z) {name} leválasztódott` alakkal mondja; a `leválaszt` tő a `style.md` Tier-1 döntése (macOS „Leválaszt”,
  „Kapcsolat bontása”) · a tő `high`, a mediopasszív alak `tentative` (a referenciákban csak a tárgyas „Eszköz
  leválasztása” szerepel, cselekvő nélküli múltra nincs mintájuk).
- **A `leválasztás` / `megszakad a kapcsolat` hasadásból SZÁNDÉKOSAN a `leválasztás` oldala.** A `megszakad a kapcsolat`
  alakot a katalógus a magától elhaló HÁLÓZATI kapcsolatra tartja fenn (`errors.volume.deviceDisconnected`,
  `errors.write.connectionInterrupted.title`); itt viszont egy fizikai meghajtó tűnt el a csatolási táblából, ami
  ugyanaz az esemény, mint a fenti két testvérkulcsé. Hogy a felhasználó rántotta-e ki vagy szoftver adta ki, ezekből a
  kulcsokból nem derül ki (az angol „disconnected” sem mondja meg), és a magyar `leválasztódott` pont ilyen ágensmentes
  · `high`.
- **❌ NEM `A(z) {volumeName}-t leválasztották`**, pedig a panel testvére
  (`errors.write.deviceDisconnected.message.copy`) a tárgyas `Az eszközt leválasztották` alakot használja: ott nincs
  helyőrző, itt viszont a `{volumeName}` tárgyragot kapna, amit egy ismeretlen kiejtésű névhez nem lehet illeszteni. A
  mediopasszív alak az egyetlen, amelyben a név ragozatlan marad. Ugyanezért `A(z)` a névelő, a `driveLeaving`
  mintájára.
- **A meghajtónév idézőjel NÉLKÜL áll** (`A(z) {volumeName} leválasztódott`) · a két testvérkulcs
  (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`) is így írja · `high`. A
  `style.md` idézőjel-szabálya a felhasználó által ÍRT névre való; egy meghajtónevet a mondat maga elhatárol.
- **„{done} of {total} files” → `{total} fájlból {done} fájlt`** · a magyar sorrend fordított, és így a `-ból` rag a
  `fájl` szóra kerül, nem a helyőrzőre · `high`. A számnév után a főnév egyes számban marad (`style.md` § Plurals).
- **„to {counterpart}” → `ide: {counterpart}`** · a katalógus és a macOS Tier-1 bevett deiktikus szerkezete
  (`Áthelyezés ide: %@`; lásd fent § A műveleti sor), és ragmentes · `high`. Mondatzáró kettőspont után a pont
  következik, ahogy a `errors.write.newDataKeptAt.message` teszi.
- **„all your files are still on {counterpart}” → `minden fájlod továbbra is a(z) {counterpart} meghajtón van`** · itt
  nem irány, hanem HELY kell, amit a kettőspontos alak nem ad ki, ezért alaptagos szerkezet: a `meghajtó` viseli a
  ragot, a név ragozatlan · `high`. Ugyanaz a fogás, mint az `az Android platform tools csomag`-nál (`style.md`).
- **„The rest are still on the drive.” → `A többi továbbra is a meghajtón van.`** · a `többi` alakra a Thunar `hu`
  („folytathatja a többi fájl átnevezésével”) és a Dolphin `hu` („A többi lap bezárása”) ad fedezetet, a `meghajtó` a
  `style.md` szótári szava · `high`. Külön mondat, ahogy az angolban, hogy a megnyugtatás önállóan álljon.
- **„Your originals are untouched…, so nothing is lost.” →
  `Az eredetiek érintetlenül a helyükön vannak, így semmi sem veszett el.`** · az `eredetiek` és az `érintetlen` a
  szállított `errors.write.destinationNotFound.message.copy` szavai („Az eredetiek érintetlenek.”), a `helyükön` a
  `errors.write.readOnlyDevice.source.suggestion`-é („Az eredetiek a helyükön maradnak.”); a macOS Tier-1 a főnévre is
  ad fedezetet (Finder `InfoWindowGeneralView` „Eredeti:”, „Új eredeti kijelölése…”) · az `eredeti` `high`, az
  `érintetlen` `high` a katalógusból, de a `hu` kupacban NULLA találata van, tehát kupac-forrásból `tentative`.
- **„nothing is lost” → `semmi sem veszett el`**, nem `nincs adatvesztés` · a `hu` kupac egyetlen közeli alakja a Double
  Commander `adatvesztést okozhat` figyelmeztetése, ami rémisztő szakregiszter; a mi mondatunk épp az ellenkezőjét
  állítja, és a hangunk köznyelvi · `tentative`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

Négy kulcs, egy panel. A művelet NEM kudarc: a másolatok valószínűleg megvannak, a Cmdr csak nem tudta bizonyítani, és
épp ezért hagyta meg az eredetiket. A magyar sem fogalmazhat kudarcként, és nem erősítheti fel a „couldn't confirm”-ot
„elromlott az áthelyezés” irányba.

- **„confirm” (itt: meggyőződni róla, hogy a fájlok kiíródtak) → `ellenőrizni`, ❌ NEM `megerősíteni`** · a szállított
  `fileOperations.cancelRollback.reason.unverifiable.named` ugyanerre az ismeretelméleti jelentésre már ezt mondja („a
  Cmdr nem tudta ellenőrizni, hogy módosult-e”) · `high`. A macOS Tier-1 `Confirm` → `Megerősítés` (AppKit `Common`) a
  JÓVÁHAGYÁS jelentésre való; magyarul a `megerősíteni az áthelyezést` úgy olvasódna, mintha a felhasználónak kellett
  volna rábólintania, ami tárgyi tévedés lenne.
- **„Couldn't confirm the move” → `Nem sikerült ellenőrizni az áthelyezést`** · a `nem sikerült …` a settled alak a
  „couldn't”-ra (lásd fent a hibakulcsok második passzának szakaszát), és a panelcímek szállított regisztere is ez
  (`errors.write.sourceNotFound.title`, `errors.write.destinationNotFound.title`, `errors.write.permissionDenied.title`)
  · `high`. Nincs benne se `hiba`, se `sikertelen` címke.
- **„were saved on {volumeName}” → `kiíródtak-e a(z) {volumeName} meghajtóra`** · a `kiír` a katalógus szava a lemezre
  írásra (`errors.write.newDataKeptAt.message` „teljesen kiírva megvan”, `fileOperations.rollbackConfirm.body` „amit a
  művelet eddig kiírt”) · `high`. Az `elment` igét kerüljük: a mediopasszív `elmentődött` csúnya, a tárgyas `elmentette`
  pedig hamis ágenst adna. A helyőrző itt is alaptagot (`meghajtóra`) kap, hogy ragozatlan maradjon.
- **„at the destination” → `a célhelyre`** · a settled `célhely` (`terms.json` `destination`), irányraggal, mert a
  `kiíródik` hova-kérdésre felel · `high`. A névtelen és a neves változat ettől eltekintve betűre azonos, ahogy az
  angolban is.
- **„so it kept your originals where they were” → `ezért az eredetieket a helyükön hagyta`** · ugyanaz az `eredeti` +
  `helyükön` pár, mint a fenti szakaszban · `high`. A magyar ok-okozat (`ezért`) az angol `so`-t viszi, és a mondat
  végére teszi a megnyugtatást, ahogy az angol.
- **„Your originals haven't moved.” → `Az eredetiek nem mozdultak el.`** · szándékosan MÁS alak, mint a fölötte álló
  üzenet `a helyükön hagyta` fordulata, mert az angol is variál (`kept … where they were` / `haven't moved`), és két
  egymás alatti sorban a szó szerinti ismétlés magyarul feltűnőbb · `high`.
- **„Have a look at the destination” → `Nézd meg a célhelyet`** · tegező felszólítás, a `style.md` szerinti
  regiszterben; a `célhely` tárgyesete rendes szó, nem helyőrző, tehát ragozható · `high`.

## A visszadugott meghajtón maradt áthelyezési munkamappa (`fileOperations.leftovers.stagingFolderKept`)

Tájékoztató buborék, amikor egy meghajtó visszakerül (vagy a Cmdr elindul), és egy soha be nem fejezett áthelyezés
munkamappája még fájlokat tartalmaz. A Cmdr SZÁNDÉKOSAN mindent a helyén hagy: ezek lehetnek az illető egyetlen
példányai, mert az áthelyezés az eredetieket már eltávolíthatta. Nincs se kérés, se veszély; a mondat egyetlen
cselekvésre bírható tartalma az, hogy a mappa REJTETT. ❌ Soha nem javasoljuk a törlését. ICU-fájl, tehát minden
aposztróf kettőződne; ebben az értékben nincs egy sem.

- **„unfinished” (egy el nem készült MŰVELET, nem egy hiányos fájl) → `félbemaradt`** · a tő fedezete erős: a szállított
  `errors.listing.deviceReconnecting.explanation` a „canceled or interrupted transfer”-t
  `megszakított vagy félbeszakadt átvitel` alakkal mondja, a Microsoft-terminológia pedig hozza a `félbehagy` címszót; a
  macOS Tier 1 az igés alakra ad mintát („did not finish in time” → `nem fejeződött be időben`), a Double Commander
  ugyanígy (`Néhány fájlművelet még nem fejeződött be.`) · a `félbe-` tő `high`, a konkrét `félbemaradt` alak
  `tentative` (a kupacban nincs rá találat; a negyedik bányászati gotcha „közös tő = bizonyíték” esete).
  - ❌ **NEM `befejezetlen`**: a `hu` kupacban ez a szó KIZÁRÓLAG könyvelési terminus (`befejezetlen termelés`,
    `befejezetlen beruházás`), tehát rossz jelentésbokor (a negyedik forráscsapda: az első találat gyakran nem a
    felületi jelentés).
  - ❌ **NEM `félbeszakadt`**, pedig az a szállított alak: azt a katalógus az „interrupted”-re foglalta le (lásd fent),
    és okot is sugall (valami elvágta). Az angol „unfinished” ágensmentes, a `félbemaradt` pontosan az.
  - ❌ **NEM `hiányos`**, pedig a legközelebbi rokon, a `fileOperations.cancelRollback.stagedLeftover.named` azt mondja:
    ott az „unfinished copy” egy FÉLIG MEGÍRT FÁJL (hiányos tárgy), itt egy be nem fejezett MŰVELET. A két kulcs
    regisztere is szemben áll: ott a Cmdr a saját maradékáról számol be, itt az illető fájljait védi.
- **„hidden” (a pont kezdetű, listából kimaradó értelemben) → `rejtett`** · a szállított katalógus egyöntetű
  (`menu.view.showHiddenFiles`, `settings.listing.showHiddenFiles.label`, `commands.viewShowHidden.label`,
  `fileExplorer.rename.hiddenAfterRename`), és a kupac is: KDE Dolphin hu („rejtett mappákat”, „A nevükben ponttal
  kezdődő mappák rejtettek”), Thunar hu, Nautilus hu, Total Commander hu · `high`.
- **„left them in place” → `a helyükön hagyta őket`** · a settled alak, szó szerint az
  `errors.write.moveNotConfirmed.message.named` záró fordulata (`az eredetieket a helyükön hagyta`), rokona az
  `errors.write.readOnlyDevice.source.suggestion` (`Az eredetiek a helyükön maradnak.`) · `high`. A `helyükön` szórendi
  fókuszba kerül, tehát épp a megnyugtatás hangsúlyos, ahogy az angolban.
- **„found” → `találta`, ❌ nem `bukkant rá` és nem `megtalálta`** · a `talál` a katalógus igéje a felfedezésre
  (`errors.listing.symlinkLoopErrno.explanation`: „körkörös láncát találta itt”), míg a `bukkan` egyszer sem szerepel
  benne · `high`. A `megtalálta` semleges szórendje azt sugallná, hogy a Cmdr kereste őket; az igekötőtlen, fókuszos
  „X-et találta” pont azt mondja, AMIT talált, ami ennek a buboréknak a hírértéke.

Helyőrző-kerülések (mindkettő ragozatlanul marad):

- **`{volumeName}` → `a(z) {volumeName} meghajtón`** · a katalógus bevett fogása: alaptag viseli a ragot, a névhez nem
  tapad semmi (`errors.write.moveNotConfirmed.message.named`: `a(z) {volumeName} meghajtóra`,
  `errors.write.deviceDisconnected.sided.destination.move`: `a(z) {counterpart} meghajtón`,
  `fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`) · `high`. A meghajtónév
  itt is idézőjel NÉLKÜL áll, a testvérkulcsok mintájára.
- **`{folderName}` → `egy rejtett, „{folderName}” nevű mappában`** · a `nevű` szerkezet a magyar szabvány megoldása
  arra, hogy egy ismeretlen kiejtésű név toldalék nélkül maradjon, és a macOS Tier 1 egyöntetűen így írja (Finder `PE1`,
  `PE62.2`, `PE68.1`: „Már létezik egy „^0” nevű elem ezen a helyen.”; AppKit: „Már létezik egy „%@” nevű elem ezen a
  helyen.”), a Nautilus hu szintén („„%s” nevű elem már létezik ezen a helyen.”) · `high`. Az `egy` határozatlan névelő
  semmivel nem egyeztet, tehát a név kezdőhangja közömbös.
  - **Itt KELL az idézőjel**, a `style.md` „csak a felhasználó által írt névre” szabálya ellenére: a név 45 karakternyi
    átlátszatlan azonosító (`.cmdr-staging-` + egy UUID), amit az illetőnek a Finderben meg kell találnia, tehát a
    mondattól el kell határolni. A legközelebbi rokon, a `fileOperations.cancelRollback.stagedLeftover.named` szintén
    idézőjelezi a Cmdr saját gyártású munkanevét (`A(z) „{name}”`). A szabály tiltó fele a MÁRKA- és szolgáltatónevekre
    vonatkozik, ahol az idézőjel gúnyosnak olvasódna.
  - A `rejtett` szándékosan az idézőjeles név ELÉ került: ez az egyetlen tartalom, amivel az illető kezdeni tud valamit,
    és így nem a hosszú azonosító után kell megkeresnie.

## A kedvencek menüje (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`, `fileExplorer.navigation.favoritesAlreadyAdded`, `fileExplorer.navigation.favoritesCantAddHere`, `fileExplorer.navigation.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

Tíz új kulcs: a ⌃D a fókuszált panel fölött nyitja meg a kedvencek listáját menüként, az első kilenc sor mellett egy-egy
számbillentyűvel, a `0` sor pedig hozzáadja a panel aktuális mappáját. A kötetválasztóból eltűnt a Kedvencek SZAKASZ, a
helyén egy sor áll, ami átvált erre a menüre.

- **A lista szava marad `kedvenc` / `Kedvencek`**, a `style.md` szótári sora és a szállított alakok szerint
  (`fileExplorer.navigation.groupFavorites` = `Kedvencek`, `commands.favoritesAdd.label` = `Hozzáadás a kedvencekhez`,
  `menu.go.addToFavorites`, `fileExplorer.navigation.favoritesEmpty` = `(A kedvenceid itt jelennek meg)`) · `high`. ❌
  Erre a listára soha nem `könyvjelző`: az a szótárban a _bookmark_ tentative sora, más funkcióé.
- **favorites menu (a FELÜLET NEVE, címpozícióban) → `Kedvencek menü`** · macOS Tier 1 (`Apple menü`), Double Commander
  hu (`Fanézet menü`, `Fastruktúra menü`), Nautilus hu (`Parancsfájlok menüben`), és a katalógus saját `a Súgó menüből`
  alakja (`style.md` § Notes) · `high`. Az értelmezős (név + `menü`) szerkezet a magyar UI szabványa, és a
  `shortcuts.scope.*` szomszédai is rövid felületnevek (`Kötetválasztó`, `Parancspaletta`, `Fájllista`).
- **favorites menu (PRÓZÁBAN, közszóként) → `a kedvencek menüje`** · Nautilus hu (`mappa menüje`, `mappa menüjének`) ·
  `high`. Az angol is kisbetűvel, köznévként írja („the favorites menu”), tehát itt a birtokos szerkezet a természetes;
  az értelmezős alak a CÍMÉ. A kettő tudatos, nem elcsúszás: egy táblázatcímke nem mondat.
- **Show favorites → `Kedvencek megjelenítése`** · macOS Tier 1: a Finder kivétel nélkül `X megjelenítése` alakban hozza
  a „Show X”-et (`Oldalsáv megjelenítése`, `Állapotsor megjelenítése`, `Útvonalsor megjelenítése`,
  `Összes megjelenítése`, `Infó megjelenítése`) · `high`. A `commands.favoritesOpen.label` és a `menu.go.showFavorites`
  angolja betű szerint azonos, tehát a magyarnak is egynek kell lennie (`desktop-i18n-term-consistency`) — és a natív
  Ugrás menüben amúgy is pont a Finder szóhasználata kell.
- **See {count} favorites → `{count} kedvenc megtekintése`** · a `view (a művelet) → megtekintés` szótári sor (macOS
  Tier 1) · `high`. Szándékosan NEM `megjelenítése`: az angol is két igét használ a két felületen (`See` a kötetválasztó
  sorában, `Show` a parancsnál/menüben), és a magyarban is jobb, ha a sor nem a parancs nevét ismétli.
- **A `seeFavorites` ágai: `=0` + `one` + `other`, és a két számos ág SZÖVEGE azonos** · CLDR `hu` = `one`/`other`
  (`style.md` § Plurals), a számnév után a főnév egyes számú, és semmi más nem egyeztet a mondatban, tehát az
  `1 kedvenc megtekintése` és a `3 kedvenc megtekintése` ugyanaz a séma · `high`. Ugyanaz a helyzet, mint a szállított
  `servers.hub.rowCount`-nál. Az ICU megköveteli az `other` ágat, az `one` pedig valódi `hu` kategória, ezért mindkettő
  kiírva marad. A `=0` ág az angol szándéka szerint nem mond nullát: `Kedvencek megtekintése`, `{count}` nélkül.
- **current folder → `(az) aktuális mappa`** · az `aktuális` a katalógus szava a „current”-re
  (`commands.editPaste.description`, `commands.favoritesAdd.description` = `a fókuszált panel aktuális mappáját`) ·
  `high`. A `0` sornak EGYETLEN kulcsa van, a `fileExplorer.navigation.favoritesAddCurrent`, névelő nélkül,
  menüpontszerűen rövid: `Aktuális mappa hozzáadása a kedvencekhez`. A billentyűparancs-lista ugyanezt a sort IDÉZI, nem
  újrafogalmazza, tehát ugyanezt az értéket olvassa · `high`. ❌ Ne írj hozzá külön, névelős változatot. Ami megmarad
  különbségnek: a valódi parancs, a `commands.favoritesAdd.label` (`Hozzáadás a kedvencekhez`), tárgy nélkül.
- **mounted share → `csatolt megosztás`** · macOS Tier 1 (`mount` = `csatol`: „Csatolja a kötetet…”, „nem csatolható”,
  „nem sikerült felcsatolni”), Nautilus hu („Ez a fájl nem csatolható”), plusz a szótár `share → megosztás` sora ·
  `high`. ❌ NEM `csatlakoztatott` (az a Double Commander eszközszava) és ❌ nem protokollnév: az angol tudatosan kerüli
  az SMB/MTP/ADB említését, mert a felhasználónak az számít, hogy a Mac maga csatolta-e.
- **disk (ebben a buborékban) → `lemez`** · macOS Tier 1 (`Adja ki a lemezt, és csatoljon le szervereket`) · `high`. Itt
  az angol `disk`-et mond, nem `drive`-ot, tehát nem a szótár `drive → meghajtó` sora a mérvadó.
- **A `favoritesCantAddHere` erről a mappáról mond tényt, az indok a kettőspont után jön** ·
  `Ez a mappa nem lehet kedvenc: a kedvencek csak lemezen és csatolt megosztáson működnek` · `high`. Ugyanazzal az
  alannyal indul, mint a testvére (`Ez a mappa már a kedvencek között van`), így a két szürke sor párban olvasható. ❌
  Már nem `mutat` a rá: az `-ra/-re` vonzat minden ragozó nyelvben esetválasztásra kényszerít a célpont felett, a
  `lemezen … működnek` viszont sima helyhatározó. A buborék tényközlés, nem hibaüzenet, ezért nincs benne se
  `nem sikerült`, se `hiba`; pont sincs a végén.
- **`commands.favoritesAdd.description` már nem a váltó Kedvencek listáját nevezi meg** (az a szakasz megszűnt), hanem
  azt, hova kerül a mappa, és mire jó ez:
  `A fókuszált panel aktuális mappáját hozzáadja a kedvencekhez, hogy a kedvencek menüjéből bármikor visszatérhess oda.`
  · `high`. A menü neve prózában birtokos szerkezet (`a kedvencek menüje`), a `shortcuts.scope.favoritesMenu` címében
  viszont értelmezős (`Kedvencek menü`) — lásd fentebb.
- **`commands.favoritesOpen.description` megnevezi az ugrás célját** ·
  `…, ahol egy szám megnyomásával az adott kedvencre ugorhatsz.` Az angol korábban csak „press a number to go”-t
  mondott, cél nélkül; most kimondja („jump to that favorite”), és a magyar ugyanazt a `az adott kedvenc` alakot
  használja, mint a `commands.favoritesOpenByNumber.label` (`Az adott számú kedvenc megnyitása`) · `high`.
- **„This folder is already a favorite” → `Ez a mappa már a kedvencek között van`** · a lista-metafora a katalógus
  egészében a `kedvencek` halmaz (`Hozzáadás a kedvencekhez`, `Eltávolítás a kedvencekből` – macOS `100384.title`) ·
  `high`. A rövidebb `már kedvenc` állítmányi alak nyelvtanilag rendben van, de a `között van` mondja ki, hogy a
  listáról van szó, amit a felhasználó épp lát. Pont nincs a végén, ahogy az angolban sincs (buborék, nem mondat).
- **Egyik érték sem azonos az angollal**, tehát `sameAsSourceJustification` egyikhez sem kell. Aposztróf egyikben sincs,
  így az ICU-kettőzés kérdése fel sem merül; a `menu.go.showFavorites` RAW kulcs, de nincs benne mit kettőzni.

## Az elutasított kiadás megnevezi, KI fogja a meghajtót (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

Hat kulcs, mind ugyanabban a buborékban, a `fileExplorer.pane.ejectFailedToast`
(`Nem sikerült kiadni: {volumeName}: {message}`) vagy a `fileExplorer.pane.disconnectFailedToast`
(`Nem sikerült a leválasztás: {message}`) kettőspontja után. RAW család (nincs ICU): egyszeres aposztróf, a `{app}` /
`{apps}` puszta szövegcsere. A hatból egyik értékben sincs aposztróf. Az `unmountRefused`
(`Valami még használja ezt a meghajtót. …`) a család generikus tagja, tehát a megnevezett változatok ugyanazt a
mondatvázat viszik: **`X még használja ezt a meghajtót.` + egy tegező felszólítás**.

- **„is still using this drive” → `még használja ezt a meghajtót`** · a szállított `errors.eject.unmountRefused` alakja
  betűre, és a `használ` tő a macOS Tier 1 kiadás-elutasításaié (AppKit `AppKitErrors`: „The disk could not be ejected
  because it is in use by „%@”.” → „… a(z) „%@” által használatban van.”, Finder `NE66`/`NE31`/`NE79`/`NE80`;
  ellenőrizve a referenciakupac `hu/macOS/` állományaiban, 2026-09-16) · `high`. A magyar az AKTÍV alakot tartja meg (az
  angol is azt mondja), nem az Apple szenvedő `által használatban van` szerkezetét: annak a mondatnak amúgy is hibás a
  magyar burkolata (az Apple `A lemezt sikerült kiadni`-t ír a „could not be ejected” helyett), tehát keretként nem is
  másolható.
- **`{app}` névelője a `A(z)` hedge** · a katalógus ugyanezt a szerkezetet szállítja appnév-helyőrzőre
  (`commands.handler.openTerminalHere.appMissing`: `A(z) {app} már nincs telepítve, …`, `errors.provider.appBased.*`:
  `a(z) {app} appot`, `main.revealNudge.heldByOtherApp`: `a(z) {app} appnál`), és a macOS Tier 1 is hedge-el ugyanebben
  a mondatban (`a(z) „%@” által`) · `high`. Az `{app}` futásidejű folyamatnév (`Preview`, `Warp`, `mds_stores`), tehát a
  kezdőhangja ismeretlen: a hedge pont erre való (`style.md` § Notes and decisions).
  - **Idézőjel NÉLKÜL**, pedig a macOS idézőjelezi: a `style.md` szabálya szerint az idézőjel a felhasználó által ÍRT
    névre való, a márka- és appnév-helyőrző pedig puszta `a(z) {name}` (így írja mind a három szállított kulcs is).
- **A TÖBBES kulcs (`unmountRefusedByApps`) nem kaphat névelőt** · a `{apps}` már kész, `Intl.ListFormat`-tal
  összefűzött felsorolás (`Preview, Warp, Photos és egyéb alkalmazások`, ellenőrizve `node`-dal, `hu` locale,
  2026-09-16), a magyar felsorolásban viszont minden tag SAJÁT névelőt kívánna, amit egy helyőrzőbe nem lehet belefűzni;
  ráadásul a záró `egyéb alkalmazások` határozatlan, tehát elé végképp nem kerülhet `a(z)`. Ezért a lista névelőtlenül,
  mondatkezdő alanyként áll · `high`. Az egyes és a többes kulcs közti névelő-aszimmetria (`A(z) {app}` vs. puszta
  `{apps}`) SZÁNDÉKOS és nyelvtani kényszer; ne „egységesítse” egy későbbi menet.
- **A többes alany TÖBBES igét kap: `még használják`** · két egyes számú név mellett a magyar egyes számú állítmányt is
  megengedne (`Preview és Warp még használja`), de a lista utolsó tagja gyakran a többes `egyéb alkalmazások`, ami
  mellett az egyes szám hibás. Egyetlen alak kell mindkét esetre, tehát a többes az · `high`. (Ez nem mond ellent a
  `style.md` § Plurals számnév-szabályának: ott SZÁMNÉV az alany, itt felsorolás.)
- **„Close anything it/they have open there” → `Zárd be, amit ott nyitva tart/tartanak`** · a `nyitva tart` a katalógus
  igéje erre (`errors.listing.deletePending.suggestion`: „Zárj be minden más appot, ami nyitva tarthatja ezt a fájlt”) ·
  `high`. A vonatkozó mellékmondat tárgyként határozott ragozást kíván (`Zárd be, amit…`), az `ott` pedig az angol
  „there”-t viszi, és kerüli a `meghajtó` harmadik említését.
- **other apps → `egyéb alkalmazások`** · Thunar hu közvetlen msgid-egyezés (`Other Applications` =
  `Egyéb alkalmazások`, `thunar-chooser-model.c:330`); a főnév a macOS Tier 1 `alkalmazás`-a, és a szállított
  testvérkulcs is ezt viszi (`errors.eject.unmountRefused`: „a nyitott fájlokat és alkalmazásokat”) · a főnév `high`, az
  `egyéb` vs. `más` választás `tentative`. A `más alkalmazások` is élő alak (Dolphin hu: „Akár más alkalmazásokba is”),
  de arra nincs msgid-szintű egyezés, és a `más` kontrasztot (= másfélét) sugall, míg itt maradékról van szó. Kisbetűs,
  mert mondat közepén, felsorolás utolsó tagjaként áll, alanyesetben, ahogy a `használják` kívánja.
- **disk image → `lemezkép`** · macOS Tier 1 (Finder `BN53`: „A(z) „^0” lemezkép lemezre írása…”,
  `InfoWindowGeneralView` „Lemezkép:”, „Lemezkép értéke”) · `high`.
  - **„is still open” → `még nyitva van`** · Double Commander hu közvetlen msgid-egyezés („the file is open in another
    program” = „a fájl más programban nyitva van”) · `high`. Nem `csatolva van`: az angol is a köznyelvi „open”-t
    mondja, és a felhasználó a lemezképet a Finderben nyitva látja.
  - A mondat egzisztenciális szórendű (`Ezen a meghajtón még nyitva van egy lemezkép.`), mert a magyarban a határozatlan
    alany az ige MÖGÉ kerül; az angol jelzős szerkezete („A disk image stored on this drive”) magyarul nehéz előtaggá
    válna (`Egy ezen a meghajtón tárolt lemezkép…`). A második mondat `azt`-ja igei fókuszban áll (`Előbb azt add ki`),
    ami pont az angol „that image first” hangsúlyát adja.
- **„macOS is still working with this drive” → `A macOS még dolgozik ezen a meghajtón`** · a `dolgozik` a katalógus
  igéje (`fileExplorer.navigation.connectionTooltipDisconnected`: „A Cmdr dolgozik a helyreállításán.”), az
  `ezen a meghajtón` helyhatározó pedig a testvér `errors.eject.busy` alakja („A Cmdr még fájlokat mozgat ezen a
  meghajtón”) · `high`. A `macOS` névelője `a`, nem hedge: a kezdőhang ismert, és a katalógus végig `a macOS`-t ír.
- **„Cmdr itself” → `Maga a Cmdr`** · a `maga a X` a magyar nyomatékosító szerkezet, és a mondatváz marad a családé
  (alany elöl), hogy a hat kulcs egy hangon szóljon · `high`. A `Ezt a meghajtót még maga a Cmdr használja.` fókuszos
  szórend is jó lett volna, de a családi váz megtartása többet ér: az olvasó a hat mondatot ugyanabban a buborékban
  látja, egymás után soha.
  - **„send a report” → `küldj jelentést`** · a katalógus a puszta `jelentés`-t használja a beépített
    visszajelzésküldésre (`errorReporter.amend.unavailable`: „küldj új jelentést a Súgó menüből”) · `high`. Nem
    `hibajelentést`: a `hiba` szót a hang kerüli (`style.md` § Voice and tone), és az angol is csak „a report”-ot mond.
  - **„if it keeps happening” → `ha továbbra is előfordul`** · a szállított alak ugyanerre az angolra
    (`errors.listing.resourceBusy.suggestion`: „Ha továbbra is előfordul, …”) · `high`. A katalógusban él a hosszabb
    `Ha ez folyamatosan előfordul` is, de az a „If this keeps happening” párja; a rövidebb illik a buborékba.
  - A `Várj egy percet` (rendszer) és a `Várj egy pillanatot` (Cmdr) különbsége szándékos, az angolt követi („a minute”
    vs. „a moment”): a Spotlight-indexelés tovább tart, mint egy Cmdr-beli leíró elengedése.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Fajta`** · macOS Finder `hu`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Összes *.{extension} fájl kijelölése`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" →
  `Mind kijelölése azonos kiterjesztéssel`) and Total Commander (`WCMD.INC` `527` →
  `Minden fájl kijelölése ugyanilyen kiterjesztéssel`) name this exact command, and the mask replaces their "same
  extension" because Cmdr shows the concrete one · `high`. A maszk JELZŐKÉNT áll a `fájl` előtt, így semmilyen
  toldaléknak nem kell illeszkednie a `{extension}` hangalakjához. Ez a kulcs a példa arra, amit a `i18n-translation.md`
  § „Write placeholder strings to be restructurable” kér.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Kijelölés`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` („… kijelöléséhez”) · `high`. Its siblings
  in that menu are verbs; this one names what the submenu holds. A `menu.bar.select` magyarul eleve névszói
  (`Kijelölés`), így a kettő azonos — ez rendben van, a magyar menücímek amúgy is névszóiak.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

A címsorban ülő figyelmeztető jelvény és a hozzá tartozó tooltip, amíg a Cmdrnek nincs teljes hozzáférése a lemezhez;
kattintásra a bevezető 1. lépése nyílik meg.

- **`onboarding.fdaBadge.label` → `Nincs teljes hozzáférés a lemezhez`** · Apple's own Rendszerbeállítások row (see the
  Full Disk Access entry above for the live-bundle evidence) · high. Longer than the compound `teljes lemezhozzáférés`,
  but the badge's whole job is to be findable in the Rendszerbeállítások, and that is the row's actual wording.
- **`onboarding.stepAi.bannerTitle.denied` carries the SAME English string** ("No full disk access"), so `i18n-terms`
  holds the two identical. It was moved from `Nincs teljes lemezhozzáférés` onto the pane name in the same pass. ❌
  Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Nincs teljes hozzáférés a lemezhez. Megnyitja a bevezető teljes lemezhozzáférésről szóló lépését.`), which is what
  satisfies `i18n-aria` (WCAG 2.5.3). The second sentence needs a `-ről` suffix on the term, and a suffixed form would
  NOT contain the bare label, so the containment rides on the opening sentence. ❌ Re-wording the label alone breaks it.
- **Tooltip terms**: the benefit list is nominal (`az egész meghajtód átkutatása, a felhős mappák olvasása, …`), the
  Hungarian UI convention; drive → `meghajtó` (settled), cloud folders → `felhős mappák`, "files macOS keeps to itself"
  → `azok a fájlok, amiket a macOS magának tart` (plain, ❌ never a macOS feature name), "Click to …" →
  `Kattints ide, …` (as in `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `bevezető` (settled).
- **Name vs prose, the boundary**: `teljes hozzáférés a lemezhez` where a string NAMES the setting. The FDA step's
  running prose (`onboarding.stepFda.revoked.noAccess` and its siblings) still says `teljes lemezhozzáférés`, which is
  idiomatic Hungarian for the concept. Whether the prose moves onto the pane name too is open (`review-queue.md`).

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Hungarian solves it
  with `a kiválasztott elemek közül {count} darab…`, where the counted noun stays singular whatever the numeral is.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Technikai részletek`), because it points at that very control.

- **locked → `zárolva`** · macOS Finder (`AXNODE1` `Zárolva`) and the settled catalog term · high.
- **"delete them permanently" → `véglegesen törölheted`** · the verb form of `commands.fileDeletePermanently.label`
  (`Végleges törlés`), so the suggestion names the command the user will run · high.
- **❗ `Shift+F8` takes no case suffix.** The natural Hungarian would be `a Shift+F8-cal`, which welds a suffix onto a
  shortcut that has to stay verbatim, so both suggestions restructure to `A Shift+F8 billentyűparanccsal viszont …` and
  leave the token bare. Do the same for any future shortcut mention.
- **badge (the title-bar pill) → `jelvény`** · the settled term (MS `badge` → `jelvény`, plus the Finder status-badge
  parallel) · high. title bar → `címsor` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in Hungarian `„…”` quotes.
- "somewhere macOS keeps to itself" → `olyan helyen vannak, amit a macOS magának tart`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## A csak online tartalom figyelmeztetése (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Ha egy felhőmappában kijelölt elem csak online érhető el, a Kuka előbb letöltené. Ezért a Cmdr a végleges törlés ablakát
nyitja meg, és a felső sávban elmagyarázza. Két sávváltozat: az egyik vegyes kijelölésre, a másik arra, amikor minden
csak online érhető el. Csak az első mondatban és a felkínált kiutakban térnek el. A harmadik kulcs az a sor, amely akkor
jelenik meg, amikor a Cmdr visszaadja a gombnyomást.

- **`.cloudOnlineOnlyMixedWarning`** · a `csak online érhető el` a Finder fordulata a kiürített fájlra; a `Kuka` és a
  `felhőszolgáltatás` a `terms.json` szavai · tentative.
- **`.cloudOnlineOnlyAllWarning`** · ugyanaz a szöveg, „Minden, amit kijelöltél” kezdettel, és a kijelölés-törlő kiút
  nélkül (a vegyes sáv `töröld a … kijelölését` alakja): ha minden csak online van, nem maradna kijelölve semmi ·
  tentative.
- **`.cloudOnlineOnlyHandedBack`** · a gomb feletti sor, miután a Cmdr szándékosan nem hajtotta végre a gombnyomást.
  Tárgyilagos, mentegetőzés nélkül · tentative.
- **Mind a négy tény maradjon benne**: (1) a Kuka letöltené a fájlokat, (2) ezért a Cmdr csak a TELJES kijelölés
  törlését ajánlja fel, (3) a Kukában utána NEM marad másolat, a szolgáltatásnál viszont igen (❌ ne legyen belőle
  „úgyis a Kukába kerül”), (4) a sávban megnevezett kiutak.
- **A két `<strong>` szakasz marad**, a „le kellene tölteni” részen és a „törlését” szón. A „Törlés” idézőjelben a gomb
  felirata: mindig ugyanaz, mint a `fileOperations.delete.confirmDelete`.
- Az overflow-ellenőrzésnél nézd meg: a sáv hosszú, és keskeny csíkban ül a fájllista fölött.

## Amikor a szerver szerint nincs is ilyen megosztás (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

A család egyetlen olyan esete, ahol az újrapróbálkozás nem segít: a szerver egyértelmű választ ad, hogy nincs ilyen nevű
megosztása. Ezért a buborék mellett nincs gomb, és a szöveg nem sugallhat semmi átmenetit (se `most`, se
`próbáld újra`), szemben a testvéreivel.

- **„the server says it has no share by that name” → `A szerver szerint ugyanis nincs ilyen nevű megosztás.`** ·
  NetAuthAgent `EINFO_NO_SHARE` („A(z) „%@” megosztás nem létezik a szerveren.”, élő bundle, macOS 26.6.2, 25G83,
  2026-09-17) és a katalógus `errors.mount.shareNotFound` · high. A NetAuthAgent önöz, a mi mondatunk tegez; a SZÓ az
  Apple-é, a MONDAT a miénk.
- **KÜLÖN mondatba kerül, nem a `mert` kötőszóval** · a settled nyitány
  (`Nem sikerült közvetlenül csatlakozni ehhez: {share}`, lásd § Rendszerkapcsolatra visszaeső SMB-buborék) a kettőspont
  miatt a NÉVVEL ér véget, tehát mellékmondat nem kapcsolható utána. Az `ugyanis` viszi az angol `because` szerepét ·
  high.
- **„This one won't sort itself out” → `Ez magától nem fog megoldódni`** · nincs forrás a kupacban; ez a bevett magyar
  fordulat, és pontosan azt mondja, ami ezt a buborékot elválasztja a többitől: a várakozás nem segít · high.
- **„may have been renamed or removed” → `lehet, hogy a megosztást átnevezték vagy eltávolították`** · szó szerint az
  `errors.write.destinationNotFound.suggestion` · high. Általános alany (`átnevezték`), ahogy ott is: nem tudjuk, ki
  tette.
- **„so it's worth checking there” → `úgyhogy érdemes ott körülnézni`** · a katalógus baráti regisztere; az `ott` a
  szerverre mutat vissza, így nem kell megismételni a szót · high.
- **„a lot slower” → `sokkal lassabb`** · itt az angol nem ad szorzószámot, a testvérrel (`négyszer`, `százszor`)
  ellentétben, tehát a magyar sem ad · high.
- **A rövid buborékban a `{server} szerint` névutós alak áll** · a `szerint` külön szó, tehát a `{server}` ismeretlen
  értéke nem kap toldalékot (`style.md` § Agglutination) · high. A záró `ezért a rendszerkapcsolaton marad` a három
  testvér-buborék (`fileExplorer.pane.directConnectionUnreachableToast`…) végződése.

## Egyformának látszó nevek a szerveren (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.explanation`, `errors.listing.ambiguousName.suggestion`, `errors.volume.ambiguousName`)

A szerver két nevet eltérő karaktersorként tárol (összetett vagy felbontott ékezet, kis- és nagybetű), a képernyőn
viszont egyformák.

- **„spelled differently” / „spells them differently” → `eltérő írásmóddal tárolja őket`** · nincs forrás a kupacban
  (egyik referencia sem nevezi meg ezt a jelenséget) · tentative. A hibapanel és az ütközési párbeszédablak ugyanezt az
  alakot használja, hogy a két felület ugyanazt mondja. ❌ Ne `Unicode` vagy `normalizálás`: az angol leírás
  kifejezetten köznyelvet kér.
- **„look the same” → `ugyanúgy néz ki`** · köznyelvi fordulat, nincs forrás · high.
- **„matches” (egy útvonal több elemre) → `illik`** · a katalógus korábbi alakja; a Double Commander a mintára
  `illeszkedik`-et mond (`A Péld* illeszkedik a Példa.txt-re`), de egy útvonalnál az `illik` a természetesebb · high.
- **„upper and lower case” → `kis- és nagybetűk`** · Double Commander, KDE Dolphin, macOS AppKit
  (`A kis- és nagybetűk azonosak`) · high.
- **Gombnév futó szövegben: `A „Felülírás” gomb`** · a címke a `fileOperations.transferProgress.conflictOverwrite`
  alakja, idézőjelben, ahogy az Apple is idézi a címkéit (`style.md` § Notes, macOS panelnevei) · high. „the one that's
  there” → `a meglévő elemet`, mert a párbeszédablak sora is `Meglévő (fájl):`.
- **Az `errors.listing.ambiguousName.explanation` útvonala kettőspont mögé kerül**
  (`… illik erre az útvonalra: {path}`), mert a korábbi mondatkezdő `A {path} útvonalra` puszta `A` névelőt tett egy
  ismeretlen kezdőhangú érték elé (`style.md` § Notes). A rövid `errors.volume.ambiguousName` marad
  `a(z) „{path}” útvonalra`: ott a házi `a(z)` alak helyes, és a mondat folytatódik.
- **„To avoid this next time, rename one…” → `Legközelebb elkerülheted, ha az egyiket átnevezed úgy, hogy…`** · a
  korábbi `Hogy legközelebb …, hogy …` két `hogy`-ot tett egy mondatba · high.

## A „Felhő-AI engedélyezése” kapcsoló és a kikapcsolt felhő-AI állapotai (`ai.cloudConsent.label`, `ai.cloudConsent.description`, `askCmdr.gate.cloudOff.body`, `settings.ai.cloudConsent.lockedHint`, `settings.askCmdr.enabled.label`)

Adatvédelmi kapcsoló: amíg ki van kapcsolva, a Cmdr semmit sem küld felhő-AI-szolgáltatásnak. A hang nyugodt, és sosem
állít többet, mint amit a Cmdr tesz. Minden döntés az itt már rögzített kifejezésekre és a katalógusra épül.

- **Allow cloud AI (a kapcsoló neve) → `Felhő-AI engedélyezése`** · a `Felhő-AI` a `settings.ai.provider.opt.cloud`
  opció, az `engedélyez` a fent rögzített allow → `Engedélyezés` (macOS), a névszói címkeforma pedig a
  `settings.fileOperations.allowFileExtensionChanges.label` mintája · `high`. Idézve `„…”` jelek között
  (`settings.ai.cloudConsent.lockedHint`). Ahol az angol igeként használja (`askCmdr.gate.cloudOff.body`,
  `settings.askCmdr.cloudOffHint`), ott `engedélyezd a felhő-AI-t`: ugyanazok a szavak, ragozva.
- **cloud AI (mondatban) → `a felhő-AI`** (kisbetűvel, tárgyeset `felhő-AI-t`); összetételben `felhő-AI-szolgáltatás`,
  mint a `settings.ai.cloudProvider.description` · `high`.
- **„X is off” → `X ki van kapcsolva`** · mint a `servers.hub.discoveryOff` · `high`.
- **Turn on Ask Cmdr → `Ask Cmdr bekapcsolása`** · a már rögzített minta (§ Ask Cmdr) · `high`.
- **Open AI settings → `AI-beállítások megnyitása`** · mint a `commands.appSettings.label` („Beállítások megnyitása”) ·
  `high`.
- **side panel → `oldalpanel`** · `tentative` (nincs katalógusbeli előzmény).
- **custom endpoints → `egyéni végpontok`** · `Egyéni` a custom (`settings.network.timeoutMode.opt.custom`), `végpont`
  az `onboarding.cloudSetup.hint.azureEndpoint` alapján · `high`. Az Ollama és az LM Studio ragozatlanul áll
  (`Az Ollama, az LM Studio … esetében`), hogy a márkanév betűre megmaradjon.
- **Cmdr + -val/-vel → `Cmdrrel`** · kötőjel nélkül, mint a katalógus `Cmdrben`, `Cmdrhez` alakjai, a kiejtett
  „commander” hangrendje szerint · `high`.
- **Ask Cmdr chats → `Ask Cmdr-csevegések`** · többszavas tulajdonnév + köznév, kötőjellel · `high`.
- A `settings.askCmdr.enabled.label` most már csak „Ask Cmdr” (a terméknév a kapcsolón),
  `sameAsSourceJustification`-nel.

## Kilépés a teljes képernyőből az Escape billentyűvel (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

Nyolc kulcs: az egyszeri értesítés, amely azután jelenik meg, hogy az Escape kiléptette a főablakot a macOS teljes
képernyős módjából, és a hozzá tartozó kapcsoló a Speciális beállításokban.

- **full screen (a macOS ablakmódja) → `teljes képernyős mód`**, a kilépés `Kilépés a teljes képernyős módból` · mac
  (AppKit `MenuCommands` „Exit Full Screen” = „Kilépés a teljes képernyős módból”, `NSExitFullScreenTemplate`; Finder
  `FV20`), ms (`teljes képernyős mód`) · high. ❌ NEM puszta `teljes képernyő`: a macOS azt az „Entire Screen” és a menü
  „Full Screen” főnevére használja, az ablak ÁLLAPOTÁRA a `-s mód` alakot.
- **Escape (a billentyű) → `Escape billentyű`, alaptaggal** · mac (AppKit `Accessibility`: „az Escape billentyűvel pedig
  bezárhatja a választót”), a katalógus alaptagos billentyűmintája (`fileExplorer.quickLookHint.enterOpens`: „Az
  <enter></enter> billentyűvel”) · high. Az alaptag azért kell, mert az `Escape` végi `e` néma, a ragja kötőjeles lenne
  (`Escape-pel`); egy második említés a mondatban állhat puszta `az Escape` alakban. A
  `shortcuts.section.pressEscToClear` `ESC-et` alakja az angol „ESC” rövidítést követi, nem ütközik.
- **„Exit full screen on Escape” (kapcsoló, a buborékban és a beállításokban azonos) →
  `Kilépés a teljes képernyős módból az Escape billentyűvel`** · a macOS menüparancsa szó szerint + az alaptagos
  billentyű · high. Hosszú (57 karakter az angol 26-tal szemben); ha kilógna a buborékból, ez a rövidítendő.
- **„Settings > Advanced” (link) → `Beállítások > Speciális`**, a `fileExplorer.quickLookHint.configurable` `itt:`
  keretében, hogy a link ne kapjon ragot · high.
- **„You'll only see this once.” → `Ezt csak egyszer látod.`** · tegező, rövid · high.
- **`…HintShown` belső kulcsok** a `settings.advanced.oldMacosNoticeShown.*` mintáját követik (`… megjelent`,
  `Belső: követi, hogy …`, `Rejtve a felülettől.`) · high.

## Az áthelyezés közben módosult eredetik (`transfer.changedDuringMove`, 2026-09-25)

- **„changed during the move” → `módosult az áthelyezés közben`** · Finder `PE56` „egy vagy több elem az írás közben
  módosult” (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25), és a fenti
  `it changed → módosult` sor · `high`. A számnévi alany mellett egyes számú állítmány (style.md § Plurals). Az
  `áthelyezés közben` és a `forrásmappákban` betű szerint a testvér `transfer.appearedDuringMove` alakja, mert a két
  mondat ugyanabban az értesítésben áll.

## A „Megnyitás ezzel” és a „Megosztás” almenü helyőrző sorai (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## A „Megnyitás ezzel” és a „Megosztás” almenü helyőrző sorai (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- **„Finding apps…” → `Appok keresése…`** · névszói alak, mint a macOS („Searching…” = „Keresés…”); `app`, mint a
  `settings.behavior.textEditorApp.checking` (`Appok ellenőrzése…`) · high.
- **„share options” → `megosztási lehetőségek`** · `lehetőség`, nem `beállítás`: az a settings jelentés, itt a megosztás
  MÓDJAIRÓL van szó (AirDrop, Mail) · tentative.
- **„No share options” → `Nincs megosztási lehetőség`** · a macOS üres menüjének mintája („No Services Apply” = „Nincs
  alkalmazható szolgáltatás”) · high.
