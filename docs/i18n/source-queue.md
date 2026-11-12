# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.

## Open

- Screenshots: `mtp-browse.png` doesn't show the `mtp.readiness.connecting` row, and the AI suggestions dialog keys ship
  without any (the pills and fact lines need them most). Needs a `pnpm i18n:shots` run with the Mac left alone. (all)
- Counted nouns in the RAW `errors.*` family can't take an ICU plural, so Slavic locales still need a colon form:
  `errors.write.trashRefused.message.*`, `errors.write.deviceDisconnected.sided.*`,
  `errors.eject.unmountRefusedByProcesses`. Either word around the count, or give the raw path a plural. (uk)
- `archive` concept: the zip-noun keys (`fileExplorer.list.rowKind.archive`, `suggestedOps.facts.archive`) now leave
  `archive-chat`, but `archive`'s own `notMatch` drops any value that's exactly "Archive", so they match neither. Needs
  a key-scoped `match`. (all)
- `menu.window.zoom` and `menu.view.zoom` are both "Zoom" in English, so cs, fr, and uk keep a `zoom` exception; split
  the English or accept it. (uk)
- `settings.fileOperations.macOnlyFiles.description`: "when copying/moving/compressing files" uses slashes and says
  "files" twice; "when copying, moving, or compressing" reads cleaner, and cs, de, es, fr, hu, nl, pt, ru, and sv
  already render it that way. (cs, de, es, fr, hu, nl, pt, ru, sv)
- The `remove` / `original` concepts match `operationLog.tree.statusRemoved`, `skipReason.locked`, and
  `skipReason.restoreTargetOccupied`, which their own descriptions exclude. (cs, de, es, fr, hu, nl, pt, ru, sv, uk, vi,
  zh, zh-Hant)
- `operationLog.dialog.selectedCount` drives its plural with the TOTAL, but "selected" agrees with the checked count
  (`3 sur 12 sélectionnés`), so fr falls back to a colon form. Pass the checked count as the plural driver. Likewise
  `operationLog.tree.backfillProgress` has no plural driver at all for "Read {doneText} … items". (cs, es, fr, pt, sv)
- `search.coverage.line.uncovered.*`: `{drive}` is either a name or the generic phrase from
  `search.coverage.unnamedDrive`, so case-marking languages read stiffly ("ezt: ez a meghajtó"), and in `.unavailable`
  it actually holds a SERVER name. Give the generic case its own variant key, or a server param of its own. (hu, ru)
- `search.coverage.line.uncovered.indexing`: the English singular "hasn't reached one folder yet, so this search skipped
  it" reads oddly; "hasn't reached this folder yet" is clearer. (sv, fr)
- `close` concept matches "keeps … closed to Cmdr" (access, not closing) in `search.coverage.line.offLimits`; add it to
  `notMatch` in `concepts.json` (cs and ru carry exceptions meanwhile). `look-inside` is defined as the AI reading a
  file, yet `search.coverage.line.denied` / `.declined` use it for a walk entering folders: widen or exclude. (cs, ru)
- "system" (the OS, `search.coverage.line.offLimits`) has no concept yet, and every locale coined its own word; add one
  sourced from Apple's localized macOS. (sv, zh, zh-Hant, uk)
- vi: `search.coverage.deniedFullDiskAccess` and `search.coverage.setUpFullDiskAccess` say `quyền truy cập toàn bộ đĩa`,
  but Apple's pane is `Quyền truy cập đầy đủ vào ổ đĩa` (vi `style.md`). Re-translate both. (vi)
- "Couldn't X" sentences naming Cmdr as the subject (`search.coverage.line.failedRoots`) collide with a style guide's
  impersonal pattern ("X ließ sich nicht öffnen") because `dont-translate` demands "Cmdr". Decide whether an actor-only
  "Cmdr" may drop in translation. (de)
- `pnpm i18n:brief`'s regional-overlay section names vocabulary forks only; `es-419` also forks the compound perfect to
  the preterite ("no pudo"), which cost a failed check. Name tense forks there. (es)
- Briefs for brand-new keys: run `sync-locale-keys.ts` before `pnpm i18n:brief`, or the brief describes skeletons the
  catalog doesn't hold yet. (cs, de, es, fr, nl, pt, sv)
- `errors.write.trashRefused.message.ownedByAnotherUser`: "their folder" can read as the other ACCOUNT's folder rather
  than the folder holding the items; "the folder holding them" (as in `message.folderNotWritable`) is clearer. (all)
- `fileOperations.validation.pathShareMissing`: "keep … the words "server" and "share" as placeholders a person
  recognizes" reads two ways; translators localized them (Windows' own `\\Server\Freigabe` style), CJK and vi kept
  English. Say which. (all)
