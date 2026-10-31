# cs review queue

Open questions only a native reviewer can settle: a `tentative` ruling the sources couldn't decide, a phrasing that
reads correctly but maybe not naturally, a register call. One bullet per item, citing the keys in backticks. Not
translator input: translators work from the brief and `terms.json`. Remove an item once a reviewer settles it (and
record the outcome in `terms.json` or `decisions.md`).

- Tab: `záložka` (TC, DC) vs `karta` (MS, Nautilus, Dolphin); macOS's own `panel` is taken by the pane. Does `záložka`
  read naturally for folder tabs in `menu.tab.*` and `commands.tab*`?
- Full Disk Access: the pile has only the search term `plný přístup k disku`; check the live macOS 27 Privacy & Security
  row label before quoting it in `onboarding.stepFda.*`.
- Roll back: `Vrátit změny` (MS) for `operationLog.dialog.rollBack` and `fileOperations.rollbackConfirm.*`; no macOS or
  TC term. Natural, or would `Vrátit zpět` read better despite matching Finder's Put Back?
- Operation log: `Protokol operací` for `menu.view.operationLog`; unsourced phrase, `Historie operací` is the
  alternative.
- Side panel: `postranní panel` for Ask Cmdr's panel (`menu.view.askCmdr` area), chosen only to differ from the macOS
  sidebar `boční panel`; confirm it doesn't read as a synonym.
- Clone pane: `Klonovat panel` (DC Klonovat) for `menu.view.clonePane`; TC's equivalent is `Cíl = zdroj`.
- License key: `licenční klíč` for `menu.app.licenseEnter`; TC says `registrační klíč`.

- `settings.appearance.appColor.opt.cmdrGold`: "Zlatá barva Cmdru" inflects the brand (Cmdru); check it reads naturally
  or prefer "Zlatá Cmdr"
- `settings.appearance.uiDensity.opt.*`: Kompaktní / Pohodlná / Vzdušná agree with hustota (f.); check Pohodlná and
  Vzdušná sound right as density levels
- `settings.appearance.dateColors.opt.wilting`: Vadnutí for the wilting metaphor; check it is understood
- `settings.fileOperations.adbEnabled.description`: "ladění přes USB" must match the Czech Android Developer options
  label exactly; "nástroje platformy Android" for SDK Platform Tools is descriptive
- `settings.fileOperations.mtpEnabled.description`: Android labels Předvolby USB / Přenos souborů quoted from memory of
  AOSP cs; verify on a Czech phone
- `settings.appearance.tintLocal.label`: tónování (macOS Barevné tónování) vs barevný nádech (MS) for the pane tint;
  settings.tint.none should become Bez tónování to match
- `settings.listing.stripedRows.label`: Pruhované řádky has no pile source

- `menu.context.makeAvailableOffline`: Zpřístupnit offline is literal; Finder's own items are Stáhnout nyní / Zachovat
  stažené. Pick whichever a Czech Mac user recognizes for downloading an iCloud-only file.
- `menu.volume.ejectBusy`: (probíhá operace) as the busy marker; check it reads naturally after Vysunout ({name}).
- `menu.app.changelog`: Seznam změn vs. Historie verzí; must read different from Co je nového (`menu.help.whatsNew`).
- `menu.app.acknowledgements`: Poděkování has no Apple source in the pile; confirm.
- `menu.sort.ascending`: Vzestupně / Sestupně under Seřadit podle (genitive children) mixes a sentence-completion list
  with adverbs; check it reads fine.
- `menu.network.pinToSwitcher`: přepínač for the volume switcher is a coinage (no ruling for switcher); confirm it
  matches whatever the volume dropdown is called elsewhere.

- `commands.appAcknowledgements.label`: Poděkování… has no pile source (no Czech Acknowledgements/Credits hit); check it
  reads as open-source credits, alternative Licence a poděkování…
- `commands.paneLeftVolumeChooser.label`: přepínač svazků is coined (TC/DC say seznam disků); confirm it reads naturally
- `commands.cmdrOpenOnboarding.label`: Úvodní průvodce… aligned with menu.app.onboarding; macOS uses úvodní nastavení
  for onboarding
