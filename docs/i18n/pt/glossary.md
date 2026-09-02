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
- word wrap · **quebra automática de linha** · MS terminology pt-BR · high. ❌ NOT the shortened "quebra de linha": an
  earlier pass clipped it for the Settings toggle only, so the viewer's View menu and Settings named the same setting
  differently. Both keys carry the full term now.
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
- zoom in / out · **Ampliar** / **Reduzir** · Safari pt-BR `MainMenu.strings` `438.title`/`439.title`, read off the
  installed macOS 26.x (2026-08-30) · high. ❌ NOT `Aumentar zoom` / `Reduzir zoom`: that was this line's earlier
  ruling, the menu-bar pass superseded it, and only `menu.json` followed. Zoom reset stays **redefinir o zoom**, and
  "Zoom to N%" stays **Zoom em N%** · standard pt-BR; macOS Finder uses "Aumentar/Diminuir Tamanho do Ícone" but
  **zoom** is kept for the UI-scale feature · high
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
- **The `errors.json` pocket is closed** (2026-08-24): the ten "drive de rede / externo / interno / virtual" strings now
  say `disco`, and so does `fileExplorer.unreachable.detailTimeout`. See § O bolsão de `drive` fechado at the end of
  this file. The remaining `drive` hits in the catalog are brand names, the `{drive}` placeholder, and the
  `driveIndex.*` key names.
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

Uma chave: a linha do aviso informativo depois de ⌘⌥C. O caminho aparece abaixo, em linha própria e monoespaçada, então
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

### Progress-chip and failure-notice terms (`queue.row.dismiss*` + `queue.toolbar.dismissAll` + `queue.failureToast.*` + `queue.chip.*`, 2026-08-08)

Two new surfaces on top of the queue window: a corner progress chip (~80 px) previewing the background operation, and a
failure notice (a ~360 px toast) plus a dismissible failed row. The head noun and the window name are settled in the
main Terms list (**operação** / **Fila de operações**); this section only adds what those two surfaces needed.

