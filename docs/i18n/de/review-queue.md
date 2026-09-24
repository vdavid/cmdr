# de review queue

Open questions for a future native German reviewer (or David). Not translator input: every item below already ships a
reasoned value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the
settled wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Product-voice calls

- **`Systemintegritätsschutz` vs Apple's on-screen `System-Integrationsschutz`** (`errors.mutation.sipProtected`):
  shipping Apple's German documentation name. Apple's Finder shows the other word in exactly one string (`ET6`, emptying
  the Trash), and it's a visible mistranslation (Integration for Integrität). Principle 1 says match what the user sees
  in Finder, which here would mean shipping Apple's typo; the recommendation is to keep ours, since both share the head
  `System…schutz` and Apple's German support page (support.apple.com/de-de/102149) uses our form. Evidence:
  `decisions.md` § Umbenennen und Anlegen.
- **`…, ist aber weitergelaufen` instead of `und weitergelaufen`** (`crashReporter.dialog.body.keptRunning`): German
  usually wants an adversative particle there and the `aber` form reads warmer; the plain one mirrors the English. A
  one-line swap.
- **„AI suggestions are waiting.“ → „KI-Vorschläge warten auf dich.“** (`suggestedOps.indicatorTooltip`): coined; the
  alternative „Es liegen KI-Vorschläge bereit.“ is more matter-of-fact and less of a nudge.
- **`Klicken, um in den Einstellungen einen einzurichten.`** (`askCmdr.wake.needsApiKey`): the bare pronoun `einen` for
  the provider is correct but bald at the end of a tooltip; the alternative repeats `einen Anbieter`.
- **`Gib Cmdr auf AlternativeTo ein Like`** (`onboarding.stepBeta.checklist.alternativeTo`): Duden has `das Like`, and
  it keeps the parallel with `Vergib dem Repo auf GitHub einen Stern`, but it reads young.
- **Full disk access in running prose**: the badge and every place that NAMES the pane say `Festplattenvollzugriff`; the
  FDA step's prose (`onboarding.stepFda.*`, `onboarding.stepAi.bannerBody.*`) still says
  `vollständiger Festplattenzugriff`. Confirm the split, or sweep the prose onto the pane name.

## Terms

- **listing → `Dateiliste`** (tentative): no canonical source; confirm against plain `Liste`.
- **The stall wording** (`fileOperations.transferProgress.stallNotice`, `.stallInFlight`): no source names a stalled
  transfer, so `Kein Fortschritt seit {duration}` and `Die Übertragung kommt nicht mehr voran.` are constructions.
- **`Autor`** (`ai.cloudConsent.askCmdr.contentsRule`): Apple's Preview writes `Autor:in`, which the gender-glyph ban
  rules out; the neutral rewrites („wer es verfasst hat“, „Verfasserangabe“) read stilted inside the list.
- **`Agent`** standalone (`operationLog.initiator.agent`/`.agentEdited`): the usual loanword, but slightly ambiguous on
  its own; **`KI-Client`** (`operationLog.initiator.aiClient`) is tentative on the loanword.
- **`Statussymbol`** for a file row's status badge (`fileExplorer.imageIndex.*`): runner-up `Statuskennzeichen`.
- **`Schlüssel-Passphrase`** (`servers.sheet.passphrase`): macOS de has no `Passphrase` (it says `Passwort` everywhere),
  which would clash with the Passwort field right above.
- **`Werkzeug`** for an AI tool (`askCmdr.tool.*`), **`Archivierung aufheben`** (unarchive a chat), **`Kameraangaben`**
  and **`Aufnahmeort`** (`ai.cloudConsent.askCmdr.*`), **`Relevanz`** (`fileExplorer.columns.sortByRelevance`),
  **`Seitenbereich`** (side panel), **`Optionen zum Teilen`** (`menu.context.shareLoading`/`.shareNone`),
  **`Mailingliste`**, **`Folgen`** / **`Folgemodus`** (viewer tail), **`Kompakt`** / **`Voll`** (view modes), **`ORD`**
  (the tight DIR status-bar slot), **`Handle`**: all tentative, each with its reasoning in `terms.json`.
- **`Vertrauenswürdig seit <Datum>`**, **`Gefunden unter {path}`**, **`Cmdr achtet auf Telefone.`**
  (`settings.servers.*`, `settings.adb.*`): constructions with no Apple precedent.
- **`Weiterarbeiten`** (`main.quit.keepWorking`) and **`In {destination} wird gearbeitet`**
  (`fileOperations.operationConflict.context`): constructed, no pile source.
- **`auf deinen Wunsch abgebrochen`** (`errors.volume.cancelled`) and **`aus den Augen verloren`**
  (`errors.mutation.*`): standard German, unattested in the pile.
- **`mit einer Zifferntaste`** (`commands.favoritesOpen.description`): neither macOS nor Microsoft has the word.
- **`Entf` for the Delete key** (`fileExplorer.navigation.favoriteShortcutAriaLabel`, `.favoriteShortcutPrompt`): `Entf`
  is the Windows forward-delete label; a Mac keyboard shows ⌫ and Apple's German docs say `Rückschritttaste`. Confirm,
  or switch to the Mac key name.
- **`Distribution`** (Linux, `errors.mount.gvfsMissing`): no source in the Linux sense.

## Overflow checks

German runs 20–35% longer than English. Look at these against the pseudolocale (`en-XA`) and real screenshots:

- `crashReporter.dialog.body.keptRunning` (about 160 characters vs 140, in a dialog that also carries the privacy note
  and the report ID).
- `errors.eject.unmountRefused` (about 125 vs 84), `errors.eject.deviceDisconnectRefused` (about 110), and
  `errors.eject.unmountRefusedByCmdr` (about 155 vs 110, the longest of the family): all land behind a frame sentence
  that is already wide with a long `{volumeName}`.
- `fileOperations.trash.goToTrashAction` / `commands.fileGoToTrash.label` (`Zum Papierkorb gehen`, 20 vs 11) in a narrow
  toast beside `Widerrufen`.
- `errorReporter.amend.submit` (`Zum Bericht hinzufügen`, 22 vs 13) beside `Abbrechen`, and
  `errorReporter.autoSentToast.viewOrAddNotes` (37 vs 31) beside `Einstellungen ändern`.
- `fileOperations.delete.cloudOnlineOnlyMixedWarning` / `.cloudOnlineOnlyAllWarning`: a long banner in a narrow strip.
- `fileOperations.transferProgress.stallNotice` on the queue row (`Kein Fortschritt seit 2 Min. 30 s` vs
  `noch 2 Min. 30 s`).
- `settings.indexing.overriddenBadge` (`Mit der Laufwerksindizierung aus`) and `menu.network.pinToSwitcher`
  (`In der Volume-Auswahl fixieren`), both much longer than the English.
