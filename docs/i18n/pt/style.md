# Portuguese (pt) translation style guide

Working notes for translating Cmdr into Portuguese. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Portuguese. Term rulings live in
`terms.json` (keyed by the shared `../concepts.json`, plus this locale's `concepts-proposed.json`), their rationale in
`decisions.md`, and open questions in `review-queue.md`.

The base `pt` tag is a decision in itself: Portuguese splits hard between Brazil (`pt-BR`) and Europe/Portugal
(`pt-PT`). Cmdr's `pt` ships Brazilian; see the variant decision below.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Variant: Brazilian (pt-BR), always.** Mine `_ignored/i18n/pt-BR/`; the bare `_ignored/i18n/pt/` pile is EUROPEAN and
  has leaked pt-PT into shipped strings before. Before shipping a batch, grep for the pt-PT tells: `ficheiro`,
  `estar a` + infinitive (`A indexar`), a proclitic pronoun before an infinitive (`para a preparar`), `consoante`,
  `Rever`, `alterar o nome`, `guardar` (for save or store), `só de leitura`, `ecrã`, `telemóvel`, `partilha`,
  `Transferências` (the pt-PT Downloads folder), and any `tu` form (`Podes`, `para ti`).
- **Address**: `você`, never `tu`. UI actions use the implied-subject form, so most strings need no pronoun. Keep an
  explicit `você` where the verb alone is ambiguous, and give a result sentence an explicit subject: `O Cmdr apagou…`,
  `A reversão parou…`, never a bare `Apagou…` (it also reads as "você apagou").
- **Register by UI slot**:
  - buttons, menu items, and `commands.*.label`: the infinitive (`Copiar`, `Mostrar favoritos`, `Apagar modelo`);
  - `commands.*.description` and help text follow the English mood: an English imperative takes the `você` imperative
    (`Abra o menu de favoritos…`), an English third person takes the third person (`Seleciona ou desmarca…`);
  - yes/no dialog titles: an infinitive question (`Reverter esta operação?`, `Apagar modelo de IA?`);
  - progress: the gerund (`Copiando`, `Indexando`, `Reconectando a {name}…`), never `A copiar`;
  - a half-line summary beside a switch has the SWITCH as its implied subject (`Permite conectar…`, `Ocupa 1 GB…`) and
    must not wrap.
- **Voice**: friendly, concise, calm. Never label an outcome with `erro`, `falha`, or `falhou`: a title says
  `Não foi possível …`, a body says `O Cmdr não conseguiu …`, the unknown case says `Algo deu errado`. `timedOut`-style
  lines say the thing may still finish (`ainda pode ser concluída`). No apologies for a deliberate choice.
- **Capitalization**: sentence case everywhere, even where Apple's pt-BR title-cases (`Nova pasta…`, not `Nova Pasta`):
  only the term comes from Finder. A name Apple gives a pane or window keeps Apple's capitals when a string NAMES it
  (`Obter Informações`, `Acesso Total ao Disco`, `Privacidade e Segurança`).
- **Typography** (`mechanics.json`): curly `“…”` outside, `‘…’` nested, `’` for the rare apostrophe, `…` for an
  ellipsis; never straight quotes, never the pt-PT `«»`. Quote a UI label in running text
  (`Ative “Permitir IA na nuvem”`) and a `{name}` wherever English quotes it. No space before `%` `!` `?` `:` `;`.
  Multipliers sit tight: `4x mais lenta do que …`. Sentence-final `por quê` takes the circumflex.
- **No hedged grammar**: never `arquivo(s)`, `selecionado(a)`, `conectado/a`, `o(a)`; use ICU `plural` / `select` when
  Cmdr knows the value, else rephrase so nothing agrees (the noun first: `a pasta {name}`; a verb; `tudo`). Never guess
  a file or pane name's gender with a contracted article (`na {name}`, `nos {system_settings}`): `em {system_settings}`.
- **Brand and Apple names**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, `Safari` stay verbatim; `Cmdr` as a subject takes
  the article (`O Cmdr`), and so does `Finder` (`no Finder`). `Dock`, `Finder`, `Mac`, `Spotlight` stay English;
  localize what Apple localizes, whatever a `@key` says: `Visualização rápida`, `pasta Aplicativos`, `Downloads` /
  `Documentos` / `Mesa`, `Editor de Texto`, `Pré-Visualização`, `Acesso às Chaves`, `Ajustes do Sistema`. On a phone,
  Android's own pt-BR wins (`depuração USB`, `Permitir`, `toque em`). `Ask Cmdr` names only the chat panel; prose about
  what the AI does says `O Cmdr` or `a IA`.
- **Plurals**: CLDR `one` / `many` / `other`, all three always written. Anything that agrees with the counted noun
  (participles, `Todas as`, `ficou`/`ficaram`) goes INSIDE the branches. `one` covers 0.
- **Placeholders and gender**: nothing agrees with an uncontrolled `{name}`, `{path}`, `{app}`, or `{volumeName}`: write
  the noun (`a pasta {name}`, `no disco {volumeName}`) or use a verb. `{app}` leads without an article. Avoid gendered
  adjectives about the user (`por conta própria`, `Não precisa` for "No, thanks"); the unmarked masculine only where
  Apple itself uses it (`Você está conectado`).
- **Clitics**: enclisis (`Ative-a`, `Abra-o`, `desafixá-lo`), never proclisis, and only when the gender closes on its
  own; otherwise repeat the noun (`abra o servidor de novo`).
- **Top traps** (details in `terms.json`):
  - delete → `Apagar` / `apagamento`, for files AND for a model or index entries; never `Excluir` / `exclusão`. trash →
    `Lixo`, never `Lixeira`; Move to trash → `Mover para o Lixo`.
  - drive → `disco` in every sense (network drive → `disco de rede`); never `unidade` or `drive`. file → `arquivo`;
    archive → `arquivo compactado`.
  - operation → `operação`; the queue → `Fila de operações`, beside `Registro de operações`; `transferência` only for
    one copy or move in flight.
  - rename → `Renomear` / `renomeação`, never `alterar o nome`. dismiss → `Dispensar` (`Ignorar` is Skip).
  - Settings → `Ajustes`; sign in → `iniciar a sessão`; quit → `Encerrar`; save → `Salvar`; turn on/off → `Ativar` /
    `Desativar` (`ligar` / `desligar` is physical power).
  - search → the `busca` / `buscar` family, never `pesquisa` for the feature. scan: the drive index's `varredura` vs the
    pre-copy count's `Analisar`. check: `verificar` when Cmdr checks, `conferir` when the person does.
  - put back has three verbs: `colocar de volta` (out of the Trash), `restaurar` (an old name), `levar de volta` (a
    rollback moving files back). retry: the button `Tentar novamente`, running text `tente de novo`.
  - phone → `celular`; badge and chip → `selo`; step → `etapa`; volume switcher → `seletor de volumes`.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. Portuguese tech UI (both variants) reads
warmer than German or Russian: contractions of preposition + article are everywhere (`no`, `na`, `do`, `da`), and that's
correct register, not slang. Error messages stay calm and actionable; avoid dramatic words. Apple's pt-BR Finder is the
closest reference for the tone Cmdr wants.

## Formality

Use the implied-subject imperative for UI actions, which sidesteps the tu/você split entirely. Portuguese verb
imperatives in UI almost never name the subject:

- "Copiar", "Mover", "Renomear", "Apagar" for buttons and menu items (infinitive-as-imperative, the macOS and Microsoft
  convention for both variants).
- When the UI must address the user in running text (onboarding, confirmations), use **você**, never **tu**. Both Apple
  and Microsoft use the você/implied register, never the tu-conjugated familiar form, in product UI.
- Avoid the explicit second-person pronoun where the verb alone carries the meaning ("Seus arquivos foram movidos" reads
  fine; prefer active "Movemos seus arquivos" where Cmdr's English uses active voice).

## Decision points

### Variant: pt-BR vs pt-PT vs a shared base

The single biggest Portuguese decision. The two variants differ in vocabulary, spelling, and some grammar, enough that a
single text reads "foreign" to one side. How the majors handle it:

- Apple ships **both** pt-BR (Brazil) and pt-PT (Portugal) as separate Finder/macOS locales; the reference pile has
  both.
- Microsoft ships **both** pt-BR and pt-PT terminology and full style guides.
- Google, Spotify, Netflix all offer **both** "Português (Brasil)" and "Português (Portugal)" as distinct UI locales.
- The industry norm is therefore two locales, not one blended Portuguese.

Concrete vocabulary splits that matter for a file manager:

- "file": pt-BR **arquivo** vs pt-PT **ficheiro**. This is the highest-frequency divergence in the whole catalog and
  alone makes a shared text wrong for one side.
- "delete / trash": generic pt-BR leans **Excluir** + **Lixeira**; pt-PT leans **Eliminar** / **Apagar** + **Lixo** /
  **Reciclagem**. Cmdr follows the macOS pt-BR Finder instead (§ "Apagar" vs "Excluir" below).
- "folder": both use **pasta** (shared, safe).
- "screen / monitor", "mouse", "username", and many UI nouns also diverge.

Spelling: the 1990 Orthographic Agreement narrowed but did not erase the gap (e.g. accentuation and some consonant
clusters still differ in practice and in vendor style guides).

**Settled: `pt` ships BRAZILIAN Portuguese (pt-BR).** Recorded in `../language-selection-decisions.md` (pt = wave 1,
ships as pt-BR; pt-PT = wave 2) and in the reference pile's own `_ignored/i18n/pt/_see-also.txt`. Never ship one blended
"Portuguese", and never mine the bare `_ignored/i18n/pt/` folder: that one is EUROPEAN, and using it is the documented
variant trap (`decisions.md` § The bare `pt` pile is European). Mine `_ignored/i18n/pt-BR/`.

**pt-PT tells worth grepping for before you ship a batch** (each one is a real regression found in a shipped batch):

- `ficheiro`/`ficheiros` → pt-BR **arquivo(s)**.
- The `estar a` + infinitive progressive (`está a indexar`, `A indexar`) → pt-BR **gerund** (`está indexando`,
  `Indexando`).
- `consoante` (= "according to") → **conforme** / **pelo quanto**.
- Proclitic object pronouns before an infinitive (`para a preparar`) → pt-BR enclisis on the infinitive
  (`para prepará-la`).
- `Rever` → **Revisar**. `alterar o nome` → **renomear**.
- A dropped `você` where the verb form alone is ambiguous (`Apagou esta pasta…` → `Você apagou esta pasta…`).
- A `tu` verb form (`Escolhes…?`, `Podes…`), `para ti`, `guardar`, or `só de leitura` → pt-BR **você**, **salvar** /
  **armazenar**, and **somente leitura**, and for a question toast the catalog's infinitive
  (`Escolher outra pasta para salvar?`, like `Tentar de novo?`), which needs no pronoun at all.
- `ecrã` → **tela**; `telemóvel` → **celular**; `partilha` → **compartilhamento**; the folder `Transferências` →
  **Downloads**.

### Spelling reform compliance

Within whichever variant you pick, follow the post-1990 Orthographic Agreement spelling (e.g. drop the silent consonants
the reform removed). Apple and Microsoft both ship reform-compliant strings. Recommendation: post-reform spelling
throughout. Confidence: high.

### Gender and inclusive language

Portuguese is grammatically gendered (o/a, -o/-a adjective agreement). Cmdr's UI rarely addresses the user with a
gendered adjective, so this is mostly avoidable by structuring around nouns and infinitives. Where an adjective would
agree with the user ("tem certeza?"), Portuguese conventionally uses the masculine as the unmarked default; the
"x"/"@"/"e" neutral forms (e.g. "todes") are activist register, NOT used by Apple, Microsoft, Google, Spotify, or
Netflix in product UI. Recommendation: avoid gendered user-adjectives by rephrasing; where unavoidable, use the
conventional unmarked masculine, matching every major. Don't use "x"/"@"/"-e" neutral morphology. Confidence: high.

### "Apagar" vs "Excluir" vs "Eliminar" for delete/trash

A file manager hits delete constantly, and the verb choice is variant-coded AND semantically loaded (permanent delete vs
move-to-trash), so it's the most likely consistency bug. **Locked in `terms.json`** (pt-BR): delete → **Apagar**, delete
permanently → **Apagar permanentemente**, the trash action → **Mover para o Lixo** (trash noun = **Lixo**, the macOS
Finder Tier-1 value). Cmdr is a macOS app, so Finder's own "Apagar" beats the Windows-influenced "Excluir" (term-choice
principle 2), for files and for a local AI model or index entries alike. "excluir" survives only in its exclude sense
(the query-scope exclude, excluded folders). See `decisions.md` § Apagar, never Excluir.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` and this locale's
`concepts-proposed.json`: `chosen`, accepted forms, usage notes, forms to avoid with the reason, a confidence
(`confirmed` / `high` / `tentative`), and sources. Tier order is macOS pt-BR (Tier 1) → Microsoft pt-BR terminology
(Tier 2) → the file-manager catalogs (Tier 3); a vendor's own pt-BR UI (Apple, Android) beats a `@key` description.
Rationale worth more than a line sits in `decisions.md` under a heading that cites its keys, and the term's `decision`
field names that heading. Never guess a term: mine `_ignored/i18n/pt-BR/` first (`../reference-pile/how-to-mine.md`), or
the installed macOS bundles when the pile isn't on the machine.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Safari. Enforced by `desktop-i18n-dont-translate`.
Apple feature names Apple translates are NOT on this list: Quick Look is `Visualização rápida`.

## Plurals

CLDR categories for `pt`: `one`, `many`, `other` (same for pt-BR and pt-PT). Note `many` is a real, distinct category in
modern CLDR Portuguese (compact/large numbers), so plural messages must write a `many` branch, not just one/other.

Two mechanics that bite in Portuguese specifically:

- **Anything that agrees with the counted noun goes INSIDE the branches**, never in the text after the plural. Number
  and gender agreement reaches across the whole clause (aberto/abertos, pode/podem, gravado/gravados, Todas as…), so an
  English tail like "… and may already be partly written" has to be duplicated into each branch, leaving only
  punctuation outside. See `transferProgress.stallInFlight` and the image-indexing plurals in `decisions.md`.
- **`one` covers zero in Portuguese** (CLDR `i = 0..1`), so a `one` branch renders "0 arquivo". That's the correct CLDR
  form; write the branch so it reads sanely at 0, or make sure the string only renders for counts ≥ 1.
- **A `*Text` count with NO integer partner has no branch to agree in.** A bare formatted number can't choose
  `ficou`/`ficaram`, and Portuguese agrees. The fallback is a verb that agrees with something else and is therefore
  invariant in the count: first-person active (`deixamos {skippedText} no Lixo`), which the Formality section already
  prefers anyway. Ask for an integer partner instead where you can: `fileOperations.trash.undonePartial` gained a
  `{skipped}` driver for exactly this reason, so its second half now agrees normally
  (`{skipped, plural, one {{skippedText} item ficou} many {{skippedText} itens ficaram} other {…}} no Lixo`).

## Notes and decisions

- **Os menus nativos seguem o texto do Finder, mas NÃO a capitalização dele.** O Finder brasileiro usa Title Case („Nova
  Pasta”); o Cmdr fica em sentence case („Nova pasta…”), como o resto do catálogo e o `docs/style-guide.md`. Só o termo
  vem do Finder. Evidência e exceções: `decisions.md` § Native menus.
- Roster: Cmdr ships pt-BR for wave 1; pt-PT is a separate wave-2 variant (vocabulary, você/tu, spelling). See
  `../language-selection-decisions.md`.
- **Multiplicadores de velocidade** ("4x slower") ficam colados ao numeral, com `x` minúsculo e sem espaço nem `×`:
  `4x mais lenta`, `(às vezes 100x)`. O comparativo usa `do que`, não `que`. Evidência e a chave onde isso aparece:
  `decisions.md` § Mount and share-list failures.
- Decimal/thousands: both use comma decimal, period (pt-PT) or period/space thousands. `Intl` handles this; don't
  hardcode.
- **network drive fica `disco de rede`, nunca "drive de rede".** O termo do disco é **disco** (Finder) em todos os
  sentidos, e o catálogo inteiro já está alinhado: as chaves de rede, os `drive externo / interno / virtual` do
  `errors.json`, e `fileExplorer.unreachable.detailTimeout`. Sobram só marcas (iCloud Drive), o placeholder `{drive}` e
  os nomes de chave `driveIndex.*`. Evidência: `decisions.md` § `drive` is always `disco`.
- **Os avisos de ejetar/desconectar entram depois de dois pontos** (`fileExplorer.pane.ejectFailedToast` /
  `disconnectFailedToast`), então cada valor é uma oração completa, começa com maiúscula e cabe em uma ou duas frases
  curtas. `timedOut` não pode soar como falha. Evidência: `decisions.md` § Eject and disconnect refusals.
- **As recusas que NOMEIAM quem segura o disco repetem o molde da genérica.** `unmountRefusedByApp` /`ByApps` e
  `unmountRefused` são uma família só (`<sujeito> ainda está usando este disco. <ação>, depois ejete-o de novo.`);
  `{app}` abre a frase sem artigo e sem aspas, `outros apps` é item final de lista (o `e` vem do `Intl.ListFormat`,
  nunca da string), e o `BySystem` troca o verbo de propósito (`trabalhando com`), porque lá não há nada para fechar.
  Evidência: `decisions.md` § Eject and disconnect refusals.
- **Um relatório já enviado recebe uma NOTA, nunca um segundo envio.** As chaves de `errorReporter.amend.*` falam do
  mesmo relatório (`e isso entra no mesmo relatório que a equipe já tem`), a caixa continua sendo uma **nota** (o termo
  do diálogo de envio, para as duas telas não terem costura) e o encaminhamento quando não dá mais para acrescentar é
  sempre **pelo menu Ajuda**, a frase que `settings.updates.errorReports.description` já publica. Evidência:
  `decisions.md` § Report notes.
- **O diálogo de falha tem três aberturas, e duas delas não podem falar em falha.** `crashReporter.dialog.body.ended`
  fala do app que encerrou; `keptRunning` e `unknown` descrevem um problema que o Cmdr atravessou (ou pode ter
  atravessado), então nelas não entram `falha`, `encerrou`, `fechou`, `parou` nem `travou`, e o relatório fica
  **relatório** sem o `de falha`. Evidência e os termos recusados: `decisions.md` § Crash dialog.
- **"Put back" tem TRÊS verbos em `pt`, e cada um é de uma família.** `colocar de volta` tira do Lixo, `restaurar`
  devolve o NOME anterior, e `levar de volta` é a reversão levando o arquivo ao lugar de origem. O inglês usa um verbo
  só e as `@key` chegam a mandar unificar; em português não dá. Evidência e as chaves de cada família: `decisions.md` §
  Rollback outcome notice.
- **Uma frase de resultado nunca fica sem sujeito.** `Apagou {countText} itens…` também se lê como `você apagou`, e a §
  Variant acima já lista o `você` omitido como indício pt-PT. As manchetes de aviso escrevem `O Cmdr` ou `A reversão`
  por extenso, ainda que o inglês elida o sujeito; as linhas de motivo escapam disso pondo o ITEM como sujeito
  (`{name} ficou como está: …`), o molde que `askCmdr.renameUndo.skipReason.*` já publica. Evidência: `decisions.md` §
  Rollback outcome notice.
- **Nada concorda com um `{name}`**: ele pode ser arquivo ou pasta, então nenhum particípio, adjetivo ou possessivo pode
  se apoiar nele; só verbos e preposições sem artigo. Quando a linha precisa do gênero, ela escreve o substantivo
  (`a pasta {name}`). Mesma lógica dos tokens de painel do macOS (`decisions.md` § macOS pane names, Apple menu items,
  and the example email).
- **`obrigado` num botão impõe um gênero ao usuário**, porque concorda com quem fala. `No, thanks` sai como
  **`Não precisa`**, uma recusa educada corriqueira e sem gênero; `Agora não` fica reservado ao `Not now`, que promete
  uma próxima vez. É o caso típico do "reestruture para o neutro" da § Gender acima: a saída neutra existe e soa
  natural, então ela ganha do masculino não marcado. Evidência e as formas recusadas: `decisions.md` § Dock pin nudge.
- **`Dock`, `Finder` e `Mac` ficam em inglês; `Applications` vira `pasta Aplicativos`.** O macOS pt-BR decide isso
  rótulo a rótulo, e a pilha fecha os três: `Adicionar ao Dock` mantém `Dock`, `Busca … no Finder` mantém `Finder`, e
  `Ir para a pasta Aplicativos` traduz a pasta. `configuration profile` é `perfil de configuração` (Apple) e `managed` é
  `gerenciado`. Evidência por chave: `decisions.md` § Dock pin nudge.
- **Android por ADB: `depuração USB` e `ferramentas de plataforma` traduzem; `adb`, `ADB`, `Android SDK` e `Homebrew`
  ficam.** As duas linhas de `settings.fileOperations.adb*` chamam o campo de `Localização do adb` (o `localização` do
  Finder), o alvo de `sistema de arquivos` (o termo do Utilitário de Disco) e o interruptor de
  `Acesso aos arquivos do Android por ADB`, no molde `por USB` que o catálogo já usa. Evidência e confiança:
  `decisions.md` § ADB settings.
- **O Ask Cmdr agora lê partes de um arquivo, e nenhuma frase pode prometer o contrário.** A tela de consentimento diz
  `arquivos inteiros, fotos ou miniaturas` (nunca enviados) e `uma parte limitada dele` (o que pode sair); o verbo de
  espiar é `olhar dentro de`, os metadados de foto são `detalhes da câmera` e o lugar é `localização` /
  `onde ela foi tirada`. Evidência: `decisions.md` § What Ask Cmdr reads inside files.
- **A forma "ocupada" de um item de menu é o item inteiro mais ` (ocupado)` no fim, sem mais nada.** O sufixo é
  invariável: fica no masculino singular porque descreve o servidor ou o disco, não o usuário nem o item, então serve
  igual para `Desconectar (ocupado)`, `Esquecer senha salva (ocupado)`, `Esquecer servidor (ocupado)` e
  `Ejetar ({name}) (ocupado)`, que é o molde original. O texto base copia byte a byte a chave irmã não ocupada
  (`menu.network.disconnect`, `menu.network.forgetSavedPassword`, `menu.network.forgetServer`, `menu.volume.eject`): as
  duas linhas se alternam no mesmo lugar do menu, e qualquer diferença de palavra lê como outro comando. Termo:
  `decisions.md` § Native menus.
- **`Servidores` e `Rede` convivem no seletor de volumes, e a diferença é o ponto.** A LINHA que abre o hub é
  `Servidores` (`fileExplorer.navigation.networkVolume`); o GRUPO onde ela fica continua `Rede`
  (`fileExplorer.navigation.groupNetwork`). A seção de atalhos dos lugares dentro de um servidor é `Locais`, o termo do
  Finder, e não mais `Navegador de compartilhamentos`, que só descrevia o SMB. Evidência: `decisions.md` § Server hub.
- **Um cabeçalho de coluna estreita não herda a forma longa nem a forma flexionada da Apple.** `Last used` sai como
  `Último uso`: a `Última Usada` do macOS trava no feminino e o sujeito é `o servidor`, e o `Usado pela última vez` do
  Mail (que é justamente um cabeçalho de tabela) tem quatro palavras. A forma nominal não concorda com nada e cabe.
  Evidência: `decisions.md` § Server hub.
- **Nenhum estado do hub pode soar como falha.** `Salvo` é o servidor parado e nada aconteceu de errado;
  `Sessão encerrada` concorda com a SESSÃO, não com a pessoa, e diz que só falta iniciar sessão de novo;
  `Aguardando você conferir a chave` põe a pessoa como quem age. O verbo de uma verificação FEITA PELA PESSOA é
  `conferir`; `verificar` fica para o que o Cmdr faz sozinho.
- **Uma manchete de progresso vai no gerúndio, e a irmã dela manda na preposição.** `Reconnecting to {name}…` sai como
  `Reconectando a {name}…`: o gerúndio é a forma pt-BR (o `A reconectar` do pt-PT é marcador de variante, § acima), o
  `Reconectando…` é literal da Apple, e o `a` vem da irmã `servers.paneState.connecting` (`Conectando a {name}…`), que
  se alterna com ela no MESMO lugar do painel. Evidência: `decisions.md` § Reconnect and key-only sign-in.
- **`então não há nada para …` é o molde fixo de "so there''s nothing to …".** Três chaves já o publicam (`ejetar`,
  `desconectar`, `digitar`), e o `desktop-i18n-term-consistency` compara pelo inglês, então uma quarta copia o molde em
  vez de reinventar a frase. O verbo de preencher uma credencial é `digitar`, o da Apple.
- **Um pronome enclítico só entra quando o gênero fecha sozinho.** `Abra-o de novo para tentar conectar.` é seguro
  porque `chave` e `senha` são femininas e `servidor` é o único masculino da frase; quando os candidatos empatam, a
  linha escreve o substantivo (`abra o servidor de novo`, `A chave do servidor mudou`). Ênclise sempre, nunca próclise.
- **`Ask Cmdr` só sobrevive onde o inglês o mantém, e ele agora só nomeia o PAINEL de chat.** Fora daí, o sujeito é
  `O Cmdr` ou `a IA`, conforme a chave em inglês; `AI features` é `os recursos de IA`. Nunca traduza pela memória de
  como a chave era antes: leia o inglês atual. Termos, fontes e as três palavras que não se confundem (`chat`,
  `conversar`, `conversa`): `decisions.md` § Ask Cmdr names only the chat panel.
- **O menu do ícone no Dock tem uma fonte Tier 1 que a pilha NÃO carrega**:
  `Dock.app/Contents/Resources/pt_BR.lproj/DockMenus.strings` (leia com `plutil -convert json -o -`). Ele decide a forma
  "verbo + nome do app" (`Abrir Cmdr`, sem artigo e sem aspas, seguindo `Ocultar %@` / `Mostrar %@`) e separa essa forma
  da de ARQUIVO, que a Apple põe entre aspas (`Abrir “%@”`). Atenção à pasta: `pt_BR.lproj` é o brasileiro. Evidência:
  `decisions.md` § Dock icon menu.
- **Um compositor `{a} ({b})` fica idêntico ao inglês em pt-BR, e há fonte para isso.** O `%@ (%@)` do AppKit sai
  inalterado no `pt` da Apple, embora a mesma chave seja adaptada em `ja`, `zh_CN`, `ar` e `he`: parênteses ASCII, um
  espaço antes, ordem núcleo → qualificador. Quando a chave só junta dois nomes vindos do disco, não invente `em` nem
  inverta a ordem; registre o `sameAsSourceJustification`. Evidência: `decisions.md` § Dock icon menu.
- **O botão e o contador de "star" do GitHub NÃO usam a mesma palavra em pt-BR, e está certo assim.** O botão é
  `Adicionar aos favoritos` (é como o próprio GitHub em português chama) e a contagem é `estrelas`. Uma linha de link
  nomeia o botão que a pessoa vai procurar; a nota abaixo nomeia o número que ela vai ver. Evidência e as duas fontes:
  `decisions.md` § Onboarding.
- **`step` é `etapa` na introdução inteira**, tanto a etapa do assistente quanto a etapa numerada de instrução. Nunca
  `passo`: o catálogo já fechou `etapa` e uma tela que alterna as duas palavras lê como dois conceitos.
- **Um resumo de meia linha ao lado de um interruptor tem o INTERRUPTOR como sujeito, e não pode quebrar linha.** Os
  quatro `onboarding.stepOptional.*.summary` são orações sem sujeito na mesma forma (`Precisa aceitar…`, `Ocupa 1 GB…`,
  `Permite conectar…`), o que dispensa o `você` que a § Variant cobra em frases de RESULTADO e também evita qualquer
  concordância de gênero com a pessoa. Como o espaço é de uma linha, cada valor ficou igual ou mais curto que o inglês:
  vale deixar implícito o que a legenda longa atrás do glifo já diz. Evidência: `decisions.md` § Onboarding.
- **`Por quê?` isolado leva circunflexo**; o `Por que` átono do meio da frase (`Por que este nome`) não. As duas formas
  convivem no catálogo e são as duas corretas.

## Open questions

Open questions for a native reviewer live in `review-queue.md`. (The `pt` = pt-BR question is settled; see the variant
section above.)