- `commands.helpWhatsNew.label`: Co je nového aligned with menu.help.whatsNew; Czech macOS System Settings says Novinky
- `commands.askCmdrToggle.description`: Chat s AI o vašich souborech… is a verbless noun phrase; check it fits beside
  the other 3rd-person descriptions
- `commands.appSettings.label`: Otevřít Nastavení capitalizes the window name (settings ruling note);
  commands.handler.openTerminalHere.openSettings should match
- `commands.navFirstInFull.label`: Skočit na první soubor vs commands.navHome.label Přejít na první soubor; check the
  distinction reads as intended in the shortcuts list

- `errors.eject.unmountRefusedByProcesses`: raw family, no ICU, so the count can't agree with a noun; rendered „Procesy
  s názvy {processes} (počet: {countText}) tam mají…“. Check it reads naturally.
- `errors.eject.notEjectable`: removable → „není vyměnitelný“ (MS wording); a native reviewer may prefer „nedá se
  vysunout“.
- `errors.listing.staleConnection.explanation`: old reference → „starý identifikátor“ (avoids odkaz, which is the
  symlink word); check it reads well.
- `errors.eject.deviceDisconnectRefused`: idle → „až bude nečinné“; check the register.
- `errors.listing.connectionReset.title`: „Připojení bylo resetováno“; macOS says „Reset připojení u partnera“. Check it
  reads naturally.

- `commands.tagsToggle*.label`: Přepnout šedou značku (plain switch) beside TC's „X nebo Y“ for named states
  (`commands.viewShowHidden.label`, `commands.tabTogglePin.label`); would Přidat nebo odstranit šedou značku read
  better?
- `commands.handler.shareLinkUnavailable`: S3 bucket kept as bucket and declined (v bucketu S3); confirm it reads
  naturally to Czech S3 users
- `commands.aboutOpenUpgrade.label`: Otevřít stránku upgradu; check it reads as the purchase page (alternative Otevřít
  stránku s nákupem licence)
- `commands.handler.openTerminalHere.hint`: the section name Navigace a operace se soubory was written before
  settings.section.navigationAndFileOps is translated; align once it is
- `commands.fileCopyShareLink.label`: (na sedm dní) / (na jeden den) / (na jednu hodinu) adds na to English (seven
  days); check it fits the palette row
- `commands.cloudAskGemini.description`: s vybraným souborem jako tématem is a bit stiff; a native may find a smoother
  frame
- `commands.dialogConfirm.label`: Potvrdit otevřené dialogové okno reads open as the adjective (the dialog currently
  shown)
- `commands.handler.viewDebugLog.disabled`: limit místa na disku větší než 0 paraphrases the setting Maximum disk space
  for log files (MB); align with settings.advanced.maxLogStorageMb.label once translated

- `fileExplorer.typeToJump.prefix`, `fileExplorer.quickFilter.intro.switchToJump`: mode names Skok / Filtr must match
  `settings.fileExplorer.typeToJump.mode.opt.jump` / `.opt.filter` (batch 28, untranslated when this batch ran); check
  Skok reads naturally as a mode name
- `fileExplorer.quickFilter.intro.gotIt`: Rozumím has no pile source; OK is the macOS alternative
- `fileExplorer.pane.fileExplorerAriaLabel`: Správce souborů for the whole two-pane region; Průzkumník would read as
  Windows Explorer
- `fileExplorer.archivedFile.tooltip`, `fileExplorer.archivedFile.label`: archivní úložiště for cold storage (MS has the
  longer úložiště málo používaných dat); check it reads as a cloud tier, not a zip
- `fileExplorer.pane.openLocalNetworkSettings`: Otevřít nastavení {localNetwork} leaves the injected name undeclined (it
  may arrive in English); check it reads acceptably
- `fileExplorer.network.browser.status.error`: Problém instead of a bare Chyba for the generic host status chip
- `fileExplorer.errorPane.retryInfo`, `fileExplorer.errorPane.retryInfoWithLast`: Retry #{count} rendered as Zkoušeno
  znovu {count}× (count of retries), poprvé / naposledy {ago}

- `settings.mediaIndex.importanceThreshold.preview`: v/ve before a digit count is chosen per plural branch (ve for 2–4);
  numbers like 22 or 300 fall in the other branch and get v, while speech says ve dvaceti, ve třech stech. Check whether
  that is acceptable