- `ask` concept matched "Your settings ask for" (`settings.ai.provider.localUnavailable`), the request sense; added
  `settings ask for` to its `notMatch`. A broader "ask for" exclusion needs a call, since prose "asks for your password"
  sits in the concept. (all)
- Termbase false positives from the keymap batch, worked around with cs `exceptions`: `move` needs `moving to a new pc`
  and `moving to a new computer` in `notMatch` (`errors.write.trashRefused.suggestion.trashNotWritable*`); `window`
  matches the OS name "Windows" (`shortcuts.conflict.takenByWindows`,
  `errors.write.homeFolderRename.suggestion.isHome*`), so it wants a case-sensitive or `Windows` exclusion; `desktop`
  matches a Linux desktop environment (`shortcuts.conflict.takenByLinuxDesktops`): add `linux desktops`,
  `the desktop usually takes`, `desktop’s keyboard settings`; `unix-group` fires on the macOS pane "Users & Groups"
  (`errors.write.homeFolderRename.suggestion.isHome*`): add `users & groups`. (cs)
- `shortcuts.scope.searchDialog`: "use the same words as those dialogs' titles" can't hold where one title is a noun
  (Search) and the other a verb phrase (Select files); say whether a combined noun heading is fine. (cs)
- The `window` concept's `match: ["windows"]` hits the OS name "Windows" (`shortcuts.conflict.takenByWindows`,
  `errors.write.homeFolderRename.suggestion.isHome*` windows branch); de added key exceptions. Make the matcher skip
  capitalized "Windows", or add a key-scoped `notMatch`. (de)
- The `desktop` concept (the macOS Desktop folder) matches the Linux desktop-environment sense in
  `shortcuts.conflict.takenByLinuxDesktops` ("the desktop usually takes", "your desktop’s keyboard settings"); de added
  an exception. A key-scoped `notMatch` fixes it for every language. (de)
- The `move` concept matches "moving to a new Mac/PC/computer" (switching machines) in
  `errors.write.trashRefused.suggestion.trashNotWritable*`; de added exceptions. A `notMatch` for "moving to a new"
  fixes it for every language. (de)
- The `move` (moving to a new PC/computer) and `window` (OS name "Windows") false positives above hit es too; es added
  the same key `exceptions`. (es)
