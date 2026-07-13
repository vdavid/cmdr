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

### Paste-clipboard-as-file terms (paste-as-file feature)

Cmdr can paste non-file clipboard content (text, an image, a PDF) into the current folder as a new file; this batch
added the setting and the confirmation toast.

- paste (verb) · **Colar** · macOS Finder (`N49_V1`/`ME3` "Paste" → "Colar", key-based EN→pt-BR) · confirmed. Reuses the
  glossary copy/paste/cut row; clipboard = **área de transferência**.
- Do nothing (behavior option) · **Não fazer nada** · standard pt-BR option label; no direct pile source (file managers
  don't carry it) · high. Radio-button label in `settings...pasteClipboardAsFile.opt.doNothing`.
- Create file / Create and rename (behavior options) · **Criar arquivo** / **Criar e renomear** · shipped pt catalog
  ("Criar arquivo em…", "Criar novo arquivo") + rename→**Renomear** (glossary) · high.
  `settings...pasteClipboardAsFile.opt.createFile/createFileAndRename`.
- "Pasted clipboard {image/PDF/text} as {filename}" (toast) · **{kind, select, image {Imagem colada} pdf {PDF colado}
  other {Texto colado}} da área de transferência como {filename}** · the participle (colada/colado) is placed inside
  each select branch so it agrees with the noun's gender, keeping `{filename}` a gender-agnostic uncontrolled insert ·
  high. `fileExplorer.clipboard.pastedAsFile`. The toast's Settings button (`pastedAsFileSettings`) → **Ajustes**
  (glossary Settings-section term).

### Archive-browsing terms (archive-browsing feature)

Cmdr browses zip/tar/7z archives like folders; this batch added the settings, menu, error, and warning strings for it.

- archive (a zip/tar/7z browsed like a folder) · **arquivo compactado** · Total Commander pt-BR (Cmdr's two-pane
  lineage; keys 98-190 render the archive as "arquivo compactado", e.g. 160 "Esta função não pode ser usada com arquivos
  compactados!", 165 "Erro no arquivo compactado"), macOS Finder ("Arquivo comprimido"/"Arquivo compactado"), AND
  already used in the shipped pt catalog (`settings...zoomResetHint`-adjacent viewer setting: "imagem, PDF, arquivo
  compactado ou outro arquivo binário") · high. Covers zip/tar/7z generically. Note the unavoidable double-"arquivo"
  when "file" (arquivo) and "archive" (arquivo compactado) co-occur in one sentence — reads naturally, kept. Used across
  `settings.archives.*`, `fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.archive*`,
  `fileExplorer.archive.useTransferToCopyOut`, `fileOperations.delete.archiveWarning*`,
  `errors.listing.archiveUnreadable.*`, `viewer.error.archive*`, and the `queue.row.label` `archive_edit` arm.
- app bundle / bundle (macOS .app/.bundle/.framework) · **pacote de aplicativo** (generic bundle: **pacote**) · macOS
  Finder ("Mostrar Conteúdo do Pacote" = Show Package Contents → bundle = pacote), MS terminology ("pacote de
  aplicativo") · high. Plural card/label "App bundles" → "Pacotes de aplicativo". `settings.archives.card.bundles`,
  `settings.archives.bundle.label`, and the `archiveEnterMenu.ariaLabel` "ou pacote".
- browse (step inside and list contents like a folder) · **Navegar** ("Browse like a folder" → "Navegar como uma pasta";
  segmented cell "Browse" → "Navegar") · macOS Finder VO ("Navegar em visualização por colunas"), Total Commander pt-BR
  hint 148 ("clicar duas vezes sobre o arquivo como em uma pasta, para mostrar seu conteúdo") · high. Distinct from
  "Abrir" (Open); the two are contrasting behaviors in the same segmented control, so they must differ.
- open (with default app) · **Abrir** / **Abrir no aplicativo padrão** · shipped pt catalog ("abrir arquivos no
  aplicativo padrão", `fileExplorer.quickLookHint.enterOpens`), macOS · confirmed. default app = **aplicativo padrão**.
- Ask (behavior option: ask each time) · **Perguntar** (segmented cell); "ask each time" (running text) → **perguntar a
  cada vez** · macOS ("Perguntar"), shipped pt catalog (`allowFileExtensionChanges.opt.ask` = "Sempre perguntar") · high
- extract (from an archive) · **extrair** (also **descompactar**) · Total Commander pt-BR ("extrair-los com F5",
  "Descompactar"), macOS · high. "browses and extracts" → "navega e extrai" (`fileExplorer.readOnly.archiveMessage`).
- damaged · **danificado** · macOS Finder (4 hits), TC ("está danificado") · high. encrypted · **criptografado** · macOS
  (6 hits) · confirmed. Used in the two archive-unreadable error/viewer strings.
- Enter (the Return/Enter key, in running text) · **Enter** (kept) · shipped pt catalog keeps "Enter" throughout
  ("Pressione Enter para buscar", "<runKey>Enter</runKey>") · confirmed. "What pressing Enter does" → "O que pressionar
  Enter faz"; the pt macOS pile localizes no distinct Return-key word here, so "Enter" stands.
- Editing archive (queue.row.label arm, changing a zip's entries) · **Editando arquivo compactado** · gerund matching
  the sibling arms (Copiando/Movendo/…) · high

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

### Archive-password dialog terms (encrypted-zip unlock modal, `fileOperations.archivePassword.*`, 2026-07-08)

- password-protected → `protegido por senha` · TC/DC pt-BR phrasing · high. Body: "… está protegido por senha."
- password (noun) → `Senha` · macOS/MS pt-BR · high.
- unlock (button + verb) → `Desbloquear` · macOS AppKit ("Desbloquear") · high. Verb form "desbloqueá-lo".
- archive (the `{name}` head / input label) → `arquivo compactado` · settled pt glossary · high. Input aria-label "Senha
  do arquivo compactado".

Settled while translating the Compress feature:

- compress (verb / control label) → `Comprimir` · pt Double Commander / Thunar / Nautilus ("Comprimir ficheiros", "A
  comprimir…"); Finder pt-BR has no `Finder/` dir in the pile, so the file-manager corpora carry it · high. Used for
  `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb branches.
- compressing (progress -ing form) → `Comprimindo` (pt-BR gerund, matching the sibling `Copiando`/`Movendo`) · high.
  `scanTitleCompress` = "Verificando antes de comprimir...".
- compressed (result toast) → `Comprimido` / plural `comprimidos` (past participle) · mirrors `transfer.split.clean`
  ("Copiado: {phrase}") and the `one`/`many`/`other` shape of `fileOnly.allDone` · high.
- replace (overwrite warning) → `substituí-lo` · Finder `Replace` → "Substituir" · high.
- archive (name) → `arquivo` (pt-BR for file; the zip is a file) · high. `.zip` in straight double quotes.
- compression level (slider label) → `Nível de compressão` · pt DC/Thunar `compressão` + `nível`; standard pt 7-Zip
  `Nível de compressão` · high. pt pile has no Total Commander. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Mais rápido` · pt comparative · high. Marks quicker packing, not app speed.
  `.faster`.
- smaller (slider high end, level 9) → `Menor` · pairs with `Mais rápido`; marks the smaller output file · high.
  `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

### Operation-log terms (Operation log dialog, `operationLog.*` + `commands.logOperationLog.*`, 2026-07-09)

- operation log → `Registro de operações` · "log" → **registro** across the pt catalog (changelog → "registro de
  alterações", `errorReporter.*` "arquivos de registro"); "operation" → operação · high. Used for
  `operationLog.dialog.title` and `commands.logOperationLog.label`.
- roll back / rollback (undo a logged operation) → `Reverter` (verb) / `Revertida` (operation, fem participle) /
  `Revertido` (per-item outcome, masc participle) / `Revertendo` (in progress) · glossary "Rollback (transfer) →
  Reverter", extended to the past participle agreeing with its subject (operação fem vs item masc) · high. "Can(’t) roll
  back" → "Pode / Não pode ser revertida" (the operation is the subject); "Partly rolled back" → "Parcialmente
  revertida"; `commands.logOperationLog.description` "roll them back" → "reverta-as".
- operation-summary verbs (past-tense log lines) → `Copiou` / `Moveu` / `Apagou` / `Renomeou` / `Criou` / `Comprimiu` /
  `Editou` / `Extraiu` · 3rd-person preterite (implied subject supplied by the initiator chip Você/Cliente de IA/Agente,
  all taking the same 3rd-person form) · high. delete = **Apagou** (matching the glossary `Apagar` delete term, not
  "Excluir"); trash arm keeps "para o Lixo"; folder/file/archive nouns per glossary (pasta / arquivo / arquivo
  compactado). `operationLog.summary.*`.
- initiator provenance chips → `Você` (You) / `Cliente de IA` (AI client) / `Agente` (Agent) · pt-BR user address (você)
  - AI → **IA** (`ai.json` throughout) · high. `operationLog.initiator.*`.
- lifecycle status → `Aguardando` (queued) / `Em andamento` (running) / `Concluído` (done) / `Não foi possível concluir`
  (didn’t finish) / `Cancelado` (canceled) · matched exactly to `queue.row.status` (queued/running/done/cancelled/failed
  arms) for cross-file consistency; "didn’t finish" avoids the banned bare "Falhou" · confirmed.
  `operationLog.status.*`.
- per-item outcome → `Concluído` (done) / `Ignorado` (skipped, glossary Skip → Ignorar) / `Não foi possível concluir`
  (didn’t finish) / `Revertido` (rolled back) · high. `operationLog.outcome.*`.

### Ask Cmdr terms (read-only AI chat rail, `askCmdr.*` + `settings.askCmdr.*` + `commands.askCmdrToggle.*`, 2026-07-13)

- chat (a conversation thread with the assistant) · **chat** (kept verbatim, masculine noun, plural **chats**) ·
  Microsoft terminology pt-BR (`instant messaging` → id 2046699 "chat", and a direct `chat`→`chat` entry, both
  BRA-tagged, masculine noun) · confirmed. Naturalized loanword in pt-BR tech UI (matches how Discord/Instagram render
  it in Brazilian Portuguese); distinct from **conversa** (used once in `askCmdr.consent.local` for the English source's
  own "conversation" variant, and in `bate-papo`-flavored running text elsewhere) — both source words appear in the
  English catalog and are translated as their closest pt-BR cognate/near-synonym.
- attach / attachment (a file or folder staged onto a chat message) · **anexar** (verb) / **anexo** (noun) · Microsoft
  terminology pt-BR (`attach`→`anexar`, id 16026 BRA; `attached file`→`anexo`, id 16077 BRA) · confirmed.
  `askCmdr.composer.dropHint` "Drop to attach"→"Solte para anexar" (**soltar** = drop, standard pt-BR drag-and-drop
  verb, no direct pile source but high-confidence common usage); `askCmdr.attachment.remove` "Remove
  attachment"→"Remover anexo".
- archive a chat / archived (hide a chat from the active list, not the zip-archive sense) · **arquivar** (verb) /
  **Arquivado** (status) / **Desarquivar** (restore) · Microsoft terminology pt-BR (`archive`→`arquivar` verb, id 14250
  BRA; `Archived`→`Arquivado` status, id 2265623 BRA) · confirmed for arquivar/Arquivado; Desarquivar is the standard
  morphological antonym (des- prefix), not directly in the pile · high. Distinct sense from the glossary's "archive (a
  zip/tar/7z browsed like a folder) → arquivo compactado" entry above; no clash because this is a verb applied to a chat
  session, never co-occurring with the noun sense in the same string.
  `askCmdr.sessions.archive/unarchive/archivedBadge`.
- Turn on / Turn off (a feature toggle) · button label → **Ativar** / **Desativar** X (infinitive); running-text advice
  ("Turn on X to use Y") → **Ative** X (imperative) · matches the shipped pattern
  (`fileExplorer.navigation.driveIndex.menuEnable` "Turn on indexing…"→"Ativar indexação…", `ai.translateError.off.body`
  "Turn on a provider…"→"Ative um provedor…") · confirmed. Feature-on/off status line ("X is on"/"is off") → **está
  ativado** / **está desativado**, matching `ai.translateError.off.title` "AI is turned off"→"A IA está desativada".
  `askCmdr.consent.accept`, `askCmdr.consent.decline`, `settings.askCmdr.turnOn/turnOff/status.on/status.off`.
- "Not now" (decline button on an opt-in screen) · **Agora não** · no direct pile hit; standard pt-BR dismissal idiom
  used across major vendors' opt-in dialogs · high. `askCmdr.consent.decline`.
- "No X yet" (empty-list state) · **Nenhum/Nenhuma X ainda** · matches shipped pt catalog (`operationLog.dialog.empty`
  "No operations yet"→"Nenhuma operação ainda", `whatsNew.dialog.empty`, `queryUi.ai.empty`) · confirmed.
  `askCmdr.sessions.empty` "No chats yet"→"Nenhum chat ainda".
- token (LLM usage unit, cost footer) · **token** (kept verbatim, masculine noun, plural **tokens**) · naturalized pt-BR
  tech loanword, no natural pt equivalent in AI-cost UI copy · high. Plural message needs the CLDR **many** branch like
  every other pt plural (see the Plurals section above): `askCmdr.cost.tokens` writes `one`/`many`/`other`, not just
  `one`/`other`.
- cost / estimate / usage (spend footer) · cost → **custo**; "about {amount}" → **cerca de {amount}**; "cost unknown" →
  **custo desconhecido**; "usage" (heading) → **Gastos** (Spending) / **uso** (running text, e.g. "token use" → "uso de
  tokens") · standard pt-BR tech usage, no pile source (Cmdr-specific AI-billing feature) · high. `askCmdr.cost.*`,
  `settings.askCmdr.spend.*`.
- "free, on-device" (cost readout for the local model) · **grátis, no dispositivo** · "no seu dispositivo" already
  shipped in `ai.local.notInstalled` ("runs entirely on your device"→"roda inteiramente no seu dispositivo"); "grátis"
  is standard pt-BR for zero-cost · high. `askCmdr.cost.free`.
- Log AI model calls (Advanced-settings toggle, `settings.advanced.logLlmCalls.*`) · **Registrar chamadas do modelo de
  IA** · "log"→**registro/registrar** (glossary "changelog"/"crash report" rows), "AI model" = the LLM the user's AI
  features talk to → **modelo de IA** · high. Referenced loosely (not as an exact string match) from
  `askCmdr.consent.logsNote` as "o registro de chamadas de IA".
- "Checking X" tool-status verb (used identically across three distinct Ask Cmdr tool calls: reading the current view,
  listing drives, scoring a folder's importance) · doing: **Conferindo** X; done: **Conferiu** X · picked once and
  reused across all three English "Checking…"/"Checked…" pairs for cross-file consistency, per the tool-status
  doing/gerund + done/preterite pattern already established in `queue.row.label` and `operationLog.summary.*` · high.
  `askCmdr.tool.appState.*`, `askCmdr.tool.listVolumes.*`, `askCmdr.tool.folderImportance.*`.

### Network image-indexing terms (opt a network drive into image-content indexing, `settings.mediaIndex.networkVolumes.*` + `search.imageResults.networkOff/paused`, 2026-07-13)

- network drive · **disco de rede** · glossary drive/disk = **disco** (macOS Finder) + "de rede" modifier (the standard
  pt-BR network qualifier: 137 "de rede" hits in the pile, incl. "discos de rede", "servidor de rede"; MS's "unidade de
  rede" not used, since Cmdr follows macOS "disco") · high. Used across the `networkVolumes.*` list and the two
  `search.imageResults` network strings.
- photo (vs "image") · **foto** / plural **fotos** · macOS pile (Fotos/foto/fotos, 90+ hits) · confirmed. The English
  deliberately says "photos" in the network strings (vs "images"/**imagens** in the on-toggle `enabled.*` row); pt keeps
  the same split (fotos vs imagens). Participles agree with fem **foto**: "foto indexada" / "fotos indexadas".
- background (image indexing runs in the background) · **em segundo plano** · glossary "background (running transfer)"
  row, reused for the indexing-pass sense (20 pile hits) · confirmed. `networkVolumes.description`.
- always index (mark a rarely-browsed drive/folder to index regardless) · "Always index this drive" → **Sempre indexar
  este disco**; "Always-index drives/folders" (internal labels) → **Discos/Pastas para sempre indexar** · standard
  pt-BR; **indexar** per the glossary Indexing row · high. `networkVolumes.alwaysLabel/alwaysAria`,
  `alwaysIndexVolumes/Folders.label`.
- photo archive (a rarely-browsed photo collection, NAS-archive case) · **acervo de fotos** · standard pt-BR for a
  collection/library; chosen over "arquivo de fotos" to avoid the file/archive ("arquivo") ambiguity · high.
  `networkVolumes.alwaysHelp`.
- reconnect / disconnect (a network drive) · **reconectar** / **desconectar** · pile (reconectar 2 hits; glossary
  Disconnect → Desconectar) · high. Status "Paused, resumes when this drive reconnects" → "Pausado, retoma quando este
  disco se reconecta" (pause status **Pausado** + resume **retoma** per the glossary pause/resume rows).
  `networkVolumes.paused`, `search.imageResults.paused`.
- "while you''re not busy" (gentle-reading reassurance) · **quando o Mac está ocioso** · restructured to agree with the
  object (o Mac), not the user, per the gender/inclusive-language rule (sidesteps the gendered "ocupado") · high.
  `networkVolumes.intro`.
- No `sameAsSourceJustification` needed: all 19 values differ from English.

### Image-indexing depth and similar-image search terms (`settings.mediaIndex.importanceThreshold.*` +

`settings.mediaIndex.progress.*` + `search.imageResults.findSimilar/similarTo/backToResults/similarEmpty`, 2026-07-13)

- similar (image-similarity search feature) · **semelhante** · standard pt-BR term for visual/content similarity
  (GNOME/Nautilus-style file-manager usage); distinct from "similar" used loosely in running text elsewhere in the
  catalog (`settings.fileOperations.mtpEnabled.description`), which is not this feature · high. `findSimilar` →
  "Encontrar imagens semelhantes"; `similarTo` → "Semelhante a {name}"; `similarEmpty` → "Nenhuma imagem semelhante
  encontrada."
- covers (a slider level covers N images/folders) · **cobre** · reuses the exact verb already shipped in
  `settings.mediaIndex.enabled.description` ("Por enquanto cobre discos locais") · confirmed. `previewCounting` "Working
  out how much this covers…" → "Calculando quanto isso cobre…".
- skipped (junk folders never indexed) · **ignorados** · reuses the glossary Skip → Ignorar row · high. `floor` "Junk
  like node_modules and system caches is always skipped." → "Itens descartáveis como node_modules e caches do sistema
  são sempre ignorados." ("Junk" avoids **Lixo**, since that word is reserved for the Trash noun in this glossary;
  "descartável" sidesteps the collision.)
- This Mac (local-disk label in the per-drive indexing progress list) · **Este Mac** · matches Apple Finder sidebar
  convention · high. `progress.local`.
- No `sameAsSourceJustification` needed: all 22 values differ from English.
