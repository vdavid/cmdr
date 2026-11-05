# cs decisions

Distilled rulings ("X over Y because Z", at most ~3 lines), edited in place, never appended as a story.
`pnpm i18n:brief` pulls the sections whose heading cites a batch key, so every heading cites its keys in backticks. Term
rulings live in `terms.json`, open questions in `review-queue.md`, voice and typography in `style.md`.

## Native macOS menu names (`menu.bar.*`, `menu.app.*`, `menu.edit.*`, `menu.window.*`, `menu.go.*`, `menu.window.zoom`, `menu.view.zoom`)

Czech macOS wording: bar Soubor, Úpravy, Zobrazení, Okno, Nápověda (nouns, as Finder) while F3/F4 and context items stay
verbs Zobrazit / Upravit; Select → Vybrat (TC); Go → Otevřít. Window Zoom → Přepnout velikost (Finder) vs the text-size
submenu Zvětšení.

## Pane is panel, tab is záložka (`menu.view.leftPane`, `menu.view.rightPane`, `menu.tab.*`, `commands.tab*`)

panel over MS podokno (TC, DC); záložka over karta because macOS calls a window tab panel and TC/DC say záložka.

## Volume is svazek, drive is disk (`commands.volumeSelect.label`, `fileExplorer.navigation.groupVolumes`)

svazek over oddíl/jednotka (macOS CoreTypes, Time Machine); disk over jednotka, which is the Windows drive letter.

## Copy: Kopírovat for files, Zkopírovat for the clipboard (`menu.file.copy`, `commands.fileCopy.label`, `fileExplorer.functionKeyBar.copy*`, `menu.edit.copy`, `ui.copyBox.copy`, `crashReporter.dialog.copy`, `errorReporter.dialog.copy`, `viewer.copyDialog.copy`)

F5 file copy is TC's imperfective Kopírovat; clipboard buttons are Apple's perfective Zkopírovat (Edit menu). Progress
Kopírování…, result Zkopírováno either way.

## Delete is smazat, remove is odstranit (`menu.file.delete`, `menu.file.deletePermanently`, `menu.context.removeDownload`, `menu.context.makeAvailableOffline`, `commands.cloudMakeOffline.label`)

smazat for files (macOS Smazat, Ihned smazat); odstranit for taking off a list (macOS Odstranit z Docku). Delete
permanently → Smazat trvale. iCloud pair as Finder: Stáhnout nyní (a one-time download) / Odstranit stažené.

## Undo, put back, roll back (`menu.edit.undo`, `fileOperations.trash.*`, `fileOperations.transferProgress.undo`, `askCmdr.renameUndo.undo`, `fileOperations.rollbackConfirm.*`, `operationLog.*`)

undo → Odvolat: the Edit menu keeps macOS Odvolat akci, toast and dialog buttons say just Odvolat. put back → Vrátit
zpět (Finder; progress Vracení zpět…), roll back → Vrátit změny (reviewed; Vracení změn, Nechat beze změny). Never Zpět
for undo: that is Back.

## Operation log is Historie operací (`menu.view.operationLog`, `operationLog.dialog.title`, `commands.logOperationLog.label`, `settings.operationLog.*`)

Historie operací over Protokol operací (reviewed): protokol is the technical log; this is the user's undoable history.

## Selection words (`menu.select.*`, `commands.selection*`, `commands.navUp.label`, `commands.navDown.label`, `onboarding.stepAi.table.rowSelect`)

vybrat / výběr / zrušit výběr / invertovat výběr (TC) over Finder's lone Odznačit. Cursor moves are Přejít na…, never
Vybrat; Jump to first/last → Skočit na…. Choosing an option is zvolit where both meet; feature-name rows take Výběr.

## Compare folders: složky over adresáře (`menu.select.compareDirectories`, `fileExplorer.compareDirectories.*`)

TC cs writes Porovnat složky although its English says directories.

## Host is hostitel, guest is host (`errors.listing.hostDown.*`, `servers.sheet.connectAsGuest`, `commands.networkRefresh.label`)

Czech host means guest (macOS NetAuthAgent), so a network host is hostitel; the hosts list is síťoví hostitelé (Znovu
načíst síťové hostitele, Vybrat síťového hostitele).