- `settings.mediaIndex.progress.kept`: the many (decimal) branch reads oddly (Dalších 1,5 obrázku zaindexovaného…); it
  can't occur for a count of images, but check the wording
- `settings.mediaIndex.parallelism.label`: workers rendered as Souběžné úlohy; check that it reads well next to the
  slider
- `settings.mediaIndex.importanceThreshold.floor`: Junk rendered as Smetí (casual); check the tone

- `errors.git.blobTooLarge.suggestion`: „Check out the file from a working tree“ rendered „otevřete ho z pracovního
  stromu“ (git checkout not named); check whether a git user expects „checkout“.
- `errors.git.missingObject.message`: pack files → „balíčkové soubory“; check it reads natural to Czech git users
  (alternative: „soubory pack“).
- `errors.git.notARepo.suggestion`: repo chip → „štítek repozitáře“; confirm against the chip’s visible label once
  fileExplorer.git is translated.
- `errors.listing.deviceProblem.explanation`: loose connection → „uvolněné připojení“; „uvolněný kontakt/konektor“ may
  be more idiomatic but fails the connect stem check.
- `errors.listing.deviceReconnecting.title`: „Opětovné připojování k zařízení“; check the register for a panel title.
- `errors.listing.objectStoreRefused.explanation`: billing issue → „problém s platbou“, usage cap → „limit využití“;
  check wording.

- `fileOperations.delete.foldersPart`: accusative „1 složku“ chosen because the phrase only follows a verb in titles;
  check no nominative use exists
- `fileOperations.trash.undoAction`: „Odvolat akci“ (macOS Edit menu form) on the trash toast; check it reads naturally
  vs. „Vrátit zpět“
- `fileOperations.trash.undoing`: „Vrací se zpět…“ progress line, check naturalness
- `fileOperations.s3Cost.infoText`: „Bezplatné tarify, slevy, objemy zahrnuté v ceně a minimální poplatky“ for free
  tiers / included allowances; check billing wording
- `fileOperations.transferDialog.policyStop`: „Ptát se u každého“ for Ask for each

- `fileExplorer.git.size.stashEntries`: „záznam ve stashi“ keeps git's word stash; a native git user should confirm vs.
  „odložená změna“ (Pro Git cs: odložit).
- `fileExplorer.git.size.linkedWorktrees`: plural of worktree left undeclined („2 propojené worktree“); check it reads
  right.
- `fileExplorer.git.tooltip.worktreeOnBranch`: „Je přepnuto na větev „{branch}““ renders git's checked out; check
  against devs' „checkoutnutá“.
- `fileExplorer.git.tooltip.worktreeDetached`: „Odpojený HEAD na commitu {id}“ for detached HEAD.
- `fileExplorer.git.tooltip.pinnedCommit`: „Zafixováno na commitu {id}“ for a submodule's pinned commit.
- `fileExplorer.dirSize.noPerms`: „<bez práv>“ in the narrow Size column; check width and tone.
- `fileExplorer.rename.needsOwnMoveDialog`: starts lowercase because it lands after a colon in
  fileExplorer.rename.chainKeptOriginalName.

- `settings.ai.cloudConsent.lockedHint`: quotes „Povolit cloudovou AI“; `ai.cloudConsent.label` is still English, make
  the label match when translated
- `settings.fileExplorer.suppressQuickLookHint.description`: quotes „Už nezobrazovat“;
  `fileExplorer.quickLookHint.dontShowAgain` is still English, translate it identically (macOS uses Nezobrazovat, TC již
  příště nezobrazovat)
- `settings.network.permissionIntroPrefix`: names the Cmdr section as Síť; check against
  `fileExplorer.navigation.groupNetwork` once translated. „Povol to“ in `settings.network.permissionIntroSuffix` assumes
  the macOS dialog button is Povolit
- `settings.network.localNetworkAccessLabel`: colon heading „Přístup: {localNetwork}“ because the OS-localized token
  cannot be declined; check it reads well as a heading
- `settings.behavior.textEditorApp.label`: „Upravovat soubory v aplikaci“ + dropdown „Podle systému (TextEdit)“; check
  the joined reading
- `settings.fileExplorer.git.showRepoChip.description`: „necommitované změny“ for dirty state is developer jargon; a
  native reviewer may prefer another git phrasing
