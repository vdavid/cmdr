# pt glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Cmdr's `pt` ships Brazilian, so mine
  `_ignored/i18n/pt-BR/` (the complete Brazilian set); the bare `_ignored/i18n/pt/` is European Portuguese, a variant
  trap. For how Apple, Microsoft, and GNOME/Xfce render the term and for similar sentences (recipes:
  `docs/i18n/reference-pile/how-to-mine.md`). Cite the source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

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
- queue (the noun) · **Fila** · macOS-adjacent file managers: Total Commander pt-BR (`4005="&Fila"`, "Download em
  fila"), Double Commander pt-BR ("Queue"→"Fila", "Add to queue"→"Adicionar à fila"), MS terminology (Queue→"Fila", id
  96569 BRA, feminine) · confirmed. `queue.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue`
- operation (the category word for one queued job: a copy, move, delete, trash, rename, folder/file creation, or archive
  edit) · **operação** (plural **operações**, feminine) · macOS Finder pt-BR is unanimous (40+ `LocalizableMerged.json`
  values render "operation" as "operação": `NE1` "A operação não pode ser completada.", `NE82` "…outra operação está em
  andamento…", `A17` "…algumas operações ainda estão em andamento."), MS terminology pt-BR (operation→"operação", ids
  333922/87969/1381673, all BRA), Double Commander pt-BR ("Current operation:"→"Operação atual:", "File
  operations"→"Operações de arquivos"), Total Commander pt-BR (`5391="Registro de Operações com Arquivos"`), GNOME
  Nautilus pt-BR ("All file operations have been completed"→"Todas as operações com arquivos foram concluídas") ·
  confirmed. Already the catalog's word via `operationLog.*` ("Registro de operações").
- operation queue (the standalone window listing running and waiting operations) · **Fila de operações** · composed from
  the two confirmed rows above; the pt-BR model for this shape is MS's own "fila de impressão" / "fila de trabalho" ·
  confirmed. **Supersedes "Fila de transferências"**, which was correct only while the window was called the "Transfer
  queue": the English widened from "transfer" to the category word because the window lists deletes, trashes, renames,
  and folder/file creations too, and "transfer" already means copy-or-move one level down (the progress dialog, the
  transfer driver). `queue.windowTitle`, `commands.queueShow.label`, and the three `fileOperations.transferProgress.*`
  toasts all carry the same string, so the window, the View menu item, and the command palette entry read identically.
- ⚠️ **The pair "Fila de operações" (present) / "Registro de operações" (past) sits in one View menu block** and shares
  the head noun on purpose. Never rename one without the other.
- **transferência stays the narrow word.** It's still correct for the copy/move job itself
  (`fileOperations.transferProgress.pauseAria` "Pausar esta transferência", `stallUnknown` "A transferência parou de
  avançar", `transferDialog.smbNativeNote`), and it must NOT come back as the queue's name. The rule: the progress
  dialog talks about one transferência; the queue window talks about operações.
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
- scan / scanning (counting/sizing items before a transfer or delete) · **Analisar** / **Analisando** (in progress) ·
  matches the shipped `fileOperations.transferProgress.stageScanning` ("Scanning" → "Analisando"). Used in the shared
  `fileOperations.shared.scanningTooltip` spinner. · high. Distinct from the conflict-check sense, which stays
  **Verificando** (see verify/check). "Concluída" is the macOS Finder term for complete/concluded ("não pode ser
  concluída", "Download concluído") when a completion phrase is needed.
- Action (what a control chooses; screen-reader label `fileOperations.transferDialog.operationAria`) · **Ação** · macOS
  Finder (6 hits), MS terminology (BRA) · confirmed.
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

### Drive-scan run-kind headers and drive-scan noun (`indexing.run.*` + `indexing.enrich.queued` + `settings.mediaIndex.importanceThreshold.waitingForDriveIndex`, 2026-07-18)

- drive scan (the noun, a full walk of the drive) · **varredura (do disco)** · aligns with the shipped
  `indexing.step.findFilesFirstScan` "Primeira varredura"; **varredura** is the drive-indexing scan noun (distinct from
  the file-operation "Analisar/Análise" sense in the glossary Terms, which is transfer/delete pre-counting) · high.
- First full scan · **Primeira varredura completa** · run-kind header; extends the "Primeira varredura" precedent with
  **completa** for "full" · high. `indexing.run.firstScan`.
- Full rescan · **Nova varredura completa** · a fresh full re-walk; "nova ... completa" reads better than a literal
  "re-" prefix · high. `indexing.run.rescan`.
- Quick update (replay recorded changes, the light path) · **Atualização rápida** · noun form of the glossary Refresh →
  **Atualizar** row; matches `indexing.step.updateIndex` "Atualizar o índice" · high. `indexing.run.update`.

### Bulk-rename review terms (`askCmdr.renameReview.*` + `askCmdr.tool.proposeRenamePlan.*`, 2026-07-20)

The Ask Cmdr rename-proposal modal: a table of proposed renames the user allows or denies row by row.

- rename (the noun: one proposed rename, a rename plan) · **renomeação** · noun of the glossary `rename → Renomear` row;
  already shipped in `askCmdr.renameReview.overwriteTooltip` ("plano de renomeação") · high. Feminine, so counts and
  participles agree: "# renomeação permitida" / "# renomeações permitidas". ❌ Never "alteração de nome" (a pt-PT-shaped
  circumlocution that also breaks the parallel with the `Renomear` verb).
- Rename N files (the primary action) · **Renomear # arquivo / # arquivos** · GNOME Nautilus pt-BR verbatim ("Rename %d
  Files" → "Renomear %d arquivos") · confirmed. `askCmdr.renameReview.rename`; the ICU plural wraps only the count +
  noun, keeping "Renomear" outside the branches.
- Review (verb, the modal title) · **Revisar**; the review itself (noun) · **revisão** · MS terminology pt-BR (review →
  "revisão"/"examinar") · high. ❌ Not "Rever", which reads pt-PT. `renameReview.title` "Review file renames" → "Revisar
  renomeações de arquivos"; `renameReview.expired` "This review expired" → "Esta revisão expirou".
- Allow / Deny (per-row approval pair) · **Permitir** / **Negar** · macOS pt-BR ("Permitir", "Permitir Mesmo Assim"), MS
  terminology pt-BR (Allow → "Permitir", Deny → "Negar", both BRA) · confirmed for Permitir, high for Negar (macOS has
  no Deny label; its permission dialogs say "Não Permitir", which is Don't-Allow, not Deny). Chosen over "Recusar" (=
  decline) because the pair is an approval gate, not an invitation.
- Allow all / Deny all · **Permitir tudo** / **Negar tudo** · the shipped "tudo" pattern for a bare all-object (glossary
  `Selecionar tudo` / `Desmarcar tudo`; macOS "Remover Tudo"; Total Commander "Substituir tudo") · high. "tudo" also
  sidesteps gender agreement with the implied feminine "renomeações".
- New name / Current name (table column headings) · **Novo nome** / **Nome atual** · **Novo nome** is unanimous across
  all five file-manager corpora (Nautilus "Novo nome do arquivo", Double Commander, Thunar "Novo nome", Dolphin, and
  Total Commander's multi-rename column set `1400="Nome antigo;Ext.;Novo nome;…"`) · confirmed. ❌ Not "Nome novo"
  (reversed order, unsourced). "Nome atual" keeps the English's deliberate current-vs-old framing, matching the
  catalog's "pasta atual".
- overwrite (the red warning badge `(overwrite!)`) · **(substituição!)** · overwrite → **substituir** is unanimous in
  the pile (MS terminology BRA, macOS Finder "Substituir", Total Commander `1334="Confirmar substituição"`, Double
  Commander "Confirm overwrites" → "Confirmar substituições"); zero "sobrescrever" hits in macOS/Nautilus/Double
  Commander · confirmed. The NOUN form keeps the badge family parallel: the sibling badges are nouns too ("(ciclo)",
  "(extensão)"), and a bare "(substituir!)" would read as a button.
- rename cycle (A→B, B→A) · **Ciclo de renomeação** · MS terminology (cycle → "ciclo") · high. The tooltip renders "one
  temporary name while rotating these files" as "um nome temporário ao trocar os nomes desses arquivos entre si": the
  literal "girar/rotacionar os arquivos" reads as rotating the images, and "entre si" is what carries the cycle.
- extension (filename extension) · **extensão** · MS terminology pt-BR ("file name extension" → "extensão" / "extensão
  de nome de arquivo") · confirmed. `extensionBadge`, `extensionTooltip`.
- rename plan (the proposal the tool prepares) · **plano de renomeação** · compositional on the renomeação row · high.
  `askCmdr.tool.proposeRenamePlan.*` keeps the doing/gerund + done/preterite tool-status pattern ("Preparando" /
  "Preparou"), same as the `Conferindo`/`Conferiu` row above.
- No `sameAsSourceJustification` needed: all 28 values differ from English.

### Image-index status and scope terms (`fileExplorer.imageIndex.*` + `settings.mediaIndex.scope/chosenFolders.*` + `askCmdr.tool.imageFacts/searchPhotos.*`, 2026-07-20)

- image search (the feature, when named in running text) · **busca de imagens** · matches the shipped card title
  `settings.mediaIndex.card` "Image search" → "Busca de imagens" and the glossary search → **busca** row · confirmed. ❌
  Not "pesquisa de imagens" when naming the feature. The adjective **pesquisável** stays where it already ships
  (`settings.mediaIndex.reclaim.line`, `progress.kept`, `chosenFolders.help`): it's a property of the indexed item, not
  the feature name.
- indexing (in progress) · **Indexando** · pt-BR gerund, matching every sibling progress label (Copiando/Movendo/
  Analisando/Baixando) and the shipped `search.imageResults.indexing` ("ainda estão sendo indexadas") · confirmed. ❌
  Never the pt-PT `A indexar` / `está a indexar`. `fileExplorer.imageIndex.indexing`, `indexingTooltip*`.
- indexing pass (one sweep of the image indexer) · **rodada** ("on the next pass" → "na próxima rodada") · standard
  pt-BR for a periodic batch run; chosen over "passagem" (reads as passage/ticket) and over **varredura**, which is
  reserved for the drive scan · high. `fileExplorer.imageIndex.indexedTooltip`.
- full check (the drive index's next scheduled full walk) · **varredura completa** · the settled drive-scan noun; the
  sibling drive-index tooltips already say "Faça uma nova varredura" / "Refaça a varredura" · confirmed. ❌ Not "análise
  completa": **Análise/Analisar** is reserved for the transfer/delete pre-count sense.
  `fileExplorer.navigation.driveIndex.tooltipCoalesced`.
- "macOS lost track of file system changes" · **O macOS deixou de acompanhar as mudanças no sistema de arquivos** ·
  high. ❌ Not "perdeu o controle", which reads as "lost control" and is alarming; these tooltips must stay reassuring
  and may never use the words for error or failed. The closing "no big deal" → **não é nada preocupante** (warm,
  unambiguous, and dodges the nada demais / nada de mais spelling fight).
- covered (a folder is / isn't inside the indexed scope) · **coberta** ("may or may not be covered" → "pode ou não estar
  coberta") · reuses the shipped `settings.mediaIndex.enabled.description` verb "cobre" · confirmed.
- "Reading what's in your photos" (the image-facts transparency tool line) · **Lendo / Leu o conteúdo das suas fotos** ·
  photo → **foto** (glossary row) + the doing/gerund + done/preterite tool-status pattern · high. "o conteúdo das suas
  fotos" is deliberately explicit that image CONTENT is read; don't soften it to "suas fotos".
- "you choose yourself" (gender-neutral restructure) · **por conta própria** · the gender rule bans a masculine-default
  user adjective, and "você mesmo" is exactly that; "por conta própria" is invariable · high.
  `settings.mediaIndex.scope.description`.
- No `sameAsSourceJustification` needed: all 26 values differ from English.

### Image-index status badge terms (`fileExplorer.imageIndex.*` + `settings.mediaIndex.showFileStatusIcons.*`, 2026-07-22)

The small per-file/folder/drive overlay indicators showing image-search indexing state, plus the Settings toggle for the
per-file badge.

- badge (small overlay marker on a file/folder icon) · **selo** · Microsoft terminology pt-BR (`badge` → `selo`, id
  1354385; reinforced by "Selo digital", "Selo do OneNote", "Selos em destaque") · high. Chosen over "distintivo" (reads
  as a police/ID badge) and "emblema" (heraldic). macOS localizes its own overlay badges only by their status meaning
  (AXBADGE keys carry no noun), so MS's "selo" is the authority. `settings.mediaIndex.showFileStatusIcons.*` ("status
  badges" → "selos de status", status kept verbatim per the glossary `status` row).
- indexed-state file tooltips agree with feminine **imagem** · the five `file.*` tooltips are subject-less in English
  ("Indexed", "Changed", "Not included"); pt picks feminine to agree with **imagem** and stay consistent with the
  folder/drive strings' "imagens indexadas" and the network row's "foto indexada". So: **Indexada**, **Modificada**,
  **incluída**, "indexada de novo" · high. `fileExplorer.imageIndex.file.indexed/pending/stale/failed/excluded`.
- "Waiting to be indexed" · **Aguardando indexação** · glossary waiting/queued → **Aguardando** + indexing noun →
  **indexação** · high. `file.pending`.
- "Changed since indexing; will be re-indexed" · **Modificada desde a indexação; será indexada de novo** · "changed"
  reuses the glossary `modified` → **Modificado(a)**; "re-indexed" → "indexada de novo" (the glossary `Full rescan`
  row's preference for "nova"/"de novo" over a literal "re-" prefix, e.g. "Navegue até aqui de novo") · high.
  `file.stale`.
- "Couldn''t be indexed" (gentle, no error/failed words) · **Não foi possível indexar** · the no-bare-error voice ("Não
  foi possível …", glossary error-copy phrasings) · high. `file.failed`.
- "Not included in image search" · **Não incluída na busca de imagens** · direct; busca de imagens per the image-search
  row above · high. `file.excluded`.
- "still working" (drive indexing in progress) · **ainda em andamento** · matches the operation-log running status **Em
  andamento**; avoids a gerund clash with the sibling **Indexando** · high. `drive.indexing`.
- "is off for this drive" (feature-off status) · **está desativada para este disco** · glossary Turn on/off → status
  **está desativado/desativada** (fem here, agreeing with "a busca de imagens"); drive → **disco** · high. `drive.off`.
  `drive.ariaLabel` "Image search status for this drive" → "Status da busca de imagens deste disco".
- ICU plurals (`folder.allIndexed/someIndexed`, `drive.indexing/done`) select on `{total}` and write pt's `one`/`many`/
  `other` branches. The **noun + participle** (imagem indexada / imagens indexadas) and any "Todas as" agreement go
  INSIDE the branches so total=1 reads "1 imagem indexada", not "Todas as 1 imagem"; `{doneText}`/"deste disco"/"; ainda
  em andamento." stay outside · high.
- No `sameAsSourceJustification` needed: all 13 values differ from English.

### Image-indexing settings restructure + Semantic-search card terms (`settings.mediaIndex.cards.*` + `settings.mediaIndex.progressSummary.title` + `settings.mediaIndex.semanticSearch.label` + `settings.mediaIndex.clip.notSupported/offButInstalled/deleteButton/deleting/deleteConfirmTitle/deleteConfirmBody/deleteFailed` + `fileExplorer.imageIndex.file.indexing`, 2026-07-22)

- search by description (the semantic-search feature, plainer name; card title stays "Busca semântica") · **busca por
  descrição** (noun) / **Buscar fotos por descrição** (the toggle label) · glossary `search → busca/buscar` (macOS
  Finder, confirmed) + the feature-name rule ("❌ Not 'pesquisa …' when naming the feature") + the shipped card "Busca
  de imagens" · high. Keeps the whole card in the **busca** family (busca semântica / busca por descrição / busca por
  palavra-chave / busca por etiqueta). ⚠️ The already-shipped `settings.mediaIndex.clip.ready` uses "pesquise … por
  descrição" (a pre-existing minor divergence, unchanged sourceHash, left as-is); new strings follow the glossary
  **buscar**.
- delete the on-device semantic-search (CLIP) model · **Excluir** (button/title) / **Excluindo…** (in progress) · reuses
  the model-deletion domain settled in `ai.local.deleteModel` = "Excluir modelo" / `deleteDialogTitle` = "Excluir modelo
  de IA?" · confirmed. ❌ NOT the file-delete **Apagar**: the glossary reconciliation note reserves "Excluir" for the
  on-device-model deletion sense (ai.json), and this CLIP model is the same domain. `deleteButton` "Delete model" →
  "Excluir modelo"; `deleteConfirmTitle` → "Excluir o modelo de busca semântica?".
- reclaim / frees (disk space when deleting the model) · **liberar** ("reclaim {size}" → "liberar {size}"; "This frees
  {size}" → "Isso libera {size}") · reuses the `settings.mediaIndex.reclaim.*` family already shipped with **liberar**
  ("liberar cerca de {size}") · confirmed.
- "couldn''t be removed just now" (delete-model failure, no error/failed words) · **Não foi possível remover o modelo
  agora.** · no-bare-error voice (glossary error-copy phrasings); "removed" → **remover** (mirrors the English variation
  from "delete") · high. "Try again in a moment." → "Tente novamente em instantes." (Try again → glossary **Tentar
  novamente**).
- Enable indexing (card title, master on/off) · **Ativar indexação** · glossary Turn on → **Ativar** + Indexing noun →
  **indexação**; matches the shipped `fileExplorer.navigation.driveIndex.menuEnable` "Turn on indexing…" → "Ativar
  indexação…" · confirmed. `cards.enable`.
- Folders to index (card title) · **Pastas para indexar** · matches the shipped always-index label pattern "Pastas para
  sempre indexar" · high. `cards.folders`.
- Indexing now (live-progress heading + the file badge tooltip; same sourceHash, same value) · **Indexando agora** ·
  glossary indexing-in-progress → **Indexando** (pt-BR gerund) + **agora** for the "now" emphasis; avoids the pt-PT "A
  indexar" · high. `progressSummary.title`, `fileExplorer.imageIndex.file.indexing`.
- Apple silicon · kept verbatim · English `@key.description` says keep it; no pt-BR pile hit for a localized form.
  `clip.notSupported`.
- keyword · **palavra-chave** · standard pt-BR · high. tag (Finder-tag search sense) · **etiqueta** · shipped pt catalog
  (`settings.listing.showTags` "etiquetas do Finder do macOS", `commands.tagsToggle*` "etiqueta") · confirmed.
  `deleteConfirmBody` "Keyword and tag search" → "A busca por palavra-chave e por etiqueta".
- No `sameAsSourceJustification` needed: all 12 values differ from English.

### Delete-dialog trash switch + transfer From/To group headings (`fileOperations.delete.trashSwitch`/`confirmDelete` + `fileOperations.transferDialog.sourceGroupTitle`/`targetGroupTitle`, 2026-07-23)

- "Move to trash" (switch in the delete dialog, on = Lixo, off = permanent delete) · **Mover para o Lixo** · macOS
  Finder pt-BR AL13/N153 verbatim; identical to this file's `transferDialog.titleVerbOnly` `other {Mover para o Lixo}`
  arm, so the switch and the confirm button read as one pair · confirmed
- "Delete" (destructive confirm button while the switch is off) · **Apagar** · the settled Finder verb (not the Windows
  "Excluir"); identical to `transferDialog.titleVerbOnly`'s `delete {Apagar}` arm · high
- "From" / "To" (headings over the source path and over the destination volume + path) · **De** / **Para** · Total
  Commander pt-BR (`662="DE: "`, `663="PARA: "`) and Double Commander pt-BR ("De:"/"Para:") both ship this label pair in
  the same copy/move dialog; sentence case here per the style guide. The settled nouns origem / destino stay for the
  destination CONTROLS ("Volume de destino", "Caminho de destino"); the headings take the light prepositional pair the
  English uses · high

### Master drive-indexing switch terms (`fileExplorer.navigation.driveIndex.refusedIndexingOff`/`tooltipIndexingOff`/`menuIndexingOffNote` + `settings.indexing.masterOffNote`/`overriddenBadge`, 2026-07-25)

- drive (the storage device, ALL senses) · **disco** · macOS pt-BR Finder is unambiguous: "Discos rígidos", "Discos
  externos", and ~40 more `disco` values; the bare English "drive" appears there only inside brand names (iCloud Drive),
  and "unidade" only for the physical mechanism ("unidade de disco"). Microsoft pt-BR says `drive` → "unidade", but
  term-choice principle 2 puts Finder first · confirmed. Catalog frequency backs it: **disco 158 hits vs drive 38**, and
  every one of those 38 is a brand name (Google/Proton/iCloud Drive), an `errors.json` "drive de rede / externo /
  interno" collocation, or the `driveIndex.*` family. ❌ Never "unidade" for a disk in Cmdr.
- ⚠️ **`errors.json` still says "drive de rede / drive externo / drive interno" in 10 explanation/suggestion strings.**
  That's the last unreconciled pocket, and it contradicts the settled `disco de rede` from the network-image-indexing
  row. Left alone here (out of that batch's scope); worth a dedicated `errors.json` pass.
- Feature on/off, the ADJECTIVE · **ativado** / **desativado** · the whole catalog's settled pair
  (`settings.askCmdr. status.on/off`, `ai.translateError.off.title`, `settings.ai.provider.opt.off` = "Desativado",
  `fileExplorer.imageIndex.drive.off`, `shortcuts.list.disabledTooltip`) · confirmed. **ligado / desligado is reserved
  for physical power** in this catalog (`errors.listing.devicePoweredOff` "Dispositivo desligado", "o host está ligado",
  a cable "desligado") and must never label a software toggle. "on or off choice" → "escolha de ativado ou desativado";
  "turn this back on" → **reativar** (matches the `settings.indexing.reEnableNotifications.button` "Reativar").
- "Off with drive indexing" (the small overridden-row badge, `settings.indexing.overriddenBadge`) · **Desativado com a
  indexação** · badge brevity is a hard constraint, so the head noun stays the short "a indexação": the badge only ever
  renders inside the "Indexação de disco" page, directly under that toggle, so the full "de disco" is redundant · high.
- Settings-path fragments quote the catalog verbatim: "Indexing > Drive indexing" → **Indexação > Indexação de disco**
  (`settings.section.indexing` + `settings.indexing.enabled.label`, both unchanged). "in Settings" → **nos Ajustes**
  (glossary Settings row); the imperative takes the enclitic object, **Ative-a em …**, agreeing with the feminine "a
  indexação de disco" (matches the sibling `tooltipDisabled` "Ative-a para ver…").
- "picks up where it left off" · **continua de onde parou** · standard pt-BR idiom; the reassurance that per-drive
  progress survives the master switch · high.
- No `sameAsSourceJustification` needed: all five values differ from English.

### pt-PT leak found and fixed (2026-07-25)

`settings.archives.compressionLevel.description` shipped European Portuguese ("ficheiro"/"ficheiros", plus the pt-PT
"demoram mais tempo **a** comprimir"). Root cause is recorded above in the Compress row: it cites "pt Double Commander /
Thunar / Nautilus" and notes "pt pile has no Total Commander" — that is the **bare `_ignored/i18n/pt/` folder, which is
EUROPEAN**. The Brazilian set is `_ignored/i18n/pt-BR/`, and it _does_ have `total-commander/`. Now reads "geram um
arquivo menor, mas demoram mais para comprimir. Vale para o comando Comprimir e para a cópia ou movimentação de arquivos
para um zip." Re-check any other row whose sources name the bare `pt` pile.

## Índice do disco: a verificação de mudanças (2026-07-28)

- **"Checking for changes" (run-kind header) → `Verificação de mudanças`** · nominal phrase matching the sibling headers
  (`Primeira varredura completa`, `Atualização rápida`); `Verificando` is macOS pt-BR's checking verb (Finder BN9
  "Verificando os conteúdos…"), `mudanças` is catalog-settled (`as mudanças recentes`) · high.
- **"Update the file list" → `Atualizar a lista de arquivos`** · composed from the settled siblings
  `Salvar a lista de arquivos` + `Atualizar o índice` · high.
- **"the check running right now" → `a varredura que está em andamento agora`** · reuses `varredura` as this catalog's
  settled word for a full check (`tooltipCoalesced`: "a próxima varredura completa do Cmdr") and that string's closing
  `vai corrigir isso` · high.

### Stalled-transfer notice terms (`fileOperations.transferProgress.close`/`stall*` + `queue.row.stalled`, 2026-07-31)

The copy/move dialog stops showing an ETA it no longer believes and explains the stall instead. Whole batch avoids
"erro"/"falhou" (and any bare "Erro"), per the no-bare-error voice rule.

- Close (button that closes the progress dialog while the transfer keeps finishing in the background; sits next to
  Cancelar) · **Fechar** · macOS Finder pt-BR (`LocalizableMerged.json` key `FR26` "Close" → "Fechar", key-based
  EN→pt-BR), Microsoft terminology pt-BR (close → "Fechar", 4 entries) · confirmed. Clearly distinct from the sibling
  **Cancelar**, so the two buttons never read as the same action.
- "No progress for {duration}" (the line that replaces the ETA on a stalled transfer) · **Sem progresso há {duration}**
  · `progresso` is macOS Finder pt-BR ("Show Copy Progress" → "Mostrar Progresso da Cópia", "Show Progress Window" →
  "Mostrar Janela de Progresso") · high. **`há`, not `por`/`durante`**: pt-BR expresses an elapsed stretch running up to
  now with `há` ("Sem progresso há 45s"), and `{duration}` is always an already-formatted elapsed span. Same value in
  both surfaces, with the dialog's period and without it on the queue row, matching English
  (`transferProgress.stallNotice`, `queue.row.stalled`).
- "Waiting for the {destination,source} to respond" · **Aguardando resposta do destino** / **Aguardando resposta da
  origem** · the noun-phrase shape is Double Commander pt-BR's ("Waiting for user response" → "Aguardando resposta do
  usuário", "Waiting for access to file source" → "Aguardando acesso à origem do arquivo") and Total Commander pt-BR's
  (`1384="Enviando dados. Aguardando resposta..."`); destination → **destino** and source → **origem** are macOS Finder
  pt-BR ("copiado para o destino", "Talvez o destino não seja compatível", "volume de destino") + MS terminology · high.
  Reuses the settled status word **Aguardando**. macOS Finder also offers the verbal pattern "Waiting for ^0 to accept…"
  → "Aguardando que ^0 aceite…"; the noun phrase was chosen because it's shorter for a status line and matches the
  file-manager lineage.
- "The transfer has stopped moving" · **A transferência parou de avançar** · `transferência` is catalog-settled for the
  copy/move job (`fileOperations.transferProgress.pauseAria` "Pausar esta transferência"); "parou de avançar" says the
  motion stopped without implying the transfer ended or broke · high. ❌ Not "travou" (reads as a crash) and not "parou"
  alone (reads as terminated).
- "Cancel it, or leave it running in the background" · **Cancele-a ou deixe-a rodando em segundo plano** · enclitic
  object pronouns agreeing with the feminine "a transferência", matching the catalog's enclitic habit ("Ative-a em…",
  "Encontre-a na fila de transferências"); **rodando em segundo plano** is verbatim catalog
  (`transferProgress.backgroundedToast` "Ainda rodando em segundo plano", `queueTooltip` "Mantenha isto rodando em
  segundo plano") · confirmed. No comma before `ou` (pt-BR doesn't take one in a two-item alternative), so the English
  comma is dropped.
- "N file(s) still open and may already be partly written" · plural branches carry the WHOLE predicate ·
  **`{count, plural, one {# arquivo ainda está aberto e já pode estar parcialmente gravado} many {…abertos e já podem estar parcialmente gravados} other {…}}.`**
  · "arquivos abertos" for open handles is Total Commander pt-BR (`616="Muitos arquivos abertos!"` = "Too many open
  files!"); "written" → **gravado** matches the sibling `transferProgress.titleFlushing` "Gravando a última parte..." ·
  high. The trailing clause has to agree in number (aberto/abertos, pode/podem, gravado/gravados), so it goes INSIDE
  each branch and only the final period stays outside — the same restructuring the image-indexing plurals needed.
- "The log has the details" (Cmdr's log FILE, not the operation log) · **O arquivo de registro tem os detalhes.** ·
  sentence shape lifted verbatim from the shipped `askCmdr` sibling ("The operation log has the details." → "O registro
  de operações tem os detalhes."); **arquivo de registro** is catalog-settled (`settings.json` "Abrir arquivo de
  registro", `errorReporter` "arquivos de registro") · confirmed. Keeping the head noun `arquivo de` is what separates
  the log file from the `registro de operações` feature; MS terminology's "arquivo de log" loses to catalog consistency.
- No `sameAsSourceJustification` needed: all eight values differ from English.

## Caminho copiado: a confirmação da área de transferência (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Uma chave: a linha do aviso informativo depois de ⌃⌘C. O caminho aparece abaixo, em linha própria e monoespaçada, então
NÃO é um marcador dentro da frase: a frase termina em dois-pontos e precisa funcionar sem ele.

- **"Copied the path, it's now on your clipboard:" → `Caminho copiado, agora está na área de transferência:`** ·
  reutiliza `path → caminho` e `clipboard → área de transferência` do glossário (macOS "Área de Transferência") · high.
  O particípio inicial segue os avisos irmãos (`{countText} itens copiados`). Sem possessivo ("sua área de
  transferência"): só existe uma, e o macOS usa o artigo.
- Sem `sameAsSourceJustification`: o valor difere do inglês.

### Operation-queue rename (`queue.*` + `commands.queueShow.*` + `fileOperations.transferProgress.queue*`/`backgroundedToast`, 2026-08-08)

The queue window was renamed from "Transfer queue" to "Operation queue" in English, a meaning change: it lists deletes,
trashes, renames, and folder/file creations too, not only transfers. Fourteen pt values widened with it. The head noun
and the window name are in the main Terms list above (**operação** / **Fila de operações**, superseding "Fila de
transferências"); the rest of the batch is below.

- "Operations" (window heading + the list's screen-reader label) · **Operações** · the bare plural, matching the
  English's category-naming plural noun · confirmed. `queue.heading`, `queue.list.aria`.
- The four per-row aria labels keep their settled verbs and only swap the object noun: **Pausar / Retomar / Cancelar /
  Selecionar esta operação** · glossary pause→**Pausar**, resume→**Retomar**, Cancel→**Cancelar**, select→
  **Selecionar** · confirmed. `queue.row.pauseAria/resumeAria/cancelAria/selectAria`.
- `commands.queueShow.label` dropped its "Mostrar" prefix, because the English label is now the bare window name and its
  `@key.description` requires the command palette entry, the View menu item, and the window title to be one string. So
  the label is exactly **Fila de operações**.
- `commands.queueShow.description` is the locale's own sibling with one noun changed (per the learnings doc's "a new key
  that VARIANTS an existing one is an edit of the sibling"): **Abra uma janela com todas as operações em andamento e
  aguardando, onde você pode pausar, retomar ou cancelar**. "aguardando" is the settled queued/waiting status word; the
  explicit **você** is kept (a dropped one is a pt-PT tell).
- `queuedToastCount` writes pt's three CLDR branches on the new noun:
  `one {# operação} many {# operações} other {# operações}`. **operação is feminine, exactly like transferência**, so
  every downstream agreement in the surrounding strings survived the rename untouched: `queuedToast`'s "na frente desta
  … ela … Encontre-a", `backgroundedToast`'s "Encontre-a", and the toolbar's "Cancelar selecionadas" / "#
  selecionada(s)".
- Regional-variant check run value by value against the style guide's pt-PT tell list (ficheiro, `estar a` + infinitive,
  consoante, proclisis before an infinitive, Rever, alterar o nome, a dropped você): zero hits. The batch's Brazilian
  markers are "gerencie" (not pt-PT "gira"), "rodando em segundo plano", and the retained "você".
- No `sameAsSourceJustification` needed: all 14 values differ from English.