## Go to folder follows Finder (`menu.go.goToPath`, `goToPath.*`)

Otevřít složku… is Finder's Go to Folder…; prose may say přejít do složky.

## Brief and Full view keep TC's names (`menu.view.briefView`, `menu.view.fullView`, `settings.appearance.card.briefMode`)

Seznam / Podrobnosti (TC, DC Ctrl+F1/F2); in prose zobrazení Seznam with the capital.

## Extract: extrahovat (`askCmdr.decision.verbExtract`, `operationLog.summary.archiveExtract`)

extrahovat (TC Alt+F9, MS) over rozbalit, which prose may use.

## Keychain: svazek klíčů and Klíčenka (`servers.sheet.remember`, `servers.refusal.*`, `ai.secretError.*`)

Store svazek klíčů, app Klíčenka (macOS); off macOS systémová klíčenka, because it isn't Apple's Keychain.

## Reports and feedback (`menu.help.sendFeedback`, `menu.help.sendErrorReport`, `feedback.*`, `crashReporter.*`, `errorReporter.*`)

feedback → názor (AppKit), crash report → zpráva o havárii, error report → zpráva o chybě: one noun for every report.

## Toggle commands follow Total Commander (`commands.viewShowHidden.label`, `commands.tabTogglePin.label`, `commands.sortToggleOrder.label`, `commands.selectionToggle*`, `commands.tagsToggle*`)

Named two states → TC's „X nebo Y“ (3013 Zobrazit nebo skrýt skryté soubory; tags Přidat nebo odstranit červenou značku,
as their descriptions); a plain switch → Přepnout; sort direction → Obrátit pořadí řazení (Nautilus).

## Settings paths and sections (`settings.section.*`, `crashReporter.sentToast.changeSettings`, `commands.handler.openTerminalHere.hint`, `settings.askCmdr.provider.*`)

Nastavení > Sekce with >, never ›; the section name is `settings.section.*` byte for byte (Aktualizace a soukromí, not
Aktualizace). In prose v části {sekce}, not v sekci.

## Sort by takes Finder's genitive (`menu.view.sortBy`, `menu.sort.*`, `multiRename.extensionMask`)

Seřadit podle → Názvu, Přípony, Velikosti, Data změny (Finder ArrangeBy): the items finish the sentence; column headers
and field labels stay nominative (Název, Velikost, Přípona). Vzestupně / Sestupně (Dolphin).

## Search button is Hledat, the feature is Hledání (`queryUi.bar.runLabel`, `search.dialog.title`, `settings.section.search`, `onboarding.stepAi.table.rowSearch`)

The run button is the verb (Finder button); the dialog, Settings section and feature row are the noun Hledání.

## Modified: Datum změny for dates, Změněné for shortcuts (`fileExplorer.columns.modified`, `queryUi.results.col.modified`, `queryUi.filters.chip.modified`, `shortcuts.section.filterModified`, `shortcuts.section.modifiedTooltip`)

Datum změny (Finder column); the shortcut chip filters what the user changed, no date, so Změněné / Změněno oproti
výchozímu nastavení.

## Date filter is od/do (`queryUi.date.comparator.*`, `queryUi.filters.date.summary.*`, `queryUi.recent.modifiedAfter`, `queryUi.recent.modifiedBefore`)

od/do over po/před because the filter includes the boundary date; recent-search chips repeat it. between → v rozmezí
(TC).

## Month presets avoid declining {month} (`queryUi.date.preset.thisMonth`, `queryUi.date.preset.lastMonth`, `onboarding.stepAi.table.searchWithout`)

{month} arrives as standalone nominative (květen), so 1. {month} is ungrammatical: 1. den tohoto měsíce ({month})
{time}, like the week presets; parity keeps the placeholder. Prose says od 1. tohoto měsíce.

## Prompt: dotaz from Cmdr, výzva from the system (`settings.indexing.*`, `queue.row.awaitingAnswerTooltip`, `fileExplorer.navigation.useSavedPasswordMessage`, `errors.*.notConnected.suggestion`)

