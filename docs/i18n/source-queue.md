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
- `check` concept (verify) matches the Operation log's CHECKBOX sense ("Check or uncheck the item", "Check every item",
  "the checked items", "Check at least one item"), so es carries five exceptions. Add a `checkbox` concept (check /
  uncheck / checked item) or a `notMatch` for these phrases. Same for `choose`: "picks" in
  `operationLog.tree.togglesCapped` / `rollback.refusalTooManyToggles` means checkbox ticks. The `remove` / `original`
  concepts also match `operationLog.tree.statusRemoved`, `skipReason.locked`, and `skipReason.restoreTargetOccupied`,
  which their own descriptions exclude. (cs, de, es, fr, hu, nl, pt, ru, sv, uk, vi, zh, zh-Hant)
- The Operation log mixes "checked" (commands, `refusalNothingSelected`) and "selected" (`rollBackSelected`,
  `selectedCount`, `titleSelected`, `rollbackableNotice*`) for the same checkbox state (and "pick" in the toggle-cap
  strings); pick one. (cs, de, es, fr, hu, nl, pt, sv, uk)
- `operationLog.dialog.selectedCount` drives its plural with the TOTAL, but "selected" agrees with the checked count
  (`3 sur 12 sélectionnés`), so fr falls back to a colon form. Pass the checked count as the plural driver. Likewise
  `operationLog.tree.backfillProgress` has no plural driver at all for "Read {doneText} … items". (cs, es, fr, pt, sv)
- `folderRollup` in `operation-tree-labels.ts` joins `operationLog.tree.rollupRolledBack` and `rollupSkipped` with a
  hardcoded ASCII `', '`, so Chinese renders `3 个已回滚, 2 个已跳过` where it wants `，`. Make the joiner a catalog key
  or use `Intl.ListFormat`. (zh, zh-Hant)
- `fileOperations.rollbackConfirm.titleRest` ("Roll back the rest?") and the `rollBackRest` buttons differ only by the
  "?", so `i18n-terms` treats them as one string and forbids a question-shaped title where a style guide asks for one
  (hu: title `Visszagörgeted a többit?`, buttons `A többi visszagörgetése`; it's the one warn left). Either the check
  should ignore a trailing "?" split, or hu needs an allowlist entry. The `titleRest` description should name the
  constraint either way. (hu, sv, zh, zh-Hant)
