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
- `askCmdr.renameUndo.skipReason.*.counted` and `fileOperations.cancelRollback.reason.*.counted` keep "alone: they …"
  outside the plural, so sv's `one` branch reads `1 objekt … som de är`. Move the tail into both branches. (sv)
- sv's `ai-lookup` ruling is `sökning`, the search word, which is the mix-up the `privacy-permission` / `ai-lookup` note
  in `concepts.json` warns about. (sv)
- No `maker` concept: `commands.feedbackSend.description` ("the maker of Cmdr") came out masculine in de (`Macher`) and
  fr (`créateur`); add a concept with a neutral-wording note, or reword around it. (de, fr)