Cmdr's own question → dotaz; a macOS or phone prompt → výzva (MS); AI prompt text → zadání (macOS cs).

## Disconnect names what it disconnects (`adb.disconnectDeviceAriaLabel`, `fileExplorer.navigation.disconnectPlaceAriaLabel`)

Odpojit zařízení {name} vs Odpojit server {name}: the head noun carries gender the placeholder can't.

## Selection summary in the genitive (`fileExplorer.summary.*`, `fileExplorer.selectionInfo.noSelection*`)

z + genitive (3 z 10 souborů) because English of is z; fileNoun/dirNoun carry genitive branches, unlike the nominative
`fileOperations.*.scanFile`.

## Errors and retries (`errors.listing.*.title`, `errors.listing.*.suggestion`, `errors.write.*`, `errors.eject.*`, `servers.hub.status.notConnectedYet`)

Errno titles reuse macOS ErrnoErrors (Prostředek je zaneprázdněný); Host is down → Hostitel nereaguje; Connection reset
→ Připojení bylo přerušeno. Retry bullet: Zkuste sem přejít znovu; Here’s what to try → Co můžete zkusit:. Raw counts
sit behind a noun or in brackets (Soubory, které…: {count}; (počet: {count})); not removable → nejde vysunout; Zatím
není připojeno / Už není připojeno.

## macOS app and pane names (`errors.listing.diskReadProblem.suggestion`, `errors.write.permissionDenied.suggestion.*`, `errors.write.fileLocked.suggestion.mac`, `settings.behavior.fileSystemWatching.lowDiskSpace*`, `settings.appearance.appColor.*`)

Quote macOS byte for byte: **Disková utilita**, **Záchrana**, Informace > zrušte zaškrtnutí políčka Zamčeno, Startovací
disk, Barva zvýraznění. balík for bundles, balíček for packages; Apple silicon → čip Apple.

## System default is Podle systému (`settings.appearance.language.opt.*`, `settings.appearance.dateTimeFormat.opt.system`, `settings.theme.mode.opt.system`, `settings.behavior.textEditorApp.systemDefault*`)

Podle systému over Výchozí systémový: no gender agreement needed and one English, one Czech.

## No singular gendered past about the user or the assistant (`askCmdr.tool.*`, `askCmdr.decision.*`, `askCmdr.renameReview.evidence.userEdited`, `errorReporter.dialog.notePlaceholder`, `onboarding.stepBeta.terms.consent`, `fileOperations.cancelRollback.reason.*`, `commands.handler.openTerminalHere.*`)

Tool lines: reflexive passive while running (Kontrolují se…), noun then participle when done (Složka vypsána, Návrh
prohlédnut; reviewed). Mám přečtené …, Cmdr as subject, ručně over sám/sama. The plural -li is neutral, so the user's
own act takes jste + -li: Odmítli jste: / Schválili jste:, Tento název jste zadali vy, Co jste se pokoušeli udělat?

## Two-pane and dialog wording from Total Commander (`fileOperations.transferDialog.*`, `fileOperations.delete.foldersPart`, `multiRename.*`, `viewer.toolbar.*`, `queryUi.ai.patternLabel.*`)

Odkud / Kam headings; count parts accusative (složku); Multi-rename labels (Maska názvu, Nahradit za, Krok, Hledat);
Lister modes Binární / Hexadecimální / Kódování; glob → Maska, pattern → Vzor.

## Typing modes are Skok and Filtr (`fileExplorer.typeToJump.*`, `fileExplorer.quickFilter.*`, `settings.fileExplorer.typeToJump.mode.opt.*`)

Nouns over verbs because the indicator prefixes (Skok: , Filtr: ) reuse the Settings option word.

## Network and servers (`fileExplorer.pane.directConnection*`, `fileExplorer.pane.openLocalNetworkSettings`, `settings.network.localNetworkAccessLabel`, `servers.sheet.signInTitle`, `servers.sheet.editTitle`, `servers.hostKey.*`, `menu.volume.*`, `onboarding.cloudSetup.title`)