- `settings.fileExplorer.typeToJump.mode.description`: the `other` branch names the PC's Backspace key, but no concept
  covers it (`delete-key` is the Mac ⌫ key), so each locale records its PC key name ad hoc (es put `Retroceso` in
  `delete-key`'s `other` form). Propose a `backspace-key` concept. (es)
- `shortcuts.system.windowMenu`: the note asks for "the name Windows gives that menu", but Windows' UI never names the
  Alt+Space menu (the Microsoft TBX has only the developer term "system menu" → `menú de sistema`); say whether a
  descriptive phrase ("the window's menu") is what's wanted. (es)
- The `move`/`window` false positives above hit hu too (key `exceptions` added); also the `select` concept matches the
  Windows-branch "select “Never show this warning again”" (clicking a control, Microsoft's verb) in
  `settings.fileViewer.suppressBinaryWarning.explanation`: a `notMatch` for "select “" would fix it everywhere. (hu)
- `fileExplorer.quickLookHint.quickView` vs `quickViewRebound`: the brief asks the two to read alike, but with a
  rebindable key in `<key></key>` an article or case ending before the chip can't be known in languages like hu; the
  rebound variant took a colon slot, so the pair now differs. Say whether `quickView` (fixed ⇧Space) should follow. (hu)
- `viewer.binaryWarning.body*`: "the actual" sits outside `<kindName>`, so a gendered language must repeat
  `{category, select, …}` outside the tag to agree the article and adjective with the noun inside it (fr:
  `{category, select, image {la véritable} other {le véritable}} <kindName>…</kindName>`). Moving "the actual" into each
  branch inside the tag would let every language agree in one place. (fr)
- `shortcuts.conflict.takenByWindows` and `shortcuts.conflict.systemShortcut` end "so it may never reach Cmdr", where
  "it" points back at `{combo}`; gendered languages either guess a pronoun for the insert or rephrase with a noun ("ce
  raccourci"). Consider "so the shortcut may never reach Cmdr" in English. (fr)
- The `move`/`window`/`desktop` false positives above hit nl too (key `exceptions` added); also `edit`'s `notMatch` has
  `(⌫) edits it` but not the `other` branch's `backspace edits it`
  (`settings.fileExplorer.typeToJump.mode.description`): add it. (nl)
- `commands.viewToggleFullScreen.label` is a plain value on every OS, but the OSes split on the term (macOS
  `schermvullende weergave`, Windows and the Linux desktops `volledig scherm` in nl), so nl kept the macOS term; it
  needs a per-OS value like `menu.view.fullScreen`'s note implies. (nl)

## From pt (keymap batch, 2026-11-12)

- **Concept matcher: `window` matches the OS name "Windows"** (`errors.write.homeFolderRename.suggestion.isHome*`,
  `shortcuts.conflict.takenByWindows`): every locale's termbase check flags a Windows-branch sentence as `window` drift.
  Proposal: add "Windows" (the OS) to `window`'s `notMatch` in `concepts.json`. pt worked around it with `exceptions`.
- **Concept matcher: `move` matches "moving to a new Mac/PC/computer"**
  (`errors.write.trashRefused.suggestion.trashNotWritable*`), a sense the concept itself already excludes in prose.
  Proposal: a `notMatch` for "moving to a new".
- **Concept matcher: `desktop` (the macOS Desktop folder) and `settings` (Cmdr's Settings) catch the Linux desktop
  environment and its keyboard settings** in `shortcuts.conflict.takenByLinuxDesktops`. Proposal: a new
  `desktop-environment` concept (or `notMatch` entries), since every locale needs a different word there.
- **`commands.searchModeByNumber.label` `@key`** could say whether the label must stay short (it's a description, not a
  command name); pt wrote a full sentence.
- The `move`/`window`/`desktop`/`select` false positives and the missing `backspace edits it` `notMatch` above hit ru
  too (key `exceptions` added; ru also added `окон` to `window`'s accept, since the genitive plural doesn't contain the
  stem `окн`). `shortcuts.scope.searchDialog` asks for both dialog titles' words, but in ru they are a noun and an
  infinitive (`Поиск`, `Выделить файлы`), which don't join into a heading; ru wrote `Поиск и выделение файлов`. Say
  whether a heading may paraphrase titles. (ru)
- The `move`/`window`/`desktop` false positives and the missing `backspace edits it` `notMatch` above hit sv too (key
  `exceptions` added, marked "pending a shared notMatch"). New one: `toggle` matches the noun in
  `settings.developer.verboseLogging.description` ("unaffected by this toggle", the Settings switch itself); a
  `notMatch` for "this toggle" would fix it everywhere (sv added an `exceptions` entry). (sv)
- `shortcuts.system.securityScreen`: like `windowMenu`, the note's target has no Windows UI name (Windows never titles
  the Ctrl+Alt+Delete screen); say whether a descriptive phrase is what's wanted. sv wrote `säkerhetsskärmen`
  (tentative). (sv)
- The `move`/`window`/`desktop` false positives above hit uk too (key `exceptions` added).
  `shortcuts.scope.searchDialog`: uk titles are a noun and an infinitive (`Пошук`, `Вибрати файли`), so uk wrote
  `Діалоги «Пошук» і «Вибрати файли»` to quote both exactly; same open question as ru on whether a heading may
  paraphrase. `shortcuts.system.inputMethodToggle`: say whether "input method" means Windows' IME (MS:
  `редактор засобів вводу`) or the general idea; uk wrote `методу введення` (tentative). (uk)
- The `window` (OS name "Windows") false positive above hits vi too (key `exceptions` added). New one: `recent` matches
  "recently closed tabs" (`settings.fileExplorer.tabs.closedTabHistorySize.description*`), which vi (and likely others)
  render as "just closed" (`vừa đóng`), as the shipped `commands.handler.noRecentlyClosedTabs` already does under an
  exception; a `notMatch` for "recently closed" would fix it everywhere. (vi)
- The `move` ("moving to a new PC/computer"), `window` (OS name "Windows"), and `toggle` ("this toggle", a noun in
  `settings.developer.verboseLogging.description`) false positives above hit zh too (key `exceptions` added; `开关`
  added to zh `toggle` accept). zh also needed an `edit` exception for "Backspace edits it"
  (`settings.fileExplorer.typeToJump.mode.description`), so a `notMatch` for "edits it" would help.
  `commands.viewToggleFullScreen.label` is a plain value shown on every OS, but zh macOS says 全屏幕 and
  Windows/browsers 全屏; consider a per-OS value. (zh)
- The `move` ("moving to a new PC/computer"), `window` (OS name "Windows"), and `menu` (Windows' own `功能表` in
  `shortcuts.system.startMenu` / `windowMenu`) false positives above hit zh-Hant too (key `exceptions` added).
  `shortcuts.system.windowMenu` / `securityScreen`: same open question as es/sv (Windows never names them); zh-Hant
  wrote tentative descriptive forms. (zh-Hant)