- `settings.fileExplorer.typeToJump.mode.description`: Delete (⌫) kept as the key name; confirm what Czech VoiceOver
  calls the ⌫ key

- `errors.write.trashRefused.suggestion.noFullDiskAccess`: quotes the badge as „Bez plného přístupu k disku“;
  `onboarding.fdaBadge.label` is still untranslated and must ship exactly that wording (or this quote must follow it).
- `errors.write.destinationNotAFolder.title`: „V cestě je soubor“ for "A file is in the way"; check it reads as a
  blocking file, not a path.
- `errors.write.archiveEntryNameRefused.message.parentTraversal`: long explanation split into three sentences; check
  clarity.
- `errors.write.sourceNotFound.message.copy`: „Soubor nebo složka, kterou…“ agrees the relative pronoun with the nearer
  noun; check it reads naturally (same in the move/delete/trash variants).
- `errors.write.noLongerConnected.title`: „Už není připojeno“ paired with „Zatím nepřipojeno“
  (`errors.write.notConnected.title`); check the pair.

- `fileOperations.transferProgress.stallNotice`: „Průběh stojí už {duration}“ for No progress for X; check naturalness
  in the narrow ETA slot
- `fileOperations.transferProgress.queuedToast`: colon frame „Před touto operací je ve frontě ještě: {countText}.“
  dodges verb agreement with the preformatted count phrase; check it reads OK
- `fileOperations.transferProgress.background`: button „Na pozadí“ (no verb) and `queue` „Do fronty“; check both read as
  actions on a button
- `fileOperations.transferProgress.titleReversalDeleting`: „Mazání souborů, které operace vytvořila“; many-branch
  wording is unreachable but check
- `fileOperations.transferProgress.titleRemovingOriginals`: „Odstraňování původních souborů…“ uses the remove verb
  (odstranit) to stay off smazat as the description asks, though the remove ruling is about lists; check
- `fileOperations.cancelRollback.recoveredOriginal`: „Cmdr uchoval původní položku z cesty {path} jako {keptAt}.“ check
  the „jako“ path frame
- `fileOperations.rollbackConfirm.leaveAsIs`: „Nechat, jak to je“ as a button
- `fileOperations.archivePassword.retryTitle`: „Tohle nevyšlo“ for That didn't work

- `fileExplorer.renameConflict.yours`, `fileExplorer.renameConflict.existing`: (vaÅ¡e verze) / (existující) chosen to
  avoid gender agreement with an unknown file or folder; check the pair reads naturally as card headers
- `fileExplorer.tabBar.unreachableAriaLabel`: Nedostupný aligned with `fileExplorer.network.browser.status.unreachable`
  for term consistency, although a tab (záložka) would take Nedostupná; check it reads acceptably on a tab
- `fileExplorer.quickLookHint.configurable`, `fileExplorer.navigation.driveIndex.refusedIndexingOff`,
  `fileExplorer.navigation.driveIndex.menuIndexingOffNote`: Settings paths written as Nastavení > Klávesové zkratky and
  Indexování > Indexování disků before `settings.section.keyboardShortcuts` / `settings.section.indexing` /
  `settings.section.driveIndexing` were translated; re-check once they are
- `fileExplorer.navigation.driveIndex.tooltipCoalesced*`: macOS as a masculine subject (macOS ztratil přehled), with the
  time window after the count; check word order
- `fileExplorer.navigation.disconnectPlaceAriaLabel`: Odpojit server {name} uses a head noun;
  `adb.disconnectDeviceAriaLabel` (same English) will need Odpojit zařízení {name}, an honest split for term-consistency

- `servers.sheet.s3GcsKeyHelp`: confirm the Czech Google Cloud console's label for the Interoperability tab (written ze
  záložky Interoperability)
- `servers.sheet.s3PathStyle`: Používat adresování ve stylu cesty (path-style) is long; check it fits the checkbox row
- `servers.sheet.passphrase`: Heslo ke klíči vs Heslová fráze klíče
- `servers.sheet.signInTitle`: colon title Přihlášení: {name}; check it reads well as a sheet title

- `errors.write.filesTooLargeForFilesystem.message.many`: raw string with a bare {count}, so the count sits behind a
  colon („Souborů příliš velkých pro tento disk: {count}.“); check it reads naturally.
