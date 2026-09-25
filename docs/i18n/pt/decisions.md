# pt decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each citing the keys it shaped. The chosen forms live in
`terms.json`; this file keeps only the non-obvious why that defends a ruling against a well-meant "fix". Voice and
grammar rules: `style.md`. Open questions: `review-queue.md`. Sources: the Brazilian pile (`_ignored/i18n/pt-BR/`) or
the installed macOS `pt_BR.lproj` bundles.

## The bare `pt` pile is European (`settings.archives.compressionLevel.description`, `fileOperations.transferDialog.compressLevelCaption`)

`settings.archives.compressionLevel.description` once shipped European Portuguese ("ficheiro"/"ficheiros", plus the
pt-PT "demoram mais tempo **a** comprimir"), and `compressLevelCaption` carried the same progressive. The Compress rows
above had cited "pt Double Commander / Thunar / Nautilus" and "pt pile has no Total Commander": that is the **bare
`_ignored/i18n/pt/` folder, which is EUROPEAN**. The Brazilian set is `_ignored/i18n/pt-BR/`, and it _does_ have
`total-commander/`. Both keys now read "demoram mais para comprimir". Re-check any row whose sources name the bare `pt`
pile.

## Índice do disco: a verificação de mudanças (`indexing.run.*`, `indexing.step.*`)

- **"Checking for changes" (run-kind header) → `Verificação de mudanças`** · nominal phrase matching the sibling headers
  (`Primeira varredura completa`, `Atualização rápida`); `Verificando` is macOS pt-BR's checking verb (Finder BN9
  "Verificando os conteúdos…"), `mudanças` is catalog-settled (`as mudanças recentes`) · high.
- **"Update the file list" → `Atualizar a lista de arquivos`** · composed from the settled siblings
  `Salvar a lista de arquivos` + `Atualizar o índice` · high.
- **"the check running right now" → `a varredura que está em andamento agora`** · reuses `varredura` as this catalog's
  settled word for a full check (`tooltipCoalesced`: "a próxima varredura completa do Cmdr") and that string's closing
  `vai corrigir isso` · high.

## Stalled-transfer notice terms (`fileOperations.transferProgress.close`/`stall*`)

The copy/move dialog stops showing an ETA it no longer believes and explains the stall instead. Whole batch avoids
"erro"/"falhou" (and any bare "Erro"), per the no-bare-error voice rule.

- Close (button that closes the progress dialog while the transfer keeps finishing in the background; sits next to
  Cancelar) · **Fechar** · macOS Finder pt-BR (`LocalizableMerged.json` key `FR26` "Close" → "Fechar", key-based
  EN→pt-BR), Microsoft terminology pt-BR (close → "Fechar", 4 entries) · confirmed. Clearly distinct from the sibling
  **Cancelar**, so the two buttons never read as the same action.
- "No progress for {duration}" (the line that replaces the ETA on a stalled transfer) · **Sem progresso há {duration}**
  · `progresso` is macOS Finder pt-BR ("Show Copy Progress" → "Mostrar Progresso da Cópia", "Show Progress Window" →
  "Mostrar Janela de Progresso") · high. **`há`, not `por`/`durante`**: pt-BR expresses an elapsed stretch running up to
  now with `há` ("Sem progresso há 45s"), and `{duration}` is always an already-formatted elapsed span. One key,
  `transferProgress.stallNotice`, feeds both the progress dialog and the queue row, and its value carries no final
  period, matching English.
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

## Caminho copiado: a confirmação da área de transferência (`fileExplorer.clipboard.copiedPath`)

Uma chave: a linha do aviso informativo depois de ⌘⌥C. O caminho aparece abaixo, em linha própria e monoespaçada, então
NÃO é um marcador dentro da frase: a frase termina em dois-pontos e precisa funcionar sem ele.

