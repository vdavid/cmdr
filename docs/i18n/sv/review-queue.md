# sv review queue

Open questions for a future native Swedish reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Terms

- **Brief / Full view → `Kortfattad` / `Fullständig`**: Cmdr-coined view-mode names; confirm they read as view modes.
- **app bundle → `Appaket`**: the compound drops one p (app + paket); confirm it reads cleanly against `Programpaket`.
- **command palette → `kommandopalett`**, **onboarding → `introduktion`** (menu `Introduktion…`, wizard
  `introduktionsguiden`, title `Kom igång med Cmdr`): no macOS source for either.
- **Character Viewer → `Teckenvisare`**, **Force Quit → `Avsluta tvingat`**, **input source switching →
  `byte av inmatningskälla`**: Apple-style Swedish that isn't in the pile; check against a live Swedish Mac.
- **modifier key → `modifierare`**: MS's `låstangent` is the lock-key sense, so the word is composed.
- **Apple silicon** kept English: Apple's Swedish marketing says `Apple-kisel`; confirm which a Swedish Mac user
  expects.
- **token plural `tokens`** (`askCmdr.cost.tokens`): identical to English, from Swedish tech press, no pile source.
- **unarchive → `avarkivera`**: composed by the av- reversal pattern (`avmontera`); no direct hit.
- **source-available → `källtillgänglig`**, **rate-limit → `hastighetsbegränsa`**, **importance → `vikt`**: composed.
- **the rename-cycle badge `(cykel)`**: correct term for a cyclic dependency; confirm a user doesn't read "bicycle".
- **the skipped outcome chip `Överhoppad`** vs `Hoppade över`.
- **`Kräver enhetsindexering`** for the "Off with drive indexing" badge (`settings.indexing.overriddenBadge`): reframes
  a state as a requirement to avoid the imperative `Av med …`. The literal alternative is
  `Av tillsammans med indexeringen`.
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
- **S3 bucket → `bucket`, en-word, `bucketen` / `bucketar` / `bucketarna`** (`servers.*`, `commands.handler.*`): MS
  keeps `bucket`; the plural is Swedish tech usage, no first-party source. No shared concept exists yet, so it has no
  `terms.json` ruling.
- **share link → `delningslänk`** (`commands.fileCopyShareLink*`, `menu.context.*ShareLink*`): composed from `dela` +
  `länk`; MS has `delningsbar länk` / `Alla-länk` for OneDrive's anonymous link.
- **secret access key → `hemlig åtkomstnyckel`**, **access key ID → `Åtkomstnyckel-ID`**: MS `hemlig nyckel` +
  `åtkomstnyckel`; AWS has no Swedish console to quote.
- **cold storage → `kall lagring`** (MS), **archived (cold-storage tier) → `arkiverad`**, **restore → `återställa`**:
  check the pair reads as a storage tier and not as a zip archive.
- **path-style addressing → `sökvägsbaserad adressering (path-style)`** and **Google's Interoperability tab →
  `fliken Interoperabilitet`** (`servers.sheet.s3GcsKeyHelp`): unverified against Google Cloud's Swedish console.

## Phrasing and tone

- **`Inget har hänt på {duration}`** (the stall notice) and **`står stilla`**: composed; macOS has no stall wording.
- **`tills du svarar`** (`fileOperations.operationConflict.pausedNote`): composed.
- **`, liksom …`** for "and so did …" (`fileExplorer.rename.chainKeptOriginalNameAndOthers`): composed.
- **`Mapp som inte kan läsas`** (`shortcuts.scope.errorScreen`, a section title): composed; `Oläsbar mapp` is shorter
  but `oläslig` / `oläsbar` lean toward illegible handwriting.
- **`förra gången`** in the crash dialog's three variants: Apple writes `När du senast …`; if `.ended` is ever reworded,
  `När Cmdr senast kördes …` is the attested alternative for all three at once.
- **`I bakgrunden`** on the empty-queue button: `Kör i bakgrunden` is the fuller alternative if it reads too elliptical.
- **`Hur?`** (the link that opens Android's own instructions, `adb.hint.how`): no one-word Apple form for "How".
- **The online-only delete warnings** (`fileOperations.delete.cloudOnlineOnly*`): long; confirm the four facts read
  clearly and calmly.
- **`Använd gäståtkomst`** (`fileExplorer.network.share.useGuest`): `Byt till gäst` is shorter if it reads stiff.
- **`Välj en server för att redigera den.`** (`servers.hub.editPickHint`): the English means moving the cursor to a
  server row; confirm `Välj` doesn't suggest a picker.
- **`Bara manuell kontroll`** (`settings.managed.summary.manualChecksOnly`, `.upToManualChecksOnly`): updates checked by
  hand only; confirm it reads as update checks, or whether `Bara när du söker själv` is clearer.
- **`din organisation håller den här Macen på {ceiling} eller tidigare`** (`updates.status.heldByPolicy`): confirm
  `håller … på` reads naturally for a version cap.
- **`Ångrade ”…” från {time}` / `Ångrades senast av ”…” från {time}`** (`operationLog.dialog.rollbackOf`,
  `.latestRollback`): verbs over the stilted noun `ångring`; confirm the past tense reads right on a row whose rollback
  is still running, and that `från {time}` reads as dated.

- **`Varje Ord Med Versal`** (`multiRename.case.words`): title-case demo, which Swedish never writes; TC's
  `Versal Först I Varje Ord` is the attested alternative. Confirm it reads as an option, not a typo.
- **`Räkna från`** / **`Steg`** / **`Antal siffror`** (`multiRename.counterStart`/`.counterStep`/`.counterDigits`): TC
  says `Börja med:`, `Steglängd:`, `Antal siffror:`; confirm `Räkna från` reads as the counter's start value.

## Layout (overflow-check against the pseudolocale)

- **`Gå till papperskorgen`** (21 characters against 11) beside `Ångra` on the trash toast.
- **`Lägg till i rapporten`** (21 against 13) beside `Stäng` in the amend dialog.
- **`Visa rapporten eller lägg till en notering`** (42 against 31) in the auto-sent toast.
- **`Lossa från volymväljaren`** (24 characters against 19) in the narrow volume dropdown (`menu.network.unpin`).
- **The online-only warning box**, which sits in a narrow strip above the file list.
