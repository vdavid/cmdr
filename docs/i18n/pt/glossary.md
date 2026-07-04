# pt glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Cmdr's `pt` ships Brazilian, so mine
  `_ignored/i18n/pt-BR/` (the complete Brazilian set); the bare `_ignored/i18n/pt/` is European Portuguese, a variant
  trap. For how Apple, Microsoft, and GNOME/Xfce render the term and for similar sentences (recipes:
  `docs/i18n/reference-pile/how-to-mine.md`). Cite the source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

## Terms

Sourced from `_ignored/i18n/pt-BR/` (macOS Finder Tier 1, then Microsoft terminology). pt-BR throughout.

- file · **arquivo** · macOS Finder, MS terminology (402 hits) · confirmed
- folder · **pasta** · macOS Finder, MS terminology · confirmed
- trash · **Lixo** · macOS Finder ("Move to Trash"→"Mover para o Lixo", "Empty Trash"→"Esvaziar Lixo", "Trash"→"Lixo",
  verified 2026-06-21 key-based EN→pt-BR in `LocalizableMerged.json`) · confirmed. Cmdr is a macOS app, so the Tier-1
  Finder value "Lixo" wins over the generic-pt-BR "Lixeira" the style.md table suggested. Used in
  `errors.write.trashNotSupported.*` and the diskFull/storageFull "empty the Trash" bullets.
- pane · **painel** · standard pt-BR UI · high
- tab · **aba** · pt-BR convention · high
- name · **Nome** · macOS Finder · confirmed
- size · **Tamanho** · macOS Finder · confirmed
- modified · **Modificado** · macOS Finder · confirmed
- created · **Criado** · macOS Finder · confirmed
- read-only · **Somente leitura** · macOS Finder · confirmed
- empty (folder) · **Pasta vazia** (empty: **Vazio/Vazia**) · macOS Finder · confirmed
- eject · **Ejetar** · macOS Finder · confirmed
- Cancel · **Cancelar** · macOS Finder (21 hits) · confirmed
- Try again / Retry · **Tentar novamente** · macOS Finder · confirmed
- Refresh · **Atualizar** · macOS Finder, MS · confirmed
- Back · **Voltar** · macOS Finder · confirmed
- Connect · **Conectar** · macOS Finder ConnectToWindow · confirmed
- Connect to server · **Conectar ao servidor** · macOS Finder ("Conectar ao Servidor", title case there; sentence case
  here per Cmdr style) · confirmed
- Server address · **Endereço do servidor** · macOS Finder · confirmed
- Sign in · **Iniciar sessão** · macOS Finder AFPUserGroupSheet · confirmed
- Username · **Nome de usuário** · pt-BR standard · high
- Password · **Senha** · macOS Finder · confirmed
- Guest · **Convidado** · macOS Finder · confirmed
- share (network) · **compartilhamento** · macOS Finder, MS terminology · confirmed
- mount · **montar** · macOS Finder, MS · confirmed
- hostname · **nome do host** · MS terminology · high
- IP address · **Endereço IP** · standard · high
- Keychain · **Acesso às Chaves** · macOS Portuguese (Brazilian) · high · localized Apple feature name (the Keychain
  Access app / credential store); not on the don't-translate brand list. The local Finder/SystemSettings pile doesn't
  capture the Keychain Access bundle, so this is from Apple's macOS pt-BR localization, not the mined pile.
