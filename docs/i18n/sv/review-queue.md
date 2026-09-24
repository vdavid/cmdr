# sv review queue

Open questions for a future native Swedish reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **Brief / Full view → `Kortfattad` / `Fullständig`**: Cmdr-coined view-mode names; confirm they read as view modes.
- **app bundle → `Appaket`**: the compound drops one p (app + paket); confirm it reads cleanly against `Programpaket`.
- **command palette → `kommandopalett`**, **onboarding → `introduktion`** (menu `Introduktion…`, wizard
  `introduktionsguiden`, title `Kom igång med Cmdr`): no macOS source for either.
- **Character Viewer → `Teckenvisare`**, **Force Quit → `Avsluta tvingat`**, **input source switching → `byte av
  inmatningskälla`**: Apple-style Swedish that isn't in the pile; check against a live Swedish Mac.
- **modifier key → `modifierare`**: MS's `låstangent` is the lock-key sense, so the word is composed.
- **Apple silicon** kept English: Apple's Swedish marketing says `Apple-kisel`; confirm which a Swedish Mac user expects.
- **token plural `tokens`** (`askCmdr.cost.tokens`): identical to English, from Swedish tech press, no pile source.
- **unarchive → `avarkivera`**: composed by the av- reversal pattern (`avmontera`); no direct hit.
- **source-available → `källtillgänglig`**, **rate-limit → `hastighetsbegränsa`**, **importance → `vikt`**: composed.
- **the rename-cycle badge `(cykel)`**: correct term for a cyclic dependency; confirm a user doesn't read "bicycle".
- **the skipped outcome chip `Överhoppad`** vs `Hoppade över`.
- **`Kräver enhetsindexering`** for the "Off with drive indexing" badge (`settings.indexing.overriddenBadge`): reframes a
  state as a requirement to avoid the imperative `Av med …`. The literal alternative is `Av tillsammans med
  indexeringen`.
- **start folder → `startmapp`**, **key file → `Nyckelfil`**, **(SSH) host key → `värdnyckel`**, **compromised →
  `komprometterad`**: composed, no first-party source.
- **text editor → `textredigerare`**: shares the catalog's `redigerare` stem; MS says `textredigeringsprogram`.
- **Android `platform tools` kept English**: no AOSP or Google Swedish string either way.
- **share options → `delningsalternativ`**: composed from `Dela` + `Alternativ`.
- **merge → `slå samman`** over Apple/GNOME's `sammanfoga`, chosen for a plainer voice.
- **redact → `maskera` / `rensa bort`**, **streaming → `strömma` / `strömningsläge`**, **western (encoding group) →
  `västerländsk`**: convention, no UI source.
- **look inside → `titta in i`** (`askCmdr.tool.inspectFile.*`): sibling-driven, no direct source.
- **mailing list → `e-postlista`**, **notice (of people and search engines) → `upptäcka`**, **work in progress →
  `under arbete`**: no first-party source.
- **viewer → `förhandsvisning`**: macOS uses `granskare` for the inspector; that's the fallback if the viewer ever
  becomes a distinct inspector surface.

## Phrasing and tone

- **`Inget har hänt på {duration}`** (the stall notice) and **`står stilla`**: composed; macOS has no stall wording.
- **`tills du svarar`** (`fileOperations.operationConflict.pausedNote`): composed.
- **`, liksom …`** for "and so did …" (`fileExplorer.rename.chainKeptOriginalNameAndOthers`): composed.
- **`förra gången`** in the crash dialog's three variants: Apple writes `När du senast …`; if `.ended` is ever reworded,
  `När Cmdr senast kördes …` is the attested alternative for all three at once.
- **`I bakgrunden`** on the empty-queue button: `Kör i bakgrunden` is the fuller alternative if it reads too elliptical.
- **`Hur?`** (the link that opens Android's own instructions, `adb.hint.how`): no one-word Apple form for "How".
- **The online-only delete warnings** (`fileOperations.delete.cloudOnlineOnly*`): long; confirm the four facts read
  clearly and calmly.

## Layout (overflow-check against the pseudolocale)

- **`Gå till papperskorgen`** (21 characters against 11) beside `Ångra` on the trash toast.
- **`Lägg till i rapporten`** (21 against 13) beside `Stäng` in the amend dialog.
- **`Visa rapporten eller lägg till en notering`** (42 against 31) in the auto-sent toast.
- **The online-only warning box**, which sits in a narrow strip above the file list.
