# pt decisions

The non-obvious why behind `terms.json` rulings. Forms: `terms.json`; voice: `style.md`; open questions:
`review-queue.md`.

## The bare `pt` pile is European (`settings.archives.compressionLevel.description`, `fileOperations.transferDialog.compressLevelCaption`)

- Mining `_ignored/i18n/pt/` once shipped `ficheiros` and `demoram mais tempo a comprimir`. Re-check rulings citing it.

## Apagar, never Excluir (`fileOperations.delete.*`, `commands.fileDelete*`, `ai.local.delete*`, `settings.mediaIndex.*`, `transfer.delete`)

- Finder pt-BR has zero `Excluir`; a files-vs-data split once put `Excluir modelo de IA?` over `Apagar`. The noun
  `apagamento` (`queue.empty.body`) keeps `exclusão` out.

## `drive` is always `disco` (`errors.listing.*`, `errors.provider.pCloudFuse.*`, `fileExplorer.unreachable.*`)

- Finder says `Discos externos`; its `unidade` is only the physical mechanism. `disco virtual do pCloud`: a common noun
  after the brand.

## Parent folder (`commands.navParent.label`, `settings.behavior.doubleClickPane*`, `fileExplorer.doubleClickHint.body`)

- `pasta superior` over the pile's `pasta pai`: switching is a whole-catalog migration or nothing (review queue).
  `pasta principal` means something else; a volume's top is `a pasta raiz de um volume`.

## Go to folder (`menu.go.goToPath`, `commands.navGoToPath.label`, `goToPath.dialog.*`)

- `Ir para pasta` (Finder N83) for menu and title alike, though Finder's dialog says `Ir Para a Pasta`: one command, one
  name. Go → `Ir` (`FR24`).

## Archives (`settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `fileOperations.archivePassword.*`)

- A double `arquivo` (file + `arquivo compactado`) reads fine; a named format is plain `arquivo zip`. `pathErrorNotZip`
  says `nome do arquivo compactado` so it doesn't ask for a file name.
- Browse → `Navegar`, beside `Abrir` in one segmented control. `settings.archives.ooxml.*` keeps bare `pacotes` so the
  row stays wider than the bundles card. Compress → `Comprimir`; level → `Nível de compressão`.

## Settings sections and transfer dialog (`settings.section.*`, `fileOperations.transferDialog.*`, `fileOperations.transferProgress.stall*`)

- `sourceGroupTitle`/`targetGroupTitle` → `De` / `Para` (Total and Double Commander pt-BR); `origem` / `destino` stay
  for the controls. Copy noun `cópia`, move noun `movimentação`.
- `Sem progresso há {duration}` (`há`, not `por` / `durante`). `parou de avançar` over `travou` (a crash) or bare
  `parou` (ended). `O arquivo de registro` keeps the log file apart from the `registro de operações` feature.

## Operation queue (`queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- `Fila de operações` because the queue also holds deletes, renames, and creations. It shares its head noun with
  `Registro de operações` in the View menu: rename both or neither.

## Queue chip, dismiss, and failure notices (`queue.row.dismiss*`, `queue.failureToast.*`, `queue.chip.*`)