- `errors.write.originalsKeptAside.message.many`: same colon workaround („Počet souborů: {count}.“).
- `errors.write.readOnlyDevice.destination.message`: {deviceName} may be a real name or the fallback „Cílové zařízení“,
  so the second sentence names „tohoto zařízení … na něj“ instead of pointing a pronoun at the insert; check it doesn't
  read as repetitive with the fallback.
- `errors.volume.deviceSessionReset`: „Zařízení restartovalo připojení“ for an MTP session reset; a native reviewer may
  prefer „obnovilo připojení“ (but obnovit is ruled for restore).
- `errors.write.newDataKeptAt.message`: „Nový soubor {path} je celý zapsaný“ keeps the full path behind the head noun;
  check it reads clearly with a long path.

- `settings.tint.teal`: Modrozelená vs macOS crayon Čírka; `settings.tint.lime` Limetková vs Limeta; confirm hue names
  read naturally as swatch labels
- `settings.fileViewer.suppressBinaryWarning.label`: surové zobrazení for raw view; the viewer catalog
  (viewer.binaryWarning.body) must use the same wording
- `settings.askCmdr.proactive.title`: Z vlastní iniciativy for On its own; check it reads well as a heading
- `settings.askCmdr.spend.title`: Využití a náklady for Spending (usage concept matches spending); check against the
  per-day spend list it heads
- `settings.analytics.email.label`: Kontaktní e‑mail pro betu; check bety/betu inflection reads naturally

- `ai.cloudConsent.askCmdr.item.sizes`: datumy (colloquial plural) chosen over data to avoid clashing with data = data;
  confirm
- `ai.local.installStepExtracting`: modul runtime (MS) vs běhové prostředí
- `ai.secretError.keyringBody`: Hesla a klíče is GNOME Seahorse's Czech name, unverified in the pile; KDE names its app
  differently
- `ai.local.ramLegendProjected`: Odhad navíc as the RAM legend for extra projected memory
- `ai.translateError.rateLimited.body`: tarif for the provider plan (MS plán)
- `ai.cloudConsent.askCmdr.proactive`: quotes „Z vlastní iniciativy“, which `settings.askCmdr.proactive.title` (still
  English in cs) must match byte for byte

- `askCmdr.thinking`: Přemýšlí… (3rd sg, like a messenger's píše…); check it reads naturally next to the impersonal tool
  lines
- `askCmdr.composer.dropHint`: Pusť sem pro přiložení; check it reads naturally as a drop overlay
- `askCmdr.sessions.unarchive`: Obnovit z archivu has no pile source; check against Czech mail/chat apps
- `askCmdr.tool.memoryEdit.doing`: Aktualizují se vlastní poznámky (its = vlastní); check it doesn't read as the user's
  own notes
- `askCmdr.tool.*.done`: participle-first lines (Vypsána složka, Prohlédnut návrh); check they don't read as stilted

- `servers.refusal.hostKeyRevoked`: kompromitovaný chosen over MS ohrožený; confirm it reads right for a key revoked in
  known_hosts.
- `servers.hostKey.changedDisclosure`: English is first person (I’ve checked it); rendered gender-neutral Mám to
  zkontrolované instead of gendered Zkontroloval(a) jsem to. Check it reads naturally as a disclosure label.
- `servers.paneState.cancelCycleTooltip`: Switch back to retry rendered as Když se sem přepnete zpět, zkusí se to znovu
  (reading: returning to this pane/place retries). Confirm the intended meaning.
- `servers.paneState.retryTotalSeconds`: one/few branches use the accusative (1 sekundu / 1 minutu) because the only
  host sentence is servers.paneState.retryKeepsTrying (zkoušet celkem {duration}); reuse elsewhere in nominative would
  read wrong.
- `servers.paneState.notConnected`: rendered as colon frame Nepřipojeno: {name} because {name} may be a server or share
  of unknown gender.

- `settings.servers.trustedHostKeys.approvedPrefix`: Důvěra udělena + a DateLabel date (absolute or relative like dnes);
  check it reads well with every DateLabel format
- `settings.control.unitItems`: položek (gen. pl.) after any typed number; reads wrong after 1–4 (1 položka, 2 položky),
  which the catalog can't vary
- `settings.section.listing`: Seznam souborů for Listing (the file-list display subsection); confirm it isn't confused
  with Brief mode Seznam
- `settings.revealHandler.notProductionBuild`: sestavení for build (no concept yet); check a native reader finds
  Vývojová a testovací sestavení natural

- `queryUi.mode.ai.label`: Zeptej se na cokoli is an imperative on a chip (English is imperative too); check it reads as
  a mode name and fits the tight chip
- `queryUi.results.live.foldersScanned`: {countText} složek prohledáno, with the many (decimal) branch složky
  prohledáno; check word order reads naturally in the status bar
- `queryUi.recent.popoverHint`: pohyb · vložit do pole · pravým tlačítkem odstranit; check the mixed noun/infinitive
  hint reads naturally
- `queryUi.results.widenToVolume`: Místo toho prohledat tento svazek assumes `queryUi.scope.thisVolume` is translated
  Tento svazek (batch 23); realign if it differs

- `askCmdr.decision.rejected`, `askCmdr.decision.approved`: rendered impersonally (Odmítnuto:/Schváleno: …) to avoid a
  gendered past; check the lines still read as the user's own answer.
- `askCmdr.renameReview.evidence.userEdited`: You typed this name → Váš vlastní název (a noun phrase, no participle);
  check it still reads as a reason label.
- `askCmdr.renameReview.coverage`: Shoduje se / Shodují se {matchedText} z {totalText} znaků; check the verb reads
  naturally as a caption.
- `askCmdr.renameReview.overwriteBadge`: (overwrite!) → (přepsání!); a noun badge, check it is not read as a command.
- `askCmdr.renameUndo.undoJobLabel`: many-branch locative v # dávky for decimals is unnatural but never reached (batch
  counts are integers).

