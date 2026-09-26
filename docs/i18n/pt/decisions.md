# pt decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each citing the keys it shaped. The chosen forms live in
`terms.json`; this file keeps only the non-obvious why that defends a ruling against a well-meant "fix". Voice and
grammar rules: `style.md`. Open questions: `review-queue.md`. Sources: the Brazilian pile (`_ignored/i18n/pt-BR/`) or
the installed macOS `pt_BR.lproj` bundles.

## The bare `pt` pile is European (`settings.archives.compressionLevel.description`, `fileOperations.transferDialog.compressLevelCaption`)

- `_ignored/i18n/pt/` is pt-PT; mining it once shipped `ficheiros` and `demoram mais tempo a comprimir`. Mine `pt-BR/`,
  and re-check any ruling whose sources name the bare `pt` pile.

## Apagar, never Excluir (`fileOperations.delete.*`, `commands.fileDelete*`, `ai.local.delete*`, `settings.mediaIndex.clip.delete*`, `settings.mediaIndex.reclaim.*`, `transfer.delete`)

- `Apagar` over `Excluir` because macOS Finder pt-BR has zero `Excluir`. One verb for files AND stored data (the AI
  model, index entries): a split once left `Excluir modelo de IA?` over an `Apagar` button.
- The noun is `apagamento` (`queue.empty.body`); it's what keeps `exclusão` out. `excluir` survives only for exclude
  (`queryUi.scope.hint`, excluded folders).

## `drive` is always `disco` (`errors.listing.*`, `errors.provider.pCloudFuse.*`, `fileExplorer.unreachable.detailTimeout`)

- `disco` over MS `unidade` and the loan `drive` because Finder says `Discos externos`, `Discos rígidos`; `unidade` is
  only the physical mechanism there. Network drive → `disco de rede`, never `unidade de rede` / `drive de rede`.
- Only brand names (iCloud Drive), the `{drive}` placeholder, and key names keep `drive`. `pCloud's virtual drive` →
  `disco virtual do pCloud`: a common noun after the brand.

## Parent folder (`commands.navParent.label`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `fileExplorer.doubleClickHint.body`, `errors.listing.notFound.suggestion`)

- `pasta superior` kept although the pile favors `pasta pai`: switching forks menu, settings, and toasts, so it's a
  whole-catalog migration or nothing (review queue). `pasta principal` is the wrong meaning.