- `Dispensar` over `Ignorar` (this catalog's Skip, two rows away) and `Descartar`.
- `Não foi possível concluir` + action noun (Finder); the `other` arm equals `queue.row.status`'s failed arm. In
  `failureToast.summary` it sits OUTSIDE the plural so only the noun branches.
- `queue.chip.ariaLabel` spells `por cento`; the tooltip keeps `{percentText}%`. Time left reuses
  `fileOperations.transferProgress.etaRemaining`.

## Background button (`fileOperations.transferProgress.background*`)

- `Em segundo plano` reads as a command; bare `Segundo plano` would title a section. Never `2º plano`.

## Quit gate (`main.quit.*`)

- `Continuar trabalhando` over `Cancelar` (would cancel the operations) and any postpone phrase (the countdown is gone).
- `em andamento` (Finder's quit sentence) over `em execução`. `é interrompido onde está` over `para onde está`, which
  garden-paths. The `encerr-` and `gravado` repeats are deliberate.

## Operation log (`operationLog.*`, `commands.logOperationLog.*`)

- log → `registro` everywhere (changelog, log files). `Revertida` (the operation) vs `Revertido` (an item) agree with
  different subjects; never unify. Status words copy `queue.row.status`.

## Rollback confirm and finish (`fileOperations.rollbackConfirm.*`, `operationLog.dialog.finishRollBack`, `queue.row.statusAwaitingAnswer`)

- `Precisa de resposta` over `Aguardando resposta`, which echoes the `queued` arm.
- `Manter os arquivos` over `Mantê-los`: the prior sentence names REPLACED files. `Concluir a reversão` pairs with
  `iniciar a reversão`; `Para concluir` reads as "in conclusion". `percorrer`, since `revisar` is Review. `em {folder}`.

## Rollback outcome notice (`fileOperations.cancelRollback.*`, `fileOperations.cancelRollback.stagedLeftover.*`, `fileOperations.leftovers.*`)

- Reason lines: `{name} ficou como está: <motivo>.` (from `askCmdr.renameUndo.skipReason.*`); `spotTaken` says
  `ficou onde está`. `mudou depois que o Cmdr colocou lá`: the null object dodges a gendered clitic.
- The three put-back verbs (`style.md`) beat the `@key` asking for one. `removed` and `deleted` both → `apagar`;
  `Remover` takes an entry off a list.
- `na próxima vez que se conectar lá`, never a time. `stagingFolderKept` guards the USER's files: never suggest deleting
  the folder. `deixou tudo onde está`; `no lugar` means "instead" here.

## Rename and create refusals (`errors.mutation.*`, `errors.volume.*`, `fileExplorer.rename.*`, `fileOperations.newEntry.*`, `fileOperations.validation.nameNotUsable`)

- `deviceSessionReset` isn't a disconnect: never `desconecte`. `perdeu a referência`, never `perdeu o controle`.
  `O macOS se recusou a mover` over `não permitiu` (reads as permissions). `a alteração` (the request) vs `as mudanças`
  (in the file system).
- `e outros {N} itens`, Finder's order (`MR201_V3`). Still-running toasts take the act as subject (`A criação de “X”`):
  X may be a folder. `nameNotUsable` has no final period, as it's composed into `{reason}`.

## Eject and disconnect refusals (`errors.eject.*`)

- Each value follows a colon in `fileExplorer.pane.ejectFailedToast` / `disconnectFailedToast`: a full, capitalized
  sentence. Either may concern a share or a phone, so the frame is
  `<sujeito> ainda está usando algo lá. <ação>, depois tente de novo.`, never `este disco` or `ejete-o`.
- `{app}` opens bare (`mds_stores` isn't an app); `Feche o app` / `esses apps`. Eject all → `Ejetar tudo` (Finder
  `N199.1`, in sentence case). A disk image `ainda está montada`.

## Trash buttons and refusals (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `errors.write.trashRefused.*`, `fileOperations.delete.cloudOnlineOnly*`)

- `colocar de volta` is Finder `N153.1`; `Este disco não tem Lixo.` states a fact.
- `trashRefused`: `{count} dos itens que você escolheu` needs no plural; never suggest retrying.
  `Desfazer mover para o Lixo` in command and menu (Finder `ME14`, `RN2`, `MT9`).
- Four mandatory facts: the Trash would download first, so only deleting the WHOLE selection is offered, no copy stays
  in the Trash (don't soften it), and the ways out. `“Apagar”` equals `fileOperations.delete.confirmDelete`.

## Crash dialog (`crashReporter.dialog.*`, `settings.updates.crashReports.*`)

- `continuou funcionando` over `continuou rodando`, which echoes `rodando em segundo plano`. `unknown` says neither
  `encerrou` nor `continuou`. The settings label keeps `Enviar relatórios de falha` (a name).

## Report notes (`errorReporter.amend*`, `errorReporter.autoSentToast.*`)

- `nota` over MS `observação`: the two dialogs share one box. View buttons → `Ver X` (`Visualizar` is the menu);
  `Ver por quê` (`askCmdr.wakeToast.openThread`), since bare `porquê` would need an article.

## Selection dialog (`selection.*`, `menu.select.*`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `Desmarcar` (Finder `Desmarcar Tudo`), never pt-PT `Desselecionar`, identical across menu, palette, settings, and
  title. Hints start with the button's exact text.
- kind → `Tipo` (Finder). `menu.context.selection` is the noun `Seleção`. `com a extensão *.{extension}`.

## One thing, one name (`queryUi.*`, `viewer.search.*`, `viewer.saveAs.*`)

- Reset → `Restaurar` (Finder), `redefinir` only for zoom; error report → `relatório de problema`; `dir` → `pasta`.
- Agreement isn't drift: keep `Ambos` / `Ambas`, `Modificado` / `Modificados`. `Pré-visualização` vs `Pré-visualizar`,
  `Busca` (a section) vs `Buscar`. `Em execução` (a server) vs `Em andamento` (a task); `memória` (RAM) vs `anotações`
  (Ask Cmdr's memory). ❌ Never sweep `de novo` → `novamente`.
- `viewer.saveAs.defaultName` is `selecao` without the cedilla: a default file name.

## Native menus (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Cut → `Cortar` in `menu.edit.cut` (Finder's menu bar), `Recortar` elsewhere (MS, Finder `QK5`); review queue.
  Changelog → `Registro de alterações` over MS `Log de alterações`. `Remover` a tag: it deletes nothing.

## Dock icon menu (`menu.dock.*`)

- Source: Dock.app `pt_BR.lproj/DockMenus.strings` (not in the pile). `menu.dock.searchFiles` equals
  `menu.edit.searchFiles`.

## Dock pin nudge (`main.dockPinNudge.*`, `settings.behavior.dockPin*`)

- The accept button copies Apple's `Adicionar ao Dock`, not `fixar`. `Não precisa` also beats `Não, valeu` (slangy).
  `addedButDockDidNotRestart` says the icon IS in place. `há alguns dias`: the threshold can change.

## Show in Finder offer (`main.reveal*`, `settings.behavior.reveal*`)

- `“Mostrar no Finder”` quoted, identical wherever named. `há um tempo`, never a number; explain, never apologize.

## macOS pane names, Apple menu items, and the example email (`settings.updates.emailPlaceholder`, `askCmdr.renameUndo.*`, `menu.app.*`)

- From the macOS bundles: `Conta Apple` (not `da Apple`), `Geral`, `Itens de Início de Sessão e Extensões`.
- `askCmdr.renameUndo.undone` names the object (`O nome anterior de … foi restaurado`), whole sentence in the plural.

## Ask Cmdr names only the chat panel (`askCmdr.wake*`, `settings.askCmdr.*`, `ai.cloudConsent.askCmdr.*`)

- `chat`, `conversar`, `conversa` each follow their own key's English.

## Bulk-rename review (`askCmdr.renameReview.*`, `askCmdr.tool.proposeRenamePlan.*`)

- `Permitir` / `Negar` over `Recusar`: an approval gate. `Permitir tudo` dodges agreement with `renomeações`.
- Overwrite badge `(substituição!)`, a noun like its siblings. `trocar os nomes desses arquivos entre si`: `girar` reads
  as rotating images.

## What Ask Cmdr reads inside files (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.*`)

- `detalhes da câmera`, never `metadados` / `dados EXIF`. `onde ela foi tirada`, since `localização` alone reads as the
  path.

## Tool lines (`askCmdr.tool.*`, `askCmdr.toolGroup.*`, `askCmdr.turnStats.*`)

- Bare third-person past (`Listou`, `Buscou`) like the shipped siblings; notes with no line subject name `a IA` or
  `O Cmdr`, never a pronoun. `{countText} para revisar` dodges agreement; `Interrompida` agrees with `a resposta`.

## Cloud AI consent (`ai.cloudConsent.*`, `askCmdr.gate.*`)

- `Ative “Permitir IA na nuvem”` reproduces the label; `Permita a IA` wouldn't. `serviço` (this family) vs `provedor`
  (`ai.cloudConsent.askCmdr.*`) follow the English.

## AI provider setup (`onboarding.cloudSetup.*`)

- endpoint → `ponto de extremidade` (MS; Apple is silent); label and caption changed together. `e depois` keeps the
  order.

## Image index (`fileExplorer.imageIndex.*`, `settings.mediaIndex.*`, `search.imageResults.*`, `askCmdr.tool.imageFacts*`)

- `busca de imagens` / `busca por descrição`; `pesquisável` stays an item's adjective. badge → `selo` over `distintivo`
  (police) and `emblema` (heraldry).
- Tooltips agree with `imagem` (`Indexada`). A pass → `rodada`. Junk → `descartáveis` (`Lixo` is the Trash).
  `deixou de acompanhar`. `fotos` vs `imagens` follow the English; a photo archive is `acervo de fotos`.

## Drive scan (`indexing.*`, `fileExplorer.navigation.driveIndex.*`, `settings.indexing.*`, `errors.write.deviceDisconnected.*`/`moveNotConfirmed.*`)

- `Primeira varredura completa`, `Nova varredura completa`; `Verificação de mudanças` matches the siblings' noun shape.
- `Desativado com a indexação` stays short: it renders only under that toggle.
- Never mention disconnecting: the phone just never reports changes. `atualizado` pairs with `desatualizado`.
  `os arquivos do celular` over `dele`, which would agree with `{name}`.
- `O disco {name} está sendo / foi desconectado`; `no disco {volumeName}`, but `para {counterpart}` bare.
- Sided lines say `disco`, generic ones `dispositivo` (phones and servers too). Don't unify.
- `moveNotConfirmed` is neither failure nor loss: `Dê uma olhada em`; `então ele manteve…`.

## Server pane, sign in, and forget (`servers.*`, `fileExplorer.navigation.connection*`, `fileExplorer.network.*`)

- `iniciar sessão` (`iniciar a sessão` in prose). Disconnect a server, never `Ejetar`.
- `authMethodUnsupported`: `O Cmdr` skips a clumsy `a que`. The forget/remove confirms write the noun
  (`tira esse servidor` / `compartilhamento` / `host`, `a senha`), never a pronoun on `{name}`.
- `como {username}` / `como convidado`; the button is `Usar como convidado`. `notConnected` → `Não conectado a {name}`
  (review queue).

## Server hub (`servers.hub.*`, `commands.servers*`, `shortcuts.scope.*`)

- `Status` over Apple's `Estado`, matching `licensing.section.labelStatus` (review queue). nearby → `por perto`;
  `Próximo` means "next". `local network` lowercase; `Rede Local` names the macOS permission.

## Server sheet and host keys (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.*`, `goToPath.dialog.*`)

- Browse (file picker) → `Escolher…` (Apple) over `Navegar` (archives), `Explorar`, MS `Procurar…`; allowlisted in
  `i18n-term-consistency`. passphrase → `frase-senha` (Apple), apart from the account `senha`.
- `quem cuida do servidor` avoids `o dono`; `A chave do servidor mudou` because `dele` would lean on `{name}`.

### NFS exports (`servers.sheet.nfs*`, `servers.refusal.export*`/`reservedPortRequired`)

- export → `exportação` (IBM, Oracle pt-BR) over `compartilhamento`, which stays SMB's. The owner lines say
  `a quem pertencem os arquivos`, dodging `dono`. The server's own settings are `configurações de NFS do servidor`.

## Reconnect and key-only sign-in (`servers.paneState.*`)

- `Abra-o de novo` is safe: `servidor` is the only masculine candidate.

## Pin, unpin, and trusted host keys (`menu.network.*`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`)

- `Fixar` / `Desafixar` (Safari, Notes). `Confiável desde`: `confiar em` doesn't passivize.
  `Seu grupo Rede está ficando grande` (`grupo longo` isn't Portuguese). ADB status lines write `O Cmdr`.

## ADB settings (`settings.fileOperations.adb*`)

- `Localização do adb` (Finder), `sistema de arquivos` (Disk Utility), `por ADB` like `por USB`; `adb` lowercase (the
  command).

## Android over ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

- `Saiba como` over bare `Como`, which doesn't read as a link. `Desconectar`, never `Ejetar`: the phone stays cabled.
  `Você parou de abrir o seu celular` over `cancelou` (`Cancelar` is the button).

## Root and start folder (`servers.sheet.rootFolder*`/`startFolder*`, `servers.refusal.*Folder*`/`rootNotFound`)

- `pasta raiz` (never `raíz`), `pasta inicial`; `pasta pessoal` / `pasta base` are the home folder. `nunca sobe além`
  avoids `sobe acima`.

## Onboarding (`onboarding.*`)

- `Rede Local` quoted as the macOS privacy row, never `Acesso à rede local`. `processo nativo do macOS` over MS
  `manipulador`, matching its caption.
- `stepAi.local.tooltip` quotes `stepAi.cloud.label` byte for byte, and the signup lines name `Salvar`: change together.
- `burro` for the deliberately blunt "dumber". `erro de digitação` is the person's typo.

## Full disk access badge (`onboarding.fdaBadge.*`)

- Sentence case `Sem acesso total ao disco`, equal to `onboarding.stepAi.bannerTitle.denied`; the aria opens with it.

## Favorites menu (`commands.favorites*`, `fileExplorer.navigation.favorites*`, `menu.go.showFavorites`)

- `Favoritos` capitalized only where the English names the section. `já está nos favoritos` over `já é um favorito`
  (clashes with `pasta`). `pressione`, never `aperte`; `ir até esse favorito`.

## Viewer fetch pane (`viewer.pull.*`, `viewer.error.*`)

- `Obtendo` (Finder) over `Baixando` (a local archive downloads nothing) and `Buscando` (search). Nothing agrees with
  `{fileName}` or the unit.

## Mount and share-list failures (`errors.mount.*`, `errors.shareList.*`, `fileExplorer.network.osMountFallback.*`, `fileOperations.transferProgress.lookAlikeHint`, `errors.*.ambiguousName*`)

- NetAuthAgent.app (not in the pile) is Tier 1: `compartilhamento`, `convidados`. `quando quiser` avoids
  `pronto/pronta`. `shareNotOnServer` can't be fixed by retrying: no `agora`, no `tente novamente`.
- Plain words (`parecem iguais`), no `Unicode` or `normalização`. `a pasta superior`; `pasta acima` reads as "listed
  above".

## F4 text editor (`settings.behavior.textEditorApp.*`, `fileExplorer.edit.*`)

- `Editar arquivos com` takes any app without `no` / `na`; `em {app}` bare.

## Old macOS and old WebKit (`main.oldMacos.*`, `main.oldWebkit.*`, `main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`, `settings.analytics.*`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analytics*`)

- `compatível` over `suportado`; best effort → `faz o que dá`. `Atualização de Software` keeps capitals.
- `Esc` (AppKit) over `ESC`. `Este aviso só aparece uma vez`: no `você`, no future.
- ❌ `anônimas` (a stable id), ❌ `pseudônimo`: `um identificador aleatório`, `ligado a`.

## O original ficou (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*`, `errors.write.deletePending.*`)

- `apague o original por conta própria`: `você mesmo` marks the masculine (`tentative`). `para ser apagado`.

## What your organization manages (MDM) (`*.managed.*`, `updates.status.*`)

- `O que sua organização gerencia`, `Só verificação manual`: no agreeing participle. `*Off` agrees with its row.

## Require SMB encryption (`settings.network.requireSmbEncryption.*`, `*encryptionRequired*`, `*EncryptionRequired*`)

- Label `Sempre exigir criptografia SMB` (`exige` is the catalog's verb); messages quote it as
  `o ajuste “…” está ativado`, so the participle agrees with `ajuste`. Managed value `Sempre exigida` (agrees with
  `Criptografia SMB`).
- `{provider}` busy lines name the type first (`O provedor {provider} está ocupado`): a brand's gender is unknown.

## AI suggestions (`suggestedOps.*`)

- `Sugestões da IA`, as `fileOperations.mkdir.aiSuggestions*`. `Por quê:` (tonic, as `Por quê?`); aria
  `Por quê, segundo a IA:`.