- `onboarding.wizard.stepProgress`, `onboarding.wizard.stepTooltip`: „Krok {step} z {total}“ follows fileOperations (z
  {total, number}); with today’s totals 4 and 3+1 careful Czech says „ze 4“, „ze 3+1“. Decide whether to switch to ze or
  keep z.
- `onboarding.stepAi.table.rowSearch`, `onboarding.stepAi.table.rowSelect`: row labels rendered as nouns (Hledání,
  Hromadné přejmenování, Výběr) although the description calls Select a short verb; term-consistency lists Výběr vs
  Vybrat (menu.bar.select).
- `onboarding.stepAi.bannerBody.denied`, `onboarding.stepAi.bannerBody.stuck`, `onboarding.stepFda.revoked.intro`: the
  user’s past choices are restructured impersonally (Plný přístup k disku tedy zůstane vypnutý; Zvolená byla možnost…;
  byl dřív udělen, ale potom odvolán) to avoid gendered past tense; check they read naturally.
- `onboarding.stepAi.table.searchWithout`: „po 1. tohoto měsíce“ is a free rendering; Cmdr has no such literal
  date-filter label to quote.

- `queryUi.date.preset.thisMonth` / `queryUi.date.preset.lastMonth`: květen, 1. den 0:00 / duben 2026, 1. den 0:00 dodge
  the nominative-only {month}; check it reads acceptably next to `queryUi.date.preset.yearStart` 1. ledna 2026 0:00
- `queryUi.date.preset.thisWeek` / `queryUi.date.preset.lastWeek`: pondělí tohoto týdne 0:00 avoids gender agreement
  with {weekday} (toto pondělí / tuto neděli); check length in the cell
- `queryUi.age.weeks` / `queryUi.age.months` / `queryUi.age.years`: před {count} týd. / měs. / r.; check the
  abbreviations read well in the chip tooltip
- `queryUi.scope.toggle.hideBoring`: Skrýt nudné složky keeps the playful 'boring'; check it doesn't read as odd

- `licensing.about.fallbackOrg`: written in the accusative (tvoji organizaci) because every frame using it is „pro
  {org}“; if a future frame uses {org} in another case it will break
- `licensing.dialog.activatedToastNamed`: „Vítej na palubě, {org}!“ addresses an organization name in the vocative slot
  with the nominative; check it reads naturally (alternative: „Vítejte na palubě!“ is vykání, rejected)
