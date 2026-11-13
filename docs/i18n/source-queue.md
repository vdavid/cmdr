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
- `look-inside` is defined as the AI reading a file, yet `search.coverage.line.denied` / `.declined` use it for a walk
  entering folders: widen or exclude. (cs, ru)
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
- `shortcuts.scope.searchDialog`: "use the same words as those dialogs' titles" can't hold where one title is a noun
  (Search) and the other a verb phrase (Select files); say whether a combined noun heading is fine. (cs)
- `settings.fileExplorer.typeToJump.mode.description`: the `other` branch names the PC's Backspace key, but no concept
  covers it (`delete-key` is the Mac ⌫ key), so each locale records its PC key name ad hoc (es put `Retroceso` in
  `delete-key`'s `other` form). Propose a `backspace-key` concept. (es)
- `shortcuts.system.windowMenu`: the note asks for "the name Windows gives that menu", but Windows' UI never names the
  Alt+Space menu (the Microsoft TBX has only the developer term "system menu" → `menú de sistema`); say whether a
  descriptive phrase ("the window's menu") is what's wanted. (es)
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
- `commands.viewToggleFullScreen.label` is a plain value on every OS, but the OSes split on the term (macOS
  `schermvullende weergave`, Windows and the Linux desktops `volledig scherm` in nl), so nl kept the macOS term; it
  needs a per-OS value like `menu.view.fullScreen`'s note implies. (nl)

## From pt (keymap batch, 2026-11-12)

- **`commands.searchModeByNumber.label` `@key`** could say whether the label must stay short (it's a description, not a
  command name); pt wrote a full sentence.
- `shortcuts.scope.searchDialog` asks for both dialog titles' words, but in ru they are a noun and an infinitive
  (`Поиск`, `Выделить файлы`), which don't join into a heading; ru wrote `Поиск и выделение файлов`. Say whether a
  heading may paraphrase titles. (ru)
- `shortcuts.system.securityScreen`: like `windowMenu`, the note's target has no Windows UI name (Windows never titles
  the Ctrl+Alt+Delete screen); say whether a descriptive phrase is what's wanted. sv wrote `säkerhetsskärmen`
  (tentative). (sv)
- `shortcuts.scope.searchDialog`: uk titles are a noun and an infinitive (`Пошук`, `Вибрати файли`), so uk wrote
  `Діалоги «Пошук» і «Вибрати файли»` to quote both exactly; same open question as ru on whether a heading may
  paraphrase. `shortcuts.system.inputMethodToggle`: say whether "input method" means Windows' IME (MS:
  `редактор засобів вводу`) or the general idea; uk wrote `методу введення` (tentative). (uk)
- `recent` matches "recently closed tabs" (`settings.fileExplorer.tabs.closedTabHistorySize.description*`), which vi
  renders as "just closed" (`vừa đóng`), as its `commands.handler.noRecentlyClosedTabs` already does under an exception.
  Only vi deviates, and the concept's sense names recently closed tabs, so it stays a vi exception unless another locale
  does the same. (vi)
- `commands.viewToggleFullScreen.label` is a plain value shown on every OS, but zh macOS says 全屏幕 and
  Windows/browsers 全屏; consider a per-OS value. (zh)
- `shortcuts.system.windowMenu` / `securityScreen`: same open question as es/sv (Windows never names them); zh-Hant
  wrote tentative descriptive forms. (zh-Hant)