- **"Copied the path, it's now on your clipboard:" → `Caminho copiado, agora está na área de transferência:`** ·
  reutiliza `path → caminho` e `clipboard → área de transferência` do termbase (macOS "Área de Transferência") · high. O
  particípio inicial segue os avisos irmãos (`{countText} itens copiados`). Sem possessivo ("sua área de
  transferência"): só existe uma, e o macOS usa o artigo.

## Operation-queue rename (`queue.*` + `commands.queueShow.*` + `fileOperations.transferProgress.queue*`/`backgroundedToast`)

The queue window is the "Operation queue", not a "Transfer queue": it lists deletes, trashes, renames, and folder/file
creations too, and "transfer" already means copy-or-move one level down (the progress dialog, the transfer driver).

- **operation → `operação`** (feminine, plural `operações`) · macOS Finder pt-BR is unanimous (40+
  `LocalizableMerged.json` values: `NE1` "A operação não pode ser completada.", `NE82` "…outra operação está em
  andamento…", `A17` "…algumas operações ainda estão em andamento."), MS terminology (ids 333922/87969/1381673, BRA),
  Double Commander ("Operações de arquivos"), Total Commander (`5391` "Registro de Operações com Arquivos"), Nautilus
  ("Todas as operações com arquivos foram concluídas") · confirmed.
- **operation queue → `Fila de operações`**, composed from `fila` (Total Commander `4005` "&Fila", Double Commander
  "Adicionar à fila", MS id 96569) and `operação`; MS's own "fila de impressão" is the model · confirmed. It replaced
  "Fila de transferências". `queue.windowTitle`, `commands.queueShow.label`, and the three
  `fileOperations.transferProgress.*` toasts carry the same string, so the window, the View menu item, and the palette
  entry read identically.
- ⚠️ **"Fila de operações" (present) and "Registro de operações" (past) sit in one View menu block** and share the head
  noun on purpose. Never rename one without the other.
- **transferência stays the narrow word** for the copy/move job itself (`fileOperations.transferProgress.pauseAria`
  "Pausar esta transferência", `stallUnknown` "A transferência parou de avançar", `transferDialog.smbNativeNote`), and
  never comes back as the queue's name. The progress dialog talks about one transferência; the queue window talks about
  operações.

- "Operations" (window heading + the list's screen-reader label) · **Operações** · the bare plural, matching the
  English's category-naming plural noun · confirmed. `queue.heading`, `queue.list.aria`.
- The four per-row aria labels keep their settled verbs and only swap the object noun: **Pausar / Retomar / Cancelar /
  Selecionar esta operação** · termbase pause→**Pausar**, resume→**Retomar**, Cancel→**Cancelar**, select→
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

## Progress-chip and failure-notice terms (`queue.row.dismiss*` + `queue.toolbar.dismissAll` + `queue.failureToast.*` + `queue.chip.*`)

Two new surfaces on top of the queue window: a corner progress chip (~80 px) previewing the background operation, and a
failure notice (a ~360 px toast) plus a dismissible failed row. The head noun and the window name are settled in the
section above (**operação** / **Fila de operações**); this section only adds what those two surfaces needed.

- dismiss (stop showing a notice or a finished-badly row; nothing is undone, retried, or deleted) · **Dispensar** · the
  pt catalog's own settled verb, five hits for the same concept before this batch (`ui.toast.dismissAria` "Dispensar
  notificação", `downloads.empty.dismiss`, `downloads.fda.dismiss`, `errorReporter.sentToast.dismiss`,
  `errorReporter.bundleSavedToast.dismiss`, `fileOperations.mkdir.timeoutDismiss`, and the viewer's
  `reloadToast.dismissTooltip` "Dispensar sem recarregar") · high. ❌ **Never MS terminology's `dismiss` → "ignorar"**
  (id 780443/1044462, BRA): **Ignorar is this catalog's Skip** (`transferProgress.conflictSkip`,
  `transferDialog.policySkip` "Ignorar todos"), so a Dismiss button labelled "Ignorar" would sit two rows from a Skip
  button meaning something else. KDE Dolphin pt-BR's "Descartar lembrete" is the runner-up, reconciled away everywhere
  (`crashReporter.dialog.dismiss`, `lowDiskSpace.toast.closeTooltip` say `Dispensar` too). `queue.row.dismiss`; the aria
  takes the sibling row shape, **Dispensar esta operação** (matching "Pausar / Retomar / Cancelar / Selecionar esta
  operação").
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
  para o Lixo, **renomeação** (termbase row; Nautilus pt-BR "Desfazer renomeação", TC `6601` "Renomeação em Lote"),
  criação da pasta, criação do arquivo, edição do arquivo compactado · high. **apagamento is the delete NOUN**,
  nominalizing the settled `Apagar`: it is what keeps the banned "exclusão" out of this family (`queue.empty.body` says
  "apagamentos" too).
- "Show in operation queue" (the toast's button) · **Mostrar na fila de operações** · termbase `Show in Finder` →
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

## Standalone conflict-prompt terms (`fileOperations.operationConflict.context`/`pausedNote`)

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
  status adjective **Pausado** (`queue.row.status` `paused`, termbase pause row); "até você responder" is the pt-BR
  personal infinitive and keeps the explicit **você** (dropping it is a pt-PT tell) · high. Reassuring, no error/failed
  words.

## Empty-queue button label (`fileOperations.transferProgress.background/backgroundAria`)

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

## Quit-gate dialog terms (`main.quit.*`)

The modal Cmdr raises when the user quits while a copy, move, delete, trash, or archive edit is still going: title,
reassurance body, a list of running operations, a live countdown, and two buttons. Terminology is anchored to the
already-shipped `queue.*` strings, since the dialog reuses `queue.row.label` verbatim for its rows.

- quit (the app stopping) · **Encerrar** (gerund **Encerrando**; "quit now" → **Encerrar agora**) · macOS Finder pt-BR
  ("Encerrar Finder", "Encerrar Sem Salvar"), MS terminology pt-BR (`quit` id 1133557 → "encerrar"), and already the
  catalog's word via `commands.appQuit.label` "Encerrar Cmdr" · confirmed. `main.quit.title/countdown/quitNow`.
- "operations are running" (the state the dialog gates on) · **operações em andamento** · macOS Finder pt-BR carries
  this exact sentence: "O Finder não pode ser encerrado porque algumas operações ainda estão em andamento." (plus
  "…outra operação está em andamento…") · confirmed. Matches the shipped `queue.row.status` `running` arm ("Em
  andamento") and the termbase's operation → **operação** row, so the dialog, the queue window, and the row statuses all
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

## Usage stats: "anônimas" dropped, "um identificador aleatório" named (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`)

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

## Confirmação de reversão e a linha que espera resposta (`fileOperations.rollbackConfirm.*`, `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip`, `transferProgress.foregroundBusyToast`/`rollbackTooltip`)

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
  macOS Tier 1 para `Replace`, e **apagar** é o `delete` fixado no termbase · high. A segunda frase usa a relativa livre
  **O que foi substituído** para ficar neutra em número (o inglês "any file" também não fala de um arquivo específico) e
  para não precisar do pronome de **a operação**. **Isso** (33 ocorrências no catálogo) e não "Isto" (3).
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

## Cadeia de renomeação: o aviso que cresce (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

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

## Renomeação sem confirmação e nome recusado (`fileExplorer.rename.unconfirmed*` + `fileOperations.validation.nameNotUsable`)

O par irmão de `chainKeptOriginalName*`: mesma forma de toast, situação oposta. `chainKept*` afirma que o arquivo
manteve o nome; `unconfirmed*` diz que o Cmdr NÃO sabe, e que a renomeação pode muito bem ter acontecido. Nunca
embaralhe os dois sentidos.

- "Couldn''t confirm …" · **Não foi possível confirmar …** · a voz de "couldn''t/failed" já fixada na seção
  Error-copy-phrasings, e o valor já publicado em `fileExplorer.pane.trashUnconfirmedToast` ("Não foi possível confirmar
  que o arquivo foi movido para a Lixeira.") · confirmed. As duas chaves de renomeação reusam essa abertura, então os
  três toasts de "não deu para confirmar" soam iguais.
- "the rename of X" · **a renomeação de X** · linha `renomeação` do termbase (substantivo de `Renomear`) · high. ❌
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

## Operações sugeridas: a janela do que o Ask Cmdr propõe (`suggestedOps.*`, `commands.suggestedOpsShow.*`)

- ops (as operações de arquivo propostas pelo agente) → `operações`; título `Operações sugeridas` · segue o termo da
  casa ("File operations" → "Operações de arquivo") · high
- approve → `Aprovar` · padrão; a pilha de referência não traz "approve" em pt · tentative
- reject → `Recusar` · padrão; a pilha só traz `Aceitar` (Nautilus/Dolphin) e nenhum par para "reject" · tentative
- "This can't be undone" → `Esta ação não pode ser desfeita` · macOS, palavra por palavra · high
- suggestion → `sugestão` · já no catálogo (`askCmdr`) · high

## Duplicar: o comando que copia na mesma pasta (`commands.fileDuplicate.*`)

- **duplicate (comando que copia a seleção dentro da própria pasta) → `Duplicar`** · macOS Finder pt-BR, menu "Arquivo >
  Duplicar" (`N154`), além de "Duplicar Itens" e "Duplica itens nas suas localizações atuais" (verificado no macOS
  26.6.1, `Finder.app/Contents/Resources/pt_BR.lproj`, 2026-08-19) · high. Convive com `Copiar` (F5) e `Mover` (F6).
- **"Make a copy of the selected files in the same folder" → `Faça uma cópia dos arquivos selecionados na mesma pasta`**
  · imperativo, como as descrições vizinhas ("Copie os arquivos selecionados…"); "mesma pasta" é a pasta onde os
  arquivos já estão · high.

## Menus nativos: barra de menus, menus de contexto, títulos de janela (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

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
- **changelog → `Registro de alterações`** (`menu.app.changelog`) · a família `log → registro` do catálogo inteiro
  (`Registro de operações`, `whatsNew.dialog.seeFullChangelog` "Ver registro completo de alterações") · high. O
  `Log de alterações` da Microsoft abria uma costura com essa família. Distinto de Ajuda > `Novidades`: um nomeia o
  documento, o outro a notícia.
- **Cut → `Cortar`** (`menu.edit.cut`) · macOS Finder pt-BR `MenuBar.json` `160.title` e `ME1` (`Cortar`) · high. Só a
  barra de menus copia o Finder aqui; o comando da paleta e a descrição dizem `Recortar` / `Recorte`, o verbo padrão da
  área de transferência (terminologia da Microsoft; o próprio Finder usa `Recortar` em `QK5`).
- **word wrap → `Quebra automática de linha`** · terminologia da Microsoft · high.
- **pin / unpin tab → `Fixar aba` / `Desafixar aba`** · Safari `pt-BR` („Fixar Aba”) · high.
- **Cores de etiqueta do Finder → `Vermelho, Laranja, Amarelo, Verde, Azul, Roxo, Cinza`** · macOS Finder (`TG_COLOR_*`)
  · high.
- **Linha de etiquetas (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Etiquetas`,
  `Adicionar “{color}”`, `Remover “{color}”`** · macOS Finder `pt-BR` (`TG5`, `TG6`, `N169.37`) · high. `Remover` bate
  com `commands.tagsToggleRed.description` (“Adiciona ou remove a etiqueta”): tirar uma etiqueta não apaga nada. O
  Finder `pt-PT` diz `Identificadores`, variante que o catálogo não segue.
- **busy (volume em uso) → `(ocupado)`** · terminologia da Microsoft · high.
- **Eject → `Ejetar`, Disconnect → `Desconectar`, Remove (de uma lista) → `Remover`** · macOS Finder · high. `Apagar`
  fica reservado para arquivos, como manda o `style.md`.
- **Idênticos ao inglês de propósito** (com `sameAsSourceJustification`): `menu.view.zoom`, `menu.window.zoom`,
  `menu.zoom.percent*`, `menu.view.askCmdr`.

## Aviso de conexão pelo sistema (`fileExplorer.network.osMountFallback.*`)

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
- "Dismiss" (fechar o aviso) · **Dispensar** · a linha `dismiss` do termbase (seção do chip de progresso), com seis
  ocorrências no catálogo · confirmed. ❌ Nunca **Descartar**.
- **A ordem da oração muda**: o inglês diz "4x slower for most connections (sometimes 100x) than …"; em português o
  adjunto vem antes do comparativo
  (`que, na maioria das conexões, é 4x mais lenta do que a conexão direta do Cmdr (às vezes 100x)`), porque separar
  "mais lenta" do seu "do que" trava a leitura.

## Recusas de renomear e criar: as 31 mensagens de uma linha (`errors.mutation.*` + `errors.volume.*`)

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
  simples **arquivo zip**; o **arquivo compactado** do termbase fica para quando o formato não é nomeado
  (`archiveNotEditable`, `needsPassword`, `archiveEditCouldntStart`).
- **"Renaming can't take an item out of / from one archive to another" →
  `A renomeação não pode tirar um item de um arquivo compactado` /
  `… não pode levar um item de um arquivo compactado para outro`** · o substantivo **renomeação** é a linha do termbase;
  **Use Mover para isso** nomeia o comando (macOS Finder `Mover`) em vez de mandar mover o item, que soaria como uma
  instrução ambígua dentro de um campo de nome · high.
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
- **"That password didn't work." → `Essa senha não funcionou.`** · do irmão já publicado
  `servers.refusal.authenticationRejected` ("Essa senha não funcionou para {username}.") · confirmed. Culpa a senha, não
  a pessoa. **password-protected → `protegido por senha`** (linha do termbase, diálogo de senha de zip).
- **"the change" (a renomeação/criação pedida) → `a alteração`** · usado em `timedOut` e em `deviceDisconnected` ("antes
  de a alteração ser concluída") · high. Distinto de **as mudanças** do sistema de arquivos (`fileExplorer.imageIndex`),
  que é o outro sentido de "changes" no catálogo.

## Recusas de mover para o Lixo: as duas mensagens de uma linha (`errors.mutation.trash*`)

Mesma superfície das 31 recusas acima (linha única sob o campo de nome ou num aviso rápido), família RAW, sem ICU.

- **"This volume has no Trash." → `Este volume não tem Lixo`** · **Lixo** é o valor Tier-1 do Finder pt-BR e já está
  fixado no termbase; o irmão publicado `errors.write.trashNotSupported.message` diz "Este volume não oferece suporte ao
  Lixo", mas o inglês novo trocou "doesn't support" pelo **has no**, mais simples, e o **não tem** acompanha esse
  registro · high. **"the only way is to delete permanently" → `então a única opção é apagar permanentemente`** ·
  **apagar permanentemente** é a linha do termbase (verbo do Finder), idêntico ao fecho já publicado em
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

## Diálogo de falha: as três aberturas (`crashReporter.dialog.body.ended`/`keptRunning`/`unknown`)

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
  do termbase, mais terminologia da Microsoft pt-BR (`background` adjetivo → "em segundo plano"; `background task` →
  "tarefa em segundo plano") e Total Commander pt-BR (`1237` "operações ativas em segundo plano") · confirmed.
- "Here''s a report with details that can help fix this" · **Aqui está um relatório com detalhes que ajudam a corrigir
  isso** · é a segunda frase já publicada em `crashReporter.dialog.body.ended`, menos o **de falha** · confirmed. O
  inglês também trocou "a crash report" por "a report" nas duas chaves novas: nada falhou, então o relatório perde o
  qualificador. `relatório` sozinho continua correto (terminologia da Microsoft: "Relatório de Erros do Windows").
- **A chave `unknown` não pode dizer nem uma coisa nem outra**: ela sai para relatórios escritos por versões antigas do
  Cmdr, que não registravam se o app seguiu rodando. Por isso ela fica só com "O Cmdr teve um problema da última vez." —
  sem `encerrou`, sem `continuou`, verdadeira nos dois casos.

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

## Recusas de ejetar e desconectar: as nove mensagens do aviso rápido (`errors.eject.*`)

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
  `errors.mutation`), então a voz fica ativa: `Algo ainda está usando este disco.` ❌ O `(ocupado)` do termbase é o
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
  termbase · high. `Esse disco` (não "este"): o disco já sumiu, então o demonstrativo se afasta.
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
  Thunar pt-BR ("dispositivos ociosos") e a linha `quando o Mac está ocioso` do termbase · high.
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

## `drive` é sempre `disco` (`errors.listing.*`, `errors.provider.pCloudFuse.*`, `fileExplorer.unreachable.detailTimeout`)

- **network drive → `disco de rede`** · o macOS pt-BR Finder usa **disco** em toda a família ("Discos rígidos", "Discos
  externos", "Ejetar discos e desmontar servidores"), e "de rede" é o qualificador corrente (Total Commander pt-BR
  `5164` "Exibir os nomes e caminhos de &discos de rede"). A terminologia da Microsoft diz `network drive` → "unidade de
  rede", mas o princípio 2 de escolha de termos põe o Finder na frente · high. ❌ Nunca "drive de rede" nem "unidade de
  rede".
- O `errors.json` inteiro segue: "discos de rede (NFS, SMB)", "um disco externo", "um disco interno", "o disco interno
  do seu Mac" (`errors.listing.staleConnection.*`, `.quotaExceeded.explanation`, `.notSupported*`,
  `.deviceProblem.suggestion`, `.crossDeviceOperation.explanation`, `.attributeNotFound.suggestion`), e
  `fileExplorer.unreachable.detailTimeout` também. `disco` é masculino como o `drive` que substituiu, então nenhuma
  concordância muda.
- `errors.provider.pCloudFuse.*` diz **no disco virtual do pCloud** ("Se o disco não reaparecer"): o inglês diz
  "pCloud's virtual drive", substantivo comum depois da marca, não o nome do produto; a marca `pCloud` e o caminho
  `/Volumes/pCloudDrive` ficam intactos.

## Os dois botões do aviso do Lixo e a família "colocar de volta" (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

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
  de drive (§ `drive` é sempre `disco`); a irmã `askCmdr.renameUndo.unavailable` diz `o disco dele` também.
- **"This drive doesn't keep a trash." → `Este disco não tem Lixo.`** · constatação de fato, então não entra no registro
  de `errors.write.trashNotSupported.message` ("não oferece suporte ao Lixo"), que é uma tela de erro · high.
- **A descrição do comando → `Abra o Lixo do disco em que você está navegando`** · imperativo, como as outras descrições
  de `commands.json` ("Faça uma cópia dos arquivos selecionados na mesma pasta"), com `você` explícito porque o verbo
  sozinho seria ambíguo · high.

## Notas anexadas a um relatório já enviado (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

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
- "See why" (o botão do aviso rápido do Ask Cmdr) · **Ver por quê** · a forma separada e acentuada, porque a pergunta
  fecha a frase; é a mesma escolha já feita em `queue.chip.failed` ("para ver por quê") · confirmed. `porquê` avulso não
  é forma correta em pt-BR: o substantivo pediria artigo (`Ver o porquê`). `askCmdr.wakeToast.openThread`.

## A caixa de diálogo de selecionar / desmarcar arquivos (`selection.*`)

Fontes do lote: macOS 26 Finder `pt-BR` (`MenuBar.json`, ids `172.title` / `300488.title`), Double Commander `pt-BR`
(`doublecmd.po`, `&Unselect All`) e Total Commander `pt-BR` (`WCMD.LNG.utf8` 7603/7604/7613/7614). A área é ICU, então
apóstrofos seriam duplos; nenhum valor do lote tem apóstrofo.

- **select → `Selecionar`; deselect → `Desmarcar`** · macOS Finder `pt-BR` diz `Selecionar Tudo` (`172.title`) e
  **`Desmarcar Tudo`** (`300488.title`) · confirmed (Tier 1, já registrado no termbase). ❌ Não `Desselecionar`: essa é
  a forma do Finder `pt-PT`, e o `pt` do Cmdr é brasileiro. As fontes Tier 3 divergem e não pesam aqui: Double Commander
  `pt-BR` usa a perífrase `Remover seleção`, e o Total Commander `pt-BR` está meio traduzido nessa tela
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

## Uma coisa, um nome (`queryUi.scope.toggle.caseSensitive`, `viewer.search.*`, `queryUi.results.scan*`, `transfer.delete`, `fileOperations.transferDialog.pathErrorNotZip`)

O `desktop-i18n-term-consistency` compara chaves com o MESMO inglês, e o `i18n:check-termbase` compara cada chave com a
regra do seu conceito. Os dois acharam deriva de verdade e fronteiras legítimas, quase todas de concordância: é
justamente por isso que uma varredura automática nunca decide sozinha em português.

### As formas que valem em todo o catálogo

- **case-sensitive → `diferenciar maiúsculas de minúsculas`** · terminologia da Microsoft pt-BR (id 28521 → 28529, BRA)
  · high. A forma do rótulo entra inteira no nome acessível (`… na correspondência`). ❗ Ao mexer em
  `queryUi.scope.toggle.caseSensitive`, mexa junto no `…Aria`: o nome acessível TEM de conter o rótulo visível (WCAG
  2.5.3), senão quem usa controle por voz não consegue acionar a caixa.
- **trash → `Lixo`, nunca `Lixeira`**, inclusive no aviso de que o Cmdr não conseguiu confirmar a ida para o Lixo.
- **zoom in / out → `Ampliar` / `Reduzir`** na paleta de comandos também, não só no menu.
- **Send feedback → `Enviar feedback`** · toda a família `feedback.*` diz `feedback`, nunca `Enviar comentário` · high.
- **Reset → `Restaurar`** · macOS Finder pt-BR (`Restaurar aos Padrões`) · high. `Restaurar tudo para os padrões` /
  `Restaurar para o padrão`; `redefinir` fica só para o zoom (`redefinir o zoom`).
- **error report → `relatório de problema`** · o `style-guide.md` do app pede que mensagens ao usuário evitem "erro" ·
  high. Também em "relatório de falha ou de problema" (`settings.updates.attachEmailToReports.description`).
- **`dir`/`dirs` nunca fica em inglês**: as chaves de contagem da varredura dizem `pasta`/`pastas`, como a barra de
  status. ❗ Elas passaram pelo `i18n-coverage` porque o ramo plural `many` deixa o valor estruturalmente diferente do
  inglês; confira contadores plurais à mão.
- **Search → a família `busca`/`buscar` em todo lugar**, o visualizador incluído (`Buscar texto`, `Fechar busca`,
  `⌘F buscar`, `Tente a busca (⌘F)`, `Buscando`). `pesquisável` continua como adjetivo de item indexado.
- **A varredura do disco é `varredura` em todo lugar**: `queryUi.results.scan*` (`Varredura em andamento`),
  `fileExplorer.dirSize.staleLine`, `indexing.rescan.incompletePreviousScan`. A família `indexing.rescan.*` diz o verbo
  (`Examinando o disco de novo …`), e `análise` fica para a pré-contagem de transferência.
- **`Ir para o último download`**, **`Não mostrar novamente`**, **`Quebra automática de linha`** nas duas telas, e a
  grafia `e-mail`.
- **`fileOperations.transferDialog.pathErrorNotZip`** diz `nome do arquivo compactado`: em português, `arquivo` sozinho
  quer dizer "file", então a frase pediria o nome errado.
- **Os nomes de pasta do macOS pt-BR**: `Downloads` (o Finder pt-BR não traduz; `Transferências` é o pt-PT),
  `Documentos`, `Mesa` (`onboarding.stepFda.pro.body`, `onboarding.stepAi.bannerBody.denied`).
- **Os apps da Apple com nome traduzido**: `Editor de Texto` (TextEdit) e `Pré-Visualização` (Preview), como o
  `InfoPlist.loctable` pt_BR de cada app diz (macOS 27.0 26A428, lido 2026-09-24), com artigo em texto corrido
  (`settings.advanced.showSafeSaveFiles.description`). O `@key` pede para manter o inglês; a regra de localizar o que a
  Apple localiza vence.
- **`chip` (o selo do repositório) → `selo`**, o mesmo `badge → selo` do catálogo; `etiqueta` é só a etiqueta do Finder
  (`settings.fileExplorer.git.showRepoChip.label`, `settings.summary.git`, `errors.git.notARepo.suggestion`).

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
- **`Problema` (o que o usuário lê)**: não sobrou nenhum prefixo de diagnóstico `Erro:` no catálogo; uma verificação de
  atualização que não deu certo vira frases inteiras (`updates.failure.check`).
- **`Em execução` (um servidor rodando) vs `Em andamento` (uma tarefa em progresso)** para `Running`; a própria checagem
  cita esse par como divergência legítima.
- **`restaurado` (devolver o NOME anterior, `askCmdr.renameUndo.*`) vs `colocado de volta` (devolver o arquivo do Lixo,
  `fileOperations.trash.undone`)**. O inglês diz "Put back" nos dois casos; a imprecisão é dele.
- **`memória` (RAM, `ai.local.*`) vs `anotações` (a memória do Ask Cmdr, `settings.askCmdr.memory.*`)**.
- **`viewer.saveAs.defaultName` fica `selecao`, sem cedilha**, de propósito: é um nome de arquivo padrão, e o `@key`
  pede algo seguro para usar como nome de arquivo.

## O que o inglês corrigiu em si mesmo, e o que isso decidiu em `pt` (`settings.updates.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`, `menu.app.showAll`/`hideOthers`)

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

## Uma operação revertida pela metade: concluir a reversão (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

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

## O aviso do que a reversão conseguiu, e o que ela deixou (`fileOperations.cancelRollback.*`, `fileOperations.rollbackConfirm.body`)

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
  **`Não foi possível reverter`** (a voz de "couldn't" da seção Error-copy-phrasings + o `reverter` do termbase; o
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
- **`removed` e `deleted` viram os dois `apagar`** · `apagar` é o `delete` fixado no termbase e o verbo que a família da
  reversão já publica (`queue.row.reversalDeleting` "Apagando o que foi criado", `transferProgress.rollbackTooltip`) ·
  high. A terminologia da Microsoft dá `remove` → `remover`, mas no catálogo `Remover` já é tirar uma entrada de uma
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
- Nenhum valor leva apóstrofo, então não há `''` no lote. Nenhum `sameAsSourceJustification` é necessário: os 18 valores
  diferem do inglês.

## `cancelRollback.stagedLeftover.*` (as sobras do próprio Cmdr no destino)

Novas em 2026-09-02. Duas linhas sobre um arquivo de trabalho que o próprio Cmdr criou e não conseguiu tirar do destino.
NÃO pertencem à lista `reason.*`: lá o Cmdr protege os arquivos da pessoa, aqui é uma sobra dele mesmo.

- **`unfinished copy` → `cópia incompleta`** · `incompleto` é a palavra da Apple para "incomplete" (macOS `LA33`:
  "danificado ou incompleto"), `cópia` vem de `NE111` ("manter uma cópia retomável") · `high`
- **`at the destination` → `no destino`** · o termo do catálogo (`stoppedDeleting`: "O resto continua no destino") ·
  `high`
- **`transfer` (substantivo) → `transferência`** · o catálogo já usa (`errors.listing.deviceReconnecting.explanation`:
  "depois de uma transferência cancelada ou interrompida") · `high`
- A segunda frase fica no mesmo verbo `apagar` do resto da família.
- ⚠️ **`numa transferência posterior`, ❌ nunca "da próxima vez".** A limpeza do Cmdr pula tudo com menos de uma hora,
  então uma nova tentativa imediata não apaga nada. Prometer o contrário seria exatamente a falha que esta linha
  conserta.

## A tela de bloqueio quando o WebKit é antigo demais (`main.oldWebkit.*`)

Três strings que o Cmdr mostra no lugar da interface quando o Safari do Mac é antigo demais. Elas ficam no invólucro
HTML, não no app, então são a única coisa que essa pessoa vai ver do Cmdr.

- **`Software Update` → `Atualização de Software`** · nome do painel nos Ajustes do Sistema; o rastro Tier 1 do Finder
  confirma o termo (`Apple Device Software Update File` → `Arquivo de Atualização de Software do Dispositivo Apple`) ·
  `high`. Mantém as maiúsculas do nome do painel, ao contrário do uso corrido.
- **`Quit` → `Encerrar`** · já no termbase, confirmado por `Encerrar Finder` na barra de menus do Finder · `high`.
- **`Safari`, `Mac` e `15.4` ficam como estão.** `Safari` entrou para `BRAND_WORDS`.

## O aviso de macOS antigo (`main.oldMacos.*`)

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

## O que o Ask Cmdr lê dentro de um arquivo (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`)

A ferramenta `inspect_file` lê uma parte limitada de um arquivo a pedido, e a tela de consentimento passou a dizer isso.
Fontes: `_ignored/i18n/pt-BR/` (macOS Finder + SystemSettings, terminologia da Microsoft, Thunar).

- **"Looking inside files" / "Looked inside files" → `Olhando dentro dos arquivos` / `Olhou dentro dos arquivos`** ·
  `olhar dentro de` é o verbo que o catálogo já usa para o Cmdr espiar um conteúdo (`search.coverage.denied` "não teve
  permissão para olhar dentro desta pasta", `search.coverage.declined` "não olha dentro de pastas de snapshot") · high.
  Gerúndio + pretérito, o molde dos irmãos (`Buscando nas suas fotos` / `Buscou nas suas fotos`). O artigo definido
  (`dos arquivos`) porque a linha aparece enquanto o assistente lê os arquivos da pergunta; a chamada cobre até 200,
  então o plural fica neutro como no inglês.
- **archive → `arquivo compactado`** · entrada já assentada (§ Archive-browsing terms) · high.
  `a lista dos arquivos dentro de um arquivo compactado` repete `arquivo` de propósito: é a mesma repetição que a
  entrada original já aceita.
- **thumbnail → `miniatura`** · macOS Finder pt-BR (`Tamanho da miniatura:`, `miniatura pequena/média/grande` nas
  descrições de acessibilidade das visualizações), terminologia da Microsoft (id 121524 `miniatura`), Thunar
  (`Mostrar miniaturas:`) · confirmed. Já estava na antiga `askCmdr.consent.noContents`.
- **camera details (EXIF de uma foto) → `detalhes da câmera`** · câmera: terminologia da Microsoft (id 27160), macOS
  SystemSettings pt-BR (`câmeras e outros dispositivos`), e o catálogo (`onboarding.stepOptional.mtp.*`); `detalhes` é a
  palavra que `askCmdr.renameReview.evidence.metadata` já usa para metadados (`Detalhes do arquivo, não o conteúdo`) ·
  high. ❌ Não `metadados` nem `dados EXIF` nesta tela: o inglês evita o jargão de propósito.
- **location (onde a foto foi tirada) → `localização`** no item da lista e no aviso do que mudou; `onde ela foi tirada`
  na regra, que tem espaço para desfazer a ambiguidade · macOS Finder pt-BR usa `localização` para lugar (`FI12`, `BU39`
  "Escolher Localização…"), terminologia da Microsoft (id 333724) · high. A regra escreve por extenso porque
  `localização de uma foto` sozinho poderia se ler como o caminho do arquivo.
- **some text / some lines of text → `um trecho de texto` / `algumas linhas de texto`** · `trecho` é o substantivo
  natural de pt-BR para um excerto; `arquivo de texto` é o termo da Microsoft (id 120995) quando o tipo de arquivo é
  nomeado · high.
- **a few pages of a PDF along with its title and author → `algumas páginas de um PDF junto com o título e o autor`** ·
  página (Microsoft id 89939), título (id 151997), autor (id 747536) · high. `PDF` fica verbatim.
- **whole files → `arquivos inteiros`** · a regra abre com `O Cmdr nunca envia arquivos inteiros, fotos ou miniaturas`,
  trocando o antigo `os arquivos em si: nem o conteúdo dos arquivos` porque agora um pedaço do conteúdo PODE sair; a
  frase não pode mais prometer que conteúdo nenhum é enviado · high.
- **"works the same way" → `funciona do mesmo jeito`** · pt-BR coloquial, casa com o tom da tela · high.
- **Recuperado da antiga `askCmdr.consent.noContents`, palavra por palavra:** a frase da busca de fotos
  (`o texto que o Cmdr reconheceu dentro das fotos correspondentes e as etiquetas delas vão para o seu provedor para que ele possa encontrá-las`)
  e a frase das sugestões
  (`O Ask Cmdr pode sugerir renomeações, movimentações e faxinas, e nada acontece com nenhum arquivo até você aprovar.`).
  `etiquetas` = tags (o catálogo já usa em `errors.listing.attributeNotFound`), `provedor` = provider (`terms.json`),
  `faxinas` = cleanups.
- **Texto de novidades (askCmdr.consent.whatsNew.body, removido)**: a segunda frase
  (`É mais do que você aceitou na época, então aqui está tudo de novo.`) ficou como estava; só a primeira foi
  retraduzida. `um arquivo quando você pergunta sobre ele` no lugar do relativo `sobre o qual`, que soa formal demais
  para a tela.
- **`askCmdr.empty.hint` e `settings.askCmdr.intro` seguem a mesma regra**:
  `Ele lê nomes, caminhos e tamanhos, e só olha dentro de um arquivo quando você pergunta sobre ele` /
  `… só olha dentro de um arquivo quando você pergunta sobre ele e nunca muda um arquivo sem a sua aprovação`. O antigo
  `é somente leitura` e o `nunca muda nada` saíram: o Ask Cmdr propõe renomeações e escreve as próprias anotações, então
  só a aprovação pode ser prometida · high.

## As duas dicas do botão Rollback (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Superfície nova: a dica do botão agora diz o que ESTA reversão faz com os arquivos, e o botão é desligado quando uma
movimentação entre dois sistemas de arquivos chega ao último passo (apagar os originais, com tudo já no destino).

- **`rollbackTooltipStopAndMoveBack` → `Parar e levar de volta todos os arquivos movidos até agora`** · o irmão
  `rollbackTooltip` dá o molde (`Parar e …`), e `levar de volta` é a forma já assentada para o retorno ao lugar de
  origem (`cancelRollback.doneMovingBack`: «O Cmdr levou o item de volta») · `high`. ❌ Não `apagar`: reverter uma
  movimentação não apaga nada.
- **`rollbackAlreadyLandedTooltip`** · a primeira oração retoma `cancelRollback.moveAlreadyLanded` («já está no
  destino»), `reversão` é o termo assentado para o rollback (`rollbackUnavailableTooltip`), e `Cancelar` é o rótulo do
  botão ao lado (`fileOperations.button.cancel`), então entra sem mudança · `high`.

## “Abrir terminal aqui” e o seletor de app (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Superfície nova: um cartão em `Comportamento > Navegação e operações de arquivo` que escolhe o app de terminal aberto
pelo comando. O macOS monta a lista; aqui só os rótulos são traduzidos.

- **terminal (o tipo de app) → `terminal`; Terminal (o app da Apple) → `Terminal`** · o macOS em pt-BR mantém o nome em
  inglês (`Abrir no Terminal`, chave `N67` em `macOS/Finder/LocalizableMerged.json`), e a palavra genérica em português
  é o mesmo empréstimo · `high`. Por isso o título do cartão `settings.navigationAndFileOps.card.terminal` leva
  `sameAsSourceJustification`: ele é idêntico ao inglês de propósito.
- **Open terminal here (o nome do comando) → `Abrir terminal aqui`** · construído sobre o `Abrir no Terminal` da Apple,
  com `aqui` para o lugar · `high`. A tradução do próprio comando (menu, paleta) precisa usar exatamente esta forma.
- **Choose an app… → `Escolher app…`** · o `Choose Application…` da Apple (chave `N137`) diz `Escolher Aplicativo…`;
  `app` no lugar de `aplicativo`, como no resto do catálogo, e em caixa de frase · `high`.
- Nenhum valor tem apóstrofo.

## `Sort by relevance`: a dica da coluna de resultados de busca (`fileExplorer.columns.sortByRelevance`)

Superfície nova: a dica que aparece ao passar o cursor sobre o cabeçalho de coluna ativo de um painel de resultados de
busca. O clique seguinte devolve as linhas à ordem do próprio buscador, com a melhor correspondência primeiro.

- **relevance (o quanto um resultado corresponde à busca) → `relevância`** · as quatro fontes do macOS concordam, tanto
  em pt-BR quanto em pt-PT: WorkflowKit (`Relevance (WFSearchSortOrder)` → `Relevância`), AppStoreKit
  (`SEARCH_FACET_RELEVANCE` → `Relevância`), Automator (`%1$[Relevância]@ …`) e Música · `high`. Minúscula depois de
  `por`, como no resto do catálogo. (verificado no macOS 26.6.2, build 25G83, extração com `plutil` das localizações
  incluídas, 2026-09-06)
- **Moldura da frase → `Ordenar por relevância`** · exatamente o padrão das chaves irmãs em `commands.json`
  (`Ordenar por nome`, `Ordenar por tamanho`) · `high`. Sem `sameAsSourceJustification`, e o valor não tem apóstrofo.

## `Documents and packages`: a nova linha OOXML (`settings.archives.ooxml.*`)

Superfície nova: uma linha no mesmo cartão de `Arquivos zip`, acima do cartão `Pacotes de aplicativo`. Ela cobre de
propósito AS DUAS coisas: documentos do Office (.docx, .xlsx, .pptx) e pacotes de aplicativo (.jar, .apk). Por isso nem
o inglês cita o Office.

- **documents (o tipo de arquivo) → `Documentos`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Documentos`; tipos
  `Documento RTF`, `Documento de Texto Simples`) · `high`.
- **packages (genérico, não só apps) → `pacotes`** · macOS Finder (`Mostrar Conteúdo do Pacote`) e a entrada de termbase
  `app bundle → pacote` · `high`. Fica o `pacotes` puro, para a linha continuar mais ampla que o cartão
  `Pacotes de aplicativo` abaixo dela — a mesma separação que o inglês faz entre `packages` e `app bundles`.
- **Moldura da frase → `O que pressionar Enter faz em um …, … ou ….`** · exatamente a moldura das chaves irmãs
  `settings.archives.zip.description` e `settings.archives.bundle.description` · `high`. Sem apóstrofo no valor.

## O hub de servidores: painel de conexão, esquecer servidor e esquecer senha (`servers.*`, `fileExplorer.navigation.connectionTooltip*` / `disconnect*` / `forget*`)

Superfície nova: o painel que mostra o servidor conectando ou recusando (SMB, SFTP, WebDAV), o pontinho de conexão de
cada linha do seletor de volumes, e os dois diálogos de confirmação (esquecer o servidor, esquecer a senha salva). A
pilha de referência não existe nesta máquina, então as fontes vêm do macOS instalado (26.6.2, build 25G83, 2026-09-06),
o caminho que `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve.

- **server → `servidor`** · Finder pt-BR, `ConnectToWindow.strings` (`1.title` `Connect to Server` →
  `Conectar ao Servidor`, `YEA-3L-WnW.placeholderString` `Server Address` → `Endereço do Servidor`) e
  `LocalizableMerged.strings` (`FR15`, `N84`, `SD13` `Connected servers` → `Servidores conectados`) · confirmed
- **disconnect → `Desconectar`** · Finder pt-BR `LocalizableMerged.strings` `MR10.1` e `N200` (`Disconnect` →
  `Desconectar`); é o que o catálogo já publica em `fileExplorer.unreachable.disconnect` e `menu.network.disconnect`,
  então `servers.paneState.disconnect` copia byte a byte · confirmed. ❌ Nunca `Ejetar` num servidor: não há nada para
  desplugar.
- **Keychain Access → `Acesso às Chaves`; certificate → `certificado`** · o próprio app, `InfoPlist.loctable` do
  `Keychain Access.app` (`CFBundleDisplayName` pt_BR = `Acesso às Chaves`, `certificate` = `certificado`) · confirmed. O
  catálogo já usava a forma em `fileExplorer.network.share.forgetPasswordTooltip` e `ai.secretError.keychainBody`.
- **trust → `confiar` / `confiança`** · Keychain Access pt-BR (`Trust Settings` → `Ajustes de Confiança`, `is trusted` →
  `é confiável`) · confirmed. Daí `O macOS não confia no certificado deste servidor.` e
  `O Cmdr ainda não confia na chave de {host}.`
- **(SSH) host key → `chave`** · sem termo próprio no macOS pt-BR para a chave de host, e o inglês também diz só "key"
  depois da primeira menção; `chave` sozinho basta porque a frase já nomeia o servidor · high
- **SSH settings (o `known_hosts` da pessoa) → `ajustes de SSH`** · `ajustes` é a palavra da Apple para settings
  (`Ajustes do Sistema`, `Ajustes de Confiança`), e o catálogo já a publica em
  `settings.appearance.appColor.description` · high
- **sign in → `iniciar sessão` (com artigo em texto corrido: `iniciar a sessão`)** · o catálogo já assentou
  (`fileExplorer.network.signIn` = `Iniciar sessão`; em texto corrido, `servers.sheet.needsStoredSecret` =
  `… e inicie a sessão uma vez.`) · confirmed. Daí `sign-in method` → **`método de início de sessão`**, na forma nominal
  que a Apple usa em `Itens de Início de Sessão`.
- **Signed out → `Sessão encerrada.`** · estado que concorda com a SESSÃO, não com a pessoa, que é como esta § evita
  gênero sem glifo nenhum · high
- **doesn't support → `não oferece suporte a`** · forma já publicada em `errors.volume.notSupported` e
  `errors.write.trashNotSupported.message` · confirmed. Por isso `servers.refusal.authMethodUnsupported` inverte a frase
  (`O Cmdr ainda não oferece suporte ao método de início de sessão que este servidor usa.`): manter o servidor como
  sujeito exigiria um `a que` que trava a leitura.
- **That doesn't look like … → `Isso não parece …`** · molde já publicado em `common.attachEmailInvalid`
  (`That doesn't look like an email address` → `Isso não parece um e-mail`), e `endereço de servidor` compõe o
  `endereço` já assentado (`servers.sheet.address` e `servers.hub.colAddress` = `Endereço`) com a linha `server` →
  **servidor** do termbase · confirmed
- **Cmdr couldn't X → `O Cmdr não conseguiu X`** · o molde do catálogo inteiro
  (`settings.mediaIndex.reclaim.couldNotDelete`, `errors.listing.notFound.explanation`), e o `O Cmdr` por extenso é a
  regra do style.md § "Uma frase de resultado nunca fica sem sujeito" · confirmed
- **drop the connection → `encerrar a conexão`** · o catálogo já publica `desconecte para encerrá-la` em
  `fileExplorer.unreachable.detailGaveUp` · high
- **A dica do botão desligado copia a irmã do ejetar, trocando só o verbo e o substantivo do alvo.**
  `disconnectBusyTooltip` = `Não é possível desconectar enquanto há operações em andamento neste servidor`, palavra por
  palavra o `fileExplorer.navigation.ejectBusyTooltip` já publicado (`… ejetar … neste dispositivo`). As duas ocupam o
  mesmo lugar da interface e qualquer diferença de estrutura lê como outra regra.
- **Nada nestas frases concorda com `{name}`.** O `{name}` é um nome de servidor que a pessoa escolheu, então nenhum
  particípio nem pronome se apoia nele: `forgetServerConfirm` diz `tira esse servidor da lista` (não `para de listá-lo`)
  e `forgetSecretConfirm` diz `vai pedir a senha` (não `vai pedi-la`). Mesma regra do style.md § final.

Consistência de valor idêntico (`desktop-i18n-term-consistency` pareia pelo inglês, então estas são cópias byte a byte
das chaves irmãs já publicadas): `Forget server` → `Esquecer servidor` (`menu.network.forgetServer`),
`Forget saved password` → `Esquecer senha salva` (`menu.network.forgetSavedPassword`,
`fileExplorer.network.share.forgetPassword`), `Disconnect` → `Desconectar`, `Cancel` → `Cancelar`, `Try again` →
`Tentar novamente` (`fileExplorer.errorPane.tryAgain`).

`fileExplorer.navigation.disconnectPlaceAriaLabel` = `Desconectar {name}`, e a substring que satisfaz a contenção WCAG
2.5.3 é **`Desconectar`**, exatamente o rótulo visível de `servers.paneState.disconnect` e
`fileExplorer.unreachable.disconnect`.

## A tabela do hub de servidores: colunas, estados e o estado vazio (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.serverPinnedToast` / `serverUnpinnedToast` / `pinRefusedToast` / `networkVolume`, `shortcuts.scope.servers` / `places`)

Superfície nova: a linha `Servidores` do seletor de volumes agora abre um HUB, uma tabela com todo servidor salvo (SFTP,
WebDAV, SMB) mais os encontrados por perto, com as colunas Nome / Tipo / Endereço / Status / Último uso e uma linha
`Adicionar servidor…` no fim. O GRUPO onde a linha fica continua sendo `Rede` (`fileExplorer.navigation.groupNetwork`),
então `Servidores` e `Rede` passam a conviver no mesmo menu: a distinção é justamente o que esta rodada compra. A pilha
de referência não existe nesta máquina, então as fontes vêm do macOS instalado (26.6.2, build 25G83, 2026-09-06), o
caminho que `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve.

- **Servers (a linha e o título de seção) → `Servidores`** · Finder pt-BR `ConnectToWindow.strings` (`13.title`
  `Favorite Servers:` → `Servidores Favoritos:`, `48.title` `Browse` → `Explorar`) e `LocalizableMerged.strings` `SD13`
  (`Connected servers` → `Servidores conectados`) · confirmed. Em caixa de frase, como o resto do catálogo.
- **Places (a seção de atalhos dos lugares dentro de um servidor) → `Locais`** · Finder pt-BR
  `LocalizableMerged.strings` `FI1` (`Recent Places` → `Locais Recentes`) e `FF20.1_V2` (`^0 Places` → `^0 Locais`) ·
  confirmed. ❌ Não `Lugares` (o termo do Freeform e do Journal, de outra família) nem `Navegador de compartilhamentos`:
  o rótulo anterior descrevia só os compartilhamentos SMB, e a seção agora cobre qualquer lugar dentro de um servidor
  (buckets de uma conta de armazenamento depois).
- **Name → `Nome`, Type → `Tipo`, Address → `Endereço`** · Finder pt-BR (`N224` `Kind` → `Tipo`,
  `ConnectToWindow.strings` `YEA-3L-WnW.placeholderString` `Server Address` → `Endereço do Servidor`), `VPN.appex` e
  `Sound.appex` (`Type` → `Tipo`), `Bluetooth.appex` (`Address` → `Endereço`) · confirmed. São cópias byte a byte das
  irmãs já publicadas (`fileExplorer.columns.name`, `menu.sort.name`, `queryUi.ai.filter.type`), que o
  `desktop-i18n-term-consistency` pareia pelo inglês.
- **Status → `Status`, verbatim** · linha fixada em `terms.json` (`status`) e byte a byte igual a
  `licensing.section.labelStatus`, a irmã com o mesmo inglês · confirmed. O macOS pt-BR diz `Estado` (`Network.appex`
  `BRIDGE_STATUS_TABLE_COLUMN_STATUS`), mas o catálogo inteiro já publica o empréstimo naturalizado, e uma coluna que
  diverge da tela de licença seria a costura visível. Leva `sameAsSourceJustification`.
- **Last used (cabeçalho de coluna estreita) → `Último uso`** · high. As duas formas da Apple não servem aqui:
  `Última Usada` (`SecurityPrivacyExtension.appex` e `PrinterScannerSettings.appex`, `LAST_USED`) trava no feminino, e o
  sujeito é `o servidor`; `Usado pela última vez` (Mail.app `AddressHistory.loctable` `12.headerCell.title`, que é
  justamente um cabeçalho de tabela) tem quatro palavras e não cabe na coluna. A forma nominal não concorda com nada e
  fica em duas palavras.
- **Connected → `Conectado`** · `Network.appex`, `VPN.appex` e `Wi-Fi.appex` (`Connected` → `Conectado`) · confirmed.
  Byte a byte igual a `ai.cloud.connected` e `fileExplorer.network.browser.status.connected`.
- **Saved (o estado de um servidor salvo e ocioso) → `Salvo`** · Apple pt-BR, Podcasts `LISTEN_NOW_SAVED` (`Saved` →
  `Salvo`) · confirmed. Masculino, concordando com `o servidor`. Nada de errado aconteceu: é só o estado parado, e
  `Salvo` não sugere falha nenhuma.
- **nearby → `por perto`; Found nearby → `Encontrado por perto`** · Apple pt-BR, Home.app (`Nearby` → `Por Perto`,
  `Nearby Accessories` → `Acessórios por Perto`), Weather.app (`Nearby Location` → `Localização por perto`) e Setup
  Assistant (`Looking for nearby devices…` → `Buscando dispositivos por perto…`) · confirmed. ❌ Não `Próximo`, que o
  catálogo já usa no sentido de "seguinte" (`commands.navDown.label` = `Selecionar próximo arquivo`).
- **Signed out → `Sessão encerrada`** · a linha já fixada na § anterior: o estado concorda com a SESSÃO, não com a
  pessoa, o que evita gênero sem glifo nenhum · high. Não é recusa nem falha; a pessoa só precisa iniciar a sessão de
  novo.
- **check the key → `conferir a chave`** · `conferir` é o verbo do catálogo para uma verificação que a PESSOA faz
  (`askCmdr.renameReview.openPreviewTooltip` = `Abra o arquivo para conferir o nome`), enquanto `verificar` fica para o
  que o Cmdr faz sozinho (`askCmdr.renameUndo.skipReason.unverifiable.*`) · high. A espera segue o `Aguardando …` já
  publicado (`fileExplorer.network.browser.status.waitingForNetwork` = `Aguardando a rede…`), e a frase fala com a
  pessoa: `Aguardando você conferir a chave`. `chave` sozinho basta para a host key, como a § anterior fixou.
- **local network discovery → `a descoberta na rede local`** · `descoberta` é o termo da Apple (`VPN.appex`
  `Auto proxy discovery` → `Descoberta de proxy automática`; Directory Utility LDAPv3
  `Initial server information discovery` → `Descoberta inicial de informações do servidor`) e o que o catálogo já
  publica em `settings.network.firstTriggerDone.label` (`Descoberta de rede iniciada`); `rede local` em minúsculas é o
  conceito, o molde de `settings.network.enabled.description` (`na sua rede local`), enquanto `Rede Local` maiúsculo
  fica reservado para o NOME da permissão do macOS · confirmed. Desligado → **`está desativada`**, o par de
  `Quando desativado` que a mesma chave publica.
- **Settings (a janela de ajustes do próprio Cmdr) → `Ajustes`** · System Settings.app `InfoPlist.loctable` (`Settings`
  → `Ajustes`) e todo o catálogo (`Ajustes > IA`, `Ajustes > Atalhos de teclado`) · confirmed. `Ajustes do Sistema`
  continua sendo só o macOS. O link é infinitivo-imperativo: `Ativar nos Ajustes`, sobre o `Ativar rede` de
  `settings.network.enabled.label`.
- **Never (na coluna Último uso) → `Nunca`** · macOS pt-BR em toda parte (`Security.prefPane`, `Wi-Fi.appex`,
  `BatteryUI.loctable` `NEVER`) · confirmed.
- **Pin / unpin → `Fixar/desafixar`** · Safari pt-BR (`Fixar Aba`) e, mais forte, a irmã já publicada
  `commands.tabTogglePin.label` = `Fixar/desafixar aba` · confirmed. A barra fica sem espaços porque é a forma que a
  paleta de comandos já mostra; duas entradas vizinhas com espaçamento diferente leem como dois padrões.
- **volume switcher → `seletor de volumes`** · o rótulo visível já publicado em `shortcuts.scope.volumeChooser`
  (`Volume chooser` → `Seletor de volumes`) · confirmed. Nunca `alternador` (esse é o `app switcher` do macOS,
  `alternador de apps`).
- **NAS fica `NAS`** · o catálogo já publica o acrônimo sem glosa em `settings.network.smbConcurrency.description`
  (`a maioria dos NAS domésticos`) · confirmed.
- **Nada concorda com `{name}`, de novo.** Os três avisos de fixar põem um VERBO logo depois do inserto
  (`{name} agora aparece…`, `{name} saiu do…`, `…onde {name} aparece.`), e onde a frase precisa de um particípio ela
  escreve o substantivo: `O servidor continua salvo`, nunca `Continua salvo`. O `pinRefusedToast` usa o molde
  `O Cmdr não conseguiu X` que a § anterior fixou.

- **screen → `tela`**, nunca `ecrã` (o marcador pt-PT mais visível): `shortcuts.scope.errorScreen` = `Tela de erro`.

## O painel de adicionar servidor, a chave de host e as duas linhas do "Ir para o caminho" (`servers.sheet.*`, `servers.hostKey.*`, `servers.paneState.signedOut`/`signIn`/`hostKeyChanged*`, `goToPath.dialog.opensServer`/`addsServer`, `commands.serversConnect.label`)

Superfície nova: o painel modal que adiciona, edita ou reautentica um servidor (SFTP, WebDAV, SMB), o passo em que a
pessoa aprova a chave SSH do host, e duas linhas de prévia do "Ir para o caminho". A pilha de referência não existe
nesta máquina, então as fontes vêm do macOS instalado (26.6.2, build 25G83, 2026-09-06), o caminho que
`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve.

- **fingerprint (a impressão da chave SSH do host) → `impressão digital`** · Apple pt-BR,
  `ActionKit.framework/Localizable.loctable`, a ação "Executar Script Via SSH": `The host key's fingerprint is %@` →
  `A impressão digital da chave do host é %@`, e
  `The SSH server has a fingerprint that is different from the fingerprint that was saved…` →
  `O servidor SSH tem uma impressão digital diferente da impressão digital que foi salva…` · confirmed. Daí
  `Key fingerprint` → **`Impressão digital da chave`**: `chave` sozinho já é a host key (§ anterior), então `do host`
  fica de fora do rótulo curto.
- **trust (o verbo do botão) → `Confiar`** · macOS pt-BR, `UsersGroups.appex/Localizable.loctable` (`Trust` → `Confiar`)
  · confirmed, e casa com o `confiar`/`confiança` que a § do hub já fixou. Daí `Trust and connect` →
  **`Confiar e conectar`** e `Trust the new key` → **`Confiar na nova chave`**.
- **passphrase → `frase-senha`** · Apple pt-BR em todo lugar: `DiskImages2.framework` (`Incorrect passphrase` →
  `Frase-senha incorreta`), `DiskManagement.framework` (`A passphrase is required for this operation.` →
  `Uma frase-senha é exigida para esta operação.`), Certificate Assistant (`Enter Passphrase:` →
  `Digite a Frase-senha:`) · confirmed. `Key passphrase` → **`Frase-senha da chave`**: é a frase que destrava o ARQUIVO
  de chave, nunca a senha da conta, e o par `frase-senha`/`senha` separa as duas sem glosa.
- **Protocol (nome acessível do seletor SMB/SFTP/WebDAV) → `Protocolo`** · macOS pt-BR,
  `AddPrinter.app/PlugIns/IP.plugin/IP.loctable` (`Protocol:` → `Protocolo:`, e a própria descrição de acessibilidade
  `100257.ibExternalAccessibilityDescription` `Protocol` → `Protocolo`) · confirmed. A fonte é justamente um rótulo de
  leitor de tela, que é o uso desta chave.
- **Advanced (a seção recolhida) → `Avançado`** · macOS pt-BR, `Security.prefPane/Localizable.loctable` (`Advanced…` →
  `Avançado…`) e `AccessibilitySettingsWidgetExtension.appex` (`Advanced` → `Avançado`) · confirmed. Byte a byte igual a
  `settings.section.advanced`, a irmã com o mesmo inglês.
- **Remote folder → `Pasta remota`** · `Remote` → `Remoto` no Finder pt-BR (`LocalizableMerged.strings`,
  `KIND_FORMATTER_29_1`) e em `Network.appex` (`Remote Management` → `Gerenciamento Remoto`); `pasta` é o termo
  compartilhado do style.md · confirmed.
- **Key file → `Arquivo de chave`** · `chave` é o termo do macOS pt-BR para chave criptográfica (Keychain Access:
  `private key` → `chave privada`; Certificate Assistant: `public key` → `chave pública`), e `arquivo de …` é o molde do
  catálogo · high.
- **Browse… (o botão que abre o seletor de arquivos do sistema) → `Escolher…`** · o botão da Apple para essa ação é
  `Choose…` → `Escolher…` (`ControlCenterHelper.xpc/BackgroundReplacement.loctable`,
  `AccessibilitySettingsWidgetExtension.appex` `global.choose`, `GPUIExtension.appex` `Choose File…` →
  `Escolher Arquivo…`), e o catálogo já publica `Escolher app…` · confirmed. ❌ Não `Navegar`, que é o `Browse` de
  entrar num arquivo compactado (`settings.archives.opt.browse`, outro sentido), nem o `Explorar` do Finder
  (`ConnectToWindow.strings` `48.title`), que é procurar servidores NA REDE, não escolher um arquivo. ❌ Nem o
  `Procurar…` da Microsoft: o Cmdr é um app de macOS (princípio 2 de escolha de termo). O `i18n-terms` lê as duas chaves
  como UMA palavra inglesa; a divisão é real e está registrada com o motivo como `Browse` em
  `apps/desktop/scripts/i18n-term-consistency-allowlist.json` (§ `reviewed.pt`).
- **Reconnect automatically → `Reconectar automaticamente`** · Apple pt-BR, exatamente este rótulo de interruptor:
  `DisplaysSettingsIntentsExtension.appex/Localizable.loctable` (`Automatically reconnect` →
  `Reconectar automaticamente`) · confirmed.
- **Connect → `Conectar`; Guest → `Convidado`** · `NetAuthAgent.app/AuthDialog.loctable`, o próprio diálogo de conectar
  a servidor da Apple (`600218.title` `Connect` → `Conectar`, `RiA-l0-ASw.title` `Guest` → `Convidado`) · confirmed.
  `servers.sheet.connect` copia byte a byte o `Conectar` que `fileExplorer.network.connect` publica; `Connect as guest`
  → `Conectar como convidado` compõe os dois termos da Apple, já que nenhuma outra chave publica esse inglês.
- **How to connect (nome acessível da escolha convidado-ou-conta) → `Como conectar`** · o grupo equivalente da Apple se
  chama `Connect As:` → `Conectar como:` (`AuthDialog.loctable` `PHL-pS-ELV.title`), e `conectar` sem objeto é a forma
  que o catálogo já publica (`fileExplorer.navigation.connectionTooltipSaved` = `Salvo. Abra-o para conectar.`) · high
- **Sign in with a username and password → `Iniciar sessão com nome de usuário e senha`** · nenhuma outra chave publica
  esse inglês, então não há cópia byte a byte a respeitar; `iniciar sessão` é a forma que o catálogo assentou
  (`fileExplorer.network.signIn`), e `nome de usuário` e `senha` vêm das próprias irmãs do formulário
  (`servers.sheet.username`, `servers.sheet.password`) · confirmed. O português dispensa o artigo indefinido que o
  inglês usa.
- **ssh-agent, Nextcloud, SFTP, WebDAV, SMB e `ssh` ficam verbatim.** `Use ssh-agent` → **`Usar o ssh-agent`**, no molde
  `Usar …` que o catálogo publica em `settings.mcp.usePortInstead` e `fileExplorer.navigation.useSavedPasswordConfirm`;
  o artigo entra porque o nome do programa é um substantivo masculino na frase.
- **First time connecting to {host} → `Primeira conexão com {host}`** · forma nominal, que é o jeito de o título não
  concordar com nada nem soar alarmante; o inglês é deliberadamente rotineiro · high
- **quem cuida do servidor**, para `the server's owner` · o dono de um NAS doméstico costuma ser a própria pessoa, e a
  forma com `quem` evita o `o dono` masculino sem glifo nenhum (style.md § Gender) · high
- **I've checked it → `Já conferi`** · primeira pessoa, é a PESSOA falando, e o verbo é o `conferir` que a § do hub
  fixou para uma verificação feita por ela · high

Consistência de valor idêntico (`desktop-i18n-term-consistency` pareia pelo inglês, então estas são cópias byte a byte
das irmãs já publicadas): `Connect` → `Conectar` (`fileExplorer.network.connect`), `Sign in` → `Iniciar sessão`
(`fileExplorer.network.signIn`), `Cancel` → `Cancelar`, `Address` → `Endereço` (`servers.hub.colAddress`), `Name` →
`Nome` (`servers.hub.colName`), `Password` → `Senha` (`fileOperations.archivePassword.placeholder`), `Advanced` →
`Avançado` (`settings.section.advanced`), `Connect to server…` → `Conectar ao servidor…`
(`settings.network.permissionIntroConnectLink`).

`Username` → `Nome de usuário` e `Remember in Keychain` → `Lembrar no Acesso às Chaves` são os únicos valores do
catálogo com esse inglês, então não há irmã para parear. Cada um se apoia num texto corrido já publicado:
`errors.listing.authRequiredEauth.suggestion` (`digite seu nome de usuário e senha de novo`) e
`servers.sheet.needsStoredSecret`, que cita o rótulo do interruptor palavra por palavra
(`Ative “Lembrar no Acesso às Chaves” e inicie a sessão uma vez.`).

Quatro `sameAsSourceJustification`: `servers.sheet.protocolSmb`, `protocolSftp`, `protocolWebdav` (nomes de protocolo,
que o macOS pt-BR também não traduz) e `addressPlaceholder` (`nas.local`, um nome mDNS literal).

`servers.paneState.hostKeyChangedHint` escreve `A chave do servidor mudou`, com o SUBSTANTIVO, porque um `dele` se
apoiaria no `{name}` do título; mesma regra do style.md § final. E nenhuma das três frases de chave trocada soa como
falha do usuário: `O Cmdr parou de conectar a {name}` põe o Cmdr como sujeito, no molde `O Cmdr parou de …` que
`search.walkHandoff.superseded` já publica.

## Duas linhas novas no painel: a reconexão automática e o login por chave (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Duas chaves do mesmo painel de servidor. A primeira é a manchete enquanto o Cmdr, sozinho e num laço de backoff, traz de
volta uma conexão que caiu (embaixo dela: um indicador de atividade, a contagem regressiva até a próxima tentativa e os
botões `Tentar agora` / `Cancelar` / `Desconectar`). A segunda é a linha que entra sob `Sessão encerrada em {name}` NO
LUGAR do botão `Iniciar sessão…`, porque este servidor se identifica com uma chave SSH ou uma identidade do ssh-agent, e
não há mesmo o que a pessoa preencher.

A pilha de referência não existe nesta máquina, então o Tier 1 vem dos bundles do macOS instalado (26.6.2, build 25G83,
lidos em 2026-09-07), o caminho que `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve.

- **Reconnecting… (manchete de progresso) → `Reconectando…`** · Apple pt-BR, literal:
  `HomeDataModel.framework/…/HFLocalizable.loctable`, `HFServiceDescriptionReconnecting` (`Reconnecting…` →
  `Reconectando…`), e `ConversationKit.framework/…/ConversationKit.loctable`, chave `Reconnecting` (`Reconnecting` →
  `Reconectando`) · confirmed. O gerúndio é a forma pt-BR (o pt-PT diria `A reconectar`, marcador do style.md).
- **Reconnecting to {name}… → `Reconectando a {name}…`** · a preposição é `a`, como no próprio Apple pt-BR
  (`HFSymptomLongDescriptionProblemNeedCaptiveLeaseRenewalLinkString`: `Reconnect HomePod to “%@”` →
  `Reconectar HomePod a “%@”`) · confirmed. É a irmã `servers.paneState.connecting` (`Conectando a {name}…`) com o
  prefixo `Re-`, e as duas manchetes se alternam no mesmo lugar: qualquer outra forma leria como outra tela. Combina com
  o `reconexão` que `retryProgressAriaLabel` já publica e com o `Reconectar automaticamente` do painel de edição.
- **so there''s nothing to type → `então não há nada para digitar`** · o molde `então não há nada para …` já está
  publicado duas vezes no catálogo, para o mesmo inglês (`so there's nothing to eject` →
  `então não há nada para ejetar`; `so there's nothing to disconnect` → `então não há nada para desconectar`) ·
  confirmed. `digitar` é o verbo da Apple para preencher uma credencial: `NetAuthAgent.app/…/Localizable.loctable`, o
  próprio diálogo de conectar a servidor (`GENERIC_MSG_PASS`: `Enter your password to connect to “%@”.` →
  `Digite sua senha para conectar-se a “%@”.`; `SMB_MSG`, `FS_MSG_PASS`, `EMSG_INVALID_NAME_PWD` na mesma linha), e o
  catálogo já o publica em `errors.listing.authRequiredEauth.suggestion` (`digite seu nome de usuário e senha de novo`).
- **signs in with a key rather than a password → `usa uma chave em vez de uma senha para iniciar a sessão`** · `chave` é
  o termo do macOS pt-BR para chave criptográfica (linha já fixada na § do painel de adicionar servidor), `senha` vem do
  `NetAuthAgent` acima e da irmã do formulário (`servers.sheet.password` = `Senha`), `iniciar a sessão` é a forma em
  texto corrido que a § do hub fixou, e `em vez de` é o que o catálogo já usa para `rather than` / `instead of` (sete
  ocorrências já publicadas, em `ai.json`, `errors.json`, `indexing.json` e `settings.json`) · high. O sujeito é **o
  servidor que usa uma chave**, não a pessoa que faz login: `Este servidor inicia a sessão…` personificaria o servidor
  em português muito mais do que o inglês faz.
- **`uma chave`, sem `SSH`** · o inglês para em `key`, e a linha vale igual para uma identidade do ssh-agent, que não é
  um arquivo de chave. `chave SSH` fica para quando o inglês disser `SSH`. Ver a bandeira de revisão abaixo.
- **Open it again to retry. → `Abra-o de novo para tentar conectar.`** · é o molde `Abra-o para conectar.` que
  `fileExplorer.navigation.connectionTooltipSaved` já publica, mais o `de novo` da irmã duas chaves acima
  (`paneState.hostKeyChangedHint`: `abra o servidor de novo para conferir a impressão digital`) · confirmed. O `tentar`
  carrega o `retry` sem repetir `de novo`/`novamente` na mesma frase.
  - **Aqui o pronome é seguro, ao contrário do de sempre.** `-o` é masculino e o único masculino da frase é `servidor`:
    `chave` e `senha` são femininas, e o `{name}` está no título, fora desta string. Ênclise, como manda o pt-BR
    (`Abra-o`, nunca `O abra`).

Notas: nenhum dos dois valores leva apóstrofo ASCII, então não há `''` a dobrar (o `there''s` do inglês some na
tradução). O `{name}` fica intacto e nada concorda com ele; a reticência é o caractere único `…` (U+2026). Nenhum
`sameAsSourceJustification`: os dois valores diferem do inglês. Varredura pt-PT: zero `ficheiro`, `ecrã`, `estar a` +
infinitivo, `consoante`, `Rever`, `alterar o nome` ou próclise.

## Fixar no seletor, chaves de host confiáveis e o painel do ADB (`menu.network.pinToSwitcher`/`unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`/`adb`, `settings.summary.servers`/`adb`, `settings.appearance.tintSmb.*`)

Três superfícies numa rodada: os dois itens de menu que põem e tiram um servidor do seletor de volumes (mais a
notificação única que aparece quando o grupo `Rede` fica cheio), a tela `Ajustes > Sistemas de arquivos > Servidores`
com as chaves de host aceitas, e a tela do Android por ADB. Mais o matiz de painel, que deixou de ser só de SMB.

A pilha de referência não existe nesta máquina, então o Tier 1 vem dos bundles do macOS instalado (26.6.2, build 25G83,
lidos em 2026-09-07), o caminho que `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve.

- **unpin → `Desafixar`** · Apple pt-BR, Notes.app `Localizable.loctable` (`Unpin Note` → `Desafixar Nota`,
  `Pin or Unpin Notes` → `Fixar ou Desafixar Notas`) · confirmed. Casa byte a byte com a família já publicada
  (`menu.tab.unpinTab` = `Desafixar aba`, `commands.tabTogglePin.label` = `Fixar/desafixar aba`), e não soa como apagar:
  o servidor continua salvo.
- **Pin to switcher → `Fixar no seletor`** · o termo cheio é `seletor de volumes` (§ da tabela do hub), mas o item fica
  num menu de contexto estreito e o inglês encurta do mesmo jeito (`switcher`, não `volume switcher`); o menu se abre
  DENTRO do seletor, então o referente está na tela · high
- **trusted (adjetivo) → `confiável`** · macOS pt-BR, `SecurityInterface.framework/Localizable.loctable`
  (`This certificate will be marked as trusted for all users of this computer.` →
  `Este certificado será marcado como confiável para todos os usuários deste computador.`) · confirmed. Daí
  `Trusted host keys` → **`Chaves de host confiáveis`** e `Nothing trusted yet.` → **`Nenhuma chave confiável ainda.`**,
  que é o estado vazio e não uma recusa. O verbo continua sendo `Confiar` (§ do painel de adicionar servidor), e
  `host key` continua `chave do host`, o termo do `ActionKit` já fixado.
- **`Trusted` antes da data → `Confiável desde`** · `confiar em algo` não passiva em português (`Confiada em 7 de…` fica
  agramatical), então a linha usa o ADJETIVO mais `desde`, que diz o mesmo fato e não concorda com pessoa nenhuma ·
  high. O componente renderiza `{prefixo} <DateLabel>` (`ServersSection.svelte`), então sai
  `Confiável desde 2026-09-07`.
- **Check Again / Re-check → `Verificar novamente`** · macOS pt-BR, `SoftwareUpdate.framework`
  `SUSoftwareUpdateController.loctable`, chave `CheckAgain` (`Check Again` → `Verificar Novamente`) · confirmed, em
  caixa de frase como o resto do catálogo. É o Cmdr que procura sozinho, então o verbo é `verificar`, não o `conferir`
  reservado para o que a PESSOA faz (§ da tabela do hub). A linha de instalação repete o rótulo palavra por palavra.
- **Watching for phones. → `O Cmdr está aguardando celulares.`** · o molde `Aguardando …` já publicado
  (`fileExplorer.network.browser.status.waitingForNetwork` = `Aguardando a rede…`) · high. As duas linhas escrevem
  `O Cmdr` por extenso porque a negativa sem sujeito (`Não está aguardando…`) se lê como `você não está`, e a § anterior
  já fixou que uma frase de estado não fica sem sujeito. Nenhuma das duas menciona o servidor do ADB, o socket ou a
  inscrição, como manda a `@key`.
- **Browse… (o botão do seletor de arquivos) → `Escolher…`** · cópia byte a byte de `servers.sheet.browse`, que tem o
  MESMO inglês e que a § do painel de adicionar servidor já fundamentou no `Choose…` → `Escolher…` da Apple · confirmed.
  O `desktop-i18n-term-consistency` pareia pelo inglês, então as duas têm que bater.
- **Got it → `Entendi`** · cópia byte a byte das três irmãs já publicadas com o mesmo inglês (`ai.toast.gotIt`,
  `main.oldMacos.gotIt`, `updates.moveToApplicationsDialog.gotIt`) · confirmed.
- **`Your Network group is getting long` → `Seu grupo Rede está ficando grande`** · `grupo` não aceita `longo` em
  português (`lista longa`, sim; `grupo longo`, não), e `grande` é o que o pt-BR diz de um grupo com itens demais ·
  high. `Rede` é o nome do cabeçalho, byte a byte com `fileExplorer.navigation.groupNetwork`, e o rótulo interno do
  ajuste (`serversPinHintSeen.label`) repete a mesma forma, no molde `Dica de … exibida` que
  `settings.behavior.openTerminalHereToastSeen.label` já publica.
- **right-click (imperativo) → `Clique em … com o botão direito`** · molde já publicado quatro vezes
  (`errors.listing.permissionDenied.suggestion`: `clique na pasta com o botão direito, escolha Obter Informações`) ·
  confirmed. O nome do comando entra entre aspas curvas “ ”, como as irmãs recentes que citam um rótulo da interface
  (`servers.sheet.needsStoredSecret`, `settings.behavior.openTerminalHereApp.description`).
- **`It stays in the Servers list.` → `O servidor continua na lista Servidores.`** · o inglês elide o sujeito, o
  português escreve o substantivo, no molde que `fileExplorer.navigation.serverUnpinnedToast` já publica
  (`O servidor continua salvo.`) · confirmed.
- **`ask whether to trust it` → `perguntar se você confia nela`** · o `nela` é a CHAVE (feminino), o único feminino da
  frase, então o pronome fecha sozinho; a mesma forma fecha o estado vazio (`e pergunta se você confia nela`), onde o
  inglês para em `and asks` e o português precisa do complemento para a frase não ficar pendurada · high
- **`Android platform tools` → `ferramentas de plataforma do Android`, `USB debugging` → `depuração USB`,
  `Location of adb` → `Localização do adb`** · termos já fixados em `terms.json`, na rodada do ADB; a linha do campo
  vazio copia o `procura o adb nos lugares de sempre` que `settings.fileOperations.adbBinaryPath.description` publica.
- **`Tint server panes (SMB, SFTP, WebDAV)` → `Matizar painéis de servidor (SMB, SFTP, WebDAV)`** · o inglês deixou de
  falar só de SMB, e o valor antigo (`Matizar painéis SMB/rede`) descrevia a versão anterior do ajuste; `painel` e
  `matiz` vêm das irmãs `tintLocal`/`tintMtp`, que ficam na mesma lista · confirmed.

Dois `sameAsSourceJustification`: `settings.section.adb` (`Android (ADB)`, nome de produto mais a sigla da ponte de
depuração, as duas na lista de não-traduzir) e `settings.adb.status.label` (`Status`, o empréstimo naturalizado que a §
Terms já fixou e que as irmãs `licensing.section.labelStatus` e `servers.hub.colStatus` publicam).

## O acesso por ADB nos Ajustes (`settings.fileOperations.adbEnabled.*`, `settings.fileOperations.adbBinaryPath.*`)

As duas linhas que ligam o acesso aos arquivos do Android por ADB e apontam onde fica o `adb`. A pilha de referência não
existia na máquina onde esta rodada correu, então as fontes vêm do macOS instalado (26.6.2 build 25G83, 2026-09-06), o
caminho de `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

- file system / filesystem · **sistema de arquivos** · macOS pt-BR `Utilitário de Disco` (`Localizable.loctable`:
  `File system` → `Sistema de arquivos`); o catálogo já usava a forma em
  `settings.advanced.fileWatcherDebounce.description` · confirmed
- location (de um arquivo ou comando) · **localização** · macOS pt-BR Finder (`FI12` `This location is read-only` →
  `Esta localização é somente leitura`; `BU39` `Choose Location…` → `Escolher Localização…`). Rótulo em caixa de frase:
  `Localização do adb` · confirmed
- debugging / debug mode · **depuração** · Apple pt-BR, Safari `pt.lproj/DeveloperPreferences.strings`
  (`Enable … debug mode` → `Ativar modo de depuração de …`). Daí `USB debugging` → **depuração USB**, que também é o
  rótulo do próprio Android em pt-BR · high
- (Android) platform tools · **ferramentas de plataforma (do Android)** · sem fonte no macOS nem na Microsoft (é termo
  do Google); forma descritiva, com o comando `adb` como âncora concreta na mesma frase · tentative
- over ADB / via ADB · **por ADB** · segue o `por USB` / `pelo USB` que o catálogo já publica
  (`settings.fileOperations.mtpConnectionWarning.description`, `fileExplorer.navigation.spaceMtpHint`) · high. O
  interruptor se chama `Acesso aos arquivos do Android por ADB`.
- Leave this empty · **Deixe em branco** · molde já publicado em `settings.askCmdr.interactiveModel.description` · high
- `adb`, `ADB`, `Android`, `Android SDK`, `Homebrew`, `Mac`, `USB`, `MTP` ficam verbatim; `adb` em minúsculas, que é o
  nome do comando.

## O painel do celular Android e a dica de depuração USB (`adb.*`, `settings.behavior.adbHintDismissed.*`)

As 19 chaves novas do ADB: o painel cheio que substitui a listagem quando um celular não abre (`adb.connect.*`), as
dicas de passar o mouse na linha do celular dentro do seletor de volumes (`adb.readiness.*`), a linha discreta no topo
de um painel que já mostra o celular por MTP (`adb.hint.*`), o botão que fecha o celular (`adb.disconnect*`), e o par
interno que só guarda se a dica já foi dispensada.

A pilha de referência não existe nesta máquina, então o Tier 1 do macOS vem dos bundles instalados (26.6.2, build 25G83,
lidos em 2026-09-07), o caminho que `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" descreve. Os
termos do Android vêm do AOSP, que é a fonte autoritativa da tradução do próprio sistema.

- **`USB debugging` → `depuração USB`** · AOSP, `frameworks/base/packages/SystemUI/res/values-pt-rBR/strings.xml`
  (`usb_debugging_title` = `Permitir a depuração USB?`, `usb_debugging_secondary_user_title` =
  `Depuração USB não permitida`, `main`, lido em 2026-09-07) · confirmed. Casa com o `depuração` da Apple (Safari
  `DeveloperPreferences.strings`) já registrado em `terms.json`, e é palavra por palavra o que a pessoa lê no próprio
  celular. Fica em minúscula no meio da frase, como no Android.
- **`Allow` (o botão do próprio Android) → `Permitir`** · AOSP, mesmo arquivo, `usb_debugging_allow` = `Permitir` (e
  `allow` = `Permitir` em `packages/apps/Settings/res/values-pt-rBR/strings.xml`) · confirmed. É o botão que a pessoa vê
  na tela do celular, então `adb.connect.unauthorized` escreve exatamente essa palavra: `toque em Permitir`.
- **`Cmdr opens your phone as soon as you do.` → `O Cmdr abre o seu celular assim que você permitir.`** · o inglês elide
  o objeto (`as soon as you do`), que em português deixa a frase pendurada; `permitir` fecha a frase E ecoa o botão
  `Permitir` da linha de cima, que é a mesma palavra na tela do celular · high
- **`Android platform tools` → `ferramentas de plataforma do Android`** · reúso da rodada de
  `settings.fileOperations.adb*` (`terms.json`), onde ficou `tentative` por ser termo do Google sem fonte no macOS nem
  na Microsoft; `settings.adb.install.intro` já publica a mesma forma · tentative
- **`phone` → `celular`** · pt-BR, já publicado em `settings.summary.adb`,
  `settings.fileOperations.mtpEnabled.description` e `errors.listing.deviceDisconnected.explanation` · confirmed.
  Marcador de variante: o pt-PT diz `telemóvel`.
- **`isn''t responding` → `não está respondendo`** · macOS pt-BR, várias fontes de primeira mão (`loginwindow.loctable`
  `INTERRUPT_NOT_RESPONDING_TITLE_LOGOUT_STANDARD`, `ScreenReader.framework/SCRGeneral.loctable` `%@ is not responding`,
  `OpenDirectoryConfigUI` `kStatusDisplayStringODDown` = `Este servidor não está respondendo.`, 26.6.2, 2026-09-07) ·
  confirmed
- **`Open Settings` → `Abrir Ajustes`** · macOS pt-BR, o rótulo de botão da Apple em vários frameworks (`ActionKit`,
  `SiriSettingsUI` `OPEN_SETTINGS`, `AuthKit` `AUTH_ERROR_BUTTON_OPEN_PASSCODE_SETUP`, `SensitiveContentAnalysisUI`
  `AGE_VERIFICATION_ALERT_OPEN_SETTINGS`, 26.6.2, 2026-09-07) · confirmed. Sem artigo, porque é botão; a forma com
  artigo (`Abra os Ajustes`) é a da Apple em texto corrido, e o catálogo já a usa lá (`notifications.permissionDenied`).
  `Ajustes` é a janela de ajustes do próprio app (`settings.window.title`).
- **`How` (o link) → `Saiba como`** · macOS pt-BR, o molde de link da Apple para `Learn how`
  (`Ecosystem.framework/Localizable.loctable`: `Learn how to update to an Apple silicon version.` →
  `Saiba como atualizar para uma versão compatível com Apple Silicon.`; `AuthKitUI`
  `AUTHORIZATION_PRIVACY_LEARN_HOW_FORMAT`, 26.6.2, 2026-09-07) · high. Uma palavra só (`Como`) não funciona como link
  em português; `Saiba como` é o mínimo que se lê como link e emenda na frase anterior
  (`… Ative a depuração USB. Saiba como`).
- **`Dismiss` → `Dispensar`** · cópia byte a byte das dez irmãs já publicadas com o mesmo inglês
  (`crashReporter.dialog.dismiss`, `queue.row.dismiss`, `downloads.fda.dismiss`, …) · confirmed. O
  `desktop-i18n-term-consistency` pareia pelo inglês, então tem que bater.
- **`Disconnect {name}` → `Desconectar {name}`** · cópia byte a byte de
  `fileExplorer.navigation.disconnectPlaceAriaLabel`, que tem o MESMO inglês e é o mesmo botão num servidor · confirmed.
  O botão diz `Desconectar` e não `Ejetar` de propósito: nada fica seguro para desligar, o celular continua no cabo.
  `adb.disconnectBusyTooltip` segue a irmã `fileExplorer.navigation.disconnectBusyTooltip` e só troca `neste servidor`
  por `neste dispositivo`, porque o inglês também trocou.
- **`reach` (alcançar um dispositivo) → `alcançar`** · molde já publicado em `servers.refusal.unreachable`
  (`O Cmdr não conseguiu alcançar {host}.`) · confirmed
- **`didn''t answer in time` → `não respondeu a tempo`** · cópia do molde de `servers.refusal.timedOut`
  (`{host} não respondeu a tempo.`), que aparece no MESMO painel · confirmed
- **`over USB` → `pelo USB`** · o `por USB` / `pelo USB` que o catálogo já publica
  (`settings.fileOperations.mtpConnectionWarning.description`, `fileExplorer.navigation.spaceMtpHint`) · high
- **`the whole filesystem` → `o sistema de arquivos inteiro`** · byte a byte com
  `settings.fileOperations.adbEnabled.description`, que já publica
  `o sistema de arquivos inteiro de um celular Android`; `sistema de arquivos` é o termo do Utilitário de Disco
  (`terms.json` `file-system`) · confirmed
- **`Wake its screen` → `Ative a tela dele`** · `ativar` é o verbo da Apple para tirar do repouso, e o `dele` aponta
  para `celular` (masculino, único candidato na frase) · high. Sem vírgula antes do `ou`: o português não a usa numa
  lista de dois, mesmo onde o inglês põe.
- **`This phone''s Android version is too old` → `A versão do Android deste celular é antiga demais`** · a concordância
  cai em `versão` (feminino), que é o sujeito; `navegar por ele` é o verbo de `settings.summary.adb`
  (`Navegue por um celular Android…`) · high
- **`You stopped opening your phone.` → `Você parou de abrir o seu celular.`** (`adb.connect.cancelled`) · `parar`, como
  na chave paralela `search.coverage.walk.cancelled` (`Você parou esta busca`) e em `errors.volume.cancelled`
  (`O Cmdr parou isso a seu pedido.`) · confirmed. O molde `parar de` + infinitivo já é o do catálogo
  (`parou de conectar a {name}`, `parou de adicionar…`, `parou de responder`), então a frase fica verbal em vez do
  pesado `a abertura do celular`. ❌ Não `cancelou`: `Cancelar` é o RÓTULO do botão (`fileOperations.button.cancel`).
  `abrir` e `celular` vêm de `adb.connect.waitingHint` (`O Cmdr abre o seu celular…`).

O par interno (`settings.behavior.adbHintDismissed.*`) copia o molde da irmã `serversPinHintSeen.*`:
`Dica de … dispensada` e `Se a linha única que oferece … já foi dispensada.`. Ele nunca aparece na interface, mas
traduzir mantém a cobertura honesta.

Nenhuma linha diz `erro` nem `falhou`, nenhuma expõe o servidor do ADB, o transporte, o daemon ou um número de série, e
nenhuma usa `só`, `simples` ou `fácil`. Varredura pt-PT do lote: zero `ficheiro`, `telemóvel`, `ecrã`, `estar a` +
infinitivo, `consoante`, `Rever`, `alterar o nome` ou próclise. Nenhum valor leva apóstrofo, então não há `''` a dobrar;
`{name}` e `{deviceName}` ficam intactos e nada concorda com eles.

## A identidade travada do servidor (`servers.sheet.identityLocked`)

As duas linhas abaixo dos campos esmaecidos `Endereço` e `Nome de usuário`, quando o usuário EDITA um servidor salvo.

- **`the account` (o campo com que você entra no servidor) → `a conta`** · o catálogo já usa a palavra nesse sentido
  (cinco vezes em `errors.json`, três em `fileExplorer.json`) · `high`.
- **A dica nomeia as ações exatamente como os botões para os quais ela aponta**: `esquecer` de
  `menu.network.forgetServer` ("Esquecer servidor") e `adicionar` de `servers.sheet.addTitle` ("Adicionar servidor"). Um
  sinônimo ("remover", "criar") manda o leitor procurar um menu que não existe.
- **`are what name this server` → `identificam este servidor`** · a folha tem o próprio campo `Nome`
  (`servers.sheet.name`), então a frase não pode usar "dar nome": pareceria falar daquele rótulo. "identificar" diz o
  que se quer (os dois valores SÃO o servidor) · `high`.
- **`add it again` → `adicione-o de novo`** · `de novo` é a forma dominante do catálogo (66 ocorrências só em
  `errors.json`) e soa mais falada que `novamente` · `high`.

## O toast quando não havia senha salva (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Não havia nenhuma senha salva de {name}.`** · repete `senha salva` e o
  `de {name}` das três irmãs já publicadas (`menu.network.forgetSavedPassword` e
  `fileExplorer.navigation.forgetSecretConfirmTitle` = `Esquecer senha salva`, `.forgetSecretConfirm` =
  `Esquecer a senha salva de {name}?`, `.forgetSecretRefusedToast`) · `high`.
- **O imperfeito `Não havia` dá o tom de constatação** que o `@key` pede: nada deu errado e não havia o que fazer, então
  nada de desculpa nem de `erro`. `salva` concorda com `senha`, nunca com `{name}` (§ style.md, «Nada concorda com um
  `{name}`»).
- A pilha de referência não estava nesta máquina (`_ignored/i18n/` também não existe no clone principal), então a
  decisão se apoia no catálogo já publicado e no termbase.

## A duração das tentativas, o título da chave do servidor e o botão Permitir do Android (`servers.paneState.retryTotalSeconds`/`retryTotalMinutes`, `servers.refusal.hostKeyRevoked`, `servers.hostKey.*`, `adb.connect.unauthorized`)

- **`{seconds}`/`{minutes}` agora têm um plural ICU com DOIS marcadores** (`servers.paneState.retryTotalSeconds`,
  `.retryTotalMinutes`): `{seconds}` só escolhe o ramo, e o que se lê é `{secondsText}`, o número já formatado. O
  português precisa de `one`, `many` e `other` (CLDR, § style.md), então os três ramos são escritos mesmo quando `many`
  e `other` coincidem. As formas vêm de `main.quit.countdown` (`{secondsText} segundo(s)`) e de
  `indexing.eta.hoursMinutesLeft` (`{minutesText} minuto(s)`) · `high`.
- **Os dois valores são peças de `servers.paneState.retryKeepsTrying`**
  (`Vai continuar tentando por um total de {duration}.`), então ficam nus, sem preposição nem ponto.
- **`Cmdr won't connect to {name}` → `O Cmdr não vai se conectar a {name}`** · a mesma perífrase da irmã
  `servers.refusal.hostKeyRevoked` (`O Cmdr não vai se conectar a esse servidor.`). O inglês trocou «stopped connecting»
  por uma recusa permanente, e o pretérito («parou de conectar») soava a uma tentativa interrompida · `high`.
- **`Allow` é o botão do próprio Android → `Permitir`**, copiado de `adb.connect.unauthorized`
  (`Confira seu celular e toque em Permitir.`), sem aspas como lá, para o usuário ler a mesma palavra que vê na tela. O
  verbo `tocar em` vem da mesma chave, e `celular` é a palavra pt-BR já usada no catálogo · `high`.
- A pilha de referência não estava nesta máquina (`_ignored/i18n/` também não existe no clone principal), então a
  decisão se apoia no catálogo já publicado e no termbase.

## O menu de contexto da linha do servidor: `Abrir` e `Editar servidor…`

- **`Open` (numa linha de servidor) → `Abrir`** (`menu.network.open`) · igual a `menu.file.open`, porque é o mesmo
  sentido: entrar em alguma coisa, não entregar um arquivo a um app. O português não separa as duas acepções, e o macOS
  também não: o Finder usa o mesmo verbo em `Abrir` (`LocalizableMerged` `N151`), `Abrir Com` (`N152`) e
  `Abrir em Nova Janela` (`FV7`, o sentido de entrar) (Finder 26.6.2, build 25G83, lido em 2026-09-07) · `high`.
- **`Edit server…` → `Editar servidor…`** (`menu.network.edit`), copiado byte a byte de `commands.serversEdit.label` ·
  `high`. Os dois abrem a mesma folha; dois rótulos diferentes se leriam como duas funções. As reticências são o
  caractere ÚNICO `…` (U+2026) e ficam. Sem artigo, como a irmã `menu.network.forgetServer` («Esquecer servidor»).
- **As duas igualdades são conferidas por um check**, não são só capricho: o `i18n-terms` avisa quando duas chaves com o
  mesmo valor em inglês divergem em português. Reescrever uma obriga a reescrever a outra.
- **`menu.*` é uma família RAW**: o Rust desenha o menu por `menu_t`, nunca por `t()`. Os apóstrofos ficam SIMPLES e um
  `''` dobrado quebra o `i18n-icu`. Nenhum dos dois valores tem apóstrofo.
- A pilha de referência não está nesta máquina, mas o `Finder.app` dá a mesma evidência de nível 1 direto do sistema
  (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## O menu de contexto da barra de teclas de função (`menu.context.hideFunctionKeyBar`, `settings.appearance.showFunctionKeyBar.*`)

- function key bar (a linha de botões de comando das teclas de função na parte inferior da janela) → barra de teclas de
  função · já definido no catálogo (`settings.appearance.showFunctionKeyBar.label`); reutilizado para o item do menu de
  contexto e o respectivo aviso · high

## A IA deixou de se chamar Ask Cmdr fora do painel de chat (`askCmdr.wake*`, `settings.askCmdr.proactive.description`, `ai.cloudConsent.askCmdr.*`)

O inglês passou a guardar `Ask Cmdr` só onde o nome aponta para o PAINEL de chat em si (o título dele, o item do menu
Visualizar, o comando da paleta, a seção de ajustes, o interruptor que liga e desliga). Toda frase que apenas descrevia
o que a IA faz trocou o sujeito: a maioria diz `Cmdr`, algumas dizem `the AI`. Quem manda é o inglês da chave, nunca a
memória de como a chave era antes.

- the AI (sujeito de uma frase) · **a IA** · forma que o catálogo já publica em `ai.translateError.*`
  (`A IA demorou demais`, `A IA retornou vazia`, `A IA está desativada`) · `high`
- AI features · **os recursos de IA** · `settings.ai.tooltipOff` (`Os recursos de IA estão desativados.`) e
  `settings.ai.provider.description` (`Escolha como os recursos de IA são alimentados.`) · `high`
- chat (o painel, como substantivo) · **o chat** · Microsoft pt-BR terminology (`chat` → `chat`), e o catálogo já diz
  `Abra o chat` e `Seus chats` · `high`
- **`chat`, `conversar` e `conversa` convivem, e cada um segue o inglês da SUA chave.** `chat` (substantivo) é `o chat`
  (`abre um chat`, em `settings.askCmdr.proactive.description`); `to chat` é `conversar` (`Converse com uma IA…`);
  `conversation` é `conversa` (`abre uma conversa`, em `ai.cloudConsent.askCmdr.proactive`). O inglês separa os três em
  chaves vizinhas, então o português separa também. Por isso `askCmdr.sessions.wakeThread` virou `este chat`: o inglês
  diz `chat`, e a tradução antiga dizia `conversa`.
- provider (de IA) · **provedor** · Microsoft pt-BR terminology (`provider` → `provedor`), já a forma do catálogo ·
  `high`
- **`Cmdr` como sujeito leva artigo: `O Cmdr`.** É o que o catálogo já faz (`O Cmdr nunca envia arquivos inteiros`,
  `O Cmdr tem vários recursos de IA`), e a § "Uma frase de resultado nunca fica sem sujeito" do `style.md` pede o
  sujeito por extenso mesmo quando o inglês o elide.
- **A segunda frase de `askCmdr.wake.needsFullDiskAccess` copia `search.coverage.setUpFullDiskAccess`**
  (`Configurar o acesso total ao disco`), porque a `@key` manda as duas telas dizerem a mesma coisa. Fica
  `Clique para configurar o acesso total ao disco.` · `high`
- **`para ti` era resíduo pt-PT** em `askCmdr.wakeToast.title`; virou `para você`, o pronome pt-BR que a § Variant do
  `style.md` já lista como marcador de variante.

## Os passos de configuração de provedor de IA (`onboarding.cloudSetup.*`)

Chaves `onboarding.cloudSetup.*`, revistas contra a pilha de referência (`pt-BR/microsoft-terminology/`).

- placeholder (o texto de exemplo dentro de um campo) · **espaço reservado** · Microsoft pt-BR terminology
  (`placeholder` → `espaço reservado`) · `high`
- deployment (do Azure) · **implantação** · Microsoft pt-BR terminology (`deployment` → `implantação`, três entradas) ·
  `high`
- download (verbo) · **baixar** · Microsoft pt-BR terminology (`download` → `baixar`) e o catálogo
  (`downloads.toast.downloaded`: `{fileName} baixado`) · `high`
- endpoint · **ponto de extremidade** · Microsoft pt-BR terminology, entrada `endpoint` no sentido exato daqui ("The
  logical representation of a location, typically expressed in URL form"), mais `endpoint address` →
  `endereço do ponto de extremidade` e `API Endpoint` → `Ponto de Extremidade de API`, que dão a forma do rótulo. A
  Apple é muda: nem `endpoint` nem `extremidade` aparecem em `pt-BR/macOS/`, então o Tier 2 decide. `high`. As duas
  chaves que carregam a palavra mudaram JUNTAS (`onboarding.cloudSetup.step.endpoint` → `URL do ponto de extremidade`, e
  a legenda `onboarding.cloudSetup.hint.azureEndpoint`), porque um rótulo em inglês sobre uma legenda em português lê
  como dois campos. O empréstimo também era minoria: 9 dos 11 catálogos completos já usavam o termo nativo (de
  `Endpunkt`, fr `point de terminaison`, es `extremo`, sv `slutpunkt`, hu `végpont`, vi `điểm cuối`, zh `端点`, zh-Hant
  `端點`).
- **`then` de um passo em duas etapas vira `e depois`**, não some: `onboarding.cloudSetup.step.lmStudioServer` diz
  `Carregue um modelo no LM Studio e depois inicie o servidor local`, porque a ordem é a informação do passo.
- `Ollama`, `LM Studio`, `Azure OpenAI`, `Azure`, `api-version` e o comando `ollama pull llama3.2` ficam verbatim.

## O convite para fixar o Cmdr no Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Pilha de referência lida em `_ignored/i18n/pt-BR/` (o conjunto brasileiro; o `pt/` nu é europeu, § style.md).

### Os três nomes da Apple

- **`Dock` → `Dock`, verbatim.** O macOS pt-BR não traduz: o Finder publica `Add to Dock` → **`Adicionar ao Dock`**
  (`macOS/Finder/LocalizableMerged.json` `N169.13` e `macOS/Finder/MenuBar.json` `300772.title`, lidos em 2026-09-09). É
  a única ocorrência de `Dock` em toda a pilha macOS pt-BR, e ela mantém a palavra inglesa · confirmed
- **`Finder` → `Finder`, verbatim**, masculino e com artigo (`no Finder`, `do Finder`, `ao lado do Finder`) ·
  `macOS/Finder/Localizable.json` (`Search for files and folders in Finder` → `Busca arquivos e pastas no Finder`;
  `New Finder windows show:` → `Novas janelas do Finder mostram:`) · confirmed
- **A pasta `Applications` → `pasta Aplicativos`** · `macOS/Finder/LocalizableMerged.json` `TL_HELP_APPS`
  (`Ir para a pasta Aplicativos`), `TL5` e `GROUP_APPLICATIONS`, mais `macOS/Finder/Localizable.json` (`Applications` →
  `Aplicativos`). O catálogo já publica a forma com possessivo em `updates.moveToApplicationsDialog.howTo`
  (`arraste-o para a sua pasta Aplicativos`), e `notAdded` copia esse molde · confirmed
- **`configuration profile` → `perfil de configuração`** · Apple, `macOS/Finder/InfoPlist.json` (`Configuration Profile`
  → `Perfil de Configuração`); a Microsoft pt-BR usa a mesma forma. Em caixa de frase aqui, como o resto do catálogo ·
  confirmed
- **`managed` → `gerenciado`** · macOS pt-BR (`Managed iCloud Drive` → `iCloud Drive Gerenciado`) e o próprio catálogo
  (`errors.provider.*`: `Esta pasta é gerenciada pelo …`). O agente que administra vira `Quem gerencia este Mac`, o
  mesmo verbo, para a frase não trocar de família no meio · confirmed
- **`log in` (na conta do Mac) → `iniciar a sessão`** · o termo Apple já travado no termbase para `Sign in`
  (`Iniciar Sessão…`, Finder `NE104`) e já publicado em `fileExplorer.navigation.connectionTooltipNeedsSignIn` e
  `servers.paneState.signedOutNothingToAsk` · confirmed

### `pin` / `unpin` no Dock: `fixar` / `desafixar`

O par já é o do catálogo (`menu.tab.pinTab`/`unpinTab`, `menu.network.pinToSwitcher`/`unpin`,
`commands.tabTogglePin.label`, `servers.pinHint.body`) e o da Microsoft pt-BR (`fixar` / `desafixar`, TBX). O inglês
pede que `pinned` (body) e `unpin` (unpinNote) continuem parentes, e `fixado` / `desafixar` são a mesma raiz · confirmed

O botão de aceitar NÃO usa esse par: ele copia o rótulo da Apple, **`Sim, adicionar ao Dock`**, porque é literalmente a
ação que o Finder chama de `Adicionar ao Dock`. Quatro palavras, cabe no botão. O possessivo do inglês (`my Dock`) cai,
como cai em quase todo rótulo de ação em pt-BR.

### `No, thanks` → `Não precisa`, e por que não `Não, obrigado`

`obrigado` concorda com QUEM FALA, então num botão ele impõe um gênero ao usuário. A § "Gender and inclusive language"
do `style.md` manda reestruturar para o neutro sempre que o resultado ainda soe natural, e `Não precisa` é uma recusa
educada corriqueira em pt-BR, sem gênero, curta e definitiva (o Cmdr não pergunta de novo). Recusadas:

- `Não, obrigado` · gendered · o motivo acima.
- `Agora não` · é a tradução já publicada de `Not now` (askCmdr.consent.decline, já removido) e promete uma próxima vez
  que não existe: depois deste botão o convite nunca mais aparece.
- `Não, valeu` · gíria demais para o registro do catálogo.

Confiança: `high` no neutro, `medium` em `Não precisa` ser a melhor das neutras.

### As quatro linhas de desfecho não podem soar como falha

Regra da casa (nenhuma mensagem de resultado usa `erro` nem `falha`, em nenhuma língua), então nenhuma das quatro traz
`erro`, `falha`, `falhou`, `não foi possível` nem culpa alguém:

- `added` · **`O Cmdr agora está no Dock.`** · fato consumado, sem parabéns. O sujeito vem por extenso (`O Cmdr`), como
  manda a § "Uma frase de resultado nunca fica sem sujeito" do `style.md`.
- `addedButDockDidNotRestart` ·
  **`O ícone do Cmdr já está no lugar, mas o Dock não recarregou. Ele vai aparecer na próxima vez que você iniciar a sessão.`**
  · o `já está no lugar` é o ponto inteiro da chave: a fixação ACONTECEU, só o redesenho ficou faltando.
  `não recarregou` descreve o Dock, não uma falha do Cmdr. ❌ Nunca escrever `não foi adicionado` aqui.
- `managedDock` ·
  **`Seu Dock é gerenciado por um perfil de configuração, então o Cmdr não consegue se adicionar. Quem gerencia este Mac pode mudar isso.`**
  · `não consegue` (capacidade) em vez de `não pode` (permissão), que soaria a proibição pessoal; a segunda frase nomeia
  quem levanta a restrição, sem sugerir contorno.
- `notAdded` ·
  **`O Cmdr não entrou no Dock desta vez. Você pode arrastá-lo da sua pasta Aplicativos até o Dock quando quiser.`** ·
  `não entrou … desta vez` é factual e não acusa ninguém; a saída manual vem no molde já publicado de
  `updates.moveToApplicationsDialog.howTo`. O `Dock` repete de propósito: sem ele, `arrastá-lo` fica sem destino.

### `a few days` fica vago

**`há alguns dias`**, nunca um número. O limiar pode mudar e o contador só começou a existir quando a funcionalidade
saiu, então qualquer numeral seria mentira para metade de quem lê. O progressivo é **gerúndio** (`vem usando`), a forma
pt-BR; `está a usar` seria marcador pt-PT (§ style.md).

### Ênclise nesta família

`deixá-lo`, `desafixá-lo` e `arrastá-lo` são seguros porque o único antecedente possível no toast é masculino (`o Cmdr`,
`o ícone`), o critério que o `style.md` já fixa em "Um pronome enclítico só entra quando o gênero fecha sozinho".
Ênclise sempre, nunca próclise.

### As duas chaves internas

`settings.behavior.dockPinNudgeOfferedAt.*` nunca aparece na tela. Seguem o molde das irmãs `*Seen`
(`openTerminalHereToastSeen`, `doubleClickOnPaneNotificationSeen`, `serversPinHintSeen`): rótulo em frase nominal
(`Oferta do Dock feita`) e descrição no formato `Se a … única … já foi …`
(`Se a oferta única de adicionar o Cmdr ao Dock já foi feita.`) · high

## O menu do ícone do Cmdr no Dock (`menu.dock.*`)

Cinco itens do menu que aparece ao clicar com o botão direito no ícone do Cmdr no Dock. Família RAW (`menu.*`):
apóstrofo SIMPLES, `{name}` e `{parent}` são alvos literais de substituição, nunca argumentos ICU. Nenhum valor leva
apóstrofo, então não há `''` no lote. Superfície nativa: nenhuma captura de tela pode fotografá-la.

### A fonte Tier 1 deste menu não está na pilha

O próprio Dock publica esse menu em
`/System/Library/CoreServices/Dock.app/Contents/Resources/pt_BR.lproj/DockMenus.strings` (lido com
`plutil -convert json -o -`, macOS 26.6.2 build 25G83, 2026-09-09). A pilha de referência traz Finder, AppKit e Ajustes
do Sistema, mas **não** o Dock, então esse arquivo é a fonte que decide a forma "verbo + nome do app". Repare na pasta:
`pt_BR.lproj` é o brasileiro e `pt_PT.lproj` o europeu (aqui a Apple não usa o `pt` nu que o AppKit usa).

- **`Open <app>` → `Abrir Cmdr`, sem artigo e sem aspas** · o Dock pt-BR usa o nome do app cru nesse molde: `HIDE_NAME`
  → `Ocultar %@`, `SHOW_NAME` → `Mostrar %@`, e `OPEN` sozinho → `Abrir`. A forma com aspas (`OPEN_FILENAME` →
  `Abrir “%@”`) é a de ARQUIVO e não vale aqui · confirmed. Bate com o que o catálogo já publica em `menu.app.hide`
  (`Ocultar Cmdr`) e `menu.app.quit` (`Encerrar Cmdr`); o artigo só entra em `Sobre o Cmdr`, seguindo o `Sobre o Finder`
  da Apple.

### Os três itens que também existem no Finder ou na barra de menus

- **`Go to folder…` → `Ir para pasta…`** · macOS Finder pt-BR, `macOS/Finder/MenuBar.json` `261.title`
  (`Ir para Pasta…`; o inglês `Go to Folder…` está na mesma chave de `en-GB/macOS/Finder/MenuBar.json`) · confirmed. Só
  o TERMO vem do Finder, não a capitalização (§ Menus nativos), daí a caixa de frase. Distinto de `menu.go.goToPath`
  (`Ir para o caminho…`), que é outro comando e outro inglês.
- **`Connect to server…` → `Conectar ao servidor…`** · macOS Finder pt-BR, `MenuBar.json` `266.title`
  (`Conectar ao Servidor…`), mais o título da janela em `ConnectToWindow.json` `1.title` · confirmed. É a linha
  `Connect to server` que este termbase já trava.
- **`Search files…` → `Buscar arquivos…`, byte a byte igual a `menu.edit.searchFiles`** · o inglês das duas chaves é o
  mesmo (`sourceHash` `149a9d1`), o `desktop-i18n-term-consistency` compara pelo inglês, e os dois itens disparam o
  MESMO comando: qualquer diferença de palavra leria como dois comandos · confirmed.

### `{name} ({parent})` fica idêntico ao inglês

`menu.dock.locationInParent` desambigua duas linhas de pasta que sairiam com o mesmo nome. O valor não muda em pt-BR, e
isso é sourced, não preguiça: o compositor `%@ (%@)` do AppKit sai como `%1$@ (%2$@)` no `pt` da Apple
(`AppKit.framework/Resources/Common.loctable`, macOS 26.6.2 build 25G83, 2026-09-09), enquanto a MESMA chave é de fato
adaptada em outros idiomas (`ja` com parênteses de largura plena, `zh_CN` sem espaço, `ar`/`he` com isoladores
bidirecionais) · confirmed. Ou seja, a Apple olhou para essa chave idioma a idioma, e o português brasileiro manteve
parênteses ASCII com um espaço antes, na ordem núcleo → qualificador.

Nada pode concordar com `{name}` nem com `{parent}`: são nomes de pasta vindos do disco, então a linha não leva artigo,
particípio nem adjetivo (a mesma regra do § "Nada concorda com um `{name}`" do `style.md`). Registrado com
`sameAsSourceJustification` na própria chave.

## A oferta de “Mostrar no Finder” e o aviso da primeira vez (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Dois momentos da mesma função: a oferta única de abrir no Cmdr o “Mostrar no Finder” de outros apps, e o aviso único na
primeira vez que um desses pedidos cai aqui. As duas superfícies apontam para comandos do próprio macOS, então a
terminologia da Apple vence (style.md § superfícies do sistema).

- **“Show in Finder” → `“Mostrar no Finder”`, com aspas curvas duplas** · Já fixado em
  `settings.navigationAndFileOps.card.showInFinder` e `settings.revealHandler.description` · `high`. Os toasts copiam
  essa forma exata, para que o cartão dos ajustes e o aviso chamem a mesma ação do mesmo jeito.
- **pane → `painel`** · Forma do catálogo em `fileExplorer.doubleClickHint.body` · `high`.
- **Settings (a janela do próprio Cmdr) → `os Ajustes`** · `settings.window.title` = “Ajustes” · `high`.
- **“for a while now” → `há um tempo`** · Deliberadamente vago: o limite pode mudar, então ❌ nunca um número. Mesma
  regra de `main.dockPinNudge.body` (“há alguns dias”) · `high`.
- **O aviso da primeira vez ❌ não é um pedido de desculpas** · Ele diz o que acabou de acontecer, por quê, e onde fica
  o botão. Daí `O Cmdr está configurado para receber esses pedidos`, ❌ nunca “desculpe” · `high`.

## Termos da reescrita da introdução (`onboarding.moreAbout`, `onboarding.wizard.stepTooltip`, `onboarding.stepFda.why`/`.ifAllow`, `onboarding.stepAi.*`, `onboarding.stepBeta.checklist.*`/`.signup.*`/`.openBeta`, `onboarding.stepOptional.*.summary`)

- star (o BOTÃO do GitHub) · **Adicionar aos favoritos** · o próprio GitHub em português chama assim
  (`docs.github.com/pt/…/saving-repositories-with-stars`: "clicar em **Adicionar aos favoritos**", e "Adicionado aos
  favoritos" quando já está), e a Microsoft terminology pt-BR fecha o mesmo verbo (`star`, verbo, "To mark an entity
  with a star" → `adicionar aos favoritos`, BRA, id 2644689; lido em 2026-09-09) · confirmed. Daí `checklist.star` =
  `Adicionar o repo aos favoritos no GitHub`. `repo` fica curto porque é rótulo de link, e o parágrafo `stepBeta.star`
  já publicava `repo`.
- stars (o CONTADOR do GitHub) · **estrelas** · o mesmo GitHub em português usa `Estrelas` para a contagem, e o
  `stepBeta.star` já publica "225 estrelas" · confirmed. ⚠️ **O botão e o contador usam palavras DIFERENTES de
  propósito**, e isso não é deriva: é exatamente o que a pessoa vê na página do GitHub, então a linha do link nomeia o
  botão que ela vai procurar e a nota logo abaixo nomeia o número que ela vai ver. (A Microsoft chega a manter `star` em
  inglês para o sentido de repositório, id 3107904 BRA; ninguém em pt-BR lê "star" como substantivo, então fica fora.)
  `checklist.starNote`.
- Like (o botão do AlternativeTo) · **Curtir** · MS terminology pt-BR, três entradas convergentes (`like` → `curtir`,
  "To express approval for a certain item"; `Like` → `Curtir`; `like` → `curtida` para o substantivo) · confirmed. O
  AlternativeTo não tem interface em português (só lista os idiomas do app), então não há rótulo do próprio site para
  copiar e `curtir` é o verbo de aprovação social do pt-BR. `checklist.alternativeTo`.
- checklist · **lista de verificação** · MS terminology pt-BR, duas entradas (`checklist` e `check list` →
  `lista de verificação`) · confirmed. `checklist.title` =
  `Lista de verificação da introdução, cada item leva 30 segundos:`, mantendo a promessa dos 30 segundos, que é o ponto
  da linha.
- Save (o botão ao lado do campo de e-mail) · **Salvar** · macOS Finder pt-BR (`LocalizableMerged` `AL2` e `BN38`,
  `Save` → `Salvar`) · confirmed. ⚠️ **As duas linhas de status NOMEIAM esse botão** (`signup.rejected`,
  `signup.unreachable` dizem "clicar em Salvar"), então elas copiam o rótulo palavra por palavra; trocar um sem o outro
  manda a pessoa procurar um botão que não existe.
- typo · **erro de digitação** · o catálogo já publica a mesma frase em `licensing.error.badSignatureHint` ("Please
  double-check for typos" → "Confira se não há erros de digitação") · confirmed. O "erro" aqui é da digitação da pessoa,
  não do app, então não colide com a regra de nunca dizer `erro`/`falha` sobre o que o Cmdr fez.
- mailing list · **lista de e-mails** · sem fonte direta: a MS terminology só tem `lista de distribuição` e
  `lista de endereçamento`, os dois do grupo de distribuição do Exchange, que é outro conceito, não uma lista opt-in
  (gotcha 2 do § Researching terms) · tentative
- signup server · **servidor de inscrição** · MS terminology pt-BR (`sign up` → `inscrever-se`) · high. A abertura da
  linha copia o molde já publicado de `onboarding.cloudSetup.status.connectionError` ("Não dá para acessar…"), no
  passado: `Não deu para acessar o servidor de inscrição agora` — nada de `falha` nem `erro`.
- "Local Network" (a linha do painel de privacidade do macOS) · **Rede Local** · citada ao pé da letra, tal como o macOS
  pt-BR a publica (`SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable`, chave `LOCAL_NETWORK`; o
  resumo `LOCAL_NETWORK_SUMMARY` fala em "permissão para buscar e se comunicar com dispositivos na sua rede local",
  macOS 26.6.2 build 25G83, lido em 2026-09-09) · confirmed. O resumo de meia linha e a legenda longa atrás do glifo de
  informação ficam a dois cliques um do outro, então dizem a mesma coisa. ❌ Não a paráfrase `Acesso à rede local`: esse
  nome não existe nos Ajustes do Sistema, então quem for procurar não acha.
- step (uma etapa numerada de instrução) · **etapa** · o catálogo já usa `etapa` para as etapas do assistente
  (`wizard.stepProgress`, `wizard.backAria`, `stepBeta.footer.continue`) e nunca `passo`; a pilha não decide (o macOS
  pt-BR não tem a palavra em contexto de instrução) · high. Daí `stepFda.ifAllow` = `Três etapas fáceis:`: a introdução
  inteira fala uma palavra só para "step".
- Why? (link isolado que abre a explicação) · **Por quê?** · com circunflexo, a forma do `por quê` isolado ou em fim de
  frase; o `Por que este nome` do catálogo é o outro caso (átono, no meio da frase) · high
- badge · **selo** · MS terminology pt-BR (`badge` → `selo`, "A small image that provides a visual indication of roles,
  contribution levels, achievements") · confirmed. Já era a palavra do `stepBeta.openBeta`.
- latest version · **última versão** · MS terminology pt-BR (`latest version` → `última versão`) · high.
  `stepOptional.updates.summary`.
- dumb (o modelo local) · **burro** · o inglês escolhe "dumber" de propósito (a `@key` diz isso), e
  `bem mais burro do que` é o comparativo franco e natural em pt-BR · high. `stepAi.local.tooltip`.
- native handler (o processo do macOS que o MTP suprime) · **processo nativo do macOS** · o `stepOptional.mtp.desc` já
  chama de "esse processo do macOS", e a MS terminology dá `suppress` → `suprimir` · high. ❌ Não `manipulador` (a
  tradução literal de `handler` na MS terminology): a legenda longa desta mesma tela já disse `processo`, e um resumo
  não troca a palavra da legenda que está logo atrás dele.

⚠️ **A tooltip do modelo local CITA o rótulo da opção de nuvem.** `stepAi.local.tooltip` fecha com
`<strong>Sim, eu quero IA</strong>`, que é o valor de `stepAi.cloud.label` byte a byte. As duas chaves são uma unidade:
mexer no rótulo sozinho manda a pessoa procurar uma opção com outro nome.

**Os quatro resumos de meia linha do `stepOptional` não têm sujeito, e o sujeito implícito é o INTERRUPTOR**, nunca a
pessoa (`Precisa aceitar…`, `Ocupa 1 GB…`, `Uma verificação mínima…`, `Permite conectar…`). É o que dispensa o `você`
que a § Variant do `style.md` cobra em frases de resultado: aqui a linha descreve o ajuste, e as quatro compartilham a
forma para lerem como uma coluna só. Elas também **não podem quebrar linha**, então cada uma ficou igual ou mais curta
que o inglês (a de indexação diz `Ocupa 1 GB` e deixa `de espaço` implícito; a de atualizações fecha em
`para ficar na última versão`, um infinitivo sem sujeito, em vez de um `para você ficar atualizado` que imporia gênero).

- released copy / Dev and test builds · **versão lançada** / **builds de desenvolvimento e de teste**
  (`settings.revealHandler.notProductionBuild`) · Microsoft terminology (`PORTUGUESE (BRAZIL).tbx`: `build` → `build` /
  `compilação`, `release` → `versão`), e o catálogo já escreve `pastas … de build` em `search.systemDirExclude.default`.
  `build` é masculino (`apagar um deles`). `versão` e não `cópia`: a frase fala de uma versão publicada, como
  `commands.appCheckForUpdates.description`, não de um segundo processo como `main.instanceLock.alertBody`. A segunda
  frase copia o molde de `settings.revealHandler.notInApplications`
  (`deixaria cada clique em “Mostrar no Finder” apontando para o nada`) · high

## O painel do visualizador enquanto o arquivo chega (`viewer.pull.*`, `viewer.error.stoppedResponding`)

Quatro chaves: o painel no meio do visualizador enquanto o Cmdr copia um arquivo de um celular, de um servidor ou de
dentro de um arquivo compactado para um arquivo temporário, e a mensagem de quando os dados param de chegar por uns 45
segundos.

- Fetching (trazer um arquivo antes de mostrá-lo) · **Obtendo** · macOS Finder pt-BR `IN_MD1` (`Fetching…` → `Obtendo…`)
  · high. A MS terminology só tem `Fetch` / `FetchXML`, nome próprio do Dynamics CRM (outro sentido). ❌ Não `Baixando`
  (Finder `PE126`): um arquivo compactado local não é baixado de lugar nenhum. ❌ Não `Buscando`: é o `Searching` do
  próprio visualizador (`viewer.search.searching`).
- to preview it · **para pré-visualizar**, sem pronome · copia `viewer.error.tooLargeToPreview`
  (`muito grande para pré-visualizar daqui`); sem o `-lo`, nada concorda com `{fileName}` · high
- `{doneText} of {totalText}` · **`{doneText} de {totalText}`** · Finder pt-BR `PW3` / `PW8` (`^0 de ^1`) e
  `askCmdr.context.tooltip` · high
- so far · **até agora**, sem particípio · a forma fixa de `search.imageResults.paused`; um `recebidos` concordaria com
  a unidade que já chega formatada (`1 byte`) · high
- stopped arriving · **`Os dados deste arquivo pararam de chegar`** · o `parou de avançar` de
  `transferProgress.stallUnknown`; `Este arquivo parou de chegar` é decalque. `Verifique se … ainda está conectado`
  copia `errors.listing.couldntReadUnknown.suggestion`, e `tente novamente` ecoa o botão `Tentar novamente`
  (`viewer.error.retry`) logo abaixo · high

## O índice do celular que fica amarelo com o cabo ligado (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`/`bodyPhone`)

As versões de celular (ADB) das três strings de disco desatualizado. O celular nunca avisa quando os arquivos mudam,
então o índice fica amarelo mesmo conectado; nenhuma das três pode falar em desconectar.

- **`keeps this index current` → `mantém este índice atualizado`** · `atualizado` / `desatualizado` é o par da família
  do ponto do índice (`tooltipFresh` = `Indexado e atualizado`, `staleDialog.title` = `pode estar desatualizado`) e do
  Finder pt-BR (`LocalizableMerged.json` `NE103`/`NE105`: `Os itens podem estar desatualizados.`) · high. ❌ Não
  `em dia`: aparece só no sentido de pôr em dia (`indexing.step.catchUp`), e quebraria o par com o título.
- **`rescan` → `nova varredura`** · o molde de `tooltipStale` (`Faça uma nova varredura`) e o substantivo travado acima
  (`terms.json` `scan`) · confirmed. A irmã `indexing.staleDialog.body` diz `nova varredura` também; `análise` é o termo
  reservado da pré-contagem de transferência.
- **`{name} doesn''t tell Cmdr when its files change` → `{name} não avisa o Cmdr quando os arquivos do celular mudam`**
  · `its files` viraria `dele` ou `nele`, que concordam com `{name}`; a frase escreve o substantivo, como manda o §
  «Nada concorda com um `{name}`» do `style.md` · high
- **`the changes Cmdr makes itself` → `as mudanças feitas pelo próprio Cmdr`** · o `feitas pelo próprio Cmdr` espelha o
  `feitas no celular` da frase seguinte, e evita um `ele mesmo` que poderia apontar para o celular · high
- **`stays as a reminder` → `fica como lembrete`** · sem fonte na pilha; forma corriqueira pt-BR · tentative
- `phone` → `celular` (§ O painel do celular Android) e `yellow status` → `status amarelo` (`staleDialog.body`,
  `settings.indexing.staleNotify.description`) reutilizados sem mudança.

Nenhum valor leva apóstrofo. Varredura pt-PT: zero `ficheiro`, `telemóvel`, `estar a` + infinitivo, próclise.

## Pasta raiz e pasta inicial de um servidor salvo (`servers.sheet.rootFolder*` / `startFolder*` / `nameHelp`, `servers.refusal.startFolderOutsideRoot` / `rootNotFound` / `startFolderNotFound` / `saveUnconfirmed`)

A folha de adicionar ou editar um servidor SFTP ou WebDAV ganhou dois campos: o TETO no servidor (o Cmdr nunca navega
acima dele) e a pasta onde o painel abre (a própria raiz ou uma pasta dentro dela; em branco = a raiz). Os dois termos
se repetem nas nove chaves (rótulos, legendas e recusas), então cada um é uma palavra fixa só.

- **root folder → `pasta raiz`** · MS terminology pt-BR (`root folder` → `pasta raiz`, BRA, id 233534; a mesma entrada
  oferece `diretório raiz` e `pasta de nível superior`, e `pasta` é o termo do catálogo) e macOS Finder pt-BR (`SC11`
  `Nenhuma pasta raiz encontrada…`, a linha `volume root` acima); o catálogo já publica `A pasta raiz de um volume` em
  `errors.json` · high. Sem acento: `raíz`, como no Double Commander pt-BR (`Go to root directory` →
  `Ir para a pasta raíz.`), é grafia errada. Substitui o rótulo antigo `Pasta remota` (a chave "Remote folder", que
  saiu), porque o campo agora é o teto e não só "a pasta do outro lado".
- **start folder → `pasta inicial`** · Total Commander pt-BR (`5071` e `5785` `Pasta &inicial:`, o campo de pasta de
  partida de um botão ou comando de menu) e Double Commander pt-BR (`in all start path...` →
  `em todos caminhos iniciais...`) · high. O macOS pt-BR não tem o conceito (nenhum `Pasta inicial` em
  `/System/Applications` nem `/System/Library/CoreServices`, macOS 26.6.2 build 25G83, 2026-09-11). ❌ Não
  `pasta pessoal` (o `Home` do Finder) nem `pasta base` (o `home folder` da MS): as duas nomeiam a pasta do usuário, não
  o ponto de abertura de um servidor.
- **never goes above this folder → `nunca sobe além desta pasta`** · `subir` é o movimento de navegação para cima (a
  linha `parent folder` → `Ir para a pasta superior`); `sobe acima` seria pleonasmo, e `além` diz o teto sem ele · high
- **Where the server opens → `A pasta em que o servidor abre`** · o fragmento inglês sem núcleo soa solto em pt-BR,
  então a legenda nomeia a pasta; o `Deixe em branco para abrir na pasta raiz` repete o `abrir` e o termo do rótulo
  irmão · high
- **Leave it empty → `Deixe em branco`** · molde já publicado em `settings.fileOperations.adbBinaryPath.description`
  (linha `Leave this empty` acima) · confirmed
- **call this server by its account and host → `usar a conta e o host como nome deste servidor`** · o
  `chamar este servidor` literal deixa em aberto quem chama; o nome montado é `ada@nas.local`, então a legenda diz o que
  vira nome. `conta` é o termo da irmã `servers.sheet.identityLocked` (`O endereço e a conta identificam este servidor`)
  e da MS terminology pt-BR (`account` → `conta`); `host` fica verbatim (linha `host (network)`) · high
- **Cmdr can''t open → `O Cmdr não consegue abrir`**, no presente porque o inglês é `can''t`, não `couldn''t`; `O Cmdr`
  por extenso, como manda o `style.md` · high. **Check that it exists → `Confira se ela existe`**: `conferir` é o verbo
  de uma verificação feita pela pessoa (`style.md` § Notes, a mesma folha já diz `Confira a impressão digital`), mesmo
  que as sugestões de `errors.listing.*` usem `Verifique se`. `lê-la` é seguro porque `pasta` é o único antecedente
  feminino antes do pronome (`conta` é o sujeito da oração) · high
- **Try again in a moment → `Tente novamente em instantes`** · a forma fixa da linha `"Try again in a moment."` acima
  (`operationLog.dialog.loadError`, `settings.mediaIndex.clip.deleteFailed`) · confirmed. `não respondeu a tempo` copia
  `servers.refusal.timedOut`, a irmã que abre com o mesmo `{host}` · confirmed. `nada foi salvo` fica passivo porque o
  inglês é passivo e `nada` é o sujeito, então a frase não fica sem sujeito.

Nenhum valor leva apóstrofo, então não há `''` a dobrar, e nenhum precisa de `sameAsSourceJustification`. Varredura
pt-PT: zero `ficheiro`, `estar a` + infinitivo, `guardar`, próclise antes de infinitivo, `tu`.

## Por que um compartilhamento não monta ou a lista não carrega (`errors.mount.*`, `errors.shareList.*`)

As frases sob «Não foi possível montar o compartilhamento» (`fileExplorer.networkMount.mountFailedTitle`) e «Não foi
possível conectar a {hostName}» (`fileExplorer.network.share.connectFailedTitle`), mais os avisos
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` e `servers.refusal.accountNotPermitted`. Tier 1 do pacote
instalado `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11, lado `pt_BR`),
que redige esses mesmos casos para «Conectar ao Servidor» e não está na pilha.

- **share → `compartilhamento`** · NetAuthAgent `EINFO_NO_SHARE` («O compartilhamento “%@” não existe no servidor.») ·
  high
- **guests → `convidados`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` («Este servidor não permite acesso a convidados.») ·
  high
- **reach → `alcançar`**, **didn't answer in time → `não respondeu a tempo`**, **isn't responding →
  `não está respondendo`** · `servers.refusal.unreachable`, `servers.refusal.timedOut`, a linha `isn't responding` acima
  · high
- **server settings (o painel de um NAS) → `configurações do servidor`** · não é o app Ajustes da Apple, então fica a
  palavra genérica pt-BR · tentative
- **when you're ready → `quando quiser`** · evita `pronto/pronta` · high
- **package → `pacote`** · KDE Dolphin («Não foi possível encontrar pacote %1.»),
  `licensing.acknowledgements.npmHeading` · high. **distribution (Linux) → `distribuição`** · a TBX só traz o sentido
  logístico · tentative
- **there's nothing to speed up → `então não há nada para acelerar`** · o molde fixo de `style.md` · high
- **Try again in a moment → `Tente novamente em instantes.`** · `operationLog.dialog.loadError` · high
- Mesmo inglês, mesmo português: `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`. Aspas retas, como `errors.volume.permissionDenied`.
  Varredura pt-PT: zero `ficheiro`, `estar a` + infinitivo, próclise pt-PT (`se recusou` e `se conectar` são a colocação
  pt-BR).

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `editor de texto`** · terminologia da Microsoft pt-BR (`text editor` → `editor de texto`) · `high`. O
  tipo de app, ❌ não o app TextEdit da Apple, cujo nome chega em `{app}`.
- **default text editor → `editor de texto padrão`** · o catálogo (`commands.fileEdit.label` "Editar no editor padrão")
  · `high`.
- **Edit files in [app] → `Editar arquivos com`** · a frase continua no menu; `com` aceita qualquer nome de app sem
  contração de gênero (`no`/`na`) · `tentative`.
- `{app}` vem depois de `em`, sem artigo, pelo mesmo motivo. Dismiss e Open settings iguais a
  `commands.handler.openTerminalHere.dismiss` / `commands.handler.openTerminalHere.openSettings`.
- **system default → `padrão do sistema`** · o catálogo (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) · `high`. `settings.behavior.textEditorApp.systemDefault` põe o nome
  do app entre parênteses depois, como `settings.appearance.language.opt.systemWithLanguage`.
- "Choose an app…" e "Checking your apps…" iguais a `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. A dica (`fileExplorer.edit.hint`) segue
  `commands.handler.openTerminalHere.hint`, sem dizer onde fica o ajuste: o botão dela leva direto.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (ejeção ou desmontagem em andamento) → `O disco {name} está sendo desconectado`** · gerúndio
  do já fixado `disconnect → Desconectar`, como o Thunar `pt-BR` (“Desmontando o dispositivo” / “Ejetando dispositivo”)
  · `high`. O substantivo `disco` vem na frente para que o particípio concorde com ele, e o gênero desconhecido de
  `{name}` não pesa. “Left its index as it was” → “como estava”, como `operationLog.rollback.partiallyRolledBackNotice`;
  “try again in a moment” → “em instantes”, como `fileExplorer.pane.directConnectionMountNotRespondingToast`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (já aconteceu, o disco saiu) → `O disco {name} foi desconectado`** · pretérito do já fixado
  `disconnect → Desconectar`, como `indexing.staleDialog.body` · `high`. O substantivo “disco” vem na frente para o
  particípio concordar com ele, e o gênero de `{name}` não pesa — mesma solução da irmã
  `fileExplorer.navigation.driveIndex.driveLeaving`, mas no passado, porque lá a ejeção ainda está em andamento. “Starts
  from scratch” → “começa do zero”. A varredura é `varredura`, o substantivo do termbase para a varredura de disco, e
  não “análise”, reservado à pré-contagem de transferência (`indexing.rescan.incompletePreviousScan` diz “A varredura
  anterior” também).

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.*`)

As quatro linhas com lado (`errors.write.deviceDisconnected.sided.destination.copy`,
`errors.write.deviceDisconnected.sided.destination.move`, `errors.write.deviceDisconnected.sided.source.copy` e
`errors.write.deviceDisconnected.sided.source.move`) entram no MESMO diálogo das genéricas
`errors.write.deviceDisconnected.message.copy` e `errors.write.deviceDisconnected.message.move`, e dividem com elas a
`errors.write.deviceDisconnected.suggestion`. Família RAW, sem ICU, apóstrofo simples (nenhum dos quatro leva
apóstrofo). O cabo foi arrancado: não é ejeção voluntária, então o pretérito **foi desconectado** vale para as quatro, e
não o gerúndio de `fileExplorer.navigation.driveIndex.driveLeaving`.

- **was disconnected (o disco saiu sozinho) · `O disco {volumeName} foi desconectado`** · idêntico à irmã
  `indexing.needsFreshScan.afterDisconnect` (§ A drive unplugged mid-index); `disconnect → Desconectar` é o termo do
  macOS pt-BR (`LocalizableMerged.json`: “Desconectar”) · high. O substantivo **disco** vem na frente para o particípio
  concordar com ele: `{volumeName}` é um nome de disco arbitrário e não pode carregar gênero.
- **drive nestas quatro é `disco`, mesmo que as genéricas digam `dispositivo`** · o inglês faz o mesmo corte (“The
  device was disconnected” nas genéricas, “still on the drive” nestas): as com lado só existem quando há um VOLUME com
  nome, as genéricas cobrem também celular e servidor. ❌ Não uniformize as duas famílias · high.
- **your originals are untouched · `Seus originais continuam intactos`** · **intactos** é o que o catálogo já publica
  para “untouched” (`errors.write.destinationNotFound.message.copy` “Os originais estão intactos.”,
  `errors.write.notConnected.message.destination` “Seus arquivos estão intactos.”) · high. O **continuam** entra no
  lugar de “estão” porque o inglês acrescenta “where they were”, e é o verbo que a próxima linha fixa.
- **where they were · `onde estavam`** · `fileOperations.cancelRollback.moveAlreadyLanded` já publica “originais
  continuam onde estavam” para um “are still where they were” · confirmed.
- **so nothing is lost · `então nada se perdeu`** · a passiva-reflexiva mantém a tranquilização em voz ativa e no fim da
  frase, que é onde o inglês a põe; o pile só tem a forma perifrástica (“serão perdidas se você não as salvar”, macOS
  pt-BR) · tentative. ❌ Não “nada foi perdido” (passiva sem agente, mais fria) nem “nada se perde” (genérico demais
  para um fato já ocorrido). A irmã `errors.write.originalsKeptAside.message.one` fecha com “Nada foi descartado.”, que
  é outro sentido (descartar ≠ perder).
- **The rest are still on the drive · `O resto continua no disco`** · cópia exata do molde já publicado em
  `fileOperations.cancelRollback.stoppedDeleting` (“The rest are still there.” → “O resto continua no destino.”) ·
  confirmed. O `no disco` sem nome aponta para `o disco {volumeName}` do começo da frase, o único chamado de “disco”
  ali: repetir o placeholder seria acrescentar um token que o inglês não tem.
- **`{counterpart}` entra sem artigo depois de `para`, e com `no disco` depois de `em`** · `para {counterpart}` dispensa
  artigo e não prejulga gênero, então `errors.write.deviceDisconnected.sided.source.copy` e
  `errors.write.deviceDisconnected.sided.source.move` usam essa forma;
  `errors.write.deviceDisconnected.sided.destination.move` precisa de “em”, que em pt-BR pede artigo, então escreve o
  substantivo (`no disco {counterpart}`), o mesmo recurso de `o disco {volumeName}`. ❌ Nunca `do {counterpart}` nem
  `no {counterpart}` · high.
- **before Cmdr could finish the move · `antes de o Cmdr concluir a movimentação`** · a regência
  `antes de o Cmdr {infinitivo}` já está publicada em `errors.listing.connectionDropped.explanation` (“antes de o Cmdr
  terminar de ler”); **movimentação** é o substantivo de “move” fixado na § Error-copy phrasings e já publicado em
  `errors.write.cancelled.message.move` e `errors.write.deviceDisconnected.message.move`; **concluir** é o verbo do
  Finder para completar, e a irmã `errors.volume.deviceDisconnected` já diz “antes de a alteração ser concluída” ·
  confirmed.
- **after Cmdr copied/moved · `depois que o Cmdr copiou` / `moveu`** · molde já publicado em
  `fileOperations.cancelRollback.reason.drift.named` (“depois que o Cmdr colocou lá”) · high. O sintagma
  `{done} de {total} arquivos` fica na ordem do inglês; os dois números chegam já formatados, então nada de ICU em volta
  deles.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.*`)

Nem falha nem perda: o Cmdr copiou, não conseguiu PROVAR que o destino gravou, e por isso guardou os originais. As
quatro linhas (`errors.write.moveNotConfirmed.title`, `errors.write.moveNotConfirmed.message.named`,
`errors.write.moveNotConfirmed.message.unnamed` e `errors.write.moveNotConfirmed.suggestion`) não podem soar como “a
movimentação deu errado”, pela mesma regra das linhas de desfecho da § O convite para fixar o Cmdr no Dock.

- **Couldn''t confirm the move · `Não foi possível confirmar a movimentação`** · a abertura de “couldn''t” fixada na §
  Error-copy phrasings, e a mesma da irmã `fileExplorer.rename.unconfirmed` (“Não foi possível confirmar a renomeação de
  …”) · confirmed. Título em caixa de frase e sem ponto final, como `errors.write.destinationNotFound.title`.
- **Cmdr couldn''t confirm · `O Cmdr não conseguiu confirmar`** · no CORPO o inglês nomeia o sujeito, então o português
  também: `O Cmdr não conseguiu {infinitivo}` é o molde já publicado em `errors.eject.unexpected` (“o Cmdr não conseguiu
  identificar o quê”) · high. O impessoal fica só para o TÍTULO, onde não há espaço para sujeito.
- **the moved files were saved · `os arquivos movidos foram salvos`** · **salvar** é o verbo travado do termbase (❌
  nunca `guardar`, indício pt-PT); a passiva é do inglês e o sujeito é `os arquivos`, então a frase não fica sem sujeito
  · high. `servers.refusal.saveUnconfirmed` usa a mesma passiva pelo mesmo motivo (“nada foi salvo”).
- **so it kept your originals · `então ele manteve seus originais onde estavam`** · o pronome **ele** retoma `O Cmdr`
  explicitamente porque a terceira pessoa também se lê como `você` (§ Notes do `style.md`, “Uma frase de resultado nunca
  fica sem sujeito”); `onde estavam` é a forma da seção acima · high.
- **on {volumeName} · `no disco {volumeName}`** · `em` pede artigo, e `{volumeName}` não pode carregar gênero, então
  entra o substantivo, como nas quatro linhas com lado. Na variante sem nome, o inglês diz “at the destination” e o
  português usa **no destino**, o termo que o `errors.json` já publica dezenas de vezes · confirmed.
- **Have a look at · `Dê uma olhada em`** · o catálogo já traduz esse mesmo “Have a look” assim em
  `settings.askCmdr.memory.description` · confirmed. Mantém o convite leve que o inglês tem; ❌ não “Verifique o
  destino”, que soa a dever de casa e colide com o `verificar` reservado ao que o Cmdr faz sozinho.
- **try the move again · `tente mover de novo`** · **tente … de novo** é o fecho de sugestão padrão de todo o
  `errors.json` (`errors.write.deviceDisconnected.suggestion`, `errors.write.readError.suggestion`) · confirmed. O
  infinitivo **mover** em vez do substantivo `a movimentação` porque o substantivo já está no título logo acima e a
  repetição pesa; o sentido é o mesmo.
- **Your originals haven''t moved · `Seus originais não saíram do lugar`** · sem atestação no pile (nenhuma ocorrência
  de “sair do lugar” no macOS pt-BR) · tentative. A alternativa atestada é `continuam onde estavam`
  (`fileOperations.cancelRollback.moveAlreadyLanded`), recusada aqui só porque repetiria palavra por palavra a frase do
  corpo, que aparece logo acima no MESMO painel; o inglês também varia a formulação entre as duas.

## An unfinished move's staging folder left in place (`fileOperations.leftovers.stagingFolderKept`)

Toast informativo quando um disco volta (ou o Cmdr inicia) e há a pasta de trabalho de uma movimentação que não
terminou, com arquivos dentro. O Cmdr deixa TODOS eles onde estão de propósito: podem ser a única cópia da pessoa,
porque a movimentação talvez já tenha tirado os originais. Nada é pedido e nada corre risco. ❌ Nunca sugerir apagar a
pasta, e ❌ nunca as palavras `erro` ou `falha`. Atenção à distinção com a § `cancelRollback.stagedLeftover.*` acima: lá
é sobra do PRÓPRIO Cmdr no destino, aqui são os arquivos DA PESSOA que ele está protegendo.

- **unfinished move · `movimentação incompleta`** · `incompleto` é a palavra da Apple para "incomplete" (macOS pt-BR
  `LA33`: "danificado ou incompleto"), já fixada na § `cancelRollback.stagedLeftover.*` para `unfinished copy` →
  `cópia incompleta`; **movimentação** é o substantivo de "move" da § Error-copy phrasings, publicado em
  `errors.write.cancelled.message.move` · high. ❌ Não `inacabada` (sem atestação na pilha) nem
  `movimentação que não terminou` (perifrástico e mais longo num toast).
- **hidden (sentido dotfile) · `oculta`** · macOS pt-BR Finder atesta exatamente esse sentido: "Se você decidir
  continuar e usar um nome que comece com um ponto, o arquivo ficará oculto." (`LocalizableMerged`, lido em 2026-09-16);
  o catálogo já publica `arquivos ocultos` em `menu.view.showHiddenFiles`, `commands.viewShowHidden.label` e
  `fileExplorer.rename.hiddenAfterRename` · confirmed. Termo do Finder, não da Microsoft (princípio 2). É a única parte
  acionável da frase: sem ligar os arquivos ocultos, a pessoa não vê a pasta.
- **a folder named {folderName} · `uma pasta chamada {folderName}`** · molde literal da Apple pt-BR, que usa
  `uma pasta chamada "^0"` e `um item chamado "^0"` em várias folhas do Finder · confirmed. `chamada` concorda com
  `pasta`, nunca com o placeholder.
- **`em uma`, ❌ nunca `numa`, nesta frase** · o Finder pt-BR escreve `em uma` por extenso e não tem nenhuma ocorrência
  de `numa` ("está em uma pasta que você não tem permissão para modificar"); o catálogo também prefere `em uma pasta` (4
  chaves) a `numa pasta` (1) · high.
- **on {volumeName} · `no disco {volumeName}`** · o esquive já fechado: `em` pede artigo e `{volumeName}` é texto
  arbitrário que não pode carregar gênero, então o substantivo **disco** vem na frente, como em
  `errors.write.moveNotConfirmed.message.named` e `indexing.needsFreshScan.afterDisconnect` · confirmed.
- **left them in place · `deixou tudo onde está`** · `ficar/deixar onde está` é o molde da família
  (`errors.write.readOnlyDevice.source.suggestion`: "Os originais ficam onde estão.";
  `fileOperations.cancelRollback.leftBehind`: "estes ficaram onde estão") · high. O **tudo** invariável entra no lugar
  do pronome objeto: evita a próclise/ênclise que o `style.md` regula e ainda diz "todos eles", que é o que a
  `@key.description` pede. Presente (`está`), não `estava`: os arquivos continuam lá agora. ❌ Não `no lugar`, que o
  catálogo já usa no sentido de "em vez de" (`main.revealNudge.body`, `onboarding.stepOptional.indexing.descCost`).
- **Sujeito explícito `O Cmdr`** · a § Notes do `style.md` ("Uma frase de resultado nunca fica sem sujeito") e o molde
  de `errors.write.moveNotConfirmed.message.named`; os dois verbos (`encontrou`, `deixou`) dividem o mesmo sujeito, sem
  brecha para ler `você`.
- ⚠️ **A tranquilização ficou um pouco mais fria que a do inglês.** "left them in place" carrega a deliberação do Cmdr
  na própria escolha de verbo; `deixou tudo onde está` diz o fato e o `tudo` recupera o "every one of them", mas não há
  um equivalente pt-BR que soe tão intencional sem acrescentar palavras que o inglês não tem (`deixou tudo intacto`
  prejulga que nada foi tocado, e `intacto` já está reservado a "untouched" em
  `errors.write.deviceDisconnected.sided.*`). Compensação: `no disco {volumeName}` + `em uma pasta oculta chamada …`
  mantêm a frase concreta e endereçável, que é a função do toast.

## O menu de favoritos (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent` / `favoritesAlreadyAdded` / `favoritesCantAddHere` / `seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D abre a lista de pastas marcadas como um MENU sobre o painel em foco: as nove primeiras linhas trazem as teclas 1–9,
que levam direto à pasta, e a última linha é o `0`, que adiciona a pasta atual do painel. O seletor de volumes não tem
mais uma SEÇÃO de favoritos: no lugar dela ficou uma linha única no topo (`seeFavorites`), que troca o seletor por esse
menu.

- **favorites (a lista do Cmdr) · `favoritos`** · macOS pt-BR Finder, Tier 1 (`LocalizableMerged`: `FI10`, `TL4`,
  `SD8.1` → `Favoritos`; `TL_HELP_FAVS` → "Ir para a sua pasta de seus Favoritos", lido em 2026-09-16); o catálogo já
  publica `Favoritos` em `fileExplorer.navigation.groupFavorites` e `favoritos` minúsculo em
  `commands.favoritesAdd.label` e `fileExplorer.navigation.favoritesEmpty` · confirmed. Minúscula quando o inglês
  escreve `favorites` minúsculo (é substantivo comum), maiúscula só onde ele nomeia a SEÇÃO (`Favorites`), como em
  `groupFavorites` e `commands.favoritesAdd.description`.
- **favorites menu · `menu de favoritos`** · composição direta do termo acima; no cabeçalho de escopo dos atalhos vira
  **`Menu de favoritos`**, na mesma forma `substantivo + de + substantivo` que as chaves vizinhas já usam
  (`shortcuts.scope.volumeChooser` = `Seletor de volumes`, `.commandPalette` = `Paleta de comandos`, `.fileList` =
  `Lista de arquivos`) · high.
- **Show favorites · `Mostrar favoritos`** · `Mostrar` é o verbo de "Show" no Finder pt-BR (`MenuBar`:
  `Mostrar Opções de Visualização`, `Mostrar Conteúdo do Pacote`, `Mostrar Original`, `Mostrar Janela de Progresso`,
  lido em 2026-09-16), e o catálogo já publica `Mostrar detalhes técnicos`
  (`commands.errorPaneToggleTechnicalDetails.label`) · confirmed. Caixa de frase, não o Title Case do Finder (regra da §
  Menus nativos). `commands.favoritesOpen.label` e `menu.go.showFavorites` têm o MESMO inglês, então o
  `desktop-i18n-term-consistency` exige o mesmo valor byte a byte: os dois são `Mostrar favoritos`.
- **current folder · `a pasta atual`** · macOS pt-BR Finder (`Localizable`: `Search the Current Folder` →
  `Buscar na Pasta Atual`, lido em 2026-09-16); o catálogo já publica `a pasta atual do painel em foco` em
  `commands.favoritesAdd.description` · confirmed. `atual` é a pasta que o painel está MOSTRANDO, não a que está sob o
  cursor; nenhuma das duas chaves precisa desambiguar isso porque o menu só tem uma leitura possível.
- **mounted share · `compartilhamento montado`** · o catálogo já fechou os dois pedaços: `mounted shares` →
  `compartilhamentos já montados` em `settings.network.enabled.description` e `settings.network.permissionWithout`, e
  `network share` → `compartilhamento de rede` em nove chaves de `errors.json` · confirmed. ❌ Nunca `partilha` (pt-PT)
  nem um nome de protocolo (SMB/MTP/ADB): o inglês evita o jargão de propósito e a tradução mantém isso.
- **point at (um favorito apontando para uma pasta) · `apontar para`** · molde já publicado em
  `errors.listing.notAFolder.suggestion` e `isAFolderErrno.suggestion` ("confirme que ele aponta para uma pasta, não
  para um arquivo") · high.
- **disk · `disco`** · macOS pt-BR Finder (`Discos rígidos`, `Discos externos`, `Disco de Inicialização`), e a § Notes
  do `style.md` já trava `disco` como o termo do disco em todos os sentidos · confirmed.
- **press a key · `pressione`** · o catálogo já publica `pressione <chip>{key}</chip>` (`downloads.toast.inAppHint`),
  `Pressione as teclas...` (`downloads.shortcutRow.pressKeys`) e `Pressione {binding}`
  (`downloads.toggleDescription.bound`) · confirmed. ❌ Não `aperte`, que o catálogo não usa.
- **Registro das duas famílias de verbo, que esta rodada mistura**: um `commands.*.label` vai no INFINITIVO
  (`Selecionar arquivo anterior`, `Abrir item sob o cursor`, `Mostrar detalhes técnicos`), e um `commands.*.description`
  vai no IMPERATIVO de 2ª pessoa (`Abra…`, `Adicione…`, `Leve…`). Por isso `favoritesOpen.label` é `Mostrar favoritos` e
  `favoritesOpen.description` começa com `Abra`.

### `seeFavorites`: as categorias de plural

`{count, plural, =0 {Ver favoritos} one {Ver {count} favorito} many {Ver {count} favoritos} other {Ver {count} favoritos}}`

- **Categorias `one` / `many` / `other`**, as do CLDR para `pt` (a § Plurals do `style.md`), mais o braço `=0` que o
  inglês carrega. `many` existe de verdade em português moderno (números compactos), e mesmo com o texto igual ao de
  `other` ele é obrigatório: 33 plurais do catálogo de `fileExplorer` e 12 do de `settings` já escrevem os três braços.
  Verificado com `intl-messageformat` em `pt`: 0 → `Ver favoritos`, 1 → `Ver 1 favorito`, 2 → `Ver 2 favoritos`, 1000000
  → `Ver 1000000 favoritos`.
- **O `=0` não diz "0"** (é a frase de "você ainda não tem nenhum") e é o único braço SEM `{count}`, como o inglês.
- **See · `Ver`** · todas as chaves cujo inglês começa em "See" já saem com `Ver`: `Ver detalhes da licença`
  (`commands.appLicenseKey.seeDetails.label`, `menu.app.licenseDetails`), `Ver por quê`
  (`askCmdr.wakeToast.openThread`), `Veja o que mudou…` (`commands.helpWhatsNew.description`) · high.

### A linha `0` tem UMA chave só

`fileExplorer.navigation.favoritesAddCurrent` = **`Adicionar a pasta atual aos favoritos`**, a última linha do menu. A
lista de atalhos CITA essa linha para explicar a tecla `0` em vez de redescrevê-la, então lê o mesmo valor. As duas
chaves que existiam antes saíam idênticas em português de qualquer forma: o inglês as separava só pelo artigo `the`, e o
português é obrigado a escrever `a pasta atual`. ❌ Não invente uma segunda redação para a lista de atalhos. A distinção
que a `@key.description` pede de verdade — não confundir com o comando real `commands.favoritesAdd.label`
(`Adicionar aos favoritos`) — continua de pé, porque esta chave acrescenta `a pasta atual`.

### `favoritesAlreadyAdded`: reestruturado para `já está nos favoritos`

`Esta pasta já está nos favoritos`, não `Esta pasta já é um favorito`. O predicativo forçaria `um favorito` (masculino)
concordando com `pasta` (feminino), que soa torto; `estar nos favoritos` espelha o `Adicionar aos favoritos` que a
pessoa acabou de ler na mesma linha do menu. Continua sendo uma constatação calma, sem `erro` nem `falha`, como a
`@key.description` pede.

### `favoritesOpen.description`: o destino agora está no inglês também

`Abra o menu de favoritos no painel em foco e pressione um número para ir até esse favorito.` `ir` sozinho não fecha
frase em português: pede destino. O inglês antes parava em "press a number to go" e esta tradução inventava o destino
(`a pasta`); agora o inglês diz "jump to that favorite", então o português nomeia o mesmo alvo, com `o favorito`
contável, o mesmo substantivo de `commands.favoritesOpenByNumber.label` · high. `ir até` em vez de `ir para` para não
emendar dois `para` seguidos; o molde `ir até` já está no catálogo (`downloads.empty.message`: "Ir até lá mesmo assim?";
`errors.listing.notFound.suggestion`: "Vá até a pasta principal").

### `favoritesCantAddHere`: um fato sobre ESTA pasta, com o motivo depois dos dois-pontos

`Esta pasta não pode ficar nos favoritos: favoritos só funcionam em discos e compartilhamentos montados` · high. Começa
com o mesmo sujeito da irmã `favoritesAlreadyAdded` (`Esta pasta já está nos favoritos`), então as duas linhas cinzas se
leem como um par, e reaproveita `estar nos favoritos`, que desvia da concordância de gênero (`ser um favorito` bateria
com `pasta`). ❌ `apontar para` saiu daqui: obrigava a reger um complemento para o destino e deixava a linha 45–90% mais
longa que o inglês; `funcionar em` é um locativo simples. Sem ponto final (é um tooltip) e sem `erro` nem `falha`.

### `favoritesAdd.description`: a seção do seletor não existe mais

`Adicione a pasta atual do painel em foco aos favoritos, para voltar até ela pelo menu de favoritos.` · high. O valor
anterior mandava a pasta para "os Favoritos do alternador", uma seção que o M3 removeu, então descrevia uma superfície
que já não existe. Imperativo de 2ª pessoa, como toda `commands.*.description`.

## Quem está segurando o disco: a recusa que NOMEIA (`errors.eject.unmountRefusedByApp`/`ByApps`/`otherApps`/`ByDiskImage`/`BySystem`/`ByCmdr`)

Seis chaves novas da mesma família das nove da § Recusas de ejetar e desconectar: o macOS recusou a ejeção e agora o
Cmdr sabe DIZER o que segura o disco. Entram no mesmo aviso rápido depois de dois pontos
(`fileExplorer.pane.ejectFailedToast` / `disconnectFailedToast`), família RAW (apóstrofo simples, `{app}` / `{apps}` são
alvos literais, nada de ICU), e a genérica `errors.eject.unmountRefused` é a irmã das duas primeiras: as três repetem o
mesmo molde `<sujeito> ainda está usando este disco. <ação>, depois ejete-o de novo.`

- **`{app}` entra SEM artigo e abre a frase** · `{app} ainda está usando este disco.` O nome chega do disco em tempo de
  execução (`Preview`, `Warp`, `mds_stores`), então nenhum artigo pode prejulgá-lo: o Finder pt-BR só escreve
  `O aplicativo “^0”` porque ele mesmo põe as aspas, e o inglês desta chave dispensa as duas coisas · high. Um nome
  próprio como sujeito é natural em pt-BR (`Safari não respondeu`), e pôr o nome em primeiro lugar é justamente o ponto
  da chave. ❌ Nunca `O app {app}` (quebra com `mds_stores`, que é comando, não app) nem aspas em volta.
- **`it` / `they` viram `ele` / `eles`, e isso é seguro** · `Feche o que ele tiver aberto lá` /
  `Feche o que eles tiverem aberto lá`. A regra "nada concorda com um `{name}`" do `style.md` vale para arquivo/pasta,
  de gênero desconhecido; aqui o referente é sempre um APP, e app é masculino em pt-BR (`o Fotos`, `o Preview`),
  qualquer que seja a forma do nome · high. O pronome também é o que separa a chave de um app da de vários, como no
  inglês. O futuro do subjuntivo composto (`tiver aberto` / `tiverem aberto`) traduz o "has open" como estado, não como
  ação passada.
- **"there" (no disco) → `lá`** · o catálogo já publica `lá` para "there" em uma dúzia de chaves
  (`fileOperations.cancelRollback.reason.drift.named` "depois que o Cmdr colocou lá",
  `errors.write.newDataKeptAt.message` "já não está lá") e nenhuma `aí` · confirmed. Evita repetir `disco` três vezes em
  duas frases curtas.
- **other apps → `outros apps`**, masculino plural e SEM artigo · o catálogo fechou `app` no lugar de `aplicativo` (§
  "Abrir terminal aqui" e o seletor de app, `Escolher app…`) e já publica exatamente `outros apps` em seis chaves
  (`main.revealNudge.turnedOn`, `settings.network.directSmbConnection.description`,
  `settings.advanced.showSafeSaveFiles.label`, `settings.revealHandler.label`, `viewer.copyDialog.confirmBody`,
  `errors.write.deletePending.suggestion`) · confirmed. É o ÚLTIMO item de uma lista que o `Intl.ListFormat` do locale
  junta (`Preview, Warp, Photos e outros apps`), então ele não leva artigo nem ponto, e o `e` NUNCA entra na string. O
  masculino plural também é o que faz `eles` funcionar na frase seguinte. ⚠️ A irmã `errors.eject.unmountRefused` ainda
  diz `aplicativos abertos` (decisão de 2026-08-23, do Finder): deriva a reconciliar numa varredura, fora do escopo
  deste lote.
- **disk image → `imagem de disco`** · macOS pt-BR Finder (`InfoWindowGeneralView.json` `tvy-hx-Gou.title`:
  `Imagem de disco:`; `LocalizableMerged` `Volume de Imagem de Disco`), e o catálogo já publica a forma em caixa baixa
  em quatro chaves (`errors.listing.readOnly.explanation`, `.readOnlyVolumeErrno.*`,
  `updates.moveToApplicationsDialog.readOnlyVolume`) · confirmed.
- **"is still open" (de uma imagem de disco) → `ainda está montada`, não `ainda está aberta`** · `montar` é o verbo da
  Apple para uma imagem que virou volume, e o catálogo já o usa para esta mesma coisa
  (`errors.listing.readOnly.explanation`: "a imagem de disco foi montada como somente leitura") · high. `aberta` puxaria
  para "feche o arquivo", e a ação certa é EJETAR: `montada` é o que torna a frase acionável.
- **"stored on this drive" → `armazenada neste disco`** · `armazenar` é o verbo do catálogo para onde um dado fica
  (`errors.listing.notSupportedErrno.suggestion` "não armazena arquivos maiores que 4 GB") · high. ❌ Nunca `guardada`,
  indício pt-PT que o `style.md` lista.
- **`macOS` leva artigo e é sujeito: `O macOS`** · o catálogo já abre frase assim
  (`errors.listing.notPermitted.explanation` "O macOS impediu o Cmdr…", "O macOS controla quais apps…") e o Finder pt-BR
  usa `pelo macOS` (`LA10` "está sendo usado pelo macOS") · confirmed. A marca fica intacta; só o artigo entra.
- **"is working with this drive" → `está trabalhando com este disco`** · colocação já publicada em
  `errors.listing.notSupportedErrno.suggestion` ("Se estiver trabalhando com um disco externo") · high. Deliberadamente
  DIFERENTE do `está usando` das outras cinco: aqui não há nada para fechar, e a ação é esperar. ❌ Não
  `está ocupado com` (`ocupado` está reservado ao sufixo de item de menu, § busy).
- **`Wait a minute` → `Espere um minuto`; `Wait a moment` → `Espere um momento`** · `Espere um momento` já está
  publicado três vezes (`errors.listing.resourceBusy.suggestion`, `errors.listing.deletePending.suggestion`,
  `errors.write.deletePending.suggestion`) · confirmed. O inglês distingue as duas durações de propósito, e o português
  mantém a distinção.
- **`Cmdr itself` → `O próprio Cmdr`** · o `próprio` assume a culpa sem a palavra `falha`, que a voz do Cmdr não usa ·
  high. **`send a report` → `envie um relatório`**, sem `de problema`: o inglês também encurtou de propósito (ver a
  `@key.description` de `settings.updates.crashReports.description`), e o comando completo (`menu.help.sendErrorReport`,
  `Enviar relatório de problema…`) é que nomeia a superfície.
- **`if it keeps happening` → `se isso continuar acontecendo`** · forma já publicada em sete chaves de `errors.json` ·
  confirmed. O `isso` explícito (e não o `se continuar acontecendo` de `errors.serverRequest.unexpected`) porque a
  oração vem logo depois de `ejete-o de novo`, e sem sujeito ela se leria como "se a ejeção continuar acontecendo".

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Tipo`** · macOS Finder `ArrangeByMenu`
  `119.title`/`338.title`, the Kind sort criterion. Every source in this section comes from the reference pile's
  `pt-BR/` folder, per the `_see-also.txt` ruling that `pt` ships Brazilian · `high`.
- **"Select all with extension `*.{extension}`" → `Selecionar tudo com a extensão *.{extension}`** · Double Commander
  (`tfrmmain.actmarkcurrentextension.caption`, "Select All with the Same Extension" →
  `Selecionar todos com a mesma extensão.`) and Total Commander (`WCMD.INC` `527` →
  `Selecionar todos os arquivos com a mesma ext.`) name this exact command, and the mask replaces their "same extension"
  because Cmdr shows the concrete one · `high`. A máscara vem depois de `com a extensão`, então nada concorda com
  `{extension}`.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Seleção`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` («adicionar … à seleção») · `high`. Its
  siblings in that menu are verbs; this one names what the submenu holds. ❌ Not the verb `Selecionar`, which is
  `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`)

A pílula de aviso na barra de título e o seu tooltip, exibidos enquanto o Cmdr não tem acesso total ao disco; um clique
reabre a introdução no passo 1.

- **`onboarding.fdaBadge.label` → `Sem acesso total ao disco`** · Apple's own Ajustes do Sistema row, confirmed against
  the live macOS bundle
  (`/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable`, key
  `ALL_FILES` = "Acesso Total ao Disco" in pt-BR, macOS 27.0 build 26A428, verified 2026-09-16) · high. **Capitalization
  is per-surface**: Apple title-cases the pane row, but the badge's English is lowercase ("No full disk access"), so the
  badge keeps sentence case. `Acesso Total ao Disco` stays for strings that NAME the pane.
- **`onboarding.stepAi.bannerTitle.denied` already carried this exact string**, and `i18n-terms` holds the two identical
  because they share one English source. ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Sem acesso total ao disco. Abrir a etapa de acesso total ao disco da introdução.`), which is what satisfies
  `i18n-aria` (WCAG 2.5.3). ❌ Re-wording the label alone breaks it.
- **Tooltip terms**: drive/disk → `disco` (settled), cloud folders → `pastas na nuvem`, "files macOS keeps to itself" →
  `arquivos que o macOS guarda para si` (plain, ❌ never a macOS feature name), "Click to …" → `Clique para …` (as in
  `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `introdução` (settled).

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Portuguese solves
  it with `{count} dos itens que você escolheu`, which takes any numeral without agreement.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Detalhes técnicos`), because it points at that very control.

- **locked → `bloqueado`** · macOS Finder pt-BR (`AXNODE1` `Bloqueado`) and the settled catalog term · high.
- **"delete them permanently" → `apagar permanentemente`** · matches `commands.fileDeletePermanently.label`, so the
  suggestion names the command the user will run · high.
- **badge (the title-bar pill) → `o selo`** · reuses the settled `badge → selo` (MS terminology pt-BR) · high. title bar
  → `barra de título` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in the catalog's `“…”` quotes. Note the badge label is
  sentence-cased (`Sem acesso total ao disco`) while the pane NAME stays `Acesso Total ao Disco`; the quote follows the
  badge, since that is what the user reads on screen.
- "somewhere macOS keeps to itself" → `um lugar que o macOS guarda para si`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## O aviso de conteúdo somente online (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Se um item selecionado numa pasta de nuvem está somente online, o Lixo teria que baixá-lo primeiro. Por isso o Cmdr abre
a janela de apagar em definitivo e explica isso no aviso. Duas variantes do aviso: uma para uma seleção mista e outra
para quando tudo está somente online. Elas só diferem na primeira frase e nas saídas que conseguem oferecer. A terceira
chave é a linha que aparece quando o Cmdr devolve um clique.

- **`.cloudOnlineOnlyMixedWarning`** · `somente online` é a fórmula do Finder para um arquivo despejado; `Lixo` e
  `serviço de nuvem` vêm de `terms.json` · medium.
- **`.cloudOnlineOnlyAllWarning`** · mesmo texto, com “Tudo o que você selecionou” no lugar de “Parte da sua seleção”, e
  sem a saída de desmarcar: se tudo está somente online, não sobraria nada selecionado · medium.
- **`.cloudOnlineOnlyHandedBack`** · a linha acima do botão depois de um clique que o Cmdr não executou de propósito.
  Tom direto, sem pedir desculpa · medium.
- **Os quatro fatos são obrigatórios**: (1) o Lixo baixaria os arquivos, (2) por isso o Cmdr só oferece apagar a seleção
  INTEIRA, (3) depois NÃO fica cópia no Lixo, mas o serviço guarda a dele (❌ não suavizar), (4) as saídas que o aviso
  nomeia.
- **Os dois trechos `<strong>` ficam**, em “download deles primeiro” e no verbo “apagar”. E “Apagar” entre aspas é o
  rótulo do botão: sempre igual a `fileOperations.delete.confirmDelete`.

## Quando o servidor diz que aquele compartilhamento não existe (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

O único caso desta família em que tentar de novo não adianta: o servidor responde com clareza que não tem nenhum
compartilhamento com aquele nome. Por isso este aviso não traz botão, e o tom não pode sugerir nada temporário (nada de
`agora` nem de `tente novamente`), ao contrário dos irmãos.

- **"the server says it has no share by that name" →
  `porque o servidor diz que não tem nenhum compartilhamento com esse nome`** · o catálogo
  (`errors.mount.shareNotFound`) e NetAuthAgent `EINFO_NO_SHARE` (pt_BR: "O compartilhamento “%@” não existe no
  servidor.", pacote VIVO, macOS 26.6.2, 25G83, 2026-09-17) · `high`. A abertura continua sendo
  `Não foi possível conectar diretamente a X`, de `fileExplorer.network.osMountFallback.message`.
- **"You are still connected" → `Você continua conectado`** · segue o `Você está conectado` já fixado no irmão, que a
  própria Apple pt-BR usa; masculino não marcado autorizado pelo `style.md` · `high`.
- **"This one won't sort itself out" → `Isso não vai se resolver sozinho`** · sem fonte na pilha; é a forma corrente em
  pt-BR e diz exatamente o que separa este aviso dos outros: esperar não muda nada · `high`.
- **"may have been renamed or removed" → `pode ter sido renomeado ou removido`** · literal de
  `errors.write.destinationNotFound.suggestion` · `high`.
- **"so it's worth checking there" → `então vale a pena dar uma olhada por lá`** · registro caloroso do `style.md`;
  `por lá` evita repetir `servidor` · `high`.
- **"a lot slower" → `bem mais lenta`** · aqui o inglês não dá multiplicador, diferente do irmão com `4x` · `high`.
- O aviso curto fecha com `então ele continua na conexão do sistema`, o mesmo fecho dos três irmãos
  (`fileExplorer.pane.directConnectionUnreachableToast`…).

## Nomes que parecem iguais no servidor (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.explanation`, `errors.volume.ambiguousName`)

Dois nomes idênticos na tela que o servidor armazena com caracteres diferentes (`é` composto vs `e` + acento, ou só a
caixa das letras).

- **"look the same" → `parecem iguais`** · a forma corrente em pt-BR, já publicada em
  `errors.listing.ambiguousName.explanation` · `high`.
- **"spells them differently" → `os escreve de forma diferente`**; "stores them spelled differently" →
  `os armazena escritos de forma diferente` · sem fonte na pilha (nem Finder nem MS falam desse caso); palavras comuns,
  sem `Unicode` nem `normalização`, como a `@key` pede · `high`.
- **store (um nome, no servidor) → `armazenar`, nunca `guardar`** · `guardar` é marcador pt-PT (§ Variant do `style.md`)
  e o catálogo já diz `O destino não consegue armazenar esse nome` (`errors.volume.invalidName`) · `high`.
- **case-sensitive → `diferenciam maiúsculas de minúsculas`** · a forma já fixada em § As derivas corrigidas. O Finder
  pt-BR diz `não faz distinção entre letras maiúsculas e minúsculas` (`LocalizableMerged.json`), que também serviria,
  mas trocar abriria uma costura com a busca · `high`.
- **"the folder above" (a pasta que contém o caminho) → `a pasta superior`** · o termo de navegação fixado em § parent
  folder; `pasta acima` lia como "a pasta listada acima" · `high`.
- **"Choose it from its folder" → `Escolha o item na pasta onde ele está`** · nomeia o item em vez do enclítico `-o` (o
  antecedente fica longe, depois de `{path}`) e evita o calque `a partir da pasta` · `high`.
- **O botão citado em texto corrido vai entre aspas curvas**: `“Substituir” troca o item que já está lá`, no molde de
  `fileOperations.delete.cloudOnlineOnlyHandedBack` (`oferece “Apagar”`). O rótulo copia
  `fileOperations.transferProgress.conflictOverwrite` byte a byte · `high`.

## O interruptor “Permitir IA na nuvem” e os estados de nuvem desligada (`ai.cloudConsent.*`, `askCmdr.gate.*`)

Um interruptor de consentimento de privacidade: desligado por padrão, e nada sai do Mac até ele ser ligado. O texto tem
de ser calmo e nunca prometer mais do que o Cmdr faz. Fontes vêm do macOS instalado (a pilha não está no M1), o caminho
de `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

- Allow cloud AI (o rótulo do interruptor, `ai.cloudConsent.label`) · **Permitir IA na nuvem** · `Allow` → `Permitir` é
  o botão dos pedidos de permissão do macOS pt-BR (`TCC.framework` `Localizable.loctable`, `REQUEST_ACCESS_ALLOW`, macOS
  26.6.2 build 25G83, lido 2026-09-23); `IA na nuvem` é o valor já publicado de `settings.ai.provider.opt.cloud`, a
  opção que a pessoa escolheu logo acima · `high`
- cloud AI (substantivo, feminino) · **a IA na nuvem**; estado desligado · **A IA na nuvem está desativada**, no molde
  de `ai.translateError.off.title` (`A IA está desativada`) · `high`
- **As frases que citam o interruptor põem o rótulo entre aspas curvas depois de `Ative`:**
  `Ative “Permitir IA na nuvem” …` (`settings.ai.cloudConsent.lockedHint`, `askCmdr.gate.cloudOff.body`,
  `settings.askCmdr.cloudOffHint`). O imperativo `Permita a IA na nuvem` não reproduziria o rótulo, e a `@key` pede o
  rótulo igual. Onde o inglês diz só `Allow it`, fica o pronome: `Permita-a em Ajustes > IA` (enclítico feminino,
  `a IA`).
- Settings > AI · **Ajustes > IA**, com `>` como nos irmãos de `ai.translateError.*` e como a `@key` pede.
- AI service / cloud AI service · **serviço de IA** / **serviço de IA na nuvem**. O inglês distingue `service` do
  `provider` das chaves `ai.cloudConsent.askCmdr.*` (que seguem com `provedor`); o português acompanha, e as duas
  famílias convivem sem choque.
- custom endpoints · **pontos de extremidade personalizados** · termo já fixado em § Os passos de configuração de
  provedor de IA · `high`
- side panel · **painel lateral** · `high`
- Nomes das funções no dobrável (dentro de `<b>`): `Sugestões de nome para novas pastas`, `Busca em linguagem natural`
  (o termo de `queryUi.bar.aria.ai`), `Seleção por descrição` · `high`
- `settings.askCmdr.enabled.label` = `Ask Cmdr`, idêntico ao inglês com `sameAsSourceJustification` (nome do produto,
  como `settings.section.askCmdr`).

## Esc e tela cheia (`main.escapeFullScreenHint.*` + `settings.advanced.exitFullScreenOnEscape*`)

- Escape (a tecla) · **Esc** · macOS pt-BR, AppKit `FunctionKeyNames.loctable` (`Escape` → `Esc`, lido no macOS 27.0
  build 26A428, 2026-09-23); o catálogo já tinha `ESC` em `shortcuts.section.pressEscToClear`, mas o nome da Apple em
  caixa normal é `Esc`. Na frase de resultado, `A tecla Esc` dá sujeito à oração · confirmed
- full screen · **tela cheia** · macOS pt-BR Finder (`FV20` `Sair da Tela Cheia`, `FV21` `Entrar em Tela Cheia`), em
  sentence case; a Microsoft oscila entre `tela cheia` e `tela inteira`, e o Finder decide · confirmed
- Exit full screen on Escape · **Sair da tela cheia com Esc** · o verbo e o objeto vêm do item de menu da Apple
  (`Sair da Tela Cheia`); o rótulo do aviso (`switchLabel`) e o de Ajustes são byte a byte iguais · high
- Settings > Advanced · **Ajustes > Avançado** · nomes já fixados acima (§ seções de Ajustes) · high
- You''ll only see this once · **Este aviso só aparece uma vez** · o aviso vira o sujeito, o que dispensa o `você` e o
  futuro; `aviso` é a palavra que o catálogo já usa para notificações curtas · high

## Originais que mudaram durante a movimentação (`transfer.changedDuringMove`, 2026-09-25)

- **"changed during the move" → `mudou/mudaram durante a movimentação`** · o verbo de
  `fileOperations.cancelRollback.reason.drift.counted` (`mudaram`); o Finder `PE56` diz "foram alterados durante a
  gravação" (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25), mas o catálogo já fixou `mudar`
  para essa ideia · `high`. `durante a movimentação`, `continua/continuam` e `pastas de origem` vêm do irmão
  `transfer.appearedDuringMove`, que aparece no mesmo aviso. A oração inteira fica dentro dos ramos (style.md §
  Plurals).

## Linhas de espera em "Abrir com" e "Compartilhar" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Linhas de espera em "Abrir com" e "Compartilhar" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- Finding apps… · **Buscando apps…** · gerúndio pt-BR, como `settings.behavior.textEditorApp.checking`
  (`Verificando seus apps…`); `buscar` = search no termbase · high
- share options · **opções de compartilhamento** · substantivo do verbo do submenu `Compartilhar`; o mesmo
  `compartilhamento` do termbase · high
- No share options · **Nenhuma opção de compartilhamento** · afirmação calma, sem "erro" · high

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