- `licensing.commercialReminder.priceInfo`: $59 kept literal per the key note; Czech convention would be 59 $ (with
  no-break space); confirm the literal form is acceptable
- `licensing.acknowledgements.rustHeading`: Rust crates left in English; check a native reader finds it natural in a
  heading
- `licensing.about.copyright`: year range keeps the English hyphen (2024-2026); Czech typography prefers an en dash, but
  the note says keep it literal

- `viewer.statusBar.badge.streaming*`: „streamování“ is a loanword; check whether „postupné čtení“ reads better to a
  Czech user
- `viewer.toolbar.tail.*`, `viewer.statusBar.hint.tail`: tail toggle rendered „Sledovat“ / „sledování“; confirm it is
  not confused with the file-watcher setting „Sledování změn souborů“
- `viewer.pull.title`: „Získává se soubor {fileName} pro náhled“; check it reads naturally above a progress bar and as
  its screen-reader name
- `viewer.statusBar.hint.image`: rendered with colons („Kliknutí: 100 % / přizpůsobit · Posouvání: zvětšení · Tažení:
  posun“); check the gesture words
- `viewer.saveAs.defaultName`: „výběr“ contains a diacritic in a default file name; safe on macOS, but confirm vs
  „vyber“

- `onboarding.stepBeta.terms.consent`: legal consent sentence restructured to avoid gender (Mám přečtené smluvní
  podmínky a souhlasím s nimi); confirm it reads as a valid acceptance and that smluvní podmínky is the right name for
  the terms page
- `onboarding.stepBeta.checklist.alternativeTo`: Like rendered as casual lajk (MS terminology lists lajk); the site's
  button still reads Like in English
- `onboarding.stepOptional.title`: You’re almost ready rendered as Ještě kousek a můžete začít, to avoid a predicate
  adjective (připraveni)
- `onboarding.cloudSetup.title`: Set up {provider} rendered as Nastavení poskytovatele {provider}; check it reads well
  for local apps (Ollama, LM Studio)

- `operationLog.initiator.user`: bare provenance label „Vy“ for English You; check it reads as a label next to „Agent“ /
  „Klient AI“ and not as an epistolary capital Vy.
- `operationLog.dialog.rollbackOf`, `operationLog.dialog.latestRollback`: „… z {time}“ with a formatted date-time (z
  2026-10-06 14:32); check it reads as dated, not as a source.
- `search.walkHandoff.finished`, `search.walkHandoff.stoppedShort`, `search.walkHandoff.superseded`: uncontrolled
  {label} placed as „{label}: nalezeno # shod.“ / „výsledky hledání {label}“ to avoid inflecting it; check naturalness
  with a long AI title.
- `search.coverage.uncovered.unavailable`: when {drive} falls back to search.coverage.unnamedDrive the server line reads
  „neumí prohledat tento disk“ although it is a server.
- `suggestedOps.changedUnderReview`: English past „while you were reading it“ rendered present „zatímco ho čtete“, which
  reads more naturally than the past „zatímco jste ho četli“.

- `shortcuts.system.characterViewer`: Emotikony a symboly is the Edit-menu name; check whether Czech macOS titles the
  Character Viewer window Prohlížeč znaků and prefer that if so
- `shortcuts.system.spaces`: rendered as přepínání ploch (Spaces = plochy in macOS cs); check it reads right inside
  „(…)“ of shortcuts.conflict.systemShortcut
- `indexing.eta.secondsLeft`, `indexing.eta.minutesLeft`, `indexing.eta.hoursLeft`: lowercase zbývá… also shows
  standalone at a line start (IndexingEnrichRow, IndexingDriveRow); check it doesn't look broken there
- `indexing.enrich.progress`: rendered as a colon frame Obrázky: {doneText} z {totalText} because z + genitive agrees
  with the total, which the plural doesn't receive
- `common.attachEmail`, `common.attachEmailPrompt`: the user addresses the Cmdr team in vykání plural (abyste se mi
  mohli ozvat); confirm this is preferred over a neutral phrasing

- `errorReporter.dialog.notePlaceholder`: English past progressive (What were you trying to do?) rendered in present
  tense (Co se snažíte udělat?), mirroring the live situation; check it reads naturally after an error
- `main.revealActivation.title`: Příkaz „Zobrazit ve Finderu“ se otevřel tady; a command does not literally open, check
  for a more idiomatic title