- dismiss (stop showing a notice or a finished-badly row; nothing is undone, retried, or deleted) · **Dispensar** · the
  pt catalog's own settled verb, five hits for the same concept before this batch (`ui.toast.dismissAria` "Dispensar
  notificação", `downloads.empty.dismiss`, `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`,
  `errorReporter.bundleSavedToast.dismiss`, `fileOperations.mkdir.timeoutDismiss`, and the viewer's
  `reloadToast.dismissTooltip` "Dispensar sem recarregar") · high. ❌ **Never MS terminology's `dismiss` → "ignorar"**
  (id 780443/1044462, BRA): **Ignorar is this catalog's Skip** (`transferProgress.conflictSkip`,
  `transferDialog.policySkip` "Ignorar todos"), so a Dismiss button labelled "Ignorar" would sit two rows from a Skip
  button meaning something else. KDE Dolphin pt-BR's "Descartar lembrete" is the runner-up, and the two stragglers that
  still said "Descartar" (`crashReporter.dialog.dismiss`, `lowDiskSpace.toast.closeTooltip`) now say `Dispensar` too.
  `queue.row.dismiss`; the aria takes the sibling row shape, **Dispensar esta operação** (matching "Pausar / Retomar /
  Cancelar / Selecionar esta operação").
- Dismiss all (toolbar) · **Dispensar tudo** · parallel to the shipped `Pausar tudo` / `Retomar tudo`, and "tudo" is the
  catalog's settled bare-all-object pattern (`Selecionar tudo`, `Permitir tudo`), which also sidesteps agreement with
  the feminine "operações" · high. `queue.toolbar.dismissAll`.
- "Couldn''t finish <doing X>" (the failure toast's nine `select` arms) · **Não foi possível concluir + [article +
  action noun]** · macOS Finder pt-BR ships this exact frame dozens of times (`NE113` "Não foi possível concluir a
  sincronização do ^0", `PW38`/`NE9`/`NE13`/`NE63` "Não foi possível concluir a operação porque…") · confirmed. The
  `other` arm is byte-identical to the `queue.row.status` `failed` arm (**Não foi possível concluir**), so the toast,
  the queue row, and the chip say the same thing; the other eight are that phrase plus the operation's noun.
- The eight action NOUNS behind those arms, each Tier-1 or catalog-settled: cópia (Finder `NE111` "concluir a cópia"),
  **movimentação** (Finder `MV2_V1` "Desfazer Movimentação de ^1", `LA17` "a movimentação ou cópia de um item"),
  **apagamento** (Finder `PW33` "Apagamento do Volume" and Localizable "até a conclusão do apagamento"), movimentação
  para o Lixo, **renomeação** (glossary row; Nautilus pt-BR "Desfazer renomeação", TC `6601` "Renomeação em Lote"),
  criação da pasta, criação do arquivo, edição do arquivo compactado · high. ⚠️ **apagamento is the delete NOUN**,
  nominalizing the settled `Apagar`: it is what keeps the banned "exclusão" out of this family. The shipped
  `queue.empty.body` still says "exclusões" (see the flag below).
- "Show in operation queue" (the toast's button) · **Mostrar na fila de operações** · glossary `Show in Finder` →
  "Mostrar no Finder" + the window name inflected the way the catalog already inflects it in running text
  (`transferProgress.queueTooltip` "gerencie na fila de operações", `backgroundedToast` "Encontre-a na fila de
  operações") · confirmed.
- "N operations couldn''t finish" (the coalesced toast + the chip's failed state) · **Não foi possível concluir
  {countText} operação/operações** · the invariant house phrase is hoisted OUTSIDE the plural and only the counted noun
  branches, the same shape `askCmdr.renameReview.rename` uses for "Renomear # arquivo(s)" · high. Hoisting also means no
  participle has to agree, so the three CLDR branches (`one` / `many` / `other`) differ only in the noun. The chip's
  second sentence, "Open the operation queue to see why", is **Abra a fila de operações para ver por quê** (imperative,
  matching the catalog's "Ative-a em…" / "Encontre-a na fila de operações"; sentence-final **por quê** takes the
  circumflex). `queue.failureToast.summary`, `queue.chip.failed`.
- "percent", spelled as a word for the screen reader · **por cento** ("42 por cento") · pt-BR reads `%` aloud as "por
  cento", so spelling it out changes nothing for VoiceOver and protects the aria label from a reader that would say
  "porcentagem" or skip the sign · high. Only in `queue.chip.ariaLabel`; the visible tooltip keeps the sign as
  **{percentText}%**, with NO space before `%` — pt-BR sets it tight, and the whole catalog already does ("100%", "50% e
  200 MB", `lowDiskSpace` "({percentText}%)"). This is the one place the de/fr/sv space-before-% rule must NOT be
  copied.
- item (the tooltip's countable, covering files and folders alike) · **item** / plural **itens** · macOS Finder pt-BR
  throughout ("^0 itens", "Remover ^0 itens", `PW5_V2` "Preparando para copiar ^0 itens") · confirmed.
- destination clause in the tooltip · **para {destination}** · Finder pt-BR ("copiado para o destino") and the transfer
  dialog's own **Para** heading · confirmed. Keep the leading space INSIDE the branch (` para {destination}`), like the
  count and detail clauses, so an absent clause leaves no double space and the empty `=0 {}` / `other {}` arms stay
  empty.
- time left, the tooltip's trailing `{detail}` · **{duration} restantes** · NOT translated in this batch: the chip
  reuses `fileOperations.transferProgress.etaRemaining` verbatim (and `queue.row.status` `paused` → **Pausado** when
  there's no honest countdown), so the chip and the progress dialog can't drift. Don't re-derive a second time-left
  phrasing for the chip.
- Regional-variant check run value by value against the style guide's pt-PT tell list (ficheiro, `estar a` + infinitive,
  consoante, proclisis before an infinitive, Rever, alterar o nome, a dropped você), plus U+2019 and double-space scans:
  zero hits across all nine. Brazilian markers in the batch: **arquivo** (never ficheiro) in two toast arms, **Lixo**
  (never Reciclagem), **renomeação** (never "alteração de nome"), and the pt-BR gerund labels the chip borrows from
  `queue.row.label`.
- No `sameAsSourceJustification` needed: all nine values differ from English.
- ⚠️ Two nearby inconsistencies found while settling **Dispensar**, both out of this batch's scope:
  `crashReporter.dialog.dismiss` and `lowDiskSpace.toast.closeTooltip` still render "Dismiss" as **Descartar**, against
  the catalog's five "Dispensar"; and `queue.empty.body` still lists deletes as **exclusões**, against the settled
  delete family (Apagar / Apagando / Apagou / apagamento). Worth one reconciliation pass each.

### Standalone conflict-prompt terms (`fileOperations.operationConflict.context`/`pausedNote`, 2026-08-09)

The main window now hosts the name-clash prompt for a backgrounded operation, so a context line under the title
`O arquivo já existe` names which operation is asking, and a quiet note explains why the rest of the queue stopped.

- Progress line with a destination · **Copiando para {destination}** / **Movendo para {destination}** · macOS Finder
  pt-BR ships exactly this frame for its own copy/move progress (`CP4_V1` "Copiando “^1” para “^2”", `CP4_V2` "Copiando
  ^0 itens para “^2”", `MV4_V1`/`MV4_V2` the same for Movendo) · confirmed. The gerund head comes from the sibling
  `queue.row.label` arms, the preposition **para** from `queue.chip.tooltip`'s ` · para {destination}`. `{destination}`
  stays UNQUOTED (Finder quotes it, the catalog's own chip tooltip doesn't) and takes no article, since a folder name is
  an uncontrolled insert.
- Generic "Working (in X)" arm · **Operação em andamento em {destination}** / **Operação em andamento** · the bare
  `queue.row.label` `other` arm "Em andamento" is a status label and strands the reader in a full sentence under a
  dialog title, so the settled head noun **operação** is supplied. macOS Finder pt-BR carries the same shape verbatim
  ("…ainda há uma operação em andamento em um dispositivo iOS", `LocalizableMerged.json`), so the "em andamento em X"
  stacking is idiomatic, not a repetition slip · high.
- `archive_edit` splits by design: the with-destination arm names the archive (**Editando {destination}**, e.g.
  "Editando fotos.zip"), the no-destination arm stays generic with an article (**Editando um arquivo compactado**),
  where the queue row's bare label is article-less. Same settled verb/noun (`Editando` + `arquivo compactado`).
- "Everything else is paused until you answer." · **Todo o resto está pausado até você responder.** · reuses the settled
  status adjective **Pausado** (`queue.row.status` `paused`, glossary pause row); "até você responder" is the pt-BR
  personal infinitive and keeps the explicit **você** (dropping it is a pt-PT tell) · high. Reassuring, no error/failed
  words.
- Regional-variant check against the style guide's pt-PT tell list (ficheiro, `estar a` + infinitive, consoante,
  proclisis before an infinitive, Rever, alterar o nome, a dropped você), plus U+2019 and double-space scans: zero hits.
  Brazilian markers: the gerunds **Copiando / Movendo / Editando** (never "a copiar"), **arquivo compactado** (never
  "ficheiro"), and the retained **você**.

### Empty-queue button label (`fileOperations.transferProgress.background/backgroundAria`, 2026-08-09)

The progress dialog's one button in its second state: with an empty operation queue there's nothing to queue behind, so
it names the action instead of the destination ("Background" / "Queue"; same click, same F2).

- "Background" (the button label: put this transfer out of sight and keep it running) · **Em segundo plano** · MS
  terminology pt-BR maps the process-sense "background" to the prepositional phrase, not to a noun, in BOTH the
  adjective entry (id 18758 → "em segundo plano") and the noun-of-an-inactive-window entry (id 18784 → "em segundo
  plano"); Total Commander pt-BR phrases its own background actions the same way (`1185` "Download em segundo plano",
  `1189` "Enviar em segundo plano", `1190` "Apagar em segundo plano") · high. Reads as an elliptical command ("[deixe
  isto] em segundo plano"), which a bare **Segundo plano** would not: that's the noun and would title a section. ❌
  Never MS's wallpaper senses (**tela de fundo**, **papel de parede**), and ❌ never Double Commander pt-BR's
  abbreviated **2º plano**. Same length class as the sibling **Fila**, so the shared button doesn't reflow.
- "Keep this running in the background" (the accessible name) · **Manter isto rodando em segundo plano** · the shipped
  `queueTooltip` already says "Mantenha isto rodando em segundo plano"; the aria takes the infinitive to match its own
  sibling `queueAria` ("Enviar para a fila de operações"), the way every aria in this dialog names the action ·
  confirmed.
- **WCAG 2.5.3 containment**: the aria contains the visible label as the substring "em segundo plano" (case-insensitive
  on the initial E, exactly the bar English sets with "Background" ⊂ "…in the background"). Never reword one of the two
  without re-checking the other.
- Regional-variant check against the style guide's pt-PT tell list: zero hits; **rodando** (not pt-PT "a correr" / "está
  a correr") is the Brazilian marker, matching the shipped `queueTooltip` and `backgroundedToast`.
- No `sameAsSourceJustification` needed: both values differ from English.

### Quit-gate dialog terms (`main.quit.*`, 2026-08-10)

The modal Cmdr raises when the user quits while a copy, move, delete, trash, or archive edit is still going: title,
reassurance body, a list of running operations, a live countdown, and two buttons. Terminology is anchored to the
already-shipped `queue.*` strings, since the dialog reuses `queue.row.label` verbatim for its rows.

- quit (the app stopping) · **Encerrar** (gerund **Encerrando**; "quit now" → **Encerrar agora**) · macOS Finder pt-BR
  ("Encerrar Finder", "Encerrar Sem Salvar"), MS terminology pt-BR (`quit` id 1133557 → "encerrar"), and already the
  catalog's word via `commands.appQuit.label` "Encerrar Cmdr" · confirmed. `main.quit.title/countdown/quitNow`.
- "operations are running" (the state the dialog gates on) · **operações em andamento** · macOS Finder pt-BR carries
  this exact sentence: "O Finder não pode ser encerrado porque algumas operações ainda estão em andamento." (plus
  "…outra operação está em andamento…") · confirmed. Matches the shipped `queue.row.status` `running` arm ("Em
  andamento") and the glossary's operation → **operação** row, so the dialog, the queue window, and the row statuses all
  use one word. ❌ Not Dolphin/Double Commander's "em execução": the Tier-1 Finder wording wins, and the catalog already
  settled "Em andamento".
- "Still running" (heading above the operation rows) · **Ainda em andamento** · the Finder sentence's own "ainda … em
  andamento", trimmed to a heading · confirmed. Shares its head with the row statuses beneath it.
- "Keep working" (the button that calls the quit off entirely) · **Continuar trabalhando** · standard pt-BR
  continue-what-you-were-doing phrasing; no direct pile hit for this exact button, since no file manager in the pile has
  a quit gate · high. Deliberately NOT **Cancelar**: in this dialog a bare "Cancelar" would read as cancelling the
  _operations_, the opposite of what the button does. It also carries no postpone sense (❌ "Agora não", ❌ "Mais
  tarde", ❌ "Lembrar depois"), because the countdown is deleted, not deferred.
- "Quit now" · **Encerrar agora** · **agora** is load-bearing (the app quits either way when the countdown ends; this
  button skips the wait), and pt-BR carries it as naturally as English · high.
- restart / logout (the OS actions Cmdr must never hold up) · **reinicialização** / **encerramento da sessão** · MS
  terminology pt-BR (`restart` ids 99514/640295 → "reiniciar"; "reinicialização" is the standard pt-BR noun) and macOS
  pt-BR's own Apple-menu wording "Encerrar Sessão", already shipped in the catalog as `shortcuts.system.loggingOut`
  ("encerrar a sessão") · high. ❌ Not MS's Windows-flavored "fazer logoff" (term-choice principle 2: the macOS term
  wins). The sentence deliberately repeats the `encerr-` root ("Encerrando … o encerramento da sessão"); each word is
  the Finder-sourced term for its own concept, and swapping either for a synonym would fork terminology.
- "so a restart or logout never waits on Cmdr" · rendered actively with Cmdr as the agent: **para o Cmdr nunca atrasar
  uma reinicialização ou o encerramento da sessão** · a literal "nunca espera pelo Cmdr" puts the OS in the subject slot
  and reads heavier in pt-BR; the active form matches the catalog's running-text pattern of naming **o Cmdr** as the
  doer ("O Cmdr cuida da cópia automaticamente") · high.
- "anything still being written" · **O que ainda está sendo gravado** · **the body must stay number-neutral**: one
  operation writes several files at once and several operations can run at once, so "O único item ainda sendo gravado"
  states something false, and **O que** scopes it without a numeral. **gravado** is the shipped word for a
  partly-written transfer target (`fileOperations.transferProgress.stallInFlight` "já pode estar parcialmente gravado")
  · confirmed. ❌ Never a literal "em voo". "stops where it is" → **é interrompido onde está**: the natural-looking
  active "para onde está" garden-paths badly, since **para** is read first as the preposition.
- "what it leaves half-written" · **o que ficou gravado pela metade** · **pela metade** is verbatim from the shipped
  `settings.advanced.showStagingTempFiles.description` ("uma falha não pode deixar um arquivo pela metade com um nome
  real") · confirmed; the verbal form replaces the noun **arquivo pela metade**, which can't stay number-neutral. The
  **gravado** echo one clause later is deliberate, the same root-repetition call this section already makes for
  **encerr-**: each is the sourced term for its own concept.
- "on its own" (the countdown's aria label) · **sozinho** · agrees with **o Cmdr**, not with the user, so the gender
  rule is satisfied without the longer "por conta própria" · high. `main.quit.countdownAria` = "Tempo até o Cmdr
  encerrar sozinho". No WCAG 2.5.3 constraint here: the countdown region has no visible label key of its own, so this
  aria has nothing to contain.
- **Plurals**: both `main.quit.title` (`{count}`) and `main.quit.countdown` (`{seconds}`) write the full pt CLDR set
  `one`/`many`/`other`. `one` covers 0..1 and renders "uma operação" / "{secondsText} segundo"; the whole sentence is
  duplicated into each branch (mirroring the English), so nothing that agrees with the count sits outside. The visible
  numbers are the preformatted `{countText}` / `{secondsText}`; the raw `count`/`seconds` only select the branch.
- Regional-variant check against the style guide's pt-PT tell list: zero hits across all seven values (**arquivo**, not
  "ficheiro"; the gerunds **Encerrando** / **sendo gravado**, never "a encerrar" / "está a gravar"; no "consoante", no
  "Rever", no proclitic pronoun before an infinitive).
- No `sameAsSourceJustification` needed: all seven values differ from English.

### Usage stats: "anônimas" dropped, "um identificador aleatório" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `pseudônimo` / `pseudonimizado` —
that jargon is exactly what the copy avoids.

- **usage stats → `estatísticas de uso`** · already the catalog's term (`onboarding.stepBeta.emailNote`); only the
  `anônimas` adjective was cut. MS terminology pt-BR agrees (usage data → `dados de uso`) · high
- **a random id → `um identificador aleatório`** · MS terminology pt-BR (random → `aleatório`, identifier →
  `identificador`) · high. Ordinary Portuguese, not jargon.
- **tied to → `ligado a`** · the catalog's own verb (`onboarding.stepBeta.emailNote` "nunca é ligado às suas
  estatísticas de uso") · high
- `emailPrivacyNote` now writes `e-mails` (hyphenated), matching the rest of the pt catalog; the old value had a bare
  `emails`.
- No `sameAsSourceJustification` needed: every value differs from English.

### Confirmação de reversão e a linha que espera resposta (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`, 2026-08-13)

O botão `Reverter` de uma cópia ou movimentação em andamento agora pede confirmação, e uma linha da `Fila de operações`
ganha um status próprio quando para porque há uma pergunta esperando na janela principal.

- "Needs your answer" (status da linha) · **Precisa de resposta** · o único acerto direto do pile no conceito é o Double
  Commander pt-BR (`Waiting for user response` → "Aguardando resposta do usuário"), inutilizável aqui: começa com
  **Aguardando**, exatamente a arm `queued` de `queue.row.status`, e a `@key` exige que os dois não se confundam · high
  em **resposta**, `tentative` na forma. **Precisa de** mantém o tom amigável do catálogo (mais quente que "Requer
  resposta" ou "Resposta necessária") e cabe na coluna estreita ao lado de "Não foi possível concluir".
- `awaitingAnswerTooltip` · **Responda à pergunta na janela principal e esta operação continua.** · o verbo
  **responder** vem do irmão `operationConflict.pausedNote` ("até você responder"), **janela principal** já é o termo do
  catálogo (`shortcuts.scope.mainWindow`, `queue.row.foregroundAria`), e "prompt" vira **pergunta**, a palavra com que o
  próprio diálogo se descreve · high. Imperativo de sujeito implícito, conforme o style guide.
- `rollbackConfirm.title` · **Reverter esta operação?** · todo título de diálogo sim/não no catálogo é infinitivo
  ("Excluir modelo de IA?", "Remover {hostName} da lista de servidores?") · high; **Reverter** é o termo já fixado para
  rollback.
- `rollbackConfirm.body` · **Isso apaga todos os arquivos que a operação gravou até agora. O que foi substituído não
  volta.** · **gravar** é a palavra do catálogo para escrever um arquivo de destino (`stallInFlight` "parcialmente
  gravado", `main.quit.body`), **até agora** é a forma fixa de "so far" (`search.imageResults.paused`), **substituir** é
  macOS Tier 1 para `Replace`, e **apagar** é o `delete` fixado no glossário · high. A segunda frase usa a relativa
  livre **O que foi substituído** para ficar neutra em número (o inglês "any file" também não fala de um arquivo
  específico) e para não precisar do pronome de **a operação**. **Isso** (33 ocorrências no catálogo) e não "Isto" (3).
- `rollbackConfirm.keep` ("Keep them", a resposta segura) · **Manter os arquivos** · macOS Finder pt-BR usa a forma
  `Manter <substantivo>` ("Manter Ambos", "Manter Original", "Manter Cópia Parcial", "Manter Downloads") · high. O
  objeto é escrito por extenso em vez de **Mantê-los**: a última frase do corpo fala dos arquivos SUBSTITUÍDOS, então o
  pronome poderia apontar para o referente errado.
- `rollbackConfirm.rollBack` · **Reverter** · exatamente o botão que abriu o diálogo
  (`transferProgress.conflictRollback`), como a `@key` pede · high.
- `transferProgress.rollbackTooltip` (novo inglês: "Stop, and delete every file written so far") · **Parar e apagar
  todos os arquivos gravados até agora** · **Parar** é o verbo do catálogo para interromper trabalho em curso (`queryUi`
  "Parar a busca") e mantém a dica longe de **Cancelar**, que é o que a `@key` proíbe evocar · high. Sem vírgula antes
  do **e**, ao contrário do inglês.
- `transferProgress.foregroundBusyToast` (novo inglês: "Something else is open here. Close it, then bring this one up.")
  · **Há outra coisa aberta aqui. Feche-a e depois traga esta para a frente.** · o novo inglês evita de propósito
  afirmar que o bloqueio é outra OPERAÇÃO (pode ser um diálogo de nova pasta ou uma confirmação de exclusão), então a
  abertura antiga "Outra operação …" tinha virado falsa · high. Ênclise em **Feche-a** (marca pt-BR, como "Encontre-a na
  fila de operações"); **esta** concorda com **operação**.
- Verificação regional contra a lista de indícios pt-PT do style guide (ficheiro, `estar a` + infinitivo, consoante,
  próclise antes de infinitivo, Rever, alterar o nome, **você** omitido): zero ocorrências. Marcas brasileiras:
  **arquivos**, **gravou**, a ênclise **Feche-a**.
- Nenhum `sameAsSourceJustification` necessário: os oito valores diferem do inglês.

### Cadeia de renomeação: o aviso que cresce (`fileExplorer.rename.chainKeptOriginalNameAndOthers`, 2026-08-18)

O mesmo toast de `fileExplorer.rename.chainKeptOriginalName`, reescrito a cada arquivo que mantém o nome: nomeia o mais
recente e conta os anteriores.

- "kept its name" · **manteve o nome** · valor já publicado no irmão `chainKeptOriginalName`; macOS Finder pt-BR apoia a
  família `Manter` ("Manter Original", "Manter Ambos") · confirmed. As duas chaves são uma frase só, então o verbo, as
  aspas retas em volta de `{name}` e o ponto depois de `{reason}` são idênticos nos dois valores.
- "and ^0 other items" (o sintagma contado) · **outros {N} itens/arquivos**, com **outros** ANTES do numeral · macOS
  Finder pt-BR, referência cruzada por chave em `LocalizableMerged.json`: `MR201_V3` "Sending “^1” and ^0 other items."
  → "Enviando “^1” e outros ^0 itens.", `MR101_V3` (Receiving) e `PE106_V4` (Merge) na mesma forma · confirmed. ❌ Não
  "{N} outros arquivos": o GNOME Nautilus pt-BR usa essa ordem ("%'d outros itens selecionados"), mas o Finder é Tier 1
  e a frase dele ("nome" + e N outros itens) é a mesma estrutura desta, então a ordem do Finder ganha.
- "and so did …" (a elipse que retoma o verbo) · **assim como …** · construção padrão do português para retomar o
  predicado sem repeti-lo; o pile tem um uso da mesma construção no macOS ("O sistema trata os itens com nomes assim
  como arquivos invisíveis"), mas no sentido comparativo, então a evidência direta é da gramática, não do pile · high.
  Escolhida em vez de "e … também" (o par "e … também" fica redundante em texto de UI) e de "e o mesmo aconteceu com …"
  (longo demais para um toast).
- "one other file" · **outro arquivo**, sem numeral · o inglês escreve "one" por extenso; o pt-BR resolve com o próprio
  **outro**, seguindo o padrão já publicado de ramos `one` sem número (`one {uma vez}`, `one {arquivo}`) · high.
- Plural: ramos `one` / `many` / `other` (o `many` do CLDR pt pega números grandes: 1.000.000 seleciona `many`,
  verificado com `intl-messageformat` em `pt`). Tudo o que concorda com o substantivo contado (**outro** / **outros**,
  **arquivo** / **arquivos**) fica DENTRO dos ramos; fora do plural sobra só o ponto final.
- Verificação regional contra a lista de indícios pt-PT do style guide: zero ocorrências. Marca brasileira: **arquivo**.
- Nenhum `sameAsSourceJustification` necessário: o valor difere do inglês.

### Renomeação sem confirmação e nome recusado (`fileExplorer.rename.unconfirmed*` + `fileOperations.validation.nameNotUsable`, 2026-08-18)

O par irmão de `chainKeptOriginalName*`: mesma forma de toast, situação oposta. `chainKept*` afirma que o arquivo
manteve o nome; `unconfirmed*` diz que o Cmdr NÃO sabe, e que a renomeação pode muito bem ter acontecido. Nunca
embaralhe os dois sentidos.

- "Couldn''t confirm …" · **Não foi possível confirmar …** · a voz de "couldn''t/failed" já fixada na seção
  Error-copy-phrasings, e o valor já publicado em `fileExplorer.pane.trashUnconfirmedToast` ("Não foi possível confirmar
  que o arquivo foi movido para a Lixeira.") · confirmed. As duas chaves de renomeação reusam essa abertura, então os
  três toasts de "não deu para confirmar" soam iguais.
- "the rename of X" · **a renomeação de X** · linha `renomeação` do glossário (substantivo de `Renomear`) · high. ❌
  Nunca "a alteração de nome" (forma pt-PT).
- "The volume may be slow" · **O volume pode estar lento** · o valor já publicado em `trashUnconfirmedToast` é
  literalmente essa oração; **volume** = volume no macOS Finder pt-BR (`LocalizableMerged.json`: "O volume de destino
  está bloqueado.", "O volume tem um formato incorreto…"), **lento** é o adjetivo padrão do pile (MS terminology
  "conexão mais lenta"; GNOME "A busca pode ser lenta…") · confirmed. O inglês hesita ("may be"), então o português
  também: nunca afirme "O volume está lento".
- "the rename may still have gone through" · **a renomeação pode ter sido concluída mesmo assim** (plural **as
  renomeações podem ter sido concluídas mesmo assim**) · o padrão "pode ter sido {particípio} mesmo assim" já publicado
  em `trashUnconfirmedToast` ("o arquivo pode ter sido movido mesmo assim", de um "may still have been moved" idêntico);
  **Concluída** é o termo do Finder para completado · confirmed. **mesmo assim** é a tradução estabelecida desse
  "still", não "ainda assim".
- ⚠️ **Não diga "o arquivo pode ter sido renomeado".** O que está sendo renomeado pode ser uma pasta, e o inglês evita
  de propósito nomear file/folder na primeira frase. O sujeito é a renomeação, não o item.
- **O substantivo é repetido na segunda frase, sem pronome**
  (`… a renomeação de "{name}". O volume pode estar lento, então a renomeação pode ter sido concluída…`), acompanhando o
  inglês, que também repete "the rename"/"the renames", e o irmão `trashUnconfirmedToast`, que repete "o arquivo". Um
  retomador `ela`/`elas` ficaria ambíguo na chave `AndOthers`, onde o núcleo singular convive com "arquivos" no plural.
  Por isso a `AndOthers` mantém **a renomeação** (singular) na primeira frase e usa **as renomeações** (plural) na
  segunda, exatamente como o inglês. O sintagma contado copia o ramo do irmão `chainKeptOriginalNameAndOthers`:
  `one {outro arquivo}` / `many` e `other` {outros {othersText} arquivos}, com **outros** antes do numeral (macOS Finder
  `MR201_V3`).
- "That filename/folder name can''t be used" · **Esse nome de arquivo / Esse nome de pasta não pode ser usado** · macOS
  Finder pt-BR é a fonte direta e é da própria família de renomear: `RN31` "O nome “^0” não pode ser usado.", `NE74`
  "…porque é muito longo.", `RN5` "…porque foi reservado pelo sistema." · confirmed. O demonstrativo **Esse** traduz o
  "That" do inglês (aponta para o nome que a pessoa acabou de digitar), e os substantivos **arquivo**/**pasta** seguem
  os irmãos `validation.empty` / `.disallowedChars` / `.nameTooLong`. Sem ponto final: o valor também entra composto em
  `{reason}` de `chainKeptOriginalName*` ("Esse nome de arquivo não pode ser usado. "notas.txt" manteve o nome.").
- Verificação regional contra a lista de indícios pt-PT do style guide: zero ocorrências (arquivo, não ficheiro; sem
  `estar a` + infinitivo; sem próclise em infinitivo).
- Nenhum `sameAsSourceJustification` necessário: os três valores diferem do inglês.

## Operações sugeridas: a janela do que o Ask Cmdr propõe (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (as operações de arquivo propostas pelo agente) → `operações`; título `Operações sugeridas` · segue o termo da
  casa ("File operations" → "Operações de arquivo") · high
- approve → `Aprovar` · padrão; a pilha de referência não traz "approve" em pt · tentative
- reject → `Recusar` · padrão; a pilha só traz `Aceitar` (Nautilus/Dolphin) e nenhum par para "reject" · tentative
- "This can't be undone" → `Esta ação não pode ser desfeita` · macOS, palavra por palavra · high
- suggestion → `sugestão` · já no catálogo (`askCmdr`) · high

## Duplicar: o comando que copia na mesma pasta (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (comando que copia a seleção dentro da própria pasta) → `Duplicar`** · macOS Finder pt-BR, menu "Arquivo >
  Duplicar" (`N154`), além de "Duplicar Itens" e "Duplica itens nas suas localizações atuais" (verificado no macOS
  26.6.1, `Finder.app/Contents/Resources/pt_BR.lproj`, 2026-08-19) · high. Convive com `Copiar` (F5) e `Mover` (F6).
- **"Make a copy of the selected files in the same folder" → `Faça uma cópia dos arquivos selecionados na mesma pasta`**
  · imperativo, como as descrições vizinhas ("Copie os arquivos selecionados…"); "mesma pasta" é a pasta onde os
  arquivos já estão · high.

## Menus nativos: barra de menus, menus de contexto, títulos de janela (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

Fontes de todo este grupo: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/pt_BR.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) é Tier 1 e decide quase tudo; o lado inglês está em `en_GB.lproj`, porque `Base.lproj` só
traz nibs compilados. O Safari 26 (`MainMenu.strings`, pasta `pt.lproj` = brasileiro) dá o vocabulário de abas, e a
terminologia da Microsoft o que a Apple não nomeia. Família RAW: **apóstrofos simples**, um `''` apareceria duplicado no
menu.

- **Barra de menus → `Arquivo`, `Editar`, `Visualizar`, `Ir`, `Janela`, `Ajuda`, `Serviços`** · macOS Finder e Safari
  `pt-BR` · high.
- **Menu Select (seleção de arquivos) → `Selecionar`** · Nautilus/Thunar/Dolphin `pt-BR` · high. O Finder não tem
  equivalente.
- **O Finder brasileiro usa Title Case nos menus („Nova Pasta”, „Mover para o Lixo”); o Cmdr NÃO.** O catálogo `pt`
  inteiro já está em sentence case, e a regra do `docs/style-guide.md` vale para todos os idiomas, então os rótulos são
  `Nova pasta…`, `Fechar aba`, `Mostrar arquivos ocultos`. Só o TERMO vem do Finder, não a capitalização.
- **Quick Look → `Visualização rápida`** · macOS Finder (`TL14`) · high. A Apple traduz esse nome de recurso, por isso
  ele não está na lista de não-traduzir.
- **Get Info → `Obter informações`, Go > Home → `Pasta pessoal`, Sort By → `Ordenar por`, Default → `Padrão`, Other… →
  `Outro…`** · macOS Finder Tier 1 · high.
- **zoom in / out → `Ampliar` / `Reduzir`** · Safari `pt-BR` (menu Visualizar) · high.
- **ascending / descending → `Crescente` / `Decrescente`** · Thunar + Dolphin `pt-BR` · high.
- **changelog → `Log de alterações`** · terminologia da Microsoft · high. Distinto de Ajuda > `Novidades`: um nomeia o
  documento, o outro a notícia.
- **word wrap → `Quebra automática de linha`** · terminologia da Microsoft · high.
- **pin / unpin tab → `Fixar aba` / `Desafixar aba`** · Safari `pt-BR` („Fixar Aba”) · high.
- **Cores de etiqueta do Finder → `Vermelho, Laranja, Amarelo, Verde, Azul, Roxo, Cinza`** · macOS Finder (`TG_COLOR_*`)
  · high.
- **busy (volume em uso) → `(ocupado)`** · terminologia da Microsoft · high.
- **Eject → `Ejetar`, Disconnect → `Desconectar`, Remove (de uma lista) → `Remover`** · macOS Finder · high. `Apagar`
  fica reservado para arquivos, como manda o `style.md`.
- **Idênticos ao inglês de propósito** (com `sameAsSourceJustification`): `menu.view.zoom`, `menu.window.zoom`,
  `menu.zoom.percent*`, `menu.view.askCmdr`.

### Aviso de conexão pelo sistema (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

A notificação que aparece quando o Cmdr não conseguiu abrir a própria conexão direta e o compartilhamento ficou na
conexão que o macOS oferece. É tranquilizadora, não alarmante: o compartilhamento funciona, só está lento.

- "Couldn''t directly connect to X" · **Não foi possível conectar diretamente a X** · a abertura "Não foi possível …" já
  fixada na seção Error-copy-phrasings, e `fileExplorer.network.share.connectFailedTitle` ("Não foi possível conectar a
  {hostName}") dá a regência `conectar a` · confirmed. O advérbio fica colado ao verbo (`conectar diretamente`),
  acompanhando `fileExplorer.navigation.connectingDirectly`.
- "You are connected" · **Você está conectado** · macOS Finder pt-BR usa exatamente essa forma ("Você já está conectado
  a este servidor, o qual não permite múltiplas conexões…", `LocalizableMerged.json`) · confirmed. É o masculino não
  marcado que o `style.md` autoriza quando não dá para reestruturar; aqui a Apple é a própria fonte.
- native (conexão do sistema operacional) · **nativa** · uso corrente pt-BR no pile (`modo nativo`, `aplicativo nativo`,
  `autenticação nativa`) · high. "conexão de rede SMB nativa do macOS".
- "4x slower" / "(sometimes 100x)" · **4x mais lenta** / **(às vezes 100x)** · o multiplicador em pt-BR se escreve
  colado ao numeral, sem espaço nem `×`; **mais lenta** vem de `conexão mais lenta` (terminologia da Microsoft) · high.
  Comparativo com **do que** (`4x mais lenta do que a conexão direta do Cmdr`), a forma cuidada do pt-BR.
- "Click the button below" · **Clique no botão abaixo** · o padrão `clique em … abaixo` já publicado em
  `onboarding.stepFda.step1`/`postAction.body` · high. E "to try again" → **para tentar novamente**, o fecho do Finder
  pt-BR ("Desbloqueie o disco e tente novamente.") e o valor já publicado em `fileExplorer.network.retry`.
- "Try connecting directly" (botão) · **Tentar conectar diretamente** · casa com
  `fileExplorer.navigation.connectDirectly` ("Conectar diretamente para acesso mais rápido") e com
  `fileExplorer.network.retry` ("Tentar novamente") · high. O botão é curto de propósito: o "para acesso mais rápido" do
  item de menu já está explicado no corpo do aviso.
- "Dismiss" (fechar o aviso) · **Dispensar** · a linha `dismiss` do glossário (seção do chip de progresso), com seis
  ocorrências no catálogo · confirmed. ❌ Não reusar o **Descartar** de `lowDiskSpace.toast.closeTooltip`: aquele é a
  inconsistência já sinalizada naquela seção, não o termo fixado.
- **A ordem da oração muda**: o inglês diz "4x slower for most connections (sometimes 100x) than …"; em português o
  adjunto vem antes do comparativo
  (`que, na maioria das conexões, é 4x mais lenta do que a conexão direta do Cmdr (às vezes 100x)`), porque separar
  "mais lenta" do seu "do que" trava a leitura.
- Marcadores brasileiros do lote: **compartilhamento** (nunca "partilha"), **conectado/conectar** (nunca "ligado"),
  gerúndio nenhum a conferir aqui. Varredura pt-PT (ficheiro, `estar a` + infinitivo, consoante, próclise, Rever,
  alterar o nome, você omitido): zero ocorrências. Nenhum valor precisa de `sameAsSourceJustification`.

### Recusas de renomear e criar: as 31 mensagens de uma linha (`errors.mutation.*` + `errors.volume.*`, 2026-08-23)

A mensagem única que aparece sob o campo de nome (ou num aviso rápido) quando um Renomear, Nova pasta ou Novo arquivo é
recusado. Família RAW: apóstrofos simples, `{path}` é um marcador literal e um insert não controlado (caminho completo,
qualquer script), então nenhuma frase depende do gênero, do número ou da inicial dele.

- **System Integrity Protection → `Proteção de Integridade do Sistema`** · macOS Finder pt-BR, `LocalizableMerged.json`
  `ET6`: "Alguns itens no Lixo não podem ser apagados devido à Proteção de Integridade do Sistema." · confirmed. Nome de
  recurso que a Apple traduz, com artigo ("com a Proteção…"), então não fica na lista de não-traduzir.
- **volume root / top folder → `a pasta raiz de um volume`** · macOS Finder pt-BR (`SC11` "Nenhuma pasta raiz encontrada
  para o item especificado.") · high. ❌ Não "pasta superior": esse é o termo da NAVEGAÇÃO para subir um nível
  (`commands.navParent.label`), e aqui o sentido é o topo do disco, não o pai da pasta atual.
- **"can't write into X" → `não consegue gravar em X`** · **gravar** é o verbo do catálogo para escrever num destino
  (`transferProgress.titleFlushing`, `main.quit.body`, `permissionDenied.suggestion.default` "acesso de gravação") ·
  confirmed.
- **"Unlock it in Finder's Get Info panel" → `Desbloqueie-o no painel Obter Informações do Finder`** · reusa o valor já
  publicado em `errors.write.fileLocked.suggestion.mac` ("Desbloqueie-o no Finder (Obter Informações > desmarque
  Bloqueado)"); a Apple usa a mesma receita em `NE43` · confirmed. Mantém a capitalização da Apple no nome do painel
  (**Obter Informações**), ao contrário dos itens de menu do próprio Cmdr, que ficam em sentence case.
- **"This volume is read-only." → `Este volume é somente leitura.`** · macOS Finder pt-BR `FI12` "Esta localização é
  somente leitura." · confirmed. **"doesn't support that" → `não oferece suporte a isso`**, do irmão já publicado
  `errors.write.trashNotSupported.message` ("Este volume não oferece suporte ao Lixo.") · confirmed.
- **"Only zip archives can be changed" → `Só arquivos zip podem ser alterados`** ·
  `fileExplorer.readOnly.archiveMessage` já publica exatamente essa oposição ("O Cmdr navega e extrai arquivos tar e 7z,
  mas somente arquivos zip podem ser editados") · confirmed. Aqui **zip** qualifica o formato, então o núcleo é o
  simples **arquivo zip**; o **arquivo compactado** do glossário fica para quando o formato não é nomeado
  (`archiveNotEditable`, `needsPassword`, `archiveEditCouldntStart`).
- **"Renaming can't take an item out of / from one archive to another" →
  `A renomeação não pode tirar um item de um arquivo compactado` /
  `… não pode levar um item de um arquivo compactado para outro`** · o substantivo **renomeação** é a linha do
  glossário; **Use Mover para isso** nomeia o comando (macOS Finder `Mover`) em vez de mandar mover o item, que soaria
  como uma instrução ambígua dentro de um campo de nome · high.
- **`timedOut` NÃO é uma falha** · `O volume ainda não respondeu, então a alteração ainda pode ser concluída.` A
  operação não foi cancelada e ainda pode dar certo, então o **ainda pode ser concluída** é obrigatório; ❌ nunca "não
  foi possível" nem "não deu certo" aqui. **Concluída** é o termo do Finder para completado.
- **`deviceSessionReset` NÃO é desconexão** ·
  `O dispositivo reiniciou a conexão. Espere alguns segundos e tente de novo.` O celular MTP continua conectado. A
  segunda frase é o valor já publicado em `errors.listing.deviceReconnecting.suggestion` ("Espere alguns segundos e
  tente de novo."), e a primeira ecoa a explicação do mesmo par ("A conexão com o dispositivo … foi reiniciada") ·
  confirmed. ❌ Nada de "desconectado" ou "desconecte o cabo": o irmão fecha justamente com "Não é preciso desconectar
  nada."
- **"lost track of the destination folder" → `perdeu a referência da pasta de destino`** · o catálogo já descreve um
  handle morto como **referência** (`errors.listing.staleConnection.explanation` "usando uma referência antiga que o
  servidor não reconhece mais") · confirmed. ❌ Não "perdeu o controle" (soa como "lost control", alarmante; mesma
  armadilha registrada na seção do índice de imagens). O fecho "Abra a pasta de novo e tente outra vez" copia
  `errors.write.destinationNotFound.suggestion`.
- **"on its way out … has it open" → `está a caminho da saída, e algo ainda o mantém aberto`** · valor irmão já
  publicado em `errors.write.deletePending.message` / `errors.listing.deletePending.explanation` · confirmed. O
  "something" do inglês fica em **algo** de propósito: pode ser outro app ou o próprio servidor, e o irmão longo já
  explica o identificador aberto.
- **"Something went wrong, and Cmdr couldn't tell what." →
  `Algo deu errado, e o Cmdr não conseguiu identificar o quê.`** · **Algo deu errado** é a frase já fixada no catálogo
  em quatro lugares (`ai.cloud.genericError`, `licensing.error.generic`, `onboarding.cloudSetup.status.genericError`,
  `askCmdr.error.provider`) · confirmed. Não usa o substantivo "erro", então respeita a regra de voz. **o quê** com
  circunflexo por estar no fim da frase.
- **"at your request" → `a seu pedido`** · `O Cmdr parou isso a seu pedido.` **Parar** é o verbo do catálogo para
  interromper trabalho em curso (`queryUi` "Parar a busca", `transferProgress.rollbackTooltip`), e **isso** (não "isto")
  segue a contagem do catálogo · high. Neutro, sem pedido de desculpas: nada deu errado.
- **"The destination can't hold that name." → `O destino não consegue armazenar esse nome.`** · reusa o verbo do irmão
  `errors.write.invalidName.message` ("um nome que o destino não consegue armazenar") · confirmed. O conserto é sempre
  outro nome, nunca repetir: **Escolha outro.**
- **"That password didn't work." → `Essa senha não funcionou.`** · `fileExplorer.smbReauth.savedPasswordFailed` ("Sua
  senha salva não funcionou.") · confirmed. Culpa a senha, não a pessoa. **password-protected → `protegido por senha`**
  (linha do glossário, diálogo de senha de zip).
- **"the change" (a renomeação/criação pedida) → `a alteração`** · usado em `timedOut` e em `deviceDisconnected` ("antes
  de a alteração ser concluída") · high. Distinto de **as mudanças** do sistema de arquivos (`fileExplorer.imageIndex`),
  que é o outro sentido de "changes" no catálogo.
- Verificação regional contra a lista de indícios pt-PT do style guide (ficheiro, `estar a` + infinitivo, consoante,
  próclise antes de infinitivo, Rever, alterar o nome, **você** omitido), mais varredura de U+2019, apóstrofo duplo e
  espaço duplo: zero ocorrências nos 31 valores. Marcas brasileiras: **arquivo** (nunca "ficheiro"), a ênclise
  **Desbloqueie-o**, **tente de novo**.
- Nenhum `sameAsSourceJustification` necessário: os 31 valores diferem do inglês.
- Duas inconsistências vizinhas, sinalizadas aqui e **corrigidas em 2026-08-24**:
  `fileOperations.transferDialog.compressLevelCaption` e o bolsão de **"drive"** em `errors.json`. Ver § O bolsão de
  `drive` fechado, no fim deste arquivo.

### Recusas de mover para o Lixo: as duas mensagens de uma linha (`errors.mutation.trash*`, 2026-08-23)

Mesma superfície das 31 recusas acima (linha única sob o campo de nome ou num aviso rápido), família RAW, sem ICU.

- **"This volume has no Trash." → `Este volume não tem Lixo`** · **Lixo** é o valor Tier-1 do Finder pt-BR e já está
  fixado neste glossário; o irmão publicado `errors.write.trashNotSupported.message` diz "Este volume não oferece
  suporte ao Lixo", mas o inglês novo trocou "doesn't support" pelo **has no**, mais simples, e o **não tem** acompanha
  esse registro · high. **"the only way is to delete permanently" → `então a única opção é apagar permanentemente`** ·
  **apagar permanentemente** é a linha do glossário (verbo do Finder), idêntico ao fecho já publicado em
  `errors.write.trashNotSupported.suggestion` ("para apagar permanentemente") · confirmed. O **então** liga as duas
  orações como no resto da família `errors.mutation.*`.
- **"macOS wouldn't move this to the Trash." → `O macOS se recusou a mover este item para o Lixo.`** · **recusar** é o
  verbo do pile para uma recusa do sistema/servidor (Nautilus pt-BR "O servidor recusou a conexão", Double Commander
  "Download recusado"), e é o sentido exato do "wouldn't" (o sistema negou, não é uma falha do Cmdr) · high. Próclise
  **se recusou** (pt-BR), não a ênclise "recusou-se". **mover … para o Lixo** é o verbo do Finder pt-BR ("Mover para o
  Lixo", "não pode ser movido para o Lixo"). O "this" vira **este item**, o substantivo que o Finder usa nessas frases
  ("O item '^1' não pode ser movido para o Lixo porque…"), em vez do pronome solto. ❌ Não "não permitiu": isso soa a
  falta de permissão, que é outra família de mensagens. A frase fica curta de propósito, porque o motivo técnico aparece
  em "Detalhes técnicos".
- Varredura pt-PT (ficheiro, `estar a` + infinitivo, consoante, próclise antes de infinitivo, Rever, alterar o nome),
  mais U+2019, apóstrofo duplo e espaço duplo: zero ocorrências nos dois valores. Nenhum `sameAsSourceJustification`
  necessário.

### Diálogo de falha: as três aberturas (`crashReporter.dialog.body.ended`/`keptRunning`/`unknown`)

O diálogo do próximo lançamento agora escolhe uma de três frases conforme o que o relatório registrou. As três abrem com
**O Cmdr** e carregam **da última vez**, e só a segunda oração muda; isso é o paralelismo que faz a diferença entre elas
ficar visível.

- "Cmdr ran into a problem" · **O Cmdr teve um problema** · o substantivo **problema** é o termo já fixado (linha "error
  report" → "relatório de problema"; guia de estilo pt-BR da Microsoft prescreve "Houve um problema." para uma abertura
  desse tipo, e o Finder pt-BR usa "houve um problema com a unidade de disco", `LocalizableMerged.json` `PE37`) · high.
  O verbo **ter** entra porque aqui o Cmdr é o sujeito (as fontes usam a forma impessoal "houve"), e `ter um problema` é
  a regência natural do pt-BR nessa posição. ❌ Não usar **falha** nesta frase: `falha` é a palavra do crash (linha
  "crash report"), e estas duas chaves existem justamente porque nada travou. A colocação exata não tem atestação no
  pile (`teve/ocorreu/encontrou um problema`: zero ocorrências).
- "and kept running" · **e continuou funcionando** · high. Diz que o app seguiu utilizável, sem afirmar que ele parou,
  fechou ou encerrou. ❌ Não **continuou em execução**: `em execução` existe no pile (Dolphin pt-BR "ainda está em
  execução", Double Commander pt-BR "TC ainda está em execução"), mas é o registro técnico de aviso, e o `crashReporter`
  é tranquilizador. ❌ Não **continuou rodando**: colidiria com "rodando em segundo plano" do catálogo
  (`transferProgress.backgroundedToast`) e faria parecer que o app seguiu _em segundo plano_. ❌ Não **travou** nem
  **parou** (linha do aviso de transferência parada): ambos leem como falha.
- "in the background" (a tarefa que teve o problema) · **em segundo plano** · a linha "background / send to background"
  do glossário, mais terminologia da Microsoft pt-BR (`background` adjetivo → "em segundo plano"; `background task` →
  "tarefa em segundo plano") e Total Commander pt-BR (`1237` "operações ativas em segundo plano") · confirmed.
- "Here''s a report with details that can help fix this" · **Aqui está um relatório com detalhes que ajudam a corrigir
  isso** · é a segunda frase já publicada em `crashReporter.dialog.body.ended`, menos o **de falha** · confirmed. O
  inglês também trocou "a crash report" por "a report" nas duas chaves novas: nada falhou, então o relatório perde o
  qualificador. `relatório` sozinho continua correto (terminologia da Microsoft: "Relatório de Erros do Windows").
- **A chave `unknown` não pode dizer nem uma coisa nem outra**: ela sai para relatórios escritos por versões antigas do
  Cmdr, que não registravam se o app seguiu rodando. Por isso ela fica só com "O Cmdr teve um problema da última vez." —
  sem `encerrou`, sem `continuou`, verdadeira nos dois casos.
- Marcadores brasileiros do lote: o gerúndio **funcionando** (nunca "a funcionar"). Varredura pt-PT (ficheiro, `estar a`
  - infinitivo, consoante, próclise antes de infinitivo, Rever, alterar o nome, você omitido): zero ocorrências. Nenhum
    valor precisa de `sameAsSourceJustification`.

## O texto do ajuste de relatórios agora vale para os dois casos (`settings.updates.crashReports.description`)

O botão também envia um relatório quando um problema em segundo plano NÃO encerrou o app, então a ajuda não pode mais
falar só de fechamento. Tudo vem da seção do diálogo de falha acima, no presente:

- **`quando o Cmdr encerra de forma inesperada`** vem do verbo de `crashReporter.dialog.body.ended`
  (`encerrou de forma inesperada`), no lugar do `fecha de forma inesperada` que esta chave ainda trazia: as duas telas
  passam a dizer o mesmo verbo para o mesmo desfecho · high.
- **`tem um problema em segundo plano`** vem de `.keptRunning` · high. O presente é morfologia, não uma decisão de termo
  nova.
- **`um relatório`** sem `de falha`, porque a frase cobre os dois casos · high. ❌ O RÓTULO
  `settings.updates.crashReports.label` continua `Enviar relatórios de falha`: é o nome do ajuste.
- **Segunda frase tirada de `crashReporter.dialog.privacyNote`** (`qual parte do código teve o problema`), no lugar de
  `o local da falha`, verdadeiro só quando algo falhou · high.

### Recusas de ejetar e desconectar: as nove mensagens do aviso rápido (`errors.eject.*`, 2026-08-23)

Cada valor entra num aviso rápido DEPOIS de dois pontos: `fileExplorer.pane.ejectFailedToast` ("Não foi possível ejetar
{volumeName}: …") ou `fileExplorer.pane.disconnectFailedToast` ("Não foi possível desconectar: …"). Família RAW, sem
ICU, apóstrofos simples (nenhum dos nove precisa de apóstrofo). O aviso é pequeno, então cada valor fica em uma ou duas
frases curtas.

- **A frase depois dos dois pontos começa com maiúscula**, como o inglês: cada valor é uma oração completa e o invólucro
  não sabe qual dos nove vai cair ali · high.
- **"is being used" (o volume ocupado) → `está usando` / `usando este disco`** · macOS Finder pt-BR é a fonte direta
  para toda essa família: "O volume não pode ser ejetado porque está sendo usado atualmente.", "Você não pode ejetar
  “^0” porque ele está sendo usado.", "Há um disco em “^0” que está em uso e não pode ser ejetado."
  (`LocalizableMerged.json`) · confirmed. Em `unmountRefused` o sujeito é o **algo** do inglês (linha já fixada em
  `errors.mutation`), então a voz fica ativa: `Algo ainda está usando este disco.` ❌ O `(ocupado)` do glossário é o
  rótulo curto do alternador de volumes, não entra em frase corrida.
- **"Close any open files and apps" → `Feche os arquivos e aplicativos abertos`** · o Finder pt-BR fecha a mesma receita
  com "Encerre todos os aplicativos abertos e tente novamente." e "Talvez alguns arquivos desses discos estejam sendo
  usados." · high. O inglês pede arquivos E apps, então os dois entram num só sintagma; **depois ejete-o de novo** usa a
  ênclise pt-BR e o `de novo` que o resto do `errors.json` já usa ("tente de novo", "monte-o de novo").
- **"isn't removable" → `não é removível`** · macOS Finder pt-BR ("Removível", "Volume Removível"), Thunar pt-BR
  ("Unidade removível"), Total Commander pt-BR ("Disco removível"), terminologia da Microsoft (`removable` → removível)
  · confirmed. O fecho **então ele continua conectado** reusa o `continua conectado` já publicado em
  `errors.listing.deviceReconnecting.explanation`.
- **"isn't connected any more" → `não está mais conectado`** · macOS Finder pt-BR "Não foi possível concluir a operação
  porque o disco “^0” não está mais disponível." dá o **não está mais**; **conectado** é a linha Connect/Disconnect do
  glossário · high. `Esse disco` (não "este"): o disco já sumiu, então o demonstrativo se afasta.
- **"network share" → `compartilhamento de rede`** · terminologia da Microsoft (`network share` → "compartilhamento de
  rede") e o valor já publicado em `errors.listing.remotePermissionDenied.explanation` ("está em um compartilhamento de
  rede") · confirmed. O "This" vira **Este item**, o substantivo que o Finder usa nessa posição (mesma decisão
  registrada na seção das recusas de mover para o Lixo), porque o alvo pode ser um disco local ou um celular MTP, e
  "Este volume" prejulgaria isso.
- **"wouldn't close its connection" → `se recusou a encerrar a conexão`** · **recusar** é o verbo do pile para uma
  recusa do sistema/dispositivo (Nautilus pt-BR "O servidor recusou a conexão", Finder pt-BR "“^0” recusou seu
  pedido."), a mesma escolha já registrada em `errors.mutation.trashRefused` · high. A colocação `encerrar a conexão`
  não tem atestação no pile (zero ocorrências); **encerrar** é o verbo do catálogo para terminar algo em curso
  (`main.quit`, "Encerrar Cmdr") e é a regência natural do pt-BR aqui.
- **"Unplug it" → `Desconecte-o`** · o catálogo já equipara unplug e desconectar em
  `errors.listing.deviceReconnecting.suggestion` ("There's nothing to unplug." → "Não é preciso desconectar nada.") ·
  confirmed. **idle → `ocioso`** · terminologia da Microsoft (`idle` → ocioso, `idle timeout` → "tempo limite ocioso"),
  Thunar pt-BR ("dispositivos ociosos") e a linha `quando o Mac está ocioso` deste glossário · high.
- **`timedOut` NÃO é uma falha** · `O disco ainda não respondeu, então a ejeção ainda pode ser concluída sozinha.` Mesma
  regra do irmão `errors.mutation.timedOut` ("O volume ainda não respondeu, então a alteração ainda pode ser
  concluída"), com o mesmo **ainda pode ser concluída**. O substantivo **a ejeção** é do Finder pt-BR ("mantenha a tecla
  Option pressionada durante a ejeção") · high; **sozinha** traduz o "on its own", que é a parte tranquilizadora: dá
  para não fazer nada. ❌ Nunca "não foi possível" nem "não deu certo" aqui.
- **`unexpected` copia o irmão letra por letra** · `Algo deu errado, e o Cmdr não conseguiu identificar o quê.`,
  idêntico a `errors.mutation.unexpected` (o inglês das duas chaves também é idêntico) · confirmed. O mesmo
  `não conseguiu identificar` serve `mtpIdMissingDevicePrefix` ("O Cmdr não conseguiu identificar qual é este
  dispositivo, então não consegue desconectá-lo.").
- **`busy`: o gerúndio brasileiro** · `O Cmdr ainda está movendo arquivos neste disco. Ejete-o assim que isso terminar.`
  **está movendo** (nunca "está a mover"), **assim que** para o "once" (Total Commander pt-BR "assim que eles forem
  salvos"), e a ênclise **Ejete-o**.
- Varredura pt-PT (ficheiro, `estar a` + infinitivo, consoante, próclise antes de infinitivo, Rever, alterar o nome,
  você omitido), mais U+2019, apóstrofo duplo e espaço duplo: zero ocorrências nos nove valores. Nenhum
  `sameAsSourceJustification` necessário: os nove diferem do inglês.

### "drive de rede" reconciliado para "disco de rede" (`errors.listing.*`, 2026-08-23)

O bolsão sinalizado nas seções do interruptor mestre de indexação e das 31 recusas: quatro valores de `errors.json`
ainda diziam **drive de rede**, contra o **disco de rede** fixado na seção do índice de imagens em rede.

- **network drive → `disco de rede`** · confirmado de novo contra o pile: o macOS pt-BR Finder usa **disco** em toda a
  família ("Discos rígidos", "Discos externos", "Ejetar discos e desmontar servidores"), e "de rede" é o qualificador
  corrente (Total Commander pt-BR `5164` "Exibir os nomes e caminhos de &discos de rede"). A terminologia da Microsoft
  diz `network drive` → "unidade de rede", mas o princípio 2 de escolha de termos põe o Finder na frente · high. ❌
  Nunca "drive de rede" nem "unidade de rede".
- Chaves alinhadas: `errors.listing.staleConnection.suggestion`, `errors.listing.pathNotFoundErrno.suggestion`,
  `errors.listing.notFound.suggestion`, `errors.listing.deviceDisconnected.explanation`. O inglês não mudou, então o
  `@key.sourceHash` das quatro fica intacto.
- **O bolsão foi fechado em 2026-08-24**; as chaves estão listadas na seção seguinte.

## O bolsão de `drive` fechado, e o último progressivo pt-PT (2026-08-24)

Nenhum termo novo: só a aplicação do `drive → disco` já assentado (§ Master drive-indexing switch terms) às chaves que
tinham ficado de fora dos lotes anteriores. `disco` é masculino como o `drive` que substitui, então nenhuma concordância
muda.

- `errors.listing.staleConnection.explanation` · "drives de rede (NFS, SMB)" → **discos de rede (NFS, SMB)**
- `errors.listing.quotaExceeded.explanation` · "e drives de rede" → **e discos de rede**
- `errors.listing.notSupported.explanation` · "certos drives de rede" → **certos discos de rede**
- `errors.listing.notSupportedErrno.suggestion` · "um drive externo" → **um disco externo**
- `errors.listing.deviceProblem.suggestion` · "Se for um drive externo" → **Se for um disco externo**
- `errors.listing.crossDeviceOperation.explanation` · "um drive interno" → **um disco interno**
- `errors.listing.attributeNotFound.suggestion` · "o drive interno do seu Mac" → **o disco interno do seu Mac**
- `errors.provider.pCloudFuse.transient` / `.needsAction` / `.serious` · "no drive virtual do **pCloud**" → **no disco
  virtual do pCloud**, e em `.serious` "Se o drive não reaparecer" → **Se o disco não reaparecer**. O inglês diz
  "pCloud's virtual drive", substantivo comum depois da marca, não o nome do produto; a marca `pCloud` e o caminho
  `/Volumes/pCloudDrive` ficam intactos.
- `fileExplorer.unreachable.detailTimeout` · "um drive de rede" → **um disco de rede** (fora do `errors.json`, mesmo
  bolsão)
- `fileOperations.transferDialog.compressLevelCaption` · "demoram mais tempo **a** comprimir" (progressivo pt-PT) →
  **"demoram mais para comprimir"**, idêntico ao fecho do irmão já corrigido
  `settings.archives.compressionLevel.description`. Mesma causa-raiz registrada em § pt-PT leak found and fixed
  (2026-07-25): o pile `_ignored/i18n/pt/` é EUROPEU; o brasileiro é `_ignored/i18n/pt-BR/`.
- Varredura pt-PT completa depois da correção (ficheiro, `estar a` + infinitivo, `tempo a` + infinitivo): zero
  ocorrências no catálogo `pt`.
- Nenhum `@key.sourceHash` muda: só os valores foram editados.

## Os dois botões do aviso do Lixo e a família "colocar de volta" (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, 2026-08-27)

Nove chaves novas: os dois botões do aviso que aparece logo depois de mover arquivos para o Lixo, os textos de progresso
e de resultado do desfazer, e o comando "Go to trash" na paleta de comandos.

- **undo (o botão) → `Desfazer`** · macOS Finder `ME13` Tier 1 (`Undo` = `Desfazer`), e o catálogo já entrega essa mesma
  palavra para o mesmo botão em inglês (`askCmdr.renameUndo.undo`) · high. Uma palavra, cabe no aviso estreito.
- **put back (a ação que o botão dispara) → `colocar de volta`** · macOS Finder `N153.1` (`Put Back` =
  `Colocar de Volta`) e `PE130_V1`/`PE130_V2` ("could not be put back" = "Não foi possível colocar … de volta") · high.
  É o termo do Finder para exatamente esta operação, então ele vale aqui em vez de `restaurar`, que o
  `askCmdr.renameUndo.*` usa para devolver o NOME anterior, outra operação.
- **"Go to trash" → `Ir para o Lixo`** · macOS Finder `TL_HELP_TCAN` Tier 1 ("Go to the Trash" = "Ir para o Lixo") ·
  high. O mesmo valor no botão e no rótulo da paleta, como no inglês. `Lixo` continua maiúsculo (o nome do recurso, já
  assentado).
- **"Putting them back..." → `Colocando de volta...`** · gerúndio pt-BR (nunca `A colocar`, que é pt-PT), na forma dos
  irmãos deste arquivo (`transferProgress.scanTitleCopy` = "Verificando antes de copiar..."). O arquivo `pt` mantém as
  três reticências do original, não `…`.
- **Concordância dentro dos ramos.** Em `undone` e na primeira metade de `undonePartial` o particípio concorda com o
  substantivo contado (`arquivo colocado de volta` / `arquivos colocados de volta`), então `{countText}` entra DENTRO de
  cada ramo, exatamente como `transfer.trash` já faz. Os três ramos CLDR (`one`, `many`, `other`) são obrigatórios.
- **A segunda metade concorda normalmente.** Em `undonePartial`, `{skippedText}` tem o inteiro parceiro `{skipped}`,
  então `ficou`/`ficaram` entram nos ramos:
  `{skipped, plural, one {{skippedText} item ficou} many {{skippedText} itens ficaram} other {{skippedText} itens ficaram}} no Lixo`.
  O contado é `item`/`itens`, o termo assentado para o `item` que a fonte diz nesta metade, não `arquivo` como na
  primeira. Se algum dia aparecer um `*Text` avulso SEM parceiro plural, a saída é um verbo invariável em número, na
  primeira pessoa (`deixamos {skippedText} no Lixo`), que ainda por cima segue o `style.md` § Formality.
- **"Nothing to put back. …" →
  `Nada a colocar de volta. Estes itens talvez já estejam de volta, ou o disco deles não está conectado.`** · segue a
  estrutura da irmã `askCmdr.renameUndo.unavailable` · high. `item`/`itens` é o termo assentado, e **`disco`** é o termo
  de drive (§ O bolsão de `drive` fechado).
  - ⚠️ REVIEW FLAG: a irmã `askCmdr.renameUndo.unavailable` ainda diz **`a unidade dele`**, que é o termo da Microsoft e
    contraria o `disco` assentado. A chave está fora deste lote; corrigir numa varredura.
- **"This drive doesn't keep a trash." → `Este disco não tem Lixo.`** · constatação de fato, então não entra no registro
  de `errors.write.trashNotSupported.message` ("não oferece suporte ao Lixo"), que é uma tela de erro · high.
- **A descrição do comando → `Abra o Lixo do disco em que você está navegando`** · imperativo, como as outras descrições
  de `commands.json` ("Faça uma cópia dos arquivos selecionados na mesma pasta"), com `você` explícito porque o verbo
  sozinho seria ambíguo · high.

### Notas anexadas a um relatório já enviado (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`, 2026-08-28)

O Cmdr envia um relatório sozinho quando a pessoa optou por isso, e agora o aviso rápido abre um diálogo que mostra o
que já foi enviado e aceita uma nota que entra NO MESMO relatório. Nada sobe uma segunda vez, e o texto precisa deixar
isso claro.

- **note (a caixa de texto livre) → `nota`** · o termo já publicado no diálogo de envio
  (`errorReporter.dialog.noteLabel` "Adicionar uma nota (opcional)", `notePlaceholder`, `noteTooLong` "A nota é longa
  demais") · high. A terminologia da Microsoft pt-BR dá `observação` (id 233427) e `nota pessoal` (id 2769303) para
  "note", mas os dois diálogos dividem a mesma caixa: trocar o termo abriria uma costura entre telas irmãs. No diálogo
  de acréscimo o rótulo é **`Sua nota`**, sem `(opcional)`, porque aqui a nota (ou o email) é o que libera o botão.
- **"Add to report" → `Adicionar ao relatório`; "Adding…" → `Adicionando…`** · KDE Dolphin pt-BR ("Add to Places" =
  "Adicionar aos locais"), Total Commander pt-BR (`1741="Adicionar ao &submenu"`), Double Commander pt-BR ("Add to
  queue" = "Adicionar à fila") · high. O par botão/estado espelha `dialog.send`/`dialog.sending` ("Enviar relatório" /
  "Enviando…"). Gerúndio brasileiro (`Adicionando…`, nunca `A adicionar…`).
- **"attach your email" → `anexe seu email`** · terminologia da Microsoft pt-BR (attach = `anexar`, ids 16026/1083539) e
  o rótulo já publicado `settings.updates.attachEmailToReports.label` ("Anexar meu email aos relatórios por padrão") ·
  high. `email` sem hífen, como as chaves desse grupo em `settings.json`.
- **"What was sent" → `O que foi enviado`** · o passado da irmã `dialog.detailsToggle` ("O que está prestes a ser
  enviado") · high. As duas abrem o mesmo painel; só o tempo verbal muda, e o paralelismo é o que faz a pessoa
  reconhecer a tela.
- **"and it''ll join what the team already has" → `e isso entra no mesmo relatório que a equipe já tem`** · high.
  `mesmo relatório` diz explicitamente o que o inglês só sugere (nada é enviado duas vezes), que é o ponto da tela. ❌
  Não `se junta ao que a equipe já tem`: literal e vago em pt-BR. `a equipe` é o termo já publicado em
  `dialog.description`.
- **"from the Help menu" → `pelo menu Ajuda`** · o menu nativo é `Ajuda` (§ Menus nativos, macOS Finder e Safari
  `pt-BR`), e a frase inteira já está publicada em `settings.updates.errorReports.description` ("Você sempre pode enviar
  um relatório manual pelo menu Ajuda.") · high. O item correspondente é `Enviar relatório de problema…`
  (`menu.help.sendErrorReport`).
- **`amend.unavailable` não fala em falha nem em erro**: `Esse relatório não aceita mais notas.` é uma constatação, não
  um aviso de problema. O plural `notas` lê melhor que o singular do inglês, e `Para levar suas notas até a equipe`
  mantém a voz ativa com a pessoa como agente.
- **"View or add notes to the report" → `Ver ou adicionar notas ao relatório`** · `Ver X` é o padrão do catálogo para
  botões e links de "View X" ("Ver detalhes da licença", "Ver registro completo de alterações"), enquanto `Visualizar`
  fica reservado ao menu Visualizar · high. As duas metades (olhar e acrescentar) ficam de pé, e o botão cabe ao lado de
  `Alterar ajustes` no aviso rápido.
- **Close → `Fechar`** · KDE Dolphin e Double Commander pt-BR · confirmed.
- Varredura pt-PT do lote (ficheiro, `estar a` + infinitivo, consoante, próclise antes de infinitivo, Rever, alterar o
  nome, `você` omitido): zero ocorrências. Nenhum valor é idêntico ao inglês, então nenhum precisa de
  `sameAsSourceJustification`; nenhum leva apóstrofo, então não há `''` no lote.
- "See why" (o botão do aviso rápido do Ask Cmdr) · **Ver por quê** · a forma separada e acentuada, porque a pergunta
  fecha a frase; é a mesma escolha já feita em `queue.chip.failed` ("para ver por quê") · confirmed. `porquê` avulso não
  é forma correta em pt-BR: o substantivo pediria artigo (`Ver o porquê`). `askCmdr.wakeToast.openThread`.

## A caixa de diálogo de selecionar / desmarcar arquivos (`selection.*`, 2026-08-29)

Fontes do lote: macOS 26 Finder `pt-BR` (`MenuBar.json`, ids `172.title` / `300488.title`), Double Commander `pt-BR`
(`doublecmd.po`, `&Unselect All`) e Total Commander `pt-BR` (`WCMD.LNG.utf8` 7603/7604/7613/7614). A área é ICU, então
apóstrofos seriam duplos; nenhum valor do lote tem apóstrofo.

- **select → `Selecionar`; deselect → `Desmarcar`** · macOS Finder `pt-BR` diz `Selecionar Tudo` (`172.title`) e
  **`Desmarcar Tudo`** (`300488.title`) · confirmed (Tier 1, já registrado neste glossário). ❌ Não `Desselecionar`:
  essa é a forma do Finder `pt-PT`, e o `pt` do Cmdr é brasileiro. As fontes Tier 3 divergem e não pesam aqui: Double
  Commander `pt-BR` usa a perífrase `Remover seleção`, e o Total Commander `pt-BR` está meio traduzido nessa tela
  (`&Remove selection by name/extensão:` ainda em inglês), então nenhum dos dois derruba o Finder.
- **Os três lugares que nomeiam a caixa de diálogo dizem a mesma coisa**: `menu.select.files` /
  `menu.select.deselectFiles` (`Selecionar arquivos…` / `Desmarcar arquivos…`), `commands.selectionSelectFiles.label` /
  `commands.selectionDeselectFiles.label`, `settings.selection.recentSelections.maxCount.description`
  (`a caixa de diálogo Selecionar / Desmarcar arquivos`) e agora os títulos `selection.dialog.title.add` / `.remove`. O
  bug que este lote conserta era o título discordar do menu que o abre · high.
- **`Select these files` → `Selecionar estes arquivos`; `Deselect these files` → `Desmarcar estes arquivos`** · mesmo
  par de verbos dos títulos, no infinitivo-imperativo de botão (`style.md` § Formality) · high.
- **`… in the focused pane` → `… no painel em foco`** · forma já publicada no catálogo
  (`commands.navGoToPath.description` "Leve o painel em foco para…", `commands.favoritesAdd.description` "a pasta atual
  do painel em foco") · high. **As dicas começam literalmente com o texto do botão** e só acrescentam o complemento
  (`Selecionar estes arquivos no painel em foco`): botão e dica se leem como uma frase só.
- **`Press Enter to filter` → `Pressione Enter para filtrar`** · decalque do irmão `search.runHint`
  (`Pressione Enter para buscar`) · high. **A tecla se chama `Enter` em pt-BR**, sem tradução (é o que está gravado no
  teclado); o verbo é `Pressione`, como no irmão.
- **`recent selections` → `seleções recentes`** · já publicado em `settings.selection.recentSelections.maxCount.label`
  (`Seleções recentes a lembrar`) · high. Os cinco textos do pop-over copiam a gramática e o registro dos gêmeos de
  busca `queryUi.recent.*`, trocando `buscas` por `seleções`: `Mostrar todas as seleções recentes`,
  `Todas as seleções recentes`, `Filtrar seleções recentes`, `Nenhuma seleção recente corresponde a esse filtro.`,
  `Seleções recentes`.
- **`selection.recent.popoverAria` e `.listboxAria` têm o mesmo inglês (`Recent selections`)**, então precisam de um
  valor idêntico em `pt` ou o `i18n-terms` acusa. As duas: `Seleções recentes`.
- **`Apply recent {mode} selection: {query}` → `Aplicar seleção {mode} recente: {query}`** · decalque do molde já
  publicado em `search.recent.runAria` (`Executar busca {mode} recente: {query}`) · high. `{mode}` chega traduzido
  (`IA`, `Regex`, `Nome de arquivo`) e `{query}` é texto livre da pessoa: o molde deixa os dois em posição neutra, sem
  concordância a resolver.
- **`Matching what is shown in the list (the full path).` →
  `Corresponde ao que aparece na lista (o caminho completo).`** · `corresponder` é o verbo do catálogo para "match"
  (`commands.selectionSelectFiles.description` "os arquivos correspondentes") e `caminho completo` já está fixado
  (`fileOperations.validation.pathTooLong`) · high. Sujeito oculto (o padrão), que mantém o aviso curto e tranquilo em
  vez de soar como alerta.
- Varredura pt-PT do lote (`ficheiro`, `estar a` + infinitivo, `consoante`, próclise antes de infinitivo, `Rever`,
  `alterar o nome`): zero ocorrências.

## Uma coisa, um nome: a rodada de deriva interna (2026-08-30)

O `desktop-i18n-term-consistency` encontrou 26 divergências em `pt` (um valor em inglês, duas ou mais formas em
português). Dezoito eram deriva de verdade; oito são fronteiras legítimas, quase todas de concordância — e é justamente
por isso que uma varredura automática nunca poderá decidir sozinha em português.

### As derivas corrigidas

- **case-sensitive → `diferenciar maiúsculas de minúsculas`** · terminologia da Microsoft pt-BR (id 28521 → 28529, BRA)
  · high. Havia QUATRO formas em duas telas de busca. A forma do rótulo agora entra inteira no nome acessível
  (`… na correspondência`), o que resolve a violação de WCAG 2.5.3 que o `desktop-i18n-aria` apontava. ❗ Ao mexer em
  `queryUi.scope.toggle.caseSensitive`, mexa junto no `…Aria`: o nome acessível TEM de conter o rótulo visível, senão
  quem usa controle por voz não consegue acionar a caixa.
- **trash → `Lixo`, nunca `Lixeira`** (a regra já estava no topo deste arquivo; duas chaves ainda diziam `Lixeira`,
  incluindo o aviso de que o Cmdr não conseguiu confirmar a ida do arquivo para o Lixo).
- **zoom in / out → `Ampliar` / `Reduzir`** na paleta de comandos também, não só no menu.
- **Dismiss → `Dispensar`** nas duas chaves que ainda diziam `Descartar`.
- **Send feedback → `Enviar feedback`** · toda a família `feedback.*` já dizia `feedback`; só a paleta e o menu Ajuda
  diziam `Enviar comentário` · high.
- **Reset → `Restaurar`** · macOS Finder pt-BR (`Restaurar aos Padrões`) · high. `Restaurar tudo para os padrões` /
  `Restaurar para o padrão`; `Redefinir` saiu (menos em `redefinir o zoom`, que é outra coisa).
- **error report → `relatório de problema`** · o `style-guide.md` do app pede que mensagens ao usuário evitem "erro", e
  `errorReporter.dialog.title` já seguia isso · high.
- **`dir`/`dirs` nunca deveria ter ficado em inglês**: as três chaves de contagem da varredura agora dizem
  `pasta`/`pastas`, como a barra de status. ❗ Elas passaram pelo `i18n-coverage` porque o ramo plural `many` deixa o
  valor estruturalmente diferente do inglês — a checagem não flagra isso, então confira contadores plurais à mão.
- **delete → `Apagar`** também no botão de apagar modelo de IA; **Searching → `Buscando`** também no visualizador;
  **`Ir para o último download`**, **`Não mostrar novamente`**, **`Quebra automática de linha`** nas duas telas, e a
  grafia `e-mail` (três chaves diziam `email`).
- **`fileOperations.transferDialog.pathErrorNotZip`**: `nome do arquivo` virou `nome do arquivo compactado`. Em
  português, `arquivo` sozinho quer dizer "file", então a frase pedia o nome errado.

### As fronteiras que NÃO se devem achatar

- **Concordância de gênero e número não é deriva**, é gramática. Nunca unifique estes pares:
  - `Ambos` (arquivos e pastas, `queryUi.filters.type.both`) vs `Ambas` (notificações,
    `…downloadsNotifications.opt.both`).
  - `Revertida` (a operação, `operationLog.rollback.rolledBack`) vs `Revertido` (o item,
    `operationLog.outcome.rolledBack`).
  - `Modificado` (a data de um arquivo) vs `Modificados` (o filtro de atalhos que o usuário mudou,
    `shortcuts.section.filterModified`).
- **Substantivo vs verbo**: `Pré-visualização` (o rótulo do painel) vs `Pré-visualizar` (o comando na paleta); `Busca`
  (o nome da seção em Ajustes, entre irmãos como `Aparência` e `Indexação`) vs `Buscar` (o botão que dispara a busca e o
  título do diálogo).
- **`Tentar novamente` (botão) vs `tente de novo` (texto corrido)**: o botão segue o Finder pt-BR (`NE106`, `PE110` =
  `Tentar Novamente`); a prosa do catálogo usa `de novo` em cerca de cem valores, e é o idioma natural ali. ❌ Não faça
  uma varredura trocando `de novo` por `novamente`.
- **`Problema` (o que o usuário lê) vs `Erro` (prefixo de diagnóstico em `settings.updates.errorPrefix`, onde o próprio
  `@key` inglês libera a palavra)**.
- **`Em execução` (um servidor rodando) vs `Em andamento` (uma tarefa em progresso)** para `Running`; a própria checagem
  cita esse par como divergência legítima.
- **`restaurado` (devolver o NOME anterior, `askCmdr.renameUndo.*`) vs `colocado de volta` (devolver o arquivo do Lixo,
  `fileOperations.trash.undone`)**. O inglês diz "Put back" nos dois casos; a imprecisão é dele.
- **`memória` (RAM, `ai.local.*`) vs `anotações` (a memória do Ask Cmdr, `settings.askCmdr.memory.*`)**.
- **`viewer.saveAs.defaultName` fica `selecao`, sem cedilha**, de propósito: é um nome de arquivo padrão, e o `@key`
  pede algo seguro para usar como nome de arquivo.

O guarda contra o português europeu continua valendo: esta rodada não empurrou nenhuma forma brasileira na direção de
pt-PT. Varredura pt-PT dos valores alterados (`ficheiro`, `estar a` + infinitivo, `ecrã`, `Rever`): zero ocorrências.

## O que o inglês corrigiu em si mesmo, e o que isso decidiu em `pt` (2026-08-30)

O catálogo `en` tirou cinco incoerências de si mesmo. Aqui fica o que isso assentou em português.

### O e-mail de exemplo: `voce@example.com`

- **Parte local em português, domínio `example.com`** · terminologia MS pt-BR (`nome@example.com`, `user@example.com`),
  RFC 2606 · high. Os três campos carregam o mesmo valor: `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`.
- `voce@` (sem cedilha, como todo endereço) é a contrapartida direta do `you@` inglês e do registro `você` do
  `style.md`. A terminologia da Microsoft também traduz a parte local (`nome@`), então traduzir é a prática corrente.
- ❌ NÃO `exemplo.com`: esse é um domínio de verdade, registrável, e pode ser o endereço real de alguém. `example.com` é
  o domínio que a RFC 2606 reserva para exemplos.

### A frase de devolver o NOME agora nomeia o objeto

- `askCmdr.renameUndo.undone` / `.partial` →
  **`{count, plural, one {O nome anterior de {countText} arquivo foi restaurado} many {Os nomes anteriores de {countText} arquivos foram restaurados} other {Os nomes anteriores de {countText} arquivos foram restaurados}}.`**
  · o vocabulário que a própria família já usa (`.undoing` = "Restaurando os nomes anteriores…", `.skipReason.failed.*`
  = "devolver o nome anterior") · high.
- O inglês dava a mesma frase ("Put back {countText} {files}.") para devolver o NOME e para tirar um arquivo do Lixo;
  `pt` já separava os dois (`restaurado` vs `colocado de volta`), e agora o inglês também nomeia o objeto. A separação
  registrada acima continua valendo.
- Como o particípio e o artigo concordam, a frase inteira entra nos ramos de plural e `{countText}` fica DENTRO deles,
  igual a `fileOperations.trash.undone`. `many` continua obrigatório em `pt`.
- `fileOperations.trash.undone` não mudou.

### Os nomes dos painéis do macOS nas mensagens de erro

Oito chaves `errors.*` traziam os nomes dos painéis escritos à mão. Agora são tokens de runtime ou o português da
própria Apple.

- `{system_settings}`, `{privacy_and_security}`, `{files_and_folders}` ficam **literais**: o app troca cada um pelo nome
  do painel como ele aparece no Mac do USUÁRIO.
- ❌ **Nunca contraia uma preposição com um token** (`nos {system_settings}`): o valor em tempo de execução é
  desconhecido, então não dá para fazer o artigo concordar. Use `em {system_settings}`, que funciona com qualquer
  palavra.
- Os nomes de painel que os tokens não cobrem seguem a Apple pt-BR:
  - **Apple Account → `Conta Apple`** · macOS 26.6.2 (25G83),
    `AppleIDSettings.appex/Contents/Resources/InfoPlist.loctable` `pt_BR.CFBundleDisplayName`, 2026-08-30 · high. ❌ NÃO
    `Conta da Apple` (o que o catálogo dizia antes). Em texto corrido, o substantivo comum continua minúsculo:
    `a conta Apple certa`.
  - **General → `Geral`** · `pt-BR/macOS/SystemSettings/Localizable.json` `GENERAL` · high.
  - **Login Items & Extensions → `Itens de Início de Sessão e Extensões`** · macOS 26.6.2 (25G83),
    `LoginItems.appex/Contents/Resources/Localizable.loctable` `pt_BR["Login Items & Extensions"]`, 2026-08-30 · high.
    ❌ NÃO `Itens de Início e Extensões` (o que o catálogo dizia antes): esse é o nome curto e antigo do painel.

### Os dois itens da Apple na barra de menus

`menu.app.showAll` / `menu.app.hideOthers` (e os gêmeos `commands.appShowAll.label` / `commands.appHideOthers.label`) →
**`Mostrar tudo`** / **`Ocultar outros`** · macOS 26.6.2 (25G83),
`Finder.app/Contents/Resources/pt_BR.lproj/MenuBar.strings` `300730.title` / `300729.title`, 2026-08-30 · high. As
palavras da Apple (`Mostrar Tudo` / `Ocultar Outros`), com a capitalização do Cmdr: a barra de menus é toda em sentence
case, então só a primeira letra fica maiúscula. A família `menu.*` é nativa e não passa pelo ICU: um apóstrofo ali se
escreve uma vez só.

## Uma operação revertida pela metade: concluir a reversão (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`, 2026-08-30)

- **`Finish rolling back` → `Concluir a reversão`** · o verbo vem do macOS Finder pt-BR `NE108` ("Finish Copying" →
  "Concluir Cópia", Tier 1, `pt-BR/macOS/Finder/LocalizableMerged.json`, conferido em 2026-08-30), em sentence case como
  manda o `docs/style-guide.md`, já que o Finder usa Title Case e o Cmdr não. O substantivo `a reversão` é o que o
  próprio catálogo já publica em `operationLog.rollback.refusalUnexpected` ("não conseguiu iniciar a reversão"), o que
  fecha o par `iniciar a reversão` / `concluir a reversão` · high. Diz terminar o que ficou pela metade, nunca começar
  de novo.
- **O valor tem que continuar idêntico em `operationLog.dialog.finishRollBack` e
  `fileOperations.rollbackConfirm.finishRollBack`.** Os dois traduzem o mesmo inglês `Finish rolling back` (sourceHash
  `dbe3771`), então o `i18n-terms` acusa assim que um dos dois for ajustado sozinho. Mexa nos dois ou em nenhum.
- **`Finish rolling this back?` → `Concluir esta reversão?`** · decalque do irmão `fileOperations.rollbackConfirm.title`
  (`Reverter esta operação?`): mesmo registro e mesma pergunta no infinitivo, verbo + `esta` + substantivo · high. O
  `this` do inglês vira `esta reversão`, e não `esta operação`, porque o que se conclui é a reversão, não a operação.
- **O aviso embaixo da linha repete o molde de `fileOperations.rollbackConfirm.bodyUndoByDeleting`** · "O Cmdr reverteu
  o que conseguiu e deixou o resto como estava. Ao concluir a reversão, o Cmdr percorre a operação mais uma vez e volta
  a pular tudo aquilo de que não tem certeza." A oração `pula tudo aquilo de que não tem certeza` sai literalmente de
  `bodyUndoByDeleting`, `como estava` vem de `refusalAlreadyRolledBack` ("Esta já voltou ao que era.") e `volta a` cobre
  o "still" do inglês · high. A frase de propósito não promete uma reversão completa.
- **`Para concluir` foi recusado** · em português a locução lê primeiro como marcador de discurso ("em conclusão"), o
  que trocaria o sentido da segunda frase. `Ao concluir a reversão` deixa o Cmdr como sujeito implícito e não tem essa
  leitura dupla. `percorrer` também foi escolhido de propósito no lugar de `revisar`, que já é o verbo do catálogo para
  "review" (`Rever` pt-PT → `Revisar` pt-BR) e criaria colisão de termo.
- **`in {folder}` → `em {folder}`** · preposição pura, sem artigo e sem aspas: o nome que chega é próprio ("Apagando o
  que foi criado em Backup") · high. Diferente do `de` e do `es`, que puseram aspas; aqui os vizinhos do catálogo também
  deixam o placeholder solto, e o inglês igualmente. Sem artigo e sem concordância, qualquer nome cabe.
- Não faz falta `sameAsSourceJustification`: os cinco valores se diferenciam do inglês.
- Varredura pt-PT do lote (`ficheiro`, `estar a` + infinitivo, `consoante`, próclise antes de infinitivo, `Rever`,
  `alterar o nome`): zero ocorrências.

## O aviso do que a reversão conseguiu, e o que ela deixou (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`, 2026-08-31)

Depois que a pessoa aperta `Reverter` numa cópia ou movimentação em andamento, um aviso conta o que o desfazer
conseguiu. Ele tem até três partes: uma manchete, a linha `leftBehind`, e uma lista de motivos (`reason.*`), cada um em
duas versões: uma que NOMEIA o único item (`*.named`) e outra que os CONTA (`*.counted`). O tom é sempre "o Cmdr fez o
cuidadoso", nunca desculpa nem alarme.

### As linhas de motivo usam o molde `{name} ficou como está: <motivo>.`

- **Toda linha de motivo tem de satisfazer as TRÊS coisas ao mesmo tempo**: o molde `ficou como está`, a marca `Cmdr`
  onde o inglês a escreve, e zero concordância com `{name}`. Satisfazer duas e esquecer a terceira é o erro fácil aqui,
  e `drift.*` é onde as três se apertam mais.
- **O molde vem inteiro de `askCmdr.renameUndo.skipReason.*`**, que já publica essas mesmas linhas para o desfazer da
  renomeação · confirmed. O sujeito é o ITEM, e não o Cmdr, então a linha não precisa de sujeito explícito e não cai no
  indício pt-PT do `você` omitido (§ Variant do `style.md`): quem "ficou" é o arquivo. Começar a linha pelo `{name}` já
  é prática da casa (`fileExplorer.rename.chainKeptOriginalName`).
- **`folderNotEmpty.named` e `.counted` TÊM de ser byte a byte iguais às gêmeas do `askCmdr`**
  (`A pasta {name} ficou como está: agora tem algo dentro.` /
  `{countText} {count, plural, one {pasta} many {pastas} other {pastas}} ficaram como estão: agora têm algo dentro.`): o
  inglês das duas famílias é idêntico nessas duas chaves, e o `desktop-i18n-term-consistency` acusa qualquer diferença.
  ❗ Mexeu numa, mexa na outra. O ramo `one` delas diz `1 pasta ficaram como estão` (o verbo ficou fora do plural na
  chave original), o que nunca aparece porque `*.counted` só renderiza com contagem ≥ 2; copiar o valor como está vale
  mais do que consertar a concordância e quebrar o par.
- **As outras `*.counted` põem a oração inteira nos ramos**, do jeito que o `style.md` § Plurals manda: o verbo
  (`ficou`/`ficaram`, `mudou`/`mudaram`, `está`/`estão`) concorda com o contado, então tudo se duplica nos três ramos
  CLDR e `{countText}` vai DENTRO. O ramo `one` é código morto (o inglês garante ≥ 2), mas é obrigatório.
- **`spotTaken` troca uma palavra só: `ficou ONDE está`** · o inglês também troca de fórmula ali
  (`Left {name} where it is`, não `alone`), porque esse motivo é sobre o LUGAR (o item não voltou), e não sobre o estado
  (o item não foi tocado) · high. À primeira vista as cinco linhas continuam o mesmo molde.
- **`it changed after Cmdr put it there` → `mudou depois que o Cmdr colocou lá`** · high. Três exigências se cruzam
  nessa oração, e ela é a mais apertada da lista:
  - **A marca fica.** `Cmdr` é palavra que não se traduz nem se omite (`desktop-i18n-dont-translate` acusa), e aqui ela
    carrega o sentido: o que segura a mão do Cmdr é o item ter mudado depois que ELE gravou. Sem a marca, a linha só diz
    que o arquivo mudou em algum momento, e o motivo evapora.
  - **Nada concorda com `{name}`.** Por isso o objeto de `colocou` fica nulo (objeto nulo é corrente no pt-BR, e o
    tópico da frase já é o item): um clítico `o`/`a` ou um particípio (`foi colocado`) escolheria um gênero
    desconhecido. A mesma forma serve ao singular e ao plural, só o verbo do item muda (`mudou`/`mudaram`).
  - **O molde continua o mesmo** (`{name} ficou como está: …`), como nas irmãs. A irmã `unverifiable` mostra que o molde
    comporta a marca sem ficar pesado (`o Cmdr não conseguiu verificar se mudou`).
- **`Cmdr couldn''t check whether it changed` → `o Cmdr não conseguiu verificar se mudou`** · valor já publicado em
  `askCmdr.renameUndo.skipReason.unverifiable.named` · confirmed. As duas famílias dizem a mesma frase porque o inglês
  também diz (só o apóstrofo difere entre os arquivos, então o `desktop-i18n-term-consistency` não as pareia sozinho).
- **`something else now sits where it came from` → `já existe outra coisa no lugar de onde veio`** · `Já existe` é a
  abertura Tier 1 do Finder pt-BR para o lugar ocupado (`NE73` "An item with the same name already exists in this
  location." → "Já existe um item com o mesmo nome nesta localização.") e o catálogo já a publica ("Já existe um arquivo
  com este nome no destino.") · high. O Finder também tem `localização original` (`BU37_V1`/`BU37_V2`), mas o inglês
  aqui é de propósito coloquial ("where it came from"), então fica `o lugar de onde veio`.
- **`reason.failed.*` fica FORA do molde**, como no inglês (`Couldn''t undo {name}.` em vez de `Left … alone`): esse
  motivo não é uma escolha do Cmdr, é o disco recusando, e a linha convida a tentar de novo. `Couldn''t undo` →
  **`Não foi possível reverter`** (a voz de "couldn't" da seção Error-copy-phrasings + o `reverter` do glossário; o
  desfazer de UM item é `Revertido` em `operationLog.outcome.rolledBack`, então o verbo é `reverter`, não `desfazer`) ·
  high. `Its drive` vira **`O disco`** sem possessivo: `dele`/`dela` concordaria com o gênero de `{name}`, que é
  desconhecido.
- **`{name}` nunca leva concordância.** Pode ser arquivo ou pasta, então nenhum particípio, adjetivo ou possessivo pode
  se apoiar nele. Só verbos (que não flexionam em gênero) e preposições sem artigo. A única linha que sabe o gênero é
  `folderNotEmpty.named`, e só porque o próprio valor escreve `a pasta {name}`.

### As manchetes e a linha que abre a lista

- **O sujeito das manchetes é explícito (`O Cmdr` / `A reversão`).** Sem sujeito, `Apagou {countText} itens…` também se
  lê como `você apagou`, que é justamente o indício pt-PT listado no `style.md` § Variant. As linhas de motivo não
  precisam disso porque o sujeito delas é o próprio item.
- **`removed` e `deleted` viram os dois `apagar`** · `apagar` é o `delete` fixado no glossário e o verbo que a família
  da reversão já publica (`queue.row.reversalDeleting` "Apagando o que foi criado", `transferProgress.rollbackTooltip`)
  · high. A terminologia da Microsoft dá `remove` → `remover`, mas no catálogo `Remover` já é tirar uma entrada de uma
  lista (`Remover {hostName} da lista de servidores?`): usá-lo para arquivos abriria uma costura. O inglês varia
  (`deletes` no diálogo, `Removed` no aviso); o português não precisa.
- **`put back` na reversão de uma movimentação → `levar de volta`, não `colocar de volta`** · é o que a própria família
  já publica: `queue.row.reversalMovingBack` ("Levando os arquivos de volta") e `rollbackConfirm.bodyUndoByMovingBack`
  ("Isso leva os arquivos de volta para onde estavam") · high. A `@key` do inglês manda usar o mesmo verbo de
  `fileOperations.trash.undone`, mas isso pressupõe que a língua tenha um verbo só. Em `pt` já são três, e a fronteira
  agora é de três lados:
  - `colocar de volta` = tirar do Lixo (macOS Finder `N153.1`, `fileOperations.trash.*`),
  - `restaurar` = devolver o NOME anterior (`askCmdr.renameUndo.*`),
  - `levar de volta` = a reversão levar o arquivo ao lugar de origem (`queue.row.reversalMovingBack`,
    `cancelRollback.doneMovingBack`/`someMovedBack`/`stoppedMovingBack`). Como os ingleses das três famílias são
    diferentes, o `desktop-i18n-term-consistency` não acusa. ❌ Não achate.
- **`Stopped after …` → `A reversão parou depois de …`** · `parar` é o verbo do catálogo para interromper trabalho em
  curso (`queryUi` "Parar a busca", `transferProgress.rollbackTooltip` "Parar e apagar…") e `a reversão` é o substantivo
  já publicado (`refusalUnexpected`, `rollbackConfirm.finishRollBack`) · high. O sujeito é a reversão, e não a pessoa: o
  inglês omite o sujeito de propósito para não soar como cobrança.
- **`The rest` → `O resto`** · já publicado em `operationLog.rollback.partiallyRolledBackNotice` ("deixou o resto como
  estava") · high. O inglês fecha vago ("still there"); o português nomeia o lugar, porque sem antecedente `lá` fica
  solto: `O resto continua no destino.` (cópia) e `O resto ficou onde a movimentação deixou.` (movimentação), com
  `destino` e `movimentação` nos termos da casa.
- **`leftBehind` repete a promessa dos diálogos, palavra por palavra**: `O Cmdr pula tudo aquilo de que não tem certeza`
  sai de `rollbackConfirm.bodyUndoByDeleting`, e o fecho é `então estes ficaram onde estão:` (demonstrativo solto, como
  o inglês, porque a lista abaixo mistura arquivos e pastas) · high. O inglês usa o mesmo verbo (`skips`) nos dois
  lugares, e o português usa `pular` nos dois; a lista embaixo fica com `ficou como está`, porque um terceiro verbo só
  somaria ruído.
- **As manchetes "completas" tiram o número do braço `one`**: `O Cmdr apagou o item que tinha gravado.` e
  `O Cmdr levou o item de volta.`, sem `{countText}` · high. O inglês passou a frase inteira para dentro do plural pelo
  mesmo motivo (`o 1 item` não se diz), e o braço `one` do português já carrega o artigo, então o número ali só
  atrapalha. Os braços `many`/`other` continuam contando.
- **`rollbackConfirm.body` ganhou a terceira frase dos irmãos, palavra por palavra**:
  `O Cmdr pula tudo aquilo de que não tem certeza, então algo pode ficar para trás.`, idêntica a
  `rollbackConfirm.bodyUndoByDeleting`, porque o inglês das duas é idêntico nessa frase. As duas primeiras frases não
  mudaram.
- Varredura pt-PT do lote (`ficheiro`, `estar a` + infinitivo, `consoante`, próclise antes de infinitivo, `Rever`,
  `alterar o nome`, `você` omitido onde o verbo é ambíguo): zero ocorrências. Marcas brasileiras: `arquivos`, `gravou`,
  `tem certeza`.
- Nenhum valor leva apóstrofo, então não há `''` no lote. Nenhum `sameAsSourceJustification` é necessário: os 18 valores
  diferem do inglês.

## A tela de bloqueio quando o WebKit é antigo demais (`main.oldWebkit.*`, 2026-09-02)

Três strings que o Cmdr mostra no lugar da interface quando o Safari do Mac é antigo demais. Elas ficam no invólucro
HTML, não no app, então são a única coisa que essa pessoa vai ver do Cmdr.

- **`Software Update` → `Atualização de Software`** · nome do painel nos Ajustes do Sistema; o rastro Tier 1 do Finder
  confirma o termo (`Apple Device Software Update File` → `Arquivo de Atualização de Software do Dispositivo Apple`) ·
  `high`. Mantém as maiúsculas do nome do painel, ao contrário do uso corrido.
- **`Quit` → `Encerrar`** · já no glossário, confirmado por `Encerrar Finder` na barra de menus do Finder · `high`.
- **`Safari`, `Mac` e `15.4` ficam como estão.** `Safari` entrou para `BRAND_WORDS`.

## O aviso de macOS antigo (`main.oldMacos.*`, 2026-09-02)

Um diálogo que aparece uma única vez num Mac abaixo do macOS 12: o Cmdr abre, mas está fora da faixa testada. Tom
honesto e tranquilo, sem pedido de desculpas e sem alarme, porque o app funciona.

- **`supported` → `compatível`** · macOS Finder pt-BR (`… porque o item não é compatível.`) · `high`. Não `suportado`,
  que é decalque.
- **`X and up` → `X e versões mais recentes`** · macOS SystemSettings pt-BR (`… pelo menos a versão %@ do OS X`) ·
  `high`.
- **`best effort` → `faz o que dá`** · o pile não traz o termo (só definições de QoS de rede) · `high` para a paráfrase.
  Deliberadamente não `melhor esforço`, que soa a contrato.
- **`layout` fica `layout`** · empréstimo corrente no pt-BR de tecnologia; `disposição` soaria acadêmico aqui.
- **A última frase é o David em primeira pessoa**, com `você`, como em `onboarding.stepBeta.greeting`.