systémové připojení vs přímé připojení; sheet titles Přihlášení: {name}, Nastavení: {provider}; host key → klíč serveru
{host}, fingerprint otisk, revoked kompromitovaný; busy suffix (probíhá operace), gender-free. The OS-named
{localNetwork} goes in quotes behind a noun: Oprávnění „{localNetwork}“, Otevřít nastavení „{localNetwork}“.

## SMB encryption, busy provider (`*requireSmbEncryption.*`, `*providerBusy.*`)

require → vyžadovat (macOS Vyžadovat heslo); prose quotes the label (volba „Vždy vyžadovat šifrování SMB“). Poskytovatel
{provider} so zaneprázdněný agrees with a noun.

## Git (`errors.git.*`, `fileExplorer.git.*`)

repozitář (úložiště is storage), větev, značka, commit, pracovní strom; worktree, stash, HEAD verbatim. Dirty chip →
necommitováno (neuloženo claims unsaved files); checkout names the command in brackets (git checkout).

## Settings values (`settings.tint.*`, `settings.appearance.appColor.opt.cmdrGold`, `settings.managed.summary.*`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `licensing.*`, `indexing.eta.*`)

Tints agree with barva (Azurová, Limetková, Modrozelená, Zlatá (Cmdr)); Off → Vypnuto; email jmeno@example.com;
perpetual → časově neomezená; Rust crates verbatim; ETA zbývá {n} min, lowercase; price 59 $, years © 2024–2026.

## Status words that must not collide (`queue.row.statusAwaitingAnswer`, `queue.toolbar.selectedCount`, `downloads.shortcutRow.*`, `fileOperations.transferProgress.existing*`, `onboarding.stepFda.*`)

Potřebuje odpověď (Čeká is queued); Existující / Příchozí (nový is the create command); Zamítnout for deny, Ukončit a
znovu spustit (macOS QUIT_APP). Shortcut chips Aktivní / Neaktivní; a bare count is Vybráno: #.

## Address is the neutral vy-plural (`onboarding.stepBeta.greeting`, `licensing.dialog.mailtoBody`, `askCmdr.limit.continueMessage`, `onboarding.stepAi.table.renameWith`, `onboarding.stepAi.table.selectWith`, `common.attachEmailPlaceholder`, `operationLog.initiator.user`, `fileOperations.transferProgress.background`, `fileOperations.backgroundStart.background`, `errors.listing.*.suggestion`)

vy-plural, lowercase, per `../formal-informal-decisions.md`: Stiskněte, Zkuste to znovu, můžete; David's welcome Hi →
Zdravím, the user's mail Hi → Dobrý den; you@ → jmeno@example.com; You as initiator → Vy. Buttons stay infinitive
(Zařadit do fronty; Přesunout na pozadí for a running operation, Spustit na pozadí in a confirm dialog, where přesunout
would read as moving files). Only words the USER types to the AI stay ty (Pokračuj prosím…), as Czechs write to a
chatbot (reviewed).

## Search coverage fallback is toto umístění (`search.coverage.*`)

The unnamed {drive} may be a drive or a server, so toto umístění; every {drive} frame keeps it bare with no head noun
(Teď se indexuje {drive}).

## Kept after native review (`menu.tab.*`, `menu.app.acknowledgements`, `menu.app.changelog`, `menu.app.onboarding`, `menu.help.whatsNew`, `fileExplorer.quickFilter.intro.gotIt`, `fileExplorer.network.browser.status.error`, `settings.mediaIndex.importanceThreshold.*`, `onboarding.wizard.step*`, `askCmdr.thinking`, `servers.refusal.hostKeyRevoked`, `viewer.statusBar.badge.streaming*`, `viewer.toolbar.tail.*`, `commands.paneLeftVolumeChooser.label`, `settings.appearance.dateColors.opt.wilting`, `ai.local.installStep*`, `commands.aboutOpenUpgrade.label`)

Confirmed as shipped: záložka, Poděkování, Seznam změn, Co je nového, Úvodní průvodce, Rozumím, Problém, Smetí, v/ve per
plural branch, Krok {step} z {total} (z before a variable, ze before a literal 4), Přemýšlí…, kompromitovaný,
streamování, Sledovat, přepínač svazků, Vadnutí. Rejected: modul runtime → běhové prostředí, upgradu → s nákupem
licence.