- Favorites · **Favoritos** · macOS Finder · confirmed
- Network · **Rede** · macOS Finder · confirmed
- Volumes · **Volumes** · macOS Finder · high
- Cloud · **Nuvem** · standard pt-BR · high
- Mobile · **Dispositivos móveis** · standard pt-BR · high
- Disconnect · **Desconectar** · standard pt-BR · high
- Indexing · **Indexação** / index: **índice**; to index: **indexar** · standard tech pt-BR · high
- drive / disk · **disco** · macOS Finder (file-manager context; MS "unidade" not used) · high
- column · **coluna** · macOS Finder · confirmed
- sort / sort by · **ordenar** / **ordenar por** · macOS Finder MenuBar ("Ordenar por") · confirmed
- search (Settings context) · **busca** / to search: **buscar** · macOS Finder · confirmed
- Settings (the app's section) · **Ajustes** · macOS pt-BR ("Ajustes do Sistema") · high
- System Settings (macOS) · **Ajustes do Sistema** · macOS SystemSettings CFBundleName · confirmed
- Appearance (macOS pane) · **Aparência** · macOS SystemSettings · confirmed
- Privacy &amp; Security (macOS pane) · **Privacidade e Segurança** · macOS SystemSettings PRIVACY_SECTION · confirmed
- Local Network (macOS permission) · **Rede Local** · macOS-localized permission name (Network→Rede) · high
- Full Disk Access (macOS permission) · **Acesso Total ao Disco** · macOS-localized permission name · high
- default (value) · **padrão** · macOS/MS standard · high
- threshold · **limite** · MS terminology · high
- buffer · **buffer** · MS terminology (kept verbatim) · high
- word wrap · **quebra de linha** · MS "quebra automática de linha", shortened for toggle · high
- toast (transient notification) · **notificação** (running text) · rendered descriptively · tentative
- shortcut (keyboard) · **atalho** · macOS standard · high
- timeout · **tempo limite** · standard pt-BR tech · high
- connection · **conexão** · macOS Finder ("Stop connecting"→"Parar conexão") · confirmed
- permission · **permissão** · macOS Finder ("You don't have permission to…"→"Você não tem permissão para…") · confirmed
- Get Info · **Obter Informações** · macOS Finder · confirmed (errors.write permissionDenied/fileLocked suggestions)
- Activity Monitor · **Monitor de Atividade** · standard macOS app name · high
- Disk Utility / First Aid · **Utilitário de Disco / Primeiros Socorros** · standard macOS app/feature names · high
- Login Items &amp; Extensions (pane) · **Itens de Início e Extensões** · inferred from macOS conventions (not directly
  value-mined); review · tentative
- search / to search · **busca** / **buscar** · macOS Finder MenuBar ("Buscar", "Buscar por Nome…"); for queryUi search
  dialog and `commands.searchOpen` · confirmed
- copy / paste / cut · **Copiar** / **Colar** / **Recortar** · macOS Finder MenuBar (157/300847; Finder uses "Cortar" in
  some menus but **Recortar** is the standard clipboard verb, MS) · high. clipboard = **área de transferência** (macOS
  "Área de Transferência")
- rename · **Renomear** · macOS Finder MenuBar (OPI-Bm-bCw) · confirmed
- select all / deselect all · **Selecionar tudo** / **Desmarcar tudo** · macOS Finder MenuBar (172/300488) · confirmed
- delete (to trash) / delete permanently · **Apagar** / **Apagar permanentemente** · macOS Finder term (replaces the
  earlier Windows-influenced "Excluir"; macOS pt-BR Finder uses "Apagar", 0 "Excluir") · high
- Show in Finder · **Mostrar no Finder** · macOS Finder (A34, N207) · confirmed
- Quick Look (mac) / Preview (other) · **Visualização rápida** / **Pré-visualizar** · macOS Finder MenuBar
  ("Visualização Rápida", 300780) · confirmed. Localized Apple feature name: use the term the user sees in their pt-BR
  Finder, never the English "Quick Look".
- New folder / New tab / New window · **Nova pasta** / **Nova aba** / **Nova janela** · macOS Finder MenuBar
  (300797/300913/kZ0-FG-6vN) · confirmed
- hidden files · **arquivos ocultos** · macOS Finder ("oculto"), Nautilus ("arquivos ocultos") · confirmed
- Quit (app) · **Encerrar Cmdr** · macOS Finder "Encerrar Finder" pattern · high
- About (app) · **Sobre o Cmdr** · macOS Finder "Sobre o Finder" pattern · confirmed
- zoom in / out / reset · **Aumentar zoom** / **Reduzir zoom** / **redefinir o zoom** · standard pt-BR; macOS Finder
  uses "Aumentar/Diminuir Tamanho do Ícone" but **zoom** is kept for the UI-scale feature · high
- command palette · **paleta de comandos** · standard pt-BR app term · high
- onboarding · **introdução** (wizard: **assistente de introdução**) · standard pt-BR · high
- What's new · **Novidades** · standard pt-BR app term · high
- offline / online · **offline** (kept) / **on-line** · MS terminology keeps "offline"; "on-line" hyphenated per pt-BR ·
  high
- host (network) · **host** · MS terminology (kept verbatim) · high
- glob · **Glob** (kept verbatim) · technical term, no common pt equivalent · high
- regex · **Regex** (kept verbatim) · technical term · confirmed
- view mode: Brief / Full · **visualização resumida** / **visualização completa** · descriptive (Cmdr's own view names;
  no direct macOS source) · tentative
- View (menu name) · **Visualizar** · used in `commands.handler.zoomResetHintMenu` menu path · tentative
- verify / check (in progress) · **Verificar** / **Verificando** · macOS Finder ("Verifying"); used for
  license/conflict/key checks (`licensing.dialog.checking`, `fileOperations.transferDialog.checkingConflicts`,
  `onboarding.cloudSetup.status.checking`) · high
- symlink · **link simbólico** · standard tech pt-BR; distinct from Finder's "atalho" (which is an alias).
  `fileOperations.delete.symlinkNotice*` · high
- Replace (conflict policy) · **Substituir** · macOS Finder conflict sheet ("Substituir") · confirmed
- Skip (conflict policy) · **Ignorar** · macOS Finder ("Ignorar") · high
- Rollback (transfer) · **Reverter** · standard pt-BR · high
- Empty (trash) · **Esvaziar** · macOS Finder ("Esvaziar Lixo") · confirmed
- Move · **Mover** · macOS Finder · confirmed
- download (verb) · **Baixar** / **Baixando** · MS, standard pt-BR. The Downloads folder name stays **Downloads** (macOS
  pt-BR keeps it; `settings.fileSystemWatching.cardDownloads`) · high
- upgrade (page/CTA) · **upgrade** (kept verbatim) · naturalized pt-BR tech usage; `commands.aboutOpenUpgrade.label`
  "Abrir página de upgrade" · high
- server · **Servidor** · macOS Finder ("Conectar ao Servidor") · confirmed
- provider (AI / cloud) · **provedor** · standard pt-BR · high
- endpoint · **Endpoint** (kept verbatim) · matches Apple pt-BR usage; `ai.cloud.endpointLabel` · high
- remaining · **restante** · standard pt-BR (AI download progress) · high
- memory (RAM) · **memória** · standard · confirmed
- path · **caminho** · macOS Finder; `goToPath.*` · high
- changelog · **registro de alterações** · standard pt-BR; `whatsNew.dialog.seeFullChangelog` · high
- crash report · **relatório de falha** · macOS pt-BR convention; `crashReporter.*` · high
- error report · **relatório de problema** · avoids the banned bare "erro"; calm and consistent; `errorReporter.*` ·
  high
- Force Quit · **Forçar Encerramento** · macOS pt-BR · high
- status · **Status** (kept verbatim) · naturalized in pt-BR tech UI; used consistently across pt
  (`licensing.section.labelStatus`, `fileExplorer.network.browser.colStatus`, `ai.local.status*`) · high
- Ext / DIR (column tags) · **Ext** / **DIR** (kept verbatim) · short column-header abbreviations; pt-BR keeps these
  terse tags (matches es); `fileExplorer.columns.ext`, `fileExplorer.selectionInfo.dir` · high
- pause (transfer) · **Pausar** (verb) / **Pausado** (status) · MS terminology (Pause→"Pausar"), Total Commander pt-BR
  (`2094="Pausar"`), Double Commander pt-BR ("Paused"→"Pausado", "Pausing"→"Pausando") · confirmed. `queue.json` +
  `fileOperations.transferProgress.pause/titlePaused`
- resume (transfer) · **Retomar** · MS terminology (resume→"retomar", ids 639983/1262427) · high. Pairs with Pausar;
  Double Commander uses generic "&Continuar" for a continue button, but MS's transfer-sense "retomar" fits the
  pause/resume toggle better. `queue.json` + `fileOperations.transferProgress.resume`
- queue (transfers) · **Fila** (noun) · macOS-adjacent file managers: Total Commander pt-BR (`4005="&Fila"`, "Download
  em fila"), Double Commander pt-BR ("Queue"→"Fila", "Add to queue"→"Adicionar à fila"), MS terminology (Queue→"Fila") ·
  confirmed. Window title "Transfer queue"→"Fila de transferências"; `queue.*`, `commands.queueShow.*`,
  `fileOperations.transferProgress.queue`
- waiting / queued (status) · **Aguardando** · Double Commander pt-BR ("Aguardando acesso à origem do arquivo",
  "Aguardando resposta do usuário") · high. The queued/waiting row status and the "waiting its turn" toast
- background / send to background (running transfer) · **segundo plano** / **em segundo plano** · Total Commander pt-BR
  (`1185="Download em segundo plano (fila separada)"`, "Work in background"→"em segundo plano") · confirmed. Process
  sense, NOT MS's wallpaper-sense "tela de fundo". `fileOperations.transferProgress.queueTooltip/backgroundedToast`
- double-click · noun **clique duplo**, verb **clicar duas vezes** / imperative **Clique duas vezes** · shipped pt-BR
  catalog: network-browser tooltips use the verb ("Double-click to connect…"→"Clique duas vezes para conectar…",
  `fileExplorer.network.browser.tooltip.doubleClickToConnect/credsStored/requiresLogin`); the viewer body uses the noun
  ("double-click the file"→"dê um duplo clique no arquivo", `viewer.binaryWarning.body`) · confirmed. Use the noun
  "clique duplo" in labels/titles, the verb form in running text.
- parent folder (navigation sense) · **pasta superior** · `commands.navParent.label` "Go to parent folder"→"Ir para a
  pasta superior" (the navigate-up action) · confirmed. Use **pasta superior** for the go-up navigation concept;
  `errors.json` uses "pasta principal" in error suggestions, but the navigation action is consistently "pasta superior".
  Note: external pile evidence actually favors **pasta pai** (MS terminology BRA-tagged; GNOME Nautilus "Parent
  folder"→"Pasta pai"; Xfce Thunar alt; macOS Finder's nearest is the context-bound "Ir para a Pasta Original"). We keep
  **pasta superior** anyway for catalog consistency — switching would fork terminology (menu "pasta superior" vs new
  settings/toast "pasta pai") and needs a full-catalog migration, not a piecemeal change. Used in the
  doubleClickPaneNavigatesToParent settings + `doubleClickHint` body.
- navigate (verb) · **navegar** · MS terminology (BRA); rendered "navegar até {path}" in
  `fileExplorer.breadcrumb.navigateTooltip` · high
- pane background (empty backing area of a pane) · **fundo do painel** (the empty space: **espaço vazio**) ·
  descriptive; no direct pile source (Double Commander's "empty part of file view" is untranslated in pt-BR). MS's "tela
  de fundo" (wallpaper) and "segundo plano" (process) are wrong senses; "fundo do painel" reads naturally · tentative
- hint (one-time tip) · **dica** · Total Commander pt-BR ("DICA:"); `doubleClickHint.*` and the seen-flag settings ·
  high
- row / file row · **linha** ("file row" → **linha de arquivo**) · MS terminology (BRA "row"→"linha"), Xfce Thunar ("by
  one row"→"uma linha") · high. Used in `doubleClickPaneNavigatesToParent.description` ("not a file row"→"não uma linha
  de arquivo") to contrast the pane background with a clickable file row.
- too large (for destination) · **muito grande** ("File too large for this drive"→"Arquivo muito grande para este
  disco"; plural "muito grandes") · GNOME Nautilus pt-BR ("File too Large for Destination"→"Arquivo muito grande para
  destino"), and "muito grande" outnumbers "grande demais" 10:1 in the pile · high. Used in
  `errors.write.filesTooLargeForFilesystem.*`.
- larger than (size comparison) · **maior(es) que** · GNOME Nautilus pt-BR ("Files bigger than 4.3 GB cannot be copied
  onto a FAT filesystem."→"Arquivos maiores que 4,3 GB não podem ser copiados num sistema de arquivos FAT.") · high
- formatted as (filesystem) · **formatado como** · standard pt-BR; macOS Disk Utility uses the noun "Formato"/"Formato:"
  for the format field; the verb phrase "formatado como FAT32" is the natural rendering · high.
  `errors.write.filesTooLargeForFilesystem.message.*`
- store (files) · **armazenar** · macOS Finder ("Store your Desktop & Documents folders…"→"Armazene as pastas…") · high.
  Used for "can't store files larger than" → "não pode armazenar arquivos maiores que".
- FAT32 / exFAT (filesystem formats) · **FAT32** / **exFAT** (kept verbatim) · macOS Finder + MS terminology both keep
  them verbatim (MS tbx term ids 153889/153903 = "FAT32"; Finder "ExFAT") · confirmed. Don't translate; source EN
  capitalization ("FAT32", "exFAT") is preserved.

### Reconciliation notes

- **delete = Apagar (macOS Finder term).** The file-delete action/command is **Apagar** / **Apagar permanentemente**
  across `fileOperations.json`, `commands.json`, `fileExplorer.json`, and the `transferDialog` `select`
  `delete {Apagar}` branch, matching macOS pt-BR Finder. "Mover para o Lixo" stays for the trash variant. Don't
  reintroduce the Windows-influenced "Excluir" for the delete action. Two non-action senses correctly keep "excluir":
  query-scope **exclude** (`queryUi.scope.hint`, filter-out, not delete) and the AI-model deletion in `ai.json`
  (separate domain). "apagar a senha" (clearing a credential, `fileExplorer.network.deletePasswordFailed`) is a
  different sense, already correct.

### Error-copy phrasings (errors.json, for cross-file consistency)

- "Here's what to try:" · **"Veja o que tentar:"**
- "Navigate here again to retry." · **"Navegue até aqui de novo para tentar outra vez."**
- "couldn't / failed" titles · never a bare "Erro/Falhou"; use **"Não foi possível …"** or **"A operação de {Verb} não
  foi possível"** (no-bare-error voice rule)

### UI section names (for cross-file consistency)

- Function keys (bottom bar) · **Teclas de função**
- File list · **Lista de arquivos**
- Volume switcher · **alternador de volumes** (running text)
- Settings sections (settings.json): Appearance→**Aparência**, Behavior→**Comportamento**, File operations→**Operações
  de arquivo**, File systems→**Sistemas de arquivos**, Search→**Busca**, Viewer→**Visualizador**,
  Developer→**Desenvolvedor**, Advanced→**Avançado**, License→**Licença**, Keyboard shortcuts→**Atalhos de teclado**,
  Updates &amp; privacy→**Atualizações e privacidade**, Logging→**Registros**, Listing→**Listagem**, Colors and
  formats→**Cores e formatos**, Zoom and density→**Zoom e densidade**, File and folder sizes→**Tamanhos de arquivos e
  pastas**
- preset (value in a settings-picker dropdown) → predefinição; "back to presets" → "Voltar às predefinições" · Microsoft
  terminology pt-BR ("indexing preset" → "predefinição da indexação"), macOS pt-BR print dialog "Predefinições" · high
- scan / scanning (counting/sizing items before a transfer or delete) · **Analisar** / **Analisando** (in progress);
  scan complete → **Análise concluída** · matches the shipped `fileOperations.transferProgress.stageScanning`
  ("Scanning" → "Analisando"); "concluída" is the macOS Finder term for complete/concluded ("não pode ser concluída",
  "Download concluído"). Used in the shared `fileOperations.shared.scanningTooltip` / `scanCompleteTooltip`
  spinner+checkmark. · high. Distinct from the conflict-check sense, which stays **Verificando** (see verify/check).
- Action (field label) · **Ação** · macOS Finder (6 hits), MS terminology (BRA) · confirmed.
  `fileOperations.shared.actionLabel` "Action:" → "Ação:" (label before the Copy/Move or Trash/Delete segmented
  control).
- Route (transfer source→destination field label) · **Rota** · MS terminology (BRA, route→rota); no macOS/file-manager
  source (Finder has no such label) · high. `fileOperations.transferDialog.routeLabel` "Route:" → "Rota:" before the
  "source → destination" line. The word is the direct pt-BR cognate and reads as a compact label; the UI usage itself is
  Cmdr-specific.
- preset (value in a settings-picker dropdown) → predefinição; "back to presets" → "Voltar às predefinições" · Microsoft
  terminology pt-BR ("indexing preset" → "predefinição da indexação"), macOS pt-BR print dialog "Predefinições" · high
- "doesn't exist yet" (destination not-yet-created warning) · **ainda não existe** · standard pt-BR; pile has "A pasta
  de destino não existe!" (file-manager) and "não existe. Deseja criá-lo?" · high.
  `fileOperations.transferDialog.targetWillBeCreated{Copy,Move}`
- "will create it during the copy/move" (auto-create reassurance) · **vai criá-la durante a {cópia/movimentação}** ·
  subject is **O Cmdr** (running-text pattern across the pt catalog, e.g. "O Cmdr cuida da cópia automaticamente"); copy
  noun = **cópia**, move noun = **movimentação** (matches `transferProgress.rollbackUnavailableTooltip` "movimentações
  no mesmo volume") · high. The two keys stay literal (operation-specific noun), no ICU select.
- **queue.row.label progress arms (rename / create folder / create file)** · `Renomeando` / `Criando pasta` /
  `Criando arquivo` · pt-BR gerund style of the sibling arms (NOT the pt-PT "A criar"/"A mudar o nome" Nautilus shows);
  settled `Renomear`→gerund, `pasta`/`arquivo` · high
