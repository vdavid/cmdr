# pt review queue

Open questions for a future native Brazilian Portuguese reviewer. Not translator input: every item below already ships a
reasoned value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the
settled wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **`Aprovar` / `Recusar`** for the AI's suggested operations (`suggestedOps.*`): standard pt-BR, but the pile has no
  approve/reject pair. `Recusar` also sits near the rename review's `Negar` (a different gate).
- **`visualização resumida` / `visualização completa`** for Brief / Full view (`menu.view.briefView`,
  `menu.view.fullView`, `commands.viewBriefMode.label`), next to `Modo resumido` for "Brief mode" in Settings: Cmdr's
  own view names, no Apple source. Confirm, and whether `no modo Resumido` (capital R) should be `no modo resumido`.
- **`ferramentas de plataforma do Android`** (`adb.connect.adbNotInstalled`, `settings.adb.install.intro`): Google's
  term, no macOS or Microsoft source.
- **`lista de e-mails`** and **`servidor de inscrição`** (`onboarding.stepBeta.signup.*`): no direct source; Microsoft's
  `lista de distribuição` is the Exchange concept.
- **`Registro de alterações`** for the Help menu's Changelog (`menu.app.changelog`): chosen over Microsoft's
  `Log de alterações` to keep the catalog's `log → registro` family; confirm it reads naturally as a menu item.
- **`Cortar` (menu bar) vs `Recortar` (command palette)** for Cut (`menu.edit.cut`, `commands.editCut.*`): the menu bar
  copies Finder's `Cortar`; everywhere else says `Recortar`. Confirm the split, or unify.
- **`Status` kept English** (`licensing.section.labelStatus`, `servers.hub.colStatus`, `settings.adb.status.label`):
  macOS pt-BR says `Estado`. Confirm the naturalized loanword over Apple's word.
- **`pasta superior` over `pasta pai`** (`commands.navParent.label` and family): the pile favors `pasta pai`; the
  catalog keeps `pasta superior`. Switching is a whole-catalog migration.
- **`online` unhyphenated** (`commands.cloudRemoveDownload.description`, `fileOperations.delete.cloudOnlineOnly*`): the
  catalog writes `online`; the VOLP spelling is `on-line`.
- **Apple app names localized against the `@key`** (`settings.advanced.showSafeSaveFiles.description`:
  `o Editor de Texto e a Pré-Visualização`): the `@key` asks to keep `TextEdit` / `Preview`; macOS pt-BR names them this
  way.
- **Smaller tentative calls**: `comparador` (`queryUi.*.aria.comparator`), `manifesto`
  (`errorReporter.dialog.manifestHeading`), `transmissão` / `transmitindo` for the viewer's streaming mode
  (`viewer.statusBar.badge.streaming*`), `Acompanhar` for Tail (`viewer.toolbar.tail.*`), `Saída detalhada`
  (`settings.developer.verboseLogging.label`), `monitor de arquivos` (`settings.advanced.fileWatcherDebounce.label`),
  `comprometida` (`servers.refusal.hostKeyRevoked`), `painel` for a provider's dashboard
  (`settings.askCmdr.spend.disclaimer`), `alternador de apps` (`shortcuts.system.appSwitcher`), `Consulta` for the query
  criterion (`queryUi.results.criteria.query`).

## Phrasing

- **`uma chave` sem `SSH`** (`servers.paneState.signedOutNothingToAsk`): in the same pane `chave` is the HOST key
  (`servers.paneState.hostKeyChangedHint`); here it is the client's. English has the same ambiguity and the two states
  never show together, so the value stays literal. Confirm, or write `uma chave SSH`.
- **`Fixar no seletor`** (`menu.network.pinToSwitcher`): the full published term is `seletor de volumes`; the short form
  follows the English `switcher`. Confirm, or accept a five-word `Fixar no seletor de volumes`.
- **`Não precisa`** for "No, thanks" (`main.dockPinNudge.*`): neutral and natural, but the least literal choice. The
  literal `Não, obrigado` agrees with the speaker's gender.
- **The online-only delete warnings** (`fileOperations.delete.cloudOnlineOnlyMixedWarning`,
  `.cloudOnlineOnlyAllWarning`, `.cloudOnlineOnlyHandedBack`): drafted without human review; also check overflow, since
  the warning is long and sits in a narrow band above the file list.
- **`deixou tudo onde está`** (`fileOperations.leftovers.stagingFolderKept`): reads a little colder than the English
  "left them in place"; no pt-BR phrasing found that sounds as deliberate without adding words.
- **Unsourced but natural phrasings**: `Precisa de resposta` (`queue.row.statusAwaitingAnswer`), `fica como lembrete`
  (`fileExplorer.navigation.driveIndex.tooltipStalePhone`), `então nada se perdeu`
  (`errors.write.deviceDisconnected.sided.*`), `Seus originais não saíram do lugar`
  (`errors.write.moveNotConfirmed.suggestion`), `Editar arquivos com` (`settings.behavior.textEditorApp.label`),
  `configurações do servidor` for a NAS's own settings (`errors.mount.*`), `distribuição` for a Linux distribution
  (`errors.mount.gvfsMissing`).