- `main.oldMacos.body`: first-person „I’d still like to hear about it“ rendered as „určitě mi o tom dejte vědět“ to
  avoid the maker’s gender (rád uslyším); best effort as „podpora bez záruk“ to match
  settings.advanced.oldMacosNoticeShown.description
- `main.dockPinNudge.accept`: „my Dock“ dropped (Ano, přidat do Docku); Czech possessive mého Docku sounds stilted on a
  button
- `errorReporter.dialog.description`: redaction rendered as anonymizovat (tentative ruling) and client-side as „už v
  aplikaci“
- `main.oldWebkit.body`: Software Update named as „nastavení Aktualizace softwaru“ (macOS cs: Otevřít nastavení
  aktualizace softwaru); confirm the pane name capitalization

- `adb.connect.cancelled`: „Otevírání telefonu bylo zastaveno.“ drops the English „You stopped…“ to avoid a gendered
  past participle; check it reads matter-of-fact, not as a fault
- `adb.readiness.offline`: „Probuď jeho obrazovku“ for wake its screen; a native may prefer „rozsviť displej“
- `transfer.fileOnly.allDone`, `transfer.fileOnly.allSkippedMany`, `transfer.fileOnly.mixedMove`: verb select and count
  plural were folded together so the participle agrees (zkopírován/zkopírovány/zkopírováno); check the longer toasts fit
- `goToPath.dialog.opensServer`: „Otevře se {name}“ uses the reflexive so {name} stays nominative

- `queue.row.statusAwaitingAnswer`: Potřebuje odpověď (a row needs the user's answer); check it reads naturally as a
  status chip next to Čeká/Běží.
- `queue.toolbar.selectedCount`: # vybraná / vybrané / vybraných agrees with the implied noun operace; check it reads
  right standing alone next to Zrušit vybrané.
- `queue.chip.ariaLabel`: uses {percentText} % instead of spelling the word, since procento/procenta/procent cannot
  agree with a pre-formatted string; check VoiceOver reads 42 % as „42 procent“.
- `ui.toast.age`: před {durationText} with durationText like 2m/1h; check the unit abbreviations Cmdr formats read
  acceptably in Czech.
- `ui.numberInput.decrease`: Snížit hodnotu {label} / Zvýšit hodnotu {label} puts the field name in the nominative after
  hodnotu; check it sounds fine with VoiceOver.
- `downloads.toast.learnIntro`: Šikovný tip, jak rychle přejít na stažené soubory, a freer rendering of the upbeat
  „Something cool to learn…“.
- `downloads.shortcutRow.registered`: Zaregistrováno / Nezaregistrováno for whether macOS claimed the global hotkey; a
  reviewer might prefer Aktivní/Neaktivní.
- Prompt split: Cmdr's own question is dotaz (`settings.indexing.reEnableNotifications.*`,
  `queue.row.awaitingAnswerTooltip`), a macOS or phone prompt is výzva
  (`fileExplorer.navigation.useSavedPasswordMessage`, `errors.listing.notConnected.suggestion`); check systémová výzva
  reads naturally for the Keychain dialog.
- `crashReporter.sentToast.changeSettings`: Změnit v Nastavení > Aktualizace a soukromí now names the full section;
  check it isn't too long for a toast button.
- `settings.advanced.resetAllConfirmTitle`: Obnovit výchozí hodnoty pokročilých nastavení (aligned with the Obnovit
  buttons) over Resetovat; confirm the long title reads well.

- `askCmdr.limit.continueMessage`, `onboarding.stepAi.table.renameWith`, `onboarding.stepAi.table.selectWith`: the
  user's own words to the AI agent („Pokračuj prosím…“, „přidej na začátek…“, „vyber všechny soubory…“) stay in the 2nd
  person singular, the way Czech users write to a chatbot; the app's own address around them is vy-plural. Confirm this
  is acceptable under "never tykání", or switch them to „Pokračujte prosím…“ / „přidejte…“ / „vyberte…“.
- `onboarding.stepBeta.greeting`, `licensing.dialog.mailtoBody`: Hi → Dobrý den (formal register); check it doesn't read
  too stiff for David's personal welcome, where dropping the greeting („Jsem David!“) is the alternative.