- The top of a volume is `a pasta raiz de um volume`, never `pasta superior` (that's one level up).

## Archives (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.archiveTitle`/`archiveMessage`, `fileOperations.archivePassword.*`, `queue.row.label`)

- `arquivo compactado` because bare `arquivo` means file; the double `arquivo` when both co-occur reads fine, keep it.
  When the format is named, the plain `arquivo zip` (`fileExplorer.readOnly.archiveMessage`), and
  `fileOperations.transferDialog.pathErrorNotZip` says `nome do arquivo compactado` so it doesn't ask for a file name.
- Browse → `Navegar`, contrasting `Abrir` in one segmented control, so the two must differ. App bundle →
  `pacote de aplicativo`; `settings.archives.ooxml.*` keeps bare `pacotes` so the row stays wider than the bundles card.
- Compress → `Comprimir` / `Comprimindo`; compression level → `Nível de compressão`.

## Settings sections and transfer dialog (`settings.section.*`, `fileOperations.transferDialog.*`, `fileOperations.shared.scanningTooltip`)

- Pre-transfer counting → `Analisar` / `Analisando`; the drive-index walk → `varredura` (§ Drive scan). Never swap them.
- `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle` → `De` / `Para` (Total and Double Commander pt-BR
  ship this pair); `origem` / `destino` stay for the controls.
- Copy noun `cópia`, move noun `movimentação`; progress arms are gerunds (`Renomeando`, `Criando pasta`), never the
  pt-PT `A criar`.

## Operation queue (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`/`backgroundedToast`)

- `Fila de operações` over `Fila de transferências` because the queue also holds deletes, renames, and creations;
  `transferência` stays the narrow word for one copy or move in flight. It sits beside `Registro de operações` in the
  View menu and shares its head noun on purpose: rename both or neither.
- `operação` is feminine like `transferência`, so `Encontre-a`, `ela`, and `selecionadas` agree unchanged.

## Queue chip, dismiss, and failure notices (`queue.row.dismiss*`, `queue.toolbar.dismissAll`, `queue.failureToast.*`, `queue.chip.*`)

- Dismiss → `Dispensar`, never MS `Ignorar`: `Ignorar` is this catalog's Skip, two rows away. Not `Descartar` either.
- `Não foi possível concluir` + the action noun (Finder's own frame); the `other` arm equals the `queue.row.status`
  failed arm, so toast, row, and chip match. In `queue.failureToast.summary` the house phrase sits OUTSIDE the plural so
  only the noun branches.
- `queue.chip.ariaLabel` spells `por cento`; the visible tooltip keeps `{percentText}%` tight. The chip's time-left
  reuses `fileOperations.transferProgress.etaRemaining`; don't derive a second phrasing.

## Background button (`fileOperations.transferProgress.background/backgroundAria`)

- `Em segundo plano` over bare `Segundo plano` because the prepositional phrase reads as a command; the noun would title
  a section. Never `2º plano` or the wallpaper senses. The aria contains the label as `em segundo plano`.

## Stalled transfer (`fileOperations.transferProgress.close`/`stall*`)

- `Sem progresso há {duration}`: `há` is how pt-BR says an elapsed stretch up to now; not `por` / `durante`.
- `A transferência parou de avançar` over `travou` (reads as a crash) or bare `parou` (reads as ended).
- `O arquivo de registro tem os detalhes`: `arquivo de` separates the log file from the `registro de operações` feature.

## Quit gate (`main.quit.*`)

- `Encerrar` (Finder and `commands.appQuit.label`). `Continuar trabalhando` over `Cancelar`, which would read as
  cancelling the operations, and over any postpone phrase, since the countdown is gone for good.
- `em andamento` over `em execução` (Finder's own quit sentence). `é interrompido onde está` over `para onde está`,
  which garden-paths on `para`. The `encerr-` and `gravado` repetitions are deliberate: each is its concept's sourced
  term.

## Operation log (`operationLog.*`, `commands.logOperationLog.*`)

- log → `registro` across the catalog (changelog, log files). Rollback → `Reverter`; the participle agrees with its
  subject: `Revertida` (the operation) vs `Revertido` (an item). Never unify them.
- Status words match `queue.row.status` exactly; `Não foi possível concluir` avoids a bare `Falhou`.

## Rollback confirm and finish (`fileOperations.rollbackConfirm.*`, `operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `queue.row.reversalInFolder`)

- `Precisa de resposta` over DC's `Aguardando resposta`, which starts like the `queued` arm the `@key` must not echo.
- `Manter os arquivos` over `Mantê-los`: the previous sentence talks about REPLACED files, so the pronoun could point
  wrong. `Concluir a reversão` pairs with `iniciar a reversão`; `Para concluir` was refused because it first reads as
  "in conclusion". `percorrer` over `revisar`, which is Review.
- `in {folder}` → `em {folder}`: no article, no quotes, so any name fits.

## Rollback outcome notice (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

- Reason lines use `{name} ficou como está: <motivo>.` (copied from `askCmdr.renameUndo.skipReason.*`): the ITEM is the
  subject, so nothing agrees with `{name}` and no `você` is implied. `spotTaken` swaps to `ficou onde está` because it's
  about place.
- `mudou depois que o Cmdr colocou lá`: the brand carries the reason, and the null object avoids a gendered clitic.
- Headlines write `O Cmdr` / `A reversão` because a bare `Apagou…` reads as `você apagou`.
- put back has three verbs, and the `@key` asking for one is wrong for pt: `colocar de volta` (out of the Trash),
  `restaurar` (an old NAME, `askCmdr.renameUndo.*`), `levar de volta` (a rollback moving files home). ❌ Don't flatten.
- `removed` and `deleted` both → `apagar`: `Remover` is taking an entry off a list.

## Staged leftovers (`fileOperations.cancelRollback.stagedLeftover.*`, `fileOperations.leftovers.stagingFolderKept`)

- `numa transferência posterior`, never `da próxima vez`: cleanup skips anything under an hour old, so an immediate
  retry deletes nothing.
- `stagingFolderKept` guards the USER's files: never suggest deleting the folder. `deixou tudo onde está`: `tudo` avoids
  a clitic; not `no lugar`, which means "instead" in this catalog.

## Rename chain and unconfirmed rename (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, `fileExplorer.rename.unconfirmed*`, `fileOperations.validation.nameNotUsable`)

- `e outros {N} itens` with `outros` BEFORE the numeral, Finder's order (`MR201_V3`), over Nautilus' `{N} outros`.
- The unconfirmed toast's subject is `a renomeação`, never `o arquivo pode ter sido renomeado`: it may be a folder. The
  noun repeats in the second sentence because `ela` / `elas` would be ambiguous in the `AndOthers` key.
- `nameNotUsable` has no final period: it's composed into `{reason}`.

## Rename and create refusals (`errors.mutation.*`, `errors.volume.*`)

- `timedOut` isn't a failure: `ainda pode ser concluída`, never `não foi possível`. `deviceSessionReset` isn't a
  disconnect: never `desconecte`.
- `perdeu a referência`, never `perdeu o controle` (reads as "lost control"). `O macOS se recusou a mover`: pt-BR
  proclisis with `se`, and `recusar` over `não permitiu`, which reads as a permission problem.
- `Obter Informações` keeps Apple's capitals when a string names the panel. `a alteração` is the requested change,
  distinct from `as mudanças` in the file system.

## Eject and disconnect refusals (`errors.eject.*`)

- Each value follows a colon in `fileExplorer.pane.ejectFailedToast` / `disconnectFailedToast`, so it's a full sentence
  with a capital. `timedOut` says `ainda pode ser concluída sozinha`, never a failure.
- The named refusals share `<sujeito> ainda está usando este disco. <ação>, depois ejete-o de novo.` `{app}` opens bare
  (a name like `mds_stores` isn't an app, so no `O app`); `ele` / `eles` are safe because the referent is always an app,
  masculine. `outros apps` is a list's last item: the `e` comes from `Intl.ListFormat`, never the string.
- `BySystem` says `está trabalhando com` because nothing can be closed; a disk image is `ainda está montada`, which
  points at ejecting.

## Trash buttons and refusals (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `errors.mutation.trash*`, `errors.write.trashRefused.title`)

- Undo → `Desfazer`; put back → `colocar de volta` (Finder `N153.1`). `Este disco não tem Lixo.` states a fact, so it
  skips the error-screen `não oferece suporte`.
- `errors.write.trashRefused.*`: `{count} dos itens que você escolheu` reads right at any count without a plural. Never
  suggest trying again (a permission refusal repeats). The title is identical to four `errors.write.*.title.trash`
  twins.

## Online-only delete warning (`fileOperations.delete.cloudOnlineOnlyMixedWarning`, `fileOperations.delete.cloudOnlineOnlyAllWarning`, `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Four facts are mandatory: the Trash would download first, so only deleting the WHOLE selection is offered, no copy
  stays in the Trash (don't soften it), and the listed ways out. The quoted `“Apagar”` equals
  `fileOperations.delete.confirmDelete`.

## Crash dialog (`crashReporter.dialog.body.ended`/`keptRunning`/`unknown`, `settings.updates.crashReports.description`)

- `keptRunning` and `unknown` must not say `falha`, `encerrou`, `parou`, or `travou`: nothing crashed.
  `O Cmdr teve um problema`, and the report loses `de falha`.
- `continuou funcionando` over `continuou rodando`, which would echo `rodando em segundo plano` and imply the app went
  to the background. `unknown` stays true either way: no `encerrou`, no `continuou`.
- The settings label keeps `Enviar relatórios de falha` (a name); its description covers both cases.

## Report notes (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- `nota`, the send dialog's word, over MS `observação`: the two dialogs share one box. `entra no mesmo relatório` says
  outright that nothing is sent twice.
- `Ver X` for View buttons; `Visualizar` is the View menu. `Ver por quê` (`askCmdr.wakeToast.openThread`):
  sentence-final `por quê`; a bare `porquê` would need an article.

## Selection dialog (`selection.*`)

- Deselect → `Desmarcar` (Finder pt-BR `Desmarcar Tudo`), never pt-PT `Desselecionar`. The menu, the palette, the
  settings description, and the dialog title all name it the same way; a mismatch was the bug.
- Hints start with the button's exact text, so button and hint read as one sentence.

## One thing, one name (`queryUi.scope.toggle.caseSensitive`, `viewer.search.*`, `queryUi.results.scan*`, `transfer.delete`, `fileOperations.transferDialog.pathErrorNotZip`)

- Catalog-wide: trash `Lixo` never `Lixeira`; Reset `Restaurar` (Finder) with `redefinir` only for zoom; error report
  `relatório de problema`; search is the `busca` family, viewer included; `dir` is always `pasta`; the drive scan is
  `varredura`.
- Agreement is grammar, not drift; never unify `Ambos` / `Ambas`, `Revertida` / `Revertido`, `Modificado` /
  `Modificados`. Noun vs verb: `Pré-visualização` vs `Pré-visualizar`, `Busca` (a section) vs `Buscar`.
- `Tentar novamente` on buttons (Finder), `tente de novo` in prose (about 100 values). ❌ Never sweep `de novo` →
  `novamente`.
- `Em execução` (a running server) vs `Em andamento` (a task); `memória` (RAM) vs `anotações` (Ask Cmdr's memory).
- `viewer.saveAs.defaultName` is `selecao` without the cedilla on purpose: it's a default file name.

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Terms come from Finder pt-BR, capitals don't: Finder title-cases (`Nova Pasta`), Cmdr uses sentence case
  (`Nova pasta…`).
- Cut is `Cortar` in `menu.edit.cut` (Finder's menu bar) and `Recortar` elsewhere (MS; Finder's own `QK5`); review
  queue. Changelog → `Registro de alterações` to keep the `log → registro` family over MS `Log de alterações`.
- A busy item is its idle sibling byte for byte plus ` (ocupado)`, invariable because it describes the server or disk.
- `Remover` on a tag (`menu.tag.removeNamed`): removing a tag deletes nothing. `Apagar` is reserved for files.

## Dock icon menu (`menu.dock.*`)

- Dock.app's `pt_BR.lproj/DockMenus.strings` is the Tier 1 source and isn't in the pile: `Abrir Cmdr`, no article or
  quotes, like `Ocultar %@`; the quoted form `Abrir “%@”` is for FILES.
- `menu.dock.locationInParent` stays `{name} ({parent})`: AppKit's `%@ (%@)` is unchanged in Apple's `pt` while `ja`,
  `zh_CN`, `ar`, and `he` adapt it, so it's sourced. `menu.dock.searchFiles` equals `menu.edit.searchFiles`.

## Dock pin nudge (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- `Dock`, `Finder` stay English (Finder `Adicionar ao Dock`); Applications → `pasta Aplicativos`. The accept button
  copies Apple's `Adicionar ao Dock`, not the `fixar` pair.
- `Não precisa` over `Não, obrigado` (agrees with the speaker's gender), `Agora não` (promises a next time that never
  comes), and `Não, valeu` (too slangy).
- Outcome lines never sound like failure: `addedButDockDidNotRestart` says the icon IS in place.
- `há alguns dias`, never a number: the threshold can change.

## Show in Finder offer (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- `“Mostrar no Finder”` in curly quotes, identical wherever the command is named. `há um tempo`, never a number. The
  first-time notice explains; it never apologizes.

## macOS pane names, Apple menu items, and the example email (`settings.updates.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`/`hideOthers`)

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` resolve at runtime, so ❌ never contract a
  preposition with them (`nos {system_settings}`): use `em {system_settings}`.
- `Conta Apple` (not `Conta da Apple`), `Geral`, `Itens de Início de Sessão e Extensões` (not the old short name), from
  the macOS bundles.
- `askCmdr.renameUndo.undone` names the object (`O nome anterior de … foi restaurado`), whole sentence inside the
  plural.

## Ask Cmdr names only the chat panel (`askCmdr.wake*`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.*`)

- `Ask Cmdr` survives only where the English keeps it; prose about the AI says `O Cmdr` or `a IA`. Read the current
  English, never the key's history.
- `chat` (noun), `conversar` (to chat), and `conversa` (conversation) each follow their own key's English.

## Bulk-rename review (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`)

- Allow / Deny → `Permitir` / `Negar` over `Recusar`, because it's an approval gate, not an invitation. `Permitir tudo`
  / `Negar tudo`: `tudo` dodges agreement with `renomeações`.
- Overwrite badge → `(substituição!)`, a noun like its sibling badges; `(substituir!)` would read as a button.
- The cycle tooltip says `trocar os nomes desses arquivos entre si`: `girar` reads as rotating images.

## What Ask Cmdr reads inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

- No sentence may promise that no content leaves: the rule says `nunca envia arquivos inteiros, fotos ou miniaturas` and
  `uma parte limitada`. `detalhes da câmera`, never `metadados` / `dados EXIF`: the English avoids jargon.
- `onde ela foi tirada` in the rule, because `localização de uma foto` alone can read as the file path.

## Cloud AI consent (`ai.cloudConsent.*`, `askCmdr.gate.*`)

- Sentences citing the switch quote its label after `Ative`: `Ative “Permitir IA na nuvem”`. `Permita a IA na nuvem`
  wouldn't reproduce the label. `serviço` (this family) and `provedor` (`ai.cloudConsent.askCmdr.*`) follow the English.

## AI provider setup (`onboarding.cloudSetup.*`)

- endpoint → `ponto de extremidade` (MS; Apple is silent). The label and its caption changed together, since a loanword
  label over a native caption reads as two fields.
- `then` in a two-step instruction stays `e depois`: the order is the information.

## Image index (`fileExplorer.imageIndex.*`, `settings.mediaIndex.*`, `search.imageResults.*`, `askCmdr.tool.imageFacts/searchPhotos.*`)

- The feature is `busca de imagens` / `busca por descrição`, never `pesquisa`; `pesquisável` stays as an item's
  adjective. badge → `selo` over `distintivo` (police badge) and `emblema` (heraldic).
- File tooltips agree with feminine `imagem` (`Indexada`, `Modificada`). An indexing pass → `rodada`, not `passagem` or
  `varredura`. Junk → `descartáveis`, since `Lixo` is the Trash.
- `deixou de acompanhar`, never `perdeu o controle`. `o conteúdo das suas fotos` stays explicit about reading content.
- Network strings keep `fotos` where the English says photos, `imagens` where it says images. A photo archive is
  `acervo de fotos`, not `arquivo`.

## Drive scan (`indexing.run.*`, `indexing.step.*`, `indexing.enrich.queued`, `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`)

- The drive walk is `varredura` (`Primeira varredura completa`, `Nova varredura completa`), distinct from the transfer
  pre-count `Analisar`. `Verificação de mudanças` matches the sibling headers' noun shape.

## Master indexing switch (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`tooltipIndexingOff`/`menuIndexingOffNote`, `settings.indexing.masterOffNote`/`overriddenBadge`)

- `ativado` / `desativado` label software; `ligado` / `desligado` is physical power. `Desativado com a indexação` keeps
  the badge short: it only renders under that toggle.

## Phone index stays yellow (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`bodyPhone`)

- None may mention disconnecting: the phone just never reports changes. `atualizado`, never `em dia`, pairs with the
  title's `desatualizado`. `os arquivos do celular` over `dele`, which would agree with `{name}`.

## Drive leaving or pulled (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`, `errors.write.moveNotConfirmed.*`)

- `O disco {name} está sendo / foi desconectado`: the noun leads so the participle agrees with it. `em` needs an
  article, so write `no disco {volumeName}` / `no disco {counterpart}`; `para {counterpart}` goes bare.
- The sided lines say `disco`, the generic ones `dispositivo` (they also cover phones and servers). Don't unify.
- `moveNotConfirmed` is neither failure nor loss: `Dê uma olhada em`, not `Verifique`; `então ele manteve…` keeps the
  subject explicit.

## Server pane, sign in, and forget (`servers.*`, `fileExplorer.navigation.connectionTooltip*`/`disconnect*`/`forget*`)

- sign in → `iniciar sessão` (`iniciar a sessão` in prose); signed out → `Sessão encerrada`, which agrees with the
  session, not the person. Disconnect a server, never `Ejetar`.
- `servers.refusal.authMethodUnsupported` makes `O Cmdr` the subject: keeping the server there needs a clumsy `a que`.
- `forgetServerConfirm` / `forgetSecretConfirm` write the noun (`tira esse servidor`, `a senha`), never a pronoun on
  `{name}`.

## Server hub (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast`/`serverUnpinnedToast`/`pinRefusedToast`/`networkVolume`, `shortcuts.scope.servers`/`places`)

- The row is `Servidores`, its group stays `Rede`: the two coexist on purpose. Places → `Locais` (Finder), not `Lugares`
  or the SMB-only `Navegador de compartilhamentos`.
- `Status` over Apple's `Estado`, to match `licensing.section.labelStatus` (review queue). Last used → `Último uso`:
  Apple's `Última Usada` locks the feminine, Mail's `Usado pela última vez` doesn't fit a column.
- `Salvo` for an idle saved server: nothing went wrong. nearby → `por perto`, not `Próximo` (means "next" here).
- `conferir` when the PERSON checks (`Aguardando você conferir a chave`), `verificar` when Cmdr does.
- `local network` lowercase is the concept; `Rede Local` capitalized is the macOS permission's name.

## Server sheet and host keys (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`signIn`/`hostKeyChanged*`, `goToPath.dialog.opensServer`/`addsServer`, `commands.serversConnect.label`)

- Browse (file picker) → `Escolher…` (Apple's `Choose…`), not `Navegar` (archives), `Explorar` (network), or MS
  `Procurar…`; the split is allowlisted in `apps/desktop/scripts/i18n-term-consistency-allowlist.json`.
- passphrase → `frase-senha` (Apple), which separates the key file's phrase from the account `senha`.
- `quem cuida do servidor` for the owner avoids a masculine `o dono`. `A chave do servidor mudou` writes the noun
  because `dele` would lean on the title's `{name}`.

## Reconnect and key-only sign-in (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

- `Reconectando a {name}…`: `a` from the sibling `servers.paneState.connecting`, which alternates in the same spot.
- `então não há nada para …` is the fixed mold (eject, disconnect, type); a fourth key copies it.
- `Abra-o de novo`: safe because `servidor` is the only masculine candidate.

## Pin, unpin, and trusted host keys (`menu.network.pinToSwitcher`/`unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.appearance.tintSmb.*`)

- `Fixar` / `Desafixar` (Safari, Notes). `Fixar no seletor` shortens like the English; the menu opens inside the
  switcher.
- `Confiável desde` because `confiar em` doesn't passivize (`Confiada em…` is ungrammatical).
- `Seu grupo Rede está ficando grande`: `grupo longo` isn't Portuguese. The ADB status lines write `O Cmdr`, since a
  subjectless negative reads as `você não está`.

## ADB settings (`settings.fileOperations.adbEnabled.*`, `settings.fileOperations.adbBinaryPath.*`)

- `Localização do adb` (Finder), `sistema de arquivos` (Disk Utility), `por ADB` like `por USB`. `adb` stays lowercase:
  it's the command.

## Android over ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

- Android's own pt-BR wins on the phone: `depuração USB`, `Permitir`, `toque em`. The ADB hint's `Saiba como` over a
  bare `Como`, which doesn't read as a link.
- `Desconectar`, never `Ejetar`: the phone stays on the cable. `Você parou de abrir o seu celular` over `cancelou`,
  since `Cancelar` is the button.

## Root and start folder (`servers.sheet.rootFolder*`/`startFolder*`/`nameHelp`, `servers.refusal.startFolderOutsideRoot`/`rootNotFound`/`startFolderNotFound`/`saveUnconfirmed`)

- `pasta raiz` (never `raíz`) and `pasta inicial`, not `pasta pessoal` / `pasta base` (those are the user's home).
  `nunca sobe além desta pasta` avoids the pleonasm `sobe acima`.

## Onboarding (`onboarding.moreAbout`, `onboarding.wizard.stepTooltip`, `onboarding.stepFda.why`/`.ifAllow`, `onboarding.stepAi.*`, `onboarding.stepBeta.checklist.*`/`.signup.*`/`.openBeta`, `onboarding.stepOptional.*.summary`)

- GitHub star: the button is `Adicionar aos favoritos` (GitHub pt), the count is `estrelas`. Different words on purpose,
  matching what the person sees on GitHub.
- step → `etapa` everywhere in onboarding, never `passo`. `Rede Local` quoted exactly as the macOS privacy row, never
  the paraphrase `Acesso à rede local`, which doesn't exist there.
- The four `stepOptional.*.summary` lines have the switch as implied subject and must not wrap: same length or shorter
  than English. `processo nativo do macOS` over MS `manipulador`, matching the caption behind it.
- `stepAi.local.tooltip` quotes `stepAi.cloud.label` byte for byte, and the signup status lines name `Salvar`: change
  them together.
- `burro` for the deliberately blunt "dumber". `erro de digitação` is the person's typo, not the app's.

## Full disk access badge (`onboarding.fdaBadge.*`)

- The badge is sentence case (`Sem acesso total ao disco`); `Acesso Total ao Disco` stays for strings naming the pane.
  It equals `onboarding.stepAi.bannerTitle.denied`, and the aria opens with it verbatim.

## Favorites menu (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent`/`favoritesAlreadyAdded`/`favoritesCantAddHere`/`seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- `favoritos` lowercase as a common noun, `Favoritos` only where the English names the section.
- `Esta pasta já está nos favoritos` over `já é um favorito`, whose masculine predicate clashes with `pasta`.
  `favoritesCantAddHere` uses `funcionar em` to stay short.
- `pressione`, never `aperte`. `ir até esse favorito` because `ir` needs a destination.

## Select same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- kind → `Tipo` (Finder). `menu.context.selection` is the NOUN `Seleção` (the submenu's content), not the verb
  `Selecionar`. `com a extensão *.{extension}` leaves nothing agreeing with the mask.

## Viewer fetch pane (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Fetching → `Obtendo` (Finder), not `Baixando` (a local archive downloads nothing) or `Buscando` (the viewer's search).
- `para pré-visualizar` and `até agora` without a pronoun or participle, so nothing agrees with `{fileName}` or the
  formatted unit.

## Mount and share-list failures (`errors.mount.*`, `errors.shareList.*`, `fileExplorer.network.osMountFallback.*`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- NetAuthAgent.app (not in the pile) is Tier 1 for these: `compartilhamento`, `convidados`. `quando quiser` avoids
  `pronto/pronta`.
- `Você está conectado` / `continua conectado`: Apple's own unmarked masculine. `4x mais lenta do que`, the multiplier
  tight.
- `shareNotOnServer` is the one case where retrying can't help: no `agora`, no `tente novamente`.

## Look-alike names (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.explanation`, `errors.volume.ambiguousName`)

- Plain words (`parecem iguais`, `escreve de forma diferente`), no `Unicode` or `normalização`. `armazenar`, never pt-PT
  `guardar`. `a pasta superior`, since `pasta acima` reads as "listed above".

## F4 text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- `Editar arquivos com` because `com` takes any app name without a gendered `no` / `na`; `{app}` goes after `em` bare.

## Usage stats (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

- No `anônimas` (a stable random id means they never were), and ❌ never `pseudônimo` / `pseudonimizado`: that jargon is
  exactly what the English avoids. `um identificador aleatório`, `ligado a`.

## Old macOS and old WebKit (`main.oldMacos.*`, `main.oldWebkit.*`)

- `compatível` over the calque `suportado`. best effort → `faz o que dá`, not `melhor esforço` (contract-speak).
  `Atualização de Software` keeps the pane's capitals.

## Esc and full screen (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `Esc` (AppKit's name) over the older `ESC`. `Este aviso só aparece uma vez` makes the notice the subject: no `você`,
  no future tense.
